//! ROLE: The windows the composed loupe cannot show by DWM thumbnail (the Windows 11 taskbar,
//! shell flyouts, any window export 147 refuses): their pixels by Windows.Graphics.Capture of
//! that one window, copied into a DirectComposition surface the loupe's discs all show.
//! OWNER: `loupe::Composed`, which opens a capture when such a window comes under the loupe and
//! drops it when the window leaves or the loupe closes.
//! INVARIANTS: A session lives only while its window is under the loupe: none runs when no such
//! window is. The loupe itself is never captured (one window per session, never the loupe's).
//! Frames are copied on the resident's thread: the capture thread only posts `FRAMES`, once
//! per burst. Each visual shows its window enlarged by a whole factor at whole pixels, nearest
//! neighbour, so every source pixel stays an exact block.
use super::*;
use shotoshot_core::loupe::Frame;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use windows::{
    Foundation::TypedEventHandler,
    Graphics::{
        Capture::{Direct3D11CaptureFramePool, GraphicsCaptureItem, GraphicsCaptureSession},
        DirectX::{Direct3D11::IDirect3DDevice, DirectXPixelFormat},
        SizeInt32,
    },
    Win32::{
        Graphics::{
            Direct3D11::*, DirectComposition::*, Dwm::*, Dxgi::Common::*, Dxgi::IDXGIDevice,
        },
        System::WinRT::{
            Direct3D11::{CreateDirect3D11DeviceFromDXGIDevice, IDirect3DDxgiInterfaceAccess},
            Graphics::Capture::IGraphicsCaptureItemInterop,
        },
    },
};
use windows_numerics::Matrix3x2;

/// Posted to the presentation's owner when a captured window has a new frame.
pub(crate) const FRAMES: u32 = WM_APP + 17;
/// A frame is already announced: a burst of frames makes one copy.
static PENDING: AtomicBool = AtomicBool::new(false);
/// How long a new capture may hold the loupe for its first frame: the taskbar arrives in a
/// frame or two, and a loupe shown without it would show what lies under the taskbar.
const FIRST_FRAME: Duration = Duration::from_millis(100);

/// Shell windows DWM thumbnails do not reproduce (2026-10-28: the taskbar 37 % exact by export
/// 147's fallback slot, 39 % by a classic thumbnail): captured even if export 147 accepts them.
const SHELL_CLASSES: [&str; 5] = [
    "Shell_TrayWnd",
    "Shell_SecondaryTrayWnd",
    "ControlCenterWindow",
    "XamlExplorerHostIslandWindow",
    "TopLevelWindowForOverflowXamlIsland",
];
/// Whether `class` names a window the loupe shows by capture from the start.
pub(super) fn shell_class(class: &str) -> bool {
    SHELL_CLASSES.contains(&class)
}

/// One window captured for the loupe.
pub(super) struct Capture {
    pub window: HWND,
    _item: GraphicsCaptureItem,
    device: IDirect3DDevice,
    pool: Direct3D11CaptureFramePool,
    session: GraphicsCaptureSession,
    arrived: i64,
    surface: IDCompositionSurface,
    size: SizeInt32,
    /// One per disc of the loupe, all showing `surface`.
    pub visuals: Vec<IDCompositionVisual2>,
}
impl Capture {
    /// Starts capturing `window` for a loupe of `discs` discs, and waits a little for its first
    /// frame. `notify` hears about later frames.
    pub fn new(
        window: HWND,
        d3d: &ID3D11Device,
        context: &ID3D11DeviceContext,
        composition: &IDCompositionDesktopDevice,
        discs: usize,
        notify: HWND,
    ) -> Result<Self> {
        unsafe {
            let winrt: IDirect3DDevice =
                CreateDirect3D11DeviceFromDXGIDevice(&d3d.cast::<IDXGIDevice>()?)?.cast()?;
            let item: GraphicsCaptureItem =
                factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>()?
                    .CreateForWindow(window)?;
            let size = item.Size()?;
            let pool = Direct3D11CaptureFramePool::CreateFreeThreaded(
                &winrt,
                DirectXPixelFormat::B8G8R8A8UIntNormalized,
                2,
                size,
            )?;
            let owner = notify.0 as isize;
            let arrived = pool.FrameArrived(&TypedEventHandler::new(move |_, _| {
                if !PENDING.swap(true, Ordering::SeqCst) {
                    let _ = PostMessageW(Some(HWND(owner as *mut _)), FRAMES, WPARAM(0), LPARAM(0));
                }
                Ok(())
            }))?;
            let session = pool.CreateCaptureSession(&item)?;
            let _ = session.SetIsCursorCaptureEnabled(false);
            // No yellow frame around a window nobody asked to record.
            let _ = session.SetIsBorderRequired(false);
            let surface = new_surface(composition, size)?;
            let mut visuals = Vec::with_capacity(discs);
            for _ in 0..discs {
                let visual = composition.CreateVisual()?;
                visual.SetContent(&surface)?;
                // Whole pixels at a whole scale: no soft edge where the window ends.
                visual.SetBorderMode(DCOMPOSITION_BORDER_MODE_HARD)?;
                visuals.push(visual);
            }
            let mut capture = Self {
                window,
                _item: item,
                device: winrt,
                pool,
                session,
                arrived,
                surface,
                size,
                visuals,
            };
            capture.session.StartCapture()?;
            let start = Instant::now();
            while !capture.refresh(composition, context)? && start.elapsed() < FIRST_FRAME {
                std::thread::sleep(Duration::from_millis(2));
            }
            Ok(capture)
        }
    }
    /// Copies the latest frame, if any came, into the surface; whether one did.
    pub fn refresh(
        &mut self,
        composition: &IDCompositionDesktopDevice,
        context: &ID3D11DeviceContext,
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
                return Ok(());
            }
            if size != self.size {
                // The window was resized: a surface of its new size, and frames to match.
                self.surface = new_surface(composition, size)?;
                for visual in &self.visuals {
                    visual.SetContent(&self.surface)?;
                }
                self.pool.Recreate(
                    &self.device,
                    DirectXPixelFormat::B8G8R8A8UIntNormalized,
                    2,
                    size,
                )?;
                self.size = size;
            }
            let source: ID3D11Texture2D = frame
                .Surface()?
                .cast::<IDirect3DDxgiInterfaceAccess>()?
                .GetInterface()?;
            let mut offset = POINT::default();
            let target: ID3D11Texture2D = self.surface.BeginDraw(None, &mut offset)?;
            context.CopySubresourceRegion(
                &target,
                0,
                offset.x as u32,
                offset.y as u32,
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
            self.surface.EndDraw()
        })();
        let _ = frame.Close();
        copied.map(|()| true)
    }
    /// Places the captured window in each disc as `frame` enlarges the screen.
    pub fn place(&self, frame: &Frame) -> Result<()> {
        let at = window_origin(self.window)?;
        let zoom = frame.zoom as f32;
        let matrix = Matrix3x2 {
            M11: zoom,
            M12: 0.,
            M21: 0.,
            M22: zoom,
            M31: ((at.x - frame.source.left) * frame.zoom) as f32,
            M32: ((at.y - frame.source.top) * frame.zoom) as f32,
        };
        for visual in &self.visuals {
            unsafe { visual.SetTransform2(&matrix)? };
        }
        Ok(())
    }
}
impl Drop for Capture {
    fn drop(&mut self) {
        let _ = self.pool.RemoveFrameArrived(self.arrived);
        let _ = self.session.Close();
        let _ = self.pool.Close();
    }
}

fn new_surface(
    composition: &IDCompositionDesktopDevice,
    size: SizeInt32,
) -> Result<IDCompositionSurface> {
    unsafe {
        composition.CreateSurface(
            size.Width.max(1) as u32,
            size.Height.max(1) as u32,
            DXGI_FORMAT_B8G8R8A8_UNORM,
            DXGI_ALPHA_MODE_PREMULTIPLIED,
        )
    }
}

/// Where a window's captured frames start on screen: its visible bounds, without the invisible
/// resizing borders Windows 11 counts in its rectangle.
fn window_origin(window: HWND) -> Result<POINT> {
    let mut bounds = RECT::default();
    unsafe {
        if DwmGetWindowAttribute(
            window,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            &mut bounds as *mut RECT as _,
            std::mem::size_of::<RECT>() as u32,
        )
        .is_err()
        {
            GetWindowRect(window, &mut bounds)?;
        }
    }
    Ok(POINT {
        x: bounds.left,
        y: bounds.top,
    })
}

/// The announced frames are being copied: the next frame announces again.
pub(super) fn frames_seen() {
    PENDING.store(false, Ordering::SeqCst);
}
