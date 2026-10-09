//! ROLE: Wheel sensitivity: whole notches out of raw wheel travel, and the size ladders one
//! notch walks. Steps grow with the size, so a notch stays a visible change at any scale.
//! INVARIANTS: Ladders are strictly increasing; a notch up then down returns to the same rung.

/// Wheel travel of one notch, as Windows reports it (`WHEEL_DELTA`).
pub const NOTCH: i16 = 120;

// The step doubles each time the size does: four notches per doubling for strokes from
// 4 px, six for text from 16 px.
const STROKE_WIDTHS: [f64; 18] = [
    1., 2., 3., 4., 5., 6., 7., 8., 10., 12., 14., 16., 20., 24., 28., 32., 36., 40.,
];
const TEXT_SIZES: [f64; 31] = [
    8., 10., 12., 14., 16., 18., 20., 22., 24., 28., 32., 36., 40., 44., 48., 56., 64., 72., 80.,
    88., 96., 112., 128., 144., 160., 176., 192., 224., 256., 288., 300.,
];

/// Wheel travel not yet spent on a notch. High-resolution wheels and touchpads report many
/// small deltas; summing them keeps their pace equal to a notched wheel's.
#[derive(Default)]
pub struct Notches(i32);
impl Notches {
    /// Whole notches completed by `delta`; reversing drops the unfinished one.
    pub fn take(&mut self, delta: i16) -> i32 {
        let delta = i32::from(delta);
        if self.0.signum() * delta.signum() < 0 {
            self.0 = 0;
        }
        self.0 += delta;
        let notches = self.0 / i32::from(NOTCH);
        self.0 %= i32::from(NOTCH);
        notches
    }
}

/// Stroke width after `notches` (positive: thicker).
pub fn stroke_width_after(width: f64, notches: i32) -> f64 {
    walk(&STROKE_WIDTHS, width, notches)
}
/// Text size after `notches` (positive: larger).
pub fn text_size_after(size: f64, notches: i32) -> f64 {
    walk(&TEXT_SIZES, size, notches)
}

/// Moves to the neighbouring rung once per notch. A value between two rungs joins the ladder
/// in the direction of travel; one outside the ladder is never pushed further out.
fn walk(ladder: &[f64], mut value: f64, notches: i32) -> f64 {
    for _ in 0..notches.unsigned_abs() {
        let rung = if notches > 0 {
            ladder.iter().find(|&&rung| rung > value)
        } else {
            ladder.iter().rev().find(|&&rung| rung < value)
        };
        match rung {
            Some(&rung) => value = rung,
            None => break,
        }
    }
    value
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ladders_increase_and_keep_the_documented_bounds() {
        for ladder in [&STROKE_WIDTHS[..], &TEXT_SIZES[..]] {
            assert!(ladder.windows(2).all(|pair| pair[0] < pair[1]));
        }
        assert_eq!((STROKE_WIDTHS[0], STROKE_WIDTHS[17]), (1., 40.));
        assert_eq!((TEXT_SIZES[0], TEXT_SIZES[30]), (8., 300.));
    }
    #[test]
    fn a_notch_changes_more_as_the_size_grows() {
        assert_eq!(stroke_width_after(4., 1), 5.);
        assert_eq!(stroke_width_after(8., 1), 10.);
        assert_eq!(stroke_width_after(24., 1), 28.);
        assert_eq!(text_size_after(20., 1), 22.);
        assert_eq!(text_size_after(24., 1), 28.);
        assert_eq!(text_size_after(96., 1), 112.);
        // The old fixed steps needed 16 and 36 notches for these two trips.
        assert_eq!(stroke_width_after(4., 9), 20.);
        assert_eq!(text_size_after(24., 12), 96.);
    }
    #[test]
    fn a_notch_up_then_down_returns_to_the_same_size() {
        for ladder in [&STROKE_WIDTHS[..], &TEXT_SIZES[..]] {
            for pair in ladder.windows(2) {
                assert_eq!(walk(ladder, pair[0], 1), pair[1]);
                assert_eq!(walk(ladder, pair[1], -1), pair[0]);
            }
        }
    }
    #[test]
    fn sizes_stop_at_the_ends_of_the_ladder() {
        assert_eq!(stroke_width_after(40., 1), 40.);
        assert_eq!(stroke_width_after(1., -1), 1.);
        assert_eq!(stroke_width_after(32., 5), 40.);
        assert_eq!(text_size_after(300., 1), 300.);
        assert_eq!(text_size_after(8., -1), 8.);
    }
    #[test]
    fn a_size_between_rungs_joins_the_ladder_in_the_direction_of_travel() {
        // Widths saved by earlier versions, texts scaled by their handles.
        assert_eq!(stroke_width_after(9., 1), 10.);
        assert_eq!(stroke_width_after(9., -1), 8.);
        assert_eq!(text_size_after(37.4, 1), 40.);
        assert_eq!(text_size_after(37.4, -1), 36.);
    }
    #[test]
    fn a_size_outside_the_ladder_is_never_pushed_further_out() {
        assert_eq!(text_size_after(420., 1), 420.);
        assert_eq!(text_size_after(420., -1), 300.);
        assert_eq!(text_size_after(5., -1), 5.);
        assert_eq!(text_size_after(5., 1), 8.);
    }
    #[test]
    fn small_deltas_add_up_to_one_notch() {
        let mut travel = Notches::default();
        assert_eq!(
            [30, 30, 30, 30, 30].map(|delta| travel.take(delta)),
            [0, 0, 0, 1, 0]
        );
        assert_eq!(travel.take(200), 1);
        assert_eq!(travel.take(240), 2);
        assert_eq!(
            travel.take(-120),
            -1,
            "reversing drops the unfinished notch"
        );
        assert_eq!([-60, -60].map(|delta| travel.take(delta)), [0, -1]);
    }
}
