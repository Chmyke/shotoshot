use windows::{
    Win32::{
        Foundation::{ERROR_SUCCESS, HWND, SetLastError},
        UI::Input::KeyboardAndMouse::*,
    },
    core::*,
};

/// Short-lived second registration that tells whether a bare Print Screen is really owned.
const CONFIRM_ID: i32 = 80;

pub struct Hotkey {
    window: HWND,
    id: Option<i32>,
    first_id: i32,
}
impl Hotkey {
    pub fn new(window: HWND) -> Self {
        Self::with_id(window, 1)
    }
    pub fn with_id(window: HWND, first_id: i32) -> Self {
        Self {
            window,
            id: None,
            first_id,
        }
    }
    pub fn matches(&self, id: usize) -> bool {
        self.id.is_some_and(|current| current as usize == id)
    }
    pub fn active(&self) -> bool {
        self.id.is_some()
    }
    /// `free` is where a bare Print Screen is taken from the Snipping Tool (see `print_screen`);
    /// None never changes Windows.
    pub fn replace(
        &mut self,
        free: Option<&crate::print_screen::Locations>,
        modifiers: u32,
        key: u32,
        persist: impl FnOnce() -> Result<()>,
    ) -> Result<()> {
        unsafe {
            let next = if self.id == Some(self.first_id) {
                self.first_id + 1
            } else {
                self.first_id
            };
            let window = Some(self.window);
            let flags = HOT_KEY_MODIFIERS(modifiers) | MOD_NOREPEAT;
            let confirm = crate::print_screen::is_bare(modifiers, key);
            crate::print_screen::register(free, modifiers, key, || {
                // A refusal without an error code must read as 0, not as a stale code.
                SetLastError(ERROR_SUCCESS);
                RegisterHotKey(window, next, flags, key)?;
                if confirm {
                    SetLastError(ERROR_SUCCESS);
                    let second = RegisterHotKey(window, CONFIRM_ID, flags, key);
                    if second.is_ok() {
                        let _ = UnregisterHotKey(window, CONFIRM_ID);
                    }
                    if !crate::print_screen::owned(&second) {
                        let _ = UnregisterHotKey(window, next);
                        return Err(crate::print_screen::dead());
                    }
                }
                Ok(())
            })?;
            if let Err(error) = persist() {
                let _ = UnregisterHotKey(Some(self.window), next);
                return Err(error);
            }
            if let Some(old) = self.id {
                let _ = UnregisterHotKey(Some(self.window), old);
            }
            self.id = Some(next);
            Ok(())
        }
    }
}
impl Drop for Hotkey {
    fn drop(&mut self) {
        if let Some(id) = self.id {
            unsafe {
                let _ = UnregisterHotKey(Some(self.window), id);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn capture_and_instant_shortcuts_keep_separate_registrations() {
        let mut capture = Hotkey::new(HWND::default());
        let mut instant = Hotkey::with_id(HWND::default(), 3);
        capture.replace(None, 7, 0x84, || Ok(())).unwrap();
        instant.replace(None, 7, 0x83, || Ok(())).unwrap();
        assert!(capture.matches(1));
        assert!(instant.matches(3));
        instant.replace(None, 7, 0x82, || Ok(())).unwrap();
        assert!(capture.matches(1));
        assert!(!capture.matches(4));
        assert!(instant.matches(4));
        assert!(!instant.matches(3));
    }
    #[test]
    fn failed_preference_write_keeps_previous_shortcut() {
        unsafe {
            // CONTRACT: Use thread hotkeys, never alter another application's registration.
            let mut hotkey = Hotkey::new(HWND::default());
            let mods = MOD_CONTROL.0 | MOD_ALT.0 | MOD_SHIFT.0;
            hotkey.replace(None, mods, 0x87, || Ok(())).unwrap();
            let old = hotkey.id;
            let result = hotkey.replace(None, mods, 0x86, || {
                Err(Error::new(
                    windows::Win32::Foundation::E_FAIL,
                    "injected persistence failure",
                ))
            });
            assert!(result.is_err());
            assert_eq!(hotkey.id, old);
            // RegisterHotKey can replace a registration on the same thread/id, so use another ID.
            assert!(
                std::thread::spawn(|| {
                    let r = RegisterHotKey(None, 45, MOD_CONTROL | MOD_ALT | MOD_SHIFT, 0x87);
                    if r.is_ok() {
                        let _ = UnregisterHotKey(None, 45);
                    }
                    r.is_err()
                })
                .join()
                .unwrap()
            );
        }
    }
}
