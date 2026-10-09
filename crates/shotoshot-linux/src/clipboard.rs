//! ROLE: Clipboard on Linux. X11 and Wayland keep a copy only while its owner runs, so a
//! detached helper (`shotoshot --serve-clipboard`) owns it until another program copies.
//! INVARIANTS: Content reaches the helper through its stdin; nothing is written to disk.
use arboard::{Clipboard, ImageData, SetExtLinux};
use shotoshot_ui::Image;
use std::{
    borrow::Cow,
    error::Error,
    io::{Read, Write},
    os::unix::process::CommandExt,
    process::{Child, Command, Stdio},
};

pub const SERVE: &str = "--serve-clipboard";
const IMAGE: u8 = b'I';
const TEXT: u8 = b'T';

/// Starts a helper that owns the clipboard with this image; returns once it has the data.
pub fn copy_image(image: &Image) -> Result<Child, Box<dyn Error>> {
    let mut payload = Vec::with_capacity(9 + image.pixels.len());
    payload.push(IMAGE);
    payload.extend_from_slice(&image.width.to_le_bytes());
    payload.extend_from_slice(&image.height.to_le_bytes());
    // BGRA to RGBA, as the clipboard expects.
    payload.extend(
        image
            .pixels
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|p| [p[2], p[1], p[0], 255]),
    );
    spawn(&payload)
}
pub fn copy_text(text: &str) -> Result<Child, Box<dyn Error>> {
    let mut payload = vec![TEXT];
    payload.extend_from_slice(text.as_bytes());
    spawn(&payload)
}
pub fn paste_text() -> Option<String> {
    Clipboard::new().ok()?.get_text().ok()
}

fn spawn(payload: &[u8]) -> Result<Child, Box<dyn Error>> {
    let mut child = Command::new(std::env::current_exe()?)
        .arg(SERVE)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        // Own process group: closing the launching terminal must not drop the copy.
        .process_group(0)
        .spawn()?;
    let mut stdin = child.stdin.take().ok_or("no helper input")?;
    stdin.write_all(payload)?;
    Ok(child)
}

/// Helper entry point: reads the payload, then serves it until the clipboard changes owner.
pub fn serve() -> Result<(), Box<dyn Error>> {
    let mut payload = Vec::new();
    std::io::stdin().read_to_end(&mut payload)?;
    let (&kind, body) = payload.split_first().ok_or("empty clipboard payload")?;
    let mut clipboard = Clipboard::new()?;
    match kind {
        IMAGE => {
            let (size, bytes) = body.split_at_checked(8).ok_or("short image payload")?;
            let width = u32::from_le_bytes(size[..4].try_into()?) as usize;
            let height = u32::from_le_bytes(size[4..].try_into()?) as usize;
            if bytes.len() != width * height * 4 {
                return Err("image payload length".into());
            }
            clipboard.set().wait().image(ImageData {
                width,
                height,
                bytes: Cow::Borrowed(bytes),
            })?;
        }
        TEXT => clipboard
            .set()
            .wait()
            .text(String::from_utf8(body.to_vec())?)?,
        _ => return Err("unknown clipboard payload".into()),
    }
    Ok(())
}

/// Size of the image currently on the clipboard (diagnostics).
pub fn image_size() -> Result<(usize, usize), Box<dyn Error>> {
    let image = Clipboard::new()?.get_image()?;
    Ok((image.width, image.height))
}
