//! UISegmentedControl, upstream `motion/uikit/segmented.ts`.

use crate::motion::roles::TableValue;
use crate::motion::{MotionSpring, SpringDescription};

/// `selectionSpring`: the selection's slide (a role).
pub const SELECTION_SPRING: SpringDescription = SpringDescription::ResponseDamping {
    response: 0.4,
    damping_ratio: 0.85,
};
/// `dragOvershoot`, pt.
pub const DRAG_OVERSHOOT: f64 = 11.;
/// `liftGrowth`, pt (width, height).
pub const LIFT_GROWTH: [f64; 2] = [24., 16.];
/// `lift` (a role).
pub const LIFT: SpringDescription = SpringDescription::ResponseDamping {
    response: 0.25,
    damping_ratio: 1.,
};

/// The roles of this table.
pub mod role {
    use super::MotionSpring;
    /// `segmented.selectionSpring`.
    pub const SELECTION_SPRING: MotionSpring =
        MotionSpring::role("segmented.selectionSpring", super::SELECTION_SPRING);
    /// `segmented.lift`.
    pub const LIFT: MotionSpring = MotionSpring::role("segmented.lift", super::LIFT);
}

/// The table's roles.
pub const ROLES: [&str; 2] = ["selectionSpring", "lift"];

pub(crate) fn leaves() -> Vec<(String, TableValue)> {
    use TableValue::{Number, Numbers, Spring};
    [
        ("selectionSpring", Spring(SELECTION_SPRING)),
        ("dragOvershoot", Number(DRAG_OVERSHOOT)),
        ("liftGrowth", Numbers(LIFT_GROWTH.to_vec())),
        ("lift", Spring(LIFT)),
    ]
    .into_iter()
    .map(|(path, value)| (path.to_string(), value))
    .collect()
}
