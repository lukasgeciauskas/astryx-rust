//! Where a release comes to rest, upstream `motion/projection.ts` (WWDC18).

/// The distance a surface released at `velocity` units/s coasts.
pub fn project_distance(velocity: f64, rate: f64) -> f64 {
    debug_assert!(rate > 0. && rate < 1., "projection: rate must be in (0, 1)");
    velocity / 1000. * rate / (1. - rate)
}

/// Where a surface released at `position` with `velocity` comes to rest.
pub fn project(position: f64, velocity: f64, rate: f64) -> f64 {
    position + project_distance(velocity, rate)
}
