//! UIScrollView's paging, upstream `motion/uikit/paging.ts`. No value is a
//! motion role.

use crate::motion::bezier::BezierPoints;
use crate::motion::roles::TableValue;

/// `integrator`: UIKit's per-step page spring.
pub mod integrator {
    /// `springPull`: the share of the distance left pulled per step.
    pub const SPRING_PULL: f64 = 0.000284858;
    /// `friction`: the velocity kept per step.
    pub const FRICTION: f64 = 0.970815;
    /// `stepsPerSecond`.
    pub const STEPS_PER_SECOND: f64 = 960.;
    /// `restDistance`, pt.
    pub const REST_DISTANCE: f64 = 0.5;
    /// `restSpeed`, pt per step.
    pub const REST_SPEED: f64 = 0.05;
}

/// `release`: which page a release goes to.
pub mod release {
    /// `commitFraction`.
    pub const COMMIT_FRACTION: f64 = 0.5;
    /// `advanceSpeed`, pt/s.
    pub const ADVANCE_SPEED: f64 = 300.;
    /// `velocityFloor`, pt/s.
    pub const VELOCITY_FLOOR: f64 = 225.;
    /// `maxPagesFromStart`.
    pub const MAX_PAGES_FROM_START: f64 = 1.;
}

/// `catchStopMs`: arbitration, not motion.
pub const CATCH_STOP_MS: f64 = 85.;
/// `panAxisRatio`.
pub const PAN_AXIS_RATIO: f64 = 2.;
/// `nestedPanAxisRatio`.
pub const NESTED_PAN_AXIS_RATIO: f64 = 1.;
/// `animatedScroll.durationMs`: `setContentOffset(_:animated:)`.
pub const ANIMATED_SCROLL_MS: f64 = 300.;
/// `animatedScroll.curve`.
pub const ANIMATED_SCROLL_CURVE: BezierPoints = [0.364212, 0., 0.635788, 1.];
/// `rubberBandDimension`: the page's width.
pub const RUBBER_BAND_DIMENSION: &str = "pageWidth";

/// The paths upstream leaves out of the registry.
pub const EXCLUDE: [&str; 1] = ["catchStopMs"];

pub(crate) fn leaves() -> Vec<(String, TableValue)> {
    use TableValue::{Number, Numbers, Text};
    [
        ("integrator.springPull", Number(integrator::SPRING_PULL)),
        ("integrator.friction", Number(integrator::FRICTION)),
        (
            "integrator.stepsPerSecond",
            Number(integrator::STEPS_PER_SECOND),
        ),
        ("integrator.restDistance", Number(integrator::REST_DISTANCE)),
        ("integrator.restSpeed", Number(integrator::REST_SPEED)),
        ("release.commitFraction", Number(release::COMMIT_FRACTION)),
        ("release.advanceSpeed", Number(release::ADVANCE_SPEED)),
        ("release.velocityFloor", Number(release::VELOCITY_FLOOR)),
        (
            "release.maxPagesFromStart",
            Number(release::MAX_PAGES_FROM_START),
        ),
        ("catchStopMs", Number(CATCH_STOP_MS)),
        ("panAxisRatio", Number(PAN_AXIS_RATIO)),
        ("nestedPanAxisRatio", Number(NESTED_PAN_AXIS_RATIO)),
        ("animatedScroll.durationMs", Number(ANIMATED_SCROLL_MS)),
        (
            "animatedScroll.curve",
            Numbers(ANIMATED_SCROLL_CURVE.to_vec()),
        ),
        ("rubberBandDimension", Text(RUBBER_BAND_DIMENSION)),
    ]
    .into_iter()
    .map(|(path, value)| (path.to_string(), value))
    .collect()
}
