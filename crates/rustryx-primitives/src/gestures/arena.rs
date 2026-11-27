//! The arena that decides which recognizer gets a touch, upstream
//! `gestures/GestureArena.ts` (UIKit's gesture environment). Pure: no GPUI.
//! The host feeds it real events and carries out what it decides; tests
//! feed it synthetic streams with explicit timestamps.
//!
//! What it decides, the way UIKit decides it:
//!
//! - **Participation.** A new touch is offered to every recognizer on its
//!   hit path, innermost first; each one that accepts it tracks it until it
//!   ends.
//! - **Exclusivity.** Two recognizers that share a touch are not active at
//!   once unless both allow it (FR15). The first to recognize fails the
//!   other possible ones; one that tries to begin while a conflicting one is
//!   active fails instead.
//! - **Failure requirements.** A recognizer that requires another to fail
//!   waits (still possible) until that one fails, then recognizes; if that
//!   one recognizes, it fails. An idle recognizer holds nothing up.
//! - **Delivery to the view.** The touch's consumer gets began, moved and
//!   ended unless a recognizer delays them or recognizing cancels them.
//! - **The window.** A touch a recognized gesture owns is claimed: the host
//!   keeps the window's pan off it. A touch whose gesture cancelled the
//!   view's touches has its click swallowed; a click held for a recognizer
//!   that may still recognize is deferred, then replayed or dropped.
//!
//! **Time.** Every input carries its timestamp, and timers are deadlines on
//! that clock. Before an input is processed, every timer due by its
//! timestamp runs first, at its own deadline.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::{Rc, Weak};

use super::recognizer::{AnyRecognizer, PendingRecognition};
use super::types::{
    DefaultEnvironment, GestureEnvironment, GestureEventContext, GestureState, GestureTouch,
    TouchBeganInput, TouchConsumer, TouchInput, TouchPhase, TouchSample, TouchSeed,
};

/// A handle to a timer a [`GestureClock`] scheduled.
pub type TimerHandle = u64;

/// The clock and timers the arena runs on (upstream `GestureClock`).
pub trait GestureClock {
    /// The current time, ms.
    fn now(&self) -> f64;
    /// Runs `callback` after `ms`.
    fn set_timeout(&self, callback: Box<dyn FnOnce()>, ms: f64) -> TimerHandle;
    fn clear_timeout(&self, handle: TimerHandle);
    /// Runs `callback` once the current work is done (`queueMicrotask`).
    fn queue_microtask(&self, callback: Box<dyn FnOnce()>) {
        self.set_timeout(callback, 0.);
    }
}

type ClockTask = (TimerHandle, f64, Box<dyn FnOnce()>);

/// A clock that moves only when told to: the harness's `ManualClock`. Its
/// timers run only on [`ManualClock::run_due`] (and microtasks on
/// [`ManualClock::run_microtasks`]).
#[derive(Default)]
pub struct ManualClock {
    time: Cell<f64>,
    next: Cell<TimerHandle>,
    pending: RefCell<Vec<ClockTask>>,
    microtasks: RefCell<Vec<Box<dyn FnOnce()>>>,
}

impl ManualClock {
    /// A clock at 0.
    pub fn new() -> Rc<Self> {
        Rc::new(Self::default())
    }

    /// Sets the time without running anything.
    pub fn set_time(&self, time: f64) {
        self.time.set(time);
    }

    /// Moves to `time`, running every timer due by then, earliest first.
    pub fn run_due(&self, time: f64) {
        loop {
            let due = {
                let pending = self.pending.borrow();
                let mut best: Option<(usize, f64)> = None;
                for (index, (_, deadline, _)) in pending.iter().enumerate() {
                    if *deadline <= time && best.is_none_or(|(_, b)| *deadline < b) {
                        best = Some((index, *deadline));
                    }
                }
                best
            };
            let Some((index, deadline)) = due else {
                break;
            };
            let (_, _, callback) = self.pending.borrow_mut().remove(index);
            self.time.set(self.time.get().max(deadline));
            callback();
        }
        self.time.set(time);
    }

    /// Advances by `ms`, running every timer due on the way.
    pub fn advance_by(&self, ms: f64) {
        self.run_due(self.time.get() + ms);
    }

    /// Runs the queued microtasks.
    pub fn run_microtasks(&self) {
        loop {
            let tasks: Vec<_> = self.microtasks.borrow_mut().drain(..).collect();
            if tasks.is_empty() {
                break;
            }
            for task in tasks {
                task();
            }
        }
    }

    /// How many timers wait.
    pub fn pending_timers(&self) -> usize {
        self.pending.borrow().len()
    }
}

impl GestureClock for ManualClock {
    fn now(&self) -> f64 {
        self.time.get()
    }
    fn set_timeout(&self, callback: Box<dyn FnOnce()>, ms: f64) -> TimerHandle {
        let handle = self.next.get() + 1;
        self.next.set(handle);
        self.pending
            .borrow_mut()
            .push((handle, self.time.get() + ms, callback));
        handle
    }
    fn clear_timeout(&self, handle: TimerHandle) {
        self.pending.borrow_mut().retain(|(h, _, _)| *h != handle);
    }
    fn queue_microtask(&self, callback: Box<dyn FnOnce()>) {
        self.microtasks.borrow_mut().push(callback);
    }
}

/// What the click after a touch should do: pass, be swallowed, or be
/// swallowed now and replayed if the recognizers holding it all fail.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClickDisposition {
    Allow,
    Suppress,
    Defer,
}

/// A new contact with the recognizers on its hit path, innermost first, and
/// the innermost view-level consumer under it.
#[derive(Clone)]
pub struct ArenaBeganInput {
    pub input: TouchBeganInput,
    pub recognizers: Vec<AnyRecognizer>,
    pub consumer: Option<Rc<dyn TouchConsumer>>,
}

type ClaimCallback = Rc<dyn Fn(&[u64], &AnyRecognizer)>;
type ReleaseCallback = Rc<dyn Fn(&[u64])>;
type DeferredClickCallback = Rc<dyn Fn(&GestureTouch, bool)>;
type TouchPredicate = Rc<dyn Fn(&GestureTouch) -> bool>;
type TransitionCallback = Rc<dyn Fn(&AnyRecognizer, GestureState, GestureState)>;

/// Upstream `GestureArenaOptions`' callbacks; settable after the arena
/// exists ([`GestureArena::callbacks`]).
#[derive(Clone, Default)]
pub struct ArenaCallbacks {
    /// Touches a recognizer now owns, and the recognizer that claimed them.
    pub on_claim: Option<ClaimCallback>,
    /// Claimed touches given back to the window.
    pub on_release: Option<ReleaseCallback>,
    /// A click deferred at touch end resolved: replay it, or drop it.
    pub on_deferred_click: Option<DeferredClickCallback>,
    /// The arena dropped its last touch.
    pub on_idle: Option<Rc<dyn Fn()>>,
    /// Whether the window keeps a contact for itself while it is down (a
    /// drag on selectable text).
    pub refuses_claim: Option<TouchPredicate>,
    /// Every recognizer transition, for debugging and tests.
    pub on_transition: Option<TransitionCallback>,
}

/// Upstream `GestureArenaOptions`.
#[derive(Clone, Default)]
pub struct ArenaOptions {
    pub clock: Option<Rc<dyn GestureClock>>,
    pub environment: Option<Rc<dyn GestureEnvironment>>,
    pub callbacks: ArenaCallbacks,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ViewDelivery {
    None,
    Held,
    Delivered,
    EndHeld,
    Done,
}

struct TouchRecord {
    touch: GestureTouch,
    participants: Vec<AnyRecognizer>,
    consumer: Option<Rc<dyn TouchConsumer>>,
    view: ViewDelivery,
    is_lifted: bool,
    is_claimed: bool,
    suppress_click: bool,
    deferred_click: bool,
}

struct Timer {
    recognizer: AnyRecognizer,
    deadline: f64,
    tag: u32,
}

pub(crate) struct ArenaInner {
    environment: Rc<dyn GestureEnvironment>,
    clock: Rc<dyn GestureClock>,
    callbacks: RefCell<ArenaCallbacks>,
    moving_ids: RefCell<Vec<u64>>,
    records: RefCell<Vec<TouchRecord>>,
    active: RefCell<Vec<AnyRecognizer>>,
    timers: RefCell<Vec<Timer>>,
    real_timer: Cell<Option<TimerHandle>>,
    real_timer_deadline: Cell<f64>,
    current_time: Cell<Option<f64>>,
    latest_time: Cell<f64>,
    depth: Cell<u32>,
    defer_depth: Cell<u32>,
    owes_resolution: Cell<bool>,
}

/// The arena (upstream `GestureArena`). Cloning shares it.
#[derive(Clone)]
pub struct GestureArena(Rc<ArenaInner>);

/// A weak handle to an arena.
#[derive(Clone)]
pub struct WeakArena(Weak<ArenaInner>);

impl WeakArena {
    pub fn upgrade(&self) -> Option<GestureArena> {
        self.0.upgrade().map(GestureArena)
    }
}

impl Default for GestureArena {
    fn default() -> Self {
        Self::new(ArenaOptions::default())
    }
}

impl GestureArena {
    /// An arena; with no clock it runs on a fresh [`ManualClock`].
    pub fn new(options: ArenaOptions) -> Self {
        let clock = options
            .clock
            .unwrap_or_else(|| ManualClock::new() as Rc<dyn GestureClock>);
        Self(Rc::new(ArenaInner {
            environment: options
                .environment
                .unwrap_or_else(|| Rc::new(DefaultEnvironment)),
            clock,
            callbacks: RefCell::new(options.callbacks),
            moving_ids: RefCell::new(Vec::new()),
            records: RefCell::new(Vec::new()),
            active: RefCell::new(Vec::new()),
            timers: RefCell::new(Vec::new()),
            real_timer: Cell::new(None),
            real_timer_deadline: Cell::new(f64::INFINITY),
            current_time: Cell::new(None),
            latest_time: Cell::new(f64::NEG_INFINITY),
            depth: Cell::new(0),
            defer_depth: Cell::new(0),
            owes_resolution: Cell::new(false),
        }))
    }

    pub(crate) fn from_inner(inner: Rc<ArenaInner>) -> Self {
        Self(inner)
    }

    pub(crate) fn downgrade_inner(&self) -> Weak<ArenaInner> {
        Rc::downgrade(&self.0)
    }

    /// A weak handle.
    pub fn downgrade(&self) -> WeakArena {
        WeakArena(Rc::downgrade(&self.0))
    }

    /// The callbacks, to set after the arena exists.
    pub fn callbacks(&self) -> std::cell::RefMut<'_, ArenaCallbacks> {
        self.0.callbacks.borrow_mut()
    }

    fn callback<T>(&self, pick: impl FnOnce(&ArenaCallbacks) -> Option<T>) -> Option<T> {
        pick(&self.0.callbacks.borrow())
    }

    /// The environment recognizers read.
    pub fn environment(&self) -> Rc<dyn GestureEnvironment> {
        self.0.environment.clone()
    }

    /// The clock the arena runs on.
    pub fn clock(&self) -> Rc<dyn GestureClock> {
        self.0.clock.clone()
    }

    // --- Queries -----------------------------------------------------------

    /// The current time on the event clock.
    pub fn now(&self) -> f64 {
        self.0
            .current_time
            .get()
            .unwrap_or_else(|| self.0.latest_time.get().max(self.0.clock.now()))
    }

    /// Whether a recognizer owns this touch, so the window must not pan it.
    pub fn is_claimed(&self, id: u64) -> bool {
        self.0
            .records
            .borrow()
            .iter()
            .any(|r| r.touch.id() == id && r.is_claimed)
    }

    /// The view-level consumer of this touch, if any.
    pub fn consumer_of(&self, id: u64) -> Option<Rc<dyn TouchConsumer>> {
        self.record(id, |r| r.consumer.clone()).flatten()
    }

    /// The live touch with this id.
    pub fn get_touch(&self, id: u64) -> Option<GestureTouch> {
        self.record(id, |r| r.touch.clone())
    }

    /// The recognizers that took touch `id` when it began.
    pub fn participants_of(&self, id: u64) -> Vec<AnyRecognizer> {
        self.record(id, |r| r.participants.clone())
            .unwrap_or_default()
    }

    /// Every touch the arena tracks, lifted ones awaiting a decision included.
    pub fn touches(&self) -> Vec<GestureTouch> {
        self.0
            .records
            .borrow()
            .iter()
            .map(|r| r.touch.clone())
            .collect()
    }

    /// Recognizers taking part in the current interaction, in resolution order.
    pub fn involved_recognizers(&self) -> Vec<AnyRecognizer> {
        self.0.active.borrow().clone()
    }

    fn record<R>(&self, id: u64, f: impl FnOnce(&TouchRecord) -> R) -> Option<R> {
        self.0
            .records
            .borrow()
            .iter()
            .find(|r| r.touch.id() == id)
            .map(f)
    }

    fn record_mut<R>(&self, id: u64, f: impl FnOnce(&mut TouchRecord) -> R) -> Option<R> {
        self.0
            .records
            .borrow_mut()
            .iter_mut()
            .find(|r| r.touch.id() == id)
            .map(f)
    }

    fn active(&self) -> Vec<AnyRecognizer> {
        self.0.active.borrow().clone()
    }

    // --- Input ---------------------------------------------------------------

    /// New contacts, each with the recognizers on its hit path.
    pub fn began(&self, inputs: Vec<ArenaBeganInput>) {
        let Some(first) = inputs.first() else {
            return;
        };
        let time = first.input.time;
        self.run(time, || {
            let mut new_touches: Vec<(AnyRecognizer, Vec<GestureTouch>)> = Vec::new();
            let mut created: Vec<GestureTouch> = Vec::new();
            for began in &inputs {
                let input = &began.input;
                if self.record(input.id, |_| ()).is_some() {
                    continue;
                }
                let time = self.clamp_time(input.time);
                let mut samples = input.history.clone();
                samples.push(TouchSample {
                    x: input.x,
                    y: input.y,
                    time,
                });
                let touch = GestureTouch::new(TouchSeed {
                    id: input.id,
                    pointer_type: input.pointer_type,
                    x: input.x,
                    y: input.y,
                    time,
                    samples,
                    target: input.target,
                    scroll: input.scroll.clone(),
                    scroll_type: input.scroll_type,
                    force: input.force,
                });
                let mut participants = Vec::new();
                for (depth, recognizer) in began.recognizers.iter().enumerate() {
                    if !recognizer.wants_touch(&touch) {
                        continue;
                    }
                    participants.push(recognizer.clone());
                    recognizer.set_arena(self);
                    {
                        let mut active = self.0.active.borrow_mut();
                        if !active.contains(recognizer) {
                            // Resolution follows `active`: a recognizer joins
                            // ahead of any further out on this touch's path,
                            // so the innermost wins a tie (FR15).
                            let outer = active.iter().position(|other| {
                                began
                                    .recognizers
                                    .iter()
                                    .position(|r| r == other)
                                    .is_some_and(|index| index > depth)
                            });
                            match outer {
                                Some(index) => active.insert(index, recognizer.clone()),
                                None => active.push(recognizer.clone()),
                            }
                        }
                    }
                    match new_touches.iter_mut().find(|(r, _)| r == recognizer) {
                        Some((_, list)) => list.push(touch.clone()),
                        None => new_touches.push((recognizer.clone(), vec![touch.clone()])),
                    }
                }
                self.0.records.borrow_mut().push(TouchRecord {
                    touch: touch.clone(),
                    participants,
                    consumer: began.consumer.clone(),
                    view: ViewDelivery::None,
                    is_lifted: false,
                    is_claimed: false,
                    suppress_click: false,
                    deferred_click: false,
                });
                created.push(touch);
            }
            let context = self.context();
            for recognizer in self.active() {
                if let Some((_, touches)) = new_touches.iter().find(|(r, _)| *r == recognizer) {
                    recognizer.deliver_began(touches, &context);
                }
            }
            // A recognizer already tracking a gesture hears of a touch that
            // began off its hit path.
            for recognizer in self.active() {
                let own = new_touches.iter().find(|(r, _)| *r == recognizer);
                let elsewhere: Vec<GestureTouch> = created
                    .iter()
                    .filter(|touch| own.is_none_or(|(_, own)| !own.contains(touch)))
                    .cloned()
                    .collect();
                if !elsewhere.is_empty() && recognizer.number_of_touches() > 0 {
                    recognizer.deliver_began_elsewhere(&elsewhere, &context);
                }
            }
            for touch in &created {
                self.deliver_view_began(touch.id());
            }
        });
    }

    /// A contact began where nothing takes part. It is never tracked and
    /// stays the window's; recognizers tracking a gesture hear of it.
    pub fn untracked_began(&self, input: TouchBeganInput) {
        if !self.active().iter().any(|r| r.number_of_touches() > 0) {
            return;
        }
        self.run(input.time, || {
            let time = self.clamp_time(input.time);
            let touch = GestureTouch::new(TouchSeed {
                id: input.id,
                pointer_type: input.pointer_type,
                x: input.x,
                y: input.y,
                time,
                samples: vec![TouchSample {
                    x: input.x,
                    y: input.y,
                    time,
                }],
                target: input.target,
                scroll: input.scroll.clone(),
                scroll_type: None,
                force: input.force,
            });
            let context = self.context();
            for recognizer in self.active() {
                if recognizer.number_of_touches() > 0 {
                    recognizer.deliver_began_elsewhere(std::slice::from_ref(&touch), &context);
                }
            }
        });
    }

    /// Contacts moved.
    pub fn moved(&self, inputs: Vec<TouchInput>) {
        let Some(first) = inputs.first() else {
            return;
        };
        let time = first.time;
        self.run(time, || {
            let changed = self.update(&inputs, TouchPhase::Moved);
            if changed.is_empty() {
                return;
            }
            let context = self.context();
            let previous = self
                .0
                .moving_ids
                .replace(changed.iter().map(GestureTouch::id).collect());
            for recognizer in self.active() {
                recognizer.deliver_moved(&changed, &context);
            }
            *self.0.moving_ids.borrow_mut() = previous;
            for touch in &changed {
                let consumer = self
                    .record(touch.id(), |r| {
                        (r.view == ViewDelivery::Delivered).then(|| r.consumer.clone())
                    })
                    .flatten()
                    .flatten();
                if let Some(consumer) = consumer {
                    consumer.touch_moved(touch);
                }
            }
        });
    }

    /// Contacts lifted. Returns, per touch, what its click should do.
    pub fn ended(&self, inputs: Vec<TouchInput>) -> HashMap<u64, ClickDisposition> {
        let mut dispositions = HashMap::new();
        let Some(first) = inputs.first() else {
            return dispositions;
        };
        let time = first.time;
        self.run(time, || {
            let changed = self.update(&inputs, TouchPhase::Ended);
            let context = self.context();
            for recognizer in self.active() {
                recognizer.deliver_ended(&changed, &context);
            }
            // A lift can decide a waiting recognition: settle those first,
            // so each click's answer is the final one (FR21).
            self.review_pending();
            for touch in &changed {
                if self
                    .record_mut(touch.id(), |r| r.is_lifted = true)
                    .is_none()
                {
                    continue;
                }
                dispositions.insert(touch.id(), self.resolve_end(touch.id()));
            }
        });
        dispositions
    }

    /// The system took these contacts.
    pub fn cancelled(&self, ids: &[u64], time: f64) {
        self.run(time, || {
            let touches = self.live_touches(ids);
            for touch in &touches {
                touch.live_mut().phase = TouchPhase::Cancelled;
            }
            let context = self.context();
            for recognizer in self.active() {
                recognizer.deliver_cancelled(&touches, &context);
            }
            for touch in &touches {
                let known = self.record_mut(touch.id(), |r| {
                    r.is_lifted = true;
                    r.suppress_click = true;
                });
                if known.is_some() {
                    self.cancel_view(touch.id());
                }
            }
        });
    }

    /// The window took these contacts for its own pan or zoom. Possible
    /// recognizers sharing them fail and active ones are cancelled, except a
    /// recognizer that keeps observing; a drag the window took stays the
    /// window's (FR14).
    pub fn browser_took_over(&self, ids: &[u64], time: f64) {
        self.run(time, || {
            let touches: Vec<GestureTouch> = self
                .live_touches(ids)
                .into_iter()
                .filter(|touch| !touch.taken_by_browser())
                .collect();
            if touches.is_empty() {
                return;
            }
            for touch in &touches {
                touch.live_mut().taken_by_browser = true;
            }
            let context = self.context();
            for recognizer in self.active() {
                recognizer.deliver_taken_by_browser(&touches, &context);
            }
            for touch in &touches {
                if self
                    .record_mut(touch.id(), |r| r.suppress_click = true)
                    .is_some()
                {
                    self.cancel_view(touch.id());
                }
            }
        });
    }

    /// Runs every timer due at or before `time`.
    pub fn advance_time(&self, time: f64) {
        self.run(time, || {});
    }

    // --- Recognizer requests -------------------------------------------------

    /// A recognizer asked for a state change.
    pub(crate) fn request_transition(&self, recognizer: &AnyRecognizer, next: GestureState) {
        let from = recognizer.state();
        if next == GestureState::Failed {
            self.fail(recognizer);
            return;
        }
        if recognizer.pending().is_some()
            && matches!(next, GestureState::Began | GestureState::Ended)
        {
            return;
        }
        if from == GestureState::Possible
            && matches!(next, GestureState::Began | GestureState::Ended)
        {
            let pending = if next == GestureState::Began {
                PendingRecognition::Began
            } else {
                PendingRecognition::Ended
            };
            self.attempt_recognition(recognizer, pending);
            return;
        }
        if from.is_active() && matches!(next, GestureState::Changed | GestureState::Ended) {
            self.apply(recognizer, next);
            return;
        }
        if from.is_active() && next == GestureState::Cancelled {
            self.apply(recognizer, GestureState::Cancelled);
        }
    }

    /// Arms a recognizer's one timer at an absolute deadline.
    pub(crate) fn set_timer(&self, recognizer: &AnyRecognizer, deadline: f64, tag: u32) {
        {
            let mut timers = self.0.timers.borrow_mut();
            match timers.iter_mut().find(|t| t.recognizer == *recognizer) {
                Some(timer) => {
                    timer.deadline = deadline;
                    timer.tag = tag;
                }
                None => timers.push(Timer {
                    recognizer: recognizer.clone(),
                    deadline,
                    tag,
                }),
            }
        }
        self.schedule_real_timer();
    }

    pub(crate) fn clear_timer(&self, recognizer: &AnyRecognizer) {
        let removed = {
            let mut timers = self.0.timers.borrow_mut();
            let before = timers.len();
            timers.retain(|t| t.recognizer != *recognizer);
            timers.len() != before
        };
        if removed {
            self.schedule_real_timer();
        }
    }

    /// A recognizer takes its touches from the view, unclaimed.
    pub(crate) fn cancel_touches_in_view_of(&self, recognizer: &AnyRecognizer) {
        self.cancel_view_touches_of(recognizer);
    }

    /// Runs `body`, code outside input that stops recognizers, without
    /// resolving what those stops unblock: a pass a microtask later resolves
    /// it, at the last event's time (react-component-runtime INV2).
    pub fn defer_resolution(&self, body: impl FnOnce()) {
        self.0.defer_depth.set(self.0.defer_depth.get() + 1);
        body();
        self.0.defer_depth.set(self.0.defer_depth.get() - 1);
        if self.0.defer_depth.get() == 0 && self.0.owes_resolution.get() {
            let weak = self.downgrade();
            self.0.clock.queue_microtask(Box::new(move || {
                if let Some(arena) = weak.upgrade()
                    && arena.0.owes_resolution.get()
                    && arena.0.defer_depth.get() == 0
                {
                    arena.run(arena.last_event_time(), || {});
                }
            }));
        }
    }

    fn last_event_time(&self) -> f64 {
        let latest = self.0.latest_time.get();
        if latest.is_finite() {
            latest
        } else {
            self.0.clock.now()
        }
    }

    /// Cancels an active recognizer, or fails a possible one.
    pub fn stop(&self, recognizer: &AnyRecognizer) {
        // A cancel from code between events happens at the last event's
        // time: wall time would run deadlines ahead of input still queued.
        let time = self
            .0
            .current_time
            .get()
            .unwrap_or_else(|| self.last_event_time());
        self.run(time, || {
            if recognizer.state().is_active() {
                self.apply(recognizer, GestureState::Cancelled);
            } else if recognizer.state() == GestureState::Possible && recognizer.is_involved() {
                self.fail(recognizer);
            }
        });
    }

    // --- Resolution ----------------------------------------------------------

    fn attempt_recognition(&self, recognizer: &AnyRecognizer, next: PendingRecognition) {
        if !recognizer.should_begin()
            || self.is_vetoed_by_consumer(recognizer)
            || self.is_refused_by_browser(recognizer)
        {
            self.fail(recognizer);
            return;
        }
        let mut is_waiting = false;
        for other in self.active() {
            if other == *recognizer || !recognizer.requires_failure_of(&other) {
                continue;
            }
            if other.state() == GestureState::Failed {
                continue;
            }
            if other.state() != GestureState::Possible {
                self.fail(recognizer);
                return;
            }
            if other.is_involved() {
                is_waiting = true;
            }
        }
        if is_waiting {
            recognizer.set_pending(Some(next));
            self.claim(recognizer);
            return;
        }
        for other in self.active() {
            if other != *recognizer
                && (other.state().is_active()
                    || (other.state() == GestureState::Ended && other.number_of_touches() > 0))
                && self.conflicts(&other, recognizer)
            {
                self.fail(recognizer);
                return;
            }
        }
        let state = match next {
            PendingRecognition::Began => GestureState::Began,
            PendingRecognition::Ended => GestureState::Ended,
        };
        self.apply(recognizer, state);
        for other in self.active() {
            if other == *recognizer || other.state() != GestureState::Possible {
                continue;
            }
            if other.requires_failure_of(recognizer) || self.conflicts(recognizer, &other) {
                self.fail(&other);
            }
        }
        self.claim(recognizer);
        // A watcher takes nothing from the view either (FR11, FR19).
        if recognizer.claims_touches() && recognizer.cancels_touches_in_view() {
            self.cancel_view_touches_of(recognizer);
        }
    }

    /// Whether `winner` being recognized prevents `other`.
    fn conflicts(&self, winner: &AnyRecognizer, other: &AnyRecognizer) -> bool {
        if !shares_touches(winner, other)
            || winner.allows_simultaneous(other)
            || !winner.claims_touches()
            || !other.claims_touches()
        {
            return false;
        }
        winner.can_prevent(other) && other.can_be_prevented(winner)
    }

    fn is_refused_by_browser(&self, recognizer: &AnyRecognizer) -> bool {
        let Some(refuses) = self.callback(|c| c.refuses_claim.clone()) else {
            return false;
        };
        if !recognizer.claims_touches() {
            return false;
        }
        recognizer.tracked_touches().iter().any(|touch| {
            !matches!(touch.phase(), TouchPhase::Ended | TouchPhase::Cancelled) && refuses(touch)
        })
    }

    /// `UIControl`'s rule: a control may veto an overlapping ancestor recognizer.
    fn is_vetoed_by_consumer(&self, recognizer: &AnyRecognizer) -> bool {
        for id in recognizer.seen_touch_ids() {
            if let Some(consumer) = self.consumer_of(id)
                && !consumer.allows_ancestor_recognizer(recognizer)
            {
                return true;
            }
        }
        false
    }

    fn fail(&self, recognizer: &AnyRecognizer) {
        let state = recognizer.state();
        if state.is_terminal() {
            return;
        }
        if state.is_active() {
            self.apply(recognizer, GestureState::Cancelled);
            return;
        }
        recognizer.set_pending(None);
        self.apply(recognizer, GestureState::Failed);
        self.review_pending();
    }

    /// A recognition that claimed its touches and then fails or is cancelled
    /// gives them back unless another recognition owns them. A gesture that
    /// cancels itself on a finger's move keeps that finger until it lifts
    /// (FR14).
    fn release_claim(&self, recognizer: &AnyRecognizer, keep: &[u64]) {
        let mut released = Vec::new();
        let active = self.active();
        for touch in recognizer.tracked_touches() {
            let claimed = self.record(touch.id(), |r| r.is_claimed).unwrap_or(false);
            if !claimed || keep.contains(&touch.id()) {
                continue;
            }
            let is_owned = active.iter().any(|other| {
                other != recognizer
                    && other.claims_touches()
                    && (other.state().is_active()
                        || other.state() == GestureState::Ended
                        || other.pending().is_some())
                    && other.tracks(touch.id())
            });
            if !is_owned {
                self.record_mut(touch.id(), |r| r.is_claimed = false);
                released.push(touch.id());
            }
        }
        if !released.is_empty()
            && let Some(on_release) = self.callback(|c| c.on_release.clone())
        {
            on_release(&released);
        }
    }

    fn apply(&self, recognizer: &AnyRecognizer, next: GestureState) {
        let from = recognizer.state();
        recognizer.apply_state(next);
        let transitioned = (recognizer.state() == next && from != next)
            || (next == GestureState::Changed && from == GestureState::Changed);
        if transitioned && let Some(on_transition) = self.callback(|c| c.on_transition.clone()) {
            on_transition(recognizer, from, next);
        }
        if next.is_terminal() {
            self.clear_timer(recognizer);
        }
        if next == GestureState::Cancelled {
            let moving = self.0.moving_ids.borrow().clone();
            self.release_claim(recognizer, &moving);
        } else if next == GestureState::Failed {
            self.release_claim(recognizer, &[]);
        }
    }

    /// Re-runs every waiting recognition whose requirements may have
    /// resolved, until nothing changes.
    fn review_pending(&self) {
        if self.0.defer_depth.get() > 0 {
            self.0.owes_resolution.set(true);
            return;
        }
        let mut changed = true;
        while changed {
            changed = false;
            for recognizer in self.active() {
                let Some(next) = recognizer.pending() else {
                    continue;
                };
                if recognizer.state() != GestureState::Possible {
                    continue;
                }
                let blocked = self.active().iter().any(|other| {
                    other != &recognizer
                        && recognizer.requires_failure_of(other)
                        && other.state() == GestureState::Possible
                        && other.is_involved()
                });
                if blocked {
                    continue;
                }
                recognizer.set_pending(None);
                self.attempt_recognition(&recognizer, next);
                changed = true;
            }
        }
    }

    fn claim(&self, recognizer: &AnyRecognizer) {
        if !recognizer.claims_touches() {
            return;
        }
        let mut claimed = Vec::new();
        for touch in recognizer.tracked_touches() {
            let took = self
                .record_mut(touch.id(), |r| {
                    if !r.is_lifted && !r.is_claimed && !r.touch.taken_by_browser() {
                        r.is_claimed = true;
                        true
                    } else {
                        false
                    }
                })
                .unwrap_or(false);
            if took {
                claimed.push(touch.id());
            }
        }
        if !claimed.is_empty()
            && let Some(on_claim) = self.callback(|c| c.on_claim.clone())
        {
            on_claim(&claimed, recognizer);
        }
    }

    // --- View delivery -------------------------------------------------------

    fn deliver_view_began(&self, id: u64) {
        let Some((consumer, view, suppress, touch)) = self.record(id, |r| {
            (
                r.consumer.clone(),
                r.view,
                r.suppress_click,
                r.touch.clone(),
            )
        }) else {
            return;
        };
        let Some(consumer) = consumer else {
            return;
        };
        if view != ViewDelivery::None {
            return;
        }
        if suppress {
            self.record_mut(id, |r| r.view = ViewDelivery::Done);
            return;
        }
        if self.begin_delayers(id) {
            self.record_mut(id, |r| r.view = ViewDelivery::Held);
            return;
        }
        self.record_mut(id, |r| r.view = ViewDelivery::Delivered);
        consumer.touch_began(&touch);
    }

    /// What a lifted touch's view and click get, decided at the lift.
    fn resolve_end(&self, id: u64) -> ClickDisposition {
        let Some((suppress, view, consumer, touch)) = self.record(id, |r| {
            (
                r.suppress_click,
                r.view,
                r.consumer.clone(),
                r.touch.clone(),
            )
        }) else {
            return ClickDisposition::Allow;
        };
        if suppress {
            return ClickDisposition::Suppress;
        }
        let is_held = self.end_delayers(id);
        match view {
            ViewDelivery::Delivered => {
                if is_held {
                    self.record_mut(id, |r| {
                        r.view = ViewDelivery::EndHeld;
                        r.deferred_click = true;
                    });
                    return ClickDisposition::Defer;
                }
                self.record_mut(id, |r| r.view = ViewDelivery::Done);
                if let Some(consumer) = consumer {
                    consumer.touch_ended(&touch);
                }
                ClickDisposition::Allow
            }
            ViewDelivery::Held => {
                if !self.begin_delayers(id) {
                    self.record_mut(id, |r| r.view = ViewDelivery::Delivered);
                    if let Some(consumer) = consumer {
                        consumer.touch_began(&touch);
                    }
                    return self.resolve_end(id);
                }
                self.record_mut(id, |r| r.deferred_click = true);
                ClickDisposition::Defer
            }
            ViewDelivery::None | ViewDelivery::EndHeld | ViewDelivery::Done => {
                if is_held {
                    self.record_mut(id, |r| r.deferred_click = true);
                    return ClickDisposition::Defer;
                }
                ClickDisposition::Allow
            }
        }
    }

    /// Releases whatever no recognizer holds any more.
    fn review_held(&self) {
        let ids: Vec<u64> = self
            .0
            .records
            .borrow()
            .iter()
            .map(|r| r.touch.id())
            .collect();
        for id in ids {
            let Some((view, suppress, lifted, consumer, touch)) = self.record(id, |r| {
                (
                    r.view,
                    r.suppress_click,
                    r.is_lifted,
                    r.consumer.clone(),
                    r.touch.clone(),
                )
            }) else {
                continue;
            };
            if view == ViewDelivery::Held && !self.begin_delayers(id) {
                if suppress {
                    self.record_mut(id, |r| r.view = ViewDelivery::Done);
                } else {
                    self.record_mut(id, |r| r.view = ViewDelivery::Delivered);
                    if let Some(consumer) = &consumer {
                        consumer.touch_began(&touch);
                    }
                    if lifted && !self.end_delayers(id) {
                        self.record_mut(id, |r| r.view = ViewDelivery::Done);
                        if let Some(consumer) = &consumer {
                            consumer.touch_ended(&touch);
                        }
                    } else if lifted {
                        self.record_mut(id, |r| r.view = ViewDelivery::EndHeld);
                    }
                }
            }
            let view = self.record(id, |r| r.view).unwrap_or(ViewDelivery::Done);
            if view == ViewDelivery::EndHeld && !self.end_delayers(id) {
                let suppress = self.record_mut(id, |r| {
                    r.view = ViewDelivery::Done;
                    r.suppress_click
                });
                if let Some(consumer) = &consumer {
                    if suppress == Some(true) {
                        consumer.touch_cancelled(&touch);
                    } else {
                        consumer.touch_ended(&touch);
                    }
                }
            }
            let Some((deferred, view, suppress)) =
                self.record(id, |r| (r.deferred_click, r.view, r.suppress_click))
            else {
                continue;
            };
            if deferred
                && view != ViewDelivery::Held
                && view != ViewDelivery::EndHeld
                && !self.end_delayers(id)
            {
                self.record_mut(id, |r| r.deferred_click = false);
                if let Some(on_deferred) = self.callback(|c| c.on_deferred_click.clone()) {
                    on_deferred(&touch, !suppress);
                }
            }
        }
    }

    fn begin_delayers(&self, id: u64) -> bool {
        self.participants_of(id).iter().any(|r| {
            r.delays_touches_began()
                && r.state() == GestureState::Possible
                && r.is_involved()
                && r.has_seen(id)
        })
    }

    fn end_delayers(&self, id: u64) -> bool {
        self.participants_of(id).iter().any(|r| {
            r.delays_touches_ended()
                && r.state() == GestureState::Possible
                && r.is_involved()
                && r.has_seen(id)
        })
    }

    fn cancel_view_touches_of(&self, recognizer: &AnyRecognizer) {
        for id in recognizer.seen_touch_ids() {
            if self.record_mut(id, |r| r.suppress_click = true).is_some() {
                self.cancel_view(id);
            }
        }
    }

    fn cancel_view(&self, id: u64) {
        let Some((view, consumer, touch)) =
            self.record(id, |r| (r.view, r.consumer.clone(), r.touch.clone()))
        else {
            return;
        };
        if view != ViewDelivery::None || consumer.is_some() {
            self.record_mut(id, |r| r.view = ViewDelivery::Done);
        }
        if matches!(view, ViewDelivery::Delivered | ViewDelivery::EndHeld)
            && let Some(consumer) = consumer
        {
            consumer.touch_cancelled(&touch);
        }
    }

    // --- Plumbing --------------------------------------------------------------

    fn context(&self) -> GestureEventContext {
        GestureEventContext {
            time: self.now(),
            all_touches: self
                .0
                .records
                .borrow()
                .iter()
                .filter(|r| !r.is_lifted)
                .map(|r| r.touch.clone())
                .collect(),
        }
    }

    fn live_touches(&self, ids: &[u64]) -> Vec<GestureTouch> {
        ids.iter()
            .filter_map(|id| {
                self.record(*id, |r| (!r.is_lifted).then(|| r.touch.clone()))
                    .flatten()
            })
            .collect()
    }

    fn update(&self, inputs: &[TouchInput], phase: TouchPhase) -> Vec<GestureTouch> {
        let mut changed = Vec::new();
        for input in inputs {
            let Some(touch) = self
                .record(input.id, |r| (!r.is_lifted).then(|| r.touch.clone()))
                .flatten()
            else {
                continue;
            };
            let time = self.clamp_time(input.time);
            {
                let mut live = touch.live_mut();
                let previous = live.time;
                let mut samples: Vec<TouchSample> = input
                    .history
                    .iter()
                    .filter(|s| s.time > previous && s.time < time)
                    .copied()
                    .collect();
                samples.push(TouchSample {
                    x: input.x,
                    y: input.y,
                    time,
                });
                live.samples = samples;
                live.x = input.x;
                live.y = input.y;
                live.time = time;
                live.phase = phase;
                if input.force.is_some() {
                    live.force = input.force;
                }
            }
            changed.push(touch);
        }
        changed
    }

    fn clamp_time(&self, time: f64) -> f64 {
        let clamped = time.max(self.0.latest_time.get());
        self.0.latest_time.set(clamped);
        clamped
    }

    /// One external entry: run the timers due by `time`, then `body`, then
    /// settle. Re-entrant calls run inline and settle with the outermost.
    fn run(&self, time: f64, body: impl FnOnce()) {
        let inner = &self.0;
        inner.depth.set(inner.depth.get() + 1);
        if inner.depth.get() == 1 && inner.defer_depth.get() == 0 && inner.owes_resolution.get() {
            inner.owes_resolution.set(false);
            inner
                .current_time
                .set(Some(self.clamp_time(self.last_event_time())));
            self.settle();
        }
        self.flush_timers(time);
        inner.current_time.set(Some(self.clamp_time(time)));
        body();
        if inner.depth.get() == 1 {
            if inner.defer_depth.get() > 0 {
                inner.owes_resolution.set(true);
            } else {
                inner.owes_resolution.set(false);
                self.settle();
            }
        }
        inner.depth.set(inner.depth.get() - 1);
        if inner.depth.get() == 0 {
            inner.current_time.set(None);
        }
    }

    fn flush_timers(&self, up_to: f64) {
        loop {
            let due = {
                let timers = self.0.timers.borrow();
                let mut best: Option<usize> = None;
                for (index, timer) in timers.iter().enumerate() {
                    if timer.deadline > up_to {
                        continue;
                    }
                    let better = match best {
                        None => true,
                        Some(b) => {
                            let current = &timers[b];
                            timer.deadline < current.deadline
                                || (timer.deadline == current.deadline
                                    && self.rank(&timer.recognizer)
                                        < self.rank(&current.recognizer))
                        }
                    };
                    if better {
                        best = Some(index);
                    }
                }
                best
            };
            let Some(index) = due else {
                break;
            };
            let timer = self.0.timers.borrow_mut().remove(index);
            self.0
                .current_time
                .set(Some(self.clamp_time(timer.deadline)));
            timer.recognizer.fire_timer(timer.tag);
            if self.0.depth.get() <= 1 {
                self.settle();
            }
        }
        self.schedule_real_timer();
    }

    fn rank(&self, recognizer: &AnyRecognizer) -> usize {
        self.0
            .active
            .borrow()
            .iter()
            .position(|r| r == recognizer)
            .unwrap_or(usize::MAX)
    }

    fn settle(&self) {
        self.review_pending();
        self.review_held();
        let mut did_reset = false;
        for recognizer in self.active() {
            let state = recognizer.state();
            let is_finished = (state.is_terminal() && recognizer.number_of_touches() == 0)
                || (state == GestureState::Possible && !recognizer.is_involved());
            if !is_finished {
                continue;
            }
            self.0.active.borrow_mut().retain(|r| *r != recognizer);
            recognizer.reset_to_possible();
            did_reset = true;
            if state != GestureState::Possible
                && let Some(on_transition) = self.callback(|c| c.on_transition.clone())
            {
                on_transition(&recognizer, state, GestureState::Possible);
            }
        }
        if did_reset {
            self.review_held();
        }
        let dropped = {
            let mut records = self.0.records.borrow_mut();
            let before = records.len();
            records.retain(|r| {
                !(r.is_lifted
                    && r.view != ViewDelivery::Held
                    && r.view != ViewDelivery::EndHeld
                    && !r.deferred_click)
            });
            before != records.len() && records.is_empty()
        };
        if dropped && let Some(on_idle) = self.callback(|c| c.on_idle.clone()) {
            on_idle();
        }
    }

    fn schedule_real_timer(&self) {
        let earliest = self
            .0
            .timers
            .borrow()
            .iter()
            .map(|t| t.deadline)
            .fold(f64::INFINITY, f64::min);
        if earliest == self.0.real_timer_deadline.get() {
            return;
        }
        if let Some(handle) = self.0.real_timer.take() {
            self.0.clock.clear_timeout(handle);
        }
        self.0.real_timer_deadline.set(earliest);
        if earliest == f64::INFINITY {
            return;
        }
        let delay = (earliest - self.0.clock.now()).max(0.);
        let weak = self.downgrade();
        let handle = self
            .0
            .clock
            .set_timeout(Box::new(move || fire(weak, earliest, true)), delay);
        self.0.real_timer.set(Some(handle));
    }
}

fn fire(weak: WeakArena, earliest: f64, may_yield: bool) {
    let Some(arena) = weak.upgrade() else {
        return;
    };
    arena.0.real_timer.set(None);
    arena.0.real_timer_deadline.set(f64::INFINITY);
    if may_yield && arena.0.clock.now() > earliest {
        // Late: a long task held the main thread past the deadline, and
        // input queued behind it may come first. Let it dispatch, once.
        arena.0.real_timer_deadline.set(earliest);
        let again = arena.downgrade();
        let handle = arena
            .0
            .clock
            .set_timeout(Box::new(move || fire(again, earliest, false)), 0.);
        arena.0.real_timer.set(Some(handle));
        return;
    }
    arena.advance_time(earliest);
}

fn shares_touches(a: &AnyRecognizer, b: &AnyRecognizer) -> bool {
    let theirs = b.seen_touch_ids();
    a.seen_touch_ids().iter().any(|id| theirs.contains(id))
}
