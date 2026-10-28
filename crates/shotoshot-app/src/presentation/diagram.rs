//! ROLE: Physical keyboard and mouse with transient pressed-character labels.
//! A still diagram is a ghost: only pressed keys, and pressed buttons in a pale mouse outline.
//! OWNER: Presentation layer supplies its reusable target and brush.
//! INVARIANTS: Immutable geometry and reusable text format; no per-frame allocations.
use super::devices::Devices;
use crate::render::color;
use windows::Win32::Graphics::{
    Direct2D::{Common::*, *},
    DirectWrite::*,
};
pub const WIDTH: i32 = Plan::FULL.width();
pub const HEIGHT: i32 = 140;
pub const LOCK_SIZE: i32 = 17;
const PITCH: f32 = 18.;
/// Height of a keycap label of one or two characters, in logical pixels.
pub const LABEL_SIZE: f32 = 11.;
/// Height of a longer keycap label (`Caps`, `Ctrl`, `F12`).
pub const SMALL_LABEL_SIZE: f32 = 7.5;

/// The format of the keycap labels, `size` high, shaped for `language`.
pub fn label_format(
    write: &IDWriteFactory,
    size: f32,
    language: shotoshot_text::Language,
) -> windows::core::Result<IDWriteTextFormat> {
    unsafe {
        let format = write.CreateTextFormat(
            windows::core::w!("Segoe UI"),
            None,
            DWRITE_FONT_WEIGHT_SEMI_BOLD,
            DWRITE_FONT_STYLE_NORMAL,
            DWRITE_FONT_STRETCH_NORMAL,
            size,
            &windows::core::HSTRING::from(language.locale_name()),
        )?;
        format.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_CENTER)?;
        format.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
        format.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)?;
        Ok(format)
    }
}
/// Left edge of the mouse body beside the full keyboard, where its outlines are authored.
const MOUSE_X: i32 = 455;

/// What the diagram shows; its width and the place of its lock follow.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Plan {
    pub keyboard: bool,
    pub mouse: bool,
}
impl Plan {
    /// The whole keyboard beside the mouse.
    pub const FULL: Self = Self {
        keyboard: true,
        mouse: true,
    };
    /// Right edge of the keyboard panel, 0 without a keyboard.
    const fn panel(self) -> i32 {
        if self.keyboard { 431 } else { 0 }
    }
    const fn mouse_x(self) -> i32 {
        if self.keyboard { self.panel() + 24 } else { 12 }
    }
    pub const fn width(self) -> i32 {
        if self.mouse {
            self.mouse_x() + 65
        } else {
            self.panel() + 1
        }
    }
    /// Top-left corner of the lock: the highest free key place on the right of the keyboard,
    /// or above a lone mouse.
    pub const fn lock(self) -> (i32, i32) {
        if self.keyboard {
            (407, 8)
        } else {
            (self.mouse_x() + 48 - LOCK_SIZE, 4)
        }
    }
}
// Scan code, horizontal position and width in physical key units.
type Key = (u16, f32, f32);
const NUMBER: &[Key] = &[
    (41, 0., 1.),
    (2, 1., 1.),
    (3, 2., 1.),
    (4, 3., 1.),
    (5, 4., 1.),
    (6, 5., 1.),
    (7, 6., 1.),
    (8, 7., 1.),
    (9, 8., 1.),
    (10, 9., 1.),
    (11, 10., 1.),
    (12, 11., 1.),
    (13, 12., 1.),
    (14, 13., 2.),
];
const TOP: &[Key] = &[
    (15, 0., 1.5),
    (16, 1.5, 1.),
    (17, 2.5, 1.),
    (18, 3.5, 1.),
    (19, 4.5, 1.),
    (20, 5.5, 1.),
    (21, 6.5, 1.),
    (22, 7.5, 1.),
    (23, 8.5, 1.),
    (24, 9.5, 1.),
    (25, 10.5, 1.),
    (26, 11.5, 1.),
    (27, 12.5, 1.),
];
const HOME: &[Key] = &[
    (58, 0., 1.75),
    (30, 1.75, 1.),
    (31, 2.75, 1.),
    (32, 3.75, 1.),
    (33, 4.75, 1.),
    (34, 5.75, 1.),
    (35, 6.75, 1.),
    (36, 7.75, 1.),
    (37, 8.75, 1.),
    (38, 9.75, 1.),
    (39, 10.75, 1.),
    (40, 11.75, 1.),
    (43, 12.75, 1.),
];
const BOTTOM: &[Key] = &[
    (42, 0., 1.25),
    (86, 1.25, 1.),
    (44, 2.25, 1.),
    (45, 3.25, 1.),
    (46, 4.25, 1.),
    (47, 5.25, 1.),
    (48, 6.25, 1.),
    (49, 7.25, 1.),
    (50, 8.25, 1.),
    (51, 9.25, 1.),
    (52, 10.25, 1.),
    (53, 11.25, 1.),
    (54, 12.25, 2.75),
];
/// Visits every rectangular key cap of `plan`: scan code, extended flag, position in logical
/// pixels, then width and height in key units. The ISO Enter outline is drawn apart.
fn caps(plan: Plan, ansi: bool, mut cap: impl FnMut(u16, bool, f32, f32, f32, f32)) {
    if !plan.keyboard {
        return;
    }
    cap(1, false, 8., 9., 1., 1.);
    for index in 0..12 {
        let scan = if index < 10 {
            59 + index
        } else {
            87 + index - 10
        };
        let x = 8. + (2. + index as f32 + (index / 4) as f32 * 0.5) * PITCH;
        cap(scan, false, x, 9., 1., 1.);
    }
    for (row, keys) in [NUMBER, TOP, HOME, BOTTOM].iter().enumerate() {
        for &(scan, mut x, mut w) in *keys {
            if ansi {
                if scan == 86 {
                    continue;
                }
                if scan == 42 {
                    w = 2.25;
                }
                if scan == 43 {
                    x = 13.5;
                    w = 1.5;
                }
            }
            let row = if ansi && scan == 43 { 1 } else { row };
            cap(scan, false, 8. + x * PITCH, 35. + row as f32 * PITCH, w, 1.);
        }
    }
    if ansi {
        cap(28, false, 8. + 12.75 * PITCH, 71., 2.25, 1.);
    }
    for (scan, ext, x, w) in [
        (29, false, 0., 1.25),
        (91, true, 1.25, 1.25),
        (56, false, 2.5, 1.25),
        (57, false, 3.75, 6.25),
        (56, true, 10., 1.25),
        (92, true, 11.25, 1.25),
        (93, true, 12.5, 1.25),
        (29, true, 13.75, 1.25),
    ] {
        cap(scan, ext, 8. + x * PITCH, 107., w, 1.);
    }
    for (scan, ext, x, y) in [
        (55, true, 0., 9.),
        (70, false, 1., 9.),
        (69, true, 2., 9.),
        (82, true, 0., 35.),
        (71, true, 1., 35.),
        (73, true, 2., 35.),
        (83, true, 0., 53.),
        (79, true, 1., 53.),
        (81, true, 2., 53.),
        (72, true, 1., 89.),
        (75, true, 0., 107.),
        (80, true, 1., 107.),
        (77, true, 2., 107.),
    ] {
        cap(scan, ext, 288. + x * PITCH, y, 1., 1.);
    }
    for (scan, ext, x, y, w, h) in [
        (69, false, 0., 35., 1., 1.),
        (53, true, 1., 35., 1., 1.),
        (55, false, 2., 35., 1., 1.),
        (74, false, 3., 35., 1., 1.),
        (71, false, 0., 53., 1., 1.),
        (72, false, 1., 53., 1., 1.),
        (73, false, 2., 53., 1., 1.),
        (78, false, 3., 53., 1., 2.),
        (75, false, 0., 71., 1., 1.),
        (76, false, 1., 71., 1., 1.),
        (77, false, 2., 71., 1., 1.),
        (79, false, 0., 89., 1., 1.),
        (80, false, 1., 89., 1., 1.),
        (81, false, 2., 89., 1., 1.),
        (28, true, 3., 89., 1., 2.),
        (82, false, 0., 107., 2., 1.),
        (83, false, 2., 107., 1., 1.),
    ] {
        cap(scan, ext, 354. + x * PITCH, y, w, h);
    }
}
/// The ISO Enter key is an outline of its own, not a cap.
fn iso_enter(plan: Plan, ansi: bool) -> bool {
    plan.keyboard && !ansi
}
/// Whether `plan` draws this key: an unseen key must cost no repaint.
pub(super) fn shows(plan: Plan, ansi: bool, scan: u16, extended: bool) -> bool {
    let mut shown = iso_enter(plan, ansi) && (scan, extended) == (28, false);
    caps(plan, ansi, |key, ext, _, _, _, _| {
        shown |= (key, ext) == (scan, extended)
    });
    shown
}
struct Painter<'a> {
    target: &'a ID2D1RenderTarget,
    brush: &'a ID2D1SolidColorBrush,
    format: &'a IDWriteTextFormat,
    small_format: &'a IDWriteTextFormat,
    outlines: &'a Outlines,
    scale: f32,
    devices: &'a Devices,
    now: u64,
    /// The whole diagram, as while it is moved; otherwise its ghost.
    whole: bool,
}
impl Painter<'_> {
    /// How much of a key shows: always whole, else while held and through its fade.
    fn presence(&self, scan: u16, extended: bool) -> f32 {
        if self.whole {
            1.
        } else {
            self.devices.presence(scan, extended, self.now)
        }
    }
    fn box_at(&self, x: f32, y: f32, w: f32, h: f32, radius: f32, active: bool) {
        self.faded_box(x, y, w, h, radius, active, 1.);
    }
    #[allow(clippy::too_many_arguments)]
    fn faded_box(&self, x: f32, y: f32, w: f32, h: f32, radius: f32, active: bool, fade: f32) {
        let s = self.scale;
        let shape = D2D1_ROUNDED_RECT {
            rect: D2D_RECT_F {
                left: x * s,
                top: y * s,
                right: (x + w) * s,
                bottom: (y + h) * s,
            },
            radiusX: radius * s,
            radiusY: radius * s,
        };
        unsafe {
            self.brush.SetColor(&color(
                if active {
                    [113, 195, 255]
                } else {
                    [32, 32, 35]
                },
                fade * if active {
                    0.96
                } else if radius <= 2. {
                    0.72
                } else if radius >= 20. {
                    0.14
                } else {
                    0.22
                },
            ));
            self.target.FillRoundedRectangle(&shape, self.brush);
            self.brush.SetColor(&color(
                if active {
                    [184, 226, 255]
                } else {
                    [110, 110, 118]
                },
                fade * if active {
                    0.95
                } else if radius <= 2. {
                    0.72
                } else if radius >= 20. {
                    0.28
                } else {
                    0.38
                },
            ));
            self.target
                .DrawRoundedRectangle(&shape, self.brush, s, None);
        }
    }
    fn key(&self, scan: u16, extended: bool, x: f32, y: f32, w: f32, h: f32) {
        let presence = self.presence(scan, extended);
        if presence <= 0. {
            return;
        }
        self.faded_box(
            x,
            y,
            w * PITCH - 3.,
            h * PITCH - 3.,
            2.,
            self.devices.active(scan, extended, self.now),
            presence,
        );
        self.label(scan, extended, x, y, w, h);
    }
    /// The pale outline around a ghost while something shows inside it.
    fn outline(&self, x: f32, y: f32, w: f32, h: f32, radius: f32, presence: f32) {
        if presence <= 0. {
            return;
        }
        let s = self.scale;
        let shape = D2D1_ROUNDED_RECT {
            rect: D2D_RECT_F {
                left: x * s,
                top: y * s,
                right: (x + w) * s,
                bottom: (y + h) * s,
            },
            radiusX: radius * s,
            radiusY: radius * s,
        };
        unsafe {
            self.brush
                .SetColor(&color([110, 110, 118], GHOST_OUTLINE * presence));
            self.target
                .DrawRoundedRectangle(&shape, self.brush, s, None);
        }
    }
    fn label(&self, scan: u16, extended: bool, x: f32, y: f32, w: f32, h: f32) {
        let (text, alpha) = self.devices.label(scan, extended, self.now);
        if text.is_empty() || alpha <= 0. {
            return;
        }
        let s = self.scale;
        let area = D2D_RECT_F {
            left: x * s,
            top: y * s,
            right: (x + w * PITCH - 3.) * s,
            bottom: (y + h * PITCH - 3.) * s,
        };
        unsafe {
            let rgb = if self.devices.active(scan, extended, self.now) {
                [20, 37, 50]
            } else {
                [235, 241, 250]
            };
            self.brush.SetColor(&color(rgb, alpha));
            self.target.DrawText(
                text,
                if text.len() >= 3 {
                    self.small_format
                } else {
                    self.format
                },
                &area,
                self.brush,
                D2D1_DRAW_TEXT_OPTIONS_CLIP,
                DWRITE_MEASURING_MODE_NATURAL,
            );
        }
    }
}
/// Opacity of the ghost mouse's outline while one of its buttons is pressed.
const GHOST_OUTLINE: f32 = 0.22;
/// Mouse parts: left, right, middle, back, forward buttons and the wheel.
const MOUSE_PARTS: usize = 6;

/// Paints the diagram: `whole` while it is moved, else its ghost (nothing when idle).
pub(super) fn draw(
    target: &ID2D1RenderTarget,
    brush: &ID2D1SolidColorBrush,
    formats: (&IDWriteTextFormat, &IDWriteTextFormat),
    outlines: &Outlines,
    devices: &Devices,
    now: u64,
    whole: bool,
) {
    let (plan, scale) = (outlines.plan, outlines.scale);
    let p = Painter {
        target,
        brush,
        format: formats.0,
        small_format: formats.1,
        outlines,
        scale,
        devices,
        now,
        whole,
    };
    let iso = iso_enter(plan, devices.ansi);
    if plan.keyboard {
        // A ghost keyboard has no frame: its pressed keys alone (Mickael, 2026-10-28).
        if whole {
            p.box_at(1., 1., plan.panel() as f32 - 1., 132., 7., false);
        }
        caps(plan, devices.ansi, |scan, extended, x, y, w, h| {
            p.key(scan, extended, x, y, w, h)
        });
    }
    // A single rounded ISO outline avoids the seam made by overlapping keycaps.
    let presence = p.presence(28, false);
    if iso && presence > 0. {
        let active = devices.active(28, false, now);
        unsafe {
            brush.SetColor(&color(
                if active {
                    [113, 195, 255]
                } else {
                    [32, 32, 35]
                },
                presence * if active { 0.96 } else { 0.72 },
            ));
            target.FillGeometry(&p.outlines.enter, brush, None);
            brush.SetColor(&color(
                if active {
                    [184, 226, 255]
                } else {
                    [110, 110, 118]
                },
                presence * if active { 0.95 } else { 0.72 },
            ));
            target.DrawGeometry(&p.outlines.enter, brush, scale, None);
        }
        p.label(28, false, 255.5, 62., 1.25, 1.);
    }
    if !plan.mouse {
        return;
    }
    let x = plan.mouse_x() as f32;
    let pressed = |index| devices.mouse_active(index, now);
    if whole {
        p.box_at(x, 24., 48., 102., 24., false);
    } else {
        let presence = if (0..MOUSE_PARTS).any(pressed) {
            1.
        } else {
            0.
        };
        p.outline(x, 24., 48., 102., 24., presence);
    }
    for (index, shape) in p.outlines.mouse.iter().enumerate() {
        let active = pressed(index);
        if !whole && !active {
            continue;
        }
        unsafe {
            brush.SetColor(&color(
                if active {
                    [113, 195, 255]
                } else {
                    [32, 32, 35]
                },
                if active { 0.96 } else { 0.18 },
            ));
            target.FillGeometry(shape, brush, None);
            brush.SetColor(&color(
                if active {
                    [184, 226, 255]
                } else {
                    [110, 110, 118]
                },
                if active { 0.95 } else { 0.38 },
            ));
            target.DrawGeometry(shape, brush, scale, None);
        }
    }
    for (index, [x, y, w, h]) in [
        (2, [x + 22., 38., 4., 19.]),
        (5, [x + 23., 42., 2., 11.]),
        (4, [x - 5., 57., 3., 15.]),
        (3, [x - 5., 77., 3., 15.]),
    ] {
        if whole || pressed(index) {
            p.box_at(x, y, w, h, 2.5, pressed(index));
        }
    }
}

// Cached once per layer/DPI, never allocated while drawing.
fn enter_geometry(factory: &ID2D1Factory, scale: f32) -> windows::core::Result<ID2D1PathGeometry> {
    let points = [
        (251., 53.),
        (275., 53.),
        (275., 86.),
        (255.5, 86.),
        (255.5, 68.),
        (251., 68.),
    ];
    let point = |p: (f32, f32)| windows_numerics::Vector2 {
        X: p.0 * scale,
        Y: p.1 * scale,
    };
    let toward = |a: (f32, f32), b: (f32, f32)| {
        let length = (b.0 - a.0).hypot(b.1 - a.1);
        (
            a.0 + (b.0 - a.0) * 1.5 / length,
            a.1 + (b.1 - a.1) * 1.5 / length,
        )
    };
    unsafe {
        let geometry = factory.CreatePathGeometry()?;
        let sink = geometry.Open()?;
        sink.BeginFigure(
            point(toward(points[0], points[5])),
            D2D1_FIGURE_BEGIN_FILLED,
        );
        for index in 0..points.len() {
            let vertex = points[index];
            sink.AddLine(point(toward(vertex, points[(index + 5) % 6])));
            sink.AddQuadraticBezier(&D2D1_QUADRATIC_BEZIER_SEGMENT {
                point1: point(vertex),
                point2: point(toward(vertex, points[(index + 1) % 6])),
            });
        }
        sink.EndFigure(D2D1_FIGURE_END_CLOSED);
        sink.Close()?;
        Ok(geometry)
    }
}

/// Curved outlines of one plan at one scale; `draw` paints that plan.
pub(super) struct Outlines {
    pub plan: Plan,
    scale: f32,
    enter: ID2D1PathGeometry,
    mouse: [ID2D1PathGeometry; 2],
}
impl Outlines {
    pub(super) fn new(
        factory: &ID2D1Factory,
        scale: f32,
        plan: Plan,
    ) -> windows::core::Result<Self> {
        let shift = (plan.mouse_x() - MOUSE_X) as f32;
        Ok(Self {
            plan,
            scale,
            enter: enter_geometry(factory, scale)?,
            mouse: [
                mouse_button(factory, scale, shift, false)?,
                mouse_button(factory, scale, shift, true)?,
            ],
        })
    }
}
fn mouse_button(
    factory: &ID2D1Factory,
    scale: f32,
    shift: f32,
    right: bool,
) -> windows::core::Result<ID2D1PathGeometry> {
    let point = |x: f32, y: f32| windows_numerics::Vector2 {
        X: ((if right { 958. - x } else { x }) + shift) * scale,
        Y: y * scale,
    };
    unsafe {
        let geometry = factory.CreatePathGeometry()?;
        let sink = geometry.Open()?;
        sink.BeginFigure(point(478., 25.), D2D1_FIGURE_BEGIN_FILLED);
        sink.AddBezier(&D2D1_BEZIER_SEGMENT {
            point1: point(465., 25.),
            point2: point(456., 35.),
            point3: point(456., 48.),
        });
        sink.AddLine(point(456., 63.));
        sink.AddQuadraticBezier(&D2D1_QUADRATIC_BEZIER_SEGMENT {
            point1: point(456., 66.),
            point2: point(459., 66.),
        });
        sink.AddLine(point(473., 66.));
        sink.AddQuadraticBezier(&D2D1_QUADRATIC_BEZIER_SEGMENT {
            point1: point(476., 66.),
            point2: point(476., 63.),
        });
        sink.AddLine(point(476., 38.));
        sink.AddBezier(&D2D1_BEZIER_SEGMENT {
            point1: point(476., 33.),
            point2: point(478., 33.),
            point3: point(478., 29.),
        });
        sink.EndFigure(D2D1_FIGURE_END_CLOSED);
        sink.Close()?;
        Ok(geometry)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan(keyboard: bool, mouse: bool) -> Plan {
        Plan { keyboard, mouse }
    }
    /// Every box of the keyboard in logical pixels, the ISO Enter as its two halves.
    fn boxes(plan: Plan, ansi: bool) -> Vec<(u16, bool, [f32; 4])> {
        let mut boxes = Vec::new();
        caps(plan, ansi, |scan, extended, x, y, w, h| {
            boxes.push((
                scan,
                extended,
                [x, y, x + w * PITCH - 3., y + h * PITCH - 3.],
            ))
        });
        if iso_enter(plan, ansi) {
            boxes.push((28, false, [251., 53., 275., 68.]));
            boxes.push((28, false, [255.5, 68., 275., 86.]));
        }
        boxes
    }
    #[test]
    fn keycap_labels_fit_their_keys() {
        use super::super::devices::special_label_in;
        use shotoshot_text::Language;
        let write: IDWriteFactory =
            unsafe { DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED) }.unwrap();
        // The width `label` is drawn at, with the format `label` picks.
        let width = |label: &str, language| {
            let units: Vec<u16> = label.encode_utf16().collect();
            let size = if units.len() >= 3 {
                SMALL_LABEL_SIZE
            } else {
                LABEL_SIZE
            };
            let format = label_format(&write, size, language).unwrap();
            let mut metrics = DWRITE_TEXT_METRICS::default();
            unsafe {
                write
                    .CreateTextLayout(&units, &format, 1000., 100.)
                    .unwrap()
                    .GetMetrics(&mut metrics)
                    .unwrap();
            }
            metrics.width
        };
        let mut overflows = Vec::new();
        for language in Language::ALL {
            let tag = language.tag();
            for (scan, extended, [left, _, right, _]) in boxes(Plan::FULL, false)
                .into_iter()
                .chain(boxes(Plan::FULL, true))
            {
                let label = special_label_in(scan, extended, language);
                assert!(
                    label.encode_utf16().count() <= 4,
                    "{tag}: `{label}` has more than 4 characters"
                );
                if label.is_empty() {
                    continue;
                }
                // English is the measure: a label may reach the edges of its key, or take as
                // much room as the English one already does.
                let english = special_label_in(scan, extended, Language::En);
                let room = (right - left).max(width(english, Language::En));
                let drawn = width(label, language);
                if drawn > room {
                    overflows.push(format!(
                        "{tag}: `{label}` is {drawn} px where `{english}` leaves {room} px"
                    ));
                }
            }
        }
        assert!(overflows.is_empty(), "{overflows:#?}");
    }
    fn apart(a: [f32; 4], b: [f32; 4]) -> bool {
        a[2] <= b[0] || b[2] <= a[0] || a[3] <= b[1] || b[3] <= a[1]
    }

    #[test]
    fn the_full_diagram_keeps_its_size_and_lock() {
        assert_eq!((WIDTH, HEIGHT), (520, 140));
        assert_eq!(Plan::FULL.lock(), (407, 8));
        assert_eq!(Plan::FULL.mouse_x(), MOUSE_X);
    }

    #[test]
    fn every_plan_fits_its_window_without_overlapping_keys_or_lock() {
        for (keyboard, mouse) in [(true, true), (true, false), (false, true)] {
            let plan = plan(keyboard, mouse);
            let (lock_x, lock_y) = plan.lock();
            let lock = [
                lock_x as f32,
                lock_y as f32,
                (lock_x + LOCK_SIZE) as f32,
                (lock_y + LOCK_SIZE) as f32,
            ];
            assert!(
                lock[0] >= 0. && lock[2] <= plan.width() as f32 && lock[3] <= HEIGHT as f32,
                "{plan:?}: lock outside the window"
            );
            if mouse {
                let body = plan.mouse_x() as f32;
                assert!(
                    body - 5. >= plan.panel() as f32,
                    "{plan:?}: mouse over the keys"
                );
                assert!(body + 48. < plan.width() as f32, "{plan:?}: mouse cut off");
                assert!(apart(lock, [body, 24., body + 48., 126.]), "{plan:?}");
            }
            for ansi in [false, true] {
                let boxes = boxes(plan, ansi);
                assert_eq!(boxes.is_empty(), !keyboard);
                for (index, (scan, _, cap)) in boxes.iter().enumerate() {
                    assert!(
                        cap[0] >= 8. && cap[1] >= 9. && cap[3] <= 122.,
                        "{plan:?}: key {scan} leaves the panel"
                    );
                    assert!(
                        cap[2] <= plan.panel() as f32 - 8.,
                        "{plan:?}: key {scan} passes the right edge"
                    );
                    assert!(apart(*cap, lock), "{plan:?}: key {scan} under the lock");
                    for (other, _, earlier) in &boxes[..index] {
                        assert!(
                            apart(*cap, *earlier),
                            "{plan:?}: keys {scan} and {other} overlap"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn shows_matches_what_each_plan_draws() {
        for ansi in [false, true] {
            assert!(shows(Plan::FULL, ansi, 28, false), "Enter, outline or cap");
            assert!(shows(Plan::FULL, ansi, 71, false), "the numeric pad");
            assert!(shows(Plan::FULL, ansi, 29, true), "right Ctrl");
            assert!(!shows(plan(false, true), ansi, 17, false), "a lone mouse");
        }
        assert!(shows(Plan::FULL, false, 86, false));
        assert!(!shows(Plan::FULL, true, 86, false));
    }
}
