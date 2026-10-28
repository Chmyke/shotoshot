//! ROLE: Renders a presentation layer into a reusable premultiplied native surface.
//! OWNER: Presentation owns the window and all drawing resources.
//! INVARIANTS: Passive layers never activate or consume mouse input.
//! HOTPATH: Drawing reuses brushes; no per-frame heap allocation.
use super::*;
use crate::{
    LiveSurface, Renderer,
    presentation::broadcast::{Broadcast, Slot},
    render::{color, rect, vector},
    resources::OwnedWindow,
};
use shotoshot_core::{Object, Rect, Shape};
use std::{cell::Cell, rc::Rc};
use windows::Win32::Graphics::{Direct2D::*, DirectWrite::*};

pub(super) struct Layer {
    window: OwnedWindow,
    pub width: i32,
    pub height: i32,
    surface: LiveSurface,
    renderer: Renderer,
    brush: ID2D1SolidColorBrush,
    label_format: IDWriteTextFormat,
    small_label_format: IDWriteTextFormat,
    outlines: diagram::Outlines,
    arrow_head: ID2D1PathGeometry,
    arrow_opacity: ID2D1Layer,
    stroke: ID2D1StrokeStyle,
    mirror: Option<(Rc<Broadcast>, Slot)>,
    // Layer-local extents drawn this frame and the previous one: the mirror uploads their union.
    drawn: Cell<Option<Rect>>,
    previous: Cell<Option<Rect>>,
}
impl Layer {
    pub fn new(
        owner: *const RefCell<Presentation>,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
    ) -> Result<Self> {
        Self::build(owner, x, y, width, height, diagram::Plan::FULL)
    }
    /// A layer sized for the input diagram of `plan`, `scale` device pixels per logical one.
    pub fn diagram(
        owner: *const RefCell<Presentation>,
        plan: diagram::Plan,
        scale: f64,
    ) -> Result<Self> {
        Self::build(
            owner,
            0,
            0,
            (plan.width() as f64 * scale) as i32,
            (diagram::HEIGHT as f64 * scale) as i32,
            plan,
        )
    }
    pub fn plan(&self) -> diagram::Plan {
        self.outlines.plan
    }
    fn build(
        owner: *const RefCell<Presentation>,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        plan: diagram::Plan,
    ) -> Result<Self> {
        // The diagram is drawn at the scale its window was sized for.
        let scale = width as f32 / plan.width() as f32;
        // PITFALL: The owner is boxed and outlives its windows; OwnedWindow clears the pointer.
        unsafe {
            let instance = GetModuleHandleW(None)?;
            if RegisterClassW(&WNDCLASSW {
                lpfnWndProc: Some(window_proc),
                hInstance: instance.into(),
                lpszClassName: w!("Shotoshot.Presentation"),
                hCursor: LoadCursorW(None, IDC_CROSS)?,
                ..Default::default()
            }) == 0
                && GetLastError() != ERROR_CLASS_ALREADY_EXISTS
            {
                return Err(Error::from_thread());
            }
            let window = OwnedWindow(CreateWindowExW(
                WS_EX_TOOLWINDOW
                    | WS_EX_LAYERED
                    | WS_EX_TRANSPARENT
                    | WS_EX_NOACTIVATE
                    | WS_EX_TOPMOST,
                w!("Shotoshot.Presentation"),
                w!("Shotoshot presentation"),
                WS_POPUP,
                x,
                y,
                width,
                height,
                None,
                None,
                Some(instance.into()),
                Some(owner.cast()),
            )?);
            let renderer = Renderer::with_transparency(true)?;
            let brush = renderer
                .dc
                .CreateSolidColorBrush(&color([255; 3], 1.), None)?;
            let write: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
            let language = shotoshot_text::language();
            let label_format =
                diagram::label_format(&write, diagram::LABEL_SIZE * scale, language)?;
            let small_label_format =
                diagram::label_format(&write, diagram::SMALL_LABEL_SIZE * scale, language)?;
            let outlines = diagram::Outlines::new(&renderer.dc.GetFactory()?, scale, plan)?;
            let arrow_head = renderer.dc.GetFactory()?.CreatePathGeometry()?;
            let sink = arrow_head.Open()?;
            sink.BeginFigure(
                vector(Point { x: 0., y: 0. }),
                windows::Win32::Graphics::Direct2D::Common::D2D1_FIGURE_BEGIN_FILLED,
            );
            sink.AddLine(vector(Point { x: -1., y: -0.45 }));
            sink.AddLine(vector(Point { x: -1., y: 0.45 }));
            sink.EndFigure(windows::Win32::Graphics::Direct2D::Common::D2D1_FIGURE_END_CLOSED);
            sink.Close()?;
            let stroke = renderer.dc.GetFactory()?.CreateStrokeStyle(
                &D2D1_STROKE_STYLE_PROPERTIES {
                    startCap: D2D1_CAP_STYLE_ROUND,
                    endCap: D2D1_CAP_STYLE_ROUND,
                    dashCap: D2D1_CAP_STYLE_ROUND,
                    lineJoin: D2D1_LINE_JOIN_ROUND,
                    miterLimit: 1.,
                    ..Default::default()
                },
                None,
            )?;
            let arrow_opacity = renderer.dc.CreateLayer(None)?;
            Ok(Self {
                window,
                width,
                height,
                surface: LiveSurface::new(width as u32, height as u32)?,
                renderer,
                brush,
                label_format,
                small_label_format,
                outlines,
                arrow_head,
                arrow_opacity,
                stroke,
                mirror: None,
                drawn: Cell::new(None),
                previous: Cell::new(None),
            })
        }
    }
    pub fn hwnd(&self) -> HWND {
        self.window.0
    }
    /// Copies every present, move and visibility change into the OBS mirror.
    pub fn mirrored(mut self, broadcast: Option<Rc<Broadcast>>, slot: Slot) -> Self {
        self.mirror = broadcast.map(|broadcast| (broadcast, slot));
        self
    }
    fn sync(&self, pixels: Option<(&[u8], RECT)>) {
        if let Some((broadcast, slot)) = &self.mirror {
            broadcast.sync(*slot, self.hwnd(), pixels);
        }
    }
    fn mark(&self, extent: Rect) {
        self.drawn.set(Some(
            self.drawn.get().map_or(extent, |drawn| drawn.union(extent)),
        ));
    }
    pub(super) fn mark_all(&self) {
        self.mark(Rect {
            min: Point { x: 0., y: 0. },
            max: Point {
                x: self.width as f64,
                y: self.height as f64,
            },
        });
    }
    /// Pixels that may differ from the last present: what was drawn now or erased since.
    fn changed(&self) -> RECT {
        let drawn = self.drawn.take();
        let Some(extent) = (match (drawn, self.previous.replace(drawn)) {
            (Some(now), Some(before)) => Some(now.union(before)),
            (now, before) => now.or(before),
        }) else {
            return RECT::default();
        };
        RECT {
            left: (extent.min.x.floor() as i32).clamp(0, self.width),
            top: (extent.min.y.floor() as i32).clamp(0, self.height),
            right: (extent.max.x.ceil() as i32).clamp(0, self.width),
            bottom: (extent.max.y.ceil() as i32).clamp(0, self.height),
        }
    }
    pub fn detach(&self) {
        unsafe {
            SetWindowLongPtrW(self.hwnd(), GWLP_USERDATA, 0);
        }
    }
    pub fn intercept(&self, active: bool) -> Result<()> {
        // PITFALL: Layered + WS_EX_TRANSPARENT passes input across threads, not just siblings.
        unsafe {
            let mut style = GetWindowLongPtrW(self.hwnd(), GWL_EXSTYLE) as u32;
            if active {
                style &= !(WS_EX_TRANSPARENT | WS_EX_NOACTIVATE).0;
            } else {
                style |= (WS_EX_TRANSPARENT | WS_EX_NOACTIVATE).0;
            }
            SetWindowLongPtrW(self.hwnd(), GWL_EXSTYLE, style as isize);
            SetWindowPos(
                self.hwnd(),
                None,
                0,
                0,
                0,
                0,
                SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
            )
        }
    }
    pub fn mouse_interactive(&self, active: bool) -> Result<()> {
        unsafe {
            let mut style = GetWindowLongPtrW(self.hwnd(), GWL_EXSTYLE) as u32;
            style |= WS_EX_NOACTIVATE.0;
            if active {
                style &= !WS_EX_TRANSPARENT.0;
            } else {
                style |= WS_EX_TRANSPARENT.0;
            }
            SetWindowLongPtrW(self.hwnd(), GWL_EXSTYLE, style as isize);
            SetWindowPos(
                self.hwnd(),
                None,
                0,
                0,
                0,
                0,
                SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
            )
        }
    }
    pub fn lock_icon(&self, locked: bool) {
        self.mark_all();
        let scale = self.width as f32 / 24.;
        unsafe {
            self.brush.SetColor(&color([32, 32, 35], 0.72));
            self.renderer.dc.FillRoundedRectangle(
                &D2D1_ROUNDED_RECT {
                    rect: windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F {
                        left: scale,
                        top: scale,
                        right: self.width as f32 - scale,
                        bottom: self.height as f32 - scale,
                    },
                    radiusX: 2.8 * scale,
                    radiusY: 2.8 * scale,
                },
                &self.brush,
            );
            self.brush.SetColor(&color([110, 110, 118], 0.72));
            self.renderer.dc.DrawRoundedRectangle(
                &D2D1_ROUNDED_RECT {
                    rect: windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F {
                        left: scale,
                        top: scale,
                        right: self.width as f32 - scale,
                        bottom: self.height as f32 - scale,
                    },
                    radiusX: 2.8 * scale,
                    radiusY: 2.8 * scale,
                },
                &self.brush,
                1.4 * scale,
                None,
            );
            self.brush.SetColor(&color(
                if locked { [113, 195, 255] } else { [225; 3] },
                0.92,
            ));
            let rect =
                |left, top, right, bottom| windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F {
                    left: left * scale,
                    top: top * scale,
                    right: right * scale,
                    bottom: bottom * scale,
                };
            self.renderer.dc.DrawRoundedRectangle(
                &D2D1_ROUNDED_RECT {
                    rect: rect(
                        if locked { 8. } else { 11. },
                        4.,
                        if locked { 16. } else { 19. },
                        15.,
                    ),
                    radiusX: 4. * scale,
                    radiusY: 4. * scale,
                },
                &self.brush,
                2. * scale,
                None,
            );
            self.renderer.dc.FillRoundedRectangle(
                &D2D1_ROUNDED_RECT {
                    rect: rect(6., 11., 18., 21.),
                    radiusX: 2. * scale,
                    radiusY: 2. * scale,
                },
                &self.brush,
            );
        }
    }
    #[cfg(test)]
    pub fn intercepts(&self) -> bool {
        unsafe { GetWindowLongPtrW(self.hwnd(), GWL_EXSTYLE) as u32 & WS_EX_TRANSPARENT.0 == 0 }
    }
    pub fn position(&self, x: i32, y: i32) -> Result<()> {
        unsafe {
            SetWindowPos(
                self.hwnd(),
                Some(HWND_TOPMOST),
                x,
                y,
                0,
                0,
                SWP_NOSIZE | SWP_NOACTIVATE,
            )?;
        }
        self.sync(None);
        Ok(())
    }
    pub fn show(&self, show: bool) {
        unsafe {
            let _ = ShowWindow(self.hwnd(), if show { SW_SHOWNOACTIVATE } else { SW_HIDE });
        }
        self.sync(None);
    }
    pub fn invalidate(&self) {
        unsafe {
            let _ = InvalidateRect(Some(self.hwnd()), None, false);
        }
    }
    pub fn begin(&self, intercept: bool) -> Result<()> {
        unsafe {
            self.renderer.dc.BindDC(
                self.surface.dc(),
                &RECT {
                    right: self.width,
                    bottom: self.height,
                    ..Default::default()
                },
            )?;
            self.renderer.dc.BeginDraw();
            self.renderer
                .dc
                .Clear(Some(&color([0; 3], if intercept { 1. / 255. } else { 0. })));
        }
        Ok(())
    }
    /// Starts a frame that redraws only `dirty`: the rest of the surface keeps its pixels.
    pub fn begin_region(&self, dirty: RECT) -> Result<()> {
        unsafe {
            self.renderer.dc.BindDC(
                self.surface.dc(),
                &RECT {
                    right: self.width,
                    bottom: self.height,
                    ..Default::default()
                },
            )?;
            self.renderer.dc.BeginDraw();
            self.renderer.dc.PushAxisAlignedClip(
                &windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F {
                    left: dirty.left as f32,
                    top: dirty.top as f32,
                    right: dirty.right as f32,
                    bottom: dirty.bottom as f32,
                },
                D2D1_ANTIALIAS_MODE_ALIASED,
            );
            // Clear respects the clip: only the dirty area becomes transparent.
            self.renderer.dc.Clear(Some(&color([0; 3], 0.)));
        }
        Ok(())
    }
    /// Ends a `begin_region` frame and sends that area alone, to the screen and the mirror.
    pub fn end_region(&self, dirty: RECT) -> Result<()> {
        unsafe {
            self.renderer.dc.PopAxisAlignedClip();
            self.renderer.dc.EndDraw(None, None)?;
        }
        self.surface.present_region(self.hwnd(), dirty)?;
        self.sync(Some((self.surface.bits(), dirty)));
        Ok(())
    }
    pub fn object(&self, object: &Object, opacity: f32, arrow_size: f64) {
        // Round caps, joins and the arrowhead stay within the stroke width plus the head size.
        self.mark(
            object
                .bounds()
                .expanded(object.style.width + arrow_size + 2.),
        );
        unsafe {
            self.brush.SetColor(&color(object.style.color, opacity));
            let width = object.style.width as f32;
            match object.shape {
                Shape::Rectangle(r) => {
                    let radius = (width * 1.6).min(r.size().x.min(r.size().y) as f32 * 0.5);
                    self.renderer.dc.DrawRoundedRectangle(
                        &D2D1_ROUNDED_RECT {
                            rect: rect(r),
                            radiusX: radius,
                            radiusY: radius,
                        },
                        &self.brush,
                        width,
                        Some(&self.stroke),
                    )
                }
                Shape::Ellipse(r) => self.renderer.dc.DrawEllipse(
                    &D2D1_ELLIPSE {
                        point: vector(r.center()),
                        radiusX: r.size().x as f32 * 0.5,
                        radiusY: r.size().y as f32 * 0.5,
                    },
                    &self.brush,
                    width,
                    None,
                ),
                Shape::Arrow { start, end, .. } => {
                    let delta = end - start;
                    let length = delta.length();
                    if length < 0.001 {
                        return;
                    }
                    if opacity < 1. {
                        // Fade the assembled arrow once: antialiased edges can overlap
                        // even when the shaft ends exactly at the triangle's flat base.
                        self.brush.SetColor(&color(object.style.color, 1.));
                        self.renderer.dc.PushLayer(
                            &D2D1_LAYER_PARAMETERS {
                                contentBounds: rect(
                                    object
                                        .bounds()
                                        .expanded(object.style.width + arrow_size + 2.),
                                ),
                                maskTransform: windows_numerics::Matrix3x2::identity(),
                                opacity,
                                ..Default::default()
                            },
                            &self.arrow_opacity,
                        );
                    }
                    let unit = delta * (1. / length);
                    let size = arrow_size * 0.68;
                    if let Some(shaft_end) =
                        shotoshot_core::arrow_shaft_end(start, end, object.style.width, size)
                    {
                        self.renderer.dc.DrawLine(
                            vector(start),
                            vector(shaft_end),
                            &self.brush,
                            width,
                            Some(&self.renderer.arrow_stroke),
                        );
                    }
                    // Reuse a unit triangle; only its transform changes per arrow.
                    self.renderer.dc.SetTransform(&windows_numerics::Matrix3x2 {
                        M11: (unit.x * size) as f32,
                        M12: (unit.y * size) as f32,
                        M21: (-unit.y * size) as f32,
                        M22: (unit.x * size) as f32,
                        M31: end.x as f32,
                        M32: end.y as f32,
                    });
                    self.renderer
                        .dc
                        .FillGeometry(&self.arrow_head, &self.brush, None);
                    self.renderer
                        .dc
                        .SetTransform(&windows_numerics::Matrix3x2::identity());
                    if opacity < 1. {
                        self.renderer.dc.PopLayer();
                    }
                }
                _ => {}
            }
        }
    }
    pub fn halo(&self, rgb: [u8; 3], pressed: bool, pulse: Option<f32>) {
        self.mark_all();
        unsafe {
            let radius = self.width as f32 / 3.;
            let ellipse = D2D1_ELLIPSE {
                point: vector(Point {
                    x: self.width as f64 * 0.5,
                    y: self.height as f64 * 0.5,
                }),
                radiusX: radius,
                radiusY: radius,
            };
            self.brush.SetColor(&color(
                if pressed { [237, 79, 92] } else { rgb },
                if pressed { 0.24 } else { 0.14 },
            ));
            self.renderer.dc.FillEllipse(&ellipse, &self.brush);
            self.brush
                .SetColor(&color(if pressed { [237, 79, 92] } else { rgb }, 0.55));
            self.renderer
                .dc
                .DrawEllipse(&ellipse, &self.brush, 2., None);
            if let Some(progress) = pulse {
                let radius = radius * (1. + progress * 0.4);
                self.brush.SetColor(&color(
                    if pressed { [237, 79, 92] } else { rgb },
                    (1. - progress) * 0.5,
                ));
                self.renderer.dc.DrawEllipse(
                    &D2D1_ELLIPSE {
                        point: ellipse.point,
                        radiusX: radius,
                        radiusY: radius,
                    },
                    &self.brush,
                    2.,
                    None,
                );
            }
        }
    }
    /// The pointer effects of one frame; `origin` is this layer's screen position.
    pub fn pointer(
        &self,
        origin: Point,
        effects: &shotoshot_core::pointer::Effects,
        settings: &shotoshot_core::pointer::PointerSettings,
        halo: shotoshot_ui::pointer::HaloState,
        scale: f64,
        now: u64,
    ) -> Result<()> {
        let mut canvas =
            crate::D2dCanvas::with_brush(&self.renderer, &self.renderer.dc, self.brush.clone());
        shotoshot_ui::pointer::paint(&mut canvas, origin, effects, settings, halo, scale, now)
    }
    pub fn devices(&self, devices: &Devices, now: u64) {
        self.mark_all();
        diagram::draw(
            &self.renderer.dc,
            &self.brush,
            (&self.label_format, &self.small_label_format),
            &self.outlines,
            devices,
            now,
        );
    }
    pub fn end(&self) -> Result<()> {
        unsafe {
            self.renderer.dc.EndDraw(None, None)?;
        }
        self.surface.present(self.hwnd())?;
        self.sync(Some((self.surface.bits(), self.changed())));
        Ok(())
    }
    pub fn composite(&self, target: HDC, x: i32, y: i32) -> Result<()> {
        unsafe {
            GdiAlphaBlend(
                target,
                x,
                y,
                self.width,
                self.height,
                self.surface.dc(),
                0,
                0,
                self.width,
                self.height,
                BLENDFUNCTION {
                    BlendOp: AC_SRC_OVER as u8,
                    BlendFlags: 0,
                    SourceConstantAlpha: 255,
                    AlphaFormat: AC_SRC_ALPHA as u8,
                },
            )
            .ok()
        }
    }
    #[cfg(test)]
    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        self.surface.pixel(x, y)
    }
}
impl Drop for Layer {
    fn drop(&mut self) {
        if let Some((broadcast, slot)) = &self.mirror {
            broadcast.remove(*slot, self.hwnd());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shotoshot_core::Style;

    #[test]
    fn mirror_region_covers_drawn_shapes_then_what_they_leave_behind() {
        let _com = crate::ComApartment::new().unwrap();
        let layer = Layer::new(std::ptr::null(), -32000, -32000, 400, 300).unwrap();
        let shape = Object {
            rotation: 0.,
            fill: Vec::new(),
            id: 0,
            style: Style::default(),
            shape: Shape::Rectangle(Rect {
                min: Point { x: 100., y: 80. },
                max: Point { x: 150., y: 120. },
            }),
        };
        let margin = shape.style.width + 10. + 2.;
        layer.begin(false).unwrap();
        layer.object(&shape, 1., 10.);
        layer.end().unwrap();
        let drawn = layer.previous.get().unwrap();
        assert_eq!(drawn, shape.bounds().expanded(margin));

        // Nothing drawn: the old extent must still be uploaded once to erase it.
        assert_eq!(
            layer.changed(),
            RECT {
                left: drawn.min.x.floor() as i32,
                top: drawn.min.y.floor() as i32,
                right: drawn.max.x.ceil() as i32,
                bottom: drawn.max.y.ceil() as i32,
            }
        );
        assert_eq!(layer.changed(), RECT::default());

        // Full-layer drawings cover exactly the surface.
        layer.begin(false).unwrap();
        layer.halo([255; 3], false, None);
        unsafe { layer.renderer.dc.EndDraw(None, None) }.unwrap();
        assert_eq!(
            layer.changed(),
            RECT {
                right: 400,
                bottom: 300,
                ..Default::default()
            }
        );
    }
}
