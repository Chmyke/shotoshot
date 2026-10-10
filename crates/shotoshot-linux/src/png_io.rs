//! ROLE: PNG files and PNG bytes to and from the portable BGRA `Image`.
use crate::files;
use shotoshot_ui::Image;
use std::{
    error::Error,
    fs::File,
    io::{BufRead, BufReader, Cursor, Seek},
    path::Path,
};

pub fn load(path: &Path) -> Result<Image, Box<dyn Error>> {
    read(BufReader::new(File::open(path)?), None)
}

/// The pixels of an in-memory PNG that announces exactly `size`: the frame of a document.
/// Nothing is allocated for an image of another size.
pub fn decode(bytes: &[u8], size: (u32, u32)) -> Result<Image, Box<dyn Error>> {
    read(Cursor::new(bytes), Some(size))
}

fn read(
    source: impl BufRead + Seek,
    expected: Option<(u32, u32)>,
) -> Result<Image, Box<dyn Error>> {
    let mut decoder = png::Decoder::new(source);
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info()?;
    let announced = (reader.info().width, reader.info().height);
    if expected.is_some_and(|size| size != announced) {
        return Err("PNG of an unexpected size".into());
    }
    let mut buffer = vec![0; reader.output_buffer_size().ok_or("PNG too large")?];
    let info = reader.next_frame(&mut buffer)?;
    let channels = match info.color_type {
        png::ColorType::Rgba => 4,
        png::ColorType::Rgb => 3,
        png::ColorType::GrayscaleAlpha => 2,
        png::ColorType::Grayscale => 1,
        png::ColorType::Indexed => return Err("unexpanded palette PNG".into()),
    };
    let mut image = Image::solid(info.width, info.height, [0, 0, 0, 255]);
    let row = info.line_size;
    for y in 0..info.height as usize {
        for x in 0..info.width as usize {
            let p = &buffer[y * row + x * channels..];
            let (r, g, b) = if channels >= 3 {
                (p[0], p[1], p[2])
            } else {
                (p[0], p[0], p[0])
            };
            let i = (y * info.width as usize + x) * 4;
            image.pixels[i..i + 4].copy_from_slice(&[b, g, r, 255]);
        }
    }
    Ok(image)
}

/// Straight RGBA pixels of an in-memory PNG (icons).
pub fn decode_rgba(bytes: &[u8]) -> Result<(u32, u32, Vec<u8>), Box<dyn Error>> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info()?;
    let mut buffer = vec![0; reader.output_buffer_size().ok_or("PNG too large")?];
    let info = reader.next_frame(&mut buffer)?;
    if info.color_type != png::ColorType::Rgba {
        return Err("icon PNG must be RGBA".into());
    }
    buffer.truncate(info.buffer_size());
    Ok((info.width, info.height, buffer))
}

/// `image` as the bytes of an opaque PNG file.
pub fn encode(image: &Image) -> Result<Vec<u8>, Box<dyn Error>> {
    image.check()?;
    let mut bytes = Vec::new();
    let mut encoder = png::Encoder::new(&mut bytes, image.width, image.height);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header()?;
    let rgb: Vec<u8> = image
        .pixels
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|p| [p[2], p[1], p[0]])
        .collect();
    writer.write_image_data(&rgb)?;
    writer.finish()?;
    Ok(bytes)
}

/// Writes `image` at `path`, which is replaced only by the complete file.
pub fn save(image: &Image, path: &Path) -> Result<(), Box<dyn Error>> {
    Ok(files::replace(&encode(image)?, path)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn png_roundtrip_keeps_bgra_pixels() {
        let mut image = Image::solid(3, 2, [10, 20, 30, 255]);
        image.pixels[4..8].copy_from_slice(&[200, 100, 50, 255]);
        let path = std::env::temp_dir().join(format!("shotoshot-png-{}.png", std::process::id()));
        save(&image, &path).unwrap();
        let back = load(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert_eq!(back.pixels, image.pixels);
    }
    #[test]
    fn an_in_memory_png_is_decoded_only_at_its_announced_size() {
        let mut image = Image::solid(3, 2, [10, 20, 30, 255]);
        image.pixels[4..8].copy_from_slice(&[200, 100, 50, 255]);
        let bytes = encode(&image).unwrap();
        assert_eq!(decode(&bytes, (3, 2)).unwrap().pixels, image.pixels);
        assert!(decode(&bytes, (2, 3)).is_err());
        assert!(decode(&bytes[..bytes.len() - 20], (3, 2)).is_err());
        assert!(decode(b"GIF89a", (3, 2)).is_err());
    }
}
