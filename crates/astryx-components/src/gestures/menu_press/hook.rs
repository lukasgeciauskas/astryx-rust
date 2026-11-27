//! The menu press model in a window, upstream `hooks/useMenuPress.ts`: the row under the
//! release acts, the highlight follows a held pointer, a mouse opens the menu on its press
//! and drags in, and a finger opens it as UIMenu does.
//!
//! The model follows its pointer at the window (GPUI has no pointer capture; the window's
//! listeners see a press wherever it goes):
//! - **The menu** ([`MenuPress::menu`]): a press that lands inside it highlights the
//!   enabled row under it ([`MenuPress::row`]); the highlight follows the held pointer; the
//!   release acts on the row under it, as the row's own activation would. A release over
//!   the menu's chrome acts on nothing; a mouse released outside dismisses the menu, a
//!   finger leaves it open.
//! - **The trigger** ([`MenuPress::trigger`]): a mouse press opens the menu under the held
//!   pointer, and the settle rule decides its release. A finger opens it as it presses
//!   (UIMenu in a fixed bar), or once it rests the content-touch delay in a scroller (a
//!   scroll may claim it first), or after `long_press_delay_ms` when one is set; the
//!   opening finger then picks only after UIMenu's handoff travel. A tap's click still
//!   opens the menu.
//! - The click a tracked release would also send is the model's: [`MenuPress::take_stray_click`]
//!   reports it to the row or trigger that receives it, and
//!   [`MenuPress::is_trigger_click_from_press`] tells the trigger the press already
//!   opened or closed the menu.
//! - A cancel, a second pointer, a scroll that takes the finger, or the window losing
//!   focus (a finger's gesture is withdrawn to its lift) ends it with nothing acting.
//! - A pointer resting near the edge of an overflowing menu scrolls it.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use astryx_primitives::gestures::Point;
use astryx_primitives::motion::uikit::gestures::LONG_PRESS_DURATION_MS;
use astryx_primitives::motion::uikit::gestures::click::STRAY_CLICK_WINDOW;
use astryx_primitives::motion::uikit::scroll::DELAYS_CONTENT_TOUCHES_MS;
use gpui::{
    AnyElement, App, Bounds, DispatchPhase, Element, ElementId, FocusHandle, GlobalElementId,
    InspectorElementId, IntoElement, LayoutId, Modifiers, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, Pixels, ScrollEvent, ScrollHandle, ScrollSource, SharedString,
    Task, TouchEvent, TouchPhase, Window, px,
};
use web_time::Instant;

use super::gesture::{
    MenuPressInput, MenuPressOutcome, MenuPressPointerType, MenuPressState, MenuPressTarget,
    menu_press_transition,
};
use super::values::MENU_PRESS_TRIGGER_DRIFT_PX;
use crate::gestures::clock::WindowClock;
use crate::gestures::long_press::{DetectorOptions, LongPressDetector, LongPressTolerance};

/// How far from a scrolling menu's edge a resting pointer scrolls it, px.
pub const MENU_PRESS_AUTOSCROLL_ZONE_PX: f32 = 24.;
const AUTOSCROLL_STEP_PX: f32 = 6.;
const AUTOSCROLL_INTERVAL: Duration = Duration::from_millis(16);
/// How long a finger rests on the trigger before the menu opens under it, with a
/// `long_press_delay_ms` model and no delay given.
pub const MENU_PRESS_LONG_PRESS_MS: f64 = LONG_PRESS_DURATION_MS;

/// The release a row acts on: its pointer, place, button and modifiers.
#[derive(Clone, Copy, Debug)]
pub struct MenuRelease {
    pub pointer: MenuPressPointerType,
    pub position: gpui::Point<Pixels>,
    pub button: MouseButton,
    pub modifiers: Modifiers,
}

type TriggerPressFn = Rc<dyn Fn(MenuPressPointerType, &mut Window, &mut App) -> bool>;
type HighlightFn = Rc<dyn Fn(Option<SharedString>, &mut Window, &mut App)>;
type ActivateFn = Rc<dyn Fn(SharedString, &MenuRelease, &mut Window, &mut App)>;
type RowActivateFn = Rc<dyn Fn(&MenuRelease, &mut Window, &mut App)>;
type DismissFn = Rc<dyn Fn(&mut Window, &mut App)>;

/// Upstream `UseMenuPressOptions`. The menu, the trigger and the enabled rows are the
/// elements wrapped in [`MenuPress::menu`], [`MenuPress::trigger`] and [`MenuPress::row`].
#[derive(Clone, Default)]
pub struct MenuPressOptions {
    /// A mouse pressed the trigger, or a finger opened it: open the menu under the held
    /// pointer. Whether it opened; `false` (the press closed an open menu) ends the
    /// gesture.
    pub on_trigger_press: Option<TriggerPressFn>,
    /// Moves the highlight; `None` clears it. Defaults to focusing the row's handle, or the
    /// menu's with no row.
    pub on_highlight: Option<HighlightFn>,
    /// Acts on the row under the release. Defaults to the row's own activation.
    pub on_activate: Option<ActivateFn>,
    /// Closes the menu: a mouse was released outside it with nothing acting.
    pub on_dismiss: Option<DismissFn>,
    /// The scroller a resting pointer near its edge scrolls, given while the menu's rows
    /// overflow it (upstream: the menu root when it overflows); none by default. A menu
    /// with one leaves a finger's pan to the window.
    pub scroller: Option<ScrollHandle>,
    /// How long a finger rests on the trigger before the menu opens. Unset, a finger opens
    /// it as UIMenu does: at once in a fixed bar, after the content-touch delay in a
    /// scroller, then picks after the handoff travel. Set, it is that wait everywhere, and
    /// the settle rule decides the release.
    pub long_press_delay_ms: Option<f64>,
    /// Whether the model is live. @default true
    pub disabled: bool,
    /// The focus the menu takes when the highlight clears.
    pub menu_focus: Option<FocusHandle>,
    /// Upstream `pressOnItemsOnly`: a press inside the menu that lands on no row (caller
    /// content beside the rows) stays that content's.
    pub press_on_items_only: bool,
}

impl MenuPressOptions {
    pub fn on_trigger_press(
        mut self,
        f: impl Fn(MenuPressPointerType, &mut Window, &mut App) -> bool + 'static,
    ) -> Self {
        self.on_trigger_press = Some(Rc::new(f));
        self
    }

    pub fn on_highlight(
        mut self,
        f: impl Fn(Option<SharedString>, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_highlight = Some(Rc::new(f));
        self
    }

    pub fn on_activate(
        mut self,
        f: impl Fn(SharedString, &MenuRelease, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_activate = Some(Rc::new(f));
        self
    }

    pub fn on_dismiss(mut self, f: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_dismiss = Some(Rc::new(f));
        self
    }

    pub fn scroller(mut self, handle: ScrollHandle) -> Self {
        self.scroller = Some(handle);
        self
    }

    pub fn long_press_delay_ms(mut self, ms: f64) -> Self {
        self.long_press_delay_ms = Some(ms);
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn menu_focus(mut self, focus: FocusHandle) -> Self {
        self.menu_focus = Some(focus);
        self
    }

    /// Upstream `pressOnItemsOnly`.
    pub fn press_on_items_only(mut self, only: bool) -> Self {
        self.press_on_items_only = only;
        self
    }
}

/// An enabled row: its key, the focus its highlight takes and its activation.
#[derive(Clone)]
pub struct MenuPressRow {
    key: SharedString,
    focus: Option<FocusHandle>,
    activate: Option<RowActivateFn>,
}

impl MenuPressRow {
    pub fn new(key: impl Into<SharedString>) -> Self {
        Self {
            key: key.into(),
            focus: None,
            activate: None,
        }
    }

    /// The handle the highlight focuses.
    pub fn focus(mut self, focus: &FocusHandle) -> Self {
        self.focus = Some(focus.clone());
        self
    }

    /// What the row does when the release acts on it.
    pub fn on_activate(
        mut self,
        f: impl Fn(&MenuRelease, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.activate = Some(Rc::new(f));
        self
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Pointer {
    Mouse(MouseButton),
    /// A finger; `None` adopts the next finger heard (a menu opened under one).
    Touch(Option<u64>),
}

#[derive(Clone)]
struct RowEntry {
    row: MenuPressRow,
    bounds: Bounds<Pixels>,
}

#[derive(Default)]
struct Registry {
    surfaces: Vec<Bounds<Pixels>>,
    rows: Vec<RowEntry>,
    trigger: Option<Bounds<Pixels>>,
    trigger_in_scroller: bool,
}

struct Inner {
    options: RefCell<MenuPressOptions>,
    gesture: RefCell<MenuPressState<SharedString>>,
    pointer: Cell<Option<Pointer>>,
    registry: RefCell<Registry>,
    /// The finger is the model's alone now: GPUI's own recognizer no longer sees it.
    claimed: Cell<bool>,
    /// A click this gesture sends must not act, until this time (ms).
    stray_until: Cell<Option<f64>>,
    /// The trigger's click comes from a press that already opened or closed the menu.
    trigger_click: Cell<bool>,
    hold: RefCell<Option<LongPressDetector>>,
    hold_fired: Cell<bool>,
    /// The row a held finger lit.
    lit: RefCell<Option<SharedString>>,
    /// The pointer's last place, for the autoscroll's re-read.
    last: Cell<Option<gpui::Point<Pixels>>>,
    autoscroll: RefCell<Option<(i8, Task<()>)>>,
    dispatch: Cell<Option<usize>>,
    reset_pending: Cell<bool>,
    origin: Instant,
    /// The second finger that ended a gesture: it starts nothing and taps nothing.
    ignored: Cell<Option<u64>>,
    /// The window's activation, watched for its blur.
    activation: RefCell<Option<gpui::Subscription>>,
    /// The window was active as the gesture began: losing that (upstream's `blur`)
    /// withdraws it.
    was_active: Cell<bool>,
}

/// Upstream `useMenuPress`'s handle. Cheap to clone.
#[derive(Clone)]
pub struct MenuPress(Rc<Inner>);

/// Upstream `useMenuPress`: the press model of the menu `id` names, kept across frames.
/// The options of the latest frame are the ones read.
///
/// ```ignore
/// let press = use_menu_press("file-menu", MenuPressOptions::default()
///     .on_trigger_press(|_, window, cx| { open(window, cx); true })
///     .on_dismiss(|window, cx| close(window, cx)), window, cx);
/// press.trigger(button)      // the control that opens it
/// press.menu(list)           // the menu surface
/// press.row(MenuPressRow::new("copy").focus(&handle).on_activate(copy), row) // each enabled row
/// ```
pub fn use_menu_press(
    id: impl Into<ElementId>,
    options: MenuPressOptions,
    window: &mut Window,
    cx: &mut App,
) -> MenuPress {
    let id = id.into();
    let state = window.use_keyed_state(
        ElementId::Name(format!("{id}-menu-press").into()),
        cx,
        |window, cx| {
            let press = MenuPress::new(window, cx);
            press.watch_activation(window, cx);
            press
        },
    );
    let press = state.read(cx).clone();
    *press.0.options.borrow_mut() = options;
    if press.0.options.borrow().disabled {
        press.end_gesture(window, cx);
    }
    press
}

impl MenuPress {
    pub(crate) fn new(window: &mut Window, cx: &mut App) -> Self {
        let press = MenuPress(Rc::new(Inner {
            options: RefCell::default(),
            gesture: RefCell::new(MenuPressState::Idle),
            pointer: Cell::new(None),
            registry: RefCell::default(),
            claimed: Cell::new(false),
            stray_until: Cell::new(None),
            trigger_click: Cell::new(false),
            hold: RefCell::new(None),
            hold_fired: Cell::new(false),
            lit: RefCell::new(None),
            last: Cell::new(None),
            autoscroll: RefCell::new(None),
            dispatch: Cell::new(None),
            reset_pending: Cell::new(false),
            origin: cx.background_executor().now(),
            ignored: Cell::new(None),
            activation: RefCell::new(None),
            was_active: Cell::new(false),
        }));
        let weak = Rc::downgrade(&press.0);
        let flush = Rc::new(move |window: &mut Window, cx: &mut App| {
            if let Some(inner) = weak.upgrade() {
                let press = MenuPress(inner);
                if press.0.hold_fired.replace(false) {
                    press.fire_long_press(window, cx);
                }
            }
        });
        let clock = WindowClock::new(window, cx, flush);
        let fired = Rc::downgrade(&press.0);
        *press.0.hold.borrow_mut() = Some(LongPressDetector::new(
            DetectorOptions {
                on_long_press: Rc::new(move |_| {
                    if let Some(inner) = fired.upgrade() {
                        inner.hold_fired.set(true);
                    }
                }),
                disabled: false,
                delay_ms: None,
                move_cancel_px: None,
            },
            clock,
        ));
        press
    }

    /// Watches the window's activation (from the keyed state that holds the model): the
    /// window losing focus mid-press withdraws a finger's gesture to its lift and ends a
    /// mouse's as a cancel, as upstream's `blur` listener does.
    pub(crate) fn watch_activation<T: 'static>(&self, window: &mut Window, cx: &gpui::Context<T>) {
        let weak = Rc::downgrade(&self.0);
        let subscription = cx.observe_window_activation(window, move |_, window, cx| {
            let Some(inner) = weak.upgrade() else {
                return;
            };
            let press = MenuPress(inner);
            if !window.is_window_active() && press.is_active() {
                press.step(MenuPressInput::Withdraw, None, window, cx);
                window.refresh();
            }
        });
        *self.0.activation.borrow_mut() = Some(subscription);
    }

    /// Ends the gesture without the window (the menu closed under it).
    pub(crate) fn reset(&self) {
        if let Some(hold) = self.0.hold.borrow().as_ref() {
            hold.cancel();
        }
        self.0.lit.borrow_mut().take();
        *self.0.autoscroll.borrow_mut() = None;
        self.0.pointer.set(None);
        self.0.last.set(None);
        *self.0.gesture.borrow_mut() = MenuPressState::Idle;
    }

    /// Replaces the options (a component that builds them after the hook ran).
    pub(crate) fn set_options(&self, options: MenuPressOptions) {
        *self.0.options.borrow_mut() = options;
    }

    /// The owner's half of the options: what a trigger press opens.
    pub(crate) fn set_trigger_press(
        &self,
        f: impl Fn(MenuPressPointerType, &mut Window, &mut App) -> bool + 'static,
    ) {
        self.0.options.borrow_mut().on_trigger_press = Some(Rc::new(f));
    }

    /// The menu's half of the options: its dismissal, scroller and focus.
    pub(crate) fn set_menu(
        &self,
        dismiss: impl Fn(&mut Window, &mut App) + 'static,
        scroller: Option<ScrollHandle>,
        focus: Option<FocusHandle>,
    ) {
        let mut options = self.0.options.borrow_mut();
        options.on_dismiss = Some(Rc::new(dismiss));
        options.scroller = scroller;
        options.menu_focus = focus;
    }

    fn now(&self, cx: &App) -> f64 {
        cx.background_executor()
            .now()
            .saturating_duration_since(self.0.origin)
            .as_secs_f64()
            * 1000.
    }

    fn options(&self) -> MenuPressOptions {
        self.0.options.borrow().clone()
    }

    fn hold(&self) -> LongPressDetector {
        self.0.hold.borrow().clone().expect("the hold detector")
    }

    /// Whether a gesture is live.
    pub fn is_active(&self) -> bool {
        !self.0.gesture.borrow().is_idle()
    }

    /// The pointer of the live gesture.
    pub fn pointer_type(&self) -> Option<MenuPressPointerType> {
        self.0.gesture.borrow().pointer()
    }

    /// The row a held finger lit: it shows the pressed paint.
    pub fn lit_row(&self) -> Option<SharedString> {
        self.0.lit.borrow().clone()
    }

    /// Upstream `isTriggerClickFromPress`: whether the click reaching the trigger belongs to
    /// the press that already opened or closed the menu. Reading it spends it.
    pub fn is_trigger_click_from_press(&self) -> bool {
        self.0.trigger_click.replace(false)
    }

    /// Whether the click arriving now is one a tracked release already answered (the
    /// browser's stray click upstream's click guard swallows). Reading it spends it.
    pub fn take_stray_click(&self, cx: &App) -> bool {
        match self.0.stray_until.take() {
            Some(until) => self.now(cx) <= until,
            None => false,
        }
    }

    /// Upstream `cancel`: ends the gesture in flight with nothing acting (the menu closed).
    pub fn cancel(&self, window: &mut Window, cx: &mut App) {
        self.end_gesture(window, cx);
    }

    /// The menu opened under a pointer that is still held (a context menu's right press,
    /// or a long press under a finger): the press continues as the trigger's would.
    pub fn opened_under(&self, pointer: MenuPressPointerType, button: MouseButton, cx: &App) {
        let time = self.now(cx);
        *self.0.gesture.borrow_mut() = MenuPressState::Open {
            pointer,
            opened_at: time,
            handoff_from: None,
        };
        self.0.pointer.set(Some(match pointer {
            MenuPressPointerType::Mouse => Pointer::Mouse(button),
            _ => Pointer::Touch(None),
        }));
        self.0.claimed.set(pointer.is_finger());
    }

    fn uikit(&self) -> bool {
        self.0.options.borrow().long_press_delay_ms.is_none()
    }

    fn end_gesture(&self, window: &mut Window, cx: &mut App) {
        if let Some(hold) = self.0.hold.borrow().as_ref() {
            hold.cancel();
        }
        if self.0.lit.borrow_mut().take().is_some() {
            window.refresh();
        }
        *self.0.autoscroll.borrow_mut() = None;
        self.0.pointer.set(None);
        self.0.last.set(None);
        *self.0.gesture.borrow_mut() = MenuPressState::Idle;
        let _ = cx;
    }

    fn arm_stray(&self, cx: &App) {
        self.0
            .stray_until
            .set(Some(self.now(cx) + STRAY_CLICK_WINDOW));
    }

    fn resolve(&self, at: gpui::Point<Pixels>) -> (Option<SharedString>, bool, bool) {
        let registry = self.0.registry.borrow();
        let in_menu = registry.surfaces.iter().any(|b| b.contains(&at));
        let on_trigger = registry.trigger.is_some_and(|b| b.contains(&at));
        let row = in_menu
            .then(|| {
                registry
                    .rows
                    .iter()
                    .rev()
                    .find(|r| r.bounds.contains(&at))
                    .map(|r| r.row.key.clone())
            })
            .flatten();
        (row, in_menu, on_trigger)
    }

    fn step(
        &self,
        input: MenuPressInput<SharedString>,
        release: Option<MenuRelease>,
        window: &mut Window,
        cx: &mut App,
    ) {
        let gesture = self.0.gesture.borrow().clone();
        if matches!(input, MenuPressInput::Down { .. }) {
            self.0.was_active.set(window.is_window_active());
        }
        let next = menu_press_transition(gesture, input);
        *self.0.gesture.borrow_mut() = next.gesture.clone();
        self.apply(next.effect, release, window, cx);
    }

    fn highlight(&self, row: Option<SharedString>, window: &mut Window, cx: &mut App) {
        let held = self.pointer_type().is_some_and(|p| p.is_finger());
        let lit = if held { row.clone() } else { None };
        if *self.0.lit.borrow() != lit {
            *self.0.lit.borrow_mut() = lit;
            window.refresh();
        }
        let options = self.options();
        if let Some(f) = options.on_highlight {
            f(row, window, cx);
            return;
        }
        let focus = match &row {
            Some(key) => self
                .0
                .registry
                .borrow()
                .rows
                .iter()
                .find(|r| &r.row.key == key)
                .and_then(|r| r.row.focus.clone()),
            None => options.menu_focus.clone(),
        };
        if let Some(focus) = focus
            && !focus.is_focused(window)
        {
            focus.focus(window, cx);
            window.refresh();
        }
    }

    fn apply(
        &self,
        effect: MenuPressOutcome<SharedString>,
        release: Option<MenuRelease>,
        window: &mut Window,
        cx: &mut App,
    ) {
        match effect {
            MenuPressOutcome::None => {}
            MenuPressOutcome::Open => {
                let pointer = self.pointer_type().unwrap_or(MenuPressPointerType::Mouse);
                self.0.trigger_click.set(true);
                let opened = self
                    .options()
                    .on_trigger_press
                    .is_some_and(|f| f(pointer, window, cx));
                if !opened {
                    self.end_gesture(window, cx);
                }
            }
            MenuPressOutcome::Highlight(row) => self.highlight(Some(row), window, cx),
            MenuPressOutcome::Clear => self.highlight(None, window, cx),
            MenuPressOutcome::Act(row) => {
                // Detach before acting: the activation usually closes the menu.
                let entry = self
                    .0
                    .registry
                    .borrow()
                    .rows
                    .iter()
                    .find(|r| r.row.key == row)
                    .cloned();
                self.end_gesture(window, cx);
                self.arm_stray(cx);
                let Some(release) = release else {
                    return;
                };
                if let Some(f) = self.options().on_activate {
                    f(row, &release, window, cx);
                } else if let Some(activate) = entry.and_then(|e| e.row.activate) {
                    activate(&release, window, cx);
                }
            }
            MenuPressOutcome::Withdraw { clear } => {
                self.hold().cancel();
                *self.0.autoscroll.borrow_mut() = None;
                if clear {
                    self.highlight(None, window, cx);
                }
            }
            MenuPressOutcome::Settle {
                stray,
                dismiss,
                clear,
            } => {
                if clear {
                    self.highlight(None, window, cx);
                }
                self.end_gesture(window, cx);
                if stray {
                    self.arm_stray(cx);
                }
                if dismiss && let Some(f) = self.options().on_dismiss {
                    f(window, cx);
                }
            }
        }
    }

    fn fire_long_press(&self, window: &mut Window, cx: &mut App) {
        let MenuPressState::TriggerPress { pointer, .. } = *self.0.gesture.borrow() else {
            return;
        };
        self.0.trigger_click.set(true);
        let opened = self
            .options()
            .on_trigger_press
            .is_some_and(|f| f(pointer, window, cx));
        if !opened {
            self.step(MenuPressInput::Cancel, None, window, cx);
            return;
        }
        // The finger now drives the menu: the window no longer scrolls under it.
        self.0.claimed.set(true);
        let time = self.now(cx);
        self.step(MenuPressInput::Opened { time }, None, window, cx);
    }

    fn update_autoscroll(&self, at: gpui::Point<Pixels>, window: &mut Window, cx: &mut App) {
        let tracking = matches!(*self.0.gesture.borrow(), MenuPressState::Tracking { .. });
        let scroller = self.0.options.borrow().scroller.clone();
        let Some(handle) = scroller.filter(|_| tracking) else {
            *self.0.autoscroll.borrow_mut() = None;
            return;
        };
        let bounds = handle.bounds();
        if handle.max_offset().y <= px(0.) || bounds.size.height <= px(0.) {
            *self.0.autoscroll.borrow_mut() = None;
            return;
        }
        let direction: i8 = if at.y <= bounds.top() + px(MENU_PRESS_AUTOSCROLL_ZONE_PX) {
            -1
        } else if at.y >= bounds.bottom() - px(MENU_PRESS_AUTOSCROLL_ZONE_PX) {
            1
        } else {
            0
        };
        if direction == 0 {
            *self.0.autoscroll.borrow_mut() = None;
            return;
        }
        if self
            .0
            .autoscroll
            .borrow()
            .as_ref()
            .is_some_and(|(d, _)| *d == direction)
        {
            return;
        }
        let weak = Rc::downgrade(&self.0);
        let task = window.spawn(cx, async move |cx| {
            loop {
                let Some(inner) = weak.upgrade() else {
                    return;
                };
                let press = MenuPress(inner);
                let moved = cx
                    .update(|window, cx| press.autoscroll_step(&handle, direction, window, cx))
                    .unwrap_or(false);
                drop(press);
                if !moved {
                    return;
                }
                cx.background_executor().timer(AUTOSCROLL_INTERVAL).await;
            }
        });
        *self.0.autoscroll.borrow_mut() = Some((direction, task));
    }

    fn autoscroll_step(
        &self,
        handle: &ScrollHandle,
        direction: i8,
        window: &mut Window,
        cx: &mut App,
    ) -> bool {
        let mut offset = handle.offset();
        let before = offset.y;
        let max = handle.max_offset().y;
        offset.y = (offset.y - px(AUTOSCROLL_STEP_PX) * f32::from(direction)).clamp(-max, px(0.));
        if offset.y == before {
            // The edge is reached.
            if let Some((_, task)) = self.0.autoscroll.borrow_mut().take() {
                task.detach();
            }
            return false;
        }
        handle.set_offset(offset);
        window.refresh();
        // The rows moved under the resting pointer: re-read the row under it.
        if let Some(at) = self.0.last.get() {
            let (row, in_menu, _) = self.resolve(at);
            let time = self.now(cx);
            self.step(
                MenuPressInput::Move {
                    row,
                    in_menu,
                    time,
                    at: None,
                },
                None,
                window,
                cx,
            );
        }
        true
    }

    // --- Input ---

    fn begin_dispatch(&self, token: usize, window: &Window, cx: &mut App) -> bool {
        if self.0.dispatch.get() == Some(token) {
            return false;
        }
        self.0.dispatch.set(Some(token));
        if !self.0.reset_pending.replace(true) {
            let weak = Rc::downgrade(&self.0);
            window.defer(cx, move |_, _| {
                if let Some(inner) = weak.upgrade() {
                    inner.dispatch.set(None);
                    inner.reset_pending.set(false);
                }
            });
        }
        true
    }

    fn mouse_down(&self, event: &MouseDownEvent, window: &mut Window, cx: &mut App) {
        if self.0.options.borrow().disabled || window.last_input_was_touch() {
            return;
        }
        // Any press ends a withdrawn gesture; a second pointer ends a live one.
        if matches!(*self.0.gesture.borrow(), MenuPressState::Withdrawn { .. }) {
            self.step(MenuPressInput::Cancel, None, window, cx);
            return;
        }
        if self.is_active() {
            if self.0.pointer.get() != Some(Pointer::Mouse(event.button)) {
                self.step(MenuPressInput::Cancel, None, window, cx);
            }
            return;
        }
        // A new press: the last gesture's click is no longer coming.
        self.0.stray_until.set(None);
        self.0.trigger_click.set(false);
        if event.button != MouseButton::Left {
            return;
        }
        let (row, in_menu, on_trigger) = self.resolve(event.position);
        let time = self.now(cx);
        let items_only = self.0.options.borrow().press_on_items_only;
        if in_menu && (row.is_some() || !items_only) {
            self.0.pointer.set(Some(Pointer::Mouse(event.button)));
            self.0.last.set(Some(event.position));
            self.step(
                MenuPressInput::Down {
                    target: MenuPressTarget::Menu,
                    pointer: MenuPressPointerType::Mouse,
                    row,
                    time,
                },
                None,
                window,
                cx,
            );
        } else if on_trigger {
            // A control-click is the context menu's on macOS.
            if cfg!(target_os = "macos") && event.modifiers.control {
                return;
            }
            self.0.pointer.set(Some(Pointer::Mouse(event.button)));
            self.step(
                MenuPressInput::Down {
                    target: MenuPressTarget::Trigger,
                    pointer: MenuPressPointerType::Mouse,
                    row: None,
                    time,
                },
                None,
                window,
                cx,
            );
        }
    }

    fn mouse_move(&self, event: &MouseMoveEvent, window: &mut Window, cx: &mut App) {
        if !matches!(self.0.pointer.get(), Some(Pointer::Mouse(_))) {
            return;
        }
        if self.blurred(window) {
            self.step(MenuPressInput::Withdraw, None, window, cx);
            return;
        }
        self.0.last.set(Some(event.position));
        let (row, in_menu, _) = self.resolve(event.position);
        let time = self.now(cx);
        self.step(
            MenuPressInput::Move {
                row,
                in_menu,
                time,
                at: None,
            },
            None,
            window,
            cx,
        );
        self.update_autoscroll(event.position, window, cx);
    }

    fn mouse_up(&self, event: &MouseUpEvent, window: &mut Window, cx: &mut App) {
        if self.0.pointer.get() != Some(Pointer::Mouse(event.button)) {
            return;
        }
        let (row, in_menu, on_trigger) = self.resolve(event.position);
        let time = self.now(cx);
        self.step(
            MenuPressInput::Up {
                row,
                in_menu,
                on_trigger,
                time,
                at: None,
            },
            Some(MenuRelease {
                pointer: MenuPressPointerType::Mouse,
                position: event.position,
                button: event.button,
                modifiers: event.modifiers,
            }),
            window,
            cx,
        );
    }

    /// The window lost its activation since the gesture began.
    fn blurred(&self, window: &Window) -> bool {
        self.0.was_active.get() && !window.is_window_active()
    }

    fn ours(&self, id: u64) -> bool {
        match self.0.pointer.get() {
            Some(Pointer::Touch(Some(own))) => own == id,
            Some(Pointer::Touch(None)) => {
                // A menu opened under a finger adopts it.
                self.0.pointer.set(Some(Pointer::Touch(Some(id))));
                true
            }
            _ => false,
        }
    }

    fn handoff(&self, at: gpui::Point<Pixels>) -> Option<Point> {
        self.uikit()
            .then(|| Point::new(f64::from(f32::from(at.x)), f64::from(f32::from(at.y))))
    }

    /// One finger event; whether the model claims that finger.
    fn touch(&self, event: &TouchEvent, window: &mut Window, cx: &mut App) -> bool {
        let at = event.position;
        if self.0.ignored.get() == Some(event.id.0) {
            if matches!(event.phase, TouchPhase::Ended | TouchPhase::Cancelled) {
                self.0.ignored.set(None);
            }
            return true;
        }
        match event.phase {
            TouchPhase::Started => {
                if self.0.options.borrow().disabled {
                    return false;
                }
                if matches!(*self.0.gesture.borrow(), MenuPressState::Withdrawn { .. }) {
                    self.step(MenuPressInput::Cancel, None, window, cx);
                    return false;
                }
                if self.is_active() {
                    if self.ours(event.id.0) {
                        return self.0.claimed.get();
                    }
                    // A second finger: a pinch or a scroll, never a pick.
                    self.0.ignored.set(Some(event.id.0));
                    self.step(MenuPressInput::Cancel, None, window, cx);
                    return true;
                }
                self.0.stray_until.set(None);
                self.0.trigger_click.set(false);
                let (row, in_menu, on_trigger) = self.resolve(at);
                let time = self.now(cx);
                let items_only = self.0.options.borrow().press_on_items_only;
                if in_menu && items_only && row.is_none() {
                    return false;
                }
                if in_menu {
                    self.0.pointer.set(Some(Pointer::Touch(Some(event.id.0))));
                    // A menu whose rows fit takes no pan (upstream's `touch-action:
                    // pinch-zoom`): the finger is the model's. One that scrolls leaves the
                    // finger's pan to the window, which then takes the gesture.
                    let scrolls = self.0.options.borrow().scroller.is_some();
                    self.0.claimed.set(!scrolls);
                    self.0.last.set(Some(at));
                    self.step(
                        MenuPressInput::Down {
                            target: MenuPressTarget::Menu,
                            pointer: MenuPressPointerType::Touch,
                            row,
                            time,
                        },
                        None,
                        window,
                        cx,
                    );
                    return self.0.claimed.get();
                }
                if !on_trigger {
                    return false;
                }
                self.0.pointer.set(Some(Pointer::Touch(Some(event.id.0))));
                self.0.claimed.set(false);
                let down = MenuPressInput::Down {
                    target: MenuPressTarget::Trigger,
                    pointer: MenuPressPointerType::Touch,
                    row: None,
                    time,
                };
                let in_scroller = self.0.registry.borrow().trigger_in_scroller;
                if self.uikit() && !in_scroller {
                    // A fixed bar: the menu opens as the finger presses.
                    self.step(down, None, window, cx);
                    self.fire_long_press(window, cx);
                    return self.0.claimed.get();
                }
                let delay = self.0.options.borrow().long_press_delay_ms;
                let hold = self.hold();
                hold.set_options(DetectorOptions {
                    delay_ms: Some(match delay {
                        Some(ms) => ms,
                        None => DELAYS_CONTENT_TOUCHES_MS,
                    }),
                    ..self.hold_options()
                });
                hold.start(
                    Point::new(f64::from(f32::from(at.x)), f64::from(f32::from(at.y))),
                    Some(LongPressTolerance {
                        move_cancel_px: Some(MENU_PRESS_TRIGGER_DRIFT_PX),
                    }),
                );
                self.step(down, None, window, cx);
                false
            }
            TouchPhase::Moved => {
                if !self.ours(event.id.0) {
                    return false;
                }
                if self.blurred(window) {
                    self.step(MenuPressInput::Withdraw, None, window, cx);
                    return self.0.claimed.get();
                }
                self.0.last.set(Some(at));
                if matches!(
                    *self.0.gesture.borrow(),
                    MenuPressState::TriggerPress { .. }
                ) {
                    // Past the drift allowance the finger is scrolling, not holding.
                    let pending = self.hold().move_to(Point::new(
                        f64::from(f32::from(at.x)),
                        f64::from(f32::from(at.y)),
                    ));
                    if !pending {
                        self.step(MenuPressInput::Cancel, None, window, cx);
                    }
                    return false;
                }
                let (row, in_menu, _) = self.resolve(at);
                let time = self.now(cx);
                self.step(
                    MenuPressInput::Move {
                        row,
                        in_menu,
                        time,
                        at: self.handoff(at),
                    },
                    None,
                    window,
                    cx,
                );
                self.update_autoscroll(at, window, cx);
                self.0.claimed.get()
            }
            TouchPhase::Ended => {
                if !self.ours(event.id.0) {
                    return false;
                }
                let claimed = self.0.claimed.get();
                let (row, in_menu, on_trigger) = self.resolve(at);
                let time = self.now(cx);
                self.step(
                    MenuPressInput::Up {
                        row,
                        in_menu,
                        on_trigger,
                        time,
                        at: self.handoff(at),
                    },
                    Some(MenuRelease {
                        pointer: MenuPressPointerType::Touch,
                        position: at,
                        button: MouseButton::Left,
                        modifiers: Modifiers::default(),
                    }),
                    window,
                    cx,
                );
                // A release the model answered sends no tap of its own.
                claimed || self.0.stray_until.get().is_some()
            }
            TouchPhase::Cancelled => {
                if !self.ours(event.id.0) {
                    return false;
                }
                self.step(MenuPressInput::Cancel, None, window, cx);
                false
            }
        }
    }

    fn hold_options(&self) -> DetectorOptions {
        let weak = Rc::downgrade(&self.0);
        DetectorOptions {
            on_long_press: Rc::new(move |_| {
                if let Some(inner) = weak.upgrade() {
                    inner.hold_fired.set(true);
                }
            }),
            disabled: false,
            delay_ms: None,
            move_cancel_px: None,
        }
    }

    fn scroll(&self, event: &ScrollEvent, window: &mut Window, cx: &mut App) {
        // The window took an unclaimed finger for its scroll.
        // While a finger rests on the trigger its drift allowance decides, not the
        // window's pan (a 12 pt drift still opens the menu).
        let holding = matches!(
            *self.0.gesture.borrow(),
            MenuPressState::TriggerPress { .. }
        );
        if event.source == ScrollSource::Touch
            && matches!(self.0.pointer.get(), Some(Pointer::Touch(_)))
            && !self.0.claimed.get()
            && !holding
        {
            self.step(MenuPressInput::Cancel, None, window, cx);
        }
    }

    /// Registers the window's listeners for this frame.
    fn listen(&self, window: &mut Window) {
        let p = self.clone();
        window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
            if phase == DispatchPhase::Capture && p.begin_dispatch(token(event), window, cx) {
                p.mouse_down(event, window, cx);
            }
        });
        let p = self.clone();
        window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
            if phase == DispatchPhase::Capture && p.begin_dispatch(token(event), window, cx) {
                p.mouse_move(event, window, cx);
            }
        });
        let p = self.clone();
        window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
            if phase == DispatchPhase::Capture && p.begin_dispatch(token(event), window, cx) {
                p.mouse_up(event, window, cx);
            }
        });
        let p = self.clone();
        window.on_mouse_event(move |event: &ScrollEvent, phase, window, cx| {
            if phase == DispatchPhase::Capture && p.begin_dispatch(token(event), window, cx) {
                p.scroll(event, window, cx);
            }
        });
        let p = self.clone();
        window.on_touch_event(move |event: &TouchEvent, window, cx| {
            p.begin_dispatch(token(event), window, cx) && p.touch(event, window, cx)
        });
    }

    // --- The wrapped elements ---

    /// Upstream `menuProps`: a surface of the menu (each level wraps its own). `first`
    /// is the level drawn first each frame (the root level), which starts the frame's
    /// registry.
    pub fn menu(&self, child: impl IntoElement, first: bool) -> MenuPressSurface {
        MenuPressSurface {
            press: self.clone(),
            part: Part::Menu { first },
            child: child.into_any_element(),
        }
    }

    /// Upstream `triggerProps`: the control that opens the menu.
    pub fn trigger(&self, child: impl IntoElement) -> MenuPressSurface {
        MenuPressSurface {
            press: self.clone(),
            part: Part::Trigger,
            child: child.into_any_element(),
        }
    }

    /// An enabled row (upstream `itemSelector`'s match). A disabled row, a divider or a
    /// heading is not wrapped: a pointer over it highlights nothing.
    pub fn row(&self, row: MenuPressRow, child: impl IntoElement) -> MenuPressSurface {
        MenuPressSurface {
            press: self.clone(),
            part: Part::Row(row),
            child: child.into_any_element(),
        }
    }
}

fn token<E>(event: &E) -> usize {
    event as *const E as usize
}

enum Part {
    Menu { first: bool },
    Trigger,
    Row(MenuPressRow),
}

/// What [`MenuPress::menu`], [`MenuPress::trigger`] and [`MenuPress::row`] return: it lays out
/// as its child and tells the model where it is.
pub struct MenuPressSurface {
    press: MenuPress,
    part: Part,
    child: AnyElement,
}

impl Element for MenuPressSurface {
    type RequestLayoutState = ();
    type PrepaintState = ();

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
    ) {
        {
            let mut registry = self.press.0.registry.borrow_mut();
            // Only what the window shows of the part counts: a scroller clips its rows.
            let shown = bounds.intersect(&window.content_mask().bounds);
            match &self.part {
                Part::Menu { first } => {
                    if *first {
                        registry.surfaces.clear();
                        registry.rows.clear();
                    }
                    registry.surfaces.push(shown);
                }
                Part::Trigger => {
                    registry.trigger = Some(shown);
                    registry.trigger_in_scroller = crate::gestures::scroller::in_scroller();
                }
                Part::Row(row) => registry.rows.push(RowEntry {
                    row: row.clone(),
                    bounds: shown,
                }),
            }
        }
        self.child.prepaint(window, cx);
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _state: &mut (),
        _prepaint: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        if !matches!(self.part, Part::Row(_)) {
            self.press.listen(window);
        }
        self.child.paint(window, cx);
    }
}
