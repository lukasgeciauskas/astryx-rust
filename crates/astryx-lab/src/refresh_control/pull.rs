//! The pull's values and math (upstream lab `RefreshControl/refreshValues.ts` and
//! `refreshPull.ts`): the mapping from finger travel to overscroll, its slope and
//! inverse, the trigger distance, the reveal, and a refresh's release and end.

// UIKit's refresh pull, as measured: the first `height` of overscroll tracks
// the finger one to one, after the pan's hysteresis. Past it, the scroll
// view's rubber band applies to the excess. The trigger distance grows with
// the scroll view's height. After a trigger within twice the height, the
// overscroll steps back onto the finger. It stays one to one up to twice the
// height, with resistance past that.

use astryx_primitives::motion::{
    cubic_bezier, rubber_band, rubber_band_inverse, rubber_band_slope,
};
use astryx_theme::authoring::js::{js_max, js_min};

/// UIRefreshControl's pull values (upstream `refreshControl`). Lengths are in pt
/// (GPUI px), durations in ms, and curves are cubic Bézier control points.
pub mod values {
    /// The refreshing inset: while refreshing, the content rests this far below the
    /// top of the scroll view, with the indicator in the gap. Also the stretch of a
    /// pull that tracks the finger one to one before the rubber band starts
    /// (measured: UIKit lab, iOS 26.2 simulator, iPhone 17 Pro, a 334 pt scroll view:
    /// content held at −60.33 while refreshing, Oct 23 2026).
    pub const HEIGHT: f64 = 60.;
    /// The trigger distance as a fraction of the scroll view's height (measured on
    /// scroll views 400, 874 and 1366 pt tall, Oct 23 2026).
    pub const TRIGGER_HEIGHT_FRACTION: f64 = 0.198;
    /// The shortest trigger distance, pt of overscroll: short scroll views trigger
    /// here (measured on a 334 pt scroll view, Oct 23 2026).
    pub const TRIGGER_MINIMUM: f64 = 73.;
    /// After the trigger, the offset follows the finger one to one again, up to this
    /// many `HEIGHT`s of overscroll. Past it, the pull resists again.
    pub const POST_TRIGGER_LINEAR_HEIGHTS: f64 = 2.;
    /// The indicator's reveal curve: the reveal goes from 0 to 1 on it as the
    /// overscroll goes from 0 to the trigger distance (UIKit's opacity animation,
    /// timing function (0.42, 0, 0.58, 1)).
    pub const REVEAL_CURVE: [f64; 4] = [0.42, 0., 0.58, 1.];
    /// `endRefreshing()` while the control is visible: the inset leaves over this
    /// long on UIView.animate's easeInEaseOut. The end plays the theme duration token
    /// nearest it (`--duration-medium`); this stays as the reference for that choice.
    pub const END_MS: f64 = 300.;
    /// The theme duration token that the end plays (upstream `endDurationToken`).
    pub const END_DURATION_TOKEN: &str = "--duration-medium";
}

/// The refreshing inset and one-to-one stretch, pt (upstream `REFRESH_HEIGHT`).
pub const REFRESH_HEIGHT: f64 = values::HEIGHT;

/// Which mapping a pull follows (upstream `PullMode`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PullMode {
    /// Before the trigger: one to one up to `height`, then the rubber band on the
    /// excess.
    Reveal,
    /// After a trigger within twice the height, and for a pull from the refreshing
    /// inset: one to one up to twice the height, then the rubber band.
    Band,
}

fn linear_stretch(mode: PullMode) -> f64 {
    match mode {
        PullMode::Band => values::HEIGHT * values::POST_TRIGGER_LINEAR_HEIGHTS,
        PullMode::Reveal => values::HEIGHT,
    }
}

/// Upstream `pullOverscroll`: the overscroll shown for `travel` px of pull (the
/// pan's translation, which already leaves out its hysteresis). `dimension` is the
/// scroll owner's height, the rubber band's dimension.
pub fn pull_overscroll(travel: f64, dimension: f64, mode: PullMode) -> f64 {
    // Nothing below zero, and nothing for NaN.
    if travel.is_nan() || travel <= 0. {
        return 0.;
    }
    let linear = linear_stretch(mode);
    if travel <= linear {
        return travel;
    }
    linear + rubber_band(travel - linear, dimension)
}

/// Upstream `pullSlope`: how fast the overscroll moves per px of finger travel at
/// `travel`: 1 where it tracks the finger, the rubber band's slope past it. A
/// release's speed times this slope is the content's speed, which its settle starts
/// at.
pub fn pull_slope(travel: f64, dimension: f64, mode: PullMode) -> f64 {
    // Nothing below zero, and nothing for NaN.
    if travel.is_nan() || travel <= 0. {
        return 0.;
    }
    let linear = linear_stretch(mode);
    if travel <= linear {
        1.
    } else {
        rubber_band_slope(travel - linear, dimension)
    }
}

/// Upstream `pullTravel`: the travel that shows `overscroll`, where a finger
/// landing on content already pulled (a re-grab mid-settle) resumes without a jump.
pub fn pull_travel(overscroll: f64, dimension: f64, mode: PullMode) -> f64 {
    if overscroll.is_nan() || overscroll <= 0. {
        return 0.;
    }
    let linear = linear_stretch(mode);
    if overscroll <= linear {
        return overscroll;
    }
    linear + rubber_band_inverse(overscroll - linear, dimension)
}

/// Upstream `modeAfterTrigger`: the mapping after a trigger at `travel`. It is the
/// one-to-one band when the trigger falls within it, and the next frame steps onto
/// the finger; otherwise the reveal mapping, as when a tall scroll view triggers past
/// the band.
pub fn mode_after_trigger(travel: f64) -> PullMode {
    if travel <= linear_stretch(PullMode::Band) {
        PullMode::Band
    } else {
        PullMode::Reveal
    }
}

/// Upstream `triggerDistance`: the overscroll at which a pull triggers, for a scroll
/// owner `dimension` tall: a fraction of its height, never below the minimum.
pub fn trigger_distance(dimension: f64) -> f64 {
    js_max(
        values::TRIGGER_MINIMUM,
        values::TRIGGER_HEIGHT_FRACTION * js_max(0., dimension),
    )
}

/// Upstream `revealFraction`: how much of the indicator a pull reveals, 0 to 1
/// (linear in the pull).
pub fn reveal_fraction(overscroll: f64, dimension: f64) -> f64 {
    js_min(1., js_max(0., overscroll / trigger_distance(dimension)))
}

/// Upstream `revealProgress`: the reveal fraction on UIKit's reveal curve, the
/// indicator's opacity.
pub fn reveal_progress(overscroll: f64, dimension: f64) -> f64 {
    cubic_bezier(values::REVEAL_CURVE).ease(reveal_fraction(overscroll, dimension))
}

/// Upstream `releaseTarget`: where the content comes to rest when the finger lifts:
/// on the refreshing inset while refreshing, back at the top otherwise.
pub fn release_target(is_refreshing: bool) -> f64 {
    if is_refreshing { values::HEIGHT } else { 0. }
}

/// How the end of a refresh moves the content (upstream `endMotion`'s result).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EndMotion {
    /// Where the inset animates to, or `None` when nothing moves.
    pub animate_to: Option<f64>,
    /// How far the scroll offset drops with the inset.
    pub scroll_by: f64,
}

/// Upstream `endMotion`: how the end of a refresh moves the content. `scrolled`
/// (≥ 0) is how far the scroll owner is scrolled with the inset still in place.
/// While the gap shows, the content slides up until the gap closes: the inset
/// animates from `height` to `scrolled`. Then the inset and the scroll offset drop
/// together with no visible change. Scrolled past the gap, nothing moves, and the
/// inset and the offset drop at once.
pub fn end_motion(scrolled: f64) -> EndMotion {
    let height = values::HEIGHT;
    let offset = js_max(0., scrolled);
    if offset >= height {
        return EndMotion {
            animate_to: None,
            scroll_by: -height,
        };
    }
    EndMotion {
        animate_to: Some(offset),
        scroll_by: -offset,
    }
}

/// The end's duration and curve (upstream `endTiming`'s result).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EndTiming {
    pub ms: f64,
    pub points: [f64; 4],
}

/// Upstream `endTiming`: the end plays UIView.animate's easeInEaseOut, whatever the
/// theme's easing, over the theme's `--duration-medium` as it applies (the tempo
/// scales it). `None` without a duration.
pub fn end_timing(duration_ms: Option<f64>) -> Option<EndTiming> {
    duration_ms.map(|ms| EndTiming {
        ms,
        points: values::REVEAL_CURVE,
    })
}
