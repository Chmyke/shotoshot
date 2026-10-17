use crate::{Point, Rect, rotate_point, segment_distance};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Style {
    pub color: [u8; 3],
    pub width: f64,
}
impl Default for Style {
    fn default() -> Self {
        Self {
            color: [237, 79, 92],
            width: 3.,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Attachment {
    pub target: u64,
    pub direction: Point,
}
#[derive(Clone, Debug, PartialEq)]
pub enum Shape {
    Rectangle(Rect),
    Ellipse(Rect),
    Arrow {
        start: Point,
        end: Point,
        start_attach: Option<Attachment>,
        end_attach: Option<Attachment>,
        /// False for the Line tool: the same bindable segment without a head.
        head: bool,
    },
    Stroke {
        points: Vec<Point>,
        // One width per segment; empty for uniform legacy/synthetic strokes.
        widths: Vec<f64>,
        highlight: bool,
    },
    Text {
        position: Point,
        text: String,
        size: f64,
        extent: Point,
    },
}
#[derive(Clone, Debug, PartialEq)]
pub struct Object {
    /// Clockwise radians around local bounds center (closed shapes and text).
    pub rotation: f64,
    pub id: u64,
    pub shape: Shape,
    pub style: Style,
    /// Opaque interior of a closed shape, drawn under its outline; ignored elsewhere.
    /// Not part of `Style`: the brush in hand never fills what it draws.
    pub fill: Option<[u8; 3]>,
}
impl Object {
    /// How far each rendered end reaches past its point: a round cap, none for a filled head.
    pub fn end_caps(&self) -> (f64, f64) {
        match self.shape {
            Shape::Arrow { head, .. } => {
                let cap = self.style.width * 0.5;
                (cap, if head { 0. } else { cap })
            }
            _ => (0., 0.),
        }
    }
    pub fn prepare_transform(&mut self, objects: &[Object], selected: &[u64]) {
        let (start_cap, end_cap) = self.end_caps();
        if let Shape::Arrow {
            start,
            end,
            start_attach,
            end_attach,
            ..
        } = &mut self.shape
        {
            *start = start_attach
                .and_then(|a| resolve(a, objects, start_cap))
                .unwrap_or(*start);
            *end = end_attach
                .and_then(|a| resolve(a, objects, end_cap))
                .unwrap_or(*end);
            if start_attach.is_some_and(|a| !selected.contains(&a.target)) {
                *start_attach = None;
            }
            if end_attach.is_some_and(|a| !selected.contains(&a.target)) {
                *end_attach = None;
            }
        }
    }
    pub fn resolved_bounds(&self, objects: &[Object]) -> Rect {
        arrow_points(self, objects)
            .and_then(|(a, b)| Rect::new(a, b))
            .unwrap_or_else(|| self.bounds())
    }
    pub fn local_to_world(&self, p: Point) -> Point {
        rotate_point(p, self.local_bounds().center(), self.rotation)
    }
    pub fn world_to_local(&self, p: Point) -> Point {
        rotate_point(p, self.local_bounds().center(), -self.rotation)
    }
    pub fn bounds(&self) -> Rect {
        let r = self.local_bounds();
        if self.rotation == 0. {
            return r;
        }
        if matches!(self.shape, Shape::Ellipse(_)) {
            let radius = r.size() * 0.5;
            let (sin, cos) = self.rotation.sin_cos();
            let extent = Point {
                x: (radius.x * cos).hypot(radius.y * sin),
                y: (radius.x * sin).hypot(radius.y * cos),
            };
            return Rect {
                min: r.center() - extent,
                max: r.center() + extent,
            };
        }
        r.corners()
            .into_iter()
            .map(|p| {
                let p = self.local_to_world(p);
                Rect { min: p, max: p }
            })
            .reduce(|a, b| a.union(b))
            .unwrap_or(r)
    }
    pub fn rotate(&mut self, center: Point, angle: f64) {
        let old_center = self.local_bounds().center();
        let new_center = rotate_point(old_center, center, angle);
        match &mut self.shape {
            Shape::Arrow { start, end, .. } => {
                *start = rotate_point(*start, center, angle);
                *end = rotate_point(*end, center, angle);
            }
            Shape::Stroke { points, .. } => {
                for p in points {
                    *p = rotate_point(*p, center, angle);
                }
            }
            _ => {
                self.translate(new_center - old_center);
                self.rotation = (self.rotation + angle).rem_euclid(std::f64::consts::TAU);
            }
        }
    }
    pub fn local_bounds(&self) -> Rect {
        match &self.shape {
            Shape::Rectangle(r) | Shape::Ellipse(r) => *r,
            Shape::Arrow { start, end, .. } => Rect::new(*start, *end).unwrap_or_default(),
            Shape::Stroke { points, .. } => points
                .iter()
                .fold(None, |r: Option<Rect>, p| {
                    Some(r.map_or(Rect { min: *p, max: *p }, |r| {
                        r.union(Rect { min: *p, max: *p })
                    }))
                })
                .unwrap_or_default(),
            Shape::Text {
                position, extent, ..
            } => Rect {
                min: *position,
                max: *position + *extent,
            },
        }
    }
    pub fn translate(&mut self, delta: Point) {
        let r = self.local_bounds();
        self.transform(
            r,
            Rect {
                min: r.min + delta,
                max: r.max + delta,
            },
        );
    }
    pub fn transform(&mut self, from: Rect, to: Rect) {
        let map = |p: Point| Point {
            x: to.min.x
                + (p.x - from.min.x)
                    * if from.size().x.abs() < 0.001 {
                        1.
                    } else {
                        to.size().x / from.size().x
                    },
            y: to.min.y
                + (p.y - from.min.y)
                    * if from.size().y.abs() < 0.001 {
                        1.
                    } else {
                        to.size().y / from.size().y
                    },
        };
        match &mut self.shape {
            Shape::Rectangle(r) | Shape::Ellipse(r) => {
                *r = Rect::new(map(r.min), map(r.max)).unwrap_or(*r);
            }
            Shape::Arrow { start, end, .. } => {
                *start = map(*start);
                *end = map(*end);
            }
            Shape::Stroke { points, .. } => {
                for p in points {
                    *p = map(*p);
                }
            }
            Shape::Text {
                position,
                size,
                extent,
                ..
            } => {
                *position = map(*position);
                let scale = if from.size().y > 0.001 {
                    to.size().y / from.size().y
                } else {
                    1.
                };
                let new_size = (*size * scale).clamp(8., 300.);
                *extent = *extent * (new_size / *size);
                *size = new_size;
            }
        }
    }
    /// Inside a closed shape, outline included; open shapes and text contain nothing.
    pub fn contains(&self, p: Point) -> bool {
        let r = self.local_bounds();
        let half = self.style.width * 0.5;
        let radius = r.size() * 0.5 + Point { x: half, y: half };
        let d = self.world_to_local(p) - r.center();
        if r.size().x < 0.001 || r.size().y < 0.001 || !d.finite() {
            return false;
        }
        match self.shape {
            Shape::Rectangle(_) => d.x.abs() <= radius.x && d.y.abs() <= radius.y,
            Shape::Ellipse(_) => (d.x / radius.x).hypot(d.y / radius.y) <= 1.,
            _ => false,
        }
    }
    pub fn hit(&self, p: Point, tolerance: f64, objects: &[Object]) -> bool {
        // A filled interior is solid to the eye, so it is grabbed like the outline.
        (self.fill.is_some() && self.contains(p)) || self.on_ink(p, tolerance, objects)
    }
    /// On what the object draws: its outline, stroke, arrow head or text box. `tolerance`
    /// widens a rectangle from its centre line and every other shape from its drawn edge.
    pub fn on_ink(&self, p: Point, tolerance: f64, objects: &[Object]) -> bool {
        match &self.shape {
            Shape::Rectangle(r) => {
                let c = r.corners().map(|p| self.local_to_world(p));
                (0..4).any(|i| segment_distance(p, c[i], c[(i + 1) % 4]) <= tolerance)
            }
            // Use the rendered geometry: radial ellipse estimates severely understate
            // distances near flat tips. Arrow heads also have a selectable filled interior.
            Shape::Ellipse(_) | Shape::Arrow { .. } => {
                crate::paths(self, objects).iter().any(|path| {
                    if matches!(self.shape, Shape::Arrow { .. }) && path.len() == 4 {
                        let cross = |a: Point, b: Point| {
                            (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x)
                        };
                        let sides = [
                            cross(path[0], path[1]),
                            cross(path[1], path[2]),
                            cross(path[2], path[0]),
                        ];
                        if sides.iter().all(|v| *v >= 0.) || sides.iter().all(|v| *v <= 0.) {
                            return true;
                        }
                    }
                    path.windows(2).any(|w| {
                        segment_distance(p, w[0], w[1]) <= tolerance + self.style.width * 0.5
                    })
                })
            }
            Shape::Stroke { .. } => crate::stroke_runs(self).iter().any(|(points, width)| {
                points
                    .windows(2)
                    .any(|w| segment_distance(p, w[0], w[1]) <= tolerance + width * 0.5)
            }),
            Shape::Text { .. } => self.local_bounds().contains(self.world_to_local(p)),
        }
    }
}
/// Numbers `objects` `first`, `first + 1`… in pile order, the order a saved scene stores
/// them in. An attachment follows its target's new number; one whose target is not among
/// `objects` is dropped, so callers resolve such ends first (`prepare_transform`).
pub fn renumber(objects: &mut [Object], first: u64) {
    let old: Vec<u64> = objects.iter().map(|o| o.id).collect();
    let new = |id: u64| Some(first + old.iter().position(|o| *o == id)? as u64);
    for (index, object) in objects.iter_mut().enumerate() {
        object.id = first + index as u64;
        if let Shape::Arrow {
            start_attach,
            end_attach,
            ..
        } = &mut object.shape
        {
            for attach in [start_attach, end_attach] {
                *attach = attach.and_then(|a| {
                    Some(Attachment {
                        target: new(a.target)?,
                        ..a
                    })
                });
            }
        }
    }
}
pub fn attach_at(p: Point, objects: &[Object]) -> Option<Attachment> {
    objects
        .iter()
        .rev()
        .find(|o| {
            matches!(o.shape, Shape::Rectangle(_) | Shape::Ellipse(_))
                && o.local_bounds()
                    .expanded(o.style.width * 0.5)
                    .contains(o.world_to_local(p))
        })
        .map(|o| Attachment {
            target: o.id,
            direction: if (o.world_to_local(p) - o.local_bounds().center()).length() < 0.001 {
                Point { x: 1., y: 0. }
            } else {
                o.world_to_local(p) - o.local_bounds().center()
            },
        })
}
/// `cap` keeps a round line end from overlapping the target's stroke.
pub fn resolve(a: Attachment, objects: &[Object], cap: f64) -> Option<Point> {
    let o = objects.iter().find(|o| o.id == a.target)?;
    let r = o.local_bounds();
    let radius = r.size() * 0.5;
    let d = a.direction;
    if radius.x < 0.001 || radius.y < 0.001 {
        return Some(r.center());
    }
    // Endpoints rest on the outer edge of the target's stroke, not on its centerline.
    let half = o.style.width * 0.5 + cap;
    let offset = match o.shape {
        Shape::Ellipse(_) => {
            let divisor = (d.x / radius.x).hypot(d.y / radius.y);
            if divisor < 0.001 {
                return Some(o.local_to_world(r.center()));
            }
            let q = d * (1. / divisor);
            let normal = Point {
                x: q.x / (radius.x * radius.x),
                y: q.y / (radius.y * radius.y),
            };
            q + normal * (half / normal.length())
        }
        _ => {
            // Mitred rectangle strokes stay rectangular, half a width further out.
            let outer = Point {
                x: radius.x + half,
                y: radius.y + half,
            };
            let divisor = (d.x / outer.x).abs().max((d.y / outer.y).abs());
            if divisor < 0.001 {
                return Some(o.local_to_world(r.center()));
            }
            d * (1. / divisor)
        }
    };
    Some(o.local_to_world(r.center() + offset))
}
pub fn arrow_points(o: &Object, objects: &[Object]) -> Option<(Point, Point)> {
    if let Shape::Arrow {
        start,
        end,
        start_attach,
        end_attach,
        ..
    } = &o.shape
    {
        let (start_cap, end_cap) = o.end_caps();
        let a = start_attach
            .and_then(|a| resolve(a, objects, start_cap))
            .unwrap_or(*start);
        let b = end_attach
            .and_then(|a| resolve(a, objects, end_cap))
            .unwrap_or(*end);
        // Coincident automatic attachments, apart from cap clearance, must not erase a
        // nonzero gesture.
        Some(
            if (b - a).length() < start_cap + end_cap + 0.001 && (*end - *start).length() >= 0.001 {
                (*start, *end)
            } else {
                (a, b)
            },
        )
    } else {
        None
    }
}
