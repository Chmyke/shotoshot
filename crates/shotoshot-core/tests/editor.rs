use shotoshot_core::*;

fn p(x: f64, y: f64) -> Point {
    Point { x, y }
}

const BIND: Modifiers = Modifiers {
    shift: false,
    alt: false,
    ctrl: true,
};

#[test]
fn a_new_editor_starts_with_the_arrow() {
    assert_eq!(Editor::default().tool, Tool::Arrow);
}

#[test]
fn closed_freehand_loop_survives_release() {
    let mut editor = Editor::default();
    editor.tool = Tool::Pen;
    editor.begin(p(10., 10.), Modifiers::default());
    for point in [p(100., 10.), p(100., 100.), p(10., 100.), p(10., 10.)] {
        editor.update(point);
    }
    editor.finish();
    assert_eq!(editor.objects().len(), 1);
}

#[test]
fn deleting_arrow_during_endpoint_drag_cancels_drag_safely() {
    let mut editor = Editor::default();
    editor.tool = Tool::Arrow;
    editor.begin(p(10., 10.), Modifiers::default());
    editor.update(p(100., 100.));
    editor.finish();
    editor.select_only(editor.objects()[0].id);
    editor.tool = Tool::Select;
    editor.begin(p(100., 100.), Modifiers::default());
    editor.update(p(120., 120.));
    editor.delete_selection();
    editor.update(p(130., 130.));
    editor.finish();
    assert!(editor.objects().is_empty());
    editor.undo();
    assert_eq!(editor.objects().len(), 1);
}

#[test]
fn attached_arrow_selection_uses_resolved_geometry() {
    let mut editor = Editor::default();
    editor.tool = Tool::Rectangle;
    editor.begin(p(10., 10.), Modifiers::default());
    editor.update(p(100., 100.));
    editor.finish();
    let target = editor.objects()[0].id;
    editor.tool = Tool::Arrow;
    editor.begin(p(300., 50.), BIND);
    editor.update(p(90., 50.));
    editor.finish();
    let arrow = editor.objects()[1].id;
    editor.select_only(target);
    editor.nudge(p(50., 0.));
    editor.select_only(arrow);
    let (a, b) = arrow_points(&editor.objects()[1], editor.objects()).unwrap();
    assert_eq!(editor.selection_bounds().unwrap(), Rect::new(a, b).unwrap());
}

#[test]
fn reversed_negative_region_and_invalid_coordinates() {
    let r = Rect::new(p(20., 40.), p(-30., -10.)).unwrap();
    assert_eq!(r.min, p(-30., -10.));
    assert_eq!(r.size(), p(50., 50.));
    assert!(Rect::new(p(f64::NAN, 0.), p(1., 1.)).is_none());
}

#[test]
fn circle_is_drawn_from_its_center() {
    let mut editor = Editor::default();
    editor.tool = Tool::Ellipse;
    editor.begin(p(100., 100.), Modifiers::default());
    editor.update(p(130., 140.));
    editor.finish();
    let bounds = editor.objects()[0].bounds();
    assert_eq!(bounds.min, p(50., 50.));
    assert_eq!(bounds.max, p(150., 150.));
}

#[test]
fn one_drag_is_one_undo_and_new_command_clears_redo() {
    let mut editor = Editor::default();
    editor.tool = Tool::Rectangle;
    editor.begin(p(10., 10.), Modifiers::default());
    for i in 20..120 {
        editor.update(p(i as f64, 80.));
    }
    editor.finish();
    assert_eq!(editor.objects().len(), 1);
    editor.undo();
    assert!(editor.objects().is_empty());
    editor.redo();
    assert_eq!(editor.objects().len(), 1);
    editor.undo();
    editor.add_text(p(0., 0.), "hello 🌍".into());
    editor.redo();
    assert_eq!(editor.objects().len(), 1);
    assert!(matches!(&editor.objects()[0].shape, Shape::Text { text, .. } if text == "hello 🌍"));
}

#[test]
fn cancel_gesture_preserves_document() {
    let mut editor = Editor::default();
    editor.begin(p(1., 1.), Modifiers::default());
    editor.update(p(80., 90.));
    editor.cancel();
    assert!(editor.objects().is_empty());
}

#[test]
fn ellipse_rectangle_modifiers_and_group_selection() {
    let mut e = Editor::default();
    e.tool = Tool::Ellipse;
    e.begin(
        p(100., 100.),
        Modifiers {
            alt: true,
            ctrl: false,
            shift: false,
        },
    );
    e.update(p(150., 125.));
    e.finish();
    assert_eq!(
        e.objects()[0].bounds(),
        Rect::new(p(50., 75.), p(150., 125.)).unwrap()
    );
    e.tool = Tool::Rectangle;
    e.begin(
        p(300., 300.),
        Modifiers {
            alt: true,
            ctrl: false,
            shift: true,
        },
    );
    e.update(p(330., 320.));
    e.finish();
    assert_eq!(
        e.objects()[1].bounds(),
        Rect::new(p(270., 270.), p(330., 330.)).unwrap()
    );
    e.tool = Tool::Select;
    e.begin(p(10., 10.), Modifiers::default());
    e.update(p(400., 400.));
    e.finish();
    assert_eq!(e.selected.len(), 2);
    e.nudge(p(10., 20.));
    assert_eq!(e.objects()[0].bounds().min, p(60., 95.));
    e.undo();
    assert_eq!(e.objects()[0].bounds().min, p(50., 75.));
}

#[test]
fn selected_shape_resizes_with_one_reversible_gesture() {
    let mut e = Editor::default();
    e.begin(p(100., 100.), Modifiers::default());
    e.update(p(200., 200.));
    e.finish();
    e.select_only(e.objects()[0].id);
    e.tool = Tool::Select;
    e.begin(p(200., 200.), Modifiers::default());
    e.update(p(300., 250.));
    e.finish();
    assert_eq!(e.objects()[0].bounds().max, p(300., 250.));
    e.undo();
    assert_eq!(e.objects()[0].bounds().max, p(200., 200.));
}

#[test]
fn moving_an_attached_arrow_alone_detaches_and_moves_both_ends() {
    let mut e = Editor::default();
    e.begin(p(10., 10.), Modifiers::default());
    e.update(p(100., 100.));
    e.finish();
    e.tool = Tool::Arrow;
    e.begin(p(300., 50.), BIND);
    e.update(p(90., 50.));
    e.finish();
    let original = arrow_points(&e.objects()[1], e.objects()).unwrap();
    e.select_only(e.objects()[1].id);
    e.nudge(p(0., 100.));
    let moved = arrow_points(&e.objects()[1], e.objects()).unwrap();
    assert_eq!(moved, (original.0 + p(0., 100.), original.1 + p(0., 100.)));
    e.undo();
    assert_eq!(
        arrow_points(&e.objects()[1], e.objects()).unwrap(),
        original
    );
}

#[test]
fn paths_are_repeatable_and_translation_preserves_geometry() {
    let mut editor = Editor::default();
    editor.tool = Tool::Rectangle;
    editor.begin(p(10., 10.), Modifiers::default());
    editor.update(p(200., 100.));
    editor.finish();
    let object = editor.objects()[0].clone();
    let a = paths(&object, editor.objects());
    assert_eq!(a, paths(&object, editor.objects()));
    let mut moved = object;
    moved.translate(p(30., 60.));
    let b = paths(&moved, &[moved.clone()]);
    for (first, second) in a.iter().flatten().zip(b.iter().flatten()) {
        assert!((second.x - first.x - 30.).abs() < 0.00001);
        assert!((second.y - first.y - 60.).abs() < 0.00001);
    }
}

#[test]
fn attached_arrow_tracks_shape_and_delete_undo_restores_attachment() {
    let mut editor = Editor::default();
    editor.tool = Tool::Rectangle;
    editor.begin(p(10., 10.), Modifiers::default());
    editor.update(p(100., 100.));
    editor.finish();
    let target = editor.objects()[0].id;
    editor.tool = Tool::Arrow;
    editor.begin(p(300., 50.), BIND);
    editor.update(p(90., 50.));
    editor.finish();
    let before = arrow_points(&editor.objects()[1], editor.objects()).unwrap();
    editor.select_only(target);
    editor.nudge(p(40., 0.));
    let after = arrow_points(&editor.objects()[1], editor.objects()).unwrap();
    assert!((after.1.x - before.1.x - 40.).abs() < 0.001);
    editor.delete_selection();
    assert_eq!(editor.objects().len(), 1);
    assert_eq!(
        arrow_points(&editor.objects()[0], editor.objects()).unwrap(),
        after
    );
    editor.undo();
    assert_eq!(editor.objects().len(), 2);
    assert_eq!(
        arrow_points(&editor.objects()[1], editor.objects()).unwrap(),
        after
    );
}

#[test]
fn changing_width_during_draw_preserves_gesture_and_one_undo() {
    for tool in [
        Tool::Pen,
        Tool::Highlight,
        Tool::Arrow,
        Tool::Rectangle,
        Tool::Ellipse,
    ] {
        let mut e = Editor::default();
        e.tool = tool;
        e.begin(p(20., 20.), Modifiers::default());
        e.update(p(80., 60.));
        let geometry = e.objects()[0].shape.clone();
        for width in [4., 2., 1., 5.] {
            e.style.width = width;
            e.apply_style();
            assert!(e.is_drawing(), "style change ended {tool:?} gesture");
            assert_eq!(e.objects().len(), 1);
            assert_eq!(e.objects()[0].shape, geometry);
            assert_eq!(e.objects()[0].style.width, width);
        }
        e.update(p(120., 100.));
        e.finish();
        let completed = e.objects().to_vec();
        e.undo();
        assert!(e.objects().is_empty());
        e.redo();
        assert_eq!(e.objects(), completed);
        e.begin(p(200., 200.), Modifiers::default());
        e.update(p(250., 250.));
        e.style.width = 9.;
        e.apply_style();
        assert!(e.cancel());
        assert_eq!(e.objects(), completed);
    }
}

#[test]
fn freehand_keeps_width_history_through_move_undo_and_cancel() {
    for tool in [Tool::Pen, Tool::Highlight] {
        let mut e = Editor::default();
        e.tool = tool;
        e.style.width = 4.;
        e.begin(p(20., 100.), Modifiers::default());
        for (width, x) in [(4., 80.), (8., 140.), (2., 200.)] {
            e.style.width = width;
            e.apply_style();
            e.update(p(x, 100.));
        }
        e.finish();
        let factor = if tool == Tool::Highlight { 5. } else { 1. };
        let widths = |o: &Object| stroke_runs(o).iter().map(|r| r.1).collect::<Vec<_>>();
        assert_eq!(
            widths(&e.objects()[0]),
            vec![4. * factor, 8. * factor, 2. * factor]
        );
        let before = e.objects().to_vec();
        e.undo();
        assert!(e.objects().is_empty());
        e.redo();
        assert_eq!(e.objects(), before);
        let mut moved = before[0].clone();
        moved.translate(p(10., 20.));
        assert_eq!(widths(&moved), widths(&before[0]));
        assert!(moved.hit(p(120., 123.), 0., &[]));
        assert!(!moved.hit(p(180., 123. + factor), 0., &[]));
        e.begin(p(30., 200.), Modifiers::default());
        e.update(p(80., 200.));
        e.style.width = 9.;
        e.apply_style();
        assert!(e.cancel());
        assert_eq!(e.objects(), before);
    }
}

#[test]
fn straight_line_replaces_endpoint_instead_of_recording_mouse_path() {
    let mut e = Editor::default();
    e.tool = Tool::Line;
    e.begin(Point { x: 10., y: 20. }, Modifiers::default());
    e.update(Point { x: 60., y: 150. });
    e.update(Point { x: 200., y: 40. });
    e.finish();
    let Shape::Arrow {
        start, end, head, ..
    } = e.objects()[0].shape
    else {
        panic!("line");
    };
    assert_eq!(
        (start, end),
        (Point { x: 10., y: 20. }, Point { x: 200., y: 40. })
    );
    assert!(!head);
    assert_eq!(paths(&e.objects()[0], e.objects()).len(), 1);
    e.undo();
    assert!(e.objects().is_empty());
    e.redo();
    assert_eq!(e.objects().len(), 1);
}

#[test]
fn clearing_removes_every_object_and_the_background_in_one_undo_step() {
    let mut editor = Editor::default();
    editor.tool = Tool::Rectangle;
    editor.begin(p(10., 10.), Modifiers::default());
    editor.update(p(60., 40.));
    editor.finish();
    editor.tool = Tool::Pen;
    editor.begin(p(80., 80.), Modifiers::default());
    editor.update(p(120., 90.));
    editor.finish();
    editor.fill_at(p(200., 200.), false);
    assert_eq!(editor.objects().len(), 2);
    assert!(
        editor.background().is_some(),
        "the bucket painted the background"
    );
    editor.clear();
    assert!(editor.objects().is_empty());
    assert_eq!(editor.background(), None);
    editor.undo();
    assert_eq!(editor.objects().len(), 2, "one undo brings everything back");
    assert!(editor.background().is_some());
    editor.redo();
    assert!(editor.objects().is_empty() && editor.background().is_none());
    // Nothing to clear records no step: the next undo is still the clear itself.
    editor.clear();
    editor.undo();
    assert_eq!(editor.objects().len(), 2);
}
