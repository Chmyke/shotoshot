//! Brief, hollow capture feedback on the captured monitor. Never activates or receives input.
use super::*;
const TICKS: isize = 16;
pub(super) struct Pulse {
    window: crate::resources::OwnedWindow,
}
impl Pulse {
    pub fn new(bounds: RECT, visible: bool) -> Result<Self> {
        let width = bounds.right - bounds.left;
        let height = bounds.bottom - bounds.top;
        if width <= 0 || height <= 0 {
            return Err(Error::from_hresult(E_INVALIDARG));
        }
        unsafe {
            let instance = GetModuleHandleW(None)?;
            if RegisterClassW(&WNDCLASSW {
                lpfnWndProc: Some(pulse_proc),
                hInstance: instance.into(),
                lpszClassName: w!("Shotoshot.CapturePulse"),
                ..Default::default()
            }) == 0
                && GetLastError() != ERROR_CLASS_ALREADY_EXISTS
            {
                return Err(Error::from_thread());
            }
            let window = crate::resources::OwnedWindow(CreateWindowExW(
                WS_EX_TOOLWINDOW
                    | WS_EX_NOACTIVATE
                    | WS_EX_TRANSPARENT
                    | WS_EX_LAYERED
                    | WS_EX_TOPMOST,
                w!("Shotoshot.CapturePulse"),
                w!(""),
                WS_POPUP,
                bounds.left,
                bounds.top,
                width,
                height,
                None,
                None,
                Some(instance.into()),
                None,
            )?);
            let thickness = (2 * GetDpiForWindow(window.0).max(96) as i32 / 96).max(1);
            let outer = CreateRectRgn(0, 0, width, height);
            let inner = CreateRectRgn(thickness, thickness, width - thickness, height - thickness);
            let combined = CombineRgn(Some(outer), Some(outer), Some(inner), RGN_DIFF);
            let _ = DeleteObject(inner.into());
            if combined == GDI_REGION_TYPE(0) {
                let _ = DeleteObject(outer.into());
                return Err(Error::from_thread());
            }
            // Windows owns the region after a successful SetWindowRgn.
            if SetWindowRgn(window.0, Some(outer), false) == 0 {
                let _ = DeleteObject(outer.into());
                return Err(Error::from_thread());
            }
            SetWindowDisplayAffinity(window.0, WDA_EXCLUDEFROMCAPTURE)?;
            SetLayeredWindowAttributes(window.0, COLORREF(0), 0, LWA_ALPHA)?;
            if SetTimer(Some(window.0), 1, 25, None) == 0 {
                return Err(Error::from_thread());
            }
            if visible {
                let _ = ShowWindow(window.0, SW_SHOWNOACTIVATE);
            }
            Ok(Self { window })
        }
    }
}
impl Drop for Pulse {
    fn drop(&mut self) {
        unsafe {
            let _ = KillTimer(Some(self.window.0), 1);
        }
    }
}

unsafe extern "system" fn pulse_proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    let result = std::panic::catch_unwind(|| unsafe {
        match msg {
            WM_NCHITTEST => return LRESULT(HTTRANSPARENT as isize),
            WM_MOUSEACTIVATE => return LRESULT(MA_NOACTIVATE as isize),
            WM_ERASEBKGND => return LRESULT(1),
            WM_TIMER if w.0 == 1 => {
                let tick = (GetWindowLongPtrW(hwnd, GWLP_USERDATA) + 1).min(TICKS);
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, tick);
                if tick == TICKS {
                    let _ = KillTimer(Some(hwnd), 1);
                    let _ = ShowWindow(hwnd, SW_HIDE);
                } else {
                    let alpha = (89.0
                        * (std::f64::consts::PI * tick as f64 / TICKS as f64)
                            .sin()
                            .powi(2))
                    .round() as u8;
                    let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), alpha, LWA_ALPHA);
                }
                return LRESULT(0);
            }
            WM_PAINT => {
                let mut paint = PAINTSTRUCT::default();
                let dc = BeginPaint(hwnd, &mut paint);
                let mut bounds = RECT::default();
                let _ = GetClientRect(hwnd, &mut bounds);
                let brush = CreateSolidBrush(notice::color(0x60cdff));
                // The hollow window region clips painting to the four edges.
                FillRect(dc, &bounds, brush);
                let _ = DeleteObject(brush.into());
                let _ = EndPaint(hwnd, &paint);
                return LRESULT(0);
            }
            _ => {}
        }
        DefWindowProcW(hwnd, msg, w, l)
    });
    result.unwrap_or(LRESULT(0))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pulse_has_only_four_edges_does_not_activate_and_disappears() {
        unsafe {
            let bounds = RECT {
                left: -1280,
                top: 80,
                right: 0,
                bottom: 800,
            };
            let pulse = Pulse::new(bounds, false).unwrap();
            let window = pulse.window.0;
            let mut rect = RECT::default();
            GetWindowRect(window, &mut rect).unwrap();
            assert_eq!(
                (rect.left, rect.top, rect.right, rect.bottom),
                (-1280, 80, 0, 800)
            );
            let region = CreateRectRgn(0, 0, 0, 0);
            assert_ne!(GetWindowRgn(window, region), GDI_REGION_TYPE(0));
            for (x, y) in [(0, 360), (1279, 360), (640, 0), (640, 719)] {
                assert!(PtInRegion(region, x, y).as_bool());
            }
            assert!(!PtInRegion(region, 640, 360).as_bool());
            assert!(!PtInRegion(region, 10, 10).as_bool());
            let _ = DeleteObject(region.into());
            let style = GetWindowLongPtrW(window, GWL_EXSTYLE) as u32;
            assert_eq!(
                style & (WS_EX_NOACTIVATE | WS_EX_TRANSPARENT).0,
                (WS_EX_NOACTIVATE | WS_EX_TRANSPARENT).0
            );
            let mut affinity = 0;
            GetWindowDisplayAffinity(window, &mut affinity).unwrap();
            assert_eq!(affinity, WDA_EXCLUDEFROMCAPTURE.0);
            for _ in 0..8 {
                SendMessageW(window, WM_TIMER, Some(WPARAM(1)), None);
            }
            let mut alpha = 0;
            GetLayeredWindowAttributes(window, None, Some(&mut alpha), None).unwrap();
            assert_eq!(alpha, 89);
            for _ in 8..16 {
                SendMessageW(window, WM_TIMER, Some(WPARAM(1)), None);
            }
            assert!(!IsWindowVisible(window).as_bool());
            assert_eq!(GetWindowLongPtrW(window, GWLP_USERDATA), 16);
            assert!(crate::app::nothing_activated_on_this_thread());
            drop(pulse);
            assert!(!IsWindow(Some(window)).as_bool());
        }
    }
}
