//! Stacking order: Page Up / Page Down move the selection through the pile of objects.
use shotoshot_core::*;

fn p(x: f64, y: f64) -> Point {
    Point { x, y }
}
/// Rectangles drawn in order, each returned id in the order they were drawn.
fn stack(rectangles: &[(Point, Point)]) -> Editor {
    let mut e = Editor::default();
    for &(a, b) in rectangles {
        e.begin(a, Modifiers::default());
        e.update(b);
        e.finish();
    }
    e.tool = Tool::Select;
    e
}
fn ids(e: &Editor) -> Vec<u64> {
    e.objects().iter().map(|o| o.id).collect()
}
/// Three rectangles: 1 at the bottom, 2 far away from it, 3 on top of both.
fn far_then_overlapping() -> Editor {
    stack(&[
        (p(0., 0.), p(100., 100.)),
        (p(500., 500.), p(600., 600.)),
        (p(50., 50.), p(550., 550.)),
    ])
}

#[test]
fn raising_steps_past_the_next_object_that_overlaps_the_selection() {
    let mut e = far_then_overlapping();
    e.select_only(1);
    e.raise_selection(false);
    assert_eq!(
        ids(&e),
        vec![2, 3, 1],
        "2 does not overlap 1: the step skips it"
    );
    assert_eq!(e.selected, vec![1]);
}
#[test]
fn lowering_steps_past_the_next_overlapping_object_below() {
    let mut e = far_then_overlapping();
    e.select_only(3);
    e.lower_selection(false);
    assert_eq!(
        ids(&e),
        vec![1, 3, 2],
        "2 overlaps 3: one step goes under it"
    );
    e.lower_selection(false);
    assert_eq!(ids(&e), vec![3, 1, 2]);
    e.lower_selection(false);
    assert_eq!(
        ids(&e),
        vec![3, 1, 2],
        "already at the bottom: nothing moves"
    );
}
#[test]
fn nothing_overlapping_above_leaves_the_stack_and_the_history_alone() {
    let mut e = stack(&[(p(0., 0.), p(100., 100.)), (p(500., 500.), p(600., 600.))]);
    e.select_only(1);
    e.raise_selection(false);
    assert_eq!(ids(&e), vec![1, 2]);
    e.undo();
    assert_eq!(
        e.objects().len(),
        1,
        "no step was recorded, so undo removes the last drawing"
    );
}
#[test]
fn to_the_end_sends_the_selection_to_the_top_or_the_bottom() {
    let mut e = far_then_overlapping();
    e.select_only(1);
    e.raise_selection(true);
    assert_eq!(ids(&e), vec![2, 3, 1]);
    e.select_only(3);
    e.lower_selection(true);
    assert_eq!(ids(&e), vec![3, 2, 1]);
}
#[test]
fn a_multiple_selection_moves_as_one_block_in_its_own_order() {
    let mut e = stack(&[
        (p(0., 0.), p(100., 100.)),
        (p(10., 10.), p(110., 110.)),
        (p(20., 20.), p(120., 120.)),
        (p(30., 30.), p(130., 130.)),
    ]);
    e.selected = vec![3, 1];
    e.raise_selection(false);
    assert_eq!(ids(&e), vec![2, 4, 1, 3]);
    e.lower_selection(true);
    assert_eq!(ids(&e), vec![1, 3, 2, 4]);
}
#[test]
fn restacking_is_one_undo_step_and_survives_later_undo_and_redo() {
    let mut e = far_then_overlapping();
    e.select_only(1);
    e.raise_selection(false);
    assert_eq!(ids(&e), vec![2, 3, 1]);
    // A later drawing, then its undo: the raised order must not snap back to creation order.
    e.tool = Tool::Rectangle;
    e.begin(p(700., 700.), Modifiers::default());
    e.update(p(750., 750.));
    e.finish();
    assert_eq!(ids(&e), vec![2, 3, 1, 4]);
    e.undo();
    assert_eq!(ids(&e), vec![2, 3, 1]);
    e.undo();
    assert_eq!(ids(&e), vec![1, 2, 3], "the restacking is its own step");
    e.redo();
    assert_eq!(ids(&e), vec![2, 3, 1]);
}
#[test]
fn undoing_a_deletion_puts_the_object_back_at_its_place_in_the_pile() {
    let mut e = far_then_overlapping();
    e.select_only(1);
    e.raise_selection(true);
    assert_eq!(ids(&e), vec![2, 3, 1]);
    e.select_only(3);
    e.delete_selection();
    assert_eq!(ids(&e), vec![2, 1]);
    e.undo();
    assert_eq!(
        ids(&e),
        vec![2, 3, 1],
        "3 returns between 2 and 1, not at the end"
    );
}
#[test]
fn undoing_a_clear_puts_the_whole_pile_back_in_order() {
    let mut e = far_then_overlapping();
    e.select_only(1);
    e.raise_selection(true);
    e.clear();
    assert!(e.objects().is_empty());
    e.undo();
    assert_eq!(ids(&e), vec![2, 3, 1]);
}
#[test]
fn a_restored_document_keeps_its_stacking_order_through_undo() {
    let mut drawn = far_then_overlapping();
    drawn.select_only(1);
    drawn.raise_selection(true);
    let objects = drawn.objects().to_vec();
    let mut e = Editor::restore(objects, drawn.last_id(), None);
    assert_eq!(ids(&e), vec![2, 3, 1]);
    e.tool = Tool::Select;
    e.select_only(2);
    e.nudge(p(1., 0.));
    e.undo();
    assert_eq!(ids(&e), vec![2, 3, 1], "undo must not sort the pile by id");
}
#[test]
fn rectangles_overlap_when_they_share_any_point() {
    let a = Rect::new(p(0., 0.), p(10., 10.)).unwrap();
    assert!(a.intersects(Rect::new(p(10., 10.), p(20., 20.)).unwrap()));
    assert!(a.intersects(Rect::new(p(2., 2.), p(3., 3.)).unwrap()));
    assert!(!a.intersects(Rect::new(p(11., 0.), p(20., 10.)).unwrap()));
    assert!(!a.intersects(Rect::new(p(0., 11.), p(10., 20.)).unwrap()));
}
