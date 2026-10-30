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
pub mod icon;
mod image;
mod interaction;
pub mod keys;
mod magnifier;
pub use magnifier::{
    CELLS as MAGNIFIER_CELLS, HINT_ROOM as MAGNIFIER_HINT_ROOM, HINT_SIZE as MAGNIFIER_HINT_SIZE,
    hint_in as magnifier_hint_in,
};
mod overlay;
pub mod pointer;
#[cfg(test)]
mod scale_tests;
pub mod settings_panel;
mod shortcut_label;
pub use shortcut_label::{shortcut_label, shortcut_label_in};
#[cfg(test)]
mod test_host;
mod text_field;
pub mod toolbar;
mod view;
pub mod wheel;
pub use canvas::{Canvas, Face, Ink, TextStyle, annotation_font};
pub use capture::{BADGE_LABEL_SIZE, BadgeText, Capture, CursorKind, Monitor, region_handles};
pub use dimension_fields::{DimensionFields, DimensionKey};
pub use document::placement;
pub use host::Host;
pub use image::Image;
pub use overlay::{
    ENTRY_ERROR_SIZE, SELECTION_HINT_NOTE_SIZE, SELECTION_HINT_TITLE_SIZE, draw_notice,
    entry_error_size, notice_size, place_hint, selection_hint_lines_in, selection_hint_size,
    top_center,
};
pub use text_field::{FieldKey, TextField};
pub use view::{MAX_ZOOM, View};
