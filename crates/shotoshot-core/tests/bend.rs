use shotoshot_core::*;
fn p(x: f64, y: f64) -> Point {
    Point { x, y }
}
fn drawn(tool: Tool) -> Editor {
    let mut e = Editor::default();
    e.tool = tool;
    e.begin(p(0., 0.), Modifiers::default());
    e.update(p(200., 0.));
    e.finish();
    e
}
fn drag(e: &mut Editor, from: Point, to: Point) {
    e.begin(from, Modifiers::default());
    e.update(to);
    e.finish();
}
fn middle(e: &Editor) -> Point {
    let o = &e.objects()[0];
    let (a, b) = arrow_points(o, e.objects()).unwrap();
    bend_point(a, b, o.bend())
}

#[test]
fn the_arrow_just_drawn_bends_through_its_middle_in_one_undo_step() {
    for tool in [Tool::Arrow, Tool::Line] {
        let mut e = drawn(tool);
        assert_eq!(e.bend_handle(), Some((0, p(100., 0.))), "{tool:?}");
        drag(&mut e, p(102., 3.), p(120., 60.));
        assert_eq!(e.objects().len(), 1, "the press bent it instead of drawing");
        assert!((middle(&e) - p(120., 60.)).length() < 1e-9);
        assert_eq!(e.bend_handle().map(|(_, h)| h), Some(middle(&e)));
        e.undo();
        assert_eq!(e.objects()[0].bend(), Point::default());
        assert_eq!(e.bend_handle(), None, "undo is the next action");
    }
}
#[test]
fn near_its_chord_the_middle_springs_straight() {
    let mut e = drawn(Tool::Arrow);
    drag(&mut e, p(100., 0.), p(150., 7.));
    assert_eq!(e.objects()[0].bend(), Point::default());
    drag(&mut e, p(100., 0.), p(150., 9.));
    assert!(bent(e.objects()[0].bend()));
    // On a 250 % screen the spring reaches 20 px.
    let mut e = drawn(Tool::Arrow);
    e.ui_scale = 2.5;
    drag(&mut e, p(100., 0.), p(100., 19.));
    assert_eq!(e.objects()[0].bend(), Point::default());
}
#[test]
fn a_press_elsewhere_draws_and_hands_the_middle_to_the_new_arrow() {
    let mut e = drawn(Tool::Arrow);
    drag(&mut e, p(0., 100.), p(200., 100.));
    assert_eq!(e.objects().len(), 2);
    assert_eq!(e.bend_handle(), Some((1, p(100., 100.))));
    e.tool = Tool::Pen;
    assert_eq!(e.bend_handle(), None, "another tool hides it");
}
#[test]
fn a_lone_selected_arrow_shows_its_middle_and_bends_with_select() {
    let mut e = drawn(Tool::Arrow);
    e.tool = Tool::Select;
    assert_eq!(e.bend_handle(), None, "not selected");
    e.select_only(e.objects()[0].id);
    assert_eq!(e.bend_handle(), Some((0, p(100., 0.))));
    assert_eq!(
        e.hover_at(p(100., 0.)),
        None,
        "a handle, not an object to add"
    );
    drag(&mut e, p(100., 0.), p(100., -50.));
    assert!((middle(&e) - p(100., -50.)).length() < 1e-9);
    assert_eq!(e.selected, vec![e.objects()[0].id]);
}
#[test]
fn the_curve_follows_its_ends_when_they_move_turn_or_attach() {
    let mut e = Editor::default();
    e.tool = Tool::Rectangle;
    drag(&mut e, p(300., -20.), p(340., 20.));
    e.tool = Tool::Arrow;
    e.begin(p(0., 0.), Modifiers::default());
    e.update_with_modifiers(
        p(310., 0.),
        Modifiers {
            ctrl: true,
            ..Modifiers::default()
        },
    );
    e.finish();
    let arrow = e.objects()[1].id;
    let (_, h) = e.bend_handle().unwrap();
    drag(&mut e, h, h + p(0., 80.));
    let bend = e.objects()[1].bend();
    // Moving the target moves the attached end; the bend, relative, stays.
    e.tool = Tool::Select;
    e.select_only(e.objects()[0].id);
    drag(&mut e, p(320., -20.), p(320., 180.));
    let o = e.objects().iter().find(|o| o.id == arrow).unwrap();
    assert_eq!(o.bend(), bend);
    let (a, b) = arrow_points(o, e.objects()).unwrap();
    assert!((b.y - 200.).abs() < 1e-9, "the end followed: {b:?}");
    assert!(o.on_ink(bend_point(a, b, bend), 0., e.objects()));
    // Turning the arrow alone turns its curve with it.
    e.select_only(arrow);
    let before = e.objects()[1].clone();
    e.begin_rotation(p(0., -100.));
    e.update(p(100., 0.));
    e.finish();
    let turned = &e.objects()[1];
    assert_eq!(turned.bend(), before.bend());
    assert_eq!(turned.rotation, 0.);
}
#[test]
fn picking_follows_the_curve_and_the_frame_holds_its_bulge() {
    let mut e = drawn(Tool::Line);
    drag(&mut e, p(100., 0.), p(100., 120.));
    let o = &e.objects()[0];
    assert!(o.on_ink(p(100., 120.), 0., e.objects()));
    assert!(
        !o.on_ink(p(100., 0.), 7., e.objects()),
        "the chord is empty"
    );
    let r = o.resolved_bounds(e.objects());
    // A quadratic through (100, 120) halfway peaks there.
    assert!((r.max.y - 120.).abs() < 1e-9, "{r:?}");
    assert_eq!(o.bounds(), r);
}
#[test]
fn the_head_points_along_the_arrival_and_the_shaft_stops_inside_it() {
    let mut e = drawn(Tool::Arrow);
    drag(&mut e, p(100., 0.), p(100., 80.));
    let o = &e.objects()[0];
    let paths = paths(o, e.objects());
    let [shaft, head] = &paths[..] else {
        panic!("{paths:?}");
    };
    let tip = head[1];
    assert_eq!(tip, p(200., 0.));
    // Control (100, 160): the curve arrives from below left, at 58 degrees.
    let base = (head[0] + head[2]) * 0.5;
    let axis = tip - base;
    let arrival = p(200., 0.) - p(100., 160.);
    let cross = axis.x * arrival.y - axis.y * arrival.x;
    assert!(cross.abs() < 1e-6, "{axis:?} vs {arrival:?}");
    let size = o.style.width * 3.84;
    let end = *shaft.last().unwrap();
    assert!(((tip - end).length() - (size - 2.)).abs() < 1e-6);
    assert_eq!(shaft[0], p(0., 0.));
    assert!(shaft.len() > 10, "a smooth curve");
}
#[test]
fn copy_and_paste_keep_the_bend() {
    let mut e = drawn(Tool::Arrow);
    drag(&mut e, p(100., 0.), p(100., 40.));
    e.tool = Tool::Select;
    e.select_only(e.objects()[0].id);
    let copied = e.copy_selected();
    e.paste_objects(&copied, p(10., 10.));
    assert_eq!(e.objects().len(), 2);
    assert_eq!(e.objects()[1].bend(), e.objects()[0].bend());
}
