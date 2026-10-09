//! ROLE: Portable painting of the capture overlay: shade, region frame, selection,
//! toolbar, stroke-size ring and the initial hint. Native widgets paint themselves.
use crate::{Canvas, Capture, Ink, region_handles, toolbar};
use shotoshot_core::{Point, Rect, Tool};

const ACCENT: Ink = Ink::solid([100, 170, 255]);
const SHADE: Ink = Ink::with_alpha([0, 0, 0], 0.48);
const DARK: Ink = Ink::solid([32, 32, 32]);

fn square(center: Point, half: f64) -> Rect {
    Rect {
        min: center - Point { x: half, y: half },
        max: center + Point { x: half, y: half },
    }
}

impl Capture {
    fn screen(&self) -> Rect {
        Rect {
            min: Point::default(),
            max: Point {
                x: self.image.width as f64,
                y: self.image.height as f64,
            },
        }
    }
    /// Dims everything outside the region, or the whole frame before a selection.
    pub fn draw_shade<C: Canvas>(&self, canvas: &mut C) -> Result<(), C::Error> {
        let screen = self.screen();
        let Some(r) = self.region else {
            return canvas.fill_rect(screen, SHADE);
        };
        for outside in [
            Rect {
                min: screen.min,
                max: Point {
                    x: screen.max.x,
                    y: r.min.y,
                },
            },
            Rect {
                min: Point { x: 0., y: r.max.y },
                max: screen.max,
            },
            Rect {
                min: Point { x: 0., y: r.min.y },
                max: Point {
                    x: r.min.x,
                    y: r.max.y,
                },
            },
            Rect {
                min: Point {
                    x: r.max.x,
                    y: r.min.y,
                },
                max: Point {
                    x: screen.max.x,
                    y: r.max.y,
                },
            },
        ] {
            canvas.fill_rect(outside, SHADE)?;
        }
        Ok(())
    }
    /// Region border and handles, then the object selection unless text is being edited.
    pub fn draw_frame<C: Canvas>(
        &self,
        canvas: &mut C,
        show_selection: bool,
    ) -> Result<(), C::Error> {
        let Some(r) = self.region else {
            return Ok(());
        };
        canvas.stroke_rect(r, ACCENT, 1.)?;
        for p in region_handles(r) {
            canvas.fill_rect(square(p, 4.), Ink::solid([250, 250, 250]))?;
            canvas.stroke_rect(square(p, 4.), DARK, 1.5)?;
        }
        if show_selection && let Some(frame) = self.editor.selection_frame() {
            if frame.angle == 0. {
                canvas.stroke_rect(frame.bounds.expanded(3.), ACCENT, 1.)?;
            } else {
                // A single turned object keeps a frame turned with it.
                let outline = frame.outline(3.);
                for (i, a) in outline.iter().enumerate() {
                    canvas.line(*a, outline[(i + 1) % 4], ACCENT, 1.)?;
                }
            }
            for p in frame.corners() {
                canvas.fill_rect(square(p, 4.), ACCENT)?;
            }
            if let Some(handle) = self.rotation_handle() {
                canvas.line(frame.top(), handle, ACCENT, 1.)?;
                let radius = Point { x: 6., y: 6. };
                canvas.fill_ellipse(handle, radius, DARK)?;
                canvas.stroke_ellipse(handle, radius, ACCENT, 2.)?;
            }
        }
        if let Some(m) = self.editor.marquee() {
            canvas.stroke_rect(m, ACCENT, 1.)?;
        }
        Ok(())
    }
    pub fn draw_toolbar<C: Canvas>(
        &self,
        canvas: &mut C,
        current_color: [u8; 3],
    ) -> Result<(), C::Error> {
        let buttons = self.buttons();
        toolbar::draw(
            canvas,
            &buttons,
            self.editor.tool,
            current_color,
            self.pointer,
            self.eyedropper,
        )
    }
    /// Paint hints last so badges and panels cannot cover them after edge clamping.
    pub fn draw_hover_hint<C: Canvas>(
        &self,
        canvas: &mut C,
        now: std::time::Instant,
        editing_dimensions: bool,
    ) -> Result<(), C::Error> {
        if self.selecting || self.picker.is_some() {
            return Ok(());
        }
        let Some((action, bounds, opacity)) = self.hover_hint.visible(now) else {
            return Ok(());
        };
        if editing_dimensions && action == toolbar::Action::Dimensions {
            return Ok(());
        }
        if let Some(button) = self
            .buttons()
            .into_iter()
            .chain(self.screen_button())
            .chain(self.dimensions_button())
            .find(|b| b.action == action && b.bounds == bounds)
        {
            toolbar::draw_hint(canvas, &button, self.screen_at(self.pointer), opacity)?;
        }
        Ok(())
    }
    /// Outline of the current stroke width at the pointer; with a width change, its size.
    pub fn draw_width_ring<C: Canvas>(&self, canvas: &mut C) -> Result<(), C::Error> {
        let width = self.editor.style.width
            * if self.editor.tool == Tool::Highlight {
                5.
            } else {
                1.
            };
        let center = self.pointer;
        let radius = Point {
            x: width / 2.,
            y: width / 2.,
        };
        canvas.stroke_ellipse(center, radius, Ink::solid([25, 25, 25]), 3.)?;
        canvas.stroke_ellipse(center, radius, Ink::solid([250, 250, 250]), 1.)?;
        if !self.width_preview {
            return Ok(());
        }
        let min = Point {
            x: (center.x + width / 2. + 10.)
                .min(self.image.width as f64 - 72.)
                .max(0.),
            y: (center.y + 10.).min(self.image.height as f64 - 24.).max(0.),
        };
        let bounds = Rect {
            min,
            max: min + Point { x: 72., y: 24. },
        };
        canvas.fill_round_rect(bounds, 4., Ink::solid([25, 25, 25]))?;
        canvas.text(
            &format!("{width:.0} px"),
            Rect {
                min: min + Point { x: 7., y: 3. },
                max: bounds.max,
            },
            12.,
            Ink::solid([250, 250, 250]),
        )
    }
    pub fn awaiting_region(&self) -> bool {
        self.selecting && self.region_origin.is_none() && self.region.is_none()
    }
    /// Guidance shown before the first selection gesture, kept on the pointer's monitor.
    pub fn draw_selection_hint<C: Canvas>(
        &self,
        canvas: &mut C,
        monitor: Rect,
    ) -> Result<(), C::Error> {
        if !self.awaiting_region() || self.pointer.x < 0. || self.pointer.y < 0. {
            return Ok(());
        }
        let title = "Select an area";
        let extent = canvas.measure_text(title, 13.)?;
        let screen = self.screen();
        let screen = Rect {
            min: Point {
                x: monitor.min.x.max(screen.min.x),
                y: monitor.min.y.max(screen.min.y),
            },
            max: Point {
                x: monitor.max.x.min(screen.max.x),
                y: monitor.max.y.min(screen.max.y),
            },
        };
        let bounds = place_hint(
            self.pointer,
            screen,
            Point {
                x: extent.x + 20.,
                y: 48.,
            },
        );
        canvas.fill_round_rect(bounds, 5., Ink::with_alpha([25, 25, 28], 0.94))?;
        canvas.text(
            title,
            Rect {
                min: bounds.min + Point { x: 10., y: 6. },
                max: bounds.max,
            },
            13.,
            Ink::solid([245, 245, 245]),
        )?;
        canvas.text(
            "Esc to cancel",
            Rect {
                min: bounds.min + Point { x: 10., y: 26. },
                max: bounds.max,
            },
            11.,
            Ink::solid([180, 183, 188]),
        )
    }
    /// Crop-size badge and entry error. While `editing`, the
    /// platform draws the two size fields over it, on each side of the "×".
    pub fn draw_dimensions<C: Canvas>(
        &self,
        canvas: &mut C,
        editing: bool,
        error: Option<&str>,
    ) -> Result<(), C::Error> {
        let (Some(bounds), Some(region)) = (self.dimensions_bounds(), self.region) else {
            return Ok(());
        };
        let hover = bounds.contains(self.pointer) && !self.selecting;
        let background = if hover {
            Ink::solid([48, 57, 70])
        } else {
            DARK
        };
        canvas.fill_round_rect(bounds, 4., background)?;
        let (label, x) = if editing {
            ("×".to_owned(), bounds.size().x / 2. - 4.)
        } else {
            let size = region.size();
            (format!("{} × {}", size.x as i32, size.y as i32), 8.)
        };
        let label_area = Rect {
            min: bounds.min + Point { x, y: 4. },
            max: bounds.max,
        };
        canvas.text(&label, label_area, 13., Ink::solid([240, 240, 240]))?;
        if let Some(button) = self.screen_button_bounds() {
            self.draw_screen_button(canvas, button, button.contains(self.pointer))?;
        }
        let Some(message) = error else {
            return Ok(());
        };
        let area = toolbar::tooltip_bounds(
            bounds,
            self.screen_at(self.pointer),
            Point { x: 460., y: 48. },
        );
        canvas.fill_rect(area, background)?;
        let text_area = Rect {
            min: area.min + Point { x: 6., y: 4. },
            max: area.max,
        };
        canvas.text(message, text_area, 12., Ink::solid([255, 160, 150]))
    }
    /// Four diagonal arrows: outward to fill the screen, inward once it is filled.
    fn draw_screen_button<C: Canvas>(
        &self,
        canvas: &mut C,
        bounds: Rect,
        hover: bool,
    ) -> Result<(), C::Error> {
        let filled = self.screen_filled();
        let background = if filled {
            Ink::solid([38, 78, 126])
        } else if hover {
            Ink::solid([48, 57, 70])
        } else {
            DARK
        };
        canvas.fill_round_rect(bounds, 4., background)?;
        let ink = Ink::solid([240, 240, 240]);
        let center = bounds.center();
        for (x, y) in [(-1., -1.), (1., -1.), (1., 1.), (-1., 1.)] {
            let near = center
                + Point {
                    x: x * 3.,
                    y: y * 3.,
                };
            let far = center
                + Point {
                    x: x * 8.,
                    y: y * 8.,
                };
            let (tail, tip, sign) = if filled {
                (far, near, -1.)
            } else {
                (near, far, 1.)
            };
            canvas.line(tail, tip, ink, 1.6)?;
            let head = 4. * sign;
            canvas.line(tip, tip - Point { x: x * head, y: 0. }, ink, 1.6)?;
            canvas.line(tip, tip - Point { x: 0., y: y * head }, ink, 1.6)?;
        }
        Ok(())
    }
}
const NOTICE_SIZE: f64 = 13.;
const NOTICE_PADDING: Point = Point { x: 12., y: 8. };

/// Size of the box `draw_notice` paints for `message`.
pub fn notice_size<C: Canvas>(canvas: &mut C, message: &str) -> Result<Point, C::Error> {
    Ok(canvas.measure_text(message, NOTICE_SIZE)? + NOTICE_PADDING * 2.)
}
/// An error message box filling `bounds` (see `notice_size`), for failures shown on
/// platforms without a native message window.
pub fn draw_notice<C: Canvas>(canvas: &mut C, bounds: Rect, message: &str) -> Result<(), C::Error> {
    canvas.fill_round_rect(bounds, 5., Ink::with_alpha([25, 25, 28], 0.94))?;
    let text = Rect {
        min: bounds.min + NOTICE_PADDING,
        max: bounds.max,
    };
    canvas.text(message, text, NOTICE_SIZE, Ink::solid([255, 160, 150]))
}
/// `size` centred horizontally near the top of `screen`, kept inside it.
pub fn top_center(screen: Rect, size: Point) -> Rect {
    let width = size.x.min(screen.size().x - 8.).max(0.);
    let min = Point {
        x: screen.center().x - width / 2.,
        y: screen.min.y + 24.,
    };
    Rect {
        min,
        max: min
            + Point {
                x: width,
                y: size.y,
            },
    }
}
pub fn place_hint(pointer: Point, screen: Rect, size: Point) -> Rect {
    let place = |p: f64, min: f64, max: f64, size: f64| {
        let preferred = if p + 20. + size <= max - 4. {
            p + 20.
        } else {
            p - 20. - size
        };
        preferred.clamp(min + 4., (max - size - 4.).max(min + 4.))
    };
    let min = Point {
        x: place(pointer.x, screen.min.x, screen.max.x, size.x),
        y: place(pointer.y, screen.min.y, screen.max.y, size.y),
    };
    Rect {
        min,
        max: min + size,
    }
}
