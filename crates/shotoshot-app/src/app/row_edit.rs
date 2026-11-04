//! ROLE: A shortcut changed in its own row of Settings, with no window of its own (E1, chosen by
//! Mickael on 2026-11-04): the row listens, a valid key is saved at once, Esc or a click
//! elsewhere leaves it as it was, the row's buttons give the default keys back, and the keys
//! from before stay one click away until the page changes.
//! The checks and the saving are the shortcut window's own (`setup.rs`): only where it shows
//! differs.

use super::*;
use shotoshot_ui::settings_panel::{Panel, RowEdit, ShortcutRow};

/// The modifier keys alone: they wait for their companion key.
const MODIFIERS: [u32; 11] = [16, 17, 18, 91, 92, 160, 161, 162, 163, 164, 165];

fn target(row: ShortcutRow) -> setup::ShortcutTarget {
    use setup::ShortcutTarget as T;
    match row {
        ShortcutRow::Capture => T::Capture,
        ShortcutRow::Instant => T::Instant,
        ShortcutRow::Board => T::Board,
        ShortcutRow::Arrow => T::Arrow,
        ShortcutRow::Rectangle => T::Rectangle,
        ShortcutRow::Ellipse => T::Ellipse,
        ShortcutRow::ShowClick => T::ShowClick,
        ShortcutRow::Spotlight => T::Spotlight,
        ShortcutRow::Loupe => T::Loupe,
    }
}

/// The keys `target` has on a fresh install; None for a pointer shortcut, which has none.
fn default_keys(target: setup::ShortcutTarget) -> Option<(u32, u32)> {
    let defaults = crate::settings::Settings::default();
    let pair = |s: crate::settings::Shortcut| Some((s.modifiers, s.key));
    match target {
        setup::ShortcutTarget::Capture => Some((0, crate::print_screen::KEY)),
        setup::ShortcutTarget::Instant => pair(defaults.instant_shortcut),
        setup::ShortcutTarget::Board => pair(defaults.board_shortcut),
        other => match other.shape_index() {
            Some(index) => pair(defaults.presentation.shape_shortcuts[index]),
            None => None,
        },
    }
}

impl App {
    fn row_panel(&mut self) -> Option<&mut Panel> {
        self.settings_ui
            .window
            .as_mut()
            .map(|window| &mut window.panel)
    }
    fn repaint_settings(&self) {
        if let Some(window) = &self.settings_ui.window {
            invalidate(window.hwnd());
        }
    }
    fn editing_row(&self) -> Option<ShortcutRow> {
        let window = self.settings_ui.window.as_ref()?;
        window.panel.editing.as_ref().map(|edit| edit.row)
    }

    /// Starts listening in `row`; a row already listening is left as it was.
    pub(super) fn row_edit_begin(&mut self, owner: HWND, row: ShortcutRow) -> Result<()> {
        if self.setup {
            if !self.setup_ui.inline {
                // The shortcut window of the first run is open: it keeps the keyboard.
                return Ok(());
            }
            self.row_edit_cancel(owner)?;
        }
        if self.persist_preferences {
            self.reload_settings()?;
        }
        if let Some(presentation) = &self.presentation {
            presentation.borrow_mut().suspend_shortcuts()?;
        }
        let target = target(row);
        self.setup = true;
        self.setup_ui = setup::Setup::change(target, self.current_shortcut(target));
        self.setup_ui.inline = true;
        self.setup_ui.language = shotoshot_text::language();
        self.settings_ui.before = None;
        if let Some(panel) = self.row_panel() {
            panel.before = None;
            panel.editing = Some(RowEdit {
                row,
                held: Vec::new(),
                error: String::new(),
            });
        }
        self.repaint_settings();
        Ok(())
    }

    /// A key pressed while a row listens: Esc leaves it, Backspace gives the default keys,
    /// any other key with the modifiers held is the new shortcut.
    pub(super) fn row_edit_key(&mut self, owner: HWND, key: u32) -> Result<()> {
        let Some(row) = self.editing_row() else {
            return Ok(());
        };
        match key {
            27 => self.row_edit_cancel(owner),
            8 => self.row_edit_default(owner, row),
            // Print Screen may arrive only on release.
            k if k == VK_SNAPSHOT.0 as u32 => Ok(()),
            _ => self.row_edit_record(owner, super::input::held_modifiers(), key),
        }
    }
    pub(super) fn row_edit_key_up(&mut self, owner: HWND, key: u32) -> Result<()> {
        if key == VK_SNAPSHOT.0 as u32 {
            return self.row_edit_record(owner, super::input::held_modifiers(), key);
        }
        self.show_held(super::input::held_modifiers());
        Ok(())
    }
    fn show_held(&mut self, mods: u32) {
        let language = self.setup_ui.language;
        let held = super::setup::modifier_labels_in(mods, language)
            .map(String::from)
            .collect();
        if let Some(edit) = self.row_panel().and_then(|panel| panel.editing.as_mut()) {
            edit.held = held;
        }
        self.repaint_settings();
    }

    /// `mods` and `key` arrived in the listening row: saved at once when valid.
    pub(super) fn row_edit_record(&mut self, owner: HWND, mods: u32, key: u32) -> Result<()> {
        if MODIFIERS.contains(&key) {
            self.show_held(mods);
            return Ok(());
        }
        self.setup_ui.recording = true;
        self.setup_ui.record(mods, key);
        self.row_edit_commit(owner)
    }

    /// Saves the keys pending in the listening row, with every check of the shortcut window.
    fn row_edit_commit(&mut self, owner: HWND) -> Result<()> {
        let Some(row) = self.editing_row() else {
            return Ok(());
        };
        let before = self.current_shortcut(target(row));
        self.setup_activate(owner, 2)?;
        if self.setup {
            // Refused: the row says why and listens for other keys.
            self.setup_ui.recording = true;
            let error = self.setup_ui.error.clone();
            if let Some(edit) = self.row_panel().and_then(|panel| panel.editing.as_mut()) {
                edit.error = error;
                edit.held.clear();
            }
        } else {
            self.row_edit_saved(row, before);
        }
        self.repaint_settings();
        Ok(())
    }
    /// The row stops listening and offers its keys from `before` back.
    fn row_edit_saved(&mut self, row: ShortcutRow, before: Option<(u32, u32)>) {
        let changed = before != self.current_shortcut(target(row));
        let language = shotoshot_text::language();
        let caps = super::settings_window::keys_in(
            before.map(|(modifiers, key)| crate::settings::Shortcut { modifiers, key }),
            false,
            language,
        )
        .caps;
        self.settings_ui.before = changed.then_some((row, before));
        if let Some(panel) = self.row_panel() {
            panel.editing = None;
            panel.before = changed.then_some((row, caps));
            panel.arrange();
        }
    }

    /// Leaves the listening row as it was.
    pub(super) fn row_edit_cancel(&mut self, owner: HWND) -> Result<()> {
        if let Some(panel) = self.row_panel() {
            panel.editing = None;
        }
        if self.setup && self.setup_ui.inline {
            self.close_setup(owner)?;
        }
        self.repaint_settings();
        Ok(())
    }

    /// Gives `row` its keys of a fresh install; a pointer shortcut, which has none, is cleared.
    pub(super) fn row_edit_default(&mut self, owner: HWND, row: ShortcutRow) -> Result<()> {
        self.row_edit_set(owner, row, default_keys(target(row)))
    }
    /// Gives `row` back its keys from before its last change.
    pub(super) fn row_edit_undo(&mut self, owner: HWND, row: ShortcutRow) -> Result<()> {
        let Some((_, before)) = self.settings_ui.before.filter(|(r, _)| *r == row) else {
            return Ok(());
        };
        self.row_edit_set(owner, row, before)?;
        // Undone: nothing more to give back.
        self.settings_ui.before = None;
        if let Some(panel) = self.row_panel() {
            panel.before = None;
        }
        self.repaint_settings();
        Ok(())
    }
    fn row_edit_set(
        &mut self,
        owner: HWND,
        row: ShortcutRow,
        keys: Option<(u32, u32)>,
    ) -> Result<()> {
        if self.editing_row() != Some(row) {
            self.row_edit_begin(owner, row)?;
        }
        match keys {
            Some(pending) => {
                self.setup_ui.pending = Some(pending);
                self.setup_ui.recording = false;
                self.row_edit_commit(owner)
            }
            None => {
                let before = self.current_shortcut(target(row));
                self.row_edit_cancel(owner)?;
                let action = match self.row_panel() {
                    Some(panel) => panel.clear_pointer_shortcut(row),
                    None => return Ok(()),
                };
                self.settings_action(owner, action)?;
                self.row_edit_saved(row, before);
                self.repaint_settings();
                Ok(())
            }
        }
    }
}
