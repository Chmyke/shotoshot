//! ROLE: The painted frame into a window's 0RGB buffer: whole rows at 1:1, a column table
//! when the window squeezes the frame, and only where the frame changed since the one shown
//! before (the damage), which the compositor is told too.
//! OWNER: One `Presenter` per window keeps the frame it showed last and the damage of the
//! last few frames, for buffers the window hands back after one or two frames.
//! INVARIANTS: Frames are RGBA bytes, opaque; buffers are `0x00RRGGBB` words, top-down.

/// A rectangle of pixels, in frame or buffer coordinates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Damage {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}
impl Damage {
    /// The whole of a `width` × `height` surface.
    pub fn whole((width, height): (u32, u32)) -> Self {
        Self {
            x: 0,
            y: 0,
            width,
            height,
        }
    }
    /// The smallest rectangle holding both.
    pub fn union(self, other: Self) -> Self {
        let x = self.x.min(other.x);
        let y = self.y.min(other.y);
        let right = (self.x + self.width).max(other.x + other.width);
        let bottom = (self.y + self.height).max(other.y + other.height);
        Self {
            x,
            y,
            width: right - x,
            height: bottom - y,
        }
    }
    /// This frame rectangle as the buffer rectangle whose pixels sample it, when the frame of
    /// `frame_size` is shown squeezed into a buffer of `buffer_size` (`blit`'s mapping).
    pub fn to_buffer(self, frame_size: (u32, u32), buffer_size: (u32, u32)) -> Self {
        if frame_size == buffer_size {
            return self;
        }
        let axis = |from: u32, len: u32, frame: u32, buffer: u32| {
            let (frame, buffer) = (frame as u64, buffer as u64);
            // Buffer pixel x samples frame pixel x * frame / buffer: frame pixels [from, to)
            // are sampled by buffer pixels [ceil(from * buffer / frame), ceil(to * buffer / frame)).
            let start = (from as u64 * buffer).div_ceil(frame).min(buffer) as u32;
            let end = ((from + len) as u64 * buffer).div_ceil(frame).min(buffer) as u32;
            (start, end.max(start) - start)
        };
        let (x, width) = axis(self.x, self.width, frame_size.0, buffer_size.0);
        let (y, height) = axis(self.y, self.height, frame_size.1, buffer_size.1);
        Self {
            x,
            y,
            width,
            height,
        }
    }
    fn is_empty(self) -> bool {
        self.width == 0 || self.height == 0
    }
}

/// How many frames back a buffer may come from and still be refreshed by its damage alone.
const REMEMBERED: usize = 3;

/// What a buffer needs for the frame about to be presented.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Plan {
    /// Frame pixels to copy into the buffer: what changed since the frame this buffer held.
    pub refresh: Damage,
    /// Frame pixels that differ from the frame on screen: what the compositor must redraw.
    pub changed: Damage,
}

/// The frame shown last and the damage of the frames before, for one window.
#[derive(Default)]
pub struct Presenter {
    shown: Vec<u8>,
    frame_size: (u32, u32),
    buffer_size: (u32, u32),
    /// Damage of the frames presented, oldest first, at most `REMEMBERED`.
    history: Vec<Damage>,
}
impl Presenter {
    pub fn new() -> Self {
        Self::default()
    }
    /// What a buffer of `age` (softbuffer's: 0 for unknown contents, n for the frame presented
    /// n frames ago) needs for `frame`, of `frame_size`, shown in a buffer of `buffer_size`;
    /// `None` when the frame on screen is this one already.
    pub fn plan(
        &mut self,
        frame: &[u8],
        frame_size: (u32, u32),
        buffer_size: (u32, u32),
        age: u8,
    ) -> Option<Plan> {
        let whole = Damage::whole(frame_size);
        let fresh = self.shown.len() != frame.len()
            || self.frame_size != frame_size
            || self.buffer_size != buffer_size;
        if fresh {
            self.shown = vec![0; frame.len()];
            self.frame_size = frame_size;
            self.buffer_size = buffer_size;
            self.history.clear();
            return Some(Plan {
                refresh: whole,
                changed: whole,
            });
        }
        let changed = damage_between(&self.shown, frame, frame_size.0)?;
        let back = age as usize;
        let refresh = if back == 0 || back > self.history.len() + 1 {
            whole
        } else {
            self.history[self.history.len() + 1 - back..]
                .iter()
                .fold(changed, |all, d| all.union(*d))
        };
        Some(Plan { refresh, changed })
    }
    /// Records `frame` as the one on screen, once `plan` has been carried out.
    pub fn shown(&mut self, frame: &[u8], plan: &Plan) {
        let width = self.frame_size.0 as usize * 4;
        let d = plan.changed;
        for y in d.y..d.y + d.height {
            let from = y as usize * width + d.x as usize * 4;
            let to = from + d.width as usize * 4;
            self.shown[from..to].copy_from_slice(&frame[from..to]);
        }
        if self.history.len() == REMEMBERED {
            self.history.remove(0);
        }
        self.history.push(d);
    }
}

/// The bounding box of the pixels that differ between `before` and `after`, two RGBA frames
/// `width` pixels wide; `None` when they are the same.
pub fn damage_between(before: &[u8], after: &[u8], width: u32) -> Option<Damage> {
    let row = width as usize * 4;
    if row == 0 || before.len() != after.len() {
        return None;
    }
    let (mut top, mut bottom) = (None, 0);
    let (mut left, mut right) = (usize::MAX, 0);
    for (y, (a, b)) in before
        .chunks_exact(row)
        .zip(after.chunks_exact(row))
        .enumerate()
    {
        if a == b {
            continue;
        }
        top.get_or_insert(y);
        bottom = y;
        let mut pixels = a.as_chunks::<4>().0.iter().zip(b.as_chunks::<4>().0);
        let first = pixels.clone().position(|(p, q)| p != q).unwrap_or(0);
        let last = pixels.rposition(|(p, q)| p != q).unwrap_or(first);
        left = left.min(first);
        right = right.max(last);
    }
    let top = top?;
    Some(Damage {
        x: left as u32,
        y: top as u32,
        width: (right + 1 - left) as u32,
        height: (bottom + 1 - top) as u32,
    })
}

/// Copies `frame` (RGBA, `frame_size`) into `buffer` (0RGB words, `buffer_size`) inside
/// `rect`, in buffer coordinates; a buffer pixel samples frame pixel `x * fw / bw`, so the
/// frame is squeezed or stretched to the buffer when their sizes differ, 1:1 otherwise.
pub fn blit(
    frame: &[u8],
    frame_size: (u32, u32),
    buffer: &mut [u32],
    buffer_size: (u32, u32),
    rect: Damage,
) {
    let (fw, fh) = (frame_size.0 as usize, frame_size.1 as usize);
    let (bw, bh) = (buffer_size.0 as usize, buffer_size.1 as usize);
    let rect = Damage {
        width: rect.width.min(buffer_size.0.saturating_sub(rect.x)),
        height: rect.height.min(buffer_size.1.saturating_sub(rect.y)),
        ..rect
    };
    if rect.is_empty() || frame.len() < fw * fh * 4 || buffer.len() < bw * bh {
        return;
    }
    let (x0, x1) = (rect.x as usize, (rect.x + rect.width) as usize);
    let rows = rect.y as usize..(rect.y + rect.height) as usize;
    let word = |p: &[u8; 4]| u32::from_be_bytes([0, p[0], p[1], p[2]]);
    if (fw, fh) == (bw, bh) {
        for y in rows {
            let src = frame[(y * fw + x0) * 4..(y * fw + x1) * 4]
                .as_chunks::<4>()
                .0;
            for (out, px) in buffer[y * bw + x0..y * bw + x1].iter_mut().zip(src) {
                *out = word(px);
            }
        }
        return;
    }
    let columns: Vec<usize> = (x0..x1).map(|x| x * fw / bw * 4).collect();
    for y in rows {
        let src = &frame[y * fh / bh * fw * 4..];
        for (out, &at) in buffer[y * bw + x0..y * bw + x1].iter_mut().zip(&columns) {
            let px: &[u8; 4] = src[at..at + 4].try_into().unwrap_or(&[0; 4]);
            *out = word(px);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A frame whose every pixel differs, from a cheap generator.
    fn frame(width: u32, height: u32, seed: u8) -> Vec<u8> {
        (0..width * height * 4)
            .map(|i| (i as u8).wrapping_mul(31).wrapping_add(seed))
            .collect()
    }
    /// The formula every pixel obeyed before: two divisions and a recomposition.
    fn reference(frame: &[u8], (fw, fh): (u32, u32), (bw, bh): (u32, u32)) -> Vec<u32> {
        let (fw, fh, bw, bh) = (fw as usize, fh as usize, bw as usize, bh as usize);
        let mut out = vec![0; bw * bh];
        for y in 0..bh {
            let sy = y * fh / bh;
            for x in 0..bw {
                let sx = x * fw / bw;
                let i = (sy * fw + sx) * 4;
                out[y * bw + x] =
                    (frame[i] as u32) << 16 | (frame[i + 1] as u32) << 8 | frame[i + 2] as u32;
            }
        }
        out
    }

    #[test]
    fn a_one_to_one_blit_matches_the_per_pixel_formula() {
        let size = (37, 11);
        let f = frame(size.0, size.1, 3);
        let mut out = vec![0; 37 * 11];
        blit(&f, size, &mut out, size, Damage::whole(size));
        assert_eq!(out, reference(&f, size, size));
    }
    #[test]
    fn a_squeezed_blit_matches_the_per_pixel_formula() {
        let (frame_size, buffer_size) = ((40, 30), (17, 9));
        let f = frame(frame_size.0, frame_size.1, 9);
        let mut out = vec![0; 17 * 9];
        blit(
            &f,
            frame_size,
            &mut out,
            buffer_size,
            Damage::whole(buffer_size),
        );
        assert_eq!(out, reference(&f, frame_size, buffer_size));
    }
    #[test]
    fn a_blit_touches_only_its_rectangle_and_stops_at_the_buffer() {
        let size = (20, 10);
        let f = frame(size.0, size.1, 5);
        let mut out = vec![u32::MAX; 20 * 10];
        let rect = Damage {
            x: 15,
            y: 7,
            width: 10,
            height: 10,
        };
        blit(&f, size, &mut out, size, rect);
        let expected = reference(&f, size, size);
        for y in 0..10 {
            for x in 0..20 {
                let inside = x >= 15 && y >= 7;
                let want = if inside {
                    expected[y * 20 + x]
                } else {
                    u32::MAX
                };
                assert_eq!(out[y * 20 + x], want, "{x},{y}");
            }
        }
    }
    #[test]
    fn damage_is_the_bounding_box_of_what_differs() {
        let size = (16, 8);
        let before = frame(size.0, size.1, 1);
        assert_eq!(damage_between(&before, &before, size.0), None);
        let mut after = before.clone();
        let poke = |a: &mut Vec<u8>, x: usize, y: usize| a[(y * 16 + x) * 4 + 1] ^= 0xFF;
        poke(&mut after, 3, 2);
        poke(&mut after, 9, 5);
        poke(&mut after, 4, 5);
        assert_eq!(
            damage_between(&before, &after, size.0),
            Some(Damage {
                x: 3,
                y: 2,
                width: 7,
                height: 4
            })
        );
    }
    #[test]
    fn a_buffer_is_refreshed_for_as_many_frames_as_its_age() {
        let size = (16, 8);
        let mut presenter = Presenter::new();
        let mut shown = frame(size.0, size.1, 1);
        let whole = Damage::whole(size);
        // First frame: everything, whatever the buffer says.
        let plan = presenter.plan(&shown, size, size, 1).unwrap();
        assert_eq!(
            plan,
            Plan {
                refresh: whole,
                changed: whole
            }
        );
        presenter.shown(&shown, &plan);
        // The same frame again: nothing to present.
        assert_eq!(presenter.plan(&shown, size, size, 1), None);
        // One pixel changes; the buffer holds the previous frame: refresh that pixel only.
        let poke = |a: &mut Vec<u8>, x: usize, y: usize| a[(y * 16 + x) * 4] ^= 0xFF;
        poke(&mut shown, 2, 1);
        let first = Damage {
            x: 2,
            y: 1,
            width: 1,
            height: 1,
        };
        let plan = presenter.plan(&shown, size, size, 1).unwrap();
        assert_eq!(
            plan,
            Plan {
                refresh: first,
                changed: first
            }
        );
        presenter.shown(&shown, &plan);
        // Another pixel; the buffer is two frames old: both changes go in, one is new.
        poke(&mut shown, 10, 6);
        let second = Damage {
            x: 10,
            y: 6,
            width: 1,
            height: 1,
        };
        let plan = presenter.plan(&shown, size, size, 2).unwrap();
        assert_eq!(
            plan,
            Plan {
                refresh: first.union(second),
                changed: second
            }
        );
        presenter.shown(&shown, &plan);
        // Unknown contents: everything again, but the compositor learns the real change.
        poke(&mut shown, 0, 0);
        let third = Damage {
            x: 0,
            y: 0,
            width: 1,
            height: 1,
        };
        let plan = presenter.plan(&shown, size, size, 0).unwrap();
        assert_eq!(
            plan,
            Plan {
                refresh: whole,
                changed: third
            }
        );
        presenter.shown(&shown, &plan);
        // Older than remembered: everything.
        poke(&mut shown, 5, 5);
        let plan = presenter.plan(&shown, size, size, 7).unwrap();
        assert_eq!(plan.refresh, whole);
        presenter.shown(&shown, &plan);
        // A new window size forgets everything.
        let plan = presenter.plan(&shown, size, (8, 4), 1).unwrap();
        assert_eq!(
            plan,
            Plan {
                refresh: whole,
                changed: whole
            }
        );
    }
    #[test]
    fn a_frame_rectangle_maps_to_every_buffer_pixel_that_samples_it() {
        let (frame_size, buffer_size) = ((23u32, 9u32), (10u32, 4u32));
        for (x, y, width, height) in [(0, 0, 23, 9), (5, 2, 3, 1), (22, 8, 1, 1), (7, 0, 1, 9)] {
            let mapped = Damage {
                x,
                y,
                width,
                height,
            }
            .to_buffer(frame_size, buffer_size);
            for by in 0..buffer_size.1 {
                for bx in 0..buffer_size.0 {
                    let (sx, sy) = (
                        bx * frame_size.0 / buffer_size.0,
                        by * frame_size.1 / buffer_size.1,
                    );
                    let samples = sx >= x && sx < x + width && sy >= y && sy < y + height;
                    let inside = bx >= mapped.x
                        && bx < mapped.x + mapped.width
                        && by >= mapped.y
                        && by < mapped.y + mapped.height;
                    assert_eq!(
                        samples, inside,
                        "frame rect {x},{y} {width}x{height} at buffer {bx},{by}"
                    );
                }
            }
        }
        assert_eq!(
            Damage::whole(frame_size).to_buffer(frame_size, frame_size),
            Damage::whole(frame_size)
        );
    }
}
