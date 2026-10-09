//! ROLE: Native regression test of the portable eyedropper loupe.
#[cfg(test)]
mod tests {
    use super::super::*;
    #[test]
    fn loupe_shows_exact_pixels_and_stays_inside_screen() {
        let _com = ComApartment::new().unwrap();
        let mut image = Image::solid(400, 300, [25, 25, 25, 255]);
        for y in 0..300usize {
            for x in 0..400usize {
                image.pixels[(y * 400 + x) * 4..(y * 400 + x) * 4 + 4].copy_from_slice(&[
                    (x % 256) as u8,
                    (y % 256) as u8,
                    ((x + y) % 256) as u8,
                    255,
                ]);
            }
        }
        let mut s = Session::new(image).unwrap();
        s.capture.region = Rect::new(Point { x: 10., y: 10. }, Point { x: 90., y: 90. });
        s.capture.selecting = false;
        s.capture.toolbar_visible = false;
        s.action(HWND::default(), Action::Eyedropper);
        s.capture.pointer = Point { x: 120.8, y: 100.2 };
        let original = s.export().unwrap().pixels;
        let preview = crate::paint_image(400, 300, |dc| s.paint(dc)).unwrap();
        let bounds = s.capture.magnifier_bounds();
        for col in [4., 5., 6.] {
            let point = bounds.min
                + Point {
                    x: 8. + col * 10. + 5.,
                    y: 63.,
                };
            let offset = ((point.y as usize) * 400 + point.x as usize) * 4;
            let source = ((100usize) * 400 + (120. + col - 5.) as usize) * 4;
            assert_eq!(
                &preview.pixels[offset..offset + 3],
                &s.capture.image.pixels[source..source + 3]
            );
        }
        crate::save_png(
            &preview,
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../target/shotoshot-eyedropper.png"),
        )
        .unwrap();
        assert_eq!(s.export().unwrap().pixels, original);
        for p in [
            Point { x: 0., y: 0. },
            Point { x: 399., y: 0. },
            Point { x: 0., y: 299. },
            Point { x: 399., y: 299. },
        ] {
            s.capture.pointer = p;
            let b = s.capture.magnifier_bounds();
            assert!(
                Rect {
                    min: Point::default(),
                    max: Point { x: 400., y: 300. }
                }
                .contains_rect(b)
            );
            assert!(!b.contains(p));
            crate::paint_image(400, 300, |dc| s.paint(dc)).unwrap();
        }
        let expected = s.capture.sample_color(s.capture.pointer);
        s.mouse_down(HWND::default(), s.capture.pointer).unwrap();
        assert_eq!(s.capture.editor.style.color, expected);
        assert!(!s.capture.eyedropper);
    }
}
