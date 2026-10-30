//! ROLE: The bucket: one click colours what it touches, drawn ink first, then the closed
//! shape under it, or else the capture; an outline already there stops the colour (`paint`).
//! INVARIANTS: A click changes at most one colour and adds at most one undo step; hovering
//! previews exactly the change a click would make (`fill_change`).
use super::*;

/// Reach of the bucket around drawn ink at 100 %. Tighter than the selection reach, so the
/// inside of a small shape stays easy to aim at.
const INK_REACH: f64 = 3.;

/// What one bucket click changes; indices are places in the pile. A zone is the closed shape
/// at that place, or the capture when `None`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FillChange {
    /// The object's ink takes `color` (no change when it already has it).
    Ink { index: usize, color: [u8; 3] },
    /// `coat` laid over the zone (`pour`).
    Pour { zone: Option<usize>, coat: Paint },
    /// The zone's coat at `coat`, the one seen under the point, taken off.
    Remove { zone: Option<usize>, coat: usize },
}

impl Editor {
    /// Colours with the current colour: ink showing under the point is recoloured, else the
    /// closed shape there, or else the capture, gets a coat that spares the closed shapes
    /// standing on it; the coat seen there in the same colour is taken off instead. `clear`
    /// takes off the coat seen under the point, whatever its colour, and leaves ink alone.
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
            FillChange::Pour {
                zone: Some(index),
                coat,
            } => pour(&mut self.objects[index].fill, coat),
            FillChange::Remove {
                zone: Some(index),
                coat,
            } => {
                self.objects[index].fill.remove(coat);
            }
            FillChange::Pour { zone: None, coat } => {
                let mut background = self.background.clone();
                pour(&mut background, coat);
                return self.set_background(background);
            }
            FillChange::Remove { zone: None, coat } => {
                let mut background = self.background.clone();
                background.remove(coat);
                return self.set_background(background);
            }
        }
        self.commit(before);
    }
    /// The change `fill_at` would make at the point, for the bucket's hover preview.
    pub fn fill_change(&self, p: Point, clear: bool) -> Option<FillChange> {
        if !p.finite() {
            return None;
        }
        let objects = &self.objects;
        if clear {
            // Clearing looks through hollow insides and spared ones: it takes the coat seen.
            let seen = objects.iter().enumerate().rev().find_map(|(index, o)| {
                if !o.contains(p) {
                    return None;
                }
                let coat = coat_seen(&o.fill, p, objects, index + 1)?;
                Some(FillChange::Remove {
                    zone: Some(index),
                    coat,
                })
            });
            return seen.or_else(|| {
                Some(FillChange::Remove {
                    zone: None,
                    coat: coat_seen(&self.background, p, objects, 0)?,
                })
            });
        }
        let color = self.style.color;
        if let Some(index) = self.ink_at(p) {
            return Some(FillChange::Ink { index, color });
        }
        let zone = objects.iter().rposition(|o| o.contains(p));
        let (coats, floor) = match zone {
            Some(index) => (&objects[index].fill[..], index + 1),
            None => (&self.background[..], 0),
        };
        if let Some(coat) = coat_seen(coats, p, objects, floor)
            && coats[coat].color == color
        {
            return Some(FillChange::Remove { zone, coat });
        }
        Some(FillChange::Pour {
            zone,
            coat: Paint {
                color,
                walls: self.walls(zone),
            },
        })
    }
    /// The closed shapes above the zone that touch it now: a coat poured now spares them.
    fn walls(&self, zone: Option<usize>) -> Vec<u64> {
        let (floor, reach) = match zone {
            Some(index) => (index + 1, Some(self.objects[index].bounds())),
            None => (0, None),
        };
        self.objects[floor..]
            .iter()
            .filter(|o| o.closed() && reach.is_none_or(|r| r.intersects(o.bounds())))
            .map(|o| o.id)
            .collect()
    }
    /// Topmost object whose ink shows under the point: a fill above it hides it, a hollow
    /// shape or a spared inside does not.
    fn ink_at(&self, p: Point) -> Option<usize> {
        let reach = INK_REACH * self.ui_scale;
        for (index, o) in self.objects.iter().enumerate().rev() {
            // A blur has no ink: the bucket colours what lies under it.
            if o.is_blur() {
                continue;
            }
            // `on_ink` measures a rectangle from its centre line, not from its drawn edge.
            let half = match o.shape {
                Shape::Rectangle(_) => o.style.width * 0.5,
                _ => 0.,
            };
            if o.on_ink(p, reach + half, &self.objects) {
                return Some(index);
            }
            if o.contains(p) && coat_seen(&o.fill, p, &self.objects, index + 1).is_some() {
                return None;
            }
        }
        None
    }
}
