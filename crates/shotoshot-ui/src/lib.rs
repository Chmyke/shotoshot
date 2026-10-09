//! ROLE: Platform-independent capture interface: layout, interaction state and drawing
//! through `Canvas`. No operating system calls live here.
mod canvas;
mod capture;
pub mod capture_name;
pub mod color_picker;
pub mod crop;
mod dimension_fields;
mod host;
mod image;
mod interaction;
pub mod keys;
mod magnifier;
pub use magnifier::CELLS as MAGNIFIER_CELLS;
mod overlay;
#[cfg(test)]
mod scale_tests;
#[cfg(test)]
mod test_host;
mod text_field;
pub mod toolbar;
pub mod wheel;
pub use canvas::{Canvas, Ink};
pub use capture::{Capture, CursorKind, Monitor, region_handles};
pub use dimension_fields::{DimensionFields, DimensionKey};
pub use host::Host;
pub use image::Image;
pub use overlay::{draw_notice, notice_size, place_hint, top_center};
pub use text_field::{FieldKey, TextField};
