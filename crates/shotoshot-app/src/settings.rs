//! ROLE: Windows storage of the shared settings format: file location, cross-process
//! lock and write-through replace. The format itself lives in `shotoshot_core::settings`.
pub use shotoshot_core::settings::*;
use std::{
    ffi::OsString,
    os::windows::ffi::OsStringExt,
    path::{Path, PathBuf},
};
use windows::{
    Win32::{
        Foundation::*,
        System::{Com::CoTaskMemFree, Threading::*},
        UI::Shell::*,
    },
    core::*,
};

struct SettingsLock(HANDLE);
impl SettingsLock {
    fn acquire() -> std::result::Result<Self, String> {
        unsafe {
            // PITFALL: Serialize read/merge/replace across resident and editor processes.
            let handle = CreateMutexW(None, false, w!("Local\\Shotoshot.SettingsWrite"))
                .map_err(|error| error.to_string())?;
            match WaitForSingleObject(handle, 2000) {
                WAIT_OBJECT_0 | WAIT_ABANDONED => Ok(Self(handle)),
                _ => {
                    let _ = CloseHandle(handle);
                    Err("Settings are busy. Please try again.".into())
                }
            }
        }
    }
}
impl Drop for SettingsLock {
    fn drop(&mut self) {
        unsafe {
            let _ = ReleaseMutex(self.0);
            let _ = CloseHandle(self.0);
        }
    }
}

pub fn known_folder(id: &GUID) -> Result<PathBuf> {
    unsafe {
        let value = SHGetKnownFolderPath(id, KF_FLAG_DEFAULT, None)?;
        let result = PathBuf::from(OsString::from_wide(value.as_wide()));
        CoTaskMemFree(Some(value.0.cast()));
        Ok(result)
    }
}
/// Windows storage of the settings file.
pub trait Store: Sized {
    fn path() -> Result<PathBuf>;
    fn load() -> std::result::Result<Self, String>;
    fn save(&self) -> std::result::Result<(), String>;
    /// Changes only the editor's custom color, preserving current resident preferences.
    fn save_custom_color(color: [u8; 3]) -> std::result::Result<(), String>;
    /// Changes only the editor's stroke width and eraser size.
    fn save_brush(brush: Brush) -> std::result::Result<(), String>;
    /// Changes only whether the board is shown, preserving current resident preferences.
    fn save_board_visible(visible: bool) -> std::result::Result<(), String>;
    /// Remembers where and in which format the editor last saved.
    fn save_destination(folder: &Path, format: SaveFormat) -> std::result::Result<(), String>;
    fn save_folder(&self) -> Result<PathBuf>;
}
impl Store for Settings {
    fn path() -> Result<PathBuf> {
        Ok(known_folder(&FOLDERID_LocalAppData)?
            .join("Shotoshot")
            .join("settings.txt"))
    }
    fn load() -> std::result::Result<Self, String> {
        let path = Self::path().map_err(|e| e.to_string())?;
        Self::load_from(&path)
    }
    fn save(&self) -> std::result::Result<(), String> {
        let _lock = SettingsLock::acquire()?;
        let path = Self::path().map_err(|e| e.to_string())?;
        self.save_to(&path, None, None, replace)
    }
    fn save_custom_color(color: [u8; 3]) -> std::result::Result<(), String> {
        let _lock = SettingsLock::acquire()?;
        let path = Self::path().map_err(|e| e.to_string())?;
        Self::save_custom_color_to(&path, color, replace)
    }
    fn save_brush(brush: Brush) -> std::result::Result<(), String> {
        let _lock = SettingsLock::acquire()?;
        let path = Self::path().map_err(|e| e.to_string())?;
        Self::save_editor_to(
            &path,
            None,
            Some(brush.stroke_width),
            Some(brush.eraser_size),
            None,
            replace,
        )
    }
    fn save_board_visible(visible: bool) -> std::result::Result<(), String> {
        let _lock = SettingsLock::acquire()?;
        let path = Self::path().map_err(|e| e.to_string())?;
        Self::save_board_visible_to(&path, visible, replace)
    }
    fn save_destination(folder: &Path, format: SaveFormat) -> std::result::Result<(), String> {
        let _lock = SettingsLock::acquire()?;
        let path = Self::path().map_err(|e| e.to_string())?;
        Self::save_editor_to(&path, None, None, None, Some((folder, format)), replace)
    }
    fn save_folder(&self) -> Result<PathBuf> {
        if self.folder.is_dir() {
            Ok(self.folder.clone())
        } else {
            known_folder(&FOLDERID_Desktop)
        }
    }
}
/// Write-through replace, so a crash right after saving keeps the new file.
fn replace(temp: &Path, path: &Path) -> std::result::Result<(), String> {
    unsafe {
        windows::Win32::Storage::FileSystem::MoveFileExW(
            PCWSTR(crate::wide(temp).as_ptr()),
            PCWSTR(crate::wide(path).as_ptr()),
            windows::Win32::Storage::FileSystem::MOVEFILE_REPLACE_EXISTING
                | windows::Win32::Storage::FileSystem::MOVEFILE_WRITE_THROUGH,
        )
        .map_err(|e| e.to_string())
    }
}
