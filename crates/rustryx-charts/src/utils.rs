//! Shared chart helpers (upstream `utils.ts`).

use crate::Datum;
use crate::types::ChartScale;

/// Upstream `isBandScale`.
pub fn is_band_scale(scale: &ChartScale) -> bool {
    scale.is_band()
}

/// A datum's x pixel (upstream `xPixel`): a band's center, or the linear position of
/// the coerced value; NaN (off canvas, which marks skip) for a value that does not
/// coerce to a finite number.
pub fn x_pixel(d: &Datum, x_key: &str, x_scale: &ChartScale) -> f64 {
    match x_scale {
        ChartScale::Band(band) => {
            band.scale(&d.string(x_key)).unwrap_or(0.) + band.bandwidth() / 2.
        }
        ChartScale::Linear(linear) => {
            let value = crate::js_number(d.get(x_key));
            if value.is_finite() {
                linear.scale(value)
            } else {
                f64::NAN
            }
        }
    }
}
