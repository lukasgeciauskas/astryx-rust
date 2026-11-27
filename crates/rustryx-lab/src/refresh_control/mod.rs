//! Pull to refresh, as UIRefreshControl, for the app's own scroll container (upstream
//! lab `RefreshControl`: `useRefreshControl`, `refreshTouchController.ts` and
//! `RefreshPullRecognizer.ts`).
//!
//! Pulling the content down past the top reveals the app's indicator. Pulling past the
//! trigger starts a refresh with the finger still down. Letting go settles the content
//! onto the refreshing inset until the refresh ends, and then it slides back up.
//!
//! A finger pulls, and so does a trackpad's two-finger scroll at the top, with the same
//! mapping, trigger and settle. A mouse or pen drag stays the platform's, for text
//! selection, and a mouse wheel's notches never pull. Always give people another way to
//! refresh, such as a button that calls [`RefreshControl::refresh`]: a pull has no
//! keyboard or assistive-technology equivalent.
//!
//! GPUI has no element tree to walk for the scroll container, so the app hands its
//! container's [`ScrollHandle`] to [`RefreshControlOptions::scroll_handle`]: the pull
//! applies while that container overflows and rests at its top. Give the container
//! `overscroll_y_contain` so a pull at its top does not scroll the page behind it
//! (upstream claims the top edge with `overscroll-behavior-y: none` for the same
//! reason). There are no CSS custom properties either: the app's indicator reads how much
//! of it shows from [`RefreshControl::reveal`] (upstream's `--astryx-refresh-reveal`).

mod model;
mod pull;

pub mod examples;
#[cfg(test)]
mod tests;

use std::cell::RefCell;
use std::rc::Rc;

use gpui::prelude::*;
use gpui::{
    AnyElement, App, Bounds, DispatchPhase, ElementId, Entity, Pixels, ScrollEvent, ScrollHandle,
    SharedString, Task, TouchPhase, WeakEntity, Window, canvas, div, point, px,
};
use rustryx_components::doc::{ComponentDoc, PropMapping, RustApi};
use rustryx_primitives::announce::{Politeness, announce};
use rustryx_primitives::gestures::host::{
    GestureAttachment, after_gesture, gesture_area, gesture_host,
};
use rustryx_primitives::gestures::native_scroll::NativeScroller;
use rustryx_primitives::gestures::{
    GestureState, PanDirection, PanGestureRecognizer, PanOptions, PointerKind, RecognizerOptions,
    ScrollType,
};
use rustryx_primitives::motion::progress::map_progress;
use rustryx_primitives::motion::uikit::scroll::BOUNCE;
use rustryx_primitives::motion::{
    AnimateOptions, EngineOptions, FnCurve, MotionCurves, MotionEngine, MotionHandle,
    MotionProperty, MotionValues, SpringTransition, cubic_bezier,
};
use rustryx_primitives::{coarse_primary_pointer, prefers_reduced_motion};
use rustryx_theme::authoring::js::js_max;
use web_time::Instant;

pub use model::{
    BeginSource, INITIAL_REFRESH_STATE, RefreshEffect, RefreshEvent, RefreshState, SettleTarget,
    reduce_refresh,
};
pub use pull::{
    EndMotion, EndTiming, PullMode, REFRESH_HEIGHT, end_motion, end_timing, mode_after_trigger,
    pull_overscroll, pull_slope, pull_travel, release_target, reveal_fraction, reveal_progress,
    trigger_distance, values,
};

/// Upstream `onRefresh`: called when a pull triggers, or when `refresh()` runs. Return a
/// task to keep the control refreshing until it completes (uncontrolled use); `None`
/// ends the refresh right after (upstream's non-promise result).
pub type RefreshHandler = Rc<dyn Fn(&mut Window, &mut App) -> Option<Task<()>>>;

/// Upstream `UseRefreshControlOptions`.
#[derive(Clone, Default)]
pub struct RefreshControlOptions {
    on_refresh: Option<RefreshHandler>,
    is_refreshing: Option<bool>,
    is_disabled: bool,
    announce: Option<SharedString>,
    scroll: Option<ScrollHandle>,
}

impl RefreshControlOptions {
    /// No handler, uncontrolled, enabled, announcing nothing, with no scroll container.
    pub fn new() -> Self {
        Self::default()
    }

    /// Upstream `onRefresh`.
    pub fn on_refresh(
        mut self,
        handler: impl Fn(&mut Window, &mut App) -> Option<Task<()>> + 'static,
    ) -> Self {
        self.on_refresh = Some(Rc::new(handler));
        self
    }

    /// Upstream `isRefreshing`: controlled refreshing state. `true` puts the control in
    /// the refreshing state (as `beginRefreshing()`), `false` ends it (as
    /// `endRefreshing()`). Leave it unset to let the task `on_refresh` returns decide.
    pub fn is_refreshing(mut self, refreshing: bool) -> Self {
        self.is_refreshing = Some(refreshing);
        self
    }

    /// Upstream `isDisabled`: turns the pull off. A refresh in progress keeps running.
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.is_disabled = disabled;
        self
    }

    /// Upstream `announce`: announced politely when a refresh starts, for example
    /// "Refreshing", in the app's own language.
    pub fn announce(mut self, text: impl Into<SharedString>) -> Self {
        self.announce = Some(text.into());
        self
    }

    /// The scroll container the content scrolls in (upstream finds the nearest ancestor
    /// that scrolls and overflows). Without one, or while it fits, nothing pulls and a
    /// refresh moves no scroll.
    pub fn scroll_handle(mut self, handle: ScrollHandle) -> Self {
        self.scroll = Some(handle);
        self
    }
}

/// A motion engine kept across frames on the executor's clock.
#[derive(Clone)]
struct Engine {
    engine: MotionEngine,
    origin: Instant,
}

impl Engine {
    fn new(cx: &App, initial: MotionValues) -> Self {
        let executor = cx.background_executor().clone();
        let origin = executor.now();
        let clock: Rc<dyn Fn() -> f64> = Rc::new(move || {
            executor
                .now()
                .saturating_duration_since(origin)
                .as_secs_f64()
                * 1000.
        });
        Engine {
            engine: MotionEngine::new(EngineOptions {
                initial,
                now: Some(clock),
                frames: true,
            }),
            origin,
        }
    }

    fn now(&self, cx: &App) -> f64 {
        cx.background_executor()
            .now()
            .saturating_duration_since(self.origin)
            .as_secs_f64()
            * 1000.
    }

    fn get(&self, property: MotionProperty) -> f64 {
        self.engine.get(property, None)
    }
}

/// The geometry of the pull in progress.
#[derive(Clone, Copy)]
struct Pull {
    travel0: f64,
    scroll_top0: f64,
    dimension: f64,
    floor: f64,
    trigger: f64,
    mode: PullMode,
}

/// An end waiting to slide, frames after its render.
struct PendingEnd {
    frames: u8,
    animate_to: f64,
    scroll_by: f64,
    owner: Option<ScrollHandle>,
}

/// A slide in flight, and what lands when it finishes.
struct EndSlide {
    handle: MotionHandle,
    scroll_by: f64,
    owner: Option<ScrollHandle>,
}

/// The two pans: the touch pull watches a finger, the scroll pull takes a trackpad's
/// scroll at the top.
struct Recognizers {
    attachment: GestureAttachment,
    pan: PanGestureRecognizer,
    scroll: PanGestureRecognizer,
}

struct Inner {
    // The wiring (upstream `useRefreshControl`).
    model: RefreshState,
    is_refreshing: bool,
    awaiting_prop: bool,
    refresh_token: u64,
    on_refresh: Option<RefreshHandler>,
    is_controlled: bool,
    announce: Option<SharedString>,
    last_prop: Option<bool>,
    // The touch layer (upstream `useRefreshTouchController`).
    scroll: Option<ScrollHandle>,
    enabled: bool,
    content: Engine,
    indicator: Engine,
    reveal: f64,
    has_inset: bool,
    pending_end: Option<PendingEnd>,
    end_slide: Option<EndSlide>,
    fade: Option<MotionHandle>,
    pull: Pull,
    finger_slope: f64,
    release_velocity: f64,
    pull_pointer: PointerKind,
    anchor: Option<f64>,
    last_translation: f64,
    fingers: Vec<u64>,
    recognizers: Option<Recognizers>,
}

/// The scroller key the content registers its container under, apart from the gesture
/// areas' keys.
const SCROLLER_KEY: u64 = 1 << 61;

/// The keyed state that holds the refresh across frames.
struct Holder(Rc<RefCell<Inner>>);

type Shared = Rc<RefCell<Inner>>;

fn overflows(handle: &ScrollHandle) -> bool {
    f32::from(handle.max_offset().y) > 0.
}

fn scroll_top_of(owner: Option<&ScrollHandle>) -> f64 {
    owner.map_or(0., |h| -f64::from(f32::from(h.offset().y)))
}

fn height_of(owner: &ScrollHandle) -> f64 {
    f64::from(f32::from(owner.bounds().size.height))
}

fn scroll_owner_by(owner: Option<&ScrollHandle>, delta: f64) {
    let Some(owner) = owner else {
        return;
    };
    if delta == 0. {
        return;
    }
    let max = f64::from(f32::from(owner.max_offset().y));
    let top = (scroll_top_of(Some(owner)) + delta).clamp(0., max.max(0.));
    let offset = owner.offset();
    owner.set_offset(point(offset.x, px(-(top as f32))));
}

/// A curve from `from` to `to` over `ms` on a cubic Bézier (upstream `bezierCurve`).
fn bezier_curve(from: f64, to: f64, ms: f64, points: [f64; 4]) -> FnCurve {
    let ease = cubic_bezier(points);
    let duration = ms / 1000.;
    let at = Rc::new(move |t: f64| from + (to - from) * ease.ease((t / duration).clamp(0., 1.)));
    let value = at.clone();
    // Upstream `derivative`: a central difference a millisecond either side.
    let velocity = move |t: f64| {
        let step = 1. / 1000.;
        let a = js_max(0., t - step);
        let b = (t + step).min(duration);
        if b > a { (at(b) - at(a)) / (b - a) } else { 0. }
    };
    FnCurve::new(duration, move |t| value(t), velocity)
}

impl Inner {
    fn owner(&self) -> Option<ScrollHandle> {
        self.scroll.clone().filter(overflows)
    }

    /// Places the content at `y` and pins the indicator at the top of the scroll view:
    /// it rides on the content's edge, so it moves by the inset minus `y`.
    fn place(&self, y: f64) {
        self.content.engine.set(MotionValues::new().y(y));
        self.indicator
            .engine
            .set(MotionValues::new().y(REFRESH_HEIGHT - y));
    }

    fn stop_placing(&self) {
        let y = [MotionProperty::Y.into()];
        self.content.engine.stop(Some(&y));
        self.indicator.engine.stop(Some(&y));
    }

    /// Puts the indicator back to opaque, writing only if it changed.
    fn restore_indicator(&self) {
        if self.indicator.get(MotionProperty::Opacity) != 1. {
            self.indicator.engine.set(MotionValues::new().opacity(1.));
        }
    }

    fn reset_indicator(&mut self) {
        self.reveal = 0.;
        self.restore_indicator();
    }

    /// Ends the indicator's fade and keeps what it shows: a finger that catches the
    /// settle back to the top pulls on, or resumes the settle, from the screen.
    fn keep_shown_indicator(&mut self) {
        let opacity = self.indicator.get(MotionProperty::Opacity);
        if opacity == 1. {
            return;
        }
        self.reveal *= opacity;
        self.restore_indicator();
    }

    /// The indicator in full: a trigger, or a refresh begun without a pull.
    fn show_indicator(&mut self) {
        self.reveal = 1.;
        self.restore_indicator();
    }

    fn start_settle(&mut self, to: SettleTarget) {
        let target = release_target(to == SettleTarget::Inset);
        let from = self.content.get(MotionProperty::Y);
        // The settle starts at the content's speed as the finger lifts. A caught settle
        // that resumes starts from rest.
        let velocity = std::mem::take(&mut self.release_velocity);
        // A finger's release starts it as a touch release, so under Reduce Motion the
        // animator cross-fades it instead of moving it. A trackpad's lands at once.
        let touch_release = self.pull_pointer == PointerKind::Touch;
        let options = AnimateOptions {
            touch_release,
            ..AnimateOptions::default()
        };
        let settle = self.content.engine.animate(
            MotionValues::new().y(target),
            SpringTransition::new(BOUNCE).velocity(MotionValues::new().y(velocity)),
            options,
        );
        self.indicator.engine.animate(
            MotionValues::new().y(REFRESH_HEIGHT - target),
            SpringTransition::new(BOUNCE).velocity(MotionValues::new().y(-velocity)),
            options,
        );
        if to == SettleTarget::Top && !self.model.is_refreshing && from > 0. {
            // The indicator fades as the content covers it, along the content's own
            // settle curve. Under Reduce Motion the settle has no curve, and the
            // indicator resets once the settle ends.
            let dimension = self.pull.dimension;
            let start = match reveal_progress(from, dimension) {
                p if p == 0. || p.is_nan() => 1.,
                p => p,
            };
            let finished = match settle.curve(MotionProperty::Y) {
                None => settle,
                Some(curve) => {
                    let fade = map_progress(curve, move |y| reveal_progress(y, dimension) / start);
                    self.indicator
                        .engine
                        .play(
                            MotionCurves::new().with(MotionProperty::Opacity, Rc::new(fade)),
                            options,
                        )
                        .unwrap_or(settle)
                }
            };
            self.fade = Some(finished);
        }
    }

    fn settle(&mut self, to: SettleTarget) {
        if to == SettleTarget::Inset {
            self.has_inset = true;
        }
        self.start_settle(to);
    }

    fn end(&mut self) {
        let had_inset = std::mem::take(&mut self.has_inset);
        if self.model.is_pulling {
            // Under the finger: the release settles back to the top, and the indicator
            // retracts with it.
            self.pull.floor = 0.;
            return;
        }
        if !had_inset {
            // A refresh that never showed (begun without a finger): nothing to move.
            self.reset_indicator();
            return;
        }
        // From rest: a settle still easing onto the inset stops where it is shown.
        self.stop_placing();
        let owner = self.owner();
        let EndMotion {
            animate_to,
            scroll_by,
        } = end_motion(scroll_top_of(owner.as_ref()));
        let Some(animate_to) = animate_to else {
            self.place(0.);
            scroll_owner_by(owner.as_ref(), scroll_by);
            self.reset_indicator();
            return;
        };
        // The slide starts two frames after the refresh's end renders, with the app's
        // update and the busy state.
        self.pending_end = Some(PendingEnd {
            frames: 3,
            animate_to,
            scroll_by,
            owner,
        });
    }

    fn finish_end(&mut self, owner: Option<&ScrollHandle>, scroll_by: f64) {
        self.place(0.);
        scroll_owner_by(owner, scroll_by);
        if !self.model.is_refreshing {
            self.reset_indicator();
        }
    }

    /// Plays a pending end. The indicator stays opaque while the content slides up over
    /// its gap (UIKit holds it at opacity 1 for nearly the whole end).
    fn play_end(&mut self, end: PendingEnd, duration_ms: Option<f64>) {
        let from = self.content.get(MotionProperty::Y);
        let timing = end_timing(duration_ms);
        let Some(EndTiming { ms, points }) = timing.filter(|t| t.ms > 0.) else {
            self.finish_end(end.owner.as_ref(), end.scroll_by);
            return;
        };
        let _ = self.indicator.engine.play(
            MotionCurves::new().with(
                MotionProperty::Y,
                Rc::new(bezier_curve(
                    REFRESH_HEIGHT - from,
                    REFRESH_HEIGHT - end.animate_to,
                    ms,
                    points,
                )),
            ),
            AnimateOptions::default(),
        );
        match self.content.engine.play(
            MotionCurves::new().with(
                MotionProperty::Y,
                Rc::new(bezier_curve(from, end.animate_to, ms, points)),
            ),
            AnimateOptions::default(),
        ) {
            Ok(handle) => {
                self.end_slide = Some(EndSlide {
                    handle,
                    scroll_by: end.scroll_by,
                    owner: end.owner,
                })
            }
            Err(_) => self.finish_end(end.owner.as_ref(), end.scroll_by),
        }
    }

    fn trigger(&mut self) {
        // Only a pull triggers: it settles onto the inset.
        self.has_inset = true;
        self.show_indicator();
    }

    fn begin(&mut self) {
        self.show_indicator();
        if !self.model.is_pulling {
            // The inset arrives with no visible motion, as beginRefreshing().
            self.pending_end = None;
            let owner = self.owner();
            self.place(REFRESH_HEIGHT);
            scroll_owner_by(owner.as_ref(), REFRESH_HEIGHT);
            self.has_inset = true;
        }
    }

    /// A settle a finger caught resumes when no pull follows.
    fn resume_settle(&mut self) {
        if self.model.is_pulling {
            return;
        }
        let target = release_target(self.model.is_refreshing);
        if (self.content.get(MotionProperty::Y) - target).abs() > 0.05 {
            self.settle(if self.model.is_refreshing {
                SettleTarget::Inset
            } else {
                SettleTarget::Top
            });
        }
    }

    /// A finger landing on a settle in flight stops it (UIScrollView brakes), as a
    /// trackpad's scroll starting does.
    fn touch_down(&mut self) {
        self.pending_end = None;
        self.stop_placing();
        self.keep_shown_indicator();
    }

    /// One frame: runs what came due, starts a pending end, and lands what finished.
    /// Whether anything still moves.
    fn tick(&mut self, duration_ms: Option<f64>, cx: &App) -> bool {
        for engine in [&self.content, &self.indicator] {
            let now = engine.now(cx);
            engine.engine.advance(now);
            engine.engine.frame(now);
        }
        if let Some(end) = self.pending_end.as_mut() {
            end.frames = end.frames.saturating_sub(1);
            if end.frames == 0
                && let Some(end) = self.pending_end.take()
            {
                self.play_end(end, duration_ms);
            }
        }
        if let Some(fade) = &self.fade
            && let Some(done) = fade.finished()
        {
            self.fade = None;
            if done && !self.model.is_refreshing {
                self.reset_indicator();
            }
        }
        if let Some(slide) = &self.end_slide
            && let Some(done) = slide.handle.finished()
        {
            let slide = self.end_slide.take().expect("a slide");
            if done {
                self.finish_end(slide.owner.as_ref(), slide.scroll_by);
            }
        }
        self.pending_end.is_some()
            || self.end_slide.is_some()
            || self.fade.is_some()
            || !self.content.engine.is_settled()
            || !self.indicator.engine.is_settled()
    }
}

/// What a pan reported, read in its own callback.
#[derive(Clone, Copy)]
struct PanSample {
    state: GestureState,
    translation: f64,
    travel_at_began: f64,
    scroll_velocity: f64,
    pointer: PointerKind,
}

fn notify(holder: &WeakEntity<Holder>, cx: &mut App) {
    if let Some(holder) = holder.upgrade() {
        holder.update(cx, |_, cx| cx.notify());
    }
}

/// Runs the state machine on an event and its effects (upstream `dispatch` and `run`).
fn dispatch(
    shared: &Shared,
    holder: &WeakEntity<Holder>,
    event: RefreshEvent,
    window: &mut Window,
    cx: &mut App,
) {
    let effects = {
        let mut s = shared.borrow_mut();
        let (next, effects) = reduce_refresh(s.model, event);
        s.model = next;
        effects
    };
    for effect in effects {
        run(shared, holder, effect, window, cx);
    }
}

fn say(shared: &Shared, window: &mut Window, cx: &mut App) {
    let words = shared.borrow().announce.clone();
    if let Some(words) = words.filter(|w| !w.is_empty()) {
        announce(words, Politeness::Polite, window, cx);
    }
}

fn run(
    shared: &Shared,
    holder: &WeakEntity<Holder>,
    effect: RefreshEffect,
    window: &mut Window,
    cx: &mut App,
) {
    match effect {
        RefreshEffect::Trigger => {
            {
                let mut s = shared.borrow_mut();
                s.is_refreshing = true;
                s.trigger();
            }
            say(shared, window, cx);
            call_handler(shared, holder, window, cx);
        }
        RefreshEffect::Begin {
            call_handler: calls,
        } => {
            shared.borrow_mut().is_refreshing = true;
            say(shared, window, cx);
            shared.borrow_mut().begin();
            if calls {
                call_handler(shared, holder, window, cx);
            }
        }
        RefreshEffect::Settle(to) => shared.borrow_mut().settle(to),
        RefreshEffect::End => {
            let mut s = shared.borrow_mut();
            s.is_refreshing = false;
            s.end();
        }
    }
    notify(holder, cx);
    window.refresh();
}

/// Calls the app's handler. Controlled, the parent takes the refresh by setting
/// `is_refreshing`, or it ends on the next render; otherwise it ends when the task the
/// handler returns completes.
fn call_handler(shared: &Shared, holder: &WeakEntity<Holder>, window: &mut Window, cx: &mut App) {
    let (token, handler, controlled) = {
        let mut s = shared.borrow_mut();
        s.refresh_token += 1;
        (s.refresh_token, s.on_refresh.clone(), s.is_controlled)
    };
    let task = handler.and_then(|handler| handler(window, cx));
    if controlled {
        shared.borrow_mut().awaiting_prop = true;
        notify(holder, cx);
        return;
    }
    let weak = Rc::downgrade(shared);
    let holder = holder.clone();
    window
        .spawn(cx, async move |cx| {
            if let Some(task) = task {
                task.await;
            }
            let _ = cx.update(|window, cx| {
                let Some(shared) = weak.upgrade() else {
                    return;
                };
                if shared.borrow().refresh_token == token {
                    dispatch(&shared, &holder, RefreshEvent::End, window, cx);
                }
            });
        })
        .detach();
}

/// Upstream `onPull`: the pull's travel to the overscroll shown, the trigger, and the
/// release's handoff to the settle.
fn on_pull(
    shared: &Shared,
    holder: &WeakEntity<Holder>,
    pan: PanSample,
    window: &mut Window,
    cx: &mut App,
) {
    match pan.state {
        GestureState::Began | GestureState::Changed => {
            let began = pan.state == GestureState::Began;
            let translation = pan.translation;
            if shared.borrow().anchor.is_none() {
                let mut s = shared.borrow_mut();
                // Not pulling yet: the content can be scrolling, in a drag that began as
                // a scroll. The pull starts in the same touch once the content is at its
                // top and the finger moves down. At `began` the finger's whole move
                // counts, the threshold's included.
                let current = s.owner();
                let is_moving_down = if began {
                    translation + pan.travel_at_began > 0.
                } else {
                    translation > s.last_translation
                };
                s.last_translation = translation;
                let Some(current) = current else {
                    return;
                };
                if !is_moving_down || scroll_top_of(Some(&current)) > 0. {
                    return;
                }
                // From rest the travel is the pan's (which leaves out its hysteresis, as
                // UIKit's does); past a scroll it starts where the content ran out.
                s.anchor = Some(if began { 0. } else { translation });
                s.pull_pointer = pan.pointer;
                let shown = s.content.get(MotionProperty::Y);
                s.stop_placing();
                let dimension = height_of(&current);
                let mode = if s.model.is_refreshing {
                    PullMode::Band
                } else {
                    PullMode::Reveal
                };
                s.pull = Pull {
                    dimension,
                    mode,
                    trigger: trigger_distance(dimension),
                    floor: if s.model.is_refreshing {
                        REFRESH_HEIGHT
                    } else {
                        0.
                    },
                    travel0: pull_travel(shown, dimension, mode),
                    scroll_top0: scroll_top_of(Some(&current)),
                };
                drop(s);
                dispatch(shared, holder, RefreshEvent::PullStart, window, cx);
            }
            let (overscroll, trigger, travel, had_triggered) = {
                let mut s = shared.borrow_mut();
                s.last_translation = translation;
                let geometry = s.pull;
                // A platform that scrolls the content back as the finger returns takes
                // over from the pull.
                let scrolled_back = scroll_top_of(s.owner().as_ref()) - geometry.scroll_top0 > 0.5;
                let anchor = s.anchor.unwrap_or(0.);
                let travel = if scrolled_back {
                    0.
                } else {
                    geometry.travel0 + (translation - anchor)
                };
                let pulled = pull_overscroll(travel, geometry.dimension, geometry.mode);
                let overscroll = js_max(geometry.floor, pulled);
                // The content follows the finger by the pull's slope, unless the
                // refreshing inset holds it.
                s.finger_slope = if pulled > geometry.floor {
                    pull_slope(travel, geometry.dimension, geometry.mode)
                } else {
                    0.
                };
                (overscroll, geometry.trigger, travel, s.model.has_triggered)
            };
            dispatch(
                shared,
                holder,
                RefreshEvent::PullMove {
                    overscroll,
                    trigger,
                },
                window,
                cx,
            );
            let mut s = shared.borrow_mut();
            if !had_triggered && s.model.has_triggered {
                // The frame after the trigger steps onto the finger (within the
                // one-to-one band).
                s.pull.mode = if s.pull.mode == PullMode::Band {
                    PullMode::Band
                } else {
                    mode_after_trigger(travel)
                };
            }
            s.place(overscroll);
            if !s.model.is_refreshing {
                s.reveal = reveal_progress(overscroll, s.pull.dimension);
            }
        }
        GestureState::Ended | GestureState::Cancelled => {
            let was_pulling = shared.borrow_mut().anchor.take().is_some();
            if !was_pulling {
                // A scroll that never reached the top: a settle the finger caught
                // resumes.
                shared.borrow_mut().resume_settle();
                return;
            }
            let ended = pan.state == GestureState::Ended;
            {
                // A lift hands the content's speed to the settle: the release velocity
                // times the pull's slope. A cancelled pull's settle starts from rest.
                let mut s = shared.borrow_mut();
                s.release_velocity = if ended {
                    pan.scroll_velocity * s.finger_slope
                } else {
                    0.
                };
            }
            let event = if ended {
                RefreshEvent::PullEnd
            } else {
                RefreshEvent::PullCancel
            };
            dispatch(shared, holder, event, window, cx);
            shared.borrow_mut().release_velocity = 0.;
        }
        _ => {}
    }
}

/// The pull's two pans (upstream `RefreshPullRecognizer` and
/// `RefreshScrollPullRecognizer`).
fn recognizers(
    shared: &Shared,
    holder: &WeakEntity<Holder>,
    window: &mut Window,
    cx: &mut App,
) -> Recognizers {
    let host = gesture_host(window, cx);
    // UIKit's refresh pull is the scroll view's own drag, not a second gesture: one touch
    // can scroll, pull past the top, and scroll the same content again. So this pan never
    // claims its touch and only watches it; the window keeps the finger for its own
    // scrolling, and the pan reads it to the lift. A pen's or a mouse's drag stays the
    // platform's, for text selection.
    let pan = PanGestureRecognizer::new(PanOptions {
        direction: PanDirection::Vertical,
        maximum_number_of_touches: 1,
        base: RecognizerOptions {
            allowed_pointer_types: Some(vec![PointerKind::Touch]),
            claims_touches: false,
            ..RecognizerOptions::named("RefreshPull")
        },
        ..PanOptions::default()
    });
    // A trackpad's two-finger scroll at the top: a scroll-edge pan that takes a
    // trackpad's scroll and no pointer, so it takes a scroll only where nothing under the
    // pointer could scroll. The scroll it takes is its own.
    let scroll = PanGestureRecognizer::new(PanOptions {
        direction: PanDirection::Vertical,
        maximum_number_of_touches: 1,
        scroll_edge: true,
        base: RecognizerOptions {
            allowed_pointer_types: Some(Vec::new()),
            allowed_scroll_types: Some(vec![ScrollType::Continuous]),
            claims_touches: true,
            ..RecognizerOptions::named("RefreshScrollPull")
        },
        ..PanOptions::default()
    });
    let attachment = host.attach(&[pan.any(), scroll.any()]);
    for (recognizer, is_scroll) in [(&pan, false), (&scroll, true)] {
        let weak = Rc::downgrade(shared);
        let listener_holder = holder.clone();
        recognizer.add_listener(move |pan| {
            let first = pan.tracked_touches().into_iter().next();
            let pointer = match (&first, is_scroll) {
                // A trackpad's scroll is no touch release.
                (_, true) => PointerKind::Mouse,
                (Some(touch), false) => touch.pointer_type(),
                (None, false) => PointerKind::Touch,
            };
            let sample = PanSample {
                state: pan.state(),
                translation: pan.translation().y,
                travel_at_began: pan.travel_at_began().y,
                scroll_velocity: pan.scroll_velocity().y,
                pointer,
            };
            let weak = weak.clone();
            let holder = listener_holder.clone();
            after_gesture(move |window, cx| {
                if let Some(shared) = weak.upgrade() {
                    on_pull(&shared, &holder, sample, window, cx);
                    notify(&holder, cx);
                }
            });
        });
        let weak = Rc::downgrade(shared);
        let holder = holder.clone();
        recognizer.add_transition_listener(move |_, _, to| {
            if to != GestureState::Failed {
                return;
            }
            let weak = weak.clone();
            let holder = holder.clone();
            after_gesture(move |_, cx| {
                if let Some(shared) = weak.upgrade() {
                    shared.borrow_mut().resume_settle();
                    notify(&holder, cx);
                }
            });
        });
    }
    Recognizers {
        attachment,
        pan,
        scroll,
    }
}

/// Upstream `UseRefreshControlResult`: the pulled content's and the indicator's
/// wrappers, `is_refreshing` and `refresh()`. Cheap to clone; every clone is the same
/// control.
#[derive(Clone)]
pub struct RefreshControl {
    shared: Shared,
    holder: WeakEntity<Holder>,
    attachment: GestureAttachment,
    is_refreshing: bool,
    content_y: f64,
    content_opacity: f64,
    indicator_y: f64,
    indicator_opacity: f64,
    reveal: f64,
}

impl RefreshControl {
    /// Upstream `contentRef`: wraps the scrolled content, which the pull moves. Put the
    /// indicator's box inside it, riding on its top edge.
    pub fn content(&self, child: impl IntoElement) -> AnyElement {
        let mut el = div().relative().w_full().child(child);
        // At rest the content carries no translate, so nothing it holds moves.
        if self.content_y != 0. && self.content_y.is_finite() {
            el = el.translate_y(px(self.content_y as f32));
        }
        if self.content_opacity < 1. {
            el = el.opacity(self.content_opacity as f32);
        }
        let shared = Rc::downgrade(&self.shared);
        let holder = self.holder.clone();
        let scroller = Rc::downgrade(&self.shared);
        let key = self.attachment.key() | SCROLLER_KEY;
        // The scroll container, for the pans' scroll-edge probe, and the landing of a
        // gesture's first contact (upstream `onTouchDown`).
        let watch = canvas(
            move |_, window, cx| {
                let Some(shared) = scroller.upgrade() else {
                    return;
                };
                let s = shared.borrow();
                let Some(handle) = s.scroll.clone() else {
                    return;
                };
                // Near its top, the container stops scroll chaining, so a pull there is
                // the pull's and not the page's (upstream's `overscroll-behavior-y: none`).
                let near = s.enabled
                    && overflows(&handle)
                    && scroll_top_of(Some(&handle)) < height_of(&handle).max(REFRESH_HEIGHT);
                drop(s);
                let mut native = NativeScroller::new(key, handle.bounds(), handle);
                native.scrolls_x = false;
                native.finger_x = false;
                native.stops_y = near;
                gesture_host(window, cx).register_native_scroller(native);
            },
            move |bounds: Bounds<Pixels>, _, window, _| {
                let touch_shared = shared.clone();
                let touch_holder = holder.clone();
                window.on_touch_event(move |event, _, cx| {
                    let Some(shared) = touch_shared.upgrade() else {
                        return false;
                    };
                    let mut s = shared.borrow_mut();
                    match event.phase {
                        TouchPhase::Started if bounds.contains(&event.position) => {
                            let first = s.fingers.is_empty();
                            s.fingers.push(event.id.0);
                            if first && s.enabled {
                                s.touch_down();
                                drop(s);
                                notify(&touch_holder, cx);
                            }
                        }
                        TouchPhase::Ended | TouchPhase::Cancelled => {
                            s.fingers.retain(|f| *f != event.id.0)
                        }
                        _ => {}
                    }
                    false
                });
                let scroll_shared = shared.clone();
                let scroll_holder = holder.clone();
                window.on_mouse_event(move |event: &ScrollEvent, phase, _, cx| {
                    if phase != DispatchPhase::Capture
                        || event.touch_phase != TouchPhase::Started
                        || !event.delta.precise()
                        || !bounds.contains(&event.position)
                    {
                        return;
                    }
                    if let Some(shared) = scroll_shared.upgrade()
                        && shared.borrow().enabled
                    {
                        shared.borrow_mut().touch_down();
                        notify(&scroll_holder, cx);
                    }
                });
            },
        )
        .absolute()
        .top_0()
        .left_0()
        .size_full();
        gesture_area(&self.attachment, el.child(watch)).into_any_element()
    }

    /// Upstream `indicatorRef`: wraps the app's indicator, a band as tall as the refresh
    /// height. It sits at the bottom of an overflow-hidden box that rides on the
    /// content's top edge; the control moves the band against the content, so it stays
    /// pinned at the top of the scroll view, and fades it as a settle takes the content
    /// back. The content's edge covers it.
    pub fn indicator(&self, child: impl IntoElement) -> AnyElement {
        let mut el = div()
            .absolute()
            .left_0()
            .right_0()
            .bottom_0()
            .h(px(REFRESH_HEIGHT as f32))
            .child(child);
        if self.indicator_y != 0. && self.indicator_y.is_finite() {
            el = el.translate_y(px(self.indicator_y as f32));
        }
        if self.indicator_opacity < 1. {
            el = el.opacity(self.indicator_opacity.max(0.) as f32);
        }
        el.into_any_element()
    }

    /// How much of the indicator the pull reveals, 0 to 1 (upstream's
    /// `--astryx-refresh-reveal`): the indicator's own styles read it.
    pub fn reveal(&self) -> f64 {
        self.reveal
    }

    /// How far the content is pulled down now, px.
    pub fn offset(&self) -> f64 {
        self.content_y
    }

    /// Upstream `isRefreshing`: whether a refresh is in progress.
    pub fn is_refreshing(&self) -> bool {
        self.is_refreshing
    }

    /// Upstream `refresh()`: starts a refresh without a pull (the non-gesture path).
    pub fn refresh(&self, window: &mut Window, cx: &mut App) {
        dispatch(
            &self.shared,
            &self.holder,
            RefreshEvent::Begin(BeginSource::User),
            window,
            cx,
        );
    }
}

/// Upstream `useRefreshControl`: pull to refresh for the scroll container in
/// `options`, kept across frames under `id`. Call it once per render.
pub fn use_refresh_control(
    id: impl Into<ElementId>,
    options: RefreshControlOptions,
    window: &mut Window,
    cx: &mut App,
) -> RefreshControl {
    let holder: Entity<Holder> = window.use_keyed_state(id.into(), cx, |_, cx| {
        Holder(Rc::new(RefCell::new(Inner {
            model: INITIAL_REFRESH_STATE,
            is_refreshing: false,
            awaiting_prop: false,
            refresh_token: 0,
            on_refresh: None,
            is_controlled: false,
            announce: None,
            last_prop: None,
            scroll: None,
            enabled: true,
            content: Engine::new(cx, MotionValues::new().y(0.)),
            indicator: Engine::new(cx, MotionValues::new().y(REFRESH_HEIGHT).opacity(1.)),
            reveal: 0.,
            has_inset: false,
            pending_end: None,
            end_slide: None,
            fade: None,
            pull: Pull {
                travel0: 0.,
                scroll_top0: 0.,
                dimension: 1.,
                floor: 0.,
                trigger: trigger_distance(1.),
                mode: PullMode::Reveal,
            },
            finger_slope: 0.,
            release_velocity: 0.,
            pull_pointer: PointerKind::Touch,
            anchor: None,
            last_translation: 0.,
            fingers: Vec::new(),
            recognizers: None,
        })))
    });
    let shared = holder.read(cx).0.clone();
    let weak = holder.downgrade();
    let reduced = prefers_reduced_motion(cx);
    let coarse = coarse_primary_pointer(window, cx);
    {
        let mut s = shared.borrow_mut();
        s.on_refresh = options.on_refresh.clone();
        s.is_controlled = options.is_refreshing.is_some();
        s.announce = options.announce.clone();
        s.scroll = options.scroll.clone();
        for engine in [&s.content, &s.indicator] {
            engine.engine.set_reduced_motion(reduced);
            engine.engine.set_coarse_pointer(coarse);
        }
    }
    if shared.borrow().recognizers.is_none() {
        let r = recognizers(&shared, &weak, window, cx);
        shared.borrow_mut().recognizers = Some(r);
    }
    // Whether the pull applies. Turned off mid-pull, the pans report `Cancelled`, and the
    // content settles back.
    {
        let mut s = shared.borrow_mut();
        let enabled = !options.is_disabled;
        if s.enabled != enabled {
            s.enabled = enabled;
            if let Some(r) = &s.recognizers {
                r.pan.set_enabled(enabled);
                r.scroll.set_enabled(enabled);
            }
        }
    }
    // A controlled `is_refreshing` drives the state machine when it changes, and a refresh
    // that the hook started under one ends unless the parent took it. Both run after the
    // frame is laid out, as upstream's effects run after the commit, so the inset meets a
    // measured scroll container.
    let prop = options.is_refreshing;
    let changed = prop.is_some() && shared.borrow().last_prop != prop;
    shared.borrow_mut().last_prop = prop;
    let awaiting = std::mem::take(&mut shared.borrow_mut().awaiting_prop);
    if changed || (awaiting && prop == Some(false)) {
        let (shared, weak) = (shared.clone(), weak.clone());
        window.defer(cx, move |window, cx| {
            if changed && let Some(refreshing) = prop {
                let event = if refreshing {
                    RefreshEvent::Begin(BeginSource::Program)
                } else {
                    RefreshEvent::End
                };
                dispatch(&shared, &weak, event, window, cx);
            }
            // The parent did not take the refresh: it ends.
            if awaiting && prop == Some(false) {
                dispatch(&shared, &weak, RefreshEvent::End, window, cx);
            }
        });
    }
    let duration_ms = {
        let theme = rustryx_theme::current(window, cx);
        Some(theme.tokens().duration.medium.as_secs_f64() * 1000.)
    };
    let moving = shared.borrow_mut().tick(duration_ms, cx);
    if moving {
        window.request_animation_frame();
    }
    let s = shared.borrow();
    let attachment = s.recognizers.as_ref().expect("attached").attachment.clone();
    let now = s.content.now(cx);
    RefreshControl {
        is_refreshing: match prop {
            Some(refreshing) => refreshing,
            None => s.is_refreshing,
        },
        content_y: s.content.get(MotionProperty::Y),
        content_opacity: s.content.engine.shown_opacity(Some(now)),
        indicator_y: s.indicator.get(MotionProperty::Y),
        indicator_opacity: s.indicator.engine.shown_opacity(Some(s.indicator.now(cx))),
        reveal: s.reveal,
        attachment,
        holder: weak,
        shared: shared.clone(),
    }
}

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

/// Upstream lab `useRefreshControl` (a hook: its options, then its result).
pub const REFRESH_CONTROL_DOC: ComponentDoc = ComponentDoc {
    name: "RefreshControl",
    rust_type: "rustryx::lab::refresh_control::use_refresh_control",
    props: &[
        p(
            "onRefresh",
            RustApi::Method("RefreshControlOptions::on_refresh"),
        ),
        p(
            "isRefreshing",
            RustApi::Method("RefreshControlOptions::is_refreshing"),
        ),
        p(
            "isDisabled",
            RustApi::Method("RefreshControlOptions::disabled"),
        ),
        p(
            "announce",
            RustApi::Method("RefreshControlOptions::announce"),
        ),
        p("contentRef", RustApi::Method("RefreshControl::content")),
        p("indicatorRef", RustApi::Method("RefreshControl::indicator")),
        p(
            "isRefreshing (result)",
            RustApi::Method("RefreshControl::is_refreshing"),
        ),
        p("refresh", RustApi::Method("RefreshControl::refresh")),
        p(
            "REFRESH_REVEAL_VAR",
            RustApi::Method("RefreshControl::reveal (GPUI has no CSS custom properties)"),
        ),
    ],
    example: include_str!("examples.rs"),
};
