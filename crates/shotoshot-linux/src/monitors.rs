//! ROLE: Finds the part of a whole-desktop screenshot that one monitor shows, so the
//! full-screen editor displays its own monitor at 1:1 instead of the squeezed desktop, and
//! the monitor that shows a reopened frame the same way.
use shotoshot_core::{Point, Rect};
use shotoshot_ui::{Image, placement};

/// A monitor in desktop pixels: position and size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Area {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// `target`'s rectangle inside a screenshot of all `monitors`, or None to keep the whole
/// image (one monitor, or a layout that does not match the screenshot, as with mixed scales).
pub fn crop_for(screenshot: (u32, u32), monitors: &[Area], target: Area) -> Option<Area> {
    if monitors.len() < 2 {
        return None;
    }
    let left = monitors.iter().map(|m| m.x).min()?;
    let top = monitors.iter().map(|m| m.y).min()?;
    let right = monitors.iter().map(|m| m.x + m.width as i32).max()?;
    let bottom = monitors.iter().map(|m| m.y + m.height as i32).max()?;
    if (right - left, bottom - top) != (screenshot.0 as i32, screenshot.1 as i32) {
        return None;
    }
    Some(Area {
        x: target.x - left,
        y: target.y - top,
        ..target
    })
}

pub fn crop(image: &Image, area: Area) -> Option<Image> {
    let (x, y) = (u32::try_from(area.x).ok()?, u32::try_from(area.y).ok()?);
    if area.width == 0
        || area.height == 0
        || x + area.width > image.width
        || y + area.height > image.height
    {
        return None;
    }
    let mut out = Image::solid(area.width, area.height, [0, 0, 0, 255]);
    let row = area.width as usize * 4;
    for r in 0..area.height as usize {
        let from = ((y as usize + r) * image.width as usize + x as usize) * 4;
        out.pixels[r * row..(r + 1) * row].copy_from_slice(&image.pixels[from..from + row]);
    }
    out.left = image.left + area.x;
    out.top = image.top + area.y;
    Some(out)
}

/// Where a reopened frame of `size`, saved at `origin`, goes on `monitors`: its corner on
/// the desktop, and the monitor it fills exactly, if any. Without one it takes a window of
/// its size, which cannot span monitors here: `None` when no monitor shows the frame whole.
/// Positions count from the corner of all monitors, as `crop` numbers a capture.
pub fn place(
    size: (u32, u32),
    origin: (i32, i32),
    monitors: &[Area],
) -> Option<((i32, i32), Option<usize>)> {
    let left = monitors.iter().map(|m| m.x).min()?;
    let top = monitors.iter().map(|m| m.y).min()?;
    let point = |x: f64, y: f64| Point { x, y };
    let screens: Vec<Rect> = monitors
        .iter()
        .map(|m| {
            let min = point((m.x - left) as f64, (m.y - top) as f64);
            Rect {
                min,
                max: min + point(m.width as f64, m.height as f64),
            }
        })
        .collect();
    let size = point(size.0 as f64, size.1 as f64);
    let origin = point(origin.0 as f64, origin.1 as f64);
    // No pointer before a window exists: the first monitor stands in for its screen.
    let pointer = screens.first()?.center();
    let corner = placement(size, origin, &screens, pointer)?;
    let home = Rect {
        min: corner,
        max: corner + size,
    };
    let filled = screens.iter().position(|s| *s == home);
    let holds = |s: &Rect| s.size().x >= size.x && s.size().y >= size.y;
    (filled.is_some() || screens.iter().any(holds))
        .then_some(((corner.x as i32, corner.y as i32), filled))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn area(x: i32, y: i32, width: u32, height: u32) -> Area {
        Area {
            x,
            y,
            width,
            height,
        }
    }
    #[test]
    fn second_monitor_is_cut_out_of_the_desktop() {
        let monitors = [area(0, 0, 1920, 1080), area(1920, -200, 1280, 1024)];
        let crop = crop_for((3200, 1280), &monitors, monitors[1]).unwrap();
        assert_eq!(crop, area(1920, 0, 1280, 1024));
        assert_eq!(
            crop_for((3200, 1280), &monitors, monitors[0]),
            Some(area(0, 200, 1920, 1080))
        );
    }
    #[test]
    fn single_or_mismatched_layouts_keep_the_whole_image() {
        let one = [area(0, 0, 1920, 1080)];
        assert_eq!(crop_for((1920, 1080), &one, one[0]), None);
        let mixed = [area(0, 0, 3840, 2160), area(3840, 0, 1920, 1080)];
        assert_eq!(crop_for((4800, 1080), &mixed, mixed[1]), None);
    }
    #[test]
    fn crop_copies_rows_and_keeps_desktop_origin() {
        let mut image = Image::solid(4, 3, [0, 0, 0, 255]);
        image.pixels[(2 * 4 + 3) * 4] = 9;
        let out = crop(&image, area(2, 1, 2, 2)).unwrap();
        assert_eq!((out.width, out.height, out.left, out.top), (2, 2, 2, 1));
        assert_eq!(out.pixels[(2 + 1) * 4], 9);
        assert!(crop(&image, area(3, 0, 2, 2)).is_none());
    }
    #[test]
    fn a_reopened_frame_fills_a_monitor_of_its_size_or_takes_a_window() {
        let full = (1920, 1080);
        let one = [area(0, 0, 1920, 1080)];
        assert_eq!(place(full, (0, 0), &one), Some(((0, 0), Some(0))));
        // Saved on another desktop: still a monitor of its size.
        assert_eq!(place(full, (5000, -300), &one), Some(((0, 0), Some(0))));
        // The monitor it was captured on comes first, wherever the desktop starts.
        let two = [area(-1920, 0, 1920, 1080), area(0, 0, 1920, 1080)];
        assert_eq!(place(full, (1920, 0), &two), Some(((1920, 0), Some(1))));
        assert_eq!(place(full, (0, 0), &two), Some(((0, 0), Some(0))));
        // Smaller than the monitor: a window of its size, no monitor filled.
        assert_eq!(place((640, 400), (0, 0), &one), Some(((640, 340), None)));
        let mixed = [area(0, 0, 1280, 720), area(1280, 0, 2560, 1440)];
        assert_eq!(place(full, (0, 0), &mixed).map(|p| p.1), Some(None));
    }
    #[test]
    fn a_frame_no_monitor_shows_whole_is_refused() {
        let two = [area(0, 0, 1920, 1080), area(1920, 0, 1920, 1080)];
        // The two monitors together would hold it, but no window spans them.
        assert_eq!(place((3000, 1000), (0, 0), &two), None);
        assert_eq!(place((3840, 1080), (0, 0), &two), None);
        assert_eq!(place((1936, 1080), (0, 0), &two[..1]), None);
        assert_eq!(place((1920, 1080), (0, 0), &[]), None);
        // A monitor not described yet holds nothing.
        assert_eq!(place((640, 400), (0, 0), &[area(0, 0, 0, 0)]), None);
    }
}
