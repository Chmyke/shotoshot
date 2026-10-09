//! ROLE: Native editing semantics with DirectWrite presentation on the capture.
//! OWNER: Session owns the EDIT window and its font until commit or cancellation.
//! INVARIANTS: The native control never paints a surface; UTF-16 offsets stay native.
use super::*;
use crate::render::vector;
use windows::Win32::{
    Graphics::{Direct2D::Common::*, DirectWrite::*},
    UI::{
        Controls::*,
        Shell::{DefSubclassProc, SetWindowSubclass},
    },
};

impl Drop for TextInput {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyWindow(self.window);
            let _ = DeleteObject(self.font.into());
        }
    }
}

// PITFALL: Win32 invokes this on the UI thread; no Session pointer crosses FFI.
unsafe extern "system" fn input_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _: usize,
    _: usize,
) -> LRESULT {
    unsafe {
        match message {
            WM_NCHITTEST => return LRESULT(HTTRANSPARENT as isize),
            WM_ERASEBKGND => return LRESULT(1),
            WM_PAINT => {
                let _ = ValidateRect(Some(hwnd), None);
                return LRESULT(0);
            }
            _ => {}
        }
        let result = DefSubclassProc(hwnd, message, wparam, lparam);
        if message == WM_SETFOCUS {
            // Hide once per creation of the native caret, not once per keystroke.
            let _ = HideCaret(Some(hwnd));
        }
        if matches!(
            message,
            WM_CHAR
                | WM_KEYDOWN
                | WM_KEYUP
                | WM_PASTE
                | WM_CUT
                | WM_CLEAR
                | WM_UNDO
                | WM_SETTEXT
                | EM_SETSEL
                | WM_IME_COMPOSITION
                | WM_SETFOCUS
                | WM_KILLFOCUS
        ) && let Ok(parent) = GetParent(hwnd)
        {
            let _ = InvalidateRect(Some(parent), None, false);
        }
        result
    }
}

pub(super) fn configure_input(hwnd: HWND, size: f32) -> Result<HFONT> {
    unsafe {
        // EDIT can draw synchronously during input, outside WM_PAINT. An empty
        // window region clips those GDI paths too, without disabling native focus.
        let region = CreateRectRgn(0, 0, 0, 0);
        if region.0.is_null() {
            return Err(Error::from_thread());
        }
        if SetWindowRgn(hwnd, Some(region), false) == 0 {
            let _ = DeleteObject(region.into());
            return Err(Error::from_thread());
        }
        // Windows owns the region after successful SetWindowRgn.
        if !SetWindowSubclass(hwnd, Some(input_proc), 1, 0).as_bool() {
            return Err(Error::from_thread());
        }
        let font = CreateFontW(
            -(size.round() as i32),
            0,
            0,
            0,
            FW_NORMAL.0 as i32,
            0,
            0,
            0,
            DEFAULT_CHARSET,
            OUT_DEFAULT_PRECIS,
            CLIP_DEFAULT_PRECIS,
            CLEARTYPE_QUALITY,
            0,
            w!("Segoe UI"),
        );
        if font.0.is_null() {
            return Err(Error::from_thread());
        }
        SendMessageW(
            hwnd,
            WM_SETFONT,
            Some(WPARAM(font.0 as usize)),
            Some(LPARAM(0)),
        );
        SendMessageW(hwnd, EM_SETLIMITTEXT, Some(WPARAM(0)), None);
        SendMessageW(
            hwnd,
            EM_SETMARGINS,
            Some(WPARAM((EC_LEFTMARGIN | EC_RIGHTMARGIN) as usize)),
            Some(LPARAM(0)),
        );
        let end = GetWindowTextLengthW(hwnd).max(0);
        SendMessageW(
            hwnd,
            EM_SETSEL,
            Some(WPARAM(end as usize)),
            Some(LPARAM(end as isize)),
        );
        Ok(font)
    }
}

fn value(hwnd: HWND) -> Vec<u16> {
    unsafe {
        let mut text = vec![0; GetWindowTextLengthW(hwnd).max(0) as usize + 1];
        let length = GetWindowTextW(hwnd, &mut text).max(0) as usize;
        text.truncate(length);
        text
    }
}

pub(super) fn selection(hwnd: HWND) -> (u32, u32) {
    let (mut start, mut end) = (0u32, 0u32);
    unsafe {
        SendMessageW(
            hwnd,
            EM_GETSEL,
            Some(WPARAM((&mut start as *mut u32) as usize)),
            Some(LPARAM((&mut end as *mut u32) as isize)),
        );
    }
    (start, end)
}

fn layout(text: &[u16], size: f32) -> Result<IDWriteTextLayout> {
    unsafe {
        let factory: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
        let format = factory.CreateTextFormat(
            w!("Segoe UI"),
            None,
            DWRITE_FONT_WEIGHT_NORMAL,
            DWRITE_FONT_STYLE_NORMAL,
            DWRITE_FONT_STRETCH_NORMAL,
            size,
            w!("en-US"),
        )?;
        format.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)?;
        factory.CreateTextLayout(text, &format, 1_000_000., 1_000_000.)
    }
}

pub(super) fn hit(input: &TextInput, point: Point) -> Result<(u32, bool)> {
    let layout = layout(&value(input.window), input.size)?;
    let mut point = rotate_point(point, text_center(input, &layout)?, -input.rotation);
    point.x += horizontal_scroll(input, &layout)? as f64;
    let (mut trailing, mut inside) = (BOOL(0), BOOL(0));
    let mut metrics = DWRITE_HIT_TEST_METRICS::default();
    unsafe {
        layout.HitTestPoint(
            (point.x - input.position.x) as f32,
            (point.y - input.position.y) as f32,
            &mut trailing,
            &mut inside,
            &mut metrics,
        )?;
    }
    Ok((
        metrics.textPosition
            + if trailing.as_bool() {
                metrics.length
            } else {
                0
            },
        inside.as_bool(),
    ))
}

impl Native {
    pub fn text_event_updated(&self, hwnd: HWND) {
        unsafe {
            let _ = InvalidateRect(Some(hwnd), None, false);
        }
    }

    pub fn text_pointer_down(&mut self, hwnd: HWND, point: Point) -> Result<bool> {
        let Some(input) = &mut self.text else {
            return Ok(false);
        };
        let (offset, inside) = hit(input, point)?;
        if !inside {
            return Ok(false);
        }
        let anchor = if modifiers().shift {
            selection(input.window).0
        } else {
            offset
        };
        input.drag_anchor = Some(anchor);
        unsafe {
            SendMessageW(
                input.window,
                EM_SETSEL,
                Some(WPARAM(anchor as usize)),
                Some(LPARAM(offset as isize)),
            );
            let _ = SetFocus(Some(input.window));
            SetCapture(hwnd);
        }
        Ok(true)
    }

    pub fn text_pointer_move(&mut self, point: Point) -> Result<bool> {
        let Some(input) = &self.text else {
            return Ok(false);
        };
        let Some(anchor) = input.drag_anchor else {
            return Ok(false);
        };
        let (offset, _) = hit(input, point)?;
        unsafe {
            SendMessageW(
                input.window,
                EM_SETSEL,
                Some(WPARAM(anchor as usize)),
                Some(LPARAM(offset as isize)),
            );
        }
        Ok(true)
    }

    pub fn text_pointer_up(&mut self) -> bool {
        let active = self
            .text
            .as_mut()
            .is_some_and(|input| input.drag_anchor.take().is_some());
        if active {
            unsafe {
                let _ = ReleaseCapture();
            }
        }
        active
    }

    pub fn draw_text_input(&self) -> Result<()> {
        let Some(input) = &self.text else {
            return Ok(());
        };
        self.draw_inline(input, true)
    }
    pub(super) fn draw_inline(&self, input: &TextInput, caret: bool) -> Result<()> {
        let layout = layout(&value(input.window), input.size)?;
        let (start, end) = selection(input.window);
        let target = &self.renderer.dc;
        let previous = unsafe {
            let mut m = windows_numerics::Matrix3x2::identity();
            target.GetTransform(&mut m);
            m
        };
        let center = text_center(input, &layout)?;
        let transform = windows_numerics::Matrix3x2::rotation_around(
            input.rotation.to_degrees() as f32,
            crate::render::vector(center),
        );
        let scroll =
            windows_numerics::Matrix3x2::translation(-horizontal_scroll(input, &layout)?, 0.);
        unsafe {
            target.SetTransform(&(scroll * transform * previous));
        }
        let result = (|| -> Result<()> {
            unsafe {
                let brush = target.CreateSolidColorBrush(&color(input.color, 1.), None)?;
                if end > start {
                    let mut count = 0;
                    let _ = layout.HitTestTextRange(
                        start,
                        end - start,
                        input.position.x as f32,
                        input.position.y as f32,
                        None,
                        &mut count,
                    );
                    let mut regions = vec![DWRITE_HIT_TEST_METRICS::default(); count as usize];
                    layout.HitTestTextRange(
                        start,
                        end - start,
                        input.position.x as f32,
                        input.position.y as f32,
                        Some(&mut regions),
                        &mut count,
                    )?;
                    let highlight =
                        target.CreateSolidColorBrush(&color([88, 166, 255], 0.4), None)?;
                    for r in regions.iter().take(count as usize) {
                        target.FillRectangle(
                            &D2D_RECT_F {
                                left: r.left,
                                top: r.top,
                                right: r.left + r.width,
                                bottom: r.top + r.height,
                            },
                            &highlight,
                        );
                    }
                }
                target.DrawTextLayout(
                    vector(input.position),
                    &layout,
                    &brush,
                    D2D1_DRAW_TEXT_OPTIONS_NONE,
                );
                if start == end && caret {
                    let (mut x, mut y) = (0., 0.);
                    let mut metrics = DWRITE_HIT_TEST_METRICS::default();
                    layout.HitTestTextPosition(end, false, &mut x, &mut y, &mut metrics)?;
                    let origin = input.position
                        + Point {
                            x: x as f64,
                            y: y as f64,
                        };
                    target.DrawLine(
                        vector(origin),
                        vector(
                            origin
                                + Point {
                                    x: 0.,
                                    y: metrics.height as f64,
                                },
                        ),
                        &brush,
                        1.,
                        None,
                    );
                }
            }
            Ok(())
        })();
        unsafe {
            target.SetTransform(&previous);
        }
        result
    }
}

fn text_center(input: &TextInput, layout: &IDWriteTextLayout) -> Result<Point> {
    let mut metrics = DWRITE_TEXT_METRICS::default();
    unsafe {
        layout.GetMetrics(&mut metrics)?;
    }
    Ok(input.position
        + Point {
            x: metrics.widthIncludingTrailingWhitespace as f64 / 2.,
            y: metrics.height as f64 / 2.,
        })
}

fn horizontal_scroll(input: &TextInput, layout: &IDWriteTextLayout) -> Result<f32> {
    unsafe {
        if GetWindowLongPtrW(input.window, GWL_STYLE) as u32 & ES_MULTILINE as u32 != 0 {
            return Ok(0.);
        }
        let first = SendMessageW(input.window, EM_GETFIRSTVISIBLELINE, None, None)
            .0
            .max(0) as u32;
        if first == 0 {
            return Ok(0.);
        }
        let (mut x, mut y) = (0., 0.);
        let mut metrics = DWRITE_HIT_TEST_METRICS::default();
        layout.HitTestTextPosition(first, false, &mut x, &mut y, &mut metrics)?;
        Ok(x)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cancelling_inline_draft_discards_content_and_color_together() -> Result<()> {
        let _com = crate::ComApartment::new()?;
        unsafe {
            let parent = CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                w!("STATIC"),
                w!(""),
                WS_POPUP,
                0,
                0,
                400,
                200,
                None,
                None,
                None,
                None,
            )?;
            let mut session = Session::new(Image::solid(400, 200, [0, 0, 0, 255]))?;
            session
                .capture
                .editor
                .add_text(Point { x: 20., y: 30. }, "Original".into());
            let before = session.capture.editor.objects().to_vec();
            session
                .native
                .begin_text(&mut session.capture, parent, Point { x: 25., y: 35. })?;
            let input = session.native.text.as_mut().expect("inline draft");
            input.color = [20, 180, 90];
            SetWindowTextW(input.window, w!("Changed"))?;
            assert_eq!(session.capture.editor.objects(), before);
            session
                .native
                .finish_text(&mut session.capture, parent, false);
            assert!(session.native.text.is_none());
            assert_eq!(session.capture.editor.objects(), before);
            session
                .native
                .begin_text(&mut session.capture, parent, Point { x: 25., y: 35. })?;
            let input = session.native.text.as_mut().expect("inline draft");
            input.color = [20, 180, 90];
            SetWindowTextW(input.window, w!("Changed"))?;
            session
                .native
                .finish_text(&mut session.capture, parent, true);
            assert_eq!(
                session.capture.editor.objects()[0].style.color,
                [20, 180, 90]
            );
            assert!(
                matches!(&session.capture.editor.objects()[0].shape, Shape::Text { text, .. } if text == "Changed")
            );
            session.capture.editor.undo();
            assert_eq!(session.capture.editor.objects(), before);
            session.capture.editor.undo();
            assert!(
                session.capture.editor.objects().is_empty(),
                "cancel creates no history step"
            );
            DestroyWindow(parent)?;
        }
        Ok(())
    }

    #[test]
    fn transparent_native_input_keeps_selection_unicode_and_undo() -> Result<()> {
        unsafe {
            let parent = CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                w!("STATIC"),
                w!(""),
                WS_POPUP,
                0,
                0,
                400,
                200,
                None,
                None,
                None,
                None,
            )?;
            let child = CreateWindowExW(
                WS_EX_TRANSPARENT,
                w!("EDIT"),
                w!("é😀\r\n日本"),
                WS_CHILD | WS_VISIBLE | WINDOW_STYLE(ES_MULTILINE as u32 | ES_AUTOHSCROLL as u32),
                0,
                0,
                400,
                200,
                Some(parent),
                None,
                None,
                None,
            )?;
            let font = configure_input(child, 24.)?;
            let input = TextInput {
                rotation: 0.,
                window: child,
                position: Point::default(),
                object: None,
                size: 24.,
                color: [255; 3],
                font,
                drag_anchor: None,
            };
            let probe_region = CreateRectRgn(0, 0, 1, 1);
            assert_eq!(GetWindowRgn(child, probe_region), NULLREGION);
            let _ = DeleteObject(probe_region.into());
            let _ = SetFocus(Some(child));
            assert_eq!(GetFocus(), child);
            assert_eq!(
                SendMessageW(child, WM_NCHITTEST, None, None).0,
                HTTRANSPARENT as isize
            );
            assert_eq!(GetWindowLongW(child, GWL_STYLE) as u32 & WS_BORDER.0, 0);
            SendMessageW(child, EM_SETSEL, Some(WPARAM(1)), Some(LPARAM(3)));
            assert_eq!(selection(child), (1, 3));
            SendMessageW(child, WM_CHAR, Some(WPARAM('X' as usize)), None);
            assert_eq!(String::from_utf16_lossy(&value(child)), "éX\r\n日本");
            SetWindowTextW(child, w!("é😀\r\n日本"))?;
            let end = GetWindowTextLengthW(child);
            SendMessageW(
                child,
                EM_SETSEL,
                Some(WPARAM(end as usize)),
                Some(LPARAM(end as isize)),
            );
            SendMessageW(child, WM_CHAR, Some(WPARAM('!' as usize)), None);
            assert_eq!(String::from_utf16_lossy(&value(child)), "é😀\r\n日本!");
            SendMessageW(child, WM_UNDO, None, None);
            assert_eq!(String::from_utf16_lossy(&value(child)), "é😀\r\n日本");
            drop(input);
            DestroyWindow(parent)?;
        }
        Ok(())
    }

    #[test]
    fn directwrite_caret_tracks_utf16_and_multiline_text() -> Result<()> {
        let text: Vec<u16> = "é😀\r\n第二行".encode_utf16().collect();
        let layout = layout(&text, 24.)?;
        let (mut x, mut y) = (0., 0.);
        let mut metric = DWRITE_HIT_TEST_METRICS::default();
        unsafe {
            layout.HitTestTextPosition(5, false, &mut x, &mut y, &mut metric)?;
        }
        assert!(y > 0.);
        assert_eq!(metric.textPosition, 5);
        let (mut trailing, mut inside) = (BOOL(0), BOOL(0));
        unsafe {
            layout.HitTestPoint(
                x + 1.,
                y + metric.height / 2.,
                &mut trailing,
                &mut inside,
                &mut metric,
            )?;
        }
        assert_eq!(metric.textPosition, 5);
        assert!(inside.as_bool());
        Ok(())
    }
}
