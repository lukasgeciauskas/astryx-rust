//! UIProgressView, upstream `motion/uikit/progressBar.ts` (registered as
//! the `progress` table).

use crate::motion::bezier::BezierPoints;
use crate::motion::roles::TableValue;

/// `fillTrackMs`: a fill change's length.
pub const FILL_TRACK_MS: f64 = 1000.;
/// `curves.fill`: linear.
pub const FILL_CURVE: BezierPoints = [0., 0., 1., 1.];
/// `reducedMotionPulseMs`: the busy loop under Reduce Motion (a fixed
/// role: the tempo leaves it as measured).
pub const REDUCED_MOTION_PULSE_MS: f64 = 2000.;
/// `reducedMotionPulseFloor`.
pub const REDUCED_MOTION_PULSE_FLOOR: f64 = 0.84;
/// `progressView.heightPt`.
pub const HEIGHT_PT: f64 = 4.;
/// `progressView.minimumFillHeights`.
pub const MINIMUM_FILL_HEIGHTS: f64 = 2.;
/// `progressView.emptyFillAlpha`.
pub const EMPTY_FILL_ALPHA: f64 = 0.;

/// The table's roles.
pub const ROLES: [&str; 3] = ["fillTrackMs", "curves.fill", "reducedMotionPulseMs"];

pub(crate) fn leaves() -> Vec<(String, TableValue)> {
    use TableValue::{Number, Numbers};
    [
        ("fillTrackMs", Number(FILL_TRACK_MS)),
        ("curves.fill", Numbers(FILL_CURVE.to_vec())),
        ("reducedMotionPulseMs", Number(REDUCED_MOTION_PULSE_MS)),
        (
            "reducedMotionPulseFloor",
            Number(REDUCED_MOTION_PULSE_FLOOR),
        ),
        ("progressView.heightPt", Number(HEIGHT_PT)),
        (
            "progressView.minimumFillHeights",
            Number(MINIMUM_FILL_HEIGHTS),
        ),
        ("progressView.emptyFillAlpha", Number(EMPTY_FILL_ALPHA)),
    ]
    .into_iter()
    .map(|(path, value)| (path.to_string(), value))
    .collect()
}
