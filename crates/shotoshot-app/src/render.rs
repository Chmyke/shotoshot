//! ROLE: Shared Direct2D drawing for the overlay and flattened output.
//! OWNER: Each capture owns one renderer; no rendering exists while resident.
use crate::{Image, ImageExt};
use shotoshot_core::{Object, Point, Rect, Shape, paths};
use shotoshot_ui::{Canvas, Ink};
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

mod scene;
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
    pub fn measure_text(&self, text: &str, size: f32) -> Result<Point> {
        unsafe {
            let format = self.write.CreateTextFormat(
                w!("Segoe UI"),
                None,
                DWRITE_FONT_WEIGHT_NORMAL,
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                size,
                w!("en-US"),
            )?;
            format.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)?;
            let layout = self.write.CreateTextLayout(
                &text.encode_utf16().collect::<Vec<_>>(),
                &format,
                1_000_000.,
                1_000_000.,
            )?;
            let mut metrics = DWRITE_TEXT_METRICS::default();
            layout.GetMetrics(&mut metrics)?;
            Ok(Point {
                x: metrics.widthIncludingTrailingWhitespace as f64,
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
        unsafe {
            let format = self.write.CreateTextFormat(
                w!("Segoe UI"),
                None,
                DWRITE_FONT_WEIGHT_NORMAL,
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                size,
                w!("en-US"),
            )?;
            let brush = target.CreateSolidColorBrush(&color(ink.0, ink.1), None)?;
            target.DrawText(
                &text.encode_utf16().collect::<Vec<_>>(),
                &format,
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
    fn annotation_range(
        &self,
        target: &ID2D1RenderTarget,
        objects: &[Object],
        skip: Option<u64>,
        range: std::ops::Range<usize>,
    ) -> Result<()> {
        let mut strokes = self.strokes.borrow_mut();
        unsafe {
            for o in &objects[range] {
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
                    let result = self.text(
                        target,
                        text,
                        Rect {
                            min: *position,
                            max: *position + *extent + Point { x: 20., y: 20. },
                        },
                        *size as f32,
                        o.style.color,
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
                        let filled_head =
                            matches!(o.shape, Shape::Arrow { .. }) && path.first() == path.last();
                        sink.BeginFigure(
                            vector(*first),
                            if filled_head {
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
            if let Some(bitmap) = &self.backdrop {
                self.dc.DrawBitmap(
                    bitmap,
                    None,
                    1.,
                    D2D1_BITMAP_INTERPOLATION_MODE_NEAREST_NEIGHBOR,
                    None,
                );
            }
            Ok(())
        }
    }
    pub fn end(&self) -> Result<()> {
        unsafe { self.dc.EndDraw(None, None) }
    }
    pub fn export(&self, image: &Image, objects: &[Object], crop: Rect) -> Result<Image> {
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
            let source = self.bitmap(&target, image)?;
            target.BeginDraw();
            target.SetTransform(&Matrix3x2::translation(-(left as f32), -(top as f32)));
            target.DrawBitmap(
                &source,
                None,
                1.,
                D2D1_BITMAP_INTERPOLATION_MODE_NEAREST_NEIGHBOR,
                None,
            );
            let result = self.annotations(&target, objects);
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

/// Direct2D implementation of the portable drawing surface, for one frame.
pub struct D2dCanvas<'a> {
    renderer: &'a Renderer,
    target: &'a ID2D1RenderTarget,
    // One brush recoloured per call: no allocation per primitive.
    brush: ID2D1SolidColorBrush,
    // The capture paints only shade rectangles before its annotations. Other drawing
    // before annotations opts out: unknown pixels must never enter a reusable checkpoint.
    underlay: Option<Vec<(Rect, Ink)>>,
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
    fn ink(&self, ink: Ink) -> &ID2D1SolidColorBrush {
        unsafe { self.brush.SetColor(&color(ink.rgb, ink.alpha)) };
        &self.brush
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
        if let Some(underlay) = &mut self.underlay {
            underlay.push((r, ink));
        }
        unsafe { self.target.FillRectangle(&rect(r), self.ink(ink)) };
        Ok(())
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
    fn text(&mut self, text: &str, r: Rect, size: f64, ink: Ink) -> Result<()> {
        self.underlay = None;
        self.renderer
            .text_opacity(self.target, text, r, size as f32, (ink.rgb, ink.alpha))
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

#[cfg(test)]
mod arrow_tests {
    use super::*;
    use shotoshot_ui::color_picker::ColorPicker;

    #[test]
    fn native_render_updates_hue_and_recovers_a_replaced_renderer() {
        let _com = crate::ComApartment::new().unwrap();
        let mut renderer = Renderer::new().unwrap();
        let mut picker = ColorPicker::new([255, 0, 0], Point::default(), 320, 350);
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
