//! One-shot toolbar motion: it starts when what is chosen changes, plays once, then nothing
//! ticks. A newly chosen button bounces and the one it replaces fades out; a newly chosen colour
//! springs into its ring; a hovered swatch swells.
use super::*;
use std::time::Instant;

const BOUNCE_MS: u64 = 280;
const RELEASE_MS: u64 = 140;
const SPRING_MS: u64 = 300;
const SWELL_MS: u64 = 120;
/// Dot of the chosen swatch inside its ring, as a share of a free swatch.
pub(super) const CHOSEN_DOT: f64 = 0.8375;
pub(super) const SWOLLEN_DOT: f64 = 1.14;
pub(super) const RING_ALPHA: f32 = 0.95;

#[derive(Clone, Copy, PartialEq)]
enum Effect {
    Bounce(Action),
    Release(Action),
    Ring(Action),
    Unring(Action),
    Swell(Action),
    Unswell(Action),
}
impl Effect {
    fn duration(self) -> u64 {
        match self {
            Effect::Bounce(_) => BOUNCE_MS,
            Effect::Release(_) => RELEASE_MS,
            Effect::Ring(_) | Effect::Unring(_) => SPRING_MS,
            Effect::Swell(_) | Effect::Unswell(_) => SWELL_MS,
        }
    }
    /// A button runs one effect per family; a new one replaces the old.
    fn slot(self) -> (u8, Action) {
        match self {
            Effect::Bounce(a) | Effect::Release(a) => (0, a),
            Effect::Ring(a) | Effect::Unring(a) => (1, a),
            Effect::Swell(a) | Effect::Unswell(a) => (2, a),
        }
    }
}

/// What the toolbar showed at the last observation.
#[derive(Clone, PartialEq)]
struct Seen {
    chosen: Vec<Action>,
    swatch: Option<Action>,
    hovered: Option<Action>,
}

/// How a colour swatch is drawn this frame: its dot's share of the free size, and its ring.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SwatchLook {
    pub dot: f64,
    pub ring: Option<(f32, f64)>,
}

#[derive(Default)]
pub struct Motion {
    seen: Option<Seen>,
    effects: Vec<(Effect, Instant)>,
}
impl Motion {
    /// Starts the effects of what changed since the last call. The first call only records:
    /// the toolbar appearing is not a choice.
    pub fn observe(&mut self, buttons: &[Button], state: Chosen, now: Instant) {
        self.effects
            .retain(|(effect, since)| elapsed(*since, now) < effect.duration() as f64);
        let seen = Seen {
            chosen: buttons
                .iter()
                .filter(|b| state.button(b))
                .map(|b| b.action)
                .collect(),
            swatch: buttons.iter().map(|b| b.action).find(|a| state.swatch(*a)),
            hovered: buttons
                .iter()
                .find(|b| {
                    is_swatch(b.action)
                        && b.enabled
                        && b.bounds.contains(state.pointer)
                        && !state.swatch(b.action)
                })
                .map(|b| b.action),
        };
        let Some(old) = self.seen.replace(seen.clone()) else {
            return;
        };
        for a in seen.chosen.iter().filter(|a| !old.chosen.contains(a)) {
            self.start(Effect::Bounce(*a), now);
        }
        for a in old.chosen.iter().filter(|a| !seen.chosen.contains(a)) {
            self.start(Effect::Release(*a), now);
        }
        let pairs = [
            (
                seen.swatch,
                old.swatch,
                Effect::Ring as fn(Action) -> Effect,
                Effect::Unring as fn(Action) -> Effect,
            ),
            (seen.hovered, old.hovered, Effect::Swell, Effect::Unswell),
        ];
        for (new, old, enter, leave) in pairs {
            if new == old {
                continue;
            }
            if let Some(a) = new {
                self.start(enter(a), now);
            }
            if let Some(a) = old {
                self.start(leave(a), now);
            }
        }
    }
    fn start(&mut self, effect: Effect, now: Instant) {
        self.effects.retain(|(e, _)| e.slot() != effect.slot());
        self.effects.push((effect, now));
    }
    /// Milliseconds to the next frame while an effect plays; `None` at rest.
    pub fn next_tick(&self, now: Instant) -> Option<u32> {
        self.effects
            .iter()
            .any(|(effect, since)| elapsed(*since, now) < effect.duration() as f64)
            .then_some(16)
    }
    /// Progress of the running effect of `slot`, from 0 to 1.
    fn progress(&self, slot: (u8, Action), now: Instant) -> Option<(Effect, f64)> {
        self.effects
            .iter()
            .find(|(effect, _)| effect.slot() == slot)
            .map(|(effect, since)| {
                let p = elapsed(*since, now) / effect.duration() as f64;
                (*effect, p.clamp(0., 1.))
            })
            .filter(|(_, p)| *p < 1.)
    }
    /// Size of a button against its rest size: tucked in, a slight overshoot, then still.
    pub(super) fn scale(&self, action: Action, now: Instant) -> f64 {
        match self.progress((0, action), now) {
            Some((Effect::Bounce(_), p)) => {
                frames(ease_out(p), &[(0., 0.78), (0.55, 1.1), (1., 1.)])
            }
            _ => 1.,
        }
    }
    /// Opacity of the chosen fill still fading from a button just left.
    pub(super) fn release(&self, action: Action, now: Instant) -> Option<f32> {
        match self.progress((0, action), now) {
            Some((Effect::Release(_), p)) => Some((1. - ease_out(p)) as f32),
            _ => None,
        }
    }
    /// A swatch's dot and ring this frame, spring included.
    pub(super) fn swatch(
        &self,
        action: Action,
        chosen: bool,
        hovered: bool,
        now: Instant,
    ) -> SwatchLook {
        let mut look = SwatchLook::rest(chosen);
        if chosen {
            if let Some((Effect::Ring(_), p)) = self.progress((1, action), now) {
                look.dot = frames(p, &[(0., 1.), (0.4, 0.79), (0.7, 0.855), (1., CHOSEN_DOT)]);
                let alpha = frames(
                    p,
                    &[(0., 0.), (0.45, RING_ALPHA as f64), (1., RING_ALPHA as f64)],
                );
                look.ring = Some((
                    alpha as f32,
                    frames(p, &[(0., 0.8), (0.45, 1.05), (1., 1.)]),
                ));
            }
            return look;
        }
        if let Some((Effect::Unring(_), p)) = self.progress((1, action), now) {
            look.dot = frames(p, &[(0., CHOSEN_DOT), (0.5, 1.05), (1., 1.)]);
        }
        match self.progress((2, action), now) {
            Some((Effect::Swell(_), p)) => look.dot = 1. + (SWOLLEN_DOT - 1.) * ease_out(p),
            Some((Effect::Unswell(_), p)) => {
                look.dot = SWOLLEN_DOT - (SWOLLEN_DOT - 1.) * ease_out(p)
            }
            _ if hovered => look.dot = SWOLLEN_DOT,
            _ => {}
        }
        look
    }
}
impl SwatchLook {
    /// A still swatch: the chosen one sits smaller inside a white ring.
    pub fn rest(chosen: bool) -> Self {
        if chosen {
            Self {
                dot: CHOSEN_DOT,
                ring: Some((RING_ALPHA, 1.)),
            }
        } else {
            Self {
                dot: 1.,
                ring: None,
            }
        }
    }
}

/// What the toolbar marks as chosen: the tool, the modes that are on and the drawing colour.
#[derive(Clone, Copy)]
pub struct Chosen {
    pub tool: Tool,
    pub eyedropper: bool,
    pub current: [u8; 3],
    pub pointer: Point,
}
impl Chosen {
    pub(super) fn button(&self, b: &Button) -> bool {
        b.active
            || matches!(b.action, Action::Tool(t) if t == self.tool)
            || matches!(b.action, Action::Eyedropper) && self.eyedropper
    }
    pub(super) fn swatch(&self, action: Action) -> bool {
        marks_colour(action, self.current)
    }
}

/// The palette swatch of the drawing colour, or the custom one when it is none of them.
pub(super) fn marks_colour(action: Action, current: [u8; 3]) -> bool {
    match action {
        Action::Color(rgb) => rgb == current,
        Action::CustomColor => !COLORS.contains(&current),
        _ => false,
    }
}
pub(super) fn is_swatch(action: Action) -> bool {
    matches!(action, Action::Color(_) | Action::CustomColor)
}
fn elapsed(since: Instant, now: Instant) -> f64 {
    now.saturating_duration_since(since).as_secs_f64() * 1000.
}
fn ease_out(p: f64) -> f64 {
    1. - (1. - p).powi(3)
}
/// Piecewise-linear value through `(progress, value)` keyframes.
fn frames(p: f64, keys: &[(f64, f64)]) -> f64 {
    keys.windows(2)
        .find(|pair| p <= pair[1].0)
        .map(|pair| {
            let ((a, va), (z, vz)) = (pair[0], pair[1]);
            va + (vz - va) * ((p - a) / (z - a)).clamp(0., 1.)
        })
        .unwrap_or(keys[keys.len() - 1].1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn panel() -> Vec<Button> {
        let region = Rect::new(Point { x: 100., y: 100. }, Point { x: 400., y: 300. }).unwrap();
        let screen = Rect::new(Point::default(), Point { x: 1920., y: 1080. }).unwrap();
        buttons_in(region, None, screen, 1., false)
    }
    fn chosen(tool: Tool, current: [u8; 3]) -> Chosen {
        Chosen {
            tool,
            eyedropper: false,
            current,
            pointer: Point::default(),
        }
    }
    const ARROW: Action = Action::Tool(Tool::Arrow);
    const PEN: Action = Action::Tool(Tool::Pen);

    #[test]
    fn a_new_tool_bounces_once_the_old_one_fades_and_then_nothing_ticks() {
        let (buttons, t0) = (panel(), Instant::now());
        let mut motion = Motion::default();
        motion.observe(&buttons, chosen(Tool::Arrow, COLORS[0]), t0);
        assert_eq!(
            motion.next_tick(t0),
            None,
            "the toolbar appearing plays nothing"
        );
        let t1 = t0 + Duration::from_millis(500);
        motion.observe(&buttons, chosen(Tool::Pen, COLORS[0]), t1);
        assert_eq!(motion.next_tick(t1), Some(16));
        assert!(
            (motion.scale(PEN, t1) - 0.78).abs() < 1e-9,
            "tucked in first"
        );
        let peak = (0..BOUNCE_MS)
            .map(|ms| motion.scale(PEN, t1 + Duration::from_millis(ms)))
            .fold(0., f64::max);
        assert!(peak > 1.05, "a slight overshoot, here {peak}");
        assert_eq!(motion.release(ARROW, t1), Some(1.));
        let done = t1 + Duration::from_millis(BOUNCE_MS);
        assert_eq!(motion.scale(PEN, done), 1.);
        assert_eq!(motion.release(ARROW, done), None);
        assert_eq!(motion.next_tick(done), None, "nothing ticks at rest");
    }

    #[test]
    fn a_new_colour_springs_into_its_ring_and_settles() {
        let (buttons, t0) = (panel(), Instant::now());
        let mut motion = Motion::default();
        motion.observe(&buttons, chosen(Tool::Arrow, COLORS[0]), t0);
        motion.observe(&buttons, chosen(Tool::Arrow, COLORS[3]), t0);
        let blue = Action::Color(COLORS[3]);
        let start = motion.swatch(blue, true, false, t0);
        assert_eq!((start.dot, start.ring), (1., Some((0., 0.8))));
        let dip = motion.swatch(blue, true, false, t0 + Duration::from_millis(120));
        assert!(
            dip.dot < CHOSEN_DOT,
            "the spring dips past its rest, here {}",
            dip.dot
        );
        let settled = t0 + Duration::from_millis(SPRING_MS);
        assert_eq!(
            motion.swatch(blue, true, false, settled),
            SwatchLook::rest(true)
        );
        let red = Action::Color(COLORS[0]);
        assert_eq!(
            motion.swatch(red, false, false, settled),
            SwatchLook::rest(false)
        );
        assert_eq!(motion.next_tick(settled), None);
    }

    #[test]
    fn a_custom_colour_marks_the_custom_swatch_and_a_hovered_swatch_swells() {
        let (buttons, t0) = (panel(), Instant::now());
        let custom = chosen(Tool::Arrow, [1, 2, 3]);
        assert!(custom.swatch(Action::CustomColor));
        assert!(!custom.swatch(Action::Color(COLORS[0])));
        let red = buttons
            .iter()
            .find(|b| b.action == Action::Color(COLORS[0]))
            .unwrap();
        let mut motion = Motion::default();
        motion.observe(&buttons, custom, t0);
        let hovering = Chosen {
            pointer: red.bounds.center(),
            ..custom
        };
        motion.observe(&buttons, hovering, t0);
        let swell = t0 + Duration::from_millis(SWELL_MS);
        assert_eq!(
            motion.swatch(red.action, false, true, swell).dot,
            SWOLLEN_DOT
        );
        assert_eq!(motion.next_tick(swell), None);
    }
}
