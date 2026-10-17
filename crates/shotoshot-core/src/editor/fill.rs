//! ROLE: The bucket: one click colours what it touches, drawn ink first, then the closed
//! shape under it, or else the capture.
//! INVARIANTS: A click changes at most one colour and adds at most one undo step; hovering
//! previews exactly the change a click would make (`fill_change`).
use super::*;

/// Reach of the bucket around drawn ink at 100 %. Tighter than the selection reach, so the
/// inside of a small shape stays easy to aim at.
const INK_REACH: f64 = 3.;

/// What one bucket click changes; indices are places in the pile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FillChange {
    /// The object's ink takes `color` (no change when it already has it).
    Ink { index: usize, color: [u8; 3] },
    /// The closed shape's fill becomes `fill`; `None` removes it.
    Fill { index: usize, fill: Option<[u8; 3]> },
    /// The capture background becomes this; `None` gives the captured pixels back.
    Background(Option<[u8; 3]>),
}

impl Editor {
    /// Colours with the current colour: ink showing under the point is recoloured, else the
    /// closed shape there is filled, or else the capture; the same fill colour again removes
    /// the fill. `clear` removes the fill seen under the point instead, whatever its colour,
    /// and leaves ink alone.
    pub fn fill_at(&mut self, p: Point, clear: bool) {
        if !p.finite() {
            return;
        }
        self.cancel();
        let Some(change) = self.fill_change(p, clear) else {
            return;
        };
        let before = self.objects.clone();
        match change {
            FillChange::Ink { index, color } => self.objects[index].style.color = color,
            FillChange::Fill { index, fill } => self.objects[index].fill = fill,
            FillChange::Background(background) => return self.set_background(background),
        }
        self.commit(before);
    }
    /// The change `fill_at` would make at the point, for the bucket's hover preview.
    pub fn fill_change(&self, p: Point, clear: bool) -> Option<FillChange> {
        if !p.finite() {
            return None;
        }
        let color = self.style.color;
        if !clear && let Some(index) = self.ink_at(p) {
            return Some(FillChange::Ink { index, color });
        }
        // Clearing looks through hollow shapes: what shows inside them is the background.
        let target = self
            .objects
            .iter()
            .rposition(|o| o.contains(p) && (!clear || o.fill.is_some()));
        let toggled = |fill: Option<[u8; 3]>| (!clear && fill != Some(color)).then_some(color);
        Some(match target {
            Some(index) => FillChange::Fill {
                index,
                fill: toggled(self.objects[index].fill),
            },
            None => FillChange::Background(toggled(self.background)),
        })
    }
    /// Topmost object whose ink shows under the point: a fill above it hides it, a hollow
    /// shape does not.
    fn ink_at(&self, p: Point) -> Option<usize> {
        let reach = INK_REACH * self.ui_scale;
        for (index, o) in self.objects.iter().enumerate().rev() {
            // `on_ink` measures a rectangle from its centre line, not from its drawn edge.
            let half = match o.shape {
                Shape::Rectangle(_) => o.style.width * 0.5,
                _ => 0.,
            };
            if o.on_ink(p, reach + half, &self.objects) {
                return Some(index);
            }
            if o.fill.is_some() && o.contains(p) {
                return None;
            }
        }
        None
    }
}
