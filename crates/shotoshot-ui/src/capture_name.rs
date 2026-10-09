//! ROLE: "Shot NNN" numbering shared by every platform's save and instant capture.
//! INVARIANTS: Every image format shares one sequence and earlier "Capture NNN.png" files
//! still count, so numbers only grow and a folder never gets two files of one number.
use shotoshot_core::settings::SaveFormat;
use std::path::{Path, PathBuf};

pub fn next_path(folder: &Path, format: SaveFormat) -> std::io::Result<PathBuf> {
    let mut largest = 0;
    for entry in std::fs::read_dir(folder)? {
        if let Some(number) = number(&entry?.path()) {
            largest = largest.max(number);
        }
    }
    let next = largest
        .checked_add(1)
        .ok_or_else(|| std::io::Error::other("Shot number exhausted"))?;
    Ok(folder.join(format!("Shot {next:03}.{}", format.extension())))
}

pub fn number(path: &Path) -> Option<u64> {
    SaveFormat::of(path)?;
    let stem = path.file_stem()?.to_str()?.to_ascii_lowercase();
    let digits = ["shot", "capture"]
        .iter()
        .find_map(|prefix| stem.strip_prefix(prefix))?
        .trim_start();
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn numbers_follow_existing_files_across_sessions_and_folders() {
        let root = std::env::temp_dir().join(format!("shotoshot-names-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let old = root.join("old");
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(old.join("Capture 001.png"), b"keep").unwrap();
        std::fs::write(old.join("capture02.PNG"), b"keep too").unwrap();
        std::fs::write(old.join("Capture 009.png"), b"latest").unwrap();
        let png = SaveFormat::Png;
        assert_eq!(next_path(&old, png).unwrap(), old.join("Shot 010.png"));
        assert_eq!(next_path(&root, png).unwrap(), root.join("Shot 001.png"));
        std::fs::write(old.join("Capture 999.png"), b"large").unwrap();
        assert_eq!(next_path(&old, png).unwrap(), old.join("Shot 1000.png"));
        std::fs::write(old.join("Shot 1000.png"), b"new name").unwrap();
        assert_eq!(next_path(&old, png).unwrap(), old.join("Shot 1001.png"));
        assert_eq!(number(Path::new("shot7.PNG")), Some(7));
        assert_eq!(number(Path::new("Shotoshot.png")), None);
        assert_eq!(std::fs::read(old.join("Capture 001.png")).unwrap(), b"keep");
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn every_image_format_shares_one_sequence() {
        let folder =
            std::env::temp_dir().join(format!("shotoshot-name-formats-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("Shot 004.png"), b"png").unwrap();
        let jpeg = next_path(&folder, SaveFormat::Jpeg).unwrap();
        assert_eq!(jpeg, folder.join("Shot 005.jpg"));
        std::fs::write(&jpeg, b"jpeg").unwrap();
        std::fs::write(folder.join("shot 7.JPEG"), b"jpeg").unwrap();
        std::fs::write(folder.join("Shot 020.gif"), b"not written by Save").unwrap();
        assert_eq!(
            next_path(&folder, SaveFormat::Png).unwrap(),
            folder.join("Shot 008.png")
        );
        assert_eq!(number(Path::new("Shot 005.jpg")), Some(5));
        assert_eq!(number(Path::new("Shot 020.gif")), None);
        std::fs::remove_dir_all(folder).unwrap();
    }
}
