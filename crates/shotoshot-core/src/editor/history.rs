use super::*;
use std::collections::{HashMap, HashSet};

/// The pile's objects by id; ids are unique within a pile.
fn by_id(objects: &[Object]) -> HashMap<u64, &Object> {
    objects.iter().map(|o| (o.id, o)).collect()
}

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
        // Ids are looked up through a map so a step costs O(N), not O(N²), on large piles.
        let now = by_id(&self.objects);
        let then = by_id(&before);
        let removed = before
            .iter()
            .filter(|o| now.get(&o.id).copied() != Some(o))
            .cloned()
            .collect();
        let added = self
            .objects
            .iter()
            .filter(|o| then.get(&o.id).copied() != Some(o))
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
        let remove: HashSet<u64> = remove.iter().map(|r| r.id).collect();
        self.objects.retain(|o| !remove.contains(&o.id));
        self.objects.extend_from_slice(add);
        if let Some(order) = order {
            let place: HashMap<u64, usize> =
                order.iter().enumerate().map(|(i, id)| (*id, i)).collect();
            self.objects.sort_by_key(|o| place.get(&o.id));
        }
        let present: HashSet<u64> = self.objects.iter().map(|o| o.id).collect();
        self.selected.retain(|id| present.contains(id));
    }
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
    pub fn undo(&mut self) {
        self.cancel();
        self.fresh = None;
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
        self.fresh = None;
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
        self.add_label(position, text, extent, color, size, None, None)
    }
    /// A text on a panel of `color` when `label` is given, or a plain text otherwise. A
    /// panel's text takes `ink`, or with `None` the black or white that contrasts with it.
    #[allow(clippy::too_many_arguments)]
    pub fn add_label(
        &mut self,
        position: Point,
        text: String,
        extent: Point,
        color: [u8; 3],
        size: f64,
        label: Option<Label>,
        ink: Option<[u8; 3]>,
    ) -> Option<u64> {
        if text.trim().is_empty()
            || !position.finite()
            || !extent.finite()
            || extent.x < 0.
            || extent.y < 0.
            || !size.is_finite()
            || !(8. ..=300.).contains(&size)
            || label
                .and_then(Label::wrap)
                .is_some_and(|w| !w.is_finite() || w <= 0.)
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
                label,
                ink: label.and(ink),
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
    /// The edited text, its extent, colour and size as one step; a panel keeps its ink.
    pub fn update_text_measured_sized_colored(
        &mut self,
        id: u64,
        text: String,
        measured: Point,
        color: [u8; 3],
        new_size: f64,
    ) {
        let Some(ink) = self
            .objects
            .iter()
            .find(|o| o.id == id)
            .map(|o| match o.shape {
                Shape::Text { ink, .. } => ink,
                _ => None,
            })
        else {
            return;
        };
        self.update_text_inked(id, text, measured, color, new_size, ink);
    }
    /// `update_text_measured_sized_colored` that also gives a panel's text `ink` (`None`:
    /// the automatic black or white), as one step.
    pub fn update_text_inked(
        &mut self,
        id: u64,
        text: String,
        measured: Point,
        color: [u8; 3],
        new_size: f64,
        ink: Option<[u8; 3]>,
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
                label,
                ink: own,
                ..
            } = &mut o.shape
        {
            *value = text;
            *size = new_size;
            *extent = measured;
            *own = label.and(ink);
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
    /// The lines of text `id` laid out again at `extent` during a gesture that resized its
    /// label, so its panel follows before the gesture commits; nothing outside a gesture.
    pub fn reflow(&mut self, id: u64, measured: Point) {
        if self.gesture.is_none() || !measured.finite() || measured.x < 0. || measured.y < 0. {
            return;
        }
        if let Some(o) = self.objects.iter_mut().find(|o| o.id == id)
            && let Shape::Text { extent, .. } = &mut o.shape
        {
            *extent = measured;
        }
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
    /// The current style onto the shape being drawn or the selection; a blur takes the
    /// blur strength instead (`blur_radius`) while drawn, and keeps its own once set down.
    pub fn apply_style(&mut self) {
        if self.drawing() {
            // Keep the drawing transaction open: all live style changes belong to
            // the same undo entry as the stroke, committed only on pointer release.
            if let Some(object) = self.objects.last_mut() {
                object.style.width = if object.is_blur() {
                    self.blur_radius
                } else {
                    object.style.color = self.style.color;
                    self.style.width
                };
            }
            return;
        }
        self.cancel();
        let before = self.objects.clone();
        for o in &mut self.objects {
            if self.selected.contains(&o.id) && !o.is_blur() {
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
    /// The current colour onto the shape being drawn or the selection; widths stay. A blur
    /// has no colour: it is left alone.
    pub fn apply_color(&mut self) {
        let color = self.style.color;
        if self.drawing() {
            if let Some(object) = self.objects.last_mut().filter(|o| !o.is_blur()) {
                object.style.color = color;
            }
            return;
        }
        self.cancel();
        let before = self.objects.clone();
        for o in &mut self.objects {
            if self.selected.contains(&o.id) && !o.is_blur() {
                o.style.color = color;
            }
        }
        self.commit(before);
    }
    /// `ink` onto the text of every selected panel (`None`: the automatic black or white),
    /// as one undo step; plain texts and shapes are left alone. True when one changed.
    pub fn apply_ink(&mut self, ink: Option<[u8; 3]>) -> bool {
        if self.drawing() {
            return false;
        }
        self.cancel();
        let before = self.objects.clone();
        for o in &mut self.objects {
            if self.selected.contains(&o.id)
                && let Shape::Text {
                    label: Some(_),
                    ink: own,
                    ..
                } = &mut o.shape
            {
                *own = ink;
            }
        }
        let changed = before != self.objects;
        self.commit(before);
        changed
    }
    /// Each selected shape's own width through `step`, colours untouched, as one undo step;
    /// `step` is told whether the shape is a blur, whose width is its strength. Returns the
    /// first one's new width and whether it is a blur; None while drawing or with nothing
    /// selected.
    pub fn step_selected_widths(&mut self, step: impl Fn(f64, bool) -> f64) -> Option<(f64, bool)> {
        if self.drawing() || self.selected.is_empty() {
            return None;
        }
        self.cancel();
        let before = self.objects.clone();
        let mut first = None;
        for o in &mut self.objects {
            if self.selected.contains(&o.id) {
                let blur = o.is_blur();
                let width = step(o.style.width, blur);
                o.style.width = width;
                if let Shape::Stroke { widths, .. } = &mut o.shape {
                    widths.fill(width);
                }
                first.get_or_insert((width, blur));
            }
        }
        self.commit(before);
        first
    }
}
