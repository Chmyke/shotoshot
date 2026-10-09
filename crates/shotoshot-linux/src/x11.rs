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
    let mut image = Image::solid(width as u32, height as u32, [0, 0, 0, 255]);
    if reply.data.len() != image.pixels.len() {
        return Err("unexpected X11 image size".into());
    }
    // Little-endian 32-bit pixels are B, G, R, X: only the padding byte changes.
    for (to, from) in image
        .pixels
        .as_chunks_mut::<4>()
        .0
        .iter_mut()
        .zip(reply.data.as_chunks::<4>().0)
    {
        *to = [from[0], from[1], from[2], 255];
    }
    Ok(image)
}
