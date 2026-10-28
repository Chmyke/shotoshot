//! ROLE: The rounded panel a text can sit on: margins, corners, ink and line breaks.
//! OWNER: Text objects (`Shape::Text::label`); renderers and text fields read it here.
//! INVARIANTS: Margins and corners follow the font size, so the wheel keeps the
//! proportions; the ink follows the panel colour and is never stored.
use crate::Point;
use std::ops::Range;

/// How a text sits on its panel.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Label {
    /// The panel hugs the text; lines break only where Enter was pressed.
    Fit,
    /// Lines also break at this width, in image pixels; the panel is as wide plus its
    /// margins, whatever the text.
    Wrap(f64),
}
impl Label {
    /// Space between the text and the panel's edges, at font `size`.
    pub fn padding(size: f64) -> Point {
        Point {
            x: size * 0.4,
            y: size * 0.25,
        }
    }
    /// Radius of the panel's corners at font `size`.
    pub fn radius(size: f64) -> f64 {
        size * 0.25
    }
    /// The narrowest line a wrapped text keeps at font `size`: about two letters.
    pub fn min_wrap(size: f64) -> f64 {
        size
    }
    pub fn wrap(self) -> Option<f64> {
        match self {
            Self::Fit => None,
            Self::Wrap(width) => Some(width),
        }
    }
}

/// Black or white, whichever contrasts more with `panel` (WCAG relative luminance).
pub fn label_ink(panel: [u8; 3]) -> [u8; 3] {
    let linear = |c: u8| {
        let c = c as f64 / 255.;
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    let [r, g, b] = panel.map(linear);
    let luminance = 0.2126 * r + 0.7152 * g + 0.0722 * b;
    // Contrast with black, (L + 0.05) / 0.05, beats contrast with white, 1.05 / (L + 0.05).
    if (luminance + 0.05).powi(2) > 0.0525 {
        [0; 3]
    } else {
        [255; 3]
    }
}

/// Byte ranges of the lines `text` is drawn on: one per typed line, each broken further
/// when `wrap` is given and it runs wider, after a space when a word fits, inside a word
/// too long for any line. A line keeps the spaces it ends with; `width` measures a piece
/// of one line without them.
pub fn text_lines(
    text: &str,
    wrap: Option<f64>,
    width: &mut impl FnMut(&str) -> f64,
) -> Vec<Range<usize>> {
    let mut lines = Vec::new();
    let mut start = 0;
    for typed in text.split('\n') {
        let end = start + typed.len();
        match wrap {
            Some(limit) => wrap_line(text, start..end, limit, width, &mut lines),
            None => lines.push(start..end),
        }
        start = end + 1;
    }
    lines
}

fn wrap_line(
    text: &str,
    line: Range<usize>,
    limit: f64,
    width: &mut impl FnMut(&str) -> f64,
    lines: &mut Vec<Range<usize>>,
) {
    let mut start = line.start;
    loop {
        // What is left fits, or is a single character that cannot be broken.
        let rest = text[start..line.end].trim_end();
        if rest.chars().nth(1).is_none() || width(rest) <= limit {
            lines.push(start..line.end);
            return;
        }
        let (mut after_space, mut fitting) = (None, None);
        let mut previous_space = false;
        for (i, c) in text[start..line.end].char_indices() {
            let at = start + i;
            if previous_space && !c.is_whitespace() {
                after_space = Some(at);
            }
            previous_space = c.is_whitespace();
            let end = at + c.len_utf8();
            if width(text[start..end].trim_end()) > limit {
                break;
            }
            fitting = Some(end);
        }
        // A word that fits moves whole; a longer one breaks where the line is full, after
        // one character at least so the text always moves on.
        let first = start + text[start..].chars().next().map_or(0, char::len_utf8);
        let end = after_space.or(fitting).unwrap_or(first).max(first);
        lines.push(start..end);
        start = end;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Monospace stand-in: 10 px per char.
    fn width(s: &str) -> f64 {
        s.chars().count() as f64 * 10.
    }
    fn lines(text: &str, wrap: Option<f64>) -> Vec<&str> {
        text_lines(text, wrap, &mut width)
            .into_iter()
            .map(|r| &text[r])
            .collect()
    }

    #[test]
    fn typed_lines_stay_and_nothing_wraps_without_a_width() {
        assert_eq!(lines("a long line\nb", None), ["a long line", "b"]);
        assert_eq!(lines("", None), [""]);
        assert_eq!(lines("x\n\ny", Some(100.)), ["x", "", "y"]);
    }
    #[test]
    fn words_move_whole_and_keep_their_space() {
        assert_eq!(
            lines("the quick brown fox", Some(100.)),
            ["the quick ", "brown fox"]
        );
        assert_eq!(lines("one two", Some(30.)), ["one ", "two"]);
        // Trailing spaces never push a line over.
        assert_eq!(lines("abc    ", Some(30.)), ["abc    "]);
    }
    #[test]
    fn a_word_longer_than_a_line_breaks_inside() {
        assert_eq!(lines("abcdefgh", Some(30.)), ["abc", "def", "gh"]);
        assert_eq!(lines("ab abcdefgh", Some(30.)), ["ab ", "abc", "def", "gh"]);
        // Not even one character fits: one per line, never an endless loop.
        assert_eq!(lines("abc", Some(5.)), ["a", "b", "c"]);
    }
    #[test]
    fn accents_and_crlf_stay_on_char_boundaries() {
        assert_eq!(lines("Ça déjà vu", Some(50.)), ["Ça ", "déjà ", "vu"]);
        assert_eq!(lines("éééé", Some(20.)), ["éé", "éé"]);
        assert_eq!(lines("ab cd\r\nef", Some(30.)), ["ab ", "cd\r", "ef"]);
    }
    #[test]
    fn the_ink_contrasts_with_the_panel() {
        assert_eq!(label_ink([255; 3]), [0; 3]);
        assert_eq!(label_ink([0; 3]), [255; 3]);
        assert_eq!(label_ink([250, 192, 66]), [0; 3], "yellow");
        assert_eq!(label_ink([28, 31, 40]), [255; 3], "near black");
        assert_eq!(label_ink([80, 153, 244]), [0; 3], "blue");
        assert_eq!(label_ink([0, 0, 180]), [255; 3], "deep blue");
    }
}
