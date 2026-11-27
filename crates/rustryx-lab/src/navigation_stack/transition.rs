//! NavigationStack's motion table (upstream `navigationMotion.ts`, over Core's UIKit
//! navigation table) and the pure math of its transition (upstream
//! `navigationTransition.ts`): the layers' values and velocities at a progress, the
//! interactive pop's release rule, and progress from a finger's travel or a caught
//! screen.
//!
//! The layers, as UIKit moves them on iOS 26.2 (measured): the top card at `x = p · W`;
//! the card under it at `x = parallax · W · (1 − p)`, from −0.3 W to 0; the dimming over
//! the card under it at opacity `1 − p` (black at 10 %); each bar moves with its
//! screen's card. Every layer is linear in `p`, so one spring run on all of them with
//! velocities scaled the same way keeps them on one trajectory.

use rustryx_primitives::motion::uikit::navigation as core;
use rustryx_primitives::motion::{rubber_band_inverse_with, rubber_band_with};
use rustryx_theme::authoring::js::{js_max, js_min};

/// The push and pop springs start moving at this many travels per second (upstream
/// `navigation.pushVelocity`).
pub const PUSH_VELOCITY: f64 = core::PUSH_VELOCITY;
/// Where the card underneath ends, as a share of the width (upstream
/// `navigation.parallax`).
pub const PARALLAX: f64 = core::PARALLAX;
/// The dimming over the card underneath: black at this alpha (upstream
/// `navigation.dimmingAlpha`).
pub const DIMMING_ALPHA: f64 = core::DIMMING_ALPHA;
/// The interactive pop finishes when progress plus this many seconds of the release
/// velocity passes the threshold (upstream `interactivePopFinishProjection`).
pub const INTERACTIVE_POP_FINISH_PROJECTION: f64 = core::INTERACTIVE_POP_FINISH_PROJECTION;
/// Upstream `interactivePopFinishThreshold`.
pub const INTERACTIVE_POP_FINISH_THRESHOLD: f64 = core::INTERACTIVE_POP_FINISH_THRESHOLD;
/// Past its start the interactive pop's top screen resists with UIKit's rubber band at
/// this coefficient over the stack's width (upstream `interactivePopRubberBand`).
pub const INTERACTIVE_POP_RUBBER_BAND: f64 = 0.71;
/// Travel before the full-width content swipe begins, px (upstream
/// `contentPopHysteresis`).
pub const CONTENT_POP_HYSTERESIS: f64 = 27.;
/// On a push from the root the back chevron starts fading in this many ms after the
/// screens start moving (upstream `backChevronPushDelayMs`).
pub const BACK_CHEVRON_PUSH_DELAY_MS: f64 = 60.;
/// How far the bar's title moves as it fades in or out over a large title (upstream
/// `inlineTitleRise`).
pub const INLINE_TITLE_RISE: f64 = 15.;
/// The bar's height (upstream `BAR_HEIGHT`).
pub const BAR_HEIGHT: f64 = 44.;
/// The interactive pop released past its start returns on this spring: response
/// 0.381 s, damping ratio 0.81 (upstream `springs.navigationInteractivePopReturn`).
pub const INTERACTIVE_POP_RETURN: (f64, f64) = (0.381, 0.81);
/// The back chevron's fade: response 0.4 s, damping ratio 1 (upstream
/// `springs.navigationBackButton`).
pub const BACK_BUTTON: (f64, f64) = (0.4, 1.);
/// The scroll edge veil is off while the content sits at most this far past its rest
/// offset (upstream `bars.scrollEdgeEffect.switchPt`).
pub const SCROLL_EDGE_SWITCH: f64 = 10.;
/// The veil fading in: mass, stiffness, damping (upstream `scrollEdgeEffect.appear`).
pub const SCROLL_EDGE_APPEAR: (f64, f64, f64) = (1., 986.96, 62.83);
/// The veil fading out (upstream `scrollEdgeEffect.disappear`).
pub const SCROLL_EDGE_DISAPPEAR: (f64, f64, f64) = (1., 322.27, 35.9);
/// How far the veil's soft edge reaches below the bar (upstream `softExtentPt`).
pub const SCROLL_EDGE_SOFT_EXTENT: f64 = 54.8;
/// The veil's alpha the web plays (upstream: UIKit's 0.85, raised to 0.95 so a bar's
/// title keeps 4.5:1 over bright content).
pub const VEIL_ALPHA: f64 = 0.95;
/// Under a titled bar the veil is half-transparent this far above the bar's bottom
/// (upstream `veil.edgeAboveContentBottomPt`).
pub const VEIL_EDGE_ABOVE_BOTTOM: f64 = 10.;
/// How soft the veil's edge is (upstream `veil.softnessPt`).
pub const VEIL_SOFTNESS: f64 = 18.;

/// The writing direction cards enter from.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Direction {
    /// Left to right: cards enter from the right.
    #[default]
    Ltr,
    /// Right to left: cards enter from the left.
    Rtl,
}

impl Direction {
    fn sign(self) -> f64 {
        if self == Direction::Rtl { -1. } else { 1. }
    }
}

/// The layers' values at a progress (upstream `TransitionLayers`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TransitionLayers {
    /// The top card's translateX, px.
    pub top_x: f64,
    /// The covered card's translateX, px.
    pub under_x: f64,
    /// The dimming's opacity over the covered card, 0 to 1.
    pub dim: f64,
}

fn clamp01(value: f64) -> f64 {
    // JavaScript's Math.min(1, Math.max(0, value)): NaN stays NaN, -0 becomes 0.
    js_min(1., js_max(0., value))
}

/// The layers at progress `p` for a container `width` px wide (upstream `layersAt`).
/// `p` is the share of the width the top card has uncovered: 0 when a push has landed,
/// 1 when a pop has. Right to left, every x flips.
pub fn layers_at(p: f64, width: f64, direction: Direction) -> TransitionLayers {
    let sign = direction.sign();
    TransitionLayers {
        top_x: sign * p * width,
        under_x: sign * PARALLAX * width * (1. - p),
        dim: clamp01(1. - p),
    }
}

/// The layers' velocities (px/s and 1/s) when progress moves at `dp` per second
/// (upstream `layerVelocities`).
pub fn layer_velocities(dp: f64, width: f64, direction: Direction) -> TransitionLayers {
    let sign = direction.sign();
    TransitionLayers {
        top_x: sign * dp * width,
        under_x: -sign * PARALLAX * width * dp,
        dim: -dp,
    }
}

/// What a programmatic transition is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransitionKind {
    /// A screen pushed on top.
    Push,
    /// The top screen popped.
    Pop,
}

/// The push and pop springs' starting velocity in progress per second (upstream
/// `programmaticVelocity`): negative for a push, positive for a pop.
pub fn programmatic_velocity(kind: TransitionKind) -> f64 {
    match kind {
        TransitionKind::Push => -PUSH_VELOCITY,
        TransitionKind::Pop => PUSH_VELOCITY,
    }
}

/// Progress from a finger's horizontal travel since the pop began, added to the
/// progress the cards had then (upstream `progressFromTravel`). At most 1; below 0 it
/// resists with UIKit's rubber band.
pub fn progress_from_travel(
    start_progress: f64,
    travel_x: f64,
    width: f64,
    direction: Direction,
) -> f64 {
    if width <= 0. {
        return clamp01(start_progress);
    }
    let p = start_progress + (direction.sign() * travel_x) / width;
    if p < 0. {
        rubber_band_with(p * width, width, INTERACTIVE_POP_RUBBER_BAND) / width
    } else if p.is_nan() {
        p
    } else {
        p.min(1.)
    }
}

/// The progress a finger resumes from when it catches the top screen at translateX `x`
/// (upstream `progressFromScreen`); past the start it undoes the rubber band.
pub fn progress_from_screen(x: f64, width: f64, direction: Direction) -> f64 {
    if width <= 0. {
        return 0.;
    }
    let shown = (if direction == Direction::Rtl { -x } else { x }) / width;
    if shown < 0. {
        rubber_band_inverse_with(shown * width, width, INTERACTIVE_POP_RUBBER_BAND) / width
    } else if shown.is_nan() {
        shown
    } else {
        shown.min(1.)
    }
}

/// The interactive pop's release rule (upstream `shouldFinishPop`): it finishes when
/// progress plus 0.35 s of the release velocity, in widths per second, passes half the
/// width; a release moving back toward the start cancels.
pub fn should_finish_pop(progress: f64, velocity_widths_per_second: f64) -> bool {
    if velocity_widths_per_second < 0. {
        return false;
    }
    progress + INTERACTIVE_POP_FINISH_PROJECTION * velocity_widths_per_second
        > INTERACTIVE_POP_FINISH_THRESHOLD
}

/// A finger's horizontal velocity, px/s, in widths per second toward a pop (upstream
/// `popVelocity`).
pub fn pop_velocity(velocity_x: f64, width: f64, direction: Direction) -> f64 {
    if width <= 0. {
        return 0.;
    }
    (direction.sign() * velocity_x) / width
}
