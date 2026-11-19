use super::*;
use shotoshot_ui::Canvas;

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
                        self.paint_scene(&mut canvas, capture.ui_scale() / view.zoom, Some(smooth))
                    },
                );
                renderer.pop_view();
                scene?;
            }
        } else {
            let mut canvas = crate::render::D2dCanvas::new(renderer, &renderer.dc)?;
            self.paint_scene(&mut canvas, capture.ui_scale(), Some(false))?;
        }
        let mut canvas = crate::render::D2dCanvas::direct(renderer, &renderer.dc)?;
        self.paint_chrome(&mut canvas, true)
    }
    /// The fingerprint of the frame `paint_contents` would paint now
    /// (`shotoshot_ui::fingerprint`), or `None` when part of it escapes the canvas: zoomed
    /// views, the live eyedropper (its loupe reads the desktop again at each move), native
    /// text fields, or a gesture editing the pile, which the fingerprint only summarises.
    /// Live capture otherwise qualifies: the desktop shows through, it is never painted.
    pub(crate) fn frame_fingerprint(&self) -> Result<Option<u64>> {
        let capture = &self.capture;
        if capture.zoomed()
            || (capture.live && capture.eyedropper)
            || self.native.text.is_some()
            || self.native.dimensions.is_some()
            || capture.editor.is_drawing()
        {
            return Ok(None);
        }
        let renderer = &self.native.renderer;
        let mut measure = crate::render::D2dCanvas::direct(renderer, &renderer.dc)?;
        let mut canvas = shotoshot_ui::fingerprint::Fingerprint::new(&mut measure);
        self.paint_scene(&mut canvas, capture.ui_scale(), None)?;
        self.paint_chrome(&mut canvas, false)?;
        Ok(Some(canvas.finish()))
    }
    /// What a view transforms: shade, background, notes, the icons over them, the text being
    /// typed, then the selection chrome at `scale`. `begin` painted the backdrop at 1:1; a
    /// zoomed pass paints its own, `smooth`. `native: None` draws only what goes through the
    /// canvas (a fingerprint); `Some(smooth)` adds the renderer's own layers.
    fn paint_scene<C: Canvas<Error = Error>>(
        &self,
        canvas: &mut C,
        scale: f64,
        native: Option<bool>,
    ) -> Result<()> {
        let capture = &self.capture;
        capture.draw_underlay(canvas)?;
        if capture.region.is_some() {
            let editing = self.native.text.as_ref().and_then(|t| t.object);
            capture.draw_annotations(canvas, editing)?;
            // What the desktop lays over the notes (a board's icons) goes over them here too,
            // under the editor's own chrome; the text being typed stays above, readable
            // (blind test of 2026-11-05: icons crossed it).
            if let Some(smooth) = native {
                self.native.renderer.paint_overlay(smooth);
                self.native.draw_text_input(scale)?;
            }
            capture.draw_frame_scaled(canvas, self.native.text.is_none(), scale)?;
        }
        Ok(())
    }
    /// What stays in screen coordinates: toolbar, pointer feedback, picker, fields, hints.
    /// `native` adds the native fields, which bypass the canvas.
    fn paint_chrome<C: Canvas<Error = Error>>(&self, canvas: &mut C, native: bool) -> Result<()> {
        let capture = &self.capture;
        if capture.region.is_some() {
            if !capture.selecting {
                capture.draw_toolbar(
                    canvas,
                    self.toolbar_color(),
                    self.field_ink(),
                    std::time::Instant::now(),
                )?;
                self.draw_pointer_feedback(canvas)?;
                if let Some(picker) = &self.capture.picker {
                    picker.draw(canvas)?;
                }
            }
            self.native.draw_dimensions(&self.capture, canvas, native)?;
            capture.draw_hover_hint(
                canvas,
                std::time::Instant::now(),
                self.native.dimensions.is_some(),
            )?;
        }
        capture.draw_selection_hint(canvas, self.pointer_monitor())
    }

    /// The open text field's ink on its panel (`Some(None)`: automatic), `None` without one.
    pub(crate) fn field_ink(&self) -> Option<Option<[u8; 3]>> {
        self.native.text.as_ref().map(|t| t.ink)
    }
    /// The colour the toolbar marks: see `Capture::toolbar_color`.
    pub(crate) fn toolbar_color(&self) -> [u8; 3] {
        self.capture
            .toolbar_color(self.native.text.as_ref().map(|t| t.color))
    }

    fn draw_pointer_feedback<C: Canvas<Error = Error>>(&self, canvas: &mut C) -> Result<()> {
        let capture = &self.capture;
        if self.capture.picker.is_some()
            || (self.native.text.is_some() && !capture.eyedropper)
            || (capture.toolbar_contains(capture.pointer) && !capture.eyedropper)
        {
            return Ok(());
        }
        if capture.eyedropper {
            capture.draw_magnifier(canvas)
        } else {
            if capture.width_ring_visible() {
                capture.draw_width_ring(canvas)?;
            }
            capture.draw_wheel_hint(canvas)
        }
    }
}
