//! ROLE: Physical input state for the presentation diagram.
//! OWNER: One presentation session owns one Devices value.
//! INVARIANTS: Fixed transient labels only, no text history; E0 keys are independent.
use shotoshot_text::{Key, Language, tr_in};
const RELEASE_MS: u64 = 180;
const LABEL_FADE_MS: u64 = 600;
#[derive(Clone, Copy, Default)]
struct Button {
    down: bool,
    until: u64,
}
impl Button {
    fn update(&mut self, down: bool, now: u64) {
        if self.down && !down {
            self.until = now.saturating_add(RELEASE_MS);
        }
        self.down = down;
    }
    fn active(self, now: u64) -> bool {
        self.down || self.until > now
    }
}
pub(super) struct Devices {
    pub ansi: bool,
    keys: [Button; 256],
    labels: [[u16; 4]; 256],
    label_lengths: [u8; 256],
    mouse: [Button; 6],
}
impl Default for Devices {
    fn default() -> Self {
        Self {
            ansi: false,
            keys: [Button::default(); 256],
            labels: [[0; 4]; 256],
            label_lengths: [0; 256],
            mouse: [Button::default(); 6],
        }
    }
}
impl Devices {
    pub fn keyboard(&mut self, scan: u16, extended: bool, down: bool, now: u64) {
        if scan < 128 {
            let index = scan as usize + usize::from(extended) * 128;
            let key = &mut self.keys[index];
            if down && !key.down {
                self.labels[index] = [0; 4];
                let text = special_label_in(scan, extended, shotoshot_text::language());
                let mut length = 0;
                for unit in text.encode_utf16().take(4) {
                    self.labels[index][length] = unit;
                    length += 1;
                }
                self.label_lengths[index] = length as u8;
            }
            if key.down && !down {
                key.until = now.saturating_add(LABEL_FADE_MS);
            }
            key.down = down;
        }
    }
    pub fn mouse(&mut self, flags: u16, now: u64) {
        for index in 0..5 {
            if flags & (1 << (index * 2)) != 0 {
                self.mouse[index].update(true, now);
            }
            if flags & (2 << (index * 2)) != 0 {
                self.mouse[index].update(false, now);
            }
        }
        if flags & 0xc00 != 0 {
            self.mouse[5].until = now.saturating_add(RELEASE_MS);
        }
    }
    pub fn active(&self, scan: u16, extended: bool, _now: u64) -> bool {
        scan < 128 && self.keys[scan as usize + usize::from(extended) * 128].down
    }
    pub fn set_label(&mut self, scan: u16, extended: bool, text: &[u16]) {
        if scan >= 128 {
            return;
        }
        let index = scan as usize + usize::from(extended) * 128;
        if !self.keys[index].down {
            return;
        }
        self.labels[index] = [0; 4];
        self.label_lengths[index] = 0;
        if text.is_empty()
            || text.len() > 4
            || !char::decode_utf16(text.iter().copied())
                .all(|c| c.is_ok_and(|c| !c.is_control() && !c.is_whitespace()))
        {
            return;
        }
        self.labels[index][..text.len()].copy_from_slice(text);
        self.label_lengths[index] = text.len() as u8;
    }
    pub fn label(&self, scan: u16, extended: bool, now: u64) -> (&[u16], f32) {
        if scan >= 128 {
            return (&[], 0.);
        }
        let index = scan as usize + usize::from(extended) * 128;
        let key = self.keys[index];
        let alpha = if key.down {
            1.
        } else {
            key.until.saturating_sub(now).min(LABEL_FADE_MS) as f32 / LABEL_FADE_MS as f32
        };
        if alpha == 0. {
            return (&[], 0.);
        }
        (
            &self.labels[index][..self.label_lengths[index] as usize],
            alpha,
        )
    }
    pub fn mouse_active(&self, index: usize, now: u64) -> bool {
        self.mouse
            .get(index)
            .is_some_and(|button| button.active(now))
    }
    pub fn needs_animation(&self, now: u64) -> bool {
        self.keys
            .iter()
            .enumerate()
            .any(|(index, key)| !key.down && key.until > now && self.label_lengths[index] > 0)
            || self
                .mouse
                .iter()
                .any(|button| !button.down && button.until > now)
    }
    /// Tracks a key the diagram does not show: held, it still shapes the labels of the others.
    pub fn hold(&mut self, scan: u16, extended: bool, down: bool) {
        if scan < 128 {
            let index = scan as usize + usize::from(extended) * 128;
            self.keys[index] = Button { down, until: 0 };
            self.label_lengths[index] = 0;
        }
    }
    /// Releases every key and button; the keyboard geometry stays the detected one.
    pub fn clear(&mut self) {
        *self = Self {
            ansi: self.ansi,
            ..Self::default()
        };
    }
}
// Physical labels remain useful when Num Lock is off or a key has no printable character.
// Words follow the keyboards of `language` (`keycap.*`); symbols and digits stay.
pub(super) fn special_label_in(scan: u16, extended: bool, language: Language) -> &'static str {
    let word = |key: Key| tr_in(language, key);
    match (scan, extended) {
        (1, false) => word(Key::KeycapEsc),
        (14, false) => "←",
        (15, false) => "⇥",
        (28, _) => "↵",
        (29, _) => word(Key::KeycapCtrl),
        (42 | 54, false) => "⇧",
        (56, false) => word(Key::KeycapAlt),
        (56, true) => word(Key::KeycapAltGr),
        (57, false) => "␣",
        (58, false) => word(Key::KeycapCaps),
        (69, false) => word(Key::KeycapNum),
        (69, true) => "Ⅱ",
        (70, false) => word(Key::KeycapScroll),
        (55, true) => word(Key::KeycapPrint),
        (91 | 92, true) => "⊞",
        (93, true) => "☰",
        (71, true) => "↖",
        (72, true) => "↑",
        (73, true) => "⇞",
        (75, true) => "←",
        (77, true) => "→",
        (79, true) => word(Key::KeycapEnd),
        (80, true) => "↓",
        (81, true) => "⇟",
        (82, true) => word(Key::KeycapInsert),
        (83, true) => word(Key::KeycapDelete),
        (53, true) => "/",
        (55, false) => "*",
        (74, false) => "−",
        (78, false) => "+",
        (71, false) => "7",
        (72, false) => "8",
        (73, false) => "9",
        (75, false) => "4",
        (76, false) => "5",
        (77, false) => "6",
        (79, false) => "1",
        (80, false) => "2",
        (81, false) => "3",
        (82, false) => "0",
        (83, false) => ".",
        (59, false) => "F1",
        (60, false) => "F2",
        (61, false) => "F3",
        (62, false) => "F4",
        (63, false) => "F5",
        (64, false) => "F6",
        (65, false) => "F7",
        (66, false) => "F8",
        (67, false) => "F9",
        (68, false) => "F10",
        (87, false) => "F11",
        (88, false) => "F12",
        _ => "",
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn special_keys_and_numpad_have_independent_fading_labels() {
        let mut d = Devices::default();
        for (scan, extended, expected) in [
            (57, false, "␣"),
            (29, false, "Ctrl"),
            (29, true, "Ctrl"),
            (56, false, "Alt"),
            (56, true, "Gr"),
            (42, false, "⇧"),
            (28, false, "↵"),
            (28, true, "↵"),
            (69, false, "Num"),
            (71, false, "7"),
            (71, true, "↖"),
            (88, false, "F12"),
        ] {
            assert!(d.label(scan, extended, 0).0.is_empty());
            d.keyboard(scan, extended, true, 10);
            assert_eq!(
                String::from_utf16(d.label(scan, extended, 10).0).unwrap(),
                expected
            );
            d.keyboard(scan, extended, false, 20);
            assert!(!d.active(scan, extended, 20));
            assert_eq!(d.label(scan, extended, 320).1, 0.5);
            assert!(d.label(scan, extended, 620).0.is_empty());
        }
    }
    #[test]
    fn labels_hold_then_fade_without_selection_after_release() {
        let mut d = Devices::default();
        d.keyboard(46, false, true, 0);
        d.set_label(46, false, &[67]);
        assert_eq!(d.label(46, false, 10000), (&[67][..], 1.));
        assert!(!d.needs_animation(10000));
        d.keyboard(46, false, false, 10000);
        assert!(!d.active(46, false, 10000));
        assert_eq!(d.label(46, false, 10000).1, 1.);
        assert_eq!(d.label(46, false, 10300).1, 0.5);
        assert!(d.needs_animation(10599));
        assert_eq!(d.label(46, false, 10600), (&[][..], 0.));
        assert!(!d.needs_animation(10600));
    }
    #[test]
    fn repress_replaces_label_and_other_releases_do_not_reset_fade() {
        let mut d = Devices::default();
        d.keyboard(30, false, true, 0);
        d.set_label(30, false, &[65]);
        d.keyboard(30, false, false, 10);
        d.keyboard(31, false, true, 20);
        d.keyboard(31, false, false, 30);
        assert_eq!(d.label(30, false, 310).1, 0.5);
        d.keyboard(30, false, true, 320);
        assert!(d.label(30, false, 320).0.is_empty());
        d.set_label(30, false, &[97]);
        assert_eq!(d.label(30, false, 320), (&[97][..], 1.));
        d.keyboard(30, false, false, 330);
        d.clear();
        assert!(d.label(30, false, 330).0.is_empty());
        assert!(!d.needs_animation(330));
    }
    #[test]
    fn printable_utf16_only_and_extended_labels_stay_independent() {
        let mut d = Devices::default();
        d.keyboard(53, true, true, 0);
        for text in [
            &[3][..],
            &[27][..],
            &[0xd800][..],
            &[32][..],
            &[65, 66, 67, 68, 69][..],
        ] {
            d.set_label(53, true, text);
            assert!(d.label(53, true, 0).0.is_empty());
        }
        d.set_label(53, true, &[0xd83d, 0xde00]);
        assert_eq!(d.label(53, true, 0).0, &[0xd83d, 0xde00]);
        assert!(d.label(53, false, 0).0.is_empty());
    }
    #[test]
    fn independent_modifiers_and_release() {
        let mut d = Devices::default();
        d.keyboard(29, false, true, 0);
        d.keyboard(29, true, true, 0);
        d.keyboard(30, false, true, 0);
        assert!(d.active(29, false, 50_000));
        assert!(!d.needs_animation(50_000));
        d.keyboard(29, false, false, 50_000);
        assert!(!d.active(29, false, 50_000));
        assert!(!d.active(29, false, 50_180));
        assert!(d.active(29, true, 50_180));
        assert!(d.active(30, false, 50_180));
        assert!(d.needs_animation(50_179));
        assert!(d.needs_animation(50_180));
        assert!(!d.needs_animation(50_600));
    }
    #[test]
    fn mouse_buttons_wheel_and_clear() {
        let mut d = Devices::default();
        d.mouse(1 | 4 | 64 | 256, 0);
        d.mouse(2, 10);
        assert!(!d.mouse_active(0, 190));
        assert!(d.mouse_active(1, 190));
        assert!(d.mouse_active(3, 190));
        assert!(d.mouse_active(4, 190));
        d.mouse(0x800, 200);
        assert!(d.mouse_active(5, 379));
        assert!(!d.mouse_active(5, 380));
        d.keyboard(90, true, true, 0);
        d.clear();
        assert!(!d.active(90, true, 0));
        assert!(!d.mouse_active(1, 0));
        assert!(!d.needs_animation(0));
    }
    #[test]
    fn a_held_hidden_key_is_active_without_label_or_fade() {
        let mut d = Devices::default();
        d.hold(29, true, true);
        assert!(d.active(29, true, 0));
        assert!(d.label(29, true, 0).0.is_empty());
        d.hold(29, true, false);
        assert!(!d.active(29, true, 0));
        assert!(!d.needs_animation(0));
        d.hold(200, false, true);
        // Releasing everything keeps the detected keyboard geometry.
        d.ansi = true;
        d.keyboard(30, false, true, 0);
        d.clear();
        assert!(d.ansi && !d.active(30, false, 0));
    }
    #[test]
    fn unmatched_release_and_invalid_scan_do_not_light_keys() {
        let mut d = Devices::default();
        d.keyboard(30, false, false, 10);
        d.keyboard(128, false, true, 10);
        d.keyboard(u16::MAX, true, true, 10);
        assert!(!d.active(30, false, 10));
        assert!(!d.active(128, false, 10));
        assert!(!d.needs_animation(10));
    }
}
