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
    /// until the drag goes further than a click, then as wide as the drag.
    fn place_label<H: Host>(&mut self, host: &mut H) {
        let (Some((origin, current)), Some(size)) = (self.label_drag, host.text_size()) else {
            return;
        };
        let size = size as f64;
        let pad = Label::padding(size);
        let drag = current - origin;
        if drag.x.abs().max(drag.y.abs()) < LABEL_DRAG * self.pick_scale() {
            host.set_text_label(origin + pad, Some(Label::Fit));
            return;
        }
        let corner = Point {
            x: origin.x.min(current.x),
            y: origin.y.min(current.y),
        };
        let wrap = (drag.x.abs() - 2. * pad.x).max(Label::min_wrap(size));
        host.set_text_label(corner + pad, Some(Label::Wrap(wrap)));
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
            Action::Thinner | Action::Thicker => {
                let notch = if a == Action::Thicker { 1 } else { -1 };
                self.width_preview = true;
                if self.editor.tool == Tool::Eraser {
                    // The eraser has a size of its own: the pen keeps its width.
                    self.editor.eraser_size =
                        wheel::eraser_size_after(self.editor.eraser_size, notch);
                } else if let Some((width, blur)) = self.editor.step_selected_widths(|w, blur| {
                    if blur {
                        wheel::blur_radius_after(w, notch)
                    } else {
                        wheel::stroke_width_after(w, notch)
                    }
                }) {
                    // A selection grows from its own widths and keeps its colours.
                    if blur {
                        self.editor.blur_radius = width;
                    } else {
                        self.editor.style.width = width;
                    }
                } else if self.editor.tool == Tool::Blur {
                    // So does the blur's strength, the one being drawn included.
                    self.editor.blur_radius =
                        wheel::blur_radius_after(self.editor.blur_radius, notch);
                    self.editor.apply_style();
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
mod tests {
    use super::*;
    use crate::{Image, Monitor, test_host::FakeHost};
    use shotoshot_core::Style;

    fn p(x: f64, y: f64) -> Point {
        Point { x, y }
    }
    /// A region selected on a single 1800 x 1300 screen at `scale`.
    fn selected_at(scale: f64) -> (Capture, FakeHost) {
        let mut c = Capture::new(Image::solid(1800, 1300, [0, 0, 0, 255]));
        c.monitors = vec![Monitor {
            bounds: Rect::new(p(0., 0.), p(1800., 1300.)).unwrap(),
            scale,
        }];
        let mut host = FakeHost::default();
        let m = Modifiers::default();
        c.pointer_down(&mut host, p(100., 100.), m).unwrap();
        c.pointer_up(&mut host, p(600., 400.), m);
        (c, host)
    }
    #[test]
    fn a_recalled_region_opens_selected_and_a_press_outside_draws_a_new_one() {
        let m = Modifiers::default();
        let recalled = Rect::new(p(100., 100.), p(600., 400.)).unwrap();
        let mut c = Capture::new(Image::solid(1800, 1300, [0, 0, 0, 255]));
        c.recall_region(recalled);
        assert_eq!(c.region, Some(recalled));
        assert!(!c.selecting);
        let mut host = FakeHost::default();
        c.pointer_down(&mut host, p(900., 800.), m).unwrap();
        c.pointer_up(&mut host, p(1200., 1000.), m);
        assert_eq!(c.region, Rect::new(p(900., 800.), p(1200., 1000.)));
        assert!(c.editor.objects().is_empty());

        // A press inside accepts it: the next press outside draws, as on any region.
        let mut c = Capture::new(Image::solid(1800, 1300, [0, 0, 0, 255]));
        c.recall_region(recalled);
        c.pointer_down(&mut host, p(200., 200.), m).unwrap();
        c.pointer_up(&mut host, p(300., 300.), m);
        c.pointer_down(&mut host, p(900., 800.), m).unwrap();
        c.pointer_up(&mut host, p(1200., 1000.), m);
        assert_eq!(c.region, Some(recalled));
        assert_eq!(c.editor.objects().len(), 2);
    }
    #[test]
    fn a_recalled_region_off_the_frame_is_ignored() {
        let mut c = Capture::new(Image::solid(800, 600, [0, 0, 0, 255]));
        c.recall_region(Rect::new(p(500., 100.), p(900., 400.)).unwrap());
        assert_eq!(c.region, None);
        assert!(c.selecting);
    }
    #[test]
    fn default_stroke_and_text_follow_the_screen_scale_and_settings_keep_the_100_percent_width() {
        for (scale, width, text) in [
            (1., 3., 24.),
            (1.25, 4., 30.),
            (1.5, 5., 36.),
            (2., 6., 48.),
            (2.5, 8., 60.),
        ] {
            let (c, _) = selected_at(scale);
            assert_eq!(c.editor.style.width, width, "stroke at {scale}");
            assert_eq!(c.text_size, text, "text at {scale}");
            assert_eq!(c.stroke_width_setting(), 3, "remembered at {scale}");
            assert_eq!(c.editor.ui_scale, scale);
        }
    }
    #[test]
    fn a_chosen_width_keeps_its_proportion_when_the_region_changes_screen() {
        let (mut c, mut host) = selected_at(2.);
        c.monitors.push(Monitor {
            bounds: Rect::new(p(1800., 0.), p(3600., 1300.)).unwrap(),
            scale: 1.,
        });
        c.image = Image::solid(3600, 1300, [0, 0, 0, 255]);
        c.action(&mut host, Action::Thicker);
        c.action(&mut host, Action::Thicker);
        assert_eq!((c.editor.style.width, c.stroke_width_setting()), (8., 4));
        c.region = Rect::new(p(2000., 100.), p(2500., 400.));
        c.pointer_move(&mut host, p(2200., 200.), Modifiers::default());
        assert_eq!((c.editor.style.width, c.text_size), (4., 24.));
        c.region = Rect::new(p(100., 100.), p(600., 400.));
        c.pointer_move(&mut host, p(300., 200.), Modifiers::default());
        assert_eq!((c.editor.style.width, c.text_size), (8., 48.));
        assert_eq!(c.stroke_width_setting(), 4);
    }
    #[test]
    fn a_click_outside_the_open_wheel_applies_its_colour_and_closes_it() {
        let (mut c, mut host) = selected_at(1.);
        c.image.pixels[(700 * 1800 + 1500) * 4..][..4].copy_from_slice(&[30, 20, 10, 255]);
        c.pointer = p(300., 300.);
        c.action(&mut host, Action::CustomColor);
        let before = c.editor.style.color;
        let square = c.picker.as_ref().unwrap().bounds().min + p(130., 144.);
        c.pointer_down(&mut host, square, Modifiers::default())
            .unwrap();
        c.pointer_up(&mut host, square, Modifiers::default());
        let chosen = c.picker.as_ref().unwrap().color();
        assert_ne!(chosen, before);
        let effect = c
            .pointer_down(&mut host, p(1500., 700.), Modifiers::default())
            .unwrap();
        assert_eq!(
            effect, None,
            "the wheel never reads the screen: that is the eyedropper's job"
        );
        assert!(c.picker.is_none());
        assert_eq!(c.editor.style.color, chosen);
        assert_eq!(c.custom_color, chosen);
    }
    #[test]
    fn a_click_inside_the_wheel_panel_keeps_it_open() {
        let (mut c, mut host) = selected_at(1.);
        c.pointer = p(300., 300.);
        c.action(&mut host, Action::CustomColor);
        let picker = c.picker.as_ref().unwrap();
        let chosen = picker.color();
        let before = c.editor.style.color;
        // The panel's margin: inside the bounds, on no control.
        let margin = picker.bounds().min + p(2., 2.);
        let effect = c
            .pointer_down(&mut host, margin, Modifiers::default())
            .unwrap();
        assert_eq!(effect, None);
        assert_eq!(c.editor.style.color, before);
        let picker = c.picker.as_ref().expect("the wheel stays open");
        assert_eq!(picker.color(), chosen);
    }
    #[test]
    fn the_picker_opens_at_the_interface_scale() {
        for scale in [1., 2.] {
            let (mut c, mut host) = selected_at(scale);
            c.action(&mut host, Action::CustomColor);
            let size = c.picker.as_ref().unwrap().bounds().size();
            assert_eq!((size.x, size.y), (260. * scale, 316. * scale));
        }
    }
    fn selected() -> (Capture, FakeHost) {
        let mut c = Capture::new(Image::solid(900, 600, [0, 0, 0, 255]));
        let mut host = FakeHost::default();
        let m = Modifiers::default();
        c.pointer_down(&mut host, p(100., 100.), m).unwrap();
        assert!(host.captured);
        c.pointer_up(&mut host, p(600., 400.), m);
        (c, host)
    }

    #[test]
    fn an_anchor_of_the_box_just_written_pulls_an_arrow_with_the_box_tool_in_hand() {
        let (mut c, mut host) = selected();
        c.action(&mut host, Action::Tool(Tool::Label));
        let id = c
            .editor
            .add_label(
                p(200., 200.),
                "Note".into(),
                p(100., 24.),
                [253, 216, 53],
                20.,
                Some(Label::Fit),
                None,
            )
            .unwrap();
        // A text just validated stays selected, and so shows its anchors.
        c.editor.select_only(id);
        let bottom = c.editor.anchors(id).unwrap()[2];
        assert_eq!(c.cursor(bottom), crate::CursorKind::Cross);
        c.pointer_down(&mut host, bottom, Modifiers::default())
            .unwrap();
        assert!(!host.text_open, "no new text opened on the anchor");
        assert!(host.captured);
        c.pointer_up(&mut host, p(450., 350.), Modifiers::default());
        let arrow = &c.editor.objects()[1];
        assert!(matches!(
            arrow.shape,
            Shape::Arrow {
                elbow: true,
                start_attach: Some(_),
                ..
            }
        ));
        assert_eq!(arrow.style.color, [253, 216, 53]);
        // Beside the box, the label tool still opens a new text.
        c.pointer_down(&mut host, p(450., 150.), Modifiers::default())
            .unwrap();
        assert!(host.text_open);
    }
    #[test]
    fn wheel_scales_arrow_head_and_shaft_around_the_fixed_tip() {
        let (mut c, mut host) = selected();
        c.action(&mut host, Action::Tool(Tool::Arrow));
        c.pointer_down(&mut host, p(200., 200.), Modifiers::default())
            .unwrap();
        c.pointer_move(&mut host, p(220., 200.), Modifiers::default());
        for delta in [120, 120, -120] {
            c.wheel(&mut host, delta).unwrap();
            let object = &c.editor.objects()[0];
            let paths = shotoshot_core::paths(object, c.editor.objects());
            let head = &paths[1];
            assert_eq!(head[1], p(220., 200.));
            assert!((head[1].x - head[0].x - object.style.width * 3.84).abs() < 1e-8);
        }
    }

    #[test]
    fn a_selection_keeps_its_colour_under_the_wheel_and_its_width_under_a_colour() {
        const BLUE: [u8; 3] = crate::toolbar::COLORS[3];
        let (mut c, mut host) = selected();
        c.action(&mut host, Action::Tool(Tool::Rectangle));
        c.action(&mut host, Action::Color(BLUE));
        c.editor.style.width = 4.;
        drag(&mut c, &mut host, p(150., 150.), p(250., 250.));
        // Another colour and width are current when the square is selected again.
        c.action(&mut host, Action::Tool(Tool::Select));
        c.action(&mut host, Action::Color(WHITE));
        for _ in 0..4 {
            c.action(&mut host, Action::Thicker);
        }
        assert_eq!(c.editor.style.width, 8.);
        let square = c.editor.objects()[0].id;
        c.editor.select_only(square);
        c.wheel(&mut host, 120).unwrap();
        assert_eq!(
            c.editor.objects()[0].style,
            Style {
                color: BLUE,
                width: 5.
            }
        );
        assert_eq!(
            c.editor.style.width, 5.,
            "the next notch starts from the square"
        );
        c.action(&mut host, Action::Color(BLACK));
        assert_eq!(
            c.editor.objects()[0].style,
            Style {
                color: BLACK,
                width: 5.
            }
        );
        c.editor.undo();
        assert_eq!(
            c.editor.objects()[0].style,
            Style {
                color: BLUE,
                width: 5.
            }
        );
    }

    const BLACK: [u8; 3] = crate::toolbar::COLORS[5];
    const WHITE: [u8; 3] = crate::toolbar::COLORS[4];
    /// The colour of the top coat.
    fn top(coats: &[shotoshot_core::Paint]) -> Option<[u8; 3]> {
        coats.last().map(|c| c.color)
    }
    fn click(c: &mut Capture, host: &mut FakeHost, at: Point, m: Modifiers) -> Option<Action> {
        let action = c.pointer_down(host, at, m).unwrap();
        c.pointer_up(host, at, m);
        action
    }
    fn drag(c: &mut Capture, host: &mut FakeHost, from: Point, to: Point) {
        c.pointer_down(host, from, Modifiers::default()).unwrap();
        c.pointer_up(host, to, Modifiers::default());
    }

    #[test]
    fn bucket_stays_in_hand_and_fills_zone_after_zone() {
        let (mut c, mut host) = selected();
        c.action(&mut host, Action::Tool(Tool::Rectangle));
        drag(&mut c, &mut host, p(150., 150.), p(250., 250.));
        drag(&mut c, &mut host, p(300., 150.), p(400., 250.));
        c.action(&mut host, Action::Color(BLACK));
        c.action(&mut host, Action::Fill);
        assert!(c.bucket);
        for zone in [p(200., 200.), p(350., 200.), p(500., 350.)] {
            assert_eq!(click(&mut c, &mut host, zone, Modifiers::default()), None);
            assert!(c.bucket, "still in hand after {zone:?}");
        }
        assert_eq!(top(&c.editor.objects()[0].fill), Some(BLACK));
        assert_eq!(top(&c.editor.objects()[1].fill), Some(BLACK));
        assert_eq!(top(c.editor.background()), Some(BLACK));
        assert!(c.editor.objects().len() == 2 && !host.captured);

        // The slate case: picking a tool puts the bucket down, then the press draws in white.
        c.action(&mut host, Action::Tool(Tool::Pen));
        c.action(&mut host, Action::Color(WHITE));
        assert!(!c.bucket);
        drag(&mut c, &mut host, p(500., 350.), p(560., 350.));
        assert_eq!(top(c.editor.background()), Some(BLACK));
        assert_eq!(c.editor.objects().len(), 3);
    }
    #[test]
    fn second_click_with_the_same_colour_takes_the_fill_back_and_keeps_the_bucket() {
        let (mut c, mut host) = selected();
        c.action(&mut host, Action::Color(BLACK));
        c.action(&mut host, Action::Fill);
        click(&mut c, &mut host, p(200., 200.), Modifiers::default());
        assert_eq!(top(c.editor.background()), Some(BLACK));
        click(&mut c, &mut host, p(200., 200.), Modifiers::default());
        assert_eq!(top(c.editor.background()), None);
        assert!(c.bucket);
    }
    #[test]
    fn bucket_fills_the_shape_under_the_pointer_and_leaves_the_capture_alone() {
        let (mut c, mut host) = selected();
        c.action(&mut host, Action::Tool(Tool::Rectangle));
        drag(&mut c, &mut host, p(200., 150.), p(400., 300.));
        c.action(&mut host, Action::Color(BLACK));
        c.action(&mut host, Action::Fill);
        click(&mut c, &mut host, p(300., 200.), Modifiers::default());
        assert_eq!(top(&c.editor.objects()[0].fill), Some(BLACK));
        assert_eq!(c.editor.objects().len(), 1, "the click drew nothing");
        assert_eq!(top(c.editor.background()), None);
    }
    #[test]
    fn bucket_on_a_drawn_line_recolours_it_and_stays_in_hand() {
        let (mut c, mut host) = selected();
        c.action(&mut host, Action::Tool(Tool::Line));
        drag(&mut c, &mut host, p(200., 200.), p(400., 200.));
        c.action(&mut host, Action::Fill);
        c.action(&mut host, Action::Color(BLACK));
        assert_ne!(c.editor.objects()[0].style.color, BLACK);
        click(&mut c, &mut host, p(300., 200.), Modifiers::default());
        assert_eq!(c.editor.objects()[0].style.color, BLACK);
        assert_eq!(top(c.editor.background()), None);
        assert!(c.bucket && c.editor.objects().len() == 1);
    }
    #[test]
    fn bucket_waits_while_its_colour_is_chosen() {
        let (mut c, mut host) = selected();
        c.action(&mut host, Action::Fill);
        let swatch = c
            .buttons()
            .into_iter()
            .find(|b| b.action == Action::Color(BLACK))
            .unwrap();
        let pressed = click(
            &mut c,
            &mut host,
            swatch.bounds.center(),
            Modifiers::default(),
        );
        assert_eq!(pressed, Some(Action::Color(BLACK)), "pressed, not filled");
        assert_eq!(top(c.editor.background()), None);
        for choosing in [
            Action::Color(BLACK),
            Action::CopyColor(BLACK),
            Action::Eyedropper,
            Action::CustomColor,
        ] {
            c.action(&mut host, choosing);
            assert!(c.bucket, "{choosing:?}");
        }
    }
    #[test]
    fn any_other_action_puts_the_bucket_down() {
        let (mut c, mut host) = selected();
        for other in [
            Action::Fill,
            Action::Tool(Tool::Arrow),
            Action::Undo,
            Action::FullScreen,
        ] {
            c.action(&mut host, Action::Fill);
            assert!(c.bucket);
            c.action(&mut host, other);
            assert!(!c.bucket, "{other:?}");
        }
    }
    #[test]
    fn bucket_stays_armed_after_a_click_outside_the_region() {
        let (mut c, mut host) = selected();
        c.action(&mut host, Action::Fill);
        click(&mut c, &mut host, p(20., 20.), Modifiers::default());
        assert!(c.bucket);
        assert_eq!(top(c.editor.background()), None);
        assert!(!c.editor.can_undo());
    }
    #[test]
    fn alt_click_with_the_bucket_clears_the_fill() {
        let (mut c, mut host) = selected();
        c.action(&mut host, Action::Fill);
        click(&mut c, &mut host, p(200., 200.), Modifiers::default());
        assert!(top(c.editor.background()).is_some());
        c.action(&mut host, Action::Color(BLACK));
        let alt = Modifiers {
            alt: true,
            ..Modifiers::default()
        };
        click(&mut c, &mut host, p(200., 200.), alt);
        assert_eq!(top(c.editor.background()), None);
        assert!(c.bucket);
    }
    #[test]
    fn right_click_with_the_bucket_takes_the_colour_off() {
        let (mut c, mut host) = selected();
        c.action(&mut host, Action::Tool(Tool::Rectangle));
        drag(&mut c, &mut host, p(200., 150.), p(400., 300.));
        c.action(&mut host, Action::Color(BLACK));
        c.action(&mut host, Action::Fill);
        click(&mut c, &mut host, p(300., 200.), Modifiers::default());
        click(&mut c, &mut host, p(500., 350.), Modifiers::default());
        c.action(&mut host, Action::Color(WHITE));
        assert!(
            c.secondary_down(&mut host, p(300., 200.)),
            "whatever the colour in hand"
        );
        assert_eq!(top(&c.editor.objects()[0].fill), None);
        assert!(c.secondary_down(&mut host, p(500., 350.)));
        assert_eq!(top(c.editor.background()), None);
        assert!(c.bucket, "still in hand");
        assert!(
            !c.secondary_down(&mut host, p(500., 350.)),
            "nothing left to take off"
        );
        c.editor.undo();
        assert!(
            !c.secondary_down(&mut host, p(20., 20.)),
            "outside the region"
        );
        c.action(&mut host, Action::Tool(Tool::Arrow));
        assert!(
            !c.secondary_down(&mut host, p(500., 350.)),
            "the bucket is down"
        );
        assert_eq!(top(c.editor.background()), Some(BLACK));
    }
    #[test]
    fn region_handles_keep_resizing_under_an_armed_bucket() {
        let (mut c, mut host) = selected();
        c.action(&mut host, Action::Fill);
        assert_eq!(c.cursor(p(600., 400.)), crate::CursorKind::ResizeNwse);
        drag(&mut c, &mut host, p(600., 400.), p(700., 500.));
        assert_eq!(c.region, Rect::new(p(100., 100.), p(700., 500.)));
        assert!(c.bucket);
        assert_eq!(top(c.editor.background()), None);
    }
    #[test]
    fn armed_bucket_shows_on_its_button_with_a_cross_and_no_stroke_ring() {
        let (mut c, mut host) = selected();
        c.action(&mut host, Action::Tool(Tool::Pen));
        c.pointer_move(&mut host, p(200., 200.), Modifiers::default());
        let lit = |c: &Capture| {
            c.buttons()
                .into_iter()
                .find(|b| b.action == Action::Fill)
                .unwrap()
                .active
        };
        assert!(!lit(&c) && c.width_ring_visible());
        c.action(&mut host, Action::Tool(Tool::Select));
        assert_eq!(c.cursor(p(200., 200.)), crate::CursorKind::Arrow);
        c.action(&mut host, Action::Tool(Tool::Pen));
        c.action(&mut host, Action::Fill);
        assert!(lit(&c) && !c.width_ring_visible());
        c.action(&mut host, Action::Tool(Tool::Select));
        c.action(&mut host, Action::Fill);
        assert_eq!(c.cursor(p(200., 200.)), crate::CursorKind::Cross);
    }
    #[test]
    fn bucket_click_on_a_rotation_handle_fills_instead_of_turning() {
        let (mut c, mut host) = selected();
        c.action(&mut host, Action::Tool(Tool::Rectangle));
        drag(&mut c, &mut host, p(200., 150.), p(400., 300.));
        c.action(&mut host, Action::Tool(Tool::Select));
        c.editor.select_only(c.editor.objects()[0].id);
        let handle = c.rotation_handle().unwrap();
        c.action(&mut host, Action::Fill);
        c.pointer_down(&mut host, handle, Modifiers::default())
            .unwrap();
        c.pointer_up(&mut host, p(500., 300.), Modifiers::default());
        assert_eq!(c.editor.objects()[0].rotation, 0.);
        assert!(top(c.editor.background()).is_some());
    }
    #[test]
    fn wheel_is_inert_while_the_bucket_is_armed() {
        let (mut c, mut host) = selected();
        let width = c.editor.style.width;
        c.action(&mut host, Action::Fill);
        c.wheel(&mut host, 120).unwrap();
        assert_eq!(c.editor.style.width, width);
        assert!(c.bucket);
    }
    /// A blur dragged from `from` to `to` with the blur tool: its id.
    fn blur(c: &mut Capture, host: &mut FakeHost, from: Point, to: Point) -> u64 {
        let m = Modifiers::default();
        c.action(host, Action::Tool(Tool::Blur));
        c.pointer_down(host, from, m).unwrap();
        c.pointer_move(host, to, m);
        c.pointer_up(host, to, m);
        c.editor.objects().last().unwrap().id
    }
    #[test]
    fn the_blur_tool_drags_a_zone_at_the_strength_the_wheel_set() {
        let (mut c, mut host) = selected();
        c.action(&mut host, Action::Tool(Tool::Blur));
        let pen = c.editor.style.width;
        c.wheel(&mut host, 120).unwrap();
        assert_eq!(c.editor.blur_radius, 8., "one rung up from 7");
        assert_eq!(c.editor.style.width, pen, "the pen keeps its width");
        blur(&mut c, &mut host, p(150., 150.), p(350., 220.));
        let o = &c.editor.objects()[0];
        assert_eq!(
            o.shape,
            Shape::Blur(Rect::new(p(150., 150.), p(350., 220.)).unwrap())
        );
        assert_eq!(o.style.width, 8.);
        assert!(!host.captured, "the pointer is let go");
        // The wheel while dragging sets the strength of the blur being drawn.
        let m = Modifiers::default();
        c.pointer_down(&mut host, p(400., 150.), m).unwrap();
        c.pointer_move(&mut host, p(500., 250.), m);
        c.wheel(&mut host, 240).unwrap();
        c.pointer_up(&mut host, p(500., 250.), m);
        assert_eq!(c.editor.objects()[1].style.width, 12.);
        c.action(&mut host, Action::Undo);
        assert_eq!(
            c.editor.objects().len(),
            1,
            "drawn and strengthened: one step"
        );
    }
    #[test]
    fn the_wheel_strengthens_the_selected_blurs_in_one_step() {
        let (mut c, mut host) = selected();
        let id = blur(&mut c, &mut host, p(150., 150.), p(350., 220.));
        c.action(&mut host, Action::Tool(Tool::Select));
        c.editor.select_only(id);
        c.wheel(&mut host, -120).unwrap();
        assert_eq!(c.editor.objects()[0].style.width, 6.);
        assert_eq!(c.editor.blur_radius, 6., "the next blur follows");
        assert_eq!(c.editor.style.width, Style::default().width);
        c.action(&mut host, Action::Undo);
        assert_eq!(c.editor.objects()[0].style.width, 7.);
        assert_eq!(c.editor.objects().len(), 1);
    }
    #[test]
    fn the_palette_and_the_bucket_leave_a_blur_alone() {
        let (mut c, mut host) = selected();
        let id = blur(&mut c, &mut host, p(150., 150.), p(350., 220.));
        c.action(&mut host, Action::Tool(Tool::Select));
        c.editor.select_only(id);
        let before = c.editor.objects().to_vec();
        c.action(&mut host, Action::Color(crate::toolbar::COLORS[3]));
        assert_eq!(c.editor.objects(), before);
        c.action(&mut host, Action::Fill);
        c.pointer_down(&mut host, p(250., 180.), Modifiers::default())
            .unwrap();
        assert_eq!(c.editor.objects(), before, "the bucket colours the capture");
        assert_eq!(c.editor.background().len(), 1);
    }
    #[test]
    fn a_board_has_no_blur_tool_and_pastes_no_blur() {
        let (mut c, mut host) = selected();
        let id = blur(&mut c, &mut host, p(150., 150.), p(350., 220.));
        c.editor.select_only(id);
        c.copied_objects = c.editor.copy_selected();
        assert_eq!(c.clipboard().len(), 1);
        c.board = true;
        assert!(c.clipboard().is_empty());
        c.action(&mut host, Action::Tool(Tool::Pen));
        c.action(&mut host, Action::Tool(Tool::Blur));
        assert_eq!(c.editor.tool, Tool::Pen);
        assert_eq!(
            c.key(&mut host, 88, Modifiers::default()),
            crate::keys::KeyOutcome::Handled
        );
        assert!(
            c.buttons()
                .iter()
                .all(|b| b.action != Action::Tool(Tool::Blur))
        );
    }
    #[test]
    fn escape_puts_the_bucket_down_first() {
        let (mut c, mut host) = selected();
        c.action(&mut host, Action::Fill);
        assert!(c.cancel_gesture(&mut host));
        assert!(!c.bucket);
        assert!(!c.cancel_gesture(&mut host));
    }

    #[test]
    fn dragging_selects_a_region_and_releases_the_pointer() {
        let (c, host) = selected();
        assert!(!c.selecting && !host.captured);
        assert_eq!(c.region, Rect::new(p(100., 100.), p(600., 400.)));
    }
    #[test]
    fn a_click_without_drag_keeps_waiting_for_a_region() {
        let mut c = Capture::new(Image::solid(900, 600, [0, 0, 0, 255]));
        let mut host = FakeHost::default();
        c.pointer_down(&mut host, p(50., 50.), Modifiers::default())
            .unwrap();
        c.pointer_up(&mut host, p(51., 50.), Modifiers::default());
        assert!(c.selecting && c.region.is_none());
    }
    #[test]
    fn toolbar_press_returns_its_action_without_drawing() {
        let (mut c, mut host) = selected();
        let button = c.buttons()[0].clone();
        let action = c
            .pointer_down(&mut host, button.bounds.center(), Modifiers::default())
            .unwrap();
        assert_eq!(action, Some(button.action));
        assert!(c.editor.objects().is_empty() && !host.captured);
    }
    #[test]
    fn drawing_tools_set_their_shape_down_off_the_region() {
        let m = Modifiers::default();
        for tool in [
            Tool::Arrow,
            Tool::Line,
            Tool::Rectangle,
            Tool::Ellipse,
            Tool::Pen,
            Tool::Highlight,
        ] {
            let (mut c, mut host) = selected();
            c.action(&mut host, Action::Tool(tool));
            // Left of and above the region (100, 100)–(600, 400).
            c.pointer_down(&mut host, p(20., 20.), m).unwrap();
            c.pointer_move(&mut host, p(80., 60.), m);
            c.pointer_up(&mut host, p(80., 60.), m);
            assert_eq!(c.editor.objects().len(), 1, "{tool:?} draws off the region");
        }
        let (mut c, mut host) = selected();
        c.action(&mut host, Action::Tool(Tool::Text));
        c.pointer_down(&mut host, p(20., 20.), m).unwrap();
        assert_eq!(host.text_at, Some(p(20., 20.)), "text starts there too");
    }
    #[test]
    fn the_eraser_takes_a_shape_by_the_part_beyond_the_region() {
        let m = Modifiers::default();
        let (mut c, mut host) = selected();
        c.action(&mut host, Action::Tool(Tool::Line));
        c.pointer_down(&mut host, p(300., 300.), m).unwrap();
        c.pointer_move(&mut host, p(800., 500.), m);
        c.pointer_up(&mut host, p(800., 500.), m);
        assert_eq!(c.editor.objects().len(), 1);
        c.action(&mut host, Action::Tool(Tool::Eraser));
        // On the line, past the region's corner (600, 400).
        c.pointer_down(&mut host, p(750., 480.), m).unwrap();
        c.pointer_up(&mut host, p(750., 480.), m);
        assert!(c.editor.objects().is_empty());
    }
    #[test]
    fn full_screen_button_press_toggles_without_drawing() {
        let (mut c, mut host) = selected();
        let before = c.region;
        for expected in [Rect::new(p(0., 0.), p(900., 600.)), before] {
            let button = c.screen_button_bounds().unwrap().center();
            c.pointer_down(&mut host, button, Modifiers::default())
                .unwrap();
            c.pointer_up(&mut host, button, Modifiers::default());
            assert_eq!(c.region, expected);
        }
        assert!(c.editor.objects().is_empty() && !c.selecting && !host.captured);
    }
    #[test]
    fn full_screen_shortcut_toggles_like_the_button() {
        let (mut c, mut host) = selected();
        let before = c.region;
        c.action(&mut host, Action::FullScreen);
        assert_eq!(c.region, Rect::new(p(0., 0.), p(900., 600.)));
        c.action(&mut host, Action::FullScreen);
        assert_eq!(c.region, before);
    }
    #[test]
    fn escape_restores_the_region_being_resized() {
        let (mut c, mut host) = selected();
        let before = c.region;
        c.pointer_down(&mut host, p(600., 400.), Modifiers::default())
            .unwrap();
        c.pointer_move(&mut host, p(700., 500.), Modifiers::default());
        assert_ne!(c.region, before);
        assert!(c.cancel_gesture(&mut host));
        assert_eq!(c.region, before);
        assert!(!c.selecting);
    }
    #[test]
    fn the_wheel_sizes_the_eraser_and_leaves_the_stroke_width_alone() {
        let (mut c, mut host) = selected();
        c.editor.style.width = 8.;
        c.action(&mut host, Action::Tool(Tool::Eraser));
        assert_eq!(c.editor.eraser_size, shotoshot_core::ERASER_SIZE);
        assert!(c.width_ring_visible(), "the disc shows at the pointer");
        c.wheel(&mut host, 120).unwrap();
        assert_eq!(c.editor.eraser_size, 16.);
        assert!(c.width_preview);
        c.wheel(&mut host, -360).unwrap();
        assert_eq!(c.editor.eraser_size, 10.);
        assert_eq!(c.editor.style.width, 8., "the pen keeps its width");
        assert_eq!(c.eraser_size_setting(), 10);
    }
    #[test]
    fn one_wheel_notch_moves_the_stroke_width_one_rung() {
        let (mut c, mut host) = selected();
        c.action(&mut host, Action::Tool(Tool::Rectangle));
        c.editor.style.width = 8.;
        c.wheel(&mut host, 120).unwrap();
        assert_eq!(c.editor.style.width, 10.);
        assert!(c.width_preview);
        c.wheel(&mut host, 240).unwrap();
        assert_eq!(c.editor.style.width, 14.);
        c.wheel(&mut host, -120).unwrap();
        assert_eq!(c.editor.style.width, 12.);
    }
    #[test]
    fn fine_wheel_travel_changes_the_width_once_per_notch() {
        let (mut c, mut host) = selected();
        c.action(&mut host, Action::Tool(Tool::Rectangle));
        c.editor.style.width = 4.;
        for _ in 0..3 {
            c.wheel(&mut host, 30).unwrap();
        }
        assert_eq!(c.editor.style.width, 4.);
        c.wheel(&mut host, 30).unwrap();
        assert_eq!(c.editor.style.width, 5.);
    }
    #[test]
    fn wheel_resizes_the_open_text_field_instead_of_the_stroke() {
        let (mut c, mut host) = selected();
        c.action(&mut host, Action::Tool(Tool::Text));
        c.pointer_down(&mut host, p(200., 200.), Modifiers::default())
            .unwrap();
        let stroke = c.editor.style.width;
        c.wheel(&mut host, 120).unwrap();
        assert_eq!((host.text_size, c.text_size), (28., 28.));
        c.wheel(&mut host, -240).unwrap();
        assert_eq!((host.text_size, c.text_size), (22., 22.));
        assert_eq!(c.editor.style.width, stroke);
    }
    #[test]
    fn wheel_sets_the_next_text_size_before_any_field_opens() {
        let (mut c, mut host) = selected();
        c.action(&mut host, Action::Tool(Tool::Text));
        c.wheel(&mut host, 120).unwrap();
        assert_eq!(c.text_size, 28.);
    }
    #[test]
    fn the_wheel_hint_shows_with_drawing_tools_until_the_first_notch() {
        let (mut c, mut host) = selected();
        c.wheel_hint = true;
        c.pointer = p(200., 200.);
        for tool in [Tool::Arrow, Tool::Pen, Tool::Text, Tool::Eraser] {
            c.action(&mut host, Action::Tool(tool));
            assert!(c.wheel_hint_visible(), "{tool:?}");
        }
        c.action(&mut host, Action::Tool(Tool::Select));
        assert!(!c.wheel_hint_visible(), "selecting does not resize a tool");
        c.action(&mut host, Action::Tool(Tool::Rectangle));
        c.pointer = p(-100., -100.);
        assert!(!c.wheel_hint_visible(), "no badge pinned to a corner");
        c.pointer = p(200., 200.);
        c.wheel(&mut host, 30).unwrap();
        assert!(c.wheel_hint, "a fraction of a notch resizes nothing yet");
        c.wheel(&mut host, 90).unwrap();
        assert!(!c.wheel_hint && !c.wheel_hint_visible());
    }
    #[test]
    fn wheel_travel_over_the_color_picker_is_not_kept_for_later() {
        let (mut c, mut host) = selected();
        c.action(&mut host, Action::Tool(Tool::Rectangle));
        c.editor.style.width = 4.;
        c.action(&mut host, Action::CustomColor);
        c.wheel(&mut host, 90).unwrap();
        assert!(c.cancel_gesture(&mut host));
        c.wheel(&mut host, 90).unwrap();
        assert_eq!(c.editor.style.width, 4.);
    }
    #[test]
    fn a_label_click_fits_its_panel_and_a_drag_sets_its_width() {
        let m = Modifiers::default();
        let (mut c, mut host) = selected();
        c.action(&mut host, Action::Tool(Tool::Label));
        c.text_size = 20.;
        // 20 px font: 8 px margins across, 5 px down.
        let pad = Label::padding(20.);
        c.pointer_down(&mut host, p(200., 200.), m).unwrap();
        assert!(
            host.text_open,
            "the field opens at the press, ready for typing"
        );
        assert_eq!(host.label, Some((p(200., 200.) + pad, Some(Label::Fit))));
        c.pointer_move(&mut host, p(203., 202.), m);
        assert_eq!(host.label.unwrap().1, Some(Label::Fit), "a shaking click");
        c.pointer_up(&mut host, p(203., 202.), m);
        assert!(!host.captured && c.label_drag.is_none());

        host.text_open = false;
        c.pointer_down(&mut host, p(400., 300.), m).unwrap();
        c.pointer_move(&mut host, p(250., 380.), m);
        // Dragged to the left: the panel starts at the left edge, 150 px wide.
        assert_eq!(
            host.label,
            Some((p(250., 300.) + pad, Some(Label::Wrap(150. - 16.))))
        );
        c.pointer_up(&mut host, p(250., 380.), m);
        assert_eq!(host.label.unwrap().1, Some(Label::Wrap(134.)));

        host.text_open = false;
        c.pointer_down(&mut host, p(400., 300.), m).unwrap();
        c.pointer_move(&mut host, p(410., 300.), m);
        assert_eq!(
            host.label.unwrap().1,
            Some(Label::Wrap(20.)),
            "never narrower than two letters"
        );
    }
    #[test]
    fn the_text_tool_opens_a_text_without_a_panel() {
        let (mut c, mut host) = selected();
        c.new_label = Some(Label::Fit);
        c.action(&mut host, Action::Tool(Tool::Text));
        c.pointer_down(&mut host, p(200., 200.), Modifiers::default())
            .unwrap();
        assert_eq!(host.label, Some((p(200., 200.), None)));
    }
    #[test]
    fn the_label_tool_edits_a_text_it_presses() {
        let (mut c, mut host) = selected();
        c.editor
            .add_label(
                p(200., 200.),
                "hi".into(),
                p(20., 20.),
                [0; 3],
                20.,
                None,
                None,
            )
            .unwrap();
        c.action(&mut host, Action::Tool(Tool::Label));
        c.pointer_down(&mut host, p(205., 205.), Modifiers::default())
            .unwrap();
        assert_eq!(host.label, Some((p(205., 205.), None)), "no panel is added");
        assert!(c.label_drag.is_none(), "no drag follows");
    }
    #[test]
    fn the_wheel_measures_a_wrapped_label_at_its_width() {
        let (mut c, mut host) = selected();
        let id = c
            .editor
            .add_label(
                p(200., 200.),
                "aaaa bbbb".into(),
                p(20., 20.),
                [0; 3],
                20.,
                Some(Label::Wrap(50.)),
                None,
            )
            .unwrap();
        c.editor.selected = vec![id];
        c.wheel(&mut host, 120).unwrap();
        let Shape::Text { size, extent, .. } = c.editor.objects()[0].shape else {
            panic!("text expected");
        };
        // The stand-in measure gives half the size per letter: one word per line.
        assert_eq!(extent.y, 2. * size);
    }
    const YELLOW: [u8; 3] = [250, 192, 66];
    const SKY: [u8; 3] = crate::toolbar::COLORS[3];
    const GRASS: [u8; 3] = crate::toolbar::COLORS[2];
    const SNOW: [u8; 3] = crate::toolbar::COLORS[4];
    fn button(c: &Capture, action: Action) -> Option<crate::toolbar::Button> {
        c.buttons().into_iter().find(|b| b.action == action)
    }
    /// A text on yellow at (200, 200), boxed unless `plain`, selected, with the text box
    /// tool in hand.
    fn boxed(plain: bool) -> (Capture, FakeHost, u64) {
        let (mut c, mut host) = selected();
        c.action(&mut host, Action::Tool(Tool::Label));
        let label = (!plain).then_some(Label::Fit);
        let id = c
            .editor
            .add_label(
                p(200., 200.),
                "hi".into(),
                p(20., 20.),
                YELLOW,
                20.,
                label,
                None,
            )
            .unwrap();
        c.editor.selected = vec![id];
        (c, host, id)
    }
    fn ink(c: &Capture) -> Option<[u8; 3]> {
        match c.editor.objects()[0].shape {
            Shape::Text { ink, .. } => ink,
            _ => panic!("text expected"),
        }
    }
    #[test]
    fn box_and_text_colour_show_before_the_swatches_only_for_text_boxes() {
        let (mut c, mut host) = selected();
        let shown = |c: &Capture| {
            let actions: Vec<_> = c.buttons().iter().map(|b| b.action).collect();
            let at = |action| actions.iter().position(|a| *a == action);
            match (at(Action::BoxColor), at(Action::TextColor)) {
                (Some(b), Some(t)) => {
                    let red = at(Action::Color(crate::toolbar::COLORS[0]));
                    assert_eq!((t, red), (b + 1, Some(b + 2)), "just before the swatches");
                    true
                }
                (None, None) => false,
                _ => panic!("both or neither"),
            }
        };
        assert!(!shown(&c), "a capture opens as before");
        c.action(&mut host, Action::Tool(Tool::Pen));
        assert!(!shown(&c));
        c.action(&mut host, Action::Tool(Tool::Label));
        assert!(shown(&c), "the text box tool in hand");
        assert!(!c.ink_target, "a capture starts on the box colour");
        assert!(button(&c, Action::BoxColor).unwrap().active);
        assert!(!button(&c, Action::TextColor).unwrap().active);

        let (mut c, mut host, id) = boxed(false);
        c.action(&mut host, Action::Tool(Tool::Select));
        assert!(!shown(&c), "nothing selected");
        c.editor.selected = vec![id];
        assert!(shown(&c), "a selected text box");
        let (mut c, mut host, id) = boxed(true);
        c.action(&mut host, Action::Tool(Tool::Select));
        c.editor.selected = vec![id];
        assert!(!shown(&c), "a plain text has no box");
    }
    #[test]
    fn the_palette_colours_the_chosen_target_until_the_other_is_clicked() {
        let (mut c, mut host, _) = boxed(false);
        let m = Modifiers::default();
        let text_color = button(&c, Action::TextColor).unwrap().bounds.center();
        assert_eq!(
            c.pointer_down(&mut host, text_color, m).unwrap(),
            Some(Action::TextColor)
        );
        c.action(&mut host, Action::TextColor);
        assert!(button(&c, Action::TextColor).unwrap().active);
        assert!(!button(&c, Action::BoxColor).unwrap().active);
        c.action(&mut host, Action::Color(SKY));
        assert_eq!(ink(&c), Some(SKY), "a swatch colours the text");
        assert_eq!(c.editor.objects()[0].style.color, YELLOW, "not the box");
        assert_ne!(c.editor.style.color, SKY, "nor the pen");
        c.action(&mut host, Action::Color(SNOW));
        assert_eq!(ink(&c), Some(SNOW), "the target stays");
        c.accept_custom_color(&mut host, [1, 2, 3]);
        assert_eq!(ink(&c), Some([1, 2, 3]), "the custom colour too");
        c.action(&mut host, Action::Eyedropper);
        let Some(Action::CopyColor(picked)) = c.pointer_down(&mut host, p(300., 300.), m).unwrap()
        else {
            panic!("the eyedropper picks");
        };
        assert_eq!(ink(&c), Some(picked), "and the eyedropper");
        assert!(c.ink_target);

        c.action(&mut host, Action::BoxColor);
        c.action(&mut host, Action::Color(SKY));
        assert_eq!(c.editor.objects()[0].style.color, SKY, "the box, as before");
        assert_eq!(ink(&c), Some(picked), "its text keeps the ink given");
    }
    #[test]
    fn right_click_on_text_colour_gives_the_automatic_ink_back_in_one_step() {
        let (mut c, mut host, _) = boxed(false);
        c.action(&mut host, Action::TextColor);
        c.action(&mut host, Action::Color(SKY));
        let text_color = button(&c, Action::TextColor).unwrap().bounds.center();
        c.action(&mut host, Action::BoxColor);
        assert!(c.secondary_down(&mut host, text_color));
        assert_eq!(ink(&c), None);
        assert_eq!(c.label_ink, None, "the next boxes too");
        assert_eq!(c.editor.objects()[0].text_ink(), [0; 3], "dark on yellow");
        assert!(!c.ink_target, "the target is left as it was");
        assert!(
            !c.secondary_down(&mut host, p(300., 300.)),
            "elsewhere, nothing"
        );
        c.editor.undo();
        assert_eq!(ink(&c), Some(SKY), "one undo step");
        c.editor.undo();
        assert_eq!(ink(&c), None, "the ink given was one step too");
    }
    #[test]
    fn the_next_boxes_and_the_open_field_take_the_ink() {
        let (mut c, mut host) = selected();
        let m = Modifiers::default();
        c.action(&mut host, Action::Tool(Tool::Label));
        c.action(&mut host, Action::TextColor);
        c.action(&mut host, Action::Color(GRASS));
        assert!(c.editor.objects().is_empty());
        c.pointer_down(&mut host, p(200., 200.), m).unwrap();
        c.pointer_up(&mut host, p(200., 200.), m);
        assert_eq!(host.ink, Some(GRASS), "a new box opens in the ink given");
        let color = host.color;
        c.action(&mut host, Action::Color(SKY));
        assert_eq!(host.ink, Some(SKY), "the field's text changes live");
        assert_eq!(host.color, color, "its box does not");
        let text_color = button(&c, Action::TextColor).unwrap().bounds.center();
        assert_eq!(
            c.pointer_down(&mut host, text_color, m).unwrap(),
            Some(Action::TextColor)
        );
        assert!(host.text_open, "the target buttons keep the field open");
        assert!(c.secondary_down(&mut host, text_color));
        assert_eq!(host.ink, None, "automatic again while typing");
        c.action(&mut host, Action::BoxColor);
        c.action(&mut host, Action::Color(SNOW));
        assert_eq!(host.color, Some(SNOW), "the box colour goes to the field");
    }
    #[test]
    fn the_bucket_and_other_tools_keep_colouring_as_before() {
        let (mut c, mut host, _) = boxed(false);
        c.action(&mut host, Action::TextColor);
        c.action(&mut host, Action::Fill);
        c.action(&mut host, Action::Color(SKY));
        assert!(c.bucket, "the bucket waits for its colour");
        assert_eq!(c.editor.style.color, SKY, "and takes it");
        assert_eq!(ink(&c), None);
        c.action(&mut host, Action::Tool(Tool::Pen));
        c.action(&mut host, Action::Color(SNOW));
        assert_eq!(c.editor.style.color, SNOW, "the pen takes the colour");
    }
}
