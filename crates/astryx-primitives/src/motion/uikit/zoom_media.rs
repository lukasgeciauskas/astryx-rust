//! The zoomed media viewer (Photos), upstream `motion/uikit/zoomMedia.ts`.

use crate::motion::roles::TableValue;
use crate::motion::{MotionSpring, SpringDescription};

/// `overshoot.limitFactor`.
pub const OVERSHOOT_LIMIT_FACTOR: f64 = 2.;
/// `viewer.doubleTapZoom`.
pub const DOUBLE_TAP_ZOOM: &str = "aspectFill";
/// `viewer.maxPixelZoomMultiple`.
pub const MAX_PIXEL_ZOOM_MULTIPLE: f64 = 6.;
/// `viewer.pageGap`, pt.
pub const PAGE_GAP: f64 = 40.;
/// `dismiss.scalePerTravelSquared`.
pub const SCALE_PER_TRAVEL_SQUARED: f64 = 4.91e-6;
/// `dismiss.backdropAlphaPerTravelSquared`.
pub const BACKDROP_ALPHA_PER_TRAVEL_SQUARED: f64 = 1.35e-5;
/// `dismiss.chromeAlphaPerPoint`.
pub const CHROME_ALPHA_PER_POINT: f64 = 0.00533;
/// `dismiss.chromeFadeStart`, pt.
pub const CHROME_FADE_START: f64 = 17.8;
/// `dismiss.cancelTravel`, pt.
pub const CANCEL_TRAVEL: f64 = 10.;
/// `dismiss.projectionRate`.
pub const PROJECTION_RATE: f64 = 0.998;
/// `dismiss.dismissPositionSpring` (a role).
pub const DISMISS_POSITION_SPRING: SpringDescription = SpringDescription::Physics {
    mass: 1.,
    stiffness: 918.09,
    damping: 60.6,
};
/// `dismiss.dismissSizeSpring` (a role).
pub const DISMISS_SIZE_SPRING: SpringDescription = SpringDescription::Physics {
    mass: 1.,
    stiffness: 947.14,
    damping: 61.55,
};

/// The roles of this table.
pub mod role {
    use super::MotionSpring;
    /// `zoomMedia.dismiss.dismissPositionSpring`.
    pub const DISMISS_POSITION_SPRING: MotionSpring = MotionSpring::role(
        "zoomMedia.dismiss.dismissPositionSpring",
        super::DISMISS_POSITION_SPRING,
    );
    /// `zoomMedia.dismiss.dismissSizeSpring`.
    pub const DISMISS_SIZE_SPRING: MotionSpring = MotionSpring::role(
        "zoomMedia.dismiss.dismissSizeSpring",
        super::DISMISS_SIZE_SPRING,
    );
}

/// The table's roles.
pub const ROLES: [&str; 2] = ["dismiss.dismissPositionSpring", "dismiss.dismissSizeSpring"];

pub(crate) fn leaves() -> Vec<(String, TableValue)> {
    use TableValue::{Number, Spring, Text};
    [
        ("overshoot.limitFactor", Number(OVERSHOOT_LIMIT_FACTOR)),
        ("viewer.doubleTapZoom", Text(DOUBLE_TAP_ZOOM)),
        (
            "viewer.maxPixelZoomMultiple",
            Number(MAX_PIXEL_ZOOM_MULTIPLE),
        ),
        ("viewer.pageGap", Number(PAGE_GAP)),
        (
            "dismiss.scalePerTravelSquared",
            Number(SCALE_PER_TRAVEL_SQUARED),
        ),
        (
            "dismiss.backdropAlphaPerTravelSquared",
            Number(BACKDROP_ALPHA_PER_TRAVEL_SQUARED),
        ),
        (
            "dismiss.chromeAlphaPerPoint",
            Number(CHROME_ALPHA_PER_POINT),
        ),
        ("dismiss.chromeFadeStart", Number(CHROME_FADE_START)),
        ("dismiss.cancelTravel", Number(CANCEL_TRAVEL)),
        ("dismiss.projectionRate", Number(PROJECTION_RATE)),
        (
            "dismiss.dismissPositionSpring",
            Spring(DISMISS_POSITION_SPRING),
        ),
        ("dismiss.dismissSizeSpring", Spring(DISMISS_SIZE_SPRING)),
    ]
    .into_iter()
    .map(|(path, value)| (path.to_string(), value))
    .collect()
}
