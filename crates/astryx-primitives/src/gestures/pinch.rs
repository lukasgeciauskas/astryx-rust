//! `UIPinchGestureRecognizer`, upstream `gestures/PinchGestureRecognizer.ts`.
//!
//! Tracks the first two fingers; more are ignored. Begins once the distance
//! between them has changed by `hysteresis` px. `scale` is then the current
//! distance over the starting distance moved by the hysteresis toward the
//! pinch, so it does not jump (settable to re-base it); `velocity` is scale
//! per second. Lifting either finger ends an active pinch and fails a
//! possible one. A ctrl-wheel (a trackpad pinch) drives it too.

use super::geometry::{centroid, distance};
use super::recognizer::{
    AnyRecognizer, NativeTouchBehavior, Recognizer, RecognizerBehavior, RecognizerFamily,
    RecognizerKind, RecognizerOptions,
};
use super::types::{GestureEventContext, GestureState, GestureTouch, Point, ScrollType};
use super::velocity_tracker::VelocityTracker;
use crate::motion::uikit::gestures::pinch as PINCH;

/// Upstream `PinchGestureRecognizerOptions`.
#[derive(Clone)]
pub struct PinchOptions {
    pub base: RecognizerOptions,
    pub hysteresis: f64,
}

impl Default for PinchOptions {
    fn default() -> Self {
        Self {
            base: RecognizerOptions::default(),
            hysteresis: PINCH::HYSTERESIS,
        }
    }
}

impl PinchOptions {
    /// Default options with a name.
    pub fn named(name: impl Into<String>) -> Self {
        Self {
            base: RecognizerOptions::named(name),
            ..Self::default()
        }
    }
}

/// The pinch's behavior.
pub struct Pinch {
    pub hysteresis: f64,
    initial_span: f64,
    scale: f64,
    tracker: VelocityTracker,
    lift_time: Option<f64>,
}

/// `PinchGestureRecognizer`.
pub type PinchGestureRecognizer = Recognizer<Pinch>;

fn span(touches: &[GestureTouch]) -> f64 {
    match touches {
        [a, b, ..] => distance(a.position(), b.position()),
        _ => 0.,
    }
}

impl Recognizer<Pinch> {
    /// A pinch recognizer.
    pub fn new(options: PinchOptions) -> Self {
        let behavior = Pinch {
            hysteresis: options.hysteresis,
            initial_span: 0.,
            scale: 1.,
            tracker: VelocityTracker::default(),
            lift_time: None,
        };
        Self::with_behavior(options.base, behavior, "PinchGestureRecognizer")
    }

    /// The current span over the span the fingers started at.
    pub fn scale(&self) -> f64 {
        self.behavior().scale
    }

    /// Re-bases: from now on the scale reads `value` at the current span.
    pub fn set_scale(&self, value: f64) {
        let span = span(&self.tracked_touches());
        let mut pinch = self.behavior_mut();
        if span > 0. && value > 0. {
            let factor = if pinch.scale > 0. {
                value / pinch.scale
            } else {
                1.
            };
            pinch.initial_span = span / value;
            pinch.scale = value;
            // The velocity continues from the re-based value, without a jump.
            pinch.tracker.scale_by(factor);
        }
    }

    /// Scale per second (UIKit's filter).
    pub fn velocity(&self) -> f64 {
        let pinch = self.behavior();
        pinch.tracker.velocity(pinch.lift_time).x
    }

    /// The midpoint between the two fingers.
    pub fn focal_point(&self) -> Point {
        centroid(&self.tracked_touches(), &[])
    }
}

impl RecognizerBehavior for Pinch {
    fn kind(&self) -> RecognizerKind {
        RecognizerKind::Pinch
    }
    fn family(&self) -> Option<RecognizerFamily> {
        Some(RecognizerFamily::Pinch)
    }
    /// A ctrl-wheel drives a pinch, from a trackpad or a mouse.
    fn default_scroll_types(&self) -> Vec<ScrollType> {
        vec![ScrollType::Continuous, ScrollType::Discrete]
    }
    /// Two fingers on this view are this recognizer's, not the window's zoom.
    fn native_touch_behavior(&self, _r: &AnyRecognizer) -> NativeTouchBehavior {
        NativeTouchBehavior {
            pan_x: true,
            pan_y: true,
            pinch_zoom: false,
            double_tap_zoom: false,
        }
    }
    fn accepts_touch(&self, r: &AnyRecognizer, touch: &GestureTouch) -> bool {
        r.number_of_touches() < 2 && r.base_accepts_touch(touch)
    }

    fn touches_began(
        &mut self,
        r: &AnyRecognizer,
        _touches: &[GestureTouch],
        event: &GestureEventContext,
    ) {
        if r.number_of_touches() == 2 {
            self.initial_span = span(&r.tracked_touches());
            self.scale = 1.;
            self.tracker.reset();
            self.tracker.add(1., 0., event.time);
        }
    }

    fn touches_moved(
        &mut self,
        r: &AnyRecognizer,
        _touches: &[GestureTouch],
        event: &GestureEventContext,
    ) {
        if r.number_of_touches() < 2 || self.initial_span <= 0. {
            return;
        }
        let span = span(&r.tracked_touches());
        // Once recognized, a pinch waiting on a requirement keeps its
        // re-based start.
        if r.state() == GestureState::Possible && r.pending().is_none() {
            if (span - self.initial_span).abs() >= self.hysteresis {
                self.initial_span +=
                    super::geometry::sign(span - self.initial_span) * self.hysteresis;
                self.scale = span / self.initial_span;
                self.tracker.add(self.scale, 0., event.time);
                r.set_state(GestureState::Began);
            }
            return;
        }
        self.scale = span / self.initial_span;
        // Every move of the fingers is a sample, so the velocity never goes
        // stale while the span holds.
        self.tracker.add_motion(self.scale, 0., event.time);
        if r.state().is_active() {
            r.set_state(GestureState::Changed);
        }
    }

    fn touches_ended(
        &mut self,
        r: &AnyRecognizer,
        _touches: &[GestureTouch],
        event: &GestureEventContext,
    ) {
        self.lift_time = Some(event.time);
        r.set_state(if r.state().is_active() {
            GestureState::Ended
        } else {
            GestureState::Failed
        });
    }

    fn reset(&mut self) {
        self.lift_time = None;
        self.initial_span = 0.;
        self.scale = 1.;
        self.tracker.reset();
    }
}
