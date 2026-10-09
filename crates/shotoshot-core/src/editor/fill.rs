//! ROLE: The bucket: one click fills the closed shape under it, or else the capture.
//! INVARIANTS: A click changes at most one fill and adds at most one undo step.
use super::*;

impl Editor {
    /// Fills with the current colour; the same colour again removes the fill. `clear`
    /// removes the fill seen under the point instead, whatever its colour.
    pub fn fill_at(&mut self, p: Point, clear: bool) {
        if !p.finite() {
            return;
        }
        self.cancel();
        let color = self.style.color;
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
}
