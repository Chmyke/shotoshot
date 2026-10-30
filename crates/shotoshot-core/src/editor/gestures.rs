use super::*;

/// Puts the selected objects back as they were when the gesture began. Moving, turning and
/// resizing change nothing else, so the rest of the pile (strokes of thousands of points
/// included) is not copied again on every pointer move.
fn restore_selected(objects: &mut Vec<Object>, before: &[Object], selected: &[u64]) {
    if objects.len() != before.len() {
        objects.clear();
        objects.extend_from_slice(before);
        return;
    }
    for (o, b) in objects.iter_mut().zip(before) {
        if selected.contains(&b.id) {
            o.clone_from(b);
        }
    }
}

impl Editor {
    pub fn begin(&mut self, p: Point, modifiers: Modifiers) {
        if !p.finite() {
            return;
        }
        self.cancel();
        let before = self.objects.clone();
        let fresh = self.fresh_bend_handle(p);
        let anchor = self.anchor_at(p);
        self.fresh = None;
        let kind = if let Some(index) = fresh {
            self.fresh = Some(self.objects[index].id);
            GestureKind::Bend { index }
        } else if let Some(source) = anchor {
            self.begin_elbow(source, p);
            GestureKind::Draw
        } else if self.tool == Tool::Select {
            self.selection_gesture(p, modifiers)
        } else if self.tool == Tool::Eraser {
            self.selected.clear();
            GestureKind::Erase {
                touched: Vec::new(),
            }
        } else {
            self.next_id += 1;
            let shape = match self.tool {
                Tool::Ellipse => Shape::Ellipse(Rect { min: p, max: p }),
                Tool::Arrow | Tool::Line => Shape::Arrow {
                    start: p,
                    end: p,
                    start_attach: None,
                    end_attach: None,
                    head: self.tool == Tool::Arrow,
                    bend: Point::default(),
                    elbow: false,
                },
                Tool::Pen | Tool::Highlight => Shape::Stroke {
                    points: vec![p],
                    widths: Vec::new(),
                    highlight: self.tool == Tool::Highlight,
                },
                _ => Shape::Rectangle(Rect { min: p, max: p }),
            };
            if matches!(self.tool, Tool::Text | Tool::Label) {
                return;
            }
            self.objects.push(Object {
                rotation: 0.,
                fill: Vec::new(),
                id: self.next_id,
                shape,
                style: self.style,
            });
            self.selected.clear();
            GestureKind::Draw
        };
        self.gesture = Some(Gesture {
            origin: p,
            current: p,
            before,
            modifiers,
            kind,
        });
        if self.tool == Tool::Eraser {
            self.erase_along(p, p);
        }
    }
    pub(super) fn selection_gesture(&mut self, p: Point, m: Modifiers) -> GestureKind {
        if let Some(kind) = self.handle_at(p) {
            return kind;
        }
        // Shift, or Ctrl as in Windows lists, adds or removes one object.
        let toggle = m.shift || m.ctrl;
        if let Some(id) = self.object_at(p) {
            if toggle && self.selected.contains(&id) {
                self.selected.retain(|i| *i != id);
            } else if !self.selected.contains(&id) {
                if !toggle {
                    self.selected.clear();
                }
                self.selected.push(id);
            }
            GestureKind::Move
        } else {
            if !toggle {
                self.selected.clear();
            }
            GestureKind::Marquee
        }
    }
    /// The object a Select press at `p` would add to the selection: what hovering
    /// outlines. None over a handle of the selection, which the press would grab instead.
    pub fn hover_at(&self, p: Point) -> Option<u64> {
        if self.tool != Tool::Select || self.gesture.is_some() || !p.finite() {
            return None;
        }
        if self.handle_at(p).is_some() {
            return None;
        }
        self.object_at(p).filter(|id| !self.selected.contains(id))
    }
    /// The text box whose four anchors show for a pointer at `p`: the lone selected one
    /// whatever the tool, else the one under or just around the pointer with Select or Arrow
    /// in hand. None during a gesture.
    pub fn anchor_owner(&self, p: Point) -> Option<u64> {
        if self.gesture.is_some() || !p.finite() {
            return None;
        }
        let boxed = |o: &&Object| matches!(o.shape, Shape::Text { label: Some(_), .. });
        if let [id] = self.selected[..]
            && self.objects.iter().filter(boxed).any(|o| o.id == id)
        {
            return Some(id);
        }
        if !matches!(self.tool, Tool::Select | Tool::Arrow) {
            return None;
        }
        // A corridor around the box keeps its anchors in reach on the way out to them.
        let corridor = (ANCHOR_GAP + 10.) * self.ui_scale;
        self.objects
            .iter()
            .rev()
            .filter(boxed)
            .find(|o| {
                o.local_bounds()
                    .expanded(corridor)
                    .contains(o.world_to_local(p))
            })
            .map(|o| o.id)
    }
    /// The four anchors of a text box, clockwise from the top: the middle of each side,
    /// `ANCHOR_GAP` interface pixels out, turned with the box.
    pub fn anchors(&self, id: u64) -> Option<[Point; 4]> {
        let o = self.objects.iter().find(|o| o.id == id)?;
        let r = o.local_bounds();
        let (c, half) = (r.center(), r.size() * 0.5);
        let gap = ANCHOR_GAP * self.ui_scale;
        let (x, y) = (half.x + gap, half.y + gap);
        Some(
            [(0., -y), (x, 0.), (0., y), (-x, 0.)]
                .map(|(dx, dy)| o.local_to_world(c + Point { x: dx, y: dy })),
        )
    }
    /// The text box whose anchor a press at `p` would pull an arrow out of; never over a
    /// handle of the selection, which the press grabs instead.
    pub fn anchor_at(&self, p: Point) -> Option<u64> {
        let id = self.anchor_owner(p)?;
        if self.tool == Tool::Select && self.handle_at(p).is_some() {
            return None;
        }
        let grip = 9. * self.ui_scale;
        self.anchors(id)?
            .iter()
            .any(|a| (*a - p).length() < grip)
            .then_some(id)
    }
    /// What the elbow arrow being pulled out would attach its free end to on release.
    pub fn connect_target(&self) -> Option<u64> {
        let g = self.gesture.as_ref()?;
        if !matches!(g.kind, GestureKind::Draw) {
            return None;
        }
        match self.objects.last()?.shape {
            Shape::Arrow {
                elbow: true,
                end_attach,
                ..
            } => end_attach.map(|a| a.target),
            _ => None,
        }
    }
    /// A new elbow arrow out of text box `source`, in the box's colour, its free end at `p`.
    fn begin_elbow(&mut self, source: u64, p: Point) {
        let Some(color) = self
            .objects
            .iter()
            .find(|o| o.id == source)
            .map(|o| o.style.color)
        else {
            return;
        };
        self.next_id += 1;
        self.objects.push(Object {
            rotation: 0.,
            fill: Vec::new(),
            id: self.next_id,
            shape: Shape::Arrow {
                start: p,
                end: p,
                start_attach: Some(Attachment {
                    target: source,
                    direction: Point { x: 1., y: 0. },
                }),
                end_attach: None,
                head: true,
                bend: Point::default(),
                elbow: true,
            },
            style: Style {
                color,
                width: self.style.width,
            },
        });
        self.selected.clear();
    }
    /// Objects the marquee being drawn would add on release.
    pub fn marquee_catch(&self) -> Vec<u64> {
        self.marquee().map_or_else(Vec::new, |r| self.caught_by(r))
    }
    /// Unselected objects lying wholly inside `r`.
    fn caught_by(&self, r: Rect) -> Vec<u64> {
        self.objects
            .iter()
            .filter(|o| {
                r.contains_rect(o.resolved_bounds(&self.objects)) && !self.selected.contains(&o.id)
            })
            .map(|o| o.id)
            .collect()
    }
    /// Topmost object within the selection reach of `p`.
    fn object_at(&self, p: Point) -> Option<u64> {
        let reach = 7. * self.ui_scale;
        self.objects
            .iter()
            .rev()
            .find(|o| o.hit(p, reach, &self.objects))
            .map(|o| o.id)
    }
    /// The selection handle under `p` and the gesture a press there starts, checked before
    /// any object: rotation, then a lone arrow's ends, then the frame corners.
    fn handle_at(&self, p: Point) -> Option<GestureKind> {
        // Grab zones follow the handles, which are drawn larger on denser screens.
        let grip = 9. * self.ui_scale;
        if self
            .rotation_handle()
            .is_some_and(|h| (p - h).length() < grip)
            && let Some(frame) = self.selection_frame()
        {
            return Some(GestureKind::Rotate {
                center: frame.bounds.center(),
            });
        }
        if self.selected.len() == 1
            && let Some(index) = self.objects.iter().position(|o| o.id == self.selected[0])
            && let Some((a, b)) = arrow_points(&self.objects[index], &self.objects)
        {
            if (p - a).length() < grip {
                return Some(GestureKind::Endpoint { index, start: true });
            }
            if (p - b).length() < grip {
                return Some(GestureKind::Endpoint {
                    index,
                    start: false,
                });
            }
            if self
                .bend_handle()
                .is_some_and(|(_, h)| (p - h).length() < grip)
            {
                return Some(GestureKind::Bend { index });
            }
        }
        if let (Some(frame), Some(bounds)) = (self.selection_frame(), self.selection_bounds()) {
            let corners = frame.corners();
            for (i, c) in corners.iter().enumerate() {
                if (p - *c).length() < grip {
                    return Some(GestureKind::Resize {
                        opposite: corners[(i + 2) % 4],
                        corner: *c,
                        bounds,
                    });
                }
            }
        }
        None
    }
    /// The middle handle of the lone selected arrow or line, or of the one just drawn while
    /// its tool is in hand: the object's index and where the curve passes halfway.
    pub fn bend_handle(&self) -> Option<(usize, Point)> {
        let id = match (self.tool, &self.selected[..]) {
            (Tool::Select, [id]) => *id,
            (Tool::Arrow | Tool::Line, _) if self.gesture.is_none() => self.fresh?,
            _ => return None,
        };
        let index = self.objects.iter().position(|o| o.id == id)?;
        let o = &self.objects[index];
        if matches!(o.shape, Shape::Arrow { elbow: true, .. }) {
            return None;
        }
        let (a, b) = arrow_points(o, &self.objects)?;
        Some((index, bend_point(a, b, o.bend())))
    }
    fn fresh_bend_handle(&self, p: Point) -> Option<usize> {
        if self.tool == Tool::Select {
            return None;
        }
        let (index, h) = self.bend_handle()?;
        ((p - h).length() < 9. * self.ui_scale).then_some(index)
    }
    pub fn update_with_modifiers(&mut self, p: Point, modifiers: Modifiers) {
        self.held = modifiers;
        if let Some(g) = &mut self.gesture {
            g.modifiers = modifiers;
        }
        self.update(p);
    }
    pub fn update_modifiers(&mut self, modifiers: Modifiers) {
        self.held = modifiers;
        if let Some(g) = &mut self.gesture {
            g.modifiers = modifiers;
            let p = g.current;
            self.update(p);
        }
    }
    pub fn update(&mut self, p: Point) {
        if !p.finite() {
            return;
        }
        let Some(g) = &mut self.gesture else {
            return;
        };
        let last = g.current;
        g.current = p;
        if matches!(g.kind, GestureKind::Erase { .. }) {
            self.erase_along(last, p);
            return;
        }
        match g.kind {
            GestureKind::Erase { .. } => {}
            GestureKind::Draw => {
                let elbow = self.objects.last().and_then(|o| match o.shape {
                    Shape::Arrow {
                        elbow: true,
                        start_attach,
                        ..
                    } => Some(start_attach.map(|a| a.target)),
                    _ => None,
                });
                if let Some(source) = elbow {
                    // Pulled out of a box: the end binds on contact, Alt keeps it free.
                    let reach = 10. * self.ui_scale;
                    let prior = &self.objects[..self.objects.len() - 1];
                    let bound = (!g.modifiers.alt)
                        .then(|| connect_at(p, prior, source, reach))
                        .flatten();
                    if let Some(Shape::Arrow {
                        end, end_attach, ..
                    }) = self.objects.last_mut().map(|o| &mut o.shape)
                    {
                        *end = p;
                        *end_attach = bound;
                    }
                    return;
                }
                let p = if g.modifiers.shift
                    && matches!(
                        self.tool,
                        Tool::Line | Tool::Arrow | Tool::Pen | Tool::Highlight
                    ) {
                    snap_angle(g.origin, p)
                } else {
                    p
                };
                let prior = &self.objects[..self.objects.len().saturating_sub(1)];
                let attaches = g.modifiers.attaches();
                let attachment = attaches.then(|| attach_at(p, prior)).flatten();
                let origin_attachment = attaches.then(|| attach_at(g.origin, prior)).flatten();
                if let Some(o) = self.objects.last_mut() {
                    match &mut o.shape {
                        Shape::Rectangle(r) => {
                            let mut d = p - g.origin;
                            if g.modifiers.shift {
                                let size = d.x.abs().max(d.y.abs());
                                d = Point {
                                    x: d.x.signum() * size,
                                    y: d.y.signum() * size,
                                };
                            }
                            *r = Rect::new(
                                if g.modifiers.alt {
                                    g.origin - d
                                } else {
                                    g.origin
                                },
                                g.origin + d,
                            )
                            .unwrap_or(*r);
                        }
                        Shape::Ellipse(r) => {
                            let d = p - g.origin;
                            let radius = if g.modifiers.alt {
                                Point {
                                    x: d.x.abs(),
                                    y: d.y.abs(),
                                }
                            } else {
                                Point {
                                    x: d.length(),
                                    y: d.length(),
                                }
                            };
                            *r = Rect {
                                min: g.origin - radius,
                                max: g.origin + radius,
                            };
                        }
                        Shape::Arrow {
                            end,
                            end_attach,
                            start_attach,
                            ..
                        } => {
                            *start_attach = origin_attachment;
                            *end = p;
                            *end_attach = attachment;
                        }
                        Shape::Stroke { points, widths, .. } if g.modifiers.shift => {
                            points.clear();
                            points.extend([g.origin, p]);
                            widths.clear();
                            o.style.width = self.style.width;
                        }
                        Shape::Stroke { points, widths, .. }
                            if points.last().is_none_or(|last| (*last - p).length() >= 1.) =>
                        {
                            widths.push(self.style.width);
                            points.push(p);
                        }
                        _ => {}
                    }
                }
            }
            GestureKind::Rotate { center } => {
                let a = g.origin - center;
                let b = p - center;
                let angle = b.y.atan2(b.x) - a.y.atan2(a.x);
                restore_selected(&mut self.objects, &g.before, &self.selected);
                for o in &mut self.objects {
                    if self.selected.contains(&o.id) {
                        o.prepare_transform(&g.before, &self.selected);
                        o.rotate(center, angle);
                    }
                }
            }
            GestureKind::Move | GestureKind::Resize { .. } => {
                // A rotated rectangle/ellipse cannot represent affine shear. Resize
                // proportionally along the frame's diagonal, anchored at the opposite corner,
                // which may itself be turned: the axis-aligned bounds scale around it.
                let to = if let GestureKind::Resize {
                    opposite,
                    corner,
                    bounds,
                } = g.kind
                {
                    if g.before
                        .iter()
                        .any(|o| self.selected.contains(&o.id) && o.rotation.abs() > 1e-9)
                    {
                        let diagonal = corner - opposite;
                        let d = p - opposite;
                        let squared = diagonal.x * diagonal.x + diagonal.y * diagonal.y;
                        let scale = if squared > 1e-9 {
                            ((d.x * diagonal.x + d.y * diagonal.y) / squared).max(0.01)
                        } else {
                            1.
                        };
                        Rect::new(
                            opposite + (bounds.min - opposite) * scale,
                            opposite + (bounds.max - opposite) * scale,
                        )
                    } else {
                        Rect::new(opposite, p)
                    }
                    .map(|to| (bounds, to))
                } else {
                    None
                };
                let resizing = matches!(g.kind, GestureKind::Resize { .. });
                restore_selected(&mut self.objects, &g.before, &self.selected);
                for o in &mut self.objects {
                    if self.selected.contains(&o.id) {
                        o.prepare_transform(&g.before, &self.selected);
                        match to {
                            Some((from, to)) => o.transform(from, to),
                            None if resizing => {}
                            None => o.translate(p - g.origin),
                        }
                    }
                }
            }
            GestureKind::Endpoint { index, start } => {
                let p = if g.modifiers.shift {
                    arrow_points(&self.objects[index], &self.objects)
                        .map_or(p, |(a, b)| snap_angle(if start { b } else { a }, p))
                } else {
                    p
                };
                let attachment = match self.objects[index].shape {
                    // An elbow's ends bind on contact as when pulled out, never both to one.
                    Shape::Arrow {
                        elbow: true,
                        start_attach,
                        end_attach,
                        ..
                    } => {
                        let other = if start { end_attach } else { start_attach };
                        let reach = 10. * self.ui_scale;
                        (!g.modifiers.alt)
                            .then(|| connect_at(p, &self.objects, other.map(|a| a.target), reach))
                            .flatten()
                    }
                    _ => g
                        .modifiers
                        .attaches()
                        .then(|| attach_at(p, &self.objects))
                        .flatten(),
                };
                if let Shape::Arrow {
                    start: a,
                    end: b,
                    start_attach,
                    end_attach,
                    ..
                } = &mut self.objects[index].shape
                {
                    if start {
                        *a = p;
                        *start_attach = attachment;
                    } else {
                        *b = p;
                        *end_attach = attachment;
                    }
                }
            }
            GestureKind::Bend { index } => {
                let o = &self.objects[index];
                if let Some((a, b)) = arrow_points(o, &self.objects) {
                    // Near the chord it springs straight again, as nothing else does.
                    let straight = (b - a).length() < 0.001
                        || ((p - a).x * (b - a).y - (p - a).y * (b - a).x).abs()
                            < BEND_SNAP * self.ui_scale * (b - a).length();
                    let bent = if straight {
                        Point::default()
                    } else {
                        bend_through(a, b, p)
                    };
                    if let Shape::Arrow { bend, .. } = &mut self.objects[index].shape {
                        *bend = bent;
                    }
                }
            }
            GestureKind::Marquee => {}
        }
    }
    /// Whether a corner of the selection is being dragged.
    pub fn resizing(&self) -> bool {
        self.gesture
            .as_ref()
            .is_some_and(|g| matches!(g.kind, GestureKind::Resize { .. }))
    }
    pub fn finish(&mut self) {
        let Some(g) = self.gesture.take() else {
            return;
        };
        if matches!(g.kind, GestureKind::Marquee) {
            if let Some(r) = Rect::new(g.origin, g.current) {
                let caught = self.caught_by(r);
                self.selected.extend(caught);
            }
        } else if let GestureKind::Erase { touched } = &g.kind {
            // Ink cut along the way and whole objects marked by it go as one step.
            self.drop_objects(touched);
            self.commit(g.before);
        } else if matches!(g.kind, GestureKind::Draw)
            && self
                .objects
                .last()
                .is_none_or(|o| o.bounds().size().length() < 1.)
        {
            self.objects = g.before;
        } else {
            if matches!(g.kind, GestureKind::Draw) {
                self.fresh = (self.objects.last())
                    .filter(|o| matches!(o.shape, Shape::Arrow { .. }))
                    .map(|o| o.id);
            }
            self.commit(g.before);
        }
    }
    pub fn cancel(&mut self) -> bool {
        if let Some(g) = self.gesture.take() {
            self.objects = g.before;
            true
        } else {
            false
        }
    }
}
