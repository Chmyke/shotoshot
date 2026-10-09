#![cfg(windows)]
//! The portable renderer (Linux, macOS) must draw annotations like Direct2D does.
use shotoshot_app::{ComApartment, Image, Renderer, save_png};
use shotoshot_core::*;
use shotoshot_raster::Raster;

fn scene() -> Vec<Object> {
    let mut e = Editor::default();
    let mut draw = |tool: Tool, color: [u8; 3], width: f64, points: &[Point]| {
        e.tool = tool;
        e.style = Style { color, width };
        e.begin(points[0], Modifiers::default());
        for p in &points[1..] {
            e.update(*p);
        }
        e.finish();
    };
    let p = |x: f64, y: f64| Point { x, y };
    draw(
        Tool::Rectangle,
        [237, 79, 92],
        3.,
        &[p(30., 30.), p(210., 150.)],
    );
    draw(
        Tool::Ellipse,
        [80, 153, 244],
        5.,
        &[p(330., 90.), p(400., 90.)],
    );
    draw(
        Tool::Arrow,
        [40, 60, 80],
        4.,
        &[p(60., 260.), p(300., 190.)],
    );
    draw(
        Tool::Line,
        [64, 199, 128],
        6.,
        &[p(60., 320.), p(420., 300.)],
    );
    draw(
        Tool::Pen,
        [28, 31, 40],
        8.,
        &[p(250., 40.), p(280., 70.), p(270., 120.), p(320., 150.)],
    );
    draw(
        Tool::Highlight,
        [250, 192, 66],
        6.,
        &[p(120., 380.), p(260., 385.), p(430., 370.)],
    );
    e.objects().to_vec()
}

#[test]
fn both_renderers_keep_fractional_arrow_joins_solid_and_tails_round() {
    let _com = ComApartment::new().unwrap();
    let image = Image::solid(240, 160, [255; 4]);
    let crop = Rect::new(Point::default(), Point { x: 240., y: 160. }).unwrap();
    let objects = [Object {
        id: 1,
        rotation: 0.,
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
        },
    }];
    for output in [
        Renderer::new()
            .unwrap()
            .export(&image, &objects, crop)
            .unwrap(),
        Raster::new().export(&image, &objects, crop).unwrap(),
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
    let native = Renderer::new()
        .unwrap()
        .export(&image, &objects, crop)
        .unwrap();
    let portable = Raster::new().export(&image, &objects, crop).unwrap();
    let target = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target");
    save_png(&native, &target.join("shotoshot-parity-direct2d.png")).unwrap();
    save_png(&portable, &target.join("shotoshot-parity-portable.png")).unwrap();
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
    assert!(mean < 1.5, "mean channel difference {mean:.2}");
    assert!(
        (far as f64 / pixels as f64) < 0.003,
        "{far} of {pixels} pixels differ strongly"
    );
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
        capture.draw_shade(&mut canvas).unwrap();
        canvas.annotations(capture.editor.objects(), None).unwrap();
        capture.draw_frame(&mut canvas, true).unwrap();
        let color = capture.editor.style.color;
        capture.draw_toolbar(&mut canvas, color).unwrap();
        capture.draw_width_ring(&mut canvas).unwrap();
    }
    let image = shotoshot_raster::image_from_pixmap(&frame, 0, 0);
    let target = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target");
    save_png(&image, &target.join("shotoshot-portable-interface.png")).unwrap();
    // Every toolbar button shows a light icon over its dark cell.
    for button in capture.buttons() {
        let r = button.bounds;
        let lit = (r.min.y as usize..r.max.y as usize)
            .flat_map(|y| (r.min.x as usize..r.max.x as usize).map(move |x| (x, y)))
            .filter(|(x, y)| {
                let i = (y * 900 + x) * 4;
                let p = &image.pixels[i..i + 3];
                // Cells are rgb(32..55); disabled icons are grey 100, enabled ones light.
                p.iter().any(|c| *c > 80)
            })
            .count();
        assert!(lit > 8, "{} has no visible icon", button.label);
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
        .map(|i| Rect {
            min: Point {
                x: i as f64 * 400.,
                y: 0.,
            },
            max: Point {
                x: (i + 1) as f64 * 400.,
                y: 600.,
            },
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
            .find(|m| m.contains(pointer))
            .unwrap();
        for (time, editing, picking) in [
            (now, false, false),
            (shown, false, false),
            (shown, true, false),
            (shown, false, true),
        ] {
            capture.picker = picking.then(|| {
                shotoshot_ui::color_picker::ColorPicker::new([255, 0, 0], pointer, 1200, 600)
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
