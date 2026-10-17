//! ROLE: Mirrors presentation layers into one stable off-screen window that OBS window capture
//! (Windows 10 method) records with its transparency, above a Game Capture source.
//! OWNER: Presentation; exists while any presentation feature is on.
//! INVARIANTS: Parked off every desktop, never activated; the lock stays private to the streamer.
//! HOTPATH: Moving a layer only changes a visual offset; pixels upload once per repaint.
use super::*;
use crate::resources::OwnedWindow;
use std::cell::{Cell, RefCell};
use windows::Win32::Graphics::{
    Direct3D::*,
    Direct3D11::*,
    DirectComposition::*,
    Dxgi::{Common::*, IDXGIDevice},
};

/// Name streamers pick in OBS; OBS also matches it to find the window again after a restart.
pub(crate) const TITLE: PCWSTR = w!("Shotoshot overlay");
// Far outside any desktop, yet still composed by DWM, so window capture keeps receiving frames.
const PARKED: i32 = -32000;

/// Mirrored layers, bottom to top.
#[derive(Clone, Copy)]
pub(super) enum Slot {
    Canvas,
    Keys,
    Halo,
}

struct Content {
    // The layer window that owns the slot; a replaced layer can no longer touch it.
    source: HWND,
    surface: IDCompositionSurface,
    width: i32,
    height: i32,
}

pub(super) struct Broadcast {
    _window: OwnedWindow,
    // Virtual-screen origin: the window maps the whole desktop one to one.
    origin: POINT,
    context: ID3D11DeviceContext,
    composition: IDCompositionDevice,
    _target: IDCompositionTarget,
    _root: IDCompositionVisual,
    visuals: [IDCompositionVisual; 3],
    contents: RefCell<[Option<Content>; 3]>,
    // A lost GPU device stops the mirror; the screen overlays keep working.
    broken: Cell<bool>,
}
impl Broadcast {
    pub fn new(owner: HWND) -> Result<Self> {
        unsafe {
            let instance = GetModuleHandleW(None)?;
            if RegisterClassW(&WNDCLASSW {
                lpfnWndProc: Some(broadcast_proc),
                hInstance: instance.into(),
                lpszClassName: w!("Shotoshot.Broadcast"),
                ..Default::default()
            }) == 0
                && GetLastError() != ERROR_CLASS_ALREADY_EXISTS
            {
                return Err(Error::from_thread());
            }
            let origin = POINT {
                x: GetSystemMetrics(SM_XVIRTUALSCREEN),
                y: GetSystemMetrics(SM_YVIRTUALSCREEN),
            };
            // PITFALL: OBS never lists WS_EX_TOOLWINDOW windows; the owner keeps this one out of
            // the taskbar and Alt+Tab instead.
            let window = OwnedWindow(CreateWindowExW(
                WS_EX_NOREDIRECTIONBITMAP | WS_EX_NOACTIVATE,
                w!("Shotoshot.Broadcast"),
                TITLE,
                WS_POPUP,
                PARKED,
                PARKED,
                GetSystemMetrics(SM_CXVIRTUALSCREEN),
                GetSystemMetrics(SM_CYVIRTUALSCREEN),
                Some(owner),
                None,
                Some(instance.into()),
                None,
            )?);
            let (device, context) = device()?;
            let composition: IDCompositionDevice =
                DCompositionCreateDevice(&device.cast::<IDXGIDevice>()?)?;
            let target = composition.CreateTargetForHwnd(window.0, true)?;
            let root = composition.CreateVisual()?;
            let visuals = [
                composition.CreateVisual()?,
                composition.CreateVisual()?,
                composition.CreateVisual()?,
            ];
            // PITFALL: With no reference visual, insertAbove = false stacks each child on top.
            for visual in &visuals {
                root.AddVisual(visual, false, None::<&IDCompositionVisual>)?;
            }
            target.SetRoot(&root)?;
            composition.Commit()?;
            let _ = ShowWindow(window.0, SW_SHOWNOACTIVATE);
            Ok(Self {
                _window: window,
                origin,
                context,
                composition,
                _target: target,
                _root: root,
                visuals,
                contents: RefCell::new([None, None, None]),
                broken: Cell::new(false),
            })
        }
    }
    #[cfg(test)]
    pub fn hwnd(&self) -> HWND {
        self._window.0
    }
    /// Mirrors `hwnd`'s placement and visibility and, when given, the changed region (layer
    /// coordinates) of its premultiplied top-down BGRA pixels.
    pub fn sync(&self, slot: Slot, hwnd: HWND, pixels: Option<(&[u8], RECT)>) {
        let mut rect = RECT::default();
        let shown = unsafe { IsWindowVisible(hwnd) }.as_bool();
        let result = unsafe { GetWindowRect(hwnd, &mut rect) }
            .and_then(|()| self.place(slot, hwnd, rect, shown, pixels));
        if result.is_err() {
            self.broken.set(true);
        }
    }
    pub fn remove(&self, slot: Slot, source: HWND) {
        let mut contents = self.contents.borrow_mut();
        if self.broken.get()
            || contents[slot as usize]
                .as_ref()
                .is_some_and(|content| content.source != source)
        {
            return;
        }
        contents[slot as usize] = None;
        let result = unsafe {
            self.visuals[slot as usize]
                .SetContent(None::<&windows::core::IUnknown>)
                .and_then(|()| self.composition.Commit())
        };
        if result.is_err() {
            self.broken.set(true);
        }
    }
    fn place(
        &self,
        slot: Slot,
        source: HWND,
        rect: RECT,
        shown: bool,
        pixels: Option<(&[u8], RECT)>,
    ) -> Result<()> {
        if self.broken.get() {
            return Ok(());
        }
        let index = slot as usize;
        let (width, height) = (rect.right - rect.left, rect.bottom - rect.top);
        let mut contents = self.contents.borrow_mut();
        let replaced = contents[index]
            .as_ref()
            .is_some_and(|content| content.source != source);
        if replaced && pixels.is_none() {
            return Ok(());
        }
        unsafe {
            if let Some((pixels, mut region)) = pixels {
                if pixels.len() != (width * height * 4) as usize
                    || region.left < 0
                    || region.top < 0
                    || region.right > width
                    || region.bottom > height
                {
                    return Err(Error::from_hresult(E_INVALIDARG));
                }
                let content = match &contents[index] {
                    Some(content)
                        if !replaced && content.width == width && content.height == height =>
                    {
                        content
                    }
                    // A new surface has no valid pixels yet: fill all of it once.
                    _ => {
                        region = RECT {
                            right: width,
                            bottom: height,
                            ..Default::default()
                        };
                        contents[index].insert(Content {
                            source,
                            surface: self.composition.CreateSurface(
                                width as u32,
                                height as u32,
                                DXGI_FORMAT_B8G8R8A8_UNORM,
                                DXGI_ALPHA_MODE_PREMULTIPLIED,
                            )?,
                            width,
                            height,
                        })
                    }
                };
                if region.right > region.left && region.bottom > region.top {
                    // The surface lives in a shared atlas: write only where BeginDraw points.
                    let mut offset = POINT::default();
                    let texture: ID3D11Texture2D =
                        content.surface.BeginDraw(Some(&region), &mut offset)?;
                    self.context.UpdateSubresource(
                        &texture,
                        0,
                        Some(&D3D11_BOX {
                            left: offset.x as u32,
                            top: offset.y as u32,
                            front: 0,
                            right: (offset.x + region.right - region.left) as u32,
                            bottom: (offset.y + region.bottom - region.top) as u32,
                            back: 1,
                        }),
                        pixels
                            .as_ptr()
                            .add(((region.top * width + region.left) * 4) as usize)
                            .cast(),
                        (width * 4) as u32,
                        0,
                    );
                    content.surface.EndDraw()?;
                }
            }
            let visual = &self.visuals[index];
            visual.SetOffsetX2((rect.left - self.origin.x) as f32)?;
            visual.SetOffsetY2((rect.top - self.origin.y) as f32)?;
            match &contents[index] {
                Some(content) if shown => visual.SetContent(&content.surface)?,
                _ => visual.SetContent(None::<&windows::core::IUnknown>)?,
            }
            self.composition.Commit()
        }
    }
}

fn device() -> Result<(ID3D11Device, ID3D11DeviceContext)> {
    // WARP keeps the mirror alive on machines without a usable GPU driver.
    let mut failure = Error::from_hresult(E_FAIL);
    for driver in [D3D_DRIVER_TYPE_HARDWARE, D3D_DRIVER_TYPE_WARP] {
        let mut device = None;
        let mut context = None;
        match unsafe {
            D3D11CreateDevice(
                None,
                driver,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                None,
                D3D11_SDK_VERSION,
                Some(&mut device),
                None,
                Some(&mut context),
            )
        } {
            Ok(()) => {
                if let (Some(device), Some(context)) = (device, context) {
                    return Ok((device, context));
                }
            }
            Err(error) => failure = error,
        }
    }
    Err(failure)
}

unsafe extern "system" fn broadcast_proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    unsafe { DefWindowProcW(hwnd, msg, w, l) }
}

#[cfg(test)]
mod tests {
    //! Captures the mirror the way OBS 28+ window capture does: Windows.Graphics.Capture on
    //! one HWND, BGRA8, premultiplied, with one session held open.
    use super::*;
    use std::time::{Duration, Instant};
    use windows::{
        Graphics::{
            Capture::{
                Direct3D11CaptureFrame, Direct3D11CaptureFramePool, GraphicsCaptureItem,
                GraphicsCaptureSession,
            },
            DirectX::{Direct3D11::IDirect3DDevice, DirectXPixelFormat},
        },
        Win32::System::WinRT::{
            Direct3D11::{CreateDirect3D11DeviceFromDXGIDevice, IDirect3DDxgiInterfaceAccess},
            Graphics::Capture::IGraphicsCaptureItemInterop,
        },
    };

    struct Capture {
        device: ID3D11Device,
        context: ID3D11DeviceContext,
        pool: Direct3D11CaptureFramePool,
        session: GraphicsCaptureSession,
    }
    impl Capture {
        fn new(hwnd: HWND) -> Self {
            unsafe {
                let (device, context) = device().unwrap();
                let winrt: IDirect3DDevice =
                    CreateDirect3D11DeviceFromDXGIDevice(&device.cast::<IDXGIDevice>().unwrap())
                        .unwrap()
                        .cast()
                        .unwrap();
                let item: GraphicsCaptureItem =
                    factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>()
                        .unwrap()
                        .CreateForWindow(hwnd)
                        .unwrap();
                let pool = Direct3D11CaptureFramePool::CreateFreeThreaded(
                    &winrt,
                    DirectXPixelFormat::B8G8R8A8UIntNormalized,
                    1,
                    item.Size().unwrap(),
                )
                .unwrap();
                let session = pool.CreateCaptureSession(&item).unwrap();
                let _ = session.SetIsCursorCaptureEnabled(false);
                session.StartCapture().unwrap();
                Self {
                    device,
                    context,
                    pool,
                    session,
                }
            }
        }
        /// Waits for the first frame whose pixel at `(x, y)` satisfies `ready`.
        fn wait(&self, x: u32, y: u32, ready: impl Fn([u8; 4]) -> bool) -> Frame {
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                if let Ok(frame) = self.pool.TryGetNextFrame() {
                    let read = self.read(&frame);
                    frame.Close().unwrap();
                    if ready(read.at(x, y)) {
                        return read;
                    }
                }
                assert!(Instant::now() < deadline, "no matching capture frame");
                std::thread::sleep(Duration::from_millis(10));
            }
        }
        fn read(&self, frame: &Direct3D11CaptureFrame) -> Frame {
            unsafe {
                let source: ID3D11Texture2D = frame
                    .Surface()
                    .unwrap()
                    .cast::<IDirect3DDxgiInterfaceAccess>()
                    .unwrap()
                    .GetInterface()
                    .unwrap();
                let mut desc = D3D11_TEXTURE2D_DESC::default();
                source.GetDesc(&mut desc);
                desc.Usage = D3D11_USAGE_STAGING;
                desc.BindFlags = 0;
                desc.CPUAccessFlags = D3D11_CPU_ACCESS_READ.0 as u32;
                desc.MiscFlags = 0;
                let mut staging = None;
                self.device
                    .CreateTexture2D(&desc, None, Some(&mut staging))
                    .unwrap();
                let staging = staging.unwrap();
                self.context.CopyResource(&staging, &source);
                let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
                self.context
                    .Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))
                    .unwrap();
                let mut pixels = Vec::with_capacity((desc.Width * desc.Height * 4) as usize);
                for row in 0..desc.Height as usize {
                    pixels.extend_from_slice(std::slice::from_raw_parts(
                        mapped
                            .pData
                            .cast::<u8>()
                            .add(row * mapped.RowPitch as usize),
                        desc.Width as usize * 4,
                    ));
                }
                self.context.Unmap(&staging, 0);
                Frame {
                    width: desc.Width,
                    height: desc.Height,
                    pixels,
                }
            }
        }
    }
    impl Drop for Capture {
        fn drop(&mut self) {
            let _ = self.session.Close();
            let _ = self.pool.Close();
        }
    }
    struct Frame {
        width: u32,
        height: u32,
        pixels: Vec<u8>,
    }
    impl Frame {
        fn at(&self, x: u32, y: u32) -> [u8; 4] {
            let i = ((y * self.width + x) * 4) as usize;
            self.pixels[i..i + 4].try_into().unwrap()
        }
    }

    fn owner() -> OwnedWindow {
        unsafe {
            OwnedWindow(
                CreateWindowExW(
                    WS_EX_TOOLWINDOW,
                    w!("STATIC"),
                    w!(""),
                    WS_POPUP,
                    0,
                    0,
                    1,
                    1,
                    None,
                    None,
                    None,
                    None,
                )
                .unwrap(),
            )
        }
    }
    fn solid(width: i32, height: i32, bgra: [u8; 4]) -> Vec<u8> {
        bgra.repeat((width * height) as usize)
    }
    const SOURCE: HWND = HWND(1 as _);
    fn whole(width: i32, height: i32) -> RECT {
        RECT {
            right: width,
            bottom: height,
            ..Default::default()
        }
    }
    fn at(origin: POINT, x: i32, y: i32, width: i32, height: i32) -> RECT {
        RECT {
            left: origin.x + x,
            top: origin.y + y,
            right: origin.x + x + width,
            bottom: origin.y + y + height,
        }
    }

    /// Captures its window through Windows.Graphics.Capture, which needs the compositor of an
    /// interactive desktop: GitHub's machines refuse it ("Could not capture the given window").
    #[test]
    #[ignore = "real desktop: captures a window through the compositor, run alone by check.ps1"]
    fn real_desktop_obs_lists_the_parked_mirror_and_receives_alpha_placement_order_and_updates() {
        let _com = crate::ComApartment::new().unwrap();
        let owner = owner();
        let mirror = Broadcast::new(owner.0).unwrap();
        let hwnd = mirror.hwnd();
        unsafe {
            // OBS lists only visible, non-tool, non-child windows (window-helpers.c).
            let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
            let style = GetWindowLongPtrW(hwnd, GWL_STYLE) as u32;
            assert!(IsWindowVisible(hwnd).as_bool());
            assert_eq!(ex & WS_EX_TOOLWINDOW.0, 0);
            assert_eq!(style & WS_CHILD.0, 0);
            assert_eq!(GetWindow(hwnd, GW_OWNER).unwrap(), owner.0);
            let mut rect = RECT::default();
            GetWindowRect(hwnd, &mut rect).unwrap();
            assert!(rect.right <= GetSystemMetrics(SM_XVIRTUALSCREEN));
        }
        let origin = mirror.origin;
        // Half-transparent red canvas under an opaque green halo that overlaps its corner.
        let red = [0, 0, 128, 128];
        let green = [0, 255, 0, 255];
        mirror
            .place(
                Slot::Canvas,
                SOURCE,
                at(origin, 10, 20, 40, 30),
                true,
                Some((&solid(40, 30, red), whole(40, 30))),
            )
            .unwrap();
        mirror
            .place(
                Slot::Halo,
                SOURCE,
                at(origin, 40, 40, 20, 20),
                true,
                Some((&solid(20, 20, green), whole(20, 20))),
            )
            .unwrap();
        let capture = Capture::new(hwnd);
        let frame = capture.wait(15, 25, |p| p[3] != 0);
        assert_eq!((frame.width, frame.height), unsafe {
            (
                GetSystemMetrics(SM_CXVIRTUALSCREEN) as u32,
                GetSystemMetrics(SM_CYVIRTUALSCREEN) as u32,
            )
        });
        assert_eq!(frame.at(15, 25), red, "premultiplied alpha must survive");
        assert_eq!(frame.at(45, 45), green, "the halo stays above the canvas");
        assert_eq!(frame.at(9, 25)[3], 0, "left of the canvas is transparent");
        assert_eq!(frame.at(55, 55), green, "the halo extends past the canvas");

        // Moving only shifts the visual; the open session sees it without new pixels.
        mirror
            .place(Slot::Halo, SOURCE, at(origin, 100, 40, 20, 20), true, None)
            .unwrap();
        let frame = capture.wait(105, 45, |p| p == green);
        assert_eq!(
            frame.at(45, 45),
            red,
            "the canvas shows again where the halo was"
        );

        // A partial upload replaces only its region and keeps the rest of the surface.
        let blue = [255, 0, 0, 255];
        let mut pixels = solid(40, 30, blue);
        pixels[..4].copy_from_slice(&green);
        let corner = RECT {
            left: 0,
            top: 0,
            right: 5,
            bottom: 5,
        };
        mirror
            .place(
                Slot::Canvas,
                SOURCE,
                at(origin, 10, 20, 40, 30),
                true,
                Some((&pixels, corner)),
            )
            .unwrap();
        let frame = capture.wait(10, 20, |p| p == green);
        assert_eq!(frame.at(14, 24), blue, "inside the region");
        assert_eq!(frame.at(16, 26), red, "outside the region");

        mirror
            .place(Slot::Halo, SOURCE, at(origin, 100, 40, 20, 20), false, None)
            .unwrap();
        capture.wait(105, 45, |p| p[3] == 0);
        // A replaced layer leaving late must not wipe its successor.
        mirror.remove(Slot::Canvas, HWND(2 as _));
        mirror
            .place(
                Slot::Canvas,
                HWND(2 as _),
                at(origin, 0, 0, 1, 1),
                false,
                None,
            )
            .unwrap();
        // Show the halo again to get a fresh frame, then check the canvas survived.
        mirror
            .place(Slot::Halo, SOURCE, at(origin, 100, 40, 20, 20), true, None)
            .unwrap();
        let frame = capture.wait(105, 45, |p| p == green);
        assert_eq!(frame.at(15, 25), red);
        assert_eq!(frame.at(10, 20), green);
        mirror.remove(Slot::Canvas, SOURCE);
        capture.wait(15, 25, |p| p[3] == 0);
        assert!(!mirror.broken.get());
    }

    #[test]
    fn mirrored_layer_uploads_on_present_and_leaves_on_drop() {
        let _com = crate::ComApartment::new().unwrap();
        let owner = owner();
        let mirror = std::rc::Rc::new(Broadcast::new(owner.0).unwrap());
        let layer = Layer::new(std::ptr::null(), -32000, -32000, 30, 20)
            .unwrap()
            .mirrored(Some(mirror.clone()), Slot::Keys);
        layer.begin(false).unwrap();
        layer.end().unwrap();
        assert!(mirror.contents.borrow()[Slot::Keys as usize].is_some());
        drop(layer);
        assert!(mirror.contents.borrow()[Slot::Keys as usize].is_none());
        assert!(!mirror.broken.get());
    }
}
