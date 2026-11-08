//! ROLE: The notification-area menu: one model for its order, painting and preview.
//! OWNER: App opens a `Popup` per opening; the Direct2D painter lives only while one is open.
//! INVARIANTS: Layout is in 96-DPI logical pixels, scaled once at paint time.
use super::*;
use crate::render::{D2dCanvas, Renderer};
use crate::settings::Shortcut;
use shotoshot_core::{Rect, Tool};
use shotoshot_text::{Key, Language, LanguageChoice, language, tr_in};
pub(super) use shotoshot_ui::icon::Icon;
pub(super) use shotoshot_ui::icon::draw as draw_icon;
use shotoshot_ui::settings_panel::{RowEdit, ShortcutRow};
use shotoshot_ui::{Canvas, Ink, TextStyle, settings_panel};
use std::borrow::Cow;
mod popup;
pub(super) use popup::{CLOSED, COMMAND, EDIT, EditOp, Popup};

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

/// What a row shows as its name.
#[derive(Clone, Copy)]
enum Label {
    Text(Key),
    /// The language submenu: its word in the interface language, then the English one.
    Languages,
    /// A language under its own name, whatever the interface language.
    Native(Language),
    Blank,
}

#[derive(Clone, Copy)]
struct Row {
    id: u32,
    label: Label,
    icon: Icon,
    kind: Kind,
    /// A second, smaller line saying what the row is about.
    note: Option<Key>,
}
const fn row(id: u32, label: Key, icon: Icon, kind: Kind) -> Row {
    Row {
        id,
        label: Label::Text(label),
        icon,
        kind,
        note: None,
    }
}
const fn title(id: u32, label: Key) -> Row {
    row(id, label, Icon::None, Kind::Title)
}
/// `row` with a line under its label: what "Instant screenshot" or "Board" mean was guessed
/// by nobody (blind test of 2026-11-05).
const fn noted(row: Row, note: Key) -> Row {
    Row {
        note: Some(note),
        ..row
    }
}
const fn choice(id: u32, label: Label) -> Row {
    Row {
        id,
        label,
        icon: Icon::None,
        kind: Kind::Choice,
        note: None,
    }
}

/// Command ids are the ones `App::menu` handles; titles (90..) and the separator are inert.
/// The menu acts; what is set once lives in the Settings window (Mickaël, 2026-10-30).
const ROOT: &[Row] = &[
    row(1, Key::MenuTakeScreenshot, Icon::Capture, Kind::Command),
    noted(
        row(7, Key::MenuInstantScreenshot, Icon::Instant, Kind::Command),
        Key::MenuInstantScreenshotNote,
    ),
    noted(title(91, Key::MenuBoard), Key::MenuBoardNote),
    row(8, Key::MenuEditBoard, Icon::BoardEdit, Kind::Command),
    row(9, Key::MenuShowBoard, Icon::Eye, Kind::Toggle),
    row(
        10,
        Key::MenuClearBoard,
        Icon::Toolbar(Action::ClearBoard),
        Kind::Command,
    ),
    title(90, Key::MenuPresentation),
    row(101, Key::MenuPointerEffects, Icon::Halo, Kind::Toggle),
    row(107, Key::MenuMagnifier, Icon::Loupe, Kind::Toggle),
    row(103, Key::MenuKeyboard, Icon::Keyboard, Kind::Toggle),
    row(104, Key::MenuMouse, Icon::Mouse, Kind::Toggle),
    row(
        102,
        Key::MenuTemporaryShapes,
        Icon::Toolbar(Action::Tool(Tool::Pen)),
        Kind::Toggle,
    ),
    // Their keys, to learn them; a click records new ones (Mickaël, 2026-10-30).
    row(
        171,
        Key::ToolbarArrow,
        Icon::Toolbar(Action::Tool(Tool::Arrow)),
        Kind::Command,
    ),
    row(
        172,
        Key::ToolbarRectangle,
        Icon::Toolbar(Action::Tool(Tool::Rectangle)),
        Kind::Command,
    ),
    row(
        173,
        Key::ToolbarEllipse,
        Icon::Toolbar(Action::Tool(Tool::Ellipse)),
        Kind::Command,
    ),
    Row {
        id: 0,
        label: Label::Blank,
        icon: Icon::None,
        kind: Kind::Separator,
        note: None,
    },
    row(170, Key::SettingsTitle, Icon::Gear, Kind::Command),
    // Right above Quit: a fixed place, found without reading the menu.
    Row {
        id: 150,
        label: Label::Languages,
        icon: Icon::Globe,
        kind: Kind::Submenu,
        note: None,
    },
    row(3, Key::MenuQuit, Icon::Power, Kind::Command),
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
        150 => &LANGUAGES,
        _ => &[],
    }
}
#[cfg(test)]
fn find(id: u32) -> Option<&'static Row> {
    ROOT.iter().chain(&LANGUAGES).find(|row| row.id == id)
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
        Label::Blank => "".into(),
    }
}
fn label(row: &Row) -> Cow<'static, str> {
    label_in(row, language())
}

/// The shortcut row `id` shows and changes in place, if any.
pub(super) fn shortcut_row(id: u32) -> Option<ShortcutRow> {
    Some(match id {
        1 => ShortcutRow::Capture,
        7 => ShortcutRow::Instant,
        8 => ShortcutRow::Board,
        171 => ShortcutRow::Arrow,
        172 => ShortcutRow::Rectangle,
        173 => ShortcutRow::Ellipse,
        _ => return None,
    })
}
/// The menu row showing `row`, if the menu shows it.
fn row_id(row: ShortcutRow) -> Option<u32> {
    ROOT.iter()
        .map(|candidate| candidate.id)
        .find(|&id| shortcut_row(id) == Some(row))
}

/// What the rows display for the current settings.
#[derive(Clone, Default)]
pub(super) struct Values {
    /// Packed with `shortcut`.
    pub capture: usize,
    pub instant: usize,
    pub halo: bool,
    pub loupe: bool,
    pub temporary_shapes: bool,
    /// The arrow, rectangle and ellipse shortcuts, packed with `shortcut`.
    pub shapes: [usize; 3],
    pub keys: bool,
    pub mouse: bool,
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
            107 => self.loupe,
            102 => self.temporary_shapes,
            103 => self.keys,
            104 => self.mouse,
            9 => self.board,
            _ => false,
        }
    }
    fn data(&self, id: u32) -> usize {
        match id {
            1 => self.capture,
            7 => self.instant,
            8 => self.board_shortcut,
            171..=173 => self.shapes[id as usize - 171],
            // The position of the current choice in its submenu.
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
/// The keys of a shortcut row in `language`, one keycap each, and whether another program
/// holds them: the keycaps then turn orange.
fn caps_in(data: usize, language: Language) -> (Vec<String>, bool) {
    if data == 0 {
        return (Vec::new(), false);
    }
    let (mods, key) = ((data & 0xffff) as u32, ((data >> 16) & 0xff) as u32);
    let caps = setup::key_label_in(mods, key, language)
        .split(" + ")
        .map(str::to_string)
        .collect();
    (caps, data & (1 << 31) != 0)
}
/// The current choice a submenu row names on its right, in `language`.
fn detail_in(row: &Row, data: usize, language: Language) -> String {
    match row.kind {
        Kind::Submenu => choices(row.id)
            .get(data)
            .map_or(String::new(), |current| label_in(current, language).into()),
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
// The type of the Settings window (Mickaël, 2026-10-30): group titles semibold.
const TEXT_SIZE: f64 = 14.;
const TITLE_SIZE: f64 = 12.;
const SWITCH: Point = Point { x: 28., y: 14. };
/// A listening row's waiting box at its narrowest, its square buttons and the gaps between them.
const EDIT_SLOT: f64 = 140.;
const EDIT_BUTTON: f64 = 24.;
const EDIT_GAP: f64 = 4.;
/// Between the label and the waiting box.
const EDIT_SPACE: f64 = 8.;
/// Between the undo button and the keys.
const UNDO_GAP: f64 = 6.;
/// Room for the submenu arrow.
const ARROW: f64 = 14.;

/// The size of a row's note and the room it adds under the label.
const NOTE_SIZE: f64 = 12.;
const NOTE_HEIGHT: f64 = 16.;
fn height(row: &Row) -> f64 {
    let base = match row.kind {
        Kind::Title => 28.,
        Kind::Separator => 9.,
        _ => 32.,
    };
    base + if row.note.is_some() { NOTE_HEIGHT } else { 0. }
}
/// Group titles are the only text at `TITLE_SIZE`, and the only semibold one.
fn style(size: f64) -> TextStyle {
    if size == TITLE_SIZE {
        TextStyle::Strong
    } else {
        TextStyle::Body
    }
}
/// Logical width a row needs to show its label and its right-hand side without clipping.
fn width(renderer: &Renderer, row: &Row, data: usize) -> Result<f64> {
    width_in(renderer, row, data, language())
}
/// `width` for the texts of `language`.
fn width_in(renderer: &Renderer, row: &Row, data: usize, language: Language) -> Result<f64> {
    let measure = |text: &str, size: f64| {
        Ok::<_, Error>(
            renderer
                .measure_styled_text_in(text, size as f32, style(size), language)?
                .x,
        )
    };
    let name = label_in(row, language);
    let note = match row.note {
        Some(key) => {
            (if row.kind == Kind::Title {
                ICON_X
            } else {
                LABEL_X
            }) + renderer
                .measure_styled_text_in(
                    tr_in(language, key),
                    NOTE_SIZE as f32,
                    TextStyle::Body,
                    language,
                )?
                .x
                + MARGIN
        }
        None => 0.,
    };
    Ok(note.max(match row.kind {
        Kind::Separator => 0.,
        Kind::Title => ICON_X + measure(&name, TITLE_SIZE)? + MARGIN,
        Kind::Toggle => LABEL_X + measure(&name, TEXT_SIZE)? + 2. * MARGIN + SWITCH.x + MARGIN,
        Kind::Choice => LABEL_X + measure(&name, TEXT_SIZE)? + 2. * MARGIN,
        Kind::Command => {
            let caps = caps_width_in(renderer, data, language)?;
            let mut right = if caps == 0. { 0. } else { 2. * MARGIN + caps };
            if shortcut_row(row.id).is_some() {
                // Room for the undo button beside the keys, and for the row listening, so that
                // neither moves nor resizes the menu.
                let placeholder = measure(
                    tr_in(language, Key::MenuPressKeys),
                    settings_panel::SMALL_SIZE,
                )?;
                right = (right + EDIT_BUTTON + UNDO_GAP)
                    .max(EDIT_SPACE + edit_width(slot_width(placeholder)));
            }
            LABEL_X + measure(&name, TEXT_SIZE)? + right + MARGIN
        }
        Kind::Submenu => {
            let detail = detail_in(row, data, language);
            let right = if detail.is_empty() {
                0.
            } else {
                2. * MARGIN + measure(&detail, TEXT_SIZE)?
            };
            LABEL_X + measure(&name, TEXT_SIZE)? + right + ARROW + MARGIN
        }
    }))
}

/// The logical width of the keycaps of shortcut item data `data`; 0 without keys.
fn caps_width_in(renderer: &Renderer, data: usize, language: Language) -> Result<f64> {
    let (caps, _) = caps_in(data, language);
    if caps.is_empty() {
        return Ok(0.);
    }
    let mut widths = Vec::with_capacity(caps.len());
    for cap in &caps {
        widths.push(
            renderer
                .measure_styled_text_in(
                    cap,
                    settings_panel::SMALL_SIZE as f32,
                    TextStyle::Body,
                    language,
                )?
                .x,
        );
    }
    Ok(settings_panel::keycaps_width(widths))
}
/// The waiting box of a listening row, wide enough for its placeholder measuring `placeholder`.
fn slot_width(placeholder: f64) -> f64 {
    // The placeholder, the caret after it and some air on both sides.
    EDIT_SLOT.max(placeholder + 32.)
}
/// The waiting box and both buttons of a listening row, side by side.
fn edit_width(slot: f64) -> f64 {
    slot + 2. * EDIT_GAP + 2. * EDIT_BUTTON
}
fn area(left: f64, top: f64, right: f64, bottom: f64) -> Rect {
    Rect {
        min: Point { x: left, y: top },
        max: Point {
            x: right,
            y: bottom,
        },
    }
}
fn scaled(r: Rect, scale: f64) -> Rect {
    Rect {
        min: r.min * scale,
        max: r.max * scale,
    }
}
/// The waiting box, the default button and the cancel button of a listening row `size`
/// logical pixels large, in its own logical pixels: they end at the row's right margin.
fn edit_parts(size: Point, slot: f64) -> [Rect; 3] {
    let right = size.x - MARGIN;
    let (top, bottom) = (
        size.y / 2. - EDIT_BUTTON / 2.,
        size.y / 2. + EDIT_BUTTON / 2.,
    );
    let cancel = area(right - EDIT_BUTTON, top, right, bottom);
    let default = area(
        cancel.min.x - EDIT_GAP - EDIT_BUTTON,
        top,
        cancel.min.x - EDIT_GAP,
        bottom,
    );
    let waiting = area(
        default.min.x - EDIT_GAP - slot,
        top,
        default.min.x - EDIT_GAP,
        bottom,
    );
    [waiting, default, cancel]
}
/// Where the keys zone of a shortcut row `width` logical pixels wide starts, its keys
/// measuring `caps`: the keys and a little air around them. Left of it is the label zone.
fn keys_left(width: f64, caps: f64) -> f64 {
    width - MARGIN - caps - EDIT_SPACE
}
/// The undo button left of keys `caps` logical pixels wide, in the row's logical pixels.
fn undo_part(size: Point, caps: f64) -> Rect {
    let right = size.x - MARGIN - caps - UNDO_GAP;
    area(
        right - EDIT_BUTTON,
        size.y / 2. - EDIT_BUTTON / 2.,
        right,
        size.y / 2. + EDIT_BUTTON / 2.,
    )
}

/// How a row looks beside the others.
#[derive(Clone, Copy, Default)]
enum Look<'a> {
    #[default]
    Plain,
    /// Its shortcut just changed: an undo button gives the keys from before back.
    Undo,
    /// Another row listens.
    Dimmed,
    /// It listens for its new keys.
    Editing(&'a RowEdit),
}

/// The part of a selected row the highlight covers: the pointer on a shortcut row lights its
/// label or its keys, which do different things; the keyboard lights the whole row.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
enum Zone {
    #[default]
    Row,
    Label,
    Keys,
}

#[derive(Clone, Copy, Default)]
struct State<'a> {
    selected: bool,
    /// The part the highlight of a selected row covers.
    zone: Zone,
    checked: bool,
    data: usize,
    look: Look<'a>,
}

/// The glyph `row` shows: Show board's eye is struck through while the board is hidden.
fn shown_icon(row: &Row, state: State) -> Icon {
    match row.icon {
        Icon::Eye if !state.checked => Icon::EyeOff,
        icon => icon,
    }
}

/// One row filling `size` device pixels, at `scale` device pixels per logical pixel.
fn paint<C: Canvas>(
    canvas: &mut C,
    row: &Row,
    state: State,
    size: Point,
    scale: f64,
) -> std::result::Result<(), C::Error> {
    if let Look::Editing(edit) = state.look {
        return paint_editing(canvas, row, edit, size, scale);
    }
    paint_row(canvas, row, state, size, scale)?;
    if let Look::Dimmed = state.look {
        canvas.fill_rect(
            Rect {
                min: Point::default(),
                max: size,
            },
            Ink::with_alpha(BACKGROUND, 0.6),
        )?;
    }
    Ok(())
}
/// A row listening for its new keys: tinted and outlined, its keys replaced by the waiting box
/// and its buttons; a refusal replaces its label, there being no room for a second line.
fn paint_editing<C: Canvas>(
    canvas: &mut C,
    row: &Row,
    edit: &RowEdit,
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
    let refused = !edit.error.is_empty();
    let edge = if refused {
        settings_panel::ERROR
    } else {
        settings_panel::ACCENT
    };
    let band = area(u(4.), u(1.), size.x - u(4.), size.y - u(1.));
    canvas.fill_round_rect(band, u(4.), Ink::solid(settings_panel::EDITING))?;
    canvas.stroke_round_rect(band.expanded(-u(0.75)), u(4.), Ink::solid(edge), u(1.5))?;
    let middle = size.y / 2.;
    draw_icon(
        canvas,
        shown_icon(row, State::default()),
        Point {
            x: u(ICON_X),
            y: middle - u(10. * ICON_SCALE),
        },
        u(ICON_SCALE),
        SECONDARY,
    )?;
    let placeholder = tr_in(language(), Key::MenuPressKeys);
    let extent = canvas
        .measure_styled_text(placeholder, u(settings_panel::SMALL_SIZE), TextStyle::Body)?
        .x
        / scale;
    let logical = Point {
        x: size.x / scale,
        y: size.y / scale,
    };
    let parts = edit_parts(logical, slot_width(extent)).map(|r| scaled(r, scale));
    let (text, ink) = if refused {
        (Cow::Borrowed(edit.error.as_str()), settings_panel::ERROR)
    } else {
        (label(row), TEXT)
    };
    let line = canvas
        .measure_styled_text("X", u(TEXT_SIZE), TextStyle::Body)?
        .y;
    canvas.styled_text(
        &text,
        area(
            u(LABEL_X),
            middle - line / 2.,
            parts[0].min.x - u(EDIT_SPACE),
            middle + line / 2.,
        ),
        u(TEXT_SIZE),
        Ink::solid(ink),
        TextStyle::Body,
    )?;
    settings_panel::waiting_keys(canvas, parts, &edit.held, Some(placeholder), edge, scale)
}
fn paint_row<C: Canvas>(
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
    // A note takes the bottom of the row; the label and its icon centre on the rest.
    let note_room = if row.note.is_some() {
        u(NOTE_HEIGHT)
    } else {
        0.
    };
    let middle = (size.y - note_room) / 2.;
    if let Some(key) = row.note {
        let left = u(if row.kind == Kind::Title {
            ICON_X
        } else {
            LABEL_X
        });
        let line = canvas
            .measure_styled_text("X", u(NOTE_SIZE), TextStyle::Body)?
            .y;
        let bottom = size.y - u(if row.kind == Kind::Title { 3. } else { 7. });
        canvas.styled_text(
            tr_in(language(), key),
            Rect {
                min: Point {
                    x: left,
                    y: bottom - line,
                },
                max: Point {
                    x: size.x,
                    y: bottom,
                },
            },
            u(NOTE_SIZE),
            Ink::solid(MUTED),
            TextStyle::Body,
        )?;
    }
    let text = |canvas: &mut C, text: &str, left: f64, right: f64, bottom: f64, size: f64, rgb| {
        let line = canvas.measure_styled_text("X", u(size), style(size))?.y;
        canvas.styled_text(
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
            style(size),
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
                size.y - u(3.) - note_room,
                TITLE_SIZE,
                MUTED,
            );
        }
        _ => {}
    }
    // A shortcut row has two zones: its label runs the command, its keys change them.
    let (caps, in_use) = caps_in(state.data, language());
    let caps_width = if caps.is_empty() {
        0.
    } else {
        let mut widths = Vec::with_capacity(caps.len());
        for cap in &caps {
            widths.push(
                canvas
                    .measure_styled_text(cap, u(settings_panel::SMALL_SIZE), TextStyle::Body)?
                    .x
                    / scale,
            );
        }
        settings_panel::keycaps_width(widths)
    };
    let zoned = shortcut_row(row.id).is_some();
    let split = keys_left(size.x / scale, caps_width);
    if state.selected && zoned && state.zone == Zone::Keys {
        // Only the keys light up, framed in the accent.
        let frame = area(
            size.x - u(MARGIN + caps_width + 4.),
            middle - u(13.),
            size.x - u(MARGIN - 4.),
            middle + u(13.),
        );
        canvas.stroke_round_rect(frame.expanded(-u(0.75)), u(6.), Ink::solid(ACCENT), u(1.5))?;
    } else if state.selected {
        let right = if zoned && state.zone == Zone::Label {
            u(split)
        } else {
            size.x - u(4.)
        };
        canvas.fill_round_rect(
            area(u(4.), u(1.), right, size.y - u(1.)),
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
            draw_icon(canvas, Icon::Check, icon_origin, u(ICON_SCALE), TEXT)?;
        }
    } else {
        draw_icon(
            canvas,
            shown_icon(row, state),
            icon_origin,
            u(ICON_SCALE),
            SECONDARY,
        )?;
    }
    let line = canvas
        .measure_styled_text("X", u(TEXT_SIZE), TextStyle::Body)?
        .y;
    let bottom = middle + line / 2.;
    text(canvas, &name, u(LABEL_X), size.x, bottom, TEXT_SIZE, TEXT)?;
    let mut right = size.x - u(MARGIN);
    if row.kind == Kind::Submenu {
        draw_icon(
            canvas,
            Icon::Chevron,
            Point {
                x: right - u(8.),
                y: middle - u(6.),
            },
            u(0.6),
            SECONDARY,
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
    if row.kind == Kind::Command {
        if let Look::Undo = state.look {
            let width = caps_width;
            let logical = Point {
                x: size.x / scale,
                y: size.y / scale,
            };
            let undo = scaled(undo_part(logical, width), scale);
            settings_panel::square_button(canvas, undo, scale)?;
            settings_panel::restore_glyph(canvas, undo.center(), SECONDARY, scale)?;
        }
        return settings_panel::keycaps(canvas, &caps, right, middle, 9., scale, in_use);
    }
    let detail = detail_in(row, state.data, language());
    if !detail.is_empty() {
        let extent = canvas
            .measure_styled_text(&detail, u(TEXT_SIZE), TextStyle::Body)?
            .x;
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
/// `view` is a shortcut row changing in place.
fn render(
    values: &Values,
    selected: u32,
    open: u32,
    view: &popup::View,
    scale: f64,
) -> Result<Image> {
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
            root.paint(
                dc,
                POINT::default(),
                values,
                position(selected),
                view,
                scale,
            )?;
            beside.paint(
                dc,
                POINT {
                    x: root.width,
                    y: hang,
                },
                values,
                None,
                &popup::View::default(),
                scale,
            )
        },
    )
}
fn sample() -> Values {
    let defaults = Settings::default();
    Values {
        capture: shortcut(
            Shortcut {
                modifiers: 0,
                key: crate::print_screen::KEY,
            },
            false,
        ),
        instant: shortcut(defaults.instant_shortcut, false),
        halo: true,
        loupe: false,
        temporary_shapes: true,
        shapes: defaults
            .presentation
            .shape_shortcuts
            .map(|keys| shortcut(keys, false)),
        keys: false,
        mouse: false,
        board: defaults.board_visible,
        board_shortcut: shortcut(defaults.board_shortcut, false),
        language: defaults.language,
    }
}
pub(super) fn preview() -> Result<()> {
    save_png(
        &render(&sample(), 170, 0, &popup::View::default(), 1.)?,
        std::path::Path::new("target/shotoshot-menu.png"),
    )?;
    // The arrow row listening with Ctrl held; the rectangle row offers its keys from before.
    let editing = popup::View {
        editing: Some(RowEdit {
            row: ShortcutRow::Arrow,
            held: vec![tr_in(Language::En, Key::KeyCtrl).into()],
            error: String::new(),
        }),
        ..Default::default()
    };
    save_png(
        &render(&sample(), 0, 0, &editing, 1.)?,
        std::path::Path::new("target/shotoshot-menu-editing.png"),
    )?;
    let saved = popup::View {
        before: Some(172),
        ..Default::default()
    };
    // The pointer on the keys of Edit board: they light up alone, framed in the accent.
    let keys = popup::View {
        pointer: Some((8, true)),
        ..Default::default()
    };
    save_png(
        &render(&sample(), 8, 0, &keys, 1.)?,
        std::path::Path::new("target/shotoshot-menu-hover-keys.png"),
    )?;
    save_png(
        &render(&sample(), 0, 0, &saved, 1.)?,
        std::path::Path::new("target/shotoshot-menu-saved.png"),
    )
}
/// Opens and closes the menu's renderer the way a real menu does, for the resident diagnostic.
pub(super) fn probe() -> Result<()> {
    render(&sample(), 0, 150, &popup::View::default(), 1.).map(|_| ())
}

#[cfg(test)]
mod tests;
