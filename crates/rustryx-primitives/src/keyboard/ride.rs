//! Upstream `keyboard/rideKeyboard.ts`: moves an element with the keyboard on the curve
//! its transition publishes, or at once.

use crate::motion::curve::FnCurve;
use crate::motion::{
    AnimateOptions, CrossfadeAs, MotionCurves, MotionEngine, MotionHandle, MotionValues,
};

use super::model::{KeyboardTransition, eased_value, eased_velocity};

/// Upstream `rideKeyboard`: moves `engine`'s element to `to` with the keyboard, on its
/// curve from `transition.start`, or at once when the keyboard jumps. `now` and the
/// transition's times are on the engine's clock (the keyboard clock,
/// [`super::keyboard_clock`]).
///
/// Every ride starts at the transition's start, so content that joins a slide late lands
/// where the keyboard is. A transition that is not animated lands at once, and so does a
/// slide that has ended by the call: a frame after a busy task never shows the element
/// short of its target. Under Reduce Motion nothing travels: the engine's own plan
/// cross-fades on a touch screen and moves at once elsewhere; with `entrance` the element
/// lands at once and fades in.
pub fn ride_keyboard(
    engine: &MotionEngine,
    to: MotionValues,
    transition: &KeyboardTransition,
    now: f64,
    entrance: bool,
) -> Option<MotionHandle> {
    let Some(slide) = transition
        .curve
        .filter(|c| transition.animated && now < transition.start + c.duration_ms)
    else {
        engine.set(to);
        return None;
    };
    let span = slide.duration_ms / 1000.;
    let mut curves = MotionCurves::new();
    for (property, target) in to.expand() {
        let from = engine.get(property, Some(transition.start));
        curves = curves.with(
            property,
            FnCurve::new(
                span,
                move |t| eased_value(from, target, slide, t),
                move |t| eased_velocity(from, target, slide, t),
            )
            .shared(),
        );
    }
    let options = AnimateOptions {
        at: Some(transition.start),
        crossfade_as: entrance.then_some(CrossfadeAs::Entrance),
        ..Default::default()
    };
    engine.play(curves, options).ok()
}
