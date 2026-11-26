//! Where a release comes to rest, upstream `motion/projection.ts` (WWDC18).

/// The distance a surface released at `velocity` units/s coasts.
pub fn project_distance(velocity: f64, rate: f64) -> f64 {
    assert!(
        rate > 0. && rate < 1.,
        "projection: rate must be in (0, 1), got {rate}"
    );
    velocity / 1000. * rate / (1. - rate)
}

/// Where a surface released at `position` with `velocity` comes to rest.
pub fn project(position: f64, velocity: f64, rate: f64) -> f64 {
    position + project_distance(velocity, rate)
}

/// The rest point a release goes to (upstream `SnapSelection`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SnapSelection {
    /// Index of the chosen point in the list passed in.
    pub index: usize,
    /// The chosen point.
    pub value: f64,
    /// The projected rest position the choice was made from.
    pub projected: f64,
}

/// What [`select_snap_point`] reads of a release.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SnapRelease {
    pub position: f64,
    pub velocity: f64,
    /// The deceleration rate; UIScrollView's normal rate by default.
    pub rate: f64,
    /// With `max_steps`, the point the release starts from.
    pub from_index: Option<usize>,
    /// How many points away from `from_index` one release may travel.
    pub max_steps: Option<usize>,
}

impl SnapRelease {
    /// A release at `position` moving at `velocity`, at the normal rate.
    pub fn new(position: f64, velocity: f64) -> Self {
        Self {
            position,
            velocity,
            rate: super::uikit::scroll::DECELERATION_NORMAL,
            from_index: None,
            max_steps: None,
        }
    }
}

/// The point nearest the projected rest position (upstream
/// `selectSnapPoint`); ties go to the point in the direction of travel.
/// `None` for no points (upstream throws).
pub fn select_snap_point(points: &[f64], release: SnapRelease) -> Option<SnapSelection> {
    if points.is_empty() {
        return None;
    }
    let projected = project(release.position, release.velocity, release.rate);
    let (mut lo, mut hi) = (0usize, points.len() - 1);
    if let (Some(from), Some(steps)) = (release.from_index, release.max_steps) {
        let from = from.min(points.len() - 1);
        lo = lo.max(from.saturating_sub(steps));
        hi = hi.min(from + steps);
    }
    let sign = |v: f64| {
        if v > 0. {
            1.
        } else if v < 0. {
            -1.
        } else {
            0.
        }
    };
    let mut best = lo;
    let mut best_distance = f64::INFINITY;
    for i in lo..=hi {
        let distance = (points[i] - projected).abs();
        let is_tie = (distance - best_distance).abs() < 1e-9;
        let is_ahead = sign(points[i] - points[best]) == sign(release.velocity);
        if distance < best_distance - 1e-9 || (is_tie && is_ahead) {
            best = i;
            best_distance = distance;
        }
    }
    Some(SnapSelection {
        index: best,
        value: points[best],
        projected,
    })
}
