//! The base every recognizer builds on, upstream `gestures/GestureRecognizer.ts`
//! (`UIGestureRecognizer`) and `gestures/recognizerInternals.ts`.
//!
//! The state machine is UIKit's:
//!
//! ```text
//! possible ─► began ─► changed* ─► ended | cancelled     (continuous)
//! possible ─► ended                                       (discrete: "recognized")
//! possible ─► failed
//! ```
//!
//! A recognizer in a terminal state ignores the touches it still has and
//! returns to `possible` (its `reset` runs) once all of them have ended.
//! Listeners (UIKit's target-action) run for began, changed, ended and
//! cancelled, never for failed, synchronously inside the input.
//!
//! A recognizer is a shared handle ([`AnyRecognizer`], or the typed
//! [`Recognizer<B>`]) over the common state and a [`RecognizerBehavior`],
//! the subclass: it interprets touches and asks for state changes, and the
//! arena decides whether a change is allowed (require-to-fail, exclusivity,
//! should-begin) and applies it. A behavior's request made inside one of its
//! own handlers is carried out as the handler returns, before the arena
//! delivers the same input to the next recognizer, so the outcome is
//! upstream's.

use std::any::{Any, TypeId};
use std::cell::{Cell, Ref, RefCell, RefMut};
use std::collections::VecDeque;
use std::fmt;
use std::marker::PhantomData;
use std::ops::Deref;
use std::rc::{Rc, Weak};

use super::arena::{ArenaInner, GestureArena};
use super::selection_lock::SelectionLock;
use super::types::{
    DefaultEnvironment, GestureEnvironment, GestureEventContext, GestureState, GestureTouch, Point,
    PointerKind, ScrollType, ViewKey,
};

/// The browser behaviors a touch-action can keep or remove: what the window
/// may still do natively on a recognizer's view.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NativeTouchBehavior {
    pub pan_x: bool,
    pub pan_y: bool,
    pub pinch_zoom: bool,
    /// Double-tap to zoom.
    pub double_tap_zoom: bool,
}

/// Everything left to the window.
pub const ALL_NATIVE_TOUCH: NativeTouchBehavior = NativeTouchBehavior {
    pan_x: true,
    pan_y: true,
    pinch_zoom: true,
    double_tap_zoom: true,
};

/// A tap-like recognizer only needs double-tap zoom gone.
pub const MANIPULATION: NativeTouchBehavior = NativeTouchBehavior {
    pan_x: true,
    pan_y: true,
    pinch_zoom: true,
    double_tap_zoom: false,
};

/// What a recognizer is. Code tells the family apart by it
/// (`architecture:react-component-runtime` INV13).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RecognizerKind {
    Tap,
    LongPress,
    Pan,
    ScreenEdgePan,
    Swipe,
    Pinch,
}

impl RecognizerKind {
    /// Upstream's name for the kind.
    pub fn name(self) -> &'static str {
        match self {
            Self::Tap => "tap",
            Self::LongPress => "longPress",
            Self::Pan => "pan",
            Self::ScreenEdgePan => "screenEdgePan",
            Self::Swipe => "swipe",
            Self::Pinch => "pinch",
        }
    }
}

/// The kinds of recognizer the host and touch behavior treat specially
/// (upstream `RecognizerFamily`, a brand a subclass inherits).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RecognizerFamily {
    Tap,
    LongPress,
    Pan,
    Pinch,
}

/// A pan's direction rule for a translation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PanVerdict {
    Begin,
    Fail,
    Wait,
}

/// A recognition waiting for a failure requirement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PendingRecognition {
    Began,
    Ended,
}

/// A delegate asked about one recognizer.
pub type RecognizerDelegate = Rc<dyn Fn(&AnyRecognizer) -> bool>;
/// A delegate asked about a recognizer and another.
pub type PairDelegate = Rc<dyn Fn(&AnyRecognizer, &AnyRecognizer) -> bool>;
/// A delegate asked about a recognizer and a touch.
pub type TouchDelegate = Rc<dyn Fn(&AnyRecognizer, &GestureTouch) -> bool>;
/// A target-action listener.
pub type Listener = Rc<dyn Fn(&AnyRecognizer)>;
/// A listener for every transition, failed and the reset included.
pub type TransitionListener = Rc<dyn Fn(&AnyRecognizer, GestureState, GestureState)>;

/// Upstream `GestureRecognizerOptions`.
#[derive(Clone)]
pub struct RecognizerOptions {
    /// A name for debugging and test output; the recognizer's type by default.
    pub name: Option<String>,
    /// A disabled recognizer receives no touches. @default true
    pub is_enabled: bool,
    /// Recognizing cancels the view's touches. @default true
    pub cancels_touches_in_view: bool,
    /// Hold the view's touch-began until this recognizer fails.
    pub delays_touches_began: bool,
    /// Hold the view's touch-ended (and the click) while this may recognize.
    /// @default true
    pub delays_touches_ended: bool,
    /// The lock a hold that does something takes for each held press
    /// (FR41). `None` takes the recognizer's own default (a long press with
    /// a hold brings one); `Some(None)` brings none.
    pub selection_lock: Option<Option<SelectionLock>>,
    /// Pointer kinds this recognizer listens to; all by default.
    pub allowed_pointer_types: Option<Vec<PointerKind>>,
    /// Scroll input it takes (`allowedScrollTypesMask`); the recognizer's
    /// default (none, or both for a pinch) when unset.
    pub allowed_scroll_types: Option<Vec<ScrollType>>,
    /// Ignore a touch of another kind than the ones tracked. @default true
    pub requires_exclusive_touch_type: bool,
    /// `false` makes a watcher: it never claims, prevents nothing and is
    /// prevented by nothing. @default true
    pub claims_touches: bool,
    /// `gestureRecognizerShouldBegin`.
    pub should_begin: Option<RecognizerDelegate>,
    /// `gestureRecognizer(_:shouldReceive:)`.
    pub should_receive_touch: Option<TouchDelegate>,
    /// `gestureRecognizer(_:shouldRecognizeSimultaneouslyWith:)`.
    pub should_recognize_simultaneously: Option<PairDelegate>,
    /// `gestureRecognizer(_:shouldRequireFailureOf:)`.
    pub should_require_failure_of: Option<PairDelegate>,
    /// `gestureRecognizer(_:shouldBeRequiredToFailBy:)`.
    pub should_be_required_to_fail_by: Option<PairDelegate>,
}

impl Default for RecognizerOptions {
    fn default() -> Self {
        Self {
            name: None,
            is_enabled: true,
            cancels_touches_in_view: true,
            delays_touches_began: false,
            delays_touches_ended: true,
            selection_lock: None,
            allowed_pointer_types: None,
            allowed_scroll_types: None,
            requires_exclusive_touch_type: true,
            claims_touches: true,
            should_begin: None,
            should_receive_touch: None,
            should_recognize_simultaneously: None,
            should_require_failure_of: None,
            should_be_required_to_fail_by: None,
        }
    }
}

impl RecognizerOptions {
    /// Options with a name.
    pub fn named(name: impl Into<String>) -> Self {
        Self {
            name: Some(name.into()),
            ..Self::default()
        }
    }
}

/// What a recognizer does with its touches: the subclass of upstream's
/// `GestureRecognizer`. Every handler gets the recognizer's handle, through
/// which it reads its touches and state and asks for changes.
pub trait RecognizerBehavior: AsAny {
    /// What this recognizer is.
    fn kind(&self) -> RecognizerKind;
    /// The family the host treats specially, if any.
    fn family(&self) -> Option<RecognizerFamily> {
        None
    }
    /// Discrete recognizers go from possible straight to ended.
    fn is_discrete(&self) -> bool {
        false
    }
    /// What the window may still do natively on this recognizer's view.
    fn native_touch_behavior(&self, _recognizer: &AnyRecognizer) -> NativeTouchBehavior {
        ALL_NATIVE_TOUCH
    }
    /// The scroll types taken when the options name none.
    fn default_scroll_types(&self) -> Vec<ScrollType> {
        Vec::new()
    }
    /// Whether a touch may join; the base checks enabled, pointer kind and
    /// exclusivity of kind.
    fn accepts_touch(&self, recognizer: &AnyRecognizer, touch: &GestureTouch) -> bool {
        recognizer.base_accepts_touch(touch)
    }
    /// New touches for this recognizer.
    fn touches_began(
        &mut self,
        _r: &AnyRecognizer,
        _touches: &[GestureTouch],
        _event: &GestureEventContext,
    ) {
    }
    fn touches_moved(
        &mut self,
        _r: &AnyRecognizer,
        _touches: &[GestureTouch],
        _event: &GestureEventContext,
    ) {
    }
    /// Touches lifted; they leave the tracked touches after this returns.
    fn touches_ended(
        &mut self,
        _r: &AnyRecognizer,
        _touches: &[GestureTouch],
        _event: &GestureEventContext,
    ) {
    }
    /// The system or the window took these touches. By default an active
    /// gesture is cancelled and a possible one fails.
    fn touches_cancelled(
        &mut self,
        r: &AnyRecognizer,
        _touches: &[GestureTouch],
        _event: &GestureEventContext,
    ) {
        default_touches_cancelled(r);
    }
    /// The window took these touches for its own pan or zoom. Return true to
    /// keep observing them; by default a claiming recognizer treats them as
    /// cancelled and a watcher follows them to the lift.
    fn touches_taken_by_browser(
        &mut self,
        r: &AnyRecognizer,
        touches: &[GestureTouch],
        event: &GestureEventContext,
    ) -> bool {
        if !r.claims_touches() {
            return true;
        }
        self.touches_cancelled(r, touches, event);
        false
    }
    /// A touch began off this recognizer's hit path while it tracks its own.
    fn touch_began_elsewhere(
        &mut self,
        _r: &AnyRecognizer,
        _touches: &[GestureTouch],
        _event: &GestureEventContext,
    ) {
    }
    /// The timer armed with [`AnyRecognizer::set_timer`] came due.
    fn timer_fired(&mut self, _r: &AnyRecognizer, _tag: u32) {}
    /// Clears per-gesture state before the next attempt.
    fn reset(&mut self) {}
    /// Whether this recognizer holds `touch_id` before it begins.
    fn keeps_touch(&self, _r: &AnyRecognizer, _touch_id: u64) -> bool {
        false
    }
    /// A pan's direction rule, for the host.
    fn classify(&self, _translation: Point) -> Option<PanVerdict> {
        None
    }
    /// `canPrevent(_:)`.
    fn can_prevent(&self, _other: &AnyRecognizer) -> bool {
        true
    }
    /// `canBePrevented(by:)`.
    fn can_be_prevented(&self, _other: &AnyRecognizer) -> bool {
        true
    }
}

/// Upcasting to `Any`, implemented for every type.
pub trait AsAny: Any {
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

impl<T: Any> AsAny for T {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

/// The base's `touchesCancelled`: cancel an active gesture, fail a possible one.
pub fn default_touches_cancelled(r: &AnyRecognizer) {
    let state = r.state();
    if state.is_active() {
        r.set_state(GestureState::Cancelled);
    } else if state == GestureState::Possible {
        r.set_state(GestureState::Failed);
    }
}

pub(crate) struct Base {
    pub name: String,
    pub is_enabled: bool,
    pub cancels_touches_in_view: bool,
    pub selection_lock: Option<SelectionLock>,
    pub delays_touches_began: bool,
    pub delays_touches_ended: bool,
    pub allowed_pointer_types: Option<Vec<PointerKind>>,
    pub allowed_scroll_types: Vec<ScrollType>,
    pub requires_exclusive_touch_type: bool,
    pub claims_touches: bool,
    pub should_begin: Option<RecognizerDelegate>,
    pub should_receive_touch: Option<TouchDelegate>,
    pub should_recognize_simultaneously: Option<PairDelegate>,
    pub should_require_failure_of: Option<PairDelegate>,
    pub should_be_required_to_fail_by: Option<PairDelegate>,
    pub view: Option<ViewKey>,
    pub arena: Option<Weak<ArenaInner>>,
    pub environment: Option<Rc<dyn GestureEnvironment>>,
    pub pending: Option<PendingRecognition>,
    pub state: GestureState,
    pub listeners: Vec<(u64, Listener)>,
    pub transition_listeners: Vec<(u64, TransitionListener)>,
    pub next_listener: u64,
    pub failure_requirements: Vec<WeakRecognizer>,
    pub simultaneous_with: Vec<WeakRecognizer>,
    pub touches: Vec<GestureTouch>,
    pub seen_touch_ids: Vec<u64>,
    pub last_location: Point,
    pub waiting: bool,
    pub on_enabled_change: Option<Rc<dyn Fn()>>,
}

enum Queued {
    State(GestureState),
    Timer(u32),
    CancelView,
    Reset,
}

pub(crate) struct Node<B: ?Sized> {
    id: u64,
    type_id: TypeId,
    kind: RecognizerKind,
    family: Option<RecognizerFamily>,
    discrete: bool,
    base: RefCell<Base>,
    busy: Cell<u32>,
    queue: RefCell<VecDeque<Queued>>,
    behavior: RefCell<B>,
}

thread_local! {
    static NEXT_ID: Cell<u64> = const { Cell::new(1) };
}

/// A recognizer, type-erased: the handle the arena, the host and listeners
/// share. Cloning shares the recognizer.
#[derive(Clone)]
pub struct AnyRecognizer(Rc<Node<dyn RecognizerBehavior>>);

/// A weak handle to a recognizer.
#[derive(Clone)]
pub struct WeakRecognizer(Weak<Node<dyn RecognizerBehavior>>);

impl WeakRecognizer {
    pub fn upgrade(&self) -> Option<AnyRecognizer> {
        self.0.upgrade().map(AnyRecognizer)
    }
    fn is(&self, other: &AnyRecognizer) -> bool {
        Weak::ptr_eq(&self.0, &Rc::downgrade(&other.0))
    }
}

impl PartialEq for AnyRecognizer {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for AnyRecognizer {}

impl fmt::Debug for AnyRecognizer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.name(), self.state())
    }
}

/// A recognizer of a known behavior: the typed handle, which derefs to
/// [`AnyRecognizer`] for the common API.
pub struct Recognizer<B>(AnyRecognizer, PhantomData<fn() -> B>);

impl<B> Clone for Recognizer<B> {
    fn clone(&self) -> Self {
        Self(self.0.clone(), PhantomData)
    }
}

impl<B> Deref for Recognizer<B> {
    type Target = AnyRecognizer;
    fn deref(&self) -> &AnyRecognizer {
        &self.0
    }
}

impl<B> PartialEq for Recognizer<B> {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl<B> fmt::Debug for Recognizer<B> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl<B: RecognizerBehavior> Recognizer<B> {
    /// A recognizer with `behavior` and the common `options`.
    pub fn with_behavior(options: RecognizerOptions, behavior: B, default_name: &str) -> Self {
        let id = NEXT_ID.with(|next| {
            let id = next.get();
            next.set(id + 1);
            id
        });
        let allowed_scroll_types = options
            .allowed_scroll_types
            .clone()
            .unwrap_or_else(|| behavior.default_scroll_types());
        let base = Base {
            name: options.name.unwrap_or_else(|| default_name.to_string()),
            is_enabled: options.is_enabled,
            cancels_touches_in_view: options.cancels_touches_in_view,
            selection_lock: options.selection_lock.flatten(),
            delays_touches_began: options.delays_touches_began,
            delays_touches_ended: options.delays_touches_ended,
            allowed_pointer_types: options.allowed_pointer_types,
            allowed_scroll_types,
            requires_exclusive_touch_type: options.requires_exclusive_touch_type,
            claims_touches: options.claims_touches,
            should_begin: options.should_begin,
            should_receive_touch: options.should_receive_touch,
            should_recognize_simultaneously: options.should_recognize_simultaneously,
            should_require_failure_of: options.should_require_failure_of,
            should_be_required_to_fail_by: options.should_be_required_to_fail_by,
            view: None,
            arena: None,
            environment: None,
            pending: None,
            state: GestureState::Possible,
            listeners: Vec::new(),
            transition_listeners: Vec::new(),
            next_listener: 1,
            failure_requirements: Vec::new(),
            simultaneous_with: Vec::new(),
            touches: Vec::new(),
            seen_touch_ids: Vec::new(),
            last_location: Point::ZERO,
            waiting: false,
            on_enabled_change: None,
        };
        let node: Rc<Node<B>> = Rc::new(Node {
            id,
            type_id: TypeId::of::<B>(),
            kind: behavior.kind(),
            family: behavior.family(),
            discrete: behavior.is_discrete(),
            base: RefCell::new(base),
            busy: Cell::new(0),
            queue: RefCell::new(VecDeque::new()),
            behavior: RefCell::new(behavior),
        });
        let node: Rc<Node<dyn RecognizerBehavior>> = node;
        Self(AnyRecognizer(node), PhantomData)
    }

    /// The type-erased handle.
    pub fn any(&self) -> &AnyRecognizer {
        &self.0
    }

    /// The behavior, to read its fields and accessors.
    pub fn behavior(&self) -> Ref<'_, B> {
        Ref::map(self.0.0.behavior.borrow(), |b| {
            b.as_any()
                .downcast_ref::<B>()
                .expect("a recognizer of this behavior")
        })
    }

    /// The behavior, to change its settings between gestures.
    pub fn behavior_mut(&self) -> RefMut<'_, B> {
        RefMut::map(self.0.0.behavior.borrow_mut(), |b| {
            b.as_any_mut()
                .downcast_mut::<B>()
                .expect("a recognizer of this behavior")
        })
    }

    /// Called on began, changed, ended (recognized) and cancelled. Returns an
    /// id for [`AnyRecognizer::remove_listener`].
    pub fn add_listener(&self, listener: impl Fn(&Recognizer<B>) + 'static) -> u64 {
        self.0.add_any_listener(Rc::new(move |any: &AnyRecognizer| {
            listener(&Recognizer(any.clone(), PhantomData));
        }))
    }

    /// Called on every transition, failed and the return to possible included.
    pub fn add_transition_listener(
        &self,
        listener: impl Fn(&Recognizer<B>, GestureState, GestureState) + 'static,
    ) -> u64 {
        self.0
            .add_any_transition_listener(Rc::new(move |any: &AnyRecognizer, from, to| {
                listener(&Recognizer(any.clone(), PhantomData), from, to);
            }))
    }
}

impl AnyRecognizer {
    fn base(&self) -> Ref<'_, Base> {
        self.0.base.borrow()
    }

    fn base_mut(&self) -> RefMut<'_, Base> {
        self.0.base.borrow_mut()
    }

    /// This recognizer as a typed handle, when its behavior is `B`.
    pub fn downcast<B: RecognizerBehavior>(&self) -> Option<Recognizer<B>> {
        (self.0.type_id == TypeId::of::<B>()).then(|| Recognizer(self.clone(), PhantomData))
    }

    /// Whether its behavior is `B`.
    pub fn is<B: RecognizerBehavior>(&self) -> bool {
        self.0.type_id == TypeId::of::<B>()
    }

    /// A weak handle.
    pub fn downgrade(&self) -> WeakRecognizer {
        WeakRecognizer(Rc::downgrade(&self.0))
    }

    /// A stable id, for ordering and debugging.
    pub fn id(&self) -> u64 {
        self.0.id
    }

    pub fn name(&self) -> String {
        self.base().name.clone()
    }

    pub fn set_name(&self, name: impl Into<String>) {
        self.base_mut().name = name.into();
    }

    /// What this recognizer is.
    pub fn kind(&self) -> RecognizerKind {
        self.0.kind
    }

    /// The family the host treats specially.
    pub fn family(&self) -> Option<RecognizerFamily> {
        self.0.family
    }

    pub fn state(&self) -> GestureState {
        self.base().state
    }

    pub fn is_discrete(&self) -> bool {
        self.0.discrete
    }

    pub fn is_enabled(&self) -> bool {
        self.base().is_enabled
    }

    pub fn cancels_touches_in_view(&self) -> bool {
        self.base().cancels_touches_in_view
    }

    pub fn set_cancels_touches_in_view(&self, value: bool) {
        self.base_mut().cancels_touches_in_view = value;
    }

    pub fn delays_touches_began(&self) -> bool {
        self.base().delays_touches_began
    }

    pub fn set_delays_touches_began(&self, value: bool) {
        self.base_mut().delays_touches_began = value;
    }

    pub fn delays_touches_ended(&self) -> bool {
        self.base().delays_touches_ended
    }

    pub fn set_delays_touches_ended(&self, value: bool) {
        self.base_mut().delays_touches_ended = value;
    }

    pub fn allowed_pointer_types(&self) -> Option<Vec<PointerKind>> {
        self.base().allowed_pointer_types.clone()
    }

    pub fn set_allowed_pointer_types(&self, kinds: Option<Vec<PointerKind>>) {
        self.base_mut().allowed_pointer_types = kinds;
    }

    pub fn allowed_scroll_types(&self) -> Vec<ScrollType> {
        self.base().allowed_scroll_types.clone()
    }

    pub fn allows_scroll_type(&self, scroll_type: ScrollType) -> bool {
        self.base().allowed_scroll_types.contains(&scroll_type)
    }

    pub fn requires_exclusive_touch_type(&self) -> bool {
        self.base().requires_exclusive_touch_type
    }

    pub fn set_requires_exclusive_touch_type(&self, value: bool) {
        self.base_mut().requires_exclusive_touch_type = value;
    }

    /// Whether recognizing takes the touches from the window (false for a
    /// watcher).
    pub fn claims_touches(&self) -> bool {
        self.base().claims_touches
    }

    /// The lock a hold that does something takes for each held press.
    pub fn selection_lock(&self) -> Option<SelectionLock> {
        self.base().selection_lock.clone()
    }

    /// The view the recognizer is attached to; the host sets it.
    pub fn view(&self) -> Option<ViewKey> {
        self.base().view
    }

    pub fn set_view(&self, view: Option<ViewKey>) {
        self.base_mut().view = view;
    }

    /// What the window may still do natively on this recognizer's view.
    pub fn native_touch_behavior(&self) -> NativeTouchBehavior {
        self.with_behavior(|b| b.native_touch_behavior(self))
            .unwrap_or(ALL_NATIVE_TOUCH)
    }

    /// Whether this recognizer listens to touch at all.
    pub fn accepts_touch_input(&self) -> bool {
        self.base()
            .allowed_pointer_types
            .as_ref()
            .is_none_or(|kinds| kinds.contains(&PointerKind::Touch))
    }

    /// How many touches it tracks now.
    pub fn number_of_touches(&self) -> usize {
        self.base().touches.len()
    }

    /// The touches being tracked, in the order they began.
    pub fn tracked_touches(&self) -> Vec<GestureTouch> {
        self.base().touches.clone()
    }

    /// Whether it tracks the touch with this id.
    pub fn tracks(&self, touch_id: u64) -> bool {
        self.base().touches.iter().any(|t| t.id() == touch_id)
    }

    /// The touch it tracks with this id.
    pub fn tracked_touch(&self, touch_id: u64) -> Option<GestureTouch> {
        self.base()
            .touches
            .iter()
            .find(|t| t.id() == touch_id)
            .cloned()
    }

    /// Every touch id received since the last reset.
    pub fn seen_touch_ids(&self) -> Vec<u64> {
        self.base().seen_touch_ids.clone()
    }

    pub(crate) fn has_seen(&self, touch_id: u64) -> bool {
        self.base().seen_touch_ids.contains(&touch_id)
    }

    /// The centroid of the tracked touches; after they end, where they were.
    pub fn location(&self) -> Point {
        let mut base = self.base_mut();
        if base.touches.is_empty() {
            return base.last_location;
        }
        let n = base.touches.len() as f64;
        let (mut x, mut y) = (0., 0.);
        for touch in &base.touches {
            let at = touch.position();
            x += at.x;
            y += at.y;
        }
        base.last_location = Point::new(x / n, y / n);
        base.last_location
    }

    /// A recognition waiting for a failure requirement.
    pub fn pending(&self) -> Option<PendingRecognition> {
        self.base().pending
    }

    pub(crate) fn set_pending(&self, pending: Option<PendingRecognition>) {
        self.base_mut().pending = pending;
    }

    /// For behaviors: whether the recognizer expects more input with no
    /// touches down (a multi-tap between taps), which keeps it involved.
    pub fn set_waiting(&self, waiting: bool) {
        self.base_mut().waiting = waiting;
    }

    /// Whether it is part of the current interaction: it tracks touches,
    /// waits on a requirement, or expects more input. An idle possible
    /// recognizer holds up nothing that requires it to fail.
    pub fn is_involved(&self) -> bool {
        let base = self.base();
        !base.touches.is_empty() || base.pending.is_some() || base.waiting
    }

    /// The arena delivering touches to it, if any.
    pub fn arena(&self) -> Option<GestureArena> {
        self.base()
            .arena
            .as_ref()
            .and_then(Weak::upgrade)
            .map(GestureArena::from_inner)
    }

    pub(crate) fn set_arena(&self, arena: &GestureArena) {
        self.base_mut().arena = Some(arena.downgrade_inner());
    }

    /// The environment a host gives a recognizer outside an arena.
    pub fn set_environment(&self, environment: Option<Rc<dyn GestureEnvironment>>) {
        self.base_mut().environment = environment;
    }

    /// Facts about the window (viewport, writing direction).
    pub fn environment(&self) -> Rc<dyn GestureEnvironment> {
        if let Some(arena) = self.arena() {
            return arena.environment();
        }
        self.base()
            .environment
            .clone()
            .unwrap_or_else(|| Rc::new(DefaultEnvironment))
    }

    /// The event clock's current time.
    pub fn now(&self) -> f64 {
        self.arena().map_or(0., |arena| arena.now())
    }

    // --- Relationships ---------------------------------------------------

    /// UIKit's `require(toFail:)`: recognize only once `other` has failed.
    pub fn require_to_fail(&self, other: &AnyRecognizer) -> &Self {
        let mut base = self.base_mut();
        if !base.failure_requirements.iter().any(|w| w.is(other)) {
            base.failure_requirements.push(other.downgrade());
        }
        self
    }

    /// Lets this and `others` be recognized at the same time (both ways).
    pub fn recognize_simultaneously_with(&self, others: &[&AnyRecognizer]) -> &Self {
        for other in others {
            {
                let mut base = self.base_mut();
                if !base.simultaneous_with.iter().any(|w| w.is(other)) {
                    base.simultaneous_with.push(other.downgrade());
                }
            }
            let mut theirs = other.base_mut();
            if !theirs.simultaneous_with.iter().any(|w| w.is(self)) {
                theirs.simultaneous_with.push(self.downgrade());
            }
        }
        self
    }

    /// Whether this recognizer must wait for `other` to fail.
    pub fn requires_failure_of(&self, other: &AnyRecognizer) -> bool {
        let (listed, mine) = {
            let base = self.base();
            (
                base.failure_requirements.iter().any(|w| w.is(other)),
                base.should_require_failure_of.clone(),
            )
        };
        if listed {
            return true;
        }
        if mine.is_some_and(|delegate| delegate(self, other)) {
            return true;
        }
        let theirs = other.base().should_be_required_to_fail_by.clone();
        theirs.is_some_and(|delegate| delegate(other, self))
    }

    /// Whether this and `other` may be active together: both allow it.
    pub fn allows_simultaneous(&self, other: &AnyRecognizer) -> bool {
        let (listed, mine) = {
            let base = self.base();
            (
                base.simultaneous_with.iter().any(|w| w.is(other)),
                base.should_recognize_simultaneously.clone(),
            )
        };
        if listed {
            return true;
        }
        let theirs = other.base().should_recognize_simultaneously.clone();
        mine.is_some_and(|delegate| delegate(self, other))
            && theirs.is_some_and(|delegate| delegate(other, self))
    }

    /// The delegate's should-begin answer.
    pub fn should_begin(&self) -> bool {
        let delegate = self.base().should_begin.clone();
        delegate.is_none_or(|delegate| delegate(self))
    }

    /// Whether this recognizer holds `touch_id` before it begins.
    pub fn keeps_touch(&self, touch_id: u64) -> bool {
        self.with_behavior(|b| b.keeps_touch(self, touch_id))
            .unwrap_or(false)
    }

    /// A pan's direction rule for a translation.
    pub fn classify(&self, translation: Point) -> Option<PanVerdict> {
        self.with_behavior(|b| b.classify(translation)).flatten()
    }

    /// `canPrevent(_:)`.
    pub fn can_prevent(&self, other: &AnyRecognizer) -> bool {
        self.with_behavior(|b| b.can_prevent(other)).unwrap_or(true)
    }

    /// `canBePrevented(by:)`.
    pub fn can_be_prevented(&self, other: &AnyRecognizer) -> bool {
        self.with_behavior(|b| b.can_be_prevented(other))
            .unwrap_or(true)
    }

    // --- Listeners --------------------------------------------------------

    pub(crate) fn add_any_listener(&self, listener: Listener) -> u64 {
        let mut base = self.base_mut();
        let id = base.next_listener;
        base.next_listener += 1;
        base.listeners.push((id, listener));
        id
    }

    pub(crate) fn add_any_transition_listener(&self, listener: TransitionListener) -> u64 {
        let mut base = self.base_mut();
        let id = base.next_listener;
        base.next_listener += 1;
        base.transition_listeners.push((id, listener));
        id
    }

    /// A target-action listener on the erased handle.
    pub fn on_action(&self, listener: impl Fn(&AnyRecognizer) + 'static) -> u64 {
        self.add_any_listener(Rc::new(listener))
    }

    /// A transition listener on the erased handle.
    pub fn on_transition(
        &self,
        listener: impl Fn(&AnyRecognizer, GestureState, GestureState) + 'static,
    ) -> u64 {
        self.add_any_transition_listener(Rc::new(listener))
    }

    /// Removes a listener either kind of add returned.
    pub fn remove_listener(&self, id: u64) {
        let mut base = self.base_mut();
        base.listeners.retain(|(l, _)| *l != id);
        base.transition_listeners.retain(|(l, _)| *l != id);
    }

    // --- Enabled and cancel -----------------------------------------------

    /// The host's hook: listeners follow whether anything is enabled.
    pub fn set_on_enabled_change(&self, hook: Option<Rc<dyn Fn()>>) {
        self.base_mut().on_enabled_change = hook;
    }

    /// UIKit's `isEnabled` setter: turning it off mid-gesture cancels an
    /// active gesture or fails a possible one.
    pub fn set_enabled(&self, value: bool) {
        let (changed, hook) = {
            let mut base = self.base_mut();
            let changed = base.is_enabled != value;
            base.is_enabled = value;
            (changed, base.on_enabled_change.clone())
        };
        if changed && let Some(hook) = hook {
            hook();
        }
        if !value && (self.state() != GestureState::Possible || self.is_involved()) {
            self.cancel();
        }
    }

    /// Cancels this recognizer if active, or fails it if possible.
    pub fn cancel(&self) {
        if let Some(arena) = self.arena() {
            arena.stop(self);
        } else if self.state().is_active() {
            self.apply_state(GestureState::Cancelled);
        } else if self.state() == GestureState::Possible && self.is_involved() {
            self.apply_state(GestureState::Failed);
        }
    }

    // --- For behaviors ----------------------------------------------------

    /// Asks for a state change. The arena may refuse it (the recognizer
    /// fails instead) or defer it until a failure requirement resolves.
    /// Asked inside one of the recognizer's own handlers, it is carried out
    /// as the handler returns.
    pub fn set_state(&self, next: GestureState) {
        if self.0.busy.get() > 0 {
            self.0.queue.borrow_mut().push_back(Queued::State(next));
            return;
        }
        self.request_state(next);
    }

    fn request_state(&self, next: GestureState) {
        match self.arena() {
            Some(arena) => arena.request_transition(self, next),
            None => self.apply_state(next),
        }
    }

    /// Arms the recognizer's one timer at an absolute time on the event
    /// clock; [`RecognizerBehavior::timer_fired`] hears it with `tag`.
    pub fn set_timer(&self, deadline: f64, tag: u32) {
        if let Some(arena) = self.arena() {
            arena.set_timer(self, deadline, tag);
        }
    }

    pub fn clear_timer(&self) {
        if let Some(arena) = self.arena() {
            arena.clear_timer(self);
        }
    }

    /// Takes this recognizer's touches from the view, as a claiming winner
    /// does: the view's press ends and no click follows; the touches stay
    /// unclaimed.
    pub fn cancel_touches_in_view(&self) {
        if self.0.busy.get() > 0 {
            self.0.queue.borrow_mut().push_back(Queued::CancelView);
            return;
        }
        if let Some(arena) = self.arena() {
            arena.cancel_touches_in_view_of(self);
        }
    }

    /// The base's touch acceptance: enabled, pointer kind, exclusivity of
    /// kind and the `shouldReceiveTouch` delegate.
    pub fn base_accepts_touch(&self, touch: &GestureTouch) -> bool {
        let delegate = {
            let base = self.base();
            if !base.is_enabled {
                return false;
            }
            if let Some(scroll_type) = touch.scroll_type() {
                if !base.allowed_scroll_types.contains(&scroll_type) {
                    return false;
                }
            } else {
                if base
                    .allowed_pointer_types
                    .as_ref()
                    .is_some_and(|kinds| !kinds.contains(&touch.pointer_type()))
                {
                    return false;
                }
                if base.requires_exclusive_touch_type
                    && base
                        .touches
                        .iter()
                        .any(|existing| existing.pointer_type() != touch.pointer_type())
                {
                    return false;
                }
            }
            base.should_receive_touch.clone()
        };
        delegate.is_none_or(|delegate| delegate(self, touch))
    }

    // --- Arena hooks --------------------------------------------------------

    /// Runs `f` on the behavior, unless one of its handlers is running.
    fn with_behavior<R>(&self, f: impl FnOnce(&dyn RecognizerBehavior) -> R) -> Option<R> {
        let behavior = self.0.behavior.try_borrow().ok()?;
        Some(f(&*behavior))
    }

    /// Runs a handler on the behavior, then carries out what it asked for.
    fn behave<R>(&self, f: impl FnOnce(&mut dyn RecognizerBehavior) -> R) -> Option<R> {
        let result = {
            let Ok(mut behavior) = self.0.behavior.try_borrow_mut() else {
                return None;
            };
            self.0.busy.set(self.0.busy.get() + 1);
            let result = f(&mut *behavior);
            self.0.busy.set(self.0.busy.get() - 1);
            result
        };
        self.drain();
        Some(result)
    }

    fn drain(&self) {
        if self.0.busy.get() > 0 {
            return;
        }
        loop {
            let next = self.0.queue.borrow_mut().pop_front();
            match next {
                None => break,
                Some(Queued::State(state)) => self.request_state(state),
                Some(Queued::Timer(tag)) => {
                    self.behave(|b| b.timer_fired(self, tag));
                }
                Some(Queued::CancelView) => {
                    if let Some(arena) = self.arena() {
                        arena.cancel_touches_in_view_of(self);
                    }
                }
                Some(Queued::Reset) => {
                    self.behave(|b| b.reset());
                }
            }
        }
    }

    pub(crate) fn fire_timer(&self, tag: u32) {
        if self.0.busy.get() > 0 {
            self.0.queue.borrow_mut().push_back(Queued::Timer(tag));
            return;
        }
        self.behave(|b| b.timer_fired(self, tag));
    }

    /// Whether this recognizer could take a contact of `kind` now (enabled,
    /// listening to the kind, and free to take one), without asking its
    /// delegates: the host's test before it claims a touch for the arena.
    pub(crate) fn could_take(&self, kind: PointerKind) -> bool {
        let base = self.base();
        base.is_enabled
            && base.pending.is_none()
            && (base.state == GestureState::Possible || base.state.is_active())
            && base
                .allowed_pointer_types
                .as_ref()
                .is_none_or(|kinds| kinds.contains(&kind))
    }

    /// Whether this recognizer takes `touch` right now.
    pub(crate) fn wants_touch(&self, touch: &GestureTouch) -> bool {
        let (state, pending) = {
            let base = self.base();
            (base.state, base.pending)
        };
        if state != GestureState::Possible && !state.is_active() {
            return false;
        }
        if pending.is_some() {
            return false;
        }
        self.with_behavior(|b| b.accepts_touch(self, touch))
            .unwrap_or(false)
    }

    fn own(&self, touches: &[GestureTouch]) -> Vec<GestureTouch> {
        let base = self.base();
        touches
            .iter()
            .filter(|touch| base.touches.iter().any(|t| t.id() == touch.id()))
            .cloned()
            .collect()
    }

    fn forget(&self, touches: &[GestureTouch]) {
        self.base_mut()
            .touches
            .retain(|t| !touches.iter().any(|gone| gone.id() == t.id()));
    }

    pub(crate) fn deliver_began(&self, touches: &[GestureTouch], event: &GestureEventContext) {
        {
            let mut base = self.base_mut();
            for touch in touches {
                if !base.touches.iter().any(|t| t.id() == touch.id()) {
                    base.touches.push(touch.clone());
                }
                if !base.seen_touch_ids.contains(&touch.id()) {
                    base.seen_touch_ids.push(touch.id());
                }
            }
        }
        self.location();
        self.behave(|b| b.touches_began(self, touches, event));
    }

    pub(crate) fn deliver_began_elsewhere(
        &self,
        touches: &[GestureTouch],
        event: &GestureEventContext,
    ) {
        if self.state().is_terminal() {
            return;
        }
        self.behave(|b| b.touch_began_elsewhere(self, touches, event));
    }

    pub(crate) fn deliver_moved(&self, touches: &[GestureTouch], event: &GestureEventContext) {
        let own = self.own(touches);
        if own.is_empty() || self.state().is_terminal() {
            return;
        }
        self.location();
        self.behave(|b| b.touches_moved(self, &own, event));
    }

    pub(crate) fn deliver_ended(&self, touches: &[GestureTouch], event: &GestureEventContext) {
        let own = self.own(touches);
        if own.is_empty() {
            return;
        }
        self.location();
        if !self.state().is_terminal() {
            self.behave(|b| b.touches_ended(self, &own, event));
        }
        self.forget(&own);
    }

    pub(crate) fn deliver_cancelled(&self, touches: &[GestureTouch], event: &GestureEventContext) {
        let own = self.own(touches);
        if own.is_empty() {
            return;
        }
        if !self.state().is_terminal() {
            self.behave(|b| b.touches_cancelled(self, &own, event));
        }
        self.forget(&own);
    }

    pub(crate) fn deliver_taken_by_browser(
        &self,
        touches: &[GestureTouch],
        event: &GestureEventContext,
    ) {
        let own = self.own(touches);
        if own.is_empty() {
            return;
        }
        if !self.state().is_terminal()
            && self
                .behave(|b| b.touches_taken_by_browser(self, &own, event))
                .unwrap_or(false)
        {
            return;
        }
        self.forget(&own);
    }

    /// Applies a transition the arena approved.
    pub(crate) fn apply_state(&self, next: GestureState) {
        let (from, listeners, transitions) = {
            let mut base = self.base_mut();
            let from = base.state;
            if from == next && next != GestureState::Changed {
                return;
            }
            if !from.allows(next) {
                return;
            }
            base.state = next;
            if next != GestureState::Possible {
                base.pending = None;
            }
            (
                from,
                base.listeners
                    .iter()
                    .map(|(_, l)| l.clone())
                    .collect::<Vec<_>>(),
                base.transition_listeners
                    .iter()
                    .map(|(_, l)| l.clone())
                    .collect::<Vec<_>>(),
            )
        };
        for listener in transitions {
            listener(self, from, next);
        }
        if matches!(
            next,
            GestureState::Began
                | GestureState::Changed
                | GestureState::Ended
                | GestureState::Cancelled
        ) {
            for listener in listeners {
                listener(self);
            }
        }
    }

    /// Returns to possible for the next attempt.
    pub(crate) fn reset_to_possible(&self) {
        self.clear_timer();
        {
            let mut base = self.base_mut();
            base.pending = None;
            base.touches.clear();
            base.seen_touch_ids.clear();
            base.waiting = false;
        }
        if self.0.busy.get() > 0 {
            self.0.queue.borrow_mut().push_back(Queued::Reset);
        } else {
            self.behave(|b| b.reset());
        }
        self.apply_state(GestureState::Possible);
    }
}
