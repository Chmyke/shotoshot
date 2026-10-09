use super::*;

const UPDATE_CAPTURE_NAME: u32 = WM_APP + 83;

// Defer until Explorer has finished updating its filename field.
unsafe extern "system" fn save_dialog_hook(
    hwnd: HWND,
    message: u32,
    _: WPARAM,
    lparam: LPARAM,
) -> usize {
    unsafe {
        if message == WM_NOTIFY && lparam.0 != 0 {
            let notification = &*(lparam.0 as *const OFNOTIFYW);
            if notification.hdr.code == CDN_FOLDERCHANGE {
                let _ = PostMessageW(Some(hwnd), UPDATE_CAPTURE_NAME, WPARAM(0), LPARAM(0));
            }
        }
        if message != UPDATE_CAPTURE_NAME {
            return 0;
        }
        let Ok(dialog) = GetParent(hwnd) else {
            return 0;
        };
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
        // Preserve a name the user has typed themselves.
        if !name.is_empty()
            && crate::capture_name::number(std::path::Path::new(&name)).is_none()
            && !std::path::Path::new(&name).is_dir()
        {
            return 0;
        }
        use std::os::windows::ffi::OsStringExt;
        let folder = std::path::PathBuf::from(std::ffi::OsString::from_wide(
            &folder[..length as usize - 1],
        ));
        if let Ok(path) = crate::capture_name::next_path(&folder) {
            let name = crate::wide(std::path::Path::new(path.file_name().unwrap()));
            // edt1 is the standard filename field of the Explorer-style save dialog.
            SendMessageW(
                dialog,
                CDM_SETCONTROLTEXT,
                Some(WPARAM(0x480)),
                Some(LPARAM(name.as_ptr() as isize)),
            );
        }
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
            let font = match super::text::configure_input(input, size) {
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
                    let extent = match self.renderer.measure_text(&text, input.size) {
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
impl Session {
    pub fn save(&self, hwnd: HWND, settings: &mut Settings) -> Result<bool> {
        // Snapshot before the modal dialog, at the Save action.
        let output = self.capture_output()?;
        unsafe {
            let folder = settings.save_folder()?;
            let folder_wide = crate::wide(&folder);
            let suggested = crate::capture_name::next_path(&folder)
                .map_err(|e| Error::new(E_FAIL, e.to_string()))?;
            let filename = suggested.file_name().unwrap().to_string_lossy();
            let mut buffer = vec![0u16; 32768];
            for (i, c) in filename.encode_utf16().enumerate() {
                buffer[i] = c;
            }
            let mut dialog = OPENFILENAMEW {
                lStructSize: std::mem::size_of::<OPENFILENAMEW>() as u32,
                hwndOwner: hwnd,
                lpstrFilter: w!("PNG image\0*.png\0\0"),
                lpstrFile: PWSTR(buffer.as_mut_ptr()),
                nMaxFile: buffer.len() as u32,
                lpstrInitialDir: PCWSTR(folder_wide.as_ptr()),
                lpstrDefExt: w!("png"),
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
                return Ok(false);
            }
            use std::os::windows::ffi::OsStringExt;
            let path = std::path::PathBuf::from(std::ffi::OsString::from_wide(
                &buffer[..buffer.iter().position(|v| *v == 0).unwrap_or(buffer.len())],
            ));
            if crate::capture_name::number(&path).is_some() {
                save_numbered_png(&output, &path)?;
            } else {
                save_png(&output, &path)?;
            }
            if let Some(parent) = path.parent() {
                settings.folder = parent.into();
            }
            Ok(true)
        }
    }
}

#[cfg(test)]
mod save_dialog_tests {
    use super::*;
    struct Probe {
        destination: Vec<u16>,
        names: Vec<String>,
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
                if state.names.len() == 1 {
                    SendMessageW(
                        dialog,
                        CDM_SETCONTROLTEXT,
                        Some(WPARAM(0x480)),
                        Some(LPARAM(state.destination.as_ptr() as isize)),
                    );
                    let _ = PostMessageW(Some(dialog), WM_COMMAND, WPARAM(1), LPARAM(0));
                }
                if state.names.len() == 2 {
                    let _ = KillTimer(Some(dialog), 123);
                    let _ = PostMessageW(Some(dialog), WM_COMMAND, WPARAM(2), LPARAM(0));
                }
            }
            0
        }
    }
    #[test]
    fn native_save_dialog_renumbers_after_folder_navigation() {
        let _com = crate::ComApartment::new().unwrap();
        let root =
            std::env::temp_dir().join(format!("shotoshot-save-dialog-{}", std::process::id()));
        let old = root.join("old");
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(root.join("Capture 001.png"), b"keep").unwrap();
        std::fs::write(old.join("Capture 008.png"), b"keep").unwrap();
        let mut state = Probe {
            destination: format!("{}\\", old.display())
                .encode_utf16()
                .chain(Some(0))
                .collect(),
            names: vec![],
        };
        let folder = crate::wide(&root);
        let mut buffer = vec![0u16; 32768];
        let name = crate::wide(std::path::Path::new("Capture 001.png"));
        buffer[..name.len()].copy_from_slice(&name);
        let mut dialog = OPENFILENAMEW {
            lStructSize: std::mem::size_of::<OPENFILENAMEW>() as u32,
            lpstrInitialDir: PCWSTR(folder.as_ptr()),
            lpstrFile: PWSTR(buffer.as_mut_ptr()),
            nMaxFile: buffer.len() as u32,
            lpstrFilter: w!("PNG image\0*.png\0\0"),
            lpstrDefExt: w!("png"),
            Flags: OFN_EXPLORER | OFN_ENABLEHOOK | OFN_NOCHANGEDIR | OFN_PATHMUSTEXIST,
            lpfnHook: Some(probe),
            lCustData: LPARAM(&mut state as *mut Probe as isize),
            ..Default::default()
        };
        unsafe {
            let _ = GetSaveFileNameW(&mut dialog);
        }
        std::fs::remove_dir_all(root).unwrap();
        assert_eq!(state.names, ["Shot 002.png", "Shot 009.png"]);
    }
}
