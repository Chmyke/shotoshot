//! ROLE: Session gestures for clipboard annotations, rotation handles and text-size wheel.
use super::*;
use windows::Win32::UI::Controls::{EM_GETSEL, EM_SETSEL};

impl Native {
    /// Gives the open text field's font a new size, keeping its selection.
    pub(super) fn set_text_size(&mut self, hwnd: HWND, size: f32) -> Result<()> {
        let Some(input) = &mut self.text else {
            return Ok(());
        };
        unsafe {
            let (mut start, mut end) = (0u32, 0u32);
            SendMessageW(
                input.window,
                EM_GETSEL,
                Some(WPARAM((&mut start as *mut u32) as usize)),
                Some(LPARAM((&mut end as *mut u32) as isize)),
            );
            let font = super::text::configure_input(input.window, size)?;
            let _ = DeleteObject(input.font.into());
            input.font = font;
            input.size = size;
            SendMessageW(
                input.window,
                EM_SETSEL,
                Some(WPARAM(start as usize)),
                Some(LPARAM(end as isize)),
            );
        }
        self.text_event_updated(hwnd);
        Ok(())
    }
}
impl Session {
    pub fn copy_annotations(&mut self, hwnd: HWND) -> Result<bool> {
        let objects = self.capture.editor.copy_selected();
        if objects.is_empty() {
            return Ok(false);
        }
        let sequence = crate::copy_annotation_marker(hwnd)?;
        self.capture.copied_objects = objects;
        self.capture.copied_sequence = sequence;
        self.capture.paste_count = 0;
        Ok(true)
    }
    pub fn paste_annotations(&mut self) {
        if !crate::annotation_clipboard_matches(self.capture.copied_sequence) {
            return;
        }
        self.capture.paste_count = self.capture.paste_count.saturating_add(1);
        let offset = 16. * self.capture.paste_count as f64;
        if self.capture.editor.paste_objects(
            &self.capture.copied_objects,
            Point {
                x: offset,
                y: offset,
            },
        ) {
            self.capture.editor.tool = Tool::Select;
        }
    }
    pub fn wheel(&mut self, hwnd: HWND, delta: i16) -> Result<()> {
        let (capture, mut host) = self.host(hwnd);
        capture.wheel(&mut host, delta)
    }
}

#[cfg(test)]
mod tests {
    use super::super::*;
    #[test]
    fn one_wheel_gesture_resizes_all_selected_texts_in_one_undo_step() -> Result<()> {
        let _com = ComApartment::new()?;
        let mut s = Session::new(Image::solid(500, 300, [0, 0, 0, 255]))?;
        s.capture
            .editor
            .add_text(Point { x: 20., y: 30. }, "First".into());
        s.capture
            .editor
            .add_text(Point { x: 120., y: 130. }, "Second".into());
        s.capture.editor.selected = s.capture.editor.objects().iter().map(|o| o.id).collect();
        let original = s.capture.editor.objects().to_vec();
        s.wheel(HWND::default(), 120)?;
        assert!(
            s.capture
                .editor
                .objects()
                .iter()
                .all(|o| matches!(o.shape, Shape::Text { size: 28., .. }))
        );
        s.capture.editor.undo();
        assert_eq!(s.capture.editor.objects(), original);
        Ok(())
    }

    #[test]
    fn wheel_size_ring_stays_on_pointer_and_hides_over_toolbar() -> Result<()> {
        let _com = ComApartment::new()?;
        let mut s = Session::new(Image::solid(900, 600, [0, 0, 0, 255]))?;
        s.capture.region = Rect::new(Point { x: 100., y: 100. }, Point { x: 600., y: 400. });
        s.capture.selecting = false;
        s.capture.editor.tool = Tool::Rectangle;
        s.capture.pointer = Point { x: 300., y: 250. };
        s.wheel(HWND::default(), 120)?;
        assert!(s.capture.width_ring_visible());
        let button = s.capture.buttons()[0].bounds.center();
        s.mouse_move(HWND::default(), button);
        assert!(!s.capture.width_ring_visible());
        s.mouse_move(HWND::default(), Point { x: 300., y: 250. });
        assert!(s.capture.width_ring_visible());
        Ok(())
    }

    #[test]
    fn preset_and_cancelled_wheel_do_not_replace_last_custom_color() -> Result<()> {
        let _com = ComApartment::new()?;
        let mut s = Session::new(Image::solid(500, 300, [0, 0, 0, 255]))?;
        s.accept_custom_color(HWND::default(), [12, 34, 56]);
        s.action(HWND::default(), Action::Color([240, 20, 20]));
        s.action(HWND::default(), Action::CustomColor);
        let color = s.capture.picker.as_ref().unwrap().color();
        for (actual, expected) in color.into_iter().zip([12u8, 34, 56]) {
            assert!(actual.abs_diff(expected) <= 1);
        }
        s.cancel_gesture(HWND::default());
        assert_eq!(s.capture.custom_color, [12, 34, 56]);
        assert_eq!(s.capture.editor.style.color, [240, 20, 20]);
        Ok(())
    }

    #[test]
    fn wheel_edits_draft_text_size_and_commits_one_undo_step() -> Result<()> {
        let _com = ComApartment::new()?;
        let window = crate::resources::OwnedWindow(unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                w!("STATIC"),
                w!(""),
                WS_POPUP,
                0,
                0,
                500,
                300,
                None,
                None,
                None,
                None,
            )?
        });
        let mut s = Session::new(Image::solid(500, 300, [0, 0, 0, 255]))?;
        s.native
            .begin_text(&mut s.capture, window.0, Point { x: 20., y: 30. })?;
        unsafe {
            SetWindowTextW(s.native.text.as_ref().unwrap().window, w!("Hello"))?;
        }
        let stroke = s.capture.editor.style.width;
        s.wheel(window.0, 120)?;
        assert_eq!(s.native.text.as_ref().unwrap().size, 28.);
        assert_eq!(s.capture.editor.style.width, stroke);
        assert!(s.capture.editor.objects().is_empty());
        s.native.finish_text(&mut s.capture, window.0, true);
        assert!(matches!(
            s.capture.editor.objects()[0].shape,
            Shape::Text { size: 28., .. }
        ));
        s.capture.editor.undo();
        assert!(s.capture.editor.objects().is_empty());
        Ok(())
    }
}
