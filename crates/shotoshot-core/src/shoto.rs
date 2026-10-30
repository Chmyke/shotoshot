//! ROLE: The `.shoto` file: a PNG of the annotated capture whose private `shOT` chunk
//! carries the scene and the untouched frame it was drawn on, so it reopens editable.
//! OWNER: Each platform encodes and decodes the two PNG images; this module moves bytes.
//! INVARIANTS: Reading never panics, never loads part of a file and bounds every size before
//! allocating. A grammar change takes a new version; every earlier version stays readable.
mod png;
mod scene;
pub use scene::Scene;

const HEADER: &[u8] = b"shotoshot-document ";
/// The latest version read; a file is written in the lowest one that holds it (`Scene::version`).
pub(crate) const VERSION: u32 = 5;
const FRAME: &[u8] = b"frame-png ";
/// Larger files are refused unread: far above any desktop saved as two PNG images.
pub const MAX_FILE: u64 = 256 << 20;

/// Why a file does not open for editing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// Not a PNG, or a PNG without Shotoshot data.
    NotEditable,
    /// Written by a later version of the format.
    Newer,
    /// Truncated, altered or inconsistent.
    Damaged,
}
impl Error {
    /// The text the user reads.
    pub fn key(self) -> shotoshot_text::Key {
        match self {
            Self::NotEditable => shotoshot_text::Key::DocumentNotEditable,
            Self::Newer => shotoshot_text::Key::DocumentNewer,
            Self::Damaged => shotoshot_text::Key::DocumentDamaged,
        }
    }
    /// That text in the interface language.
    pub fn message(self) -> &'static str {
        self.message_in(shotoshot_text::language())
    }
    /// That text in `language`.
    pub fn message_in(self, language: shotoshot_text::Language) -> &'static str {
        shotoshot_text::tr_in(language, self.key())
    }
}

/// The file for `scene`: `visible` is the flattened capture and `frame` the screen under it,
/// both already PNG. `None` if `unpack` would refuse the result.
pub fn pack(visible: &[u8], scene: &Scene, frame: &[u8]) -> Option<Vec<u8>> {
    if !scene.valid() {
        return None;
    }
    let version = scene.version();
    let mut payload = format!("shotoshot-document {version}\n{}", scene.encode()).into_bytes();
    payload.extend_from_slice(format!("frame-png {}\n", frame.len()).as_bytes());
    payload.extend_from_slice(frame);
    png::embed(visible, &payload).filter(|file| file.len() as u64 <= MAX_FILE)
}

/// The scene of a file and the PNG bytes of its frame.
pub fn unpack(file: &[u8]) -> Result<(Scene, &[u8]), Error> {
    if file.len() as u64 > MAX_FILE {
        return Err(Error::Damaged);
    }
    let payload = png::extract(file)?;
    let (header, mut rest) = line(payload).ok_or(Error::Damaged)?;
    let version = header
        .strip_prefix(HEADER)
        .and_then(|v| std::str::from_utf8(v).ok()?.parse::<u32>().ok())
        .filter(|v| *v != 0)
        .ok_or(Error::Damaged)?;
    if version > VERSION {
        return Err(Error::Newer);
    }
    let body = rest;
    loop {
        let (next, after) = line(rest).ok_or(Error::Damaged)?;
        if let Some(length) = next.strip_prefix(FRAME) {
            let text = &body[..body.len() - rest.len()];
            let scene = std::str::from_utf8(length)
                .ok()
                .and_then(|n| n.parse::<usize>().ok())
                .filter(|n| *n == after.len())
                .and_then(|_| scene::decode(std::str::from_utf8(text).ok()?, version));
            return scene.map(|scene| (scene, after)).ok_or(Error::Damaged);
        }
        rest = after;
    }
}

/// The first line of `bytes` without its line feed, and what follows it.
fn line(bytes: &[u8]) -> Option<(&[u8], &[u8])> {
    let end = bytes.iter().position(|b| *b == b'\n')?;
    Some((&bytes[..end], &bytes[end + 1..]))
}

#[cfg(test)]
mod tests {
    use super::{png::tests::PIXEL, scene::tests::sample, *};

    // Any bytes: the reader must not look for its own markers inside the frame.
    const FRAME_IMAGE: &[u8] = b"\x89PNG stand-in\nframe-png 3\nshotoshot-document 9\n\0\xFF";

    #[test]
    fn refusals_are_keys() {
        use shotoshot_text::{Key, Language, tr_in};
        for (error, key) in [
            (Error::NotEditable, Key::DocumentNotEditable),
            (Error::Newer, Key::DocumentNewer),
            (Error::Damaged, Key::DocumentDamaged),
        ] {
            assert_eq!(error.key(), key);
            assert_eq!(error.message_in(Language::Fr), tr_in(Language::Fr, key));
        }
    }

    #[test]
    fn a_packed_file_is_a_png_that_gives_its_scene_and_frame_back() {
        let file = pack(&PIXEL, &sample(), FRAME_IMAGE).unwrap();
        assert!(file.starts_with(&PIXEL[..58]) && file.ends_with(&PIXEL[58..]));
        assert_eq!(unpack(&file), Ok((sample(), FRAME_IMAGE)));
    }
    #[test]
    fn a_file_takes_the_lowest_version_that_holds_its_scene() {
        let file = pack(&PIXEL, &sample(), FRAME_IMAGE).unwrap();
        let payload = png::extract(&file).unwrap();
        assert!(
            payload.starts_with(b"shotoshot-document 1\n"),
            "0.3.6 still opens it"
        );
        let mut layered = sample();
        layered.background[0].walls.push(1);
        let file = pack(&PIXEL, &layered, FRAME_IMAGE).unwrap();
        let payload = png::extract(&file).unwrap();
        assert!(payload.starts_with(b"shotoshot-document 2\n"));
        assert_eq!(unpack(&file), Ok((layered, FRAME_IMAGE)));
        let mut labelled = sample();
        if let crate::Shape::Text { label, .. } = &mut labelled.objects[6].shape {
            *label = Some(crate::Label::Fit);
        }
        let file = pack(&PIXEL, &labelled, FRAME_IMAGE).unwrap();
        let payload = png::extract(&file).unwrap();
        assert!(payload.starts_with(b"shotoshot-document 3\n"));
        assert_eq!(unpack(&file), Ok((labelled.clone(), FRAME_IMAGE)));
        if let crate::Shape::Text { ink, .. } = &mut labelled.objects[6].shape {
            *ink = Some([1, 2, 3]);
        }
        let file = pack(&PIXEL, &labelled, FRAME_IMAGE).unwrap();
        let payload = png::extract(&file).unwrap();
        assert!(payload.starts_with(b"shotoshot-document 5\n"));
        assert_eq!(unpack(&file), Ok((labelled, FRAME_IMAGE)));
    }
    #[test]
    fn what_cannot_be_read_back_is_not_written() {
        let mut scene = sample();
        scene.objects[0].rotation = f64::NAN;
        assert_eq!(pack(&PIXEL, &scene, FRAME_IMAGE), None);
        assert_eq!(pack(b"not a png", &sample(), FRAME_IMAGE), None);
    }
    #[test]
    fn each_refusal_has_its_reason() {
        let with = |payload: &str| png::embed(&PIXEL, payload.as_bytes()).unwrap();
        let scene = sample().encode();
        assert_eq!(unpack(&PIXEL), Err(Error::NotEditable));
        assert_eq!(unpack(b"plain text"), Err(Error::NotEditable));
        assert_eq!(
            unpack(&with(&format!(
                "shotoshot-document {}\nanything at all\n",
                VERSION + 1
            ))),
            Err(Error::Newer)
        );
        for payload in [
            String::new(),
            "shotoshot-document 1".into(),
            format!("shotoshot-document 0\n{scene}frame-png 0\n"),
            format!("shotoshot-document one\n{scene}frame-png 0\n"),
            format!("another-document 1\n{scene}frame-png 0\n"),
            format!("shotoshot-document 1\n{scene}"),
            format!("shotoshot-document 1\n{scene}frame-png 4\nabc"),
            format!("shotoshot-document 1\n{scene}frame-png 2\nabc"),
            format!("shotoshot-document 1\n{scene}frame-png -3\nabc"),
            format!("shotoshot-document 1\n{scene}note\nframe-png 3\nabc"),
        ] {
            assert_eq!(unpack(&with(&payload)), Err(Error::Damaged), "{payload:?}");
        }
        assert_eq!(
            unpack(&with(&format!(
                "shotoshot-document 1\n{scene}frame-png 3\nabc"
            ))),
            Ok((sample(), &b"abc"[..]))
        );
    }
    #[test]
    fn no_truncated_or_altered_file_stops_the_reader() {
        let file = pack(&PIXEL, &sample(), FRAME_IMAGE).unwrap();
        for end in 0..file.len() {
            assert!(unpack(&file[..end]).is_err(), "prefix of {end} bytes");
        }
        let mut altered = file.clone();
        for at in 0..file.len() {
            for mask in [0x01, 0x80, 0xFF] {
                altered[at] = file[at] ^ mask;
                // Only the visible image around the chunk may change unnoticed.
                if unpack(&altered).is_ok() {
                    assert!(
                        !(58..file.len() - 12).contains(&at),
                        "byte {at} ^ {mask:#x}"
                    );
                }
            }
            altered[at] = file[at];
        }
    }
}
