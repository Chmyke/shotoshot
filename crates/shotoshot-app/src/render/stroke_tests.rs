//! Observe actual geometry construction and pixels, not a mock cache.
use super::*;
use shotoshot_core::{Editor, Style};
use std::cell::Cell;

thread_local! {
    static BUILDS: Cell<usize> = const { Cell::new(0) };
}

pub(super) fn record_build() {
    BUILDS.set(BUILDS.get() + 1);
}

pub(super) fn builds() -> usize {
    BUILDS.get()
}
pub(super) fn reset_builds() {
    BUILDS.set(0);
}

fn p(x: f64, y: f64) -> Point {
    Point { x, y }
}

fn stroke(id: u64, highlight: bool) -> Object {
    Object {
        id,
        rotation: 0.,
        fill: Vec::new(),
        style: Style {
            color: [255, 0, 0],
            width: 3.,
        },
        shape: Shape::Stroke {
            points: vec![p(20., 60.), p(160., 60.), p(80., 20.), p(80., 100.)],
            widths: vec![3., 7., 3.],
            highlight,
        },
    }
}

fn draw(renderer: &Renderer, objects: &[Object]) -> Image {
    renderer
        .export(
            &Image::solid(240, 180, [255; 4]),
            &[],
            objects,
            Rect {
                min: p(0., 0.),
                max: p(240., 180.),
            },
        )
        .unwrap()
}

fn assert_matches_fresh(renderer: &Renderer, objects: &[Object]) -> Image {
    let actual = draw(renderer, objects);
    let expected = draw(&Renderer::software().unwrap(), objects);
    assert_eq!(actual.pixels, expected.pixels, "cached outline is stale");
    actual
}

#[test]
fn unchanged_strokes_reuse_the_final_outline_without_changing_pixels() {
    let _com = crate::ComApartment::new().unwrap();
    let renderer = Renderer::software().unwrap();
    let objects = [stroke(1, false), stroke(2, true)];
    BUILDS.set(0);
    let first = draw(&renderer, &objects);
    assert_eq!(BUILDS.get(), 2);
    let second = draw(&renderer, &objects);
    assert_eq!(first.pixels, second.pixels);
    assert_eq!(BUILDS.get(), 2, "unchanged strokes were widened again");
}

#[test]
fn changed_points_width_color_and_rotation_refresh_the_same_id() {
    let _com = crate::ComApartment::new().unwrap();
    let renderer = Renderer::software().unwrap();
    let mut object = stroke(1, false);
    let initial = draw(&renderer, std::slice::from_ref(&object));
    if let Shape::Stroke { points, .. } = &mut object.shape {
        points.push(p(200., 130.));
    }
    let edited = assert_matches_fresh(&renderer, std::slice::from_ref(&object));
    assert_ne!(initial.pixels, edited.pixels);
    if let Shape::Stroke { widths, .. } = &mut object.shape {
        widths.clear();
    }
    object.style.width = 11.;
    let wider = assert_matches_fresh(&renderer, std::slice::from_ref(&object));
    assert_ne!(edited.pixels, wider.pixels);
    object.style.color = [0, 90, 255];
    let colored = assert_matches_fresh(&renderer, std::slice::from_ref(&object));
    assert_ne!(wider.pixels, colored.pixels);
    object.rotate(p(100., 80.), 0.4);
    let turned = assert_matches_fresh(&renderer, &[object]);
    assert_ne!(colored.pixels, turned.pixels);
}

#[test]
fn removed_strokes_release_their_outline_before_the_id_returns() {
    let _com = crate::ComApartment::new().unwrap();
    let renderer = Renderer::software().unwrap();
    let objects = [stroke(1, false), stroke(2, false)];
    BUILDS.set(0);
    draw(&renderer, &objects);
    draw(&renderer, &objects[1..]);
    assert_eq!(BUILDS.get(), 2, "the surviving stroke must stay cached");
    draw(&renderer, &objects);
    assert_eq!(
        BUILDS.get(),
        3,
        "the removed outline must have been released"
    );
    draw(&renderer, &[]);
    draw(&renderer, &objects);
    assert_eq!(BUILDS.get(), 5, "an empty document releases every outline");
}

#[test]
fn undo_and_redo_restore_cached_stroke_pixels() {
    let _com = crate::ComApartment::new().unwrap();
    let renderer = Renderer::software().unwrap();
    let mut editor = Editor::default();
    assert!(editor.paste_objects(&[stroke(1, false)], p(0., 0.)));
    let initial = draw(&renderer, editor.objects());
    editor.nudge(p(15., 20.));
    let moved = assert_matches_fresh(&renderer, editor.objects());
    assert_ne!(initial.pixels, moved.pixels);
    editor.undo();
    assert_eq!(initial.pixels, draw(&renderer, editor.objects()).pixels);
    editor.redo();
    assert_eq!(moved.pixels, draw(&renderer, editor.objects()).pixels);
}

#[test]
fn cached_highlighter_keeps_one_fill_per_object_at_self_crossings() {
    let _com = crate::ComApartment::new().unwrap();
    let renderer = Renderer::software().unwrap();
    let objects = [stroke(1, true), stroke(2, true)];
    let single = draw(&renderer, &objects[..1]);
    let at_crossing = (60 * 240 + 80) * 4;
    assert!((176..=180).contains(&single.pixels[at_crossing]));
    let overlapping = draw(&renderer, &objects);
    assert!((122..=127).contains(&overlapping.pixels[at_crossing]));
    assert_eq!(overlapping.pixels, draw(&renderer, &objects).pixels);
    assert_matches_fresh(&renderer, &objects);
}
