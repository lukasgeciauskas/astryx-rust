//! `UISwipeGestureRecognizer`, upstream `gestures/SwipeGestureRecognizer.ts`.
//!
//! Recognized while the fingers move, as UIKit's is: as soon as the centroid
//! has travelled `minimum_distance` along an allowed direction, within
//! `maximum_duration` of touch-down, with off-axis travel no more than
//! `maximum_secondary_movement` across it. Off-axis travel past that, or the
//! distance travelled the wrong way, rules a direction out; the swipe fails
//! once every allowed direction is ruled out, as it does for too many
//! fingers, the duration running out, and a lift before it was recognized.

use super::geometry::centroid;
use super::recognizer::{
    AnyRecognizer, NativeTouchBehavior, Recognizer, RecognizerBehavior, RecognizerKind,
    RecognizerOptions,
};
use super::types::{GestureEventContext, GestureState, GestureTouch, Point};
use crate::motion::uikit::gestures::swipe as SWIPE;

/// A swipe's direction.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SwipeDirection {
    Left,
    Right,
    Up,
    Down,
}

/// Upstream `SwipeGestureRecognizerOptions`.
#[derive(Clone)]
pub struct SwipeOptions {
    pub base: RecognizerOptions,
    /// @default [Right]
    pub direction: Vec<SwipeDirection>,
    /// @default 1
    pub number_of_touches_required: usize,
    pub minimum_distance: f64,
    pub maximum_duration: f64,
    pub maximum_secondary_movement: f64,
}

impl Default for SwipeOptions {
    fn default() -> Self {
        Self {
            base: RecognizerOptions::default(),
            direction: vec![SwipeDirection::Right],
            number_of_touches_required: 1,
            minimum_distance: SWIPE::MINIMUM_DISTANCE,
            maximum_duration: SWIPE::MAXIMUM_DURATION,
            maximum_secondary_movement: SWIPE::MAXIMUM_SECONDARY_MOVEMENT,
        }
    }
}

impl SwipeOptions {
    /// Default options with a name.
    pub fn named(name: impl Into<String>) -> Self {
        Self {
            base: RecognizerOptions::named(name),
            ..Self::default()
        }
    }
}

/// The swipe's behavior.
pub struct Swipe {
    pub directions: Vec<SwipeDirection>,
    pub number_of_touches_required: usize,
    pub minimum_distance: f64,
    pub maximum_duration: f64,
    pub maximum_secondary_movement: f64,
    /// The direction of the recognized swipe.
    pub recognized_direction: Option<SwipeDirection>,
    origin: Point,
    ruled_out: Vec<SwipeDirection>,
    last: Option<Point>,
}

/// `SwipeGestureRecognizer`.
pub type SwipeGestureRecognizer = Recognizer<Swipe>;

impl Recognizer<Swipe> {
    /// A swipe recognizer.
    pub fn new(options: SwipeOptions) -> Self {
        let behavior = Swipe {
            directions: options.direction,
            number_of_touches_required: options.number_of_touches_required,
            minimum_distance: options.minimum_distance,
            maximum_duration: options.maximum_duration,
            maximum_secondary_movement: options.maximum_secondary_movement,
            recognized_direction: None,
            origin: Point::ZERO,
            ruled_out: Vec::new(),
            last: None,
        };
        Self::with_behavior(options.base, behavior, "SwipeGestureRecognizer")
    }

    /// The direction of the recognized swipe.
    pub fn recognized_direction(&self) -> Option<SwipeDirection> {
        self.behavior().recognized_direction
    }
}

/// Where on the segment from `start` to `end` (0 to 1) the value first
/// reaches `limit`; 0 when it starts there, -1 when it never does.
fn segment_crossing(start: f64, end: f64, limit: f64) -> f64 {
    if start >= limit {
        return 0.;
    }
    if end < limit {
        return -1.;
    }
    (limit - start) / (end - start)
}

impl Swipe {
    /// Travel along and across `direction` from the origin to `point`.
    fn project(&self, direction: SwipeDirection, point: Point) -> (f64, f64) {
        let dx = point.x - self.origin.x;
        let dy = point.y - self.origin.y;
        match direction {
            SwipeDirection::Left => (-dx, dy),
            SwipeDirection::Right => (dx, dy),
            SwipeDirection::Up => (-dy, dx),
            SwipeDirection::Down => (dy, dx),
        }
    }
}

impl RecognizerBehavior for Swipe {
    fn kind(&self) -> RecognizerKind {
        RecognizerKind::Swipe
    }
    fn is_discrete(&self) -> bool {
        true
    }
    fn native_touch_behavior(&self, _r: &AnyRecognizer) -> NativeTouchBehavior {
        let horizontal = self
            .directions
            .iter()
            .any(|d| matches!(d, SwipeDirection::Left | SwipeDirection::Right));
        let vertical = self
            .directions
            .iter()
            .any(|d| matches!(d, SwipeDirection::Up | SwipeDirection::Down));
        NativeTouchBehavior {
            pan_x: !horizontal,
            pan_y: !vertical,
            pinch_zoom: true,
            double_tap_zoom: false,
        }
    }

    fn touches_began(
        &mut self,
        r: &AnyRecognizer,
        touches: &[GestureTouch],
        event: &GestureEventContext,
    ) {
        let count = r.number_of_touches();
        if count > self.number_of_touches_required {
            r.set_state(GestureState::Failed);
            return;
        }
        if count == touches.len() {
            r.set_timer(event.time + self.maximum_duration, 0);
        }
        // Measure from where every required finger is down.
        self.origin = centroid(&r.tracked_touches(), &[]);
        self.last = None;
    }

    fn timer_fired(&mut self, r: &AnyRecognizer, _tag: u32) {
        // A swipe recognized and waiting on a requirement holds (FR15).
        if r.pending().is_none() {
            r.set_state(GestureState::Failed);
        }
    }

    fn touches_moved(
        &mut self,
        r: &AnyRecognizer,
        _touches: &[GestureTouch],
        _event: &GestureEventContext,
    ) {
        if r.pending().is_some() || r.number_of_touches() != self.number_of_touches_required {
            return;
        }
        let current = centroid(&r.tracked_touches(), &[]);
        let from = self.last.unwrap_or(self.origin);
        self.last = Some(current);
        for direction in self.directions.clone() {
            if self.ruled_out.contains(&direction) {
                continue;
            }
            // The path between two events is read as a straight segment.
            let (along0, across0) = self.project(direction, from);
            let (along1, across1) = self.project(direction, current);
            let reaches = segment_crossing(along0, along1, self.minimum_distance);
            let strays = segment_crossing(across0, across1, self.maximum_secondary_movement).max(
                segment_crossing(-across0, -across1, self.maximum_secondary_movement),
            );
            let stray_at = if strays < 0. { f64::INFINITY } else { strays };
            if reaches >= 0. && reaches <= stray_at {
                self.recognized_direction = Some(direction);
                r.set_state(GestureState::Ended);
                return;
            }
            if strays >= 0. || along1 <= -self.minimum_distance {
                self.ruled_out.push(direction);
            }
        }
        if self.directions.iter().all(|d| self.ruled_out.contains(d)) {
            r.set_state(GestureState::Failed);
        }
    }

    fn touches_ended(
        &mut self,
        r: &AnyRecognizer,
        _touches: &[GestureTouch],
        _event: &GestureEventContext,
    ) {
        // Recognized and waiting, it holds past the lift (FR15).
        if r.pending().is_none() {
            r.set_state(GestureState::Failed);
        }
    }

    fn reset(&mut self) {
        self.recognized_direction = None;
        self.ruled_out.clear();
        self.last = None;
        self.origin = Point::ZERO;
    }
}
