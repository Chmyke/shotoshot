//! ROLE: Keyboard handling of a capture session.
//! INVARIANTS: Key codes are Windows virtual-key codes, the canonical format across
//! platforms; each platform translates its own key events into them.
use crate::{Capture, Host, toolbar::Action};
use shotoshot_core::{Modifiers, Point};

pub const KEY_TAB: u32 = 9;
pub const KEY_SPACE: u32 = 32;
pub const KEY_ENTER: u32 = 13;
pub const KEY_ESCAPE: u32 = 27;
pub const KEY_PAGE_UP: u32 = 33;
pub const KEY_PAGE_DOWN: u32 = 34;
pub const KEY_LEFT: u32 = 37;
pub const KEY_UP: u32 = 38;
pub const KEY_RIGHT: u32 = 39;
pub const KEY_DOWN: u32 = 40;
pub const KEY_DELETE: u32 = 46;
const KEY_0: u32 = 48;
const KEY_NUMPAD_0: u32 = 96;
const KEY_C: u32 = 67;
const KEY_V: u32 = 86;

/// What the platform must do after a key press.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum KeyOutcome {
    /// Nothing changed; no repaint.
    Ignored,
    /// The capture changed; repaint.
    Handled,
    /// Run a toolbar action, then repaint.
    Action(Action),
    /// Ctrl+C: copy selected annotations, or the image when none is selected.
    CopyAnnotations,
    /// Ctrl+V: paste annotations copied from this session.
    PasteAnnotations,
    /// Escape with nothing pending: end the session.
    Close,
}

impl Capture {
    pub fn key<H: Host>(&mut self, host: &mut H, key: u32, m: Modifiers) -> KeyOutcome {
        if host.dimension_key(self, key) {
            return KeyOutcome::Handled;
        }
        self.editor.update_modifiers(m);
        if let Some(picker) = &self.picker {
            match key {
                KEY_ENTER => {
                    let rgb = picker.color();
                    self.picker = None;
                    self.accept_custom_color(host, rgb);
                    host.focus_text();
                }
                KEY_ESCAPE => {
                    self.cancel_gesture(host);
                    host.focus_text();
                }
                _ => {}
            }
            return KeyOutcome::Handled;
        }
        if self.eyedropper {
            if key != KEY_ESCAPE {
                return KeyOutcome::Ignored;
            }
            self.cancel_gesture(host);
            host.focus_text();
            return KeyOutcome::Handled;
        }
        if host.text_active() {
            return KeyOutcome::Ignored;
        }
        if key == KEY_TAB {
            self.toolbar_visible = !self.toolbar_visible;
            return KeyOutcome::Handled;
        }
        if key == KEY_ESCAPE {
            return if self.cancel_gesture(host) {
                KeyOutcome::Handled
            } else {
                KeyOutcome::Close
            };
        }
        if key == KEY_SPACE && !self.views.is_empty() {
            self.hold_space(true);
            return KeyOutcome::Handled;
        }
        if m.ctrl && !m.alt && !m.shift && matches!(key, KEY_0 | KEY_NUMPAD_0) && self.reset_view()
        {
            self.adopt_scale();
            return KeyOutcome::Handled;
        }
        if m.ctrl {
            let plain = !m.alt && !m.shift;
            return if plain && key == KEY_C {
                KeyOutcome::CopyAnnotations
            } else if plain && key == KEY_V {
                KeyOutcome::PasteAnnotations
            } else {
                crate::toolbar::shortcut_action(key, true, m.shift, m.alt)
                    .map_or(KeyOutcome::Handled, KeyOutcome::Action)
            };
        }
        if let Some(a) = crate::toolbar::shortcut_action(key, false, m.shift, m.alt) {
            if a == Action::Live && self.reopened {
                return KeyOutcome::Handled;
            }
            return KeyOutcome::Action(a);
        }
        let amount = if m.shift { 10. } else { 1. };
        match key {
            KEY_DELETE => self.editor.delete_selection(),
            // Through the pile, as the arrows move across the plane; Shift goes all the way.
            KEY_PAGE_UP => self.editor.raise_selection(m.shift),
            KEY_PAGE_DOWN => self.editor.lower_selection(m.shift),
            KEY_LEFT => self.editor.nudge(Point { x: -amount, y: 0. }),
            KEY_RIGHT => self.editor.nudge(Point { x: amount, y: 0. }),
            KEY_UP => self.editor.nudge(Point { x: 0., y: -amount }),
            KEY_DOWN => self.editor.nudge(Point { x: 0., y: amount }),
            _ => {}
        }
        KeyOutcome::Handled
    }
    /// A key released; only Space matters, which ends the pan modifier.
    pub fn key_up(&mut self, key: u32) {
        if key == KEY_SPACE {
            self.hold_space(false);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Image, test_host::FakeHost};
    use shotoshot_core::{Rect, Tool};

    fn capture() -> Capture {
        let mut c = Capture::new(Image::solid(900, 600, [0, 0, 0, 255]));
        c.region = Rect::new(Point { x: 100., y: 100. }, Point { x: 600., y: 400. });
        c.selecting = false;
        c
    }
    fn ctrl() -> Modifiers {
        Modifiers {
            ctrl: true,
            ..Modifiers::default()
        }
    }
    #[test]
    fn g_arms_the_bucket_and_keys_keep_working_while_it_waits() {
        let mut c = capture();
        let mut host = FakeHost::default();
        let none = Modifiers::default();
        assert_eq!(c.key(&mut host, 71, none), KeyOutcome::Action(Action::Fill));
        c.action(&mut host, Action::Fill);
        // Unlike the eyedropper, the bucket leaves the keyboard free: 6 picks its colour.
        let black = Action::Color(crate::toolbar::COLORS[5]);
        assert_eq!(c.key(&mut host, 54, none), KeyOutcome::Action(black));
        assert_eq!(c.key(&mut host, KEY_ESCAPE, none), KeyOutcome::Handled);
        assert!(!c.bucket);
        assert_eq!(c.key(&mut host, KEY_ESCAPE, none), KeyOutcome::Close);
    }
    #[test]
    fn escape_closes_only_when_nothing_is_pending() {
        let mut c = capture();
        let mut host = FakeHost::default();
        c.eyedropper = true;
        let none = Modifiers::default();
        assert_eq!(c.key(&mut host, KEY_ESCAPE, none), KeyOutcome::Handled);
        assert!(!c.eyedropper);
        assert_eq!(c.key(&mut host, KEY_ESCAPE, none), KeyOutcome::Close);
    }
    #[test]
    fn shortcuts_tab_and_clipboard_keys_map_to_outcomes() {
        let mut c = capture();
        let mut host = FakeHost::default();
        let none = Modifiers::default();
        assert_eq!(
            c.key(&mut host, 82, none),
            KeyOutcome::Action(Action::Tool(Tool::Rectangle))
        );
        assert_eq!(c.key(&mut host, KEY_C, ctrl()), KeyOutcome::CopyAnnotations);
        assert_eq!(
            c.key(&mut host, KEY_V, ctrl()),
            KeyOutcome::PasteAnnotations
        );
        assert_eq!(c.key(&mut host, KEY_TAB, none), KeyOutcome::Handled);
        assert!(!c.toolbar_visible);
    }
    #[test]
    fn arrows_nudge_the_selection_and_text_editing_swallows_keys() {
        let mut c = capture();
        let mut host = FakeHost::default();
        c.editor
            .begin(Point { x: 200., y: 200. }, Modifiers::default());
        c.editor.update(Point { x: 300., y: 300. });
        c.editor.finish();
        c.editor.select_only(c.editor.objects()[0].id);
        let shift = Modifiers {
            shift: true,
            ..Modifiers::default()
        };
        c.key(&mut host, KEY_RIGHT, shift);
        assert_eq!(c.editor.objects()[0].bounds().min.x, 210.);
        host.text_open = true;
        assert_eq!(
            c.key(&mut host, KEY_DELETE, Modifiers::default()),
            KeyOutcome::Ignored
        );
        assert_eq!(c.editor.objects().len(), 1);
    }
    #[test]
    fn page_keys_move_the_selection_through_the_pile_and_shift_sends_it_to_the_end() {
        let mut c = capture();
        let mut host = FakeHost::default();
        for (from, to) in [(200., 300.), (210., 310.), (220., 320.)] {
            c.editor
                .begin(Point { x: from, y: from }, Modifiers::default());
            c.editor.update(Point { x: to, y: to });
            c.editor.finish();
        }
        let ids = |c: &Capture| c.editor.objects().iter().map(|o| o.id).collect::<Vec<_>>();
        c.editor.select_only(1);
        let none = Modifiers::default();
        assert_eq!(c.key(&mut host, KEY_PAGE_UP, none), KeyOutcome::Handled);
        assert_eq!(ids(&c), vec![2, 1, 3]);
        let shift = Modifiers {
            shift: true,
            ..Modifiers::default()
        };
        c.key(&mut host, KEY_PAGE_UP, shift);
        assert_eq!(ids(&c), vec![2, 3, 1]);
        c.key(&mut host, KEY_PAGE_DOWN, none);
        assert_eq!(ids(&c), vec![2, 1, 3]);
        c.key(&mut host, KEY_PAGE_DOWN, shift);
        assert_eq!(ids(&c), vec![1, 2, 3]);
    }
}
