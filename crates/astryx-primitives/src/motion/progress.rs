//! A path's progress and companion curves, upstream `motion/progress.ts`.
//!
//! A path is described by a normalized progress: 0 where it starts, 1 where
//! it rests, beyond either end on a spring's overshoot; its velocity is in
//! progress per second. An interrupted motion continues from what is on
//! screen: its progress starts with the presented velocity projected on the
//! path's tangent at its start, v·t / (t·t). A companion follows a driving
//! curve through [`map_progress`], most often with [`map_range`].

use std::rc::Rc;

use super::animator::{MotionEngine, MotionKey};
use super::{
    FnCurve, MotionProperty, SETTLING_EPSILON, SharedCurve, SpringDescription, solve_spring,
};

/// The step of the finite differences taken on a progress.
const STEP: f64 = 1e-4;

/// The progress of `spring` from 0 to 1, starting with `velocity` (progress
/// per second): exactly 1 once it settles within `epsilon` (upstream
/// `progressCurve`; `SETTLING_EPSILON` by default).
pub fn progress_curve(spring: SpringDescription, velocity: f64, epsilon: Option<f64>) -> FnCurve {
    let curve = solve_spring(spring.to_spring(), 0., 1., velocity);
    let duration = curve.settle_time(epsilon.unwrap_or(SETTLING_EPSILON));
    FnCurve::new(
        duration,
        move |t| if t >= duration { 1. } else { curve.value(t) },
        move |t| if t >= duration { 0. } else { curve.velocity(t) },
    )
}

/// The curve of `f(driver(t))`, its velocity by the chain rule (upstream
/// `mapProgress`): a channel following a path as its progress moves, or a
/// companion following any driving curve.
pub fn map_progress(driver: SharedCurve, f: impl Fn(f64) -> f64 + 'static) -> FnCurve {
    let f = Rc::new(f);
    let (value_driver, speed_driver) = (driver.clone(), driver.clone());
    let speed_f = f.clone();
    FnCurve::new(
        driver.duration(),
        move |t| f(value_driver.at(t)),
        move |t| {
            let value = speed_driver.at(t);
            (speed_f(value + STEP) - speed_f(value - STEP)) / (2. * STEP) * speed_driver.velocity(t)
        },
    )
}

/// The linear map from `input` to `output`, held at `output`'s ends outside
/// `input` unless `clamp` is false (upstream `mapRange`). Either range may
/// run backward.
pub fn map_range(input: [f64; 2], output: [f64; 2], clamp: bool) -> impl Fn(f64) -> f64 + Clone {
    let [a, b] = input;
    let [c, d] = output;
    move |x| {
        let mut t = if a == b {
            if x < a { 0. } else { 1. }
        } else {
            (x - a) / (b - a)
        };
        if clamp {
            t = t.clamp(0., 1.);
        }
        if t == 0. {
            c
        } else if t == 1. {
            d
        } else {
            c + (d - c) * t
        }
    }
}

/// One channel of a path: a property and its value at each progress
/// (upstream `ProgressChannel`).
#[derive(Clone)]
pub struct ProgressChannel {
    pub property: MotionProperty,
    /// The property's value at progress `p`.
    pub at: Rc<dyn Fn(f64) -> f64>,
    /// What one unit of the property weighs in the projection, in a unit the
    /// channels share (px per unit of scale for a box that wide); 1 by
    /// default.
    pub weight: f64,
}

impl ProgressChannel {
    /// A channel with weight 1.
    pub fn new(property: MotionProperty, at: impl Fn(f64) -> f64 + 'static) -> Self {
        Self {
            property,
            at: Rc::new(at),
            weight: 1.,
        }
    }

    /// With a weight.
    pub fn weight(mut self, weight: f64) -> Self {
        self.weight = weight;
        self
    }
}

/// The progress velocity, per second, that continues what is presented
/// along a path that starts there (upstream `presentedProgressVelocity`):
/// `get_velocity` reads a property's presented velocity at the one time the
/// caller reads them all. 0 when the path moves none of its channels.
pub fn presented_progress_velocity(
    get_velocity: impl Fn(MotionProperty) -> f64,
    channels: &[ProgressChannel],
) -> f64 {
    let mut along = 0.;
    let mut squared = 0.;
    for channel in channels {
        let tangent = ((channel.at)(STEP) - (channel.at)(0.)) / STEP * channel.weight;
        along += get_velocity(channel.property) * channel.weight * tangent;
        squared += tangent * tangent;
    }
    if squared > 1e-12 { along / squared } else { 0. }
}

/// [`presented_progress_velocity`] of what `engine` presents at `now`.
pub fn presented_progress_velocity_of(
    engine: &MotionEngine,
    channels: &[ProgressChannel],
    now: Option<f64>,
) -> f64 {
    presented_progress_velocity(
        |property| engine.get_velocity(MotionKey::from(property), now),
        channels,
    )
}
