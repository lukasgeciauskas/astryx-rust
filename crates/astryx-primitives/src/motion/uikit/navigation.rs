//! UINavigationController's push and pop, upstream
//! `motion/uikit/navigation.ts`. No value is a motion role.

use crate::motion::roles::TableValue;

/// `pushVelocity`: the push's initial velocity, relative.
pub const PUSH_VELOCITY: f64 = 8.773;
/// `parallax`: how far the covered view moves, as a share of the width.
pub const PARALLAX: f64 = -0.3;
/// `dimmingAlpha`.
pub const DIMMING_ALPHA: f64 = 0.1;
/// `transitionMs`.
pub const TRANSITION_MS: f64 = 350.;
/// `interactivePopFinishProjection`.
pub const INTERACTIVE_POP_FINISH_PROJECTION: f64 = 0.35;
/// `interactivePopFinishThreshold`.
pub const INTERACTIVE_POP_FINISH_THRESHOLD: f64 = 0.5;

pub(crate) fn leaves() -> Vec<(String, TableValue)> {
    use TableValue::Number;
    [
        ("pushVelocity", Number(PUSH_VELOCITY)),
        ("parallax", Number(PARALLAX)),
        ("dimmingAlpha", Number(DIMMING_ALPHA)),
        ("transitionMs", Number(TRANSITION_MS)),
        (
            "interactivePopFinishProjection",
            Number(INTERACTIVE_POP_FINISH_PROJECTION),
        ),
        (
            "interactivePopFinishThreshold",
            Number(INTERACTIVE_POP_FINISH_THRESHOLD),
        ),
    ]
    .into_iter()
    .map(|(path, value)| (path.to_string(), value))
    .collect()
}
