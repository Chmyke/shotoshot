//! ROLE: The scene text inside a `.shoto` file: frame, region and objects. Version 2 adds
//! stacked coats and the shapes each spares (`Paint`); version 1 has one plain coat at most.
//! Version 3 adds the panel of a text (`Label`), 4 bends, 5 the ink of a panel's text, 6 elbow
//! arrows, 7 blurred zones (`Shape::Blur`), 8 a box left without text and 9 a pen loop
//! that holds or spares a colour.
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
    /// bends, 5 once a panel's text has an ink of its own, 6 once an arrow is an elbow, 7 once
    /// a zone is blurred, 8 once a box is left empty, 9 once a pen loop is filled or spared
    /// by a coat, so simpler files still open in an earlier Shotoshot.
    pub fn version(&self) -> u32 {
        let plain = |coats: &[Paint]| coats.len() <= 1 && coats.iter().all(|c| c.walls.is_empty());
        let labelled = |o: &Object| matches!(o.shape, Shape::Text { label: Some(_), .. });
        let inked = |o: &Object| matches!(o.shape, Shape::Text { ink: Some(_), .. });
        let elbow = |o: &Object| matches!(o.shape, Shape::Arrow { elbow: true, .. });
        let empty =
            |o: &Object| matches!(&o.shape, Shape::Text { text, .. } if text.trim().is_empty());
        let pen = |id: &u64| {
            self.objects
                .iter()
                .any(|o| o.id == *id && matches!(o.shape, Shape::Stroke { .. }))
        };
        let pen_loop = |o: &Object| matches!(o.shape, Shape::Stroke { .. }) && !o.fill.is_empty();
        if self.objects.iter().any(pen_loop) || self.coats().any(|c| c.walls.iter().any(pen)) {
            9
        } else if self.objects.iter().any(empty) {
            8
        } else if self.objects.iter().any(Object::is_blur) {
            7
        } else if self.objects.iter().any(elbow) {
            6
        } else if self.objects.iter().any(inked) {
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
        fill,
    } = object;
    let measure = |n: &f64| n.is_finite() && *n >= 0.;
    rotation.is_finite()
        && measure(width)
        && match shape {
            Shape::Rectangle(r) | Shape::Ellipse(r) => finite(*r),
            // Never turned, never filled, a strength the effect knows.
            Shape::Blur(r) => {
                finite(*r)
                    && *rotation == 0.
                    && fill.is_empty()
                    && (crate::blur::MIN_BLUR_RADIUS..=crate::blur::MAX_BLUR_RADIUS).contains(width)
            }
            Shape::Arrow {
                start,
                end,
                start_attach,
                end_attach,
                head: _,
                bend,
                elbow,
            } => {
                start.finite()
                    && end.finite()
                    && bend.finite()
                    && !(*elbow && bent(*bend))
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
                    // A box stays when validated empty; a bare text needs words.
                    && (label.is_some() || !text.trim().is_empty())
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
        Shape::Blur(_) => "blur",
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
        Shape::Rectangle(r) | Shape::Ellipse(r) | Shape::Blur(r) => {
            write!(out, " rect={},{},{},{}", r.min.x, r.min.y, r.max.x, r.max.y)?;
        }
        Shape::Arrow {
            start,
            end,
            start_attach,
            end_attach,
            head,
            bend,
            elbow,
        } => {
            write!(
                out,
                " from={},{} to={},{} head={}",
                start.x, start.y, end.x, end.y, *head as u8
            )?;
            if *elbow {
                out.push_str(" elbow=1");
            } else if bent(*bend) {
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
        "blur" if version >= 7 => Shape::Blur(rect(f.take("rect")?)?),
        "arrow" => {
            let start = point(f.take("from")?)?;
            let end = point(f.take("to")?)?;
            let head = flag(f.take("head")?)?;
            let start_attach = match f.take("start") {
                Some(text) => Some(attachment(text)?),
                None => None,
            };
            let end_attach = match f.take("end") {
                Some(text) => Some(attachment(text)?),
                None => None,
            };
            let bend = match f.take("bend") {
                Some(_) if version < 4 => return None,
                // A straight line is written without the field, never as a zero bend.
                Some(text) => point(text).filter(|b| bent(*b))?,
                None => Point::default(),
            };
            let elbow = match f.take("elbow") {
                Some(_) if version < 6 => return None,
                // Written only when set, and never with a bend it would not draw.
                Some("1") if !bent(bend) => true,
                Some(_) => return None,
                None => false,
            };
            Shape::Arrow {
                start,
                end,
                start_attach,
                end_attach,
                head,
                bend,
                elbow,
            }
        }
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
            let text =
                String::from_utf8(hex(f.take("text").filter(|t| t.len() <= 2 * MAX_TEXT)?)?)
                    .ok()?;
            if version < 8 && text.trim().is_empty() {
                return None;
            }
            Shape::Text {
                position: pair(x, y),
                size: number(f.take("size")?)?,
                extent: pair(w, h),
                label,
                ink,
                text,
            }
        }
        _ => return None,
    };
    // Before 9 the bucket filled no pen stroke.
    if version < 9 && matches!(shape, Shape::Stroke { .. }) && !fill.is_empty() {
        return None;
    }
    f.0.is_empty().then_some(Object {
        rotation,
        id,
        shape,
        style,
        fill,
    })
}

#[cfg(test)]
pub(super) mod tests;
