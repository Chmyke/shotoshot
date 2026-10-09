//! ROLE: One-time migration of a pre-0.2.0 install from the former name "Shotshot" to
//! "Shotoshot" (renamed on 2026-09-28): data folder, registry preferences and the former
//! "start with Windows" value.
//! CONTRACT: Idempotent and best effort: a failure leaves the old data where it was and
//! never blocks startup. Runs in the resident only, before the settings are loaded.
//! DELETE: with its call in `app/runtime.rs` once no pre-0.2.0 install remains.
use crate::settings::known_folder;
use windows::{
    Win32::{Foundation::*, System::Registry::*, UI::Shell::FOLDERID_LocalAppData},
    core::*,
};

const OLD_KEY: PCWSTR = w!("Software\\Shotshot");
const NEW_KEY: PCWSTR = w!("Software\\Shotoshot");
const RUN: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
const OLD_RUN_VALUE: PCWSTR = w!("Shotshot");

/// The resident rewrites its own Run value from the migrated settings right after this, so
/// the user's "start with Windows" choice survives without pointing at the removed exe.
pub fn run() {
    if let Ok(local) = known_folder(&FOLDERID_LocalAppData) {
        let _ = shotoshot_core::rename_migration::move_data_folder(
            &local.join("Shotshot"),
            &local.join("Shotoshot"),
        );
    }
    let _ = move_key(OLD_KEY, NEW_KEY);
    let _ = delete_value(RUN, OLD_RUN_VALUE);
}

/// Moves `old` (values and subkeys) to `new` unless `new` already holds values.
/// Returns whether anything moved.
fn move_key(old: PCWSTR, new: PCWSTR) -> Result<bool> {
    unsafe {
        let mut source = HKEY::default();
        let status = RegOpenKeyExW(HKEY_CURRENT_USER, old, None, KEY_READ, &mut source);
        if status == ERROR_FILE_NOT_FOUND {
            return Ok(false);
        }
        status.ok()?;
        let source = Key(source);
        let mut target = HKEY::default();
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            new,
            None,
            PCWSTR::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_ALL_ACCESS,
            None,
            &mut target,
            None,
        )
        .ok()?;
        let target = Key(target);
        let mut values = 0u32;
        RegQueryInfoKeyW(
            target.0,
            None,
            None,
            None,
            None,
            None,
            None,
            Some(&mut values),
            None,
            None,
            None,
            None,
        )
        .ok()?;
        if values != 0 {
            return Ok(false);
        }
        RegCopyTreeW(source.0, PCWSTR::null(), target.0).ok()?;
        drop(source);
        RegDeleteTreeW(HKEY_CURRENT_USER, old).ok()?;
        Ok(true)
    }
}

/// Deletes `name` under `key`; false when it did not exist.
fn delete_value(key: PCWSTR, name: PCWSTR) -> Result<bool> {
    unsafe {
        let status = RegDeleteKeyValueW(HKEY_CURRENT_USER, key, name);
        if status == ERROR_FILE_NOT_FOUND {
            return Ok(false);
        }
        status.ok()?;
        Ok(true)
    }
}

struct Key(HKEY);
impl Drop for Key {
    fn drop(&mut self) {
        unsafe {
            let _ = RegCloseKey(self.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// CONTRACT: tests use a private HKCU key, never the real preferences or Run key.
    struct Scratch(HSTRING);
    impl Scratch {
        fn new(name: &str) -> Self {
            Self(HSTRING::from(format!(
                "Software\\Shotoshot\\Tests\\Rename-{}-{name}",
                std::process::id()
            )))
        }
        fn key(&self, child: &str) -> HSTRING {
            HSTRING::from(format!("{}\\{child}", self.0))
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            unsafe {
                let _ = RegDeleteTreeW(HKEY_CURRENT_USER, &self.0);
                // Fails harmlessly while another test still owns a sibling key.
                let _ = RegDeleteKeyW(HKEY_CURRENT_USER, w!("Software\\Shotoshot\\Tests"));
            }
        }
    }

    fn set(key: &HSTRING, name: PCWSTR, value: u32) {
        unsafe {
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                key,
                name,
                REG_DWORD.0,
                Some((&value as *const u32).cast()),
                4,
            )
            .ok()
            .unwrap();
        }
    }
    fn get(key: &HSTRING, name: PCWSTR) -> Option<u32> {
        let mut data = 0u32;
        let mut size = 4u32;
        let status = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                key,
                name,
                RRF_RT_REG_DWORD,
                None,
                Some((&mut data as *mut u32).cast()),
                Some(&mut size),
            )
        };
        (status == ERROR_SUCCESS).then_some(data)
    }
    fn exists(key: &HSTRING) -> bool {
        let mut handle = HKEY::default();
        let status = unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, key, None, KEY_READ, &mut handle) };
        if status == ERROR_SUCCESS {
            drop(Key(handle));
        }
        status == ERROR_SUCCESS
    }

    #[test]
    fn preferences_key_moves_once_with_its_values() {
        let scratch = Scratch::new("key");
        let (old, new) = (scratch.key("Shotshot"), scratch.key("Shotoshot"));
        let value = w!("SnippingPrintScreenBefore");
        set(&old, value, 1);
        let moved = move_key(PCWSTR(old.as_ptr()), PCWSTR(new.as_ptr())).unwrap();
        assert!(moved);
        assert_eq!(get(&new, value), Some(1));
        assert!(!exists(&old));
        assert!(!move_key(PCWSTR(old.as_ptr()), PCWSTR(new.as_ptr())).unwrap());
        assert_eq!(get(&new, value), Some(1));
    }

    #[test]
    fn preferences_already_under_the_new_name_win() {
        let scratch = Scratch::new("kept");
        let (old, new) = (scratch.key("Shotshot"), scratch.key("Shotoshot"));
        let value = w!("SnippingPrintScreenBefore");
        set(&old, value, 1);
        set(&new, value, 0);
        assert!(!move_key(PCWSTR(old.as_ptr()), PCWSTR(new.as_ptr())).unwrap());
        assert_eq!(get(&new, value), Some(0));
        assert_eq!(get(&old, value), Some(1));
    }

    #[test]
    fn empty_new_key_left_by_tests_does_not_block_the_move() {
        let scratch = Scratch::new("empty");
        let (old, new) = (scratch.key("Shotshot"), scratch.key("Shotoshot"));
        let value = w!("SnippingPrintScreenBefore");
        set(&old, value, 1);
        set(&scratch.key("Shotoshot\\Tests"), w!("Other"), 7);
        assert!(move_key(PCWSTR(old.as_ptr()), PCWSTR(new.as_ptr())).unwrap());
        assert_eq!(get(&new, value), Some(1));
        assert!(!exists(&old));
    }

    #[test]
    fn empty_old_key_is_removed() {
        let scratch = Scratch::new("bare");
        let (old, new) = (scratch.key("Shotshot"), scratch.key("Shotoshot"));
        set(&scratch.key("Shotshot\\Tests"), w!("Other"), 7);
        unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, &scratch.key("Shotshot\\Tests")) }
            .ok()
            .unwrap();
        set(&new, w!("Kept"), 3);
        let _ = unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, &new, w!("Kept")) };
        assert!(exists(&old) && exists(&new));
        assert!(move_key(PCWSTR(old.as_ptr()), PCWSTR(new.as_ptr())).unwrap());
        assert!(!exists(&old));
    }

    #[test]
    fn former_run_value_is_removed_once() {
        let scratch = Scratch::new("run");
        let run = scratch.key("Run");
        set(&run, OLD_RUN_VALUE, 1);
        set(&run, w!("Shotoshot"), 2);
        assert!(delete_value(PCWSTR(run.as_ptr()), OLD_RUN_VALUE).unwrap());
        assert!(!delete_value(PCWSTR(run.as_ptr()), OLD_RUN_VALUE).unwrap());
        assert_eq!(get(&run, OLD_RUN_VALUE), None);
        assert_eq!(get(&run, w!("Shotoshot")), Some(2));
    }
}
