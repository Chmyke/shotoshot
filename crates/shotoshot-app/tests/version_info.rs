#![cfg(windows)]
use windows::Win32::Storage::FileSystem::*;
use windows::core::{HSTRING, PCWSTR};

/// Reads one string of the English (US, Unicode) version block of `exe`.
fn version_string(exe: &str, key: &str) -> String {
    let path = HSTRING::from(exe);
    unsafe {
        let size = GetFileVersionInfoSizeW(&path, None);
        assert!(size > 0, "{exe} has no version resource");
        let mut block = vec![0u8; size as usize];
        GetFileVersionInfoW(&path, None, size, block.as_mut_ptr().cast()).unwrap();
        let query = HSTRING::from(format!("\\StringFileInfo\\040904B0\\{key}"));
        let mut value = std::ptr::null_mut();
        let mut length = 0;
        assert!(
            VerQueryValueW(block.as_ptr().cast(), &query, &mut value, &mut length).as_bool(),
            "missing {key}"
        );
        PCWSTR(value.cast()).to_string().unwrap()
    }
}

#[test]
fn executable_identifies_its_publisher_and_version() {
    let exe = env!("CARGO_BIN_EXE_shotoshot");
    assert_eq!(version_string(exe, "ProductName"), "Shotoshot");
    assert_eq!(
        version_string(exe, "FileDescription"),
        "Shotoshot screenshot tool"
    );
    assert_eq!(version_string(exe, "CompanyName"), "Mickaël Mancini");
    assert_eq!(version_string(exe, "OriginalFilename"), "shotoshot.exe");
    assert_eq!(
        version_string(exe, "FileVersion"),
        format!("{}.0", env!("CARGO_PKG_VERSION"))
    );
}
