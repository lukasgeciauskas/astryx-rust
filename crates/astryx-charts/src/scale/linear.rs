//! d3-scale's `scaleLinear` with a two-stop domain and range.

use astryx_theme::authoring::js::{js_max, js_min};

use super::number_format::{FormatSpecifier, NumberFormat, tick_format};
use super::ticks::{tick_increment, ticks};

/// A continuous linear scale from a domain to a range (d3-scale `scaleLinear`), built
/// up the way d3's chained setters are:
/// `LinearScale::new().domain(0., 100.).range(height, 0.).nice()`. [`Clone`] is d3's
/// `copy`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LinearScale {
    domain: [f64; 2],
    range: [f64; 2],
    clamp: bool,
}

impl Default for LinearScale {
    fn default() -> Self {
        Self::new()
    }
}

impl LinearScale {
    /// A scale with the domain [0, 1], the range [0, 1] and no clamping (d3-scale
    /// `scaleLinear()`).
    pub fn new() -> Self {
        Self {
            domain: [0., 1.],
            range: [0., 1.],
            clamp: false,
        }
    }

    /// Sets the domain (d3-scale `linear.domain([d0, d1])`).
    pub fn domain(mut self, d0: f64, d1: f64) -> Self {
        self.domain = [d0, d1];
        self
    }

    /// Sets the range (d3-scale `linear.range([r0, r1])`).
    pub fn range(mut self, r0: f64, r1: f64) -> Self {
        self.range = [r0, r1];
        self
    }

    /// Whether values outside the domain map to the range's ends instead of past them
    /// (d3-scale `linear.clamp(clamp)`).
    pub fn clamp(mut self, clamp: bool) -> Self {
        self.clamp = clamp;
        self
    }

    /// Widens the domain to round values for about ten ticks (d3-scale `linear.nice()`).
    pub fn nice(self) -> Self {
        self.nice_count(10.)
    }

    /// Widens the domain to round values for about `count` ticks (d3-scale
    /// `linear.nice(count)`). The domain changes only once the tick step settles,
    /// within ten rounds; a step that never settles, is zero or is NaN leaves it as it
    /// was.
    pub fn nice_count(mut self, count: f64) -> Self {
        let [d0, d1] = self.domain;
        let (mut start, mut stop) = (d0, d1);
        let reverse = stop < start;
        if reverse {
            std::mem::swap(&mut start, &mut stop);
        }
        let mut prestep: Option<f64> = None;
        for _ in 0..10 {
            let step = tick_increment(start, stop, count);
            if prestep == Some(step) {
                self.domain = if reverse {
                    [stop, start]
                } else {
                    [start, stop]
                };
                return self;
            } else if step > 0. {
                start = (start / step).floor() * step;
                stop = (stop / step).ceil() * step;
            } else if step < 0. {
                start = (start * step).ceil() / step;
                stop = (stop * step).floor() / step;
            } else {
                break;
            }
            prestep = Some(step);
        }
        self
    }

    /// The domain (d3-scale `linear.domain()`).
    pub fn get_domain(&self) -> [f64; 2] {
        self.domain
    }

    /// The range (d3-scale `linear.range()`).
    pub fn get_range(&self) -> [f64; 2] {
        self.range
    }

    /// Whether the scale clamps (d3-scale `linear.clamp()`).
    pub fn is_clamped(&self) -> bool {
        self.clamp
    }

    /// The range value for a domain value (d3-scale `linear(value)`). NaN maps to NaN,
    /// d3's `unknown` (`undefined`).
    pub fn scale(&self, value: f64) -> f64 {
        if value.is_nan() {
            return f64::NAN;
        }
        let value = if self.clamp {
            clamper(self.domain, value)
        } else {
            value
        };
        bimap(self.domain, self.range, value)
    }

    /// The domain value for a range value (d3-scale `linear.invert(value)`), clamped
    /// to the domain when the scale clamps.
    pub fn invert(&self, value: f64) -> f64 {
        let value = bimap(self.range, self.domain, value);
        if self.clamp {
            clamper(self.domain, value)
        } else {
            value
        }
    }

    /// About `count` round values across the domain (d3-scale `linear.ticks(count)`,
    /// ten when d3 is given none).
    pub fn ticks(&self, count: f64) -> Vec<f64> {
        ticks(self.domain[0], self.domain[1], count)
    }

    /// A number format suited to the ticks for `count` (d3-scale
    /// `linear.tickFormat(count, specifier)`): with no specifier, `,f` at the
    /// precision of the tick step.
    pub fn tick_format(&self, count: f64, specifier: Option<&FormatSpecifier>) -> NumberFormat {
        tick_format(self.domain[0], self.domain[1], count, specifier)
    }
}

/// `t` in [0, 1] for `x` in [a, b] (d3-scale's internal `normalize`): a zero-length
/// span maps every value to 0.5, a NaN span to NaN.
fn normalize(a: f64, b: f64, x: f64) -> f64 {
    let span = b - a;
    if span != 0. && !span.is_nan() {
        (x - a) / span
    } else if span.is_nan() {
        f64::NAN
    } else {
        0.5
    }
}

/// d3-interpolate `interpolateNumber(a, b)(t)`.
fn interpolate(a: f64, b: f64, t: f64) -> f64 {
    a * (1. - t) + b * t
}

/// Maps `x` from `from` to `to` (d3-scale's internal `bimap`), reading a descending
/// `from` from its low end.
fn bimap(from: [f64; 2], to: [f64; 2], x: f64) -> f64 {
    let [d0, d1] = from;
    let [r0, r1] = to;
    if d1 < d0 {
        interpolate(r1, r0, normalize(d1, d0, x))
    } else {
        interpolate(r0, r1, normalize(d0, d1, x))
    }
}

/// `x` held within the domain's bounds (d3-scale's internal `clamper`).
fn clamper(domain: [f64; 2], x: f64) -> f64 {
    let [mut a, mut b] = domain;
    if a > b {
        std::mem::swap(&mut a, &mut b);
    }
    js_max(a, js_min(b, x))
}
