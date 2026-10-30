use crate::{Object, Point, Shape, arrow_points, bent, bent_arrival, bent_points};

// Bound the deviation from the analytic ellipse in screen/document pixels.
const CURVE_ERROR: f64 = 0.2;
const MAX_ELLIPSE_SEGMENTS: usize = 4096;
const ARROW_HEAD_HALF_WIDTH: f64 = 0.45;
const ARROW_JOIN_OVERLAP: f64 = 2.;

/// Keep the flat shaft cap inside the head so separately antialiased edges do not
/// leave a seam. Translucent renderers must apply opacity to the assembled arrow.
pub fn arrow_shaft_end(start: Point, end: Point, width: f64, head_length: f64) -> Option<Point> {
    let delta = end - start;
    let length = delta.length();
    if length <= head_length + width * 0.5 {
        return None;
    }
    let overlap = ARROW_JOIN_OVERLAP.min(head_length - width / (2. * ARROW_HEAD_HALF_WIDTH));
    Some(end - delta * ((head_length - overlap.max(0.)) / length))
}

/// A bent shaft cut where it enters the head, at the distance from the tip that
/// `arrow_shaft_end` keeps on a straight one; empty when the head covers the curve.
fn bent_shaft(mut curve: Vec<Point>, width: f64, head_length: f64) -> Vec<Point> {
    let (Some(&a), Some(&b)) = (curve.first(), curve.last()) else {
        return curve;
    };
    if (b - a).length() <= head_length + width * 0.5 {
        return Vec::new();
    }
    let overlap = ARROW_JOIN_OVERLAP.min(head_length - width / (2. * ARROW_HEAD_HALF_WIDTH));
    let reach = head_length - overlap.max(0.);
    // The last sample still outside the reach, then the crossing on the segment after it.
    let Some(i) = curve.iter().rposition(|p| (b - *p).length() >= reach) else {
        return Vec::new();
    };
    let Some(&inside) = curve.get(i + 1) else {
        return curve;
    };
    let outside = curve[i];
    let (mut low, mut high) = (0., 1.);
    for _ in 0..40 {
        let t = (low + high) * 0.5;
        if (b - (outside + (inside - outside) * t)).length() >= reach {
            low = t;
        } else {
            high = t;
        }
    }
    curve.truncate(i + 1);
    curve.push(outside + (inside - outside) * low);
    curve
}

pub fn paths(object: &Object, objects: &[Object]) -> Vec<Vec<Point>> {
    match &object.shape {
        Shape::Rectangle(r) => {
            let mut p = r.corners().to_vec();
            p.push(r.min);
            vec![p.into_iter().map(|p| object.local_to_world(p)).collect()]
        }
        Shape::Ellipse(r) => {
            let c = r.center();
            let radius = r.size() * 0.5;
            let max_radius = radius.x.max(radius.y);
            let angle = (1. - CURVE_ERROR / max_radius.max(CURVE_ERROR)).acos();
            let segments = ((std::f64::consts::PI / angle).ceil() as usize)
                .clamp(16, MAX_ELLIPSE_SEGMENTS)
                .next_multiple_of(4);
            let mut points: Vec<_> = (0..segments)
                .map(|i| {
                    let angle = i as f64 * std::f64::consts::TAU / segments as f64;
                    c + Point {
                        x: angle.cos() * radius.x,
                        y: angle.sin() * radius.y,
                    }
                })
                .collect();
            points.push(points[0]);
            vec![
                points
                    .into_iter()
                    .map(|p| object.local_to_world(p))
                    .collect(),
            ]
        }
        Shape::Arrow { head, bend, .. } => {
            let (a, b) = arrow_points(object, objects).unwrap_or_default();
            let d = b - a;
            let length = d.length();
            if length < 0.001 {
                return vec![];
            }
            let curve = bent(*bend).then(|| bent_points(a, b, *bend));
            if !head {
                return vec![curve.unwrap_or_else(|| vec![a, b])];
            }
            let unit = match curve {
                Some(_) => {
                    let arrival = bent_arrival(a, b, *bend);
                    arrival * (1. / arrival.length())
                }
                None => d * (1. / length),
            };
            let wing = Point {
                x: -unit.y,
                y: unit.x,
            };
            let size = object.style.width * 3.84;
            let shaft = match curve {
                Some(curve) => bent_shaft(curve, object.style.width, size),
                None => arrow_shaft_end(a, b, object.style.width, size)
                    .map_or_else(Vec::new, |end| vec![a, end]),
            };
            vec![
                shaft,
                vec![
                    b - unit * size + wing * (size * ARROW_HEAD_HALF_WIDTH),
                    b,
                    b - unit * size - wing * (size * ARROW_HEAD_HALF_WIDTH),
                    b - unit * size + wing * (size * ARROW_HEAD_HALF_WIDTH),
                ],
            ]
        }
        Shape::Stroke { .. } => stroke_runs(object)
            .into_iter()
            .map(|(points, _)| points)
            .collect(),
        Shape::Text { .. } => vec![],
    }
}

// Round only a two-pixel neighbourhood of each sample. Keep endpoints and avoid
// spline overshoot, so the pen follows the hand without lag or changing bounds.
fn smooth_pen(points: &[Point]) -> Vec<Point> {
    if points.len() < 3 {
        return points.to_vec();
    }
    let mut result = Vec::with_capacity(points.len() * 5);
    result.push(points[0]);
    for w in points.windows(3) {
        let a = w[0] - w[1];
        let b = w[2] - w[1];
        let entry = w[1] + a * (0.5_f64.min(2. / a.length().max(1e-9)));
        let exit = w[1] + b * (0.5_f64.min(2. / b.length().max(1e-9)));
        result.push(entry);
        for step in 1..=4 {
            let t = step as f64 / 4.;
            result
                .push(entry * ((1. - t) * (1. - t)) + w[1] * (2. * t * (1. - t)) + exit * (t * t));
        }
    }
    result.push(*points.last().unwrap());
    result
}

/// Contiguous runs share their boundary point, while preserving each segment's width.
/// The same geometry drives rendering and hit-testing.
pub fn stroke_runs(object: &Object) -> Vec<(Vec<Point>, f64)> {
    let Shape::Stroke {
        points,
        widths,
        highlight,
    } = &object.shape
    else {
        return vec![];
    };
    if points.len() < 2 {
        return vec![];
    }
    let factor = if *highlight { 5. } else { 1. };
    let width_at = |i: usize| widths.get(i).copied().unwrap_or(object.style.width) * factor;
    let mut runs = Vec::new();
    let mut start = 0;
    let mut width = width_at(0);
    for i in 1..points.len() {
        if i == points.len() - 1 || width_at(i) != width {
            let slice = &points[start..=i];
            runs.push((
                if *highlight {
                    slice.to_vec()
                } else {
                    smooth_pen(slice)
                },
                width,
            ));
            start = i;
            if i < points.len() - 1 {
                width = width_at(i);
            }
        }
    }
    runs
}
