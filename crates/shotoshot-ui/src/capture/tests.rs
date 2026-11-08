use super::*;
use shotoshot_core::TextSpec;
fn capture() -> Capture {
    let mut c = Capture::new(Image::solid(900, 600, [0, 0, 0, 255]));
    c.region = Rect::new(Point { x: 100., y: 100. }, Point { x: 600., y: 400. });
    c.selecting = false;
    c
}
#[test]
fn the_pointer_is_an_arrow_everywhere_while_the_wheel_is_open() {
    let mut c = Capture::new(Image::solid(900, 600, [0, 0, 0, 255]));
    c.region = Rect::new(Point { x: 100., y: 100. }, Point { x: 600., y: 400. });
    c.selecting = false;
    c.pointer = Point { x: 300., y: 300. };
    let mut host = crate::test_host::FakeHost::default();
    c.action(&mut host, Action::CustomColor);
    let inside = c.picker.as_ref().unwrap().bounds().center();
    let outside = Point { x: 50., y: 50. };
    assert_eq!(c.cursor(inside), CursorKind::Arrow);
    assert_eq!(c.cursor(outside), CursorKind::Arrow);
    c.picker = None;
    c.eyedropper = true;
    assert_eq!(c.cursor(outside), CursorKind::Cross);
}
#[test]
fn tool_glyphs_take_the_colour_under_the_hovering_eyedropper() {
    let mut c = capture();
    let at = Point { x: 300., y: 250. };
    let offset = (250 * 900 + 300) * 4;
    c.image.pixels[offset..offset + 4].copy_from_slice(&[30, 140, 255, 255]);
    c.pointer = at;
    let red = [237, 79, 92];
    assert_eq!(c.tool_tint(red), red, "no eyedropper: the drawing colour");
    c.eyedropper = true;
    assert_eq!(
        c.tool_tint(red),
        [255, 140, 30],
        "the pixel under the eyedropper"
    );
    c.pointer = c.buttons()[0].bounds.center();
    assert_eq!(
        c.tool_tint(red),
        red,
        "over the toolbar a click picks a button"
    );
}
#[test]
fn region_handles_drive_resize_cursors() {
    let c = capture();
    assert_eq!(c.cursor(Point { x: 100., y: 100. }), CursorKind::ResizeNwse);
    assert_eq!(c.cursor(Point { x: 600., y: 250. }), CursorKind::ResizeWe);
    assert_eq!(c.cursor(Point { x: 350., y: 400. }), CursorKind::ResizeNs);
    assert_eq!(c.cursor(Point { x: 300., y: 250. }), CursorKind::Cross);
}
#[test]
fn the_middle_of_the_arrow_just_drawn_shows_it_can_be_dragged() {
    let mut c = capture();
    c.editor.tool = Tool::Arrow;
    c.editor.begin(
        Point { x: 200., y: 200. },
        shotoshot_core::Modifiers::default(),
    );
    c.editor.update(Point { x: 400., y: 200. });
    c.editor.finish();
    assert_eq!(c.cursor(Point { x: 300., y: 200. }), CursorKind::Move);
    assert_eq!(c.cursor(Point { x: 300., y: 230. }), CursorKind::Cross);
}
#[test]
fn the_text_tool_shows_whether_a_click_edits_or_creates() {
    let mut c = capture();
    c.editor.add_text(
        Point { x: 200., y: 200. },
        "note".into(),
        TextSpec::measured(Point { x: 60., y: 30. }),
    );
    c.editor.tool = Tool::Text;
    let on_text = Point { x: 230., y: 215. };
    assert!(c.text_at(on_text).is_some());
    assert_eq!(c.cursor(on_text), CursorKind::IBeam, "edits");
    assert_eq!(
        c.cursor(Point { x: 400., y: 300. }),
        CursorKind::NewText,
        "creates"
    );
}
#[test]
fn toolbar_hides_size_ring_and_shows_arrow() {
    let mut c = capture();
    c.width_preview = true;
    c.pointer = Point { x: 300., y: 250. };
    assert!(c.width_ring_visible());
    let button = c.buttons()[0].bounds.center();
    c.pointer = button;
    assert!(!c.width_ring_visible());
    assert_eq!(c.cursor(button), CursorKind::Arrow);
}
fn three_screens() -> Capture {
    let mut c = capture();
    c.image = Image::solid(5760, 1080, [0, 0, 0, 255]);
    c.monitors = (0..3)
        .map(|i| {
            let x = i as f64 * 1920.;
            Rect::new(
                Point { x, y: 0. },
                Point {
                    x: x + 1920.,
                    y: 1080.,
                },
            )
            .unwrap()
            .into()
        })
        .collect();
    c
}
fn point(x: f64, y: f64) -> Point {
    Point { x, y }
}
/// A 250 % laptop panel beside a 100 % television.
fn laptop_and_television() -> Capture {
    let mut c = capture();
    c.image = Image::solid(5760, 2160, [0, 0, 0, 255]);
    c.monitors = vec![
        Monitor {
            bounds: Rect::new(point(0., 0.), point(3840., 2160.)).unwrap(),
            scale: 2.5,
        },
        Monitor {
            bounds: Rect::new(point(3840., 0.), point(5760., 1080.)).unwrap(),
            scale: 1.,
        },
    ];
    c
}
#[test]
fn interface_scale_is_that_of_the_screen_under_the_region_then_the_pointer() {
    assert_eq!(capture().ui_scale(), 1., "no monitor list: 100 %");
    let mut c = laptop_and_television();
    assert_eq!(c.ui_scale(), 2.5);
    c.region = Rect::new(point(4000., 100.), point(4600., 500.));
    assert_eq!(c.ui_scale(), 1.);
    // A region across both screens follows the one holding its centre.
    c.region = Rect::new(point(3000., 100.), point(4000., 500.));
    assert_eq!(c.ui_scale(), 2.5);
    c.region = None;
    c.pointer = point(5000., 500.);
    assert_eq!(c.ui_scale(), 1.);
    c.pointer = point(100., 100.);
    assert_eq!(c.ui_scale(), 2.5);
    for (reported, used) in [(0., 1.), (f64::NAN, 1.), (0.5, 1.), (9., 5.)] {
        c.monitors[0].scale = reported;
        assert_eq!(c.ui_scale(), used, "a misreported {reported} stays usable");
    }
}
#[test]
fn buttons_badges_and_handles_keep_their_apparent_size_at_every_scale() {
    let near = |a: f64, b: f64| (a - b).abs() < 1e-9;
    for scale in [1., 1.25, 1.5, 1.75, 2., 2.5, 3.] {
        let mut c = capture();
        c.image = Image::solid(3840, 2160, [0, 0, 0, 255]);
        let screen = Rect::new(point(0., 0.), point(3840., 2160.)).unwrap();
        c.monitors = vec![Monitor {
            bounds: screen,
            scale,
        }];
        c.region = Rect::new(point(800., 600.), point(2400., 1500.));
        let region = c.region.unwrap();
        let buttons = c.buttons();
        assert_eq!(buttons.len(), 27);
        for b in &buttons {
            let size = b.bounds.size();
            assert!(
                near(size.x, 30. * scale) && near(size.y, 30. * scale),
                "a button is 30 logical pixels at {scale}, not {size:?}"
            );
        }
        for panel in toolbar::panels(&buttons, scale) {
            assert!(screen.contains_rect(panel), "panel on screen at {scale}");
        }
        let badge = c.dimensions_bounds().unwrap().size();
        assert!(near(badge.x, 108. * scale) && near(badge.y, 26. * scale));
        let button = c.screen_button_bounds().unwrap().size();
        assert!(near(button.x, 26. * scale) && near(button.y, 26. * scale));
        let inside = region.min + point(7.9 * scale, 0.);
        let outside = region.min + point(8.1 * scale, 0.);
        assert_eq!(
            c.region_handle(inside),
            Some(0),
            "handle grabbed at {scale}"
        );
        assert_eq!(c.region_handle(outside), None);
        let loupe = c.magnifier_bounds().size();
        assert!(near(loupe.x, 126. * scale) && near(loupe.y, 164. * scale));
    }
}
#[test]
fn screen_button_fills_the_region_monitor_and_restores_on_second_press() {
    let mut c = three_screens();
    let small = Rect::new(Point { x: 2000., y: 300. }, Point { x: 2400., y: 500. });
    c.region = small;
    c.toggle_screen();
    assert_eq!(c.region, c.monitors.get(1).map(|m| m.bounds));
    assert!(c.screen_filled());
    c.toggle_screen();
    assert_eq!(c.region, small);
    assert!(!c.screen_filled());
}
#[test]
fn screen_button_picks_its_own_monitor_even_when_most_of_region_is_elsewhere() {
    let mut c = three_screens();
    c.region = Rect::new(Point { x: 200., y: 100. }, Point { x: 2000., y: 900. });
    assert!(
        c.monitors[1]
            .bounds
            .contains(c.screen_button_bounds().unwrap().center())
    );
    c.toggle_screen();
    assert_eq!(c.region, c.monitors.get(1).map(|m| m.bounds));
}
#[test]
fn toolbar_stays_on_a_single_selected_monitor_but_not_for_a_cross_monitor_region() {
    let mut c = three_screens();
    c.region = c.monitors.get(1).map(|m| m.bounds);
    assert!(
        toolbar::panels(&c.buttons(), 1.)
            .iter()
            .all(|panel| c.monitors[1].bounds.contains_rect(*panel))
    );

    c.region = Rect::new(Point { x: 1800., y: 300. }, Point { x: 2200., y: 700. });
    assert_eq!(
        toolbar::panels(&c.buttons(), 1.),
        toolbar::panels(
            &toolbar::buttons(c.region.unwrap(), c.image.width, c.image.height),
            1.
        )
    );
}
#[test]
fn a_toolbar_following_the_pointer_stays_on_the_pointer_screen_of_a_frame_wide_region() {
    let mut c = three_screens();
    c.region = Rect::new(Point::default(), Point { x: 5760., y: 1080. });
    c.board = true;
    for (index, x) in [(0, 300.), (2, 4000.), (1, 2500.)] {
        c.pointer = Point { x, y: 500. };
        let screen = c.monitors[index].bounds;
        let panels = toolbar::panels(&c.buttons(), 1.);
        assert!(!panels.is_empty());
        assert!(
            panels.iter().all(|panel| screen.contains_rect(*panel)),
            "pointer on screen {index}: {panels:?} in {screen:?}"
        );
    }
    // Without the flag, a frame-wide region keeps its placement beside the region.
    c.board = false;
    assert_eq!(
        toolbar::panels(&c.buttons(), 1.),
        toolbar::panels(
            &toolbar::buttons(c.region.unwrap(), c.image.width, c.image.height),
            1.
        )
    );
}
#[test]
fn only_a_board_offers_clear_board_and_puts_it_before_close() {
    let mut c = three_screens();
    c.region = Rect::new(Point::default(), Point { x: 5760., y: 1080. });
    assert!(
        !c.buttons().iter().any(|b| b.action == Action::ClearBoard),
        "a capture has no board to clear"
    );
    c.board = true;
    let buttons = c.buttons();
    let clear = buttons
        .iter()
        .position(|b| b.action == Action::ClearBoard)
        .expect("the board's clear button");
    let close = buttons
        .iter()
        .position(|b| b.action == Action::Close)
        .unwrap();
    assert_eq!(clear + 1, close, "clear sits right before close");
    assert_eq!(
        buttons[clear].panel, buttons[close].panel,
        "in the action panel"
    );
    assert!(buttons[clear].enabled);
}
#[test]
fn monitor_selection_follows_points_including_shared_edges_and_frame_clipping() {
    let mut c = three_screens();
    for (index, x) in [0., 1920., 3840.].into_iter().enumerate() {
        assert_eq!(c.screen_at(Point { x, y: 100. }), c.monitors[index].bounds);
        c.region = Rect::new(
            Point { x: 10., y: 100. },
            Point {
                x: x + 500.,
                y: 700.,
            },
        );
        c.toggle_screen();
        assert_eq!(c.region, Some(c.monitors[index].bounds));
    }
    c.monitors = vec![
        Rect {
            min: Point { x: -100., y: -100. },
            max: Point { x: 500., y: 500. },
        }
        .into(),
    ];
    assert_eq!(
        c.screen_at(Point { x: 900., y: 900. }),
        Rect {
            min: Point::default(),
            max: Point { x: 500., y: 500. }
        }
    );
}
#[test]
fn dimensions_badge_uses_the_delayed_hover_system() {
    let mut c = capture();
    let bounds = c.dimensions_bounds().unwrap();
    let target = c.hover_target(bounds.center());
    assert!(target.is_some());
    let now = std::time::Instant::now();
    c.hover_hint.update(target, now);
    assert!(c.hover_hint.visible(now).is_none());
    assert!(
        c.hover_hint
            .visible(now + std::time::Duration::from_millis(250))
            .is_some()
    );
    c.selecting = true;
    assert!(c.hover_target(bounds.center()).is_none());
}
#[test]
fn dimensions_badge_is_exactly_as_wide_as_its_label() {
    let mut c = capture();
    // 7 px a character, measured at the label size.
    let mut sizes = Vec::new();
    let mut badge = |c: &mut Capture| {
        c.measure_badge(None, |text, size| {
            sizes.push(size);
            Ok::<_, ()>(text.chars().count() as f64 * 7.)
        })
        .unwrap();
        c.dimensions_bounds().unwrap()
    };
    let wide = badge(&mut c);
    assert_eq!(c.dimensions_label().unwrap(), "500 × 300");
    assert_eq!(wide.size().x, 8. + 9. * 7. + 8.);
    let button = c.screen_button_bounds().unwrap();
    assert_eq!(button.min.x - wide.max.x, 4.);
    assert_eq!(button.max.x, 600.);
    // Two digits fewer, two digits of badge fewer; the right edge stays put.
    c.region = Rect::new(Point { x: 550., y: 370. }, Point { x: 600., y: 400. });
    let narrow = badge(&mut c);
    assert_eq!(narrow.size().x, wide.size().x - 2. * 7.);
    assert_eq!(narrow.max.x, wide.max.x);
    assert_eq!(sizes, [BADGE_LABEL_SIZE; 2]);
}
#[test]
fn screen_button_expands_again_after_the_region_was_edited() {
    let mut c = three_screens();
    c.toggle_screen();
    c.region = Rect::new(Point { x: 10., y: 10. }, Point { x: 50., y: 50. });
    assert!(!c.screen_filled());
    c.toggle_screen();
    assert_eq!(c.region, c.monitors.first().map(|m| m.bounds));
}
#[test]
fn screen_button_uses_the_frame_without_monitor_list() {
    let mut c = capture();
    c.toggle_screen();
    assert_eq!(
        c.region,
        Rect::new(Point::default(), Point { x: 900., y: 600. })
    );
}
#[test]
fn screen_button_sits_right_of_the_badge_and_hides_while_selecting() {
    let mut c = capture();
    let badge = c.dimensions_bounds().unwrap();
    let button = c.screen_button_bounds().unwrap();
    assert!(badge.max.x < button.min.x && button.max.x == 600.);
    assert_eq!(c.cursor(button.center()), CursorKind::Hand);
    c.selecting = true;
    assert!(c.screen_button_bounds().is_none());
}
#[test]
fn screen_button_hint_names_its_shortcut_and_state() {
    let mut c = capture();
    let button = c.screen_button_bounds().unwrap();
    assert_eq!(
        c.hover_target(button.center()),
        Some((Action::FullScreen, button))
    );
    assert_eq!(c.screen_button().unwrap().label, toolbar::FULL_SCREEN_LABEL);
    c.toggle_screen();
    let button = c.screen_button().unwrap();
    assert!(button.active && button.label == toolbar::RESTORE_SELECTION_LABEL);
}
#[test]
fn the_toolbar_never_covers_the_size_badge_or_the_full_screen_button() {
    // A 1080p screen at 100 % and a 4K one at 200 %, swept in steps of 60 logical pixels.
    for scale in [1., 2.] {
        let screen = Point { x: 1920., y: 1080. } * scale;
        let mut c = Capture::new(Image::solid(screen.x as u32, screen.y as u32, [0; 4]));
        c.selecting = false;
        c.monitors = vec![Monitor {
            bounds: Rect::new(Point::default(), screen).unwrap(),
            scale,
        }];
        for (w, h) in [(80., 60.), (400., 300.), (900., 700.), (1920., 1080.)] {
            let size = Point { x: w, y: h } * scale;
            for x in (0..=(1920. - w) as i32).step_by(60) {
                for y in (0..=(1080. - h) as i32).step_by(60) {
                    let min = Point {
                        x: x as f64,
                        y: y as f64,
                    } * scale;
                    c.region = Rect::new(min, min + size);
                    let panels = toolbar::panels(&c.buttons(), c.ui_scale());
                    let badges = [c.dimensions_bounds(), c.screen_button_bounds()];
                    for badge in badges.into_iter().flatten() {
                        assert!(
                            panels.iter().all(|p| !toolbar::overlaps(*p, badge)),
                            "{badge:?} under the toolbar {panels:?} for {:?} at {scale}",
                            c.region
                        );
                    }
                }
            }
        }
    }
}
/// The tools and actions panels, then the badge with its button, for `region` on 1080p.
fn panels_and_badge(region: Rect) -> ([Rect; 2], Rect) {
    let mut c = Capture::new(Image::solid(1920, 1080, [0; 4]));
    c.selecting = false;
    c.region = Some(region);
    let panels = toolbar::panels(&c.buttons(), 1.);
    let badge = c.dimensions_bounds().unwrap();
    (
        [panels[0], panels[1]],
        badge.union(c.screen_button_bounds().unwrap()),
    )
}
#[test]
fn above_the_region_the_actions_slide_left_of_the_badge_at_its_height() {
    let region = Rect::new(Point { x: 1300., y: 700. }, Point { x: 1920., y: 1080. }).unwrap();
    let ([tools, actions], badge) = panels_and_badge(region);
    assert!(actions.max.y <= region.min.y && actions.max.y > badge.min.y);
    assert!(
        actions.max.x <= badge.min.x,
        "{actions:?} left of {badge:?}"
    );
    assert!(!toolbar::overlaps(actions, tools));
}
#[test]
fn the_actions_rise_over_the_badge_rather_than_cover_the_tools() {
    let region = Rect::new(Point { x: 1800., y: 900. }, Point { x: 1920., y: 1080. }).unwrap();
    let ([tools, actions], badge) = panels_and_badge(region);
    // Slid left, they would cross the tools column standing left of this narrow region.
    assert!(tools.max.x > badge.min.x - 8. - actions.size().x && tools.min.y < region.min.y);
    assert!(actions.max.y <= badge.min.y, "{actions:?} over {badge:?}");
    assert!(!toolbar::overlaps(actions, tools));
}
#[test]
fn every_state_hint_is_among_the_measured_ones() {
    use shotoshot_text::Language;
    let listed = toolbar::all_tooltips_in(Language::En);
    let mut c = capture();
    let mut seen = Vec::new();
    for (live, reopened, searching, board) in [
        (false, false, false, false),
        (true, false, true, false),
        (false, true, false, false),
        (false, false, false, true),
    ] {
        (c.live, c.reopened, c.searching, c.board) = (live, reopened, searching, board);
        for filled in [false, true] {
            if c.screen_filled() != filled {
                c.toggle_screen();
            }
            let buttons = c.buttons().into_iter().chain(c.screen_button());
            seen.extend(
                buttons
                    .chain(c.dimensions_button())
                    .map(|b| toolbar::tooltip_in(&b, Language::En)),
            );
        }
    }
    for hint in &seen {
        assert!(listed.contains(hint), "{hint} is never measured");
    }
    for hint in [
        "Nothing to undo · Ctrl+Z",
        "Google search in progress · Ctrl+G",
        "Live capture is for new captures",
        "Live capture on · F2 to freeze",
        "Capture frozen · F2 for live capture",
        "Back to previous selection · F",
        "Edit dimensions · expressions: 800+20, 1920/2",
        "Clear board · Ctrl+Z restores",
    ] {
        assert!(seen.iter().any(|s| s == hint), "{hint} not shown");
    }
}
#[test]
fn sampled_color_is_rgb_from_bgra_and_clamped() {
    let mut c = capture();
    c.image.pixels[..4].copy_from_slice(&[10, 20, 30, 255]);
    assert_eq!(c.sample_color(Point { x: -5., y: -5. }), [30, 20, 10]);
}
#[test]
fn the_luminance_under_the_cross_is_what_the_shade_leaves_of_the_capture() {
    let mut c = Capture::new(Image::solid(900, 600, [255, 255, 255, 255]));
    let at = Point { x: 300., y: 250. };
    // Before any region, the shade covers the white page: a mid grey, the cross's trap.
    assert_eq!(c.luminance_under(at, 12.), 133);
    c.region = Rect::new(Point { x: 100., y: 100. }, Point { x: 600., y: 400. });
    assert_eq!(
        c.luminance_under(at, 12.),
        255,
        "inside the region: no shade"
    );
    assert_eq!(c.luminance_under(Point { x: 50., y: 50. }, 12.), 133);
    // Left half black: the mean of what the square covers, 13 white columns of 25.
    for y in 0..600 {
        c.image.pixels[y * 900 * 4..(y * 900 + 300) * 4].fill(0);
    }
    let mean = c.luminance_under(at, 12.);
    assert!((125..=140).contains(&mean), "{mean}");
    // Pure blue is dark, pure yellow light, as the eye sees them.
    c.image
        .pixels
        .chunks_mut(4)
        .for_each(|p| p.copy_from_slice(&[255, 0, 0, 255]));
    assert_eq!(c.luminance_under(at, 12.), 29);
    c.image
        .pixels
        .chunks_mut(4)
        .for_each(|p| p.copy_from_slice(&[0, 255, 255, 255]));
    assert_eq!(c.luminance_under(at, 12.), 225);
    // Clamped at the corner, never past the image.
    assert_eq!(c.luminance_under(Point { x: -40., y: 900. }, 12.), 117);
}
