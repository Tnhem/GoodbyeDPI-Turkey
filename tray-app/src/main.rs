#![windows_subsystem = "windows"]

//! One executable, three roles:
//!   (no args)     tray icon, runs as the normal user (no admin needed)
//!   --service     Windows service that owns goodbyedpi.exe (started by the SCM as SYSTEM)
//!   --install     one-time elevated setup: copies files, registers service + autostart
//!   --uninstall   elevated removal

mod common;
mod icon;
mod install;
mod scm;
mod service;
mod tray;

fn main() {
    match std::env::args().nth(1).as_deref() {
        Some("--service") => service::run(),
        Some("--install") => install::run_install(),
        Some("--uninstall") => install::run_uninstall(),
        _ => tray::run(),
    }
}
