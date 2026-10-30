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
        None,
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
    e.add_label(
        p(10., 10.),
        "x".into(),
        p(20., 30.),
        [9; 3],
        24.,
        None,
        Some([1; 3]),
    )
    .unwrap();
    let o = &e.objects()[0];
    assert_eq!(
        o.local_bounds(),
        Rect {
            min: p(10., 10.),
            max: p(30., 40.)
        }
    );
    assert_eq!(o.text_ink(), [9; 3], "its own colour");
    assert!(
        matches!(o.shape, Shape::Text { ink: None, .. }),
        "no ink is kept without a panel"
    );
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
            None,
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

fn ink(e: &Editor, index: usize) -> Option<[u8; 3]> {
    match e.objects()[index].shape {
        Shape::Text { ink, .. } => ink,
        _ => panic!("text expected"),
    }
}

#[test]
fn a_given_ink_outlives_panel_colours_until_it_goes_back_to_automatic() {
    let mut e = Editor::default();
    let id = label(&mut e, Label::Fit);
    let plain = e
        .add_label(
            p(10., 10.),
            "x".into(),
            p(20., 30.),
            [9; 3],
            24.,
            None,
            None,
        )
        .unwrap();
    e.selected = vec![id, plain];
    assert!(e.apply_ink(Some([237, 79, 92])));
    assert_eq!(e.objects()[0].text_ink(), [237, 79, 92]);
    assert_eq!(ink(&e, 1), None, "a plain text keeps its own colour");
    assert_eq!(e.objects()[1].text_ink(), [9; 3]);
    e.style.color = [28, 31, 40];
    e.apply_color();
    assert_eq!(
        e.objects()[0].text_ink(),
        [237, 79, 92],
        "a new panel colour keeps the given ink"
    );
    let copied = e.copy_selected();
    assert!(e.paste_objects(&copied, p(16., 16.)));
    assert_eq!(ink(&e, 2), Some([237, 79, 92]), "a copy keeps it");
    e.selected = vec![id];
    assert!(e.apply_ink(None));
    assert_eq!(e.objects()[0].text_ink(), [255; 3], "automatic again");
    assert!(!e.apply_ink(None), "nothing left to change");
    e.undo();
    assert_eq!(ink(&e, 0), Some([237, 79, 92]), "one step back");
    e.undo();
    e.undo();
    assert_eq!(ink(&e, 0), Some([237, 79, 92]));
    e.undo();
    assert_eq!(ink(&e, 0), None, "the ink was one undo step");
}

#[test]
fn an_edited_panel_keeps_its_ink_or_takes_the_new_one() {
    let mut e = Editor::default();
    let id = e
        .add_label(
            p(100., 100.),
            "Note".into(),
            p(60., 30.),
            [250, 192, 66],
            20.,
            Some(Label::Fit),
            Some([1, 2, 3]),
        )
        .unwrap();
    e.update_text_measured_sized_colored(id, "Notes".into(), p(70., 30.), [0; 3], 20.);
    assert_eq!(ink(&e, 0), Some([1, 2, 3]));
    e.update_text_inked(id, "Notes".into(), p(70., 30.), [0; 3], 20., None);
    assert_eq!(ink(&e, 0), None);
    e.undo();
    assert_eq!(ink(&e, 0), Some([1, 2, 3]), "one undo step");
}

fn empty_box(e: &mut Editor, label: Label, extent: Point) -> Option<u64> {
    e.add_label(
        p(100., 100.),
        String::new(),
        extent,
        [250, 192, 66],
        20.,
        Some(label),
        None,
    )
}
#[test]
fn a_box_validated_empty_stays_at_its_size_and_a_bare_text_does_not() {
    let mut e = Editor::default();
    let id = empty_box(&mut e, Label::Wrap(150.), p(150., 80.)).expect("an empty box stays");
    assert_eq!(shape(&e).2, p(150., 80.));
    assert!(
        e.objects()[0].hit(p(200., 150.), 0., e.objects()),
        "grabbed inside"
    );
    assert_eq!(
        e.add_label(p(0., 0.), " ".into(), p(5., 24.), [0; 3], 20., None, None),
        None,
        "the Text tool still drops an empty text"
    );
    // Typed in afterwards, it follows its text like any box.
    e.update_text_inked(id, "Hi".into(), p(150., 24.), [250, 192, 66], 20., None);
    assert_eq!(shape(&e).2, p(150., 24.));
}
#[test]
fn an_empty_box_keeps_its_height_through_corner_and_wheel() {
    let mut e = Editor::default();
    let id = empty_box(&mut e, Label::Wrap(150.), p(150., 80.)).unwrap();
    e.tool = Tool::Select;
    e.selected = vec![id];
    // Bottom-right corner of the panel (92,95)-(258,185) pulled 50 px in.
    e.begin(p(258., 185.), Modifiers::default());
    e.update(p(208., 185.));
    e.reflow(id, p(100., 24.));
    e.finish();
    assert_eq!(
        shape(&e).2,
        p(100., 80.),
        "one empty line measured, drawn height kept"
    );
    // A font twice as large doubles it, as it would its line.
    assert!(e.set_text_size(id, 40., p(100., 48.)));
    assert_eq!(shape(&e).2, p(100., 160.));
}
#[test]
fn a_clicked_empty_box_is_one_line_at_least_two_letters_wide() {
    assert_eq!(
        kept_block("", Some(Label::Fit), p(0., 24.), 20., 0.),
        p(20., 24.)
    );
    assert_eq!(
        kept_block("Note", Some(Label::Fit), p(40., 24.), 20., 90.),
        p(40., 24.),
        "a box with text follows it"
    );
    assert_eq!(kept_block("", None, p(0., 24.), 20., 90.), p(0., 24.));
}
