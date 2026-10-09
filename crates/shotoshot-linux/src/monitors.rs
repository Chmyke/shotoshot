//! ROLE: Finds the part of a whole-desktop screenshot that one monitor shows, so the
//! full-screen editor displays its own monitor at 1:1 instead of the squeezed desktop.
use shotoshot_ui::Image;

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
}
