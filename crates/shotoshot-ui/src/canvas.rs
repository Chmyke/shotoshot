//! ROLE: The drawing surface portable UI code paints on.
//! OWNER: Each platform implements it over its native renderer for one frame.
//! INVARIANTS: Coordinates are device pixels; lines use flat caps unless stated.
use shotoshot_core::{Object, Point, Rect};

/// Straight RGB colour with its opacity.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ink {
    pub rgb: [u8; 3],
    pub alpha: f32,
}
impl Ink {
    pub const fn solid(rgb: [u8; 3]) -> Self {
        Self { rgb, alpha: 1. }
    }
    pub const fn with_alpha(rgb: [u8; 3], alpha: f32) -> Self {
        Self { rgb, alpha }
    }
}

/// The product's two typefaces.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Face {
    /// Labels, fields and hints of the interface: the system font.
    Interface,
    /// Text annotations: the embedded `annotation_font`.
    Annotation,
}

/// The typeface of text annotations: embedded, so every platform draws the same glyphs.
pub mod annotation_font {
    pub const FAMILY: &str = "Roboto";
    pub const WEIGHT: u16 = 500;
    /// Line height relative to the font size, as DirectWrite lays this font out.
    pub const LINE_HEIGHT: f32 = 1.2;
    /// Roboto Medium, SIL Open Font License 1.1: `assets/Roboto-OFL.txt`.
    pub static DATA: &[u8] = include_bytes!("../assets/Roboto-Medium.ttf");
}

pub trait Canvas {
    type Error;
    /// Size of the drawable area.
    fn size(&self) -> Point;
    fn fill_rect(&mut self, r: Rect, ink: Ink) -> Result<(), Self::Error>;
    /// `r` filled except inside the closed polygons of `holes`, which may overlap: a coat
    /// of the capture and the shapes it spares.
    fn fill_rect_except(
        &mut self,
        r: Rect,
        holes: &[Vec<Point>],
        ink: Ink,
    ) -> Result<(), Self::Error>;
    fn stroke_rect(&mut self, r: Rect, ink: Ink, width: f64) -> Result<(), Self::Error>;
    fn fill_round_rect(&mut self, r: Rect, radius: f64, ink: Ink) -> Result<(), Self::Error>;
    fn stroke_round_rect(
        &mut self,
        r: Rect,
        radius: f64,
        ink: Ink,
        width: f64,
    ) -> Result<(), Self::Error>;
    fn fill_ellipse(&mut self, center: Point, radius: Point, ink: Ink) -> Result<(), Self::Error>;
    fn stroke_ellipse(
        &mut self,
        center: Point,
        radius: Point,
        ink: Ink,
        width: f64,
    ) -> Result<(), Self::Error>;
    fn line(&mut self, a: Point, b: Point, ink: Ink, width: f64) -> Result<(), Self::Error>;
    /// The polygon through `points`, filled; it closes by itself.
    fn fill_polygon(&mut self, points: &[Point], ink: Ink) -> Result<(), Self::Error>;
    /// One line through `points` with round caps and joins, back to the first when `closed`.
    fn polyline(
        &mut self,
        points: &[Point],
        closed: bool,
        ink: Ink,
        width: f64,
    ) -> Result<(), Self::Error>;
    /// Interface text clipped to `r`, top-left aligned; each newline starts a line one
    /// `measure_text("X", size).y` lower.
    fn text(&mut self, text: &str, r: Rect, size: f64, ink: Ink) -> Result<(), Self::Error>;
    /// Like `text`, in the typeface of text annotations and with its own line height.
    fn annotation_text(
        &mut self,
        text: &str,
        r: Rect,
        size: f64,
        ink: Ink,
    ) -> Result<(), Self::Error>;
    /// Rotates later drawing by `radians` clockwise around a point; None restores it.
    fn set_rotation(&mut self, rotation: Option<(Point, f64)>) -> Result<(), Self::Error>;
    fn measure_text(&mut self, text: &str, size: f64) -> Result<Point, Self::Error>;
    fn measure_annotation_text(&mut self, text: &str, size: f64) -> Result<Point, Self::Error>;
    /// Premultiplied BGRA raster scaled into `dst`. The canvas may keep it under `key`
    /// until `version` changes; `pixels` runs only when it has no current copy.
    fn raster(
        &mut self,
        key: &'static str,
        version: u64,
        size: (u32, u32),
        pixels: &mut dyn FnMut() -> Vec<u8>,
        dst: Rect,
    ) -> Result<(), Self::Error>;
    /// Annotation objects exactly as they appear in the exported image, except `skip`
    /// (an object whose text is being edited in place).
    fn annotations(&mut self, objects: &[Object], skip: Option<u64>) -> Result<(), Self::Error>;
    /// `annotations` with the objects of `fading` (those a sweep is about to erase) drawn
    /// faded, in their place in the pile, where the platform can; plainly otherwise.
    fn annotations_fading(
        &mut self,
        objects: &[Object],
        skip: Option<u64>,
        _fading: &[u64],
    ) -> Result<(), Self::Error> {
        self.annotations(objects, skip)
    }
    /// `objects` laid over the scene at `opacity`: what a click would change, shown before
    /// it. Their attachments must be resolved already; plainly where the platform cannot fade.
    fn preview(&mut self, objects: &[Object], _opacity: f32) -> Result<(), Self::Error> {
        self.annotations(objects, None)
    }
}
