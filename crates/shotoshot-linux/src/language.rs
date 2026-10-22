//! ROLE: Linux side of the interface language: the locale of the session, and the application
//! of the stored choice when a process starts.
//! INVARIANTS: Reads three environment variables and nothing else (law 2).
use shotoshot_core::settings::Settings;
use shotoshot_text::set_language;

/// The first non-empty of `LC_ALL`, `LC_MESSAGES` and `LANG`, in the order POSIX gives them
/// precedence for message catalogs.
fn pick(lc_all: Option<&str>, lc_messages: Option<&str>, lang: Option<&str>) -> String {
    [lc_all, lc_messages, lang]
        .into_iter()
        .flatten()
        .find(|value| !value.is_empty())
        .unwrap_or_default()
        .to_owned()
}

/// The locale name of the session (`fr_FR.UTF-8`), or an empty string when none is set.
pub fn system_tag() -> String {
    let read = |name| std::env::var(name).ok();
    pick(
        read("LC_ALL").as_deref(),
        read("LC_MESSAGES").as_deref(),
        read("LANG").as_deref(),
    )
}

/// Makes the language of `settings` (or the session's, for the automatic choice) the language
/// of this process. Called once, right after the settings are loaded.
pub fn apply(settings: &Settings) {
    set_language(settings.language.resolve(&system_tag()));
}

#[cfg(test)]
mod tests {
    use super::*;
    use shotoshot_text::Language;

    #[test]
    fn prefers_lc_all_then_lc_messages_then_lang() {
        assert_eq!(pick(Some("de_DE"), Some("fr_FR"), Some("it_IT")), "de_DE");
        assert_eq!(pick(Some(""), Some("fr_FR"), Some("it_IT")), "fr_FR");
        assert_eq!(pick(None, Some("fr_FR"), Some("it_IT")), "fr_FR");
        assert_eq!(pick(None, Some(""), Some("it_IT")), "it_IT");
        assert_eq!(pick(None, None, Some("it_IT")), "it_IT");
        assert_eq!(pick(None, None, None), "");
        assert_eq!(pick(Some(""), Some(""), Some("")), "");
    }

    #[test]
    fn a_picked_locale_selects_its_language() {
        let tag = pick(None, None, Some("pl_PL.UTF-8"));
        assert_eq!(Language::from_system(&tag), Language::Pl);
        assert_eq!(Language::from_system(&pick(None, None, None)), Language::En);
    }
}
