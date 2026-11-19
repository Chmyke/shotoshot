//! Exact, composited capture pixels: at most two content-sized checkpoints.
//! A stable prefix lets the last stroke change without rerasterizing earlier objects.
//! Export targets stay on the geometry path; their bitmaps are a different resource domain.
use super::*;
use shotoshot_core::blur::Zone;

#[derive(Default)]
pub(super) struct SceneCache {
    context: Option<Context>,
    prefix: Option<Checkpoint>,
    full: Option<Checkpoint>,
}

#[derive(PartialEq)]
struct Context {
    size: (u32, u32),
    skip: Option<u64>,
    underlay: Vec<Underlay>,
    /// The blurs under the objects, bottom first: a checkpoint holds their pixels.
    blurs: Vec<BlurPatch>,
}

struct Checkpoint {
    objects: Vec<Object>,
    bounds: D2D_RECT_U,
    bitmap: ID2D1Bitmap,
}

impl SceneCache {
    pub(super) fn invalidate(&mut self) {
        *self = Self::default();
    }

    pub(super) fn draw(
        &mut self,
        renderer: &Renderer,
        target: &ID2D1RenderTarget,
        device: &ID2D1DeviceContext,
        objects: &[Object],
        skip: Option<u64>,
        (underlay, blurs): (&[Underlay], &[BlurPatch]),
    ) -> Result<()> {
        let size = unsafe { target.GetPixelSize() };
        match &mut self.context {
            Some(c)
                if c.size == (size.width, size.height)
                    && c.skip == skip
                    && c.underlay == underlay =>
            {
                if c.blurs != blurs {
                    // A blur being drawn or moved changes its zones only: a checkpoint
                    // elsewhere still holds the right pixels.
                    let changed = changed_zones(&c.blurs, blurs);
                    c.blurs = blurs.to_vec();
                    for slot in [&mut self.prefix, &mut self.full] {
                        if slot.as_ref().is_some_and(|checkpoint| {
                            changed
                                .iter()
                                .any(|zone| overlaps(checkpoint.bounds, *zone))
                        }) {
                            *slot = None;
                        }
                    }
                }
            }
            _ => {
                self.invalidate();
                self.context = Some(Context {
                    size: (size.width, size.height),
                    skip,
                    underlay: underlay.to_vec(),
                    blurs: blurs.to_vec(),
                });
            }
        }
        if objects.is_empty() {
            self.invalidate();
            return renderer.annotations_except(target, objects, skip);
        }
        if let Some(full) = &self.full
            && same_pile(&full.objects, objects)
        {
            return full.restore(target, device);
        }
        let mut common = self.full.as_ref().map_or(0, |full| {
            full.objects
                .iter()
                .zip(objects)
                .take_while(|(a, b)| same(a, b))
                .count()
        });
        // An unchanged arrow or fill can depend on a later shape: its target, or a shape
        // it spares. Conservatively redraw from the first of them when anything changes,
        // without quadratic lookups.
        if let Some(index) = objects[..common].iter().position(Object::depends_on_others) {
            common = index;
        }
        if self
            .full
            .as_ref()
            .is_some_and(|full| full.objects.len() == common)
        {
            std::mem::swap(&mut self.prefix, &mut self.full);
        }
        let restored = match &self.prefix {
            Some(prefix)
                if prefix.objects.len() <= common
                    && same_pile(&prefix.objects, &objects[..prefix.objects.len()]) =>
            {
                prefix.restore(target, device)?;
                prefix.objects.len()
            }
            _ => 0,
        };
        if restored < common {
            renderer.annotation_range(target, objects, skip, restored..common)?;
        }
        if common == 0 {
            self.prefix = None;
        } else if restored != common {
            Checkpoint::capture(
                &mut self.prefix,
                renderer,
                target,
                &objects[..common],
                objects,
                skip,
            )?;
        }
        if common == objects.len() && self.prefix.is_some() {
            self.full = self.prefix.take();
            return Ok(());
        }
        renderer.annotation_range(target, objects, skip, common..objects.len())?;
        Checkpoint::capture(&mut self.full, renderer, target, objects, objects, skip)
    }
}

impl Checkpoint {
    fn restore(&self, target: &ID2D1RenderTarget, device: &ID2D1DeviceContext) -> Result<()> {
        // Copy, rather than source-over: live pixels already include their original
        // alpha (including the 1/255 input surface). Reblending would darken them.
        unsafe {
            let previous = device.GetPrimitiveBlend();
            device.SetPrimitiveBlend(D2D1_PRIMITIVE_BLEND_COPY);
            target.DrawBitmap(
                &self.bitmap,
                Some(&D2D_RECT_F {
                    left: self.bounds.left as f32,
                    top: self.bounds.top as f32,
                    right: self.bounds.right as f32,
                    bottom: self.bounds.bottom as f32,
                }),
                1.,
                D2D1_BITMAP_INTERPOLATION_MODE_NEAREST_NEIGHBOR,
                None,
            );
            device.SetPrimitiveBlend(previous);
        }
        Ok(())
    }

    fn capture(
        slot: &mut Option<Self>,
        renderer: &Renderer,
        target: &ID2D1RenderTarget,
        prefix: &[Object],
        objects: &[Object],
        skip: Option<u64>,
    ) -> Result<()> {
        let Some(bounds) = bounds(renderer, prefix, objects, skip, unsafe {
            target.GetPixelSize()
        })?
        else {
            *slot = None;
            return Ok(());
        };
        let size = D2D_SIZE_U {
            width: bounds.right - bounds.left,
            height: bounds.bottom - bounds.top,
        };
        if slot
            .as_ref()
            .is_some_and(|old| unsafe { old.bitmap.GetPixelSize() } != size)
        {
            *slot = None;
        }
        if slot.is_none() {
            let bitmap = unsafe {
                target.CreateBitmap(
                    size,
                    None,
                    0,
                    &D2D1_BITMAP_PROPERTIES {
                        pixelFormat: target.GetPixelFormat(),
                        dpiX: 96.,
                        dpiY: 96.,
                    },
                )?
            };
            *slot = Some(Self {
                objects: Vec::new(),
                bounds,
                bitmap,
            });
        }
        let checkpoint = slot.as_mut().unwrap();
        unsafe {
            checkpoint
                .bitmap
                .CopyFromRenderTarget(None, target, Some(&bounds))?
        };
        checkpoint.bounds = bounds;
        // Every frame of a gesture captures again: keep the objects already held (a long
        // stroke's points) and copy only from the first that differs.
        let kept = checkpoint
            .objects
            .iter()
            .zip(prefix)
            .take_while(|(a, b)| a == b)
            .count();
        checkpoint.objects.truncate(kept);
        checkpoint.objects.extend_from_slice(&prefix[kept..]);
        Ok(())
    }
}

/// Equal as a checkpoint sees them: a blur draws nothing there, its zone is the context's.
fn same(a: &Object, b: &Object) -> bool {
    a == b || (a.is_blur() && b.is_blur() && a.id == b.id)
}

fn same_pile(a: &[Object], b: &[Object]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(a, b)| same(a, b))
}

/// The zones whose pixels differ between two piles of blurs: those of every place where
/// they differ, in either pile, so a reordering counts as well as a move.
fn changed_zones(old: &[BlurPatch], new: &[BlurPatch]) -> Vec<Zone> {
    (0..old.len().max(new.len()))
        .filter(|&i| old.get(i) != new.get(i))
        .flat_map(|i| [old.get(i), new.get(i)])
        .flatten()
        .map(|patch| patch.zone)
        .collect()
}

fn overlaps(bounds: D2D_RECT_U, zone: Zone) -> bool {
    zone.x < bounds.right
        && bounds.left < zone.x + zone.width
        && zone.y < bounds.bottom
        && bounds.top < zone.y + zone.height
}

fn bounds(
    renderer: &Renderer,
    prefix: &[Object],
    objects: &[Object],
    skip: Option<u64>,
    size: D2D_SIZE_U,
) -> Result<Option<D2D_RECT_U>> {
    // A blur draws nothing over the capture here: `Canvas::blurs` laid it under.
    let bounds = prefix
        .iter()
        .filter(|o| Some(o.id) != skip && !o.is_blur())
        .map(|object| {
            if matches!(object.shape, Shape::Ellipse(_)) {
                // Ellipses are rendered as polygons: a flat ellipse can have sharp
                // miter tips beyond its analytic radius plus half the stroke width.
                let geometry = unsafe {
                    let geometry = renderer.factory.CreatePathGeometry()?;
                    let sink = geometry.Open()?;
                    for path in paths(object, objects) {
                        if let Some(first) = path.first() {
                            sink.BeginFigure(vector(*first), D2D1_FIGURE_BEGIN_HOLLOW);
                            for point in &path[1..] {
                                sink.AddLine(vector(*point));
                            }
                            sink.EndFigure(D2D1_FIGURE_END_CLOSED);
                        }
                    }
                    sink.Close()?;
                    geometry.GetWidenedBounds(object.style.width as f32, None, None, 0.25)?
                };
                return Ok(Rect {
                    min: Point {
                        x: geometry.left as f64,
                        y: geometry.top as f64,
                    },
                    max: Point {
                        x: geometry.right as f64,
                        y: geometry.bottom as f64,
                    },
                }
                .expanded(2.));
            }
            let margin = match &object.shape {
                Shape::Stroke {
                    widths, highlight, ..
                } => {
                    widths.iter().copied().fold(object.style.width, f64::max)
                        * if *highlight { 2.5 } else { 0.5 }
                        + 2.
                }
                Shape::Text { .. } => 32., // rotated 20 px layout padding + antialiasing
                Shape::Arrow { .. } => object.style.width * 5.3 + 2.,
                // The 90-degree miter rotates with the rectangle; its furthest
                // coordinate can extend by width / sqrt(2), rather than width / 2.
                Shape::Rectangle(_) => object.style.width * std::f64::consts::FRAC_1_SQRT_2 + 2.,
                _ => object.style.width * 0.5 + 2.,
            };
            Ok(object.resolved_bounds(objects).expanded(margin))
        })
        .try_fold(None::<Rect>, |acc, next: Result<Rect>| {
            let next = next?;
            Ok::<_, windows::core::Error>(Some(acc.map_or(next, |bounds| bounds.union(next))))
        })?;
    let Some(bounds) = bounds else {
        return Ok(None);
    };
    let result = D2D_RECT_U {
        left: bounds.min.x.floor().clamp(0., size.width as f64) as u32,
        top: bounds.min.y.floor().clamp(0., size.height as f64) as u32,
        right: bounds.max.x.ceil().clamp(0., size.width as f64) as u32,
        bottom: bounds.max.y.ceil().clamp(0., size.height as f64) as u32,
    };
    Ok((result.left < result.right && result.top < result.bottom).then_some(result))
}
