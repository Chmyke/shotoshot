use shotoshot_core::*;
fn p(x: f64, y: f64) -> Point {
    Point { x, y }
}
fn rectangle() -> Editor {
    let mut e = Editor::default();
    e.begin(p(20., 40.), Modifiers::default());
    e.update(p(120., 80.));
    e.finish();
    e.select_only(e.objects()[0].id);
    e.tool = Tool::Select;
    e
}
const BIND: Modifiers = Modifiers {
    shift: false,
    alt: false,
    ctrl: true,
};
#[test]
fn shift_snaps_line_and_arrow_to_diagonal() {
    for tool in [Tool::Line, Tool::Arrow, Tool::Pen, Tool::Highlight] {
        let mut e = Editor::default();
        e.tool = tool;
        e.begin(
            p(0., 0.),
            Modifiers {
                shift: true,
                alt: false,
                ctrl: false,
            },
        );
        e.update(p(100., 70.));
        e.finish();
        let r = e.objects()[0].bounds();
        assert!((r.size().x - r.size().y).abs() < 1e-8);
    }
}
#[test]
fn selected_rotation_handle_rotates_rectangle_and_undo_restores_it() {
    let mut e = rectangle();
    let before = e.objects().to_vec();
    e.begin(p(70., 12.), Modifiers::default());
    e.update(p(118., 60.));
    e.finish();
    let r = e.objects()[0].bounds();
    assert!((r.size().x - 40.).abs() < 1e-8);
    assert!((r.size().y - 100.).abs() < 1e-8);
    assert!(e.objects()[0].hit(p(50., 60.), 0.1, e.objects()));
    e.undo();
    assert_eq!(e.objects(), before);
}
#[test]
fn arrow_head_width_is_ninety_percent_of_length() {
    let mut e = Editor::default();
    e.tool = Tool::Arrow;
    e.begin(p(0., 0.), Modifiers::default());
    e.update(p(200., 0.));
    e.finish();
    let paths = paths(&e.objects()[0], e.objects());
    let h = &paths[1];
    let length = h[1].x - h[0].x;
    assert!(((h[0].y - h[2].y) / length - 0.9).abs() < 1e-8);
}
#[test]
fn copy_paste_remaps_internal_attachments_and_detaches_external_ones() {
    let mut e = rectangle();
    let target = e.objects()[0].id;
    e.tool = Tool::Arrow;
    e.begin(p(200., 60.), BIND);
    e.update(p(110., 60.));
    e.finish();
    let arrow = e.objects()[1].id;
    e.selected = vec![target, arrow];
    let clipboard = e.copy_selected();
    assert!(e.paste_objects(&clipboard, p(10., 20.)));
    let copied_target = e.objects()[2].id;
    let copied_arrow = &e.objects()[3];
    assert!(
        matches!(&copied_arrow.shape, Shape::Arrow{end_attach:Some(a),..} if a.target==copied_target)
    );
    let (_, end) = arrow_points(copied_arrow, e.objects()).unwrap();
    assert_eq!(end, p(131.5, 80.));
    e.undo();
    assert_eq!(e.objects().len(), 2);
    e.redo();
    assert_eq!(e.objects().len(), 4);
    e.select_only(arrow);
    let clipboard = e.copy_selected();
    e.paste_objects(&clipboard, p(0., 10.));
    assert!(
        matches!(&e.objects()[4].shape, Shape::Arrow{end_attach:None,end,..} if *end==p(121.5,70.))
    );
}
#[test]
fn changing_shift_without_motion_recomputes_original_pointer() {
    let mut e = Editor::default();
    e.tool = Tool::Line;
    e.begin(p(0., 0.), Modifiers::default());
    e.update(p(-100., 70.));
    e.update_modifiers(Modifiers {
        shift: true,
        alt: false,
        ctrl: false,
    });
    let r = e.objects()[0].bounds();
    assert!((r.size().x - r.size().y).abs() < 1e-8);
    e.update_modifiers(Modifiers::default());
    assert_eq!(
        e.objects()[0].bounds(),
        Rect::new(p(0., 0.), p(-100., 70.)).unwrap()
    );
}
#[test]
fn text_size_content_color_and_measurements_commit_together() {
    let mut e = Editor::default();
    let id = e
        .add_text_measured_sized_colored(p(10., 20.), "A".into(), p(25., 45.), [2, 3, 4], 36.)
        .unwrap();
    let original = e.objects()[0].clone();
    e.update_text_measured_sized_colored(id, "AB".into(), p(55., 60.), [5, 6, 7], 48.);
    assert!(
        matches!(&e.objects()[0].shape,Shape::Text{size,extent,..} if *size==48. && *extent==p(55.,60.))
    );
    e.undo();
    assert_eq!(e.objects(), &[original]);
    e.undo();
    assert!(e.objects().is_empty());
}
#[test]
fn rotation_cancel_restores_text_and_hit_testing_uses_rotation() {
    let mut e = Editor::default();
    let id = e
        .add_text_measured_sized_colored(p(20., 40.), "ABC".into(), p(100., 40.), [0, 0, 0], 24.)
        .unwrap();
    e.select_only(id);
    let original = e.objects().to_vec();
    assert!(e.begin_rotation(p(70., 12.)));
    e.update(p(118., 60.));
    assert!(e.objects()[0].hit(p(70., 100.), 0., e.objects()));
    assert!(!e.objects()[0].hit(p(115., 60.), 0., e.objects()));
    e.cancel();
    assert_eq!(e.objects(), original);
}
#[test]
fn shift_arrow_uses_actual_start_and_snaps_existing_endpoint() {
    let mut e = rectangle();
    e.tool = Tool::Arrow;
    e.begin(p(70., 60.), Modifiers::default());
    e.update_with_modifiers(
        p(200., 140.),
        Modifiers {
            shift: true,
            alt: false,
            ctrl: false,
        },
    );
    e.finish();
    let (a, b) = arrow_points(&e.objects()[1], e.objects()).unwrap();
    assert!(((b - a).x - (b - a).y).abs() < 1e-8);
    e.select_only(e.objects()[1].id);
    e.tool = Tool::Select;
    e.begin(b, Modifiers::default());
    e.update_with_modifiers(
        p(180., 70.),
        Modifiers {
            shift: true,
            alt: false,
            ctrl: false,
        },
    );
    let (a, b) = arrow_points(&e.objects()[1], e.objects()).unwrap();
    assert!((a.y - b.y).abs() < 1e-8);
}
#[test]
fn attachments_follow_rotated_target_local_surface() {
    let mut e = rectangle();
    let target = e.objects()[0].id;
    e.tool = Tool::Arrow;
    e.begin(p(200., 60.), BIND);
    e.update(p(110., 60.));
    e.finish();
    e.select_only(target);
    e.begin_rotation(p(70., 12.));
    e.update(p(118., 60.));
    e.finish();
    // Half of the default 3 px stroke beyond the rotated edge.
    let (_, b) = arrow_points(&e.objects()[1], e.objects()).unwrap();
    assert!((b - p(70., 111.5)).length() < 1e-8);
    let a = attach_at(p(70., 100.), e.objects()).unwrap();
    assert!((resolve(a, e.objects(), 0.).unwrap() - p(70., 111.5)).length() < 1e-8);
}
#[test]
fn bound_endpoints_touch_the_outer_edge_of_thick_strokes() {
    // A filled head touches the edge; a round line cap clears it by its own radius.
    for (linear, cap) in [(Tool::Arrow, 0.), (Tool::Line, 6.)] {
        for tool in [Tool::Rectangle, Tool::Ellipse] {
            for target in [p(130., 70.), p(170., 85.), p(40., 50.)] {
                let mut e = Editor::default();
                e.style.width = 12.;
                e.tool = tool;
                e.begin(p(20., 40.), Modifiers::default());
                e.update(p(180., 100.));
                e.finish();
                e.tool = linear;
                e.begin(p(100., 70.) + (target - p(100., 70.)) * 4., BIND);
                e.update(target);
                e.finish();
                let objects = e.objects();
                let (_, b) = arrow_points(&objects[1], objects).unwrap();
                let centerline = paths(&objects[0], objects)
                    .iter()
                    .flat_map(|path| path.windows(2).map(|w| segment_distance(b, w[0], w[1])))
                    .fold(f64::MAX, f64::min);
                assert!(
                    (centerline - 6. - cap).abs() < 0.25,
                    "{linear:?} to {tool:?} toward {target:?}: {centerline}"
                );
                // Outside the centerline, not inside the shape.
                let q = b - p(100., 70.);
                let (x, y) = ((q.x / 80.).abs(), (q.y / 30.).abs());
                let outside = if tool == Tool::Ellipse {
                    x.hypot(y)
                } else {
                    x.max(y)
                };
                assert!(outside > 1., "{linear:?} to {tool:?} {b:?}");
            }
        }
    }
}
#[test]
fn text_resizing_limits_keep_extent_in_step_with_font_size() {
    let mut e = Editor::default();
    e.add_text_measured_sized_colored(p(0., 0.), "X".into(), p(100., 100.), [0, 0, 0], 100.);
    let mut o = e.objects()[0].clone();
    o.transform(o.bounds(), Rect::new(p(0., 0.), p(1000., 1000.)).unwrap());
    assert!(matches!(o.shape,Shape::Text{size,extent,..} if size==300. && extent==p(300.,300.)));
}
#[test]
fn rotated_resize_keeps_shape_proportions_and_opposite_corner_with_undo() {
    let mut e = rectangle();
    e.begin_rotation(p(70., 12.));
    e.update(p(118., 60.));
    e.finish();
    let original = e.objects().to_vec();
    let r = e.selection_bounds().unwrap();
    e.begin(r.max, Modifiers::default());
    e.update(r.max + p(40., 0.));
    e.finish();
    let resized = e.selection_bounds().unwrap();
    assert!((resized.size().x / resized.size().y - 0.4).abs() < 1e-8);
    assert!((resized.min - r.min).length() < 1e-8);
    assert!(resized.size().x > r.size().x);
    e.undo();
    assert_eq!(e.objects(), original);
}
#[test]
fn rotated_ellipse_paths_hit_and_bounds_agree() {
    let mut e = Editor::default();
    e.tool = Tool::Ellipse;
    e.begin(
        p(100., 100.),
        Modifiers {
            alt: true,
            ctrl: false,
            shift: false,
        },
    );
    e.update(p(160., 120.));
    e.finish();
    e.select_only(e.objects()[0].id);
    e.begin_rotation(p(100., 52.));
    e.update(p(148., 100.));
    e.finish();
    let o = &e.objects()[0];
    assert!((o.bounds().size() - p(40., 120.)).length() < 1e-8);
    for path in paths(o, e.objects()) {
        for point in path {
            assert!(o.hit(point, 0.01, e.objects()));
            assert!(o.bounds().expanded(1e-8).contains(point));
        }
    }
    assert!(!o.hit(p(150., 100.), 1., e.objects()));
}

#[test]
fn text_size_batch_is_atomic_and_one_undo_for_all_selected_text() {
    let mut e = Editor::default();
    let a = e
        .add_text_measured_sized_colored(p(10., 20.), "A".into(), p(20., 30.), [1, 2, 3], 24.)
        .unwrap();
    let b = e
        .add_text_measured_sized_colored(p(50., 60.), "B".into(), p(30., 40.), [4, 5, 6], 32.)
        .unwrap();
    e.selected = vec![a, b];
    e.begin_rotation(p(45., -8.));
    e.update(p(113., 60.));
    e.finish();
    let original = e.objects().to_vec();
    assert!(e.set_text_sizes(&[(a, 28., p(25., 35.)), (b, 36., p(35., 45.))]));
    for (index, size, extent) in [(0, 28., p(25., 35.)), (1, 36., p(35., 45.))] {
        let mut expected = original[index].clone();
        if let Shape::Text {
            size: value,
            extent: measured,
            ..
        } = &mut expected.shape
        {
            *value = size;
            *measured = extent;
        }
        assert_eq!(e.objects()[index], expected);
    }
    let changed = e.objects().to_vec();
    e.undo();
    assert_eq!(e.objects(), original);
    e.redo();
    assert_eq!(e.objects(), changed);
    assert!(!e.set_text_sizes(&[(a, 30., p(30., 40.)), (b, f64::NAN, p(40., 50.))]));
    assert_eq!(e.objects(), changed);
}

#[test]
fn rotating_arrow_bakes_endpoints_so_endpoint_drag_has_no_residual_transform() {
    let mut e = Editor::default();
    e.tool = Tool::Arrow;
    e.begin(p(20., 60.), Modifiers::default());
    e.update(p(120., 60.));
    e.finish();
    e.select_only(e.objects()[0].id);
    e.begin_rotation(p(70., 32.));
    e.update(p(98., 60.));
    e.finish();
    e.tool = Tool::Select;
    let (a, b) = arrow_points(&e.objects()[0], e.objects()).unwrap();
    assert!((a - p(70., 10.)).length() < 1e-8);
    assert!((b - p(70., 110.)).length() < 1e-8);
    assert_eq!(e.objects()[0].rotation, 0.);
    let original = e.objects().to_vec();
    e.begin(b, Modifiers::default());
    e.update(p(150., 110.));
    e.finish();
    let (a, b) = arrow_points(&e.objects()[0], e.objects()).unwrap();
    assert!((a - p(70., 10.)).length() < 1e-8);
    assert_eq!(b, p(150., 110.));
    assert!(e.objects()[0].hit(p(110., 60.), 1., e.objects()));
    e.undo();
    assert_eq!(e.objects(), original);
}

#[test]
fn pen_and_highlighter_shift_changes_during_gesture_and_undo_is_atomic() {
    for tool in [Tool::Pen, Tool::Highlight] {
        let mut e = Editor::default();
        e.tool = tool;
        e.begin(p(10., 10.), Modifiers::default());
        e.update(p(30., 70.));
        e.update(p(100., 80.));
        e.update_modifiers(Modifiers {
            shift: true,
            alt: false,
            ctrl: false,
        });
        let Shape::Stroke {
            points,
            widths,
            highlight,
        } = &e.objects()[0].shape
        else {
            panic!()
        };
        assert_eq!(points.len(), 2);
        assert!(widths.is_empty());
        assert_eq!(*highlight, tool == Tool::Highlight);
        assert!((points[1].x - points[1].y).abs() < 1e-8);
        e.update_modifiers(Modifiers::default());
        e.update(p(130., 100.));
        let Shape::Stroke { points, .. } = &e.objects()[0].shape else {
            panic!()
        };
        assert_eq!(points.last(), Some(&p(130., 100.)));
        e.finish();
        e.undo();
        assert!(e.objects().is_empty());
        e.redo();
        assert_eq!(e.objects().len(), 1);
    }
}
#[test]
fn linear_shapes_bind_only_while_ctrl_or_alt_is_held() {
    let bound = |m: Modifiers| m.ctrl || m.alt;
    let cases = [
        Modifiers::default(),
        BIND,
        Modifiers {
            alt: true,
            ..Modifiers::default()
        },
        Modifiers {
            shift: true,
            ..BIND
        },
    ];
    for tool in [Tool::Arrow, Tool::Line] {
        for m in cases {
            let mut e = rectangle();
            e.tool = tool;
            e.begin(p(200., 60.), m);
            e.update(p(110., 60.));
            e.finish();
            let Shape::Arrow { end_attach, .. } = e.objects()[1].shape else {
                panic!("linear shape");
            };
            assert_eq!(end_attach.is_some(), bound(m) && !m.shift, "{tool:?} {m:?}");
        }
    }
}
#[test]
fn bound_line_follows_its_target_and_releasing_ctrl_unbinds_live() {
    let mut e = rectangle();
    let target = e.objects()[0].id;
    e.tool = Tool::Line;
    e.begin(p(200., 60.), BIND);
    e.update(p(110., 60.));
    let (_, snapped) = arrow_points(&e.objects()[1], e.objects()).unwrap();
    assert_eq!(snapped, p(123., 60.));
    e.update_modifiers(Modifiers::default());
    let (_, free) = arrow_points(&e.objects()[1], e.objects()).unwrap();
    assert_eq!(free, p(110., 60.));
    e.update_modifiers(BIND);
    e.finish();
    e.select_only(target);
    e.nudge(p(0., 30.));
    let (start, end) = arrow_points(&e.objects()[1], e.objects()).unwrap();
    assert_eq!((start, end), (p(200., 60.), p(123., 90.)));
}
#[test]
fn dragging_an_endpoint_binds_only_with_ctrl_or_alt() {
    for (m, bound) in [(Modifiers::default(), false), (BIND, true)] {
        let mut e = rectangle();
        e.tool = Tool::Arrow;
        e.begin(p(300., 60.), Modifiers::default());
        e.update(p(200., 60.));
        e.finish();
        e.tool = Tool::Select;
        e.select_only(e.objects()[1].id);
        e.begin(p(200., 60.), m);
        e.update(p(110., 60.));
        e.finish();
        let Shape::Arrow { end_attach, .. } = e.objects()[1].shape else {
            panic!("arrow");
        };
        assert_eq!(end_attach.is_some(), bound);
    }
}
#[test]
fn turned_selection_frame_follows_the_object_and_resizes_from_its_turned_corner() {
    let mut e = rectangle();
    let (center, angle) = (p(70., 60.), 30f64.to_radians());
    e.begin(p(70., 12.), Modifiers::default());
    e.update(rotate_point(p(70., 12.), center, angle));
    e.finish();
    let frame = e.selection_frame().unwrap();
    assert!((frame.angle - angle).abs() < 1e-9);
    assert!((frame.bounds.size() - p(100., 40.)).length() < 1e-9);
    // The handle turns with the frame instead of sitting above the axis-aligned bounds.
    let handle = e.rotation_handle().unwrap();
    assert!((handle - rotate_point(p(70., 12.), center, angle)).length() < 1e-9);
    // Dragging a turned corner outward scales around the turned opposite corner.
    let [opposite, _, corner, _] = frame.corners();
    e.begin(corner, Modifiers::default());
    e.update(opposite + (corner - opposite) * 1.5);
    e.finish();
    let resized = e.selection_frame().unwrap();
    assert!((resized.corners()[0] - opposite).length() < 1e-6);
    assert!((resized.bounds.size() - p(150., 60.)).length() < 1e-6);
    assert!((resized.angle - angle).abs() < 1e-9);
}
