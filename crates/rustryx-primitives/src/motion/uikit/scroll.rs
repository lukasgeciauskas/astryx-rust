//! UIScrollView, upstream `motion/uikit/scroll.ts`.

use crate::motion::SpringDescription;
use crate::motion::roles::TableValue;

/// `decelerationRate.normal`: the velocity factor per millisecond.
pub const DECELERATION_NORMAL: f64 = 0.998;
/// `decelerationRate.fast`.
pub const DECELERATION_FAST: f64 = 0.99;
/// `rubberBand.coefficient`.
pub const RUBBER_BAND_COEFFICIENT: f64 = 0.55;
/// `bounce`: the spring that returns content released past an edge.
pub const BOUNCE: SpringDescription = SpringDescription::DurationBounce {
    duration: 0.5,
    bounce: 0.,
};
/// `delaysContentTouchesMs`: arbitration, not motion.
pub const DELAYS_CONTENT_TOUCHES_MS: f64 = 150.;
/// `scrollToTopRate`: the per-ms factor of a tap on the status bar.
pub const SCROLL_TO_TOP_RATE: f64 = 0.99035;

/// The paths upstream leaves out of the registry.
pub const EXCLUDE: [&str; 1] = ["delaysContentTouchesMs"];

pub(crate) fn leaves() -> Vec<(String, TableValue)> {
    use TableValue::{Number, Spring};
    [
        ("decelerationRate.normal", Number(DECELERATION_NORMAL)),
        ("decelerationRate.fast", Number(DECELERATION_FAST)),
        ("rubberBand.coefficient", Number(RUBBER_BAND_COEFFICIENT)),
        ("bounce", Spring(BOUNCE)),
        ("delaysContentTouchesMs", Number(DELAYS_CONTENT_TOUCHES_MS)),
        ("scrollToTopRate", Number(SCROLL_TO_TOP_RATE)),
    ]
    .into_iter()
    .map(|(path, value)| (path.to_string(), value))
    .collect()
}
