//! ROLE: Tests that the DirectWrite text locale follows the interface language and that
//! every translated text has an installed font. They measure text only; no screen is read.
use super::*;
use shotoshot_text::{Key, Language, Text};
use windows::Win32::Graphics::DirectWrite::{IDWriteNumberSubstitution, IDWriteTextAnalysisSource};
use windows::core::implement;

// DirectWrite reports locale names in lower case.
#[test]
fn interface_format_uses_the_current_locale() {
    let _com = crate::ComApartment::new().unwrap();
    let renderer = Renderer::software().unwrap();
    for &language in &Language::ALL {
        let format = renderer.interface_format_in(14., language).unwrap();
        let length = unsafe { format.GetLocaleNameLength() } as usize;
        let mut name = vec![0u16; length + 1];
        unsafe { format.GetLocaleName(&mut name) }.unwrap();
        assert_eq!(
            String::from_utf16_lossy(&name[..length]).to_lowercase(),
            language.locale_name().to_lowercase(),
            "{}",
            language.tag()
        );
    }
    // English keeps the locale it always had, so its rendering does not change.
    let english = renderer.interface_format_in(14., Language::En).unwrap();
    let mut name = [0u16; 16];
    unsafe { english.GetLocaleName(&mut name) }.unwrap();
    assert_eq!(String::from_utf16_lossy(&name[..5]), "en-us");
}

#[test]
fn annotation_format_follows_the_language_too() {
    let _com = crate::ComApartment::new().unwrap();
    let renderer = Renderer::software().unwrap();
    let format = annotation_font::format_in(&renderer.write, 14., Language::Ja).unwrap();
    let length = unsafe { format.GetLocaleNameLength() } as usize;
    let mut name = vec![0u16; length + 1];
    unsafe { format.GetLocaleName(&mut name) }.unwrap();
    assert_eq!(String::from_utf16_lossy(&name[..length]), "ja-jp");
}

/// One run of text in one locale, as DirectWrite's font fallback reads it.
#[implement(IDWriteTextAnalysisSource)]
struct Run {
    text: Vec<u16>,
    /// NUL-terminated.
    locale: Vec<u16>,
}

#[allow(non_snake_case)]
impl IDWriteTextAnalysisSource_Impl for Run_Impl {
    fn GetTextAtPosition(
        &self,
        position: u32,
        text: *mut *mut u16,
        length: *mut u32,
    ) -> Result<()> {
        let position = (position as usize).min(self.text.len());
        unsafe {
            *text = self.text.as_ptr().add(position).cast_mut();
            *length = (self.text.len() - position) as u32;
        }
        Ok(())
    }
    fn GetTextBeforePosition(
        &self,
        position: u32,
        text: *mut *mut u16,
        length: *mut u32,
    ) -> Result<()> {
        let position = (position as usize).min(self.text.len());
        unsafe {
            *text = self.text.as_ptr().cast_mut();
            *length = position as u32;
        }
        Ok(())
    }
    fn GetParagraphReadingDirection(&self) -> DWRITE_READING_DIRECTION {
        DWRITE_READING_DIRECTION_LEFT_TO_RIGHT
    }
    fn GetLocaleName(&self, position: u32, length: *mut u32, locale: *mut *mut u16) -> Result<()> {
        unsafe {
            *locale = self.locale.as_ptr().cast_mut();
            *length = (self.text.len() as u32).saturating_sub(position);
        }
        Ok(())
    }
    fn GetNumberSubstitution(
        &self,
        position: u32,
        length: *mut u32,
        substitution: OutRef<IDWriteNumberSubstitution>,
    ) -> Result<()> {
        unsafe { *length = (self.text.len() as u32).saturating_sub(position) };
        substitution.write(None)
    }
}

/// The first character of `text` that Segoe UI and the system font fallback cannot draw.
fn first_uncovered(
    fallback: &IDWriteFontFallback,
    text: &str,
    language: Language,
) -> Result<Option<char>> {
    let units: Vec<u16> = text.encode_utf16().collect();
    let source: IDWriteTextAnalysisSource = Run {
        text: units.clone(),
        locale: language.locale_name().encode_utf16().chain([0]).collect(),
    }
    .into();
    let mut position = 0u32;
    while (position as usize) < units.len() {
        let (mut mapped, mut font, mut scale) = (0u32, None, 0f32);
        unsafe {
            fallback.MapCharacters(
                &source,
                position,
                units.len() as u32 - position,
                None,
                w!("Segoe UI"),
                DWRITE_FONT_WEIGHT_NORMAL,
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                &mut mapped,
                &mut font,
                &mut scale,
            )?;
        }
        // No font means the run is drawn as missing-glyph boxes.
        assert!(mapped > 0, "font fallback made no progress");
        if font.is_none() {
            let rest = String::from_utf16_lossy(&units[position as usize..]);
            return Ok(rest.chars().next());
        }
        position += mapped;
    }
    Ok(None)
}

#[test]
fn every_text_maps_to_an_installed_font() {
    let _com = crate::ComApartment::new().unwrap();
    let renderer = Renderer::software().unwrap();
    let factory: IDWriteFactory2 = renderer.write.cast().unwrap();
    let fallback = unsafe { factory.GetSystemFontFallback() }.unwrap();
    for &language in &Language::ALL {
        for &key in Key::ALL {
            let values: Vec<(&str, &str)> = key
                .placeholders()
                .iter()
                .map(|&name| (name, "12"))
                .collect();
            // A line break starts a line; it is not drawn.
            let text = Text::with_in(language, key, &values)
                .to_string()
                .replace('\n', " ");
            let missing = first_uncovered(&fallback, &text, language).unwrap();
            assert_eq!(
                missing,
                None,
                "{} / {}: no installed font draws {:?} in `{}`",
                language.tag(),
                key.id(),
                missing,
                text
            );
        }
    }
}

#[test]
fn font_fallback_check_detects_an_undrawable_character() {
    let _com = crate::ComApartment::new().unwrap();
    let renderer = Renderer::software().unwrap();
    let factory: IDWriteFactory2 = renderer.write.cast().unwrap();
    let fallback = unsafe { factory.GetSystemFontFallback() }.unwrap();
    // Plane-15 private use: no installed font draws it.
    let missing = first_uncovered(&fallback, "ok \u{F0000} ok", Language::En).unwrap();
    assert_eq!(missing, Some('\u{F0000}'));
    assert_eq!(
        first_uncovered(&fallback, "Language", Language::En).unwrap(),
        None
    );
    assert_eq!(
        first_uncovered(&fallback, "言語 언어 Язык", Language::Ja).unwrap(),
        None
    );
}
