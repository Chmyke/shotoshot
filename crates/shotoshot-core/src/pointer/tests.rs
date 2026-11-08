use super::*;

fn at(x: f64, y: f64) -> Point {
    Point { x, y }
}

#[test]
fn settings_round_trip_and_reject_out_of_range_values() {
    let settings = PointerSettings {
        halo_shape: HaloShape::Neon,
        halo_diameter: 96,
        follow: Follow::Smooth,
        trail: TrailStyle::Sparks,
        click: ClickStyle::Waves,
        sound: ClickSound::Click,
        click_shortcut: Some(Shortcut {
            modifiers: 3,
            key: 0x4b,
        }),
        spotlight: SpotlightShape::Horizontal,
        spotlight_hidden_moving: true,
        ..Default::default()
    };
    let line = settings.encode();
    assert_eq!(PointerSettings::decode(&line), Ok(settings.clone()));
    // The retired shake to find field is written off and read whatever 0.3.16 saved.
    let mut fields: Vec<String> = line.split(',').map(String::from).collect();
    assert_eq!(fields[18], "0");
    fields[18] = "1".into();
    assert_eq!(PointerSettings::decode(&fields.join(",")), Ok(settings));
    fields[18] = "2".into();
    assert!(PointerSettings::decode(&fields.join(",")).is_err(), "flag");
    assert_eq!(
        PointerSettings::decode(&PointerSettings::default().encode()),
        Ok(PointerSettings::default())
    );
    let mut fields: Vec<String> = line.split(',').map(String::from).collect();
    fields[1] = "300".into();
    assert!(
        PointerSettings::decode(&fields.join(",")).is_err(),
        "diameter"
    );
    fields[1] = "96".into();
    fields[0] = "9".into();
    assert!(PointerSettings::decode(&fields.join(",")).is_err(), "shape");
    fields[0] = "01".into();
    assert!(
        PointerSettings::decode(&fields.join(",")).is_err(),
        "padded"
    );
    assert!(PointerSettings::decode("1,2,3").is_err(), "short");
}

#[test]
fn the_loupe_is_stored_with_its_own_choices_and_never_as_a_shape() {
    let settings = PointerSettings {
        loupe_size: 320,
        loupe_zoom: 4,
        loupe_shortcut: Some(Shortcut {
            modifiers: 2,
            key: 0x4c,
        }),
        loupe_hold: true,
        ..Default::default()
    };
    let line = settings.encode();
    assert_eq!(line.split(',').count(), 24);
    assert_eq!(PointerSettings::decode(&line), Ok(settings.clone()));
    // A line of 0.3.21 has no loupe: the default one.
    let old: Vec<_> = PointerSettings::default()
        .encode()
        .split(',')
        .take(20)
        .map(String::from)
        .collect();
    assert_eq!(
        PointerSettings::decode(&old.join(",")),
        Ok(PointerSettings::default())
    );
    let mut fields: Vec<String> = line.split(',').map(String::from).collect();
    // Any size and zoom of their range, as the sliders set them; none outside.
    fields[20] = "200".into();
    fields[21] = "5".into();
    let decoded = PointerSettings::decode(&fields.join(",")).unwrap();
    assert_eq!((decoded.loupe_size, decoded.loupe_zoom), (200, 5));
    fields[20] = "500".into();
    assert!(PointerSettings::decode(&fields.join(",")).is_err(), "size");
    fields[20] = "160".into();
    fields[21] = "9".into();
    assert!(PointerSettings::decode(&fields.join(",")).is_err(), "zoom");
    fields[21] = "3".into();
    fields[23] = "2".into();
    assert!(PointerSettings::decode(&fields.join(",")).is_err(), "hold");
    assert!(
        PointerSettings::decode(&fields[..22].join(",")).is_err(),
        "cut"
    );
    // The trial build's loupe shape comes back as the disc, and is never written.
    fields[23] = "0".into();
    fields[0] = "4".into();
    let read = PointerSettings::decode(&fields.join(",")).unwrap();
    assert_eq!(read.halo_shape, HaloShape::Disc);
    assert!(read.encode().starts_with("1,"));
    fields[0] = "5".into();
    assert!(PointerSettings::decode(&fields.join(",")).is_err(), "shape");
}

#[test]
fn the_loupe_hides_the_halo_shape_and_alone_shows_nothing_else() {
    let busy = PointerSettings {
        trail: TrailStyle::Line,
        click: ClickStyle::Ring,
        spotlight: SpotlightShape::Circle,
        follow: Follow::Smooth,
        ..Default::default()
    };
    // No ring, no ticks: with the halo on, the loupe replaces its shape only.
    let both = busy.drawn(true, true);
    assert_eq!(both.halo_shape, HaloShape::Hidden);
    assert_eq!(
        (both.trail, both.click, both.follow),
        (TrailStyle::Line, ClickStyle::Ring, Follow::Instant)
    );
    assert_eq!(busy.drawn(true, false), busy);
    let alone = busy.drawn(false, true);
    assert_eq!(alone.halo_shape, HaloShape::Hidden);
    assert_eq!(
        (alone.trail, alone.click, alone.spotlight),
        (TrailStyle::Off, ClickStyle::Off, SpotlightShape::Off)
    );
    assert!(!alone.draws_around());
    assert_eq!(busy.drawn(false, false).halo_shape, HaloShape::Hidden);
    let wild = PointerSettings {
        loupe_size: 7,
        loupe_zoom: 9,
        ..Default::default()
    }
    .clamped();
    assert_eq!((wild.loupe_size, wild.loupe_zoom), (120, 8));
}

#[test]
fn clamping_keeps_every_number_in_its_range() {
    let wild = PointerSettings {
        halo_diameter: 0,
        halo_opacity: 900,
        halo_thickness: 0,
        spotlight_size: 9000,
        spotlight_dim: 1,
        ..Default::default()
    }
    .clamped();
    assert_eq!(wild.halo_diameter, HALO_DIAMETER_RANGE.0);
    assert_eq!(wild.halo_opacity, HALO_OPACITY_RANGE.1);
    assert_eq!(wild.halo_thickness, HALO_THICKNESS_RANGE.0);
    assert_eq!(wild.spotlight_size, SPOTLIGHT_SIZE_RANGE.1);
    assert_eq!(wild.spotlight_dim, SPOTLIGHT_DIM_RANGE.0);
    assert!(PointerSettings::decode(&wild.encode()).is_ok());
}

#[test]
fn the_trail_keeps_recent_points_and_forgets_old_ones() {
    let settings = PointerSettings {
        trail: TrailStyle::Line,
        trail_length: TrailLength::Short,
        ..Default::default()
    };
    let mut effects = Effects::default();
    for step in 0..10u64 {
        effects.moved(at(step as f64 * 10., 0.), step * 20, 1., &settings);
    }
    let points: Vec<_> = effects.trail(180, &settings).collect();
    assert!(!points.is_empty());
    assert!(points.iter().all(|(_, age)| (0. ..1.).contains(age)));
    assert!(
        points.windows(2).all(|pair| pair[0].1 >= pair[1].1),
        "oldest first"
    );
    assert!(effects.animating(180, &settings));
    assert_eq!(effects.trail(1000, &settings).count(), 0);
    assert!(
        !effects.animating(1000, &settings),
        "nothing left to animate"
    );
    let off = PointerSettings::default();
    assert_eq!(effects.trail(180, &off).count(), 0, "off draws nothing");
}

#[test]
fn the_trail_survives_more_moves_than_it_can_hold() {
    let settings = PointerSettings {
        trail: TrailStyle::Dots,
        trail_length: TrailLength::Long,
        ..Default::default()
    };
    let mut effects = Effects::default();
    for step in 0..500u64 {
        effects.moved(at(step as f64 * 4., 0.), step, 1., &settings);
    }
    let points: Vec<_> = effects.trail(499, &settings).collect();
    assert_eq!(points.len(), TRAIL_POINTS);
    assert_eq!(points.last().map(|(p, _)| p.x), Some(499. * 4.));
}

#[test]
fn the_trail_path_is_smooth_evenly_spaced_and_ends_under_the_halo() {
    let settings = PointerSettings {
        trail: TrailStyle::Line,
        trail_length: TrailLength::Long,
        ..Default::default()
    };
    let mut effects = Effects::default();
    // A right angle, then a resting hand that jitters by one pixel.
    for (step, (x, y)) in [
        (0., 0.),
        (40., 0.),
        (80., 0.),
        (80., 40.),
        (80., 80.),
        (81., 80.),
        (80., 81.),
    ]
    .into_iter()
    .enumerate()
    {
        effects.moved(at(x, y), step as u64 * 20, 1., &settings);
    }
    let mut path = Vec::new();
    effects.trail_path(140, &settings, 4., &mut path);
    // 160 pixels of polyline, a little less once the corner is cut round.
    assert!(path.len() > 32, "{}", path.len());
    assert_eq!(path.first().map(|p| p.at), Some(at(0., 0.)));
    assert_eq!(
        path.last().map(|p| p.at),
        effects.shown(),
        "ends under the halo"
    );
    for pair in path.windows(2) {
        let gap = (pair[1].at - pair[0].at).length();
        // A curve is a little longer than its chord: samples stay close to the spacing.
        assert!(gap <= 6., "even spacing, {gap}");
        assert!(pair[0].time <= pair[1].time, "oldest first");
    }
    // The corner is rounded: no sample sits exactly on it.
    assert!(path.iter().all(|p| p.at != at(80., 0.) || p.time == 40));
    let off = PointerSettings::default();
    effects.trail_path(140, &off, 4., &mut path);
    assert!(path.is_empty());
}

#[test]
fn a_quick_shaky_stroke_leaves_a_ribbon_without_visible_corners() {
    let settings = PointerSettings {
        trail: TrailStyle::Line,
        trail_length: TrailLength::Long,
        ..Default::default()
    };
    let mut effects = Effects::default();
    // Far apart samples of a fast hand that wobbles by 5 pixels, unevenly spaced.
    for step in 0..16u64 {
        let x = (step * 30 + step % 3 * 9) as f64;
        let y = if step % 2 == 0 { 0. } else { 5. };
        effects.moved(at(x, y), step * 8, 1., &settings);
    }
    let mut path = Vec::new();
    effects.trail_path(130, &settings, 2.5, &mut path);
    let mut sharpest: f64 = 0.;
    for triple in path.windows(3) {
        let a = triple[1].at - triple[0].at;
        let b = triple[2].at - triple[1].at;
        if a.length() < 1e-6 || b.length() < 1e-6 {
            continue;
        }
        let cos = ((a.x * b.x + a.y * b.y) / (a.length() * b.length())).clamp(-1., 1.);
        sharpest = sharpest.max(cos.acos().to_degrees());
    }
    assert!(
        sharpest < 3.,
        "the ribbon turns by {sharpest:.1} degrees between samples"
    );
}

#[test]
fn a_resting_hand_adds_no_trail_vertex() {
    let settings = PointerSettings {
        trail: TrailStyle::Line,
        ..Default::default()
    };
    let mut effects = Effects::default();
    effects.moved(at(10., 10.), 0, 1., &settings);
    for step in 1..20u64 {
        let wobble = (step % 2) as f64;
        effects.moved(at(10. + wobble, 10. - wobble), step, 1., &settings);
    }
    assert_eq!(effects.trail(19, &settings).count(), 1);
}

#[test]
fn the_first_pointer_line_still_reads_with_a_sharp_edge() {
    let line = PointerSettings::default().encode();
    let first: Vec<&str> = line.split(',').take(19).collect();
    let read = PointerSettings::decode(&first.join(",")).unwrap();
    assert_eq!(read.halo_softness, 0);
    let soft = PointerSettings {
        halo_softness: 60,
        ..Default::default()
    };
    assert_eq!(PointerSettings::decode(&soft.encode()), Ok(soft));
}

#[test]
fn the_bounds_cover_the_halo_the_trail_and_the_clicks() {
    let settings = PointerSettings {
        trail: TrailStyle::Line,
        click: ClickStyle::Ring,
        ..Default::default()
    };
    let mut effects = Effects::default();
    assert_eq!(effects.bounds(0, &settings, 1.), None);
    for step in 0..10u64 {
        effects.moved(at(100. + step as f64 * 20., 300.), step * 10, 1., &settings);
    }
    effects.clicked(at(500., 500.), Button::Left, 90);
    let bounds = effects.bounds(100, &settings, 2.).unwrap();
    assert!(bounds.contains(at(100., 300.)), "the tail");
    assert!(
        bounds.contains(at(280. + 2. * AROUND_RADIUS - 1., 300.)),
        "the halo's room"
    );
    assert!(
        bounds.contains(at(500. + 2. * CLICK_REACH - 1., 500.)),
        "the click"
    );
    // Once everything has faded, only the halo's room is left.
    let rest = effects.bounds(5000, &settings, 1.).unwrap();
    assert!(
        !rest.contains(at(500. + CLICK_REACH, 500.)),
        "the faded click"
    );
}

/// Mickaël's rule (2026-10-30): a fresh install makes no sound; the click sound is opt-in.
#[test]
fn a_fresh_install_is_silent() {
    assert_eq!(PointerSettings::default().sound, ClickSound::Off);
}

#[test]
fn clicks_animate_then_stop() {
    let settings = PointerSettings::default();
    let mut effects = Effects::default();
    effects.clicked(at(5., 5.), Button::Right, 100);
    let clicks: Vec<_> = effects.clicks(100).collect();
    assert_eq!(clicks.len(), 1);
    assert_eq!(clicks[0].0.button, Button::Right);
    assert_eq!(clicks[0].1, 0.);
    assert!(effects.animating(300, &settings));
    assert!(!effects.animating(100 + CLICK_MS + 2 * WAVE_GAP_MS, &settings));
    for index in 0..30 {
        effects.clicked(at(1., 1.), Button::Left, 200 + index);
    }
    assert_eq!(effects.clicks(230).count(), CLICKS, "bounded storage");
}

#[test]
fn shaking_the_mouse_plays_nothing() {
    let settings = PointerSettings::default();
    let mut effects = Effects::default();
    let mut now = 0;
    for x in [0., 100., 0., 100., 0., 100.] {
        now += 60;
        effects.moved(at(x, 0.), now, 1., &settings);
    }
    assert!(
        !effects.animating(now, &settings),
        "no animation after a shake"
    );
    let halo = layer_radius(&settings) + 2.;
    let bounds = effects.bounds(now, &settings, 1.).unwrap();
    assert_eq!(
        (bounds.min, bounds.max),
        (at(100. - halo, -halo), at(100. + halo, halo))
    );
}

#[test]
fn smooth_follow_catches_up_and_instant_never_lags() {
    let smooth = PointerSettings {
        follow: Follow::Smooth,
        ..Default::default()
    };
    let mut effects = Effects::default();
    effects.place(at(0., 0.), 0);
    effects.moved(at(100., 0.), 0, 1., &smooth);
    let first = effects.step(16, &smooth).unwrap();
    assert!(first.x > 0. && first.x < 100., "{first:?}");
    assert!(effects.animating(16, &smooth));
    let late = effects.step(1000, &smooth).unwrap();
    assert_eq!(late, at(100., 0.));
    assert!(!effects.animating(1000, &smooth));

    let instant = PointerSettings::default();
    let mut effects = Effects::default();
    effects.moved(at(40., 30.), 0, 1., &instant);
    assert_eq!(effects.step(1, &instant), Some(at(40., 30.)));
}

#[test]
fn the_spotlight_obeys_its_shortcut_and_rests_after_moves() {
    let mut settings = PointerSettings {
        spotlight: SpotlightShape::Circle,
        ..Default::default()
    };
    let mut effects = Effects::default();
    effects.moved(at(1., 1.), 1000, 1., &settings);
    assert!(effects.spotlight_lit(1000, false, &settings));
    settings.spotlight_hidden_moving = true;
    assert!(!effects.spotlight_lit(1100, false, &settings), "moving");
    assert!(effects.animating(1100, &settings), "a timer brings it back");
    assert!(effects.spotlight_lit(1300, false, &settings), "rested");
    settings.spotlight_shortcut = Some(Shortcut {
        modifiers: 2,
        key: 0x20,
    });
    assert!(!effects.spotlight_lit(1300, false, &settings), "not held");
    assert!(effects.spotlight_lit(1300, true, &settings), "held");
    settings.spotlight = SpotlightShape::Off;
    assert!(!effects.spotlight_lit(1300, true, &settings), "off");
}

#[test]
fn spotlight_holes_follow_the_pointer_and_span_the_screen() {
    let screen = crate::Rect {
        min: at(0., 0.),
        max: at(1000., 800.),
    };
    let mut settings = PointerSettings {
        spotlight: SpotlightShape::Circle,
        spotlight_size: 200,
        ..Default::default()
    };
    let circle = spotlight_hole(&settings, at(500., 400.), 2., screen, 0.);
    assert_eq!(circle.len(), 48);
    assert!(
        circle
            .iter()
            .all(|p| ((*p - at(500., 400.)).length() - 200.).abs() < 1e-6),
        "diameter 200 logical at scale 2"
    );
    settings.spotlight = SpotlightShape::Horizontal;
    let band = spotlight_hole(&settings, at(500., 400.), 1., screen, 0.);
    assert_eq!(band[0], at(0., 300.));
    assert_eq!(band[2], at(1000., 500.));
    settings.spotlight = SpotlightShape::Vertical;
    let band = spotlight_hole(&settings, at(500., 400.), 1., screen, 0.);
    assert_eq!(band[0], at(400., 0.));
    assert_eq!(band[2], at(600., 800.));
}

#[test]
fn the_spotlight_fades_over_its_soft_edge() {
    let screen = crate::Rect {
        min: at(0., 0.),
        max: at(1000., 800.),
    };
    let mut settings = PointerSettings {
        spotlight: SpotlightShape::Circle,
        spotlight_size: 200,
        ..Default::default()
    };
    // Diameter 200 logical at scale 2: light to 200 device pixels, edge 60 more.
    let edge = spotlight_box(&settings, at(500., 400.), 2., screen);
    assert_eq!((edge.min, edge.max), (at(240., 140.), at(760., 660.)));
    let shade = |x: f64| spotlight_shade(&settings, at(x, 0.), 2.);
    assert_eq!(shade(0.), 0.);
    assert_eq!(shade(200.), 0.);
    assert!((shade(230.) - 0.5).abs() < 1e-9, "halfway through the edge");
    assert!(shade(215.) < shade(230.) && shade(230.) < shade(245.));
    assert_eq!(shade(260.), 1.);
    assert_eq!(
        spotlight_shade(&settings, at(300., 300.), 2.),
        1.,
        "box corner"
    );
    settings.spotlight = SpotlightShape::Horizontal;
    let band = spotlight_box(&settings, at(500., 400.), 1., screen);
    assert_eq!((band.min, band.max), (at(0., 270.), at(1000., 530.)));
    assert_eq!(
        spotlight_shade(&settings, at(900., 50.), 1.),
        0.,
        "inside the band"
    );
    settings.spotlight = SpotlightShape::Vertical;
    let band = spotlight_box(&settings, at(500., 400.), 1., screen);
    assert_eq!((band.min, band.max), (at(370., 0.), at(630., 800.)));
    assert_eq!(spotlight_shade(&settings, at(130., 0.), 1.), 1.);
}

#[test]
fn the_layer_grows_only_for_effects_around_the_halo() {
    let quiet = PointerSettings::default();
    assert!(layer_radius(&quiet) < 60., "the default halo stays small");
    let trail = PointerSettings {
        trail: TrailStyle::Dots,
        ..Default::default()
    };
    assert!(layer_radius(&trail) >= AROUND_RADIUS);
}
