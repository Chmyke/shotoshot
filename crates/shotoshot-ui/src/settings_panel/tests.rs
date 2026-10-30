use super::*;
use crate::scale_tests::Recorder;
use shotoshot_core::settings::Shortcut;

/// Widths proportional to the text, as a font would give, in logical pixels.
fn measure(text: &str, size: f64, _style: TextStyle) -> Result<f64, ()> {
    Ok(text.chars().count() as f64 * size * 0.56)
}
fn values() -> Values {
    let caps = |keys: &[&str]| Keys {
        caps: keys.iter().map(|k| k.to_string()).collect(),
        in_use: false,
    };
    Values {
        shortcuts: [
            caps(&["Print Screen"]),
            caps(&["Scroll Lock"]),
            caps(&["Alt", "B"]),
            caps(&["Alt", "A"]),
            Keys {
                in_use: true,
                ..caps(&["Alt", "R"])
            },
            caps(&["Alt", "C"]),
            caps(&[]),
            caps(&[]),
            caps(&[]),
        ],
        ..Values::default()
    }
}
fn panel_in(language: Language) -> Panel {
    let mut panel = Panel::new(
        PointerSettings::default(),
        [255, 211, 64],
        false,
        values(),
        language,
    );
    panel.fit(measure).unwrap();
    panel
}
fn panel() -> Panel {
    panel_in(Language::En)
}
fn on(panel: &mut Panel, page: Page) {
    panel.show_page(page);
}
fn zone(panel: &Panel, item: Item) -> Zone {
    panel
        .layout
        .zone(item)
        .unwrap_or_else(|| panic!("{item:?}"))
}
fn segment_point(panel: &Panel, item: Item, index: usize) -> Point {
    let r = zone(panel, item).control;
    let widths = panel.segment_widths(item);
    let x = r.min.x + 2. + widths[..index].iter().sum::<f64>() + widths[index] / 2.;
    Point { x, y: r.center().y }
}

#[test]
fn every_page_fits_every_language_in_one_window_size() {
    for language in Language::ALL {
        let mut panel = panel_in(language);
        let size = panel.layout.size;
        for page in PAGES {
            on(&mut panel, page);
            let cards: Vec<Option<Card>> = if page == Page::Pointer {
                CARDS.map(Some).to_vec()
            } else {
                vec![panel.open]
            };
            for card in cards {
                panel.open = card;
                panel.arrange();
                assert_eq!(panel.layout.size, size, "{language:?} {page:?}: one size");
                assert_eq!(
                    panel.overflows(measure),
                    Ok(Vec::new()),
                    "{language:?} {page:?} {card:?}"
                );
                let zones = &panel.layout.zones;
                for (index, a) in zones.iter().enumerate() {
                    assert!(a.row.contains_rect(a.control) || a.item == Item::Reset);
                    for b in &zones[index + 1..] {
                        let apart =
                            a.row.max.y <= b.row.min.y + 0.01 || b.row.max.y <= a.row.min.y + 0.01;
                        assert!(apart, "{language:?} {page:?} {a:?} {b:?}");
                    }
                }
                if page == Page::Pointer {
                    let preview = panel.layout.preview;
                    assert!(zones.iter().all(|z| z.row.max.x <= preview.min.x));
                }
            }
        }
    }
}

#[test]
fn the_navigation_shows_each_page_and_the_preview_only_moves_on_its_own() {
    let mut panel = panel();
    assert_eq!(panel.page, Page::General);
    assert!(!panel.tick(16), "no preview off the pointer page");
    let (_, r) = panel.layout.nav[3];
    assert_eq!(panel.press(r.center()), Action::Repaint);
    assert_eq!(panel.page, Page::Pointer);
    assert!(panel.tick(16));
    assert!(panel.layout.zone(Item::Card(Card::Halo)).is_some());
    assert!(panel.layout.zone(Item::Startup).is_none());
}

#[test]
fn general_rows_ask_the_platform() {
    let mut panel = panel();
    let startup = zone(&panel, Item::Startup);
    assert_eq!(
        panel.press(startup.control.center()),
        Action::Command(Command::Startup)
    );
    let live = zone(&panel, Item::Live);
    assert_eq!(
        panel.press(live.row.center()),
        Action::Command(Command::Live)
    );
}

#[test]
fn shortcut_rows_edit_and_only_the_pointer_ones_clear() {
    let mut panel = panel();
    on(&mut panel, Page::Shortcuts);
    let capture = zone(&panel, Item::Shortcut(ShortcutRow::Capture));
    assert_eq!(
        panel.press(capture.row.center()),
        Action::Command(Command::EditShortcut(ShortcutRow::Capture))
    );
    let click = zone(&panel, Item::Shortcut(ShortcutRow::ShowClick));
    assert_eq!(
        panel.press(click.control.center()),
        Action::Command(Command::EditShortcut(ShortcutRow::ShowClick)),
        "not set: adds one"
    );
    panel.settings.click_shortcut = Some(Shortcut {
        modifiers: 2,
        key: 0x4b,
    });
    panel.values.shortcuts[6].caps = vec!["Ctrl".into(), "K".into()];
    panel.fit(measure).unwrap();
    on(&mut panel, Page::Shortcuts);
    let click = zone(&panel, Item::Shortcut(ShortcutRow::ShowClick));
    let clear = Point {
        x: click.control.max.x - 4.,
        y: click.control.center().y,
    };
    assert!(matches!(
        panel.press(clear),
        Action::Changed { save: true, .. }
    ));
    assert_eq!(panel.settings.click_shortcut, None);
    assert!(panel.values.shortcuts[6].caps.is_empty());
}

#[test]
fn presentation_segments_and_defaults_are_commands() {
    let mut panel = panel();
    on(&mut panel, Page::Presentation);
    let point = segment_point(&panel, Item::InputScale, 2);
    assert_eq!(
        panel.press(point),
        Action::Command(Command::InputScale(200))
    );
    let point = segment_point(&panel, Item::Duration, 1);
    assert_eq!(panel.press(point), Action::Repaint, "already chosen");
    let point = segment_point(&panel, Item::Duration, 0);
    assert_eq!(panel.press(point), Action::Command(Command::Duration(1000)));
    let reset = zone(&panel, Item::Reset);
    assert_eq!(
        panel.press(reset.row.center()),
        Action::Command(Command::PresentationDefaults)
    );
    // Moving the keyboard is a switch: the window stays open while it is on.
    let r#move = zone(&panel, Item::MoveInput);
    assert_eq!(
        panel.press(r#move.row.center()),
        Action::Command(Command::MoveInput)
    );
    panel.focus = Item::MoveInput;
    assert_eq!(panel.key(KeyInput::Left), Action::None, "already off");
    assert_eq!(
        panel.key(KeyInput::Right),
        Action::Command(Command::MoveInput)
    );
    panel.values.moving_input = true;
    assert_eq!(panel.key(KeyInput::Right), Action::None, "already on");
    assert_eq!(
        panel.key(KeyInput::Left),
        Action::Command(Command::MoveInput)
    );
}

#[test]
fn every_menu_switch_has_its_switch_in_the_settings() {
    let mut panel = panel();
    for (page, shown) in [
        (Page::General, Shown::Board),
        (Page::Presentation, Shown::PointerEffects),
        (Page::Presentation, Shown::Magnifier),
        (Page::Presentation, Shown::Keyboard),
        (Page::Presentation, Shown::Mouse),
        (Page::Presentation, Shown::TemporaryShapes),
    ] {
        on(&mut panel, page);
        let item = Item::Shown(shown);
        assert_eq!(
            panel.press(zone(&panel, item).row.center()),
            Action::Command(Command::Show(shown)),
            "{shown:?}"
        );
        panel.focus = item;
        assert_eq!(
            panel.key(KeyInput::Left),
            Action::None,
            "{shown:?} already off"
        );
        assert_eq!(
            panel.key(KeyInput::Activate),
            Action::Command(Command::Show(shown))
        );
    }
    // The switch reads what is on screen.
    panel.values.keyboard = true;
    panel.loupe_on = true;
    assert!(panel.is_shown(Shown::Keyboard) && panel.is_shown(Shown::Magnifier));
    panel.focus = Item::Shown(Shown::Keyboard);
    assert_eq!(panel.key(KeyInput::Right), Action::None, "already on");
    assert_eq!(
        panel.key(KeyInput::Left),
        Action::Command(Command::Show(Shown::Keyboard))
    );
}

#[test]
fn one_card_opens_at_a_time_and_its_switch_turns_its_effect_off_and_back() {
    let mut panel = panel();
    on(&mut panel, Page::Pointer);
    assert_eq!(panel.open, Some(Card::Halo));
    let trail = zone(&panel, Item::Card(Card::Trail));
    panel.press(Point {
        x: trail.row.min.x + 20.,
        y: trail.row.center().y,
    });
    assert_eq!(panel.open, Some(Card::Trail));
    assert!(panel.layout.zone(Item::Field(Field::HaloSize)).is_none());
    assert!(panel.layout.zone(Item::Field(Field::TrailLength)).is_some());
    // The halo's switch: off keeps nothing of the shape, on brings it back.
    panel.settings.halo_shape = HaloShape::Neon;
    let halo = zone(&panel, Item::Card(Card::Halo));
    let switch = Point {
        x: halo.control.max.x - 8.,
        y: halo.control.center().y,
    };
    assert!(matches!(panel.press(switch), Action::Changed { .. }));
    assert_eq!(panel.settings.halo_shape, HaloShape::Hidden);
    assert_eq!(panel.summary(Card::Halo), "Off");
    panel.press(switch);
    assert_eq!(panel.settings.halo_shape, HaloShape::Neon);
    assert!(panel.halo_on, "switched on: seen");
}

#[test]
fn a_change_in_a_card_turns_its_effect_and_the_pointer_effects_on() {
    let mut panel = panel();
    on(&mut panel, Page::Pointer);
    panel.settings.trail = TrailStyle::Off;
    panel.halo_on = false;
    panel.open = Some(Card::Trail);
    panel.arrange();
    let point = segment_point(&panel, Item::Field(Field::TrailLength), 0);
    assert!(matches!(panel.press(point), Action::Changed { .. }));
    assert_ne!(panel.settings.trail, TrailStyle::Off);
    assert!(panel.halo_on);
    // The loupe's settings turn the loupe on, not the halo.
    panel.halo_on = false;
    panel.open = Some(Card::Loupe);
    panel.arrange();
    let slider = zone(&panel, Item::Field(Field::LoupeZoom)).control;
    panel.press(Point {
        x: slider.max.x,
        y: slider.center().y,
    });
    assert!(panel.loupe_on && !panel.halo_on);
    assert_eq!(panel.settings.loupe_zoom, LOUPE_ZOOM_RANGE.1);
}

#[test]
fn the_loupe_zoom_and_size_are_sliders_shown_in_its_summary() {
    let mut panel = panel();
    on(&mut panel, Page::Pointer);
    panel.open = Some(Card::Loupe);
    panel.loupe_on = true;
    panel.arrange();
    for field in [Field::LoupeZoom, Field::LoupeSize] {
        let slider = zone(&panel, Item::Field(field)).control;
        let y = slider.center().y;
        panel.press(Point { x: slider.min.x, y });
        panel.release(Point { x: slider.min.x, y });
    }
    let low = (panel.settings.loupe_zoom, panel.settings.loupe_size);
    assert_eq!(low, (LOUPE_ZOOM_RANGE.0, LOUPE_SIZE_RANGE.0));
    assert_eq!(panel.summary(Card::Loupe), "2× · 120 px");
    // The keys move them by whole steps: one zoom, ten pixels.
    panel.focus = Item::Field(Field::LoupeZoom);
    panel.key(KeyInput::Right);
    panel.focus = Item::Field(Field::LoupeSize);
    panel.key(KeyInput::Right);
    assert_eq!(panel.summary(Card::Loupe), "3× · 130 px");
}

#[test]
fn dependent_settings_show_only_when_they_act() {
    let mut panel = panel();
    on(&mut panel, Page::Pointer);
    panel.open = Some(Card::Clicks);
    panel.settings.sound = ClickSound::Off;
    panel.arrange();
    assert!(panel.layout.zone(Item::Field(Field::Volume)).is_none());
    let point = segment_point(&panel, Item::Field(Field::Sound), 1);
    assert_eq!(
        panel.press(point),
        Action::Changed {
            save: true,
            sound: true
        },
        "a new sound is heard"
    );
    panel.arrange();
    assert!(panel.layout.zone(Item::Field(Field::Volume)).is_some());
    panel.open = Some(Card::Loupe);
    panel.arrange();
    assert!(panel.layout.zone(Item::Field(Field::LoupeHold)).is_none());
}

#[test]
fn a_slider_follows_the_drag_and_saves_on_release() {
    let mut panel = panel();
    on(&mut panel, Page::Pointer);
    let r = zone(&panel, Item::Field(Field::HaloSize)).control;
    let y = r.center().y;
    assert_eq!(
        panel.press(Point { x: r.min.x + 8., y }),
        Action::Changed {
            save: false,
            sound: false
        }
    );
    assert_eq!(panel.settings.halo_diameter, HALO_DIAMETER_RANGE.0);
    panel.drag(Point {
        x: r.max.x + 50.,
        y,
    });
    assert_eq!(panel.settings.halo_diameter, HALO_DIAMETER_RANGE.1);
    assert_eq!(
        panel.release(Point { x: r.center().x, y }),
        Action::Changed {
            save: true,
            sound: false
        }
    );
    assert_eq!(panel.drag(Point { x: r.min.x, y }), Action::None);
}

#[test]
fn swatches_answer_clicks() {
    let mut panel = panel();
    on(&mut panel, Page::Pointer);
    panel.open = Some(Card::Clicks);
    panel.arrange();
    let r = zone(&panel, Item::Field(Field::LeftColor)).control;
    panel.press(swatch_center(r, 5, 1.));
    assert_eq!(panel.settings.left_click_color, COLORS[5]);
}

#[test]
fn the_keyboard_reaches_every_row_and_changes_it() {
    let mut panel = panel();
    on(&mut panel, Page::Pointer);
    panel.focus = Item::Nav(Page::General);
    let count = panel.order().len();
    let mut seen = Vec::new();
    for _ in 0..count {
        seen.push(panel.focus);
        panel.key(KeyInput::Next);
    }
    assert_eq!(panel.focus, Item::Nav(Page::General), "a full lap");
    assert!(seen.contains(&Item::Field(Field::HaloShape)));
    assert!(seen.contains(&Item::Reset));
    panel.focus = Item::Field(Field::HaloShape);
    panel.key(KeyInput::Right);
    assert_eq!(panel.settings.halo_shape, HaloShape::Ring);
    panel.focus = Item::Nav(Page::Pointer);
    panel.key(KeyInput::Left);
    assert_eq!(panel.page, Page::Presentation, "arrows move between pages");
    assert_eq!(panel.key(KeyInput::Escape), Action::Close);
}

#[test]
fn reset_keeps_the_pointer_shortcuts() {
    let shortcut = Some(Shortcut {
        modifiers: 3,
        key: 0x20,
    });
    let mut panel = Panel::new(
        PointerSettings {
            trail: TrailStyle::Stars,
            spotlight_shortcut: shortcut,
            loupe_shortcut: shortcut,
            ..Default::default()
        },
        [64, 156, 255],
        true,
        values(),
        Language::En,
    );
    panel.fit(measure).unwrap();
    on(&mut panel, Page::Pointer);
    let reset = zone(&panel, Item::Reset);
    assert!(matches!(
        panel.press(reset.row.center()),
        Action::Changed { .. }
    ));
    assert_eq!(panel.settings.trail, PointerSettings::default().trail);
    assert_eq!(panel.settings.spotlight_shortcut, shortcut);
    assert_eq!(panel.settings.loupe_shortcut, shortcut);
    assert_eq!(panel.halo_color, [255, 211, 64]);
}

#[test]
fn every_page_paints_inside_itself_and_the_pointer_one_with_its_preview() {
    let mut panel = Panel::new(
        PointerSettings {
            halo_shape: HaloShape::Neon,
            trail: TrailStyle::Sparks,
            click: ClickStyle::Waves,
            spotlight: SpotlightShape::Horizontal,
            ..Default::default()
        },
        [255, 211, 64],
        true,
        values(),
        Language::En,
    );
    panel.fit(measure).unwrap();
    for page in PAGES {
        on(&mut panel, page);
        for now in (0..3000).step_by(16) {
            panel.tick(now);
        }
        let mut canvas = Recorder {
            size: panel.layout.size * 2.,
            ops: Vec::new(),
        };
        panel.paint(&mut canvas, 2., 3000).unwrap();
        if page == Page::Pointer {
            assert!(
                canvas.ops.iter().any(|op| op.name == "fill_rect_except"),
                "spotlight"
            );
            assert!(
                canvas.ops.iter().filter(|op| op.name == "line").count() > 20,
                "sparks"
            );
        }
    }
}

#[test]
fn rows_and_pages_show_their_own_icon() {
    // Starting with Windows: an arrow going in; the power symbol is Quit's, in the menu.
    assert_eq!(Item::Startup.icon(), Icon::SignIn);
    assert_ne!(Item::Startup.icon(), Icon::Power);
    let pages: Vec<Icon> = PAGES.iter().map(|page| page.icon()).collect();
    let rows: Vec<Icon> = [Item::Startup, Item::Live, Item::InputScale, Item::Duration]
        .into_iter()
        .chain([Item::MoveInput])
        .chain(SHORTCUT_ROWS.map(Item::Shortcut))
        .map(Item::icon)
        .collect();
    for list in [&pages, &rows] {
        for (index, icon) in list.iter().enumerate() {
            assert_ne!(*icon, Icon::None);
            assert!(!list[index + 1..].contains(icon), "{icon:?} twice");
        }
    }
    // The shortcut of an action borrows the icon its menu row shows.
    assert_eq!(ShortcutRow::Board.icon(), Icon::BoardEdit);
    for card in CARDS {
        assert_ne!(card.icon(), Icon::None);
    }
}

#[test]
fn explanations_stand_under_the_labels_that_need_one() {
    let mut panel = Panel::new(
        PointerSettings::default(),
        [255, 211, 64],
        true,
        values(),
        Language::En,
    );
    panel.fit(measure).unwrap();
    panel.show_page(Page::General);
    for item in [Item::Startup, Item::Live] {
        let row = panel.layout.zone(item).unwrap().row;
        assert_eq!(row.size().y, NOTE_ROW, "{item:?}");
    }
    panel.show_page(Page::Shortcuts);
    let row = panel
        .layout
        .zone(Item::Shortcut(ShortcutRow::Capture))
        .unwrap()
        .row;
    assert_eq!(row.size().y, ROW);
}
