use super::*;
use shotoshot_ui::{Capture, TextField};

fn blank() -> Pixmap {
    pixmap_from_bgra(&Image::solid(400, 200, [255, 255, 255, 255])).unwrap()
}
fn inked(p: &Pixmap, r: std::ops::Range<u32>, rows: std::ops::Range<u32>) -> usize {
    rows.flat_map(|y| r.clone().map(move |x| (x, y)))
        .filter(|&(x, y)| {
            p.pixel(x, y)
                .is_some_and(|c| c.red() < 200 || c.green() < 200)
        })
        .count()
}

const SLATE: [u8; 3] = [28, 31, 40];
const GREEN: [u8; 3] = [64, 199, 128];
const BLUE: [u8; 3] = [80, 153, 244];
const PAPER: [u8; 3] = [255, 255, 255];
fn at(x: f64, y: f64) -> Point {
    Point { x, y }
}
fn rgb(image: &Image, x: usize, y: usize) -> [u8; 3] {
    let i = (y * image.width as usize + x) * 4;
    [image.pixels[i + 2], image.pixels[i + 1], image.pixels[i]]
}
/// A filled rectangle, a filled circle and a hollow rectangle, outlined in red.
fn filled_scene() -> shotoshot_core::Editor {
    use shotoshot_core::{Modifiers, Tool};
    let mut e = shotoshot_core::Editor::default();
    for (tool, from, to) in [
        (Tool::Rectangle, at(40., 40.), at(140., 120.)),
        (Tool::Ellipse, at(260., 80.), at(310., 80.)),
        (Tool::Rectangle, at(330., 40.), at(390., 120.)),
    ] {
        e.tool = tool;
        e.begin(from, Modifiers::default());
        e.update(to);
        e.finish();
    }
    e.style.color = SLATE;
    e.fill_at(at(90., 80.), false);
    e.style.color = GREEN;
    e.fill_at(at(260., 80.), false);
    e
}

#[test]
fn export_fills_closed_shapes_under_their_outline() {
    let e = filled_scene();
    let image = Image::solid(400, 200, [255; 4]);
    let crop = Rect::new(at(0., 0.), at(400., 200.)).unwrap();
    let out = Raster::new()
        .export(&image, e.background(), e.objects(), crop)
        .unwrap();
    assert_eq!(rgb(&out, 90, 80), SLATE);
    assert_eq!(rgb(&out, 260, 80), GREEN);
    assert_eq!(
        rgb(&out, 214, 34),
        PAPER,
        "a circle is not filled to its box"
    );
    assert_eq!(rgb(&out, 360, 80), PAPER, "a hollow shape stays hollow");
    assert_eq!(
        rgb(&out, 40, 80),
        [237, 79, 92],
        "the outline lies over the fill"
    );
}
#[test]
fn export_puts_the_background_under_everything_inside_the_crop() {
    let mut e = filled_scene();
    e.style.color = BLUE;
    e.fill_at(at(5., 5.), false);
    assert_eq!(e.background().last().map(|c| c.color), Some(BLUE));
    let image = Image::solid(400, 200, [255; 4]);
    let crop = Rect::new(at(200., 0.), at(400., 200.)).unwrap();
    let out = Raster::new()
        .export(&image, e.background(), e.objects(), crop)
        .unwrap();
    assert_eq!((out.width, out.height), (200, 200));
    for (x, y) in [(0, 0), (199, 199), (14, 34)] {
        assert_eq!(rgb(&out, x, y), BLUE, "{x},{y}");
    }
    assert_eq!(rgb(&out, 60, 80), GREEN);
    assert_eq!(
        rgb(&out, 160, 80),
        PAPER,
        "the hollow rectangle drawn before spares its inside"
    );
}
#[test]
fn a_fill_spares_the_shape_standing_in_it_wherever_it_goes() {
    use shotoshot_core::{Modifiers, Tool};
    let mut e = shotoshot_core::Editor::default();
    e.tool = Tool::Rectangle;
    for (from, to) in [
        (at(40., 40.), at(240., 160.)),
        (at(100., 80.), at(160., 120.)),
    ] {
        e.begin(from, Modifiers::default());
        e.update(to);
        e.finish();
    }
    let inner = e.objects()[1].id;
    e.style.color = SLATE;
    e.fill_at(at(60., 60.), false);
    let image = Image::solid(400, 200, [255; 4]);
    let crop = Rect::new(at(0., 0.), at(400., 200.)).unwrap();
    let mut raster = Raster::new();
    let out = raster
        .export(&image, e.background(), e.objects(), crop)
        .unwrap();
    assert_eq!(rgb(&out, 60, 60), SLATE);
    assert_eq!(rgb(&out, 130, 100), PAPER);
    e.select_only(inner);
    e.nudge(at(60., 0.));
    let out = raster
        .export(&image, e.background(), e.objects(), crop)
        .unwrap();
    assert_eq!(rgb(&out, 130, 100), SLATE, "where it was");
    assert_eq!(rgb(&out, 190, 100), PAPER, "where it is");
}
#[test]
fn the_background_covers_the_region_on_screen_and_only_dims_around_it() {
    let mut capture = Capture::new(Image::solid(400, 200, [255; 4]));
    capture.selecting = false;
    capture.region = Rect::new(at(100., 50.), at(300., 150.));
    let mut raster = Raster::new();
    let mut frame = |capture: &Capture| {
        let mut frame = blank();
        capture
            .draw_underlay(&mut raster.canvas(&mut frame))
            .unwrap();
        image_from_pixmap(&frame, 0, 0)
    };
    let plain = frame(&capture);
    let dimmed = rgb(&plain, 20, 20);
    assert_eq!(rgb(&plain, 200, 100), PAPER);
    assert!(dimmed[0] > 100 && dimmed[0] < 160, "{dimmed:?}");

    capture.editor.style.color = SLATE;
    capture.editor.fill_at(at(200., 100.), false);
    let slate = frame(&capture);
    for (x, y) in [(200, 100), (100, 50), (299, 149)] {
        assert_eq!(rgb(&slate, x, y), SLATE, "{x},{y}");
    }
    for (x, y) in [(99, 100), (300, 100), (200, 49), (200, 150), (20, 20)] {
        assert_eq!(rgb(&slate, x, y), dimmed, "{x},{y}");
    }

    // The slate belongs to the capture: it follows the region.
    capture.region = Rect::new(at(150., 50.), at(350., 150.));
    let moved = frame(&capture);
    assert_eq!(rgb(&moved, 120, 100), dimmed);
    assert_eq!(rgb(&moved, 340, 100), SLATE);
}
#[test]
fn a_text_is_laid_out_once_however_often_it_is_measured_or_drawn() {
    let mut raster = Raster::new();
    let first = raster.measure_annotation_text("Shotoshot", 24.);
    assert_eq!(raster.layouts.len(), 1);
    for _ in 0..3 {
        assert_eq!(raster.measure_annotation_text("Shotoshot", 24.), first);
        let mut frame = blank();
        let r = Rect::new(at(10., 10.), at(390., 190.)).unwrap();
        raster
            .canvas(&mut frame)
            .annotation_text("Shotoshot", r, 24., Ink::solid([0; 3]))
            .unwrap();
    }
    assert_eq!(
        raster.layouts.len(),
        1,
        "measured and drawn from the same layout"
    );
    // Another size or face is another layout; the same text in both faces differs.
    raster.measure_annotation_text("Shotoshot", 25.);
    raster.measure_text("Shotoshot", 24.);
    assert_eq!(raster.layouts.len(), 3);
    assert_ne!(
        raster.measure_text("Shotoshot", 24.),
        first,
        "the interface face is not the annotation face"
    );
    // A full cache starts over instead of growing without bound.
    for i in 0..LAYOUTS_KEPT {
        raster.measure_text(&format!("line {i}"), 12.);
    }
    assert!(raster.layouts.len() <= LAYOUTS_KEPT);
    assert_eq!(raster.measure_annotation_text("Shotoshot", 24.), first);
}
#[test]
fn translucent_text_is_lighter_than_opaque_text() {
    let darkness = |alpha: f32| {
        let mut raster = Raster::new();
        let mut frame = blank();
        let r = Rect::new(at(10., 10.), at(390., 190.)).unwrap();
        raster
            .canvas(&mut frame)
            .text("Shotoshot", r, 24., Ink::with_alpha([0; 3], alpha))
            .unwrap();
        (0..400u32)
            .flat_map(|x| (0..200u32).map(move |y| (x, y)))
            .map(|(x, y)| 255 - frame.pixel(x, y).map_or(255, |c| c.red()) as u32)
            .sum::<u32>()
    };
    let (opaque, half) = (darkness(1.), darkness(0.5));
    assert!(opaque > 2000, "text missing ({opaque})");
    assert!(
        half * 10 > opaque * 4 && half * 10 < opaque * 6,
        "half-transparent text should be about half as dark: {half} of {opaque}"
    );
    assert_eq!(darkness(0.), 0, "invisible text leaves no mark");
}
#[test]
fn rotated_text_is_drawn_where_the_rotation_puts_it() {
    let mut raster = Raster::new();
    let mut frame = blank();
    let r = Rect::new(at(200., 100.), at(1e6, 1e6)).unwrap();
    {
        let mut canvas = raster.canvas(&mut frame);
        canvas
            .set_rotation(Some((at(200., 100.), std::f64::consts::PI)))
            .unwrap();
        canvas
            .annotation_text("Shotoshot", r, 24., Ink::solid([0; 3]))
            .unwrap();
        canvas.set_rotation(None).unwrap();
    }
    // Turned half a circle around its own corner, the text lies up and to the left.
    assert!(inked(&frame, 60..200, 60..100) > 60, "rotated text missing");
    assert_eq!(
        inked(&frame, 200..400, 100..200),
        0,
        "nothing where it would lie unrotated"
    );
}
#[test]
fn fills_with_holes_do_not_remember_each_other() {
    let mut raster = Raster::new();
    let mut frame = blank();
    let hole = |min: Point, max: Point| vec![min, at(max.x, min.y), max, at(min.x, max.y)];
    {
        let mut canvas = raster.canvas(&mut frame);
        let left = Rect::new(at(0., 0.), at(200., 200.)).unwrap();
        canvas
            .fill_rect_except(
                left,
                &[hole(at(50., 50.), at(100., 100.))],
                Ink::solid(SLATE),
            )
            .unwrap();
        // The second fill covers where the first one's hole was: it must be filled.
        let whole = Rect::new(at(0., 0.), at(400., 200.)).unwrap();
        canvas
            .fill_rect_except(
                whole,
                &[hole(at(300., 50.), at(350., 100.))],
                Ink::solid(GREEN),
            )
            .unwrap();
    }
    let image = image_from_pixmap(&frame, 0, 0);
    assert_eq!(
        rgb(&image, 75, 75),
        GREEN,
        "the first hole is no hole for the second fill"
    );
    assert_eq!(rgb(&image, 325, 75), PAPER, "the second hole is spared");
    assert_eq!(rgb(&image, 10, 10), GREEN);
}
#[test]
fn text_field_draws_text_selection_and_caret() {
    let capture = Capture::new(Image::solid(400, 200, [255; 4]));
    let mut field = TextField::open(&capture, Point { x: 40., y: 40. });
    field.insert("Shotoshot");
    let mut raster = Raster::new();
    let mut frame = blank();
    field
        .draw(&mut raster.canvas(&mut frame), true, 1.)
        .unwrap();
    let width = raster
        .canvas(&mut blank())
        .measure_annotation_text("Shotoshot", 24.)
        .unwrap()
        .x;
    assert!(
        inked(&frame, 40..40 + width as u32, 40..70) > 60,
        "text missing"
    );
    // The caret is a vertical line just after the text.
    let caret = 40 + width.round() as u32;
    assert!(
        inked(&frame, caret - 1..caret + 2, 45..65) >= 10,
        "caret missing"
    );
    let mut selected = blank();
    field.key(
        36,
        shotoshot_core::Modifiers {
            shift: true,
            ..Default::default()
        },
        &mut |s| Point {
            x: s.len() as f64,
            y: 1.,
        },
    );
    field
        .draw(&mut raster.canvas(&mut selected), true, 1.)
        .unwrap();
    let blue = |p: &Pixmap| {
        (40..40 + width as u32)
            .flat_map(|x| (42..68).map(move |y| (x, y)))
            .filter(|&(x, y)| {
                p.pixel(x, y)
                    .is_some_and(|c| c.blue() > 200 && c.red() < 230 && c.red() > 120)
            })
            .count()
    };
    assert!(blue(&selected) > blue(&frame) + 100, "selection missing");
}
#[test]
fn dimension_fields_draw_inside_their_boxes() {
    // A frame too narrow for the expression, which then scrolls inside its field.
    let mut capture = Capture::new(Image::solid(200, 300, [0, 0, 0, 255]));
    capture.selecting = false;
    capture.region = Rect::new(Point { x: 20., y: 70. }, Point { x: 190., y: 250. });
    let mut raster = Raster::new();
    let mut fields = shotoshot_ui::DimensionFields::open(&capture).unwrap();
    let size = fields.font_size();
    fields.insert("1000+200+30+4-1-1-1", &mut |s| raster.measure_text(s, size));
    // What the editor does before each frame.
    capture
        .measure_badge(Some(fields.texts()), |text, size| {
            Ok::<_, ()>(raster.measure_text(text, size).x)
        })
        .unwrap();
    fields.place(&capture, &mut |s| raster.measure_text(s, size));
    let mut frame = pixmap_from_bgra(&capture.image).unwrap();
    fields.draw(&mut raster.canvas(&mut frame), true).unwrap();
    let [width, height] = capture.dimension_field_bounds().unwrap();
    let bright = |x: std::ops::Range<u32>| {
        let rows = width.min.y as u32..width.max.y as u32;
        rows.flat_map(|y| x.clone().map(move |x| (x, y)))
            .filter(|&(x, y)| frame.pixel(x, y).is_some_and(|c| c.red() > 120))
            .count()
    };
    let span = |r: Rect| r.min.x as u32..r.max.x as u32;
    assert!(bright(span(width)) > 40, "width text missing");
    assert!(bright(span(height)) > 20, "height text missing");
    let between = width.max.x as u32 + 1..height.min.x as u32;
    assert_eq!(bright(between), 0, "long text must stay inside its field");
}
#[test]
fn notices_fit_their_text_near_the_top_of_the_screen() {
    let mut raster = Raster::new();
    let mut frame = blank();
    let mut canvas = raster.canvas(&mut frame);
    let message = "Could not save the capture";
    let size = shotoshot_ui::notice_size(&mut canvas, message, 1.).unwrap();
    let text = canvas.measure_text(message, 13.).unwrap();
    assert!(size.x > text.x + 20. && size.y > text.y + 12.);
    let screen = Rect {
        min: Point::default(),
        max: Point { x: 400., y: 200. },
    };
    let bounds = shotoshot_ui::top_center(screen, size, 1.);
    assert!((bounds.center().x - 200.).abs() < 1. && bounds.min.y == 24.);
    shotoshot_ui::draw_notice(&mut canvas, bounds, message, 1.).unwrap();
    let (x, y) = (bounds.min.x as u32, bounds.min.y as u32);
    assert!(inked(&frame, x..x + size.x as u32, y..y + size.y as u32) > 500);
    assert_eq!(inked(&frame, 0..400, 0..20), 0, "nothing above the box");
}
#[test]
fn rotated_text_field_draws_vertically() {
    let capture = Capture::new(Image::solid(400, 200, [255; 4]));
    let mut field = TextField::open(&capture, Point { x: 150., y: 90. });
    field.insert("Vertical");
    field.rotation = std::f64::consts::FRAC_PI_2;
    let mut raster = Raster::new();
    let mut frame = blank();
    field
        .draw(&mut raster.canvas(&mut frame), false, 1.)
        .unwrap();
    let tall = inked(&frame, 0..400, 0..200);
    let horizontal_band = inked(&frame, 0..400, 90..120);
    assert!(tall > 80);
    assert!(horizontal_band * 2 < tall, "text was not rotated");
}

/// Real text of 20 px, set in the annotation face, loses what makes it readable under a
/// blur of the default strength: the fine contrast of its strokes, and what tells two
/// different lines apart. Same measures as `shotoshot-core/tests/blur.rs`.
#[test]
fn twenty_pixel_text_is_unreadable_under_a_default_blur() {
    let (width, height) = (400u32, 60u32);
    let page = |line: &str| {
        let mut frame = pixmap_from_bgra(&Image::solid(width, height, [255; 4])).unwrap();
        let mut raster = Raster::new();
        let r = Rect::new(at(10., 15.), at(390., 50.)).unwrap();
        raster
            .canvas(&mut frame)
            .annotation_text(line, r, 20., Ink::solid([20, 20, 20]))
            .unwrap();
        image_from_pixmap(&frame, 0, 0).pixels
    };
    let luma = |pixels: &[u8]| -> Vec<f64> {
        let chunks = pixels.as_chunks::<4>().0;
        chunks.iter().map(|p| p[1] as f64).collect()
    };
    let (w, h) = (width as usize, height as usize);
    let detail = |l: &[f64]| -> f64 {
        let mut energy = 0.;
        for y in 2..h - 2 {
            for x in 2..w - 2 {
                let mean = (y - 2..=y + 2)
                    .flat_map(|yy| (x - 2..=x + 2).map(move |xx| (xx, yy)))
                    .map(|(xx, yy)| l[yy * w + xx])
                    .sum::<f64>()
                    / 25.;
                energy += (l[y * w + x] - mean).powi(2);
            }
        }
        energy
    };
    let zone = shotoshot_core::blur::Zone {
        x: 0,
        y: 0,
        width,
        height,
    };
    let blur = |pixels: &[u8]| {
        let radius = shotoshot_core::BLUR_RADIUS as u32;
        shotoshot_core::blur::patch(pixels, width, zone, radius, 1)
    };
    let (a, b) = (
        page("Invoice 4921 7730 jane@mail.com"),
        page("Account 8156 2047 marc@site.org"),
    );
    let (blur_a, blur_b) = (blur(&a), blur(&b));
    let kept = detail(&luma(&blur_a)) / detail(&luma(&a));
    assert!(
        kept < 0.01,
        "{kept:.4} of the strokes' fine contrast survives"
    );
    let difference = |x: &[u8], y: &[u8]| {
        luma(x)
            .iter()
            .zip(luma(y))
            .map(|(p, q)| (p - q).powi(2))
            .sum::<f64>()
    };
    let apart = difference(&blur_a, &blur_b) / difference(&a, &b);
    assert!(
        apart < 0.1,
        "{apart:.4} of what tells the lines apart survives"
    );
}

/// The Windows bound of every hint (`every_tooltip_fits`), measured by this renderer and
/// the fonts of the machine: on the Raspberry Pi, those a Linux user has.
#[test]
fn every_tooltip_fits_with_cosmic_text() {
    use shotoshot_ui::toolbar::{MAX_HINT_WIDTH, all_tooltips_in};
    let mut raster = Raster::new();
    for language in shotoshot_text::Language::ALL {
        for hint in all_tooltips_in(language) {
            let width = raster.measure_text(&hint, 12.).x;
            assert!(
                width <= MAX_HINT_WIDTH,
                "{}: `{hint}` is {width:.0} px wide, over {MAX_HINT_WIDTH} px",
                language.tag()
            );
        }
    }
}

/// The Windows bound of the loupe's hint (`magnifier_hint_fits_the_loupe`), measured by
/// this renderer: the loupe keeps its size in every language.
#[test]
fn magnifier_hint_fits_the_loupe_with_cosmic_text() {
    use shotoshot_ui::{MAGNIFIER_HINT_ROOM as ROOM, MAGNIFIER_HINT_SIZE, magnifier_hint_in};
    let mut raster = Raster::new();
    let too_long: Vec<String> = shotoshot_text::Language::ALL
        .into_iter()
        .filter_map(|language| {
            let hint = magnifier_hint_in(language);
            let measured = raster.measure_text(&hint, MAGNIFIER_HINT_SIZE);
            assert!(measured.x > 0., "{}: nothing measured", language.tag());
            (measured.x > ROOM.x || measured.y > ROOM.y).then(|| {
                let tag = language.tag();
                format!("{tag}: `{hint}` is {:.0}×{:.0} px", measured.x, measured.y)
            })
        })
        .collect();
    assert!(
        too_long.is_empty(),
        "the loupe holds {}×{} px, shorten:\n{}",
        ROOM.x,
        ROOM.y,
        too_long.join("\n")
    );
}

/// The Windows bound of the colour wheel's labels (`color_picker_labels_grow_from_minimum`),
/// measured by this renderer: each one fits the panel's width and its line.
#[test]
fn color_picker_labels_fit_with_cosmic_text() {
    use shotoshot_ui::color_picker::{
        HINT_HEIGHT, HINT_SIZE, MAX_LABEL_WIDTH, TITLE_HEIGHT, TITLE_SIZE, labels_in,
    };
    let mut raster = Raster::new();
    let mut overflows = Vec::new();
    for language in shotoshot_text::Language::ALL {
        let labels = labels_in(language);
        for (label, size, height) in [
            (&labels[0], TITLE_SIZE, TITLE_HEIGHT),
            (&labels[1], HINT_SIZE, HINT_HEIGHT),
        ] {
            let measured = raster.measure_text(label, size);
            assert!(measured.x > 0., "{}: nothing measured", language.tag());
            if measured.x > MAX_LABEL_WIDTH || measured.y > height {
                let tag = language.tag();
                overflows.push(format!(
                    "{tag}: `{label}` is {:.0}×{:.0} px",
                    measured.x, measured.y
                ));
            }
        }
    }
    assert!(
        overflows.is_empty(),
        "the panel holds {MAX_LABEL_WIDTH} px per line, shorten:\n{}",
        overflows.join("\n")
    );
}
