use windows::{
    Win32::{
        Foundation::{LPARAM, RECT},
        Graphics::Gdi::*,
        UI::WindowsAndMessaging::*,
    },
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

/// Where a capture reads the desktop from. The application only knows the real one; a test
/// paints its own, so its verdict never depends on what the screen shows or on who uses it.
pub(crate) enum Screen {
    Desktop,
    #[cfg(test)]
    Painted {
        surface: LiveSurface,
        left: i32,
        top: i32,
    },
}
impl Screen {
    /// Left, top, width and height in virtual-desktop coordinates.
    fn bounds(&self) -> (i32, i32, i32, i32) {
        match self {
            Self::Desktop => unsafe {
                (
                    GetSystemMetrics(SM_XVIRTUALSCREEN),
                    GetSystemMetrics(SM_YVIRTUALSCREEN),
                    GetSystemMetrics(SM_CXVIRTUALSCREEN),
                    GetSystemMetrics(SM_CYVIRTUALSCREEN),
                )
            },
            #[cfg(test)]
            Self::Painted { surface, left, top } => {
                (*left, *top, surface.width as i32, surface.height as i32)
            }
        }
    }
    /// The context pixels are copied from, addressed in virtual-desktop coordinates.
    /// `desktop` is the screen context the caller already holds.
    fn source(&self, desktop: HDC) -> HDC {
        match self {
            Self::Desktop => desktop,
            #[cfg(test)]
            Self::Painted { surface, .. } => surface.dc(),
        }
    }
    /// A valid live canvas has dimensions and storage, but reads no screen pixels.
    pub fn blank(&self) -> Result<Image> {
        let (left, top, width, height) = self.bounds();
        if width <= 0 || height <= 0 {
            return Err(Error::from_thread());
        }
        let mut image = Image::solid(width as u32, height as u32, [0, 0, 0, 255]);
        image.left = left;
        image.top = top;
        Ok(image)
    }
    pub fn capture(&self) -> Result<Image> {
        // PITFALL: A top-down DIB preserves physical virtual-desktop coordinates, including negatives.
        unsafe {
            let mut image = self.blank()?;
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
                Some(self.source(screen)),
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
}
#[cfg(test)]
impl Screen {
    /// A black desktop of this size whose top-left corner sits at `left`, `top`.
    pub fn painted(left: i32, top: i32, width: u32, height: u32) -> Result<Self> {
        let surface = LiveSurface::new(width, height)?;
        unsafe {
            SetWindowOrgEx(surface.dc(), left, top, None).ok()?;
        }
        Ok(Self::Painted { surface, left, top })
    }
    /// Fills an area given in virtual-desktop coordinates, as a window shown there would.
    pub fn fill(&self, area: windows::Win32::Foundation::RECT, rgb: u32) {
        let Self::Painted { surface, .. } = self else {
            panic!("a test never paints the real desktop");
        };
        unsafe {
            let brush = CreateSolidBrush(windows::Win32::Foundation::COLORREF(rgb));
            FillRect(surface.dc(), &area, brush);
            let _ = DeleteObject(brush.into());
            GdiFlush().unwrap();
        }
    }
}
pub(crate) fn blank_desktop() -> Result<Image> {
    Screen::Desktop.blank()
}
pub fn capture_desktop() -> Result<Image> {
    Screen::Desktop.capture()
}

/// The screen a probe on the real desktop shows its windows on: the rightmost one, never the
/// primary screen while there is another, as Mickaël plays or works on the primary one, in the
/// middle (2026-10-29: the loupe's probe covered every screen during his game).
pub(crate) fn probe_screen() -> RECT {
    let mut screens: Vec<(RECT, bool)> = Vec::new();
    unsafe extern "system" fn each(monitor: HMONITOR, _: HDC, _: *mut RECT, list: LPARAM) -> BOOL {
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        unsafe {
            if GetMonitorInfoW(monitor, &mut info).as_bool() {
                (*(list.0 as *mut Vec<(RECT, bool)>))
                    .push((info.rcMonitor, info.dwFlags & MONITORINFOF_PRIMARY != 0));
            }
        }
        true.into()
    }
    unsafe {
        let _ = EnumDisplayMonitors(
            None,
            None,
            Some(each),
            LPARAM(&mut screens as *mut _ as isize),
        );
    }
    screens
        .iter()
        .filter(|(_, primary)| !primary)
        .max_by_key(|(r, _)| r.left)
        .or_else(|| screens.iter().max_by_key(|(r, _)| r.left))
        .map(|(r, _)| *r)
        .unwrap_or(RECT {
            left: 0,
            top: 0,
            right: unsafe { GetSystemMetrics(SM_CXSCREEN) },
            bottom: unsafe { GetSystemMetrics(SM_CYSCREEN) },
        })
}

/// Copies `src` into `dst` at (`x`, `y`), clipped.
pub(crate) fn blit(dst: &mut Image, src: &Image, x: i32, y: i32) {
    for row in 0..src.height as i32 {
        let ty = y + row;
        if ty < 0 || ty >= dst.height as i32 {
            continue;
        }
        let from = (-x).clamp(0, src.width as i32);
        let to = (dst.width as i32 - x).clamp(0, src.width as i32);
        if to <= from {
            continue;
        }
        let s = (row as usize * src.width as usize + from as usize) * 4;
        let d = (ty as usize * dst.width as usize + (x + from) as usize) * 4;
        let n = (to - from) as usize * 4;
        dst.pixels[d..d + n].copy_from_slice(&src.pixels[s..s + n]);
    }
}

/// The desktop as Explorer composes it, wallpaper and icons and no window: a DWM rendering of
/// the Progman window, drawn over `into` where the two overlap. Fails when Progman is missing
/// or when the rendering comes back entirely black (the sign that nothing was drawn).
pub fn render_desktop(into: &mut Image) -> Result<()> {
    use windows::Win32::{
        Foundation::{E_FAIL, RECT},
        Storage::Xps::{PRINT_WINDOW_FLAGS, PrintWindow},
    };
    unsafe {
        let progman = FindWindowW(w!("Progman"), None)?;
        let mut area = RECT::default();
        GetWindowRect(progman, &mut area)?;
        let (width, height) = (area.right - area.left, area.bottom - area.top);
        if width <= 0 || height <= 0 {
            return Err(Error::new(E_FAIL, "The desktop window has no area"));
        }
        let mut rendering = Image::solid(width as u32, height as u32, [0, 0, 0, 255]);
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
            &rendering.info(),
            DIB_RGB_COLORS,
            &mut bits,
            None,
            0,
        )?;
        surface.previous = SelectObject(surface.memory, surface.bitmap.into());
        // PITFALL: PW_RENDERFULLCONTENT asks DWM for the composed content, children included,
        // and works while other windows cover the desktop; a BitBlt of the screen would copy
        // those windows into the picture.
        if !PrintWindow(
            progman,
            surface.memory,
            PRINT_WINDOW_FLAGS(PW_RENDERFULLCONTENT),
        )
        .as_bool()
        {
            return Err(Error::new(E_FAIL, "The desktop could not be rendered"));
        }
        GdiFlush().ok()?;
        std::ptr::copy_nonoverlapping(
            bits.cast::<u8>(),
            rendering.pixels.as_mut_ptr(),
            rendering.pixels.len(),
        );
        if rendering
            .pixels
            .chunks(4)
            .all(|pixel| pixel[0] | pixel[1] | pixel[2] == 0)
        {
            return Err(Error::new(E_FAIL, "The desktop rendering is empty"));
        }
        for pixel in rendering.pixels.as_chunks_mut::<4>().0 {
            pixel[3] = 255;
        }
        blit(into, &rendering, area.left - into.left, area.top - into.top);
        Ok(())
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
    pub fn refresh(
        &self,
        screen: &Screen,
        image: &mut Image,
        pointer: shotoshot_core::Point,
    ) -> Result<()> {
        unsafe {
            if (
                image.left,
                image.top,
                image.width as i32,
                image.height as i32,
            ) != screen.bounds()
            {
                return Err(crate::failure::visible(
                    shotoshot_text::Key::ErrorDisplayChanged,
                    &[],
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
                Some(screen.source(self.0.surface.screen)),
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
    /// Pixels to write directly, for a surface no GDI or Direct2D drawing touches.
    pub fn bits_mut(&mut self) -> &mut [u8] {
        unsafe {
            std::slice::from_raw_parts_mut(
                self.bits.cast_mut(),
                (self.width * self.height * 4) as usize,
            )
        }
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
    /// Sends only `dirty` of the surface to its layered window, which keeps its place: the
    /// window never moves, so its position and its pixels can never disagree for a frame.
    pub fn present_region(
        &self,
        hwnd: windows::Win32::Foundation::HWND,
        dirty: windows::Win32::Foundation::RECT,
    ) -> Result<()> {
        if dirty.right <= dirty.left || dirty.bottom <= dirty.top {
            return Ok(());
        }
        unsafe {
            GdiFlush().ok()?;
            let size = windows::Win32::Foundation::SIZE {
                cx: self.width as i32,
                cy: self.height as i32,
            };
            let source = windows::Win32::Foundation::POINT { x: 0, y: 0 };
            let blend = BLENDFUNCTION {
                BlendOp: AC_SRC_OVER as u8,
                BlendFlags: 0,
                SourceConstantAlpha: 255,
                AlphaFormat: AC_SRC_ALPHA as u8,
            };
            UpdateLayeredWindowIndirect(
                hwnd,
                &UPDATELAYEREDWINDOWINFO {
                    cbSize: std::mem::size_of::<UPDATELAYEREDWINDOWINFO>() as u32,
                    hdcDst: self.surface.screen,
                    pptDst: std::ptr::null(),
                    psize: &size,
                    hdcSrc: self.surface.memory,
                    pptSrc: &source,
                    crKey: windows::Win32::Foundation::COLORREF(0),
                    pblend: &blend,
                    dwFlags: ULW_ALPHA,
                    prcDirty: &dirty,
                },
            )
            .ok()
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

#[cfg(test)]
mod blit_tests {
    use super::*;
    #[test]
    fn blit_copies_a_picture_clipped_to_the_destination() {
        let mut dst = Image::solid(3, 2, [0, 0, 0, 255]);
        let src = Image::solid(2, 2, [9, 9, 9, 255]);
        blit(&mut dst, &src, 2, 1); // only one pixel is inside
        let painted: Vec<usize> = dst
            .pixels
            .chunks(4)
            .enumerate()
            .filter(|(_, p)| p[0] == 9)
            .map(|(i, _)| i)
            .collect();
        assert_eq!(painted, vec![5], "bottom right pixel only");
        blit(&mut dst, &src, -5, -5); // fully outside: nothing happens
        blit(&mut dst, &src, 3, 0); // fully to the right: nothing happens
        assert_eq!(dst.pixels.iter().filter(|b| **b == 9).count(), 3);
    }
}

#[cfg(test)]
mod desktop_rendering_tests {
    /// Reads the desktop composition only and changes nothing; `check.ps1` runs it on its own.
    #[test]
    #[ignore = "real desktop: reads Explorer's composition of the desktop, run alone by check.ps1"]
    fn real_desktop_rendering_covers_the_frame_without_any_window() {
        let mut frame = super::Screen::Desktop.blank().unwrap();
        super::render_desktop(&mut frame).unwrap();
        assert!(
            frame.pixels.chunks(4).any(|p| p[0] | p[1] | p[2] != 0),
            "something was drawn"
        );
    }
}
