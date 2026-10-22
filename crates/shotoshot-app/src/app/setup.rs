//! ROLE: The shortcut window: what it records, how it is painted and where it is clicked.
//! OWNER: App holds one `Setup` per opening; its painter lives only while the window is shown.
//! INVARIANTS: Layout is in 96-DPI logical pixels, scaled once at paint time. Painting and
//! clicks read the same `SetupLayout`; English keeps the sizes the window had before translation.
use super::*;
use crate::render::{D2dCanvas, Renderer};
use shotoshot_core::Rect;
use shotoshot_text::{Key, Language, Text, tr_in};
use shotoshot_ui::{Canvas, Ink};
use tray_menu::{ACCENT, MUTED, RULE, SECONDARY, TEXT};

const fn area(left: f64, top: f64, right: f64, bottom: f64) -> Rect {
    Rect {
        min: Point { x: left, y: top },
        max: Point {
            x: right,
            y: bottom,
        },
    }
}
const ICON: Point = Point { x: 20., y: 22. };
const TITLE_X: f64 = 46.;
/// Between the window's edges and its field, hint and buttons.
const MARGIN: f64 = 20.;
/// The English window: every other language keeps at least this size.
const MIN_WIDTH: f64 = 420.;
const FIELD_TOP: f64 = 58.;
const FIELD_HEIGHT: f64 = 64.;
const HINT_GAP: f64 = 10.;
const MIN_HINT_HEIGHT: f64 = 52.;
const BUTTONS_GAP: f64 = 12.;
const BUTTON_HEIGHT: f64 = 32.;
const MIN_BUTTON_WIDTH: f64 = 88.;
/// Room kept between a button's edge and its label.
const BUTTON_PADDING: f64 = 16.;
const BUTTON_SPACING: f64 = 8.;

/// Where the field, the hint and the buttons sit, in logical pixels at scale 1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct SetupLayout {
    /// Client area, in whole pixels.
    pub size: Point,
    field: Rect,
    hint: Rect,
    cancel: Rect,
    save: Rect,
}
impl SetupLayout {
    /// The window as English draws it, and every language with texts no longer than English.
    const ENGLISH: Self = Self {
        size: Point { x: 420., y: 248. },
        field: area(20., 58., 400., 122.),
        hint: area(20., 132., 400., 184.),
        cancel: area(216., 196., 304., 228.),
        save: area(312., 196., 400., 228.),
    };
    /// The window for texts of `widths`, whose tallest hint is `hint` high once wrapped at the
    /// hint width of `widths.window()`. Each zone keeps its English size unless a text needs more.
    fn fit(widths: Widths, hint: f64) -> Self {
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
}
impl Default for SetupLayout {
    fn default() -> Self {
        Self::ENGLISH
    }
}

/// The widest texts of one language, in logical pixels at scale 1: a title, a full row of
/// keycaps or a prompt of the field, a button label.
#[derive(Clone, Copy, Default)]
struct Widths {
    title: f64,
    keys: f64,
    button: f64,
}
impl Widths {
    /// The window width these texts need, never below the English one.
    fn window(self) -> f64 {
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

/// The window sized for the texts of `language`, as `renderer` measures them.
pub(super) fn layout(renderer: &Renderer, language: Language) -> Result<SetupLayout> {
    let width = |key, size: f64| -> Result<f64> {
        Ok(renderer
            .measure_text_in(tr_in(language, key), size as f32, language)?
            .x)
    };
    let mut widths = Widths {
        keys: keys_width(renderer, language)?,
        ..Default::default()
    };
    for target in TARGETS {
        widths.title = widths.title.max(width(target.title(), TITLE_SIZE)?);
    }
    for key in [Key::SetupCancel, Key::SetupSave] {
        widths.button = widths.button.max(width(key, TEXT_SIZE)?);
    }
    let wrap = (widths.window() - 2. * MARGIN) as f32;
    let mut hint: f64 = 0.;
    for text in hints_in(language) {
        let wrapped = renderer.measure_wrapped_text_in(&text, HINT_SIZE as f32, wrap, language)?;
        hint = hint.max(wrapped.y);
    }
    Ok(SetupLayout::fit(widths, hint))
}

/// The widest content of the field in `language`: every modifier with the longest key name the
/// interface writes, or a prompt.
fn keys_width(renderer: &Renderer, language: Language) -> Result<f64> {
    let width = |text: &str| {
        Ok::<_, Error>(
            renderer
                .measure_text_in(text, TEXT_SIZE as f32, language)?
                .x,
        )
    };
    let mut longest_key: f64 = 0.;
    let fallback = code_name(0xFF, language);
    for name in (0..=255)
        .filter_map(|key| table_name(key, language))
        .chain([fallback])
    {
        longest_key = longest_key.max(cap_width(width(&name)?, 1.));
    }
    let mut keys = longest_key;
    for modifier in modifier_labels_in(15, language) {
        keys += cap_width(width(modifier)?, 1.) + GAP;
    }
    let mut widest = keys;
    for prompt in [Key::SetupPress, Key::SetupChoose] {
        widest = widest.max(width(tr_in(language, prompt))?.ceil());
    }
    Ok(widest)
}

#[derive(Clone, Copy, Default, PartialEq)]
pub(super) enum ShortcutTarget {
    #[default]
    Capture,
    Instant,
    Arrow,
    Rectangle,
    Ellipse,
    Board,
}
/// Every window the menu opens: the layout fits the title of each.
const TARGETS: [ShortcutTarget; 6] = [
    ShortcutTarget::Capture,
    ShortcutTarget::Instant,
    ShortcutTarget::Arrow,
    ShortcutTarget::Rectangle,
    ShortcutTarget::Ellipse,
    ShortcutTarget::Board,
];
impl ShortcutTarget {
    pub fn shape_index(self) -> Option<usize> {
        match self {
            Self::Capture | Self::Instant | Self::Board => None,
            Self::Arrow => Some(0),
            Self::Rectangle => Some(1),
            Self::Ellipse => Some(2),
        }
    }
    /// The notification menu row that opens this shortcut; the window borrows its icon.
    fn menu_id(self) -> u32 {
        match self {
            Self::Capture => 2,
            Self::Instant => 6,
            Self::Arrow => 110,
            Self::Rectangle => 111,
            Self::Ellipse => 112,
            Self::Board => 13,
        }
    }
    fn title(self) -> Key {
        match self {
            Self::Capture => Key::SetupTitleCapture,
            Self::Instant => Key::SetupTitleInstant,
            Self::Arrow => Key::SetupTitleArrow,
            Self::Rectangle => Key::SetupTitleRectangle,
            Self::Ellipse => Key::SetupTitleEllipse,
            Self::Board => Key::SetupTitleBoard,
        }
    }
}

#[derive(Default)]
pub(super) struct Setup {
    pub target: ShortcutTarget,
    pub pending: Option<(u32, u32)>,
    pub recording: bool,
    pub focus: usize,
    pub error: String,
    /// Modifiers held while recording, shown before their companion key arrives.
    pub held: u32,
    /// The language of this opening: its texts and the layout that fits them.
    pub language: Language,
    pub layout: SetupLayout,
    pub painter: Option<Renderer>,
}
impl Setup {
    /// Opened from the menu: that click already asked for a change, so it listens at once.
    pub fn change(target: ShortcutTarget, current: Option<(u32, u32)>) -> Self {
        Self {
            target,
            pending: current,
            recording: true,
            ..Default::default()
        }
    }
    /// First launch proposes Print Screen; Save confirms it, clicking the field changes it.
    pub fn first_run() -> Self {
        Self {
            pending: Some((0, crate::print_screen::KEY)),
            focus: 2,
            ..Default::default()
        }
    }
    /// Sizes the window for `language` before it is shown. The renderer that measured becomes
    /// the painter, dropped with it when the window closes.
    pub fn fit(&mut self, language: Language) {
        let renderer = match self.painter.take() {
            Some(renderer) => Ok(renderer),
            None => Renderer::software(),
        };
        let measured = renderer.and_then(|renderer| Ok((layout(&renderer, language)?, renderer)));
        self.place(language, measured);
    }
    /// Takes the layout `measured` for `language`. A renderer that could not measure leaves
    /// the English layout: the window opens all the same, and painting makes its own renderer.
    fn place(&mut self, language: Language, measured: Result<(SetupLayout, Renderer)>) {
        self.language = language;
        match measured {
            Ok((layout, renderer)) => {
                self.layout = layout;
                self.painter = Some(renderer);
            }
            Err(_) => self.layout = SetupLayout::default(),
        }
    }
    /// The text of `key` in the language of this opening.
    pub fn text(&self, key: Key) -> String {
        tr_in(self.language, key).into()
    }
    fn hint(&self) -> &str {
        if self.error.is_empty() {
            tr_in(self.language, self.hint_key())
        } else {
            &self.error
        }
    }
    fn hint_key(&self) -> Key {
        let any_key = matches!(
            self.target,
            ShortcutTarget::Capture | ShortcutTarget::Instant
        );
        if self.recording {
            if any_key {
                Key::SetupAnyKeyHint
            } else if self.target == ShortcutTarget::Board {
                Key::SetupBoardHint
            } else {
                Key::SetupShapeHint
            }
        } else if any_key
            && self
                .pending
                .is_some_and(|(m, k)| crate::print_screen::is_bare(m, k))
        {
            Key::SetupPrintScreenHint
        } else {
            Key::SetupChangeHint
        }
    }
    pub fn record(&mut self, mods: u32, key: u32) {
        if [16, 17, 18, 91, 92, 160, 161, 162, 163, 164, 165].contains(&key) {
            return;
        }
        self.pending = Some((mods, key));
        self.recording = false;
        self.focus = 2;
        self.error.clear();
    }
}

/// Every text the hint area can show in `language`, errors included, each with its longest
/// value: the layout makes room for the tallest.
fn hints_in(language: Language) -> Vec<String> {
    use shotoshot_core::settings::{BOARD_SHORTCUT_REFUSALS, SHAPE_SHORTCUT_REFUSALS};
    // A board shortcut is refused alone, a shape shortcut inside `setup.invalid`
    // (`App::save_shape_shortcut`).
    let mut texts: Vec<String> = [
        Key::SetupAnyKeyHint,
        Key::SetupBoardHint,
        Key::SetupShapeHint,
        Key::SetupPrintScreenHint,
        Key::SetupChangeHint,
        Key::SetupUsedForInstant,
        Key::SetupUsedByPresentation,
        Key::SetupUsedByBoard,
        Key::SetupUsedByShotoshot,
        Key::SetupPrintScreenHeld,
    ]
    .into_iter()
    .chain(BOARD_SHORTCUT_REFUSALS)
    .map(|key| tr_in(language, key).to_string())
    .collect();
    // The widest code `{code}` can print.
    let code = i32::MIN.to_string();
    for key in [Key::SetupRegisterFailed, Key::SetupUnavailable] {
        texts.push(Text::with_in(language, key, &[("code", &code)]).to_string());
    }
    for detail in SHAPE_SHORTCUT_REFUSALS
        .into_iter()
        .chain([Key::SetupUsedForCapture, Key::SetupUsedByBoard])
    {
        let detail = tr_in(language, detail);
        texts.push(Text::with_in(language, Key::SetupInvalid, &[("detail", detail)]).to_string());
    }
    texts
}

fn modifier_labels_in(mods: u32, language: Language) -> impl Iterator<Item = &'static str> {
    [
        (2, Key::KeyCtrl),
        (4, Key::KeyShift),
        (1, Key::KeyAlt),
        (8, Key::KeyWin),
    ]
    .into_iter()
    .filter(move |(mask, _)| mods & mask != 0)
    .map(move |(_, key)| tr_in(language, key))
}
/// The name the interface writes for `key` in `language`, or `None` for a key Windows names
/// after the keyboard layout (letters, digits, punctuation).
fn table_name(key: u32, language: Language) -> Option<String> {
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
fn code_name(key: u32, language: Language) -> String {
    Text::with_in(language, Key::KeyCode, &[("code", &format!("{key:02X}"))]).to_string()
}
/// The name Windows gives `key` on the current keyboard layout.
fn windows_name(key: u32) -> Option<String> {
    unsafe {
        let scan = MapVirtualKeyW(key, MAPVK_VK_TO_VSC);
        let extended = matches!(key, 33..=40 | 45 | 46 | 111 | 144);
        let mut buffer = [0u16; 128];
        let len = GetKeyNameTextW(
            ((scan << 16) | (u32::from(extended) << 24)) as i32,
            &mut buffer,
        );
        (len > 0).then(|| String::from_utf16_lossy(&buffer[..len as usize]))
    }
}
/// Modifiers then the key: one entry per keycap.
fn key_parts_in(mods: u32, key: u32, language: Language) -> Vec<String> {
    let mut parts: Vec<String> = modifier_labels_in(mods, language)
        .map(String::from)
        .collect();
    parts.push(
        table_name(key, language)
            .or_else(|| windows_name(key))
            .unwrap_or_else(|| code_name(key, language)),
    );
    parts
}
/// The combination as one line, in `language`.
pub(super) fn key_label_in(mods: u32, key: u32, language: Language) -> String {
    key_parts_in(mods, key, language).join(" + ")
}

/// Explains a Print Screen that stays taken once Shotoshot has freed it from the Snipping Tool.
pub(super) fn print_screen_error(
    mods: u32,
    key: u32,
    error: &Error,
    language: Language,
) -> Option<String> {
    (crate::print_screen::is_bare(mods, key) && crate::print_screen::held(error))
        .then(|| tr_in(language, Key::SetupPrintScreenHeld).into())
}

impl App {
    /// Gives Print Screen back to the Snipping Tool once no Shotoshot shortcut uses it.
    pub(super) fn restore_print_screen_if_unused(&self) {
        let s = &self.settings;
        if self.persist_preferences
            && !crate::print_screen::is_bare(s.modifiers, s.key)
            && !crate::print_screen::is_bare(s.instant_shortcut.modifiers, s.instant_shortcut.key)
        {
            // Best effort: the shortcut is already saved; the uninstaller retries the restore.
            let _ = crate::print_screen::restore();
        }
    }
    pub(super) fn setup_activate(&mut self, hwnd: HWND, action: usize) -> Result<()> {
        self.setup_ui.focus = action;
        match action {
            0 => {
                self.setup_ui.recording = true;
                self.setup_ui.error.clear();
            }
            1 => self.close_setup(hwnd)?,
            2 => {
                let Some((mods, key)) = self.setup_ui.pending else {
                    return Ok(());
                };
                if let Some(index) = self.setup_ui.target.shape_index() {
                    if self.save_shape_shortcut(hwnd, index, mods, key)? {
                        self.close_setup(hwnd)?;
                        self.notify(Notice::Preferences);
                    }
                    invalidate(hwnd);
                    return Ok(());
                }
                if self.setup_ui.target == ShortcutTarget::Instant {
                    self.save_instant_shortcut(hwnd, mods, key)?;
                    invalidate(hwnd);
                    return Ok(());
                }
                if self.setup_ui.target == ShortcutTarget::Board {
                    return self
                        .save_board_shortcut(hwnd, mods, key)
                        .map(|_| invalidate(hwnd));
                }
                if self.settings.instant_shortcut
                    == (crate::settings::Shortcut {
                        modifiers: mods,
                        key,
                    })
                {
                    self.setup_ui.error = self.setup_ui.text(Key::SetupUsedForInstant);
                    invalidate(hwnd);
                    return Ok(());
                }
                if self
                    .settings
                    .presentation
                    .shape_shortcuts
                    .iter()
                    .any(|s| s.modifiers == mods && s.key == key)
                    || (mods == 3 && key == 27)
                {
                    self.setup_ui.error = self.setup_ui.text(Key::SetupUsedByPresentation);
                    invalidate(hwnd);
                    return Ok(());
                }
                if self.settings.board_shortcut
                    == (crate::settings::Shortcut {
                        modifiers: mods,
                        key,
                    })
                {
                    self.setup_ui.error = self.setup_ui.text(Key::SetupUsedByBoard);
                    invalidate(hwnd);
                    return Ok(());
                }
                if !(self.hotkey.active()
                    && self.settings.key == key
                    && self.settings.modifiers == mods)
                {
                    let mut next = self.settings.clone();
                    next.key = key;
                    next.modifiers = mods;
                    let free = crate::print_screen::windows_option(self.persist_preferences);
                    if let Err(error) = self.hotkey.replace(free, mods, key, || {
                        if self.persist_preferences {
                            next.save().map_err(crate::settings::unsaved)
                        } else {
                            Ok(())
                        }
                    }) {
                        let language = self.setup_ui.language;
                        self.setup_ui.error = print_screen_error(mods, key, &error, language)
                            .unwrap_or_else(|| {
                                let code = error.code().0.to_string();
                                Text::with_in(
                                    language,
                                    Key::SetupRegisterFailed,
                                    &[("code", &code)],
                                )
                                .to_string()
                            });
                        invalidate(hwnd);
                        return Ok(());
                    }
                    self.settings = next;
                    self.restore_print_screen_if_unused();
                }
                self.close_setup(hwnd)?;
                self.notify(Notice::Preferences);
            }
            _ => {}
        }
        invalidate(hwnd);
        Ok(())
    }
    /// Saves the shortcut that opens a board session; it needs Ctrl or Alt.
    pub(super) fn save_board_shortcut(&mut self, hwnd: HWND, mods: u32, key: u32) -> Result<()> {
        let shortcut = crate::settings::Shortcut {
            modifiers: mods,
            key,
        };
        if let Err(refusal) = Settings::validate_board_shortcut(&shortcut) {
            self.setup_ui.error = self.setup_ui.text(refusal);
            return Ok(());
        }
        if (self.settings.modifiers == mods && self.settings.key == key)
            || self.settings.instant_shortcut == shortcut
            || self
                .settings
                .presentation
                .shape_shortcuts
                .contains(&shortcut)
        {
            self.setup_ui.error = self.setup_ui.text(Key::SetupUsedByShotoshot);
            return Ok(());
        }
        if !(self.board_hotkey.active() && self.settings.board_shortcut == shortcut) {
            let mut next = self.settings.clone();
            next.board_shortcut = shortcut;
            // Ctrl or Alt is required: never a bare Print Screen to take back from Windows.
            if let Err(error) = self.board_hotkey.replace(None, mods, key, || {
                if self.persist_preferences {
                    next.save().map_err(crate::settings::unsaved)
                } else {
                    Ok(())
                }
            }) {
                let code = error.code().0.to_string();
                self.setup_ui.error = Text::with_in(
                    self.setup_ui.language,
                    Key::SetupUnavailable,
                    &[("code", &code)],
                )
                .to_string();
                return Ok(());
            }
            self.settings = next;
        }
        self.close_setup(hwnd)?;
        self.notify(Notice::Preferences);
        Ok(())
    }
}

const BACKGROUND: [u8; 3] = [0x20, 0x20, 0x20];
/// The shortcut field and the Cancel button.
const SURFACE: [u8; 3] = [0x2b, 0x2b, 0x2b];
const KEYCAP: [u8; 3] = [0x3a, 0x3a, 0x3a];
const KEYCAP_EDGE: [u8; 3] = [0x56, 0x56, 0x56];
const DISABLED: [u8; 3] = [0x33, 0x33, 0x33];
const ERROR: [u8; 3] = [0xf8, 0x51, 0x49];
/// Toolbar glyphs sit on a 20 px grid; the title shows its icon at 16 px.
const ICON_SCALE: f64 = 0.8;
const TITLE_SIZE: f64 = 16.;
const TEXT_SIZE: f64 = 13.;
const HINT_SIZE: f64 = 12.;
const CAP_HEIGHT: f64 = 28.;
const CAP_MARGIN: f64 = 10.;
const GAP: f64 = 6.;
/// Room kept between the field's edge and its keys.
const FIELD_MARGIN: f64 = 16.;
const RADIUS: f64 = 5.;

/// Width of a keycap whose label is `label` wide, at `scale`.
fn cap_width(label: f64, scale: f64) -> f64 {
    (label + 2. * CAP_MARGIN * scale)
        .max(CAP_HEIGHT * scale)
        .ceil()
}

/// What the shortcut field shows, left to right.
enum Piece {
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
fn pieces(ui: &Setup) -> Vec<Piece> {
    if ui.recording {
        let mut held = caps(modifier_labels_in(ui.held, ui.language).map(String::from));
        if held.is_empty() {
            return vec![Piece::Prompt(Key::SetupPress)];
        }
        held.extend([Piece::Plus, Piece::Prompt(Key::SetupWaiting)]);
        held
    } else if let Some((mods, key)) = ui.pending {
        caps(key_parts_in(mods, key, ui.language).into_iter())
    } else {
        vec![Piece::Prompt(Key::SetupChoose)]
    }
}

/// One line of text centred in `r`.
fn centered<C: Canvas>(
    canvas: &mut C,
    text: &str,
    r: Rect,
    size: f64,
    rgb: [u8; 3],
) -> std::result::Result<(), C::Error> {
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
) -> std::result::Result<(), C::Error> {
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
fn paint<C: Canvas>(canvas: &mut C, ui: &Setup, scale: f64) -> std::result::Result<(), C::Error> {
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
    tray_menu::draw_icon(
        canvas,
        tray_menu::icon(ui.target.menu_id()),
        ICON * scale,
        u(ICON_SCALE),
        SECONDARY,
    )?;
    let title_text = text(ui.target.title());
    let title = canvas.measure_text(title_text, u(TITLE_SIZE))?;
    let title_top = u(ICON.y + 10. * ICON_SCALE) - title.y / 2.;
    canvas.text(
        title_text,
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
        Ink::solid(if ui.recording || ui.focus == 0 {
            ACCENT
        } else {
            RULE
        }),
        u(1.),
    )?;
    let mut slots = Vec::new();
    for piece in pieces(ui) {
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
        ui.hint(),
        px(zones.hint),
        u(HINT_SIZE),
        Ink::solid(if ui.error.is_empty() { MUTED } else { ERROR }),
    )?;
    button(
        canvas,
        px(zones.cancel),
        scale,
        text(Key::SetupCancel),
        [SURFACE, if ui.focus == 1 { ACCENT } else { RULE }, TEXT],
    )?;
    let fill = if ui.pending.is_some() && !ui.recording {
        ACCENT
    } else {
        DISABLED
    };
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

impl Setup {
    /// Paints the window over `bounds` of `dc`. Opening or the first paint creates the painter
    /// and closing the window drops it: the resident keeps no renderer at rest.
    pub fn draw(&mut self, dc: HDC, bounds: RECT) -> Result<()> {
        if bounds.right <= bounds.left || bounds.bottom <= bounds.top {
            return Ok(());
        }
        // Software like the menu's: a Direct3D device would outlive the window in the resident.
        let renderer = match self.painter.take() {
            Some(renderer) => renderer,
            None => Renderer::software()?,
        };
        let scale = (bounds.right - bounds.left) as f64 / self.layout.size.x;
        let painted = unsafe {
            renderer.dc.BindDC(dc, &bounds).and_then(|()| {
                renderer.dc.BeginDraw();
                renderer
                    .dc
                    .SetTransform(&windows_numerics::Matrix3x2::identity());
                let painted = D2dCanvas::new(&renderer, &renderer.dc)
                    .and_then(|mut canvas| paint(&mut canvas, self, scale));
                let ended = renderer.dc.EndDraw(None, None);
                painted.and(ended)
            })
        };
        // A lost render target is rebuilt by the next paint.
        if painted.is_ok() {
            self.painter = Some(renderer);
        }
        painted
    }
}

fn render(ui: &mut Setup, scale: i32) -> Result<Image> {
    let size = ui.layout.size;
    let (width, height) = (size.x as i32 * scale, size.y as i32 * scale);
    paint_image(width as u32, height as u32, |dc| {
        ui.draw(
            dc,
            RECT {
                right: width,
                bottom: height,
                ..Default::default()
            },
        )
    })
}
pub(super) fn preview() -> Result<()> {
    notice::preview()?;
    for (name, mut ui) in [
        (
            "shotoshot-welcome.png",
            Setup::change(ShortcutTarget::Capture, None),
        ),
        ("shotoshot-print-screen.png", Setup::first_run()),
        (
            "shotoshot-shortcut.png",
            Setup {
                target: ShortcutTarget::Arrow,
                pending: Some((6, 0x46)),
                focus: 2,
                ..Default::default()
            },
        ),
    ] {
        save_png(
            &render(&mut ui, 1)?,
            &std::path::Path::new("target").join(name),
        )?;
    }
    tray_menu::preview()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pixels of the logical area `r` whose RGB colour passes `test`.
    fn count(image: &Image, r: Rect, scale: i32, test: impl Fn([u8; 3]) -> bool) -> usize {
        let scale = scale as f64;
        let columns = (r.min.x * scale) as usize..(r.max.x * scale) as usize;
        let rows = (r.min.y * scale) as usize..(r.max.y * scale) as usize;
        image
            .pixels
            .as_chunks::<4>()
            .0
            .iter()
            .enumerate()
            .filter(|(index, pixel)| {
                columns.contains(&(index % image.width as usize))
                    && rows.contains(&(index / image.width as usize))
                    && test([pixel[2], pixel[1], pixel[0]])
            })
            .count()
    }
    /// Keycaps crossed by a scan line just under their top edge, where no glyph reaches.
    fn keycaps(image: &Image, scale: i32, zones: SetupLayout) -> usize {
        let row = ((zones.field.center().y - CAP_HEIGHT / 2. + 3.) * scale as f64) as usize;
        let line = &image.pixels[row * image.width as usize * 4..][..image.width as usize * 4];
        let cap: Vec<bool> = line
            .as_chunks::<4>()
            .0
            .iter()
            .map(|pixel| [pixel[2], pixel[1], pixel[0]] == KEYCAP)
            .collect();
        cap.windows(2).filter(|pair| !pair[0] && pair[1]).count()
    }
    const ZONES: SetupLayout = SetupLayout::ENGLISH;
    const FIELD: Rect = ZONES.field;
    const HINT: Rect = ZONES.hint;
    const CANCEL: Rect = ZONES.cancel;
    const SAVE: Rect = ZONES.save;
    fn shown(pending: (u32, u32)) -> Setup {
        Setup {
            pending: Some(pending),
            focus: 2,
            ..Default::default()
        }
    }

    #[test]
    fn a_shortcut_shows_one_keycap_per_key() {
        for (pending, expected) in [((0, 19), 1), ((2, 0x46), 2), ((6, 19), 3), ((15, 0x2C), 5)] {
            let image = render(&mut shown(pending), 1).unwrap();
            assert_eq!(keycaps(&image, 1, ZONES), expected, "{pending:?}");
        }
    }

    #[test]
    fn the_longest_shortcut_stays_inside_its_field() {
        let image = render(&mut shown((15, 0x2C)), 1).unwrap();
        assert_eq!(
            keycaps(&image, 1, ZONES),
            5,
            "every key stays, separators go"
        );
        // On the rows the keycaps occupy, nothing is drawn in the field's margins.
        let (top, bottom) = (
            FIELD.center().y - CAP_HEIGHT / 2.,
            FIELD.center().y + CAP_HEIGHT / 2.,
        );
        for (from, to) in [
            (FIELD.min.x + 2., FIELD.min.x + FIELD_MARGIN),
            (FIELD.max.x - FIELD_MARGIN, FIELD.max.x - 2.),
        ] {
            let margin = area(from, top, to, bottom);
            assert_eq!(count(&image, margin, 1, |rgb| rgb != SURFACE), 0);
        }
    }

    #[test]
    fn opening_from_the_menu_listens_at_once_and_frames_the_field() {
        let mut ui = Setup::change(ShortcutTarget::Instant, Some((0, 145)));
        assert!(ui.recording);
        assert_eq!(
            ui.pending,
            Some((0, 145)),
            "Cancel keeps the current shortcut"
        );
        let accent = |rgb| rgb == ACCENT;
        let listening = render(&mut ui, 1).unwrap();
        assert!(count(&listening, FIELD, 1, accent) > 300);
        assert_eq!(keycaps(&listening, 1, ZONES), 0);
        assert_eq!(
            count(&listening, SAVE, 1, accent),
            0,
            "nothing to save while listening"
        );
        ui.held = 6;
        assert_eq!(
            keycaps(&render(&mut ui, 1).unwrap(), 1, ZONES),
            2,
            "held modifiers show before the key"
        );
        ui.record(6, 19);
        let recorded = render(&mut ui, 1).unwrap();
        assert_eq!(count(&recorded, FIELD, 1, accent), 0);
        assert_eq!(keycaps(&recorded, 1, ZONES), 3);
        assert!(count(&recorded, SAVE, 1, accent) > 1500);
    }

    #[test]
    fn clicks_land_on_what_is_painted() {
        for (zone, action) in [(FIELD, 0), (CANCEL, 1), (SAVE, 2)] {
            let (center, inside) = (zone.center(), zone.expanded(-1.));
            assert_eq!(ZONES.hit(center.x, center.y), Some(action));
            assert_eq!(ZONES.hit(inside.min.x, inside.min.y), Some(action));
            assert_eq!(ZONES.hit(inside.max.x, inside.max.y), Some(action));
        }
        assert_eq!(ZONES.hit(ICON.x, ICON.y), None);
        assert_eq!(ZONES.hit(CANCEL.max.x + 4., CANCEL.center().y), None);
        let image = render(&mut shown((0, 19)), 1).unwrap();
        // Just inside each zone's top edge, above any text: the control covers the window.
        for zone in [FIELD, CANCEL, SAVE] {
            let strip = area(
                zone.min.x + 4.,
                zone.min.y + 2.,
                zone.max.x - 4.,
                zone.min.y + 5.,
            );
            assert_eq!(count(&image, strip, 1, |rgb| rgb == BACKGROUND), 0);
        }
    }

    #[test]
    fn each_shortcut_shows_the_icon_of_its_menu_row() {
        let icon = Rect {
            min: ICON,
            max: ICON + Point { x: 16., y: 16. },
        };
        let drawn = |target| {
            let image = render(
                &mut Setup {
                    target,
                    ..Default::default()
                },
                1,
            )
            .unwrap();
            count(&image, icon, 1, |rgb| rgb != BACKGROUND)
        };
        let inks = [
            ShortcutTarget::Capture,
            ShortcutTarget::Instant,
            ShortcutTarget::Arrow,
            ShortcutTarget::Rectangle,
            ShortcutTarget::Ellipse,
            ShortcutTarget::Board,
        ]
        .map(drawn);
        assert!(inks.iter().all(|&ink| ink > 20), "{inks:?}");
        assert_ne!(
            inks[3], inks[4],
            "rectangle and ellipse are different glyphs"
        );
    }

    #[test]
    fn the_board_shortcut_asks_for_ctrl_or_alt() {
        let mut ui = Setup::change(ShortcutTarget::Board, Some((1, 0x42)));
        assert!(ui.hint().contains("Ctrl or Alt"), "{}", ui.hint());
        ui.record(0, crate::print_screen::KEY);
        assert!(
            !ui.hint().contains("Snipping Tool"),
            "the board never takes Print Screen from Windows"
        );
    }

    #[test]
    fn an_error_replaces_the_hint_in_red() {
        let red = |rgb: [u8; 3]| rgb[0] > 150 && rgb[0] as i32 - rgb[1] as i32 > 60;
        let mut ui = shown((1, 0x52));
        assert_eq!(count(&render(&mut ui, 1).unwrap(), HINT, 1, red), 0);
        ui.error = print_screen_error(
            0,
            crate::print_screen::KEY,
            &Error::from_hresult(HRESULT::from_win32(ERROR_HOTKEY_ALREADY_REGISTERED.0)),
            Language::En,
        )
        .unwrap();
        let image = render(&mut ui, 1).unwrap();
        assert!(count(&image, HINT, 1, red) > 100);
        // The longest message wraps inside the hint area instead of reaching the buttons.
        let below = area(HINT.min.x, HINT.max.y, HINT.max.x, CANCEL.min.y);
        assert_eq!(count(&image, below, 1, |rgb| rgb != BACKGROUND), 0);
    }

    #[test]
    fn the_window_scales_with_the_monitor() {
        let (normal, doubled) = (
            render(&mut shown((6, 19)), 1).unwrap(),
            render(&mut shown((6, 19)), 2).unwrap(),
        );
        assert_eq!(
            (doubled.width, doubled.height),
            (2 * ZONES.size.x as u32, 2 * ZONES.size.y as u32)
        );
        assert_eq!(keycaps(&doubled, 2, ZONES), 3);
        let accent = |rgb| rgb == ACCENT;
        let ratio =
            count(&doubled, SAVE, 2, accent) as f64 / count(&normal, SAVE, 1, accent) as f64;
        assert!((3.5..=5.).contains(&ratio), "{ratio}");
    }

    fn renderer() -> (crate::ComApartment, Renderer) {
        let com = crate::ComApartment::new().unwrap();
        (com, Renderer::software().unwrap())
    }

    #[test]
    fn english_layout_is_unchanged() {
        let english = SetupLayout {
            size: Point { x: 420., y: 248. },
            field: area(20., 58., 400., 122.),
            hint: area(20., 132., 400., 184.),
            cancel: area(216., 196., 304., 228.),
            save: area(312., 196., 400., 228.),
        };
        let (_com, renderer) = renderer();
        assert_eq!(layout(&renderer, Language::En).unwrap(), english);
        assert_eq!(SetupLayout::default(), english);
    }

    #[test]
    fn the_window_grows_only_where_a_text_needs_room() {
        let english = SetupLayout::default();
        let short = Widths {
            title: 100.,
            keys: 200.,
            button: 40.,
        };
        assert_eq!(SetupLayout::fit(short, 30.), english, "short texts keep it");
        // A long title widens the window; the buttons stay on its right edge.
        let wide = SetupLayout::fit(
            Widths {
                title: 500.,
                ..short
            },
            30.,
        );
        assert_eq!(wide.size, Point { x: 566., y: 248. });
        assert_eq!((wide.field.max.x, wide.hint.max.x), (546., 546.));
        assert_eq!(wide.save, area(458., 196., 546., 228.));
        assert_eq!(wide.cancel, area(362., 196., 450., 228.));
        // A long row of keycaps keeps the field's inner margins.
        let keys = SetupLayout::fit(
            Widths {
                keys: 400.,
                ..short
            },
            30.,
        );
        assert_eq!(keys.size.x, 472.);
        // A hint taller than its zone pushes the buttons down, the width stays.
        let tall = SetupLayout::fit(short, 70.2);
        assert_eq!(tall.hint, area(20., 132., 400., 203.));
        assert_eq!(tall.cancel, area(216., 215., 304., 247.));
        assert_eq!(tall.size, Point { x: 420., y: 267. });
        // A long button label widens both buttons alike.
        let long = SetupLayout::fit(
            Widths {
                button: 70.,
                ..short
            },
            30.,
        );
        assert_eq!(long.size, english.size);
        assert_eq!(long.save, area(298., 196., 400., 228.));
        assert_eq!(long.cancel, area(188., 196., 290., 228.));
    }

    #[test]
    fn setup_layout_fits_every_language() {
        let (_com, renderer) = renderer();
        for language in Language::ALL {
            let tag = language.tag();
            let zones = layout(&renderer, language).unwrap();
            let measure = |text: &str, size: f64| {
                renderer
                    .measure_text_in(text, size as f32, language)
                    .unwrap()
            };
            assert!(
                zones.size.x <= 640. && zones.size.y <= 400.,
                "{tag}: {:?} is larger than 640×400, shorten its texts",
                zones.size
            );
            for target in TARGETS {
                let title = tr_in(language, target.title());
                assert!(
                    TITLE_X + measure(title, TITLE_SIZE).x + MARGIN <= zones.size.x,
                    "{tag}: `{title}`"
                );
            }
            for (key, button) in [
                (Key::SetupCancel, zones.cancel),
                (Key::SetupSave, zones.save),
            ] {
                let label = measure(tr_in(language, key), TEXT_SIZE);
                assert!(
                    button.size().x >= label.x + 2. * BUTTON_PADDING && button.size().y >= label.y,
                    "{tag}: `{}`",
                    tr_in(language, key)
                );
            }
            assert!(zones.cancel.max.x < zones.save.min.x, "{tag}");
            for hint in hints_in(language) {
                let wrapped = renderer
                    .measure_wrapped_text_in(
                        &hint,
                        HINT_SIZE as f32,
                        zones.hint.size().x as f32,
                        language,
                    )
                    .unwrap();
                assert!(wrapped.y <= zones.hint.size().y, "{tag}: `{hint}`");
                assert!(wrapped.x <= zones.hint.size().x, "{tag}: `{hint}`");
            }
            let room = zones.field.size().x - 2. * FIELD_MARGIN;
            assert!(
                keys_width(&renderer, language).unwrap() <= room,
                "{tag}: the longest shortcut leaves the field"
            );
            if language == Language::En {
                assert_eq!(zones, SetupLayout::default());
            }
        }
    }

    #[test]
    fn key_names_are_translated_but_layout_names_stay() {
        let de = |key| tr_in(Language::De, key);
        assert_eq!(
            key_label_in(6, 0x2C, Language::De),
            format!(
                "{} + {} + {}",
                de(Key::KeyCtrl),
                de(Key::KeyShift),
                de(Key::KeyPrintScreen)
            )
        );
        assert_eq!(key_label_in(2, 0x2C, Language::De), "Strg + Druck");
        assert_eq!(key_label_in(6, 19, Language::En), "Ctrl + Shift + Pause");
        assert_eq!(key_label_in(0, 0x67, Language::En), "Numpad 7");
        assert_eq!(key_label_in(0, 0x6B, Language::En), "Numpad +");
        assert_eq!(key_label_in(8, 0x75, Language::Fr), "Win + F6");
        // A key Windows names follows the keyboard layout, never the interface language.
        for key in [0x41, 0x5A, 0x31, 0xBA, 0xDE] {
            let windows = key_label_in(0, key, Language::En);
            for language in Language::ALL {
                assert_eq!(
                    key_label_in(0, key, language),
                    windows,
                    "{}",
                    language.tag()
                );
            }
        }
    }

    #[test]
    fn the_window_speaks_the_language_it_was_fitted_for() {
        let ui = Setup {
            language: Language::De,
            pending: Some((2, 0x2C)),
            ..Default::default()
        };
        assert_eq!(ui.hint(), tr_in(Language::De, Key::SetupChangeHint));
        let shown: Vec<String> = pieces(&ui)
            .into_iter()
            .filter_map(|piece| match piece {
                Piece::Cap(label) => Some(label),
                _ => None,
            })
            .collect();
        assert_eq!(shown, ["Strg", "Druck"]);
    }

    #[test]
    fn a_translated_window_draws_inside_its_zones() {
        let (_com, renderer) = renderer();
        for language in Language::ALL {
            let tag = language.tag();
            let mut ui = Setup {
                language,
                layout: layout(&renderer, language).unwrap(),
                pending: Some((15, 0x2C)),
                focus: 2,
                ..Default::default()
            };
            ui.error = print_screen_error(
                0,
                crate::print_screen::KEY,
                &Error::from_hresult(HRESULT::from_win32(ERROR_HOTKEY_ALREADY_REGISTERED.0)),
                language,
            )
            .unwrap();
            let image = render(&mut ui, 1).unwrap();
            let zones = ui.layout;
            assert_eq!(
                (image.width, image.height),
                (zones.size.x as u32, zones.size.y as u32),
                "{tag}"
            );
            let below = area(
                zones.hint.min.x,
                zones.hint.max.y,
                zones.hint.max.x,
                zones.cancel.min.y,
            );
            assert_eq!(count(&image, below, 1, |rgb| rgb != BACKGROUND), 0, "{tag}");
            assert_eq!(keycaps(&image, 1, zones), 5, "{tag}");
        }
    }

    #[test]
    fn the_hint_area_makes_room_for_every_refusal() {
        use shotoshot_core::settings::{BOARD_SHORTCUT_REFUSALS, SHAPE_SHORTCUT_REFUSALS};
        for language in Language::ALL {
            let hints = hints_in(language);
            // A shape shortcut is refused inside `setup.invalid`, a board shortcut alone.
            for detail in SHAPE_SHORTCUT_REFUSALS
                .into_iter()
                .chain([Key::SetupUsedForCapture, Key::SetupUsedByBoard])
            {
                let composed = Text::with_in(
                    language,
                    Key::SetupInvalid,
                    &[("detail", tr_in(language, detail))],
                )
                .to_string();
                assert!(hints.contains(&composed), "{}: {composed}", language.tag());
            }
            for refusal in BOARD_SHORTCUT_REFUSALS {
                let alone = tr_in(language, refusal).to_string();
                assert!(hints.contains(&alone), "{}: {alone}", language.tag());
            }
        }
    }

    #[test]
    fn a_window_that_cannot_measure_keeps_the_english_layout() {
        let mut ui = Setup::change(ShortcutTarget::Board, None);
        ui.layout.size = Point { x: 1., y: 1. };
        ui.place(Language::De, Err(Error::new(E_FAIL, "no renderer")));
        assert_eq!(ui.layout, SetupLayout::default());
        assert_eq!(ui.language, Language::De);
        assert!(ui.painter.is_none(), "painting creates its own renderer");
    }

    #[test]
    fn only_painting_creates_the_painter() {
        let mut ui = Setup::change(ShortcutTarget::Capture, None);
        assert!(ui.painter.is_none());
        render(&mut ui, 1).unwrap();
        assert!(ui.painter.is_some(), "kept between paints of one opening");
        assert!(Setup::first_run().painter.is_none());
    }
}
