//! ROLE: Portable state of one capture session: frame, selected region, editor and
//! interface flags. Platform sessions wrap it with their native widgets and renderer.
//! OWNER: One capture; dropped when it ends.
use crate::{
    Image,
    toolbar::{self, Action, Button, HoverHint},
};
use shotoshot_core::{Editor, Object, Point, Rect, Tool, arrow_points};

const DIMENSIONS_BADGE_WIDTH: f64 = 108.;

/// Pointer shape requested by the capture; each platform maps it to a native cursor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CursorKind {
    Arrow,
    Cross,
    IBeam,
    Hand,
    Move,
    ResizeNwse,
    ResizeNesw,
    ResizeNs,
    ResizeWe,
}

pub struct Capture {
    pub hover_hint: HoverHint,
    pub custom_color: [u8; 3],
    pub custom_color_changed: bool,
    pub copied_objects: Vec<Object>,
    pub copied_sequence: u32,
    pub paste_count: u32,
    pub text_size: f32,
    pub live: bool,
    pub searching: bool,
    pub image: Image,
    pub editor: Editor,
    pub region: Option<Rect>,
    pub selecting: bool,
    pub region_origin: Option<Point>,
    pub region_before: Option<Rect>,
    pub pointer: Point,
    pub eyedropper: bool,
    pub width_preview: bool,
    pub(crate) wheel_travel: crate::wheel::Notches,
    pub region_resize: Option<usize>,
    pub toolbar_visible: bool,
    pub picker: Option<crate::color_picker::ColorPicker>,
    /// Monitors in frame coordinates; empty when the frame is a single screen.
    pub monitors: Vec<Rect>,
    /// Selection replaced by the full-screen button, and the screen it applied.
    pub screen_toggle: Option<(Rect, Rect)>,
}
impl Capture {
    pub fn new(image: Image) -> Self {
        let editor = Editor::default();
        Self {
            hover_hint: HoverHint::default(),
            custom_color: editor.style.color,
            custom_color_changed: false,
            copied_objects: Vec::new(),
            copied_sequence: 0,
            paste_count: 0,
            text_size: 24.,
            live: false,
            searching: false,
            image,
            editor,
            region: None,
            selecting: true,
            region_origin: None,
            region_before: None,
            pointer: Point { x: -100., y: -100. },
            eyedropper: false,
            width_preview: false,
            wheel_travel: Default::default(),
            region_resize: None,
            toolbar_visible: true,
            picker: None,
            monitors: Vec::new(),
            screen_toggle: None,
        }
    }
    pub fn buttons(&self) -> Vec<Button> {
        if !self.toolbar_visible || self.selecting {
            return Vec::new();
        }
        let mut buttons = self
            .region
            .map(|r| toolbar::buttons_in(r, self.toolbar_screen(r)))
            .unwrap_or_default();
        for b in &mut buttons {
            match b.action {
                Action::Undo => {
                    b.enabled = self.editor.can_undo();
                    b.label = if b.enabled {
                        "Undo · Ctrl+Z"
                    } else {
                        "Nothing to undo · Ctrl+Z"
                    };
                }
                Action::Redo => {
                    b.enabled = self.editor.can_redo();
                    b.label = if b.enabled {
                        "Redo · Ctrl+Y / Ctrl+Shift+Z"
                    } else {
                        "Nothing to redo · Ctrl+Y / Ctrl+Shift+Z"
                    };
                }
                _ => {}
            }
            if matches!(b.action, Action::GoogleSearch) && self.searching {
                b.active = true;
                b.label = "Google search in progress · Ctrl+G";
            }
            if matches!(b.action, Action::Live) {
                b.active = self.live;
                b.label = if self.live {
                    "Live capture on · F2 to freeze"
                } else {
                    "Capture frozen · F2 for live capture"
                };
            }
        }
        buttons
    }
    fn toolbar_screen(&self, region: Rect) -> Rect {
        self.monitors
            .iter()
            .copied()
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
        toolbar::bounds(&self.buttons())
    }
    pub fn toolbar_contains(&self, p: Point) -> bool {
        toolbar::panels(&self.buttons())
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
            .position(|h| (*h - p).length() <= 8.)
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
    pub fn rotation_handle(&self) -> Option<Point> {
        self.editor.rotation_handle().map(|p| Point {
            x: p.x.clamp(8., (self.image.width as f64 - 8.).max(8.)),
            y: p.y.clamp(8., (self.image.height as f64 - 8.).max(8.)),
        })
    }
    /// The stroke-size ring follows the pointer on the canvas only: over the
    /// toolbar the pointer targets buttons, so the ring is hidden, never displaced.
    pub fn width_ring_visible(&self) -> bool {
        !self.toolbar_contains(self.pointer)
            && (self.width_preview
                || (self.region.is_some_and(|r| r.contains(self.pointer))
                    && matches!(self.editor.tool, Tool::Pen | Tool::Highlight)))
    }
    /// Badge and full-screen button, side by side above the region's top-right corner.
    fn region_badges(&self) -> Option<(Rect, Rect)> {
        let r = self.region?;
        let (frame_width, frame_height) = (self.image.width as f64, self.image.height as f64);
        let height = 26_f64.min(frame_height);
        let button = height;
        let width = (DIMENSIONS_BADGE_WIDTH + 4. + button).min(frame_width);
        let min = Point {
            x: (r.max.x - width).clamp(0., frame_width - width),
            y: (r.min.y - height - 6.).clamp(0., frame_height - height),
        };
        let at = |x: f64, w: f64| Rect {
            min: Point { x, y: min.y },
            max: Point {
                x: x + w,
                y: min.y + height,
            },
        };
        let badge = (width - 4. - button).max(0.);
        Some((at(min.x, badge), at(min.x + width - button, button)))
    }
    /// Clickable crop-size badge above the region's top-right corner.
    pub fn dimensions_bounds(&self) -> Option<Rect> {
        self.region_badges().map(|(badge, _)| badge)
    }
    /// Button extending the region to its whole screen, next to the size badge.
    pub fn screen_button_bounds(&self) -> Option<Rect> {
        if self.selecting {
            return None;
        }
        self.region_badges().map(|(_, button)| button)
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
            label: "Edit dimensions · expressions: 800+20, 1920/2",
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
    /// Monitor containing `point`, clipped to the frame; nearest monitor in a gap.
    pub(crate) fn screen_at(&self, point: Point) -> Rect {
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
            .filter_map(|m| clip(*m, frame))
            .min_by(|a, b| {
                let contains = |m: Rect| {
                    point.x >= m.min.x
                        && point.x < m.max.x
                        && point.y >= m.min.y
                        && point.y < m.max.y
                };
                contains(*b)
                    .cmp(&contains(*a))
                    .then_with(|| distance(*a).total_cmp(&distance(*b)))
            })
            .unwrap_or(frame)
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
    /// Pointer shape for the canvas, once overlays (picker, size fields) are ruled out.
    pub fn cursor(&self, p: Point) -> CursorKind {
        if self.screen_button_bounds().is_some_and(|r| r.contains(p)) {
            return CursorKind::Hand;
        }
        if self.toolbar_contains(p) {
            return CursorKind::Arrow;
        }
        if self.eyedropper {
            return CursorKind::Cross;
        }
        if let Some(index) = self.region_handle(p) {
            return match index {
                0 | 4 => CursorKind::ResizeNwse,
                2 | 6 => CursorKind::ResizeNesw,
                1 | 5 => CursorKind::ResizeNs,
                _ => CursorKind::ResizeWe,
            };
        }
        if self.editor.tool == Tool::Text {
            return CursorKind::IBeam;
        }
        if self.editor.tool != Tool::Select {
            return CursorKind::Cross;
        }
        if self
            .rotation_handle()
            .is_some_and(|h| (h - p).length() <= 9.)
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
            && ((p - a).length() < 9. || (p - b).length() < 9.)
        {
            return CursorKind::Move;
        }
        if let Some(frame) = self.editor.selection_frame()
            && let Some(i) = frame.corners().iter().position(|c| (*c - p).length() < 9.)
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
            .any(|o| o.hit(p, 7., self.editor.objects()))
        {
            return CursorKind::Move;
        }
        CursorKind::Arrow
    }
}
/// Corners and edge midpoints, clockwise from the top-left corner.
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
    fn region_handles_drive_resize_cursors() {
        let c = capture();
        assert_eq!(c.cursor(Point { x: 100., y: 100. }), CursorKind::ResizeNwse);
        assert_eq!(c.cursor(Point { x: 600., y: 250. }), CursorKind::ResizeWe);
        assert_eq!(c.cursor(Point { x: 350., y: 400. }), CursorKind::ResizeNs);
        assert_eq!(c.cursor(Point { x: 300., y: 250. }), CursorKind::Cross);
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
            })
            .collect();
        c
    }
    #[test]
    fn screen_button_fills_the_region_monitor_and_restores_on_second_press() {
        let mut c = three_screens();
        let small = Rect::new(Point { x: 2000., y: 300. }, Point { x: 2400., y: 500. });
        c.region = small;
        c.toggle_screen();
        assert_eq!(c.region, c.monitors.get(1).copied());
        assert!(c.screen_filled());
        c.toggle_screen();
        assert_eq!(c.region, small);
        assert!(!c.screen_filled());
    }
    #[test]
    fn screen_button_picks_its_own_monitor_even_when_most_of_region_is_elsewhere() {
        let mut c = three_screens();
        c.region = Rect::new(Point { x: 200., y: 100. }, Point { x: 2000., y: 900. });
        assert!(c.monitors[1].contains(c.screen_button_bounds().unwrap().center()));
        c.toggle_screen();
        assert_eq!(c.region, c.monitors.get(1).copied());
    }
    #[test]
    fn toolbar_stays_on_a_single_selected_monitor_but_not_for_a_cross_monitor_region() {
        let mut c = three_screens();
        c.region = c.monitors.get(1).copied();
        assert!(
            toolbar::panels(&c.buttons())
                .iter()
                .all(|panel| c.monitors[1].contains_rect(*panel))
        );

        c.region = Rect::new(Point { x: 1800., y: 300. }, Point { x: 2200., y: 700. });
        assert_eq!(
            toolbar::panels(&c.buttons()),
            toolbar::panels(&toolbar::buttons(
                c.region.unwrap(),
                c.image.width,
                c.image.height
            ))
        );
    }
    #[test]
    fn monitor_selection_follows_points_including_shared_edges_and_frame_clipping() {
        let mut c = three_screens();
        for (index, x) in [0., 1920., 3840.].into_iter().enumerate() {
            assert_eq!(c.screen_at(Point { x, y: 100. }), c.monitors[index]);
            c.region = Rect::new(
                Point { x: 10., y: 100. },
                Point {
                    x: x + 500.,
                    y: 700.,
                },
            );
            c.toggle_screen();
            assert_eq!(c.region, Some(c.monitors[index]));
        }
        c.monitors = vec![Rect {
            min: Point { x: -100., y: -100. },
            max: Point { x: 500., y: 500. },
        }];
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
    fn dimensions_badge_is_compact_for_two_five_digit_values() {
        let c = capture();
        let badge = c.dimensions_bounds().unwrap();
        let button = c.screen_button_bounds().unwrap();
        assert_eq!(badge.size().x, 108.);
        assert_eq!(button.min.x - badge.max.x, 4.);
        assert_eq!(button.max.x, 600.);
    }
    #[test]
    fn screen_button_expands_again_after_the_region_was_edited() {
        let mut c = three_screens();
        c.toggle_screen();
        c.region = Rect::new(Point { x: 10., y: 10. }, Point { x: 50., y: 50. });
        assert!(!c.screen_filled());
        c.toggle_screen();
        assert_eq!(c.region, c.monitors.first().copied());
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
    fn sampled_color_is_rgb_from_bgra_and_clamped() {
        let mut c = capture();
        c.image.pixels[..4].copy_from_slice(&[10, 20, 30, 255]);
        assert_eq!(c.sample_color(Point { x: -5., y: -5. }), [30, 20, 10]);
    }
}
