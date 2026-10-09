//! ROLE: Inline crop-size fields; native text editing and validated arithmetic.
//! OWNER: Session owns both child controls and their font until apply/cancel.
use super::*;
use windows::Win32::UI::Controls::{EM_SETLIMITTEXT, EM_SETSEL};

pub(super) struct DimensionsInput {
    pub width: TextInput,
    pub height: TextInput,
    error: Option<&'static str>,
}

impl Native {
    pub fn has_dimensions_input(&self) -> bool {
        self.dimensions.is_some()
    }
    pub fn is_dimension_window(&self, hwnd: HWND) -> bool {
        self.dimensions
            .as_ref()
            .is_some_and(|d| d.width.window == hwnd || d.height.window == hwnd)
    }
    pub(super) fn begin_dimensions(&mut self, capture: &mut Capture, hwnd: HWND) -> Result<()> {
        let Some(region) = capture.region else {
            return Ok(());
        };
        self.finish_text(capture, hwnd, true);
        if self.text.is_some() {
            return Ok(());
        }
        capture.editor.cancel();
        self.dimensions = None;
        // Field geometry is in logical pixels, like the badge that hosts it.
        let scale = capture.ui_scale();
        let font_size = (14. * scale) as f32;
        unsafe {
            let make = |value: f64| -> Result<TextInput> {
                let window = CreateWindowExW(
                    WS_EX_TRANSPARENT,
                    w!("EDIT"),
                    &HSTRING::from(format!("{value:.0}")),
                    WS_CHILD | WS_VISIBLE | WINDOW_STYLE(ES_AUTOHSCROLL as u32),
                    0,
                    0,
                    1,
                    1,
                    Some(hwnd),
                    None,
                    None,
                    None,
                )?;
                let font = match super::text::configure_input(window, font_size, Face::Interface) {
                    Ok(font) => font,
                    Err(error) => {
                        let _ = DestroyWindow(window);
                        return Err(error);
                    }
                };
                SendMessageW(window, EM_SETLIMITTEXT, Some(WPARAM(128)), None);
                Ok(TextInput {
                    window,
                    position: Point::default(),
                    object: None,
                    size: font_size,
                    face: Face::Interface,
                    color: [240, 240, 240],
                    font,
                    rotation: 0.,
                    drag_anchor: None,
                })
            };
            self.dimensions = Some(DimensionsInput {
                width: make(region.size().x)?,
                height: make(region.size().y)?,
                error: None,
            });
            let bounds = capture.dimensions_bounds().expect("capture region");
            let field_width = ((bounds.size().x - 22. * scale) / 2.).max(1.) as i32;
            let fields = self.dimensions.as_mut().expect("active fields");
            for (index, field) in [&mut fields.width, &mut fields.height]
                .into_iter()
                .enumerate()
            {
                field.position = bounds.min
                    + Point {
                        x: 4. * scale + index as f64 * (field_width as f64 + 14. * scale),
                        y: 3. * scale,
                    };
                SetWindowPos(
                    field.window,
                    None,
                    field.position.x as i32,
                    field.position.y as i32,
                    field_width,
                    (20. * scale) as i32,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                )?;
            }
            let _ = SetFocus(Some(fields.width.window));
            SendMessageW(
                fields.width.window,
                EM_SETSEL,
                Some(WPARAM(0)),
                Some(LPARAM(-1)),
            );
        }
        Ok(())
    }
    pub fn finish_dimensions(&mut self, capture: &mut Capture, hwnd: HWND, commit: bool) -> bool {
        let Some(input) = &self.dimensions else {
            return true;
        };
        if commit {
            let read = |window| unsafe {
                let mut buffer = [0u16; 129];
                let length = GetWindowTextW(window, &mut buffer).max(0) as usize;
                String::from_utf16_lossy(&buffer[..length])
            };
            let Some(region) = capture.region else {
                return false;
            };
            match shotoshot_ui::crop::resize_region(
                region,
                &read(input.width.window),
                &read(input.height.window),
                capture.image.width,
                capture.image.height,
            ) {
                Ok(region) => capture.region = Some(region),
                Err(error) => {
                    self.dimensions.as_mut().expect("active fields").error = Some(error);
                    self.focus_dimensions();
                    return false;
                }
            }
        }
        self.dimensions = None;
        unsafe {
            let _ = SetFocus(Some(hwnd));
        }
        true
    }
    pub fn dimension_command(&mut self, capture: &mut Capture, hwnd: HWND, key: u32) -> bool {
        if self.dimensions.is_none() {
            return false;
        }
        match key {
            13 => {
                self.finish_dimensions(capture, hwnd, true);
                true
            }
            27 => {
                self.finish_dimensions(capture, hwnd, false);
                true
            }
            9 => {
                let input = self.dimensions.as_ref().expect("active fields");
                unsafe {
                    let next = if GetFocus() == input.width.window {
                        input.height.window
                    } else {
                        input.width.window
                    };
                    let _ = SetFocus(Some(next));
                    SendMessageW(next, EM_SETSEL, Some(WPARAM(0)), Some(LPARAM(-1)));
                }
                true
            }
            _ => false,
        }
    }
    pub(super) fn focus_dimensions(&self) {
        if let Some(input) = &self.dimensions {
            unsafe {
                if !self.is_dimension_window(GetFocus()) {
                    let _ = SetFocus(Some(input.width.window));
                }
            }
        }
    }
    pub(super) fn dimension_pointer_down(
        &mut self,
        capture: &mut Capture,
        hwnd: HWND,
        point: Point,
    ) -> Result<bool> {
        let Some(bounds) = capture.dimensions_bounds() else {
            return Ok(false);
        };
        let Some(fields) = &mut self.dimensions else {
            return Ok(false);
        };
        if !bounds.contains(point) {
            return Ok(false);
        }
        let field = if point.x < bounds.center().x {
            &mut fields.width
        } else {
            &mut fields.height
        };
        let (offset, _) = super::text::hit(field, point)?;
        let anchor = if modifiers().shift {
            super::text::selection(field.window).0
        } else {
            offset
        };
        field.drag_anchor = Some(anchor);
        fields.error = None;
        unsafe {
            let _ = SetFocus(Some(field.window));
            SendMessageW(
                field.window,
                EM_SETSEL,
                Some(WPARAM(anchor as usize)),
                Some(LPARAM(offset as isize)),
            );
            SetCapture(hwnd);
        }
        Ok(true)
    }
    pub(super) fn dimension_pointer_move(&mut self, point: Point) -> Result<()> {
        if let Some(fields) = &self.dimensions {
            for field in [&fields.width, &fields.height] {
                if let Some(anchor) = field.drag_anchor {
                    let (offset, _) = super::text::hit(field, point)?;
                    unsafe {
                        SendMessageW(
                            field.window,
                            EM_SETSEL,
                            Some(WPARAM(anchor as usize)),
                            Some(LPARAM(offset as isize)),
                        );
                    }
                }
            }
        }
        Ok(())
    }
    pub(super) fn dimension_pointer_up(&mut self) -> bool {
        let Some(fields) = &mut self.dimensions else {
            return false;
        };
        let captured =
            fields.width.drag_anchor.take().is_some() | fields.height.drag_anchor.take().is_some();
        if captured {
            unsafe {
                let _ = ReleaseCapture();
            }
        }
        true
    }
    /// The portable badge, then the native fields over it.
    pub(super) fn draw_dimensions(
        &self,
        capture: &Capture,
        canvas: &mut crate::render::D2dCanvas,
    ) -> Result<()> {
        let error = self.dimensions.as_ref().and_then(|d| d.error);
        capture.draw_dimensions(canvas, self.dimensions.is_some(), error)?;
        let (Some(bounds), Some(fields)) = (capture.dimensions_bounds(), &self.dimensions) else {
            return Ok(());
        };
        let target = &self.renderer.dc;
        let scale = capture.ui_scale();
        let field_width = ((bounds.size().x - 22. * scale) / 2.).max(1.);
        for field in [&fields.width, &fields.height] {
            let clip = Rect {
                min: field.position,
                max: field.position
                    + Point {
                        x: field_width,
                        y: 20. * scale,
                    },
            };
            unsafe { target.PushAxisAlignedClip(&rect(clip), D2D1_ANTIALIAS_MODE_ALIASED) };
            let result = self.draw_inline(field, unsafe { GetFocus() } == field.window);
            unsafe { target.PopAxisAlignedClip() };
            result?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::*;

    #[test]
    fn dimension_text_is_rendered_into_frozen_and_live_surfaces() -> Result<()> {
        let _com = ComApartment::new()?;
        let window = crate::resources::OwnedWindow(unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                w!("STATIC"),
                w!(""),
                WS_POPUP,
                0,
                0,
                500,
                300,
                None,
                None,
                None,
                None,
            )?
        });
        for live in [false, true] {
            let mut session = Session::new(Image::solid(500, 300, [0, 0, 0, 255]))?;
            session.capture.region =
                Rect::new(Point { x: 20., y: 70. }, Point { x: 450., y: 250. });
            session.capture.selecting = false;
            if live {
                session.native.renderer = Renderer::with_transparency(true)?;
            }
            let baseline = crate::paint_image(500, 300, |dc| session.paint(dc))?;
            if live {
                session.native.renderer = Renderer::with_transparency(true)?;
            }
            let badge_before = session.capture.dimensions_bounds();
            session
                .native
                .begin_dimensions(&mut session.capture, window.0)?;
            assert_eq!(badge_before, session.capture.dimensions_bounds());
            unsafe {
                SetWindowTextW(
                    session.native.dimensions.as_ref().unwrap().width.window,
                    w!("400+20"),
                )?;
            }
            let bounds = session.capture.dimensions_bounds().unwrap();
            let image = crate::paint_image(500, 300, |dc| session.paint(dc))?;
            for (x, y) in [(20, 160), (100, 240)] {
                let index = (y * 500 + x) * 4;
                assert_eq!(
                    &image.pixels[index..index + 3],
                    &baseline.pixels[index..index + 3],
                    "crop handle / toolbar must survive dimension rendering (live={live}, x={x}, y={y})"
                );
                assert!(baseline.pixels[index..index + 3].iter().any(|c| *c > 10));
            }
            let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
                "../../target/shotoshot-dimensions-{}.png",
                if live { "live" } else { "frozen" }
            ));
            crate::save_png(&image, &path)?;
            assert_eq!(crate::load_png(&path)?.pixels, image.pixels);
            let mut ink = 0;
            for y in (bounds.min.y as usize + 3)..(bounds.min.y as usize + 23) {
                for x in (bounds.min.x as usize + 4)..(bounds.min.x as usize + 95) {
                    let pixel = (y * 500 + x) * 4;
                    if image.pixels[pixel..pixel + 3].iter().all(|c| *c > 160) {
                        ink += 1;
                    }
                }
            }
            assert!(
                ink > 20,
                "dimension digits must remain in the overlay surface (live={live}, ink={ink})"
            );
        }
        Ok(())
    }

    #[test]
    fn dimension_fields_apply_expressions_and_preserve_annotations() -> Result<()> {
        let _com = ComApartment::new()?;
        let window = crate::resources::OwnedWindow(unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                w!("STATIC"),
                w!(""),
                WS_POPUP,
                0,
                0,
                500,
                300,
                None,
                None,
                None,
                None,
            )?
        });
        let mut session = Session::new(Image::solid(500, 300, [0, 0, 0, 255]))?;
        session.capture.selecting = false;
        session.capture.region = Rect::new(Point { x: 20., y: 30. }, Point { x: 100., y: 100. });
        session
            .capture
            .editor
            .add_text(Point { x: 40., y: 50. }, "Keep me".into());
        let before = session.capture.editor.objects().to_vec();
        session
            .native
            .begin_dimensions(&mut session.capture, window.0)?;
        let fields = session.native.dimensions.as_ref().unwrap();
        unsafe {
            SetWindowTextW(fields.width.window, w!("100+20"))?;
            SetWindowTextW(fields.height.window, w!("160/2"))?;
        }
        assert!(
            session
                .native
                .finish_dimensions(&mut session.capture, window.0, true)
        );
        assert_eq!(
            session.capture.region.unwrap().max,
            Point { x: 140., y: 110. }
        );
        assert_eq!(session.capture.editor.objects(), before);
        session
            .native
            .begin_dimensions(&mut session.capture, window.0)?;
        unsafe {
            SetWindowTextW(
                session.native.dimensions.as_ref().unwrap().width.window,
                w!("1/0"),
            )?;
        }
        assert!(
            !session
                .native
                .finish_dimensions(&mut session.capture, window.0, true)
        );
        assert!(session.native.dimensions.is_some());
        assert_eq!(
            session.capture.region.unwrap().max,
            Point { x: 140., y: 110. }
        );
        assert!(
            session
                .native
                .finish_dimensions(&mut session.capture, window.0, false)
        );
        assert!(session.native.dimensions.is_none());
        Ok(())
    }
}
