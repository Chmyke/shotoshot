//! ROLE: Direct root-window screenshot on X11 sessions (no portal needed, no dialog).
//! INVARIANTS: Only 32-bit ZPixmap visuals (all common X servers); anything else is refused.
use shotoshot_ui::Image;
use std::error::Error;
use x11rb::{
    connection::Connection,
    protocol::xproto::{ConnectionExt, ImageFormat},
};

pub fn screenshot() -> Result<Image, Box<dyn Error>> {
    let (connection, screen) = x11rb::connect(None)?;
    let setup = connection.setup();
    let root = &setup.roots[screen];
    let (width, height) = (root.width_in_pixels, root.height_in_pixels);
    let reply = connection
        .get_image(ImageFormat::Z_PIXMAP, root.root, 0, 0, width, height, !0)?
        .reply()?;
    let bits = setup
        .pixmap_formats
        .iter()
        .find(|f| f.depth == reply.depth)
        .map(|f| f.bits_per_pixel);
    if bits != Some(32) || setup.image_byte_order != x11rb::protocol::xproto::ImageOrder::LSB_FIRST
    {
        return Err("unsupported X11 pixel format".into());
    }
    let mut pixels = reply.data;
    if pixels.len() != width as usize * height as usize * 4 {
        return Err("unexpected X11 image size".into());
    }
    // Little-endian 32-bit pixels are B, G, R, X: the server's buffer is the image, once
    // its padding byte is made opaque.
    for pixel in pixels.as_chunks_mut::<4>().0 {
        pixel[3] = 255;
    }
    Ok(Image {
        width: width as u32,
        height: height as u32,
        pixels,
        left: 0,
        top: 0,
    })
}
