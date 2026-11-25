//! UIScrollView's rubber band past an edge, upstream `motion/rubberBand.ts`.

use super::uikit::scroll::RUBBER_BAND_COEFFICIENT as C;

/// The overflow shown for a finger `overflow` past the edge (signed);
/// `dimension` is the scroll view's size on the axis.
pub fn rubber_band(overflow: f64, dimension: f64) -> f64 {
    if dimension.is_nan() || dimension <= 0. {
        return 0.;
    }
    let banded = (1. - 1. / (overflow.abs() * C / dimension + 1.)) * dimension;
    banded.copysign(overflow)
}

/// The finger overflow that shows `shown`; infinite at the dimension.
pub fn rubber_band_inverse(shown: f64, dimension: f64) -> f64 {
    if dimension.is_nan() || dimension <= 0. {
        return 0.;
    }
    let b = shown.abs();
    if b >= dimension {
        return f64::INFINITY.copysign(shown);
    }
    (dimension / C * b / (dimension - b)).copysign(shown)
}

/// A raw value held to `[min, max]`: unchanged inside, banded past an edge.
pub fn rubber_band_clamp(value: f64, min: f64, max: f64, dimension: f64) -> f64 {
    if value < min {
        min + rubber_band(value - min, dimension)
    } else if value > max {
        max + rubber_band(value - max, dimension)
    } else {
        value
    }
}

/// The raw value that [`rubber_band_clamp`] shows as `shown`.
pub fn rubber_band_unclamp(shown: f64, min: f64, max: f64, dimension: f64) -> f64 {
    if shown < min {
        min + rubber_band_inverse(shown - min, dimension)
    } else if shown > max {
        max + rubber_band_inverse(shown - max, dimension)
    } else {
        shown
    }
}
