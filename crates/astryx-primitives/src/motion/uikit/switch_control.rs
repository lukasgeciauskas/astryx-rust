//! UISwitch, upstream `motion/uikit/switch.ts` (the `switchControl`
//! table).

use crate::motion::roles::TableValue;
use crate::motion::{MotionSpring, SpringDescription};

const fn rd(response: f64, damping_ratio: f64) -> SpringDescription {
    SpringDescription::ResponseDamping {
        response,
        damping_ratio,
    }
}

/// `trackSize`, pt.
pub const TRACK_SIZE: [f64; 2] = [63., 28.];
/// `thumbSize`, pt.
pub const THUMB_SIZE: [f64; 2] = [37., 24.];
/// `liftedThumbSize`, pt.
pub const LIFTED_THUMB_SIZE: [f64; 2] = [58., 38.333];
/// `lift`.
pub const LIFT: SpringDescription = rd(0.23, 0.55);
/// `settle`.
pub const SETTLE: SpringDescription = rd(0.5, 0.7);
/// `minimumLiftMs`.
pub const MINIMUM_LIFT_MS: f64 = 235.;
/// `travel`: `springs.switchThumbMove`.
pub const TRAVEL: SpringDescription = super::springs::SWITCH_THUMB_MOVE;
/// `tracking`: follows the finger (a fixed role).
pub const TRACKING: SpringDescription = rd(0.36, 1.);
/// `rubberBand.coefficient`.
pub const RUBBER_BAND_COEFFICIENT: f64 = 0.55;
/// `rubberBand.dimension`, pt.
pub const RUBBER_BAND_DIMENSION: f64 = 12.;
/// `flipDistance`, pt.
pub const FLIP_DISTANCE: f64 = 26.;
/// `wellOn`.
pub const WELL_ON: SpringDescription = SpringDescription::Physics {
    mass: 1.,
    stiffness: 85.37720070146501,
    damping: 18.47995678582231,
};
/// `wellOff`.
pub const WELL_OFF: SpringDescription = SpringDescription::Physics {
    mass: 1.,
    stiffness: 246.74011002723395,
    damping: 31.41592653589793,
};
/// `liftedShadow`: blur, y, alpha.
pub const LIFTED_SHADOW: [f64; 3] = [7., 3., 0.12];
/// `haptic`.
pub const HAPTIC: &str = "none";

/// The roles of this table.
pub mod role {
    use super::MotionSpring;
    /// `switchControl.lift`.
    pub const LIFT: MotionSpring = MotionSpring::role("switchControl.lift", super::LIFT);
    /// `switchControl.settle`.
    pub const SETTLE: MotionSpring = MotionSpring::role("switchControl.settle", super::SETTLE);
    /// `switchControl.travel`.
    pub const TRAVEL: MotionSpring = MotionSpring::role("switchControl.travel", super::TRAVEL);
    /// `switchControl.tracking`.
    pub const TRACKING: MotionSpring =
        MotionSpring::role("switchControl.tracking", super::TRACKING);
    /// `switchControl.wellOn`.
    pub const WELL_ON: MotionSpring = MotionSpring::role("switchControl.wellOn", super::WELL_ON);
    /// `switchControl.wellOff`.
    pub const WELL_OFF: MotionSpring = MotionSpring::role("switchControl.wellOff", super::WELL_OFF);
}

/// The table's roles.
pub const ROLES: [&str; 7] = [
    "lift",
    "settle",
    "travel",
    "tracking",
    "wellOn",
    "wellOff",
    "minimumLiftMs",
];

pub(crate) fn leaves() -> Vec<(String, TableValue)> {
    use TableValue::{Number, Numbers, Spring, Text};
    [
        ("trackSize", Numbers(TRACK_SIZE.to_vec())),
        ("thumbSize", Numbers(THUMB_SIZE.to_vec())),
        ("liftedThumbSize", Numbers(LIFTED_THUMB_SIZE.to_vec())),
        ("lift", Spring(LIFT)),
        ("settle", Spring(SETTLE)),
        ("minimumLiftMs", Number(MINIMUM_LIFT_MS)),
        ("travel", Spring(TRAVEL)),
        ("tracking", Spring(TRACKING)),
        ("rubberBand.coefficient", Number(RUBBER_BAND_COEFFICIENT)),
        ("rubberBand.dimension", Number(RUBBER_BAND_DIMENSION)),
        ("flipDistance", Number(FLIP_DISTANCE)),
        ("wellOn", Spring(WELL_ON)),
        ("wellOff", Spring(WELL_OFF)),
        ("liftedShadow", Numbers(LIFTED_SHADOW.to_vec())),
        ("haptic", Text(HAPTIC)),
    ]
    .into_iter()
    .map(|(path, value)| (path.to_string(), value))
    .collect()
}
