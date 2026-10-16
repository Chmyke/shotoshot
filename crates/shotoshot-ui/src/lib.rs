//! ROLE: Platform-independent capture interface: layout, interaction state and drawing
//! through `Canvas`. No operating system calls live here.
mod canvas;
mod capture;
pub mod capture_name;
pub mod caret;
pub mod color_picker;
pub mod crop;
pub mod cursor_image;
mod dimension_fields;
mod document;
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
mod view;
pub mod wheel;
pub use canvas::{Canvas, Face, Ink, annotation_font};
pub use capture::{BADGE_LABEL_SIZE, BadgeText, Capture, CursorKind, Monitor, region_handles};
pub use dimension_fields::{DimensionFields, DimensionKey};
pub use document::placement;
pub use host::Host;
pub use image::Image;
pub use overlay::{draw_notice, notice_size, place_hint, top_center};
pub use text_field::{FieldKey, TextField};
pub use view::{MAX_ZOOM, View};
