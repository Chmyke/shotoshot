//! ROLE: The live spotlight: one dark surface over the virtual screen, with the light and its
//! soft edge painted into it around the pointer.
//! OWNER: Presentation, while the spotlight is chosen; destroyed with the other surfaces.
//! INVARIANTS: Click-through and never activated. The darkness and the soft edge live in one
//! window that never moves, so the screen always shows them together: two windows moved one
//! after the other let a frame show a gap between them (2026-10-28). A move repaints and sends
//! only the area the light left and the area it reaches; the edge is computed once per size.
use super::*;
use crate::image::LiveSurface;
use crate::resources::OwnedWindow;
use shotoshot_core::pointer::{PointerSettings, SpotlightShape, spotlight_box, spotlight_shade};

/// What the soft edge was computed for: a change computes it again.
#[derive(Clone, Copy, PartialEq)]
struct EdgeKey {
    shape: SpotlightShape,
    size: u32,
    alpha: u8,
    width: i32,
    height: i32,
}

pub(super) struct Spotlight {
    window: OwnedWindow,
    origin: POINT,
    size: SIZE,
    /// The darkness and the light as shown, allocated once the spotlight first lights.
    surface: Option<LiveSurface>,
    /// The light and its soft edge, premultiplied BGRA, for `key`.
    edge: Vec<u8>,
    key: Option<EdgeKey>,
    /// Where the edge sits in the surface now, None when the surface is dark all over.
    placed: Option<RECT>,
    shown: bool,
    /// Darkness of the surface, 0..=255.
    alpha: u8,
}
impl Spotlight {
    pub fn new() -> Result<Self> {
        unsafe {
            let origin = POINT {
                x: GetSystemMetrics(SM_XVIRTUALSCREEN),
                y: GetSystemMetrics(SM_YVIRTUALSCREEN),
            };
            let size = SIZE {
                cx: GetSystemMetrics(SM_CXVIRTUALSCREEN),
                cy: GetSystemMetrics(SM_CYVIRTUALSCREEN),
            };
            Ok(Self {
                window: layered_window(w!("Shotoshot spotlight"), origin, size)?,
                origin,
                size,
                surface: None,
                edge: Vec::new(),
                key: None,
                placed: None,
                shown: false,
                alpha: 0,
            })
        }
    }
    pub fn hwnd(&self) -> HWND {
        self.window.0
    }
    /// Lights `at` (screen pixels) when `lit`, hides the darkness otherwise.
    pub fn update(
        &mut self,
        lit: bool,
        at: POINT,
        settings: &PointerSettings,
        scale: f64,
    ) -> Result<()> {
        if !lit {
            if self.shown {
                unsafe {
                    let _ = ShowWindow(self.hwnd(), SW_HIDE);
                }
                self.shown = false;
            }
            return Ok(());
        }
        let alpha = (settings.spotlight_dim * 255 / 100) as u8;
        let whole = RECT {
            right: self.size.cx,
            bottom: self.size.cy,
            ..Default::default()
        };
        let screen = shotoshot_core::Rect {
            min: Point { x: 0., y: 0. },
            max: Point {
                x: self.size.cx as f64,
                y: self.size.cy as f64,
            },
        };
        let local = Point {
            x: (at.x - self.origin.x) as f64,
            y: (at.y - self.origin.y) as f64,
        };
        let edge = spotlight_box(settings, local, scale, screen);
        let place = RECT {
            left: edge.min.x.round() as i32,
            top: edge.min.y.round() as i32,
            right: edge.max.x.round() as i32,
            bottom: edge.max.y.round() as i32,
        };
        let key = EdgeKey {
            shape: settings.spotlight,
            size: settings.spotlight_size,
            alpha,
            width: place.right - place.left,
            height: place.bottom - place.top,
        };
        if self.key != Some(key) {
            self.edge = edge_tile(settings, scale, alpha, key.width, key.height);
            self.key = Some(key);
            // The old edge may differ anywhere it reached.
            self.placed = self.placed.map(|old| union(old, place));
        }
        let fresh = self.surface.is_none() || alpha != self.alpha;
        if self.surface.is_none() {
            self.surface = Some(LiveSurface::new(self.size.cx as u32, self.size.cy as u32)?);
        }
        let dirty = if fresh {
            whole
        } else {
            match self.placed {
                Some(old) => clip(union(old, place), whole),
                None => clip(place, whole),
            }
        };
        let hwnd = self.hwnd();
        if let Some(surface) = &mut self.surface {
            compose(
                surface.bits_mut(),
                self.size.cx,
                dirty,
                place,
                &self.edge,
                alpha,
            );
            if fresh {
                surface.present(hwnd)?;
            } else {
                surface.present_region(hwnd, dirty)?;
            }
        }
        self.alpha = alpha;
        self.placed = Some(place);
        if !self.shown {
            unsafe {
                SetWindowPos(
                    self.hwnd(),
                    Some(HWND_TOPMOST),
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
                )?;
            }
            self.shown = true;
        }
        Ok(())
    }
}

/// A click-through, never activated, topmost layered window over `size` from `origin`.
fn layered_window(title: PCWSTR, origin: POINT, size: SIZE) -> Result<OwnedWindow> {
    unsafe {
        let instance = GetModuleHandleW(None)?;
        if RegisterClassW(&WNDCLASSW {
            lpfnWndProc: Some(spotlight_proc),
            hInstance: instance.into(),
            lpszClassName: w!("Shotoshot.Spotlight"),
            ..Default::default()
        }) == 0
            && GetLastError() != ERROR_CLASS_ALREADY_EXISTS
        {
            return Err(Error::from_thread());
        }
        Ok(OwnedWindow(CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE | WS_EX_TOPMOST,
            w!("Shotoshot.Spotlight"),
            title,
            WS_POPUP,
            origin.x,
            origin.y,
            size.cx,
            size.cy,
            None,
            None,
            Some(instance.into()),
            None,
        )?))
    }
}

/// The light and its soft edge, `width` by `height` premultiplied BGRA, darkness `alpha` at
/// most: black whose alpha byte alone carries the shade.
fn edge_tile(
    settings: &PointerSettings,
    scale: f64,
    alpha: u8,
    width: i32,
    height: i32,
) -> Vec<u8> {
    let center = Point {
        x: width as f64 * 0.5,
        y: height as f64 * 0.5,
    };
    let mut tile = vec![0; (width.max(0) * height.max(0) * 4) as usize];
    for (row, line) in tile.chunks_exact_mut(width.max(1) as usize * 4).enumerate() {
        for (column, pixel) in line.as_chunks_mut::<4>().0.iter_mut().enumerate() {
            let offset = Point {
                x: column as f64 + 0.5 - center.x,
                y: row as f64 + 0.5 - center.y,
            };
            let shade = spotlight_shade(settings, offset, scale);
            *pixel = [0, 0, 0, (alpha as f64 * shade).round() as u8];
        }
    }
    tile
}

/// Paints `dirty` of a `width`-wide premultiplied BGRA surface: even darkness `dark`, with the
/// edge `tile` laid over `place` (which may run past the surface).
fn compose(bits: &mut [u8], width: i32, dirty: RECT, place: RECT, tile: &[u8], dark: u8) {
    let tile_width = place.right - place.left;
    let row_bytes = width as usize * 4;
    for y in dirty.top..dirty.bottom {
        let row = &mut bits[y as usize * row_bytes..(y as usize + 1) * row_bytes];
        for pixel in row[dirty.left as usize * 4..dirty.right as usize * 4]
            .as_chunks_mut::<4>()
            .0
        {
            *pixel = [0, 0, 0, dark];
        }
        if y < place.top || y >= place.bottom {
            continue;
        }
        let from = dirty.left.max(place.left);
        let to = dirty.right.min(place.right);
        if from >= to {
            continue;
        }
        let source = ((y - place.top) * tile_width + (from - place.left)) as usize * 4;
        let count = (to - from) as usize * 4;
        row[from as usize * 4..from as usize * 4 + count]
            .copy_from_slice(&tile[source..source + count]);
    }
}
fn union(a: RECT, b: RECT) -> RECT {
    RECT {
        left: a.left.min(b.left),
        top: a.top.min(b.top),
        right: a.right.max(b.right),
        bottom: a.bottom.max(b.bottom),
    }
}
fn clip(r: RECT, bounds: RECT) -> RECT {
    let left = r.left.clamp(bounds.left, bounds.right);
    let top = r.top.clamp(bounds.top, bounds.bottom);
    RECT {
        left,
        top,
        right: r.right.clamp(left, bounds.right),
        bottom: r.bottom.clamp(top, bounds.bottom),
    }
}
unsafe extern "system" fn spotlight_proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    unsafe { DefWindowProcW(hwnd, msg, w, l) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shotoshot_core::pointer::SPOTLIGHT_FEATHER;

    fn rect(left: i32, top: i32, right: i32, bottom: i32) -> RECT {
        RECT {
            left,
            top,
            right,
            bottom,
        }
    }
    fn alpha_at(bits: &[u8], width: i32, x: i32, y: i32) -> u8 {
        bits[((y * width + x) * 4 + 3) as usize]
    }

    /// After every move the surface holds the edge where the light is and even darkness
    /// everywhere else, though only the dirty areas were painted.
    #[test]
    fn moving_the_light_leaves_no_trace_and_no_gap() {
        let (width, height) = (60, 40);
        let dark = 150;
        // A 10 x 8 edge whose pixels say where they come from.
        let tile: Vec<u8> = (0..80u8).flat_map(|i| [0, 0, 0, i]).collect();
        let mut bits = vec![0; (width * height * 4) as usize];
        let whole = rect(0, 0, width, height);
        let mut placed: Option<RECT> = None;
        for (x, y) in [(5, 5), (8, 6), (30, 20), (55, 36), (-4, -3), (20, 10)] {
            let place = rect(x, y, x + 10, y + 8);
            let dirty = match placed {
                Some(old) => clip(union(old, place), whole),
                None => whole,
            };
            compose(&mut bits, width, dirty, place, &tile, dark);
            placed = Some(place);
            for py in 0..height {
                for px in 0..width {
                    let inside = (place.left..place.right).contains(&px)
                        && (place.top..place.bottom).contains(&py);
                    let expected = if inside {
                        ((py - place.top) * 10 + (px - place.left)) as u8
                    } else {
                        dark
                    };
                    assert_eq!(
                        alpha_at(&bits, width, px, py),
                        expected,
                        "{x},{y} at {px},{py}"
                    );
                }
            }
        }
    }

    #[test]
    fn the_edge_is_light_in_the_middle_and_dark_at_its_corners() {
        let settings = PointerSettings {
            spotlight: SpotlightShape::Circle,
            spotlight_size: 100,
            ..Default::default()
        };
        let side = 100 + 2 * SPOTLIGHT_FEATHER as i32;
        let tile = edge_tile(&settings, 1., 200, side, side);
        assert_eq!(alpha_at(&tile, side, side / 2, side / 2), 0);
        assert_eq!(alpha_at(&tile, side, 0, 0), 200);
        let mid = alpha_at(
            &tile,
            side,
            side / 2 + 50 + SPOTLIGHT_FEATHER as i32 / 2,
            side / 2,
        );
        assert!((90..=110).contains(&mid), "{mid}");
    }

    /// The darkness on the real screen fades across the soft edge instead of stepping, and
    /// stays whole while the light moves.
    #[test]
    #[ignore = "real desktop: reads back composed windows, run alone by check.ps1"]
    fn real_desktop_the_spotlight_fades_in_across_its_soft_edge() {
        let settings = PointerSettings {
            spotlight: SpotlightShape::Circle,
            spotlight_size: 200,
            spotlight_dim: 60,
            ..Default::default()
        };
        // A white sheet under the spotlight, so every shade reads as one known value, on the
        // probe's screen. The darkness itself spans every screen, for a second.
        let screen = crate::image::probe_screen();
        let origin = POINT {
            x: screen.left + 100,
            y: screen.top + 100,
        };
        let sheet = layered_window(
            w!("Shotoshot spotlight test"),
            origin,
            SIZE { cx: 700, cy: 600 },
        )
        .unwrap();
        let mut white = LiveSurface::new(700, 600).unwrap();
        white.bits_mut().fill(255);
        white.present(sheet.0).unwrap();
        unsafe {
            ShowWindow(sheet.0, SW_SHOWNOACTIVATE).ok().ok();
        }
        let mut spotlight = Spotlight::new().unwrap();
        let half = 100;
        let feather = SPOTLIGHT_FEATHER as i32;
        for (dx, dy) in [(300, 300), (337, 295), (400, 300)] {
            let center = POINT {
                x: origin.x + dx,
                y: origin.y + dy,
            };
            spotlight.update(true, center, &settings, 1.).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(300));
            let screen = crate::image::capture_desktop().unwrap();
            let red = |dx: i32| {
                let (x, y) = (center.x + dx - screen.left, center.y - screen.top);
                screen.pixels[(y as usize * screen.width as usize + x as usize) * 4 + 2] as f64
            };
            let lit = red(0);
            let dark = red(half + feather + 20);
            let middle = red(half + feather / 2);
            assert!(lit > 245., "light {lit}");
            assert!((dark - 102.).abs() < 6., "darkness {dark}");
            assert!((middle - 178.).abs() < 12., "halfway {middle}");
            let mut previous = lit;
            for dx in half..=half + feather + 2 {
                let value = red(dx);
                assert!(
                    value <= previous + 1.,
                    "steps back up at {dx}: {previous} then {value}"
                );
                assert!(
                    previous - value < 12.,
                    "a hard step at {dx}: {previous} then {value}"
                );
                previous = value;
            }
        }
    }
}
