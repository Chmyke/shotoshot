//! A text on its panel behaves as one object: bounds, gestures, history and copies.
use shotoshot_core::*;

fn p(x: f64, y: f64) -> Point {
    Point { x, y }
}
fn label(e: &mut Editor, label: Label) -> u64 {
    e.add_label(
        p(100., 100.),
        "Note".into(),
        p(60., 30.),
        [250, 192, 66],
        20.,
        Some(label),
    )
    .unwrap()
}
fn shape(e: &Editor) -> (Point, f64, Point, Option<Label>) {
    match &e.objects()[0].shape {
        Shape::Text {
            position,
            size,
            extent,
            label,
            ..
        } => (*position, *size, *extent, *label),
        _ => panic!("text expected"),
    }
}

#[test]
fn the_panel_wraps_the_text_with_its_margins() {
    let mut e = Editor::default();
    label(&mut e, Label::Fit);
    // 20 px font: 8 px margins across, 5 px down.
    assert_eq!(
        e.objects()[0].local_bounds(),
        Rect {
            min: p(92., 95.),
            max: p(168., 135.)
        }
    );
    assert!(
        e.objects()[0].hit(p(93., 96.), 0., e.objects()),
        "a margin is grabbed"
    );
    assert_eq!(e.objects()[0].text_ink(), [0; 3], "dark ink on yellow");

    let mut e = Editor::default();
    label(&mut e, Label::Wrap(200.));
    assert_eq!(
        e.objects()[0].local_bounds().max,
        p(308., 135.),
        "a wrapped panel is as wide as its lines can be"
    );
}

#[test]
fn a_plain_text_is_unchanged() {
    let mut e = Editor::default();
    e.add_label(p(10., 10.), "x".into(), p(20., 30.), [9; 3], 24., None)
        .unwrap();
    let o = &e.objects()[0];
    assert_eq!(
        o.local_bounds(),
        Rect {
            min: p(10., 10.),
            max: p(30., 40.)
        }
    );
    assert_eq!(o.text_ink(), [9; 3]);
}

#[test]
fn a_bad_wrap_width_is_refused() {
    let mut e = Editor::default();
    for width in [0., -5., f64::NAN, f64::INFINITY] {
        let added = e.add_label(
            p(0., 0.),
            "x".into(),
            p(1., 1.),
            [0; 3],
            24.,
            Some(Label::Wrap(width)),
        );
        assert_eq!(added, None, "{width}");
    }
}

#[test]
fn moving_keeps_the_width_and_the_font() {
    let mut e = Editor::default();
    label(&mut e, Label::Wrap(200.));
    e.tool = Tool::Select;
    e.begin(p(150., 110.), Modifiers::default());
    e.update(p(180., 150.));
    e.finish();
    let (position, size, _, wrap) = shape(&e);
    assert_eq!(position, p(130., 140.));
    assert_eq!(size, 20.);
    assert_eq!(wrap, Some(Label::Wrap(200.)));
}

#[test]
fn a_corner_changes_a_wrapped_width_not_its_font() {
    let mut e = Editor::default();
    label(&mut e, Label::Wrap(200.));
    e.tool = Tool::Select;
    e.selected = vec![e.objects()[0].id];
    // Bottom-right corner of the panel (92,95)-(308,135) dragged 100 px to the left.
    e.begin(p(308., 135.), Modifiers::default());
    e.update(p(208., 180.));
    let id = e.objects()[0].id;
    e.reflow(id, p(90., 58.));
    e.finish();
    let (position, size, extent, wrap) = shape(&e);
    assert_eq!(size, 20.);
    assert_eq!(position, p(100., 100.), "the opposite corner stays");
    assert_eq!(wrap, Some(Label::Wrap(100.)));
    assert_eq!(extent, p(90., 58.), "the host laid the lines out again");
    e.undo();
    assert_eq!(shape(&e).3, Some(Label::Wrap(200.)));
    assert_eq!(shape(&e).2, p(60., 30.));
    e.redo();
    assert_eq!(shape(&e).3, Some(Label::Wrap(100.)));
}

#[test]
fn a_wrapped_width_never_shrinks_below_two_letters() {
    let mut e = Editor::default();
    label(&mut e, Label::Wrap(200.));
    e.tool = Tool::Select;
    e.selected = vec![e.objects()[0].id];
    e.begin(p(308., 135.), Modifiers::default());
    // The corner stops 4 px right of the opposite one: narrower than two letters.
    e.update(p(96., 140.));
    e.finish();
    assert_eq!(shape(&e).3, Some(Label::Wrap(20.)));
}

#[test]
fn a_fitted_panel_scales_with_its_text() {
    let mut e = Editor::default();
    label(&mut e, Label::Fit);
    e.tool = Tool::Select;
    e.selected = vec![e.objects()[0].id];
    // Panel (92,95)-(168,135), 40 px high, dragged to 80 px from its top-left corner.
    e.begin(p(168., 135.), Modifiers::default());
    e.update(p(244., 175.));
    e.finish();
    let (position, size, extent, _) = shape(&e);
    assert_eq!(size, 40.);
    assert_eq!(extent, p(120., 60.));
    assert_eq!(position, p(92. + 16., 95. + 10.), "margins follow the font");
}

#[test]
fn reflow_outside_a_gesture_changes_nothing() {
    let mut e = Editor::default();
    let id = label(&mut e, Label::Wrap(200.));
    e.reflow(id, p(1., 1.));
    assert_eq!(shape(&e).2, p(60., 30.));
}

#[test]
fn copies_and_colours_keep_the_panel() {
    let mut e = Editor::default();
    let id = label(&mut e, Label::Wrap(200.));
    e.selected = vec![id];
    let copied = e.copy_selected();
    assert!(e.paste_objects(&copied, p(16., 16.)));
    assert_eq!(e.objects().len(), 2);
    assert!(matches!(
        e.objects()[1].shape,
        Shape::Text {
            label: Some(Label::Wrap(200.)),
            ..
        }
    ));
    e.selected = vec![id];
    e.style.color = [28, 31, 40];
    e.apply_color();
    assert_eq!(
        e.objects()[0].style.color,
        [28, 31, 40],
        "the panel takes the colour"
    );
    assert_eq!(
        e.objects()[0].text_ink(),
        [255; 3],
        "and the ink turns light"
    );
}
