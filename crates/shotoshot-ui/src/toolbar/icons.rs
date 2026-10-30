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
    /// A stroke with round ends, like every line of the glyphs.
    fn line(&mut self, a: (f64, f64), z: (f64, f64)) -> Result<(), C::Error> {
        self.outline(&[a, z], false)
    }
    fn path(&mut self, points: &[(f64, f64)]) -> Result<(), C::Error> {
        self.outline(points, false)
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
    fn rect(&self, l: f64, t: f64, r: f64, b: f64) -> Rect {
        Rect {
            min: self.p(l, t),
            max: self.p(r, b),
        }
    }
    fn points(&self, points: &[(f64, f64)]) -> Vec<Point> {
        points.iter().map(|(x, y)| self.p(*x, *y)).collect()
    }
    /// Joined line through `points` in the glyph's ink, round at its ends and corners.
    fn outline(&mut self, points: &[(f64, f64)], closed: bool) -> Result<(), C::Error> {
        let points = self.points(points);
        self.canvas
            .polyline(&points, closed, self.ink, 1.6 * self.scale)
    }
    fn fill(&mut self, points: &[(f64, f64)], ink: Ink) -> Result<(), C::Error> {
        let points = self.points(points);
        self.canvas.fill_polygon(&points, ink)
    }
    /// Flat-ended segment of any ink and width.
    fn band(&mut self, a: (f64, f64), z: (f64, f64), ink: Ink, width: f64) -> Result<(), C::Error> {
        let (a, z) = (self.p(a.0, a.1), self.p(z.0, z.1));
        self.canvas.line(a, z, ink, width * self.scale)
    }
}

/// A paint too close to the fill under the glyph to show by itself: its drop keeps a rim.
fn faint_on(rgb: [u8; 3], ground: [u8; 3]) -> bool {
    let luma = |c: [u8; 3]| 0.2126 * c[0] as f64 + 0.7152 * c[1] as f64 + 0.0722 * c[2] as f64;
    (luma(rgb) - luma(ground)).abs() < 40.
}

/// `label` is the panel and ink of the text box the box and text colour buttons show.
pub(super) fn draw<C: Canvas>(
    canvas: &mut C,
    b: &Button,
    colors: ([u8; 3], [u8; 3]),
    label: ([u8; 3], [u8; 3]),
    (selected, swatch): (bool, Option<SwatchLook>),
    scale: f64,
) -> Result<(), C::Error> {
    let (foreground, ground) = if !b.enabled {
        ([100, 100, 100], PANEL)
    } else if selected {
        (CHOSEN_GLYPH, CHOSEN_FILL)
    } else {
        ([235, 235, 235], PANEL)
    };
    let origin = b.bounds.min
        + Point {
            x: 5. * scale,
            y: 5. * scale,
        };
    let look = Look {
        foreground,
        ground,
        swatch,
        label,
    };
    paint(canvas, b.action, origin, scale, look, colors)
}

/// How a glyph sits on its button: its ink, the fill under it, a swatch's dot and ring, and
/// the panel and ink of the text box the box and text colour buttons show.
struct Look {
    foreground: [u8; 3],
    ground: [u8; 3],
    swatch: Option<SwatchLook>,
    label: ([u8; 3], [u8; 3]),
}

/// One action's glyph with its top-left corner at `origin`: 20 px wide at scale 1.
/// `current` is the drawing colour the colour swatches mark. `tint` paints the pen and
/// highlighter tips, the highlighter's swath and the bucket's paint: the drawing colour, or the
/// one under the eyedropper while it hovers.
/// Swatches are still: the drawing colour's sits in its ring.
pub fn glyph<C: Canvas>(
    canvas: &mut C,
    action: Action,
    origin: Point,
    scale: f64,
    foreground: [u8; 3],
    (current, tint): ([u8; 3], [u8; 3]),
) -> Result<(), C::Error> {
    let look = Look {
        foreground,
        ground: PANEL,
        swatch: Some(SwatchLook::rest(motion::marks_colour(action, current))),
        label: (current, shotoshot_core::label_ink(current)),
    };
    paint(canvas, action, origin, scale, look, (current, tint))
}

fn paint<C: Canvas>(
    canvas: &mut C,
    action: Action,
    origin: Point,
    scale: f64,
    Look {
        foreground,
        ground,
        swatch,
        label: (panel_rgb, ink_rgb),
    }: Look,
    (current, tint): ([u8; 3], [u8; 3]),
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
            // The chosen colour sits smaller inside a white ring; the custom swatch's ring is
            // its rainbow.
            let look = swatch.unwrap_or(SwatchLook::rest(false));
            let center = g.p(10., 10.);
            let radius = g.r(8. * look.dot, 8. * look.dot);
            g.canvas.fill_ellipse(center, radius, Ink::solid(rgb))?;
            g.canvas.stroke_ellipse(center, radius, g.ink, scale)?;
            if let Some((alpha, size)) = look.ring
                && matches!(action, Action::Color(_))
            {
                let ring = g.r(10.2 * size, 10.2 * size);
                let white = Ink::with_alpha([255, 255, 255], alpha);
                g.canvas.stroke_ellipse(center, ring, white, 1.8 * scale)?;
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
        // A solid pointer over a dashed frame: nothing like the arrow tool's outline. The
        // dashes stop short of the pointer.
        Action::Tool(Tool::Select) => {
            for (a, z) in [
                ((1.5, 1.5), (5.25, 1.5)),
                ((7.75, 1.5), (11.5, 1.5)),
                ((14., 1.5), (14., 5.25)),
                ((14., 7.75), (14., 9.)),
                ((6.4, 14.), (4., 14.)),
                ((1.5, 14.), (1.5, 10.25)),
                ((1.5, 7.75), (1.5, 4.)),
            ] {
                g.line(a, z)?;
            }
            let pointer = [
                (8., 7.),
                (8., 18.),
                (10.9, 15.3),
                (12.8, 19.4),
                (14.6, 18.6),
                (12.7, 14.5),
                (16.6, 14.5),
            ];
            g.fill(&pointer, g.ink)?;
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
                fill: Vec::new(),
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
                    bend: Default::default(),
                    elbow: false,
                },
            };
            g.canvas.annotations(&[arrow], None)?;
        }
        // The cone takes the tint.
        Action::Tool(Tool::Pen) => {
            g.fill(&[(2., 18.), (5., 11.), (9., 15.)], Ink::solid(tint))?;
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
        // An upright marker, unlike the slanted pen: cap, barrel, chisel tip, and the swath
        // that starts from it.
        Action::Tool(Tool::Highlight) => {
            g.band((6.5, 17.), (19., 17.), Ink::with_alpha(tint, 0.6), 4.6)?;
            let tip = [(4., 10.), (9., 10.), (9., 12.5), (6.5, 15.), (4., 15.)];
            g.fill(&tip, Ink::solid(tint))?;
            g.outline(&tip, true)?;
            let barrel = g.rect(3., 1., 10., 10.);
            g.canvas
                .stroke_round_rect(barrel, 1.6 * scale, g.ink, 1.6 * scale)?;
            g.outline(&[(3., 4.5), (10., 4.5)], false)?;
        }
        // A panel of the tint with a letter on it, in the ink a label gives that panel.
        Action::Tool(Tool::Label) => {
            let panel = g.rect(1.5, 4., 18.5, 16.);
            let radius = 3. * scale;
            g.canvas.fill_round_rect(panel, radius, Ink::solid(tint))?;
            g.canvas
                .stroke_round_rect(panel, radius, g.ink, 1.6 * scale)?;
            let ink = Ink::solid(shotoshot_core::label_ink(tint));
            g.band((6., 8.), (14., 8.), ink, 1.6)?;
            g.band((10., 8.), (10., 13.), ink, 1.6)?;
        }
        // The box's own colour, a plain square, beside the text colour's letter on it.
        Action::BoxColor => {
            let panel = g.rect(3., 3., 17., 17.);
            let radius = 3. * scale;
            g.canvas
                .fill_round_rect(panel, radius, Ink::solid(panel_rgb))?;
            g.canvas
                .stroke_round_rect(panel, radius, g.ink, 1.6 * scale)?;
        }
        // An A in the text's actual ink on the box colour: the colour of my text.
        Action::TextColor => {
            let panel = g.rect(1.5, 1.5, 18.5, 18.5);
            let radius = 3. * scale;
            g.canvas
                .fill_round_rect(panel, radius, Ink::solid(panel_rgb))?;
            g.canvas
                .stroke_round_rect(panel, radius, g.ink, 1.6 * scale)?;
            let letter = g.points(&[(5.5, 15.), (10., 4.5), (14.5, 15.)]);
            let ink = Ink::solid(ink_rgb);
            g.canvas.polyline(&letter, false, ink, 2.2 * scale)?;
            g.band((7.4, 11.5), (12.6, 11.5), ink, 2.)?;
        }
        Action::Tool(Tool::Text) => {
            g.line((3., 3.), (17., 3.))?;
            g.line((10., 3.), (10., 18.))?;
            g.line((3., 3.), (3., 6.))?;
            g.line((17., 3.), (17., 6.))?;
        }
        // A tilted block, sleeve filled and rubber hollow, rubbing out a line: crumbs behind
        // it, the line still whole ahead.
        Action::Tool(Tool::Eraser) => {
            let (l, t, r, b) = (6., 5., 19., 12.);
            let sleeve = l + (r - l) * 0.45;
            let radius = 1.8 * scale;
            g.canvas
                .set_rotation(Some((g.p(12.5, 8.5), (-45_f64).to_radians())))?;
            g.canvas
                .fill_round_rect(g.rect(sleeve, t, r, b), radius, g.ink)?;
            g.canvas
                .fill_rect(g.rect(sleeve, t, sleeve + 2.5, b), g.ink)?;
            g.canvas
                .stroke_round_rect(g.rect(l, t, r, b), radius, g.ink, 1.6 * scale)?;
            g.canvas.set_rotation(None)?;
            let crumbs = Ink::with_alpha(foreground, 0.35);
            g.band((1., 18.5), (3., 18.5), crumbs, 1.6)?;
            g.band((5., 18.5), (7., 18.5), crumbs, 1.6)?;
            g.outline(&[(12., 18.5), (19., 18.5)], false)?;
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
            // Tilted bucket, half full, with its handle and the drop it pours: paint and drop
            // take the tint.
            let paint = Ink::solid(tint);
            g.fill(&[(2., 11.), (16., 11.), (9., 18.)], paint)?;
            g.outline(&[(2., 11.), (9., 4.), (16., 11.), (9., 18.)], true)?;
            g.outline(&[(2., 11.), (16., 11.)], false)?;
            g.outline(&[(9., 4.), (6., 1.)], false)?;
            let (center, radius) = (g.p(17.5, 16.5), g.r(1.8, 2.4));
            g.canvas.fill_ellipse(center, radius, paint)?;
            if faint_on(tint, ground) {
                g.canvas.stroke_ellipse(center, radius, g.ink, scale)?;
            }
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
        // A bin: the whole board goes, unlike the eraser's one object (Mickaël, 2026-10-30;
        // the broom read as a rake).
        Action::ClearBoard => {
            g.line((3., 5.5), (17., 5.5))?;
            g.path(&[(7.5, 5.5), (7.5, 2.8), (12.5, 2.8), (12.5, 5.5)])?;
            g.path(&[(4.8, 5.5), (5.8, 17.8), (14.2, 17.8), (15.2, 5.5)])?;
            g.line((8.3, 9.), (8.3, 14.5))?;
            g.line((11.7, 9.), (11.7, 14.5))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scale_tests::Recorder;

    fn record(action: Action, colors: ([u8; 3], [u8; 3])) -> Recorder {
        let mut canvas = Recorder {
            size: Point { x: 30., y: 30. },
            ops: Vec::new(),
        };
        glyph(&mut canvas, action, Point::default(), 1., [235; 3], colors).unwrap();
        canvas
    }

    #[test]
    fn tips_and_paint_take_the_tint_while_swatches_mark_the_drawing_colour() {
        let (current, tint) = ([237, 79, 92], [255, 140, 30]);
        let tinted = format!("{:?}", Ink::solid(tint));
        for action in [
            Action::Tool(Tool::Pen),
            Action::Tool(Tool::Highlight),
            Action::Fill,
        ] {
            let ops = record(action, (current, tint)).ops;
            let painted = ops
                .iter()
                .any(|op| op.name == "fill_polygon" && op.fixed == tinted);
            assert!(painted, "{action:?} shows the tint");
        }
        let circles = |rgb| {
            let ops = record(Action::Color(rgb), (current, tint)).ops;
            ops.iter().filter(|op| op.name == "stroke_ellipse").count()
        };
        assert_eq!(circles(current), 2, "the drawing colour sits in its ring");
        assert_eq!(circles(tint), 1, "the hovered colour gets none");
    }

    #[test]
    fn box_colour_shows_the_panel_and_text_colour_its_letter_in_the_ink() {
        let (panel, ink) = ([250, 192, 66], [237, 79, 92]);
        let ops = |action| {
            let mut canvas = Recorder {
                size: Point { x: 30., y: 30. },
                ops: Vec::new(),
            };
            let button = Button {
                bounds: Rect::new(Point::default(), Point { x: 30., y: 30. }).unwrap(),
                label: Key::ToolbarTextColor,
                action,
                panel: 1,
                active: false,
                enabled: true,
            };
            let colors = ([1; 3], [2; 3]);
            draw(
                &mut canvas,
                &button,
                colors,
                (panel, ink),
                (false, None),
                1.,
            )
            .unwrap();
            canvas.ops
        };
        let filled = |ops: &[crate::scale_tests::Op], rgb| {
            let solid = format!("{:?}", Ink::solid(rgb));
            ops.iter()
                .any(|op| op.name == "fill_round_rect" && op.fixed == solid)
        };
        let inked = |ops: &[crate::scale_tests::Op], rgb| {
            let solid = format!("{:?}", Ink::solid(rgb));
            ops.iter()
                .any(|op| matches!(op.name, "polyline" | "line") && op.fixed.starts_with(&solid))
        };
        let box_color = ops(Action::BoxColor);
        assert!(filled(&box_color, panel), "the box colour");
        assert!(!inked(&box_color, ink));
        let text_color = ops(Action::TextColor);
        assert!(filled(&text_color, panel), "on the box colour");
        assert!(inked(&text_color, ink), "a letter in the text's ink");
    }

    #[test]
    fn a_drop_the_blue_of_a_chosen_button_keeps_a_rim_there() {
        let blue = COLORS[3];
        let bucket = Button {
            bounds: Rect::new(Point::default(), Point { x: 30., y: 30. }).unwrap(),
            label: Key::ToolbarFill,
            action: Action::Fill,
            panel: 1,
            active: true,
            enabled: true,
        };
        let rim = |selected| {
            let mut canvas = Recorder {
                size: Point { x: 30., y: 30. },
                ops: Vec::new(),
            };
            draw(
                &mut canvas,
                &bucket,
                (blue, blue),
                (blue, blue),
                (selected, None),
                1.,
            )
            .unwrap();
            canvas.ops.iter().any(|op| op.name == "stroke_ellipse")
        };
        assert!(rim(true), "on the chosen fill");
        assert!(!rim(false), "on the panel");
    }

    #[test]
    fn a_drop_too_dark_for_the_panel_keeps_a_rim() {
        let rim = |rgb| {
            let ops = record(Action::Fill, (rgb, rgb)).ops;
            ops.iter().any(|op| op.name == "stroke_ellipse")
        };
        assert!(rim([28, 31, 40]), "palette black");
        assert!(!rim([237, 79, 92]), "palette red");
        assert!(!rim([245, 245, 248]), "palette white");
    }
}
