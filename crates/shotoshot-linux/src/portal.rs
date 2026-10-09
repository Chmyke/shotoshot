//! ROLE: Full-desktop screenshot through the XDG desktop portal (Wayland and X11 desktops).
//! INVARIANTS: Non-interactive request; the portal's temporary file is read then removed.
use crate::png_io;
use shotoshot_ui::Image;
use std::{collections::HashMap, error::Error};
use zbus::{
    blocking::{Connection, Proxy},
    zvariant::{OwnedObjectPath, OwnedValue, Value},
};

const DESTINATION: &str = "org.freedesktop.portal.Desktop";

pub fn screenshot() -> Result<Image, Box<dyn Error>> {
    let connection = Connection::session()?;
    let sender = connection
        .unique_name()
        .ok_or("no D-Bus name")?
        .trim_start_matches(':')
        .replace('.', "_");
    let token = format!("shotoshot_{}", std::process::id());
    // Subscribe before asking: the portal may answer before the call returns.
    let path = format!("/org/freedesktop/portal/desktop/request/{sender}/{token}");
    let request = Proxy::new(
        &connection,
        DESTINATION,
        path.as_str(),
        "org.freedesktop.portal.Request",
    )?;
    let mut responses = request.receive_signal("Response")?;
    let portal = Proxy::new(
        &connection,
        DESTINATION,
        "/org/freedesktop/portal/desktop",
        "org.freedesktop.portal.Screenshot",
    )?;
    let mut options: HashMap<&str, Value> = HashMap::new();
    options.insert("handle_token", Value::from(token.as_str()));
    options.insert("interactive", Value::from(false));
    let _: OwnedObjectPath = portal.call("Screenshot", &("", options))?;
    let message = responses.next().ok_or("the screenshot portal closed")?;
    let (code, results): (u32, HashMap<String, OwnedValue>) = message.body().deserialize()?;
    match code {
        0 => {}
        1 => return Err(Box::new(Cancelled)),
        _ => return Err("the desktop refused the screenshot".into()),
    }
    let uri: String = results
        .get("uri")
        .ok_or("the portal returned no file")?
        .try_clone()?
        .try_into()?;
    let path = file_path(&uri).ok_or("unexpected screenshot location")?;
    let image = png_io::load(&path);
    let _ = std::fs::remove_file(&path);
    image
}

/// The user dismissed the portal's screenshot dialog: an answer, not a failure.
#[derive(Debug)]
pub struct Cancelled;
impl std::fmt::Display for Cancelled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("screenshot cancelled")
    }
}
impl Error for Cancelled {}
pub fn cancelled(error: &(dyn Error + 'static)) -> bool {
    error.downcast_ref::<Cancelled>().is_some()
}

/// Local path of a `file://` URI, percent-decoded.
fn file_path(uri: &str) -> Option<std::path::PathBuf> {
    let encoded = uri.strip_prefix("file://")?;
    let mut bytes = Vec::with_capacity(encoded.len());
    let mut rest = encoded.as_bytes();
    while let Some((&b, tail)) = rest.split_first() {
        if b == b'%' && tail.len() >= 2 {
            let hex = std::str::from_utf8(&tail[..2]).ok()?;
            bytes.push(u8::from_str_radix(hex, 16).ok()?);
            rest = &tail[2..];
        } else {
            bytes.push(b);
            rest = tail;
        }
    }
    use std::os::unix::ffi::OsStringExt;
    Some(std::ffi::OsString::from_vec(bytes).into())
}

#[cfg(test)]
mod tests {
    #[test]
    fn file_uris_are_percent_decoded() {
        assert_eq!(
            super::file_path("file:///tmp/Screen%20shot%C3%A9.png").unwrap(),
            std::path::PathBuf::from("/tmp/Screen shoté.png")
        );
        assert!(super::file_path("https://example.com/a.png").is_none());
    }
}
