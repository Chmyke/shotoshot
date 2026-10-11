//! Small, consistent vector glyphs; no font-dependent symbol substitutions.
use super::*;

/// Glyph pen: icon-local coordinates on a 20 px grid, scaled from the glyph's origin.
struct Pen<'a, C: Canvas> {
    canvas: &'a mut C,
    origin: Point,
    scale: f64,
    ink: Ink,
}
impl<C: Canvas> Pen<'_, C> {
    fn p(&self, x: f64, y: f64) -> Point {
        self.origin
            + Point {
                x: x * self.scale,
                y: y * self.scale,
            }
    }
    fn r(&self, x: f64, y: f64) -> Point {
        Point {
            x: x * self.scale,
            y: y * self.scale,
        }
    }
    fn line(&mut self, a: (f64, f64), z: (f64, f64)) -> Result<(), C::Error> {
        let (a, z) = (self.p(a.0, a.1), self.p(z.0, z.1));
        self.canvas.line(a, z, self.ink, 1.6 * self.scale)
    }
    fn path(&mut self, points: &[(f64, f64)]) -> Result<(), C::Error> {
        for pair in points.windows(2) {
            self.line(pair[0], pair[1])?;
        }
        Ok(())
    }
    fn box_at(&mut self, x: f64, y: f64, w: f64, h: f64) -> Result<(), C::Error> {
        let r = Rect {
            min: self.p(x, y),
            max: self.p(x + w, y + h),
        };
        self.canvas.stroke_rect(r, self.ink, 1.6 * self.scale)
    }
    fn stroke(&mut self, a: Point, z: Point, rgb: [u8; 3], width: f64) -> Result<(), C::Error> {
        self.canvas.line(a, z, Ink::solid(rgb), width * self.scale)
    }
}

pub(super) fn draw<C: Canvas>(
    canvas: &mut C,
    b: &Button,
    current: [u8; 3],
    selected: bool,
    scale: f64,
) -> Result<(), C::Error> {
    let foreground = if !b.enabled {
        [100, 100, 100]
    } else if selected {
        [153, 218, 255]
    } else {
        [235, 235, 235]
    };
    glyph(
        canvas,
        b.action,
        b.bounds.min
            + Point {
                x: 5. * scale,
                y: 5. * scale,
            },
        scale,
        foreground,
        current,
    )
}

/// One action's glyph with its top-left corner at `origin`: 20 px wide at scale 1.
/// `current` is the drawing colour shown by the colour and highlighter glyphs.
pub fn glyph<C: Canvas>(
    canvas: &mut C,
    action: Action,
    origin: Point,
    scale: f64,
    foreground: [u8; 3],
    current: [u8; 3],
) -> Result<(), C::Error> {
    let mut g = Pen {
        origin,
        scale,
        canvas,
        ink: Ink::solid(foreground),
    };
    match action {
        // Drawn by the capture overlay beside the size badge.
        Action::CopyColor(_) | Action::FullScreen | Action::Dimensions | Action::SaveAs => {}
        Action::Color(_) | Action::CustomColor => {
            let rgb = if let Action::Color(rgb) = action {
                rgb
            } else {
                current
            };
            let center = g.p(10., 10.);
            let radius = g.r(8., 8.);
            g.canvas.fill_ellipse(center, radius, Ink::solid(rgb))?;
            g.canvas.stroke_ellipse(center, radius, g.ink, scale)?;
            if rgb == current || matches!(action, Action::CustomColor) {
                let contrast = if rgb.iter().map(|x| *x as u32).sum::<u32>() > 380 {
                    [15, 15, 15]
                } else {
                    [255, 255, 255]
                };
                g.stroke(g.p(6., 10.), g.p(9., 13.), contrast, 1.5)?;
                g.stroke(g.p(9., 13.), g.p(14., 7.), contrast, 1.5)?;
            }
            if matches!(action, Action::CustomColor) {
                let hues = [
                    [240, 80, 80],
                    [245, 205, 60],
                    [80, 200, 90],
                    [70, 210, 225],
                    [90, 130, 245],
                    [220, 100, 235],
                ];
                for index in 0..36 {
                    let a = index as f64 * std::f64::consts::TAU / 36.;
                    let z = (index + 1) as f64 * std::f64::consts::TAU / 36.;
                    g.stroke(
                        g.p(10. + a.cos() * 11., 10. + a.sin() * 11.),
                        g.p(10. + z.cos() * 11., 10. + z.sin() * 11.),
                        hues[index / 6],
                        2.,
                    )?;
                }
            }
        }
        Action::Tool(Tool::Select) => {
            g.path(&[(3., 1.), (17., 10.), (10., 12.), (7., 19.), (3., 1.)])?
        }
        Action::Tool(Tool::Rectangle) => g.box_at(2., 4., 16., 12.)?,
        Action::Tool(Tool::Ellipse) => {
            let (center, radius) = (g.p(10., 10.), g.r(8., 7.));
            g.canvas
                .stroke_ellipse(center, radius, g.ink, 1.6 * scale)?
        }
        Action::Tool(Tool::Line) => g.line((3., 17.), (17., 3.))?,
        Action::Tool(Tool::Arrow) => {
            let arrow = shotoshot_core::Object {
                rotation: 0.,
                fill: None,
                id: 0,
                style: shotoshot_core::Style {
                    color: foreground,
                    width: 2.4 * scale,
                },
                shape: shotoshot_core::Shape::Arrow {
                    start: g.p(3., 17.),
                    end: g.p(17., 3.),
                    start_attach: None,
                    end_attach: None,
                    head: true,
                },
            };
            g.canvas.annotations(&[arrow], None)?;
        }
        Action::Tool(Tool::Pen) => {
            g.path(&[
                (2., 18.),
                (5., 11.),
                (14., 2.),
                (18., 6.),
                (9., 15.),
                (2., 18.),
            ])?;
            g.line((5., 11.), (9., 15.))?;
            g.line((12., 4.), (16., 8.))?;
        }
        Action::Tool(Tool::Highlight) => {
            g.path(&[(5., 12.), (13., 2.), (19., 7.), (11., 16.), (5., 12.)])?;
            g.path(&[(5., 12.), (3., 17.), (8., 17.), (11., 16.)])?;
            g.stroke(g.p(1., 20.), g.p(19., 20.), current, 2.5)?;
        }
        Action::Tool(Tool::Text) => {
            g.line((3., 3.), (17., 3.))?;
            g.line((10., 3.), (10., 18.))?;
            g.line((3., 3.), (3., 6.))?;
            g.line((17., 3.), (17., 6.))?;
        }
        // An eraser block seen from above: the tilted block, the band of its sleeve, the line
        // it rubs along.
        Action::Tool(Tool::Eraser) => {
            g.path(&[(3., 13.), (11., 3.), (17., 9.), (9., 17.), (3., 13.)])?;
            g.line((7., 8.), (12., 13.))?;
            g.line((2., 19.), (18., 19.))?;
        }
        Action::Eyedropper => {
            // Narrow glass stem, cross guard and rubber bulb distinguish it from a pencil.
            g.path(&[
                (2., 18.),
                (3., 14.),
                (12., 5.),
                (15., 8.),
                (6., 17.),
                (2., 18.),
            ])?;
            g.line((10., 3.), (17., 10.))?;
            let (center, radius) = (g.p(16., 4.), g.r(3., 3.));
            g.canvas.fill_ellipse(center, radius, g.ink)?;
        }
        Action::Fill => {
            // Tilted bucket, half full, with its handle and the drop it pours.
            g.path(&[(2., 11.), (9., 4.), (16., 11.), (9., 18.), (2., 11.)])?;
            g.line((2., 11.), (16., 11.))?;
            g.line((5.5, 14.5), (12.5, 14.5))?;
            g.line((9., 4.), (6., 1.))?;
            let (center, radius) = (g.p(17.5, 16.5), g.r(1.7, 2.3));
            g.canvas.fill_ellipse(center, radius, g.ink)?;
        }
        Action::Thinner => g.line((4., 10.), (16., 10.))?,
        Action::Thicker => {
            g.line((4., 10.), (16., 10.))?;
            g.line((10., 4.), (10., 16.))?;
        }
        Action::Live => {
            g.box_at(1., 3., 18., 12.)?;
            g.line((6., 19.), (14., 19.))?;
            g.line((10., 15.), (10., 19.))?;
            g.path(&[(8., 6.), (13., 9.), (8., 12.), (8., 6.)])?;
        }
        Action::GoogleSearch => {
            // Compact multicolour G, rendered as vector segments at the icon's size.
            for (start, end, rgb) in [
                (-42_f64, -135_f64, [234, 67, 53]),
                (-135., -195., [251, 188, 5]),
                (-195., -285., [52, 168, 83]),
                (-285., -360., [66, 133, 244]),
            ] {
                for step in 0..24 {
                    let angle = |i: f64| (start + (end - start) * i / 24.).to_radians();
                    let a = angle(step as f64);
                    let z = angle(step as f64 + 1.);
                    g.stroke(
                        g.p(10. + 8. * a.cos(), 10. + 8. * a.sin()),
                        g.p(10. + 8. * z.cos(), 10. + 8. * z.sin()),
                        rgb,
                        3.4,
                    )?;
                }
            }
            g.stroke(g.p(10., 10.), g.p(18.8, 10.), [66, 133, 244], 3.4)?;
        }
        Action::Undo => {
            g.path(&[(8., 3.), (2., 8.), (8., 13.)])?;
            g.path(&[(2., 8.), (12., 8.), (17., 11.), (17., 17.)])?;
        }
        Action::Redo => {
            g.path(&[(12., 3.), (18., 8.), (12., 13.)])?;
            g.path(&[(18., 8.), (8., 8.), (3., 11.), (3., 17.)])?;
        }
        Action::Copy => {
            g.box_at(6., 6., 12., 13.)?;
            g.path(&[(3., 14.), (1., 14.), (1., 1.), (13., 1.), (13., 3.)])?;
        }
        Action::Save => {
            g.path(&[
                (2., 1.),
                (15., 1.),
                (19., 5.),
                (19., 19.),
                (2., 19.),
                (2., 1.),
            ])?;
            g.box_at(6., 1., 7., 6.)?;
            g.box_at(6., 12., 9., 7.)?;
        }
        Action::Close => {
            g.line((4., 4.), (16., 16.))?;
            g.line((16., 4.), (4., 16.))?;
        }
        // A broom: the handle, the band that holds the bristles, three bristles.
        Action::ClearBoard => {
            g.line((17., 3.), (9., 11.))?;
            g.line((6., 10.), (12., 16.))?;
            g.path(&[(7., 12.), (2., 15.), (3., 18.)])?;
            g.path(&[(9., 13.), (5., 18.), (7., 19.)])?;
            g.path(&[(11., 15.), (9., 19.), (11., 19.)])?;
        }
    }
    Ok(())
}
