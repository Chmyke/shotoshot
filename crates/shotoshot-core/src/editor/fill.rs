//! ROLE: The bucket: one click colours what it touches, drawn ink first, then the closed
//! shape under it, or else the capture.
//! INVARIANTS: A click changes at most one colour and adds at most one undo step.
use super::*;

/// Reach of the bucket around drawn ink at 100 %. Tighter than the selection reach, so the
/// inside of a small shape stays easy to aim at.
const INK_REACH: f64 = 3.;

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
        let color = self.style.color;
        if !clear && let Some(index) = self.ink_at(p) {
            let before = self.objects.clone();
            self.objects[index].style.color = color;
            self.commit(before);
            return;
        }
        // Clearing looks through hollow shapes: what shows inside them is the background.
        let target = self
            .objects
            .iter()
            .rposition(|o| o.contains(p) && (!clear || o.fill.is_some()));
        let toggled = |fill: Option<[u8; 3]>| (!clear && fill != Some(color)).then_some(color);
        match target {
            Some(index) => {
                let before = self.objects.clone();
                self.objects[index].fill = toggled(self.objects[index].fill);
                self.commit(before);
            }
            None => self.set_background(toggled(self.background)),
        }
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
