use windows::{
    Win32::{Graphics::Gdi::*, UI::WindowsAndMessaging::*},
    core::*,
};

pub use shotoshot_ui::Image;

/// Windows views of the portable frame.
pub trait ImageExt {
    fn validate(&self) -> Result<()>;
    fn info(&self) -> BITMAPINFO;
}
impl ImageExt for Image {
    fn validate(&self) -> Result<()> {
        self.check()
            .map_err(|message| Error::new(windows::Win32::Foundation::E_INVALIDARG, message))
    }
    fn info(&self) -> BITMAPINFO {
        BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: self.width as i32,
                biHeight: -(self.height as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        }
    }
}
struct CaptureSurface {
    screen: HDC,
    memory: HDC,
    bitmap: HBITMAP,
    previous: HGDIOBJ,
}
impl Drop for CaptureSurface {
    fn drop(&mut self) {
        unsafe {
            if !self.previous.is_invalid() {
                SelectObject(self.memory, self.previous);
            }
            if !self.bitmap.is_invalid() {
                let _ = DeleteObject(self.bitmap.into());
            }
            if !self.memory.is_invalid() {
                let _ = DeleteDC(self.memory);
            }
            ReleaseDC(None, self.screen);
        }
    }
}

/// A valid live canvas has dimensions and storage, but reads no screen pixels.
pub(crate) fn blank_desktop() -> Result<Image> {
    unsafe {
        let (width, height) = (
            GetSystemMetrics(SM_CXVIRTUALSCREEN),
            GetSystemMetrics(SM_CYVIRTUALSCREEN),
        );
        if width <= 0 || height <= 0 {
            return Err(Error::from_thread());
        }
        let mut image = Image::solid(width as u32, height as u32, [0, 0, 0, 255]);
        image.left = GetSystemMetrics(SM_XVIRTUALSCREEN);
        image.top = GetSystemMetrics(SM_YVIRTUALSCREEN);
        Ok(image)
    }
}
pub fn capture_desktop() -> Result<Image> {
    // PITFALL: A top-down DIB preserves physical virtual-desktop coordinates, including negatives.
    unsafe {
        let mut image = blank_desktop()?;
        let (width, height) = (image.width as i32, image.height as i32);
        let screen = GetDC(None);
        if screen.is_invalid() {
            return Err(Error::from_thread());
        }
        let mut surface = CaptureSurface {
            screen,
            memory: CreateCompatibleDC(Some(screen)),
            bitmap: HBITMAP::default(),
            previous: HGDIOBJ::default(),
        };
        if surface.memory.is_invalid() {
            return Err(Error::from_thread());
        }
        let mut bits = std::ptr::null_mut();
        surface.bitmap = CreateDIBSection(
            Some(screen),
            &image.info(),
            DIB_RGB_COLORS,
            &mut bits,
            None,
            0,
        )?;
        surface.previous = SelectObject(surface.memory, surface.bitmap.into());
        BitBlt(
            surface.memory,
            0,
            0,
            width,
            height,
            Some(screen),
            image.left,
            image.top,
            SRCCOPY | CAPTUREBLT,
        )?;
        GdiFlush().ok()?;
        std::ptr::copy_nonoverlapping(
            bits.cast::<u8>(),
            image.pixels.as_mut_ptr(),
            image.pixels.len(),
        );
        for pixel in image.pixels.as_chunks_mut::<4>().0 {
            pixel[3] = 255;
        }
        Ok(image)
    }
}

/// Reused local DIB for the live eyedropper. The portable image keeps its desktop size;
/// only the pixels visible in the loupe are refreshed.
pub(crate) struct DesktopSample(LiveSurface);
impl DesktopSample {
    pub fn new() -> Result<Self> {
        let cells = shotoshot_ui::MAGNIFIER_CELLS as u32;
        Ok(Self(LiveSurface::new(cells, cells)?))
    }
    pub fn refresh(&self, image: &mut Image, pointer: shotoshot_core::Point) -> Result<()> {
        unsafe {
            if (
                image.left,
                image.top,
                image.width as i32,
                image.height as i32,
            ) != (
                GetSystemMetrics(SM_XVIRTUALSCREEN),
                GetSystemMetrics(SM_YVIRTUALSCREEN),
                GetSystemMetrics(SM_CXVIRTUALSCREEN),
                GetSystemMetrics(SM_CYVIRTUALSCREEN),
            ) {
                return Err(Error::new(
                    windows::Win32::Foundation::E_FAIL,
                    "Display configuration changed. Start a new capture.",
                ));
            }
            let rect = sample_bounds(image, pointer);
            let (width, height) = (rect.right - rect.left, rect.bottom - rect.top);
            BitBlt(
                self.0.dc(),
                0,
                0,
                width,
                height,
                Some(self.0.surface.screen),
                rect.left,
                rect.top,
                SRCCOPY | CAPTUREBLT,
            )?;
            GdiFlush().ok()?;
            let bytes = self.0.bits();
            let stride = shotoshot_ui::MAGNIFIER_CELLS as usize * 4;
            let (x, y) = (
                (rect.left - image.left) as usize,
                (rect.top - image.top) as usize,
            );
            for row in 0..height as usize {
                let to = ((y + row) * image.width as usize + x) * 4;
                let pixels = &mut image.pixels[to..to + width as usize * 4];
                pixels.copy_from_slice(&bytes[row * stride..row * stride + pixels.len()]);
                for pixel in pixels.as_chunks_mut::<4>().0 {
                    pixel[3] = 255;
                }
            }
            Ok(())
        }
    }
}
fn sample_bounds(
    image: &Image,
    pointer: shotoshot_core::Point,
) -> windows::Win32::Foundation::RECT {
    let radius = shotoshot_ui::MAGNIFIER_CELLS / 2;
    let x = pointer.x.clamp(0., (image.width - 1) as f64) as i32;
    let y = pointer.y.clamp(0., (image.height - 1) as f64) as i32;
    windows::Win32::Foundation::RECT {
        left: image.left + (x - radius).max(0),
        top: image.top + (y - radius).max(0),
        right: image.left + (x + radius + 1).min(image.width as i32),
        bottom: image.top + (y + radius + 1).min(image.height as i32),
    }
}

pub fn paint_image(
    width: u32,
    height: u32,
    paint: impl FnOnce(HDC) -> Result<()>,
) -> Result<Image> {
    unsafe {
        let mut image = Image::solid(width, height, [0, 0, 0, 255]);
        let screen = GetDC(None);
        if screen.is_invalid() {
            return Err(Error::from_thread());
        }
        let mut surface = CaptureSurface {
            screen,
            memory: CreateCompatibleDC(Some(screen)),
            bitmap: HBITMAP::default(),
            previous: HGDIOBJ::default(),
        };
        if surface.memory.is_invalid() {
            return Err(Error::from_thread());
        }
        let mut bits = std::ptr::null_mut();
        surface.bitmap = CreateDIBSection(
            Some(screen),
            &image.info(),
            DIB_RGB_COLORS,
            &mut bits,
            None,
            0,
        )?;
        surface.previous = SelectObject(surface.memory, surface.bitmap.into());
        paint(surface.memory)?;
        GdiFlush().ok()?;
        std::ptr::copy_nonoverlapping(
            bits.cast::<u8>(),
            image.pixels.as_mut_ptr(),
            image.pixels.len(),
        );
        for p in image.pixels.as_chunks_mut::<4>().0 {
            p[3] = 255;
        }
        Ok(image)
    }
}

/// Persistent premultiplied DIB an overlay is drawn into: presented as a layered window
/// (live annotations, presentation layer) or copied to an opaque one. No frame polling.
pub(crate) struct LiveSurface {
    surface: CaptureSurface,
    // Top-down premultiplied BGRA owned by the DIB section above.
    bits: *const u8,
    width: u32,
    height: u32,
}
impl LiveSurface {
    pub fn new(width: u32, height: u32) -> Result<Self> {
        unsafe {
            let image = Image {
                width,
                height,
                left: 0,
                top: 0,
                pixels: Vec::new(),
            };
            if width == 0
                || height == 0
                || width > i32::MAX as u32
                || height > i32::MAX as u32
                || (width as usize)
                    .checked_mul(height as usize)
                    .and_then(|pixels| pixels.checked_mul(4))
                    .is_none_or(|bytes| bytes > isize::MAX as usize)
            {
                return Err(Error::new(
                    windows::Win32::Foundation::E_INVALIDARG,
                    "Invalid surface dimensions",
                ));
            }
            let screen = GetDC(None);
            if screen.is_invalid() {
                return Err(Error::from_thread());
            }
            let mut surface = CaptureSurface {
                screen,
                memory: CreateCompatibleDC(Some(screen)),
                bitmap: HBITMAP::default(),
                previous: HGDIOBJ::default(),
            };
            if surface.memory.is_invalid() {
                return Err(Error::from_thread());
            }
            let mut bits = std::ptr::null_mut();
            surface.bitmap = CreateDIBSection(
                Some(screen),
                &image.info(),
                DIB_RGB_COLORS,
                &mut bits,
                None,
                0,
            )?;
            surface.previous = SelectObject(surface.memory, surface.bitmap.into());
            Ok(Self {
                surface,
                bits: bits.cast(),
                width,
                height,
            })
        }
    }
    #[cfg(test)]
    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        assert!(x < self.width && y < self.height);
        unsafe {
            let mut info = DIBSECTION::default();
            assert_ne!(
                GetObjectW(
                    self.surface.bitmap.into(),
                    std::mem::size_of::<DIBSECTION>() as i32,
                    Some((&mut info as *mut DIBSECTION).cast())
                ),
                0
            );
            std::ptr::read_unaligned(
                info.dsBm
                    .bmBits
                    .cast::<u8>()
                    .add(((y * self.width + x) * 4) as usize)
                    .cast::<[u8; 4]>(),
            )
        }
    }
    pub fn dc(&self) -> HDC {
        self.surface.memory
    }
    /// Pixels as last flushed by `present`; GDI may still be batching later drawing.
    pub fn bits(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts(self.bits, (self.width * self.height * 4) as usize) }
    }
    /// Copies the whole surface to an opaque window's paint DC.
    pub fn copy_to(&self, window: HDC) -> Result<()> {
        unsafe {
            GdiFlush().ok()?;
            BitBlt(
                window,
                0,
                0,
                self.width as i32,
                self.height as i32,
                Some(self.surface.memory),
                0,
                0,
                SRCCOPY,
            )
        }
    }
    pub fn present(&self, hwnd: windows::Win32::Foundation::HWND) -> Result<()> {
        unsafe {
            GdiFlush().ok()?;
            UpdateLayeredWindow(
                hwnd,
                Some(self.surface.screen),
                None,
                Some(&windows::Win32::Foundation::SIZE {
                    cx: self.width as i32,
                    cy: self.height as i32,
                }),
                Some(self.surface.memory),
                Some(&windows::Win32::Foundation::POINT { x: 0, y: 0 }),
                windows::Win32::Foundation::COLORREF(0),
                Some(&BLENDFUNCTION {
                    BlendOp: AC_SRC_OVER as u8,
                    BlendFlags: 0,
                    SourceConstantAlpha: 255,
                    AlphaFormat: AC_SRC_ALPHA as u8,
                }),
                ULW_ALPHA,
            )
        }
    }
}

#[cfg(test)]
mod sample_tests {
    use super::*;
    #[test]
    fn surface_dimensions_are_checked_without_allocating_an_image() {
        for (width, height) in [(0, 2), (2, 0), (u32::MAX, 1), (1, u32::MAX)] {
            assert!(LiveSurface::new(width, height).is_err());
        }
        assert_eq!(LiveSurface::new(3, 2).unwrap().bits().len(), 24);
    }
    #[test]
    fn loupe_clips_edges_and_keeps_negative_desktop_coordinates() {
        let mut image = Image::solid(100, 80, [0; 4]);
        image.left = -100;
        image.top = -80;
        for (x, y, expected) in [
            (50., 40., [-55, -45, -44, -34]),
            (0., 0., [-100, -80, -94, -74]),
            (99., 79., [-6, -6, 0, 0]),
            (-500., 200., [-100, -6, -94, 0]),
        ] {
            let r = sample_bounds(&image, shotoshot_core::Point { x, y });
            assert_eq!([r.left, r.top, r.right, r.bottom], expected);
        }
    }
}
