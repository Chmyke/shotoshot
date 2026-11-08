use super::*;

fn object(id: u64, rotation: f64, color: [u8; 3], width: f64, shape: Shape) -> Object {
    Object {
        rotation,
        id,
        shape,
        style: Style { color, width },
        fill: Vec::new(),
    }
}
fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Rect {
    Rect {
        min: pair(x0, y0),
        max: pair(x1, y1),
    }
}
/// One object of every kind, with every optional field present somewhere.
pub(in crate::shoto) fn sample() -> Scene {
    let attach = |target, x, y| {
        Some(Attachment {
            target,
            direction: pair(x, y),
        })
    };
    let arrow = |start, end, start_attach, end_attach, head| Shape::Arrow {
        start,
        end,
        start_attach,
        end_attach,
        head,
        bend: Point::default(),
        elbow: false,
    };
    let (red, white, blue) = ([237, 79, 92], [255; 3], [16, 32, 48]);
    let from = pair(1., 2.);
    let to = pair(300., 400.125);
    let filled = Object {
        fill: vec![Paint::solid(blue)],
        ..object(2, 0.5, [0; 3], 1., Shape::Ellipse(rect(-5.5, 0., 40., 30.)))
    };
    Scene {
        width: 1920,
        height: 1080,
        origin: (-1920, 0),
        region: rect(100.5, 200., 900., 700.25),
        background: vec![Paint::solid([28, 31, 40])],
        objects: vec![
            object(1, 0., red, 3., Shape::Rectangle(rect(10., 20., 110., 220.))),
            filled,
            object(
                4,
                0.,
                white,
                12.,
                arrow(from, to, attach(1, 0.5, -1.), None, true),
            ),
            object(
                5,
                0.,
                white,
                12.,
                arrow(pair(0., 0.), pair(10., 0.), None, attach(2, 0., 1.), false),
            ),
            object(
                7,
                0.,
                blue,
                2.,
                Shape::Stroke {
                    points: vec![pair(0., 0.), pair(1.5, 2.), pair(3., 4.)],
                    widths: vec![2., 2.5],
                    highlight: false,
                },
            ),
            object(
                8,
                0.,
                blue,
                2.,
                Shape::Stroke {
                    points: vec![pair(7., 7.)],
                    widths: Vec::new(),
                    highlight: true,
                },
            ),
            object(
                9,
                1.25,
                red,
                3.,
                Shape::Text {
                    position: pair(50., 60.),
                    text: "Héllo\r\nwörld".into(),
                    size: 24.,
                    extent: pair(120.5, 64.),
                    label: None,
                    ink: None,
                },
            ),
        ],
        last_id: 9,
    }
}
/// Version 1 as it was first written. Files with this text exist: it never changes.
const VERSION_1: &str = "\
frame 1920 1080
origin -1920 0
region 100.5 200 900 700.25
last 9
background 1c1f28
object id=1 kind=rectangle color=ed4f5c width=3 rotation=0 rect=10,20,110,220
object id=2 kind=ellipse color=000000 width=1 rotation=0.5 fill=102030 rect=-5.5,0,40,30
object id=4 kind=arrow color=ffffff width=12 rotation=0 from=1,2 to=300,400.125 head=1 start=1,0.5,-1
object id=5 kind=arrow color=ffffff width=12 rotation=0 from=0,0 to=10,0 head=0 end=2,0,1
object id=7 kind=stroke color=102030 width=2 rotation=0 highlight=0 points=0,0;1.5,2;3,4 widths=2;2.5
object id=8 kind=stroke color=102030 width=2 rotation=0 highlight=1 points=7,7
object id=9 kind=text color=ed4f5c width=3 rotation=1.25 at=50,60 size=24 extent=120.5,64 text=48c3a96c6c6f0d0a77c3b6726c64
";

#[test]
fn version_1_text_is_frozen() {
    assert_eq!(sample().version(), 1);
    assert_eq!(sample().encode(), VERSION_1);
    assert_eq!(decode(VERSION_1, 1), Some(sample()));
    assert_eq!(
        decode(VERSION_1, 2),
        Some(sample()),
        "version 2 reads it unchanged"
    );
}
#[test]
fn awkward_numbers_come_back_exactly() {
    let mut scene = sample();
    let values = [0.1 + 0.2, 1e-7, 1e21, -0.0, f64::MIN_POSITIVE, 1. / 3.];
    scene.objects[4].shape = Shape::Stroke {
        points: values.iter().map(|v| pair(*v, -*v)).collect(),
        widths: values.iter().map(|v| v.abs()).collect(),
        highlight: false,
    };
    let back = decode(&scene.encode(), 1).unwrap();
    assert_eq!(back, scene);
    let Shape::Stroke { points, .. } = &back.objects[4].shape else {
        panic!("stroke expected");
    };
    for (p, v) in points.iter().zip(values) {
        assert_eq!(p.x.to_bits(), v.to_bits());
    }
}
#[test]
fn an_empty_scene_is_a_scene() {
    let scene = Scene {
        objects: Vec::new(),
        last_id: 0,
        background: Vec::new(),
        ..sample()
    };
    assert_eq!(decode(&scene.encode(), 1), Some(scene));
}
#[test]
fn anything_not_fully_understood_is_refused() {
    let edits: &[(&str, &str)] = &[
        // Structure.
        ("frame 1920 1080\n", ""),
        ("frame 1920 1080\n", "frame 1920 1080 1\n"),
        ("frame 1920 1080\n", "frame 0 1080\n"),
        ("frame 1920 1080\n", "frame 40000 1080\n"),
        ("origin -1920 0", "origin -1920"),
        ("origin -1920 0", "origin -1920.5 0"),
        ("last 9\n", "last 9\nnote hello\n"),
        ("background 1c1f28", "background 1c1f2"),
        (
            "background 1c1f28\n",
            "background 1c1f28\nbackground 1c1f28\n",
        ),
        ("last 9\nbackground 1c1f28\n", "background 1c1f28\nlast 9\n"),
        ("last 9\n", "last 9\n\n"),
        ("last 9\n", "last 9\r\n"),
        ("rotation=0.5 ", "rotation=0.5  "),
        // Region.
        ("region 100.5 200", "region 2000 200"),
        ("region 100.5 200 900", "region 100.5 200 100.5"),
        ("region 100.5", "region NaN"),
        // Identifiers.
        ("last 9", "last 8"),
        ("object id=2 ", "object id=1 "),
        ("object id=2 ", "object id=4 "),
        ("object id=2 ", "object "),
        // Fields.
        ("kind=ellipse", "kind=star"),
        ("kind=ellipse", "kind=ellipse glow=ff0000"),
        ("fill=102030", "fill=1020"),
        ("fill=102030", "fill=102030 fill=102030"),
        ("kind=ellipse", "kind=ellipse kind=ellipse"),
        ("color=000000", "color=00000"),
        ("color=000000", "color=00000g"),
        ("color=000000", "color=+00000"),
        ("width=1 ", "width=-1 "),
        ("width=1 ", "width=inf "),
        ("rotation=0.5", "rotation=nan"),
        ("rect=-5.5,0,40,30", "rect=-5.5,0,40"),
        ("rect=-5.5,0,40,30", "rect=-5.5,0,40,30,1"),
        ("head=1", "head=2"),
        ("start=1,0.5,-1", "start=1,0.5"),
        ("start=1,0.5,-1", "start=-1,0.5,-1"),
        ("highlight=1 points=7,7", "highlight=1 points="),
        ("highlight=1 points=7,7", "highlight=1 points=7"),
        ("highlight=1 points=7,7", "highlight=1 points=7,7;"),
        ("widths=2;2.5", "widths=2;-2.5"),
        ("size=24", "size=7"),
        ("size=24", "size=301"),
        ("extent=120.5,64", "extent=-1,64"),
        ("text=48c3", "text=8c3"),
        ("text=48c3", "text=48ff"),
        ("text=48c3a96c6c6f0d0a77c3b6726c64", "text="),
        ("text=48c3a96c6c6f0d0a77c3b6726c64", "text=200a"),
    ];
    for (from, to) in edits {
        assert!(VERSION_1.contains(from), "{from:?} is not in the reference");
        let text = VERSION_1.replacen(from, to, 1);
        assert_eq!(decode(&text, 1), None, "{from:?} replaced by {to:?}");
    }
    assert_eq!(decode(VERSION_1.trim_end(), 1), None);
    assert_eq!(decode("", 1), None);
}
#[test]
fn sizes_are_refused_before_they_are_allocated() {
    let head = "frame 10 10\norigin 0 0\nregion 0 0 10 10\n";
    let ellipse = "kind=ellipse color=000000 width=1 rotation=0 rect=0,0,1,1";
    let crowd: String = (1..=MAX_OBJECTS + 1)
        .map(|id| format!("object id={id} {ellipse}\n"))
        .collect();
    let last = MAX_OBJECTS + 1;
    assert_eq!(decode(&format!("{head}last {last}\n{crowd}"), 1), None);
    let path = "0,0;".repeat(MAX_POINTS) + "0,0";
    let stroke = "object id=1 kind=stroke color=000000 width=1 rotation=0 highlight=0";
    assert_eq!(
        decode(&format!("{head}last 1\n{stroke} points={path}\n"), 1),
        None
    );
    let long = "61".repeat(MAX_TEXT + 1);
    let text = "object id=1 kind=text color=000000 width=1 rotation=0 at=0,0 size=24";
    assert_eq!(
        decode(&format!("{head}last 1\n{text} extent=1,1 text={long}\n"), 1),
        None
    );
    let walls = (1..=MAX_POINTS + 1)
        .map(|_| "1")
        .collect::<Vec<_>>()
        .join(",");
    let filled = "object id=1 kind=ellipse color=000000 width=1 rotation=0 rect=0,0,1,1";
    assert_eq!(
        decode(&format!("{head}last 1\n{filled} fill=000000/{walls}\n"), 2),
        None,
        "walls count against the same budget"
    );
}
/// Version 2 as it was first written: coats stacked on the capture and in a shape, each
/// sparing the closed shapes above its zone. Files with this text exist: it never changes.
const VERSION_2: &str = "\
frame 1920 1080
origin -1920 0
region 100.5 200 900 700.25
last 3
background 1c1f28;ffffff/1,3
object id=1 kind=rectangle color=ed4f5c width=3 rotation=0 fill=102030/3;00ff00 rect=10,20,110,220
object id=3 kind=ellipse color=000000 width=1 rotation=0.5 rect=-5.5,0,40,30
";
fn layered() -> Scene {
    let coat = |color, walls: &[u64]| Paint {
        color,
        walls: walls.to_vec(),
    };
    Scene {
        background: vec![coat([28, 31, 40], &[]), coat([255; 3], &[1, 3])],
        objects: vec![
            Object {
                fill: vec![coat([16, 32, 48], &[3]), coat([0, 255, 0], &[])],
                ..object(
                    1,
                    0.,
                    [237, 79, 92],
                    3.,
                    Shape::Rectangle(rect(10., 20., 110., 220.)),
                )
            },
            object(3, 0.5, [0; 3], 1., Shape::Ellipse(rect(-5.5, 0., 40., 30.))),
        ],
        last_id: 3,
        ..sample()
    }
}
#[test]
fn version_2_text_is_frozen() {
    assert_eq!(layered().version(), 2);
    assert_eq!(layered().encode(), VERSION_2);
    assert_eq!(decode(VERSION_2, 2), Some(layered()));
}
#[test]
fn version_1_knows_one_plain_coat_only() {
    assert_eq!(decode(VERSION_2, 1), None);
    let one_wall = VERSION_1.replacen("fill=102030", "fill=102030/1", 1);
    assert_eq!(decode(&one_wall, 1), None);
    assert!(decode(&one_wall, 2).is_some());
    let two_coats = VERSION_1.replacen("background 1c1f28", "background 1c1f28;1c1f28", 1);
    assert_eq!(decode(&two_coats, 1), None);
    assert!(decode(&two_coats, 2).is_some());
}
#[test]
fn coats_not_fully_understood_are_refused() {
    for (from, to) in [
        ("ffffff/1,3", "ffffff/1,4"),
        ("ffffff/1,3", "ffffff/"),
        ("ffffff/1,3", "ffffff/1,,3"),
        ("ffffff/1,3", "ffffff/1;3"),
        ("ffffff/1,3", "ffffff/-1"),
        ("ffffff/1,3", "ffffff/x"),
        ("ffffff/1,3", "ffffff/1/3"),
        ("1c1f28;", "1c1f28;;"),
        ("1c1f28;", ";"),
        ("fill=102030/3;00ff00", "fill=102030/3;"),
        ("fill=102030/3;00ff00", "fill="),
    ] {
        assert!(VERSION_2.contains(from), "{from:?} is not in the reference");
        let text = VERSION_2.replacen(from, to, 1);
        assert_eq!(decode(&text, 2), None, "{from:?} replaced by {to:?}");
    }
}
/// Version 3 as it was first written: a text fitted to its panel and one wrapped at a
/// width. Files with this text exist: it never changes.
const VERSION_3: &str = "\
frame 1920 1080
origin -1920 0
region 100.5 200 900 700.25
last 3
object id=1 kind=text color=fac042 width=3 rotation=0 at=50,60 size=24 extent=120.5,29 label=fit text=4e6f7465
object id=3 kind=text color=1c1f28 width=3 rotation=0.5 at=10,20 size=32 extent=180,77 label=wrap:200.25 text=c3876120646f69742072657374657220696369
";
fn labelled() -> Scene {
    let text = |text: &str, position, size, extent, label| Shape::Text {
        position,
        text: text.into(),
        size,
        extent,
        label: Some(label),
        ink: None,
    };
    Scene {
        background: Vec::new(),
        objects: vec![
            object(
                1,
                0.,
                [250, 192, 66],
                3.,
                text("Note", pair(50., 60.), 24., pair(120.5, 29.), Label::Fit),
            ),
            object(
                3,
                0.5,
                [28, 31, 40],
                3.,
                text(
                    "Ça doit rester ici",
                    pair(10., 20.),
                    32.,
                    pair(180., 77.),
                    Label::Wrap(200.25),
                ),
            ),
        ],
        last_id: 3,
        ..sample()
    }
}
#[test]
fn version_3_text_is_frozen() {
    assert_eq!(labelled().version(), 3);
    assert_eq!(labelled().encode(), VERSION_3);
    assert_eq!(decode(VERSION_3, 3), Some(labelled()));
    assert_eq!(
        decode(VERSION_1, 3),
        Some(sample()),
        "version 3 reads version 1"
    );
    assert_eq!(decode(VERSION_2, 3), Some(layered()), "and version 2");
}
#[test]
fn earlier_versions_know_no_label() {
    assert_eq!(decode(VERSION_3, 2), None);
    assert_eq!(decode(VERSION_3, 1), None);
    let plain = VERSION_3
        .replacen(" label=fit", "", 1)
        .replacen(" label=wrap:200.25", "", 1);
    assert!(decode(&plain, 2).is_some(), "the same texts without panels");
}
#[test]
fn labels_not_fully_understood_are_refused() {
    for (from, to) in [
        ("label=fit", "label=fits"),
        ("label=fit", "label="),
        ("label=fit", "label=fit label=fit"),
        ("label=wrap:200.25", "label=wrap:"),
        ("label=wrap:200.25", "label=wrap:-1"),
        ("label=wrap:200.25", "label=wrap:0"),
        ("label=wrap:200.25", "label=wrap:inf"),
        ("label=wrap:200.25", "label=200.25"),
    ] {
        assert!(VERSION_3.contains(from), "{from:?} is not in the reference");
        let text = VERSION_3.replacen(from, to, 1);
        assert_eq!(decode(&text, 3), None, "{from:?} replaced by {to:?}");
    }
}
/// Version 4 as it was first written: a bent arrow and a bent line. Files with this
/// text exist: it never changes.
const VERSION_4: &str = "\
frame 1920 1080
origin -1920 0
region 100.5 200 900 700.25
last 2
object id=1 kind=arrow color=ed4f5c width=3 rotation=0 from=10,20 to=210,20 head=1 bend=0.125,-0.3
object id=2 kind=arrow color=1c1f28 width=5 rotation=0 from=40,90 to=40,300.5 head=0 bend=0,0.25
";
fn bent() -> Scene {
    let arrow = |start, end, head, bend| Shape::Arrow {
        start,
        end,
        start_attach: None,
        end_attach: None,
        head,
        bend,
        elbow: false,
    };
    Scene {
        background: Vec::new(),
        objects: vec![
            object(
                1,
                0.,
                [237, 79, 92],
                3.,
                arrow(pair(10., 20.), pair(210., 20.), true, pair(0.125, -0.3)),
            ),
            object(
                2,
                0.,
                [28, 31, 40],
                5.,
                arrow(pair(40., 90.), pair(40., 300.5), false, pair(0., 0.25)),
            ),
        ],
        last_id: 2,
        ..sample()
    }
}
#[test]
fn version_4_text_is_frozen() {
    assert_eq!(bent().version(), 4);
    assert_eq!(bent().encode(), VERSION_4);
    assert_eq!(decode(VERSION_4, 4), Some(bent()));
    assert_eq!(decode(VERSION_1, 4), Some(sample()), "version 4 reads 1");
    assert_eq!(decode(VERSION_2, 4), Some(layered()), "and 2");
    assert_eq!(decode(VERSION_3, 4), Some(labelled()), "and 3");
    let mut labelled_and_bent = labelled();
    labelled_and_bent
        .objects
        .extend(bent().objects.into_iter().map(|mut o| {
            o.id += 3;
            o
        }));
    labelled_and_bent.last_id = 5;
    assert_eq!(labelled_and_bent.version(), 4, "a bend outranks a label");
}
#[test]
fn earlier_versions_know_no_bend() {
    for version in 1..4 {
        assert_eq!(decode(VERSION_4, version), None, "version {version}");
    }
    let straight = VERSION_4
        .replacen(" bend=0.125,-0.3", "", 1)
        .replacen(" bend=0,0.25", "", 1);
    assert!(decode(&straight, 1).is_some(), "the same arrows straight");
}
#[test]
fn bends_not_fully_understood_are_refused() {
    for (from, to) in [
        ("bend=0,0.25", "bend=0,0"),
        ("bend=0,0.25", "bend=0.25"),
        ("bend=0,0.25", "bend="),
        ("bend=0,0.25", "bend=0,inf"),
        ("bend=0,0.25", "bend=0,0.25 bend=0,0.25"),
        ("bend=0,0.25", "bend=0,0.25,1"),
    ] {
        assert!(VERSION_4.contains(from), "{from:?} is not in the reference");
        let text = VERSION_4.replacen(from, to, 1);
        assert_eq!(decode(&text, 4), None, "{from:?} replaced by {to:?}");
    }
}
/// Version 5 as it was first written: a panel's text in an ink of its own, beside one
/// that keeps the automatic black or white. Files with this text exist: it never changes.
const VERSION_5: &str = "frame 1920 1080
origin -1920 0
region 100.5 200 900 700.25
last 3
object id=1 kind=text color=fac042 width=3 rotation=0 at=50,60 size=24 extent=120.5,29 label=fit ink=ed4f5c text=4e6f7465
object id=3 kind=text color=1c1f28 width=3 rotation=0.5 at=10,20 size=32 extent=180,77 label=wrap:200.25 text=c3876120646f69742072657374657220696369
";
fn inked() -> Scene {
    let mut scene = labelled();
    if let Shape::Text { ink, .. } = &mut scene.objects[0].shape {
        *ink = Some([237, 79, 92]);
    }
    scene
}
#[test]
fn version_5_text_is_frozen() {
    assert_eq!(inked().version(), 5);
    assert_eq!(inked().encode(), VERSION_5);
    assert_eq!(decode(VERSION_5, 5), Some(inked()));
    assert_eq!(decode(VERSION_1, 5), Some(sample()), "version 5 reads 1");
    assert_eq!(decode(VERSION_2, 5), Some(layered()), "and 2");
    assert_eq!(
        decode(VERSION_3, 5),
        Some(labelled()),
        "and 3, automatic ink"
    );
    assert_eq!(decode(VERSION_4, 5), Some(bent()), "and 4");
    let mut inked_and_bent = inked();
    inked_and_bent
        .objects
        .extend(bent().objects.into_iter().map(|mut o| {
            o.id += 3;
            o
        }));
    inked_and_bent.last_id = 5;
    assert_eq!(inked_and_bent.version(), 5, "an ink outranks a bend");
}
#[test]
fn earlier_versions_know_no_ink() {
    for version in 1..5 {
        assert_eq!(decode(VERSION_5, version), None, "version {version}");
    }
    let automatic = VERSION_5.replacen(" ink=ed4f5c", "", 1);
    assert_eq!(decode(&automatic, 3), Some(labelled()));
}
#[test]
fn inks_not_fully_understood_are_refused() {
    for (from, to) in [
        ("ink=ed4f5c", "ink=ed4f5"),
        ("ink=ed4f5c", "ink=ed4f5c00"),
        ("ink=ed4f5c", "ink=red"),
        ("ink=ed4f5c", "ink="),
        ("ink=ed4f5c", "ink=ed4f5c ink=ed4f5c"),
        // Only a panel's text has an ink of its own.
        ("label=fit ink=ed4f5c", "ink=ed4f5c"),
    ] {
        assert!(VERSION_5.contains(from), "{from:?} is not in the reference");
        let text = VERSION_5.replacen(from, to, 1);
        assert_eq!(decode(&text, 5), None, "{from:?} replaced by {to:?}");
    }
}
/// Version 6 as it was first written: an elbow arrow pulled out of one text box into
/// another. Files with this text exist: it never changes.
const VERSION_6: &str = "frame 1920 1080
origin -1920 0
region 100.5 200 900 700.25
last 4
object id=1 kind=text color=fac042 width=3 rotation=0 at=50,60 size=24 extent=120.5,29 label=fit text=4e6f7465
object id=3 kind=text color=1c1f28 width=3 rotation=0.5 at=10,20 size=32 extent=180,77 label=wrap:200.25 text=c3876120646f69742072657374657220696369
object id=4 kind=arrow color=fac042 width=4 rotation=0 from=120,100 to=300,250 head=1 elbow=1 start=1,1,0 end=3,-1,0.5
";
fn elbowed() -> Scene {
    let mut scene = labelled();
    scene.objects.push(object(
        4,
        0.,
        [250, 192, 66],
        4.,
        Shape::Arrow {
            start: pair(120., 100.),
            end: pair(300., 250.),
            start_attach: Some(Attachment {
                target: 1,
                direction: pair(1., 0.),
            }),
            end_attach: Some(Attachment {
                target: 3,
                direction: pair(-1., 0.5),
            }),
            head: true,
            bend: Point::default(),
            elbow: true,
        },
    ));
    scene.last_id = 4;
    scene
}
#[test]
fn version_6_text_is_frozen() {
    assert_eq!(elbowed().version(), 6);
    assert_eq!(elbowed().encode(), VERSION_6);
    assert_eq!(decode(VERSION_6, 6), Some(elbowed()));
    assert_eq!(decode(VERSION_5, 6), Some(inked()), "version 6 reads 5");
    assert_eq!(decode(VERSION_4, 6), Some(bent()), "and 4");
    assert_eq!(decode(VERSION_1, 6), Some(sample()), "and 1");
}
#[test]
fn earlier_versions_know_no_elbow() {
    for version in 1..6 {
        assert_eq!(decode(VERSION_6, version), None, "version {version}");
    }
}
#[test]
fn elbows_not_fully_understood_are_refused() {
    for (from, to) in [
        ("elbow=1", "elbow=0"),
        ("elbow=1", "elbow=yes"),
        ("elbow=1", "elbow="),
        ("elbow=1", "elbow=1 elbow=1"),
        // An elbow never bends: the bend would be written and not drawn.
        ("elbow=1", "elbow=1 bend=0,0.25"),
    ] {
        assert!(VERSION_6.contains(from), "{from:?} is not in the reference");
        let text = VERSION_6.replacen(from, to, 1);
        assert_eq!(decode(&text, 6), None, "{from:?} replaced by {to:?}");
    }
}
/// Version 7 as it was first written: a blurred zone at the default strength under an
/// arrow, and a stronger one. Files with this text exist: it never changes.
const VERSION_7: &str = "frame 1920 1080
origin -1920 0
region 100.5 200 900 700.25
last 3
object id=1 kind=blur color=ed4f5c width=7 rotation=0 rect=120,220.5,480,260
object id=2 kind=arrow color=ed4f5c width=3 rotation=0 from=10,20 to=210,20 head=1
object id=3 kind=blur color=000000 width=14 rotation=0 rect=500,300,640,420
";
fn blurred() -> Scene {
    Scene {
        background: Vec::new(),
        objects: vec![
            object(
                1,
                0.,
                [237, 79, 92],
                7.,
                Shape::Blur(rect(120., 220.5, 480., 260.)),
            ),
            object(
                2,
                0.,
                [237, 79, 92],
                3.,
                Shape::Arrow {
                    start: pair(10., 20.),
                    end: pair(210., 20.),
                    start_attach: None,
                    end_attach: None,
                    head: true,
                    bend: Point::default(),
                    elbow: false,
                },
            ),
            object(
                3,
                0.,
                [0; 3],
                14.,
                Shape::Blur(rect(500., 300., 640., 420.)),
            ),
        ],
        last_id: 3,
        ..sample()
    }
}
#[test]
fn version_7_text_is_frozen() {
    assert_eq!(blurred().version(), 7);
    assert_eq!(blurred().encode(), VERSION_7);
    assert_eq!(decode(VERSION_7, 7), Some(blurred()));
    assert_eq!(decode(VERSION_1, 7), Some(sample()), "version 7 reads 1");
    assert_eq!(decode(VERSION_2, 7), Some(layered()), "and 2");
    assert_eq!(decode(VERSION_3, 7), Some(labelled()), "and 3");
    assert_eq!(decode(VERSION_4, 7), Some(bent()), "and 4");
    assert_eq!(decode(VERSION_5, 7), Some(inked()), "and 5");
    assert_eq!(decode(VERSION_6, 7), Some(elbowed()), "and 6");
    let mut inked_and_blurred = inked();
    inked_and_blurred.objects.push(Object {
        id: 4,
        ..blurred().objects[0].clone()
    });
    inked_and_blurred.last_id = 4;
    assert_eq!(inked_and_blurred.version(), 7, "a blur outranks an ink");
}
#[test]
fn earlier_versions_know_no_blur() {
    for version in 1..7 {
        assert_eq!(decode(VERSION_7, version), None, "version {version}");
    }
    let without = VERSION_7
        .lines()
        .filter(|line| !line.contains("kind=blur"))
        .map(|line| format!("{line}\n"))
        .collect::<String>()
        .replacen("object id=2", "object id=1", 1)
        .replacen("last 3", "last 1", 1);
    assert!(decode(&without, 1).is_some(), "the same arrow alone");
}
#[test]
fn blurs_not_fully_understood_are_refused() {
    for (from, to) in [
        ("width=7 ", "width=0.5 "),
        ("width=7 ", "width=65 "),
        ("width=7 rotation=0 ", "width=7 rotation=0.5 "),
        ("rect=120,220.5,480,260", "rect=120,220.5,480"),
        ("rect=120,220.5,480,260", "rect=120,220.5,480,inf"),
        ("rect=120,220.5,480,260", ""),
        (
            "rect=120,220.5,480,260",
            "rect=120,220.5,480,260 fill=102030",
        ),
        ("rect=120,220.5,480,260", "rect=120,220.5,480,260 head=1"),
        ("kind=blur", "kind=blurred"),
    ] {
        assert!(VERSION_7.contains(from), "{from:?} is not in the reference");
        let text = VERSION_7.replacen(from, to, 1);
        assert_eq!(decode(&text, 7), None, "{from:?} replaced by {to:?}");
    }
}
/// Version 8 as it was first written: a box validated without text, which keeps the
/// size it was drawn at. Files with this text exist: it never changes.
const VERSION_8: &str = "frame 1920 1080
origin -1920 0
region 100.5 200 900 700.25
last 5
object id=1 kind=text color=fac042 width=3 rotation=0 at=50,60 size=24 extent=120.5,29 label=fit text=4e6f7465
object id=3 kind=text color=1c1f28 width=3 rotation=0.5 at=10,20 size=32 extent=180,77 label=wrap:200.25 text=c3876120646f69742072657374657220696369
object id=5 kind=text color=fac042 width=3 rotation=0 at=400,300 size=24 extent=150,90.5 label=wrap:150 text=
";
fn emptied() -> Scene {
    let mut scene = labelled();
    scene.objects.push(object(
        5,
        0.,
        [250, 192, 66],
        3.,
        Shape::Text {
            position: pair(400., 300.),
            text: String::new(),
            size: 24.,
            extent: pair(150., 90.5),
            label: Some(Label::Wrap(150.)),
            ink: None,
        },
    ));
    scene.last_id = 5;
    scene
}
#[test]
fn version_8_text_is_frozen() {
    assert_eq!(emptied().version(), 8);
    assert_eq!(emptied().encode(), VERSION_8);
    assert_eq!(decode(VERSION_8, 8), Some(emptied()));
    assert_eq!(decode(VERSION_7, 8), Some(blurred()), "version 8 reads 7");
    assert_eq!(decode(VERSION_6, 8), Some(elbowed()), "and 6");
    assert_eq!(decode(VERSION_1, 8), Some(sample()), "and 1");
}
#[test]
fn earlier_versions_know_no_empty_box() {
    for version in 1..8 {
        assert_eq!(decode(VERSION_8, version), None, "version {version}");
    }
}
/// Version 9 as it was first written: a pen loop filled by the bucket, spared by the
/// capture's colour. Files with this text exist: it never changes.
const VERSION_9: &str = "frame 1920 1080
origin -1920 0
region 100.5 200 900 700.25
last 1
background 1c1f28/1
object id=1 kind=stroke color=ed4f5c width=14 rotation=0 fill=f6a5c0 highlight=0 points=10,10;200,10;200,200;10,200;12,14
";
fn filled_loop() -> Scene {
    let corners = [
        (10., 10.),
        (200., 10.),
        (200., 200.),
        (10., 200.),
        (12., 14.),
    ];
    Scene {
        region: rect(100.5, 200., 900., 700.25),
        background: vec![Paint {
            color: [28, 31, 40],
            walls: vec![1],
        }],
        objects: vec![Object {
            fill: vec![Paint::solid([246, 165, 192])],
            ..object(
                1,
                0.,
                [237, 79, 92],
                14.,
                Shape::Stroke {
                    points: corners.iter().map(|&(x, y)| pair(x, y)).collect(),
                    widths: Vec::new(),
                    highlight: false,
                },
            )
        }],
        last_id: 1,
        ..sample()
    }
}
#[test]
fn version_9_text_is_frozen() {
    let scene = filled_loop();
    assert!(scene.objects[0].closed(), "the fixture is a loop");
    assert_eq!(scene.version(), 9);
    assert_eq!(scene.encode(), VERSION_9);
    assert_eq!(decode(VERSION_9, 9), Some(scene));
    assert_eq!(decode(VERSION_8, 9), Some(emptied()), "version 9 reads 8");
    assert_eq!(decode(VERSION_1, 9), Some(sample()), "and 1");
}
#[test]
fn earlier_versions_know_no_filled_pen_loop() {
    for version in 1..9 {
        assert_eq!(decode(VERSION_9, version), None, "version {version}");
    }
    // Spared without being filled is enough to need 9.
    let mut spared = filled_loop();
    spared.objects[0].fill.clear();
    assert_eq!(spared.version(), 9);
    spared.background[0].walls.clear();
    assert!(spared.version() < 9);
}
#[test]
fn only_a_box_may_be_empty() {
    let bare = VERSION_8.replacen(" label=wrap:150 text=", " text=", 1);
    assert_eq!(decode(&bare, 8), None, "a text without panel needs words");
    let mut scene = emptied();
    if let Shape::Text { label, .. } = &mut scene.objects[2].shape {
        *label = None;
    }
    assert!(!scene.valid());
}
#[test]
fn an_invalid_scene_is_recognised_before_it_is_written() {
    assert!(sample().valid());
    let mut stray_ink = sample();
    if let Shape::Text { ink, .. } = &mut stray_ink.objects[6].shape {
        *ink = Some([1, 2, 3]);
    }
    assert!(!stray_ink.valid(), "an ink without a panel is not written");
    let broken: [fn(&mut Scene); 8] = [
        |s| s.width = 0,
        |s| s.region.max.x = 5000.,
        |s| s.last_id = 3,
        |s| s.objects.swap(0, 1),
        |s| s.objects[0].style.width = f64::NAN,
        |s| s.objects[1].rotation = f64::INFINITY,
        |s| s.background[0].walls.push(3),
        |s| s.objects[1].fill[0].walls.push(10),
    ];
    for change in broken {
        let mut scene = sample();
        change(&mut scene);
        assert!(!scene.valid());
    }
}
