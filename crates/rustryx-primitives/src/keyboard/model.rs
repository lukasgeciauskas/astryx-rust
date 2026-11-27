//! Upstream `keyboard/keyboardModel.ts`: the keyboard layout guide's clock-free core. It
//! infers the keyboard's transition from the visual viewport's reports and focus changes,
//! each input with its own time.

use crate::motion::android::keyboard as android;
use crate::motion::{BezierPoints, cubic_bezier};

/// Which browser's keyboard reports the guide reads (upstream `KeyboardProfile`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyboardProfile {
    /// Chrome on Android, with the VirtualKeyboard API: it reports each slide as it starts.
    Chromium,
}

/// An eased slide: the curve a transition plays (Gboard going up or down).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KeyboardCurve {
    /// Length, ms.
    pub duration_ms: f64,
    /// Cubic-bézier control points.
    pub points: BezierPoints,
}

/// How the keyboard's cover changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyboardTransitionKind {
    Show,
    Hide,
    Change,
}

/// One change of the keyboard's cover, as the guide reports it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KeyboardTransition {
    pub kind: KeyboardTransitionKind,
    /// Inset presented at `start`, px.
    pub from: f64,
    /// Inset the keyboard comes to rest at, px.
    pub to: f64,
    /// When the keyboard starts to move, ms on the keyboard clock. For a slide Chrome
    /// reported, dated back to Gboard's start; for a hide after a blur, shortly ahead.
    pub start: f64,
    /// The inset's velocity at `start`, px/s (0 unless it interrupts one).
    pub velocity: f64,
    /// False when the inset jumps at `start` (another keyboard while one is up).
    pub animated: bool,
    /// True until the visual viewport confirms a transition the guide inferred.
    pub predicted: bool,
    /// The eased slide the keyboard moves on, or `None` when the inset jumps. It is the
    /// keyboard's own motion, with no motion role.
    pub curve: Option<KeyboardCurve>,
}

const EPSILON: f64 = 0.5;

/// A slide's value and velocity, seconds after its start.
#[derive(Clone, Copy, Debug)]
struct InsetCurve {
    from: f64,
    to: f64,
    slide: KeyboardCurve,
}

impl InsetCurve {
    fn value(&self, t: f64) -> f64 {
        eased_value(self.from, self.to, self.slide, t)
    }

    fn velocity(&self, t: f64) -> f64 {
        eased_velocity(self.from, self.to, self.slide, t)
    }
}

/// The value `t` seconds into a slide from `from` to `to`.
pub(crate) fn eased_value(from: f64, to: f64, slide: KeyboardCurve, t: f64) -> f64 {
    let span = slide.duration_ms / 1000.;
    let ease = cubic_bezier(slide.points);
    let progress = if span > 0. {
        (t / span).clamp(0., 1.)
    } else {
        1.
    };
    from + (to - from) * ease.ease(progress)
}

/// The velocity `t` seconds into a slide, units/s, by a central difference.
pub(crate) fn eased_velocity(from: f64, to: f64, slide: KeyboardCurve, t: f64) -> f64 {
    let span = slide.duration_ms / 1000.;
    if t >= span {
        return 0.;
    }
    let a = (t - 0.001).max(0.);
    let b = (t + 0.001).min(span);
    (eased_value(from, to, slide, b) - eased_value(from, to, slide, a)) / (b - a)
}

/// Gboard's slide up, as Chrome draws it.
pub const SHOW_SLIDE: KeyboardCurve = KeyboardCurve {
    duration_ms: android::SHOW_MS,
    points: android::SHOW_CURVE,
};

/// Gboard's slide down.
pub const HIDE_SLIDE: KeyboardCurve = KeyboardCurve {
    duration_ms: android::HIDE_MS,
    points: android::HIDE_CURVE,
};

/// Upstream `createKeyboardModel`: the guide's core. Tests drive it with explicit times,
/// and the window's guide feeds it events.
///
/// - Show: from the report, dated back `SHOW_REPORT_LAG_MS`, on Gboard's show curve,
///   joining the slide where it is. A smaller first report is the accessory bar: it rides
///   to the last keyboard's height as a prediction, which the keyboard's report confirms.
/// - Hide: on the hide curve. It starts `HIDE_AFTER_BLUR_MS` after a blur that brings no
///   keyboard, as a prediction; with nothing predicting it (Back), from Chrome's report
///   dated back `HIDE_REPORT_LAG_MS`.
/// - A keyboard that changes while up lays out at once.
///
/// A prediction the viewport has not confirmed by its end gives way to what the viewport
/// says ([`KeyboardModel::verify`]).
#[derive(Clone, Debug)]
pub struct KeyboardModel {
    transition: KeyboardTransition,
    curve: Option<InsetCurve>,
    /// The inset of the last keyboard that was up, for a hide that turns back.
    shown_inset: f64,
}

impl KeyboardModel {
    /// A model at rest at `initial_inset`.
    pub fn new(initial_inset: f64) -> Self {
        KeyboardModel {
            transition: KeyboardTransition {
                kind: KeyboardTransitionKind::Change,
                from: initial_inset,
                to: initial_inset,
                start: f64::NEG_INFINITY,
                velocity: 0.,
                animated: false,
                predicted: false,
                curve: None,
            },
            curve: None,
            shown_inset: initial_inset,
        }
    }

    /// The inset the keyboard rests at (or is heading to), px.
    pub fn inset(&self) -> f64 {
        self.transition.to
    }

    /// The latest transition.
    pub fn transition(&self) -> KeyboardTransition {
        self.transition
    }

    fn length(&self) -> f64 {
        self.transition.curve.map_or(0., |c| c.duration_ms)
    }

    /// The inset presented at `time`, px.
    pub fn presentation(&self, time: f64) -> f64 {
        if time < self.transition.start {
            return self.transition.from;
        }
        match self.curve {
            Some(curve) if time < self.transition.start + self.length() => {
                curve.value((time - self.transition.start) / 1000.)
            }
            _ => self.transition.to,
        }
    }

    /// The presented inset's velocity at `time`, px/s.
    pub fn velocity(&self, time: f64) -> f64 {
        if time <= self.transition.start {
            return self.transition.velocity;
        }
        match self.curve {
            Some(curve) if time < self.transition.start + self.length() => {
                curve.velocity((time - self.transition.start) / 1000.)
            }
            _ => 0.,
        }
    }

    fn begin(
        &mut self,
        kind: KeyboardTransitionKind,
        to: f64,
        start: f64,
        animated: bool,
        predicted: bool,
    ) -> KeyboardTransition {
        // Never date a transition before the one it interrupts.
        let at = start.max(self.transition.start);
        let from = self.presentation(at);
        let v = if animated { self.velocity(at) } else { 0. };
        let slide = animated.then_some(if to > from { SHOW_SLIDE } else { HIDE_SLIDE });
        self.transition = KeyboardTransition {
            kind,
            from,
            to,
            start: at,
            velocity: v,
            animated,
            predicted,
            curve: slide,
        };
        self.curve = slide
            .filter(|_| (to - from).abs() + v.abs() > 0.)
            .map(|slide| InsetCurve { from, to, slide });
        if to > EPSILON {
            self.shown_inset = to;
        }
        self.transition
    }

    fn confirm(&mut self) -> Option<KeyboardTransition> {
        self.transition.predicted = false;
        None
    }

    /// The visual viewport reports an inset (`None` when unknown) at `time`. Returns the
    /// transition the report starts, if any.
    pub fn report(&mut self, inset: Option<f64>, time: f64) -> Option<KeyboardTransition> {
        let inset = inset?;
        let target = self.transition.to;
        if (inset - target).abs() < EPSILON {
            return self.confirm();
        }
        if inset <= EPSILON {
            // A hide nothing predicted (Back): Chrome reports it just after Gboard starts
            // down, so it rides from that moment.
            return Some(self.begin(
                KeyboardTransitionKind::Hide,
                0.,
                time - android::HIDE_REPORT_LAG_MS,
                true,
                false,
            ));
        }
        if target <= EPSILON {
            // The show, joined where its slide is. A smaller first report is the accessory
            // bar: ride to the last keyboard's height and let its report confirm it.
            let predicted = self.shown_inset > inset + EPSILON;
            let to = if predicted { self.shown_inset } else { inset };
            return Some(self.begin(
                KeyboardTransitionKind::Show,
                to,
                time - android::SHOW_REPORT_LAG_MS,
                true,
                predicted,
            ));
        }
        // The keyboard changed while up: Chrome lays the page out at once.
        Some(self.begin(KeyboardTransitionKind::Change, inset, time, false, false))
    }

    /// A control that brings up a keyboard lost focus to nothing that brings up one.
    pub fn blurred(&mut self, time: f64) -> Option<KeyboardTransition> {
        if self.transition.to <= EPSILON {
            return None;
        }
        Some(self.begin(
            KeyboardTransitionKind::Hide,
            0.,
            time + android::HIDE_AFTER_BLUR_MS,
            true,
            true,
        ))
    }

    /// A control that brings up a keyboard gained focus.
    pub fn focused(&mut self, time: f64) -> Option<KeyboardTransition> {
        if self.transition.kind != KeyboardTransitionKind::Hide || !self.transition.predicted {
            return None;
        }
        // Focus came back before the predicted hide finished: the keyboard turns back to
        // the inset it had.
        let start = time.max(self.transition.start);
        let to = self.shown_inset;
        Some(self.begin(KeyboardTransitionKind::Show, to, start, true, true))
    }

    /// Checks a predicted transition against the viewport once it should have ended; moves
    /// to what the viewport says when they disagree.
    pub fn verify(&mut self, inset: Option<f64>, time: f64) -> Option<KeyboardTransition> {
        let inset = inset?;
        if !self.transition.predicted || time < self.transition.start + self.length() {
            return None;
        }
        if (inset - self.transition.to).abs() < EPSILON {
            return self.confirm();
        }
        let kind = if inset > self.transition.to {
            KeyboardTransitionKind::Show
        } else {
            KeyboardTransitionKind::Hide
        };
        Some(self.begin(kind, inset, time, true, false))
    }
}
