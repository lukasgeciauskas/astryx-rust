//! `UILongPressGestureRecognizer`, upstream
//! `gestures/LongPressGestureRecognizer.ts`.
//!
//! Begins once `number_of_touches_required` fingers have stayed down for
//! `minimum_press_duration` without any travelling further than
//! `allowable_movement` (after `number_of_taps_required` quick taps, when
//! set). After it begins, movement is unlimited and reports changed; lifting
//! a finger ends it. Lifting or travelling before the duration fails it.
//! With a duration of zero it begins on the touch itself, in the same event
//! (a selection tracker over a group of actions). The begin time is exact: a
//! deadline on the event clock.

use super::geometry::travel;
use super::recognizer::{
    AnyRecognizer, MANIPULATION, NativeTouchBehavior, Recognizer, RecognizerBehavior,
    RecognizerFamily, RecognizerKind, RecognizerOptions,
};
use super::selection_lock::lock_selection;
use super::types::{GestureEventContext, GestureState, GestureTouch};
use crate::motion::uikit::gestures::{long_press as LONG_PRESS, tap as TAP};

const PRESS_TIMER: u32 = 0;
const FAIL_TIMER: u32 = 1;

/// Upstream `LongPressGestureRecognizerOptions`.
#[derive(Clone)]
pub struct LongPressOptions {
    pub base: RecognizerOptions,
    pub minimum_press_duration: f64,
    pub allowable_movement: f64,
    /// Quick taps before the press. @default 0
    pub number_of_taps_required: u32,
    /// @default 1
    pub number_of_touches_required: usize,
    /// A second finger after the press began cancels it, nothing acting (upstream
    /// `ActionGroupPress`); a plain long press ignores it. @default false
    pub cancels_on_extra_touch: bool,
    /// The press's clock inside a scroller (upstream `receiveTouch`): a finger's or a
    /// pen's press that lands in one waits this duration and fails past this movement, so
    /// the scroller takes a flick. @default none
    pub scroller_press: Option<(f64, f64)>,
}

impl Default for LongPressOptions {
    fn default() -> Self {
        Self {
            base: RecognizerOptions::default(),
            minimum_press_duration: LONG_PRESS::MINIMUM_PRESS_DURATION,
            allowable_movement: LONG_PRESS::ALLOWABLE_MOVEMENT,
            number_of_taps_required: 0,
            number_of_touches_required: 1,
            cancels_on_extra_touch: false,
            scroller_press: None,
        }
    }
}

impl LongPressOptions {
    /// Default options with a name.
    pub fn named(name: impl Into<String>) -> Self {
        Self {
            base: RecognizerOptions::named(name),
            ..Self::default()
        }
    }
}

/// The long press's behavior.
pub struct LongPress {
    pub minimum_press_duration: f64,
    pub allowable_movement: f64,
    pub number_of_taps_required: u32,
    pub number_of_touches_required: usize,
    taps_done: u32,
    tap_started_at: Option<f64>,
    tap_most_touches: usize,
    is_press_timer_armed: bool,
    /// When the gesture began, on the event clock.
    pub began_at: Option<f64>,
    pub cancels_on_extra_touch: bool,
    pub scroller_press: Option<(f64, f64)>,
    /// The duration and movement outside a scroller.
    pub plain_press: (f64, f64),
}

/// `LongPressGestureRecognizer`.
pub type LongPressGestureRecognizer = Recognizer<LongPress>;

impl Recognizer<LongPress> {
    /// A long-press recognizer. A press with a hold (a duration above 0)
    /// brings the selection lock unless the options name one (FR41).
    pub fn new(options: LongPressOptions) -> Self {
        let mut base = options.base;
        if base.selection_lock.is_none() {
            base.selection_lock = Some((options.minimum_press_duration > 0.).then(lock_selection));
        }
        let behavior = LongPress {
            minimum_press_duration: options.minimum_press_duration,
            allowable_movement: options.allowable_movement,
            number_of_taps_required: options.number_of_taps_required,
            number_of_touches_required: options.number_of_touches_required,
            taps_done: 0,
            tap_started_at: None,
            tap_most_touches: 0,
            is_press_timer_armed: false,
            began_at: None,
            cancels_on_extra_touch: options.cancels_on_extra_touch,
            scroller_press: options.scroller_press,
            plain_press: (options.minimum_press_duration, options.allowable_movement),
        };
        Self::with_behavior(base, behavior, "LongPressGestureRecognizer")
    }

    /// When the gesture began, on the event clock.
    pub fn began_at(&self) -> Option<f64> {
        self.behavior().began_at
    }
}

impl RecognizerBehavior for LongPress {
    fn kind(&self) -> RecognizerKind {
        RecognizerKind::LongPress
    }
    fn family(&self) -> Option<RecognizerFamily> {
        Some(RecognizerFamily::LongPress)
    }
    fn native_touch_behavior(&self, _r: &AnyRecognizer) -> NativeTouchBehavior {
        MANIPULATION
    }

    fn accepts_touch(&self, r: &AnyRecognizer, touch: &GestureTouch) -> bool {
        // Extra fingers after the press began do not end or move it, unless they cancel it.
        if r.state().is_active() {
            // The extra finger is taken, so its tap acts on nothing, and it cancels the
            // press as it lands (touches_began).
            return self.cancels_on_extra_touch;
        }
        r.base_accepts_touch(touch)
    }

    fn touches_began(
        &mut self,
        r: &AnyRecognizer,
        touches: &[GestureTouch],
        event: &GestureEventContext,
    ) {
        let count = r.number_of_touches();
        if self.cancels_on_extra_touch && r.state().is_active() && count > 1 {
            r.set_state(GestureState::Cancelled);
            return;
        }
        if let (Some((delay, slop)), Some(first)) = (self.scroller_press, touches.first())
            && count == touches.len()
        {
            let inside = first.pointer_type() != super::types::PointerKind::Mouse
                && first
                    .scroll()
                    .is_some_and(|probe| probe.is_inside_scroller());
            (self.minimum_press_duration, self.allowable_movement) = if inside {
                (delay, slop)
            } else {
                self.plain_press
            };
        }
        if count > self.number_of_touches_required {
            r.set_state(GestureState::Failed);
            return;
        }
        if self.taps_done > 0 {
            // The next tap (or the press) landed in time.
            r.clear_timer();
        }
        if self.taps_done < self.number_of_taps_required {
            self.tap_started_at.get_or_insert(event.time);
            self.tap_most_touches = self.tap_most_touches.max(count);
        }
        if count == self.number_of_touches_required
            && self.taps_done >= self.number_of_taps_required
        {
            if self.minimum_press_duration <= 0. {
                self.began_at = Some(event.time);
                r.set_state(GestureState::Began);
                return;
            }
            self.is_press_timer_armed = true;
            r.set_timer(event.time + self.minimum_press_duration, PRESS_TIMER);
        }
    }

    fn timer_fired(&mut self, r: &AnyRecognizer, tag: u32) {
        if tag == FAIL_TIMER {
            r.set_state(GestureState::Failed);
            return;
        }
        self.is_press_timer_armed = false;
        if r.state() == GestureState::Possible
            && r.number_of_touches() == self.number_of_touches_required
        {
            self.began_at = Some(r.now());
            r.set_state(GestureState::Began);
        }
    }

    fn touches_moved(
        &mut self,
        r: &AnyRecognizer,
        touches: &[GestureTouch],
        _event: &GestureEventContext,
    ) {
        let state = r.state();
        if state == GestureState::Possible {
            if touches.iter().any(|t| travel(t) > self.allowable_movement) {
                r.set_state(GestureState::Failed);
            }
            return;
        }
        if state.is_active() {
            r.set_state(GestureState::Changed);
        }
    }

    fn touches_ended(
        &mut self,
        r: &AnyRecognizer,
        touches: &[GestureTouch],
        event: &GestureEventContext,
    ) {
        if r.state().is_active() {
            r.set_state(GestureState::Ended);
            return;
        }
        if self.is_press_timer_armed || self.taps_done >= self.number_of_taps_required {
            // Lifted before the duration: not a long press.
            r.set_state(GestureState::Failed);
            return;
        }
        if r.number_of_touches() > touches.len() {
            return;
        }
        // A completed tap of the taps phase. GUESS: its hold limit and
        // interval are the tap recognizer's (M-G20).
        let held_for = event.time - self.tap_started_at.unwrap_or(event.time);
        let fingers = self.tap_most_touches;
        self.tap_started_at = None;
        self.tap_most_touches = 0;
        if held_for > TAP::MAXIMUM_TAP_DURATION || fingers != self.number_of_touches_required {
            r.set_state(GestureState::Failed);
            return;
        }
        self.taps_done += 1;
        r.set_waiting(true);
        r.set_timer(event.time + TAP::MAXIMUM_INTERVAL_BETWEEN_TAPS, FAIL_TIMER);
    }

    fn reset(&mut self) {
        self.taps_done = 0;
        self.tap_started_at = None;
        self.tap_most_touches = 0;
        self.is_press_timer_armed = false;
        self.began_at = None;
    }
}
