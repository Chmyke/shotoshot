//! ROLE: What Explorer shows of the board, read through its rendering of the desktop: our notes
//! taken out of the editor's frame, and the wait until a new bake is on screen.
//! INVARIANTS: Reads the desktop, never changes it; the comparisons are pure and work on pictures
//! of one screen, top-down BGRA.
use super::{bake::Baked, base::is_baked_name, wallpaper::Wallpaper};
use crate::{Image, image::blit};
use shotoshot_core::board::Screen;
use std::time::{Duration, Instant};
use windows::core::Result;

/// Largest difference per channel still taken as the same pixel: a PNG round trip is exact,
/// DWM's composition of the wallpaper may round.
const TOLERANCE: u8 = 2;
/// Share of a screen's pixels that must show our picture for its notes to be taken out; below,
/// the rendering is not our picture as we know it and the screen shows its base alone.
const MIN_UNBAKE_MATCH: f64 = 0.9;
/// Share of the points a bake changes that must show the new picture.
const MIN_SHOWN_MATCH: f64 = 0.95;
/// Most sample points one screen's wait reads on each poll.
const MAX_POINTS: usize = 1024;
/// Sample points per side of a screen when the old picture is of another size and cannot be
/// compared pixel by pixel.
const GRID: u32 = 32;
const POLL: Duration = Duration::from_millis(50);
/// How long a session waits for Windows to show its bake (measured: 260 to 617 ms).
pub const SHOWN_WAIT: Duration = Duration::from_millis(1500);

fn same(a: &[u8], b: &[u8]) -> bool {
    a[..3]
        .iter()
        .zip(&b[..3])
        .all(|(x, y)| x.abs_diff(*y) <= TOLERANCE)
}
fn same_size(a: &Image, b: &Image) -> bool {
    (a.width, a.height) == (b.width, b.height)
}
fn pixel(image: &Image, (x, y): (u32, u32)) -> &[u8] {
    let i = (y as usize * image.width as usize + x as usize) * 4;
    &image.pixels[i..i + 4]
}

/// Takes our notes out of Explorer's rendering of one screen. Where `rendering` shows what
/// `baked` holds, notes included, `base` is written instead; elsewhere something lies over our
/// picture (an icon, its label) and the rendering is kept. Returns that picture and the share of
/// pixels that showed `baked`; pictures of different sizes share nothing.
pub fn unbake(rendering: &Image, baked: &Image, base: &Image) -> (Image, f64) {
    let mut out = rendering.clone();
    if !same_size(rendering, baked) || !same_size(rendering, base) {
        return (out, 0.);
    }
    let mut matched = 0usize;
    for ((shown, ours), under) in out
        .pixels
        .as_chunks_mut::<4>()
        .0
        .iter_mut()
        .zip(baked.pixels.as_chunks::<4>().0)
        .zip(base.pixels.as_chunks::<4>().0)
    {
        if same(shown, ours) {
            *shown = *under;
            matched += 1;
        }
    }
    let total = (out.width as usize * out.height as usize).max(1);
    (out, matched as f64 / total as f64)
}

/// One screen of the editor's frame: its `rendering` without the notes of `baked`, the picture
/// of ours it shows. The base alone when that picture could not be read or the rendering does
/// not show it: notes the editor cannot remove never appear in the frame.
fn without_notes(rendering: &Image, baked: Option<&Image>, base: &Image) -> Image {
    match baked.map(|baked| unbake(rendering, baked, base)) {
        Some((unbaked, share)) if share >= MIN_UNBAKE_MATCH => unbaked,
        _ => base.clone(),
    }
}

/// `screen`'s rectangle cut out of `frame`, which `left`/`top` place on the desktop.
fn cut(frame: &Image, screen: &Screen) -> Image {
    let size = screen.bounds.size();
    let (left, top) = (screen.bounds.min.x as i32, screen.bounds.min.y as i32);
    let mut out = Image::solid(size.x as u32, size.y as u32, [0, 0, 0, 255]);
    blit(&mut out, frame, frame.left - left, frame.top - top);
    (out.left, out.top) = (left, top);
    out
}
fn paste(frame: &mut Image, picture: &Image, screen: &Screen) {
    let (x, y) = (
        screen.bounds.min.x as i32 - frame.left,
        screen.bounds.min.y as i32 - frame.top,
    );
    blit(frame, picture, x, y);
}

/// The editor's frame and what lies over its notes.
pub struct DesktopFrame {
    /// Explorer's rendering of the desktop without our notes: the base and the icons.
    pub frame: Image,
    /// The icons alone, premultiplied BGRA, transparent where `frame` shows its base, so the
    /// editor lays them over the notes as the desktop does. `None` when nothing lies over any
    /// screen, or Windows rendered nothing.
    pub icons: Option<Image>,
}

/// The editor's frame: Explorer's rendering of the desktop (pictures and icons, no window) over
/// `base_frame`, each screen showing a picture of ours without its notes, so the session's
/// objects are the only notes in it. The base alone where Windows renders nothing.
pub fn desktop_frame(
    base_frame: &Image,
    wallpaper: &Wallpaper,
    screens: &[Screen],
) -> Result<DesktopFrame> {
    let mut frame = base_frame.clone();
    // Fails without drawing anything: the frame is then the base, without icons.
    if crate::image::render_desktop(&mut frame).is_err() {
        return Ok(DesktopFrame { frame, icons: None });
    }
    for screen in screens {
        let Some(shown) = wallpaper.shown(&screen.id)?.filter(|p| is_baked_name(p)) else {
            continue;
        };
        let baked = crate::load_png(&shown).ok();
        let kept = without_notes(
            &cut(&frame, screen),
            baked.as_ref(),
            &cut(base_frame, screen),
        );
        paste(&mut frame, &kept, screen);
    }
    let icons = icons_over(&frame, base_frame, screens);
    Ok(DesktopFrame { frame, icons })
}

/// Largest share of a screen's pixels taken for icons and labels; above, the rendering is not
/// the base as we draw it (a photo Windows scales its own way) and the screen gets no icons.
const MAX_ICON_SHARE: f64 = 0.5;

/// What lies over the base on each screen of `frame`: its pixels that differ from `base_frame`,
/// opaque, over transparency. A screen differing on more than `MAX_ICON_SHARE` of its pixels
/// contributes nothing. `None` when nothing lies over any screen.
pub(crate) fn icons_over(frame: &Image, base_frame: &Image, screens: &[Screen]) -> Option<Image> {
    let mut icons = Image::solid(frame.width, frame.height, [0, 0, 0, 0]);
    (icons.left, icons.top) = (frame.left, frame.top);
    let mut any = false;
    for screen in screens {
        let (layer, differing) = lift(&cut(frame, screen), &cut(base_frame, screen));
        let total = (layer.width as usize * layer.height as usize).max(1);
        if differing == 0 || differing as f64 / total as f64 > MAX_ICON_SHARE {
            continue;
        }
        paste(&mut icons, &layer, screen);
        any = true;
    }
    any.then_some(icons)
}
/// `shown` where it differs from `base`, opaque, transparent elsewhere, with the count of
/// differing pixels.
fn lift(shown: &Image, base: &Image) -> (Image, usize) {
    let mut out = Image::solid(shown.width, shown.height, [0, 0, 0, 0]);
    let mut differing = 0;
    for ((pixel, over), under) in out
        .pixels
        .as_chunks_mut::<4>()
        .0
        .iter_mut()
        .zip(shown.pixels.as_chunks::<4>().0)
        .zip(base.pixels.as_chunks::<4>().0)
    {
        if !same(over, under) {
            *pixel = [over[0], over[1], over[2], 255];
            differing += 1;
        }
    }
    (out, differing)
}

/// The points of a regular grid over a picture of `width` by `height`, each one once.
fn grid(width: u32, height: u32) -> Vec<(u32, u32)> {
    let axis = |length: u32| {
        let mut values: Vec<u32> = (0..GRID)
            .map(|i| ((2 * i + 1) as u64 * length as u64 / (2 * GRID) as u64) as u32)
            .collect();
        values.dedup();
        values
    };
    let (xs, ys) = (axis(width), axis(height));
    ys.iter()
        .flat_map(|y| xs.iter().map(move |x| (*x, *y)))
        .collect()
}
/// Where a screen showing `old` will look different once it shows `new`: the only places that
/// tell whether Windows shows the new picture. Every pixel is compared (one pass), so a thin
/// stroke or a short text yields points; when more than `MAX_POINTS` pixels changed, every k-th
/// of them is kept, k being the smallest step that leaves at most `MAX_POINTS`. A grid over the
/// screen when the sizes differ, every pixel having changed then.
pub fn changed_points(old: &Image, new: &Image) -> Vec<(u32, u32)> {
    if !same_size(old, new) {
        return grid(new.width, new.height);
    }
    let changed: Vec<u32> = old
        .pixels
        .as_chunks::<4>()
        .0
        .iter()
        .zip(new.pixels.as_chunks::<4>().0)
        .enumerate()
        .filter(|(_, (a, b))| !same(*a, *b))
        .map(|(i, _)| i as u32)
        .collect();
    let step = changed.len().div_ceil(MAX_POINTS).max(1);
    changed
        .into_iter()
        .step_by(step)
        .map(|i| (i % new.width, i / new.width))
        .collect()
}
/// The share of `points` where `rendering` shows `new`; all of them when there is none to look at.
pub fn shown_share(rendering: &Image, new: &Image, points: &[(u32, u32)]) -> f64 {
    if points.is_empty() {
        return 1.;
    }
    if !same_size(rendering, new) {
        return 0.;
    }
    let shown = points
        .iter()
        .filter(|p| same(pixel(rendering, **p), pixel(new, **p)))
        .count();
    shown as f64 / points.len() as f64
}

/// The picture of ours each screen shows now, read before a bake rewrites its file: what that
/// screen looks like, icons aside, until Windows shows the new one. A screen showing the user's
/// own background is left out.
pub fn shown_pictures(wallpaper: &Wallpaper) -> Result<Vec<(String, Image)>> {
    let mut pictures = Vec::new();
    for screen in wallpaper.screens()? {
        if let Some(path) = wallpaper.shown(&screen.id)?.filter(|p| is_baked_name(p))
            && let Ok(picture) = crate::load_png(&path)
        {
            pictures.push((screen.id, picture));
        }
    }
    Ok(pictures)
}

/// Waits until Windows shows `baked` on every screen where it changes something, reading the
/// desktop every `POLL`, for `limit` at most. A screen counts as showing its picture once
/// `MIN_SHOWN_MATCH` of its `changed_points` do: what is checked is those pixels, not the whole
/// screen. `before` is what `shown_pictures` read before the bake; a screen missing from it
/// showed the base. True when every screen shows its picture; false at the deadline, or when the
/// desktop cannot be read.
pub fn wait_until_shown(before: &[(String, Image)], baked: &Baked, limit: Duration) -> bool {
    /// A screen not yet seen with its new picture, and the points that will tell.
    struct Pending<'a> {
        screen: &'a Screen,
        picture: &'a Image,
        points: Vec<(u32, u32)>,
    }
    let deadline = Instant::now() + limit;
    let mut pending = Vec::new();
    for (id, picture) in &baked.pictures {
        let Some(screen) = baked.screens.iter().find(|s| s.id == *id) else {
            continue;
        };
        let points = match before.iter().find(|(shown, _)| shown == id) {
            Some((_, old)) => changed_points(old, picture),
            None => changed_points(&cut(&baked.frame, screen), picture),
        };
        if !points.is_empty() {
            pending.push(Pending {
                screen,
                picture,
                points,
            });
        }
    }
    // Every poll draws over the same frame: the rendering covers it whole wherever a screen is.
    let mut rendering = baked.frame.clone();
    while !pending.is_empty() {
        if crate::image::render_desktop(&mut rendering).is_err() {
            // Nothing to read: waiting longer would tell nothing more.
            return false;
        }
        pending.retain(|p| {
            shown_share(&cut(&rendering, p.screen), p.picture, &p.points) < MIN_SHOWN_MATCH
        });
        if pending.is_empty() || Instant::now() >= deadline {
            break;
        }
        std::thread::sleep(POLL);
    }
    pending.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    const GREY: [u8; 4] = [90, 90, 90, 255];
    const NOTE: [u8; 4] = [92, 79, 237, 255];
    const ICON: [u8; 4] = [200, 120, 10, 255];

    fn put(image: &mut Image, (x, y): (u32, u32), color: [u8; 4]) {
        let i = (y as usize * image.width as usize + x as usize) * 4;
        image.pixels[i..i + 4].copy_from_slice(&color);
    }
    /// A 10 by 10 picture of `color` with `marks` painted on it.
    fn picture(color: [u8; 4], marks: &[((u32, u32), [u8; 4])]) -> Image {
        let mut image = Image::solid(10, 10, color);
        for (point, mark) in marks {
            put(&mut image, *point, *mark);
        }
        image
    }
    fn at(image: &Image, point: (u32, u32)) -> [u8; 4] {
        pixel(image, point).try_into().unwrap()
    }

    #[test]
    fn icons_are_what_lies_over_the_base_and_a_screen_too_different_has_none() {
        use shotoshot_core::{Point, Rect};
        let screen = |id: &str, x: f64| Screen {
            id: id.into(),
            bounds: Rect::new(Point { x, y: 0. }, Point { x: x + 10., y: 10. }).unwrap(),
        };
        let screens = [screen("a", 0.), screen("b", 10.)];
        let base = Image::solid(20, 10, GREY);
        let mut frame = base.clone();
        put(&mut frame, (3, 4), ICON);
        // Six rows of ten on the second screen: more than half of it is not its base.
        for x in 10..20 {
            for y in 0..6 {
                put(&mut frame, (x, y), ICON);
            }
        }
        let icons = icons_over(&frame, &base, &screens).expect("an icon on the first screen");
        assert_eq!((icons.width, icons.height), (20, 10));
        assert_eq!(
            at(&icons, (3, 4)),
            ICON,
            "opaque where something lies over the base"
        );
        assert_eq!(
            at(&icons, (0, 0)),
            [0, 0, 0, 0],
            "transparent where the base shows"
        );
        assert_eq!(
            at(&icons, (12, 2)),
            [0, 0, 0, 0],
            "a screen too different from its base has no icons"
        );
        assert!(
            icons_over(&base, &base, &screens).is_none(),
            "nothing over any screen: no layer"
        );
    }
    #[test]
    fn unbaking_removes_our_notes_and_keeps_what_lies_over_them() {
        let base = picture(GREY, &[]);
        let baked = picture(GREY, &[((2, 2), NOTE), ((3, 2), NOTE)]);
        // Windows shows our picture, with an icon over the plain base and one over a note.
        let rendering = picture(GREY, &[((2, 2), NOTE), ((3, 2), ICON), ((7, 7), ICON)]);
        let (unbaked, share) = unbake(&rendering, &baked, &base);
        assert_eq!(at(&unbaked, (2, 2)), GREY, "the note is gone");
        assert_eq!(at(&unbaked, (3, 2)), ICON, "an icon over a note stays");
        assert_eq!(at(&unbaked, (7, 7)), ICON, "an icon stays");
        assert_eq!(at(&unbaked, (0, 0)), GREY);
        assert_eq!(share, 0.98, "two pixels of a hundred are icons");
        // A rounding of the composition is still our picture.
        let mut rounded = rendering.clone();
        rounded.pixels[(2 * 10 + 2) * 4] += 2;
        assert_eq!(at(&unbake(&rounded, &baked, &base).0, (2, 2)), GREY);
        assert_eq!(
            without_notes(&rendering, Some(&baked), &base).pixels,
            unbaked.pixels
        );
    }

    #[test]
    fn a_rendering_that_is_not_our_picture_gives_the_base_alone() {
        let base = picture(GREY, &[]);
        let baked = picture(GREY, &[((2, 2), NOTE)]);
        // Another wallpaper, or ours not shown yet: almost nothing matches.
        let other = picture([10, 10, 10, 255], &[((2, 2), NOTE)]);
        let (_, share) = unbake(&other, &baked, &base);
        assert!(share < MIN_UNBAKE_MATCH, "{share}");
        assert_eq!(
            without_notes(&other, Some(&baked), &base).pixels,
            base.pixels
        );
        // Our picture could not be read: its notes may be on screen, so the base too.
        assert_eq!(without_notes(&other, None, &base).pixels, base.pixels);
        // A picture baked for another resolution matches nowhere.
        let small = Image::solid(5, 5, GREY);
        assert_eq!(unbake(&base, &small, &base).1, 0.);
    }

    #[test]
    fn a_screen_is_cut_out_of_the_frame_and_pasted_back_at_its_place() {
        let screen = Screen {
            id: "b".into(),
            bounds: shotoshot_core::Rect::new(
                shotoshot_core::Point { x: 0., y: -2. },
                shotoshot_core::Point { x: 2., y: 0. },
            )
            .unwrap(),
        };
        // A 4 by 4 frame starting at (-2, -2): the screen is its top right quarter.
        let mut frame = Image::solid(4, 4, GREY);
        (frame.left, frame.top) = (-2, -2);
        frame.pixels[(2 * 4)..(3 * 4)].copy_from_slice(&NOTE);
        let piece = cut(&frame, &screen);
        assert_eq!(
            (piece.width, piece.height, piece.left, piece.top),
            (2, 2, 0, -2)
        );
        assert_eq!(at(&piece, (0, 0)), NOTE);
        paste(&mut frame, &Image::solid(2, 2, ICON), &screen);
        let icon = (0..4)
            .flat_map(|y| (0..4).map(move |x| (x, y)))
            .filter(|p| at(&frame, *p) == ICON)
            .collect::<Vec<_>>();
        assert_eq!(icon, vec![(2, 0), (3, 0), (2, 1), (3, 1)]);
    }

    #[test]
    fn a_new_picture_is_shown_once_the_points_it_changes_show_it() {
        let old = picture(GREY, &[((2, 2), NOTE)]);
        // The note moved: two places change, the rest of the screen does not.
        let new = picture(GREY, &[((6, 6), NOTE)]);
        let points = changed_points(&old, &new);
        assert_eq!(points, vec![(2, 2), (6, 6)]);
        assert_eq!(
            shown_share(&old, &new, &points),
            0.,
            "still the old picture"
        );
        assert_eq!(shown_share(&new, &new, &points), 1., "the new one");
        // Icons elsewhere do not matter; one over a changed point does.
        let iconed = picture(GREY, &[((6, 6), NOTE), ((0, 0), ICON)]);
        assert_eq!(shown_share(&iconed, &new, &points), 1.);
        let covered = picture(GREY, &[((6, 6), ICON)]);
        assert!(shown_share(&covered, &new, &points) < MIN_SHOWN_MATCH);
        // Nothing changes: nothing to wait for.
        assert!(changed_points(&new, &new).is_empty());
        assert_eq!(shown_share(&old, &new, &[]), 1.);
        // Another size: every point changes.
        assert_eq!(changed_points(&Image::solid(5, 5, GREY), &new).len(), 100);
    }

    #[test]
    fn a_stroke_between_grid_lines_still_gives_points_to_wait_on() {
        let old = Image::solid(640, 480, GREY);
        let mut new = old.clone();
        // One pixel high, 100 long, on a row the 32 by 32 grid never reads.
        for x in 100..200 {
            put(&mut new, (x, 12), NOTE);
        }
        assert!(
            grid(640, 480)
                .into_iter()
                .all(|p| same(pixel(&old, p), pixel(&new, p))),
            "the old sampling saw nothing here"
        );
        let points = changed_points(&old, &new);
        assert_eq!(points.len(), 100);
        assert!(
            points
                .iter()
                .all(|p| p.1 == 12 && (100..200).contains(&p.0))
        );
        assert_eq!(
            shown_share(&old, &new, &points),
            0.,
            "still the old picture"
        );
        assert_eq!(shown_share(&new, &new, &points), 1., "the new one");
    }

    #[test]
    fn a_large_change_is_sampled_evenly_in_at_most_the_point_limit() {
        let old = Image::solid(640, 480, GREY);
        let mut new = old.clone();
        // A block of 300 by 200 pixels: 60 000 changed, kept one in 59.
        for y in 100..300 {
            for x in 50..350 {
                put(&mut new, (x, y), NOTE);
            }
        }
        let points = changed_points(&old, &new);
        assert!(points.len() <= MAX_POINTS && points.len() > MAX_POINTS / 2);
        assert!(
            points
                .iter()
                .all(|p| (50..350).contains(&p.0) && (100..300).contains(&p.1))
        );
        // From the top of the block to its bottom.
        assert_eq!(points[0], (50, 100));
        assert!(points.last().unwrap().1 >= 290);
        // Exactly the limit changed: nothing is dropped.
        let mut limit = old.clone();
        for x in 0..MAX_POINTS as u32 {
            put(&mut limit, (x % 640, x / 640), NOTE);
        }
        assert_eq!(changed_points(&old, &limit).len(), MAX_POINTS);
    }

    #[test]
    fn the_grid_covers_a_large_screen_evenly_and_a_small_one_once_per_pixel() {
        let points = grid(1920, 1080);
        assert_eq!(points.len(), (GRID * GRID) as usize);
        assert_eq!(points[0], (30, 16));
        assert_eq!(*points.last().unwrap(), (1890, 1063));
        assert_eq!(
            grid(3, 2),
            vec![(0, 0), (1, 0), (2, 0), (0, 1), (1, 1), (2, 1)]
        );
    }
}
