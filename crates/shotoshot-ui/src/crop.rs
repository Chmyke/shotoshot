//! ROLE: Validates arithmetic dimension input and resizes a crop without moving its origin.
use shotoshot_core::{Point, Rect};

const INVALID: &str =
    "Enter a whole number of pixels (at least 2), or an expression such as 800+20.";

pub fn dimension(expression: &str) -> Result<u32, &'static str> {
    if expression.len() > 128 {
        return Err(INVALID);
    }
    let mut parser = Parser {
        bytes: expression.as_bytes(),
        cursor: 0,
    };
    let value = parser.sum(0)?;
    parser.spaces();
    if parser.cursor != parser.bytes.len()
        || !value.is_finite()
        || value < 2.
        || value > u32::MAX as f64
        || value.fract().abs() > 1e-9
    {
        return Err(INVALID);
    }
    Ok(value.round() as u32)
}

pub fn resize_region(
    region: Rect,
    width: &str,
    height: &str,
    screen_width: u32,
    screen_height: u32,
) -> Result<Rect, &'static str> {
    let size = Point {
        x: dimension(width)? as f64,
        y: dimension(height)? as f64,
    };
    let max = region.min + size;
    if !region.min.finite()
        || region.min.x < 0.
        || region.min.y < 0.
        || max.x > screen_width as f64
        || max.y > screen_height as f64
    {
        return Err("The capture must stay inside the desktop. Reduce its width or height.");
    }
    Ok(Rect {
        min: region.min,
        max,
    })
}

struct Parser<'a> {
    bytes: &'a [u8],
    cursor: usize,
}
impl Parser<'_> {
    fn spaces(&mut self) {
        while self
            .bytes
            .get(self.cursor)
            .is_some_and(u8::is_ascii_whitespace)
        {
            self.cursor += 1;
        }
    }
    fn take(&mut self, byte: u8) -> bool {
        self.spaces();
        if self.bytes.get(self.cursor) == Some(&byte) {
            self.cursor += 1;
            true
        } else {
            false
        }
    }
    fn sum(&mut self, depth: usize) -> Result<f64, &'static str> {
        let mut value = self.product(depth)?;
        loop {
            if self.take(b'+') {
                value += self.product(depth)?;
            } else if self.take(b'-') {
                value -= self.product(depth)?;
            } else {
                return Ok(value);
            }
        }
    }
    fn product(&mut self, depth: usize) -> Result<f64, &'static str> {
        let mut value = self.atom(depth)?;
        loop {
            if self.take(b'*') {
                value *= self.atom(depth)?;
            } else if self.take(b'/') {
                let divisor = self.atom(depth)?;
                if divisor == 0. {
                    return Err(INVALID);
                }
                value /= divisor;
            } else {
                return Ok(value);
            }
        }
    }
    fn atom(&mut self, depth: usize) -> Result<f64, &'static str> {
        if depth >= 16 {
            return Err(INVALID);
        }
        if self.take(b'+') {
            return self.atom(depth + 1);
        }
        if self.take(b'-') {
            return Ok(-self.atom(depth + 1)?);
        }
        if self.take(b'(') {
            let value = self.sum(depth + 1)?;
            return if self.take(b')') {
                Ok(value)
            } else {
                Err(INVALID)
            };
        }
        self.spaces();
        let start = self.cursor;
        while self
            .bytes
            .get(self.cursor)
            .is_some_and(|c| c.is_ascii_digit() || *c == b'.' || *c == b',')
        {
            self.cursor += 1;
        }
        let number = std::str::from_utf8(&self.bytes[start..self.cursor]).map_err(|_| INVALID)?;
        number.replace(',', ".").parse::<f64>().map_err(|_| INVALID)
    }
}
