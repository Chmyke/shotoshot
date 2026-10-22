use super::*;

impl Session {
    pub(super) fn paint_contents(&self) -> Result<()> {
        let capture = &self.capture;
        let renderer = &self.native.renderer;
        if capture.zoomed() {
            // Each screen through its own view, without the scene checkpoints, which only
            // apply at the identity transform.
            for view in &capture.views {
                renderer.push_view(view);
                let scene = crate::render::D2dCanvas::direct(renderer, &renderer.dc).and_then(
                    |mut canvas| {
                        let smooth = view.zoomed();
                        renderer.paint_backdrop(smooth);
                        self.paint_scene(&mut canvas, capture.ui_scale() / view.zoom, smooth)
                    },
                );
                renderer.pop_view();
                scene?;
            }
        } else {
            let mut canvas = crate::render::D2dCanvas::new(renderer, &renderer.dc)?;
            self.paint_scene(&mut canvas, capture.ui_scale(), false)?;
        }
        let mut canvas = crate::render::D2dCanvas::direct(renderer, &renderer.dc)?;
        self.paint_chrome(&mut canvas)
    }
    /// What a view transforms: shade, background, notes, the text being typed, the icons over
    /// them, then the selection chrome at `scale`. `begin` painted the backdrop at 1:1; a
    /// zoomed pass paints its own, `smooth`.
    fn paint_scene(
        &self,
        canvas: &mut crate::render::D2dCanvas,
        scale: f64,
        smooth: bool,
    ) -> Result<()> {
        let capture = &self.capture;
        capture.draw_underlay(canvas)?;
        if capture.region.is_some() {
            let editing = self.native.text.as_ref().and_then(|t| t.object);
            let result = capture.draw_annotations(canvas, editing);
            result.and_then(|_| self.native.draw_text_input(scale))?;
            // What the desktop lays over the notes (a board's icons) goes over them here too,
            // under the editor's own chrome.
            self.native.renderer.paint_overlay(smooth);
            capture.draw_frame_scaled(canvas, self.native.text.is_none(), scale)?;
        }
        Ok(())
    }
    /// What stays in screen coordinates: toolbar, pointer feedback, picker, fields, hints.
    fn paint_chrome(&self, canvas: &mut crate::render::D2dCanvas) -> Result<()> {
        let capture = &self.capture;
        if capture.region.is_some() {
            if !capture.selecting {
                capture.draw_toolbar(canvas, self.toolbar_color(), std::time::Instant::now())?;
                self.draw_pointer_feedback(canvas)?;
                if let Some(picker) = &self.capture.picker {
                    picker.draw(canvas)?;
                }
            }
            self.native.draw_dimensions(&self.capture, canvas)?;
            capture.draw_hover_hint(
                canvas,
                std::time::Instant::now(),
                self.native.dimensions.is_some(),
            )?;
        }
        capture.draw_selection_hint(canvas, self.pointer_monitor())
    }

    /// The colour the toolbar marks: the wheel's while it is open, the text's while typing,
    /// else the drawing colour.
    pub(crate) fn toolbar_color(&self) -> [u8; 3] {
        let capture = &self.capture;
        capture
            .picker
            .as_ref()
            .map(|p| p.color())
            .unwrap_or_else(|| {
                self.native
                    .text
                    .as_ref()
                    .map(|t| t.color)
                    .unwrap_or(capture.editor.style.color)
            })
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
