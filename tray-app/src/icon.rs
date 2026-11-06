//! Draws the colored status dot used as the tray icon, so no .ico files are needed.

use std::ptr::null_mut;

use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::UI::WindowsAndMessaging::{CreateIconIndirect, HICON, ICONINFO};

pub fn dot(size: i32, (r, g, b): (u8, u8, u8)) -> HICON {
    unsafe {
        let mut bmi: BITMAPINFO = std::mem::zeroed();
        bmi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        bmi.bmiHeader.biWidth = size;
        bmi.bmiHeader.biHeight = -size; // top-down
        bmi.bmiHeader.biPlanes = 1;
        bmi.bmiHeader.biBitCount = 32;
        bmi.bmiHeader.biCompression = BI_RGB;

        let mut bits = null_mut();
        let hdc = GetDC(null_mut());
        let color = CreateDIBSection(hdc, &bmi, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
        ReleaseDC(null_mut(), hdc);
        if color.is_null() || bits.is_null() {
            return null_mut();
        }

        let pixels = std::slice::from_raw_parts_mut(bits as *mut u32, (size * size) as usize);
        let center = size as f32 / 2.0;
        let radius = center - 1.0;
        let border = (size as f32 / 16.0).max(1.0);

        for y in 0..size {
            for x in 0..size {
                let dx = x as f32 + 0.5 - center;
                let dy = y as f32 + 0.5 - center;
                let dist = (dx * dx + dy * dy).sqrt();
                // Anti-aliased coverage of the whole dot, and of its lighter interior.
                let alpha = (radius - dist + 0.5).clamp(0.0, 1.0);
                let inner = (radius - border - dist + 0.5).clamp(0.0, 1.0);
                let shade = |c: u8| {
                    let dark = c as f32 * 0.6;
                    ((dark + (c as f32 - dark) * inner) * alpha) as u32 // premultiplied
                };
                pixels[(y * size + x) as usize] =
                    ((alpha * 255.0) as u32) << 24 | shade(r) << 16 | shade(g) << 8 | shade(b);
            }
        }

        // The AND mask is ignored for 32-bit icons with alpha, but must exist.
        let mask_bytes = vec![0u8; (((size + 15) / 16 * 2) * size) as usize];
        let mask = CreateBitmap(size, size, 1, 1, mask_bytes.as_ptr() as *const _);

        let info = ICONINFO { fIcon: 1, xHotspot: 0, yHotspot: 0, hbmMask: mask, hbmColor: color };
        let icon = CreateIconIndirect(&info);
        DeleteObject(color);
        DeleteObject(mask);
        icon
    }
}
