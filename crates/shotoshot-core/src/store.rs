//! ROLE: The rules that differ when Shotoshot runs from its Microsoft Store package (MSIX), kept
//! pure so they are tested here; the Windows crate asks the system and applies them.
//! Measured on 2026-11-21 (docs/contexte/microsoft-store.md): inside the package, a folder
//! created in %LOCALAPPDATA% is hidden in the package's LocalCache, invisible to Explorer, while
//! a folder that already existed is written for real.
use std::path::{Path, PathBuf};

/// The Store's own "Downloads and updates" page: the Updates row opens it instead of asking the
/// site, since the Store updates the package itself.
pub const UPDATES_PAGE: &str = "ms-windows-store://downloadsandupdates";
/// The package's startup task, declared in packaging/store/AppxManifest.xml.
pub const STARTUP_TASK: &str = "ShotoshotStartup";

/// The folder of settings and board. `local` is %LOCALAPPDATA%, `family` the package family
/// name when packaged. Packaged, the folder is named by its physical place, which Explorer sees
/// (the board's wallpaper is read by Explorer), unless only the website version's folder exists:
/// then both versions share it, real on disk.
pub fn data_folder(local: &Path, family: Option<&str>, exists: impl Fn(&Path) -> bool) -> PathBuf {
    let shared = local.join("Shotoshot");
    let Some(family) = family else {
        return shared;
    };
    let own = local
        .join("Packages")
        .join(family)
        .join("LocalCache")
        .join("Local")
        .join("Shotoshot");
    if !exists(&own) && exists(&shared) {
        shared
    } else {
        own
    }
}

/// What the resident gives back when Windows closes it with `WM_ENDSESSION`: only a close for
/// servicing (`ENDSESSION_CLOSEAPP`, sent when the package is removed or updated) gives back the
/// Print Screen key and the wallpaper; a log off or shut down keeps them for the next session.
pub fn gives_back_on_end_session(ending: bool, flags: u32) -> bool {
    const ENDSESSION_CLOSEAPP: u32 = 0x1;
    ending && flags & ENDSESSION_CLOSEAPP != 0
}

#[cfg(test)]
mod tests {
    use super::*;

    const FAMILY: &str = "MickaelMancini.Shotoshot_abc";

    fn own(local: &Path) -> PathBuf {
        // One component at a time: the core's tests also run on Linux, where `\` splits nothing.
        ["Packages", FAMILY, "LocalCache", "Local", "Shotoshot"]
            .iter()
            .fold(local.to_path_buf(), |path, part| path.join(part))
    }

    #[test]
    fn the_website_version_keeps_its_folder() {
        let local = Path::new(r"D:\Profile\AppData\Local");
        assert_eq!(data_folder(local, None, |_| true), local.join("Shotoshot"));
    }

    #[test]
    fn a_fresh_store_install_names_the_folder_explorer_sees() {
        let local = Path::new(r"D:\Profile\AppData\Local");
        assert_eq!(data_folder(local, Some(FAMILY), |_| false), own(local));
    }

    #[test]
    fn the_store_version_shares_the_folder_the_website_version_left() {
        let local = Path::new(r"D:\Profile\AppData\Local");
        let shared = local.join("Shotoshot");
        assert_eq!(data_folder(local, Some(FAMILY), |p| p == shared), shared);
    }

    #[test]
    fn the_store_version_keeps_its_own_folder_once_it_has_one() {
        let local = Path::new(r"D:\Profile\AppData\Local");
        assert_eq!(data_folder(local, Some(FAMILY), |_| true), own(local));
    }

    #[test]
    fn only_a_close_for_servicing_gives_back() {
        assert!(gives_back_on_end_session(true, 0x1));
        assert!(gives_back_on_end_session(true, 0x1 | 0x8000_0000));
        // Log off, shut down, or a cancelled end.
        assert!(!gives_back_on_end_session(true, 0x8000_0000));
        assert!(!gives_back_on_end_session(true, 0));
        assert!(!gives_back_on_end_session(false, 0x1));
    }
}
