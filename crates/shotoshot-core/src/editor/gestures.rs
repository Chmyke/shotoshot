use super::*;

impl Editor {
    pub fn begin(&mut self, p: Point, modifiers: Modifiers) {
        if !p.finite() {
            return;
        }
        self.cancel();
        let before = self.objects.clone();
        let kind = if self.tool == Tool::Select {
            self.selection_gesture(p, modifiers)
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
                },
                Tool::Pen | Tool::Highlight => Shape::Stroke {
                    points: vec![p],
                    widths: Vec::new(),
                    highlight: self.tool == Tool::Highlight,
                },
                _ => Shape::Rectangle(Rect { min: p, max: p }),
            };
            if self.tool == Tool::Text {
                return;
            }
            self.objects.push(Object {
                rotation: 0.,
                fill: None,
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
    }
    pub(super) fn selection_gesture(&mut self, p: Point, m: Modifiers) -> GestureKind {
        // Grab zones follow the handles, which are drawn larger on denser screens.
        let (grip, reach) = (9. * self.ui_scale, 7. * self.ui_scale);
        if self
            .rotation_handle()
            .is_some_and(|h| (p - h).length() < grip)
            && let Some(frame) = self.selection_frame()
        {
            return GestureKind::Rotate {
                center: frame.bounds.center(),
            };
        }
        if self.selected.len() == 1
            && let Some(index) = self.objects.iter().position(|o| o.id == self.selected[0])
            && let Some((a, b)) = arrow_points(&self.objects[index], &self.objects)
        {
            if (p - a).length() < grip {
                return GestureKind::Endpoint { index, start: true };
            }
            if (p - b).length() < grip {
                return GestureKind::Endpoint {
                    index,
                    start: false,
                };
            }
        }
        if let (Some(frame), Some(bounds)) = (self.selection_frame(), self.selection_bounds()) {
            let corners = frame.corners();
            for (i, c) in corners.iter().enumerate() {
                if (p - *c).length() < grip {
                    return GestureKind::Resize {
                        opposite: corners[(i + 2) % 4],
                        corner: *c,
                        bounds,
                    };
                }
            }
        }
        if let Some(id) = self
            .objects
            .iter()
            .rev()
            .find(|o| o.hit(p, reach, &self.objects))
            .map(|o| o.id)
        {
            if m.shift && self.selected.contains(&id) {
                self.selected.retain(|i| *i != id);
            } else if !self.selected.contains(&id) {
                if !m.shift {
                    self.selected.clear();
                }
                self.selected.push(id);
            }
            GestureKind::Move
        } else {
            if !m.shift {
                self.selected.clear();
            }
            GestureKind::Marquee
        }
    }
    pub fn update_with_modifiers(&mut self, p: Point, modifiers: Modifiers) {
        if let Some(g) = &mut self.gesture {
            g.modifiers = modifiers;
        }
        self.update(p);
    }
    pub fn update_modifiers(&mut self, modifiers: Modifiers) {
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
        g.current = p;
        match g.kind {
            GestureKind::Draw => {
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
                self.objects.clone_from(&g.before);
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
                self.objects.clone_from(&g.before);
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
                let attachment = g
                    .modifiers
                    .attaches()
                    .then(|| attach_at(p, &self.objects))
                    .flatten();
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
            GestureKind::Marquee => {}
        }
    }
    pub fn finish(&mut self) {
        let Some(g) = self.gesture.take() else {
            return;
        };
        if matches!(g.kind, GestureKind::Marquee) {
            if let Some(r) = Rect::new(g.origin, g.current) {
                for o in &self.objects {
                    if r.contains_rect(o.resolved_bounds(&self.objects))
                        && !self.selected.contains(&o.id)
                    {
                        self.selected.push(o.id);
                    }
                }
            }
        } else if matches!(g.kind, GestureKind::Draw)
            && self
                .objects
                .last()
                .is_none_or(|o| o.bounds().size().length() < 1.)
        {
            self.objects = g.before;
        } else {
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
