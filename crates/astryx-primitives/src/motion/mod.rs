//! Motion, upstream `motion/` and `animator/`: closed-form springs, the
//! scroll view's momentum decay and its projection, the rubber band, the
//! UIKit constants they read (`motion/uikit/`), the motion roles and the
//! theme's timing, the Reduce Motion plan, and the animator engine that
//! drives an element's x, y, rotate, scale and opacity on an injectable
//! clock. Pure numbers in seconds, ms, px and px/s, with no GPUI types:
//! components drive them frame by frame.

pub mod animator;
pub mod bezier;
pub mod coast;
pub mod create_animator;
pub mod curve;
pub mod decay;
pub mod linear_easing;
pub mod paging;
pub mod progress;
pub mod projection;
pub mod reduced_motion;
pub mod reduced_motion_plan;
pub mod resolve_motion;
pub mod resolve_snap_point;
pub mod role_value;
pub mod roles;
pub mod rubber_band;
pub mod spring;
pub mod theme_tempo;
pub mod theme_timing;
pub mod uikit;

pub use animator::{
    AnimateOptions, Clock, CrossfadeAs, DecayCurves, DecayOptions, EngineOptions, EngineRegistry,
    MAX_DURATION_MS, MotionCurves, MotionEngine, MotionError, MotionHandle, MotionKey,
    MotionValues, RebaseOptions, ReducedMotion, SpringTransition, StartMode,
};
pub use bezier::{BezierPoints, CubicBezierEasing, cubic_bezier};
pub use coast::{CoastBounds, CoastOptions, CoastVelocity, coast, coast_from};
pub use create_animator::{
    Animator, AnimatorAnimateOptions, AnimatorCall, AnimatorChannel, AnimatorSnapshot,
    AnimatorValues,
};
pub use curve::{
    ENTRANCE_OPACITY_FLOOR, FnCurve, LATE_START_MS, MotionCurve, MotionProperty, MotionSpring,
    SETTLE_THRESHOLD, SharedCurve,
};
pub use decay::{DecayCurve, decay_rest_distance, solve_decay};
pub use projection::{SnapRelease, SnapSelection, project, project_distance, select_snap_point};
pub use resolve_motion::{
    FIXED_ROLES, MoveTiming, SpringMove, resolve_duration_ms, resolve_move_timing,
    resolve_settle_ms, resolve_spring,
};
pub use rubber_band::{
    rubber_band, rubber_band_clamp, rubber_band_inverse, rubber_band_inverse_with,
    rubber_band_slope, rubber_band_slope_with, rubber_band_unclamp, rubber_band_with,
};
pub use spring::{
    DurationBounce, ResponseDamping, SETTLING_EPSILON, Spring, SpringCurve, SpringDescription,
    SpringError, SpringRampCurve, absolute_velocity, relative_velocity, settling_duration,
    settling_duration_with, solve_spring, solve_spring_toward_ramp, spring_from_duration_bounce,
    spring_from_physics, spring_from_response, spring_from_uikit_damping_duration,
};

pub use theme_tempo::theme_tempo;

#[cfg(test)]
mod animator_tests;
#[cfg(test)]
mod coast_tests;
#[cfg(test)]
mod create_animator_tests;
#[cfg(test)]
mod math_tests;
#[cfg(test)]
mod misc_tests;
#[cfg(test)]
mod paging_tests;
#[cfg(test)]
mod progress_tests;
#[cfg(test)]
mod resolve_tests;
#[cfg(test)]
mod roles_tests;
#[cfg(test)]
mod spring_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod theme_timing_tests;
