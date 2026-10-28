//! ROLE: The native side of a capture session, as seen by the portable interaction logic.
//! OWNER: Each platform implements it over its window, text field and crop-size fields.
//! INVARIANTS: Methods that edit the document receive the capture they act on.
use crate::Capture;
use shotoshot_core::{Label, Point};

pub trait Host {
    type Error;
    /// Keep receiving pointer events outside the window until `release_pointer`.
    fn capture_pointer(&mut self);
    fn release_pointer(&mut self);
    /// Return keyboard focus to the active inline field, if any.
    fn focus_text(&mut self);

    /// Whether an in-place text field is open.
    fn text_active(&self) -> bool;
    fn set_text_color(&mut self, rgb: [u8; 3]);
    /// Abandons a selection drag inside the text field; true if one was in progress.
    fn cancel_text_drag(&mut self) -> bool;
    fn begin_text(&mut self, capture: &mut Capture, p: Point) -> Result<(), Self::Error>;
    fn finish_text(&mut self, capture: &mut Capture, commit: bool);
    /// True when the press belongs to the open text field.
    fn text_pointer_down(&mut self, p: Point) -> Result<bool, Self::Error>;
    fn text_pointer_move(&mut self, p: Point) -> bool;
    fn text_pointer_up(&mut self) -> bool;
    /// Font size of the open field, if any.
    fn text_size(&self) -> Option<f32>;
    /// Gives the open field's font a new size; nothing without a field.
    fn set_text_size(&mut self, size: f32) -> Result<(), Self::Error>;
    /// Moves the open field's first line to `position` and sets the panel it sits on;
    /// nothing without a field. A new field takes its panel from `Capture::new_label`.
    fn set_text_label(&mut self, position: Point, label: Option<Label>);
    /// Extent of committed annotation text at `size`, as the renderer lays it out.
    fn measure_text(&mut self, text: &str, size: f64) -> Result<Point, Self::Error>;
    /// `measure_text` with lines also broken at `width`, as a wrapped label lays them out.
    fn measure_wrapped_text(
        &mut self,
        text: &str,
        size: f64,
        width: f64,
    ) -> Result<Point, Self::Error>;
    /// Extent of a text object's lines: wrapped at its label's width when it has one.
    fn measure_text_object(
        &mut self,
        text: &str,
        size: f64,
        label: Option<Label>,
    ) -> Result<Point, Self::Error> {
        match label.and_then(Label::wrap) {
            Some(width) => self.measure_wrapped_text(text, size, width),
            None => self.measure_text(text, size),
        }
    }

    /// Whether the crop-size fields are open.
    fn dimensions_active(&self) -> bool;
    fn begin_dimensions(&mut self, capture: &mut Capture) -> Result<(), Self::Error>;
    /// Applies or discards the fields; false keeps them open (invalid entry).
    fn finish_dimensions(&mut self, capture: &mut Capture, commit: bool) -> bool;
    fn dimension_pointer_down(
        &mut self,
        capture: &mut Capture,
        p: Point,
    ) -> Result<bool, Self::Error>;
    fn dimension_pointer_move(&mut self, p: Point);
    fn dimension_pointer_up(&mut self) -> bool;
    /// Consumes a key while the crop-size fields are open; true when it was theirs.
    fn dimension_key(&mut self, capture: &mut Capture, key: u32) -> bool;
}
