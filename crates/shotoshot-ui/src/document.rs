//! ROLE: A capture as a saved document and back: which screens of the frame are kept, where
//! the objects land on them, and where a reopened frame goes on today's screens.
//! INVARIANTS: A document holds nothing outside its frame: no other screen, no object that
//! would be out of reach once reopened.
use crate::{Capture, Image, Monitor};
use shotoshot_core::{Editor, Object, Point, Rect, Shape, shoto::Scene};

/// Whether two rectangles share more than an edge.
fn overlap(a: Rect, b: Rect) -> bool {
    a.min.x < b.max.x && b.min.x < a.max.x && a.min.y < b.max.y && b.min.y < a.max.y
}

impl Capture {
    /// The region and every screen it touches, in whole pixels of the frame.
    fn document_frame(&self, region: Rect) -> Rect {
        let whole = Rect {
            min: Point::default(),
            max: Point {
                x: self.image.width as f64,
                y: self.image.height as f64,
            },
        };
        if self.monitors.is_empty() {
            return whole;
        }
        let region = Rect {
            min: Point {
                x: region.min.x.floor(),
                y: region.min.y.floor(),
            },
            max: Point {
                x: region.max.x.ceil(),
                y: region.max.y.ceil(),
            },
        };
        let kept = self
            .monitors
            .iter()
            .map(|m| m.bounds)
            .filter(|screen| overlap(*screen, region))
            .fold(region, Rect::union);
        Rect {
            min: self.clamp(kept.min),
            max: self.clamp(kept.max),
        }
    }
    /// What a document keeps of this capture: its scene, and the screens under the region
    /// cut out of `image`, which is the frame or the desktop grabbed for a live capture.
    pub fn document(&self, image: &Image) -> Option<(Scene, Image)> {
        let region = self.region?;
        if image.check().is_err()
            || (image.width, image.height) != (self.image.width, self.image.height)
        {
            return None;
        }
        let kept = self.document_frame(region);
        let (left, top) = (kept.min.x as usize, kept.min.y as usize);
        let (width, height) = (kept.size().x as usize, kept.size().y as usize);
        let row = image.width as usize * 4;
        let mut pixels = Vec::with_capacity(width * height * 4);
        for y in top..top + height {
            pixels.extend_from_slice(&image.pixels[y * row + left * 4..][..width * 4]);
        }
        let all = self.editor.objects();
        // Wider than any rendering of the stroke: highlighter and arrow head included.
        let reach = |o: &Object| o.resolved_bounds(all).expanded(o.style.width * 4.);
        let inside = |o: &&Object| {
            let r = reach(o);
            r.min.x <= kept.max.x
                && kept.min.x <= r.max.x
                && r.min.y <= kept.max.y
                && kept.min.y <= r.max.y
        };
        let ids: Vec<u64> = all.iter().filter(inside).map(|o| o.id).collect();
        let delta = Point::default() - kept.min;
        let moved = delta != Point::default();
        let mut objects: Vec<Object> = all
            .iter()
            .filter(inside)
            .map(|o| {
                let mut o = o.clone();
                let bound = |a: Option<shotoshot_core::Attachment>| {
                    a.is_some_and(|a| !ids.contains(&a.target))
                };
                if let Shape::Arrow {
                    start_attach,
                    end_attach,
                    ..
                } = o.shape
                    && (bound(start_attach) || bound(end_attach))
                {
                    // An end bound to an object left out stays where it is, unbound.
                    o.prepare_transform(all, &ids);
                }
                if moved {
                    o.translate(delta);
                }
                o
            })
            .collect();
        // Restacking and erasing leave the pile out of id order; a scene stores it in order.
        shotoshot_core::renumber(&mut objects, 1);
        let scene = Scene {
            width: width as u32,
            height: height as u32,
            origin: (image.left + left as i32, image.top + top as i32),
            region: Rect {
                min: region.min + delta,
                max: region.max + delta,
            },
            background: self.editor.background(),
            objects,
            last_id: self.editor.last_id(),
        };
        let frame = Image {
            width: scene.width,
            height: scene.height,
            pixels,
            left: scene.origin.0,
            top: scene.origin.1,
        };
        Some((scene, frame))
    }
    /// A capture reopened on a document: `frame` is the decoded image of the file,
    /// `monitors` today's screens over it, and `measure` the local text layout, because
    /// the font is not in the file.
    pub fn reopen(
        mut scene: Scene,
        frame: Image,
        monitors: Vec<Monitor>,
        mut measure: impl FnMut(&str, f64) -> Option<Point>,
    ) -> Option<Self> {
        if frame.check().is_err() || (frame.width, frame.height) != (scene.width, scene.height) {
            return None;
        }
        for object in &mut scene.objects {
            if let Shape::Text {
                text, size, extent, ..
            } = &mut object.shape
            {
                *extent = measure(text, *size)?;
            }
        }
        let mut capture = Self::new(frame);
        capture.editor = Editor::restore(scene.objects, scene.last_id, scene.background);
        capture.region = Some(scene.region);
        capture.selecting = false;
        capture.reopened = true;
        let whole = Rect {
            min: Point::default(),
            max: capture.clamp(Point {
                x: f64::INFINITY,
                y: f64::INFINITY,
            }),
        };
        capture.monitors = monitors
            .into_iter()
            .filter(|m| overlap(m.bounds, whole))
            .collect();
        Some(capture)
    }
}

/// Top-left corner of a reopened frame of `size` on today's `screens`, in desktop
/// coordinates: back at `origin` if it still covers exactly the screens under it, else on a
/// screen of its size, else centred where it fits. `None` when no screen shows it whole.
pub fn placement(size: Point, origin: Point, screens: &[Rect], pointer: Point) -> Option<Point> {
    let home = Rect {
        min: origin,
        max: origin + size,
    };
    let under = screens.iter().copied().filter(|s| overlap(*s, home));
    if under.reduce(Rect::union) == Some(home) {
        return Some(origin);
    }
    let pointed = screens.iter().copied().find(|s| s.contains(pointer));
    let same = |s: &Rect| s.size() == size;
    if let Some(screen) = pointed
        .filter(same)
        .or_else(|| screens.iter().copied().find(same))
    {
        return Some(screen.min);
    }
    let desktop = screens.iter().copied().reduce(Rect::union);
    [pointed.or(screens.first().copied()), desktop]
        .into_iter()
        .flatten()
        .find(|s| s.size().x >= size.x && s.size().y >= size.y)
        .map(|s| {
            let corner = s.min + (s.size() - size) * 0.5;
            Point {
                x: corner.x.floor(),
                y: corner.y.floor(),
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{keys::KeyOutcome, test_host::FakeHost, toolbar::Action};
    use shotoshot_core::{Modifiers, Tool};

    fn point(x: f64, y: f64) -> Point {
        Point { x, y }
    }
    fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Rect {
        Rect {
            min: point(x0, y0),
            max: point(x1, y1),
        }
    }
    /// Two 40×20 screens side by side, the left one left of the desktop origin. Each pixel
    /// holds its own column and row.
    fn two_screens() -> Capture {
        let mut image = Image::solid(80, 20, [0, 0, 0, 255]);
        for (i, p) in image.pixels.as_chunks_mut::<4>().0.iter_mut().enumerate() {
            (p[0], p[1]) = ((i % 80) as u8, (i / 80) as u8);
        }
        image.left = -40;
        let mut c = Capture::new(image);
        c.monitors = vec![
            rect(0., 0., 40., 20.).into(),
            rect(40., 0., 80., 20.).into(),
        ];
        c.selecting = false;
        c
    }
    fn draw(c: &mut Capture, tool: Tool, from: Point, to: Point, modifiers: Modifiers) -> u64 {
        c.editor.tool = tool;
        c.editor.begin(from, modifiers);
        c.editor.update(to);
        c.editor.finish();
        c.editor.objects().last().unwrap().id
    }
    fn none() -> Modifiers {
        Modifiers::default()
    }

    #[test]
    fn a_single_undeclared_screen_is_kept_whole() {
        let mut c = Capture::new(Image::solid(30, 10, [1, 2, 3, 255]));
        c.region = Some(rect(4.5, 2., 9., 6.));
        let rectangle = draw(
            &mut c,
            Tool::Rectangle,
            point(5., 3.),
            point(8., 5.),
            none(),
        );
        let (scene, frame) = c.document(&c.image).unwrap();
        assert_eq!((scene.width, scene.height, scene.origin), (30, 10, (0, 0)));
        assert_eq!(scene.region, rect(4.5, 2., 9., 6.));
        assert_eq!(scene.objects, c.editor.objects());
        assert_eq!(scene.last_id, rectangle);
        assert_eq!(frame.pixels, c.image.pixels);
        assert!(scene.valid());
    }
    #[test]
    fn only_the_screen_under_the_region_is_kept() {
        let mut c = two_screens();
        c.region = Some(rect(50., 5., 70., 15.));
        let inside = draw(
            &mut c,
            Tool::Rectangle,
            point(52., 6.),
            point(60., 12.),
            none(),
        );
        let outside = draw(
            &mut c,
            Tool::Rectangle,
            point(5., 5.),
            point(20., 15.),
            none(),
        );
        let (scene, frame) = c.document(&c.image).unwrap();
        assert_eq!((scene.width, scene.height), (40, 20));
        // The right screen starts at the desktop origin.
        assert_eq!(scene.origin, (0, 0));
        assert_eq!((frame.left, frame.top), (0, 0));
        assert_eq!(scene.region, rect(10., 5., 30., 15.));
        assert_eq!(scene.objects.len(), 1);
        assert_eq!(scene.objects[0].id, inside);
        assert_eq!(
            scene.objects[0].shape,
            Shape::Rectangle(rect(12., 6., 20., 12.))
        );
        // Ids are never reused, even those of objects left out.
        assert_eq!(scene.last_id, outside);
        assert_eq!(frame.pixels.len(), 40 * 20 * 4);
        assert_eq!(frame.pixels[..2], [40, 0]);
        assert_eq!(frame.pixels[(19 * 40 + 39) * 4..][..2], [79, 19]);
        assert!(scene.valid());
    }
    #[test]
    fn a_region_across_two_screens_keeps_both() {
        let mut c = two_screens();
        c.region = Some(rect(30., 5., 50., 15.));
        let (scene, frame) = c.document(&c.image).unwrap();
        assert_eq!((scene.width, scene.origin), (80, (-40, 0)));
        assert_eq!(scene.region, rect(30., 5., 50., 15.));
        assert_eq!(frame.pixels, c.image.pixels);
    }
    #[test]
    fn a_restacked_pile_saves_in_its_order_with_its_bindings() {
        let mut c = two_screens();
        c.region = Some(rect(50., 5., 78., 15.));
        let target = draw(
            &mut c,
            Tool::Rectangle,
            point(52., 6.),
            point(60., 14.),
            none(),
        );
        let bind = Modifiers {
            ctrl: true,
            ..none()
        };
        let arrow = draw(&mut c, Tool::Arrow, point(75., 10.), point(56., 10.), bind);
        // The box goes on top: the pile no longer follows the ids.
        c.editor.selected = vec![target];
        c.editor.raise_selection(true);
        let pile: Vec<u64> = c.editor.objects().iter().map(|o| o.id).collect();
        assert_eq!(pile, [arrow, target]);
        let (scene, _) = c.document(&c.image).unwrap();
        assert!(scene.valid(), "a restacked capture still saves as editable");
        assert!(matches!(scene.objects[0].shape, Shape::Arrow { .. }));
        assert!(matches!(scene.objects[1].shape, Shape::Rectangle(_)));
        let Shape::Arrow { end_attach, .. } = scene.objects[0].shape else {
            panic!("arrow expected");
        };
        assert_eq!(
            end_attach.map(|a| a.target),
            Some(scene.objects[1].id),
            "the arrow stays bound to the box under its new number"
        );
    }
    #[test]
    fn an_arrow_bound_to_an_object_left_out_keeps_its_end_unbound() {
        let mut c = two_screens();
        c.region = Some(rect(50., 5., 70., 15.));
        let target = draw(
            &mut c,
            Tool::Rectangle,
            point(5., 4.),
            point(20., 16.),
            none(),
        );
        let bind = Modifiers {
            ctrl: true,
            ..none()
        };
        draw(&mut c, Tool::Arrow, point(60., 10.), point(12., 10.), bind);
        let Shape::Arrow { end_attach, .. } = c.editor.objects()[1].shape else {
            panic!("arrow expected");
        };
        assert_eq!(end_attach.map(|a| a.target), Some(target));
        let (scene, _) = c.document(&c.image).unwrap();
        let [arrow] = &scene.objects[..] else {
            panic!("only the arrow reaches the kept screen");
        };
        let Shape::Arrow {
            start,
            end,
            end_attach,
            ..
        } = arrow.shape
        else {
            panic!("arrow expected");
        };
        assert_eq!(end_attach, None);
        assert_eq!(start, point(20., 10.));
        // Still on the rectangle, which is now left of the kept screen.
        assert!(end.x < 0. && (end.y - 10.).abs() < 1e-9);
    }
    #[test]
    fn a_live_capture_keeps_the_grabbed_desktop_not_its_placeholder() {
        let mut c = two_screens();
        c.region = Some(rect(50., 5., 70., 15.));
        let grabbed = Image {
            left: -40,
            ..Image::solid(80, 20, [9, 9, 9, 255])
        };
        let (_, frame) = c.document(&grabbed).unwrap();
        let grey = [9, 9, 9, 255];
        assert!(frame.pixels.as_chunks::<4>().0.iter().all(|p| *p == grey));
        assert!(c.document(&Image::solid(81, 20, [0; 4])).is_none());
        c.region = None;
        assert!(c.document(&grabbed).is_none());
    }
    #[test]
    fn a_reopened_document_is_the_capture_that_was_saved() {
        let mut c = two_screens();
        c.region = Some(rect(50., 5., 70., 15.));
        draw(
            &mut c,
            Tool::Ellipse,
            point(52., 6.),
            point(60., 12.),
            none(),
        );
        c.editor
            .add_text_measured(point(55., 8.), "note".into(), point(3., 2.));
        // The bucket on bare capture: a solid background.
        c.editor.fill_at(point(75., 18.), false);
        let (scene, frame) = c.document(&c.image).unwrap();
        let screens = vec![rect(0., 0., 40., 20.).into()];
        let measure = |text: &str, size: f64| Some(point(text.len() as f64, size));
        let reopened =
            Capture::reopen(scene.clone(), frame.clone(), screens.clone(), measure).unwrap();
        assert!(reopened.reopened && !reopened.selecting && !reopened.live);
        assert_eq!(scene.background, Some(c.editor.style.color));
        assert_eq!(reopened.editor.background(), scene.background);
        assert_eq!(reopened.region, Some(scene.region));
        assert_eq!(reopened.editor.last_id(), scene.last_id);
        assert!(!reopened.editor.can_undo());
        let Shape::Text { extent, .. } = reopened.editor.objects()[1].shape else {
            panic!("text expected");
        };
        // Measured again here: the saved extent belongs to the font of the writer.
        assert_eq!(extent, point(4., 24.));
        // Saved again unchanged, it is the same document.
        let (again, pixels) = reopened.document(&reopened.image).unwrap();
        assert_eq!(pixels.pixels, frame.pixels);
        assert_eq!(again.region, scene.region);
        assert_eq!(again.objects[0], scene.objects[0]);
        assert_eq!((again.origin, again.last_id), (scene.origin, scene.last_id));
        assert_eq!(again.background, scene.background);
        let wrong = Image::solid(41, 20, [0, 0, 0, 255]);
        assert!(Capture::reopen(scene.clone(), wrong, screens.clone(), measure).is_none());
        assert!(Capture::reopen(scene, frame, screens, |_, _| None).is_none());
    }
    #[test]
    fn live_capture_is_not_offered_on_a_reopened_document() {
        let mut c = two_screens();
        c.region = Some(rect(50., 5., 70., 15.));
        let live = |c: &Capture| {
            c.buttons()
                .into_iter()
                .find(|b| b.action == Action::Live)
                .unwrap()
        };
        assert!(live(&c).enabled);
        assert_eq!(
            c.key(&mut FakeHost::default(), 113, none()),
            KeyOutcome::Action(Action::Live)
        );
        c.reopened = true;
        assert!(!live(&c).enabled);
        assert_eq!(
            c.key(&mut FakeHost::default(), 113, none()),
            KeyOutcome::Handled
        );
    }
    #[test]
    fn a_reopened_frame_goes_home_then_to_a_screen_of_its_size_then_where_it_fits() {
        let size = point(1920., 1080.);
        let left = rect(-1920., 0., 0., 1080.);
        let main = rect(0., 0., 2560., 1440.);
        let far = rect(2560., 0., 4480., 1080.);
        let on_main = point(100., 100.);
        // Home: the screen it was captured on is still there.
        assert_eq!(
            placement(size, left.min, &[main, left], on_main),
            Some(left.min)
        );
        // Home is gone: a screen of the same size, the pointer's first.
        assert_eq!(
            placement(size, point(5000., 0.), &[main, left, far], on_main),
            Some(left.min)
        );
        assert_eq!(
            placement(
                size,
                point(5000., 0.),
                &[main, left, far],
                point(3000., 10.)
            ),
            Some(far.min)
        );
        // Home now straddles two screens: not home any more.
        assert_eq!(
            placement(size, point(-960., 0.), &[main, left], on_main),
            Some(left.min)
        );
        // No screen of that size: centred on the pointer's screen.
        assert_eq!(
            placement(size, point(5000., 0.), &[main], on_main),
            Some(point(320., 180.))
        );
        // Too large for the pointer's screen: centred on the whole desktop.
        let wide = point(3000., 1000.);
        assert_eq!(
            placement(wide, point(9000., 0.), &[left, main], on_main),
            Some(point(-1180., 220.))
        );
        // A frame of two screens goes home when both are still there.
        let both = point(4480., 1440.);
        assert_eq!(
            placement(both, left.min, &[left, main], on_main),
            Some(left.min)
        );
        // Larger than everything.
        assert_eq!(
            placement(point(9000., 9000.), left.min, &[left, main], on_main),
            None
        );
        assert_eq!(placement(size, left.min, &[], on_main), None);
    }
}
