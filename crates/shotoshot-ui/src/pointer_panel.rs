//! ROLE: The pointer effects window, without its operating system: which rows it shows, where
//! each sits for the texts of a language, how it is painted with its live preview, and what a
//! click, a drag, the wheel or a key changes.
//! OWNER: The platform window holds one `Panel` while it is open and applies its `Action`s.
//! INVARIANTS: Layout is in 96-DPI logical pixels, measured from the texts of the language: no
//! text is cut. Painting and hit testing read the same `PanelLayout`.
use crate::{Canvas, Ink, pointer};
use shotoshot_core::{
    Point, Rect,
    pointer::{
        Button as MouseButton, ClickSound, ClickStyle, Effects, Follow, HALO_DIAMETER_RANGE,
        HALO_OPACITY_RANGE, HALO_SOFTNESS_RANGE, HALO_THICKNESS_RANGE, HaloShape, PULSE_MS,
        PointerSettings, SPOTLIGHT_DIM_RANGE, SPOTLIGHT_SIZE_RANGE, SpotlightShape, TrailLength,
        TrailStyle, Volume,
    },
};
use shotoshot_text::{Key, Language, tr_in};

pub const BACKGROUND: [u8; 3] = [0x20, 0x20, 0x20];
const SURFACE: [u8; 3] = [0x2b, 0x2b, 0x2b];
const TEXT: [u8; 3] = [0xf0, 0xf0, 0xf0];
const SECONDARY: [u8; 3] = [0xc6, 0xc6, 0xc6];
const MUTED: [u8; 3] = [0x9a, 0x9a, 0x9a];
const RULE: [u8; 3] = [0x49, 0x49, 0x49];
const ACCENT: [u8; 3] = [0x60, 0xcd, 0xff];
/// The two backgrounds of the preview: a dark editor and a light page.
const PREVIEW_DARK: [u8; 3] = [0x1b, 0x1e, 0x24];
const PREVIEW_LIGHT: [u8; 3] = [0xf3, 0xf4, 0xf6];

pub const TITLE_SIZE: f64 = 16.;
pub const LABEL_SIZE: f64 = 13.;
pub const CHOICE_SIZE: f64 = 12.;
pub const SECTION_SIZE: f64 = 12.;
const MARGIN: f64 = 20.;
const TITLE_HEIGHT: f64 = 52.;
const ROW: f64 = 32.;
const SECTION: f64 = 30.;
const CONTROL_HEIGHT: f64 = 24.;
const LABEL_GAP: f64 = 16.;
const COLUMN_GAP: f64 = 32.;
const ARROW: f64 = 24.;
const CLEAR: f64 = 22.;
const MIN_CONTROL: f64 = 172.;
const PREVIEW_WIDTH: f64 = 250.;
const PREVIEW_GAP: f64 = 8.;
const BUTTON_HEIGHT: f64 = 32.;
const BUTTON_PADDING: f64 = 16.;
const MIN_BUTTON: f64 = 88.;
const BUTTON_SPACING: f64 = 8.;
const RADIUS: f64 = 5.;
const SWATCH: f64 = 8.;
const SWITCH: Point = Point { x: 28., y: 14. };
/// The preview draws the effects smaller than life so that they fit beside the settings.
const PREVIEW_SCALE: f64 = 0.7;
/// The preview pointer clicks this often, left then right.
const PREVIEW_CLICK_MS: u64 = 1400;

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

/// One setting of the window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    HaloOn,
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
    ClickShortcut,
    Spotlight,
    SpotlightSize,
    SpotlightDim,
    SpotlightMoving,
    SpotlightShortcut,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Control {
    /// One value at a time between two arrows.
    Choice,
    Slider,
    Colors,
    Switch,
    Shortcut,
}
/// The two columns of settings, each a list of sections.
const COLUMNS: [&[(Key, &[Field])]; 2] = [
    &[
        (
            Key::PointerSectionHalo,
            &[
                Field::HaloOn,
                Field::HaloShape,
                Field::HaloColor,
                Field::HaloSize,
                Field::HaloOpacity,
                Field::HaloSoftness,
                Field::HaloThickness,
                Field::Movement,
            ],
        ),
        (
            Key::PointerSectionTrail,
            &[Field::Trail, Field::TrailLength],
        ),
    ],
    &[
        (
            Key::PointerSectionClicks,
            &[
                Field::Click,
                Field::LeftColor,
                Field::RightColor,
                Field::Sound,
                Field::Volume,
                Field::ClickShortcut,
            ],
        ),
        (
            Key::PointerSectionSpotlight,
            &[
                Field::Spotlight,
                Field::SpotlightSize,
                Field::SpotlightDim,
                Field::SpotlightMoving,
                Field::SpotlightShortcut,
            ],
        ),
    ],
];
/// Every field in focus order: down the first column, then the second.
pub const FIELDS: [Field; 21] = [
    Field::HaloOn,
    Field::HaloShape,
    Field::HaloColor,
    Field::HaloSize,
    Field::HaloOpacity,
    Field::HaloSoftness,
    Field::HaloThickness,
    Field::Movement,
    Field::Trail,
    Field::TrailLength,
    Field::Click,
    Field::LeftColor,
    Field::RightColor,
    Field::Sound,
    Field::Volume,
    Field::ClickShortcut,
    Field::Spotlight,
    Field::SpotlightSize,
    Field::SpotlightDim,
    Field::SpotlightMoving,
    Field::SpotlightShortcut,
];
impl Field {
    fn control(self) -> Control {
        match self {
            Self::HaloShape
            | Self::Movement
            | Self::Trail
            | Self::TrailLength
            | Self::Click
            | Self::Sound
            | Self::Volume
            | Self::Spotlight => Control::Choice,
            Self::HaloSize
            | Self::HaloOpacity
            | Self::HaloSoftness
            | Self::HaloThickness
            | Self::SpotlightSize
            | Self::SpotlightDim => Control::Slider,
            Self::HaloColor | Self::LeftColor | Self::RightColor => Control::Colors,
            Self::HaloOn | Self::SpotlightMoving => Control::Switch,
            Self::ClickShortcut | Self::SpotlightShortcut => Control::Shortcut,
        }
    }
    fn label(self) -> Key {
        match self {
            Self::HaloOn => Key::PointerShowHalo,
            Self::HaloShape | Self::Spotlight => Key::PointerShape,
            Self::HaloColor => Key::PointerColor,
            Self::HaloSize | Self::SpotlightSize => Key::PointerSize,
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
            Self::ClickShortcut => Key::PointerShowClick,
            Self::SpotlightDim => Key::PointerDarkness,
            Self::SpotlightMoving => Key::PointerHideMoving,
            Self::SpotlightShortcut => Key::PointerHold,
        }
    }
    /// The names of a choice's values, in the order of their enum.
    fn choices(self) -> &'static [Key] {
        match self {
            Self::HaloShape => &[
                Key::PointerHidden,
                Key::PointerDisc,
                Key::PointerRing,
                Key::PointerNeon,
            ],
            Self::Movement => &[Key::PointerInstant, Key::PointerSmooth],
            Self::Trail => &[
                Key::PointerOff,
                Key::PointerLine,
                Key::PointerDots,
                Key::PointerSparks,
                Key::PointerStars,
            ],
            Self::TrailLength => &[
                Key::PointerShort,
                Key::PointerMediumLength,
                Key::PointerLong,
            ],
            Self::Click => &[
                Key::PointerOff,
                Key::PointerRipple,
                Key::PointerClickRing,
                Key::PointerWaves,
            ],
            Self::Sound => &[Key::PointerOff, Key::PointerSoft, Key::PointerClickSound],
            Self::Volume => &[Key::PointerLow, Key::PointerMediumVolume, Key::PointerHigh],
            Self::Spotlight => &[
                Key::PointerOff,
                Key::PointerCircle,
                Key::PointerHorizontal,
                Key::PointerVertical,
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
            _ => ((0, 0), 1),
        }
    }
    /// Which of the two shortcuts a shortcut row edits.
    pub fn shortcut_index(self) -> Option<usize> {
        match self {
            Self::ClickShortcut => Some(0),
            Self::SpotlightShortcut => Some(1),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    Reset,
    Undo,
    Close,
}
const BUTTONS: [(Button, Key); 3] = [
    (Button::Reset, Key::PointerReset),
    (Button::Undo, Key::PointerUndo),
    (Button::Close, Key::ToolbarClose),
];
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Focus {
    Field(Field),
    Button(Button),
}

/// What the platform does after an input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    None,
    /// Only the preview changed.
    Repaint,
    /// Apply the settings to the live pointer; `save` when the change is complete (not while a
    /// slider is dragged). `sound` when the click sound was changed: let it be heard.
    Changed {
        save: bool,
        sound: bool,
    },
    /// Record a shortcut for row 0 (show a click) or 1 (spotlight).
    EditShortcut(usize),
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

/// Where everything sits, in logical pixels.
#[derive(Clone, Debug, PartialEq)]
pub struct PanelLayout {
    pub size: Point,
    title: Rect,
    /// Every section title with its text.
    sections: Vec<(Key, Rect)>,
    /// Every row: its field, label area and control area.
    rows: Vec<(Field, Rect, Rect)>,
    preview: [Rect; 2],
    buttons: Vec<(Button, Rect)>,
}
impl PanelLayout {
    fn control(&self, field: Field) -> Option<Rect> {
        self.rows
            .iter()
            .find(|(row, ..)| *row == field)
            .map(|(_, _, control)| *control)
    }
}

pub struct Panel {
    pub settings: PointerSettings,
    pub halo_color: [u8; 3],
    /// Whether the halo is on screen. Any other change made here turns it on: what is set
    /// here is meant to be seen. Only its own switch turns it off.
    pub halo_on: bool,
    /// What the window started from, for "Undo changes".
    opened: (PointerSettings, [u8; 3]),
    pub focus: Focus,
    /// The slider being dragged, and where its knob is, from 0 to 1: the knob follows the
    /// pointer exactly even where the value moves by whole steps.
    drag: Option<(Field, f64)>,
    /// The two shortcuts as the interface writes them, empty when not set.
    pub shortcuts: [String; 2],
    pub language: Language,
    pub layout: PanelLayout,
    preview: Effects,
    last_click: u64,
    clicks: u64,
}

impl Panel {
    pub fn new(
        settings: PointerSettings,
        halo_color: [u8; 3],
        halo_on: bool,
        language: Language,
    ) -> Self {
        Self {
            opened: (settings.clone(), halo_color),
            settings,
            halo_color,
            halo_on,
            focus: Focus::Field(Field::HaloOn),
            drag: None,
            shortcuts: Default::default(),
            language,
            layout: PanelLayout {
                size: Point { x: 0., y: 0. },
                title: Rect::default(),
                sections: Vec::new(),
                rows: Vec::new(),
                preview: [Rect::default(); 2],
                buttons: Vec::new(),
            },
            preview: Effects::default(),
            last_click: 0,
            clicks: 0,
        }
    }
    fn text(&self, key: Key) -> &'static str {
        tr_in(self.language, key)
    }
    /// The widest text a control shows, the longest label: `measure` gives a text's width at a
    /// size, in logical pixels.
    pub fn fit<E>(
        &mut self,
        mut measure: impl FnMut(&str, f64) -> Result<f64, E>,
    ) -> Result<(), E> {
        let mut label: f64 = 0.;
        let mut control: f64 = MIN_CONTROL;
        for field in FIELDS {
            label = label.max(measure(self.text(field.label()), LABEL_SIZE)?.ceil());
            for choice in field.choices() {
                let width = measure(self.text(*choice), CHOICE_SIZE)?.ceil();
                control = control.max(width + 2. * ARROW + 8.);
            }
        }
        for shortcut in self
            .shortcuts
            .iter()
            .map(String::as_str)
            .chain([self.text(Key::PointerNoShortcut)])
        {
            control = control.max(measure(shortcut, CHOICE_SIZE)?.ceil() + 16. + CLEAR);
        }
        let mut section: f64 = 0.;
        for (title, _) in COLUMNS.iter().flat_map(|column| column.iter()) {
            section = section.max(measure(self.text(*title), SECTION_SIZE)?.ceil());
        }
        let column = (label + LABEL_GAP + control).max(section);
        let mut sections = Vec::new();
        let mut rows = Vec::new();
        let mut bottom: f64 = TITLE_HEIGHT;
        for (index, sections_of) in COLUMNS.iter().enumerate() {
            let left = MARGIN + index as f64 * (column + COLUMN_GAP);
            let mut top = TITLE_HEIGHT;
            for (title, fields) in sections_of.iter() {
                sections.push((*title, area(left, top, left + column, top + SECTION)));
                top += SECTION;
                for field in fields.iter() {
                    let label_area = area(left, top, left + label, top + ROW);
                    let middle = top + ROW / 2.;
                    let control_area = area(
                        left + column - control,
                        middle - CONTROL_HEIGHT / 2.,
                        left + column,
                        middle + CONTROL_HEIGHT / 2.,
                    );
                    rows.push((*field, label_area, control_area));
                    top += ROW;
                }
            }
            bottom = bottom.max(top);
        }
        let preview_left = MARGIN + 2. * column + 2. * COLUMN_GAP;
        let half = ((bottom - TITLE_HEIGHT - PREVIEW_GAP) / 2.).floor();
        let preview = [
            area(
                preview_left,
                TITLE_HEIGHT,
                preview_left + PREVIEW_WIDTH,
                TITLE_HEIGHT + half,
            ),
            area(
                preview_left,
                TITLE_HEIGHT + half + PREVIEW_GAP,
                preview_left + PREVIEW_WIDTH,
                TITLE_HEIGHT + 2. * half + PREVIEW_GAP,
            ),
        ];
        let mut width = preview_left + PREVIEW_WIDTH + MARGIN;
        let button_top = bottom + 16.;
        let mut widths = Vec::new();
        for (button, key) in BUTTONS {
            let text = measure(self.text(key), LABEL_SIZE)?.ceil();
            widths.push((button, (text + 2. * BUTTON_PADDING).max(MIN_BUTTON)));
        }
        let total: f64 =
            widths.iter().map(|(_, w)| w).sum::<f64>() + BUTTON_SPACING * (widths.len() - 1) as f64;
        width = width.max(total + 2. * MARGIN).ceil();
        let mut buttons = Vec::new();
        // Restore defaults stays on the left, away from Close.
        let (reset, rest) = widths
            .split_first()
            .map(|(a, b)| (*a, b.to_vec()))
            .unwrap_or(((Button::Reset, MIN_BUTTON), Vec::new()));
        buttons.push((
            reset.0,
            area(
                MARGIN,
                button_top,
                MARGIN + reset.1,
                button_top + BUTTON_HEIGHT,
            ),
        ));
        let mut right = width - MARGIN;
        for (button, w) in rest.iter().rev() {
            buttons.push((
                *button,
                area(right - w, button_top, right, button_top + BUTTON_HEIGHT),
            ));
            right -= w + BUTTON_SPACING;
        }
        self.layout = PanelLayout {
            size: Point {
                x: width,
                y: (button_top + BUTTON_HEIGHT + MARGIN).ceil(),
            },
            title: area(MARGIN, 0., width - MARGIN, TITLE_HEIGHT),
            sections,
            rows,
            preview,
            buttons,
        };
        Ok(())
    }

    /// The texts that would not fit their place in the current layout, measured by `measure`
    /// as `fit` does: empty when every label, choice and button has room.
    pub fn overflows<E>(
        &self,
        mut measure: impl FnMut(&str, f64) -> Result<f64, E>,
    ) -> Result<Vec<String>, E> {
        let mut cut = Vec::new();
        for (field, label, control) in &self.layout.rows {
            let text = self.text(field.label());
            if measure(text, LABEL_SIZE)? > label.size().x + 0.5 {
                cut.push(text.to_string());
            }
            for choice in field.choices() {
                let text = self.text(*choice);
                if measure(text, CHOICE_SIZE)? > control.size().x - 2. * ARROW {
                    cut.push(text.to_string());
                }
            }
        }
        for (button, r) in &self.layout.buttons {
            if let Some((_, key)) = BUTTONS.iter().find(|(b, _)| b == button) {
                let text = self.text(*key);
                if measure(text, LABEL_SIZE)? > r.size().x - 8. {
                    cut.push(text.to_string());
                }
            }
        }
        for (key, r) in &self.layout.sections {
            let text = self.text(*key);
            if measure(text, SECTION_SIZE)? > r.size().x + 0.5 {
                cut.push(text.to_string());
            }
        }
        Ok(cut)
    }

    /// Advances the preview to `now`; true when it changed and must be painted again.
    pub fn tick(&mut self, now: u64) -> bool {
        let [dark, _] = self.layout.preview;
        if dark.size().x <= 0. {
            return false;
        }
        let t = now as f64 / 1000.;
        let reach = Point {
            x: (dark.size().x * 0.5 - 30.).max(0.),
            y: (dark.size().y * 0.5 - 30.).max(0.),
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

    pub fn paint<C: Canvas>(&self, canvas: &mut C, scale: f64, now: u64) -> Result<(), C::Error> {
        let zones = &self.layout;
        let px = |r: Rect| Rect {
            min: r.min * scale,
            max: r.max * scale,
        };
        let u = |value: f64| value * scale;
        canvas.fill_rect(
            px(area(0., 0., zones.size.x, zones.size.y)),
            Ink::solid(BACKGROUND),
        )?;
        let title = self.text(Key::PointerTitle);
        let extent = canvas.measure_text(title, u(TITLE_SIZE))?;
        let title_area = px(zones.title);
        canvas.text(
            title,
            Rect {
                min: Point {
                    x: title_area.min.x,
                    y: title_area.center().y - extent.y / 2.,
                },
                max: Point {
                    x: title_area.max.x,
                    y: title_area.center().y + extent.y,
                },
            },
            u(TITLE_SIZE),
            Ink::solid(TEXT),
        )?;
        for (key, r) in &zones.sections {
            let r = px(*r);
            let text = self.text(*key);
            let extent = canvas.measure_text(text, u(SECTION_SIZE))?;
            canvas.text(
                text,
                line_rect(r, extent),
                u(SECTION_SIZE),
                Ink::solid(ACCENT),
            )?;
        }
        for (field, label, control) in &zones.rows {
            let active = self.active(*field);
            let text = self.text(field.label());
            let label = px(*label);
            let extent = canvas.measure_text(text, u(LABEL_SIZE))?;
            canvas.text(
                text,
                line_rect(label, extent),
                u(LABEL_SIZE),
                Ink::solid(if active { SECONDARY } else { MUTED }),
            )?;
            self.paint_control(canvas, *field, px(*control), scale, active)?;
            if self.focus == Focus::Field(*field) {
                canvas.stroke_round_rect(
                    px(control.expanded(3.)),
                    u(RADIUS + 2.),
                    Ink::solid(ACCENT),
                    u(1.5),
                )?;
            }
        }
        self.paint_preview(canvas, scale, now)?;
        for (button, r) in &zones.buttons {
            let key = BUTTONS
                .iter()
                .find(|(b, _)| b == button)
                .map_or(Key::ToolbarClose, |(_, key)| *key);
            let focused = self.focus == Focus::Button(*button);
            let r = px(*r);
            let primary = *button == Button::Close;
            canvas.fill_round_rect(
                r,
                u(RADIUS),
                Ink::solid(if primary { ACCENT } else { SURFACE }),
            )?;
            canvas.stroke_round_rect(
                r.expanded(-u(0.5)),
                u(RADIUS),
                Ink::solid(if focused {
                    TEXT
                } else if primary {
                    ACCENT
                } else {
                    RULE
                }),
                u(if focused { 1.5 } else { 1. }),
            )?;
            centered(
                canvas,
                self.text(key),
                r,
                u(LABEL_SIZE),
                if primary { BACKGROUND } else { TEXT },
            )?;
        }
        Ok(())
    }

    /// Whether a row's setting has an effect with the other choices: dimmed when not.
    fn active(&self, field: Field) -> bool {
        let s = &self.settings;
        match field {
            Field::HaloOpacity | Field::HaloSoftness | Field::HaloThickness => {
                s.halo_shape != HaloShape::Hidden
            }
            Field::TrailLength => s.trail != TrailStyle::Off,
            Field::LeftColor | Field::RightColor => s.click != ClickStyle::Off,
            Field::Volume => s.sound != ClickSound::Off,
            Field::SpotlightSize
            | Field::SpotlightDim
            | Field::SpotlightMoving
            | Field::SpotlightShortcut => s.spotlight != SpotlightShape::Off,
            _ => true,
        }
    }

    fn paint_control<C: Canvas>(
        &self,
        canvas: &mut C,
        field: Field,
        r: Rect,
        scale: f64,
        active: bool,
    ) -> Result<(), C::Error> {
        let u = |value: f64| value * scale;
        let ink = if active { TEXT } else { MUTED };
        match field.control() {
            Control::Choice => {
                canvas.fill_round_rect(r, u(RADIUS), Ink::solid(SURFACE))?;
                canvas.stroke_round_rect(
                    r.expanded(-u(0.5)),
                    u(RADIUS),
                    Ink::solid(RULE),
                    u(1.),
                )?;
                let middle = r.center().y;
                for (x, direction) in [
                    (r.min.x + u(ARROW / 2.), -1.),
                    (r.max.x - u(ARROW / 2.), 1.),
                ] {
                    let tip = Point {
                        x: x + direction * u(2.5),
                        y: middle,
                    };
                    canvas.polyline(
                        &[
                            tip + Point {
                                x: -direction * u(5.),
                                y: -u(5.),
                            },
                            tip,
                            tip + Point {
                                x: -direction * u(5.),
                                y: u(5.),
                            },
                        ],
                        false,
                        Ink::solid(SECONDARY),
                        u(1.5),
                    )?;
                }
                let text = self.text(field.choices()[self.choice(field)]);
                centered(canvas, text, r, u(CHOICE_SIZE), ink)?;
            }
            Control::Slider => {
                let ((low, high), _) = field.range();
                let value = self.number(field);
                let fraction = match self.drag {
                    Some((dragged, fraction)) if dragged == field => fraction,
                    _ => (value - low) as f64 / (high - low).max(1) as f64,
                };
                let left = r.min.x + u(8.);
                let right = r.max.x - u(8.);
                let knob = left + (right - left) * fraction;
                let middle = r.center().y;
                canvas.line(
                    Point { x: left, y: middle },
                    Point {
                        x: right,
                        y: middle,
                    },
                    Ink::solid(RULE),
                    u(4.),
                )?;
                canvas.line(
                    Point { x: left, y: middle },
                    Point { x: knob, y: middle },
                    Ink::solid(if active { ACCENT } else { MUTED }),
                    u(4.),
                )?;
                canvas.fill_ellipse(
                    Point { x: knob, y: middle },
                    Point { x: u(7.), y: u(7.) },
                    Ink::solid(if active { TEXT } else { SECONDARY }),
                )?;
            }
            Control::Colors => {
                let current = self.color(field);
                for (index, rgb) in COLORS.iter().enumerate() {
                    let center = swatch_center(r, index, scale);
                    canvas.fill_ellipse(
                        center,
                        Point {
                            x: u(SWATCH),
                            y: u(SWATCH),
                        },
                        Ink::with_alpha(*rgb, if active { 1. } else { 0.45 }),
                    )?;
                    if *rgb == current {
                        canvas.stroke_ellipse(
                            center,
                            Point {
                                x: u(SWATCH + 3.),
                                y: u(SWATCH + 3.),
                            },
                            Ink::solid(TEXT),
                            u(1.5),
                        )?;
                    }
                }
            }
            Control::Switch => {
                let on = self.switch(field);
                let middle = r.center().y;
                let track = Rect {
                    min: Point {
                        x: r.max.x - u(SWITCH.x),
                        y: middle - u(SWITCH.y / 2.),
                    },
                    max: Point {
                        x: r.max.x,
                        y: middle + u(SWITCH.y / 2.),
                    },
                };
                if on {
                    canvas.fill_round_rect(track, u(SWITCH.y / 2.), Ink::solid(ACCENT))?;
                    canvas.fill_ellipse(
                        Point {
                            x: track.max.x - u(SWITCH.y / 2.),
                            y: middle,
                        },
                        Point {
                            x: u(4.5),
                            y: u(4.5),
                        },
                        Ink::solid(BACKGROUND),
                    )?;
                } else {
                    canvas.stroke_round_rect(
                        track.expanded(-u(0.5)),
                        u(SWITCH.y / 2. - 0.5),
                        Ink::solid(MUTED),
                        u(1.),
                    )?;
                    canvas.fill_ellipse(
                        Point {
                            x: track.min.x + u(SWITCH.y / 2.),
                            y: middle,
                        },
                        Point { x: u(4.), y: u(4.) },
                        Ink::solid(SECONDARY),
                    )?;
                }
            }
            Control::Shortcut => {
                let index = field.shortcut_index().unwrap_or(0);
                canvas.fill_round_rect(r, u(RADIUS), Ink::solid(SURFACE))?;
                canvas.stroke_round_rect(
                    r.expanded(-u(0.5)),
                    u(RADIUS),
                    Ink::solid(RULE),
                    u(1.),
                )?;
                let set = !self.shortcuts[index].is_empty();
                let text = if set {
                    self.shortcuts[index].as_str()
                } else {
                    self.text(Key::PointerNoShortcut)
                };
                let label = Rect {
                    min: r.min,
                    max: Point {
                        x: r.max.x - u(CLEAR),
                        y: r.max.y,
                    },
                };
                centered(
                    canvas,
                    text,
                    label,
                    u(CHOICE_SIZE),
                    if set { ink } else { MUTED },
                )?;
                if set {
                    let c = Point {
                        x: r.max.x - u(CLEAR / 2.),
                        y: r.center().y,
                    };
                    let arm = u(4.);
                    for (a, b) in [
                        (Point { x: -arm, y: -arm }, Point { x: arm, y: arm }),
                        (Point { x: -arm, y: arm }, Point { x: arm, y: -arm }),
                    ] {
                        canvas.line(c + a, c + b, Ink::solid(SECONDARY), u(1.5))?;
                    }
                }
            }
        }
        Ok(())
    }

    fn paint_preview<C: Canvas>(
        &self,
        canvas: &mut C,
        scale: f64,
        now: u64,
    ) -> Result<(), C::Error> {
        let effect_scale = scale * PREVIEW_SCALE;
        let pulse = now.saturating_sub(self.last_click);
        let halo = pointer::HaloState {
            color: self.halo_color,
            pressed: pulse < 150,
            pulse: (pulse < PULSE_MS).then(|| pulse as f64 / PULSE_MS as f64),
        };
        for (r, background) in self
            .layout
            .preview
            .iter()
            .zip([PREVIEW_DARK, PREVIEW_LIGHT])
        {
            let r = Rect {
                min: r.min * scale,
                max: r.max * scale,
            };
            canvas.fill_round_rect(r, scale * RADIUS, Ink::solid(background))?;
            // Lines of "text" on the page, so that the spotlight and the trail have something
            // to cover.
            let ink = if background == PREVIEW_DARK {
                Ink::with_alpha([0x8a, 0x94, 0xa6], 0.5)
            } else {
                Ink::with_alpha([0x6b, 0x72, 0x80], 0.35)
            };
            let mut y = r.min.y + scale * 22.;
            let mut line = 0;
            while y < r.max.y - scale * 14. {
                let length = [0.78, 0.55, 0.9, 0.66, 0.42][line % 5];
                canvas.line(
                    Point {
                        x: r.min.x + scale * 16.,
                        y,
                    },
                    Point {
                        x: r.min.x + scale * 16. + (r.size().x - scale * 32.) * length,
                        y,
                    },
                    ink,
                    scale * 4.,
                )?;
                y += scale * 18.;
                line += 1;
            }
            // Preview positions are offsets from the middle of each half, in logical pixels.
            let center = r.center();
            let to_canvas = |p: Point| center + p * effect_scale;
            if let Some(at) = self.preview.shown() {
                pointer::spotlight(canvas, r, to_canvas(at), &self.settings, effect_scale)?;
            }
            let placed = self.preview.mapped(to_canvas);
            pointer::paint(
                canvas,
                Point { x: 0., y: 0. },
                &placed,
                &self.settings,
                halo,
                effect_scale,
                now,
            )?;
        }
        Ok(())
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
            Field::HaloOn => self.halo_on,
            _ => self.settings.spotlight_hidden_moving,
        }
    }
    fn toggle(&mut self, field: Field) {
        match field {
            Field::HaloOn => self.halo_on ^= true,
            _ => self.settings.spotlight_hidden_moving ^= true,
        }
    }
    /// A change made anywhere but on the halo's own switch turns the halo on.
    fn shown(&mut self, action: Action) -> Action {
        if matches!(action, Action::Changed { .. }) && self.focus != Focus::Field(Field::HaloOn) {
            self.halo_on = true;
        }
        action
    }
    fn changed(field: Field) -> Action {
        Action::Changed {
            save: true,
            sound: matches!(field, Field::Sound | Field::Volume),
        }
    }
    /// Moves the value of `field` by `steps` (negative: back).
    fn step(&mut self, field: Field, steps: i32) -> Action {
        match field.control() {
            Control::Choice => {
                let count = field.choices().len() as i32;
                let next = (self.choice(field) as i32 + steps).rem_euclid(count) as usize;
                self.set_choice(field, next);
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
                let on = steps > 0;
                if self.switch(field) == on {
                    return Action::None;
                }
                self.toggle(field);
            }
            Control::Shortcut => return Action::None,
        }
        Self::changed(field)
    }

    fn at(&self, p: Point) -> Option<Focus> {
        for (field, label, control) in &self.layout.rows {
            let row = Rect {
                min: Point {
                    x: label.min.x,
                    y: label.min.y,
                },
                max: Point {
                    x: control.max.x,
                    y: label.max.y,
                },
            };
            if row.contains(p) {
                return Some(Focus::Field(*field));
            }
        }
        self.layout
            .buttons
            .iter()
            .find(|(_, r)| r.contains(p))
            .map(|(button, _)| Focus::Button(*button))
    }

    fn press_unseen(&mut self, p: Point) -> Action {
        let Some(focus) = self.at(p) else {
            return Action::None;
        };
        self.focus = focus;
        match focus {
            Focus::Button(button) => self.button(button),
            Focus::Field(field) => {
                let Some(r) = self.layout.control(field) else {
                    return Action::Repaint;
                };
                match field.control() {
                    Control::Choice => {
                        if !r.contains(p) {
                            return Action::Repaint;
                        }
                        self.step(field, if p.x < r.center().x { -1 } else { 1 })
                    }
                    Control::Slider => {
                        if !r.expanded(4.).contains(p) {
                            return Action::Repaint;
                        }
                        self.drag = Some((field, 0.));
                        self.slide(field, p.x, false)
                    }
                    Control::Colors => {
                        let picked = (0..COLORS.len()).find(|index| {
                            (swatch_center(r, *index, 1.) - p).length() <= SWATCH + 3.
                        });
                        match picked {
                            Some(index) => {
                                self.set_color(field, COLORS[index]);
                                Self::changed(field)
                            }
                            None => Action::Repaint,
                        }
                    }
                    Control::Switch => {
                        self.toggle(field);
                        Self::changed(field)
                    }
                    Control::Shortcut => {
                        let index = field.shortcut_index().unwrap_or(0);
                        if !r.contains(p) {
                            return Action::Repaint;
                        }
                        if !self.shortcuts[index].is_empty() && p.x >= r.max.x - CLEAR {
                            self.clear_shortcut(index)
                        } else {
                            Action::EditShortcut(index)
                        }
                    }
                }
            }
        }
    }
    fn slide(&mut self, field: Field, x: f64, done: bool) -> Action {
        let Some(r) = self.layout.control(field) else {
            return Action::None;
        };
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
    fn drag_unseen(&mut self, p: Point) -> Action {
        match self.drag {
            Some((field, _)) => self.slide(field, p.x, false),
            None => Action::None,
        }
    }
    fn release_unseen(&mut self, p: Point) -> Action {
        match self.drag.take() {
            Some((field, _)) => self.slide(field, p.x, true),
            None => Action::None,
        }
    }
    fn wheel_unseen(&mut self, p: Point, notches: i32) -> Action {
        match self.at(p) {
            Some(Focus::Field(field)) if notches != 0 => {
                self.focus = Focus::Field(field);
                self.step(field, notches.signum())
            }
            _ => Action::None,
        }
    }
    fn key_unseen(&mut self, key: KeyInput) -> Action {
        let order: Vec<Focus> = FIELDS
            .iter()
            .map(|field| Focus::Field(*field))
            .chain([
                Focus::Button(Button::Reset),
                Focus::Button(Button::Undo),
                Focus::Button(Button::Close),
            ])
            .collect();
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
            (KeyInput::Left, Focus::Field(field)) => self.step(field, -1),
            (KeyInput::Right, Focus::Field(field)) => self.step(field, 1),
            (KeyInput::Activate, Focus::Field(field)) => match field.control() {
                Control::Switch => {
                    self.toggle(field);
                    Self::changed(field)
                }
                Control::Shortcut => Action::EditShortcut(field.shortcut_index().unwrap_or(0)),
                _ => self.step(field, 1),
            },
            (KeyInput::Delete, Focus::Field(field)) => match field.shortcut_index() {
                Some(index) => self.clear_shortcut(index),
                None => Action::None,
            },
            (KeyInput::Activate, Focus::Button(button)) => self.button(button),
            _ => Action::None,
        }
    }
    /// A press at `p`, in logical pixels.
    pub fn press(&mut self, p: Point) -> Action {
        let action = self.press_unseen(p);
        self.shown(action)
    }
    /// The pointer moved to `p` with the button held.
    pub fn drag(&mut self, p: Point) -> Action {
        let action = self.drag_unseen(p);
        self.shown(action)
    }
    /// The button was released at `p`: a dragged slider saves its value.
    pub fn release(&mut self, p: Point) -> Action {
        let action = self.release_unseen(p);
        self.shown(action)
    }
    /// The wheel turned `notches` (positive: away from the user) over `p`.
    pub fn wheel(&mut self, p: Point, notches: i32) -> Action {
        let action = self.wheel_unseen(p, notches);
        self.shown(action)
    }
    pub fn key(&mut self, key: KeyInput) -> Action {
        let action = self.key_unseen(key);
        self.shown(action)
    }
    fn clear_shortcut(&mut self, index: usize) -> Action {
        if index == 0 {
            self.settings.click_shortcut = None;
        } else {
            self.settings.spotlight_shortcut = None;
        }
        self.shortcuts[index].clear();
        Action::Changed {
            save: true,
            sound: false,
        }
    }
    fn button(&mut self, button: Button) -> Action {
        match button {
            Button::Close => Action::Close,
            Button::Reset => {
                // Shortcuts stay: they are chosen with care and found nowhere else.
                let shortcuts = (
                    self.settings.click_shortcut,
                    self.settings.spotlight_shortcut,
                );
                self.settings = PointerSettings {
                    click_shortcut: shortcuts.0,
                    spotlight_shortcut: shortcuts.1,
                    ..Default::default()
                };
                self.halo_color =
                    shotoshot_core::settings::PresentationSettings::default().halo_color;
                Action::Changed {
                    save: true,
                    sound: false,
                }
            }
            Button::Undo => {
                let (settings, color) = self.opened.clone();
                self.settings = settings;
                self.halo_color = color;
                Action::Changed {
                    save: true,
                    sound: false,
                }
            }
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
fn swatch_center(r: Rect, index: usize, scale: f64) -> Point {
    let span = r.size().x - 2. * (SWATCH + 3.) * scale;
    let step = span / (COLORS.len() - 1) as f64;
    Point {
        x: r.min.x + (SWATCH + 3.) * scale + step * index as f64,
        y: r.center().y,
    }
}
/// A text line vertically centred in `r`, starting at its left edge.
fn line_rect(r: Rect, extent: Point) -> Rect {
    let top = r.center().y - extent.y / 2.;
    Rect {
        min: Point { x: r.min.x, y: top },
        max: Point {
            x: r.max.x.max(r.min.x + extent.x + 1.),
            y: top + extent.y * 2.,
        },
    }
}
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scale_tests::Recorder;
    use shotoshot_core::settings::Shortcut;

    /// Widths proportional to the text, as a font would give, in logical pixels.
    fn measure(text: &str, size: f64) -> Result<f64, ()> {
        Ok(text.chars().count() as f64 * size * 0.56)
    }
    fn panel() -> Panel {
        let mut panel = Panel::new(
            PointerSettings::default(),
            [255, 211, 64],
            false,
            Language::En,
        );
        panel.fit(measure).unwrap();
        panel
    }
    fn control_of(panel: &Panel, field: Field) -> Rect {
        panel.layout.control(field).unwrap()
    }

    #[test]
    fn every_text_has_room_and_nothing_overlaps() {
        for language in Language::ALL {
            let mut panel = Panel::new(PointerSettings::default(), [255, 211, 64], true, language);
            panel.shortcuts = ["Ctrl+Alt+Shift+F12".into(), String::new()];
            panel.fit(measure).unwrap();
            let zones = &panel.layout;
            let window = area(0., 0., zones.size.x, zones.size.y);
            for (field, label, control) in &zones.rows {
                let text = measure(tr_in(language, field.label()), LABEL_SIZE).unwrap();
                assert!(text <= label.size().x + 0.5, "{language:?} {field:?}");
                assert!(label.max.x <= control.min.x, "{language:?} {field:?}");
                for choice in field.choices() {
                    let width = measure(tr_in(language, *choice), CHOICE_SIZE).unwrap();
                    assert!(
                        width <= control.size().x - 2. * ARROW,
                        "{language:?} {choice:?}"
                    );
                }
                assert!(window.contains_rect(*control), "{language:?} {field:?}");
            }
            for (index, (_, a, _)) in zones.rows.iter().enumerate() {
                for (_, b, _) in &zones.rows[index + 1..] {
                    let apart = a.max.y <= b.min.y
                        || b.max.y <= a.min.y
                        || a.max.x <= b.min.x
                        || b.max.x <= a.min.x;
                    assert!(apart, "{language:?} rows overlap");
                }
            }
            for (_, r) in &zones.buttons {
                assert!(window.contains_rect(*r), "{language:?} button");
            }
            for r in zones.preview {
                assert!(
                    window.contains_rect(r) && r.size().y > 100.,
                    "{language:?} preview"
                );
            }
            assert_eq!(panel.overflows(measure), Ok(Vec::new()), "{language:?}");
        }
    }

    #[test]
    fn a_choice_steps_forward_on_its_right_half_and_back_on_its_left() {
        let mut panel = panel();
        let r = control_of(&panel, Field::HaloShape);
        let right = Point {
            x: r.max.x - 4.,
            y: r.center().y,
        };
        let left = Point {
            x: r.min.x + 4.,
            y: r.center().y,
        };
        assert_eq!(panel.settings.halo_shape, HaloShape::Disc);
        assert!(matches!(
            panel.press(right),
            Action::Changed { save: true, .. }
        ));
        assert_eq!(panel.settings.halo_shape, HaloShape::Ring);
        panel.press(left);
        panel.press(left);
        assert_eq!(panel.settings.halo_shape, HaloShape::Hidden);
        panel.press(left);
        assert_eq!(panel.settings.halo_shape, HaloShape::Neon, "wraps around");
        assert_eq!(panel.focus, Focus::Field(Field::HaloShape));
    }

    #[test]
    fn a_slider_follows_the_drag_and_saves_on_release() {
        let mut panel = panel();
        let r = control_of(&panel, Field::HaloSize);
        let y = r.center().y;
        assert_eq!(
            panel.press(Point { x: r.min.x + 8., y }),
            Action::Changed {
                save: false,
                sound: false
            }
        );
        assert_eq!(panel.settings.halo_diameter, HALO_DIAMETER_RANGE.0);
        panel.drag(Point {
            x: r.max.x + 50.,
            y,
        });
        assert_eq!(
            panel.settings.halo_diameter, HALO_DIAMETER_RANGE.1,
            "clamped"
        );
        assert_eq!(
            panel.release(Point { x: r.center().x, y }),
            Action::Changed {
                save: true,
                sound: false
            }
        );
        let middle = panel.settings.halo_diameter;
        let halfway = (HALO_DIAMETER_RANGE.0 + HALO_DIAMETER_RANGE.1) / 2;
        assert!(
            middle.abs_diff(halfway) <= 1,
            "{middle}: the knob is not held to steps"
        );
        assert_eq!(
            panel.drag(Point { x: r.min.x, y }),
            Action::None,
            "released"
        );
    }

    #[test]
    fn swatches_switches_and_shortcuts_answer_clicks() {
        let mut panel = panel();
        let r = control_of(&panel, Field::LeftColor);
        panel.press(swatch_center(r, 5, 1.));
        assert_eq!(panel.settings.left_click_color, COLORS[5]);
        let r = control_of(&panel, Field::SpotlightMoving);
        panel.press(r.center());
        assert!(panel.settings.spotlight_hidden_moving);
        let r = control_of(&panel, Field::ClickShortcut);
        assert_eq!(panel.press(r.center()), Action::EditShortcut(0));
        panel.settings.click_shortcut = Some(Shortcut {
            modifiers: 2,
            key: 0x4b,
        });
        panel.shortcuts[0] = "Ctrl+K".into();
        let clear = Point {
            x: r.max.x - 4.,
            y: r.center().y,
        };
        assert!(matches!(
            panel.press(clear),
            Action::Changed { save: true, .. }
        ));
        assert_eq!(panel.settings.click_shortcut, None);
        assert!(panel.shortcuts[0].is_empty());
    }

    #[test]
    fn the_keyboard_reaches_and_changes_everything() {
        let mut panel = panel();
        let mut seen = Vec::new();
        for _ in 0..FIELDS.len() + 3 {
            seen.push(panel.focus);
            panel.key(KeyInput::Next);
        }
        assert_eq!(panel.focus, Focus::Field(Field::HaloOn), "a full lap");
        assert_eq!(seen.len(), 24);
        assert!(seen.contains(&Focus::Button(Button::Close)));
        panel.key(KeyInput::Next);
        panel.key(KeyInput::Right);
        assert_eq!(panel.settings.halo_shape, HaloShape::Ring);
        panel.key(KeyInput::Previous);
        panel.key(KeyInput::Previous);
        assert_eq!(panel.focus, Focus::Button(Button::Close));
        assert_eq!(panel.key(KeyInput::Activate), Action::Close);
        assert_eq!(panel.key(KeyInput::Escape), Action::Close);
        panel.focus = Focus::Field(Field::Sound);
        assert_eq!(
            panel.key(KeyInput::Right),
            Action::Changed {
                save: true,
                sound: true
            },
            "a new sound is heard"
        );
    }

    #[test]
    fn any_change_shows_the_halo_and_only_its_switch_hides_it() {
        let mut panel = panel();
        assert_eq!(
            panel.focus,
            Focus::Field(Field::HaloOn),
            "the switch comes first"
        );
        // A change from each kind of control turns the halo on.
        let size = control_of(&panel, Field::HaloSize);
        let colors = control_of(&panel, Field::LeftColor);
        let switch = control_of(&panel, Field::SpotlightMoving);
        let changes: [&dyn Fn(&mut Panel) -> Action; 6] = [
            &|p| p.key(KeyInput::Right),
            &|p| {
                p.press(Point {
                    x: size.min.x + 8.,
                    y: size.center().y,
                })
            },
            &|p| p.press(swatch_center(colors, 2, 1.)),
            &|p| p.press(switch.center()),
            &|p| p.wheel(size.center(), 1),
            &|p| {
                p.focus = Focus::Button(Button::Reset);
                p.key(KeyInput::Activate)
            },
        ];
        for (index, change) in changes.iter().enumerate() {
            panel.halo_on = false;
            panel.focus = Focus::Field(Field::HaloShape);
            assert!(
                matches!(change(&mut panel), Action::Changed { .. }),
                "{index}"
            );
            assert!(panel.halo_on, "change {index} shows the halo");
        }
        // Moving between rows changes nothing and leaves the halo as it is.
        panel.halo_on = false;
        panel.key(KeyInput::Next);
        assert!(!panel.halo_on);
        // The switch turns it off, and on again.
        let r = control_of(&panel, Field::HaloOn);
        panel.halo_on = true;
        assert!(matches!(
            panel.press(r.center()),
            Action::Changed { save: true, .. }
        ));
        assert!(!panel.halo_on, "off by its switch");
        panel.press(r.center());
        assert!(panel.halo_on);
        panel.key(KeyInput::Left);
        assert!(!panel.halo_on, "off from the keyboard");
    }

    #[test]
    fn undo_returns_to_the_opening_and_reset_keeps_the_shortcuts() {
        let shortcut = Some(Shortcut {
            modifiers: 3,
            key: 0x20,
        });
        let opened = PointerSettings {
            trail: TrailStyle::Stars,
            spotlight_shortcut: shortcut,
            ..Default::default()
        };
        let mut panel = Panel::new(opened.clone(), [64, 156, 255], true, Language::En);
        panel.fit(measure).unwrap();
        panel.settings.trail = TrailStyle::Dots;
        panel.halo_color = COLORS[0];
        panel.button(Button::Undo);
        assert_eq!(panel.settings, opened);
        assert_eq!(panel.halo_color, [64, 156, 255]);
        panel.button(Button::Reset);
        assert_eq!(panel.settings.trail, TrailStyle::Off);
        assert_eq!(panel.settings.spotlight_shortcut, shortcut);
        assert_eq!(panel.halo_color, [255, 211, 64]);
    }

    #[test]
    fn the_window_paints_inside_itself_with_a_live_preview() {
        let mut panel = Panel::new(
            PointerSettings {
                halo_shape: HaloShape::Neon,
                trail: TrailStyle::Sparks,
                click: ClickStyle::Waves,
                spotlight: SpotlightShape::Horizontal,
                ..Default::default()
            },
            [255, 211, 64],
            true,
            Language::En,
        );
        panel.fit(measure).unwrap();
        for now in (0..3000).step_by(16) {
            assert!(panel.tick(now));
        }
        let mut canvas = Recorder {
            size: panel.layout.size * 2.,
            ops: Vec::new(),
        };
        panel.paint(&mut canvas, 2., 3000).unwrap();
        assert!(
            canvas.ops.iter().any(|op| op.name == "fill_rect_except"),
            "spotlight"
        );
        assert!(
            canvas.ops.iter().filter(|op| op.name == "line").count() > 20,
            "sparks"
        );
    }
}
