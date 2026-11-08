//! ROLE: Portable state of one capture session: frame, selected region, editor and
//! interface flags. Platform sessions wrap it with their native widgets and renderer.
//! OWNER: One capture; dropped when it ends.
use crate::{
    Image,
    toolbar::{self, Action, Button, HoverHint, Motion},
    wheel,
};
use shotoshot_core::{
    Editor, FillChange, Label, Object, Point, Rect, Shape, Tool, arrow_points, label_ink,
};
use shotoshot_text::Key;

/// Margin of the size badge on each side of what it shows, in logical pixels.
const BADGE_PADDING: f64 = 8.;
/// Room for the "×" between the two size fields, in logical pixels.
const BADGE_SEPARATOR: f64 = 14.;
/// Narrowest size field, so an emptied one can still be clicked, in logical pixels.
const BADGE_FIELD_MIN: f64 = 8.;
/// Font size of the size badge's "W × H" label, in logical pixels.
pub(crate) const BADGE_LABEL_SIZE: f64 = 13.;

/// How wide the size badge's text is, in pixels, as the platform last measured it: the
/// badge is exactly as wide as its digits, shown or being typed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BadgeText {
    /// Before the first frame: room for two five-digit values.
    Unmeasured,
    /// The "W × H" label at `BADGE_LABEL_SIZE`.
    Label(f64),
    /// The text of the two fields being edited, at `DimensionFields::FONT_SIZE`.
    Fields([f64; 2]),
}

/// Size badge geometry: the badge, the full-screen button and, while editing, the fields.
struct Badges {
    badge: Rect,
    button: Rect,
    fields: Option<[Rect; 2]>,
}

/// A monitor in frame coordinates with its interface scale: 1 at 96 DPI, 2.5 at 250 %.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Monitor {
    pub bounds: Rect,
    pub scale: f64,
}
impl From<Rect> for Monitor {
    fn from(bounds: Rect) -> Self {
        Self { bounds, scale: 1. }
    }
}

/// Pointer shape requested by the capture; each platform maps it to a native cursor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CursorKind {
    Arrow,
    Cross,
    IBeam,
    /// The text tool away from any text: a click starts one (`cursor_image::new_text`).
    NewText,
    Hand,
    Move,
    ResizeNwse,
    ResizeNesw,
    ResizeNs,
    ResizeWe,
}

pub struct Capture {
    pub hover_hint: HoverHint,
    pub toolbar_motion: Motion,
    pub custom_color: [u8; 3],
    pub custom_color_changed: bool,
    pub copied_objects: Vec<Object>,
    pub copied_sequence: u32,
    pub paste_count: u32,
    pub text_size: f32,
    pub live: bool,
    /// Reopened from a saved document: its frame is the capture, never today's desktop.
    pub reopened: bool,
    pub searching: bool,
    pub image: Image,
    pub editor: Editor,
    pub region: Option<Rect>,
    pub selecting: bool,
    pub region_origin: Option<Point>,
    pub region_before: Option<Rect>,
    pub pointer: Point,
    pub eyedropper: bool,
    /// Armed bucket: every click in the region fills, until another action puts it down.
    pub bucket: bool,
    pub width_preview: bool,
    /// Shows "Scroll to resize" by the pointer of a drawing tool, until the first wheel notch.
    pub wheel_hint: bool,
    pub(crate) wheel_travel: crate::wheel::Notches,
    pub region_resize: Option<usize>,
    pub toolbar_visible: bool,
    /// The mini toolbar setting (issue #6); a board ignores it and shows every tool.
    pub mini_toolbar: bool,
    /// The mini toolbar unfolded by its button; the next capture opens the same way.
    pub toolbar_unfolded: bool,
    pub picker: Option<crate::color_picker::ColorPicker>,
    /// Monitors in frame coordinates; empty when the frame is a single screen at 100 %.
    pub monitors: Vec<Monitor>,
    /// Interface scale the current stroke width and text size were chosen at.
    style_scale: f64,
    /// Selection replaced by the full-screen button, and the screen it applied.
    pub screen_toggle: Option<(Rect, Rect)>,
    /// A board session: the region spans every screen, so the toolbar stays on the screen under
    /// the pointer, where the hand is, and offers `Clear board`.
    pub board: bool,
    /// One view per monitor while a session may zoom (a board); empty elsewhere, where screen
    /// and frame coordinates coincide.
    pub views: Vec<crate::View>,
    /// A drag of the view in progress: the screen it moves and the last pointer position.
    pub(crate) pan: Option<(usize, Point)>,
    /// Space held: the next left drag moves the view instead of drawing.
    pub(crate) space_held: bool,
    /// The view a press began in: the drag that follows stays in it, whatever screen the
    /// pointer crosses, and never reaches past what it shows.
    pub(crate) gesture_view: Option<usize>,
    /// Set by the platform before each frame with `measure_badge`.
    pub badge_text: BadgeText,
    /// A text just committed, which Shotoshot selected rather than the user: Ctrl+C still
    /// copies the image until the next press.
    pub(crate) committed_text: Option<u64>,
    /// The region came back from the previous capture: until the next press inside it, a
    /// press outside starts a new one instead of drawing.
    pub(crate) recalled_region: bool,
    /// A press of the label tool, and where the pointer is now: a click fits the panel to
    /// the text, a drag sets its width.
    pub(crate) label_drag: Option<(Point, Point)>,
    /// The panel the next new text opens with, set by the label tool for `Host::begin_text`.
    pub new_label: Option<Label>,
    /// The palette colours the text of text boxes rather than their panel (`Action::TextColor`),
    /// until `Action::BoxColor`. Every capture starts on the panel.
    pub ink_target: bool,
    /// The ink the next text boxes of this capture take, the last one given; `None` keeps
    /// the automatic black or white.
    pub label_ink: Option<[u8; 3]>,
}
impl Capture {
    pub fn new(image: Image) -> Self {
        let editor = Editor::default();
        Self {
            hover_hint: HoverHint::default(),
            toolbar_motion: Motion::default(),
            custom_color: editor.style.color,
            custom_color_changed: false,
            copied_objects: Vec::new(),
            copied_sequence: 0,
            paste_count: 0,
            text_size: 24.,
            label_drag: None,
            new_label: None,
            ink_target: false,
            label_ink: None,
            live: false,
            reopened: false,
            searching: false,
            image,
            editor,
            region: None,
            selecting: true,
            region_origin: None,
            region_before: None,
            pointer: Point { x: -100., y: -100. },
            eyedropper: false,
            bucket: false,
            width_preview: false,
            wheel_hint: false,
            wheel_travel: Default::default(),
            region_resize: None,
            toolbar_visible: true,
            mini_toolbar: false,
            toolbar_unfolded: false,
            picker: None,
            monitors: Vec::new(),
            style_scale: 1.,
            screen_toggle: None,
            board: false,
            views: Vec::new(),
            pan: None,
            space_held: false,
            gesture_view: None,
            badge_text: BadgeText::Unmeasured,
            committed_text: None,
            recalled_region: false,
        }
    }
    /// Opens on `region`, the previous capture's, already selected; ignored unless it lies
    /// whole within this frame, which a change of screens may have moved.
    pub fn recall_region(&mut self, region: Rect) {
        let frame = Rect::new(
            Point::default(),
            Point {
                x: self.image.width as f64,
                y: self.image.height as f64,
            },
        );
        let size = region.size();
        if size.x < 2. || size.y < 2. || !frame.is_some_and(|f| f.contains_rect(region)) {
            return;
        }
        self.region = Some(region);
        self.selecting = false;
        self.recalled_region = true;
        self.adopt_scale();
    }
    /// Selects a text the user just committed, as a result rather than a choice.
    pub fn select_committed_text(&mut self, id: u64) {
        self.editor.select_only(id);
        self.committed_text = Some(id);
    }
    /// The selection is only the text Shotoshot selected on commit: Ctrl+C copies the image.
    pub(crate) fn selection_is_committed_text(&self) -> bool {
        self.committed_text
            .is_some_and(|id| self.editor.selected == [id])
    }
    /// Interface scale: that of the screen under the region, or under the pointer before a
    /// selection. Every interface size is in 96-DPI logical pixels multiplied by it.
    pub fn ui_scale(&self) -> f64 {
        let anchor = self.region.map_or(self.pointer, |r| r.center());
        // Windows offers 100 % to 500 %; anything else is a platform misreport.
        match self.monitor_at(anchor) {
            Some(m) if m.scale.is_finite() => m.scale.clamp(1., 5.),
            _ => 1.,
        }
    }
    /// Brings the editor in line with the screen under the region: handle sizes, and the
    /// current stroke width, eraser and text size, so an annotation looks the
    /// same at every scale.
    pub(crate) fn adopt_scale(&mut self) {
        let scale = self.ui_scale();
        self.editor.ui_scale = scale / self.zoom_at_pointer();
        if scale == self.style_scale {
            return;
        }
        let factor = scale / self.style_scale;
        self.editor.style.width = wheel::stroke_width_scaled(self.editor.style.width, factor);
        self.editor.eraser_size = wheel::eraser_size_scaled(self.editor.eraser_size, factor);
        self.text_size = wheel::text_size_scaled(self.text_size.into(), factor) as f32;
        self.style_scale = scale;
    }
    /// Stroke width to remember between captures: the current one as it is at 100 %.
    pub fn stroke_width_setting(&self) -> u32 {
        wheel::stroke_width_scaled(self.editor.style.width, 1. / self.style_scale) as u32
    }
    /// Eraser diameter to remember between captures, as `stroke_width_setting`.
    pub fn eraser_size_setting(&self) -> u32 {
        wheel::eraser_size_scaled(self.editor.eraser_size, 1. / self.style_scale) as u32
    }
    /// Opens with the mini toolbar when `on` (issue #6), as the last capture left it:
    /// `unfolded`, or folded on the pen, the one drawing tool it shows. A board keeps every
    /// tool, and its own.
    pub fn use_mini_toolbar(&mut self, on: bool, unfolded: bool) {
        self.mini_toolbar = on;
        self.toolbar_unfolded = unfolded;
        if on && !unfolded && !self.board {
            self.editor.tool = Tool::Pen;
        }
    }
    /// Whether the mini toolbar is unfolded, to remember for the next capture; `None` when
    /// this capture has nothing to say (mini toolbar off, or a board, which shows everything).
    pub fn fold_to_remember(&self) -> Option<bool> {
        (self.mini_toolbar && !self.board).then_some(self.toolbar_unfolded)
    }
    /// How much of the toolbar this capture shows.
    pub fn fold(&self) -> toolbar::Fold {
        match (self.mini_toolbar && !self.board, self.toolbar_unfolded) {
            (false, _) => toolbar::Fold::Full,
            (true, false) => toolbar::Fold::Folded,
            (true, true) => toolbar::Fold::Unfolded,
        }
    }
    pub fn buttons(&self) -> Vec<Button> {
        if !self.toolbar_visible || self.selecting {
            return Vec::new();
        }
        // The toolbar goes around the region and the badges stuck to it, never over them.
        let badge = self.region_badges().map(|b| b.badge.union(b.button));
        let mut buttons = self
            .region
            .map(|r| {
                let screen = self.toolbar_screen(r);
                toolbar::buttons_in(
                    r,
                    badge,
                    screen,
                    self.ui_scale(),
                    self.board,
                    self.label_context(),
                    self.fold(),
                )
            })
            .unwrap_or_default();
        for b in &mut buttons {
            match b.action {
                Action::MoreTools => b.active = self.toolbar_unfolded,
                Action::BoxColor => b.active = !self.ink_target,
                Action::TextColor => b.active = self.ink_target,
                Action::Undo => {
                    b.enabled = self.editor.can_undo();
                    if !b.enabled {
                        b.label = Key::CaptureNothingToUndo;
                    }
                }
                Action::Redo => {
                    b.enabled = self.editor.can_redo();
                    if !b.enabled {
                        b.label = Key::CaptureNothingToRedo;
                    }
                }
                _ => {}
            }
            if matches!(b.action, Action::GoogleSearch) && self.searching {
                b.active = true;
                b.label = Key::CaptureGoogleSearchInProgress;
            }
            if matches!(b.action, Action::Fill) {
                b.active = self.bucket;
            }
            if matches!(b.action, Action::Live) {
                b.active = self.live;
                b.enabled = !self.reopened;
                b.label = if self.reopened {
                    Key::CaptureLiveForNewCaptures
                } else if self.live {
                    Key::CaptureLiveOn
                } else {
                    Key::CaptureLiveFrozen
                };
            }
        }
        buttons
    }
    /// The screen that keeps the toolbar: the one under the pointer on a board, else the
    /// monitor holding the whole region, else the whole frame.
    fn toolbar_screen(&self, region: Rect) -> Rect {
        if self.board {
            return self.screen_at(self.pointer);
        }
        self.monitors
            .iter()
            .map(|m| m.bounds)
            .find(|screen| screen.contains_rect(region))
            .unwrap_or(Rect {
                min: Point::default(),
                max: Point {
                    x: self.image.width as f64,
                    y: self.image.height as f64,
                },
            })
    }
    pub fn toolbar_bounds(&self) -> Option<Rect> {
        toolbar::bounds(&self.buttons(), self.ui_scale())
    }
    pub fn toolbar_contains(&self, p: Point) -> bool {
        toolbar::panels(&self.buttons(), self.ui_scale())
            .iter()
            .any(|r| r.contains(p))
    }
    /// The colour the toolbar marks: the wheel's while it is open, `text` (the open text
    /// field's colour) while typing, else the drawing colour.
    pub fn toolbar_color(&self, text: Option<[u8; 3]>) -> [u8; 3] {
        self.picker
            .as_ref()
            .map(|p| p.color())
            .unwrap_or_else(|| text.unwrap_or(self.editor.style.color))
    }
    pub fn clamp(&self, p: Point) -> Point {
        Point {
            x: p.x.clamp(0., self.image.width as f64),
            y: p.y.clamp(0., self.image.height as f64),
        }
    }
    pub(crate) fn region_handle(&self, p: Point) -> Option<usize> {
        self.region
            .map(region_handles)?
            .iter()
            .position(|h| (*h - p).length() <= 8. * self.ui_scale())
    }
    pub fn sample_color(&self, p: Point) -> [u8; 3] {
        let x = p.x.clamp(0., (self.image.width - 1) as f64) as usize;
        let y = p.y.clamp(0., (self.image.height - 1) as f64) as usize;
        let offset = (y * self.image.width as usize + x) * 4;
        [
            self.image.pixels[offset + 2],
            self.image.pixels[offset + 1],
            self.image.pixels[offset],
        ]
    }
    /// Mean luminance (0 to 255) of what the window shows in the square reaching `reach`
    /// screen pixels around client point `p`: the capture, darkened where the shade covers it.
    /// Annotations are left out; the cross reads the picture they sit on.
    pub fn luminance_under(&self, p: Point, reach: f64) -> u8 {
        let f = self.to_frame(p);
        let reach = (self.to_frame(p + Point { x: reach, y: 0. }).x - f.x).max(0.5);
        let (width, height) = (self.image.width as f64, self.image.height as f64);
        let span = |c: f64, limit: f64| {
            let start = (c - reach).floor().clamp(0., limit - 1.) as usize;
            start..=((c + reach).floor().clamp(0., limit - 1.) as usize)
        };
        // A cursor at 500 % covers 120 pixels a side: 24 samples a side keep it cheap.
        let (xs, ys) = (span(f.x, width), span(f.y, height));
        let step = (xs.clone().count() / 24).max(1);
        let (mut sum, mut count) = (0u64, 0u64);
        for y in ys.step_by(step) {
            for x in xs.clone().step_by(step) {
                let i = (y * self.image.width as usize + x) * 4;
                let [b, g, r] = [0, 1, 2].map(|c| self.image.pixels[i + c] as u64);
                sum += (299 * r + 587 * g + 114 * b) / 1000;
                count += 1;
            }
        }
        let mean = (sum / count.max(1)) as f32;
        let shaded = self.region.is_none_or(|r| !r.contains(f));
        let shown = if shaded {
            mean * (1. - crate::overlay::SHADE_ALPHA)
        } else {
            mean
        };
        shown.round() as u8
    }
    /// The colour the pen, highlighter and bucket glyphs show: the pixel under the eyedropper
    /// while it hovers outside the toolbar, `current` otherwise.
    pub(crate) fn tool_tint(&self, current: [u8; 3]) -> [u8; 3] {
        if self.eyedropper && !self.toolbar_contains(self.pointer) {
            self.sample_color(self.loupe_center())
        } else {
            current
        }
    }
    /// Starts the toolbar's one-shot motion when what it marks has changed. Call it after each
    /// input and before asking `next_tick`; while the toolbar is hidden, nothing is recorded.
    /// `field_ink` is the open text field's ink (`Some(None)`: automatic), `None` without one.
    pub fn observe_toolbar(
        &mut self,
        current_color: [u8; 3],
        field_ink: Option<Option<[u8; 3]>>,
        now: std::time::Instant,
    ) {
        if self.region.is_none() || self.selecting {
            return;
        }
        let state = self.toolbar_state(current_color, field_ink);
        let buttons = self.buttons();
        self.toolbar_motion.observe(&buttons, state, now);
    }
    /// Milliseconds to the next toolbar or hint frame; `None` when nothing moves.
    pub fn next_tick(&self, now: std::time::Instant) -> Option<u32> {
        let ticks = [
            self.hover_hint.next_tick(now),
            self.toolbar_motion.next_tick(now),
        ];
        ticks.into_iter().flatten().min()
    }
    pub(crate) fn toolbar_state(
        &self,
        current: [u8; 3],
        field_ink: Option<Option<[u8; 3]>>,
    ) -> toolbar::Chosen {
        toolbar::Chosen {
            tool: self.editor.tool,
            eyedropper: self.eyedropper,
            current,
            pointer: self.pointer,
            label: self.label_colors(current, field_ink),
        }
    }
    /// Whether the box and text colour buttons show: the text box tool is in hand or a
    /// selected text sits on a panel.
    pub(crate) fn label_context(&self) -> bool {
        self.editor.tool == Tool::Label || self.selected_label().is_some()
    }
    /// The first selected text that sits on a panel.
    fn selected_label(&self) -> Option<&Object> {
        self.editor.objects().iter().find(|o| {
            self.editor.selected.contains(&o.id)
                && matches!(o.shape, Shape::Text { label: Some(_), .. })
        })
    }
    /// Whether a colour now goes to the text of text boxes rather than to their panel.
    pub(crate) fn inks(&self) -> bool {
        self.ink_target && !self.bucket && self.label_context()
    }
    /// The panel and ink the box and text colour buttons show: the open field's, else the
    /// first selected box's, else those the next box takes.
    fn label_colors(
        &self,
        current: [u8; 3],
        field_ink: Option<Option<[u8; 3]>>,
    ) -> ([u8; 3], [u8; 3]) {
        let ink = |forced: Option<[u8; 3]>| forced.unwrap_or_else(|| label_ink(current));
        match (field_ink, self.selected_label()) {
            (Some(forced), _) => (current, ink(forced)),
            (None, Some(o)) => (o.style.color, o.text_ink()),
            (None, None) => (current, ink(self.label_ink)),
        }
    }
    /// The annotations Ctrl+V lays down here: those copied, without blurs on a board, which
    /// has no capture under it to blur.
    pub fn clipboard(&self) -> Vec<Object> {
        self.copied_objects
            .iter()
            .filter(|o| !(self.board && o.is_blur()))
            .cloned()
            .collect()
    }
    pub fn rotation_handle(&self) -> Option<Point> {
        let margin = 8. * self.ui_scale();
        self.editor.rotation_handle().map(|p| Point {
            x: p.x
                .clamp(margin, (self.image.width as f64 - margin).max(margin)),
            y: p.y
                .clamp(margin, (self.image.height as f64 - margin).max(margin)),
        })
    }
    /// The stroke-size ring follows the pointer on the canvas only: over the
    /// toolbar the pointer targets buttons, so the ring is hidden, never displaced. Off the
    /// region too, where these tools work as well.
    pub fn width_ring_visible(&self) -> bool {
        !self.bucket
            && !self.toolbar_contains(self.pointer)
            && (self.width_preview
                || (self.region.is_some()
                    && !self.selecting
                    && matches!(self.editor.tool, Tool::Pen | Tool::Highlight | Tool::Eraser)))
    }
    /// True when a wheel notch would land on blurs alone, which have no width to change: the
    /// blur tool in hand with nothing selected, or a selection of blurs only.
    pub(crate) fn wheel_meets_only_blurs(&self) -> bool {
        let selected = &self.editor.selected;
        if selected.is_empty() {
            return self.editor.tool == Tool::Blur;
        }
        self.editor
            .objects()
            .iter()
            .filter(|o| selected.contains(&o.id))
            .all(Object::is_blur)
    }
    /// The wheel hint shows where a wheel notch would resize the tool about to draw.
    pub(crate) fn wheel_hint_visible(&self) -> bool {
        self.wheel_hint
            && !self.bucket
            && !self.eyedropper
            && self.picker.is_none()
            && self.region.is_some()
            && !self.selecting
            // Nothing to resize: the select tool, and the blur, whose strength is its zone's.
            && !matches!(self.editor.tool, Tool::Select | Tool::Blur)
            && !self.toolbar_contains(self.pointer)
            // A pointer gone from the window would pin the badge to a corner.
            && (0. ..self.image.width as f64).contains(&self.pointer.x)
            && (0. ..self.image.height as f64).contains(&self.pointer.y)
    }
    /// The size badge's "W × H" label.
    pub(crate) fn dimensions_label(&self) -> Option<String> {
        let size = self.region?.size();
        Some(format!("{} × {}", size.x as i32, size.y as i32))
    }
    /// Records how wide the badge's text is, through `measure(text, font size)`, before
    /// each frame: the texts of the two `fields` being edited, or else the label.
    pub fn measure_badge<E>(
        &mut self,
        fields: Option<[&str; 2]>,
        mut measure: impl FnMut(&str, f64) -> Result<f64, E>,
    ) -> Result<(), E> {
        let s = self.ui_scale();
        self.badge_text = match (fields, self.dimensions_label()) {
            (Some([width, height]), _) => {
                let size = crate::DimensionFields::FONT_SIZE * s;
                BadgeText::Fields([measure(width, size)?, measure(height, size)?])
            }
            (None, Some(label)) => BadgeText::Label(measure(&label, BADGE_LABEL_SIZE * s)?),
            (None, None) => BadgeText::Unmeasured,
        };
        Ok(())
    }
    /// Badge and full-screen button, side by side above the region's top-right corner;
    /// the badge as wide as its text, within the frame.
    fn region_badges(&self) -> Option<Badges> {
        // A board spans every screen: its size ("5760 × 1080") and a full-screen button said
        // nothing there (blind test of 2026-11-05).
        if self.board {
            return None;
        }
        let r = self.region?;
        let s = self.ui_scale();
        let (frame_width, frame_height) = (self.image.width as f64, self.image.height as f64);
        let height = (26. * s).min(frame_height);
        let button = height;
        let gap = 4. * s;
        let (pad, separator) = (BADGE_PADDING * s, BADGE_SEPARATOR * s);
        let room = (frame_width - gap - button).max(0.);
        // Whole logical pixels, so the text never wraps and the badge scales as one piece.
        let whole = |width: f64| (width / s).ceil() * s;
        let mut fields = None;
        let content = match self.badge_text {
            BadgeText::Unmeasured => 92. * s,
            BadgeText::Label(width) => whole(width),
            BadgeText::Fields(widths) => {
                let size = crate::DimensionFields::FONT_SIZE * s;
                let (_, reach) = crate::caret::reach(size, s);
                let [a, b] = widths.map(|w| whole((w + reach).max(BADGE_FIELD_MIN * s)));
                let shared = share(((room - 2. * pad - separator) / s).max(0.), a / s, b / s);
                let [a, b] = shared.map(|w| w * s);
                fields = Some([a, b]);
                a + separator + b
            }
        };
        let badge = (content + 2. * pad).min(room);
        let width = (badge + gap + button).min(frame_width);
        let min = Point {
            x: (r.max.x - width).clamp(0., frame_width - width),
            y: (r.min.y - height - 6. * s).clamp(0., frame_height - height),
        };
        let at = |x: f64, w: f64| Rect {
            min: Point { x, y: min.y },
            max: Point {
                x: x + w,
                y: min.y + height,
            },
        };
        let field = |x: f64, w: f64| Rect {
            min: Point {
                x,
                y: min.y + 3. * s,
            },
            max: Point {
                x: x + w,
                y: min.y + 23. * s,
            },
        };
        Some(Badges {
            badge: at(min.x, badge),
            button: at(min.x + width - button, button),
            fields: fields.map(|[a, b]| {
                let left = min.x + pad;
                [field(left, a), field(left + a + separator, b)]
            }),
        })
    }
    /// The two size fields while editing, each as wide as its text and caret.
    pub fn dimension_field_bounds(&self) -> Option<[Rect; 2]> {
        self.region_badges()?.fields
    }
    /// Clickable crop-size badge above the region's top-right corner.
    pub fn dimensions_bounds(&self) -> Option<Rect> {
        self.region_badges().map(|b| b.badge)
    }
    /// Button extending the region to its whole screen, next to the size badge.
    pub fn screen_button_bounds(&self) -> Option<Rect> {
        if self.selecting {
            return None;
        }
        self.region_badges().map(|b| b.button)
    }
    /// The full-screen toggle as a button, so it shares the toolbar hints.
    pub fn screen_button(&self) -> Option<Button> {
        let filled = self.screen_filled();
        self.screen_button_bounds().map(|bounds| Button {
            bounds,
            label: if filled {
                toolbar::RESTORE_SELECTION_LABEL
            } else {
                toolbar::FULL_SCREEN_LABEL
            },
            action: Action::FullScreen,
            panel: 2,
            active: filled,
            enabled: true,
        })
    }
    pub(crate) fn dimensions_button(&self) -> Option<Button> {
        if self.selecting {
            return None;
        }
        self.dimensions_bounds().map(|bounds| Button {
            bounds,
            label: Key::CaptureEditDimensions,
            action: Action::Dimensions,
            panel: 2,
            active: false,
            enabled: true,
        })
    }
    /// Button under the pointer for the shared delayed hint.
    pub fn hover_target(&self, p: Point) -> Option<(Action, Rect)> {
        if self.picker.is_some() {
            return None;
        }
        self.buttons()
            .into_iter()
            .chain(self.screen_button())
            .chain(self.dimensions_button())
            .find(|b| b.bounds.contains(p))
            .map(|b| (b.action, b.bounds))
    }
    /// True while the region is still the screen the button applied.
    pub(crate) fn screen_filled(&self) -> bool {
        self.screen_toggle
            .is_some_and(|(_, screen)| self.region == Some(screen))
    }
    /// Bounds of the monitor containing `point`, or the whole frame without monitors.
    pub(crate) fn screen_at(&self, point: Point) -> Rect {
        self.monitor_at(point).map_or(
            Rect {
                min: Point::default(),
                max: Point {
                    x: self.image.width as f64,
                    y: self.image.height as f64,
                },
            },
            |m| m.bounds,
        )
    }
    /// Monitor containing `point`, clipped to the frame; nearest monitor in a gap.
    fn monitor_at(&self, point: Point) -> Option<Monitor> {
        let frame = Rect {
            min: Point::default(),
            max: Point {
                x: self.image.width as f64,
                y: self.image.height as f64,
            },
        };
        let clip = |m: Rect, to: Rect| {
            let min = Point {
                x: m.min.x.max(to.min.x),
                y: m.min.y.max(to.min.y),
            };
            let max = Point {
                x: m.max.x.min(to.max.x),
                y: m.max.y.min(to.max.y),
            };
            (min.x < max.x && min.y < max.y).then_some(Rect { min, max })
        };
        let distance = |m: Rect| {
            let nearest = Point {
                x: point.x.clamp(m.min.x, m.max.x),
                y: point.y.clamp(m.min.y, m.max.y),
            };
            (nearest - point).length()
        };
        self.monitors
            .iter()
            .filter_map(|m| {
                clip(m.bounds, frame).map(|bounds| Monitor {
                    bounds,
                    scale: m.scale,
                })
            })
            .min_by(|a, b| {
                let contains = |m: Rect| {
                    point.x >= m.min.x
                        && point.x < m.max.x
                        && point.y >= m.min.y
                        && point.y < m.max.y
                };
                contains(b.bounds)
                    .cmp(&contains(a.bounds))
                    .then_with(|| distance(a.bounds).total_cmp(&distance(b.bounds)))
            })
    }
    /// Fills the screen containing the button, or restores the replaced region.
    pub(crate) fn toggle_screen(&mut self) {
        let Some(region) = self.region else {
            return;
        };
        if self.screen_filled() {
            self.region = self.screen_toggle.take().map(|(before, _)| before);
            return;
        }
        let anchor = self
            .screen_button_bounds()
            .map_or(region.center(), |r| r.center());
        let screen = self.screen_at(anchor);
        self.screen_toggle = Some((region, screen));
        self.region = Some(screen);
    }
    /// The topmost text a click of the text tool at frame point `p` edits.
    pub fn text_at(&self, p: Point) -> Option<&Object> {
        let objects = self.editor.objects();
        objects
            .iter()
            .rev()
            .find(|o| matches!(o.shape, Shape::Text { .. }) && o.hit(p, 5., objects))
    }
    /// Objects outlined as selectable: those the marquee being drawn would add, else the
    /// one a Select press under the pointer would pick.
    pub fn hovered(&self) -> Vec<u64> {
        if self.editor.marquee().is_some() {
            return self.editor.marquee_catch();
        }
        if self.bucket || !self.pointer_on_document() {
            return Vec::new();
        }
        let f = self.to_frame(self.pointer);
        self.editor.hover_at(f).into_iter().collect()
    }
    /// What a bucket click under the pointer would change; Alt held previews clearing.
    pub(crate) fn bucket_preview(&self) -> Option<FillChange> {
        if !self.bucket || !self.pointer_on_document() {
            return None;
        }
        let f = self.to_frame(self.pointer);
        if !self.region.is_some_and(|r| r.contains(f)) {
            return None;
        }
        self.editor.fill_change(f, self.editor.held().alt)
    }
    /// The pointer rests on the document itself: no panel, button, badge, region handle or
    /// pan between them.
    pub(crate) fn pointer_on_document(&self) -> bool {
        let p = self.pointer;
        let over = |r: Option<Rect>| r.is_some_and(|r| r.contains(p));
        !(self.selecting
            || self.picker.is_some()
            || self.eyedropper
            || self.panning()
            || self.space_held()
            || self.toolbar_contains(p)
            || over(self.screen_button_bounds())
            || over(self.dimensions_bounds())
            || self.region_handle(self.to_frame(p)).is_some())
    }
    /// Pointer shape for the canvas, once the size fields are ruled out.
    pub fn cursor(&self, p: Point) -> CursorKind {
        if self.picker.is_some() {
            // The wheel is a panel of its own: an arrow everywhere, a click outside applies.
            return CursorKind::Arrow;
        }
        if self.screen_button_bounds().is_some_and(|r| r.contains(p)) {
            return CursorKind::Hand;
        }
        if self.toolbar_contains(p) {
            return CursorKind::Arrow;
        }
        if self.eyedropper {
            return CursorKind::Cross;
        }
        if self.panning() || self.space_held() {
            return CursorKind::Move;
        }
        let f = self.to_frame(p);
        let (grip, reach) = (9. * self.pick_scale(), 7. * self.pick_scale());
        if let Some(index) = self.region_handle(f) {
            return match index {
                0 | 4 => CursorKind::ResizeNwse,
                2 | 6 => CursorKind::ResizeNesw,
                1 | 5 => CursorKind::ResizeNs,
                _ => CursorKind::ResizeWe,
            };
        }
        if self.recalled_region && self.region.is_some_and(|r| !r.contains(f)) {
            return CursorKind::Cross;
        }
        if self.bucket {
            return CursorKind::Cross;
        }
        if self.editor.anchor_at(f).is_some() {
            return CursorKind::Cross;
        }
        if matches!(self.editor.tool, Tool::Text | Tool::Label) {
            // A click on a text edits it, anywhere else it starts one: two shapes to tell.
            return if self.text_at(f).is_some() {
                CursorKind::IBeam
            } else {
                CursorKind::NewText
            };
        }
        if self
            .editor
            .bend_handle()
            .is_some_and(|(_, h)| (h - f).length() < grip)
        {
            return CursorKind::Move;
        }
        if self.editor.tool != Tool::Select {
            return CursorKind::Cross;
        }
        if self
            .rotation_handle()
            .is_some_and(|h| (h - f).length() <= grip)
        {
            return CursorKind::Hand;
        }
        if self.editor.selected.len() == 1
            && let Some(object) = self
                .editor
                .objects()
                .iter()
                .find(|o| o.id == self.editor.selected[0])
            && let Some((a, b)) = arrow_points(object, self.editor.objects())
            && ((f - a).length() < grip || (f - b).length() < grip)
        {
            return CursorKind::Move;
        }
        if let Some(frame) = self.editor.selection_frame()
            && let Some(i) = frame
                .corners()
                .iter()
                .position(|c| (*c - f).length() < grip)
        {
            return if i % 2 == 0 {
                CursorKind::ResizeNwse
            } else {
                CursorKind::ResizeNesw
            };
        }
        if self
            .editor
            .objects()
            .iter()
            .any(|o| o.hit(f, reach, self.editor.objects()))
        {
            return CursorKind::Move;
        }
        CursorKind::Arrow
    }
}
/// Corners and edge midpoints, clockwise from the top-left corner.
/// Widths `a` and `b` within `room`: both whole if they fit, else the narrower whole and
/// the wider taking the rest, or half each.
fn share(room: f64, a: f64, b: f64) -> [f64; 2] {
    let half = (room / 2.).floor();
    match () {
        _ if a + b <= room => [a, b],
        _ if a <= half => [a, room - a],
        _ if b <= half => [room - b, b],
        _ => [half, room - half],
    }
}
pub(crate) fn region_handles(r: Rect) -> [Point; 8] {
    let mid = (r.min + r.max) * 0.5;
    [
        r.min,
        Point {
            x: mid.x,
            y: r.min.y,
        },
        Point {
            x: r.max.x,
            y: r.min.y,
        },
        Point {
            x: r.max.x,
            y: mid.y,
        },
        r.max,
        Point {
            x: mid.x,
            y: r.max.y,
        },
        Point {
            x: r.min.x,
            y: r.max.y,
        },
        Point {
            x: r.min.x,
            y: mid.y,
        },
    ]
}

#[cfg(test)]
mod tests;
