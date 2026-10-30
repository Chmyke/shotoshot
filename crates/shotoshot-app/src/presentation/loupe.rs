//! ROLE: The live loupe's window: DWM draws in it, enlarged, the windows under the pointer, as a
//! pure round magnifier whose edge fades into the screen, with nothing drawn around it.
//! OWNER: Presentation, while the loupe is on screen; dropped as soon as it leaves.
//! INVARIANTS: The loupe is never one of its own sources, nor is any window the caller excludes
//! (halo, input diagram). It is shown only once its content is in place. A window that refuses
//! capture never enters it. Nothing polls: pointer moves and window events drive it. A failure
//! of the composed renderer never ends the loupe: it falls back to the banded one, said once.
//! Its thread holds a COM apartment (`ComApartment`) while it lives: captures go through WinRT.
//!
//! Two renderers (`docs/recherche/2026-10-28-loupe-essai/`):
//! - Composed, preferred: DWM shared thumbnail visuals of the windows (dwmapi's undocumented
//!   export 147, as VLC loads it) in a DirectComposition tree, in nested round discs whose
//!   opacities fade the edge over 4 pixels. Pixel for pixel at 2x inside the fading
//!   edge, seen by every capture API.
//! - Banded, the fallback: classic thumbnails in round bands inside a round window region. Its
//!   edge is stepped, a few pixels at most, with no ring to hide the steps.
use super::loupe_capture::{self, Capture};
use super::*;
use crate::resources::OwnedWindow;
use shotoshot_core::loupe::{self as geometry, PixelRect};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use windows::Win32::{
    Graphics::{
        Direct2D::Common::D2D_MATRIX_5X4_F,
        Direct3D11::{ID3D11Device, ID3D11DeviceContext},
        DirectComposition::*,
        Dwm::*,
        Dxgi::IDXGIDevice,
    },
    System::Registry::*,
    UI::Accessibility::*,
};

/// Posted to the presentation's owner when a window may have moved under the loupe.
pub(crate) const WINDOWS_CHANGED: u32 = WM_APP + 16;
/// The owner window that hears about window changes, while a loupe exists.
static NOTIFY: AtomicIsize = AtomicIsize::new(0);
/// A change is already posted: a burst of events makes one refresh.
static PENDING: AtomicBool = AtomicBool::new(false);
/// Where nothing is under the banded loupe (beyond the desktop's edge).
const BACKGROUND: u32 = 0x00202020;
/// How far, in window pixels, the banded loupe's steps may stray from its circle, inside or
/// out: half of it each way. Finer needs about one thumbnail per row and window.
pub(super) const BAND_STEPS: i32 = 6;

/// How the loupe is drawn.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Renderer {
    Composed,
    Banded,
}

pub(super) struct Loupe {
    // ORDER: The composition goes before the window it targets.
    draw: Draw,
    window: OwnedWindow,
    hooks: Vec<HWINEVENTHOOK>,
    side: i32,
    shown: bool,
    /// What the loupe last showed: when nothing of it changed, a window event elsewhere on the
    /// desktop costs a listing and nothing more.
    last: Option<Shown>,
}
enum Draw {
    Composed(Composed),
    /// Registered thumbnails, bottom first and band by band, with the window each shows.
    Banded(Vec<(HWND, isize)>),
}
/// The frame, the window above and the windows under the loupe.
type Shown = (geometry::Frame, Option<HWND>, Vec<(HWND, PixelRect)>);
impl Loupe {
    /// The composed loupe where this Windows offers it, the banded one otherwise.
    pub fn new(owner: HWND) -> Result<Self> {
        Self::with(owner, Renderer::Composed)
    }
    /// The fallback loupe, whatever this Windows offers: for its own probe.
    #[cfg(test)]
    pub fn banded(owner: HWND) -> Result<Self> {
        Self::with(owner, Renderer::Banded)
    }
    fn with(owner: HWND, wanted: Renderer) -> Result<Self> {
        let composed = (wanted == Renderer::Composed)
            .then(shared_visuals)
            .flatten();
        let (window, draw) = match composed.map(|create| {
            let window = loupe_window(true)?;
            let composed = Composed::new(window.0, create)?;
            Ok::<_, Error>((window, composed))
        }) {
            Some(Ok((window, composed))) => (window, Draw::Composed(composed)),
            failed => {
                if let Some(Err(error)) = failed {
                    fell_back(format!("DirectComposition: {error}"));
                }
                (loupe_window(false)?, Draw::Banded(Vec::new()))
            }
        };
        NOTIFY.store(owner.0 as isize, Ordering::SeqCst);
        PENDING.store(false, Ordering::SeqCst);
        // Out of context: the callbacks arrive through this thread's message loop.
        let mut hooks = Vec::new();
        for (first, last) in [
            (EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_MINIMIZEEND),
            (EVENT_OBJECT_SHOW, EVENT_OBJECT_REORDER),
            (EVENT_OBJECT_LOCATIONCHANGE, EVENT_OBJECT_LOCATIONCHANGE),
        ] {
            let hook = unsafe {
                SetWinEventHook(
                    first,
                    last,
                    None,
                    Some(window_event),
                    0,
                    0,
                    WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
                )
            };
            if !hook.is_invalid() {
                hooks.push(hook);
            }
        }
        Ok(Self {
            draw,
            window,
            hooks,
            side: 0,
            shown: false,
            last: None,
        })
    }
    pub fn hwnd(&self) -> HWND {
        self.window.0
    }
    /// How many windows under the loupe it shows by capture.
    pub fn captured(&self) -> usize {
        match &self.draw {
            Draw::Composed(composed) => composed.captures.len(),
            Draw::Banded(_) => 0,
        }
    }
    pub fn renderer(&self) -> Renderer {
        match self.draw {
            Draw::Composed(_) => Renderer::Composed,
            Draw::Banded(_) => Renderer::Banded,
        }
    }
    /// Centres the loupe on `at` and fills it with what lies under it, just under `above`.
    /// `diameter` is in device pixels.
    pub fn update(
        &mut self,
        at: POINT,
        diameter: i32,
        zoom: i32,
        above: Option<HWND>,
        excluded: &[HWND],
    ) -> Result<()> {
        let frame = geometry::frame(at.x, at.y, diameter, zoom);
        let mut skip = excluded.to_vec();
        skip.push(self.hwnd());
        let under = windows_under(&frame.source, &skip);
        let seen = (frame, above, under);
        if self.shown && self.last.as_ref() == Some(&seen) {
            return Ok(());
        }
        if let Err(error) = self.draw(&frame, &seen.2) {
            if self.renderer() == Renderer::Banded {
                return Err(error);
            }
            // A new window, never shown yet: the banded loupe takes over from the next frame.
            fell_back(format!("DirectComposition: {error}"));
            self.draw = Draw::Banded(Vec::new());
            self.window = loupe_window(false)?;
            self.side = 0;
            self.shown = false;
            self.draw(&frame, &seen.2)?;
        }
        self.last = Some(seen);
        // Shown with its content in the same composition: never empty for a frame.
        let show = if self.shown {
            SET_WINDOW_POS_FLAGS(0)
        } else {
            SWP_SHOWWINDOW
        };
        let side = frame.window.width();
        unsafe {
            SetWindowPos(
                self.hwnd(),
                Some(above.unwrap_or(HWND_TOPMOST)),
                frame.window.left,
                frame.window.top,
                side,
                side,
                SWP_NOACTIVATE | show,
            )?;
        }
        self.shown = true;
        Ok(())
    }
    /// Copies the frames of the windows the loupe shows by capture. A failure leaves the last
    /// frame on screen, said once: the loupe goes on.
    pub fn frames(&mut self) {
        loupe_capture::frames_seen();
        if let Draw::Composed(composed) = &mut self.draw
            && let Err(error) = composed.refresh()
        {
            left_out(&error);
        }
    }
    /// Puts in the loupe's window the windows `under` it, enlarged as `frame` says.
    fn draw(&mut self, frame: &geometry::Frame, under: &[(HWND, PixelRect)]) -> Result<()> {
        let hwnd = self.hwnd();
        let side = frame.window.width();
        match &mut self.draw {
            Draw::Composed(composed) => composed.place(frame, under)?,
            Draw::Banded(thumbnails) => {
                if side != self.side {
                    // The region belongs to the window once set: no handle to free.
                    unsafe {
                        let region = CreateEllipticRgn(0, 0, side + 1, side + 1);
                        SetWindowRgn(hwnd, Some(region), true);
                    }
                }
                place_bands(hwnd, thumbnails, frame, under);
            }
        }
        self.side = side;
        Ok(())
    }
}
impl Drop for Loupe {
    fn drop(&mut self) {
        for hook in self.hooks.drain(..) {
            unsafe {
                let _ = UnhookWinEvent(hook);
            }
        }
        NOTIFY.store(0, Ordering::SeqCst);
        if let Draw::Banded(thumbnails) = &mut self.draw {
            unregister(thumbnails);
        }
    }
}

/// The loupe's click-through, never activated, topmost window, hidden. The composed one has no
/// redirection surface: DirectComposition alone draws in it.
fn loupe_window(composed: bool) -> Result<OwnedWindow> {
    unsafe {
        let instance = GetModuleHandleW(None)?;
        if RegisterClassW(&WNDCLASSW {
            lpfnWndProc: Some(loupe_proc),
            hInstance: instance.into(),
            lpszClassName: w!("Shotoshot.Loupe"),
            hbrBackground: CreateSolidBrush(COLORREF(BACKGROUND)),
            ..Default::default()
        }) == 0
            && GetLastError() != ERROR_CLASS_ALREADY_EXISTS
        {
            return Err(Error::from_thread());
        }
        let surface = if composed {
            WS_EX_NOREDIRECTIONBITMAP
        } else {
            WINDOW_EX_STYLE(0)
        };
        let window = OwnedWindow(CreateWindowExW(
            WS_EX_TOOLWINDOW
                | WS_EX_LAYERED
                | WS_EX_TRANSPARENT
                | WS_EX_NOACTIVATE
                | WS_EX_TOPMOST
                | surface,
            w!("Shotoshot.Loupe"),
            w!("Shotoshot loupe"),
            WS_POPUP,
            0,
            0,
            1,
            1,
            None,
            None,
            Some(instance.into()),
            None,
        )?);
        // A layered window passes clicks through; its own pixels stay opaque.
        SetLayeredWindowAttributes(window.0, COLORREF(0), 255, LWA_ALPHA)?;
        Ok(window)
    }
}

/// dwmapi's export 147, `DwmpCreateSharedThumbnailVisual`: a thumbnail of `source` as a
/// DirectComposition visual of `device`, for `destination`. Undocumented; signature from VLC
/// (`compositor_dcomp_acrylicsurface.hpp`) and the 2026-10-28 spike.
type CreateSharedThumbnailVisual = unsafe extern "system" fn(
    destination: HWND,
    source: HWND,
    flags: u32,
    properties: *mut DWM_THUMBNAIL_PROPERTIES,
    device: *mut core::ffi::c_void,
    visual: *mut *mut core::ffi::c_void,
    thumbnail: *mut isize,
) -> HRESULT;
/// The only thumbnail flags value known to work with export 147.
const SHARED_THUMBNAIL_FLAGS: u32 = 2;
/// Undocumented thumbnail property the spike and VLC pass with shared visuals.
const DWM_TNP_ENABLE3D: u32 = 0x0400_0000;
/// First build the composed loupe is tried on: Windows 11. The ordinal is not a contract, and
/// an older Windows may hold something else there.
const FIRST_COMPOSED_BUILD: u32 = 22000;

/// Why the composed loupe is not used, once it failed or proved missing: said once, kept for
/// the diagnostic.
static FALLBACK: OnceLock<String> = OnceLock::new();
fn fell_back(reason: String) {
    if FALLBACK.set(reason).is_ok()
        && let Some(reason) = FALLBACK.get()
    {
        eprintln!("Loupe: {reason}; drawn in round bands instead");
    }
}
/// Why the composed loupe is not used, if it is not.
pub(crate) fn fallback_reason() -> Option<&'static str> {
    FALLBACK.get().map(String::as_str)
}
/// The composed loupe showed a window in this process: export 147 works here. A window it
/// refuses (2026-10-28: E_INVALIDARG, every time, for a `com.pais.handy-sic` window) is then
/// left out of the loupe, as the banded loupe leaves out a refused thumbnail, instead of giving
/// up the fading edge for good. Before, a frame where it showed no window at all and refused
/// one gives the loupe to the banded renderer.
static SHARED_WORKED: AtomicBool = AtomicBool::new(false);
/// A window left out of the composed loupe: said once.
fn left_out(error: &Error) {
    static SAID: AtomicBool = AtomicBool::new(false);
    if !SAID.swap(true, Ordering::SeqCst) {
        eprintln!("Loupe: a window left out ({error})");
    }
}

/// Export 147 of dwmapi, when this Windows has it and it never failed in this process.
fn shared_visuals() -> Option<CreateSharedThumbnailVisual> {
    static EXPORT: OnceLock<Option<CreateSharedThumbnailVisual>> = OnceLock::new();
    if FALLBACK.get().is_some() {
        return None;
    }
    *EXPORT.get_or_init(|| {
        let build = windows_build();
        if build < FIRST_COMPOSED_BUILD {
            fell_back(format!("Windows build {build}, before {FIRST_COMPOSED_BUILD}"));
            return None;
        }
        // dwmapi is linked already: loading it again never unloads it.
        let export = unsafe {
            LoadLibraryW(w!("dwmapi.dll"))
                .ok()
                .and_then(|module| GetProcAddress(module, PCSTR(147 as *const u8)))
        };
        let Some(export) = export else {
            fell_back("dwmapi has no export 147".into());
            return None;
        };
        // SAFETY: the signature VLC and the spike call on Windows 11.
        Some(unsafe {
            std::mem::transmute::<unsafe extern "system" fn() -> isize, CreateSharedThumbnailVisual>(
                export,
            )
        })
    })
}
/// This Windows' build number, 0 when unknown.
fn windows_build() -> u32 {
    let mut text = [0u16; 16];
    let mut size = std::mem::size_of_val(&text) as u32;
    let read = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            w!(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion"),
            w!("CurrentBuildNumber"),
            RRF_RT_REG_SZ,
            None,
            Some(text.as_mut_ptr().cast()),
            Some(&mut size),
        )
    };
    if read.is_err() {
        return 0;
    }
    String::from_utf16_lossy(&text)
        .trim_end_matches('\0')
        .parse()
        .unwrap_or(0)
}

/// Discs of the composed loupe's fading edge beyond the opaque one: each holds its own copy of
/// the windows under the loupe, so the cost of a move grows with them.
pub(super) const FEATHER_STEPS: usize = 4;

/// The composed loupe: discs of the same enlargement laid over one another, an opaque one
/// within the fading edge, then wider and fainter ones (`geometry::feathered`): the loupe fades
/// into the screen with no line where they meet.
struct Composed {
    create: CreateSharedThumbnailVisual,
    /// Rendering device of the composition, which also copies captured frames.
    d3d: ID3D11Device,
    context: ID3D11DeviceContext,
    device: IDCompositionDesktopDevice,
    _target: IDCompositionTarget,
    _root: IDCompositionVisual2,
    /// The opaque disc first.
    discs: Vec<Disc>,
    /// Windows under the loupe shown by capture, and those export 147 refused.
    captures: Vec<Capture>,
    refused: Vec<HWND>,
    hwnd: HWND,
    side: i32,
}
/// One disc of the composed loupe: a rounded clip, an opacity applied to the disc as a whole
/// (so windows inside it never show through one another), and a shared thumbnail visual per
/// window.
struct Disc {
    /// The rounded clip's visual, holding `visual`.
    rounded: IDCompositionVisual2,
    /// The disc's windows, rendered as one surface by its effect (the opaque disc has none).
    visual: IDCompositionVisual2,
    clip: IDCompositionRectangleClip,
    /// Bottom first, as they are stacked under the disc.
    shared: Vec<Shared>,
    /// Shared thumbnails of windows that left the loupe lately, out of the tree, oldest first:
    /// a pointer going to and fro across a window's edge finds them again instead of creating
    /// them anew (2026-10-28: 20 ms a move for one window, five discs, against 3).
    spare: Vec<Shared>,
}
/// Windows whose thumbnails each disc keeps after they left the loupe.
const SPARE: usize = 16;
/// A window in a disc of the composed loupe. The slot stays when the window cannot be shown,
/// so slots and placements stay paired.
struct Shared {
    window: HWND,
    content: Content,
}
enum Content {
    /// A shared thumbnail visual (export 147) and its thumbnail, which lives as long as the
    /// visual: never unregistered by hand (a project using export 147 warns against it).
    Thumbnail(IDCompositionVisual2, isize),
    /// This disc's visual on the window's capture.
    Captured(IDCompositionVisual2),
    /// Shown neither way: left out.
    Missing,
}
/// An effect that shows what it is given at `opacity`, rendered as one surface first.
fn faded(
    device: &IDCompositionDesktopDevice,
    opacity: f32,
) -> Result<IDCompositionColorMatrixEffect> {
    let mut matrix = D2D_MATRIX_5X4_F::default();
    // Rows of 4: red, green and blue kept, alpha scaled, nothing added.
    for (at, value) in [(0, 1.), (5, 1.), (10, 1.), (15, opacity)] {
        unsafe { matrix.Anonymous.m[at] = value };
    }
    unsafe {
        let effect = device
            .cast::<IDCompositionDevice3>()?
            .CreateColorMatrixEffect()?;
        effect.SetMatrix(&matrix)?;
        Ok(effect)
    }
}
impl Composed {
    fn new(hwnd: HWND, create: CreateSharedThumbnailVisual) -> Result<Self> {
        unsafe {
            let (d3d, context) = broadcast::device()?;
            let device: IDCompositionDesktopDevice =
                DCompositionCreateDevice3(&d3d.cast::<IDXGIDevice>()?)?;
            let target = device.CreateTargetForHwnd(hwnd, true)?;
            let root = device.CreateVisual()?;
            // Whole source pixels become exact blocks; only the discs' edges are smoothed.
            root.SetBitmapInterpolationMode(
                DCOMPOSITION_BITMAP_INTERPOLATION_MODE_NEAREST_NEIGHBOR,
            )?;
            root.SetBorderMode(DCOMPOSITION_BORDER_MODE_SOFT)?;
            // The opacities hang on the number of discs only; their radii on the loupe's size.
            let mut discs = Vec::new();
            for ring in geometry::feathered(1., 1., FEATHER_STEPS) {
                let visual = device.CreateVisual()?;
                // The clip hangs on a visual of its own, around the disc's windows rendered
                // into one surface by an effect: clipped as they are, each window is cut apart,
                // and where the soft edge covers part of a pixel the windows hidden under the top
                // one show through it. An opacity layer does not prevent it; an effect does
                // (2026-10-30: a dark window under the probe's pattern left 460 pixels of 2 520
                // in the fading edge no blend of the enlargement and the screen; none since).
                let rounded = device.CreateVisual()?;
                if ring.opacity < 1. {
                    visual
                        .cast::<IDCompositionVisual3>()?
                        .SetEffect(&faded(&device, ring.opacity as f32)?)?;
                } else {
                    // Its edge lies where the faint discs under it already show 99 % of the
                    // enlargement: a hard edge there differs by a level or two, while a soft
                    // one blends neighbouring source pixels into a faint circle.
                    rounded.SetBorderMode(DCOMPOSITION_BORDER_MODE_HARD)?;
                }
                let clip = device.CreateRectangleClip()?;
                rounded.SetClip(&clip)?;
                rounded.AddVisual(&visual, false, None::<&IDCompositionVisual>)?;
                discs.push(Disc {
                    rounded,
                    visual,
                    clip,
                    shared: Vec::new(),
                    spare: Vec::new(),
                });
            }
            // ORDER: The faint discs from the narrowest up, the widest on top: where a disc's
            // soft edge strays from a pure blend, the fainter ones over it hide it (2026-10-28:
            // 9 stray pixels in 9 720; the other way, 617). The opaque disc above them all:
            // within the fading edge the loupe shows it alone, pixel for pixel, instead of a
            // pile of blends that rounds each pixel eight times.
            // PITFALL: With no reference visual, insertAbove = false stacks each child on top.
            for disc in discs[1..].iter().chain(&discs[..1]) {
                root.AddVisual(&disc.rounded, false, None::<&IDCompositionVisual>)?;
            }
            target.SetRoot(&root)?;
            device.Commit()?;
            Ok(Self {
                create,
                d3d,
                context,
                device,
                _target: target,
                _root: root,
                discs,
                captures: Vec::new(),
                refused: Vec::new(),
                hwnd,
                side: 0,
            })
        }
    }
    /// What disc `disc` shows of the placement's window: its shared thumbnail, or its capture
    /// when it is a shell window thumbnails get wrong or export 147 refused it.
    fn content(&mut self, disc: usize, placement: &geometry::Placement<HWND>) -> Result<Content> {
        let window = placement.window;
        if !self.refused.contains(&window) && !loupe_capture::shell_class(&class_name(window)) {
            match share(self.create, self.hwnd, &self.device, placement) {
                Ok(content) => return Ok(content),
                // 2026-10-28: the taskbar, every time, with E_INVALIDARG.
                Err(_) => self.refused.push(window),
            }
        }
        let capture = match self.captures.iter().position(|c| c.window == window) {
            Some(index) => &self.captures[index],
            None => {
                let capture = Capture::new(
                    window,
                    &self.d3d,
                    &self.context,
                    &self.device,
                    FEATHER_STEPS + 1,
                    HWND(NOTIFY.load(Ordering::SeqCst) as *mut _),
                )
                .map_err(|error| {
                    Error::new(
                        error.code(),
                        format!("capture of {}: {error}", class_name(window)),
                    )
                })?;
                self.captures.push(capture);
                &self.captures[self.captures.len() - 1]
            }
        };
        Ok(Content::Captured(capture.visuals[disc].clone()))
    }
    /// Copies the frames captured windows announced; whether any came.
    fn refresh(&mut self) -> Result<bool> {
        let mut any = false;
        for capture in &mut self.captures {
            any |= capture.refresh(&self.device, &self.context)?;
        }
        if any {
            unsafe { self.device.Commit()? };
        }
        Ok(any)
    }
    fn place(&mut self, frame: &geometry::Frame, under: &[(HWND, PixelRect)]) -> Result<()> {
        // The discs round the loupe: each window is one rectangle, cut to the source.
        let placed = geometry::stack(frame, &[frame.source], under);
        // Captures and refusals of windows no longer under the loupe are forgotten.
        self.captures
            .retain(|capture| placed.iter().any(|p| p.window == capture.window));
        self.refused
            .retain(|window| placed.iter().any(|p| p.window == *window));
        // The first window shown neither by export 147 nor by capture in this frame.
        let mut missing = None;
        let mut shown = 0;
        let mut discs = std::mem::take(&mut self.discs);
        let result = (|| {
            for (index, disc) in discs.iter_mut().enumerate() {
                let same = placed.len() == disc.shared.len()
                    && placed
                        .iter()
                        .zip(&disc.shared)
                        .all(|(placement, shared)| placement.window == shared.window);
                unsafe {
                    if !same {
                        // The windows still under the loupe keep their visuals, restacked; a
                        // window shown by neither means before is asked again.
                        let mut kept = std::mem::take(&mut disc.shared);
                        disc.visual.RemoveAllVisuals()?;
                        for placement in &placed {
                            let usable = |s: &Shared| {
                                s.window == placement.window
                                    && !matches!(s.content, Content::Missing)
                            };
                            let reused = match kept.iter().position(usable) {
                                Some(kept_at) => Some(kept.swap_remove(kept_at)),
                                None => disc
                                    .spare
                                    .iter()
                                    .position(usable)
                                    .map(|spare_at| disc.spare.remove(spare_at)),
                            };
                            let shared = match reused {
                                Some(shared) => shared,
                                None => Shared {
                                    window: placement.window,
                                    content: self.content(index, placement).unwrap_or_else(
                                        |error| {
                                            missing.get_or_insert(error);
                                            Content::Missing
                                        },
                                    ),
                                },
                            };
                            match &shared.content {
                                Content::Thumbnail(visual, _) | Content::Captured(visual) => disc
                                    .visual
                                    .AddVisual(visual, false, None::<&IDCompositionVisual>)?,
                                Content::Missing => {}
                            }
                            disc.shared.push(shared);
                        }
                        // Captures stop with their window's stay; thumbnails wait a while.
                        disc.spare.extend(
                            kept.into_iter()
                                .filter(|s| matches!(s.content, Content::Thumbnail(..))),
                        );
                        let over = disc.spare.len().saturating_sub(SPARE);
                        disc.spare.drain(..over);
                    }
                    for (placement, shared) in placed.iter().zip(&disc.shared) {
                        match &shared.content {
                            Content::Thumbnail(_, thumbnail) => {
                                match DwmUpdateThumbnailProperties(
                                    *thumbnail,
                                    &properties(placement),
                                ) {
                                    Ok(()) => shown += 1,
                                    Err(error) => {
                                        missing.get_or_insert(Error::new(
                                            error.code(),
                                            format!("{}: {error}", class_name(placement.window)),
                                        ));
                                    }
                                }
                            }
                            Content::Captured(_) => shown += 1,
                            Content::Missing => {}
                        }
                    }
                }
            }
            Ok::<_, Error>(())
        })();
        self.discs = discs;
        result?;
        for capture in &self.captures {
            capture.place(frame)?;
        }
        if let Some(error) = missing {
            if shown == 0 && !SHARED_WORKED.load(Ordering::SeqCst) {
                return Err(error);
            }
            left_out(&error);
        }
        if shown > 0 {
            SHARED_WORKED.store(true, Ordering::SeqCst);
        }
        let side = frame.window.width();
        unsafe {
            if side != self.side {
                let radius = side as f64 / 2.;
                let rings =
                    geometry::feathered(radius, geometry::feather(side) as f64, FEATHER_STEPS);
                for (disc, ring) in self.discs.iter().zip(rings) {
                    // A disc centred on the window, of the ring's radius.
                    let (near, far, r) = (
                        (radius - ring.radius) as f32,
                        (radius + ring.radius) as f32,
                        ring.radius as f32,
                    );
                    let clip = &disc.clip;
                    clip.SetLeft2(near)?;
                    clip.SetTop2(near)?;
                    clip.SetRight2(far)?;
                    clip.SetBottom2(far)?;
                    clip.SetTopLeftRadiusX2(r)?;
                    clip.SetTopLeftRadiusY2(r)?;
                    clip.SetTopRightRadiusX2(r)?;
                    clip.SetTopRightRadiusY2(r)?;
                    clip.SetBottomLeftRadiusX2(r)?;
                    clip.SetBottomLeftRadiusY2(r)?;
                    clip.SetBottomRightRadiusX2(r)?;
                    clip.SetBottomRightRadiusY2(r)?;
                }
                self.side = side;
            }
            self.device.Commit()
        }
    }
}
/// A new shared thumbnail visual of the placement's window, for the loupe's window `hwnd`.
fn share(
    create: CreateSharedThumbnailVisual,
    hwnd: HWND,
    device: &IDCompositionDesktopDevice,
    placement: &geometry::Placement<HWND>,
) -> Result<Content> {
    let mut properties = properties(placement);
    let mut visual = std::ptr::null_mut();
    let mut thumbnail = 0isize;
    unsafe {
        create(
            hwnd,
            placement.window,
            SHARED_THUMBNAIL_FLAGS,
            &mut properties,
            device.as_raw(),
            &mut visual,
            &mut thumbnail,
        )
        .ok()
        .map_err(|error| {
            Error::new(
                error.code(),
                format!("export 147, {}: {error}", class_name(placement.window)),
            )
        })?;
        if visual.is_null() {
            return Err(Error::new(E_POINTER, "export 147 gave no visual"));
        }
        Ok(Content::Thumbnail(
            IDCompositionVisual2::from_raw(visual),
            thumbnail,
        ))
    }
}
/// The thumbnail of a placement: part of the window, enlarged into the loupe.
fn properties(placement: &geometry::Placement<HWND>) -> DWM_THUMBNAIL_PROPERTIES {
    DWM_THUMBNAIL_PROPERTIES {
        dwFlags: DWM_TNP_RECTDESTINATION
            | DWM_TNP_RECTSOURCE
            | DWM_TNP_VISIBLE
            | DWM_TNP_OPACITY
            | DWM_TNP_SOURCECLIENTAREAONLY
            | DWM_TNP_ENABLE3D,
        rcDestination: rect(placement.destination),
        rcSource: rect(placement.source),
        opacity: 255,
        fVisible: true.into(),
        fSourceClientAreaOnly: false.into(),
    }
}

/// The banded loupe: classic thumbnails ignore the round region, so the enlargement is drawn in
/// round bands.
fn place_bands(
    hwnd: HWND,
    thumbnails: &mut Vec<(HWND, isize)>,
    frame: &geometry::Frame,
    under: &[(HWND, PixelRect)],
) {
    let bands = geometry::bands(frame, BAND_STEPS);
    let placed = geometry::stack(frame, &bands, under);
    let same = placed.len() == thumbnails.len()
        && placed
            .iter()
            .zip(thumbnails.iter())
            .all(|(placement, (window, _))| placement.window == *window);
    unsafe {
        if !same {
            unregister(thumbnails);
            // DWM draws the thumbnails in the order they were registered, the last on top.
            // A refused window keeps its slot, empty, so slots and placements stay paired.
            for placement in &placed {
                let id = DwmRegisterThumbnail(hwnd, placement.window).unwrap_or(0);
                thumbnails.push((placement.window, id));
            }
        }
        for (placement, (_, id)) in placed.iter().zip(thumbnails.iter()) {
            if *id == 0 {
                continue;
            }
            let properties = DWM_THUMBNAIL_PROPERTIES {
                dwFlags: DWM_TNP_RECTDESTINATION
                    | DWM_TNP_RECTSOURCE
                    | DWM_TNP_VISIBLE
                    | DWM_TNP_SOURCECLIENTAREAONLY,
                rcDestination: rect(placement.destination),
                rcSource: rect(placement.source),
                opacity: 255,
                fVisible: true.into(),
                fSourceClientAreaOnly: false.into(),
            };
            let _ = DwmUpdateThumbnailProperties(*id, &properties);
        }
    }
}
fn unregister(thumbnails: &mut Vec<(HWND, isize)>) {
    for (_, id) in thumbnails.drain(..).filter(|(_, id)| *id != 0) {
        unsafe {
            let _ = DwmUnregisterThumbnail(id);
        }
    }
}

/// A window's class, to say which one failed.
fn class_name(hwnd: HWND) -> String {
    let mut name = [0u16; 128];
    let length = unsafe { GetClassNameW(hwnd, &mut name) };
    String::from_utf16_lossy(&name[..length.max(0) as usize])
}

unsafe extern "system" fn loupe_proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    unsafe { DefWindowProcW(hwnd, msg, w, l) }
}

fn rect(r: PixelRect) -> RECT {
    RECT {
        left: r.left,
        top: r.top,
        right: r.right,
        bottom: r.bottom,
    }
}

extern "system" fn collect(hwnd: HWND, list: LPARAM) -> BOOL {
    unsafe { (*(list.0 as *mut Vec<HWND>)).push(hwnd) };
    TRUE
}

/// Every visible top-level window that meets `source`, top first, but those in `skip`, hidden
/// on another virtual desktop, minimised, or kept out of captures by their program.
fn windows_under(source: &PixelRect, skip: &[HWND]) -> Vec<(HWND, PixelRect)> {
    let mut all: Vec<HWND> = Vec::new();
    unsafe {
        let _ = EnumWindows(Some(collect), LPARAM(&mut all as *mut _ as isize));
    }
    all.into_iter()
        .filter(|hwnd| !skip.contains(hwnd))
        .filter_map(|hwnd| unsafe {
            if !IsWindowVisible(hwnd).as_bool() || IsIconic(hwnd).as_bool() {
                return None;
            }
            let mut cloaked = 0u32;
            let _ = DwmGetWindowAttribute(
                hwnd,
                DWMWA_CLOAKED,
                &mut cloaked as *mut u32 as _,
                std::mem::size_of::<u32>() as u32,
            );
            if cloaked != 0 {
                return None;
            }
            // A password manager or a protected video asked not to be captured: the loupe,
            // which captures do see, must not show it either.
            let mut affinity = 0u32;
            if GetWindowDisplayAffinity(hwnd, &mut affinity).is_err() || affinity != WDA_NONE.0 {
                return None;
            }
            let mut r = RECT::default();
            GetWindowRect(hwnd, &mut r).ok()?;
            let r = PixelRect {
                left: r.left,
                top: r.top,
                right: r.right,
                bottom: r.bottom,
            };
            r.intersection(source).map(|_| (hwnd, r))
        })
        .collect()
}

unsafe extern "system" fn window_event(
    _hook: HWINEVENTHOOK,
    _event: u32,
    hwnd: HWND,
    object: i32,
    child: i32,
    _thread: u32,
    _time: u32,
) {
    // Only whole windows count: the caret and the cursor move all the time.
    if object != OBJID_WINDOW.0 || child != CHILDID_SELF as i32 || hwnd.is_invalid() {
        return;
    }
    let owner = NOTIFY.load(Ordering::SeqCst);
    if owner == 0 || PENDING.swap(true, Ordering::SeqCst) {
        return;
    }
    unsafe {
        let _ = PostMessageW(
            Some(HWND(owner as *mut _)),
            WINDOWS_CHANGED,
            WPARAM(0),
            LPARAM(0),
        );
    }
}
/// The posted change was handled: the next event posts again.
pub(super) fn changes_seen() {
    PENDING.store(false, Ordering::SeqCst);
}
