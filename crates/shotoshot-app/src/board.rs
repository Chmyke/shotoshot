//! ROLE: Desktop notes baked into the wallpaper: files, screens, base and session glue.
//! INVARIANTS: Nothing here stays resident; every function runs to completion and returns.
pub mod bake;
pub mod base;
pub mod clear;
pub mod probe;
pub mod rendering;
pub mod wallpaper;
use crate::settings::{Settings, Store};
use std::path::{Path, PathBuf};
use windows::{Win32::Foundation::E_FAIL, core::Result};

/// Where the board folder is, next to the settings file, whether it exists or not.
fn location() -> Result<PathBuf> {
    let settings = Settings::path()?;
    Ok(settings.parent().unwrap_or(Path::new(".")).join("board"))
}
/// The board folder next to the settings file, created on first use.
pub fn folder() -> Result<PathBuf> {
    let folder = location()?;
    std::fs::create_dir_all(&folder)
        .map_err(|error| windows::core::Error::new(E_FAIL, error.to_string()))?;
    Ok(folder)
}
pub fn document_path(folder: &Path) -> PathBuf {
    folder.join("page-1.txt")
}
pub fn base_path(folder: &Path) -> PathBuf {
    folder.join("base.txt")
}

/// A file name token for a screen id: its first letters and digits, plus a hash of the whole id
/// so two ids never share a file.
pub fn slug(id: &str) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in id.bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    let kept: String = id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .take(24)
        .collect();
    format!("{kept}-{hash:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_slug_is_a_stable_file_name_token_that_tells_screens_apart() {
        let first = "\\\\?\\DISPLAY#IVM613D#5&1a5da745&0&UID4352#{e6f07b5f}";
        let second = "\\\\?\\DISPLAY#IVM613D#5&1a5da745&0&UID4353#{e6f07b5f}";
        assert_eq!(slug(first), slug(first));
        assert_ne!(slug(first), slug(second));
        let token = slug(first);
        assert!(
            token.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'),
            "{token}"
        );
        assert!(token.starts_with("DISPLAYIVM613D5"), "{token}");
    }

    #[test]
    fn document_and_base_live_in_the_folder() {
        let folder = Path::new("board");
        assert_eq!(document_path(folder), folder.join("page-1.txt"));
        assert_eq!(base_path(folder), folder.join("base.txt"));
    }
}
