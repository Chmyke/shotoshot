//! ROLE: The notification-area menu: one model for its order, painting and preview.
//! OWNER: App opens a `Popup` per opening; the Direct2D painter lives only while one is open.
//! INVARIANTS: Layout is in 96-DPI logical pixels, scaled once at paint time.
use super::*;
use crate::presentation::menu as commands;
use crate::render::{D2dCanvas, Renderer};
use crate::settings::{InputLayout, Shortcut};
use shotoshot_core::{Rect, Tool};
use shotoshot_text::{Key, Language, LanguageChoice, Text, language, tr_in};
use shotoshot_ui::{Canvas, Ink};
use std::borrow::Cow;
mod popup;
pub(super) use popup::{CLOSED, COMMAND, Popup};

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Title,
    Separator,
    /// Runs or opens something; shows its shortcut when it has one.
    Command,
    Toggle,
    /// Opens a submenu of choices and shows the current one.
    Submenu,
    /// One value of a submenu; the current one is ticked.
    Choice,
}

#[derive(Clone, Copy)]
pub(super) enum Icon {
    None,
    Capture,
    Instant,
    Halo,
    Keyboard,
    Mouse,
    Layout,
    Size,
    Clock,
    Sound,
    Power,
    /// A note pinned to the desktop.
    Board,
    /// The interface language.
    Globe,
    /// The editor toolbar's glyph for the same notion.
    Toolbar(Action),
}

/// What a row shows as its name.
#[derive(Clone, Copy)]
enum Label {
    Text(Key),
    /// The language submenu: its word in the interface language, then the English one.
    Languages,
    /// A language under its own name, whatever the interface language.
    Native(Language),
    /// A size of the input diagram, in percent.
    Percent(u32),
    Blank,
}

#[derive(Clone, Copy)]
struct Row {
    id: u32,
    label: Label,
    icon: Icon,
    kind: Kind,
}
const fn row(id: u32, label: Key, icon: Icon, kind: Kind) -> Row {
    Row {
        id,
        label: Label::Text(label),
        icon,
        kind,
    }
}
const fn title(id: u32, label: Key) -> Row {
    row(id, label, Icon::None, Kind::Title)
}
const fn choice(id: u32, label: Label) -> Row {
    Row {
        id,
        label,
        icon: Icon::None,
        kind: Kind::Choice,
    }
}

/// Command ids are the ones `App::menu` handles; titles (90..) and the separator are inert.
const ROOT: &[Row] = &[
    row(1, Key::MenuTakeScreenshot, Icon::Capture, Kind::Command),
    row(8, Key::MenuEditBoard, Icon::Board, Kind::Command),
    row(9, Key::MenuShowBoard, Icon::Board, Kind::Toggle),
    row(
        10,
        Key::MenuClearBoard,
        Icon::Toolbar(Action::ClearBoard),
        Kind::Command,
    ),
    title(90, Key::MenuPresentation),
    row(101, Key::MenuPointerHalo, Icon::Halo, Kind::Toggle),
    row(103, Key::MenuKeyboard, Icon::Keyboard, Kind::Toggle),
    row(104, Key::MenuMouse, Icon::Mouse, Kind::Toggle),
    row(130, Key::MenuKeyboardLayout, Icon::Layout, Kind::Submenu),
    row(140, Key::MenuInputSize, Icon::Size, Kind::Submenu),
    row(
        102,
        Key::MenuTemporaryShapes,
        Icon::Toolbar(Action::Tool(Tool::Pen)),
        Kind::Toggle,
    ),
    row(120, Key::MenuShapeDuration, Icon::Clock, Kind::Submenu),
    title(91, Key::MenuShortcuts),
    row(2, Key::MenuCapture, Icon::Capture, Kind::Command),
    row(6, Key::MenuInstantScreenshot, Icon::Instant, Kind::Command),
    row(
        110,
        Key::ToolbarArrow,
        Icon::Toolbar(Action::Tool(Tool::Arrow)),
        Kind::Command,
    ),
    row(
        111,
        Key::ToolbarRectangle,
        Icon::Toolbar(Action::Tool(Tool::Rectangle)),
        Kind::Command,
    ),
    row(
        112,
        Key::ToolbarEllipse,
        Icon::Toolbar(Action::Tool(Tool::Ellipse)),
        Kind::Command,
    ),
    row(13, Key::MenuBoard, Icon::Board, Kind::Command),
    title(92, Key::MenuOptions),
    row(7, Key::MenuScreenshotSound, Icon::Sound, Kind::Toggle),
    row(
        5,
        Key::MenuLiveCaptureDefault,
        Icon::Toolbar(Action::Live),
        Kind::Toggle,
    ),
    row(4, Key::MenuStartWithWindows, Icon::Power, Kind::Toggle),
    // Last of the options, right above Quit: a fixed place, found without reading the menu.
    Row {
        id: 150,
        label: Label::Languages,
        icon: Icon::Globe,
        kind: Kind::Submenu,
    },
    Row {
        id: 0,
        label: Label::Blank,
        icon: Icon::None,
        kind: Kind::Separator,
    },
    row(
        3,
        Key::MenuQuit,
        Icon::Toolbar(Action::Close),
        Kind::Command,
    ),
];
const DURATIONS: &[Row] = &[
    choice(121, Label::Text(Key::MenuDuration1)),
    choice(122, Label::Text(Key::MenuDuration3)),
    choice(123, Label::Text(Key::MenuDuration5)),
];
const LAYOUTS: &[Row] = &[
    choice(131, Label::Text(Key::MenuLayoutFull)),
    choice(132, Label::Text(Key::MenuLayoutCompact)),
    choice(133, Label::Text(Key::MenuLayoutGaming)),
];
const SIZES: &[Row] = &[
    choice(141, Label::Percent(100)),
    choice(142, Label::Percent(150)),
    choice(143, Label::Percent(200)),
];
/// The automatic choice (151), then every language in table order (152..=166).
const LANGUAGES: [Row; 1 + Language::ALL.len()] = {
    let mut rows = [choice(151, Label::Text(Key::MenuLanguageAuto)); 1 + Language::ALL.len()];
    let mut index = 0;
    while index < Language::ALL.len() {
        rows[index + 1] = choice(152 + index as u32, Label::Native(Language::ALL[index]));
        index += 1;
    }
    rows
};
/// The values a submenu row opens.
fn choices(id: u32) -> &'static [Row] {
    match id {
        120 => DURATIONS,
        130 => LAYOUTS,
        140 => SIZES,
        150 => &LANGUAGES,
        _ => &[],
    }
}
fn find(id: u32) -> Option<&'static Row> {
    ROOT.iter()
        .chain(DURATIONS)
        .chain(LAYOUTS)
        .chain(SIZES)
        .chain(&LANGUAGES)
        .find(|row| row.id == id)
}
/// The language a choice of the language submenu selects, None for any other command.
pub(super) fn on_language_choice(id: u32) -> Option<LanguageChoice> {
    let index = id.checked_sub(151)?;
    LanguageChoice::ALL.get(index as usize).copied()
}
/// The name of a row in `language`.
fn label_in(row: &Row, language: Language) -> Cow<'static, str> {
    match row.label {
        Label::Text(key) => tr_in(language, key).into(),
        Label::Languages => shotoshot_text::language_menu_label_in(language),
        Label::Native(name) => name.native_name().into(),
        Label::Percent(percent) => percent_in(percent as usize, language).into(),
        Label::Blank => "".into(),
    }
}
fn label(row: &Row) -> Cow<'static, str> {
    label_in(row, language())
}
fn percent_in(percent: usize, language: Language) -> String {
    Text::with_in(language, Key::MenuPercent, &[("n", &percent.to_string())]).to_string()
}
/// The icon a menu row shows, for the surfaces that continue what the row opens.
pub(super) fn icon(id: u32) -> Icon {
    find(id).map_or(Icon::None, |row| row.icon)
}

/// What the rows display for the current settings.
#[derive(Clone, Default)]
pub(super) struct Values {
    /// Packed with `shortcut`.
    pub capture: usize,
    pub instant: usize,
    /// Arrow, rectangle, ellipse.
    pub shapes: [usize; 3],
    pub duration_ms: u32,
    pub layout: InputLayout,
    /// Size of the input diagram, in percent.
    pub size: u32,
    pub halo: bool,
    pub temporary_shapes: bool,
    pub keys: bool,
    pub mouse: bool,
    pub sound: bool,
    pub live: bool,
    pub startup: bool,
    /// Whether the board is shown in the wallpaper.
    pub board: bool,
    /// Packed with `shortcut`.
    pub board_shortcut: usize,
    pub language: LanguageChoice,
}
impl Values {
    fn checked(&self, id: u32) -> bool {
        if let Some(choice) = on_language_choice(id) {
            return choice == self.language;
        }
        match id {
            101 => self.halo,
            102 => self.temporary_shapes,
            103 => self.keys,
            104 => self.mouse,
            7 => self.sound,
            5 => self.live,
            4 => self.startup,
            9 => self.board,
            _ => {
                commands::duration_ms(id) == Some(self.duration_ms)
                    || commands::layout(id) == Some(self.layout)
                    || commands::scale(id) == Some(self.size)
            }
        }
    }
    fn data(&self, id: u32) -> usize {
        match id {
            1 | 2 => self.capture,
            6 => self.instant,
            8 | 13 => self.board_shortcut,
            110..=112 => self.shapes[(id - 110) as usize],
            120 => self.duration_ms as usize,
            130 => self.layout.index(),
            140 => self.size as usize,
            // The position of the current choice in its submenu, as for the layout.
            150 => LANGUAGES
                .iter()
                .position(|row| on_language_choice(row.id) == Some(self.language))
                .unwrap_or(0),
            _ => 0,
        }
    }
}

/// Item data of a row showing a shortcut; `unavailable` marks one another program holds.
pub(super) fn shortcut(shortcut: Shortcut, unavailable: bool) -> usize {
    if shortcut.key == 0 {
        return 0;
    }
    ((shortcut.key << 16) | shortcut.modifiers | (u32::from(unavailable) << 31)) as usize
}
fn shortcut_text_in(data: usize, language: Language) -> String {
    if data == 0 {
        return String::new();
    }
    let (mods, key) = ((data & 0xffff) as u32, ((data >> 16) & 0xff) as u32);
    let keys = setup::key_label_in(mods, key, language).replace(" + ", "+");
    if data & (1 << 31) != 0 {
        Text::with_in(language, Key::MenuShortcutInUse, &[("shortcut", &keys)]).to_string()
    } else {
        keys
    }
}
/// Text on the right of a row, in `language`.
fn detail_in(row: &Row, data: usize, language: Language) -> String {
    match row.kind {
        Kind::Command => shortcut_text_in(data, language),
        Kind::Submenu => match row.id {
            130 | 150 => choices(row.id)
                .get(data)
                .map_or(String::new(), |current| label_in(current, language).into()),
            140 => percent_in(data, language),
            _ => Text::with_in(
                language,
                Key::MenuSeconds,
                &[("n", &(data / 1000).to_string())],
            )
            .to_string(),
        },
        _ => String::new(),
    }
}

const BACKGROUND: [u8; 3] = [0x2b, 0x2b, 0x2b];
const HOVER: [u8; 3] = [0x41, 0x41, 0x41];
pub(super) const TEXT: [u8; 3] = [0xf0, 0xf0, 0xf0];
pub(super) const SECONDARY: [u8; 3] = [0xc6, 0xc6, 0xc6];
pub(super) const MUTED: [u8; 3] = [0x9a, 0x9a, 0x9a];
pub(super) const RULE: [u8; 3] = [0x49, 0x49, 0x49];
pub(super) const ACCENT: [u8; 3] = [0x60, 0xcd, 0xff];
const ICON_X: f64 = 12.;
/// Toolbar glyphs sit on a 20 px grid; the menu shows them at 16 px.
const ICON_SCALE: f64 = 0.8;
const LABEL_X: f64 = 38.;
const MARGIN: f64 = 14.;
const TEXT_SIZE: f64 = 13.;
const TITLE_SIZE: f64 = 11.;
const SWITCH: Point = Point { x: 28., y: 14. };
/// Room for the submenu arrow.
const ARROW: f64 = 14.;

fn height(kind: Kind) -> f64 {
    match kind {
        Kind::Title => 26.,
        Kind::Separator => 9.,
        _ => 28.,
    }
}
/// Logical width a row needs to show its label and its right-hand side without clipping.
fn width(renderer: &Renderer, row: &Row, data: usize) -> Result<f64> {
    width_in(renderer, row, data, language())
}
/// `width` for the texts of `language`.
fn width_in(renderer: &Renderer, row: &Row, data: usize, language: Language) -> Result<f64> {
    let measure = |text: &str, size: f64| {
        Ok::<_, Error>(renderer.measure_text_in(text, size as f32, language)?.x)
    };
    let name = label_in(row, language);
    Ok(match row.kind {
        Kind::Separator => 0.,
        Kind::Title => ICON_X + measure(&name, TITLE_SIZE)? + MARGIN,
        Kind::Toggle => LABEL_X + measure(&name, TEXT_SIZE)? + 2. * MARGIN + SWITCH.x + MARGIN,
        Kind::Choice => LABEL_X + measure(&name, TEXT_SIZE)? + 2. * MARGIN,
        Kind::Command | Kind::Submenu => {
            let detail = detail_in(row, data, language);
            let arrow = if row.kind == Kind::Submenu { ARROW } else { 0. };
            let right = if detail.is_empty() {
                0.
            } else {
                2. * MARGIN + measure(&detail, TEXT_SIZE)?
            };
            LABEL_X + measure(&name, TEXT_SIZE)? + right + arrow + MARGIN
        }
    })
}

#[derive(Clone, Copy, Default)]
struct State {
    selected: bool,
    checked: bool,
    data: usize,
}

fn segments<C: Canvas>(
    canvas: &mut C,
    origin: Point,
    scale: f64,
    rgb: [u8; 3],
    points: &[(f64, f64)],
) -> std::result::Result<(), C::Error> {
    let at = |p: (f64, f64)| {
        origin
            + Point {
                x: p.0 * scale,
                y: p.1 * scale,
            }
    };
    for pair in points.windows(2) {
        canvas.line(at(pair[0]), at(pair[1]), Ink::solid(rgb), 1.6 * scale)?;
    }
    Ok(())
}
/// Clockwise arc in degrees from the positive x axis, on the glyph's 20 px grid.
fn arc<C: Canvas>(
    canvas: &mut C,
    origin: Point,
    scale: f64,
    rgb: [u8; 3],
    (center, radius): ((f64, f64), f64),
    (from, to): (f64, f64),
) -> std::result::Result<(), C::Error> {
    let points: Vec<_> = (0..=16)
        .map(|step| {
            let angle = (from + (to - from) * step as f64 / 16.).to_radians();
            (
                center.0 + radius * angle.cos(),
                center.1 + radius * angle.sin(),
            )
        })
        .collect();
    segments(canvas, origin, scale, rgb, &points)
}
pub(super) fn draw_icon<C: Canvas>(
    canvas: &mut C,
    icon: Icon,
    origin: Point,
    scale: f64,
    rgb: [u8; 3],
) -> std::result::Result<(), C::Error> {
    let at = |x: f64, y: f64| {
        origin
            + Point {
                x: x * scale,
                y: y * scale,
            }
    };
    let radius = |r: f64| Point {
        x: r * scale,
        y: r * scale,
    };
    let ink = Ink::solid(rgb);
    match icon {
        Icon::None => {}
        Icon::Toolbar(action) => {
            crate::toolbar::glyph(canvas, action, origin, scale, rgb, (rgb, rgb))?
        }
        Icon::Capture => {
            // Corner brackets of a selected area.
            for corner in [
                [(2., 7.), (2., 2.), (7., 2.)],
                [(13., 2.), (18., 2.), (18., 7.)],
                [(18., 13.), (18., 18.), (13., 18.)],
                [(7., 18.), (2., 18.), (2., 13.)],
            ] {
                segments(canvas, origin, scale, rgb, &corner)?;
            }
        }
        Icon::Instant => segments(
            canvas,
            origin,
            scale,
            rgb,
            &[
                (11.5, 1.5),
                (4., 11.5),
                (9.5, 11.5),
                (8.5, 18.5),
                (16., 8.5),
                (10.5, 8.5),
                (11.5, 1.5),
            ],
        )?,
        Icon::Halo => {
            canvas.stroke_ellipse(at(10., 10.), radius(8.), ink, 1.6 * scale)?;
            canvas.fill_ellipse(at(10., 10.), radius(2.8), ink)?;
        }
        Icon::Keyboard => {
            canvas.stroke_round_rect(
                Rect {
                    min: at(1., 4.),
                    max: at(19., 16.),
                },
                2.5 * scale,
                ink,
                1.6 * scale,
            )?;
            for key in [
                [(4.5, 8.5), (6., 8.5)],
                [(9.25, 8.5), (10.75, 8.5)],
                [(14., 8.5), (15.5, 8.5)],
                [(6.5, 12.5), (13.5, 12.5)],
            ] {
                segments(canvas, origin, scale, rgb, &key)?;
            }
        }
        Icon::Mouse => {
            canvas.stroke_round_rect(
                Rect {
                    min: at(5., 1.5),
                    max: at(15., 18.5),
                },
                5. * scale,
                ink,
                1.6 * scale,
            )?;
            segments(canvas, origin, scale, rgb, &[(10., 5.), (10., 8.5)])?;
        }
        Icon::Layout => {
            // A keyboard with its left part set apart.
            canvas.stroke_round_rect(
                Rect {
                    min: at(1., 4.),
                    max: at(19., 16.),
                },
                2.5 * scale,
                ink,
                1.6 * scale,
            )?;
            for stroke in [
                [(11., 4.), (11., 16.)],
                [(4.5, 8.5), (7.5, 8.5)],
                [(4.5, 12.5), (7.5, 12.5)],
            ] {
                segments(canvas, origin, scale, rgb, &stroke)?;
            }
        }
        Icon::Size => {
            // Two corners pulled apart along a diagonal.
            for stroke in [
                &[(11.5, 3.), (17., 3.), (17., 8.5)][..],
                &[(8.5, 17.), (3., 17.), (3., 11.5)],
                &[(16.5, 3.5), (3.5, 16.5)],
            ] {
                segments(canvas, origin, scale, rgb, stroke)?;
            }
        }
        Icon::Clock => {
            canvas.stroke_ellipse(at(10., 10.), radius(8.), ink, 1.6 * scale)?;
            segments(
                canvas,
                origin,
                scale,
                rgb,
                &[(10., 5.5), (10., 10.), (13.5, 12.)],
            )?;
        }
        Icon::Sound => {
            segments(
                canvas,
                origin,
                scale,
                rgb,
                &[
                    (2., 7.5),
                    (5.5, 7.5),
                    (10.5, 3.5),
                    (10.5, 16.5),
                    (5.5, 12.5),
                    (2., 12.5),
                    (2., 7.5),
                ],
            )?;
            arc(canvas, origin, scale, rgb, ((10.5, 10.), 4.), (-45., 45.))?;
            arc(canvas, origin, scale, rgb, ((10.5, 10.), 7.5), (-50., 50.))?;
        }
        Icon::Power => {
            arc(canvas, origin, scale, rgb, ((10., 10.5), 7.5), (-55., 235.))?;
            segments(canvas, origin, scale, rgb, &[(10., 2.), (10., 10.)])?;
        }
        Icon::Board => {
            // A note and two lines of its text.
            canvas.stroke_round_rect(
                Rect {
                    min: at(3., 2.5),
                    max: at(17., 17.5),
                },
                2.5 * scale,
                ink,
                1.6 * scale,
            )?;
            for line in [[(6.5, 8.), (13.5, 8.)], [(6.5, 12.), (11., 12.)]] {
                segments(canvas, origin, scale, rgb, &line)?;
            }
        }
        Icon::Globe => {
            // The earth, one meridian and the equator.
            canvas.stroke_ellipse(at(10., 10.), radius(8.), ink, 1.6 * scale)?;
            canvas.stroke_ellipse(
                at(10., 10.),
                Point {
                    x: 3.5 * scale,
                    y: 8. * scale,
                },
                ink,
                1.6 * scale,
            )?;
            segments(canvas, origin, scale, rgb, &[(2., 10.), (18., 10.)])?;
        }
    }
    Ok(())
}

/// One row filling `size` device pixels, at `scale` device pixels per logical pixel.
fn paint<C: Canvas>(
    canvas: &mut C,
    row: &Row,
    state: State,
    size: Point,
    scale: f64,
) -> std::result::Result<(), C::Error> {
    let u = |value: f64| value * scale;
    canvas.fill_rect(
        Rect {
            min: Point::default(),
            max: size,
        },
        Ink::solid(BACKGROUND),
    )?;
    let name = label(row);
    let middle = size.y / 2.;
    let text = |canvas: &mut C, text: &str, left: f64, right: f64, bottom: f64, size: f64, rgb| {
        let line = canvas.measure_text("X", u(size))?.y;
        canvas.text(
            text,
            Rect {
                min: Point {
                    x: left,
                    y: bottom - line,
                },
                max: Point {
                    x: right,
                    y: bottom,
                },
            },
            u(size),
            Ink::solid(rgb),
        )
    };
    match row.kind {
        Kind::Separator => {
            return canvas.fill_rect(
                Rect {
                    min: Point {
                        x: u(ICON_X),
                        y: middle.floor(),
                    },
                    max: Point {
                        x: size.x - u(ICON_X),
                        y: middle.floor() + u(1.).round().max(1.),
                    },
                },
                Ink::solid(RULE),
            );
        }
        Kind::Title => {
            return text(
                canvas,
                &name,
                u(ICON_X),
                size.x,
                size.y - u(3.),
                TITLE_SIZE,
                MUTED,
            );
        }
        _ => {}
    }
    if state.selected {
        canvas.fill_round_rect(
            Rect {
                min: Point { x: u(4.), y: u(1.) },
                max: Point {
                    x: size.x - u(4.),
                    y: size.y - u(1.),
                },
            },
            u(4.),
            Ink::solid(HOVER),
        )?;
    }
    let icon_origin = Point {
        x: u(ICON_X),
        y: middle - u(10. * ICON_SCALE),
    };
    if row.kind == Kind::Choice {
        if state.checked {
            segments(
                canvas,
                icon_origin,
                u(ICON_SCALE),
                TEXT,
                &[(4., 10.5), (8.5, 15.), (16., 5.5)],
            )?;
        }
    } else {
        draw_icon(canvas, row.icon, icon_origin, u(ICON_SCALE), SECONDARY)?;
    }
    let line = canvas.measure_text("X", u(TEXT_SIZE))?.y;
    let bottom = middle + line / 2.;
    text(canvas, &name, u(LABEL_X), size.x, bottom, TEXT_SIZE, TEXT)?;
    let mut right = size.x - u(MARGIN);
    if row.kind == Kind::Submenu {
        segments(
            canvas,
            Point {
                x: right - u(8.),
                y: middle - u(6.),
            },
            u(0.6),
            SECONDARY,
            &[(6., 4.), (12., 10.), (6., 16.)],
        )?;
        right -= u(ARROW);
    }
    if row.kind == Kind::Toggle {
        let track = Rect {
            min: Point {
                x: right - u(SWITCH.x),
                y: middle - u(SWITCH.y / 2.),
            },
            max: Point {
                x: right,
                y: middle + u(SWITCH.y / 2.),
            },
        };
        let knob = |x: f64| Point { x, y: middle };
        if state.checked {
            canvas.fill_round_rect(track, u(SWITCH.y / 2.), Ink::solid(ACCENT))?;
            canvas.fill_ellipse(
                knob(track.max.x - u(SWITCH.y / 2.)),
                Point {
                    x: u(4.5),
                    y: u(4.5),
                },
                Ink::solid(BACKGROUND),
            )?;
        } else {
            let inset = Point {
                x: u(0.5),
                y: u(0.5),
            };
            canvas.stroke_round_rect(
                Rect {
                    min: track.min + inset,
                    max: track.max - inset,
                },
                u(SWITCH.y / 2. - 0.5),
                Ink::solid(MUTED),
                u(1.),
            )?;
            canvas.fill_ellipse(
                knob(track.min.x + u(SWITCH.y / 2.)),
                Point { x: u(4.), y: u(4.) },
                Ink::solid(SECONDARY),
            )?;
        }
        return Ok(());
    }
    let detail = detail_in(row, state.data, language());
    if !detail.is_empty() {
        let extent = canvas.measure_text(&detail, u(TEXT_SIZE))?.x;
        text(
            canvas,
            &detail,
            right - extent,
            // Measuring and drawing round differently: never clip the last glyph.
            size.x,
            bottom,
            TEXT_SIZE,
            SECONDARY,
        )?;
    }
    Ok(())
}

thread_local! {
    // Created by the first row of an opening menu and released when it closes:
    // the resident keeps no renderer at rest.
    static PAINTER: RefCell<Option<Renderer>> = const { RefCell::new(None) };
}
/// Releases the painter when the menu closes, including on an early return.
pub(super) struct Open;
impl Drop for Open {
    fn drop(&mut self) {
        PAINTER.with(|painter| painter.borrow_mut().take());
    }
}
fn with_painter<T>(work: impl FnOnce(&Renderer) -> Result<T>) -> Result<T> {
    PAINTER.with(|slot| {
        let mut slot = slot.borrow_mut();
        let renderer = match &mut *slot {
            Some(renderer) => renderer,
            None => slot.insert(Renderer::software()?),
        };
        let result = work(renderer);
        if result.is_err() {
            // A lost render target is rebuilt by the next row.
            *slot = None;
        }
        result
    })
}
fn draw_row(
    renderer: &Renderer,
    dc: HDC,
    bounds: RECT,
    row: &Row,
    state: State,
    scale: f64,
) -> Result<()> {
    unsafe {
        renderer.dc.BindDC(dc, &bounds)?;
        renderer.dc.BeginDraw();
        renderer
            .dc
            .SetTransform(&windows_numerics::Matrix3x2::identity());
        let size = Point {
            x: (bounds.right - bounds.left) as f64,
            y: (bounds.bottom - bounds.top) as f64,
        };
        let painted = D2dCanvas::new(renderer, &renderer.dc)
            .and_then(|mut canvas| paint(&mut canvas, row, state, size, scale));
        let ended = renderer.dc.EndDraw(None, None);
        painted?;
        ended
    }
}

/// Paints the whole menu, with the submenu of row `open` beside it, at `scale`.
fn render(values: &Values, selected: u32, open: u32, scale: f64) -> Result<Image> {
    let _open = Open;
    let root = popup::Column::new(ROOT, values, scale)?;
    let beside = popup::Column::new(choices(open), values, scale)?;
    let position = |id: u32| ROOT.iter().position(|row| row.id == id);
    // The submenu's first row lines up with the row that opens it, as on screen.
    let hang = position(open).map_or(0, |index| root.top(index) - beside.top(0));
    paint_image(
        (root.width + beside.width) as u32,
        root.height().max(hang + beside.height()) as u32,
        |dc| {
            root.paint(dc, POINT::default(), values, position(selected), scale)?;
            beside.paint(
                dc,
                POINT {
                    x: root.width,
                    y: hang,
                },
                values,
                None,
                scale,
            )
        },
    )
}
fn sample() -> Values {
    let defaults = Settings::default();
    let shapes = defaults.presentation.shape_shortcuts;
    Values {
        capture: shortcut(
            Shortcut {
                modifiers: 0,
                key: crate::print_screen::KEY,
            },
            false,
        ),
        instant: shortcut(defaults.instant_shortcut, false),
        shapes: [
            shortcut(shapes[0], false),
            shortcut(shapes[1], true),
            shortcut(shapes[2], false),
        ],
        duration_ms: defaults.presentation.duration_ms,
        layout: defaults.presentation.input_layout,
        size: defaults.presentation.input_scale,
        halo: true,
        temporary_shapes: true,
        keys: false,
        mouse: false,
        sound: defaults.instant_sound,
        live: defaults.live_capture,
        startup: defaults.start_with_windows,
        board: defaults.board_visible,
        board_shortcut: shortcut(defaults.board_shortcut, false),
        language: defaults.language,
    }
}
pub(super) fn preview() -> Result<()> {
    save_png(
        &render(&sample(), 110, 120, 1.)?,
        std::path::Path::new("target/shotoshot-menu.png"),
    )
}
/// Opens and closes the menu's renderer the way a real menu does, for the resident diagnostic.
pub(super) fn probe() -> Result<()> {
    render(&sample(), 0, 120, 1.).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The English text of a row, as the existing assertions compare it.
    fn english(row: &Row) -> String {
        label_in(row, Language::En).into_owned()
    }

    const WIDTH: i32 = 320;
    fn rendered(row: &Row, state: State, scale: f64) -> Image {
        let _open = Open;
        let (width, height) = (
            (WIDTH as f64 * scale) as i32,
            (height(row.kind) * scale).round() as i32,
        );
        paint_image(width as u32, height as u32, |dc| {
            with_painter(|renderer| {
                draw_row(
                    renderer,
                    dc,
                    RECT {
                        right: width,
                        bottom: height,
                        ..Default::default()
                    },
                    row,
                    state,
                    scale,
                )
            })
        })
        .unwrap()
    }
    /// Pixels of logical columns `from..to` that differ from the menu background.
    fn drawn(image: &Image, from: f64, to: f64, scale: f64) -> usize {
        let columns = (from * scale) as usize..(to * scale) as usize;
        image
            .pixels
            .as_chunks::<4>()
            .0
            .iter()
            .enumerate()
            .filter(|(index, pixel)| {
                columns.contains(&(index % image.width as usize))
                    && pixel[..3] != [BACKGROUND[2], BACKGROUND[1], BACKGROUND[0]]
            })
            .count()
    }
    fn accent(image: &Image) -> usize {
        image
            .pixels
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|pixel| pixel[..3] == [ACCENT[2], ACCENT[1], ACCENT[0]])
            .count()
    }

    #[test]
    fn every_command_and_setting_has_an_icon() {
        for row in ROOT {
            if matches!(row.kind, Kind::Title | Kind::Separator) {
                continue;
            }
            let image = rendered(row, State::default(), 1.);
            assert!(
                drawn(&image, ICON_X, ICON_X + 16., 1.) > 20,
                "{} has no icon",
                english(row)
            );
        }
    }

    #[test]
    fn the_board_rows_show_their_state_and_shortcut() {
        let values = Values {
            board: true,
            board_shortcut: 77,
            ..Default::default()
        };
        assert!(values.checked(9));
        assert_eq!([values.data(8), values.data(13)], [77, 77]);
        assert!(find(8).is_some_and(|r| english(r) == "Edit board"));
        assert!(find(9).is_some_and(|r| english(r) == "Show board" && r.kind == Kind::Toggle));
        assert!(find(13).is_some_and(|r| english(r) == "Board" && r.kind == Kind::Command));
        assert!(find(10).is_some_and(|r| english(r) == "Clear board"
            && r.kind == Kind::Command
            && matches!(r.icon, Icon::Toolbar(Action::ClearBoard))));
    }

    #[test]
    fn a_switch_shows_whether_a_setting_is_on() {
        let row = find(101).unwrap();
        let off = rendered(row, State::default(), 1.);
        let on = rendered(
            row,
            State {
                checked: true,
                ..Default::default()
            },
            1.,
        );
        let switch = (WIDTH as f64 - MARGIN - SWITCH.x, WIDTH as f64 - MARGIN);
        assert!(
            drawn(&off, switch.0, switch.1, 1.) > 40,
            "an off switch stays visible"
        );
        assert_eq!(accent(&off), 0);
        assert!(accent(&on) > 150);
    }

    #[test]
    fn only_the_current_duration_is_ticked() {
        let row = find(122).unwrap();
        let other = rendered(row, State::default(), 1.);
        let current = rendered(
            row,
            State {
                checked: true,
                ..Default::default()
            },
            1.,
        );
        assert_eq!(drawn(&other, ICON_X, ICON_X + 16., 1.), 0);
        assert!(drawn(&current, ICON_X, ICON_X + 16., 1.) > 15);
        let values = Values {
            duration_ms: 3000,
            layout: InputLayout::Gaming,
            size: 150,
            ..Default::default()
        };
        assert_eq!(
            [121, 122, 123].map(|id| values.checked(id)),
            [false, true, false]
        );
        assert_eq!(
            [131, 132, 133].map(|id| values.checked(id)),
            [false, false, true]
        );
        assert_eq!(
            [141, 142, 143].map(|id| values.checked(id)),
            [false, true, false]
        );
    }

    #[test]
    fn a_submenu_row_names_its_current_choice() {
        let values = Values {
            duration_ms: 5000,
            layout: InputLayout::Compact,
            size: 200,
            ..Default::default()
        };
        let shown = |id| detail_in(find(id).unwrap(), values.data(id), Language::En);
        assert_eq!(
            [shown(120), shown(130), shown(140)],
            ["5 s", "Compact", "200%"]
        );
        // The presentation submenus offer three values; the language one, every language.
        for row in ROOT
            .iter()
            .filter(|row| row.kind == Kind::Submenu && row.id != 150)
        {
            assert_eq!(choices(row.id).len(), 3, "{}", english(row));
        }
        assert_eq!(
            LAYOUTS.iter().map(english).collect::<Vec<_>>(),
            ["Full", "Compact", "Gaming"]
        );
    }

    #[test]
    fn rows_scale_with_the_monitor() {
        let row = find(4).unwrap();
        let state = State {
            checked: true,
            ..Default::default()
        };
        let (normal, doubled) = (rendered(row, state, 1.), rendered(row, state, 2.));
        assert_eq!((doubled.width, doubled.height), (2 * normal.width, 56));
        // Same layout at twice the size: four times the ink, give or take antialiasing.
        let ratio = accent(&doubled) as f64 / accent(&normal) as f64;
        assert!((3.5..=5.).contains(&ratio), "{ratio}");
        assert_eq!(drawn(&doubled, 0., ICON_X - 1., 2.), 0);
        assert!(drawn(&doubled, ICON_X, ICON_X + 16., 2.) > 80);
    }

    #[test]
    fn width_leaves_room_for_the_shortcut() {
        let _open = Open;
        let row = find(6).unwrap();
        let width_of = |data| with_painter(|renderer| width(renderer, row, data)).unwrap();
        let short = shortcut(
            Shortcut {
                modifiers: 0,
                key: 145,
            },
            false,
        );
        let long = shortcut(
            Shortcut {
                modifiers: 7,
                key: 44,
            },
            true,
        );
        assert_eq!(shortcut_text_in(short, Language::En), "Scroll Lock");
        assert_eq!(
            shortcut_text_in(long, Language::En),
            "Ctrl+Shift+Alt+Print Screen (in use)"
        );
        assert!(width_of(0) < width_of(short) && width_of(short) < width_of(long));
        // The longest shortcut is drawn entirely inside the width the row asks for.
        let needed = width_of(long);
        let image = paint_image(needed.ceil() as u32 + 40, 28, |dc| {
            with_painter(|renderer| {
                draw_row(
                    renderer,
                    dc,
                    RECT {
                        right: needed.ceil() as i32,
                        bottom: 28,
                        ..Default::default()
                    },
                    row,
                    State {
                        data: long,
                        ..Default::default()
                    },
                    1.,
                )
            })
        })
        .unwrap();
        let label_end = LABEL_X
            + with_painter(|renderer| renderer.measure_text(&label(row), TEXT_SIZE as f32))
                .unwrap()
                .x;
        assert_eq!(drawn(&image, label_end + 2., label_end + MARGIN, 1.), 0);
        assert_eq!(drawn(&image, needed - MARGIN + 2., needed, 1.), 0);
    }

    #[test]
    fn english_labels_are_unchanged() {
        let root: Vec<_> = ROOT
            .iter()
            .filter(|row| row.id != 150)
            .map(english)
            .collect();
        assert_eq!(
            root,
            [
                "Take a screenshot",
                "Edit board",
                "Show board",
                "Clear board",
                "Presentation",
                "Pointer halo",
                "Keyboard",
                "Mouse",
                "Keyboard layout",
                "Keyboard and mouse size",
                "Temporary shapes",
                "Shape duration",
                "Shortcuts · click to change",
                "Capture",
                "Instant screenshot",
                "Arrow",
                "Rectangle",
                "Ellipse",
                "Board",
                "Options",
                "Screenshot sound",
                "Live capture by default",
                "Start with Windows",
                "",
                "Quit Shotoshot",
            ]
        );
        assert_eq!(
            DURATIONS.iter().map(english).collect::<Vec<_>>(),
            ["1 second", "3 seconds", "5 seconds"]
        );
        assert_eq!(
            SIZES.iter().map(english).collect::<Vec<_>>(),
            ["100%", "150%", "200%"]
        );
    }

    #[test]
    fn language_row_keeps_the_english_word() {
        let row = find(150).unwrap();
        assert!(row.kind == Kind::Submenu && matches!(row.icon, Icon::Globe));
        assert_eq!(label_in(row, Language::En), "Language");
        assert_eq!(label_in(row, Language::Ko), "언어 · Language");
        for language in Language::ALL.into_iter().skip(1) {
            let label = label_in(row, language);
            assert!(
                label.ends_with(" · Language"),
                "{}: {label}",
                language.tag()
            );
            assert!(
                !label.starts_with("Language"),
                "{}: {label}",
                language.tag()
            );
        }
    }

    #[test]
    fn language_choices_show_native_names() {
        let expected = [
            "English",
            "简体中文",
            "Русский",
            "Español",
            "Português (Brasil)",
            "Deutsch",
            "日本語",
            "Français",
            "Polski",
            "한국어",
            "繁體中文",
            "Türkçe",
            "Українська",
            "Italiano",
            "Čeština",
        ];
        for language in Language::ALL {
            let names: Vec<_> = (152..=166)
                .map(|id| label_in(find(id).unwrap(), language))
                .collect();
            assert_eq!(names, expected, "{}", language.tag());
        }
        assert_eq!(
            label_in(find(151).unwrap(), Language::En),
            "Windows language"
        );
        assert_eq!(choices(150).len(), 16);
        assert!(choices(150).iter().all(|row| row.kind == Kind::Choice));
    }

    #[test]
    fn checked_language_follows_the_setting() {
        let ticked = |values: &Values| {
            (150..=167)
                .filter(|&id| values.checked(id))
                .collect::<Vec<_>>()
        };
        assert_eq!(ticked(&Values::default()), [151]);
        let german = Values {
            language: LanguageChoice::Fixed(Language::De),
            ..Default::default()
        };
        assert_eq!(ticked(&german), [157]);
        // The row names the current choice, like the other submenus.
        let row = find(150).unwrap();
        assert_eq!(detail_in(row, german.data(150), Language::En), "Deutsch");
        assert_eq!(
            detail_in(row, Values::default().data(150), Language::En),
            "Windows language"
        );
    }

    #[test]
    fn a_language_choice_is_read_from_its_id() {
        assert_eq!(on_language_choice(151), Some(LanguageChoice::Auto));
        assert_eq!(
            on_language_choice(152),
            Some(LanguageChoice::Fixed(Language::En))
        );
        assert_eq!(
            on_language_choice(157),
            Some(LanguageChoice::Fixed(Language::De))
        );
        assert_eq!(
            on_language_choice(166),
            Some(LanguageChoice::Fixed(Language::Cs))
        );
        for id in [0, 3, 143, 150, 167] {
            assert_eq!(on_language_choice(id), None, "{id}");
        }
        for (index, row) in choices(150).iter().enumerate().skip(1) {
            assert_eq!(
                on_language_choice(row.id),
                Some(LanguageChoice::Fixed(Language::ALL[index - 1]))
            );
        }
    }

    /// Widest row of a menu column in `language`, in logical pixels at scale 1.
    fn widest(rows: &[Row], values: &Values, language: Language) -> f64 {
        with_painter(|renderer| {
            let mut widest: f64 = 0.;
            for row in rows {
                // The language row names any of its sixteen choices.
                let data: Vec<usize> = if row.id == 150 {
                    (0..choices(150).len()).collect()
                } else {
                    vec![values.data(row.id)]
                };
                for data in data {
                    widest = widest.max(width_in(renderer, row, data, language)?);
                }
            }
            Ok(widest)
        })
        .unwrap()
    }

    #[test]
    fn every_menu_fits() {
        let _open = Open;
        let values = sample();
        for language in Language::ALL {
            for rows in [ROOT, DURATIONS, LAYOUTS, SIZES, &LANGUAGES[..]] {
                let width = widest(rows, &values, language);
                assert!(
                    width <= 480.,
                    "{}: a menu is {width:.0} px wide; shorten its longest text",
                    language.tag()
                );
            }
        }
    }

    #[test]
    fn the_language_row_leaves_the_english_menu_as_wide_as_it_was() {
        let _open = Open;
        let values = sample();
        let others: Vec<Row> = ROOT.iter().filter(|row| row.id != 150).copied().collect();
        let language_row = [*find(150).unwrap()];
        assert!(
            widest(&language_row, &values, Language::En) <= widest(&others, &values, Language::En)
        );
    }

    #[test]
    fn a_closed_menu_keeps_no_renderer() {
        for open in [120, 130, 140] {
            render(&sample(), 110, open, 1.).unwrap();
        }
        assert!(PAINTER.with(|painter| painter.borrow().is_none()));
    }
}
