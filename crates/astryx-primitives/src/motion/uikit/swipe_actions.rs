//! The swipe-action row's constants, upstream `motion/uikit/swipeActions.ts`: its
//! gesture thresholds (the web's claim and UIKit's swipe), its tracking and settle
//! springs, and the speed and position that pick where a release lands. Measured on a
//! UIKit host app (plain `UITableView`, 402 pt rows, two trailing actions, iOS 26.2);
//! lengths in pt (px), springs in the vocabulary UIKit was read in. Each value's source
//! is in upstream's table.
//!
//! Reduce Motion is not a value here: UIKit keeps both springs under it, and Astryx's
//! Reduce Motion rules govern instead (what a release starts cross-fades).

use crate::motion::SpringDescription;

/// `claim`: the sideways travel at which the row claims the touch, px. Not a UIKit
/// value: on the web a scrolling ancestor takes the touch at about 10 px.
pub const CLAIM: f64 = 8.;

/// `hysteresis`: UIKit's finger travel before its swipe begins, px. The confirm point
/// is judged on the swipe counted from here.
pub const HYSTERESIS: f64 = 25.;

/// `tracking`: UIKit's tracking spring (critically damped at 10 rad/s). A confirmed
/// row runs ahead to the confirmation offset on it.
pub const TRACKING: SpringDescription = SpringDescription::Physics {
    mass: 2.,
    stiffness: 200.,
    damping: 100.,
};

/// `settle`: the settle after the finger lifts, to open, closed or out, seeded with
/// the row's own velocity.
pub const SETTLE: SpringDescription = SpringDescription::Physics {
    mass: 2.,
    stiffness: 200.,
    damping: 100.,
};

/// `openFraction`: past this fraction of the open width a slow release opens the row.
pub const OPEN_FRACTION: f64 = 0.5;

/// `releaseVelocity`: a release this fast (px/s) goes the way it moves.
pub const RELEASE_VELOCITY: f64 = 48.;

/// `confirmInset`: the full swipe's confirmation offset is the row width less this, px.
pub const CONFIRM_INSET: f64 = 29.;

/// `confirmFraction`: how far from the open width toward the confirmation offset the
/// finger drags before a full swipe confirms.
pub const CONFIRM_FRACTION: f64 = 0.69;

/// `slideOutOvershoot`: how far past the row's edge the row travels when it goes out, px.
pub const SLIDE_OUT_OVERSHOOT: f64 = 24.;
