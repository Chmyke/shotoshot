//! ROLE: Shotoshot's own pointers: the new-text I-beam with a plus, beside the system I-beam
//! that says a click edits one; and the selection cross, black or white against what it covers.
//! OWNER: Pure pixels; each platform turns them into its own cursor, once per size and ink.
//! INVARIANTS: Every pixel is opaque or transparent, so no format premultiplies anything;
//! black strokes in a white outline read on any background, as the system pointers do. The
//! cross has one ink and no outline (Mickaël's choice on the 2026-10-22 board: C1).

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
    let ink = strokes(&STROKES, size);
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

/// The colour of the selection cross.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum CrossInk {
    #[default]
    Black,
    White,
}

/// Mean luminance (0 to 255) at or below which a black cross turns white, and at or above
/// which a white one turns black. Black and white contrast equally near 118; the gap between
/// the two keeps the cross from flickering along an edge.
const TO_WHITE: u8 = 110;
const TO_BLACK: u8 = 126;

impl CrossInk {
    /// The ink that reads over a mean `luminance`, starting from the one shown now.
    pub fn against(self, luminance: u8) -> Self {
        match self {
            Self::Black if luminance <= TO_WHITE => Self::White,
            Self::White if luminance >= TO_BLACK => Self::Black,
            ink => ink,
        }
    }
}

/// The arms of the system cross on its 32-unit grid, two units thick, 24 long.
const CROSS: [[f64; 4]; 2] = [[4., 15., 28., 17.], [15., 4., 17., 28.]];

/// Half the span of the cross arms, as a share of the cursor size: what the cross covers.
pub const CROSS_REACH: f64 = 12. / 32.;

/// The selection cross at `size` pixels square, the shape and hot spot of the system one.
pub fn cross(size: u32, ink: CrossInk) -> CursorImage {
    let size = size.max(16);
    let k = size as f64 / 32.;
    let rgb = match ink {
        CrossInk::Black => [0, 0, 0],
        CrossInk::White => [255, 255, 255],
    };
    let rgba = strokes(&CROSS, size)
        .into_iter()
        .flat_map(|inked| {
            if inked {
                [rgb[0], rgb[1], rgb[2], 255]
            } else {
                [0; 4]
            }
        })
        .collect();
    CursorImage {
        size,
        rgba,
        hotspot: ((16. * k).round() as u32, (16. * k).round() as u32),
    }
}

/// Which pixels of a `size` square the 32-unit `rects` cover, rows top down.
fn strokes(rects: &[[f64; 4]], size: u32) -> Vec<bool> {
    let k = size as f64 / 32.;
    let side = size as usize;
    let mut ink = vec![false; side * side];
    for &[x0, y0, x1, y1] in rects {
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
    ink
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

    #[test]
    fn the_cross_has_the_system_shape_in_one_ink_with_no_outline() {
        for size in [32, 48, 64, 96] {
            for (ink, rgb) in [(CrossInk::Black, [0, 0, 0]), (CrossInk::White, [255; 3])] {
                let image = cross(size, ink);
                let k = size as f64 / 32.;
                let at = |x: f64, y: f64| pixel(&image, (x * k) as u32, (y * k) as u32);
                let solid = [rgb[0], rgb[1], rgb[2], 255];
                let (hx, hy) = image.hotspot;
                assert_eq!(pixel(&image, hx, hy), solid, "{size} {ink:?}");
                // Arm ends, then beside an arm and past its end: nothing.
                for end in [at(4.5, 15.5), at(27.5, 16.5), at(15.5, 4.5), at(16.5, 27.5)] {
                    assert_eq!(end, solid, "{size} {ink:?}");
                }
                assert_eq!(at(10., 13.), [0; 4], "{size} {ink:?}");
                assert_eq!(at(16., 30.), [0; 4], "{size} {ink:?}");
                assert!(image.rgba.chunks(4).all(|p| p == [0; 4] || p == solid));
            }
        }
        assert_eq!(cross(32, CrossInk::Black).hotspot, (16, 16), "as IDC_CROSS");
    }

    #[test]
    fn the_cross_ink_flips_only_past_the_band_that_keeps_edges_steady() {
        use CrossInk::*;
        assert_eq!(Black.against(255), Black);
        assert_eq!(Black.against(TO_BLACK - 1), Black, "inside the band");
        assert_eq!(Black.against(TO_WHITE + 1), Black, "inside the band");
        assert_eq!(Black.against(TO_WHITE), White);
        assert_eq!(White.against(0), White);
        assert_eq!(White.against(TO_WHITE + 1), White, "inside the band");
        assert_eq!(White.against(TO_BLACK), Black);
        // A white page under the shade outside the region reads about 133: still black.
        assert_eq!(White.against(133), Black);
    }
}
