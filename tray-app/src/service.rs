//! The Windows service: owns goodbyedpi.exe, switches modes on request from the tray,
//! and restarts goodbyedpi.exe if it crashes.

use std::collections::VecDeque;
use std::ffi::c_void;
use std::os::windows::io::AsRawHandle;
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::{Child, Command};
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicPtr, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use windows_sys::core::PWSTR;
use windows_sys::Win32::Foundation::{CloseHandle, ERROR_CALL_NOT_IMPLEMENTED, HANDLE, NO_ERROR};
use windows_sys::Win32::System::JobObjects::*;
use windows_sys::Win32::System::Services::*;
use windows_sys::Win32::System::Threading::CREATE_NO_WINDOW;

use crate::common::{self, wide, Health, Mode, SERVICE_NAME};

const BASE_ARGS: &[&str] = &[
    "-5", "--set-ttl", "5",
    "--dns-addr", "77.88.8.8", "--dns-port", "1253",
    "--dnsv6-addr", "2a02:6b8::feed:0ff", "--dnsv6-port", "1253",
];

/// Give up restarting after this many crashes inside `CRASH_WINDOW`.
const MAX_CRASHES: usize = 5;
const CRASH_WINDOW: Duration = Duration::from_secs(60);

enum Cmd {
    SetMode(Mode),
    Stop,
}

static CMD_TX: OnceLock<Sender<Cmd>> = OnceLock::new();
static STATUS_HANDLE: AtomicPtr<c_void> = AtomicPtr::new(null_mut());

pub fn run() {
    let name = wide(SERVICE_NAME);
    let table = [
        SERVICE_TABLE_ENTRYW { lpServiceName: name.as_ptr() as *mut u16, lpServiceProc: Some(service_main) },
        SERVICE_TABLE_ENTRYW { lpServiceName: null_mut(), lpServiceProc: None },
    ];
    unsafe { StartServiceCtrlDispatcherW(table.as_ptr()) };
}

fn set_status(state: u32) {
    let status = SERVICE_STATUS {
        dwServiceType: SERVICE_WIN32_OWN_PROCESS,
        dwCurrentState: state,
        dwControlsAccepted: if state == SERVICE_RUNNING { SERVICE_ACCEPT_STOP | SERVICE_ACCEPT_SHUTDOWN } else { 0 },
        dwWin32ExitCode: NO_ERROR,
        dwServiceSpecificExitCode: 0,
        dwCheckPoint: 0,
        dwWaitHint: 5000,
    };
    unsafe { SetServiceStatus(STATUS_HANDLE.load(Ordering::SeqCst), &status) };
}

fn send(cmd: Cmd) {
    if let Some(tx) = CMD_TX.get() {
        let _ = tx.send(cmd);
    }
}

unsafe extern "system" fn handler(control: u32, _event: u32, _data: *mut c_void, _ctx: *mut c_void) -> u32 {
    match control {
        SERVICE_CONTROL_STOP | SERVICE_CONTROL_SHUTDOWN => {
            set_status(SERVICE_STOP_PENDING);
            send(Cmd::Stop);
            NO_ERROR
        }
        SERVICE_CONTROL_INTERROGATE => NO_ERROR,
        code => match Mode::from_control_code(code) {
            Some(mode) => {
                send(Cmd::SetMode(mode));
                NO_ERROR
            }
            None => ERROR_CALL_NOT_IMPLEMENTED,
        },
    }
}

unsafe extern "system" fn service_main(_argc: u32, _argv: *mut PWSTR) {
    let (tx, rx) = mpsc::channel();
    let _ = CMD_TX.set(tx);

    let name = wide(SERVICE_NAME);
    let handle = RegisterServiceCtrlHandlerExW(name.as_ptr(), Some(handler), null_mut());
    if handle.is_null() {
        return;
    }
    STATUS_HANDLE.store(handle, Ordering::SeqCst);
    set_status(SERVICE_START_PENDING);

    let mut runner = Runner::new();
    runner.apply(common::load_mode());
    set_status(SERVICE_RUNNING);

    loop {
        match rx.recv_timeout(Duration::from_secs(1)) {
            Ok(Cmd::SetMode(mode)) => {
                common::save_mode(mode);
                runner.apply(mode);
            }
            Ok(Cmd::Stop) | Err(RecvTimeoutError::Disconnected) => break,
            Err(RecvTimeoutError::Timeout) => runner.check(),
        }
    }

    // The saved mode is kept, so the same mode comes back on next boot.
    runner.kill();
    set_status(SERVICE_STOPPED);
}

struct Runner {
    mode: Mode,
    health: Health,
    child: Option<Child>,
    crashes: VecDeque<Instant>,
    /// Kill-on-close job: if this service dies, Windows takes goodbyedpi.exe down with it.
    job: HANDLE,
    exe: PathBuf,
    blacklist: PathBuf,
}

impl Runner {
    fn new() -> Runner {
        let dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.to_path_buf()))
            .unwrap_or_default();

        let job = unsafe {
            let job = CreateJobObjectW(null(), null());
            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const c_void,
                std::mem::size_of_val(&info) as u32,
            );
            job
        };

        Runner {
            mode: Mode::Off,
            health: Health::Off,
            child: None,
            crashes: VecDeque::new(),
            job,
            exe: dir.join("x86_64").join("goodbyedpi.exe"),
            blacklist: dir.join("discord_domains.txt"),
        }
    }

    fn apply(&mut self, mode: Mode) {
        self.kill();
        self.mode = mode;
        self.crashes.clear();
        if mode == Mode::Off {
            self.health = Health::Off;
        } else {
            self.spawn();
        }
        common::write_state(self.mode, self.health);
    }

    fn spawn(&mut self) {
        let mut cmd = Command::new(&self.exe);
        cmd.args(BASE_ARGS).creation_flags(CREATE_NO_WINDOW);
        if let Some(dir) = self.exe.parent() {
            cmd.current_dir(dir);
        }
        if self.mode == Mode::DiscordOnly {
            cmd.arg("--blacklist").arg(&self.blacklist);
        }

        match cmd.spawn() {
            Ok(child) => {
                unsafe { AssignProcessToJobObject(self.job, child.as_raw_handle() as HANDLE) };
                self.child = Some(child);
                self.health = Health::Running;
            }
            Err(_) => self.health = Health::Error,
        }
    }

    /// Called every second: restart goodbyedpi.exe if it died unexpectedly.
    fn check(&mut self) {
        let exited = match self.child.as_mut() {
            Some(child) => !matches!(child.try_wait(), Ok(None)),
            None => false,
        };
        if !exited {
            return;
        }
        self.child = None;

        let now = Instant::now();
        self.crashes.push_back(now);
        while self.crashes.front().is_some_and(|t| now.duration_since(*t) > CRASH_WINDOW) {
            self.crashes.pop_front();
        }

        if self.crashes.len() >= MAX_CRASHES {
            self.health = Health::Error;
        } else {
            self.spawn();
        }
        common::write_state(self.mode, self.health);
    }

    fn kill(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl Drop for Runner {
    fn drop(&mut self) {
        self.kill();
        unsafe { CloseHandle(self.job) };
    }
}
