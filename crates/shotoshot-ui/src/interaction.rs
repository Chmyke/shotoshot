//! ROLE: Pointer state machine and toolbar actions of a capture session.
//! INVARIANTS: Inline native fields get the first chance at each press; a press never
//! both starts a gesture and triggers a button.
use crate::{Capture, Host, color_picker::ColorPicker, toolbar::Action, wheel};
use shotoshot_core::{Modifiers, Point, Rect, Shape, Tool};

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
        self.width_preview = false;
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
            let color = picker.color();
            self.picker = None;
            self.accept_custom_color(host, color);
            host.focus_text();
            return Ok(None);
        }
        if self.eyedropper {
            let rgb = self.sample_color(p);
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
                    Action::Color(_) | Action::CustomColor | Action::Eyedropper
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
        if host.text_pointer_down(p)? {
            return Ok(None);
        }
        if host.text_active() {
            host.finish_text(self, true);
        }
        if self.editor.tool == Tool::Select
            && self
                .rotation_handle()
                .is_some_and(|h| (h - p).length() <= 9.)
        {
            self.editor.begin_rotation(p);
            host.capture_pointer();
            return Ok(None);
        }
        if self.selecting {
            self.region_before = self.region;
            self.region_origin = Some(p);
            self.region = Rect::new(p, p);
            host.capture_pointer();
            return Ok(None);
        }
        if let Some(region) = self.region
            && let Some(index) = self.region_handle(p)
        {
            self.region_before = self.region;
            self.region_origin = Some(region.min);
            self.region_resize = Some(index);
            self.selecting = true;
            host.capture_pointer();
            return Ok(None);
        }
        // The crop limits export, not manipulation of existing annotations.
        if self.editor.tool != Tool::Select && !self.region.is_some_and(|r| r.contains(p)) {
            return Ok(None);
        }
        if self.editor.tool == Tool::Text {
            host.begin_text(self, p)?;
        } else {
            self.editor.begin(p, modifiers);
            host.capture_pointer();
        }
        Ok(None)
    }
    pub fn pointer_move<H: Host>(&mut self, host: &mut H, p: Point, modifiers: Modifiers) {
        let p = self.clamp(p);
        self.pointer = p;
        if host.dimensions_active() {
            host.dimension_pointer_move(p);
            return;
        }
        if let Some(picker) = &mut self.picker {
            picker.pointer_move(p);
            return;
        }
        if host.text_pointer_move(p) {
            return;
        }
        if let Some(origin) = self.region_origin {
            if let (Some(index), Some(before)) = (self.region_resize, self.region_before) {
                let mut min = before.min;
                let mut max = before.max;
                if [0, 6, 7].contains(&index) {
                    min.x = p.x;
                }
                if [2, 3, 4].contains(&index) {
                    max.x = p.x;
                }
                if [0, 1, 2].contains(&index) {
                    min.y = p.y;
                }
                if [4, 5, 6].contains(&index) {
                    max.y = p.y;
                }
                self.region = Rect::new(min, max);
            } else {
                self.region = Rect::new(origin, p);
            }
        } else {
            self.editor.update_with_modifiers(p, modifiers);
        }
    }
    pub fn pointer_up<H: Host>(&mut self, host: &mut H, p: Point, modifiers: Modifiers) {
        self.pointer_move(host, p, modifiers);
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
        } else {
            self.editor.finish();
        }
        host.release_pointer();
    }
    /// Toolbar and shortcut actions that only change the capture itself.
    pub fn action<H: Host>(&mut self, host: &mut H, a: Action) {
        if !matches!(a, Action::Thinner | Action::Thicker) {
            self.editor.cancel();
        }
        self.eyedropper = false;
        match a {
            Action::CustomColor => {
                self.picker = Some(ColorPicker::new(
                    self.custom_color,
                    self.pointer,
                    self.image.width,
                    self.image.height,
                ));
            }
            Action::Eyedropper => {
                self.eyedropper = true;
            }
            Action::Tool(t) => {
                self.editor.tool = t;
                self.editor.selected.clear();
            }
            Action::Color(c) => {
                self.editor.style.color = c;
                if host.text_active() {
                    host.set_text_color(c);
                } else {
                    self.editor.apply_style();
                }
            }
            Action::Thinner | Action::Thicker => {
                let notch = if a == Action::Thicker { 1 } else { -1 };
                self.width_preview = true;
                self.editor.style.width = wheel::stroke_width_after(self.editor.style.width, notch);
                self.editor.apply_style();
            }
            Action::FullScreen if !self.selecting => self.toggle_screen(),
            Action::Undo => self.editor.undo(),
            Action::Redo => self.editor.redo(),
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
        if self.picker.is_some() || host.dimensions_active() || self.eyedropper {
            return Ok(());
        }
        let notches = self.wheel_travel.take(delta);
        if notches == 0 {
            return Ok(());
        }
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
                    && let Shape::Text { text, size, .. } = &o.shape
                {
                    Some((o.id, text.clone(), *size))
                } else {
                    None
                }
            })
            .collect();
        if !texts.is_empty() {
            let mut changes = Vec::with_capacity(texts.len());
            for (id, text, size) in texts {
                let size = wheel::text_size_after(size, notches);
                // Sizes go through f32 exactly as the native text field stores them.
                let extent = host.measure_text(&text, size as f32 as f64)?;
                changes.push((id, size, extent));
                self.text_size = size as f32;
            }
            self.editor.set_text_sizes(&changes);
            self.width_preview = false;
        } else if self.editor.tool == Tool::Text {
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
    pub fn accept_custom_color<H: Host>(&mut self, host: &mut H, color: [u8; 3]) {
        self.custom_color = color;
        self.custom_color_changed = true;
        self.action(host, Action::Color(color));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Image, test_host::FakeHost};

    fn p(x: f64, y: f64) -> Point {
        Point { x, y }
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
    fn text_tool_opens_the_native_field_inside_the_region_only() {
        let (mut c, mut host) = selected();
        c.action(&mut host, Action::Tool(Tool::Text));
        c.pointer_down(&mut host, p(20., 20.), Modifiers::default())
            .unwrap();
        assert_eq!(host.text_at, None);
        c.pointer_down(&mut host, p(200., 200.), Modifiers::default())
            .unwrap();
        assert_eq!(host.text_at, Some(p(200., 200.)));
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
}
