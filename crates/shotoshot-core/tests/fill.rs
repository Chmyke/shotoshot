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

/// The colour of the top coat.
fn top(coats: &[Paint]) -> Option<[u8; 3]> {
    coats.last().map(|c| c.color)
}

fn fill_of(editor: &Editor, id: u64) -> Option<[u8; 3]> {
    top(&editor.objects().iter().find(|o| o.id == id).unwrap().fill)
}

fn background(editor: &Editor) -> Option<[u8; 3]> {
    top(editor.background())
}

fn colour_of(editor: &Editor, id: u64) -> [u8; 3] {
    let object = editor.objects().iter().find(|o| o.id == id).unwrap();
    object.style.color
}

#[test]
fn bucket_inside_a_rectangle_fills_that_rectangle_only() {
    let (mut editor, id) = with_rectangle();
    editor.style.color = BLACK;
    editor.fill_at(p(200., 150.), false);
    assert_eq!(fill_of(&editor, id), Some(BLACK));
    assert_eq!(background(&editor), None);
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
    assert_eq!(background(&editor), Some(BLACK));
    assert_eq!(fill_of(&editor, id), None);
}

#[test]
fn bucket_on_an_empty_capture_fills_the_background() {
    let mut editor = Editor::default();
    editor.style.color = BLACK;
    editor.fill_at(p(5., 5.), false);
    assert_eq!(background(&editor), Some(BLACK));
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
    assert_eq!(background(&editor), Some(editor.style.color));
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

/// A pen stroke through `points`, in the editor's current colour and width.
fn pen(editor: &mut Editor, points: &[Point]) -> u64 {
    editor.tool = Tool::Pen;
    editor.begin(points[0], Modifiers::default());
    for point in &points[1..] {
        editor.update(*point);
    }
    editor.finish();
    editor.objects().last().unwrap().id
}

/// A square loop from (10, 10) to (100, 100), its hand lifted `gap` pixels short of the start.
fn pen_loop(editor: &mut Editor, gap: f64) -> u64 {
    pen(
        editor,
        &[
            p(10., 10.),
            p(100., 10.),
            p(100., 100.),
            p(10., 100.),
            p(10., 10. + gap),
        ],
    )
}

#[test]
fn arrows_and_text_are_not_fillable() {
    let mut editor = Editor::default();
    draw(&mut editor, Tool::Arrow, p(200., 50.), p(300., 50.));
    editor.add_text(p(200., 200.), "label".into(), TextSpec::default());
    // Beside the arrow, the click reaches the capture behind it.
    editor.fill_at(p(250., 70.), false);
    assert!(background(&editor).is_some());
    assert!(editor.objects().iter().all(|o| o.fill.is_empty()));
}

#[test]
fn bucket_fills_a_pen_loop_closed_by_hand() {
    // Mickaël, 2026-11-08: the heart drawn with the pen fills like a circle.
    let mut editor = Editor::default();
    let id = pen_loop(&mut editor, 10.);
    editor.style.color = BLACK;
    editor.fill_at(p(55., 55.), false);
    assert_eq!(fill_of(&editor, id), Some(BLACK));
    assert_eq!(background(&editor), None);
    assert_eq!(
        colour_of(&editor, id),
        Style::default().color,
        "the ink keeps its colour"
    );
    // The inside of a filled loop is solid to the eye: it is grabbed like the outline.
    assert!(editor.objects()[0].hit(p(55., 55.), 0., editor.objects()));
    editor.fill_at(p(55., 55.), false);
    assert_eq!(
        fill_of(&editor, id),
        None,
        "the same colour again takes it off"
    );
}

#[test]
fn an_open_stroke_or_a_line_drawn_there_and_back_fills_the_capture() {
    for points in [
        // Its hand lifted far from the start.
        vec![
            p(10., 10.),
            p(100., 10.),
            p(100., 100.),
            p(10., 100.),
            p(10., 60.),
        ],
        // Back where it started, but around nothing.
        vec![p(10., 10.), p(100., 12.), p(12., 14.)],
    ] {
        let mut editor = Editor::default();
        pen(&mut editor, &points);
        assert!(!editor.objects()[0].closed(), "{points:?}");
        editor.fill_at(p(55., 55.), false);
        assert!(background(&editor).is_some(), "{points:?}");
        assert!(editor.objects()[0].fill.is_empty());
    }
}

#[test]
fn a_highlighter_loop_is_never_filled() {
    let mut editor = Editor::default();
    editor.tool = Tool::Highlight;
    editor.begin(p(10., 10.), Modifiers::default());
    for point in [p(100., 10.), p(100., 100.), p(10., 100.), p(10., 10.)] {
        editor.update(point);
    }
    editor.finish();
    assert!(!editor.objects()[0].closed());
}

#[test]
fn a_pen_loop_already_drawn_stops_the_colour_of_the_capture() {
    let mut editor = Editor::default();
    let id = pen_loop(&mut editor, 0.);
    editor.style.color = BLACK;
    editor.fill_at(p(300., 300.), false);
    let coat = editor.background().last().unwrap();
    assert_eq!(
        coat.walls,
        [id],
        "its outline stops the colour, as a circle's does"
    );
    assert_eq!(
        coat_seen(editor.background(), p(55., 55.), editor.objects(), 0),
        None
    );
    assert!(coat_seen(editor.background(), p(300., 300.), editor.objects(), 0).is_some());
}

#[test]
fn erasing_a_filled_loop_open_takes_its_colour_off() {
    let mut editor = Editor::default();
    let id = pen_loop(&mut editor, 0.);
    editor.style.color = BLACK;
    editor.fill_at(p(55., 55.), false);
    assert_eq!(fill_of(&editor, id), Some(BLACK));
    editor.tool = Tool::Eraser;
    editor.begin(p(55., 10.), Modifiers::default());
    editor.finish();
    assert!(
        editor.objects().iter().all(|o| !o.closed()),
        "the eraser cut the loop open"
    );
    assert!(
        editor.objects().iter().all(|o| o.fill.is_empty()),
        "an open stroke holds no colour"
    );
}

#[test]
fn bucket_on_drawn_ink_recolours_it_and_fills_nothing() {
    let mut editor = Editor::default();
    editor.style.color = RED;
    editor.tool = Tool::Pen;
    editor.begin(p(10., 10.), Modifiers::default());
    for point in [p(100., 10.), p(100., 100.), p(10., 100.), p(10., 10.)] {
        editor.update(point);
    }
    editor.finish();
    let pen = editor.objects()[0].id;
    let line = draw(&mut editor, Tool::Line, p(200., 30.), p(300., 30.));
    let arrow = draw(&mut editor, Tool::Arrow, p(200., 60.), p(300., 60.));
    let marker = draw(&mut editor, Tool::Highlight, p(200., 90.), p(300., 90.));
    editor.add_text(p(200., 200.), "label".into(), TextSpec::default());
    let text = editor.objects().last().unwrap().id;

    editor.style.color = BLACK;
    for (id, on) in [
        (pen, p(100., 50.)),
        (line, p(250., 30.)),
        (arrow, p(250., 60.)),
        (marker, p(250., 90.)),
        (text, p(210., 210.)),
    ] {
        assert_eq!(colour_of(&editor, id), RED);
        editor.fill_at(on, false);
        assert_eq!(colour_of(&editor, id), BLACK, "{on:?}");
        assert_eq!(background(&editor), None, "{on:?}");
        assert!(editor.objects().iter().all(|o| o.fill.is_empty()));
        editor.undo();
        assert_eq!(colour_of(&editor, id), RED, "one click, one undo step");
        editor.redo();
    }
}

#[test]
fn bucket_on_an_outline_recolours_it_and_inside_still_fills() {
    let (mut editor, rectangle) = with_rectangle();
    // Circles grow from their centre: this one spans (400, 100) to (600, 300).
    let circle = draw(&mut editor, Tool::Ellipse, p(500., 200.), p(600., 200.));
    editor.style.color = BLACK;
    for (id, edge, inside) in [
        (rectangle, p(100., 150.), p(200., 150.)),
        (circle, p(400., 200.), p(500., 200.)),
    ] {
        editor.fill_at(edge, false);
        assert_eq!(colour_of(&editor, id), BLACK);
        assert_eq!(fill_of(&editor, id), None, "the outline, not the inside");
        editor.undo();
        editor.fill_at(inside, false);
        assert_eq!(colour_of(&editor, id), RED, "the inside, not the outline");
        assert_eq!(fill_of(&editor, id), Some(BLACK));
    }
    assert_eq!(background(&editor), None);
}

#[test]
fn bucket_reaches_three_pixels_around_the_ink_and_no_further() {
    let mut editor = Editor::default();
    editor.style.color = RED;
    // 3 px wide: its ink ends 1.5 px from the centre line.
    let line = draw(&mut editor, Tool::Line, p(200., 50.), p(300., 50.));
    editor.style.color = BLACK;
    editor.fill_at(p(250., 55.), false);
    assert_eq!(colour_of(&editor, line), RED);
    assert_eq!(background(&editor), Some(BLACK), "5 px away is the capture");
    editor.undo();
    editor.fill_at(p(250., 54.), false);
    assert_eq!(colour_of(&editor, line), BLACK);
    assert_eq!(background(&editor), None);

    // The reach follows the screen scale, like every pick tolerance.
    editor.undo();
    editor.ui_scale = 2.;
    editor.fill_at(p(250., 57.), false);
    assert_eq!(colour_of(&editor, line), BLACK);
}

#[test]
fn bucket_reach_starts_at_the_drawn_edge_of_a_thick_outline() {
    let mut editor = Editor::default();
    editor.style = Style {
        color: RED,
        width: 20.,
    };
    let id = draw(&mut editor, Tool::Rectangle, p(100., 100.), p(300., 200.));
    editor.style.color = BLACK;
    // The outline is drawn 10 px either side of x = 100.
    editor.fill_at(p(86., 150.), false);
    assert_eq!(colour_of(&editor, id), RED);
    assert_eq!(background(&editor), Some(BLACK), "14 px out is the capture");
    editor.undo();
    editor.fill_at(p(88., 150.), false);
    assert_eq!(colour_of(&editor, id), BLACK, "2 px off the drawn edge");
    editor.undo();
    editor.fill_at(p(114., 150.), false);
    assert_eq!(colour_of(&editor, id), RED);
    assert_eq!(fill_of(&editor, id), Some(BLACK), "14 px in is the inside");
}

#[test]
fn bucket_recolours_the_topmost_ink_where_two_strokes_cross() {
    let mut editor = Editor::default();
    editor.style.color = RED;
    let below = draw(&mut editor, Tool::Line, p(100., 150.), p(300., 150.));
    let above = draw(&mut editor, Tool::Line, p(200., 50.), p(200., 250.));
    editor.style.color = BLACK;
    editor.fill_at(p(200., 150.), false);
    assert_eq!(colour_of(&editor, above), BLACK);
    assert_eq!(colour_of(&editor, below), RED);
}

#[test]
fn the_same_colour_on_ink_changes_nothing_and_adds_no_history_step() {
    let mut editor = Editor::default();
    draw(&mut editor, Tool::Line, p(200., 50.), p(300., 50.));
    editor.fill_at(p(250., 50.), false);
    assert_eq!(background(&editor), None);
    editor.undo();
    assert!(editor.objects().is_empty(), "the only step was the line");
}

#[test]
fn ink_seen_through_a_hollow_shape_is_recoloured_and_ink_under_a_fill_is_not() {
    let mut editor = Editor::default();
    editor.style.color = RED;
    let line = draw(&mut editor, Tool::Line, p(120., 150.), p(280., 150.));
    let frame = draw(&mut editor, Tool::Rectangle, p(100., 100.), p(300., 200.));
    editor.style.color = BLACK;
    editor.fill_at(p(200., 150.), false);
    assert_eq!(colour_of(&editor, line), BLACK);
    assert_eq!(fill_of(&editor, frame), None);

    editor.fill_at(p(200., 120.), false);
    assert_eq!(fill_of(&editor, frame), Some(BLACK));
    editor.style.color = RED;
    editor.fill_at(p(200., 150.), false);
    assert_eq!(
        fill_of(&editor, frame),
        Some(RED),
        "the fill hides the line"
    );
    assert_eq!(colour_of(&editor, line), BLACK);
}

#[test]
fn clearing_on_an_outline_removes_the_fill_and_leaves_the_outline_alone() {
    let (mut editor, id) = with_rectangle();
    editor.style.color = BLACK;
    editor.fill_at(p(200., 150.), false);
    editor.fill_at(p(100., 150.), true);
    assert_eq!(fill_of(&editor, id), None);
    assert_eq!(colour_of(&editor, id), RED);
}

#[test]
fn another_colour_recolours_and_the_same_colour_removes() {
    let (mut editor, id) = with_rectangle();
    for at in [p(200., 150.), p(20., 20.)] {
        let read = |e: &Editor| {
            if at.x > 100. {
                fill_of(e, id)
            } else {
                background(e)
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
    assert_eq!(background(&editor), Some(BLACK), "one click, one fill");
    editor.fill_at(p(20., 20.), true);
    assert_eq!(background(&editor), None);
}

#[test]
fn clearing_through_a_hollow_shape_removes_the_background_seen_inside() {
    let mut editor = Editor::default();
    editor.fill_at(p(20., 20.), false);
    // Drawn after the coat: it does not cut it, so the coat shows through.
    let id = draw(&mut editor, Tool::Rectangle, p(100., 100.), p(300., 200.));
    editor.fill_at(p(200., 150.), true);
    assert_eq!(background(&editor), None);
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
    assert_eq!(background(&editor), Some(BLACK));
    editor.undo();
    assert_eq!(background(&editor), None);
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
    assert_eq!(background(&editor), Some(RED));
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
    assert_eq!(top(&editor.objects()[1].fill), Some(BLACK));
}

#[test]
fn deleting_a_filled_shape_removes_its_fill_and_keeps_the_background() {
    let (mut editor, id) = with_rectangle();
    editor.fill_at(p(200., 150.), false);
    editor.fill_at(p(20., 20.), false);
    editor.select_only(id);
    editor.delete_selection();
    assert!(editor.objects().is_empty());
    assert!(background(&editor).is_some());
}

#[test]
fn bucket_ignores_invalid_points_and_an_open_gesture() {
    let mut editor = Editor::default();
    editor.fill_at(p(f64::NAN, 0.), false);
    assert_eq!(background(&editor), None);
    editor.tool = Tool::Rectangle;
    editor.begin(p(10., 10.), Modifiers::default());
    editor.update(p(50., 50.));
    editor.fill_at(p(400., 400.), false);
    assert!(
        !editor.is_drawing(),
        "the unfinished shape is cancelled first"
    );
    assert!(editor.objects().is_empty());
    assert_eq!(background(&editor), Some(editor.style.color));
}

/// A circle spanning (400, 100) to (600, 300), drawn in red.
fn circle(editor: &mut Editor) -> u64 {
    editor.style.color = RED;
    // Circles grow from their centre.
    draw(editor, Tool::Ellipse, p(500., 200.), p(600., 200.))
}

/// The capture's coat seen at `at`, if any.
fn background_at(editor: &Editor, at: Point) -> Option<[u8; 3]> {
    let coats = editor.background();
    coat_seen(coats, at, editor.objects(), 0).map(|coat| coats[coat].color)
}

#[test]
fn bucket_outside_a_hollow_shape_spares_its_inside() {
    let mut editor = Editor::default();
    let id = circle(&mut editor);
    editor.style.color = BLACK;
    editor.fill_at(p(50., 50.), false);
    assert_eq!(background_at(&editor, p(50., 50.)), Some(BLACK));
    assert_eq!(
        background_at(&editor, p(500., 200.)),
        None,
        "the outline stops it"
    );
    assert_eq!(fill_of(&editor, id), None);
    // The inside is the circle's own zone: the bucket fills it there.
    editor.style.color = RED;
    editor.fill_at(p(500., 200.), false);
    assert_eq!(fill_of(&editor, id), Some(RED));
}

#[test]
fn the_spared_inside_follows_its_shape() {
    let mut editor = Editor::default();
    let id = circle(&mut editor);
    editor.fill_at(p(50., 50.), false);
    editor.select_only(id);
    editor.nudge(p(-300., 0.));
    assert_eq!(
        background_at(&editor, p(200., 200.)),
        None,
        "where it is now"
    );
    assert!(
        background_at(&editor, p(500., 200.)).is_some(),
        "where it was"
    );
}

#[test]
fn a_shape_drawn_after_a_coat_does_not_cut_it() {
    let mut editor = Editor::default();
    editor.style.color = BLACK;
    editor.fill_at(p(50., 50.), false);
    circle(&mut editor);
    assert_eq!(background_at(&editor, p(500., 200.)), Some(BLACK));
}

#[test]
fn repainting_keeps_the_old_colour_inside_a_shape_drawn_since() {
    // The slate: a black board, a red circle on it, then the board turned red.
    let mut editor = Editor::default();
    editor.style.color = BLACK;
    editor.fill_at(p(50., 50.), false);
    circle(&mut editor);
    editor.fill_at(p(50., 50.), false);
    assert_eq!(background_at(&editor, p(50., 50.)), Some(RED));
    assert_eq!(background_at(&editor, p(500., 200.)), Some(BLACK));
    // Black again over the whole board: the red coat is hidden, so it goes.
    editor.style.color = BLACK;
    editor.fill_at(p(50., 50.), false);
    assert_eq!(editor.background().len(), 2);
    assert!(editor.background().iter().all(|c| c.color == BLACK));
}

#[test]
fn repainting_the_same_zone_keeps_a_single_coat() {
    let mut editor = Editor::default();
    circle(&mut editor);
    for color in [BLACK, RED, BLACK] {
        editor.style.color = color;
        editor.fill_at(p(50., 50.), false);
    }
    assert_eq!(editor.background().len(), 1);
}

#[test]
fn filling_a_shape_spares_the_shapes_standing_in_it() {
    let (mut editor, frame) = with_rectangle();
    let inner = draw(&mut editor, Tool::Rectangle, p(150., 120.), p(200., 180.));
    let far = circle(&mut editor);
    editor.style.color = BLACK;
    editor.fill_at(p(250., 150.), false);
    let fill = &editor.objects()[0].fill;
    assert_eq!(fill.len(), 1);
    assert_eq!(fill[0].walls, [inner], "the circle never touched it");
    let all = editor.objects();
    assert!(all[0].coat_seen(p(250., 150.), all).is_some());
    assert_eq!(all[0].coat_seen(p(175., 150.), all), None);
    assert_eq!(fill_of(&editor, inner), None);
    assert_eq!(fill_of(&editor, far), None);
    assert_eq!(fill_of(&editor, frame), Some(BLACK));
}

#[test]
fn a_shape_lowered_under_its_zone_is_spared_no_more() {
    let (mut editor, _) = with_rectangle();
    let inner = draw(&mut editor, Tool::Rectangle, p(150., 120.), p(200., 180.));
    editor.fill_at(p(250., 150.), false);
    editor.select_only(inner);
    editor.lower_selection(true);
    let all = editor.objects();
    assert_eq!(all[0].id, inner);
    assert!(
        all[1].coat_seen(p(175., 150.), all).is_some(),
        "the fill now covers it"
    );
}

#[test]
fn deleting_a_spared_shape_lets_the_coat_cover_its_place() {
    let mut editor = Editor::default();
    let id = circle(&mut editor);
    editor.fill_at(p(50., 50.), false);
    editor.select_only(id);
    editor.delete_selection();
    assert!(background_at(&editor, p(500., 200.)).is_some());
    editor.undo();
    assert_eq!(
        background_at(&editor, p(500., 200.)),
        None,
        "it spares again"
    );
}

#[test]
fn clearing_inside_a_spared_shape_takes_nothing() {
    let mut editor = Editor::default();
    circle(&mut editor);
    editor.fill_at(p(50., 50.), false);
    assert_eq!(editor.fill_change(p(500., 200.), true), None);
    editor.fill_at(p(500., 200.), true);
    assert_eq!(editor.background().len(), 1);
}

#[test]
fn clearing_peels_the_coat_seen_and_shows_the_one_below() {
    let mut editor = Editor::default();
    editor.style.color = BLACK;
    editor.fill_at(p(50., 50.), false);
    circle(&mut editor);
    editor.fill_at(p(50., 50.), false);
    editor.fill_at(p(50., 50.), true);
    assert_eq!(background_at(&editor, p(50., 50.)), Some(BLACK));
    editor.fill_at(p(50., 50.), true);
    assert!(editor.background().is_empty());
}

#[test]
fn copies_keep_the_walls_they_carry_and_drop_the_others() {
    let (mut editor, frame) = with_rectangle();
    let inner = draw(&mut editor, Tool::Rectangle, p(150., 120.), p(200., 180.));
    editor.fill_at(p(250., 150.), false);

    editor.selected = vec![frame, inner];
    let both = editor.copy_selected();
    assert!(editor.paste_objects(&both, p(0., 300.)));
    let pasted = &editor.objects()[2..];
    assert_eq!(pasted[0].fill[0].walls, [pasted[1].id]);

    editor.select_only(frame);
    let alone = editor.copy_selected();
    assert!(editor.paste_objects(&alone, p(0., 300.)));
    assert!(editor.objects().last().unwrap().fill[0].walls.is_empty());
}

#[test]
fn renumbering_carries_the_walls_and_drops_the_missing() {
    let mut editor = Editor::default();
    let gone = circle(&mut editor);
    let kept = draw(&mut editor, Tool::Rectangle, p(10., 10.), p(60., 60.));
    editor.fill_at(p(300., 300.), false);
    let mut background = editor.background().to_vec();
    assert_eq!(background[0].walls, [gone, kept]);
    let mut objects = editor.objects()[1..].to_vec();
    renumber(&mut objects, &mut background, 1);
    assert_eq!(objects[0].id, 1);
    assert_eq!(background[0].walls, [1]);
}
