//! ROLE: The Windows host of the pointer effects window (`shotoshot_ui::pointer_panel`): its
//! native window, painting, mouse, wheel and keys, and what each change does to the settings.
//! OWNER: App holds one `PointerWindow` while it is open; closing drops the window, its timer
//! and its painter, so the resident keeps nothing of it at rest.
//! INVARIANTS: Every change applies to the live pointer at once; a change is saved when it is
//! complete (a dragged slider saves on release).
use super::*;
use crate::render::{D2dCanvas, Renderer};
use crate::resources::OwnedWindow;
use shotoshot_text::{Key, Text, tr_in};
use shotoshot_ui::pointer_panel::{Action, KeyInput, Panel};

/// The preview's frame timer.
const PREVIEW_TIMER: usize = 1;
const CLASS: PCWSTR = w!("Shotoshot.Pointer");

pub(super) struct PointerWindow {
    window: OwnedWindow,
    pub panel: Panel,
    painter: Option<Renderer>,
    clock: std::time::Instant,
    /// A dragged slider changed the settings: the live pointer takes them at the next frame,
    /// not at every mouse move.
    pending: bool,
}
impl PointerWindow {
    pub fn hwnd(&self) -> HWND {
        self.window.0
    }
    fn now(&self) -> u64 {
        self.clock.elapsed().as_millis().min(u64::MAX as u128) as u64
    }
    /// Device pixels per logical pixel of the window.
    fn scale(&self) -> f64 {
        unsafe { GetDpiForWindow(self.hwnd()) as f64 / 96. }
    }
    fn logical(&self, l: LPARAM) -> Point {
        let scale = self.scale();
        Point {
            x: (l.0 as u16 as i16) as f64 / scale,
            y: ((l.0 >> 16) as u16 as i16) as f64 / scale,
        }
    }
    /// Measures the texts of the panel's language and sizes the window for them.
    fn fit(&mut self) -> Result<()> {
        let renderer = match self.painter.take() {
            Some(renderer) => renderer,
            None => Renderer::software()?,
        };
        let language = self.panel.language;
        self.panel.fit(|text, size| {
            renderer
                .measure_text_in(text, size as f32, language)
                .map(|extent| extent.x)
        })?;
        self.painter = Some(renderer);
        self.place()
    }
    /// Sizes the window to the layout at its monitor's scale, kept inside the work area.
    fn place(&self) -> Result<()> {
        unsafe {
            let hwnd = self.hwnd();
            let dpi = GetDpiForWindow(hwnd);
            let size = self.panel.layout.size;
            let mut bounds = RECT {
                right: (size.x * dpi as f64 / 96.).ceil() as i32,
                bottom: (size.y * dpi as f64 / 96.).ceil() as i32,
                ..Default::default()
            };
            AdjustWindowRectExForDpi(
                &mut bounds,
                WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX,
                false,
                WS_EX_LEFT,
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
            let work = monitor.rcWork;
            let width = bounds.right - bounds.left;
            let height = bounds.bottom - bounds.top;
            let x = work.left + ((work.right - work.left - width) / 2).max(0);
            let y = work.top + ((work.bottom - work.top - height) / 2).max(0);
            SetWindowPos(
                hwnd,
                None,
                x,
                y,
                width,
                height,
                SWP_NOZORDER | SWP_NOACTIVATE,
            )
        }
    }
    fn draw(&mut self, dc: HDC, bounds: RECT) -> Result<()> {
        if bounds.right <= bounds.left || bounds.bottom <= bounds.top {
            return Ok(());
        }
        let renderer = match self.painter.take() {
            Some(renderer) => renderer,
            None => Renderer::software()?,
        };
        let scale = self.scale();
        let now = self.now();
        let painted = unsafe {
            renderer.dc.BindDC(dc, &bounds).and_then(|()| {
                renderer.dc.BeginDraw();
                renderer
                    .dc
                    .SetTransform(&windows_numerics::Matrix3x2::identity());
                let painted = D2dCanvas::new(&renderer, &renderer.dc)
                    .and_then(|mut canvas| self.panel.paint(&mut canvas, scale, now));
                let ended = renderer.dc.EndDraw(None, None);
                painted.and(ended)
            })
        };
        // A lost render target is rebuilt by the next paint.
        if painted.is_ok() {
            self.painter = Some(renderer);
        }
        painted
    }
}

impl App {
    /// The two pointer shortcuts as the window writes them, "(in use)" when another program
    /// holds one.
    fn pointer_shortcut_labels(&self) -> [String; 2] {
        let pointer = &self.settings.presentation.pointer;
        let conflicts = self
            .presentation
            .as_ref()
            .map_or([false; 2], |p| p.borrow().pointer_conflicts);
        let language = shotoshot_text::language();
        [pointer.click_shortcut, pointer.spotlight_shortcut]
            .into_iter()
            .zip(conflicts)
            .map(|(shortcut, conflict)| {
                shortcut.map_or_else(String::new, |s| {
                    let keys = shortcut_text(s.modifiers, s.key);
                    if conflict {
                        Text::with_in(language, Key::MenuShortcutInUse, &[("shortcut", &keys)])
                            .to_string()
                    } else {
                        keys
                    }
                })
            })
            .collect::<Vec<_>>()
            .try_into()
            .unwrap_or_default()
    }
    pub(super) fn open_pointer_window(&mut self, owner: HWND) -> Result<()> {
        if let Some(window) = &self.pointer_window {
            unsafe {
                let _ = ShowWindow(window.hwnd(), SW_RESTORE);
                let _ = SetForegroundWindow(window.hwnd());
            }
            return Ok(());
        }
        if self.persist_preferences {
            self.reload_settings()?;
        }
        let language = shotoshot_text::language();
        let mut panel = Panel::new(
            self.settings.presentation.pointer.clone(),
            self.settings.presentation.halo_color,
            self.halo_on(),
            language,
        );
        panel.shortcuts = self.pointer_shortcut_labels();
        let title = format!("{} - Shotoshot", tr_in(language, Key::PointerTitle));
        unsafe {
            let instance = GetModuleHandleW(None)?;
            if RegisterClassW(&WNDCLASSW {
                lpfnWndProc: Some(pointer_proc),
                hInstance: instance.into(),
                lpszClassName: CLASS,
                hCursor: LoadCursorW(None, IDC_ARROW)?,
                ..Default::default()
            }) == 0
                && GetLastError() != ERROR_CLASS_ALREADY_EXISTS
            {
                return Err(Error::from_thread());
            }
            let mut cursor = POINT::default();
            let _ = GetCursorPos(&mut cursor);
            let window = OwnedWindow(CreateWindowExW(
                WS_EX_LEFT,
                CLASS,
                &HSTRING::from(title),
                WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX,
                cursor.x,
                cursor.y,
                100,
                100,
                Some(owner),
                None,
                Some(instance.into()),
                None,
            )?);
            let dark: i32 = 1;
            let _ = windows::Win32::Graphics::Dwm::DwmSetWindowAttribute(
                window.0,
                windows::Win32::Graphics::Dwm::DWMWA_USE_IMMERSIVE_DARK_MODE,
                (&dark as *const i32).cast(),
                std::mem::size_of_val(&dark) as u32,
            );
            let mut opened = PointerWindow {
                window,
                panel,
                painter: None,
                clock: std::time::Instant::now(),
                pending: false,
            };
            opened.fit()?;
            if SetTimer(Some(opened.hwnd()), PREVIEW_TIMER, 16, None) == 0 {
                return Err(Error::from_thread());
            }
            let _ = ShowWindow(opened.hwnd(), SW_SHOW);
            let _ = SetForegroundWindow(opened.hwnd());
            self.pointer_window = Some(opened);
        }
        Ok(())
    }
    pub(super) fn close_pointer_window(&mut self) {
        if let Some(window) = self.pointer_window.take() {
            unsafe {
                let _ = KillTimer(Some(window.hwnd()), PREVIEW_TIMER);
            }
            // Dropping the window destroys it; the painter goes with it.
        }
    }
    /// Whether the halo is on screen now.
    fn halo_on(&self) -> bool {
        self.presentation
            .as_ref()
            .is_some_and(|presentation| presentation.borrow().enabled.halo)
    }
    /// Turns the halo on or off as the open window shows it, and the menu with it.
    fn apply_halo(&mut self) -> Result<()> {
        let Some(wanted) = self
            .pointer_window
            .as_ref()
            .map(|window| window.panel.halo_on)
        else {
            return Ok(());
        };
        if wanted != self.halo_on() {
            if self.worker_busy() {
                // The menu refuses the same switch now; the window shows what stays.
                if let Some(window) = &mut self.pointer_window {
                    window.panel.halo_on = !wanted;
                }
                return Ok(());
            }
            if let Some(presentation) = &self.presentation {
                let mut presentation = presentation.borrow_mut();
                let mut next = presentation.enabled;
                next.halo = wanted;
                presentation.set_enabled(next)?;
            }
        }
        if let Some(menu) = &self.menu {
            menu.update(self.menu_values()?)?;
        }
        Ok(())
    }
    /// Shows in the open window whether the halo is on, after the menu switched it.
    pub(super) fn refresh_pointer_halo(&mut self) {
        let on = self.halo_on();
        if let Some(window) = &mut self.pointer_window
            && window.panel.halo_on != on
        {
            window.panel.halo_on = on;
            invalidate(window.hwnd());
        }
    }
    /// Applies the panel's settings to the live pointer, and saves them when `save`.
    fn apply_pointer(&mut self, save: bool) -> Result<()> {
        self.apply_halo()?;
        let Some(window) = &self.pointer_window else {
            return Ok(());
        };
        let mut next = self.settings.clone();
        next.presentation.pointer = window.panel.settings.clone().clamped();
        next.presentation.halo_color = window.panel.halo_color;
        if save && self.persist_preferences {
            next.save().map_err(crate::settings::unsaved)?;
        }
        self.settings = next;
        if let Some(presentation) = &self.presentation {
            presentation
                .borrow_mut()
                .update_preferences(self.settings.presentation.clone())?;
        }
        self.refresh_pointer_shortcuts();
        Ok(())
    }
    /// Shows the current pointer shortcuts and their conflicts in the open window.
    pub(super) fn refresh_pointer_shortcuts(&mut self) {
        let labels = self.pointer_shortcut_labels();
        if let Some(window) = &mut self.pointer_window {
            window.panel.settings.click_shortcut =
                self.settings.presentation.pointer.click_shortcut;
            window.panel.settings.spotlight_shortcut =
                self.settings.presentation.pointer.spotlight_shortcut;
            if window.panel.shortcuts != labels {
                window.panel.shortcuts = labels;
                // A longer shortcut may need a wider field.
                let _ = window.fit();
            }
            invalidate(window.hwnd());
        }
    }
    /// Saves the shortcut recorded for pointer row `index` (0 shows a click, 1 holds the
    /// spotlight). It needs Ctrl or Alt and must not be another Shotoshot shortcut.
    pub(super) fn save_pointer_shortcut(
        &mut self,
        hwnd: HWND,
        index: usize,
        mods: u32,
        key: u32,
    ) -> Result<()> {
        let shortcut = crate::settings::Shortcut {
            modifiers: mods,
            key,
        };
        if mods & 3 == 0 || mods > 15 || !(1..=254).contains(&key) {
            self.setup_ui.error = self.setup_ui.text(Key::SetupPointerNeedsModifier);
            return Ok(());
        }
        let pointer = &self.settings.presentation.pointer;
        let other = if index == 0 {
            pointer.spotlight_shortcut
        } else {
            pointer.click_shortcut
        };
        if (self.settings.modifiers == mods && self.settings.key == key)
            || self.settings.instant_shortcut == shortcut
            || self.settings.board_shortcut == shortcut
            || self
                .settings
                .presentation
                .shape_shortcuts
                .contains(&shortcut)
            || other == Some(shortcut)
            || (mods == 3 && key == 27)
        {
            self.setup_ui.error = self.setup_ui.text(Key::SetupUsedByShotoshot);
            return Ok(());
        }
        // A shortcut is a change made in the window too: what it sets is meant to be seen.
        if let Some(window) = &mut self.pointer_window {
            window.panel.halo_on = true;
        }
        self.apply_halo()?;
        let mut next = self.settings.clone();
        if index == 0 {
            next.presentation.pointer.click_shortcut = Some(shortcut);
        } else {
            next.presentation.pointer.spotlight_shortcut = Some(shortcut);
        }
        if self.persist_preferences {
            next.save().map_err(crate::settings::unsaved)?;
        }
        self.settings = next;
        if let Some(presentation) = &self.presentation {
            presentation
                .borrow_mut()
                .update_preferences(self.settings.presentation.clone())?;
        }
        self.close_setup(hwnd)?;
        self.refresh_pointer_shortcuts();
        Ok(())
    }
    fn pointer_action(&mut self, owner: HWND, action: Action) -> Result<()> {
        match action {
            Action::None => {}
            Action::Repaint => {}
            Action::Changed { save: false, .. } => {
                self.apply_halo()?;
                if let Some(window) = &mut self.pointer_window {
                    window.pending = true;
                }
            }
            Action::Changed { save, sound } => {
                self.apply_pointer(save)?;
                if let Some(window) = &mut self.pointer_window {
                    window.pending = false;
                }
                if sound {
                    let pointer = &self.settings.presentation.pointer;
                    crate::presentation::play_click_sound(pointer.sound, pointer.volume);
                }
            }
            Action::EditShortcut(index) => {
                let target = if index == 0 {
                    setup::ShortcutTarget::ShowClick
                } else {
                    setup::ShortcutTarget::Spotlight
                };
                self.show_shortcut_setup(owner, target)?;
            }
            Action::Close => self.close_pointer_window(),
        }
        if let Some(window) = &self.pointer_window {
            invalidate(window.hwnd());
        }
        Ok(())
    }
    /// A message of the pointer window; None lets the system handle it.
    fn pointer_message(
        &mut self,
        owner: HWND,
        hwnd: HWND,
        msg: u32,
        w: WPARAM,
        l: LPARAM,
    ) -> Result<Option<LRESULT>> {
        let Some(window) = &mut self.pointer_window else {
            return Ok(None);
        };
        if window.hwnd() != hwnd {
            return Ok(None);
        }
        let action = match msg {
            WM_PAINT => unsafe {
                let mut paint = PAINTSTRUCT::default();
                let hdc = BeginPaint(hwnd, &mut paint);
                let mut rect = RECT::default();
                let _ = GetClientRect(hwnd, &mut rect);
                let result = window.draw(hdc, rect);
                let _ = EndPaint(hwnd, &paint);
                result?;
                return Ok(Some(LRESULT(0)));
            },
            WM_ERASEBKGND => return Ok(Some(LRESULT(1))),
            WM_TIMER if w.0 == PREVIEW_TIMER => {
                let now = window.now();
                // A minimised window paints nothing: its preview waits.
                if unsafe { !IsIconic(hwnd).as_bool() } && window.panel.tick(now) {
                    invalidate(hwnd);
                }
                if std::mem::take(&mut window.pending) {
                    self.apply_pointer(false)?;
                }
                return Ok(Some(LRESULT(0)));
            }
            WM_LBUTTONDOWN => {
                unsafe {
                    SetCapture(hwnd);
                }
                let p = window.logical(l);
                window.panel.press(p)
            }
            WM_MOUSEMOVE if unsafe { GetCapture() } == hwnd => {
                let p = window.logical(l);
                window.panel.drag(p)
            }
            WM_LBUTTONUP => {
                unsafe {
                    let _ = ReleaseCapture();
                }
                let p = window.logical(l);
                window.panel.release(p)
            }
            WM_MOUSEWHEEL => {
                let mut screen = POINT {
                    x: (l.0 as u16 as i16) as i32,
                    y: ((l.0 >> 16) as u16 as i16) as i32,
                };
                unsafe {
                    let _ = ScreenToClient(hwnd, &mut screen);
                }
                let scale = window.scale();
                let p = Point {
                    x: screen.x as f64 / scale,
                    y: screen.y as f64 / scale,
                };
                let delta = (w.0 >> 16) as u16 as i16;
                window.panel.wheel(p, (delta / 120).into())
            }
            WM_KEYDOWN => {
                let shift = unsafe { GetKeyState(VK_SHIFT.0 as i32) } < 0;
                let input = match VIRTUAL_KEY(w.0 as u16) {
                    VK_TAB if shift => Some(KeyInput::Previous),
                    VK_TAB => Some(KeyInput::Next),
                    VK_UP => Some(KeyInput::Up),
                    VK_DOWN => Some(KeyInput::Down),
                    VK_LEFT => Some(KeyInput::Left),
                    VK_RIGHT => Some(KeyInput::Right),
                    VK_SPACE | VK_RETURN => Some(KeyInput::Activate),
                    VK_DELETE | VK_BACK => Some(KeyInput::Delete),
                    VK_ESCAPE => Some(KeyInput::Escape),
                    _ => None,
                };
                match input {
                    Some(input) => window.panel.key(input),
                    None => return Ok(None),
                }
            }
            WM_DPICHANGED => {
                // The layout is logical: only the window's pixel size follows the new scale.
                let suggested = unsafe { &*(l.0 as *const RECT) };
                unsafe {
                    let _ = SetWindowPos(
                        hwnd,
                        None,
                        suggested.left,
                        suggested.top,
                        suggested.right - suggested.left,
                        suggested.bottom - suggested.top,
                        SWP_NOZORDER | SWP_NOACTIVATE,
                    );
                }
                let _ = window.place();
                invalidate(hwnd);
                return Ok(Some(LRESULT(0)));
            }
            WM_CLOSE => Action::Close,
            _ => return Ok(None),
        };
        self.pointer_action(owner, action)?;
        Ok(Some(LRESULT(0)))
    }
}

unsafe extern "system" fn pointer_proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    // PITFALL: No unwind may cross the system callback. The App lives behind the owner window;
    // a message that arrives while App is busy is left to the system.
    let outcome = std::panic::catch_unwind(|| unsafe {
        let owner = GetWindow(hwnd, GW_OWNER).unwrap_or_default();
        let app = if owner.is_invalid() {
            std::ptr::null()
        } else {
            GetWindowLongPtrW(owner, GWLP_USERDATA) as *const RefCell<App>
        };
        if !app.is_null()
            && let Ok(mut app) = (*app).try_borrow_mut()
        {
            match app.pointer_message(owner, hwnd, msg, w, l) {
                Ok(Some(value)) => return value,
                Ok(None) => {}
                Err(error) => {
                    report(hwnd, &error);
                    return LRESULT(0);
                }
            }
        }
        DefWindowProcW(hwnd, msg, w, l)
    });
    outcome.unwrap_or(LRESULT(0))
}

/// The window as English shows it with lively choices, for the rendered previews.
pub(super) fn preview() -> Result<()> {
    use shotoshot_core::pointer::{
        ClickStyle, HaloShape, PointerSettings, SpotlightShape, TrailStyle,
    };
    let mut panel = Panel::new(
        PointerSettings {
            halo_shape: HaloShape::Disc,
            halo_diameter: 64,
            halo_softness: 60,
            trail: TrailStyle::Line,
            trail_length: shotoshot_core::pointer::TrailLength::Long,
            click: ClickStyle::Waves,
            spotlight: SpotlightShape::Horizontal,
            ..Default::default()
        },
        [255, 211, 64],
        true,
        shotoshot_text::Language::En,
    );
    let renderer = Renderer::software()?;
    panel.fit(|text, size| {
        renderer
            .measure_text_in(text, size as f32, shotoshot_text::Language::En)
            .map(|extent| extent.x)
    })?;
    // A fixed moment of the preview animation: the same picture on every run.
    for now in (0..2100).step_by(16) {
        panel.tick(now);
    }
    let size = panel.layout.size;
    let (width, height) = (size.x as u32, size.y as u32);
    let image = paint_image(width, height, |dc| unsafe {
        renderer.dc.BindDC(
            dc,
            &RECT {
                right: width as i32,
                bottom: height as i32,
                ..Default::default()
            },
        )?;
        renderer.dc.BeginDraw();
        let painted = D2dCanvas::new(&renderer, &renderer.dc)
            .and_then(|mut canvas| panel.paint(&mut canvas, 1., 2100));
        painted.and(renderer.dc.EndDraw(None, None))
    })?;
    save_png(
        &image,
        &std::path::Path::new("target").join("shotoshot-pointer-effects.png"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use shotoshot_core::pointer::PointerSettings;
    use shotoshot_text::Language;

    #[test]
    fn pointer_panel_fits_every_language() {
        let _com = crate::ComApartment::new().unwrap();
        let renderer = Renderer::software().unwrap();
        for language in Language::ALL {
            let mut panel = Panel::new(PointerSettings::default(), [255, 211, 64], true, language);
            // The longest shortcut the field may show.
            panel.shortcuts = [shortcut_text(15, 0x7b), shortcut_text(15, 0x2e)];
            let measure = |text: &str, size: f64| {
                renderer
                    .measure_text_in(text, size as f32, language)
                    .map(|extent| extent.x)
            };
            panel.fit(measure).unwrap();
            assert_eq!(
                panel.overflows(measure).unwrap(),
                Vec::<String>::new(),
                "{language:?}"
            );
            let size = panel.layout.size;
            // A 1366 x 768 screen at 100 %, or a 1920 x 1080 one at 150 %, keeps it whole.
            assert!(size.x <= 1280. && size.y <= 640., "{language:?} {size:?}");
        }
    }
}
