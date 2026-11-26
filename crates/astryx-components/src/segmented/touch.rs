//! Which segment a pointer counts for, upstream `useSegmentedTouchTracking.ts`
//! (`segmentTargetAt`, `liftTargetAt`), and how the selected face follows a held lens
//! (`useSegmentedSelectionMotion.ts`'s `followLens`, FR12). Coordinates are the track's.

/// A segment as measured when the finger landed.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct SegmentSpan {
    pub left: f32,
    pub right: f32,
    /// The box's top and bottom, when measured.
    pub top: Option<f32>,
    pub bottom: Option<f32>,
    /// The box grown by the segment's touch hit band, when measured.
    pub band: Option<Area>,
    pub disabled: bool,
}

/// A box: left, top, right, bottom.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Area {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

impl Area {
    fn holds(&self, x: f32, y: f32) -> bool {
        x >= self.left && x <= self.right && y >= self.top && y <= self.bottom
    }
}

/// Upstream `segmentTargetAt`: the segment a finger at `x` counts for: the one under
/// it, else the nearest (a gap splits at its middle, as UIKit's segments share an edge;
/// past the ends, the end segment). `None` when that segment is disabled.
pub(crate) fn segment_target_at(x: f32, segments: &[SegmentSpan]) -> Option<usize> {
    let mut best: Option<usize> = None;
    let mut distance = f32::INFINITY;
    for (i, s) in segments.iter().enumerate() {
        let d = (s.left - x).max(0.).max(x - s.right);
        if d < distance {
            distance = d;
            best = Some(i);
        }
    }
    best.filter(|&i| !segments[i].disabled)
}

/// Upstream `liftTargetAt`: the segment a lift at (`x`, `y`) commits: inside any box of
/// the control's lift `area`, the segment whose own box, else whose touch band, holds the
/// `finger`, else the one [`segment_target_at`] counts `x` for; outside them, none. The
/// tracker passes (`x`, `y`) on the control's center line, so a drift off the control keeps
/// choosing by its horizontal position.
pub(crate) fn lift_target_at(
    x: f32,
    y: f32,
    area: &[Area],
    segments: &[SegmentSpan],
    finger: Option<(f32, f32)>,
) -> Option<usize> {
    if !area.iter().any(|b| b.holds(x, y)) {
        return None;
    }
    let (fx, fy) = finger.unwrap_or((x, y));
    // A segment's own box first, then its band, so a band never takes a press that
    // lands on a neighbor.
    let own = segments.iter().position(|s| match (s.top, s.bottom) {
        (Some(top), Some(bottom)) => Area {
            left: s.left,
            top,
            right: s.right,
            bottom,
        }
        .holds(fx, fy),
        _ => false,
    });
    let under = own.or_else(|| {
        segments
            .iter()
            .position(|s| s.band.is_some_and(|b| b.holds(fx, fy)))
    });
    if let Some(i) = under {
        return (!segments[i].disabled).then_some(i);
    }
    segment_target_at(x, segments)
}

/// The face's hysteresis around a boundary: a quarter of the segment it moves to, at
/// least 8 px.
const FACE_HYSTERESIS_RATIO: f32 = 0.25;
const FACE_HYSTERESIS_MIN_PX: f32 = 8.;

/// Upstream `followLens` (FR12): the segment that shows the selected face for a held
/// lens centered at `center`, from the one that shows it now. It moves to a neighbor
/// only once the center is past the boundary by the hysteresis, so a finger resting on
/// a boundary never flips it. Disabled segments never take the face.
pub(crate) fn follow_face(current: usize, center: f32, segments: &[SegmentSpan]) -> usize {
    let mut spans: Vec<(usize, SegmentSpan)> = segments
        .iter()
        .copied()
        .enumerate()
        .filter(|(_, s)| !s.disabled)
        .collect();
    spans.sort_by(|a, b| a.1.left.total_cmp(&b.1.left));
    let Some(at) = spans.iter().position(|(i, _)| *i == current) else {
        return current;
    };
    let edge = |i: usize| (spans[i].1.right + spans[i + 1].1.left) / 2.;
    let mut next = at;
    while next + 1 < spans.len() && center > edge(next) {
        next += 1;
    }
    while next > 0 && center < edge(next - 1) {
        next -= 1;
    }
    if next == at {
        return current;
    }
    let boundary = if next > at { edge(at) } else { edge(at - 1) };
    let width = spans[next].1.right - spans[next].1.left;
    let hysteresis = FACE_HYSTERESIS_MIN_PX.max(FACE_HYSTERESIS_RATIO * width);
    if (center - boundary).abs() >= hysteresis {
        spans[next].0
    } else {
        current
    }
}
