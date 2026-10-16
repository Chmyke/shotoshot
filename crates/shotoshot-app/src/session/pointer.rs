//! ROLE: Windows cursor of the portable new-text pointer (`shotoshot_ui::cursor_image`).
//! OWNER: The UI thread keeps one handle per size for the life of the process: few sizes ever
//! appear, and a cursor destroyed while on screen would leave the pointer undefined.
use super::*;
use std::{cell::RefCell, collections::HashMap};

thread_local! {
    static CURSORS: RefCell<HashMap<u32, HCURSOR>> = RefCell::new(HashMap::new());
}

/// The new-text cursor at the system cursor size for interface `scale`.
pub(super) fn new_text(scale: f64) -> Result<HCURSOR> {
    let dpi = (96. * scale).round() as u32;
    let size = unsafe {
        windows::Win32::UI::HiDpi::GetSystemMetricsForDpi(SM_CXCURSOR, dpi).max(32) as u32
    };
    CURSORS.with_borrow_mut(|cursors| {
        if let Some(cursor) = cursors.get(&size) {
            return Ok(*cursor);
        }
        let cursor = create(&shotoshot_ui::cursor_image::new_text(size))?;
        cursors.insert(size, cursor);
        Ok(cursor)
    })
}

fn create(image: &shotoshot_ui::cursor_image::CursorImage) -> Result<HCURSOR> {
    let side = image.size as i32;
    unsafe {
        let header = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: side,
                // Negative: rows top down, as the image lists them.
                biHeight: -side,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits = std::ptr::null_mut();
        let color = CreateDIBSection(None, &header, DIB_RGB_COLORS, &mut bits, None, 0)?;
        let bgra = std::slice::from_raw_parts_mut(bits as *mut u8, image.rgba.len());
        let pixels = bgra.as_chunks_mut::<4>().0.iter_mut();
        for (to, &[r, g, b, a]) in pixels.zip(image.rgba.as_chunks::<4>().0) {
            *to = [b, g, r, a];
        }
        // The colour bitmap carries the alpha; the mask only has to exist, and be blank.
        let rows = (side as usize).div_ceil(16) * 2;
        let blank = vec![0u8; rows * side as usize];
        let mask = CreateBitmap(side, side, 1, 1, Some(blank.as_ptr().cast()));
        let info = ICONINFO {
            fIcon: false.into(),
            xHotspot: image.hotspot.0,
            yHotspot: image.hotspot.1,
            hbmMask: mask,
            hbmColor: color,
        };
        let icon = CreateIconIndirect(&info);
        let _ = DeleteObject(mask.into());
        let _ = DeleteObject(color.into());
        Ok(HCURSOR(icon?.0))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_new_text_cursor_is_made_once_per_size_with_its_hot_spot() -> super::Result<()> {
        use windows::Win32::{Graphics::Gdi::DeleteObject, UI::WindowsAndMessaging::*};
        let cursor = super::new_text(1.)?;
        assert_eq!(
            super::new_text(1.)?,
            cursor,
            "kept for the next pointer move"
        );
        assert_ne!(super::new_text(2.)?, cursor, "a denser screen has its own");
        let mut info = ICONINFO::default();
        unsafe {
            GetIconInfo(HICON(cursor.0), &mut info)?;
            let _ = DeleteObject(info.hbmMask.into());
            let _ = DeleteObject(info.hbmColor.into());
        }
        let size = unsafe {
            windows::Win32::UI::HiDpi::GetSystemMetricsForDpi(SM_CXCURSOR, 96).max(32) as u32
        };
        let expected = shotoshot_ui::cursor_image::new_text(size).hotspot;
        assert!(!info.fIcon.as_bool());
        assert_eq!((info.xHotspot, info.yHotspot), expected);
        Ok(())
    }
}
