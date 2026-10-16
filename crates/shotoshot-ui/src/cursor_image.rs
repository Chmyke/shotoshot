//! ROLE: The pointer saying that a click starts a new text: an I-beam with a plus, beside the
//! system I-beam that says a click edits one.
//! OWNER: Pure pixels; each platform turns them into its own cursor, once per size.
//! INVARIANTS: Every pixel is opaque or transparent, so no format premultiplies anything;
//! black strokes in a white outline read on any background, as the system pointers do.

/// Straight RGBA pixels of a square cursor, rows top down, and its hot spot.
pub struct CursorImage {
    pub size: u32,
    pub rgba: Vec<u8>,
    pub hotspot: (u32, u32),
}

/// Black strokes on a 32-unit grid, one unit thick like the system I-beam: the I-beam,
/// then the plus at its lower right.
const STROKES: [[f64; 4]; 5] = [
    // Stem, then the bars that close it top and bottom.
    [11., 7., 12., 25.],
    [8., 6., 15., 7.],
    [8., 25., 15., 26.],
    // Plus.
    [17., 21., 24., 22.],
    [20., 18., 21., 25.],
];

/// The new-text pointer at `size` pixels square (the system's cursor size at the screen's
/// scale); the hot spot is the middle of the I-beam, as on the system one.
pub fn new_text(size: u32) -> CursorImage {
    let size = size.max(16);
    let k = size as f64 / 32.;
    let side = size as usize;
    let mut ink = vec![false; side * side];
    for [x0, y0, x1, y1] in STROKES {
        // Whole pixels, never thinner than one, so the strokes stay crisp at every size.
        let span = |a: f64, b: f64| {
            let start = (a * k).round() as usize;
            start..((b * k).round() as usize).max(start + 1).min(side)
        };
        for y in span(y0, y1) {
            for x in span(x0, x1) {
                ink[y * side + x] = true;
            }
        }
    }
    let outline = k.round().max(1.) as usize;
    let near_ink = |x: usize, y: usize| {
        let (ys, xs) = (y.saturating_sub(outline), x.saturating_sub(outline));
        (ys..=(y + outline).min(side - 1))
            .any(|v| (xs..=(x + outline).min(side - 1)).any(|u| ink[v * side + u]))
    };
    let mut rgba = Vec::with_capacity(side * side * 4);
    for y in 0..side {
        for x in 0..side {
            rgba.extend_from_slice(match () {
                _ if ink[y * side + x] => &[0, 0, 0, 255],
                _ if near_ink(x, y) => &[255, 255, 255, 255],
                _ => &[0, 0, 0, 0],
            });
        }
    }
    CursorImage {
        size,
        rgba,
        hotspot: ((11. * k).round() as u32, (16. * k).round() as u32),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pixel(image: &CursorImage, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * image.size + x) * 4) as usize;
        image.rgba[i..i + 4].try_into().unwrap()
    }

    #[test]
    fn an_i_beam_with_a_plus_outlined_on_transparency() {
        for size in [32, 48, 64, 96] {
            let image = new_text(size);
            let k = size as f64 / 32.;
            let at = |x: f64, y: f64| pixel(&image, (x * k) as u32, (y * k) as u32);
            assert_eq!(image.rgba.len(), (size * size * 4) as usize);
            // The hot spot is on the stem.
            let (hx, hy) = image.hotspot;
            assert_eq!(pixel(&image, hx, hy), [0, 0, 0, 255], "{size}");
            // Bars, plus centre, white outline beside the stem, nothing in the corners.
            assert_eq!(at(8.5, 6.5), [0, 0, 0, 255], "{size}");
            assert_eq!(at(20.5, 21.5), [0, 0, 0, 255], "{size}");
            assert_eq!(at(10.5, 16.), [255, 255, 255, 255], "{size}");
            assert_eq!(at(1., 1.), [0, 0, 0, 0], "{size}");
            assert_eq!(at(30., 2.), [0, 0, 0, 0], "{size}");
            // Only opaque or transparent pixels.
            assert!(image.rgba.chunks(4).all(|p| p[3] == 0 || p[3] == 255));
        }
    }
}
