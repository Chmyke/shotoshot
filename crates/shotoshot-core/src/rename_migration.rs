//! ROLE: One-time move of the user's data folder from the former product name "Shotshot"
//! to "Shotoshot" (renamed on 2026-09-28, version 0.2.0). Shared by Windows and Linux.
//! CONTRACT: Idempotent; never overwrites data already stored under the new name.
//! DELETE: with its callers once no pre-0.2.0 install remains.
use std::{fs, io, path::Path};

/// Moves `old` to `new` when `old` exists and `new` is missing or empty.
/// Returns whether anything moved. A rename is tried first; across volumes, or while a
/// file is held open, the content is copied instead and `old` is left in place.
pub fn move_data_folder(old: &Path, new: &Path) -> io::Result<bool> {
    if !old.is_dir() {
        return Ok(false);
    }
    if new.exists() {
        if fs::read_dir(new)?.next().is_some() {
            return Ok(false);
        }
        fs::remove_dir(new)?;
    }
    if let Some(parent) = new.parent() {
        fs::create_dir_all(parent)?;
    }
    if fs::rename(old, new).is_ok() {
        return Ok(true);
    }
    copy_tree(old, new).inspect_err(|_| {
        // A partial copy would block every later attempt.
        let _ = fs::remove_dir_all(new);
    })?;
    Ok(true)
}

fn copy_tree(from: &Path, to: &Path) -> io::Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    struct Scratch(PathBuf);
    impl Scratch {
        fn new(name: &str) -> Self {
            let root = std::env::temp_dir()
                .join(format!("shotoshot-rename-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&root);
            let old = root.join("Shotshot");
            fs::create_dir_all(old.join("sound-comparison")).unwrap();
            fs::write(old.join("settings.txt"), "shotshot-settings-10\n").unwrap();
            fs::write(old.join("sound-comparison/00.wav"), [1, 2, 3]).unwrap();
            Self(root)
        }
        fn old(&self) -> PathBuf {
            self.0.join("Shotshot")
        }
        fn new_folder(&self) -> PathBuf {
            self.0.join("Shotoshot")
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn assert_moved(scratch: &Scratch) {
        let new = scratch.new_folder();
        assert_eq!(
            fs::read_to_string(new.join("settings.txt")).unwrap(),
            "shotshot-settings-10\n"
        );
        assert_eq!(
            fs::read(new.join("sound-comparison/00.wav")).unwrap(),
            [1, 2, 3]
        );
        assert!(!scratch.old().exists());
    }

    #[test]
    fn moves_the_old_folder_once_and_is_idempotent() {
        let scratch = Scratch::new("once");
        assert!(move_data_folder(&scratch.old(), &scratch.new_folder()).unwrap());
        assert_moved(&scratch);
        assert!(!move_data_folder(&scratch.old(), &scratch.new_folder()).unwrap());
        assert_moved(&scratch);
    }

    #[test]
    fn replaces_an_empty_new_folder() {
        let scratch = Scratch::new("empty");
        fs::create_dir_all(scratch.new_folder()).unwrap();
        assert!(move_data_folder(&scratch.old(), &scratch.new_folder()).unwrap());
        assert_moved(&scratch);
    }

    #[test]
    fn never_overwrites_data_already_under_the_new_name() {
        let scratch = Scratch::new("kept");
        fs::create_dir_all(scratch.new_folder()).unwrap();
        fs::write(scratch.new_folder().join("settings.txt"), "new").unwrap();
        assert!(!move_data_folder(&scratch.old(), &scratch.new_folder()).unwrap());
        assert_eq!(
            fs::read_to_string(scratch.new_folder().join("settings.txt")).unwrap(),
            "new"
        );
        assert!(scratch.old().join("settings.txt").exists());
    }

    #[test]
    fn nothing_to_do_without_an_old_folder() {
        let scratch = Scratch::new("none");
        fs::remove_dir_all(scratch.old()).unwrap();
        assert!(!move_data_folder(&scratch.old(), &scratch.new_folder()).unwrap());
        assert!(!scratch.new_folder().exists());
    }

    #[test]
    fn copy_fallback_keeps_nested_content() {
        let scratch = Scratch::new("copy");
        copy_tree(&scratch.old(), &scratch.new_folder()).unwrap();
        let new = scratch.new_folder();
        assert!(new.join("settings.txt").is_file());
        assert_eq!(
            fs::read(new.join("sound-comparison/00.wav")).unwrap(),
            [1, 2, 3]
        );
        assert!(scratch.old().join("settings.txt").is_file());
    }
}
