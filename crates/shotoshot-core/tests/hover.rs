//! What hovering shows must be what the click then does: selection toggles, the hover
//! outline, the marquee's catch and the bucket's preview.
use shotoshot_core::*;

const RED: [u8; 3] = [237, 79, 92];
const BLACK: [u8; 3] = [28, 31, 40];
const SHIFT: Modifiers = Modifiers {
    shift: true,
    alt: false,
    ctrl: false,
};
const CTRL: Modifiers = Modifiers {
    shift: false,
    alt: false,
    ctrl: true,
};

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

fn click(editor: &mut Editor, at: Point, modifiers: Modifiers) {
    editor.tool = Tool::Select;
    editor.begin(at, modifiers);
    editor.finish();
}

/// Two hollow rectangles side by side: (100, 100)–(200, 200) and (300, 100)–(400, 200).
fn two_rectangles() -> (Editor, u64, u64) {
    let mut editor = Editor::default();
    editor.style.color = RED;
    let a = draw(&mut editor, Tool::Rectangle, p(100., 100.), p(200., 200.));
    let b = draw(&mut editor, Tool::Rectangle, p(300., 100.), p(400., 200.));
    editor.tool = Tool::Select;
    (editor, a, b)
}

#[test]
fn shift_and_ctrl_both_add_and_remove_one_object() {
    for toggle in [SHIFT, CTRL] {
        let (mut editor, a, b) = two_rectangles();
        click(&mut editor, p(100., 150.), Modifiers::default());
        click(&mut editor, p(300., 150.), toggle);
        assert_eq!(editor.selected, [a, b], "{toggle:?} adds");
        click(&mut editor, p(100., 150.), toggle);
        assert_eq!(editor.selected, [b], "{toggle:?} removes");
        click(&mut editor, p(250., 300.), toggle);
        assert_eq!(
            editor.selected,
            [b],
            "{toggle:?} in the void keeps the selection"
        );
        click(&mut editor, p(250., 300.), Modifiers::default());
        assert!(editor.selected.is_empty());
    }
}

#[test]
fn ctrl_on_the_end_of_a_lone_selected_arrow_still_attaches_it() {
    let (mut editor, a, _) = two_rectangles();
    let arrow = draw(&mut editor, Tool::Arrow, p(250., 300.), p(250., 400.));
    click(&mut editor, p(250., 350.), Modifiers::default());
    assert_eq!(editor.selected, [arrow]);
    editor.begin(p(250., 400.), CTRL);
    editor.update_with_modifiers(p(150., 200.), CTRL);
    editor.finish();
    assert_eq!(editor.selected, [arrow], "the end was dragged, not toggled");
    let Shape::Arrow { end_attach, .. } = &editor.objects()[2].shape else {
        panic!("still an arrow");
    };
    assert_eq!(end_attach.as_ref().map(|t| t.target), Some(a));
}

#[test]
fn hovering_names_the_object_a_select_press_would_add() {
    let (mut editor, a, b) = two_rectangles();
    assert_eq!(editor.hover_at(p(102., 150.)), Some(a));
    assert_eq!(
        editor.hover_at(p(150., 150.)),
        None,
        "a hollow inside is not grabbed"
    );
    assert_eq!(editor.hover_at(p(250., 300.)), None);
    click(&mut editor, p(100., 150.), Modifiers::default());
    assert_eq!(editor.hover_at(p(102., 150.)), None, "already selected");
    assert_eq!(editor.hover_at(p(300., 150.)), Some(b));
    // A corner of the selection resizes: no promise of a selection there.
    assert_eq!(editor.hover_at(p(100., 200.)), None);
    editor.tool = Tool::Rectangle;
    assert_eq!(editor.hover_at(p(300., 150.)), None, "drawing tools draw");
}

#[test]
fn a_marquee_shows_what_its_release_selects() {
    let (mut editor, a, b) = two_rectangles();
    editor.begin(p(90., 90.), Modifiers::default());
    editor.update(p(250., 250.));
    assert_eq!(editor.marquee_catch(), [a]);
    editor.update(p(450., 250.));
    assert_eq!(editor.marquee_catch(), [a, b]);
    editor.finish();
    assert_eq!(editor.selected, [a, b]);
    assert!(editor.marquee_catch().is_empty(), "no marquee, no catch");
}

#[test]
fn the_bucket_preview_is_exactly_what_the_click_does() {
    // Ink, a hollow inside, the same colour again, the capture, and Alt clearing each.
    let cases = [
        (p(100., 150.), false, BLACK),
        (p(150., 150.), false, BLACK),
        (p(150., 150.), false, BLACK),
        (p(250., 300.), false, BLACK),
        (p(350., 150.), false, RED),
        (p(350., 150.), true, RED),
        (p(250., 300.), true, RED),
    ];
    let (mut editor, ..) = two_rectangles();
    for (at, clear, color) in cases {
        editor.style.color = color;
        let change = editor
            .fill_change(at, clear)
            .expect("the bucket always aims");
        editor.fill_at(at, clear);
        match change {
            FillChange::Ink { index, color } => {
                assert_eq!(editor.objects()[index].style.color, color)
            }
            FillChange::Fill { index, fill } => assert_eq!(editor.objects()[index].fill, fill),
            FillChange::Background(background) => assert_eq!(editor.background(), background),
        }
    }
    assert_eq!(editor.objects()[0].style.color, BLACK);
    assert_eq!(
        editor.objects()[0].fill,
        None,
        "filled then emptied by the same colour"
    );
    assert_eq!(editor.objects()[1].fill, None, "Alt cleared it");
    assert_eq!(editor.background(), None, "Alt cleared it");
}

#[test]
fn alt_held_is_remembered_between_gestures_for_the_preview() {
    let mut editor = Editor::default();
    editor.update_modifiers(Modifiers {
        alt: true,
        ..Modifiers::default()
    });
    assert!(editor.held().alt);
    editor.update_with_modifiers(p(1., 1.), Modifiers::default());
    assert!(!editor.held().alt);
}
