//! ROLE: Carries a payload in a PNG stream as one private chunk, and finds it again.
//! INVARIANTS: The chunk name is ancillary, private and unsafe to copy: viewers skip it and
//! an image editor that rewrites the pixels drops it. Nothing after `IEND` is ever kept.
use super::Error;

const SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";
const NAME: &[u8] = b"shOT";
const END: &[u8] = b"IEND";

const TABLE: [u32; 256] = {
    let mut table = [0; 256];
    let mut n = 0;
    while n < 256 {
        let mut c = n as u32;
        let mut bit = 0;
        while bit < 8 {
            c = if c & 1 == 0 {
                c >> 1
            } else {
                0xEDB8_8320 ^ (c >> 1)
            };
            bit += 1;
        }
        table[n] = c;
        n += 1;
    }
    table
};

/// CRC-32 of the concatenated `parts`, as PNG stores it after each chunk.
fn crc32(parts: &[&[u8]]) -> u32 {
    !parts.iter().copied().flatten().fold(!0, |c, byte| {
        TABLE[((c ^ *byte as u32) & 0xFF) as usize] ^ (c >> 8)
    })
}

/// Visits each chunk up to `IEND` included: offset, name, data and stored CRC.
/// `None` when the stream ends or overruns before `IEND`.
fn walk<'a>(file: &'a [u8], mut each: impl FnMut(usize, &'a [u8], &'a [u8], u32)) -> Option<()> {
    let mut at = SIGNATURE.len();
    loop {
        let head = file.get(at..at.checked_add(8)?)?;
        let length = u32::from_be_bytes(head[..4].try_into().ok()?) as usize;
        let data_end = at.checked_add(8)?.checked_add(length)?;
        let data = file.get(at + 8..data_end)?;
        let crc = file.get(data_end..data_end.checked_add(4)?)?;
        each(
            at,
            &head[4..],
            data,
            u32::from_be_bytes(crc.try_into().ok()?),
        );
        if &head[4..] == END {
            return Some(());
        }
        at = data_end + 4;
    }
}

/// `png` with `payload` in a chunk just before `IEND`. `None` unless `png` is a PNG stream
/// that does not already carry one.
pub(super) fn embed(png: &[u8], payload: &[u8]) -> Option<Vec<u8>> {
    if !png.starts_with(SIGNATURE) || payload.len() > i32::MAX as usize {
        return None;
    }
    let (mut end, mut carried) = (0..0, false);
    walk(png, |at, name, data, _| {
        carried |= name == NAME;
        end = at..at + 12 + data.len();
    })?;
    if carried {
        return None;
    }
    let mut file = Vec::with_capacity(end.end + 12 + payload.len());
    file.extend_from_slice(&png[..end.start]);
    file.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    file.extend_from_slice(NAME);
    file.extend_from_slice(payload);
    file.extend_from_slice(&crc32(&[NAME, payload]).to_be_bytes());
    file.extend_from_slice(&png[end]);
    Some(file)
}

/// The payload `embed` stored in `file`.
pub(super) fn extract(file: &[u8]) -> Result<&[u8], Error> {
    if !file.starts_with(SIGNATURE) {
        return Err(Error::NotEditable);
    }
    let (mut count, mut payload) = (0, None);
    walk(file, |_, name, data, crc| {
        if name == NAME {
            count += 1;
            payload = (crc32(&[name, data]) == crc).then_some(data);
        }
    })
    .ok_or(Error::Damaged)?;
    match (count, payload) {
        (0, _) => Err(Error::NotEditable),
        (1, Some(payload)) => Ok(payload),
        _ => Err(Error::Damaged),
    }
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;

    /// A valid 1×1 PNG: `IHDR`, `IDAT`, `IEND`.
    pub(in crate::shoto) const PIXEL: [u8; 70] = [
        137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 6,
        0, 0, 0, 31, 21, 196, 137, 0, 0, 0, 13, 73, 68, 65, 84, 120, 218, 99, 100, 96, 248, 95, 15,
        0, 2, 135, 1, 128, 235, 71, 186, 146, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
    ];

    #[test]
    fn crc_matches_the_png_reference_values() {
        assert_eq!(crc32(&[b"123456789"]), 0xCBF4_3926);
        assert_eq!(crc32(&[b"shOT", b"abc"]), 0xF0E2_5742);
        assert_eq!(crc32(&[b"IEND"]), 0xAE42_6082);
    }
    #[test]
    fn the_chunk_sits_before_iend_and_comes_back() {
        let file = embed(&PIXEL, b"abc").unwrap();
        assert_eq!(file[..58], PIXEL[..58]);
        assert_eq!(
            file[58..73],
            [
                0, 0, 0, 3, b's', b'h', b'O', b'T', b'a', b'b', b'c', 0xF0, 0xE2, 0x57, 0x42
            ]
        );
        assert_eq!(file[73..], PIXEL[58..]);
        assert_eq!(extract(&file), Ok(&b"abc"[..]));
    }
    #[test]
    fn bytes_after_iend_are_not_carried_over() {
        let mut trailing = PIXEL.to_vec();
        trailing.extend_from_slice(b"left behind by a cropped file");
        assert_eq!(embed(&trailing, b"abc"), embed(&PIXEL, b"abc"));
    }
    #[test]
    fn only_a_png_without_the_chunk_receives_one() {
        let file = embed(&PIXEL, b"abc").unwrap();
        assert_eq!(embed(&file, b"again"), None);
        assert_eq!(embed(b"not a png", b"abc"), None);
        assert_eq!(embed(&PIXEL[..60], b"abc"), None);
    }
    #[test]
    fn extraction_names_what_is_wrong() {
        assert_eq!(extract(b"not a png"), Err(Error::NotEditable));
        assert_eq!(extract(&PIXEL), Err(Error::NotEditable));
        let file = embed(&PIXEL, b"abc").unwrap();
        let mut altered = file.clone();
        altered[67] ^= 1;
        assert_eq!(extract(&altered), Err(Error::Damaged));
        assert_eq!(extract(&file[..file.len() - 1]), Err(Error::Damaged));
        let mut twice = file[..73].to_vec();
        twice.extend_from_slice(&file[58..]);
        assert_eq!(extract(&twice), Err(Error::Damaged));
    }
}
