//! ROLE: Pointer state machine and toolbar actions of a capture session.
//! INVARIANTS: Inline native fields get the first chance at each press; a press never
//! both starts a gesture and triggers a button.
use crate::{Capture, Host, color_picker::ColorPicker, toolbar::Action, wheel};
use shotoshot_core::{Label, Modifiers, Point, Rect, Shape, Tool};

/// How far, in interface pixels, a press of the label tool travels before it is a drag that
/// sets the panel's width rather than a click that fits it to the text.
const LABEL_DRAG: f64 = 6.;

impl Capture {
    /// Returns the toolbar action to run, if the press hit an enabled button.
    pub fn pointer_down<H: Host>(
        &mut self,
        host: &mut H,
        p: Point,
        modifiers: Modifiers,
    ) -> Result<Option<Action>, H::Error> {
        let p = self.clamp(p);
        self.pointer = p;
        self.adopt_scale();
        self.width_preview = false;
        self.committed_text = None;
        if self.panning() {
            return Ok(None);
        }
        if self.space_held() && self.begin_pan(p) {
            host.capture_pointer();
            return Ok(None);
        }
        // Buttons, panels and badges live in screen coordinates; the document in frame ones.
        self.gesture_view = self.view_index(p);
        let f = self.drag_frame(p);
        if host.dimension_pointer_down(self, p)? {
            return Ok(None);
        }
        if host.dimensions_active() && !host.finish_dimensions(self, true) {
            return Ok(None);
        }
        if let Some(picker) = &mut self.picker {
            if picker.bounds().contains(p) && picker.pointer_down(p) {
                host.capture_pointer();
                return Ok(None);
            }
            // A click off the panel applies the wheel's colour. The wheel never reads the
            // screen: that is the eyedropper's job, a tool of its own (Mickaël, 2026-10-10).
            let color = picker.color();
            self.picker = None;
            self.accept_custom_color(host, color);
            host.focus_text();
            return Ok(None);
        }
        if self.eyedropper {
            let rgb = self.sample_color(f);
            self.eyedropper = false;
            self.accept_custom_color(host, rgb);
            host.focus_text();
            return Ok(Some(Action::CopyColor(rgb)));
        }
        if !self.selecting
            && let Some(action) = self
                .buttons()
                .iter()
                .find(|b| b.enabled && b.bounds.contains(p))
                .map(|b| b.action)
        {
            if host.text_active()
                && !matches!(
                    action,
                    Action::Color(_)
                        | Action::CustomColor
                        | Action::Eyedropper
                        | Action::BoxColor
                        | Action::TextColor
                )
            {
                host.finish_text(self, true);
            }
            return Ok(Some(action));
        }
        if self.toolbar_contains(p) {
            return Ok(None);
        }
        if self.screen_button_bounds().is_some_and(|r| r.contains(p)) {
            if host.text_active() {
                host.finish_text(self, true);
            }
            self.toggle_screen();
            return Ok(None);
        }
        if !self.selecting && self.dimensions_bounds().is_some_and(|r| r.contains(p)) {
            host.begin_dimensions(self)?;
            return Ok(None);
        }
        if host.text_pointer_down(f)? {
            return Ok(None);
        }
        if host.text_active() {
            host.finish_text(self, true);
            // A click off the text being typed settles it, nothing more: it used to open an
            // empty text (or box) at once, read as a stray caret (blind test of 2026-11-05).
            if matches!(self.editor.tool, Tool::Text | Tool::Label) && !self.selecting {
                return Ok(None);
            }
        }
        if self.editor.tool == Tool::Select
            && !self.bucket
            && self
                .rotation_handle()
                .is_some_and(|h| (h - f).length() <= 9. * self.pick_scale())
        {
            self.editor.begin_rotation(f);
            host.capture_pointer();
            return Ok(None);
        }
        if self.selecting {
            self.region_before = self.region;
            self.region_origin = Some(f);
            self.region = Rect::new(f, f);
            host.capture_pointer();
            return Ok(None);
        }
        if let Some(region) = self.region
            && let Some(index) = self.region_handle(f)
        {
            self.region_before = self.region;
            self.region_origin = Some(region.min);
            self.region_resize = Some(index);
            self.selecting = true;
            host.capture_pointer();
            return Ok(None);
        }
        // A recalled region is an offer: a press outside it draws a new one, as at the start.
        if std::mem::take(&mut self.recalled_region) && self.region.is_some_and(|r| !r.contains(f))
        {
            self.selecting = true;
            self.region_before = self.region;
            self.region_origin = Some(f);
            self.region = Rect::new(f, f);
            host.capture_pointer();
            return Ok(None);
        }
        if self.bucket {
            // The bucket stays in hand, zone after zone, until another action puts it down.
            if self.region.is_some_and(|r| r.contains(f)) {
                self.editor.fill_at(f, modifiers.alt);
            }
            return Ok(None);
        }
        // The crop limits export only: every tool but the bucket works off the region too, so
        // a shape set down beside it joins the capture once the region grows over it.
        // An anchor of a text box pulls an arrow out of it whatever the tool.
        if self.editor.anchor_at(f).is_some() {
            self.editor.begin(f, modifiers);
            host.capture_pointer();
        } else if self.editor.tool == Tool::Text {
            self.new_label = None;
            host.begin_text(self, f)?;
        } else if self.editor.tool == Tool::Label {
            self.begin_label(host, f)?;
        } else {
            self.editor.begin(f, modifiers);
            host.capture_pointer();
        }
        Ok(None)
    }
    /// A secondary press. On the text colour button it gives text boxes back their automatic
    /// black or white ink. With the bucket in hand it takes off the coat seen under the
    /// pointer, whatever its colour, like Alt + click (Mickaël, 2026-10-17); it does nothing
    /// else. True when it changed what is shown.
    pub fn secondary_down<H: Host>(&mut self, host: &mut H, p: Point) -> bool {
        self.pointer = self.clamp(p);
        let on_text_color = self
            .buttons()
            .iter()
            .any(|b| b.action == Action::TextColor && b.enabled && b.bounds.contains(self.pointer));
        if on_text_color {
            self.give_ink(host, None);
            return true;
        }
        if !self.bucket || !self.pointer_on_document() {
            return false;
        }
        let f = self.to_frame(self.pointer);
        if !self.region.is_some_and(|r| r.contains(f)) || self.editor.fill_change(f, true).is_none()
        {
            return false;
        }
        self.editor.fill_at(f, true);
        true
    }
    pub fn pointer_move<H: Host>(&mut self, host: &mut H, p: Point, modifiers: Modifiers) {
        let p = self.clamp(p);
        self.pointer = p;
        self.adopt_scale();
        if self.panning() {
            self.pan_to(p);
            return;
        }
        let f = self.drag_frame(p);
        if host.dimensions_active() {
            host.dimension_pointer_move(p);
            return;
        }
        if let Some(picker) = &mut self.picker {
            picker.pointer_move(p);
            return;
        }
        if host.text_pointer_move(f) {
            return;
        }
        if let Some((origin, _)) = self.label_drag {
            self.label_drag = Some((origin, f));
            self.place_label(host);
            return;
        }
        if let Some(origin) = self.region_origin {
            if let (Some(index), Some(before)) = (self.region_resize, self.region_before) {
                let mut min = before.min;
                let mut max = before.max;
                if [0, 6, 7].contains(&index) {
                    min.x = f.x;
                }
                if [2, 3, 4].contains(&index) {
                    max.x = f.x;
                }
                if [0, 1, 2].contains(&index) {
                    min.y = f.y;
                }
                if [4, 5, 6].contains(&index) {
                    max.y = f.y;
                }
                self.region = Rect::new(min, max);
            } else {
                self.region = Rect::new(origin, f);
            }
        } else {
            self.editor.update_with_modifiers(f, modifiers);
            self.reflow_labels(host);
        }
    }
    /// The label tool's press: on a text it edits it, elsewhere it opens a new text on a
    /// panel fitted to it, whose width a drag then sets (`place_label`).
    fn begin_label<H: Host>(&mut self, host: &mut H, f: Point) -> Result<(), H::Error> {
        if self.text_at(f).is_some() {
            self.new_label = None;
            return host.begin_text(self, f);
        }
        self.new_label = Some(Label::Fit);
        host.begin_text(self, f)?;
        self.label_drag = Some((f, f));
        self.place_label(host);
        host.capture_pointer();
        Ok(())
    }
    /// The panel of a label being drawn: its top-left corner at the press, fitted to the text
    /// until the drag goes further than a click, then as wide as the drag; left empty, it
    /// also keeps the drag's height (Mickaël, 2026-10-30).
    fn place_label<H: Host>(&mut self, host: &mut H) {
        let (Some((origin, current)), Some(size)) = (self.label_drag, host.text_size()) else {
            return;
        };
        let size = size as f64;
        let pad = Label::padding(size);
        let drag = current - origin;
        if drag.x.abs().max(drag.y.abs()) < LABEL_DRAG * self.pick_scale() {
            host.set_text_label(origin + pad, Some(Label::Fit), 0.);
            return;
        }
        let corner = Point {
            x: origin.x.min(current.x),
            y: origin.y.min(current.y),
        };
        let wrap = (drag.x.abs() - 2. * pad.x).max(Label::min_wrap(size));
        let height = (drag.y.abs() - 2. * pad.y).max(0.);
        host.set_text_label(corner + pad, Some(Label::Wrap(wrap)), height);
    }
    /// Lays out again the lines of wrapped labels a gesture is resizing, so their panels
    /// follow the hand before it lets go.
    fn reflow_labels<H: Host>(&mut self, host: &mut H) {
        if !self.editor.resizing() {
            return;
        }
        let wrapped: Vec<_> = self
            .editor
            .objects()
            .iter()
            .filter(|o| self.editor.selected.contains(&o.id))
            .filter_map(|o| match &o.shape {
                Shape::Text {
                    text,
                    size,
                    label: label @ Some(Label::Wrap(_)),
                    ..
                } => Some((o.id, text.clone(), *size, *label)),
                _ => None,
            })
            .collect();
        for (id, text, size, label) in wrapped {
            // A failed measure keeps the lines of the last frame.
            if let Ok(extent) = host.measure_text_object(&text, size, label) {
                self.editor.reflow(id, extent);
            }
        }
    }
    pub fn pointer_up<H: Host>(&mut self, host: &mut H, p: Point, modifiers: Modifiers) {
        self.pointer_move(host, p, modifiers);
        self.gesture_view = None;
        if self.panning() {
            self.end_pan();
            host.release_pointer();
            return;
        }
        if host.dimension_pointer_up() {
            return;
        }
        if let Some(picker) = &mut self.picker {
            picker.pointer_up();
            host.release_pointer();
            return;
        }
        if host.text_pointer_up() {
            host.release_pointer();
            return;
        }
        if self.label_drag.take().is_some() {
            host.release_pointer();
            return;
        }
        if self.region_origin.take().is_some() {
            self.region_resize = None;
            if self
                .region
                .is_some_and(|r| r.size().x >= 2. && r.size().y >= 2.)
            {
                self.selecting = false;
            } else {
                self.region = None;
            }
            // The finished region may sit on a screen of another scale.
            self.adopt_scale();
        } else {
            self.editor.finish();
        }
        host.release_pointer();
    }
    /// Toolbar and shortcut actions that only change the capture itself.
    pub fn action<H: Host>(&mut self, host: &mut H, a: Action) {
        self.adopt_scale();
        if !matches!(a, Action::Thinner | Action::Thicker) {
            self.editor.cancel();
        }
        self.eyedropper = false;
        // The bucket waits while its colour is chosen; any other action puts it down.
        let armed = std::mem::take(&mut self.bucket);
        self.bucket = armed
            && matches!(
                a,
                Action::Color(_)
                    | Action::CopyColor(_)
                    | Action::CustomColor
                    | Action::Eyedropper
                    | Action::BoxColor
                    | Action::TextColor
            );
        match a {
            Action::Fill => self.bucket = !armed,
            Action::MoreTools => self.toolbar_unfolded = !self.toolbar_unfolded,
            Action::CustomColor => {
                self.picker = Some(ColorPicker::new(
                    self.custom_color,
                    self.pointer,
                    self.image.width,
                    self.image.height,
                    self.ui_scale(),
                ));
            }
            Action::Eyedropper => {
                self.eyedropper = true;
            }
            // A board has no capture under its notes to blur: no button, and X does nothing.
            Action::Tool(Tool::Blur) if self.board => {}
            Action::Tool(t) => {
                self.editor.tool = t;
                self.editor.selected.clear();
            }
            Action::BoxColor => self.ink_target = false,
            Action::TextColor => self.ink_target = true,
            Action::Color(c) if self.inks() => self.give_ink(host, Some(c)),
            Action::Color(c) => {
                self.editor.style.color = c;
                if host.text_active() {
                    host.set_text_color(c);
                } else {
                    self.editor.apply_color();
                }
            }
            // A blur's strength follows its zone (Mickaël, 2026-11-06): the wheel has nothing
            // to set with the blur tool in hand, nor on a selection of blurs only.
            Action::Thinner | Action::Thicker if self.wheel_meets_only_blurs() => {}
            Action::Thinner | Action::Thicker => {
                let notch = if a == Action::Thicker { 1 } else { -1 };
                self.width_preview = true;
                if self.editor.tool == Tool::Eraser {
                    // The eraser has a size of its own: the pen keeps its width.
                    self.editor.eraser_size =
                        wheel::eraser_size_after(self.editor.eraser_size, notch);
                } else if let Some(width) = self
                    .editor
                    .step_selected_widths(|w| wheel::stroke_width_after(w, notch))
                {
                    // A selection grows from its own widths and keeps its colours.
                    self.editor.style.width = width;
                } else {
                    self.editor.style.width =
                        wheel::stroke_width_after(self.editor.style.width, notch);
                    self.editor.apply_style();
                }
            }
            Action::FullScreen if !self.selecting => self.toggle_screen(),
            Action::Undo => self.editor.undo(),
            Action::Redo => self.editor.redo(),
            Action::ClearBoard if self.board => self.editor.clear(),
            _ => {}
        }
    }
    /// Escape: undoes the innermost pending gesture; false when nothing was pending.
    pub fn cancel_gesture<H: Host>(&mut self, host: &mut H) -> bool {
        if host.cancel_text_drag() {
            return true;
        }
        if self.picker.take().is_some() {
            return true;
        }
        if self.eyedropper {
            self.eyedropper = false;
            return true;
        }
        if self.bucket {
            self.bucket = false;
            return true;
        }
        if self.region_origin.take().is_some() {
            self.region_resize = None;
            self.region = self.region_before;
            self.selecting = self.region.is_none();
            true
        } else if self.selecting && self.region.is_some() {
            self.selecting = false;
            true
        } else {
            self.editor.cancel()
        }
    }
    /// Wheel: text size while typing or for selected texts, else the stroke width. Each
    /// whole notch of travel moves one rung along the size ladder (see `wheel`).
    pub fn wheel<H: Host>(&mut self, host: &mut H, delta: i16) -> Result<(), H::Error> {
        if self.picker.is_some() || host.dimensions_active() || self.eyedropper || self.bucket {
            return Ok(());
        }
        self.adopt_scale();
        let notches = self.wheel_travel.take(delta);
        if notches == 0 {
            return Ok(());
        }
        self.wheel_hint = false;
        if let Some(size) = host.text_size() {
            let resized = wheel::text_size_after(size.into(), notches) as f32;
            if resized != size {
                host.set_text_size(resized)?;
                self.text_size = resized;
            }
            return Ok(());
        }
        let texts: Vec<_> = self
            .editor
            .objects()
            .iter()
            .filter_map(|o| {
                if self.editor.selected.contains(&o.id)
                    && let Shape::Text {
                        text, size, label, ..
                    } = &o.shape
                {
                    Some((o.id, text.clone(), *size, *label))
                } else {
                    None
                }
            })
            .collect();
        if !texts.is_empty() {
            let mut changes = Vec::with_capacity(texts.len());
            for (id, text, size, label) in texts {
                let size = wheel::text_size_after(size, notches);
                // Sizes go through f32 exactly as the native text field stores them.
                let extent = host.measure_text_object(&text, size as f32 as f64, label)?;
                changes.push((id, size, extent));
                self.text_size = size as f32;
            }
            self.editor.set_text_sizes(&changes);
            self.width_preview = false;
        } else if matches!(self.editor.tool, Tool::Text | Tool::Label) {
            self.text_size = wheel::text_size_after(self.text_size.into(), notches) as f32;
        } else {
            let step = if notches > 0 {
                Action::Thicker
            } else {
                Action::Thinner
            };
            for _ in 0..notches.unsigned_abs() {
                self.action(host, step);
            }
        }
        Ok(())
    }
    /// `ink` (`None`: the automatic black or white) onto the open field's text, else onto
    /// the selected text boxes as one undo step, and onto the next boxes of this capture.
    fn give_ink<H: Host>(&mut self, host: &mut H, ink: Option<[u8; 3]>) {
        self.label_ink = ink;
        if host.text_active() {
            host.set_text_ink(ink);
        } else {
            self.editor.apply_ink(ink);
        }
    }
    pub fn accept_custom_color<H: Host>(&mut self, host: &mut H, color: [u8; 3]) {
        self.custom_color = color;
        self.custom_color_changed = true;
        self.action(host, Action::Color(color));
    }
}

#[cfg(test)]
mod tests;
