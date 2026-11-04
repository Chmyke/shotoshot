//! ROLE: A shortcut changed in its own row of Settings, with no window of its own (E1, chosen by
//! Mickael on 2026-11-04): the row listens, a valid key is saved at once, Esc or a click
//! elsewhere leaves it as it was, the row's buttons give the default keys back, and the keys
//! from before stay one click away until the page changes.
//! The checks and the saving are the shortcut window's own (`setup.rs`): only where it shows
//! differs. The notification menu changes its shortcut rows the same way (`Place::Menu`).

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

/// Where a shortcut row changes in place.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Place {
    Settings,
    Menu,
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
    /// The row listening in `place`, as it shows.
    fn edit_view(&self, place: Place) -> Option<RowEdit> {
        match place {
            Place::Settings => self.settings_ui.window.as_ref()?.panel.editing.clone(),
            Place::Menu => self.menu.as_ref()?.editing(),
        }
    }
    fn set_edit_view(&mut self, place: Place, edit: Option<RowEdit>) {
        match place {
            Place::Settings => {
                if let Some(panel) = self.row_panel() {
                    panel.editing = edit;
                }
                self.repaint_settings();
            }
            Place::Menu => {
                if let Some(menu) = &self.menu {
                    menu.set_editing(edit);
                }
            }
        }
    }
    /// Shows the undo button of a row, with its keys from before, or none anywhere.
    fn set_before_view(&mut self, shown: Option<(Place, ShortcutRow, Vec<String>)>) {
        let (in_settings, in_menu) = match shown {
            Some((Place::Settings, row, caps)) => (Some((row, caps)), None),
            Some((Place::Menu, row, _)) => (None, Some(row)),
            None => (None, None),
        };
        if let Some(panel) = self.row_panel() {
            panel.before = in_settings;
            panel.arrange();
        }
        if let Some(menu) = &self.menu {
            menu.set_before(in_menu);
        }
        self.repaint_settings();
    }
    pub(super) fn editing_row(&self) -> Option<(Place, ShortcutRow)> {
        [Place::Settings, Place::Menu]
            .into_iter()
            .find_map(|place| Some((place, self.edit_view(place)?.row)))
    }

    /// Starts listening in `row` of `place`; a row already listening is left as it was.
    pub(super) fn row_edit_begin(
        &mut self,
        owner: HWND,
        place: Place,
        row: ShortcutRow,
    ) -> Result<()> {
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
        self.set_before_view(None);
        self.set_edit_view(
            place,
            Some(RowEdit {
                row,
                held: Vec::new(),
                error: String::new(),
            }),
        );
        Ok(())
    }

    /// A key pressed while a row listens: Esc leaves it, Backspace gives the default keys,
    /// any other key with the modifiers held is the new shortcut.
    pub(super) fn row_edit_key(&mut self, owner: HWND, key: u32) -> Result<()> {
        let Some((place, row)) = self.editing_row() else {
            return Ok(());
        };
        match key {
            27 => self.row_edit_cancel(owner),
            8 => self.row_edit_default(owner, place, row),
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
    /// Changes the listening row as it shows.
    fn update_edit_view(&mut self, change: impl FnOnce(&mut RowEdit)) {
        let Some((place, _)) = self.editing_row() else {
            return;
        };
        if let Some(mut edit) = self.edit_view(place) {
            change(&mut edit);
            self.set_edit_view(place, Some(edit));
        }
    }
    fn show_held(&mut self, mods: u32) {
        let language = self.setup_ui.language;
        let held = super::setup::modifier_labels_in(mods, language)
            .map(String::from)
            .collect();
        self.update_edit_view(|edit| edit.held = held);
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
        let Some((place, row)) = self.editing_row() else {
            return Ok(());
        };
        let before = self.current_shortcut(target(row));
        self.setup_activate(owner, 2)?;
        if self.setup {
            // Refused: the row says why and listens for other keys.
            self.setup_ui.recording = true;
            let error = self.setup_ui.error.clone();
            self.update_edit_view(|edit| {
                edit.error = error;
                edit.held.clear();
            });
        } else {
            self.row_edit_saved(place, row, before)?;
        }
        self.repaint_settings();
        Ok(())
    }
    /// The row stops listening and offers its keys from `before` back.
    fn row_edit_saved(
        &mut self,
        place: Place,
        row: ShortcutRow,
        before: Option<(u32, u32)>,
    ) -> Result<()> {
        let changed = before != self.current_shortcut(target(row));
        let language = shotoshot_text::language();
        let caps = super::settings_window::keys_in(
            before.map(|(modifiers, key)| crate::settings::Shortcut { modifiers, key }),
            false,
            language,
        )
        .caps;
        self.settings_ui.before = changed.then_some((place, row, before));
        self.set_edit_view(place, None);
        self.set_before_view(changed.then_some((place, row, caps)));
        if let Some(menu) = &self.menu {
            // The menu shows the new keys.
            menu.update(self.menu_values()?)?;
        }
        Ok(())
    }

    /// Leaves the listening row as it was.
    pub(super) fn row_edit_cancel(&mut self, owner: HWND) -> Result<()> {
        for place in [Place::Settings, Place::Menu] {
            if self.edit_view(place).is_some() {
                self.set_edit_view(place, None);
            }
        }
        if self.setup && self.setup_ui.inline {
            self.close_setup(owner)?;
        }
        self.repaint_settings();
        Ok(())
    }

    /// Gives `row` its keys of a fresh install; a pointer shortcut, which has none, is cleared.
    pub(super) fn row_edit_default(
        &mut self,
        owner: HWND,
        place: Place,
        row: ShortcutRow,
    ) -> Result<()> {
        self.row_edit_set(owner, place, row, default_keys(target(row)))
    }
    /// Gives `row` back its keys from before its last change.
    pub(super) fn row_edit_undo(
        &mut self,
        owner: HWND,
        place: Place,
        row: ShortcutRow,
    ) -> Result<()> {
        let Some((_, _, before)) = self
            .settings_ui
            .before
            .filter(|(p, r, _)| *p == place && *r == row)
        else {
            return Ok(());
        };
        self.row_edit_set(owner, place, row, before)?;
        // Undone: nothing more to give back.
        self.settings_ui.before = None;
        self.set_before_view(None);
        Ok(())
    }
    fn row_edit_set(
        &mut self,
        owner: HWND,
        place: Place,
        row: ShortcutRow,
        keys: Option<(u32, u32)>,
    ) -> Result<()> {
        if self.editing_row() != Some((place, row)) {
            self.row_edit_begin(owner, place, row)?;
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
                self.row_edit_saved(place, row, before)?;
                self.repaint_settings();
                Ok(())
            }
        }
    }

    /// A shortcut row of the notification menu changed in place: its menu id and what happened.
    pub(super) fn menu_edit(&mut self, owner: HWND, id: u32, op: tray_menu::EditOp) -> Result<()> {
        use tray_menu::EditOp;
        let Some(row) = tray_menu::shortcut_row(id) else {
            return Ok(());
        };
        if self.menu.is_none() {
            return Ok(());
        }
        match op {
            EditOp::Begin => self.row_edit_begin(owner, Place::Menu, row),
            EditOp::KeyDown(key) => self.row_edit_key(owner, key),
            EditOp::KeyUp(key) => self.row_edit_key_up(owner, key),
            EditOp::Default => self.row_edit_default(owner, Place::Menu, row),
            EditOp::Cancel => self.row_edit_cancel(owner),
            EditOp::Undo => self.row_edit_undo(owner, Place::Menu, row),
        }
    }
    /// The menu goes: a row listening in it keeps its keys, and its undo button goes too.
    pub(super) fn menu_closing(&mut self, owner: HWND) {
        if matches!(self.editing_row(), Some((Place::Menu, _))) {
            let _ = self.row_edit_cancel(owner);
        }
        if matches!(self.settings_ui.before, Some((Place::Menu, ..))) {
            self.settings_ui.before = None;
        }
    }
}
