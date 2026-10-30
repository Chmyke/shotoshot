//! ROLE: What the blur tool makes of the capture under it: every pixel takes the colour of a
//! random pixel at most `radius` away in each axis, then the whole is smoothed by three box
//! passes (close to a Gaussian of 0.43 × radius). Computed here, on the CPU, so the Windows
//! and Linux renderers and every export show the same pixels.
//! OWNER: Pure functions over a top-down 4-byte-per-pixel frame; channels are treated alike,
//! so BGRA and RGBA give the same result.
//! INVARIANTS: Deterministic: the same zone of the same pixels, radius and seed always give the
//! same patch, whatever its place in the frame. A patch reads only the zone it covers, clamped
//! at its edges, and is opaque.
use crate::{Object, Rect, Shape};

/// Shuffle radius of a new blur at 100 %, in pixels: text of about 20 px becomes unreadable
/// (`tests/blur.rs`), the board of real renders validated by Mickaël on 2026-10-30.
pub const BLUR_RADIUS: f64 = 7.;
/// Smallest and largest radius the format accepts; the wheel's ladder stays inside.
pub const MIN_BLUR_RADIUS: f64 = 1.;
pub const MAX_BLUR_RADIUS: f64 = 64.;

/// Whole pixels of the frame, `width` × `height` from (`x`, `y`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Zone {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}
impl Zone {
    /// The pixels whose centres lie in `r`, within a frame of `width` × `height`; `None`
    /// when that is empty.
    pub fn of(r: Rect, width: u32, height: u32) -> Option<Self> {
        let span = |a: f64, b: f64, limit: u32| {
            let lo = a.min(b).round().clamp(0., limit as f64) as u32;
            let hi = a.max(b).round().clamp(0., limit as f64) as u32;
            (hi > lo).then_some((lo, hi - lo))
        };
        if !(r.min.finite() && r.max.finite()) {
            return None;
        }
        let (x, width) = span(r.min.x, r.max.x, width)?;
        let (y, height) = span(r.min.y, r.max.y, height)?;
        Some(Self {
            x,
            y,
            width,
            height,
        })
    }
    fn len(self) -> usize {
        self.width as usize * self.height as usize * 4
    }
}

/// One blur over a frame: where, how strong, and the seed of its shuffle.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BlurPatch {
    /// The object's id: it seeds the shuffle, so a blur never flickers between frames.
    pub id: u64,
    pub zone: Zone,
    /// Shuffle radius in whole pixels, at least 1.
    pub radius: u32,
}
impl BlurPatch {
    /// The blur `object` lays over a frame of `width` × `height`, if it is one and covers a
    /// pixel of it.
    pub fn of(object: &Object, width: u32, height: u32) -> Option<Self> {
        let Shape::Blur(r) = object.shape else {
            return None;
        };
        Some(Self {
            id: object.id,
            zone: Zone::of(r, width, height)?,
            radius: whole_radius(object.style.width),
        })
    }
    /// Every blur of `objects` over a frame of `width` × `height`, bottom first.
    pub fn all(objects: &[Object], width: u32, height: u32) -> Vec<Self> {
        objects
            .iter()
            .filter_map(|o| Self::of(o, width, height))
            .collect()
    }
    /// Changes whenever the patch's pixels would, the frame aside: a cache key.
    pub fn version(&self) -> u64 {
        let Zone {
            x,
            y,
            width,
            height,
        } = self.zone;
        [x, y, width, height, self.radius]
            .into_iter()
            .fold(mix(self.id), |hash, n| mix(hash ^ u64::from(n)))
    }
    /// The patch's opaque pixels, top-down, from the frame `pixels` `width` wide.
    pub fn pixels(&self, pixels: &[u8], width: u32) -> Vec<u8> {
        patch(pixels, width, self.zone, self.radius, self.id)
    }
}

/// A radius as the effect uses it: whole pixels within the accepted range.
fn whole_radius(radius: f64) -> u32 {
    if radius.is_finite() {
        radius.round().clamp(MIN_BLUR_RADIUS, MAX_BLUR_RADIUS) as u32
    } else {
        BLUR_RADIUS as u32
    }
}

/// splitmix64's finaliser: a well-spread hash of `n`.
fn mix(n: u64) -> u64 {
    let mut z = n.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Half-width of each of the three box passes for a shuffle `radius`: their sum is close to a
/// Gaussian blur of 0.43 × radius, the board's choice C (shuffle 7, blur 3).
pub fn box_radius(radius: u32) -> usize {
    let sigma = 0.43 * radius as f64;
    (((4. * sigma * sigma + 1.).sqrt() - 1.) / 2.)
        .round()
        .max(1.) as usize
}

/// The zone of the frame `pixels` (`width` wide) shuffled within `radius` and smoothed, as an
/// opaque top-down patch of `zone.width` × `zone.height` pixels.
pub fn patch(pixels: &[u8], width: u32, zone: Zone, radius: u32, seed: u64) -> Vec<u8> {
    let threads = if zone.len() < PARALLEL_FROM {
        1
    } else {
        std::thread::available_parallelism().map_or(1, |n| n.get().min(8))
    };
    patch_on(pixels, width, zone, radius, seed, threads)
}
/// `patch` worked out on up to `threads` threads.
fn patch_on(
    pixels: &[u8],
    width: u32,
    zone: Zone,
    radius: u32,
    seed: u64,
    threads: usize,
) -> Vec<u8> {
    let (w, h) = (zone.width as usize, zone.height as usize);
    let row = width as usize * 4;
    let span = 2 * radius as u64 + 1;
    let seed = mix(seed);
    let stride = w * 4;
    let mut out = vec![0u8; zone.len()];
    // Each output pixel takes a random neighbour, clamped to the zone: nothing outside it is
    // read. The offsets come from the high and low halves of one hash per pixel, scaled to
    // the span by a multiplication rather than a division.
    by_bands(&mut out, stride, threads, |first, band| {
        for (i, line) in band.chunks_mut(stride).enumerate() {
            let y = first + i;
            for (x, to) in line.as_chunks_mut::<4>().0.iter_mut().enumerate() {
                let hash = mix(seed ^ (((x as u64) << 32) | y as u64));
                let dx = (((hash & 0xFFFF_FFFF) * span) >> 32) as usize;
                let dy = (((hash >> 32) * span) >> 32) as usize;
                let sx = (x + dx).saturating_sub(radius as usize).min(w - 1) + zone.x as usize;
                let sy = (y + dy).saturating_sub(radius as usize).min(h - 1) + zone.y as usize;
                let at = sy * row + sx * 4;
                *to = [pixels[at], pixels[at + 1], pixels[at + 2], 255];
            }
        }
    });
    let b = box_radius(radius);
    let mut scratch = vec![0u8; out.len()];
    for _ in 0..3 {
        by_bands(&mut scratch, stride, threads, |first, band| {
            horizontal(&out[first * stride..][..band.len()], band, w, b);
        });
        by_bands(&mut out, stride, threads, |first, band| {
            vertical(&scratch, band, w, h, b, first);
        });
    }
    for p in out.as_chunks_mut::<4>().0 {
        p[3] = 255;
    }
    out
}

/// Bytes of patch from which the work is shared between threads: below, starting them
/// costs more than it saves.
const PARALLEL_FROM: usize = 1 << 20;

/// `work(first row, its rows)` over bands of whole rows of `out` (`stride` bytes each), one
/// band per thread. Every row is computed the same way whatever its band, so the result never
/// depends on the number of threads.
fn by_bands(out: &mut [u8], stride: usize, threads: usize, work: impl Fn(usize, &mut [u8]) + Sync) {
    let rows = out.len() / stride.max(1);
    if threads <= 1 || rows < 2 {
        return work(0, out);
    }
    let band = rows.div_ceil(threads);
    std::thread::scope(|scope| {
        for (i, chunk) in out.chunks_mut(band * stride).enumerate() {
            let work = &work;
            scope.spawn(move || work(i * band, chunk));
        }
    });
}

/// `sum / n` rounded, for sums of at most `255 × n` pixels: a multiplication by the
/// reciprocal, exact for these sizes (sum × n stays far below 2³²).
#[derive(Clone, Copy)]
struct Mean {
    half: u32,
    reciprocal: u64,
}
impl Mean {
    fn new(n: u32) -> Self {
        Self {
            half: n / 2,
            reciprocal: (1u64 << 32).div_ceil(u64::from(n)),
        }
    }
    fn of(self, sum: u32) -> u8 {
        ((u64::from(sum + self.half) * self.reciprocal) >> 32) as u8
    }
}

/// Box average of half-width `b` along each row of `from` into the same row of `to`, rows
/// `w` pixels wide, edges repeated.
fn horizontal(from: &[u8], to: &mut [u8], w: usize, b: usize) {
    let mean = Mean::new((2 * b + 1) as u32);
    for (src, dst) in from.chunks(w * 4).zip(to.chunks_mut(w * 4)) {
        let src = src.as_chunks::<4>().0;
        let dst = dst.as_chunks_mut::<4>().0;
        let at = |x: usize| src[x.min(w - 1)].map(u32::from);
        let mut sum = at(0).map(|v| v * (b as u32 + 1));
        for i in 1..=b {
            let add = at(i);
            for c in 0..4 {
                sum[c] += add[c];
            }
        }
        for (x, out) in dst.iter_mut().enumerate() {
            *out = sum.map(|s| mean.of(s));
            let (add, remove) = (at(x + b + 1), at(x.saturating_sub(b)));
            for c in 0..4 {
                sum[c] = sum[c] + add[c] - remove[c];
            }
        }
    }
}

/// Box average of half-width `b` down each column of `from` (`w` × `h`), edges repeated,
/// into `to`, its rows from `first` on: rows are walked in order, one running sum per
/// column, so memory is read as it lies.
fn vertical(from: &[u8], to: &mut [u8], w: usize, h: usize, b: usize, first: usize) {
    let mean = Mean::new((2 * b + 1) as u32);
    let stride = w * 4;
    let line = |y: usize| &from[y.min(h - 1) * stride..][..stride];
    let mut sum = vec![0u32; stride];
    for y in first as isize - b as isize..=(first + b) as isize {
        for (s, v) in sum.iter_mut().zip(line(y.max(0) as usize)) {
            *s += u32::from(*v);
        }
    }
    for (i, dst) in to.chunks_mut(stride).enumerate() {
        let y = first + i;
        let (add, remove) = (line(y + b + 1), line(y.saturating_sub(b)));
        for (((out, s), a), r) in dst.iter_mut().zip(&mut sum).zip(add).zip(remove) {
            *out = mean.of(*s);
            *s = *s + u32::from(*a) - u32::from(*r);
        }
    }
}

/// The frame `pixels` (`width` × `height`) with the capture under every blur of `objects`
/// replaced by what that blur shows, as on screen: each patch comes from the untouched
/// capture, later ones over earlier ones. True when anything changed.
pub fn flatten(pixels: &mut [u8], width: u32, height: u32, objects: &[Object]) -> bool {
    let blurs = BlurPatch::all(objects, width, height);
    let patches: Vec<_> = blurs.iter().map(|b| b.pixels(pixels, width)).collect();
    let row = width as usize * 4;
    for (blur, patch) in blurs.iter().zip(&patches) {
        let Zone {
            x,
            y,
            width: w,
            height: h,
        } = blur.zone;
        let span = w as usize * 4;
        for line in 0..h as usize {
            let at = (y as usize + line) * row + x as usize * 4;
            pixels[at..at + span].copy_from_slice(&patch[line * span..][..span]);
        }
    }
    !blurs.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Point;

    fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Rect {
        Rect {
            min: Point { x: x0, y: y0 },
            max: Point { x: x1, y: y1 },
        }
    }

    #[test]
    fn a_zone_holds_the_pixels_whose_centres_it_covers_within_the_frame() {
        let zone = |r| Zone::of(r, 100, 50);
        assert_eq!(
            zone(rect(10.4, 5.6, 20.5, 9.)),
            Some(Zone {
                x: 10,
                y: 6,
                width: 11,
                height: 3
            })
        );
        assert_eq!(
            zone(rect(-20., -5., 300., 400.)),
            Some(Zone {
                x: 0,
                y: 0,
                width: 100,
                height: 50
            }),
            "clamped to the frame"
        );
        assert_eq!(zone(rect(120., 0., 140., 10.)), None, "off the frame");
        assert_eq!(
            zone(rect(10., 10., 10.2, 20.)),
            None,
            "thinner than a pixel"
        );
        assert_eq!(zone(rect(f64::NAN, 0., 1., 1.)), None);
    }

    #[test]
    fn the_number_of_threads_never_changes_a_pixel() {
        let (width, height) = (97u32, 61u32);
        let pixels: Vec<u8> = (0..width * height * 4)
            .map(|i| (i.wrapping_mul(2_654_435_761) >> 13) as u8)
            .collect();
        let z = Zone {
            x: 3,
            y: 2,
            width: 90,
            height: 57,
        };
        for radius in [1, 7, 30] {
            let alone = patch_on(&pixels, width, z, radius, 5, 1);
            for threads in [2, 3, 8, 57, 200] {
                assert_eq!(
                    patch_on(&pixels, width, z, radius, 5, threads),
                    alone,
                    "{radius} on {threads}"
                );
            }
        }
    }

    #[test]
    fn the_reciprocal_mean_is_exact_for_every_box_the_radii_give() {
        let widest = 2 * box_radius(MAX_BLUR_RADIUS as u32) as u32 + 1;
        for n in 1..=widest {
            let mean = Mean::new(n);
            for sum in 0..=255 * n {
                assert_eq!(u32::from(mean.of(sum)), (sum + n / 2) / n, "{sum} / {n}");
            }
        }
    }

    #[test]
    fn three_box_passes_match_the_chosen_gaussian() {
        // σ of three boxes of half-width b: √(3 × ((2b + 1)² − 1) / 12).
        let sigma = |b: usize| (3. * (((2 * b + 1) * (2 * b + 1)) as f64 - 1.) / 12.).sqrt();
        for radius in [3, 7, 14, 30] {
            let wanted = 0.43 * radius as f64;
            let got = sigma(box_radius(radius));
            assert!(
                (got - wanted).abs() <= 0.5 * wanted,
                "{radius}: {got} for {wanted}"
            );
        }
        assert_eq!(box_radius(7), 3, "the default");
        assert_eq!(box_radius(1), 1, "never none");
    }

    #[test]
    fn a_flat_zone_stays_flat_and_nothing_outside_it_changes() {
        let mut pixels = [9u8, 8, 7, 255].repeat(40 * 30);
        for p in pixels.as_chunks_mut::<4>().0[..40].iter_mut() {
            *p = [0, 0, 0, 255];
        }
        let before = pixels.clone();
        let blur = Object {
            rotation: 0.,
            id: 3,
            shape: Shape::Blur(rect(5., 5., 25., 20.)),
            style: crate::Style {
                color: [0; 3],
                width: 7.,
            },
            fill: Vec::new(),
        };
        assert!(flatten(&mut pixels, 40, 30, std::slice::from_ref(&blur)));
        assert_eq!(pixels, before, "the zone never reads the black first row");
        assert!(!flatten(&mut pixels, 40, 30, &[]));
    }
}
