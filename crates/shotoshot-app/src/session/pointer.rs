//! ROLE: Windows cursors of the portable pointers (`shotoshot_ui::cursor_image`): new text,
//! and the selection cross in each ink.
//! OWNER: The UI thread keeps one handle per shape and size for the life of the process: few
//! sizes ever appear, and a cursor destroyed while on screen would leave the pointer undefined.
use super::*;
use shotoshot_ui::cursor_image::{self, CrossInk};
use std::{cell::RefCell, collections::HashMap};

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Shape {
    NewText,
    Cross(CrossInk),
}

thread_local! {
    static CURSORS: RefCell<HashMap<(Shape, u32), HCURSOR>> = RefCell::new(HashMap::new());
}

/// The system cursor size, in pixels, for interface `scale`.
pub(super) fn size(scale: f64) -> u32 {
    let dpi = (96. * scale).round() as u32;
    unsafe { windows::Win32::UI::HiDpi::GetSystemMetricsForDpi(SM_CXCURSOR, dpi).max(32) as u32 }
}

/// The new-text cursor at the system cursor size for interface `scale`.
pub(super) fn new_text(scale: f64) -> Result<HCURSOR> {
    cached(Shape::NewText, size(scale))
}

/// The selection cross in `ink` at the system cursor size for interface `scale`.
pub(super) fn cross(scale: f64, ink: CrossInk) -> Result<HCURSOR> {
    cached(Shape::Cross(ink), size(scale))
}

fn cached(shape: Shape, size: u32) -> Result<HCURSOR> {
    CURSORS.with_borrow_mut(|cursors| {
        if let Some(cursor) = cursors.get(&(shape, size)) {
            return Ok(*cursor);
        }
        let cursor = create(&match shape {
            Shape::NewText => cursor_image::new_text(size),
            Shape::Cross(ink) => cursor_image::cross(size, ink),
        })?;
        cursors.insert((shape, size), cursor);
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
        let expected = shotoshot_ui::cursor_image::new_text(super::size(1.)).hotspot;
        assert!(!info.fIcon.as_bool());
        assert_eq!((info.xHotspot, info.yHotspot), expected);
        Ok(())
    }

    #[test]
    fn each_cross_ink_is_its_own_cursor_kept_per_size() -> super::Result<()> {
        use super::CrossInk::*;
        let black = super::cross(1., Black)?;
        assert_eq!(
            super::cross(1., Black)?,
            black,
            "kept for the next pointer move"
        );
        assert_ne!(super::cross(1., White)?, black);
        assert_ne!(
            super::cross(2., Black)?,
            black,
            "a denser screen has its own"
        );
        assert_ne!(super::new_text(1.)?, black, "the shapes never share a slot");
        Ok(())
    }

    #[test]
    fn the_session_cross_reads_the_shaded_capture_and_holds_its_ink_inside_the_band()
    -> super::Result<()> {
        use super::{super::*, CrossInk::*};
        let _com = ComApartment::new()?;
        let at = Point { x: 300., y: 250. };
        let mut s = Session::immediate(Image::solid(900, 600, [255, 255, 255, 255]))?;
        s.capture.eyedropper = true;
        let scale = s.capture.ui_scale();
        let paint = |s: &mut Session, grey: u8| s.capture.image.pixels.fill(grey);
        // The shade turns the white page mid grey, where the inverting system cross vanished.
        assert_eq!(s.cursor(at)?, super::cross(scale, Black)?);
        paint(&mut s, 20);
        assert_eq!(s.cursor(at)?, super::cross(scale, White)?);
        s.capture.region = Rect::new(Point { x: 100., y: 100. }, Point { x: 600., y: 400. });
        paint(&mut s, 118);
        assert_eq!(
            s.cursor(at)?,
            super::cross(scale, White)?,
            "kept inside the band"
        );
        paint(&mut s, 255);
        assert_eq!(s.cursor(at)?, super::cross(scale, Black)?);
        paint(&mut s, 118);
        assert_eq!(
            s.cursor(at)?,
            super::cross(scale, Black)?,
            "kept inside the band"
        );
        Ok(())
    }
}
