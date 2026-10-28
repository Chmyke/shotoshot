//! ROLE: Draws the pointer effects of the presentation (halo, trail, click waves, spotlight)
//! on any `Canvas`.
//! OWNER: The resident's pointer layer paints the live frame; the pointer window paints its
//! preview with the same code, so the preview shows exactly what the desktop will.
//! INVARIANTS: Device pixels; `origin` is the screen position of the canvas's top-left
//! corner. Nothing allocates per point except the spotlight hole.
use crate::{Canvas, Ink};
use shotoshot_core::{
    Point, Rect,
    pointer::{
        Button, CLICK_MS, ClickStyle, Effects, HaloShape, NEON_GLOW, NEON_SOFT_GROWTH,
        PULSE_GROWTH, PointerSettings, SOFT_REACH, SPOTLIGHT_FEATHER, SpotlightShape, TrailPoint,
        TrailStyle, WAVE_GAP_MS, spotlight_fade, spotlight_hole,
    },
};

/// The halo's colour while a button is held.
pub const PRESSED: [u8; 3] = [237, 79, 92];
/// Warm colours the sparks cycle through.
const SPARKS: [[u8; 3]; 3] = [[255, 214, 92], [255, 150, 64], [255, 250, 220]];
/// A soft edge or a trail is drawn as this many overlapping shapes of low opacity: where all
/// overlap the opacity is full, and it falls off smoothly without a seam between pieces.
const GRADIENT_STEPS: usize = 16;
/// Opacity of the trail ribbon where it leaves the halo.
const RIBBON_ALPHA: f32 = 0.85;

/// What the halo shows besides the settings: the halo colour, a held button, the click pulse.
#[derive(Clone, Copy, Debug)]
pub struct HaloState {
    pub color: [u8; 3],
    pub pressed: bool,
    /// Progress of the click pulse in 0..1, None when it is not playing.
    pub pulse: Option<f64>,
}

/// One frame of every effect that follows the pointer, spotlight excepted.
pub fn paint<C: Canvas>(
    canvas: &mut C,
    origin: Point,
    effects: &Effects,
    settings: &PointerSettings,
    halo: HaloState,
    scale: f64,
    now: u64,
) -> Result<(), C::Error> {
    let local = |p: Point| p - origin;
    trail(canvas, &local, effects, settings, halo.color, scale, now)?;
    for (click, progress) in effects.clicks(now) {
        let rgb = match click.button {
            Button::Left => settings.left_click_color,
            Button::Right | Button::Middle => settings.right_click_color,
        };
        click_wave(
            canvas,
            local(click.at),
            settings.click,
            progress,
            rgb,
            scale,
        )?;
    }
    if let Some(center) = effects.shown() {
        halo_shape(canvas, local(center), settings, halo, scale)?;
    }
    Ok(())
}

fn halo_shape<C: Canvas>(
    canvas: &mut C,
    center: Point,
    settings: &PointerSettings,
    halo: HaloState,
    scale: f64,
) -> Result<(), C::Error> {
    if settings.halo_shape == HaloShape::Hidden {
        return Ok(());
    }
    let opacity = settings.halo_opacity as f32 / 100.;
    let soft = settings.halo_softness as f64 / 100.;
    let rgb = if halo.pressed { PRESSED } else { halo.color };
    let radius = settings.halo_diameter as f64 * 0.5 * scale;
    let round = |r: f64| Point { x: r, y: r };
    let width = settings.halo_thickness as f64 * scale;
    match settings.halo_shape {
        HaloShape::Hidden => {}
        HaloShape::Disc if soft > 0. => {
            // A sharp core whose outer part fades to nothing: nested discs, each faint.
            let core = if halo.pressed { 0.65 } else { 0.55 } * opacity;
            let inner = radius * (1. - soft);
            let step = step_alpha(core);
            for index in 0..GRADIENT_STEPS {
                let r = radius - (radius - inner) * index as f64 / GRADIENT_STEPS as f64;
                canvas.fill_ellipse(center, round(r), Ink::with_alpha(rgb, step))?;
            }
        }
        HaloShape::Disc => {
            let fill = if halo.pressed { 0.24 } else { 0.14 };
            canvas.fill_ellipse(center, round(radius), Ink::with_alpha(rgb, fill * opacity))?;
            canvas.stroke_ellipse(
                center,
                round(radius),
                Ink::with_alpha(rgb, 0.55 * opacity),
                width,
            )?;
        }
        HaloShape::Ring if soft > 0. => {
            // The ring's line, blurred on both sides: nested strokes, the widest the faintest.
            let reach = radius * SOFT_REACH * soft;
            let step = step_alpha(0.9 * opacity);
            for index in 0..GRADIENT_STEPS {
                let spread = 2. * reach * (1. - index as f64 / GRADIENT_STEPS as f64);
                canvas.stroke_ellipse(
                    center,
                    round(radius),
                    Ink::with_alpha(rgb, step),
                    width + spread,
                )?;
            }
        }
        HaloShape::Ring => {
            canvas.stroke_ellipse(
                center,
                round(radius),
                Ink::with_alpha(rgb, 0.9 * opacity),
                width,
            )?;
        }
        HaloShape::Neon => {
            let glow = NEON_GLOW * (1. + NEON_SOFT_GROWTH * soft);
            for step in (1..=4).rev() {
                let spread = width + step as f64 * glow * 0.5 * scale;
                canvas.stroke_ellipse(
                    center,
                    round(radius),
                    Ink::with_alpha(rgb, 0.07 * opacity),
                    spread,
                )?;
            }
            canvas.stroke_ellipse(
                center,
                round(radius),
                Ink::with_alpha(rgb, 0.95 * opacity),
                width,
            )?;
            canvas.stroke_ellipse(
                center,
                round(radius),
                Ink::with_alpha(lighten(rgb), 0.9 * opacity),
                (width * 0.4).max(scale * 0.75),
            )?;
        }
    }
    if let Some(progress) = halo.pulse {
        canvas.stroke_ellipse(
            center,
            round(radius * (1. + progress * PULSE_GROWTH)),
            Ink::with_alpha(rgb, (1. - progress as f32) * 0.5 * opacity),
            2. * scale,
        )?;
    }
    Ok(())
}

/// The opacity of each of `GRADIENT_STEPS` overlapping shapes that add up to `total`.
fn step_alpha(total: f32) -> f32 {
    1. - (1. - total.clamp(0., 0.999)).powf(1. / GRADIENT_STEPS as f32)
}

fn trail<C: Canvas>(
    canvas: &mut C,
    local: &impl Fn(Point) -> Point,
    effects: &Effects,
    settings: &PointerSettings,
    rgb: [u8; 3],
    scale: f64,
    now: u64,
) -> Result<(), C::Error> {
    // Logical distance between two samples of the smoothed path.
    let spacing = match settings.trail {
        TrailStyle::Off => return Ok(()),
        TrailStyle::Line => 2.5,
        TrailStyle::Dots => 9.,
        TrailStyle::Sparks => 5.,
        TrailStyle::Stars => 16.,
    };
    let mut path: Vec<TrailPoint> = Vec::new();
    effects.trail_path(now, settings, spacing * scale, &mut path);
    if path.len() < 2 {
        return Ok(());
    }
    let lifetime = settings.trail_length.lifetime_ms() as f64;
    let life =
        |point: &TrailPoint| (1. - now.saturating_sub(point.time) as f64 / lifetime).clamp(0., 1.);
    match settings.trail {
        TrailStyle::Off => {}
        TrailStyle::Line => {
            let points: Vec<Point> = path.iter().map(|p| local(p.at)).collect();
            let lives: Vec<f64> = path.iter().map(life).collect();
            let width = (settings.halo_diameter as f64 * 0.3).clamp(4., 36.) * scale;
            ribbon(canvas, &points, &lives, width, rgb)?;
        }
        TrailStyle::Dots => {
            for point in &path {
                let life = life(point);
                let radius = 3.5 * scale * (0.3 + 0.7 * life);
                canvas.fill_ellipse(
                    local(point.at),
                    Point {
                        x: radius,
                        y: radius,
                    },
                    Ink::with_alpha(rgb, 0.85 * life as f32),
                )?;
            }
        }
        TrailStyle::Sparks => {
            for point in &path {
                let life = life(point);
                // Seeded by the moment the pointer passed: each spark keeps its own way.
                let seed = hash(point.time / 4);
                let angle = (seed % 628) as f64 / 100.;
                let drift = 14. * scale * (1. - life);
                let center = local(point.at)
                    + Point {
                        x: angle.cos() * drift,
                        y: angle.sin() * drift,
                    };
                let arm = 4. * scale * (0.3 + 0.7 * life);
                let ink = Ink::with_alpha(SPARKS[(seed / 7 % 3) as usize], 0.95 * life as f32);
                canvas.line(
                    center - Point { x: arm, y: 0. },
                    center + Point { x: arm, y: 0. },
                    ink,
                    scale * 1.4,
                )?;
                canvas.line(
                    center - Point { x: 0., y: arm },
                    center + Point { x: 0., y: arm },
                    ink,
                    scale * 1.4,
                )?;
            }
        }
        TrailStyle::Stars => {
            for point in &path {
                let life = life(point);
                let radius = 6. * scale * (0.4 + 0.6 * life);
                let spin = (1. - life) * 2.4 + (hash(point.time / 4) % 100) as f64 / 50.;
                let star = star_points(local(point.at), radius, spin);
                canvas.fill_polygon(&star, Ink::with_alpha(lighten(rgb), 0.95 * life as f32))?;
            }
        }
    }
    Ok(())
}

/// The trail as one ribbon, as a game engine's trail renderer draws it: a strip whose width
/// and opacity fall to nothing at the tail, rounded under the halo. Drawn as nested outlines,
/// from the whole ribbon to its youngest part, each faint: no joint overlaps another.
fn ribbon<C: Canvas>(
    canvas: &mut C,
    points: &[Point],
    lives: &[f64],
    width: f64,
    rgb: [u8; 3],
) -> Result<(), C::Error> {
    let count = points.len();
    let mut left = Vec::with_capacity(count);
    let mut right = Vec::with_capacity(count);
    let mut normal = Point { x: 0., y: -1. };
    let mut forward = Point { x: 1., y: 0. };
    for index in 0..count {
        let along = points[(index + 1).min(count - 1)] - points[index.saturating_sub(1)];
        let length = along.length();
        if length > 1e-6 {
            forward = along * (1. / length);
            normal = Point {
                x: -forward.y,
                y: forward.x,
            };
        }
        let half = width * 0.5 * lives[index].powf(0.8);
        left.push(points[index] + normal * half);
        right.push(points[index] - normal * half);
    }
    // The round end under the halo: a half circle from the left edge, ahead, to the right one.
    let head = points[count - 1];
    let half = width * 0.5 * lives[count - 1].powf(0.8);
    let cap: Vec<Point> = (1..8)
        .map(|index| {
            let angle = std::f64::consts::PI * index as f64 / 8.;
            head + normal * (half * angle.cos()) + forward * (half * angle.sin())
        })
        .collect();
    let step = step_alpha(RIBBON_ALPHA);
    let mut outline = Vec::with_capacity(2 * count + cap.len());
    for layer in 0..GRADIENT_STEPS {
        let threshold = layer as f64 / GRADIENT_STEPS as f64;
        let start = lives
            .iter()
            .position(|life| *life > threshold)
            .unwrap_or(count);
        if count - start < 2 {
            continue;
        }
        outline.clear();
        outline.extend_from_slice(&left[start..]);
        outline.extend_from_slice(&cap);
        outline.extend(right[start..].iter().rev());
        canvas.fill_polygon(&outline, Ink::with_alpha(rgb, step))?;
    }
    Ok(())
}

fn click_wave<C: Canvas>(
    canvas: &mut C,
    at: Point,
    style: ClickStyle,
    progress: f64,
    rgb: [u8; 3],
    scale: f64,
) -> Result<(), C::Error> {
    let ring = |canvas: &mut C, progress: f64, width: f64| -> Result<(), C::Error> {
        if !(0. ..1.).contains(&progress) {
            return Ok(());
        }
        let radius = (6. + 30. * ease_out(progress)) * scale;
        canvas.stroke_ellipse(
            at,
            Point {
                x: radius,
                y: radius,
            },
            Ink::with_alpha(rgb, (1. - progress as f32) * 0.9),
            width * scale,
        )
    };
    match style {
        ClickStyle::Off => Ok(()),
        ClickStyle::Ripple => {
            if progress >= 1. {
                return Ok(());
            }
            let radius = (8. + 26. * ease_out(progress)) * scale;
            canvas.fill_ellipse(
                at,
                Point {
                    x: radius,
                    y: radius,
                },
                Ink::with_alpha(rgb, (1. - progress as f32) * 0.35),
            )?;
            canvas.stroke_ellipse(
                at,
                Point {
                    x: radius,
                    y: radius,
                },
                Ink::with_alpha(rgb, (1. - progress as f32) * 0.7),
                1.5 * scale,
            )
        }
        ClickStyle::Ring => ring(canvas, progress, 3.),
        ClickStyle::Waves => {
            let gap = WAVE_GAP_MS as f64 / CLICK_MS as f64;
            for wave in 0..3 {
                ring(canvas, progress - wave as f64 * gap, 2.)?;
            }
            Ok(())
        }
    }
}

/// The dimmed screen around the spotlight, for the preview: the live spotlight is a window.
/// The soft edge is laid as stacked rings, each darkening what lies beyond it a little more.
pub fn spotlight<C: Canvas>(
    canvas: &mut C,
    area: Rect,
    at: Point,
    settings: &PointerSettings,
    scale: f64,
) -> Result<(), C::Error> {
    const RINGS: u32 = 10;
    if settings.spotlight == SpotlightShape::Off {
        return Ok(());
    }
    let dim = settings.spotlight_dim as f64 / 100.;
    let feather = SPOTLIGHT_FEATHER * scale;
    let mut laid = 0.;
    for ring in 0..=RINGS {
        let darkness = dim * spotlight_fade((ring as f64 + 0.5) / RINGS as f64);
        let alpha = 1. - (1. - darkness) / (1. - laid);
        let margin = feather * ring as f64 / RINGS as f64;
        let hole = spotlight_hole(settings, at, scale, area, margin);
        canvas.fill_rect_except(area, &[hole], Ink::with_alpha([0; 3], alpha as f32))?;
        laid = darkness;
    }
    Ok(())
}

fn ease_out(t: f64) -> f64 {
    1. - (1. - t.clamp(0., 1.)).powi(3)
}
fn lighten([red, green, blue]: [u8; 3]) -> [u8; 3] {
    let mix = |value: u8| value + (255 - value) / 2;
    [mix(red), mix(green), mix(blue)]
}
/// A small deterministic scramble: sparks look random but repeat frame to frame.
fn hash(value: u64) -> u64 {
    let mut x = value.wrapping_mul(0x9e37_79b9_7f4a_7c15);
    x ^= x >> 29;
    x = x.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x ^ (x >> 32)
}
fn star_points(center: Point, radius: f64, spin: f64) -> [Point; 10] {
    let mut points = [center; 10];
    for (index, point) in points.iter_mut().enumerate() {
        let angle = spin + index as f64 * std::f64::consts::PI / 5. - std::f64::consts::FRAC_PI_2;
        let reach = if index.is_multiple_of(2) {
            radius
        } else {
            radius * 0.45
        };
        *point = center
            + Point {
                x: angle.cos() * reach,
                y: angle.sin() * reach,
            };
    }
    points
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scale_tests::Recorder;
    use shotoshot_core::pointer::{Follow, SpotlightShape, TrailLength};

    fn at(x: f64, y: f64) -> Point {
        Point { x, y }
    }
    fn recorder() -> Recorder {
        Recorder {
            size: at(800., 600.),
            ops: Vec::new(),
        }
    }
    const HALO: HaloState = HaloState {
        color: [255, 211, 64],
        pressed: false,
        pulse: None,
    };
    /// A scene with every effect busy, `scale` device pixels per logical pixel.
    fn busy(settings: &PointerSettings, scale: f64) -> Recorder {
        let mut effects = Effects::default();
        for step in 0..30u64 {
            let p = at(200. + step as f64 * 3., 200.) * scale;
            effects.moved(p, step * 8, scale, settings);
        }
        effects.clicked(at(250., 200.) * scale, Button::Left, 200);
        let mut canvas = recorder();
        let halo = HaloState {
            pulse: Some(0.5),
            ..HALO
        };
        paint(
            &mut canvas,
            at(0., 0.),
            &effects,
            settings,
            halo,
            scale,
            240,
        )
        .unwrap();
        canvas
    }

    #[test]
    fn each_halo_shape_draws_and_hidden_draws_nothing() {
        for shape in HaloShape::ALL {
            let settings = PointerSettings {
                halo_shape: *shape,
                ..Default::default()
            };
            let mut effects = Effects::default();
            effects.place(at(100., 100.), 0);
            let mut canvas = recorder();
            paint(&mut canvas, at(0., 0.), &effects, &settings, HALO, 1., 0).unwrap();
            assert_eq!(
                canvas.ops.is_empty(),
                *shape == HaloShape::Hidden,
                "{shape:?}"
            );
        }
    }

    #[test]
    fn every_trail_and_click_style_draws_only_when_chosen_and_near_the_pointer() {
        for trail in TrailStyle::ALL {
            for click in ClickStyle::ALL {
                let settings = PointerSettings {
                    halo_shape: HaloShape::Hidden,
                    trail: *trail,
                    trail_length: TrailLength::Long,
                    click: *click,
                    follow: Follow::Instant,
                    ..Default::default()
                };
                let mut effects = Effects::default();
                for step in 0..30u64 {
                    effects.moved(at(200. + step as f64 * 3., 200.), step * 8, 1., &settings);
                }
                effects.clicked(at(250., 200.), Button::Left, 200);
                let mut canvas = recorder();
                paint(&mut canvas, at(0., 0.), &effects, &settings, HALO, 1., 240).unwrap();
                let expected = *trail != TrailStyle::Off || *click != ClickStyle::Off;
                assert_eq!(!canvas.ops.is_empty(), expected, "{trail:?} {click:?}");
                for op in &canvas.ops {
                    let (x, y) = (op.lengths[0], op.lengths[1]);
                    assert!(
                        (0. ..=540.).contains(&x) && (0. ..=440.).contains(&y),
                        "{trail:?} {click:?} {op:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn the_effects_scale_as_one_piece() {
        for shape in HaloShape::ALL {
            let settings = PointerSettings {
                halo_shape: *shape,
                trail: TrailStyle::Line,
                click: ClickStyle::Waves,
                ..Default::default()
            };
            let small = busy(&settings, 1.);
            let large = busy(&settings, 2.);
            assert_eq!(small.ops.len(), large.ops.len(), "{shape:?}");
            for (a, b) in small.ops.iter().zip(&large.ops) {
                assert_eq!(a.name, b.name);
                assert_eq!(a.fixed, b.fixed, "{shape:?} {}", a.name);
                for (x, y) in a.lengths.iter().zip(&b.lengths) {
                    assert!((x * 2. - y).abs() < 1e-6, "{shape:?} {} {x} {y}", a.name);
                }
            }
        }
    }

    #[test]
    fn the_spotlight_preview_dims_around_its_hole() {
        let settings = PointerSettings {
            spotlight: SpotlightShape::Circle,
            ..Default::default()
        };
        let area = Rect {
            min: at(0., 0.),
            max: at(400., 300.),
        };
        let mut canvas = recorder();
        spotlight(&mut canvas, area, at(200., 150.), &settings, 1.).unwrap();
        // Stacked rings lay the soft edge, each one wider than the last.
        assert!(canvas.ops.len() > 1);
        assert!(canvas.ops.iter().all(|op| op.name == "fill_rect_except"));
        let mut off = recorder();
        spotlight(
            &mut off,
            area,
            at(200., 150.),
            &PointerSettings::default(),
            1.,
        )
        .unwrap();
        assert!(off.ops.is_empty());
    }
}
