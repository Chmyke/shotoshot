//! ROLE: The shortcut window shared by Windows and Linux: its zones, the names of its keys and
//! how it is painted. Each platform records the keys, measures the texts and shows the window.
//! INVARIANTS: Layout is in 96-DPI logical pixels, scaled once at paint time. Painting and
//! clicks read the same `Layout`; English keeps the sizes the window had before translation.
use crate::canvas::{Canvas, Ink};
use crate::icon::{self, Icon};
use shotoshot_core::{Point, Rect};
use shotoshot_text::{Key, Language, Text, tr_in};

pub const fn area(left: f64, top: f64, right: f64, bottom: f64) -> Rect {
    Rect {
        min: Point { x: left, y: top },
        max: Point {
            x: right,
            y: bottom,
        },
    }
}
pub const ICON: Point = Point { x: 20., y: 22. };
pub const TITLE_X: f64 = 46.;
/// Between the window's edges and its field, hint and buttons.
pub const MARGIN: f64 = 20.;
/// The English window: every other language keeps at least this size.
pub const MIN_WIDTH: f64 = 420.;
const FIELD_TOP: f64 = 58.;
const FIELD_HEIGHT: f64 = 64.;
const HINT_GAP: f64 = 10.;
const MIN_HINT_HEIGHT: f64 = 52.;
const BUTTONS_GAP: f64 = 12.;
const BUTTON_HEIGHT: f64 = 32.;
const MIN_BUTTON_WIDTH: f64 = 88.;
/// Room kept between a button's edge and its label.
pub const BUTTON_PADDING: f64 = 16.;
const BUTTON_SPACING: f64 = 8.;

/// Where the field, the hint and the buttons sit, in logical pixels at scale 1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Layout {
    /// Client area, in whole pixels.
    pub size: Point,
    pub field: Rect,
    pub hint: Rect,
    pub cancel: Rect,
    pub save: Rect,
}
impl Layout {
    /// The window as English draws it, and every language with texts no longer than English.
    pub const ENGLISH: Self = Self {
        size: Point { x: 420., y: 248. },
        field: area(20., 58., 400., 122.),
        hint: area(20., 132., 400., 184.),
        cancel: area(216., 196., 304., 228.),
        save: area(312., 196., 400., 228.),
    };
    /// The window for texts of `widths`, whose tallest hint is `hint` high once wrapped at the
    /// hint width of `widths.window()`. Each zone keeps its English size unless a text needs more.
    pub fn fit(widths: Widths, hint: f64) -> Self {
        let width = widths.window();
        let right = width - MARGIN;
        let field = area(MARGIN, FIELD_TOP, right, FIELD_TOP + FIELD_HEIGHT);
        let hint_top = field.max.y + HINT_GAP;
        let hint = area(
            MARGIN,
            hint_top,
            right,
            hint_top + hint.ceil().max(MIN_HINT_HEIGHT),
        );
        let (top, button) = (hint.max.y + BUTTONS_GAP, button_width(widths.button));
        let save = area(right - button, top, right, top + BUTTON_HEIGHT);
        let cancel_right = save.min.x - BUTTON_SPACING;
        let cancel = area(cancel_right - button, top, cancel_right, save.max.y);
        Self {
            size: Point {
                x: width,
                y: save.max.y + MARGIN,
            },
            field,
            hint,
            cancel,
            save,
        }
    }
    /// The action under a logical point: 0 the shortcut field, 1 Cancel, 2 Save.
    pub fn hit(&self, x: f64, y: f64) -> Option<usize> {
        [self.field, self.cancel, self.save]
            .iter()
            .position(|zone| zone.contains(Point { x, y }))
    }
    /// The window that fits `titles`, `hints` and the keys of `language`. `width` measures one
    /// line, `wrapped` the height of a text broken at a width, both in logical pixels.
    pub fn measured<E>(
        language: Language,
        titles: &[&str],
        hints: &[String],
        name: &dyn Fn(u32) -> Option<String>,
        width: &mut dyn FnMut(&str, f64) -> Result<f64, E>,
        wrapped: &mut dyn FnMut(&str, f64, f64) -> Result<f64, E>,
    ) -> Result<Self, E> {
        let mut widths = Widths {
            keys: keys_width(language, name, width)?,
            ..Default::default()
        };
        for title in titles {
            widths.title = widths.title.max(width(title, TITLE_SIZE)?);
        }
        for key in [Key::SetupCancel, Key::SetupSave] {
            widths.button = widths.button.max(width(tr_in(language, key), TEXT_SIZE)?);
        }
        let wrap = widths.window() - 2. * MARGIN;
        let mut hint: f64 = 0.;
        for text in hints {
            hint = hint.max(wrapped(text, HINT_SIZE, wrap)?);
        }
        Ok(Self::fit(widths, hint))
    }
}
impl Default for Layout {
    fn default() -> Self {
        Self::ENGLISH
    }
}

/// The widest texts of one language, in logical pixels at scale 1: a title, a full row of
/// keycaps or a prompt of the field, a button label.
#[derive(Clone, Copy, Default)]
pub struct Widths {
    pub title: f64,
    pub keys: f64,
    pub button: f64,
}
impl Widths {
    /// The window width these texts need, never below the English one.
    pub fn window(self) -> f64 {
        [
            MIN_WIDTH,
            TITLE_X + self.title + MARGIN,
            2. * (MARGIN + FIELD_MARGIN) + self.keys,
            2. * MARGIN + 2. * button_width(self.button) + BUTTON_SPACING,
        ]
        .into_iter()
        .fold(0., f64::max)
        .ceil()
    }
}
fn button_width(label: f64) -> f64 {
    (label + 2. * BUTTON_PADDING).ceil().max(MIN_BUTTON_WIDTH)
}

/// The widest content of the field in `language`: every modifier with the longest key name the
/// interface writes, or a prompt.
pub fn keys_width<E>(
    language: Language,
    name: &dyn Fn(u32) -> Option<String>,
    width: &mut dyn FnMut(&str, f64) -> Result<f64, E>,
) -> Result<f64, E> {
    let mut longest_key: f64 = 0.;
    let fallback = code_name(0xFF, language);
    for label in (0..=255)
        .filter_map(|key| table_name(key, language).or_else(|| name(key)))
        .chain([fallback])
    {
        longest_key = longest_key.max(cap_width(width(&label, TEXT_SIZE)?, 1.));
    }
    let mut keys = longest_key;
    for modifier in modifier_labels_in(15, language) {
        keys += cap_width(width(modifier, TEXT_SIZE)?, 1.) + GAP;
    }
    let mut widest = keys;
    for prompt in [Key::SetupPress, Key::SetupChoose] {
        widest = widest.max(width(tr_in(language, prompt), TEXT_SIZE)?.ceil());
    }
    Ok(widest)
}

/// Modifier bits as Windows `RegisterHotKey` numbers them, the settings format on every system.
pub const ALT: u32 = 1;
pub const CTRL: u32 = 2;
pub const SHIFT: u32 = 4;
/// Windows on Windows, Super on Linux.
pub const LOGO: u32 = 8;

/// The names of the modifiers in `mods`, in the order the field writes them.
pub fn modifier_labels_in(mods: u32, language: Language) -> impl Iterator<Item = &'static str> {
    [
        (CTRL, Key::KeyCtrl),
        (SHIFT, Key::KeyShift),
        (ALT, Key::KeyAlt),
        (LOGO, Key::KeyWin),
    ]
    .into_iter()
    .filter(move |(mask, _)| mods & mask != 0)
    .map(move |(_, key)| tr_in(language, key))
}
/// The name the interface writes for `key` in `language`, or `None` for a key the keyboard
/// layout names (letters, digits, punctuation).
pub fn table_name(key: u32, language: Language) -> Option<String> {
    let named = |name| Some(tr_in(language, name).to_string());
    let numpad = |symbol: &str| {
        Some(Text::with_in(language, Key::KeyNumpad, &[("key", symbol)]).to_string())
    };
    match key {
        8 => named(Key::KeyBackspace),
        9 => named(Key::KeyTab),
        13 => named(Key::KeyEnter),
        20 => named(Key::KeyCapsLock),
        27 => named(Key::KeyEsc),
        32 => named(Key::KeySpace),
        33 => named(Key::KeyPageUp),
        34 => named(Key::KeyPageDown),
        35 => named(Key::KeyEnd),
        36 => named(Key::KeyHome),
        37 => named(Key::KeyLeft),
        38 => named(Key::KeyUp),
        39 => named(Key::KeyRight),
        40 => named(Key::KeyDown),
        45 => named(Key::KeyInsert),
        46 => named(Key::KeyDelete),
        96..=105 => numpad(&(key - 96).to_string()),
        106 => numpad("*"),
        107 => numpad("+"),
        109 => numpad("-"),
        110 => numpad("."),
        111 => numpad("/"),
        112..=135 => Some(format!("F{}", key - 111)),
        144 => named(Key::KeyNumLock),
        145 => named(Key::KeyScrollLock),
        19 => named(Key::KeyPause),
        3 => named(Key::KeyBreak),
        44 => named(Key::KeyPrintScreen),
        _ => None,
    }
}
/// A key nobody names, by its virtual-key code.
pub fn code_name(key: u32, language: Language) -> String {
    Text::with_in(language, Key::KeyCode, &[("code", &format!("{key:02X}"))]).to_string()
}
/// Modifiers then the key: one entry per keycap. `name` gives the layout's name of a key the
/// table does not name.
pub fn key_parts_in(
    mods: u32,
    key: u32,
    language: Language,
    name: &dyn Fn(u32) -> Option<String>,
) -> Vec<String> {
    let mut parts: Vec<String> = modifier_labels_in(mods, language)
        .map(String::from)
        .collect();
    parts.push(
        table_name(key, language)
            .or_else(|| name(key))
            .unwrap_or_else(|| code_name(key, language)),
    );
    parts
}

pub const BACKGROUND: [u8; 3] = [0x20, 0x20, 0x20];
/// The shortcut field and the Cancel button.
pub const SURFACE: [u8; 3] = [0x2b, 0x2b, 0x2b];
pub const KEYCAP: [u8; 3] = [0x3a, 0x3a, 0x3a];
pub const KEYCAP_EDGE: [u8; 3] = [0x56, 0x56, 0x56];
pub const DISABLED: [u8; 3] = [0x33, 0x33, 0x33];
pub const ERROR: [u8; 3] = [0xf8, 0x51, 0x49];
/// The colours of the Windows tray menu, which the window shares.
pub const TEXT: [u8; 3] = [0xf0, 0xf0, 0xf0];
pub const SECONDARY: [u8; 3] = [0xc6, 0xc6, 0xc6];
pub const MUTED: [u8; 3] = [0x9a, 0x9a, 0x9a];
pub const RULE: [u8; 3] = [0x49, 0x49, 0x49];
pub const ACCENT: [u8; 3] = [0x60, 0xcd, 0xff];
/// Toolbar glyphs sit on a 20 px grid; the title shows its icon at 16 px.
const ICON_SCALE: f64 = 0.8;
pub const TITLE_SIZE: f64 = 16.;
pub const TEXT_SIZE: f64 = 13.;
pub const HINT_SIZE: f64 = 12.;
pub const CAP_HEIGHT: f64 = 28.;
const CAP_MARGIN: f64 = 10.;
const GAP: f64 = 6.;
/// Room kept between the field's edge and its keys.
pub const FIELD_MARGIN: f64 = 16.;
const RADIUS: f64 = 5.;

/// Width of a keycap whose label is `label` wide, at `scale`.
pub fn cap_width(label: f64, scale: f64) -> f64 {
    (label + 2. * CAP_MARGIN * scale)
        .max(CAP_HEIGHT * scale)
        .ceil()
}

/// What the shortcut field shows, left to right.
pub enum Piece {
    Cap(String),
    Plus,
    Prompt(Key),
}
fn caps(labels: impl Iterator<Item = String>) -> Vec<Piece> {
    let mut pieces = Vec::new();
    for label in labels {
        if !pieces.is_empty() {
            pieces.push(Piece::Plus);
        }
        pieces.push(Piece::Cap(label));
    }
    pieces
}
/// The field while `recording` with `held` modifiers down, or showing `pending`.
pub fn pieces(
    recording: bool,
    held: u32,
    pending: Option<(u32, u32)>,
    language: Language,
    name: &dyn Fn(u32) -> Option<String>,
) -> Vec<Piece> {
    if recording {
        let mut held = caps(modifier_labels_in(held, language).map(String::from));
        if held.is_empty() {
            return vec![Piece::Prompt(Key::SetupPress)];
        }
        held.extend([Piece::Plus, Piece::Prompt(Key::SetupWaiting)]);
        held
    } else if let Some((mods, key)) = pending {
        caps(key_parts_in(mods, key, language, name).into_iter())
    } else {
        vec![Piece::Prompt(Key::SetupChoose)]
    }
}

/// Everything one frame of the window shows.
pub struct View<'a> {
    pub language: Language,
    pub layout: Layout,
    pub icon: Icon,
    pub title: &'a str,
    pub pieces: Vec<Piece>,
    pub hint: &'a str,
    /// The hint is a refusal, in red.
    pub error: bool,
    /// The field is listening or focused: framed in the accent colour.
    pub field_framed: bool,
    /// 0 the field, 1 Cancel, 2 Save.
    pub focus: usize,
    pub save_enabled: bool,
}

/// One line of text centred in `r`.
fn centered<C: Canvas>(
    canvas: &mut C,
    text: &str,
    r: Rect,
    size: f64,
    rgb: [u8; 3],
) -> Result<(), C::Error> {
    let extent = canvas.measure_text(text, size)?;
    let min = r.center() - extent * 0.5;
    canvas.text(
        text,
        Rect {
            min,
            // Measuring and drawing round differently: never wrap or clip the last glyph.
            max: min + extent + Point { x: size, y: 0. },
        },
        size,
        Ink::solid(rgb),
    )
}
fn button<C: Canvas>(
    canvas: &mut C,
    r: Rect,
    scale: f64,
    label: &str,
    [fill, edge, ink]: [[u8; 3]; 3],
) -> Result<(), C::Error> {
    canvas.fill_round_rect(r, RADIUS * scale, Ink::solid(fill))?;
    canvas.stroke_round_rect(
        r.expanded(-0.5 * scale),
        RADIUS * scale,
        Ink::solid(edge),
        scale,
    )?;
    centered(canvas, label, r, TEXT_SIZE * scale, ink)
}

/// The whole window, at `scale` device pixels per logical pixel.
pub fn paint<C: Canvas>(canvas: &mut C, ui: View, scale: f64) -> Result<(), C::Error> {
    let zones = ui.layout;
    let text = |key| tr_in(ui.language, key);
    let u = |value: f64| value * scale;
    let px = |r: Rect| Rect {
        min: r.min * scale,
        max: r.max * scale,
    };
    canvas.fill_rect(
        px(area(0., 0., zones.size.x, zones.size.y)),
        Ink::solid(BACKGROUND),
    )?;
    icon::draw(canvas, ui.icon, ICON * scale, u(ICON_SCALE), SECONDARY)?;
    let title = canvas.measure_text(ui.title, u(TITLE_SIZE))?;
    let title_top = u(ICON.y + 10. * ICON_SCALE) - title.y / 2.;
    canvas.text(
        ui.title,
        Rect {
            min: Point {
                x: u(TITLE_X),
                y: title_top,
            },
            max: Point {
                x: u(zones.size.x),
                y: title_top + title.y,
            },
        },
        u(TITLE_SIZE),
        Ink::solid(TEXT),
    )?;

    let field = px(zones.field);
    canvas.fill_round_rect(field, u(RADIUS + 1.), Ink::solid(SURFACE))?;
    canvas.stroke_round_rect(
        field.expanded(-u(0.5)),
        u(RADIUS + 1.),
        Ink::solid(if ui.field_framed { ACCENT } else { RULE }),
        u(1.),
    )?;
    let mut slots = Vec::new();
    for piece in ui.pieces {
        let width = match &piece {
            Piece::Cap(label) => cap_width(canvas.measure_text(label, u(TEXT_SIZE))?.x, scale),
            Piece::Plus => canvas.measure_text("+", u(TEXT_SIZE))?.x.ceil(),
            Piece::Prompt(key) => canvas.measure_text(text(*key), u(TEXT_SIZE))?.x.ceil(),
        };
        slots.push((piece, width));
    }
    let span = |slots: &[(Piece, f64)]| {
        slots.iter().map(|(_, width)| width).sum::<f64>()
            + u(GAP) * slots.len().saturating_sub(1) as f64
    };
    if span(&slots) > field.size().x - u(2. * FIELD_MARGIN) {
        // A shortcut too long for the field keeps its keys and gives up the separators.
        slots.retain(|(piece, _)| !matches!(piece, Piece::Plus));
    }
    let total = span(&slots);
    let middle = field.center();
    let mut left = (middle.x - total / 2.).round();
    for (piece, width) in slots {
        let slot = Rect {
            min: Point {
                x: left,
                y: middle.y - u(CAP_HEIGHT / 2.),
            },
            max: Point {
                x: left + width,
                y: middle.y + u(CAP_HEIGHT / 2.),
            },
        };
        match piece {
            Piece::Cap(label) => button(canvas, slot, scale, &label, [KEYCAP, KEYCAP_EDGE, TEXT])?,
            Piece::Plus => centered(canvas, "+", slot, u(TEXT_SIZE), MUTED)?,
            Piece::Prompt(key) => centered(canvas, text(key), slot, u(TEXT_SIZE), SECONDARY)?,
        }
        left += width + u(GAP);
    }

    canvas.text(
        ui.hint,
        px(zones.hint),
        u(HINT_SIZE),
        Ink::solid(if ui.error { ERROR } else { MUTED }),
    )?;
    button(
        canvas,
        px(zones.cancel),
        scale,
        text(Key::SetupCancel),
        [SURFACE, if ui.focus == 1 { ACCENT } else { RULE }, TEXT],
    )?;
    let fill = if ui.save_enabled { ACCENT } else { DISABLED };
    button(
        canvas,
        px(zones.save),
        scale,
        text(Key::SetupSave),
        [
            fill,
            if ui.focus == 2 { TEXT } else { fill },
            if fill == ACCENT { BACKGROUND } else { MUTED },
        ],
    )
}
