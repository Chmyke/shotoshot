//! ROLE: Portable state of one capture session: frame, selected region, editor and
//! interface flags. Platform sessions wrap it with their native widgets and renderer.
//! OWNER: One capture; dropped when it ends.
use crate::{
    Image,
    toolbar::{self, Action, Button, HoverHint, Motion},
    wheel,
};
use shotoshot_core::{Editor, FillChange, Object, Point, Rect, Shape, Tool, arrow_points};
use shotoshot_text::Key;

/// Margin of the size badge on each side of what it shows, in logical pixels.
const BADGE_PADDING: f64 = 8.;
/// Room for the "×" between the two size fields, in logical pixels.
const BADGE_SEPARATOR: f64 = 14.;
/// Narrowest size field, so an emptied one can still be clicked, in logical pixels.
const BADGE_FIELD_MIN: f64 = 8.;
/// Font size of the size badge's "W × H" label, in logical pixels.
pub const BADGE_LABEL_SIZE: f64 = 13.;

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
    pub(crate) wheel_travel: crate::wheel::Notches,
    pub region_resize: Option<usize>,
    pub toolbar_visible: bool,
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
            wheel_travel: Default::default(),
            region_resize: None,
            toolbar_visible: true,
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
        }
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
    /// current stroke width and text size, so an annotation looks the same at every scale.
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
    pub fn buttons(&self) -> Vec<Button> {
        if !self.toolbar_visible || self.selecting {
            return Vec::new();
        }
        // The toolbar goes around the region and the badges stuck to it, never over them.
        let occupied = |r: Rect| {
            self.region_badges()
                .map_or(r, |b| r.union(b.badge).union(b.button))
        };
        let mut buttons = self
            .region
            .map(|r| {
                let screen = self.toolbar_screen(r);
                toolbar::buttons_in(occupied(r), screen, self.ui_scale(), self.board)
            })
            .unwrap_or_default();
        for b in &mut buttons {
            match b.action {
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
    pub fn clamp(&self, p: Point) -> Point {
        Point {
            x: p.x.clamp(0., self.image.width as f64),
            y: p.y.clamp(0., self.image.height as f64),
        }
    }
    pub fn region_handle(&self, p: Point) -> Option<usize> {
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
    pub fn tool_tint(&self, current: [u8; 3]) -> [u8; 3] {
        if self.eyedropper && !self.toolbar_contains(self.pointer) {
            self.sample_color(self.loupe_center())
        } else {
            current
        }
    }
    /// Starts the toolbar's one-shot motion when what it marks has changed. Call it after each
    /// input and before asking `next_tick`; while the toolbar is hidden, nothing is recorded.
    pub fn observe_toolbar(&mut self, current_color: [u8; 3], now: std::time::Instant) {
        if self.region.is_none() || self.selecting {
            return;
        }
        let (buttons, state) = (self.buttons(), self.toolbar_state(current_color));
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
    pub(crate) fn toolbar_state(&self, current: [u8; 3]) -> toolbar::Chosen {
        toolbar::Chosen {
            tool: self.editor.tool,
            eyedropper: self.eyedropper,
            current,
            pointer: self.pointer,
        }
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
    /// The size badge's "W × H" label.
    pub fn dimensions_label(&self) -> Option<String> {
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
    pub fn screen_filled(&self) -> bool {
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
    pub fn toggle_screen(&mut self) {
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
    pub fn bucket_preview(&self) -> Option<FillChange> {
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
        if self.bucket {
            return CursorKind::Cross;
        }
        if self.editor.tool == Tool::Text {
            // A click on a text edits it, anywhere else it starts one: two shapes to tell.
            return if self.text_at(f).is_some() {
                CursorKind::IBeam
            } else {
                CursorKind::NewText
            };
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
pub fn region_handles(r: Rect) -> [Point; 8] {
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
mod tests {
    use super::*;
    fn capture() -> Capture {
        let mut c = Capture::new(Image::solid(900, 600, [0, 0, 0, 255]));
        c.region = Rect::new(Point { x: 100., y: 100. }, Point { x: 600., y: 400. });
        c.selecting = false;
        c
    }
    #[test]
    fn the_pointer_is_an_arrow_everywhere_while_the_wheel_is_open() {
        let mut c = Capture::new(Image::solid(900, 600, [0, 0, 0, 255]));
        c.region = Rect::new(Point { x: 100., y: 100. }, Point { x: 600., y: 400. });
        c.selecting = false;
        c.pointer = Point { x: 300., y: 300. };
        let mut host = crate::test_host::FakeHost::default();
        c.action(&mut host, Action::CustomColor);
        let inside = c.picker.as_ref().unwrap().bounds().center();
        let outside = Point { x: 50., y: 50. };
        assert_eq!(c.cursor(inside), CursorKind::Arrow);
        assert_eq!(c.cursor(outside), CursorKind::Arrow);
        c.picker = None;
        c.eyedropper = true;
        assert_eq!(c.cursor(outside), CursorKind::Cross);
    }
    #[test]
    fn tool_glyphs_take_the_colour_under_the_hovering_eyedropper() {
        let mut c = capture();
        let at = Point { x: 300., y: 250. };
        let offset = (250 * 900 + 300) * 4;
        c.image.pixels[offset..offset + 4].copy_from_slice(&[30, 140, 255, 255]);
        c.pointer = at;
        let red = [237, 79, 92];
        assert_eq!(c.tool_tint(red), red, "no eyedropper: the drawing colour");
        c.eyedropper = true;
        assert_eq!(
            c.tool_tint(red),
            [255, 140, 30],
            "the pixel under the eyedropper"
        );
        c.pointer = c.buttons()[0].bounds.center();
        assert_eq!(
            c.tool_tint(red),
            red,
            "over the toolbar a click picks a button"
        );
    }
    #[test]
    fn region_handles_drive_resize_cursors() {
        let c = capture();
        assert_eq!(c.cursor(Point { x: 100., y: 100. }), CursorKind::ResizeNwse);
        assert_eq!(c.cursor(Point { x: 600., y: 250. }), CursorKind::ResizeWe);
        assert_eq!(c.cursor(Point { x: 350., y: 400. }), CursorKind::ResizeNs);
        assert_eq!(c.cursor(Point { x: 300., y: 250. }), CursorKind::Cross);
    }
    #[test]
    fn the_text_tool_shows_whether_a_click_edits_or_creates() {
        let mut c = capture();
        c.editor.add_text_measured(
            Point { x: 200., y: 200. },
            "note".into(),
            Point { x: 60., y: 30. },
        );
        c.editor.tool = Tool::Text;
        let on_text = Point { x: 230., y: 215. };
        assert!(c.text_at(on_text).is_some());
        assert_eq!(c.cursor(on_text), CursorKind::IBeam, "edits");
        assert_eq!(
            c.cursor(Point { x: 400., y: 300. }),
            CursorKind::NewText,
            "creates"
        );
    }
    #[test]
    fn toolbar_hides_size_ring_and_shows_arrow() {
        let mut c = capture();
        c.width_preview = true;
        c.pointer = Point { x: 300., y: 250. };
        assert!(c.width_ring_visible());
        let button = c.buttons()[0].bounds.center();
        c.pointer = button;
        assert!(!c.width_ring_visible());
        assert_eq!(c.cursor(button), CursorKind::Arrow);
    }
    fn three_screens() -> Capture {
        let mut c = capture();
        c.image = Image::solid(5760, 1080, [0, 0, 0, 255]);
        c.monitors = (0..3)
            .map(|i| {
                let x = i as f64 * 1920.;
                Rect::new(
                    Point { x, y: 0. },
                    Point {
                        x: x + 1920.,
                        y: 1080.,
                    },
                )
                .unwrap()
                .into()
            })
            .collect();
        c
    }
    fn point(x: f64, y: f64) -> Point {
        Point { x, y }
    }
    /// A 250 % laptop panel beside a 100 % television.
    fn laptop_and_television() -> Capture {
        let mut c = capture();
        c.image = Image::solid(5760, 2160, [0, 0, 0, 255]);
        c.monitors = vec![
            Monitor {
                bounds: Rect::new(point(0., 0.), point(3840., 2160.)).unwrap(),
                scale: 2.5,
            },
            Monitor {
                bounds: Rect::new(point(3840., 0.), point(5760., 1080.)).unwrap(),
                scale: 1.,
            },
        ];
        c
    }
    #[test]
    fn interface_scale_is_that_of_the_screen_under_the_region_then_the_pointer() {
        assert_eq!(capture().ui_scale(), 1., "no monitor list: 100 %");
        let mut c = laptop_and_television();
        assert_eq!(c.ui_scale(), 2.5);
        c.region = Rect::new(point(4000., 100.), point(4600., 500.));
        assert_eq!(c.ui_scale(), 1.);
        // A region across both screens follows the one holding its centre.
        c.region = Rect::new(point(3000., 100.), point(4000., 500.));
        assert_eq!(c.ui_scale(), 2.5);
        c.region = None;
        c.pointer = point(5000., 500.);
        assert_eq!(c.ui_scale(), 1.);
        c.pointer = point(100., 100.);
        assert_eq!(c.ui_scale(), 2.5);
        for (reported, used) in [(0., 1.), (f64::NAN, 1.), (0.5, 1.), (9., 5.)] {
            c.monitors[0].scale = reported;
            assert_eq!(c.ui_scale(), used, "a misreported {reported} stays usable");
        }
    }
    #[test]
    fn buttons_badges_and_handles_keep_their_apparent_size_at_every_scale() {
        let near = |a: f64, b: f64| (a - b).abs() < 1e-9;
        for scale in [1., 1.25, 1.5, 1.75, 2., 2.5, 3.] {
            let mut c = capture();
            c.image = Image::solid(3840, 2160, [0, 0, 0, 255]);
            let screen = Rect::new(point(0., 0.), point(3840., 2160.)).unwrap();
            c.monitors = vec![Monitor {
                bounds: screen,
                scale,
            }];
            c.region = Rect::new(point(800., 600.), point(2400., 1500.));
            let region = c.region.unwrap();
            let buttons = c.buttons();
            assert_eq!(buttons.len(), 25);
            for b in &buttons {
                let size = b.bounds.size();
                assert!(
                    near(size.x, 30. * scale) && near(size.y, 30. * scale),
                    "a button is 30 logical pixels at {scale}, not {size:?}"
                );
            }
            for panel in toolbar::panels(&buttons, scale) {
                assert!(screen.contains_rect(panel), "panel on screen at {scale}");
            }
            let badge = c.dimensions_bounds().unwrap().size();
            assert!(near(badge.x, 108. * scale) && near(badge.y, 26. * scale));
            let button = c.screen_button_bounds().unwrap().size();
            assert!(near(button.x, 26. * scale) && near(button.y, 26. * scale));
            let inside = region.min + point(7.9 * scale, 0.);
            let outside = region.min + point(8.1 * scale, 0.);
            assert_eq!(
                c.region_handle(inside),
                Some(0),
                "handle grabbed at {scale}"
            );
            assert_eq!(c.region_handle(outside), None);
            let loupe = c.magnifier_bounds().size();
            assert!(near(loupe.x, 126. * scale) && near(loupe.y, 164. * scale));
        }
    }
    #[test]
    fn screen_button_fills_the_region_monitor_and_restores_on_second_press() {
        let mut c = three_screens();
        let small = Rect::new(Point { x: 2000., y: 300. }, Point { x: 2400., y: 500. });
        c.region = small;
        c.toggle_screen();
        assert_eq!(c.region, c.monitors.get(1).map(|m| m.bounds));
        assert!(c.screen_filled());
        c.toggle_screen();
        assert_eq!(c.region, small);
        assert!(!c.screen_filled());
    }
    #[test]
    fn screen_button_picks_its_own_monitor_even_when_most_of_region_is_elsewhere() {
        let mut c = three_screens();
        c.region = Rect::new(Point { x: 200., y: 100. }, Point { x: 2000., y: 900. });
        assert!(
            c.monitors[1]
                .bounds
                .contains(c.screen_button_bounds().unwrap().center())
        );
        c.toggle_screen();
        assert_eq!(c.region, c.monitors.get(1).map(|m| m.bounds));
    }
    #[test]
    fn toolbar_stays_on_a_single_selected_monitor_but_not_for_a_cross_monitor_region() {
        let mut c = three_screens();
        c.region = c.monitors.get(1).map(|m| m.bounds);
        assert!(
            toolbar::panels(&c.buttons(), 1.)
                .iter()
                .all(|panel| c.monitors[1].bounds.contains_rect(*panel))
        );

        c.region = Rect::new(Point { x: 1800., y: 300. }, Point { x: 2200., y: 700. });
        assert_eq!(
            toolbar::panels(&c.buttons(), 1.),
            toolbar::panels(
                &toolbar::buttons(c.region.unwrap(), c.image.width, c.image.height),
                1.
            )
        );
    }
    #[test]
    fn a_toolbar_following_the_pointer_stays_on_the_pointer_screen_of_a_frame_wide_region() {
        let mut c = three_screens();
        c.region = Rect::new(Point::default(), Point { x: 5760., y: 1080. });
        c.board = true;
        for (index, x) in [(0, 300.), (2, 4000.), (1, 2500.)] {
            c.pointer = Point { x, y: 500. };
            let screen = c.monitors[index].bounds;
            let panels = toolbar::panels(&c.buttons(), 1.);
            assert!(!panels.is_empty());
            assert!(
                panels.iter().all(|panel| screen.contains_rect(*panel)),
                "pointer on screen {index}: {panels:?} in {screen:?}"
            );
        }
        // Without the flag, a frame-wide region keeps its placement beside the region.
        c.board = false;
        assert_eq!(
            toolbar::panels(&c.buttons(), 1.),
            toolbar::panels(
                &toolbar::buttons(c.region.unwrap(), c.image.width, c.image.height),
                1.
            )
        );
    }
    #[test]
    fn only_a_board_offers_clear_board_and_puts_it_before_close() {
        let mut c = three_screens();
        c.region = Rect::new(Point::default(), Point { x: 5760., y: 1080. });
        assert!(
            !c.buttons().iter().any(|b| b.action == Action::ClearBoard),
            "a capture has no board to clear"
        );
        c.board = true;
        let buttons = c.buttons();
        let clear = buttons
            .iter()
            .position(|b| b.action == Action::ClearBoard)
            .expect("the board's clear button");
        let close = buttons
            .iter()
            .position(|b| b.action == Action::Close)
            .unwrap();
        assert_eq!(clear + 1, close, "clear sits right before close");
        assert_eq!(
            buttons[clear].panel, buttons[close].panel,
            "in the action panel"
        );
        assert!(buttons[clear].enabled);
    }
    #[test]
    fn monitor_selection_follows_points_including_shared_edges_and_frame_clipping() {
        let mut c = three_screens();
        for (index, x) in [0., 1920., 3840.].into_iter().enumerate() {
            assert_eq!(c.screen_at(Point { x, y: 100. }), c.monitors[index].bounds);
            c.region = Rect::new(
                Point { x: 10., y: 100. },
                Point {
                    x: x + 500.,
                    y: 700.,
                },
            );
            c.toggle_screen();
            assert_eq!(c.region, Some(c.monitors[index].bounds));
        }
        c.monitors = vec![
            Rect {
                min: Point { x: -100., y: -100. },
                max: Point { x: 500., y: 500. },
            }
            .into(),
        ];
        assert_eq!(
            c.screen_at(Point { x: 900., y: 900. }),
            Rect {
                min: Point::default(),
                max: Point { x: 500., y: 500. }
            }
        );
    }
    #[test]
    fn dimensions_badge_uses_the_delayed_hover_system() {
        let mut c = capture();
        let bounds = c.dimensions_bounds().unwrap();
        let target = c.hover_target(bounds.center());
        assert!(target.is_some());
        let now = std::time::Instant::now();
        c.hover_hint.update(target, now);
        assert!(c.hover_hint.visible(now).is_none());
        assert!(
            c.hover_hint
                .visible(now + std::time::Duration::from_millis(250))
                .is_some()
        );
        c.selecting = true;
        assert!(c.hover_target(bounds.center()).is_none());
    }
    #[test]
    fn dimensions_badge_is_exactly_as_wide_as_its_label() {
        let mut c = capture();
        // 7 px a character, measured at the label size.
        let mut sizes = Vec::new();
        let mut badge = |c: &mut Capture| {
            c.measure_badge(None, |text, size| {
                sizes.push(size);
                Ok::<_, ()>(text.chars().count() as f64 * 7.)
            })
            .unwrap();
            c.dimensions_bounds().unwrap()
        };
        let wide = badge(&mut c);
        assert_eq!(c.dimensions_label().unwrap(), "500 × 300");
        assert_eq!(wide.size().x, 8. + 9. * 7. + 8.);
        let button = c.screen_button_bounds().unwrap();
        assert_eq!(button.min.x - wide.max.x, 4.);
        assert_eq!(button.max.x, 600.);
        // Two digits fewer, two digits of badge fewer; the right edge stays put.
        c.region = Rect::new(Point { x: 550., y: 370. }, Point { x: 600., y: 400. });
        let narrow = badge(&mut c);
        assert_eq!(narrow.size().x, wide.size().x - 2. * 7.);
        assert_eq!(narrow.max.x, wide.max.x);
        assert_eq!(sizes, [BADGE_LABEL_SIZE; 2]);
    }
    #[test]
    fn screen_button_expands_again_after_the_region_was_edited() {
        let mut c = three_screens();
        c.toggle_screen();
        c.region = Rect::new(Point { x: 10., y: 10. }, Point { x: 50., y: 50. });
        assert!(!c.screen_filled());
        c.toggle_screen();
        assert_eq!(c.region, c.monitors.first().map(|m| m.bounds));
    }
    #[test]
    fn screen_button_uses_the_frame_without_monitor_list() {
        let mut c = capture();
        c.toggle_screen();
        assert_eq!(
            c.region,
            Rect::new(Point::default(), Point { x: 900., y: 600. })
        );
    }
    #[test]
    fn screen_button_sits_right_of_the_badge_and_hides_while_selecting() {
        let mut c = capture();
        let badge = c.dimensions_bounds().unwrap();
        let button = c.screen_button_bounds().unwrap();
        assert!(badge.max.x < button.min.x && button.max.x == 600.);
        assert_eq!(c.cursor(button.center()), CursorKind::Hand);
        c.selecting = true;
        assert!(c.screen_button_bounds().is_none());
    }
    #[test]
    fn screen_button_hint_names_its_shortcut_and_state() {
        let mut c = capture();
        let button = c.screen_button_bounds().unwrap();
        assert_eq!(
            c.hover_target(button.center()),
            Some((Action::FullScreen, button))
        );
        assert_eq!(c.screen_button().unwrap().label, toolbar::FULL_SCREEN_LABEL);
        c.toggle_screen();
        let button = c.screen_button().unwrap();
        assert!(button.active && button.label == toolbar::RESTORE_SELECTION_LABEL);
    }
    #[test]
    fn the_toolbar_never_covers_the_size_badge_or_the_full_screen_button() {
        // A 1080p screen at 100 % and a 4K one at 200 %, swept in steps of 60 logical pixels.
        for scale in [1., 2.] {
            let screen = Point { x: 1920., y: 1080. } * scale;
            let mut c = Capture::new(Image::solid(screen.x as u32, screen.y as u32, [0; 4]));
            c.selecting = false;
            c.monitors = vec![Monitor {
                bounds: Rect::new(Point::default(), screen).unwrap(),
                scale,
            }];
            for (w, h) in [(80., 60.), (400., 300.), (900., 700.), (1920., 1080.)] {
                let size = Point { x: w, y: h } * scale;
                for x in (0..=(1920. - w) as i32).step_by(60) {
                    for y in (0..=(1080. - h) as i32).step_by(60) {
                        let min = Point {
                            x: x as f64,
                            y: y as f64,
                        } * scale;
                        c.region = Rect::new(min, min + size);
                        let panels = toolbar::panels(&c.buttons(), c.ui_scale());
                        let badges = [c.dimensions_bounds(), c.screen_button_bounds()];
                        for badge in badges.into_iter().flatten() {
                            assert!(
                                panels.iter().all(|p| !toolbar::overlaps(*p, badge)),
                                "{badge:?} under the toolbar {panels:?} for {:?} at {scale}",
                                c.region
                            );
                        }
                    }
                }
            }
        }
    }
    #[test]
    fn every_state_hint_is_among_the_measured_ones() {
        use shotoshot_text::Language;
        let listed = toolbar::all_tooltips_in(Language::En);
        let mut c = capture();
        let mut seen = Vec::new();
        for (live, reopened, searching, board) in [
            (false, false, false, false),
            (true, false, true, true),
            (false, true, false, false),
        ] {
            (c.live, c.reopened, c.searching, c.board) = (live, reopened, searching, board);
            for filled in [false, true] {
                if c.screen_filled() != filled {
                    c.toggle_screen();
                }
                let buttons = c.buttons().into_iter().chain(c.screen_button());
                seen.extend(
                    buttons
                        .chain(c.dimensions_button())
                        .map(|b| toolbar::tooltip_in(&b, Language::En)),
                );
            }
        }
        for hint in &seen {
            assert!(listed.contains(hint), "{hint} is never measured");
        }
        for hint in [
            "Nothing to undo · Ctrl+Z",
            "Google search in progress · Ctrl+G",
            "Live capture is for new captures",
            "Live capture on · F2 to freeze",
            "Capture frozen · F2 for live capture",
            "Back to previous selection · F",
            "Edit dimensions · expressions: 800+20, 1920/2",
            "Clear board · Ctrl+Z restores",
        ] {
            assert!(seen.iter().any(|s| s == hint), "{hint} not shown");
        }
    }
    #[test]
    fn sampled_color_is_rgb_from_bgra_and_clamped() {
        let mut c = capture();
        c.image.pixels[..4].copy_from_slice(&[10, 20, 30, 255]);
        assert_eq!(c.sample_color(Point { x: -5., y: -5. }), [30, 20, 10]);
    }
    #[test]
    fn the_luminance_under_the_cross_is_what_the_shade_leaves_of_the_capture() {
        let mut c = Capture::new(Image::solid(900, 600, [255, 255, 255, 255]));
        let at = Point { x: 300., y: 250. };
        // Before any region, the shade covers the white page: a mid grey, the cross's trap.
        assert_eq!(c.luminance_under(at, 12.), 133);
        c.region = Rect::new(Point { x: 100., y: 100. }, Point { x: 600., y: 400. });
        assert_eq!(
            c.luminance_under(at, 12.),
            255,
            "inside the region: no shade"
        );
        assert_eq!(c.luminance_under(Point { x: 50., y: 50. }, 12.), 133);
        // Left half black: the mean of what the square covers, 13 white columns of 25.
        for y in 0..600 {
            c.image.pixels[y * 900 * 4..(y * 900 + 300) * 4].fill(0);
        }
        let mean = c.luminance_under(at, 12.);
        assert!((125..=140).contains(&mean), "{mean}");
        // Pure blue is dark, pure yellow light, as the eye sees them.
        c.image
            .pixels
            .chunks_mut(4)
            .for_each(|p| p.copy_from_slice(&[255, 0, 0, 255]));
        assert_eq!(c.luminance_under(at, 12.), 29);
        c.image
            .pixels
            .chunks_mut(4)
            .for_each(|p| p.copy_from_slice(&[0, 255, 255, 255]));
        assert_eq!(c.luminance_under(at, 12.), 225);
        // Clamped at the corner, never past the image.
        assert_eq!(c.luminance_under(Point { x: -40., y: 900. }, 12.), 117);
    }
}
