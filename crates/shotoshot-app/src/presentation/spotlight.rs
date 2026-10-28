//! ROLE: The live spotlight: one dark window over the virtual screen with the light cut out.
//! OWNER: Presentation, while the spotlight is chosen; destroyed with the other surfaces.
//! INVARIANTS: Click-through and never activated. Moving the light only changes the window's
//! region: the system composes the darkness, nothing is painted per move.
use super::*;
use crate::resources::OwnedWindow;
use shotoshot_core::pointer::{PointerSettings, spotlight_hole};

pub(super) struct Spotlight {
    window: OwnedWindow,
    origin: POINT,
    shown: bool,
    /// Darkness applied to the window, 0..=255.
    alpha: u8,
}
impl Spotlight {
    pub fn new() -> Result<Self> {
        unsafe {
            let instance = GetModuleHandleW(None)?;
            if RegisterClassW(&WNDCLASSW {
                lpfnWndProc: Some(spotlight_proc),
                hInstance: instance.into(),
                lpszClassName: w!("Shotoshot.Spotlight"),
                hbrBackground: HBRUSH(GetStockObject(BLACK_BRUSH).0),
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
            let window = OwnedWindow(CreateWindowExW(
                WS_EX_TOOLWINDOW
                    | WS_EX_LAYERED
                    | WS_EX_TRANSPARENT
                    | WS_EX_NOACTIVATE
                    | WS_EX_TOPMOST,
                w!("Shotoshot.Spotlight"),
                w!("Shotoshot spotlight"),
                WS_POPUP,
                origin.x,
                origin.y,
                GetSystemMetrics(SM_CXVIRTUALSCREEN),
                GetSystemMetrics(SM_CYVIRTUALSCREEN),
                None,
                None,
                Some(instance.into()),
                None,
            )?);
            Ok(Self {
                window,
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
            let hole: Vec<POINT> = spotlight_hole(settings, local, scale, screen)
                .into_iter()
                .map(|p| POINT {
                    x: p.x.round() as i32,
                    y: p.y.round() as i32,
                })
                .collect();
            let region = CreateRectRgn(0, 0, bounds.right, bounds.bottom);
            let light = CreatePolygonRgn(&hole, WINDING);
            CombineRgn(Some(region), Some(region), Some(light), RGN_DIFF);
            let _ = DeleteObject(light.into());
            // The window owns the region from here, including on failure.
            SetWindowRgn(self.hwnd(), Some(region), self.shown);
            if !self.shown {
                SetWindowPos(
                    self.hwnd(),
                    Some(HWND_TOPMOST),
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
                )?;
                self.shown = true;
            }
        }
        Ok(())
    }
}
unsafe extern "system" fn spotlight_proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    unsafe { DefWindowProcW(hwnd, msg, w, l) }
}
