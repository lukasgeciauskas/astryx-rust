//! The momentum curves of a released scroll view, upstream
//! `motion/coast.ts`: UIScrollView's coast, handing its velocity to an edge
//! spring at a bound, for [`MotionEngine::decay`] to play.
//!
//! Each axis starts at its presentation value, so a coast continues
//! whatever was running without a jump: inside its bounds it coasts at
//! `rate` and rests where it projects; a coast that reaches an edge hands
//! its velocity to the edge spring, which settles on the edge; released past
//! an edge, it springs straight back with the release velocity (a
//! transition, which Reduce Motion replaces); at zero velocity it rests.

use super::animator::{DecayCurves, MotionEngine};
use super::resolve_motion::resolve_spring;
use super::uikit::scroll;
use super::{FnCurve, MotionProperty, MotionSpring, SharedCurve, solve_decay, solve_spring};

/// An axis's edges.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CoastBounds {
    pub min: f64,
    pub max: f64,
}

/// Options of [`coast`] (upstream `CoastOptions`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CoastOptions {
    /// Per-millisecond velocity factor; UIScrollView's normal rate by default.
    pub rate: f64,
    /// Edges per axis: a coast crossing one hands its velocity to `bounce`.
    pub bounds_x: Option<CoastBounds>,
    pub bounds_y: Option<CoastBounds>,
    /// The edge spring; `scroll.bounce` by default, which is no motion role.
    pub bounce: MotionSpring,
    /// The release time, ms; now by default. Pass the same `at` to `decay`.
    pub at: Option<f64>,
}

impl Default for CoastOptions {
    fn default() -> Self {
        Self {
            rate: scroll::DECELERATION_NORMAL,
            bounds_x: None,
            bounds_y: None,
            bounce: MotionSpring::inline(scroll::BOUNCE),
            at: None,
        }
    }
}

/// A release's velocity per translate axis, px/s.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CoastVelocity {
    pub x: Option<f64>,
    pub y: Option<f64>,
}

fn settling(
    duration: f64,
    end: f64,
    at: impl Fn(f64) -> f64 + 'static,
    velocity: impl Fn(f64) -> f64 + 'static,
) -> FnCurve {
    FnCurve::new(
        duration,
        move |t| if t >= duration { end } else { at(t) },
        move |t| if t >= duration { 0. } else { velocity(t) },
    )
}

/// The momentum curves of a release on `engine` (upstream `coast`).
pub fn coast(engine: &MotionEngine, velocity: CoastVelocity, options: CoastOptions) -> DecayCurves {
    let tempo = engine.tempo();
    coast_from(
        |property, at| engine.get(property, at),
        tempo,
        velocity,
        options,
    )
}

/// [`coast`] over any reader of presentation values (`read(property, at)`)
/// and the theme's tempo.
pub fn coast_from(
    read: impl Fn(MotionProperty, Option<f64>) -> f64,
    tempo: f64,
    velocity: CoastVelocity,
    options: CoastOptions,
) -> DecayCurves {
    let bounce = resolve_spring(options.bounce, tempo).to_spring();
    let mut curves = DecayCurves::default();
    let axes = [
        (MotionProperty::X, velocity.x, options.bounds_x),
        (MotionProperty::Y, velocity.y, options.bounds_y),
    ];
    for (property, v0, bounds) in axes {
        let Some(v0) = v0.filter(|v| v.is_finite()) else {
            continue;
        };
        let from = read(property, options.at);
        let threshold = property.settle_threshold();
        let curve: SharedCurve = match bounds {
            Some(bounds) if from < bounds.min || from > bounds.max => {
                let edge = if from < bounds.min {
                    bounds.min
                } else {
                    bounds.max
                };
                let back = solve_spring(bounce, from, edge, v0);
                settling(
                    back.settle_time(threshold),
                    edge,
                    move |t| back.value(t),
                    move |t| back.velocity(t),
                )
                .transition()
                .shared()
            }
            _ => {
                let glide = solve_decay(from, v0, Some(options.rate));
                let edge = bounds.and_then(|b| {
                    if glide.rest < b.min {
                        Some(b.min)
                    } else if glide.rest > b.max {
                        Some(b.max)
                    } else {
                        None
                    }
                });
                let hit = edge.and_then(|edge| glide.time_to_reach(edge));
                if let (Some(edge), Some(hit)) = (edge, hit) {
                    let spring = solve_spring(bounce, edge, edge, glide.velocity(hit));
                    settling(
                        hit + spring.settle_time(threshold),
                        edge,
                        move |t| {
                            if t < hit {
                                glide.value(t)
                            } else {
                                spring.value(t - hit)
                            }
                        },
                        move |t| {
                            if t < hit {
                                glide.velocity(t)
                            } else {
                                spring.velocity(t - hit)
                            }
                        },
                    )
                    .shared()
                } else if v0 == 0. {
                    settling(0., from, move |_| from, |_| 0.).shared()
                } else {
                    settling(
                        glide.settle_time(threshold * 2.),
                        edge.unwrap_or(glide.rest),
                        move |t| glide.value(t),
                        move |t| glide.velocity(t),
                    )
                    .shared()
                }
            }
        };
        match property {
            MotionProperty::X => curves.x = Some(curve),
            _ => curves.y = Some(curve),
        }
    }
    curves
}
