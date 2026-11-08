use super::*;

/// Pixels of the logical area `r` whose RGB colour passes `test`.
fn count(image: &Image, r: Rect, scale: i32, test: impl Fn([u8; 3]) -> bool) -> usize {
    let scale = scale as f64;
    let columns = (r.min.x * scale) as usize..(r.max.x * scale) as usize;
    let rows = (r.min.y * scale) as usize..(r.max.y * scale) as usize;
    image
        .pixels
        .as_chunks::<4>()
        .0
        .iter()
        .enumerate()
        .filter(|(index, pixel)| {
            columns.contains(&(index % image.width as usize))
                && rows.contains(&(index / image.width as usize))
                && test([pixel[2], pixel[1], pixel[0]])
        })
        .count()
}
/// Keycaps crossed by a scan line just under their top edge, where no glyph reaches.
fn keycaps(image: &Image, scale: i32, zones: SetupLayout) -> usize {
    let row = ((zones.field.center().y - CAP_HEIGHT / 2. + 3.) * scale as f64) as usize;
    let line = &image.pixels[row * image.width as usize * 4..][..image.width as usize * 4];
    let cap: Vec<bool> = line
        .as_chunks::<4>()
        .0
        .iter()
        .map(|pixel| [pixel[2], pixel[1], pixel[0]] == KEYCAP)
        .collect();
    cap.windows(2).filter(|pair| !pair[0] && pair[1]).count()
}
const ZONES: SetupLayout = SetupLayout::ENGLISH;
const FIELD: Rect = ZONES.field;
const HINT: Rect = ZONES.hint;
const CANCEL: Rect = ZONES.cancel;
const SAVE: Rect = ZONES.save;
fn shown(pending: (u32, u32)) -> Setup {
    Setup {
        pending: Some(pending),
        focus: 2,
        ..Default::default()
    }
}

#[test]
fn a_shortcut_shows_one_keycap_per_key() {
    for (pending, expected) in [((0, 19), 1), ((2, 0x46), 2), ((6, 19), 3), ((15, 0x2C), 5)] {
        let image = render(&mut shown(pending), 1).unwrap();
        assert_eq!(keycaps(&image, 1, ZONES), expected, "{pending:?}");
    }
}

#[test]
fn the_longest_shortcut_stays_inside_its_field() {
    let image = render(&mut shown((15, 0x2C)), 1).unwrap();
    assert_eq!(
        keycaps(&image, 1, ZONES),
        5,
        "every key stays, separators go"
    );
    // On the rows the keycaps occupy, nothing is drawn in the field's margins.
    let (top, bottom) = (
        FIELD.center().y - CAP_HEIGHT / 2.,
        FIELD.center().y + CAP_HEIGHT / 2.,
    );
    for (from, to) in [
        (FIELD.min.x + 2., FIELD.min.x + FIELD_MARGIN),
        (FIELD.max.x - FIELD_MARGIN, FIELD.max.x - 2.),
    ] {
        let margin = area(from, top, to, bottom);
        assert_eq!(count(&image, margin, 1, |rgb| rgb != SURFACE), 0);
    }
}

#[test]
fn opening_from_the_menu_listens_at_once_and_frames_the_field() {
    let mut ui = Setup::change(ShortcutTarget::Instant, Some((0, 145)));
    assert!(ui.recording);
    assert_eq!(
        ui.pending,
        Some((0, 145)),
        "Cancel keeps the current shortcut"
    );
    let accent = |rgb| rgb == ACCENT;
    let listening = render(&mut ui, 1).unwrap();
    assert!(count(&listening, FIELD, 1, accent) > 300);
    assert_eq!(keycaps(&listening, 1, ZONES), 0);
    assert_eq!(
        count(&listening, SAVE, 1, accent),
        0,
        "nothing to save while listening"
    );
    ui.held = 6;
    assert_eq!(
        keycaps(&render(&mut ui, 1).unwrap(), 1, ZONES),
        2,
        "held modifiers show before the key"
    );
    ui.record(6, 19);
    let recorded = render(&mut ui, 1).unwrap();
    assert_eq!(count(&recorded, FIELD, 1, accent), 0);
    assert_eq!(keycaps(&recorded, 1, ZONES), 3);
    assert!(count(&recorded, SAVE, 1, accent) > 1500);
}

#[test]
fn clicks_land_on_what_is_painted() {
    for (zone, action) in [(FIELD, 0), (CANCEL, 1), (SAVE, 2)] {
        let (center, inside) = (zone.center(), zone.expanded(-1.));
        assert_eq!(ZONES.hit(center.x, center.y), Some(action));
        assert_eq!(ZONES.hit(inside.min.x, inside.min.y), Some(action));
        assert_eq!(ZONES.hit(inside.max.x, inside.max.y), Some(action));
    }
    assert_eq!(ZONES.hit(ICON.x, ICON.y), None);
    assert_eq!(ZONES.hit(CANCEL.max.x + 4., CANCEL.center().y), None);
    let image = render(&mut shown((0, 19)), 1).unwrap();
    // Just inside each zone's top edge, above any text: the control covers the window.
    for zone in [FIELD, CANCEL, SAVE] {
        let strip = area(
            zone.min.x + 4.,
            zone.min.y + 2.,
            zone.max.x - 4.,
            zone.min.y + 5.,
        );
        assert_eq!(count(&image, strip, 1, |rgb| rgb == BACKGROUND), 0);
    }
}

#[test]
fn each_shortcut_shows_the_icon_of_its_menu_row() {
    let icon = Rect {
        min: ICON,
        max: ICON + Point { x: 16., y: 16. },
    };
    let drawn = |target| {
        let image = render(
            &mut Setup {
                target,
                ..Default::default()
            },
            1,
        )
        .unwrap();
        count(&image, icon, 1, |rgb| rgb != BACKGROUND)
    };
    let inks = [
        ShortcutTarget::Capture,
        ShortcutTarget::Instant,
        ShortcutTarget::Arrow,
        ShortcutTarget::Rectangle,
        ShortcutTarget::Ellipse,
        ShortcutTarget::Board,
    ]
    .map(drawn);
    assert!(inks.iter().all(|&ink| ink > 20), "{inks:?}");
    assert_ne!(
        inks[3], inks[4],
        "rectangle and ellipse are different glyphs"
    );
}

#[test]
fn the_board_shortcut_asks_for_ctrl_or_alt() {
    let mut ui = Setup::change(ShortcutTarget::Board, Some((1, 0x42)));
    assert!(ui.hint().contains("Ctrl or Alt"), "{}", ui.hint());
    ui.record(0, crate::print_screen::KEY);
    assert!(
        !ui.hint().contains("Snipping Tool"),
        "the board never takes Print Screen from Windows"
    );
}

#[test]
fn an_error_replaces_the_hint_in_red() {
    let red = |rgb: [u8; 3]| rgb[0] > 150 && rgb[0] as i32 - rgb[1] as i32 > 60;
    let mut ui = shown((1, 0x52));
    assert_eq!(count(&render(&mut ui, 1).unwrap(), HINT, 1, red), 0);
    ui.error = print_screen_error(
        0,
        crate::print_screen::KEY,
        &Error::from_hresult(HRESULT::from_win32(ERROR_HOTKEY_ALREADY_REGISTERED.0)),
        Language::En,
    )
    .unwrap();
    let image = render(&mut ui, 1).unwrap();
    assert!(count(&image, HINT, 1, red) > 100);
    // The longest message wraps inside the hint area instead of reaching the buttons.
    let below = area(HINT.min.x, HINT.max.y, HINT.max.x, CANCEL.min.y);
    assert_eq!(count(&image, below, 1, |rgb| rgb != BACKGROUND), 0);
}

#[test]
fn the_window_scales_with_the_monitor() {
    let (normal, doubled) = (
        render(&mut shown((6, 19)), 1).unwrap(),
        render(&mut shown((6, 19)), 2).unwrap(),
    );
    assert_eq!(
        (doubled.width, doubled.height),
        (2 * ZONES.size.x as u32, 2 * ZONES.size.y as u32)
    );
    assert_eq!(keycaps(&doubled, 2, ZONES), 3);
    let accent = |rgb| rgb == ACCENT;
    let ratio = count(&doubled, SAVE, 2, accent) as f64 / count(&normal, SAVE, 1, accent) as f64;
    assert!((3.5..=5.).contains(&ratio), "{ratio}");
}

fn renderer() -> (crate::ComApartment, Renderer) {
    let com = crate::ComApartment::new().unwrap();
    (com, Renderer::software().unwrap())
}

#[test]
fn english_layout_is_unchanged() {
    let english = SetupLayout {
        size: Point { x: 420., y: 248. },
        field: area(20., 58., 400., 122.),
        hint: area(20., 132., 400., 184.),
        cancel: area(216., 196., 304., 228.),
        save: area(312., 196., 400., 228.),
    };
    let (_com, renderer) = renderer();
    assert_eq!(layout(&renderer, Language::En).unwrap(), english);
    assert_eq!(SetupLayout::default(), english);
}

#[test]
fn the_window_grows_only_where_a_text_needs_room() {
    let english = SetupLayout::default();
    let short = Widths {
        title: 100.,
        keys: 200.,
        button: 40.,
    };
    assert_eq!(SetupLayout::fit(short, 30.), english, "short texts keep it");
    // A long title widens the window; the buttons stay on its right edge.
    let wide = SetupLayout::fit(
        Widths {
            title: 500.,
            ..short
        },
        30.,
    );
    assert_eq!(wide.size, Point { x: 566., y: 248. });
    assert_eq!((wide.field.max.x, wide.hint.max.x), (546., 546.));
    assert_eq!(wide.save, area(458., 196., 546., 228.));
    assert_eq!(wide.cancel, area(362., 196., 450., 228.));
    // A long row of keycaps keeps the field's inner margins.
    let keys = SetupLayout::fit(
        Widths {
            keys: 400.,
            ..short
        },
        30.,
    );
    assert_eq!(keys.size.x, 472.);
    // A hint taller than its zone pushes the buttons down, the width stays.
    let tall = SetupLayout::fit(short, 70.2);
    assert_eq!(tall.hint, area(20., 132., 400., 203.));
    assert_eq!(tall.cancel, area(216., 215., 304., 247.));
    assert_eq!(tall.size, Point { x: 420., y: 267. });
    // A long button label widens both buttons alike.
    let long = SetupLayout::fit(
        Widths {
            button: 70.,
            ..short
        },
        30.,
    );
    assert_eq!(long.size, english.size);
    assert_eq!(long.save, area(298., 196., 400., 228.));
    assert_eq!(long.cancel, area(188., 196., 290., 228.));
}

#[test]
fn setup_layout_fits_every_language() {
    let (_com, renderer) = renderer();
    for language in Language::ALL {
        let tag = language.tag();
        let zones = layout(&renderer, language).unwrap();
        let measure = |text: &str, size: f64| {
            renderer
                .measure_text_in(text, size as f32, language)
                .unwrap()
        };
        assert!(
            zones.size.x <= 640. && zones.size.y <= 400.,
            "{tag}: {:?} is larger than 640×400, shorten its texts",
            zones.size
        );
        for target in TARGETS {
            let title = tr_in(language, target.title());
            assert!(
                TITLE_X + measure(title, TITLE_SIZE).x + MARGIN <= zones.size.x,
                "{tag}: `{title}`"
            );
        }
        for (key, button) in [
            (Key::SetupCancel, zones.cancel),
            (Key::SetupSave, zones.save),
        ] {
            let label = measure(tr_in(language, key), TEXT_SIZE);
            assert!(
                button.size().x >= label.x + 2. * BUTTON_PADDING && button.size().y >= label.y,
                "{tag}: `{}`",
                tr_in(language, key)
            );
        }
        assert!(zones.cancel.max.x < zones.save.min.x, "{tag}");
        for hint in hints_in(language) {
            let wrapped = renderer
                .measure_wrapped_text_in(
                    &hint,
                    HINT_SIZE as f32,
                    zones.hint.size().x as f32,
                    language,
                )
                .unwrap();
            assert!(wrapped.y <= zones.hint.size().y, "{tag}: `{hint}`");
            assert!(wrapped.x <= zones.hint.size().x, "{tag}: `{hint}`");
        }
        let room = zones.field.size().x - 2. * FIELD_MARGIN;
        assert!(
            keys_width(&renderer, language).unwrap() <= room,
            "{tag}: the longest shortcut leaves the field"
        );
        if language == Language::En {
            assert_eq!(zones, SetupLayout::default());
        }
    }
}

#[test]
fn key_names_are_translated_but_layout_names_stay() {
    let de = |key| tr_in(Language::De, key);
    assert_eq!(
        key_label_in(6, 0x2C, Language::De),
        format!(
            "{} + {} + {}",
            de(Key::KeyCtrl),
            de(Key::KeyShift),
            de(Key::KeyPrintScreen)
        )
    );
    assert_eq!(key_label_in(2, 0x2C, Language::De), "Strg + Druck");
    assert_eq!(key_label_in(6, 19, Language::En), "Ctrl + Shift + Pause");
    assert_eq!(key_label_in(0, 0x67, Language::En), "Numpad 7");
    assert_eq!(key_label_in(0, 0x6B, Language::En), "Numpad +");
    assert_eq!(key_label_in(8, 0x75, Language::Fr), "Win + F6");
    // A key Windows names follows the keyboard layout, never the interface language.
    for key in [0x41, 0x5A, 0x31, 0xBA, 0xDE] {
        let windows = key_label_in(0, key, Language::En);
        for language in Language::ALL {
            assert_eq!(
                key_label_in(0, key, language),
                windows,
                "{}",
                language.tag()
            );
        }
    }
}

#[test]
fn the_window_speaks_the_language_it_was_fitted_for() {
    let ui = Setup {
        language: Language::De,
        pending: Some((2, 0x2C)),
        ..Default::default()
    };
    assert_eq!(ui.hint(), tr_in(Language::De, Key::SetupChangeHint));
    let shown: Vec<String> = pieces(&ui)
        .into_iter()
        .filter_map(|piece| match piece {
            Piece::Cap(label) => Some(label),
            _ => None,
        })
        .collect();
    assert_eq!(shown, ["Strg", "Druck"]);
}

#[test]
fn a_translated_window_draws_inside_its_zones() {
    let (_com, renderer) = renderer();
    for language in Language::ALL {
        let tag = language.tag();
        let mut ui = Setup {
            language,
            layout: layout(&renderer, language).unwrap(),
            pending: Some((15, 0x2C)),
            focus: 2,
            ..Default::default()
        };
        ui.error = print_screen_error(
            0,
            crate::print_screen::KEY,
            &Error::from_hresult(HRESULT::from_win32(ERROR_HOTKEY_ALREADY_REGISTERED.0)),
            language,
        )
        .unwrap();
        let image = render(&mut ui, 1).unwrap();
        let zones = ui.layout;
        assert_eq!(
            (image.width, image.height),
            (zones.size.x as u32, zones.size.y as u32),
            "{tag}"
        );
        let below = area(
            zones.hint.min.x,
            zones.hint.max.y,
            zones.hint.max.x,
            zones.cancel.min.y,
        );
        assert_eq!(count(&image, below, 1, |rgb| rgb != BACKGROUND), 0, "{tag}");
        assert_eq!(keycaps(&image, 1, zones), 5, "{tag}");
    }
}

#[test]
fn the_hint_area_makes_room_for_every_refusal() {
    use shotoshot_core::settings::{BOARD_SHORTCUT_REFUSALS, SHAPE_SHORTCUT_REFUSALS};
    for language in Language::ALL {
        let hints = hints_in(language);
        // A shape shortcut is refused inside `setup.invalid`, a board shortcut alone.
        for detail in SHAPE_SHORTCUT_REFUSALS
            .into_iter()
            .chain([Key::SetupUsedForCapture, Key::SetupUsedByBoard])
        {
            let composed = Text::with_in(
                language,
                Key::SetupInvalid,
                &[("detail", tr_in(language, detail))],
            )
            .to_string();
            assert!(hints.contains(&composed), "{}: {composed}", language.tag());
        }
        for refusal in BOARD_SHORTCUT_REFUSALS {
            let alone = tr_in(language, refusal).to_string();
            assert!(hints.contains(&alone), "{}: {alone}", language.tag());
        }
    }
}

#[test]
fn a_window_that_cannot_measure_keeps_the_english_layout() {
    let mut ui = Setup::change(ShortcutTarget::Board, None);
    ui.layout.size = Point { x: 1., y: 1. };
    ui.place(Language::De, Err(Error::new(E_FAIL, "no renderer")));
    assert_eq!(ui.layout, SetupLayout::default());
    assert_eq!(ui.language, Language::De);
    assert!(ui.painter.is_none(), "painting creates its own renderer");
}

#[test]
fn only_painting_creates_the_painter() {
    let mut ui = Setup::change(ShortcutTarget::Capture, None);
    assert!(ui.painter.is_none());
    render(&mut ui, 1).unwrap();
    assert!(ui.painter.is_some(), "kept between paints of one opening");
    assert!(Setup::first_run().painter.is_none());
}
