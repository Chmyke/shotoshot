//! ROLE: Native editing semantics with DirectWrite presentation on the capture.
//! OWNER: Session owns the EDIT window and its font until commit or cancellation.
//! INVARIANTS: The native control never paints a surface; UTF-16 offsets stay native;
//! what a one-line field shows of its text is decided here, never read from the control.
use super::*;
use crate::render::{annotation_font, vector};
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

pub(super) fn configure_input(hwnd: HWND, size: f32, face: Face) -> Result<HFONT> {
    unsafe {
        // EDIT draws its line itself on every keystroke, outside WM_PAINT: a white band
        // as wide as the field, which an empty window region does not clip. With redraw
        // off it draws nothing and keeps focus, text and selection. This also clears
        // WS_VISIBLE: the capture window already hit-tests the text itself.
        SendMessageW(hwnd, WM_SETREDRAW, Some(WPARAM(0)), None);
        if !SetWindowSubclass(hwnd, Some(input_proc), 1, 0).as_bool() {
            return Err(Error::from_thread());
        }
        let font = match face {
            Face::Annotation => annotation_font::gdi_font(size)?,
            Face::Interface => CreateFontW(
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
            ),
        };
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

fn layout(text: &[u16], size: f32, face: Face) -> Result<IDWriteTextLayout> {
    unsafe {
        let factory: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
        let format = match face {
            Face::Annotation => annotation_font::format(&factory, size)?,
            Face::Interface => factory.CreateTextFormat(
                w!("Segoe UI"),
                None,
                DWRITE_FONT_WEIGHT_NORMAL,
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                size,
                w!("en-US"),
            )?,
        };
        format.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)?;
        factory.CreateTextLayout(text, &format, 1_000_000., 1_000_000.)
    }
}

/// How a one-line field last showed its text: the shift to the left, in pixels, and the
/// selection that shift revealed. Only the drawing writes it, so a click is hit-tested
/// against what is on screen.
// PITFALL: EDIT scrolls its line by itself but reports nothing usable: it keeps a pixel
// offset in its own GDI measure, which runs wider than this layout, and
// EM_GETFIRSTVISIBLELINE answers 0.
#[derive(Clone, Copy, Default)]
pub struct Scroll {
    offset: f32,
    selection: (u32, u32),
}

impl Scroll {
    /// Follows the end of `selection` that moved, in a field `view` pixels wide.
    fn reveal(self, layout: &IDWriteTextLayout, selection: (u32, u32), view: f32) -> Result<Self> {
        let (start, end) = selection;
        // A caret is always followed; of a selection, only the end being placed.
        let moved = if start == end || end != self.selection.1 {
            Some(end)
        } else if start != self.selection.0 {
            Some(start)
        } else {
            None
        };
        let caret = moved
            .map(|position| -> Result<f32> {
                let (mut x, mut y) = (0., 0.);
                let mut metrics = DWRITE_HIT_TEST_METRICS::default();
                unsafe {
                    layout.HitTestTextPosition(position, false, &mut x, &mut y, &mut metrics)?;
                }
                Ok(x)
            })
            .transpose()?;
        let mut content = DWRITE_TEXT_METRICS::default();
        unsafe { layout.GetMetrics(&mut content)? };
        let width = content.widthIncludingTrailingWhitespace;
        Ok(Self {
            offset: scrolled(self.offset, caret, width, view),
            selection,
        })
    }
}

/// The smallest move of a `view`-wide window over a `content`-wide line that shows the
/// caret at `caret`, if there is one to follow, and leaves no gap after the text. Whole
/// pixels: the glyphs keep their rendering while the line moves.
fn scrolled(offset: f32, caret: Option<f32>, content: f32, view: f32) -> f32 {
    // One pixel on either side for the caret line itself.
    let shown = caret.map_or(offset, |x| {
        offset.min((x - 1.).floor()).max((x + 1. - view).ceil())
    });
    shown.min((content + 1. - view).ceil()).max(0.)
}

pub(super) fn hit(input: &TextInput, point: Point) -> Result<(u32, bool)> {
    let layout = layout(&value(input.window), input.size, input.face)?;
    let mut point = rotate_point(point, text_center(input, &layout)?, -input.rotation);
    point.x += input.scroll.get().offset as f64;
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
        self.draw_inline(input, true, None)
    }
    /// `view` is the width of a one-line field, which then scrolls to its caret; the
    /// caller clips to it.
    pub(super) fn draw_inline(
        &self,
        input: &TextInput,
        caret: bool,
        view: Option<f32>,
    ) -> Result<()> {
        let layout = layout(&value(input.window), input.size, input.face)?;
        let (start, end) = selection(input.window);
        if let Some(view) = view {
            let shown = input.scroll.get().reveal(&layout, (start, end), view)?;
            input.scroll.set(shown);
        }
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
        let scroll = windows_numerics::Matrix3x2::translation(-input.scroll.get().offset, 0.);
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
            let font = configure_input(child, 24., Face::Annotation)?;
            let input = TextInput {
                rotation: 0.,
                window: child,
                position: Point::default(),
                object: None,
                size: 24.,
                face: Face::Annotation,
                color: [255; 3],
                font,
                drag_anchor: None,
                scroll: Default::default(),
            };
            // Quick tripwire only; the pixels are watched by the real_desktop_ tests below.
            assert_eq!(GetWindowLongW(child, GWL_STYLE) as u32 & WS_VISIBLE.0, 0);
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
        let layout = layout(&text, 24., Face::Annotation)?;
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

    #[test]
    fn a_one_line_field_moves_its_text_only_to_keep_the_caret_in_view() {
        let (view, content) = (43., 120.4);
        // A text that fits never moves.
        assert_eq!(scrolled(0., Some(30.), 30., view), 0.);
        // Typing past the right edge takes the text along, caret line included.
        let end = scrolled(0., Some(content), content, view);
        assert_eq!(end, 79.);
        // Stepping back inside the view leaves the text where it is.
        assert_eq!(scrolled(end, Some(100.), content, view), end);
        // Past the left edge the text follows, by no more than it takes.
        assert_eq!(scrolled(end, Some(60.3), content, view), 59.);
        assert_eq!(scrolled(end, Some(0.), content, view), 0.);
        // Deleting the tail leaves no gap after the text, caret followed or not.
        assert_eq!(scrolled(end, Some(90.), 90., view), 48.);
        assert_eq!(scrolled(end, None, 90., view), 48.);
        assert_eq!(scrolled(end, None, 20., view), 0.);
        // A selection whose ends did not move leaves the text alone.
        assert_eq!(scrolled(end, None, content, view), end);
        // Wherever the text was and wherever the caret goes, its line is whole in the view.
        for from in [0., 40., 79.] {
            for step in 0..=172 {
                let caret = step as f32 * 0.7;
                let offset = scrolled(from, Some(caret), content, view);
                assert_eq!(offset, offset.round());
                assert!(caret + 1. <= offset + view, "{from} {caret} {offset}");
                assert!(
                    offset == 0. || caret - 1. >= offset,
                    "{from} {caret} {offset}"
                );
            }
        }
    }

    const STAGE_CLASS: PCWSTR = w!("Shotoshot.TextStage");
    unsafe extern "system" fn stage_proc(h: HWND, m: u32, w: WPARAM, l: LPARAM) -> LRESULT {
        unsafe { DefWindowProcW(h, m, w, l) }
    }
    /// A shown, black stand-in for the capture window that never repaints by itself: its
    /// pixels are what a gesture leaves on screen until the next WM_PAINT.
    struct Stage {
        window: HWND,
        instance: HINSTANCE,
        size: (i32, i32),
        painted: Vec<String>,
    }
    impl Drop for Stage {
        fn drop(&mut self) {
            unsafe {
                let _ = DestroyWindow(self.window);
                let _ = UnregisterClassW(STAGE_CLASS, Some(self.instance));
            }
        }
    }
    impl Stage {
        fn new(width: i32, height: i32) -> Result<Self> {
            unsafe {
                let instance: HINSTANCE =
                    windows::Win32::System::LibraryLoader::GetModuleHandleW(None)?.into();
                let class = WNDCLASSW {
                    lpfnWndProc: Some(stage_proc),
                    hInstance: instance,
                    lpszClassName: STAGE_CLASS,
                    ..Default::default()
                };
                assert_ne!(RegisterClassW(&class), 0);
                let window = CreateWindowExW(
                    WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOACTIVATE,
                    STAGE_CLASS,
                    w!(""),
                    WS_POPUP,
                    40,
                    40,
                    width,
                    height,
                    None,
                    None,
                    Some(instance),
                    None,
                )?;
                let stage = Self {
                    window,
                    instance,
                    size: (width, height),
                    painted: vec![],
                };
                let _ = ShowWindow(window, SW_SHOWNOACTIVATE);
                // Gestures read the modifier keys: none is held, whatever the user is typing.
                SetKeyboardState(&[0; 256])?;
                stage.blacken();
                assert_eq!(stage.lit()?, None);
                // A stage that cannot see paint would let every gesture pass.
                let seen = RECT {
                    left: 8,
                    top: 8,
                    right: 16,
                    bottom: 12,
                };
                stage.fill(seen, WHITE_BRUSH);
                assert_eq!(stage.lit()?, Some([8, 8, 15, 11]));
                stage.blacken();
                Ok(stage)
            }
        }
        fn fill(&self, area: RECT, brush: GET_STOCK_OBJECT_FLAGS) {
            unsafe {
                let dc = GetDC(Some(self.window));
                FillRect(dc, &area, HBRUSH(GetStockObject(brush).0));
                ReleaseDC(Some(self.window), dc);
            }
        }
        fn blacken(&self) {
            let all = RECT {
                left: 0,
                top: 0,
                right: self.size.0,
                bottom: self.size.1,
            };
            self.fill(all, BLACK_BRUSH);
        }
        /// Left, top, right and bottom of the pixels that are no longer black.
        fn lit(&self) -> Result<Option<[usize; 4]>> {
            let (width, height) = self.size;
            let seen = unsafe {
                let dc = GetDC(Some(self.window));
                let seen = crate::paint_image(width as u32, height as u32, |memory| {
                    BitBlt(memory, 0, 0, width, height, Some(dc), 0, 0, SRCCOPY)
                });
                ReleaseDC(Some(self.window), dc);
                seen?
            };
            let mut bounds: Option<[usize; 4]> = None;
            for (index, pixel) in seen.pixels.as_chunks::<4>().0.iter().enumerate() {
                if pixel[..3] != [0, 0, 0] {
                    let (x, y) = (index % width as usize, index / width as usize);
                    let b = bounds.get_or_insert([x, y, x, y]);
                    *b = [b[0].min(x), b[1].min(y), b[2].max(x), b[3].max(y)];
                }
            }
            Ok(bounds)
        }
        /// Notes what `gesture` put on screen before any WM_PAINT, then clears it.
        fn after(&mut self, gesture: &str) -> Result<()> {
            if let Some([left, top, right, bottom]) = self.lit()? {
                self.painted
                    .push(format!("{gesture}: x {left}..={right}, y {top}..={bottom}"));
                self.blacken();
            }
            Ok(())
        }
        /// Types into `field` the characters the message loop translates from keys.
        fn type_text(&mut self, field: HWND, text: &str) -> Result<()> {
            for (unit, typed) in text.encode_utf16().zip(text.chars()) {
                unsafe { SendMessageW(field, WM_CHAR, Some(WPARAM(unit as usize)), None) };
                self.after(&format!("typing {typed:?}"))?;
            }
            Ok(())
        }
        fn assert_nothing_painted(&self) {
            assert!(
                self.painted.is_empty(),
                "the native field painted the screen before the window did:\n{}",
                self.painted.join("\n")
            );
        }
    }

    #[test]
    #[ignore = "real desktop: shows a window and reads its pixels, run alone by check.ps1"]
    fn real_desktop_text_field_paints_nothing_while_typing() -> Result<()> {
        let _com = crate::ComApartment::new()?;
        let mut stage = Stage::new(640, 200)?;
        let hwnd = stage.window;
        let mut session = Session::new(Image::solid(640, 200, [0, 0, 0, 255]))?;
        session.capture.region = Rect::new(Point::default(), Point { x: 640., y: 200. });
        session.capture.selecting = false;
        session.capture.toolbar_visible = false;
        session.capture.editor.tool = Tool::Text;
        let origin = Point { x: 40., y: 40. };
        session.mouse_down(hwnd, origin)?;
        session.mouse_up(hwnd, origin);
        let field = session.native.text.as_ref().expect("open text").window;
        stage.after("opening the field")?;
        assert_eq!(unsafe { GetFocus() }, field);
        // Backspace, then Enter for a second line.
        stage.type_text(field, "ab c\u{8}\rd")?;
        assert_eq!(String::from_utf16_lossy(&value(field)), "ab \r\nd");
        session.wheel(hwnd, 120)?;
        stage.after("the size wheel")?;
        assert_eq!(session.native.text.as_ref().expect("open text").size, 28.);
        stage.type_text(field, "e")?;
        assert_eq!(unsafe { GetFocus() }, field);
        // The window itself still paints where the field stands.
        let under = RECT {
            left: 300,
            top: 60,
            right: 400,
            bottom: 100,
        };
        stage.fill(under, WHITE_BRUSH);
        assert_eq!(stage.lit()?, Some([300, 60, 399, 99]));
        stage.blacken();
        // Ctrl+Enter commits; a click in the committed text reopens it at the clicked letter.
        assert!(session.text_command(hwnd, 13, true));
        let first_letter = Point { x: 41., y: 45. };
        session.mouse_down(hwnd, first_letter)?;
        session.mouse_up(hwnd, first_letter);
        let field = session.native.text.as_ref().expect("reopened text").window;
        stage.after("clicking in an existing text")?;
        assert_eq!(unsafe { GetFocus() }, field);
        assert_eq!(selection(field), (0, 0));
        stage.type_text(field, "X")?;
        assert_eq!(String::from_utf16_lossy(&value(field)), "Xab \r\nde");
        stage.assert_nothing_painted();
        Ok(())
    }

    #[test]
    #[ignore = "real desktop: shows a window and reads its pixels, run alone by check.ps1"]
    fn real_desktop_dimension_fields_paint_nothing_while_typing() -> Result<()> {
        let _com = crate::ComApartment::new()?;
        let mut stage = Stage::new(500, 300)?;
        let hwnd = stage.window;
        let mut session = Session::new(Image::solid(500, 300, [0, 0, 0, 255]))?;
        session.capture.region = Rect::new(Point { x: 20., y: 70. }, Point { x: 450., y: 250. });
        session.capture.selecting = false;
        let badge = session.capture.dimensions_bounds().expect("size badge");
        let on_width = Point {
            x: badge.min.x + 8.,
            y: badge.center().y,
        };
        session.mouse_down(hwnd, on_width)?;
        session.mouse_up(hwnd, on_width);
        let fields = session.native.dimensions.as_ref().expect("open fields");
        let (width, height) = (fields.width.window, fields.height.window);
        stage.after("opening the fields")?;
        assert_eq!(unsafe { GetFocus() }, width);
        // The width opens selected: typing replaces it.
        stage.type_text(width, "400")?;
        assert!(
            session
                .native
                .dimension_command(&mut session.capture, hwnd, 9)
        );
        stage.after("Tab to the height")?;
        assert_eq!(unsafe { GetFocus() }, height);
        // Wider than the field: the control scrolls its own line, still without painting.
        stage.type_text(height, "100+10+10+10+10+10")?;
        session.mouse_down(hwnd, on_width)?;
        session.mouse_up(hwnd, on_width);
        stage.after("clicking in a field")?;
        assert_eq!(unsafe { GetFocus() }, width);
        assert!(
            session
                .native
                .dimension_command(&mut session.capture, hwnd, 13)
        );
        assert_eq!(
            session.capture.region.expect("region").size(),
            Point { x: 400., y: 150. }
        );
        stage.assert_nothing_painted();
        Ok(())
    }
}
