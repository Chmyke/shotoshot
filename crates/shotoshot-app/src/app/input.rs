use super::*;
use shotoshot_ui::keys::KeyOutcome;

/// Modifier keys down as of the message being handled, as a hotkey mask.
pub(super) fn held_modifiers() -> u32 {
    let mut mods = 0;
    if control() {
        mods |= MOD_CONTROL.0;
    }
    if modifiers().shift {
        mods |= MOD_SHIFT.0;
    }
    if modifiers().alt {
        mods |= MOD_ALT.0;
    }
    unsafe {
        if GetKeyState(VK_LWIN.0 as i32) < 0 || GetKeyState(VK_RWIN.0 as i32) < 0 {
            mods |= MOD_WIN.0;
        }
    }
    mods
}

impl App {
    /// Remembers whether the board is shown, here and for the other processes.
    fn save_board_visible(&mut self, visible: bool) -> Result<()> {
        if self.persist_preferences {
            Settings::save_board_visible(visible).map_err(crate::settings::unsaved)?;
        }
        self.settings.board_visible = visible;
        Ok(())
    }
    pub(super) fn key(&mut self, hwnd: HWND, key: u32) -> Result<()> {
        if self.setup {
            return self.setup_key(hwnd, key);
        }
        let Some(s) = &mut self.session else {
            return Ok(());
        };
        match s.key(hwnd, key) {
            KeyOutcome::Ignored => return Ok(()),
            KeyOutcome::Handled => {}
            KeyOutcome::Action(a) => self.action(hwnd, a)?,
            KeyOutcome::CopyAnnotations => {
                if !s.copy_annotations(hwnd)? {
                    self.action(hwnd, Action::Copy)?;
                }
            }
            KeyOutcome::PasteAnnotations => s.paste_annotations(),
            KeyOutcome::Close => {
                self.close_session(hwnd);
                return Ok(());
            }
        }
        invalidate(hwnd);
        Ok(())
    }
    pub(super) fn setup_key(&mut self, hwnd: HWND, key: u32) -> Result<()> {
        if self.setup_ui.recording {
            self.setup_ui.held = held_modifiers();
            self.setup_ui.record(self.setup_ui.held, key);
        } else {
            match key {
                9 => {
                    self.setup_ui.focus =
                        (self.setup_ui.focus + if modifiers().shift { 2 } else { 1 }) % 3
                }
                13 | 32 => self.setup_activate(hwnd, self.setup_ui.focus)?,
                27 => self.setup_activate(hwnd, 1)?,
                _ => {}
            }
        }
        invalidate(hwnd);
        Ok(())
    }
    /// Saves the language chosen in the menu and shows it at once: the resident's own texts,
    /// its icon's hint, and an editor prepared again, since a prepared one keeps its texts.
    pub(super) fn choose_language(
        &mut self,
        hwnd: HWND,
        choice: shotoshot_text::LanguageChoice,
    ) -> Result<()> {
        let mut next = self.settings.clone();
        next.language = choice;
        if self.persist_preferences {
            let path = Settings::path()?;
            let folder = path.parent().unwrap_or(&path);
            crate::language::store_menu_choice(&next, folder, |s| s.save())
                .map_err(crate::settings::unsaved)?;
        }
        self.settings = next;
        crate::language::apply(&self.settings);
        self.tray.szTip = tray_tip(shotoshot_text::tr(shotoshot_text::Key::TrayTooltip));
        if !self.tray.hWnd.is_invalid() {
            unsafe {
                let _ = Shell_NotifyIconW(NIM_MODIFY, &self.tray);
            }
        }
        self.restart_prepared_editor(hwnd);
        Ok(())
    }
    /// What the menu rows show for the current settings.
    pub(super) fn menu_values(&self) -> Result<tray_menu::Values> {
        let (enabled, conflicts) = self
            .presentation
            .as_ref()
            .map(|presentation| {
                let presentation = presentation.borrow();
                (presentation.enabled, presentation.shortcut_conflicts())
            })
            .unwrap_or_default();
        let shapes = self.settings.presentation.shape_shortcuts;
        Ok(tray_menu::Values {
            capture: tray_menu::shortcut(
                crate::settings::Shortcut {
                    modifiers: self.settings.modifiers,
                    key: self.settings.key,
                },
                !self.hotkey.active(),
            ),
            instant: tray_menu::shortcut(
                self.settings.instant_shortcut,
                !self.instant.hotkey.active(),
            ),
            shapes: std::array::from_fn(|index| {
                tray_menu::shortcut(shapes[index], conflicts[index])
            }),
            duration_ms: self.settings.presentation.duration_ms,
            layout: self.settings.presentation.input_layout,
            size: self.settings.presentation.input_scale,
            halo: enabled.halo,
            temporary_shapes: enabled.shapes,
            keys: enabled.keys,
            mouse: enabled.mouse,
            live: self.settings.live_capture,
            startup: crate::startup::enabled()?,
            board: self.settings.board_visible,
            board_shortcut: tray_menu::shortcut(
                self.settings.board_shortcut,
                !self.board_hotkey.active(),
            ),
            language: self.settings.language,
        })
    }
    /// A click on the notification icon: opens the menu, or closes the open one.
    pub(super) fn tray_click(&mut self, hwnd: HWND) -> Result<()> {
        // The press already closed the menu by taking the foreground: this release ends there.
        let closed_by_press = self
            .menu_closed_on_icon
            .take()
            .is_some_and(|at| at.elapsed() < std::time::Duration::from_secs(1));
        if self.menu.take().is_some() || closed_by_press {
            return Ok(());
        }
        self.menu(hwnd)
    }
    /// The menu window closed itself; `on_icon` when a press on the notification icon did it.
    pub(super) fn menu_closed(&mut self, root: HWND, on_icon: bool) {
        if self.menu.as_ref().is_some_and(|menu| menu.root() == root) {
            self.menu = None;
            if on_icon {
                self.menu_closed_on_icon = Some(std::time::Instant::now());
            }
        }
    }
    fn menu(&mut self, hwnd: HWND) -> Result<()> {
        if self.persist_preferences {
            // A completed editor worker may have changed the last save folder.
            self.reload_settings()?;
        }
        if self.presentation.is_none() {
            self.presentation = Some(crate::presentation::Presentation::new(
                hwnd,
                self.settings.presentation.clone(),
            )?);
        }
        let values = self.menu_values()?;
        unsafe {
            // Unknown, a press on the icon closes the menu and the release opens it again.
            let icon = Shell_NotifyIconGetRect(&NOTIFYICONIDENTIFIER {
                cbSize: std::mem::size_of::<NOTIFYICONIDENTIFIER>() as u32,
                hWnd: self.tray.hWnd,
                uID: self.tray.uID,
                ..Default::default()
            })
            .unwrap_or_default();
            let mut point = POINT::default();
            GetCursorPos(&mut point)?;
            if let Some(presentation) = &self.presentation {
                presentation
                    .borrow_mut()
                    .remember_foreground(GetForegroundWindow());
            }
            self.menu = Some(tray_menu::Popup::open(hwnd, icon, values, point, true)?);
        }
        Ok(())
    }
    /// A row chosen in the menu; `stays_open` for a setting, which the menu then shows.
    pub(super) fn menu_command(&mut self, hwnd: HWND, chosen: u32, stays_open: bool) -> Result<()> {
        if !stays_open {
            self.menu = None;
        }
        self.run_menu_command(hwnd, chosen)?;
        if let Some(menu) = &self.menu {
            menu.update(self.menu_values()?)?;
        }
        Ok(())
    }
    fn run_menu_command(&mut self, hwnd: HWND, chosen: u32) -> Result<()> {
        unsafe {
            if (101..=143).contains(&chosen) && self.worker_busy() {
                message(hwnd, &Text::new(Key::ErrorPresentationBusy));
                return Ok(());
            }
            match chosen {
                110..=112 => {
                    let target = [
                        setup::ShortcutTarget::Arrow,
                        setup::ShortcutTarget::Rectangle,
                        setup::ShortcutTarget::Ellipse,
                    ][(chosen - 110) as usize];
                    self.show_shortcut_setup(hwnd, target)?;
                }
                101..=143 => {
                    if let Some(preferences) =
                        crate::presentation::menu::preference(chosen, &self.settings.presentation)
                    {
                        let mut next = self.settings.clone();
                        next.presentation = preferences.clone();
                        if self.persist_preferences {
                            next.save().map_err(crate::settings::unsaved)?;
                        }
                        self.settings = next;
                        if let Some(presentation) = &self.presentation {
                            presentation.borrow_mut().update_preferences(preferences)?;
                        }
                    } else if let Some(presentation) = &self.presentation {
                        presentation.borrow_mut().command(chosen)?;
                    }
                }
                1 => {
                    PostMessageW(Some(hwnd), CAPTURE_MESSAGE, WPARAM(0), LPARAM(0))?;
                }
                2 => self.show_setup(hwnd)?,
                6 => self.show_shortcut_setup(hwnd, setup::ShortcutTarget::Instant)?,
                8 => super::board::spawn_session()?,
                // Every note gone, in a process of its own; it refuses while a session is open.
                10 => crate::board::clear::spawn()?,
                9 => {
                    let visible = !self.settings.board_visible;
                    // ORDER: saved first. A bake process reads it once it holds the bake's lock,
                    // so one still waiting behind `hide` leaves without showing the board again.
                    self.save_board_visible(visible)?;
                    let applied = if visible {
                        // Baked in a process of its own, like every bake the resident starts.
                        // TRADE-OFF: only the start is checked here. The switch reflects the
                        // request: a bake that then fails shows its own error, one that gives up
                        // behind a lock held for 60 s says nothing, and either leaves the switch
                        // on with nothing on screen until the next display or wallpaper event
                        // bakes again. Waiting for the result would block this thread for the
                        // bake's length; turning the setting off from the failed process would
                        // cancel that retry, which only runs for a visible board.
                        crate::board::bake::spawn()
                    } else {
                        // Quick, on this thread; waits for a running bake, up to 10 s, and for the
                        // desktop to show the base, up to 2 s.
                        crate::board::folder().and_then(|folder| {
                            crate::board::bake::hide(
                                &folder,
                                &crate::board::wallpaper::Wallpaper::new()?,
                            )
                        })
                    };
                    if let Err(error) = applied {
                        // Not done: the menu offers the same choice again.
                        self.save_board_visible(!visible)?;
                        return Err(error);
                    }
                }
                13 => self.show_shortcut_setup(hwnd, setup::ShortcutTarget::Board)?,
                4 => {
                    let enabled = !crate::startup::enabled()?;
                    crate::startup::set_enabled(enabled)?;
                    self.settings.start_with_windows = enabled;
                    if self.persist_preferences {
                        self.settings.save().map_err(crate::settings::unsaved)?;
                    }
                }
                5 => {
                    let previous = self.settings.live_capture;
                    self.settings.live_capture = !previous;
                    if let Err(error) = self.settings.save() {
                        self.settings.live_capture = previous;
                        return Err(crate::settings::unsaved(error));
                    }
                }
                3 => PostQuitMessage(0),
                _ => {
                    if let Some(choice) = tray_menu::on_language_choice(chosen) {
                        self.choose_language(hwnd, choice)?;
                    }
                }
            }
            Ok(())
        }
    }
}
