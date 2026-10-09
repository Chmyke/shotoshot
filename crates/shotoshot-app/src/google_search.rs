//! Explicit image search only: annotated crop -> Google Lens -> default browser.
//! No clipboard mutation, temporary image file, third-party host or idle network activity.
use windows::{
    Win32::{Foundation::*, Networking::WinHttp::*},
    core::*,
};
const BOUNDARY: &str = "Shotoshot-Lens-7bf973acd8024d91";
struct Handle(*mut std::ffi::c_void);
impl Handle {
    fn new(value: *mut std::ffi::c_void) -> Result<Self> {
        if value.is_null() {
            Err(Error::from_thread())
        } else {
            Ok(Self(value))
        }
    }
}
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            let _ = WinHttpCloseHandle(self.0);
        }
    }
}
fn multipart(png: &[u8]) -> Vec<u8> {
    let mut body = format!("--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"encoded_image\"; filename=\"capture.png\"\r\nContent-Type: image/png\r\n\r\n").into_bytes();
    body.extend_from_slice(png);
    body.extend_from_slice(format!("\r\n--{BOUNDARY}--\r\n").as_bytes());
    body
}
fn valid_result(url: &str) -> bool {
    (url.starts_with("https://www.google.com/search?")
        || url.starts_with("https://lens.google.com/"))
        && !url.chars().any(char::is_control)
}
pub(crate) fn upload(image: &crate::Image) -> Result<String> {
    let _com = crate::ComApartment::new()?;
    let png = crate::png_bytes(image)?;
    let body = multipart(&png);
    unsafe {
        let session = Handle::new(WinHttpOpen(
            w!("Shotoshot/0.2"),
            WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
            PCWSTR::null(),
            PCWSTR::null(),
            0,
        ))?;
        WinHttpSetTimeouts(session.0, 10000, 10000, 20000, 20000)?;
        let connection = Handle::new(WinHttpConnect(session.0, w!("lens.google.com"), 443, 0))?;
        let request = Handle::new(WinHttpOpenRequest(
            connection.0,
            w!("POST"),
            w!("/v3/upload?ep=cntpubb&hl=en"),
            PCWSTR::null(),
            PCWSTR::null(),
            std::ptr::null(),
            WINHTTP_FLAG_SECURE,
        ))?;
        WinHttpSetOption(
            Some(request.0),
            WINHTTP_OPTION_DISABLE_FEATURE,
            Some(&(WINHTTP_DISABLE_REDIRECTS | WINHTTP_DISABLE_COOKIES).to_ne_bytes()),
        )?;
        // Request-local compatibility cookie; no browser cookie jar is read, changed or persisted.
        let headers: Vec<u16> = format!(
            "Content-Type: multipart/form-data; boundary={BOUNDARY}\r\nCookie: SOCS=CAI\r\n"
        )
        .encode_utf16()
        .collect();
        WinHttpSendRequest(
            request.0,
            Some(&headers),
            Some(body.as_ptr().cast()),
            body.len() as u32,
            body.len() as u32,
            0,
        )?;
        WinHttpReceiveResponse(request.0, std::ptr::null_mut())?;
        let mut status = 0u32;
        let mut size = 4;
        WinHttpQueryHeaders(
            request.0,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            PCWSTR::null(),
            Some((&mut status as *mut u32).cast()),
            &mut size,
            std::ptr::null_mut(),
        )?;
        if ![301, 302, 303, 307, 308].contains(&status) {
            return Err(Error::new(
                E_FAIL,
                format!(
                    "Google Lens did not accept the image (HTTP {status}). Your capture is preserved."
                ),
            ));
        }
        let mut buffer = [0u16; 8192];
        size = std::mem::size_of_val(&buffer) as u32;
        WinHttpQueryHeaders(
            request.0,
            WINHTTP_QUERY_LOCATION,
            PCWSTR::null(),
            Some(buffer.as_mut_ptr().cast()),
            &mut size,
            std::ptr::null_mut(),
        )?;
        let end = buffer.iter().position(|c| *c == 0).unwrap_or(buffer.len());
        let url = String::from_utf16(&buffer[..end])
            .map_err(|_| Error::new(E_FAIL, "Invalid Google Lens link"))?;
        if !valid_result(&url) {
            return Err(Error::new(
                E_FAIL,
                format!(
                    "Google Lens returned an unexpected link: {}",
                    url.split('?').next().unwrap_or("")
                ),
            ));
        }
        Ok(url)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn multipart_preserves_binary_image_and_uses_google_file_field() {
        let png = [0, 255, 13, 10, 128];
        let body = multipart(&png);
        assert!(body.windows(png.len()).any(|w| w == png));
        assert!(body.starts_with(format!("--{BOUNDARY}\r\n").as_bytes()));
        assert!(body.ends_with(format!("\r\n--{BOUNDARY}--\r\n").as_bytes()));
        assert!(String::from_utf8_lossy(&body).contains("name=\"encoded_image\""));
    }
    #[test]
    fn only_google_https_result_links_are_opened() {
        assert!(valid_result("https://www.google.com/search?x=1"));
        for url in [
            "https://www.google.com.evil.test/search?",
            "file:///test",
            "http://lens.google.com/",
            "https://lens.google.com/\n",
        ] {
            assert!(!valid_result(url));
        }
    }
    #[test]
    fn png_encoding_stays_in_memory() {
        let _com = crate::ComApartment::new().unwrap();
        let png = crate::png_bytes(&crate::Image::solid(3, 2, [0, 0, 255, 255])).unwrap();
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    }
}
