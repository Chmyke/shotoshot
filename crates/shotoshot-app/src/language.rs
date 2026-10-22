//! ROLE: Windows side of the interface language: the system's preferred language, the choice
//! handed over by the installer, and the application of the stored choice when a process starts.
//! INVARIANTS: Reads the user's language list and, in the resident only, one small file the
//! installer may have left; no network (law 2).
use shotoshot_core::settings::Settings;
use shotoshot_text::{Language, LanguageChoice, set_language};
use std::path::Path;
use windows::Win32::Globalization::{GetUserPreferredUILanguages, MUI_LANGUAGE_NAME};
use windows::core::PWSTR;

/// In the settings folder, the language chosen on the installer's page: `auto` or a language
/// tag, written just before the installer starts the app (`packaging/shotoshot.iss`,
/// `WriteLanguageChoice`). Nothing is written by a silent install.
pub const INSTALLER_CHOICE_FILE: &str = "installer-language.txt";

/// The choice the installer left in `dir`, the file left in place until it is stored. A file
/// that says nothing known is removed at once. `None` without a file, or for such a file.
fn installer_choice(dir: &Path) -> Option<LanguageChoice> {
    let path = dir.join(INSTALLER_CHOICE_FILE);
    let bytes = std::fs::read(&path).ok()?;
    let value = std::str::from_utf8(&bytes)
        .ok()
        .map(|text| text.trim_matches(|c: char| c == '\u{feff}' || c.is_whitespace()));
    let choice = value.and_then(|value| {
        if value.eq_ignore_ascii_case(LanguageChoice::Auto.encode()) {
            Some(LanguageChoice::Auto)
        } else {
            Language::from_tag(value).map(LanguageChoice::Fixed)
        }
    });
    if choice.is_none() {
        let _ = std::fs::remove_file(&path);
    }
    choice
}

/// Makes the choice the installer left in `dir`, if any, the language of `settings`, stored
/// with `save` when it changes them. The file is removed once the choice is stored, so it
/// applies once; when `save` fails, the choice stays in `settings` for this run and the file
/// stays for the next start to try again.
pub fn adopt_installer_choice(
    settings: &mut Settings,
    dir: &Path,
    save: impl FnOnce(&Settings) -> Result<(), String>,
) -> Result<(), String> {
    let Some(choice) = installer_choice(dir) else {
        return Ok(());
    };
    if choice != settings.language {
        settings.language = choice;
        save(settings)?;
    }
    let _ = std::fs::remove_file(dir.join(INSTALLER_CHOICE_FILE));
    Ok(())
}

/// The resident's start, before its first window: adopts the installer's choice and makes the
/// language of `settings` the process language with `apply`, even when the choice could not be
/// stored. A failed save is not a failed start: `unsaved` hears why, once the language is
/// applied.
pub fn start_with_installer_choice(
    settings: &mut Settings,
    dir: &Path,
    save: impl FnOnce(&Settings) -> Result<(), String>,
    apply: impl FnOnce(&Settings),
    unsaved: impl FnOnce(String),
) {
    let stored = settings.language;
    let result = adopt_installer_choice(settings, dir, save);
    if settings.language != stored {
        apply(settings);
    }
    if let Err(detail) = result {
        unsaved(detail);
    }
}

/// Stores `next`, whose language was chosen in the menu, with `save`. Once stored, that choice
/// outranks one the installer left in `dir` and could not store: the file is removed, so the
/// next start keeps the menu's choice. When `save` fails, the file stays.
pub fn store_menu_choice(
    next: &Settings,
    dir: &Path,
    save: impl FnOnce(&Settings) -> Result<(), String>,
) -> Result<(), String> {
    save(next)?;
    let _ = std::fs::remove_file(dir.join(INSTALLER_CHOICE_FILE));
    Ok(())
}

/// The first language of the user's Windows display-language list as a locale name
/// (`fr-FR`, `zh-Hant-TW`), or an empty string when Windows does not say.
pub fn system_tag() -> String {
    let mut count = 0u32;
    let mut size = 0u32;
    // SAFETY: the first call only asks for the buffer size; the second fills a buffer of
    // exactly that many UTF-16 units, as a double-null-terminated list.
    unsafe {
        if GetUserPreferredUILanguages(MUI_LANGUAGE_NAME, &mut count, None, &mut size).is_err()
            || size == 0
        {
            return String::new();
        }
        let mut buffer = vec![0u16; size as usize];
        if GetUserPreferredUILanguages(
            MUI_LANGUAGE_NAME,
            &mut count,
            Some(PWSTR(buffer.as_mut_ptr())),
            &mut size,
        )
        .is_err()
        {
            return String::new();
        }
        first_entry(&buffer)
    }
}

/// The first string of a null-separated list of UTF-16 strings.
fn first_entry(list: &[u16]) -> String {
    let end = list
        .iter()
        .position(|&unit| unit == 0)
        .unwrap_or(list.len());
    String::from_utf16_lossy(&list[..end])
}

/// Makes the language of `settings` (or the system's, for the automatic choice) the language
/// of this process. Called once, right after the settings are loaded.
pub fn apply(settings: &Settings) {
    set_language(settings.language.resolve(&system_tag()));
}

#[cfg(test)]
mod tests {
    use super::*;
    use shotoshot_text::language;

    #[test]
    fn first_entry_stops_at_the_first_separator() {
        let list: Vec<u16> = "fr-FR\0en-US\0\0".encode_utf16().collect();
        assert_eq!(first_entry(&list), "fr-FR");
        assert_eq!(first_entry(&[0, 0]), "");
        assert_eq!(first_entry(&[]), "");
        let unterminated: Vec<u16> = "de-DE".encode_utf16().collect();
        assert_eq!(first_entry(&unterminated), "de-DE");
    }

    #[test]
    fn system_tag_is_a_locale_name_or_empty() {
        let tag = system_tag();
        assert!(
            tag.is_empty()
                || tag
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
            "{tag:?}"
        );
        assert!(!tag.contains('\0'));
    }

    /// A folder of its own under the system's temporary folder, removed when dropped.
    struct Scratch(std::path::PathBuf);
    impl Scratch {
        fn new(name: &str) -> Scratch {
            let dir = std::env::temp_dir().join(format!(
                "shotoshot-installer-language-{name}-{}",
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Scratch(dir)
        }
        fn handoff(&self) -> std::path::PathBuf {
            self.0.join(INSTALLER_CHOICE_FILE)
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn installer_choice_is_applied_once() {
        let scratch = Scratch::new("once");
        assert_eq!(installer_choice(&scratch.0), None, "nothing to take");
        std::fs::write(scratch.handoff(), "fr").unwrap();
        assert_eq!(
            installer_choice(&scratch.0),
            Some(LanguageChoice::Fixed(Language::Fr))
        );
        assert!(scratch.handoff().exists(), "read, not yet stored");
        let mut settings = Settings::default();
        let mut saved = 0;
        adopt_installer_choice(&mut settings, &scratch.0, |_| {
            saved += 1;
            Ok(())
        })
        .unwrap();
        assert_eq!(settings.language, LanguageChoice::Fixed(Language::Fr));
        assert_eq!(saved, 1);
        assert!(!scratch.handoff().exists(), "stored, then removed");
        adopt_installer_choice(&mut settings, &scratch.0, |_| {
            saved += 1;
            Ok(())
        })
        .unwrap();
        assert_eq!(saved, 1, "applied once");
        // What Inno's SaveStringToFile may add around the value is tolerated.
        std::fs::write(scratch.handoff(), "\u{feff}auto\r\n").unwrap();
        assert_eq!(installer_choice(&scratch.0), Some(LanguageChoice::Auto));
        std::fs::write(scratch.handoff(), "zh-Hant").unwrap();
        assert_eq!(
            installer_choice(&scratch.0),
            Some(LanguageChoice::Fixed(Language::ZhHant))
        );
    }

    #[test]
    fn a_failed_save_keeps_the_choice_for_this_run_and_the_file_for_the_next() {
        let scratch = Scratch::new("unsaved");
        std::fs::write(scratch.handoff(), "ja").unwrap();
        let mut settings = Settings::default();
        let result =
            adopt_installer_choice(&mut settings, &scratch.0, |_| Err("disk full".to_string()));
        assert_eq!(result, Err("disk full".to_string()));
        assert_eq!(settings.language, LanguageChoice::Fixed(Language::Ja));
        assert!(scratch.handoff().exists(), "the next start tries again");
        // The next start reads its settings from disk: still the old language.
        let mut next = Settings::default();
        let mut saved = None;
        adopt_installer_choice(&mut next, &scratch.0, |s| {
            saved = Some(s.language);
            Ok(())
        })
        .unwrap();
        assert_eq!(saved, Some(LanguageChoice::Fixed(Language::Ja)));
        assert!(!scratch.handoff().exists());
    }

    #[test]
    fn the_resident_starts_even_when_the_installer_choice_cannot_be_saved() {
        let scratch = Scratch::new("start");
        std::fs::write(scratch.handoff(), "ja").unwrap();
        let mut settings = Settings::default();
        let mut heard = None;
        // `apply` is recorded, not run: the process-wide language other tests read stays.
        let mut applied = None;
        start_with_installer_choice(
            &mut settings,
            &scratch.0,
            |_| Err("Access is denied.".to_string()),
            |s| applied = Some(s.language),
            |detail| heard = Some(detail),
        );
        assert_eq!(heard.as_deref(), Some("Access is denied."), "the notice");
        assert_eq!(settings.language, LanguageChoice::Fixed(Language::Ja));
        assert_eq!(
            applied,
            Some(LanguageChoice::Fixed(Language::Ja)),
            "applied anyway"
        );
        assert!(scratch.handoff().exists());
        // Stored this time: no notice, the file is gone.
        let mut settings = Settings::default();
        let mut heard = None;
        let mut applied = None;
        start_with_installer_choice(
            &mut settings,
            &scratch.0,
            |_| Ok(()),
            |s| applied = Some(s.language),
            |detail| heard = Some(detail),
        );
        assert_eq!(heard, None);
        assert_eq!(settings.language, LanguageChoice::Fixed(Language::Ja));
        assert_eq!(applied, Some(LanguageChoice::Fixed(Language::Ja)));
        assert!(!scratch.handoff().exists());
        // Nothing left by the installer: the language already applied is not applied again.
        let mut applied = false;
        start_with_installer_choice(
            &mut settings,
            &scratch.0,
            |_| Ok(()),
            |_| applied = true,
            |_| {},
        );
        assert!(!applied);
    }

    #[test]
    fn a_menu_choice_outranks_an_installer_choice_that_could_not_be_stored() {
        let scratch = Scratch::new("menu");
        // The installer left Japanese and the resident could not store it.
        std::fs::write(scratch.handoff(), "ja").unwrap();
        let next = Settings {
            language: LanguageChoice::Fixed(Language::De),
            ..Default::default()
        };
        // A failed save keeps the file: nothing was stored over it.
        let result = store_menu_choice(&next, &scratch.0, |_| Err("disk full".to_string()));
        assert_eq!(result, Err("disk full".to_string()));
        assert!(scratch.handoff().exists());
        // Stored: the file is gone, so the next start keeps German.
        let mut saved = None;
        store_menu_choice(&next, &scratch.0, |s| {
            saved = Some(s.language);
            Ok(())
        })
        .unwrap();
        assert_eq!(saved, Some(LanguageChoice::Fixed(Language::De)));
        assert!(!scratch.handoff().exists());
        let mut start = next.clone();
        let mut stored_again = false;
        adopt_installer_choice(&mut start, &scratch.0, |_| {
            stored_again = true;
            Ok(())
        })
        .unwrap();
        assert_eq!(start.language, LanguageChoice::Fixed(Language::De));
        assert!(!stored_again);
        // Without a file, a menu choice is stored all the same.
        store_menu_choice(&next, &scratch.0, |_| Ok(())).unwrap();
    }

    #[test]
    fn garbage_installer_file_is_ignored_and_removed() {
        let scratch = Scratch::new("garbage");
        for garbage in [
            &b"klingon"[..],
            b"",
            b"fr\nde",
            b"\xff\xfe\x00",
            b"auto-ish",
        ] {
            std::fs::write(scratch.handoff(), garbage).unwrap();
            assert_eq!(installer_choice(&scratch.0), None, "{garbage:?}");
            assert!(!scratch.handoff().exists(), "{garbage:?} is removed");
        }
    }

    #[test]
    fn other_settings_are_untouched() {
        let scratch = Scratch::new("untouched");
        let path = scratch.0.join("settings.txt");
        let stored = Settings {
            key: 44,
            modifiers: 2,
            last_custom_color: [12, 34, 56],
            capture_stroke_width: 7,
            capture_eraser_size: 30,
            start_with_windows: true,
            board_visible: true,
            ..Default::default()
        };
        let before = stored.encode();
        std::fs::write(&path, &before).unwrap();
        std::fs::write(scratch.handoff(), "ja").unwrap();
        let rename = |temp: &Path, to: &Path| std::fs::rename(temp, to).map_err(|e| e.to_string());
        let mut settings = Settings::load_from(&path).unwrap();
        adopt_installer_choice(&mut settings, &scratch.0, |s| {
            s.save_to(&path, None, None, rename)
        })
        .unwrap();
        assert_eq!(settings.language, LanguageChoice::Fixed(Language::Ja));
        let after = std::fs::read_to_string(&path).unwrap();
        let changed: Vec<(&str, &str)> = before
            .lines()
            .zip(after.lines())
            .filter(|(a, b)| a != b)
            .collect();
        assert_eq!(changed, [("auto", "ja")], "only the language line changes");
        assert_eq!(
            after,
            before.replacen("\nauto\n", "\nja\n", 1),
            "byte for byte"
        );
        assert!(!scratch.handoff().exists());

        // The same choice again writes nothing.
        std::fs::write(scratch.handoff(), "ja").unwrap();
        let mut saved = false;
        adopt_installer_choice(&mut settings, &scratch.0, |_| {
            saved = true;
            Ok(())
        })
        .unwrap();
        assert!(!saved);
        assert!(!scratch.handoff().exists());
    }

    #[test]
    fn a_fixed_choice_wins_over_the_system() {
        // English is the process default, so this does not disturb tests running beside it.
        let settings = Settings {
            language: LanguageChoice::Fixed(Language::En),
            ..Default::default()
        };
        apply(&settings);
        assert_eq!(language(), Language::En);
    }
}
