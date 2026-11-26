//! The GPUI binding, upstream `gestures/GestureHost.ts`: a window's host feeds
//! the arena GPUI's raw input for the gesture areas the window paints, and
//! carries out what the arena decides.
//!
//! **Hosting recognizers.** A component creates its recognizers (and a press
//! tracker, if it has one) once, attaches them with [`GestureHost::attach`],
//! and wraps its element in [`gesture_area`] with the attachment. The area
//! inserts a hitbox and registers the host's window listeners each frame.
//! Recognizer listeners run inside the input, synchronously; one that needs
//! the window or the app schedules the work with [`after_gesture`], which the
//! host runs before the input returns. The host refreshes the window after
//! any transition.
//!
//! **Input.** Each area registers window-level listeners (so a gesture keeps
//! its pointer wherever it goes: GPUI has no `setPointerCapture`, and the
//! listener of an area under the press is a capture while the press lasts);
//! the first listener of a dispatch handles it for every area:
//!
//! - **Mouse** (`MouseDown`, `MouseMove`, `MouseUp`): a contact of kind mouse. The
//!   right button (and a ctrl-click on macOS) is a context click and starts
//!   no press. A click the arena suppresses is kept from the elements under
//!   the release (propagation stops in the capture phase); a deferred one is
//!   replayed as a press and release at its place if it is allowed.
//! - **Touch** (fork patch R23's raw touch listener, `Window::on_touch_event`):
//!   every finger's `gpui::TouchEvent`, with its own id and force, reaches
//!   the host before GPUI's own recognizer (`gpui::gestures`, which turns
//!   touches into taps, pans, long presses and touch drags). So a touch is
//!   never handled twice, the host claims each touch that lands on an area
//!   with a participant that takes a finger: GPUI's recognizer never sees
//!   it, and the arena alone decides it, every finger at once (a two-finger
//!   pinch or pan). The host then does what the platform would have done for
//!   whatever no recognizer takes: a tap the arena allows is replayed to
//!   GPUI as a touch of its own (a `TouchEvent` pair the host never claims),
//!   so its click, click count and touch modality are GPUI's; a lone finger
//!   nobody claims that travels past the press slop along an axis its areas
//!   leave to the window becomes the window's scroll, emitted as touch
//!   `ScrollEvent`s with UIKit's deceleration after the lift (the arena hears
//!   `browser_took_over`). A touch that lands elsewhere stays GPUI's; the
//!   host follows it (the finger record, the finger's scroll, the scroll
//!   brake), and a page hold claims one whose pan would scroll the page.
//! - **Wheel** (`ScrollEvent` from a pointer): the wheel segmenter splits a
//!   trackpad's stream into gestures and reads its lift where the platform
//!   reports no phase; a pan that allows continuous scrolling follows a
//!   trackpad, and a ctrl-wheel drives a pinch.
//! - **Pinch** (`PinchEvent`, a trackpad pinch): a pinch's two contacts.
//!
//! Only `gpui::TouchEvent` input reaches the touch path, whatever platform
//! delivers it.

use std::any::TypeId;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::{Rc, Weak};
use std::time::Duration;

use gpui::{
    AnyElement, App, AsyncWindowContext, BackgroundExecutor, Bounds, DispatchPhase, Element,
    ElementId, FocusHandle, ForegroundExecutor, GlobalElementId, Hitbox, HitboxBehavior,
    InspectorElementId, IntoElement, KeyDownEvent, LayoutId, Modifiers, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, PinchEvent, Pixels, PlatformInput, ScrollDelta,
    ScrollEvent, ScrollSource, Task, TouchEvent, TouchId, Window, WindowId, px,
};
use web_time::Instant;

use super::arena::{
    ArenaBeganInput, ArenaOptions, ClickDisposition, GestureArena, GestureClock, TimerHandle,
};
use super::geometry::distance;
use super::host_registry::GestureHosts;
use super::native_scroll::{NativeScroller, ScrollerRegistry};
use super::recognizer::{AnyRecognizer, RecognizerFamily};
use super::touch_action::combined_native_behavior;
use super::types::{
    GestureEnvironment, GestureState, GestureTouch, Point, PointerKind, ScrollType,
    TouchBeganInput, TouchConsumer, TouchInput, ViewKey, ViewRect,
};
use super::velocity_tracker::VelocityTracker;
use super::wheel_segmenter::WheelSegmenter;
use crate::motion::uikit::gestures::press as PRESS;
use crate::motion::uikit::scroll::DECELERATION_NORMAL;

/// How long after its last event a pinch or a mouse wheel's notches end, ms.
const WHEEL_END_MS: f64 = 100.;
/// How long after its last event a vertical trackpad scroll ends, ms.
const WHEEL_FOLLOW_END_V_MS: f64 = 180.;
/// The same horizontally, ms.
const WHEEL_FOLLOW_END_MS: f64 = 220.;
/// A line-mode wheel's line, px.
const WHEEL_LINE_PX: f64 = 40.;
/// The span a ctrl-wheel pinch's two contacts start at, px.
const WHEEL_PINCH_SPAN: f64 = 100.;
/// A page hold judges a finger's pan past this travel, px.
const PAGE_HOLD_SLOP: f64 = 5.;
/// The slowest release that coasts, px/s (GPUI's own).
const MIN_FLING_VELOCITY: f64 = 50.;
/// Coasting below this speed is imperceptible, px/s.
const MOMENTUM_STOP_VELOCITY: f64 = 10.;
/// Where the host's own contact ids (a mouse's, a wheel's) start, far from
/// the platform's touch ids, which the arena takes as they are.
const CONTACT_IDS: u64 = 1 << 62;
/// A frame of the window's emulated momentum, ms.
const FRAME_MS: f64 = 1000. / 60.;

/// Work a listener hands the host for the window and the app.
type WindowJob = Box<dyn FnOnce(&mut Window, &mut App)>;
/// A listener with no arguments.
type Callback = Rc<dyn Fn()>;
/// A selection lock's undo, by contact.
type Unlock = (u64, Box<dyn FnOnce()>);

thread_local! {
    static AFTER: RefCell<Vec<WindowJob>> = const { RefCell::new(Vec::new()) };
}

/// Schedules work that needs the window and the app from a recognizer
/// listener or a consumer callback: the host runs it before the input (or
/// the timer) that called the listener returns.
pub fn after_gesture(f: impl FnOnce(&mut Window, &mut App) + 'static) {
    AFTER.with(|after| after.borrow_mut().push(Box::new(f)));
}

fn drain_after(window: &mut Window, cx: &mut App) {
    loop {
        let jobs: Vec<_> = AFTER.with(|after| after.borrow_mut().drain(..).collect());
        if jobs.is_empty() {
            break;
        }
        for job in jobs {
            job(window, cx);
        }
    }
}

fn to_point(p: gpui::Point<Pixels>) -> Point {
    Point::new(f64::from(f32::from(p.x)), f64::from(f32::from(p.y)))
}

fn to_pixels(p: Point) -> gpui::Point<Pixels> {
    gpui::point(px(p.x as f32), px(p.y as f32))
}

// --- The clock -------------------------------------------------------------

/// The window's event clock: the executor's time (fake under the test
/// platform), with timers on the window's foreground tasks. A timer runs in
/// the window's context and lets the host finish its work after it.
pub(crate) struct GpuiClock {
    executor: BackgroundExecutor,
    foreground: ForegroundExecutor,
    origin: Instant,
    window: AsyncWindowContext,
    tasks: Rc<RefCell<HashMap<TimerHandle, Task<()>>>>,
    next: Cell<TimerHandle>,
    host: RefCell<Weak<HostInner>>,
}

impl GestureClock for GpuiClock {
    fn now(&self) -> f64 {
        self.executor
            .now()
            .duration_since(self.origin)
            .as_secs_f64()
            * 1000.
    }

    fn set_timeout(&self, callback: Box<dyn FnOnce()>, ms: f64) -> TimerHandle {
        let handle = self.next.get() + 1;
        self.next.set(handle);
        let timer = self
            .executor
            .timer(Duration::from_secs_f64(ms.max(0.) / 1000.));
        let mut window = self.window.clone();
        let host = self.host.borrow().clone();
        let tasks = Rc::downgrade(&self.tasks);
        let task = self.foreground.spawn(async move {
            timer.await;
            if let Some(tasks) = tasks.upgrade() {
                tasks.borrow_mut().remove(&handle);
            }
            let _ = window.update(|window, cx| {
                callback();
                if let Some(host) = host.upgrade() {
                    GestureHost(host).finish(window, cx);
                }
            });
        });
        self.tasks.borrow_mut().insert(handle, task);
        handle
    }

    fn clear_timeout(&self, handle: TimerHandle) {
        self.tasks.borrow_mut().remove(&handle);
    }
}

// --- Attachments -----------------------------------------------------------

/// A pointer context click or a context-menu key, for an attachment that
/// answers context requests ([`super::context_request`]).
pub(crate) trait ContextHandler {
    fn pointer(&self, point: Point, pointer: PointerKind, time: f64) -> bool;
    fn key(&self, repeat: bool, time: f64) -> bool;
    fn is_enabled(&self) -> bool;
}

pub(crate) struct AttachmentInner {
    key: ViewKey,
    host: Weak<HostInner>,
    recognizers: RefCell<Vec<AnyRecognizer>>,
    consumer: RefCell<Option<Rc<dyn TouchConsumer>>>,
    bounds: Cell<Option<Bounds<Pixels>>>,
    rtl: Cell<bool>,
    focus: RefCell<Option<FocusHandle>>,
    pub(crate) context: RefCell<Option<Rc<dyn ContextHandler>>>,
    detached: Cell<bool>,
}

impl Drop for AttachmentInner {
    fn drop(&mut self) {
        if !self.detached.get() {
            detach_recognizers(&self.recognizers.borrow());
        }
    }
}

fn detach_recognizers(recognizers: &[AnyRecognizer]) {
    // Detaching ends a gesture in flight, as unmounting does upstream; the
    // listener hears nothing after it.
    for recognizer in recognizers {
        if recognizer.state() != GestureState::Possible || recognizer.is_involved() {
            recognizer.cancel();
        }
        recognizer.set_view(None);
        recognizer.set_environment(None);
    }
}

/// Recognizers (and a view-level consumer) attached to one gesture area:
/// upstream's `host.attach(element, recognizer)`. Dropping the last handle,
/// or [`GestureAttachment::detach`], detaches them.
#[derive(Clone)]
pub struct GestureAttachment(Rc<AttachmentInner>);

impl GestureAttachment {
    /// The area's key: the view its recognizers report.
    pub fn key(&self) -> ViewKey {
        self.0.key
    }

    /// The recognizers attached.
    pub fn recognizers(&self) -> Vec<AnyRecognizer> {
        self.0.recognizers.borrow().clone()
    }

    /// Attaches another recognizer.
    pub fn add_recognizer(&self, recognizer: &AnyRecognizer) {
        if let Some(host) = self.0.host.upgrade() {
            recognizer.set_view(Some(self.0.key));
            recognizer.set_environment(Some(host.environment.clone()));
        }
        let mut list = self.0.recognizers.borrow_mut();
        if !list.contains(recognizer) {
            list.push(recognizer.clone());
        }
    }

    /// Detaches one recognizer, ending its gesture in flight.
    pub fn remove_recognizer(&self, recognizer: &AnyRecognizer) {
        let removed = {
            let mut list = self.0.recognizers.borrow_mut();
            let before = list.len();
            list.retain(|r| r != recognizer);
            before != list.len()
        };
        if removed {
            detach_recognizers(std::slice::from_ref(recognizer));
        }
    }

    /// The view-level consumer (a press tracker's).
    pub fn set_consumer(&self, consumer: Option<Rc<dyn TouchConsumer>>) {
        *self.0.consumer.borrow_mut() = consumer;
    }

    pub fn consumer(&self) -> Option<Rc<dyn TouchConsumer>> {
        self.0.consumer.borrow().clone()
    }

    /// The writing direction of the area (leading and trailing edges).
    pub fn set_rtl(&self, rtl: bool) {
        self.0.rtl.set(rtl);
    }

    /// The focus a context-menu key needs to reach this area.
    pub fn set_focus(&self, focus: Option<FocusHandle>) {
        *self.0.focus.borrow_mut() = focus;
    }

    /// The area's bounds as last laid out.
    pub fn bounds(&self) -> Option<Bounds<Pixels>> {
        self.0.bounds.get()
    }

    /// The area's bounds as a press tracker reads them.
    pub fn press_bounds(&self) -> Option<super::press_tracker::PressBounds> {
        self.0
            .bounds
            .get()
            .map(|b| super::press_tracker::PressBounds {
                left: f64::from(f32::from(b.left())),
                top: f64::from(f32::from(b.top())),
                right: f64::from(f32::from(b.right())),
                bottom: f64::from(f32::from(b.bottom())),
            })
    }

    /// A `get_bounds` for [`super::PressTrackerOptions`] that reads this area.
    pub fn bounds_reader(
        &self,
    ) -> impl Fn() -> Option<super::press_tracker::PressBounds> + 'static {
        let weak = Rc::downgrade(&self.0);
        move || {
            weak.upgrade()
                .and_then(|inner| GestureAttachment(inner).press_bounds())
        }
    }

    /// The host after a consumer's `is_enabled` or a recognizer's options
    /// changed (upstream `host.update(element)`): nothing to re-sync, since
    /// the listeners are registered each frame; kept for parity.
    pub fn update(&self) {}

    /// Detaches every recognizer and the consumer.
    pub fn detach(&self) {
        if self.0.detached.replace(true) {
            return;
        }
        let recognizers = std::mem::take(&mut *self.0.recognizers.borrow_mut());
        detach_recognizers(&recognizers);
        *self.0.consumer.borrow_mut() = None;
        *self.0.context.borrow_mut() = None;
    }

    pub(crate) fn inner(&self) -> &Rc<AttachmentInner> {
        &self.0
    }

    /// Whether anything attached takes a contact of this kind now.
    fn takes(&self, kind: PointerKind) -> bool {
        self.0
            .recognizers
            .borrow()
            .iter()
            .any(|r| r.could_take(kind))
            || self
                .0
                .consumer
                .borrow()
                .as_ref()
                .is_some_and(|c| c.is_enabled())
            || self
                .0
                .context
                .borrow()
                .as_ref()
                .is_some_and(|c| c.is_enabled())
    }
}

// --- The element -------------------------------------------------------------

/// The element that hosts an attachment's recognizers: it lays out as its
/// child, inserts a hitbox over it and registers the window's listeners.
pub struct GestureArea {
    attachment: GestureAttachment,
    child: AnyElement,
}

/// Wraps `child` in a gesture area for `attachment`.
pub fn gesture_area(attachment: &GestureAttachment, child: impl IntoElement) -> GestureArea {
    GestureArea {
        attachment: attachment.clone(),
        child: child.into_any_element(),
    }
}

impl Element for GestureArea {
    type RequestLayoutState = ();
    type PrepaintState = Option<Hitbox>;

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
    ) -> Option<Hitbox> {
        // The area's hitbox goes in before its content's, so an inner area
        // registers after this one: the host reads that order as nesting.
        let hitbox = window.insert_hitbox(bounds, HitboxBehavior::Normal);
        self.attachment.0.bounds.set(Some(bounds));
        if let Some(host) = self.attachment.0.host.upgrade() {
            GestureHost(host).register_area(&self.attachment, hitbox.clone());
        }
        self.child.prepaint(window, cx);
        Some(hitbox)
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _state: &mut (),
        _hitbox: &mut Option<Hitbox>,
        window: &mut Window,
        cx: &mut App,
    ) {
        if let Some(host) = self.attachment.0.host.upgrade() {
            GestureHost(host).listen(window);
        }
        self.child.paint(window, cx);
    }
}

// --- The host ----------------------------------------------------------------

struct AreaEntry {
    key: ViewKey,
    attachment: Weak<AttachmentInner>,
    hitbox: Hitbox,
    seq: u64,
}

struct HostEnvironment {
    viewport: Cell<(f64, f64)>,
    host: RefCell<Weak<HostInner>>,
}

impl GestureEnvironment for HostEnvironment {
    fn viewport_size(&self) -> (f64, f64) {
        self.viewport.get()
    }

    fn is_rtl(&self, view: Option<ViewKey>) -> bool {
        self.attachment(view).is_some_and(|a| a.rtl.get())
    }

    fn view_rect(&self, view: Option<ViewKey>) -> Option<ViewRect> {
        let bounds = self.attachment(view)?.bounds.get()?;
        Some(ViewRect {
            left: f64::from(f32::from(bounds.left())),
            top: f64::from(f32::from(bounds.top())),
            width: f64::from(f32::from(bounds.size.width)),
            height: f64::from(f32::from(bounds.size.height)),
        })
    }
}

impl HostEnvironment {
    fn attachment(&self, view: Option<ViewKey>) -> Option<Rc<AttachmentInner>> {
        let view = view?;
        let host = self.host.borrow().upgrade()?;
        host.areas
            .borrow()
            .iter()
            .find(|entry| entry.key == view)
            .and_then(|entry| entry.attachment.upgrade())
    }
}

/// The window's own pan of a finger nobody claimed: the scroll the host
/// emits for it.
struct WindowPan {
    horizontal: bool,
    held: bool,
    tracker: VelocityTracker,
}

struct TouchContact {
    id: u64,
    start: Point,
    last: Point,
    origin: Option<ViewKey>,
    allows_x: bool,
    allows_y: bool,
    window_pan: Option<WindowPan>,
}

struct MouseContact {
    id: u64,
    position: Point,
}

/// A finger GPUI kept (it landed where no area takes it), followed by its
/// raw touches.
struct NativeFinger {
    id: u64,
    landed: Point,
    origin: Option<ViewKey>,
    /// A page hold's verdict on its pan, once judged.
    verdict: Option<bool>,
}

#[derive(Clone, Copy)]
enum ReplayKind {
    Mouse,
    Touch,
}

struct PendingClick {
    id: u64,
    position: Point,
    kind: ReplayKind,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum WheelKind {
    Pan,
    Pinch,
}

struct WheelFollow {
    lifted: bool,
    held_x: f64,
    held_y: f64,
    last_move: f64,
    true_x: f64,
    true_y: f64,
    base_x: f64,
    base_y: f64,
    speed_x: f64,
    speed_y: f64,
    read_from_x: f64,
    read_from_y: f64,
    read_span: f64,
    prior_speed_x: f64,
    prior_speed_y: f64,
}

struct WheelGesture {
    kind: WheelKind,
    ids: Vec<u64>,
    x: f64,
    y: f64,
    span: f64,
    last: f64,
    end_ms: f64,
    timer: Option<TimerHandle>,
    takers: Vec<AnyRecognizer>,
    follow: Option<WheelFollow>,
}

/// A scroll lock's decision: whether a one-finger pan from `origin` (the
/// innermost gesture area under the finger) by (dx, dy) would scroll the
/// page.
pub type PageScrollDecision = Rc<dyn Fn(Option<ViewKey>, f64, f64) -> bool>;

#[derive(Default)]
pub(crate) struct FingerWatch {
    /// Watched scroller keys, and how many watchers each has.
    pub scrollers: Vec<(ViewKey, usize)>,
    pub finger_scrollers: Vec<ViewKey>,
    pub ended_while_down: Vec<ViewKey>,
    pub end_listeners: Vec<(ViewKey, u64, Callback)>,
    pub landings: Vec<(ViewKey, f64)>,
    pub starts: Vec<(ViewKey, f64)>,
    pub last_spans: Vec<(ViewKey, f64, f64)>,
    pub fallback: Option<TimerHandle>,
}

pub(crate) struct HostInner {
    arena: GestureArena,
    clock: Rc<GpuiClock>,
    environment: Rc<HostEnvironment>,
    areas: RefCell<Vec<AreaEntry>>,
    scrollers: ScrollerRegistry,
    next_seq: Cell<u64>,
    next_id: Cell<u64>,
    dispatch: Cell<Option<(TypeId, usize)>>,
    dispatch_reset: Cell<bool>,
    replaying: Cell<u32>,
    emitting: Cell<u32>,
    dirty: Cell<bool>,
    active: Cell<Option<bool>>,
    mouse: RefCell<Option<MouseContact>>,
    touches: RefCell<Vec<TouchContact>>,
    natives: RefCell<Vec<NativeFinger>>,
    next_contact: Cell<u64>,
    pending_clicks: RefCell<Vec<PendingClick>>,
    replays: RefCell<Vec<(Point, ReplayKind)>>,
    locks: RefCell<Vec<Unlock>>,
    momentum: RefCell<Option<Task<()>>>,
    momentum_origin: Cell<Option<Point>>,
    wheel: RefCell<Option<WheelGesture>>,
    segmenter: RefCell<WheelSegmenter>,
    pointer: Cell<Option<PointerKind>>,
    finger_listeners: RefCell<Vec<(u64, Callback)>>,
    next_listener: Cell<u64>,
    last_touch: Cell<Option<(Point, f64)>>,
    last_scroll: Cell<Option<(Option<Bounds<Pixels>>, f64)>>,
    holds: RefCell<Vec<(u64, PageScrollDecision)>>,
    pub(crate) finger_watch: RefCell<FingerWatch>,
}

/// A window's gesture host (upstream `GestureHost`). Cloning shares it.
#[derive(Clone)]
pub struct GestureHost(Rc<HostInner>);

/// The window's host, created on first use (upstream `getGestureHost`).
pub fn gesture_host(window: &mut Window, cx: &mut App) -> GestureHost {
    let id = window.window_handle().window_id();
    if let Some(host) = super::host_registry::existing_gesture_host(id, cx) {
        return host;
    }
    let host = GestureHost::new(window, cx);
    cx.default_global::<GestureHosts>()
        .0
        .insert(id, host.clone());
    host
}

/// The pointer kind the window's host saw last, if it has seen one.
pub fn last_pointer_kind(window: WindowId, cx: &App) -> Option<PointerKind> {
    super::host_registry::existing_gesture_host(window, cx).and_then(|host| host.0.pointer.get())
}

impl GestureHost {
    fn new(window: &mut Window, cx: &mut App) -> Self {
        let clock = Rc::new(GpuiClock {
            executor: cx.background_executor().clone(),
            foreground: cx.foreground_executor().clone(),
            origin: cx.background_executor().now(),
            window: window.to_async(cx),
            tasks: Rc::new(RefCell::new(HashMap::new())),
            next: Cell::new(0),
            host: RefCell::new(Weak::new()),
        });
        let environment = Rc::new(HostEnvironment {
            viewport: Cell::new((0., 0.)),
            host: RefCell::new(Weak::new()),
        });
        let arena = GestureArena::new(ArenaOptions {
            clock: Some(clock.clone()),
            environment: Some(environment.clone()),
            callbacks: Default::default(),
        });
        let inner = Rc::new(HostInner {
            arena: arena.clone(),
            clock: clock.clone(),
            environment: environment.clone(),
            areas: RefCell::new(Vec::new()),
            scrollers: ScrollerRegistry::default(),
            next_seq: Cell::new(0),
            next_id: Cell::new(0),
            dispatch: Cell::new(None),
            dispatch_reset: Cell::new(false),
            replaying: Cell::new(0),
            emitting: Cell::new(0),
            dirty: Cell::new(false),
            active: Cell::new(None),
            mouse: RefCell::new(None),
            touches: RefCell::new(Vec::new()),
            natives: RefCell::new(Vec::new()),
            next_contact: Cell::new(CONTACT_IDS),
            pending_clicks: RefCell::new(Vec::new()),
            replays: RefCell::new(Vec::new()),
            locks: RefCell::new(Vec::new()),
            momentum: RefCell::new(None),
            momentum_origin: Cell::new(None),
            wheel: RefCell::new(None),
            segmenter: RefCell::new(WheelSegmenter::new()),
            pointer: Cell::new(None),
            finger_listeners: RefCell::new(Vec::new()),
            next_listener: Cell::new(0),
            last_touch: Cell::new(None),
            last_scroll: Cell::new(None),
            holds: RefCell::new(Vec::new()),
            finger_watch: RefCell::new(FingerWatch::default()),
        });
        *clock.host.borrow_mut() = Rc::downgrade(&inner);
        *environment.host.borrow_mut() = Rc::downgrade(&inner);
        let weak = Rc::downgrade(&inner);
        {
            let mut callbacks = arena.callbacks();
            let on_deferred = weak.clone();
            callbacks.on_deferred_click = Some(Rc::new(move |touch: &GestureTouch, allowed| {
                if let Some(host) = on_deferred.upgrade() {
                    GestureHost(host).deferred_click(touch.id(), allowed);
                }
            }));
            let on_transition = weak.clone();
            callbacks.on_transition = Some(Rc::new(move |_r: &AnyRecognizer, _from, _to| {
                if let Some(host) = on_transition.upgrade() {
                    host.dirty.set(true);
                }
            }));
        }
        let host = Self(inner);
        host.sync_viewport(window);
        host
    }

    /// The arena deciding this window's gestures.
    pub fn arena(&self) -> GestureArena {
        self.0.arena.clone()
    }

    /// The window's event clock, for press trackers and timers of its own.
    pub fn clock(&self) -> Rc<dyn GestureClock> {
        self.0.clock.clone()
    }

    /// The event clock's time, ms.
    pub fn now(&self) -> f64 {
        self.0.arena.now()
    }

    /// The window's registered native scrollers.
    pub fn scrollers(&self) -> ScrollerRegistry {
        self.0.scrollers.clone()
    }

    /// Registers a native scroller for the current frame (from prepaint).
    pub fn register_native_scroller(&self, scroller: NativeScroller) {
        self.0.scrollers.register(scroller);
    }

    /// Attaches recognizers (upstream `attach`): they report this area as
    /// their view, and the window's facts through the host's environment.
    pub fn attach(&self, recognizers: &[&AnyRecognizer]) -> GestureAttachment {
        let key = self.next_id();
        let attachment = GestureAttachment(Rc::new(AttachmentInner {
            key,
            host: Rc::downgrade(&self.0),
            recognizers: RefCell::new(Vec::new()),
            consumer: RefCell::new(None),
            bounds: Cell::new(None),
            rtl: Cell::new(false),
            focus: RefCell::new(None),
            context: RefCell::new(None),
            detached: Cell::new(false),
        }));
        for recognizer in recognizers {
            attachment.add_recognizer(recognizer);
        }
        attachment
    }

    /// Attaches a view-level consumer (upstream `attachConsumer`).
    pub fn attach_consumer(&self, consumer: Rc<dyn TouchConsumer>) -> GestureAttachment {
        let attachment = self.attach(&[]);
        attachment.set_consumer(Some(consumer));
        attachment
    }

    /// How many fingers are on the screen, as far as the host heard.
    pub fn fingers_down(&self) -> usize {
        self.0.touches.borrow().len() + self.0.natives.borrow().len()
    }

    /// Whether a recognizer owns this touch.
    pub fn is_touch_claimed(&self, id: u64) -> bool {
        self.0.arena.is_claimed(id)
    }

    /// The pointer kind the host saw last.
    pub fn last_pointer(&self) -> Option<PointerKind> {
        self.0.pointer.get()
    }

    /// Whether a finger that landed inside the area or scroller with this
    /// key is down now.
    pub fn is_finger_down_in(&self, key: ViewKey) -> bool {
        let Some(bounds) = self.bounds_of(key) else {
            return false;
        };
        let inside = |p: Point| bounds.contains(&to_pixels(p));
        self.0.touches.borrow().iter().any(|t| inside(t.start))
            || self.0.natives.borrow().iter().any(|f| inside(f.landed))
    }

    /// Whether a pen went down inside `key` and is down now. GPUI reports no
    /// pen, so none is.
    pub fn is_pen_down_in(&self, _key: ViewKey) -> bool {
        false
    }

    /// When a pen hold inside `key` last ended; never, without pens.
    pub fn last_pen_lift_in(&self, _key: ViewKey) -> f64 {
        f64::NEG_INFINITY
    }

    /// The platform pointer id of a contact; GPUI's touch drags carry none,
    /// so it is the arena's own id.
    pub fn pointer_id_of(&self, touch_id: u64) -> Option<u64> {
        self.0.arena.get_touch(touch_id).map(|t| t.id())
    }

    /// Whether a press consumer that keeps its press sits on the path from
    /// `target` up to `boundary` (both areas, `target` innermost).
    pub fn keeps_press_between(&self, target: Option<ViewKey>, boundary: ViewKey) -> bool {
        let Some(target) = target else {
            return false;
        };
        let Some(target_bounds) = self.bounds_of(target) else {
            return false;
        };
        let boundary_seq = self.seq_of(boundary).unwrap_or(0);
        self.0.areas.borrow().iter().any(|entry| {
            entry.seq >= boundary_seq
                && entry.attachment.upgrade().is_some_and(|a| {
                    a.bounds
                        .get()
                        .is_some_and(|b| b.contains(&target_bounds.center()))
                        && a.consumer
                            .borrow()
                            .as_ref()
                            .is_some_and(|c| c.can_keep_touch() && c.is_enabled())
                })
        })
    }

    /// Whether a finger set a scroller containing `point` moving within the
    /// last `window_ms` (the scroll brake: a native list presses nothing
    /// for a touch that stops its flick).
    pub fn is_scroll_braking(&self, point: Point, window_ms: f64) -> bool {
        self.0.last_scroll.get().is_some_and(|(bounds, time)| {
            self.now() - time < window_ms && bounds.is_none_or(|b| b.contains(&to_pixels(point)))
        })
    }

    /// A press tracker's `is_braking` for this host, at UIKit's 150 ms.
    pub fn braking_reader(&self) -> Rc<dyn Fn(&GestureTouch) -> bool> {
        let weak = Rc::downgrade(&self.0);
        Rc::new(move |touch: &GestureTouch| {
            weak.upgrade().is_some_and(|host| {
                GestureHost(host).is_scroll_braking(touch.start(), PRESS::SCROLL_BRAKE)
            })
        })
    }

    /// Calls `listener` after each touch the host hears, once the finger
    /// record is up to date. Returns an id for [`Self::remove_fingers_listener`].
    pub fn on_fingers_change(&self, listener: impl Fn() + 'static) -> u64 {
        let id = self.0.next_listener.get() + 1;
        self.0.next_listener.set(id);
        self.0
            .finger_listeners
            .borrow_mut()
            .push((id, Rc::new(listener)));
        id
    }

    pub fn remove_fingers_listener(&self, id: u64) {
        self.0
            .finger_listeners
            .borrow_mut()
            .retain(|(l, _)| *l != id);
    }

    /// Holds the page still under a finger (upstream `holdPageScroll`):
    /// see [`super::page_hold`]. Returns the hold's id.
    pub(crate) fn add_page_hold(&self, decide: PageScrollDecision) -> u64 {
        let id = self.next_id();
        self.0.holds.borrow_mut().push((id, decide));
        id
    }

    pub(crate) fn remove_page_hold(&self, id: u64) {
        self.0.holds.borrow_mut().retain(|(h, _)| *h != id);
    }

    pub(crate) fn watch_state(&self) -> &RefCell<FingerWatch> {
        &self.0.finger_watch
    }

    pub(crate) fn next_listener_id(&self) -> u64 {
        let id = self.0.next_listener.get() + 1;
        self.0.next_listener.set(id);
        id
    }

    pub(crate) fn downgrade(&self) -> Weak<HostInner> {
        Rc::downgrade(&self.0)
    }

    pub(crate) fn from_weak(weak: &Weak<HostInner>) -> Option<Self> {
        weak.upgrade().map(Self)
    }

    fn next_contact(&self) -> u64 {
        let id = self.0.next_contact.get() + 1;
        self.0.next_contact.set(id);
        id
    }

    fn next_id(&self) -> u64 {
        let id = self.0.next_id.get() + 1;
        self.0.next_id.set(id);
        id
    }

    pub(crate) fn bounds_of(&self, key: ViewKey) -> Option<Bounds<Pixels>> {
        let area = self
            .0
            .areas
            .borrow()
            .iter()
            .find(|e| e.key == key)
            .and_then(|e| e.attachment.upgrade())
            .and_then(|a| a.bounds.get());
        area.or_else(|| self.0.scrollers.get(key).map(|s| s.bounds))
    }

    fn seq_of(&self, key: ViewKey) -> Option<u64> {
        self.0
            .areas
            .borrow()
            .iter()
            .find(|e| e.key == key)
            .map(|e| e.seq)
    }

    fn sync_viewport(&self, window: &Window) {
        let size = window.viewport_size();
        self.0.environment.viewport.set((
            f64::from(f32::from(size.width)),
            f64::from(f32::from(size.height)),
        ));
    }

    fn record_pointer(&self, kind: PointerKind) {
        self.0.pointer.set(Some(kind));
    }

    fn fingers_changed(&self) {
        let listeners: Vec<_> = self
            .0
            .finger_listeners
            .borrow()
            .iter()
            .map(|(_, l)| l.clone())
            .collect();
        for listener in listeners {
            listener();
        }
        super::finger_scroll::on_fingers(self);
    }

    pub(crate) fn register_area(&self, attachment: &GestureAttachment, hitbox: Hitbox) {
        let seq = self.0.next_seq.get() + 1;
        self.0.next_seq.set(seq);
        let mut areas = self.0.areas.borrow_mut();
        areas.retain(|e| e.key != attachment.0.key && e.attachment.strong_count() > 0);
        if attachment.0.detached.get() {
            return;
        }
        areas.push(AreaEntry {
            key: attachment.0.key,
            attachment: Rc::downgrade(&attachment.0),
            hitbox,
            seq,
        });
    }

    /// The areas under the pointer, innermost first.
    fn path(&self, window: &Window, scroll: bool) -> Vec<GestureAttachment> {
        let mut hits: Vec<(u64, GestureAttachment)> = self
            .0
            .areas
            .borrow()
            .iter()
            .filter(|e| {
                if scroll {
                    e.hitbox.should_handle_scroll(window)
                } else {
                    e.hitbox.is_hovered(window)
                }
            })
            .filter_map(|e| {
                e.attachment
                    .upgrade()
                    .map(|a| (e.seq, GestureAttachment(a)))
            })
            .filter(|(_, a)| !a.0.detached.get())
            .collect();
        hits.sort_by_key(|(seq, _)| std::cmp::Reverse(*seq));
        hits.into_iter().map(|(_, a)| a).collect()
    }

    /// The areas under a touch's position, innermost first (a touch moves
    /// no mouse, so its hit test is its own).
    fn path_at(&self, window: &Window, point: Point) -> Vec<GestureAttachment> {
        let at = to_pixels(point);
        let mut hits: Vec<(u64, GestureAttachment)> = self
            .0
            .areas
            .borrow()
            .iter()
            .filter(|e| e.hitbox.is_hovered_at(at, window))
            .filter_map(|e| {
                e.attachment
                    .upgrade()
                    .map(|a| (e.seq, GestureAttachment(a)))
            })
            .filter(|(_, a)| !a.0.detached.get())
            .collect();
        hits.sort_by_key(|(seq, _)| std::cmp::Reverse(*seq));
        hits.into_iter().map(|(_, a)| a).collect()
    }

    /// Whether this listener call is the first of its dispatch.
    fn begin_dispatch<E: 'static>(&self, event: &E, window: &Window, cx: &mut App) -> bool {
        let token = (TypeId::of::<E>(), event as *const E as usize);
        if self.0.dispatch.get() == Some(token) {
            return false;
        }
        self.0.dispatch.set(Some(token));
        if !self.0.dispatch_reset.replace(true) {
            let weak = self.downgrade();
            window.defer(cx, move |_, _| {
                if let Some(host) = weak.upgrade() {
                    host.dispatch.set(None);
                    host.dispatch_reset.set(false);
                }
            });
        }
        true
    }

    /// Registers the window's listeners for this frame (one area's share).
    fn listen(&self, window: &mut Window) {
        let host = self.clone();
        window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
            if phase == DispatchPhase::Capture && host.begin_dispatch(event, window, cx) {
                host.mouse_down(event, window, cx);
            }
        });
        let host = self.clone();
        window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
            if phase == DispatchPhase::Capture && host.begin_dispatch(event, window, cx) {
                host.mouse_move(event, window, cx);
            }
        });
        let host = self.clone();
        window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
            if phase == DispatchPhase::Capture && host.begin_dispatch(event, window, cx) {
                host.mouse_up(event, window, cx);
            }
        });
        let host = self.clone();
        // Every finger's raw touches (fork patch R23), before GPUI's own
        // recognizer: a touch the host claims is the arena's alone.
        window.on_touch_event(move |event: &TouchEvent, window, cx| {
            // A tap the host replays to GPUI is GPUI's: never claimed again.
            host.0.replaying.get() == 0
                && host.begin_dispatch(event, window, cx)
                && host.touch_event(event, window, cx)
        });
        let host = self.clone();
        window.on_mouse_event(move |event: &ScrollEvent, phase, window, cx| {
            if phase == DispatchPhase::Capture && host.begin_dispatch(event, window, cx) {
                host.scroll(event, window, cx);
            }
        });
        let host = self.clone();
        window.on_mouse_event(move |event: &PinchEvent, phase, window, cx| {
            if phase == DispatchPhase::Capture && host.begin_dispatch(event, window, cx) {
                host.pinch(event, window, cx);
            }
        });
        let host = self.clone();
        window.on_key_event(move |event: &KeyDownEvent, phase, window, cx| {
            if phase == DispatchPhase::Capture && host.begin_dispatch(event, window, cx) {
                host.key_down(event, window, cx);
            }
        });
    }

    /// Ends the host's part of an input: replays, deferred work, a refresh.
    pub(crate) fn finish(&self, window: &mut Window, cx: &mut App) {
        let replays: Vec<_> = self.0.replays.borrow_mut().drain(..).collect();
        for (position, kind) in replays {
            self.replay(position, kind, window, cx);
        }
        drain_after(window, cx);
        if self.0.dirty.replace(false) {
            window.refresh();
        }
    }

    fn deferred_click(&self, id: u64, allowed: bool) {
        let pending = {
            let mut clicks = self.0.pending_clicks.borrow_mut();
            let index = clicks.iter().position(|c| c.id == id);
            index.map(|i| clicks.remove(i))
        };
        if let Some(click) = pending
            && allowed
        {
            self.0
                .replays
                .borrow_mut()
                .push((click.position, click.kind));
        }
        self.0.dirty.set(true);
    }

    /// Gives a click the arena allowed back to GPUI: a touch's as a touch of
    /// its own (GPUI's recognizer makes it a tap, with its click count and
    /// the touch modality), a mouse's as a press and release.
    fn replay(&self, position: Point, kind: ReplayKind, window: &mut Window, cx: &mut App) {
        let weak = self.downgrade();
        let id = self.next_id();
        window.defer(cx, move |window, cx| {
            let Some(host) = weak.upgrade() else {
                return;
            };
            host.replaying.set(host.replaying.get() + 1);
            let at = to_pixels(position);
            match kind {
                ReplayKind::Touch => {
                    let touch = TouchId(u64::MAX - id);
                    for phase in [gpui::TouchPhase::Started, gpui::TouchPhase::Ended] {
                        window.dispatch_event(
                            PlatformInput::Touch(TouchEvent {
                                id: touch,
                                phase,
                                position: at,
                                predicted_position: None,
                                force: None,
                            }),
                            cx,
                        );
                    }
                }
                ReplayKind::Mouse => {
                    window.dispatch_event(
                        PlatformInput::MouseDown(MouseDownEvent {
                            button: MouseButton::Left,
                            position: at,
                            modifiers: Modifiers::default(),
                            click_count: 1,
                            first_mouse: false,
                        }),
                        cx,
                    );
                    window.dispatch_event(
                        PlatformInput::MouseUp(MouseUpEvent {
                            button: MouseButton::Left,
                            position: at,
                            modifiers: Modifiers::default(),
                            click_count: 1,
                        }),
                        cx,
                    );
                }
            }
            host.replaying.set(host.replaying.get() - 1);
        });
    }

    fn began_input(
        &self,
        id: u64,
        kind: PointerKind,
        point: Point,
        time: f64,
        path: &[GestureAttachment],
    ) -> ArenaBeganInput {
        let recognizers: Vec<AnyRecognizer> = path.iter().flat_map(|a| a.recognizers()).collect();
        let consumer = path
            .iter()
            .find_map(|a| a.consumer().filter(|c| c.is_enabled()));
        let mut input = TouchBeganInput::new(id, kind, point.x, point.y, time);
        input.target = path.first().map(GestureAttachment::key);
        input.scroll = Some(self.0.scrollers.probe(point, true));
        ArenaBeganInput {
            input,
            recognizers,
            consumer,
        }
    }

    /// The selection locks of the participants that would take this contact
    /// (FR41): a finger's only, under any pointer.
    fn lock_selection_for(&self, id: u64, input: &ArenaBeganInput) {
        if input.input.pointer_type == PointerKind::Mouse {
            return;
        }
        let mut undo = Vec::new();
        let target = input.input.target;
        for recognizer in &input.recognizers {
            if let (Some(lock), Some(view)) = (recognizer.selection_lock(), recognizer.view())
                && recognizer.is_enabled()
            {
                undo.push(lock(&[view], target));
            }
        }
        if let (Some(consumer), Some(view)) = (&input.consumer, target)
            && let Some(lock) = consumer.selection_lock()
        {
            undo.push(lock(&[view], target));
        }
        let mut locks = self.0.locks.borrow_mut();
        for release in undo {
            locks.push((id, release));
        }
    }

    fn unlock(&self, id: u64) {
        let released: Vec<_> = {
            let mut locks = self.0.locks.borrow_mut();
            let (gone, kept): (Vec<_>, Vec<_>) = locks.drain(..).partition(|(l, _)| *l == id);
            *locks = kept;
            gone
        };
        for (_, release) in released {
            release();
        }
    }

    /// Losing the window's focus cancels every contact in flight, as UIKit
    /// does when the app resigns active.
    fn check_active(&self, window: &Window) {
        let active = window.is_window_active();
        let was = self.0.active.replace(Some(active));
        if active || was != Some(true) {
            return;
        }
        let time = self.now();
        let mut ids = Vec::new();
        if let Some(mouse) = self.0.mouse.borrow_mut().take() {
            ids.push(mouse.id);
        }
        ids.extend(self.0.touches.borrow_mut().drain(..).map(|t| t.id));
        if !ids.is_empty() {
            self.0.arena.cancelled(&ids, time);
            for id in ids {
                self.unlock(id);
            }
        }
        self.end_wheel(Some(time));
    }

    // --- Mouse ---------------------------------------------------------------

    fn mouse_down(&self, event: &MouseDownEvent, window: &mut Window, cx: &mut App) {
        if self.0.replaying.get() > 0 {
            return;
        }
        if window.last_input_was_touch() {
            // GPUI's tap for a finger it kept (or one the host replayed).
            return;
        }
        self.sync_viewport(window);
        self.check_active(window);
        self.record_pointer(PointerKind::Mouse);
        let time = self.now();
        // A contact coming down ends a wheel gesture and catches the
        // window's coasting.
        self.end_wheel(Some(time));
        self.stop_momentum(window, cx);
        let point = to_point(event.position);
        let path = self.path(window, false);
        let context_click = event.button == MouseButton::Right
            || (cfg!(target_os = "macos")
                && event.button == MouseButton::Left
                && event.modifiers.control);
        if context_click {
            // A right click (a ctrl-click on macOS) is a context click and
            // starts no press; the innermost request answers it.
            for area in &path {
                let handler = area.0.context.borrow().clone();
                if let Some(handler) = handler
                    && handler.is_enabled()
                    && handler.pointer(point, PointerKind::Mouse, time)
                {
                    break;
                }
            }
            self.finish(window, cx);
            return;
        }
        if event.button != MouseButton::Left || self.0.mouse.borrow().is_some() {
            return;
        }
        let id = self.next_contact();
        let input = self.began_input(id, PointerKind::Mouse, point, time, &path);
        if input.recognizers.is_empty() && input.consumer.is_none() {
            self.0.arena.untracked_began(input.input);
            self.finish(window, cx);
            return;
        }
        *self.0.mouse.borrow_mut() = Some(MouseContact {
            id,
            position: point,
        });
        self.0.arena.began(vec![input]);
        self.0.dirty.set(true);
        self.finish(window, cx);
    }

    fn mouse_move(&self, event: &MouseMoveEvent, window: &mut Window, cx: &mut App) {
        if self.0.replaying.get() > 0 || window.last_input_was_touch() {
            return;
        }
        self.record_pointer(PointerKind::Mouse);
        let Some(id) = self.0.mouse.borrow().as_ref().map(|m| m.id) else {
            return;
        };
        let point = to_point(event.position);
        let time = self.now();
        if event.pressed_button != Some(MouseButton::Left) {
            // The release happened where no listener heard it.
            self.release_mouse(id, point, time, window, cx);
            return;
        }
        if let Some(mouse) = self.0.mouse.borrow_mut().as_mut() {
            mouse.position = point;
        }
        self.0
            .arena
            .moved(vec![TouchInput::new(id, point.x, point.y, time)]);
        if self.0.arena.is_claimed(id) {
            // A claimed drag owns the pointer: nothing under it hovers or
            // selects meanwhile (FR41).
            cx.stop_propagation();
        }
        self.finish(window, cx);
    }

    fn mouse_up(&self, event: &MouseUpEvent, window: &mut Window, cx: &mut App) {
        if self.0.replaying.get() > 0 || window.last_input_was_touch() {
            return;
        }
        if event.button != MouseButton::Left {
            return;
        }
        let Some(id) = self.0.mouse.borrow().as_ref().map(|m| m.id) else {
            return;
        };
        let disposition = self.release_mouse(id, to_point(event.position), self.now(), window, cx);
        if matches!(
            disposition,
            ClickDisposition::Suppress | ClickDisposition::Defer
        ) {
            // The click is the arena's: the elements under the release never
            // hear it (a deferred one is replayed if it is allowed).
            cx.stop_propagation();
        }
    }

    fn release_mouse(
        &self,
        id: u64,
        point: Point,
        time: f64,
        window: &mut Window,
        cx: &mut App,
    ) -> ClickDisposition {
        *self.0.mouse.borrow_mut() = None;
        let disposition = self
            .0
            .arena
            .ended(vec![TouchInput::new(id, point.x, point.y, time)])
            .get(&id)
            .copied()
            .unwrap_or(ClickDisposition::Allow);
        if disposition == ClickDisposition::Defer {
            self.0.pending_clicks.borrow_mut().push(PendingClick {
                id,
                position: point,
                kind: ReplayKind::Mouse,
            });
        }
        self.unlock(id);
        self.0.dirty.set(true);
        self.finish(window, cx);
        disposition
    }

    // --- Touch ---------------------------------------------------------------

    /// One finger's raw touch. Returns whether the host claims it from GPUI.
    fn touch_event(&self, event: &TouchEvent, window: &mut Window, cx: &mut App) -> bool {
        let time = self.now();
        let id = event.id.0;
        let point = to_point(event.position);
        let force = event.force.map(f64::from);
        match event.phase {
            gpui::TouchPhase::Started => {
                self.sync_viewport(window);
                self.check_active(window);
                self.record_pointer(PointerKind::Touch);
                self.end_wheel(Some(time));
                self.stop_momentum(window, cx);
                self.0.last_touch.set(Some((point, time)));
                let path = self.path_at(window, point);
                let takes = path.iter().any(|a| a.takes(PointerKind::Touch));
                if !takes {
                    // The touch stays GPUI's: the host only follows it.
                    self.0.natives.borrow_mut().push(NativeFinger {
                        id,
                        landed: point,
                        origin: path.first().map(GestureAttachment::key),
                        verdict: None,
                    });
                    let mut input =
                        TouchBeganInput::new(id, PointerKind::Touch, point.x, point.y, time);
                    input.target = path.first().map(GestureAttachment::key);
                    input.force = force;
                    self.0.arena.untracked_began(input);
                    self.fingers_changed();
                    self.finish(window, cx);
                    return false;
                }
                // The arena decides this touch: GPUI's recognizer never sees it.
                let mut input = self.began_input(id, PointerKind::Touch, point, time, &path);
                input.input.force = force;
                let behavior = combined_native_behavior(&input.recognizers);
                self.0.touches.borrow_mut().push(TouchContact {
                    id,
                    start: point,
                    last: point,
                    origin: input.input.target,
                    allows_x: behavior.pan_x,
                    allows_y: behavior.pan_y,
                    window_pan: None,
                });
                self.lock_selection_for(id, &input);
                self.0.arena.began(vec![input]);
                self.0.dirty.set(true);
                self.fingers_changed();
                self.finish(window, cx);
                true
            }
            gpui::TouchPhase::Moved => {
                if self.0.touches.borrow().iter().any(|t| t.id == id) {
                    self.touch_moved(id, point, force, time, window, cx);
                    self.finish(window, cx);
                    return true;
                }
                self.native_moved(id, point)
            }
            gpui::TouchPhase::Ended | gpui::TouchPhase::Cancelled => {
                let contact = {
                    let mut touches = self.0.touches.borrow_mut();
                    let index = touches.iter().position(|t| t.id == id);
                    index.map(|i| touches.remove(i))
                };
                let cancelled = event.phase == gpui::TouchPhase::Cancelled;
                let claimed = match contact {
                    Some(contact) => {
                        let mut input = TouchInput::new(id, point.x, point.y, time);
                        input.force = force;
                        self.touch_ended(contact, input, cancelled, window, cx);
                        true
                    }
                    None => {
                        let mut natives = self.0.natives.borrow_mut();
                        let index = natives.iter().position(|f| f.id == id);
                        index
                            .map(|i| natives.remove(i))
                            .is_some_and(|f| f.verdict == Some(true))
                    }
                };
                self.fingers_changed();
                self.finish(window, cx);
                claimed
            }
        }
    }

    /// A finger GPUI keeps moved. Under a page hold, once per one-finger
    /// touch past a 5 px slop, the holds judge its pan; one that would
    /// scroll the page is claimed from GPUI, which cancels its pan, so the
    /// page holds still for the rest of the touch.
    fn native_moved(&self, id: u64, point: Point) -> bool {
        let judge = {
            let natives = self.0.natives.borrow();
            let Some(finger) = natives.iter().find(|f| f.id == id) else {
                return false;
            };
            match finger.verdict {
                Some(held) => return held,
                None => {
                    let (dx, dy) = (point.x - finger.landed.x, point.y - finger.landed.y);
                    (dx.abs().max(dy.abs()) >= PAGE_HOLD_SLOP && !self.0.holds.borrow().is_empty())
                        .then_some((finger.origin, dx, dy))
                }
            }
        };
        let Some((origin, dx, dy)) = judge else {
            return false;
        };
        let held = self.page_hold_decides(origin, dx, dy);
        if let Some(finger) = self.0.natives.borrow_mut().iter_mut().find(|f| f.id == id) {
            finger.verdict = Some(held);
        }
        held
    }

    fn touch_moved(
        &self,
        id: u64,
        point: Point,
        force: Option<f64>,
        time: f64,
        window: &mut Window,
        cx: &mut App,
    ) {
        let window_pan = {
            let mut touches = self.0.touches.borrow_mut();
            let Some(contact) = touches.iter_mut().find(|t| t.id == id) else {
                return;
            };
            let previous = contact.last;
            contact.last = point;
            contact.window_pan.as_mut().map(|pan| {
                pan.tracker.add(point.x, point.y, time);
                let delta = if pan.horizontal {
                    Point::new(point.x - previous.x, 0.)
                } else {
                    Point::new(0., point.y - previous.y)
                };
                (pan.held, delta, contact.start)
            })
        };
        if let Some((held, delta, start)) = window_pan {
            // The window's scroll, which no recognizer can take back (FR14).
            if !held {
                self.emit_scroll(start, delta, gpui::TouchPhase::Moved, window, cx);
            }
            return;
        }
        let mut input = TouchInput::new(id, point.x, point.y, time);
        input.force = force;
        self.0.arena.moved(vec![input]);
        self.maybe_take(id, time, window, cx);
    }

    /// The window takes a finger nobody claims once it travels past the
    /// press slop along an axis its areas leave to the window (WebKit's
    /// 10-11 px; a pan claims at 10, first).
    fn maybe_take(&self, id: u64, time: f64, window: &mut Window, cx: &mut App) {
        let Some((start, last, allows_x, allows_y, origin)) = self
            .0
            .touches
            .borrow()
            .iter()
            .find(|t| t.id == id)
            .map(|t| (t.start, t.last, t.allows_x, t.allows_y, t.origin))
        else {
            return;
        };
        // Two fingers down are no window pan (the window would pinch).
        if self.0.arena.is_claimed(id)
            || self.fingers_down() > 1
            || distance(start, last) <= PRESS::SLOP
        {
            return;
        }
        let keeps = self
            .0
            .arena
            .participants_of(id)
            .iter()
            .any(|r| r.keeps_touch(id))
            || self
                .0
                .arena
                .consumer_of(id)
                .is_some_and(|c| c.keeps_touch(id));
        if keeps {
            return;
        }
        let (dx, dy) = (last.x - start.x, last.y - start.y);
        let horizontal = dx.abs() > dy.abs();
        if !(if horizontal { allows_x } else { allows_y }) {
            return;
        }
        self.0.arena.browser_took_over(&[id], time);
        let held = self.page_hold_decides(origin, dx, dy);
        let mut tracker = VelocityTracker::default();
        if let Some(touch) = self.0.arena.get_touch(id) {
            tracker.add(touch.start_x(), touch.start_y(), touch.start_time());
        }
        tracker.add(last.x, last.y, time);
        if let Some(contact) = self.0.touches.borrow_mut().iter_mut().find(|t| t.id == id) {
            contact.window_pan = Some(WindowPan {
                horizontal,
                held,
                tracker,
            });
        }
        self.0.dirty.set(true);
        if !held {
            let delta = if horizontal {
                Point::new(dx, 0.)
            } else {
                Point::new(0., dy)
            };
            self.emit_scroll(start, delta, gpui::TouchPhase::Started, window, cx);
        }
    }

    fn touch_ended(
        &self,
        contact: TouchContact,
        input: TouchInput,
        cancelled: bool,
        window: &mut Window,
        cx: &mut App,
    ) {
        let id = contact.id;
        let (point, time) = (Point::new(input.x, input.y), input.time);
        if cancelled {
            self.0.arena.cancelled(&[id], time);
        }
        let disposition = if cancelled {
            ClickDisposition::Suppress
        } else {
            self.0
                .arena
                .ended(vec![input])
                .get(&id)
                .copied()
                .unwrap_or(ClickDisposition::Suppress)
        };
        self.unlock(id);
        self.0.dirty.set(true);
        if let Some(pan) = contact.window_pan {
            if pan.held {
                return;
            }
            let phase = if cancelled {
                gpui::TouchPhase::Cancelled
            } else {
                gpui::TouchPhase::Ended
            };
            self.emit_scroll(contact.start, Point::ZERO, phase, window, cx);
            if !cancelled {
                let velocity = pan.tracker.scroll_velocity(time);
                let velocity = if pan.horizontal {
                    Point::new(velocity.x, 0.)
                } else {
                    Point::new(0., velocity.y)
                };
                self.start_momentum(contact.start, velocity, window, cx);
            }
            return;
        }
        match disposition {
            ClickDisposition::Allow => self.0.replays.borrow_mut().push((point, ReplayKind::Touch)),
            ClickDisposition::Defer => self.0.pending_clicks.borrow_mut().push(PendingClick {
                id,
                position: point,
                kind: ReplayKind::Touch,
            }),
            ClickDisposition::Suppress => {}
        }
    }

    /// Emits the window's own touch scroll, as GPUI's recognizer would.
    fn emit_scroll(
        &self,
        position: Point,
        delta: Point,
        phase: gpui::TouchPhase,
        window: &mut Window,
        cx: &mut App,
    ) {
        let weak = self.downgrade();
        window.defer(cx, move |window, cx| {
            let Some(host) = weak.upgrade() else {
                return;
            };
            host.emitting.set(host.emitting.get() + 1);
            window.dispatch_event(
                PlatformInput::Scroll(ScrollEvent {
                    position: to_pixels(position),
                    delta: ScrollDelta::Pixels(to_pixels(delta)),
                    modifiers: Modifiers::default(),
                    touch_phase: phase,
                    source: ScrollSource::Touch,
                }),
                cx,
            );
            host.emitting.set(host.emitting.get() - 1);
        });
    }

    /// The window's coasting after a lift: UIKit's normal deceleration.
    fn start_momentum(&self, position: Point, velocity: Point, window: &mut Window, cx: &mut App) {
        let speed = velocity.x.hypot(velocity.y);
        if speed < MIN_FLING_VELOCITY {
            return;
        }
        let direction = Point::new(velocity.x / speed, velocity.y / speed);
        let started = self.now();
        let weak = self.downgrade();
        let ln = DECELERATION_NORMAL.ln();
        self.0.momentum_origin.set(Some(position));
        let task = window.spawn(cx, async move |cx| {
            let mut emitted = 0.;
            loop {
                let Some(executor) = weak.upgrade().map(|host| host.clock.executor.clone()) else {
                    return;
                };
                executor
                    .timer(Duration::from_secs_f64(FRAME_MS / 1000.))
                    .await;
                let Some(host) = weak.upgrade() else {
                    return;
                };
                let host = GestureHost(host);
                let elapsed = host.now() - started;
                let travelled = speed / 1000. * (DECELERATION_NORMAL.powf(elapsed) - 1.) / ln;
                let step = (travelled - emitted).max(0.);
                emitted = emitted.max(travelled);
                let done = speed * DECELERATION_NORMAL.powf(elapsed) < MOMENTUM_STOP_VELOCITY;
                let delta = Point::new(direction.x * step, direction.y * step);
                let result = cx.update(|window, cx| {
                    host.0.emitting.set(host.0.emitting.get() + 1);
                    window.dispatch_event(
                        PlatformInput::Scroll(ScrollEvent {
                            position: to_pixels(position),
                            delta: ScrollDelta::Pixels(to_pixels(delta)),
                            modifiers: Modifiers::default(),
                            touch_phase: if done {
                                gpui::TouchPhase::Ended
                            } else {
                                gpui::TouchPhase::Moved
                            },
                            source: ScrollSource::Touch,
                        }),
                        cx,
                    );
                    host.0.emitting.set(host.0.emitting.get() - 1);
                });
                if result.is_err() || done {
                    host.0.momentum_origin.set(None);
                    return;
                }
            }
        });
        *self.0.momentum.borrow_mut() = Some(task);
    }

    /// A contact coming down catches the window's coasting.
    fn stop_momentum(&self, window: &mut Window, cx: &mut App) {
        let task = self.0.momentum.borrow_mut().take();
        if task.is_some()
            && let Some(position) = self.0.momentum_origin.take()
        {
            self.emit_scroll(position, Point::ZERO, gpui::TouchPhase::Ended, window, cx);
        }
    }

    // --- Scroll ----------------------------------------------------------------

    fn scroll(&self, event: &ScrollEvent, window: &mut Window, cx: &mut App) {
        if event.source == ScrollSource::Touch {
            self.finger_scroll(event, window, cx);
            return;
        }
        self.sync_viewport(window);
        self.check_active(window);
        self.wheel(event, window, cx);
        self.finish(window, cx);
    }

    /// A finger's scroll: GPUI's own pan of a finger it kept, or the
    /// window's pan the host emits. The host follows it for the finger
    /// record, the scroll brake, page holds and finger-scroll watches.
    fn finger_scroll(&self, event: &ScrollEvent, window: &mut Window, cx: &mut App) {
        let time = self.now();
        let position = to_point(event.position);
        // A scroll is a finger's only if a finger was down there recently.
        if let Some((landed, at)) = self.0.last_touch.get()
            && time - at < PRESS::MOMENTUM_MEMORY
        {
            let scroller = self.0.scrollers.chain_at(landed).first().map(|s| s.bounds);
            self.0.last_scroll.set(Some((scroller, time)));
        }
        super::finger_scroll::on_scroll(self, position, event.touch_phase, time);
        let _ = (window, cx);
    }

    fn page_hold_decides(&self, origin: Option<ViewKey>, dx: f64, dy: f64) -> bool {
        if self.fingers_down() > 1 {
            // A second finger is a pinch: zooming the page stays available.
            return false;
        }
        let holds: Vec<_> = self
            .0
            .holds
            .borrow()
            .iter()
            .map(|(_, d)| d.clone())
            .collect();
        holds.iter().any(|decide| decide(origin, dx, dy))
    }

    fn wheel(&self, event: &ScrollEvent, window: &mut Window, cx: &mut App) {
        let time = self.now();
        let kind = if event.modifiers.control {
            WheelKind::Pinch
        } else {
            WheelKind::Pan
        };
        let (gx, gy, mut scroll_type) = match event.delta {
            ScrollDelta::Pixels(p) => (
                f64::from(f32::from(p.x)),
                f64::from(f32::from(p.y)),
                ScrollType::Continuous,
            ),
            ScrollDelta::Lines(l) => (
                f64::from(l.x) * WHEEL_LINE_PX,
                f64::from(l.y) * WHEEL_LINE_PX,
                ScrollType::Discrete,
            ),
        };
        if event.modifiers.shift {
            scroll_type = ScrollType::Discrete;
        }
        // The web's wheel deltas: positive scrolls the content up and left.
        let (mut dx, mut dy) = (-gx, -gy);
        let continuing = self
            .0
            .wheel
            .borrow()
            .as_ref()
            .map(|g| (g.kind, g.last, g.end_ms));
        if let Some((g_kind, last, end_ms)) = continuing
            && (g_kind != kind || time - last >= end_ms)
        {
            self.end_wheel(Some(time));
        }
        let leave = self.0.wheel.borrow().as_ref().is_some_and(|g| {
            !g.ids.is_empty() && !g.follow.as_ref().is_some_and(|f| f.lifted) && !wheel_has_taker(g)
        });
        if leave {
            // Every recognizer that took it has left: the rest is the page's.
            let last = self.0.wheel.borrow().as_ref().map(|g| g.last);
            self.end_wheel(last);
            *self.0.wheel.borrow_mut() = Some(kept_wheel(kind, event, time, scroll_type, dx, dy));
        }
        let horizontal = dx.abs() > dy.abs();
        let segment = if kind == WheelKind::Pan {
            Some(self.0.segmenter.borrow_mut().push(
                time,
                if horizontal { dx } else { dy },
                if horizontal {
                    WHEEL_FOLLOW_END_MS
                } else {
                    WHEEL_FOLLOW_END_V_MS
                },
                scroll_type == ScrollType::Continuous,
            ))
        } else {
            self.0.segmenter.borrow_mut().end();
            None
        };
        // A platform that reports the trackpad's phases starts a gesture with
        // its first wheel.
        let phase_boundary = event.touch_phase == gpui::TouchPhase::Started;
        if (segment.is_some_and(|s| s.boundary.is_some()) || phase_boundary)
            && self.0.wheel.borrow().is_some()
        {
            self.end_wheel(Some(time));
        }
        if let Some(segment) = segment
            && segment.carried != 0.
        {
            if horizontal {
                dx += segment.carried;
            } else {
                dy += segment.carried;
            }
        }
        if self.0.wheel.borrow().is_none() {
            let gesture = self.begin_wheel(event, kind, dx, dy, time, scroll_type, window);
            *self.0.wheel.borrow_mut() = Some(gesture);
        }
        let timer = {
            let mut wheel = self.0.wheel.borrow_mut();
            let gesture = wheel.as_mut().expect("a wheel gesture");
            gesture.last = time;
            (gesture.timer.take(), gesture.end_ms)
        };
        if let Some(old) = timer.0 {
            self.0.clock.clear_timeout(old);
        }
        let weak = self.downgrade();
        let handle = self.0.clock.set_timeout(
            Box::new(move || {
                if let Some(host) = weak.upgrade() {
                    host.segmenter.borrow_mut().end();
                    GestureHost(host).end_wheel(None);
                }
            }),
            timer.1,
        );
        let (taken, follows) = {
            let mut wheel = self.0.wheel.borrow_mut();
            let gesture = wheel.as_mut().expect("a wheel gesture");
            gesture.timer = Some(handle);
            (!gesture.ids.is_empty(), gesture.follow.is_some())
        };
        if !taken {
            return;
        }
        // A wheel a recognizer takes moves nothing under it.
        cx.stop_propagation();
        self.0.dirty.set(true);
        if kind == WheelKind::Pan && follows {
            self.follow_wheel(segment, dx, dy, time, event.touch_phase);
            return;
        }
        let moves = {
            let mut wheel = self.0.wheel.borrow_mut();
            let gesture = wheel.as_mut().expect("a wheel gesture");
            if gesture.kind == WheelKind::Pan {
                gesture.x -= dx;
                gesture.y -= dy;
                vec![TouchInput::new(gesture.ids[0], gesture.x, gesture.y, time)]
            } else {
                gesture.span *= (-dy / 100.).exp();
                pinch_contacts(gesture, time)
            }
        };
        self.0.arena.moved(moves);
    }

    fn follow_wheel(
        &self,
        segment: Option<super::wheel_segmenter::WheelSegment>,
        dx: f64,
        dy: f64,
        time: f64,
        phase: gpui::TouchPhase,
    ) {
        let lift_now = segment.is_some_and(|s| s.lift_now) || phase == gpui::TouchPhase::Ended;
        let decaying = segment.is_some_and(|s| s.decaying);
        let bunched = segment.is_some_and(|s| s.bunched);
        let mut wheel = self.0.wheel.borrow_mut();
        let Some(gesture) = wheel.as_mut() else {
            return;
        };
        let id = gesture.ids[0];
        let Some(f) = gesture.follow.as_mut() else {
            return;
        };
        if f.lifted {
            return;
        }
        if lift_now {
            // Momentum began: the pan keeps the travel it showed and lets go
            // here, still moving at the fingers' speed.
            let moved = follow_provisionally(gesture, time);
            if let Some(f) = gesture.follow.as_mut() {
                f.lifted = true;
            }
            let (x, y) = (gesture.x, gesture.y);
            drop(wheel);
            if let Some(input) = moved {
                self.0.arena.moved(vec![input]);
            }
            self.0.arena.ended(vec![TouchInput::new(id, x, y, time)]);
            return;
        }
        if decaying {
            // Undecided: the real deltas wait while the fingers' last speed
            // carries the pan on.
            f.held_x += dx;
            f.held_y += dy;
            if bunched {
                let since = time - f.last_move;
                let (speed_x, speed_y) = (f.speed_x, f.speed_y);
                let (tx, ty) = (f.true_x - f.held_x, f.true_y - f.held_y);
                join_reading(f, tx, ty);
                f.base_x += (speed_x - f.speed_x) * since;
                f.base_y += (speed_y - f.speed_y) * since;
            } else {
                f.read_span = 0.;
            }
            let moved = follow_provisionally(gesture, time);
            drop(wheel);
            if let Some(input) = moved {
                self.0.arena.moved(vec![input]);
            }
            return;
        }
        // The fingers' own delta; the held ones were theirs too.
        let before_x = f.true_x - f.held_x;
        let before_y = f.true_y - f.held_y;
        let debt_x = absorbed_debt(gesture.x - before_x, dx);
        let debt_y = absorbed_debt(gesture.y - before_y, dy);
        let true_x = before_x - dx;
        let true_y = before_y - dy;
        let elapsed = time - f.last_move;
        if bunched {
            join_reading(f, true_x, true_y);
        } else if elapsed > 0. {
            f.prior_speed_x = f.speed_x;
            f.prior_speed_y = f.speed_y;
            f.read_from_x = f.true_x;
            f.read_from_y = f.true_y;
            f.read_span = elapsed;
            f.speed_x = (true_x - f.true_x) / elapsed;
            f.speed_y = (true_y - f.true_y) / elapsed;
        }
        f.true_x = true_x;
        f.true_y = true_y;
        f.held_x = 0.;
        f.held_y = 0.;
        f.last_move = time;
        gesture.x = true_x + debt_x;
        gesture.y = true_y + debt_y;
        if let Some(f) = gesture.follow.as_mut() {
            f.base_x = gesture.x;
            f.base_y = gesture.y;
        }
        let input = TouchInput::new(id, gesture.x, gesture.y, time);
        drop(wheel);
        self.0.arena.moved(vec![input]);
    }

    /// Starts a wheel gesture: who takes it, decided once, or nobody.
    #[allow(clippy::too_many_arguments)]
    fn begin_wheel(
        &self,
        event: &ScrollEvent,
        kind: WheelKind,
        dx: f64,
        dy: f64,
        time: f64,
        scroll_type: ScrollType,
        window: &Window,
    ) -> WheelGesture {
        let mut kept = kept_wheel(kind, event, time, scroll_type, dx, dy);
        // A contact already down keeps its gesture; the wheel stays the page's.
        if !self.0.arena.touches().is_empty() || (dx == 0. && dy == 0.) {
            return kept;
        }
        let point = Point::new(kept.x, kept.y);
        let horizontal = dx.abs() > dy.abs();
        let probe = self.0.scrollers.probe(point, false);
        let path = self.path(window, true);
        let mut takers = Vec::new();
        for area in &path {
            for recognizer in area.recognizers() {
                let family_matches = match kind {
                    WheelKind::Pan => recognizer.family() == Some(RecognizerFamily::Pan),
                    WheelKind::Pinch => recognizer.family() == Some(RecognizerFamily::Pinch),
                };
                if !recognizer.is_enabled()
                    || !recognizer.claims_touches()
                    || !recognizer.allows_scroll_type(scroll_type)
                    || !family_matches
                    || (kind == WheelKind::Pan && scroll_type != ScrollType::Continuous)
                {
                    continue;
                }
                if kind == WheelKind::Pan {
                    let takes_move = recognizer
                        .classify(Point::new(-dx, -dy))
                        .is_none_or(|v| v == super::recognizer::PanVerdict::Begin);
                    let scroller_takes = self.scroller_takes(area, point, dx, dy, horizontal);
                    let scroll_edge = recognizer
                        .downcast::<super::pan::Pan>()
                        .is_some_and(|pan| pan.behavior().scroll_edge);
                    if !takes_move || scroller_takes || (scroll_edge && probe.can_scroll(-dx, -dy))
                    {
                        continue;
                    }
                }
                takers.push(recognizer);
            }
        }
        if takers.is_empty() {
            return kept;
        }
        let ids: Vec<u64> = match kind {
            WheelKind::Pan => vec![self.next_contact()],
            WheelKind::Pinch => vec![self.next_contact(), self.next_contact()],
        };
        kept.ids = ids.clone();
        kept.takers = takers.clone();
        if kind == WheelKind::Pan && scroll_type == ScrollType::Continuous {
            kept.follow = Some(WheelFollow {
                lifted: false,
                held_x: 0.,
                held_y: 0.,
                last_move: time,
                true_x: kept.x,
                true_y: kept.y,
                base_x: kept.x,
                base_y: kept.y,
                speed_x: 0.,
                speed_y: 0.,
                read_from_x: kept.x,
                read_from_y: kept.y,
                read_span: 0.,
                prior_speed_x: 0.,
                prior_speed_y: 0.,
            });
        }
        let contacts = match kind {
            WheelKind::Pan => vec![TouchInput::new(ids[0], kept.x, kept.y, time)],
            WheelKind::Pinch => pinch_contacts(&kept, time),
        };
        let inputs: Vec<ArenaBeganInput> = contacts
            .into_iter()
            .map(|contact| {
                let mut input = TouchBeganInput::new(
                    contact.id,
                    PointerKind::Mouse,
                    contact.x,
                    contact.y,
                    time,
                );
                input.target = path.first().map(GestureAttachment::key);
                input.scroll = Some(probe.clone());
                input.scroll_type = Some(scroll_type);
                ArenaBeganInput {
                    input,
                    recognizers: takers.clone(),
                    consumer: None,
                }
            })
            .collect();
        self.0.arena.began(inputs);
        // Every recognizer offered it may decline it: then nobody took the
        // wheel, and it scrolls the page from its first step.
        if ids
            .iter()
            .all(|id| self.0.arena.participants_of(*id).is_empty())
        {
            self.0.arena.cancelled(&ids, time);
            kept.ids.clear();
            kept.takers.clear();
            kept.follow = None;
        }
        kept
    }

    /// Whether a registered scroller inside `area` (or the area's own, on
    /// an axis its recognizers do not keep) scrolls this wheel now.
    fn scroller_takes(
        &self,
        area: &GestureAttachment,
        point: Point,
        dx: f64,
        dy: f64,
        horizontal: bool,
    ) -> bool {
        let Some(bounds) = area.bounds() else {
            return false;
        };
        let keeps_axis = {
            let behavior = combined_native_behavior(&area.recognizers());
            if horizontal {
                !behavior.pan_x
            } else {
                !behavior.pan_y
            }
        };
        self.0.scrollers.chain_at(point).iter().any(|scroller| {
            let inside = bounds.contains(&scroller.bounds.origin) && scroller.bounds != bounds;
            let own = scroller.bounds == bounds;
            (inside || (own && !keeps_axis))
                && scroller.can_scroll(
                    if horizontal { -dx } else { 0. },
                    if horizontal { 0. } else { -dy },
                )
        })
    }

    /// Ends the wheel gesture in progress, never later than `end_time`.
    pub(crate) fn end_wheel(&self, end_time: Option<f64>) {
        let Some(mut gesture) = self.0.wheel.borrow_mut().take() else {
            return;
        };
        if let Some(timer) = gesture.timer.take() {
            self.0.clock.clear_timeout(timer);
        }
        if gesture.ids.is_empty() || gesture.follow.as_ref().is_some_and(|f| f.lifted) {
            return;
        }
        // Fingers that stop without a lift end on their travel, where it runs
        // ahead of what the pan shows.
        if let Some(f) = &gesture.follow {
            let x = ahead(gesture.x, f.true_x - f.held_x, f.speed_x);
            let y = ahead(gesture.y, f.true_y - f.held_y, f.speed_y);
            if x != gesture.x || y != gesture.y {
                gesture.x = x;
                gesture.y = y;
                self.0
                    .arena
                    .moved(vec![TouchInput::new(gesture.ids[0], x, y, gesture.last)]);
            }
        }
        let end = gesture.last + gesture.end_ms;
        let time = end_time.map_or(end, |t| gesture.last.max(end.min(t)));
        let contacts = match gesture.kind {
            WheelKind::Pan => vec![TouchInput::new(gesture.ids[0], gesture.x, gesture.y, time)],
            WheelKind::Pinch => pinch_contacts(&gesture, time),
        };
        self.0.arena.ended(contacts);
        self.0.dirty.set(true);
    }

    // --- Pinch -----------------------------------------------------------------

    /// A trackpad pinch: two contacts a span apart around its center, as a
    /// ctrl-wheel drives them.
    fn pinch(&self, event: &PinchEvent, window: &mut Window, cx: &mut App) {
        self.sync_viewport(window);
        let time = self.now();
        let center = to_point(event.position);
        let continuing = self
            .0
            .wheel
            .borrow()
            .as_ref()
            .is_some_and(|g| g.kind == WheelKind::Pinch && time - g.last < g.end_ms);
        if !continuing || event.phase == gpui::TouchPhase::Started {
            self.end_wheel(Some(time));
            let synthetic = ScrollEvent {
                position: event.position,
                delta: ScrollDelta::Pixels(gpui::point(px(0.), px(1.))),
                modifiers: event.modifiers,
                touch_phase: gpui::TouchPhase::Started,
                source: ScrollSource::Pointer,
            };
            let mut gesture = self.begin_wheel(
                &synthetic,
                WheelKind::Pinch,
                0.,
                -1.,
                time,
                ScrollType::Continuous,
                window,
            );
            gesture.x = center.x;
            gesture.y = center.y;
            *self.0.wheel.borrow_mut() = Some(gesture);
        }
        let taken = {
            let mut wheel = self.0.wheel.borrow_mut();
            let Some(gesture) = wheel.as_mut() else {
                return;
            };
            gesture.last = time;
            if gesture.ids.is_empty() {
                None
            } else {
                gesture.span *= 1. + f64::from(event.delta);
                Some(pinch_contacts(gesture, time))
            }
        };
        let Some(contacts) = taken else {
            return;
        };
        cx.stop_propagation();
        self.0.arena.moved(contacts);
        if matches!(
            event.phase,
            gpui::TouchPhase::Ended | gpui::TouchPhase::Cancelled
        ) {
            self.end_wheel(Some(time));
        } else {
            let weak = self.downgrade();
            let handle = self.0.clock.set_timeout(
                Box::new(move || {
                    if let Some(host) = weak.upgrade() {
                        GestureHost(host).end_wheel(None);
                    }
                }),
                WHEEL_END_MS,
            );
            if let Some(old) = self
                .0
                .wheel
                .borrow_mut()
                .as_mut()
                .and_then(|g| g.timer.replace(handle))
            {
                self.0.clock.clear_timeout(old);
            }
        }
        self.0.dirty.set(true);
        self.finish(window, cx);
    }

    // --- Keys --------------------------------------------------------------------

    /// Shift+F10 and the context-menu key: a keyboard context request for the
    /// innermost focused area that answers one.
    fn key_down(&self, event: &KeyDownEvent, window: &mut Window, cx: &mut App) {
        let key = event.keystroke.key.as_str();
        let is_request = (key == "f10" && event.keystroke.modifiers.shift)
            || key == "menu"
            || key == "contextmenu";
        if !is_request {
            return;
        }
        let time = self.now();
        let mut areas: Vec<(u64, GestureAttachment)> = self
            .0
            .areas
            .borrow()
            .iter()
            .filter_map(|e| {
                e.attachment
                    .upgrade()
                    .map(|a| (e.seq, GestureAttachment(a)))
            })
            .collect();
        areas.sort_by_key(|(seq, _)| std::cmp::Reverse(*seq));
        for (_, area) in areas {
            let focused = area
                .0
                .focus
                .borrow()
                .as_ref()
                .is_some_and(|focus| focus.contains_focused(window, cx));
            let handler = area.0.context.borrow().clone();
            if focused
                && let Some(handler) = handler
                && handler.is_enabled()
                && handler.key(event.is_held, time)
            {
                cx.stop_propagation();
                break;
            }
        }
        self.finish(window, cx);
    }
}

fn kept_wheel(
    kind: WheelKind,
    event: &ScrollEvent,
    time: f64,
    scroll_type: ScrollType,
    dx: f64,
    dy: f64,
) -> WheelGesture {
    let at = to_point(event.position);
    let horizontal = dx.abs() > dy.abs();
    WheelGesture {
        kind,
        ids: Vec::new(),
        x: at.x,
        y: at.y,
        span: WHEEL_PINCH_SPAN,
        last: time,
        end_ms: if kind == WheelKind::Pinch || scroll_type != ScrollType::Continuous {
            WHEEL_END_MS
        } else if horizontal {
            WHEEL_FOLLOW_END_MS
        } else {
            WHEEL_FOLLOW_END_V_MS
        },
        timer: None,
        takers: Vec::new(),
        follow: None,
    }
}

fn wheel_has_taker(gesture: &WheelGesture) -> bool {
    gesture.takers.iter().any(|recognizer| {
        let state = recognizer.state();
        recognizer.is_enabled()
            && (state.is_active() || (state == GestureState::Possible && recognizer.is_involved()))
    })
}

fn pinch_contacts(gesture: &WheelGesture, time: f64) -> Vec<TouchInput> {
    vec![
        TouchInput::new(
            gesture.ids[0],
            gesture.x - gesture.span / 2.,
            gesture.y,
            time,
        ),
        TouchInput::new(
            gesture.ids[1],
            gesture.x + gesture.span / 2.,
            gesture.y,
            time,
        ),
    ]
}

/// Moves a followed pan through the segmenter's undecided stretch, at the
/// fingers' last speed.
fn follow_provisionally(gesture: &mut WheelGesture, time: f64) -> Option<TouchInput> {
    let f = gesture.follow.as_ref()?;
    let elapsed = (time - f.last_move).max(0.);
    let x = f.base_x + f.speed_x * elapsed;
    let y = f.base_y + f.speed_y * elapsed;
    if x == gesture.x && y == gesture.y {
        return None;
    }
    gesture.x = x;
    gesture.y = y;
    Some(TouchInput::new(gesture.ids[0], x, y, time))
}

/// A provisional lead's debt after a move: half the move is absorbed.
fn absorbed_debt(debt: f64, moved: f64) -> f64 {
    super::geometry::sign(debt) * (debt.abs() - moved.abs() / 2.).max(0.)
}

/// A stall's bunched wheel joins the reading of the move before it.
fn join_reading(f: &mut WheelFollow, true_x: f64, true_y: f64) {
    if f.read_span <= 0. {
        return;
    }
    f.speed_x = joined_speed(
        f.speed_x,
        (true_x - f.read_from_x) / f.read_span,
        f.prior_speed_x,
    );
    f.speed_y = joined_speed(
        f.speed_y,
        (true_y - f.read_from_y) / f.read_span,
        f.prior_speed_y,
    );
}

fn joined_speed(current: f64, reading: f64, prior: f64) -> f64 {
    let held = super::geometry::sign(reading) * reading.abs().min(prior.abs());
    if held * current >= 0. && held.abs() > current.abs() {
        held
    } else {
        current
    }
}

/// `target` where it is ahead of `shown` in the direction of `speed`.
fn ahead(shown: f64, target: f64, speed: f64) -> f64 {
    if speed == 0. || (target - shown) * super::geometry::sign(speed) > 0. {
        target
    } else {
        shown
    }
}
