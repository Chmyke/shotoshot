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
                    scroll: Default::default(),
                })
            };
            self.dimensions = Some(DimensionsInput {
                width: make(region.size().x)?,
                height: make(region.size().y)?,
                error: None,
            });
            self.layout_badge(capture)?;
            let areas = capture.dimension_field_bounds().expect("open fields");
            let fields = self.dimensions.as_mut().expect("active fields");
            for (field, area) in [&fields.width, &fields.height].into_iter().zip(areas) {
                // Only where the text starts matters: the capture window draws and
                // hit-tests the fields, and the controls scroll their text themselves.
                SetWindowPos(
                    field.window,
                    None,
                    area.min.x as i32,
                    area.min.y as i32,
                    area.size().x as i32,
                    area.size().y as i32,
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
    /// Measures what the size badge shows, so it is as wide as its digits, and moves the
    /// open fields where it puts them; before each frame and when the fields open.
    pub(super) fn layout_badge(&mut self, capture: &mut Capture) -> Result<()> {
        let texts = self.dimensions.as_ref().map(|fields| {
            [&fields.width, &fields.height]
                .map(|field| String::from_utf16_lossy(&super::text::value(field.window)))
        });
        let renderer = &self.renderer;
        capture.measure_badge(
            texts
                .as_ref()
                .map(|[width, height]| [width.as_str(), height.as_str()]),
            |text, size| {
                renderer
                    .measure_text(text, size as f32)
                    .map(|extent| extent.x)
            },
        )?;
        if let (Some(fields), Some([width, height])) =
            (&mut self.dimensions, capture.dimension_field_bounds())
        {
            fields.width.position = width.min;
            fields.height.position = height.min;
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
        let (Some(bounds), Some([width, height])) = (
            capture.dimensions_bounds(),
            capture.dimension_field_bounds(),
        ) else {
            return Ok(false);
        };
        let Some(fields) = &mut self.dimensions else {
            return Ok(false);
        };
        if !bounds.contains(point) {
            return Ok(false);
        }
        let field = if point.x < (width.max.x + height.min.x) / 2. {
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
        let (Some(areas), Some(fields)) = (capture.dimension_field_bounds(), &self.dimensions)
        else {
            return Ok(());
        };
        let target = &self.renderer.dc;
        for (field, clip) in [&fields.width, &fields.height].into_iter().zip(areas) {
            unsafe { target.PushAxisAlignedClip(&rect(clip), D2D1_ANTIALIAS_MODE_ALIASED) };
            let focused = unsafe { GetFocus() } == field.window;
            let view = Some(clip.size().x as f32);
            let result = self.draw_inline(field, focused, view, capture.ui_scale());
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
            let badge_before = session.capture.dimensions_bounds().unwrap();
            session
                .native
                .begin_dimensions(&mut session.capture, window.0)?;
            // The badge keeps its corner over the region: only its left edge follows the text.
            let badge_open = session.capture.dimensions_bounds().unwrap();
            assert_eq!(
                (badge_before.max, badge_before.min.y),
                (badge_open.max, badge_open.min.y)
            );
            unsafe {
                SetWindowTextW(
                    session.native.dimensions.as_ref().unwrap().width.window,
                    w!("400+20"),
                )?;
            }
            let image = crate::paint_image(500, 300, |dc| session.paint(dc))?;
            let bounds = session.capture.dimensions_bounds().unwrap();
            assert!(
                bounds.min.x < badge_open.min.x,
                "the badge grew with the text"
            );
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
                for x in (bounds.min.x as usize + 4)..(bounds.max.x as usize - 4) {
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

    /// Left, top, right and bottom of the pixels two paints of the same size disagree on.
    fn difference(a: &Image, b: &Image) -> Option<[usize; 4]> {
        let width = a.width as usize;
        let pixels = a.pixels.as_chunks::<4>().0.iter();
        let mut bounds: Option<[usize; 4]> = None;
        for (index, (one, other)) in pixels.zip(b.pixels.as_chunks::<4>().0).enumerate() {
            if one != other {
                let (x, y) = (index % width, index / width);
                let b = bounds.get_or_insert([x, y, x, y]);
                *b = [b[0].min(x), b[1].min(y), b[2].max(x), b[3].max(y)];
            }
        }
        bounds
    }

    #[test]
    fn an_expression_wider_than_its_field_follows_the_caret() -> Result<()> {
        let _com = ComApartment::new()?;
        // The field grows with its screen, and so does the caret line.
        for scale in [1., 2.5] {
            follows_the_caret(scale)?;
        }
        Ok(())
    }

    fn follows_the_caret(scale: f64) -> Result<()> {
        // A screen so narrow that the badge, as wide as it may be, cuts the expression.
        let size = ((130. * scale) as u32, 500);
        let window = crate::resources::OwnedWindow(unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                w!("STATIC"),
                w!(""),
                WS_POPUP,
                0,
                0,
                size.0 as i32,
                size.1 as i32,
                None,
                None,
                None,
                None,
            )?
        });
        // The click below reads the modifier keys: none is held, whatever the user is typing.
        unsafe { SetKeyboardState(&[0; 256])? };
        let mut session = Session::new(Image::solid(size.0, size.1, [0, 0, 0, 255]))?;
        session.native.caret = super::super::text::CaretBlink::steady();
        let screen = Rect {
            min: Point::default(),
            max: Point {
                x: size.0 as f64,
                y: size.1 as f64,
            },
        };
        session.capture.monitors = vec![shotoshot_ui::Monitor {
            bounds: screen,
            scale,
        }];
        let region_right = size.0 as f64 - 10.;
        session.capture.region = Rect::new(
            Point { x: 10., y: 200. },
            Point {
                x: region_right,
                y: 460.,
            },
        );
        session.capture.selecting = false;
        session
            .native
            .begin_dimensions(&mut session.capture, window.0)?;
        let field = session.native.dimensions.as_ref().unwrap().width.window;
        let send = |message: u32, value: usize| unsafe {
            SendMessageW(field, message, Some(WPARAM(value)), None);
        };
        let paint =
            |session: &mut Session| crate::paint_image(size.0, size.1, |dc| session.paint(dc));

        // The width opens selected: the expression replaces it, about three fields wide.
        let expression = "100+100+100+100";
        for unit in expression.encode_utf16() {
            send(WM_CHAR, unit as usize);
        }
        paint(&mut session)?;
        // The field on screen, in whole pixels: left, top, right and bottom.
        let [area, _] = session.capture.dimension_field_bounds().unwrap();
        let (left, top) = (area.min.x as usize, area.min.y as usize);
        let (right, bottom) = (area.max.x as usize - 1, area.max.y as usize - 1);
        let text = session
            .native
            .renderer
            .measure_text(expression, (14. * scale) as f32)?;
        assert!(
            text.x > 2. * area.size().x,
            "the expression overflows, at {scale}"
        );
        let inside = |[l, t, r, b]: [usize; 4]| l >= left && t >= top && r <= right && b <= bottom;
        // The caret is what the field paints with the focus and not without it.
        let caret = |session: &mut Session, after: &str| -> Result<[usize; 4]> {
            let focused = paint(session)?;
            unsafe {
                let _ = SetFocus(None);
            }
            let idle = paint(session)?;
            unsafe {
                let _ = SetFocus(Some(field));
            }
            let caret = difference(&focused, &idle)
                .unwrap_or_else(|| panic!("no caret in sight after {after}, at {scale}"));
            assert!(
                inside(caret),
                "caret {caret:?} outside {left}..={right} after {after}, at {scale}"
            );
            Ok(caret)
        };
        let y = (top + bottom) as f64 / 2.;
        let click = |session: &mut Session, x: usize| -> Result<u32> {
            let point = Point { x: x as f64, y };
            assert!(session.native.dimension_pointer_down(
                &mut session.capture,
                window.0,
                point
            )?);
            session.native.dimension_pointer_up();
            let (start, end) = super::super::text::selection(field);
            assert_eq!(start, end);
            Ok(end)
        };

        let at_end = caret(&mut session, "typing")?;
        // The end of the expression is in view: changing its last digit changes the field.
        let typed = paint(&mut session)?;
        send(WM_CHAR, 8);
        send(WM_CHAR, '7' as usize);
        let last_digit = difference(&typed, &paint(&mut session)?)
            .unwrap_or_else(|| panic!("the last digit is out of view, at {scale}"));
        assert!(inside(last_digit), "{last_digit:?} at {scale}");

        // A click lands on the character shown under it, not on the one the unscrolled
        // text would have there.
        let length = expression.len() as u32;
        assert!(click(&mut session, right - 1)? >= length - 1, "at {scale}");
        let first_shown = click(&mut session, left + 1)?;
        assert!(
            (6..length - 1).contains(&first_shown),
            "{first_shown} at {scale}"
        );

        // Home brings the start back; End returns to the last digit.
        const HOME: usize = 0x24;
        const END: usize = 0x23;
        const LEFT: usize = 0x25;
        send(WM_KEYDOWN, HOME);
        let at_home = caret(&mut session, "Home")?;
        // The caret line starts half its width after the first glyph's edge.
        let gap = shotoshot_ui::caret::width(14. * scale, scale) / 2.;
        assert!(
            at_home[0] <= left + gap.ceil() as usize,
            "{at_home:?} at {scale}"
        );
        assert_eq!(click(&mut session, left + 1)?, 0, "at {scale}");
        send(WM_KEYDOWN, END);
        let back = caret(&mut session, "End")?;
        assert_eq!(back, at_end, "at {scale}");
        // The text only moves when the caret would leave: Left steps over one digit.
        send(WM_KEYDOWN, LEFT);
        let stepped = caret(&mut session, "Left")?;
        let step = (back[0] as f64 - stepped[0] as f64) / scale;
        assert!((5. ..=10.).contains(&step), "{step} at {scale}");

        // Dragging past the left edge selects towards the start, and the text follows.
        let from = Point {
            x: (right - 1) as f64,
            y,
        };
        session
            .native
            .dimension_pointer_down(&mut session.capture, window.0, from)?;
        paint(&mut session)?;
        let beyond = Point {
            x: left as f64 - 60. * scale,
            y,
        };
        session.native.dimension_pointer_move(beyond)?;
        paint(&mut session)?;
        session.native.dimension_pointer_up();
        let (start, end) = super::super::text::selection(field);
        assert!(start < 6 && end >= length - 1, "{start}..{end} at {scale}");
        assert!(
            click(&mut session, left + 1)?.abs_diff(start) <= 1,
            "at {scale}"
        );
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
