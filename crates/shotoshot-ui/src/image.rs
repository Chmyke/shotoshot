//! ROLE: A captured frame in memory, shared by every platform.
//! INVARIANTS: Top-down BGRA rows, 4 bytes per pixel; `left`/`top` place it on the desktop.
use shotoshot_core::Object;
use std::borrow::Cow;

#[derive(Clone)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
    pub left: i32,
    pub top: i32,
}
impl Image {
    /// Rejects empty, oversized or inconsistent buffers before any renderer reads them.
    pub fn check(&self) -> Result<(), &'static str> {
        let expected = (self.width as usize)
            .checked_mul(self.height as usize)
            .and_then(|n| n.checked_mul(4));
        if self.width == 0
            || self.height == 0
            || self.width > i32::MAX as u32 / 4
            || self.height > i32::MAX as u32
            || expected != Some(self.pixels.len())
        {
            return Err("Invalid BGRA image dimensions or buffer length");
        }
        Ok(())
    }
    pub fn solid(width: u32, height: u32, color: [u8; 4]) -> Self {
        Self {
            width,
            height,
            pixels: color.repeat(width as usize * height as usize),
            left: 0,
            top: 0,
        }
    }
    /// This frame with the capture under every blur of `objects` replaced, for good, by what
    /// the blur shows on screen: what exports and saved documents hold. Borrowed unchanged
    /// when no blur covers it.
    pub fn blurred(&self, objects: &[Object]) -> Cow<'_, Image> {
        if !objects.iter().any(Object::is_blur) || self.check().is_err() {
            return Cow::Borrowed(self);
        }
        let mut blurred = self.clone();
        shotoshot_core::blur::flatten(&mut blurred.pixels, self.width, self.height, objects);
        Cow::Owned(blurred)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn check_rejects_empty_and_mismatched_buffers() {
        assert!(Image::solid(3, 2, [1, 2, 3, 4]).check().is_ok());
        assert!(Image::solid(0, 2, [0; 4]).check().is_err());
        let mut short = Image::solid(3, 2, [0; 4]);
        short.pixels.pop();
        assert!(short.check().is_err());
    }
}
