use shotoshot_core::*;

fn point(x: f64, y: f64) -> Point {
    Point { x, y }
}
fn ellipse(x: f64, y: f64) -> Object {
    Object {
        rotation: 0.,
        fill: Vec::new(),
        id: 42,
        shape: Shape::Ellipse(Rect {
            min: point(-x, -y),
            max: point(x, y),
        }),
        style: Style {
            width: 1.,
            ..Style::default()
        },
    }
}
fn arrow(length: f64) -> Object {
    Object {
        rotation: 0.,
        fill: Vec::new(),
        id: 42,
        shape: Shape::Arrow {
            start: point(0., 0.),
            end: point(length, 0.),
            start_attach: None,
            end_attach: None,
            head: true,
            bend: Default::default(),
            elbow: false,
        },
        style: Style {
            width: 3.,
            ..Style::default()
        },
    }
}

#[test]
fn large_circle_chords_stay_within_quarter_pixel_of_curve() {
    let object = ellipse(4000., 4000.);
    for segment in paths(&object, &[])[0].windows(2) {
        assert!(4000. - ((segment[0] + segment[1]) * 0.5).length() <= 0.25);
    }
}

#[test]
fn small_circle_stays_small_and_closes_exactly() {
    let object = ellipse(2., 2.);
    for path in paths(&object, &[]) {
        assert_eq!(path.first(), path.last());
        for p in path {
            assert!((p.length() - 2.).abs() <= 0.4);
        }
    }
}

#[test]
fn arrow_wheel_width_scales_the_head_without_moving_its_tip() {
    for length in [2., 20., 4000.] {
        let mut editor = Editor::default();
        editor.tool = Tool::Arrow;
        editor.begin(point(0., 0.), Modifiers::default());
        editor.update(point(length, 0.));
        for width in [1., 2., 10., 40., 1.] {
            editor.style.width = width;
            editor.apply_style();
            let rendered = paths(&editor.objects()[0], editor.objects());
            let head = &rendered[1];
            assert_eq!(head[1], point(length, 0.), "scale moved the tip");
            assert!((head[1].x - head[0].x - 3.84 * width).abs() < 1e-8);
            assert!((head[0].y - head[2].y - 3.456 * width).abs() < 1e-8);
        }
    }
}

#[test]
fn arrow_keeps_shaft_and_head_joined_at_every_size() {
    for length in [2., 20., 4000.] {
        let object = arrow(length);
        let rendered = paths(&object, &[]);
        for pass in rendered.as_chunks::<2>().0 {
            let base = (pass[1][0] + pass[1][2]) * 0.5;
            if length > 11.52 + 1.5 {
                let shaft_end = *pass[0].last().unwrap();
                assert!(shaft_end.x >= base.x && shaft_end.x < length);
                assert_eq!(shaft_end.y, base.y);
                assert_eq!(pass[0].first(), Some(&point(0., 0.)));
            } else {
                assert!(
                    pass[0].is_empty(),
                    "a short arrow must not reverse its shaft"
                );
            }
            assert_eq!(pass[1].first(), pass[1].last());
            let interior = base * 0.6 + pass[1][1] * 0.2 + pass[1][0] * 0.2;
            assert!(object.hit(interior, 0., &[]));
            assert!(pass[1].contains(&point(length, 0.)));
        }
    }
}

#[test]
fn flat_ellipse_does_not_hit_far_beyond_its_tip() {
    let object = ellipse(1000., 5.);
    assert!(!object.hit(point(1100., 0.), 5., &[]));
    assert!(object.hit(point(1002., 0.), 5., &[]));
}

#[test]
fn arrow_head_is_selectable_away_from_shaft() {
    let object = arrow(200.);
    let wing = paths(&object, &[])[1][0];
    assert!(object.hit(wing, 2., &[]));
}

#[test]
fn huge_ellipse_sampling_is_bounded_and_paths_translate() {
    assert!(paths(&ellipse(1e12, 1e12), &[])[0].len() <= 4097);
    assert!(
        paths(&ellipse(1e12, 1e12), &[])
            .iter()
            .all(|path| path.len() <= 4097)
    );
    for mut object in [ellipse(4000., 4.), arrow(4000.)] {
        let before = paths(&object, &[]);
        assert_eq!(before, paths(&object, &[]));
        object.translate(point(30., 60.));
        let after = paths(&object, &[]);
        assert_eq!(before.len(), after.len());
        for (original, moved) in before.iter().zip(&after) {
            assert_eq!(original.len(), moved.len());
            for (a, b) in original.iter().zip(moved) {
                assert!((*b - *a - point(30., 60.)).length() < 1e-8);
            }
        }
    }
}

#[test]
fn pen_smoothing_preserves_endpoints_bounds_and_straight_lines() {
    for points in [
        vec![point(0., 0.), point(10., 0.), point(10., 10.)],
        vec![point(0., 0.), point(10., 0.), point(20., 0.)],
    ] {
        let o = Object {
            rotation: 0.,
            fill: Vec::new(),
            id: 1,
            shape: Shape::Stroke {
                widths: Vec::new(),
                points: points.clone(),
                highlight: false,
            },
            style: Style::default(),
        };
        let p = paths(&o, &[]).remove(0);
        assert_eq!(p.first(), points.first());
        assert_eq!(p.last(), points.last());
        assert!(p.iter().all(|p| o.bounds().contains(*p)));
        assert!(p.iter().all(|p| {
            points
                .windows(2)
                .any(|w| segment_distance(*p, w[0], w[1]) <= 2.)
        }));
        if points[2].y > 0. {
            assert!(!p.contains(&points[1]));
        } else {
            assert!(p.iter().all(|p| p.y == 0.));
        }
    }
}

#[test]
fn arrow_inside_shape_does_not_collapse_onto_one_attachment() {
    let mut editor = Editor::default();
    editor.tool = Tool::Rectangle;
    editor.begin(point(0., 0.), Modifiers::default());
    editor.update(point(200., 200.));
    editor.finish();
    editor.tool = Tool::Arrow;
    editor.style.width = 1.;
    editor.begin(
        point(130., 100.),
        Modifiers {
            ctrl: true,
            ..Modifiers::default()
        },
    );
    editor.update(point(180., 100.));
    let arrow = editor.objects().last().unwrap();
    let (a, b) = arrow_points(arrow, editor.objects()).unwrap();
    assert!(
        (b - a).length() >= 1.,
        "attachments swallowed the live arrow"
    );
}
