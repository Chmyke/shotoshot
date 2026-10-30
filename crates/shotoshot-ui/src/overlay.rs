//! ROLE: Portable painting of the capture overlay: shade, region frame, selection,
//! toolbar, stroke-size ring and the initial hint. Native widgets paint themselves.
//! INVARIANTS: Interface sizes are 96-DPI logical pixels times `Capture::ui_scale`;
//! only the capture and its annotations are in raw pixels (`scale_tests`).
use crate::{Canvas, Capture, Ink, region_handles, toolbar};
use shotoshot_core::{FillChange, Object, Paint, Point, Rect, Tool, paths};
use shotoshot_text::{Key, Language, Text, language, tr, tr_in};

const ACCENT: Ink = Ink::solid([100, 170, 255]);
/// Opacity of the black over everything outside the region.
pub(crate) const SHADE_ALPHA: f32 = 0.48;
const SHADE: Ink = Ink::with_alpha([0, 0, 0], SHADE_ALPHA);
const DARK: Ink = Ink::solid([32, 32, 32]);

fn square(center: Point, half: f64) -> Rect {
    Rect {
        min: center - Point { x: half, y: half },
        max: center + Point { x: half, y: half },
    }
}

impl Capture {
    fn screen(&self) -> Rect {
        Rect {
            min: Point::default(),
            max: Point {
                x: self.image.width as f64,
                y: self.image.height as f64,
            },
        }
    }
    /// All that lies under the annotations: the shade outside the region (the whole
    /// frame before a selection) and, inside it, the background's coats when there are any.
    pub fn draw_underlay<C: Canvas>(&self, canvas: &mut C) -> Result<(), C::Error> {
        let screen = self.screen();
        let Some(r) = self.region else {
            return canvas.fill_rect(screen, SHADE);
        };
        let objects = self.editor.objects();
        for coat in self.editor.background() {
            canvas.fill_rect_except(r, &holes(coat, objects, 0), Ink::solid(coat.color))?;
        }
        for outside in [
            Rect {
                min: screen.min,
                max: Point {
                    x: screen.max.x,
                    y: r.min.y,
                },
            },
            Rect {
                min: Point { x: 0., y: r.max.y },
                max: screen.max,
            },
            Rect {
                min: Point { x: 0., y: r.min.y },
                max: Point {
                    x: r.min.x,
                    y: r.max.y,
                },
            },
            Rect {
                min: Point {
                    x: r.max.x,
                    y: r.min.y,
                },
                max: Point {
                    x: screen.max.x,
                    y: r.max.y,
                },
            },
        ] {
            canvas.fill_rect(outside, SHADE)?;
        }
        Ok(())
    }
    /// The objects as exported, except `skip`, with those a sweep touched drawn faded in
    /// their place so the hand sees what the release will remove.
    pub fn draw_annotations<C: Canvas>(
        &self,
        canvas: &mut C,
        skip: Option<u64>,
    ) -> Result<(), C::Error> {
        let erasing = self.editor.erasing();
        if erasing.is_empty() {
            canvas.annotations(self.editor.objects(), skip)
        } else {
            canvas.annotations_fading(self.editor.objects(), skip, erasing)
        }
    }
    /// Region border and handles, then the object selection unless text is being edited.
    pub fn draw_frame<C: Canvas>(
        &self,
        canvas: &mut C,
        show_selection: bool,
    ) -> Result<(), C::Error> {
        self.draw_frame_scaled(canvas, show_selection, self.ui_scale())
    }
    /// `draw_frame` with handle and line sizes at `scale` frame pixels per interface pixel:
    /// under a zoomed view, the interface scale over the zoom keeps them their size on screen.
    pub fn draw_frame_scaled<C: Canvas>(
        &self,
        canvas: &mut C,
        show_selection: bool,
        scale: f64,
    ) -> Result<(), C::Error> {
        let Some(r) = self.region else {
            return Ok(());
        };
        let s = scale;
        canvas.stroke_rect(r, ACCENT, s)?;
        for p in region_handles(r) {
            canvas.fill_rect(square(p, 4. * s), Ink::solid([250, 250, 250]))?;
            canvas.stroke_rect(square(p, 4. * s), DARK, 1.5 * s)?;
        }
        self.draw_bucket_preview(canvas, s)?;
        if show_selection {
            // Selectable, not selected: the selection's frame in dashes, without handles
            // (Mickaël's choice on the 2026-10-17 board: H2).
            for id in self.hovered() {
                if let Some(frame) = self.editor.frame_of(id) {
                    dashed(canvas, frame.outline(3. * s), ACCENT, s)?;
                }
            }
        }
        if show_selection && let Some(frame) = self.editor.selection_frame() {
            if frame.angle == 0. {
                canvas.stroke_rect(frame.bounds.expanded(3. * s), ACCENT, s)?;
            } else {
                // A single turned object keeps a frame turned with it.
                let outline = frame.outline(3. * s);
                for (i, a) in outline.iter().enumerate() {
                    canvas.line(*a, outline[(i + 1) % 4], ACCENT, s)?;
                }
            }
            for p in frame.corners() {
                canvas.fill_rect(square(p, 4. * s), ACCENT)?;
            }
            if let Some(handle) = self.rotation_handle() {
                canvas.line(frame.top(), handle, ACCENT, s)?;
                let radius = Point {
                    x: 6. * s,
                    y: 6. * s,
                };
                canvas.fill_ellipse(handle, radius, DARK)?;
                canvas.stroke_ellipse(handle, radius, ACCENT, 2. * s)?;
            }
        }
        // The middle of the selected arrow, or of the one just drawn, which bends it.
        if show_selection && let Some((_, middle)) = self.editor.bend_handle() {
            let radius = Point {
                x: 5. * s,
                y: 5. * s,
            };
            canvas.fill_ellipse(middle, radius, ACCENT)?;
            canvas.stroke_ellipse(middle, radius, Ink::solid([250, 250, 250]), 1.5 * s)?;
        }
        if show_selection {
            self.draw_anchors(canvas, s)?;
        }
        if let Some(m) = self.editor.marquee() {
            canvas.stroke_rect(m, ACCENT, s)?;
        }
        Ok(())
    }
    /// The four anchors of the text box an arrow can be pulled out of, the one under the
    /// pointer larger and filled with a plus; while one is pulled, the object its end would
    /// attach to in the hover's dashes.
    fn draw_anchors<C: Canvas>(&self, canvas: &mut C, s: f64) -> Result<(), C::Error> {
        if let Some(id) = self.editor.connect_target()
            && let Some(frame) = self.editor.frame_of(id)
        {
            dashed(canvas, frame.outline(3. * s), ACCENT, s)?;
        }
        if !self.pointer_on_document() {
            return Ok(());
        }
        let f = self.to_frame(self.pointer);
        let Some(id) = self.editor.anchor_owner(f) else {
            return Ok(());
        };
        let Some(anchors) = self.editor.anchors(id) else {
            return Ok(());
        };
        let hot = self.editor.anchor_at(f).is_some();
        let white = Ink::solid([250, 250, 250]);
        for a in anchors {
            let near = hot && (a - f).length() < 9. * s;
            let r = if near { 6. } else { 4. } * s;
            let radius = Point { x: r, y: r };
            canvas.fill_ellipse(a, radius, if near { ACCENT } else { white })?;
            canvas.stroke_ellipse(a, radius, ACCENT, 1.5 * s)?;
            if near {
                let arm = 2.5 * s;
                canvas.line(
                    a - Point { x: arm, y: 0. },
                    a + Point { x: arm, y: 0. },
                    white,
                    1.5 * s,
                )?;
                canvas.line(
                    a - Point { x: 0., y: arm },
                    a + Point { x: 0., y: arm },
                    white,
                    1.5 * s,
                )?;
            }
        }
        Ok(())
    }
    /// The outline of what a bucket click under the pointer would change, in dashes: in the
    /// bucket's colour for a stroke it recolours or a zone it fills, in the accent for a fill
    /// it removes (Mickaël, 2026-10-23: a translucent colour read as a transparent fill).
    fn draw_bucket_preview<C: Canvas>(&self, canvas: &mut C, s: f64) -> Result<(), C::Error> {
        let objects = self.editor.objects();
        let Some(region) = self.region else {
            return Ok(());
        };
        // The region's own border is solid: the capture's outline sits just inside it.
        let capture = || {
            let mut p = region.expanded(-3. * s).corners().to_vec();
            p.push(p[0]);
            p
        };
        let (mut outline, ink) = match self.bucket_preview() {
            Some(FillChange::Ink { index, color }) if objects[index].style.color != color => {
                (self.contour(index, s), color)
            }
            Some(FillChange::Pour { zone, coat }) => match zone {
                Some(index) => {
                    let mut outline = self.contour(index, s);
                    outline.extend(holes(&coat, objects, index + 1));
                    (outline, coat.color)
                }
                None => {
                    let mut outline = holes(&coat, objects, 0);
                    outline.push(capture());
                    (outline, coat.color)
                }
            },
            Some(FillChange::Remove { zone, coat }) => match zone {
                Some(index) => {
                    let mut outline = self.contour(index, s);
                    if let Some(coat) = objects[index].fill.get(coat) {
                        outline.extend(holes(coat, objects, index + 1));
                    }
                    (outline, ACCENT.rgb)
                }
                None => {
                    let mut outline = match self.editor.background().get(coat) {
                        Some(coat) => holes(coat, objects, 0),
                        None => Vec::new(),
                    };
                    outline.push(capture());
                    (outline, ACCENT.rgb)
                }
            },
            _ => return Ok(()),
        };
        outline.retain(|p| p.len() > 1);
        let pieces: Vec<_> = outline.iter().flat_map(|p| dashes(p, s)).collect();
        // A halo of the opposite lightness keeps the dashes readable on any capture.
        let light = 0.299 * ink[0] as f64 + 0.587 * ink[1] as f64 + 0.114 * ink[2] as f64 > 128.;
        let halo = if light {
            DARK
        } else {
            Ink::solid([250, 250, 250])
        };
        for &(a, b) in &pieces {
            canvas.line(a, b, halo, 3. * s)?;
        }
        for &(a, b) in &pieces {
            canvas.line(a, b, Ink::solid(ink), 1.5 * s)?;
        }
        Ok(())
    }
    /// The drawn outline of the object at `index`, or its frame when it has none (text).
    fn contour(&self, index: usize, s: f64) -> Vec<Vec<Point>> {
        let objects = self.editor.objects();
        let lines = paths(&objects[index], objects);
        if !lines.is_empty() {
            return lines;
        }
        self.editor
            .frame_of(objects[index].id)
            .map(|frame| {
                let mut p = frame.outline(3. * s).to_vec();
                p.push(p[0]);
                vec![p]
            })
            .unwrap_or_default()
    }
    /// `field_ink` is the open text field's ink (`Some(None)`: automatic), `None` without one.
    pub fn draw_toolbar<C: Canvas>(
        &self,
        canvas: &mut C,
        current_color: [u8; 3],
        field_ink: Option<Option<[u8; 3]>>,
        now: std::time::Instant,
    ) -> Result<(), C::Error> {
        let buttons = self.buttons();
        toolbar::draw(
            canvas,
            &buttons,
            self.toolbar_state(current_color, field_ink),
            self.tool_tint(current_color),
            (&self.toolbar_motion, now),
            self.ui_scale(),
        )
    }
    /// Paint hints last so badges and panels cannot cover them after edge clamping.
    pub fn draw_hover_hint<C: Canvas>(
        &self,
        canvas: &mut C,
        now: std::time::Instant,
        editing_dimensions: bool,
    ) -> Result<(), C::Error> {
        if self.selecting || self.picker.is_some() {
            return Ok(());
        }
        let Some((action, bounds, opacity)) = self.hover_hint.visible(now) else {
            return Ok(());
        };
        if editing_dimensions && action == toolbar::Action::Dimensions {
            return Ok(());
        }
        if let Some(button) = self
            .buttons()
            .into_iter()
            .chain(self.screen_button())
            .chain(self.dimensions_button())
            .find(|b| b.action == action && b.bounds == bounds)
        {
            let screen = self.screen_at(self.pointer);
            toolbar::draw_hint(canvas, &button, screen, opacity, self.ui_scale())?;
        }
        Ok(())
    }
    /// Outline of the current stroke width at the pointer, or of the eraser's disc as the
    /// view shows it; with a size change, its size.
    pub fn draw_width_ring<C: Canvas>(&self, canvas: &mut C) -> Result<(), C::Error> {
        let (width, shown) = self.ring_width();
        let s = self.ui_scale();
        let center = self.pointer;
        let radius = Point {
            x: width / 2.,
            y: width / 2.,
        };
        canvas.stroke_ellipse(center, radius, Ink::solid([25, 25, 25]), 3. * s)?;
        canvas.stroke_ellipse(center, radius, Ink::solid([250, 250, 250]), s)?;
        if !self.width_preview {
            return Ok(());
        }
        let label = width_label_in(language(), shown);
        self.draw_pointer_badge(canvas, &label, width / 2.)
    }
    /// "Scroll to resize" beside the pointer of a drawing tool, past the ring when there is
    /// one, until the user first turns the wheel (issue #2).
    pub fn draw_wheel_hint<C: Canvas>(&self, canvas: &mut C) -> Result<(), C::Error> {
        if !self.wheel_hint_visible() {
            return Ok(());
        }
        let reach = if self.width_ring_visible() {
            self.ring_width().0 / 2.
        } else {
            0.
        };
        let label = tr(Key::OverlayWheelHint);
        self.draw_pointer_badge(canvas, label, reach.max(8. * self.ui_scale()))
    }
    /// The ring's diameter as the view shows it, and the size its badge names.
    fn ring_width(&self) -> (f64, f64) {
        match self.editor.tool {
            Tool::Eraser => (
                self.editor.eraser_size * self.zoom_at_pointer(),
                self.editor.eraser_size,
            ),
            Tool::Highlight => (self.editor.style.width * 5., self.editor.style.width * 5.),
            _ => (self.editor.style.width, self.editor.style.width),
        }
    }
    /// A dark badge holding `label`, below and right of the pointer at `reach` past it, kept
    /// inside the frame.
    fn draw_pointer_badge<C: Canvas>(
        &self,
        canvas: &mut C,
        label: &str,
        reach: f64,
    ) -> Result<(), C::Error> {
        let s = self.ui_scale();
        let center = self.pointer;
        let size = width_label_size(canvas.measure_text(label, 12. * s)?, s);
        let min = Point {
            x: (center.x + reach + 10. * s)
                .min(self.image.width as f64 - size.x)
                .max(0.),
            y: (center.y + 10. * s)
                .min(self.image.height as f64 - size.y)
                .max(0.),
        };
        let bounds = Rect {
            min,
            max: min + size,
        };
        canvas.fill_round_rect(bounds, 4. * s, Ink::solid([25, 25, 25]))?;
        canvas.text(
            label,
            Rect {
                min: min
                    + Point {
                        x: 7. * s,
                        y: 3. * s,
                    },
                max: bounds.max,
            },
            12. * s,
            Ink::solid([250, 250, 250]),
        )
    }
    pub fn awaiting_region(&self) -> bool {
        self.selecting && self.region_origin.is_none() && self.region.is_none()
    }
    /// Guidance shown before the first selection gesture, kept on the pointer's monitor.
    pub fn draw_selection_hint<C: Canvas>(
        &self,
        canvas: &mut C,
        monitor: Rect,
    ) -> Result<(), C::Error> {
        if !self.awaiting_region() || self.pointer.x < 0. || self.pointer.y < 0. {
            return Ok(());
        }
        let s = self.ui_scale();
        let [title, note] = selection_hint_lines_in(language());
        let (title_size, note_size) = (SELECTION_HINT_TITLE_SIZE * s, SELECTION_HINT_NOTE_SIZE * s);
        let size = selection_hint_size(
            canvas.measure_text(&title, title_size)?,
            canvas.measure_text(&note, note_size)?,
            s,
        );
        let screen = self.screen();
        let screen = Rect {
            min: Point {
                x: monitor.min.x.max(screen.min.x),
                y: monitor.min.y.max(screen.min.y),
            },
            max: Point {
                x: monitor.max.x.min(screen.max.x),
                y: monitor.max.y.min(screen.max.y),
            },
        };
        let bounds = place_hint(self.pointer, screen, size, s);
        let at = |x: f64, y: f64| bounds.min + Point { x: x * s, y: y * s };
        canvas.fill_round_rect(bounds, 5. * s, Ink::with_alpha([25, 25, 28], 0.94))?;
        canvas.text(
            &title,
            Rect {
                min: at(10., 6.),
                max: bounds.max,
            },
            title_size,
            Ink::solid([245, 245, 245]),
        )?;
        canvas.text(
            &note,
            Rect {
                min: at(10., 26.),
                max: bounds.max,
            },
            note_size,
            Ink::solid([180, 183, 188]),
        )
    }
    /// Crop-size badge and entry error. While `editing`, the
    /// platform draws the two size fields over it, on each side of the "×".
    pub fn draw_dimensions<C: Canvas>(
        &self,
        canvas: &mut C,
        editing: bool,
        error: Option<Key>,
    ) -> Result<(), C::Error> {
        let (Some(bounds), Some(region)) = (self.dimensions_bounds(), self.region) else {
            return Ok(());
        };
        let s = self.ui_scale();
        let hover = bounds.contains(self.pointer) && !self.selecting;
        let background = if hover {
            Ink::solid([48, 57, 70])
        } else {
            DARK
        };
        canvas.fill_round_rect(bounds, 4. * s, background)?;
        // Editing, the "×" sits between the fields; else the label fills the badge.
        let fields = self.dimension_field_bounds().filter(|_| editing);
        let (label, x) = match fields {
            Some([width, _]) => ("×".to_owned(), width.max.x + 3. * s),
            None => (
                format!("{} × {}", region.size().x as i32, region.size().y as i32),
                bounds.min.x + 8. * s,
            ),
        };
        let label_area = Rect {
            min: Point {
                x,
                y: bounds.min.y + 4. * s,
            },
            max: bounds.max,
        };
        let size = crate::BADGE_LABEL_SIZE * s;
        canvas.text(&label, label_area, size, Ink::solid([240, 240, 240]))?;
        if let Some(button) = self.screen_button_bounds() {
            self.draw_screen_button(canvas, button, button.contains(self.pointer))?;
        }
        let Some(key) = error else {
            return Ok(());
        };
        let message = tr(key);
        let text_size = ENTRY_ERROR_SIZE * s;
        let size = entry_error_size(canvas.measure_text(message, text_size)?, s);
        let area = toolbar::tooltip_bounds(bounds, self.screen_at(self.pointer), size, s);
        canvas.fill_rect(area, background)?;
        let text_area = Rect {
            min: area.min
                + Point {
                    x: 6. * s,
                    y: 4. * s,
                },
            max: area.max,
        };
        canvas.text(message, text_area, text_size, Ink::solid([255, 160, 150]))
    }
    /// Four diagonal arrows: outward to fill the screen, inward once it is filled.
    fn draw_screen_button<C: Canvas>(
        &self,
        canvas: &mut C,
        bounds: Rect,
        hover: bool,
    ) -> Result<(), C::Error> {
        let filled = self.screen_filled();
        let background = if filled {
            Ink::solid([38, 78, 126])
        } else if hover {
            Ink::solid([48, 57, 70])
        } else {
            DARK
        };
        let s = self.ui_scale();
        canvas.fill_round_rect(bounds, 4. * s, background)?;
        let ink = Ink::solid([240, 240, 240]);
        let center = bounds.center();
        for (x, y) in [(-s, -s), (s, -s), (s, s), (-s, s)] {
            let near = center
                + Point {
                    x: x * 3.,
                    y: y * 3.,
                };
            let far = center
                + Point {
                    x: x * 8.,
                    y: y * 8.,
                };
            let (tail, tip, sign) = if filled {
                (far, near, -1.)
            } else {
                (near, far, 1.)
            };
            canvas.line(tail, tip, ink, 1.6 * s)?;
            let head = 4. * sign;
            canvas.line(tip, tip - Point { x: x * head, y: 0. }, ink, 1.6 * s)?;
            canvas.line(tip, tip - Point { x: 0., y: y * head }, ink, 1.6 * s)?;
        }
        Ok(())
    }
}
/// Font sizes of the selection hint's title and note, in logical pixels.
pub const SELECTION_HINT_TITLE_SIZE: f64 = 13.;
pub const SELECTION_HINT_NOTE_SIZE: f64 = 11.;

/// The selection hint's title and note in `language`.
pub fn selection_hint_lines_in(language: Language) -> [String; 2] {
    let esc = [("shortcut", tr_in(language, Key::KeyEsc))];
    [
        Text::new_in(language, Key::OverlaySelectArea).to_string(),
        Text::with_in(language, Key::OverlayCancel, &esc).to_string(),
    ]
}
/// Size of the selection hint whose lines measure `title` and `note`: 10 px each side of
/// the wider line, 48 px high unless the note, 26 px down, needs more (6 px below it).
pub fn selection_hint_size(title: Point, note: Point, scale: f64) -> Point {
    Point {
        x: title.x.max(note.x) + 20. * scale,
        y: (48. * scale).max(note.y + 32. * scale),
    }
}
/// Font size of a size-entry error, in logical pixels.
pub const ENTRY_ERROR_SIZE: f64 = 12.;

/// Size of the box under the size badge for an error that measures `message`: 460×48,
/// grown for a longer message with 6 px each side and 4 px above and below.
pub fn entry_error_size(message: Point, scale: f64) -> Point {
    Point {
        x: (460. * scale).max(message.x + 12. * scale),
        y: (48. * scale).max(message.y + 8. * scale),
    }
}
/// The stroke width as the ring's badge shows it.
fn width_label_in(language: Language, width: f64) -> String {
    let width = format!("{width:.0}");
    Text::with_in(language, Key::OverlayStrokeWidth, &[("width", &width)]).to_string()
}
/// Size of the stroke-width badge for a label measuring `label`: 72×24, grown with 7 px
/// each side.
fn width_label_size(label: Point, scale: f64) -> Point {
    Point {
        x: (72. * scale).max(label.x + 14. * scale),
        y: 24. * scale,
    }
}
/// The outlines of the shapes `coat` spares, its zone's place in the pile being `floor`.
fn holes(coat: &Paint, objects: &[Object], floor: usize) -> Vec<Vec<Point>> {
    coat.holes(objects, floor)
        .flat_map(|o| paths(o, objects))
        .collect()
}
/// A closed outline through `corners`, in dashes of 4 by 3 interface pixels at `scale`.
fn dashed<C: Canvas>(
    canvas: &mut C,
    corners: [Point; 4],
    ink: Ink,
    scale: f64,
) -> Result<(), C::Error> {
    let mut outline = corners.to_vec();
    outline.push(corners[0]);
    for (a, b) in dashes(&outline, scale) {
        canvas.line(a, b, ink, scale)?;
    }
    Ok(())
}
/// The pieces of `line` drawn in dashes of 4 by 3 interface pixels at `scale`, the rhythm
/// running on across its corners so a curve of short segments stays dashed.
fn dashes(line: &[Point], scale: f64) -> Vec<(Point, Point)> {
    let (dash, period) = (4. * scale, 7. * scale);
    let mut pieces = Vec::new();
    let mut walked = 0.;
    for pair in line.windows(2) {
        let side = pair[1] - pair[0];
        let length = side.length();
        if length <= 0. {
            continue;
        }
        let unit = side * (1. / length);
        let mut t = 0.;
        while t < length {
            let phase = (walked + t) % period;
            let next = if phase < dash {
                let end = (t + dash - phase).min(length);
                pieces.push((pair[0] + unit * t, pair[0] + unit * end));
                end
            } else {
                t + period - phase
            };
            // Rounding can leave a step of nothing; skip to the next phase instead.
            t = if next > t { next } else { t + 1e-6 * scale };
        }
        walked += length;
    }
    pieces
}
const NOTICE_SIZE: f64 = 13.;
const NOTICE_PADDING: Point = Point { x: 12., y: 8. };

/// Size of the box `draw_notice` paints for `message` at interface `scale`.
pub fn notice_size<C: Canvas>(
    canvas: &mut C,
    message: &str,
    scale: f64,
) -> Result<Point, C::Error> {
    Ok(canvas.measure_text(message, NOTICE_SIZE * scale)? + NOTICE_PADDING * (2. * scale))
}
/// An error message box filling `bounds` (see `notice_size`), for failures shown on
/// platforms without a native message window.
pub fn draw_notice<C: Canvas>(
    canvas: &mut C,
    bounds: Rect,
    message: &str,
    scale: f64,
) -> Result<(), C::Error> {
    canvas.fill_round_rect(bounds, 5. * scale, Ink::with_alpha([25, 25, 28], 0.94))?;
    let text = Rect {
        min: bounds.min + NOTICE_PADDING * scale,
        max: bounds.max,
    };
    let size = NOTICE_SIZE * scale;
    canvas.text(message, text, size, Ink::solid([255, 160, 150]))
}
/// `size` centred horizontally near the top of `screen`, kept inside it; margins are in
/// logical pixels at `scale`.
pub fn top_center(screen: Rect, size: Point, scale: f64) -> Rect {
    let width = size.x.min(screen.size().x - 8. * scale).max(0.);
    let min = Point {
        x: screen.center().x - width / 2.,
        y: screen.min.y + 24. * scale,
    };
    Rect {
        min,
        max: min
            + Point {
                x: width,
                y: size.y,
            },
    }
}
/// A box of `size` beside the pointer, kept on `screen`; distances are in logical pixels at
/// `scale`.
pub fn place_hint(pointer: Point, screen: Rect, size: Point, scale: f64) -> Rect {
    let (away, edge) = (20. * scale, 4. * scale);
    let place = |p: f64, min: f64, max: f64, size: f64| {
        let preferred = if p + away + size <= max - edge {
            p + away
        } else {
            p - away - size
        };
        preferred.clamp(min + edge, (max - size - edge).max(min + edge))
    };
    let min = Point {
        x: place(pointer.x, screen.min.x, screen.max.x, size.x),
        y: place(pointer.y, screen.min.y, screen.max.y, size.y),
    };
    Rect {
        min,
        max: min + size,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::magnifier_hint_in;

    #[test]
    fn english_hint_texts_are_unchanged() {
        assert_eq!(
            selection_hint_lines_in(Language::En),
            ["Select an area", "Esc to cancel"]
        );
        assert_eq!(magnifier_hint_in(Language::En), "Click to pick · Esc");
        assert_eq!(width_label_in(Language::En, 12.), "12 px");
        let [_, note] = selection_hint_lines_in(Language::Fr);
        assert!(note.contains("Échap"), "{note}");
    }

    #[test]
    fn hint_zones_keep_their_size_unless_a_line_needs_more() {
        let title = Point { x: 90., y: 17. };
        let short = Point { x: 70., y: 15. };
        assert_eq!(
            selection_hint_size(title, short, 1.),
            Point { x: 110., y: 48. }
        );
        let long = Point { x: 130., y: 15. };
        assert_eq!(selection_hint_size(title, long, 1.).x, 150.);
        let tall = Point { x: 70., y: 20. };
        assert_eq!(selection_hint_size(title, tall, 1.).y, 52.);
        assert_eq!(
            selection_hint_size(title * 2., short * 2., 2.),
            Point { x: 220., y: 96. }
        );

        let line = Point { x: 400., y: 16. };
        assert_eq!(entry_error_size(line, 1.), Point { x: 460., y: 48. });
        let wide = Point { x: 500., y: 16. };
        assert_eq!(entry_error_size(wide, 1.), Point { x: 512., y: 48. });
        let tall = Point { x: 400., y: 44. };
        assert_eq!(entry_error_size(tall, 1.).y, 52.);

        assert_eq!(
            width_label_size(Point { x: 40., y: 16. }, 1.),
            Point { x: 72., y: 24. }
        );
        assert_eq!(width_label_size(Point { x: 70., y: 16. }, 1.).x, 84.);
    }
}
