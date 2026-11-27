//! d3-shape's line generator.

use super::curve::{Curve, CurveState};
use super::path::ChartPath;

/// A per-datum accessor: the datum and its index.
pub(crate) type Accessor<'a, T> = Box<dyn Fn(&T, usize) -> f64 + 'a>;

/// A per-datum test: the datum and its index.
pub(crate) type Predicate<'a, T> = Box<dyn Fn(&T, usize) -> bool + 'a>;

/// A line generator (d3-shape `line()`): joins each datum's point with a curve,
/// breaking the line where `defined` is false.
///
/// d3 reads x and y as `d[0]` and `d[1]` until they are set; a Rust datum has no
/// index, so they read NaN, as d3 reads them from an object without those keys.
pub struct Line<'a, T> {
    x: Accessor<'a, T>,
    y: Accessor<'a, T>,
    defined: Predicate<'a, T>,
    curve: Curve,
}

impl<T> Default for Line<'_, T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a, T> Line<'a, T> {
    /// A linear line with every datum defined (d3-shape `line()`).
    pub fn new() -> Self {
        Self {
            x: Box::new(|_, _| f64::NAN),
            y: Box::new(|_, _| f64::NAN),
            defined: Box::new(|_, _| true),
            curve: Curve::Linear,
        }
    }

    /// Sets the x accessor (d3-shape `line.x(x)`).
    pub fn x(mut self, x: impl Fn(&T, usize) -> f64 + 'a) -> Self {
        self.x = Box::new(x);
        self
    }

    /// Sets the y accessor (d3-shape `line.y(y)`).
    pub fn y(mut self, y: impl Fn(&T, usize) -> f64 + 'a) -> Self {
        self.y = Box::new(y);
        self
    }

    /// Sets which data have a point; the line breaks around the others (d3-shape
    /// `line.defined(defined)`).
    pub fn defined(mut self, defined: impl Fn(&T, usize) -> bool + 'a) -> Self {
        self.defined = Box::new(defined);
        self
    }

    /// Sets the curve (d3-shape `line.curve(curve)`).
    pub fn curve(mut self, curve: Curve) -> Self {
        self.curve = curve;
        self
    }

    /// The line through `data` (d3-shape `line(data)`). An empty path stands for d3's
    /// `null`, which it returns when nothing is drawn.
    pub fn generate(&self, data: &[T]) -> ChartPath {
        let n = data.len();
        let mut path = ChartPath::new();
        let mut output = CurveState::new(self.curve);
        let mut defined0 = false;
        // One step past the end closes the last run.
        for i in 0..=n {
            let d = data.get(i);
            let defined = d.is_some_and(|d| (self.defined)(d, i));
            if defined != defined0 {
                defined0 = defined;
                if defined0 {
                    output.line_start();
                } else {
                    output.line_end(&mut path);
                }
            }
            if let Some(d) = d.filter(|_| defined0) {
                let x = (self.x)(d, i);
                let y = (self.y)(d, i);
                output.point(&mut path, x, y);
            }
        }
        path
    }
}
