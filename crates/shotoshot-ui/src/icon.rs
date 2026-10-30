//! ROLE: The glyphs of the tray menu and the Settings window: one per notion, shared by both.
//! OWNER: Painters call `draw` per frame; nothing is kept.
//! INVARIANTS: Drawn on the editor toolbar's 20 px grid with its 1.6 px stroke, in one colour.
use crate::canvas::{Canvas, Ink};
use crate::toolbar::{Action, glyph};
use shotoshot_core::{Point, Rect};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Icon {
    None,
    Capture,
    Instant,
    Halo,
    /// The live loupe: a lens and its handle.
    Loupe,
    Keyboard,
    Mouse,
    /// A note being written on: editing the board.
    BoardEdit,
    /// The board is shown in the wallpaper.
    Eye,
    /// The board is hidden: the same eye, struck through.
    EyeOff,
    /// The Settings window.
    Gear,
    /// The interface language.
    Globe,
    /// Quitting Shotoshot: the power symbol.
    Power,
    /// Starting with Windows: an arrow entering, the moment of signing in.
    SignIn,
    /// The General page: two sliders.
    Sliders,
    /// The Presentation page: a screen on its stand, with a chart.
    Presentation,
    /// Keyboard and mouse size: a small square and an arrow growing it.
    Size,
    /// How long temporary shapes stay.
    Clock,
    /// Moving the keyboard and mouse: arrows to the four sides.
    Move,
    /// A click shown: the pointer and its burst.
    Click,
    /// The spotlight: a bright band across the screen.
    Spotlight,
    /// The trail: a wave behind the pointer.
    Trail,
    /// The current choice of a submenu.
    Check,
    /// A submenu or a page that opens to the right.
    Chevron,
    /// The editor toolbar's glyph for the same notion.
    Toolbar(Action),
}

/// `icon` in `rgb`, its 20 px grid placed at `origin` and scaled by `scale`.
pub fn draw<C: Canvas>(
    canvas: &mut C,
    icon: Icon,
    origin: Point,
    scale: f64,
    rgb: [u8; 3],
) -> Result<(), C::Error> {
    let at = |x: f64, y: f64| {
        origin
            + Point {
                x: x * scale,
                y: y * scale,
            }
    };
    let radius = |r: f64| Point {
        x: r * scale,
        y: r * scale,
    };
    let ink = Ink::solid(rgb);
    let width = 1.6 * scale;
    let mut path = |canvas: &mut C, points: &[(f64, f64)], closed: bool| {
        let points: Vec<_> = points.iter().map(|&(x, y)| at(x, y)).collect();
        canvas.polyline(&points, closed, ink, width)
    };
    match icon {
        Icon::None => {}
        Icon::Toolbar(action) => glyph(canvas, action, origin, scale, rgb, (rgb, rgb))?,
        Icon::Capture => {
            // Corner brackets of a selected area.
            for corner in [
                [(2., 7.), (2., 2.), (7., 2.)],
                [(13., 2.), (18., 2.), (18., 7.)],
                [(18., 13.), (18., 18.), (13., 18.)],
                [(7., 18.), (2., 18.), (2., 13.)],
            ] {
                path(canvas, &corner, false)?;
            }
        }
        Icon::Instant => path(
            canvas,
            &[
                (11.5, 1.5),
                (4., 11.5),
                (9.5, 11.5),
                (8.5, 18.5),
                (16., 8.5),
                (10.5, 8.5),
            ],
            true,
        )?,
        Icon::Halo => {
            canvas.stroke_ellipse(at(10., 10.), radius(8.), ink, width)?;
            canvas.fill_ellipse(at(10., 10.), radius(2.8), ink)?;
        }
        Icon::Loupe => {
            canvas.stroke_ellipse(at(8.5, 8.5), radius(6.), ink, width)?;
            path(canvas, &[(13., 13.), (18., 18.)], false)?;
        }
        Icon::Keyboard => {
            canvas.stroke_round_rect(
                Rect {
                    min: at(1., 4.),
                    max: at(19., 16.),
                },
                2.5 * scale,
                ink,
                width,
            )?;
            for key in [
                [(4.5, 8.5), (6., 8.5)],
                [(9.25, 8.5), (10.75, 8.5)],
                [(14., 8.5), (15.5, 8.5)],
                [(6.5, 12.5), (13.5, 12.5)],
            ] {
                path(canvas, &key, false)?;
            }
        }
        Icon::Mouse => {
            canvas.stroke_round_rect(
                Rect {
                    min: at(5., 1.5),
                    max: at(15., 18.5),
                },
                5. * scale,
                ink,
                width,
            )?;
            path(canvas, &[(10., 5.), (10., 8.5)], false)?;
        }
        Icon::BoardEdit => {
            // The note, open at the corner where a pencil writes two lines of it.
            path(
                canvas,
                &[
                    (13., 8.5),
                    (13., 4.5),
                    (11., 2.5),
                    (4.5, 2.5),
                    (2.5, 4.5),
                    (2.5, 15.),
                    (4.5, 17.),
                    (8., 17.),
                ],
                false,
            )?;
            path(canvas, &[(5.5, 7.), (10., 7.)], false)?;
            path(canvas, &[(5.5, 10.5), (8.5, 10.5)], false)?;
            path(
                canvas,
                &[
                    (16.2, 9.4),
                    (18.4, 11.6),
                    (12.6, 17.4),
                    (9.8, 18.2),
                    (10.6, 15.),
                ],
                true,
            )?;
        }
        Icon::Eye | Icon::EyeOff => {
            // Two lids and the iris; struck through when the board is hidden.
            arc(&mut path, canvas, ((10., 16.), 10.), (-145., -35.))?;
            arc(&mut path, canvas, ((10., 4.), 10.), (35., 145.))?;
            canvas.stroke_ellipse(at(10., 10.), radius(2.6), ink, width)?;
            if icon == Icon::EyeOff {
                path(canvas, &[(3., 3.), (17., 17.)], false)?;
            }
        }
        Icon::Gear => {
            // A wheel with eight teeth around a hole.
            canvas.stroke_ellipse(at(10., 10.), radius(5.5), ink, width)?;
            canvas.stroke_ellipse(at(10., 10.), radius(2.), ink, width)?;
            for tooth in 0..8 {
                let angle = tooth as f64 * std::f64::consts::FRAC_PI_4;
                let (sin, cos) = angle.sin_cos();
                path(
                    canvas,
                    &[
                        (10. + 5.5 * cos, 10. + 5.5 * sin),
                        (10. + 8.5 * cos, 10. + 8.5 * sin),
                    ],
                    false,
                )?;
            }
        }
        Icon::Globe => {
            // The earth, one meridian and the equator.
            canvas.stroke_ellipse(at(10., 10.), radius(8.), ink, width)?;
            canvas.stroke_ellipse(
                at(10., 10.),
                Point {
                    x: 3.5 * scale,
                    y: 8. * scale,
                },
                ink,
                width,
            )?;
            path(canvas, &[(2., 10.), (18., 10.)], false)?;
        }
        Icon::Power => {
            arc(&mut path, canvas, ((10., 10.5), 7.5), (-55., 235.))?;
            path(canvas, &[(10., 2.), (10., 10.)], false)?;
        }
        Icon::SignIn => {
            // A door frame open to the left, and an arrow going in.
            path(
                canvas,
                &[
                    (11.5, 2.8),
                    (15.3, 2.8),
                    (17., 4.5),
                    (17., 15.5),
                    (15.3, 17.2),
                    (11.5, 17.2),
                ],
                false,
            )?;
            path(canvas, &[(2.5, 10.), (12., 10.)], false)?;
            path(canvas, &[(8.5, 6.5), (12., 10.), (8.5, 13.5)], false)?;
        }
        Icon::Sliders => {
            path(canvas, &[(3., 6.), (10., 6.)], false)?;
            path(canvas, &[(14., 6.), (17., 6.)], false)?;
            path(canvas, &[(3., 14.), (6., 14.)], false)?;
            path(canvas, &[(10., 14.), (17., 14.)], false)?;
            canvas.stroke_ellipse(at(12., 6.), radius(2.), ink, width)?;
            canvas.stroke_ellipse(at(8., 14.), radius(2.), ink, width)?;
        }
        Icon::Presentation => {
            canvas.stroke_round_rect(
                Rect {
                    min: at(2., 2.5),
                    max: at(18., 13.),
                },
                1.8 * scale,
                ink,
                width,
            )?;
            path(canvas, &[(10., 13.), (10., 15.5)], false)?;
            path(canvas, &[(6., 18.5), (10., 15.5), (14., 18.5)], false)?;
            path(
                canvas,
                &[(6., 9.5), (8.5, 7.), (10.5, 9.), (13.5, 6.)],
                false,
            )?;
        }
        Icon::Size => {
            canvas.stroke_round_rect(
                Rect {
                    min: at(2., 9.),
                    max: at(9., 16.),
                },
                1.2 * scale,
                ink,
                width,
            )?;
            path(canvas, &[(12., 3.), (17., 3.), (17., 8.)], false)?;
            path(canvas, &[(17., 3.), (10.5, 9.5)], false)?;
        }
        Icon::Clock => {
            canvas.stroke_ellipse(at(10., 10.), radius(8.), ink, width)?;
            path(canvas, &[(10., 5.5), (10., 10.), (13., 12.)], false)?;
        }
        Icon::Move => {
            path(canvas, &[(10., 2.), (10., 18.)], false)?;
            path(canvas, &[(2., 10.), (18., 10.)], false)?;
            for head in [
                [(7.5, 4.5), (10., 2.), (12.5, 4.5)],
                [(7.5, 15.5), (10., 18.), (12.5, 15.5)],
                [(4.5, 7.5), (2., 10.), (4.5, 12.5)],
                [(15.5, 7.5), (18., 10.), (15.5, 12.5)],
            ] {
                path(canvas, &head, false)?;
            }
        }
        Icon::Click => {
            path(
                canvas,
                &[(8., 8.), (17.5, 11.5), (13.5, 13.), (12., 17.)],
                true,
            )?;
            for ray in [
                [(5.2, 1.8), (6., 4.4)],
                [(1.8, 5.2), (4.4, 6.)],
                [(2.6, 10.6), (4.8, 9.)],
                [(10.6, 2.6), (9., 4.8)],
            ] {
                path(canvas, &ray, false)?;
            }
        }
        Icon::Spotlight => {
            canvas.stroke_round_rect(
                Rect {
                    min: at(1.5, 2.5),
                    max: at(18.5, 17.5),
                },
                2. * scale,
                ink,
                width,
            )?;
            path(canvas, &[(1.5, 8.), (18.5, 8.)], false)?;
            path(canvas, &[(1.5, 12.), (18.5, 12.)], false)?;
        }
        Icon::Check => path(canvas, &[(4., 10.5), (8.5, 15.), (16., 5.5)], false)?,
        Icon::Chevron => path(canvas, &[(6., 4.), (12., 10.), (6., 16.)], false)?,
        Icon::Trail => {
            path(
                canvas,
                &[
                    (2., 16.),
                    (3.8, 14.8),
                    (5., 12.),
                    (6.4, 10.4),
                    (8., 10.),
                    (9.8, 10.6),
                    (11., 12.4),
                    (12.4, 13.6),
                    (14., 14.),
                ],
                false,
            )?;
            canvas.fill_ellipse(at(16.5, 5.5), radius(2.5), ink)?;
        }
    }
    Ok(())
}

/// Clockwise arc in degrees from the positive x axis, on the glyph's 20 px grid.
fn arc<C: Canvas, P>(
    path: &mut P,
    canvas: &mut C,
    (center, radius): ((f64, f64), f64),
    (from, to): (f64, f64),
) -> Result<(), C::Error>
where
    P: FnMut(&mut C, &[(f64, f64)], bool) -> Result<(), C::Error>,
{
    let points: Vec<_> = (0..=16)
        .map(|step| {
            let angle = (from + (to - from) * step as f64 / 16.).to_radians();
            (
                center.0 + radius * angle.cos(),
                center.1 + radius * angle.sin(),
            )
        })
        .collect();
    path(canvas, &points, false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scale_tests::Recorder;
    use shotoshot_core::Tool;

    /// Every glyph the menu and Settings draw, each with its own look.
    pub(crate) const ALL: [Icon; 26] = [
        Icon::Check,
        Icon::Chevron,
        Icon::Capture,
        Icon::Instant,
        Icon::Halo,
        Icon::Loupe,
        Icon::Keyboard,
        Icon::Mouse,
        Icon::BoardEdit,
        Icon::Eye,
        Icon::EyeOff,
        Icon::Gear,
        Icon::Globe,
        Icon::Power,
        Icon::SignIn,
        Icon::Sliders,
        Icon::Presentation,
        Icon::Size,
        Icon::Clock,
        Icon::Move,
        Icon::Click,
        Icon::Spotlight,
        Icon::Trail,
        Icon::Toolbar(Action::ClearBoard),
        Icon::Toolbar(Action::Tool(Tool::Pen)),
        Icon::Toolbar(Action::Tool(Tool::Eraser)),
    ];

    fn drawing(icon: Icon) -> String {
        let mut canvas = Recorder {
            size: Point { x: 20., y: 20. },
            ops: Vec::new(),
        };
        draw(&mut canvas, icon, Point::default(), 1., [235; 3]).unwrap();
        format!("{:?}", canvas.ops)
    }

    #[test]
    fn every_glyph_draws_something_of_its_own() {
        for (index, icon) in ALL.iter().enumerate() {
            let own = drawing(*icon);
            assert!(own.len() > 2, "{icon:?} draws nothing");
            for other in &ALL[index + 1..] {
                assert_ne!(own, drawing(*other), "{icon:?} looks like {other:?}");
            }
        }
    }
}
