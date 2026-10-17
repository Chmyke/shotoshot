//! ROLE: What the bucket pours on a zone, a closed shape or the capture: coats of colour that
//! each spare the inside of the closed shapes standing on the zone when it was poured: an
//! outline already drawn stops the colour (Mickaël, 2026-10-17).
//! INVARIANTS: A wall is an id, so the spared inside follows its shape wherever it moves; a wall
//! that is gone, open or below its zone spares nothing. A shape drawn after a coat is no wall
//! of it.
use crate::{Object, Point};

/// One bucket click on a zone.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Paint {
    pub color: [u8; 3],
    /// Ids of the closed shapes above the zone that touched it when the coat was poured.
    pub walls: Vec<u64>,
}

impl Paint {
    /// A coat that spares nothing.
    pub fn solid(color: [u8; 3]) -> Self {
        Self {
            color,
            walls: Vec::new(),
        }
    }
    /// The shapes whose inside this coat spares. `floor` is the first place above the zone:
    /// its shape's index plus one, 0 for the capture.
    pub fn holes<'a>(
        &'a self,
        objects: &'a [Object],
        floor: usize,
    ) -> impl Iterator<Item = &'a Object> {
        objects
            .get(floor..)
            .unwrap_or_default()
            .iter()
            .filter(|o| o.closed() && self.walls.contains(&o.id))
    }
}

/// The topmost of a zone's coats that shows at `p`, a point of the zone.
pub fn coat_seen(coats: &[Paint], p: Point, objects: &[Object], floor: usize) -> Option<usize> {
    coats
        .iter()
        .rposition(|c| !c.holes(objects, floor).any(|o| o.contains(p)))
}

/// `coat` laid over a zone's coats. A coat that spares no more than one below hides it
/// entirely, so that one goes: repainting the same zone keeps a single coat.
pub fn pour(coats: &mut Vec<Paint>, coat: Paint) {
    coats.retain(|below| !coat.walls.iter().all(|w| below.walls.contains(w)));
    coats.push(coat);
}

/// Wall ids through `map`; a wall `map` does not know is dropped.
pub fn remap_walls(coats: &mut [Paint], map: impl Fn(u64) -> Option<u64>) {
    for coat in coats {
        coat.walls = coat.walls.iter().filter_map(|w| map(*w)).collect();
    }
}
