//! ROLE: Eyedropper loupe: exact source pixels around the pointer and the picked colour.
use crate::{Canvas, Capture, Ink};
use shotoshot_core::{Point, Rect};

pub const CELLS: i32 = 11;
const ZOOM: f64 = 10.;
const WIDTH: f64 = 126.;
const HEIGHT: f64 = 164.;

impl Capture {
    /// Beside the pointer, flipped near edges and kept inside the frame.
    pub fn magnifier_bounds(&self) -> Rect {
        let place = |p: f64, size: f64, screen: f64| {
            let preferred = if p + 24. + size <= screen {
                p + 24.
            } else {
                p - 24. - size
            };
            preferred.clamp(0., (screen - size).max(0.))
        };
        let min = Point {
            x: place(self.pointer.x, WIDTH, self.image.width as f64),
            y: place(self.pointer.y, HEIGHT, self.image.height as f64),
        };
        Rect {
            min,
            max: min
                + Point {
                    x: WIDTH,
                    y: HEIGHT,
                },
        }
    }
    pub fn draw_magnifier<C: Canvas>(&self, canvas: &mut C) -> Result<(), C::Error> {
        let dark = Ink::solid([25, 25, 25]);
        let bounds = self.magnifier_bounds();
        canvas.fill_round_rect(bounds, 6., dark)?;
        let origin = bounds.min + Point { x: 8., y: 8. };
        let cx = self.pointer.x.clamp(0., (self.image.width - 1) as f64) as i32;
        let cy = self.pointer.y.clamp(0., (self.image.height - 1) as f64) as i32;
        for row in 0..CELLS {
            for col in 0..CELLS {
                let x = cx + col - CELLS / 2;
                let y = cy + row - CELLS / 2;
                let min = origin
                    + Point {
                        x: col as f64 * ZOOM,
                        y: row as f64 * ZOOM,
                    };
                let cell = Rect {
                    min,
                    max: min + Point { x: ZOOM, y: ZOOM },
                };
                if x >= 0 && y >= 0 && x < self.image.width as i32 && y < self.image.height as i32 {
                    let rgb = self.sample_color(Point {
                        x: x as f64,
                        y: y as f64,
                    });
                    canvas.fill_rect(cell, Ink::solid(rgb))?;
                }
                canvas.stroke_rect(cell, Ink::with_alpha([0, 0, 0], 0.25), 0.5)?;
            }
        }
        let center = origin + Point { x: 50., y: 50. };
        let selected = Rect {
            min: center,
            max: center + Point { x: ZOOM, y: ZOOM },
        };
        canvas.stroke_rect(selected.expanded(1.), dark, 3.)?;
        canvas.stroke_rect(selected, Ink::solid([250, 250, 250]), 1.)?;
        let rgb = self.sample_color(self.pointer);
        canvas.fill_rect(
            Rect {
                min: bounds.min + Point { x: 8., y: 126. },
                max: bounds.min + Point { x: 24., y: 142. },
            },
            Ink::solid(rgb),
        )?;
        canvas.text(
            &format!("#{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2]),
            Rect {
                min: bounds.min + Point { x: 32., y: 125. },
                max: bounds.max,
            },
            13.,
            Ink::solid([250, 250, 250]),
        )?;
        canvas.text(
            "Click to pick · Esc",
            Rect {
                min: bounds.min + Point { x: 8., y: 147. },
                max: bounds.max,
            },
            10.,
            Ink::solid([190, 190, 190]),
        )
    }
}
