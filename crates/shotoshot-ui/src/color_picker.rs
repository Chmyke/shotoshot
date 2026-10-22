//! ROLE: Hue wheel and saturation/value picker for annotation colors.
//! OWNER: The capture session owns the picker and commits or cancels its result.
//! INVARIANTS: HSV stays normalized; dragging is explicit.
//! HOTPATH: Pointer movement changes only HSV; the canvas caches both rasters, and the
//! square is rebuilt only when the hue changes.
use crate::{Canvas, Ink};
use shotoshot_core::{Point, Rect};
use shotoshot_text::{Key, Language, Text, language, tr_in};
use std::f64::consts::TAU;

const WIDTH: f64 = 260.;
const HEIGHT: f64 = 316.;
const CENTER: Point = Point { x: 130., y: 144. };
const OUTER_RADIUS: f64 = 100.;
const INNER_RADIUS: f64 = 82.;
const SV: Rect = Rect {
    min: Point { x: 75., y: 89. },
    max: Point { x: 185., y: 199. },
};
/// Left edge of the title and of the hint line, in design pixels.
const LABEL_X: f64 = 18.;
/// Title: font size, and the box it is drawn in until a longer text widens it.
pub const TITLE_SIZE: f64 = 15.;
pub const TITLE_WIDTH: f64 = 222.;
pub const TITLE_HEIGHT: f64 = 25.;
/// Hint line under the swatch, likewise.
pub const HINT_SIZE: f64 = 11.;
pub const HINT_WIDTH: f64 = 230.;
pub const HINT_HEIGHT: f64 = 18.;
/// Widest label the panel holds with its 12 px right margin; a longer text is shortened.
pub const MAX_LABEL_WIDTH: f64 = WIDTH - LABEL_X - 12.;
const WHEEL_PIXELS: u32 = 204;
const SV_PIXELS: u32 = 128;

#[derive(Clone, Copy)]
enum Drag {
    Hue,
    SaturationValue,
}

pub struct ColorPicker {
    bounds: Rect,
    scale: f64,
    hsv: [f64; 3],
    drag: Option<Drag>,
    hex: String,
}

impl ColorPicker {
    /// A picker at `anchor`, at interface scale `ui_scale` or smaller to fit the screen.
    pub fn new(
        rgb: [u8; 3],
        anchor: Point,
        screen_width: u32,
        screen_height: u32,
        ui_scale: f64,
    ) -> Self {
        let scale = (screen_width as f64 / WIDTH)
            .min(screen_height as f64 / HEIGHT)
            .min(ui_scale);
        let size = Point {
            x: (WIDTH * scale).min(screen_width as f64),
            y: (HEIGHT * scale).min(screen_height as f64),
        };
        let anchor = if anchor.finite() {
            anchor
        } else {
            Point::default()
        };
        let min = Point {
            x: anchor.x.clamp(0., (screen_width as f64 - size.x).max(0.)),
            y: anchor.y.clamp(0., (screen_height as f64 - size.y).max(0.)),
        };
        Self {
            bounds: Rect {
                min,
                max: min + size,
            },
            scale,
            hsv: rgb_to_hsv(rgb),
            drag: None,
            hex: format!("#{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2]),
        }
    }

    pub fn bounds(&self) -> Rect {
        self.bounds
    }
    pub fn color(&self) -> [u8; 3] {
        hsv_to_rgb(self.hsv[0], self.hsv[1], self.hsv[2])
    }

    pub fn pointer_down(&mut self, p: Point) -> bool {
        self.drag = None;
        if self.scale == 0. || !p.finite() || !self.bounds.contains(p) {
            return false;
        }
        let local = (p - self.bounds.min) * (1. / self.scale);
        let radius = (local - CENTER).length();
        self.drag = if (INNER_RADIUS..=OUTER_RADIUS).contains(&radius) {
            Some(Drag::Hue)
        } else if SV.contains(local) {
            Some(Drag::SaturationValue)
        } else {
            None
        };
        self.pointer_move(p);
        true
    }

    /// Returns whether the selection changed, including hue changes at black/white.
    pub fn pointer_move(&mut self, p: Point) -> bool {
        if !p.finite() || self.scale == 0. {
            return false;
        }
        let local = (p - self.bounds.min) * (1. / self.scale);
        let previous = self.hsv;
        match self.drag {
            Some(Drag::Hue) => {
                let delta = local - CENTER;
                if delta.length() > f64::EPSILON {
                    self.hsv[0] = (delta.y.atan2(delta.x) / TAU).rem_euclid(1.);
                }
            }
            Some(Drag::SaturationValue) => {
                self.hsv[1] = ((local.x - SV.min.x) / SV.size().x).clamp(0., 1.);
                self.hsv[2] = (1. - (local.y - SV.min.y) / SV.size().y).clamp(0., 1.);
            }
            None => return false,
        }
        if previous == self.hsv {
            return false;
        }
        let rgb = self.color();
        use std::fmt::Write;
        self.hex.clear();
        let _ = write!(self.hex, "#{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2]);
        true
    }

    pub fn pointer_up(&mut self) {
        self.drag = None;
    }

    /// Panel-local design coordinates to screen coordinates.
    pub fn at(&self, x: f64, y: f64) -> Point {
        self.bounds.min + Point { x, y } * self.scale
    }
    fn area(&self, x: f64, y: f64, width: f64, height: f64) -> Rect {
        Rect {
            min: self.at(x, y),
            max: self.at(x + width, y + height),
        }
    }

    pub fn draw<C: Canvas>(&self, canvas: &mut C) -> Result<(), C::Error> {
        if self.scale == 0. {
            return Ok(());
        }
        // Outlines keep at least their 100 % size on a picker shrunk to fit a small screen.
        let line = self.scale.max(1.);
        canvas.fill_round_rect(self.bounds, 10. * line, Ink::solid([32, 32, 32]))?;
        canvas.stroke_round_rect(self.bounds, 10. * line, Ink::solid([72, 72, 72]), line)?;
        canvas.raster(
            "picker.wheel",
            0,
            (WHEEL_PIXELS, WHEEL_PIXELS),
            &mut wheel_pixels,
            self.area(28., 42., 204., 204.),
        )?;
        let hue = self.hsv[0];
        canvas.raster(
            "picker.square",
            hue.to_bits(),
            (SV_PIXELS, SV_PIXELS),
            &mut || square_pixels(hue),
            self.area(SV.min.x, SV.min.y, SV.size().x, SV.size().y),
        )?;
        let angle = self.hsv[0] * TAU;
        let radius = (INNER_RADIUS + OUTER_RADIUS) * 0.5;
        for marker in [
            self.at(
                CENTER.x + angle.cos() * radius,
                CENTER.y + angle.sin() * radius,
            ),
            self.at(
                SV.min.x + self.hsv[1] * SV.size().x,
                SV.max.y - self.hsv[2] * SV.size().y,
            ),
        ] {
            let radius = Point {
                x: 5. * self.scale,
                y: 5. * self.scale,
            };
            let outline = Ink::with_alpha([0, 0, 0], 0.85);
            canvas.stroke_ellipse(marker, radius, outline, 4. * self.scale)?;
            canvas.stroke_ellipse(marker, radius, Ink::solid([255; 3]), 2. * self.scale)?;
        }
        let swatch = self.area(18., 258., 30., 26.);
        canvas.fill_rect(swatch, Ink::solid(self.color()))?;
        canvas.stroke_rect(swatch, Ink::solid([112, 112, 112]), line)?;
        let [title, hint] = labels_in(language());
        // Font sizes are rounded through f32 exactly as the native renderer expects.
        let font = |size: f32| (size * self.scale as f32) as f64;
        let (title_size, hint_size) = (font(TITLE_SIZE as f32), font(HINT_SIZE as f32));
        let title_width = label_width(
            TITLE_WIDTH,
            canvas.measure_text(&title, title_size)?.x,
            self.scale,
        );
        let hint_width = label_width(
            HINT_WIDTH,
            canvas.measure_text(&hint, hint_size)?.x,
            self.scale,
        );
        for (text, area, size, rgb) in [
            (
                title.as_str(),
                self.area(LABEL_X, 13., title_width, TITLE_HEIGHT),
                title_size,
                [240, 240, 240],
            ),
            (
                self.hex.as_str(),
                self.area(60., 260., 175., 25.),
                font(14.),
                [240, 240, 240],
            ),
            (
                hint.as_str(),
                self.area(LABEL_X, 294., hint_width, HINT_HEIGHT),
                hint_size,
                [174, 174, 174],
            ),
        ] {
            canvas.text(text, area, size, Ink::solid(rgb))?;
        }
        Ok(())
    }
}

/// The panel's title and hint line in `language`.
pub fn labels_in(language: Language) -> [String; 2] {
    let esc = [("shortcut", tr_in(language, Key::KeyEsc))];
    [
        Text::new_in(language, Key::ToolbarCustomColor).to_string(),
        Text::with_in(language, Key::PickerApply, &esc).to_string(),
    ]
}

/// Design width of a label box: `minimum`, or the text's width when it needs more;
/// `measured` is at the panel's `scale`.
pub fn label_width(minimum: f64, measured: f64, scale: f64) -> f64 {
    minimum.max(measured / scale)
}

fn wheel_pixels() -> Vec<u8> {
    let mut pixels = vec![0; (WHEEL_PIXELS * WHEEL_PIXELS * 4) as usize];
    for y in 0..WHEEL_PIXELS {
        for x in 0..WHEEL_PIXELS {
            let dx = x as f64 + 0.5 - WHEEL_PIXELS as f64 / 2.;
            let dy = y as f64 + 0.5 - WHEEL_PIXELS as f64 / 2.;
            let radius = dx.hypot(dy);
            let alpha = (OUTER_RADIUS + 0.5 - radius)
                .min(radius - INNER_RADIUS + 0.5)
                .clamp(0., 1.);
            let rgb = hsv_to_rgb((dy.atan2(dx) / TAU).rem_euclid(1.), 1., 1.);
            let offset = ((y * WHEEL_PIXELS + x) * 4) as usize;
            pixels[offset..offset + 4].copy_from_slice(&[
                (rgb[2] as f64 * alpha).round() as u8,
                (rgb[1] as f64 * alpha).round() as u8,
                (rgb[0] as f64 * alpha).round() as u8,
                (255. * alpha).round() as u8,
            ]);
        }
    }
    pixels
}

fn square_pixels(hue: f64) -> Vec<u8> {
    let mut pixels = Vec::with_capacity((SV_PIXELS * SV_PIXELS * 4) as usize);
    for y in 0..SV_PIXELS {
        for x in 0..SV_PIXELS {
            let rgb = hsv_to_rgb(
                hue,
                x as f64 / (SV_PIXELS - 1) as f64,
                1. - y as f64 / (SV_PIXELS - 1) as f64,
            );
            pixels.extend_from_slice(&[rgb[2], rgb[1], rgb[0], 255]);
        }
    }
    pixels
}

fn rgb_to_hsv(rgb: [u8; 3]) -> [f64; 3] {
    let [r, g, b] = rgb.map(|v| v as f64 / 255.);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;
    let hue = if delta == 0. {
        0.
    } else if max == r {
        ((g - b) / delta / 6.).rem_euclid(1.)
    } else if max == g {
        ((b - r) / delta + 2.) / 6.
    } else {
        ((r - g) / delta + 4.) / 6.
    };
    [hue, if max == 0. { 0. } else { delta / max }, max]
}

fn hsv_to_rgb(hue: f64, saturation: f64, value: f64) -> [u8; 3] {
    let sector = hue.rem_euclid(1.) * 6.;
    let chroma = value * saturation;
    let secondary = chroma * (1. - (sector % 2. - 1.).abs());
    let rgb = match sector as u8 {
        0 => [chroma, secondary, 0.],
        1 => [secondary, chroma, 0.],
        2 => [0., chroma, secondary],
        3 => [0., secondary, chroma],
        4 => [secondary, 0., chroma],
        _ => [chroma, 0., secondary],
    };
    rgb.map(|component| {
        ((component + value - chroma) * 255.)
            .round()
            .clamp(0., 255.) as u8
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_keep_their_width_unless_the_text_needs_more() {
        assert_eq!(label_width(TITLE_WIDTH, 100., 1.), TITLE_WIDTH);
        assert_eq!(label_width(TITLE_WIDTH, 225., 1.), 225.);
        // Measured at 2x, the design width is half of it.
        assert_eq!(label_width(HINT_WIDTH, 400., 2.), HINT_WIDTH);
        assert_eq!(label_width(HINT_WIDTH, 470., 2.), 235.);
        let [title, hint] = labels_in(shotoshot_text::Language::En);
        assert_eq!(title, "Custom color");
        assert_eq!(hint, "Click outside to apply · Esc to cancel");
        let [_, hint] = labels_in(shotoshot_text::Language::Fr);
        assert!(hint.contains("Échap"), "{hint}");
    }

    #[test]
    fn rgb_hsv_roundtrip_preserves_colors() {
        for r in (0..=255).step_by(17) {
            for g in (0..=255).step_by(17) {
                for b in (0..=255).step_by(17) {
                    let rgb = [r, g, b];
                    let [h, s, v] = rgb_to_hsv(rgb);
                    assert_eq!(hsv_to_rgb(h, s, v), rgb);
                }
            }
        }
    }

    #[test]
    fn panel_fits_and_drag_clamps_to_sv_limits() {
        let mut picker = ColorPicker::new([255, 0, 0], Point { x: 999., y: 999. }, 320, 350, 1.);
        let screen = Rect {
            min: Point::default(),
            max: Point { x: 320., y: 350. },
        };
        assert!(screen.contains_rect(picker.bounds()));
        assert!(!picker.pointer_down(Point { x: -1., y: -1. }));
        assert!(!picker.pointer_move(picker.at(130., 144.)));
        assert!(picker.pointer_down(picker.at(130., 144.)));
        assert!(picker.pointer_move(Point {
            x: 9999.,
            y: -9999.
        }));
        assert_eq!(picker.color(), [255, 0, 0]);
        assert!(picker.pointer_move(Point {
            x: -9999.,
            y: 9999.
        }));
        assert_eq!(picker.color(), [0, 0, 0]);
        picker.pointer_up();
        assert!(!picker.pointer_move(picker.at(185., 89.)));
        assert_eq!(picker.color(), [0, 0, 0]);
    }

    #[test]
    fn hue_survives_black_and_white() {
        for initial in [[0, 0, 0], [255, 255, 255]] {
            let mut picker = ColorPicker::new(initial, Point::default(), 1000, 1000, 1.);
            let angle = std::f64::consts::TAU / 3.;
            picker.pointer_down(picker.at(130. + 91. * angle.cos(), 144. + 91. * angle.sin()));
            picker.pointer_up();
            assert_eq!(picker.color(), initial);
            picker.pointer_down(picker.at(185., 89.));
            assert_eq!(picker.color(), [0, 255, 0]);
        }
    }

    #[test]
    fn tiny_screen_and_nonfinite_pointer_are_safe() {
        let mut picker = ColorPicker::new([40, 80, 120], Point::default(), 80, 60, 1.);
        assert!(picker.bounds().max.x <= 80. && picker.bounds().max.y <= 60.);
        assert!(!picker.pointer_down(Point { x: f64::NAN, y: 0. }));
        assert_eq!(picker.color(), [40, 80, 120]);
    }

    #[test]
    fn raster_has_transparent_center_and_exact_sv_endpoints() {
        let wheel = wheel_pixels();
        let center = ((WHEEL_PIXELS / 2 * WHEEL_PIXELS + WHEEL_PIXELS / 2) * 4) as usize;
        assert_eq!(wheel[center + 3], 0);
        let red = ((WHEEL_PIXELS / 2 * WHEEL_PIXELS + WHEEL_PIXELS / 2 + 91) * 4) as usize;
        assert_eq!(wheel[red + 2], 255);
        assert_eq!(wheel[red + 3], 255);
        let square = square_pixels(1. / 3.);
        assert_eq!(&square[..4], &[255, 255, 255, 255]);
        let top_right = ((SV_PIXELS - 1) * 4) as usize;
        assert_eq!(&square[top_right..top_right + 4], &[0, 255, 0, 255]);
        assert_eq!(&square[square.len() - 4..], &[0, 0, 0, 255]);
    }
}
