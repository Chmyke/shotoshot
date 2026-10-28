//! ROLE: Owns the editable document and reversible gesture transactions.
//! OWNER: One capture session; no operating system resources.
//! INVARIANTS: A gesture commits once; image pixels never enter undo history.
mod erase;
mod fill;
mod gestures;
mod history;
mod stacking;
use crate::*;
pub use fill::FillChange;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tool {
    Select,
    Rectangle,
    Ellipse,
    Line,
    /// The most useful tool, so every new capture starts with it.
    #[default]
    Arrow,
    Pen,
    Highlight,
    Text,
    /// A text on a rounded panel: a click fits the panel to the text, a drag sets its width.
    Label,
    /// A disc that removes the freehand ink it passes over and whole other objects.
    Eraser,
}
/// The eraser's diameter at 100 % until the wheel changes it, in pixels.
pub const ERASER_SIZE: f64 = 14.;
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
#[derive(Clone)]
struct Repaint {
    before: Vec<Paint>,
    after: Vec<Paint>,
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
    /// Objects swept so far, in pile order; removed together on release.
    Erase {
        touched: Vec<u64>,
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
    /// Diameter of the eraser's disc, in frame pixels.
    pub eraser_size: f64,
    objects: Vec<Object>,
    /// Coats over the captured pixels of the region, bottom first, under every object.
    background: Vec<Paint>,
    undo: Vec<Change>,
    redo: Vec<Change>,
    gesture: Option<Gesture>,
    next_id: u64,
    /// Modifier keys last reported, gesture or not: the bucket's preview follows Alt.
    held: Modifiers,
}
impl Default for Editor {
    fn default() -> Self {
        Self {
            tool: Tool::default(),
            style: Style::default(),
            selected: Vec::new(),
            ui_scale: 1.,
            eraser_size: ERASER_SIZE,
            objects: Vec::new(),
            background: Vec::new(),
            undo: Vec::new(),
            redo: Vec::new(),
            gesture: None,
            next_id: 0,
            held: Modifiers::default(),
        }
    }
}
impl Editor {
    /// An editor reopened on a saved document: `objects` in stacking order, ids up to
    /// `last_id`. Nothing to undo yet.
    pub fn restore(objects: Vec<Object>, last_id: u64, background: Vec<Paint>) -> Self {
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
    /// The capture's coats, bottom first; empty when the captured pixels show.
    pub fn background(&self) -> &[Paint] {
        &self.background
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
        if let [id] = self.selected[..] {
            return self.frame_of(id);
        }
        self.selection_bounds()
            .map(|bounds| Frame { bounds, angle: 0. })
    }
    /// The frame the object would get selected alone: turned with it when it is rotated
    /// and not an arrow.
    pub fn frame_of(&self, id: u64) -> Option<Frame> {
        let o = self.objects.iter().find(|o| o.id == id)?;
        Some(
            if o.rotation.abs() > 1e-9 && arrow_points(o, &self.objects).is_none() {
                Frame {
                    bounds: o.local_bounds(),
                    angle: o.rotation,
                }
            } else {
                Frame {
                    bounds: o.resolved_bounds(&self.objects),
                    angle: 0.,
                }
            },
        )
    }
    pub fn held(&self) -> Modifiers {
        self.held
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
