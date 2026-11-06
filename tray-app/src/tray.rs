//! The tray icon. Runs as the normal user and only talks to the service, so it never needs admin.

use std::cell::Cell;
use std::ptr::{null, null_mut};
use std::time::Duration;

use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Services::*;
use windows_sys::Win32::System::Threading::CreateMutexW;
use windows_sys::Win32::UI::HiDpi::{SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2};
use windows_sys::Win32::UI::Shell::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::common::*;
use crate::{icon, install, scm};

const WM_TRAY: u32 = WM_APP + 1;
const TIMER_ID: usize = 1;
const POLL_MS: u32 = 1500;

const ID_FULL: usize = 1;
const ID_DISCORD: usize = 2;
const ID_OFF: usize = 3;
const ID_INSTALL: usize = 10;
const ID_UNINSTALL: usize = 11;
const ID_EXIT: usize = 12;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Status {
    NotInstalled,
    ServiceStopped,
    Off,
    Running(Mode),
    Error(Mode),
}

impl Status {
    fn label(self) -> &'static str {
        match self {
            Status::NotInstalled => "Kurulu değil",
            Status::ServiceStopped => "Hizmet çalışmıyor",
            Status::Off => "Kapalı",
            Status::Running(Mode::DiscordOnly) => "Discord Modu Aktif",
            Status::Running(_) => "Tüm Sistem Aktif",
            Status::Error(_) => "Hata (goodbyedpi çalışmıyor)",
        }
    }

    fn message(self) -> &'static str {
        match self {
            Status::NotInstalled => "Hizmet kurulu değil. Menüden \"Kur\" seçeneğini kullanın.",
            Status::ServiceStopped => "GoodbyeDPI hizmeti çalışmıyor.",
            Status::Off => "DPI atlatma kapalı.",
            Status::Running(Mode::DiscordOnly) => "DPI atlatma yalnızca Discord için aktif.",
            Status::Running(_) => "Sistem genelinde DPI atlatma aktif.",
            Status::Error(_) => "goodbyedpi.exe sürekli kapanıyor veya başlatılamıyor. Antivirüs engelliyor olabilir.",
        }
    }

    fn mode(self) -> Option<Mode> {
        match self {
            Status::Off => Some(Mode::Off),
            Status::Running(m) | Status::Error(m) => Some(m),
            _ => None,
        }
    }
}

struct Icons {
    off: HICON,
    full: HICON,
    discord: HICON,
    error: HICON,
}

thread_local! {
    static HWND_CELL: Cell<HWND> = const { Cell::new(null_mut()) };
    static STATUS: Cell<Option<Status>> = const { Cell::new(None) };
    static ICONS: Cell<Option<&'static Icons>> = const { Cell::new(None) };
    static TASKBAR_CREATED: Cell<u32> = const { Cell::new(0) };
}

pub fn run() {
    unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };

    let mutex_name = wide("Local\\GoodbyeDPI-Tray-SingleInstance");
    let _mutex = unsafe { CreateMutexW(null(), 1, mutex_name.as_ptr()) };
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        return; // already running (e.g. autostart + manual launch)
    }

    if query_status() == Status::NotInstalled {
        let answer = message_box(
            "GoodbyeDPI Tray henüz kurulu değil. Şimdi kurulsun mu?\n\n\
             Kurulum yalnızca bir kez yönetici izni ister. Sonrasında Windows ile otomatik başlar \
             ve mod değiştirmek için yönetici izni gerekmez.",
            MB_YESNO | MB_ICONQUESTION,
        );
        if answer == IDYES {
            install::elevate("--install"); // the installer launches the installed tray itself
        }
        return;
    }

    unsafe {
        let hinstance = GetModuleHandleW(null());
        let class = wide("GoodbyeDPITrayWindow");
        let wc = WNDCLASSW {
            style: 0,
            lpfnWndProc: Some(wndproc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hinstance,
            hIcon: null_mut(),
            hCursor: null_mut(),
            hbrBackground: null_mut(),
            lpszMenuName: null(),
            lpszClassName: class.as_ptr(),
        };
        RegisterClassW(&wc);

        // A hidden top-level window (not message-only) so it receives the TaskbarCreated broadcast.
        let hwnd = CreateWindowExW(
            0, class.as_ptr(), class.as_ptr(), 0, 0, 0, 0, 0,
            null_mut(), null_mut(), hinstance, null(),
        );
        HWND_CELL.set(hwnd);

        let tc = wide("TaskbarCreated");
        TASKBAR_CREATED.set(RegisterWindowMessageW(tc.as_ptr()));

        let size = GetSystemMetrics(SM_CXSMICON).max(16);
        ICONS.set(Some(Box::leak(Box::new(Icons {
            off: icon::dot(size, (128, 128, 128)),
            full: icon::dot(size, (46, 160, 67)),
            discord: icon::dot(size, (88, 101, 242)),
            error: icon::dot(size, (218, 54, 51)),
        }))));

        add_icon();
        refresh(false);
        SetTimer(hwnd, TIMER_ID, POLL_MS, None);

        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_TRAY => {
            match lparam as u32 {
                WM_LBUTTONUP | WM_RBUTTONUP => show_menu(hwnd),
                _ => {}
            }
            0
        }
        WM_TIMER => {
            refresh(false);
            0
        }
        WM_DESTROY => {
            remove_icon();
            PostQuitMessage(0);
            0
        }
        m if m != 0 && m == TASKBAR_CREATED.get() => {
            // Explorer restarted; our icon is gone and must be re-added.
            add_icon();
            STATUS.set(None);
            refresh(false);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

fn query_status() -> Status {
    let service = match scm::open(SERVICE_NAME, SERVICE_QUERY_STATUS) {
        Ok(s) => s,
        Err(ERROR_SERVICE_DOES_NOT_EXIST) => return Status::NotInstalled,
        Err(_) => return Status::ServiceStopped,
    };
    if !service.is_running() {
        return Status::ServiceStopped;
    }
    match read_state() {
        None | Some((Mode::Off, _)) => Status::Off,
        Some((mode, Health::Error)) => Status::Error(mode),
        Some((mode, _)) => Status::Running(mode),
    }
}

fn base_nid() -> NOTIFYICONDATAW {
    let mut nid: NOTIFYICONDATAW = unsafe { std::mem::zeroed() };
    nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
    nid.hWnd = HWND_CELL.get();
    nid.uID = 1;
    nid
}

fn add_icon() {
    let mut nid = base_nid();
    nid.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
    nid.uCallbackMessage = WM_TRAY;
    nid.hIcon = ICONS.get().map_or(null_mut(), |i| i.off);
    copy_wide(&mut nid.szTip, APP_TITLE);
    unsafe { Shell_NotifyIconW(NIM_ADD, &nid) };
}

fn remove_icon() {
    let nid = base_nid();
    unsafe { Shell_NotifyIconW(NIM_DELETE, &nid) };
}

/// Re-reads the service state; updates the icon when it changed. `announce` forces a balloon.
fn refresh(announce: bool) {
    let status = query_status();
    let previous = STATUS.replace(Some(status));
    let changed = previous != Some(status);
    if !changed && !announce {
        return;
    }

    let Some(icons) = ICONS.get() else { return };
    let mut nid = base_nid();
    nid.uFlags = NIF_ICON | NIF_TIP;
    nid.hIcon = match status {
        Status::Running(Mode::DiscordOnly) => icons.discord,
        Status::Running(_) => icons.full,
        Status::Error(_) | Status::ServiceStopped => icons.error,
        _ => icons.off,
    };
    copy_wide(&mut nid.szTip, &format!("{APP_TITLE} - {}", status.label()));

    // Balloon on user actions, and unprompted when something goes wrong in the background.
    let went_bad = changed && previous.is_some() && matches!(status, Status::Error(_) | Status::ServiceStopped);
    if announce || went_bad {
        nid.uFlags |= NIF_INFO;
        nid.dwInfoFlags = if went_bad { NIIF_WARNING } else { NIIF_INFO };
        copy_wide(&mut nid.szInfoTitle, APP_TITLE);
        copy_wide(&mut nid.szInfo, status.message());
    }
    unsafe { Shell_NotifyIconW(NIM_MODIFY, &nid) };
}

fn set_mode(mode: Mode) {
    let access = SERVICE_QUERY_STATUS | SERVICE_START | SERVICE_USER_DEFINED_CONTROL;
    let result = scm::open(SERVICE_NAME, access).and_then(|service| {
        if !service.is_running() {
            service.start()?;
            service.wait_for(SERVICE_RUNNING, Duration::from_secs(10));
        }
        service.control(mode.control_code())
    });

    match result {
        Ok(()) => {
            std::thread::sleep(Duration::from_millis(500)); // let the service switch over
            refresh(true);
        }
        Err(err) => {
            message_box(
                &format!(
                    "Mod değiştirilemedi (hata {err}).\n\n\
                     Sorun devam ederse programı kaldırıp yeniden kurun."
                ),
                MB_OK | MB_ICONERROR,
            );
        }
    }
}

unsafe fn show_menu(hwnd: HWND) {
    let status = STATUS.get().unwrap_or_else(query_status);
    let menu = CreatePopupMenu();
    let add = |flags: u32, id: usize, text: &str| {
        let text = wide(text);
        AppendMenuW(menu, flags, id, text.as_ptr());
    };
    let check = |m: Mode| if status.mode() == Some(m) { MF_CHECKED } else { MF_UNCHECKED };

    add(MF_STRING | MF_GRAYED, 0, &format!("Durum: {}", status.label()));
    AppendMenuW(menu, MF_SEPARATOR, 0, null());
    if status == Status::NotInstalled {
        add(MF_STRING, ID_INSTALL, "Kur (Windows ile başlat)...");
    } else {
        add(MF_STRING | check(Mode::Full), ID_FULL, "Tüm Sistem (Sistem Geneli DPI Atlatma)");
        add(MF_STRING | check(Mode::DiscordOnly), ID_DISCORD, "Sadece Discord");
        add(MF_STRING | check(Mode::Off), ID_OFF, "Kapalı");
        AppendMenuW(menu, MF_SEPARATOR, 0, null());
        add(MF_STRING, ID_UNINSTALL, "Kaldır...");
    }
    add(MF_STRING, ID_EXIT, "Simgeyi gizle (seçili mod çalışmaya devam eder)");

    let mut pt = POINT { x: 0, y: 0 };
    GetCursorPos(&mut pt);
    // Required so the menu closes when clicking elsewhere (KB135788).
    SetForegroundWindow(hwnd);
    let cmd = TrackPopupMenu(menu, TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_NONOTIFY, pt.x, pt.y, 0, hwnd, null());
    PostMessageW(hwnd, WM_NULL, 0, 0);
    DestroyMenu(menu);

    match cmd as usize {
        ID_FULL => set_mode(Mode::Full),
        ID_DISCORD => set_mode(Mode::DiscordOnly),
        ID_OFF => set_mode(Mode::Off),
        ID_INSTALL => {
            if install::elevate("--install") {
                DestroyWindow(hwnd);
            }
        }
        // The uninstaller asks for confirmation and closes this tray itself.
        ID_UNINSTALL => {
            install::elevate("--uninstall");
        }
        ID_EXIT => {
            DestroyWindow(hwnd);
        }
        _ => {}
    }
}
