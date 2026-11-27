//! A motion role at the theme's tempo, upstream `motion/resolveMotion.ts`.
//!
//! Every spring and duration the animator plays passes through here, so the
//! role a value plays is named in one place. The tempo stretches a spring
//! and scales a duration; a curve, a part that follows a finger and the busy
//! loop play as measured. At tempo 1 every role plays exactly its table
//! value, and a value that is no registered role comes back unchanged.

use super::linear_easing::{linear_easing, linear_easing_stops};
use super::roles::{motion_role_in, motion_role_of};
use super::{MotionSpring, SpringDescription, settling_duration, solve_spring};

/// The roles the tempo leaves as measured: the parts that follow a finger,
/// and ProgressBar's busy loop (upstream `FIXED_ROLES`).
pub const FIXED_ROLES: [&str; 3] = [
    "springs.fingerTracking",
    "switchControl.tracking",
    "progress.reducedMotionPulseMs",
];

/// `spring` at `tempo`: in each form the undamped period grows by the
/// tempo and the damping ratio stays, so the motion keeps its shape and
/// takes `tempo` times as long.
pub fn spring_at_tempo(spring: SpringDescription, tempo: f64) -> SpringDescription {
    match spring {
        SpringDescription::ResponseDamping {
            response,
            damping_ratio,
        } => SpringDescription::ResponseDamping {
            response: response * tempo,
            damping_ratio,
        },
        SpringDescription::DurationBounce { duration, bounce } => {
            SpringDescription::DurationBounce {
                duration: duration * tempo,
                bounce,
            }
        }
        SpringDescription::Physics {
            mass,
            stiffness,
            damping,
        } => SpringDescription::Physics {
            mass,
            stiffness: stiffness / (tempo * tempo),
            damping: damping / tempo,
        },
    }
}

/// A spring role's value at `tempo` (upstream `resolveMotion` for a
/// spring); a spring that is no registered role, or a fixed one, plays as
/// given.
pub fn resolve_spring(spring: MotionSpring, tempo: f64) -> SpringDescription {
    match motion_role_of(spring) {
        Some(role) if !FIXED_ROLES.contains(&role.id.as_str()) && tempo != 1. => {
            spring_at_tempo(spring.spring, tempo)
        }
        _ => spring.spring,
    }
}

/// A duration role's value at `tempo` (upstream `resolveMotion(element,
/// group, key)` for a `…Ms` key): `role` is the value's table path, such
/// as `timing.stateChangeMs`. A duration that is no role plays as given.
pub fn resolve_duration_ms(role: Option<&str>, ms: f64, tempo: f64) -> f64 {
    match role.and_then(motion_role_in) {
        Some(role) if !FIXED_ROLES.contains(&role.id.as_str()) && tempo != 1. => ms * tempo,
        _ => ms,
    }
}

/// One move a spring plays, as the animator plays it (upstream
/// `SpringMove`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpringMove {
    pub from: f64,
    pub to: f64,
    /// The start velocity, units/s.
    pub velocity: f64,
    /// The move is over once it stays this close to `to`.
    pub epsilon: f64,
}

/// How long `spring` takes to settle at `tempo`, ms (upstream
/// `resolveSettleMs`): the resolved spring's settling duration, or with
/// `motion`, how long that move plays, as the animator times it.
pub fn resolve_settle_ms(spring: MotionSpring, tempo: f64, motion: Option<SpringMove>) -> f64 {
    let resolved = resolve_spring(spring, tempo).to_spring();
    match motion {
        None => settling_duration(resolved) * 1000.,
        Some(m) => solve_spring(resolved, m.from, m.to, m.velocity).settle_time(m.epsilon) * 1000.,
    }
}

/// How one move plays (upstream `resolveMoveTiming`'s result).
#[derive(Clone, Debug, PartialEq)]
pub struct MoveTiming {
    pub duration_ms: f64,
    /// The progress over the duration as CSS `linear()` easing.
    pub easing: String,
    /// The same progress, one stop per frame at 60 Hz.
    pub stops: Vec<f64>,
}

/// How `motion` on `spring` plays at `tempo`: its duration and its progress
/// toward `done_at` (default `motion.to`), which reaches 1 the first time the
/// move gets there and stays at 1 (upstream `resolveMoveTiming`).
pub fn resolve_move_timing(
    spring: MotionSpring,
    tempo: f64,
    motion: SpringMove,
    done_at: Option<f64>,
) -> MoveTiming {
    let curve = solve_spring(
        resolve_spring(spring, tempo).to_spring(),
        motion.from,
        motion.to,
        motion.velocity,
    );
    let duration_ms = curve.settle_time(motion.epsilon) * 1000.;
    let span = done_at.unwrap_or(motion.to) - motion.from;
    if span == 0. || duration_ms == 0. {
        return MoveTiming {
            duration_ms,
            easing: linear_easing(duration_ms, |_| 1.),
            stops: linear_easing_stops(duration_ms, |_| 1.),
        };
    }
    let progress_at = |t: f64| (curve.value(t / 1000.) - motion.from) / span;
    let mut reached_at = f64::INFINITY;
    let mut t = 0.;
    while t <= duration_ms {
        if progress_at(t) >= 1. {
            reached_at = t;
            break;
        }
        t += 1.;
    }
    let done = |t: f64| if t >= reached_at { 1. } else { progress_at(t) };
    MoveTiming {
        duration_ms,
        easing: linear_easing(duration_ms, done),
        stops: linear_easing_stops(duration_ms, done),
    }
}
