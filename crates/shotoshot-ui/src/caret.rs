//! ROLE: What a text caret looks like and when it shows, for every text field.
//! OWNER: Pure rules; each platform brings its clock, blink period and timer.
//! INVARIANTS: A caret is solid for a whole period after it moves; at the end of a line
//! it never touches the glyph before it, and it never reaches left of its line.
use std::time::{Duration, Instant};

/// The blink half-period where the system says nothing (Windows' default).
pub const DEFAULT_PERIOD: Duration = Duration::from_millis(530);

/// Line width of the caret in text of `size` pixels at interface `scale`: a twelfth of
/// the text, never under two logical pixels, as the system's single pixel gets lost on a
/// capture. Proportional to both, so the interface scales as one piece.
pub fn width(size: f64, scale: f64) -> f64 {
    (size / 12.).max(2. * scale)
}
/// Where the caret stands on its line, which decides how it sits on its boundary.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Place {
    /// Between two glyphs: centred on the boundary, as everywhere.
    Between,
    /// Before the first glyph: from the boundary on, never left of the line.
    Start,
    /// After the last glyph (or on an empty line): half its width clear of the glyph.
    End,
}
impl Place {
    /// The place of a caret with or without a glyph `before` and `after` it on its line.
    pub fn of(before: bool, after: bool) -> Self {
        match (before, after) {
            (_, false) => Self::End,
            (false, true) => Self::Start,
            (true, true) => Self::Between,
        }
    }
}
/// Horizontal centre of the caret at the glyph boundary `x`.
pub fn center(x: f64, size: f64, scale: f64, place: Place) -> f64 {
    let w = width(size, scale);
    match place {
        Place::Between => x,
        Place::Start => x + w / 2.,
        Place::End => x + w,
    }
}
/// How far the caret line reaches left and right of its glyph boundary, at most.
pub fn reach(size: f64, scale: f64) -> (f64, f64) {
    let w = width(size, scale);
    (w / 2., 1.5 * w)
}

/// The blink of one caret: since when it shows, and what it showed last, so a moved caret
/// (`K` changed) starts solid again, as after every keystroke.
#[derive(Clone, Copy, Debug)]
pub struct Blink<K> {
    since: Instant,
    /// None: the caret does not blink (a system setting).
    period: Option<Duration>,
    last: Option<K>,
}

impl<K: Copy + PartialEq> Blink<K> {
    pub fn new(now: Instant, period: Option<Duration>) -> Self {
        Self {
            since: now,
            period: period.filter(|p| !p.is_zero()),
            last: None,
        }
    }
    /// Whether the caret in state `state` shows at `now`; a new state restarts the blink.
    pub fn follow(&mut self, now: Instant, state: K) -> bool {
        if self.last != Some(state) {
            self.last = Some(state);
            self.since = now;
        }
        self.shown(now)
    }
    pub fn shown(&self, now: Instant) -> bool {
        let Some(period) = self.period else {
            return true;
        };
        (now.saturating_duration_since(self.since).as_nanos() / period.as_nanos()).is_multiple_of(2)
    }
    /// When `shown` changes next; None for a caret that does not blink.
    pub fn next_change(&self, now: Instant) -> Option<Instant> {
        let period = self.period?;
        let phases = now.saturating_duration_since(self.since).as_nanos() / period.as_nanos();
        let next = period.as_nanos() * (phases + 1);
        Some(self.since + Duration::from_nanos(u64::try_from(next).unwrap_or(u64::MAX)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_caret_is_thicker_than_a_pixel_and_clear_of_the_glyph() {
        for scale in [1., 1.5, 2.] {
            for size in [8., 14., 24., 48., 200.] {
                let near = |a: f64, b: f64| (a - b).abs() < 1e-9;
                let (size, w) = (size * scale, width(size * scale, scale));
                assert!(w >= 2. * scale, "{size} at {scale}: {w}");
                assert!(
                    near(w, width(size / scale, 1.) * scale),
                    "{size} at {scale}"
                );
                let (left, right) = reach(size, scale);
                // At the end of a line, half a width clear of the glyph, up to its reach.
                let end = center(10., size, scale, Place::End);
                assert!(near(end - w / 2., 10. + w / 2.), "{size} at {scale}");
                assert!(near(end + w / 2., 10. + right), "{size} at {scale}");
                // Between glyphs, on the boundary; before the first, from it.
                assert_eq!(center(10., size, scale, Place::Between), 10.);
                let start = center(10., size, scale, Place::Start);
                assert!(near(start - w / 2., 10.), "{size} at {scale}");
                assert!(near(10. - left, 10. - w / 2.), "{size} at {scale}");
            }
        }
        assert_eq!(Place::of(true, false), Place::End);
        assert_eq!(Place::of(false, false), Place::End, "an empty line");
        assert_eq!(Place::of(false, true), Place::Start);
        assert_eq!(Place::of(true, true), Place::Between);
        // Two whole pixels at the default 24 px text; thicker with larger text.
        assert_eq!(width(24., 1.), 2.);
        assert!(width(48., 1.) > width(24., 1.));
    }

    #[test]
    fn the_caret_blinks_and_moving_it_makes_it_solid_again() {
        let t0 = Instant::now();
        let period = Duration::from_millis(500);
        let at = |ms| t0 + Duration::from_millis(ms);
        let mut blink = Blink::new(t0, Some(period));
        assert!(blink.follow(t0, 3));
        assert!(blink.follow(at(499), 3));
        assert!(!blink.follow(at(500), 3));
        assert!(!blink.follow(at(999), 3));
        assert!(blink.follow(at(1000), 3));
        assert_eq!(blink.next_change(at(1200)), Some(at(1500)));
        // Hidden at 1600, then the caret moves: solid for a whole period from there.
        assert!(!blink.follow(at(1600), 3));
        assert!(blink.follow(at(1600), 4));
        assert!(blink.follow(at(2099), 4));
        assert!(!blink.follow(at(2100), 4));
        assert_eq!(blink.next_change(at(1700)), Some(at(2100)));
    }

    #[test]
    fn a_caret_set_not_to_blink_stays_solid() {
        let t0 = Instant::now();
        for period in [None, Some(Duration::ZERO)] {
            let mut blink = Blink::new(t0, period);
            for s in [0, 1, 7, 60] {
                assert!(blink.follow(t0 + Duration::from_secs(s), 1));
            }
            assert_eq!(blink.next_change(t0), None);
        }
    }
}
