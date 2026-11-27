//! Where a release lands, upstream `motion/resolveSnapPoint.ts`: the release
//! policy on [`select_snap_point`], the math primitive.
//!
//! - A slow release lands on the point nearest where it would rest: its
//!   position, or with `projection`, its position plus the distance it would
//!   coast (below `projection.min_speed` nothing is projected).
//! - A flick (`flick_threshold` or faster) lands at least on the next point
//!   beyond its position in its direction (a point within `tolerance` counts
//!   as reached), or with `flick_past_projection`, on the first point at or
//!   past its projection.
//! - With `start`, a release moving back toward it cancels to the point
//!   nearest `start`.
//! - With `from_index` and `max_steps`, it lands at most `max_steps` points
//!   from `from_index`.

use super::projection::{SnapRelease, select_snap_point};

/// Where a release lands (upstream `SnapPointRelease`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SnapPointRelease {
    /// Index into the points passed in.
    pub index: usize,
    /// The rest position the choice was judged from.
    pub projected: f64,
}

/// A release's projection (upstream `SnapPointProjection`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SnapPointProjection {
    /// Seconds of the release velocity the projection reaches.
    pub seconds: f64,
    /// Below this speed (units/s), nothing is projected.
    pub min_speed: f64,
}

/// What [`resolve_snap_point`] reads.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct SnapPointQuery {
    /// Snap point positions, ascending.
    pub points: Vec<f64>,
    pub position: f64,
    pub velocity: f64,
    /// From this speed (units/s) on, a release is a flick.
    pub flick_threshold: f64,
    /// A point this close to the position counts as reached by a flick.
    pub tolerance: f64,
    /// A flick lands on the first point at or past its projection.
    pub flick_past_projection: bool,
    pub projection: Option<SnapPointProjection>,
    /// Where the drag began: a release moving back toward it cancels to it.
    pub start: Option<f64>,
    pub from_index: Option<usize>,
    pub max_steps: Option<usize>,
}

/// The deceleration rate whose projection reaches `seconds` of the velocity.
fn rate_for_projection(seconds: f64) -> f64 {
    let ms = seconds * 1000.;
    ms / (1. + ms)
}

fn nearest_index(points: &[f64], position: f64) -> usize {
    let mut best = 0;
    for i in 1..points.len() {
        if (points[i] - position).abs() < (points[best] - position).abs() {
            best = i;
        }
    }
    best
}

fn sign(v: f64) -> f64 {
    if v > 0. {
        1.
    } else if v < 0. {
        -1.
    } else {
        0.
    }
}

/// The snap point a release lands on (upstream `resolveSnapPoint`); `None`
/// for no points (upstream throws).
pub fn resolve_snap_point(query: &SnapPointQuery) -> Option<SnapPointRelease> {
    let points = &query.points;
    if points.is_empty() {
        return None;
    }
    let velocity = query.velocity;
    let projected_velocity = match query.projection {
        Some(projection) if velocity.abs() >= projection.min_speed => velocity,
        _ => 0.,
    };
    let choice = select_snap_point(
        points,
        SnapRelease {
            position: query.position,
            velocity: projected_velocity,
            rate: query
                .projection
                .map_or(super::uikit::scroll::DECELERATION_NORMAL, |p| {
                    rate_for_projection(p.seconds)
                }),
            from_index: query.from_index,
            max_steps: query.max_steps,
        },
    )?;
    let mut index = choice.index as isize;
    let position = query.position;
    let tolerance = query.tolerance;
    if velocity.abs() >= query.flick_threshold {
        let next = if velocity > 0. {
            points
                .iter()
                .position(|d| *d > position + tolerance)
                .map(|i| i as isize)
        } else {
            points
                .iter()
                .rposition(|d| *d < position - tolerance)
                .map(|i| i as isize)
        };
        if let Some(next) = next
            && (if velocity > 0. {
                next > index
            } else {
                next < index
            })
        {
            index = next;
        }
        if query.flick_past_projection {
            let projected = choice.projected;
            let past = if velocity > 0. {
                points
                    .iter()
                    .position(|d| *d >= projected - tolerance)
                    .unwrap_or(points.len() - 1)
            } else {
                points
                    .iter()
                    .rposition(|d| *d <= projected + tolerance)
                    .unwrap_or(0)
            } as isize;
            let ahead = if velocity > 0. {
                past > index
            } else {
                past < index
            };
            if ahead {
                index = past;
            }
        }
    }
    if let Some(start) = query.start
        && velocity != 0.
        && sign(velocity) == sign(start - position)
    {
        index = nearest_index(points, start) as isize;
    }
    if let (Some(from), Some(steps)) = (query.from_index, query.max_steps) {
        let from = from.min(points.len() - 1) as isize;
        index = index.clamp(from - steps as isize, from + steps as isize);
    }
    Some(SnapPointRelease {
        index: index.clamp(0, points.len() as isize - 1) as usize,
        projected: choice.projected,
    })
}
