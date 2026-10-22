//! ROLE: What the user reads when something fails outside the editor, as on Windows: a failure
//! meant for them is already a translated text, any other reads as "Something went wrong."
//! followed by its technical detail as is.
//! INVARIANTS: A visible failure is worded once, in the language of the process when it fails;
//! the language is applied before anything can fail (`lib::dispatch`).
use shotoshot_text::{Key, Language, Text};
use std::error::Error;

/// A failure the user is meant to read: its text, already in the interface language.
#[derive(Debug)]
pub struct Visible(String);
impl std::fmt::Display for Visible {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl Error for Visible {}

/// `text` as a failure the user reads as it is.
pub fn visible(text: impl std::fmt::Display) -> Visible {
    Visible(text.to_string())
}

/// What the message window says for `error`, in `language`: a visible failure as it is, any
/// other as `error.unexpected` with its technical text as `{detail}`.
pub fn text_in(error: &(dyn Error + 'static), language: Language) -> String {
    match error.downcast_ref::<Visible>() {
        Some(visible) => visible.0.clone(),
        None => Text::with_in(
            language,
            Key::ErrorUnexpected,
            &[("detail", &error.to_string())],
        )
        .to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shotoshot_text::tr_in;

    #[test]
    fn a_visible_failure_reads_as_it_is() {
        let text = Text::new_in(Language::De, Key::ErrorScreenshotRefused);
        let error: Box<dyn Error> = Box::new(visible(&text));
        for language in Language::ALL {
            assert_eq!(text_in(error.as_ref(), language), text.as_str());
        }
        // `?` keeps it visible on its way up.
        let refused = || -> Result<(), Box<dyn Error>> {
            let region: Option<()> = None;
            region.ok_or_else(|| visible(tr_in(Language::En, Key::ErrorNoRegion)))?;
            Ok(())
        };
        let error = refused().unwrap_err();
        assert_eq!(
            text_in(error.as_ref(), Language::Fr),
            "Select a capture region first"
        );
    }

    #[test]
    fn any_other_failure_reads_as_unexpected_with_its_detail() {
        let error: Box<dyn Error> = "XDG_RUNTIME_DIR is not set".into();
        assert_eq!(
            text_in(error.as_ref(), Language::En),
            "Something went wrong.\nXDG_RUNTIME_DIR is not set"
        );
        assert_eq!(
            text_in(error.as_ref(), Language::Ja),
            Text::with_in(
                Language::Ja,
                Key::ErrorUnexpected,
                &[("detail", "XDG_RUNTIME_DIR is not set")]
            )
            .as_str()
        );
        let io: Box<dyn Error> = Box::new(std::io::Error::other("disk full"));
        assert_eq!(
            text_in(io.as_ref(), Language::En),
            "Something went wrong.\ndisk full"
        );
    }
}
