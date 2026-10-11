//! ROLE: The view of one screen during a board session: a zoom and the frame point shown at
//! the screen's top-left corner. Screen points are window pixels; frame points are the
//! capture's. At zoom 1 the two coincide, as in every capture.
//! INVARIANTS: `zoom` stays in [1, MAX_ZOOM]; the view never shows anything outside its
//! screen's part of the frame; `to_screen(to_frame(p)) == p`.
use shotoshot_core::{Point, Rect};

pub const MAX_ZOOM: f64 = 8.;
/// One wheel notch multiplies the zoom by this: ten notches from 1× to 8×.
const NOTCH: f64 = 1.25;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View {
    pub screen: Rect,
    pub zoom: f64,
    /// Frame point shown at `screen.min`.
    pub origin: Point,
}
impl View {
    pub fn identity(screen: Rect) -> Self {
        Self {
            screen,
            zoom: 1.,
            origin: screen.min,
        }
    }
    pub fn zoomed(&self) -> bool {
        self.zoom != 1.
    }
    pub fn to_frame(&self, p: Point) -> Point {
        self.origin + (p - self.screen.min) * (1. / self.zoom)
    }
    pub fn to_screen(&self, p: Point) -> Point {
        self.screen.min + (p - self.origin) * self.zoom
    }
    /// Zooms by `notches` (negative zooms out) keeping the frame point under the screen
    /// point `at` where it is; the view then settles inside its screen.
    pub fn zoom_at(&mut self, at: Point, notches: i32) {
        let anchor = self.to_frame(at);
        let zoom = (self.zoom * NOTCH.powi(notches)).clamp(1., MAX_ZOOM);
        // Ten notches down from 8× land on 1× exactly, whatever the rounding.
        self.zoom = if zoom < 1. + 1e-9 { 1. } else { zoom };
        self.origin = anchor - (at - self.screen.min) * (1. / self.zoom);
        self.settle();
    }
    /// Slides the frame with the hand: a screen-space `delta` moves the view the other way.
    pub fn pan(&mut self, delta: Point) {
        self.origin = self.origin - delta * (1. / self.zoom);
        self.settle();
    }
    pub fn reset(&mut self) {
        *self = Self::identity(self.screen);
    }
    /// The frame point `f` brought back inside what the view shows.
    pub fn clamp_visible(&self, f: Point) -> Point {
        let span = self.screen.size() * (1. / self.zoom);
        Point {
            x: f.x.clamp(self.origin.x, self.origin.x + span.x),
            y: f.y.clamp(self.origin.y, self.origin.y + span.y),
        }
    }
    fn settle(&mut self) {
        let span = self.screen.size() * (1. / self.zoom);
        self.origin = Point {
            x: self
                .origin
                .x
                .clamp(self.screen.min.x, self.screen.max.x - span.x),
            y: self
                .origin
                .y
                .clamp(self.screen.min.y, self.screen.max.y - span.y),
        };
    }
}

impl crate::Capture {
    /// Lets each screen of this capture zoom on its own: one view per monitor, or one for the
    /// whole frame when the platform listed none.
    pub fn enable_views(&mut self) {
        let whole = Rect {
            min: Point::default(),
            max: Point {
                x: self.image.width as f64,
                y: self.image.height as f64,
            },
        };
        self.views = if self.monitors.is_empty() {
            vec![View::identity(whole)]
        } else {
            self.monitors
                .iter()
                .map(|m| View::identity(m.bounds))
                .collect()
        };
    }
    pub fn zoomed(&self) -> bool {
        self.views.iter().any(View::zoomed)
    }
    /// The view of the screen under screen point `p`: the one containing it, else the nearest
    /// (a gap between screens), none without views.
    pub(crate) fn view_index(&self, p: Point) -> Option<usize> {
        if self.views.is_empty() {
            return None;
        }
        let screen = self.screen_at(p);
        self.views
            .iter()
            .position(|v| v.screen == screen)
            .or(Some(0))
    }
    /// The frame point shown at screen point `p`.
    pub fn to_frame(&self, p: Point) -> Point {
        self.view_index(p).map_or(p, |i| self.views[i].to_frame(p))
    }
    /// `to_frame` for a drag: through the zoomed view the press began in, whatever screen
    /// the pointer is over now, and never past what that view shows. At 1:1 a drag may
    /// cross screens, as a note across two of them does.
    pub(crate) fn drag_frame(&self, p: Point) -> Point {
        match self.gesture_view.map(|i| &self.views[i]) {
            Some(view) if view.zoomed() => view.clamp_visible(view.to_frame(p)),
            _ => self.to_frame(p),
        }
    }
    pub(crate) fn zoom_at_pointer(&self) -> f64 {
        self.view_index(self.pointer)
            .map_or(1., |i| self.views[i].zoom)
    }
    /// Handle sizes and pick reaches in frame pixels: the interface scale over the zoom, so
    /// they keep their size on screen.
    pub fn pick_scale(&self) -> f64 {
        self.ui_scale() / self.zoom_at_pointer()
    }
    /// Ctrl+wheel: zooms the pointed screen around the pointer. False without views.
    pub fn zoom_wheel(&mut self, delta: i16) -> bool {
        let Some(i) = self.view_index(self.pointer) else {
            return false;
        };
        let notches = self.wheel_travel.take(delta);
        if notches != 0 {
            self.views[i].zoom_at(self.pointer, notches);
        }
        true
    }
    /// Ctrl+0: the pointed screen back at 1:1. False without views.
    pub fn reset_view(&mut self) -> bool {
        let Some(i) = self.view_index(self.pointer) else {
            return false;
        };
        self.views[i].reset();
        true
    }
    /// Starts dragging the view of the screen under `p`. False without views, and while a
    /// shape is being drawn: the hand finishes one thing at a time.
    pub fn begin_pan(&mut self, p: Point) -> bool {
        if self.editor.is_drawing() {
            return false;
        }
        let Some(i) = self.view_index(p) else {
            return false;
        };
        self.pan = Some((i, p));
        true
    }
    pub fn pan_to(&mut self, p: Point) {
        if let Some((i, last)) = self.pan {
            self.views[i].pan(p - last);
            self.pan = Some((i, p));
        }
    }
    pub fn end_pan(&mut self) {
        self.pan = None;
    }
    pub fn panning(&self) -> bool {
        self.pan.is_some()
    }
    /// Space held or released: a left drag pans while it is held. The platform lets go when
    /// the window loses the pointer or the foreground, where the release never arrives.
    pub fn hold_space(&mut self, held: bool) {
        self.space_held = held && !self.views.is_empty();
    }
    pub fn space_held(&self) -> bool {
        self.space_held
    }
    /// The frame point the eyedropper loupe magnifies.
    pub fn loupe_center(&self) -> Point {
        self.to_frame(self.pointer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn screen() -> Rect {
        Rect::new(Point { x: 100., y: 0. }, Point { x: 500., y: 300. }).unwrap()
    }
    fn p(x: f64, y: f64) -> Point {
        Point { x, y }
    }

    #[test]
    fn at_one_the_view_is_the_identity() {
        let v = View::identity(screen());
        assert!(!v.zoomed());
        assert_eq!(v.to_frame(p(250., 40.)), p(250., 40.));
        assert_eq!(v.to_screen(p(250., 40.)), p(250., 40.));
    }

    #[test]
    fn zooming_keeps_the_frame_point_under_the_pointer_still() {
        let mut v = View::identity(screen());
        v.zoom_at(p(300., 150.), 3);
        assert!((v.zoom - 1.953125).abs() < 1e-9);
        assert_eq!(v.to_frame(p(300., 150.)), p(300., 150.));
        // A point 100 screen px right of the pointer is now 100 / zoom frame px away.
        let right = v.to_frame(p(400., 150.));
        assert!((right.x - (300. + 100. / v.zoom)).abs() < 1e-9);
        let back = v.to_screen(v.to_frame(p(123., 45.)));
        assert!((back.x - 123.).abs() < 1e-9 && (back.y - 45.).abs() < 1e-9);
    }

    #[test]
    fn zoom_stays_between_one_and_the_ceiling_and_returns_to_one_exactly() {
        let mut v = View::identity(screen());
        v.zoom_at(p(300., 150.), -5);
        assert_eq!(v.zoom, 1., "never below 1");
        assert_eq!(v.origin, screen().min);
        v.zoom_at(p(300., 150.), 40);
        assert_eq!(v.zoom, MAX_ZOOM);
        v.zoom_at(p(300., 150.), -10);
        assert_eq!(v.zoom, 1., "ten notches down from 8× is exactly 1×");
        assert!(!v.zoomed());
    }

    #[test]
    fn zooming_out_near_an_edge_slides_the_view_back_inside_the_screen() {
        let mut v = View::identity(screen());
        v.zoom_at(p(490., 290.), 6);
        // Zooming out around the top-left of a view that shows the bottom-right corner would
        // show past the screen: the view slides back.
        v.zoom_at(p(110., 10.), -3);
        let shown = Rect::new(v.origin, v.to_frame(screen().max)).unwrap();
        assert!(screen().contains_rect(shown), "{shown:?}");
        assert!((shown.max.x - 500.).abs() < 1e-9 && (shown.max.y - 300.).abs() < 1e-9);
    }

    #[test]
    fn panning_never_shows_anything_outside_the_screen() {
        let mut v = View::identity(screen());
        v.zoom_at(p(300., 150.), 3);
        v.pan(p(10_000., 10_000.));
        assert_eq!(v.origin, screen().min, "slid all the way to the top-left");
        v.pan(p(-10_000., -10_000.));
        let shown = Rect::new(v.origin, v.to_frame(screen().max)).unwrap();
        assert!((shown.max.x - 500.).abs() < 1e-9 && (shown.max.y - 300.).abs() < 1e-9);
        let mut still = View::identity(screen());
        still.pan(p(50., 50.));
        assert_eq!(still, View::identity(screen()), "nothing moves at 1×");
    }

    #[test]
    fn reset_puts_the_screen_back_at_one() {
        let mut v = View::identity(screen());
        v.zoom_at(p(300., 150.), 4);
        v.pan(p(-30., -20.));
        v.reset();
        assert_eq!(v, View::identity(screen()));
    }
}

#[cfg(test)]
mod capture_tests {
    use super::*;
    use crate::{Capture, Image, Monitor, keys::KeyOutcome, test_host::FakeHost, toolbar::Action};
    use shotoshot_core::{Modifiers, Tool};

    fn p(x: f64, y: f64) -> Point {
        Point { x, y }
    }
    /// Two 400 × 300 screens side by side, the whole frame the region, views on.
    fn board() -> (Capture, FakeHost) {
        let mut c = Capture::new(Image::solid(800, 300, [0, 0, 0, 255]));
        c.region = Rect::new(Point::default(), p(800., 300.));
        c.selecting = false;
        c.monitors = vec![
            Monitor::from(Rect::new(Point::default(), p(400., 300.)).unwrap()),
            Monitor::from(Rect::new(p(400., 0.), p(800., 300.)).unwrap()),
        ];
        c.board = true;
        c.enable_views();
        (c, FakeHost::default())
    }
    fn ctrl() -> Modifiers {
        Modifiers {
            ctrl: true,
            ..Modifiers::default()
        }
    }

    #[test]
    fn a_capture_has_no_view_and_ignores_the_zoom_gestures() {
        let mut c = Capture::new(Image::solid(800, 300, [0, 0, 0, 255]));
        c.pointer = p(100., 100.);
        assert!(!c.zoom_wheel(120));
        assert!(!c.reset_view());
        assert!(!c.begin_pan(p(100., 100.)));
        assert!(!c.zoomed());
        assert_eq!(c.to_frame(p(100., 100.)), p(100., 100.));
    }

    #[test]
    fn ctrl_wheel_zooms_the_screen_under_the_pointer_only() {
        let (mut c, _) = board();
        c.pointer = p(500., 150.);
        assert!(c.zoom_wheel(120));
        assert_eq!(c.views[0].zoom, 1., "the left screen stays");
        assert_eq!(c.views[1].zoom, 1.25);
        assert!(c.zoomed());
        assert_eq!(
            c.to_frame(p(500., 150.)),
            p(500., 150.),
            "the pointed frame point holds"
        );
        assert_eq!(
            c.to_frame(p(100., 100.)),
            p(100., 100.),
            "the left screen is 1:1"
        );
    }

    #[test]
    fn a_zoomed_screen_maps_the_pointer_to_the_frame() {
        let (mut c, mut host) = board();
        c.pointer = p(0., 0.);
        for _ in 0..3 {
            c.zoom_wheel(120);
        }
        let zoom = c.views[0].zoom;
        c.action(&mut host, Action::Tool(Tool::Rectangle));
        c.pointer_down(&mut host, p(100., 100.), Modifiers::default())
            .unwrap();
        c.pointer_up(&mut host, p(200., 150.), Modifiers::default());
        let drawn = c.editor.objects()[0].bounds();
        let near = |a: f64, b: f64| (a - b / zoom).abs() < 1e-9;
        assert!(
            near(drawn.min.x, 100.) && near(drawn.max.x, 200.),
            "{drawn:?}"
        );
        assert!(
            near(drawn.min.y, 100.) && near(drawn.max.y, 150.),
            "{drawn:?}"
        );
        // A click on the drawn edge, in screen coordinates, grabs it with the selection tool.
        c.action(&mut host, Action::Tool(Tool::Select));
        c.pointer_down(&mut host, p(100., 125.), Modifiers::default())
            .unwrap();
        c.pointer_up(&mut host, p(100., 125.), Modifiers::default());
        assert_eq!(c.editor.selected.len(), 1);
    }

    #[test]
    fn toolbar_buttons_answer_in_screen_coordinates_while_the_editor_works_in_frame_ones() {
        let (mut c, mut host) = board();
        c.pointer = p(390., 290.);
        for _ in 0..5 {
            c.zoom_wheel(120);
        }
        let button = c
            .buttons()
            .iter()
            .find(|b| b.action == Action::Tool(Tool::Select))
            .unwrap()
            .bounds
            .center();
        assert!(
            c.views[0].screen.contains(button),
            "the toolbar sits on the pointed screen"
        );
        c.action(&mut host, Action::Tool(Tool::Line));
        let action = c
            .pointer_down(&mut host, button, Modifiers::default())
            .unwrap();
        assert_eq!(action, Some(Action::Tool(Tool::Select)));
        assert!(
            c.editor.objects().is_empty(),
            "a button press draws nothing"
        );
        c.pointer_up(&mut host, button, Modifiers::default());
    }

    #[test]
    fn middle_button_and_space_drag_the_view_and_draw_nothing() {
        let (mut c, mut host) = board();
        c.pointer = p(200., 150.);
        for _ in 0..4 {
            c.zoom_wheel(120);
        }
        let origin = c.views[0].origin;
        assert!(c.begin_pan(p(200., 150.)));
        assert!(c.panning());
        c.pointer_move(&mut host, p(150., 120.), Modifiers::default());
        assert!(
            c.views[0].origin.x > origin.x && c.views[0].origin.y > origin.y,
            "the frame slid with the hand"
        );
        c.end_pan();
        assert!(!c.panning());
        // Space held: a left drag pans instead of drawing.
        c.action(&mut host, Action::Tool(Tool::Rectangle));
        assert_eq!(
            c.key(&mut host, crate::keys::KEY_SPACE, Modifiers::default()),
            KeyOutcome::Handled
        );
        let before = c.views[0].origin;
        c.pointer_down(&mut host, p(150., 120.), Modifiers::default())
            .unwrap();
        c.pointer_move(&mut host, p(160., 130.), Modifiers::default());
        c.pointer_up(&mut host, p(160., 130.), Modifiers::default());
        assert_ne!(c.views[0].origin, before);
        assert!(c.editor.objects().is_empty());
        c.key_up(crate::keys::KEY_SPACE);
        c.pointer_down(&mut host, p(150., 120.), Modifiers::default())
            .unwrap();
        c.pointer_up(&mut host, p(250., 200.), Modifiers::default());
        assert_eq!(c.editor.objects().len(), 1, "Space released: drawing again");
    }

    #[test]
    fn ctrl_zero_resets_the_pointed_screen_and_pick_reach_follows_the_zoom() {
        let (mut c, mut host) = board();
        c.pointer = p(100., 100.);
        for _ in 0..4 {
            c.zoom_wheel(120);
        }
        let zoom = c.views[0].zoom;
        assert!((c.pick_scale() - 1. / zoom).abs() < 1e-9);
        c.pointer_move(&mut host, p(100., 100.), Modifiers::default());
        assert!(
            (c.editor.ui_scale - 1. / zoom).abs() < 1e-9,
            "the editor's reach shrinks with the zoom"
        );
        assert_eq!(c.key(&mut host, 48, ctrl()), KeyOutcome::Handled);
        assert_eq!(c.views[0], View::identity(c.views[0].screen));
        assert_eq!(
            c.key(&mut host, 96, ctrl()),
            KeyOutcome::Handled,
            "the keypad zero too"
        );
        c.pointer_move(&mut host, p(100., 100.), Modifiers::default());
        assert_eq!(c.editor.ui_scale, 1.);
    }

    #[test]
    fn a_drag_across_the_seam_stays_in_the_view_where_it_began() {
        let (mut c, mut host) = board();
        // The left screen at 4x shows frame x in [0, 100]; the right screen is 1:1. The
        // toolbar would sit where the drag begins on such a small screen: out of the way.
        c.views[0].zoom = 4.;
        c.toolbar_visible = false;
        c.editor.tool = Tool::Rectangle;
        c.editor.begin(p(300., 50.), Modifiers::default());
        c.editor.update(p(350., 80.));
        c.editor.finish();
        let hidden = c.editor.objects()[0].id;
        c.action(&mut host, Action::Tool(Tool::Rectangle));
        c.pointer_down(&mut host, p(380., 100.), Modifiers::default())
            .unwrap();
        c.pointer_move(&mut host, p(420., 120.), Modifiers::default());
        c.pointer_up(&mut host, p(420., 120.), Modifiers::default());
        let drawn = c.editor.objects()[1].bounds();
        assert!(
            (drawn.min.x - 95.).abs() < 1e-9 && (drawn.min.y - 25.).abs() < 1e-9,
            "{drawn:?}"
        );
        assert!(
            drawn.max.x <= 100. + 1e-9 && drawn.max.y <= 30. + 1e-9,
            "the drag stops at the edge of what the view shows: {drawn:?}"
        );
        c.action(&mut host, Action::Tool(Tool::Eraser));
        c.pointer_down(&mut host, p(380., 60.), Modifiers::default())
            .unwrap();
        c.pointer_move(&mut host, p(420., 60.), Modifiers::default());
        c.pointer_up(&mut host, p(420., 60.), Modifiers::default());
        assert!(
            c.editor.objects().iter().any(|o| o.id == hidden),
            "a sweep never reaches what the view does not show"
        );
    }

    #[test]
    fn the_middle_button_is_refused_while_a_shape_is_being_drawn() {
        let (mut c, mut host) = board();
        c.views[0].zoom = 2.;
        c.action(&mut host, Action::Tool(Tool::Rectangle));
        c.pointer_down(&mut host, p(100., 100.), Modifiers::default())
            .unwrap();
        assert!(!c.begin_pan(p(120., 120.)));
        c.pointer_up(&mut host, p(200., 150.), Modifiers::default());
        assert_eq!(c.editor.objects().len(), 1, "the shape was finished");
    }

    #[test]
    fn the_loupe_samples_the_frame_under_the_pointer() {
        let (mut c, _) = board();
        let at = |x: usize, y: usize| (y * 800 + x) * 4;
        c.image.pixels[at(100, 100)..at(100, 100) + 4].copy_from_slice(&[0, 0, 255, 255]);
        c.pointer = p(0., 0.);
        for _ in 0..3 {
            c.zoom_wheel(120);
        }
        let zoom = c.views[0].zoom;
        c.pointer = p(100. * zoom, 100. * zoom);
        assert_eq!(c.sample_color(c.to_frame(c.pointer)), [255, 0, 0]);
        assert_eq!(c.loupe_center(), p(100., 100.));
    }
}
