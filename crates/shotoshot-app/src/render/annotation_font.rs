//! ROLE: The embedded typeface of text annotations, handed to DirectWrite and to GDI.
//! OWNER: Each thread builds its private font collection on first use; nothing exists
//! while resident or before the first annotation text.
//! INVARIANTS: The bytes are `shotoshot_ui::annotation_font::DATA`. Every DirectWrite
//! factory of the app is the shared one, so a collection serves any of them. DirectWrite
//! and the hidden EDIT control lay annotation text out with this one face.
use shotoshot_ui::annotation_font::{DATA, FAMILY, WEIGHT};
use std::{cell::OnceCell, sync::OnceLock};
use windows::{
    Win32::{
        Foundation::E_FAIL,
        Graphics::{DirectWrite::*, Gdi::*},
    },
    core::*,
};

/// GDI knows the face by its legacy family name, not by the typographic `FAMILY`.
const GDI_FACE: PCWSTR = w!("Roboto Medium");

thread_local! {
    static COLLECTION: OnceCell<IDWriteFontCollection> = const { OnceCell::new() };
}

fn collection(write: &IDWriteFactory) -> Result<IDWriteFontCollection> {
    COLLECTION.with(|cell| {
        if let Some(collection) = cell.get() {
            return Ok(collection.clone());
        }
        let collection = load(write)?;
        Ok(cell.get_or_init(|| collection).clone())
    })
}

fn load(write: &IDWriteFactory) -> Result<IDWriteFontCollection> {
    unsafe {
        let factory: IDWriteFactory5 = write.cast()?;
        let loader = factory.CreateInMemoryFontFileLoader()?;
        factory.RegisterFontFileLoader(&loader)?;
        // The bytes are static: given an owner, the loader keeps our pointer instead of
        // copying the file.
        let owner: IUnknown = factory.cast()?;
        let file = loader.CreateInMemoryFontFileReference(
            write,
            DATA.as_ptr().cast(),
            DATA.len() as u32,
            &owner,
        )?;
        let builder = factory.CreateFontSetBuilder()?;
        builder.AddFontFile(&file)?;
        factory
            .CreateFontCollectionFromFontSet(&builder.CreateFontSet()?)?
            .cast()
    }
}

/// The text format of annotations at `size`.
pub fn format(write: &IDWriteFactory, size: f32) -> Result<IDWriteTextFormat> {
    format_in(write, size, shotoshot_text::language())
}

/// The text format of annotations at `size`, shaped for `language`.
pub fn format_in(
    write: &IDWriteFactory,
    size: f32,
    language: shotoshot_text::Language,
) -> Result<IDWriteTextFormat> {
    unsafe {
        write.CreateTextFormat(
            &HSTRING::from(FAMILY),
            &collection(write)?,
            DWRITE_FONT_WEIGHT(WEIGHT as i32),
            DWRITE_FONT_STYLE_NORMAL,
            DWRITE_FONT_STRETCH_NORMAL,
            size,
            &HSTRING::from(language.locale_name()),
        )
    }
}

/// A GDI font of the same face, owned by the caller: the hidden EDIT control moves its
/// caret with its own layout, which must agree with DirectWrite.
pub fn gdi_font(size: f32) -> Result<HFONT> {
    static REGISTERED: OnceLock<bool> = OnceLock::new();
    // Private to the process: no other application can enumerate or use the face.
    let registered = *REGISTERED.get_or_init(|| unsafe {
        let mut faces = 0u32;
        let font = AddFontMemResourceEx(
            DATA.as_ptr().cast(),
            DATA.len() as u32,
            None,
            &raw mut faces,
        );
        !font.is_invalid() && faces > 0
    });
    if !registered {
        return Err(Error::new(E_FAIL, "Annotation font not registered"));
    }
    let font = unsafe {
        CreateFontW(
            -(size.round() as i32),
            0,
            0,
            0,
            WEIGHT as i32,
            0,
            0,
            0,
            DEFAULT_CHARSET,
            OUT_DEFAULT_PRECIS,
            CLIP_DEFAULT_PRECIS,
            CLEARTYPE_QUALITY,
            0,
            GDI_FACE,
        )
    };
    if font.0.is_null() {
        return Err(Error::from_thread());
    }
    Ok(font)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_collection_resolves_the_annotation_family_at_its_weight() -> Result<()> {
        unsafe {
            let write: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
            let fonts = collection(&write)?;
            let (mut index, mut exists) = (0, BOOL(0));
            fonts.FindFamilyName(&HSTRING::from(FAMILY), &mut index, &mut exists)?;
            assert!(exists.as_bool());
            let font = fonts.GetFontFamily(index)?.GetFirstMatchingFont(
                DWRITE_FONT_WEIGHT(WEIGHT as i32),
                DWRITE_FONT_STRETCH_NORMAL,
                DWRITE_FONT_STYLE_NORMAL,
            )?;
            // The exact weight: DirectWrite would otherwise embolden a lighter face.
            assert_eq!(font.GetWeight().0, WEIGHT as i32);
            assert_eq!(font.GetSimulations(), DWRITE_FONT_SIMULATIONS_NONE);
            Ok(())
        }
    }

    #[test]
    fn hidden_edit_font_resolves_to_the_embedded_face() -> Result<()> {
        unsafe {
            let font = gdi_font(24.)?;
            let dc = CreateCompatibleDC(None);
            let previous = SelectObject(dc, font.into());
            let mut name = [0u16; 64];
            let length = GetTextFaceW(dc, Some(&mut name)) as usize;
            SelectObject(dc, previous);
            let _ = DeleteDC(dc);
            let _ = DeleteObject(font.into());
            let name = String::from_utf16_lossy(&name[..length]);
            assert_eq!(name.trim_end_matches('\0'), "Roboto Medium");
            Ok(())
        }
    }
}
