//! The press state machine, upstream `utils/pressGesture.ts`: the pure half of the touch
//! press model. A native list delays a press by 0.15 s inside a scroller, cancels it when a
//! scroll claims the gesture, never brings it back in that gesture, and fades it only on
//! release. A surface with no scroller above it paints in the frame the finger lands, as a
//! `UIControl` highlights. A mouse button or the Space key on a touch-first device presses
//! at once (`ButtonDown`). The machine knows nothing of GPUI: [`press_gesture_step`]'s
//! cases are the state diagram, and [`super::press_feedback`] drives it.

use rustryx_primitives::motion::uikit::gestures::press as PRESS;

/// How long a finger must stay put before the surface believes it: UIKit's
/// `delaysContentTouches` 0.15 s.
pub const PRESS_ONSET_MS: f64 = PRESS::CONTENT_TOUCH_DELAY;
/// How far a finger may travel and still be a press, px.
pub const PRESS_SLOP_PX: f64 = PRESS::SLOP;
/// How long a too-quick tap holds the paint at full strength before fading, ms.
pub const PRESS_FLASH_MS: f64 = PRESS::QUICK_TAP_FLASH;
/// How long the paint takes to leave after a release when no theme sets the exit
/// duration: that token's default, ms.
pub const PRESS_FADE_MS: f64 = 175.;
/// How recently a scroller must have moved for a touch on it to be a brake, not a press.
pub const PRESS_SCROLL_BRAKE_MS: f64 = PRESS::SCROLL_BRAKE;

/// What the surface should be painting. `Fading` is the release's exit.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PressPaint {
    #[default]
    None,
    On,
    Fading,
}

/// Where a gesture is. `Dead` is the load-bearing one: a gesture that became a scroll
/// stays dead until the finger lifts, so a finger that stops, or comes back, paints
/// nothing. `ButtonHeld` is a press of a mouse button or the Space key.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PressPhase {
    #[default]
    Idle,
    Armed,
    Pressed,
    ButtonHeld,
    Flash,
    Fading,
    Dead,
}

/// One gesture: its phase and where the finger landed.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PressGesture {
    pub phase: PressPhase,
    pub origin_x: f64,
    pub origin_y: f64,
}

/// One event of a gesture.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PressEvent {
    /// A finger landed; `delayed` waits [`PRESS_ONSET_MS`] (a surface in a scroller).
    Down {
        x: f64,
        y: f64,
        delayed: bool,
    },
    /// A mouse button or the Space key went down where the controller paints it.
    ButtonDown,
    Move {
        x: f64,
        y: f64,
    },
    Up,
    /// The platform took the gesture away: a cancel, a scroll, a drag.
    Cancel,
    /// [`PRESS_ONSET_MS`] elapsed with the finger still down.
    Onset,
    /// [`PRESS_FLASH_MS`] elapsed on a too-quick tap.
    FlashEnd,
    /// The release is over.
    FadeEnd,
}

/// The timer a step wants armed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PressTimer {
    Onset,
    Flash,
    Fade,
}

/// One step's answer. `timer: None` arms nothing new, which is not the same as
/// cancelling what runs: a step that keeps its phase is a pass-through, and the phase it
/// leaves running still owes its transition.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PressStep {
    pub gesture: PressGesture,
    pub paint: PressPaint,
    pub timer: Option<PressTimer>,
}

const IDLE: PressGesture = PressGesture {
    phase: PressPhase::Idle,
    origin_x: 0.,
    origin_y: 0.,
};

/// Upstream `idlePressGesture`.
pub fn idle_press_gesture() -> PressGesture {
    IDLE
}

/// A finger that has gone this far is scrolling, not pressing.
fn travelled(g: &PressGesture, x: f64, y: f64) -> bool {
    (x - g.origin_x).hypot(y - g.origin_y) > PRESS_SLOP_PX
}

fn step(
    g: PressGesture,
    phase: PressPhase,
    paint: PressPaint,
    timer: Option<PressTimer>,
) -> PressStep {
    PressStep {
        gesture: PressGesture { phase, ..g },
        paint,
        timer,
    }
}

fn keep(g: PressGesture, paint: PressPaint) -> PressStep {
    PressStep {
        gesture: g,
        paint,
        timer: None,
    }
}

/// Upstream `pressGestureStep`: one event, one answer.
pub fn press_gesture_step(g: PressGesture, event: PressEvent) -> PressStep {
    use PressEvent as E;
    use PressPaint as P;
    use PressPhase as S;
    match event {
        // No scroller above: nothing can turn this touch into a scroll.
        E::Down {
            x,
            y,
            delayed: false,
        } => {
            return PressStep {
                gesture: PressGesture {
                    phase: S::Pressed,
                    origin_x: x,
                    origin_y: y,
                },
                paint: P::On,
                timer: None,
            };
        }
        // A new touch always starts armed and painting nothing: the only way out of dead.
        E::Down {
            x,
            y,
            delayed: true,
        } => {
            return PressStep {
                gesture: PressGesture {
                    phase: S::Armed,
                    origin_x: x,
                    origin_y: y,
                },
                paint: P::None,
                timer: Some(PressTimer::Onset),
            };
        }
        // Nothing can turn a button or a key into a scroll: paint now.
        E::ButtonDown => {
            return PressStep {
                gesture: PressGesture {
                    phase: S::ButtonHeld,
                    ..IDLE
                },
                paint: P::On,
                timer: None,
            };
        }
        _ => {}
    }
    match g.phase {
        S::Armed => match event {
            E::Move { x, y } if travelled(&g, x, y) => step(g, S::Dead, P::None, None),
            // The scroller claimed it inside the delay: a flick paints nothing.
            E::Cancel => step(g, S::Dead, P::None, None),
            E::Onset => step(g, S::Pressed, P::On, None),
            // Too quick for the delay: the paint appears at the lift.
            E::Up => step(g, S::Flash, P::On, Some(PressTimer::Flash)),
            _ => keep(g, P::None),
        },
        S::Pressed => match event {
            // A press with a shaky hand is still a press.
            E::Move { x, y } if !travelled(&g, x, y) => keep(g, P::On),
            // A cancel removes the paint with no transition.
            E::Move { .. } | E::Cancel => step(g, S::Dead, P::None, None),
            E::Up => step(g, S::Fading, P::Fading, Some(PressTimer::Fade)),
            _ => keep(g, P::On),
        },
        S::ButtonHeld => match event {
            E::Up => step(g, S::Fading, P::Fading, Some(PressTimer::Fade)),
            E::Cancel => step(g, S::Dead, P::None, None),
            _ => keep(g, P::On),
        },
        S::Flash => match event {
            E::FlashEnd => step(g, S::Fading, P::Fading, Some(PressTimer::Fade)),
            // The finger is already up: a cancel now must not clip the tap's answer.
            _ => keep(g, P::On),
        },
        S::Fading => match event {
            E::FadeEnd => keep(IDLE, P::None),
            _ => keep(g, P::Fading),
        },
        // The lift ends a cancelled gesture, so the next touch is heard.
        S::Dead => match event {
            E::Up => keep(IDLE, P::None),
            _ => keep(g, P::None),
        },
        S::Idle => keep(IDLE, P::None),
    }
}
