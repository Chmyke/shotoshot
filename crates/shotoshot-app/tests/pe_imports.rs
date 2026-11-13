#![cfg(windows)]
//! What the executable loads when it starts: the resident must stay light at rest (law 2).

/// The DLL names of an import table: `index` 1 for the load-time imports, 13 for the delayed
/// ones; `entry` is the size of one descriptor and `name_at` where its name's address sits.
fn imports(image: &[u8], index: usize, entry: usize, name_at: usize) -> Vec<String> {
    let u16_at = |o: usize| u16::from_le_bytes([image[o], image[o + 1]]) as usize;
    let u32_at = |o: usize| u32::from_le_bytes(image[o..o + 4].try_into().unwrap()) as usize;
    let pe = u32_at(0x3c);
    assert_eq!(&image[pe..pe + 4], b"PE\0\0");
    let sections = u16_at(pe + 6);
    let optional = pe + 24;
    assert_eq!(u16_at(optional), 0x20b, "a 64-bit image");
    let directories = optional + 112;
    let table = optional + u16_at(pe + 20);
    let offset = |rva: usize| {
        (0..sections)
            .map(|i| table + 40 * i)
            .find_map(|s| {
                let (size, va, raw) = (
                    u32_at(s + 8).max(u32_at(s + 16)),
                    u32_at(s + 12),
                    u32_at(s + 20),
                );
                (va <= rva && rva < va + size).then(|| rva - va + raw)
            })
            .expect("an address inside a section")
    };
    let rva = u32_at(directories + 8 * index);
    let mut names = Vec::new();
    if rva == 0 {
        return names;
    }
    let mut at = offset(rva);
    loop {
        let name = u32_at(at + name_at);
        if name == 0 {
            return names;
        }
        let start = offset(name);
        let end = start + image[start..].iter().position(|b| *b == 0).unwrap();
        names.push(String::from_utf8_lossy(&image[start..end]).to_ascii_lowercase());
        at += entry;
    }
}

/// WinHTTP serves only the "Check for updates" click: it is loaded then, not when Shotoshot
/// starts (build.rs, `/DELAYLOAD`).
#[test]
fn the_network_library_loads_only_on_the_update_click() {
    let image = std::fs::read(env!("CARGO_BIN_EXE_shotoshot")).unwrap();
    let load_time = imports(&image, 1, 20, 12);
    let delayed = imports(&image, 13, 32, 4);
    assert!(
        load_time.iter().any(|dll| dll == "user32.dll"),
        "{load_time:?}"
    );
    assert!(
        !load_time.iter().any(|dll| dll == "winhttp.dll"),
        "{load_time:?}"
    );
    assert!(
        delayed.iter().any(|dll| dll == "winhttp.dll"),
        "{delayed:?}"
    );
}
