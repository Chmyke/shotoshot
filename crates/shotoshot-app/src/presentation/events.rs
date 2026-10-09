//! ROLE: Translates native events into opt-in presentation state changes.
//! OWNER: Presentation, on its resident message loop.
use super::*;
impl Presentation {
    pub fn raw_input(&mut self, l: LPARAM) -> Result<()> {
        let raw = input::read_raw(l)?;
        // PITFALL: Read only the union member identified by the native header.
        unsafe {
            if raw.header.dwType == RIM_TYPEMOUSE.0 {
                self.move_halo()?;
                let flags = raw.data.mouse.Anonymous.Anonymous.usButtonFlags;
                // The diagram never activates: native SetCapture alone cannot guarantee
                // background drag events outside its window. Raw Input supplies the release.
                if self.input_drag.is_some() {
                    self.move_input_drag()?;
                    if flags & 2 != 0 {
                        self.end_input_drag(true)?;
                    }
                }
                if flags != 0 {
                    if self.enabled.halo {
                        self.halo_state.mouse(flags, self.now());
                    }
                    if let Some(halo) = &self.halo {
                        self.paint(halo.hwnd())?;
                    }
                    if self.enabled.mouse {
                        self.devices.mouse(flags, self.now());
                        if let Some(keys) = &self.keys {
                            self.paint(keys.hwnd())?;
                        }
                    }
                    self.sync_timer()?;
                }
            } else if raw.header.dwType == RIM_TYPEKEYBOARD.0 && self.enabled.keys {
                let key = raw.data.keyboard;
                // Keep independent break events as recovery if a hook missed a release.
                if self.bindings.priority_active() && key.Flags & 1 == 0 {
                    return Ok(());
                }
                if self.keyboard(key) {
                    if let Some(keys) = &self.keys {
                        self.paint(keys.hwnd())?;
                    }
                    self.sync_timer()?;
                }
            }
        }
        Ok(())
    }
    /// Records a key event. False when the diagram does not show that key: nothing to repaint.
    pub(super) fn keyboard(&mut self, key: RAWKEYBOARD) -> bool {
        if key.VKey == 255 || key.MakeCode == 0xff {
            return false;
        }
        let scan = if key.MakeCode == 0 {
            unsafe { MapVirtualKeyW(key.VKey as u32, MAPVK_VK_TO_VSC_EX) as u16 }
        } else {
            key.MakeCode
        };
        let extended = key.VKey != VK_NUMLOCK.0 && (key.Flags & 2 != 0 || scan & 0xff00 == 0xe000);
        let now = self.now();
        let shown = |devices: &Devices, scan, extended| {
            diagram::shows(self.plan(), devices.ansi, scan, extended)
        };
        // Pause has an E1 make sequence without a corresponding break.
        if key.VKey == VK_PAUSE.0 {
            if !shown(&self.devices, 0x45, true) {
                return false;
            }
            self.devices.keyboard(0x45, true, true, now);
            self.devices.keyboard(0x45, true, false, now);
        } else if key.Flags & 4 == 0 {
            let down = key.Flags & 1 == 0;
            if !shown(&self.devices, scan & 0xff, extended) {
                self.devices.hold(scan & 0xff, extended, down);
                return false;
            }
            self.devices.keyboard(scan & 0xff, extended, down, now);
            if down {
                unsafe {
                    let mut state = [0u8; 256];
                    if GetKeyboardState(&mut state).is_ok() {
                        // The observer owns physical modifier state; the resident input queue
                        // may belong to a different application and lag behind the hook.
                        for (vk, scan, ext) in [
                            (VK_LSHIFT, 42, false),
                            (VK_RSHIFT, 54, false),
                            (VK_LCONTROL, 29, false),
                            (VK_RCONTROL, 29, true),
                            (VK_LMENU, 56, false),
                            (VK_RMENU, 56, true),
                        ] {
                            state[vk.0 as usize] = if self.devices.active(scan, ext, now) {
                                0x80
                            } else {
                                0
                            };
                        }
                        state[VK_SHIFT.0 as usize] =
                            state[VK_LSHIFT.0 as usize] | state[VK_RSHIFT.0 as usize];
                        state[VK_CONTROL.0 as usize] =
                            state[VK_LCONTROL.0 as usize] | state[VK_RCONTROL.0 as usize];
                        state[VK_MENU.0 as usize] =
                            state[VK_LMENU.0 as usize] | state[VK_RMENU.0 as usize];
                        // Show the printable key for Ctrl+C/Alt+F; preserve AltGr's layout mapping.
                        if state[VK_RMENU.0 as usize] & 0x80 == 0 {
                            for vk in [
                                VK_CONTROL,
                                VK_LCONTROL,
                                VK_RCONTROL,
                                VK_MENU,
                                VK_LMENU,
                                VK_RMENU,
                            ] {
                                state[vk.0 as usize] = 0;
                            }
                        }
                        let foreground = GetForegroundWindow();
                        let thread = GetWindowThreadProcessId(foreground, None);
                        let layout = GetKeyboardLayout(thread);
                        self.devices.ansi = layout::is_ansi(layout);
                        if scan == 56 && extended && self.devices.ansi {
                            self.devices.set_label(scan, extended, &[65, 108, 116]);
                        }
                        let physical = (scan & 0xff) | if extended { 0xe000 } else { 0 };
                        let (label, length) = layout::translate(physical, &state, layout);
                        let text = &label[..length];
                        if !text.is_empty() && text.iter().all(|unit| *unit > 32) {
                            self.devices.set_label(scan & 0xff, extended, text);
                        }
                    }
                }
            }
        }
        true
    }
    pub fn hotkey(&mut self, id: i32) -> Result<()> {
        match id - HOTKEY_BASE {
            0 => self.stop(),
            1 => self.arm(LiveTool::Arrow),
            2 => self.arm(LiveTool::Rectangle),
            3 => self.arm(LiveTool::Ellipse),
            _ => Ok(()),
        }
    }
    pub(super) fn handle(
        &mut self,
        hwnd: HWND,
        msg: u32,
        w: WPARAM,
        l: LPARAM,
    ) -> Result<Option<LRESULT>> {
        if msg == WM_PAINT {
            unsafe {
                let mut paint = PAINTSTRUCT::default();
                let _ = BeginPaint(hwnd, &mut paint);
                let result = self.paint(hwnd);
                let _ = EndPaint(hwnd, &paint);
                result?;
            }
            return Ok(Some(LRESULT(0)));
        }
        if msg == WM_ERASEBKGND {
            return Ok(Some(LRESULT(1)));
        }
        if let Some(result) = self.input_window_message(hwnd, msg)? {
            return Ok(Some(result));
        }
        if !self
            .canvas
            .as_ref()
            .is_some_and(|layer| layer.hwnd() == hwnd)
        {
            return Ok(None);
        }
        let point = Point {
            x: (l.0 as u16 as i16) as f64,
            y: ((l.0 >> 16) as u16 as i16) as f64,
        };
        match msg {
            WM_LBUTTONDOWN if self.scene.intercepts_input() => {
                self.scene.begin(point);
                let mods = held_modifiers();
                self.scene.update(point, mods[2], mods[1]);
                let scale = monitor_scale(POINT {
                    x: point.x as i32 + self.origin.x,
                    y: point.y as i32 + self.origin.y,
                });
                self.scene.set_style(&self.preferences, scale);
                unsafe {
                    SetCapture(hwnd);
                }
            }
            WM_MOUSEMOVE if self.scene.intercepts_input() => {
                let mods = held_modifiers();
                self.scene.update(point, mods[2], mods[1]);
                if let Some(draft) = &mut self.scene.draft {
                    draft.style.color = self.preferences.shape_color;
                }
            }
            WM_MOUSEWHEEL if self.scene.intercepts_input() => {
                let notches = self.wheel_travel.take((w.0 >> 16) as u16 as i16);
                self.preferences.stroke_width =
                    wheel::stroke_width_after(self.preferences.stroke_width as f64, notches) as u32;
                self.scene
                    .set_style(&self.preferences, monitor_scale(cursor()?));
                self.input_preferences_changed()?;
            }
            WM_LBUTTONUP if self.scene.intercepts_input() => {
                let mods = held_modifiers();
                self.scene.update(point, mods[2], mods[1]);
                if let Some(draft) = &mut self.scene.draft {
                    draft.style.color = self.preferences.shape_color;
                }
                self.scene.finish(self.now(), self.preferences.duration_ms);
                self.release_gesture()?;
                self.sync_timer()?;
            }
            WM_KEYDOWN | WM_KEYUP
                if self.scene.intercepts_input()
                    && [VK_SHIFT, VK_LSHIFT, VK_RSHIFT]
                        .iter()
                        .any(|key| w.0 == key.0 as usize) =>
            {
                let mut position = POINT::default();
                unsafe {
                    GetCursorPos(&mut position)?;
                }
                let mods = held_modifiers();
                self.scene.update(
                    Point {
                        x: (position.x - self.origin.x) as f64,
                        y: (position.y - self.origin.y) as f64,
                    },
                    mods[2],
                    mods[1],
                );
            }
            WM_KEYDOWN if w.0 == VK_ESCAPE.0 as usize => {
                self.release_gesture()?;
            }
            WM_CLOSE | WM_RBUTTONDOWN | WM_CANCELMODE | WM_KILLFOCUS | WM_CAPTURECHANGED => {
                self.release_gesture()?;
            }
            WM_SETCURSOR => {
                unsafe {
                    SetCursor(Some(LoadCursorW(None, IDC_CROSS)?));
                }
                return Ok(Some(LRESULT(1)));
            }
            _ => return Ok(None),
        }
        if let Some(canvas) = &self.canvas {
            canvas.invalidate();
        }
        Ok(Some(LRESULT(0)))
    }
}

pub(crate) fn resident_message(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> Option<LRESULT> {
    let hotkey = msg == WM_HOTKEY && (HOTKEY_BASE..HOTKEY_BASE + 4).contains(&(w.0 as i32));
    if !(msg == WM_INPUT
        || msg == OBSERVED_KEY
        || msg == WM_TIMER && w.0 == TIMER
        || hotkey
        || msg == FAILED)
    {
        return None;
    }
    // PITFALL: The property is removed before Presentation starts destruction.
    unsafe {
        let pointer = GetPropW(hwnd, STATE_PROPERTY).0 as *const RefCell<Presentation>;
        if pointer.is_null() {
            return None;
        }
        let mut failed = msg == FAILED;
        if let Ok(mut p) = (*pointer).try_borrow_mut() {
            let result = if msg == WM_INPUT {
                p.raw_input(l)
            } else if msg == OBSERVED_KEY && p.enabled.keys {
                let shown = p.keyboard(RAWKEYBOARD {
                    VKey: w.0 as u16,
                    MakeCode: (l.0 as u32 & 0xffff) as u16,
                    Flags: ((l.0 as u32) >> 16) as u16,
                    ..Default::default()
                });
                let result = match &p.keys {
                    Some(keys) if shown => p.paint(keys.hwnd()),
                    _ => Ok(()),
                };
                result.and_then(|_| p.sync_timer())
            } else if hotkey {
                let _ = EndMenu();
                p.hotkey(w.0 as i32)
            } else if msg == WM_TIMER {
                p.tick()
            } else {
                Ok(())
            };
            if result.is_err() {
                let _ = p.stop();
                failed = true;
            }
        } else if msg == OBSERVED_KEY {
            // Foreground changes may re-enter this window while arming a shape.
            // Keep the packet for the next dispatch instead of losing its release.
            let _ = PostMessageW(Some(hwnd), msg, w, l);
        }
        if failed {
            MessageBoxW(
                Some(hwnd),
                w!(
                    "Presentation stopped after an error. You can enable it again from the Shotoshot menu."
                ),
                w!("Shotoshot"),
                MB_OK | MB_ICONINFORMATION,
            );
        }
        Some(if msg == WM_INPUT {
            DefWindowProcW(hwnd, msg, w, l)
        } else {
            LRESULT(0)
        })
    }
}
