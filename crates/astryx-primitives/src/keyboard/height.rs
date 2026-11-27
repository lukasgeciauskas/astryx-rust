//! Upstream `keyboard/keyboardHeight.ts` and `utils/layoutViewport.ts`: Core's one source
//! for the on-screen keyboard (AST-099 FR1, FR2, FR6, FR7): its height, one set of
//! listeners per window, the focus moves, and the fields that bring a keyboard up.
//!
//! GPUI gives each window one visual-viewport callback and one insets callback, which
//! refresh the window. So the module samples once per drawn frame: every reader renders
//! [`keyboard_probe`] (the text editor registers itself, which does the same), and after
//! the frame the module measures the height, compares the focused element with the
//! last frame's, and reports both to its listeners in the order upstream's events come.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::{Rc, Weak};
use std::time::Duration;

use gpui::{
    App, Bounds, FocusHandle, Global, Pixels, ScrollHandle, Task, Window, WindowId, WindowInsets,
};
use web_time::Instant;

use crate::gestures::host::{GestureHost, gesture_host};
use crate::motion::Clock;

/// A shortfall smaller than this is rounding, not a keyboard, px.
pub const KEYBOARD_EPSILON: f64 = 1.;
/// How long a step of a hide waits for the next report, ms: longer than Gboard's whole
/// hide.
const HIDE_STEP_WAIT_MS: f64 = 300.;
/// How long a hide's steps wait in all, from the first, ms.
const HIDE_STEP_LIMIT_MS: f64 = 1000.;

// ---------------------------------------------------------------------------------------
// The clock

struct KeyboardClock(Instant);

impl Global for KeyboardClock {}

fn origin(cx: &mut App) -> Instant {
    if let Some(clock) = cx.try_global::<KeyboardClock>() {
        return clock.0;
    }
    let now = cx.background_executor().now();
    cx.set_global(KeyboardClock(now));
    now
}

/// The keyboard clock, ms: the time every report, transition and ride shares (upstream's
/// document timeline, `performance.now()`).
pub fn keyboard_now(cx: &mut App) -> f64 {
    let origin = origin(cx);
    cx.background_executor()
        .now()
        .saturating_duration_since(origin)
        .as_secs_f64()
        * 1000.
}

/// The keyboard clock as a motion engine's clock, so a ride's times are the guide's.
pub fn keyboard_clock(cx: &mut App) -> Clock {
    let origin = origin(cx);
    let executor = cx.background_executor().clone();
    Rc::new(move || {
        executor
            .now()
            .saturating_duration_since(origin)
            .as_secs_f64()
            * 1000.
    })
}

// ---------------------------------------------------------------------------------------
// Measuring

/// Upstream `layoutViewportHeight` (`utils/layoutViewport.ts`): the layout viewport's
/// height, px. In GPUI it is the window's drawable height: the web build holds its canvas
/// at the keyboard-free height while a software keyboard may be up, so a keyboard never
/// shrinks it. GPUI's scrollbars overlay the content (fork patch R17), so no classic
/// scrollbar takes a part of it, and WebKit's zoomed `innerHeight` has no counterpart.
pub fn layout_viewport_height(window: &Window) -> f64 {
    f64::from(window.viewport_size().height)
}

/// Upstream `isKeyboardMeasurable`: whether this window measures the keyboard. Natively
/// a window always does. On the web it is a top-level document (FR7): inside an iframe
/// the frame's visual viewport does not change for the keyboard. Capabilities decide it,
/// never the user agent or the pointer (FR6).
pub fn is_keyboard_measurable(_window: &Window) -> bool {
    #[cfg(target_family = "wasm")]
    {
        web_sys::window().is_some_and(|w| {
            let top = w.top().ok().flatten();
            top.is_some_and(|top| {
                let me: &wasm_bindgen::JsValue = w.as_ref();
                let top: &wasm_bindgen::JsValue = top.as_ref();
                me == top
            })
        })
    }
    #[cfg(not(target_family = "wasm"))]
    {
        true
    }
}

/// The visual viewport's pinch-zoom scale: the browser's on the web, 1 natively.
fn visual_viewport_scale() -> f64 {
    #[cfg(target_family = "wasm")]
    {
        web_sys::window()
            .and_then(|w| w.visual_viewport())
            .map_or(1., |v| v.scale())
    }
    #[cfg(not(target_family = "wasm"))]
    {
        1.
    }
}

/// Upstream `measureKeyboardHeight` (FR2): the on-screen keyboard's height over the
/// layout viewport, px. It is the layout viewport's height less the visual viewport's
/// height times its scale, so a pan (an offset of the visual viewport) is no smaller
/// keyboard and a pinch zoom no keyboard; where the platform reports the keyboard itself
/// (GPUI's `WindowInsets::ime`, natively on iOS and Android) the larger of the two. It
/// is 0 when none is shown, and wherever [`is_keyboard_measurable`] is false. A desktop
/// window's visual viewport never shrinks, so it reads 0; where the app lets the browser
/// shrink the layout viewport (`interactive-widget=resizes-content`) it reads 0 too,
/// since the browser has made the room (FR8).
pub fn measure_keyboard_height(window: &Window) -> f64 {
    if !is_keyboard_measurable(window) {
        return 0.;
    }
    let visual = window.visual_viewport_bounds();
    let from_viewport =
        layout_viewport_height(window) - f64::from(visual.size.height) * visual_viewport_scale();
    let ime = f64::from(window.insets().ime.bottom);
    let height = from_viewport.max(ime);
    if height > KEYBOARD_EPSILON {
        (height * 100.).round() / 100.
    } else {
        0.
    }
}

/// Upstream `hasVirtualKeyboard`: whether the browser has the VirtualKeyboard API, as
/// Chrome on Android does. The layout guide reads Chrome's profile there. Natively, and
/// in tests unless [`set_virtual_keyboard_api`] says otherwise, false.
pub fn has_virtual_keyboard(cx: &App) -> bool {
    if let Some(api) = cx.try_global::<VirtualKeyboardApi>() {
        return api.0;
    }
    #[cfg(target_family = "wasm")]
    {
        web_sys::window().is_some_and(|w| {
            js_sys::Reflect::has(
                &w.navigator(),
                &wasm_bindgen::JsValue::from_str("virtualKeyboard"),
            )
            .unwrap_or(false)
        })
    }
    #[cfg(not(target_family = "wasm"))]
    {
        false
    }
}

struct VirtualKeyboardApi(bool);

impl Global for VirtualKeyboardApi {}

/// Overrides [`has_virtual_keyboard`]: tests and harnesses that stand in for a browser
/// with (or without) Chrome's VirtualKeyboard API.
#[doc(hidden)]
pub fn set_virtual_keyboard_api(api: bool, cx: &mut App) {
    cx.set_global(VirtualKeyboardApi(api));
}

// ---------------------------------------------------------------------------------------
// Fields

/// A keyboard target: a surface that keeps its own fields above the keyboard (a sheet, a
/// chat composer's dock). A field inside one moves nothing itself (upstream
/// `registerKeyboardTarget`, `isInsideKeyboardTarget`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct KeyboardTargetId(pub u64);

/// Provided around a keyboard target's content: the fields that render inside register
/// as inside it.
#[derive(Clone, Debug, Default)]
pub struct KeyboardTargetScope(pub Vec<KeyboardTargetId>);

/// Provides `id` as a keyboard target around `child` (nested targets stack).
pub fn provide_keyboard_target(
    id: KeyboardTargetId,
    child: impl gpui::IntoElement,
    cx: &App,
) -> crate::Provider<KeyboardTargetScope> {
    let mut scope = crate::use_context::<KeyboardTargetScope>(cx).unwrap_or_default();
    scope.0.push(id);
    crate::provide(scope, child)
}

/// What a focusable control does to the keyboard (upstream `isTextEntryControl` and
/// `bringsUpKeyboard`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FieldKind {
    /// Focusing it brings up a keyboard for typing text: an editable field whose input
    /// mode is not `none`.
    pub text_entry: bool,
    /// A select, whose picker takes the keyboard's place on iOS.
    pub select: bool,
    /// A select or a date or time field, which Chrome on Android edits in a picker over
    /// the page while the keyboard goes away.
    pub picker_over_page: bool,
}

impl FieldKind {
    /// Upstream `bringsUpKeyboard`: a keyboard, or another input view in its place.
    pub fn brings_up_keyboard(&self) -> bool {
        self.text_entry || self.select
    }
}

/// A focusable control as it was drawn this frame.
#[derive(Clone, Debug)]
pub struct KeyboardField {
    pub focus: FocusHandle,
    pub kind: FieldKind,
    /// Where it was drawn, window coordinates (with any ride's translate).
    pub bounds: Bounds<Pixels>,
    /// Its nearest scroller ([`push_scroller`]): the handle and where it shows.
    pub scroller: Option<(ScrollHandle, Bounds<Pixels>)>,
    /// The page's scroller (`ScrollHost`), upstream's document.
    pub document: Option<ScrollHandle>,
    /// The keyboard targets it renders in, outermost first.
    pub targets: Vec<KeyboardTargetId>,
    /// Whether it keeps itself above the keyboard (Core's text fields do, FR9).
    pub keeps_itself: bool,
}

impl KeyboardField {
    /// Upstream `isInsideKeyboardTarget`.
    pub fn is_inside_target(&self) -> bool {
        !self.targets.is_empty()
    }

    /// Whether it renders inside `target`.
    pub fn is_in(&self, target: KeyboardTargetId) -> bool {
        self.targets.contains(&target)
    }
}

/// A scroller's handle and where it shows.
type Scroller = (ScrollHandle, Bounds<Pixels>);

thread_local! {
    /// The scrollers around the element prepainting now, innermost last.
    static SCROLLERS: RefCell<Vec<Option<Scroller>>> = const { RefCell::new(Vec::new()) };
}

/// A scrolling container's content starts prepainting (its scroll handle and where it
/// shows), so the fields inside know their nearest scroller. Balance with
/// [`pop_scroller`].
pub fn push_scroller(scroller: Option<(ScrollHandle, Bounds<Pixels>)>) {
    SCROLLERS.with(|s| s.borrow_mut().push(scroller));
}

/// A scrolling container's content has prepainted.
pub fn pop_scroller() {
    SCROLLERS.with(|s| {
        s.borrow_mut().pop();
    });
}

/// Upstream `nearestScroller`: the innermost scroller around the element prepainting
/// now.
pub fn nearest_scroller() -> Option<(ScrollHandle, Bounds<Pixels>)> {
    SCROLLERS.with(|s| s.borrow().iter().rev().find_map(|s| s.clone()))
}

/// Registers a focusable control drawn this frame (during prepaint): the text editor does
/// for every Core field. `bounds` are where it is drawn.
pub fn register_field(
    focus: &FocusHandle,
    kind: FieldKind,
    bounds: Bounds<Pixels>,
    keeps_itself: bool,
    window: &mut Window,
    cx: &mut App,
) {
    let field = KeyboardField {
        focus: focus.clone(),
        kind,
        bounds,
        scroller: nearest_scroller(),
        document: crate::use_context::<crate::ScrollHost>(cx).map(|h| h.0),
        targets: crate::use_context::<KeyboardTargetScope>(cx)
            .map(|s| s.0)
            .unwrap_or_default(),
        keeps_itself,
    };
    let store = store_of(window, cx);
    store.borrow_mut().frame_fields.push(field);
    schedule(&store, window, cx);
}

/// Whether a field drawn this frame keeps itself above the keyboard: the window's field
/// path (`keyboardTarget.ts`) attaches for it.
fn keeps_fields(store: &Rc<RefCell<Store>>) -> bool {
    store.borrow().frame_fields.iter().any(|f| f.keeps_itself)
}

/// The control with focus as last drawn, when it registered itself.
pub fn focused_field(window: &mut Window, cx: &mut App) -> Option<KeyboardField> {
    let store = store_of(window, cx);
    let s = store.borrow();
    s.fields
        .iter()
        .find(|f| f.focus.is_focused(window))
        .cloned()
}

/// Every control drawn last frame.
pub fn drawn_fields(window: &mut Window, cx: &mut App) -> Vec<KeyboardField> {
    store_of(window, cx).borrow().fields.clone()
}

/// Upstream `isTextEntryControl(document.activeElement)`: whether the focused element
/// brings up a keyboard for typing text.
pub fn is_text_entry_focused(window: &mut Window, cx: &mut App) -> bool {
    focused_field(window, cx).is_some_and(|f| f.kind.text_entry)
}

// ---------------------------------------------------------------------------------------
// The store

/// Where focus went or came from (upstream's `target` and `relatedTarget`).
#[derive(Clone, Debug)]
pub enum FocusTarget {
    /// Nothing has focus (`null`).
    Nothing,
    /// An element that registered no field.
    Element,
    /// A registered control.
    Field(KeyboardField),
}

impl FocusTarget {
    /// The control, when it is one.
    pub fn field(&self) -> Option<&KeyboardField> {
        match self {
            FocusTarget::Field(field) => Some(field),
            _ => None,
        }
    }

    /// Upstream `bringsUpKeyboard(target)`.
    pub fn brings_up_keyboard(&self) -> bool {
        self.field().is_some_and(|f| f.kind.brings_up_keyboard())
    }

    /// Upstream `isTextEntryControl(target)`.
    pub fn is_text_entry(&self) -> bool {
        self.field().is_some_and(|f| f.kind.text_entry)
    }
}

/// A report from the keyboard module, in the order the window raised them.
#[derive(Clone, Debug)]
pub enum KeyboardHeightEvent {
    /// The visual viewport, the insets or the window changed.
    Viewport {
        /// The keyboard height after this report.
        height: f64,
        /// The report's time on the keyboard clock.
        time: f64,
    },
    /// Focus moved into `target` (from `related`).
    FocusIn {
        height: f64,
        time: f64,
        target: FocusTarget,
        related: FocusTarget,
    },
    /// Focus moved out of `target` (to `related`).
    FocusOut {
        height: f64,
        time: f64,
        target: FocusTarget,
        related: FocusTarget,
    },
}

/// A keyboard listener (upstream `subscribeKeyboardHeight`'s `listener`).
pub type KeyboardListener = Rc<dyn Fn(&KeyboardHeightEvent, &mut Window, &mut App)>;

struct StepHold {
    height: f64,
    limit: f64,
    _timer: Task<()>,
}

pub(crate) struct Store {
    window: WindowId,
    height: f64,
    /// The visual viewport, the insets and the layout size at the last report.
    sampled: Option<(Bounds<Pixels>, WindowInsets, gpui::Size<Pixels>)>,
    listeners: Vec<(u64, KeyboardListener, bool)>,
    next_id: u64,
    step: Option<StepHold>,
    /// The controls drawn this frame, and last frame's.
    frame_fields: Vec<KeyboardField>,
    fields: Vec<KeyboardField>,
    focused: Option<FocusHandle>,
    focus_seen: bool,
    scheduled: bool,
    host: GestureHost,
    fingers: usize,
    /// Touch starts since the store came, and their count at the last hide.
    starts: u64,
    starts_at_hide: Option<u64>,
    attached: bool,
}

#[derive(Default)]
struct Stores(HashMap<WindowId, Rc<RefCell<Store>>>);

impl Global for Stores {}

pub(crate) fn store_of(window: &mut Window, cx: &mut App) -> Rc<RefCell<Store>> {
    let id = window.window_handle().window_id();
    if let Some(store) = cx.try_global::<Stores>().and_then(|s| s.0.get(&id)) {
        return store.clone();
    }
    let host = gesture_host_of(window, cx);
    let store = Rc::new(RefCell::new(Store {
        window: id,
        height: 0.,
        sampled: None,
        listeners: Vec::new(),
        next_id: 0,
        step: None,
        frame_fields: Vec::new(),
        fields: Vec::new(),
        focused: None,
        focus_seen: false,
        scheduled: false,
        host: host.clone(),
        fingers: 0,
        starts: 0,
        starts_at_hide: None,
        attached: false,
    }));
    // The input stream's touch counts (upstream `getTouches`): fingers on the screen, and
    // how many touches have started.
    let weak = Rc::downgrade(&store);
    host.on_fingers_change(move || {
        let Some(store) = weak.upgrade() else { return };
        let Ok(mut s) = store.try_borrow_mut() else {
            return;
        };
        let fingers = s.host.fingers_down();
        if fingers > s.fingers {
            s.starts += (fingers - s.fingers) as u64;
        }
        s.fingers = fingers;
    });
    cx.default_global::<Stores>().0.insert(id, store.clone());
    store
}

fn gesture_host_of(window: &mut Window, cx: &mut App) -> GestureHost {
    // The host needs a mutable window only to create itself; the store keeps it.
    gesture_host(window, cx)
}

fn schedule(store: &Rc<RefCell<Store>>, window: &Window, cx: &mut App) {
    if std::mem::replace(&mut store.borrow_mut().scheduled, true) {
        return;
    }
    let weak = Rc::downgrade(store);
    window.defer(cx, move |window, cx| {
        if let Some(store) = weak.upgrade() {
            process(&store, window, cx);
        }
    });
}

/// Keeps the module sampling: every reader renders it (or calls it in render) each frame.
/// After the frame the module measures the keyboard and reports what changed.
pub fn keyboard_probe(window: &mut Window, cx: &mut App) {
    let store = store_of(window, cx);
    schedule(&store, window, cx);
}

/// Upstream `isFingerOnScreen`: whether a finger is on the window's screen, from the
/// gesture host's count. A finger that was down at the keyboard's last hide counts no
/// more until a touch starts.
pub fn is_finger_on_screen(window: &mut Window, cx: &mut App) -> bool {
    let store = store_of(window, cx);
    let s = store.borrow();
    s.host.fingers_down() > 0 && s.starts_at_hide.is_none_or(|at| at != s.starts)
}

/// How many touches have started in the window (upstream `getTouches().starts`).
pub fn touch_starts(window: &mut Window, cx: &mut App) -> u64 {
    store_of(window, cx).borrow().starts
}

/// Upstream `getKeyboardHeight`: while the module listens, the height measured at the
/// last report, which every reader shares; with nothing listening, the height now.
pub fn get_keyboard_height(window: &mut Window, cx: &mut App) -> f64 {
    let store = store_of(window, cx);
    let s = store.borrow();
    if s.attached {
        s.height
    } else {
        drop(s);
        measure_keyboard_height(window)
    }
}

/// A subscription to the keyboard module; dropping it unsubscribes, and the last one
/// detaches the module's listening.
pub struct KeyboardSubscription {
    store: Weak<RefCell<Store>>,
    id: u64,
}

impl Drop for KeyboardSubscription {
    fn drop(&mut self) {
        let Some(store) = self.store.upgrade() else {
            return;
        };
        let Ok(mut s) = store.try_borrow_mut() else {
            return;
        };
        s.listeners.retain(|(id, _, _)| *id != self.id);
        if s.listeners.is_empty() {
            s.attached = false;
            s.step = None;
        }
    }
}

/// Upstream `subscribeKeyboardHeight`: calls `listener` with every report of the
/// window's keyboard until the subscription drops: the viewport's changes, and focus moves
/// too with `focus`. All subscribers of a window share one sampling.
pub fn subscribe_keyboard_height(
    window: &mut Window,
    cx: &mut App,
    focus: bool,
    listener: impl Fn(&KeyboardHeightEvent, &mut Window, &mut App) + 'static,
) -> KeyboardSubscription {
    let store = store_of(window, cx);
    let measured = measure_keyboard_height(window);
    let sample = (
        window.visual_viewport_bounds(),
        window.insets(),
        window.viewport_size(),
    );
    let focused = window.focused(cx);
    let id = {
        let mut s = store.borrow_mut();
        if !s.attached {
            s.attached = true;
            s.height = measured;
            s.sampled = Some(sample);
        }
        if focus && !s.listeners.iter().any(|(_, _, f)| *f) {
            s.focused = focused;
            s.focus_seen = true;
        }
        let id = s.next_id;
        s.next_id += 1;
        s.listeners.push((id, Rc::new(listener), focus));
        id
    };
    schedule(&store, window, cx);
    KeyboardSubscription {
        store: Rc::downgrade(&store),
        id,
    }
}

fn listeners(store: &Rc<RefCell<Store>>, focus_only: bool) -> Vec<KeyboardListener> {
    store
        .borrow()
        .listeners
        .iter()
        .filter(|(_, _, focus)| !focus_only || *focus)
        .map(|(_, l, _)| l.clone())
        .collect()
}

fn deliver(store: &Rc<RefCell<Store>>, height: f64, time: f64, window: &mut Window, cx: &mut App) {
    {
        let mut s = store.borrow_mut();
        if height <= KEYBOARD_EPSILON && s.height > KEYBOARD_EPSILON {
            s.starts_at_hide = Some(s.starts);
        }
        s.height = height;
    }
    let report = KeyboardHeightEvent::Viewport { height, time };
    for listener in listeners(store, false) {
        listener(&report, window, cx);
    }
}

/// Whether a report of `height` is one step of a hide: under Remove animations, Chrome on
/// Android reports a hide as several lower keyboards still up before the report of none.
fn is_hide_step(shown: f64, height: f64, cx: &App) -> bool {
    height > KEYBOARD_EPSILON
        && height < shown - KEYBOARD_EPSILON
        && has_virtual_keyboard(cx)
        && crate::prefers_reduced_motion(cx)
}

fn on_viewport(store: &Rc<RefCell<Store>>, window: &mut Window, cx: &mut App) {
    let height = measure_keyboard_height(window);
    let now = keyboard_now(cx);
    let shown = store.borrow().height;
    if is_hide_step(shown, height, cx) {
        let mut s = store.borrow_mut();
        // A pan reports the same step again, and leaves the wait as it is.
        if s.step
            .as_ref()
            .is_some_and(|step| (height - step.height).abs() < KEYBOARD_EPSILON)
        {
            return;
        }
        let limit = s
            .step
            .as_ref()
            .map_or(now + HIDE_STEP_LIMIT_MS, |step| step.limit);
        let wait = HIDE_STEP_WAIT_MS.min(limit - now).max(0.);
        let weak = Rc::downgrade(store);
        // The report of none, a report back up, or a quiet spell ends the wait.
        let timer = window.spawn(cx, async move |cx| {
            cx.background_executor()
                .timer(Duration::from_secs_f64(wait / 1000.))
                .await;
            cx.update(|window, cx| {
                let Some(store) = weak.upgrade() else { return };
                store.borrow_mut().step = None;
                let now = keyboard_now(cx);
                deliver(&store, measure_keyboard_height(window), now, window, cx);
            })
            .ok();
        });
        s.step = Some(StepHold {
            height,
            limit,
            _timer: timer,
        });
        return;
    }
    store.borrow_mut().step = None;
    deliver(store, height, now, window, cx);
}

/// After a frame: the viewport's change and focus's move, reported.
fn process(store: &Rc<RefCell<Store>>, window: &mut Window, cx: &mut App) {
    if keeps_fields(store) {
        super::sync_keeper(window, cx);
    }
    let (attached, focus_listening) = {
        let mut s = store.borrow_mut();
        s.scheduled = false;
        // The fields drawn this frame are the ones every reader sees until the next.
        if !s.frame_fields.is_empty() || !s.fields.is_empty() {
            let frame = std::mem::take(&mut s.frame_fields);
            s.fields = frame;
        }
        (s.attached, s.listeners.iter().any(|(_, _, focus)| *focus))
    };
    if !attached {
        return;
    }
    debug_assert_eq!(store.borrow().window, window.window_handle().window_id());
    let sample = (
        window.visual_viewport_bounds(),
        window.insets(),
        window.viewport_size(),
    );
    let changed = store.borrow().sampled.as_ref() != Some(&sample);
    if focus_listening {
        let now_focused = window.focused(cx);
        let (was, seen) = {
            let s = store.borrow();
            (s.focused.clone(), s.focus_seen)
        };
        if seen && was != now_focused {
            let (target_of, height) = {
                let s = store.borrow();
                let lookup = |handle: &Option<FocusHandle>| match handle {
                    None => FocusTarget::Nothing,
                    Some(handle) => s
                        .fields
                        .iter()
                        .find(|f| &f.focus == handle)
                        .cloned()
                        .map_or(FocusTarget::Element, FocusTarget::Field),
                };
                ((lookup(&was), lookup(&now_focused)), s.height)
            };
            {
                let mut s = store.borrow_mut();
                s.focused = now_focused.clone();
            }
            let time = keyboard_now(cx);
            let (old, new) = target_of;
            if was.is_some() {
                let event = KeyboardHeightEvent::FocusOut {
                    height,
                    time,
                    target: old.clone(),
                    related: new.clone(),
                };
                for listener in listeners(store, true) {
                    listener(&event, window, cx);
                }
            }
            if now_focused.is_some() {
                let event = KeyboardHeightEvent::FocusIn {
                    height,
                    time,
                    target: new,
                    related: old,
                };
                for listener in listeners(store, true) {
                    listener(&event, window, cx);
                }
            }
        } else if !seen {
            let mut s = store.borrow_mut();
            s.focused = now_focused;
            s.focus_seen = true;
        }
    }
    if changed {
        store.borrow_mut().sampled = Some(sample);
        on_viewport(store, window, cx);
    }
}

/// How many listeners the window's keyboard module has (tests: one sampling per window).
#[doc(hidden)]
pub fn listener_count(window: &mut Window, cx: &mut App) -> (usize, usize) {
    let store = store_of(window, cx);
    let s = store.borrow();
    (
        s.listeners.len(),
        s.listeners.iter().filter(|(_, _, f)| *f).count(),
    )
}
