//! ROLE: Linux storage of the settings file shared with Windows (`shotoshot_core::settings`),
//! at `$XDG_CONFIG_HOME/shotoshot/settings.txt` (default `~/.config`).
//! INVARIANTS: Writers hold an exclusive lock on `settings.lock` in the same folder; a
//! damaged file is reported and never overwritten.
use shotoshot_core::settings::{Settings, rename};
use std::path::{Path, PathBuf};

fn folder() -> PathBuf {
    config_home().join("shotoshot")
}
fn config_home() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::var_os("HOME")
                .map_or_else(|| PathBuf::from("."), PathBuf::from)
                .join(".config")
        })
}

/// One-time move of `~/.config/shotshot`, the folder of the former product name (renamed
/// on 2026-09-28). DELETE: once no pre-0.2.0 install remains.
pub fn migrate_former_name() {
    let _ = shotoshot_core::rename_migration::move_data_folder(
        &config_home().join("shotshot"),
        &folder(),
    );
}

pub fn load() -> Result<Settings, String> {
    load_in(&folder())
}
fn load_in(folder: &Path) -> Result<Settings, String> {
    Settings::load_from(&folder.join("settings.txt"))
}

/// Stores the editor's stroke width, eraser size and custom colour when given, keeping
/// every other preference as stored.
pub fn save_editor(
    width: Option<u32>,
    eraser: Option<u32>,
    color: Option<[u8; 3]>,
) -> Result<(), String> {
    save_editor_in(&folder(), width, eraser, color)
}
fn save_editor_in(
    folder: &Path,
    width: Option<u32>,
    eraser: Option<u32>,
    color: Option<[u8; 3]>,
) -> Result<(), String> {
    if width.is_none() && eraser.is_none() && color.is_none() {
        return Ok(());
    }
    std::fs::create_dir_all(folder).map_err(|e| e.to_string())?;
    let lock = std::fs::File::create(folder.join("settings.lock")).map_err(|e| e.to_string())?;
    lock.lock().map_err(|e| e.to_string())?;
    let path = folder.join("settings.txt");
    Settings::save_editor_to(&path, color, width, eraser, None, rename)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editor_preferences_persist_separately_and_a_damaged_file_is_kept() {
        let folder =
            std::env::temp_dir().join(format!("shotoshot-settings-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        assert_eq!(load_in(&folder).unwrap(), Settings::default());
        save_editor_in(&folder, Some(7), None, None).unwrap();
        save_editor_in(&folder, None, Some(40), None).unwrap();
        save_editor_in(&folder, None, None, Some([12, 34, 56])).unwrap();
        let stored = load_in(&folder).unwrap();
        assert_eq!(
            (
                stored.capture_stroke_width,
                stored.capture_eraser_size,
                stored.last_custom_color
            ),
            (7, 40, [12, 34, 56])
        );
        let path = folder.join("settings.txt");
        std::fs::write(&path, "damaged settings").unwrap();
        assert!(load_in(&folder).is_err());
        assert!(save_editor_in(&folder, Some(9), None, None).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "damaged settings");
        std::fs::remove_dir_all(&folder).unwrap();
    }
}
