//! `UITapGestureRecognizer`, upstream `gestures/TapGestureRecognizer.ts`.
//!
//! Recognizes when `number_of_taps_required` taps of
//! `number_of_touches_required` fingers each have lifted. A tap fails when
//! a finger travels further than `allowable_movement`, when more fingers
//! land than required, when the next tap does not land within
//! `maximum_interval_between_taps` of the previous lift or lands further
//! than `allowable_separation` from the first, or when a finger stays down
//! longer than `maximum_tap_duration`. With several fingers a tap is placed
//! at their centroid: the first where its fingers lifted, the next where its
//! fingers landed, once all of them have. Between taps the recognizer stays
//! possible with no touches, so a single tap that requires a double tap to
//! fail waits the interval out.

use super::geometry::{distance, travel};
use super::recognizer::{
    AnyRecognizer, MANIPULATION, NativeTouchBehavior, Recognizer, RecognizerBehavior,
    RecognizerFamily, RecognizerKind, RecognizerOptions,
};
use super::types::{GestureEventContext, GestureState, GestureTouch, Point};
use crate::motion::uikit::gestures::tap as TAP;

/// Upstream `TapGestureRecognizerOptions`.
#[derive(Clone)]
pub struct TapOptions {
    pub base: RecognizerOptions,
    /// @default 1
    pub number_of_taps_required: u32,
    /// @default 1
    pub number_of_touches_required: usize,
    pub allowable_movement: f64,
    pub maximum_interval_between_taps: f64,
    pub allowable_separation: f64,
    pub maximum_tap_duration: f64,
}

impl Default for TapOptions {
    fn default() -> Self {
        Self {
            base: RecognizerOptions::default(),
            number_of_taps_required: 1,
            number_of_touches_required: 1,
            allowable_movement: TAP::ALLOWABLE_MOVEMENT,
            maximum_interval_between_taps: TAP::MAXIMUM_INTERVAL_BETWEEN_TAPS,
            allowable_separation: TAP::ALLOWABLE_SEPARATION,
            maximum_tap_duration: TAP::MAXIMUM_TAP_DURATION,
        }
    }
}

impl TapOptions {
    /// Default options with a name.
    pub fn named(name: impl Into<String>) -> Self {
        Self {
            base: RecognizerOptions::named(name),
            ..Self::default()
        }
    }
}

/// The tap's behavior; its settings are public fields.
pub struct Tap {
    pub number_of_taps_required: u32,
    pub number_of_touches_required: usize,
    pub allowable_movement: f64,
    pub maximum_interval_between_taps: f64,
    pub allowable_separation: f64,
    pub maximum_tap_duration: f64,
    tap_count: u32,
    is_tap_in_progress: bool,
    max_touches_this_tap: usize,
    first_tap_location: Option<Point>,
    tap_starts: Vec<(u64, Point)>,
    tap_lifts: Vec<(u64, Point)>,
}

/// `TapGestureRecognizer`.
pub type TapGestureRecognizer = Recognizer<Tap>;

impl Recognizer<Tap> {
    /// A tap recognizer.
    pub fn new(options: TapOptions) -> Self {
        let behavior = Tap {
            number_of_taps_required: options.number_of_taps_required,
            number_of_touches_required: options.number_of_touches_required,
            allowable_movement: options.allowable_movement,
            maximum_interval_between_taps: options.maximum_interval_between_taps,
            allowable_separation: options.allowable_separation,
            maximum_tap_duration: options.maximum_tap_duration,
            tap_count: 0,
            is_tap_in_progress: false,
            max_touches_this_tap: 0,
            first_tap_location: None,
            tap_starts: Vec::new(),
            tap_lifts: Vec::new(),
        };
        Self::with_behavior(options.base, behavior, "TapGestureRecognizer")
    }

    /// Taps completed so far in the current sequence.
    pub fn completed_taps(&self) -> u32 {
        self.behavior().tap_count
    }
}

fn upsert(points: &mut Vec<(u64, Point)>, id: u64, point: Point) {
    match points.iter_mut().find(|(i, _)| *i == id) {
        Some(entry) => entry.1 = point,
        None => points.push((id, point)),
    }
}

fn center_of(points: &[(u64, Point)]) -> Point {
    if points.is_empty() {
        return Point::ZERO;
    }
    let n = points.len() as f64;
    let (x, y) = points
        .iter()
        .fold((0., 0.), |(x, y), (_, p)| (x + p.x, y + p.y));
    Point::new(x / n, y / n)
}

impl RecognizerBehavior for Tap {
    fn kind(&self) -> RecognizerKind {
        RecognizerKind::Tap
    }
    fn family(&self) -> Option<RecognizerFamily> {
        Some(RecognizerFamily::Tap)
    }
    fn is_discrete(&self) -> bool {
        true
    }
    fn native_touch_behavior(&self, _r: &AnyRecognizer) -> NativeTouchBehavior {
        MANIPULATION
    }

    fn touches_began(
        &mut self,
        r: &AnyRecognizer,
        touches: &[GestureTouch],
        event: &GestureEventContext,
    ) {
        if !self.is_tap_in_progress {
            self.is_tap_in_progress = true;
            self.tap_starts.clear();
            self.tap_lifts.clear();
            self.max_touches_this_tap = 0;
            r.clear_timer();
            if self.maximum_tap_duration.is_finite() {
                r.set_timer(event.time + self.maximum_tap_duration, 0);
            }
        }
        for touch in touches {
            upsert(&mut self.tap_starts, touch.id(), touch.start());
        }
        let count = r.number_of_touches();
        self.max_touches_this_tap = self.max_touches_this_tap.max(count);
        if count > self.number_of_touches_required {
            r.set_state(GestureState::Failed);
            return;
        }
        // A later tap must land near the first, centroid to centroid, once
        // all of its fingers have landed.
        if let Some(first) = self.first_tap_location
            && self.tap_starts.len() == self.number_of_touches_required
            && distance(center_of(&self.tap_starts), first) > self.allowable_separation
        {
            r.set_state(GestureState::Failed);
        }
    }

    fn touches_moved(
        &mut self,
        r: &AnyRecognizer,
        touches: &[GestureTouch],
        _event: &GestureEventContext,
    ) {
        if touches.iter().any(|t| travel(t) > self.allowable_movement) {
            r.set_state(GestureState::Failed);
        }
    }

    fn touches_ended(
        &mut self,
        r: &AnyRecognizer,
        touches: &[GestureTouch],
        event: &GestureEventContext,
    ) {
        if touches.iter().any(|t| travel(t) > self.allowable_movement) {
            r.set_state(GestureState::Failed);
            return;
        }
        for touch in touches {
            upsert(&mut self.tap_lifts, touch.id(), touch.position());
        }
        if r.number_of_touches() > touches.len() {
            // Wait for every finger of this tap to lift.
            return;
        }
        self.is_tap_in_progress = false;
        r.clear_timer();
        if self.max_touches_this_tap != self.number_of_touches_required {
            r.set_state(GestureState::Failed);
            return;
        }
        self.tap_count += 1;
        r.set_waiting(true);
        if self.first_tap_location.is_none() {
            self.first_tap_location = Some(center_of(&self.tap_lifts));
        }
        if self.tap_count >= self.number_of_taps_required {
            r.set_state(GestureState::Ended);
            return;
        }
        r.set_timer(event.time + self.maximum_interval_between_taps, 0);
    }

    fn timer_fired(&mut self, r: &AnyRecognizer, _tag: u32) {
        r.set_state(GestureState::Failed);
    }

    fn reset(&mut self) {
        self.tap_count = 0;
        self.is_tap_in_progress = false;
        self.max_touches_this_tap = 0;
        self.first_tap_location = None;
        self.tap_starts.clear();
        self.tap_lifts.clear();
    }
}
