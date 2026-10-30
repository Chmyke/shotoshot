#![cfg(windows)]
//! The portable renderer (Linux, macOS) must draw annotations like Direct2D does.
use shotoshot_app::{ComApartment, Image, Renderer, save_png};
use shotoshot_core::*;
use shotoshot_raster::Raster;

fn scene() -> Vec<Object> {
    let mut e = Editor::default();
    let draw = |e: &mut Editor, tool: Tool, color: [u8; 3], width: f64, points: &[Point]| {
        e.tool = tool;
        e.style = Style { color, width };
        e.begin(points[0], Modifiers::default());
        for p in &points[1..] {
            e.update(*p);
        }
        e.finish();
    };
    let p = |x: f64, y: f64| Point { x, y };
    // The arrow or line just drawn, bent by its middle handle.
    let bend = |e: &mut Editor, by: Point| {
        let (_, middle) = e.bend_handle().unwrap();
        e.begin(middle, Modifiers::default());
        e.update(middle + by);
        e.finish();
    };
    draw(
        &mut e,
        Tool::Rectangle,
        [237, 79, 92],
        3.,
        &[p(30., 30.), p(210., 150.)],
    );
    draw(
        &mut e,
        Tool::Ellipse,
        [80, 153, 244],
        5.,
        &[p(330., 90.), p(400., 90.)],
    );
    draw(
        &mut e,
        Tool::Arrow,
        [40, 60, 80],
        4.,
        &[p(60., 260.), p(300., 190.)],
    );
    bend(&mut e, p(0., -30.));
    draw(
        &mut e,
        Tool::Line,
        [64, 199, 128],
        6.,
        &[p(60., 320.), p(420., 300.)],
    );
    bend(&mut e, p(0., 40.));
    draw(
        &mut e,
        Tool::Pen,
        [28, 31, 40],
        8.,
        &[p(250., 40.), p(280., 70.), p(270., 120.), p(320., 150.)],
    );
    draw(
        &mut e,
        Tool::Highlight,
        [250, 192, 66],
        6.,
        &[p(120., 380.), p(260., 385.), p(430., 370.)],
    );
    // Fills lie under their own outline and under whatever was drawn afterwards.
    e.style.color = [250, 192, 66];
    e.fill_at(p(120., 90.), false);
    e.style.color = [28, 31, 40];
    e.fill_at(p(380., 90.), false);
    assert!(e.objects()[..2].iter().all(|o| !o.fill.is_empty()));
    let mut objects = e.objects().to_vec();
    // An elbow arrow out of the rectangle's bottom, one rounded turn to a free point.
    objects.push(Object {
        id: e.last_id() + 1,
        rotation: 0.,
        fill: Vec::new(),
        style: Style {
            color: [80, 153, 244],
            width: 5.,
        },
        shape: Shape::Arrow {
            start: p(120., 150.),
            end: p(300., 250.),
            start_attach: Some(Attachment {
                target: objects[0].id,
                direction: p(0., 1.),
            }),
            end_attach: None,
            head: true,
            bend: Point::default(),
            elbow: true,
        },
    });
    objects
}

#[test]
fn both_renderers_fill_shapes_and_the_capture_with_the_exact_colour() {
    let _com = ComApartment::new().unwrap();
    const SLATE: [u8; 3] = [28, 31, 40];
    let image = Image::solid(480, 420, [245, 242, 238, 255]);
    let crop = Rect::new(Point { x: 20., y: 10. }, Point { x: 470., y: 410. }).unwrap();
    let objects = scene();
    for output in [
        Renderer::new()
            .unwrap()
            .export(&image, &[Paint::solid(SLATE)], &objects, crop)
            .unwrap(),
        Raster::new()
            .export(&image, &[Paint::solid(SLATE)], &objects, crop)
            .unwrap(),
    ] {
        assert_eq!((output.width, output.height), (450, 400));
        let rgb = |x: usize, y: usize| {
            let i = ((y - 10) * 450 + x - 20) * 4;
            [output.pixels[i + 2], output.pixels[i + 1], output.pixels[i]]
        };
        assert_eq!(rgb(120, 90), [250, 192, 66], "filled rectangle");
        assert_eq!(rgb(380, 90), SLATE, "filled circle");
        assert_eq!(rgb(30, 89), [237, 79, 92], "outline over the fill");
        for (x, y) in [(20, 10), (469, 409), (440, 200), (265, 25)] {
            assert_eq!(rgb(x, y), SLATE, "background at {x},{y}");
        }
    }
}

#[test]
fn both_renderers_keep_fractional_arrow_joins_solid_and_tails_round() {
    let _com = ComApartment::new().unwrap();
    let image = Image::solid(240, 160, [255; 4]);
    let crop = Rect::new(Point::default(), Point { x: 240., y: 160. }).unwrap();
    let objects = [Object {
        id: 1,
        rotation: 0.,
        fill: Vec::new(),
        style: Style {
            color: [255, 0, 0],
            width: 10.,
        },
        shape: Shape::Arrow {
            start: Point { x: 20., y: 80. },
            end: Point { x: 220.7, y: 80. },
            start_attach: None,
            end_attach: None,
            head: true,
            bend: Default::default(),
            elbow: false,
        },
    }];
    for output in [
        Renderer::new()
            .unwrap()
            .export(&image, &[], &objects, crop)
            .unwrap(),
        Raster::new().export(&image, &[], &objects, crop).unwrap(),
    ] {
        for (x, y) in [(182, 80), (183, 80), (17, 80), (190, 74)] {
            let offset = (y * 240 + x) * 4;
            assert_eq!(&output.pixels[offset..offset + 4], &[0, 0, 255, 255]);
        }
    }
}

#[test]
fn portable_renderer_matches_direct2d_annotations() {
    let _com = ComApartment::new().unwrap();
    let image = Image::solid(480, 420, [245, 242, 238, 255]);
    let crop = Rect::new(Point::default(), Point { x: 480., y: 420. }).unwrap();
    let objects = scene();
    // The slate spares every closed shape, as a coat poured after them does.
    let walls = objects
        .iter()
        .filter(|o| o.closed())
        .map(|o| o.id)
        .collect();
    let slate = Paint {
        color: [28, 31, 40],
        walls,
    };
    for (background, name) in [(vec![], "parity"), (vec![slate], "parity-slate")] {
        let native = Renderer::new()
            .unwrap()
            .export(&image, &background, &objects, crop)
            .unwrap();
        let portable = Raster::new()
            .export(&image, &background, &objects, crop)
            .unwrap();
        let target = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target");
        save_png(
            &native,
            &target.join(format!("shotoshot-{name}-direct2d.png")),
        )
        .unwrap();
        save_png(
            &portable,
            &target.join(format!("shotoshot-{name}-portable.png")),
        )
        .unwrap();
        let pixels = native.pixels.len() / 4;
        let (mut far, mut total) = (0usize, 0u64);
        for (a, b) in native
            .pixels
            .as_chunks::<4>()
            .0
            .iter()
            .zip(portable.pixels.as_chunks::<4>().0)
        {
            let d = (0..3).map(|i| a[i].abs_diff(b[i])).max().unwrap();
            total += d as u64;
            // Anti-aliasing differs by a few levels on edges; a far pixel is a shape mismatch.
            if d > 96 {
                far += 1;
            }
        }
        let mean = total as f64 / pixels as f64;
        assert!(mean < 1.5, "{name}: mean channel difference {mean:.2}");
        assert!(
            (far as f64 / pixels as f64) < 0.003,
            "{name}: {far} of {pixels} pixels differ strongly"
        );
    }
}

#[test]
fn both_renderers_blur_the_same_pixels_on_screen_and_in_the_export() {
    use shotoshot_ui::Capture;
    let _com = ComApartment::new().unwrap();
    // A capture whose every pixel differs from its neighbours.
    let mut image = Image::solid(320, 200, [0, 0, 0, 255]);
    for (i, p) in image.pixels.as_chunks_mut::<4>().0.iter_mut().enumerate() {
        let noise = (i as u32).wrapping_mul(2_654_435_761);
        p[..3].copy_from_slice(&[(noise >> 24) as u8, (noise >> 16) as u8, (noise >> 8) as u8]);
    }
    let blur = |id, r: Rect, width| Object {
        id,
        rotation: 0.,
        fill: Vec::new(),
        style: Style {
            color: [0; 3],
            width,
        },
        shape: Shape::Blur(r),
    };
    let r = |x0, y0, x1, y1| Rect::new(Point { x: x0, y: y0 }, Point { x: x1, y: y1 }).unwrap();
    // Two that overlap, one past the frame's edge, and an arrow over them all.
    let mut objects = vec![
        blur(1, r(20.4, 30., 180., 120.), 7.),
        blur(2, r(140., 90., 260., 170.), 12.),
        blur(3, r(250., -40., 360., 60.), 3.),
    ];
    objects.push(Object {
        id: 4,
        rotation: 0.,
        fill: Vec::new(),
        style: Style {
            color: [237, 79, 92],
            width: 4.,
        },
        shape: Shape::Arrow {
            start: Point { x: 30., y: 180. },
            end: Point { x: 290., y: 20. },
            start_attach: None,
            end_attach: None,
            head: true,
            bend: Default::default(),
            elbow: false,
        },
    });
    let mut shown = image.clone();
    assert!(shotoshot_core::blur::flatten(
        &mut shown.pixels,
        320,
        200,
        &objects
    ));
    let crop = r(0., 0., 320., 200.);
    let native = Renderer::new()
        .unwrap()
        .export(&image, &[], &objects, crop)
        .unwrap();
    let portable = Raster::new().export(&image, &[], &objects, crop).unwrap();
    // Off the arrow, both exports hold exactly the shared blur of the capture.
    let ink =
        |image: &Image, i: usize| image.pixels[i * 4..i * 4 + 3] != shown.pixels[i * 4..i * 4 + 3];
    let (mut same, mut inked) = (0, 0);
    for i in 0..320 * 200 {
        if ink(&native, i) || ink(&portable, i) {
            inked += 1;
            continue;
        }
        same += 1;
    }
    assert!(
        inked < 4000,
        "{inked} pixels differ from the blurred capture"
    );
    assert!(same > 60_000);
    // On screen, the portable canvas lays the same patches (Direct2D: `scene_tests`).
    let mut capture = Capture::new(image.clone());
    capture.region = Some(crop);
    capture.selecting = false;
    capture.editor = Editor::restore(objects.clone(), 4, Vec::new());
    let mut raster = Raster::new();
    let mut frame = shotoshot_raster::pixmap_from_bgra(&capture.image).unwrap();
    capture
        .draw_underlay(&mut raster.canvas(&mut frame))
        .unwrap();
    let screen = shotoshot_raster::image_from_pixmap(&frame, 0, 0);
    assert_eq!(screen.pixels, shown.pixels, "under the annotations");
    // And the arrow over them stays sharp: its red is the annotation's own.
    let red = native
        .pixels
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|p| p[..3] == [92, 79, 237])
        .count();
    assert!(red > 300, "the arrow is not blurred ({red} pure pixels)");
}

#[test]
fn portable_renderer_paints_the_whole_capture_overlay() {
    use shotoshot_ui::{Canvas, Capture};
    let _com = ComApartment::new().unwrap();
    let mut backdrop = Image::solid(900, 560, [60, 52, 46, 255]);
    for y in 80..440usize {
        for x in 60..760usize {
            let i = (y * 900 + x) * 4;
            backdrop.pixels[i..i + 4].copy_from_slice(&[248, 245, 242, 255]);
        }
    }
    let mut capture = Capture::new(backdrop);
    capture.region = Rect::new(Point { x: 60., y: 80. }, Point { x: 760., y: 440. });
    capture.selecting = false;
    capture
        .editor
        .add_text(Point { x: 100., y: 380. }, "Portable text".into());
    capture.pointer = Point { x: 400., y: 300. };
    capture.width_preview = true;
    let mut raster = Raster::new();
    let mut frame = shotoshot_raster::pixmap_from_bgra(&capture.image).unwrap();
    {
        let mut canvas = raster.canvas(&mut frame);
        capture.draw_underlay(&mut canvas).unwrap();
        canvas.annotations(capture.editor.objects(), None).unwrap();
        capture.draw_frame(&mut canvas, true).unwrap();
        let color = capture.editor.style.color;
        let now = std::time::Instant::now();
        capture.draw_toolbar(&mut canvas, color, None, now).unwrap();
        capture.draw_width_ring(&mut canvas).unwrap();
    }
    let image = shotoshot_raster::image_from_pixmap(&frame, 0, 0);
    let target = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target");
    save_png(&image, &target.join("shotoshot-portable-interface.png")).unwrap();
    // Every toolbar button shows an icon that stands out from its cell: light over the dark
    // cells, dark over the chosen one's blue.
    for button in capture.buttons() {
        let r = button.bounds;
        let pixel = |x: usize, y: usize| {
            let i = (y * 900 + x) * 4;
            [image.pixels[i], image.pixels[i + 1], image.pixels[i + 2]]
        };
        // Left of the 20 px glyph grid, which starts 5 px in.
        let cell = pixel(r.min.x as usize + 2, r.center().y as usize);
        let lit = (r.min.y as usize..r.max.y as usize)
            .flat_map(|y| (r.min.x as usize..r.max.x as usize).map(move |x| (x, y)))
            .filter(|(x, y)| {
                let p = pixel(*x, *y);
                // Disabled icons are grey 100 over cells of 32.
                (0..3).any(|c| p[c].abs_diff(cell[c]) > 60)
            })
            .count();
        assert!(lit > 8, "{:?} has no visible icon", button.label);
    }
    // The committed text is drawn in the annotation colour inside the region.
    let red = (380..420usize)
        .flat_map(|y| (100..260usize).map(move |x| (x, y)))
        .filter(|(x, y)| {
            let i = (y * 900 + x) * 4;
            image.pixels[i + 2] > 180 && image.pixels[i + 1] < 120
        })
        .count();
    assert!(red > 40, "annotation text missing ({red} pixels)");
}

#[test]
fn hover_hints_render_with_shared_style_only_on_the_pointer_monitor() {
    use shotoshot_ui::{Capture, toolbar::HoverHint};
    let _com = ComApartment::new().unwrap();
    let mut capture = Capture::new(Image::solid(1200, 600, [0, 0, 0, 255]));
    capture.selecting = false;
    capture.region = Rect::new(Point { x: 40., y: 100. }, Point { x: 490., y: 450. });
    capture.monitors = (0..3)
        .map(|i| {
            Rect {
                min: Point {
                    x: i as f64 * 400.,
                    y: 0.,
                },
                max: Point {
                    x: (i + 1) as f64 * 400.,
                    y: 600.,
                },
            }
            .into()
        })
        .collect();
    let badge = capture.dimensions_bounds().unwrap();
    let screen_button = capture.screen_button_bounds().unwrap();
    let toolbar_button = capture.buttons()[0].bounds.center();
    let now = std::time::Instant::now();
    let shown = now + std::time::Duration::from_millis(250);
    let mut raster = Raster::new();
    for pointer in [
        Point {
            x: 385.,
            y: badge.center().y,
        },
        Point {
            x: 415.,
            y: badge.center().y,
        },
        screen_button.center(),
        toolbar_button,
    ] {
        capture.pointer = pointer;
        capture.hover_hint = HoverHint::default();
        capture
            .hover_hint
            .update(capture.hover_target(pointer), now);
        let monitor = capture
            .monitors
            .iter()
            .map(|m| m.bounds)
            .find(|m| m.contains(pointer))
            .unwrap();
        for (time, editing, picking) in [
            (now, false, false),
            (shown, false, false),
            (shown, true, false),
            (shown, false, true),
        ] {
            capture.picker = picking.then(|| {
                shotoshot_ui::color_picker::ColorPicker::new([255, 0, 0], pointer, 1200, 600, 1.)
            });
            let mut frame = shotoshot_raster::pixmap_from_bgra(&capture.image).unwrap();
            capture
                .draw_hover_hint(&mut raster.canvas(&mut frame), time, editing)
                .unwrap();
            let image = shotoshot_raster::image_from_pixmap(&frame, 0, 0);
            if time == shown && !editing && !picking && pointer.x == 385. {
                let target = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../target/tooltip-proof.png");
                save_png(&image, &target).unwrap();
            }
            let mut dark_background = 0;
            let mut light_text = 0;
            for (i, pixel) in image.pixels.as_chunks::<4>().0.iter().enumerate() {
                if pixel[..3] != [0, 0, 0] {
                    let point = Point {
                        x: (i % 1200) as f64,
                        y: (i / 1200) as f64,
                    };
                    assert!(
                        monitor.contains(point),
                        "hint escaped pointer monitor: {point:?}"
                    );
                }
                dark_background += usize::from(pixel[..3] == [32, 32, 32]);
                // Small antialiased glyphs need not contain a fully opaque pixel.
                light_text += usize::from(pixel[..3].iter().all(|c| *c > 100));
            }
            let hidden = time == now || picking || (editing && badge.contains(pointer));
            if hidden {
                assert_eq!((dark_background, light_text), (0, 0));
            } else {
                assert!(
                    dark_background > 100 && light_text > 10,
                    "pointer={pointer:?}, target={:?}, editing={editing}, background={dark_background}, text={light_text}",
                    capture.hover_target(pointer)
                );
            }
        }
        assert!(
            capture.hover_target(pointer).is_none(),
            "palette consumes pointer input"
        );
        capture.picker = None;
    }
}

/// Bounds of the pixels darker than mid grey.
fn ink_bounds(image: &Image) -> [u32; 4] {
    let (mut left, mut top, mut right, mut bottom) = (u32::MAX, u32::MAX, 0, 0);
    for y in 0..image.height {
        for x in 0..image.width {
            if image.pixels[((y * image.width + x) * 4) as usize] < 128 {
                (left, top) = (left.min(x), top.min(y));
                (right, bottom) = (right.max(x), bottom.max(y));
            }
        }
    }
    [left, top, right, bottom]
}

#[test]
fn both_renderers_set_annotation_text_in_the_same_embedded_face() {
    let _com = ComApartment::new().unwrap();
    let native = Renderer::new().unwrap();
    let mut portable = Raster::new();
    // Two lines: the line height is part of the face.
    let (text, size) = ("Vérifier ce bouton\nÀ corriger", 32.);
    let extent = native.measure_annotation_text(text, size).unwrap();
    let other = portable.measure_annotation_text(text, size as f64);
    assert!(
        (extent.x - other.x).abs() < 1.5 && (extent.y - other.y).abs() < 1.5,
        "Direct2D {extent:?}, portable {other:?}"
    );
    // Its line height sets it apart from the interface face it would silently fall back to.
    let interface = native.measure_text(text, size).unwrap();
    assert!(
        (extent.y - interface.y).abs() > 3.,
        "annotation text fell back"
    );

    let image = Image::solid(420, 140, [255; 4]);
    let crop = Rect::new(Point::default(), Point { x: 420., y: 140. }).unwrap();
    let object = Object {
        rotation: 0.,
        fill: Vec::new(),
        id: 1,
        style: Style {
            color: [0, 0, 0],
            width: 2.,
        },
        shape: Shape::Text {
            position: Point { x: 24., y: 20. },
            text: text.into(),
            size: size as f64,
            extent,
            label: None,
            ink: None,
        },
    };
    let objects = std::slice::from_ref(&object);
    let direct = ink_bounds(&native.export(&image, &[], objects, crop).unwrap());
    let raster = ink_bounds(&portable.export(&image, &[], objects, crop).unwrap());
    for (a, b) in direct.into_iter().zip(raster) {
        assert!(
            a.abs_diff(b) <= 2,
            "ink bounds {direct:?} against {raster:?}"
        );
    }
}

#[test]
fn both_renderers_set_a_label_on_the_same_panel_and_lines() {
    let _com = ComApartment::new().unwrap();
    let native = Renderer::new().unwrap();
    let mut portable = Raster::new();
    // Words of clearly different widths: both line breakers must cut after "panel".
    let (text, size, width) = ("A white panel with dark ink", 24., 170.);
    let extent = native
        .measure_wrapped_annotation_text(text, size, width)
        .unwrap();
    let other = portable.measure_wrapped_annotation_text(text, size as f64, width as f64);
    assert!(
        (extent.y - other.y).abs() < 1.5,
        "Direct2D {extent:?}, portable {other:?}: not the same lines"
    );
    let image = Image::solid(420, 200, [0, 0, 0, 255]);
    let crop = Rect::new(Point::default(), Point { x: 420., y: 200. }).unwrap();
    let object = Object {
        rotation: 0.,
        fill: Vec::new(),
        id: 1,
        style: Style {
            color: [255, 255, 255],
            width: 2.,
        },
        shape: Shape::Text {
            position: Point { x: 40., y: 40. },
            text: text.into(),
            size: size as f64,
            extent,
            label: Some(Label::Wrap(width as f64)),
            ink: None,
        },
    };
    let objects = std::slice::from_ref(&object);
    let direct = native.export(&image, &[], objects, crop).unwrap();
    let raster = portable.export(&image, &[], objects, crop).unwrap();
    // The white panel: its bounds are those of the pixels brighter than mid grey.
    let panel = |image: &Image| {
        let (mut left, mut top, mut right, mut bottom) = (u32::MAX, u32::MAX, 0, 0);
        for y in 0..image.height {
            for x in 0..image.width {
                if image.pixels[((y * image.width + x) * 4) as usize] >= 128 {
                    (left, top) = (left.min(x), top.min(y));
                    (right, bottom) = (right.max(x), bottom.max(y));
                }
            }
        }
        [left, top, right, bottom]
    };
    for (a, b) in panel(&direct).into_iter().zip(panel(&raster)) {
        assert!(
            a.abs_diff(b) <= 1,
            "panels {:?} against {:?}",
            panel(&direct),
            panel(&raster)
        );
    }
    // The dark ink inside it, set on the same lines and centred alike.
    let inside = |image: &Image| {
        let [l, t, r, b] = panel(image);
        let (mut left, mut top, mut right, mut bottom) = (u32::MAX, u32::MAX, 0, 0);
        for y in t + 4..b - 4 {
            for x in l + 4..r - 4 {
                if image.pixels[((y * image.width + x) * 4) as usize] < 128 {
                    (left, top) = (left.min(x), top.min(y));
                    (right, bottom) = (right.max(x), bottom.max(y));
                }
            }
        }
        [left, top, right, bottom]
    };
    for (a, b) in inside(&direct).into_iter().zip(inside(&raster)) {
        assert!(
            a.abs_diff(b) <= 2,
            "ink {:?} against {:?}",
            inside(&direct),
            inside(&raster)
        );
    }
    // An ink of its own: both renderers set the letters in it, green in any channel order.
    let mut inked = object.clone();
    if let Shape::Text { ink, .. } = &mut inked.shape {
        *ink = Some([0, 200, 0]);
    }
    let green = |image: &Image| {
        image
            .pixels
            .as_chunks::<4>()
            .0
            .iter()
            .any(|p| p[1] > 150 && p[0] < 80 && p[2] < 80)
    };
    assert!(!green(&direct), "no green before");
    let objects = std::slice::from_ref(&inked);
    assert!(green(&native.export(&image, &[], objects, crop).unwrap()));
    assert!(green(&portable.export(&image, &[], objects, crop).unwrap()));
}
#[test]
fn both_renderers_draw_an_empty_box_at_its_drawn_size() {
    let _com = ComApartment::new().unwrap();
    let native = Renderer::new().unwrap();
    let mut portable = Raster::new();
    let (size, width) = (24., 150.);
    let line = native
        .measure_wrapped_annotation_text("", size, width)
        .unwrap();
    let other = portable.measure_wrapped_annotation_text("", size as f64, width as f64);
    assert!(
        (line.y - other.y).abs() < 1.5,
        "an empty line: Direct2D {line:?}, portable {other:?}"
    );
    let label = Some(Label::Wrap(width as f64));
    let object = Object {
        rotation: 0.,
        fill: Vec::new(),
        id: 1,
        style: Style {
            color: [255, 255, 255],
            width: 2.,
        },
        shape: Shape::Text {
            position: Point { x: 40., y: 40. },
            text: String::new(),
            size: size as f64,
            extent: kept_block("", label, line, size as f64, 80.),
            label,
            ink: None,
        },
    };
    let image = Image::solid(300, 200, [0, 0, 0, 255]);
    let crop = Rect::new(Point::default(), Point { x: 300., y: 200. }).unwrap();
    let objects = std::slice::from_ref(&object);
    let bright = |image: &Image| {
        let (mut left, mut top, mut right, mut bottom) = (u32::MAX, u32::MAX, 0, 0);
        for y in 0..image.height {
            for x in 0..image.width {
                if image.pixels[((y * image.width + x) * 4) as usize] >= 128 {
                    (left, top) = (left.min(x), top.min(y));
                    (right, bottom) = (right.max(x), bottom.max(y));
                }
            }
        }
        [left, top, right, bottom]
    };
    let direct = bright(&native.export(&image, &[], objects, crop).unwrap());
    let raster = bright(&portable.export(&image, &[], objects, crop).unwrap());
    // 24 px font: margins of 9.6 across and 6 down around the 150 by 80 block.
    let expected = [30, 34, 199, 125];
    for (got, want) in direct
        .into_iter()
        .chain(raster)
        .zip(expected.into_iter().cycle())
    {
        assert!(
            got.abs_diff(want) <= 1,
            "Direct2D {direct:?}, portable {raster:?}"
        );
    }
}
