//! ROLE: Owns the Windows resident, first-run shortcut setup and capture session.
//! OWNER: Main thread; window callbacks borrow checked RefCell state.
//! INVARIANTS: No polling or rendering while idle; presentation input is explicitly opt-in.
mod board;
mod document;
mod editor_events;
mod editor_process;
mod events;
mod input;
mod instant;
mod notice;
mod pulse;
mod warm_probe;
use notice::Notice;
mod runtime;
mod setup;
mod tray_menu;
use crate::{
    session::{Saved, Session, control, modifiers},
    settings::{Settings, Store},
    toolbar::Action,
    *,
};
pub use runtime::{reports_errors_on_stderr, run};
use shotoshot_core::Point;
use shotoshot_text::{Key, Text};
use std::cell::RefCell;
use windows::{
    Win32::{
        Foundation::*,
        Graphics::Gdi::*,
        System::{LibraryLoader::*, Threading::*},
        UI::{HiDpi::*, Input::KeyboardAndMouse::*, Shell::*, WindowsAndMessaging::*},
    },
    core::*,
};

const TRAY_MESSAGE: u32 = WM_APP + 1;
const CAPTURE_MESSAGE: u32 = WM_APP + 2;
const GOOGLE_FINISHED: u32 = WM_APP + 4;
const RENDERER_READY: u32 = WM_APP + 7;
const BRUSH_TIMER: usize = 0x5357;
/// Ends a reopened document's process once its confirmation has faded.
const QUIT_TIMER: usize = 0x5158;
/// Looks at the wallpaper once a change has settled, to bake the board over a foreign one.
const BOARD_TIMER: usize = 0x5342;
/// How long the wallpaper or the displays must stay unchanged before `BOARD_TIMER` looks.
const BOARD_SETTLE_MS: u32 = 1500;
const BRUSH_DELAY_MS: u32 = 250;
#[cfg(test)]
mod display_tests;
#[cfg(test)]
mod tests;
/// The tests leave the foreground, and so the keyboard, to the user. Many drive production code
/// that calls SetFocus or SetForegroundWindow on their hidden windows; a test binary started
/// from the foreground program (a terminal, an agent) is allowed to take it, and the user's
/// keystrokes then went to an invisible window. Locked before main, Windows lets no program take
/// the foreground by itself until the user presses Alt or clicks; the lock ends with the process.
/// Windows refuses the lock to a process that could not take the foreground anyway.
/// `check-foreground.ps1` watches the real foreground during the tests.
#[cfg(test)]
#[used]
#[unsafe(link_section = ".CRT$XCU")]
static LEAVE_THE_FOREGROUND_TO_THE_USER: extern "C" fn() = {
    extern "C" fn lock() {
        use windows::Win32::UI::WindowsAndMessaging::{LSFW_LOCK, LockSetForegroundWindow};
        let _ = unsafe { LockSetForegroundWindow(LSFW_LOCK) };
    }
    lock
};
/// "Does not activate": no window of the calling test's thread is active. Activation within a
/// thread happens whether or not Windows lets the process take the foreground (the tests lock
/// it, above), so this holds on any workstation. Other tests' windows do not count: tests run in
/// parallel, one thread each, and some bring their capture back on purpose.
#[cfg(test)]
fn nothing_activated_on_this_thread() -> bool {
    use windows::Win32::{
        System::Threading::GetCurrentThreadId,
        UI::{
            Input::KeyboardAndMouse::GetActiveWindow,
            WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId},
        },
    };
    unsafe {
        GetActiveWindow().is_invalid()
            && GetWindowThreadProcessId(GetForegroundWindow(), None) != GetCurrentThreadId()
    }
}
struct App {
    settings: Settings,
    session: Option<Session>,
    setup: bool,
    setup_ui: setup::Setup,
    hotkey: crate::hotkey::Hotkey,
    instant: instant::InstantCapture,
    /// Opens a board session (`Settings::board_shortcut`).
    board_hotkey: crate::hotkey::Hotkey,
    tray: NOTIFYICONDATAW,
    quit_after_capture: bool,
    persist_preferences: bool,
    pending_brush: Option<(shotoshot_core::settings::Brush, std::time::Instant)>,
    worker: Option<editor_process::EditorProcess>,
    prepared: Option<crate::session::Preparation>,
    notification_owner: Option<HWND>,
    toast: Option<notice::Toast>,
    google_search: Option<std::sync::mpsc::Receiver<std::result::Result<String, String>>>,
    first_frame: Option<crate::diagnostics::FirstFrame>,
    presentation: Option<Box<RefCell<crate::presentation::Presentation>>>,
    /// The notification menu, while it is open.
    menu: Option<tray_menu::Popup>,
    /// When a press on the notification icon last closed the menu.
    menu_closed_on_icon: Option<std::time::Instant>,
    /// The next `BOARD_TIMER` bakes even while every screen shows our pictures: the displays
    /// changed, and the pictures must be baked again for them.
    board_rebake_forced: bool,
}
/// Paints the notification menu off screen, then closes it, for the resident diagnostic.
pub(crate) fn menu_probe() -> Result<()> {
    tray_menu::probe()
}
/// A message box over `hwnd`: only a translated text can be shown.
pub(crate) fn message(hwnd: HWND, text: &Text) {
    unsafe {
        MessageBoxW(
            Some(hwnd),
            &HSTRING::from(text.as_str()),
            w!("Shotoshot"),
            MB_OK | MB_ICONINFORMATION,
        );
    }
}
/// `error` in a message box, as `failure::text` words it.
fn report(hwnd: HWND, error: &Error) {
    message(hwnd, &crate::failure::text(error));
}
/// The combination `mods` + `key` on one line as the menu writes it (`Ctrl+Alt+A`), in the
/// interface language.
pub(crate) fn shortcut_text(mods: u32, key: u32) -> String {
    setup::key_label_in(mods, key, shotoshot_text::language()).replace(" + ", "+")
}
/// The hint of the notification-area icon, cut to the 127 characters Windows keeps.
fn tray_tip(text: &str) -> [u16; 128] {
    let mut tip = [0; 128];
    for (slot, unit) in tip[..127].iter_mut().zip(text.encode_utf16()) {
        *slot = unit;
    }
    tip
}
fn invalidate(hwnd: HWND) {
    unsafe {
        let _ = InvalidateRect(Some(hwnd), None, false);
    }
}

unsafe extern "system" fn window_proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    // PITFALL: No Rust unwind may cross the system callback; reentrant notifications cannot alias state.
    let outcome = std::panic::catch_unwind(|| unsafe {
        if msg == WM_ACTIVATEAPP
            && GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32 & WS_EX_APPWINDOW.0 != 0
        {
            // Alt+Tab changes only the window's stacking, never the capture or native inputs.
            // Handle this outside App's borrow: Save dialogs dispatch it in a nested loop.
            let _ = SetWindowPos(
                hwnd,
                Some(if w.0 != 0 {
                    HWND_TOPMOST
                } else {
                    // NOTOPMOST still places us above normal apps if activation arrived late.
                    HWND_BOTTOM
                }),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );
        }
        if let Some(result) = crate::presentation::resident_message(hwnd, msg, w, l) {
            return result;
        }
        if msg == WM_NCCREATE {
            let create = &*(l.0 as *const CREATESTRUCTW);
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as isize);
        }
        let pointer = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const RefCell<App>;
        if !pointer.is_null()
            && let Ok(mut app) = (*pointer).try_borrow_mut()
        {
            match app.handle(hwnd, msg, w, l) {
                Ok(Some(value)) => return value,
                Ok(None) => {}
                Err(error) => {
                    report(hwnd, &error);
                    if app.quit_after_capture && app.session.is_none() {
                        PostQuitMessage(1);
                    }
                }
            }
        } else if !pointer.is_null()
            && matches!(
                msg,
                editor_process::READY | editor_process::EXITED | WM_DISPLAYCHANGE
            )
        {
            editor_events::defer(hwnd, msg, w, l);
            return LRESULT(0);
        } else if !pointer.is_null() && msg == instant::FINISHED {
            // Menus dispatch messages reentrantly while borrowing App. Retry after that borrow,
            // rather than losing the one completion event and leaving the worker busy forever.
            SetTimer(Some(hwnd), instant::RETRY_TIMER, 30, None);
            return LRESULT(0);
        }
        DefWindowProcW(hwnd, msg, w, l)
    });
    match outcome {
        Ok(value) => value,
        Err(_) => {
            unsafe {
                PostQuitMessage(1);
            }
            LRESULT(0)
        }
    }
}
impl App {
    fn worker_busy(&self) -> bool {
        self.worker.as_ref().is_some_and(|worker| worker.busy())
    }
    /// Whether the board is shown, as last saved: a board session's own process saves it.
    fn board_visible(&mut self) -> Result<bool> {
        if self.persist_preferences {
            self.reload_settings()?;
        }
        Ok(self.settings.board_visible)
    }
    /// Reads the settings another process may have saved, the language included.
    fn reload_settings(&mut self) -> Result<()> {
        self.settings = Settings::load().map_err(crate::settings::unreadable)?;
        crate::language::apply(&self.settings);
        Ok(())
    }
    /// Replaces the prepared editor, whose texts were laid out in the previous language.
    /// A busy one is left alone: its capture goes on, and the editor prepared after it
    /// reads the new language at start.
    fn restart_prepared_editor(&mut self, hwnd: HWND) {
        match &mut self.worker {
            // The replacement starts once this one has exited (`editor_exited`).
            Some(worker) if !worker.busy() => worker.restart(),
            Some(_) => {}
            None => self.prepare_editor(hwnd),
        }
    }
    fn prepare_editor(&mut self, hwnd: HWND) {
        // Preparation is optional: an unavailable renderer must never show a boot dialog
        // or trigger an automatic restart loop. The next explicit request can try cold.
        if self.worker.is_none() {
            self.worker = editor_process::EditorProcess::spawn(hwnd, true).ok();
        }
    }
    fn activate_editor(&mut self, window: HWND) -> Result<()> {
        let Some(worker) = &self.worker else {
            return Ok(());
        };
        unsafe {
            let _ = AllowSetForegroundWindow(worker.pid());
        }
        let result = (|| {
            if let Some(presentation) = &self.presentation {
                presentation.borrow_mut().suspend_capture()?;
            }
            unsafe { PostMessageW(Some(window), CAPTURE_MESSAGE, WPARAM(0), LPARAM(0)) }
        })();
        if result.is_err() {
            // request/ready marks the child active. Never leave it busy after a failed send.
            // Keep the slot occupied until EXIT confirms death: at most two processes,
            // even if another hotkey is queued while termination is finishing.
            if let Some(worker) = &self.worker {
                worker.stop();
            }
            if let Some(presentation) = &self.presentation {
                presentation.borrow_mut().resume_capture()?;
            }
        }
        result
    }
    fn editor_exited(&mut self, hwnd: HWND, pid: u32) -> Result<()> {
        let Some(worker) = &self.worker else {
            return Ok(());
        };
        if worker.pid() != pid {
            return Ok(());
        }
        let Some(code) = worker.exit_code()? else {
            return Ok(());
        };
        let (replenish, retry_cold) = (worker.replenish(code), worker.retry_cold());
        let failed_request = worker.pending_request() && !retry_cold;
        self.worker = None;
        if let Some(presentation) = &self.presentation {
            presentation.borrow_mut().resume_capture()?;
        }
        if retry_cold {
            self.start_capture(hwnd)?;
        } else if replenish {
            self.prepare_editor(hwnd);
        } else if failed_request {
            return Err(crate::failure::visible(Key::ErrorEditorStart, &[]));
        }
        Ok(())
    }
    fn show_setup(&mut self, hwnd: HWND) -> Result<()> {
        self.show_shortcut_setup(hwnd, setup::ShortcutTarget::Capture)
    }
    fn show_shortcut_setup(&mut self, hwnd: HWND, target: setup::ShortcutTarget) -> Result<()> {
        if self.persist_preferences {
            self.reload_settings()?;
        }
        unsafe {
            if let Some(p) = &self.presentation {
                p.borrow_mut().suspend_shortcuts()?;
            }
            self.setup = true;
            self.setup_ui = if target == setup::ShortcutTarget::Capture && self.settings.key == 0 {
                setup::Setup::first_run()
            } else {
                setup::Setup::change(
                    target,
                    if let Some(index) = target.shape_index() {
                        let s = self.settings.presentation.shape_shortcuts[index];
                        Some((s.modifiers, s.key))
                    } else if target == setup::ShortcutTarget::Instant {
                        Some((
                            self.settings.instant_shortcut.modifiers,
                            self.settings.instant_shortcut.key,
                        ))
                    } else if target == setup::ShortcutTarget::Board {
                        Some((
                            self.settings.board_shortcut.modifiers,
                            self.settings.board_shortcut.key,
                        ))
                    } else {
                        (self.settings.key != 0)
                            .then_some((self.settings.modifiers, self.settings.key))
                    },
                )
            };
            SetWindowLongPtrW(
                hwnd,
                GWL_STYLE,
                (WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU).0 as isize,
            );
            let dark: i32 = 1;
            let _ = windows::Win32::Graphics::Dwm::DwmSetWindowAttribute(
                hwnd,
                windows::Win32::Graphics::Dwm::DWMWA_USE_IMMERSIVE_DARK_MODE,
                (&dark as *const i32).cast(),
                std::mem::size_of_val(&dark) as u32,
            );
            // Never fails: a window that cannot measure keeps the English layout.
            self.setup_ui.fit(shotoshot_text::language());
            let size = self.setup_ui.layout.size;
            let dpi = GetDpiForWindow(hwnd);
            let mut bounds = RECT {
                right: size.x as i32 * dpi as i32 / 96,
                bottom: size.y as i32 * dpi as i32 / 96,
                ..Default::default()
            };
            AdjustWindowRectExForDpi(
                &mut bounds,
                WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU,
                false,
                WS_EX_TOOLWINDOW,
                dpi,
            )?;
            let mut monitor = MONITORINFO {
                cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                ..Default::default()
            };
            let _ = GetMonitorInfoW(
                MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST),
                &mut monitor,
            );
            let width = bounds.right - bounds.left;
            let height = bounds.bottom - bounds.top;
            SetWindowPos(
                hwnd,
                Some(HWND_NOTOPMOST),
                monitor.rcWork.left + (monitor.rcWork.right - monitor.rcWork.left - width) / 2,
                monitor.rcWork.top + (monitor.rcWork.bottom - monitor.rcWork.top - height) / 2,
                width,
                height,
                SWP_FRAMECHANGED | SWP_SHOWWINDOW,
            )?;
            let _ = SetForegroundWindow(hwnd);
            invalidate(hwnd);
            Ok(())
        }
    }
    fn close_setup(&mut self, hwnd: HWND) -> Result<()> {
        self.setup = false;
        // The resident keeps no renderer at rest.
        self.setup_ui.painter = None;
        if let Some(p) = &self.presentation {
            let _conflicts = p.borrow_mut().resume_shortcuts()?;
        }
        unsafe {
            if self.hotkey.active()
                || self.instant.hotkey.active()
                || self.board_hotkey.active()
                || self.setup_ui.target.shape_index().is_some()
            {
                let _ = ShowWindow(hwnd, SW_HIDE);
            } else {
                PostQuitMessage(0);
            }
        }
        Ok(())
    }
    fn save_shape_shortcut(
        &mut self,
        hwnd: HWND,
        index: usize,
        mods: u32,
        key: u32,
    ) -> Result<bool> {
        let mut next = self.settings.clone();
        next.presentation.shape_shortcuts[index] = crate::settings::Shortcut {
            modifiers: mods,
            key,
        };
        let result = (|| -> Result<()> {
            let refused = |key| crate::failure::visible(key, &[]);
            next.presentation
                .validate_shape_shortcuts()
                .map_err(refused)?;
            if (self.settings.modifiers == mods && self.settings.key == key)
                || self.settings.instant_shortcut
                    == (crate::settings::Shortcut {
                        modifiers: mods,
                        key,
                    })
            {
                return Err(refused(Key::SetupUsedForCapture));
            }
            if self.settings.board_shortcut
                == (crate::settings::Shortcut {
                    modifiers: mods,
                    key,
                })
            {
                return Err(refused(Key::SetupUsedByBoard));
            }
            // A real collision is allowed: drawing shortcuts now take priority through the hook.
            let reserved = match unsafe {
                RegisterHotKey(Some(hwnd), 99, HOT_KEY_MODIFIERS(mods) | MOD_NOREPEAT, key)
            } {
                Ok(()) => true,
                Err(error)
                    if error.code() == HRESULT::from_win32(ERROR_HOTKEY_ALREADY_REGISTERED.0) =>
                {
                    false
                }
                Err(error) => return Err(error),
            };
            let save = if self.persist_preferences {
                next.save().map_err(crate::settings::unsaved)
            } else {
                Ok(())
            };
            if reserved {
                unsafe {
                    let _ = UnregisterHotKey(Some(hwnd), 99);
                }
            }
            save?;
            if let Some(p) = &self.presentation {
                p.borrow_mut()
                    .update_preferences(next.presentation.clone())?;
            }
            Ok(())
        })();
        if let Err(error) = result {
            let language = self.setup_ui.language;
            self.setup_ui.error = Text::with_in(
                language,
                Key::SetupInvalid,
                &[("detail", &crate::failure::reason_in(&error, language))],
            )
            .to_string();
            invalidate(hwnd);
            return Ok(false);
        }
        self.settings = next;
        Ok(true)
    }
    fn start_capture(&mut self, hwnd: HWND) -> Result<()> {
        self.toast = None;
        self.instant.clear_feedback();
        if self.session.is_some() {
            // A capture left behind another application (Alt+Tab, Google search) comes back.
            // The resident already allowed this process to take the foreground.
            unsafe {
                let _ = SetForegroundWindow(hwnd);
            }
            return Ok(());
        }
        if self.setup {
            return Ok(());
        }
        if !self.quit_after_capture {
            if self.worker.is_none() {
                self.worker = Some(editor_process::EditorProcess::spawn(hwnd, false)?);
            }
            let worker = self.worker.as_mut().unwrap();
            unsafe {
                let _ = AllowSetForegroundWindow(worker.pid());
            }
            if let Some(window) = worker.request() {
                self.activate_editor(window)?;
            } else if let Some(window) = worker.active_window() {
                unsafe { PostMessageW(Some(window), CAPTURE_MESSAGE, WPARAM(0), LPARAM(0))? };
            }
            return Ok(());
        }
        if self.persist_preferences {
            self.reload_settings()?;
        }
        let live = self.settings.live_capture;
        let image = if live {
            crate::image::blank_desktop()?
        } else {
            capture_desktop()?
        };
        let (width, height) = (image.width, image.height);
        let prepared = match self.prepared.take() {
            Some(prepared) => prepared,
            None => crate::session::Preparation::cold(width, height, live)?,
        };
        let mut session = prepared.activate(image, live)?;
        session.capture.monitors = session.frame_monitors();
        if live {
            unsafe {
                SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE)?;
                let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
                SetWindowLongPtrW(hwnd, GWL_EXSTYLE, style | WS_EX_LAYERED.0 as isize);
            }
        }
        session.prepare_acceleration(hwnd, RENDERER_READY)?;
        self.enter(hwnd, session)
    }
    /// Gives `session` the user's last colour, stroke width and eraser size, then shows the window over
    /// its frame and takes the keyboard.
    fn enter(&mut self, hwnd: HWND, mut session: Session) -> Result<()> {
        let image = &session.capture.image;
        let (left, top, width, height) = (image.left, image.top, image.width, image.height);
        session.capture.custom_color = self.settings.last_custom_color;
        session.capture.editor.style.width = self.settings.capture_stroke_width as f64;
        session.capture.editor.eraser_size = self.settings.capture_eraser_size as f64;
        let mut pointer = POINT::default();
        if unsafe { GetCursorPos(&mut pointer) }.is_ok() {
            session.capture.pointer = Point {
                x: (pointer.x - left) as f64,
                y: (pointer.y - top) as f64,
            };
        }
        self.session = Some(session);
        unsafe {
            SetWindowLongPtrW(hwnd, GWL_STYLE, WS_POPUP.0 as isize);
            SetWindowPos(
                hwnd,
                Some(HWND_TOPMOST),
                left,
                top,
                width as i32,
                height as i32,
                SWP_FRAMECHANGED | SWP_SHOWWINDOW,
            )?;
            let _ = SetForegroundWindow(hwnd);
            let _ = SetFocus(Some(hwnd));
        }
        invalidate(hwnd);
        Ok(())
    }
    fn persist_brush(&mut self, hwnd: HWND) -> Result<()> {
        if let Some(s) = &self.session {
            let brush = shotoshot_core::settings::Brush {
                stroke_width: s.capture.stroke_width_setting(),
                eraser_size: s.capture.eraser_size_setting(),
            };
            if self.settings.brush() != brush {
                self.settings.capture_stroke_width = brush.stroke_width;
                self.settings.capture_eraser_size = brush.eraser_size;
                self.pending_brush = Some((
                    brush,
                    std::time::Instant::now()
                        + std::time::Duration::from_millis(BRUSH_DELAY_MS as u64),
                ));
                if unsafe { SetTimer(Some(hwnd), BRUSH_TIMER, BRUSH_DELAY_MS, None) } == 0 {
                    self.flush_brush()?;
                }
            }
        }
        Ok(())
    }
    fn flush_brush(&mut self) -> Result<()> {
        let persist = self.persist_preferences;
        self.flush_brush_with(|brush| {
            if persist {
                Settings::save_brush(brush)
            } else {
                Ok(())
            }
        })
    }
    fn flush_brush_with(
        &mut self,
        save: impl FnOnce(shotoshot_core::settings::Brush) -> std::result::Result<(), String>,
    ) -> Result<()> {
        if let Some((brush, _)) = self.pending_brush {
            save(brush).map_err(crate::settings::unsaved)?;
            self.pending_brush = None;
        }
        Ok(())
    }
    fn brush_timer(&mut self, hwnd: HWND) -> Result<()> {
        unsafe {
            let _ = KillTimer(Some(hwnd), BRUSH_TIMER);
        }
        if let Some((_, deadline)) = self.pending_brush {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            if remaining.is_zero()
                || unsafe {
                    SetTimer(
                        Some(hwnd),
                        BRUSH_TIMER,
                        remaining.as_millis().max(1) as u32,
                        None,
                    )
                } == 0
            {
                self.flush_brush()?;
            }
        }
        Ok(())
    }
    fn close_session(&mut self, hwnd: HWND) {
        self.close_session_saying(hwnd, None);
    }
    /// Ends the session. A board session first writes its notes and stays open if it cannot;
    /// then it bakes them and, once the window is hidden, shows `board_saved` if all went well.
    fn close_session_saying(&mut self, hwnd: HWND, board_saved: Option<Key>) {
        let board_saved = if self.session.as_ref().is_some_and(|s| s.board.is_some()) {
            if let Err(error) = self.save_board(hwnd) {
                // Nothing is lost: Escape tries again, Ctrl+S still exports the picture.
                let detail = crate::failure::detail(&error);
                message(
                    hwnd,
                    &Text::with(Key::ErrorBoardUnsaved, &[("detail", &detail)]),
                );
                invalidate(hwnd);
                return;
            }
            match self.apply_board() {
                Ok(()) => board_saved,
                Err(error) => {
                    let detail = crate::failure::detail(&error);
                    message(
                        hwnd,
                        &Text::with(Key::ErrorWallpaperUnchanged, &[("detail", &detail)]),
                    );
                    None
                }
            }
        } else {
            None
        };
        if let Err(error) = self.persist_brush(hwnd).and_then(|_| self.flush_brush()) {
            report(hwnd, &error);
        }
        unsafe {
            let _ = KillTimer(Some(hwnd), crate::toolbar::HINT_TIMER);
            let _ = KillTimer(Some(hwnd), BRUSH_TIMER);
        }
        if let Err(error) = self.persist_custom_color() {
            report(hwnd, &error);
        }
        if let Some(s) = &mut self.session {
            s.native.finish_text(&mut s.capture, hwnd, false);
        }
        self.session = None;
        self.google_search = None;
        unsafe {
            let _ = ShowWindow(hwnd, SW_HIDE);
        }
        if let Some(text) = board_saved {
            // Before the quit request below: a message box shown after it closes at once.
            message(hwnd, &Text::new(text));
        }
        if self.quit_after_capture {
            unsafe {
                // A reopened document confirms by itself: it leaves once that has faded.
                if self.toast.is_none()
                    || SetTimer(Some(hwnd), QUIT_TIMER, notice::DURATION_MS, None) == 0
                {
                    PostQuitMessage(0);
                }
            }
        }
    }
    fn action(&mut self, hwnd: HWND, action: Action) -> Result<()> {
        let Some(s) = &mut self.session else {
            return Ok(());
        };
        if s.native.has_dimensions_input()
            && !s
                .native
                .finish_dimensions(&mut s.capture, hwnd, !matches!(action, Action::Close))
        {
            invalidate(hwnd);
            return Ok(());
        }
        match action {
            Action::CopyColor(rgb) => {
                crate::copy_text(
                    hwnd,
                    &format!("#{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2]),
                )?;
            }
            Action::Live => {
                s.set_live(hwnd, !s.capture.live)?;
            }
            Action::GoogleSearch => {
                if self.google_search.is_some() {
                    return Ok(());
                }
                let image = s.capture_output()?;
                let (tx, rx) = std::sync::mpsc::channel();
                let owner = hwnd.0 as usize;
                std::thread::Builder::new()
                    .name("Shotoshot Google Lens".into())
                    .spawn(move || {
                        let result = crate::google_search::prepare(&image)
                            .map(|page| page.to_string_lossy().into_owned())
                            .map_err(|e| crate::failure::reason(&e));
                        if tx.send(result).is_ok() {
                            unsafe {
                                let _ = PostMessageW(
                                    Some(HWND(owner as *mut _)),
                                    GOOGLE_FINISHED,
                                    WPARAM(0),
                                    LPARAM(0),
                                );
                            }
                        }
                    })
                    .map_err(|e| Error::new(E_FAIL, e.to_string()))?;
                s.capture.searching = true;
                self.google_search = Some(rx);
                self.notify(Notice::Searching);
            }
            Action::Copy => {
                let output = s.capture_output()?;
                copy_image(&output, hwnd)?;
                self.notify(Notice::Copied);
                self.close_session(hwnd);
            }
            Action::Save | Action::SaveAs => {
                let saved = s.save(hwnd, &mut self.settings, action == Action::SaveAs)?;
                if saved != Saved::Cancelled {
                    if self.persist_preferences
                        && let Err(error) = Settings::save_destination(
                            &self.settings.folder,
                            self.settings.save_format,
                        )
                    {
                        report(hwnd, &crate::settings::unsaved(error));
                    }
                    self.notify(if saved == Saved::Document {
                        Notice::SavedEditable
                    } else {
                        Notice::Saved
                    });
                    self.close_session(hwnd);
                }
            }
            Action::Close => self.close_session(hwnd),
            _ => {
                s.action(hwnd, action);
                s.refresh_live_sample(s.capture.pointer)?;
                if s.capture.picker.is_some() || s.capture.eyedropper {
                    unsafe {
                        let _ = SetFocus(Some(hwnd));
                    }
                } else {
                    s.focus_text();
                }
            }
        }
        invalidate(hwnd);
        Ok(())
    }
    fn persist_custom_color(&mut self) -> Result<()> {
        if let Some(session) = &mut self.session
            && session.capture.custom_color_changed
        {
            if self.persist_preferences {
                Settings::save_custom_color(session.capture.custom_color)
                    .map_err(crate::settings::unsaved)?;
            }
            self.settings.last_custom_color = session.capture.custom_color;
            session.capture.custom_color_changed = false;
        }
        Ok(())
    }
}
