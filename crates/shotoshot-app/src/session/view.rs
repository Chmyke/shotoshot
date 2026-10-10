use super::*;

impl Session {
    pub(super) fn paint_contents(&self) -> Result<()> {
        let capture = &self.capture;
        let mut canvas =
            crate::render::D2dCanvas::new(&self.native.renderer, &self.native.renderer.dc)?;
        capture.draw_underlay(&mut canvas)?;
        if capture.region.is_some() {
            let editing = self.native.text.as_ref().and_then(|t| t.object);
            let result = canvas.annotations(capture.editor.objects(), editing);
            result.and_then(|_| self.native.draw_text_input())?;
            // What the desktop lays over the notes (a board's icons) goes over them here too,
            // under the editor's own chrome.
            self.native.renderer.paint_overlay();
            capture.draw_frame(&mut canvas, self.native.text.is_none())?;
            if !capture.selecting {
                let current = self
                    .capture
                    .picker
                    .as_ref()
                    .map(|p| p.color())
                    .unwrap_or_else(|| {
                        self.native
                            .text
                            .as_ref()
                            .map(|t| t.color)
                            .unwrap_or(capture.editor.style.color)
                    });
                capture.draw_toolbar(&mut canvas, current)?;
                self.draw_pointer_feedback(&mut canvas)?;
                if let Some(picker) = &self.capture.picker {
                    picker.draw(&mut canvas)?;
                }
            }
            self.native.draw_dimensions(&self.capture, &mut canvas)?;
            capture.draw_hover_hint(
                &mut canvas,
                std::time::Instant::now(),
                self.native.dimensions.is_some(),
            )?;
        }
        capture.draw_selection_hint(&mut canvas, self.pointer_monitor())
    }

    fn draw_pointer_feedback(&self, canvas: &mut crate::render::D2dCanvas) -> Result<()> {
        let capture = &self.capture;
        if self.capture.picker.is_some()
            || (self.native.text.is_some() && !capture.eyedropper)
            || (capture.toolbar_contains(capture.pointer) && !capture.eyedropper)
        {
            return Ok(());
        }
        if capture.eyedropper {
            capture.draw_magnifier(canvas)
        } else if capture.width_ring_visible() {
            capture.draw_width_ring(canvas)
        } else {
            Ok(())
        }
    }
}
