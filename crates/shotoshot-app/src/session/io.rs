use super::*;
use crate::settings::SaveFormat;
use std::path::{Path, PathBuf};
use windows::Win32::Foundation::ERROR_CANCELLED;
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint,
};
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance, CoTaskMemFree};
use windows::Win32::System::Ole::IOleWindow;
use windows::Win32::UI::Shell::Common::COMDLG_FILTERSPEC;
use windows::Win32::UI::Shell::{
    FDE_OVERWRITE_RESPONSE, FDE_SHAREVIOLATION_RESPONSE, FDEOR_DEFAULT, FDESVR_DEFAULT,
    FOS_FORCEFILESYSTEM, FOS_NOCHANGEDIR, FOS_OVERWRITEPROMPT, FOS_PATHMUSTEXIST, FileSaveDialog,
    IFileDialog, IFileDialogEvents, IFileDialogEvents_Impl, IFileSaveDialog, IShellItem,
    SHCreateItemFromParsingName, SIGDN_FILESYSPATH,
};
use windows::core::{HSTRING, Interface};

/// The type list of the Save dialog in `language`: one entry per `SaveFormat::ALL`, in that
/// order, then the editable document, whose label says what such a file keeps at the moment it
/// is chosen.
fn save_types_in(language: shotoshot_text::Language) -> [(String, &'static str); 3] {
    use shotoshot_text::{Key, tr_in};
    [
        (Key::SavePng, "*.png"),
        (Key::SaveJpeg, "*.jpg;*.jpeg"),
        (Key::SaveShoto, "*.shoto"),
    ]
    .map(|(label, pattern)| (tr_in(language, label).to_string(), pattern))
}

fn listed_format(filter_index: u32) -> SaveFormat {
    SaveFormat::ALL
        .get((filter_index as usize).wrapping_sub(1))
        .copied()
        .unwrap_or_default()
}
/// One entry of the type list: an image format, or the editable document.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Listed {
    Image(SaveFormat),
    Document,
}
impl Listed {
    /// The document comes after every image format.
    fn entry(filter_index: u32) -> Self {
        if filter_index as usize == SaveFormat::ALL.len() + 1 {
            Self::Document
        } else {
            Self::Image(listed_format(filter_index))
        }
    }
    /// What the extension of `path` names, when it is one Save writes.
    fn of(path: &Path) -> Option<Self> {
        if crate::capture_name::is_document(path) {
            Some(Self::Document)
        } else {
            SaveFormat::of(path).map(Self::Image)
        }
    }
    /// Extension given to a new file.
    fn extension(self) -> &'static str {
        match self {
            Self::Image(format) => format.extension(),
            Self::Document => crate::capture_name::DOCUMENT,
        }
    }
    /// The next numbered name in `folder`: every entry shares the sequence.
    fn next_path(self, folder: &Path) -> std::io::Result<PathBuf> {
        match self {
            Self::Image(format) => crate::capture_name::next_path(folder, format),
            Self::Document => Ok(crate::capture_name::next_path(folder, SaveFormat::Png)?
                .with_extension(crate::capture_name::DOCUMENT)),
        }
    }
}
/// The file is what its name says; any other name is written as listed.
fn chosen(path: &Path, filter_index: u32) -> Listed {
    Listed::of(path).unwrap_or_else(|| Listed::entry(filter_index))
}
/// The name the dialog should propose once the folder or the listed type changes, or `None`
/// to leave the name as it stands. A name the user typed is kept, and only an extension Save
/// writes follows the type list; a numbered or empty name is renumbered for `folder`.
fn proposed_name(name: &str, folder: &Path, listed: Listed) -> Option<PathBuf> {
    let typed = Path::new(name);
    if !name.is_empty() && crate::capture_name::number(typed).is_none() && !typed.is_dir() {
        return match Listed::of(typed) {
            Some(current) if current != listed => Some(typed.with_extension(listed.extension())),
            _ => None,
        };
    }
    let path = listed.next_path(folder).ok()?;
    Some(PathBuf::from(path.file_name()?))
}

/// Text the shell allocated, read then freed.
unsafe fn taken(text: PWSTR) -> std::ffi::OsString {
    use std::os::windows::ffi::OsStringExt;
    unsafe {
        let value = std::ffi::OsString::from_wide(text.as_wide());
        CoTaskMemFree(Some(text.0 as *const _));
        value
    }
}

/// Keeps the proposed name in step with the folder and the type list, and opens the dialog on
/// the screen of the capture: owned by the capture window, which spans every screen, Windows
/// centred it on the primary one (blind test of 2026-11-05).
#[windows_core::implement(IFileDialogEvents)]
struct SaveDialogEvents {
    /// A point of the screen the dialog opens on.
    near: POINT,
    placed: std::cell::Cell<bool>,
}

impl SaveDialogEvents {
    fn follow(&self, dialog: &IFileDialog) -> windows::core::Result<()> {
        unsafe {
            let listed = Listed::entry(dialog.GetFileTypeIndex()?);
            dialog.SetDefaultExtension(&HSTRING::from(listed.extension()))?;
            let folder = PathBuf::from(taken(
                dialog.GetFolder()?.GetDisplayName(SIGDN_FILESYSPATH)?,
            ));
            let name = taken(dialog.GetFileName()?).to_string_lossy().into_owned();
            if let Some(name) = proposed_name(&name, &folder, listed) {
                dialog.SetFileName(&HSTRING::from(name.as_os_str()))?;
            }
            Ok(())
        }
    }
    fn place(&self, dialog: &IFileDialog) {
        if self.placed.replace(true) {
            return;
        }
        unsafe {
            let Ok(window) = dialog.cast::<IOleWindow>().and_then(|w| w.GetWindow()) else {
                return;
            };
            let monitor = MonitorFromPoint(self.near, MONITOR_DEFAULTTONEAREST);
            let mut info = MONITORINFO {
                cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                ..Default::default()
            };
            let mut frame = RECT::default();
            if !GetMonitorInfoW(monitor, &mut info).as_bool()
                || GetWindowRect(window, &mut frame).is_err()
            {
                return;
            }
            let work = info.rcWork;
            let (width, height) = (frame.right - frame.left, frame.bottom - frame.top);
            let x = work.left + ((work.right - work.left - width) / 2).max(0);
            let y = work.top + ((work.bottom - work.top - height) / 2).max(0);
            let _ = SetWindowPos(
                window,
                None,
                x,
                y,
                0,
                0,
                SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
            );
        }
    }
}

impl IFileDialogEvents_Impl for SaveDialogEvents_Impl {
    fn OnFileOk(&self, _: windows::core::Ref<IFileDialog>) -> windows::core::Result<()> {
        Ok(())
    }
    fn OnFolderChanging(
        &self,
        _: windows::core::Ref<IFileDialog>,
        _: windows::core::Ref<IShellItem>,
    ) -> windows::core::Result<()> {
        Ok(())
    }
    fn OnFolderChange(&self, dialog: windows::core::Ref<IFileDialog>) -> windows::core::Result<()> {
        let dialog = dialog.ok()?;
        self.place(dialog);
        // A failure only leaves the name as it was.
        let _ = self.follow(dialog);
        Ok(())
    }
    fn OnSelectionChange(&self, _: windows::core::Ref<IFileDialog>) -> windows::core::Result<()> {
        Ok(())
    }
    fn OnShareViolation(
        &self,
        _: windows::core::Ref<IFileDialog>,
        _: windows::core::Ref<IShellItem>,
    ) -> windows::core::Result<FDE_SHAREVIOLATION_RESPONSE> {
        Ok(FDESVR_DEFAULT)
    }
    fn OnTypeChange(&self, dialog: windows::core::Ref<IFileDialog>) -> windows::core::Result<()> {
        let _ = self.follow(dialog.ok()?);
        Ok(())
    }
    fn OnOverwrite(
        &self,
        _: windows::core::Ref<IFileDialog>,
        _: windows::core::Ref<IShellItem>,
    ) -> windows::core::Result<FDE_OVERWRITE_RESPONSE> {
        Ok(FDEOR_DEFAULT)
    }
}

/// The Windows Save dialog, modern style, on the screen holding `near`: proposes `name` in
/// `folder` with `format` listed. Returns the path chosen and the one-based type list entry,
/// or `None` when cancelled.
fn ask_save_path(
    owner: Option<HWND>,
    folder: &Path,
    name: &str,
    format: SaveFormat,
    near: POINT,
    language: shotoshot_text::Language,
) -> windows::core::Result<Option<(PathBuf, u32)>> {
    unsafe {
        let dialog: IFileSaveDialog =
            CoCreateInstance(&FileSaveDialog, None, CLSCTX_INPROC_SERVER)?;
        let entries = save_types_in(language)
            .map(|(label, pattern)| (HSTRING::from(label), HSTRING::from(pattern)));
        let specs: Vec<COMDLG_FILTERSPEC> = entries
            .iter()
            .map(|(label, pattern)| COMDLG_FILTERSPEC {
                pszName: PCWSTR(label.as_ptr()),
                pszSpec: PCWSTR(pattern.as_ptr()),
            })
            .collect();
        dialog.SetFileTypes(&specs)?;
        dialog.SetFileTypeIndex(format.index() as u32 + 1)?;
        dialog.SetDefaultExtension(&HSTRING::from(format.extension()))?;
        let start: IShellItem =
            SHCreateItemFromParsingName(&HSTRING::from(folder.as_os_str()), None)?;
        dialog.SetFolder(&start)?;
        dialog.SetFileName(&HSTRING::from(name))?;
        let options = dialog.GetOptions()?;
        dialog.SetOptions(
            options
                | FOS_OVERWRITEPROMPT
                | FOS_PATHMUSTEXIST
                | FOS_NOCHANGEDIR
                | FOS_FORCEFILESYSTEM,
        )?;
        let events: IFileDialogEvents = SaveDialogEvents {
            near,
            placed: std::cell::Cell::new(false),
        }
        .into();
        let cookie = dialog.Advise(&events)?;
        let shown = dialog.Show(owner);
        let _ = dialog.Unadvise(cookie);
        match shown {
            Ok(()) => {}
            Err(e) if e.code() == HRESULT::from_win32(ERROR_CANCELLED.0) => return Ok(None),
            Err(e) => return Err(e),
        }
        let path = PathBuf::from(taken(
            dialog.GetResult()?.GetDisplayName(SIGDN_FILESYSPATH)?,
        ));
        Ok(Some((path, dialog.GetFileTypeIndex()?)))
    }
}

impl Native {
    pub fn begin_text(&mut self, capture: &mut Capture, hwnd: HWND, p: Point) -> Result<()> {
        unsafe {
            let existing = capture.text_at(p);
            let (position, value, id, size, rgb, rotation, label, ink, kept) =
                if let Some(Object {
                    id,
                    shape:
                        Shape::Text {
                            position,
                            text,
                            size,
                            label,
                            ink,
                            extent,
                        },
                    style,
                    rotation,
                    ..
                }) = existing
                {
                    (
                        *position,
                        text.clone(),
                        Some(*id),
                        *size as f32,
                        style.color,
                        *rotation,
                        *label,
                        *ink,
                        extent.y,
                    )
                } else {
                    (
                        p,
                        String::new(),
                        None,
                        capture.text_size,
                        capture.editor.style.color,
                        0.,
                        capture.new_label,
                        capture.label_ink,
                        0.,
                    )
                };
            let width = (capture.image.width as f64 - position.x).max(1.) as i32;
            let height = (capture.image.height as f64 - position.y).max(1.) as i32;
            let input = CreateWindowExW(
                WS_EX_TRANSPARENT,
                w!("EDIT"),
                &HSTRING::from(value),
                WS_CHILD
                    | WS_VISIBLE
                    | WINDOW_STYLE(
                        ES_MULTILINE as u32
                            | ES_AUTOVSCROLL as u32
                            | ES_AUTOHSCROLL as u32
                            | ES_WANTRETURN as u32,
                    ),
                position.x as i32,
                position.y as i32,
                width,
                height,
                Some(hwnd),
                None,
                None,
                None,
            )?;
            let font = match super::text::configure_input(input, size, Face::Annotation) {
                Ok(font) => font,
                Err(error) => {
                    let _ = DestroyWindow(input);
                    return Err(error);
                }
            };
            self.text = Some(TextInput {
                rotation,
                window: input,
                position,
                object: id,
                size,
                face: Face::Annotation,
                color: rgb,
                label,
                ink,
                height: kept,
                font,
                drag_anchor: None,
                scroll: Default::default(),
            });
            let _ = SetFocus(Some(input));
            if id.is_some() {
                self.text_pointer_down(hwnd, p)?;
            }
            self.text_event_updated(hwnd);
            Ok(())
        }
    }
    pub fn finish_text(&mut self, capture: &mut Capture, hwnd: HWND, commit: bool) {
        if let Some(input) = self.text.take() {
            unsafe {
                if commit {
                    let length = GetWindowTextLengthW(input.window).max(0) as usize;
                    let mut buffer = vec![0u16; length + 1];
                    let copied = GetWindowTextW(input.window, &mut buffer) as usize;
                    let text = String::from_utf16_lossy(&buffer[..copied]);
                    let wrap = input.label.and_then(Label::wrap).map(|w| w as f32);
                    let measured = match wrap {
                        Some(width) => self
                            .renderer
                            .measure_wrapped_annotation_text(&text, input.size, width),
                        None => self.renderer.measure_annotation_text(&text, input.size),
                    };
                    let extent = match measured {
                        Ok(extent) => {
                            kept_block(&text, input.label, extent, input.size as f64, input.height)
                        }
                        Err(error) => {
                            // The title says what failed, the body why, as Windows put it.
                            let title =
                                shotoshot_text::Text::new(shotoshot_text::Key::ErrorTextLayout);
                            MessageBoxW(
                                Some(hwnd),
                                &HSTRING::from(error.to_string()),
                                &HSTRING::from(title.as_str()),
                                MB_OK | MB_ICONERROR,
                            );
                            self.text = Some(input);
                            return;
                        }
                    };
                    if let Some(id) = input.object {
                        capture.editor.update_text_inked(
                            id,
                            text,
                            extent,
                            input.color,
                            input.size as f64,
                            input.ink,
                        );
                        capture.select_committed_text(id);
                    } else if let Some(id) = capture.editor.add_label(
                        input.position,
                        text,
                        extent,
                        input.color,
                        input.size as f64,
                        input.label,
                        input.ink,
                    ) {
                        capture.select_committed_text(id);
                    }
                }
                if input.drag_anchor.is_some() {
                    let _ = ReleaseCapture();
                }
                drop(input);
                let _ = SetFocus(Some(hwnd));
            }
        }
    }
}
/// How a save ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Saved {
    Cancelled,
    /// A flattened image.
    Image,
    /// An editable `.shoto` document.
    Document,
}

impl Session {
    /// The `.shoto` file of this capture: its flattened image as any viewer shows it, and
    /// inside it the screens of `background` under the region and the objects, still apart.
    pub(crate) fn document_bytes(&self, background: &Image) -> Result<Vec<u8>> {
        self.document_bytes_from(background, &self.flatten(background)?)
    }
    /// `document_bytes` with the capture already `flattened` over `background`.
    fn document_bytes_from(&self, background: &Image, flattened: &Image) -> Result<Vec<u8>> {
        let refused = || crate::failure::visible(shotoshot_text::Key::ErrorNotEditableCapture, &[]);
        let (scene, frame) = self.capture.document(background).ok_or_else(refused)?;
        let visible = png_data(flattened)?;
        shoto::pack(&visible, &scene, &png_data(&frame)?).ok_or_else(refused)
    }
    /// Saves the capture. A reopened document is rewritten in place unless `choose` asks
    /// where to save; a capture always asks, listing the image format last used.
    pub fn save(&self, hwnd: HWND, settings: &mut Settings, choose: bool) -> Result<Saved> {
        // Snapshot before the modal dialog, at the Save action.
        let grabbed = if self.capture.live {
            Some(self.fresh_desktop()?)
        } else {
            None
        };
        let background = grabbed.as_ref().unwrap_or(&self.capture.image);
        // Flattened once: the image format gets it as is, the `.shoto` as its visible picture.
        let output = self.flatten(background)?;
        if let Some(path) = self.document.as_ref().filter(|_| !choose) {
            save_bytes(&self.document_bytes_from(background, &output)?, path, false)?;
            return Ok(Saved::Document);
        }
        let folder = settings.save_folder()?;
        let suggested = crate::capture_name::next_path(&folder, settings.save_format)
            .map_err(|e| Error::new(E_FAIL, e.to_string()))?;
        let filename = suggested.file_name().unwrap().to_string_lossy();
        // The screen the user works on: the toolbar's Save or the keys, the pointer is there.
        let mut near = POINT::default();
        unsafe {
            let _ = GetCursorPos(&mut near);
        }
        let asked = ask_save_path(
            Some(hwnd),
            &folder,
            &filename,
            settings.save_format,
            near,
            shotoshot_text::language(),
        )
        .map_err(|e| {
            crate::failure::visible(
                shotoshot_text::Key::ErrorSaveDialog,
                &[("code", &format!("{:#010x}", e.code().0))],
            )
        })?;
        let Some((path, listed)) = asked else {
            return Ok(Saved::Cancelled);
        };
        {
            let numbered = crate::capture_name::number(&path).is_some();
            let saved = match chosen(&path, listed) {
                Listed::Document => {
                    save_bytes(
                        &self.document_bytes_from(background, &output)?,
                        &path,
                        numbered,
                    )?;
                    Saved::Document
                }
                Listed::Image(format) => {
                    if numbered {
                        save_numbered(&output, &path, format)?;
                    } else {
                        save_image(&output, &path, format)?;
                    }
                    // Only an image format is remembered: the editable type, which keeps
                    // the whole screen, is chosen on purpose each time.
                    settings.save_format = format;
                    Saved::Image
                }
            };
            if let Some(parent) = path.parent() {
                settings.folder = parent.into();
            }
            Ok(saved)
        }
    }
}

#[cfg(test)]
mod save_dialog_tests {
    use super::*;
    #[test]
    fn the_proposed_name_follows_the_folder_and_the_type_list() {
        let root =
            std::env::temp_dir().join(format!("shotoshot-save-names-{}", std::process::id()));
        let old = root.join("old");
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(old.join("Capture 008.png"), b"keep").unwrap();
        std::fs::write(old.join("Shot 011.jpg"), b"keep").unwrap();
        let png = Listed::Image(SaveFormat::Png);
        let jpeg = Listed::Image(SaveFormat::Jpeg);
        let named = |name: &str, listed| {
            proposed_name(name, &old, listed).map(|p| p.to_string_lossy().into_owned())
        };
        // Renumbered for the folder reached, where a JPEG holds the largest number.
        assert_eq!(named("002.png", png).as_deref(), Some("012.png"));
        assert_eq!(named("", png).as_deref(), Some("012.png"));
        // The proposed name follows the type list, to the editable document too.
        assert_eq!(
            named("012.png", Listed::Document).as_deref(),
            Some("012.shoto")
        );
        assert_eq!(named("012.shoto", jpeg).as_deref(), Some("012.jpg"));
        // A typed name is kept; only an extension Save writes follows.
        assert_eq!(
            named("Invoice May.jpg", png).as_deref(),
            Some("Invoice May.png")
        );
        assert_eq!(named("Invoice May.png", png), None);
        assert_eq!(named("Invoice May", jpeg), None);
        assert_eq!(named("notes.txt", jpeg), None);
        std::fs::remove_dir_all(root).unwrap();
    }
    // The real dialog takes the foreground, which the tests of windows that must not
    // activate are watching: it runs apart.
    #[test]
    #[ignore = "real desktop: opens the Windows Save dialog, run alone by check.ps1"]
    fn real_desktop_save_dialog_opens_on_the_screen_of_the_capture() {
        let _com = crate::ComApartment::new().unwrap();
        let screen = crate::image::probe_screen();
        let near = POINT {
            x: (screen.left + screen.right) / 2,
            y: (screen.top + screen.bottom) / 2,
        };
        let thread = unsafe { windows::Win32::System::Threading::GetCurrentThreadId() };
        // Reads where the dialog stands, then cancels it.
        let watcher = std::thread::spawn(move || -> Option<RECT> {
            unsafe extern "system" fn dialog_of(window: HWND, found: LPARAM) -> BOOL {
                unsafe {
                    let mut class = [0u16; 16];
                    let length = GetClassNameW(window, &mut class) as usize;
                    if IsWindowVisible(window).as_bool()
                        && class[..length] == *w!("#32770").as_wide()
                    {
                        *(found.0 as *mut HWND) = window;
                        return false.into();
                    }
                    true.into()
                }
            }
            for _ in 0..100 {
                std::thread::sleep(std::time::Duration::from_millis(100));
                let mut found = HWND::default();
                unsafe {
                    let _ = EnumThreadWindows(
                        thread,
                        Some(dialog_of),
                        LPARAM(&mut found as *mut HWND as isize),
                    );
                    if !found.is_invalid() {
                        // Let the dialog settle where it opens before reading it.
                        std::thread::sleep(std::time::Duration::from_millis(300));
                        let mut frame = RECT::default();
                        GetWindowRect(found, &mut frame).ok()?;
                        let _ = PostMessageW(Some(found), WM_COMMAND, WPARAM(2), LPARAM(0));
                        return Some(frame);
                    }
                }
            }
            None
        });
        let folder = std::env::temp_dir();
        let asked = ask_save_path(
            None,
            &folder,
            "001.png",
            SaveFormat::Png,
            near,
            shotoshot_text::Language::En,
        )
        .unwrap();
        let frame = watcher.join().unwrap().expect("the Save dialog showed");
        assert_eq!(asked, None, "cancelled");
        let centre = POINT {
            x: (frame.left + frame.right) / 2,
            y: (frame.top + frame.bottom) / 2,
        };
        assert!(
            centre.x >= screen.left
                && centre.x < screen.right
                && centre.y >= screen.top
                && centre.y < screen.bottom,
            "{frame:?} is not on {screen:?}"
        );
    }
    #[test]
    fn the_name_decides_the_format_and_the_type_list_settles_the_rest() {
        let named = |name: &str, listed| chosen(Path::new(name), listed);
        assert_eq!(named("Shot 001.png", 2), Listed::Image(SaveFormat::Png));
        assert_eq!(named("photo.JPEG", 1), Listed::Image(SaveFormat::Jpeg));
        assert_eq!(named("notes.txt", 2), Listed::Image(SaveFormat::Jpeg));
        assert_eq!(named("bare", 1), Listed::Image(SaveFormat::Png));
        // The editable document: by its name whatever is listed, or as the last entry.
        assert_eq!(named("Shot 001.SHOTO", 1), Listed::Document);
        assert_eq!(named("bare", 3), Listed::Document);
        assert_eq!(named("Shot 001.png", 3), Listed::Image(SaveFormat::Png));
    }
    #[test]
    fn save_types_are_translated() {
        use shotoshot_text::{Key, Language, tr_in};
        for language in Language::ALL {
            let labels =
                [Key::SavePng, Key::SaveJpeg, Key::SaveShoto].map(|key| tr_in(language, key));
            let types = save_types_in(language);
            for ((label, _), expected) in types.iter().zip(labels) {
                assert_eq!(label, expected);
            }
        }
        assert_eq!(
            save_types_in(Language::En),
            [
                ("PNG image".to_string(), "*.png"),
                ("JPEG image".to_string(), "*.jpg;*.jpeg"),
                (
                    "Editable Shotoshot image, keeps the whole screen".to_string(),
                    "*.shoto"
                ),
            ]
        );
    }
    #[test]
    fn type_list_entries_follow_the_save_formats() {
        let types = save_types_in(shotoshot_text::Language::En);
        // The editable document closes the list.
        assert_eq!(types.len(), SaveFormat::ALL.len() + 1);
        assert_eq!(types.last().unwrap().1, "*.shoto");
        let document = SaveFormat::ALL.len() as u32 + 1;
        assert_eq!(Listed::entry(document), Listed::Document);
        assert_eq!(Listed::Document.extension(), "shoto");
        for (format, (_, pattern)) in SaveFormat::ALL.into_iter().zip(types) {
            assert!(
                pattern.starts_with(&format!("*.{}", format.extension())),
                "{pattern} is not {format:?}"
            );
            assert_eq!(listed_format(format.index() as u32 + 1), format);
        }
        assert_eq!(listed_format(0), SaveFormat::Png);
    }
}
