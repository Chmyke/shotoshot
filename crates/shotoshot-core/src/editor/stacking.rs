//! ROLE: Moves the selection through the pile of objects (Page Up / Page Down).
//! INVARIANTS: A step is visible or nothing happens; the selection moves as one block in
//! its own order; each move is one undo step.
use super::*;

impl Editor {
    /// The selection climbs past the nearest object above it that overlaps it, so every
    /// press shows; `to_end` puts it on top of the pile.
    pub fn raise_selection(&mut self, to_end: bool) {
        self.restack(true, to_end);
    }
    /// The mirror of `raise_selection`: under the nearest overlapping object below, or at
    /// the bottom of the pile.
    pub fn lower_selection(&mut self, to_end: bool) {
        self.restack(false, to_end);
    }
    fn restack(&mut self, up: bool, to_end: bool) {
        self.cancel();
        let Some(bounds) = self.selection_bounds() else {
            return;
        };
        let before = self.objects.clone();
        let selected = |o: &Object| self.selected.contains(&o.id);
        let (block, rest): (Vec<Object>, Vec<Object>) = before.iter().cloned().partition(selected);
        if rest.is_empty() {
            return;
        }
        // The neighbour the block steps over: the first object past the block's edge, in the
        // direction of travel, that overlaps the selection.
        let crossed = if to_end {
            None
        } else {
            let edge = if up {
                before.iter().rposition(selected)
            } else {
                before.iter().position(selected)
            };
            let Some(edge) = edge else {
                return;
            };
            let overlaps =
                |o: &Object| !selected(o) && o.resolved_bounds(&before).intersects(bounds);
            let found = if up {
                before[edge + 1..].iter().find(|o| overlaps(o))
            } else {
                before[..edge].iter().rev().find(|o| overlaps(o))
            };
            match found {
                Some(o) => Some(o.id),
                None => return,
            }
        };
        let mut pile = Vec::with_capacity(before.len());
        if to_end && !up {
            pile.extend(block.iter().cloned());
        }
        for o in rest {
            let crossing = crossed == Some(o.id);
            if crossing && !up {
                pile.extend(block.iter().cloned());
            }
            pile.push(o);
            if crossing && up {
                pile.extend(block.iter().cloned());
            }
        }
        if to_end && up {
            pile.extend(block.iter().cloned());
        }
        self.objects = pile;
        self.commit(before);
    }
}
