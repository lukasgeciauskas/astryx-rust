//! The public long press, upstream `hooks/useLongPress.ts` over `hooks/longPressDetector.ts`,
//! and the press claim (`utils/pressClaim.ts`).
//!
//! One press at a time: it fires with the touch's start point once a single finger has
//! stayed within the movement tolerance (on each axis) for the hold. A second finger, a
//! lift, a cancel or travel past the tolerance drops it. The hold is a deadline on the
//! touch events' own clock: a touch stamped at or past the deadline (a busy window that
//! delayed the timer) reports the press before it acts; a press started by hand from a
//! point has no stamp and waits the full delay; a late timer yields to queued input once,
//! so a lift stamped before the deadline still cancels it. The defaults are the long-press
//! recognizer's (UIKit's 500 ms and 10 pt). A press that fires claims the press that is
//! down: its lift is no tap.
//!
//! [`use_long_press`] hands out [`LongPressHandlers`]: an [`LongPressHandlers::area`] that
//! hears the fingers on an element (the touch handlers upstream spreads), and
//! `start` / `move_to` / `cancel` for a caller already following the pointer itself.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gpui::{
    AnyElement, App, Bounds, Element, ElementId, GlobalElementId, Hitbox, HitboxBehavior,
    InspectorElementId, IntoElement, LayoutId, Pixels, TouchEvent, TouchId, TouchPhase, Window,
};
use rustryx_primitives::gestures::{GestureClock, Point, TimerHandle};
use rustryx_primitives::motion::uikit::gestures::{
    LONG_PRESS_ALLOWABLE_MOVEMENT_PX, LONG_PRESS_DURATION_MS,
};

use super::clock::WindowClock;

// --- The press claim -------------------------------------------------------

type ClaimWatcher = (u64, Rc<dyn Fn()>);

thread_local! {
    static CLAIM_WATCHERS: RefCell<Vec<ClaimWatcher>> = const { RefCell::new(Vec::new()) };
    static NEXT_WATCHER: Cell<u64> = const { Cell::new(0) };
}

/// Upstream `claimPress`: tells each watcher that a long press acted on the press that is
/// down.
pub(crate) fn claim_press() {
    let watchers: Vec<_> =
        CLAIM_WATCHERS.with(|w| w.borrow().iter().map(|(_, f)| f.clone()).collect());
    for watcher in watchers {
        watcher();
    }
}

/// Upstream `watchPressClaims`: calls `watcher` at each claim until
/// [`unwatch_press_claims`] runs with the returned id.
pub(crate) fn watch_press_claims(watcher: impl Fn() + 'static) -> u64 {
    let id = NEXT_WATCHER.with(|n| {
        n.set(n.get() + 1);
        n.get()
    });
    CLAIM_WATCHERS.with(|w| w.borrow_mut().push((id, Rc::new(watcher))));
    id
}

pub(crate) fn unwatch_press_claims(id: u64) {
    CLAIM_WATCHERS.with(|w| w.borrow_mut().retain(|(i, _)| *i != id));
}

// --- The detector ----------------------------------------------------------

/// A long press's listener: the touch's start point, in window px.
pub type LongPressListener = Rc<dyn Fn(Point, &mut Window, &mut App)>;

/// Upstream `LongPressDetectorOptions` (internal): what a detector reads at each start, move
/// and fire.
#[derive(Clone)]
pub(crate) struct DetectorOptions {
    pub on_long_press: Rc<dyn Fn(Point)>,
    pub disabled: bool,
    pub delay_ms: Option<f64>,
    pub move_cancel_px: Option<f64>,
}

impl DetectorOptions {
    #[cfg(test)]
    pub(crate) fn new(on_long_press: impl Fn(Point) + 'static) -> Self {
        Self {
            on_long_press: Rc::new(on_long_press),
            disabled: false,
            delay_ms: None,
            move_cancel_px: None,
        }
    }
}

/// Upstream `UseLongPressOptions`.
#[derive(Clone)]
pub struct LongPressOptions {
    /// Fired with the start point once the press is held for `delay_ms`.
    pub on_long_press: LongPressListener,
    /// When true, a start does nothing.
    pub disabled: bool,
    /// The hold before the press fires, ms. Defaults to 500.
    pub delay_ms: Option<f64>,
    /// Movement past this distance (px, either axis) cancels the press. Defaults to 10.
    pub move_cancel_px: Option<f64>,
}

impl LongPressOptions {
    pub fn new(on_long_press: impl Fn(Point, &mut Window, &mut App) + 'static) -> Self {
        Self {
            on_long_press: Rc::new(on_long_press),
            disabled: false,
            delay_ms: None,
            move_cancel_px: None,
        }
    }

    /// Upstream `disabled`.
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Upstream `delayMs`.
    pub fn delay_ms(mut self, ms: f64) -> Self {
        self.delay_ms = Some(ms);
        self
    }

    /// Upstream `moveCancelPx`.
    pub fn move_cancel_px(mut self, px: f64) -> Self {
        self.move_cancel_px = Some(px);
        self
    }
}

/// Upstream `LongPressTolerance` (internal): a press's own movement tolerance, for a
/// component that decides it as the press starts (a menu trigger).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct LongPressTolerance {
    pub move_cancel_px: Option<f64>,
}

/// Upstream `LongPressTouchEvent`: the fingers down and the event's time on the
/// detector's clock (`None`, or 0, for an event that carries none).
#[derive(Clone, Debug, Default)]
pub struct LongPressTouch {
    pub touches: Vec<Point>,
    pub time: Option<f64>,
}

impl LongPressTouch {
    /// Fingers at `touches`, stamped at `time`.
    pub fn new(touches: impl Into<Vec<Point>>, time: Option<f64>) -> Self {
        Self {
            touches: touches.into(),
            time,
        }
    }
}

struct DetectorState {
    options: DetectorOptions,
    timer: Option<TimerHandle>,
    from: Option<Point>,
    /// None for a press started by hand: it has no stamp.
    deadline: Option<f64>,
    tolerance: Option<LongPressTolerance>,
    generation: u64,
}

struct DetectorInner {
    clock: Rc<dyn GestureClock>,
    state: RefCell<DetectorState>,
}

/// Upstream `createLongPressDetector` (internal): one long press at a time, timed on
/// `clock`.
#[derive(Clone)]
pub(crate) struct LongPressDetector(Rc<DetectorInner>);

impl LongPressDetector {
    pub(crate) fn new(options: DetectorOptions, clock: Rc<dyn GestureClock>) -> Self {
        Self(Rc::new(DetectorInner {
            clock,
            state: RefCell::new(DetectorState {
                options,
                timer: None,
                from: None,
                deadline: None,
                tolerance: None,
                generation: 0,
            }),
        }))
    }

    /// The options the next start, move and fire read.
    pub(crate) fn set_options(&self, options: DetectorOptions) {
        self.0.state.borrow_mut().options = options;
    }

    /// Drops any pending press.
    pub(crate) fn cancel(&self) {
        let timer = {
            let mut s = self.0.state.borrow_mut();
            s.from = None;
            s.deadline = None;
            s.generation += 1;
            s.timer.take()
        };
        if let Some(timer) = timer {
            self.0.clock.clear_timeout(timer);
        }
    }

    /// Whether a press is in progress.
    pub(crate) fn is_pending(&self) -> bool {
        self.0.state.borrow().from.is_some()
    }

    fn fire(&self) {
        let Some(at) = self.0.state.borrow().from else {
            return;
        };
        self.cancel();
        // The press acted, so its lift is not a tap.
        claim_press();
        let listener = self.0.state.borrow().options.on_long_press.clone();
        listener(at);
    }

    /// A touch stamped at or past the deadline: the finger was down for the whole delay.
    fn fire_if_due(&self, time: Option<f64>) {
        let deadline = self.0.state.borrow().deadline;
        if let (Some(deadline), Some(time)) = (deadline, time.filter(|t| *t > 0.))
            && time >= deadline
        {
            self.fire();
        }
    }

    fn begin(&self, point: Point, tolerance: Option<LongPressTolerance>, started: Option<f64>) {
        if self.0.state.borrow().options.disabled {
            return;
        }
        self.cancel();
        let now = self.0.clock.now();
        let (delay, generation) = {
            let mut s = self.0.state.borrow_mut();
            let delay = s.options.delay_ms.unwrap_or(LONG_PRESS_DURATION_MS);
            s.from = Some(point);
            s.tolerance = tolerance;
            s.deadline = started.map(|at| at + delay);
            (delay, s.generation)
        };
        let deadline = started.map(|at| at + delay);
        // Wait only for the rest of the hold, never more than the delay.
        let remaining = match deadline {
            Some(deadline) => delay.min((deadline - now).max(0.)),
            None => delay,
        };
        self.arm(remaining, deadline, generation, true);
    }

    fn arm(&self, ms: f64, deadline: Option<f64>, generation: u64, may_yield: bool) {
        let weak = Rc::downgrade(&self.0);
        let timer = self.0.clock.set_timeout(
            Box::new(move || {
                let Some(inner) = weak.upgrade() else {
                    return;
                };
                let detector = LongPressDetector(inner);
                {
                    let mut s = detector.0.state.borrow_mut();
                    if s.generation != generation {
                        return;
                    }
                    s.timer = None;
                }
                // A late timer yields to queued input once, so a lift stamped before
                // the deadline still cancels the press.
                // (One millisecond, as a browser's zero timeout runs.)
                if may_yield && deadline.is_some_and(|d| detector.0.clock.now() > d) {
                    detector.arm(1., deadline, generation, false);
                    return;
                }
                detector.fire();
            }),
            ms,
        );
        self.0.state.borrow_mut().timer = Some(timer);
    }

    /// Begins detection at a point, dropping any press still pending.
    pub(crate) fn start(&self, point: Point, tolerance: Option<LongPressTolerance>) {
        self.begin(point, tolerance, None);
    }

    /// The pointer's new position: past the tolerance the press is off. Whether a press is
    /// still pending.
    pub(crate) fn move_to(&self, point: Point) -> bool {
        let (from, limit) = {
            let s = self.0.state.borrow();
            let Some(from) = s.from else {
                return false;
            };
            let limit = s
                .tolerance
                .and_then(|t| t.move_cancel_px)
                .or(s.options.move_cancel_px)
                .unwrap_or(LONG_PRESS_ALLOWABLE_MOVEMENT_PX);
            (from, limit)
        };
        if (point.x - from.x).abs() > limit || (point.y - from.y).abs() > limit {
            self.cancel();
            return false;
        }
        true
    }

    /// A touch started: one finger begins a press, a second one ends it.
    pub(crate) fn touch_start(&self, event: &LongPressTouch) {
        if self.0.state.borrow().options.disabled {
            return;
        }
        if event.touches.len() != 1 {
            // A second finger ends the press, unless the press was already due.
            self.fire_if_due(event.time);
            self.cancel();
            return;
        }
        self.begin(event.touches[0], None, event.time.filter(|t| *t > 0.));
    }

    /// A touch moved: past the tolerance the press is off.
    pub(crate) fn touch_move(&self, event: &LongPressTouch) {
        if !self.is_pending() {
            return;
        }
        self.fire_if_due(event.time);
        if !self.is_pending() {
            return;
        }
        if event.touches.len() != 1 {
            self.cancel();
            return;
        }
        self.move_to(event.touches[0]);
    }

    /// The lift or the cancel. A press it is stamped past reports first.
    pub(crate) fn touch_end(&self, event: Option<&LongPressTouch>) {
        self.fire_if_due(event.and_then(|e| e.time));
        self.cancel();
    }
}

// --- The hook --------------------------------------------------------------

struct HookState {
    detector: LongPressDetector,
    /// Points that fired and wait for the window.
    fired: Rc<RefCell<Vec<Point>>>,
    listener: Rc<RefCell<Option<LongPressListener>>>,
    /// The fingers down on the area, and whether the press claimed them.
    fingers: Rc<RefCell<Vec<(TouchId, Point)>>>,
    claimed: Rc<Cell<bool>>,
    claim_watch: u64,
}

impl Drop for HookState {
    fn drop(&mut self) {
        // The pending timer ends with the element.
        self.detector.cancel();
        unwatch_press_claims(self.claim_watch);
    }
}

/// Upstream `UseLongPressHandlers`. Cheap to clone.
#[derive(Clone)]
pub struct LongPressHandlers {
    detector: LongPressDetector,
    fired: Rc<RefCell<Vec<Point>>>,
    listener: Rc<RefCell<Option<LongPressListener>>>,
    fingers: Rc<RefCell<Vec<(TouchId, Point)>>>,
    claimed: Rc<Cell<bool>>,
}

impl LongPressHandlers {
    fn flush(&self, window: &mut Window, cx: &mut App) {
        flush_fired(&self.fired, &self.listener, window, cx);
    }

    /// Upstream `onTouchStart`: a finger landed; `touches` are every finger down.
    pub fn on_touch_start(&self, event: &LongPressTouch, window: &mut Window, cx: &mut App) {
        self.detector.touch_start(event);
        self.flush(window, cx);
    }

    /// Upstream `onTouchMove`.
    pub fn on_touch_move(&self, event: &LongPressTouch, window: &mut Window, cx: &mut App) {
        self.detector.touch_move(event);
        self.flush(window, cx);
    }

    /// Upstream `onTouchEnd`: pass the event, so a press it is stamped past still
    /// reports first.
    pub fn on_touch_end(&self, event: Option<&LongPressTouch>, window: &mut Window, cx: &mut App) {
        self.detector.touch_end(event);
        self.flush(window, cx);
    }

    /// Upstream `onTouchCancel`.
    pub fn on_touch_cancel(
        &self,
        event: Option<&LongPressTouch>,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.on_touch_end(event, window, cx);
    }

    /// Upstream `start`: begins detection at a point, for a caller driving this from its
    /// own pointer events. The public start takes only the point.
    pub fn start(&self, point: Point) {
        self.detector.start(point, None);
    }

    /// Upstream `moveTo`: past `move_cancel_px` the press is off; whether one is still
    /// pending.
    pub fn move_to(&self, point: Point) -> bool {
        self.detector.move_to(point)
    }

    /// Upstream `cancel`.
    pub fn cancel(&self) {
        self.detector.cancel();
    }

    /// Whether a press is pending.
    pub fn is_pending(&self) -> bool {
        self.detector.is_pending()
    }

    /// The touch handlers on an element: wraps `child`, whose fingers start, move and end
    /// the press. Once the press fired, the fingers are claimed: their lift taps nothing.
    pub fn area(&self, child: impl IntoElement) -> LongPressArea {
        LongPressArea {
            handlers: self.clone(),
            child: child.into_any_element(),
        }
    }

    /// The touch handlers as a child of the element they listen on (absolute, its full
    /// size): for an element that must stay a `Div`.
    pub(crate) fn probe(&self) -> gpui::Canvas<Hitbox> {
        let handlers = self.clone();
        gpui::canvas(
            |bounds, window, _| window.insert_hitbox(bounds, HitboxBehavior::Normal),
            move |_, hitbox, window, _| {
                window.on_touch_event(move |event, window, cx| {
                    let inside = event.phase == TouchPhase::Started
                        && hitbox.is_hovered_at(event.position, window);
                    handlers.touch(event, inside, window, cx)
                });
            },
        )
    }

    fn touches(&self) -> Vec<Point> {
        self.fingers.borrow().iter().map(|(_, p)| *p).collect()
    }

    fn touch(&self, event: &TouchEvent, inside: bool, window: &mut Window, cx: &mut App) -> bool {
        let point = Point::new(
            f64::from(f32::from(event.position.x)),
            f64::from(f32::from(event.position.y)),
        );
        let ours = self.fingers.borrow().iter().any(|(id, _)| *id == event.id);
        match event.phase {
            TouchPhase::Started => {
                if !inside && self.fingers.borrow().is_empty() {
                    return false;
                }
                if self.fingers.borrow().is_empty() {
                    self.claimed.set(false);
                }
                self.fingers.borrow_mut().push((event.id, point));
                self.on_touch_start(&LongPressTouch::new(self.touches(), None), window, cx);
            }
            TouchPhase::Moved if ours => {
                for finger in self.fingers.borrow_mut().iter_mut() {
                    if finger.0 == event.id {
                        finger.1 = point;
                    }
                }
                self.on_touch_move(&LongPressTouch::new(self.touches(), None), window, cx);
            }
            TouchPhase::Ended | TouchPhase::Cancelled if ours => {
                self.fingers.borrow_mut().retain(|(id, _)| *id != event.id);
                self.on_touch_end(None, window, cx);
            }
            _ => return false,
        }
        self.claimed.get()
    }
}

fn flush_fired(
    fired: &Rc<RefCell<Vec<Point>>>,
    listener: &Rc<RefCell<Option<LongPressListener>>>,
    window: &mut Window,
    cx: &mut App,
) {
    let points: Vec<Point> = fired.borrow_mut().drain(..).collect();
    let Some(listener) = listener.borrow().clone() else {
        return;
    };
    for point in points {
        listener(point, window, cx);
    }
}

/// The detector's options for the hook's: a fire queues its point for the window.
fn detector_options(
    options: &LongPressOptions,
    fired: &Rc<RefCell<Vec<Point>>>,
) -> DetectorOptions {
    let fired = fired.clone();
    DetectorOptions {
        on_long_press: Rc::new(move |at| fired.borrow_mut().push(at)),
        disabled: options.disabled,
        delay_ms: options.delay_ms,
        move_cancel_px: options.move_cancel_px,
    }
}

/// Upstream `useLongPress`: a long press on the element `id` names. The options of the
/// latest frame are the ones read; the pending press ends with the element.
///
/// ```ignore
/// let hold = use_long_press("row", LongPressOptions::new(|at, window, cx| open_menu(at, window, cx)), window, cx);
/// hold.area(div().id("row").child("Hold me"))
/// ```
pub fn use_long_press(
    id: impl Into<ElementId>,
    options: LongPressOptions,
    window: &mut Window,
    cx: &mut App,
) -> LongPressHandlers {
    let id = id.into();
    let initial = options.clone();
    let state = window.use_keyed_state(
        ElementId::Name(format!("{id}-long-press").into()),
        cx,
        move |window, cx| {
            let fired: Rc<RefCell<Vec<Point>>> = Rc::default();
            let listener: Rc<RefCell<Option<LongPressListener>>> = Rc::default();
            let flush = {
                let (fired, listener) = (fired.clone(), listener.clone());
                Rc::new(move |window: &mut Window, cx: &mut App| {
                    flush_fired(&fired, &listener, window, cx)
                })
            };
            let clock = WindowClock::new(window, cx, flush);
            let claimed: Rc<Cell<bool>> = Rc::default();
            let fingers: Rc<RefCell<Vec<(TouchId, Point)>>> = Rc::default();
            // A claim while this area's finger is down claims that finger.
            let claim_watch = {
                let (claimed, fingers) = (claimed.clone(), fingers.clone());
                watch_press_claims(move || {
                    if !fingers.borrow().is_empty() {
                        claimed.set(true);
                    }
                })
            };
            HookState {
                detector: LongPressDetector::new(detector_options(&initial, &fired), clock),
                fired,
                listener,
                fingers,
                claimed,
                claim_watch,
            }
        },
    );
    state.update(cx, |s, _| {
        s.detector.set_options(detector_options(&options, &s.fired));
        *s.listener.borrow_mut() = Some(options.on_long_press.clone());
        LongPressHandlers {
            detector: s.detector.clone(),
            fired: s.fired.clone(),
            listener: s.listener.clone(),
            fingers: s.fingers.clone(),
            claimed: s.claimed.clone(),
        }
    })
}

/// The element [`LongPressHandlers::area`] returns: it lays out as its child and hears
/// the fingers that land on it.
pub struct LongPressArea {
    handlers: LongPressHandlers,
    child: AnyElement,
}

impl Element for LongPressArea {
    type RequestLayoutState = ();
    type PrepaintState = Hitbox;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        (self.child.request_layout(window, cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _state: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) -> Hitbox {
        let hitbox = window.insert_hitbox(bounds, HitboxBehavior::Normal);
        self.child.prepaint(window, cx);
        hitbox
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _state: &mut (),
        hitbox: &mut Hitbox,
        window: &mut Window,
        cx: &mut App,
    ) {
        let handlers = self.handlers.clone();
        let hitbox = hitbox.clone();
        window.on_touch_event(move |event, window, cx| {
            let inside =
                event.phase == TouchPhase::Started && hitbox.is_hovered_at(event.position, window);
            handlers.touch(event, inside, window, cx)
        });
        self.child.paint(window, cx);
    }
}
