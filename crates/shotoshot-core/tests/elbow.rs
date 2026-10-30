//! Arrows pulled out of a text box by one of its four anchors (Mickaël, 2026-10-30): a
//! rounded elbow out of the side facing the target, binding on contact, Alt to keep it free.
use shotoshot_core::*;

fn p(x: f64, y: f64) -> Point {
    Point { x, y }
}
const YELLOW: [u8; 3] = [253, 216, 53];
/// A fitted text box whose panel spans `x - 8 .. x + 108` and `y - 5 .. y + 29`.
fn label(id: u64, x: f64, y: f64) -> Object {
    Object {
        rotation: 0.,
        id,
        shape: Shape::Text {
            position: p(x, y),
            text: "Note".into(),
            size: 20.,
            extent: p(100., 24.),
            label: Some(Label::Fit),
            ink: None,
        },
        style: Style {
            color: YELLOW,
            width: 3.,
        },
        fill: Vec::new(),
    }
}
fn rectangle(id: u64, min: Point, max: Point) -> Object {
    Object {
        rotation: 0.,
        id,
        shape: Shape::Rectangle(Rect { min, max }),
        style: Style::default(),
        fill: Vec::new(),
    }
}
fn editor(objects: Vec<Object>, tool: Tool) -> Editor {
    let last = objects.iter().map(|o| o.id).max().unwrap_or(0);
    let mut e = Editor::restore(objects, last, Vec::new());
    e.tool = tool;
    e
}
fn pull(e: &mut Editor, from: Point, to: Point, modifiers: Modifiers) {
    e.begin(from, modifiers);
    e.update_with_modifiers(to, modifiers);
    e.finish();
}
fn arrow(e: &Editor) -> &Object {
    e.objects()
        .iter()
        .rev()
        .find(|o| matches!(o.shape, Shape::Arrow { .. }))
        .expect("an arrow")
}
fn attachments(o: &Object) -> (Option<u64>, Option<u64>) {
    match o.shape {
        Shape::Arrow {
            start_attach,
            end_attach,
            ..
        } => (start_attach.map(|a| a.target), end_attach.map(|a| a.target)),
        _ => panic!("not an arrow"),
    }
}
fn corners(e: &Editor) -> Vec<Point> {
    elbow_corners(arrow(e), e.objects()).expect("an elbow")
}
fn close(a: Point, b: Point) -> bool {
    (a - b).length() < 1e-9
}
/// The anchors of the box at (100, 100): top, right, bottom, left.
const TOP: Point = Point { x: 150., y: 83. };
const RIGHT: Point = Point { x: 220., y: 112. };
const BOTTOM: Point = Point { x: 150., y: 141. };

#[test]
fn a_box_shows_four_anchors_a_dozen_pixels_out_of_its_sides() {
    let e = editor(vec![label(1, 100., 100.)], Tool::Arrow);
    let anchors = e.anchors(1).unwrap();
    assert_eq!(anchors, [TOP, RIGHT, BOTTOM, p(80., 112.)]);
    assert_eq!(e.anchor_owner(p(150., 112.)), Some(1), "over the box");
    assert_eq!(e.anchor_owner(RIGHT), Some(1), "and out to its anchors");
    assert_eq!(e.anchor_owner(p(400., 400.)), None, "not far away");
    assert_eq!(e.anchor_at(BOTTOM + p(5., 5.)), Some(1));
    assert_eq!(
        e.anchor_at(p(150., 112.)),
        None,
        "the box itself is no anchor"
    );
}
#[test]
fn anchors_belong_to_text_boxes_and_to_tools_that_point() {
    let mut plain = label(2, 100., 300.);
    if let Shape::Text { label, .. } = &mut plain.shape {
        *label = None;
    }
    let rect = rectangle(3, p(400., 100.), p(500., 200.));
    let mut e = editor(vec![label(1, 100., 100.), plain, rect], Tool::Arrow);
    assert_eq!(e.anchor_owner(p(150., 312.)), None, "a text without panel");
    assert_eq!(e.anchor_owner(p(450., 150.)), None, "a rectangle");
    for tool in [Tool::Pen, Tool::Rectangle, Tool::Label, Tool::Text] {
        e.tool = tool;
        assert_eq!(e.anchor_owner(p(150., 112.)), None, "{tool:?} hovering");
    }
    // A box selected alone keeps its anchors whatever the tool, wherever the pointer.
    e.select_only(1);
    e.tool = Tool::Label;
    assert_eq!(e.anchor_owner(p(700., 700.)), Some(1));
    assert_eq!(e.anchor_at(BOTTOM), Some(1));
}
#[test]
fn pulling_an_anchor_draws_an_elbow_in_the_box_colour_straight_out_of_its_side() {
    let mut e = editor(vec![label(1, 100., 100.)], Tool::Arrow);
    e.style.width = 3.;
    pull(&mut e, BOTTOM, p(400., 300.), Modifiers::default());
    assert_eq!(e.objects().len(), 2);
    let o = arrow(&e);
    assert!(matches!(
        o.shape,
        Shape::Arrow {
            elbow: true,
            head: true,
            ..
        }
    ));
    assert_eq!(o.style.color, YELLOW);
    assert_eq!(attachments(o), (Some(1), None));
    // Out of the bottom (below the panel by the round cap), down, then across to the point.
    assert_eq!(
        corners(&e),
        vec![p(150., 130.5), p(150., 300.), p(400., 300.)]
    );
    assert_eq!(e.bend_handle(), None, "an elbow has no middle to bend");
    e.undo();
    assert_eq!(e.objects().len(), 1, "one undo step");
}
#[test]
fn the_side_follows_the_target_whichever_anchor_was_pulled() {
    let mut e = editor(vec![label(1, 100., 100.)], Tool::Arrow);
    // Pulled from the top towards a point to the right, level with the box.
    pull(&mut e, TOP, p(500., 115.), Modifiers::default());
    let c = corners(&e);
    assert!(close(c[0], p(209.5, 112.)), "{c:?}");
    assert_eq!(c[1].y, 112., "leaves horizontally");
    assert!(close(*c.last().unwrap(), p(500., 115.)));
    // Moved below its target, the box sends the arrow out of its top.
    e.tool = Tool::Select;
    pull(&mut e, p(150., 112.), p(450., 412.), Modifiers::default());
    let c = corners(&e);
    assert!(close(c[0], p(450., 393.5)), "{c:?}");
    assert_eq!(c[1].x, 450., "leaves vertically");
}
#[test]
fn released_on_another_box_it_binds_and_enters_the_side_facing_it() {
    let boxes = vec![label(1, 100., 100.), label(2, 400., 300.)];
    let mut e = editor(boxes, Tool::Select);
    pull(&mut e, RIGHT, p(450., 312.), Modifiers::default());
    let o = arrow(&e);
    assert_eq!(attachments(o), (Some(1), Some(2)));
    // Out of the bottom of the first, into the top of the second, meeting halfway.
    let c = corners(&e);
    assert!(close(c[0], p(150., 130.5)), "{c:?}");
    assert!(close(*c.last().unwrap(), p(450., 295.)), "{c:?}");
    assert_eq!(c[c.len() - 2].x, 450., "enters vertically");
    // The head lies along that last run, pointing down into the box.
    let head = &paths(arrow(&e), e.objects())[1];
    assert!(close(head[1], p(450., 295.)));
    assert!(head[0].y < 295. && (head[0].y - head[2].y).abs() < 1e-9);
    // Moving the target beside the source turns both ends to face each other.
    pull(&mut e, p(450., 312.), p(750., 112.), Modifiers::default());
    let c = corners(&e);
    assert!(close(c[0], p(209.5, 112.)), "{c:?}");
    assert!(close(*c.last().unwrap(), p(692., 112.)), "{c:?}");
    assert_eq!(c.len(), 2, "level boxes join in a straight line");
}
#[test]
fn alt_keeps_the_end_free_over_a_box() {
    let mut e = editor(
        vec![label(1, 100., 100.), label(2, 400., 300.)],
        Tool::Arrow,
    );
    let alt = Modifiers {
        alt: true,
        ..Modifiers::default()
    };
    pull(&mut e, RIGHT, p(450., 312.), alt);
    assert_eq!(attachments(arrow(&e)), (Some(1), None));
}
#[test]
fn an_empty_rectangle_binds_on_its_outline_and_lets_the_inside_point_at_the_image() {
    let rect = rectangle(2, p(300., 300.), p(500., 450.));
    let mut e = editor(vec![label(1, 100., 100.), rect], Tool::Arrow);
    pull(&mut e, BOTTOM, p(400., 375.), Modifiers::default());
    assert_eq!(attachments(arrow(&e)), (Some(1), None), "inside: free");
    pull(&mut e, BOTTOM, p(306., 375.), Modifiers::default());
    assert_eq!(
        attachments(arrow(&e)),
        (Some(1), Some(2)),
        "near its outline"
    );
    assert_eq!(e.connect_target(), None, "only while pulling");
}
#[test]
fn the_target_shows_while_pulling() {
    let mut e = editor(
        vec![label(1, 100., 100.), label(2, 400., 300.)],
        Tool::Arrow,
    );
    e.begin(RIGHT, Modifiers::default());
    e.update(p(600., 600.));
    assert_eq!(e.connect_target(), None);
    e.update(p(450., 312.));
    assert_eq!(e.connect_target(), Some(2));
    assert!(e.cancel());
    assert_eq!(e.objects().len(), 2, "Escape leaves nothing");
}
#[test]
fn deleting_its_box_lets_the_arrow_go_where_it_was() {
    let mut e = editor(vec![label(1, 100., 100.)], Tool::Arrow);
    pull(&mut e, BOTTOM, p(400., 300.), Modifiers::default());
    e.select_only(1);
    e.delete_selection();
    let o = arrow(&e);
    assert_eq!(attachments(o), (None, None));
    assert!(close(
        arrow_points(o, e.objects()).unwrap().0,
        p(150., 130.5)
    ));
    e.undo();
    assert_eq!(
        attachments(arrow(&e)),
        (Some(1), None),
        "undo binds it again"
    );
}
#[test]
fn a_selected_handle_wins_over_an_anchor() {
    let mut e = editor(vec![label(1, 100., 100.)], Tool::Select);
    e.select_only(1);
    // The rotation handle sits 28 px above the frame, the top anchor 12 px: their grips meet.
    let handle = e.rotation_handle().unwrap();
    assert_eq!(e.anchor_at(handle), None);
    assert_eq!(e.anchor_at(TOP), Some(1));
}
#[test]
fn a_turned_box_sends_the_arrow_out_along_its_own_side() {
    let mut turned = label(1, 100., 100.);
    turned.rotation = std::f64::consts::FRAC_PI_2;
    let e = editor(vec![turned], Tool::Arrow);
    // A quarter turn clockwise: the box's top now faces right.
    let top = e.anchors(1).unwrap()[0];
    assert!(close(top, p(150. + 17. + 12., 112.)), "{top:?}");
}
