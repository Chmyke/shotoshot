use super::*;

/// The English text of a row, as the existing assertions compare it.
fn english(row: &Row) -> String {
    label_in(row, Language::En).into_owned()
}

const WIDTH: i32 = 320;
fn rendered(row: &Row, state: State, scale: f64) -> Image {
    let _open = Open;
    let (width, height) = (
        (WIDTH as f64 * scale) as i32,
        (height(row) * scale).round() as i32,
    );
    paint_image(width as u32, height as u32, |dc| {
        with_painter(|renderer| {
            draw_row(
                renderer,
                dc,
                RECT {
                    right: width,
                    bottom: height,
                    ..Default::default()
                },
                row,
                state,
                scale,
            )
        })
    })
    .unwrap()
}
/// Pixels of logical columns `from..to` that differ from the menu background.
fn drawn(image: &Image, from: f64, to: f64, scale: f64) -> usize {
    let columns = (from * scale) as usize..(to * scale) as usize;
    image
        .pixels
        .as_chunks::<4>()
        .0
        .iter()
        .enumerate()
        .filter(|(index, pixel)| {
            columns.contains(&(index % image.width as usize))
                && pixel[..3] != [BACKGROUND[2], BACKGROUND[1], BACKGROUND[0]]
        })
        .count()
}
fn accent(image: &Image) -> usize {
    image
        .pixels
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|pixel| pixel[..3] == [ACCENT[2], ACCENT[1], ACCENT[0]])
        .count()
}

#[test]
fn every_command_and_setting_has_an_icon() {
    for row in ROOT {
        if matches!(row.kind, Kind::Title | Kind::Separator) {
            continue;
        }
        let image = rendered(row, State::default(), 1.);
        assert!(
            drawn(&image, ICON_X, ICON_X + 16., 1.) > 20,
            "{} has no icon",
            english(row)
        );
    }
}

#[test]
fn the_board_rows_show_their_state_and_shortcut() {
    let values = Values {
        board: true,
        board_shortcut: 77,
        ..Default::default()
    };
    assert!(values.checked(9));
    assert_eq!(values.data(8), 77);
    assert!(find(8).is_some_and(|r| english(r) == "Edit board"));
    assert!(find(9).is_some_and(|r| english(r) == "Show board" && r.kind == Kind::Toggle));
    assert!(find(10).is_some_and(|r| english(r) == "Clear board"
        && r.kind == Kind::Command
        && matches!(r.icon, Icon::Toolbar(Action::ClearBoard))));
}

#[test]
fn the_shape_rows_teach_their_shortcuts() {
    let values = Values {
        shapes: [11, 22, 33],
        ..Default::default()
    };
    // Right under the switch they need, each with its keys; a click records new ones.
    let at = |id| ROOT.iter().position(|row| row.id == id).unwrap();
    assert_eq!(
        [at(171), at(172), at(173)],
        [at(102) + 1, at(102) + 2, at(102) + 3]
    );
    for (id, keys) in [(171, 11), (172, 22), (173, 33)] {
        assert!(find(id).is_some_and(|row| row.kind == Kind::Command));
        assert_eq!(values.data(id), keys);
    }
}

#[test]
fn a_switch_shows_whether_a_setting_is_on() {
    let row = find(101).unwrap();
    let off = rendered(row, State::default(), 1.);
    let on = rendered(
        row,
        State {
            checked: true,
            ..Default::default()
        },
        1.,
    );
    let switch = (WIDTH as f64 - MARGIN - SWITCH.x, WIDTH as f64 - MARGIN);
    assert!(
        drawn(&off, switch.0, switch.1, 1.) > 40,
        "an off switch stays visible"
    );
    assert_eq!(accent(&off), 0);
    assert!(accent(&on) > 150);
}

#[test]
fn the_menu_acts_and_settings_open_elsewhere() {
    let actions = ROOT
        .iter()
        .filter(|row| !matches!(row.kind, Kind::Title | Kind::Separator))
        .count();
    assert_eq!(actions, 15, "a short menu");
    assert!(find(170).is_some_and(|r| english(r) == "Settings" && r.kind == Kind::Command));
    // No language here: it is chosen in the Settings window (Mickael, 2026-11-12).
    assert!(
        ROOT.iter()
            .all(|row| row.id != 150 && row.icon != Icon::Globe)
    );
    // Each board row has its own icon.
    assert!(matches!(find(8).map(|r| r.icon), Some(Icon::BoardEdit)));
    assert!(matches!(find(9).map(|r| r.icon), Some(Icon::Eye)));
}

#[test]
fn each_command_shows_its_own_icon() {
    // Mickaël, 2026-10-30: the power symbol quits, a bin clears the board, a pencil edits
    // it and the eye of Show board is struck through while the board is hidden.
    let icon = |id: u32| find(id).map(|row| row.icon);
    assert_eq!(icon(3), Some(Icon::Power));
    assert_eq!(icon(10), Some(Icon::Toolbar(Action::ClearBoard)));
    assert_eq!(icon(8), Some(Icon::BoardEdit));
    let show = find(9).unwrap();
    let shown = |checked| {
        shown_icon(
            show,
            State {
                checked,
                ..Default::default()
            },
        )
    };
    assert_eq!(shown(true), Icon::Eye);
    assert_eq!(shown(false), Icon::EyeOff);
    // No two rows of the menu share a glyph.
    let icons: Vec<Icon> = ROOT
        .iter()
        .filter(|row| !matches!(row.kind, Kind::Title | Kind::Separator))
        .map(|row| row.icon)
        .collect();
    for (index, a) in icons.iter().enumerate() {
        assert!(*a != Icon::None, "a row without an icon");
        assert!(!icons[index + 1..].contains(a), "{a:?} is used by two rows");
    }
    // The board bin is not the eraser of one object.
    assert_ne!(icon(10), Some(Icon::Toolbar(Action::Tool(Tool::Eraser))));
}

#[test]
fn rows_scale_with_the_monitor() {
    let row = find(101).unwrap();
    let state = State {
        checked: true,
        ..Default::default()
    };
    let (normal, doubled) = (rendered(row, state, 1.), rendered(row, state, 2.));
    assert_eq!(
        (doubled.width, doubled.height),
        (2 * normal.width, 2 * height(row) as u32)
    );
    // Same layout at twice the size: four times the ink, give or take antialiasing.
    let ratio = accent(&doubled) as f64 / accent(&normal) as f64;
    assert!((3.5..=5.).contains(&ratio), "{ratio}");
    assert_eq!(drawn(&doubled, 0., ICON_X - 1., 2.), 0);
    assert!(drawn(&doubled, ICON_X, ICON_X + 16., 2.) > 80);
}

#[test]
fn width_leaves_room_for_the_shortcut() {
    let _open = Open;
    // The capture row: a shortcut row without a note, whose keys alone set the width.
    let row = find(1).unwrap();
    let width_of = |data| with_painter(|renderer| width(renderer, row, data)).unwrap();
    let short = shortcut(
        Shortcut {
            modifiers: 0,
            key: 145,
        },
        false,
    );
    let long = shortcut(
        Shortcut {
            modifiers: 7,
            key: 44,
        },
        true,
    );
    assert_eq!(
        caps_in(short, Language::En),
        (vec!["Scroll Lock".into()], false)
    );
    assert_eq!(
        caps_in(long, Language::En),
        (
            ["Ctrl", "Shift", "Alt", "Print Screen"]
                .map(String::from)
                .to_vec(),
            true
        )
    );
    // A shortcut row keeps room for itself listening, whatever its keys.
    assert!(width_of(0) <= width_of(short) && width_of(short) < width_of(long));
    // The longest shortcut is drawn entirely inside the width the row asks for.
    let needed = width_of(long);
    let image = paint_image(needed.ceil() as u32 + 40, 28, |dc| {
        with_painter(|renderer| {
            draw_row(
                renderer,
                dc,
                RECT {
                    right: needed.ceil() as i32,
                    bottom: 28,
                    ..Default::default()
                },
                row,
                State {
                    data: long,
                    ..Default::default()
                },
                1.,
            )
        })
    })
    .unwrap();
    let label_end = LABEL_X
        + with_painter(|renderer| {
            renderer.measure_styled_text(&label(row), TEXT_SIZE as f32, TextStyle::Body)
        })
        .unwrap()
        .x;
    assert_eq!(drawn(&image, label_end + 2., label_end + MARGIN, 1.), 0);
    assert_eq!(drawn(&image, needed - MARGIN + 2., needed, 1.), 0);
}

#[test]
fn english_labels_are_unchanged() {
    let root: Vec<_> = ROOT.iter().map(english).collect();
    assert_eq!(
        root,
        [
            "Take a screenshot",
            "Instant screenshot",
            "Board",
            "Edit board",
            "Show board",
            "Clear board",
            "Presentation",
            "Pointer effects",
            "Magnifier",
            "Keyboard",
            "Mouse",
            "Temporary shapes",
            "Arrow",
            "Rectangle",
            "Ellipse",
            "",
            "Settings",
            "Quit Shotoshot",
        ]
    );
}

/// Widest row of a menu column in `language`, in logical pixels at scale 1.
fn widest(rows: &[Row], values: &Values, language: Language) -> f64 {
    with_painter(|renderer| {
        let mut widest: f64 = 0.;
        for row in rows {
            widest = widest.max(width_in(renderer, row, values.data(row.id), language)?);
        }
        Ok(widest)
    })
    .unwrap()
}

#[test]
fn a_listening_row_fits_the_width_it_asked_for() {
    let _open = Open;
    let values = sample();
    for language in Language::ALL {
        for row in ROOT.iter().filter(|row| shortcut_row(row.id).is_some()) {
            let (needed, label_width, placeholder) = with_painter(|renderer| {
                let measure = |text: &str| {
                    Ok::<_, Error>(
                        renderer
                            .measure_styled_text_in(
                                text,
                                settings_panel::SMALL_SIZE as f32,
                                TextStyle::Body,
                                language,
                            )?
                            .x,
                    )
                };
                Ok((
                    width_in(renderer, row, values.data(row.id), language)?,
                    renderer
                        .measure_styled_text_in(
                            &label_in(row, language),
                            TEXT_SIZE as f32,
                            TextStyle::Body,
                            language,
                        )?
                        .x,
                    measure(tr_in(language, Key::MenuPressKeys))?,
                ))
            })
            .unwrap();
            let size = Point {
                x: needed,
                y: height(row),
            };
            let [waiting, default, cancel] = edit_parts(size, slot_width(placeholder));
            let tag = language.tag();
            assert!(
                waiting.min.x - EDIT_SPACE >= LABEL_X + label_width - 0.01,
                "{tag}: the waiting box covers the label of row {}",
                row.id
            );
            assert!(waiting.max.x <= default.min.x && default.max.x <= cancel.min.x);
            assert!(cancel.max.x <= needed - MARGIN + 0.01);
            for part in [waiting, default, cancel] {
                assert!(part.min.y >= 0. && part.max.y <= size.y, "{tag}");
            }
        }
    }
}

#[test]
fn the_label_and_the_keys_light_up_apart() {
    let row = find(1).unwrap();
    let data = sample().capture;
    let caps = with_painter(|renderer| caps_width_in(renderer, data, language())).unwrap();
    let split = keys_left(WIDTH as f64, caps);
    let hover = |image: &Image, from: f64, to: f64| {
        let columns = from as usize..to as usize;
        image
            .pixels
            .as_chunks::<4>()
            .0
            .iter()
            .enumerate()
            .filter(|(index, pixel)| {
                columns.contains(&(index % image.width as usize))
                    && pixel[..3] == [HOVER[2], HOVER[1], HOVER[0]]
            })
            .count()
    };
    let lit = |zone| {
        rendered(
            row,
            State {
                selected: true,
                data,
                zone,
                ..Default::default()
            },
            1.,
        )
    };
    let label = lit(Zone::Label);
    assert!(hover(&label, 0., split) > 0);
    assert_eq!(hover(&label, split + 1., WIDTH as f64), 0);
    assert_eq!(accent(&label), 0);
    let keys = lit(Zone::Keys);
    assert_eq!(hover(&keys, 0., WIDTH as f64), 0, "the label stays neutral");
    assert!(accent(&keys) > 40, "the keys are framed in the accent");
    assert_eq!(drawn(&keys, LABEL_X + 120., split, 1.), 0);
    // The keyboard lights the whole row.
    let whole = lit(Zone::Row);
    assert!(hover(&whole, split + 1., WIDTH as f64 - 5.) > 0);
}

#[test]
fn a_listening_row_paints_inside_itself() {
    let row = find(1).unwrap();
    let edit = RowEdit {
        row: ShortcutRow::Capture,
        held: vec!["Ctrl".into()],
        error: String::new(),
    };
    let state = State {
        look: Look::Editing(&edit),
        ..Default::default()
    };
    let image = rendered(row, state, 1.);
    assert!(accent(&image) > 40, "the row is outlined in the accent");
    // Right of the cancel button, inside the outline: the tint alone.
    let cancel = edit_parts(
        Point {
            x: WIDTH as f64,
            y: height(row),
        },
        EDIT_SLOT,
    )[2];
    let tint = [
        settings_panel::EDITING[2],
        settings_panel::EDITING[1],
        settings_panel::EDITING[0],
    ];
    let stray = image
        .pixels
        .as_chunks::<4>()
        .0
        .iter()
        .enumerate()
        .filter(|(index, pixel)| {
            let (x, y) = (index % image.width as usize, index / image.width as usize);
            (cancel.max.x as usize + 1..WIDTH as usize - 7).contains(&x)
                && (4..28).contains(&y)
                && pixel[..3] != tint
        })
        .count();
    assert_eq!(stray, 0);
    // A refusal shows in red in place of the label.
    let refused = RowEdit {
        error: "Refused".into(),
        ..edit
    };
    let image = rendered(
        row,
        State {
            look: Look::Editing(&refused),
            ..Default::default()
        },
        1.,
    );
    assert_eq!(accent(&image), 0, "the outline turns red");
}

#[test]
fn every_menu_fits() {
    let _open = Open;
    let values = sample();
    for language in Language::ALL {
        let width = widest(ROOT, &values, language);
        assert!(
            width <= 480.,
            "{}: the menu is {width:.0} px wide; shorten its longest text",
            language.tag()
        );
    }
}

#[test]
fn a_closed_menu_keeps_no_renderer() {
    render(&sample(), 170, &popup::View::default(), 1.).unwrap();
    assert!(PAINTER.with(|painter| painter.borrow().is_none()));
}
