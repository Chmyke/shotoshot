use crate::{Canvas, Ink};
use shotoshot_core::{Point, Rect, Tool};
use shotoshot_text::{Key, Language, Text, language, tr_in};
mod icons;
mod motion;
pub use icons::glyph;
pub use motion::{Chosen, Motion, SwatchLook};

/// A chosen button: filled with the blue of shape selection, under a dark glyph.
const CHOSEN_FILL: [u8; 3] = [100, 170, 255];
const CHOSEN_GLYPH: [u8; 3] = [16, 22, 32];
const PANEL: [u8; 3] = [32, 32, 32];
const HOVER: [u8; 3] = [55, 55, 55];

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Action {
    Tool(Tool),
    Color([u8; 3]),
    /// Text box context only: the palette colours the box's panel (`Capture::ink_target`).
    BoxColor,
    /// Text box context only: the palette colours the box's text; a right-click on it gives
    /// the text back its automatic black or white.
    TextColor,
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
    /// Mini toolbar only: shows the full toolbar, or folds it back (`Fold`). No key.
    MoreTools,
    Close,
}

/// How much of the toolbar a capture shows. The mini toolbar is a setting; whether it is
/// unfolded belongs to the capture alone, which always opens folded.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Fold {
    /// The mini toolbar is off: every button, as always.
    #[default]
    Full,
    /// Only `MINI` and the button that unfolds the rest.
    Folded,
    /// Every button, and the lit button that folds them back.
    Unfolded,
}

/// What the folded mini toolbar keeps (issue #6): the pen, then undo, copy, save and close;
/// Google Lens went too (Mickael, 2026-11-06). Every key still reaches every other action.
const MINI: [Action; 5] = [
    Action::Tool(Tool::Pen),
    Action::Undo,
    Action::Copy,
    Action::Save,
    Action::Close,
];
#[derive(Clone)]
pub struct Button {
    pub bounds: Rect,
    /// What the button is; its hint adds the keys it answers to (`tooltip`).
    pub label: Key,
    pub action: Action,
    pub panel: u8,
    pub active: bool,
    pub enabled: bool,
}
pub(crate) const HINT_DELAY_MS: u64 = 250;
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
const SHORTCUTS: [(Key, Action, u32, bool, bool); 27] = [
    (
        Key::ToolbarSelect,
        Action::Tool(Tool::Select),
        86,
        false,
        false,
    ),
    (
        Key::ToolbarRectangle,
        Action::Tool(Tool::Rectangle),
        82,
        false,
        false,
    ),
    (
        Key::ToolbarEllipse,
        Action::Tool(Tool::Ellipse),
        67,
        false,
        false,
    ),
    (Key::ToolbarLine, Action::Tool(Tool::Line), 76, false, false),
    (
        Key::ToolbarArrow,
        Action::Tool(Tool::Arrow),
        65,
        false,
        false,
    ),
    (Key::ToolbarPen, Action::Tool(Tool::Pen), 80, false, false),
    (
        Key::ToolbarHighlighter,
        Action::Tool(Tool::Highlight),
        72,
        false,
        false,
    ),
    (Key::ToolbarBlur, Action::Tool(Tool::Blur), 88, false, false),
    (
        Key::ToolbarLabel,
        Action::Tool(Tool::Label),
        66,
        false,
        false,
    ),
    (Key::ToolbarText, Action::Tool(Tool::Text), 84, false, false),
    (
        Key::ToolbarEraser,
        Action::Tool(Tool::Eraser),
        69,
        false,
        false,
    ),
    (Key::ToolbarLiveCapture, Action::Live, 113, false, false),
    (Key::ToolbarRed, Action::Color(COLORS[0]), 49, false, false),
    (
        Key::ToolbarYellow,
        Action::Color(COLORS[1]),
        50,
        false,
        false,
    ),
    (
        Key::ToolbarGreen,
        Action::Color(COLORS[2]),
        51,
        false,
        false,
    ),
    (Key::ToolbarBlue, Action::Color(COLORS[3]), 52, false, false),
    (
        Key::ToolbarWhite,
        Action::Color(COLORS[4]),
        53,
        false,
        false,
    ),
    (
        Key::ToolbarBlack,
        Action::Color(COLORS[5]),
        54,
        false,
        false,
    ),
    (
        Key::ToolbarCustomColor,
        Action::CustomColor,
        83,
        false,
        false,
    ),
    (Key::ToolbarEyedropper, Action::Eyedropper, 73, false, false),
    (Key::ToolbarFill, Action::Fill, 71, false, false),
    (Key::ToolbarUndo, Action::Undo, 90, true, false),
    (Key::ToolbarRedo, Action::Redo, 89, true, false),
    (
        Key::ToolbarGoogleLens,
        Action::GoogleSearch,
        71,
        true,
        false,
    ),
    // The hint names the shorter Ctrl+C (`named_keys`), which copies the image whenever no
    // annotation is selected (`keys`); the key read here, Ctrl+Shift+C, copies it always.
    (Key::ToolbarCopyImage, Action::Copy, 67, true, true),
    (Key::ToolbarSave, Action::Save, 83, true, false),
    (Key::ToolbarClose, Action::Close, 27, false, false),
];
/// Ctrl+Shift+Z, the second key of Redo, as (key, Ctrl, Shift).
const REDO_ALTERNATE: (u32, bool, bool) = (90, true, true);

/// Full-screen toggle: a button beside the size badge, outside the toolbar panels.
pub(crate) const FULL_SCREEN_KEY: u32 = 70;
pub(crate) const FULL_SCREEN_LABEL: Key = Key::CaptureFullScreen;
pub(crate) const RESTORE_SELECTION_LABEL: Key = Key::CaptureRestoreSelection;

pub(crate) fn shortcut_action(key: u32, ctrl: bool, shift: bool, alt: bool) -> Option<Action> {
    if alt {
        return None;
    }
    if key == FULL_SCREEN_KEY && !ctrl {
        return Some(Action::FullScreen);
    }
    if (key, ctrl, shift) == REDO_ALTERNATE {
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

/// Widest text a hint may have, in logical pixels at its size (12): a translation beyond it is
/// shortened rather than the hint widened (`every_tooltip_fits`).
pub const MAX_HINT_WIDTH: f64 = 600.;

/// Labels the capture gives a button in some state in place of its own (`Capture::buttons`),
/// and the buttons beside the size badge, each with its button's action.
const STATE_HINTS: [(Key, Action); 9] = [
    (Key::CaptureNothingToUndo, Action::Undo),
    (Key::CaptureNothingToRedo, Action::Redo),
    (Key::CaptureGoogleSearchInProgress, Action::GoogleSearch),
    (Key::CaptureLiveForNewCaptures, Action::Live),
    (Key::CaptureLiveOn, Action::Live),
    (Key::CaptureLiveFrozen, Action::Live),
    (FULL_SCREEN_LABEL, Action::FullScreen),
    (RESTORE_SELECTION_LABEL, Action::FullScreen),
    (Key::CaptureEditDimensions, Action::Dimensions),
];

/// Labels that explain why their button does nothing, so they name no key.
const KEYLESS_LABELS: [Key; 1] = [Key::CaptureLiveForNewCaptures];

/// The key combinations a hint names for `action`, as (key, Ctrl, Shift).
fn named_keys(action: Action) -> Vec<(u32, bool, bool)> {
    match action {
        Action::Copy => vec![(67, true, false)],
        Action::Redo => vec![(89, true, false), REDO_ALTERNATE],
        Action::FullScreen => vec![(FULL_SCREEN_KEY, false, false)],
        // Undo brings a cleared board back.
        Action::ClearBoard => named_keys(Action::Undo),
        _ => SHORTCUTS
            .iter()
            .filter(|entry| entry.1 == action)
            .map(|&(_, _, key, ctrl, shift)| (key, ctrl, shift))
            .collect(),
    }
}

/// The hint of `button` in the interface language: its label and the keys it answers to.
pub fn tooltip(button: &Button) -> String {
    tooltip_in(button, language())
}

/// The hint of `button` in `language`.
pub(crate) fn tooltip_in(button: &Button, language: Language) -> String {
    hint_in(button.label, button.action, language)
}

/// A label with a `{shortcut}` places the keys itself; any other is followed by them through the
/// `tooltip.with_shortcut` template, or stands alone when the action has no key.
fn hint_in(label: Key, action: Action, language: Language) -> String {
    let keys = if KEYLESS_LABELS.contains(&label) {
        Vec::new()
    } else {
        named_keys(action)
    };
    if keys.is_empty() {
        return Text::new_in(language, label).to_string();
    }
    let shortcut = keys
        .iter()
        .map(|&(key, ctrl, shift)| crate::shortcut_label_in(key, ctrl, shift, language))
        .collect::<Vec<_>>()
        .join(" / ");
    if label.placeholders().contains(&"shortcut") {
        return Text::with_in(language, label, &[("shortcut", &shortcut)]).to_string();
    }
    let args = [("label", tr_in(language, label)), ("shortcut", &shortcut)];
    // A drawing tool also says that the wheel sizes it (issue #2: nobody guessed).
    let template = match action {
        // Drawn from its centre, Alt frees it: nobody guessed (blind test of 2026-11-05).
        Action::Tool(Tool::Ellipse) => Key::TooltipEllipse,
        // The blur's strength follows its zone: the wheel does not change it.
        Action::Tool(tool) if !matches!(tool, Tool::Select | Tool::Blur) => {
            Key::TooltipWithShortcutAndWheel
        }
        _ => Key::TooltipWithShortcut,
    };
    Text::with_in(language, template, &args).to_string()
}

/// Every label a hint can show with its button's action: the toolbar, the board and the states.
fn all_hints() -> impl Iterator<Item = (Key, Action)> {
    SHORTCUTS
        .iter()
        .map(|&(label, action, ..)| (label, action))
        .chain(LABEL_ACTIONS)
        .chain(BOARD_ACTIONS)
        .chain(FOLD_ACTIONS)
        .chain(STATE_HINTS)
}

/// Every hint the toolbar and the buttons beside the size badge can show, in `language`.
pub fn all_tooltips_in(language: Language) -> Vec<String> {
    all_hints()
        .map(|(label, action)| hint_in(label, action, language))
        .collect()
}

pub fn buttons(region: Rect, width: u32, height: u32) -> Vec<Button> {
    buttons_in(
        region,
        None,
        Rect {
            min: Point::default(),
            max: Point {
                x: width as f64,
                y: height as f64,
            },
        },
        1.,
        false,
        false,
        Fold::Full,
    )
}

/// Buttons a board session adds to the action panel, before Close. They have no key.
const BOARD_ACTIONS: [(Key, Action); 1] = [(Key::ToolbarClearBoard, Action::ClearBoard)];
/// The mini toolbar's last tool button, folded then unfolded. It has no key.
const FOLD_ACTIONS: [(Key, Action); 2] = [
    (Key::ToolbarMoreTools, Action::MoreTools),
    (Key::ToolbarFewerTools, Action::MoreTools),
];
/// What the palette colours, box or text: buttons the action panel shows just before the
/// swatches while a text box is in hand or selected. They have no key.
const LABEL_ACTIONS: [(Key, Action); 2] = [
    (Key::ToolbarBoxColor, Action::BoxColor),
    (Key::ToolbarTextColor, Action::TextColor),
];

/// Both panels beside `region`, clear of `badge` (the size badge and its button) and kept on
/// `screen`, with the board's own buttons when `board` and the box and text colour buttons
/// when `label_colors`, as much of it as `fold` shows. The layout is in 96-DPI logical pixels and scaled once here, so
/// buttons keep their apparent size on every screen.
pub(crate) fn buttons_in(
    region: Rect,
    badge: Option<Rect>,
    screen: Rect,
    scale: f64,
    board: bool,
    label_colors: bool,
    fold: Fold,
) -> Vec<Button> {
    let resized = |r: Rect, factor: f64| Rect {
        min: r.min * factor,
        max: r.max * factor,
    };
    let mut buttons = layout(
        resized(region, 1. / scale),
        badge.map(|b| resized(b, 1. / scale)),
        resized(screen, 1. / scale),
        board,
        label_colors,
        fold,
    );
    for b in &mut buttons {
        b.bounds = resized(b.bounds, scale);
    }
    buttons
}

fn layout(
    region: Rect,
    badge: Option<Rect>,
    screen: Rect,
    board: bool,
    label_colors: bool,
    fold: Fold,
) -> Vec<Button> {
    let cell = 32.;
    let entries = SHORTCUTS.map(|(label, action, _, _, _)| (label, action));
    let is_tool = |a: Action| matches!(a, Action::Tool(_));
    // A board has no capture under its notes to blur.
    let mut tools: Vec<_> = entries
        .iter()
        .copied()
        .filter(|(_, a)| is_tool(*a) && !(board && *a == Action::Tool(Tool::Blur)))
        .collect();
    // Nor a live screen: its button only said so, greyed (blind test of 2026-11-05).
    let mut actions: Vec<_> = entries
        .iter()
        .copied()
        .filter(|(_, a)| !is_tool(*a) && !(board && *a == Action::Live))
        .collect();
    if board {
        let close = actions.len().saturating_sub(1);
        actions.splice(close..close, BOARD_ACTIONS);
    }
    if label_colors
        && let Some(swatches) = actions
            .iter()
            .position(|(_, a)| matches!(a, Action::Color(_)))
    {
        actions.splice(swatches..swatches, LABEL_ACTIONS);
    }
    match fold {
        Fold::Full => {}
        // One bar: the button that unfolds, then what the mini toolbar keeps.
        Fold::Folded => {
            actions = std::iter::once(FOLD_ACTIONS[0])
                .chain(tools.iter().chain(&actions).copied())
                .filter(|(_, a)| *a == Action::MoreTools || MINI.contains(a))
                .collect();
            tools.clear();
        }
        Fold::Unfolded => actions.insert(0, FOLD_ACTIONS[1]),
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
        .clamp(1., tools.len().max(1) as f64) as usize;
    // The folded mini toolbar has no tool panel: nothing for the actions to keep clear of.
    let tool_size = if tools.is_empty() {
        Point::default()
    } else {
        Point {
            x: tools.len().div_ceil(rows) as f64 * cell + 8.,
            y: rows as f64 * cell + 8.,
        }
    };
    let crowds = |a: Rect, tools: Rect| tools.size().x > 0. && overlaps(a, tools);
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
    let rect = |min: Point, size: Point| Rect {
        min,
        max: min + size,
    };
    // The badge sits on the region's top edge: the tools go beside both.
    let occupied = badge.map_or(region, |b| region.union(b));
    let mut tool_min = clamp(
        Point {
            x: if occupied.max.x + 8. + tool_size.x <= screen.max.x - 4. {
                occupied.max.x + 8.
            } else if occupied.min.x - 8. - tool_size.x >= screen.min.x + 4. {
                occupied.min.x - 8. - tool_size.x
            } else {
                occupied.max.x - tool_size.x - 8.
            },
            y: region.max.y - tool_size.y,
        },
        tool_size,
    );
    let above = region.min.y - 8. - action_size.y;
    let mut action_min = if region.max.y + 8. + action_size.y <= screen.max.y - 4. {
        clamp(
            Point {
                x: region.max.x - action_size.x,
                y: region.max.y + 8.,
            },
            action_size,
        )
    } else if above >= screen.min.y + 4. {
        // Above the region, at the badge's height: slid left of the badge when that keeps it
        // on the screen and off the tools, else raised over the badge.
        let beside = |x: f64| clamp(Point { x, y: above }, action_size);
        let clear = |min: Point| badge.is_none_or(|b| !overlaps(rect(min, action_size), b));
        let level = beside(region.max.x - action_size.x);
        let slid = badge.map(|b| beside(b.min.x - 8. - action_size.x));
        let raised = occupied.min.y - 8. - action_size.y;
        match slid {
            _ if clear(level) => level,
            Some(slid)
                if clear(slid) && !crowds(rect(slid, action_size), rect(tool_min, tool_size)) =>
            {
                slid
            }
            _ if raised >= screen.min.y + 4. => Point { y: raised, ..level },
            _ => level,
        }
    } else {
        clamp(
            Point {
                x: region.max.x - action_size.x,
                y: region.max.y - action_size.y - 8.,
            },
            action_size,
        )
    };
    let action_rect = rect(action_min, action_size);
    if crowds(action_rect, rect(tool_min, tool_size)) {
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

pub(crate) fn overlaps(a: Rect, b: Rect) -> bool {
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
/// Both panels as `state` marks them, with `motion` at `now`. `tint` paints the live tips.
pub fn draw<C: Canvas>(
    canvas: &mut C,
    buttons: &[Button],
    state: Chosen,
    tint: [u8; 3],
    (motion, now): (&Motion, std::time::Instant),
    scale: f64,
) -> Result<(), C::Error> {
    for id in [0, 1] {
        let group: Vec<_> = buttons.iter().filter(|b| b.panel == id).cloned().collect();
        draw_panel(canvas, &group, state, tint, (motion, now), scale)?;
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
    let label = tooltip(button);
    let extent = canvas.measure_text(&label, 12. * scale)?;
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
        &label,
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
    state: Chosen,
    tint: [u8; 3],
    (motion, now): (&Motion, std::time::Instant),
    scale: f64,
) -> Result<(), C::Error> {
    let Some(panel) = bounds(buttons, scale) else {
        return Ok(());
    };
    canvas.fill_round_rect(panel, 8. * scale, Ink::solid(PANEL))?;
    canvas.stroke_round_rect(panel, 8. * scale, Ink::solid([78, 78, 78]), scale)?;
    for b in buttons {
        let selected = state.button(b);
        let hovered = b.enabled && b.bounds.contains(state.pointer);
        let swatch = motion::is_swatch(b.action);
        // A bounce scales the whole button, glyph included, about its centre.
        let k = motion.scale(b.action, now);
        let (center, half) = (b.bounds.center(), b.bounds.size() * (k / 2.));
        let button = Button {
            bounds: Rect {
                min: center - half,
                max: center + half,
            },
            ..b.clone()
        };
        let radius = 5. * scale * k;
        let rgb = if selected {
            CHOSEN_FILL
        } else if hovered && !swatch {
            HOVER
        } else {
            PANEL
        };
        canvas.fill_round_rect(button.bounds, radius, Ink::solid(rgb))?;
        if !selected && let Some(alpha) = motion.release(b.action, now) {
            canvas.fill_round_rect(button.bounds, radius, Ink::with_alpha(CHOSEN_FILL, alpha))?;
        }
        let look = swatch.then(|| motion.swatch(b.action, state.swatch(b.action), hovered, now));
        icons::draw(
            canvas,
            &button,
            (state.current, tint),
            state.label,
            (selected, look),
            scale * k,
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
