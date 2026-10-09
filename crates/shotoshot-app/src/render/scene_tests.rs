//! Check raster reuse through the same DC/Canvas path as capture painting.
use super::*;
use shotoshot_core::Style;
use std::cell::Cell;

thread_local! {
    static FILLS: Cell<usize> = const { Cell::new(0) };
}

pub(super) fn record_fill() {
    FILLS.set(FILLS.get() + 1);
}

fn p(x: f64, y: f64) -> Point {
    Point { x, y }
}

fn stroke(id: u64) -> Object {
    Object {
        id,
        rotation: 0.,
        fill: None,
        style: Style {
            color: [222, 55, 88],
            width: 5.,
        },
        shape: Shape::Stroke {
            points: vec![p(20.2, 60.7), p(160.4, 60.2), p(80.3, 20.6), p(80.1, 100.4)],
            widths: vec![3., 7., 3.],
            highlight: true,
        },
    }
}

fn renderer() -> Renderer {
    let mut renderer = Renderer::software().unwrap();
    let mut image = Image::solid(240, 180, [255; 4]);
    for (i, pixel) in image.pixels.as_chunks_mut::<4>().0.iter_mut().enumerate() {
        pixel[..3].copy_from_slice(&[
            (i % 256) as u8,
            ((i * 13) % 256) as u8,
            ((i * 29) % 256) as u8,
        ]);
    }
    renderer.set_image(&image).unwrap();
    renderer
}

fn paint(renderer: &Renderer, objects: &[Object], shade: f64, skip: Option<u64>) -> Image {
    crate::paint_image(240, 180, |dc| {
        renderer.begin(dc, 240, 180)?;
        let mut canvas = D2dCanvas::new(renderer, &renderer.dc)?;
        canvas.fill_rect(
            Rect {
                min: p(0., 0.),
                max: p(shade, 180.),
            },
            Ink::with_alpha([0, 0, 0], 0.48),
        )?;
        canvas.annotations(objects, skip)?;
        renderer.end()
    })
    .unwrap()
}

#[test]
fn unchanged_capture_pixels_skip_stroke_rasterization() {
    let _com = crate::ComApartment::new().unwrap();
    let renderer = renderer();
    let objects = [stroke(1)];
    FILLS.set(0);
    let first = paint(&renderer, &objects, 45., None);
    let second = paint(&renderer, &objects, 45., None);
    assert_eq!(first.pixels, second.pixels);
    assert_eq!(
        FILLS.get(),
        1,
        "unchanged capture rasterized the stroke again"
    );
}

#[test]
fn checkpoints_match_the_warmed_direct_render_on_both_backends() {
    let _com = crate::ComApartment::new().unwrap();
    for backend in [Backend::Software, Backend::Hardware] {
        let mut renderer = Renderer::with(backend, false).unwrap();
        renderer
            .set_image(&Image::solid(240, 180, [23, 97, 143, 255]))
            .unwrap();
        let mut objects = vec![stroke(1), stroke(2)];
        objects[1].translate(p(7.2, 2.4));
        objects.push(Object {
            id: 3,
            rotation: 2.3,
            fill: None,
            style: Style {
                color: [220, 15, 60],
                width: 3.,
            },
            shape: Shape::Text {
                position: p(100., 100.),
                text: "éM 文".into(),
                size: 28.,
                extent: renderer.measure_text("éM 文", 28.).unwrap(),
            },
        });
        let direct = || {
            crate::paint_image(240, 180, |dc| {
                renderer.begin(dc, 240, 180)?;
                D2dCanvas::new(&renderer, &renderer.dc)?.fill_rect(
                    Rect {
                        min: p(0., 0.),
                        max: p(45., 180.),
                    },
                    Ink::with_alpha([0, 0, 0], 0.48),
                )?;
                renderer.annotations(&renderer.dc, &objects)?;
                renderer.end()
            })
            .unwrap()
        };
        // The hardware renderer itself can differ on its first draw. Compare
        // against its repeatable direct output, without a pixel tolerance.
        direct();
        direct();
        let reference = direct();
        assert!(
            reference.pixels == direct().pixels,
            "direct renderer is not repeatable: {backend:?}"
        );
        let cold = paint(&renderer, &objects, 45., None);
        let warm = paint(&renderer, &objects, 45., None);
        assert!(
            reference.pixels == cold.pixels,
            "checkpoint capture changed pixels: {backend:?}"
        );
        assert!(
            reference.pixels == warm.pixels,
            "checkpoint restore changed pixels: {backend:?}"
        );
    }
}

#[test]
fn drawing_the_last_stroke_restores_the_unchanged_prefix() {
    let _com = crate::ComApartment::new().unwrap();
    let renderer = renderer();
    let mut objects = vec![stroke(1)];
    FILLS.set(0);
    paint(&renderer, &objects, 45., None);
    objects.push(stroke(2));
    paint(&renderer, &objects, 45., None);
    assert_eq!(FILLS.get(), 2, "adding a stroke rerasterized an old one");
    if let Shape::Stroke { points, .. } = &mut objects[1].shape {
        points.push(p(210., 130.));
    }
    let edited = paint(&renderer, &objects, 45., None);
    assert_eq!(
        FILLS.get(),
        3,
        "editing the last stroke rerasterized its prefix"
    );
    assert_eq!(edited.pixels, paint(&renderer, &objects, 45., None).pixels);
    assert_eq!(FILLS.get(), 3);
    assert_eq!(
        edited.pixels,
        paint(&self::renderer(), &objects, 45., None).pixels
    );
}

#[test]
fn crop_background_skip_and_deleted_objects_cannot_leave_stale_pixels() {
    let _com = crate::ComApartment::new().unwrap();
    let mut renderer = renderer();
    let mut objects = vec![stroke(1), stroke(2)];
    paint(&renderer, &objects, 45., None);
    for shade in [90., 10.] {
        assert_eq!(
            paint(&renderer, &objects, shade, None).pixels,
            paint(&self::renderer(), &objects, shade, None).pixels
        );
    }
    assert_eq!(
        paint(&renderer, &objects, 10., Some(1)).pixels,
        paint(&self::renderer(), &objects, 10., Some(1)).pixels
    );
    objects.remove(0);
    assert_eq!(
        paint(&renderer, &objects, 10., None).pixels,
        paint(&self::renderer(), &objects, 10., None).pixels
    );
    renderer
        .set_image(&Image::solid(240, 180, [60, 70, 80, 255]))
        .unwrap();
    let mut fresh = Renderer::software().unwrap();
    fresh
        .set_image(&Image::solid(240, 180, [60, 70, 80, 255]))
        .unwrap();
    assert_eq!(
        paint(&renderer, &objects, 10., None).pixels,
        paint(&fresh, &objects, 10., None).pixels
    );
    assert_eq!(
        paint(&renderer, &[], 10., None).pixels,
        paint(&fresh, &[], 10., None).pixels
    );
}

#[test]
fn live_checkpoint_copies_alpha_without_reblending_the_background() {
    let _com = crate::ComApartment::new().unwrap();
    for backend in [Backend::Software, Backend::Hardware] {
        let renderer = Renderer::with(backend, true).unwrap();
        let surface = crate::image::LiveSurface::new(240, 180).unwrap();
        let objects = [stroke(1)];
        let draw = || {
            renderer.begin(surface.dc(), 240, 180).unwrap();
            D2dCanvas::new(&renderer, &renderer.dc)
                .unwrap()
                .annotations(&objects, None)
                .unwrap();
            renderer.end().unwrap();
            [
                surface.pixel(40, 40),
                surface.pixel(80, 60),
                surface.pixel(0, 0),
            ]
        };
        let first = draw();
        assert_eq!(
            first[0][3], 1,
            "unpainted live input area retains alpha 1/255"
        );
        assert!(first[1][3] > 70 && first[1][3] < 90, "highlighter alpha");
        for _ in 0..4 {
            assert_eq!(first, draw(), "cached live alpha accumulated: {backend:?}");
        }
    }
}

#[test]
fn rotated_text_and_offscreen_strokes_keep_their_complete_pixels() {
    let _com = crate::ComApartment::new().unwrap();
    let renderer = renderer();
    let mut pen = stroke(1);
    pen.rotate(p(110., 80.), 1.9);
    pen.translate(p(-70., -15.));
    let mut text = Object {
        id: 2,
        rotation: 2.3,
        fill: None,
        style: Style {
            color: [22, 71, 126],
            width: 3.,
        },
        shape: Shape::Text {
            position: p(160., 80.),
            text: "éM 文".into(),
            size: 28.,
            extent: renderer.measure_text("éM 文", 28.).unwrap(),
        },
    };
    let mut objects = vec![pen, text.clone()];
    let first = paint(&renderer, &objects, 120., None);
    assert_eq!(first.pixels, paint(&renderer, &objects, 120., None).pixels);
    assert_eq!(
        first.pixels,
        paint(&self::renderer(), &objects, 120., None).pixels
    );
    text.rotate(p(110., 80.), 2.8);
    objects[1] = text;
    let edited = paint(&renderer, &objects, 120., None);
    assert_eq!(
        edited.pixels,
        paint(&self::renderer(), &objects, 120., None).pixels
    );
    assert_eq!(edited.pixels, paint(&renderer, &objects, 120., None).pixels);
    let source = Image::solid(240, 180, [80, 90, 100, 255]);
    let crop = Rect {
        min: p(80., 40.),
        max: p(220., 160.),
    };
    assert_eq!(
        renderer
            .export(&source, None, &objects, crop)
            .unwrap()
            .pixels,
        self::renderer()
            .export(&source, None, &objects, crop)
            .unwrap()
            .pixels,
        "preview checkpoints cannot leak across export targets or crops"
    );
}

#[test]
fn cached_arrow_prefix_updates_when_its_later_attachment_target_moves() {
    use shotoshot_core::Attachment;
    let _com = crate::ComApartment::new().unwrap();
    let renderer = renderer();
    let mut objects = vec![
        Object {
            id: 1,
            rotation: 0.,
            fill: None,
            style: Style::default(),
            shape: Shape::Arrow {
                start: p(10., 90.),
                end: p(110., 90.),
                start_attach: None,
                end_attach: Some(Attachment {
                    target: 2,
                    direction: p(-1., 0.),
                }),
                head: true,
            },
        },
        Object {
            id: 2,
            rotation: 0.,
            fill: None,
            style: Style::default(),
            shape: Shape::Rectangle(Rect {
                min: p(110., 50.),
                max: p(190., 130.),
            }),
        },
    ];
    let initial = paint(&renderer, &objects, 45., None);
    objects[1].translate(p(15., 20.));
    let moved = paint(&renderer, &objects, 45., None);
    assert_ne!(initial.pixels, moved.pixels);
    assert_eq!(
        moved.pixels,
        paint(&self::renderer(), &objects, 45., None).pixels
    );
}

#[test]
fn short_thick_arrow_head_fits_inside_the_checkpoint() {
    let _com = crate::ComApartment::new().unwrap();
    let renderer = renderer();
    let objects = [Object {
        id: 1,
        rotation: 0.,
        fill: None,
        style: Style {
            color: [220, 15, 60],
            width: 30.,
        },
        shape: Shape::Arrow {
            start: p(50., 60.),
            end: p(40., 50.),
            start_attach: None,
            end_attach: None,
            head: true,
        },
    }];
    let first = paint(&renderer, &objects, 45., None);
    let warm = paint(&renderer, &objects, 45., None);
    assert_eq!(
        first.pixels, warm.pixels,
        "cache cut off the scaled arrowhead"
    );
}

#[test]
fn rotated_rectangle_miter_corners_fit_inside_the_checkpoint() {
    let _com = crate::ComApartment::new().unwrap();
    let renderer = renderer();
    let objects = [Object {
        id: 1,
        rotation: std::f64::consts::FRAC_PI_4,
        fill: None,
        style: Style {
            color: [220, 15, 60],
            width: 40.,
        },
        shape: Shape::Rectangle(Rect {
            min: p(80., 65.),
            max: p(160., 115.),
        }),
    }];
    let first = paint(&renderer, &objects, 45., None);
    let warm = paint(&renderer, &objects, 45., None);
    assert!(
        first.pixels == warm.pixels,
        "cached rectangle cut off its rotated miter corners"
    );
}

#[test]
fn flattened_ellipse_miter_tips_fit_inside_the_checkpoint() {
    let _com = crate::ComApartment::new().unwrap();
    let mut renderer = Renderer::software().unwrap();
    renderer
        .set_image(&Image::solid(480, 180, [255; 4]))
        .unwrap();
    let objects = [Object {
        id: 1,
        rotation: 0.,
        fill: None,
        style: Style {
            color: [220, 15, 60],
            width: 40.,
        },
        shape: Shape::Ellipse(Rect {
            min: p(140., 88.),
            max: p(340., 92.),
        }),
    }];
    let draw = || {
        crate::paint_image(480, 180, |dc| {
            renderer.begin(dc, 480, 180)?;
            D2dCanvas::new(&renderer, &renderer.dc)?.annotations(&objects, None)?;
            renderer.end()
        })
        .unwrap()
    };
    let first = draw();
    let warm = draw();
    assert!(
        first.pixels == warm.pixels,
        "cached ellipse cut off its polygon miter tips"
    );
}

#[test]
fn cached_prefix_retains_unchanged_outlines_when_an_earlier_object_is_edited() {
    let _com = crate::ComApartment::new().unwrap();
    let renderer = renderer();
    let mut objects = vec![stroke(1), stroke(2)];
    stroke_tests::reset_builds();
    paint(&renderer, &objects, 45., None);
    objects.push(stroke(3));
    paint(&renderer, &objects, 45., None);
    assert_eq!(stroke_tests::builds(), 3);
    objects[0].translate(p(1., 0.));
    paint(&renderer, &objects, 45., None);
    assert_eq!(
        stroke_tests::builds(),
        4,
        "an unchanged prefix outline was discarded"
    );
}
