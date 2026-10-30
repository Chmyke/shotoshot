//! ROLE: The Windows host of the Settings window (`shotoshot_ui::settings_panel`): its native
//! window, painting, mouse, wheel and keys, what each pointer change does to the settings, and
//! which menu command each other change runs.
//! OWNER: App holds one `SettingsUi`; the window lives only while it is open, so the resident
//! keeps nothing of it at rest but the page it showed last.
//! INVARIANTS: Every change applies at once; a pointer change is saved when it is complete (a
//! dragged slider saves on release). The window opens on the screen of the pointer, inside its
//! work area.
use super::*;
use crate::render::{D2dCanvas, Renderer};
use crate::resources::OwnedWindow;
use shotoshot_text::{Key, Language, tr_in};
use shotoshot_ui::settings_panel::{
    Action, Command, KeyInput, Keys, PAGES, Page, Panel, SHORTCUT_ROWS, ShortcutRow, Shown, Values,
};

/// The preview's frame timer.
const PREVIEW_TIMER: usize = 1;
const CLASS: PCWSTR = w!("Shotoshot.Settings");

/// The Settings window while it is open, and the page it showed last.
pub(super) struct SettingsUi {
    window: Option<SettingsWindow>,
    page: Page,
}
impl Default for SettingsUi {
    fn default() -> Self {
        Self {
            window: None,
            page: Page::General,
        }
    }
}

struct SettingsWindow {
    window: OwnedWindow,
    panel: Panel,
    painter: Option<Renderer>,
    clock: std::time::Instant,
    /// A dragged slider changed the settings: the live pointer takes them at the next frame,
    /// not at every mouse move.
    pending: bool,
}
impl SettingsWindow {
    fn hwnd(&self) -> HWND {
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
        self.panel.fit(|text, size, style| {
            renderer
                .measure_styled_text_in(text, size as f32, style, language)
                .map(|extent| extent.x)
        })?;
        self.painter = Some(renderer);
        self.place(false)
    }
    /// Sizes the window to the layout at its monitor's scale, inside that monitor's work area;
    /// `center` places it in the middle of it, else it keeps its corner where it fits.
    fn place(&self, center: bool) -> Result<()> {
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
            let (x, y) = if center {
                (
                    work.left + ((work.right - work.left - width) / 2).max(0),
                    work.top + ((work.bottom - work.top - height) / 2).max(0),
                )
            } else {
                let mut now = RECT::default();
                GetWindowRect(hwnd, &mut now)?;
                (
                    now.left.min(work.right - width).max(work.left),
                    now.top.min(work.bottom - height).max(work.top),
                )
            };
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

/// One shortcut as keycaps, in `language`.
fn keys_in(shortcut: Option<crate::settings::Shortcut>, in_use: bool, language: Language) -> Keys {
    match shortcut {
        Some(s) if s.key != 0 => Keys {
            caps: setup::key_label_in(s.modifiers, s.key, language)
                .split(" + ")
                .map(str::to_string)
                .collect(),
            in_use,
        },
        _ => Keys::default(),
    }
}

impl App {
    /// What the General, Shortcuts and Presentation pages show now, in `language`.
    fn settings_values(&self, language: Language) -> Result<Values> {
        let conflicts = self
            .presentation
            .as_ref()
            .map(|p| p.borrow().shortcut_conflicts())
            .unwrap_or_default();
        let pointer_conflicts = self
            .presentation
            .as_ref()
            .map_or([false; 3], |p| p.borrow().pointer_conflicts);
        let enabled = self.enabled();
        let s = &self.settings;
        let pointer = s.presentation.pointer.shortcuts();
        let shortcut = |row: ShortcutRow| match row {
            ShortcutRow::Capture => keys_in(
                Some(crate::settings::Shortcut {
                    modifiers: s.modifiers,
                    key: s.key,
                }),
                !self.hotkey.active(),
                language,
            ),
            ShortcutRow::Instant => keys_in(
                Some(s.instant_shortcut),
                !self.instant.hotkey.active(),
                language,
            ),
            ShortcutRow::Board => keys_in(
                Some(s.board_shortcut),
                !self.board_hotkey.active(),
                language,
            ),
            ShortcutRow::Arrow | ShortcutRow::Rectangle | ShortcutRow::Ellipse => {
                let index = row as usize - ShortcutRow::Arrow as usize;
                keys_in(
                    Some(s.presentation.shape_shortcuts[index]),
                    conflicts[index],
                    language,
                )
            }
            ShortcutRow::ShowClick | ShortcutRow::Spotlight | ShortcutRow::Loupe => {
                let index = row.pointer_index().unwrap_or(0);
                keys_in(pointer[index], pointer_conflicts[index], language)
            }
        };
        Ok(Values {
            startup: crate::startup::enabled()?,
            live: s.live_capture,
            shortcuts: SHORTCUT_ROWS.map(shortcut),
            input_scale: s.presentation.input_scale,
            duration_ms: s.presentation.duration_ms,
            moving_input: self.moving_input(),
            board: s.board_visible,
            keyboard: enabled.keys,
            mouse: enabled.mouse,
            temporary_shapes: enabled.shapes,
        })
    }
    pub(super) fn open_settings_window(&mut self, owner: HWND) -> Result<()> {
        if let Some(window) = &self.settings_ui.window {
            unsafe {
                let _ = ShowWindow(window.hwnd(), SW_RESTORE);
                let _ = SetForegroundWindow(window.hwnd());
            }
            return Ok(());
        }
        let hwnd = self.create_settings_window(owner)?;
        unsafe {
            let _ = ShowWindow(hwnd, SW_SHOW);
            let _ = SetForegroundWindow(hwnd);
        }
        Ok(())
    }
    /// Creates the Settings window, placed and running, but not yet shown.
    fn create_settings_window(&mut self, owner: HWND) -> Result<HWND> {
        if self.persist_preferences {
            self.reload_settings()?;
        }
        let language = shotoshot_text::language();
        let mut panel = Panel::new(
            self.settings.presentation.pointer.clone(),
            self.settings.presentation.halo_color,
            self.halo_on(),
            self.settings_values(language)?,
            language,
        );
        panel.loupe_on = self.enabled().loupe;
        panel.page = self.settings_ui.page;
        panel.focus = shotoshot_ui::settings_panel::Item::Nav(panel.page);
        unsafe {
            let instance = GetModuleHandleW(None)?;
            if RegisterClassW(&WNDCLASSW {
                lpfnWndProc: Some(settings_proc),
                hInstance: instance.into(),
                lpszClassName: CLASS,
                hCursor: LoadCursorW(None, IDC_ARROW)?,
                ..Default::default()
            }) == 0
                && GetLastError() != ERROR_CLASS_ALREADY_EXISTS
            {
                return Err(Error::from_thread());
            }
            // Created at the pointer: its monitor is the one the window then centres on.
            let mut cursor = POINT::default();
            let _ = GetCursorPos(&mut cursor);
            let window = OwnedWindow(CreateWindowExW(
                WS_EX_LEFT,
                CLASS,
                &HSTRING::from(title(language)),
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
            let mut opened = SettingsWindow {
                window,
                panel,
                painter: None,
                clock: std::time::Instant::now(),
                pending: false,
            };
            opened.fit()?;
            opened.place(true)?;
            if SetTimer(Some(opened.hwnd()), PREVIEW_TIMER, 16, None) == 0 {
                return Err(Error::from_thread());
            }
            let hwnd = opened.hwnd();
            self.settings_ui.window = Some(opened);
            Ok(hwnd)
        }
    }
    fn close_settings_window(&mut self) {
        // The switch that fixes the keyboard and the mouse goes with the window.
        if let Some(presentation) = &self.presentation {
            let _ = presentation.borrow_mut().end_move();
        }
        if let Some(window) = self.settings_ui.window.take() {
            self.settings_ui.page = window.panel.page;
            unsafe {
                let _ = KillTimer(Some(window.hwnd()), PREVIEW_TIMER);
            }
            // Dropping the window destroys it; the painter goes with it.
        }
    }
    /// What the presentation shows now.
    fn enabled(&self) -> crate::presentation::Enabled {
        self.presentation
            .as_ref()
            .map(|presentation| presentation.borrow().enabled)
            .unwrap_or_default()
    }
    /// Whether the keyboard and the mouse on screen can be dragged now.
    fn moving_input(&self) -> bool {
        self.presentation
            .as_ref()
            .is_some_and(|presentation| presentation.borrow().moving)
    }
    /// Whether the pointer effects are on screen now.
    fn halo_on(&self) -> bool {
        self.enabled().halo
    }
    /// Turns the pointer effects and the loupe on or off as the open window shows them, and
    /// the menu with them.
    fn apply_halo(&mut self) -> Result<()> {
        let Some((halo, loupe)) = self
            .settings_ui
            .window
            .as_ref()
            .map(|window| (window.panel.halo_on, window.panel.loupe_on))
        else {
            return Ok(());
        };
        let now = self.enabled();
        if (halo, loupe) != (now.halo, now.loupe) {
            if self.worker_busy() {
                // The menu refuses the same switches now; the window shows what stays.
                if let Some(window) = &mut self.settings_ui.window {
                    window.panel.halo_on = now.halo;
                    window.panel.loupe_on = now.loupe;
                }
                return Ok(());
            }
            if let Some(presentation) = &self.presentation {
                let mut presentation = presentation.borrow_mut();
                let mut next = presentation.enabled;
                next.halo = halo;
                next.loupe = loupe;
                presentation.set_enabled(next)?;
            }
        }
        if let Some(menu) = &self.menu {
            menu.update(self.menu_values()?)?;
        }
        Ok(())
    }
    /// Shows in the open window what is on screen, after the menu or a shortcut switched it,
    /// and whether the keyboard still moves (Enter or Esc fix it).
    pub(super) fn refresh_pointer_halo(&mut self) {
        let now = self.enabled();
        let moving = self.moving_input();
        let board = self.settings.board_visible;
        if let Some(window) = &mut self.settings_ui.window {
            let panel = &mut window.panel;
            let shown = (
                panel.halo_on,
                panel.loupe_on,
                panel.values.moving_input,
                panel.values.board,
                panel.values.keyboard,
                panel.values.mouse,
                panel.values.temporary_shapes,
            );
            if shown
                != (
                    now.halo, now.loupe, moving, board, now.keys, now.mouse, now.shapes,
                )
            {
                panel.halo_on = now.halo;
                panel.loupe_on = now.loupe;
                panel.values.moving_input = moving;
                panel.values.board = board;
                panel.values.keyboard = now.keys;
                panel.values.mouse = now.mouse;
                panel.values.temporary_shapes = now.shapes;
                invalidate(window.hwnd());
            }
        }
    }
    /// Applies the panel's pointer settings to the live pointer, and saves them when `save`.
    fn apply_pointer(&mut self, save: bool) -> Result<()> {
        self.apply_halo()?;
        let Some(window) = &self.settings_ui.window else {
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
        Ok(())
    }
    /// Shows the current settings in the open window: after a command, a recorded shortcut or
    /// a change of language from the menu.
    pub(super) fn refresh_settings_window(&mut self) -> Result<()> {
        let Some(window) = &self.settings_ui.window else {
            return Ok(());
        };
        let hwnd = window.hwnd();
        let language = shotoshot_text::language();
        let values = self.settings_values(language)?;
        let pointer = self.settings.presentation.pointer.clone();
        let Some(window) = &mut self.settings_ui.window else {
            return Ok(());
        };
        let relabel = window.panel.language != language;
        // The pointer shortcuts live in the settings, recorded by the shortcut window.
        window.panel.settings.click_shortcut = pointer.click_shortcut;
        window.panel.settings.spotlight_shortcut = pointer.spotlight_shortcut;
        window.panel.settings.loupe_shortcut = pointer.loupe_shortcut;
        if window.panel.values != values || relabel {
            window.panel.values = values;
            window.panel.language = language;
            // Another language or a longer shortcut may need a wider window.
            window.fit()?;
            if relabel {
                unsafe {
                    SetWindowTextW(hwnd, &HSTRING::from(title(language)))?;
                }
            }
        } else {
            window.panel.arrange();
        }
        invalidate(hwnd);
        Ok(())
    }
    /// Saves the shortcut recorded for pointer row `index` (0 shows a click, 1 holds the
    /// spotlight, 2 shows the loupe). It needs Ctrl or Alt and must not be another Shotoshot
    /// shortcut.
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
        let others = pointer.shortcuts();
        let taken =
            (0..others.len()).any(|other| other != index && others[other] == Some(shortcut));
        if (self.settings.modifiers == mods && self.settings.key == key)
            || self.settings.instant_shortcut == shortcut
            || self.settings.board_shortcut == shortcut
            || self
                .settings
                .presentation
                .shape_shortcuts
                .contains(&shortcut)
            || taken
            || (mods == 3 && key == 27)
        {
            self.setup_ui.error = self.setup_ui.text(Key::SetupUsedByShotoshot);
            return Ok(());
        }
        let mut next = self.settings.clone();
        *next.presentation.pointer.shortcut_mut(index) = Some(shortcut);
        if self.persist_preferences {
            next.save().map_err(crate::settings::unsaved)?;
        }
        self.settings = next;
        if let Some(presentation) = &self.presentation {
            presentation
                .borrow_mut()
                .update_preferences(self.settings.presentation.clone())?;
        }
        self.close_setup(hwnd)
    }
    /// Runs a change the window asks for: most are the menu's own commands.
    fn settings_command(&mut self, owner: HWND, command: Command) -> Result<()> {
        use crate::presentation::menu as commands;
        let menu_id = |all: &[u32], value: u32, first: u32| {
            all.iter()
                .position(|v| *v == value)
                .map(|index| first + index as u32)
        };
        match command {
            Command::Startup => self.run_menu_command(owner, 4)?,
            Command::Live => self.run_menu_command(owner, 5)?,
            Command::EditShortcut(row) => {
                let target = match row {
                    ShortcutRow::Capture => return self.show_setup(owner),
                    ShortcutRow::Instant => setup::ShortcutTarget::Instant,
                    ShortcutRow::Board => setup::ShortcutTarget::Board,
                    ShortcutRow::Arrow => setup::ShortcutTarget::Arrow,
                    ShortcutRow::Rectangle => setup::ShortcutTarget::Rectangle,
                    ShortcutRow::Ellipse => setup::ShortcutTarget::Ellipse,
                    ShortcutRow::ShowClick => setup::ShortcutTarget::ShowClick,
                    ShortcutRow::Spotlight => setup::ShortcutTarget::Spotlight,
                    ShortcutRow::Loupe => setup::ShortcutTarget::Loupe,
                };
                self.show_shortcut_setup(owner, target)?;
            }
            Command::InputScale(value) => {
                if let Some(id) = menu_id(&crate::settings::INPUT_SCALES, value, 141) {
                    self.run_menu_command(owner, id)?;
                }
            }
            Command::Duration(ms) => {
                let durations = [121, 122, 123].map(|id| commands::duration_ms(id).unwrap_or(0));
                if let Some(id) = menu_id(&durations, ms, 121) {
                    self.run_menu_command(owner, id)?;
                }
            }
            Command::PresentationDefaults => {
                let defaults = crate::settings::PresentationSettings::default();
                for (all, value, first) in [
                    (
                        crate::settings::INPUT_SCALES.to_vec(),
                        defaults.input_scale,
                        141,
                    ),
                    (
                        [121, 122, 123]
                            .map(|id| commands::duration_ms(id).unwrap_or(0))
                            .to_vec(),
                        defaults.duration_ms,
                        121,
                    ),
                ] {
                    if let Some(id) = menu_id(&all, value, first) {
                        self.run_menu_command(owner, id)?;
                    }
                }
            }
            // The window stays open: its switch fixes the keyboard and the mouse again.
            Command::MoveInput => self.run_menu_command(owner, 106)?,
            Command::Show(shown) => {
                let id = match shown {
                    Shown::Board => 9,
                    Shown::PointerEffects => 101,
                    Shown::Magnifier => 107,
                    Shown::Keyboard => 103,
                    Shown::Mouse => 104,
                    Shown::TemporaryShapes => 102,
                };
                self.run_menu_command(owner, id)?;
            }
        }
        if let Some(menu) = &self.menu {
            menu.update(self.menu_values()?)?;
        }
        self.refresh_settings_window()
    }
    fn settings_action(&mut self, owner: HWND, action: Action) -> Result<()> {
        match action {
            Action::None | Action::Repaint => {}
            Action::Changed { save: false, .. } => {
                self.apply_halo()?;
                if let Some(window) = &mut self.settings_ui.window {
                    window.pending = true;
                }
            }
            Action::Changed { save, sound } => {
                self.apply_pointer(save)?;
                if let Some(window) = &mut self.settings_ui.window {
                    window.pending = false;
                }
                if sound {
                    let pointer = &self.settings.presentation.pointer;
                    crate::presentation::play_click_sound(pointer.sound, pointer.volume);
                }
            }
            Action::Command(command) => self.settings_command(owner, command)?,
            Action::Close => self.close_settings_window(),
        }
        if let Some(window) = &self.settings_ui.window {
            invalidate(window.hwnd());
        }
        Ok(())
    }
    /// A message of the Settings window; None lets the system handle it.
    fn settings_message(
        &mut self,
        owner: HWND,
        hwnd: HWND,
        msg: u32,
        w: WPARAM,
        l: LPARAM,
    ) -> Result<Option<LRESULT>> {
        let Some(window) = &mut self.settings_ui.window else {
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
                // The loupe's shortcut switches it from anywhere: the window follows.
                self.refresh_pointer_halo();
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
                let _ = window.place(false);
                invalidate(hwnd);
                return Ok(Some(LRESULT(0)));
            }
            WM_CLOSE => Action::Close,
            _ => return Ok(None),
        };
        self.settings_action(owner, action)?;
        Ok(Some(LRESULT(0)))
    }
}

/// The window's title in `language`.
fn title(language: Language) -> String {
    format!("{} - Shotoshot", tr_in(language, Key::SettingsTitle))
}

#[cfg(test)]
mod window_tests;

unsafe extern "system" fn settings_proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
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
            match app.settings_message(owner, hwnd, msg, w, l) {
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

/// The values the rendered previews show: the shortcuts as installed, one held elsewhere.
fn preview_values(language: Language) -> Values {
    use crate::settings::Shortcut;
    let shortcut = |modifiers, key| Some(Shortcut { modifiers, key });
    let shortcuts = [
        keys_in(shortcut(0, 0x2c), false, language),
        keys_in(shortcut(0, 0x91), false, language),
        keys_in(shortcut(1, 0x42), false, language),
        keys_in(shortcut(1, 0x41), false, language),
        keys_in(shortcut(1, 0x52), true, language),
        keys_in(shortcut(1, 0x43), false, language),
        keys_in(shortcut(1, 0x4b), false, language),
        Keys::default(),
        Keys::default(),
    ];
    Values {
        shortcuts,
        ..Values::default()
    }
}

/// Every page as English shows it with lively choices, for the rendered previews.
pub(super) fn preview() -> Result<()> {
    use shotoshot_core::pointer::{
        ClickStyle, HaloShape, PointerSettings, SpotlightShape, TrailStyle,
    };
    let language = Language::En;
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
        preview_values(language),
        language,
    );
    let renderer = Renderer::software()?;
    panel.fit(|text, size, style| {
        renderer
            .measure_styled_text_in(text, size as f32, style, language)
            .map(|extent| extent.x)
    })?;
    for (page, name) in PAGES
        .into_iter()
        .zip(["general", "shortcuts", "presentation", "pointer"])
    {
        panel.show_page(page);
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
            &std::path::Path::new("target").join(format!("shotoshot-settings-{name}.png")),
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use shotoshot_core::pointer::PointerSettings;

    #[test]
    fn settings_window_fits_every_language() {
        let _com = crate::ComApartment::new().unwrap();
        let renderer = Renderer::software().unwrap();
        for language in Language::ALL {
            let mut values = preview_values(language);
            // The longest shortcuts a row may show.
            for keys in values.shortcuts.iter_mut() {
                *keys = keys_in(
                    Some(crate::settings::Shortcut {
                        modifiers: 15,
                        key: 0x7b,
                    }),
                    true,
                    language,
                );
            }
            let mut panel = Panel::new(
                PointerSettings::default(),
                [255, 211, 64],
                true,
                values,
                language,
            );
            let measure = |text: &str, size: f64, style: shotoshot_ui::TextStyle| {
                renderer
                    .measure_styled_text_in(text, size as f32, style, language)
                    .map(|extent| extent.x)
            };
            panel.fit(measure).unwrap();
            for page in PAGES {
                panel.show_page(page);
                assert_eq!(
                    panel.overflows(measure).unwrap(),
                    Vec::<String>::new(),
                    "{language:?} {page:?}"
                );
            }
            let size = panel.layout.size;
            // A 1366 x 768 screen at 100 %, or a 1920 x 1080 one at 150 %, keeps it whole.
            assert!(size.x <= 1280. && size.y <= 680., "{language:?} {size:?}");
        }
    }
}
