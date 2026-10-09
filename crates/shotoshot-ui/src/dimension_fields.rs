//! ROLE: Crop-size fields (width, height) for platforms without native edit boxes.
//! OWNER: The platform host keeps one while the size badge is open and draws it over
//! the badge (`Capture::draw_dimensions`); `apply` resizes the region.
//! INVARIANTS: Each field shows its text from byte `scroll`, a char boundary, and its
//! caret stays inside the visible width; geometry comes from the rendering measure.
use crate::{Canvas, Capture, FieldKey, Ink, TextField, crop, keys, text_field::SELECTION};
use shotoshot_core::{Modifiers, Point, Rect};

const FIELD_SIZE: f64 = 14.;
const FIELD_HEIGHT: f64 = 20.;
const FIELD_INK: [u8; 3] = [240, 240, 240];

/// What the platform must do after a key reached the open fields.
#[derive(Clone, Debug, PartialEq)]
pub enum DimensionKey {
    /// Enter: apply with `DimensionFields::apply`, closing on success.
    Commit,
    /// Escape: close without applying.
    Cancel,
    Field(FieldKey),
}

struct Slot {
    field: TextField,
    left: f64,
    scroll: usize,
}

pub struct DimensionFields {
    bounds: Rect,
    width: f64,
    slots: [Slot; 2],
    focus: usize,
    /// Why the last `apply` was refused; shown under the badge until the next press.
    pub error: Option<&'static str>,
}

impl DimensionFields {
    /// Font size of the fields, as on Windows; platforms measure with it.
    pub const FONT_SIZE: f64 = FIELD_SIZE;
    /// Width and height fields over the badge, the width one focused and selected.
    pub fn open(capture: &Capture) -> Option<Self> {
        let size = capture.region?.size();
        let bounds = capture.dimensions_bounds()?;
        // Whole pixels, as the Windows edit boxes are laid out.
        let width = ((bounds.size().x - 22.) / 2.).max(1.).floor();
        let slot = |index: usize, value: f64| {
            let left = bounds.min.x + 4. + index as f64 * (width + 14.);
            let position = Point {
                x: left,
                y: bounds.min.y + 3.,
            };
            Slot {
                field: TextField::line(
                    position,
                    FIELD_SIZE as f32,
                    FIELD_INK,
                    format!("{value:.0}"),
                ),
                left,
                scroll: 0,
            }
        };
        Some(Self {
            bounds,
            width,
            slots: [slot(0, size.x), slot(1, size.y)],
            focus: 0,
            error: None,
        })
    }
    pub fn texts(&self) -> [&str; 2] {
        [self.slots[0].field.text(), self.slots[1].field.text()]
    }
    /// Index of the focused field: 0 width, 1 height.
    pub fn focus(&self) -> usize {
        self.focus
    }
    /// Resizes the region from the typed expressions; false (with `error`) keeps it.
    pub fn apply(&mut self, capture: &mut Capture) -> bool {
        let Some(region) = capture.region else {
            return false;
        };
        let [width, height] = self.texts();
        match crop::resize_region(
            region,
            width,
            height,
            capture.image.width,
            capture.image.height,
        ) {
            Ok(region) => {
                capture.region = Some(region);
                true
            }
            Err(error) => {
                self.error = Some(error);
                false
            }
        }
    }
    /// Enter, Escape and Tab belong to the fields; other keys edit the focused one.
    pub fn key(
        &mut self,
        key: u32,
        m: Modifiers,
        measure: &mut impl FnMut(&str) -> Point,
    ) -> DimensionKey {
        match key {
            keys::KEY_ENTER => DimensionKey::Commit,
            keys::KEY_ESCAPE => DimensionKey::Cancel,
            keys::KEY_TAB => {
                self.focus = 1 - self.focus;
                self.slots[self.focus].field.select_all();
                self.reveal(self.focus, measure);
                DimensionKey::Field(FieldKey::Edited)
            }
            _ => {
                let result = self.slots[self.focus].field.key(key, m, measure);
                self.reveal(self.focus, measure);
                DimensionKey::Field(result)
            }
        }
    }
    /// Typed or pasted text replaces the focused field's selection.
    pub fn insert(&mut self, text: &str, measure: &mut impl FnMut(&str) -> Point) {
        self.slots[self.focus].field.insert(text);
        self.reveal(self.focus, measure);
    }
    /// Focuses the field under `p` and places its caret (Shift extends); true when
    /// the press is on the badge, which the fields own while open.
    pub fn pointer_down(
        &mut self,
        p: Point,
        extend: bool,
        measure: &mut impl FnMut(&str) -> Point,
    ) -> bool {
        if !self.bounds.contains(p) {
            return false;
        }
        self.focus = usize::from(p.x >= self.bounds.center().x);
        self.error = None;
        self.slots[self.focus].field.press(p, extend, measure);
        self.reveal(self.focus, measure);
        true
    }
    pub fn pointer_move(&mut self, p: Point, measure: &mut impl FnMut(&str) -> Point) {
        for index in 0..2 {
            if self.slots[index].field.pointer_move(p, measure) {
                self.reveal(index, measure);
            }
        }
    }
    /// Ends a selection drag; true if one was in progress.
    pub fn pointer_up(&mut self) -> bool {
        let [a, b] = &mut self.slots;
        a.field.pointer_up() | b.field.pointer_up()
    }

    /// Scrolls field `index` by whole characters so its caret stays visible.
    fn reveal(&mut self, index: usize, measure: &mut impl FnMut(&str) -> Point) {
        let visible = self.width - 2.;
        let slot = &mut self.slots[index];
        let text = slot.field.text();
        let caret = slot.field.caret();
        let mut scroll = slot.scroll.min(caret);
        while !text.is_char_boundary(scroll) {
            scroll -= 1;
        }
        if measure(text).x <= visible {
            scroll = 0;
        }
        while scroll < caret && measure(&text[scroll..caret]).x > visible {
            scroll += text[scroll..].chars().next().map_or(1, char::len_utf8);
        }
        slot.scroll = scroll;
        slot.field.position.x = slot.left - measure(&text[..scroll]).x;
    }

    /// Both fields, clipped to their boxes; selection and caret in the focused one.
    pub fn draw<C: Canvas>(&self, canvas: &mut C) -> Result<(), C::Error> {
        let top = self.bounds.min.y + 3.;
        for (index, slot) in self.slots.iter().enumerate() {
            let text = slot.field.text();
            let right = slot.left + self.width;
            let area = Rect {
                min: Point {
                    x: slot.left,
                    y: top,
                },
                max: Point {
                    x: right,
                    y: top + FIELD_HEIGHT,
                },
            };
            let mut x_at = |offset: usize| -> Result<f64, C::Error> {
                let offset = offset.max(slot.scroll);
                let x = slot.left
                    + canvas
                        .measure_text(&text[slot.scroll..offset], FIELD_SIZE)?
                        .x;
                Ok(x.min(right))
            };
            let focused = index == self.focus;
            let (a, b) = slot.field.selection();
            let marks = if focused {
                Some((x_at(a)?, x_at(b)?))
            } else {
                None
            };
            if let Some((from, to)) = marks
                && b > a
            {
                let selection = Rect {
                    min: Point { x: from, y: top },
                    max: Point {
                        x: to,
                        y: top + FIELD_HEIGHT,
                    },
                };
                canvas.fill_rect(selection, SELECTION)?;
            }
            let ink = Ink::solid(slot.field.color);
            canvas.text(&text[slot.scroll..], area, FIELD_SIZE, ink)?;
            if let Some((from, _)) = marks
                && a == b
            {
                let caret = Point {
                    x: from,
                    y: top + 2.,
                };
                let bottom = Point {
                    x: from,
                    y: top + FIELD_HEIGHT - 2.,
                };
                canvas.line(caret, bottom, ink, 1.)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Image;

    /// Monospace stand-in: 10 px per char.
    fn measure(s: &str) -> Point {
        Point {
            x: s.chars().count() as f64 * 10.,
            y: 18.,
        }
    }
    fn capture() -> Capture {
        let mut capture = Capture::new(Image::solid(500, 300, [0; 4]));
        capture.selecting = false;
        capture.region = Rect::new(Point { x: 20., y: 70. }, Point { x: 120., y: 150. });
        capture
    }
    fn press(fields: &mut DimensionFields, key: u32) -> DimensionKey {
        fields.key(key, Modifiers::default(), &mut measure)
    }

    #[test]
    fn fields_open_with_the_region_size_and_replace_it_when_typing() {
        let mut capture = capture();
        let mut fields = DimensionFields::open(&capture).unwrap();
        assert_eq!(fields.texts(), ["100", "80"]);
        fields.insert("100+20", &mut measure);
        assert_eq!(
            press(&mut fields, keys::KEY_TAB),
            DimensionKey::Field(FieldKey::Edited)
        );
        fields.insert("160/2", &mut measure);
        assert_eq!(fields.texts(), ["100+20", "160/2"]);
        assert_eq!(press(&mut fields, keys::KEY_ENTER), DimensionKey::Commit);
        assert!(fields.apply(&mut capture));
        let region = capture.region.unwrap();
        assert_eq!(
            (region.min, region.max),
            (Point { x: 20., y: 70. }, Point { x: 140., y: 150. })
        );
    }

    #[test]
    fn invalid_or_oversized_entries_are_refused_with_a_message() {
        let mut capture = capture();
        let before = capture.region;
        let mut fields = DimensionFields::open(&capture).unwrap();
        fields.insert("1/0", &mut measure);
        assert!(!fields.apply(&mut capture));
        assert!(fields.error.is_some());
        assert_eq!(capture.region, before);
        let ctrl = Modifiers {
            ctrl: true,
            ..Default::default()
        };
        fields.key(65 /* A */, ctrl, &mut measure);
        fields.insert("490", &mut measure);
        assert!(!fields.apply(&mut capture), "must stay inside the desktop");
        assert_eq!(capture.region, before);
        assert_eq!(press(&mut fields, keys::KEY_ESCAPE), DimensionKey::Cancel);
    }

    #[test]
    fn newlines_never_enter_a_field_and_a_press_clears_the_error() {
        let capture = capture();
        let mut fields = DimensionFields::open(&capture).unwrap();
        fields.insert("12\n3", &mut measure);
        assert_eq!(fields.texts()[0], "123");
        fields.error = Some("bad");
        let height = Point {
            x: fields.bounds.max.x - 10.,
            y: fields.bounds.center().y,
        };
        assert!(fields.pointer_down(height, false, &mut measure));
        assert_eq!((fields.focus(), fields.error), (1, None));
        assert!(fields.pointer_up());
        let outside = fields.bounds.max + Point { x: 5., y: 5. };
        assert!(!fields.pointer_down(outside, false, &mut measure));
    }

    #[test]
    fn a_long_expression_scrolls_to_keep_the_caret_visible() {
        let capture = capture();
        let mut fields = DimensionFields::open(&capture).unwrap();
        let text = "1000+200+30+4-1-1-1";
        fields.insert(text, &mut measure);
        let slot = &fields.slots[0];
        let shown = measure(&text[slot.scroll..]).x;
        assert!(slot.scroll > 0, "the start scrolls out");
        assert!(shown <= fields.width - 2., "the caret end stays inside");
        assert_eq!(
            slot.field.position.x,
            slot.left - measure(&text[..slot.scroll]).x
        );
        // Home brings the start back into view.
        press(&mut fields, 36);
        assert_eq!(fields.slots[0].scroll, 0);
        assert_eq!(fields.slots[0].field.position.x, fields.slots[0].left);
    }
}
