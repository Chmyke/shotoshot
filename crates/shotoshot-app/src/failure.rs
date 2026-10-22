//! ROLE: What the user reads when something fails: a failure meant for them carries its text
//! key inside the `windows` error that travels up, any other one reads as "Something went
//! wrong." followed by its technical detail as is, code included.
//! INVARIANTS: A displayed failure is always a translated text; nothing is translated twice.
use shotoshot_text::{Key, Language, Text, language};
use windows::core::{Error, HRESULT};

/// Marks an error whose message is a text key and its values, not a technical detail.
const VISIBLE: HRESULT = HRESULT(0xA053_0001_u32 as i32);
/// Separates the key and each `name`/`value` pair in the message of a visible error.
const FIELD: char = '\u{1f}';
const VALUE: char = '\u{1e}';

/// A failure the user is meant to read: `key` with its variables, in the language of the moment
/// it is shown.
pub(crate) fn visible(key: Key, args: &[(&str, &str)]) -> Error {
    let mut message = key.id().to_string();
    for (name, value) in args {
        message.push(FIELD);
        message.push_str(name);
        message.push(VALUE);
        message.push_str(value);
    }
    Error::new(VISIBLE, message)
}

/// The key and values of a visible failure, `None` for any other error.
fn decode(error: &Error) -> Option<(Key, Vec<(String, String)>)> {
    if error.code() != VISIBLE {
        return None;
    }
    let message = error.message();
    let mut fields = message.split(FIELD);
    let id = fields.next()?;
    let key = *Key::ALL.iter().find(|key| key.id() == id)?;
    let args = fields
        .map(|field| {
            let (name, value) = field.split_once(VALUE).unwrap_or((field, ""));
            (name.to_string(), value.to_string())
        })
        .collect();
    Some((key, args))
}

/// What a message box shows for `error`, in the interface language.
pub fn text(error: &Error) -> Text {
    text_in(error, language())
}
pub(crate) fn text_in(error: &Error, language: Language) -> Text {
    worded_in(error, language, error.to_string())
}

/// Like [`text`], for a box that showed the technical message alone before translation: any
/// other error reads as "Something went wrong." followed by its message, without its code.
pub fn brief_text(error: &Error) -> Text {
    brief_text_in(error, language())
}
pub(crate) fn brief_text_in(error: &Error, language: Language) -> Text {
    worded_in(error, language, error.message())
}

/// The text of a visible failure, or `technical` after "Something went wrong.".
fn worded_in(error: &Error, language: Language, technical: String) -> Text {
    match decode(error) {
        Some((key, args)) => {
            let args: Vec<(&str, &str)> = args
                .iter()
                .map(|(name, value)| (name.as_str(), value.as_str()))
                .collect();
            Text::with_in(language, key, &args)
        }
        None => Text::with_in(language, Key::ErrorUnexpected, &[("detail", &technical)]),
    }
}

/// `error` as the `{detail}` of another text: the translated text of a visible failure, the
/// technical message and code of any other.
pub fn detail(error: &Error) -> String {
    detail_in(error, language())
}
pub(crate) fn detail_in(error: &Error, language: Language) -> String {
    if decode(error).is_some() {
        text_in(error, language).to_string()
    } else {
        error.to_string()
    }
}

/// Like [`detail`], for a text that showed the technical message alone before translation:
/// any other error gives its message without its code.
pub fn reason(error: &Error) -> String {
    reason_in(error, language())
}
pub(crate) fn reason_in(error: &Error, language: Language) -> String {
    if decode(error).is_some() {
        text_in(error, language).to_string()
    } else {
        error.message()
    }
}

/// `error` for a log on stderr: the translated text of a visible failure, the technical
/// message and code of any other.
pub fn log_line(error: &Error) -> String {
    detail(error)
}

#[cfg(test)]
mod tests {
    use super::*;
    use shotoshot_text::tr_in;
    use windows::Win32::Foundation::E_FAIL;

    #[test]
    fn a_visible_failure_keeps_its_key_and_values() {
        let args = [
            ("path", r"C:\notes\a.shoto"),
            ("detail", "Access is denied."),
        ];
        let error = visible(Key::ErrorDocumentUnread, &args);
        for language in Language::ALL {
            assert_eq!(
                text_in(&error, language),
                Text::with_in(language, Key::ErrorDocumentUnread, &args)
            );
            assert_eq!(
                detail_in(&error, language),
                Text::with_in(language, Key::ErrorDocumentUnread, &args).to_string()
            );
        }
        assert_eq!(
            text_in(&error, Language::En).as_str(),
            r"Could not open C:\notes\a.shoto: Access is denied."
        );
        let bare = visible(Key::ErrorEditorStart, &[]);
        assert_eq!(
            text_in(&bare, Language::De).as_str(),
            tr_in(Language::De, Key::ErrorEditorStart)
        );
    }

    #[test]
    fn any_other_error_reads_as_unexpected_with_its_detail() {
        // The technical detail keeps its code, as the message box showed it before translation.
        let error = Error::new(E_FAIL, "Image frame missing");
        assert_eq!(
            text_in(&error, Language::En).as_str(),
            "Something went wrong.\nImage frame missing (0x80004005)"
        );
        assert_eq!(
            text_in(&error, Language::Fr),
            Text::with_in(
                Language::Fr,
                Key::ErrorUnexpected,
                &[("detail", "Image frame missing (0x80004005)")]
            )
        );
        assert_eq!(
            detail_in(&error, Language::Fr),
            "Image frame missing (0x80004005)"
        );
        assert_eq!(detail_in(&error, Language::Fr), log_line(&error));
        // A visible code with an unknown key is not trusted as one.
        let forged = Error::new(VISIBLE, "no.such.key");
        assert_eq!(
            text_in(&forged, Language::En).as_str(),
            "Something went wrong.\nno.such.key (0xA0530001)"
        );
    }

    /// Each place that shows a technical detail keeps, in English, the wording it had before
    /// translation: the code only where the text showed it then.
    #[test]
    fn english_technical_details_read_as_before_translation() {
        let error = Error::new(E_FAIL, "Hotkey already registered");
        let en = Language::En;
        let with = |key: Key, detail: &str| Text::with_in(en, key, &[("detail", detail)]);
        // Instant screenshot toast, shortcut window line, Google Lens box: message alone.
        assert_eq!(
            with(Key::NoticeScreenshotError, &reason_in(&error, en)).as_str(),
            "Screenshot failed: Hotkey already registered"
        );
        assert_eq!(
            with(Key::SetupInvalid, &reason_in(&error, en)).as_str(),
            "Shortcut unavailable or invalid: Hotkey already registered"
        );
        assert_eq!(
            with(Key::ErrorLensFailed, &reason_in(&error, en)).as_str(),
            "Google search failed. Your capture is still open.\nHotkey already registered"
        );
        // Presentation start box: the message, now under the unexpected-error line.
        assert_eq!(
            brief_text_in(&error, en).as_str(),
            "Something went wrong.\nHotkey already registered"
        );
        // Board boxes and the unexpected-error box: message and code.
        assert_eq!(
            with(Key::ErrorBoardUnsaved, &detail_in(&error, en)).as_str(),
            "The board could not be saved: Hotkey already registered (0x80004005)"
        );
        assert_eq!(
            with(Key::ErrorWallpaperUnchanged, &detail_in(&error, en)).as_str(),
            "The notes are saved, but the wallpaper was not updated: \
             Hotkey already registered (0x80004005)"
        );
        assert_eq!(
            text_in(&error, en).as_str(),
            "Something went wrong.\nHotkey already registered (0x80004005)"
        );
        // A visible failure reads the same whichever way it is asked for.
        let shown = visible(Key::ErrorPresentationBusy, &[]);
        for language in Language::ALL {
            let expected = Text::new_in(language, Key::ErrorPresentationBusy);
            assert_eq!(brief_text_in(&shown, language), expected);
            assert_eq!(reason_in(&shown, language), expected.to_string());
        }
    }
}
