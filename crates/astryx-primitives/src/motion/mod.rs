//! Motion math, upstream `motion/`: closed-form springs, the scroll view's
//! momentum decay and its projection, the rubber band, and the UIKit
//! constants they read (`motion/uikit/`). Pure numbers in seconds, px and
//! px/s; components drive them frame by frame.
//!
//! M8 pulled this forward for the date and time pickers' wheels; the motion
//! layer (M11) builds its animator and roles on it.

mod decay;
mod projection;
mod rubber_band;
mod spring;
pub mod uikit;

pub use decay::{DecayCurve, decay_rest_distance, solve_decay};
pub use projection::{project, project_distance};
pub use rubber_band::{rubber_band, rubber_band_clamp, rubber_band_inverse, rubber_band_unclamp};
pub use spring::{
    SETTLING_EPSILON, Spring, SpringCurve, SpringDescription, settling_duration, solve_spring,
};

#[cfg(test)]
mod tests;
