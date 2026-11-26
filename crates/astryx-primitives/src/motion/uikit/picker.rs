//! UIPickerView, measured at its 32 pt row, upstream
//! `motion/uikit/picker.ts`.

use crate::motion::roles::TableValue;

/// `geometry.rowHeight`, pt.
pub const ROW_HEIGHT: f64 = 32.;
/// `geometry.height`, pt.
pub const HEIGHT: f64 = 216.;
/// `geometry.rubberBandDimension`, pt.
pub const RUBBER_BAND_DIMENSION: f64 = 291.;
/// `release.stopSpeed`, pt/ms: where a coast ends.
pub const STOP_SPEED: f64 = 0.0089;
/// `release.minimumSpeed`, pt/ms: slower releases inside the rows snap.
pub const MINIMUM_SPEED: f64 = 0.25;
/// `snapRate`: the share of the distance left covered per millisecond.
pub const SNAP_RATE: f64 = 0.011;
/// `tapSpin.durationMs`: the measured reference for a turn (a role).
pub const TAP_SPIN_MS: f64 = 300.;
/// The role id of [`TAP_SPIN_MS`].
pub const TAP_SPIN_MS_ROLE: &str = "picker.tapSpin.durationMs";
/// `selectionTick.minimumIntervalMs`.
pub const SELECTION_TICK_INTERVAL_MS: f64 = 40.;

/// The table's roles.
pub const ROLES: [&str; 1] = ["tapSpin.durationMs"];

pub(crate) fn leaves() -> Vec<(String, TableValue)> {
    use TableValue::Number;
    [
        ("geometry.rowHeight", Number(ROW_HEIGHT)),
        ("geometry.height", Number(HEIGHT)),
        (
            "geometry.rubberBandDimension",
            Number(RUBBER_BAND_DIMENSION),
        ),
        ("release.stopSpeed", Number(STOP_SPEED)),
        ("release.minimumSpeed", Number(MINIMUM_SPEED)),
        ("snapRate", Number(SNAP_RATE)),
        ("tapSpin.durationMs", Number(TAP_SPIN_MS)),
        (
            "selectionTick.minimumIntervalMs",
            Number(SELECTION_TICK_INTERVAL_MS),
        ),
    ]
    .into_iter()
    .map(|(path, value)| (path.to_string(), value))
    .collect()
}
