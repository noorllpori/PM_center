use base64::Engine;
use image::{ImageBuffer, Rgba};
use std::ffi::OsStr;
use std::io::Cursor;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;

#[tauri::command]
pub async fn extract_icon(path: String) -> Result<Option<String>, String> {
    let path = Path::new(&path);
    if !path.exists() {
        return Ok(None);
    }
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    if ext != "exe" && ext != "dll" {
        return Ok(None);
    }
    match extract_windows_icon(path) {
        Some(png_bytes) => {
            let b64 = base64::engine::general_purpose::STANDARD.encode(&png_bytes);
            Ok(Some(format!("data:image/png;base64,{}", b64)))
        }
        None => Ok(None),
    }
}

#[cfg(windows)]
fn extract_windows_icon(path: &Path) -> Option<Vec<u8>> {
    use windows::Win32::Graphics::Gdi::{
        CreateCompatibleDC, DeleteDC, DeleteObject, GetDC, GetDIBits, ReleaseDC, SelectObject,
        BITMAP, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
    };
    use windows::Win32::UI::Shell::ExtractAssociatedIconW;
    use windows::Win32::UI::WindowsAndMessaging::{DestroyIcon, GetIconInfo, ICONINFO};

    unsafe {
        let mut wide_path: [u16; 128] = [0; 128];
        let path_wide: Vec<u16> = OsStr::new(path).encode_wide().collect();
        let len = path_wide.len().min(127);
        wide_path[..len].copy_from_slice(&path_wide[..len]);
        wide_path[len] = 0;

        let mut icon_index: u16 = 0;
        let h_icon = ExtractAssociatedIconW(None, &mut wide_path, &mut icon_index);
        if h_icon.is_invalid() {
            return None;
        }

        let mut icon_info = ICONINFO::default();
        if GetIconInfo(h_icon, &mut icon_info).is_err() {
            let _ = DestroyIcon(h_icon);
            return None;
        }

        let hdc_screen = GetDC(None);
        let hdc_mem = CreateCompatibleDC(hdc_screen);
        let _old_bmp = SelectObject(hdc_mem, icon_info.hbmColor);

        let mut bmp: BITMAP = std::mem::zeroed();
        let bmp_size = std::mem::size_of::<BITMAP>() as i32;
        if windows::Win32::Graphics::Gdi::GetObjectW(
            icon_info.hbmColor,
            bmp_size,
            Some(&mut bmp as *mut _ as *mut _),
        ) == 0
        {
            let _ = DeleteDC(hdc_mem);
            let _ = ReleaseDC(None, hdc_screen);
            let _ = DeleteObject(icon_info.hbmColor);
            let _ = DeleteObject(icon_info.hbmMask);
            let _ = DestroyIcon(h_icon);
            return None;
        }

        let width = bmp.bmWidth;
        let height = bmp.bmHeight;

        let mut bmi = BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            biHeight: -height,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0 as u32,
            biSizeImage: 0,
            biXPelsPerMeter: 0,
            biYPelsPerMeter: 0,
            biClrUsed: 0,
            biClrImportant: 0,
        };

        let mut buffer: Vec<u8> = vec![0; (width * height * 4) as usize];

        if GetDIBits(
            hdc_mem,
            icon_info.hbmColor,
            0,
            height as u32,
            Some(buffer.as_mut_ptr() as *mut _),
            std::ptr::addr_of_mut!(bmi) as *mut _,
            DIB_RGB_COLORS,
        ) == 0
        {
            let _ = DeleteDC(hdc_mem);
            let _ = ReleaseDC(None, hdc_screen);
            let _ = DeleteObject(icon_info.hbmColor);
            let _ = DeleteObject(icon_info.hbmMask);
            let _ = DestroyIcon(h_icon);
            return None;
        }

        let _ = DeleteDC(hdc_mem);
        let _ = ReleaseDC(None, hdc_screen);
        let _ = DeleteObject(icon_info.hbmColor);
        let _ = DeleteObject(icon_info.hbmMask);
        let _ = DestroyIcon(h_icon);

        let mut rgba_buffer: Vec<u8> = Vec::with_capacity((width * height * 4) as usize);
        for y in 0..height {
            for x in 0..width {
                let idx = ((y * width + x) * 4) as usize;
                rgba_buffer.push(buffer[idx + 2]); // r
                rgba_buffer.push(buffer[idx + 1]); // g
                rgba_buffer.push(buffer[idx]); // b
                rgba_buffer.push(buffer[idx + 3]); // a
            }
        }

        let img = ImageBuffer::<Rgba<u8>, _>::from_raw(width as u32, height as u32, rgba_buffer)?;
        let mut png_bytes: Vec<u8> = Vec::new();
        {
            let mut cursor = Cursor::new(&mut png_bytes);
            img.write_to(&mut cursor, image::ImageFormat::Png).ok()?;
        }
        Some(png_bytes)
    }
}

#[cfg(not(windows))]
fn extract_windows_icon(_path: &Path) -> Option<Vec<u8>> {
    None
}
