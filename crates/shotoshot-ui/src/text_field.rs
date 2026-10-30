//! ROLE: In-place, multi-line text field for platforms without a native one (Linux, macOS).
//! OWNER: The platform host opens one field at a time and commits or discards it.
//! INVARIANTS: `caret` and `anchor` are byte offsets on char boundaries of `text`;
//! geometry comes from the caller's text measure, the same one that renders the text.
use crate::{Canvas, Capture, Ink, caret, keys};
use shotoshot_core::{
    Label, Modifiers, Object, Point, Rect, Shape, kept_block, label_ink, rotate_point, text_lines,
};
use std::ops::Range;

const KEY_BACKSPACE: u32 = 8;
const KEY_END: u32 = 35;
const KEY_HOME: u32 = 36;
const KEY_A: u32 = 65;
const KEY_C: u32 = 67;
const KEY_V: u32 = 86;
const KEY_X: u32 = 88;
const KEY_Z: u32 = 90;
pub(crate) const SELECTION: Ink = Ink::with_alpha([88, 166, 255], 0.4);

/// What the platform must do after a key reached the field.
#[derive(Clone, Debug, PartialEq)]
pub enum FieldKey {
    /// Not a field key; nothing changed.
    Ignored,
    /// The field changed; repaint.
    Edited,
    /// Put this text on the clipboard (the field already removed it for a cut).
    Copy(String),
    /// Insert the clipboard's text with `insert`.
    Paste,
}

pub struct TextField {
    pub position: Point,
    /// Existing text object being edited; hidden from the annotations meanwhile.
    pub object: Option<u64>,
    pub size: f32,
    pub color: [u8; 3],
    /// Clockwise radians around the text centre, inherited from an edited object.
    pub rotation: f64,
    /// The panel the text sits on, whose width may break its lines.
    pub label: Option<Label>,
    /// The ink of the text on its panel; `None` is the black or white that contrasts with it.
    pub ink: Option<[u8; 3]>,
    /// The height of the text block a box keeps while it holds no text: the height it was
    /// drawn at, or its own when reopened (`kept_block`).
    pub height: f64,
    /// Newlines are refused (typed, pasted or Enter); used by the crop-size fields.
    single_line: bool,
    text: String,
    caret: usize,
    anchor: usize,
    dragging: bool,
    undo: Vec<(String, usize, usize)>,
}

impl TextField {
    /// A field at `p`, or over the topmost existing text hit by `p`.
    pub fn open(capture: &Capture, p: Point) -> Self {
        match capture.text_at(p) {
            Some(Object {
                id,
                shape:
                    Shape::Text {
                        position,
                        text,
                        size,
                        label,
                        ink,
                        extent,
                        ..
                    },
                style,
                rotation,
                ..
            }) => Self {
                position: *position,
                object: Some(*id),
                size: *size as f32,
                color: style.color,
                rotation: *rotation,
                label: *label,
                ink: *ink,
                height: extent.y,
                caret: text.len(),
                anchor: text.len(),
                text: text.clone(),
                single_line: false,
                dragging: false,
                undo: Vec::new(),
            },
            _ => Self {
                position: p,
                object: None,
                size: capture.text_size,
                color: capture.editor.style.color,
                rotation: 0.,
                label: capture.new_label,
                ink: capture.label_ink,
                height: 0.,
                single_line: false,
                text: String::new(),
                caret: 0,
                anchor: 0,
                dragging: false,
                undo: Vec::new(),
            },
        }
    }
    /// A one-line field holding `text`, entirely selected so typing replaces it.
    pub fn line(position: Point, size: f32, color: [u8; 3], text: String) -> Self {
        Self {
            position,
            object: None,
            size,
            color,
            rotation: 0.,
            label: None,
            ink: None,
            height: 0.,
            single_line: true,
            caret: text.len(),
            anchor: 0,
            text,
            dragging: false,
            undo: Vec::new(),
        }
    }
    pub fn text(&self) -> &str {
        &self.text
    }
    pub fn caret(&self) -> usize {
        self.caret
    }
    pub fn select_all(&mut self) {
        self.anchor = 0;
        self.caret = self.text.len();
    }
    /// Selected byte range, ordered.
    pub fn selection(&self) -> (usize, usize) {
        (self.caret.min(self.anchor), self.caret.max(self.anchor))
    }
    /// What a moved caret changes, for its blink to start over.
    pub fn caret_state(&self) -> [usize; 3] {
        [self.caret, self.anchor, self.text.len()]
    }
    pub fn selected_text(&self) -> &str {
        let (a, b) = self.selection();
        &self.text[a..b]
    }
    pub fn dragging(&self) -> bool {
        self.dragging
    }
    fn size(&self) -> f64 {
        // Sizes go through f32 exactly as the Windows field stores them.
        self.size as f64
    }

    fn remember(&mut self) {
        self.undo.push((self.text.clone(), self.caret, self.anchor));
        if self.undo.len() > 100 {
            self.undo.remove(0);
        }
    }
    /// Replaces the selection with `s` (typing or pasting).
    pub fn insert(&mut self, s: &str) {
        let s: String = s
            .chars()
            .filter(|c| (*c == '\n' && !self.single_line) || !c.is_control())
            .collect();
        if s.is_empty() && self.caret == self.anchor {
            return;
        }
        self.remember();
        let (a, b) = self.selection();
        self.text.replace_range(a..b, &s);
        self.caret = a + s.len();
        self.anchor = self.caret;
    }
    fn delete(&mut self, forward: bool) {
        if self.caret == self.anchor {
            let to = if forward {
                next_char(&self.text, self.caret)
            } else {
                previous_char(&self.text, self.caret)
            };
            if to == self.caret {
                return;
            }
            self.anchor = to;
        }
        self.insert("");
    }
    fn move_to(&mut self, offset: usize, extend: bool) {
        self.caret = offset;
        if !extend {
            self.anchor = offset;
        }
    }
    /// Byte ranges of the lines as drawn: typed lines, broken at the label's width.
    fn lines(&self, measure: &mut impl FnMut(&str) -> Point) -> Vec<Range<usize>> {
        let wrap = self.label.and_then(Label::wrap);
        text_lines(&self.text, wrap, &mut |s| measure(s).x)
    }
    /// The drawn line `offset` is on; at a break inside a typed line, the one it starts.
    fn row(lines: &[Range<usize>], offset: usize) -> usize {
        lines.iter().rposition(|l| l.start <= offset).unwrap_or(0)
    }
    /// How far line `row` starts right of `position`: a label sets each line in the middle
    /// of a block `width` wide, a plain text at its left.
    fn indent(
        &self,
        lines: &[Range<usize>],
        row: usize,
        width: f64,
        measure: &mut impl FnMut(&str) -> Point,
    ) -> f64 {
        if self.label.is_none() {
            return 0.;
        }
        let shown = self.text[lines[row].clone()].trim_end();
        ((width - measure(shown).x) / 2.).max(0.)
    }
    /// Where the caret may stand on line `row`: before the spaces a wrapped line ends with,
    /// since that end is where the next line starts.
    fn reach(&self, lines: &[Range<usize>], row: usize) -> Range<usize> {
        let line = lines[row].clone();
        if lines
            .get(row + 1)
            .is_some_and(|next| next.start == line.end)
        {
            line.start..line.start + self.text[line].trim_end_matches(' ').len()
        } else {
            line
        }
    }

    /// Editing keys (virtual-key codes). Escape and Ctrl+Enter belong to the session.
    pub fn key(
        &mut self,
        key: u32,
        m: Modifiers,
        measure: &mut impl FnMut(&str) -> Point,
    ) -> FieldKey {
        let extend = m.shift;
        match key {
            KEY_BACKSPACE => self.delete(false),
            keys::KEY_DELETE => self.delete(true),
            keys::KEY_ENTER if !m.ctrl && !self.single_line => self.insert("\n"),
            keys::KEY_LEFT if !extend && self.caret != self.anchor => {
                let (a, _) = self.selection();
                self.move_to(a, false);
            }
            keys::KEY_RIGHT if !extend && self.caret != self.anchor => {
                let (_, b) = self.selection();
                self.move_to(b, false);
            }
            keys::KEY_LEFT => {
                let to = if m.ctrl {
                    previous_word(&self.text, self.caret)
                } else {
                    previous_char(&self.text, self.caret)
                };
                self.move_to(to, extend);
            }
            keys::KEY_RIGHT => {
                let to = if m.ctrl {
                    next_word(&self.text, self.caret)
                } else {
                    next_char(&self.text, self.caret)
                };
                self.move_to(to, extend);
            }
            KEY_HOME | KEY_END if m.ctrl => {
                let to = if key == KEY_HOME { 0 } else { self.text.len() };
                self.move_to(to, extend);
            }
            KEY_HOME | KEY_END => {
                let lines = self.lines(measure);
                let line = self.reach(&lines, Self::row(&lines, self.caret));
                let to = if key == KEY_HOME {
                    line.start
                } else {
                    line.end
                };
                self.move_to(to, extend);
            }
            keys::KEY_UP | keys::KEY_DOWN => {
                let lines = self.lines(measure);
                let width = self.block(measure).x;
                let row = Self::row(&lines, self.caret);
                let start = lines[row].start;
                let x = self.indent(&lines, row, width, measure)
                    + measure(&self.text[start..self.caret]).x;
                let target = if key == keys::KEY_UP {
                    row.checked_sub(1)
                } else {
                    Some(row + 1).filter(|r| *r < lines.len())
                };
                let to = match target {
                    Some(r) => {
                        let line = self.reach(&lines, r);
                        let x = x - self.indent(&lines, r, width, measure);
                        line.start + nearest_offset(&self.text[line], x, measure)
                    }
                    None if key == keys::KEY_UP => 0,
                    None => self.text.len(),
                };
                self.move_to(to, extend);
            }
            KEY_A if m.ctrl => self.select_all(),
            KEY_C if m.ctrl => return FieldKey::Copy(self.selected_text().to_owned()),
            KEY_X if m.ctrl => {
                let cut = self.selected_text().to_owned();
                self.insert("");
                return FieldKey::Copy(cut);
            }
            KEY_V if m.ctrl => return FieldKey::Paste,
            KEY_Z if m.ctrl => {
                let Some((text, caret, anchor)) = self.undo.pop() else {
                    return FieldKey::Edited;
                };
                (self.text, self.caret, self.anchor) = (text, caret, anchor);
            }
            _ => return FieldKey::Ignored,
        }
        FieldKey::Edited
    }

    fn line_height(&self, measure: &mut impl FnMut(&str) -> Point) -> f64 {
        measure("X").y.max(1.)
    }
    /// Size of the text block the committed object will have: its lines, and a wrapped
    /// label's whole width.
    fn block(&self, measure: &mut impl FnMut(&str) -> Point) -> Point {
        let measured = match self.label.and_then(Label::wrap) {
            None => measure(&self.text),
            Some(wrap) => Point {
                x: wrap,
                y: self.lines(measure).len() as f64 * self.line_height(measure),
            },
        };
        kept_block(&self.text, self.label, measured, self.size(), self.height)
    }
    fn center(&self, measure: &mut impl FnMut(&str) -> Point) -> Point {
        self.position + self.block(measure) * 0.5
    }
    /// Byte offset nearest to `p` and whether `p` lies on the field's text or its panel.
    pub fn hit(&self, p: Point, measure: &mut impl FnMut(&str) -> Point) -> (usize, bool) {
        let local = rotate_point(p, self.center(measure), -self.rotation) - self.position;
        let line_height = self.line_height(measure);
        let extent = self.block(measure);
        let lines = self.lines(measure);
        let row = ((local.y / line_height).floor().max(0.) as usize).min(lines.len() - 1);
        let line = self.reach(&lines, row);
        let x = local.x - self.indent(&lines, row, extent.x, measure);
        let offset = line.start + nearest_offset(&self.text[line], x, measure);
        let pad = self
            .label
            .map_or(Point::default(), |_| Label::padding(self.size()));
        // A slack margin keeps an empty or thin field easy to click.
        let inside = local.x >= -4. - pad.x
            && local.y >= -4. - pad.y
            && local.x <= extent.x.max(line_height) + 4. + pad.x
            && local.y <= extent.y.max(line_height) + 4. + pad.y;
        (offset, inside)
    }
    /// Places the caret (Shift extends); true when the press is on the field.
    pub fn pointer_down(
        &mut self,
        p: Point,
        extend: bool,
        measure: &mut impl FnMut(&str) -> Point,
    ) -> bool {
        if !self.hit(p, measure).1 {
            return false;
        }
        self.press(p, extend, measure);
        true
    }
    /// Places the caret nearest to `p` and starts a selection drag, wherever `p` lies.
    pub fn press(&mut self, p: Point, extend: bool, measure: &mut impl FnMut(&str) -> Point) {
        let offset = self.hit(p, measure).0;
        self.move_to(offset, extend);
        self.dragging = true;
    }
    pub fn pointer_move(&mut self, p: Point, measure: &mut impl FnMut(&str) -> Point) -> bool {
        if !self.dragging {
            return false;
        }
        self.caret = self.hit(p, measure).0;
        true
    }
    /// Ends a selection drag; true if one was in progress.
    pub fn pointer_up(&mut self) -> bool {
        std::mem::take(&mut self.dragging)
    }

    /// Adds the text, or updates the edited object, and selects it. Empty new text is
    /// dropped, unless it is a box, which keeps its size.
    pub fn commit(self, capture: &mut Capture, extent: Point) {
        let size = self.size();
        let extent = kept_block(&self.text, self.label, extent, size, self.height);
        if let Some(id) = self.object {
            capture
                .editor
                .update_text_inked(id, self.text, extent, self.color, size, self.ink);
            capture.select_committed_text(id);
        } else if let Some(id) = capture.editor.add_label(
            self.position,
            self.text,
            extent,
            self.color,
            size,
            self.label,
            self.ink,
        ) {
            capture.select_committed_text(id);
        }
    }

    /// Text, selection highlight and caret (unless blinked off), rotated like the
    /// committed text; the caret keeps its thickness at interface `scale`.
    pub fn draw<C: Canvas>(&self, canvas: &mut C, caret: bool, scale: f64) -> Result<(), C::Error> {
        let size = self.size();
        let mut measure_error = None;
        let mut measure = |s: &str| {
            canvas.measure_annotation_text(s, size).unwrap_or_else(|e| {
                measure_error.get_or_insert(e);
                Point::default()
            })
        };
        let line_height = self.line_height(&mut measure);
        let center = self.center(&mut measure);
        let block = self.block(&mut measure);
        let lines = self.lines(&mut measure);
        let caret_row = Self::row(&lines, self.caret);
        // Selection rectangles and caret, computed before borrowing the canvas to draw.
        let (a, b) = self.selection();
        let mut marks = Vec::new();
        // What is drawn: each line as broken here, never broken again by the canvas.
        let mut drawn = Vec::with_capacity(lines.len());
        for (row, line) in lines.iter().enumerate() {
            let (line_start, line_end) = (line.start, line.end);
            let top = self.position.y + row as f64 * line_height;
            let left = self.position.x + self.indent(&lines, row, block.x, &mut measure);
            drawn.push((
                Point { x: left, y: top },
                self.text[line.clone()].trim_end(),
            ));
            let x_at = |offset: usize, measure: &mut dyn FnMut(&str) -> Point| {
                left + measure(&self.text[line_start..offset]).x
            };
            if b > a && a <= line_end && b >= line_start {
                let from = x_at(a.max(line_start), &mut measure);
                let to = x_at(b.min(line_end), &mut measure);
                // A selected line break shows as a narrow trailing mark, as on Windows.
                let to = if b > line_end { to + size * 0.3 } else { to };
                marks.push((from, to, top, true));
            }
            if caret && a == b && row == caret_row {
                let place = caret::Place::of(self.caret > line_start, self.caret < line_end);
                let x = caret::center(x_at(self.caret, &mut measure), size, scale, place);
                marks.push((x, x, top, false));
            }
        }
        let ink = match self.label {
            Some(_) => self.ink.unwrap_or_else(|| label_ink(self.color)),
            None => self.color,
        };
        if let Some(error) = measure_error {
            return Err(error);
        }
        canvas.set_rotation((self.rotation != 0.).then_some((center, self.rotation)))?;
        let result = (|| {
            if self.label.is_some() {
                let pad = Label::padding(size);
                let panel = Rect {
                    min: self.position - pad,
                    max: self.position + block + pad,
                };
                canvas.fill_round_rect(panel, Label::radius(size), Ink::solid(self.color))?;
            }
            for &(from, to, top, selected) in &marks {
                if selected {
                    let r = Rect {
                        min: Point { x: from, y: top },
                        max: Point {
                            x: to,
                            y: top + line_height,
                        },
                    };
                    canvas.fill_rect(r, SELECTION)?;
                }
            }
            for &(min, line) in &drawn {
                let bounds = Rect {
                    min,
                    max: min + Point { x: 1e6, y: 1e6 },
                };
                canvas.annotation_text(line, bounds, size, Ink::solid(ink))?;
            }
            for &(x, _, top, selected) in &marks {
                if !selected {
                    let a = Point { x, y: top };
                    let b = Point {
                        x,
                        y: top + line_height,
                    };
                    canvas.line(a, b, Ink::solid(ink), caret::width(size, scale))?;
                }
            }
            Ok(())
        })();
        canvas.set_rotation(None)?;
        result
    }
}

fn previous_char(s: &str, i: usize) -> usize {
    s[..i].char_indices().next_back().map_or(0, |(j, _)| j)
}
fn next_char(s: &str, i: usize) -> usize {
    s[i..].chars().next().map_or(i, |c| i + c.len_utf8())
}
fn previous_word(s: &str, i: usize) -> usize {
    let head = s[..i].trim_end_matches(|c: char| !c.is_alphanumeric());
    head.trim_end_matches(char::is_alphanumeric).len()
}
fn next_word(s: &str, i: usize) -> usize {
    let tail = &s[i..];
    let word = tail.len() - tail.trim_start_matches(char::is_alphanumeric).len();
    let gap = tail[word..].len()
        - tail[word..]
            .trim_start_matches(|c: char| !c.is_alphanumeric())
            .len();
    i + word + gap
}
/// Offset within `line` whose prefix width is closest to `x`.
fn nearest_offset(line: &str, x: f64, measure: &mut impl FnMut(&str) -> Point) -> usize {
    let mut best = (0, x.abs());
    for (i, c) in line.char_indices() {
        let end = i + c.len_utf8();
        let distance = (measure(&line[..end]).x - x).abs();
        if distance < best.1 {
            best = (end, distance);
        }
    }
    best.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Image;

    /// Monospace stand-in: 10 px per char, 20 px lines.
    fn measure(s: &str) -> Point {
        let lines: Vec<&str> = s.split('\n').collect();
        Point {
            x: lines.iter().map(|l| l.chars().count()).max().unwrap_or(0) as f64 * 10.,
            y: lines.len() as f64 * 20.,
        }
    }
    fn none() -> Modifiers {
        Modifiers::default()
    }
    fn with(f: impl Fn(&mut Modifiers)) -> Modifiers {
        let mut m = Modifiers::default();
        f(&mut m);
        m
    }
    fn field(text: &str) -> TextField {
        let capture = Capture::new(Image::solid(400, 300, [0; 4]));
        let mut f = TextField::open(&capture, Point { x: 50., y: 60. });
        f.insert(text);
        f
    }

    #[test]
    fn typing_deleting_and_undo_edit_unicode_text() {
        let mut f = field("héllo");
        f.key(KEY_BACKSPACE, none(), &mut measure);
        f.key(keys::KEY_LEFT, none(), &mut measure);
        f.key(keys::KEY_LEFT, none(), &mut measure);
        f.key(KEY_BACKSPACE, none(), &mut measure);
        assert_eq!(f.text(), "hll");
        f.insert("é");
        f.key(keys::KEY_ENTER, none(), &mut measure);
        f.insert("🎮");
        assert_eq!(f.text(), "hé\n🎮ll");
        f.key(KEY_Z, with(|m| m.ctrl = true), &mut measure);
        assert_eq!(f.text(), "hé\nll");
        f.key(KEY_Z, with(|m| m.ctrl = true), &mut measure);
        assert_eq!(f.text(), "héll");
    }
    #[test]
    fn shift_and_ctrl_navigation_select_words_and_lines() {
        let mut f = field("alpha beta\ngamma");
        f.key(KEY_HOME, with(|m| m.ctrl = true), &mut measure);
        f.key(
            keys::KEY_RIGHT,
            with(|m| (m.ctrl, m.shift) = (true, true)),
            &mut measure,
        );
        assert_eq!(f.selected_text(), "alpha ");
        f.key(KEY_END, with(|m| m.shift = true), &mut measure);
        assert_eq!(f.selected_text(), "alpha beta");
        assert_eq!(
            f.key(KEY_X, with(|m| m.ctrl = true), &mut measure),
            FieldKey::Copy("alpha beta".into())
        );
        assert_eq!(f.text(), "\ngamma");
        f.key(keys::KEY_DOWN, none(), &mut measure);
        assert_eq!(f.selection(), (1, 1));
    }
    #[test]
    fn vertical_moves_keep_the_caret_column() {
        let mut f = field("abcdef\nxy\nlonger line");
        f.key(keys::KEY_UP, none(), &mut measure);
        f.key(keys::KEY_UP, none(), &mut measure);
        // From the end of "longer line" (x=110) to "xy" end, then to "abcdef" column 2.
        assert_eq!(f.selection(), (2, 2));
    }
    #[test]
    fn clicks_place_the_caret_and_drags_select() {
        let mut f = field("one\ntwo");
        assert!(f.pointer_down(Point { x: 71., y: 85. }, false, &mut measure));
        assert_eq!(f.selection(), (6, 6));
        f.pointer_move(Point { x: 49., y: 61. }, &mut measure);
        assert_eq!(f.selected_text(), "one\ntw");
        assert!(f.pointer_up());
        assert!(!f.pointer_down(Point { x: 300., y: 250. }, false, &mut measure));
    }
    #[test]
    fn rotated_field_hits_in_its_own_frame() {
        let mut f = field("abcd");
        f.rotation = std::f64::consts::FRAC_PI_2;
        // Centre (70, 70); the text's right end rotates to the bottom.
        let (offset, inside) = f.hit(Point { x: 70., y: 89. }, &mut measure);
        assert!(inside);
        assert_eq!(offset, 4);
    }
    #[test]
    fn commit_adds_new_text_and_updates_an_existing_object() {
        let mut capture = Capture::new(Image::solid(400, 300, [0; 4]));
        let mut f = TextField::open(&capture, Point { x: 50., y: 60. });
        f.insert("Hello");
        f.commit(&mut capture, measure("Hello"));
        let id = capture.editor.objects()[0].id;
        let mut again = TextField::open(&capture, Point { x: 55., y: 65. });
        assert_eq!((again.object, again.text()), (Some(id), "Hello"));
        again.key(KEY_BACKSPACE, none(), &mut measure);
        again.commit(&mut capture, measure("Hell"));
        assert!(matches!(
            &capture.editor.objects()[0].shape,
            Shape::Text { text, .. } if text == "Hell"
        ));
        let empty = TextField::open(&capture, Point { x: 300., y: 200. });
        empty.commit(&mut capture, Point::default());
        assert_eq!(capture.editor.objects().len(), 1);
    }
    #[test]
    fn an_empty_box_keeps_the_height_it_was_drawn_at_and_reopens() {
        let mut capture = Capture::new(Image::solid(400, 300, [0; 4]));
        capture.new_label = Some(Label::Wrap(120.));
        let mut f = TextField::open(&capture, Point { x: 50., y: 60. });
        f.height = 70.;
        let panel = |f: &TextField| f.block(&mut measure);
        assert_eq!(panel(&f), Point { x: 120., y: 70. }, "drawn while empty");
        f.insert("a");
        assert_eq!(panel(&f).y, 20., "a typed line takes over");
        f.key(KEY_BACKSPACE, none(), &mut measure);
        f.commit(&mut capture, measure(""));
        assert!(matches!(
            &capture.editor.objects()[0].shape,
            Shape::Text { text, extent, .. } if text.is_empty() && extent.y == 70.
        ));
        let mut again = TextField::open(&capture, Point { x: 60., y: 90. });
        assert_eq!((again.object.is_some(), again.height), (true, 70.));
        again.insert("Typed");
        again.commit(&mut capture, measure("Typed"));
        assert!(matches!(
            &capture.editor.objects()[0].shape,
            Shape::Text { text, .. } if text == "Typed"
        ));
    }
    #[test]
    fn a_wrapped_label_moves_the_caret_along_its_drawn_lines() {
        let mut capture = Capture::new(Image::solid(400, 300, [0; 4]));
        capture.new_label = Some(Label::Wrap(50.));
        let mut f = TextField::open(&capture, Point { x: 50., y: 60. });
        // Lines "ab cd " and "efg" at 10 px a letter.
        f.insert("ab cd efg");
        assert_eq!(f.label, Some(Label::Wrap(50.)));
        f.key(KEY_HOME, none(), &mut measure);
        assert_eq!(f.caret(), 6, "the second line starts after the space");
        // Lines are centred: "efg" starts 10 px in, under the "b" of "ab cd".
        f.key(keys::KEY_UP, none(), &mut measure);
        assert_eq!(f.caret(), 1);
        f.key(KEY_END, none(), &mut measure);
        assert_eq!(f.caret(), 5, "before the space the line ends with");
        f.key(keys::KEY_DOWN, none(), &mut measure);
        assert_eq!(f.caret(), 9, "the end of the shorter line below");
        // A click on the second line, inside the panel's left margin.
        let (offset, inside) = f.hit(Point { x: 46., y: 85. }, &mut measure);
        assert_eq!((offset, inside), (6, true));
    }
    #[test]
    fn a_box_field_takes_the_given_ink_draws_with_it_and_keeps_it() {
        let mut capture = Capture::new(Image::solid(400, 300, [0; 4]));
        capture.new_label = Some(Label::Fit);
        capture.editor.style.color = [250, 192, 66];
        capture.label_ink = Some([237, 79, 92]);
        let mut f = TextField::open(&capture, Point { x: 50., y: 60. });
        assert_eq!(
            f.ink,
            Some([237, 79, 92]),
            "the next box takes the ink given"
        );
        f.insert("Hi");
        let inks = |f: &TextField| {
            let mut canvas = crate::scale_tests::Recorder {
                size: Point { x: 400., y: 300. },
                ops: Vec::new(),
            };
            f.draw(&mut canvas, false, 1.).unwrap();
            canvas
                .ops
                .into_iter()
                .filter(|op| op.name == "text")
                .map(|op| op.fixed)
                .collect::<Vec<_>>()
        };
        assert_eq!(inks(&f), [format!("{:?}", Ink::solid([237, 79, 92]))]);
        f.ink = None;
        assert_eq!(
            inks(&f),
            [format!("{:?}", Ink::solid([0; 3]))],
            "automatic: dark on yellow"
        );
        f.ink = Some([1, 2, 3]);
        f.commit(&mut capture, measure("Hi"));
        assert_eq!(capture.editor.objects()[0].text_ink(), [1, 2, 3]);
        capture.label_ink = None;
        let again = TextField::open(&capture, Point { x: 55., y: 65. });
        assert_eq!(again.ink, Some([1, 2, 3]), "an edited box keeps its own");
    }
}
