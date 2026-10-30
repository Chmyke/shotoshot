//! ROLE: Paints the Settings window from its `Layout`: navigation, rows, cards, controls, the
//! pointer preview and the open language list.
//! INVARIANTS: The accent marks only what is on or chosen (rule R1 of the 2026-10-28 boards).
use super::*;
use crate::icon;
use crate::pointer;
use shotoshot_core::pointer::PULSE_MS;

pub const BACKGROUND: [u8; 3] = [0x20, 0x20, 0x20];
const CARD_FILL: [u8; 3] = [0x2b, 0x2b, 0x2b];
const INSET: [u8; 3] = [0x26, 0x26, 0x26];
const SELECTED: [u8; 3] = [0x2d, 0x2d, 0x2d];
const FIELD: [u8; 3] = [0x1d, 0x1d, 0x1d];
const CHOSEN: [u8; 3] = [0x3d, 0x3d, 0x3d];
const CAP: [u8; 3] = [0x36, 0x36, 0x36];
const CAP_EDGE: [u8; 3] = [0x5e, 0x5e, 0x5e];
const BUTTON: [u8; 3] = [0x37, 0x37, 0x37];
const BUTTON_EDGE: [u8; 3] = [0x48, 0x48, 0x48];
const ICON_INK: [u8; 3] = [0xd6, 0xd6, 0xd6];
const TEXT: [u8; 3] = [0xf0, 0xf0, 0xf0];
const SECONDARY: [u8; 3] = [0xc6, 0xc6, 0xc6];
const MUTED: [u8; 3] = [0x9a, 0x9a, 0x9a];
const RULE: [u8; 3] = [0x49, 0x49, 0x49];
const ACCENT: [u8; 3] = [0x60, 0xcd, 0xff];
/// A shortcut another program holds.
const WARN: [u8; 3] = [0xf0, 0xa0, 0x4b];
/// The two halves of the preview: a dark editor and a light page.
const PREVIEW_DARK: [u8; 3] = [0x1b, 0x1e, 0x24];
const PREVIEW_LIGHT: [u8; 3] = [0xf3, 0xf4, 0xf6];
const RADIUS: f64 = 6.;

impl Panel {
    pub fn paint<C: Canvas>(&self, canvas: &mut C, scale: f64, now: u64) -> Result<(), C::Error> {
        let zones = &self.layout;
        let px = |r: Rect| Rect {
            min: r.min * scale,
            max: r.max * scale,
        };
        let u = |value: f64| value * scale;
        canvas.fill_rect(
            px(area(0., 0., zones.size.x, zones.size.y)),
            Ink::solid(BACKGROUND),
        )?;
        for (page, r) in &zones.nav {
            let r = px(*r);
            if *page == self.page {
                canvas.fill_round_rect(r, u(5.), Ink::solid(SELECTED))?;
                let middle = r.center().y;
                canvas.fill_round_rect(
                    Rect {
                        min: Point {
                            x: r.min.x,
                            y: middle - u(8.),
                        },
                        max: Point {
                            x: r.min.x + u(3.),
                            y: middle + u(8.),
                        },
                    },
                    u(1.5),
                    Ink::solid(ACCENT),
                )?;
            }
            icon::draw(
                canvas,
                page.icon(),
                Point {
                    x: r.min.x + u(NAV_TEXT),
                    y: r.center().y - u(NAV_ICON / 2.),
                },
                u(NAV_ICON / 20.),
                ICON_INK,
            )?;
            let text_area = Rect {
                min: Point {
                    x: r.min.x + u(2. * NAV_TEXT + NAV_ICON),
                    y: r.min.y,
                },
                max: r.max,
            };
            left_text(
                canvas,
                self.text(page.name()),
                text_area,
                u(LABEL_SIZE),
                TextStyle::Body,
                TEXT,
            )?;
        }
        left_text(
            canvas,
            self.text(self.page.name()),
            px(zones.title),
            u(TITLE_SIZE),
            TextStyle::Title,
            TEXT,
        )?;
        for (key, r) in &zones.groups {
            let r = px(*r);
            let low = Rect {
                min: Point {
                    x: r.min.x,
                    y: r.center().y,
                },
                max: r.max,
            };
            left_text(
                canvas,
                self.text(*key),
                low,
                u(LABEL_SIZE),
                TextStyle::Strong,
                TEXT,
            )?;
        }
        for block in &zones.blocks {
            canvas.fill_round_rect(px(*block), u(RADIUS), Ink::solid(CARD_FILL))?;
        }
        for inset in &zones.insets {
            let r = px(*inset);
            canvas.fill_round_rect(r, u(RADIUS), Ink::solid(INSET))?;
            // Square at the top, where the card's header continues it.
            canvas.fill_rect(
                Rect {
                    min: r.min,
                    max: Point {
                        x: r.max.x,
                        y: r.min.y + u(RADIUS),
                    },
                },
                Ink::solid(INSET),
            )?;
        }
        for zone in &zones.zones {
            self.paint_zone(canvas, zone, scale)?;
        }
        if self.page == Page::Pointer {
            self.paint_preview(canvas, scale, now)?;
        }
        if self.keyboard {
            let focused = match self.focus {
                Item::Nav(page) => zones.nav.iter().find(|(p, _)| *p == page).map(|(_, r)| *r),
                item => zones.zone(item).map(|zone| zone.row),
            };
            if let Some(r) = focused {
                canvas.stroke_round_rect(px(r), u(RADIUS), Ink::solid(ACCENT), u(1.5))?;
            }
        }
        Ok(())
    }

    fn paint_zone<C: Canvas>(
        &self,
        canvas: &mut C,
        zone: &Zone,
        scale: f64,
    ) -> Result<(), C::Error> {
        let u = |value: f64| value * scale;
        let px = |r: Rect| Rect {
            min: r.min * scale,
            max: r.max * scale,
        };
        let row = px(zone.row);
        let control = px(zone.control);
        let label_area = Rect {
            min: Point {
                x: row.min.x + u(zone.item.label_left()),
                y: row.min.y,
            },
            max: Point {
                x: control.min.x,
                y: row.max.y,
            },
        };
        let label = self.label(zone.item);
        // As in the menu, the eye of Show board is struck through while the board is hidden.
        let glyph = match zone.item.icon() {
            Icon::Eye if !self.is_shown(Shown::Board) => Icon::EyeOff,
            glyph => glyph,
        };
        if glyph != Icon::None {
            icon::draw(
                canvas,
                glyph,
                Point {
                    x: row.min.x + u(ROW_PAD),
                    y: row.center().y - u(ICON / 2.),
                },
                scale,
                ICON_INK,
            )?;
        }
        match zone.item {
            Item::Reset => {
                return left_text(canvas, &label, row, u(LABEL_SIZE), TextStyle::Body, ACCENT);
            }
            Item::Card(card) => {
                two_lines(
                    canvas,
                    (&label, LABEL_SIZE, TEXT),
                    (&self.summary(card), NOTE_SIZE, MUTED),
                    label_area,
                    scale,
                )?;
            }
            Item::Shortcut(row_item) if self.values.shortcuts[row_item.index()].in_use => {
                two_lines(
                    canvas,
                    (&label, LABEL_SIZE, TEXT),
                    (self.text(Key::SettingsInUse), NOTE_SIZE, WARN),
                    label_area,
                    scale,
                )?;
            }
            item if item.note().is_some() => {
                let note = item.note().map_or("", |key| self.text(key));
                two_lines(
                    canvas,
                    (&label, LABEL_SIZE, TEXT),
                    (note, NOTE_SIZE, SECONDARY),
                    label_area,
                    scale,
                )?;
            }
            Item::Field(_) => {
                left_text(
                    canvas,
                    &label,
                    label_area,
                    u(LABEL_SIZE),
                    TextStyle::Body,
                    SECONDARY,
                )?;
            }
            _ => left_text(
                canvas,
                &label,
                label_area,
                u(LABEL_SIZE),
                TextStyle::Body,
                TEXT,
            )?,
        }
        match zone.item {
            Item::Startup => switch(canvas, control, self.values.startup, scale),
            Item::Live => switch(canvas, control, self.values.live, scale),
            Item::Shortcut(row_item) => self.paint_keys(canvas, row_item, control, scale),
            Item::InputScale | Item::Duration => {
                self.paint_segments(canvas, zone.item, control, scale)
            }
            Item::MoveInput => switch(canvas, control, self.values.moving_input, scale),
            Item::Shown(shown) => switch(canvas, control, self.is_shown(shown), scale),
            Item::Card(card) => {
                chevron_vertical(
                    canvas,
                    Point {
                        x: control.min.x + u(CHEVRON / 2.),
                        y: control.center().y,
                    },
                    self.open == Some(card),
                    scale,
                )?;
                switch(canvas, control, self.card_on(card), scale)
            }
            Item::Field(field) => self.paint_field(canvas, field, control, scale),
            Item::Nav(_) | Item::Reset => Ok(()),
        }
    }

    fn paint_keys<C: Canvas>(
        &self,
        canvas: &mut C,
        row: ShortcutRow,
        r: Rect,
        scale: f64,
    ) -> Result<(), C::Error> {
        let u = |value: f64| value * scale;
        let keys = &self.values.shortcuts[row.index()];
        let clear = row.pointer_index().is_some() && !keys.caps.is_empty();
        let right = if row.pointer_index().is_some() {
            r.max.x - u(CLEAR)
        } else {
            r.max.x
        };
        if keys.caps.is_empty() {
            // A plain button: the accent marks only what is on (rule R1).
            let middle = r.center().y;
            let button = Rect {
                min: Point {
                    x: right - u(self.add_width()),
                    y: middle - u(BUTTON_HEIGHT / 2.),
                },
                max: Point {
                    x: right,
                    y: middle + u(BUTTON_HEIGHT / 2.),
                },
            };
            canvas.fill_round_rect(button, u(5.), Ink::solid(BUTTON))?;
            canvas.stroke_round_rect(
                button.expanded(-u(0.5)),
                u(5.),
                Ink::solid(BUTTON_EDGE),
                u(1.),
            )?;
            return centered(
                canvas,
                self.text(Key::SettingsAdd),
                button,
                u(LABEL_SIZE),
                TextStyle::Body,
                TEXT,
            );
        }
        keycaps(
            canvas,
            &keys.caps,
            right,
            r.center().y,
            11.,
            scale,
            keys.in_use,
        )?;
        if clear {
            let c = Point {
                x: r.max.x - u(CLEAR / 2.) + u(2.),
                y: r.center().y,
            };
            let arm = u(4.);
            for (a, b) in [
                (Point { x: -arm, y: -arm }, Point { x: arm, y: arm }),
                (Point { x: -arm, y: arm }, Point { x: arm, y: -arm }),
            ] {
                canvas.line(c + a, c + b, Ink::solid(MUTED), u(1.5))?;
            }
        }
        Ok(())
    }

    fn paint_segments<C: Canvas>(
        &self,
        canvas: &mut C,
        item: Item,
        r: Rect,
        scale: f64,
    ) -> Result<(), C::Error> {
        let u = |value: f64| value * scale;
        canvas.fill_round_rect(r, u(RADIUS), Ink::solid(FIELD))?;
        canvas.stroke_round_rect(r.expanded(-u(0.5)), u(RADIUS), Ink::solid(RULE), u(1.))?;
        let current = self.segment(item);
        let mut x = r.min.x + u(2.);
        for (index, (name, width)) in self
            .segment_names(item)
            .iter()
            .zip(self.segment_widths(item))
            .enumerate()
        {
            let cell = Rect {
                min: Point {
                    x,
                    y: r.min.y + u(2.),
                },
                max: Point {
                    x: x + u(width),
                    y: r.max.y - u(2.),
                },
            };
            let chosen = current == Some(index);
            if chosen {
                canvas.fill_round_rect(cell, u(4.), Ink::solid(CHOSEN))?;
                canvas.line(
                    Point {
                        x: cell.min.x + u(8.),
                        y: cell.max.y - u(1.),
                    },
                    Point {
                        x: cell.max.x - u(8.),
                        y: cell.max.y - u(1.),
                    },
                    Ink::solid(ACCENT),
                    u(2.),
                )?;
            }
            centered(
                canvas,
                name,
                cell,
                u(SMALL_SIZE),
                TextStyle::Body,
                if chosen { TEXT } else { SECONDARY },
            )?;
            x += u(width);
        }
        Ok(())
    }

    fn paint_field<C: Canvas>(
        &self,
        canvas: &mut C,
        field: Field,
        r: Rect,
        scale: f64,
    ) -> Result<(), C::Error> {
        let u = |value: f64| value * scale;
        match field.control() {
            Control::Segments => self.paint_segments(canvas, Item::Field(field), r, scale),
            Control::Slider => {
                let ((low, high), _) = field.range();
                let value = self.number(field);
                let fraction = match self.drag {
                    Some((dragged, fraction)) if dragged == field => fraction,
                    _ => (value - low) as f64 / (high - low).max(1) as f64,
                };
                let left = r.min.x + u(8.);
                let right = r.max.x - u(8.);
                let knob = left + (right - left) * fraction;
                let middle = r.center().y;
                canvas.line(
                    Point { x: left, y: middle },
                    Point {
                        x: right,
                        y: middle,
                    },
                    Ink::solid(RULE),
                    u(4.),
                )?;
                canvas.line(
                    Point { x: left, y: middle },
                    Point { x: knob, y: middle },
                    Ink::solid(ACCENT),
                    u(4.),
                )?;
                canvas.fill_ellipse(
                    Point { x: knob, y: middle },
                    Point { x: u(8.), y: u(8.) },
                    Ink::solid(CHOSEN),
                )?;
                canvas.fill_ellipse(
                    Point { x: knob, y: middle },
                    Point { x: u(5.), y: u(5.) },
                    Ink::solid(TEXT),
                )
            }
            Control::Colors => {
                let current = self.color(field);
                for (index, rgb) in COLORS.iter().enumerate() {
                    let center = swatch_center(r, index, scale);
                    canvas.fill_ellipse(
                        center,
                        Point {
                            x: u(SWATCH),
                            y: u(SWATCH),
                        },
                        Ink::solid(*rgb),
                    )?;
                    if *rgb == current {
                        canvas.stroke_ellipse(
                            center,
                            Point {
                                x: u(SWATCH + 3.),
                                y: u(SWATCH + 3.),
                            },
                            Ink::solid(TEXT),
                            u(1.5),
                        )?;
                    }
                }
                Ok(())
            }
            Control::Switch => switch(canvas, r, self.switch(field), scale),
        }
    }

    fn paint_preview<C: Canvas>(
        &self,
        canvas: &mut C,
        scale: f64,
        now: u64,
    ) -> Result<(), C::Error> {
        let effect_scale = scale * PREVIEW_SCALE;
        let pulse = now.saturating_sub(self.last_click);
        let halo = pointer::HaloState {
            color: self.halo_color,
            pressed: pulse < 150,
            pulse: (pulse < PULSE_MS).then(|| pulse as f64 / PULSE_MS as f64),
        };
        let r = Rect {
            min: self.layout.preview.min * scale,
            max: self.layout.preview.max * scale,
        };
        // One page, dark on the left and light on the right: the effects cross both.
        canvas.fill_round_rect(r, scale * RADIUS, Ink::solid(PREVIEW_LIGHT))?;
        let half = Rect {
            min: r.min,
            max: Point {
                x: r.center().x,
                y: r.max.y,
            },
        };
        canvas.fill_round_rect(half, scale * RADIUS, Ink::solid(PREVIEW_DARK))?;
        canvas.fill_rect(
            Rect {
                min: Point {
                    x: half.max.x - scale * RADIUS,
                    y: half.min.y,
                },
                max: half.max,
            },
            Ink::solid(PREVIEW_DARK),
        )?;
        // Lines of "text" on the page, so that the spotlight and the trail have something to
        // cover.
        let middle = r.center().x;
        for (left, right, ink) in [
            (r.min.x, middle, Ink::with_alpha([0x8a, 0x94, 0xa6], 0.5)),
            (middle, r.max.x, Ink::with_alpha([0x6b, 0x72, 0x80], 0.35)),
        ] {
            let mut y = r.min.y + scale * 22.;
            let mut line = 0;
            while y < r.max.y - scale * 14. {
                let length = [0.78, 0.55, 0.9, 0.66, 0.42][line % 5];
                let from = left + scale * 14.;
                let to = from + (right - left - scale * 28.) * length;
                canvas.line(Point { x: from, y }, Point { x: to, y }, ink, scale * 4.)?;
                y += scale * 18.;
                line += 1;
            }
        }
        // Preview positions are offsets from the middle, in logical pixels.
        let center = r.center();
        let to_canvas = |p: Point| center + p * effect_scale;
        if !self.halo_on {
            return Ok(());
        }
        if let Some(at) = self.preview.shown() {
            pointer::spotlight(canvas, r, to_canvas(at), &self.settings, effect_scale)?;
        }
        let placed = self.preview.mapped(to_canvas);
        pointer::paint(
            canvas,
            Point { x: 0., y: 0. },
            &placed,
            &self.settings,
            halo,
            effect_scale,
            now,
        )
    }
}

/// The logical width of a row of keycaps whose texts measure `widths` at `SMALL_SIZE`.
pub fn keycaps_width(widths: impl IntoIterator<Item = f64>) -> f64 {
    let mut total = 0.;
    let mut count = 0;
    for width in widths {
        total += width.max(10.) + 2. * CAP_PAD;
        count += 1;
    }
    total + CAP_GAP * (count.max(1) - 1) as f64
}
/// Draws `caps` as keycaps ending at `right` and centred on `middle`, in device pixels, each
/// `half_height` logical pixels above and below it; `in_use` when another program holds them.
pub fn keycaps<C: Canvas>(
    canvas: &mut C,
    caps: &[String],
    right: f64,
    middle: f64,
    half_height: f64,
    scale: f64,
    in_use: bool,
) -> Result<(), C::Error> {
    let u = |value: f64| value * scale;
    let ink = if in_use { WARN } else { CAP_EDGE };
    let mut widths = Vec::with_capacity(caps.len());
    for cap in caps {
        let text = canvas
            .measure_styled_text(cap, u(SMALL_SIZE), TextStyle::Body)?
            .x;
        widths.push(text.max(u(10.)) + u(2. * CAP_PAD));
    }
    let total = widths.iter().sum::<f64>() + u(CAP_GAP) * (widths.len().max(1) - 1) as f64;
    let mut x = right - total;
    for (cap, width) in caps.iter().zip(widths) {
        let key = Rect {
            min: Point {
                x,
                y: middle - u(half_height),
            },
            max: Point {
                x: x + width,
                y: middle + u(half_height),
            },
        };
        canvas.fill_round_rect(key, u(4.), Ink::solid(CAP))?;
        canvas.stroke_round_rect(key.expanded(-u(0.5)), u(4.), Ink::solid(ink), u(1.))?;
        // The thicker bottom edge of a keycap.
        canvas.line(
            Point {
                x: key.min.x + u(3.),
                y: key.max.y - u(0.5),
            },
            Point {
                x: key.max.x - u(3.),
                y: key.max.y - u(0.5),
            },
            Ink::solid(ink),
            u(1.5),
        )?;
        centered(
            canvas,
            cap,
            key,
            u(SMALL_SIZE),
            TextStyle::Body,
            if in_use { WARN } else { TEXT },
        )?;
        x += width + u(CAP_GAP);
    }
    Ok(())
}
fn switch<C: Canvas>(canvas: &mut C, r: Rect, on: bool, scale: f64) -> Result<(), C::Error> {
    let u = |value: f64| value * scale;
    let middle = r.center().y;
    let track = Rect {
        min: Point {
            x: r.max.x - u(SWITCH.x),
            y: middle - u(SWITCH.y / 2.),
        },
        max: Point {
            x: r.max.x,
            y: middle + u(SWITCH.y / 2.),
        },
    };
    if on {
        canvas.fill_round_rect(track, u(SWITCH.y / 2.), Ink::solid(ACCENT))?;
        canvas.fill_ellipse(
            Point {
                x: track.max.x - u(SWITCH.y / 2.),
                y: middle,
            },
            Point { x: u(5.), y: u(5.) },
            Ink::solid(BACKGROUND),
        )
    } else {
        canvas.stroke_round_rect(
            track.expanded(-u(0.5)),
            u(SWITCH.y / 2. - 0.5),
            Ink::solid(MUTED),
            u(1.),
        )?;
        canvas.fill_ellipse(
            Point {
                x: track.min.x + u(SWITCH.y / 2.),
                y: middle,
            },
            Point {
                x: u(4.5),
                y: u(4.5),
            },
            Ink::solid(SECONDARY),
        )
    }
}
/// A small arrow pointing down: a card to open.
fn chevron<C: Canvas>(canvas: &mut C, at: Point, scale: f64) -> Result<(), C::Error> {
    let arm = 4. * scale;
    let points = [
        at + Point {
            x: -arm,
            y: -arm / 2.,
        },
        at + Point { x: 0., y: arm / 2. },
        at + Point {
            x: arm,
            y: -arm / 2.,
        },
    ];
    canvas.polyline(&points, false, Ink::solid(SECONDARY), 1.5 * scale)
}
/// The arrow of a card: up when it is open, down when it is folded.
fn chevron_vertical<C: Canvas>(
    canvas: &mut C,
    at: Point,
    up: bool,
    scale: f64,
) -> Result<(), C::Error> {
    if !up {
        return chevron(canvas, at, scale);
    }
    let arm = 4. * scale;
    canvas.polyline(
        &[
            at + Point {
                x: -arm,
                y: arm / 2.,
            },
            at + Point {
                x: 0.,
                y: -arm / 2.,
            },
            at + Point {
                x: arm,
                y: arm / 2.,
            },
        ],
        false,
        Ink::solid(SECONDARY),
        1.5 * scale,
    )
}
/// A text line vertically centred in `r`, starting at its left edge.
fn left_text<C: Canvas>(
    canvas: &mut C,
    text: &str,
    r: Rect,
    size: f64,
    style: TextStyle,
    rgb: [u8; 3],
) -> Result<(), C::Error> {
    let extent = canvas.measure_styled_text(text, size, style)?;
    let top = r.center().y - extent.y / 2.;
    canvas.styled_text(
        text,
        Rect {
            min: Point { x: r.min.x, y: top },
            // Measuring and drawing round differently: never wrap or clip the last glyph.
            max: Point {
                x: r.min.x + extent.x + size,
                y: top + extent.y * 2.,
            },
        },
        size,
        Ink::solid(rgb),
        style,
    )
}
/// A name with a smaller explanation under it, both centred in `r` together.
fn two_lines<C: Canvas>(
    canvas: &mut C,
    (first, first_size, first_ink): (&str, f64, [u8; 3]),
    (second, second_size, second_ink): (&str, f64, [u8; 3]),
    r: Rect,
    scale: f64,
) -> Result<(), C::Error> {
    let a = canvas.measure_styled_text(first, first_size * scale, TextStyle::Body)?;
    let b = canvas.measure_styled_text(second, second_size * scale, TextStyle::Caption)?;
    let top = r.center().y - (a.y + b.y) / 2.;
    let line = |y: f64, extent: Point| Rect {
        min: Point { x: r.min.x, y },
        max: Point {
            x: r.min.x,
            y: y + extent.y,
        },
    };
    let first_area = line(top, a);
    left_text(
        canvas,
        first,
        Rect {
            max: Point {
                x: first_area.max.x,
                y: first_area.max.y,
            },
            ..first_area
        },
        first_size * scale,
        TextStyle::Body,
        first_ink,
    )?;
    left_text(
        canvas,
        second,
        line(top + a.y, b),
        second_size * scale,
        TextStyle::Caption,
        second_ink,
    )
}
fn centered<C: Canvas>(
    canvas: &mut C,
    text: &str,
    r: Rect,
    size: f64,
    style: TextStyle,
    rgb: [u8; 3],
) -> Result<(), C::Error> {
    let extent = canvas.measure_styled_text(text, size, style)?;
    let min = r.center() - extent * 0.5;
    canvas.styled_text(
        text,
        Rect {
            min,
            max: min + extent + Point { x: size, y: 0. },
        },
        size,
        Ink::solid(rgb),
        style,
    )
}

#[cfg(test)]
mod contrast_tests {
    use super::*;

    /// WCAG contrast ratio of two sRGB colours.
    fn contrast(a: [u8; 3], b: [u8; 3]) -> f64 {
        let luminance = |rgb: [u8; 3]| {
            let channel = |c: u8| {
                let c = c as f64 / 255.;
                if c <= 0.03928 {
                    c / 12.92
                } else {
                    ((c + 0.055) / 1.055).powf(2.4)
                }
            };
            0.2126 * channel(rgb[0]) + 0.7152 * channel(rgb[1]) + 0.0722 * channel(rgb[2])
        };
        let (light, dark) = {
            let (x, y) = (luminance(a), luminance(b));
            (x.max(y), x.min(y))
        };
        (light + 0.05) / (dark + 0.05)
    }

    #[test]
    fn every_text_reads_on_its_background() {
        // Labels, explanations, card summaries and warnings on rows; names on the window.
        for ink in [TEXT, SECONDARY, MUTED, WARN, ICON_INK] {
            for ground in [CARD_FILL, INSET] {
                let ratio = contrast(ink, ground);
                assert!(ratio >= 4.5, "{ink:?} on {ground:?}: {ratio:.2}");
            }
        }
        assert!(contrast(TEXT, BUTTON) >= 4.5);
        assert!(contrast(TEXT, BACKGROUND) >= 4.5);
        assert!(contrast(ACCENT, BACKGROUND) >= 4.5);
    }
}
