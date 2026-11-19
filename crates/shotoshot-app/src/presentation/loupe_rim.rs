//! ROLE: The bent rim of the composed loupe (`shotoshot_core::loupe::Lens`): between the flat
//! plateau DWM draws in exact blocks and the edge, the enlargement falls back smoothly to the
//! screen. DWM thumbnails only enlarge evenly, so the rim is drawn by a pixel shader from
//! Windows.Graphics.Capture frames of the windows under the loupe.
//! OWNER: `loupe::Composed`, which gives it a visual under the plateau and keeps it while the
//! loupe lives.
//! INVARIANTS: The loupe itself is never captured (one session per window, never the loupe's).
//! A capture lives only while its window is under the loupe: a session kept for a window that
//! left would copy every frame it paints for nothing. Frames are copied and the rim drawn on the resident's thread: the
//! capture threads only post `FRAMES`. Inside the plateau only three pixels under its edge are
//! drawn, as its exact blocks; beyond the edge nothing at all. A window the rim cannot capture is left out
//! of it, said once.
use super::loupe_capture::{announced, window_origin};
use super::*;
use shotoshot_core::loupe::{Frame, Lens, PixelRect};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use windows::{
    Graphics::{
        Capture::{Direct3D11CaptureFramePool, GraphicsCaptureItem, GraphicsCaptureSession},
        DirectX::{Direct3D11::IDirect3DDevice, DirectXPixelFormat},
        SizeInt32,
    },
    Win32::{
        Graphics::{
            Direct3D::{D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST, Fxc::D3DCompile, ID3DBlob},
            Direct3D11::*,
            DirectComposition::*,
            Dxgi::Common::*,
            Dxgi::IDXGIDevice,
        },
        System::WinRT::{
            Direct3D11::{CreateDirect3D11DeviceFromDXGIDevice, IDirect3DDxgiInterfaceAccess},
            Graphics::Capture::IGraphicsCaptureItemInterop,
        },
    },
};

/// The rim's shaders: a triangle over the whole surface, then, per window, every rim pixel as
/// the average of 4 x 4 points across it, each showing the screen point `Lens::source` gives,
/// read from that window's frame. Points beyond the edge stay clear: the screen shows there.
const SHADER: &str = r"
cbuffer Rim : register(b0) {
    float2 offset; float radius; float plateau;
    float4 cubic;
    float2 plateauCentre; float2 edgeShift;
    float2 origin; float2 size;
    float zoom; float clear; float2 unused;
};
Texture2D shown : register(t0);
SamplerState smooth : register(s0);

float4 vs(uint id : SV_VertexID) : SV_Position {
    float2 at = float2((id << 1) & 2, id & 2);
    return float4(at * float2(2, -2) + float2(-1, 1), 0, 1);
}

float2 source(float2 p) {
    float2 d = p - radius;
    float r = max(length(d), 0.0001);
    float t = saturate((r - plateau) / (radius - plateau));
    float far = cubic.x + t * (cubic.y + t * (cubic.z + t * cubic.w));
    if (r <= plateau) far = r / zoom;
    return plateauCentre + d * (far / r) + edgeShift * (t * t * (3 - 2 * t));
}

float4 ps(float4 position : SV_Position) : SV_Target {
    if (clear > 0.5) return 0;
    float2 pixel = floor(position.xy - offset);
    float centre = length(pixel + 0.5 - radius);
    if (centre < plateau - 3 || centre > radius + 1) discard;
    // Under the plateau's edge, its exact block: where DWM's round clip lets a little through,
    // the same pixel shows, not a blend.
    if (centre < plateau) {
        int2 texel = int2(floor(source(pixel + 0.5) - origin));
        if (any(texel < 0) || any(texel >= int2(size))) discard;
        return shown.Load(int3(texel, 0));
    }
    float4 sum = 0;
    [unroll] for (int i = 0; i < 4; i++) {
        [unroll] for (int j = 0; j < 4; j++) {
            float2 p = pixel + (float2(i, j) + 0.5) / 4;
            if (length(p - radius) > radius) continue;
            float2 uv = (source(p) - origin) / size;
            if (all(uv >= 0) && all(uv <= 1)) sum += shown.SampleLevel(smooth, uv, 0);
        }
    }
    return sum / 16;
}
";

/// The constants of one pass, as `cbuffer Rim` lays them out.
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Constants {
    offset: [f32; 2],
    radius: f32,
    plateau: f32,
    cubic: [f32; 4],
    plateau_centre: [f32; 2],
    edge_shift: [f32; 2],
    origin: [f32; 2],
    size: [f32; 2],
    zoom: f32,
    clear: f32,
    unused: [f32; 2],
}

/// Vertex then pixel shader bytecode, or why the compiler refused them.
type Bytecode = std::result::Result<(Vec<u8>, Vec<u8>), String>;
/// The shaders' bytecode, compiled once per process.
fn bytecode() -> Result<&'static (Vec<u8>, Vec<u8>)> {
    static CODE: OnceLock<Bytecode> = OnceLock::new();
    CODE.get_or_init(|| Ok((compile("vs", "vs_4_0")?, compile("ps", "ps_4_0")?)))
        .as_ref()
        .map_err(|message| Error::new(E_FAIL, message.clone()))
}
fn compile(entry: &str, target: &str) -> std::result::Result<Vec<u8>, String> {
    let entry = std::ffi::CString::new(entry).unwrap_or_default();
    let target = std::ffi::CString::new(target).unwrap_or_default();
    let mut code: Option<ID3DBlob> = None;
    let mut errors: Option<ID3DBlob> = None;
    let compiled = unsafe {
        D3DCompile(
            SHADER.as_ptr().cast(),
            SHADER.len(),
            s!("loupe_rim"),
            None,
            None,
            PCSTR(entry.as_ptr().cast()),
            PCSTR(target.as_ptr().cast()),
            0,
            0,
            &mut code,
            Some(&mut errors),
        )
    };
    let bytes = |blob: &ID3DBlob| unsafe {
        std::slice::from_raw_parts(blob.GetBufferPointer().cast::<u8>(), blob.GetBufferSize())
            .to_vec()
    };
    match (compiled, code) {
        (Ok(()), Some(code)) => Ok(bytes(&code)),
        (result, _) => Err(format!(
            "rim shader: {:?} {}",
            result.err(),
            errors
                .map(|e| String::from_utf8_lossy(&bytes(&e)).into_owned())
                .unwrap_or_default()
        )),
    }
}

/// One window captured for the rim, with its latest frame.
struct Source {
    window: HWND,
    _item: GraphicsCaptureItem,
    pool: Direct3D11CaptureFramePool,
    session: GraphicsCaptureSession,
    arrived: i64,
    size: SizeInt32,
    /// A copy of the latest frame the shader reads, once one came.
    frame: Option<(ID3D11Texture2D, ID3D11ShaderResourceView, SizeInt32)>,
}
impl Source {
    fn new(window: HWND, winrt: &IDirect3DDevice, notify: HWND) -> Result<Self> {
        unsafe {
            let item: GraphicsCaptureItem =
                factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>()?
                    .CreateForWindow(window)?;
            let size = item.Size()?;
            let pool = Direct3D11CaptureFramePool::CreateFreeThreaded(
                winrt,
                DirectXPixelFormat::B8G8R8A8UIntNormalized,
                2,
                size,
            )?;
            let arrived = announced(&pool, notify)?;
            let session = pool.CreateCaptureSession(&item)?;
            let _ = session.SetIsCursorCaptureEnabled(false);
            let _ = session.SetIsBorderRequired(false);
            session.StartCapture()?;
            Ok(Self {
                window,
                _item: item,
                pool,
                session,
                arrived,
                size,
                frame: None,
            })
        }
    }
    /// Copies the latest frame, if any came; whether one did.
    fn refresh(
        &mut self,
        d3d: &ID3D11Device,
        context: &ID3D11DeviceContext,
        winrt: &IDirect3DDevice,
    ) -> Result<bool> {
        let mut latest = None;
        while let Ok(frame) = self.pool.TryGetNextFrame() {
            if let Some(older) = latest.replace(frame) {
                let _ = older.Close();
            }
        }
        let Some(frame) = latest else {
            return Ok(false);
        };
        let copied = (|| unsafe {
            let size = frame.ContentSize()?;
            if size.Width <= 0 || size.Height <= 0 {
                return Ok(false);
            }
            if size != self.size {
                self.pool
                    .Recreate(winrt, DirectXPixelFormat::B8G8R8A8UIntNormalized, 2, size)?;
                self.size = size;
            }
            let source: ID3D11Texture2D = frame
                .Surface()?
                .cast::<IDirect3DDxgiInterfaceAccess>()?
                .GetInterface()?;
            if self.frame.as_ref().is_none_or(|(_, _, held)| *held != size) {
                let desc = D3D11_TEXTURE2D_DESC {
                    Width: size.Width as u32,
                    Height: size.Height as u32,
                    MipLevels: 1,
                    ArraySize: 1,
                    Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                    SampleDesc: DXGI_SAMPLE_DESC {
                        Count: 1,
                        Quality: 0,
                    },
                    Usage: D3D11_USAGE_DEFAULT,
                    BindFlags: D3D11_BIND_SHADER_RESOURCE.0 as u32,
                    ..Default::default()
                };
                let mut texture = None;
                d3d.CreateTexture2D(&desc, None, Some(&mut texture))?;
                let texture = texture.ok_or_else(|| Error::from_hresult(E_POINTER))?;
                let mut view = None;
                d3d.CreateShaderResourceView(&texture, None, Some(&mut view))?;
                let view = view.ok_or_else(|| Error::from_hresult(E_POINTER))?;
                self.frame = Some((texture, view, size));
            }
            if let Some((texture, _, _)) = &self.frame {
                // The frame may be larger than its content: only the content is copied.
                context.CopySubresourceRegion(
                    texture,
                    0,
                    0,
                    0,
                    0,
                    &source,
                    0,
                    Some(&D3D11_BOX {
                        left: 0,
                        top: 0,
                        front: 0,
                        right: size.Width as u32,
                        bottom: size.Height as u32,
                        back: 1,
                    }),
                );
            }
            Ok(true)
        })();
        let _ = frame.Close();
        copied
    }
}
impl Drop for Source {
    fn drop(&mut self) {
        let _ = self.pool.RemoveFrameArrived(self.arrived);
        let _ = self.session.Close();
        let _ = self.pool.Close();
    }
}

/// A window the rim leaves out: said once.
fn left_out(error: &Error) {
    static SAID: AtomicBool = AtomicBool::new(false);
    if !SAID.swap(true, Ordering::SeqCst) {
        eprintln!("Loupe rim: a window left out ({error})");
    }
}

pub(super) struct Rim {
    d3d: ID3D11Device,
    context: ID3D11DeviceContext,
    winrt: IDirect3DDevice,
    vertex: ID3D11VertexShader,
    pixel: ID3D11PixelShader,
    sampler: ID3D11SamplerState,
    over: ID3D11BlendState,
    constants: ID3D11Buffer,
    composition: IDCompositionDesktopDevice,
    surface: Option<IDCompositionSurface>,
    /// The rim's visual, to lay under the plateau.
    pub visual: IDCompositionVisual2,
    /// Windows under the loupe, bottom first.
    sources: Vec<Source>,
    /// Windows that refused capture while under the loupe.
    refused: Vec<HWND>,
    notify: HWND,
    side: i32,
    /// What the rim last showed: the frame and the windows under it, bottom first.
    drawn: Option<(Frame, Vec<(HWND, PixelRect)>)>,
}
impl Rim {
    pub fn new(
        d3d: &ID3D11Device,
        context: &ID3D11DeviceContext,
        composition: &IDCompositionDesktopDevice,
        notify: HWND,
    ) -> Result<Self> {
        let (vs, ps) = bytecode()?;
        unsafe {
            let winrt: IDirect3DDevice =
                CreateDirect3D11DeviceFromDXGIDevice(&d3d.cast::<IDXGIDevice>()?)?.cast()?;
            let mut vertex = None;
            d3d.CreateVertexShader(vs, None, Some(&mut vertex))?;
            let mut pixel = None;
            d3d.CreatePixelShader(ps, None, Some(&mut pixel))?;
            let mut sampler = None;
            d3d.CreateSamplerState(
                &D3D11_SAMPLER_DESC {
                    Filter: D3D11_FILTER_MIN_MAG_MIP_LINEAR,
                    AddressU: D3D11_TEXTURE_ADDRESS_CLAMP,
                    AddressV: D3D11_TEXTURE_ADDRESS_CLAMP,
                    AddressW: D3D11_TEXTURE_ADDRESS_CLAMP,
                    MaxLOD: f32::MAX,
                    ..Default::default()
                },
                Some(&mut sampler),
            )?;
            // Premultiplied, each window over those under it.
            let mut blend = D3D11_BLEND_DESC::default();
            blend.RenderTarget[0] = D3D11_RENDER_TARGET_BLEND_DESC {
                BlendEnable: true.into(),
                SrcBlend: D3D11_BLEND_ONE,
                DestBlend: D3D11_BLEND_INV_SRC_ALPHA,
                BlendOp: D3D11_BLEND_OP_ADD,
                SrcBlendAlpha: D3D11_BLEND_ONE,
                DestBlendAlpha: D3D11_BLEND_INV_SRC_ALPHA,
                BlendOpAlpha: D3D11_BLEND_OP_ADD,
                RenderTargetWriteMask: D3D11_COLOR_WRITE_ENABLE_ALL.0 as u8,
            };
            let mut over = None;
            d3d.CreateBlendState(&blend, Some(&mut over))?;
            let mut constants = None;
            d3d.CreateBuffer(
                &D3D11_BUFFER_DESC {
                    ByteWidth: std::mem::size_of::<Constants>() as u32,
                    Usage: D3D11_USAGE_DEFAULT,
                    BindFlags: D3D11_BIND_CONSTANT_BUFFER.0 as u32,
                    ..Default::default()
                },
                None,
                Some(&mut constants),
            )?;
            let visual = composition.CreateVisual()?;
            let missing = || Error::from_hresult(E_POINTER);
            Ok(Self {
                d3d: d3d.clone(),
                context: context.clone(),
                winrt,
                vertex: vertex.ok_or_else(missing)?,
                pixel: pixel.ok_or_else(missing)?,
                sampler: sampler.ok_or_else(missing)?,
                over: over.ok_or_else(missing)?,
                constants: constants.ok_or_else(missing)?,
                composition: composition.clone(),
                surface: None,
                visual,
                sources: Vec::new(),
                refused: Vec::new(),
                notify,
                side: 0,
                drawn: None,
            })
        }
    }

    /// Shows, in the rim of the loupe of `frame`, the windows `under` its window, top first.
    pub fn place(&mut self, frame: &Frame, under: &[(HWND, PixelRect)]) -> Result<()> {
        let bottom_first: Vec<_> = under.iter().rev().copied().collect();
        self.refused
            .retain(|window| bottom_first.iter().any(|(w, _)| w == window));
        let mut kept = std::mem::take(&mut self.sources);
        for (window, _) in &bottom_first {
            if let Some(at) = kept.iter().position(|s| s.window == *window) {
                self.sources.push(kept.swap_remove(at));
            } else if !self.refused.contains(window) {
                match Source::new(*window, &self.winrt, self.notify) {
                    Ok(source) => self.sources.push(source),
                    Err(error) => {
                        left_out(&error);
                        self.refused.push(*window);
                    }
                }
            }
        }
        // Captures of windows that left stop here.
        drop(kept);
        self.refresh_frames();
        self.drawn = Some((*frame, bottom_first));
        self.draw()
    }

    /// Whether every window under the rim has shown it a frame.
    pub fn ready(&self) -> bool {
        self.sources.iter().all(|source| source.frame.is_some())
    }
    /// Copies the frames captured windows announced, and draws the rim again if any came.
    pub fn refresh(&mut self) -> Result<bool> {
        if !self.refresh_frames() || self.drawn.is_none() {
            return Ok(false);
        }
        self.draw()?;
        Ok(true)
    }
    fn refresh_frames(&mut self) -> bool {
        let mut any = false;
        for source in &mut self.sources {
            match source.refresh(&self.d3d, &self.context, &self.winrt) {
                Ok(new) => any |= new,
                Err(error) => left_out(&error),
            }
        }
        any
    }

    fn draw(&mut self) -> Result<()> {
        let Some((frame, under)) = &self.drawn else {
            return Ok(());
        };
        let side = frame.window.width();
        unsafe {
            if side != self.side || self.surface.is_none() {
                let surface = self.composition.CreateSurface(
                    side.max(1) as u32,
                    side.max(1) as u32,
                    DXGI_FORMAT_B8G8R8A8_UNORM,
                    DXGI_ALPHA_MODE_PREMULTIPLIED,
                )?;
                self.visual.SetContent(&surface)?;
                self.surface = Some(surface);
                self.side = side;
            }
            let Some(surface) = &self.surface else {
                return Ok(());
            };
            let lens = Lens::new(frame);
            let mut offset = POINT::default();
            let target: ID3D11Texture2D = surface.BeginDraw(None, &mut offset)?;
            let drawn = (|| {
                let mut view = None;
                self.d3d
                    .CreateRenderTargetView(&target, None, Some(&mut view))?;
                let context = &self.context;
                context.OMSetRenderTargets(Some(&[view]), None);
                context.RSSetViewports(Some(&[D3D11_VIEWPORT {
                    TopLeftX: offset.x as f32,
                    TopLeftY: offset.y as f32,
                    Width: side as f32,
                    Height: side as f32,
                    MinDepth: 0.,
                    MaxDepth: 1.,
                }]));
                context.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
                context.IASetInputLayout(None);
                context.VSSetShader(&self.vertex, None);
                context.PSSetShader(&self.pixel, None);
                context.PSSetSamplers(0, Some(&[Some(self.sampler.clone())]));
                context.PSSetConstantBuffers(0, Some(&[Some(self.constants.clone())]));
                let centre = lens.radius as f32;
                let plateau_centre = [
                    frame.source.left as f32 + centre / lens.zoom as f32,
                    frame.source.top as f32 + centre / lens.zoom as f32,
                ];
                let mut constants = Constants {
                    offset: [offset.x as f32, offset.y as f32],
                    radius: centre,
                    plateau: lens.plateau as f32,
                    cubic: lens.cubic.map(|c| c as f32),
                    plateau_centre,
                    edge_shift: [
                        frame.window.left as f32 + centre - plateau_centre[0],
                        frame.window.top as f32 + centre - plateau_centre[1],
                    ],
                    zoom: lens.zoom as f32,
                    clear: 1.,
                    ..Default::default()
                };
                // The whole surface clear first, with no blending: BeginDraw keeps nothing.
                context.OMSetBlendState(None, None, u32::MAX);
                context.UpdateSubresource(
                    &self.constants,
                    0,
                    None,
                    (&raw const constants).cast(),
                    0,
                    0,
                );
                context.Draw(3, 0);
                context.OMSetBlendState(&self.over, None, u32::MAX);
                constants.clear = 0.;
                for (window, _) in under {
                    let Some(source) = self.sources.iter().find(|s| s.window == *window) else {
                        continue;
                    };
                    let Some((_, view, size)) = &source.frame else {
                        continue;
                    };
                    let Ok(at) = window_origin(*window) else {
                        continue;
                    };
                    constants.origin = [at.x as f32, at.y as f32];
                    constants.size = [size.Width as f32, size.Height as f32];
                    context.UpdateSubresource(
                        &self.constants,
                        0,
                        None,
                        (&raw const constants).cast(),
                        0,
                        0,
                    );
                    context.PSSetShaderResources(0, Some(&[Some(view.clone())]));
                    context.Draw(3, 0);
                }
                context.PSSetShaderResources(0, Some(&[None]));
                context.OMSetRenderTargets(None, None);
                Ok::<_, Error>(())
            })();
            surface.EndDraw()?;
            drawn
        }
    }
}
