//! Shared chart helpers (upstream lab `utils.ts`): astryx-charts' band test and x pixel,
//! which upstream's lab and charts packages share.

use astryx_charts::{ChartValue, Datum};

pub use astryx_charts::{is_band_scale, x_pixel};

/// A datum's value for a mark (upstream's `typeof d[key] === 'number' ? d[key] : 0`): the
/// number, NaN included, and 0 for anything else.
pub(crate) fn number_or_zero(d: &Datum, key: &str) -> f64 {
    match d.get(key) {
        Some(ChartValue::Number(n)) => *n,
        _ => 0.,
    }
}

/// JavaScript's `Math.min` of two numbers: NaN wins.
pub(crate) fn js_min(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else {
        a.min(b)
    }
}

/// JavaScript's `Math.max` of two numbers: NaN wins.
pub(crate) fn js_max(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else {
        a.max(b)
    }
}
