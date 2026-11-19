//! ROLE: A canvas that draws nothing and sums up every drawing call it receives: two frames
//! with the same fingerprint show the same pixels, so a host can skip repainting a window
//! whose pointer moved without changing anything on it.
//! INVARIANTS: Every argument that changes pixels enters the hash. Measurements go to the
//! real canvas, so the layout followed is the one painted. Objects enter by identity, style,
//! bounds and text, not point by point: a host only trusts a fingerprint while no gesture
//! edits the pile, and every edit outside a gesture repaints by itself.
use crate::canvas::{Canvas, Ink, TextStyle};
use shotoshot_core::{BlurPatch, Object, Point, Rect, Shape};
use std::hash::{DefaultHasher, Hash, Hasher};

/// Hashes what is drawn on it; measures through `inner`.
pub struct Fingerprint<'a, C: Canvas> {
    inner: &'a mut C,
    hash: DefaultHasher,
}

impl<'a, C: Canvas> Fingerprint<'a, C> {
    pub fn new(inner: &'a mut C) -> Self {
        Self {
            inner,
            hash: DefaultHasher::new(),
        }
    }
    /// The sum of everything drawn so far.
    pub fn finish(&self) -> u64 {
        self.hash.finish()
    }
    fn call(&mut self, name: &str) {
        name.hash(&mut self.hash);
    }
    fn f64(&mut self, v: f64) {
        v.to_bits().hash(&mut self.hash);
    }
    fn point(&mut self, p: Point) {
        self.f64(p.x);
        self.f64(p.y);
    }
    fn rect(&mut self, r: Rect) {
        self.point(r.min);
        self.point(r.max);
    }
    fn ink(&mut self, ink: Ink) {
        ink.rgb.hash(&mut self.hash);
        ink.alpha.to_bits().hash(&mut self.hash);
    }
    fn points(&mut self, points: &[Point]) {
        points.len().hash(&mut self.hash);
        for p in points {
            self.point(*p);
        }
    }
    fn object(&mut self, o: &Object) {
        o.id.hash(&mut self.hash);
        self.f64(o.rotation);
        o.style.color.hash(&mut self.hash);
        self.f64(o.style.width);
        for coat in &o.fill {
            coat.color.hash(&mut self.hash);
            coat.walls.hash(&mut self.hash);
        }
        std::mem::discriminant(&o.shape).hash(&mut self.hash);
        self.rect(o.bounds());
        if let Shape::Text { text, .. } = &o.shape {
            text.hash(&mut self.hash);
        }
    }
    fn objects(&mut self, objects: &[Object], skip: Option<u64>) {
        objects.len().hash(&mut self.hash);
        for o in objects {
            self.object(o);
        }
        skip.hash(&mut self.hash);
    }
}

impl<C: Canvas> Canvas for Fingerprint<'_, C> {
    type Error = C::Error;
    fn size(&self) -> Point {
        self.inner.size()
    }
    fn fill_rect(&mut self, r: Rect, ink: Ink) -> Result<(), Self::Error> {
        self.call("fill_rect");
        self.rect(r);
        self.ink(ink);
        Ok(())
    }
    fn fill_rect_except(
        &mut self,
        r: Rect,
        holes: &[Vec<Point>],
        ink: Ink,
    ) -> Result<(), Self::Error> {
        self.call("fill_rect_except");
        self.rect(r);
        for hole in holes {
            self.points(hole);
        }
        self.ink(ink);
        Ok(())
    }
    fn stroke_rect(&mut self, r: Rect, ink: Ink, width: f64) -> Result<(), Self::Error> {
        self.call("stroke_rect");
        self.rect(r);
        self.ink(ink);
        self.f64(width);
        Ok(())
    }
    fn fill_round_rect(&mut self, r: Rect, radius: f64, ink: Ink) -> Result<(), Self::Error> {
        self.call("fill_round_rect");
        self.rect(r);
        self.f64(radius);
        self.ink(ink);
        Ok(())
    }
    fn stroke_round_rect(
        &mut self,
        r: Rect,
        radius: f64,
        ink: Ink,
        width: f64,
    ) -> Result<(), Self::Error> {
        self.call("stroke_round_rect");
        self.rect(r);
        self.f64(radius);
        self.ink(ink);
        self.f64(width);
        Ok(())
    }
    fn fill_ellipse(&mut self, center: Point, radius: Point, ink: Ink) -> Result<(), Self::Error> {
        self.call("fill_ellipse");
        self.point(center);
        self.point(radius);
        self.ink(ink);
        Ok(())
    }
    fn stroke_ellipse(
        &mut self,
        center: Point,
        radius: Point,
        ink: Ink,
        width: f64,
    ) -> Result<(), Self::Error> {
        self.call("stroke_ellipse");
        self.point(center);
        self.point(radius);
        self.ink(ink);
        self.f64(width);
        Ok(())
    }
    fn line(&mut self, a: Point, b: Point, ink: Ink, width: f64) -> Result<(), Self::Error> {
        self.call("line");
        self.point(a);
        self.point(b);
        self.ink(ink);
        self.f64(width);
        Ok(())
    }
    fn fill_polygon(&mut self, points: &[Point], ink: Ink) -> Result<(), Self::Error> {
        self.call("fill_polygon");
        self.points(points);
        self.ink(ink);
        Ok(())
    }
    fn polyline(
        &mut self,
        points: &[Point],
        closed: bool,
        ink: Ink,
        width: f64,
    ) -> Result<(), Self::Error> {
        self.call("polyline");
        self.points(points);
        closed.hash(&mut self.hash);
        self.ink(ink);
        self.f64(width);
        Ok(())
    }
    fn text(&mut self, text: &str, r: Rect, size: f64, ink: Ink) -> Result<(), Self::Error> {
        self.styled_text(text, r, size, ink, TextStyle::Body)
    }
    fn annotation_text(
        &mut self,
        text: &str,
        r: Rect,
        size: f64,
        ink: Ink,
    ) -> Result<(), Self::Error> {
        self.call("annotation_text");
        text.hash(&mut self.hash);
        self.rect(r);
        self.f64(size);
        self.ink(ink);
        Ok(())
    }
    fn set_rotation(&mut self, rotation: Option<(Point, f64)>) -> Result<(), Self::Error> {
        self.call("set_rotation");
        if let Some((center, radians)) = rotation {
            self.point(center);
            self.f64(radians);
        }
        Ok(())
    }
    fn measure_text(&mut self, text: &str, size: f64) -> Result<Point, Self::Error> {
        self.inner.measure_text(text, size)
    }
    fn measure_annotation_text(&mut self, text: &str, size: f64) -> Result<Point, Self::Error> {
        self.inner.measure_annotation_text(text, size)
    }
    fn styled_text(
        &mut self,
        text: &str,
        r: Rect,
        size: f64,
        ink: Ink,
        style: TextStyle,
    ) -> Result<(), Self::Error> {
        self.call("text");
        text.hash(&mut self.hash);
        self.rect(r);
        self.f64(size);
        self.ink(ink);
        style.hash(&mut self.hash);
        Ok(())
    }
    fn measure_styled_text(
        &mut self,
        text: &str,
        size: f64,
        style: TextStyle,
    ) -> Result<Point, Self::Error> {
        self.inner.measure_styled_text(text, size, style)
    }
    fn raster(
        &mut self,
        key: &'static str,
        version: u64,
        size: (u32, u32),
        _pixels: &mut dyn FnMut() -> Vec<u8>,
        dst: Rect,
    ) -> Result<(), Self::Error> {
        self.call("raster");
        key.hash(&mut self.hash);
        version.hash(&mut self.hash);
        size.hash(&mut self.hash);
        self.rect(dst);
        Ok(())
    }
    fn blurs(
        &mut self,
        patches: &[BlurPatch],
        _pixels: &mut dyn FnMut(usize) -> Vec<u8>,
    ) -> Result<(), Self::Error> {
        self.call("blurs");
        for patch in patches {
            patch.id.hash(&mut self.hash);
            patch.zone.hash(&mut self.hash);
            patch.radius.hash(&mut self.hash);
        }
        Ok(())
    }
    fn annotations(&mut self, objects: &[Object], skip: Option<u64>) -> Result<(), Self::Error> {
        self.call("annotations");
        self.objects(objects, skip);
        Ok(())
    }
    fn glyph(&mut self, object: &Object) -> Result<(), Self::Error> {
        self.call("glyph");
        self.object(object);
        Ok(())
    }
    fn annotations_fading(
        &mut self,
        objects: &[Object],
        skip: Option<u64>,
        fading: &[u64],
    ) -> Result<(), Self::Error> {
        self.call("annotations_fading");
        self.objects(objects, skip);
        fading.hash(&mut self.hash);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Capture, Image, scale_tests::Recorder, test_host::FakeHost, toolbar::Action};
    use shotoshot_core::{Modifiers, Tool};

    fn recorder() -> Recorder {
        Recorder {
            size: Point { x: 400., y: 300. },
            ops: Vec::new(),
        }
    }
    fn sum(draw: impl Fn(&mut Fingerprint<Recorder>)) -> u64 {
        let mut inner = recorder();
        let mut canvas = Fingerprint::new(&mut inner);
        draw(&mut canvas);
        canvas.finish()
    }
    fn r(x: f64) -> Rect {
        Rect {
            min: Point { x, y: 0. },
            max: Point { x: x + 10., y: 10. },
        }
    }

    #[test]
    fn the_same_calls_give_the_same_sum_and_any_argument_changes_it() {
        let red = Ink::solid([255, 0, 0]);
        let a = sum(|c| {
            c.fill_rect(r(0.), red).unwrap();
            c.text("Pen", r(5.), 12., red).unwrap();
        });
        assert_eq!(
            a,
            sum(|c| {
                c.fill_rect(r(0.), red).unwrap();
                c.text("Pen", r(5.), 12., red).unwrap();
            })
        );
        for other in [
            sum(|c| {
                c.fill_rect(r(1.), red).unwrap();
                c.text("Pen", r(5.), 12., red).unwrap();
            }),
            sum(|c| {
                c.fill_rect(r(0.), Ink::with_alpha([255, 0, 0], 0.5))
                    .unwrap();
                c.text("Pen", r(5.), 12., red).unwrap();
            }),
            sum(|c| {
                c.fill_rect(r(0.), red).unwrap();
                c.text("Pan", r(5.), 12., red).unwrap();
            }),
            sum(|c| {
                c.text("Pen", r(5.), 12., red).unwrap();
                c.fill_rect(r(0.), red).unwrap();
            }),
            sum(|c| c.fill_rect(r(0.), red).unwrap()),
        ] {
            assert_ne!(a, other);
        }
    }

    #[test]
    fn a_pointer_moving_over_nothing_keeps_the_frame_and_a_hover_changes_it() {
        let mut c = Capture::new(Image::solid(400, 300, [0, 0, 0, 255]));
        c.region = Rect::new(Point::default(), Point { x: 400., y: 300. });
        c.selecting = false;
        let mut host = FakeHost::default();
        let m = Modifiers::default();
        let at = |x, y| Point { x, y };
        c.action(&mut host, Action::Tool(Tool::Rectangle));
        c.pointer_down(&mut host, at(100., 100.), m).unwrap();
        c.pointer_up(&mut host, at(200., 200.), m);
        c.action(&mut host, Action::Tool(Tool::Select));
        let frame = |c: &Capture| {
            let mut inner = recorder();
            let mut canvas = Fingerprint::new(&mut inner);
            c.draw_underlay(&mut canvas).unwrap();
            c.draw_annotations(&mut canvas, None).unwrap();
            c.draw_frame(&mut canvas, true).unwrap();
            canvas.finish()
        };
        c.pointer_move(&mut host, at(300., 40.), m);
        let empty = frame(&c);
        c.pointer_move(&mut host, at(310., 45.), m);
        assert_eq!(frame(&c), empty, "nothing under either point");
        c.pointer_move(&mut host, at(101., 150.), m);
        assert_ne!(
            frame(&c),
            empty,
            "the rectangle under the pointer is framed"
        );
        c.pointer_move(&mut host, at(300., 40.), m);
        assert_eq!(frame(&c), empty);
    }
}
