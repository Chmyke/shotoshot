//! ROLE: The scene text inside a `.shoto` file: frame, region and objects. Version 2 adds
//! stacked coats and the shapes each spares (`Paint`); version 1 has one plain coat at most.
//! Version 3 adds the panel of a text (`Label`).
//! INVARIANTS: `decode` gives back the scene `encode` wrote, and accepts nothing it does not
//! fully understand: skipping an unknown field could drop a mask at the next save.
use crate::{Attachment, Label, Object, Paint, Point, Rect, Shape, Style, bent};
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
    /// Coats laid over the captured pixels of the region, bottom first.
    pub background: Vec<Paint>,
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
            && self.coats().all(|coat| {
                coat.walls
                    .iter()
                    .all(|w| self.objects.binary_search_by_key(w, |o| o.id).is_ok())
            })
    }
    /// The lowest format version that holds this scene: 2 once a zone has more than one coat
    /// or a coat spares a shape, 3 once a text sits on a panel, 4 once an arrow or a line
    /// bends, 5 once a panel's text has an ink of its own, so simpler files still open in an
    /// earlier Shotoshot.
    pub fn version(&self) -> u32 {
        let plain = |coats: &[Paint]| coats.len() <= 1 && coats.iter().all(|c| c.walls.is_empty());
        let labelled = |o: &Object| matches!(o.shape, Shape::Text { label: Some(_), .. });
        let inked = |o: &Object| matches!(o.shape, Shape::Text { ink: Some(_), .. });
        if self.objects.iter().any(inked) {
            5
        } else if self.objects.iter().any(|o| bent(o.bend())) {
            4
        } else if self.objects.iter().any(labelled) {
            3
        } else if plain(&self.background) && self.objects.iter().all(|o| plain(&o.fill)) {
            1
        } else {
            2
        }
    }
    fn coats(&self) -> impl Iterator<Item = &Paint> {
        self.background
            .iter()
            .chain(self.objects.iter().flat_map(|o| &o.fill))
    }
    /// The text of this scene, as written inside a `.shoto` file under `version()`.
    pub fn to_text(&self) -> String {
        self.encode()
    }
    /// A scene read back from `to_text` written under `version`; `None` for anything not
    /// fully understood.
    pub fn from_text(text: &str, version: u32) -> Option<Self> {
        decode(text, version)
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
        if !self.background.is_empty() {
            out.push_str("background ");
            let _ = write_coats(&mut out, &self.background);
            out.push('\n');
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
                bend,
            } => {
                start.finite()
                    && end.finite()
                    && bend.finite()
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
                label,
                ink,
            } => {
                (label.is_some() || ink.is_none())
                    && label
                        .and_then(Label::wrap)
                        .is_none_or(|w| w.is_finite() && w > 0.)
                    && position.finite()
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
    if !fill.is_empty() {
        out.push_str(" fill=");
        write_coats(out, fill)?;
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
            bend,
        } => {
            write!(
                out,
                " from={},{} to={},{} head={}",
                start.x, start.y, end.x, end.y, *head as u8
            )?;
            if bent(*bend) {
                write!(out, " bend={},{}", bend.x, bend.y)?;
            }
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
            label,
            ink,
        } => {
            write!(
                out,
                " at={},{} size={size} extent={},{}",
                position.x, position.y, extent.x, extent.y
            )?;
            match label {
                Some(Label::Fit) => out.push_str(" label=fit"),
                Some(Label::Wrap(width)) => write!(out, " label=wrap:{width}")?,
                None => {}
            }
            if let Some([r, g, b]) = ink {
                write!(out, " ink={r:02x}{g:02x}{b:02x}")?;
            }
            out.push_str(" text=");
            for byte in text.bytes() {
                write!(out, "{byte:02x}")?;
            }
        }
    }
    out.push('\n');
    Ok(())
}
/// `rrggbb` per coat, bottom first, separated by `;`; a coat that spares shapes adds `/`
/// and their ids separated by `,`.
fn write_coats(out: &mut String, coats: &[Paint]) -> std::fmt::Result {
    for (i, Paint { color, walls }) in coats.iter().enumerate() {
        let [r, g, b] = color;
        write!(
            out,
            "{}{r:02x}{g:02x}{b:02x}",
            if i == 0 { "" } else { ";" }
        )?;
        for (j, wall) in walls.iter().enumerate() {
            write!(out, "{}{wall}", if j == 0 { "/" } else { "," })?;
        }
    }
    Ok(())
}

pub(super) fn decode(text: &str, version: u32) -> Option<Scene> {
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
        background: Vec::new(),
        objects: Vec::new(),
        last_id: last_id.parse().ok()?,
    };
    let mut points = 0;
    if let Some(text) = lines
        .peek()
        .and_then(|line| line.strip_prefix("background "))
    {
        scene.background = coats(text, version, &mut points)?;
        lines.next();
    }
    for line in lines {
        if scene.objects.len() == MAX_OBJECTS {
            return None;
        }
        scene
            .objects
            .push(decode_object(line, version, &mut points)?);
    }
    scene.valid().then_some(scene)
}
/// What `write_coats` wrote; version 1 knows a single coat that spares nothing.
fn coats(text: &str, version: u32, points: &mut usize) -> Option<Vec<Paint>> {
    let coats: Vec<Paint> = list(text, ';', points)?
        .map(|coat| {
            let (color, walls) = match coat.split_once('/') {
                Some((color, walls)) => (
                    color,
                    list(walls, ',', points)?
                        .map(|id| id.parse().ok())
                        .collect::<Option<_>>()?,
                ),
                None => (coat, Vec::new()),
            };
            Some(Paint {
                color: hex(color)?.try_into().ok()?,
                walls,
            })
        })
        .collect::<Option<_>>()?;
    let plain = coats.len() == 1 && coats[0].walls.is_empty();
    (version >= 2 || plain).then_some(coats)
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
fn list<'a>(
    text: &'a str,
    separator: char,
    points: &mut usize,
) -> Option<std::str::Split<'a, char>> {
    *points += text.chars().filter(|c| *c == separator).count() + 1;
    (*points <= MAX_POINTS).then(|| text.split(separator))
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

fn decode_object(line: &str, version: u32, points: &mut usize) -> Option<Object> {
    let fields = line.strip_prefix("object ")?.split(' ');
    let mut f = Fields(fields.map(|f| f.split_once('=')).collect::<Option<_>>()?);
    let id = f.take("id")?.parse().ok()?;
    let style = Style {
        color: hex(f.take("color")?)?.try_into().ok()?,
        width: number(f.take("width")?)?,
    };
    let rotation = number(f.take("rotation")?)?;
    let fill = match f.take("fill") {
        Some(text) => coats(text, version, points)?,
        None => Vec::new(),
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
            bend: match f.take("bend") {
                Some(_) if version < 4 => return None,
                // A straight line is written without the field, never as a zero bend.
                Some(text) => point(text).filter(|b| bent(*b))?,
                None => Point::default(),
            },
        },
        "stroke" => Shape::Stroke {
            highlight: flag(f.take("highlight")?)?,
            points: list(f.take("points")?, ';', points)?
                .map(point)
                .collect::<Option<_>>()?,
            widths: match f.take("widths") {
                Some(text) => list(text, ';', points)?
                    .map(number)
                    .collect::<Option<_>>()?,
                None => Vec::new(),
            },
        },
        "text" => {
            let [x, y] = numbers(f.take("at")?)?;
            let [w, h] = numbers(f.take("extent")?)?;
            let label = match f.take("label") {
                Some(_) if version < 3 => return None,
                Some("fit") => Some(Label::Fit),
                Some(text) => Some(Label::Wrap(number(text.strip_prefix("wrap:")?)?)),
                None => None,
            };
            let ink = match f.take("ink") {
                // Only a panel's text has an ink of its own.
                Some(_) if version < 5 || label.is_none() => return None,
                Some(text) => Some(hex(text)?.try_into().ok()?),
                None => None,
            };
            Shape::Text {
                position: pair(x, y),
                size: number(f.take("size")?)?,
                extent: pair(w, h),
                label,
                ink,
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
            fill: Vec::new(),
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
            bend: Point::default(),
        };
        let (red, white, blue) = ([237, 79, 92], [255; 3], [16, 32, 48]);
        let from = pair(1., 2.);
        let to = pair(300., 400.125);
        let filled = Object {
            fill: vec![Paint::solid(blue)],
            ..object(2, 0.5, [0; 3], 1., Shape::Ellipse(rect(-5.5, 0., 40., 30.)))
        };
        Scene {
            width: 1920,
            height: 1080,
            origin: (-1920, 0),
            region: rect(100.5, 200., 900., 700.25),
            background: vec![Paint::solid([28, 31, 40])],
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
                        label: None,
                        ink: None,
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
        assert_eq!(sample().version(), 1);
        assert_eq!(sample().encode(), VERSION_1);
        assert_eq!(decode(VERSION_1, 1), Some(sample()));
        assert_eq!(
            decode(VERSION_1, 2),
            Some(sample()),
            "version 2 reads it unchanged"
        );
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
        let back = decode(&scene.encode(), 1).unwrap();
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
            background: Vec::new(),
            ..sample()
        };
        assert_eq!(decode(&scene.encode(), 1), Some(scene));
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
            assert_eq!(decode(&text, 1), None, "{from:?} replaced by {to:?}");
        }
        assert_eq!(decode(VERSION_1.trim_end(), 1), None);
        assert_eq!(decode("", 1), None);
    }
    #[test]
    fn sizes_are_refused_before_they_are_allocated() {
        let head = "frame 10 10\norigin 0 0\nregion 0 0 10 10\n";
        let ellipse = "kind=ellipse color=000000 width=1 rotation=0 rect=0,0,1,1";
        let crowd: String = (1..=MAX_OBJECTS + 1)
            .map(|id| format!("object id={id} {ellipse}\n"))
            .collect();
        let last = MAX_OBJECTS + 1;
        assert_eq!(decode(&format!("{head}last {last}\n{crowd}"), 1), None);
        let path = "0,0;".repeat(MAX_POINTS) + "0,0";
        let stroke = "object id=1 kind=stroke color=000000 width=1 rotation=0 highlight=0";
        assert_eq!(
            decode(&format!("{head}last 1\n{stroke} points={path}\n"), 1),
            None
        );
        let long = "61".repeat(MAX_TEXT + 1);
        let text = "object id=1 kind=text color=000000 width=1 rotation=0 at=0,0 size=24";
        assert_eq!(
            decode(&format!("{head}last 1\n{text} extent=1,1 text={long}\n"), 1),
            None
        );
        let walls = (1..=MAX_POINTS + 1)
            .map(|_| "1")
            .collect::<Vec<_>>()
            .join(",");
        let filled = "object id=1 kind=ellipse color=000000 width=1 rotation=0 rect=0,0,1,1";
        assert_eq!(
            decode(&format!("{head}last 1\n{filled} fill=000000/{walls}\n"), 2),
            None,
            "walls count against the same budget"
        );
    }
    /// Version 2 as it was first written: coats stacked on the capture and in a shape, each
    /// sparing the closed shapes above its zone. Files with this text exist: it never changes.
    const VERSION_2: &str = "\
frame 1920 1080
origin -1920 0
region 100.5 200 900 700.25
last 3
background 1c1f28;ffffff/1,3
object id=1 kind=rectangle color=ed4f5c width=3 rotation=0 fill=102030/3;00ff00 rect=10,20,110,220
object id=3 kind=ellipse color=000000 width=1 rotation=0.5 rect=-5.5,0,40,30
";
    fn layered() -> Scene {
        let coat = |color, walls: &[u64]| Paint {
            color,
            walls: walls.to_vec(),
        };
        Scene {
            background: vec![coat([28, 31, 40], &[]), coat([255; 3], &[1, 3])],
            objects: vec![
                Object {
                    fill: vec![coat([16, 32, 48], &[3]), coat([0, 255, 0], &[])],
                    ..object(
                        1,
                        0.,
                        [237, 79, 92],
                        3.,
                        Shape::Rectangle(rect(10., 20., 110., 220.)),
                    )
                },
                object(3, 0.5, [0; 3], 1., Shape::Ellipse(rect(-5.5, 0., 40., 30.))),
            ],
            last_id: 3,
            ..sample()
        }
    }
    #[test]
    fn version_2_text_is_frozen() {
        assert_eq!(layered().version(), 2);
        assert_eq!(layered().encode(), VERSION_2);
        assert_eq!(decode(VERSION_2, 2), Some(layered()));
    }
    #[test]
    fn version_1_knows_one_plain_coat_only() {
        assert_eq!(decode(VERSION_2, 1), None);
        let one_wall = VERSION_1.replacen("fill=102030", "fill=102030/1", 1);
        assert_eq!(decode(&one_wall, 1), None);
        assert!(decode(&one_wall, 2).is_some());
        let two_coats = VERSION_1.replacen("background 1c1f28", "background 1c1f28;1c1f28", 1);
        assert_eq!(decode(&two_coats, 1), None);
        assert!(decode(&two_coats, 2).is_some());
    }
    #[test]
    fn coats_not_fully_understood_are_refused() {
        for (from, to) in [
            ("ffffff/1,3", "ffffff/1,4"),
            ("ffffff/1,3", "ffffff/"),
            ("ffffff/1,3", "ffffff/1,,3"),
            ("ffffff/1,3", "ffffff/1;3"),
            ("ffffff/1,3", "ffffff/-1"),
            ("ffffff/1,3", "ffffff/x"),
            ("ffffff/1,3", "ffffff/1/3"),
            ("1c1f28;", "1c1f28;;"),
            ("1c1f28;", ";"),
            ("fill=102030/3;00ff00", "fill=102030/3;"),
            ("fill=102030/3;00ff00", "fill="),
        ] {
            assert!(VERSION_2.contains(from), "{from:?} is not in the reference");
            let text = VERSION_2.replacen(from, to, 1);
            assert_eq!(decode(&text, 2), None, "{from:?} replaced by {to:?}");
        }
    }
    /// Version 3 as it was first written: a text fitted to its panel and one wrapped at a
    /// width. Files with this text exist: it never changes.
    const VERSION_3: &str = "\
frame 1920 1080
origin -1920 0
region 100.5 200 900 700.25
last 3
object id=1 kind=text color=fac042 width=3 rotation=0 at=50,60 size=24 extent=120.5,29 label=fit text=4e6f7465
object id=3 kind=text color=1c1f28 width=3 rotation=0.5 at=10,20 size=32 extent=180,77 label=wrap:200.25 text=c3876120646f69742072657374657220696369
";
    fn labelled() -> Scene {
        let text = |text: &str, position, size, extent, label| Shape::Text {
            position,
            text: text.into(),
            size,
            extent,
            label: Some(label),
            ink: None,
        };
        Scene {
            background: Vec::new(),
            objects: vec![
                object(
                    1,
                    0.,
                    [250, 192, 66],
                    3.,
                    text("Note", pair(50., 60.), 24., pair(120.5, 29.), Label::Fit),
                ),
                object(
                    3,
                    0.5,
                    [28, 31, 40],
                    3.,
                    text(
                        "Ça doit rester ici",
                        pair(10., 20.),
                        32.,
                        pair(180., 77.),
                        Label::Wrap(200.25),
                    ),
                ),
            ],
            last_id: 3,
            ..sample()
        }
    }
    #[test]
    fn version_3_text_is_frozen() {
        assert_eq!(labelled().version(), 3);
        assert_eq!(labelled().encode(), VERSION_3);
        assert_eq!(decode(VERSION_3, 3), Some(labelled()));
        assert_eq!(
            decode(VERSION_1, 3),
            Some(sample()),
            "version 3 reads version 1"
        );
        assert_eq!(decode(VERSION_2, 3), Some(layered()), "and version 2");
    }
    #[test]
    fn earlier_versions_know_no_label() {
        assert_eq!(decode(VERSION_3, 2), None);
        assert_eq!(decode(VERSION_3, 1), None);
        let plain = VERSION_3
            .replacen(" label=fit", "", 1)
            .replacen(" label=wrap:200.25", "", 1);
        assert!(decode(&plain, 2).is_some(), "the same texts without panels");
    }
    #[test]
    fn labels_not_fully_understood_are_refused() {
        for (from, to) in [
            ("label=fit", "label=fits"),
            ("label=fit", "label="),
            ("label=fit", "label=fit label=fit"),
            ("label=wrap:200.25", "label=wrap:"),
            ("label=wrap:200.25", "label=wrap:-1"),
            ("label=wrap:200.25", "label=wrap:0"),
            ("label=wrap:200.25", "label=wrap:inf"),
            ("label=wrap:200.25", "label=200.25"),
        ] {
            assert!(VERSION_3.contains(from), "{from:?} is not in the reference");
            let text = VERSION_3.replacen(from, to, 1);
            assert_eq!(decode(&text, 3), None, "{from:?} replaced by {to:?}");
        }
    }
    /// Version 4 as it was first written: a bent arrow and a bent line. Files with this
    /// text exist: it never changes.
    const VERSION_4: &str = "\
frame 1920 1080
origin -1920 0
region 100.5 200 900 700.25
last 2
object id=1 kind=arrow color=ed4f5c width=3 rotation=0 from=10,20 to=210,20 head=1 bend=0.125,-0.3
object id=2 kind=arrow color=1c1f28 width=5 rotation=0 from=40,90 to=40,300.5 head=0 bend=0,0.25
";
    fn bent() -> Scene {
        let arrow = |start, end, head, bend| Shape::Arrow {
            start,
            end,
            start_attach: None,
            end_attach: None,
            head,
            bend,
        };
        Scene {
            background: Vec::new(),
            objects: vec![
                object(
                    1,
                    0.,
                    [237, 79, 92],
                    3.,
                    arrow(pair(10., 20.), pair(210., 20.), true, pair(0.125, -0.3)),
                ),
                object(
                    2,
                    0.,
                    [28, 31, 40],
                    5.,
                    arrow(pair(40., 90.), pair(40., 300.5), false, pair(0., 0.25)),
                ),
            ],
            last_id: 2,
            ..sample()
        }
    }
    #[test]
    fn version_4_text_is_frozen() {
        assert_eq!(bent().version(), 4);
        assert_eq!(bent().encode(), VERSION_4);
        assert_eq!(decode(VERSION_4, 4), Some(bent()));
        assert_eq!(decode(VERSION_1, 4), Some(sample()), "version 4 reads 1");
        assert_eq!(decode(VERSION_2, 4), Some(layered()), "and 2");
        assert_eq!(decode(VERSION_3, 4), Some(labelled()), "and 3");
        let mut labelled_and_bent = labelled();
        labelled_and_bent
            .objects
            .extend(bent().objects.into_iter().map(|mut o| {
                o.id += 3;
                o
            }));
        labelled_and_bent.last_id = 5;
        assert_eq!(labelled_and_bent.version(), 4, "a bend outranks a label");
    }
    #[test]
    fn earlier_versions_know_no_bend() {
        for version in 1..4 {
            assert_eq!(decode(VERSION_4, version), None, "version {version}");
        }
        let straight =
            VERSION_4
                .replacen(" bend=0.125,-0.3", "", 1)
                .replacen(" bend=0,0.25", "", 1);
        assert!(decode(&straight, 1).is_some(), "the same arrows straight");
    }
    #[test]
    fn bends_not_fully_understood_are_refused() {
        for (from, to) in [
            ("bend=0,0.25", "bend=0,0"),
            ("bend=0,0.25", "bend=0.25"),
            ("bend=0,0.25", "bend="),
            ("bend=0,0.25", "bend=0,inf"),
            ("bend=0,0.25", "bend=0,0.25 bend=0,0.25"),
            ("bend=0,0.25", "bend=0,0.25,1"),
        ] {
            assert!(VERSION_4.contains(from), "{from:?} is not in the reference");
            let text = VERSION_4.replacen(from, to, 1);
            assert_eq!(decode(&text, 4), None, "{from:?} replaced by {to:?}");
        }
    }
    /// Version 5 as it was first written: a panel's text in an ink of its own, beside one
    /// that keeps the automatic black or white. Files with this text exist: it never changes.
    const VERSION_5: &str = "frame 1920 1080
origin -1920 0
region 100.5 200 900 700.25
last 3
object id=1 kind=text color=fac042 width=3 rotation=0 at=50,60 size=24 extent=120.5,29 label=fit ink=ed4f5c text=4e6f7465
object id=3 kind=text color=1c1f28 width=3 rotation=0.5 at=10,20 size=32 extent=180,77 label=wrap:200.25 text=c3876120646f69742072657374657220696369
";
    fn inked() -> Scene {
        let mut scene = labelled();
        if let Shape::Text { ink, .. } = &mut scene.objects[0].shape {
            *ink = Some([237, 79, 92]);
        }
        scene
    }
    #[test]
    fn version_5_text_is_frozen() {
        assert_eq!(inked().version(), 5);
        assert_eq!(inked().encode(), VERSION_5);
        assert_eq!(decode(VERSION_5, 5), Some(inked()));
        assert_eq!(decode(VERSION_1, 5), Some(sample()), "version 5 reads 1");
        assert_eq!(decode(VERSION_2, 5), Some(layered()), "and 2");
        assert_eq!(
            decode(VERSION_3, 5),
            Some(labelled()),
            "and 3, automatic ink"
        );
        assert_eq!(decode(VERSION_4, 5), Some(bent()), "and 4");
        let mut inked_and_bent = inked();
        inked_and_bent
            .objects
            .extend(bent().objects.into_iter().map(|mut o| {
                o.id += 3;
                o
            }));
        inked_and_bent.last_id = 5;
        assert_eq!(inked_and_bent.version(), 5, "an ink outranks a bend");
    }
    #[test]
    fn earlier_versions_know_no_ink() {
        for version in 1..5 {
            assert_eq!(decode(VERSION_5, version), None, "version {version}");
        }
        let automatic = VERSION_5.replacen(" ink=ed4f5c", "", 1);
        assert_eq!(decode(&automatic, 3), Some(labelled()));
    }
    #[test]
    fn inks_not_fully_understood_are_refused() {
        for (from, to) in [
            ("ink=ed4f5c", "ink=ed4f5"),
            ("ink=ed4f5c", "ink=ed4f5c00"),
            ("ink=ed4f5c", "ink=red"),
            ("ink=ed4f5c", "ink="),
            ("ink=ed4f5c", "ink=ed4f5c ink=ed4f5c"),
            // Only a panel's text has an ink of its own.
            ("label=fit ink=ed4f5c", "ink=ed4f5c"),
        ] {
            assert!(VERSION_5.contains(from), "{from:?} is not in the reference");
            let text = VERSION_5.replacen(from, to, 1);
            assert_eq!(decode(&text, 5), None, "{from:?} replaced by {to:?}");
        }
    }
    #[test]
    fn an_invalid_scene_is_recognised_before_it_is_written() {
        assert!(sample().valid());
        let mut stray_ink = sample();
        if let Shape::Text { ink, .. } = &mut stray_ink.objects[6].shape {
            *ink = Some([1, 2, 3]);
        }
        assert!(!stray_ink.valid(), "an ink without a panel is not written");
        let broken: [fn(&mut Scene); 8] = [
            |s| s.width = 0,
            |s| s.region.max.x = 5000.,
            |s| s.last_id = 3,
            |s| s.objects.swap(0, 1),
            |s| s.objects[0].style.width = f64::NAN,
            |s| s.objects[1].rotation = f64::INFINITY,
            |s| s.background[0].walls.push(3),
            |s| s.objects[1].fill[0].walls.push(10),
        ];
        for change in broken {
            let mut scene = sample();
            change(&mut scene);
            assert!(!scene.valid());
        }
    }
}
