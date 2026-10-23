//! ROLE: Portable CPU implementation of `Canvas` (tiny-skia paths, cosmic-text glyphs) for
//! Linux and macOS, plus the flattened export of a capture.
//! OWNER: One `Raster` per capture session keeps fonts, glyph cache and cached rasters.
//! INVARIANTS: Pixmaps are premultiplied RGBA; `Image` stays top-down BGRA at the edges.
use cosmic_text::{
    Attrs, Buffer, Color, Family, FontSystem, Metrics, Shaping, SwashCache, Weight, fontdb,
};
use shotoshot_core::{Object, Point, Rect, Shape, paths, stroke_runs};
use shotoshot_ui::{Canvas, Face, Image, Ink, annotation_font};
use std::sync::Arc;
use tiny_skia::{
    FillRule, FilterQuality, LineCap, LineJoin, Mask, Paint, PathBuilder, Pixmap, PixmapPaint,
    Stroke, Transform,
};

/// Line height relative to font size, close to the Windows UI font metrics.
const LINE_HEIGHT: f32 = 1.33;

/// How this renderer resolves a typeface: the system sans-serif, or the embedded font of
/// text annotations with the same glyphs as on Windows.
fn attrs(face: Face) -> Attrs<'static> {
    match face {
        Face::Interface => Attrs::new().family(Family::SansSerif),
        Face::Annotation => Attrs::new()
            .family(Family::Name(annotation_font::FAMILY))
            .weight(Weight(annotation_font::WEIGHT)),
    }
}
fn line_height(face: Face) -> f32 {
    match face {
        Face::Interface => LINE_HEIGHT,
        Face::Annotation => annotation_font::LINE_HEIGHT,
    }
}

#[derive(Debug)]
pub struct RasterError(pub &'static str);

/// Session-lifetime rendering resources.
pub struct Raster {
    fonts: FontSystem,
    glyphs: SwashCache,
    rasters: Vec<(&'static str, u64, Pixmap)>,
}
impl Default for Raster {
    fn default() -> Self {
        Self::new()
    }
}
impl Raster {
    pub fn new() -> Self {
        let mut fonts = FontSystem::new();
        let annotation = fontdb::Source::Binary(Arc::new(annotation_font::DATA));
        fonts.db_mut().load_font_source(annotation);
        Self {
            fonts,
            glyphs: SwashCache::new(),
            rasters: Vec::new(),
        }
    }
    /// Extent of interface `text` laid out at `size`, exactly as `Canvas::text` draws it.
    pub fn measure_text(&mut self, text: &str, size: f64) -> Point {
        self.extent(text, size, Face::Interface)
    }
    /// Extent of annotation `text` at `size`, exactly as annotations draw it.
    pub fn measure_annotation_text(&mut self, text: &str, size: f64) -> Point {
        self.extent(text, size, Face::Annotation)
    }
    fn extent(&mut self, text: &str, size: f64, face: Face) -> Point {
        let fonts = &mut self.fonts;
        let size = size as f32;
        let mut buffer = Buffer::new(fonts, Metrics::new(size, size * line_height(face)));
        let mut buffer = buffer.borrow_with(fonts);
        buffer.set_size(None, None);
        buffer.set_text(text, &attrs(face), Shaping::Advanced, None);
        let (mut width, mut height) = (0f32, 0f32);
        for run in buffer.layout_runs() {
            width = width.max(run.line_w);
            height = run.line_top + run.line_height;
        }
        Point {
            x: width as f64,
            y: height as f64,
        }
    }
    /// A drawing surface over `target` for one frame.
    pub fn canvas<'a>(&'a mut self, target: &'a mut Pixmap) -> SkiaCanvas<'a> {
        SkiaCanvas {
            raster: self,
            target,
            transform: Transform::identity(),
            base: Transform::identity(),
        }
    }
    /// The capture flattened with its annotations, cropped to `crop`, the `background`'s
    /// coats laid over the captured pixels.
    pub fn export(
        &mut self,
        image: &Image,
        background: &[shotoshot_core::Paint],
        objects: &[Object],
        crop: Rect,
    ) -> Result<Image, RasterError> {
        image.check().map_err(RasterError)?;
        let left = crop.min.x.floor().clamp(0., image.width as f64) as u32;
        let top = crop.min.y.floor().clamp(0., image.height as f64) as u32;
        let right = crop.max.x.ceil().clamp(0., image.width as f64) as u32;
        let bottom = crop.max.y.ceil().clamp(0., image.height as f64) as u32;
        if right <= left || bottom <= top {
            return Err(RasterError("Empty capture region"));
        }
        let mut target = pixmap_from_bgra(image)?;
        let whole = Rect {
            min: Point::default(),
            max: Point {
                x: image.width as f64,
                y: image.height as f64,
            },
        };
        let mut canvas = self.canvas(&mut target);
        for coat in background {
            let ink = Ink::solid(coat.color);
            canvas.fill_rect_except(whole, &holes(coat, objects, 0), ink)?;
        }
        canvas.annotations(objects, None)?;
        let full = image_from_pixmap(&target, image.left, image.top);
        let (width, height) = (right - left, bottom - top);
        let mut output = Image::solid(width, height, [0, 0, 0, 255]);
        for row in 0..height {
            let from = (((top + row) * image.width + left) * 4) as usize;
            let to = (row * width * 4) as usize;
            output.pixels[to..to + (width * 4) as usize]
                .copy_from_slice(&full.pixels[from..from + (width * 4) as usize]);
        }
        Ok(output)
    }
}

/// Outlines of the shapes `coat` spares, `floor` being the first place above its zone.
fn holes(coat: &shotoshot_core::Paint, objects: &[Object], floor: usize) -> Vec<Vec<Point>> {
    coat.holes(objects, floor)
        .flat_map(|o| paths(o, objects))
        .collect()
}

/// Opaque BGRA frame to a premultiplied RGBA pixmap.
pub fn pixmap_from_bgra(image: &Image) -> Result<Pixmap, RasterError> {
    image.check().map_err(RasterError)?;
    let mut pixmap =
        Pixmap::new(image.width, image.height).ok_or(RasterError("Pixmap too large"))?;
    for (to, from) in pixmap
        .data_mut()
        .as_chunks_mut::<4>()
        .0
        .iter_mut()
        .zip(image.pixels.as_chunks::<4>().0)
    {
        *to = [from[2], from[1], from[0], 255];
    }
    Ok(pixmap)
}
/// Pixmap back to an opaque BGRA frame (alpha forced to 255, as exports are opaque).
pub fn image_from_pixmap(pixmap: &Pixmap, left: i32, top: i32) -> Image {
    let mut image = Image::solid(pixmap.width(), pixmap.height(), [0, 0, 0, 255]);
    for (to, from) in image
        .pixels
        .as_chunks_mut::<4>()
        .0
        .iter_mut()
        .zip(pixmap.data().as_chunks::<4>().0)
    {
        *to = [from[2], from[1], from[0], 255];
    }
    image.left = left;
    image.top = top;
    image
}

pub struct SkiaCanvas<'a> {
    raster: &'a mut Raster,
    target: &'a mut Pixmap,
    // Rotation set by `set_rotation` after `base`, applied to every primitive.
    transform: Transform,
    // Where the canvas sits in the frame: identity, or a shift for a preview layer.
    base: Transform,
}
fn paint(ink: Ink) -> Paint<'static> {
    let mut paint = Paint::default();
    paint.set_color_rgba8(
        ink.rgb[0],
        ink.rgb[1],
        ink.rgb[2],
        (ink.alpha.clamp(0., 1.) * 255.).round() as u8,
    );
    paint.anti_alias = true;
    paint
}
fn sk_rect(r: Rect) -> Option<tiny_skia::Rect> {
    tiny_skia::Rect::from_ltrb(
        r.min.x as f32,
        r.min.y as f32,
        r.max.x as f32,
        r.max.y as f32,
    )
}
fn stroke(width: f64, round: bool) -> Stroke {
    Stroke {
        width: width as f32,
        line_cap: if round { LineCap::Round } else { LineCap::Butt },
        line_join: if round {
            LineJoin::Round
        } else {
            LineJoin::Miter
        },
        ..Stroke::default()
    }
}
fn round_rect(r: Rect, radius: f64) -> Option<tiny_skia::Path> {
    let radius = radius.min(r.size().x / 2.).min(r.size().y / 2.).max(0.) as f32;
    let (l, t, rt, b) = (
        r.min.x as f32,
        r.min.y as f32,
        r.max.x as f32,
        r.max.y as f32,
    );
    // Cubic quarter-circle approximation.
    let k = radius * 0.552_284_8;
    let mut pb = PathBuilder::new();
    pb.move_to(l + radius, t);
    pb.line_to(rt - radius, t);
    pb.cubic_to(rt - radius + k, t, rt, t + radius - k, rt, t + radius);
    pb.line_to(rt, b - radius);
    pb.cubic_to(rt, b - radius + k, rt - radius + k, b, rt - radius, b);
    pb.line_to(l + radius, b);
    pb.cubic_to(l + radius - k, b, l, b - radius + k, l, b - radius);
    pb.line_to(l, t + radius);
    pb.cubic_to(l, t + radius - k, l + radius - k, t, l + radius, t);
    pb.close();
    pb.finish()
}
fn polyline(points: &[Point], close: bool) -> Option<tiny_skia::Path> {
    let (first, rest) = points.split_first()?;
    let mut pb = PathBuilder::new();
    pb.move_to(first.x as f32, first.y as f32);
    for p in rest {
        pb.line_to(p.x as f32, p.y as f32);
    }
    if close {
        pb.close();
    }
    pb.finish()
}

impl SkiaCanvas<'_> {
    fn stroke_path(&mut self, path: Option<tiny_skia::Path>, ink: Ink, s: &Stroke) {
        if let Some(path) = path {
            self.target
                .stroke_path(&path, &paint(ink), s, self.transform, None);
        }
    }
    fn fill_path(&mut self, path: Option<tiny_skia::Path>, ink: Ink, transform: Transform) {
        if let Some(path) = path {
            let transform = transform.post_concat(self.transform);
            self.target
                .fill_path(&path, &paint(ink), FillRule::Winding, transform, None);
        }
    }
    /// `area` filled except inside the closed `holes`, which may overlap.
    fn fill_except(&mut self, area: Option<tiny_skia::Path>, holes: &[Vec<Point>], ink: Ink) {
        if holes.is_empty() {
            return self.fill_path(area, ink, Transform::identity());
        }
        let (Some(area), Some(mut mask)) =
            (area, Mask::new(self.target.width(), self.target.height()))
        else {
            return;
        };
        for hole in holes {
            if let Some(path) = polyline(hole, true) {
                mask.fill_path(&path, FillRule::Winding, true, self.transform);
            }
        }
        mask.invert();
        self.target.fill_path(
            &area,
            &paint(ink),
            FillRule::Winding,
            self.transform,
            Some(&mask),
        );
    }
    fn text_at(
        &mut self,
        text: &str,
        r: Rect,
        (face, size): (Face, f64),
        ink: Ink,
        transform: Transform,
        clip: bool,
    ) {
        let raster = &mut *self.raster;
        let size = size as f32;
        let metrics = Metrics::new(size, size * line_height(face));
        let mut buffer = Buffer::new(&mut raster.fonts, metrics);
        let mut buffer = buffer.borrow_with(&mut raster.fonts);
        buffer.set_size(None, None);
        buffer.set_text(text, &attrs(face), Shaping::Advanced, None);
        let alpha = (ink.alpha.clamp(0., 1.) * 255.).round() as u8;
        let color = Color::rgba(ink.rgb[0], ink.rgb[1], ink.rgb[2], alpha);
        let (origin, limit) = (r.min, r.max);
        let transform = transform.post_concat(self.transform);
        let target = &mut *self.target;
        buffer.draw(&mut raster.glyphs, color, |x, y, w, h, c| {
            let (x, y) = (origin.x as f32 + x as f32, origin.y as f32 + y as f32);
            if clip && (x >= limit.x as f32 || y >= limit.y as f32) {
                return;
            }
            let Some(rect) = tiny_skia::Rect::from_xywh(x, y, w as f32, h as f32) else {
                return;
            };
            let mut p = Paint::default();
            p.set_color_rgba8(c.r(), c.g(), c.b(), c.a());
            target.fill_rect(rect, &p, transform, None);
        });
    }
}

impl Canvas for SkiaCanvas<'_> {
    type Error = RasterError;
    fn size(&self) -> Point {
        Point {
            x: self.target.width() as f64,
            y: self.target.height() as f64,
        }
    }
    fn fill_rect(&mut self, r: Rect, ink: Ink) -> Result<(), RasterError> {
        if let Some(rect) = sk_rect(r) {
            self.target
                .fill_rect(rect, &paint(ink), self.transform, None);
        }
        Ok(())
    }
    fn fill_rect_except(
        &mut self,
        r: Rect,
        holes: &[Vec<Point>],
        ink: Ink,
    ) -> Result<(), RasterError> {
        if holes.is_empty() {
            return self.fill_rect(r, ink);
        }
        self.fill_except(sk_rect(r).map(PathBuilder::from_rect), holes, ink);
        Ok(())
    }
    fn stroke_rect(&mut self, r: Rect, ink: Ink, width: f64) -> Result<(), RasterError> {
        let path = sk_rect(r).map(PathBuilder::from_rect);
        self.stroke_path(path, ink, &stroke(width, false));
        Ok(())
    }
    fn fill_round_rect(&mut self, r: Rect, radius: f64, ink: Ink) -> Result<(), RasterError> {
        self.fill_path(round_rect(r, radius), ink, Transform::identity());
        Ok(())
    }
    fn stroke_round_rect(
        &mut self,
        r: Rect,
        radius: f64,
        ink: Ink,
        width: f64,
    ) -> Result<(), RasterError> {
        self.stroke_path(round_rect(r, radius), ink, &stroke(width, false));
        Ok(())
    }
    fn fill_ellipse(&mut self, center: Point, radius: Point, ink: Ink) -> Result<(), RasterError> {
        let path = sk_rect(Rect {
            min: center - radius,
            max: center + radius,
        })
        .and_then(PathBuilder::from_oval);
        self.fill_path(path, ink, Transform::identity());
        Ok(())
    }
    fn stroke_ellipse(
        &mut self,
        center: Point,
        radius: Point,
        ink: Ink,
        width: f64,
    ) -> Result<(), RasterError> {
        let path = sk_rect(Rect {
            min: center - radius,
            max: center + radius,
        })
        .and_then(PathBuilder::from_oval);
        self.stroke_path(path, ink, &stroke(width, false));
        Ok(())
    }
    fn line(&mut self, a: Point, b: Point, ink: Ink, width: f64) -> Result<(), RasterError> {
        self.stroke_path(polyline(&[a, b], false), ink, &stroke(width, false));
        Ok(())
    }
    fn fill_polygon(&mut self, points: &[Point], ink: Ink) -> Result<(), RasterError> {
        self.fill_path(polyline(points, true), ink, Transform::identity());
        Ok(())
    }
    fn polyline(
        &mut self,
        points: &[Point],
        closed: bool,
        ink: Ink,
        width: f64,
    ) -> Result<(), RasterError> {
        self.stroke_path(polyline(points, closed), ink, &stroke(width, true));
        Ok(())
    }
    fn text(&mut self, text: &str, r: Rect, size: f64, ink: Ink) -> Result<(), RasterError> {
        self.text_at(
            text,
            r,
            (Face::Interface, size),
            ink,
            Transform::identity(),
            true,
        );
        Ok(())
    }
    fn annotation_text(
        &mut self,
        text: &str,
        r: Rect,
        size: f64,
        ink: Ink,
    ) -> Result<(), RasterError> {
        self.text_at(
            text,
            r,
            (Face::Annotation, size),
            ink,
            Transform::identity(),
            true,
        );
        Ok(())
    }
    fn set_rotation(&mut self, rotation: Option<(Point, f64)>) -> Result<(), RasterError> {
        self.transform = rotation
            .map_or(Transform::identity(), |(center, radians)| {
                Transform::from_rotate_at(
                    radians.to_degrees() as f32,
                    center.x as f32,
                    center.y as f32,
                )
            })
            .post_concat(self.base);
        Ok(())
    }
    fn measure_text(&mut self, text: &str, size: f64) -> Result<Point, RasterError> {
        Ok(self.raster.measure_text(text, size))
    }
    fn measure_annotation_text(&mut self, text: &str, size: f64) -> Result<Point, RasterError> {
        Ok(self.raster.measure_annotation_text(text, size))
    }
    fn raster(
        &mut self,
        key: &'static str,
        version: u64,
        size: (u32, u32),
        pixels: &mut dyn FnMut() -> Vec<u8>,
        dst: Rect,
    ) -> Result<(), RasterError> {
        let rasters = &mut self.raster.rasters;
        let index = match rasters.iter().position(|(k, _, _)| *k == key) {
            Some(i) if rasters[i].1 == version => i,
            found => {
                let bgra = pixels();
                let mut pixmap = Pixmap::new(size.0, size.1).ok_or(RasterError("Empty raster"))?;
                if bgra.len() != pixmap.data().len() {
                    return Err(RasterError("Raster buffer length"));
                }
                for (to, from) in pixmap
                    .data_mut()
                    .as_chunks_mut::<4>()
                    .0
                    .iter_mut()
                    .zip(bgra.as_chunks::<4>().0)
                {
                    *to = [from[2], from[1], from[0], from[3]];
                }
                match found {
                    Some(i) => {
                        rasters[i] = (key, version, pixmap);
                        i
                    }
                    None => {
                        rasters.push((key, version, pixmap));
                        rasters.len() - 1
                    }
                }
            }
        };
        let source = &self.raster.rasters[index].2;
        let scale = Transform::from_row(
            (dst.size().x / size.0 as f64) as f32,
            0.,
            0.,
            (dst.size().y / size.1 as f64) as f32,
            dst.min.x as f32,
            dst.min.y as f32,
        );
        let paint = PixmapPaint {
            quality: FilterQuality::Bilinear,
            ..PixmapPaint::default()
        };
        let scale = scale.post_concat(self.transform);
        self.target
            .draw_pixmap(0, 0, source.as_ref(), &paint, scale, None);
        Ok(())
    }
    fn annotations(&mut self, objects: &[Object], skip: Option<u64>) -> Result<(), RasterError> {
        for (index, o) in objects.iter().enumerate() {
            if Some(o.id) == skip {
                continue;
            }
            match &o.shape {
                Shape::Text {
                    position,
                    text,
                    size,
                    extent,
                } => {
                    let center = o.local_bounds().center();
                    let transform = Transform::from_rotate_at(
                        o.rotation.to_degrees() as f32,
                        center.x as f32,
                        center.y as f32,
                    );
                    let r = Rect {
                        min: *position,
                        max: *position + *extent + Point { x: 20., y: 20. },
                    };
                    let ink = Ink::solid(o.style.color);
                    self.text_at(text, r, (Face::Annotation, *size), ink, transform, false);
                }
                Shape::Stroke { highlight, .. } => {
                    // Fill the union once, so translucent highlighter runs do not
                    // become darker where two thicknesses meet.
                    let mut union = PathBuilder::new();
                    for (points, width) in stroke_runs(o) {
                        if let Some(outline) = polyline(&points, false)
                            .and_then(|p| p.stroke(&stroke(width, true), 1.))
                        {
                            union.push_path(&outline);
                        }
                    }
                    let ink = Ink::with_alpha(o.style.color, if *highlight { 0.3 } else { 1. });
                    self.fill_path(union.finish(), ink, Transform::identity());
                }
                _ => {
                    let arrow = matches!(o.shape, Shape::Arrow { .. });
                    let ink = Ink::solid(o.style.color);
                    let headed = matches!(o.shape, Shape::Arrow { head: true, .. });
                    let s = stroke(o.style.width, arrow && !headed);
                    for path in paths(o, objects) {
                        if path.is_empty() {
                            continue;
                        }
                        let closed = path.len() > 2 && path.first() == path.last();
                        for coat in o.fill.iter().filter(|_| !arrow) {
                            let interior = polyline(&path, true);
                            let holes = holes(coat, objects, index + 1);
                            self.fill_except(interior, &holes, Ink::solid(coat.color));
                        }
                        if arrow && closed {
                            self.fill_path(polyline(&path, true), ink, Transform::identity());
                        } else {
                            if headed {
                                self.fill_ellipse(
                                    path[0],
                                    Point {
                                        x: o.style.width * 0.5,
                                        y: o.style.width * 0.5,
                                    },
                                    ink,
                                )?;
                            }
                            self.stroke_path(polyline(&path, closed), ink, &s);
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shotoshot_ui::{Capture, TextField};

    fn blank() -> Pixmap {
        pixmap_from_bgra(&Image::solid(400, 200, [255, 255, 255, 255])).unwrap()
    }
    fn inked(p: &Pixmap, r: std::ops::Range<u32>, rows: std::ops::Range<u32>) -> usize {
        rows.flat_map(|y| r.clone().map(move |x| (x, y)))
            .filter(|&(x, y)| {
                p.pixel(x, y)
                    .is_some_and(|c| c.red() < 200 || c.green() < 200)
            })
            .count()
    }

    const SLATE: [u8; 3] = [28, 31, 40];
    const GREEN: [u8; 3] = [64, 199, 128];
    const BLUE: [u8; 3] = [80, 153, 244];
    const PAPER: [u8; 3] = [255, 255, 255];
    fn at(x: f64, y: f64) -> Point {
        Point { x, y }
    }
    fn rgb(image: &Image, x: usize, y: usize) -> [u8; 3] {
        let i = (y * image.width as usize + x) * 4;
        [image.pixels[i + 2], image.pixels[i + 1], image.pixels[i]]
    }
    /// A filled rectangle, a filled circle and a hollow rectangle, outlined in red.
    fn filled_scene() -> shotoshot_core::Editor {
        use shotoshot_core::{Modifiers, Tool};
        let mut e = shotoshot_core::Editor::default();
        for (tool, from, to) in [
            (Tool::Rectangle, at(40., 40.), at(140., 120.)),
            (Tool::Ellipse, at(260., 80.), at(310., 80.)),
            (Tool::Rectangle, at(330., 40.), at(390., 120.)),
        ] {
            e.tool = tool;
            e.begin(from, Modifiers::default());
            e.update(to);
            e.finish();
        }
        e.style.color = SLATE;
        e.fill_at(at(90., 80.), false);
        e.style.color = GREEN;
        e.fill_at(at(260., 80.), false);
        e
    }

    #[test]
    fn export_fills_closed_shapes_under_their_outline() {
        let e = filled_scene();
        let image = Image::solid(400, 200, [255; 4]);
        let crop = Rect::new(at(0., 0.), at(400., 200.)).unwrap();
        let out = Raster::new()
            .export(&image, e.background(), e.objects(), crop)
            .unwrap();
        assert_eq!(rgb(&out, 90, 80), SLATE);
        assert_eq!(rgb(&out, 260, 80), GREEN);
        assert_eq!(
            rgb(&out, 214, 34),
            PAPER,
            "a circle is not filled to its box"
        );
        assert_eq!(rgb(&out, 360, 80), PAPER, "a hollow shape stays hollow");
        assert_eq!(
            rgb(&out, 40, 80),
            [237, 79, 92],
            "the outline lies over the fill"
        );
    }
    #[test]
    fn export_puts_the_background_under_everything_inside_the_crop() {
        let mut e = filled_scene();
        e.style.color = BLUE;
        e.fill_at(at(5., 5.), false);
        assert_eq!(e.background().last().map(|c| c.color), Some(BLUE));
        let image = Image::solid(400, 200, [255; 4]);
        let crop = Rect::new(at(200., 0.), at(400., 200.)).unwrap();
        let out = Raster::new()
            .export(&image, e.background(), e.objects(), crop)
            .unwrap();
        assert_eq!((out.width, out.height), (200, 200));
        for (x, y) in [(0, 0), (199, 199), (14, 34)] {
            assert_eq!(rgb(&out, x, y), BLUE, "{x},{y}");
        }
        assert_eq!(rgb(&out, 60, 80), GREEN);
        assert_eq!(
            rgb(&out, 160, 80),
            PAPER,
            "the hollow rectangle drawn before spares its inside"
        );
    }
    #[test]
    fn a_fill_spares_the_shape_standing_in_it_wherever_it_goes() {
        use shotoshot_core::{Modifiers, Tool};
        let mut e = shotoshot_core::Editor::default();
        e.tool = Tool::Rectangle;
        for (from, to) in [
            (at(40., 40.), at(240., 160.)),
            (at(100., 80.), at(160., 120.)),
        ] {
            e.begin(from, Modifiers::default());
            e.update(to);
            e.finish();
        }
        let inner = e.objects()[1].id;
        e.style.color = SLATE;
        e.fill_at(at(60., 60.), false);
        let image = Image::solid(400, 200, [255; 4]);
        let crop = Rect::new(at(0., 0.), at(400., 200.)).unwrap();
        let mut raster = Raster::new();
        let out = raster
            .export(&image, e.background(), e.objects(), crop)
            .unwrap();
        assert_eq!(rgb(&out, 60, 60), SLATE);
        assert_eq!(rgb(&out, 130, 100), PAPER);
        e.select_only(inner);
        e.nudge(at(60., 0.));
        let out = raster
            .export(&image, e.background(), e.objects(), crop)
            .unwrap();
        assert_eq!(rgb(&out, 130, 100), SLATE, "where it was");
        assert_eq!(rgb(&out, 190, 100), PAPER, "where it is");
    }
    #[test]
    fn the_background_covers_the_region_on_screen_and_only_dims_around_it() {
        let mut capture = Capture::new(Image::solid(400, 200, [255; 4]));
        capture.selecting = false;
        capture.region = Rect::new(at(100., 50.), at(300., 150.));
        let mut raster = Raster::new();
        let mut frame = |capture: &Capture| {
            let mut frame = blank();
            capture
                .draw_underlay(&mut raster.canvas(&mut frame))
                .unwrap();
            image_from_pixmap(&frame, 0, 0)
        };
        let plain = frame(&capture);
        let dimmed = rgb(&plain, 20, 20);
        assert_eq!(rgb(&plain, 200, 100), PAPER);
        assert!(dimmed[0] > 100 && dimmed[0] < 160, "{dimmed:?}");

        capture.editor.style.color = SLATE;
        capture.editor.fill_at(at(200., 100.), false);
        let slate = frame(&capture);
        for (x, y) in [(200, 100), (100, 50), (299, 149)] {
            assert_eq!(rgb(&slate, x, y), SLATE, "{x},{y}");
        }
        for (x, y) in [(99, 100), (300, 100), (200, 49), (200, 150), (20, 20)] {
            assert_eq!(rgb(&slate, x, y), dimmed, "{x},{y}");
        }

        // The slate belongs to the capture: it follows the region.
        capture.region = Rect::new(at(150., 50.), at(350., 150.));
        let moved = frame(&capture);
        assert_eq!(rgb(&moved, 120, 100), dimmed);
        assert_eq!(rgb(&moved, 340, 100), SLATE);
    }
    #[test]
    fn text_field_draws_text_selection_and_caret() {
        let capture = Capture::new(Image::solid(400, 200, [255; 4]));
        let mut field = TextField::open(&capture, Point { x: 40., y: 40. });
        field.insert("Shotoshot");
        let mut raster = Raster::new();
        let mut frame = blank();
        field
            .draw(&mut raster.canvas(&mut frame), true, 1.)
            .unwrap();
        let width = raster
            .canvas(&mut blank())
            .measure_annotation_text("Shotoshot", 24.)
            .unwrap()
            .x;
        assert!(
            inked(&frame, 40..40 + width as u32, 40..70) > 60,
            "text missing"
        );
        // The caret is a vertical line just after the text.
        let caret = 40 + width.round() as u32;
        assert!(
            inked(&frame, caret - 1..caret + 2, 45..65) >= 10,
            "caret missing"
        );
        let mut selected = blank();
        field.key(
            36,
            shotoshot_core::Modifiers {
                shift: true,
                ..Default::default()
            },
            &mut |s| Point {
                x: s.len() as f64,
                y: 1.,
            },
        );
        field
            .draw(&mut raster.canvas(&mut selected), true, 1.)
            .unwrap();
        let blue = |p: &Pixmap| {
            (40..40 + width as u32)
                .flat_map(|x| (42..68).map(move |y| (x, y)))
                .filter(|&(x, y)| {
                    p.pixel(x, y)
                        .is_some_and(|c| c.blue() > 200 && c.red() < 230 && c.red() > 120)
                })
                .count()
        };
        assert!(blue(&selected) > blue(&frame) + 100, "selection missing");
    }
    #[test]
    fn dimension_fields_draw_inside_their_boxes() {
        // A frame too narrow for the expression, which then scrolls inside its field.
        let mut capture = Capture::new(Image::solid(200, 300, [0, 0, 0, 255]));
        capture.selecting = false;
        capture.region = Rect::new(Point { x: 20., y: 70. }, Point { x: 190., y: 250. });
        let mut raster = Raster::new();
        let mut fields = shotoshot_ui::DimensionFields::open(&capture).unwrap();
        let size = fields.font_size();
        fields.insert("1000+200+30+4-1-1-1", &mut |s| raster.measure_text(s, size));
        // What the editor does before each frame.
        capture
            .measure_badge(Some(fields.texts()), |text, size| {
                Ok::<_, ()>(raster.measure_text(text, size).x)
            })
            .unwrap();
        fields.place(&capture, &mut |s| raster.measure_text(s, size));
        let mut frame = pixmap_from_bgra(&capture.image).unwrap();
        fields.draw(&mut raster.canvas(&mut frame), true).unwrap();
        let [width, height] = capture.dimension_field_bounds().unwrap();
        let bright = |x: std::ops::Range<u32>| {
            let rows = width.min.y as u32..width.max.y as u32;
            rows.flat_map(|y| x.clone().map(move |x| (x, y)))
                .filter(|&(x, y)| frame.pixel(x, y).is_some_and(|c| c.red() > 120))
                .count()
        };
        let span = |r: Rect| r.min.x as u32..r.max.x as u32;
        assert!(bright(span(width)) > 40, "width text missing");
        assert!(bright(span(height)) > 20, "height text missing");
        let between = width.max.x as u32 + 1..height.min.x as u32;
        assert_eq!(bright(between), 0, "long text must stay inside its field");
    }
    #[test]
    fn notices_fit_their_text_near_the_top_of_the_screen() {
        let mut raster = Raster::new();
        let mut frame = blank();
        let mut canvas = raster.canvas(&mut frame);
        let message = "Could not save the capture";
        let size = shotoshot_ui::notice_size(&mut canvas, message, 1.).unwrap();
        let text = canvas.measure_text(message, 13.).unwrap();
        assert!(size.x > text.x + 20. && size.y > text.y + 12.);
        let screen = Rect {
            min: Point::default(),
            max: Point { x: 400., y: 200. },
        };
        let bounds = shotoshot_ui::top_center(screen, size, 1.);
        assert!((bounds.center().x - 200.).abs() < 1. && bounds.min.y == 24.);
        shotoshot_ui::draw_notice(&mut canvas, bounds, message, 1.).unwrap();
        let (x, y) = (bounds.min.x as u32, bounds.min.y as u32);
        assert!(inked(&frame, x..x + size.x as u32, y..y + size.y as u32) > 500);
        assert_eq!(inked(&frame, 0..400, 0..20), 0, "nothing above the box");
    }
    #[test]
    fn rotated_text_field_draws_vertically() {
        let capture = Capture::new(Image::solid(400, 200, [255; 4]));
        let mut field = TextField::open(&capture, Point { x: 150., y: 90. });
        field.insert("Vertical");
        field.rotation = std::f64::consts::FRAC_PI_2;
        let mut raster = Raster::new();
        let mut frame = blank();
        field
            .draw(&mut raster.canvas(&mut frame), false, 1.)
            .unwrap();
        let tall = inked(&frame, 0..400, 0..200);
        let horizontal_band = inked(&frame, 0..400, 90..120);
        assert!(tall > 80);
        assert!(horizontal_band * 2 < tall, "text was not rotated");
    }

    /// The Windows bound of every hint (`every_tooltip_fits`), measured by this renderer and
    /// the fonts of the machine: on the Raspberry Pi, those a Linux user has.
    #[test]
    fn every_tooltip_fits_with_cosmic_text() {
        use shotoshot_ui::toolbar::{MAX_HINT_WIDTH, all_tooltips_in};
        let mut raster = Raster::new();
        for language in shotoshot_text::Language::ALL {
            for hint in all_tooltips_in(language) {
                let width = raster.measure_text(&hint, 12.).x;
                assert!(
                    width <= MAX_HINT_WIDTH,
                    "{}: `{hint}` is {width:.0} px wide, over {MAX_HINT_WIDTH} px",
                    language.tag()
                );
            }
        }
    }

    /// The Windows bound of the loupe's hint (`magnifier_hint_fits_the_loupe`), measured by
    /// this renderer: the loupe keeps its size in every language.
    #[test]
    fn magnifier_hint_fits_the_loupe_with_cosmic_text() {
        use shotoshot_ui::{MAGNIFIER_HINT_ROOM as ROOM, MAGNIFIER_HINT_SIZE, magnifier_hint_in};
        let mut raster = Raster::new();
        let too_long: Vec<String> = shotoshot_text::Language::ALL
            .into_iter()
            .filter_map(|language| {
                let hint = magnifier_hint_in(language);
                let measured = raster.measure_text(&hint, MAGNIFIER_HINT_SIZE);
                assert!(measured.x > 0., "{}: nothing measured", language.tag());
                (measured.x > ROOM.x || measured.y > ROOM.y).then(|| {
                    let tag = language.tag();
                    format!("{tag}: `{hint}` is {:.0}×{:.0} px", measured.x, measured.y)
                })
            })
            .collect();
        assert!(
            too_long.is_empty(),
            "the loupe holds {}×{} px, shorten:\n{}",
            ROOM.x,
            ROOM.y,
            too_long.join("\n")
        );
    }

    /// The Windows bound of the colour wheel's labels (`color_picker_labels_grow_from_minimum`),
    /// measured by this renderer: each one fits the panel's width and its line.
    #[test]
    fn color_picker_labels_fit_with_cosmic_text() {
        use shotoshot_ui::color_picker::{
            HINT_HEIGHT, HINT_SIZE, MAX_LABEL_WIDTH, TITLE_HEIGHT, TITLE_SIZE, labels_in,
        };
        let mut raster = Raster::new();
        let mut overflows = Vec::new();
        for language in shotoshot_text::Language::ALL {
            let labels = labels_in(language);
            for (label, size, height) in [
                (&labels[0], TITLE_SIZE, TITLE_HEIGHT),
                (&labels[1], HINT_SIZE, HINT_HEIGHT),
            ] {
                let measured = raster.measure_text(label, size);
                assert!(measured.x > 0., "{}: nothing measured", language.tag());
                if measured.x > MAX_LABEL_WIDTH || measured.y > height {
                    let tag = language.tag();
                    overflows.push(format!(
                        "{tag}: `{label}` is {:.0}×{:.0} px",
                        measured.x, measured.y
                    ));
                }
            }
        }
        assert!(
            overflows.is_empty(),
            "the panel holds {MAX_LABEL_WIDTH} px per line, shorten:\n{}",
            overflows.join("\n")
        );
    }
}
