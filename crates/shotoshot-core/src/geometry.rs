use std::ops::{Add, Mul, Sub};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}
impl Point {
    pub fn length(self) -> f64 {
        self.x.hypot(self.y)
    }
    pub fn finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite()
    }
}
impl Add for Point {
    type Output = Self;
    fn add(self, b: Self) -> Self {
        Self {
            x: self.x + b.x,
            y: self.y + b.y,
        }
    }
}
impl Sub for Point {
    type Output = Self;
    fn sub(self, b: Self) -> Self {
        Self {
            x: self.x - b.x,
            y: self.y - b.y,
        }
    }
}
impl Mul<f64> for Point {
    type Output = Self;
    fn mul(self, b: f64) -> Self {
        Self {
            x: self.x * b,
            y: self.y * b,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub min: Point,
    pub max: Point,
}
impl Rect {
    pub fn new(a: Point, b: Point) -> Option<Self> {
        if !a.finite() || !b.finite() {
            return None;
        }
        Some(Self {
            min: Point {
                x: a.x.min(b.x),
                y: a.y.min(b.y),
            },
            max: Point {
                x: a.x.max(b.x),
                y: a.y.max(b.y),
            },
        })
    }
    pub fn size(self) -> Point {
        self.max - self.min
    }
    pub fn center(self) -> Point {
        (self.min + self.max) * 0.5
    }
    pub fn contains(self, p: Point) -> bool {
        p.x >= self.min.x && p.x <= self.max.x && p.y >= self.min.y && p.y <= self.max.y
    }
    pub fn contains_rect(self, r: Self) -> bool {
        self.contains(r.min) && self.contains(r.max)
    }
    pub fn expanded(self, n: f64) -> Self {
        Self {
            min: self.min - Point { x: n, y: n },
            max: self.max + Point { x: n, y: n },
        }
    }
    pub fn union(self, r: Self) -> Self {
        Self {
            min: Point {
                x: self.min.x.min(r.min.x),
                y: self.min.y.min(r.min.y),
            },
            max: Point {
                x: self.max.x.max(r.max.x),
                y: self.max.y.max(r.max.y),
            },
        }
    }
    pub fn corners(self) -> [Point; 4] {
        [
            self.min,
            Point {
                x: self.max.x,
                y: self.min.y,
            },
            self.max,
            Point {
                x: self.min.x,
                y: self.max.y,
            },
        ]
    }
}

pub fn segment_distance(p: Point, a: Point, b: Point) -> f64 {
    let delta = b - a;
    let squared = delta.x * delta.x + delta.y * delta.y;
    if squared < f64::EPSILON {
        return (p - a).length();
    }
    let t = (((p.x - a.x) * delta.x + (p.y - a.y) * delta.y) / squared).clamp(0., 1.);
    (p - (a + delta * t)).length()
}

/// Rotate in document coordinates, whose positive Y points downward.
pub fn rotate_point(p: Point, center: Point, angle: f64) -> Point {
    let d = p - center;
    let (sin, cos) = angle.sin_cos();
    center
        + Point {
            x: d.x * cos - d.y * sin,
            y: d.x * sin + d.y * cos,
        }
}
pub fn snap_angle(origin: Point, p: Point) -> Point {
    let delta = p - origin;
    let step = std::f64::consts::FRAC_PI_4;
    let angle = (delta.y.atan2(delta.x) / step).round() * step;
    origin
        + Point {
            x: angle.cos(),
            y: angle.sin(),
        } * delta.length()
}
