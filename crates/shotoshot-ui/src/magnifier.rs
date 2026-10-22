//! ROLE: Eyedropper loupe: exact source pixels around the pointer and the picked colour.
use crate::{Canvas, Capture, Ink};
use shotoshot_core::{Point, Rect};
use shotoshot_text::{Key, Language, Text, language, tr_in};

pub const CELLS: i32 = 11;
const ZOOM: f64 = 10.;
const WIDTH: f64 = 126.;
const HEIGHT: f64 = 164.;
/// Where the hint line starts in the loupe, in logical pixels.
const HINT_AT: Point = Point { x: 8., y: 147. };
/// Font size of the hint line, in logical pixels.
pub const HINT_SIZE: f64 = 10.;
/// Room for the hint line: the loupe keeps its size, so every language must fit it.
pub const HINT_ROOM: Point = Point {
    x: WIDTH - 2. * HINT_AT.x,
    y: HEIGHT - HINT_AT.y,
};

/// The loupe's hint line in `language`.
pub fn hint_in(language: Language) -> String {
    let esc = [("shortcut", tr_in(language, Key::KeyEsc))];
    Text::with_in(language, Key::MagnifierPick, &esc).to_string()
}

impl Capture {
    /// Beside the pointer, flipped near edges and kept inside the frame.
    pub fn magnifier_bounds(&self) -> Rect {
        let s = self.ui_scale();
        let place = |p: f64, size: f64, screen: f64| {
            let preferred = if p + 24. * s + size <= screen {
                p + 24. * s
            } else {
                p - 24. * s - size
            };
            preferred.clamp(0., (screen - size).max(0.))
        };
        let size = Point {
            x: WIDTH * s,
            y: HEIGHT * s,
        };
        let min = Point {
            x: place(self.pointer.x, size.x, self.image.width as f64),
            y: place(self.pointer.y, size.y, self.image.height as f64),
        };
        Rect {
            min,
            max: min + size,
        }
    }
    pub fn draw_magnifier<C: Canvas>(&self, canvas: &mut C) -> Result<(), C::Error> {
        let dark = Ink::solid([25, 25, 25]);
        let s = self.ui_scale();
        // Panel-local logical coordinates to frame coordinates.
        let bounds = self.magnifier_bounds();
        let at = |x: f64, y: f64| bounds.min + Point { x: x * s, y: y * s };
        let zoom = ZOOM * s;
        canvas.fill_round_rect(bounds, 6. * s, dark)?;
        let origin = at(8., 8.);
        let center = self.loupe_center();
        let cx = center.x.clamp(0., (self.image.width - 1) as f64) as i32;
        let cy = center.y.clamp(0., (self.image.height - 1) as f64) as i32;
        for row in 0..CELLS {
            for col in 0..CELLS {
                let x = cx + col - CELLS / 2;
                let y = cy + row - CELLS / 2;
                let min = origin
                    + Point {
                        x: col as f64 * zoom,
                        y: row as f64 * zoom,
                    };
                let cell = Rect {
                    min,
                    max: min + Point { x: zoom, y: zoom },
                };
                if x >= 0 && y >= 0 && x < self.image.width as i32 && y < self.image.height as i32 {
                    let rgb = self.sample_color(Point {
                        x: x as f64,
                        y: y as f64,
                    });
                    canvas.fill_rect(cell, Ink::solid(rgb))?;
                }
                canvas.stroke_rect(cell, Ink::with_alpha([0, 0, 0], 0.25), 0.5 * s)?;
            }
        }
        let center = at(58., 58.);
        let selected = Rect {
            min: center,
            max: center + Point { x: zoom, y: zoom },
        };
        canvas.stroke_rect(selected.expanded(s), dark, 3. * s)?;
        canvas.stroke_rect(selected, Ink::solid([250, 250, 250]), s)?;
        let rgb = self.sample_color(self.loupe_center());
        canvas.fill_rect(
            Rect {
                min: at(8., 126.),
                max: at(24., 142.),
            },
            Ink::solid(rgb),
        )?;
        canvas.text(
            &format!("#{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2]),
            Rect {
                min: at(32., 125.),
                max: bounds.max,
            },
            13. * s,
            Ink::solid([250, 250, 250]),
        )?;
        canvas.text(
            &hint_in(language()),
            Rect {
                min: at(HINT_AT.x, HINT_AT.y),
                max: bounds.max,
            },
            HINT_SIZE * s,
            Ink::solid([190, 190, 190]),
        )
    }
}
