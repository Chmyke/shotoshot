//! ROLE: Pointer effects of the presentation: their settings, how they are stored, and the
//! time-driven state (trail, click waves, smooth follow, spotlight).
//! OWNER: The resident presentation feeds pointer events and reads frames; the pointer window
//! drives a copy for its preview.
//! INVARIANTS: Time is injected in milliseconds; nothing here reads a clock or allocates per
//! event. Positions are device pixels; sizes in settings are logical (96-DPI) pixels.
use crate::{Point, settings::Shortcut};

/// How the halo itself is drawn.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum HaloShape {
    /// No halo: only the other effects follow the pointer.
    Hidden,
    /// A tinted disc with a thin edge: the halo Shotoshot always had.
    #[default]
    Disc,
    Ring,
    /// A ring that glows.
    Neon,
}
/// How the halo follows the pointer.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Follow {
    #[default]
    Instant,
    /// Eases towards the pointer and catches up within a few frames.
    Smooth,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum TrailStyle {
    #[default]
    Off,
    Line,
    Dots,
    Sparks,
    Stars,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum TrailLength {
    Short,
    #[default]
    Medium,
    Long,
}
impl TrailLength {
    /// How long a point of the trail stays visible.
    pub fn lifetime_ms(self) -> u64 {
        match self {
            Self::Short => 160,
            Self::Medium => 320,
            Self::Long => 560,
        }
    }
}
/// The animation a click plays where it happened.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ClickStyle {
    #[default]
    Off,
    /// A filled disc that grows and fades.
    Ripple,
    /// One ring that grows and fades.
    Ring,
    /// Three rings, one after the other.
    Waves,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ClickSound {
    #[default]
    Off,
    Soft,
    Click,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Volume {
    Low,
    #[default]
    Medium,
    High,
}
impl Volume {
    pub fn gain(self) -> f32 {
        match self {
            Self::Low => 0.25,
            Self::Medium => 0.5,
            Self::High => 0.9,
        }
    }
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SpotlightShape {
    #[default]
    Off,
    Circle,
    /// A band across the screen at the pointer's height: to follow a line.
    Horizontal,
    /// A band down the screen at the pointer's column.
    Vertical,
}

/// Every choice of the pointer window. Bounds are the `*_RANGE` constants.
#[derive(Clone, Debug, PartialEq)]
pub struct PointerSettings {
    pub halo_shape: HaloShape,
    /// Logical pixels.
    pub halo_diameter: u32,
    /// Percent of the shape's own opacities.
    pub halo_opacity: u32,
    /// Logical pixels, for the edge of the disc and the rings.
    pub halo_thickness: u32,
    /// Percent of the halo's radius that fades out: 0 is a sharp edge.
    pub halo_softness: u32,
    pub follow: Follow,
    pub trail: TrailStyle,
    pub trail_length: TrailLength,
    pub click: ClickStyle,
    pub left_click_color: [u8; 3],
    /// Right and middle buttons.
    pub right_click_color: [u8; 3],
    pub sound: ClickSound,
    pub volume: Volume,
    /// Plays the click animation at the pointer without clicking. None: no shortcut.
    pub click_shortcut: Option<Shortcut>,
    pub spotlight: SpotlightShape,
    /// Logical pixels: the circle's diameter, or the band's thickness.
    pub spotlight_size: u32,
    /// Percent of darkness around the light.
    pub spotlight_dim: u32,
    /// The spotlight steps aside while the pointer moves and comes back when it rests.
    pub spotlight_hidden_moving: bool,
    /// With a shortcut, the spotlight shows only while it is held.
    pub spotlight_shortcut: Option<Shortcut>,
    /// Logical pixels: the loupe's diameter, inside `LOUPE_SIZE_RANGE`.
    pub loupe_size: u32,
    /// How many times the loupe enlarges, inside `LOUPE_ZOOM_RANGE`.
    pub loupe_zoom: u32,
    /// Switches the loupe on or off. None: no shortcut.
    pub loupe_shortcut: Option<Shortcut>,
    /// With its shortcut, the loupe shows only while it is held instead of switching.
    pub loupe_hold: bool,
}
/// The loupe's diameter, in logical pixels.
pub const LOUPE_SIZE_RANGE: (u32, u32) = (120, 480);
pub const LOUPE_ZOOM_RANGE: (u32, u32) = (2, 8);
pub const HALO_DIAMETER_RANGE: (u32, u32) = (24, 160);
pub const HALO_OPACITY_RANGE: (u32, u32) = (10, 100);
pub const HALO_THICKNESS_RANGE: (u32, u32) = (1, 12);
pub const HALO_SOFTNESS_RANGE: (u32, u32) = (0, 100);
pub const SPOTLIGHT_SIZE_RANGE: (u32, u32) = (60, 600);
pub const SPOTLIGHT_DIM_RANGE: (u32, u32) = (20, 90);
impl Default for PointerSettings {
    fn default() -> Self {
        Self {
            // The halo of the earlier versions: 48 px disc, 2 px edge.
            halo_shape: HaloShape::Disc,
            halo_diameter: 48,
            halo_opacity: 100,
            halo_thickness: 2,
            halo_softness: 0,
            follow: Follow::Instant,
            trail: TrailStyle::Off,
            trail_length: TrailLength::Medium,
            click: ClickStyle::Off,
            left_click_color: [237, 79, 92],
            right_click_color: [64, 156, 255],
            sound: ClickSound::Off,
            volume: Volume::Medium,
            click_shortcut: None,
            spotlight: SpotlightShape::Off,
            spotlight_size: 240,
            spotlight_dim: 60,
            spotlight_hidden_moving: false,
            spotlight_shortcut: None,
            loupe_size: 240,
            loupe_zoom: 2,
            loupe_shortcut: None,
            loupe_hold: false,
        }
    }
}

/// An enum stored as the index of its variant.
macro_rules! indexed {
    ($($name:ident: [$($variant:ident),+]),+ $(,)?) => {$(
        impl $name {
            pub const ALL: &'static [Self] = &[$(Self::$variant),+];
            pub fn index(self) -> usize {
                Self::ALL.iter().position(|value| *value == self).unwrap_or(0)
            }
            fn decode(text: &str) -> Option<Self> {
                let index: usize = text.parse().ok()?;
                (index.to_string() == text).then(|| Self::ALL.get(index).copied()).flatten()
            }
        }
    )+};
}
indexed! {
    HaloShape: [Hidden, Disc, Ring, Neon],
    Follow: [Instant, Smooth],
    TrailStyle: [Off, Line, Dots, Sparks, Stars],
    TrailLength: [Short, Medium, Long],
    ClickStyle: [Off, Ripple, Ring, Waves],
    ClickSound: [Off, Soft, Click],
    Volume: [Low, Medium, High],
    SpotlightShape: [Off, Circle, Horizontal, Vertical],
}

impl PointerSettings {
    /// Brings every number back inside its range.
    pub fn clamped(mut self) -> Self {
        let clamp = |value: u32, (low, high): (u32, u32)| value.clamp(low, high);
        self.halo_diameter = clamp(self.halo_diameter, HALO_DIAMETER_RANGE);
        self.halo_opacity = clamp(self.halo_opacity, HALO_OPACITY_RANGE);
        self.halo_thickness = clamp(self.halo_thickness, HALO_THICKNESS_RANGE);
        self.halo_softness = clamp(self.halo_softness, HALO_SOFTNESS_RANGE);
        self.spotlight_size = clamp(self.spotlight_size, SPOTLIGHT_SIZE_RANGE);
        self.spotlight_dim = clamp(self.spotlight_dim, SPOTLIGHT_DIM_RANGE);
        self.loupe_size = clamp(self.loupe_size, LOUPE_SIZE_RANGE);
        self.loupe_zoom = clamp(self.loupe_zoom, LOUPE_ZOOM_RANGE);
        self
    }
    /// The pointer window's shortcuts: show a click, hold the spotlight, show the loupe.
    pub fn shortcuts(&self) -> [Option<Shortcut>; 3] {
        [
            self.click_shortcut,
            self.spotlight_shortcut,
            self.loupe_shortcut,
        ]
    }
    pub fn shortcut_mut(&mut self, index: usize) -> &mut Option<Shortcut> {
        match index {
            0 => &mut self.click_shortcut,
            1 => &mut self.spotlight_shortcut,
            _ => &mut self.loupe_shortcut,
        }
    }
    /// The settings the pointer layer is drawn with: `halo` when the halo is on, `loupe` while
    /// the loupe is on screen. The loupe takes the halo shape's place and draws nothing around
    /// itself (Mickaël, 2026-10-28: a pure round magnifier); trail and clicks stay with the halo.
    pub fn drawn(&self, halo: bool, loupe: bool) -> Self {
        let mut settings = self.clone();
        if loupe || !halo {
            settings.halo_shape = HaloShape::Hidden;
        }
        if loupe {
            // Nothing eases towards the pointer: no timer runs for a hidden shape.
            settings.follow = Follow::Instant;
        }
        if !halo {
            settings.trail = TrailStyle::Off;
            settings.click = ClickStyle::Off;
            settings.sound = ClickSound::Off;
            settings.spotlight = SpotlightShape::Off;
        }
        settings
    }
    /// One line of the settings file: comma-separated fields, shortcuts as `mods:key` or `-`.
    pub fn encode(&self) -> String {
        let color = |[red, green, blue]: [u8; 3]| format!("{red:02x}{green:02x}{blue:02x}");
        let shortcut = |value: Option<Shortcut>| {
            value.map_or_else(
                || "-".to_string(),
                |shortcut| format!("{}:{}", shortcut.modifiers, shortcut.key),
            )
        };
        [
            self.halo_shape.index().to_string(),
            self.halo_diameter.to_string(),
            self.halo_opacity.to_string(),
            self.halo_thickness.to_string(),
            self.follow.index().to_string(),
            self.trail.index().to_string(),
            self.trail_length.index().to_string(),
            self.click.index().to_string(),
            color(self.left_click_color),
            color(self.right_click_color),
            self.sound.index().to_string(),
            self.volume.index().to_string(),
            shortcut(self.click_shortcut),
            self.spotlight.index().to_string(),
            self.spotlight_size.to_string(),
            self.spotlight_dim.to_string(),
            u8::from(self.spotlight_hidden_moving).to_string(),
            shortcut(self.spotlight_shortcut),
            // Shake to find, removed on 2026-10-28: always off, so an older version reads it so.
            "0".to_string(),
            self.halo_softness.to_string(),
            self.loupe_size.to_string(),
            self.loupe_zoom.to_string(),
            shortcut(self.loupe_shortcut),
            u8::from(self.loupe_hold).to_string(),
        ]
        .join(",")
    }
    pub fn decode(line: &str) -> std::result::Result<Self, String> {
        let invalid = || "Invalid pointer settings".to_string();
        let fields: Vec<_> = line.split(',').collect();
        // The first pointer line had 19 fields; the soft edge came after it, sharp by default,
        // then the loupe's four.
        if !matches!(fields.len(), 19 | 20 | 24) {
            return Err(invalid());
        }
        let number = |text: &str, (low, high): (u32, u32)| {
            text.parse::<u32>()
                .ok()
                .filter(|value| (low..=high).contains(value) && value.to_string() == text)
                .ok_or_else(invalid)
        };
        let color = |text: &str| -> std::result::Result<[u8; 3], String> {
            if text.len() != 6 || !text.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                return Err(invalid());
            }
            let [_, red, green, blue] = u32::from_str_radix(text, 16)
                .map_err(|_| invalid())?
                .to_be_bytes();
            Ok([red, green, blue])
        };
        let flag = |text: &str| match text {
            "0" => Ok(false),
            "1" => Ok(true),
            _ => Err(invalid()),
        };
        let shortcut = |text: &str| -> std::result::Result<Option<Shortcut>, String> {
            if text == "-" {
                return Ok(None);
            }
            let (modifiers, key) = text.split_once(':').ok_or_else(invalid)?;
            let shortcut = Shortcut {
                modifiers: number(modifiers, (0, 15))?,
                key: number(key, (1, 254))?,
            };
            Ok(Some(shortcut))
        };
        // The retired shake to find flag is still checked, so a damaged line stays refused.
        flag(fields[18])?;
        Ok(Self {
            // A trial build of 2026-10-28 stored the loupe as a shape (4): read as the disc.
            halo_shape: match fields[0] {
                "4" => HaloShape::Disc,
                text => HaloShape::decode(text).ok_or_else(invalid)?,
            },
            halo_diameter: number(fields[1], HALO_DIAMETER_RANGE)?,
            halo_opacity: number(fields[2], HALO_OPACITY_RANGE)?,
            halo_thickness: number(fields[3], HALO_THICKNESS_RANGE)?,
            follow: Follow::decode(fields[4]).ok_or_else(invalid)?,
            trail: TrailStyle::decode(fields[5]).ok_or_else(invalid)?,
            trail_length: TrailLength::decode(fields[6]).ok_or_else(invalid)?,
            click: ClickStyle::decode(fields[7]).ok_or_else(invalid)?,
            left_click_color: color(fields[8])?,
            right_click_color: color(fields[9])?,
            sound: ClickSound::decode(fields[10]).ok_or_else(invalid)?,
            volume: Volume::decode(fields[11]).ok_or_else(invalid)?,
            click_shortcut: shortcut(fields[12])?,
            spotlight: SpotlightShape::decode(fields[13]).ok_or_else(invalid)?,
            spotlight_size: number(fields[14], SPOTLIGHT_SIZE_RANGE)?,
            spotlight_dim: number(fields[15], SPOTLIGHT_DIM_RANGE)?,
            spotlight_hidden_moving: flag(fields[16])?,
            spotlight_shortcut: shortcut(fields[17])?,
            halo_softness: match fields.get(19) {
                Some(value) => number(value, HALO_SOFTNESS_RANGE)?,
                None => 0,
            },
            loupe_size: match fields.get(20) {
                Some(value) => number(value, LOUPE_SIZE_RANGE)?,
                None => Self::default().loupe_size,
            },
            loupe_zoom: match fields.get(21) {
                Some(value) => number(value, LOUPE_ZOOM_RANGE)?,
                None => Self::default().loupe_zoom,
            },
            loupe_shortcut: match fields.get(22) {
                Some(value) => shortcut(value)?,
                None => None,
            },
            loupe_hold: match fields.get(23) {
                Some(value) => flag(value)?,
                None => false,
            },
        })
    }
    /// Whether something is drawn around the halo on every frame: the layer then needs room.
    pub fn draws_around(&self) -> bool {
        self.trail != TrailStyle::Off || self.click != ClickStyle::Off
    }
}

/// Logical half-size of the square layer that carries the halo and the effects around it.
pub fn layer_radius(settings: &PointerSettings) -> f64 {
    let radius = settings.halo_diameter as f64 * 0.5;
    let soft = radius * SOFT_REACH * settings.halo_softness as f64 / 100.;
    let neon = if settings.halo_shape == HaloShape::Neon {
        2. * NEON_GLOW * NEON_SOFT_GROWTH * settings.halo_softness as f64 / 100.
    } else {
        0.
    };
    let halo = radius * (1. + PULSE_GROWTH) + 2. * NEON_GLOW + neon + soft;
    if settings.draws_around() {
        halo.max(AROUND_RADIUS)
    } else {
        halo
    }
}
/// How much the click pulse grows the halo, relative to its radius.
pub const PULSE_GROWTH: f64 = 0.4;
/// Logical width of the neon glow beyond the ring.
pub const NEON_GLOW: f64 = 8.;
/// At full softness the neon glow is this much wider.
pub const NEON_SOFT_GROWTH: f64 = 2.;
/// At full softness a ring's blur reaches this fraction of the radius on each side.
pub const SOFT_REACH: f64 = 0.5;
/// Logical reach of a click wave around its click, its stroke included.
pub const CLICK_REACH: f64 = 42.;
/// Room around the pointer for the trail and the click waves.
const AROUND_RADIUS: f64 = 240.;

pub const PULSE_MS: u64 = 300;
pub const CLICK_MS: u64 = 450;
/// Waves start one after the other, this far apart.
pub const WAVE_GAP_MS: u64 = 120;
/// The spotlight returns this long after the last move when it steps aside while moving.
const REST_MS: u64 = 250;
const TRAIL_POINTS: usize = 96;
/// Logical distance the pointer travels before the trail takes a new vertex: below it, the
/// jitter of a resting hand would make the ribbon tremble.
const MIN_VERTEX: f64 = 3.;
const CLICKS: usize = 12;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TrailPoint {
    pub at: Point,
    pub time: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    Left,
    Right,
    Middle,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Click {
    pub at: Point,
    pub time: u64,
    pub button: Button,
}

/// The animated state of the pointer effects.
#[derive(Clone, Debug)]
pub struct Effects {
    trail: [TrailPoint; TRAIL_POINTS],
    /// Points stored, oldest at `trail_start`.
    trail_len: usize,
    trail_start: usize,
    clicks: [Option<Click>; CLICKS],
    next_click: usize,
    /// Where the halo is drawn: the pointer, or behind it when following smoothly.
    shown: Option<Point>,
    pointer: Option<Point>,
    last_move: Option<u64>,
    /// The time `shown` last stepped towards the pointer.
    last_step: u64,
}
impl Default for Effects {
    fn default() -> Self {
        Self {
            trail: [TrailPoint::default(); TRAIL_POINTS],
            trail_len: 0,
            trail_start: 0,
            clicks: [None; CLICKS],
            next_click: 0,
            shown: None,
            pointer: None,
            last_move: None,
            last_step: 0,
        }
    }
}
impl Effects {
    /// The pointer moved to `at`; `scale` is device pixels per logical pixel there.
    pub fn moved(&mut self, at: Point, now: u64, scale: f64, settings: &PointerSettings) {
        if !at.finite() {
            return;
        }
        if self.pointer == Some(at) {
            return;
        }
        self.pointer = Some(at);
        self.last_move = Some(now);
        if self.shown.is_none() || settings.follow == Follow::Instant {
            self.shown = Some(at);
            self.last_step = now;
        }
        if settings.trail != TrailStyle::Off {
            self.expire_trail(now, settings.trail_length.lifetime_ms());
            let close = self.trail_len > 0
                && (self.trail[(self.trail_start + self.trail_len - 1) % TRAIL_POINTS].at - at)
                    .length()
                    < MIN_VERTEX * scale;
            if close {
                return;
            }
            let point = TrailPoint { at, time: now };
            if self.trail_len < TRAIL_POINTS {
                self.trail[(self.trail_start + self.trail_len) % TRAIL_POINTS] = point;
                self.trail_len += 1;
            } else {
                self.trail[self.trail_start] = point;
                self.trail_start = (self.trail_start + 1) % TRAIL_POINTS;
            }
        }
    }
    /// A button went down at `at`, or the click shortcut asked for the animation there.
    pub fn clicked(&mut self, at: Point, button: Button, now: u64) {
        if !at.finite() {
            return;
        }
        self.clicks[self.next_click] = Some(Click {
            at,
            time: now,
            button,
        });
        self.next_click = (self.next_click + 1) % CLICKS;
    }
    /// Advances the smooth follow to `now`; returns where the halo is drawn.
    pub fn step(&mut self, now: u64, settings: &PointerSettings) -> Option<Point> {
        let pointer = self.pointer?;
        let shown = self.shown.unwrap_or(pointer);
        let next = match settings.follow {
            Follow::Instant => pointer,
            Follow::Smooth => {
                let elapsed = now.saturating_sub(self.last_step) as f64;
                // Closes about 70 % of the gap every 50 ms, whatever the frame rate.
                let keep = 0.3f64.powf(elapsed / 50.);
                let eased = pointer + (shown - pointer) * keep;
                if (eased - pointer).length() < 0.5 {
                    pointer
                } else {
                    eased
                }
            }
        };
        self.shown = Some(next);
        self.last_step = now;
        Some(next)
    }
    /// Places the pointer without counting a move: on show, or after a display change.
    pub fn place(&mut self, at: Point, now: u64) {
        if at.finite() {
            self.pointer = Some(at);
            self.shown = Some(at);
            self.last_step = now;
        }
    }
    pub fn shown(&self) -> Option<Point> {
        self.shown
    }
    pub fn pointer(&self) -> Option<Point> {
        self.pointer
    }
    fn expire_trail(&mut self, now: u64, lifetime: u64) {
        while self.trail_len > 0
            && now.saturating_sub(self.trail[self.trail_start].time) >= lifetime
        {
            self.trail_start = (self.trail_start + 1) % TRAIL_POINTS;
            self.trail_len -= 1;
        }
    }
    /// Trail points still visible at `now`, oldest first, with their age in 0..1.
    pub fn trail(
        &self,
        now: u64,
        settings: &PointerSettings,
    ) -> impl Iterator<Item = (Point, f64)> + '_ {
        let lifetime = settings.trail_length.lifetime_ms();
        let enabled = settings.trail != TrailStyle::Off;
        (0..self.trail_len)
            .map(move |index| self.trail[(self.trail_start + index) % TRAIL_POINTS])
            .filter(move |point| enabled && now.saturating_sub(point.time) < lifetime)
            .map(move |point| {
                (
                    point.at,
                    now.saturating_sub(point.time) as f64 / lifetime as f64,
                )
            })
    }
    /// The trail as a smooth curve through its vertices and the drawn halo, resampled every
    /// `spacing` device pixels, oldest first, into `out`. Each sample keeps the time its place
    /// was passed, so a shape drawn there stays the same from frame to frame.
    pub fn trail_path(
        &self,
        now: u64,
        settings: &PointerSettings,
        spacing: f64,
        out: &mut Vec<TrailPoint>,
    ) {
        out.clear();
        if settings.trail == TrailStyle::Off || spacing <= 0. {
            return;
        }
        let lifetime = settings.trail_length.lifetime_ms();
        let mut vertices: Vec<TrailPoint> = (0..self.trail_len)
            .map(|index| self.trail[(self.trail_start + index) % TRAIL_POINTS])
            .filter(|point| now.saturating_sub(point.time) < lifetime)
            .collect();
        if vertices.is_empty() {
            return;
        }
        // The ribbon ends under the halo, wherever smooth follow has it.
        if let Some(head) = self.shown
            && vertices.last().is_some_and(|last| last.at != head)
        {
            vertices.push(TrailPoint {
                at: head,
                time: now,
            });
        }
        smooth_vertices(&mut vertices);
        let count = vertices.len();
        for index in 0..count.saturating_sub(1) {
            let p1 = vertices[index];
            let p2 = vertices[index + 1];
            // At either end, a phantom vertex mirrors the neighbour: the curve leaves straight.
            let p0 = match index {
                0 => p1.at + (p1.at - p2.at),
                _ => vertices[index - 1].at,
            };
            let p3 = match vertices.get(index + 2) {
                Some(next) => next.at,
                None => p2.at + (p2.at - p1.at),
            };
            let steps = ((p2.at - p1.at).length() / spacing).ceil().max(1.) as usize;
            for step in 0..steps {
                let t = step as f64 / steps as f64;
                out.push(TrailPoint {
                    at: catmull_rom(p0, p1.at, p2.at, p3, t),
                    time: p1.time + ((p2.time.saturating_sub(p1.time)) as f64 * t) as u64,
                });
            }
        }
        if let Some(last) = vertices.last() {
            out.push(*last);
        }
    }
    /// What the effects cover at `now`, in device pixels, `scale` per logical pixel: a
    /// surface that stays put repaints and sends only this area (and the last one).
    pub fn bounds(&self, now: u64, settings: &PointerSettings, scale: f64) -> Option<crate::Rect> {
        let around = |at: Point, reach: f64| crate::Rect {
            min: at - Point { x: reach, y: reach },
            max: at + Point { x: reach, y: reach },
        };
        let mut bounds: Option<crate::Rect> = None;
        let mut add = |r: crate::Rect| bounds = Some(bounds.map_or(r, |b| b.union(r)));
        if let Some(shown) = self.shown {
            add(around(shown, layer_radius(settings) * scale + 2.));
        }
        // The widest trail mark: the ribbon's half width, a drifting spark, a star.
        let trail_reach = 26. * scale;
        for (at, _) in self.trail(now, settings) {
            add(around(at, trail_reach));
        }
        for (click, _) in self.clicks(now) {
            add(around(click.at, CLICK_REACH * scale));
        }
        bounds
    }
    /// Clicks still animating at `now`, with their progress in 0..1.
    pub fn clicks(&self, now: u64) -> impl Iterator<Item = (Click, f64)> + '_ {
        self.clicks.iter().flatten().filter_map(move |click| {
            let age = now.saturating_sub(click.time);
            (age < CLICK_MS + 2 * WAVE_GAP_MS).then_some((*click, age as f64 / CLICK_MS as f64))
        })
    }
    /// Whether the spotlight is lit now; `held` says whether its shortcut is pressed.
    pub fn spotlight_lit(&self, now: u64, held: bool, settings: &PointerSettings) -> bool {
        if settings.spotlight == SpotlightShape::Off {
            return false;
        }
        if settings.spotlight_shortcut.is_some() && !held {
            return false;
        }
        !settings.spotlight_hidden_moving
            || self
                .last_move
                .is_none_or(|last| now.saturating_sub(last) >= REST_MS)
    }
    /// Whether a later frame differs from this one without any new event: the caller keeps a
    /// timer only while this is true.
    pub fn animating(&self, now: u64, settings: &PointerSettings) -> bool {
        let trail = self.trail(now, settings).next().is_some();
        let clicks = self.clicks(now).next().is_some();
        let following = settings.follow == Follow::Smooth && self.shown != self.pointer;
        let resting = settings.spotlight != SpotlightShape::Off
            && settings.spotlight_hidden_moving
            && self
                .last_move
                .is_some_and(|last| now.saturating_sub(last) < REST_MS);
        trail || clicks || following || resting
    }
    pub fn clear(&mut self) {
        *self = Self::default();
    }
    /// A copy with every position passed through `map`: a preview draws the same state
    /// elsewhere and at another size.
    pub fn mapped(&self, map: impl Fn(Point) -> Point) -> Self {
        let mut copy = self.clone();
        for point in &mut copy.trail {
            point.at = map(point.at);
        }
        for click in copy.clicks.iter_mut().flatten() {
            click.at = map(click.at);
        }
        copy.shown = copy.shown.map(&map);
        copy.pointer = copy.pointer.map(&map);
        copy
    }
}

/// Passes of the [1, 2, 1] filter over the trail's vertices before the curve goes through them.
const SMOOTHING_PASSES: usize = 2;

/// Eases the hand's jitter and the corners between mouse samples out of the vertices; the
/// first and last stay put, so the ribbon still starts at its tail and ends under the halo.
fn smooth_vertices(vertices: &mut [TrailPoint]) {
    let count = vertices.len();
    if count < 3 {
        return;
    }
    let mut previous: Vec<Point> = vertices.iter().map(|point| point.at).collect();
    for _ in 0..SMOOTHING_PASSES {
        for index in 1..count - 1 {
            vertices[index].at =
                (previous[index - 1] + previous[index] * 2. + previous[index + 1]) * 0.25;
        }
        for (kept, vertex) in previous.iter_mut().zip(vertices.iter()) {
            *kept = vertex.at;
        }
    }
}

/// The centripetal Catmull-Rom curve from `p1` (t = 0) to `p2` (t = 1): unlike the uniform
/// one, it never loops or spikes where the mouse samples are unevenly spaced.
fn catmull_rom(p0: Point, p1: Point, p2: Point, p3: Point, t: f64) -> Point {
    // Knot gaps are the square root of each chord; a vanishing one would divide by zero.
    let gap = |a: Point, b: Point| (a - b).length().sqrt().max(1e-6);
    let t1 = gap(p0, p1);
    let t2 = t1 + gap(p1, p2);
    let t3 = t2 + gap(p2, p3);
    let u = t1 + (t2 - t1) * t;
    let lerp = |a: Point, b: Point, from: f64, to: f64| a + (b - a) * ((u - from) / (to - from));
    let a1 = lerp(p0, p1, 0., t1);
    let a2 = lerp(p1, p2, t1, t2);
    let a3 = lerp(p2, p3, t2, t3);
    let b1 = lerp(a1, a2, 0., t2);
    let b2 = lerp(a2, a3, t1, t3);
    lerp(b1, b2, t1, t2)
}

/// Width of the soft edge where the spotlight's darkness fades into its light, in logical pixels.
pub const SPOTLIGHT_FEATHER: f64 = 30.;

/// The spotlight's light grown by `margin` device pixels, as a polygon in the coordinates of
/// `screen` (device pixels), around `at`. A circle is a 48-gon; the preview fills around it.
pub fn spotlight_hole(
    settings: &PointerSettings,
    at: Point,
    scale: f64,
    screen: crate::Rect,
    margin: f64,
) -> Vec<Point> {
    let half = settings.spotlight_size as f64 * scale * 0.5 + margin;
    match settings.spotlight {
        SpotlightShape::Off => Vec::new(),
        SpotlightShape::Circle => (0..48)
            .map(|index| {
                let angle = index as f64 * std::f64::consts::TAU / 48.;
                Point {
                    x: at.x + half * angle.cos(),
                    y: at.y + half * angle.sin(),
                }
            })
            .collect(),
        SpotlightShape::Horizontal => vec![
            Point {
                x: screen.min.x,
                y: at.y - half,
            },
            Point {
                x: screen.max.x,
                y: at.y - half,
            },
            Point {
                x: screen.max.x,
                y: at.y + half,
            },
            Point {
                x: screen.min.x,
                y: at.y + half,
            },
        ],
        SpotlightShape::Vertical => vec![
            Point {
                x: at.x - half,
                y: screen.min.y,
            },
            Point {
                x: at.x + half,
                y: screen.min.y,
            },
            Point {
                x: at.x + half,
                y: screen.max.y,
            },
            Point {
                x: at.x - half,
                y: screen.max.y,
            },
        ],
    }
}

/// The spotlight's light and its soft edge, in device pixels: outside it the screen is evenly
/// dark, inside it `spotlight_shade` says how dark each pixel is. Bands span `screen`.
pub fn spotlight_box(
    settings: &PointerSettings,
    at: Point,
    scale: f64,
    screen: crate::Rect,
) -> crate::Rect {
    let reach = settings.spotlight_size as f64 * scale * 0.5 + SPOTLIGHT_FEATHER * scale;
    let (x, y) = match settings.spotlight {
        SpotlightShape::Horizontal => ((screen.min.x, screen.max.x), (at.y - reach, at.y + reach)),
        SpotlightShape::Vertical => ((at.x - reach, at.x + reach), (screen.min.y, screen.max.y)),
        SpotlightShape::Off | SpotlightShape::Circle => {
            ((at.x - reach, at.x + reach), (at.y - reach, at.y + reach))
        }
    };
    crate::Rect {
        min: Point { x: x.0, y: y.0 },
        max: Point { x: x.1, y: y.1 },
    }
}

/// How dark the spotlight is `offset` device pixels from its center, 0 in the light to 1 past
/// the soft edge.
pub fn spotlight_shade(settings: &PointerSettings, offset: Point, scale: f64) -> f64 {
    let half = settings.spotlight_size as f64 * scale * 0.5;
    let distance = match settings.spotlight {
        SpotlightShape::Off => return 0.,
        SpotlightShape::Circle => offset.length(),
        SpotlightShape::Horizontal => offset.y.abs(),
        SpotlightShape::Vertical => offset.x.abs(),
    } - half;
    spotlight_fade(distance / (SPOTLIGHT_FEATHER * scale))
}
/// The darkness `t` of the way through the soft edge: smooth at both ends, so no line shows.
pub fn spotlight_fade(t: f64) -> f64 {
    let t = t.clamp(0., 1.);
    t * t * (3. - 2. * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(x: f64, y: f64) -> Point {
        Point { x, y }
    }

    #[test]
    fn settings_round_trip_and_reject_out_of_range_values() {
        let settings = PointerSettings {
            halo_shape: HaloShape::Neon,
            halo_diameter: 96,
            follow: Follow::Smooth,
            trail: TrailStyle::Sparks,
            click: ClickStyle::Waves,
            sound: ClickSound::Click,
            click_shortcut: Some(Shortcut {
                modifiers: 3,
                key: 0x4b,
            }),
            spotlight: SpotlightShape::Horizontal,
            spotlight_hidden_moving: true,
            ..Default::default()
        };
        let line = settings.encode();
        assert_eq!(PointerSettings::decode(&line), Ok(settings.clone()));
        // The retired shake to find field is written off and read whatever 0.3.16 saved.
        let mut fields: Vec<String> = line.split(',').map(String::from).collect();
        assert_eq!(fields[18], "0");
        fields[18] = "1".into();
        assert_eq!(PointerSettings::decode(&fields.join(",")), Ok(settings));
        fields[18] = "2".into();
        assert!(PointerSettings::decode(&fields.join(",")).is_err(), "flag");
        assert_eq!(
            PointerSettings::decode(&PointerSettings::default().encode()),
            Ok(PointerSettings::default())
        );
        let mut fields: Vec<String> = line.split(',').map(String::from).collect();
        fields[1] = "300".into();
        assert!(
            PointerSettings::decode(&fields.join(",")).is_err(),
            "diameter"
        );
        fields[1] = "96".into();
        fields[0] = "9".into();
        assert!(PointerSettings::decode(&fields.join(",")).is_err(), "shape");
        fields[0] = "01".into();
        assert!(
            PointerSettings::decode(&fields.join(",")).is_err(),
            "padded"
        );
        assert!(PointerSettings::decode("1,2,3").is_err(), "short");
    }

    #[test]
    fn the_loupe_is_stored_with_its_own_choices_and_never_as_a_shape() {
        let settings = PointerSettings {
            loupe_size: 320,
            loupe_zoom: 4,
            loupe_shortcut: Some(Shortcut {
                modifiers: 2,
                key: 0x4c,
            }),
            loupe_hold: true,
            ..Default::default()
        };
        let line = settings.encode();
        assert_eq!(line.split(',').count(), 24);
        assert_eq!(PointerSettings::decode(&line), Ok(settings.clone()));
        // A line of 0.3.21 has no loupe: the default one.
        let old: Vec<_> = PointerSettings::default()
            .encode()
            .split(',')
            .take(20)
            .map(String::from)
            .collect();
        assert_eq!(
            PointerSettings::decode(&old.join(",")),
            Ok(PointerSettings::default())
        );
        let mut fields: Vec<String> = line.split(',').map(String::from).collect();
        // Any size and zoom of their range, as the sliders set them; none outside.
        fields[20] = "200".into();
        fields[21] = "5".into();
        let decoded = PointerSettings::decode(&fields.join(",")).unwrap();
        assert_eq!((decoded.loupe_size, decoded.loupe_zoom), (200, 5));
        fields[20] = "500".into();
        assert!(PointerSettings::decode(&fields.join(",")).is_err(), "size");
        fields[20] = "160".into();
        fields[21] = "9".into();
        assert!(PointerSettings::decode(&fields.join(",")).is_err(), "zoom");
        fields[21] = "3".into();
        fields[23] = "2".into();
        assert!(PointerSettings::decode(&fields.join(",")).is_err(), "hold");
        assert!(
            PointerSettings::decode(&fields[..22].join(",")).is_err(),
            "cut"
        );
        // The trial build's loupe shape comes back as the disc, and is never written.
        fields[23] = "0".into();
        fields[0] = "4".into();
        let read = PointerSettings::decode(&fields.join(",")).unwrap();
        assert_eq!(read.halo_shape, HaloShape::Disc);
        assert!(read.encode().starts_with("1,"));
        fields[0] = "5".into();
        assert!(PointerSettings::decode(&fields.join(",")).is_err(), "shape");
    }

    #[test]
    fn the_loupe_hides_the_halo_shape_and_alone_shows_nothing_else() {
        let busy = PointerSettings {
            trail: TrailStyle::Line,
            click: ClickStyle::Ring,
            spotlight: SpotlightShape::Circle,
            follow: Follow::Smooth,
            ..Default::default()
        };
        // No ring, no ticks: with the halo on, the loupe replaces its shape only.
        let both = busy.drawn(true, true);
        assert_eq!(both.halo_shape, HaloShape::Hidden);
        assert_eq!(
            (both.trail, both.click, both.follow),
            (TrailStyle::Line, ClickStyle::Ring, Follow::Instant)
        );
        assert_eq!(busy.drawn(true, false), busy);
        let alone = busy.drawn(false, true);
        assert_eq!(alone.halo_shape, HaloShape::Hidden);
        assert_eq!(
            (alone.trail, alone.click, alone.spotlight),
            (TrailStyle::Off, ClickStyle::Off, SpotlightShape::Off)
        );
        assert!(!alone.draws_around());
        assert_eq!(busy.drawn(false, false).halo_shape, HaloShape::Hidden);
        let wild = PointerSettings {
            loupe_size: 7,
            loupe_zoom: 9,
            ..Default::default()
        }
        .clamped();
        assert_eq!((wild.loupe_size, wild.loupe_zoom), (120, 8));
    }

    #[test]
    fn clamping_keeps_every_number_in_its_range() {
        let wild = PointerSettings {
            halo_diameter: 0,
            halo_opacity: 900,
            halo_thickness: 0,
            spotlight_size: 9000,
            spotlight_dim: 1,
            ..Default::default()
        }
        .clamped();
        assert_eq!(wild.halo_diameter, HALO_DIAMETER_RANGE.0);
        assert_eq!(wild.halo_opacity, HALO_OPACITY_RANGE.1);
        assert_eq!(wild.halo_thickness, HALO_THICKNESS_RANGE.0);
        assert_eq!(wild.spotlight_size, SPOTLIGHT_SIZE_RANGE.1);
        assert_eq!(wild.spotlight_dim, SPOTLIGHT_DIM_RANGE.0);
        assert!(PointerSettings::decode(&wild.encode()).is_ok());
    }

    #[test]
    fn the_trail_keeps_recent_points_and_forgets_old_ones() {
        let settings = PointerSettings {
            trail: TrailStyle::Line,
            trail_length: TrailLength::Short,
            ..Default::default()
        };
        let mut effects = Effects::default();
        for step in 0..10u64 {
            effects.moved(at(step as f64 * 10., 0.), step * 20, 1., &settings);
        }
        let points: Vec<_> = effects.trail(180, &settings).collect();
        assert!(!points.is_empty());
        assert!(points.iter().all(|(_, age)| (0. ..1.).contains(age)));
        assert!(
            points.windows(2).all(|pair| pair[0].1 >= pair[1].1),
            "oldest first"
        );
        assert!(effects.animating(180, &settings));
        assert_eq!(effects.trail(1000, &settings).count(), 0);
        assert!(
            !effects.animating(1000, &settings),
            "nothing left to animate"
        );
        let off = PointerSettings::default();
        assert_eq!(effects.trail(180, &off).count(), 0, "off draws nothing");
    }

    #[test]
    fn the_trail_survives_more_moves_than_it_can_hold() {
        let settings = PointerSettings {
            trail: TrailStyle::Dots,
            trail_length: TrailLength::Long,
            ..Default::default()
        };
        let mut effects = Effects::default();
        for step in 0..500u64 {
            effects.moved(at(step as f64 * 4., 0.), step, 1., &settings);
        }
        let points: Vec<_> = effects.trail(499, &settings).collect();
        assert_eq!(points.len(), TRAIL_POINTS);
        assert_eq!(points.last().map(|(p, _)| p.x), Some(499. * 4.));
    }

    #[test]
    fn the_trail_path_is_smooth_evenly_spaced_and_ends_under_the_halo() {
        let settings = PointerSettings {
            trail: TrailStyle::Line,
            trail_length: TrailLength::Long,
            ..Default::default()
        };
        let mut effects = Effects::default();
        // A right angle, then a resting hand that jitters by one pixel.
        for (step, (x, y)) in [
            (0., 0.),
            (40., 0.),
            (80., 0.),
            (80., 40.),
            (80., 80.),
            (81., 80.),
            (80., 81.),
        ]
        .into_iter()
        .enumerate()
        {
            effects.moved(at(x, y), step as u64 * 20, 1., &settings);
        }
        let mut path = Vec::new();
        effects.trail_path(140, &settings, 4., &mut path);
        // 160 pixels of polyline, a little less once the corner is cut round.
        assert!(path.len() > 32, "{}", path.len());
        assert_eq!(path.first().map(|p| p.at), Some(at(0., 0.)));
        assert_eq!(
            path.last().map(|p| p.at),
            effects.shown(),
            "ends under the halo"
        );
        for pair in path.windows(2) {
            let gap = (pair[1].at - pair[0].at).length();
            // A curve is a little longer than its chord: samples stay close to the spacing.
            assert!(gap <= 6., "even spacing, {gap}");
            assert!(pair[0].time <= pair[1].time, "oldest first");
        }
        // The corner is rounded: no sample sits exactly on it.
        assert!(path.iter().all(|p| p.at != at(80., 0.) || p.time == 40));
        let off = PointerSettings::default();
        effects.trail_path(140, &off, 4., &mut path);
        assert!(path.is_empty());
    }

    #[test]
    fn a_quick_shaky_stroke_leaves_a_ribbon_without_visible_corners() {
        let settings = PointerSettings {
            trail: TrailStyle::Line,
            trail_length: TrailLength::Long,
            ..Default::default()
        };
        let mut effects = Effects::default();
        // Far apart samples of a fast hand that wobbles by 5 pixels, unevenly spaced.
        for step in 0..16u64 {
            let x = (step * 30 + step % 3 * 9) as f64;
            let y = if step % 2 == 0 { 0. } else { 5. };
            effects.moved(at(x, y), step * 8, 1., &settings);
        }
        let mut path = Vec::new();
        effects.trail_path(130, &settings, 2.5, &mut path);
        let mut sharpest: f64 = 0.;
        for triple in path.windows(3) {
            let a = triple[1].at - triple[0].at;
            let b = triple[2].at - triple[1].at;
            if a.length() < 1e-6 || b.length() < 1e-6 {
                continue;
            }
            let cos = ((a.x * b.x + a.y * b.y) / (a.length() * b.length())).clamp(-1., 1.);
            sharpest = sharpest.max(cos.acos().to_degrees());
        }
        assert!(
            sharpest < 3.,
            "the ribbon turns by {sharpest:.1} degrees between samples"
        );
    }

    #[test]
    fn a_resting_hand_adds_no_trail_vertex() {
        let settings = PointerSettings {
            trail: TrailStyle::Line,
            ..Default::default()
        };
        let mut effects = Effects::default();
        effects.moved(at(10., 10.), 0, 1., &settings);
        for step in 1..20u64 {
            let wobble = (step % 2) as f64;
            effects.moved(at(10. + wobble, 10. - wobble), step, 1., &settings);
        }
        assert_eq!(effects.trail(19, &settings).count(), 1);
    }

    #[test]
    fn the_first_pointer_line_still_reads_with_a_sharp_edge() {
        let line = PointerSettings::default().encode();
        let first: Vec<&str> = line.split(',').take(19).collect();
        let read = PointerSettings::decode(&first.join(",")).unwrap();
        assert_eq!(read.halo_softness, 0);
        let soft = PointerSettings {
            halo_softness: 60,
            ..Default::default()
        };
        assert_eq!(PointerSettings::decode(&soft.encode()), Ok(soft));
    }

    #[test]
    fn the_bounds_cover_the_halo_the_trail_and_the_clicks() {
        let settings = PointerSettings {
            trail: TrailStyle::Line,
            click: ClickStyle::Ring,
            ..Default::default()
        };
        let mut effects = Effects::default();
        assert_eq!(effects.bounds(0, &settings, 1.), None);
        for step in 0..10u64 {
            effects.moved(at(100. + step as f64 * 20., 300.), step * 10, 1., &settings);
        }
        effects.clicked(at(500., 500.), Button::Left, 90);
        let bounds = effects.bounds(100, &settings, 2.).unwrap();
        assert!(bounds.contains(at(100., 300.)), "the tail");
        assert!(
            bounds.contains(at(280. + 2. * AROUND_RADIUS - 1., 300.)),
            "the halo's room"
        );
        assert!(
            bounds.contains(at(500. + 2. * CLICK_REACH - 1., 500.)),
            "the click"
        );
        // Once everything has faded, only the halo's room is left.
        let rest = effects.bounds(5000, &settings, 1.).unwrap();
        assert!(
            !rest.contains(at(500. + CLICK_REACH, 500.)),
            "the faded click"
        );
    }

    /// Mickaël's rule (2026-10-30): a fresh install makes no sound; the click sound is opt-in.
    #[test]
    fn a_fresh_install_is_silent() {
        assert_eq!(PointerSettings::default().sound, ClickSound::Off);
    }

    #[test]
    fn clicks_animate_then_stop() {
        let settings = PointerSettings::default();
        let mut effects = Effects::default();
        effects.clicked(at(5., 5.), Button::Right, 100);
        let clicks: Vec<_> = effects.clicks(100).collect();
        assert_eq!(clicks.len(), 1);
        assert_eq!(clicks[0].0.button, Button::Right);
        assert_eq!(clicks[0].1, 0.);
        assert!(effects.animating(300, &settings));
        assert!(!effects.animating(100 + CLICK_MS + 2 * WAVE_GAP_MS, &settings));
        for index in 0..30 {
            effects.clicked(at(1., 1.), Button::Left, 200 + index);
        }
        assert_eq!(effects.clicks(230).count(), CLICKS, "bounded storage");
    }

    #[test]
    fn shaking_the_mouse_plays_nothing() {
        let settings = PointerSettings::default();
        let mut effects = Effects::default();
        let mut now = 0;
        for x in [0., 100., 0., 100., 0., 100.] {
            now += 60;
            effects.moved(at(x, 0.), now, 1., &settings);
        }
        assert!(
            !effects.animating(now, &settings),
            "no animation after a shake"
        );
        let halo = layer_radius(&settings) + 2.;
        let bounds = effects.bounds(now, &settings, 1.).unwrap();
        assert_eq!(
            (bounds.min, bounds.max),
            (at(100. - halo, -halo), at(100. + halo, halo))
        );
    }

    #[test]
    fn smooth_follow_catches_up_and_instant_never_lags() {
        let smooth = PointerSettings {
            follow: Follow::Smooth,
            ..Default::default()
        };
        let mut effects = Effects::default();
        effects.place(at(0., 0.), 0);
        effects.moved(at(100., 0.), 0, 1., &smooth);
        let first = effects.step(16, &smooth).unwrap();
        assert!(first.x > 0. && first.x < 100., "{first:?}");
        assert!(effects.animating(16, &smooth));
        let late = effects.step(1000, &smooth).unwrap();
        assert_eq!(late, at(100., 0.));
        assert!(!effects.animating(1000, &smooth));

        let instant = PointerSettings::default();
        let mut effects = Effects::default();
        effects.moved(at(40., 30.), 0, 1., &instant);
        assert_eq!(effects.step(1, &instant), Some(at(40., 30.)));
    }

    #[test]
    fn the_spotlight_obeys_its_shortcut_and_rests_after_moves() {
        let mut settings = PointerSettings {
            spotlight: SpotlightShape::Circle,
            ..Default::default()
        };
        let mut effects = Effects::default();
        effects.moved(at(1., 1.), 1000, 1., &settings);
        assert!(effects.spotlight_lit(1000, false, &settings));
        settings.spotlight_hidden_moving = true;
        assert!(!effects.spotlight_lit(1100, false, &settings), "moving");
        assert!(effects.animating(1100, &settings), "a timer brings it back");
        assert!(effects.spotlight_lit(1300, false, &settings), "rested");
        settings.spotlight_shortcut = Some(Shortcut {
            modifiers: 2,
            key: 0x20,
        });
        assert!(!effects.spotlight_lit(1300, false, &settings), "not held");
        assert!(effects.spotlight_lit(1300, true, &settings), "held");
        settings.spotlight = SpotlightShape::Off;
        assert!(!effects.spotlight_lit(1300, true, &settings), "off");
    }

    #[test]
    fn spotlight_holes_follow_the_pointer_and_span_the_screen() {
        let screen = crate::Rect {
            min: at(0., 0.),
            max: at(1000., 800.),
        };
        let mut settings = PointerSettings {
            spotlight: SpotlightShape::Circle,
            spotlight_size: 200,
            ..Default::default()
        };
        let circle = spotlight_hole(&settings, at(500., 400.), 2., screen, 0.);
        assert_eq!(circle.len(), 48);
        assert!(
            circle
                .iter()
                .all(|p| ((*p - at(500., 400.)).length() - 200.).abs() < 1e-6),
            "diameter 200 logical at scale 2"
        );
        settings.spotlight = SpotlightShape::Horizontal;
        let band = spotlight_hole(&settings, at(500., 400.), 1., screen, 0.);
        assert_eq!(band[0], at(0., 300.));
        assert_eq!(band[2], at(1000., 500.));
        settings.spotlight = SpotlightShape::Vertical;
        let band = spotlight_hole(&settings, at(500., 400.), 1., screen, 0.);
        assert_eq!(band[0], at(400., 0.));
        assert_eq!(band[2], at(600., 800.));
    }

    #[test]
    fn the_spotlight_fades_over_its_soft_edge() {
        let screen = crate::Rect {
            min: at(0., 0.),
            max: at(1000., 800.),
        };
        let mut settings = PointerSettings {
            spotlight: SpotlightShape::Circle,
            spotlight_size: 200,
            ..Default::default()
        };
        // Diameter 200 logical at scale 2: light to 200 device pixels, edge 60 more.
        let edge = spotlight_box(&settings, at(500., 400.), 2., screen);
        assert_eq!((edge.min, edge.max), (at(240., 140.), at(760., 660.)));
        let shade = |x: f64| spotlight_shade(&settings, at(x, 0.), 2.);
        assert_eq!(shade(0.), 0.);
        assert_eq!(shade(200.), 0.);
        assert!((shade(230.) - 0.5).abs() < 1e-9, "halfway through the edge");
        assert!(shade(215.) < shade(230.) && shade(230.) < shade(245.));
        assert_eq!(shade(260.), 1.);
        assert_eq!(
            spotlight_shade(&settings, at(300., 300.), 2.),
            1.,
            "box corner"
        );
        settings.spotlight = SpotlightShape::Horizontal;
        let band = spotlight_box(&settings, at(500., 400.), 1., screen);
        assert_eq!((band.min, band.max), (at(0., 270.), at(1000., 530.)));
        assert_eq!(
            spotlight_shade(&settings, at(900., 50.), 1.),
            0.,
            "inside the band"
        );
        settings.spotlight = SpotlightShape::Vertical;
        let band = spotlight_box(&settings, at(500., 400.), 1., screen);
        assert_eq!((band.min, band.max), (at(370., 0.), at(630., 800.)));
        assert_eq!(spotlight_shade(&settings, at(130., 0.), 1.), 1.);
    }

    #[test]
    fn the_layer_grows_only_for_effects_around_the_halo() {
        let quiet = PointerSettings::default();
        assert!(layer_radius(&quiet) < 60., "the default halo stays small");
        let trail = PointerSettings {
            trail: TrailStyle::Dots,
            ..Default::default()
        };
        assert!(layer_radius(&trail) >= AROUND_RADIUS);
    }
}
