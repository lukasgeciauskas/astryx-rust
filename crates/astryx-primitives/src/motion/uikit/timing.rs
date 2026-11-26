//! UIKit's durations and timing curves, upstream `motion/uikit/timing.ts`
//! (each value's source is in upstream's table).

use crate::motion::bezier::BezierPoints;
use crate::motion::roles::TableValue;

/// `press`: a control's highlight clocks.
pub mod press {
    /// `highlightInMs`.
    pub const HIGHLIGHT_IN_MS: f64 = 0.;
    /// `flashMs`.
    pub const FLASH_MS: f64 = 100.;
    /// `highlightOutMs`.
    pub const HIGHLIGHT_OUT_MS: f64 = 200.;
    /// `highlightOutCurve`.
    pub const HIGHLIGHT_OUT_CURVE: super::BezierPoints = [0., 0., 0.2, 1.];
    /// `scrollBrakeMs`: arbitration, not motion.
    pub const SCROLL_BRAKE_MS: f64 = 150.;
    /// `buttonReleaseMs`.
    pub const BUTTON_RELEASE_MS: f64 = 250.;
}

/// The Reduce Motion cross-fade: its length and curve (upstream
/// `reducedMotionCrossFade`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CrossFade {
    pub duration_ms: f64,
    pub curve: BezierPoints,
}

/// `reducedMotionCrossFade`: 200 ms on Core Animation's ease-in-ease-out.
pub const REDUCED_MOTION_CROSS_FADE: CrossFade = CrossFade {
    duration_ms: 200.,
    curve: [0.42, 0., 0.58, 1.],
};

/// `reducedMotionCrossFadeMs`; nothing may lengthen it, so it is no role.
pub const REDUCED_MOTION_CROSS_FADE_MS: f64 = REDUCED_MOTION_CROSS_FADE.duration_ms;

/// `curves`: Core Animation's named timing functions.
pub mod curves {
    use super::BezierPoints;

    /// `viewChange`: `default`.
    pub const VIEW_CHANGE: BezierPoints = [0.25, 0.1, 0.25, 1.];
    /// `move`: `easeInEaseOut`, the Reduce Motion fade's curve.
    pub const MOVE: BezierPoints = super::REDUCED_MOTION_CROSS_FADE.curve;
    /// `exit`: `easeIn`.
    pub const EXIT: BezierPoints = [0.42, 0., 1., 1.];
    /// `entrance`: `easeOut`.
    pub const ENTRANCE: BezierPoints = [0., 0., 0.58, 1.];
}

/// `navigationBarHideShowMs`.
pub const NAVIGATION_BAR_HIDE_SHOW_MS: f64 = 200.;
/// `keyboardNotificationMs`: the keyboard's own timing, no role.
pub const KEYBOARD_NOTIFICATION_MS: f64 = 383.3;
/// `stateChangeMs`: Core Animation's implicit animation duration.
pub const STATE_CHANGE_MS: f64 = 250.;
/// The role id of [`STATE_CHANGE_MS`], for `resolve_duration_ms`.
pub const STATE_CHANGE_MS_ROLE: &str = "timing.stateChangeMs";

/// The paths upstream leaves out of the registry.
pub const EXCLUDE: [&str; 3] = [
    "press.scrollBrakeMs",
    "reducedMotionCrossFadeMs",
    "keyboardNotificationMs",
];

/// The table's roles (spec:AST-094 FR4).
pub const ROLES: [&str; 5] = [
    "stateChangeMs",
    "curves.viewChange",
    "curves.exit",
    "curves.entrance",
    "curves.move",
];

pub(crate) fn leaves() -> Vec<(String, TableValue)> {
    use TableValue::{Number, Numbers};
    [
        ("press.highlightInMs", Number(press::HIGHLIGHT_IN_MS)),
        ("press.flashMs", Number(press::FLASH_MS)),
        ("press.highlightOutMs", Number(press::HIGHLIGHT_OUT_MS)),
        (
            "press.highlightOutCurve",
            Numbers(press::HIGHLIGHT_OUT_CURVE.to_vec()),
        ),
        ("press.scrollBrakeMs", Number(press::SCROLL_BRAKE_MS)),
        ("press.buttonReleaseMs", Number(press::BUTTON_RELEASE_MS)),
        ("curves.viewChange", Numbers(curves::VIEW_CHANGE.to_vec())),
        ("curves.move", Numbers(curves::MOVE.to_vec())),
        ("curves.exit", Numbers(curves::EXIT.to_vec())),
        ("curves.entrance", Numbers(curves::ENTRANCE.to_vec())),
        (
            "navigationBarHideShowMs",
            Number(NAVIGATION_BAR_HIDE_SHOW_MS),
        ),
        ("keyboardNotificationMs", Number(KEYBOARD_NOTIFICATION_MS)),
        ("stateChangeMs", Number(STATE_CHANGE_MS)),
        (
            "reducedMotionCrossFadeMs",
            Number(REDUCED_MOTION_CROSS_FADE_MS),
        ),
    ]
    .into_iter()
    .map(|(path, value)| (path.to_string(), value))
    .collect()
}
