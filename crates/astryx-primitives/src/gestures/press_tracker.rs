//! A pressable control's view-level touch tracking, upstream
//! `gestures/PressTracker.ts` (`UIControl`'s `beginTracking`,
//! `continueTracking` and `endTracking`), and its profiles.
//!
//! - `TouchDown` when the finger lands; the highlight comes on in the same
//!   frame, unless the control sits in a scroller, where it waits
//!   `contentTouchDelay` from the touch's own time, so a flick paints
//!   nothing.
//! - `TouchDragInside` / `TouchDragOutside` on every move, `TouchDragExit` /
//!   `TouchDragEnter` when the finger crosses the bounds expanded by
//!   `tracking_expansion`.
//! - `TouchUpInside` / `TouchUpOutside` at the lift. A tap quicker than the
//!   delay shows the highlight at the lift for `quick_tap_flash`.
//! - `TouchCancel` when a recognizer or the window took the touch.
//!
//! And `UIControl`'s iOS 6 rule: an ancestor's single-finger single tap does
//! not beat the control's own action for a touch on it. The bounds are read
//! once, when the finger lands.

use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

use super::arena::{GestureClock, TimerHandle};
use super::geometry::travel;
use super::recognizer::{AnyRecognizer, RecognizerKind};
use super::selection_lock::SelectionLock;
use super::tap::Tap;
use super::types::{GestureTouch, PointerKind, TouchConsumer, ViewKey};
use crate::motion::uikit::gestures::{control as CONTROL, pan as PAN, press as PRESS};

/// A control event (`UIControl.Event`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControlEvent {
    TouchDown,
    TouchDragInside,
    TouchDragOutside,
    TouchDragEnter,
    TouchDragExit,
    TouchUpInside,
    TouchUpOutside,
    TouchCancel,
}

/// A control's bounds in window px.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PressBounds {
    pub left: f64,
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
}

/// When the highlight waits `contentTouchDelay`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PressHighlightDelay {
    InScrollers,
    Always,
    Never,
}

/// How a kind of pressable tracks a finger and how long its release takes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PressProfile {
    pub highlight_delay: PressHighlightDelay,
    pub cancel_travel: Option<f64>,
    pub quick_tap_flash: f64,
    /// The release clock a painter uses, ms.
    pub release_ms: f64,
}

/// `PRESS_PROFILES`: a control's and a table cell's tracking.
pub struct PressProfiles {
    /// `UIControl`: same-frame highlight outside scrollers, 70 pt tracking.
    pub control: PressProfile,
    /// A table or collection cell: the delay inside a scroll view, at once
    /// outside one (AST-094 FR11); cancelled by the scroll.
    pub cell: PressProfile,
}

/// The two profiles.
pub const PRESS_PROFILES: PressProfiles = PressProfiles {
    control: PressProfile {
        highlight_delay: PressHighlightDelay::InScrollers,
        cancel_travel: None,
        quick_tap_flash: PRESS::QUICK_TAP_FLASH,
        release_ms: PRESS::RELEASE_MS,
    },
    cell: PressProfile {
        highlight_delay: PressHighlightDelay::InScrollers,
        cancel_travel: Some(PAN::HYSTERESIS),
        quick_tap_flash: 0.,
        release_ms: PRESS::RELEASE_MS,
    },
};

type ControlEventCallback = Rc<dyn Fn(ControlEvent, &GestureTouch)>;
type RecognizerTest = Rc<dyn Fn(&AnyRecognizer) -> bool>;
type TouchTest = Rc<dyn Fn(&GestureTouch) -> bool>;

/// Upstream `PressTrackerOptions`.
#[derive(Clone)]
pub struct PressTrackerOptions {
    /// The control's bounds, read when a finger lands.
    pub get_bounds: Rc<dyn Fn() -> Option<PressBounds>>,
    /// The view the tracker belongs to; recognizers on it are not ancestors.
    pub view: Option<ViewKey>,
    pub on_control_event: Option<ControlEventCallback>,
    pub on_highlight_change: Option<Rc<dyn Fn(bool)>>,
    /// A continuous-tracking control keeps its touch once its press is
    /// delivered. @default false
    pub keeps_touch: bool,
    /// For a control whose own hold acts (FR41).
    pub selection_lock: Option<SelectionLock>,
    /// Whether the press of a touch holds the context menu closed now.
    pub holds_context_menu: Option<Rc<dyn Fn(u64) -> bool>>,
    pub tracking_expansion: f64,
    /// @default InScrollers
    pub highlight_delay: PressHighlightDelay,
    /// Travel past which the press is cancelled for good.
    pub cancel_travel: Option<f64>,
    /// How long a quicker tap shows the highlight at the lift, ms.
    pub quick_tap_flash: f64,
    /// Which ancestor recognizers the control's own action beats.
    pub vetoes_ancestor_recognizer: Option<RecognizerTest>,
    pub is_enabled: Option<Rc<dyn Fn() -> bool>>,
    /// Whether this finger is braking a scroll it set moving; asked only
    /// for a finger (FR11).
    pub is_braking: Option<TouchTest>,
}

impl PressTrackerOptions {
    /// Options with the control profile.
    pub fn new(get_bounds: impl Fn() -> Option<PressBounds> + 'static) -> Self {
        Self {
            get_bounds: Rc::new(get_bounds),
            view: None,
            on_control_event: None,
            on_highlight_change: None,
            keeps_touch: false,
            selection_lock: None,
            holds_context_menu: None,
            tracking_expansion: CONTROL::TRACKING_EXPANSION,
            highlight_delay: PressHighlightDelay::InScrollers,
            cancel_travel: None,
            quick_tap_flash: PRESS::QUICK_TAP_FLASH,
            vetoes_ancestor_recognizer: None,
            is_enabled: None,
            is_braking: None,
        }
    }

    /// Takes a profile's tracking.
    pub fn with_profile(mut self, profile: PressProfile) -> Self {
        self.highlight_delay = profile.highlight_delay;
        self.cancel_travel = profile.cancel_travel;
        self.quick_tap_flash = profile.quick_tap_flash;
        self
    }
}

/// UIButton's rule, the default ancestor veto: an ancestor's single-finger
/// single tap.
pub fn vetoes_single_tap(recognizer: &AnyRecognizer) -> bool {
    if recognizer.kind() != RecognizerKind::Tap {
        return false;
    }
    recognizer.downcast::<Tap>().is_some_and(|tap| {
        let tap = tap.behavior();
        tap.number_of_taps_required == 1 && tap.number_of_touches_required == 1
    })
}

#[derive(Default)]
struct State {
    is_kept: bool,
    touch_id: Option<u64>,
    bounds: Option<PressBounds>,
    highlight_timer: Option<TimerHandle>,
    flash_timer: Option<TimerHandle>,
    is_pending_dead: bool,
    is_highlighted: bool,
    is_tracking: bool,
    is_touch_inside: bool,
}

struct Inner {
    options: PressTrackerOptions,
    clock: Rc<dyn GestureClock>,
    tracking_expansion: Cell<f64>,
    state: RefCell<State>,
    this: Weak<Inner>,
}

/// A control's touch tracking (upstream `PressTracker`, built by
/// `createPressTracker`). Cloning shares it; [`PressTracker::consumer`] is
/// what the host attaches.
#[derive(Clone)]
pub struct PressTracker(Rc<Inner>);

impl PressTracker {
    /// `createPressTracker`: its timers run on `clock`.
    pub fn new(options: PressTrackerOptions, clock: Rc<dyn GestureClock>) -> Self {
        let expansion = options.tracking_expansion;
        Self(Rc::new_cyclic(|this| Inner {
            options,
            clock,
            tracking_expansion: Cell::new(expansion),
            state: RefCell::new(State::default()),
            this: this.clone(),
        }))
    }

    /// The consumer the arena delivers to.
    pub fn consumer(&self) -> Rc<dyn TouchConsumer> {
        self.0.clone()
    }

    /// Whether the control paints its pressed highlight now.
    pub fn is_highlighted(&self) -> bool {
        self.0.state.borrow().is_highlighted
    }

    /// Whether a touch is being tracked.
    pub fn is_tracking(&self) -> bool {
        self.0.state.borrow().is_tracking
    }

    /// Whether the tracked touch is inside the bounds plus the expansion.
    pub fn is_touch_inside(&self) -> bool {
        self.0.state.borrow().is_touch_inside
    }

    /// How far past its bounds a tracked touch still counts as inside.
    pub fn tracking_expansion(&self) -> f64 {
        self.0.tracking_expansion.get()
    }

    pub fn set_tracking_expansion(&self, value: f64) {
        self.0.tracking_expansion.set(value);
    }

    /// Whether the control takes touches now.
    pub fn is_enabled(&self) -> bool {
        self.0.is_enabled()
    }

    /// Present only for a `keeps_touch` control: whether it keeps this touch.
    pub fn keeps_touch(&self, touch_id: u64) -> Option<bool> {
        self.0
            .options
            .keeps_touch
            .then(|| self.0.keeps_touch(touch_id))
    }

    /// Drops any highlight and timers.
    pub fn dispose(&self) {
        self.0.clear_highlight_timer();
        self.0.clear_flash();
        self.0.set_highlighted(false);
        self.0.finish();
    }

    /// The release clock of the profile in use, ms.
    pub fn release_ms(&self) -> f64 {
        PRESS::RELEASE_MS
    }
}

impl Inner {
    fn emit(&self, event: ControlEvent, touch: &GestureTouch) {
        if let Some(callback) = &self.options.on_control_event {
            callback(event, touch);
        }
    }

    fn set_highlighted(&self, value: bool) {
        let changed = {
            let mut state = self.state.borrow_mut();
            if value && state.touch_id.is_some() {
                // Delivered: the control keeps this touch until it ends.
                state.is_kept = true;
            }
            let changed = state.is_highlighted != value;
            state.is_highlighted = value;
            changed
        };
        if changed && let Some(callback) = &self.options.on_highlight_change {
            callback(value);
        }
    }

    fn clear_highlight_timer(&self) {
        let timer = self.state.borrow_mut().highlight_timer.take();
        if let Some(timer) = timer {
            self.clock.clear_timeout(timer);
        }
    }

    fn clear_flash(&self) {
        let timer = self.state.borrow_mut().flash_timer.take();
        if let Some(timer) = timer {
            self.clock.clear_timeout(timer);
        }
    }

    fn finish(&self) {
        let mut state = self.state.borrow_mut();
        state.is_kept = false;
        state.touch_id = None;
        state.is_tracking = false;
        state.is_touch_inside = false;
        state.bounds = None;
    }

    fn contains(&self, touch: &GestureTouch) -> bool {
        let Some(bounds) = self.state.borrow().bounds else {
            return true;
        };
        let margin = self.tracking_expansion.get();
        let (x, y) = (touch.x(), touch.y());
        x >= bounds.left - margin
            && x <= bounds.right + margin
            && y >= bounds.top - margin
            && y <= bounds.bottom + margin
    }

    fn tracked(&self) -> Option<u64> {
        self.state.borrow().touch_id
    }
}

impl TouchConsumer for Inner {
    fn touch_began(&self, touch: &GestureTouch) {
        if self.tracked().is_some()
            || !self.is_enabled()
            // The scroll brake is a finger's (FR11, DEC-21).
            || (touch.pointer_type() == PointerKind::Touch
                && self.options.is_braking.as_ref().is_some_and(|brakes| brakes(touch)))
        {
            return;
        }
        self.clear_flash();
        let bounds = (self.options.get_bounds)();
        {
            let mut state = self.state.borrow_mut();
            state.touch_id = Some(touch.id());
            state.bounds = bounds;
            state.is_tracking = true;
            state.is_touch_inside = true;
            state.is_pending_dead = false;
        }
        self.emit(ControlEvent::TouchDown, touch);
        let delay = self.options.highlight_delay;
        let delays = touch.pointer_type() == PointerKind::Touch
            && (delay == PressHighlightDelay::Always
                || (delay == PressHighlightDelay::InScrollers
                    && touch
                        .scroll()
                        .is_some_and(|probe| probe.is_inside_scroller())));
        if delays {
            // The delay counts from the touch, so a busy main thread's
            // lateness comes off the wait.
            let since = (self.clock.now() - touch.time()).max(0.);
            let wait = (PRESS::CONTENT_TOUCH_DELAY - since).max(0.);
            let this = self.this.clone();
            let handle = self.clock.set_timeout(
                Box::new(move || {
                    let Some(inner) = this.upgrade() else {
                        return;
                    };
                    let inside = {
                        let mut state = inner.state.borrow_mut();
                        state.highlight_timer = None;
                        state.is_touch_inside
                    };
                    if inside {
                        inner.set_highlighted(true);
                    }
                }),
                wait,
            );
            self.state.borrow_mut().highlight_timer = Some(handle);
        } else {
            self.set_highlighted(true);
        }
    }

    fn touch_moved(&self, touch: &GestureTouch) {
        if self.tracked() != Some(touch.id()) {
            return;
        }
        if let Some(cancel_travel) = self.options.cancel_travel
            && travel(touch) > cancel_travel
        {
            // A cell's scroll view took the touch: the press is over.
            self.touch_cancelled(touch);
            return;
        }
        let pending = self.state.borrow().highlight_timer.is_some();
        if pending && travel(touch) > PRESS::SLOP {
            // A finger travelling before the delay ran out is scrolling.
            self.clear_highlight_timer();
            self.state.borrow_mut().is_pending_dead = true;
        }
        let inside = self.contains(touch);
        let crossed = {
            let mut state = self.state.borrow_mut();
            let crossed = inside != state.is_touch_inside;
            if crossed {
                state.is_touch_inside = inside;
            }
            crossed
        };
        if crossed {
            self.emit(
                if inside {
                    ControlEvent::TouchDragEnter
                } else {
                    ControlEvent::TouchDragExit
                },
                touch,
            );
            let (pending, dead) = {
                let state = self.state.borrow();
                (state.highlight_timer.is_some(), state.is_pending_dead)
            };
            if !pending && !dead {
                self.set_highlighted(inside);
            }
        }
        self.emit(
            if inside {
                ControlEvent::TouchDragInside
            } else {
                ControlEvent::TouchDragOutside
            },
            touch,
        );
    }

    fn touch_ended(&self, touch: &GestureTouch) {
        if self.tracked() != Some(touch.id()) {
            return;
        }
        self.state.borrow_mut().is_kept = false;
        let inside = self.contains(touch);
        let was_pending = self.state.borrow().highlight_timer.is_some();
        self.clear_highlight_timer();
        self.emit(
            if inside {
                ControlEvent::TouchUpInside
            } else {
                ControlEvent::TouchUpOutside
            },
            touch,
        );
        let flash = self.options.quick_tap_flash;
        if was_pending && inside {
            // Quicker than the delay: the highlight shows at the lift.
            self.set_highlighted(true);
            if flash > 0. {
                let this = self.this.clone();
                let handle = self.clock.set_timeout(
                    Box::new(move || {
                        if let Some(inner) = this.upgrade() {
                            inner.state.borrow_mut().flash_timer = None;
                            inner.set_highlighted(false);
                        }
                    }),
                    flash,
                );
                self.state.borrow_mut().flash_timer = Some(handle);
            } else {
                self.set_highlighted(false);
            }
        } else {
            self.set_highlighted(false);
        }
        self.finish();
    }

    fn touch_cancelled(&self, touch: &GestureTouch) {
        if self.tracked() != Some(touch.id()) {
            return;
        }
        self.state.borrow_mut().is_kept = false;
        self.clear_highlight_timer();
        self.emit(ControlEvent::TouchCancel, touch);
        self.set_highlighted(false);
        self.finish();
    }

    fn allows_ancestor_recognizer(&self, recognizer: &AnyRecognizer) -> bool {
        if recognizer.view().is_some() && recognizer.view() == self.options.view {
            return true;
        }
        match &self.options.vetoes_ancestor_recognizer {
            Some(vetoes) => !vetoes(recognizer),
            None => !vetoes_single_tap(recognizer),
        }
    }

    fn can_keep_touch(&self) -> bool {
        self.options.keeps_touch
    }

    fn keeps_touch(&self, touch_id: u64) -> bool {
        let state = self.state.borrow();
        self.options.keeps_touch && state.is_kept && state.touch_id == Some(touch_id)
    }

    fn selection_lock(&self) -> Option<SelectionLock> {
        self.options.selection_lock.clone()
    }

    fn holds_context_menu(&self, touch_id: u64) -> bool {
        self.options
            .holds_context_menu
            .as_ref()
            .is_some_and(|holds| holds(touch_id))
    }

    fn is_enabled(&self) -> bool {
        self.options
            .is_enabled
            .as_ref()
            .is_none_or(|enabled| enabled())
    }
}
