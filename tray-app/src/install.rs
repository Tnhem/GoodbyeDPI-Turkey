//! One-time elevated install / uninstall.

use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use windows_sys::Win32::System::Registry::{RegDeleteKeyValueW, RegSetKeyValueW, HKEY_LOCAL_MACHINE, REG_SZ};
use windows_sys::Win32::System::Services::SERVICE_ALL_ACCESS;
use windows_sys::Win32::System::Threading::CREATE_NO_WINDOW;
use windows_sys::Win32::UI::Shell::{IsUserAnAdmin, ShellExecuteExW, SHELLEXECUTEINFOW};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::common::*;
use crate::scm;

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";

/// Authenticated users may query, start and send mode-switch controls; only admins may stop,
/// reconfigure or delete. Built-in admins and SYSTEM keep the standard rights.
const SERVICE_SDDL: &str = "D:(A;;CCLCSWRPLOCRRC;;;AU)(A;;CCDCLCSWRPWPDTLOCRSDRCWDWO;;;BA)(A;;CCLCSWRPWPDTLOCRRC;;;SY)";

pub fn is_admin() -> bool {
    unsafe { IsUserAnAdmin() != 0 }
}

/// Re-launches this exe elevated (UAC prompt). Returns false if the user declined.
pub fn elevate(args: &str) -> bool {
    let Ok(exe) = std::env::current_exe() else { return false };
    let verb = wide("runas");
    let file = wide(&exe.to_string_lossy());
    let params = wide(args);
    let mut info: SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
    info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
    info.lpVerb = verb.as_ptr();
    info.lpFile = file.as_ptr();
    info.lpParameters = params.as_ptr();
    info.nShow = SW_SHOWNORMAL;
    unsafe { ShellExecuteExW(&mut info) != 0 }
}

pub fn run_install() {
    if !is_admin() {
        elevate("--install");
        return;
    }
    match install() {
        Ok(()) => {
            message_box(
                "Kurulum tamamlandı.\n\n\
                 GoodbyeDPI artık Windows ile otomatik başlar ve tepsi simgesinden yönetici izni \
                 gerekmeden mod değiştirebilirsiniz.\n\n\
                 Mod seçmek için görev çubuğundaki simgeye tıklayın.",
                MB_OK | MB_ICONINFORMATION,
            );
        }
        Err(e) => {
            message_box(&format!("Kurulum başarısız:\n{e}"), MB_OK | MB_ICONERROR);
        }
    }
}

pub fn run_uninstall() {
    if !is_admin() {
        elevate("--uninstall");
        return;
    }
    let answer = message_box(
        "GoodbyeDPI Tray kaldırılsın mı?\n\nHizmet, otomatik başlatma ve kurulu dosyalar silinecek.",
        MB_YESNO | MB_ICONQUESTION,
    );
    if answer != IDYES {
        return;
    }
    match uninstall() {
        Ok(()) => message_box("GoodbyeDPI Tray kaldırıldı.", MB_OK | MB_ICONINFORMATION),
        Err(e) => message_box(&format!("Kaldırma sırasında hata:\n{e}"), MB_OK | MB_ICONERROR),
    };
}

fn exe_dir() -> Result<PathBuf, String> {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .ok_or_else(|| "Program konumu bulunamadı.".to_string())
}

fn same_dir(a: &Path, b: &Path) -> bool {
    a.to_string_lossy().trim_end_matches('\\').eq_ignore_ascii_case(b.to_string_lossy().trim_end_matches('\\'))
}

fn install() -> Result<(), String> {
    let src = exe_dir()?;
    let dst = install_dir();

    // The upstream service_install_*.cmd service would run a second goodbyedpi.exe alongside ours.
    if scm::exists(LEGACY_SERVICE_NAME) {
        let answer = message_box(
            "Bilgisayarınızda eski \"GoodbyeDPI\" hizmeti kurulu (service_install_*.cmd ile kurulmuş).\n\n\
             İkisi aynı anda çalışamaz. Eski hizmet kaldırılsın mı?",
            MB_YESNO | MB_ICONWARNING,
        );
        if answer != IDYES {
            return Err("Eski GoodbyeDPI hizmeti kaldırılmadığı için kurulum iptal edildi.".into());
        }
        let legacy = scm::open(LEGACY_SERVICE_NAME, SERVICE_ALL_ACCESS)
            .map_err(|e| format!("Eski hizmet açılamadı (hata {e})."))?;
        legacy.stop_and_wait();
        legacy.delete().map_err(|e| format!("Eski hizmet silinemedi (hata {e})."))?;
    }

    // Upgrade: release the files we are about to overwrite.
    if let Ok(existing) = scm::open(SERVICE_NAME, SERVICE_ALL_ACCESS) {
        existing.stop_and_wait();
    }
    kill_other_trays();

    if !same_dir(&src, &dst) {
        copy_files(&src, &dst)?;
    }

    let exe = dst.join(EXE_NAME);
    let bin_path = format!("\"{}\" --service", exe.display());
    let service = scm::create_or_update(
        SERVICE_NAME,
        SERVICE_DISPLAY_NAME,
        &bin_path,
        "GoodbyeDPI Turkey DPI atlatma aracını çalıştırır. Mod, tepsi simgesinden değiştirilir.",
    )
    .map_err(|e| format!("Hizmet oluşturulamadı (hata {e})."))?;

    run_hidden("sc.exe", &["sdset", SERVICE_NAME, SERVICE_SDDL])?;
    run_hidden(
        "sc.exe",
        &["failure", SERVICE_NAME, "reset=", "86400", "actions=", "restart/5000/restart/5000/restart/30000"],
    )?;

    set_run_key(&format!("\"{}\"", exe.display()))?;

    std::fs::create_dir_all(data_dir()).map_err(|e| format!("Veri klasörü oluşturulamadı: {e}"))?;
    service.start().map_err(|e| format!("Hizmet başlatılamadı (hata {e})."))?;

    // Launching through explorer.exe starts the tray as the normal (non-elevated) user.
    let _ = Command::new("explorer.exe").arg(&exe).spawn();
    Ok(())
}

fn uninstall() -> Result<(), String> {
    if let Ok(service) = scm::open(SERVICE_NAME, SERVICE_ALL_ACCESS) {
        service.stop_and_wait();
        service.delete().map_err(|e| format!("Hizmet silinemedi (hata {e})."))?;
    }
    delete_run_key();
    kill_other_trays();
    let _ = std::fs::remove_dir_all(data_dir());

    let dst = install_dir();
    if same_dir(&exe_dir()?, &dst) {
        // Can't delete our own running exe; let a hidden cmd do it once we've exited.
        let _ = Command::new("cmd.exe")
            .raw_arg(format!("/c ping 127.0.0.1 -n 3 >nul & rmdir /s /q \"{}\"", dst.display()))
            .creation_flags(CREATE_NO_WINDOW)
            .spawn();
    } else if dst.exists() {
        retry(|| std::fs::remove_dir_all(&dst)).map_err(|e| format!("{} silinemedi: {e}", dst.display()))?;
    }
    Ok(())
}

fn copy_files(src: &Path, dst: &Path) -> Result<(), String> {
    let bin_src = src.join("x86_64");
    if !bin_src.join("goodbyedpi.exe").exists() {
        return Err(format!("goodbyedpi.exe bulunamadı:\n{}", bin_src.join("goodbyedpi.exe").display()));
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;

    let copy = |from: &Path, to: &Path| {
        retry(|| std::fs::copy(from, to)).map_err(|e| format!("{} kopyalanamadı: {e}", from.display()))
    };

    std::fs::create_dir_all(dst.join("x86_64")).map_err(|e| e.to_string())?;
    copy(&exe, &dst.join(EXE_NAME))?;
    copy(&src.join("discord_domains.txt"), &dst.join("discord_domains.txt"))?;
    copy_dir(&bin_src, &dst.join("x86_64"), &copy)?;
    if src.join("licenses").is_dir() {
        std::fs::create_dir_all(dst.join("licenses")).map_err(|e| e.to_string())?;
        copy_dir(&src.join("licenses"), &dst.join("licenses"), &copy)?;
    }
    Ok(())
}

fn copy_dir(from: &Path, to: &Path, copy: &dyn Fn(&Path, &Path) -> Result<u64, String>) -> Result<(), String> {
    for entry in std::fs::read_dir(from).map_err(|e| e.to_string())?.flatten() {
        if entry.path().is_file() {
            copy(&entry.path(), &to.join(entry.file_name()))?;
        }
    }
    Ok(())
}

/// Files can stay locked for a moment after the processes using them are killed.
fn retry<T>(mut f: impl FnMut() -> std::io::Result<T>) -> std::io::Result<T> {
    let mut attempt = 0;
    loop {
        match f() {
            Ok(v) => return Ok(v),
            Err(e) if attempt >= 10 => return Err(e),
            Err(_) => {
                attempt += 1;
                std::thread::sleep(Duration::from_millis(300));
            }
        }
    }
}

fn kill_other_trays() {
    let filter = format!("PID ne {}", std::process::id());
    let _ = Command::new("taskkill.exe")
        .args(["/F", "/IM", EXE_NAME, "/FI", &filter])
        .creation_flags(CREATE_NO_WINDOW)
        .status();
}

fn run_hidden(program: &str, args: &[&str]) -> Result<(), String> {
    let status = Command::new(program)
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .map_err(|e| format!("{program} çalıştırılamadı: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{program} {} başarısız oldu ({status}).", args.join(" ")))
    }
}

fn set_run_key(command: &str) -> Result<(), String> {
    let (key, name, value) = (wide(RUN_KEY), wide(SERVICE_NAME), wide(command));
    let err = unsafe {
        RegSetKeyValueW(
            HKEY_LOCAL_MACHINE,
            key.as_ptr(),
            name.as_ptr(),
            REG_SZ,
            value.as_ptr() as *const _,
            (value.len() * 2) as u32,
        )
    };
    if err == 0 {
        Ok(())
    } else {
        Err(format!("Otomatik başlatma kaydı yazılamadı (hata {err})."))
    }
}

fn delete_run_key() {
    let (key, name) = (wide(RUN_KEY), wide(SERVICE_NAME));
    unsafe { RegDeleteKeyValueW(HKEY_LOCAL_MACHINE, key.as_ptr(), name.as_ptr()) };
}
