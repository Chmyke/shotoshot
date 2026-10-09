use super::*;
use crate::settings::SaveFormat;
use std::path::{Path, PathBuf};

/// Posted to the hook with the one-based entry of the type list as WPARAM.
const UPDATE_CAPTURE_NAME: u32 = WM_APP + 83;
/// The type list of the Save dialog: one entry per `SaveFormat::ALL`, in that order, then the
/// editable document, whose label says what such a file keeps at the moment it is chosen.
const SAVE_FILTER: PCWSTR = w!(
    "PNG image\0*.png\0JPEG image\0*.jpg;*.jpeg\0Editable Shotoshot image, keeps the whole screen\0*.shoto\0\0"
);

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
/// The path the dialog left in its null-terminated buffer.
fn chosen_path(buffer: &[u16]) -> PathBuf {
    use std::os::windows::ffi::OsStringExt;
    let length = buffer.iter().position(|v| *v == 0).unwrap_or(buffer.len());
    PathBuf::from(std::ffi::OsString::from_wide(&buffer[..length]))
}

// Defer until Explorer has finished updating its filename field.
unsafe extern "system" fn save_dialog_hook(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> usize {
    unsafe {
        if message == WM_NOTIFY && lparam.0 != 0 {
            let notification = &*(lparam.0 as *const OFNOTIFYW);
            if matches!(notification.hdr.code, CDN_FOLDERCHANGE | CDN_TYPECHANGE)
                && !notification.lpOFN.is_null()
            {
                let listed = (*notification.lpOFN).nFilterIndex as usize;
                let _ = PostMessageW(Some(hwnd), UPDATE_CAPTURE_NAME, WPARAM(listed), LPARAM(0));
            }
        }
        if message != UPDATE_CAPTURE_NAME {
            return 0;
        }
        let listed = Listed::entry(wparam.0 as u32);
        let Ok(dialog) = GetParent(hwnd) else {
            return 0;
        };
        // A name typed without an extension takes the listed entry's.
        let extension = crate::wide(Path::new(listed.extension()));
        SendMessageW(
            dialog,
            CDM_SETDEFEXT,
            Some(WPARAM(0)),
            Some(LPARAM(extension.as_ptr() as isize)),
        );
        let mut folder = vec![0u16; 32768];
        let length = SendMessageW(
            dialog,
            CDM_GETFOLDERPATH,
            Some(WPARAM(folder.len())),
            Some(LPARAM(folder.as_mut_ptr() as isize)),
        )
        .0;
        if length <= 1 || length as usize > folder.len() {
            return 0;
        }
        let mut name = vec![0u16; 32768];
        let name_length = SendMessageW(
            dialog,
            CDM_GETSPEC,
            Some(WPARAM(name.len())),
            Some(LPARAM(name.as_mut_ptr() as isize)),
        )
        .0;
        if name_length <= 0 || name_length as usize > name.len() {
            return 0;
        }
        let name = String::from_utf16_lossy(
            &name[..name.iter().position(|c| *c == 0).unwrap_or(name.len())],
        );
        let typed = Path::new(&name);
        let name = if !name.is_empty()
            && crate::capture_name::number(typed).is_none()
            && !typed.is_dir()
        {
            // Preserve a name the user has typed themselves: only an extension Save
            // writes follows the type list.
            match Listed::of(typed) {
                Some(current) if current != listed => typed.with_extension(listed.extension()),
                _ => return 0,
            }
        } else {
            use std::os::windows::ffi::OsStringExt;
            let folder = PathBuf::from(std::ffi::OsString::from_wide(
                &folder[..length as usize - 1],
            ));
            match listed.next_path(&folder) {
                Ok(path) => PathBuf::from(path.file_name().unwrap()),
                Err(_) => return 0,
            }
        };
        let name = crate::wide(&name);
        // edt1 is the standard filename field of the Explorer-style save dialog.
        SendMessageW(
            dialog,
            CDM_SETCONTROLTEXT,
            Some(WPARAM(0x480)),
            Some(LPARAM(name.as_ptr() as isize)),
        );
        0
    }
}

impl Native {
    pub fn begin_text(&mut self, capture: &mut Capture, hwnd: HWND, p: Point) -> Result<()> {
        unsafe {
            let existing = capture.editor.objects().iter().rev().find(|o| {
                matches!(o.shape, Shape::Text { .. }) && o.hit(p, 5., capture.editor.objects())
            });
            let (position, value, id, size, rgb, rotation) = if let Some(Object {
                id,
                shape:
                    Shape::Text {
                        position,
                        text,
                        size,
                        ..
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
                )
            } else {
                (
                    p,
                    String::new(),
                    None,
                    capture.text_size,
                    capture.editor.style.color,
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
                font,
                drag_anchor: None,
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
                    let extent = match self.renderer.measure_annotation_text(&text, input.size) {
                        Ok(extent) => extent,
                        Err(error) => {
                            MessageBoxW(
                                Some(hwnd),
                                &HSTRING::from(error.to_string()),
                                w!("Text layout failed"),
                                MB_OK | MB_ICONERROR,
                            );
                            self.text = Some(input);
                            return;
                        }
                    };
                    if let Some(id) = input.object {
                        capture.editor.update_text_measured_sized_colored(
                            id,
                            text,
                            extent,
                            input.color,
                            input.size as f64,
                        );
                        capture.editor.select_only(id);
                    } else if let Some(id) = capture.editor.add_text_measured_sized_colored(
                        input.position,
                        text,
                        extent,
                        input.color,
                        input.size as f64,
                    ) {
                        capture.editor.select_only(id);
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
        let refused = || Error::new(E_FAIL, "This capture cannot be saved as an editable image.");
        let (scene, frame) = self.capture.document(background).ok_or_else(refused)?;
        let visible = png_data(&self.flatten(background)?)?;
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
        if let Some(path) = self.document.as_ref().filter(|_| !choose) {
            save_bytes(&self.document_bytes(background)?, path, false)?;
            return Ok(Saved::Document);
        }
        let output = self.flatten(background)?;
        unsafe {
            let folder = settings.save_folder()?;
            let folder_wide = crate::wide(&folder);
            let suggested = crate::capture_name::next_path(&folder, settings.save_format)
                .map_err(|e| Error::new(E_FAIL, e.to_string()))?;
            let filename = suggested.file_name().unwrap().to_string_lossy();
            let mut buffer = vec![0u16; 32768];
            for (i, c) in filename.encode_utf16().enumerate() {
                buffer[i] = c;
            }
            let extension = crate::wide(Path::new(settings.save_format.extension()));
            let mut dialog = OPENFILENAMEW {
                lStructSize: std::mem::size_of::<OPENFILENAMEW>() as u32,
                hwndOwner: hwnd,
                lpstrFilter: SAVE_FILTER,
                nFilterIndex: settings.save_format.index() as u32 + 1,
                lpstrFile: PWSTR(buffer.as_mut_ptr()),
                nMaxFile: buffer.len() as u32,
                lpstrInitialDir: PCWSTR(folder_wide.as_ptr()),
                lpstrDefExt: PCWSTR(extension.as_ptr()),
                Flags: OFN_OVERWRITEPROMPT
                    | OFN_PATHMUSTEXIST
                    | OFN_NOCHANGEDIR
                    | OFN_EXPLORER
                    | OFN_ENABLEHOOK,
                lpfnHook: Some(save_dialog_hook),
                ..Default::default()
            };
            if !GetSaveFileNameW(&mut dialog).as_bool() {
                let error = CommDlgExtendedError().0;
                if error != 0 {
                    return Err(Error::new(
                        E_FAIL,
                        format!("Could not open the Save dialog ({error})"),
                    ));
                }
                return Ok(Saved::Cancelled);
            }
            let path = chosen_path(&buffer);
            let numbered = crate::capture_name::number(&path).is_some();
            let saved = match chosen(&path, dialog.nFilterIndex) {
                Listed::Document => {
                    save_bytes(&self.document_bytes(background)?, &path, numbered)?;
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
    /// What the probe does each time the dialog shows a name, in order.
    enum Step {
        /// Types this folder and confirms, which navigates there.
        Navigate(PathBuf),
        /// Types this name, if any, then picks this entry of the type list.
        List(Option<&'static str>, usize),
        /// Presses Save.
        Confirm,
    }
    struct Probe {
        steps: Vec<Step>,
        names: Vec<String>,
    }
    unsafe fn type_name(dialog: HWND, name: &str) {
        let name: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
        unsafe {
            SendMessageW(
                dialog,
                CDM_SETCONTROLTEXT,
                Some(WPARAM(0x480)),
                Some(LPARAM(name.as_ptr() as isize)),
            );
        }
    }
    unsafe extern "system" fn timeout(hwnd: HWND, _: u32, _: usize, _: u32) {
        unsafe {
            let _ = PostMessageW(Some(hwnd), WM_COMMAND, WPARAM(2), LPARAM(0));
        }
    }
    unsafe extern "system" fn probe(
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> usize {
        unsafe {
            save_dialog_hook(hwnd, message, wparam, lparam);
            if message == WM_NOTIFY
                && lparam.0 != 0
                && (*(lparam.0 as *const OFNOTIFYW)).hdr.code == CDN_INITDONE
            {
                // Once placed, move the dialog off the screens other tests read pixels from.
                let dialog = GetParent(hwnd).unwrap();
                let flags = SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE;
                SetWindowPos(dialog, None, -30000, -30000, 0, 0, flags).unwrap();
            }
            if message == WM_INITDIALOG {
                let data = (*(lparam.0 as *const OPENFILENAMEW)).lCustData.0;
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, data);
                let dialog = GetParent(hwnd).unwrap();
                SetTimer(Some(dialog), 123, 5000, Some(timeout));
            }
            if message == UPDATE_CAPTURE_NAME {
                let state = &mut *(GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Probe);
                let dialog = GetParent(hwnd).unwrap();
                let mut name = vec![0u16; 32768];
                SendMessageW(
                    dialog,
                    CDM_GETSPEC,
                    Some(WPARAM(name.len())),
                    Some(LPARAM(name.as_mut_ptr() as isize)),
                );
                state.names.push(String::from_utf16_lossy(
                    &name[..name.iter().position(|v| *v == 0).unwrap_or(0)],
                ));
                match state.steps.get(state.names.len() - 1) {
                    Some(Step::Navigate(folder)) => {
                        type_name(dialog, &format!("{}\\", folder.display()));
                        let _ = PostMessageW(Some(dialog), WM_COMMAND, WPARAM(1), LPARAM(0));
                    }
                    Some(Step::List(typed, entry)) => {
                        if let Some(typed) = typed {
                            type_name(dialog, typed);
                        }
                        // cmb1 is the standard type list. The dialog only reads it once
                        // it closes, so the probe sends what a click on an entry sends.
                        let list = GetDlgItem(Some(dialog), 0x470).unwrap();
                        let notify = |code: u32| {
                            SendMessageW(
                                dialog,
                                WM_COMMAND,
                                Some(WPARAM(((code as usize) << 16) | 0x470)),
                                Some(LPARAM(list.0 as isize)),
                            );
                        };
                        notify(CBN_DROPDOWN);
                        SendMessageW(list, CB_SETCURSEL, Some(WPARAM(*entry)), None);
                        notify(CBN_SELENDOK);
                        notify(CBN_CLOSEUP);
                        notify(CBN_SELCHANGE);
                    }
                    last => {
                        let _ = KillTimer(Some(dialog), 123);
                        let button = if last.is_some() { 1 } else { 2 };
                        let _ = PostMessageW(Some(dialog), WM_COMMAND, WPARAM(button), LPARAM(0));
                    }
                }
            }
            0
        }
    }
    /// Opens the real dialog on `root`, listing PNG, and plays `steps`. Returns every name
    /// it showed and, when Save was pressed, the path and type list entry chosen.
    fn shown(root: &Path, steps: Vec<Step>) -> (Vec<String>, Option<(PathBuf, u32)>) {
        let _com = crate::ComApartment::new().unwrap();
        let mut state = Probe {
            steps,
            names: vec![],
        };
        let folder = crate::wide(root);
        let mut buffer = vec![0u16; 32768];
        let name = crate::wide(Path::new("Capture 001.png"));
        buffer[..name.len()].copy_from_slice(&name);
        let mut dialog = OPENFILENAMEW {
            lStructSize: std::mem::size_of::<OPENFILENAMEW>() as u32,
            lpstrInitialDir: PCWSTR(folder.as_ptr()),
            lpstrFile: PWSTR(buffer.as_mut_ptr()),
            nMaxFile: buffer.len() as u32,
            lpstrFilter: SAVE_FILTER,
            nFilterIndex: 1,
            lpstrDefExt: w!("png"),
            Flags: OFN_EXPLORER | OFN_ENABLEHOOK | OFN_NOCHANGEDIR | OFN_PATHMUSTEXIST,
            lpfnHook: Some(probe),
            lCustData: LPARAM(&mut state as *mut Probe as isize),
            ..Default::default()
        };
        let saved = unsafe { GetSaveFileNameW(&mut dialog) }
            .as_bool()
            .then(|| (chosen_path(&buffer), dialog.nFilterIndex));
        (state.names, saved)
    }
    // The real dialog takes the foreground, which the tests of windows that must not
    // activate are watching: it runs apart, and one dialog plays the whole scenario.
    #[test]
    #[ignore = "real desktop: opens the Windows Save dialog, run alone by check.ps1"]
    fn real_desktop_save_dialog_follows_the_folder_and_the_type_list() {
        let root =
            std::env::temp_dir().join(format!("shotoshot-save-dialog-{}", std::process::id()));
        let old = root.join("old");
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(root.join("Capture 001.png"), b"keep").unwrap();
        std::fs::write(old.join("Capture 008.png"), b"keep").unwrap();
        std::fs::write(old.join("Shot 011.jpg"), b"keep").unwrap();
        let (names, saved) = shown(
            &root,
            vec![
                Step::Navigate(old.clone()),
                Step::List(None, 2),
                Step::List(None, 1),
                Step::List(Some("Invoice May.jpg"), 0),
                Step::List(Some("Invoice May"), 1),
                Step::Confirm,
            ],
        );
        std::fs::remove_dir_all(root).unwrap();
        assert_eq!(
            names,
            [
                "Shot 002.png",
                // Renumbered for the folder reached, where a JPEG holds the largest number.
                "Shot 012.png",
                // The proposed name follows the type list, to the editable document too.
                "Shot 012.shoto",
                "Shot 012.jpg",
                // A typed name is kept; only its image extension follows.
                "Invoice May.png",
                // The dialog reports a bare name with the extension it is about to append.
                "Invoice May.jpg",
            ]
        );
        let (path, listed) = saved.unwrap();
        assert_eq!(path.file_name().unwrap(), "Invoice May.jpg");
        assert_eq!(path.parent().unwrap().file_name().unwrap(), "old");
        assert_eq!(listed, 2);
        assert_eq!(chosen(&path, listed), Listed::Image(SaveFormat::Jpeg));
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
    fn type_list_entries_follow_the_save_formats() {
        // The list ends at its first empty string.
        let mut units = vec![];
        let mut unit = SAVE_FILTER.0;
        unsafe {
            while *unit != 0 || *unit.add(1) != 0 {
                units.push(*unit);
                unit = unit.add(1);
            }
        }
        let entries: Vec<_> = units
            .split(|unit| *unit == 0)
            .map(String::from_utf16_lossy)
            .collect();
        // The editable document closes the list.
        assert_eq!(entries.len(), SaveFormat::ALL.len() * 2 + 2);
        assert_eq!(entries.last().unwrap(), "*.shoto");
        let document = SaveFormat::ALL.len() as u32 + 1;
        assert_eq!(Listed::entry(document), Listed::Document);
        assert_eq!(Listed::Document.extension(), "shoto");
        for (format, entry) in SaveFormat::ALL.into_iter().zip(entries.chunks(2)) {
            let pattern = format!("*.{}", format.extension());
            assert!(
                entry[1].starts_with(&pattern),
                "{entry:?} is not {format:?}"
            );
            assert_eq!(listed_format(format.index() as u32 + 1), format);
        }
        assert_eq!(listed_format(0), SaveFormat::Png);
    }
}
