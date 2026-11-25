//! UIKit's measured motion constants, upstream `motion/uikit/` (the rows
//! the wheels read; M11 adds the rest). Each value's source is in upstream's
//! table.

use super::SpringDescription;

/// `scroll`: UIScrollView.
pub mod scroll {
    use super::SpringDescription;

    /// `decelerationRate.normal`: the velocity factor per millisecond.
    pub const DECELERATION_NORMAL: f64 = 0.998;
    /// `decelerationRate.fast`.
    pub const DECELERATION_FAST: f64 = 0.99;
    /// `rubberBand.coefficient`.
    pub const RUBBER_BAND_COEFFICIENT: f64 = 0.55;
    /// `bounce`: the spring that returns content released past an edge.
    pub const BOUNCE: SpringDescription = SpringDescription::DurationBounce {
        duration: 0.5,
        bounce: 0.,
    };
}

/// `picker`: UIPickerView, measured at its 32 pt row.
pub mod picker {
    /// `geometry.rowHeight`, pt.
    pub const ROW_HEIGHT: f64 = 32.;
    /// `geometry.height`, pt.
    pub const HEIGHT: f64 = 216.;
    /// `geometry.rubberBandDimension`, pt.
    pub const RUBBER_BAND_DIMENSION: f64 = 291.;
    /// `release.stopSpeed`, pt/ms: where a coast ends.
    pub const STOP_SPEED: f64 = 0.0089;
    /// `release.minimumSpeed`, pt/ms: slower releases inside the rows snap.
    pub const MINIMUM_SPEED: f64 = 0.25;
    /// `snapRate`: the share of the distance left covered per millisecond.
    pub const SNAP_RATE: f64 = 0.011;
    /// `tapSpin.durationMs`: the measured reference for a turn.
    pub const TAP_SPIN_MS: f64 = 300.;
    /// `selectionTick.minimumIntervalMs`.
    pub const SELECTION_TICK_INTERVAL_MS: f64 = 40.;
}

/// `springs`.
pub mod springs {
    use super::SpringDescription;

    /// `settle`: SwiftUI's `.smooth`.
    pub const SETTLE: SpringDescription = SpringDescription::DurationBounce {
        duration: 0.5,
        bounce: 0.,
    };
}

/// `timing`'s Reduce Motion cross-fade.
pub mod timing {
    /// `reducedMotionCrossFade.durationMs`.
    pub const REDUCED_MOTION_CROSS_FADE_MS: f64 = 200.;
}

/// `UIKIT_VELOCITY` (`motion/uikit/gestures.ts`).
pub mod velocity {
    /// `currentWeight`: a pan's newest interval in its velocity.
    pub const CURRENT_WEIGHT: f64 = 0.2;
    /// `scrollReleaseWeight`: a scrolling surface's newest pan value.
    pub const SCROLL_RELEASE_WEIGHT: f64 = 0.25;
    /// `stillness`, ms: a contact at rest this long releases at 0.
    pub const STILLNESS_MS: f64 = 40.;
}

/// `UIKIT_PAN`'s hysteresis, px: the travel that begins a pan.
pub const PAN_HYSTERESIS: f64 = 10.;

/// Core Animation's `easeInEaseOut`.
pub const EASE_IN_EASE_OUT: (f32, f32, f32, f32) = (0.42, 0., 0.58, 1.);
