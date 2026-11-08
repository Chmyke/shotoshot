use super::*;
use shotoshot_text::Language;

/// The English hints as they were written before translation, in listing order.
const ENGLISH_HINTS: [&str; 41] = [
    "Select · V · Page Up/Down forward or back",
    "Rectangle · R · Scroll to resize",
    "Ellipse · C · Drawn from its centre, Alt to stretch it · Scroll to resize",
    "Line · L · Scroll to resize",
    "Arrow · A · Scroll to resize",
    "Pen · P · Scroll to resize",
    "Highlighter · H · Scroll to resize",
    "Blur · X",
    "Text box · B · Scroll to resize",
    "Text · T · Scroll to resize",
    "Eraser · E · Scroll to resize",
    "Live capture · F2 · refresh on copy or save",
    "Red · 1",
    "Yellow · 2",
    "Green · 3",
    "Blue · 4",
    "White · 5",
    "Black · 6",
    "Custom color · S",
    "Eyedropper · I",
    "Color a stroke, a shape or the background · G · Right-click or Alt+click clears",
    "Undo · Ctrl+Z",
    "Redo · Ctrl+Y / Ctrl+Shift+Z",
    "Search with Google Lens · Ctrl+G",
    "Copy image · Ctrl+C",
    "Save · Ctrl+S",
    "Close · Esc",
    "Box color",
    "Text color · Right-click for automatic black or white",
    "Clear board · Ctrl+Z restores",
    "More tools",
    "Fewer tools",
    "Nothing to undo · Ctrl+Z",
    "Nothing to redo · Ctrl+Y / Ctrl+Shift+Z",
    "Google search in progress · Ctrl+G",
    "Live capture is for new captures",
    "Live capture on · F2 to freeze",
    "Capture frozen · F2 for live capture",
    "Full screen · F",
    "Back to previous selection · F",
    "Edit dimensions · expressions: 800+20, 1920/2",
];

#[test]
fn english_tooltips_are_unchanged() {
    assert_eq!(all_tooltips_in(Language::En), ENGLISH_HINTS);
    let region = Rect::new(Point { x: 100., y: 100. }, Point { x: 400., y: 300. }).unwrap();
    let screen = Rect::new(Point::default(), Point { x: 1920., y: 1080. }).unwrap();
    for board in [false, true] {
        for button in buttons_in(region, None, screen, 1., board, board, Fold::Full) {
            let hint = tooltip_in(&button, Language::En);
            assert!(ENGLISH_HINTS.contains(&hint.as_str()), "{hint}");
        }
    }
}

#[test]
fn french_tooltip_keeps_the_shortcut() {
    let region = Rect::new(Point { x: 100., y: 100. }, Point { x: 400., y: 300. }).unwrap();
    let all = buttons(region, 1920, 1080);
    let find = |action| all.iter().find(|b| b.action == action).unwrap();
    let rectangle = tooltip_in(find(Action::Tool(Tool::Rectangle)), Language::Fr);
    assert!(rectangle.contains("· R ·"), "{rectangle}");
    assert_eq!(
        tooltip_in(find(Action::Tool(Tool::Pen)), Language::Fr),
        "Stylo · P · Molette pour la taille"
    );
    let redo = tooltip_in(find(Action::Redo), Language::Fr);
    assert!(redo.ends_with("· Ctrl+Y / Ctrl+Maj+Z"), "{redo}");
    let redo = tooltip_in(find(Action::Redo), Language::De);
    assert!(redo.ends_with("· Strg+Y / Strg+Umschalt+Z"), "{redo}");
    assert!(tooltip_in(find(Action::Close), Language::De).ends_with("· Esc"));
}

#[test]
fn box_and_text_colour_sit_just_before_the_swatches_only_when_asked() {
    let region = Rect::new(Point { x: 100., y: 100. }, Point { x: 400., y: 300. }).unwrap();
    let screen = Rect::new(Point::default(), Point { x: 1920., y: 1080. }).unwrap();
    let plain = buttons_in(region, None, screen, 1., false, false, Fold::Full);
    let with = buttons_in(region, None, screen, 1., false, true, Fold::Full);
    assert!(
        !plain
            .iter()
            .any(|b| matches!(b.action, Action::BoxColor | Action::TextColor))
    );
    assert_eq!(with.len(), plain.len() + 2);
    let actions: Vec<_> = with.iter().map(|b| b.action).collect();
    let red = actions
        .iter()
        .position(|a| *a == Action::Color(COLORS[0]))
        .unwrap();
    assert_eq!(actions[red - 2..red], [Action::BoxColor, Action::TextColor]);
    let box_color = &with[red - 2];
    assert_eq!(box_color.panel, 1, "in the action panel");
    assert_eq!(tooltip_in(box_color, Language::En), "Box color");
    assert_eq!(
        tooltip_in(&with[red - 1], Language::En),
        "Text color · Right-click for automatic black or white"
    );
}

#[test]
fn every_hint_key_is_listed_for_measuring() {
    let listed: Vec<Key> = all_hints().map(|(label, _)| label).collect();
    for &key in Key::ALL {
        if key.id().starts_with("toolbar.") || key.id().starts_with("capture.") {
            assert!(listed.contains(&key), "{} is never measured", key.id());
        }
    }
    for language in Language::ALL {
        for hint in all_tooltips_in(language) {
            assert!(!hint.contains(['{', '}']), "{}: {hint}", language.tag());
        }
    }
}

#[test]
fn e_selects_the_eraser_and_its_button_follows_text_in_every_layout() {
    assert_eq!(
        shortcut_action(69, false, false, false),
        Some(Action::Tool(Tool::Eraser))
    );
    assert_eq!(
        shortcut_action(69, true, false, false),
        None,
        "Ctrl+E is nobody's"
    );
    let region = Rect::new(Point { x: 100., y: 100. }, Point { x: 400., y: 300. }).unwrap();
    let screen = Rect::new(Point::default(), Point { x: 1920., y: 1080. }).unwrap();
    for board in [false, true] {
        let buttons = buttons_in(region, None, screen, 1., board, false, Fold::Full);
        let tools: Vec<Tool> = buttons
            .iter()
            .filter_map(|b| match b.action {
                Action::Tool(t) => Some(t),
                _ => None,
            })
            .collect();
        let text = tools.iter().position(|t| *t == Tool::Text).unwrap();
        assert_eq!(tools.get(text + 1), Some(&Tool::Eraser), "board {board}");
        let eraser = buttons
            .iter()
            .find(|b| b.action == Action::Tool(Tool::Eraser))
            .unwrap();
        assert_eq!(
            tooltip_in(eraser, Language::En),
            "Eraser · E · Scroll to resize"
        );
    }
}
#[test]
fn x_selects_the_blur_just_after_the_highlighter_except_on_a_board() {
    assert_eq!(
        shortcut_action(88, false, false, false),
        Some(Action::Tool(Tool::Blur))
    );
    assert_eq!(shortcut_action(88, true, false, false), None, "Ctrl+X");
    let region = Rect::new(Point { x: 100., y: 100. }, Point { x: 400., y: 300. }).unwrap();
    let screen = Rect::new(Point::default(), Point { x: 1920., y: 1080. }).unwrap();
    let tools = |board| {
        buttons_in(region, None, screen, 1., board, false, Fold::Full)
            .into_iter()
            .filter_map(|b| match b.action {
                Action::Tool(t) => Some((t, b)),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    let capture = tools(false);
    let highlight = capture
        .iter()
        .position(|(t, _)| *t == Tool::Highlight)
        .unwrap();
    let (tool, blur) = &capture[highlight + 1];
    assert_eq!(*tool, Tool::Blur);
    assert_eq!(blur.panel, 0, "among the drawing tools");
    assert_eq!(
        tooltip_in(blur, Language::En),
        "Blur · X",
        "its strength follows its zone, not the wheel"
    );
    assert_eq!(tooltip_in(blur, Language::Fr), "Flou · X");
    let board = tools(true);
    assert!(board.iter().all(|(t, _)| *t != Tool::Blur));
    assert_eq!(board.len() + 1, capture.len());
}
#[test]
fn hover_delay_fade_and_timer_stop() {
    let now = std::time::Instant::now();
    let bounds = Rect {
        min: Point { x: 100., y: 100. },
        max: Point { x: 130., y: 130. },
    };
    let mut hint = HoverHint::default();
    hint.update(Some((Action::Save, bounds)), now);
    assert_eq!(hint.next_tick(now), Some(250));
    assert_eq!(
        hint.visible(now + std::time::Duration::from_millis(249)),
        None
    );
    let shown = now + std::time::Duration::from_millis(250);
    assert_eq!(hint.visible(shown), Some((Action::Save, bounds, 1.)));
    assert_eq!(hint.next_tick(shown), None);
    assert!(!hint.update(Some((Action::Save, bounds)), shown));
    hint.update(None, shown);
    let half = shown + std::time::Duration::from_millis(60);
    assert!((hint.visible(half).unwrap().2 - 0.5).abs() < 0.001);
    assert_eq!(hint.next_tick(half), Some(16));
    let end = shown + std::time::Duration::from_millis(120);
    assert_eq!(hint.visible(end), None);
    assert_eq!(hint.next_tick(end), None);
}
#[test]
fn quick_crossing_never_shows_and_changing_button_restarts_delay() {
    let now = std::time::Instant::now();
    let bounds = Rect {
        min: Point::default(),
        max: Point { x: 30., y: 30. },
    };
    let mut hint = HoverHint::default();
    hint.update(Some((Action::Save, bounds)), now);
    hint.update(None, now + std::time::Duration::from_millis(100));
    assert_eq!(
        hint.visible(now + std::time::Duration::from_millis(300)),
        None
    );
    hint.update(
        Some((Action::Save, bounds)),
        now + std::time::Duration::from_millis(400),
    );
    let next = now + std::time::Duration::from_millis(700);
    hint.update(Some((Action::Close, bounds)), next);
    assert_eq!(hint.visible(next).unwrap().0, Action::Save);
    assert_eq!(
        hint.visible(next + std::time::Duration::from_millis(150)),
        None
    );
    assert_eq!(
        hint.visible(next + std::time::Duration::from_millis(250))
            .unwrap()
            .0,
        Action::Close
    );
}
#[test]
fn tooltip_is_below_button_and_flips_at_screen_bottom() {
    let screen = Rect {
        min: Point::default(),
        max: Point { x: 800., y: 600. },
    };
    let size = Point { x: 170., y: 25. };
    let button = Rect {
        min: Point { x: 100., y: 100. },
        max: Point { x: 130., y: 130. },
    };
    let normal = tooltip_bounds(button, screen, size, 1.);
    assert_eq!(normal.min, Point { x: 115., y: 136. });
    let bottom = Rect {
        min: Point { x: 765., y: 565. },
        max: Point { x: 795., y: 595. },
    };
    let flipped = tooltip_bounds(bottom, screen, size, 1.);
    assert!(flipped.max.y < bottom.min.y && flipped.max.x <= 796.);
}
#[test]
fn tooltip_stays_inside_each_offset_monitor_even_at_seams_and_corners() {
    let monitors = [
        Rect {
            min: Point { x: 0., y: 200. },
            max: Point { x: 1920., y: 1280. },
        },
        Rect {
            min: Point { x: 1920., y: 0. },
            max: Point { x: 3840., y: 1080. },
        },
        Rect {
            min: Point { x: 3840., y: 120. },
            max: Point { x: 5760., y: 1200. },
        },
    ];
    for monitor in monitors {
        for corner in monitor.corners() {
            let button = Rect {
                min: corner - Point { x: 15., y: 15. },
                max: corner + Point { x: 15., y: 15. },
            };
            for width in [170., 460., 2400.] {
                let hint = tooltip_bounds(button, monitor, Point { x: width, y: 25. }, 1.);
                assert!(hint.min.x >= monitor.min.x + 4. && hint.max.x <= monitor.max.x - 4.);
                assert!(hint.min.y >= monitor.min.y + 4. && hint.max.y <= monitor.max.y - 4.);
            }
        }
    }
}
#[test]
fn toolbar_shortcuts_dispatch_uniquely_and_preserve_contextual_copy() {
    for (index, &(_, action, key, ctrl, shift)) in SHORTCUTS.iter().enumerate() {
        assert_eq!(shortcut_action(key, ctrl, shift, false), Some(action));
        assert!(
            SHORTCUTS[..index]
                .iter()
                .all(|&(_, _, other, c, s)| (key, ctrl, shift) != (other, c, s))
        );
        assert_eq!(shortcut_action(key, ctrl, shift, true), None);
    }
    assert_eq!(
        shortcut_action(49, false, true, false),
        Some(Action::Color(COLORS[0]))
    );
    assert_eq!(
        shortcut_action(86, false, true, false),
        Some(Action::Tool(Tool::Select))
    );
    assert_eq!(shortcut_action(67, true, false, false), None);
    assert_eq!(shortcut_action(86, true, false, false), None);
    assert_eq!(shortcut_action(90, true, true, false), Some(Action::Redo));
    assert!(SHORTCUTS.iter().all(|s| s.2 != FULL_SCREEN_KEY));
    assert_eq!(
        shortcut_action(FULL_SCREEN_KEY, false, false, false),
        Some(Action::FullScreen)
    );
    assert_eq!(shortcut_action(67, true, true, false), Some(Action::Copy));
}
#[test]
fn circle_is_c_and_the_color_wheel_is_s() {
    let plain = |key| shortcut_action(key, false, false, false);
    assert_eq!(plain(67), Some(Action::Tool(Tool::Ellipse)));
    assert_eq!(plain(83), Some(Action::CustomColor));
    assert_eq!(plain(79), None, "O no longer selects a tool");
    assert_eq!(shortcut_action(83, true, false, false), Some(Action::Save));
    assert_eq!(shortcut_action(83, true, true, false), Some(Action::SaveAs));
}
#[test]
fn fill_is_g_beside_the_eyedropper_and_ctrl_g_still_searches() {
    assert_eq!(shortcut_action(71, false, false, false), Some(Action::Fill));
    assert_eq!(
        shortcut_action(71, true, false, false),
        Some(Action::GoogleSearch)
    );
    let region = Rect::new(Point { x: 100., y: 100. }, Point { x: 600., y: 500. }).unwrap();
    let all = buttons(region, 1920, 1080);
    let row: Vec<_> = all.iter().filter(|b| b.panel == 1).collect();
    let eyedropper = row
        .iter()
        .position(|b| b.action == Action::Eyedropper)
        .unwrap();
    let fill = row[eyedropper + 1];
    assert_eq!(fill.action, Action::Fill);
    assert_eq!(fill.bounds.min.y, row[eyedropper].bounds.min.y);
    assert!(tooltip_in(fill, Language::En).contains("Right-click or Alt+click"));
    assert!(
        all.iter().all(|b| b.panel == 1 || b.action != Action::Fill),
        "not among the drawing tools"
    );
}
#[test]
fn live_shape_shortcuts_are_alt_plus_the_editor_letter() {
    const ALT: u32 = 1;
    let live = shotoshot_core::settings::PresentationSettings::default().shape_shortcuts;
    // `shape_shortcuts` order: arrow, rectangle, ellipse.
    for (shortcut, tool) in live
        .iter()
        .zip([Tool::Arrow, Tool::Rectangle, Tool::Ellipse])
    {
        assert_eq!(shortcut.modifiers, ALT);
        assert_eq!(
            shortcut_action(shortcut.key, false, false, false),
            Some(Action::Tool(tool))
        );
    }
}
#[test]
fn corner_panels_stay_separate_and_reachable() {
    for (w, h) in [(640, 480), (1000, 700), (1920, 1080)] {
        for r in [
            Rect::new(Point { x: 100., y: 100. }, Point { x: 500., y: 400. }).unwrap(),
            Rect::new(
                Point::default(),
                Point {
                    x: w as f64,
                    y: h as f64,
                },
            )
            .unwrap(),
            Rect::new(Point::default(), Point { x: 100., y: 100. }).unwrap(),
            Rect::new(
                Point {
                    x: w as f64 - 30.,
                    y: h as f64 - 30.,
                },
                Point {
                    x: w as f64,
                    y: h as f64,
                },
            )
            .unwrap(),
        ] {
            let buttons = buttons(r, w, h);
            assert_eq!(buttons.len(), 27);
            let panels = panels(&buttons, 1.);
            let screen = Rect {
                min: Point::default(),
                max: Point {
                    x: w as f64,
                    y: h as f64,
                },
            };
            assert!(
                panels.iter().all(|p| screen.contains_rect(*p)),
                "{w}x{h} {r:?} {panels:?}"
            );
            assert!(!overlaps(panels[0], panels[1]), "panels overlap {r:?}");
        }
    }
    let r = Rect::new(Point { x: 100., y: 100. }, Point { x: 600., y: 500. }).unwrap();
    let panels = panels(&buttons(r, 1000, 800), 1.);
    assert!(panels[0].min.x > r.max.x);
    assert!(panels[1].min.y > r.max.y);
    assert_eq!(panels[0].max.y, r.max.y);
    assert_eq!(panels[1].max.x, r.max.x);
}
#[test]
fn the_folded_mini_toolbar_is_one_bar_and_unfolds_to_every_button() {
    let screen = Rect::new(Point::default(), Point { x: 1920., y: 1080. }).unwrap();
    let region = Rect::new(Point { x: 300., y: 200. }, Point { x: 1300., y: 800. }).unwrap();
    let layout = |fold| buttons_in(region, None, screen, 1., false, false, fold);
    let folded = layout(Fold::Folded);
    let actions: Vec<_> = folded.iter().map(|b| b.action).collect();
    assert_eq!(
        actions,
        [
            Action::MoreTools,
            Action::Tool(Tool::Pen),
            Action::Undo,
            Action::Copy,
            Action::Save,
            Action::Close,
        ]
    );
    assert!(folded.iter().all(|b| b.panel == 1), "a single bar");
    assert_eq!(panels(&folded, 1.).len(), 1);
    assert_eq!(folded[0].label, Key::ToolbarMoreTools);
    // Right under the region, against its right edge, as the full action bar.
    let bar = panels(&folded, 1.)[0];
    assert_eq!(bar.max.x, region.max.x);
    assert!(bar.min.y > region.max.y);

    let full = layout(Fold::Full);
    let unfolded = layout(Fold::Unfolded);
    assert_eq!(unfolded.len(), full.len() + 1);
    let more = unfolded
        .iter()
        .find(|b| b.action == Action::MoreTools)
        .unwrap();
    assert_eq!((more.panel, more.label), (1, Key::ToolbarFewerTools));
    let first_action = unfolded.iter().find(|b| b.panel == 1).unwrap();
    assert_eq!(first_action.action, Action::MoreTools, "first in its bar");
    assert!(full.iter().all(|b| b.action != Action::MoreTools));
}
