//! ROLE: winit key events to the canonical virtual-key codes used by `shotoshot-ui`.
//! INVARIANTS: Letters follow the active layout (AZERTY "A" is VK_A), digits follow the
//! physical top row (AZERTY "&/1" is VK_1), as Windows virtual keys do.
use winit::keyboard::{Key, KeyCode, NamedKey, PhysicalKey};

pub fn virtual_key(logical: &Key, physical: PhysicalKey) -> Option<u32> {
    if let PhysicalKey::Code(code) = physical
        && let Some(digit) = digit(code)
    {
        return Some(b'0' as u32 + digit);
    }
    match logical {
        Key::Character(s) => {
            let mut chars = s.chars();
            let c = chars.next()?.to_ascii_uppercase();
            (chars.next().is_none() && c.is_ascii_uppercase()).then_some(c as u32)
        }
        Key::Named(named) => Some(match named {
            NamedKey::Backspace => 8,
            NamedKey::Tab => 9,
            NamedKey::Enter => 13,
            NamedKey::Escape => 27,
            NamedKey::PageUp => 33,
            NamedKey::PageDown => 34,
            NamedKey::End => 35,
            NamedKey::Home => 36,
            NamedKey::ArrowLeft => 37,
            NamedKey::ArrowUp => 38,
            NamedKey::ArrowRight => 39,
            NamedKey::ArrowDown => 40,
            NamedKey::Delete => 46,
            NamedKey::F2 => 113,
            _ => return None,
        }),
        _ => None,
    }
}

/// The virtual key a shortcut records: the keys every desktop can bind by name, after the
/// physical key for those a layout never changes (Print Screen with Alt is `Sys_Req`).
pub fn shortcut_key(logical: &Key, physical: PhysicalKey) -> Option<u32> {
    if let PhysicalKey::Code(code) = physical {
        let function = [
            KeyCode::F1,
            KeyCode::F2,
            KeyCode::F3,
            KeyCode::F4,
            KeyCode::F5,
            KeyCode::F6,
            KeyCode::F7,
            KeyCode::F8,
            KeyCode::F9,
            KeyCode::F10,
            KeyCode::F11,
            KeyCode::F12,
            KeyCode::F13,
            KeyCode::F14,
            KeyCode::F15,
            KeyCode::F16,
            KeyCode::F17,
            KeyCode::F18,
            KeyCode::F19,
            KeyCode::F20,
            KeyCode::F21,
            KeyCode::F22,
            KeyCode::F23,
            KeyCode::F24,
        ];
        if let Some(n) = function.iter().position(|f| *f == code) {
            return Some(112 + n as u32);
        }
        let named = match code {
            KeyCode::PrintScreen => Some(44),
            KeyCode::Pause => Some(19),
            KeyCode::ScrollLock => Some(145),
            KeyCode::Insert => Some(45),
            KeyCode::Delete => Some(46),
            KeyCode::Home => Some(36),
            KeyCode::End => Some(35),
            KeyCode::PageUp => Some(33),
            KeyCode::PageDown => Some(34),
            KeyCode::ArrowLeft => Some(37),
            KeyCode::ArrowUp => Some(38),
            KeyCode::ArrowRight => Some(39),
            KeyCode::ArrowDown => Some(40),
            KeyCode::Backspace => Some(8),
            KeyCode::Tab => Some(9),
            KeyCode::Enter => Some(13),
            KeyCode::Space => Some(32),
            KeyCode::Escape => Some(27),
            _ => None,
        };
        if named.is_some() {
            return named;
        }
    }
    virtual_key(logical, physical)
}

fn digit(code: KeyCode) -> Option<u32> {
    Some(match code {
        KeyCode::Digit0 => 0,
        KeyCode::Digit1 => 1,
        KeyCode::Digit2 => 2,
        KeyCode::Digit3 => 3,
        KeyCode::Digit4 => 4,
        KeyCode::Digit5 => 5,
        KeyCode::Digit6 => 6,
        KeyCode::Digit7 => 7,
        KeyCode::Digit8 => 8,
        KeyCode::Digit9 => 9,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use winit::keyboard::SmolStr;

    fn character(s: &str, code: KeyCode) -> Option<u32> {
        virtual_key(&Key::Character(SmolStr::new(s)), PhysicalKey::Code(code))
    }
    #[test]
    fn layout_letters_physical_digits_and_named_keys() {
        // AZERTY: the key at QWERTY "Q" types "a" and is VK_A.
        assert_eq!(character("a", KeyCode::KeyQ), Some(65));
        assert_eq!(character("R", KeyCode::KeyR), Some(82));
        // AZERTY top row types "&" without Shift but is still VK_1.
        assert_eq!(character("&", KeyCode::Digit1), Some(49));
        assert_eq!(character("é", KeyCode::Digit2), Some(50));
        assert_eq!(character("é", KeyCode::Slash), None);
        assert_eq!(
            virtual_key(
                &Key::Named(NamedKey::Escape),
                PhysicalKey::Code(KeyCode::Escape)
            ),
            Some(27)
        );
        assert_eq!(
            virtual_key(
                &Key::Named(NamedKey::PageUp),
                PhysicalKey::Code(KeyCode::PageUp)
            ),
            Some(33)
        );
        assert_eq!(
            virtual_key(
                &Key::Named(NamedKey::PageDown),
                PhysicalKey::Code(KeyCode::PageDown)
            ),
            Some(34)
        );
    }
    #[test]
    fn shortcut_keys_follow_the_physical_key_for_named_keys() {
        let sys_req = Key::Named(NamedKey::Print);
        assert_eq!(
            shortcut_key(&sys_req, PhysicalKey::Code(KeyCode::PrintScreen)),
            Some(44)
        );
        assert_eq!(
            shortcut_key(&Key::Named(NamedKey::F5), PhysicalKey::Code(KeyCode::F5)),
            Some(116)
        );
        assert_eq!(
            shortcut_key(
                &Key::Character(SmolStr::new("a")),
                PhysicalKey::Code(KeyCode::KeyQ)
            ),
            Some(65)
        );
    }
}
