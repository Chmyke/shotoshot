//! Synthetic visual regression sheet using the capture/export path renderer.
use crate::*;
use shotoshot_core::*;
use windows::core::Result;

pub(super) fn render(path: &std::path::Path) -> Result<()> {
    let mut renderer = Renderer::new()?;
    renderer.set_image(&Image::solid(1360, 840, [246, 246, 246, 255]))?;
    let preview = paint_image(1360, 840, |dc| {
        renderer.begin(dc, 1360, 840)?;
        let draw = (|| -> Result<()> {
            renderer.text(
                &renderer.dc,
                "Shape scales · 2 / 8 px",
                Rect {
                    min: Point { x: 30., y: 20. },
                    max: Point { x: 1300., y: 60. },
                },
                24.,
                [30, 30, 30],
            )?;
            let mut objects = Vec::new();
            for (row, width) in [2., 8.].into_iter().enumerate() {
                let top = 80. + row as f64 * 380.;
                renderer.text(
                    &renderer.dc,
                    if row == 0 { "2 PX" } else { "8 PX" },
                    Rect {
                        min: Point { x: 30., y: top },
                        max: Point {
                            x: 1300.,
                            y: top + 25.,
                        },
                    },
                    14.,
                    [60, 60, 60],
                )?;
                let style = Style {
                    color: [40, 105, 190],
                    width,
                };
                for (index, (x, radius)) in [(60., 2.), (100., 8.), (180., 30.), (380., 140.)]
                    .into_iter()
                    .enumerate()
                {
                    let center = Point { x, y: top + 170. };
                    objects.push(Object {
                        rotation: 0.,
                        fill: Vec::new(),
                        id: (row * 20 + index + 1) as u64,
                        shape: Shape::Ellipse(Rect {
                            min: center
                                - Point {
                                    x: radius,
                                    y: radius,
                                },
                            max: center
                                + Point {
                                    x: radius,
                                    y: radius,
                                },
                        }),
                        style,
                    });
                }
                objects.push(Object {
                    rotation: 0.,
                    fill: Vec::new(),
                    id: (row * 20 + 6) as u64,
                    shape: Shape::Ellipse(Rect {
                        min: Point {
                            x: 550.,
                            y: top + 120.,
                        },
                        max: Point {
                            x: 950.,
                            y: top + 130.,
                        },
                    }),
                    style,
                });
                for (index, length) in [4., 16., 100., 600.].into_iter().enumerate() {
                    let start = Point {
                        x: 620.,
                        y: top + 175. + index as f64 * 45.,
                    };
                    objects.push(Object {
                        rotation: 0.,
                        fill: Vec::new(),
                        id: (row * 20 + 10 + index) as u64,
                        shape: Shape::Arrow {
                            start,
                            end: start + Point { x: length, y: 0. },
                            start_attach: None,
                            end_attach: None,
                            head: true,
                        },
                        style,
                    });
                }
            }
            renderer.annotations(&renderer.dc, &objects)
        })();
        let end = renderer.end();
        draw?;
        end
    })?;
    save_png(&preview, path)?;
    thick_samples(&path.with_file_name("shotoshot-thick-strokes.png"))
}

fn thick_samples(path: &std::path::Path) -> Result<()> {
    let mut renderer = Renderer::new()?;
    renderer.set_image(&Image::solid(800, 460, [25, 25, 25, 255]))?;
    let style = Style {
        width: 40.,
        color: [237, 79, 92],
    };
    let points = (0..160)
        .map(|i| {
            let t = i as f64 / 159. * std::f64::consts::TAU * 0.85;
            Point {
                x: 590. + 85. * t.cos(),
                y: 240. + 120. * t.sin(),
            }
        })
        .collect();
    let objects = vec![
        Object {
            rotation: 0.,
            fill: Vec::new(),
            id: 1,
            style,
            shape: Shape::Arrow {
                start: Point { x: 340., y: 380. },
                end: Point { x: 170., y: 120. },
                start_attach: None,
                end_attach: None,
                head: true,
            },
        },
        Object {
            rotation: 0.,
            fill: Vec::new(),
            id: 2,
            style,
            shape: Shape::Stroke {
                widths: Vec::new(),
                points,
                highlight: false,
            },
        },
    ];
    let preview = paint_image(800, 460, |dc| {
        renderer.begin(dc, 800, 460)?;
        let result = (|| -> Result<()> {
            renderer.annotations(&renderer.dc, &objects)?;
            renderer.text(
                &renderer.dc,
                "40 px · straight arrow / round pen",
                Rect {
                    min: Point { x: 30., y: 25. },
                    max: Point { x: 760., y: 65. },
                },
                22.,
                [240, 240, 240],
            )
        })();
        let end = renderer.end();
        result?;
        end
    })?;
    save_png(&preview, path)
}
