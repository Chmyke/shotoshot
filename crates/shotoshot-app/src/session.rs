//! ROLE: Capture-session adapter between Windows events, pure editor, and rendering.
//! OWNER: Resident application; dropped when the session ends.
mod board;
mod dimensions;
mod document;
mod editing;
mod fill;
mod hint;
mod io;
pub use io::Saved;
pub use text::CARET_TIMER;
mod live;
mod magnifier;
mod pointer;
mod preparation;
pub(crate) use preparation::Preparation;
mod text;
mod view;
use crate::{
    render::{color, rect},
    settings::{Settings, Store},
    toolbar::Action,
    *,
};
use shotoshot_core::*;
use shotoshot_ui::{Capture, CursorKind, Face, Host, keys::KeyOutcome};
use windows::{
    Win32::{
        Foundation::*,
        Graphics::{Direct2D::*, Gdi::*},
        UI::{Controls::Dialogs::*, Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
    },
    core::*,
};

pub struct TextInput {
    pub rotation: f64,
    pub window: HWND,
    pub position: Point,
    pub object: Option<u64>,
    pub size: f32,
    /// Typeface the hidden control and its DirectWrite presentation share.
    pub face: Face,
    pub color: [u8; 3],
    /// The panel the text sits on; a wrapped one breaks the lines at its width.
    pub label: Option<Label>,
    /// The ink of the text on its panel; `None` is the black or white that contrasts with it.
    pub ink: Option<[u8; 3]>,
    /// The height of the text block a box keeps while it holds no text: the height it was
    /// drawn at, or its own when reopened (`kept_block`).
    pub height: f64,
    pub font: HFONT,
    pub drag_anchor: Option<u32>,
    /// What a one-line field shows of a text wider than itself; a text with no field
    /// to overflow never moves.
    pub scroll: std::cell::Cell<text::Scroll>,
}
/// Windows-only pieces of a capture: renderer, in-place text field, crop-size fields and
/// the surfaces the overlay is drawn into before it reaches the window.
pub struct Native {
    pub renderer: Renderer,
    pub text: Option<TextInput>,
    dimensions: Option<dimensions::DimensionsInput>,
    // What live capture and the live eyedropper read; a test swaps in a painted one.
    pub(crate) screen: crate::image::Screen,
    live_surface: Option<crate::image::LiveSurface>,
    live_sample: Option<crate::image::DesktopSample>,
    // Where the software renderer draws a frozen capture; see `paint_window`.
    back_buffer: Option<crate::image::LiveSurface>,
    // The hardware renderer being built for `accelerate`.
    acceleration: Option<std::sync::mpsc::Receiver<Result<Prepared>>>,
    pub caret: text::CaretBlink,
    // The ink the cross last showed: it changes only past a band, see `CrossInk::against`.
    cross_ink: std::cell::Cell<shotoshot_ui::cursor_image::CrossInk>,
}
pub struct Session {
    pub capture: Capture,
    pub native: Native,
    /// The `.shoto` file this session was reopened from; Save rewrites it.
    pub document: Option<std::path::PathBuf>,
    /// For a board session, the screens its frame covers: the notes go back to them when it
    /// ends, even if the display changed meanwhile.
    pub board: Option<board::BoardSession>,
}
impl Session {
    pub fn new(image: Image) -> Result<Self> {
        Self::with_backend(Backend::Hardware, image)
    }
    /// For the capture shortcut: the first frame must not wait about 150 ms for a Direct3D
    /// device. The software renderer draws until `accelerate` receives the hardware one.
    pub fn immediate(image: Image) -> Result<Self> {
        Self::with_backend(Backend::Software, image)
    }
    fn with_backend(backend: Backend, image: Image) -> Result<Self> {
        let mut renderer = Renderer::with(backend, false)?;
        renderer.set_image(&image)?;
        Ok(Self {
            capture: Capture::new(image),
            native: Native {
                renderer,
                text: None,
                dimensions: None,
                screen: crate::image::Screen::Desktop,
                live_surface: None,
                live_sample: None,
                back_buffer: None,
                acceleration: None,
                caret: Default::default(),
                cross_ink: Default::default(),
            },
            document: None,
            board: None,
        })
    }
    /// Builds the hardware renderer on another thread, then posts `ready` to `hwnd`, whose
    /// handler calls `accelerate`. Nothing is posted if the session is dropped first.
    pub fn prepare_acceleration(&mut self, hwnd: HWND, ready: u32) -> Result<()> {
        let (sender, receiver) = std::sync::mpsc::channel();
        let (owner, transparent) = (hwnd.0 as usize, self.capture.live);
        std::thread::Builder::new()
            .name("Shotoshot renderer".into())
            .spawn(move || {
                if sender
                    .send(Prepared::new(Backend::Hardware, transparent))
                    .is_ok()
                {
                    unsafe {
                        let _ =
                            PostMessageW(Some(HWND(owner as *mut _)), ready, WPARAM(0), LPARAM(0));
                    }
                }
            })
            .map_err(|e| Error::new(E_FAIL, e.to_string()))?;
        self.native.acceleration = Some(receiver);
        Ok(())
    }
    /// Hands the drawing over to the renderer `prepare_acceleration` built. Without one, or
    /// if Windows could not build it, the software renderer keeps drawing.
    pub fn accelerate(&mut self) -> Result<()> {
        let Some(receiver) = &self.native.acceleration else {
            return Ok(());
        };
        let built = match receiver.try_recv() {
            Err(std::sync::mpsc::TryRecvError::Empty) => return Ok(()),
            other => other,
        };
        self.native.acceleration = None;
        let Ok(Ok(prepared)) = built else {
            return Ok(());
        };
        let mut renderer = prepared.into_renderer();
        if renderer.transparent() != self.capture.live {
            // Live was toggled while it was being built.
            renderer = Renderer::with(renderer.backend(), self.capture.live)?;
        }
        if !self.capture.live {
            renderer.set_image(&self.capture.image)?;
        }
        self.native.renderer = renderer;
        self.native.back_buffer = None;
        Ok(())
    }
    pub fn text_command(&mut self, hwnd: HWND, key: u32, ctrl: bool) -> bool {
        if self.native.text.is_none()
            || self.capture.picker.is_some()
            || self.capture.eyedropper
            || !(key == 27 || key == 13 && ctrl)
        {
            return false;
        }
        // A board loses nothing: Escape settles the text being typed instead of dropping it.
        let commit = key != 27 || self.board.is_some();
        self.native.finish_text(&mut self.capture, hwnd, commit);
        true
    }
    pub(crate) fn host(&mut self, hwnd: HWND) -> (&mut Capture, WinHost<'_>) {
        (
            &mut self.capture,
            WinHost {
                hwnd,
                native: &mut self.native,
            },
        )
    }
    pub fn mouse_down(&mut self, hwnd: HWND, p: Point) -> Result<Option<Action>> {
        self.refresh_live_sample(p)?;
        let (capture, mut host) = self.host(hwnd);
        capture.pointer_down(&mut host, p, modifiers())
    }
    pub fn mouse_move(&mut self, hwnd: HWND, p: Point) {
        let (capture, mut host) = self.host(hwnd);
        capture.pointer_move(&mut host, p, modifiers());
    }
    pub fn mouse_up(&mut self, hwnd: HWND, p: Point) {
        let (capture, mut host) = self.host(hwnd);
        capture.pointer_up(&mut host, p, modifiers());
    }
    pub fn action(&mut self, hwnd: HWND, a: Action) {
        let (capture, mut host) = self.host(hwnd);
        capture.action(&mut host, a);
    }
    /// A right press; true when it changed what is shown (`Capture::secondary_down`).
    pub fn secondary_down(&mut self, hwnd: HWND, p: Point) -> bool {
        let (capture, mut host) = self.host(hwnd);
        capture.secondary_down(&mut host, p)
    }
    pub fn cancel_gesture(&mut self, hwnd: HWND) -> bool {
        let (capture, mut host) = self.host(hwnd);
        capture.cancel_gesture(&mut host)
    }
    #[cfg(test)]
    pub fn accept_custom_color(&mut self, hwnd: HWND, color: [u8; 3]) {
        let (capture, mut host) = self.host(hwnd);
        capture.accept_custom_color(&mut host, color);
    }
    pub fn focus_text(&self) {
        self.native.focus_text();
    }
    pub fn key(&mut self, hwnd: HWND, key: u32) -> KeyOutcome {
        let (capture, mut host) = self.host(hwnd);
        capture.key(&mut host, key, modifiers())
    }
    /// The pointer over client point `p`.
    pub fn cursor(&self, p: Point) -> Result<HCURSOR> {
        let over_badge = !self.capture.selecting
            && self
                .capture
                .dimensions_bounds()
                .is_some_and(|r| r.contains(p));
        // The text being typed is not an object yet; a click in it places the caret.
        let over_field = self.native.text.as_ref().is_some_and(|input| {
            text::hit(input, self.capture.to_frame(p)).is_ok_and(|(_, inside)| inside)
        });
        let kind = if over_badge || over_field {
            CursorKind::IBeam
        } else {
            self.capture.cursor(p)
        };
        let system = match kind {
            CursorKind::NewText => return pointer::new_text(self.capture.ui_scale()),
            CursorKind::Arrow => IDC_ARROW,
            CursorKind::Cross => {
                let scale = self.capture.ui_scale();
                let reach = pointer::size(scale) as f64 * shotoshot_ui::cursor_image::CROSS_REACH;
                let shown = self.native.cross_ink.get();
                let ink = shown.against(self.capture.luminance_under(p, reach));
                self.native.cross_ink.set(ink);
                return pointer::cross(scale, ink);
            }
            CursorKind::IBeam => IDC_IBEAM,
            CursorKind::Hand => IDC_HAND,
            CursorKind::Move => IDC_SIZEALL,
            CursorKind::ResizeNwse => IDC_SIZENWSE,
            CursorKind::ResizeNesw => IDC_SIZENESW,
            CursorKind::ResizeNs => IDC_SIZENS,
            CursorKind::ResizeWe => IDC_SIZEWE,
        };
        unsafe { LoadCursorW(None, system) }
    }
    pub fn rebuild_renderer(&mut self) -> Result<()> {
        let mut replacement = Renderer::with(self.native.renderer.backend(), self.capture.live)?;
        if !self.capture.live {
            replacement.set_image(&self.capture.image)?;
        }
        replacement.set_overlay(self.native.renderer.overlay().cloned())?;
        self.native.renderer = replacement;
        Ok(())
    }
    pub fn paint(&mut self, hdc: HDC) -> Result<()> {
        self.native.layout_badge(&mut self.capture)?;
        let result = self.paint_once(hdc);
        if result
            .as_ref()
            .is_err_and(|error| error.code() == D2DERR_RECREATE_TARGET)
        {
            self.rebuild_renderer()?;
            self.paint_once(hdc)
        } else {
            result
        }
    }
    fn paint_once(&self, hdc: HDC) -> Result<()> {
        let (width, height) = (
            self.capture.image.width as i32,
            self.capture.image.height as i32,
        );
        if self.capture.zoomed() {
            self.native.renderer.begin_blank(hdc, width, height)?;
        } else {
            self.native.renderer.begin(hdc, width, height)?;
        }
        let draw = self.paint_contents();
        let end = self.native.renderer.end();
        draw?;
        end
    }

    pub fn export(&self) -> Result<Image> {
        self.flatten(&self.capture.image)
    }
    /// The region of `image` under the capture's background and annotations.
    fn flatten(&self, image: &Image) -> Result<Image> {
        let region = self
            .capture
            .region
            .ok_or_else(|| crate::failure::visible(shotoshot_text::Key::ErrorNoRegion, &[]))?;
        let editor = &self.capture.editor;
        self.native
            .renderer
            .export(image, editor.background(), editor.objects(), region)
    }
}
impl Native {
    pub fn focus_text(&self) {
        if self.dimensions.is_some() {
            self.focus_dimensions();
            return;
        }
        if let Some(text) = &self.text {
            unsafe {
                let _ = SetFocus(Some(text.window));
            }
        }
    }
}
/// Windows side of the portable session: the capture window and its native fields.
pub(crate) struct WinHost<'a> {
    hwnd: HWND,
    native: &'a mut Native,
}
impl Host for WinHost<'_> {
    type Error = Error;
    fn capture_pointer(&mut self) {
        unsafe {
            SetCapture(self.hwnd);
        }
    }
    fn release_pointer(&mut self) {
        unsafe {
            let _ = ReleaseCapture();
        }
    }
    fn focus_text(&mut self) {
        self.native.focus_text();
    }
    fn text_active(&self) -> bool {
        self.native.text.is_some()
    }
    fn set_text_color(&mut self, rgb: [u8; 3]) {
        if let Some(text) = &mut self.native.text {
            text.color = rgb;
        }
    }
    fn set_text_ink(&mut self, ink: Option<[u8; 3]>) {
        if let Some(text) = &mut self.native.text {
            text.ink = ink;
        }
    }
    fn cancel_text_drag(&mut self) -> bool {
        self.native
            .text
            .as_mut()
            .is_some_and(|t| t.drag_anchor.take().is_some())
    }
    fn begin_text(&mut self, capture: &mut Capture, p: Point) -> Result<()> {
        self.native.begin_text(capture, self.hwnd, p)
    }
    fn finish_text(&mut self, capture: &mut Capture, commit: bool) {
        self.native.finish_text(capture, self.hwnd, commit);
    }
    fn text_pointer_down(&mut self, p: Point) -> Result<bool> {
        self.native.text_pointer_down(self.hwnd, p)
    }
    fn text_pointer_move(&mut self, p: Point) -> bool {
        self.native.text_pointer_move(p).unwrap_or(false)
    }
    fn text_pointer_up(&mut self) -> bool {
        self.native.text_pointer_up()
    }
    fn text_size(&self) -> Option<f32> {
        self.native.text.as_ref().map(|text| text.size)
    }
    fn set_text_size(&mut self, size: f32) -> Result<()> {
        self.native.set_text_size(self.hwnd, size)
    }
    fn set_text_label(&mut self, position: Point, label: Option<Label>, height: f64) {
        if let Some(text) = &mut self.native.text {
            text.position = position;
            text.label = label;
            text.height = height;
            self.native.text_event_updated(self.hwnd);
        }
    }
    fn measure_text(&mut self, text: &str, size: f64) -> Result<Point> {
        self.native
            .renderer
            .measure_annotation_text(text, size as f32)
    }
    fn measure_wrapped_text(&mut self, text: &str, size: f64, width: f64) -> Result<Point> {
        self.native
            .renderer
            .measure_wrapped_annotation_text(text, size as f32, width as f32)
    }
    fn dimensions_active(&self) -> bool {
        self.native.dimensions.is_some()
    }
    fn begin_dimensions(&mut self, capture: &mut Capture) -> Result<()> {
        self.native.begin_dimensions(capture, self.hwnd)
    }
    fn finish_dimensions(&mut self, capture: &mut Capture, commit: bool) -> bool {
        self.native.finish_dimensions(capture, self.hwnd, commit)
    }
    fn dimension_pointer_down(&mut self, capture: &mut Capture, p: Point) -> Result<bool> {
        self.native.dimension_pointer_down(capture, self.hwnd, p)
    }
    fn dimension_pointer_move(&mut self, p: Point) {
        let _ = self.native.dimension_pointer_move(p);
    }
    fn dimension_pointer_up(&mut self) -> bool {
        self.native.dimension_pointer_up()
    }
    fn dimension_key(&mut self, capture: &mut Capture, key: u32) -> bool {
        self.native.dimension_command(capture, self.hwnd, key) || self.native.has_dimensions_input()
    }
}
pub fn modifiers() -> Modifiers {
    unsafe {
        Modifiers {
            shift: GetKeyState(VK_SHIFT.0 as i32) < 0,
            alt: GetKeyState(VK_MENU.0 as i32) < 0,
            ctrl: GetKeyState(VK_CONTROL.0 as i32) < 0,
        }
    }
}
pub fn control() -> bool {
    unsafe { GetKeyState(VK_CONTROL.0 as i32) < 0 }
}
