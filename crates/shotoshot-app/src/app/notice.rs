//! ROLE: Displays Shotoshot-owned transient feedback, independent of Windows notifications.
//! OWNER: App; editor workers send numeric events only to their parent window.
//! INVARIANTS: No focus changes, polling, text history or private image/path payload.
use super::*;
pub(super) const MESSAGE: u32 = WM_APP + 5;
const WIDTH: i32 = 360;
const HEIGHT: i32 = 56;
const HOLD_TICKS: isize = 60;
const TOTAL_TICKS: isize = 70;
const TICK_MS: u32 = 30;
/// How long a confirmation stays on screen, fade included.
pub(super) const DURATION_MS: u32 = TOTAL_TICKS as u32 * TICK_MS;
const INSTANT_STYLE: isize = 1 << 16;
const INSTANT_ALPHA: u8 = 89;
const INSTANT_HOLD: isize = 19;
const INSTANT_TOTAL: isize = 25;

pub(super) struct Toast {
    window: crate::resources::OwnedWindow,
}
impl Toast {
    #[cfg(test)]
    pub(super) fn text(&self) -> String {
        let mut text = [0u16; 256];
        let len = unsafe { GetWindowTextW(self.window.0, &mut text) } as usize;
        String::from_utf16_lossy(&text[..len])
    }
    pub fn new(text: &str, visible: bool) -> Result<Self> {
        Self::new_at(text, visible, None)
    }
    pub fn new_at(text: &str, visible: bool, target_monitor: Option<RECT>) -> Result<Self> {
        // PITFALL: The window owns only its text and timer; callbacks never borrow App.
        unsafe {
            let instance = GetModuleHandleW(None)?;
            if RegisterClassW(&WNDCLASSW {
                lpfnWndProc: Some(toast_proc),
                hInstance: instance.into(),
                lpszClassName: w!("Shotoshot.Confirmation"),
                ..Default::default()
            }) == 0
                && GetLastError() != ERROR_CLASS_ALREADY_EXISTS
            {
                return Err(Error::from_thread());
            }
            let mut pointer = POINT::default();
            GetCursorPos(&mut pointer)?;
            let mut monitor = MONITORINFO {
                cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                ..Default::default()
            };
            GetMonitorInfoW(
                MonitorFromPoint(pointer, MONITOR_DEFAULTTONEAREST),
                &mut monitor,
            )
            .ok()?;
            if let Some(bounds) = target_monitor {
                monitor.rcWork = bounds;
            }
            let window = crate::resources::OwnedWindow(CreateWindowExW(
                WS_EX_TOOLWINDOW
                    | WS_EX_NOACTIVATE
                    | WS_EX_TRANSPARENT
                    | WS_EX_LAYERED
                    | WS_EX_TOPMOST,
                w!("Shotoshot.Confirmation"),
                &HSTRING::from(text),
                WS_POPUP,
                monitor.rcWork.left,
                monitor.rcWork.top,
                WIDTH,
                HEIGHT,
                None,
                None,
                Some(instance.into()),
                None,
            )?);
            let scale = GetDpiForWindow(window.0).max(96) as i32;
            let (width, height) = if target_monitor.is_some() {
                compact_size(text, scale)?
            } else {
                (WIDTH * scale / 96, HEIGHT * scale / 96)
            };
            let width = width.min(monitor.rcWork.right - monitor.rcWork.left);
            SetWindowPos(
                window.0,
                Some(HWND_TOPMOST),
                monitor.rcWork.left + (monitor.rcWork.right - monitor.rcWork.left - width) / 2,
                if target_monitor.is_some() {
                    monitor.rcWork.top + 24 * scale / 96
                } else {
                    monitor.rcWork.bottom - height - 24 * scale / 96
                },
                width,
                height,
                SWP_NOACTIVATE,
            )?;
            let region = CreateRoundRectRgn(
                0,
                0,
                width + 1,
                height + 1,
                12 * scale / 96,
                12 * scale / 96,
            );
            if SetWindowRgn(window.0, Some(region), false) == 0 {
                let _ = DeleteObject(region.into());
            }
            let _ = SetWindowDisplayAffinity(window.0, WDA_EXCLUDEFROMCAPTURE);
            let alpha = if target_monitor.is_some() {
                SetWindowLongPtrW(window.0, GWLP_USERDATA, INSTANT_STYLE);
                INSTANT_ALPHA
            } else {
                255
            };
            SetLayeredWindowAttributes(window.0, COLORREF(0), alpha, LWA_ALPHA)?;
            if SetTimer(Some(window.0), 1, TICK_MS, None) == 0 {
                return Err(Error::from_thread());
            }
            if visible {
                let _ = ShowWindow(window.0, SW_SHOWNOACTIVATE);
            }
            Ok(Self { window })
        }
    }
}
impl Drop for Toast {
    fn drop(&mut self) {
        unsafe {
            let _ = KillTimer(Some(self.window.0), 1);
        }
    }
}
unsafe extern "system" fn toast_proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    let result = std::panic::catch_unwind(|| unsafe {
        match msg {
            WM_NCHITTEST => return LRESULT(HTTRANSPARENT as isize),
            WM_MOUSEACTIVATE => return LRESULT(MA_NOACTIVATE as isize),
            WM_TIMER if w.0 == 1 => {
                let state = GetWindowLongPtrW(hwnd, GWLP_USERDATA);
                let style = state & INSTANT_STYLE;
                let (hold, total, opacity) = if style != 0 {
                    (INSTANT_HOLD, INSTANT_TOTAL, INSTANT_ALPHA as isize)
                } else {
                    (HOLD_TICKS, TOTAL_TICKS, 255)
                };
                let tick = ((state & 0xffff) + 1).min(total);
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, style | tick);
                if tick >= total {
                    let _ = KillTimer(Some(hwnd), 1);
                    let _ = ShowWindow(hwnd, SW_HIDE);
                } else if tick > hold {
                    let alpha = (opacity * (total - tick) / (total - hold)) as u8;
                    let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), alpha, LWA_ALPHA);
                }
                return LRESULT(0);
            }
            WM_PAINT => {
                let mut paint = PAINTSTRUCT::default();
                let dc = BeginPaint(hwnd, &mut paint);
                let mut bounds = RECT::default();
                let _ = GetClientRect(hwnd, &mut bounds);
                let mut text = [0u16; 256];
                let len = GetWindowTextW(hwnd, &mut text) as usize;
                let text = String::from_utf16_lossy(&text[..len]);
                if GetWindowLongPtrW(hwnd, GWLP_USERDATA) & INSTANT_STYLE != 0 {
                    draw_compact(
                        dc,
                        bounds.right,
                        bounds.bottom,
                        &text,
                        GetDpiForWindow(hwnd).max(96) as i32,
                    );
                } else {
                    draw(dc, bounds.right, bounds.bottom, &text);
                }
                let _ = EndPaint(hwnd, &paint);
                return LRESULT(0);
            }
            WM_ERASEBKGND => return LRESULT(1),
            WM_CLOSE => {
                let _ = KillTimer(Some(hwnd), 1);
                let _ = ShowWindow(hwnd, SW_HIDE);
                return LRESULT(0);
            }
            _ => {}
        }
        DefWindowProcW(hwnd, msg, w, l)
    });
    result.unwrap_or(LRESULT(0))
}
pub(super) fn color(rgb: u32) -> COLORREF {
    COLORREF(((rgb & 255) << 16) | (rgb & 0xff00) | ((rgb >> 16) & 255))
}
// Save/restore selections before deleting each owned GDI object.
fn panel(dc: HDC, rect: RECT, fill: u32, border: u32, radius: i32) {
    unsafe {
        let brush = CreateSolidBrush(color(fill));
        let pen = CreatePen(PS_SOLID, 1, color(border));
        let old_brush = SelectObject(dc, brush.into());
        let old_pen = SelectObject(dc, pen.into());
        let _ = RoundRect(
            dc,
            rect.left,
            rect.top,
            rect.right,
            rect.bottom,
            radius,
            radius,
        );
        SelectObject(dc, old_pen);
        SelectObject(dc, old_brush);
        let _ = DeleteObject(pen.into());
        let _ = DeleteObject(brush.into());
    }
}
fn label(dc: HDC, text: &str, mut bounds: RECT, size: i32, weight: i32, ink: u32, centered: bool) {
    unsafe {
        let font = CreateFontW(
            -size,
            0,
            0,
            0,
            weight,
            0,
            0,
            0,
            DEFAULT_CHARSET,
            OUT_DEFAULT_PRECIS,
            CLIP_DEFAULT_PRECIS,
            ANTIALIASED_QUALITY,
            DEFAULT_PITCH.0 as u32,
            w!("Segoe UI"),
        );
        let old = SelectObject(dc, font.into());
        SetBkMode(dc, TRANSPARENT);
        SetTextColor(dc, color(ink));
        let mut text: Vec<u16> = text.encode_utf16().collect();
        DrawTextW(
            dc,
            &mut text,
            &mut bounds,
            DT_NOPREFIX
                | if centered {
                    DT_CENTER | DT_VCENTER | DT_SINGLELINE
                } else {
                    DT_LEFT | DT_WORDBREAK
                },
        );
        SelectObject(dc, old);
        let _ = DeleteObject(font.into());
    }
}
fn compact_size(text: &str, scale: i32) -> Result<(i32, i32)> {
    unsafe {
        let dc = GetDC(None);
        if dc.is_invalid() {
            return Err(Error::from_thread());
        }
        let font = CreateFontW(
            -14 * scale / 96,
            0,
            0,
            0,
            400,
            0,
            0,
            0,
            DEFAULT_CHARSET,
            OUT_DEFAULT_PRECIS,
            CLIP_DEFAULT_PRECIS,
            ANTIALIASED_QUALITY,
            DEFAULT_PITCH.0 as u32,
            w!("Segoe UI"),
        );
        if font.is_invalid() {
            ReleaseDC(None, dc);
            return Err(Error::from_thread());
        }
        let previous = SelectObject(dc, font.into());
        let mut size = SIZE::default();
        let text: Vec<u16> = text.encode_utf16().collect();
        let measured = GetTextExtentPoint32W(dc, &text, &mut size).ok();
        SelectObject(dc, previous);
        let _ = DeleteObject(font.into());
        ReleaseDC(None, dc);
        measured?;
        Ok((size.cx + 24 * scale / 96, size.cy + 16 * scale / 96))
    }
}
fn draw_compact(dc: HDC, width: i32, height: i32, text: &str, scale: i32) {
    let bounds = RECT {
        right: width,
        bottom: height,
        ..Default::default()
    };
    panel(dc, bounds, 0x262626, 0x494949, 8 * scale / 96);
    label(dc, text, bounds, 14 * scale / 96, 400, 0xf0f0f0, true);
}
fn draw(dc: HDC, width: i32, height: i32, text: &str) {
    unsafe {
        let saved = SaveDC(dc);
        SetMapMode(dc, MM_ANISOTROPIC);
        let _ = SetWindowExtEx(dc, WIDTH, HEIGHT, None);
        let _ = SetViewportExtEx(dc, width, height, None);
        panel(
            dc,
            RECT {
                left: 0,
                top: 0,
                right: WIDTH,
                bottom: HEIGHT,
            },
            0x262626,
            0x494949,
            12,
        );
        label(
            dc,
            text,
            RECT {
                left: 16,
                top: 0,
                right: WIDTH - 16,
                bottom: HEIGHT,
            },
            14,
            400,
            0xf0f0f0,
            true,
        );
        let _ = RestoreDC(dc, saved);
    }
}
pub(super) fn preview() -> Result<()> {
    let (width, height) = compact_size("Screenshot failed — file not saved", 96)?;
    let compact = crate::paint_image(width as u32, height as u32, |dc| {
        draw_compact(dc, width, height, "Screenshot failed — file not saved", 96);
        Ok(())
    })?;
    crate::save_png(
        &compact,
        std::path::Path::new("target/shotoshot-instant-confirmation.png"),
    )?;
    let image = crate::paint_image(WIDTH as u32, HEIGHT as u32, |dc| {
        draw(dc, WIDTH, HEIGHT, Notice::Copied.text());
        Ok(())
    })?;
    crate::save_png(
        &image,
        std::path::Path::new("target/shotoshot-confirmation.png"),
    )
}

#[derive(Clone, Copy)]
pub(super) enum Notice {
    Copied = 1,
    Saved = 2,
    Searching = 3,
    Preferences = 4,
    SavedEditable = 5,
}
impl Notice {
    pub fn from_code(code: usize) -> Option<Self> {
        Some(match code {
            1 => Self::Copied,
            2 => Self::Saved,
            3 => Self::Searching,
            4 => Self::Preferences,
            5 => Self::SavedEditable,
            _ => return None,
        })
    }
    fn text(self) -> &'static str {
        match self {
            Self::Copied => "Screenshot copied to clipboard.",
            Self::Saved => "Image saved.",
            Self::Searching => "Searching Google…",
            Self::Preferences => "Settings saved.",
            Self::SavedEditable => "Editable image saved, screen included.",
        }
    }
}

impl App {
    pub(super) fn notify(&mut self, notice: Notice) {
        unsafe {
            if let Some(owner) = self.notification_owner {
                let _ = PostMessageW(
                    Some(owner),
                    MESSAGE,
                    WPARAM(GetCurrentProcessId() as usize),
                    LPARAM(notice as isize),
                );
            } else if (!self.quit_after_capture && self.persist_preferences)
                || self.session.as_ref().is_some_and(|s| s.document.is_some())
            {
                // A reopened document runs alone: no resident owns its process.
                self.toast = Toast::new(notice.text(), true).ok();
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn instant_toast_is_translucent_top_center_brief_and_does_not_activate() {
        let bounds = RECT {
            left: -1920,
            top: 0,
            right: 0,
            bottom: 1080,
        };
        let foreground = unsafe { GetForegroundWindow() };
        let toast = Toast::new_at("Screenshot saved", false, Some(bounds)).unwrap();
        unsafe {
            let mut rect = RECT::default();
            GetWindowRect(toast.window.0, &mut rect).unwrap();
            assert_eq!(rect.left + rect.right, bounds.left + bounds.right);
            let scale = GetDpiForWindow(toast.window.0).max(96) as i32;
            assert!(
                rect.right - rect.left < 220 * scale / 96,
                "short text should have a compact box"
            );
            assert!(
                rect.bottom - rect.top < 44 * scale / 96,
                "avoid excessive vertical padding"
            );
            assert_eq!(rect.top, bounds.top + 24 * scale / 96);
            let mut alpha = 0;
            GetLayeredWindowAttributes(toast.window.0, None, Some(&mut alpha), None).unwrap();
            assert_eq!(alpha, 89);
            for _ in 0..22 {
                SendMessageW(toast.window.0, WM_TIMER, Some(WPARAM(1)), None);
            }
            GetLayeredWindowAttributes(toast.window.0, None, Some(&mut alpha), None).unwrap();
            assert_eq!(alpha, 44);
            for _ in 22..25 {
                SendMessageW(toast.window.0, WM_TIMER, Some(WPARAM(1)), None);
            }
            assert_eq!(
                GetWindowLongPtrW(toast.window.0, GWLP_USERDATA) & 0xffff,
                25
            );
            assert_eq!(GetForegroundWindow(), foreground);
            let mut affinity = 0;
            GetWindowDisplayAffinity(toast.window.0, &mut affinity).unwrap();
            assert_eq!(affinity, WDA_EXCLUDEFROMCAPTURE.0);
        }
    }
    #[test]
    fn worker_notices_reach_the_parent_without_activating_a_window() {
        unsafe {
            let owner = crate::resources::OwnedWindow(
                CreateWindowExW(
                    WS_EX_TOOLWINDOW,
                    w!("STATIC"),
                    w!("Notice test"),
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
                .unwrap(),
            );
            let mut app = App {
                settings: Settings::default(),
                session: None,
                setup: false,
                setup_ui: setup::Setup::default(),
                hotkey: crate::hotkey::Hotkey::new(owner.0),
                instant: instant::InstantCapture::new(owner.0),
                board_hotkey: crate::hotkey::Hotkey::with_id(owner.0, 5),
                tray: NOTIFYICONDATAW::default(),
                quit_after_capture: true,
                persist_preferences: false,
                pending_stroke_width: None,
                worker: None,
                prepared: None,
                notification_owner: Some(owner.0),
                toast: None,
                google_search: None,
                first_frame: None,
                presentation: None,
                board_rebake_forced: false,
            };
            let foreground = GetForegroundWindow();
            app.notify(Notice::Copied);
            let mut message = MSG::default();
            assert!(
                PeekMessageW(&mut message, Some(owner.0), MESSAGE, MESSAGE, PM_REMOVE).as_bool()
            );
            assert_eq!(message.wParam.0, GetCurrentProcessId() as usize);
            assert_eq!(message.lParam.0, Notice::Copied as isize);
            assert_eq!(GetForegroundWindow(), foreground);
        }
    }
    #[test]
    fn toast_is_passive_expires_and_releases_its_window() {
        let toast = Toast::new("Screenshot copied", false).unwrap();
        let window = toast.window.0;
        unsafe {
            let style = GetWindowLongPtrW(window, GWL_EXSTYLE) as u32;
            assert_ne!(style & WS_EX_NOACTIVATE.0, 0);
            assert_ne!(style & WS_EX_TRANSPARENT.0, 0);
            assert_ne!(style & WS_EX_TOOLWINDOW.0, 0);
            for _ in 0..HOLD_TICKS + 5 {
                SendMessageW(window, WM_TIMER, Some(WPARAM(1)), None);
            }
            let mut alpha = 0;
            GetLayeredWindowAttributes(window, None, Some(&mut alpha), None).unwrap();
            assert_eq!(alpha, 127, "fade reaches half opacity before dismissal");
            for _ in HOLD_TICKS + 5..TOTAL_TICKS {
                SendMessageW(window, WM_TIMER, Some(WPARAM(1)), None);
            }
            assert!(!IsWindowVisible(window).as_bool());
            assert_eq!(GetWindowLongPtrW(window, GWLP_USERDATA), TOTAL_TICKS);
        }
        drop(toast);
        assert!(!unsafe { IsWindow(Some(window)) }.as_bool());
        assert!(Notice::from_code(0).is_none());
        assert!(Notice::from_code(usize::MAX).is_none());
    }
}
