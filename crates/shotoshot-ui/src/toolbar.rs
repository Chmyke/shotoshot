use crate::{Canvas, Ink};
use shotoshot_core::{Point, Rect, Tool};
mod icons;
pub use icons::glyph;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Action {
    Tool(Tool),
    Color([u8; 3]),
    CopyColor([u8; 3]),
    CustomColor,
    Eyedropper,
    Fill,
    Thinner,
    Thicker,
    Undo,
    Redo,
    GoogleSearch,
    Live,
    FullScreen,
    Dimensions,
    Copy,
    Save,
    /// Ctrl+Shift+S: always asks where to save, even for a reopened document. No button.
    SaveAs,
    /// Board sessions only: every note and the background gone, one undo step. No key.
    ClearBoard,
    Close,
}
#[derive(Clone)]
pub struct Button {
    pub bounds: Rect,
    pub label: &'static str,
    pub action: Action,
    pub panel: u8,
    pub active: bool,
    pub enabled: bool,
}
pub const HINT_DELAY_MS: u64 = 250;
const HINT_FADE_MS: u64 = 120;
#[derive(Default)]
pub struct HoverHint {
    target: Option<(Action, Rect)>,
    since: Option<std::time::Instant>,
    fading: Option<(Action, Rect, std::time::Instant, f32)>,
}
impl HoverHint {
    pub fn update(&mut self, target: Option<(Action, Rect)>, now: std::time::Instant) -> bool {
        if self.target == target {
            return false;
        }
        if self.target.is_some()
            && let Some((action, bounds, alpha)) = self.visible(now)
        {
            self.fading = Some((action, bounds, now, alpha));
        }
        self.target = target;
        self.since = target.map(|_| now);
        true
    }
    pub fn visible(&self, now: std::time::Instant) -> Option<(Action, Rect, f32)> {
        if self
            .since
            .is_some_and(|since| now.duration_since(since).as_millis() >= HINT_DELAY_MS as u128)
        {
            return self.target.map(|(action, bounds)| (action, bounds, 1.));
        }
        self.fading.and_then(|(action, bounds, since, alpha)| {
            let remaining =
                1. - now.duration_since(since).as_secs_f32() / (HINT_FADE_MS as f32 / 1000.);
            (remaining > 0.).then_some((action, bounds, alpha * remaining))
        })
    }
    pub fn next_tick(&self, now: std::time::Instant) -> Option<u32> {
        if self.fading.is_some_and(|(_, _, since, _)| {
            now.duration_since(since).as_millis() < HINT_FADE_MS as u128
        }) {
            return Some(16);
        }
        self.since.and_then(|since| {
            let elapsed = now.duration_since(since).as_millis();
            (elapsed < HINT_DELAY_MS as u128).then(|| (HINT_DELAY_MS as u128 - elapsed) as u32)
        })
    }
}
pub const COLORS: [[u8; 3]; 6] = [
    [237, 79, 92],
    [250, 192, 66],
    [64, 199, 128],
    [80, 153, 244],
    [245, 245, 248],
    [28, 31, 40],
];
// One source for toolbar hints and keyboard dispatch. Native text fields handle their own keys.
const SHORTCUTS: [(&str, Action, u32, bool, bool); 25] = [
    (
        "Select · V · Page Up/Down forward or back",
        Action::Tool(Tool::Select),
        86,
        false,
        false,
    ),
    (
        "Rectangle · R",
        Action::Tool(Tool::Rectangle),
        82,
        false,
        false,
    ),
    ("Ellipse · C", Action::Tool(Tool::Ellipse), 67, false, false),
    ("Line · L", Action::Tool(Tool::Line), 76, false, false),
    ("Arrow · A", Action::Tool(Tool::Arrow), 65, false, false),
    ("Pen · P", Action::Tool(Tool::Pen), 80, false, false),
    (
        "Highlighter · H",
        Action::Tool(Tool::Highlight),
        72,
        false,
        false,
    ),
    ("Text · T", Action::Tool(Tool::Text), 84, false, false),
    ("Eraser · E", Action::Tool(Tool::Eraser), 69, false, false),
    (
        "Live capture · F2 · refresh on copy or save",
        Action::Live,
        113,
        false,
        false,
    ),
    ("Red · 1", Action::Color(COLORS[0]), 49, false, false),
    ("Yellow · 2", Action::Color(COLORS[1]), 50, false, false),
    ("Green · 3", Action::Color(COLORS[2]), 51, false, false),
    ("Blue · 4", Action::Color(COLORS[3]), 52, false, false),
    ("White · 5", Action::Color(COLORS[4]), 53, false, false),
    ("Black · 6", Action::Color(COLORS[5]), 54, false, false),
    ("Custom color · S", Action::CustomColor, 83, false, false),
    ("Eyedropper · I", Action::Eyedropper, 73, false, false),
    (
        "Color a stroke, a shape or the background · G · Right-click or Alt+click clears",
        Action::Fill,
        71,
        false,
        false,
    ),
    ("Undo · Ctrl+Z", Action::Undo, 90, true, false),
    (
        "Redo · Ctrl+Y / Ctrl+Shift+Z",
        Action::Redo,
        89,
        true,
        false,
    ),
    (
        "Search with Google Lens · Ctrl+G",
        Action::GoogleSearch,
        71,
        true,
        false,
    ),
    // The hint names the shorter Ctrl+C, which copies the image whenever no annotation is
    // selected (`keys`); the key read here, Ctrl+Shift+C, copies it always.
    ("Copy image · Ctrl+C", Action::Copy, 67, true, true),
    ("Save · Ctrl+S", Action::Save, 83, true, false),
    ("Close · Esc", Action::Close, 27, false, false),
];

/// Full-screen toggle: a button beside the size badge, outside the toolbar panels.
pub const FULL_SCREEN_KEY: u32 = 70;
pub const FULL_SCREEN_LABEL: &str = "Full screen · F";
pub const RESTORE_SELECTION_LABEL: &str = "Back to previous selection · F";

pub fn shortcut_action(key: u32, ctrl: bool, shift: bool, alt: bool) -> Option<Action> {
    if alt {
        return None;
    }
    if key == FULL_SCREEN_KEY && !ctrl {
        return Some(Action::FullScreen);
    }
    if key == 90 && ctrl && shift {
        return Some(Action::Redo);
    }
    if key == 83 && ctrl && shift {
        return Some(Action::SaveAs);
    }
    SHORTCUTS
        .iter()
        .find_map(|&(_, action, expected, control, shifted)| {
            // Shift also reaches unmodified keys on layouts such as AZERTY.
            (key == expected && ctrl == control && (!ctrl || shift == shifted)).then_some(action)
        })
}

pub fn buttons(region: Rect, width: u32, height: u32) -> Vec<Button> {
    buttons_in(
        region,
        Rect {
            min: Point::default(),
            max: Point {
                x: width as f64,
                y: height as f64,
            },
        },
        1.,
        false,
    )
}

/// Buttons a board session adds to the action panel, before Close. They have no key.
const BOARD_ACTIONS: [(&str, Action); 1] = [("Clear board · Ctrl+Z restores", Action::ClearBoard)];

/// Both panels beside `region`, kept on `screen`, with the board's own buttons when `board`.
/// The layout is in 96-DPI logical pixels and scaled once here, so buttons keep their apparent
/// size on every screen.
pub(crate) fn buttons_in(region: Rect, screen: Rect, scale: f64, board: bool) -> Vec<Button> {
    let resized = |r: Rect, factor: f64| Rect {
        min: r.min * factor,
        max: r.max * factor,
    };
    let mut buttons = layout(
        resized(region, 1. / scale),
        resized(screen, 1. / scale),
        board,
    );
    for b in &mut buttons {
        b.bounds = resized(b.bounds, scale);
    }
    buttons
}

fn layout(region: Rect, screen: Rect, board: bool) -> Vec<Button> {
    let cell = 32.;
    let entries = SHORTCUTS.map(|(label, action, _, _, _)| (label, action));
    let is_tool = |a: Action| matches!(a, Action::Tool(_));
    let tools: Vec<_> = entries
        .iter()
        .copied()
        .filter(|(_, a)| is_tool(*a))
        .collect();
    let mut actions: Vec<_> = entries
        .iter()
        .copied()
        .filter(|(_, a)| !is_tool(*a))
        .collect();
    if board {
        let close = actions.len().saturating_sub(1);
        actions.splice(close..close, BOARD_ACTIONS);
    }
    let columns = ((screen.size().x - 36.) / cell)
        .floor()
        .clamp(1., actions.len() as f64) as usize;
    let action_size = Point {
        x: columns as f64 * cell + 8.,
        y: actions.len().div_ceil(columns) as f64 * cell + 8.,
    };
    let rows = ((screen.size().y - action_size.y - 40.) / cell)
        .floor()
        .clamp(1., tools.len() as f64) as usize;
    let tool_size = Point {
        x: tools.len().div_ceil(rows) as f64 * cell + 8.,
        y: rows as f64 * cell + 8.,
    };
    let clamp = |p: Point, size: Point| Point {
        x: p.x.clamp(
            screen.min.x + 4.,
            (screen.max.x - size.x - 4.).max(screen.min.x + 4.),
        ),
        y: p.y.clamp(
            screen.min.y + 4.,
            (screen.max.y - size.y - 4.).max(screen.min.y + 4.),
        ),
    };
    let mut action_min = clamp(
        Point {
            x: region.max.x - action_size.x,
            y: if region.max.y + 8. + action_size.y <= screen.max.y - 4. {
                region.max.y + 8.
            } else if region.min.y - 8. - action_size.y >= screen.min.y + 4. {
                region.min.y - 8. - action_size.y
            } else {
                region.max.y - action_size.y - 8.
            },
        },
        action_size,
    );
    let action_rect = Rect {
        min: action_min,
        max: action_min + action_size,
    };
    let mut tool_min = clamp(
        Point {
            x: if region.max.x + 8. + tool_size.x <= screen.max.x - 4. {
                region.max.x + 8.
            } else if region.min.x - 8. - tool_size.x >= screen.min.x + 4. {
                region.min.x - 8. - tool_size.x
            } else {
                region.max.x - tool_size.x - 8.
            },
            y: region.max.y - tool_size.y,
        },
        tool_size,
    );
    if overlaps(
        Rect {
            min: tool_min,
            max: tool_min + tool_size,
        },
        action_rect,
    ) {
        tool_min.y = if action_min.y - tool_size.y - 8. >= screen.min.y + 4. {
            action_min.y - tool_size.y - 8.
        } else {
            action_rect.max.y + 8.
        };
        tool_min = clamp(tool_min, tool_size);
        if overlaps(
            Rect {
                min: tool_min,
                max: tool_min + tool_size,
            },
            action_rect,
        ) {
            tool_min.x = (screen.max.x - tool_size.x - 4.).max(screen.min.x + 4.);
            if overlaps(
                Rect {
                    min: tool_min,
                    max: tool_min + tool_size,
                },
                action_rect,
            ) {
                action_min.y = (screen.max.y - action_size.y - 4.).max(screen.min.y + 4.);
                tool_min.y = (action_min.y - tool_size.y - 8.).max(screen.min.y + 4.);
            }
        }
    }
    let mut result = Vec::new();
    for (panel, list) in [tools, actions].into_iter().enumerate() {
        for (i, (label, action)) in list.into_iter().enumerate() {
            let min = if panel == 0 {
                tool_min
                    + Point {
                        x: 5. + (i / rows) as f64 * cell,
                        y: 5. + (i % rows) as f64 * cell,
                    }
            } else {
                action_min
                    + Point {
                        x: 5. + (i % columns) as f64 * cell,
                        y: 5. + (i / columns) as f64 * cell,
                    }
            };
            result.push(Button {
                bounds: Rect {
                    min,
                    max: min + Point { x: 30., y: 30. },
                },
                label,
                action,
                panel: panel as u8,
                active: false,
                enabled: true,
            });
        }
    }
    result
}

fn overlaps(a: Rect, b: Rect) -> bool {
    a.min.x < b.max.x && a.max.x > b.min.x && a.min.y < b.max.y && a.max.y > b.min.y
}
/// Panel outlines around `buttons`, laid out at interface `scale`.
pub fn panels(buttons: &[Button], scale: f64) -> Vec<Rect> {
    [0, 1]
        .into_iter()
        .filter_map(|id| {
            let mut matching = buttons.iter().filter(|b| b.panel == id);
            let first = matching.next()?;
            let all = matching.fold(first.bounds, |r, b| r.union(b.bounds));
            Some(all.expanded(5. * scale))
        })
        .collect()
}
pub fn bounds(buttons: &[Button], scale: f64) -> Option<Rect> {
    panels(buttons, scale).into_iter().reduce(|a, b| a.union(b))
}
pub fn draw<C: Canvas>(
    canvas: &mut C,
    buttons: &[Button],
    tool: Tool,
    colors: ([u8; 3], [u8; 3]),
    pointer: Point,
    eyedropper: bool,
    scale: f64,
) -> Result<(), C::Error> {
    for id in [0, 1] {
        let group: Vec<_> = buttons.iter().filter(|b| b.panel == id).cloned().collect();
        let pointer = (pointer, eyedropper);
        draw_panel(canvas, &group, tool, colors, pointer, scale)?;
    }
    Ok(())
}
pub(crate) fn draw_hint<C: Canvas>(
    canvas: &mut C,
    button: &Button,
    screen: Rect,
    opacity: f32,
    scale: f64,
) -> Result<(), C::Error> {
    let label = button.label;
    let extent = canvas.measure_text(label, 12. * scale)?;
    let hint = tooltip_bounds(
        button.bounds,
        screen,
        Point {
            x: extent.x + 18. * scale,
            y: 25. * scale,
        },
        scale,
    );
    canvas.fill_round_rect(hint, 4. * scale, Ink::with_alpha([32, 32, 32], opacity))?;
    canvas.text(
        label,
        Rect {
            min: hint.min
                + Point {
                    x: 8. * scale,
                    y: 4. * scale,
                },
            max: hint.max,
        },
        12. * scale,
        Ink::with_alpha([245, 245, 245], opacity),
    )?;
    Ok(())
}
/// A box of `size` under `button` (above it at the bottom of `screen`), with margins in
/// logical pixels at `scale`.
pub(crate) fn tooltip_bounds(button: Rect, screen: Rect, size: Point, scale: f64) -> Rect {
    let (edge, gap) = (4. * scale, 6. * scale);
    let size = Point {
        x: size.x.min((screen.size().x - 2. * edge).max(0.)),
        y: size.y.min((screen.size().y - 2. * edge).max(0.)),
    };
    let left = button.center().x.clamp(
        screen.min.x + edge,
        (screen.max.x - size.x - edge).max(screen.min.x + edge),
    );
    let top = if button.max.y + gap + size.y <= screen.max.y - edge {
        button.max.y + gap
    } else {
        button.min.y - size.y - gap
    }
    .clamp(
        screen.min.y + edge,
        (screen.max.y - size.y - edge).max(screen.min.y + edge),
    );
    let min = Point { x: left, y: top };
    Rect {
        min,
        max: min + size,
    }
}
fn draw_panel<C: Canvas>(
    canvas: &mut C,
    buttons: &[Button],
    tool: Tool,
    colors: ([u8; 3], [u8; 3]),
    (pointer, eyedropper): (Point, bool),
    scale: f64,
) -> Result<(), C::Error> {
    let Some(panel) = bounds(buttons, scale) else {
        return Ok(());
    };
    canvas.fill_round_rect(panel, 8. * scale, Ink::solid([32, 32, 32]))?;
    canvas.stroke_round_rect(panel, 8. * scale, Ink::solid([78, 78, 78]), scale)?;
    for b in buttons {
        let selected = b.active
            || matches!(b.action,Action::Tool(t)if t==tool)
            || matches!(b.action, Action::Eyedropper) && eyedropper;
        let rgb = if selected {
            [55, 70, 80]
        } else if b.enabled && b.bounds.contains(pointer) {
            [55, 55, 55]
        } else {
            [32, 32, 32]
        };
        canvas.fill_round_rect(b.bounds, 5. * scale, Ink::solid(rgb))?;
        icons::draw(canvas, b, colors, selected, scale)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn e_selects_the_eraser_and_its_button_follows_text_in_every_layout() {
        assert_eq!(
            shortcut_action(69, false, false, false),
            Some(Action::Tool(Tool::Eraser))
        );
        assert_eq!(
            shortcut_action(69, true, false, false),
            None,
            "Ctrl+E is nobody's"
        );
        let region = Rect::new(Point { x: 100., y: 100. }, Point { x: 400., y: 300. }).unwrap();
        let screen = Rect::new(Point::default(), Point { x: 1920., y: 1080. }).unwrap();
        for board in [false, true] {
            let buttons = buttons_in(region, screen, 1., board);
            let tools: Vec<Tool> = buttons
                .iter()
                .filter_map(|b| match b.action {
                    Action::Tool(t) => Some(t),
                    _ => None,
                })
                .collect();
            let text = tools.iter().position(|t| *t == Tool::Text).unwrap();
            assert_eq!(tools.get(text + 1), Some(&Tool::Eraser), "board {board}");
            let eraser = buttons
                .iter()
                .find(|b| b.action == Action::Tool(Tool::Eraser))
                .unwrap();
            assert_eq!(eraser.label, "Eraser · E");
        }
    }
    #[test]
    fn hover_delay_fade_and_timer_stop() {
        let now = std::time::Instant::now();
        let bounds = Rect {
            min: Point { x: 100., y: 100. },
            max: Point { x: 130., y: 130. },
        };
        let mut hint = HoverHint::default();
        hint.update(Some((Action::Save, bounds)), now);
        assert_eq!(hint.next_tick(now), Some(250));
        assert_eq!(
            hint.visible(now + std::time::Duration::from_millis(249)),
            None
        );
        let shown = now + std::time::Duration::from_millis(250);
        assert_eq!(hint.visible(shown), Some((Action::Save, bounds, 1.)));
        assert_eq!(hint.next_tick(shown), None);
        assert!(!hint.update(Some((Action::Save, bounds)), shown));
        hint.update(None, shown);
        let half = shown + std::time::Duration::from_millis(60);
        assert!((hint.visible(half).unwrap().2 - 0.5).abs() < 0.001);
        assert_eq!(hint.next_tick(half), Some(16));
        let end = shown + std::time::Duration::from_millis(120);
        assert_eq!(hint.visible(end), None);
        assert_eq!(hint.next_tick(end), None);
    }
    #[test]
    fn quick_crossing_never_shows_and_changing_button_restarts_delay() {
        let now = std::time::Instant::now();
        let bounds = Rect {
            min: Point::default(),
            max: Point { x: 30., y: 30. },
        };
        let mut hint = HoverHint::default();
        hint.update(Some((Action::Save, bounds)), now);
        hint.update(None, now + std::time::Duration::from_millis(100));
        assert_eq!(
            hint.visible(now + std::time::Duration::from_millis(300)),
            None
        );
        hint.update(
            Some((Action::Save, bounds)),
            now + std::time::Duration::from_millis(400),
        );
        let next = now + std::time::Duration::from_millis(700);
        hint.update(Some((Action::Close, bounds)), next);
        assert_eq!(hint.visible(next).unwrap().0, Action::Save);
        assert_eq!(
            hint.visible(next + std::time::Duration::from_millis(150)),
            None
        );
        assert_eq!(
            hint.visible(next + std::time::Duration::from_millis(250))
                .unwrap()
                .0,
            Action::Close
        );
    }
    #[test]
    fn tooltip_is_below_button_and_flips_at_screen_bottom() {
        let screen = Rect {
            min: Point::default(),
            max: Point { x: 800., y: 600. },
        };
        let size = Point { x: 170., y: 25. };
        let button = Rect {
            min: Point { x: 100., y: 100. },
            max: Point { x: 130., y: 130. },
        };
        let normal = tooltip_bounds(button, screen, size, 1.);
        assert_eq!(normal.min, Point { x: 115., y: 136. });
        let bottom = Rect {
            min: Point { x: 765., y: 565. },
            max: Point { x: 795., y: 595. },
        };
        let flipped = tooltip_bounds(bottom, screen, size, 1.);
        assert!(flipped.max.y < bottom.min.y && flipped.max.x <= 796.);
    }
    #[test]
    fn tooltip_stays_inside_each_offset_monitor_even_at_seams_and_corners() {
        let monitors = [
            Rect {
                min: Point { x: 0., y: 200. },
                max: Point { x: 1920., y: 1280. },
            },
            Rect {
                min: Point { x: 1920., y: 0. },
                max: Point { x: 3840., y: 1080. },
            },
            Rect {
                min: Point { x: 3840., y: 120. },
                max: Point { x: 5760., y: 1200. },
            },
        ];
        for monitor in monitors {
            for corner in monitor.corners() {
                let button = Rect {
                    min: corner - Point { x: 15., y: 15. },
                    max: corner + Point { x: 15., y: 15. },
                };
                for width in [170., 460., 2400.] {
                    let hint = tooltip_bounds(button, monitor, Point { x: width, y: 25. }, 1.);
                    assert!(hint.min.x >= monitor.min.x + 4. && hint.max.x <= monitor.max.x - 4.);
                    assert!(hint.min.y >= monitor.min.y + 4. && hint.max.y <= monitor.max.y - 4.);
                }
            }
        }
    }
    #[test]
    fn toolbar_shortcuts_dispatch_uniquely_and_preserve_contextual_copy() {
        for (index, &(_, action, key, ctrl, shift)) in SHORTCUTS.iter().enumerate() {
            assert_eq!(shortcut_action(key, ctrl, shift, false), Some(action));
            assert!(
                SHORTCUTS[..index]
                    .iter()
                    .all(|&(_, _, other, c, s)| (key, ctrl, shift) != (other, c, s))
            );
            assert_eq!(shortcut_action(key, ctrl, shift, true), None);
        }
        assert_eq!(
            shortcut_action(49, false, true, false),
            Some(Action::Color(COLORS[0]))
        );
        assert_eq!(
            shortcut_action(86, false, true, false),
            Some(Action::Tool(Tool::Select))
        );
        assert_eq!(shortcut_action(67, true, false, false), None);
        assert_eq!(shortcut_action(86, true, false, false), None);
        assert_eq!(shortcut_action(90, true, true, false), Some(Action::Redo));
        assert!(SHORTCUTS.iter().all(|s| s.2 != FULL_SCREEN_KEY));
        assert_eq!(
            shortcut_action(FULL_SCREEN_KEY, false, false, false),
            Some(Action::FullScreen)
        );
        assert_eq!(shortcut_action(67, true, true, false), Some(Action::Copy));
    }
    #[test]
    fn circle_is_c_and_the_color_wheel_is_s() {
        let plain = |key| shortcut_action(key, false, false, false);
        assert_eq!(plain(67), Some(Action::Tool(Tool::Ellipse)));
        assert_eq!(plain(83), Some(Action::CustomColor));
        assert_eq!(plain(79), None, "O no longer selects a tool");
        assert_eq!(shortcut_action(83, true, false, false), Some(Action::Save));
        assert_eq!(shortcut_action(83, true, true, false), Some(Action::SaveAs));
    }
    #[test]
    fn fill_is_g_beside_the_eyedropper_and_ctrl_g_still_searches() {
        assert_eq!(shortcut_action(71, false, false, false), Some(Action::Fill));
        assert_eq!(
            shortcut_action(71, true, false, false),
            Some(Action::GoogleSearch)
        );
        let region = Rect::new(Point { x: 100., y: 100. }, Point { x: 600., y: 500. }).unwrap();
        let all = buttons(region, 1920, 1080);
        let row: Vec<_> = all.iter().filter(|b| b.panel == 1).collect();
        let eyedropper = row
            .iter()
            .position(|b| b.action == Action::Eyedropper)
            .unwrap();
        let fill = row[eyedropper + 1];
        assert_eq!(fill.action, Action::Fill);
        assert_eq!(fill.bounds.min.y, row[eyedropper].bounds.min.y);
        assert!(fill.label.contains("Right-click or Alt+click"));
        assert!(
            all.iter().all(|b| b.panel == 1 || b.action != Action::Fill),
            "not among the drawing tools"
        );
    }
    #[test]
    fn live_shape_shortcuts_are_alt_plus_the_editor_letter() {
        const ALT: u32 = 1;
        let live = shotoshot_core::settings::PresentationSettings::default().shape_shortcuts;
        // `shape_shortcuts` order: arrow, rectangle, ellipse.
        for (shortcut, tool) in live
            .iter()
            .zip([Tool::Arrow, Tool::Rectangle, Tool::Ellipse])
        {
            assert_eq!(shortcut.modifiers, ALT);
            assert_eq!(
                shortcut_action(shortcut.key, false, false, false),
                Some(Action::Tool(tool))
            );
        }
    }
    #[test]
    fn corner_panels_stay_separate_and_reachable() {
        for (w, h) in [(640, 480), (1000, 700), (1920, 1080)] {
            for r in [
                Rect::new(Point { x: 100., y: 100. }, Point { x: 500., y: 400. }).unwrap(),
                Rect::new(
                    Point::default(),
                    Point {
                        x: w as f64,
                        y: h as f64,
                    },
                )
                .unwrap(),
                Rect::new(Point::default(), Point { x: 100., y: 100. }).unwrap(),
                Rect::new(
                    Point {
                        x: w as f64 - 30.,
                        y: h as f64 - 30.,
                    },
                    Point {
                        x: w as f64,
                        y: h as f64,
                    },
                )
                .unwrap(),
            ] {
                let buttons = buttons(r, w, h);
                assert_eq!(buttons.len(), 25);
                let panels = panels(&buttons, 1.);
                let screen = Rect {
                    min: Point::default(),
                    max: Point {
                        x: w as f64,
                        y: h as f64,
                    },
                };
                assert!(
                    panels.iter().all(|p| screen.contains_rect(*p)),
                    "{w}x{h} {r:?} {panels:?}"
                );
                assert!(!overlaps(panels[0], panels[1]), "panels overlap {r:?}");
            }
        }
        let r = Rect::new(Point { x: 100., y: 100. }, Point { x: 600., y: 500. }).unwrap();
        let panels = panels(&buttons(r, 1000, 800), 1.);
        assert!(panels[0].min.x > r.max.x);
        assert!(panels[1].min.y > r.max.y);
        assert_eq!(panels[0].max.y, r.max.y);
        assert_eq!(panels[1].max.x, r.max.x);
    }
}
