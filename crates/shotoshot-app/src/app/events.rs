use super::*;
use windows::Win32::UI::Controls::WM_MOUSELEAVE;

impl App {
    pub(super) fn handle(
        &mut self,
        hwnd: HWND,
        msg: u32,
        w: WPARAM,
        l: LPARAM,
    ) -> Result<Option<LRESULT>> {
        let point = Point {
            x: (l.0 as u16 as i16) as f64,
            y: ((l.0 >> 16) as u16 as i16) as f64,
        };
        match msg {
            WM_TIMER if w.0 == editor_events::RETRY => {
                editor_events::dispatch(hwnd, |message, w, l| {
                    self.handle(hwnd, message, w, l).map(|_| ())
                })?;
            }
            editor_process::READY => {
                if let Some(worker) = &mut self.worker
                    && worker.pid() == w.0 as u32
                    && let Some(window) = worker.ready(HWND(l.0 as _))
                {
                    self.activate_editor(window)?;
                }
            }
            editor_process::EXITED => self.editor_exited(hwnd, w.0 as u32)?,
            WM_TIMER if w.0 == STROKE_WIDTH_TIMER => self.stroke_width_timer(hwnd)?,
            instant::FINISHED => self.finish_instant_capture(),
            WM_TIMER if w.0 == instant::RETRY_TIMER => {
                unsafe {
                    let _ = KillTimer(Some(hwnd), instant::RETRY_TIMER);
                }
                self.finish_instant_capture();
            }
            notice::MESSAGE => {
                if self
                    .worker
                    .as_ref()
                    .is_some_and(|worker| worker.pid() == w.0 as u32)
                    && let Some(notice) = Notice::from_code(l.0 as usize)
                {
                    self.notify(notice);
                }
            }
            GOOGLE_FINISHED => {
                if let Some(receiver) = self.google_search.take() {
                    if let Some(s) = &mut self.session {
                        s.capture.searching = false;
                    }
                    match receiver.try_recv() {
                        Ok(Ok(url)) => {
                            let opened = unsafe {
                                ShellExecuteW(
                                    Some(hwnd),
                                    w!("open"),
                                    &HSTRING::from(url),
                                    None,
                                    None,
                                    SW_SHOWNORMAL,
                                )
                            };
                            if opened.0 as isize <= 32 {
                                message(
                                    hwnd,
                                    "Could not open the browser. Your capture is still open.",
                                );
                            } else {
                                self.close_session(hwnd);
                            }
                        }
                        Ok(Err(error)) => message(
                            hwnd,
                            &format!("Google search failed. Your capture is still open.\n{error}"),
                        ),
                        Err(_) => message(
                            hwnd,
                            "Google search was interrupted. Your capture is still open.",
                        ),
                    }
                    invalidate(hwnd);
                }
            }

            RENDERER_READY => {
                if let Some(s) = &mut self.session {
                    s.accelerate()?;
                    invalidate(hwnd);
                }
            }

            WM_PAINT => unsafe {
                let mut paint = PAINTSTRUCT::default();
                let hdc = BeginPaint(hwnd, &mut paint);
                let result = if let Some(s) = &mut self.session {
                    s.paint_window(hwnd, hdc)
                } else if self.setup {
                    let mut rect = RECT::default();
                    let _ = GetClientRect(hwnd, &mut rect);
                    self.setup_ui.draw(hdc, rect)
                } else {
                    Ok(())
                };
                let _ = EndPaint(hwnd, &paint);
                result?;
                if let (Some(probe), Some(s)) = (&mut self.first_frame, &self.session)
                    && probe.painted(s.native.renderer.backend() == Backend::Hardware)?
                {
                    self.close_session(hwnd);
                }
            },
            WM_ERASEBKGND => return Ok(Some(LRESULT(1))),
            WM_COMMAND if self.session.is_some() => {
                invalidate(hwnd);
            }
            WM_HOTKEY if self.setup => {
                if self.setup_ui.recording {
                    self.setup_ui
                        .record((l.0 as u32) & 0xffff, (l.0 as u32) >> 16);
                    invalidate(hwnd);
                }
            }
            WM_HOTKEY if self.instant.hotkey.matches(w.0) => self.start_instant_capture(hwnd),
            WM_HOTKEY if self.hotkey.matches(w.0) => self.start_capture(hwnd)?,
            CAPTURE_MESSAGE => {
                if w.0 != 0
                    && let Some(probe) = &mut self.first_frame
                {
                    probe.requested(w.0);
                }
                self.start_capture(hwnd)?;
            }
            WM_KEYUP | WM_SYSKEYUP
                if self.setup && self.setup_ui.recording && w.0 == VK_SNAPSHOT.0 as usize =>
            {
                // Print Screen may arrive only on release; other keys record on key-down.
                self.setup_key(hwnd, w.0 as u32)?;
            }
            WM_KEYDOWN | WM_SYSKEYDOWN => {
                // Holding the recorded key must not activate the newly focused Save button.
                if !self.setup || l.0 & (1 << 30) == 0 {
                    self.key(hwnd, w.0 as u32)?;
                }
            }
            WM_KEYUP | WM_SYSKEYUP => {
                if let Some(s) = &mut self.session {
                    s.capture.editor.update_modifiers(modifiers());
                    invalidate(hwnd);
                } else if self.setup && self.setup_ui.recording {
                    self.setup_ui.held = input::held_modifiers();
                    invalidate(hwnd);
                }
            }
            WM_SETCURSOR if self.setup => unsafe {
                SetCursor(Some(LoadCursorW(None, IDC_ARROW)?));
                return Ok(Some(LRESULT(1)));
            },
            WM_SETCURSOR if self.session.is_some() => unsafe {
                let s = self.session.as_ref().unwrap();
                let mut location = POINT::default();
                GetCursorPos(&mut location)?;
                let _ = ScreenToClient(hwnd, &mut location);
                let cursor = s.cursor(Point {
                    x: location.x as f64,
                    y: location.y as f64,
                });
                SetCursor(Some(LoadCursorW(None, cursor)?));
                return Ok(Some(LRESULT(1)));
            },
            WM_DPICHANGED if self.setup => unsafe {
                let rect = &*(l.0 as *const RECT);
                SetWindowPos(
                    hwnd,
                    None,
                    rect.left,
                    rect.top,
                    rect.right - rect.left,
                    rect.bottom - rect.top,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                )?;
                invalidate(hwnd);
            },
            WM_LBUTTONDOWN => {
                if self.setup {
                    let mut rect = RECT::default();
                    unsafe {
                        GetClientRect(hwnd, &mut rect)?;
                    }
                    if let Some(action) = setup::hit(
                        point.x * setup::WIDTH as f64 / rect.right as f64,
                        point.y * setup::HEIGHT as f64 / rect.bottom as f64,
                    ) && (action != 2 || !self.setup_ui.recording)
                    {
                        self.setup_activate(hwnd, action)?;
                    }
                }
                if let Some(s) = &mut self.session {
                    s.capture.hover_hint.update(None, std::time::Instant::now());
                    unsafe {
                        let _ = KillTimer(Some(hwnd), crate::toolbar::HINT_TIMER);
                    }
                }
                if let Some(s) = &mut self.session
                    && let Some(a) = s.mouse_down(hwnd, point)?
                {
                    self.action(hwnd, a)?;
                }
                invalidate(hwnd);
            }
            WM_TIMER if w.0 == crate::toolbar::HINT_TIMER => {
                unsafe {
                    let _ = KillTimer(Some(hwnd), crate::toolbar::HINT_TIMER);
                }
                invalidate(hwnd);
            }
            WM_MOUSELEAVE => {
                if let Some(s) = &mut self.session {
                    s.capture.hover_hint.update(None, std::time::Instant::now());
                }
                unsafe {
                    let _ = KillTimer(Some(hwnd), crate::toolbar::HINT_TIMER);
                }
                invalidate(hwnd);
            }
            WM_MOUSEMOVE => {
                if let Some(s) = &mut self.session {
                    s.refresh_live_sample(point)?;
                    s.mouse_move(hwnd, point);
                    let target = s.capture.hover_target(point);
                    s.capture
                        .hover_hint
                        .update(target, std::time::Instant::now());
                    unsafe {
                        let mut tracking = TRACKMOUSEEVENT {
                            cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                            dwFlags: TME_LEAVE,
                            hwndTrack: hwnd,
                            dwHoverTime: 0,
                        };
                        let _ = TrackMouseEvent(&mut tracking);
                    }
                    invalidate(hwnd);
                }
            }
            WM_LBUTTONUP => {
                if let Some(s) = &mut self.session {
                    s.mouse_up(hwnd, point);
                    invalidate(hwnd);
                }
            }
            WM_MOUSEWHEEL => {
                if let Some(s) = &mut self.session {
                    s.wheel(hwnd, (w.0 >> 16) as u16 as i16)?;
                    invalidate(hwnd);
                }
            }
            WM_CAPTURECHANGED => {
                if let Some(s) = &mut self.session {
                    s.cancel_gesture(hwnd);
                    if unsafe { GetForegroundWindow() } == hwnd {
                        s.focus_text();
                    }
                    invalidate(hwnd);
                }
            }
            WM_ACTIVATE if w.0 & 0xffff != WA_INACTIVE as usize => {
                if let Some(s) = &self.session
                    && s.capture.picker.is_none()
                    && !s.capture.eyedropper
                {
                    s.focus_text();
                }
            }
            WM_DISPLAYCHANGE => {
                self.prepared = None;
                if let Some(presentation) = &self.presentation {
                    presentation.borrow_mut().display_changed()?;
                }
                if self.session.is_some() {
                    self.close_session(hwnd);
                    message(
                        hwnd,
                        "The display configuration changed. Start a new capture.",
                    );
                }
            }
            crate::presentation::PREFERENCES_CHANGED => {
                if let Some(presentation) = &self.presentation {
                    let preferences = presentation.borrow().preferences.clone();
                    let mut next = if self.persist_preferences {
                        Settings::load().map_err(|e| Error::new(E_FAIL, e))?
                    } else {
                        self.settings.clone()
                    };
                    // Only the fields changed by manipulation are merged; another window may
                    // have saved duration, colors or shortcuts while this notification was queued.
                    next.presentation.input_offset = preferences.input_offset;
                    next.presentation.input_locked = preferences.input_locked;
                    next.presentation.stroke_width = preferences.stroke_width;
                    if self.persist_preferences {
                        next.save().map_err(|e| Error::new(E_FAIL, e))?;
                    }
                    self.settings = next;
                }
            }
            TRAY_MESSAGE => {
                if matches!(l.0 as u32, WM_LBUTTONUP | WM_RBUTTONUP) {
                    self.menu(hwnd)?;
                }
            }
            WM_CLOSE => {
                if self.session.is_some() {
                    self.close_session(hwnd);
                } else if self.setup && self.hotkey.active() {
                    self.close_setup(hwnd)?;
                } else {
                    unsafe {
                        PostQuitMessage(0);
                    }
                }
            }
            WM_DESTROY => unsafe {
                editor_events::take(hwnd);
                PostQuitMessage(0);
            },
            _ => return Ok(None),
        }
        unsafe {
            let _ = KillTimer(Some(hwnd), crate::toolbar::HINT_TIMER);
            if let Some(delay) = self
                .session
                .as_ref()
                .and_then(|s| s.capture.hover_hint.next_tick(std::time::Instant::now()))
            {
                SetTimer(Some(hwnd), crate::toolbar::HINT_TIMER, delay, None);
            }
        }
        self.persist_stroke_width(hwnd)?;
        self.persist_custom_color()?;
        Ok(Some(LRESULT(0)))
    }
}
