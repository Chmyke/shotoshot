//! Final widened pen/highlighter outlines, owned by their Direct2D factory's renderer.
//! Exact snapshots detect edits and undo without requiring model revision counters.
use super::*;
use std::collections::{HashMap, hash_map::Entry};

#[derive(Default)]
pub(super) struct StrokeCache {
    entries: HashMap<u64, CachedStroke>,
}

struct CachedStroke {
    object: Object,
    outline: ID2D1PathGeometry,
    used: bool,
}

impl StrokeCache {
    pub(super) fn retain_present(&mut self, objects: &[Object]) {
        self.begin_frame();
        for object in objects {
            if matches!(object.shape, Shape::Stroke { .. })
                && let Some(entry) = self.entries.get_mut(&object.id)
            {
                entry.used = true;
            }
        }
        self.end_frame();
    }

    pub(super) fn begin_frame(&mut self) {
        for entry in self.entries.values_mut() {
            entry.used = false;
        }
    }

    pub(super) fn outline(
        &mut self,
        renderer: &Renderer,
        object: &Object,
    ) -> Result<&ID2D1PathGeometry> {
        let cached = match self.entries.entry(object.id) {
            Entry::Occupied(entry) => {
                let cached = entry.into_mut();
                if cached.object != *object {
                    *cached = CachedStroke::new(renderer, object)?;
                }
                cached.used = true;
                cached
            }
            Entry::Vacant(entry) => entry.insert(CachedStroke::new(renderer, object)?),
        };
        Ok(&cached.outline)
    }

    pub(super) fn end_frame(&mut self) {
        // One scan of the cache, rather than a document search for every cached id.
        self.entries.retain(|_, entry| entry.used);
    }
}

impl CachedStroke {
    fn new(renderer: &Renderer, object: &Object) -> Result<Self> {
        // Fill the union once, so translucent highlighter runs do not become darker
        // where two thicknesses meet. The geometry is independent of the draw target.
        #[cfg(test)]
        stroke_tests::record_build();
        let outline = unsafe {
            let outline = renderer.factory.CreatePathGeometry()?;
            let outline_sink = outline.Open()?;
            outline_sink.SetFillMode(D2D1_FILL_MODE_WINDING);
            for (points, width) in shotoshot_core::stroke_runs(object) {
                let geometry = renderer.factory.CreatePathGeometry()?;
                let sink = geometry.Open()?;
                sink.BeginFigure(vector(points[0]), D2D1_FIGURE_BEGIN_HOLLOW);
                for p in &points[1..] {
                    sink.AddLine(vector(*p));
                }
                sink.EndFigure(D2D1_FIGURE_END_OPEN);
                sink.Close()?;
                geometry.Widen(
                    width as f32,
                    &renderer.round_stroke,
                    None,
                    0.1,
                    &outline_sink,
                )?;
            }
            outline_sink.Close()?;
            outline
        };
        Ok(Self {
            object: object.clone(),
            outline,
            used: true,
        })
    }
}
