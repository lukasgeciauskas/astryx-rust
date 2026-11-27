//! The touch wheel's motion, upstream `DateInput/wheelPhysics.ts`: the
//! release plan, the turn's spring, the snap, the coast and the rubber band.
//! Offsets are px (row k is under the band at k rows), times seconds.

use rustryx_primitives::motion::uikit::{picker, scroll};
use rustryx_primitives::motion::{
    DecayCurve, SpringCurve, SpringDescription, project_distance, rubber_band, solve_decay,
    solve_spring,
};
use rustryx_theme::CubicBezier;

/// Below half a device pixel at 3×, a motion is on its row (pt).
const AT_REST_PT: f64 = 1. / 6.;
/// The longest a plan runs, s (the animator's limit).
pub(crate) const MAX_PLAN_SECONDS: f64 = 30.;

/// The rows a wheel moves over.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct WheelLayout {
    pub row_height: f64,
    pub row_count: usize,
    /// The rows repeat without end; the offset is unbounded.
    pub looping: bool,
}

/// The picker constants' scale for rows `row_height` px tall.
pub(crate) fn scale_for(row_height: f64) -> f64 {
    row_height / picker::ROW_HEIGHT
}

/// The offsets of the first and last rows.
pub(crate) fn offset_bounds(layout: &WheelLayout) -> (f64, f64) {
    if layout.looping {
        return (f64::NEG_INFINITY, f64::INFINITY);
    }
    (
        0.,
        layout.row_count.saturating_sub(1) as f64 * layout.row_height,
    )
}

/// The rubber band's dimension for the layout.
fn band_dimension(layout: &WheelLayout) -> f64 {
    picker::RUBBER_BAND_DIMENSION * scale_for(layout.row_height)
}

/// The offset a drag shows for the finger's `offset`: banded past an end.
pub(crate) fn drag_offset(offset: f64, layout: &WheelLayout) -> f64 {
    let (min, max) = offset_bounds(layout);
    let dimension = band_dimension(layout);
    if offset < min {
        min + rubber_band(offset - min, dimension)
    } else if offset > max {
        max + rubber_band(offset - max, dimension)
    } else {
        offset
    }
}

/// The finger's offset for a shown one: past an end, the travel the band
/// shows as `shown`, so a drag that starts in a bounce does not jump.
pub(crate) fn finger_offset(shown: f64, layout: &WheelLayout) -> f64 {
    use rustryx_primitives::motion::rubber_band_inverse;
    let (min, max) = offset_bounds(layout);
    let dimension = band_dimension(layout);
    if shown < min {
        min + rubber_band_inverse(shown - min, dimension)
    } else if shown > max {
        max + rubber_band_inverse(shown - max, dimension)
    } else {
        shown
    }
}

/// The position of the row nearest `offset`, clamped unless looping.
pub(crate) fn nearest_index(offset: f64, layout: &WheelLayout) -> i64 {
    let index = (offset / layout.row_height).round() as i64;
    if layout.looping {
        return index;
    }
    index.clamp(0, layout.row_count.saturating_sub(1) as i64)
}

/// A motion of the wheel's offset from t = 0 to its duration.
#[derive(Clone, Copy, Debug)]
pub(crate) enum WheelCurve {
    /// Already at rest.
    Rest(f64),
    /// UIKit's exponential ease onto a row.
    Snap {
        to: f64,
        distance: f64,
        rate: f64,
        duration: f64,
    },
    /// A coast that keeps the release velocity and stops on `to`.
    Coast {
        from: f64,
        to: f64,
        direction: f64,
        speed: f64,
        lambda: f64,
        duration: f64,
    },
    /// A spring onto `to`.
    Spring { curve: SpringCurve, duration: f64 },
    /// A decay into an end, then the bounce spring from there.
    Edge {
        decay: DecayCurve,
        hit: f64,
        bounce: SpringCurve,
        edge: f64,
        duration: f64,
    },
    /// Core Animation's ease-in-ease-out (a column's smooth scroll).
    Ease { from: f64, to: f64, duration: f64 },
}

const EASE: CubicBezier = CubicBezier(0.42, 0., 0.58, 1.);

impl WheelCurve {
    pub fn duration(&self) -> f64 {
        match *self {
            Self::Rest(_) => 0.,
            Self::Snap { duration, .. }
            | Self::Coast { duration, .. }
            | Self::Spring { duration, .. }
            | Self::Edge { duration, .. }
            | Self::Ease { duration, .. } => duration,
        }
    }

    pub fn to(&self) -> f64 {
        match *self {
            Self::Rest(to)
            | Self::Snap { to, .. }
            | Self::Coast { to, .. }
            | Self::Ease { to, .. } => to,
            Self::Spring { curve, .. } => curve.to,
            Self::Edge { edge, .. } => edge,
        }
    }

    /// The offset at `t` seconds; `to` from the duration on.
    pub fn value(&self, t: f64) -> f64 {
        if t >= self.duration() {
            return self.to();
        }
        match *self {
            Self::Rest(to) => to,
            Self::Snap {
                to, distance, rate, ..
            } => to + distance * (-rate * t).exp(),
            Self::Coast {
                from,
                direction,
                speed,
                lambda,
                ..
            } => from + direction * speed * -(-lambda * t).exp_m1() / lambda,
            Self::Spring { curve, .. } => curve.value(t),
            Self::Edge {
                decay, hit, bounce, ..
            } => {
                if t < hit {
                    decay.value(t)
                } else {
                    bounce.value(t - hit)
                }
            }
            Self::Ease { from, to, duration } => {
                from + (to - from) * f64::from(EASE.ease((t / duration) as f32))
            }
        }
    }
}

/// UIKit's ease onto `to` after a release without velocity.
pub(crate) fn snap_curve(from: f64, to: f64, scale: f64) -> WheelCurve {
    let distance = from - to;
    let rate = picker::SNAP_RATE * 1000.;
    let epsilon = AT_REST_PT * scale;
    if distance.abs() <= epsilon {
        return WheelCurve::Rest(to);
    }
    WheelCurve::Snap {
        to,
        distance,
        rate,
        duration: (distance.abs() / epsilon).ln() / rate,
    }
}

/// A coast from `from` at `velocity` that ends on `to` at the stop speed.
pub(crate) fn coast_curve(from: f64, velocity: f64, to: f64, scale: f64) -> WheelCurve {
    let stop = picker::STOP_SPEED * 1000. * scale;
    let distance = to - from;
    let speed = velocity.abs();
    if distance == 0. || distance.signum() != velocity.signum() || speed <= stop {
        return snap_curve(from, to, scale);
    }
    let lambda = (speed - stop) / distance.abs();
    WheelCurve::Coast {
        from,
        to,
        direction: distance.signum(),
        speed,
        lambda,
        duration: (speed / stop).ln() / lambda,
    }
}

/// A spring from `from` with `velocity` onto `to`: a return past an end
/// (`scroll.bounce`) or a turn from rest (`springs.settle`).
pub(crate) fn bounce_curve(
    from: f64,
    velocity: f64,
    to: f64,
    scale: f64,
    spring: SpringDescription,
) -> WheelCurve {
    let curve = solve_spring(spring.to_spring(), from, to, velocity);
    WheelCurve::Spring {
        curve,
        duration: MAX_PLAN_SECONDS.min(curve.settle_time(AT_REST_PT * scale)),
    }
}

/// A coast into an end: the decay to `edge`, then the bounce.
pub(crate) fn edge_curve(
    from: f64,
    velocity: f64,
    edge: f64,
    scale: f64,
    spring: SpringDescription,
) -> WheelCurve {
    let decay = solve_decay(from, velocity, None);
    let Some(hit) = decay.time_to_reach(edge) else {
        return bounce_curve(from, velocity, edge, scale, spring);
    };
    let bounce = solve_spring(spring.to_spring(), edge, edge, decay.velocity(hit));
    let duration = MAX_PLAN_SECONDS.min(hit + bounce.settle_time(AT_REST_PT * scale));
    WheelCurve::Edge {
        decay,
        hit,
        bounce,
        edge,
        duration,
    }
}

/// Where the scroll view projects a release, short of the stop speed.
pub(crate) fn project_release(offset: f64, velocity: f64, scale: f64) -> f64 {
    let stop = picker::STOP_SPEED * 1000. * scale;
    let speed = velocity.abs();
    if speed <= stop {
        return offset;
    }
    offset + velocity.signum() * project_distance(speed - stop, scroll::DECELERATION_NORMAL)
}

/// How a release moves the wheel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PlanKind {
    Coast,
    Snap,
    Edge,
    Return,
}

/// What a release does; its curve ends on the row it rests on.
#[derive(Clone, Copy, Debug)]
pub(crate) struct WheelPlan {
    pub kind: PlanKind,
    pub curve: WheelCurve,
}

/// What the wheel does when the finger lifts at `offset` with `velocity`
/// (px/s, positive when the rows move up), as UIKit does (`planRelease`).
pub(crate) fn plan_release(offset: f64, velocity: f64, layout: &WheelLayout) -> WheelPlan {
    let spring = scroll::BOUNCE;
    let scale = scale_for(layout.row_height);
    let (min, max) = offset_bounds(layout);
    if offset < min || offset > max {
        let edge = if offset < min { min } else { max };
        return WheelPlan {
            kind: PlanKind::Return,
            curve: bounce_curve(offset, velocity, edge, scale, spring),
        };
    }
    let minimum = picker::MINIMUM_SPEED * 1000. * scale;
    let speed = if velocity.abs() < minimum {
        0.
    } else {
        velocity
    };
    let projected = project_release(offset, speed, scale);
    if projected < min || projected > max {
        let edge = if projected < min { min } else { max };
        return WheelPlan {
            kind: PlanKind::Edge,
            curve: edge_curve(offset, speed, edge, scale, spring),
        };
    }
    let index = nearest_index(projected, layout);
    let target = index as f64 * layout.row_height;
    if speed == 0. {
        return WheelPlan {
            kind: PlanKind::Snap,
            curve: snap_curve(offset, target, scale),
        };
    }
    let curve = coast_curve(offset, speed, target, scale);
    WheelPlan {
        kind: if curve.duration() > 0. && matches!(curve, WheelCurve::Coast { .. }) {
            PlanKind::Coast
        } else {
            PlanKind::Snap
        },
        curve,
    }
}

/// Upstream `reachesRow`: whether moving from offset `from` to `to` brings a row's centre onto
/// the band.
pub(crate) fn reaches_row(from: f64, to: f64, row_height: f64) -> bool {
    if to > from {
        (to / row_height).floor() > (from / row_height).floor()
    } else if to < from {
        (to / row_height).ceil() < (from / row_height).ceil()
    } else {
        false
    }
}

/// Upstream `tickTimes`: when, in seconds, a motion on `curve` brings a row onto the band,
/// at most one tick per picker selection-tick interval.
pub(crate) fn tick_times(curve: &WheelCurve, row_height: f64) -> Vec<f64> {
    let interval = rustryx_primitives::motion::uikit::picker::SELECTION_TICK_INTERVAL_MS / 1000.;
    let (step, until) = (1. / 240., curve.duration());
    let mut times = Vec::new();
    let mut previous = curve.value(0.);
    let mut last = f64::NEG_INFINITY;
    let mut t = step;
    while t < until + step {
        let time = t.min(until);
        let value = curve.value(time);
        if reaches_row(previous, value, row_height) && time - last >= interval {
            times.push(time);
            last = time;
        }
        previous = value;
        if time == until {
            break;
        }
        t += step;
    }
    times
}
