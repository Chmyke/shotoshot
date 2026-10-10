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
            Settings::save_board_visible(visible).map_err(|e| Error::new(E_FAIL, e))?;
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
    pub(super) fn menu(&mut self, hwnd: HWND) -> Result<()> {
        if self.persist_preferences {
            // A completed editor worker may have changed the last save folder.
            self.settings = Settings::load().map_err(|error| Error::new(E_FAIL, error))?;
        }
        if self.presentation.is_none() {
            self.presentation = Some(crate::presentation::Presentation::new(
                hwnd,
                self.settings.presentation.clone(),
            )?);
        }
        unsafe {
            let menu = CreatePopupMenu()?;
            let owned = crate::resources::OwnedMenu {
                handle: menu,
                background: CreateSolidBrush(notice::color(0x2b2b2b)),
            };
            let (enabled, conflicts) = self
                .presentation
                .as_ref()
                .map(|presentation| {
                    let presentation = presentation.borrow();
                    (presentation.enabled, presentation.shortcut_conflicts())
                })
                .unwrap_or_default();
            let shapes = self.settings.presentation.shape_shortcuts;
            tray_menu::build(
                menu,
                &tray_menu::Values {
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
                    sound: self.settings.instant_sound,
                    live: self.settings.live_capture,
                    startup: crate::startup::enabled()?,
                    board: self.settings.board_visible,
                    board_shortcut: tray_menu::shortcut(
                        self.settings.board_shortcut,
                        !self.board_hotkey.active(),
                    ),
                },
            )?;
            let _ = SetMenuInfo(
                menu,
                &MENUINFO {
                    cbSize: std::mem::size_of::<MENUINFO>() as u32,
                    fMask: MIM_BACKGROUND | MIM_APPLYTOSUBMENUS,
                    hbrBack: owned.background,
                    ..Default::default()
                },
            );
            let mut point = POINT::default();
            GetCursorPos(&mut point)?;
            if let Some(presentation) = &self.presentation {
                presentation
                    .borrow_mut()
                    .remember_foreground(GetForegroundWindow());
            }
            let _ = SetForegroundWindow(hwnd);
            let painter = tray_menu::Open;
            let chosen = TrackPopupMenu(
                menu,
                TPM_RETURNCMD | TPM_RIGHTBUTTON,
                point.x,
                point.y,
                Some(0),
                hwnd,
                None,
            )
            .0;
            drop(painter);
            drop(owned);
            if (101..=143).contains(&chosen) && self.worker_busy() {
                message(
                    hwnd,
                    "Finish the current capture before changing presentation settings.",
                );
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
                    if let Some(preferences) = crate::presentation::menu::preference(
                        chosen as u32,
                        &self.settings.presentation,
                    ) {
                        let mut next = self.settings.clone();
                        next.presentation = preferences.clone();
                        if self.persist_preferences {
                            next.save().map_err(|error| Error::new(E_FAIL, error))?;
                        }
                        self.settings = next;
                        if let Some(presentation) = &self.presentation {
                            presentation.borrow_mut().update_preferences(preferences)?;
                        }
                    } else if let Some(presentation) = &self.presentation {
                        presentation.borrow_mut().command(chosen as u32)?;
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
                7 => {
                    let mut next = self.settings.clone();
                    next.instant_sound = !next.instant_sound;
                    if self.persist_preferences {
                        next.save().map_err(|e| Error::new(E_FAIL, e))?;
                    }
                    self.settings = next;
                }
                4 => {
                    let enabled = !crate::startup::enabled()?;
                    crate::startup::set_enabled(enabled)?;
                    self.settings.start_with_windows = enabled;
                    if self.persist_preferences {
                        self.settings.save().map_err(|e| Error::new(E_FAIL, e))?;
                    }
                }
                5 => {
                    let previous = self.settings.live_capture;
                    self.settings.live_capture = !previous;
                    if let Err(error) = self.settings.save() {
                        self.settings.live_capture = previous;
                        return Err(Error::new(E_FAIL, error));
                    }
                }
                3 => PostQuitMessage(0),
                _ => {}
            }
            Ok(())
        }
    }
}
