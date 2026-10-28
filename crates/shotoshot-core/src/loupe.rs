//! ROLE: Geometry of the live loupe: where its window sits, which screen pixels it enlarges,
//! where each window under it is drawn inside it, and how its edge fades into the screen.
//! OWNER: The platform loupe asks for a frame on each pointer move and places its window
//! thumbnails with `stack`.
//! INVARIANTS: Device pixels throughout. Every source pixel becomes an exact `zoom` x `zoom`
//! block, and the pointer's pixel is the block at the loupe's centre. The loupe itself is never
//! a source: the platform leaves its own windows out of the list it passes.

/// A rectangle of whole device pixels; `right` and `bottom` are excluded.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PixelRect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}
impl PixelRect {
    pub fn width(&self) -> i32 {
        self.right - self.left
    }
    pub fn height(&self) -> i32 {
        self.bottom - self.top
    }
    pub fn intersection(&self, other: &Self) -> Option<Self> {
        let cut = Self {
            left: self.left.max(other.left),
            top: self.top.max(other.top),
            right: self.right.min(other.right),
            bottom: self.bottom.min(other.bottom),
        };
        (cut.left < cut.right && cut.top < cut.bottom).then_some(cut)
    }
}

/// Where the loupe sits and what it enlarges.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Frame {
    /// The loupe's window, on screen.
    pub window: PixelRect,
    /// The screen pixels it shows, `zoom` times larger.
    pub source: PixelRect,
    pub zoom: i32,
}

/// The loupe of about `diameter` device pixels around the pointer pixel `(x, y)`. The window is
/// a whole number of source pixels wide, so its side may be a little under `diameter`.
pub fn frame(x: i32, y: i32, diameter: i32, zoom: i32) -> Frame {
    let zoom = zoom.max(1);
    let side = (diameter / zoom).max(1);
    let source = PixelRect {
        left: x - side / 2,
        top: y - side / 2,
        right: x - side / 2 + side,
        bottom: y - side / 2 + side,
    };
    let window = PixelRect {
        left: x - (side / 2) * zoom,
        top: y - (side / 2) * zoom,
        right: x - (side / 2) * zoom + side * zoom,
        bottom: y - (side / 2) * zoom + side * zoom,
    };
    Frame {
        window,
        source,
        zoom,
    }
}

/// One window drawn in the loupe: the part of it shown, in its own coordinates, and where it
/// lands, in the loupe's.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Placement<T> {
    pub window: T,
    pub source: PixelRect,
    pub destination: PixelRect,
}

/// Horizontal bands of source pixels whose enlargement fills the round loupe: DWM clips a
/// thumbnail to its rectangle only, never to the window's round region, so the loupe is drawn
/// as a stack of rectangles. Each band covers the disc inside the rim, whose width is `rim`
/// window pixels centred on the window's edge, and never reaches past the rim's outer edge:
/// the rim hides the steps.
pub fn bands(frame: &Frame, rim: i32) -> Vec<PixelRect> {
    let z = frame.zoom;
    let side = frame.source.height();
    let centre = frame.window.height() as f64 / 2.;
    let inner = centre - rim as f64 / 2.;
    let outer = centre + rim as f64 / 2.;
    // Half-width of a disc of `radius` at the band's row nearest to, or farthest from, the centre.
    let half = |radius: f64, dy: f64| (radius * radius - dy * dy).max(0.).sqrt();
    let reach = |first: i32, end: i32| {
        let (top, bottom) = ((first * z) as f64, (end * z) as f64);
        let near = if (top..=bottom).contains(&centre) {
            0.
        } else {
            (top - centre).abs().min((bottom - centre).abs())
        };
        let far = (top - centre).abs().max((bottom - centre).abs());
        // Source columns each side of the centre that cover the inner disc, and whether they
        // stay within the rim.
        let columns = (half(inner, near) / z as f64).ceil() as i32;
        (
            columns,
            (columns * z) as f64 <= half(outer, far) || columns == 0,
        )
    };
    let mut bands = Vec::new();
    let mut first = 0;
    while first < side {
        let mut end = first + 1;
        while end < side && reach(first, end + 1).1 {
            end += 1;
        }
        let (columns, _) = reach(first, end);
        if columns > 0 {
            let middle = frame.source.left + frame.source.width() / 2;
            bands.push(PixelRect {
                left: (middle - columns).max(frame.source.left),
                top: frame.source.top + first,
                right: (middle + columns).min(frame.source.right),
                bottom: frame.source.top + end,
            });
        }
        first = end;
    }
    bands
}

/// The windows under the loupe, given top first with their screen rectangles, in the order to
/// draw them: bottom first, so the last drawn is on top, each cut into the `areas` of source
/// pixels the loupe shows (its `bands`, or the whole source). Windows outside are left out.
pub fn stack<T: Copy>(
    frame: &Frame,
    areas: &[PixelRect],
    top_first: &[(T, PixelRect)],
) -> Vec<Placement<T>> {
    top_first
        .iter()
        .rev()
        .flat_map(|(window, rect)| areas.iter().map(move |area| (*window, *rect, *area)))
        .filter_map(|(window, rect, area)| {
            let shown = rect.intersection(&area)?;
            let z = frame.zoom;
            Some(Placement {
                window,
                source: PixelRect {
                    left: shown.left - rect.left,
                    top: shown.top - rect.top,
                    right: shown.right - rect.left,
                    bottom: shown.bottom - rect.top,
                },
                destination: PixelRect {
                    left: (shown.left - frame.source.left) * z,
                    top: (shown.top - frame.source.top) * z,
                    right: (shown.right - frame.source.left) * z,
                    bottom: (shown.bottom - frame.source.top) * z,
                },
            })
        })
        .collect()
}

/// Width of the loupe's fading edge, in device pixels, whatever its size: wide enough that no
/// line shows where the enlargement meets the screen (Mickaël, 2026-10-28: "it must really be
/// invisible"), narrow enough to stay an edge (then: "3 or 4 pixels at most", a tenth of the
/// loupe was far too wide).
pub fn feather(_side: i32) -> i32 {
    4
}

/// One disc of the fading edge: its radius in device pixels and its opacity.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ring {
    pub radius: f64,
    pub opacity: f64,
}

/// Discs of the same enlargement, centred on the loupe and laid over one another, whose summed
/// coverage is 1 within `radius - width`, then falls smoothly (smoothstep) to 0 at `radius`
/// in `steps` steps. The first disc is opaque; each next one is wider and fainter. Coverage at
/// a distance is 1 less the product of `1 - opacity` over the discs that reach it, whatever
/// their order.
pub fn feathered(radius: f64, width: f64, steps: usize) -> Vec<Ring> {
    let steps = steps.max(1);
    let inner = radius - width;
    // Coverage wanted between disc k - 1 and disc k, k from 1: the middle of that step.
    let wanted = |k: usize| {
        let t = (k as f64 - 0.5) / steps as f64;
        1. - t * t * (3. - 2. * t)
    };
    let mut rings = vec![Ring {
        radius: inner,
        opacity: 1.,
    }];
    for k in 1..=steps {
        // What lets the screen through at step k is the product over discs k to `steps`.
        let through = 1. - wanted(k);
        let beyond = if k == steps { 1. } else { 1. - wanted(k + 1) };
        rings.push(Ring {
            radius: inner + width * k as f64 / steps as f64,
            opacity: 1. - through / beyond,
        });
    }
    rings
}
/// The coverage of `rings` at `distance` from their centre.
pub fn coverage(rings: &[Ring], distance: f64) -> f64 {
    1. - rings
        .iter()
        .filter(|ring| distance < ring.radius)
        .map(|ring| 1. - ring.opacity)
        .product::<f64>()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(left: i32, top: i32, right: i32, bottom: i32) -> PixelRect {
        PixelRect {
            left,
            top,
            right,
            bottom,
        }
    }

    #[test]
    fn the_edge_fades_smoothly_from_the_enlargement_to_the_screen() {
        assert_eq!(feather(240), 4);
        assert_eq!(feather(320), 4);
        for steps in [4, 8, 12] {
            let rings = feathered(120., 4., steps);
            assert_eq!(rings.len(), steps + 1);
            assert_eq!(
                rings[0],
                Ring {
                    radius: 116.,
                    opacity: 1.
                }
            );
            assert!((rings[steps].radius - 120.).abs() < 1e-9);
            assert!(rings.iter().all(|r| (0. ..=1.).contains(&r.opacity)));
            assert_eq!(coverage(&rings, 50.), 1.);
            assert_eq!(coverage(&rings, 115.9), 1.);
            assert_eq!(coverage(&rings, 120.), 0.);
            // Falls at every step, never by more than the steps allow.
            let mut last = 1.;
            for tenth in 1160..1200 {
                let now = coverage(&rings, tenth as f64 / 10.);
                assert!(now <= last + 1e-12, "{steps}: rises at {tenth}");
                assert!(last - now <= 1.6 / steps as f64, "{steps}: drop at {tenth}");
                last = now;
            }
        }
    }

    #[test]
    fn every_source_pixel_is_an_exact_block_and_the_pointer_sits_at_the_centre() {
        for zoom in [2, 3, 4] {
            for (x, y) in [(500, 400), (-1920, 0), (1919, 1079), (-7, -3)] {
                let frame = frame(x, y, 240, zoom);
                assert_eq!(frame.window.width(), frame.source.width() * zoom);
                assert_eq!(frame.window.height(), frame.source.height() * zoom);
                assert!(frame.window.width() <= 240 && frame.window.width() > 240 - zoom);
                // The pointer's pixel maps to the block that starts at the pointer.
                let block = frame.window.left + (x - frame.source.left) * zoom;
                assert_eq!(block, x, "zoom {zoom} at {x}");
                let block = frame.window.top + (y - frame.source.top) * zoom;
                assert_eq!(block, y);
                // And that block is the middle of the window, give or take one block.
                let centre = frame.window.left + frame.window.width() / 2;
                assert!((centre - x).abs() <= zoom, "zoom {zoom}: {centre} vs {x}");
            }
        }
    }

    #[test]
    fn windows_are_stacked_bottom_first_and_cut_to_what_the_loupe_shows() {
        let frame = frame(100, 100, 40, 2);
        assert_eq!(frame.source, rect(90, 90, 110, 110));
        let top_first = [
            ("menu", rect(105, 95, 300, 200)),
            ("far", rect(500, 500, 600, 600)),
            ("desktop", rect(-1920, 0, 1920, 1080)),
        ];
        let placed = stack(&frame, &[frame.source], &top_first);
        let order: Vec<_> = placed.iter().map(|p| p.window).collect();
        assert_eq!(order, ["desktop", "menu"], "bottom first, far one left out");
        assert_eq!(placed[0].source, rect(1920 + 90, 90, 1920 + 110, 110));
        assert_eq!(placed[0].destination, rect(0, 0, 40, 40));
        assert_eq!(placed[1].source, rect(0, 0, 5, 15));
        assert_eq!(placed[1].destination, rect(30, 10, 40, 40));
    }

    #[test]
    fn at_a_screen_corner_the_source_reaches_past_it_and_nothing_fills_that_part() {
        // A pointer in the top-left corner of a screen at the origin of the desktop.
        let frame = frame(0, 0, 240, 2);
        let screen = [("desktop", rect(0, 0, 1920, 1080))];
        let placed = stack(&frame, &[frame.source], &screen);
        assert_eq!(placed.len(), 1);
        assert_eq!(placed[0].source, rect(0, 0, 60, 60));
        assert_eq!(placed[0].destination, rect(120, 120, 240, 240));
        let far = [("left", rect(-300, -300, -200, -200))];
        assert!(stack(&frame, &[frame.source], &far).is_empty());
    }

    #[test]
    fn the_bands_fill_the_disc_inside_the_rim_and_never_reach_past_it() {
        for size in [160, 240, 320] {
            for zoom in [2, 3, 4] {
                for rim in [6, 9, 12] {
                    let frame = frame(500, 400, size, zoom);
                    let bands = bands(&frame, rim);
                    assert!(bands.len() <= 50, "{size} {zoom} {rim}: {}", bands.len());
                    let centre = frame.window.width() as f64 / 2.;
                    let (inner, outer) = (centre - rim as f64 / 2., centre + rim as f64 / 2.);
                    let covered = |x: i32, y: i32| {
                        let (sx, sy) = (frame.source.left + x / zoom, frame.source.top + y / zoom);
                        bands.iter().any(|b| {
                            (b.left..b.right).contains(&sx) && (b.top..b.bottom).contains(&sy)
                        })
                    };
                    for y in 0..frame.window.height() {
                        for x in 0..frame.window.width() {
                            let (dx, dy) = (x as f64 + 0.5 - centre, y as f64 + 0.5 - centre);
                            let distance = (dx * dx + dy * dy).sqrt();
                            if distance < inner - 1. {
                                assert!(covered(x, y), "{size} {zoom} {rim}: hole at {x},{y}");
                            }
                            if distance > outer + 1. {
                                assert!(!covered(x, y), "{size} {zoom} {rim}: corner at {x},{y}");
                            }
                        }
                    }
                }
            }
        }
    }
}
