use super::*;

/// The pile's ids, bottom first.
fn ids(objects: &[Object]) -> Vec<u64> {
    objects.iter().map(|o| o.id).collect()
}

impl Editor {
    pub(super) fn commit(&mut self, before: Vec<Object>) {
        if before == self.objects {
            return;
        }
        // PERF: History retains only changed objects, never full document snapshots or pixels.
        let removed = before
            .iter()
            .filter(|o| self.objects.iter().find(|n| n.id == o.id) != Some(o))
            .cloned()
            .collect();
        let added = self
            .objects
            .iter()
            .filter(|o| before.iter().find(|n| n.id == o.id) != Some(o))
            .cloned()
            .collect();
        // Undo swaps objects in and out at the end of the pile: the order puts them back.
        self.undo.push(Change {
            before: removed,
            after: added,
            order: Some(Stacking {
                before: ids(&before),
                after: ids(&self.objects),
            }),
            background: None,
        });
        self.redo.clear();
    }
    /// One reversible step of its own; an unchanged background adds none.
    pub(super) fn set_background(&mut self, background: Vec<Paint>) {
        if background == self.background {
            return;
        }
        self.undo.push(Change {
            before: Vec::new(),
            after: Vec::new(),
            order: None,
            background: Some(Repaint {
                before: std::mem::replace(&mut self.background, background.clone()),
                after: background,
            }),
        });
        self.redo.clear();
    }
    /// Swaps `remove` for `add`, then puts the pile back in `order` (bottom first) when the
    /// step recorded one; a step that only changed objects leaves their places alone.
    pub(super) fn apply(&mut self, remove: &[Object], add: &[Object], order: Option<&[u64]>) {
        self.objects
            .retain(|o| !remove.iter().any(|r| r.id == o.id));
        self.objects.extend_from_slice(add);
        if let Some(order) = order {
            self.objects
                .sort_by_key(|o| order.iter().position(|id| *id == o.id));
        }
        self.selected
            .retain(|id| self.objects.iter().any(|o| o.id == *id));
    }
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
    pub fn undo(&mut self) {
        self.cancel();
        if let Some(c) = self.undo.pop() {
            self.apply(&c.after, &c.before, c.order.as_ref().map(|s| &s.before[..]));
            if let Some(repaint) = &c.background {
                self.background = repaint.before.clone();
            }
            self.redo.push(c);
        }
    }
    pub fn redo(&mut self) {
        self.cancel();
        if let Some(c) = self.redo.pop() {
            self.apply(&c.before, &c.after, c.order.as_ref().map(|s| &s.after[..]));
            if let Some(repaint) = &c.background {
                self.background = repaint.after.clone();
            }
            self.undo.push(c);
        }
    }
    pub fn nudge(&mut self, delta: Point) {
        if !delta.finite() {
            return;
        }
        self.cancel();
        let before = self.objects.clone();
        for o in &mut self.objects {
            if self.selected.contains(&o.id) {
                o.prepare_transform(&before, &self.selected);
                o.translate(delta);
            }
        }
        self.commit(before);
    }
    /// Every object and the background gone, as one reversible step; nothing when there is
    /// nothing to clear.
    pub fn clear(&mut self) {
        self.cancel();
        if self.objects.is_empty() && self.background.is_empty() {
            return;
        }
        let before = std::mem::take(&mut self.objects);
        self.selected.clear();
        let background = (!self.background.is_empty()).then(|| Repaint {
            before: std::mem::take(&mut self.background),
            after: Vec::new(),
        });
        self.undo.push(Change {
            order: Some(Stacking {
                before: ids(&before),
                after: Vec::new(),
            }),
            before,
            after: Vec::new(),
            background,
        });
        self.redo.clear();
    }
    pub fn delete_selection(&mut self) {
        let ids = std::mem::take(&mut self.selected);
        self.remove_objects(&ids);
    }
    pub fn add_text(&mut self, position: Point, text: String) {
        if text.trim().is_empty() || !position.finite() {
            return;
        }
        let size = 24.;
        let lines = text.lines().count().max(1) as f64;
        let extent = Point {
            x: text.lines().map(|l| l.chars().count()).max().unwrap_or(1) as f64 * size * 0.65,
            y: lines * size * 1.4,
        };
        self.add_text_measured(position, text, extent);
    }
    pub fn add_text_measured(&mut self, position: Point, text: String, extent: Point) {
        self.add_text_measured_colored(position, text, extent, self.style.color);
    }
    pub fn add_text_measured_colored(
        &mut self,
        position: Point,
        text: String,
        extent: Point,
        color: [u8; 3],
    ) -> Option<u64> {
        self.add_text_measured_sized_colored(position, text, extent, color, 24.)
    }
    pub fn add_text_measured_sized_colored(
        &mut self,
        position: Point,
        text: String,
        extent: Point,
        color: [u8; 3],
        size: f64,
    ) -> Option<u64> {
        if text.trim().is_empty()
            || !position.finite()
            || !extent.finite()
            || extent.x < 0.
            || extent.y < 0.
            || !size.is_finite()
            || !(8. ..=300.).contains(&size)
        {
            return None;
        }
        self.cancel();
        let before = self.objects.clone();
        self.next_id += 1;
        self.objects.push(Object {
            rotation: 0.,
            fill: Vec::new(),
            id: self.next_id,
            shape: Shape::Text {
                position,
                text,
                size,
                extent,
            },
            style: Style {
                color,
                ..self.style
            },
        });
        self.commit(before);
        Some(self.next_id)
    }
    pub fn update_text(&mut self, id: u64, text: String) {
        let size = self
            .objects
            .iter()
            .find(|o| o.id == id)
            .and_then(|o| {
                if let Shape::Text { size, .. } = o.shape {
                    Some(size)
                } else {
                    None
                }
            })
            .unwrap_or(24.);
        let extent = Point {
            x: text.lines().map(|l| l.chars().count()).max().unwrap_or(1) as f64 * size * 0.65,
            y: text.lines().count().max(1) as f64 * size * 1.4,
        };
        self.update_text_measured(id, text, extent);
    }
    pub fn update_text_measured(&mut self, id: u64, text: String, measured: Point) {
        let Some(color) = self
            .objects
            .iter()
            .find(|o| o.id == id)
            .map(|o| o.style.color)
        else {
            return;
        };
        self.update_text_measured_colored(id, text, measured, color);
    }
    pub fn update_text_measured_colored(
        &mut self,
        id: u64,
        text: String,
        measured: Point,
        color: [u8; 3],
    ) {
        let Some(size) = self.objects.iter().find(|o| o.id == id).and_then(|o| {
            if let Shape::Text { size, .. } = o.shape {
                Some(size)
            } else {
                None
            }
        }) else {
            return;
        };
        self.update_text_measured_sized_colored(id, text, measured, color, size);
    }
    pub fn update_text_measured_sized_colored(
        &mut self,
        id: u64,
        text: String,
        measured: Point,
        color: [u8; 3],
        new_size: f64,
    ) {
        if !measured.finite()
            || measured.x < 0.
            || measured.y < 0.
            || !new_size.is_finite()
            || !(8. ..=300.).contains(&new_size)
        {
            return;
        }
        self.cancel();
        let before = self.objects.clone();
        if let Some(o) = self.objects.iter_mut().find(|o| o.id == id)
            && let Shape::Text {
                text: value,
                extent,
                size,
                ..
            } = &mut o.shape
        {
            *value = text;
            *size = new_size;
            *extent = measured;
            o.style.color = color;
        }
        self.commit(before);
    }
    /// Apply all measured font sizes as one reversible edit, or reject the entire batch.
    pub fn set_text_sizes(&mut self, sizes: &[(u64, f64, Point)]) -> bool {
        let objects = self.gesture.as_ref().map_or(&self.objects, |g| &g.before);
        if sizes.is_empty()
            || !sizes.iter().all(|&(id, size, extent)| {
                size.is_finite()
                    && (8. ..=300.).contains(&size)
                    && extent.finite()
                    && extent.x >= 0.
                    && extent.y >= 0.
                    && objects
                        .iter()
                        .any(|o| o.id == id && matches!(o.shape, Shape::Text { .. }))
            })
        {
            return false;
        }
        self.cancel();
        let before = self.objects.clone();
        for &(id, new_size, measured) in sizes {
            if let Some(o) = self.objects.iter_mut().find(|o| o.id == id)
                && let Shape::Text { size, extent, .. } = &mut o.shape
            {
                *size = new_size;
                *extent = measured;
            }
        }
        self.commit(before);
        true
    }
    pub fn set_text_size(&mut self, id: u64, size: f64, extent: Point) -> bool {
        self.set_text_sizes(&[(id, size, extent)])
    }
    pub fn copy_selected(&self) -> Vec<Object> {
        self.objects
            .iter()
            .filter(|o| self.selected.contains(&o.id))
            .cloned()
            .map(|mut o| {
                o.prepare_transform(&self.objects, &self.selected);
                o
            })
            .collect()
    }
    pub fn paste_objects(&mut self, clipboard: &[Object], offset: Point) -> bool {
        if clipboard.is_empty() || !offset.finite() {
            return false;
        }
        self.cancel();
        let before = self.objects.clone();
        let remap: Vec<_> = clipboard
            .iter()
            .map(|o| {
                self.next_id += 1;
                (o.id, self.next_id)
            })
            .collect();
        let copied_ids: Vec<_> = remap.iter().map(|(old, _)| *old).collect();
        self.selected.clear();
        for (original, (_, id)) in clipboard.iter().zip(&remap) {
            let mut object = original.clone();
            object.prepare_transform(clipboard, &copied_ids);
            object.id = *id;
            // A wall left behind would spare the inside of a shape the copy does not hold.
            remap_walls(&mut object.fill, |wall| {
                Some(remap.iter().find(|(old, _)| *old == wall)?.1)
            });
            if let Shape::Arrow {
                start_attach,
                end_attach,
                ..
            } = &mut object.shape
            {
                for attachment in [start_attach, end_attach] {
                    *attachment = attachment.and_then(|mut a| {
                        a.target = remap.iter().find(|(old, _)| *old == a.target)?.1;
                        Some(a)
                    });
                }
            }
            object.translate(offset);
            self.selected.push(object.id);
            self.objects.push(object);
        }
        self.commit(before);
        true
    }
    pub fn apply_style(&mut self) {
        if self.drawing() {
            // Keep the drawing transaction open: all live style changes belong to
            // the same undo entry as the stroke, committed only on pointer release.
            if let Some(object) = self.objects.last_mut() {
                object.style = self.style;
            }
            return;
        }
        self.cancel();
        let before = self.objects.clone();
        for o in &mut self.objects {
            if self.selected.contains(&o.id) {
                o.style = self.style;
                if let Shape::Stroke { widths, .. } = &mut o.shape {
                    widths.fill(self.style.width);
                }
            }
        }
        self.commit(before);
    }
    fn drawing(&self) -> bool {
        self.gesture
            .as_ref()
            .is_some_and(|g| matches!(g.kind, GestureKind::Draw))
    }
    /// The current colour onto the shape being drawn or the selection; widths stay.
    pub fn apply_color(&mut self) {
        let color = self.style.color;
        if self.drawing() {
            if let Some(object) = self.objects.last_mut() {
                object.style.color = color;
            }
            return;
        }
        self.cancel();
        let before = self.objects.clone();
        for o in &mut self.objects {
            if self.selected.contains(&o.id) {
                o.style.color = color;
            }
        }
        self.commit(before);
    }
    /// Each selected shape's own width through `step`, colours untouched, as one undo step.
    /// Returns the first one's new width; None while drawing or with nothing selected.
    pub fn step_selected_widths(&mut self, step: impl Fn(f64) -> f64) -> Option<f64> {
        if self.drawing() || self.selected.is_empty() {
            return None;
        }
        self.cancel();
        let before = self.objects.clone();
        let mut first = None;
        for o in &mut self.objects {
            if self.selected.contains(&o.id) {
                let width = step(o.style.width);
                o.style.width = width;
                if let Shape::Stroke { widths, .. } = &mut o.shape {
                    widths.fill(width);
                }
                first.get_or_insert(width);
            }
        }
        self.commit(before);
        first
    }
}
