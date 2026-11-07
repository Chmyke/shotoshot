//! ROLE: Portable CPU implementation of `Canvas` (tiny-skia paths, cosmic-text glyphs) for
//! Linux and macOS, plus the flattened export of a capture.
//! OWNER: One `Raster` per capture session keeps fonts, glyph cache, laid-out texts and
//! cached rasters.
//! INVARIANTS: Pixmaps are premultiplied RGBA; `Image` stays top-down BGRA at the edges.
//! Glyphs are composed straight into the pixels when text is not rotated.
pub mod present;
use cosmic_text::{
    Attrs, Buffer, Family, FontSystem, Metrics, PhysicalGlyph, Shaping, SwashCache, Weight, fontdb,
};
use shotoshot_core::{
    BlurPatch, Label, Object, Point, Rect, Shape, paths, stroke_runs, text_lines,
};
use shotoshot_ui::{Canvas, Face, Image, Ink, annotation_font};
use std::{
    collections::HashMap,
    hash::{Hash, Hasher},
    sync::Arc,
    time::{Duration, Instant},
};
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

/// How many laid-out texts are kept before the cache starts over: enough for every prefix
/// of a text being typed, measured on each pointer move, and the interface's labels.
const LAYOUTS_KEPT: usize = 512;

/// A text laid out once, drawn and measured from then on.
struct Layout {
    face: Face,
    size_bits: u32,
    text: String,
    buffer: Buffer,
    extent: Point,
}

/// The fonts of a `Raster` loading on their own thread, so that the capture and the window
/// do not wait for them (`FontSystem::new` reads every system font).
pub struct Loading(std::thread::JoinHandle<(FontSystem, Duration)>);
impl Loading {
    pub fn start() -> Self {
        Self(std::thread::spawn(|| {
            let started = Instant::now();
            let fonts = Raster::fonts();
            (fonts, started.elapsed())
        }))
    }
    /// The renderer once its fonts are in, and how long they took to load; a thread that
    /// failed loads them here.
    pub fn wait(self) -> (Raster, Duration) {
        match self.0.join() {
            Ok((fonts, took)) => (Raster::with_fonts(fonts), took),
            Err(_) => (Raster::new(), Duration::ZERO),
        }
    }
}

/// Session-lifetime rendering resources.
pub struct Raster {
    fonts: FontSystem,
    glyphs: SwashCache,
    /// Laid-out texts by the hash of (face, size, text), checked on each hit.
    layouts: HashMap<u64, Layout>,
    rasters: Vec<(&'static str, u64, Pixmap)>,
    /// What each blur makes of the capture, by object id with its version.
    blurs: Vec<(u64, u64, Pixmap)>,
    /// Coverage for fills with holes, kept at the size of the last target that needed one;
    /// only the rows of each fill's box are written.
    mask: Option<Mask>,
}
impl Default for Raster {
    fn default() -> Self {
        Self::new()
    }
}
impl Raster {
    pub fn new() -> Self {
        Self::with_fonts(Self::fonts())
    }
    /// The system fonts and the embedded annotation font.
    fn fonts() -> FontSystem {
        let mut fonts = FontSystem::new();
        let annotation = fontdb::Source::Binary(Arc::new(annotation_font::DATA));
        fonts.db_mut().load_font_source(annotation);
        fonts
    }
    fn with_fonts(fonts: FontSystem) -> Self {
        Self {
            fonts,
            glyphs: SwashCache::new(),
            layouts: HashMap::new(),
            rasters: Vec::new(),
            blurs: Vec::new(),
            mask: None,
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
    /// The lines a label draws `text` on, without the spaces they end with: typed lines,
    /// broken at `wrap` when given. The break is decided here, then nothing breaks again.
    pub fn label_lines<'t>(&mut self, text: &'t str, size: f64, wrap: Option<f64>) -> Vec<&'t str> {
        let mut measure = |s: &str| self.measure_annotation_text(s, size).x;
        text_lines(text, wrap, &mut measure)
            .into_iter()
            .map(|line| text[line].trim_end())
            .collect()
    }
    /// `text` broken into the lines a label `width` wide draws, one per line of the result.
    pub fn wrapped_annotation_text(&mut self, text: &str, size: f64, width: f64) -> String {
        self.label_lines(text, size, Some(width)).join("\n")
    }
    /// Extent of annotation `text` at `size` broken into lines `width` wide, exactly as a
    /// wrapped label draws it.
    pub fn measure_wrapped_annotation_text(&mut self, text: &str, size: f64, width: f64) -> Point {
        let lines = self.wrapped_annotation_text(text, size, width);
        self.measure_annotation_text(&lines, size)
    }
    fn extent(&mut self, text: &str, size: f64, face: Face) -> Point {
        let key = self.layout(text, size, face);
        self.layouts[&key].extent
    }
    /// The key of `text` laid out at `size` in `face`, laying it out on the first call.
    fn layout(&mut self, text: &str, size: f64, face: Face) -> u64 {
        let size = size as f32;
        let size_bits = size.to_bits();
        let mut hasher = std::hash::DefaultHasher::new();
        (face == Face::Annotation, size_bits, text).hash(&mut hasher);
        let key = hasher.finish();
        let hit = self
            .layouts
            .get(&key)
            .is_some_and(|l| l.face == face && l.size_bits == size_bits && l.text == text);
        if !hit {
            if self.layouts.len() >= LAYOUTS_KEPT {
                self.layouts.clear();
            }
            let fonts = &mut self.fonts;
            let mut buffer = Buffer::new(fonts, Metrics::new(size, size * line_height(face)));
            let mut borrowed = buffer.borrow_with(fonts);
            borrowed.set_size(None, None);
            borrowed.set_text(text, &attrs(face), Shaping::Advanced, None);
            borrowed.shape_until_scroll(false);
            let (mut width, mut height) = (0f32, 0f32);
            for run in buffer.layout_runs() {
                width = width.max(run.line_w);
                height = run.line_top + run.line_height;
            }
            let extent = Point {
                x: width as f64,
                y: height as f64,
            };
            self.layouts.insert(
                key,
                Layout {
                    face,
                    size_bits,
                    text: text.to_owned(),
                    buffer,
                    extent,
                },
            );
        }
        key
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
    /// coats laid over the captured pixels, the blurred zones blurred.
    pub fn export(
        &mut self,
        image: &Image,
        background: &[shotoshot_core::Paint],
        objects: &[Object],
        crop: Rect,
    ) -> Result<Image, RasterError> {
        image.check().map_err(RasterError)?;
        let blurred = image.blurred(objects);
        let image = &*blurred;
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
    /// `area` filled except inside the closed `holes`, which may overlap. The coverage mask
    /// is as large as the target (tiny-skia wants it so) but only the rows and columns of
    /// the area's box are written: cleared, holed, then inverted there.
    fn fill_except(&mut self, area: Option<tiny_skia::Path>, holes: &[Vec<Point>], ink: Ink) {
        if holes.is_empty() {
            return self.fill_path(area, ink, Transform::identity());
        }
        let Some(area) = area.and_then(|a| a.transform(self.transform)) else {
            return;
        };
        let (width, height) = (self.target.width(), self.target.height());
        let bounds = area.bounds();
        let x0 = (bounds.left().floor() - 1.).max(0.) as usize;
        let y0 = (bounds.top().floor() - 1.).max(0.) as usize;
        let x1 = ((bounds.right().ceil() + 1.).max(0.) as usize).min(width as usize);
        let y1 = ((bounds.bottom().ceil() + 1.).max(0.) as usize).min(height as usize);
        if x1 <= x0 || y1 <= y0 {
            return;
        }
        let mask = match &mut self.raster.mask {
            Some(mask) if mask.width() == width && mask.height() == height => mask,
            slot => match Mask::new(width, height) {
                Some(mask) => slot.insert(mask),
                None => return,
            },
        };
        let row = width as usize;
        for y in y0..y1 {
            mask.data_mut()[y * row + x0..y * row + x1].fill(0);
        }
        for hole in holes {
            if let Some(path) = polyline(hole, true) {
                mask.fill_path(&path, FillRule::Winding, true, self.transform);
            }
        }
        for y in y0..y1 {
            for coverage in &mut mask.data_mut()[y * row + x0..y * row + x1] {
                *coverage = 255 - *coverage;
            }
        }
        self.target.fill_path(
            &area,
            &paint(ink),
            FillRule::Winding,
            Transform::identity(),
            Some(mask),
        );
    }
    /// `text` laid out from `r.min`, each glyph's coverage composed into the pixels when
    /// `transform` only moves it, drawn as a small bitmap through the transform otherwise.
    /// `clip` cuts what lies at or past `r.max`, in the text's own coordinates.
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
        let key = raster.layout(text, size, face);
        let glyphs: Vec<PhysicalGlyph> = raster.layouts[&key]
            .buffer
            .layout_runs()
            .flat_map(|run| {
                run.glyphs
                    .iter()
                    .map(move |glyph| glyph.physical((0., run.line_y), 1.))
            })
            .collect();
        let alpha = (ink.alpha.clamp(0., 1.) * 255.).round() as u8;
        let transform = transform.post_concat(self.transform);
        let moved_only = !transform.has_scale() && !transform.has_skew();
        let (origin_x, origin_y) = (r.min.x as f32, r.min.y as f32);
        let limit = clip.then_some((r.max.x as f32, r.max.y as f32));
        for glyph in glyphs {
            let Some(image) = raster.glyphs.get_image(&mut raster.fonts, glyph.cache_key) else {
                continue;
            };
            let placement = image.placement;
            // The glyph's top-left corner in the text's coordinates, as cosmic-text lays it.
            let x = origin_x + (glyph.x + placement.left) as f32;
            let y = origin_y + (glyph.y - placement.top) as f32;
            // Columns and rows at or past the limit are cut.
            let kept = |start: f32, limit: Option<f32>, full: u32| {
                limit.map_or(full, |limit| {
                    ((limit - start).ceil().max(0.) as u32).min(full)
                })
            };
            let source = GlyphPixels {
                data: &image.data,
                colored: matches!(image.content, cosmic_text::SwashContent::Color),
                stride: placement.width,
                width: kept(x, limit.map(|l| l.0), placement.width),
                height: kept(y, limit.map(|l| l.1), placement.height),
            };
            if source.width == 0 || source.height == 0 {
                continue;
            }
            if moved_only {
                let at = (
                    (x + transform.tx).round() as i32,
                    (y + transform.ty).round() as i32,
                );
                compose(self.target, at, &source, ink.rgb, alpha);
            } else if let Some(bitmap) = tinted(&source, ink.rgb, alpha) {
                let paint = PixmapPaint {
                    quality: FilterQuality::Bilinear,
                    ..PixmapPaint::default()
                };
                let at = Transform::from_translate(x, y).post_concat(transform);
                self.target
                    .draw_pixmap(0, 0, bitmap.as_ref(), &paint, at, None);
            }
        }
    }
}

/// The pixels of one rasterised glyph: a coverage byte per pixel, or straight RGBA when
/// `colored` (emoji); `width` × `height` of them are drawn, on rows `stride` pixels long.
struct GlyphPixels<'a> {
    data: &'a [u8],
    colored: bool,
    stride: u32,
    width: u32,
    height: u32,
}
impl GlyphPixels<'_> {
    /// Straight colour and coverage of the pixel at (`col`, `row`), tinted `rgb` at `alpha`.
    fn pixel(&self, col: u32, row: u32, rgb: [u8; 3], alpha: u8) -> ([u8; 3], u8) {
        let i = (row * self.stride + col) as usize;
        let (rgb, coverage) = if self.colored {
            let p = &self.data[i * 4..i * 4 + 4];
            ([p[0], p[1], p[2]], p[3])
        } else {
            (rgb, self.data[i])
        };
        (rgb, div255(coverage as u32 * alpha as u32))
    }
}
/// `x / 255`, rounded, for `x` up to `255 * 255`.
fn div255(x: u32) -> u8 {
    ((x + 128 + ((x + 128) >> 8)) >> 8) as u8
}
/// Lays `glyph` over `target` with its top-left corner at `at`, source-over on the
/// premultiplied pixels, clipped to the target.
fn compose(target: &mut Pixmap, at: (i32, i32), glyph: &GlyphPixels, rgb: [u8; 3], alpha: u8) {
    let (width, height) = (target.width() as i32, target.height() as i32);
    let stride = target.width() as usize * 4;
    let data = target.data_mut();
    for row in 0..glyph.height as i32 {
        let y = at.1 + row;
        if y < 0 || y >= height {
            continue;
        }
        let first = (-at.0).max(0);
        let last = (width - at.0).min(glyph.width as i32);
        for col in first..last {
            let (color, coverage) = glyph.pixel(col as u32, row as u32, rgb, alpha);
            if coverage == 0 {
                continue;
            }
            let i = y as usize * stride + (at.0 + col) as usize * 4;
            let keep = 255 - coverage as u32;
            let dst = &mut data[i..i + 4];
            for (channel, value) in dst.iter_mut().zip([color[0], color[1], color[2], 255]) {
                *channel = div255(value as u32 * coverage as u32 + *channel as u32 * keep);
            }
        }
    }
}
/// `glyph` as a premultiplied bitmap of its own, for drawing through a transform.
fn tinted(glyph: &GlyphPixels, rgb: [u8; 3], alpha: u8) -> Option<Pixmap> {
    let mut bitmap = Pixmap::new(glyph.width, glyph.height)?;
    for (i, pixel) in bitmap
        .data_mut()
        .as_chunks_mut::<4>()
        .0
        .iter_mut()
        .enumerate()
    {
        let (col, row) = (i as u32 % glyph.width, i as u32 / glyph.width);
        let (color, coverage) = glyph.pixel(col, row, rgb, alpha);
        *pixel = [
            div255(color[0] as u32 * coverage as u32),
            div255(color[1] as u32 * coverage as u32),
            div255(color[2] as u32 * coverage as u32),
            coverage,
        ];
    }
    Some(bitmap)
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
    fn blurs(
        &mut self,
        patches: &[BlurPatch],
        pixels: &mut dyn FnMut(usize) -> Vec<u8>,
    ) -> Result<(), RasterError> {
        self.raster
            .blurs
            .retain(|(id, _, _)| patches.iter().any(|p| p.id == *id));
        for (i, patch) in patches.iter().enumerate() {
            let cache = &mut self.raster.blurs;
            let version = patch.version();
            let index = match cache.iter().position(|(id, _, _)| *id == patch.id) {
                Some(at) if cache[at].1 == version => at,
                found => {
                    let zone = patch.zone;
                    let mut pixmap =
                        Pixmap::new(zone.width, zone.height).ok_or(RasterError("Empty blur"))?;
                    let bgra = pixels(i);
                    if bgra.len() != pixmap.data().len() {
                        return Err(RasterError("Blur buffer length"));
                    }
                    for (to, from) in pixmap
                        .data_mut()
                        .as_chunks_mut::<4>()
                        .0
                        .iter_mut()
                        .zip(bgra.as_chunks::<4>().0)
                    {
                        *to = [from[2], from[1], from[0], 255];
                    }
                    match found {
                        Some(at) => {
                            cache[at] = (patch.id, version, pixmap);
                            at
                        }
                        None => {
                            cache.push((patch.id, version, pixmap));
                            cache.len() - 1
                        }
                    }
                }
            };
            let at = Transform::from_translate(patch.zone.x as f32, patch.zone.y as f32)
                .post_concat(self.transform);
            let source = &self.raster.blurs[index].2;
            self.target
                .draw_pixmap(0, 0, source.as_ref(), &PixmapPaint::default(), at, None);
        }
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
                    label,
                    ink: _,
                } => {
                    let center = o.local_bounds().center();
                    let transform = Transform::from_rotate_at(
                        o.rotation.to_degrees() as f32,
                        center.x as f32,
                        center.y as f32,
                    );
                    if label.is_some() {
                        let panel = round_rect(o.local_bounds(), Label::radius(*size));
                        self.fill_path(panel, Ink::solid(o.style.color), transform);
                    }
                    let ink = Ink::solid(o.text_ink());
                    let Some(label) = label else {
                        let r = Rect {
                            min: *position,
                            max: *position + *extent + Point { x: 20., y: 20. },
                        };
                        self.text_at(text, r, (Face::Annotation, *size), ink, transform, false);
                        continue;
                    };
                    // Each line of a label sits in the middle of its panel.
                    let width = label.wrap().unwrap_or(extent.x);
                    let line_height = *size * annotation_font::LINE_HEIGHT as f64;
                    let lines = self.raster.label_lines(text, *size, label.wrap());
                    for (row, line) in lines.into_iter().enumerate() {
                        let measured = self.raster.measure_annotation_text(line, *size).x;
                        let min = *position
                            + Point {
                                x: ((width - measured) / 2.).max(0.),
                                y: row as f64 * line_height,
                            };
                        let r = Rect {
                            min,
                            max: min + Point { x: 1e6, y: 1e6 },
                        };
                        self.text_at(line, r, (Face::Annotation, *size), ink, transform, false);
                    }
                }
                // A blur belongs to the capture: `blurs` drew it under everything.
                Shape::Blur(_) => {}
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
    fn a_text_is_laid_out_once_however_often_it_is_measured_or_drawn() {
        let mut raster = Raster::new();
        let first = raster.measure_annotation_text("Shotoshot", 24.);
        assert_eq!(raster.layouts.len(), 1);
        for _ in 0..3 {
            assert_eq!(raster.measure_annotation_text("Shotoshot", 24.), first);
            let mut frame = blank();
            let r = Rect::new(at(10., 10.), at(390., 190.)).unwrap();
            raster
                .canvas(&mut frame)
                .annotation_text("Shotoshot", r, 24., Ink::solid([0; 3]))
                .unwrap();
        }
        assert_eq!(
            raster.layouts.len(),
            1,
            "measured and drawn from the same layout"
        );
        // Another size or face is another layout; the same text in both faces differs.
        raster.measure_annotation_text("Shotoshot", 25.);
        raster.measure_text("Shotoshot", 24.);
        assert_eq!(raster.layouts.len(), 3);
        assert_ne!(
            raster.measure_text("Shotoshot", 24.),
            first,
            "the interface face is not the annotation face"
        );
        // A full cache starts over instead of growing without bound.
        for i in 0..LAYOUTS_KEPT {
            raster.measure_text(&format!("line {i}"), 12.);
        }
        assert!(raster.layouts.len() <= LAYOUTS_KEPT);
        assert_eq!(raster.measure_annotation_text("Shotoshot", 24.), first);
    }
    #[test]
    fn translucent_text_is_lighter_than_opaque_text() {
        let darkness = |alpha: f32| {
            let mut raster = Raster::new();
            let mut frame = blank();
            let r = Rect::new(at(10., 10.), at(390., 190.)).unwrap();
            raster
                .canvas(&mut frame)
                .text("Shotoshot", r, 24., Ink::with_alpha([0; 3], alpha))
                .unwrap();
            (0..400u32)
                .flat_map(|x| (0..200u32).map(move |y| (x, y)))
                .map(|(x, y)| 255 - frame.pixel(x, y).map_or(255, |c| c.red()) as u32)
                .sum::<u32>()
        };
        let (opaque, half) = (darkness(1.), darkness(0.5));
        assert!(opaque > 2000, "text missing ({opaque})");
        assert!(
            half * 10 > opaque * 4 && half * 10 < opaque * 6,
            "half-transparent text should be about half as dark: {half} of {opaque}"
        );
        assert_eq!(darkness(0.), 0, "invisible text leaves no mark");
    }
    #[test]
    fn rotated_text_is_drawn_where_the_rotation_puts_it() {
        let mut raster = Raster::new();
        let mut frame = blank();
        let r = Rect::new(at(200., 100.), at(1e6, 1e6)).unwrap();
        {
            let mut canvas = raster.canvas(&mut frame);
            canvas
                .set_rotation(Some((at(200., 100.), std::f64::consts::PI)))
                .unwrap();
            canvas
                .annotation_text("Shotoshot", r, 24., Ink::solid([0; 3]))
                .unwrap();
            canvas.set_rotation(None).unwrap();
        }
        // Turned half a circle around its own corner, the text lies up and to the left.
        assert!(inked(&frame, 60..200, 60..100) > 60, "rotated text missing");
        assert_eq!(
            inked(&frame, 200..400, 100..200),
            0,
            "nothing where it would lie unrotated"
        );
    }
    #[test]
    fn fills_with_holes_do_not_remember_each_other() {
        let mut raster = Raster::new();
        let mut frame = blank();
        let hole = |min: Point, max: Point| vec![min, at(max.x, min.y), max, at(min.x, max.y)];
        {
            let mut canvas = raster.canvas(&mut frame);
            let left = Rect::new(at(0., 0.), at(200., 200.)).unwrap();
            canvas
                .fill_rect_except(
                    left,
                    &[hole(at(50., 50.), at(100., 100.))],
                    Ink::solid(SLATE),
                )
                .unwrap();
            // The second fill covers where the first one's hole was: it must be filled.
            let whole = Rect::new(at(0., 0.), at(400., 200.)).unwrap();
            canvas
                .fill_rect_except(
                    whole,
                    &[hole(at(300., 50.), at(350., 100.))],
                    Ink::solid(GREEN),
                )
                .unwrap();
        }
        let image = image_from_pixmap(&frame, 0, 0);
        assert_eq!(
            rgb(&image, 75, 75),
            GREEN,
            "the first hole is no hole for the second fill"
        );
        assert_eq!(rgb(&image, 325, 75), PAPER, "the second hole is spared");
        assert_eq!(rgb(&image, 10, 10), GREEN);
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

    /// Real text of 20 px, set in the annotation face, loses what makes it readable under a
    /// blur of the default strength: the fine contrast of its strokes, and what tells two
    /// different lines apart. Same measures as `shotoshot-core/tests/blur.rs`.
    #[test]
    fn twenty_pixel_text_is_unreadable_under_a_default_blur() {
        let (width, height) = (400u32, 60u32);
        let page = |line: &str| {
            let mut frame = pixmap_from_bgra(&Image::solid(width, height, [255; 4])).unwrap();
            let mut raster = Raster::new();
            let r = Rect::new(at(10., 15.), at(390., 50.)).unwrap();
            raster
                .canvas(&mut frame)
                .annotation_text(line, r, 20., Ink::solid([20, 20, 20]))
                .unwrap();
            image_from_pixmap(&frame, 0, 0).pixels
        };
        let luma = |pixels: &[u8]| -> Vec<f64> {
            let chunks = pixels.as_chunks::<4>().0;
            chunks.iter().map(|p| p[1] as f64).collect()
        };
        let (w, h) = (width as usize, height as usize);
        let detail = |l: &[f64]| -> f64 {
            let mut energy = 0.;
            for y in 2..h - 2 {
                for x in 2..w - 2 {
                    let mean = (y - 2..=y + 2)
                        .flat_map(|yy| (x - 2..=x + 2).map(move |xx| (xx, yy)))
                        .map(|(xx, yy)| l[yy * w + xx])
                        .sum::<f64>()
                        / 25.;
                    energy += (l[y * w + x] - mean).powi(2);
                }
            }
            energy
        };
        let zone = shotoshot_core::blur::Zone {
            x: 0,
            y: 0,
            width,
            height,
        };
        let blur = |pixels: &[u8]| {
            let radius = shotoshot_core::BLUR_RADIUS as u32;
            shotoshot_core::blur::patch(pixels, width, zone, radius, 1)
        };
        let (a, b) = (
            page("Invoice 4921 7730 jane@mail.com"),
            page("Account 8156 2047 marc@site.org"),
        );
        let (blur_a, blur_b) = (blur(&a), blur(&b));
        let kept = detail(&luma(&blur_a)) / detail(&luma(&a));
        assert!(
            kept < 0.01,
            "{kept:.4} of the strokes' fine contrast survives"
        );
        let difference = |x: &[u8], y: &[u8]| {
            luma(x)
                .iter()
                .zip(luma(y))
                .map(|(p, q)| (p - q).powi(2))
                .sum::<f64>()
        };
        let apart = difference(&blur_a, &blur_b) / difference(&a, &b);
        assert!(
            apart < 0.1,
            "{apart:.4} of what tells the lines apart survives"
        );
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
