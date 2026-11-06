# GoodbyeDPI Turkey — Tepsi Uygulaması (Tray App)

[Türkçe](#türkçe) · [English](#english)

## Türkçe

### Nedir?

`goodbyedpi-tray.exe`, GoodbyeDPI Turkey için küçük bir sistem tepsisi (saat yanındaki simgeler) uygulamasıdır. Tek bir kurulumdan sonra DPI atlatmayı **yönetici izni istemeden** tepsi simgesinden açıp kapatabilir ve mod değiştirebilirsiniz.

- Rust ile yazılmıştır, yalnızca `windows-sys` kullanır; ~250 KB, .NET veya başka bir çalışma ortamı gerektirmez.
- Tek bir exe, birden fazla rol üstlenir:
  - argümansız: tepsi simgesi (normal kullanıcı olarak çalışır)
  - `--service`: `goodbyedpi.exe`'yi SYSTEM olarak çalıştıran ve çökerse yeniden başlatan **GoodbyeDPI-Tray** Windows hizmeti
  - `--install`: tek seferlik, yönetici yetkili kurulum
  - `--uninstall`: kaldırma
- **64 bit Windows** gerektirir (Windows 10/11 önerilir).

### Kurulum

1. [Releases](https://github.com/Tnhem/GoodbyeDPI-Turkey/releases) sayfasından `goodbyedpi-0.2.3rc3-turkey-tray.zip` dosyasını indirin.
2. ZIP'i bir klasöre çıkarın (ZIP içinden doğrudan çalıştırmayın).
3. `goodbyedpi-tray.exe`'yi çalıştırın ve **bir kez** çıkan UAC (yönetici izni) penceresini onaylayın.

Kurulum şunları yapar:

- Gerekli dosyaları `C:\Program Files\GoodbyeDPI-Tray` klasörüne kopyalar.
- Windows ile otomatik başlayan **GoodbyeDPI-Tray** hizmetini kaydeder.
- Tepsi simgesinin oturum açıldığında başlaması için HKLM `Run` kaydı ekler.
- Bilgisayarda `service_install_*.cmd` betikleriyle kurulmuş eski **GoodbyeDPI** hizmeti varsa, onu kaldırmayı teklif eder (iki hizmet aynı anda çalışamaz).

Bu uygulama `service_install_*.cmd` betiklerinin yerini alır; onları ayrıca çalıştırmanıza gerek yoktur.

### Modlar

Tepsi simgesine sağ tıklayın:

| Menü | Açıklama |
|---|---|
| **Tüm Sistem (Sistem Geneli DPI Atlatma)** | Tüm trafik için DPI atlatma. `turkey_dnsredir.cmd` ile aynı goodbyedpi argümanları. |
| **Sadece Discord** | Aynı argümanlar + `--blacklist discord_domains.txt`; yalnızca Discord alan adları etkilenir. |
| **Kapalı** | goodbyedpi çalışmaz. |

Kurulumdan sonra mod değiştirmek için yönetici izni gerekmez. Seçilen mod hatırlanır ve bilgisayar yeniden başladığında otomatik uygulanır. "Simgeyi gizle" yalnızca simgeyi kapatır; seçili mod çalışmaya devam eder.

### Kaldırma

- Tepsi menüsünden **Kaldır...** seçeneğini kullanın, veya
- komut satırından: `goodbyedpi-tray.exe --uninstall`

Hizmet, otomatik başlatma kaydı ve `C:\Program Files\GoodbyeDPI-Tray` silinir.

### Bilinen durumlar

- **IPv6:** Tüm Sistem modunda Yandex DNS yönlendirmesi bazen zaman aşımına uğrayan Google IPv6 adresleri döndürebilir; tarayıcılar kısa bir gecikmeyle IPv4'e geri döner. Bu, upstream `turkey_dnsredir.cmd` ile aynı davranıştır.
- **Antivirüs:** Bazı antivirüs programları WinDivert (`WinDivert.dll`, `WinDivert64.sys`) dosyalarını zararlı olarak işaretleyebilir. Bu, upstream GoodbyeDPI ile aynı yanlış alarmdır.
- **İmzasız:** `goodbyedpi-tray.exe` dijital olarak imzalanmamıştır; Windows SmartScreen uyarı gösterebilir ("Ek bilgi" → "Yine de çalıştır").

### Kaynaktan derleme

[Rust](https://rustup.rs/) (MSVC araç zinciri) gerekir:

```
cd tray-app
cargo build --release
```

Çıktı: `tray-app\target\release\goodbyedpi-tray.exe`. Bu exe'yi GoodbyeDPI Turkey sürüm klasörüne (`x86_64\` ve `discord_domains.txt` ile aynı yere) koyun.

---

## English

### What is it?

`goodbyedpi-tray.exe` is a small system tray app for GoodbyeDPI Turkey. After a one-time install you can switch DPI circumvention on/off and change modes from the tray icon **without admin rights**.

- Written in Rust using only `windows-sys`; ~250 KB, no .NET or other runtime needed.
- One exe with several roles:
  - no arguments: tray icon (runs as the normal user)
  - `--service`: Windows service **GoodbyeDPI-Tray** that runs `goodbyedpi.exe` as SYSTEM and restarts it if it crashes
  - `--install`: one-time elevated install
  - `--uninstall`: removal
- Requires **64-bit Windows** (Windows 10/11 recommended).

### Install

1. Download `goodbyedpi-0.2.3rc3-turkey-tray.zip` from [Releases](https://github.com/Tnhem/GoodbyeDPI-Turkey/releases).
2. Extract the zip to a folder (don't run it from inside the zip).
3. Run `goodbyedpi-tray.exe` and accept the UAC prompt **once**.

The installer copies the files to `C:\Program Files\GoodbyeDPI-Tray`, registers the auto-start **GoodbyeDPI-Tray** service, adds an HKLM `Run` entry so the tray icon starts at logon, and offers to remove the old **GoodbyeDPI** service installed by the `service_install_*.cmd` scripts. This app replaces those scripts.

### Modes

Right-click the tray icon:

- **Tüm Sistem** (full system): DPI circumvention for all traffic, same goodbyedpi arguments as `turkey_dnsredir.cmd`.
- **Sadece Discord** (Discord only): same arguments plus `--blacklist discord_domains.txt`.
- **Kapalı** (off): goodbyedpi is stopped.

Switching modes needs no admin rights. The selected mode is remembered and applied automatically at boot. "Simgeyi gizle" only hides the icon; the selected mode keeps running.

### Uninstall

Use **Kaldır...** in the tray menu, or run `goodbyedpi-tray.exe --uninstall`. This removes the service, the autostart entry and `C:\Program Files\GoodbyeDPI-Tray`.

### Known caveats

- **IPv6:** in full-system mode the Yandex DNS redirect can return Google IPv6 addresses that time out; browsers fall back to IPv4 after a short delay. This is the same behavior as upstream `turkey_dnsredir.cmd`.
- **Antivirus:** some antivirus products flag WinDivert (`WinDivert.dll`, `WinDivert64.sys`). This is the same false positive as upstream GoodbyeDPI.
- **Unsigned:** `goodbyedpi-tray.exe` is not code-signed, so Windows SmartScreen may warn ("More info" → "Run anyway").

### Build from source

Requires [Rust](https://rustup.rs/) (MSVC toolchain):

```
cd tray-app
cargo build --release
```

Output: `tray-app\target\release\goodbyedpi-tray.exe`. Put it in the GoodbyeDPI Turkey release folder, next to `x86_64\` and `discord_domains.txt`.

## License / Lisans

Apache-2.0. Copyright 2026 Kayra Ulusoy. Based on [GoodbyeDPI](https://github.com/ValdikSS/GoodbyeDPI) by ValdikSS and [GoodbyeDPI-Turkey](https://github.com/cagritaskn/GoodbyeDPI-Turkey) by cagritaskn.
