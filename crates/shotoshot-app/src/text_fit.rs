//! Interface texts measured with DirectWrite, as they are drawn, in every language: each one
//! fits its zone, and English keeps the zones it had before translation. No screen is read.
use crate::render::Renderer;
use shotoshot_core::Point;
use shotoshot_text::{Key, Language};
use shotoshot_ui::color_picker;

fn renderer() -> (crate::ComApartment, Renderer) {
    let com = crate::ComApartment::new().unwrap();
    (com, Renderer::software().unwrap())
}

fn contains(zone: Point, text: Point, margins: Point) -> bool {
    zone.x >= text.x + margins.x && zone.y >= text.y + margins.y
}

#[test]
fn selection_hint_fits_both_lines() {
    let (_com, renderer) = renderer();
    for language in Language::ALL {
        let [title, note] = shotoshot_ui::selection_hint_lines_in(language);
        let measure = |text: &str, size: f64| {
            renderer
                .measure_text_in(text, size as f32, language)
                .unwrap()
        };
        let title = measure(&title, shotoshot_ui::SELECTION_HINT_TITLE_SIZE);
        let note = measure(&note, shotoshot_ui::SELECTION_HINT_NOTE_SIZE);
        let zone = shotoshot_ui::selection_hint_size(title, note, 1.);
        let tag = language.tag();
        // 10 px each side; the title from 6 px, the note from 26 px.
        assert!(contains(zone, title, Point { x: 20., y: 6. }), "{tag}");
        assert!(contains(zone, note, Point { x: 20., y: 26. }), "{tag}");
        assert!(6. + title.y <= 26., "{tag}: the title runs into the note");
        if language == Language::En {
            assert_eq!(
                zone,
                Point {
                    x: title.x + 20.,
                    y: 48.
                }
            );
        }
    }
}

#[test]
fn color_picker_labels_grow_from_minimum() {
    let (_com, renderer) = renderer();
    let minimums = [color_picker::TITLE_WIDTH, color_picker::HINT_WIDTH];
    let sizes = [color_picker::TITLE_SIZE, color_picker::HINT_SIZE];
    let heights = [color_picker::TITLE_HEIGHT, color_picker::HINT_HEIGHT];
    for language in Language::ALL {
        let labels = color_picker::labels_in(language);
        for (index, label) in labels.iter().enumerate() {
            let measured = renderer
                .measure_text_in(label, sizes[index] as f32, language)
                .unwrap();
            let width = color_picker::label_width(minimums[index], measured.x, 1.);
            let tag = language.tag();
            assert_eq!(width, minimums[index].max(measured.x), "{tag}: `{label}`");
            assert!(
                width <= color_picker::MAX_LABEL_WIDTH,
                "{tag}: `{label}` is {:.0} px wide, the panel holds {} px: shorten it",
                measured.x,
                color_picker::MAX_LABEL_WIDTH
            );
            assert!(measured.y <= heights[index], "{tag}: `{label}` is too tall");
            if language == Language::En {
                assert_eq!(width, minimums[index], "English keeps its width");
            }
        }
    }
}

#[test]
fn size_entry_error_fits() {
    let (_com, renderer) = renderer();
    for language in Language::ALL {
        for key in [Key::CropInvalid, Key::CropOutside] {
            let message = shotoshot_text::tr_in(language, key);
            let size = shotoshot_ui::ENTRY_ERROR_SIZE as f32;
            let measured = renderer.measure_text_in(message, size, language).unwrap();
            let zone = shotoshot_ui::entry_error_size(measured, 1.);
            let tag = language.tag();
            // 6 px each side, 4 px above and below.
            assert!(
                contains(zone, measured, Point { x: 12., y: 8. }),
                "{tag}: `{message}`"
            );
            assert!(zone.x >= 460. && zone.y >= 48., "{tag}: below the minimum");
            if language == Language::En {
                assert_eq!(zone, Point { x: 460., y: 48. }, "{message}");
            }
        }
    }
}

#[test]
fn magnifier_hint_fits_the_loupe() {
    let (_com, renderer) = renderer();
    let room = shotoshot_ui::MAGNIFIER_HINT_ROOM;
    let too_long: Vec<String> = Language::ALL
        .iter()
        .filter_map(|&language| {
            let hint = shotoshot_ui::magnifier_hint_in(language);
            let size = shotoshot_ui::MAGNIFIER_HINT_SIZE as f32;
            let measured = renderer.measure_text_in(&hint, size, language).unwrap();
            (measured.x > room.x || measured.y > room.y).then(|| {
                let tag = language.tag();
                format!("{tag}: `{hint}` is {:.0}×{:.0} px", measured.x, measured.y)
            })
        })
        .collect();
    assert!(
        too_long.is_empty(),
        "the loupe holds {}×{} px, shorten:\n{}",
        room.x,
        room.y,
        too_long.join("\n")
    );
}
