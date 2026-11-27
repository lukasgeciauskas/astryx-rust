//! The release rules and geometry of a row's swipe, upstream `useSwipeAction.ts`'s
//! `resolveRelease`, `confirmPoint` and `confirmedOffset`, and its stops.
//!
//! A UIKit list row fires its outermost action only from a confirmed full swipe: the
//! finger dragged past the confirm point and is still past it when it lifts, at any
//! speed. A swipe that confirmed and backed out closes the row. Any other release goes
//! by its speed first: at `releaseVelocity` or faster it goes the way it moves; a
//! slower one goes by where it is, against `openFraction` of the open width. So a flick
//! opens the row even from a short drag, closes it from anywhere, and never fires.
//! Travel and velocity here are logical: toward the side being uncovered.

use astryx_primitives::motion::uikit::swipe_actions as SWIPE;

use super::ItemSwipeBehavior;

/// Where a release lands (upstream `SwipeRelease`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SwipeRelease {
    Closed,
    Open,
    Perform,
}

/// A release as [`resolve_release`] reads it.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ReleaseQuery {
    /// px, at least 0, toward the side being uncovered.
    pub travel: f64,
    /// px/s, positive toward the side being uncovered.
    pub velocity: f64,
    pub open_width: f64,
    /// The finger is past the confirm point as it lifts.
    pub is_confirmed: bool,
    /// It was past it at some point during this swipe.
    pub was_confirmed: bool,
    pub behavior: ItemSwipeBehavior,
}

/// Upstream `resolveRelease`.
pub(crate) fn resolve_release(q: ReleaseQuery) -> SwipeRelease {
    if q.is_confirmed {
        return SwipeRelease::Perform;
    }
    if q.behavior == ItemSwipeBehavior::Commit || q.was_confirmed {
        return SwipeRelease::Closed;
    }
    if q.velocity >= SWIPE::RELEASE_VELOCITY {
        return SwipeRelease::Open;
    }
    if q.velocity <= -SWIPE::RELEASE_VELOCITY {
        return SwipeRelease::Closed;
    }
    if q.travel >= SWIPE::OPEN_FRACTION * q.open_width {
        SwipeRelease::Open
    } else {
        SwipeRelease::Closed
    }
}

/// Upstream `confirmPoint`: the swipe at which a full swipe confirms,
/// `confirmFraction` of the way from the open width to the confirmation offset, and
/// never short of the open width.
pub(crate) fn confirm_point(open_width: f64, row_width: f64) -> f64 {
    let confirm_at = row_width - SWIPE::CONFIRM_INSET;
    open_width.max(open_width + (confirm_at - open_width) * SWIPE::CONFIRM_FRACTION)
}

/// Upstream `confirmedOffset`: the row's offset once a full swipe has confirmed
/// (UIKit's `confirmationThreshold`).
pub(crate) fn confirmed_offset(row_width: f64) -> f64 {
    row_width - SWIPE::CONFIRM_INSET
}

/// A side of the row: `Leading` is uncovered by a drag toward the inline end and sits
/// at the inline start; `Trailing` the reverse (FR8).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum SwipeSide {
    Leading,
    Trailing,
}

impl SwipeSide {
    pub fn index(self) -> usize {
        match self {
            Self::Leading => 0,
            Self::Trailing => 1,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Leading => "leading",
            Self::Trailing => "trailing",
        }
    }
}

/// A side's geometry as a drag reads it (upstream `SwipeLayout`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SwipeLayout {
    pub side: SwipeSide,
    /// 1 when the inline end is to the right (left-to-right), -1 under right-to-left.
    pub inline_sign: f64,
    /// The side's entries at their natural widths together: how far the row opens.
    pub open_width: f64,
    /// The outermost entry's width.
    pub outer_width: f64,
    pub row_width: f64,
}

/// The side a drag from a closed row uncovers: toward the inline end is leading.
pub(crate) fn side_for(dx: f64, inline_sign: f64) -> SwipeSide {
    if dx * inline_sign > 0. {
        SwipeSide::Leading
    } else {
        SwipeSide::Trailing
    }
}

/// 1 when the side's panel is on the physical left, -1 on the right.
pub(crate) fn physical_sign(side: SwipeSide, inline_sign: f64) -> f64 {
    match side {
        SwipeSide::Leading => inline_sign,
        SwipeSide::Trailing => -inline_sign,
    }
}

/// How far out a row goes when an entry fires and the row leaves (logical px).
pub(crate) fn out_travel(layout: &SwipeLayout) -> f64 {
    layout.row_width + SWIPE::SLIDE_OUT_OVERSHOOT
}

/// Which stop: rest, open, or out past the row's edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Stop {
    Rest,
    Open,
    Out,
}

fn stop_value(layout: &SwipeLayout, stop: Stop) -> f64 {
    let sign = physical_sign(layout.side, layout.inline_sign);
    match stop {
        Stop::Rest => 0.,
        Stop::Open => sign * layout.open_width,
        Stop::Out => sign * out_travel(layout),
    }
}

/// A side's stops, physical px along x, ascending: rest, the open width, and out.
pub(crate) fn stops_of(layout: &SwipeLayout) -> Vec<f64> {
    let mut stops: Vec<f64> = [Stop::Rest, Stop::Open, Stop::Out]
        .iter()
        .map(|s| stop_value(layout, *s))
        .collect();
    stops.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    stops
}

pub(crate) fn stop_index(layout: &SwipeLayout, stop: Stop) -> usize {
    let value = stop_value(layout, stop);
    stops_of(layout)
        .iter()
        .position(|v| *v == value)
        .unwrap_or(0)
}

pub(crate) fn stop_at(layout: &SwipeLayout, index: usize) -> Stop {
    let value = stops_of(layout).get(index).copied().unwrap_or(0.);
    if value == 0. {
        Stop::Rest
    } else if value.abs() == layout.open_width {
        Stop::Open
    } else {
        Stop::Out
    }
}

/// Upstream `swipeRelease.test.ts`: the release rule against the UIKit lab's releases,
/// where a full swipe confirms, and the swipe spring.
#[cfg(test)]
mod tests {
    use super::*;
    use astryx_primitives::motion::settling_duration;

    fn release(
        behavior: ItemSwipeBehavior,
        open_width: f64,
        travel: f64,
        velocity: f64,
        is_confirmed: bool,
        was_confirmed: bool,
    ) -> SwipeRelease {
        resolve_release(ReleaseQuery {
            travel,
            velocity,
            open_width,
            is_confirmed,
            was_confirmed,
            behavior,
        })
    }

    const REVEAL: ItemSwipeBehavior = ItemSwipeBehavior::Reveal;
    const COMMIT: ItemSwipeBehavior = ItemSwipeBehavior::Commit;
    use SwipeRelease::{Closed, Open, Perform};

    #[test]
    fn resolve_release_lands_where_uikit_did() {
        // The UIKit lab's releases (402 pt rows, two trailing actions, 156.67 pt open
        // width): the swipe's travel past the pan's 25 pt and its velocity in pt/s.
        let releases: &[(f64, f64, SwipeRelease)] = &[
            (35., -19., Closed),
            (65., 490., Open),
            (125., 1506., Open),
            (5., -492., Closed),
            (-55., -1532., Closed),
            (95., -16., Open),
            (125., 478., Open),
            (185., 1565., Open),
            (65., -468., Closed),
            (-67., -2545., Closed),
            (-47.7, -1025., Closed),
            (-3., -256., Closed),
            (12.3, -22., Closed),
            (30.3, 293., Open),
            (64., 1058., Open),
            (132.3, 1983., Open),
            (-67., -2060., Closed),
            (-10., -1087., Closed),
            (35.7, 458., Open),
            (50., -18., Closed),
            (63., 286., Open),
            (93., 1372., Open),
            (170., 2031., Open),
            (1.7, -2168., Closed),
            (27.3, -996., Closed),
            (69.3, -287., Closed),
            (87.7, -5., Open),
            (100.3, 148., Open),
            (147.3, 1271., Open),
            (190.3, 608., Open),
            (39.3, -2052., Closed),
            (65., -983., Closed),
            (107., -308., Closed),
            (125., -18., Open),
            (143., 347., Open),
            (185., 948., Open),
            (245., 2082., Open),
            (42.3, -1975., Closed),
            (102.7, -991., Closed),
            (144.3, -283., Closed),
            (162.3, -19., Open),
            (180.3, 271., Open),
            (222.7, 1002., Open),
            (282.3, 1971., Open),
        ];
        for &(travel, velocity, lands) in releases {
            assert_eq!(
                release(REVEAL, 156.67, travel.max(0.), velocity, false, false),
                lands,
                "{travel} pt at {velocity} pt/s"
            );
        }
    }

    #[test]
    fn resolve_release_goes_the_way_it_moves_from_about_50_pt_per_s() {
        for velocity in [-19.9, -21.2, -35.7, -40.7, 0., 56.1] {
            assert_eq!(release(REVEAL, 204.67, 143., velocity, false, false), Open);
        }
        for velocity in [-56.1, -56.5, -68.5, -105., -200., -308.] {
            assert_eq!(
                release(REVEAL, 204.67, 143., velocity, false, false),
                Closed
            );
        }
    }

    #[test]
    fn resolve_release_fires_on_a_confirmed_release_at_any_velocity_only() {
        for velocity in [0., 1000., -1000.] {
            assert_eq!(release(REVEAL, 204.67, 340., velocity, true, true), Perform);
        }
        assert_eq!(release(REVEAL, 204.67, 44., 3000., false, false), Open);
    }

    #[test]
    fn resolve_release_fires_under_commit_only_when_confirmed_and_never_rests() {
        assert_eq!(release(COMMIT, 72., 250., -1000., true, true), Perform);
        assert_eq!(release(COMMIT, 72., 100., 0., false, false), Closed);
        assert_eq!(release(COMMIT, 72., 40., 3000., false, false), Closed);
    }

    #[test]
    fn fr5_never_fires_an_entry_on_speed_alone() {
        let flings = [
            (222.7, 1002.),
            (64., 1058.),
            (147.3, 1271.),
            (93., 1372.),
            (125., 1506.),
            (185., 1565.),
            (282.3, 1971.),
            (132.3, 1983.),
            (170., 2031.),
            (245., 2082.),
        ];
        for (travel, velocity) in flings {
            assert_eq!(
                release(REVEAL, 156.67, travel, velocity, false, false),
                Open
            );
            assert_eq!(
                release(COMMIT, 156.67, travel, velocity, false, false),
                Closed
            );
        }
    }

    #[test]
    fn fr5_closes_a_swipe_that_confirmed_and_came_back() {
        assert_eq!(release(REVEAL, 204.67, 275., 0., false, true), Closed);
    }

    #[test]
    fn fr5_closes_a_release_past_the_panel_that_is_closing_fast() {
        assert_eq!(release(REVEAL, 204.67, 305., -300., false, false), Closed);
        for velocity in [0., 300.] {
            assert_eq!(release(REVEAL, 204.67, 305., velocity, false, false), Open);
        }
    }

    #[test]
    fn fr5_opens_a_release_short_of_half_the_panel_that_is_opening_fast() {
        for (travel, velocity) in [
            (30.3, 293.),
            (35.7, 458.),
            (63., 286.),
            (64., 1058.),
            (65., 490.),
        ] {
            assert!(travel < SWIPE::OPEN_FRACTION * 156.67);
            assert_eq!(
                release(REVEAL, 156.67, travel, velocity, false, false),
                Open
            );
        }
    }

    #[test]
    fn confirm_point_lands_inside_uikits_brackets() {
        let wide = confirm_point(204.67, 402.);
        assert!(wide > 315. && wide <= 335., "{wide}");
        let narrow = confirm_point(156.67, 402.);
        assert!(narrow > 275. && narrow <= 315., "{narrow}");
    }

    #[test]
    fn confirm_point_is_never_short_of_the_open_width() {
        assert_eq!(confirm_point(390., 402.), 390.);
        assert_eq!(confirmed_offset(402.), 373.);
        assert_eq!(confirmed_offset(375.), 346.);
    }

    #[test]
    fn the_swipe_spring_is_the_one_uikit_reported() {
        for description in [SWIPE::TRACKING, SWIPE::SETTLE] {
            let spring = description.to_spring();
            assert!((spring.damping_ratio() - 1.).abs() < 1e-9);
            assert!((spring.natural_frequency() - 10.).abs() < 1e-9);
            let settle = settling_duration(spring);
            assert!((settle - 0.9235).abs() < 1e-3, "{settle}");
        }
    }

    #[test]
    fn the_stops_run_rest_open_out_on_the_sides_physical_sign() {
        let layout = SwipeLayout {
            side: SwipeSide::Trailing,
            inline_sign: 1.,
            open_width: 144.,
            outer_width: 72.,
            row_width: 300.,
        };
        // Trailing in left-to-right text opens leftward: negative travel.
        assert_eq!(stops_of(&layout), vec![-324., -144., 0.]);
        assert_eq!(
            stop_at(&layout, stop_index(&layout, Stop::Open)),
            Stop::Open
        );
        assert_eq!(stop_at(&layout, 2), Stop::Rest);
        assert_eq!(stop_at(&layout, 0), Stop::Out);
        let rtl = SwipeLayout {
            inline_sign: -1.,
            ..layout
        };
        assert_eq!(stops_of(&rtl), vec![0., 144., 324.]);
        assert_eq!(side_for(10., 1.), SwipeSide::Leading);
        assert_eq!(side_for(10., -1.), SwipeSide::Trailing);
    }
}
