//! An elbow arrow, pulled out of a text box by one of its four anchors: it leaves straight
//! out of the side that faces its target, turns at right angles with rounded corners and,
//! when attached, enters the side of its target that faces it (Mickaël, 2026-10-30).
use crate::{Attachment, Object, Point, Rect, Shape};

/// How far, in interface pixels, an anchor sits outside the side of its box.
pub const ANCHOR_GAP: f64 = 12.;
/// How deep, in image pixels, the head bites into its target's outline.
const BITE: f64 = 1.;
/// Corners in samples: a quarter turn drawn as this many chords.
const CORNER_SEGMENTS: usize = 8;

/// One end of an elbow arrow: where it rests and, on a side, the outward normal it leaves or
/// enters along.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Port {
    pub point: Point,
    pub normal: Option<Point>,
}

/// The middle of the side of `target` that faces `aim`, `gap` outside it, with that side's
/// outward normal; both turn with the target.
pub fn facing_port(target: &Object, aim: Point, gap: f64) -> Port {
    let r = target.local_bounds();
    let c = r.center();
    let half = r.size() * 0.5;
    let d = target.world_to_local(aim) - c;
    // Judged against the box's own proportions: a wide label faces up or down further out.
    let across = |v: f64, h: f64| if h < 1e-9 { 0. } else { (v / h).abs() };
    let n = if across(d.x, half.x) > across(d.y, half.y) {
        Point {
            x: d.x.signum(),
            y: 0.,
        }
    } else {
        Point {
            x: 0.,
            y: if d.y < 0. { -1. } else { 1. },
        }
    };
    let side = c + Point {
        x: n.x * (half.x + gap),
        y: n.y * (half.y + gap),
    };
    Port {
        point: target.local_to_world(side),
        normal: Some(target.local_to_world(c + n) - target.local_to_world(c)),
    }
}

/// How far outside its target the start rests: just inside a panel, its round cap hidden in
/// the panel's own colour; past a closed shape's stroke plus `cap` (Mickaël, 2026-10-30).
fn start_gap(target: &Object, cap: f64) -> f64 {
    match target.shape {
        Shape::Text { .. } => -cap,
        _ => target.style.width * 0.5 + cap,
    }
}
/// How far outside its target the end rests: `BITE` into a panel's edge or into a closed
/// shape's stroke, never past its middle, so the head touches (Mickaël, 2026-10-30).
fn end_gap(target: &Object, cap: f64) -> f64 {
    match target.shape {
        Shape::Text { .. } => cap - BITE,
        _ => (target.style.width * 0.5 - BITE).max(0.) + cap,
    }
}

/// Both ends of an elbow arrow, each attached one on the side facing the other end: the
/// other target's centre, or the other free point.
pub fn elbow_ends(o: &Object, objects: &[Object]) -> Option<(Port, Port)> {
    let Shape::Arrow {
        start,
        end,
        start_attach,
        end_attach,
        elbow: true,
        ..
    } = &o.shape
    else {
        return None;
    };
    let (start_cap, end_cap) = o.end_caps();
    let target = |a: &Option<Attachment>| {
        a.and_then(|a| objects.iter().find(|t| t.id == a.target && t.id != o.id))
    };
    let (from, to) = (target(start_attach), target(end_attach));
    let centre = |t: &Object| t.local_bounds().center();
    let mut a = match from {
        Some(t) => facing_port(t, to.map_or(*end, centre), start_gap(t, start_cap)),
        None => Port {
            point: *start,
            normal: None,
        },
    };
    let mut b = match to {
        Some(t) => facing_port(t, from.map_or(*start, centre), end_gap(t, end_cap)),
        None => Port {
            point: *end,
            normal: None,
        },
    };
    // Ends almost in line run straight rather than through a cramped jog (Mickaël,
    // 2026-10-30): the start slides along its side, else the end along its own.
    let tolerance = 2. * corner_radius(o.style.width);
    if !slide_in_line(&mut a, from, b.point, tolerance) {
        slide_in_line(&mut b, to, a.point, tolerance);
    }
    Some((a, b))
}

/// Slides `port` along the side of `target` it rests on until it lies in line with `other`,
/// ahead of it, when that takes less than `tolerance` and stays on the side. False when the
/// port is free, on a turned target, too far aside or would leave its side.
fn slide_in_line(port: &mut Port, target: Option<&Object>, other: Point, tolerance: f64) -> bool {
    let (Some(target), Some(n)) = (target, port.normal) else {
        return false;
    };
    let n_axis = axis(n);
    if (n - n_axis).length() > 1e-9 {
        return false;
    }
    let along = Point {
        x: n_axis.y.abs(),
        y: n_axis.x.abs(),
    };
    let d = other - port.point;
    let ahead = d.x * n_axis.x + d.y * n_axis.y;
    let offset = d.x * along.x + d.y * along.y;
    if ahead <= 0. || offset.abs() > tolerance {
        return false;
    }
    let point = port.point + along * offset;
    let side = target.local_bounds();
    let (low, high) = (
        side.min.x * along.x + side.min.y * along.y,
        side.max.x * along.x + side.max.y * along.y,
    );
    let at = point.x * along.x + point.y * along.y;
    if at < low || at > high {
        return false;
    }
    port.point = point;
    true
}

/// The straight leg before the first turn and after the last: room for the head and a
/// rounded corner, so the head always sits on a straight run into its side.
pub fn elbow_leg(width: f64) -> f64 {
    width * 3.84 + corner_radius(width) + 4.
}
fn corner_radius(width: f64) -> f64 {
    (width * 4.).clamp(8., 24.)
}
/// The nearest of the four directions to `n`.
fn axis(n: Point) -> Point {
    if n.x.abs() >= n.y.abs() {
        Point {
            x: n.x.signum(),
            y: 0.,
        }
    } else {
        Point {
            x: 0.,
            y: n.y.signum(),
        }
    }
}
/// The turns from `p`, which leaves along `n`, to a free point `q`: one turn when `q` lies
/// ahead and far enough aside for the head, else two through the middle, or back round when
/// it lies behind.
fn toward_free(p: Point, n: Point, q: Point, leg: f64) -> Vec<Point> {
    let horizontal = n.x != 0.;
    let (ahead, aside) = if horizontal {
        ((q.x - p.x) * n.x, (q.y - p.y).abs())
    } else {
        ((q.y - p.y) * n.y, (q.x - p.x).abs())
    };
    let turn = |a: Point, b: Point| {
        if horizontal {
            Point { x: b.x, y: a.y }
        } else {
            Point { x: a.x, y: b.y }
        }
    };
    if ahead > 0. && aside >= leg {
        vec![turn(p, q)]
    } else if ahead > 0. {
        let m = (p + q) * 0.5;
        vec![turn(p, m), turn(q, m)]
    } else {
        vec![turn(q, p)]
    }
}

/// The corners of the route from `a` to `b`, ends included, without rounding: a leg out of
/// each side, then right angles in screen axes. A free start bound for a side leaves along
/// the axis that leads most toward it, as if out of a side of its own: an S or an L, never a
/// run flat along the box it came off (Mickaël, 2026-10-30).
pub fn elbow_route(a: Port, b: Port, leg: f64) -> Vec<Point> {
    let nb = b.normal.map(axis);
    let na = a
        .normal
        .map(axis)
        .or_else(|| nb.map(|_| axis(b.point - a.point)));
    // Facing sides closer than two legs cross at the true middle of the gap: legs that long
    // would overshoot it and come back in a hook (Mickaël, 2026-10-30).
    let mut leg_a = leg;
    let mut leg_b = leg;
    if let (Some(na), Some(nb)) = (na, nb)
        && na.x == -nb.x
        && na.y == -nb.y
    {
        let d = b.point - a.point;
        let gap = d.x * na.x + d.y * na.y;
        if gap > 0. && gap < 2. * leg {
            (leg_a, leg_b) = (gap * 0.5, gap * 0.5);
        }
    }
    let p = na.map_or(a.point, |n| a.point + n * leg_a);
    let q = nb.map_or(b.point, |n| b.point + n * leg_b);
    let middle = match (na, nb) {
        (Some(na), Some(nb)) => match (na.x != 0., nb.x != 0.) {
            (true, true) => {
                let x = (p.x + q.x) * 0.5;
                vec![Point { x, y: p.y }, Point { x, y: q.y }]
            }
            (false, false) => {
                let y = (p.y + q.y) * 0.5;
                vec![Point { x: p.x, y }, Point { x: q.x, y }]
            }
            (true, false) => vec![Point { x: q.x, y: p.y }],
            (false, true) => vec![Point { x: p.x, y: q.y }],
        },
        (Some(n), None) => toward_free(p, n, q, leg),
        (None, _) => {
            let d = q - p;
            let n = axis(if d.length() < 1e-9 {
                Point { x: 1., y: 0. }
            } else {
                d
            });
            toward_free(p, n, q, leg)
        }
    };
    let mut corners: Vec<Point> = Vec::with_capacity(middle.len() + 4);
    for point in [a.point, p].into_iter().chain(middle).chain([q, b.point]) {
        if corners.last().is_some_and(|l| (*l - point).length() < 0.01) {
            continue;
        }
        // A point in line with the two before it is no turn.
        if let [.., u, v] = corners[..] {
            let cross = (v.x - u.x) * (point.y - v.y) - (v.y - u.y) * (point.x - v.x);
            let back = (v.x - u.x) * (point.x - v.x) + (v.y - u.y) * (point.y - v.y);
            if cross.abs() < 0.01 && back >= 0. {
                corners.pop();
            }
        }
        corners.push(point);
    }
    corners
}

/// `corners` with each turn rounded, never by more than half of either side of it.
pub fn round_corners(corners: &[Point], radius: f64) -> Vec<Point> {
    let mut out = Vec::with_capacity(corners.len() * (CORNER_SEGMENTS + 1));
    let Some(&first) = corners.first() else {
        return out;
    };
    out.push(first);
    for w in corners.windows(3) {
        let (a, b, c) = (w[0], w[1], w[2]);
        let (into, out_of) = ((b - a).length(), (c - b).length());
        let r = radius.min(into * 0.5).min(out_of * 0.5);
        if r < 0.5 {
            out.push(b);
            continue;
        }
        let entry = b + (a - b) * (r / into);
        let exit = b + (c - b) * (r / out_of);
        for step in 0..=CORNER_SEGMENTS {
            let t = step as f64 / CORNER_SEGMENTS as f64;
            out.push(entry * ((1. - t) * (1. - t)) + b * (2. * t * (1. - t)) + exit * (t * t));
        }
    }
    if corners.len() > 1 {
        out.push(corners[corners.len() - 1]);
    }
    out
}

/// The elbow arrow's corners from its resolved ends.
pub fn elbow_corners(o: &Object, objects: &[Object]) -> Option<Vec<Point>> {
    let (a, b) = elbow_ends(o, objects)?;
    Some(elbow_route(a, b, elbow_leg(o.style.width)))
}
/// The elbow arrow's centre line as a polyline, corners rounded: what renders and is hit.
pub fn elbow_points(o: &Object, objects: &[Object]) -> Option<Vec<Point>> {
    elbow_corners(o, objects).map(|c| round_corners(&c, corner_radius(o.style.width)))
}
/// The extent of a polyline.
pub fn polyline_bounds(points: &[Point]) -> Option<Rect> {
    let (first, rest) = points.split_first()?;
    rest.iter().try_fold(Rect::new(*first, *first)?, |r, p| {
        Some(r.union(Rect::new(*p, *p)?))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(x: f64, y: f64) -> Point {
        Point { x, y }
    }
    fn free(point: Point) -> Port {
        Port {
            point,
            normal: None,
        }
    }
    fn side(point: Point, normal: Point) -> Port {
        Port {
            point,
            normal: Some(normal),
        }
    }
    /// Every run of the route is horizontal or vertical.
    fn square(route: &[Point]) -> bool {
        route
            .windows(2)
            .all(|w| (w[1].x - w[0].x).abs() < 1e-9 || (w[1].y - w[0].y).abs() < 1e-9)
    }

    #[test]
    fn a_free_end_aside_takes_one_turn_and_leaves_straight_out_of_its_side() {
        let route = elbow_route(side(p(100., 50.), p(1., 0.)), free(p(300., 200.)), 20.);
        assert_eq!(route, vec![p(100., 50.), p(300., 50.), p(300., 200.)]);
    }
    #[test]
    fn a_free_end_almost_in_line_turns_twice_through_the_middle() {
        let route = elbow_route(side(p(100., 50.), p(1., 0.)), free(p(300., 60.)), 20.);
        assert_eq!(
            route,
            vec![p(100., 50.), p(210., 50.), p(210., 60.), p(300., 60.)]
        );
        let straight = elbow_route(side(p(100., 50.), p(1., 0.)), free(p(300., 50.)), 20.);
        assert_eq!(straight, vec![p(100., 50.), p(300., 50.)]);
    }
    #[test]
    fn a_free_end_behind_the_side_goes_out_then_back_round() {
        let route = elbow_route(side(p(100., 50.), p(0., 1.)), free(p(40., 10.)), 20.);
        assert!(square(&route), "{route:?}");
        assert_eq!(
            route[1],
            p(100., 70.),
            "the leg leaves along the normal first"
        );
        assert_eq!(route.last(), Some(&p(40., 10.)));
    }
    #[test]
    fn two_sides_facing_each_other_meet_halfway() {
        let route = elbow_route(
            side(p(100., 50.), p(1., 0.)),
            side(p(300., 150.), p(-1., 0.)),
            20.,
        );
        assert_eq!(
            route,
            vec![p(100., 50.), p(200., 50.), p(200., 150.), p(300., 150.)]
        );
        let crossed = elbow_route(
            side(p(100., 50.), p(1., 0.)),
            side(p(300., 150.), p(0., -1.)),
            20.,
        );
        assert_eq!(
            crossed,
            vec![p(100., 50.), p(300., 50.), p(300., 150.)],
            "out of one side, into the top of the other"
        );
    }
    #[test]
    fn a_free_start_bound_for_a_side_leaves_along_its_main_axis() {
        let s = elbow_route(free(p(100., 50.)), side(p(300., 150.), p(-1., 0.)), 20.);
        assert_eq!(
            s,
            vec![p(100., 50.), p(200., 50.), p(200., 150.), p(300., 150.)],
            "an S: out, across halfway, in"
        );
        let down = elbow_route(free(p(100., 50.)), side(p(120., 300.), p(0., -1.)), 20.);
        assert_eq!(
            down,
            vec![p(100., 50.), p(100., 175.), p(120., 175.), p(120., 300.)]
        );
        let l = elbow_route(free(p(100., 50.)), side(p(300., 150.), p(0., -1.)), 20.);
        assert_eq!(l, vec![p(100., 50.), p(300., 50.), p(300., 150.)], "an L");
    }
    #[test]
    fn close_facing_sides_cross_at_the_true_middle_without_a_hook() {
        // 40 px apart with 30 px legs: they used to reach 130 and 110, then come back.
        let route = elbow_route(
            side(p(100., 100.), p(0., 1.)),
            side(p(300., 140.), p(0., -1.)),
            30.,
        );
        assert_eq!(
            route,
            vec![p(100., 100.), p(100., 120.), p(300., 120.), p(300., 140.)]
        );
        assert!(route.windows(2).all(|w| w[1].y >= w[0].y), "never back up");
    }
    #[test]
    fn rounded_corners_keep_the_ends_and_stay_inside_the_turn() {
        let corners = [p(0., 0.), p(100., 0.), p(100., 100.)];
        let round = round_corners(&corners, 12.);
        assert_eq!(round.first(), Some(&p(0., 0.)));
        assert_eq!(round.last(), Some(&p(100., 100.)));
        assert!(round.contains(&p(88., 0.)) && round.contains(&p(100., 12.)));
        assert!(round.iter().all(|q| q.x <= 100. && q.y >= 0.));
        // A short side halves the radius rather than overshooting.
        let tight = round_corners(&[p(0., 0.), p(10., 0.), p(10., 100.)], 12.);
        assert!(tight.contains(&p(5., 0.)), "{tight:?}");
    }
}
