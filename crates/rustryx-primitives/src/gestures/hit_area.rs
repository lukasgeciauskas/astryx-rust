//! The touch hit area's geometry, upstream `utils/hitArea.ts` (`clampedHitInsets`,
//! `resolveHitTarget`, `groupGrowth`): how far a target may grow toward 44 px on each
//! side without taking a tap that is nearer a neighbour.
//!
//! A finger needs a 44 × 44 px target, but a control may not grow on a phone, and its
//! hit area must never take a neighbour's taps. Each side grows toward the minimum and
//! stops at half the gap to the nearest neighbour on that side, so a tap always goes to
//! the nearer target.

/// A rect in window px.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct HitRect {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

impl HitRect {
    pub fn new(left: f32, top: f32, right: f32, bottom: f32) -> Self {
        HitRect {
            left,
            top,
            right,
            bottom,
        }
    }

    /// From GPUI bounds.
    pub fn from_bounds(b: gpui::Bounds<gpui::Pixels>) -> Self {
        HitRect {
            left: f32::from(b.origin.x),
            top: f32::from(b.origin.y),
            right: f32::from(b.origin.x + b.size.width),
            bottom: f32::from(b.origin.y + b.size.height),
        }
    }

    /// Whether it holds (x, y), edges included.
    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.left && x <= self.right && y >= self.top && y <= self.bottom
    }

    /// Whether it has an area.
    pub fn has_area(&self) -> bool {
        self.right > self.left && self.bottom > self.top
    }

    /// Grown by `insets` on each side.
    pub fn grown(&self, insets: HitInsets) -> HitRect {
        HitRect {
            left: self.left - insets.left,
            top: self.top - insets.top,
            right: self.right + insets.right,
            bottom: self.bottom + insets.bottom,
        }
    }

    fn intersects(&self, b: &HitRect) -> bool {
        self.left < b.right && b.left < self.right && self.top < b.bottom && b.top < self.bottom
    }

    fn distance_to(&self, x: f32, y: f32) -> f32 {
        let dx = (self.left - x).max(0.).max(x - self.right);
        let dy = (self.top - y).max(0.).max(y - self.bottom);
        dx.hypot(dy)
    }
}

/// How far a hit area extends past its target on each side, px.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct HitInsets {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

/// Upstream `MIN_TOUCH_TARGET`: the smallest touch target, px (Apple's 44 pt).
pub const MIN_TOUCH_TARGET: f32 = 44.;

/// Upstream `NO_HIT_INSETS`.
pub const NO_HIT_INSETS: HitInsets = HitInsets {
    top: 0.,
    right: 0.,
    bottom: 0.,
    left: 0.,
};

/// The gap between two rects on each axis: positive apart, negative overlapping.
fn gaps(a: &HitRect, b: &HitRect) -> (f32, f32) {
    (
        (b.left - a.right).max(a.left - b.right),
        (b.top - a.bottom).max(a.top - b.bottom),
    )
}

/// How far `rect` grows toward `min_size` on each axis, centered, unclamped.
pub fn growth(rect: &HitRect, min_size: f32) -> HitInsets {
    let x = ((min_size - (rect.right - rect.left)) / 2.).max(0.);
    let y = ((min_size - (rect.bottom - rect.top)) / 2.).max(0.);
    HitInsets {
        top: y,
        right: x,
        bottom: y,
        left: x,
    }
}

/// Upstream `groupGrowth`: how far a group's box grows on each side before any
/// neighbour stops it, as far as its members on that edge grow toward `min_size`; a side
/// that reaches past its edge member shrinks to that member's grown edge (negative).
pub fn group_growth(group: &HitRect, members: &[HitRect], min_size: f32) -> HitInsets {
    if members.is_empty() {
        return NO_HIT_INSETS;
    }
    let mut insets = HitInsets {
        top: f32::NEG_INFINITY,
        right: f32::NEG_INFINITY,
        bottom: f32::NEG_INFINITY,
        left: f32::NEG_INFINITY,
    };
    for member in members {
        let own = growth(member, min_size);
        insets.top = insets.top.max(own.top - (member.top - group.top));
        insets.right = insets.right.max(own.right - (group.right - member.right));
        insets.bottom = insets
            .bottom
            .max(own.bottom - (group.bottom - member.bottom));
        insets.left = insets.left.max(own.left - (member.left - group.left));
    }
    insets
}

/// Upstream `clampedHitInsets`: how far `target` may grow on each side, toward
/// `min_size` and on each side no further than half the gap to a neighbour that either
/// one would reach into (a diagonal neighbour stops the side with the larger gap). A
/// neighbour overlapping the target (an ancestor, an overlay) stops nothing. `full`
/// replaces the centered growth (a group's box).
pub fn clamped_hit_insets(
    target: &HitRect,
    neighbours: &[HitRect],
    min_size: f32,
    full: Option<HitInsets>,
) -> HitInsets {
    let full = full.unwrap_or_else(|| growth(target, min_size));
    if full == NO_HIT_INSETS {
        return NO_HIT_INSETS;
    }
    let mut insets = full;
    for rect in neighbours {
        let (gx, gy) = gaps(target, rect);
        if gx < 0. && gy < 0. {
            continue;
        }
        let half = HitInsets {
            top: gy.max(0.) / 2.,
            right: gx.max(0.) / 2.,
            bottom: gy.max(0.) / 2.,
            left: gx.max(0.) / 2.,
        };
        let reach = target.grown(full);
        let neighbour_reach = rect.grown(growth(rect, min_size));
        let crosses = reach.intersects(&rect.grown(half))
            || neighbour_reach.intersects(&target.grown(half))
            || reach.intersects(&neighbour_reach);
        if !crosses {
            continue;
        }
        if gx >= gy {
            if rect.left >= target.right {
                insets.right = insets.right.min(half.right);
            } else {
                insets.left = insets.left.min(half.left);
            }
        } else if rect.top >= target.bottom {
            insets.bottom = insets.bottom.min(half.bottom);
        } else {
            insets.top = insets.top.min(half.top);
        }
    }
    insets
}

/// Upstream `resolveHitTarget`: which of `rects` a touch at (x, y) is for: the nearest
/// (inside overlapping ones, the one whose center is nearer), when the touch is within
/// its growth toward `min_size`; `None` otherwise.
pub fn resolve_hit_target(x: f32, y: f32, rects: &[HitRect], min_size: f32) -> Option<usize> {
    let mut nearest = None;
    let mut nearest_distance = f32::INFINITY;
    for (index, rect) in rects.iter().enumerate() {
        let center = (x - (rect.left + rect.right) / 2.).hypot(y - (rect.top + rect.bottom) / 2.);
        let distance = rect.distance_to(x, y) * 1e6 + center;
        if distance < nearest_distance {
            nearest = Some(index);
            nearest_distance = distance;
        }
    }
    let index = nearest?;
    let rect = rects[index];
    rect.grown(growth(&rect, min_size))
        .contains(x, y)
        .then_some(index)
}

// ---------------------------------------------------------------------------
// The window's interactive targets (upstream `watchHitArea`'s neighbours)
// ---------------------------------------------------------------------------

/// How long a target drawn no more counts as a neighbour.
const STALE: std::time::Duration = std::time::Duration::from_millis(500);

type Targets = std::collections::HashMap<u64, (HitRect, web_time::Instant)>;

thread_local! {
    static TARGETS: std::cell::RefCell<std::collections::HashMap<gpui::WindowId, Targets>> =
        std::cell::RefCell::default();
}

/// Notes an interactive target's box this frame (in paint), under a coarse primary
/// pointer: the hit areas around it stop halfway to it. GPUI keeps no list of a page's
/// interactive elements (upstream reads `INTERACTIVE_SELECTOR`), so every pressable
/// surface, text field and hit-area owner notes itself here.
pub fn note_target(
    window: &gpui::Window,
    key: u64,
    bounds: gpui::Bounds<gpui::Pixels>,
    now: web_time::Instant,
) {
    let id = window.window_handle().window_id();
    TARGETS.with(|t| {
        let mut t = t.borrow_mut();
        let targets = t.entry(id).or_default();
        targets.insert(key, (HitRect::from_bounds(bounds), now));
        if targets.len() > 512 {
            targets.retain(|_, (_, seen)| now.saturating_duration_since(*seen) < STALE);
        }
    });
}

/// The box last noted for `key`, and the live neighbours around it.
pub fn target_and_neighbours(
    window: &gpui::Window,
    key: u64,
    now: web_time::Instant,
) -> Option<(HitRect, Vec<HitRect>)> {
    let id = window.window_handle().window_id();
    TARGETS.with(|t| {
        let t = t.borrow();
        let targets = t.get(&id)?;
        let own = targets.get(&key)?.0;
        let neighbours = targets
            .iter()
            .filter(|(k, (_, seen))| **k != key && now.saturating_duration_since(*seen) < STALE)
            .map(|(_, (rect, _))| *rect)
            .collect();
        Some((own, neighbours))
    })
}

/// A probe (absolute, its parent's full size) that notes the parent as an interactive
/// target under a coarse primary pointer.
pub fn target_probe(key: u64) -> impl gpui::IntoElement {
    use gpui::Styled;
    gpui::canvas(
        |_, _, _| {},
        move |bounds, _, window, cx| {
            if crate::coarse_primary_pointer(window, cx) {
                note_target(window, key, bounds, cx.background_executor().now());
            }
        },
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(left: f32, top: f32, right: f32, bottom: f32) -> HitRect {
        HitRect::new(left, top, right, bottom)
    }

    fn insets(top: f32, right: f32, bottom: f32, left: f32) -> HitInsets {
        HitInsets {
            top,
            right,
            bottom,
            left,
        }
    }

    #[test]
    fn grows_each_side_of_a_row_as_far_as_the_member_on_that_edge_would() {
        let group = r(0., 0., 100., 24.);
        let members = [r(0., 0., 24., 24.), r(76., 0., 100., 24.)];
        assert_eq!(
            group_growth(&group, &members, 44.),
            insets(10., 10., 10., 10.)
        );
        // A wide group by its narrow end members: its sides past them shrink.
        let wide = r(0., 0., 200., 24.);
        let members = [r(20., 0., 44., 24.), r(150., 0., 174., 24.)];
        let g = group_growth(&wide, &members, 44.);
        assert_eq!((g.left, g.right), (-10., -16.));
    }

    #[test]
    fn grows_a_lone_24_px_target_to_44_px_centered() {
        assert_eq!(
            clamped_hit_insets(&r(0., 0., 24., 24.), &[], 44., None),
            insets(10., 10., 10., 10.)
        );
    }

    #[test]
    fn does_not_grow_a_target_already_44_px_or_larger_on_an_axis() {
        assert_eq!(
            clamped_hit_insets(&r(0., 0., 60., 24.), &[], 44., None),
            insets(10., 0., 10., 0.)
        );
        assert_eq!(
            clamped_hit_insets(&r(0., 0., 44., 44.), &[], 44., None),
            NO_HIT_INSETS
        );
    }

    #[test]
    fn stops_at_half_the_gap_to_a_neighbour_on_that_side_and_only_that_side() {
        let got = clamped_hit_insets(&r(0., 0., 24., 24.), &[r(32., 0., 56., 24.)], 44., None);
        assert_eq!(got, insets(10., 4., 10., 10.));
    }

    #[test]
    fn does_not_grow_toward_a_neighbour_it_touches() {
        let got = clamped_hit_insets(&r(0., 0., 24., 24.), &[r(24., 0., 48., 24.)], 44., None);
        assert_eq!(got.right, 0.);
    }

    #[test]
    fn ignores_a_neighbour_at_least_twice_its_growth_away() {
        let got = clamped_hit_insets(&r(0., 0., 24., 24.), &[r(64., 0., 88., 24.)], 44., None);
        assert_eq!(got, insets(10., 10., 10., 10.));
    }

    #[test]
    fn stops_a_diagonal_neighbour_on_the_side_with_the_larger_gap() {
        let got = clamped_hit_insets(&r(0., 0., 24., 24.), &[r(30., 28., 54., 52.)], 44., None);
        // The x gap (6) is larger than the y gap (4): the right side stops at 3.
        assert_eq!(got.right, 3.);
        assert_eq!(got.bottom, 10.);
    }

    #[test]
    fn ignores_a_neighbour_that_overlaps_the_target_an_ancestor_or_an_overlay() {
        let got = clamped_hit_insets(&r(10., 10., 34., 34.), &[r(0., 0., 200., 200.)], 44., None);
        assert_eq!(got, insets(10., 10., 10., 10.));
    }

    #[test]
    fn gives_a_touch_in_the_gap_to_the_nearer_button() {
        let rects = [r(0., 0., 24., 24.), r(32., 0., 56., 24.)];
        assert_eq!(resolve_hit_target(27., 12., &rects, 44.), Some(0));
        assert_eq!(resolve_hit_target(29., 12., &rects, 44.), Some(1));
    }

    #[test]
    fn extends_each_button_toward_44_px_where_nothing_is_near() {
        assert_eq!(
            resolve_hit_target(-9., 12., &[r(0., 0., 24., 24.)], 44.),
            Some(0)
        );
    }

    #[test]
    fn answers_none_beyond_every_hit_area() {
        assert_eq!(
            resolve_hit_target(-11., 12., &[r(0., 0., 24., 24.)], 44.),
            None
        );
    }

    #[test]
    fn picks_the_nearer_target_not_the_earlier_one_where_targets_overlap() {
        let rects = [r(0., 0., 40., 28.), r(30., 0., 70., 28.)];
        assert_eq!(resolve_hit_target(36., 14., &rects, 44.), Some(1));
    }
}
