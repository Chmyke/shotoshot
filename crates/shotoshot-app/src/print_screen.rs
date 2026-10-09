//! ROLE: Hand the bare Print Screen key from the Windows Snipping Tool to Shotoshot, and give it back.
//! CONTRACT: Touches only the user's own preference (HKCU), only to register a bare Print Screen
//! the user saved (when saving it, and each time the resident starts); the previous value is
//! kept so leaving Print Screen or uninstalling (`--restore-print-screen`) restores it.
//! No hook, no admin rights.
use windows::{
    Win32::{Foundation::*, System::Registry::*},
    core::*,
};

pub const KEY: u32 = 0x2C;
/// Backup marker for "the Windows value did not exist".
const ABSENT: u32 = u32::MAX;
const ATTEMPTS: u32 = 10;

#[derive(Clone, Copy)]
struct Place {
    key: PCWSTR,
    value: PCWSTR,
}
/// Where the Snipping Tool option and Shotoshot's backup of it live.
pub struct Locations {
    setting: Place,
    backup: Place,
}
const WINDOWS: Locations = Locations {
    setting: Place {
        key: w!("Control Panel\\Keyboard"),
        value: w!("PrintScreenKeyForSnippingEnabled"),
    },
    backup: Place {
        key: w!("Software\\Shotoshot"),
        value: w!("SnippingPrintScreenBefore"),
    },
};

pub fn is_bare(modifiers: u32, key: u32) -> bool {
    modifiers == 0 && key == KEY
}

/// The signed-in user's Windows option; None for non-persistent sessions (tests, diagnostics),
/// which must never change it.
pub fn windows_option(persistent: bool) -> Option<&'static Locations> {
    persistent.then_some(&WINDOWS)
}

/// Gives the Print Screen key back to the Snipping Tool if Shotoshot took it.
pub fn restore() -> Result<()> {
    restore_at(&WINDOWS)
}

/// Runs `register`, which must report a dead registration as held (see `owned`). A bare Print
/// Screen first turns the Snipping Tool option off at `at`, then retries briefly while Windows
/// lets go. `Hotkey::replace` is the only caller, so no registration can skip it.
pub fn register(
    at: Option<&Locations>,
    modifiers: u32,
    key: u32,
    mut register: impl FnMut() -> Result<()>,
) -> Result<()> {
    let Some(at) = at.filter(|_| is_bare(modifiers, key)) else {
        return register();
    };
    let released = release_at(at)?;
    let first = register();
    if !released || !matches!(&first, Err(error) if held(error)) {
        return first;
    }
    for _ in 0..ATTEMPTS {
        std::thread::sleep(std::time::Duration::from_millis(100));
        match register() {
            Err(error) if held(&error) => {}
            other => return other,
        }
    }
    first
}

/// True when the key is not ours to take: registered elsewhere, or refused without any error
/// code as Windows does for a bare Print Screen it keeps (last error cleared before the call).
pub fn held(error: &Error) -> bool {
    error.code() == already_registered() || error.code() == S_OK
}

fn already_registered() -> HRESULT {
    HRESULT::from_win32(ERROR_HOTKEY_ALREADY_REGISTERED.0)
}

/// Reads the answer to a second registration of a bare Print Screen that Windows just accepted:
/// the first one owns the key only if the second is refused as already registered.
/// PITFALL: while the Snipping Tool option is on, and still at the instant it goes off, Windows 11
/// accepts RegisterHotKey for Print Screen but the hot key is dead: it never fires, the Snipping
/// Tool or the legacy clipboard copy answers instead, other processes can still register the key
/// and a second registration is refused without an error code. It stays dead until registered
/// again; 12 ms after the option went off that was enough (build 26200, measured 2026-10-01).
pub fn owned(second: &Result<()>) -> bool {
    matches!(second, Err(error) if error.code() == already_registered())
}

/// What `register` reports for a registration that Windows accepted but keeps dead.
pub fn dead() -> Error {
    Error::from_hresult(already_registered())
}

/// Turns the Snipping Tool option off; false when it already was.
fn release_at(at: &Locations) -> Result<bool> {
    let current = read(at.setting)?;
    if current == Some(0) {
        return Ok(false);
    }
    // ORDER: keep the backup before changing Windows, so an interruption loses nothing.
    if read(at.backup)?.is_none() {
        write(at.backup, current.unwrap_or(ABSENT))?;
    }
    write(at.setting, 0)?;
    Ok(true)
}

fn restore_at(at: &Locations) -> Result<()> {
    let Some(before) = read(at.backup)? else {
        return Ok(());
    };
    // A choice made since in Windows Settings wins: only undo Shotoshot's own 0.
    if read(at.setting)? == Some(0) {
        if before == ABSENT {
            delete(at.setting)?;
        } else {
            write(at.setting, before)?;
        }
    }
    delete(at.backup)
}

fn read(place: Place) -> Result<Option<u32>> {
    let mut data = 0u32;
    let mut size = std::mem::size_of::<u32>() as u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            place.key,
            place.value,
            RRF_RT_REG_DWORD,
            None,
            Some((&mut data as *mut u32).cast()),
            Some(&mut size),
        )
    };
    if status == ERROR_FILE_NOT_FOUND {
        return Ok(None);
    }
    status.ok()?;
    Ok(Some(data))
}

fn write(place: Place, value: u32) -> Result<()> {
    unsafe {
        RegSetKeyValueW(
            HKEY_CURRENT_USER,
            place.key,
            place.value,
            REG_DWORD.0,
            Some((&value as *const u32).cast()),
            std::mem::size_of::<u32>() as u32,
        )
        .ok()
    }
}

fn delete(place: Place) -> Result<()> {
    let status = unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, place.key, place.value) };
    if status == ERROR_FILE_NOT_FOUND {
        Ok(())
    } else {
        status.ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// CONTRACT: tests use a private HKCU key, never the real Windows preference.
    struct Scratch {
        root: HSTRING,
        setting: HSTRING,
        backup: HSTRING,
    }
    impl Scratch {
        fn new(name: &str) -> Self {
            let root = format!(
                "Software\\Shotoshot\\Tests\\PrintScreen-{}-{name}",
                std::process::id()
            );
            Self {
                setting: HSTRING::from(format!("{root}\\Keyboard")),
                backup: HSTRING::from(format!("{root}\\Shotoshot")),
                root: HSTRING::from(root),
            }
        }
        fn at(&self) -> Locations {
            Locations {
                setting: Place {
                    key: PCWSTR(self.setting.as_ptr()),
                    value: w!("PrintScreenKeyForSnippingEnabled"),
                },
                backup: Place {
                    key: PCWSTR(self.backup.as_ptr()),
                    value: w!("SnippingPrintScreenBefore"),
                },
            }
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            unsafe {
                let _ = RegDeleteTreeW(HKEY_CURRENT_USER, &self.root);
                // Fails harmlessly while another test still owns a sibling key.
                let _ = RegDeleteKeyW(HKEY_CURRENT_USER, w!("Software\\Shotoshot\\Tests"));
            }
        }
    }
    fn held_error() -> Error {
        Error::from_hresult(HRESULT::from_win32(ERROR_HOTKEY_ALREADY_REGISTERED.0))
    }

    #[test]
    fn absent_setting_is_released_then_removed_again() {
        let scratch = Scratch::new("absent");
        let at = scratch.at();
        assert!(release_at(&at).unwrap());
        assert_eq!(read(at.setting).unwrap(), Some(0));
        assert_eq!(read(at.backup).unwrap(), Some(ABSENT));
        assert!(!release_at(&at).unwrap(), "already released");
        assert_eq!(read(at.backup).unwrap(), Some(ABSENT), "backup kept");
        restore_at(&at).unwrap();
        assert_eq!(read(at.setting).unwrap(), None);
        assert_eq!(read(at.backup).unwrap(), None);
        restore_at(&at).unwrap();
    }

    #[test]
    fn enabled_setting_is_restored_unless_windows_settings_changed_it() {
        let scratch = Scratch::new("enabled");
        let at = scratch.at();
        write(at.setting, 1).unwrap();
        assert!(release_at(&at).unwrap());
        restore_at(&at).unwrap();
        assert_eq!(read(at.setting).unwrap(), Some(1));

        delete(at.setting).unwrap();
        assert!(release_at(&at).unwrap());
        write(at.setting, 1).unwrap(); // re-enabled from Windows Settings meanwhile
        restore_at(&at).unwrap();
        assert_eq!(read(at.setting).unwrap(), Some(1));
        assert_eq!(read(at.backup).unwrap(), None);
    }

    #[test]
    fn setting_already_off_is_left_alone() {
        let scratch = Scratch::new("off");
        let at = scratch.at();
        write(at.setting, 0).unwrap();
        let mut calls = 0;
        let result = register(Some(&at), 0, KEY, || {
            calls += 1;
            Err(held_error())
        });
        assert!(result.is_err());
        assert_eq!(calls, 1, "nothing to free, no retry");
        assert_eq!(read(at.backup).unwrap(), None);
        restore_at(&at).unwrap();
        assert_eq!(read(at.setting).unwrap(), Some(0));
    }

    #[test]
    fn held_print_screen_is_freed_and_registered_on_retry() {
        let scratch = Scratch::new("retry");
        let at = scratch.at();
        let mut calls = 0;
        register(Some(&at), 0, KEY, || {
            calls += 1;
            if calls < 3 { Err(held_error()) } else { Ok(()) }
        })
        .unwrap();
        assert_eq!(calls, 3);
        assert_eq!(read(at.setting).unwrap(), Some(0));
    }

    #[test]
    fn accepted_print_screen_still_turns_the_option_off() {
        let scratch = Scratch::new("accepted");
        let at = scratch.at();
        let mut calls = 0;
        register(Some(&at), 0, KEY, || {
            calls += 1;
            Ok(())
        })
        .unwrap();
        assert_eq!(calls, 1);
        assert_eq!(read(at.setting).unwrap(), Some(0));
        assert_eq!(read(at.backup).unwrap(), Some(ABSENT));
    }

    #[test]
    fn only_a_refusal_as_already_registered_proves_ownership() {
        assert!(owned(&Err(held_error())));
        assert!(!owned(&Ok(())), "a second registration was accepted");
        let no_code = Error::from_hresult(S_OK);
        assert!(!owned(&Err(no_code.clone())), "refused without a code");
        assert!(held(&no_code), "a key Windows keeps is retried");
        assert!(held(&dead()), "a dead registration is retried");
    }

    #[test]
    fn every_hotkey_registration_frees_a_bare_print_screen() {
        // A thread hotkey of this test only; whether the key is free here does not matter.
        let scratch = Scratch::new("hotkey");
        let at = scratch.at();
        let mut hotkey = crate::hotkey::Hotkey::new(HWND::default());
        let _ = hotkey.replace(Some(&at), 0, KEY, || Ok(()));
        assert_eq!(read(at.setting).unwrap(), Some(0), "option turned off");
        assert_eq!(
            read(at.backup).unwrap(),
            Some(ABSENT),
            "previous state kept"
        );
    }

    #[test]
    fn other_keys_and_non_persistent_sessions_never_touch_windows() {
        let scratch = Scratch::new("other");
        let at = scratch.at();
        for (mods, key, place) in [(4, KEY, Some(&at)), (0, 19, Some(&at)), (0, KEY, None)] {
            assert!(register(place, mods, key, || Err(held_error())).is_err());
        }
        assert_eq!(read(at.setting).unwrap(), None);
        assert_eq!(read(at.backup).unwrap(), None);
    }
}
