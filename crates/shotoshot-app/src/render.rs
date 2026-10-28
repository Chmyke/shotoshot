//! ROLE: Shared Direct2D drawing for the overlay and flattened output.
//! OWNER: Each capture owns one renderer; no rendering exists while resident.
use crate::{Image, ImageExt};
use shotoshot_core::{Object, Paint, Point, Rect, Shape, paths};
use shotoshot_ui::{Canvas, Ink, View};
use windows::{
    Win32::{
        Graphics::{
            Direct2D::Common::*, Direct2D::*, DirectWrite::*, Dxgi::Common::*, Gdi::*, Imaging::*,
        },
        System::Com::*,
    },
    core::*,
};
use windows_numerics::{Matrix3x2, Vector2};

pub(crate) mod annotation_font;
mod scene;

/// Opacity of the objects a sweep is about to erase: the highlighter's, already in the eye.
const FADING: f32 = 0.3;
#[cfg(test)]
mod locale_tests;
#[cfg(test)]
mod scene_tests;
#[cfg(test)]
mod stroke_tests;
mod strokes;

pub struct Renderer {
    factory: ID2D1Factory,
    write: IDWriteFactory,
    round_stroke: ID2D1StrokeStyle,
    pub(crate) arrow_stroke: ID2D1StrokeStyle,
    pub dc: ID2D1DCRenderTarget,
    backdrop: Option<ID2D1Bitmap>,
    // What lies over the annotations (the desktop icons of a board), kept with its bitmap for
    // this render target; export targets get their own from the picture.
    overlay: Option<(Image, ID2D1Bitmap)>,
    // Canvas rasters by key with their version; they die with this render target.
    rasters: std::cell::RefCell<Vec<(&'static str, u64, ID2D1Bitmap)>>,
    strokes: std::cell::RefCell<strokes::StrokeCache>,
    scene: std::cell::RefCell<scene::SceneCache>,
    backend: Backend,
    transparent: bool,
}
/// What rasterizes the render target.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Backend {
    /// Direct3D when Windows has it: about 2 ms a frame on a 5760×1080 desktop, but about
    /// 150 ms to create the device.
    Hardware,
    /// CPU: created in about 13 ms and about 12 ms a frame; no Direct3D device, whose memory
    /// would outlive the renderer in a process that otherwise never draws.
    Software,
}
/// A renderer on its way from the thread that built it to the thread that will draw with it.
pub struct Prepared(Renderer);
// SAFETY: the objects of a single-threaded Direct2D factory have no thread affinity; they only
// require that calls never overlap. The building thread gives the renderer away and keeps nothing.
unsafe impl Send for Prepared {}
impl Prepared {
    pub fn new(backend: Backend, transparent: bool) -> Result<Self> {
        Renderer::with(backend, transparent).map(Self)
    }
    pub fn into_renderer(self) -> Renderer {
        self.0
    }
}
pub fn vector(p: Point) -> Vector2 {
    Vector2 {
        X: p.x as f32,
        Y: p.y as f32,
    }
}
pub fn rect(r: Rect) -> D2D_RECT_F {
    D2D_RECT_F {
        left: r.min.x as f32,
        top: r.min.y as f32,
        right: r.max.x as f32,
        bottom: r.max.y as f32,
    }
}
pub fn color(rgb: [u8; 3], alpha: f32) -> D2D1_COLOR_F {
    D2D1_COLOR_F {
        r: rgb[0] as f32 / 255.,
        g: rgb[1] as f32 / 255.,
        b: rgb[2] as f32 / 255.,
        a: alpha,
    }
}
/// Draws `bitmap` whole at the target's origin, pixel for pixel.
fn draw_bitmap(target: &ID2D1RenderTarget, bitmap: &ID2D1Bitmap) {
    draw_bitmap_with(target, bitmap, false)
}
/// `smooth` interpolates linearly: a magnified desktop stays legible rather than blocky.
fn draw_bitmap_with(target: &ID2D1RenderTarget, bitmap: &ID2D1Bitmap, smooth: bool) {
    unsafe {
        target.DrawBitmap(
            bitmap,
            None,
            1.,
            if smooth {
                D2D1_BITMAP_INTERPOLATION_MODE_LINEAR
            } else {
                D2D1_BITMAP_INTERPOLATION_MODE_NEAREST_NEIGHBOR
            },
            None,
        );
    }
}
fn properties() -> D2D1_RENDER_TARGET_PROPERTIES {
    D2D1_RENDER_TARGET_PROPERTIES {
        pixelFormat: D2D1_PIXEL_FORMAT {
            format: DXGI_FORMAT_B8G8R8A8_UNORM,
            alphaMode: D2D1_ALPHA_MODE_IGNORE,
        },
        dpiX: 96.,
        dpiY: 96.,
        ..Default::default()
    }
}
impl Renderer {
    /// The typeface of interface text at `size`.
    fn interface_format(&self, size: f32) -> Result<IDWriteTextFormat> {
        self.interface_format_in(size, shotoshot_text::language())
    }
    /// The typeface of interface text at `size`, shaped for `language` (the locale picks
    /// the regional glyph forms of shared Han characters and the fallback fonts).
    fn interface_format_in(
        &self,
        size: f32,
        language: shotoshot_text::Language,
    ) -> Result<IDWriteTextFormat> {
        unsafe {
            self.write.CreateTextFormat(
                w!("Segoe UI"),
                None,
                DWRITE_FONT_WEIGHT_NORMAL,
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                size,
                &HSTRING::from(language.locale_name()),
            )
        }
    }
    pub fn measure_text(&self, text: &str, size: f32) -> Result<Point> {
        self.measure_text_in(text, size, shotoshot_text::language())
    }
    /// Extent of interface `text` at `size` as it is drawn in `language`.
    pub fn measure_text_in(
        &self,
        text: &str,
        size: f32,
        language: shotoshot_text::Language,
    ) -> Result<Point> {
        self.measure(text, &self.interface_format_in(size, language)?, None)
    }
    /// Extent of interface `text` at `size` in `language`, wrapped at `width` as it is when
    /// drawn into a rectangle that wide.
    pub fn measure_wrapped_text_in(
        &self,
        text: &str,
        size: f32,
        width: f32,
        language: shotoshot_text::Language,
    ) -> Result<Point> {
        self.measure(
            text,
            &self.interface_format_in(size, language)?,
            Some(width),
        )
    }
    /// Extent of annotation `text` at `size`, exactly as annotations draw it.
    pub fn measure_annotation_text(&self, text: &str, size: f32) -> Result<Point> {
        self.measure(text, &annotation_font::format(&self.write, size)?, None)
    }
    /// Extent of `text` on one line, or wrapped at `wrap` when given.
    fn measure(&self, text: &str, format: &IDWriteTextFormat, wrap: Option<f32>) -> Result<Point> {
        unsafe {
            if wrap.is_none() {
                format.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)?;
            }
            let layout = self.write.CreateTextLayout(
                &text.encode_utf16().collect::<Vec<_>>(),
                format,
                wrap.unwrap_or(1_000_000.),
                1_000_000.,
            )?;
            let mut metrics = DWRITE_TEXT_METRICS::default();
            layout.GetMetrics(&mut metrics)?;
            Ok(Point {
                // A wrapped line's trailing space hangs past the edge without being drawn.
                x: if wrap.is_some() {
                    metrics.width
                } else {
                    metrics.widthIncludingTrailingWhitespace
                } as f64,
                y: metrics.height as f64,
            })
        }
    }
    pub fn new() -> Result<Self> {
        Self::with_transparency(false)
    }
    pub fn with_transparency(transparent: bool) -> Result<Self> {
        Self::with(Backend::Hardware, transparent)
    }
    /// For the resident's notification menu and windows, which must leave no device behind.
    pub fn software() -> Result<Self> {
        Self::with(Backend::Software, false)
    }
    pub fn with(backend: Backend, transparent: bool) -> Result<Self> {
        let mut props = properties();
        if backend == Backend::Software {
            props.r#type = D2D1_RENDER_TARGET_TYPE_SOFTWARE;
        }
        if transparent {
            props.pixelFormat.alphaMode = D2D1_ALPHA_MODE_PREMULTIPLIED;
        }
        unsafe {
            let factory: ID2D1Factory = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
            let write = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
            let dc = factory.CreateDCRenderTarget(&props)?;
            let round_stroke = factory.CreateStrokeStyle(
                &D2D1_STROKE_STYLE_PROPERTIES {
                    startCap: D2D1_CAP_STYLE_ROUND,
                    endCap: D2D1_CAP_STYLE_ROUND,
                    dashCap: D2D1_CAP_STYLE_ROUND,
                    lineJoin: D2D1_LINE_JOIN_ROUND,
                    miterLimit: 1.,
                    dashStyle: D2D1_DASH_STYLE_SOLID,
                    dashOffset: 0.,
                },
                None,
            )?;
            let arrow_stroke = factory.CreateStrokeStyle(
                &D2D1_STROKE_STYLE_PROPERTIES {
                    startCap: D2D1_CAP_STYLE_ROUND,
                    endCap: D2D1_CAP_STYLE_FLAT,
                    lineJoin: D2D1_LINE_JOIN_ROUND,
                    ..Default::default()
                },
                None,
            )?;
            Ok(Self {
                factory,
                write,
                round_stroke,
                arrow_stroke,
                dc,
                backdrop: None,
                overlay: None,
                rasters: Default::default(),
                strokes: Default::default(),
                scene: Default::default(),
                backend,
                transparent,
            })
        }
    }
    pub fn backend(&self) -> Backend {
        self.backend
    }
    pub fn transparent(&self) -> bool {
        self.transparent
    }
    pub fn set_image(&mut self, image: &Image) -> Result<()> {
        self.backdrop = Some(self.bitmap(&self.dc, image)?);
        self.scene.get_mut().invalidate();
        Ok(())
    }
    fn bitmap(&self, target: &ID2D1RenderTarget, image: &Image) -> Result<ID2D1Bitmap> {
        image.validate()?;
        unsafe {
            target.CreateBitmap(
                D2D_SIZE_U {
                    width: image.width,
                    height: image.height,
                },
                Some(image.pixels.as_ptr().cast()),
                image.width * 4,
                &D2D1_BITMAP_PROPERTIES {
                    pixelFormat: properties().pixelFormat,
                    dpiX: 96.,
                    dpiY: 96.,
                },
            )
        }
    }
    /// Lays `image`, premultiplied BGRA with its own alpha, over the annotations of every
    /// paint and export; `None` lays nothing. Transparent pixels show the notes through.
    pub fn set_overlay(&mut self, image: Option<Image>) -> Result<()> {
        self.overlay = match image {
            Some(image) => {
                let bitmap = self.alpha_bitmap(&self.dc, &image)?;
                Some((image, bitmap))
            }
            None => None,
        };
        Ok(())
    }
    /// The picture laid over the annotations, to hand to a replacement renderer.
    pub fn overlay(&self) -> Option<&Image> {
        self.overlay.as_ref().map(|(image, _)| image)
    }
    /// Draws the overlay on this renderer's target, at the frame's origin, through the
    /// current transform; `smooth` for a magnified view.
    pub fn paint_overlay(&self, smooth: bool) {
        if let Some((_, bitmap)) = &self.overlay {
            draw_bitmap_with(&self.dc, bitmap, smooth);
        }
    }
    /// The capture under everything, through the current transform; `smooth` for a
    /// magnified view.
    pub fn paint_backdrop(&self, smooth: bool) {
        if let Some(bitmap) = &self.backdrop {
            draw_bitmap_with(&self.dc, bitmap, smooth);
        }
    }
    /// Draws what follows through `view`: scaled by its zoom from its origin, clipped to its
    /// screen. `pop_view` undoes both.
    pub fn push_view(&self, view: &View) {
        let zoom = view.zoom as f32;
        unsafe {
            self.dc
                .PushAxisAlignedClip(&rect(view.screen), D2D1_ANTIALIAS_MODE_ALIASED);
            self.dc.SetTransform(&Matrix3x2 {
                M11: zoom,
                M12: 0.,
                M21: 0.,
                M22: zoom,
                M31: (view.screen.min.x - view.origin.x * view.zoom) as f32,
                M32: (view.screen.min.y - view.origin.y * view.zoom) as f32,
            });
        }
    }
    pub fn pop_view(&self) {
        unsafe {
            self.dc.SetTransform(&Matrix3x2::identity());
            self.dc.PopAxisAlignedClip();
        }
    }
    fn alpha_bitmap(&self, target: &ID2D1RenderTarget, image: &Image) -> Result<ID2D1Bitmap> {
        image.validate()?;
        unsafe {
            target.CreateBitmap(
                D2D_SIZE_U {
                    width: image.width,
                    height: image.height,
                },
                Some(image.pixels.as_ptr().cast()),
                image.width * 4,
                &D2D1_BITMAP_PROPERTIES {
                    pixelFormat: D2D1_PIXEL_FORMAT {
                        format: DXGI_FORMAT_B8G8R8A8_UNORM,
                        alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
                    },
                    dpiX: 96.,
                    dpiY: 96.,
                },
            )
        }
    }
    pub fn text(
        &self,
        target: &ID2D1RenderTarget,
        text: &str,
        r: Rect,
        size: f32,
        rgb: [u8; 3],
    ) -> Result<()> {
        self.text_opacity(target, text, r, size, (rgb, 1.))
    }
    pub fn text_opacity(
        &self,
        target: &ID2D1RenderTarget,
        text: &str,
        r: Rect,
        size: f32,
        ink: ([u8; 3], f32),
    ) -> Result<()> {
        self.draw_text(target, text, r, &self.interface_format(size)?, ink)
    }
    /// Like `text_opacity`, in the typeface of text annotations.
    pub fn annotation_text(
        &self,
        target: &ID2D1RenderTarget,
        text: &str,
        r: Rect,
        size: f32,
        ink: ([u8; 3], f32),
    ) -> Result<()> {
        let format = annotation_font::format(&self.write, size)?;
        self.draw_text(target, text, r, &format, ink)
    }
    fn draw_text(
        &self,
        target: &ID2D1RenderTarget,
        text: &str,
        r: Rect,
        format: &IDWriteTextFormat,
        ink: ([u8; 3], f32),
    ) -> Result<()> {
        unsafe {
            let brush = target.CreateSolidColorBrush(&color(ink.0, ink.1), None)?;
            target.DrawText(
                &text.encode_utf16().collect::<Vec<_>>(),
                format,
                &rect(r),
                &brush,
                D2D1_DRAW_TEXT_OPTIONS_CLIP,
                DWRITE_MEASURING_MODE_NATURAL,
            );
            Ok(())
        }
    }
    pub fn annotations(&self, target: &ID2D1RenderTarget, objects: &[Object]) -> Result<()> {
        self.annotations_except(target, objects, None)
    }
    pub fn annotations_except(
        &self,
        target: &ID2D1RenderTarget,
        objects: &[Object],
        skip: Option<u64>,
    ) -> Result<()> {
        self.strokes.borrow_mut().retain_present(objects);
        self.annotation_range(target, objects, skip, 0..objects.len())
    }
    /// The closed polygon through `points`.
    fn polygon(&self, points: &[Point]) -> Result<ID2D1Geometry> {
        unsafe {
            let geometry = self.factory.CreatePathGeometry()?;
            let sink = geometry.Open()?;
            if let Some((first, rest)) = points.split_first() {
                sink.BeginFigure(vector(*first), D2D1_FIGURE_BEGIN_FILLED);
                for p in rest {
                    sink.AddLine(vector(*p));
                }
                sink.EndFigure(D2D1_FIGURE_END_CLOSED);
            }
            sink.Close()?;
            Ok(geometry.into())
        }
    }
    /// `area` filled with `brush` except inside the closed `holes`, which may overlap.
    fn fill_except(
        &self,
        target: &ID2D1RenderTarget,
        mut area: ID2D1Geometry,
        holes: &[Vec<Point>],
        brush: &ID2D1Brush,
    ) -> Result<()> {
        unsafe {
            for hole in holes {
                let rest = self.factory.CreatePathGeometry()?;
                let sink = rest.Open()?;
                area.CombineWithGeometry(
                    &self.polygon(hole)?,
                    D2D1_COMBINE_MODE_EXCLUDE,
                    None,
                    D2D1_DEFAULT_FLATTENING_TOLERANCE,
                    &sink,
                )?;
                sink.Close()?;
                area = rest.into();
            }
            target.FillGeometry(&area, brush, None);
        }
        Ok(())
    }
    fn annotation_range(
        &self,
        target: &ID2D1RenderTarget,
        objects: &[Object],
        skip: Option<u64>,
        range: std::ops::Range<usize>,
    ) -> Result<()> {
        let mut strokes = self.strokes.borrow_mut();
        unsafe {
            for index in range {
                let o = &objects[index];
                if Some(o.id) == skip {
                    continue;
                }
                if let Shape::Text {
                    position,
                    text,
                    size,
                    extent,
                } = &o.shape
                {
                    let mut previous = Matrix3x2::identity();
                    target.GetTransform(&mut previous);
                    let rotation = Matrix3x2::rotation_around(
                        o.rotation.to_degrees() as f32,
                        vector(o.local_bounds().center()),
                    );
                    target.SetTransform(&(rotation * previous));
                    let result = self.annotation_text(
                        target,
                        text,
                        Rect {
                            min: *position,
                            max: *position + *extent + Point { x: 20., y: 20. },
                        },
                        *size as f32,
                        (o.style.color, 1.),
                    );
                    target.SetTransform(&previous);
                    result?;
                } else {
                    let highlight = matches!(
                        o.shape,
                        Shape::Stroke {
                            highlight: true,
                            ..
                        }
                    );
                    let brush = target.CreateSolidColorBrush(
                        &color(o.style.color, if highlight { 0.3 } else { 1. }),
                        None,
                    )?;
                    if matches!(o.shape, Shape::Stroke { .. }) {
                        #[cfg(test)]
                        scene_tests::record_fill();
                        target.FillGeometry(strokes.outline(self, o)?, &brush, None);
                        continue;
                    }
                    let width = if highlight {
                        o.style.width * 5.
                    } else {
                        o.style.width
                    };
                    let stroke = match o.shape {
                        Shape::Arrow { head: true, .. } => Some(&self.arrow_stroke),
                        Shape::Arrow { head: false, .. } => Some(&self.round_stroke),
                        _ => None,
                    };
                    for path in paths(o, objects) {
                        let Some(first) = path.first() else {
                            continue;
                        };
                        let geometry = self.factory.CreatePathGeometry()?;
                        let sink = geometry.Open()?;
                        let arrow = matches!(o.shape, Shape::Arrow { .. });
                        let filled_head = arrow && path.first() == path.last();
                        let fill = if arrow { &[][..] } else { &o.fill[..] };
                        sink.BeginFigure(
                            vector(*first),
                            if filled_head || !fill.is_empty() {
                                D2D1_FIGURE_BEGIN_FILLED
                            } else {
                                D2D1_FIGURE_BEGIN_HOLLOW
                            },
                        );
                        for p in &path[1..] {
                            sink.AddLine(vector(*p));
                        }
                        sink.EndFigure(if path.first() == path.last() {
                            D2D1_FIGURE_END_CLOSED
                        } else {
                            D2D1_FIGURE_END_OPEN
                        });
                        sink.Close()?;
                        for coat in fill {
                            let interior =
                                target.CreateSolidColorBrush(&color(coat.color, 1.), None)?;
                            let holes = holes(coat, objects, index + 1);
                            self.fill_except(target, geometry.clone().into(), &holes, &interior)?;
                        }
                        if filled_head {
                            target.FillGeometry(&geometry, &brush, None);
                        } else {
                            target.DrawGeometry(&geometry, &brush, width as f32, stroke);
                        }
                    }
                }
            }
            Ok(())
        }
    }
    pub fn begin(&self, hdc: HDC, width: i32, height: i32) -> Result<()> {
        self.start(hdc, width, height, true)
    }
    /// `begin` without the backdrop: the caller paints it view by view.
    pub fn begin_blank(&self, hdc: HDC, width: i32, height: i32) -> Result<()> {
        self.start(hdc, width, height, false)
    }
    fn start(&self, hdc: HDC, width: i32, height: i32, backdrop: bool) -> Result<()> {
        unsafe {
            self.dc.BindDC(
                hdc,
                &windows::Win32::Foundation::RECT {
                    left: 0,
                    top: 0,
                    right: width,
                    bottom: height,
                },
            )?;
            self.dc.BeginDraw();
            self.dc.SetTransform(&Matrix3x2::identity());
            // Alpha 1/255 is visually negligible but keeps mouse input on the
            // annotation overlay instead of passing through to underlying apps.
            self.dc.Clear(Some(&color([0, 0, 0], 1. / 255.)));
        }
        if backdrop {
            self.paint_backdrop(false);
        }
        Ok(())
    }
    pub fn end(&self) -> Result<()> {
        unsafe { self.dc.EndDraw(None, None) }
    }
    /// The capture flattened with its annotations, cropped to `crop`, the `background`'s
    /// coats laid over the captured pixels.
    pub fn export(
        &self,
        image: &Image,
        background: &[Paint],
        objects: &[Object],
        crop: Rect,
    ) -> Result<Image> {
        unsafe {
            let left = crop.min.x.floor().clamp(0., image.width as f64) as u32;
            let top = crop.min.y.floor().clamp(0., image.height as f64) as u32;
            let right = crop.max.x.ceil().clamp(0., image.width as f64) as u32;
            let bottom = crop.max.y.ceil().clamp(0., image.height as f64) as u32;
            if right <= left || bottom <= top {
                return Err(Error::new(
                    windows::Win32::Foundation::E_INVALIDARG,
                    "Empty capture region",
                ));
            }
            let width = right - left;
            let height = bottom - top;
            let wic: IWICImagingFactory =
                CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)?;
            let bitmap = wic.CreateBitmap(
                width,
                height,
                &GUID_WICPixelFormat32bppBGR,
                WICBitmapCacheOnLoad,
            )?;
            let target = self
                .factory
                .CreateWicBitmapRenderTarget(&bitmap, &properties())?;
            // A coat that spares nothing hides the captured pixels: no need to decode them.
            let covered = background
                .iter()
                .any(|coat| coat.holes(objects, 0).next().is_none());
            let source = if covered {
                None
            } else {
                Some(self.bitmap(&target, image)?)
            };
            target.BeginDraw();
            target.SetTransform(&Matrix3x2::translation(-(left as f32), -(top as f32)));
            if let Some(source) = &source {
                target.DrawBitmap(
                    source,
                    None,
                    1.,
                    D2D1_BITMAP_INTERPOLATION_MODE_NEAREST_NEIGHBOR,
                    None,
                );
            }
            let whole = Rect {
                min: Point::default(),
                max: Point {
                    x: image.width as f64,
                    y: image.height as f64,
                },
            };
            let coats = background.iter().try_for_each(|coat| {
                let brush = target.CreateSolidColorBrush(&color(coat.color, 1.), None)?;
                let area = self.polygon(&whole.corners())?;
                self.fill_except(&target, area, &holes(coat, objects, 0), &brush)
            });
            let result = coats
                .and_then(|()| self.annotations(&target, objects))
                .and_then(|()| {
                    if let Some((image, _)) = &self.overlay {
                        draw_bitmap(&target, &self.alpha_bitmap(&target, image)?);
                    }
                    Ok(())
                });
            let end = target.EndDraw(None, None);
            result?;
            end?;
            let mut output = Image::solid(width, height, [0, 0, 0, 255]);
            bitmap.CopyPixels(std::ptr::null(), width * 4, &mut output.pixels)?;
            for p in output.pixels.as_chunks_mut::<4>().0 {
                p[3] = 255;
            }
            Ok(output)
        }
    }
}

/// Outlines of the shapes `coat` spares, `floor` being the first place above its zone.
fn holes(coat: &Paint, objects: &[Object], floor: usize) -> Vec<Vec<Point>> {
    coat.holes(objects, floor)
        .flat_map(|o| paths(o, objects))
        .collect()
}

/// One fill under the annotations: a rectangle, its ink and the outlines it spares.
pub(crate) type Underlay = (Rect, Ink, Vec<Vec<Point>>);

/// Direct2D implementation of the portable drawing surface, for one frame.
pub struct D2dCanvas<'a> {
    renderer: &'a Renderer,
    target: &'a ID2D1RenderTarget,
    // One brush recoloured per call: no allocation per primitive.
    brush: ID2D1SolidColorBrush,
    // The capture paints only shade rectangles and the background's coats before its
    // annotations. Other drawing before annotations opts out: unknown pixels must never
    // enter a reusable checkpoint.
    underlay: Option<Vec<Underlay>>,
}
impl<'a> D2dCanvas<'a> {
    pub fn new(renderer: &'a Renderer, target: &'a ID2D1RenderTarget) -> Result<Self> {
        let brush = unsafe { target.CreateSolidColorBrush(&color([0, 0, 0], 1.), None)? };
        Ok(Self {
            renderer,
            target,
            brush,
            underlay: Some(Vec::new()),
        })
    }
    /// A direct canvas recolouring `brush`: a surface painted every frame keeps one brush.
    pub fn with_brush(
        renderer: &'a Renderer,
        target: &'a ID2D1RenderTarget,
        brush: ID2D1SolidColorBrush,
    ) -> Self {
        Self {
            renderer,
            target,
            brush,
            underlay: None,
        }
    }
    /// A canvas that never records a checkpoint: for drawing under a transform, where the
    /// scene cache does not apply.
    pub fn direct(renderer: &'a Renderer, target: &'a ID2D1RenderTarget) -> Result<Self> {
        let mut canvas = Self::new(renderer, target)?;
        canvas.underlay = None;
        Ok(canvas)
    }
    fn ink(&self, ink: Ink) -> &ID2D1SolidColorBrush {
        unsafe { self.brush.SetColor(&color(ink.rgb, ink.alpha)) };
        &self.brush
    }
    /// The figure through `points`, closed when `closed`; None without points.
    fn figure(&self, points: &[Point], closed: bool) -> Result<Option<ID2D1PathGeometry>> {
        let Some((first, rest)) = points.split_first() else {
            return Ok(None);
        };
        unsafe {
            let geometry = self.renderer.factory.CreatePathGeometry()?;
            let sink = geometry.Open()?;
            // Winding, as the portable renderer fills: a ribbon that crosses itself on a sharp
            // turn stays filled instead of leaving a hole.
            sink.SetFillMode(D2D1_FILL_MODE_WINDING);
            sink.BeginFigure(vector(*first), D2D1_FIGURE_BEGIN_FILLED);
            for p in rest {
                sink.AddLine(vector(*p));
            }
            sink.EndFigure(if closed {
                D2D1_FIGURE_END_CLOSED
            } else {
                D2D1_FIGURE_END_OPEN
            });
            sink.Close()?;
            Ok(Some(geometry))
        }
    }
}
fn rounded(r: Rect, radius: f64) -> D2D1_ROUNDED_RECT {
    D2D1_ROUNDED_RECT {
        rect: rect(r),
        radiusX: radius as f32,
        radiusY: radius as f32,
    }
}
fn ellipse(center: Point, radius: Point) -> D2D1_ELLIPSE {
    D2D1_ELLIPSE {
        point: vector(center),
        radiusX: radius.x as f32,
        radiusY: radius.y as f32,
    }
}
impl Canvas for D2dCanvas<'_> {
    type Error = Error;
    fn size(&self) -> Point {
        let size = unsafe { self.target.GetSize() };
        Point {
            x: size.width as f64,
            y: size.height as f64,
        }
    }
    fn fill_rect(&mut self, r: Rect, ink: Ink) -> Result<()> {
        self.fill_rect_except(r, &[], ink)
    }
    fn fill_rect_except(&mut self, r: Rect, holes: &[Vec<Point>], ink: Ink) -> Result<()> {
        if let Some(underlay) = &mut self.underlay {
            underlay.push((r, ink, holes.to_vec()));
        }
        if holes.is_empty() {
            unsafe { self.target.FillRectangle(&rect(r), self.ink(ink)) };
            return Ok(());
        }
        let area = self.renderer.polygon(&r.corners())?;
        self.renderer
            .fill_except(self.target, area, holes, self.ink(ink))
    }
    fn stroke_rect(&mut self, r: Rect, ink: Ink, width: f64) -> Result<()> {
        self.underlay = None;
        unsafe {
            self.target
                .DrawRectangle(&rect(r), self.ink(ink), width as f32, None)
        };
        Ok(())
    }
    fn fill_round_rect(&mut self, r: Rect, radius: f64, ink: Ink) -> Result<()> {
        self.underlay = None;
        unsafe {
            self.target
                .FillRoundedRectangle(&rounded(r, radius), self.ink(ink))
        };
        Ok(())
    }
    fn stroke_round_rect(&mut self, r: Rect, radius: f64, ink: Ink, width: f64) -> Result<()> {
        self.underlay = None;
        unsafe {
            self.target
                .DrawRoundedRectangle(&rounded(r, radius), self.ink(ink), width as f32, None)
        };
        Ok(())
    }
    fn fill_ellipse(&mut self, center: Point, radius: Point, ink: Ink) -> Result<()> {
        self.underlay = None;
        unsafe {
            self.target
                .FillEllipse(&ellipse(center, radius), self.ink(ink))
        };
        Ok(())
    }
    fn stroke_ellipse(&mut self, center: Point, radius: Point, ink: Ink, width: f64) -> Result<()> {
        self.underlay = None;
        unsafe {
            self.target
                .DrawEllipse(&ellipse(center, radius), self.ink(ink), width as f32, None)
        };
        Ok(())
    }
    fn line(&mut self, a: Point, b: Point, ink: Ink, width: f64) -> Result<()> {
        self.underlay = None;
        unsafe {
            self.target
                .DrawLine(vector(a), vector(b), self.ink(ink), width as f32, None)
        };
        Ok(())
    }
    fn fill_polygon(&mut self, points: &[Point], ink: Ink) -> Result<()> {
        self.underlay = None;
        if let Some(geometry) = self.figure(points, true)? {
            unsafe { self.target.FillGeometry(&geometry, self.ink(ink), None) };
        }
        Ok(())
    }
    fn polyline(&mut self, points: &[Point], closed: bool, ink: Ink, width: f64) -> Result<()> {
        self.underlay = None;
        if let Some(geometry) = self.figure(points, closed)? {
            let style = &self.renderer.round_stroke;
            unsafe {
                self.target
                    .DrawGeometry(&geometry, self.ink(ink), width as f32, style)
            };
        }
        Ok(())
    }
    fn text(&mut self, text: &str, r: Rect, size: f64, ink: Ink) -> Result<()> {
        self.underlay = None;
        self.renderer
            .text_opacity(self.target, text, r, size as f32, (ink.rgb, ink.alpha))
    }
    fn annotation_text(&mut self, text: &str, r: Rect, size: f64, ink: Ink) -> Result<()> {
        self.underlay = None;
        self.renderer
            .annotation_text(self.target, text, r, size as f32, (ink.rgb, ink.alpha))
    }
    fn set_rotation(&mut self, rotation: Option<(Point, f64)>) -> Result<()> {
        self.underlay = None;
        let matrix = rotation.map_or_else(Matrix3x2::identity, |(center, radians)| {
            Matrix3x2::rotation_around(radians.to_degrees() as f32, vector(center))
        });
        unsafe { self.target.SetTransform(&matrix) };
        Ok(())
    }
    fn measure_text(&mut self, text: &str, size: f64) -> Result<Point> {
        self.renderer.measure_text(text, size as f32)
    }
    fn measure_annotation_text(&mut self, text: &str, size: f64) -> Result<Point> {
        self.renderer.measure_annotation_text(text, size as f32)
    }
    fn raster(
        &mut self,
        key: &'static str,
        version: u64,
        size: (u32, u32),
        pixels: &mut dyn FnMut() -> Vec<u8>,
        dst: Rect,
    ) -> Result<()> {
        self.underlay = None;
        let mut rasters = self.renderer.rasters.borrow_mut();
        let index = match rasters.iter().position(|(k, _, _)| *k == key) {
            Some(i) if rasters[i].1 == version => i,
            found => {
                let pixels = pixels();
                let bitmap = unsafe {
                    self.target.CreateBitmap(
                        D2D_SIZE_U {
                            width: size.0,
                            height: size.1,
                        },
                        Some(pixels.as_ptr().cast()),
                        size.0 * 4,
                        &D2D1_BITMAP_PROPERTIES {
                            pixelFormat: D2D1_PIXEL_FORMAT {
                                format: DXGI_FORMAT_B8G8R8A8_UNORM,
                                alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
                            },
                            dpiX: 96.,
                            dpiY: 96.,
                        },
                    )?
                };
                match found {
                    Some(i) => {
                        rasters[i] = (key, version, bitmap);
                        i
                    }
                    None => {
                        rasters.push((key, version, bitmap));
                        rasters.len() - 1
                    }
                }
            }
        };
        unsafe {
            self.target.DrawBitmap(
                &rasters[index].2,
                Some(&rect(dst)),
                1.,
                D2D1_BITMAP_INTERPOLATION_MODE_LINEAR,
                None,
            )
        };
        Ok(())
    }
    fn annotations_fading(
        &mut self,
        objects: &[Object],
        skip: Option<u64>,
        fading: &[u64],
    ) -> Result<()> {
        // One retention for the whole pile: a run drawn apart must not evict the others.
        self.renderer.strokes.borrow_mut().retain_present(objects);
        let parameters = layer(FADING);
        let mut start = 0;
        while start < objects.len() {
            let fades = fading.contains(&objects[start].id);
            let mut end = start + 1;
            while end < objects.len() && fading.contains(&objects[end].id) == fades {
                end += 1;
            }
            if fades {
                unsafe { self.target.PushLayer(&parameters, None) };
            }
            let result = self
                .renderer
                .annotation_range(self.target, objects, skip, start..end);
            if fades {
                unsafe { self.target.PopLayer() };
            }
            result?;
            start = end;
        }
        Ok(())
    }
    fn annotations(&mut self, objects: &[Object], skip: Option<u64>) -> Result<()> {
        if let Some(underlay) = self.underlay.take()
            && self.target.as_raw() == self.renderer.dc.as_raw()
            && let Ok(context) = self.target.cast::<ID2D1DeviceContext>()
        {
            let mut transform = Matrix3x2::identity();
            unsafe { self.target.GetTransform(&mut transform) };
            if transform == Matrix3x2::identity() {
                self.renderer.strokes.borrow_mut().retain_present(objects);
                return self.renderer.scene.borrow_mut().draw(
                    self.renderer,
                    self.target,
                    &context,
                    objects,
                    skip,
                    &underlay,
                );
            }
        }
        self.renderer.annotations_except(self.target, objects, skip)
    }
}

/// A layer that draws its content at `opacity`, everywhere.
fn layer(opacity: f32) -> D2D1_LAYER_PARAMETERS {
    D2D1_LAYER_PARAMETERS {
        contentBounds: D2D_RECT_F {
            left: f32::MIN,
            top: f32::MIN,
            right: f32::MAX,
            bottom: f32::MAX,
        },
        geometricMask: std::mem::ManuallyDrop::new(None),
        maskAntialiasMode: D2D1_ANTIALIAS_MODE_PER_PRIMITIVE,
        maskTransform: Matrix3x2::identity(),
        opacity,
        opacityBrush: std::mem::ManuallyDrop::new(None),
        layerOptions: D2D1_LAYER_OPTIONS_NONE,
    }
}

#[cfg(test)]
mod arrow_tests {
    use super::*;
    use shotoshot_ui::color_picker::ColorPicker;

    #[test]
    fn native_render_updates_hue_and_recovers_a_replaced_renderer() {
        let _com = crate::ComApartment::new().unwrap();
        let mut renderer = Renderer::new().unwrap();
        let mut picker = ColorPicker::new([255, 0, 0], Point::default(), 320, 350, 1.);
        let render = |renderer: &Renderer, picker: &ColorPicker| {
            crate::paint_image(320, 350, |hdc| {
                renderer.begin(hdc, 320, 350)?;
                picker.draw(&mut D2dCanvas::new(renderer, &renderer.dc)?)?;
                renderer.end()
            })
            .unwrap()
        };
        // Sample saturated, bright pixels away from the selection marker.
        let pixel = |image: &crate::Image| {
            let offset = (95 * image.width as usize + 175) * 4;
            <[u8; 3]>::try_from(&image.pixels[offset..offset + 3]).unwrap()
        };
        let before = pixel(&render(&renderer, &picker));
        assert!(before[2] > 200 && before[1] < 40 && before[0] < 40);
        // Hue ring centre in panel coordinates: (130, 144).
        let angle = std::f64::consts::TAU / 3.;
        picker.pointer_down(picker.at(130. + 91. * angle.cos(), 144. + 91. * angle.sin()));
        picker.pointer_up();
        let changed = pixel(&render(&renderer, &picker));
        assert!(changed[1] > 200 && changed[0] < 40 && changed[2] < 40);
        renderer = Renderer::new().unwrap();
        assert_eq!(pixel(&render(&renderer, &picker)), changed);
    }

    #[test]
    fn exported_arrow_has_a_filled_head_and_round_tail() {
        let _com = crate::ComApartment::new().unwrap();
        let renderer = Renderer::new().unwrap();
        let image = Image::solid(240, 100, [255; 4]);
        let object = Object {
            rotation: 0.,
            fill: Vec::new(),
            id: 1,
            style: shotoshot_core::Style {
                width: 10.,
                color: [255, 0, 0],
            },
            shape: Shape::Arrow {
                start: Point { x: 20., y: 50. },
                end: Point { x: 220., y: 50. },
                start_attach: None,
                end_attach: None,
                head: true,
            },
        };
        let output = renderer
            .export(
                &image,
                &[],
                &[object],
                Rect {
                    min: Point::default(),
                    max: Point { x: 240., y: 100. },
                },
            )
            .unwrap();
        let pixel = |x: usize, y: usize| &output.pixels[(y * 240 + x) * 4..(y * 240 + x) * 4 + 4];
        assert_eq!(
            pixel(188, 44),
            &[0, 0, 255, 255],
            "triangle interior must be red"
        );
        assert_eq!(pixel(188, 25), &[255; 4], "outside triangle stays empty");
        assert_eq!(
            pixel(17, 50),
            &[0, 0, 255, 255],
            "round tail extends behind its endpoint"
        );
    }
}
