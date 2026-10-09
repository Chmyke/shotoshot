#![cfg(windows)]
use shotoshot_app::{ComApartment, Image, Renderer};
use shotoshot_core::{Object, Point, Rect, Shape};
#[test]
fn exported_rotated_text_matches_quarter_turn_pixels_and_crop_translation() {
    let _com = ComApartment::new().unwrap();
    let renderer = Renderer::new().unwrap();
    let image = Image::solid(200, 200, [255; 4]);
    let object = Object {
        rotation: 0.,
        id: 1,
        style: shotoshot_core::Style {
            color: [0, 0, 0],
            width: 2.,
        },
        shape: Shape::Text {
            position: Point { x: 50., y: 80. },
            text: "MMMM".into(),
            size: 28.,
            extent: Point { x: 100., y: 40. },
        },
    };
    let crop = Rect {
        min: Point { x: 20., y: 20. },
        max: Point { x: 180., y: 180. },
    };
    let normal = renderer
        .export(&image, std::slice::from_ref(&object), crop)
        .unwrap();
    let mut rotated = object;
    rotated.rotation = std::f64::consts::FRAC_PI_2;
    let turned = renderer.export(&image, &[rotated], crop).unwrap();
    let dark_bounds = |image: &Image| {
        let mut bounds = Rect {
            min: Point { x: 1e6, y: 1e6 },
            max: Point::default(),
        };
        for y in 0..image.height {
            for x in 0..image.width {
                if image.pixels[((y * image.width + x) * 4) as usize] < 100 {
                    bounds.min.x = bounds.min.x.min(x as f64);
                    bounds.min.y = bounds.min.y.min(y as f64);
                    bounds.max.x = bounds.max.x.max(x as f64);
                    bounds.max.y = bounds.max.y.max(y as f64);
                }
            }
        }
        bounds
    };
    let a = dark_bounds(&normal);
    let b = dark_bounds(&turned);
    assert!((a.size().x - b.size().y).abs() <= 2.);
    assert!((a.size().y - b.size().x).abs() <= 2.);
    assert!((b.min.x - (159. - a.max.y)).abs() <= 2.);
    assert!((b.min.y - a.min.x).abs() <= 2.);
}
