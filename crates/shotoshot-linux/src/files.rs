//! ROLE: Whole-file writes: a reader finds the previous file or the complete new one, never
//! a part of it.
//! INVARIANTS: The temporary file lives beside its destination, so publishing it is one
//! rename or link inside a file system; it never outlives the call.
use shotoshot_core::settings::SaveFormat;
use shotoshot_ui::capture_name;
use std::{
    ffi::OsString,
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};

/// `bytes` in a hidden file beside `path`, flushed to the disk.
fn staged(bytes: &[u8], path: &Path) -> io::Result<PathBuf> {
    let name = path
        .file_name()
        .ok_or_else(|| io::Error::other("the file has no name"))?;
    let mut hidden = OsString::from(".");
    hidden.push(name);
    hidden.push(format!(".{}.tmp", std::process::id()));
    let temporary = path.with_file_name(hidden);
    let written = fs::File::create(&temporary).and_then(|mut file| {
        file.write_all(bytes)?;
        // Without it a power cut after the rename can leave the new name on an empty file.
        file.sync_all()
    });
    if let Err(error) = written {
        let _ = fs::remove_file(&temporary);
        return Err(error);
    }
    Ok(temporary)
}

/// Makes `path` a complete file holding `bytes`, replacing the one there. A link is
/// followed and a read-only file refused, as writing into the file itself would.
pub fn replace(bytes: &[u8], path: &Path) -> io::Result<()> {
    let path = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let previous = fs::metadata(&path).ok().map(|m| m.permissions());
    if previous.as_ref().is_some_and(|p| p.readonly()) {
        return Err(io::ErrorKind::PermissionDenied.into());
    }
    let temporary = staged(bytes, &path)?;
    let published = previous
        .map_or(Ok(()), |p| fs::set_permissions(&temporary, p))
        .and_then(|()| fs::rename(&temporary, &path));
    if published.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    published
}

/// Creates `path` holding `bytes`, and fails with `AlreadyExists` rather than replace
/// anything: a numbered capture is never the one that erases another.
pub fn create(bytes: &[u8], path: &Path) -> io::Result<()> {
    let temporary = staged(bytes, path)?;
    let published = fs::hard_link(&temporary, path).or_else(|error| {
        if error.kind() == io::ErrorKind::AlreadyExists || path.symlink_metadata().is_ok() {
            Err(io::ErrorKind::AlreadyExists.into())
        } else {
            // No hard links here (FAT, some network shares): checked above, then renamed.
            fs::rename(&temporary, path)
        }
    });
    // Gone already when it was renamed.
    let _ = fs::remove_file(&temporary);
    published
}

/// Saves `bytes` as the next "Shot NNN" of `folder`, created if missing: a PNG image, or
/// an editable document when `document` is set. Both take their number from one sequence.
pub fn save_numbered(bytes: &[u8], folder: &Path, document: bool) -> io::Result<PathBuf> {
    fs::create_dir_all(folder)?;
    let mut attempts = 0;
    loop {
        let mut path = capture_name::next_path(folder, SaveFormat::Png)?;
        if document {
            path.set_extension(capture_name::DOCUMENT);
        }
        attempts += 1;
        match create(bytes, &path) {
            // Another capture took this number meanwhile: the next one is free.
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists && attempts < 8 => {}
            created => return created.map(|()| path),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn folder(name: &str) -> PathBuf {
        let folder = std::env::temp_dir().join(format!("shotoshot-{name}-{}", std::process::id()));
        fs::create_dir_all(&folder).unwrap();
        folder
    }
    fn names(folder: &Path) -> Vec<String> {
        let mut names: Vec<_> = fs::read_dir(folder)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn a_replaced_file_is_whole_and_alone() {
        let folder = folder("replace");
        let path = folder.join("Shot 001.png");
        replace(b"first", &path).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        replace(b"second, longer", &path).unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"second, longer");
        // The file stays as private as its owner made it.
        let mode = fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
        assert_eq!(names(&folder), ["Shot 001.png"]);
        fs::remove_dir_all(folder).unwrap();
    }
    #[test]
    fn a_failed_write_leaves_the_previous_file_and_nothing_else() {
        let folder = folder("refused");
        let path = folder.join("kept.shoto");
        fs::write(&path, b"kept").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o444)).unwrap();
        let refused = replace(b"lost", &path).unwrap_err();
        assert_eq!(refused.kind(), io::ErrorKind::PermissionDenied);
        assert_eq!(fs::read(&path).unwrap(), b"kept");
        assert_eq!(names(&folder), ["kept.shoto"]);
        assert!(replace(b"nowhere", &folder.join("missing/a.png")).is_err());
        assert!(replace(b"unnamed", Path::new("/")).is_err());
        fs::remove_dir_all(folder).unwrap();
    }
    #[test]
    fn a_link_keeps_pointing_at_the_replaced_file() {
        let folder = folder("link");
        let (file, link) = (folder.join("file.shoto"), folder.join("link.shoto"));
        fs::write(&file, b"old").unwrap();
        std::os::unix::fs::symlink(&file, &link).unwrap();
        replace(b"new", &link).unwrap();
        assert!(fs::symlink_metadata(&link).unwrap().is_symlink());
        assert_eq!(fs::read(&file).unwrap(), b"new");
        fs::remove_dir_all(folder).unwrap();
    }
    #[test]
    fn a_created_file_never_takes_the_place_of_another() {
        let folder = folder("create");
        let path = folder.join("Shot 002.shoto");
        create(b"mine", &path).unwrap();
        let taken = create(b"theirs", &path).unwrap_err();
        assert_eq!(taken.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(fs::read(&path).unwrap(), b"mine");
        assert_eq!(names(&folder), ["Shot 002.shoto"]);
        // A dangling link is a name already taken too.
        let dangling = folder.join("Shot 003.png");
        std::os::unix::fs::symlink(folder.join("gone"), &dangling).unwrap();
        assert!(create(b"other", &dangling).is_err());
        fs::remove_dir_all(folder).unwrap();
    }
    #[test]
    fn images_and_documents_share_one_numbering() {
        let folder = folder("numbered").join("Pictures");
        let image = save_numbered(b"png", &folder, false).unwrap();
        assert_eq!(image, folder.join("Shot 001.png"));
        let document = save_numbered(b"shoto", &folder, true).unwrap();
        assert_eq!(document, folder.join("Shot 002.shoto"));
        assert_eq!(
            save_numbered(b"png", &folder, false).unwrap(),
            folder.join("Shot 003.png")
        );
        assert_eq!(fs::read(&image).unwrap(), b"png");
        assert_eq!(fs::read(&document).unwrap(), b"shoto");
        assert_eq!(names(&folder).len(), 3);
        fs::remove_dir_all(folder.parent().unwrap()).unwrap();
    }
}
