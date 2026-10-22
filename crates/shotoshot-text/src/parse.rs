//! Parser and validator of the locale files (`locales/<tag>.txt`).
//!
//! This file is compiled twice: as a module of the library (tests only) and by `build.rs` through
//! `#[path]`, so it must not depend on anything else of the crate.

/// Whether an entry is declared identical to the English text on purpose (`key == text`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Same {
    No,
    Yes,
}

/// One line of a locale file: key, unescaped text, and the `==` marker.
pub type Entry = (String, String, Same);

use std::collections::{HashMap, HashSet};

/// A key is dot-separated segments of `[a-z0-9_]+`.
fn is_valid_key(key: &str) -> bool {
    !key.is_empty()
        && key.split('.').all(|segment| {
            !segment.is_empty()
                && segment
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
        })
}

fn is_valid_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

/// Resolves `\n` and `\\`; any other escape is an error.
fn unescape(text: &str) -> Result<String, String> {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('\\') => out.push('\\'),
            Some(other) => return Err(format!("unknown escape `\\{other}` (only \\n and \\\\)")),
            None => return Err("text ends with a lone backslash".to_string()),
        }
    }
    Ok(out)
}

/// Every `{` must open a `{name}` variable and every `}` must close one.
fn check_braces(text: &str) -> Result<(), String> {
    let mut rest = text;
    while let Some(at) = rest.find(['{', '}']) {
        if rest.as_bytes()[at] == b'}' {
            return Err("`}` without a matching `{`".to_string());
        }
        let after = &rest[at + 1..];
        let Some(end) = after.find('}') else {
            return Err("`{` without a closing `}`".to_string());
        };
        if !is_valid_name(&after[..end]) {
            return Err(format!(
                "`{{{}}}` is not a valid variable name",
                &after[..end]
            ));
        }
        rest = &after[end + 1..];
    }
    Ok(())
}

/// Parses a locale file. Every problem is reported, one message per problem.
pub fn parse(source: &str) -> Result<Vec<Entry>, Vec<String>> {
    if source.starts_with('\u{feff}') {
        return Err(vec![
            "the file starts with a byte order mark; save it as UTF-8 without BOM".to_string(),
        ]);
    }
    let mut entries = Vec::new();
    let mut errors = Vec::new();
    let mut seen = HashSet::new();
    for (index, raw) in source.lines().enumerate() {
        let number = index + 1;
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, rest)) = line.split_once('=') else {
            errors.push(format!("line {number}: expected `key = text`"));
            continue;
        };
        let key = key.trim();
        if !is_valid_key(key) {
            errors.push(format!(
                "line {number}: invalid key `{key}` (lowercase letters, digits, `_`, dot-separated)"
            ));
            continue;
        }
        let (same, text) = match rest.strip_prefix('=') {
            Some(text) => (Same::Yes, text),
            None => (Same::No, rest),
        };
        let text = match unescape(text.trim()) {
            Ok(text) if text.is_empty() => {
                errors.push(format!("line {number}: key `{key}` has no text"));
                continue;
            }
            Ok(text) => text,
            Err(problem) => {
                errors.push(format!("line {number}: key `{key}`: {problem}"));
                continue;
            }
        };
        if let Err(problem) = check_braces(&text) {
            errors.push(format!("line {number}: key `{key}`: {problem}"));
            continue;
        }
        if !seen.insert(key.to_string()) {
            errors.push(format!("line {number}: duplicate key `{key}`"));
            continue;
        }
        entries.push((key.to_string(), text, same));
    }
    if errors.is_empty() {
        Ok(entries)
    } else {
        Err(errors)
    }
}

/// The variable names of a text, sorted and without duplicates.
pub fn placeholders(text: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find('{') {
        let after = &rest[start + 1..];
        let Some(end) = after.find('}') else { break };
        if is_valid_name(&after[..end]) {
            names.push(after[..end].to_string());
        }
        rest = &after[end + 1..];
    }
    names.sort();
    names.dedup();
    names
}

/// A text with no word (two letters in a row) outside its variables, such as `{n} s` or `{n}%`,
/// cannot be translated and may equal the English text without being marked.
fn is_letterless(text: &str) -> bool {
    let mut without_variables = String::new();
    let mut in_variable = false;
    for c in text.chars() {
        match c {
            '{' => in_variable = true,
            '}' => in_variable = false,
            _ if !in_variable => without_variables.push(c),
            _ => {}
        }
    }
    let mut previous_letter = false;
    for c in without_variables.chars() {
        let letter = c.is_alphabetic();
        if letter && previous_letter {
            return false;
        }
        previous_letter = letter;
    }
    true
}

/// Checks one language against English: same keys, same variables, no unmarked copy of English,
/// no `==` mark on a text that differs. Returns every problem found.
pub fn validate(tag: &str, en: &[Entry], other: &[Entry]) -> Result<(), Vec<String>> {
    let mut errors = Vec::new();
    let translated: HashMap<&str, (&str, Same)> = other
        .iter()
        .map(|(key, text, same)| (key.as_str(), (text.as_str(), *same)))
        .collect();
    let known: HashSet<&str> = en.iter().map(|(key, _, _)| key.as_str()).collect();
    for (key, english, _) in en {
        let Some(&(text, same)) = translated.get(key.as_str()) else {
            errors.push(format!("{tag}: missing key `{key}`"));
            continue;
        };
        let expected = placeholders(english);
        let found = placeholders(text);
        if expected != found {
            errors.push(format!(
                "{tag}: key `{key}`: variables {found:?} differ from English {expected:?}"
            ));
        }
        match same {
            Same::Yes if text != english => errors.push(format!(
                "{tag}: key `{key}` is marked `==` (same as English) but its text differs"
            )),
            Same::No if text == english && !is_letterless(english) => errors.push(format!(
                "{tag}: key `{key}` is a copy of the English text; translate it, or write `==` if it is meant to stay identical"
            )),
            _ => {}
        }
    }
    for (key, _, _) in other {
        if !known.contains(key.as_str()) {
            errors.push(format!(
                "{tag}: unknown key `{key}` (not defined in en.txt)"
            ));
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entries(source: &str) -> Vec<Entry> {
        parse(source).expect("test source parses")
    }

    #[test]
    fn parses_entries_comments_escapes() {
        let parsed = entries("# c\n\na.b = One\\nTwo\nc == X\n");
        assert_eq!(
            parsed,
            vec![
                ("a.b".to_string(), "One\nTwo".to_string(), Same::No),
                ("c".to_string(), "X".to_string(), Same::Yes),
            ]
        );
    }

    #[test]
    fn parses_backslash_and_crlf() {
        let parsed = entries("a = C:\\\\x\r\nb = y\r\n");
        assert_eq!(parsed[0].1, "C:\\x");
        assert_eq!(parsed[1].1, "y");
    }

    #[test]
    fn rejects_duplicate_key() {
        let errors = parse("a = One\na = Two\n").unwrap_err();
        assert!(
            errors
                .iter()
                .any(|e| e.contains("duplicate") && e.contains('a'))
        );
    }

    #[test]
    fn rejects_bad_key() {
        assert!(parse("A.b = x\n").is_err());
        assert!(parse("a..b = x\n").is_err());
        assert!(parse(".a = x\n").is_err());
        assert!(parse(" = x\n").is_err());
    }

    #[test]
    fn rejects_line_without_separator() {
        assert!(parse("just text\n").is_err());
    }

    #[test]
    fn rejects_bad_escape_empty_text_and_bad_braces() {
        assert!(parse("a = \\q\n").is_err());
        assert!(parse("a = tail\\\n").is_err());
        assert!(parse("a =\n").is_err());
        assert!(parse("a = {open\n").is_err());
        assert!(parse("a = close}\n").is_err());
        assert!(parse("a = {Bad Name}\n").is_err());
        assert!(parse("a = {}\n").is_err());
    }

    #[test]
    fn rejects_byte_order_mark() {
        assert!(parse("\u{feff}a = x\n").is_err());
    }

    #[test]
    fn reports_every_parse_error() {
        let errors = parse("A = x\nb\nc = y\nc = z\n").unwrap_err();
        assert_eq!(errors.len(), 3);
    }

    #[test]
    fn placeholders_are_collected_sorted() {
        assert_eq!(placeholders("Saved {path} at {time}"), ["path", "time"]);
        assert_eq!(placeholders("{b} {a} {b}"), ["a", "b"]);
        assert!(placeholders("plain").is_empty());
    }

    fn pair(en: &str, other: &str) -> Result<(), Vec<String>> {
        validate("fr", &entries(en), &entries(other))
    }

    #[test]
    fn valid_translation_passes() {
        assert!(
            pair(
                "a = Save {path}\nb == OK\n",
                "a = Enregistrer {path}\nb == OK\n"
            )
            .is_ok()
        );
    }

    #[test]
    fn missing_key_is_reported() {
        let errors = pair("a = One\nb = Two\n", "a = Un\n").unwrap_err();
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("fr") && errors[0].contains("`b`"));
    }

    #[test]
    fn unknown_key_is_reported() {
        let errors = pair("a = One\n", "a = Un\nz = Zed\n").unwrap_err();
        assert!(errors[0].contains("fr") && errors[0].contains("`z`"));
    }

    #[test]
    fn placeholder_mismatch_is_reported() {
        let errors = pair("a = Saved {path}\n", "a = Enregistre {chemin}\n").unwrap_err();
        assert!(errors[0].contains("`a`") && errors[0].contains("path"));
        assert!(pair("a = Saved {path}\n", "a = Enregistre\n").is_err());
        assert!(pair("a = Saved\n", "a = Enregistre {path}\n").is_err());
    }

    #[test]
    fn copy_of_english_is_reported() {
        let errors = pair("a = Save\n", "a = Save\n").unwrap_err();
        assert!(errors[0].contains("`a`") && errors[0].contains("copy"));
    }

    #[test]
    fn marked_same_must_equal_english() {
        assert!(pair("a = Save\n", "a == Save\n").is_ok());
        let errors = pair("a = Save\n", "a == Enregistrer\n").unwrap_err();
        assert!(errors[0].contains("`a`") && errors[0].contains("=="));
    }

    #[test]
    fn letterless_text_may_equal_english() {
        assert!(pair("a = {n} s\nb = {n}%\n", "a = {n} s\nb = {n}%\n").is_ok());
        assert!(pair("a = {n} seconds\n", "a = {n} seconds\n").is_err());
    }

    #[test]
    fn all_errors_are_listed_together() {
        let errors = pair("a = One\nb = Two\nc = Three\n", "a = One\nz = Zed\n").unwrap_err();
        assert_eq!(errors.len(), 4);
    }
}
