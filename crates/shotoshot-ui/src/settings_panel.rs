//! ROLE: The Settings window, without its operating system: its pages (General, Shortcuts,
//! Presentation, Pointer effects), where each row sits for the texts of a language, how it is
//! painted with the pointer's live preview, and what a click, a drag, the wheel or a key changes.
//! OWNER: The platform window holds one `Panel` while it is open; it applies the pointer changes
//! and runs the `Command`s, then hands the panel the new `Values`.
//! INVARIANTS: Layout is in 96-DPI logical pixels, measured from the texts of the language: no
//! text is cut. The window keeps one size whatever the page or the open card, so nothing jumps.
//! Painting and hit testing read the same `Layout`.
use crate::icon::Icon;
use crate::{Canvas, Ink, TextStyle};
use shotoshot_core::{
    Point, Rect,
    pointer::{
        Button as MouseButton, ClickSound, ClickStyle, Effects, Follow, HALO_DIAMETER_RANGE,
        HALO_OPACITY_RANGE, HALO_SOFTNESS_RANGE, HALO_THICKNESS_RANGE, HaloShape, LOUPE_SIZE_RANGE,
        LOUPE_ZOOM_RANGE, PointerSettings, SPOTLIGHT_DIM_RANGE, SPOTLIGHT_SIZE_RANGE,
        SpotlightShape, TrailLength, TrailStyle, Volume,
    },
    settings::INPUT_SCALES,
};
use shotoshot_text::{Key, Language, Text, tr_in};
use std::collections::HashMap;
mod draw;

pub use draw::{BACKGROUND, keycaps, keycaps_width};
// Type of the window (Mickaël, 2026-10-30): titles in the display face, group headings
// semibold, explanations in the face cut for small sizes.
pub const TITLE_SIZE: f64 = 28.;
pub const LABEL_SIZE: f64 = 14.;
pub const SMALL_SIZE: f64 = 12.;
pub const NOTE_SIZE: f64 = 12.;
const MARGIN: f64 = 24.;
const NAV_PAD: f64 = 8.;
const NAV_ROW: f64 = 36.;
/// From a navigation row's edge to its icon, and from its icon to its name.
const NAV_TEXT: f64 = 12.;
const NAV_ICON: f64 = 16.;
const MIN_NAV: f64 = 196.;
const TITLE_HEIGHT: f64 = 68.;
const GROUP: f64 = 38.;
const ROW: f64 = 48.;
/// A row with an explanation under its label.
const NOTE_ROW: f64 = 62.;
/// Rows stand apart, each on its own background, this far.
const ROW_GAP: f64 = 2.;
const ROW_PAD: f64 = 16.;
/// A row's icon, and the room between it and the label.
const ICON: f64 = 20.;
const ICON_GAP: f64 = 16.;
const GAP: f64 = 24.;
const CARD_HEADER: f64 = 54.;
const CARD_GAP: f64 = 2.;
const FIELD_ROW: f64 = 34.;
/// Fields start under their card's title, past its icon.
const FIELD_INDENT: f64 = ICON + ICON_GAP;
const LINK: f64 = 36.;
/// The Add button of a shortcut not set: its padding beside the text, and its height.
const BUTTON_PAD: f64 = 14.;
const BUTTON_HEIGHT: f64 = 28.;
const CONTROL_HEIGHT: f64 = 26.;
const SEGMENT_PAD: f64 = 12.;
const MIN_SEGMENT: f64 = 44.;
const SLIDER: f64 = 176.;
const SWATCH_STEP: f64 = 22.;
pub(crate) const SWATCH: f64 = 8.;
const SWITCH: Point = Point { x: 36., y: 18. };
const CHEVRON: f64 = 20.;
const CAP_PAD: f64 = 7.;
const CAP_GAP: f64 = 4.;
const CLEAR: f64 = 24.;
const PREVIEW: Point = Point { x: 232., y: 236. };
/// The preview draws the effects smaller than life so that they fit beside the settings.
const PREVIEW_SCALE: f64 = 0.7;
/// The preview pointer clicks this often, left then right.
const PREVIEW_CLICK_MS: u64 = 1400;
/// The shape lifetimes the Presentation page offers, in milliseconds.
pub const DURATIONS: [u32; 3] = [1000, 3000, 5000];

/// Colours offered for the halo and the clicks.
pub const COLORS: [[u8; 3]; 8] = [
    [255, 211, 64],
    [255, 140, 0],
    [237, 79, 92],
    [236, 72, 153],
    [168, 85, 247],
    [64, 156, 255],
    [74, 222, 128],
    [255, 255, 255],
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Page {
    General,
    Shortcuts,
    Presentation,
    Pointer,
}
pub const PAGES: [Page; 4] = [
    Page::General,
    Page::Shortcuts,
    Page::Presentation,
    Page::Pointer,
];
impl Page {
    fn name(self) -> Key {
        match self {
            Self::General => Key::SettingsGeneral,
            Self::Shortcuts => Key::SettingsShortcuts,
            Self::Presentation => Key::MenuPresentation,
            Self::Pointer => Key::MenuPointerEffects,
        }
    }
    fn icon(self) -> Icon {
        match self {
            Self::General => Icon::Sliders,
            Self::Shortcuts => Icon::Keyboard,
            Self::Presentation => Icon::Presentation,
            Self::Pointer => Icon::Halo,
        }
    }
}

/// A shortcut the Shortcuts page shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ShortcutRow {
    Capture,
    Instant,
    Board,
    Arrow,
    Rectangle,
    Ellipse,
    ShowClick,
    Spotlight,
    Loupe,
}
/// The rows of `Values::shortcuts`, in this order.
pub const SHORTCUT_ROWS: [ShortcutRow; 9] = [
    ShortcutRow::Capture,
    ShortcutRow::Instant,
    ShortcutRow::Board,
    ShortcutRow::Arrow,
    ShortcutRow::Rectangle,
    ShortcutRow::Ellipse,
    ShortcutRow::ShowClick,
    ShortcutRow::Spotlight,
    ShortcutRow::Loupe,
];
const SHORTCUT_GROUPS: [(Key, &[ShortcutRow]); 3] = [
    (
        Key::MenuCapture,
        &[
            ShortcutRow::Capture,
            ShortcutRow::Instant,
            ShortcutRow::Board,
        ],
    ),
    (
        Key::MenuTemporaryShapes,
        &[
            ShortcutRow::Arrow,
            ShortcutRow::Rectangle,
            ShortcutRow::Ellipse,
        ],
    ),
    (
        Key::MenuPointerEffects,
        &[
            ShortcutRow::ShowClick,
            ShortcutRow::Spotlight,
            ShortcutRow::Loupe,
        ],
    ),
];
impl ShortcutRow {
    fn label(self) -> Key {
        match self {
            Self::Capture => Key::MenuTakeScreenshot,
            Self::Instant => Key::MenuInstantScreenshot,
            Self::Board => Key::MenuEditBoard,
            Self::Arrow => Key::ToolbarArrow,
            Self::Rectangle => Key::ToolbarRectangle,
            Self::Ellipse => Key::ToolbarEllipse,
            Self::ShowClick => Key::PointerShowClick,
            Self::Spotlight => Key::PointerSectionSpotlight,
            Self::Loupe => Key::PointerSectionLoupe,
        }
    }
    /// The icon of the action, as the tray menu and the editor toolbar draw it.
    fn icon(self) -> Icon {
        use crate::toolbar::Action;
        use shotoshot_core::Tool;
        match self {
            Self::Capture => Icon::Capture,
            Self::Instant => Icon::Instant,
            Self::Board => Icon::BoardEdit,
            Self::Arrow => Icon::Toolbar(Action::Tool(Tool::Arrow)),
            Self::Rectangle => Icon::Toolbar(Action::Tool(Tool::Rectangle)),
            Self::Ellipse => Icon::Toolbar(Action::Tool(Tool::Ellipse)),
            Self::ShowClick => Icon::Click,
            Self::Spotlight => Icon::Spotlight,
            Self::Loupe => Icon::Loupe,
        }
    }
    fn index(self) -> usize {
        SHORTCUT_ROWS
            .iter()
            .position(|row| *row == self)
            .unwrap_or(0)
    }
    /// Which of the three pointer shortcuts this row is: those may also be left unset.
    pub fn pointer_index(self) -> Option<usize> {
        match self {
            Self::ShowClick => Some(0),
            Self::Spotlight => Some(1),
            Self::Loupe => Some(2),
            _ => None,
        }
    }
}

/// What the menu shows or hides with a switch: the Settings window switches it too
/// (Mickaël, 2026-10-30).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Shown {
    Board,
    PointerEffects,
    Magnifier,
    Keyboard,
    Mouse,
    TemporaryShapes,
}
/// The Presentation page's switches, in the menu's order.
const PRESENTATION_SHOWN: [Shown; 5] = [
    Shown::PointerEffects,
    Shown::Magnifier,
    Shown::Keyboard,
    Shown::Mouse,
    Shown::TemporaryShapes,
];
impl Shown {
    /// The icon of its row in the tray menu.
    fn icon(self) -> Icon {
        use crate::toolbar::Action;
        use shotoshot_core::Tool;
        match self {
            Self::Board => Icon::Eye,
            Self::PointerEffects => Icon::Halo,
            Self::Magnifier => Icon::Loupe,
            Self::Keyboard => Icon::Keyboard,
            Self::Mouse => Icon::Mouse,
            Self::TemporaryShapes => Icon::Toolbar(Action::Tool(Tool::Pen)),
        }
    }
    fn label(self) -> Key {
        match self {
            Self::Board => Key::MenuShowBoard,
            Self::PointerEffects => Key::MenuPointerEffects,
            Self::Magnifier => Key::MenuMagnifier,
            Self::Keyboard => Key::MenuKeyboard,
            Self::Mouse => Key::MenuMouse,
            Self::TemporaryShapes => Key::MenuTemporaryShapes,
        }
    }
}

/// One shortcut as the page shows it: its keys, one keycap each, empty when not set.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Keys {
    pub caps: Vec<String>,
    /// Another program holds it: Shotoshot cannot use it.
    pub in_use: bool,
}

/// What the platform knows and the General, Shortcuts and Presentation pages show.
#[derive(Clone, Debug, PartialEq)]
pub struct Values {
    pub startup: bool,
    pub live: bool,
    /// In the order of `SHORTCUT_ROWS`.
    pub shortcuts: [Keys; 9],
    /// One of `INPUT_SCALES`.
    pub input_scale: u32,
    /// One of `DURATIONS`.
    pub duration_ms: u32,
    /// The keyboard and the mouse on screen can be dragged into place.
    pub moving_input: bool,
    /// On screen now: the board in the wallpaper, the keyboard, the mouse, the temporary
    /// shapes. The pointer effects and the loupe are the panel's `halo_on` and `loupe_on`.
    pub board: bool,
    pub keyboard: bool,
    pub mouse: bool,
    pub temporary_shapes: bool,
}
impl Default for Values {
    fn default() -> Self {
        Self {
            startup: true,
            live: false,
            shortcuts: Default::default(),
            input_scale: INPUT_SCALES[0],
            duration_ms: DURATIONS[1],
            moving_input: false,
            board: false,
            keyboard: false,
            mouse: false,
            temporary_shapes: false,
        }
    }
}

/// A change the platform makes: the panel shows its result with the next `Values`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    Startup,
    Live,
    EditShortcut(ShortcutRow),
    InputScale(u32),
    Duration(u32),
    /// The Presentation page's own defaults: size and lifetime.
    PresentationDefaults,
    /// Lets the keyboard and the mouse on screen be dragged into place, or fixes them again.
    MoveInput,
    /// Shows or hides it, as its menu switch does.
    Show(Shown),
}

/// What the platform does after an input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    None,
    /// Only the panel changed.
    Repaint,
    /// Apply the pointer settings to the live pointer; `save` when the change is complete (not
    /// while a slider is dragged). `sound` when the click sound was changed: let it be heard.
    Changed {
        save: bool,
        sound: bool,
    },
    Command(Command),
    Close,
}

/// The keys the window answers to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyInput {
    Next,
    Previous,
    Up,
    Down,
    Left,
    Right,
    Activate,
    Delete,
    Escape,
}

/// One effect of the Pointer effects page, with its own switch.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Card {
    Halo,
    Trail,
    Clicks,
    Spotlight,
    Loupe,
}
pub const CARDS: [Card; 5] = [
    Card::Halo,
    Card::Trail,
    Card::Clicks,
    Card::Spotlight,
    Card::Loupe,
];
impl Card {
    fn icon(self) -> Icon {
        match self {
            Self::Halo => Icon::Halo,
            Self::Trail => Icon::Trail,
            Self::Clicks => Icon::Click,
            Self::Spotlight => Icon::Spotlight,
            Self::Loupe => Icon::Loupe,
        }
    }
    fn title(self) -> Key {
        match self {
            Self::Halo => Key::PointerSectionHalo,
            Self::Trail => Key::PointerSectionTrail,
            Self::Clicks => Key::PointerSectionClicks,
            Self::Spotlight => Key::PointerSectionSpotlight,
            Self::Loupe => Key::PointerSectionLoupe,
        }
    }
    pub fn fields(self) -> &'static [Field] {
        match self {
            Self::Halo => &[
                Field::HaloShape,
                Field::HaloColor,
                Field::HaloSize,
                Field::HaloOpacity,
                Field::HaloSoftness,
                Field::HaloThickness,
                Field::Movement,
            ],
            Self::Trail => &[Field::Trail, Field::TrailLength],
            Self::Clicks => &[
                Field::Click,
                Field::LeftColor,
                Field::RightColor,
                Field::Sound,
                Field::Volume,
            ],
            Self::Spotlight => &[
                Field::Spotlight,
                Field::SpotlightSize,
                Field::SpotlightDim,
                Field::SpotlightMoving,
            ],
            Self::Loupe => &[Field::LoupeZoom, Field::LoupeSize, Field::LoupeHold],
        }
    }
    /// The position of the card among the ones whose style is remembered while off.
    fn slot(self) -> usize {
        match self {
            Self::Halo => 0,
            Self::Trail => 1,
            Self::Clicks => 2,
            Self::Spotlight => 3,
            Self::Loupe => 4,
        }
    }
}

/// One pointer setting of a card.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Field {
    HaloShape,
    HaloColor,
    HaloSize,
    HaloOpacity,
    HaloSoftness,
    HaloThickness,
    Movement,
    Trail,
    TrailLength,
    Click,
    LeftColor,
    RightColor,
    Sound,
    Volume,
    Spotlight,
    SpotlightSize,
    SpotlightDim,
    SpotlightMoving,
    LoupeZoom,
    LoupeSize,
    LoupeHold,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Control {
    /// Every value side by side, the current one marked.
    Segments,
    Slider,
    Colors,
    Switch,
}
impl Field {
    fn control(self) -> Control {
        match self {
            Self::HaloSize
            | Self::HaloOpacity
            | Self::HaloSoftness
            | Self::HaloThickness
            | Self::SpotlightSize
            | Self::SpotlightDim
            | Self::LoupeZoom
            | Self::LoupeSize => Control::Slider,
            Self::HaloColor | Self::LeftColor | Self::RightColor => Control::Colors,
            Self::SpotlightMoving | Self::LoupeHold => Control::Switch,
            _ => Control::Segments,
        }
    }
    fn label(self) -> Key {
        match self {
            Self::HaloShape | Self::Spotlight => Key::PointerShape,
            Self::HaloColor => Key::PointerColor,
            Self::HaloSize | Self::SpotlightSize | Self::LoupeSize => Key::PointerSize,
            Self::HaloOpacity => Key::PointerOpacity,
            Self::HaloSoftness => Key::PointerSoftness,
            Self::HaloThickness => Key::PointerThickness,
            Self::Movement => Key::PointerMovement,
            Self::Trail => Key::PointerStyle,
            Self::TrailLength => Key::PointerLength,
            Self::Click => Key::PointerAnimation,
            Self::LeftColor => Key::PointerLeftClick,
            Self::RightColor => Key::PointerRightClick,
            Self::Sound => Key::PointerSound,
            Self::Volume => Key::PointerVolume,
            Self::SpotlightDim => Key::PointerDarkness,
            Self::SpotlightMoving => Key::PointerHideMoving,
            Self::LoupeHold => Key::PointerHold,
            Self::LoupeZoom => Key::PointerZoom,
        }
    }
    /// The values a segmented field offers: each one's name and the index of its enum value.
    /// "Off" is the card's switch, not a value, except for the sound.
    fn options(self) -> &'static [(Key, usize)] {
        match self {
            Self::HaloShape => &[
                (Key::PointerDisc, 1),
                (Key::PointerRing, 2),
                (Key::PointerNeon, 3),
            ],
            Self::Movement => &[(Key::PointerInstant, 0), (Key::PointerSmooth, 1)],
            Self::Trail => &[
                (Key::PointerLine, 1),
                (Key::PointerDots, 2),
                (Key::PointerSparks, 3),
                (Key::PointerStars, 4),
            ],
            Self::TrailLength => &[
                (Key::PointerShort, 0),
                (Key::PointerMediumLength, 1),
                (Key::PointerLong, 2),
            ],
            Self::Click => &[
                (Key::PointerRipple, 1),
                (Key::PointerClickRing, 2),
                (Key::PointerWaves, 3),
            ],
            Self::Sound => &[
                (Key::PointerOff, 0),
                (Key::PointerSoft, 1),
                (Key::PointerClickSound, 2),
            ],
            Self::Volume => &[
                (Key::PointerLow, 0),
                (Key::PointerMediumVolume, 1),
                (Key::PointerHigh, 2),
            ],
            Self::Spotlight => &[
                (Key::PointerCircle, 1),
                (Key::PointerHorizontal, 2),
                (Key::PointerVertical, 3),
            ],
            _ => &[],
        }
    }
    /// A slider's range and step.
    fn range(self) -> ((u32, u32), u32) {
        match self {
            Self::HaloSize => (HALO_DIAMETER_RANGE, 4),
            Self::HaloOpacity => (HALO_OPACITY_RANGE, 5),
            Self::HaloSoftness => (HALO_SOFTNESS_RANGE, 5),
            Self::HaloThickness => (HALO_THICKNESS_RANGE, 1),
            Self::SpotlightSize => (SPOTLIGHT_SIZE_RANGE, 20),
            Self::SpotlightDim => (SPOTLIGHT_DIM_RANGE, 5),
            Self::LoupeZoom => (LOUPE_ZOOM_RANGE, 1),
            Self::LoupeSize => (LOUPE_SIZE_RANGE, 10),
            _ => ((0, 0), 1),
        }
    }
    fn card(self) -> Card {
        CARDS
            .into_iter()
            .find(|card| card.fields().contains(&self))
            .unwrap_or(Card::Halo)
    }
}

/// Something of a page that takes the focus and answers to clicks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Item {
    Nav(Page),
    Startup,
    Live,
    Shortcut(ShortcutRow),
    InputScale,
    Duration,
    MoveInput,
    Shown(Shown),
    Card(Card),
    Field(Field),
    Reset,
}

impl Item {
    /// The glyph at the start of the item's row; fields and links have none.
    fn icon(self) -> Icon {
        use crate::toolbar::Action;
        match self {
            Self::Nav(page) => page.icon(),
            Self::Startup => Icon::SignIn,
            Self::Live => Icon::Toolbar(Action::Live),
            Self::Shortcut(row) => row.icon(),
            Self::InputScale => Icon::Size,
            Self::Duration => Icon::Clock,
            Self::MoveInput => Icon::Move,
            Self::Shown(shown) => shown.icon(),
            Self::Card(card) => card.icon(),
            Self::Field(_) | Self::Reset => Icon::None,
        }
    }
    /// The explanation under the label, where the label alone does not say what changes.
    fn note(self) -> Option<Key> {
        match self {
            Self::Startup => Some(Key::SettingsStartWithWindowsNote),
            Self::Live => Some(Key::SettingsLiveCaptureNote),
            Self::Duration => Some(Key::SettingsShapeDurationNote),
            _ => None,
        }
    }
    /// From the row's left edge to its label.
    fn label_left(self) -> f64 {
        match self {
            Self::Field(_) | Self::Reset => ROW_PAD,
            _ => ROW_PAD + ICON + ICON_GAP,
        }
    }
    fn height(self) -> f64 {
        if self.note().is_some() { NOTE_ROW } else { ROW }
    }
}

/// Where an item sits: the whole row, and its control on the right.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Zone {
    item: Item,
    row: Rect,
    control: Rect,
}

/// Where everything of the current page sits, in logical pixels.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Layout {
    pub size: Point,
    nav: Vec<(Page, Rect)>,
    title: Rect,
    /// Group titles with their text.
    groups: Vec<(Key, Rect)>,
    /// The rounded backgrounds of row groups and cards.
    blocks: Vec<Rect>,
    /// The inner backgrounds of open cards.
    insets: Vec<Rect>,
    zones: Vec<Zone>,
    preview: Rect,
}
impl Layout {
    fn zone(&self, item: Item) -> Option<Zone> {
        self.zones.iter().copied().find(|zone| zone.item == item)
    }
}

pub struct Panel {
    pub settings: PointerSettings,
    pub halo_color: [u8; 3],
    /// Whether the pointer effects are on screen. A change made on the Pointer effects page
    /// turns them on: what is set here is meant to be seen. Only the menu turns them off.
    pub halo_on: bool,
    /// Whether the loupe is on screen: its card's switch, or a change of its settings.
    pub loupe_on: bool,
    pub values: Values,
    pub language: Language,
    pub page: Page,
    /// The one card that shows its settings; None when all are folded.
    pub open: Option<Card>,
    pub focus: Item,
    /// The focus ring shows only once the keyboard moved it.
    keyboard: bool,
    /// The slider being dragged, and where its knob is, from 0 to 1: the knob follows the
    /// pointer exactly even where the value moves by whole steps.
    drag: Option<(Field, f64)>,
    /// The style each card returns to when switched on again, as an enum index.
    remembered: [usize; 4],
    /// Text widths measured by `fit`, by text, size in tenths of a pixel and style.
    widths: HashMap<(String, u32, TextStyle), f64>,
    pub layout: Layout,
    preview: Effects,
    last_click: u64,
    clicks: u64,
}

impl Panel {
    pub fn new(
        settings: PointerSettings,
        halo_color: [u8; 3],
        halo_on: bool,
        values: Values,
        language: Language,
    ) -> Self {
        let defaults = PointerSettings::default();
        let style = |current: usize, fallback: usize| if current > 0 { current } else { fallback };
        let remembered = [
            style(
                settings.halo_shape.index(),
                defaults.halo_shape.index().max(1),
            ),
            style(settings.trail.index(), 1),
            style(settings.click.index(), defaults.click.index().max(1)),
            style(settings.spotlight.index(), 1),
        ];
        Self {
            settings,
            halo_color,
            halo_on,
            loupe_on: false,
            values,
            language,
            page: Page::General,
            open: Some(Card::Halo),
            focus: Item::Nav(Page::General),
            keyboard: false,
            drag: None,
            remembered,
            widths: HashMap::new(),
            layout: Layout::default(),
            preview: Effects::default(),
            last_click: 0,
            clicks: 0,
        }
    }
    fn text(&self, key: Key) -> &'static str {
        tr_in(self.language, key)
    }
    fn width(&self, text: &str, size: f64, style: TextStyle) -> f64 {
        self.widths
            .get(&(text.to_string(), (size * 10.) as u32, style))
            .copied()
            .unwrap_or(text.chars().count() as f64 * size * 0.6)
    }
    /// The size names of the Presentation page.
    fn percent(&self, value: u32) -> String {
        Text::with_in(
            self.language,
            Key::MenuPercent,
            &[("n", &value.to_string())],
        )
        .to_string()
    }
    fn seconds(&self, ms: u32) -> String {
        Text::with_in(
            self.language,
            Key::MenuSeconds,
            &[("n", &(ms / 1000).to_string())],
        )
        .to_string()
    }
    /// The names of a segmented item's values.
    fn segment_names(&self, item: Item) -> Vec<String> {
        match item {
            Item::InputScale => INPUT_SCALES.iter().map(|v| self.percent(*v)).collect(),
            Item::Duration => DURATIONS.iter().map(|v| self.seconds(*v)).collect(),
            Item::Field(field) => field
                .options()
                .iter()
                .map(|(key, _)| self.text(*key).to_string())
                .collect(),
            _ => Vec::new(),
        }
    }
    /// Which of a segmented item's values is the current one.
    fn segment(&self, item: Item) -> Option<usize> {
        match item {
            Item::InputScale => INPUT_SCALES
                .iter()
                .position(|v| *v == self.values.input_scale),
            Item::Duration => DURATIONS.iter().position(|v| *v == self.values.duration_ms),
            Item::Field(field) => {
                let current = self.choice(field);
                field
                    .options()
                    .iter()
                    .position(|(_, index)| *index == current)
            }
            _ => None,
        }
    }
    /// The label of an item, as its row shows it.
    fn label(&self, item: Item) -> String {
        match item {
            Item::Nav(page) => self.text(page.name()).into(),
            Item::Startup => self.text(Key::MenuStartWithWindows).into(),
            Item::Live => self.text(Key::MenuLiveCaptureDefault).into(),
            Item::Shortcut(row) => self.text(row.label()).into(),
            Item::InputScale => self.text(Key::MenuInputSize).into(),
            Item::Duration => self.text(Key::MenuShapeDuration).into(),
            Item::MoveInput => self.text(Key::MenuMoveInput).into(),
            Item::Shown(shown) => self.text(shown.label()).into(),
            Item::Card(card) => self.text(card.title()).into(),
            Item::Field(field) => self.text(field.label()).into(),
            Item::Reset => self.text(Key::PointerReset).into(),
        }
    }
    /// What a folded card says of its effect.
    fn summary(&self, card: Card) -> String {
        if !self.card_on(card) {
            return self.text(Key::PointerOff).into();
        }
        let name = |field: Field| {
            let current = self.choice(field);
            field
                .options()
                .iter()
                .find(|(_, index)| *index == current)
                .map_or("", |(key, _)| self.text(*key))
        };
        let parts: Vec<&str> = match card {
            Card::Halo => vec![name(Field::HaloShape), name(Field::Movement)],
            Card::Trail => vec![name(Field::Trail), name(Field::TrailLength)],
            Card::Clicks if self.settings.sound != ClickSound::Off => {
                vec![name(Field::Click), name(Field::Sound)]
            }
            Card::Clicks => vec![name(Field::Click)],
            Card::Spotlight => vec![name(Field::Spotlight)],
            Card::Loupe => {
                let s = &self.settings;
                return [
                    (Key::PointerZoomValue, "zoom", s.loupe_zoom),
                    (Key::PointerSizeValue, "size", s.loupe_size),
                ]
                .map(|(key, name, value)| {
                    Text::with_in(self.language, key, &[(name, &value.to_string())]).to_string()
                })
                .join(" · ");
            }
        };
        parts.join(" · ")
    }

    /// Every text the window may show with the size and style it shows it at.
    fn texts(&self) -> Vec<(String, f64, TextStyle)> {
        use TextStyle::{Body, Caption, Strong, Title};
        let mut texts = Vec::new();
        for page in PAGES {
            texts.push((self.text(page.name()).to_string(), LABEL_SIZE, Body));
            texts.push((self.text(page.name()).to_string(), TITLE_SIZE, Title));
        }
        for item in [
            Item::Startup,
            Item::Live,
            Item::InputScale,
            Item::Duration,
            Item::MoveInput,
            Item::Reset,
        ]
        .into_iter()
        .chain(SHORTCUT_ROWS.map(Item::Shortcut))
        .chain(PRESENTATION_SHOWN.map(Item::Shown))
        .chain([Item::Shown(Shown::Board)])
        .chain(CARDS.map(Item::Card))
        .chain(
            CARDS
                .iter()
                .flat_map(|card| card.fields().iter().map(|f| Item::Field(*f))),
        ) {
            texts.push((self.label(item), LABEL_SIZE, Body));
            if let Some(note) = item.note() {
                texts.push((self.text(note).into(), NOTE_SIZE, Caption));
            }
            for name in self.segment_names(item) {
                texts.push((name, SMALL_SIZE, Body));
            }
        }
        for (key, _) in SHORTCUT_GROUPS {
            texts.push((self.text(key).into(), LABEL_SIZE, Strong));
        }
        for keys in &self.values.shortcuts {
            for cap in &keys.caps {
                texts.push((cap.clone(), SMALL_SIZE, Body));
            }
        }
        texts.push((self.text(Key::SettingsAdd).into(), LABEL_SIZE, Body));
        texts.push((self.text(Key::SettingsInUse).into(), NOTE_SIZE, Caption));
        texts.push((self.text(Key::PointerOff).into(), NOTE_SIZE, Caption));
        for card in CARDS {
            texts.push((self.summary(card), NOTE_SIZE, Caption));
        }
        texts
    }
    /// Measures every text of the language with `measure` (a text's width at a size, in logical
    /// pixels) and sizes the window for the widest and tallest page.
    pub fn fit<E>(
        &mut self,
        mut measure: impl FnMut(&str, f64, TextStyle) -> Result<f64, E>,
    ) -> Result<(), E> {
        self.widths.clear();
        for (text, size, style) in self.texts() {
            let width = measure(&text, size, style)?.ceil();
            self.widths
                .insert((text, (size * 10.) as u32, style), width);
        }
        // Summaries change with the values: measure every one a card may show.
        let saved = (self.settings.clone(), self.loupe_on);
        let mut summaries = Vec::new();
        for card in CARDS {
            for field in card.fields() {
                for (_, index) in field.options() {
                    self.set_choice(*field, *index);
                    summaries.push(self.summary(card));
                }
                if field.control() == Control::Slider {
                    // The widest numbers a summary may show are at the ends of the range.
                    let ((low, high), _) = field.range();
                    for value in [low, high] {
                        self.set_exact(*field, value);
                        summaries.push(self.summary(card));
                    }
                }
            }
        }
        (self.settings, self.loupe_on) = saved;
        for text in summaries {
            let width = measure(&text, NOTE_SIZE, TextStyle::Caption)?.ceil();
            self.widths
                .insert((text, (NOTE_SIZE * 10.) as u32, TextStyle::Caption), width);
        }
        let (page, open) = (self.page, self.open);
        let mut size = Point { x: 0., y: 0. };
        for each in PAGES {
            self.page = each;
            let cases: Vec<Option<Card>> = if each == Page::Pointer {
                CARDS.map(Some).to_vec()
            } else {
                vec![open]
            };
            for case in cases {
                self.open = case;
                let needed = self.arrange_page(true);
                size.x = size.x.max(needed.x);
                size.y = size.y.max(needed.y);
            }
        }
        (self.page, self.open) = (page, open);
        self.layout.size = Point {
            x: size.x.ceil(),
            y: size.y.ceil(),
        };
        self.arrange();
        Ok(())
    }
    /// Places the current page in the window's size.
    pub fn arrange(&mut self) {
        let size = self.layout.size;
        self.arrange_page(false);
        self.layout.size = size;
    }
    fn nav_width(&self) -> f64 {
        PAGES
            .iter()
            .map(|page| {
                self.width(self.text(page.name()), LABEL_SIZE, TextStyle::Body)
                    + 2. * (NAV_PAD + NAV_TEXT)
                    + NAV_ICON
                    + NAV_TEXT
            })
            .fold(MIN_NAV, f64::max)
    }
    /// The width a control needs.
    fn control_width(&self, item: Item) -> f64 {
        match item {
            Item::Startup | Item::Live | Item::MoveInput | Item::Shown(_) => SWITCH.x,
            Item::Shortcut(row) => {
                let keys = &self.values.shortcuts[row.index()];
                let caps = if keys.caps.is_empty() {
                    self.add_width()
                } else {
                    keycaps_width(
                        keys.caps
                            .iter()
                            .map(|cap| self.width(cap, SMALL_SIZE, TextStyle::Body)),
                    )
                };
                caps + if row.pointer_index().is_some() {
                    CLEAR
                } else {
                    0.
                }
            }
            Item::InputScale | Item::Duration => self.segments_width(item),
            Item::Card(_) => SWITCH.x + CHEVRON + 12.,
            Item::Field(field) => match field.control() {
                Control::Segments => self.segments_width(item),
                Control::Slider => SLIDER,
                Control::Colors => SWATCH_STEP * COLORS.len() as f64,
                Control::Switch => SWITCH.x,
            },
            Item::Nav(_) | Item::Reset => 0.,
        }
    }
    /// The Add button of a shortcut not set.
    fn add_width(&self) -> f64 {
        self.width(self.text(Key::SettingsAdd), LABEL_SIZE, TextStyle::Body) + 2. * BUTTON_PAD
    }
    fn segments_width(&self, item: Item) -> f64 {
        self.segment_widths(item).iter().sum::<f64>() + 4.
    }
    fn segment_widths(&self, item: Item) -> Vec<f64> {
        self.segment_names(item)
            .iter()
            .map(|name| {
                (self.width(name, SMALL_SIZE, TextStyle::Body) + 2. * SEGMENT_PAD).max(MIN_SEGMENT)
            })
            .collect()
    }
    /// The widest label among `items`, measured from the row's padding: its icon and its
    /// explanation, or the note of a shortcut held elsewhere, included.
    fn label_width(&self, items: &[Item]) -> f64 {
        let note = |key: Key| self.width(self.text(key), NOTE_SIZE, TextStyle::Caption);
        items
            .iter()
            .map(|item| {
                let mut width = self.width(&self.label(*item), LABEL_SIZE, TextStyle::Body);
                if matches!(item, Item::Shortcut(_)) {
                    width = width.max(note(Key::SettingsInUse));
                }
                if let Some(key) = item.note() {
                    width = width.max(note(key));
                }
                item.label_left() - ROW_PAD + width
            })
            .fold(0., f64::max)
    }
    /// Lays out the current page; with `natural`, as wide and tall as it needs, else in the
    /// window's size. Returns the size it needs.
    fn arrange_page(&mut self, natural: bool) -> Point {
        let nav_width = self.nav_width();
        let mut layout = Layout {
            size: self.layout.size,
            ..Layout::default()
        };
        let mut top = NAV_PAD + 4.;
        for page in PAGES {
            layout
                .nav
                .push((page, area(NAV_PAD, top, nav_width - NAV_PAD, top + NAV_ROW)));
            top += NAV_ROW + 2.;
        }
        let left = nav_width + 8.;
        let title = self.width(self.text(self.page.name()), TITLE_SIZE, TextStyle::Title);
        layout.title = area(left, 0., left + title + TITLE_SIZE, TITLE_HEIGHT);
        let mut needed_width: f64 = title;
        let mut top = TITLE_HEIGHT;
        // The width the rows of a block need, then their right edge in the window.
        let content_right = |needed: f64, layout_width: f64| {
            if natural {
                left + needed
            } else {
                layout_width - MARGIN
            }
        };
        match self.page {
            Page::General | Page::Presentation => {
                // Presentation: what is on screen first, as the menu lists it, then how it shows.
                let blocks: Vec<Vec<Item>> = if self.page == Page::General {
                    vec![vec![Item::Startup, Item::Live, Item::Shown(Shown::Board)]]
                } else {
                    vec![
                        PRESENTATION_SHOWN.map(Item::Shown).to_vec(),
                        vec![Item::InputScale, Item::Duration, Item::MoveInput],
                    ]
                };
                let items: Vec<Item> = blocks.concat();
                let label = self.label_width(&items);
                let control = items
                    .iter()
                    .map(|i| self.control_width(*i))
                    .fold(0., f64::max);
                let needed = 2. * ROW_PAD + label + GAP + control;
                needed_width = needed_width.max(needed);
                let right = content_right(needed, layout.size.x);
                for (index, block) in blocks.into_iter().enumerate() {
                    if index > 0 {
                        top += GAP / 2.;
                    }
                    for item in block {
                        let width = self.control_width(item);
                        let row = area(left, top, right, top + item.height());
                        layout.zones.push(Zone {
                            item,
                            row,
                            control: control_rect(row, width, ROW_PAD),
                        });
                        layout.blocks.push(row);
                        top += item.height() + ROW_GAP;
                    }
                }
                if self.page == Page::Presentation {
                    top += 8.;
                    let width =
                        self.width(self.text(Key::PointerReset), LABEL_SIZE, TextStyle::Body);
                    let row = area(left, top, left + width + 8., top + LINK);
                    layout.zones.push(Zone {
                        item: Item::Reset,
                        row,
                        control: row,
                    });
                    top += LINK;
                }
            }
            Page::Shortcuts => {
                let items: Vec<Item> = SHORTCUT_ROWS.map(Item::Shortcut).to_vec();
                let label = self.label_width(&items);
                let control = items
                    .iter()
                    .map(|i| self.control_width(*i))
                    .fold(0., f64::max);
                let groups = SHORTCUT_GROUPS
                    .iter()
                    .map(|(key, _)| self.width(self.text(*key), LABEL_SIZE, TextStyle::Strong))
                    .fold(0., f64::max);
                let needed = (2. * ROW_PAD + label + GAP + control).max(groups);
                needed_width = needed_width.max(needed);
                let right = content_right(needed, layout.size.x);
                for (index, (key, rows)) in SHORTCUT_GROUPS.iter().enumerate() {
                    if index > 0 {
                        top += 6.;
                    }
                    layout
                        .groups
                        .push((*key, area(left, top, right, top + GROUP)));
                    top += GROUP;
                    for row_item in rows.iter() {
                        let item = Item::Shortcut(*row_item);
                        let row = area(left, top, right, top + ROW);
                        layout.zones.push(Zone {
                            item,
                            row,
                            control: control_rect(row, self.control_width(item), ROW_PAD),
                        });
                        layout.blocks.push(row);
                        top += ROW + ROW_GAP;
                    }
                }
            }
            Page::Pointer => {
                let headers: Vec<Item> = CARDS.map(Item::Card).to_vec();
                let fields: Vec<Item> = CARDS
                    .iter()
                    .flat_map(|card| card.fields().iter().map(|f| Item::Field(*f)))
                    .collect();
                let header_text = CARDS
                    .iter()
                    .map(|card| {
                        self.width(self.text(card.title()), LABEL_SIZE, TextStyle::Body)
                            .max(self.summary_width(*card))
                    })
                    .fold(0., f64::max)
                    + ICON
                    + ICON_GAP;
                let header = 2. * ROW_PAD + header_text + GAP + self.control_width(headers[0]);
                let label = self.label_width(&fields);
                let control = fields
                    .iter()
                    .map(|i| self.control_width(*i))
                    .fold(0., f64::max);
                let body = 2. * ROW_PAD + FIELD_INDENT + label + GAP + control;
                let cards_width = header.max(body);
                needed_width = needed_width.max(cards_width + MARGIN + PREVIEW.x);
                let right = if natural {
                    left + cards_width
                } else {
                    layout.size.x - MARGIN - PREVIEW.x - MARGIN
                };
                layout.preview = area(
                    right + MARGIN,
                    top,
                    right + MARGIN + PREVIEW.x,
                    top + PREVIEW.y,
                );
                for card in CARDS {
                    let row = area(left, top, right, top + CARD_HEADER);
                    layout.zones.push(Zone {
                        item: Item::Card(card),
                        row,
                        control: control_rect(row, self.control_width(Item::Card(card)), ROW_PAD),
                    });
                    let block_top = top;
                    top += CARD_HEADER;
                    if self.open == Some(card) {
                        let inset_top = top;
                        top += 4.;
                        for field in card.fields() {
                            // Without its own condition met, a dependent setting would do nothing:
                            // it is left out, except to measure the page.
                            if !natural && !self.visible(*field) {
                                continue;
                            }
                            let item = Item::Field(*field);
                            let row = area(left + FIELD_INDENT, top, right, top + FIELD_ROW);
                            layout.zones.push(Zone {
                                item,
                                row,
                                control: control_rect(row, self.control_width(item), ROW_PAD),
                            });
                            top += FIELD_ROW;
                        }
                        top += 6.;
                        layout.insets.push(area(left, inset_top, right, top));
                    }
                    layout.blocks.push(area(left, block_top, right, top));
                    top += CARD_GAP;
                }
                top += 8.;
                let width = self.width(self.text(Key::PointerReset), LABEL_SIZE, TextStyle::Body);
                let row = area(left, top, left + width + 8., top + LINK);
                layout.zones.push(Zone {
                    item: Item::Reset,
                    row,
                    control: row,
                });
                top += LINK;
                top = top.max(layout.preview.max.y);
            }
        }
        let nav_height = NAV_PAD + 4. + PAGES.len() as f64 * (NAV_ROW + 2.);
        let needed = Point {
            x: left + needed_width + MARGIN,
            y: (top + MARGIN).max(nav_height),
        };
        self.layout = layout;
        needed
    }
    fn summary_width(&self, card: Card) -> f64 {
        self.width(&self.summary(card), NOTE_SIZE, TextStyle::Caption)
    }

    /// The texts that would not fit their place in the current page, measured by `measure`
    /// as `fit` does: empty when every label, value and keycap has room.
    pub fn overflows<E>(
        &self,
        mut measure: impl FnMut(&str, f64, TextStyle) -> Result<f64, E>,
    ) -> Result<Vec<String>, E> {
        let mut cut = Vec::new();
        let window = area(0., 0., self.layout.size.x, self.layout.size.y);
        for zone in &self.layout.zones {
            if !window.contains_rect(zone.row) {
                cut.push(format!("{:?} outside the window", zone.item));
            }
            let label = self.label(zone.item);
            let room = match zone.item {
                Item::Reset => zone.row.size().x,
                item => zone.control.min.x - zone.row.min.x - item.label_left() - 4.,
            };
            if measure(&label, LABEL_SIZE, TextStyle::Body)? > room + 0.5 {
                cut.push(label);
            }
            if let Some(key) = zone.item.note() {
                let note = self.text(key);
                if measure(note, NOTE_SIZE, TextStyle::Caption)? > room + 0.5 {
                    cut.push(note.into());
                }
            }
            let names = self.segment_names(zone.item);
            if !names.is_empty() {
                let widths = self.segment_widths(zone.item);
                for (name, width) in names.iter().zip(widths) {
                    if measure(name, SMALL_SIZE, TextStyle::Body)? > width - 8. {
                        cut.push(name.clone());
                    }
                }
                if zone.control.min.x < zone.row.min.x {
                    cut.push(format!("{:?} segments", zone.item));
                }
            }
        }
        for (page, r) in &self.layout.nav {
            let text = self.text(page.name());
            if measure(text, LABEL_SIZE, TextStyle::Body)?
                > r.size().x - 3. * NAV_TEXT - NAV_ICON + 0.5
            {
                cut.push(text.into());
            }
        }
        let title = self.text(self.page.name());
        if self.layout.title.max.x > self.layout.size.x
            || measure(title, TITLE_SIZE, TextStyle::Title)? > self.layout.title.size().x + 0.5
        {
            cut.push(title.into());
        }
        Ok(cut)
    }

    /// Advances the preview to `now`; true when it changed and must be painted again.
    pub fn tick(&mut self, now: u64) -> bool {
        let r = self.layout.preview;
        if self.page != Page::Pointer || r.size().x <= 0. {
            return false;
        }
        let t = now as f64 / 1000.;
        let reach = Point {
            x: (r.size().x * 0.5 - 30.).max(0.),
            y: (r.size().y * 0.5 - 30.).max(0.),
        };
        // A slow figure of eight, in preview coordinates (logical pixels, scale applied later).
        let at = Point {
            x: (t * 0.9).sin() * reach.x,
            y: (t * 1.7).sin() * reach.y,
        };
        self.preview.moved(at, now, PREVIEW_SCALE, &self.settings);
        if now.saturating_sub(self.last_click) >= PREVIEW_CLICK_MS {
            self.last_click = now;
            self.clicks += 1;
            let button = if self.clicks.is_multiple_of(2) {
                MouseButton::Right
            } else {
                MouseButton::Left
            };
            if self.settings.click != ClickStyle::Off {
                self.preview.clicked(at, button, now);
            }
        }
        self.preview.step(now, &self.settings);
        true
    }

    /// Whether a dependent setting has an effect with the other choices: shown only then.
    fn visible(&self, field: Field) -> bool {
        match field {
            Field::Volume => self.settings.sound != ClickSound::Off,
            Field::LoupeHold => self.settings.loupe_shortcut.is_some(),
            _ => true,
        }
    }
    /// Whether it is on screen now.
    pub fn is_shown(&self, shown: Shown) -> bool {
        match shown {
            Shown::Board => self.values.board,
            Shown::PointerEffects => self.halo_on,
            Shown::Magnifier => self.loupe_on,
            Shown::Keyboard => self.values.keyboard,
            Shown::Mouse => self.values.mouse,
            Shown::TemporaryShapes => self.values.temporary_shapes,
        }
    }
    pub fn card_on(&self, card: Card) -> bool {
        let s = &self.settings;
        match card {
            Card::Halo => s.halo_shape != HaloShape::Hidden,
            Card::Trail => s.trail != TrailStyle::Off,
            Card::Clicks => s.click != ClickStyle::Off,
            Card::Spotlight => s.spotlight != SpotlightShape::Off,
            Card::Loupe => self.loupe_on,
        }
    }
    /// The field whose enum value a card's switch sets: off is index 0.
    fn style_field(card: Card) -> Option<Field> {
        match card {
            Card::Halo => Some(Field::HaloShape),
            Card::Trail => Some(Field::Trail),
            Card::Clicks => Some(Field::Click),
            Card::Spotlight => Some(Field::Spotlight),
            Card::Loupe => None,
        }
    }
    fn set_card(&mut self, card: Card, on: bool) {
        match Self::style_field(card) {
            Some(field) => {
                let slot = card.slot();
                if on {
                    self.set_choice(field, self.remembered[slot]);
                } else {
                    let current = self.choice(field);
                    if current > 0 {
                        self.remembered[slot] = current;
                    }
                    self.set_choice(field, 0);
                }
            }
            None => self.loupe_on = on,
        }
    }
    fn choice(&self, field: Field) -> usize {
        let s = &self.settings;
        match field {
            Field::HaloShape => s.halo_shape.index(),
            Field::Movement => s.follow.index(),
            Field::Trail => s.trail.index(),
            Field::TrailLength => s.trail_length.index(),
            Field::Click => s.click.index(),
            Field::Sound => s.sound.index(),
            Field::Volume => s.volume.index(),
            Field::Spotlight => s.spotlight.index(),
            _ => 0,
        }
    }
    fn set_choice(&mut self, field: Field, index: usize) {
        fn pick<T: Copy>(all: &[T], index: usize) -> T {
            all[index % all.len()]
        }
        let s = &mut self.settings;
        match field {
            Field::HaloShape => s.halo_shape = pick(HaloShape::ALL, index),
            Field::Movement => s.follow = pick(Follow::ALL, index),
            Field::Trail => s.trail = pick(TrailStyle::ALL, index),
            Field::TrailLength => s.trail_length = pick(TrailLength::ALL, index),
            Field::Click => s.click = pick(ClickStyle::ALL, index),
            Field::Sound => s.sound = pick(ClickSound::ALL, index),
            Field::Volume => s.volume = pick(Volume::ALL, index),
            Field::Spotlight => s.spotlight = pick(SpotlightShape::ALL, index),
            _ => {}
        }
    }
    fn number(&self, field: Field) -> u32 {
        let s = &self.settings;
        match field {
            Field::HaloSize => s.halo_diameter,
            Field::HaloOpacity => s.halo_opacity,
            Field::HaloSoftness => s.halo_softness,
            Field::HaloThickness => s.halo_thickness,
            Field::SpotlightSize => s.spotlight_size,
            Field::SpotlightDim => s.spotlight_dim,
            Field::LoupeZoom => s.loupe_zoom,
            Field::LoupeSize => s.loupe_size,
            _ => 0,
        }
    }
    fn set_number(&mut self, field: Field, value: u32) {
        let ((low, high), step) = field.range();
        // Values land on the step grid from the low end, inside the range.
        let value = low + ((value.clamp(low, high) - low + step / 2) / step) * step;
        self.set_exact(field, value.min(high));
    }
    /// Sets a slider's value as given, inside its range: a dragged knob is not held to steps.
    fn set_exact(&mut self, field: Field, value: u32) {
        let ((low, high), _) = field.range();
        let value = value.clamp(low, high);
        let s = &mut self.settings;
        match field {
            Field::HaloSize => s.halo_diameter = value,
            Field::HaloOpacity => s.halo_opacity = value,
            Field::HaloSoftness => s.halo_softness = value,
            Field::HaloThickness => s.halo_thickness = value,
            Field::SpotlightSize => s.spotlight_size = value,
            Field::SpotlightDim => s.spotlight_dim = value,
            Field::LoupeZoom => s.loupe_zoom = value,
            Field::LoupeSize => s.loupe_size = value,
            _ => {}
        }
    }
    fn color(&self, field: Field) -> [u8; 3] {
        match field {
            Field::HaloColor => self.halo_color,
            Field::LeftColor => self.settings.left_click_color,
            _ => self.settings.right_click_color,
        }
    }
    fn set_color(&mut self, field: Field, rgb: [u8; 3]) {
        match field {
            Field::HaloColor => self.halo_color = rgb,
            Field::LeftColor => self.settings.left_click_color = rgb,
            _ => self.settings.right_click_color = rgb,
        }
    }
    fn switch(&self, field: Field) -> bool {
        match field {
            Field::LoupeHold => self.settings.loupe_hold,
            _ => self.settings.spotlight_hidden_moving,
        }
    }
    fn toggle(&mut self, field: Field) {
        match field {
            Field::LoupeHold => self.settings.loupe_hold ^= true,
            _ => self.settings.spotlight_hidden_moving ^= true,
        }
    }
    fn changed(field: Option<Field>) -> Action {
        Action::Changed {
            save: true,
            sound: matches!(field, Some(Field::Sound | Field::Volume)),
        }
    }
    /// A change of a card's settings shows its effect: the loupe's turns the loupe on, any
    /// other turns its card and the pointer effects on.
    fn shown(&mut self, card: Card, action: Action) -> Action {
        if matches!(action, Action::Changed { .. }) {
            if card == Card::Loupe {
                self.loupe_on = true;
            } else {
                if !self.card_on(card) {
                    self.set_card(card, true);
                }
                self.halo_on = true;
            }
        }
        action
    }
    /// Moves the value of `item` by `steps` (negative: back).
    fn step(&mut self, item: Item, steps: i32) -> Action {
        match item {
            Item::InputScale | Item::Duration => {
                let count = if item == Item::InputScale {
                    INPUT_SCALES.len()
                } else {
                    DURATIONS.len()
                } as i32;
                let current = self.segment(item).unwrap_or(0) as i32;
                let next = (current + steps).clamp(0, count - 1) as usize;
                if next as i32 == current {
                    return Action::None;
                }
                Action::Command(if item == Item::InputScale {
                    Command::InputScale(INPUT_SCALES[next])
                } else {
                    Command::Duration(DURATIONS[next])
                })
            }
            Item::Startup if (steps > 0) != self.values.startup => {
                Action::Command(Command::Startup)
            }
            Item::Live if (steps > 0) != self.values.live => Action::Command(Command::Live),
            Item::MoveInput if (steps > 0) != self.values.moving_input => {
                Action::Command(Command::MoveInput)
            }
            Item::Shown(shown) if (steps > 0) != self.is_shown(shown) => {
                Action::Command(Command::Show(shown))
            }
            Item::Card(card) => {
                if (steps > 0) == self.card_on(card) {
                    return Action::None;
                }
                self.toggle_card(card)
            }
            Item::Nav(page) => {
                let index = PAGES.iter().position(|p| *p == page).unwrap_or(0) as i32;
                let next = PAGES[(index + steps).clamp(0, PAGES.len() as i32 - 1) as usize];
                self.show_page(next);
                self.focus = Item::Nav(next);
                Action::Repaint
            }
            Item::Field(field) => {
                let action = self.step_field(field, steps);
                self.shown(field.card(), action)
            }
            _ => Action::None,
        }
    }
    fn step_field(&mut self, field: Field, steps: i32) -> Action {
        match field.control() {
            Control::Segments => {
                let options = field.options();
                let current = self.segment(Item::Field(field)).unwrap_or(0) as i32;
                let next = (current + steps).rem_euclid(options.len() as i32) as usize;
                self.set_choice(field, options[next].1);
            }
            Control::Slider => {
                let (_, step) = field.range();
                let value = self.number(field) as i64 + steps as i64 * step as i64;
                self.set_number(field, value.max(0) as u32);
            }
            Control::Colors => {
                let current = COLORS
                    .iter()
                    .position(|rgb| *rgb == self.color(field))
                    .map_or(if steps > 0 { -1 } else { 0 }, |index| index as i32);
                let next = (current + steps).rem_euclid(COLORS.len() as i32) as usize;
                self.set_color(field, COLORS[next]);
            }
            Control::Switch => {
                if self.switch(field) == (steps > 0) {
                    return Action::None;
                }
                self.toggle(field);
            }
        }
        Self::changed(Some(field))
    }
    fn toggle_card(&mut self, card: Card) -> Action {
        let on = !self.card_on(card);
        self.set_card(card, on);
        if on && card != Card::Loupe {
            self.halo_on = true;
        }
        Self::changed(None)
    }
    pub fn show_page(&mut self, page: Page) {
        self.page = page;
        self.drag = None;
        self.arrange();
    }
    fn open_card(&mut self, card: Card) {
        self.open = if self.open == Some(card) {
            None
        } else {
            Some(card)
        };
        self.arrange();
    }

    /// Where the control of `item` sits on the current page, in logical pixels.
    pub fn control(&self, item: Item) -> Option<Rect> {
        self.layout.zone(item).map(|zone| zone.control)
    }
    fn at(&self, p: Point) -> Option<Item> {
        if let Some((page, _)) = self.layout.nav.iter().find(|(_, r)| r.contains(p)) {
            return Some(Item::Nav(*page));
        }
        self.layout
            .zones
            .iter()
            .find(|zone| zone.row.contains(p))
            .map(|zone| zone.item)
    }
    fn press_unseen(&mut self, p: Point) -> Action {
        self.keyboard = false;
        let Some(item) = self.at(p) else {
            return Action::None;
        };
        self.focus = item;
        let Some(zone) = self.layout.zone(item) else {
            // A page of the navigation.
            if let Item::Nav(page) = item {
                self.show_page(page);
            }
            return Action::Repaint;
        };
        let r = zone.control;
        match item {
            Item::Nav(_) => Action::Repaint,
            Item::Startup => Action::Command(Command::Startup),
            Item::Live => Action::Command(Command::Live),
            Item::Shortcut(row) => {
                if let Some(index) = row.pointer_index()
                    && !self.values.shortcuts[row.index()].caps.is_empty()
                    && p.x >= r.max.x - CLEAR
                {
                    return self.clear_shortcut(index);
                }
                Action::Command(Command::EditShortcut(row))
            }
            Item::InputScale | Item::Duration => match self.segment_at(item, r, p) {
                Some(index) => {
                    let current = self.segment(item);
                    if current == Some(index) {
                        Action::Repaint
                    } else {
                        Action::Command(if item == Item::InputScale {
                            Command::InputScale(INPUT_SCALES[index])
                        } else {
                            Command::Duration(DURATIONS[index])
                        })
                    }
                }
                None => Action::Repaint,
            },
            Item::MoveInput => Action::Command(Command::MoveInput),
            Item::Shown(shown) => Action::Command(Command::Show(shown)),
            Item::Card(card) => {
                if r.expanded(4.).contains(p) && p.x >= r.max.x - SWITCH.x - 4. {
                    self.toggle_card(card)
                } else {
                    self.open_card(card);
                    Action::Repaint
                }
            }
            Item::Field(field) => {
                let action = self.press_field(field, r, p);
                self.shown(field.card(), action)
            }
            Item::Reset => self.reset(),
        }
    }
    fn segment_at(&self, item: Item, r: Rect, p: Point) -> Option<usize> {
        if !r.expanded(2.).contains(p) {
            return None;
        }
        let mut x = r.min.x + 2.;
        for (index, width) in self.segment_widths(item).into_iter().enumerate() {
            if p.x < x + width {
                return Some(index);
            }
            x += width;
        }
        None
    }
    fn press_field(&mut self, field: Field, r: Rect, p: Point) -> Action {
        match field.control() {
            Control::Segments => match self.segment_at(Item::Field(field), r, p) {
                Some(index) => {
                    let value = field.options()[index].1;
                    if self.choice(field) == value {
                        return Action::Repaint;
                    }
                    self.set_choice(field, value);
                    Self::changed(Some(field))
                }
                None => Action::Repaint,
            },
            Control::Slider => {
                if !r.expanded(4.).contains(p) {
                    return Action::Repaint;
                }
                self.drag = Some((field, 0.));
                self.slide(field, p.x, false)
            }
            Control::Colors => {
                let picked = (0..COLORS.len())
                    .find(|index| (swatch_center(r, *index, 1.) - p).length() <= SWATCH + 3.);
                match picked {
                    Some(index) => {
                        self.set_color(field, COLORS[index]);
                        Self::changed(Some(field))
                    }
                    None => Action::Repaint,
                }
            }
            Control::Switch => {
                self.toggle(field);
                Self::changed(Some(field))
            }
        }
    }
    fn slide(&mut self, field: Field, x: f64, done: bool) -> Action {
        let Some(zone) = self.layout.zone(Item::Field(field)) else {
            return Action::None;
        };
        let r = zone.control;
        let ((low, high), _) = field.range();
        let fraction = ((x - r.min.x - 8.) / (r.size().x - 16.)).clamp(0., 1.);
        let value = low as f64 + fraction * (high - low) as f64;
        let before = self.number(field);
        self.set_exact(field, value.round() as u32);
        if !done {
            self.drag = Some((field, fraction));
        }
        if done || self.number(field) != before {
            Action::Changed {
                save: done,
                sound: false,
            }
        } else {
            Action::Repaint
        }
    }
    /// A press at `p`, in logical pixels.
    pub fn press(&mut self, p: Point) -> Action {
        self.press_unseen(p)
    }
    /// The pointer moved to `p` with the button held.
    pub fn drag(&mut self, p: Point) -> Action {
        match self.drag {
            Some((field, _)) => {
                let action = self.slide(field, p.x, false);
                self.shown(field.card(), action)
            }
            None => Action::None,
        }
    }
    /// The button was released at `p`: a dragged slider saves its value.
    pub fn release(&mut self, p: Point) -> Action {
        match self.drag.take() {
            Some((field, _)) => {
                let action = self.slide(field, p.x, true);
                self.shown(field.card(), action)
            }
            None => Action::None,
        }
    }
    /// The wheel turned `notches` (positive: away from the user) over `p`.
    pub fn wheel(&mut self, p: Point, notches: i32) -> Action {
        if notches == 0 {
            return Action::None;
        }
        match self.at(p) {
            Some(item @ Item::Field(_)) => {
                self.focus = item;
                self.step(item, notches.signum())
            }
            _ => Action::None,
        }
    }
    /// Every item of the window in focus order: the pages, then the current page's rows.
    fn order(&self) -> Vec<Item> {
        PAGES
            .map(Item::Nav)
            .into_iter()
            .chain(self.layout.zones.iter().map(|zone| zone.item))
            .collect()
    }
    pub fn key(&mut self, key: KeyInput) -> Action {
        self.keyboard = true;
        let order = self.order();
        let index = order.iter().position(|f| *f == self.focus).unwrap_or(0) as i32;
        let moved = |delta: i32| order[(index + delta).rem_euclid(order.len() as i32) as usize];
        match (key, self.focus) {
            (KeyInput::Escape, _) => Action::Close,
            (KeyInput::Next | KeyInput::Down, _) => {
                self.focus = moved(1);
                Action::Repaint
            }
            (KeyInput::Previous | KeyInput::Up, _) => {
                self.focus = moved(-1);
                Action::Repaint
            }
            (KeyInput::Left, item) => self.step(item, -1),
            (KeyInput::Right, item) => self.step(item, 1),
            (KeyInput::Activate, item) => match item {
                Item::Nav(page) => {
                    self.show_page(page);
                    Action::Repaint
                }
                Item::Startup => Action::Command(Command::Startup),
                Item::Live => Action::Command(Command::Live),
                Item::Shortcut(row) => Action::Command(Command::EditShortcut(row)),
                Item::MoveInput => Action::Command(Command::MoveInput),
                Item::Shown(shown) => Action::Command(Command::Show(shown)),
                Item::Card(card) => {
                    self.open_card(card);
                    Action::Repaint
                }
                Item::Field(field) if field.control() == Control::Switch => {
                    self.toggle(field);
                    let action = Self::changed(Some(field));
                    self.shown(field.card(), action)
                }
                Item::Reset => self.reset(),
                other => self.step(other, 1),
            },
            (KeyInput::Delete, Item::Shortcut(row)) => match row.pointer_index() {
                Some(index) => self.clear_shortcut(index),
                None => Action::None,
            },
            _ => Action::None,
        }
    }
    fn clear_shortcut(&mut self, index: usize) -> Action {
        *self.settings.shortcut_mut(index) = None;
        self.values.shortcuts[ShortcutRow::ShowClick.index() + index]
            .caps
            .clear();
        Action::Changed {
            save: true,
            sound: false,
        }
    }
    /// Restores the current page's defaults: the pointer's keeps its shortcuts, chosen with care.
    fn reset(&mut self) -> Action {
        match self.page {
            Page::Presentation => Action::Command(Command::PresentationDefaults),
            Page::Pointer => {
                let shortcuts = self.settings.shortcuts();
                let mut settings = PointerSettings::default();
                for (index, shortcut) in shortcuts.into_iter().enumerate() {
                    *settings.shortcut_mut(index) = shortcut;
                }
                self.settings = settings;
                self.halo_color =
                    shotoshot_core::settings::PresentationSettings::default().halo_color;
                self.halo_on = true;
                Self::changed(None)
            }
            _ => Action::None,
        }
    }
}

const fn area(left: f64, top: f64, right: f64, bottom: f64) -> Rect {
    Rect {
        min: Point { x: left, y: top },
        max: Point {
            x: right,
            y: bottom,
        },
    }
}
/// A control of `width` at the right of `row`, centred on it.
fn control_rect(row: Rect, width: f64, pad: f64) -> Rect {
    let middle = row.center().y;
    area(
        row.max.x - pad - width,
        middle - CONTROL_HEIGHT / 2.,
        row.max.x - pad,
        middle + CONTROL_HEIGHT / 2.,
    )
}
fn swatch_center(r: Rect, index: usize, scale: f64) -> Point {
    Point {
        x: r.min.x + (SWATCH_STEP / 2. + SWATCH_STEP * index as f64) * scale,
        y: r.center().y,
    }
}

#[cfg(test)]
mod tests;
