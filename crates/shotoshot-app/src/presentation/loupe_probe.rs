//! ROLE: Checks the live loupe on the real desktop: a painted pattern under it, every pixel it
//! shows compared with the pattern enlarged, at the screen's edges, its round edge smooth and
//! with nothing around it, with windows it must leave out, and how soon a move reaches the
//! composed screen.
//! OWNER: `--loupe-diagnostic` (timed, run by check.ps1 once the machine is calm) and the
//! `real_desktop_loupe_*` tests.
//! INVARIANTS: No simulated input and no foreground taken: the pointer is never moved, the
//! loupe is placed where a pointer would be. A pattern found covered by another window is a
//! failure that says so, never a pass. Everything opened here is closed before returning.
use super::loupe::{BAND_STEPS, Loupe, Renderer, WINDOWS_CHANGED, fallback_reason};
use super::*;
use crate::{LiveSurface, image::capture_desktop, resources::OwnedWindow};
use shotoshot_core::loupe::{feather, frame};
use windows::Win32::Graphics::Dwm::DwmFlush;

/// The loupe's diameter in the probe, device pixels.
const DIAMETER: i32 = 240;
/// Shares of exact pixels, bit for bit, the loupe must reach: under the pattern, and across a
/// screen edge. The composed loupe needs no allowance: 99.6 % at worst, 100 % in the middle of
/// a screen (2026-10-28), where the spike feared one level off on 0.5 % of pixels.
const EXACT: f64 = 0.99;
const EXACT_AT_EDGES: f64 = 0.98;
/// Latency from a move of the loupe to the composed screen, beyond that of a plain window moved
/// the same way, in milliseconds: one frame at 60 Hz (the sixth pass measured 15 ms against a
/// floor of 12 by DXGI). Then a ceiling whatever the floor.
const LATENCY_OVER_FLOOR_MS: f64 = 17.;
const LATENCY_MAX_P95_MS: f64 = 100.;
const JUMPS: usize = 25;

/// A colour that changes with every pixel and says where it comes from: a shift of one pixel,
/// or a block of the wrong size, changes what is read.
fn pattern_bgra(x: i32, y: i32) -> [u8; 4] {
    let checker = if (x + y) & 1 == 0 { 0 } else { 0x80 };
    [
        (x as u8) ^ checker,
        (y as u8) ^ checker,
        ((x >> 8) as u8 & 0x0f) << 4 | ((y >> 8) as u8 & 0x0f),
        255,
    ]
}

/// A click-through, never activated, topmost layered window showing `pixel` for each of its
/// screen pixels, or one colour.
struct Sheet {
    window: OwnedWindow,
    _surface: LiveSurface,
}
impl Sheet {
    fn new(area: RECT, pixel: impl Fn(i32, i32) -> [u8; 4]) -> Result<Self> {
        Self::with(area, true, pixel)
    }
    /// A sheet in the topmost band, or at the top of the ordinary one: under the taskbar.
    fn with(area: RECT, topmost: bool, pixel: impl Fn(i32, i32) -> [u8; 4]) -> Result<Self> {
        let band = if topmost {
            WS_EX_TOPMOST
        } else {
            WINDOW_EX_STYLE(0)
        };
        unsafe {
            let instance = GetModuleHandleW(None)?;
            if RegisterClassW(&WNDCLASSW {
                lpfnWndProc: Some(sheet_proc),
                hInstance: instance.into(),
                lpszClassName: w!("Shotoshot.LoupeProbe"),
                ..Default::default()
            }) == 0
                && GetLastError() != ERROR_CLASS_ALREADY_EXISTS
            {
                return Err(Error::from_thread());
            }
            let (width, height) = (area.right - area.left, area.bottom - area.top);
            let window = OwnedWindow(CreateWindowExW(
                WS_EX_TOOLWINDOW | WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE | band,
                w!("Shotoshot.LoupeProbe"),
                w!("Shotoshot loupe probe"),
                WS_POPUP,
                area.left,
                area.top,
                width,
                height,
                None,
                None,
                Some(instance.into()),
                None,
            )?);
            let mut surface = LiveSurface::new(width as u32, height as u32)?;
            let (bgra, _) = surface.bits_mut().as_chunks_mut::<4>();
            for (index, bgra) in bgra.iter_mut().enumerate() {
                let (x, y) = (
                    area.left + (index % width as usize) as i32,
                    area.top + (index / width as usize) as i32,
                );
                *bgra = pixel(x, y);
            }
            surface.present(window.0)?;
            ShowWindow(window.0, SW_SHOWNOACTIVATE).ok().ok();
            Ok(Self {
                window,
                _surface: surface,
            })
        }
    }
    fn hwnd(&self) -> HWND {
        self.window.0
    }
}
unsafe extern "system" fn sheet_proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    unsafe { DefWindowProcW(hwnd, msg, w, l) }
}

/// A topmost window with a title bar and a sizing frame, never activated, its client area
/// painted with the pattern of the screen pixels it covers.
fn framed_window(area: RECT) -> Result<OwnedWindow> {
    unsafe {
        let instance = GetModuleHandleW(None)?;
        if RegisterClassW(&WNDCLASSW {
            lpfnWndProc: Some(framed_proc),
            hInstance: instance.into(),
            lpszClassName: w!("Shotoshot.LoupeFramed"),
            ..Default::default()
        }) == 0
            && GetLastError() != ERROR_CLASS_ALREADY_EXISTS
        {
            return Err(Error::from_thread());
        }
        let window = OwnedWindow(CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TOPMOST,
            w!("Shotoshot.LoupeFramed"),
            w!("Shotoshot loupe framed probe"),
            WS_OVERLAPPEDWINDOW,
            area.left,
            area.top,
            area.right - area.left,
            area.bottom - area.top,
            None,
            None,
            Some(instance.into()),
            None,
        )?);
        let _ = ShowWindow(window.0, SW_SHOWNOACTIVATE);
        let _ = UpdateWindow(window.0);
        Ok(window)
    }
}
unsafe extern "system" fn framed_proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    unsafe {
        if msg != WM_PAINT {
            return DefWindowProcW(hwnd, msg, w, l);
        }
        let mut paint = PAINTSTRUCT::default();
        let dc = BeginPaint(hwnd, &mut paint);
        let mut client = RECT::default();
        let _ = GetClientRect(hwnd, &mut client);
        let mut origin = POINT::default();
        let _ = ClientToScreen(hwnd, &mut origin);
        let (width, height) = (client.right, client.bottom);
        let mut bits = Vec::with_capacity((width * height) as usize * 4);
        for y in 0..height {
            for x in 0..width {
                bits.extend_from_slice(&pattern_bgra(origin.x + x, origin.y + y));
            }
        }
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: -height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        StretchDIBits(
            dc,
            0,
            0,
            width,
            height,
            0,
            0,
            width,
            height,
            Some(bits.as_ptr().cast()),
            &info,
            DIB_RGB_COLORS,
            SRCCOPY,
        );
        let _ = EndPaint(hwnd, &paint);
        LRESULT(0)
    }
}

/// Lets DWM compose what was just changed, and the next frame after it.
fn composed() {
    unsafe {
        let _ = DwmFlush();
        let _ = DwmFlush();
    }
}
fn bgr(image: &crate::Image, x: i32, y: i32) -> Option<[u8; 3]> {
    let (dx, dy) = (x - image.left, y - image.top);
    if dx < 0 || dy < 0 || dx >= image.width as i32 || dy >= image.height as i32 {
        return None;
    }
    let at = (dy as usize * image.width as usize + dx as usize) * 4;
    Some([image.pixels[at], image.pixels[at + 1], image.pixels[at + 2]])
}

/// Where, around its circle, each renderer's loupe may differ from both the enlargement and
/// the screen under it, in window pixels: inside, then outside the circle.
fn margins(renderer: Renderer) -> (f64, f64) {
    match renderer {
        // The fading edge, 4 pixels, and a pixel of the last disc's soft border.
        Renderer::Composed => (feather(DIAMETER) as f64 + 2., 1.5),
        // Steps of the bands, half inside and half out, and a pixel of rounding.
        Renderer::Banded => (BAND_STEPS as f64 / 2. + 1., BAND_STEPS as f64 / 2. + 1.),
    }
}

/// How the loupe at `at` agrees with `before`, the screen without it, enlarged.
#[derive(Debug)]
pub(crate) struct Look {
    pub at: (i32, i32),
    pub zoom: i32,
    pub exact: usize,
    pub total: usize,
    /// The largest difference, in levels on one channel, between a pixel and the enlargement.
    pub worst: u8,
    /// Pixels whose source lies beyond the desktop and that show something else than what the
    /// loupe shows there: the screen under it (composed) or its background (banded).
    pub stray: usize,
    /// Pixels outside the round loupe, in the corners of its window, that do not show the
    /// screen as it is: the loupe must be round, with nothing around it.
    pub corners: usize,
}
impl Look {
    pub fn share(&self) -> f64 {
        self.exact as f64 / self.total.max(1) as f64
    }
}
fn look(
    before: &crate::Image,
    after: &crate::Image,
    at: POINT,
    zoom: i32,
    renderer: Renderer,
) -> Look {
    let frame = frame(at.x, at.y, DIAMETER, zoom);
    let window = frame.window;
    let (inner, outer) = margins(renderer);
    let radius = window.width() as f64 / 2. - inner;
    let centre = (
        window.left as f64 + window.width() as f64 / 2.,
        window.top as f64 + window.height() as f64 / 2.,
    );
    let mut result = Look {
        at: (at.x, at.y),
        zoom,
        exact: 0,
        total: 0,
        worst: 0,
        stray: 0,
        corners: 0,
    };
    let outside = window.width() as f64 / 2. + outer;
    for y in window.top..window.bottom {
        for x in window.left..window.right {
            let (dx, dy) = (x as f64 + 0.5 - centre.0, y as f64 + 0.5 - centre.1);
            if dx * dx + dy * dy > outside * outside {
                if bgr(after, x, y) != bgr(before, x, y) {
                    result.corners += 1;
                }
                continue;
            }
            if dx * dx + dy * dy > radius * radius {
                continue;
            }
            let Some(shown) = bgr(after, x, y) else {
                continue;
            };
            let source = (
                frame.source.left + (x - window.left) / zoom,
                frame.source.top + (y - window.top) / zoom,
            );
            match bgr(before, source.0, source.1) {
                Some(expected) => {
                    result.total += 1;
                    result.exact += usize::from(expected == shown);
                    let worst = (0..3).map(|i| expected[i].abs_diff(shown[i])).max();
                    result.worst = result.worst.max(worst.unwrap_or(0));
                }
                None => {
                    let beyond = match renderer {
                        Renderer::Composed => bgr(before, x, y),
                        Renderer::Banded => Some([0x20, 0x20, 0x20]),
                    };
                    if beyond != Some(shown) {
                        result.stray += 1;
                    }
                }
            }
        }
    }
    result
}

/// The loupe's fading edge, read along rays from its centre.
#[derive(Debug, Default)]
pub(crate) struct Feather {
    /// Pixels read in the fading band, and those that are no blend of the enlargement and the
    /// screen under the loupe (more than 8 levels off the nearest blend).
    pub samples: usize,
    pub unblended: usize,
    /// How much of the enlargement shows, on average, just inside the band and just outside
    /// the circle: about 1, then about 0.
    pub inside: f64,
    pub outside: f64,
    /// Along the average profile: steps where the enlargement comes back going out (by more
    /// than 0.02), and the largest fall from one pixel to the next: a visible line or step.
    pub rises: usize,
    pub steepest: f64,
    /// The average share of the enlargement, and the largest distance to a blend, pixel by
    /// pixel from just inside the band outwards.
    pub profile: Vec<(f64, f64)>,
}
/// How much of `content` shows in `shown` over `under`, when the two differ enough to tell,
/// and how far `shown` lies from that blend, in levels.
fn blend(shown: [u8; 3], content: [u8; 3], under: [u8; 3]) -> Option<(f64, f64)> {
    let channels = || (0..3).map(|i| (shown[i] as f64, content[i] as f64, under[i] as f64));
    let spread: f64 = channels().map(|(_, c, u)| (c - u) * (c - u)).sum();
    if spread < 40. * 40. {
        return None;
    }
    let share = channels().map(|(s, c, u)| (s - u) * (c - u)).sum::<f64>() / spread;
    let off = channels()
        .map(|(s, c, u)| (s - (u + share * (c - u))).abs())
        .fold(0., f64::max);
    Some((share, off))
}
/// Reads the fading edge of the loupe at `at`, at zoom 2: along a ray a degree, each pixel from
/// just inside the band to just outside the circle.
pub(crate) fn feathering(before: &crate::Image, after: &crate::Image, at: POINT) -> Feather {
    let frame = frame(at.x, at.y, DIAMETER, 2);
    let window = frame.window;
    let centre = (
        (window.left + window.right) as f64 / 2.,
        (window.top + window.bottom) as f64 / 2.,
    );
    let radius = window.width() as f64 / 2.;
    let band = feather(window.width()) as f64;
    let mut result = Feather::default();
    let (mut inside, mut outside) = (Vec::new(), Vec::new());
    let steps = band as usize + 3;
    let mut sums = vec![(0f64, 0f64, 0usize); steps];
    for degree in 0..360 {
        let angle = (degree as f64).to_radians();
        let mut distance = radius - band - 1.;
        while distance <= radius + 1. {
            let (x, y) = (
                (centre.0 + distance * angle.cos()).floor() as i32,
                (centre.1 + distance * angle.sin()).floor() as i32,
            );
            let content = bgr(
                before,
                frame.source.left + (x - window.left) / 2,
                frame.source.top + (y - window.top) / 2,
            );
            if let (Some(shown), Some(content), Some(under)) =
                (bgr(after, x, y), content, bgr(before, x, y))
                && let Some((share, off)) = blend(shown, content, under)
            {
                result.samples += 1;
                result.unblended += usize::from(off > 8.);
                let step = &mut sums[(distance - (radius - band - 1.)) as usize];
                *step = (step.0 + share, step.1.max(off), step.2 + 1);
                if distance < radius - band {
                    inside.push(share);
                } else if distance > radius + 0.5 {
                    outside.push(share);
                }
            }
            distance += 1.;
        }
    }
    let mean = |values: &[f64]| values.iter().sum::<f64>() / values.len().max(1) as f64;
    result.inside = mean(&inside);
    result.outside = mean(&outside);
    result.profile = sums
        .iter()
        .map(|(share, off, count)| {
            (
                (share / (*count).max(1) as f64 * 100.).round() / 100.,
                off.round(),
            )
        })
        .collect();
    for pair in result.profile.windows(2) {
        let fall = pair[0].0 - pair[1].0;
        result.rises += usize::from(fall < -0.02);
        result.steepest = result.steepest.max(fall);
    }
    result
}
/// An 8x enlargement, pixel for pixel, of the 40 x 40 pixels of `image` from `(left, top)`.
fn zoomed(image: &crate::Image, left: i32, top: i32) -> crate::Image {
    const SIDE: i32 = 40;
    const ZOOM: i32 = 8;
    let mut pixels = Vec::with_capacity((SIDE * ZOOM * SIDE * ZOOM * 4) as usize);
    for y in 0..SIDE * ZOOM {
        for x in 0..SIDE * ZOOM {
            let [b, g, r] = bgr(image, left + x / ZOOM, top + y / ZOOM).unwrap_or([0, 0, 0]);
            pixels.extend_from_slice(&[b, g, r, 255]);
        }
    }
    crate::Image {
        width: (SIDE * ZOOM) as u32,
        height: (SIDE * ZOOM) as u32,
        pixels,
        left: 0,
        top: 0,
    }
}

/// What one probe run found.
#[derive(Default)]
pub(crate) struct Report {
    /// How the loupe was drawn at the end of the run, and why not composed if it was not.
    pub renderer: Option<Renderer>,
    pub fallback: Option<String>,
    pub looks: Vec<Look>,
    /// The fading edge of the loupe over the pattern, and its upper left part enlarged 8x.
    pub feather: Feather,
    pub edge_zoom: Option<crate::Image>,
    /// Pixels of a window the loupe must leave out that it showed anyway.
    pub excluded_seen: usize,
    pub protected_seen: usize,
    pub latencies_ms: Vec<f64>,
    /// Time spent placing the loupe on each of those moves, in this process.
    pub update_ms: Vec<f64>,
    pub floor_ms: Vec<f64>,
    pub idle_cpu_percent: f64,
    /// The loupe window or its hooks outlived it.
    pub left_behind: bool,
}

fn fail(message: String) -> Error {
    Error::new(E_FAIL, message)
}

/// Places the pattern over the whole desktop, then the loupe at the centre of each screen, at
/// its corners and across the edge between two screens, at every zoom; then its edge, and
/// windows it must leave out. `timed` adds the latency and the idle cost. `renderer` is the
/// one asked for: the banded one, or the composed one where this Windows offers it.
pub(crate) fn probe(timed: bool, renderer: Renderer) -> Result<Report> {
    // The loupe captures windows through WinRT, which needs an apartment on this thread.
    let _com = crate::ComApartment::new()?;
    // One screen only, away from the one Mickaël uses (`probe_screen`).
    let m = crate::image::probe_screen();
    let pattern = Sheet::new(m, pattern_bgra)?;
    composed();
    let before = capture_desktop()?;
    // The pattern must be what the screen shows: another window over it voids the run.
    let probe_at = |x: i32, y: i32| bgr(&before, x, y) == bgr_of(pattern_bgra(x, y));
    let middle = POINT {
        x: (m.left + m.right) / 2,
        y: (m.top + m.bottom) / 2,
    };
    let mut points = vec![(middle, EXACT)];
    // Its right edge, where no screen lies beyond on this desk: a loupe on the left one would
    // spill onto the screen in use and read what changes there.
    for corner in [
        POINT {
            x: m.right - 1,
            y: m.top,
        },
        POINT {
            x: m.right - 1,
            y: m.bottom - 1,
        },
        POINT {
            x: m.right - 1,
            y: middle.y,
        },
    ] {
        points.push((corner, EXACT_AT_EDGES));
    }
    for (at, _) in &points {
        if !probe_at(at.x, at.y) {
            return Err(fail(format!(
                "the pattern is covered at {:?}: close what lies over it and run again",
                (at.x, at.y)
            )));
        }
    }
    let mut report = Report::default();
    let mut loupe = match renderer {
        Renderer::Composed => Loupe::new(pattern.hwnd())?,
        #[cfg(test)]
        Renderer::Banded => Loupe::banded(pattern.hwnd())?,
        #[cfg(not(test))]
        Renderer::Banded => return Err(fail("the banded loupe is probed by its test".into())),
    };
    // What a renderer that fell back mid-run drew is judged by the rules of what it became.
    let drawn = |loupe: &Loupe| loupe.renderer();
    for zoom in [2, 3, 4] {
        for (at, _) in &points {
            loupe.update(*at, DIAMETER, zoom, None, &[])?;
            composed();
            let after = capture_desktop()?;
            report
                .looks
                .push(look(&before, &after, *at, zoom, drawn(&loupe)));
        }
    }
    // A window to leave out and one that refuses capture, both under the loupe.
    let at = middle;
    let square = |dx: i32| RECT {
        left: at.x + dx,
        top: at.y - 20,
        right: at.x + dx + 30,
        bottom: at.y + 20,
    };
    let excluded = Sheet::new(square(-40), |_, _| [255, 0, 255, 255])?;
    let protected = Sheet::new(square(10), |_, _| [0, 255, 0, 255])?;
    unsafe {
        SetWindowDisplayAffinity(protected.hwnd(), WDA_EXCLUDEFROMCAPTURE)?;
    }
    loupe.update(at, DIAMETER, 2, None, &[excluded.hwnd()])?;
    composed();
    let after = capture_desktop()?;
    // What lies under both is the pattern: the loupe shows it, not a hole where they are.
    report
        .looks
        .push(look(&before, &after, at, 2, drawn(&loupe)));
    let inside = frame(at.x, at.y, DIAMETER, 2).window;
    for y in inside.top..inside.bottom {
        for x in inside.left..inside.right {
            match bgr(&after, x, y) {
                Some([255, 0, 255]) => report.excluded_seen += 1,
                Some([0, 255, 0]) => report.protected_seen += 1,
                _ => {}
            }
        }
    }
    drop(excluded);
    drop(protected);
    // The edge over the pattern, away from the screen's edges, once the windows left out are
    // gone: a loupe that shows the same is not restacked over windows opened since.
    loupe.update(at, DIAMETER, 2, None, &[])?;
    composed();
    let after = capture_desktop()?;
    report.feather = feathering(&before, &after, at);
    let window = frame(at.x, at.y, DIAMETER, 2).window;
    // Where the middle of the fading edge crosses the window's diagonal, up and left of the
    // centre.
    let middle = (window.width() - feather(window.width())) as f64 / 2.;
    let corner = (window.width() as f64 / 2. - middle * std::f64::consts::FRAC_1_SQRT_2) as i32;
    report.edge_zoom = Some(zoomed(
        &after,
        window.left + corner - 20,
        window.top + corner - 20,
    ));
    // An ordinary window with a frame: Windows 11 adds invisible borders around it, which the
    // thumbnail's coordinates must count as the window rectangle does. Read over its title bar
    // and client area, away from its edges: DWM draws the shadow, the rounded corners and the
    // thin border outside thumbnails, so the loupe shows a window without them (2026-10-28: 6 to
    // 11 % of the pixels of a loupe over a corner).
    let framed = framed_window(RECT {
        left: at.x - 200,
        top: at.y - 150,
        right: at.x + 200,
        bottom: at.y + 150,
    })?;
    composed();
    let with_frame = capture_desktop()?;
    for corner in [
        POINT {
            x: at.x,
            y: at.y - 150 + 70,
        },
        POINT {
            x: at.x - 110,
            y: at.y + 60,
        },
    ] {
        loupe.update(corner, DIAMETER, 2, None, &[])?;
        composed();
        let after = capture_desktop()?;
        report
            .looks
            .push(look(&with_frame, &after, corner, 2, drawn(&loupe)));
    }
    drop(framed);
    if timed {
        (report.latencies_ms, report.update_ms) = latencies(&mut loupe, &before, m)?;
        report.floor_ms = floor(m)?;
        report.idle_cpu_percent = idle_cpu(&mut loupe, at)?;
    }
    report.renderer = Some(loupe.renderer());
    report.fallback = fallback_reason().map(String::from);
    drop(loupe);
    // Only this process's: the installed resident, or another checkout's tests running at the
    // same time, may show a loupe of their own (2026-10-30: one run in ten).
    report.left_behind = unsafe {
        let mut after = None;
        loop {
            match FindWindowExW(None, after, w!("Shotoshot.Loupe"), PCWSTR::null()) {
                Ok(hwnd) if !hwnd.is_invalid() => {
                    let mut process = 0;
                    GetWindowThreadProcessId(hwnd, Some(&mut process));
                    if process == std::process::id() {
                        break true;
                    }
                    after = Some(hwnd);
                }
                _ => break false,
            }
        }
    };
    drop(pattern);
    Ok(report)
}
// The taskbar is compared bit for bit: over one colour (`BACKDROP`), the translucent taskbar
// reads the same through the loupe as on the screen (2026-10-28: 100 % of 16 050 pixels, three
// runs). Over the user's windows it read up to 4 levels off and the timed runs failed.
/// The colour behind the taskbar while it is probed: a mid grey-blue, far from the taskbar's
/// own dark or light tint, so its blur stays one colour and a missing taskbar shows.
const BACKDROP: [u8; 4] = [160, 110, 70, 255];

/// What the loupe showed over the taskbar of the probe's screen, which DWM thumbnails get
/// wrong.
pub(crate) struct Taskbar {
    /// Pixels inside the fading edge whose source lies in the taskbar.
    pub look: Look,
    /// Windows the loupe showed by capture there.
    pub captured: usize,
    pub latencies_ms: Vec<f64>,
    pub update_ms: Vec<f64>,
}
/// Places the loupe over the taskbar of the probe's screen (`probe_screen`), on the real screen
/// (no pattern: it would cover the taskbar), and compares what it shows of the taskbar with the
/// screen before. `timed` adds the latency of moves along it. None when that screen has no
/// taskbar at its bottom, or it hides itself.
pub(crate) fn taskbar(timed: bool) -> Result<Option<Taskbar>> {
    let _com = crate::ComApartment::new()?;
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    let probed = crate::image::probe_screen();
    unsafe {
        GetMonitorInfoW(
            MonitorFromPoint(
                POINT {
                    x: (probed.left + probed.right) / 2,
                    y: (probed.top + probed.bottom) / 2,
                },
                MONITOR_DEFAULTTONEAREST,
            ),
            &mut info,
        )
        .ok()?;
    }
    let (screen, work) = (info.rcMonitor, info.rcWork);
    if work.bottom >= screen.bottom {
        return Ok(None);
    }
    let strip = RECT {
        top: work.bottom,
        ..screen
    };
    let inside =
        |r: &RECT, x: i32, y: i32| (r.left..r.right).contains(&x) && (r.top..r.bottom).contains(&y);
    // One colour behind the translucent taskbar, over whatever the user has open: its blur is
    // that colour, on the screen as in the loupe (2026-10-28: windows passing under the taskbar
    // made every timed run fail). Under the topmost taskbar, above every ordinary window.
    let backdrop = Sheet::with(screen, false, |_, _| BACKDROP)?;
    unsafe {
        SetWindowPos(
            backdrop.hwnd(),
            Some(HWND_TOP),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        )?;
    }
    composed();
    let before = capture_desktop()?;
    // Only hears about frames; never shown.
    let owner = OwnedWindow(unsafe {
        CreateWindowExW(
            WS_EX_TOOLWINDOW,
            w!("STATIC"),
            w!("Loupe taskbar probe"),
            WS_POPUP,
            -32000,
            -32000,
            1,
            1,
            None,
            None,
            None,
            None,
        )?
    });
    let mut loupe = Loupe::new(owner.0)?;
    let y = (strip.top + strip.bottom) / 2;
    let at = POINT {
        x: (screen.left + screen.right) / 2,
        y,
    };
    loupe.update(at, DIAMETER, 2, None, &[])?;
    loupe.frames();
    composed();
    let after = capture_desktop()?;
    let frame = frame(at.x, at.y, DIAMETER, 2);
    let window = frame.window;
    let (margin, _) = margins(loupe.renderer());
    let radius = window.width() as f64 / 2. - margin;
    let centre = (
        window.left as f64 + window.width() as f64 / 2.,
        window.top as f64 + window.height() as f64 / 2.,
    );
    let mut look = Look {
        at: (at.x, at.y),
        zoom: 2,
        exact: 0,
        total: 0,
        worst: 0,
        stray: 0,
        corners: 0,
    };
    for y in window.top..window.bottom {
        for x in window.left..window.right {
            let (dx, dy) = (x as f64 + 0.5 - centre.0, y as f64 + 0.5 - centre.1);
            let source = (
                frame.source.left + (x - window.left) / 2,
                frame.source.top + (y - window.top) / 2,
            );
            if dx * dx + dy * dy > radius * radius
                || !inside(&screen, x, y)
                || !inside(&strip, source.0, source.1)
            {
                continue;
            }
            if let (Some(shown), Some(expected)) =
                (bgr(&after, x, y), bgr(&before, source.0, source.1))
            {
                let worst = (0..3)
                    .map(|i| expected[i].abs_diff(shown[i]))
                    .max()
                    .unwrap_or(0);
                look.total += 1;
                look.exact += usize::from(worst == 0);
                look.worst = look.worst.max(worst);
            }
        }
    }
    let captured = loupe.captured();
    let (mut latencies_ms, mut update_ms) = (Vec::new(), Vec::new());
    if timed {
        // Two places among the pinned apps of a centred taskbar, away from the clock, whose
        // minute could turn during the run.
        let places = [at, POINT { x: at.x + 120, y }];
        latencies_ms = timed_jumps(
            places,
            0,
            |at| {
                let start = std::time::Instant::now();
                let placed = loupe.update(at, DIAMETER, 2, None, &[]);
                update_ms.push(start.elapsed().as_secs_f64() * 1000.);
                placed
            },
            |at, x, y| {
                let frame = shotoshot_core::loupe::frame(at.x, at.y, DIAMETER, 2);
                let source = (
                    frame.source.left + (x - frame.window.left) / 2,
                    frame.source.top + (y - frame.window.top) / 2,
                );
                (inside(&screen, x, y) && inside(&strip, source.0, source.1))
                    .then(|| bgr(&before, source.0, source.1))
                    .flatten()
            },
        )?;
    }
    drop(loupe);
    drop(backdrop);
    Ok(Some(Taskbar {
        look,
        captured,
        latencies_ms,
        update_ms,
    }))
}
/// Every reason what the loupe showed of the taskbar fails, measured against `floor_ms`, a
/// plain window moved the same way.
pub(crate) fn taskbar_verdict(taskbar: &Taskbar, floor_ms: Option<&[f64]>) -> Vec<String> {
    let mut faults = Vec::new();
    let look = &taskbar.look;
    if look.total < 1000 || look.share() < EXACT_AT_EDGES {
        faults.push(format!(
            "over the taskbar: {:.2} % exact of {} pixels (needs {:.0} %), at worst {} levels off",
            100. * look.share(),
            look.total,
            100. * EXACT_AT_EDGES,
            look.worst
        ));
    }
    if let Some(floor_ms) = floor_ms {
        let (p50, p95, count) = spread(&taskbar.latencies_ms);
        let (floor50, floor95, _) = spread(floor_ms);
        if count < JUMPS
            || p50 > floor50 + LATENCY_OVER_FLOOR_MS
            || p95 > floor95 + LATENCY_OVER_FLOOR_MS
            || p95 > LATENCY_MAX_P95_MS
        {
            faults.push(format!(
                "over the taskbar, latency p50 {p50:.1} ms, p95 {p95:.1} ms over {count} moves, floor {floor50:.1} and {floor95:.1} ms"
            ));
        }
    }
    faults
}
fn bgr_of(bgra: [u8; 4]) -> Option<[u8; 3]> {
    Some([bgra[0], bgra[1], bgra[2]])
}

/// Two places on the probe's screen it jumps between.
fn places(m: RECT) -> [POINT; 2] {
    [
        POINT {
            x: m.left + (m.right - m.left) / 3,
            y: (m.top + m.bottom) / 2,
        },
        POINT {
            x: m.left + 2 * (m.right - m.left) / 3,
            y: (m.top + m.bottom) / 2,
        },
    ]
}
/// Pixels around the centre of a place, read on the composed screen.
const SAMPLES: [(i32, i32); 3] = [(-31, -17), (23, 29), (41, -37)];

/// Times, in milliseconds, from each `jump` to the composed screen showing what `expected`
/// says the samples around the new place must read, within `levels` on each channel.
fn timed_jumps(
    places: [POINT; 2],
    levels: u8,
    mut jump: impl FnMut(POINT) -> Result<()>,
    expected: impl Fn(POINT, i32, i32) -> Option<[u8; 3]>,
) -> Result<Vec<f64>> {
    let mut times = Vec::new();
    unsafe {
        let screen = GetDC(None);
        let result = (|| {
            for index in 0..JUMPS + 1 {
                let at = places[index % 2];
                let samples: Vec<(i32, i32, [u8; 3])> = SAMPLES
                    .iter()
                    .filter_map(|(dx, dy)| {
                        let (x, y) = (at.x + dx, at.y + dy);
                        expected(at, x, y).map(|c| (x, y, c))
                    })
                    .collect();
                let start = std::time::Instant::now();
                jump(at)?;
                loop {
                    let arrived = samples.iter().all(|(x, y, c)| {
                        let pixel = GetPixel(screen, *x, *y).0;
                        let read = [(pixel >> 16) as u8, (pixel >> 8) as u8, pixel as u8];
                        (0..3).all(|i| read[i].abs_diff(c[i]) <= levels)
                    });
                    let elapsed = start.elapsed().as_secs_f64() * 1000.;
                    if arrived {
                        // The first jump only places what moves.
                        if index > 0 {
                            times.push(elapsed);
                        }
                        break;
                    }
                    if elapsed > 500. {
                        return Err(fail(format!("never reached {:?}", (at.x, at.y))));
                    }
                }
                composed();
            }
            Ok(())
        })();
        ReleaseDC(None, screen);
        result?;
    }
    Ok(times)
}

/// Moves the loupe between two places and times how soon the composed screen shows it there.
/// Also returns how long each move took to place, in milliseconds.
fn latencies(
    loupe: &mut Loupe,
    before: &crate::Image,
    screen: RECT,
) -> Result<(Vec<f64>, Vec<f64>)> {
    let mut updates = Vec::new();
    let times = timed_jumps(
        places(screen),
        0,
        |at| {
            let start = std::time::Instant::now();
            let placed = loupe.update(at, DIAMETER, 2, None, &[]);
            updates.push(start.elapsed().as_secs_f64() * 1000.);
            placed
        },
        |at, x, y| {
            let frame = frame(at.x, at.y, DIAMETER, 2);
            bgr(
                before,
                frame.source.left + (x - frame.window.left) / 2,
                frame.source.top + (y - frame.window.top) / 2,
            )
        },
    )?;
    Ok((times, updates))
}
/// The same jumps made by a plain window of one colour: the floor of the reading method, as
/// GDI sees the composed screen a frame or two after it changed.
fn floor(screen: RECT) -> Result<Vec<f64>> {
    const COLOR: [u8; 4] = [12, 34, 56, 255];
    let half = DIAMETER / 2;
    let sheet = Sheet::new(
        RECT {
            left: screen.left,
            top: screen.top,
            right: screen.left + DIAMETER,
            bottom: screen.top + DIAMETER,
        },
        |_, _| COLOR,
    )?;
    timed_jumps(
        places(screen),
        0,
        |at| unsafe {
            SetWindowPos(
                sheet.hwnd(),
                Some(HWND_TOPMOST),
                at.x - half,
                at.y - half,
                0,
                0,
                SWP_NOSIZE | SWP_NOACTIVATE,
            )
        },
        |_, _, _| Some([COLOR[0], COLOR[1], COLOR[2]]),
    )
}

/// The process's CPU while the loupe stays on screen and nothing moves, in percent of a core.
fn idle_cpu(loupe: &mut Loupe, at: POINT) -> Result<f64> {
    loupe.update(at, DIAMETER, 2, None, &[])?;
    composed();
    let used = || unsafe {
        let (mut created, mut exited, mut kernel, mut user) = Default::default();
        let _ = windows::Win32::System::Threading::GetProcessTimes(
            windows::Win32::System::Threading::GetCurrentProcess(),
            &mut created,
            &mut exited,
            &mut kernel,
            &mut user,
        );
        let ticks = |t: FILETIME| ((t.dwHighDateTime as u64) << 32) | t.dwLowDateTime as u64;
        ticks(kernel) + ticks(user)
    };
    let start = std::time::Instant::now();
    let first = used();
    // The loupe's events still arrive meanwhile: pump them as the resident would.
    // Windows counts CPU time in ticks of about 15.6 ms: over 4 s one tick is 0.4 %, so two
    // stray ticks of other work no longer read as a loupe that costs 1.56 % (2026-10-28).
    while start.elapsed() < std::time::Duration::from_secs(4) {
        unsafe {
            let mut msg = MSG::default();
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                if msg.message == WINDOWS_CHANGED {
                    super::loupe::changes_seen();
                }
                if msg.message == super::loupe_capture::FRAMES {
                    loupe.frames();
                }
                DispatchMessageW(&msg);
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let busy = (used() - first) as f64 / 10_000.;
    Ok(100. * busy / (start.elapsed().as_secs_f64() * 1000.))
}

/// Median, 95th percentile and count.
fn spread(times: &[f64]) -> (f64, f64, usize) {
    let mut sorted = times.to_vec();
    sorted.sort_by(f64::total_cmp);
    (
        percentile(&sorted, 0.5),
        percentile(&sorted, 0.95),
        sorted.len(),
    )
}
fn percentile(sorted: &[f64], share: f64) -> f64 {
    if sorted.is_empty() {
        return f64::INFINITY;
    }
    sorted[((sorted.len() - 1) as f64 * share).round() as usize]
}

/// Every reason the report fails the protocol; empty when it passes.
pub(crate) fn verdict(report: &Report, timed: bool) -> Vec<String> {
    let mut faults = Vec::new();
    let renderer = report.renderer.unwrap_or(Renderer::Banded);
    let (inner, _) = margins(renderer);
    for look in &report.looks {
        let edge = (look.total as f64)
            < 0.9 * std::f64::consts::PI * (DIAMETER as f64 / 2. - inner).powi(2);
        let needed = if edge { EXACT_AT_EDGES } else { EXACT };
        if look.share() < needed || look.stray > 0 || look.corners > 0 {
            faults.push(format!(
                "loupe at {:?}, zoom {}: {:.2} % exact of {} pixels (needs {:.0} %), {} stray, {} in the corners",
                look.at,
                look.zoom,
                100. * look.share(),
                look.total,
                100. * needed,
                look.stray,
                look.corners
            ));
        }
    }
    // The composed loupe fades into the screen: blends only (a few pixels of a disc's edge may
    // stray), from the enlargement to the screen, never back, with no fall steep enough to draw
    // a line: a smooth fall over 4 pixels drops about 0.4 a pixel at its steepest, a step of
    // 1 would be a line (Mickaël wants 3 or 4 pixels, 2026-10-28). The banded one's steps are
    // bounded by its margins, checked above.
    let f = &report.feather;
    if renderer == Renderer::Composed
        && (f.samples < 1000
            // A narrow edge counts more pixels where two discs' soft borders meet. Measured on
            // 2 520 samples (2026-10-28, 4 px, five discs): 103 to 132 over eleven runs, calm
            // or under check.ps1's load; the limit is the worst run with 40 % to spare, 7.5 %.
            // A missing fade or a hard line reads far more (617 for misordered discs).
            || f.unblended * 40 > f.samples * 3
            || f.inside < 0.95
            || f.outside > 0.05
            || f.rises > 0
            || f.steepest > 0.6)
    {
        faults.push(format!("the fading edge: {f:?}"));
    }
    if report.excluded_seen > 0 {
        faults.push(format!(
            "{} pixels of a left-out window shown",
            report.excluded_seen
        ));
    }
    if report.protected_seen > 0 {
        faults.push(format!(
            "{} pixels of a window kept from captures shown",
            report.protected_seen
        ));
    }
    if report.left_behind {
        faults.push("the loupe window outlived its loupe".into());
    }
    if timed {
        let (p50, p95, count) = spread(&report.latencies_ms);
        let (floor50, floor95, _) = spread(&report.floor_ms);
        if count < JUMPS
            || p50 > floor50 + LATENCY_OVER_FLOOR_MS
            || p95 > floor95 + LATENCY_OVER_FLOOR_MS
            || p95 > LATENCY_MAX_P95_MS
        {
            faults.push(format!(
                "latency p50 {p50:.1} ms, p95 {p95:.1} ms over {count} moves, floor {floor50:.1} and {floor95:.1} ms (needs at most {LATENCY_OVER_FLOOR_MS} ms more, and p95 under {LATENCY_MAX_P95_MS} ms)"
            ));
        }
        if report.idle_cpu_percent >= 1. {
            faults.push(format!(
                "{:.2} % of a core with the loupe still",
                report.idle_cpu_percent
            ));
        }
    }
    faults
}

/// `--loupe-diagnostic`: the whole protocol, timed, with the composed loupe, which this
/// machine (Windows 11 26200) offers: falling back there is a failure. Figures in
/// target/loupe-diagnostics.txt, the edge enlarged 8x in target/loupe-edge-zoom.png.
pub(crate) fn run() -> Result<()> {
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
    let report = probe(true, Renderer::Composed)?;
    let taskbar = taskbar(true)?;
    let mut faults = verdict(&report, true);
    match &taskbar {
        Some(taskbar) => faults.extend(taskbar_verdict(taskbar, Some(&report.floor_ms))),
        None => faults.push("no taskbar at the bottom of the probe's screen: untested".into()),
    }
    if report.renderer != Some(Renderer::Composed) {
        faults.push(format!(
            "the loupe fell back to round bands: {}",
            report.fallback.as_deref().unwrap_or("no reason given")
        ));
    }
    let (p50, p95, count) = spread(&report.latencies_ms);
    let (floor50, floor95, _) = spread(&report.floor_ms);
    let (update50, update_max, _) = spread(&report.update_ms);
    let mut text = format!(
        "renderer {:?}{}\nlatency p50 {p50:.1} ms, p95 {p95:.1} ms over {count} moves; plain window moved the same way {floor50:.1} and {floor95:.1} ms\nupdate p50 {update50:.2} ms, max {update_max:.2} ms ({} discs)\nidle CPU {:.3} %\nfading edge: {:?}\n",
        report.renderer,
        report
            .fallback
            .as_deref()
            .map(|reason| format!(" ({reason})"))
            .unwrap_or_default(),
        super::loupe::FEATHER_STEPS + 1,
        report.idle_cpu_percent,
        report.feather
    );
    for look in &report.looks {
        text += &format!(
            "at {:?} zoom {}: {:.2} % of {} exact (at worst {} levels off), {} stray, {} in the corners\n",
            look.at,
            look.zoom,
            100. * look.share(),
            look.total,
            look.worst,
            look.stray,
            look.corners
        );
    }
    if let Some(taskbar) = &taskbar {
        let (p50, p95, count) = spread(&taskbar.latencies_ms);
        let (update50, update_max, _) = spread(&taskbar.update_ms);
        text += &format!(
            "over the taskbar at {:?}: {:.2} % of {} exact (at worst {} levels off), {} window(s) by capture; latency p50 {p50:.1} ms, p95 {p95:.1} ms over {count} moves; update p50 {update50:.2} ms, max {update_max:.2} ms\n",
            taskbar.look.at,
            100. * taskbar.look.share(),
            taskbar.look.total,
            taskbar.look.worst,
            taskbar.captured
        );
    }
    text += &faults.join("\n");
    let _ = std::fs::write("target/loupe-diagnostics.txt", &text);
    if let Some(zoom) = &report.edge_zoom {
        let _ = crate::export::save_png(zoom, std::path::Path::new("target/loupe-edge-zoom.png"));
    }
    println!("{text}");
    if faults.is_empty() {
        Ok(())
    } else {
        Err(fail(faults.join("; ")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The loupe on the real screen shows the painted pattern enlarged, pixel for pixel, at every
    /// zoom, at the screens' corners and edges, round with a smooth edge; never a window it
    /// leaves out, never one kept from captures; and nothing of it stays once it is gone.
    #[test]
    #[ignore = "real desktop: covers the screen with a pattern and reads it back, run alone by check.ps1"]
    fn real_desktop_loupe_shows_the_screen_enlarged_and_never_itself() {
        let report = probe(false, Renderer::Composed).unwrap();
        let faults = verdict(&report, false);
        assert!(faults.is_empty(), "{}", faults.join("\n"));
    }

    /// The fallback, where Windows has no shared visuals: the same, with steps on its edge.
    #[test]
    #[ignore = "real desktop: covers the screen with a pattern and reads it back, run alone by check.ps1"]
    fn real_desktop_loupe_falls_back_to_round_bands() {
        let report = probe(false, Renderer::Banded).unwrap();
        assert_eq!(report.renderer, Some(Renderer::Banded));
        let faults = verdict(&report, false);
        assert!(faults.is_empty(), "{}", faults.join("\n"));
    }

    /// The presentation with the halo and the loupe on: the loupe stands alone, with no ring,
    /// no ticks and no halo around it, and shows the screen under it.
    #[test]
    #[ignore = "real desktop: covers the screen with a pattern and reads it back, run alone by check.ps1"]
    fn real_desktop_loupe_comes_without_ring_or_halo() {
        let _com = crate::ComApartment::new().unwrap();
        let owner = OwnedWindow(unsafe {
            CreateWindowExW(
                WS_EX_TOOLWINDOW,
                w!("STATIC"),
                w!("Loupe presentation test"),
                WS_POPUP,
                -32000,
                -32000,
                1,
                1,
                None,
                None,
                None,
                None,
            )
            .unwrap()
        });
        let yellow = [255, 211, 64];
        let preferences = PresentationSettings {
            halo_color: yellow,
            ..Default::default()
        };
        let screen = crate::image::probe_screen();
        let pattern = Sheet::new(screen, pattern_bgra).unwrap();
        // The loupe opens where the pointer is said to be, on the probe's screen.
        POINTER.set(Some(POINT {
            x: (screen.left + screen.right) / 2,
            y: (screen.top + screen.bottom) / 2,
        }));
        composed();
        let before = capture_desktop().unwrap();
        // The loupe sits where the pointer was when it opened; nothing moves it afterwards, as
        // this test dispatches no input: read its place from its window.
        let state = Presentation::new(owner.0, preferences.clone()).unwrap();
        let mut p = state.borrow_mut();
        p.set_enabled(Enabled {
            halo: true,
            loupe: true,
            ..Default::default()
        })
        .unwrap();
        let loupe = p.loupe.as_ref().expect("the loupe's switch opens it");
        let (hwnd, renderer) = (loupe.hwnd(), loupe.renderer());
        composed();
        let after = capture_desktop().unwrap();
        let mut placed = RECT::default();
        unsafe { GetWindowRect(hwnd, &mut placed).unwrap() };
        p.stop().unwrap();
        assert!(p.loupe.is_none(), "stopping closes it");
        drop(pattern);
        assert_eq!(renderer, Renderer::Composed, "{:?}", fallback_reason());
        // The window's left edge is the pointer less half the source, enlarged.
        let half = (placed.right - placed.left) / 2 / 2 * 2;
        let at = POINT {
            x: placed.left + half,
            y: placed.top + half,
        };
        assert_eq!(frame(at.x, at.y, DIAMETER, 2).window.left, placed.left);
        let seen = look(&before, &after, at, 2, renderer);
        assert!(
            seen.share() >= EXACT_AT_EDGES && seen.corners == 0,
            "{seen:?}"
        );
        // The halo's colour nowhere on or around the loupe: no ring, no ticks, no disc.
        let halo = [yellow[2], yellow[1], yellow[0]];
        let mut tinted = 0;
        for y in placed.top - 16..placed.bottom + 16 {
            for x in placed.left - 16..placed.right + 16 {
                tinted += usize::from(bgr(&after, x, y) == Some(halo));
            }
        }
        assert_eq!(tinted, 0, "the halo's colour around the loupe at {at:?}");
    }

    /// The Windows 11 taskbar under the loupe, which DWM thumbnails get wrong: shown by
    /// capture, pixel for pixel. Its flyouts (quick settings, notifications) need a click to
    /// open and stay untested.
    #[test]
    #[ignore = "real desktop: reads the taskbar on the screen and through the loupe, run alone by check.ps1"]
    fn real_desktop_loupe_shows_the_taskbar() {
        let Some(taskbar) = taskbar(false).unwrap() else {
            return;
        };
        let faults = taskbar_verdict(&taskbar, None);
        assert!(faults.is_empty(), "{}", faults.join("\n"));
        assert!(taskbar.captured > 0, "the taskbar was not captured");
    }

    #[test]
    fn the_pattern_tells_neighbours_apart() {
        for (x, y) in [(0, 0), (-1920, 5), (1919, 1079), (300, 300)] {
            let here = pattern_bgra(x, y);
            for (dx, dy) in [(1, 0), (0, 1), (2, 0), (0, 2), (256, 0), (0, 256)] {
                assert_ne!(here, pattern_bgra(x + dx, y + dy), "{x},{y} +{dx},{dy}");
            }
        }
    }
}
