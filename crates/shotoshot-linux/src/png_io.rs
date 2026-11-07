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
    let (width, height) = announced;
    let mut image = Image {
        width,
        height,
        pixels: vec![0; width as usize * height as usize * 4],
        left: 0,
        top: 0,
    };
    // The rows are decoded at the end of the pixel buffer, then spread forward in place:
    // with one to four channels, a pixel is written at or before where it is read, and the
    // pixels still to read lie further on. RGBA decodes exactly in place.
    let needed = reader.output_buffer_size().ok_or("PNG too large")?;
    if needed > image.pixels.len() {
        return Err("PNG rows larger than its pixels".into());
    }
    let start = image.pixels.len() - needed;
    let info = reader.next_frame(&mut image.pixels[start..])?;
    let channels = match info.color_type {
        png::ColorType::Rgba => 4,
        png::ColorType::Rgb => 3,
        png::ColorType::GrayscaleAlpha => 2,
        png::ColorType::Grayscale => 1,
        png::ColorType::Indexed => return Err("unexpanded palette PNG".into()),
    };
    let (row, width) = (info.line_size, width as usize);
    for y in 0..height as usize {
        for x in 0..width {
            let from = start + y * row + x * channels;
            let [r, g, b] = match channels {
                3 | 4 => [
                    image.pixels[from],
                    image.pixels[from + 1],
                    image.pixels[from + 2],
                ],
                _ => [image.pixels[from]; 3],
            };
            let to = (y * width + x) * 4;
            image.pixels[to..to + 4].copy_from_slice(&[b, g, r, 255]);
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
    // Fast PNG (Mickael, 2026-11-07): files a little larger, saved several times faster.
    encoder.set_compression(png::Compression::Fast);
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
    /// A PNG of `color` with `channels` per pixel, each sample from `samples`, as the
    /// portal or another program may write one.
    fn foreign_png(color: png::ColorType, channels: usize, samples: &[u8]) -> Vec<u8> {
        let mut bytes = Vec::new();
        let mut encoder = png::Encoder::new(&mut bytes, 3, 2);
        encoder.set_color(color);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        let data: Vec<u8> = (0..6 * channels)
            .map(|i| samples[i % samples.len()])
            .collect();
        writer.write_image_data(&data).unwrap();
        writer.finish().unwrap();
        bytes
    }
    #[test]
    fn pngs_of_every_channel_count_decode_to_opaque_bgra() {
        let rgba = foreign_png(png::ColorType::Rgba, 4, &[200, 100, 50, 7]);
        let image = decode(&rgba, (3, 2)).unwrap();
        assert_eq!(image.pixels[..8], [50, 100, 200, 255, 50, 100, 200, 255]);
        let rgb = foreign_png(png::ColorType::Rgb, 3, &[1, 2, 3]);
        assert!(
            decode(&rgb, (3, 2))
                .unwrap()
                .pixels
                .as_chunks::<4>()
                .0
                .iter()
                .all(|p| *p == [3, 2, 1, 255])
        );
        let gray = foreign_png(png::ColorType::Grayscale, 1, &[9]);
        assert!(
            decode(&gray, (3, 2))
                .unwrap()
                .pixels
                .as_chunks::<4>()
                .0
                .iter()
                .all(|p| *p == [9, 9, 9, 255])
        );
        let gray_alpha = foreign_png(png::ColorType::GrayscaleAlpha, 2, &[40, 0]);
        assert!(
            decode(&gray_alpha, (3, 2))
                .unwrap()
                .pixels
                .as_chunks::<4>()
                .0
                .iter()
                .all(|p| *p == [40, 40, 40, 255])
        );
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
