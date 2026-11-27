//! A sampled curve as CSS `linear()` easing, upstream
//! `motion/linearEasing.ts`.

/// Samples per second of a sampled curve: one per frame at 60 Hz.
const SAMPLES_PER_SECOND: f64 = 60.;

/// How many samples cover `duration_ms`, both ends included: at least 2
/// (upstream `sampleCount`).
pub fn sample_count(duration_ms: f64) -> usize {
    let count = (duration_ms / 1000. * SAMPLES_PER_SECOND).ceil() + 1.;
    if count.is_nan() {
        2
    } else {
        (count as usize).max(2)
    }
}

/// The fraction `done(t)` of a move `t` ms in, sampled once per frame over
/// `duration_ms` and rounded to four decimals: the stops of
/// [`linear_easing`].
pub fn linear_easing_stops(duration_ms: f64, done: impl Fn(f64) -> f64) -> Vec<f64> {
    let count = sample_count(duration_ms);
    (0..count)
        .map(|i| {
            let value = done(i as f64 / (count - 1) as f64 * duration_ms);
            (value * 10000.).round() / 10000.
        })
        .map(|v| if v == 0. { 0. } else { v })
        .collect()
}

/// The curve `done` traces over `duration_ms` as CSS `linear()` easing
/// text, one stop per frame (upstream `linearEasing`).
pub fn linear_easing(duration_ms: f64, done: impl Fn(f64) -> f64) -> String {
    let stops: Vec<String> = linear_easing_stops(duration_ms, done)
        .into_iter()
        .map(|v| format!("{v}"))
        .collect();
    format!("linear({})", stops.join(", "))
}
