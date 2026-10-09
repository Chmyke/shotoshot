#![cfg(windows)]
use shotoshot_app::{
    ComApartment, Image, ImageExt, Renderer, load_png, save_image, save_numbered, save_png,
};
use shotoshot_core::{settings::SaveFormat, *};

#[test]
fn numbered_save_never_overwrites_an_existing_capture() {
    let _com = ComApartment::new().unwrap();
    let folder =
        std::env::temp_dir().join(format!("shotoshot-numbered-export-{}", std::process::id()));
    std::fs::create_dir_all(&folder).unwrap();
    let original = folder.join("Capture 001.png");
    std::fs::write(&original, b"previous screenshot").unwrap();
    let image = Image::solid(2, 2, [10, 20, 30, 255]);
    let second = save_numbered(&image, &original, SaveFormat::Png).unwrap();
    assert_eq!(second, folder.join("Shot 002.png"));
    let third = save_numbered(&image, &original, SaveFormat::Png).unwrap();
    assert_eq!(third, folder.join("Shot 003.png"));
    assert_eq!(std::fs::read(&original).unwrap(), b"previous screenshot");
    assert_eq!(load_png(&second).unwrap().pixels, image.pixels);
    assert_eq!(load_png(&third).unwrap().pixels, image.pixels);
    // A JPEG taken by another editor meanwhile is kept, and the retry stays a JPEG.
    let taken = folder.join("Shot 004.jpg");
    std::fs::write(&taken, b"previous jpeg").unwrap();
    let fifth = save_numbered(&image, &taken, SaveFormat::Jpeg).unwrap();
    assert_eq!(fifth, folder.join("Shot 005.jpg"));
    assert_eq!(std::fs::read(&taken).unwrap(), b"previous jpeg");
    assert_eq!(&std::fs::read(&fifth).unwrap()[..3], &[0xFF, 0xD8, 0xFF]);
    std::fs::remove_dir_all(folder).unwrap();
}

#[test]
fn jpeg_keeps_one_pixel_colored_lines_sharp() {
    let _com = ComApartment::new().unwrap();
    // Saturated one-pixel lines on white, as annotations are: a lower quality or a halved
    // color resolution, the encoder's default, smears them.
    let mut image = Image::solid(64, 48, [255, 255, 255, 255]);
    for (index, pixel) in image.pixels.as_chunks_mut::<4>().0.iter_mut().enumerate() {
        let (x, y) = (index % 64, index / 64);
        if x % 7 == 3 {
            *pixel = [0, 0, 255, 255];
        } else if y % 5 == 2 {
            *pixel = [255, 0, 0, 255];
        }
    }
    let file = std::env::temp_dir().join(format!("shotoshot-test-{}-é.jpg", std::process::id()));
    save_image(&image, &file, SaveFormat::Jpeg).unwrap();
    let signature = std::fs::read(&file).unwrap()[..3].to_vec();
    let decoded = load_png(&file).unwrap();
    std::fs::remove_file(file).unwrap();
    assert_eq!(signature, [0xFF, 0xD8, 0xFF]);
    assert_eq!((decoded.width, decoded.height), (64, 48));
    let worst = image
        .pixels
        .iter()
        .zip(&decoded.pixels)
        .map(|(expected, actual)| expected.abs_diff(*actual))
        .max()
        .unwrap();
    assert!(worst <= 4, "a channel moved by {worst}");
}

#[test]
fn render_and_png_roundtrip_preserve_pixels_and_crop() {
    let _com = ComApartment::new().unwrap();
    let image = Image::solid(320, 200, [240, 240, 240, 255]);
    let mut editor = Editor::default();
    editor.tool = Tool::Rectangle;
    editor.begin(Point { x: 20., y: 20. }, Modifiers::default());
    editor.update(Point { x: 140., y: 100. });
    editor.finish();
    let renderer = Renderer::new().unwrap();
    let crop = Rect::new(Point { x: 10., y: 10. }, Point { x: 200., y: 150. }).unwrap();
    let output = renderer
        .export(&image, None, editor.objects(), crop)
        .unwrap();
    assert_eq!((output.width, output.height), (190, 140));
    assert_eq!(&output.pixels[..4], &[240, 240, 240, 255]);
    assert!(
        output
            .pixels
            .as_chunks::<4>()
            .0
            .iter()
            .any(|p| p[2] > 200 && p[1] < 120)
    );
    let file = std::env::temp_dir().join(format!("shotoshot-test-{}-é.png", std::process::id()));
    save_png(&output, &file).unwrap();
    let decoded = load_png(&file).unwrap();
    std::fs::remove_file(file).unwrap();
    assert_eq!(decoded.pixels, output.pixels);
}

#[test]
fn unicode_text_measurement_matches_layout_and_undo_geometry() {
    let _com = ComApartment::new().unwrap();
    let renderer = Renderer::new().unwrap();
    let extent = renderer
        .measure_annotation_text("WWWW\n日本語 🎮", 24.)
        .unwrap();
    assert!(extent.x > 80.);
    assert!(extent.y > 48.);
    let mut editor = Editor::default();
    editor.add_text_measured(Point { x: 0., y: 0. }, "WWWW\n日本語 🎮".into(), extent);
    assert_eq!(editor.objects()[0].bounds().size(), extent);
    editor.undo();
    editor.redo();
    assert_eq!(editor.objects()[0].bounds().size(), extent);
}

#[test]
fn inconsistent_image_dimensions_are_rejected_before_ffi() {
    let invalid = Image {
        width: 100,
        height: 100,
        pixels: vec![0; 4],
        left: 0,
        top: 0,
    };
    assert!(invalid.validate().is_err());
    assert!(
        Image {
            width: 0,
            height: 5,
            pixels: vec![],
            left: 0,
            top: 0
        }
        .validate()
        .is_err()
    );
}

#[test]
fn thick_pen_has_round_caps_and_no_join_spikes() {
    let _com = ComApartment::new().unwrap();
    let image = Image::solid(260, 180, [25, 25, 25, 255]);
    let points = vec![
        Point { x: 40., y: 100. },
        Point { x: 100., y: 100. },
        Point { x: 60., y: 103. },
        Point { x: 130., y: 60. },
        Point { x: 170., y: 100. },
    ];
    let object = Object {
        rotation: 0.,
        fill: None,
        id: 1,
        shape: Shape::Stroke {
            widths: Vec::new(),
            points: points.clone(),
            highlight: false,
        },
        style: Style {
            width: 40.,
            color: [237, 79, 92],
        },
    };
    let output = Renderer::new()
        .unwrap()
        .export(
            &image,
            None,
            &[object],
            Rect {
                min: Point::default(),
                max: Point { x: 260., y: 180. },
            },
        )
        .unwrap();
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/shotoshot-thick-pen.png");
    save_png(&output, &path).unwrap();
    for (i, pixel) in output.pixels.as_chunks::<4>().0.iter().enumerate() {
        if pixel[2] < 100 {
            continue;
        }
        let p = Point {
            x: (i % 260) as f64 + 0.5,
            y: (i / 260) as f64 + 0.5,
        };
        let distance = points
            .windows(2)
            .map(|w| segment_distance(p, w[0], w[1]))
            .fold(f64::INFINITY, f64::min);
        assert!(distance <= 21., "join protrudes at {p:?}: {distance}");
    }
    assert!(
        output.pixels[(100 * 260 + 25) * 4 + 2] > 200,
        "round start cap missing"
    );
}

#[test]
fn one_pixel_arrows_render_while_drawing_in_every_direction() {
    let _com = ComApartment::new().unwrap();
    let renderer = Renderer::new().unwrap();
    let image = Image::solid(260, 260, [25, 25, 25, 255]);
    for end in [
        Point { x: 220., y: 40. },
        Point { x: 40., y: 220. },
        Point { x: 220., y: 220. },
        Point { x: 43., y: 43. },
    ] {
        let mut editor = Editor::default();
        editor.tool = Tool::Arrow;
        editor.style.width = 1.;
        editor.begin(Point { x: 40., y: 40. }, Modifiers::default());
        editor.update(end);
        let out = renderer
            .export(
                &image,
                None,
                editor.objects(),
                Rect {
                    min: Point::default(),
                    max: Point { x: 260., y: 260. },
                },
            )
            .unwrap();
        assert!(
            out.pixels
                .as_chunks::<4>()
                .0
                .iter()
                .filter(|p| p[2] > 60)
                .count()
                >= 3,
            "invisible arrow {end:?}"
        );
    }
}

#[test]
fn variable_highlighter_keeps_one_opacity_at_width_boundary() {
    let _com = ComApartment::new().unwrap();
    let mut e = Editor::default();
    e.tool = Tool::Highlight;
    e.style.width = 4.;
    e.begin(Point { x: 30., y: 100. }, Modifiers::default());
    e.update(Point { x: 100., y: 100. });
    e.style.width = 8.;
    e.apply_style();
    e.update(Point { x: 180., y: 100. });
    e.finish();
    let image = Renderer::new()
        .unwrap()
        .export(
            &Image::solid(220, 180, [255, 255, 255, 255]),
            None,
            e.objects(),
            Rect {
                min: Point::default(),
                max: Point { x: 220., y: 180. },
            },
        )
        .unwrap();
    let sample = |x: usize, y: usize| &image.pixels[(y * 220 + x) * 4..(y * 220 + x) * 4 + 3];
    assert_eq!(
        sample(60, 100),
        sample(100, 100),
        "overlap doubled marker opacity"
    );
    assert_eq!(sample(60, 100), sample(150, 100));
    assert_eq!(sample(60, 115), [255, 255, 255]);
    assert_ne!(sample(150, 115), [255, 255, 255]);
}
