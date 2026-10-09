use shotoshot_core::*;

const RED: [u8; 3] = [237, 79, 92];
const BLACK: [u8; 3] = [28, 31, 40];

fn p(x: f64, y: f64) -> Point {
    Point { x, y }
}

fn draw(editor: &mut Editor, tool: Tool, from: Point, to: Point) -> u64 {
    editor.tool = tool;
    editor.begin(from, Modifiers::default());
    editor.update(to);
    editor.finish();
    editor.objects().last().unwrap().id
}

/// A rectangle from (100, 100) to (300, 200), drawn in red.
fn with_rectangle() -> (Editor, u64) {
    let mut editor = Editor::default();
    editor.style.color = RED;
    let id = draw(&mut editor, Tool::Rectangle, p(100., 100.), p(300., 200.));
    (editor, id)
}

fn fill_of(editor: &Editor, id: u64) -> Option<[u8; 3]> {
    editor.objects().iter().find(|o| o.id == id).unwrap().fill
}

#[test]
fn bucket_inside_a_rectangle_fills_that_rectangle_only() {
    let (mut editor, id) = with_rectangle();
    editor.style.color = BLACK;
    editor.fill_at(p(200., 150.), false);
    assert_eq!(fill_of(&editor, id), Some(BLACK));
    assert_eq!(editor.background(), None);
    assert_eq!(
        editor.objects()[0].style.color,
        RED,
        "outline keeps its colour"
    );
}

#[test]
fn bucket_outside_every_shape_fills_the_background() {
    let (mut editor, id) = with_rectangle();
    editor.style.color = BLACK;
    editor.fill_at(p(20., 20.), false);
    assert_eq!(editor.background(), Some(BLACK));
    assert_eq!(fill_of(&editor, id), None);
}

#[test]
fn bucket_on_an_empty_capture_fills_the_background() {
    let mut editor = Editor::default();
    editor.style.color = BLACK;
    editor.fill_at(p(5., 5.), false);
    assert_eq!(editor.background(), Some(BLACK));
}

#[test]
fn bucket_fills_the_topmost_shape_under_the_point() {
    let (mut editor, below) = with_rectangle();
    let above = draw(&mut editor, Tool::Rectangle, p(150., 120.), p(250., 180.));
    editor.fill_at(p(200., 150.), false);
    assert_eq!(fill_of(&editor, above), Some(RED));
    assert_eq!(fill_of(&editor, below), None);
}

#[test]
fn bucket_follows_the_real_outline_of_an_ellipse() {
    let mut editor = Editor::default();
    // Circles grow from their centre: this one spans (100, 100) to (300, 300).
    let id = draw(&mut editor, Tool::Ellipse, p(200., 200.), p(300., 200.));
    assert_eq!(editor.objects()[0].bounds().min, p(100., 100.));
    editor.fill_at(p(110., 110.), false);
    assert_eq!(
        fill_of(&editor, id),
        None,
        "the corner of its box is outside"
    );
    assert_eq!(editor.background(), Some(editor.style.color));
    editor.fill_at(p(200., 200.), false);
    assert!(fill_of(&editor, id).is_some());
}

#[test]
fn bucket_follows_a_turned_rectangle() {
    let (mut editor, id) = with_rectangle();
    editor.select_only(id);
    editor.tool = Tool::Select;
    let handle = editor.rotation_handle().unwrap();
    assert!(editor.begin_rotation(handle));
    // A quarter turn around (200, 150): the box now spans x 150..250, y 50..250.
    editor.update(p(300., 150.));
    editor.finish();
    editor.fill_at(p(120., 150.), false);
    assert_eq!(
        fill_of(&editor, id),
        None,
        "inside the old box, outside the turned one"
    );
    editor.undo();
    editor.fill_at(p(200., 60.), false);
    assert!(fill_of(&editor, id).is_some(), "inside the turned box only");
}

#[test]
fn pen_loops_arrows_and_text_are_not_fillable() {
    let mut editor = Editor::default();
    editor.tool = Tool::Pen;
    editor.begin(p(10., 10.), Modifiers::default());
    for point in [p(100., 10.), p(100., 100.), p(10., 100.), p(10., 10.)] {
        editor.update(point);
    }
    editor.finish();
    draw(&mut editor, Tool::Arrow, p(200., 50.), p(300., 50.));
    editor.add_text(p(200., 200.), "label".into());
    for inside in [p(50., 50.), p(250., 50.), p(210., 210.)] {
        editor.fill_at(inside, false);
        assert!(editor.background().is_some());
        assert!(editor.objects().iter().all(|o| o.fill.is_none()));
        editor.undo();
    }
}

#[test]
fn another_colour_recolours_and_the_same_colour_removes() {
    let (mut editor, id) = with_rectangle();
    for at in [p(200., 150.), p(20., 20.)] {
        let read = |e: &Editor| {
            if at.x > 100. {
                fill_of(e, id)
            } else {
                e.background()
            }
        };
        editor.style.color = BLACK;
        editor.fill_at(at, false);
        editor.style.color = RED;
        editor.fill_at(at, false);
        assert_eq!(read(&editor), Some(RED));
        editor.fill_at(at, false);
        assert_eq!(read(&editor), None);
    }
}

#[test]
fn clearing_removes_whatever_the_colour() {
    let (mut editor, id) = with_rectangle();
    editor.style.color = BLACK;
    editor.fill_at(p(200., 150.), false);
    editor.fill_at(p(20., 20.), false);
    editor.style.color = RED;
    editor.fill_at(p(200., 150.), true);
    assert_eq!(fill_of(&editor, id), None);
    assert_eq!(editor.background(), Some(BLACK), "one click, one fill");
    editor.fill_at(p(20., 20.), true);
    assert_eq!(editor.background(), None);
}

#[test]
fn clearing_through_a_hollow_shape_removes_the_background_seen_inside() {
    let (mut editor, id) = with_rectangle();
    editor.fill_at(p(20., 20.), false);
    editor.fill_at(p(200., 150.), true);
    assert_eq!(editor.background(), None);
    assert_eq!(fill_of(&editor, id), None);
}

#[test]
fn clearing_nothing_adds_no_history_step() {
    let mut editor = Editor::default();
    editor.fill_at(p(20., 20.), true);
    assert!(!editor.can_undo());
}

#[test]
fn fills_are_undone_and_redone_one_click_at_a_time() {
    let (mut editor, id) = with_rectangle();
    editor.style.color = BLACK;
    editor.fill_at(p(200., 150.), false);
    editor.fill_at(p(20., 20.), false);
    editor.style.color = RED;
    editor.fill_at(p(20., 20.), false);

    editor.undo();
    assert_eq!(editor.background(), Some(BLACK));
    editor.undo();
    assert_eq!(editor.background(), None);
    assert_eq!(fill_of(&editor, id), Some(BLACK));
    editor.undo();
    assert_eq!(fill_of(&editor, id), None);
    assert_eq!(
        editor.objects().len(),
        1,
        "the rectangle itself is one step further"
    );

    editor.redo();
    editor.redo();
    editor.redo();
    assert_eq!(fill_of(&editor, id), Some(BLACK));
    assert_eq!(editor.background(), Some(RED));
    assert!(!editor.can_redo());
}

#[test]
fn a_new_fill_after_undo_discards_the_redo_steps() {
    let mut editor = Editor::default();
    editor.fill_at(p(20., 20.), false);
    editor.undo();
    assert!(editor.can_redo());
    editor.style.color = BLACK;
    editor.fill_at(p(20., 20.), false);
    assert!(!editor.can_redo());
}

#[test]
fn a_filled_shape_is_grabbed_by_its_interior_and_a_hollow_one_by_its_edge() {
    let (mut editor, id) = with_rectangle();
    let all = editor.objects().to_vec();
    assert!(!all[0].hit(p(200., 150.), 7., &all));
    assert!(all[0].hit(p(100., 150.), 7., &all));
    editor.fill_at(p(200., 150.), false);
    let all = editor.objects().to_vec();
    assert!(all[0].hit(p(200., 150.), 7., &all));
    assert!(!all[0].hit(p(50., 150.), 7., &all));

    editor.tool = Tool::Select;
    editor.begin(p(200., 150.), Modifiers::default());
    editor.update(p(230., 150.));
    editor.finish();
    assert_eq!(editor.selected, [id]);
    assert_eq!(editor.objects()[0].bounds().min, p(130., 100.));
    assert!(
        fill_of(&editor, id).is_some(),
        "the fill moves with its shape"
    );
}

#[test]
fn a_fill_survives_recolouring_the_outline_and_duplication() {
    let (mut editor, id) = with_rectangle();
    editor.style.color = BLACK;
    editor.fill_at(p(200., 150.), false);
    editor.select_only(id);
    editor.style.color = RED;
    editor.apply_style();
    assert_eq!(fill_of(&editor, id), Some(BLACK));

    let copied = editor.copy_selected();
    assert!(editor.paste_objects(&copied, p(20., 20.)));
    assert_eq!(editor.objects().len(), 2);
    assert_eq!(editor.objects()[1].fill, Some(BLACK));
}

#[test]
fn deleting_a_filled_shape_removes_its_fill_and_keeps_the_background() {
    let (mut editor, id) = with_rectangle();
    editor.fill_at(p(200., 150.), false);
    editor.fill_at(p(20., 20.), false);
    editor.select_only(id);
    editor.delete_selection();
    assert!(editor.objects().is_empty());
    assert!(editor.background().is_some());
}

#[test]
fn bucket_ignores_invalid_points_and_an_open_gesture() {
    let mut editor = Editor::default();
    editor.fill_at(p(f64::NAN, 0.), false);
    assert_eq!(editor.background(), None);
    editor.tool = Tool::Rectangle;
    editor.begin(p(10., 10.), Modifiers::default());
    editor.update(p(50., 50.));
    editor.fill_at(p(400., 400.), false);
    assert!(
        !editor.is_drawing(),
        "the unfinished shape is cancelled first"
    );
    assert!(editor.objects().is_empty());
    assert_eq!(editor.background(), Some(editor.style.color));
}
