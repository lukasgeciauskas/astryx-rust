//! d3-shape's area generator.

use super::curve::{Curve, CurveState};
use super::line::{Accessor, Predicate};
use super::path::ChartPath;

/// An area generator (d3-shape `area()`): the region between a topline (x1, y1) and a
/// baseline (x0, y0), each joined with a curve, broken where `defined` is false.
///
/// d3 reads x and y1 as `d[0]` and `d[1]` until they are set and y0 as 0; a Rust
/// datum has no index, so x and y1 read NaN, as d3 reads them from an object without
/// those keys.
pub struct Area<'a, T> {
    x0: Accessor<'a, T>,
    x1: Option<Accessor<'a, T>>,
    y0: Accessor<'a, T>,
    y1: Option<Accessor<'a, T>>,
    defined: Predicate<'a, T>,
    curve: Curve,
}

impl<T> Default for Area<'_, T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a, T> Area<'a, T> {
    /// A linear area on a zero baseline with every datum defined (d3-shape `area()`).
    pub fn new() -> Self {
        Self {
            x0: Box::new(|_, _| f64::NAN),
            x1: None,
            y0: Box::new(|_, _| 0.),
            y1: Some(Box::new(|_, _| f64::NAN)),
            defined: Box::new(|_, _| true),
            curve: Curve::Linear,
        }
    }

    /// Sets x for both lines (d3-shape `area.x(x)`: x0, with x1 cleared).
    pub fn x(mut self, x: impl Fn(&T, usize) -> f64 + 'a) -> Self {
        self.x0 = Box::new(x);
        self.x1 = None;
        self
    }

    /// Sets the baseline's x (d3-shape `area.x0(x)`).
    pub fn x0(mut self, x: impl Fn(&T, usize) -> f64 + 'a) -> Self {
        self.x0 = Box::new(x);
        self
    }

    /// Sets the topline's x (d3-shape `area.x1(x)`); without one the topline uses x0.
    pub fn x1(mut self, x: impl Fn(&T, usize) -> f64 + 'a) -> Self {
        self.x1 = Some(Box::new(x));
        self
    }

    /// Sets y for both lines (d3-shape `area.y(y)`: y0, with y1 cleared).
    pub fn y(mut self, y: impl Fn(&T, usize) -> f64 + 'a) -> Self {
        self.y0 = Box::new(y);
        self.y1 = None;
        self
    }

    /// Sets the baseline's y (d3-shape `area.y0(y)`).
    pub fn y0(mut self, y: impl Fn(&T, usize) -> f64 + 'a) -> Self {
        self.y0 = Box::new(y);
        self
    }

    /// Sets the topline's y (d3-shape `area.y1(y)`); without one the topline uses y0.
    pub fn y1(mut self, y: impl Fn(&T, usize) -> f64 + 'a) -> Self {
        self.y1 = Some(Box::new(y));
        self
    }

    /// Sets which data have a point; the area breaks around the others (d3-shape
    /// `area.defined(defined)`).
    pub fn defined(mut self, defined: impl Fn(&T, usize) -> bool + 'a) -> Self {
        self.defined = Box::new(defined);
        self
    }

    /// Sets the curve (d3-shape `area.curve(curve)`).
    pub fn curve(mut self, curve: Curve) -> Self {
        self.curve = curve;
        self
    }

    /// The area over `data` (d3-shape `area(data)`): for each run of defined data,
    /// the topline forward, then the baseline back, closed. An empty path stands for
    /// d3's `null`, which it returns when nothing is drawn.
    pub fn generate(&self, data: &[T]) -> ChartPath {
        let n = data.len();
        let mut path = ChartPath::new();
        let mut output = CurveState::new(self.curve);
        let mut defined0 = false;
        let mut j = 0;
        let mut x0z = vec![0.; n];
        let mut y0z = vec![0.; n];
        // One step past the end closes the last run.
        for i in 0..=n {
            let d = data.get(i);
            let defined = d.is_some_and(|d| (self.defined)(d, i));
            if defined != defined0 {
                defined0 = defined;
                if defined0 {
                    j = i;
                    output.area_start();
                    output.line_start();
                } else {
                    output.line_end(&mut path);
                    output.line_start();
                    for k in (j..i).rev() {
                        output.point(&mut path, x0z[k], y0z[k]);
                    }
                    output.line_end(&mut path);
                    output.area_end();
                }
            }
            if let Some(d) = d.filter(|_| defined0) {
                x0z[i] = (self.x0)(d, i);
                y0z[i] = (self.y0)(d, i);
                let x = self.x1.as_ref().map_or(x0z[i], |x1| x1(d, i));
                let y = self.y1.as_ref().map_or(y0z[i], |y1| y1(d, i));
                output.point(&mut path, x, y);
            }
        }
        path
    }
}
