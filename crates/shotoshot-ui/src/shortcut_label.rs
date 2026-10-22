//! ROLE: Writes a key combination the way the interface shows it ("Ctrl+Shift+Z"), with the
//! modifier and special key names of the interface language. The keys themselves never change.
use shotoshot_text::{Key, Language, language, tr_in};

/// The combination of Windows virtual key `vk` with Ctrl and Shift, in the interface language.
pub fn shortcut_label(vk: u32, ctrl: bool, shift: bool) -> String {
    shortcut_label_in(vk, ctrl, shift, language())
}

/// The combination of Windows virtual key `vk` with Ctrl and Shift, in `language`.
pub fn shortcut_label_in(vk: u32, ctrl: bool, shift: bool, language: Language) -> String {
    let modifiers = [(ctrl, Key::KeyCtrl), (shift, Key::KeyShift)];
    let mut parts: Vec<String> = modifiers
        .into_iter()
        .filter(|&(held, _)| held)
        .map(|(_, key)| tr_in(language, key).to_string())
        .collect();
    parts.push(key_name_in(vk, language));
    parts.join("+")
}

/// Letters and digits as printed on the key, `F1` to `F24` as such, the other keys by the name
/// keyboards of `language` print.
fn key_name_in(vk: u32, language: Language) -> String {
    let named = |key| tr_in(language, key).to_string();
    match vk {
        48..=57 | 65..=90 => char::from(vk as u8).to_string(),
        112..=135 => format!("F{}", vk - 111),
        16 => named(Key::KeyShift),
        17 => named(Key::KeyCtrl),
        18 => named(Key::KeyAlt),
        27 => named(Key::KeyEsc),
        46 => named(Key::KeyDelete),
        91 | 92 => named(Key::KeyWin),
        // No hint names another key: a code to look up rather than a wrong name.
        _ => format!("0x{vk:02X}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shotoshot_text::Language;

    #[test]
    fn letters_digits_and_function_keys_stay_as_printed() {
        for language in Language::ALL {
            assert_eq!(shortcut_label_in(82, false, false, language), "R");
            assert_eq!(shortcut_label_in(49, false, false, language), "1");
            assert_eq!(shortcut_label_in(113, false, false, language), "F2");
        }
    }

    #[test]
    fn modifier_names_follow_the_language() {
        assert_eq!(
            shortcut_label_in(90, true, true, Language::En),
            "Ctrl+Shift+Z"
        );
        assert_eq!(
            shortcut_label_in(90, true, true, Language::De),
            "Strg+Umschalt+Z"
        );
        assert_eq!(
            shortcut_label_in(90, true, true, Language::Fr),
            "Ctrl+Maj+Z"
        );
        assert_eq!(shortcut_label_in(27, false, false, Language::En), "Esc");
        assert_eq!(shortcut_label_in(46, false, false, Language::De), "Entf");
    }
}
