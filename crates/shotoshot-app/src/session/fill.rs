//! ROLE: Native regression test of the bucket: the frame on screen and the export agree.
#[cfg(test)]
mod tests {
    use super::super::*;
    use shotoshot_core::Modifiers;

    const SLATE: [u8; 3] = [28, 31, 40];
    const CHALK: [u8; 3] = [245, 245, 248];
    const DESK: [u8; 3] = [40, 120, 200];

    fn p(x: f64, y: f64) -> Point {
        Point { x, y }
    }
    fn rgb(image: &Image, x: usize, y: usize) -> [u8; 3] {
        let i = (y * image.width as usize + x) * 4;
        [image.pixels[i + 2], image.pixels[i + 1], image.pixels[i]]
    }
    fn session() -> Session {
        let mut s = Session::new(Image::solid(400, 300, [DESK[2], DESK[1], DESK[0], 255])).unwrap();
        s.capture.region = Rect::new(p(50., 50.), p(250., 200.));
        s.capture.selecting = false;
        s.capture.toolbar_visible = false;
        s.capture.editor.style.color = CHALK;
        s
    }
    fn draw(s: &mut Session, tool: Tool, from: Point, to: Point) {
        let e = &mut s.capture.editor;
        e.tool = tool;
        e.begin(from, Modifiers::default());
        e.update(to);
        e.finish();
    }
    fn frame(s: &mut Session) -> Image {
        crate::paint_image(400, 300, |dc| s.paint(dc)).unwrap()
    }

    #[test]
    fn a_swept_object_pales_on_screen_and_goes_on_release() -> Result<()> {
        let _com = ComApartment::new()?;
        let mut s = Session::new(Image::solid(300, 200, [0, 0, 0, 255]))?;
        s.capture.region = Rect::new(Point::default(), p(300., 200.));
        s.capture.selecting = false;
        s.capture.toolbar_visible = false;
        s.capture.editor.style.color = [255, 255, 255];
        s.capture.editor.style.width = 10.;
        draw(&mut s, Tool::Line, p(100., 20.), p(100., 180.));
        let at = |image: &Image, x: usize, y: usize| image.pixels[(y * 300 + x) * 4];
        let plain = crate::paint_image(300, 200, |dc| s.paint(dc))?;
        assert_eq!(at(&plain, 100, 100), 255, "white line");
        s.action(HWND::default(), Action::Tool(Tool::Eraser));
        s.mouse_down(HWND::default(), p(50., 100.))?;
        s.mouse_move(HWND::default(), p(150., 100.));
        let faded = crate::paint_image(300, 200, |dc| s.paint(dc))?;
        let value = at(&faded, 100, 100);
        assert!((60..=95).contains(&value), "faded to about 30 %: {value}");
        s.mouse_up(HWND::default(), p(150., 100.));
        let gone = crate::paint_image(300, 200, |dc| s.paint(dc))?;
        assert_eq!(at(&gone, 100, 100), 0, "the line is gone");
        Ok(())
    }

    #[test]
    fn slate_is_the_same_on_screen_and_in_the_export_and_follows_the_region() {
        let _com = ComApartment::new().unwrap();
        let mut s = session();
        draw(&mut s, Tool::Line, p(80., 80.), p(200., 80.));
        // This first frame leaves checkpoints taken over the captured pixels.
        assert_eq!(rgb(&frame(&mut s), 100, 150), DESK);

        s.capture.editor.style.color = SLATE;
        s.capture.editor.fill_at(p(100., 150.), false);
        let shown = frame(&mut s);
        assert_eq!(rgb(&shown, 100, 150), SLATE);
        assert_eq!(rgb(&shown, 140, 80), CHALK, "drawings stay above the slate");
        assert_ne!(
            rgb(&shown, 20, 20),
            SLATE,
            "around the region: dimmed screen"
        );
        let exported = s.export().unwrap();
        assert_eq!((exported.width, exported.height), (200, 150));
        for (x, y) in [(50, 100), (0, 0), (199, 149)] {
            assert_eq!(rgb(&exported, x, y), SLATE, "{x},{y}");
        }
        assert_eq!(rgb(&exported, 90, 30), CHALK);

        s.capture.region = Rect::new(p(50., 50.), p(350., 250.));
        assert_eq!(rgb(&frame(&mut s), 300, 220), SLATE);
        assert_eq!(rgb(&s.export().unwrap(), 280, 190), SLATE);

        s.capture.editor.undo();
        assert_eq!(rgb(&frame(&mut s), 100, 150), DESK);
        assert_eq!(rgb(&s.export().unwrap(), 50, 100), DESK);
    }

    #[test]
    fn filled_shapes_are_solid_on_screen_and_in_the_export_and_move_with_their_fill() {
        let _com = ComApartment::new().unwrap();
        let mut s = session();
        draw(&mut s, Tool::Rectangle, p(80., 80.), p(160., 160.));
        draw(&mut s, Tool::Ellipse, p(205., 120.), p(235., 120.));
        assert_eq!(rgb(&frame(&mut s), 120, 120), DESK, "hollow at first");

        s.capture.editor.style.color = SLATE;
        s.capture.editor.fill_at(p(120., 120.), false);
        s.capture.editor.fill_at(p(205., 120.), false);
        assert_eq!(s.capture.editor.background(), None);
        let exported = s.export().unwrap();
        for (image, dx, dy) in [(frame(&mut s), 0, 0), (exported, 50, 50)] {
            let at = |x: usize, y: usize| rgb(&image, x - dx, y - dy);
            assert_eq!(at(120, 120), SLATE);
            assert_eq!(at(205, 120), SLATE);
            assert_eq!(at(80, 120), CHALK, "the outline lies over the fill");
            assert_eq!(at(180, 95), DESK, "a circle is not filled to its box");
            assert_eq!(at(170, 120), DESK, "between the two shapes");
        }

        let rectangle = s.capture.editor.objects()[0].id;
        s.capture.editor.select_only(rectangle);
        s.capture.editor.nudge(p(0., 30.));
        // No selection frame or rotation stem over the pixels read below.
        s.capture.editor.selected.clear();
        let shown = frame(&mut s);
        assert_eq!(rgb(&shown, 120, 95), DESK, "nothing stays behind");
        assert_eq!(rgb(&shown, 120, 175), SLATE);
    }
}
