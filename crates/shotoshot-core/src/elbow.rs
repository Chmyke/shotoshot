//! An elbow arrow, pulled out of a text box by one of its four anchors: it leaves straight
//! out of the side that faces its target, turns at right angles with rounded corners and,
//! when attached, enters the side of its target that faces it (Mickaël, 2026-10-30).
use crate::{Attachment, Object, Point, Rect, Shape};

/// How far, in interface pixels, an anchor sits outside the side of its box.
pub const ANCHOR_GAP: f64 = 12.;
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

/// How far outside its target an end rests: past a closed shape's stroke, on a panel's edge,
/// plus `cap` for a round line end.
fn gap(target: &Object, cap: f64) -> f64 {
    match target.shape {
        Shape::Text { .. } => cap,
        _ => target.style.width * 0.5 + cap,
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
    let a = match from {
        Some(t) => facing_port(t, to.map_or(*end, centre), gap(t, start_cap)),
        None => Port {
            point: *start,
            normal: None,
        },
    };
    let b = match to {
        Some(t) => facing_port(t, from.map_or(*start, centre), gap(t, end_cap)),
        None => Port {
            point: *end,
            normal: None,
        },
    };
    Some((a, b))
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
/// each side, then right angles in screen axes.
pub fn elbow_route(a: Port, b: Port, leg: f64) -> Vec<Point> {
    let (na, nb) = (a.normal.map(axis), b.normal.map(axis));
    let p = na.map_or(a.point, |n| a.point + n * leg);
    let q = nb.map_or(b.point, |n| b.point + n * leg);
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
        (None, Some(n)) => {
            let mut back = toward_free(q, n, p, leg);
            back.reverse();
            back
        }
        (None, None) => {
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
