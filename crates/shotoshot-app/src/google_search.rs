//! Explicit image search only: annotated crop -> local page -> default browser -> Google Lens.
//! The browser sends the image itself, as Chrome's own Lens form does (`lens_form.ts`): Google
//! binds a visual search to the client that uploaded it, so a search uploaded by Shotoshot and
//! opened in the browser showed "Expired visual search" (2026-10-14). Shotoshot itself makes no
//! network request and leaves the clipboard alone; the page lives a minute in the temp folder.
//! PITFALL: Google refuses the form from another site's origin (HTTP 403), and a local server is
//! one; a page opened from a file posts with the origin `null`, which it accepts.
use shotoshot_text::{Key, Language, tr_in};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use windows::{
    Win32::{
        Foundation::*,
        UI::{Shell::*, WindowsAndMessaging::SW_SHOWNORMAL},
    },
    core::*,
};

/// Followed by `&hl=` and the language of Google's page.
const FORM_ACTION: &str = "https://lens.google.com/v3/upload?ep=cntpubb";
const PAGE_PREFIX: &str = "lens-";
const PAGE_EXTENSION: &str = "html";
/// Started by `open`: waits `PAGE_LIFETIME`, then deletes the page it was given.
pub(crate) const FORGET_PAGE: &str = "--forget-lens-page";
/// Long enough for a browser started cold to read the page.
const PAGE_LIFETIME: Duration = Duration::from_secs(60);

fn io(error: std::io::Error) -> Error {
    Error::new(E_FAIL, error.to_string())
}

fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, &b)| n | u32::from(b) << (16 - 8 * i));
        for i in 0..4 {
            out.push(if i <= chunk.len() {
                ALPHABET[(n >> (18 - 6 * i) & 63) as usize] as char
            } else {
                '='
            });
        }
    }
    out
}

/// `text` as HTML character data or attribute value.
fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Google's name for `language` in `hl`: its Chinese codes name the region, not the script.
fn google_hl(language: Language) -> &'static str {
    match language {
        Language::ZhHans => "zh-CN",
        Language::ZhHant => "zh-TW",
        _ => language.tag(),
    }
}

/// A page that hands `png` to Google Lens as soon as it opens, with the fields of Chrome's form;
/// what it says while it sends, and Google's page after it, are in `language`.
fn page_in(png: &[u8], start_ms: u128, language: Language) -> String {
    let hl = google_hl(language);
    let action = format!("{FORM_ACTION}&hl={hl}&st={start_ms}").replace('&', "&amp;");
    let image = base64(png);
    let tag = language.tag();
    let sending = escape(tr_in(language, Key::LensSending));
    let needs_script = escape(tr_in(language, Key::LensNeedsScript));
    format!(
        r#"<!doctype html>
<html lang="{tag}"><meta charset="utf-8"><title>Google Lens</title>
<body style="background:#202124;color:#e8eaed;font:15px 'Segoe UI',sans-serif;margin:40px">
<p>{sending}</p>
<noscript><p>{needs_script}</p></noscript>
<form id="lens" method="post" enctype="multipart/form-data" action="{action}">
<input id="image" type="file" name="encoded_image" accept="image/png" hidden>
</form>
<script>
const text = atob("{image}");
const bytes = new Uint8Array(text.length);
for (let i = 0; i < text.length; i++) bytes[i] = text.charCodeAt(i);
const files = new DataTransfer();
files.items.add(new File([bytes], "capture.png", {{ type: "image/png" }}));
document.getElementById("image").files = files.files;
document.getElementById("lens").submit();
</script>
"#
    )
}

fn folder() -> PathBuf {
    std::env::temp_dir().join("Shotoshot")
}

fn is_page(path: &Path) -> bool {
    path.extension().is_some_and(|e| e == PAGE_EXTENSION)
        && path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.starts_with(PAGE_PREFIX))
}

/// Deletes the pages older than their lifetime that a forgetter could not remove (shutdown,
/// failed start). Anything else in `folder` is left alone.
fn forget_stale(folder: &Path, now: SystemTime) {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return;
    };
    for path in entries.flatten().map(|e| e.path()) {
        let expired = std::fs::metadata(&path)
            .and_then(|m| m.modified())
            .is_ok_and(|t| now.duration_since(t).unwrap_or_default() >= PAGE_LIFETIME);
        if is_page(&path) && expired {
            let _ = std::fs::remove_file(path);
        }
    }
}

/// Writes the page for `image`; nothing leaves the machine until the browser opens it.
pub(crate) fn prepare(image: &crate::Image) -> Result<PathBuf> {
    let _com = crate::ComApartment::new()?;
    let png = crate::png_bytes(image)?;
    let folder = folder();
    std::fs::create_dir_all(&folder).map_err(io)?;
    forget_stale(&folder, SystemTime::now());
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let path = folder.join(format!(
        "{PAGE_PREFIX}{}-{}.{PAGE_EXTENSION}",
        std::process::id(),
        now.as_nanos()
    ));
    let page = page_in(&png, now.as_millis(), shotoshot_text::language());
    std::fs::write(&path, page).map_err(io)?;
    Ok(path)
}

/// The program that opens https links: the `.html` association may be an editor.
fn browser() -> Option<PathBuf> {
    let mut buffer = [0u16; 1024];
    let mut size = buffer.len() as u32;
    let found = unsafe {
        AssocQueryStringW(
            ASSOCF_NOTRUNCATE,
            ASSOCSTR_EXECUTABLE,
            w!("https"),
            w!("open"),
            Some(PWSTR(buffer.as_mut_ptr())),
            &mut size,
        )
    };
    if found.is_err() {
        return None;
    }
    let end = buffer.iter().position(|c| *c == 0).unwrap_or(buffer.len());
    let path = PathBuf::from(String::from_utf16(&buffer[..end]).ok()?);
    path.is_file().then_some(path)
}

/// Opens `page` in the default browser and schedules its deletion; false if nothing opened.
pub(crate) fn open(owner: HWND, page: &Path) -> bool {
    let opened = unsafe {
        match browser() {
            Some(browser) => ShellExecuteW(
                Some(owner),
                w!("open"),
                &HSTRING::from(browser.as_os_str()),
                &HSTRING::from(format!("\"{}\"", page.display())),
                None,
                SW_SHOWNORMAL,
            ),
            None => ShellExecuteW(
                Some(owner),
                w!("open"),
                &HSTRING::from(page.as_os_str()),
                None,
                None,
                SW_SHOWNORMAL,
            ),
        }
    };
    if opened.0 as isize <= 32 {
        let _ = std::fs::remove_file(page);
        return false;
    }
    // The editor ends with its session: a process of its own deletes the page later. If it
    // cannot start, the next search removes the page.
    if let Ok(exe) = std::env::current_exe() {
        let _ = std::process::Command::new(exe)
            .arg(FORGET_PAGE)
            .arg(page)
            .spawn();
    }
    true
}

/// `--forget-lens-page <page>`: deletes a page of ours once the browser has had time to read it.
pub(crate) fn forget_later(page: &Path) -> Result<()> {
    if !is_page(page) || page.parent() != Some(folder().as_path()) {
        return Err(Error::new(E_INVALIDARG, "Not a Google Lens page"));
    }
    std::thread::sleep(PAGE_LIFETIME);
    match std::fs::remove_file(page) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(io(error)),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn decode(text: &str) -> Vec<u8> {
        const ALPHABET: &[u8; 64] =
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let digits: Vec<u32> = text
            .bytes()
            .filter(|&b| b != b'=')
            .map(|b| ALPHABET.iter().position(|&a| a == b).unwrap() as u32)
            .collect();
        let mut out = Vec::new();
        for group in digits.chunks(4) {
            let n = group
                .iter()
                .enumerate()
                .fold(0, |n, (i, &d)| n | d << (18 - 6 * i));
            for i in 0..group.len() - 1 {
                out.push((n >> (16 - 8 * i)) as u8);
            }
        }
        out
    }
    #[test]
    fn base64_matches_the_standard_alphabet_and_padding() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
        let all: Vec<u8> = (0..=255).collect();
        assert_eq!(decode(&base64(&all)), all);
    }
    #[test]
    fn page_posts_the_png_with_the_fields_of_chromes_lens_form() {
        let png = [0x89, b'P', b'N', b'G', 0, 255, 13, 10];
        let html = page_in(&png, 1234, Language::En);
        assert!(html.contains(
            r#"action="https://lens.google.com/v3/upload?ep=cntpubb&amp;hl=en&amp;st=1234""#
        ));
        assert!(html.contains(r#"method="post" enctype="multipart/form-data""#));
        assert!(html.contains(r#"name="encoded_image""#));
        let start = html.find("atob(\"").unwrap() + 6;
        let end = start + html[start..].find('"').unwrap();
        assert_eq!(decode(&html[start..end]), png);
        assert!(html.contains(".submit()"));
    }
    #[test]
    fn lens_page_declares_the_language() {
        for language in Language::ALL {
            let html = page_in(b"png", 1, language);
            assert!(
                html.contains(&format!(r#"<html lang="{}">"#, language.tag())),
                "{}",
                language.tag()
            );
            for key in [Key::LensSending, Key::LensNeedsScript] {
                assert!(html.contains(&escape(tr_in(language, key))), "{}", key.id());
            }
        }
        let english = page_in(b"png", 1, Language::En);
        assert!(english.contains("<p>Sending the capture to Google Lens...</p>"));
        assert_eq!(escape(r#"a<b>&"c""#), "a&lt;b&gt;&amp;&quot;c&quot;");
    }
    #[test]
    fn google_page_opens_in_the_chosen_language() {
        for (language, hl) in [
            (Language::Fr, "fr"),
            (Language::PtBr, "pt-BR"),
            (Language::ZhHans, "zh-CN"),
            (Language::ZhHant, "zh-TW"),
        ] {
            let html = page_in(b"png", 7, language);
            assert!(
                html.contains(&format!("?ep=cntpubb&amp;hl={hl}&amp;st=7\"")),
                "{}",
                language.tag()
            );
        }
    }
    #[test]
    fn stale_pages_are_deleted_and_nothing_else() {
        let folder =
            std::env::temp_dir().join(format!("shotoshot-lens-test-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let page = folder.join("lens-1-2.html");
        let other = folder.join("notes.html");
        std::fs::write(&page, "x").unwrap();
        std::fs::write(&other, "x").unwrap();
        forget_stale(&folder, SystemTime::now());
        assert!(
            page.exists(),
            "a fresh page is still being read by the browser"
        );
        forget_stale(&folder, SystemTime::now() + PAGE_LIFETIME);
        assert!(!page.exists());
        assert!(other.exists());
        std::fs::remove_dir_all(&folder).unwrap();
    }
    #[test]
    fn the_forgetter_refuses_any_file_but_a_lens_page_of_ours() {
        for path in [
            folder().join("settings.txt"),
            std::env::temp_dir().join("lens-1-2.html"),
            PathBuf::from(r"C:\Windows\lens-1-2.html"),
        ] {
            assert!(forget_later(&path).is_err(), "{}", path.display());
        }
    }
    #[test]
    fn prepared_page_holds_the_capture_in_the_temp_folder() {
        let image = crate::Image::solid(3, 2, [0, 0, 255, 255]);
        let path = prepare(&image).unwrap();
        assert!(is_page(&path));
        assert_eq!(path.parent(), Some(folder().as_path()));
        let html = std::fs::read_to_string(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        let start = html.find("atob(\"").unwrap() + 6;
        let end = start + html[start..].find('"').unwrap();
        assert_eq!(&decode(&html[start..end])[..8], b"\x89PNG\r\n\x1a\n");
    }
}
