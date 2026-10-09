//! ROLE: Version 1 of the scene text inside a `.shoto` file: frame, region and objects.
//! INVARIANTS: `decode` gives back the scene `encode` wrote, and accepts nothing it does not
//! fully understand: skipping an unknown field could drop a mask at the next save.
use crate::{Attachment, Object, Point, Rect, Shape, Style};
use std::fmt::Write;

/// Largest frame side, far above any desktop.
const MAX_SIDE: u32 = 32_768;
const MAX_OBJECTS: usize = 100_000;
/// Stroke points and widths of the whole scene.
const MAX_POINTS: usize = 4_000_000;
const MAX_TEXT: usize = 1 << 20;

/// What a `.shoto` file holds besides its two images.
#[derive(Clone, Debug, PartialEq)]
pub struct Scene {
    /// Frame size in pixels.
    pub width: u32,
    pub height: u32,
    /// Where the frame was on the desktop at capture, to reopen it in the same place.
    pub origin: (i32, i32),
    /// Selected region, in frame coordinates like the objects.
    pub region: Rect,
    /// Solid colour shown in the region instead of the captured pixels.
    pub background: Option<[u8; 3]>,
    /// In stacking order, which is ascending id order.
    pub objects: Vec<Object>,
    /// Highest id ever given; the editor continues above it.
    pub last_id: u64,
}

impl Scene {
    /// Whether `decode` accepts this scene: what is written can always be read back.
    pub fn valid(&self) -> bool {
        let frame = Rect {
            min: Point::default(),
            max: Point {
                x: self.width as f64,
                y: self.height as f64,
            },
        };
        let size = self.region.size();
        let mut points = 0;
        (1..=MAX_SIDE).contains(&self.width)
            && (1..=MAX_SIDE).contains(&self.height)
            && finite(self.region)
            && size.x > 0.
            && size.y > 0.
            && frame.contains_rect(self.region)
            && self.objects.len() <= MAX_OBJECTS
            && self.objects.windows(2).all(|pair| pair[0].id < pair[1].id)
            && self.objects.last().is_none_or(|o| o.id <= self.last_id)
            && self.objects.iter().all(|o| object_valid(o, &mut points))
    }
    pub(super) fn encode(&self) -> String {
        let mut out = String::new();
        let r = self.region;
        // Writing to a String cannot fail.
        let _ = writeln!(out, "frame {} {}", self.width, self.height);
        let _ = writeln!(out, "origin {} {}", self.origin.0, self.origin.1);
        let _ = writeln!(
            out,
            "region {} {} {} {}",
            r.min.x, r.min.y, r.max.x, r.max.y
        );
        let _ = writeln!(out, "last {}", self.last_id);
        if let Some([r, g, b]) = self.background {
            let _ = writeln!(out, "background {r:02x}{g:02x}{b:02x}");
        }
        for object in &self.objects {
            let _ = encode_object(&mut out, object);
        }
        out
    }
}

fn finite(r: Rect) -> bool {
    r.min.finite() && r.max.finite()
}
fn object_valid(object: &Object, points: &mut usize) -> bool {
    let Object {
        rotation,
        id: _,
        shape,
        style: Style { color: _, width },
        fill: _,
    } = object;
    let measure = |n: &f64| n.is_finite() && *n >= 0.;
    rotation.is_finite()
        && measure(width)
        && match shape {
            Shape::Rectangle(r) | Shape::Ellipse(r) => finite(*r),
            Shape::Arrow {
                start,
                end,
                start_attach,
                end_attach,
                head: _,
            } => {
                start.finite()
                    && end.finite()
                    && [start_attach, end_attach]
                        .iter()
                        .all(|a| a.is_none_or(|a| a.direction.finite()))
            }
            Shape::Stroke {
                points: path,
                widths,
                highlight: _,
            } => {
                *points += path.len() + widths.len();
                !path.is_empty()
                    && *points <= MAX_POINTS
                    && path.iter().all(|p| p.finite())
                    && widths.iter().all(measure)
            }
            Shape::Text {
                position,
                text,
                size,
                extent,
            } => {
                position.finite()
                    && (8. ..=300.).contains(size)
                    && extent.finite()
                    && extent.x >= 0.
                    && extent.y >= 0.
                    && text.len() <= MAX_TEXT
                    && !text.trim().is_empty()
            }
        }
}

fn encode_object(out: &mut String, object: &Object) -> std::fmt::Result {
    // No `..` here: a field added to the model has to be given a place in the format.
    let Object {
        rotation,
        id,
        shape,
        style: Style {
            color: [r, g, b],
            width,
        },
        fill,
    } = object;
    let kind = match shape {
        Shape::Rectangle(_) => "rectangle",
        Shape::Ellipse(_) => "ellipse",
        Shape::Arrow { .. } => "arrow",
        Shape::Stroke { .. } => "stroke",
        Shape::Text { .. } => "text",
    };
    write!(
        out,
        "object id={id} kind={kind} color={r:02x}{g:02x}{b:02x} width={width} rotation={rotation}"
    )?;
    if let Some([r, g, b]) = fill {
        write!(out, " fill={r:02x}{g:02x}{b:02x}")?;
    }
    match shape {
        Shape::Rectangle(r) | Shape::Ellipse(r) => {
            write!(out, " rect={},{},{},{}", r.min.x, r.min.y, r.max.x, r.max.y)?;
        }
        Shape::Arrow {
            start,
            end,
            start_attach,
            end_attach,
            head,
        } => {
            write!(
                out,
                " from={},{} to={},{} head={}",
                start.x, start.y, end.x, end.y, *head as u8
            )?;
            for (key, attach) in [("start", start_attach), ("end", end_attach)] {
                if let Some(Attachment { target, direction }) = attach {
                    write!(out, " {key}={target},{},{}", direction.x, direction.y)?;
                }
            }
        }
        Shape::Stroke {
            points,
            widths,
            highlight,
        } => {
            write!(out, " highlight={} points=", *highlight as u8)?;
            for (i, p) in points.iter().enumerate() {
                write!(out, "{}{},{}", if i == 0 { "" } else { ";" }, p.x, p.y)?;
            }
            for (i, w) in widths.iter().enumerate() {
                write!(out, "{}{w}", if i == 0 { " widths=" } else { ";" })?;
            }
        }
        Shape::Text {
            position,
            text,
            size,
            extent,
        } => {
            write!(
                out,
                " at={},{} size={size} extent={},{} text=",
                position.x, position.y, extent.x, extent.y
            )?;
            for byte in text.bytes() {
                write!(out, "{byte:02x}")?;
            }
        }
    }
    out.push('\n');
    Ok(())
}

pub(super) fn decode(text: &str) -> Option<Scene> {
    let mut lines = text.strip_suffix('\n')?.split('\n').peekable();
    let [width, height] = record(lines.next()?, "frame")?;
    let [x, y] = record(lines.next()?, "origin")?;
    let [x0, y0, x1, y1] = record(lines.next()?, "region")?;
    let [last_id] = record(lines.next()?, "last")?;
    let mut scene = Scene {
        width: width.parse().ok()?,
        height: height.parse().ok()?,
        origin: (x.parse().ok()?, y.parse().ok()?),
        region: Rect {
            min: pair(number(x0)?, number(y0)?),
            max: pair(number(x1)?, number(y1)?),
        },
        background: None,
        objects: Vec::new(),
        last_id: last_id.parse().ok()?,
    };
    if let Some(color) = lines
        .peek()
        .and_then(|line| line.strip_prefix("background "))
    {
        scene.background = Some(hex(color)?.try_into().ok()?);
        lines.next();
    }
    let mut points = 0;
    for line in lines {
        if scene.objects.len() == MAX_OBJECTS {
            return None;
        }
        scene.objects.push(decode_object(line, &mut points)?);
    }
    scene.valid().then_some(scene)
}

/// The `N` fields of the line `name field field…`.
fn record<'a, const N: usize>(line: &'a str, name: &str) -> Option<[&'a str; N]> {
    let mut fields = line.strip_prefix(name)?.strip_prefix(' ')?.split(' ');
    let mut out = [""; N];
    for field in &mut out {
        *field = fields.next()?;
    }
    fields.next().is_none().then_some(out)
}
fn number(text: &str) -> Option<f64> {
    text.parse().ok().filter(|n: &f64| n.is_finite())
}
fn pair(x: f64, y: f64) -> Point {
    Point { x, y }
}
/// `N` numbers separated by commas.
fn numbers<const N: usize>(text: &str) -> Option<[f64; N]> {
    let mut fields = text.split(',');
    let mut out = [0.; N];
    for field in &mut out {
        *field = number(fields.next()?)?;
    }
    fields.next().is_none().then_some(out)
}
fn point(text: &str) -> Option<Point> {
    numbers(text).map(|[x, y]| pair(x, y))
}
fn flag(text: &str) -> Option<bool> {
    match text {
        "0" => Some(false),
        "1" => Some(true),
        _ => None,
    }
}
fn attachment(text: &str) -> Option<Attachment> {
    let (target, direction) = text.split_once(',')?;
    Some(Attachment {
        target: target.parse().ok()?,
        direction: point(direction)?,
    })
}
/// Splits a list whose entry count is checked against the scene budget before any allocation.
fn list<'a>(text: &'a str, points: &mut usize) -> Option<std::str::Split<'a, char>> {
    *points += text.bytes().filter(|b| *b == b';').count() + 1;
    (*points <= MAX_POINTS).then(|| text.split(';'))
}
/// Bytes written as pairs of hexadecimal digits.
fn hex(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) || !text.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).ok())
        .collect()
}

/// The `key=value` fields of an object line; each is taken once and none may be left.
struct Fields<'a>(Vec<(&'a str, &'a str)>);
impl<'a> Fields<'a> {
    fn take(&mut self, key: &str) -> Option<&'a str> {
        let at = self.0.iter().position(|(k, _)| *k == key)?;
        Some(self.0.swap_remove(at).1)
    }
}

fn decode_object(line: &str, points: &mut usize) -> Option<Object> {
    let fields = line.strip_prefix("object ")?.split(' ');
    let mut f = Fields(fields.map(|f| f.split_once('=')).collect::<Option<_>>()?);
    let id = f.take("id")?.parse().ok()?;
    let style = Style {
        color: hex(f.take("color")?)?.try_into().ok()?,
        width: number(f.take("width")?)?,
    };
    let rotation = number(f.take("rotation")?)?;
    let fill = match f.take("fill") {
        Some(color) => Some(hex(color)?.try_into().ok()?),
        None => None,
    };
    let rect = |text| {
        numbers(text).map(|[x0, y0, x1, y1]| Rect {
            min: pair(x0, y0),
            max: pair(x1, y1),
        })
    };
    let shape = match f.take("kind")? {
        "rectangle" => Shape::Rectangle(rect(f.take("rect")?)?),
        "ellipse" => Shape::Ellipse(rect(f.take("rect")?)?),
        "arrow" => Shape::Arrow {
            start: point(f.take("from")?)?,
            end: point(f.take("to")?)?,
            head: flag(f.take("head")?)?,
            start_attach: match f.take("start") {
                Some(text) => Some(attachment(text)?),
                None => None,
            },
            end_attach: match f.take("end") {
                Some(text) => Some(attachment(text)?),
                None => None,
            },
        },
        "stroke" => Shape::Stroke {
            highlight: flag(f.take("highlight")?)?,
            points: list(f.take("points")?, points)?
                .map(point)
                .collect::<Option<_>>()?,
            widths: match f.take("widths") {
                Some(text) => list(text, points)?.map(number).collect::<Option<_>>()?,
                None => Vec::new(),
            },
        },
        "text" => {
            let [x, y] = numbers(f.take("at")?)?;
            let [w, h] = numbers(f.take("extent")?)?;
            Shape::Text {
                position: pair(x, y),
                size: number(f.take("size")?)?,
                extent: pair(w, h),
                text: String::from_utf8(hex(f
                    .take("text")
                    .filter(|t| t.len() <= 2 * MAX_TEXT)?)?)
                .ok()?,
            }
        }
        _ => return None,
    };
    f.0.is_empty().then_some(Object {
        rotation,
        id,
        shape,
        style,
        fill,
    })
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;

    fn object(id: u64, rotation: f64, color: [u8; 3], width: f64, shape: Shape) -> Object {
        Object {
            rotation,
            id,
            shape,
            style: Style { color, width },
            fill: None,
        }
    }
    fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Rect {
        Rect {
            min: pair(x0, y0),
            max: pair(x1, y1),
        }
    }
    /// One object of every kind, with every optional field present somewhere.
    pub(in crate::shoto) fn sample() -> Scene {
        let attach = |target, x, y| {
            Some(Attachment {
                target,
                direction: pair(x, y),
            })
        };
        let arrow = |start, end, start_attach, end_attach, head| Shape::Arrow {
            start,
            end,
            start_attach,
            end_attach,
            head,
        };
        let (red, white, blue) = ([237, 79, 92], [255; 3], [16, 32, 48]);
        let from = pair(1., 2.);
        let to = pair(300., 400.125);
        let filled = Object {
            fill: Some(blue),
            ..object(2, 0.5, [0; 3], 1., Shape::Ellipse(rect(-5.5, 0., 40., 30.)))
        };
        Scene {
            width: 1920,
            height: 1080,
            origin: (-1920, 0),
            region: rect(100.5, 200., 900., 700.25),
            background: Some([28, 31, 40]),
            objects: vec![
                object(1, 0., red, 3., Shape::Rectangle(rect(10., 20., 110., 220.))),
                filled,
                object(
                    4,
                    0.,
                    white,
                    12.,
                    arrow(from, to, attach(1, 0.5, -1.), None, true),
                ),
                object(
                    5,
                    0.,
                    white,
                    12.,
                    arrow(pair(0., 0.), pair(10., 0.), None, attach(2, 0., 1.), false),
                ),
                object(
                    7,
                    0.,
                    blue,
                    2.,
                    Shape::Stroke {
                        points: vec![pair(0., 0.), pair(1.5, 2.), pair(3., 4.)],
                        widths: vec![2., 2.5],
                        highlight: false,
                    },
                ),
                object(
                    8,
                    0.,
                    blue,
                    2.,
                    Shape::Stroke {
                        points: vec![pair(7., 7.)],
                        widths: Vec::new(),
                        highlight: true,
                    },
                ),
                object(
                    9,
                    1.25,
                    red,
                    3.,
                    Shape::Text {
                        position: pair(50., 60.),
                        text: "Héllo\r\nwörld".into(),
                        size: 24.,
                        extent: pair(120.5, 64.),
                    },
                ),
            ],
            last_id: 9,
        }
    }
    /// Version 1 as it was first written. Files with this text exist: it never changes.
    const VERSION_1: &str = "\
frame 1920 1080
origin -1920 0
region 100.5 200 900 700.25
last 9
background 1c1f28
object id=1 kind=rectangle color=ed4f5c width=3 rotation=0 rect=10,20,110,220
object id=2 kind=ellipse color=000000 width=1 rotation=0.5 fill=102030 rect=-5.5,0,40,30
object id=4 kind=arrow color=ffffff width=12 rotation=0 from=1,2 to=300,400.125 head=1 start=1,0.5,-1
object id=5 kind=arrow color=ffffff width=12 rotation=0 from=0,0 to=10,0 head=0 end=2,0,1
object id=7 kind=stroke color=102030 width=2 rotation=0 highlight=0 points=0,0;1.5,2;3,4 widths=2;2.5
object id=8 kind=stroke color=102030 width=2 rotation=0 highlight=1 points=7,7
object id=9 kind=text color=ed4f5c width=3 rotation=1.25 at=50,60 size=24 extent=120.5,64 text=48c3a96c6c6f0d0a77c3b6726c64
";

    #[test]
    fn version_1_text_is_frozen() {
        assert_eq!(sample().encode(), VERSION_1);
        assert_eq!(decode(VERSION_1), Some(sample()));
    }
    #[test]
    fn awkward_numbers_come_back_exactly() {
        let mut scene = sample();
        let values = [0.1 + 0.2, 1e-7, 1e21, -0.0, f64::MIN_POSITIVE, 1. / 3.];
        scene.objects[4].shape = Shape::Stroke {
            points: values.iter().map(|v| pair(*v, -*v)).collect(),
            widths: values.iter().map(|v| v.abs()).collect(),
            highlight: false,
        };
        let back = decode(&scene.encode()).unwrap();
        assert_eq!(back, scene);
        let Shape::Stroke { points, .. } = &back.objects[4].shape else {
            panic!("stroke expected");
        };
        for (p, v) in points.iter().zip(values) {
            assert_eq!(p.x.to_bits(), v.to_bits());
        }
    }
    #[test]
    fn an_empty_scene_is_a_scene() {
        let scene = Scene {
            objects: Vec::new(),
            last_id: 0,
            background: None,
            ..sample()
        };
        assert_eq!(decode(&scene.encode()), Some(scene));
    }
    #[test]
    fn anything_not_fully_understood_is_refused() {
        let edits: &[(&str, &str)] = &[
            // Structure.
            ("frame 1920 1080\n", ""),
            ("frame 1920 1080\n", "frame 1920 1080 1\n"),
            ("frame 1920 1080\n", "frame 0 1080\n"),
            ("frame 1920 1080\n", "frame 40000 1080\n"),
            ("origin -1920 0", "origin -1920"),
            ("origin -1920 0", "origin -1920.5 0"),
            ("last 9\n", "last 9\nnote hello\n"),
            ("background 1c1f28", "background 1c1f2"),
            (
                "background 1c1f28\n",
                "background 1c1f28\nbackground 1c1f28\n",
            ),
            ("last 9\nbackground 1c1f28\n", "background 1c1f28\nlast 9\n"),
            ("last 9\n", "last 9\n\n"),
            ("last 9\n", "last 9\r\n"),
            ("rotation=0.5 ", "rotation=0.5  "),
            // Region.
            ("region 100.5 200", "region 2000 200"),
            ("region 100.5 200 900", "region 100.5 200 100.5"),
            ("region 100.5", "region NaN"),
            // Identifiers.
            ("last 9", "last 8"),
            ("object id=2 ", "object id=1 "),
            ("object id=2 ", "object id=4 "),
            ("object id=2 ", "object "),
            // Fields.
            ("kind=ellipse", "kind=star"),
            ("kind=ellipse", "kind=ellipse glow=ff0000"),
            ("fill=102030", "fill=1020"),
            ("fill=102030", "fill=102030 fill=102030"),
            ("kind=ellipse", "kind=ellipse kind=ellipse"),
            ("color=000000", "color=00000"),
            ("color=000000", "color=00000g"),
            ("color=000000", "color=+00000"),
            ("width=1 ", "width=-1 "),
            ("width=1 ", "width=inf "),
            ("rotation=0.5", "rotation=nan"),
            ("rect=-5.5,0,40,30", "rect=-5.5,0,40"),
            ("rect=-5.5,0,40,30", "rect=-5.5,0,40,30,1"),
            ("head=1", "head=2"),
            ("start=1,0.5,-1", "start=1,0.5"),
            ("start=1,0.5,-1", "start=-1,0.5,-1"),
            ("highlight=1 points=7,7", "highlight=1 points="),
            ("highlight=1 points=7,7", "highlight=1 points=7"),
            ("highlight=1 points=7,7", "highlight=1 points=7,7;"),
            ("widths=2;2.5", "widths=2;-2.5"),
            ("size=24", "size=7"),
            ("size=24", "size=301"),
            ("extent=120.5,64", "extent=-1,64"),
            ("text=48c3", "text=8c3"),
            ("text=48c3", "text=48ff"),
            ("text=48c3a96c6c6f0d0a77c3b6726c64", "text="),
            ("text=48c3a96c6c6f0d0a77c3b6726c64", "text=200a"),
        ];
        for (from, to) in edits {
            assert!(VERSION_1.contains(from), "{from:?} is not in the reference");
            let text = VERSION_1.replacen(from, to, 1);
            assert_eq!(decode(&text), None, "{from:?} replaced by {to:?}");
        }
        assert_eq!(decode(VERSION_1.trim_end()), None);
        assert_eq!(decode(""), None);
    }
    #[test]
    fn sizes_are_refused_before_they_are_allocated() {
        let head = "frame 10 10\norigin 0 0\nregion 0 0 10 10\n";
        let ellipse = "kind=ellipse color=000000 width=1 rotation=0 rect=0,0,1,1";
        let crowd: String = (1..=MAX_OBJECTS + 1)
            .map(|id| format!("object id={id} {ellipse}\n"))
            .collect();
        let last = MAX_OBJECTS + 1;
        assert_eq!(decode(&format!("{head}last {last}\n{crowd}")), None);
        let path = "0,0;".repeat(MAX_POINTS) + "0,0";
        let stroke = "object id=1 kind=stroke color=000000 width=1 rotation=0 highlight=0";
        assert_eq!(
            decode(&format!("{head}last 1\n{stroke} points={path}\n")),
            None
        );
        let long = "61".repeat(MAX_TEXT + 1);
        let text = "object id=1 kind=text color=000000 width=1 rotation=0 at=0,0 size=24";
        assert_eq!(
            decode(&format!("{head}last 1\n{text} extent=1,1 text={long}\n")),
            None
        );
    }
    #[test]
    fn an_invalid_scene_is_recognised_before_it_is_written() {
        assert!(sample().valid());
        let broken: [fn(&mut Scene); 6] = [
            |s| s.width = 0,
            |s| s.region.max.x = 5000.,
            |s| s.last_id = 3,
            |s| s.objects.swap(0, 1),
            |s| s.objects[0].style.width = f64::NAN,
            |s| s.objects[1].rotation = f64::INFINITY,
        ];
        for change in broken {
            let mut scene = sample();
            change(&mut scene);
            assert!(!scene.valid());
        }
    }
}
