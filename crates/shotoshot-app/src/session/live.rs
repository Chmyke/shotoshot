use super::*;

impl Session {
    pub fn set_live(&mut self, hwnd: HWND, live: bool) -> Result<()> {
        if self.capture.live == live {
            return Ok(());
        }
        unsafe {
            // Own overlay only: keep the desktop visible to capture APIs beneath it.
            SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE)?;
            windows::Win32::Graphics::Dwm::DwmFlush()?;
        }
        let mut renderer = Renderer::with(self.native.renderer.backend(), live)?;
        let surface = if live {
            Some(crate::image::LiveSurface::new(
                self.capture.image.width,
                self.capture.image.height,
            )?)
        } else {
            None
        };
        if !live {
            let image = self.fresh_desktop()?;
            renderer.set_image(&image)?;
            self.capture.image = image;
        }
        unsafe {
            let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
            SetWindowLongPtrW(
                hwnd,
                GWL_EXSTYLE,
                if live {
                    style | WS_EX_LAYERED.0
                } else {
                    style & !WS_EX_LAYERED.0
                } as isize,
            );
        }
        self.native.renderer = renderer;
        self.native.live_surface = surface;
        self.native.back_buffer = None;
        self.capture.live = live;
        Ok(())
    }
    fn fresh_desktop(&self) -> Result<Image> {
        let image = self.native.screen.capture()?;
        if (image.left, image.top, image.width, image.height)
            != (
                self.capture.image.left,
                self.capture.image.top,
                self.capture.image.width,
                self.capture.image.height,
            )
        {
            return Err(Error::new(
                E_FAIL,
                "Display configuration changed. Start a new capture.",
            ));
        }
        Ok(image)
    }
    pub fn refresh_live_sample(&mut self, pointer: Point) -> Result<()> {
        if self.capture.live && self.capture.eyedropper {
            if self.native.live_sample.is_none() {
                self.native.live_sample = Some(crate::image::DesktopSample::new()?);
            }
            if let Some(sample) = &self.native.live_sample {
                sample.refresh(&self.native.screen, &mut self.capture.image, pointer)?;
            }
        }
        Ok(())
    }
    pub fn capture_output(&self) -> Result<Image> {
        if !self.capture.live {
            return self.export();
        }
        self.flatten(&self.fresh_desktop()?)
    }
    pub fn paint_window(&mut self, hwnd: HWND, hdc: HDC) -> Result<()> {
        if let Some(surface) = &self.native.live_surface {
            let memory = surface.dc();
            self.paint(memory)?;
            self.native.live_surface.as_ref().unwrap().present(hwnd)
        } else if self.native.renderer.backend() == Backend::Software {
            // PITFALL: bound to a window DC, BeginDraw first reads the window back. On a
            // 5760×1080 desktop the software renderer then needs 57 ms for its first frame
            // and 19 ms for each next one, against 19 and 12 through a memory surface.
            // The hardware renderer is the opposite: 2 ms a frame straight to the window.
            let (width, height) = (self.capture.image.width, self.capture.image.height);
            let buffer = match self.native.back_buffer.take() {
                Some(buffer) => buffer,
                None => crate::image::LiveSurface::new(width, height)?,
            };
            let painted = self.paint(buffer.dc()).and_then(|_| buffer.copy_to(hdc));
            self.native.back_buffer = Some(buffer);
            painted
        } else {
            self.paint(hdc)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    /// The app calls `accelerate` when the window message arrives; a test polls instead.
    fn wait_for_acceleration(s: &mut Session) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while s.native.renderer.backend() == Backend::Software {
            assert!(
                std::time::Instant::now() < deadline,
                "the hardware renderer never arrived"
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
            s.accelerate().unwrap();
        }
    }
    #[test]
    fn the_software_first_frame_matches_the_hardware_frame_that_replaces_it() {
        let _com = ComApartment::new().unwrap();
        let backdrop = [70, 110, 160, 255];
        let mut s = Session::immediate(Image::solid(900, 600, backdrop)).unwrap();
        s.capture.region = Rect::new(Point { x: 100., y: 80. }, Point { x: 700., y: 500. });
        s.capture.selecting = false;
        s.capture.editor.tool = Tool::Pen;
        s.capture
            .editor
            .begin(Point { x: 150., y: 150. }, Modifiers::default());
        s.capture.editor.update(Point { x: 400., y: 300. });
        s.capture.editor.finish();
        let frame = |s: &mut Session| {
            paint_image(900, 600, |dc| s.paint_window(HWND::default(), dc)).unwrap()
        };
        assert_eq!(s.native.renderer.backend(), Backend::Software);
        let software = frame(&mut s);
        let at = |image: &Image, x: usize, y: usize| {
            let i = (y * 900 + x) * 4;
            [image.pixels[i], image.pixels[i + 1], image.pixels[i + 2]]
        };
        assert_eq!(
            at(&software, 600, 450),
            [backdrop[0], backdrop[1], backdrop[2]],
            "the capture is missing from the first frame"
        );
        assert_ne!(
            at(&software, 50, 40),
            at(&software, 600, 450),
            "the first frame has no shade around the region"
        );
        assert_ne!(
            at(&software, 275, 225),
            at(&software, 600, 450),
            "the first frame has no annotation"
        );
        s.prepare_acceleration(HWND::default(), WM_NULL).unwrap();
        // Still being built on the other thread, or already there: neither may lose it.
        s.accelerate().unwrap();
        wait_for_acceleration(&mut s);
        assert!(
            s.native.back_buffer.is_none(),
            "the memory surface was kept"
        );
        let hardware = frame(&mut s);
        // Both rasterizers may round antialiased edges differently, never whole areas.
        let different = software
            .pixels
            .as_chunks::<4>()
            .0
            .iter()
            .zip(hardware.pixels.as_chunks::<4>().0)
            .filter(|(a, b)| (0..3).any(|c| a[c].abs_diff(b[c]) > 48))
            .count();
        assert!(
            different * 500 < 900 * 600,
            "{different} pixels changed when the hardware renderer took over"
        );
    }
    #[test]
    fn a_renderer_built_before_a_live_toggle_is_replaced_by_a_transparent_one() {
        let _com = ComApartment::new().unwrap();
        unsafe {
            let overlay = crate::resources::OwnedWindow(
                CreateWindowExW(
                    WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                    w!("STATIC"),
                    w!("Shotoshot acceleration test overlay"),
                    WS_POPUP,
                    0,
                    0,
                    300,
                    200,
                    None,
                    None,
                    None,
                    None,
                )
                .unwrap(),
            );
            let mut s = Session::immediate(Image::solid(300, 200, [0, 0, 0, 255])).unwrap();
            s.prepare_acceleration(overlay.0, WM_NULL).unwrap();
            s.set_live(overlay.0, true).unwrap();
            assert_eq!(
                s.native.renderer.backend(),
                Backend::Software,
                "toggling live must not wait for a Direct3D device"
            );
            wait_for_acceleration(&mut s);
            assert!(s.native.renderer.transparent());
            s.paint_window(overlay.0, HDC::default()).unwrap();
        }
    }
    #[test]
    fn live_capture_reads_current_desktop_and_freeze_latches_it() {
        let _com = ComApartment::new().unwrap();
        // Painted, with its corner left of and above the primary screen as a second monitor
        // gives: what the real screen shows, and who uses it, cannot change the verdict.
        let screen = crate::image::Screen::painted(-200, -100, 640, 420).unwrap();
        let paint = |screen: &crate::image::Screen, rgb: u32| {
            screen.fill(
                RECT {
                    left: 100,
                    top: 100,
                    right: 400,
                    bottom: 320,
                },
                rgb,
            )
        };
        paint(&screen, 0x00332211);
        let image = screen.capture().unwrap();
        let index =
            (((120 - image.top) as u32 * image.width + (120 - image.left) as u32) * 4) as usize;
        assert_eq!(
            &image.pixels[index..index + 3],
            &[0x33, 0x22, 0x11],
            "the capture missed what the desktop shows"
        );
        let mut s = Session::new(image).unwrap();
        s.native.screen = screen;
        let paint = |s: &Session, rgb: u32| paint(&s.native.screen, rgb);
        unsafe {
            // Never shown: the session only needs a window to exclude, layer and present to.
            let overlay = crate::resources::OwnedWindow(
                CreateWindowExW(
                    WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                    w!("STATIC"),
                    w!("Shotoshot live test overlay"),
                    WS_POPUP,
                    -32000,
                    -32000,
                    s.capture.image.width as i32,
                    s.capture.image.height as i32,
                    None,
                    None,
                    None,
                    None,
                )
                .unwrap(),
            );
            s.capture.region = Rect::new(
                Point {
                    x: 120. - s.capture.image.left as f64,
                    y: 120. - s.capture.image.top as f64,
                },
                Point {
                    x: 220. - s.capture.image.left as f64,
                    y: 200. - s.capture.image.top as f64,
                },
            );
            s.capture.selecting = false;
            s.capture.toolbar_visible = false;
            s.set_live(overlay.0, true).unwrap();
            let mut affinity = 0;
            GetWindowDisplayAffinity(overlay.0, &mut affinity).unwrap();
            assert_eq!(
                affinity, WDA_EXCLUDEFROMCAPTURE.0,
                "Windows must be told to keep the overlay out of what live capture reads"
            );
            s.paint_window(overlay.0, HDC::default()).unwrap();
            let origin = s.capture.region.unwrap().min;
            let alpha = s
                .native
                .live_surface
                .as_ref()
                .unwrap()
                .pixel(origin.x as u32 + 30, origin.y as u32 + 30)[3];
            assert_eq!(
                alpha, 1,
                "live canvas must reveal the desktop and retain mouse input"
            );
            let first = s.capture_output().unwrap();
            assert_eq!(&first.pixels[0..3], &[0x33, 0x22, 0x11]);
            paint(&s, 0x00776655);
            let second = s.capture_output().unwrap();
            assert_eq!(
                &second.pixels[0..3],
                &[0x77, 0x66, 0x55],
                "live output was frozen or contained the overlay"
            );
            s.capture.eyedropper = true;
            s.capture.pointer = origin;
            s.capture.image.pixels[..4].copy_from_slice(&[3, 5, 7, 255]);
            s.refresh_live_sample(origin).unwrap();
            assert_eq!(s.capture.sample_color(origin), [0x55, 0x66, 0x77]);
            assert_eq!(
                &s.capture.image.pixels[..4],
                &[3, 5, 7, 255],
                "the eyedropper must refresh only its local pixels"
            );
            paint(&s, 0x00443322);
            let previous_color = s.capture.editor.style.color;
            let clicked = origin + Point { x: 40., y: 20. };
            assert_eq!(
                s.mouse_down(overlay.0, clicked).unwrap(),
                Some(Action::CopyColor([0x22, 0x33, 0x44])),
                "a click at a new position must read its fresh pixel without a preceding move"
            );
            paint(&s, 0x00776655);
            s.capture.editor.style.color = previous_color;
            s.capture.eyedropper = false;
            s.capture.editor.tool = Tool::Pen;
            s.capture
                .editor
                .begin(origin + Point { x: 10., y: 30. }, Modifiers::default());
            s.capture.editor.update(origin + Point { x: 80., y: 30. });
            s.capture.editor.finish();
            s.paint_window(overlay.0, HDC::default()).unwrap();
            let annotated = s.capture_output().unwrap();
            assert!(
                annotated.pixels[(30 * 100 + 40) * 4 + 2] > 160,
                "annotations missing from live export"
            );
            assert_eq!(&annotated.pixels[0..3], &[0x77, 0x66, 0x55]);
            s.set_live(overlay.0, false).unwrap();
            paint(&s, 0x00bbaa99);
            assert_eq!(
                &s.capture_output().unwrap().pixels[0..3],
                &[0x77, 0x66, 0x55],
                "freeze did not latch"
            );
        }
    }
}
