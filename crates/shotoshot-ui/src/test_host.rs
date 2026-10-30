//! Test double for `Host`: records native requests and owns no widgets.
use crate::{Capture, Host};
use shotoshot_core::{Label, Point};

#[derive(Default)]
pub struct FakeHost {
    pub captured: bool,
    pub text_at: Option<Point>,
    pub text_open: bool,
    pub text_size: f32,
    /// Where the open field's first line is and the panel it sits on.
    pub label: Option<(Point, Option<Label>)>,
    /// The open field's colour and its ink on a panel (`None`: automatic).
    pub color: Option<[u8; 3]>,
    pub ink: Option<[u8; 3]>,
}
impl Host for FakeHost {
    type Error = ();
    fn capture_pointer(&mut self) {
        self.captured = true;
    }
    fn release_pointer(&mut self) {
        self.captured = false;
    }
    fn focus_text(&mut self) {}
    fn text_active(&self) -> bool {
        self.text_open
    }
    fn set_text_color(&mut self, rgb: [u8; 3]) {
        self.color = Some(rgb);
    }
    fn set_text_ink(&mut self, ink: Option<[u8; 3]>) {
        self.ink = ink;
    }
    fn cancel_text_drag(&mut self) -> bool {
        false
    }
    fn begin_text(&mut self, capture: &mut Capture, p: Point) -> Result<(), ()> {
        self.text_at = Some(p);
        self.text_open = true;
        self.text_size = capture.text_size;
        self.label = Some((p, capture.new_label));
        self.color = Some(capture.editor.style.color);
        self.ink = capture.label_ink;
        Ok(())
    }
    fn finish_text(&mut self, _: &mut Capture, _: bool) {
        self.text_open = false;
    }
    fn text_pointer_down(&mut self, _: Point) -> Result<bool, ()> {
        Ok(false)
    }
    fn text_pointer_move(&mut self, _: Point) -> bool {
        false
    }
    fn text_pointer_up(&mut self) -> bool {
        false
    }
    fn text_size(&self) -> Option<f32> {
        self.text_open.then_some(self.text_size)
    }
    fn set_text_size(&mut self, size: f32) -> Result<(), ()> {
        self.text_size = size;
        Ok(())
    }
    fn set_text_label(&mut self, position: Point, label: Option<Label>) {
        if self.text_open {
            self.label = Some((position, label));
        }
    }
    fn measure_text(&mut self, text: &str, size: f64) -> Result<Point, ()> {
        Ok(Point {
            x: text.chars().count() as f64 * size * 0.5,
            y: size,
        })
    }
    fn measure_wrapped_text(&mut self, text: &str, size: f64, width: f64) -> Result<Point, ()> {
        let advance = |s: &str| s.chars().count() as f64 * size * 0.5;
        let lines = shotoshot_core::text_lines(text, Some(width), &mut |s| advance(s));
        Ok(Point {
            x: lines
                .iter()
                .map(|r| advance(text[r.clone()].trim_end()))
                .fold(0., f64::max),
            y: lines.len() as f64 * size,
        })
    }
    fn dimensions_active(&self) -> bool {
        false
    }
    fn begin_dimensions(&mut self, _: &mut Capture) -> Result<(), ()> {
        Ok(())
    }
    fn finish_dimensions(&mut self, _: &mut Capture, _: bool) -> bool {
        true
    }
    fn dimension_pointer_down(&mut self, _: &mut Capture, _: Point) -> Result<bool, ()> {
        Ok(false)
    }
    fn dimension_pointer_move(&mut self, _: Point) {}
    fn dimension_pointer_up(&mut self) -> bool {
        false
    }
    fn dimension_key(&mut self, _: &mut Capture, _: u32) -> bool {
        false
    }
}
