//! The fifteen interface languages and the way the system language selects one of them.

/// An interface language. The order of the variants is the order of the translation tables
/// (and of `build.rs`); `En` is the source and the fallback.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Language {
    #[default]
    En,
    ZhHans,
    Ru,
    Es,
    PtBr,
    De,
    Ja,
    Fr,
    Pl,
    Ko,
    ZhHant,
    Tr,
    Uk,
    It,
    Cs,
}

impl Language {
    /// Every language, in table order.
    pub const ALL: [Language; 15] = [
        Language::En,
        Language::ZhHans,
        Language::Ru,
        Language::Es,
        Language::PtBr,
        Language::De,
        Language::Ja,
        Language::Fr,
        Language::Pl,
        Language::Ko,
        Language::ZhHant,
        Language::Tr,
        Language::Uk,
        Language::It,
        Language::Cs,
    ];

    /// The language of a table index, English when out of range.
    pub(crate) fn from_index(index: u8) -> Language {
        Language::ALL
            .get(usize::from(index))
            .copied()
            .unwrap_or(Language::En)
    }

    /// The tag used in settings and as the locale file name (`fr`, `zh-Hans`, `pt-BR`).
    pub fn tag(self) -> &'static str {
        match self {
            Language::En => "en",
            Language::ZhHans => "zh-Hans",
            Language::Ru => "ru",
            Language::Es => "es",
            Language::PtBr => "pt-BR",
            Language::De => "de",
            Language::Ja => "ja",
            Language::Fr => "fr",
            Language::Pl => "pl",
            Language::Ko => "ko",
            Language::ZhHant => "zh-Hant",
            Language::Tr => "tr",
            Language::Uk => "uk",
            Language::It => "it",
            Language::Cs => "cs",
        }
    }

    /// The name of the language in itself, as shown in the language menu.
    pub fn native_name(self) -> &'static str {
        match self {
            Language::En => "English",
            Language::ZhHans => "简体中文",
            Language::Ru => "Русский",
            Language::Es => "Español",
            Language::PtBr => "Português (Brasil)",
            Language::De => "Deutsch",
            Language::Ja => "日本語",
            Language::Fr => "Français",
            Language::Pl => "Polski",
            Language::Ko => "한국어",
            Language::ZhHant => "繁體中文",
            Language::Tr => "Türkçe",
            Language::Uk => "Українська",
            Language::It => "Italiano",
            Language::Cs => "Čeština",
        }
    }

    /// The BCP 47 locale name handed to the text engines (DirectWrite shapes ideographs by it).
    pub fn locale_name(self) -> &'static str {
        match self {
            Language::En => "en-US",
            Language::ZhHans => "zh-CN",
            Language::Ru => "ru-RU",
            Language::Es => "es-ES",
            Language::PtBr => "pt-BR",
            Language::De => "de-DE",
            Language::Ja => "ja-JP",
            Language::Fr => "fr-FR",
            Language::Pl => "pl-PL",
            Language::Ko => "ko-KR",
            Language::ZhHant => "zh-TW",
            Language::Tr => "tr-TR",
            Language::Uk => "uk-UA",
            Language::It => "it-IT",
            Language::Cs => "cs-CZ",
        }
    }

    /// The language with exactly this tag, ignoring case.
    pub fn from_tag(tag: &str) -> Option<Language> {
        Language::ALL
            .into_iter()
            .find(|language| language.tag().eq_ignore_ascii_case(tag))
    }

    /// The language that best matches a system locale name (`fr-FR`, `de_AT.UTF-8@euro`,
    /// `zh-Hant-TW`, `C`). Portuguese and Spanish map to `pt-BR` and `es`; Chinese picks the
    /// script (or the region when the script is absent, simplified when neither tells);
    /// anything else is English.
    pub fn from_system(locale: &str) -> Language {
        let base = locale.split(['.', '@']).next().unwrap_or("");
        let mut parts = base.split(['-', '_']);
        let primary = parts.next().unwrap_or("").to_ascii_lowercase();
        match primary.as_str() {
            "zh" => {
                let mut script = None;
                let mut region = None;
                for part in parts {
                    if part.eq_ignore_ascii_case("hans") || part.eq_ignore_ascii_case("hant") {
                        script = Some(part.eq_ignore_ascii_case("hant"));
                    } else if region.is_none() {
                        region = Some(part.to_ascii_uppercase());
                    }
                }
                let traditional =
                    script.unwrap_or(matches!(region.as_deref(), Some("TW" | "HK" | "MO")));
                if traditional {
                    Language::ZhHant
                } else {
                    Language::ZhHans
                }
            }
            "pt" => Language::PtBr,
            "es" => Language::Es,
            "ru" => Language::Ru,
            "de" => Language::De,
            "ja" => Language::Ja,
            "fr" => Language::Fr,
            "pl" => Language::Pl,
            "ko" => Language::Ko,
            "tr" => Language::Tr,
            "uk" => Language::Uk,
            "it" => Language::It,
            "cs" => Language::Cs,
            _ => Language::En,
        }
    }
}

/// What the user chose in the settings: follow the system, or one fixed language.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LanguageChoice {
    #[default]
    Auto,
    Fixed(Language),
}

impl LanguageChoice {
    /// Every choice in the order of a language menu: the automatic one, then each language
    /// in table order.
    pub const ALL: [LanguageChoice; 1 + Language::ALL.len()] = {
        let mut choices = [LanguageChoice::Auto; 1 + Language::ALL.len()];
        let mut index = 0;
        while index < Language::ALL.len() {
            choices[index + 1] = LanguageChoice::Fixed(Language::ALL[index]);
            index += 1;
        }
        choices
    };

    /// The place of this choice in [`LanguageChoice::ALL`].
    pub fn position(self) -> usize {
        match self {
            LanguageChoice::Auto => 0,
            LanguageChoice::Fixed(language) => 1 + language as usize,
        }
    }

    /// The value stored in the settings: `auto` or a language tag.
    pub fn encode(self) -> &'static str {
        match self {
            LanguageChoice::Auto => "auto",
            LanguageChoice::Fixed(language) => language.tag(),
        }
    }

    /// Reads a stored value; anything unknown means `Auto`.
    pub fn decode(value: &str) -> LanguageChoice {
        Language::from_tag(value.trim()).map_or(LanguageChoice::Auto, LanguageChoice::Fixed)
    }

    /// The language to use, given the system locale name for `Auto`.
    pub fn resolve(self, system_tag: &str) -> Language {
        match self {
            LanguageChoice::Auto => Language::from_system(system_tag),
            LanguageChoice::Fixed(language) => language,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_system_maps_variants() {
        let cases = [
            ("pt-PT", Language::PtBr),
            ("pt_BR", Language::PtBr),
            ("es-MX", Language::Es),
            ("zh-CN", Language::ZhHans),
            ("zh-SG", Language::ZhHans),
            ("zh-Hans-CN", Language::ZhHans),
            ("zh-TW", Language::ZhHant),
            ("zh_HK.UTF-8", Language::ZhHant),
            ("zh-MO", Language::ZhHant),
            ("zh-Hant-TW", Language::ZhHant),
            ("zh-Hant", Language::ZhHant),
            ("zh", Language::ZhHans),
            ("de_AT.UTF-8@euro", Language::De),
            ("fr-CA", Language::Fr),
            ("FR-fr", Language::Fr),
            ("uk-UA", Language::Uk),
            ("nl-NL", Language::En),
            ("en-GB", Language::En),
            ("C", Language::En),
            ("POSIX", Language::En),
            ("", Language::En),
        ];
        for (locale, expected) in cases {
            assert_eq!(Language::from_system(locale), expected, "{locale}");
        }
    }

    #[test]
    fn every_tag_and_locale_name_maps_back_to_its_language() {
        for language in Language::ALL {
            assert_eq!(Language::from_tag(language.tag()), Some(language));
            assert_eq!(
                Language::from_tag(&language.tag().to_ascii_uppercase()),
                Some(language)
            );
            assert_eq!(Language::from_system(language.locale_name()), language);
            assert_eq!(Language::from_index(language as u8), language);
        }
        assert_eq!(Language::from_tag("pt"), None);
        assert_eq!(Language::from_tag("ar"), None);
        assert_eq!(Language::from_index(200), Language::En);
    }

    #[test]
    fn choice_roundtrip_and_unknown_is_auto() {
        assert_eq!(
            LanguageChoice::decode("fr"),
            LanguageChoice::Fixed(Language::Fr)
        );
        assert_eq!(LanguageChoice::decode("ar"), LanguageChoice::Auto);
        assert_eq!(LanguageChoice::decode(""), LanguageChoice::Auto);
        assert_eq!(LanguageChoice::Auto.encode(), "auto");
        assert_eq!(LanguageChoice::decode("auto"), LanguageChoice::Auto);
        for language in Language::ALL {
            let choice = LanguageChoice::Fixed(language);
            assert_eq!(LanguageChoice::decode(choice.encode()), choice);
        }
    }

    #[test]
    fn menu_choices_start_with_auto_then_follow_the_table() {
        assert_eq!(LanguageChoice::ALL[0], LanguageChoice::Auto);
        for (index, choice) in LanguageChoice::ALL.into_iter().enumerate() {
            assert_eq!(choice.position(), index);
        }
        for (language, choice) in Language::ALL.into_iter().zip(&LanguageChoice::ALL[1..]) {
            assert_eq!(*choice, LanguageChoice::Fixed(language));
        }
    }

    #[test]
    fn resolve_follows_the_system_only_for_auto() {
        assert_eq!(LanguageChoice::Auto.resolve("ja-JP"), Language::Ja);
        assert_eq!(
            LanguageChoice::Fixed(Language::De).resolve("ja-JP"),
            Language::De
        );
        assert_eq!(LanguageChoice::Auto.resolve("nl-NL"), Language::En);
    }

    #[test]
    fn native_names_match_spec() {
        let expected = [
            "English",
            "简体中文",
            "Русский",
            "Español",
            "Português (Brasil)",
            "Deutsch",
            "日本語",
            "Français",
            "Polski",
            "한국어",
            "繁體中文",
            "Türkçe",
            "Українська",
            "Italiano",
            "Čeština",
        ];
        let names: Vec<&str> = Language::ALL.iter().map(|l| l.native_name()).collect();
        assert_eq!(names, expected);
    }

    #[test]
    fn tags_and_locale_names_match_spec() {
        let tags: Vec<&str> = Language::ALL.iter().map(|l| l.tag()).collect();
        assert_eq!(
            tags,
            [
                "en", "zh-Hans", "ru", "es", "pt-BR", "de", "ja", "fr", "pl", "ko", "zh-Hant",
                "tr", "uk", "it", "cs"
            ]
        );
        let locales: Vec<&str> = Language::ALL.iter().map(|l| l.locale_name()).collect();
        assert_eq!(
            locales,
            [
                "en-US", "zh-CN", "ru-RU", "es-ES", "pt-BR", "de-DE", "ja-JP", "fr-FR", "pl-PL",
                "ko-KR", "zh-TW", "tr-TR", "uk-UA", "it-IT", "cs-CZ",
            ]
        );
    }
}
