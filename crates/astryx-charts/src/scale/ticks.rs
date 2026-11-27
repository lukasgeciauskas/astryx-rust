//! d3-array's tick arithmetic (`ticks`, `tickIncrement`, `tickStep`, `nice`), as
//! d3-array 3.2.4 computes it.

use astryx_theme::authoring::js::{js_max, js_round};

/// The longest array JavaScript can allocate; d3 throws a `RangeError` past it.
const MAX_ARRAY_LENGTH: f64 = 4_294_967_295.;

/// `Math.pow(10, power)` as V8 computes it. Every integer power reads as the nearest
/// double to 10^power, except 10^23, which V8's `pow` gives one unit high.
pub(crate) fn js_pow10(power: f64) -> f64 {
    if power.is_nan() {
        return f64::NAN;
    }
    if power == f64::INFINITY {
        return f64::INFINITY;
    }
    if power == f64::NEG_INFINITY {
        return 0.;
    }
    if power.fract() != 0. {
        return 10f64.powf(power);
    }
    if power == 23. {
        return 1.0000000000000001e23;
    }
    if power > 400. {
        return f64::INFINITY;
    }
    if power < -400. {
        return 0.;
    }
    format!("1e{power}").parse().unwrap_or(f64::NAN)
}

/// The integer bounds and the increment of the ticks between `start` and `stop`
/// (d3-array's internal `tickSpec`). A negative increment means the ticks are
/// `i / -inc`, which keeps fractional steps exact.
fn tick_spec(start: f64, stop: f64, count: f64) -> (f64, f64, f64) {
    let step = (stop - start) / js_max(0., count);
    let power = step.log10().floor();
    let error = step / js_pow10(power);
    // The error thresholds that pick a step of 10, 5 or 2 times a power of ten.
    let factor = if error >= 50f64.sqrt() {
        10.
    } else if error >= 10f64.sqrt() {
        5.
    } else if error >= 2f64.sqrt() {
        2.
    } else {
        1.
    };
    let (mut i1, mut i2, inc);
    if power < 0. {
        let positive = js_pow10(-power) / factor;
        i1 = js_round(start * positive);
        i2 = js_round(stop * positive);
        if i1 / positive < start {
            i1 += 1.;
        }
        if i2 / positive > stop {
            i2 -= 1.;
        }
        inc = -positive;
    } else {
        inc = js_pow10(power) * factor;
        i1 = js_round(start / inc);
        i2 = js_round(stop / inc);
        if i1 * inc < start {
            i1 += 1.;
        }
        if i2 * inc > stop {
            i2 -= 1.;
        }
    }
    if i2 < i1 && (0.5..2.).contains(&count) {
        return tick_spec(start, stop, count * 2.);
    }
    (i1, i2, inc)
}

/// About `count` evenly spaced, round values between `start` and `stop` (both
/// included when they are round), in the order of `start` to `stop` (d3-array
/// `ticks`). A `count` that is not positive gives none; equal bounds give that one
/// value. Where d3 throws a `RangeError` (more than 2^32 − 1 ticks) this returns none,
/// and so it does when the ticks do not fit in memory.
pub fn ticks(start: f64, stop: f64, count: f64) -> Vec<f64> {
    // `!(count > 0)`: NaN counts give none too.
    if count.is_nan() || count <= 0. {
        return Vec::new();
    }
    if start == stop {
        return vec![start];
    }
    let reverse = stop < start;
    let (i1, i2, inc) = if reverse {
        tick_spec(stop, start, count)
    } else {
        tick_spec(start, stop, count)
    };
    if i1.is_nan() || i2.is_nan() || i2 < i1 {
        return Vec::new();
    }
    let n = i2 - i1 + 1.;
    // `new Array(n)` throws for a length that is not an integer below 2^32.
    if n.is_nan() || n > MAX_ARRAY_LENGTH || n.fract() != 0. {
        return Vec::new();
    }
    let n = n as usize;
    let mut ticks = Vec::new();
    if ticks.try_reserve_exact(n).is_err() {
        return Vec::new();
    }
    for i in 0..n {
        let i = i as f64;
        ticks.push(match (reverse, inc < 0.) {
            (true, true) => (i2 - i) / -inc,
            (true, false) => (i2 - i) * inc,
            (false, true) => (i1 + i) / -inc,
            (false, false) => (i1 + i) * inc,
        });
    }
    ticks
}

/// The tick increment between `start` and `stop` (d3-array `tickIncrement`): a
/// positive step, or for a step below one the negative of its inverse (-10 for 0.1).
/// `start` must not be above `stop`.
pub fn tick_increment(start: f64, stop: f64, count: f64) -> f64 {
    tick_spec(start, stop, count).2
}

/// The signed distance between adjacent ticks (d3-array `tickStep`), negative when
/// `stop` is below `start`.
pub fn tick_step(start: f64, stop: f64, count: f64) -> f64 {
    let reverse = stop < start;
    let inc = if reverse {
        tick_increment(stop, start, count)
    } else {
        tick_increment(start, stop, count)
    };
    (if reverse { -1. } else { 1. }) * (if inc < 0. { 1. / -inc } else { inc })
}

/// The bounds widened to round values on the tick step for `count` ticks (d3-array
/// `nice`), repeated until the step settles. d3 has no bound on the rounds, and a step
/// that alternates between two values never lets it return; this stops after 10,000
/// rounds with the bounds reached, past the roughly 2,100 a domain can take to widen
/// from the smallest double to the largest.
pub fn nice(start: f64, stop: f64, count: f64) -> [f64; 2] {
    let (mut start, mut stop) = (start, stop);
    let mut prestep: Option<f64> = None;
    for _ in 0..10_000 {
        let step = tick_increment(start, stop, count);
        if prestep == Some(step) || step == 0. || !step.is_finite() {
            return [start, stop];
        } else if step > 0. {
            start = (start / step).floor() * step;
            stop = (stop / step).ceil() * step;
        } else if step < 0. {
            start = (start * step).ceil() / step;
            stop = (stop * step).floor() / step;
        }
        prestep = Some(step);
    }
    [start, stop]
}
