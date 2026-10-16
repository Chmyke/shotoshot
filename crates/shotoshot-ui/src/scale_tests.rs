//! The interface must scale as one piece: the same scene on a denser screen, at its scale,
//! paints exactly that much larger. A size left in raw pixels breaks this test, whichever
//! widget it hides in.
use crate::{
    Canvas, Capture, DimensionFields, Image, Ink, Monitor, draw_notice, notice_size,
    test_host::FakeHost,
    toolbar::{Action, HINT_DELAY_MS},
    top_center,
};
use shotoshot_core::{Modifiers, Object, Point, Rect, Tool};
use std::time::{Duration, Instant};

#[derive(Clone, Debug, PartialEq)]
struct Op {
    name: &'static str,
    /// Coordinates and sizes: they follow the scale.
    lengths: Vec<f64>,
    /// Colours, angles and raster identities: they do not.
    fixed: String,
}

struct Recorder {
    size: Point,
    ops: Vec<Op>,
}
impl Recorder {
    fn push(&mut self, name: &'static str, lengths: &[f64], fixed: String) -> Result<(), ()> {
        self.ops.push(Op {
            name,
            lengths: lengths.to_vec(),
            fixed,
        });
        Ok(())
    }
}
fn rect(r: Rect) -> [f64; 4] {
    [r.min.x, r.min.y, r.max.x, r.max.y]
}
impl Canvas for Recorder {
    type Error = ();
    fn size(&self) -> Point {
        self.size
    }
    fn fill_rect(&mut self, r: Rect, ink: Ink) -> Result<(), ()> {
        self.push("fill_rect", &rect(r), format!("{ink:?}"))
    }
    fn stroke_rect(&mut self, r: Rect, ink: Ink, width: f64) -> Result<(), ()> {
        let [a, b, c, d] = rect(r);
        self.push("stroke_rect", &[a, b, c, d, width], format!("{ink:?}"))
    }
    fn fill_round_rect(&mut self, r: Rect, radius: f64, ink: Ink) -> Result<(), ()> {
        let [a, b, c, d] = rect(r);
        self.push("fill_round_rect", &[a, b, c, d, radius], format!("{ink:?}"))
    }
    fn stroke_round_rect(&mut self, r: Rect, radius: f64, ink: Ink, width: f64) -> Result<(), ()> {
        let [a, b, c, d] = rect(r);
        let lengths = [a, b, c, d, radius, width];
        self.push("stroke_round_rect", &lengths, format!("{ink:?}"))
    }
    fn fill_ellipse(&mut self, center: Point, radius: Point, ink: Ink) -> Result<(), ()> {
        let lengths = [center.x, center.y, radius.x, radius.y];
        self.push("fill_ellipse", &lengths, format!("{ink:?}"))
    }
    fn stroke_ellipse(
        &mut self,
        center: Point,
        radius: Point,
        ink: Ink,
        width: f64,
    ) -> Result<(), ()> {
        let lengths = [center.x, center.y, radius.x, radius.y, width];
        self.push("stroke_ellipse", &lengths, format!("{ink:?}"))
    }
    fn line(&mut self, a: Point, b: Point, ink: Ink, width: f64) -> Result<(), ()> {
        self.push("line", &[a.x, a.y, b.x, b.y, width], format!("{ink:?}"))
    }
    fn text(&mut self, _: &str, r: Rect, size: f64, ink: Ink) -> Result<(), ()> {
        let [a, b, c, d] = rect(r);
        self.push("text", &[a, b, c, d, size], format!("{ink:?}"))
    }
    fn annotation_text(&mut self, text: &str, r: Rect, size: f64, ink: Ink) -> Result<(), ()> {
        self.text(text, r, size, ink)
    }
    fn set_rotation(&mut self, rotation: Option<(Point, f64)>) -> Result<(), ()> {
        match rotation {
            Some((p, angle)) => self.push("rotation", &[p.x, p.y], format!("{angle}")),
            None => self.push("rotation", &[], String::new()),
        }
    }
    // Width independent of the text, so scenes whose labels differ (pixel sizes) compare.
    fn measure_text(&mut self, _: &str, size: f64) -> Result<Point, ()> {
        Ok(Point {
            x: size * 4.,
            y: size * 1.25,
        })
    }
    fn measure_annotation_text(&mut self, text: &str, size: f64) -> Result<Point, ()> {
        self.measure_text(text, size)
    }
    fn raster(
        &mut self,
        key: &'static str,
        version: u64,
        size: (u32, u32),
        _: &mut dyn FnMut() -> Vec<u8>,
        dst: Rect,
    ) -> Result<(), ()> {
        self.push("raster", &rect(dst), format!("{key} {version} {size:?}"))
    }
    fn annotations(&mut self, objects: &[Object], _: Option<u64>) -> Result<(), ()> {
        self.push("annotations", &[], objects.len().to_string())
    }
    fn annotations_fading(
        &mut self,
        objects: &[Object],
        _: Option<u64>,
        fading: &[u64],
    ) -> Result<(), ()> {
        let fixed = format!("{} objects, {} fading", objects.len(), fading.len());
        self.push("annotations_fading", &[], fixed)
    }
}

#[test]
fn swept_objects_are_drawn_faded_apart_from_the_rest() {
    let mut c = Capture::new(Image::solid(400, 300, [0, 0, 0, 255]));
    c.region = Rect::new(Point::default(), Point { x: 400., y: 300. });
    c.selecting = false;
    let mut host = FakeHost::default();
    for x in [50., 150.] {
        c.action(&mut host, Action::Tool(Tool::Line));
        c.pointer_down(&mut host, Point { x, y: 10. }, Modifiers::default())
            .unwrap();
        c.pointer_up(&mut host, Point { x, y: 200. }, Modifiers::default());
    }
    c.action(&mut host, Action::Tool(Tool::Eraser));
    c.pointer_down(&mut host, Point { x: 0., y: 100. }, Modifiers::default())
        .unwrap();
    c.pointer_move(&mut host, Point { x: 100., y: 100. }, Modifiers::default());
    let mut recorder = Recorder {
        size: Point { x: 400., y: 300. },
        ops: Vec::new(),
    };
    c.draw_annotations(&mut recorder, None).unwrap();
    let drawn: Vec<(&str, &str)> = recorder
        .ops
        .iter()
        .map(|o| (o.name, o.fixed.as_str()))
        .collect();
    assert_eq!(
        drawn,
        [("annotations_fading", "2 objects, 1 fading")],
        "the whole pile goes in one call, the swept line named as fading"
    );
    c.pointer_up(&mut host, Point { x: 100., y: 100. }, Modifiers::default());
    recorder.ops.clear();
    c.draw_annotations(&mut recorder, None).unwrap();
    assert_eq!(
        recorder.ops.iter().map(|o| o.name).collect::<Vec<_>>(),
        ["annotations"],
        "nothing fades once the sweep is over"
    );
}

#[test]
fn the_loupe_swatch_shows_the_frame_pixel_under_a_zoomed_pointer() {
    let mut c = Capture::new(Image::solid(800, 300, [0, 0, 0, 255]));
    c.region = Rect::new(Point::default(), Point { x: 800., y: 300. });
    c.selecting = false;
    c.monitors = vec![
        Monitor::from(Rect::new(Point::default(), Point { x: 400., y: 300. }).unwrap()),
        Monitor::from(Rect::new(Point { x: 400., y: 0. }, Point { x: 800., y: 300. }).unwrap()),
    ];
    c.board = true;
    c.enable_views();
    let at = |x: usize, y: usize| (y * 800 + x) * 4;
    c.image.pixels[at(100, 100)..at(100, 100) + 4].copy_from_slice(&[0, 0, 255, 255]);
    c.views[0].zoom = 2.;
    c.pointer = Point { x: 200., y: 200. };
    c.eyedropper = true;
    let mut recorder = Recorder {
        size: Point { x: 800., y: 300. },
        ops: Vec::new(),
    };
    c.draw_magnifier(&mut recorder).unwrap();
    // The swatch is the one 16 px square; the cells are 10 px.
    let swatch = recorder
        .ops
        .iter()
        .find(|o| o.name == "fill_rect" && (o.lengths[2] - o.lengths[0] - 16.).abs() < 1e-9)
        .expect("the colour swatch");
    assert!(
        swatch.fixed.contains("[255, 0, 0]"),
        "the swatch shows frame (100, 100), red, not screen (200, 200): {}",
        swatch.fixed
    );
}

/// Every interface drawing of one editing scene, on a single screen at `scale`.
fn scene(scale: f64) -> Vec<Op> {
    let at = |x: f64, y: f64| Point {
        x: x * scale,
        y: y * scale,
    };
    let frame = Rect {
        min: Point::default(),
        max: at(1000., 640.),
    };
    let mut c = Capture::new(Image::solid(
        frame.max.x as u32,
        frame.max.y as u32,
        [0, 0, 0, 255],
    ));
    c.monitors = vec![Monitor {
        bounds: frame,
        scale,
    }];
    let mut host = FakeHost::default();
    let m = Modifiers::default();
    let mut canvas = Recorder {
        size: frame.max,
        ops: Vec::new(),
    };

    c.pointer_move(&mut host, at(520., 300.), m);
    c.draw_selection_hint(&mut canvas, frame).unwrap();

    c.pointer_down(&mut host, at(150., 100.), m).unwrap();
    c.pointer_up(&mut host, at(650., 420.), m);
    c.action(&mut host, Action::Tool(Tool::Rectangle));
    c.pointer_down(&mut host, at(250., 170.), m).unwrap();
    c.pointer_up(&mut host, at(400., 300.), m);
    c.action(&mut host, Action::Tool(Tool::Select));
    c.pointer_down(&mut host, at(250., 230.), m).unwrap();
    c.pointer_up(&mut host, at(250., 230.), m);
    assert_eq!(c.editor.selected.len(), 1, "the rectangle is selected");

    c.draw_underlay(&mut canvas).unwrap();
    c.draw_frame(&mut canvas, true).unwrap();
    c.draw_toolbar(&mut canvas, [1, 2, 3]).unwrap();

    let button = c.buttons()[0].bounds.center();
    c.pointer_move(&mut host, button, m);
    let now = Instant::now();
    c.hover_hint.update(c.hover_target(button), now);
    let shown = now + Duration::from_millis(HINT_DELAY_MS);
    c.draw_hover_hint(&mut canvas, shown, false).unwrap();

    c.draw_dimensions(&mut canvas, false, Some("Too large"))
        .unwrap();
    let mut fields = DimensionFields::open(&c).expect("size fields");
    c.measure_badge(Some(fields.texts()), |t, size| {
        canvas.measure_text(t, size).map(|e| e.x)
    })
    .unwrap();
    let size = fields.font_size();
    fields.place(&c, &mut |t| canvas.measure_text(t, size).unwrap());
    fields.draw(&mut canvas, true).unwrap();

    c.action(&mut host, Action::Tool(Tool::Pen));
    c.pointer_move(&mut host, at(450., 350.), m);
    c.width_preview = true;
    c.draw_width_ring(&mut canvas).unwrap();
    c.draw_magnifier(&mut canvas).unwrap();
    c.action(&mut host, Action::CustomColor);
    c.picker
        .as_ref()
        .expect("picker")
        .draw(&mut canvas)
        .unwrap();

    let size = notice_size(&mut canvas, "Could not save", scale).unwrap();
    let bounds = top_center(frame, size, scale);
    draw_notice(&mut canvas, bounds, "Could not save", scale).unwrap();
    canvas.ops
}

#[test]
fn the_whole_interface_scales_as_one_piece() {
    let base = scene(1.);
    assert!(base.len() > 300, "the scene draws every widget");
    // Whole factors keep the default 3 px stroke and 24 px text exact.
    for scale in [2., 3.] {
        let scaled = scene(scale);
        assert_eq!(scaled.len(), base.len(), "same drawings at {scale}");
        for (index, (a, b)) in base.iter().zip(&scaled).enumerate() {
            assert_eq!((a.name, &a.fixed), (b.name, &b.fixed), "drawing {index}");
            assert_eq!(a.lengths.len(), b.lengths.len(), "drawing {index}");
            for (x, y) in a.lengths.iter().zip(&b.lengths) {
                assert!(
                    (x * scale - y).abs() <= 1e-6 * y.abs().max(1.),
                    "drawing {index} ({}) at {scale}: {x} became {y}\n{a:?}\n{b:?}",
                    a.name
                );
            }
        }
    }
}
