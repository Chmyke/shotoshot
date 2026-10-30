//! The blur tool: what its effect does to the capture, and how its objects behave in the
//! editor. No screen is read: every frame is painted here.
use shotoshot_core::blur::{Zone, box_radius, flatten, patch};
use shotoshot_core::{
    BLUR_RADIUS, BlurPatch, Editor, Modifiers, Object, Point, Rect, Shape, Style, Tool,
};

fn at(x: f64, y: f64) -> Point {
    Point { x, y }
}
fn zone(x: u32, y: u32, width: u32, height: u32) -> Zone {
    Zone {
        x,
        y,
        width,
        height,
    }
}

/// A white frame `width` × `height` with lines of dark pseudo-glyphs about 20 px tall: strokes
/// 2 px wide, letters 11 px apart, as small screen text is. BGRA.
fn text_frame(width: u32, height: u32, seed: u64) -> Vec<u8> {
    let mut pixels = [255u8; 4].repeat((width * height) as usize);
    let mut state = seed;
    let mut next = || {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (state >> 33) as u32
    };
    let mut ink = |x: u32, y: u32| {
        if x < width && y < height {
            let i = ((y * width + x) * 4) as usize;
            pixels[i..i + 3].copy_from_slice(&[30, 30, 30]);
        }
    };
    for line in (6..height.saturating_sub(24)).step_by(28) {
        for letter in (4..width.saturating_sub(12)).step_by(11) {
            if next() % 7 == 0 {
                continue; // a space
            }
            // Each letter: a few of eight strokes of a 7 × 14 cell, 2 px wide.
            let strokes = next();
            for stroke in 0..8 {
                if strokes >> stroke & 1 == 0 {
                    continue;
                }
                for t in 0..14 {
                    for w in 0..2 {
                        let (x, y) = match stroke {
                            0 => (letter + w, line + 3 + t),
                            1 => (letter + 6 + w, line + 3 + t),
                            2 => (letter + t / 2, line + 3 + w),
                            3 => (letter + t / 2, line + 10 + w),
                            4 => (letter + t / 2, line + 16 + w),
                            5 => (letter + 3 + w, line + 3 + t),
                            6 => (letter + t / 2, line + 3 + t),
                            _ => (letter + 7 - t / 2, line + 3 + t),
                        };
                        ink(x, y);
                    }
                }
            }
        }
    }
    pixels
}

/// Luminance of a BGRA buffer `width` wide, as floats.
fn luma(pixels: &[u8]) -> Vec<f64> {
    pixels
        .as_chunks::<4>()
        .0
        .iter()
        .map(|p| 0.114 * p[0] as f64 + 0.587 * p[1] as f64 + 0.299 * p[2] as f64)
        .collect()
}
/// The fine detail of an image: each pixel minus the mean of its 5 × 5 neighbourhood, the
/// scale at which strokes of small text differ from one another.
fn detail(l: &[f64], w: usize, h: usize) -> Vec<f64> {
    let mut out = vec![0.; l.len()];
    for y in 0..h {
        for x in 0..w {
            let (mut sum, mut n) = (0., 0.);
            for yy in y.saturating_sub(2)..(y + 3).min(h) {
                for xx in x.saturating_sub(2)..(x + 3).min(w) {
                    sum += l[yy * w + xx];
                    n += 1.;
                }
            }
            out[y * w + x] = l[y * w + x] - sum / n;
        }
    }
    out
}
fn energy(v: &[f64]) -> f64 {
    v.iter().map(|x| x * x).sum::<f64>() / v.len() as f64
}
fn correlation(a: &[f64], b: &[f64]) -> f64 {
    let dot: f64 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    dot / (energy(a) * energy(b)).sqrt() / a.len() as f64
}
/// The original's text region and the blurred patch over it, as luminance.
fn text_and_blur(radius: u32) -> (Vec<f64>, Vec<f64>, usize, usize) {
    let (width, height) = (320u32, 120u32);
    let pixels = text_frame(width, height, 42);
    let z = zone(0, 0, width, height);
    let blurred = patch(&pixels, width, z, radius, 9);
    (
        luma(&pixels),
        luma(&blurred),
        width as usize,
        height as usize,
    )
}

#[test]
fn text_of_twenty_pixels_loses_the_detail_that_makes_it_readable() {
    let (original, blurred, w, h) = text_and_blur(BLUR_RADIUS as u32);
    let (fine, kept) = (detail(&original, w, h), detail(&blurred, w, h));
    // The strokes' fine contrast is gone: what is left is a soft cloud.
    let ratio = energy(&kept) / energy(&fine);
    assert!(ratio < 0.01, "{ratio:.4} of the fine detail survives");
    // And what is left does not follow the strokes: no row nor column keeps their pattern.
    let r = correlation(&fine, &kept);
    assert!(r.abs() < 0.1, "fine detail still correlates at {r:.3}");
    // Nor does any row through the letters' bodies: their strokes left no trace along it.
    // (Rows along a line's top and bottom keep the line's edge, not its letters.)
    for y in (0..h).filter(|y| (12..=20).contains(&(y % 28))) {
        let row = |v: &[f64]| v[y * w..(y + 1) * w].to_vec();
        let r = correlation(&row(&fine), &row(&kept));
        // 0.21 at most on 2026-10-30.
        assert!(r.abs() < 0.3, "row {y} keeps the strokes' pattern: {r:.3}");
    }
    // The page's overall lightness stays: the zone reads as blurred text, not as a box.
    let mean = |v: &[f64]| v.iter().sum::<f64>() / v.len() as f64;
    assert!((mean(&original) - mean(&blurred)).abs() < 3.);
}

#[test]
fn two_texts_that_differ_look_alike_once_blurred() {
    // Two pages of different letters on the same lines: telling them apart once blurred
    // is all a reader could hope for, and almost nothing of their difference is left.
    let (width, height) = (320u32, 120u32);
    let z = zone(0, 0, width, height);
    let (a, b) = (text_frame(width, height, 42), text_frame(width, height, 43));
    let (blur_a, blur_b) = (patch(&a, width, z, 7, 9), patch(&b, width, z, 7, 9));
    let difference = |x: &[u8], y: &[u8]| {
        let (x, y) = (luma(x), luma(y));
        let d: Vec<f64> = x.iter().zip(&y).map(|(p, q)| p - q).collect();
        energy(&d)
    };
    let ratio = difference(&blur_a, &blur_b) / difference(&a, &b);
    // 0.066 on 2026-10-30: the shade of a whole word, not its letters.
    assert!(ratio < 0.1, "{ratio:.4} of the difference survives");
}

#[test]
fn a_light_blur_keeps_more_than_the_default_and_a_strong_one_less() {
    let survives = |radius| {
        let (original, blurred, w, h) = text_and_blur(radius);
        energy(&detail(&blurred, w, h)) / energy(&detail(&original, w, h))
    };
    let (light, default, strong) = (survives(3), survives(7), survives(20));
    assert!(
        light > default && default > strong,
        "{light} {default} {strong}"
    );
}

#[test]
fn the_same_blur_gives_the_same_pixels_and_another_seed_others() {
    let pixels = text_frame(200, 80, 3);
    let z = zone(10, 5, 150, 60);
    let a = patch(&pixels, 200, z, 7, 11);
    assert_eq!(a, patch(&pixels, 200, z, 7, 11), "deterministic");
    assert_eq!(a.len(), 150 * 60 * 4);
    assert!(a.as_chunks::<4>().0.iter().all(|p| p[3] == 255), "opaque");
    assert_ne!(a, patch(&pixels, 200, z, 7, 12), "the seed matters");
    assert_ne!(a, patch(&pixels, 200, z, 14, 11), "so does the strength");
    assert_eq!(box_radius(BLUR_RADIUS as u32), 3, "the board's blur of 3");
}

#[test]
fn a_patch_depends_on_its_zone_only_wherever_it_lies() {
    // The same 40 × 30 picture at two places of two different frames.
    let picture = text_frame(40, 30, 5);
    let mut first = [0u8, 0, 255, 255].repeat(100 * 60);
    let mut second = [255u8, 0, 0, 255].repeat(70 * 90);
    let paste = |frame: &mut [u8], width: usize, x: usize, y: usize| {
        for row in 0..30 {
            let to = ((y + row) * width + x) * 4;
            frame[to..to + 160].copy_from_slice(&picture[row * 160..][..160]);
        }
    };
    paste(&mut first, 100, 50, 20);
    paste(&mut second, 70, 3, 44);
    assert_eq!(
        patch(&first, 100, zone(50, 20, 40, 30), 7, 1),
        patch(&second, 70, zone(3, 44, 40, 30), 7, 1),
        "nothing around the zone leaks in"
    );
}

fn blur_object(id: u64, r: Rect, radius: f64) -> Object {
    Object {
        rotation: 0.,
        id,
        shape: Shape::Blur(r),
        style: Style {
            color: [0; 3],
            width: radius,
        },
        fill: Vec::new(),
    }
}

#[test]
fn flattening_lays_each_patch_from_the_untouched_capture() {
    let (width, height) = (120u32, 90u32);
    let source = text_frame(width, height, 8);
    let a = blur_object(1, Rect::new(at(10., 10.), at(70., 60.)).unwrap(), 7.);
    let b = blur_object(2, Rect::new(at(40., 30.), at(110., 80.)).unwrap(), 4.);
    let mut flat = source.clone();
    assert!(flatten(&mut flat, width, height, &[a.clone(), b.clone()]));
    let expected = |blur: &Object| {
        BlurPatch::of(blur, width, height)
            .unwrap()
            .pixels(&source, width)
    };
    let row = |pixels: &[u8], z: Zone, line: u32| {
        let at = (((z.y + line) * width + z.x) * 4) as usize;
        pixels[at..at + z.width as usize * 4].to_vec()
    };
    let zb = BlurPatch::of(&b, width, height).unwrap().zone;
    let pb = expected(&b);
    for line in 0..zb.height {
        let span = zb.width as usize * 4;
        assert_eq!(
            row(&flat, zb, line),
            pb[line as usize * span..][..span],
            "the upper blur shows whole"
        );
    }
    // The lower one shows where the upper does not cover it, computed from the capture.
    let za = BlurPatch::of(&a, width, height).unwrap().zone;
    let pa = expected(&a);
    let span = za.width as usize * 4;
    assert_eq!(row(&flat, za, 0), pa[..span]);
    // Outside both, the capture is untouched.
    assert_eq!(&flat[..40], &source[..40]);
}

fn drag(e: &mut Editor, tool: Tool, from: Point, to: Point) -> u64 {
    e.tool = tool;
    e.begin(from, Modifiers::default());
    e.update(to);
    e.finish();
    e.objects().last().unwrap().id
}

#[test]
fn a_drag_of_the_blur_tool_sets_down_a_zone_at_the_blur_strength() {
    let mut e = Editor::default();
    e.style.width = 3.;
    e.blur_radius = 10.;
    let id = drag(&mut e, Tool::Blur, at(80., 60.), at(20., 10.));
    let o = &e.objects()[0];
    assert_eq!(o.id, id);
    assert_eq!(
        o.shape,
        Shape::Blur(Rect::new(at(20., 10.), at(80., 60.)).unwrap())
    );
    assert_eq!(o.style.width, 10., "the blur strength, not the pen's width");
    assert_eq!(e.style.width, 3.);
    // A click sets nothing down.
    e.tool = Tool::Blur;
    e.begin(at(200., 200.), Modifiers::default());
    e.finish();
    assert_eq!(e.objects().len(), 1);
    e.undo();
    assert!(e.objects().is_empty());
    e.redo();
    assert_eq!(e.objects().len(), 1);
}

#[test]
fn a_blur_moves_resizes_and_is_selected_by_its_inside_but_never_turns() {
    let mut e = Editor::default();
    let id = drag(&mut e, Tool::Blur, at(20., 20.), at(120., 80.));
    e.tool = Tool::Select;
    // A press inside selects it, as the zone is solid to the eye.
    e.begin(at(70., 50.), Modifiers::default());
    e.update(at(90., 60.));
    e.finish();
    assert_eq!(e.selected, [id]);
    let moved = Rect::new(at(40., 30.), at(140., 90.)).unwrap();
    assert_eq!(e.objects()[0].shape, Shape::Blur(moved));
    assert_eq!(
        e.rotation_handle(),
        None,
        "a lone blur has no rotation handle"
    );
    // The bottom-right corner handle resizes it.
    e.begin(at(140., 90.), Modifiers::default());
    e.update(at(200., 130.));
    e.finish();
    assert_eq!(
        e.objects()[0].shape,
        Shape::Blur(Rect::new(at(40., 30.), at(200., 130.)).unwrap())
    );
    // Turned with a rectangle, it only follows.
    let rectangle = drag(&mut e, Tool::Rectangle, at(300., 30.), at(360., 70.));
    e.tool = Tool::Select;
    e.selected = vec![id, rectangle];
    let handle = e.rotation_handle().expect("the rectangle turns");
    let center = e.selection_frame().unwrap().bounds.center();
    e.begin(handle, Modifiers::default());
    e.update(center + (center - handle));
    e.finish();
    let blur = &e.objects()[0];
    assert_eq!(blur.rotation, 0.);
    let size = blur.bounds().size();
    assert!((size.x - 160.).abs() < 1e-9 && (size.y - 100.).abs() < 1e-9);
    assert!(e.objects()[1].rotation != 0.);
}

#[test]
fn colours_and_the_bucket_pass_a_blur_by() {
    let mut e = Editor::default();
    e.style.color = [1, 2, 3];
    let id = drag(&mut e, Tool::Blur, at(20., 20.), at(120., 80.));
    let before = e.objects().to_vec();
    e.select_only(id);
    e.style.color = [200, 0, 0];
    e.apply_color();
    assert_eq!(e.objects(), before, "the palette");
    e.style.width = 30.;
    e.apply_style();
    assert_eq!(e.objects(), before, "a stroke width");
    assert!(!e.can_redo());
    e.selected.clear();
    e.fill_at(at(70., 50.), false);
    assert_eq!(e.objects(), before, "the bucket");
    assert_eq!(
        e.background().len(),
        1,
        "it colours the capture under the blur"
    );
}

#[test]
fn the_wheel_step_tells_a_blur_from_a_stroke() {
    let mut e = Editor::default();
    e.style.width = 4.;
    let blur = drag(&mut e, Tool::Blur, at(20., 20.), at(120., 80.));
    let line = drag(&mut e, Tool::Line, at(200., 20.), at(300., 20.));
    e.selected = vec![blur, line];
    let first = e.step_selected_widths(|w, blur| if blur { w * 2. } else { w + 1. });
    assert_eq!(first, Some((14., true)));
    assert_eq!(e.objects()[0].style.width, 14.);
    assert_eq!(e.objects()[1].style.width, 5.);
    e.undo();
    assert_eq!(e.objects()[0].style.width, BLUR_RADIUS, "one undo step");
    assert_eq!(e.objects()[1].style.width, 4.);
}

#[test]
fn the_eraser_takes_a_blur_by_its_edge_only() {
    let mut e = Editor::default();
    drag(&mut e, Tool::Blur, at(20., 20.), at(220., 120.));
    e.eraser_size = 10.;
    e.tool = Tool::Eraser;
    let sweep = |e: &mut Editor, from, to| {
        e.begin(from, Modifiers::default());
        e.update(to);
        e.finish();
    };
    // Rubbing out inside the zone leaves it.
    sweep(&mut e, at(60., 60.), at(180., 80.));
    assert_eq!(e.objects().len(), 1);
    // Across its edge, it goes, as one step.
    sweep(&mut e, at(10., 70.), at(30., 70.));
    assert!(e.objects().is_empty());
    e.undo();
    assert_eq!(e.objects().len(), 1);
}

#[test]
fn a_copied_blur_pastes_as_a_blur() {
    let mut e = Editor::default();
    e.blur_radius = 12.;
    let id = drag(&mut e, Tool::Blur, at(20., 20.), at(120., 80.));
    e.select_only(id);
    let copied = e.copy_selected();
    assert!(e.paste_objects(&copied, at(10., 10.)));
    let pasted = e.objects().last().unwrap();
    assert_ne!(pasted.id, id);
    assert_eq!(
        pasted.shape,
        Shape::Blur(Rect::new(at(30., 30.), at(130., 90.)).unwrap())
    );
    assert_eq!(pasted.style.width, 12.);
}

/// Cost of blurring a whole 4K frame, the worst a drag can ask. Run by hand:
/// `cargo test --release -p shotoshot-core --test blur -- --ignored --nocapture`.
#[test]
#[ignore]
fn cost_of_a_full_4k_blur() {
    let (width, height) = (3840u32, 2160u32);
    let pixels = text_frame(width, height, 1);
    for (w, h) in [(3840, 2160), (1200, 300), (400, 60)] {
        let start = std::time::Instant::now();
        let rounds = 5;
        for seed in 0..rounds {
            std::hint::black_box(patch(&pixels, width, zone(0, 0, w, h), 7, seed));
        }
        let each = start.elapsed() / rounds as u32;
        println!("{w} x {h}: {each:?} a patch");
    }
}
