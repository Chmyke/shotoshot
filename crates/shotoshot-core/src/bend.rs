//! A bent arrow or line: the quadratic curve from its start to its end that passes through
//! a point set relative to those ends, so it follows them when they move, turn or attach.
#[cfg(test)]
use crate::segment_distance;
use crate::{Point, Rect};

/// Bound the deviation from the analytic curve in screen/document pixels, as for ellipses.
const CURVE_ERROR: f64 = 0.2;
const MAX_SEGMENTS: usize = 512;

/// Straight: the curve's middle is the chord's.
pub fn bent(bend: Point) -> bool {
    bend != Point::default()
}
/// The chord turned a quarter clockwise on screen, whose Y points down.
fn across(a: Point, b: Point) -> Point {
    let d = b - a;
    Point { x: -d.y, y: d.x }
}
/// Where the curve passes halfway. `bend.x` runs along the chord and `bend.y` across it,
/// both in chord lengths from its middle.
pub fn bend_point(a: Point, b: Point, bend: Point) -> Point {
    (a + b) * 0.5 + (b - a) * bend.x + across(a, b) * bend.y
}
/// The bend that makes the curve pass through `m` halfway; straight for a point-like chord.
pub fn bend_through(a: Point, b: Point, m: Point) -> Point {
    let d = b - a;
    let squared = d.x * d.x + d.y * d.y;
    if squared < 1e-9 {
        return Point::default();
    }
    let v = m - (a + b) * 0.5;
    Point {
        x: (v.x * d.x + v.y * d.y) / squared,
        y: (v.y * d.x - v.x * d.y) / squared,
    }
}
/// The control point of the quadratic: the curve's middle is halfway to it from the chord's.
pub fn bend_control(a: Point, b: Point, bend: Point) -> Point {
    bend_point(a, b, bend) * 2. - (a + b) * 0.5
}
/// The curve as a polyline from `a` to `b`, within `CURVE_ERROR` of it.
pub fn bent_points(a: Point, b: Point, bend: Point) -> Vec<Point> {
    let c = bend_control(a, b, bend);
    // Its second derivative is 2(a - 2c + b) everywhere, so n even chords stray at most
    // |a - 2c + b| / 4n² from it.
    let spread = (a - c * 2. + b).length();
    let segments = ((spread / (4. * CURVE_ERROR)).sqrt().ceil() as usize).clamp(1, MAX_SEGMENTS);
    (0..=segments)
        .map(|i| {
            let t = i as f64 / segments as f64;
            a * ((1. - t) * (1. - t)) + c * (2. * t * (1. - t)) + b * (t * t)
        })
        .collect()
}
/// The curve's exact extent: its ends and any turning point of either coordinate.
pub fn bent_bounds(a: Point, b: Point, bend: Point) -> Option<Rect> {
    let c = bend_control(a, b, bend);
    let at = |t: f64| a * ((1. - t) * (1. - t)) + c * (2. * t * (1. - t)) + b * (t * t);
    let turn = |a: f64, c: f64, b: f64| {
        let divisor = a - 2. * c + b;
        (divisor.abs() > 1e-12)
            .then(|| (a - c) / divisor)
            .filter(|t| *t > 0. && *t < 1.)
    };
    [turn(a.x, c.x, b.x), turn(a.y, c.y, b.y)]
        .into_iter()
        .flatten()
        .map(at)
        .try_fold(Rect::new(a, b)?, |r, p| Some(r.union(Rect::new(p, p)?)))
}
/// The direction the curve arrives at `b` in, which its head points along.
pub fn bent_arrival(a: Point, b: Point, bend: Point) -> Point {
    let d = b - bend_control(a, b, bend);
    if d.length() < 1e-9 { b - a } else { d }
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: Point = Point { x: 10., y: 20. };
    const B: Point = Point { x: 210., y: 20. };

    #[test]
    fn the_curve_passes_through_its_bend_point_and_both_ends() {
        let bend = Point { x: 0.1, y: 0.3 };
        let m = bend_point(A, B, bend);
        assert_eq!(m, Point { x: 130., y: 80. });
        let points = bent_points(A, B, bend);
        assert_eq!(points.first(), Some(&A));
        assert_eq!(points.last(), Some(&B));
        let nearest = points
            .windows(2)
            .map(|w| segment_distance(m, w[0], w[1]))
            .fold(f64::MAX, f64::min);
        assert!(nearest <= CURVE_ERROR, "{nearest}");
        let c = bend_control(A, B, bend);
        let halfway = A * 0.25 + c * 0.5 + B * 0.25;
        assert!((halfway - m).length() < 1e-9);
    }
    #[test]
    fn a_bend_point_round_trips_through_any_chord() {
        for (a, b) in [(A, B), (B, A), (A, Point { x: -40., y: 300. })] {
            let m = Point { x: 77., y: -13. };
            let back = bend_point(a, b, bend_through(a, b, m));
            assert!((back - m).length() < 1e-9, "{back:?}");
        }
        assert_eq!(bend_through(A, A, B), Point::default());
    }
    #[test]
    fn flattening_stays_within_the_error_and_a_straight_chord_is_one_segment() {
        assert_eq!(bent_points(A, B, Point::default()), vec![A, B]);
        let bend = Point { x: 0., y: 0.8 };
        let points = bent_points(A, B, bend);
        let c = bend_control(A, B, bend);
        let n = (points.len() - 1) as f64;
        for (i, w) in points.windows(2).enumerate() {
            // Each chord strays most at its middle, from the curve at the middle parameter.
            let t = (i as f64 + 0.5) / n;
            let on = A * ((1. - t) * (1. - t)) + c * (2. * t * (1. - t)) + B * (t * t);
            let stray = (on - (w[0] + w[1]) * 0.5).length();
            assert!(stray <= CURVE_ERROR, "{stray}");
        }
    }
    #[test]
    fn bounds_hold_the_bulge_and_nothing_more() {
        let r = bent_bounds(A, B, Point { x: 0., y: 0.5 }).unwrap();
        // Halfway is 100 px below the chord; that is the curve's lowest point.
        assert!((r.max.y - 120.).abs() < 1e-9, "{r:?}");
        assert_eq!((r.min.x, r.max.x, r.min.y), (10., 210., 20.));
        assert_eq!(bent_bounds(A, B, Point::default()), Rect::new(A, B));
    }
    #[test]
    fn the_head_arrives_along_the_tangent() {
        let d = bent_arrival(A, B, Point { x: 0., y: 0.5 });
        // Control at (110, 220): the curve comes up into B from below left.
        assert!(d.x > 0. && d.y < 0., "{d:?}");
        assert_eq!(
            bent_arrival(A, B, Point::default()),
            B - bend_control(A, B, Point::default())
        );
    }
}
