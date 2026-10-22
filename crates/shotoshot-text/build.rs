//! Turns `locales/<tag>.txt` into `$OUT_DIR/keys.rs` (the `Key` enum and the translation table)
//! and fails the build, with every problem listed, when a language is inconsistent with English.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::Path;
use std::{env, fs};

#[path = "src/parse.rs"]
mod parse;

// The library's own list: the tables are built in the order of `Language::ALL`, so they cannot
// drift apart.
#[path = "src/language.rs"]
#[allow(dead_code)]
mod language;

use language::Language;
use parse::{Entry, Same};

/// Language tags in table order: the order of `Language::ALL`.
fn tags() -> [&'static str; 15] {
    Language::ALL.map(Language::tag)
}

/// `toolbar.rectangle` becomes `ToolbarRectangle`.
fn variant_name(key: &str) -> String {
    key.split(['.', '_'])
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            let first = chars.next().map(|c| c.to_ascii_uppercase());
            first.into_iter().chain(chars).collect::<String>()
        })
        .collect()
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src/parse.rs");
    println!("cargo:rerun-if-changed=src/language.rs");
    println!("cargo:rerun-if-changed=locales");

    let manifest = env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    let directory = Path::new(&manifest).join("locales");
    let mut errors: Vec<String> = Vec::new();
    let tags = tags();

    // A file that is not `<tag>.txt` for a known tag is a typo or a language not wired in.
    match fs::read_dir(&directory) {
        Ok(listing) => {
            for entry in listing.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                let known = name
                    .strip_suffix(".txt")
                    .is_some_and(|tag| tags.contains(&tag));
                if !known {
                    errors.push(format!(
                        "locales/{name}: not a locale file of a known language"
                    ));
                }
            }
        }
        Err(problem) => errors.push(format!("locales: cannot be listed: {problem}")),
    }

    // None = the file could not be read or parsed; the reason is already in `errors`.
    let mut tables: Vec<Option<Vec<Entry>>> = Vec::new();
    for tag in tags {
        let path = directory.join(format!("{tag}.txt"));
        let source = match fs::read_to_string(&path) {
            Ok(source) => source,
            Err(problem) => {
                errors.push(format!(
                    "{tag}: locales/{tag}.txt cannot be read: {problem}"
                ));
                tables.push(None);
                continue;
            }
        };
        match parse::parse(&source) {
            Ok(entries) => tables.push(Some(entries)),
            Err(problems) => {
                errors.extend(
                    problems
                        .into_iter()
                        .map(|p| format!("{tag}: locales/{tag}.txt: {p}")),
                );
                tables.push(None);
            }
        }
    }

    let english = tables[0].clone().unwrap_or_default();
    for (key, _, same) in &english {
        if *same == Same::Yes {
            errors.push(format!(
                "en: key `{key}`: `==` has no meaning in en.txt, which is the source"
            ));
        }
    }
    if english.is_empty() && tables[0].is_some() {
        errors.push("en: locales/en.txt defines no key".to_string());
    }
    if tables[0].is_some() {
        for (tag, entries) in tags.iter().zip(&tables).skip(1) {
            if let Some(entries) = entries
                && let Err(problems) = parse::validate(tag, &english, entries)
            {
                errors.extend(problems);
            }
        }
    }

    // Variant names must be distinct, valid Rust identifiers.
    let mut variants: HashMap<String, &str> = HashMap::new();
    for (key, _, _) in &english {
        let name = variant_name(key);
        if !name.chars().next().is_some_and(|c| c.is_ascii_alphabetic()) {
            errors.push(format!(
                "en: key `{key}` does not give a variant name starting with a letter"
            ));
        } else if let Some(other) = variants.insert(name.clone(), key) {
            errors.push(format!(
                "en: keys `{other}` and `{key}` both give the variant `{name}`"
            ));
        }
    }

    if !errors.is_empty() {
        panic!(
            "the translation tables are invalid:\n{}\n",
            errors.join("\n")
        );
    }

    let mut out = String::new();
    let count = english.len();
    out.push_str("/// A translatable text of the interface; `locales/en.txt` defines the keys.\n");
    out.push_str("#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]\npub enum Key {\n");
    for (key, _, _) in &english {
        let _ = writeln!(out, "    /// `{key}`\n    {},", variant_name(key));
    }
    out.push_str("}\n\n");
    let _ = writeln!(out, "static IDS: [&str; {count}] = [");
    for (key, _, _) in &english {
        let _ = writeln!(out, "    {key:?},");
    }
    out.push_str("];\n\n");
    let _ = writeln!(out, "static PLACEHOLDERS: [&[&str]; {count}] = [");
    for (_, text, _) in &english {
        let names: Vec<String> = parse::placeholders(text)
            .iter()
            .map(|n| format!("{n:?}"))
            .collect();
        let _ = writeln!(out, "    &[{}],", names.join(", "));
    }
    out.push_str("];\n\n");
    out.push_str("impl Key {\n    /// Every key, in the order of `en.txt`.\n");
    let _ = writeln!(out, "    pub const ALL: &[Key] = &[");
    for (key, _, _) in &english {
        let _ = writeln!(out, "        Key::{},", variant_name(key));
    }
    out.push_str("    ];\n\n");
    out.push_str("    /// The dotted key as written in the locale files.\n");
    out.push_str("    pub fn id(self) -> &'static str {\n        IDS[self as usize]\n    }\n\n");
    out.push_str("    /// The variable names of the text, sorted.\n");
    out.push_str("    pub fn placeholders(self) -> &'static [&'static str] {\n        PLACEHOLDERS[self as usize]\n    }\n}\n\n");
    let _ = writeln!(out, "static TABLE: [[&str; {count}]; {}] = [", tags.len());
    for (tag, entries) in tags.iter().zip(tables.iter().flatten()) {
        let by_key: HashMap<&str, &str> = entries
            .iter()
            .map(|(k, t, _)| (k.as_str(), t.as_str()))
            .collect();
        let _ = writeln!(out, "    // {tag}\n    [");
        for (key, _, _) in &english {
            let _ = writeln!(out, "        {:?},", by_key[key.as_str()]);
        }
        out.push_str("    ],\n");
    }
    out.push_str("];\n");

    let target = Path::new(&env::var("OUT_DIR").expect("OUT_DIR")).join("keys.rs");
    fs::write(target, out).expect("write keys.rs");
}
