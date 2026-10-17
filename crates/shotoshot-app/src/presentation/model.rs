//! ROLE: Pure gesture geometry and independent lifetimes for desktop annotations.
//! OWNER: The resident presentation controller owns each Scene and injects time.
//! INVARIANTS: Finishing releases input; cancellation preserves completed shapes.
//! HOTPATH: Gesture updates and expiration allocate nothing.

use crate::settings::PresentationSettings;
use shotoshot_core::{Object, Point, Rect, Shape, Style};

const FADE_MS: u64 = 500;
const MAX_SHAPES: usize = 128;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LiveTool {
    Arrow,
    Rectangle,
    Ellipse,
}

pub struct TimedShape {
    pub object: Object,
    pub created_at: u64,
    pub duration_ms: u32,
    pub arrow_size: f64,
}

impl TimedShape {
    pub fn opacity(&self, now: u64) -> f32 {
        let duration = u64::from(self.duration_ms);
        let fade = duration.min(FADE_MS);
        if fade == 0 {
            return 0.;
        }
        let remaining = duration.saturating_sub(now.saturating_sub(self.created_at));
        remaining.min(fade) as f32 / fade as f32
    }
}

pub struct Scene {
    pub shapes: Vec<TimedShape>,
    pub draft: Option<Object>,
    pub armed: Option<LiveTool>,
    start: Option<Point>,
    pub arrow_size: f64,
}
impl Default for Scene {
    fn default() -> Self {
        Self {
            shapes: Vec::new(),
            draft: None,
            armed: None,
            start: None,
            arrow_size: 28.,
        }
    }
}

impl Scene {
    pub fn set_style(&mut self, settings: &PresentationSettings, scale: f64) {
        if let Some(draft) = &mut self.draft {
            draft.style.color = settings.shape_color;
            draft.style.width = settings.stroke_width as f64 * scale;
            self.arrow_size = settings.arrow_size as f64 * draft.style.width
                / PresentationSettings::default().stroke_width as f64;
        }
    }

    pub fn arm(&mut self, tool: LiveTool) {
        self.cancel();
        self.armed = Some(tool);
    }

    pub fn begin(&mut self, point: Point) {
        if self.armed.is_none() {
            return;
        }
        self.draft = None;
        self.start = point.finite().then_some(point);
        self.update(point, false, false);
    }

    pub fn update(&mut self, point: Point, constrain: bool, ellipse: bool) {
        let (Some(start), Some(tool)) = (self.start, self.armed) else {
            return;
        };
        let delta = point - start;
        if !point.finite() || !delta.finite() {
            return;
        }
        let end = if constrain && tool == LiveTool::Arrow {
            shotoshot_core::snap_angle(start, point)
        } else if constrain {
            let side = delta.x.abs().max(delta.y.abs());
            start
                + Point {
                    x: side.copysign(delta.x),
                    y: side.copysign(delta.y),
                }
        } else {
            point
        };
        let Some(bounds) = Rect::new(start, end) else {
            return;
        };
        let shape = match tool {
            LiveTool::Arrow => Shape::Arrow {
                start,
                end,
                start_attach: None,
                end_attach: None,
                head: true,
            },
            LiveTool::Rectangle => Shape::Rectangle(bounds),
            LiveTool::Ellipse => {
                let radius = if ellipse {
                    Point {
                        x: delta.x.abs(),
                        y: delta.y.abs(),
                    }
                } else {
                    Point {
                        x: delta.length(),
                        y: delta.length(),
                    }
                };
                let Some(bounds) = Rect::new(start - radius, start + radius) else {
                    return;
                };
                Shape::Ellipse(bounds)
            }
        };
        if let Some(draft) = &mut self.draft {
            draft.shape = shape;
        } else {
            self.draft = Some(Object {
                // Presentation shapes cannot attach to or select another object.
                rotation: 0.,
                fill: Vec::new(),
                id: 0,
                shape,
                style: Style::default(),
            });
        }
    }

    pub fn finish(&mut self, now: u64, duration_ms: u32) -> bool {
        self.armed = None;
        self.start = None;
        let Some(object) = self.draft.take() else {
            return false;
        };
        let useful = match &object.shape {
            Shape::Arrow { start, end, .. } => (*end - *start).length() > 0.,
            Shape::Rectangle(bounds) | Shape::Ellipse(bounds) => {
                bounds.size().x > 0. && bounds.size().y > 0.
            }
            _ => false,
        };
        if !useful {
            return false;
        }
        if self.shapes.len() >= MAX_SHAPES {
            self.shapes.drain(..self.shapes.len() + 1 - MAX_SHAPES);
        }
        self.shapes.push(TimedShape {
            object,
            created_at: now,
            duration_ms,
            arrow_size: self.arrow_size,
        });
        true
    }

    pub fn cancel(&mut self) {
        self.draft = None;
        self.start = None;
        self.armed = None;
    }

    pub fn clear(&mut self) {
        self.cancel();
        self.shapes.clear();
    }

    pub fn expire(&mut self, now: u64) -> bool {
        let previous_len = self.shapes.len();
        self.shapes
            .retain(|shape| now.saturating_sub(shape.created_at) < u64::from(shape.duration_ms));
        self.shapes.len() != previous_len
    }

    pub fn intercepts_input(&self) -> bool {
        self.armed.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draw(scene: &mut Scene, tool: LiveTool, now: u64, duration_ms: u32) {
        scene.arm(tool);
        scene.begin(Point { x: 10., y: 20. });
        scene.update(Point { x: 40., y: 70. }, false, false);
        assert!(scene.finish(now, duration_ms));
    }

    fn timed_shape(created_at: u64, duration_ms: u32) -> TimedShape {
        TimedShape {
            object: Object {
                rotation: 0.,
                fill: Vec::new(),
                id: 0,
                shape: Shape::Rectangle(Rect::default()),
                style: Style::default(),
            },
            created_at,
            duration_ms,
            arrow_size: 28.,
        }
    }

    #[test]
    fn opacity_fades_only_during_the_final_half_second() {
        let shape = timed_shape(1_000, 3_000);
        for (now, opacity) in [
            (900, 1.),
            (3_500, 1.),
            (3_750, 0.5),
            (4_000, 0.),
            (9_000, 0.),
        ] {
            assert_eq!(shape.opacity(now), opacity, "at {now} ms");
        }
        let short = timed_shape(1_000, 200);
        assert_eq!(short.opacity(900), 1.);
        assert_eq!(short.opacity(1_100), 0.5);
        assert_eq!(short.opacity(1_200), 0.);
        assert_eq!(timed_shape(1_000, 0).opacity(1_000), 0.);
    }

    #[test]
    fn expiration_keeps_independent_durations_and_catches_up_after_a_delayed_timer() {
        let mut scene = Scene::default();
        draw(&mut scene, LiveTool::Arrow, 1_000, 5_000);
        draw(&mut scene, LiveTool::Rectangle, 1_200, 1_000);
        draw(&mut scene, LiveTool::Ellipse, 1_400, 3_000);
        assert!(!scene.expire(2_199));
        assert_eq!(scene.shapes.len(), 3);
        assert!(scene.expire(2_200));
        assert_eq!(scene.shapes.len(), 2);
        assert_eq!(scene.shapes[0].created_at, 1_000);
        assert_eq!(scene.shapes[0].duration_ms, 5_000);
        assert_eq!(scene.shapes[1].created_at, 1_400);
        assert_eq!(scene.shapes[1].duration_ms, 3_000);
        assert!(!scene.expire(500));
        assert!(scene.expire(6_000));
        assert!(scene.shapes.is_empty());
        assert!(!scene.expire(9_000));
    }

    #[test]
    fn finishing_any_tool_immediately_releases_input() {
        for tool in [LiveTool::Arrow, LiveTool::Rectangle, LiveTool::Ellipse] {
            let mut scene = Scene::default();
            assert!(!scene.intercepts_input());
            scene.arm(tool);
            assert!(scene.intercepts_input());
            scene.begin(Point { x: 10., y: 20. });
            scene.update(Point { x: 40., y: 70. }, false, false);
            assert!(scene.finish(100, 3_000));
            assert!(!scene.intercepts_input());
            assert!(scene.draft.is_none());
            assert_eq!(scene.shapes.len(), 1);
        }
    }

    #[test]
    fn cancellation_releases_input_and_preserves_previous_shapes() {
        let mut scene = Scene::default();
        draw(&mut scene, LiveTool::Arrow, 100, 3_000);
        scene.arm(LiveTool::Rectangle);
        scene.begin(Point { x: 10., y: 20. });
        scene.update(Point { x: 40., y: 70. }, false, false);
        scene.cancel();
        assert!(!scene.intercepts_input());
        assert!(scene.draft.is_none());
        assert_eq!(scene.shapes.len(), 1);
        assert_eq!(scene.shapes[0].created_at, 100);
        assert!(!scene.finish(200, 3_000));
    }

    #[test]
    fn empty_gestures_release_input_without_adding_shapes() {
        for tool in [LiveTool::Arrow, LiveTool::Rectangle, LiveTool::Ellipse] {
            let mut scene = Scene::default();
            scene.arm(tool);
            assert!(!scene.finish(100, 3_000));
            assert!(!scene.intercepts_input());
            scene.arm(tool);
            scene.begin(Point { x: 10., y: 20. });
            assert!(!scene.finish(100, 3_000));
            assert!(!scene.intercepts_input());
            assert!(scene.draft.is_none());
            assert!(scene.shapes.is_empty());
        }
        {
            let tool = LiveTool::Rectangle;
            let mut scene = Scene::default();
            scene.arm(tool);
            scene.begin(Point { x: 10., y: 20. });
            scene.update(Point { x: 10., y: 80. }, false, false);
            assert!(!scene.finish(100, 3_000));
            assert!(!scene.intercepts_input());
        }
    }

    #[test]
    fn shift_constrains_square_in_every_quadrant() {
        let cases = [
            (
                Point { x: 35., y: 30. },
                Point { x: 10., y: 20. },
                Point { x: 35., y: 45. },
            ),
            (
                Point { x: -15., y: 30. },
                Point { x: -15., y: 20. },
                Point { x: 10., y: 45. },
            ),
            (
                Point { x: -15., y: 10. },
                Point { x: -15., y: -5. },
                Point { x: 10., y: 20. },
            ),
            (
                Point { x: 35., y: 10. },
                Point { x: 10., y: -5. },
                Point { x: 35., y: 20. },
            ),
            (
                Point { x: 15., y: 50. },
                Point { x: 10., y: 20. },
                Point { x: 40., y: 50. },
            ),
        ];
        {
            let tool = LiveTool::Rectangle;
            for (end, min, max) in cases {
                let mut scene = Scene::default();
                scene.arm(tool);
                scene.begin(Point { x: 10., y: 20. });
                scene.update(end, true, false);
                let draft = scene.draft.as_ref().expect("gesture preview");
                let expected = Rect { min, max };
                assert_eq!(draft.bounds(), expected, "{tool:?} toward {end:?}");
                match tool {
                    LiveTool::Rectangle => assert_eq!(draft.shape, Shape::Rectangle(expected)),
                    LiveTool::Ellipse => assert_eq!(draft.shape, Shape::Ellipse(expected)),
                    LiveTool::Arrow => unreachable!(),
                }
            }
        }
    }

    #[test]
    fn circle_grows_from_center_and_alt_stretches_an_ellipse() {
        let mut scene = Scene::default();
        scene.arm(LiveTool::Ellipse);
        scene.begin(Point { x: 40., y: 70. });
        scene.update(Point { x: 40., y: 20. }, false, false);
        assert_eq!(
            scene.draft.as_ref().unwrap().bounds(),
            Rect {
                min: Point { x: -10., y: 20. },
                max: Point { x: 90., y: 120. }
            }
        );
        scene.update(Point { x: 10., y: 20. }, false, true);
        assert_eq!(
            scene.draft.as_ref().expect("ellipse preview").shape,
            Shape::Ellipse(Rect {
                min: Point { x: 10., y: 20. },
                max: Point { x: 70., y: 120. },
            })
        );
    }

    #[test]
    fn arrow_shift_snaps_diagonal_and_horizontal_then_release_restores_pointer() {
        let mut scene = Scene::default();
        scene.arm(LiveTool::Arrow);
        scene.begin(Point { x: 10., y: 20. });
        let pointer = Point { x: 110., y: 90. };
        scene.update(pointer, true, false);
        let Shape::Arrow { start, end, .. } = scene.draft.as_ref().unwrap().shape else {
            panic!("arrow");
        };
        assert!(((end - start).x - (end - start).y).abs() < 1e-8);
        scene.update(pointer, false, false);
        assert!(
            matches!(scene.draft.as_ref().unwrap().shape, Shape::Arrow { end, .. } if end == pointer)
        );
        scene.update(Point { x: 110., y: 30. }, true, false);
        let Shape::Arrow { start, end, .. } = scene.draft.as_ref().unwrap().shape else {
            panic!("arrow");
        };
        assert!((end.y - start.y).abs() < 1e-8);
        scene.update(Point { x: 110., y: 30. }, false, false);
        assert!(
            matches!(scene.draft.as_ref().unwrap().shape, Shape::Arrow { end, .. } if end == Point{x:110.,y:30.})
        );
    }

    #[test]
    fn horizontal_arrow_is_useful_and_shift_leaves_its_endpoint_unchanged() {
        let mut scene = Scene::default();
        scene.arm(LiveTool::Arrow);
        scene.begin(Point { x: 10., y: 20. });
        scene.update(Point { x: 40., y: 20. }, true, false);
        assert!(scene.finish(100, 3_000));
        assert_eq!(
            scene.shapes[0].object.shape,
            Shape::Arrow {
                start: Point { x: 10., y: 20. },
                end: Point { x: 40., y: 20. },
                start_attach: None,
                end_attach: None,
                head: true,
            }
        );
    }

    #[test]
    fn rearming_discards_the_current_gesture_but_preserves_finished_shapes() {
        let mut scene = Scene::default();
        draw(&mut scene, LiveTool::Arrow, 100, 3_000);
        scene.arm(LiveTool::Rectangle);
        scene.begin(Point { x: 10., y: 20. });
        scene.update(Point { x: 40., y: 70. }, false, false);
        scene.arm(LiveTool::Ellipse);
        scene.update(Point { x: 80., y: 90. }, false, false);
        assert_eq!(scene.armed, Some(LiveTool::Ellipse));
        assert!(scene.draft.is_none());
        assert!(!scene.finish(200, 3_000));
        assert_eq!(scene.shapes.len(), 1);
    }

    #[test]
    fn width_and_dpi_scale_the_whole_arrow_and_survive_the_next_gesture() {
        let mut scene = Scene::default();
        let mut settings = PresentationSettings::default();
        let start = Point { x: 10., y: 20. };
        let end = Point { x: 200., y: 100. };
        scene.arm(LiveTool::Arrow);
        scene.begin(start);
        scene.update(end, false, false);
        for (width, dpi, expected_width, expected_head) in [
            (5, 1., 5., 28.),
            (10, 1., 10., 56.),
            (1, 2., 2., 11.2),
            (40, 1.5, 60., 336.),
            (5, 1., 5., 28.),
        ] {
            settings.stroke_width = width;
            scene.set_style(&settings, dpi);
            let draft = scene.draft.as_ref().unwrap();
            assert_eq!(draft.style.width, expected_width);
            assert!((scene.arrow_size - expected_head).abs() < 1e-8);
            assert!(
                matches!(draft.shape, Shape::Arrow { start: a, end: b, .. } if a == start && b == end)
            );
        }
        scene.finish(100, 3000);
        scene.arm(LiveTool::Arrow);
        scene.begin(start);
        settings.stroke_width = 10;
        scene.set_style(&settings, 1.);
        assert_eq!(scene.arrow_size, 56.);
        assert_eq!(scene.shapes[0].arrow_size, 28.);
        assert_eq!(scene.shapes[0].object.style.width, 5.);
    }

    #[test]
    fn draft_style_survives_geometry_updates_and_completion() {
        let mut scene = Scene::default();
        scene.arm(LiveTool::Arrow);
        scene.begin(Point { x: 10., y: 20. });
        let draft = scene.draft.as_mut().expect("arrow preview");
        assert_eq!(draft.style, Style::default());
        draft.style.color = [1, 2, 3];
        scene.arrow_size = 40.;
        scene.update(Point { x: 40., y: 70. }, false, false);
        assert!(scene.finish(100, 3_000));
        assert_eq!(scene.shapes[0].object.style.color, [1, 2, 3]);
        scene.arrow_size = 16.;
        assert_eq!(
            scene.shapes[0].arrow_size, 40.,
            "existing arrows retain their chosen size"
        );
    }

    #[test]
    fn clear_removes_finished_shapes_and_the_current_gesture() {
        let mut scene = Scene::default();
        draw(&mut scene, LiveTool::Arrow, 100, 3_000);
        scene.arm(LiveTool::Rectangle);
        scene.begin(Point { x: 10., y: 20. });
        scene.clear();
        assert!(scene.shapes.is_empty());
        assert!(scene.draft.is_none());
        assert!(!scene.intercepts_input());
        assert!(!scene.finish(200, 3_000));
    }

    #[test]
    fn unarmed_input_never_creates_a_gesture() {
        let mut scene = Scene::default();
        scene.begin(Point { x: 10., y: 20. });
        scene.update(Point { x: 40., y: 70. }, false, false);
        assert!(scene.draft.is_none());
        assert!(!scene.finish(100, 3_000));
        assert!(scene.shapes.is_empty());
    }

    #[test]
    fn shape_limit_discards_the_oldest_completed_annotations() {
        let mut scene = Scene::default();
        for now in 0..130 {
            draw(&mut scene, LiveTool::Arrow, now, 3_000);
        }
        assert_eq!(scene.shapes.len(), 128);
        assert_eq!(scene.shapes[0].created_at, 2);
        assert_eq!(scene.shapes[127].created_at, 129);
    }
}
