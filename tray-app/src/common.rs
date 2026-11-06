use std::path::PathBuf;
use std::ptr::null_mut;

use windows_sys::Win32::UI::WindowsAndMessaging::{MessageBoxW, MESSAGEBOX_STYLE};

pub const SERVICE_NAME: &str = "GoodbyeDPI-Tray";
pub const SERVICE_DISPLAY_NAME: &str = "GoodbyeDPI Turkey (Tray)";
/// Service name used by the upstream service_install_*.cmd scripts.
pub const LEGACY_SERVICE_NAME: &str = "GoodbyeDPI";
pub const EXE_NAME: &str = "goodbyedpi-tray.exe";
pub const APP_TITLE: &str = "GoodbyeDPI Turkey";

/// User-defined service control codes (128..=255) carry the requested mode.
const CONTROL_BASE: u32 = 128;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Off = 0,
    Full = 1,
    DiscordOnly = 2,
}

impl Mode {
    pub fn as_str(self) -> &'static str {
        match self {
            Mode::Off => "Off",
            Mode::Full => "Full",
            Mode::DiscordOnly => "DiscordOnly",
        }
    }

    pub fn parse(s: &str) -> Option<Mode> {
        match s.trim() {
            "Off" => Some(Mode::Off),
            "Full" => Some(Mode::Full),
            "DiscordOnly" => Some(Mode::DiscordOnly),
            _ => None,
        }
    }

    pub fn control_code(self) -> u32 {
        CONTROL_BASE + self as u32
    }

    pub fn from_control_code(code: u32) -> Option<Mode> {
        match code.checked_sub(CONTROL_BASE)? {
            0 => Some(Mode::Off),
            1 => Some(Mode::Full),
            2 => Some(Mode::DiscordOnly),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Health {
    Off,
    Running,
    /// goodbyedpi.exe keeps crashing or can't be started; the service gave up.
    Error,
}

impl Health {
    fn as_str(self) -> &'static str {
        match self {
            Health::Off => "off",
            Health::Running => "running",
            Health::Error => "error",
        }
    }

    fn parse(s: &str) -> Option<Health> {
        match s {
            "off" => Some(Health::Off),
            "running" => Some(Health::Running),
            "error" => Some(Health::Error),
            _ => None,
        }
    }
}

/// Admin-only location the service runs from (so a normal user can't swap the binaries).
pub fn install_dir() -> PathBuf {
    let base = std::env::var_os("ProgramFiles").unwrap_or_else(|| r"C:\Program Files".into());
    PathBuf::from(base).join("GoodbyeDPI-Tray")
}

/// Written by the service, read by the tray.
pub fn data_dir() -> PathBuf {
    let base = std::env::var_os("ProgramData").unwrap_or_else(|| r"C:\ProgramData".into());
    PathBuf::from(base).join("GoodbyeDPI-Tray")
}

pub fn mode_path() -> PathBuf {
    data_dir().join("mode.txt")
}

fn state_path() -> PathBuf {
    data_dir().join("state.txt")
}

pub fn load_mode() -> Mode {
    std::fs::read_to_string(mode_path())
        .ok()
        .and_then(|s| Mode::parse(&s))
        .unwrap_or(Mode::Off)
}

pub fn save_mode(mode: Mode) {
    let _ = std::fs::create_dir_all(data_dir());
    let _ = std::fs::write(mode_path(), mode.as_str());
}

pub fn write_state(mode: Mode, health: Health) {
    let _ = std::fs::create_dir_all(data_dir());
    let _ = std::fs::write(state_path(), format!("{} {}", mode.as_str(), health.as_str()));
}

pub fn read_state() -> Option<(Mode, Health)> {
    let text = std::fs::read_to_string(state_path()).ok()?;
    let mut parts = text.split_whitespace();
    let mode = Mode::parse(parts.next()?)?;
    let health = Health::parse(parts.next()?)?;
    Some((mode, health))
}

/// Null-terminated UTF-16 for Win32 calls.
pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Copies `s` into a fixed-size Win32 string buffer, truncating and keeping the terminator.
pub fn copy_wide(dst: &mut [u16], s: &str) {
    let max = dst.len().saturating_sub(1);
    let mut n = 0;
    for (slot, ch) in dst.iter_mut().zip(s.encode_utf16()).take(max) {
        *slot = ch;
        n += 1;
    }
    dst[n] = 0;
}

pub fn message_box(text: &str, style: MESSAGEBOX_STYLE) -> i32 {
    let text = wide(text);
    let title = wide(APP_TITLE);
    unsafe { MessageBoxW(null_mut(), text.as_ptr(), title.as_ptr(), style) }
}
