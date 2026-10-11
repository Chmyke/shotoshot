//! ROLE: The eraser: whole objects under the pointer or along its sweep go, as one undo step.
//! INVARIANTS: Nothing leaves the pile before the gesture ends; Escape keeps everything.
use super::*;

/// Objects within selection reach of the segment `from`–`to`, sampled every reach length so a
/// fast sweep still catches what lies between two pointer positions. In pile order, no repeat.
pub(super) fn touched(objects: &[Object], ui_scale: f64, from: Point, to: Point) -> Vec<u64> {
    let reach = 7. * ui_scale;
    let steps = ((to - from).length() / reach).ceil().max(1.) as usize;
    let mut ids = Vec::new();
    for i in 0..=steps {
        let p = from + (to - from) * (i as f64 / steps as f64);
        for o in objects {
            if !ids.contains(&o.id) && o.hit(p, reach, objects) {
                ids.push(o.id);
            }
        }
    }
    ids
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
    /// Removes `ids` as one reversible step. An arrow attached to one of them keeps its place
    /// and loses the attachment, as with the Delete key.
    pub fn remove_objects(&mut self, ids: &[u64]) {
        self.cancel();
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
        self.commit(before);
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
        assert_eq!(touched(e.objects(), 1., p(0., 50.), p(0., 60.)), [a, b]);
        assert!(touched(e.objects(), 1., p(500., 500.), p(500., 500.)).is_empty());
    }
}
