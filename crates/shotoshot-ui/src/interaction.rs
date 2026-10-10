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
        self.adopt_scale();
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
            && !self.bucket
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
        if self.bucket {
            // The bucket stays in hand, zone after zone, until another action puts it down.
            if self.region.is_some_and(|r| r.contains(p)) {
                self.editor.fill_at(p, modifiers.alt);
            }
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
        self.adopt_scale();
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
                Action::Color(_) | Action::CopyColor(_) | Action::CustomColor | Action::Eyedropper
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
    use crate::{Image, Monitor, test_host::FakeHost};

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

    const BLACK: [u8; 3] = crate::toolbar::COLORS[5];
    const WHITE: [u8; 3] = crate::toolbar::COLORS[4];
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
        assert_eq!(c.editor.objects()[0].fill, Some(BLACK));
        assert_eq!(c.editor.objects()[1].fill, Some(BLACK));
        assert_eq!(c.editor.background(), Some(BLACK));
        assert!(c.editor.objects().len() == 2 && !host.captured);

        // The slate case: picking a tool puts the bucket down, then the press draws in white.
        c.action(&mut host, Action::Tool(Tool::Pen));
        c.action(&mut host, Action::Color(WHITE));
        assert!(!c.bucket);
        drag(&mut c, &mut host, p(500., 350.), p(560., 350.));
        assert_eq!(c.editor.background(), Some(BLACK));
        assert_eq!(c.editor.objects().len(), 3);
    }
    #[test]
    fn second_click_with_the_same_colour_takes_the_fill_back_and_keeps_the_bucket() {
        let (mut c, mut host) = selected();
        c.action(&mut host, Action::Color(BLACK));
        c.action(&mut host, Action::Fill);
        click(&mut c, &mut host, p(200., 200.), Modifiers::default());
        assert_eq!(c.editor.background(), Some(BLACK));
        click(&mut c, &mut host, p(200., 200.), Modifiers::default());
        assert_eq!(c.editor.background(), None);
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
        assert_eq!(c.editor.objects()[0].fill, Some(BLACK));
        assert_eq!(c.editor.objects().len(), 1, "the click drew nothing");
        assert_eq!(c.editor.background(), None);
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
        assert_eq!(c.editor.background(), None);
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
        assert_eq!(c.editor.background(), None);
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
        assert_eq!(c.editor.background(), None);
        assert!(!c.editor.can_undo());
    }
    #[test]
    fn alt_click_with_the_bucket_clears_the_fill() {
        let (mut c, mut host) = selected();
        c.action(&mut host, Action::Fill);
        click(&mut c, &mut host, p(200., 200.), Modifiers::default());
        assert!(c.editor.background().is_some());
        c.action(&mut host, Action::Color(BLACK));
        let alt = Modifiers {
            alt: true,
            ..Modifiers::default()
        };
        click(&mut c, &mut host, p(200., 200.), alt);
        assert_eq!(c.editor.background(), None);
        assert!(c.bucket);
    }
    #[test]
    fn region_handles_keep_resizing_under_an_armed_bucket() {
        let (mut c, mut host) = selected();
        c.action(&mut host, Action::Fill);
        assert_eq!(c.cursor(p(600., 400.)), crate::CursorKind::ResizeNwse);
        drag(&mut c, &mut host, p(600., 400.), p(700., 500.));
        assert_eq!(c.region, Rect::new(p(100., 100.), p(700., 500.)));
        assert!(c.bucket);
        assert_eq!(c.editor.background(), None);
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
        assert!(c.editor.background().is_some());
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
