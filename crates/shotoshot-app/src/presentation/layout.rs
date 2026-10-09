//! ROLE: Translates physical keys using the foreground application's keyboard layout.
//! INVARIANTS: Translation never changes Windows dead-key state or stores typed text.
use windows::Win32::UI::Input::KeyboardAndMouse::*;

pub(super) fn translate(scan: u16, state: &[u8; 256], layout: HKL) -> ([u16; 4], usize) {
    let mut text = [0; 4];
    let mut state = *state;
    // PITFALL: Windows models AltGr as Ctrl+Alt, but its synthetic Ctrl event
    // can be filtered by the non-injected physical input observer.
    let right_alt = state[VK_RMENU.0 as usize] & 0x80 != 0;
    if right_alt {
        state[VK_CONTROL.0 as usize] = 0x80;
        state[VK_MENU.0 as usize] = 0x80;
    }
    unsafe {
        let vk = MapVirtualKeyExW(scan as u32, MAPVK_VSC_TO_VK_EX, Some(layout));
        let mut length = ToUnicodeEx(vk, scan as u32, &state, &mut text, 4, Some(layout));
        if right_alt && length == 0 {
            state[VK_CONTROL.0 as usize] = 0;
            state[VK_MENU.0 as usize] = 0;
            state[VK_RMENU.0 as usize] = 0;
            length = ToUnicodeEx(vk, scan as u32, &state, &mut text, 4, Some(layout));
        }
        let length = if length == -1 {
            1
        } else if (1..=4).contains(&length) {
            length as usize
        } else {
            0
        };
        (text, length)
    }
}

// Windows exposes a logical layout, not the physical keyboard enclosure.
// US-family layouts conventionally use ANSI; other layouts retain the ISO diagram.
pub(super) fn is_ansi(layout: HKL) -> bool {
    layout.0 as usize & 0xffff == 0x0409
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::core::w;
    struct Loaded {
        handle: HKL,
        owned: bool,
    }
    impl Loaded {
        fn new(name: windows::core::PCWSTR) -> Self {
            unsafe {
                let mut before = vec![HKL::default(); GetKeyboardLayoutList(None) as usize];
                GetKeyboardLayoutList(Some(&mut before));
                let handle = LoadKeyboardLayoutW(name, ACTIVATE_KEYBOARD_LAYOUT_FLAGS(0)).unwrap();
                Self {
                    handle,
                    owned: !before.contains(&handle),
                }
            }
        }
    }
    impl Drop for Loaded {
        fn drop(&mut self) {
            if self.owned {
                unsafe {
                    let _ = UnloadKeyboardLayout(self.handle);
                }
            }
        }
    }
    #[test]
    fn layouts_accents_altgr_and_dead_keys_follow_physical_keys() {
        // Load without activation; leave the user's active layout unchanged.
        {
            let french = Loaded::new(w!("0000040C"));
            let american = Loaded::new(w!("00000409"));
            let german = Loaded::new(w!("00000407"));
            let british = Loaded::new(w!("00000809"));
            let (fr, us) = (french.handle, american.handle);
            assert!(!is_ansi(fr));
            assert!(is_ansi(us));
            assert!(!is_ansi(british.handle));
            let text = |scan, state: &[u8; 256], layout| {
                let (buffer, len) = translate(scan, state, layout);
                String::from_utf16(&buffer[..len]).unwrap()
            };
            let mut state = [0; 256];
            assert_eq!(text(16, &state, fr), "a");
            assert_eq!(text(16, &state, us), "q");
            assert_eq!(text(21, &state, german.handle), "z");
            assert_eq!(text(21, &state, us), "y");
            assert_eq!(text(3, &state, fr), "é");
            state[VK_SHIFT.0 as usize] = 0x80;
            assert_eq!(text(16, &state, fr), "A");
            assert_eq!(text(3, &state, fr), "2");
            state = [0; 256];
            state[VK_CONTROL.0 as usize] = 0x80;
            state[VK_MENU.0 as usize] = 0x80;
            assert_eq!(text(11, &state, fr), "@");
            state = [0; 256];
            state[VK_RMENU.0 as usize] = 0x80;
            assert_eq!(
                text(11, &state, fr),
                "@",
                "AltGr must work without an injected Ctrl event"
            );
            state = [0; 256];
            assert_eq!(text(26, &state, fr), "^");
            assert_eq!(
                text(18, &state, fr),
                "e",
                "dead key must not affect later input"
            );
        }
    }
}
