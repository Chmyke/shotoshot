//! ROLE: The live spotlight: one dark window over the virtual screen with the light cut out,
//! and one window over the light that fades the darkness in across its soft edge.
//! OWNER: Presentation, while the spotlight is chosen; destroyed with the other surfaces.
//! INVARIANTS: Click-through and never activated. Moving the light only changes the dark
//! window's region and the edge window's place: the soft edge is painted once per size, not
//! per move. The two windows never overlap, so no pixel is darkened twice.
use super::*;
use crate::image::LiveSurface;
use crate::resources::OwnedWindow;
use shotoshot_core::pointer::{PointerSettings, SpotlightShape, spotlight_box, spotlight_shade};

/// What the soft edge was painted for: a change repaints it.
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
    edge: OwnedWindow,
    key: Option<EdgeKey>,
    edge_at: POINT,
    origin: POINT,
    shown: bool,
    /// Darkness applied to the window, 0..=255.
    alpha: u8,
}
impl Spotlight {
    pub fn new() -> Result<Self> {
        unsafe {
            let instance = GetModuleHandleW(None)?;
            for (class, background) in [
                (
                    w!("Shotoshot.Spotlight"),
                    HBRUSH(GetStockObject(BLACK_BRUSH).0),
                ),
                (w!("Shotoshot.SpotlightEdge"), HBRUSH::default()),
            ] {
                if RegisterClassW(&WNDCLASSW {
                    lpfnWndProc: Some(spotlight_proc),
                    hInstance: instance.into(),
                    lpszClassName: class,
                    hbrBackground: background,
                    ..Default::default()
                }) == 0
                    && GetLastError() != ERROR_CLASS_ALREADY_EXISTS
                {
                    return Err(Error::from_thread());
                }
            }
            let origin = POINT {
                x: GetSystemMetrics(SM_XVIRTUALSCREEN),
                y: GetSystemMetrics(SM_YVIRTUALSCREEN),
            };
            let create = |class: PCWSTR, title: PCWSTR, width: i32, height: i32| {
                CreateWindowExW(
                    WS_EX_TOOLWINDOW
                        | WS_EX_LAYERED
                        | WS_EX_TRANSPARENT
                        | WS_EX_NOACTIVATE
                        | WS_EX_TOPMOST,
                    class,
                    title,
                    WS_POPUP,
                    origin.x,
                    origin.y,
                    width,
                    height,
                    None,
                    None,
                    Some(instance.into()),
                    None,
                )
                .map(OwnedWindow)
            };
            let window = create(
                w!("Shotoshot.Spotlight"),
                w!("Shotoshot spotlight"),
                GetSystemMetrics(SM_CXVIRTUALSCREEN),
                GetSystemMetrics(SM_CYVIRTUALSCREEN),
            )?;
            let edge = create(
                w!("Shotoshot.SpotlightEdge"),
                w!("Shotoshot spotlight edge"),
                1,
                1,
            )?;
            Ok(Self {
                window,
                edge,
                key: None,
                edge_at: origin,
                origin,
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
        unsafe {
            if !lit {
                if self.shown {
                    let _ = ShowWindow(self.hwnd(), SW_HIDE);
                    let _ = ShowWindow(self.edge.0, SW_HIDE);
                    self.shown = false;
                }
                return Ok(());
            }
            let alpha = (settings.spotlight_dim * 255 / 100) as u8;
            if alpha != self.alpha {
                SetLayeredWindowAttributes(self.hwnd(), COLORREF(0), alpha, LWA_ALPHA)?;
                self.alpha = alpha;
            }
            let mut bounds = RECT::default();
            GetClientRect(self.hwnd(), &mut bounds)?;
            let screen = shotoshot_core::Rect {
                min: Point { x: 0., y: 0. },
                max: Point {
                    x: bounds.right as f64,
                    y: bounds.bottom as f64,
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
            self.place_edge(settings, scale, alpha, place)?;
            let region = CreateRectRgn(0, 0, bounds.right, bounds.bottom);
            let light = CreateRectRgn(place.left, place.top, place.right, place.bottom);
            CombineRgn(Some(region), Some(region), Some(light), RGN_DIFF);
            let _ = DeleteObject(light.into());
            // The window owns the region from here, including on failure.
            SetWindowRgn(self.hwnd(), Some(region), self.shown);
            if !self.shown {
                for hwnd in [self.edge.0, self.hwnd()] {
                    SetWindowPos(
                        hwnd,
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
        }
        Ok(())
    }
    /// Puts the soft edge over `place` (pixels of the dark window), painting it first when its
    /// shape, size or darkness changed.
    fn place_edge(
        &mut self,
        settings: &PointerSettings,
        scale: f64,
        alpha: u8,
        place: RECT,
    ) -> Result<()> {
        let key = EdgeKey {
            shape: settings.spotlight,
            size: settings.spotlight_size,
            alpha,
            width: place.right - place.left,
            height: place.bottom - place.top,
        };
        let at = POINT {
            x: self.origin.x + place.left,
            y: self.origin.y + place.top,
        };
        unsafe {
            if self.key != Some(key) {
                // The window keeps its own copy of the pixels once updated.
                let mut surface = LiveSurface::new(key.width as u32, key.height as u32)?;
                let center = Point {
                    x: key.width as f64 * 0.5,
                    y: key.height as f64 * 0.5,
                };
                for (row, line) in surface
                    .bits_mut()
                    .chunks_exact_mut(key.width as usize * 4)
                    .enumerate()
                {
                    for (column, pixel) in line.as_chunks_mut::<4>().0.iter_mut().enumerate() {
                        let offset = Point {
                            x: column as f64 + 0.5 - center.x,
                            y: row as f64 + 0.5 - center.y,
                        };
                        let shade = spotlight_shade(settings, offset, scale);
                        // Premultiplied black: only the alpha byte carries the darkness.
                        *pixel = [0, 0, 0, (alpha as f64 * shade).round() as u8];
                    }
                }
                UpdateLayeredWindow(
                    self.edge.0,
                    None,
                    Some(&at),
                    Some(&SIZE {
                        cx: key.width,
                        cy: key.height,
                    }),
                    Some(surface.dc()),
                    Some(&POINT { x: 0, y: 0 }),
                    COLORREF(0),
                    Some(&BLENDFUNCTION {
                        BlendOp: AC_SRC_OVER as u8,
                        BlendFlags: 0,
                        SourceConstantAlpha: 255,
                        AlphaFormat: AC_SRC_ALPHA as u8,
                    }),
                    ULW_ALPHA,
                )?;
                self.key = Some(key);
            } else if at != self.edge_at {
                SetWindowPos(
                    self.edge.0,
                    None,
                    at.x,
                    at.y,
                    0,
                    0,
                    SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
                )?;
            }
        }
        self.edge_at = at;
        Ok(())
    }
}
unsafe extern "system" fn spotlight_proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    unsafe { DefWindowProcW(hwnd, msg, w, l) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shotoshot_core::pointer::{SPOTLIGHT_FEATHER, SpotlightShape};

    /// The darkness on the real screen fades across the soft edge instead of stepping.
    #[test]
    #[ignore = "real desktop: reads back composed windows, run alone by check.ps1"]
    fn real_desktop_the_spotlight_fades_in_across_its_soft_edge() {
        let settings = PointerSettings {
            spotlight: SpotlightShape::Circle,
            spotlight_size: 200,
            spotlight_dim: 60,
            ..Default::default()
        };
        let center = POINT { x: 400, y: 400 };
        let mut spotlight = Spotlight::new().unwrap();
        // A white sheet under the spotlight, so every shade reads as one known value.
        let sheet = Spotlight::new().unwrap();
        let mut white = LiveSurface::new(600, 600).unwrap();
        white.bits_mut().fill(255);
        unsafe {
            UpdateLayeredWindow(
                sheet.edge.0,
                None,
                Some(&POINT { x: 100, y: 100 }),
                Some(&SIZE { cx: 600, cy: 600 }),
                Some(white.dc()),
                Some(&POINT { x: 0, y: 0 }),
                COLORREF(0),
                Some(&BLENDFUNCTION {
                    BlendOp: AC_SRC_OVER as u8,
                    BlendFlags: 0,
                    SourceConstantAlpha: 255,
                    AlphaFormat: AC_SRC_ALPHA as u8,
                }),
                ULW_ALPHA,
            )
            .unwrap();
            ShowWindow(sheet.edge.0, SW_SHOWNOACTIVATE).ok().ok();
        }
        spotlight.update(true, center, &settings, 1.).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(400));
        let screen = crate::image::capture_desktop().unwrap();
        let red = |dx: i32| {
            let (x, y) = (center.x + dx - screen.left, center.y - screen.top);
            screen.pixels[(y as usize * screen.width as usize + x as usize) * 4 + 2] as f64
        };
        let half = 100;
        let feather = SPOTLIGHT_FEATHER as i32;
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
