//! ROLE: The eraser, a disc of `Editor::eraser_size`: freehand ink goes where the disc passes,
//! other objects go whole, the whole sweep as one undo step.
//! INVARIANTS: Whole objects leave the pile only when the gesture ends; Escape gives back
//! everything, cut ink included. A cut stroke keeps its id on its first piece and its place.
use super::*;

/// Objects other than freehand strokes within `reach` of the segment `from`–`to`, sampled
/// every reach length so a fast sweep still catches what lies between two pointer positions.
/// In pile order, no repeat.
pub(super) fn touched(objects: &[Object], reach: f64, from: Point, to: Point) -> Vec<u64> {
    let steps = ((to - from).length() / reach).ceil().max(1.) as usize;
    let mut ids = Vec::new();
    for i in 0..=steps {
        let p = from + (to - from) * (i as f64 / steps as f64);
        for o in objects {
            if !matches!(o.shape, Shape::Stroke { .. })
                && !ids.contains(&o.id)
                && o.hit(p, reach, objects)
            {
                ids.push(o.id);
            }
        }
    }
    ids
}

/// Shortest distance between the segments `a`–`b` and `c`–`d`.
fn segments_distance(a: Point, b: Point, c: Point, d: Point) -> f64 {
    let cross =
        |o: Point, p: Point, q: Point| (p.x - o.x) * (q.y - o.y) - (p.y - o.y) * (q.x - o.x);
    let (d1, d2) = (cross(c, d, a), cross(c, d, b));
    let (d3, d4) = (cross(a, b, c), cross(a, b, d));
    if d1 * d2 < 0. && d3 * d4 < 0. {
        return 0.;
    }
    segment_distance(a, c, d)
        .min(segment_distance(b, c, d))
        .min(segment_distance(c, a, b))
        .min(segment_distance(d, a, b))
}

/// The part of `a`–`b`, as `(t0, t1)` along it, within `reach` of the sweep `from`–`to`. The
/// distance to a segment is convex along a line, so that part is a single interval.
fn erased_span(a: Point, b: Point, reach: f64, from: Point, to: Point) -> Option<(f64, f64)> {
    if segments_distance(a, b, from, to) > reach {
        return None;
    }
    let at = |t: f64| a + (b - a) * t;
    let inside = |t: f64| segment_distance(at(t), from, to) <= reach;
    // Somewhere inside: the distance's minimum, by ternary search.
    let (mut lo, mut hi) = (0., 1.);
    for _ in 0..60 {
        let (m1, m2) = (lo + (hi - lo) / 3., hi - (hi - lo) / 3.);
        if segment_distance(at(m1), from, to) <= segment_distance(at(m2), from, to) {
            hi = m2;
        } else {
            lo = m1;
        }
    }
    let middle = (lo + hi) / 2.;
    if !inside(middle) {
        return None;
    }
    // Each edge between an outside and an inside parameter, by bisection.
    let edge = |mut outside: f64, mut within: f64| {
        for _ in 0..50 {
            let t = (outside + within) / 2.;
            if inside(t) {
                within = t;
            } else {
                outside = t;
            }
        }
        within
    };
    let t0 = if inside(0.) { 0. } else { edge(0., middle) };
    let t1 = if inside(1.) { 1. } else { edge(1., middle) };
    Some((t0, t1))
}

/// What is left of a stroke once a disc of radius `reach` has swept from `from` to `to`: its
/// pieces as points and per-segment widths, or `None` when the sweep missed its ink. A piece
/// ends where the disc touches the ink's edge, so its round cap stays outside the disc;
/// pieces shorter than a pixel go, as a pen click that never moved does.
fn cut(object: &Object, reach: f64, from: Point, to: Point) -> Option<Vec<(Vec<Point>, Vec<f64>)>> {
    let Shape::Stroke {
        points,
        widths,
        highlight,
    } = &object.shape
    else {
        return None;
    };
    let factor = if *highlight { 5. } else { 1. };
    let width = |i: usize| widths.get(i).copied().unwrap_or(object.style.width);
    let widest = widths.iter().copied().fold(object.style.width, f64::max) * factor;
    let sweep = Rect::new(from, to)?.expanded(reach + widest);
    if points.len() < 2 || !object.bounds().intersects(sweep) {
        return None;
    }
    let mut pieces = Vec::new();
    let mut piece: Option<(Vec<Point>, Vec<f64>)> = None;
    let extend = |piece: &mut Option<(Vec<Point>, Vec<f64>)>, a: Point, b: Point, w: f64| {
        let (points, widths) = piece.get_or_insert_with(|| (vec![a], Vec::new()));
        points.push(b);
        widths.push(w);
    };
    let mut touched = false;
    for (i, pair) in points.windows(2).enumerate() {
        let (a, b, w) = (pair[0], pair[1], width(i));
        match erased_span(a, b, reach + w * factor / 2., from, to) {
            None => extend(&mut piece, a, b, w),
            Some((t0, t1)) => {
                touched = true;
                if t0 > 0. {
                    extend(&mut piece, a, a + (b - a) * t0, w);
                }
                pieces.extend(piece.take());
                if t1 < 1. {
                    extend(&mut piece, a + (b - a) * t1, b, w);
                }
            }
        }
    }
    if !touched {
        return None;
    }
    pieces.extend(piece);
    let length = |points: &[Point]| {
        points
            .windows(2)
            .map(|w| (w[1] - w[0]).length())
            .sum::<f64>()
    };
    pieces.retain(|(points, _)| length(points) >= 1.);
    if widths.is_empty() {
        // A uniform stroke stays uniform: its pieces follow its style's width.
        for (_, widths) in &mut pieces {
            widths.clear();
        }
    }
    Some(pieces)
}

impl Editor {
    /// Objects the current sweep will remove on release; empty outside an erase gesture.
    pub fn erasing(&self) -> &[u64] {
        match &self.gesture {
            Some(Gesture {
                kind: GestureKind::Erase { touched },
                ..
            }) => touched,
            _ => &[],
        }
    }
    /// The eraser's disc going from `from` to `to`: whole objects it touches are marked for
    /// release, freehand ink under it goes now.
    pub(super) fn erase_along(&mut self, from: Point, to: Point) {
        let reach = self.eraser_size / 2.;
        let hits = touched(&self.objects, reach, from, to);
        if let Some(Gesture {
            kind: GestureKind::Erase { touched },
            ..
        }) = &mut self.gesture
        {
            for id in hits {
                if !touched.contains(&id) {
                    touched.push(id);
                }
            }
        }
        let mut i = 0;
        while i < self.objects.len() {
            let Some(pieces) = cut(&self.objects[i], reach, from, to) else {
                i += 1;
                continue;
            };
            let original = self.objects.remove(i);
            let highlight = matches!(
                original.shape,
                Shape::Stroke {
                    highlight: true,
                    ..
                }
            );
            let count = pieces.len();
            for (k, (points, widths)) in pieces.into_iter().enumerate() {
                let id = if k == 0 {
                    original.id
                } else {
                    self.next_id += 1;
                    self.next_id
                };
                self.objects.insert(
                    i + k,
                    Object {
                        id,
                        shape: Shape::Stroke {
                            points,
                            widths,
                            highlight,
                        },
                        ..original.clone()
                    },
                );
            }
            i += count;
        }
    }
    /// Removes `ids` as one reversible step. An arrow attached to one of them keeps its place
    /// and loses the attachment, as with the Delete key.
    pub fn remove_objects(&mut self, ids: &[u64]) {
        self.cancel();
        let before = self.objects.clone();
        self.drop_objects(ids);
        self.commit(before);
    }
    /// `remove_objects` without its own undo step, for a gesture that commits once.
    pub(super) fn drop_objects(&mut self, ids: &[u64]) {
        let before = self.objects.clone();
        for o in &mut self.objects {
            let (start_cap, end_cap) = o.end_caps();
            if let Shape::Arrow {
                start,
                end,
                start_attach,
                end_attach,
                ..
            } = &mut o.shape
            {
                if start_attach.is_some_and(|a| ids.contains(&a.target)) {
                    *start = start_attach
                        .and_then(|a| resolve(a, &before, start_cap))
                        .unwrap_or(*start);
                    *start_attach = None;
                }
                if end_attach.is_some_and(|a| ids.contains(&a.target)) {
                    *end = end_attach
                        .and_then(|a| resolve(a, &before, end_cap))
                        .unwrap_or(*end);
                    *end_attach = None;
                }
            }
        }
        self.objects.retain(|o| !ids.contains(&o.id));
        self.selected.retain(|id| !ids.contains(id));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(x: f64, y: f64) -> Point {
        Point { x, y }
    }
    fn draw(e: &mut Editor, tool: Tool, from: Point, to: Point) -> u64 {
        e.tool = tool;
        e.begin(from, Modifiers::default());
        e.update(to);
        e.finish();
        e.objects().last().unwrap().id
    }
    fn sweep(e: &mut Editor, path: &[Point]) {
        e.tool = Tool::Eraser;
        e.begin(path[0], Modifiers::default());
        for q in &path[1..] {
            e.update(*q);
        }
        e.finish();
    }

    #[test]
    fn a_click_removes_the_object_under_the_pointer_and_leaves_the_rest() {
        let mut e = Editor::default();
        let box1 = draw(&mut e, Tool::Rectangle, p(10., 10.), p(100., 80.));
        let box2 = draw(&mut e, Tool::Rectangle, p(200., 10.), p(300., 80.));
        sweep(&mut e, &[p(10., 40.)]);
        let ids: Vec<u64> = e.objects().iter().map(|o| o.id).collect();
        assert_eq!(
            ids,
            [box2],
            "the first box's left edge was under the pointer"
        );
        assert!(!ids.contains(&box1));
        assert!(e.erasing().is_empty(), "nothing pending after release");
    }

    #[test]
    fn a_sweep_removes_every_object_it_touched_in_one_undo_step() {
        let mut e = Editor::default();
        draw(&mut e, Tool::Line, p(50., 0.), p(50., 100.));
        draw(&mut e, Tool::Line, p(150., 0.), p(150., 100.));
        let kept = draw(&mut e, Tool::Line, p(400., 0.), p(400., 100.));
        e.tool = Tool::Eraser;
        e.begin(p(0., 50.), Modifiers::default());
        e.update(p(100., 50.));
        assert_eq!(
            e.erasing().len(),
            1,
            "the first line is marked, still in the pile"
        );
        assert_eq!(e.objects().len(), 3);
        e.update(p(200., 50.));
        assert_eq!(e.erasing().len(), 2);
        e.finish();
        assert_eq!(e.objects().iter().map(|o| o.id).collect::<Vec<_>>(), [kept]);
        e.undo();
        assert_eq!(e.objects().len(), 3, "one step gives everything back");
    }

    #[test]
    fn a_sweep_catches_what_lies_between_two_pointer_samples() {
        let mut e = Editor::default();
        e.style.width = 1.;
        draw(&mut e, Tool::Line, p(300., 0.), p(300., 100.));
        // Two samples 600 px apart, neither within reach of the line.
        sweep(&mut e, &[p(0., 50.), p(600., 50.)]);
        assert!(
            e.objects().is_empty(),
            "the segment between the samples crosses the line"
        );
    }

    #[test]
    fn escape_during_a_sweep_keeps_every_object() {
        let mut e = Editor::default();
        draw(&mut e, Tool::Rectangle, p(10., 10.), p(100., 80.));
        e.tool = Tool::Eraser;
        e.begin(p(10., 40.), Modifiers::default());
        assert_eq!(e.erasing().len(), 1);
        assert!(e.cancel());
        assert_eq!(e.objects().len(), 1);
        e.undo();
        assert!(e.objects().is_empty(), "the drawing was the only step");
    }

    #[test]
    fn an_arrow_attached_to_an_erased_shape_stays_and_lets_go() {
        let mut e = Editor::default();
        let shape = draw(&mut e, Tool::Rectangle, p(100., 100.), p(200., 200.));
        e.tool = Tool::Arrow;
        e.begin(
            p(10., 50.),
            Modifiers {
                ctrl: true,
                ..Modifiers::default()
            },
        );
        e.update(p(150., 150.));
        e.finish();
        assert!(
            matches!(
                e.objects()[1].shape,
                Shape::Arrow {
                    end_attach: Some(_),
                    ..
                }
            ),
            "the arrow binds to the box"
        );
        // The box's bottom edge, far from the arrow, which ends on its left edge.
        sweep(&mut e, &[p(150., 200.)]);
        assert_eq!(e.objects().len(), 1);
        assert_ne!(e.objects()[0].id, shape);
        assert!(matches!(
            e.objects()[0].shape,
            Shape::Arrow {
                end_attach: None,
                ..
            }
        ));
    }

    #[test]
    fn delete_key_and_eraser_share_one_removal() {
        let mut e = Editor::default();
        let id = draw(&mut e, Tool::Ellipse, p(100., 100.), p(150., 100.));
        e.selected = vec![id];
        e.delete_selection();
        assert!(e.objects().is_empty());
        assert!(e.selected.is_empty());
        e.undo();
        assert_eq!(e.objects().len(), 1);
    }

    #[test]
    fn touching_lists_in_pile_order_without_repeats() {
        let mut e = Editor::default();
        let a = draw(&mut e, Tool::Rectangle, p(0., 0.), p(100., 100.));
        let b = draw(&mut e, Tool::Rectangle, p(0., 0.), p(100., 100.));
        assert_eq!(touched(e.objects(), 7., p(0., 50.), p(0., 60.)), [a, b]);
        assert!(touched(e.objects(), 7., p(500., 500.), p(500., 500.)).is_empty());
    }

    fn strokes(e: &Editor) -> Vec<(u64, Vec<Point>)> {
        e.objects()
            .iter()
            .filter_map(|o| match &o.shape {
                Shape::Stroke { points, .. } => Some((o.id, points.clone())),
                _ => None,
            })
            .collect()
    }
    fn near(a: Point, b: Point) -> bool {
        (a - b).length() < 1e-6
    }

    #[test]
    fn a_click_on_a_pen_stroke_cuts_out_only_the_disc_in_one_undo_step() {
        let mut e = Editor::default();
        e.style.width = 3.;
        let id = draw(&mut e, Tool::Pen, p(0., 50.), p(200., 50.));
        sweep(&mut e, &[p(100., 50.)]);
        let pieces = strokes(&e);
        assert_eq!(pieces.len(), 2, "the stroke is cut in two");
        assert_eq!(pieces[0].0, id, "the first piece keeps the stroke's id");
        // The disc's radius 7 plus half the ink: each cap just touches the disc.
        assert!(near(*pieces[0].1.last().unwrap(), p(91.5, 50.)));
        assert!(near(pieces[1].1[0], p(108.5, 50.)));
        assert!(near(*pieces[1].1.last().unwrap(), p(200., 50.)));
        e.undo();
        assert_eq!(strokes(&e), [(id, vec![p(0., 50.), p(200., 50.)])]);
        e.redo();
        assert_eq!(strokes(&e).len(), 2);
    }

    #[test]
    fn ink_goes_as_the_disc_passes_and_escape_gives_it_back() {
        let mut e = Editor::default();
        draw(&mut e, Tool::Pen, p(0., 50.), p(200., 50.));
        e.tool = Tool::Eraser;
        e.begin(p(50., 0.), Modifiers::default());
        e.update(p(50., 100.));
        assert_eq!(
            strokes(&e).len(),
            2,
            "cut before release, between two samples"
        );
        assert!(e.erasing().is_empty(), "ink is not faded, it is gone");
        e.update(p(150., 100.));
        e.update(p(150., 0.));
        assert_eq!(strokes(&e).len(), 3);
        assert!(e.cancel());
        assert_eq!(strokes(&e).len(), 1, "Escape puts the whole stroke back");
        e.undo();
        assert!(e.objects().is_empty(), "the drawing was the only step");
    }

    #[test]
    fn a_sweep_over_the_whole_stroke_removes_it() {
        let mut e = Editor::default();
        draw(&mut e, Tool::Pen, p(0., 50.), p(100., 50.));
        sweep(&mut e, &[p(-20., 50.), p(120., 50.)]);
        assert!(e.objects().is_empty());
        e.undo();
        assert_eq!(e.objects().len(), 1);
    }

    #[test]
    fn the_highlighter_is_cut_at_its_wider_ink_and_the_size_is_the_disc() {
        let mut e = Editor::default();
        e.style.width = 3.;
        draw(&mut e, Tool::Highlight, p(0., 50.), p(200., 50.));
        sweep(&mut e, &[p(100., 50.)]);
        // Highlighter ink is five times the width: 7 + 7.5.
        assert!(near(*strokes(&e)[0].1.last().unwrap(), p(85.5, 50.)));
        let mut e = Editor::default();
        e.style.width = 3.;
        e.eraser_size = 40.;
        draw(&mut e, Tool::Pen, p(0., 50.), p(200., 50.));
        sweep(&mut e, &[p(100., 50.)]);
        assert!(near(*strokes(&e)[0].1.last().unwrap(), p(78.5, 50.)));
    }

    #[test]
    fn pieces_keep_the_stroke_place_in_the_pile_and_its_style() {
        let mut e = Editor::default();
        let below = draw(&mut e, Tool::Rectangle, p(300., 0.), p(400., 100.));
        e.style.color = [1, 2, 3];
        let ink = draw(&mut e, Tool::Pen, p(0., 50.), p(200., 50.));
        let above = draw(&mut e, Tool::Rectangle, p(500., 0.), p(600., 100.));
        sweep(&mut e, &[p(100., 50.)]);
        let pile: Vec<u64> = e.objects().iter().map(|o| o.id).collect();
        assert_eq!(pile.len(), 4);
        assert_eq!((pile[0], pile[1], pile[3]), (below, ink, above));
        assert!(pile[2] > above, "the new piece takes a fresh id");
        assert!(e.objects()[1..3].iter().all(|o| o.style.color == [1, 2, 3]));
    }

    #[test]
    fn a_stroke_with_width_changes_keeps_each_segment_width() {
        let mut e = Editor::default();
        e.style.width = 2.;
        e.tool = Tool::Pen;
        e.begin(p(0., 50.), Modifiers::default());
        e.update(p(100., 50.));
        e.style.width = 8.;
        e.apply_style();
        e.update(p(200., 50.));
        e.finish();
        sweep(&mut e, &[p(100., 50.)]);
        let widths: Vec<Vec<f64>> = e
            .objects()
            .iter()
            .map(|o| match &o.shape {
                Shape::Stroke { widths, .. } => widths.clone(),
                _ => panic!("stroke"),
            })
            .collect();
        assert_eq!(widths.len(), 2);
        assert!(widths[0].iter().all(|w| *w == 2.));
        assert!(widths[1].iter().all(|w| *w == 8.));
    }
}
