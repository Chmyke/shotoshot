#![forbid(unsafe_code)]
//! Every visible text of Shotoshot, as tables generated at build time from `locales/<tag>.txt`.
//!
//! `locales/en.txt` defines the keys; `build.rs` refuses a build in which a language misses a key,
//! knows an extra one, changes the variables of a text or silently copies the English. Nothing is
//! read or parsed at run time: a lookup is an array index.

mod language;
#[cfg(test)]
mod parse;

use std::borrow::Cow;
use std::fmt;
use std::sync::atomic::{AtomicU8, Ordering};

pub use language::{Language, LanguageChoice};

include!(concat!(env!("OUT_DIR"), "/keys.rs"));

static CURRENT: AtomicU8 = AtomicU8::new(Language::En as u8);

/// Selects the language used by [`tr`] and [`Text::new`].
pub fn set_language(language: Language) {
    store(&CURRENT, language);
}

/// The language used by [`tr`] and [`Text::new`] (English until [`set_language`] is called).
pub fn language() -> Language {
    load(&CURRENT)
}

/// What [`set_language`] does to `cell`; tests use a cell of their own, never `CURRENT`.
fn store(cell: &AtomicU8, language: Language) {
    cell.store(language as u8, Ordering::Relaxed);
}

/// What [`language`] reads from `cell`.
fn load(cell: &AtomicU8) -> Language {
    Language::from_index(cell.load(Ordering::Relaxed))
}

/// The text of `key` in the current language, variables left as `{name}`.
pub fn tr(key: Key) -> &'static str {
    tr_in(language(), key)
}

/// The text of `key` in `language`, variables left as `{name}`.
pub fn tr_in(language: Language, key: Key) -> &'static str {
    TABLE[language as usize][key as usize]
}

/// The name of the language submenu of a notification-area menu, in `language`: its word, then
/// the English one ("言語 · Language"); "Language" alone in English.
pub fn language_menu_label_in(language: Language) -> Cow<'static, str> {
    if language == Language::En {
        return tr_in(language, Key::LanguageMenu).into();
    }
    // DELIBERATE: the English word stays after the translated one, so that someone who cannot
    // read the current language still finds the way back (Mickaël, 2026-10-17).
    format!("{} · Language", tr_in(language, Key::LanguageMenu)).into()
}

/// Replaces each `{name}` of `template` by its value in `args`; a variable without a value
/// becomes an empty string. Never panics.
fn substitute(template: &str, args: &[(&str, &str)]) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let Some(end) = after.find('}') else {
            out.push_str(&rest[start..]);
            return out;
        };
        let name = &after[..end];
        if let Some((_, value)) = args.iter().find(|(candidate, _)| *candidate == name) {
            out.push_str(value);
        }
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    out
}

/// A visible text. It can only be built from a [`Key`], never from a free string, so a text
/// written by hand in the interface does not compile.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Text(String);

impl Text {
    /// The text of `key` in the current language. Its variables, if any, become empty.
    pub fn new(key: Key) -> Text {
        Text::with(key, &[])
    }

    /// The text of `key` in the current language with its variables filled in.
    pub fn with(key: Key, args: &[(&str, &str)]) -> Text {
        Text::with_in(language(), key, args)
    }

    /// The text of `key` in `language`.
    pub fn new_in(language: Language, key: Key) -> Text {
        Text::with_in(language, key, &[])
    }

    /// The text of `key` in `language` with its variables filled in; a variable missing from
    /// `args` becomes an empty string.
    pub fn with_in(language: Language, key: Key, args: &[(&str, &str)]) -> Text {
        Text(substitute(tr_in(language, key), args))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Text {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_key_formats_without_leftover_braces() {
        for &language in &Language::ALL {
            for &key in Key::ALL {
                let values: Vec<(&str, &str)> = key
                    .placeholders()
                    .iter()
                    .map(|&name| (name, "VALUE"))
                    .collect();
                let text = Text::with_in(language, key, &values);
                assert!(
                    !text.as_str().contains(['{', '}']),
                    "{} / {}: `{}`",
                    language.tag(),
                    key.id(),
                    text
                );
            }
        }
    }

    #[test]
    fn missing_argument_leaves_no_brace() {
        for &language in &Language::ALL {
            for &key in Key::ALL {
                let text = Text::with_in(language, key, &[]);
                assert!(
                    !text.as_str().contains(['{', '}']),
                    "{} / {}",
                    language.tag(),
                    key.id()
                );
            }
        }
        assert_eq!(
            substitute("Saved {path} at {time}", &[("time", "now")]),
            "Saved  at now"
        );
        assert_eq!(substitute("{a}{a}", &[("a", "x")]), "xx");
        assert_eq!(substitute("no variables", &[("a", "x")]), "no variables");
        assert_eq!(substitute("{n} s", &[]), " s");
    }

    #[test]
    fn substitute_never_panics_on_malformed_templates() {
        assert_eq!(substitute("open {brace", &[]), "open {brace");
        assert_eq!(substitute("close} only", &[]), "close} only");
        assert_eq!(substitute("", &[]), "");
        assert_eq!(substitute("{}", &[("", "empty")]), "empty");
    }

    #[test]
    fn placeholders_are_sorted_and_unique() {
        for &key in Key::ALL {
            let names = key.placeholders();
            assert!(
                names.windows(2).all(|pair| pair[0] < pair[1]),
                "{}",
                key.id()
            );
        }
    }

    #[test]
    fn key_ids_are_unique_and_ordered_like_all() {
        let ids: Vec<&str> = Key::ALL.iter().map(|key| key.id()).collect();
        let mut sorted = ids.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), ids.len());
        for (index, &key) in Key::ALL.iter().enumerate() {
            assert_eq!(key as usize, index);
        }
    }

    #[test]
    fn seed_key_is_translated_in_every_language() {
        let expected = [
            "Language", "语言", "Язык", "Idioma", "Idioma", "Sprache", "言語", "Langue", "Język",
            "언어", "語言", "Dil", "Мова", "Lingua", "Jazyk",
        ];
        for (language, word) in Language::ALL.iter().zip(expected) {
            assert_eq!(
                tr_in(*language, Key::LanguageMenu),
                word,
                "{}",
                language.tag()
            );
            assert_eq!(Key::LanguageMenu.id(), "language.menu");
        }
        // Spanish and Portuguese share the seed word: a key they say differently tells their
        // tables apart.
        assert_eq!(tr_in(Language::Es, Key::MenuQuit), "Salir de Shotoshot");
        assert_eq!(tr_in(Language::PtBr, Key::MenuQuit), "Sair do Shotoshot");
    }

    #[test]
    fn no_long_dash_in_any_text() {
        // Rule of 2026-10-21: a long dash reads as machine-written text. A comma, a colon,
        // parentheses or a rewording replace it; only a lone "—" standing for an empty value
        // may stay.
        const EM: char = '\u{2014}'; // dash-ok: the character the rule forbids
        const SPACED_EN: &str = " \u{2013} ";
        for &language in &Language::ALL {
            for &key in Key::ALL {
                let text = tr_in(language, key);
                let lone_em = text.chars().eq([EM]);
                assert!(
                    lone_em || !(text.contains(EM) || text.contains(SPACED_EN)),
                    "{} / {}: `{}`",
                    language.tag(),
                    key.id(),
                    text
                );
            }
        }
    }

    #[test]
    fn japanese_writes_sudeni_in_kana_everywhere() {
        for &key in Key::ALL {
            assert!(
                !tr_in(Language::Ja, key).contains("既に"),
                "{}: すでに, as in the other texts",
                key.id()
            );
        }
    }

    #[test]
    // Lists the source tree, which the Linux test machine does not have; build.rs refuses a
    // stray or missing file there anyway.
    #[cfg_attr(
        not(windows),
        ignore = "lists the source tree, absent on the Linux test machine"
    )]
    fn locale_files_match_the_language_list() {
        // build.rs takes its tags and their order from `Language::ALL` (it includes
        // language.rs); the file set is checked here.
        let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("locales");
        let mut names: Vec<String> = std::fs::read_dir(directory)
            .expect("locales directory")
            .map(|entry| {
                entry
                    .expect("entry")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        names.sort();
        let mut expected: Vec<String> = Language::ALL
            .iter()
            .map(|language| format!("{}.txt", language.tag()))
            .collect();
        expected.sort();
        assert_eq!(names, expected);
    }

    #[test]
    fn language_menu_keeps_the_english_word() {
        assert_eq!(language_menu_label_in(Language::En), "Language");
        assert_eq!(language_menu_label_in(Language::Ja), "言語 · Language");
        for language in Language::ALL.into_iter().skip(1) {
            let label = language_menu_label_in(language);
            assert_eq!(
                label,
                format!("{} · Language", tr_in(language, Key::LanguageMenu)),
                "{}",
                language.tag()
            );
        }
    }

    #[test]
    fn current_language_selects_the_table() {
        // The process-wide language stays English: other tests read it in parallel.
        assert_eq!(language(), Language::En);
        assert_eq!(tr(Key::LanguageMenu), tr_in(language(), Key::LanguageMenu));
        assert_eq!(Text::new(Key::LanguageMenu).as_str(), "Language");
        // What set_language stores, language reads back, on a cell of the test's own.
        let cell = AtomicU8::new(Language::En as u8);
        for chosen in Language::ALL {
            store(&cell, chosen);
            assert_eq!(load(&cell), chosen);
        }
        store(&cell, Language::Fr);
        assert_eq!(tr_in(load(&cell), Key::LanguageMenu), "Langue");
        assert_eq!(
            Text::new_in(load(&cell), Key::LanguageMenu).to_string(),
            "Langue"
        );
    }
}
