//! ROLE: Owns the editable document and reversible gesture transactions.
//! OWNER: One capture session; no operating system resources.
//! INVARIANTS: A gesture commits once; image pixels never enter undo history.
mod fill;
mod gestures;
mod history;
mod stacking;
use crate::*;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tool {
    Select,
    #[default]
    Rectangle,
    Ellipse,
    Line,
    Arrow,
    Pen,
    Highlight,
    Text,
}
#[derive(Clone, Copy, Debug, Default)]
pub struct Modifiers {
    pub shift: bool,
    pub alt: bool,
    pub ctrl: bool,
}
impl Modifiers {
    /// Linear endpoints bind to shapes only on request; Shift keeps its angle snap.
    pub fn attaches(self) -> bool {
        (self.ctrl || self.alt) && !self.shift
    }
}
#[derive(Clone)]
struct Change {
    before: Vec<Object>,
    after: Vec<Object>,
    /// The pile on both sides of the step; none for a step that only repainted the background.
    order: Option<Stacking>,
    /// Set when the step changed the capture background.
    background: Option<Repaint>,
}
/// Ids of the whole pile, bottom first, on both sides of one undo step: the objects
/// themselves do not carry their place.
#[derive(Clone)]
struct Stacking {
    before: Vec<u64>,
    after: Vec<u64>,
}
/// The capture background on both sides of one undo step.
#[derive(Clone, Copy)]
struct Repaint {
    before: Option<[u8; 3]>,
    after: Option<[u8; 3]>,
}
struct Gesture {
    origin: Point,
    current: Point,
    before: Vec<Object>,
    modifiers: Modifiers,
    kind: GestureKind,
}
enum GestureKind {
    Draw,
    Rotate {
        center: Point,
    },
    Move,
    Marquee,
    /// `bounds` is the axis-aligned selection; `opposite` and `corner` are frame corners.
    Resize {
        opposite: Point,
        corner: Point,
        bounds: Rect,
    },
    Endpoint {
        index: usize,
        start: bool,
    },
}

/// The selection as the user sees it: a single rotated object keeps its own turned box,
/// anything else is framed by its axis-aligned bounds (`angle` 0).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    /// Unrotated box, turned by `angle` around its center.
    pub bounds: Rect,
    pub angle: f64,
}
impl Frame {
    fn turn(self, p: Point) -> Point {
        rotate_point(p, self.bounds.center(), self.angle)
    }
    /// Corners in cyclic order, so the opposite of corner `i` is `(i + 2) % 4`.
    pub fn corners(self) -> [Point; 4] {
        self.bounds.corners().map(|p| self.turn(p))
    }
    /// Outline corners, `margin` outside the box.
    pub fn outline(self, margin: f64) -> [Point; 4] {
        self.bounds.expanded(margin).corners().map(|p| self.turn(p))
    }
    pub fn top(self) -> Point {
        self.turn(Point {
            x: self.bounds.center().x,
            y: self.bounds.min.y,
        })
    }
    /// Rotation handle, 28 px (at `scale` 1) beyond the top edge along the frame's own up
    /// direction.
    pub fn handle(self, scale: f64) -> Point {
        self.turn(Point {
            x: self.bounds.center().x,
            y: self.bounds.min.y - 28. * scale,
        })
    }
}

pub struct Editor {
    pub tool: Tool,
    pub style: Style,
    pub selected: Vec<u64>,
    /// Size factor of handles and pick tolerances: 1 at 96 DPI, 2.5 on a 250 % screen.
    pub ui_scale: f64,
    objects: Vec<Object>,
    /// Solid colour replacing the captured pixels of the region, under every object.
    background: Option<[u8; 3]>,
    undo: Vec<Change>,
    redo: Vec<Change>,
    gesture: Option<Gesture>,
    next_id: u64,
}
impl Default for Editor {
    fn default() -> Self {
        Self {
            tool: Tool::default(),
            style: Style::default(),
            selected: Vec::new(),
            ui_scale: 1.,
            objects: Vec::new(),
            background: None,
            undo: Vec::new(),
            redo: Vec::new(),
            gesture: None,
            next_id: 0,
        }
    }
}
impl Editor {
    /// An editor reopened on a saved document: `objects` in stacking order, ids up to
    /// `last_id`. Nothing to undo yet.
    pub fn restore(objects: Vec<Object>, last_id: u64, background: Option<[u8; 3]>) -> Self {
        Self {
            objects,
            background,
            next_id: last_id,
            ..Self::default()
        }
    }
    pub fn objects(&self) -> &[Object] {
        &self.objects
    }
    pub fn background(&self) -> Option<[u8; 3]> {
        self.background
    }
    /// Highest id given so far; a saved document keeps it so ids are never reused.
    pub fn last_id(&self) -> u64 {
        self.next_id
    }
    pub fn is_drawing(&self) -> bool {
        self.gesture.is_some()
    }
    pub fn select_only(&mut self, id: u64) {
        self.selected.clear();
        self.selected.push(id);
    }
    pub fn selection_bounds(&self) -> Option<Rect> {
        self.objects
            .iter()
            .filter(|o| self.selected.contains(&o.id))
            .map(|o| o.resolved_bounds(&self.objects))
            .reduce(|a, b| a.union(b))
    }
    pub fn selection_frame(&self) -> Option<Frame> {
        if let [id] = self.selected[..]
            && let Some(o) = self.objects.iter().find(|o| o.id == id)
            && o.rotation.abs() > 1e-9
            && arrow_points(o, &self.objects).is_none()
        {
            return Some(Frame {
                bounds: o.local_bounds(),
                angle: o.rotation,
            });
        }
        self.selection_bounds()
            .map(|bounds| Frame { bounds, angle: 0. })
    }
    pub fn rotation_handle(&self) -> Option<Point> {
        self.selection_frame().map(|f| f.handle(self.ui_scale))
    }
    pub fn begin_rotation(&mut self, p: Point) -> bool {
        if !p.finite() {
            return false;
        }
        self.cancel();
        let Some(frame) = self.selection_frame() else {
            return false;
        };
        self.gesture = Some(Gesture {
            origin: p,
            current: p,
            before: self.objects.clone(),
            modifiers: Modifiers::default(),
            kind: GestureKind::Rotate {
                center: frame.bounds.center(),
            },
        });
        true
    }
    pub fn marquee(&self) -> Option<Rect> {
        self.gesture
            .as_ref()
            .filter(|g| matches!(g.kind, GestureKind::Marquee))
            .and_then(|g| Rect::new(g.origin, g.current))
    }
}
