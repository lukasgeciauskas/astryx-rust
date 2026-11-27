//! The touch press model, upstream `hooks/usePressFeedback.ts` over the window's press
//! controller (`utils/pressFeedback.ts`).
//!
//! One controller per window follows the presses on every pressable surface: a surface
//! costs one listener registration a frame and no state of its own beyond its key. The
//! innermost surface under a finger takes the press (a button inside a pressable row takes
//! it, as in a native cell); a disabled surface takes it and paints nothing, so a press on
//! a disabled control lights no row around it.
//!
//! - **A finger** runs the whole machine ([`super::press_gesture`]): outside a scroller the
//!   surface paints in the frame the finger lands, as a `UIControl` highlights; inside one it
//!   waits the content-touch delay, since the touch can become a scroll. Travel past the
//!   slop, a scroll or the platform's cancel kills it for the rest of the gesture; a tap
//!   quicker than the delay paints at the lift and holds a moment; the release fades over
//!   the theme's exit duration and easing (no longer than the Reduce Motion fade under Reduce
//!   Motion). A finger landing on a scroller still gliding from a finger's fling is a brake,
//!   not a press, and paints nothing. A second finger ends the press.
//! - **A mouse button or the Space key** keeps GPUI's own active state where the primary
//!   pointer is fine. Under a coarse primary pointer (a phone or a tablet) the controller
//!   paints them too, at once, and fades them at the release, so components drop their
//!   active style there ([`PressFeedback::paints_buttons`]). A menu's rows keep their
//!   highlight as their only mouse paint.
//!
//! A component calls [`use_press_feedback`], wraps the element that paints in
//! [`PressFeedback::surface`], and paints the pressed overlay at
//! [`PressFeedback::overlay`]'s strength.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::{Rc, Weak};
use std::time::Duration;

use astryx_primitives::motion::uikit::timing::REDUCED_MOTION_CROSS_FADE_MS;
use astryx_primitives::{coarse_primary_pointer, prefers_reduced_motion};
use astryx_theme::CubicBezier;
use gpui::{
    AnyElement, App, Bounds, DispatchPhase, Element, ElementId, FocusHandle, Global,
    GlobalElementId, Hitbox, HitboxBehavior, Hsla, InspectorElementId, IntoElement, KeyDownEvent,
    KeyUpEvent, LayoutId, MouseButton, MouseDownEvent, MouseUpEvent, Pixels, ScrollEvent,
    ScrollSource, Task, TouchEvent, TouchPhase, Window, WindowId,
};
use web_time::Instant;

use super::press_gesture::{
    PRESS_FLASH_MS, PRESS_ONSET_MS, PRESS_SCROLL_BRAKE_MS, PressEvent, PressGesture, PressPaint,
    PressPhase, PressStep, PressTimer, idle_press_gesture, press_gesture_step,
};

/// What a surface registers each frame.
#[derive(Clone, Copy)]
struct SurfaceInfo {
    key: u64,
    bounds: Bounds<Pixels>,
    /// Inside a scroller: a finger waits the content-touch delay.
    delayed: bool,
    /// Where the innermost scroller around it shows.
    scroller: Option<Bounds<Pixels>>,
    disabled: bool,
    /// The release's clock and curve.
    fade_ms: f64,
    ease: CubicBezier,
    /// A menu row: its highlight is its only mouse and key paint.
    in_menu: bool,
    /// The surface owns a control's box (a checkbox's or a radio's row): a mouse's press
    /// paints here at once on any primary pointer.
    owns_box: bool,
    /// The device was touch-first as the surface was last drawn.
    touch_first: bool,
    /// A group press tracker paints this surface's fingers (upstream's press-group item).
    skip_fingers: bool,
}

#[derive(Clone, Copy)]
enum Candidate {
    Finger { id: u64, x: f64, y: f64 },
    Mouse,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SlotKind {
    Finger,
    Button,
}

struct Live {
    surface: SurfaceInfo,
    gesture: PressGesture,
    /// The finger's touch; `None` for the Space key, `Some(u64::MAX)` for the mouse.
    pointer: Option<u64>,
    /// The clock the current phase owes.
    timer: Option<Task<()>>,
    paint: PressPaint,
    /// When the release fade began.
    fade_start: Option<Instant>,
}

const MOUSE: u64 = u64::MAX;

#[derive(Default)]
struct Controller {
    finger: RefCell<Option<Live>>,
    button: RefCell<Option<Live>>,
    /// The event a dispatch already handled, so only the first listener runs it.
    dispatch: Cell<Option<usize>>,
    reset_pending: Cell<bool>,
    /// The innermost surface already took this touch's start.
    started: Cell<Option<usize>>,
    /// The surfaces under this dispatch's new touch or mouse press: the innermost (the
    /// smallest) takes it once the dispatch is over.
    candidates: RefCell<Vec<(SurfaceInfo, Candidate)>>,
    /// Surfaces a press in this dispatch began on a control nested inside, other than the
    /// surface's own action: they show no press for it (upstream `cancelNestedPress`).
    vetoed: RefCell<Vec<u64>>,
    /// When and where a finger last drove a scroll.
    last_finger_scroll: Cell<Option<(Instant, gpui::Point<Pixels>)>>,
    space_held: Cell<bool>,
}

#[derive(Default)]
struct Controllers(HashMap<WindowId, Rc<Controller>>);

impl Global for Controllers {}

fn controller(window: &Window, cx: &mut App) -> Rc<Controller> {
    let id = window.window_handle().window_id();
    cx.default_global::<Controllers>()
        .0
        .entry(id)
        .or_default()
        .clone()
}

impl Controller {
    fn slot(&self, kind: SlotKind) -> &RefCell<Option<Live>> {
        match kind {
            SlotKind::Finger => &self.finger,
            SlotKind::Button => &self.button,
        }
    }

    /// Whether this listener is the first of its dispatch.
    fn begin_dispatch(self: &Rc<Self>, token: usize, window: &Window, cx: &mut App) -> bool {
        if self.dispatch.get() == Some(token) {
            return false;
        }
        self.dispatch.set(Some(token));
        self.schedule_reset(window, cx);
        true
    }

    fn schedule_reset(self: &Rc<Self>, window: &Window, cx: &mut App) {
        if !self.reset_pending.replace(true) {
            let weak = Rc::downgrade(self);
            window.defer(cx, move |_, _| {
                if let Some(c) = weak.upgrade() {
                    c.dispatch.set(None);
                    c.started.set(None);
                    c.reset_pending.set(false);
                }
            });
        }
    }

    fn paint_of(&self, key: u64) -> (PressPaint, Option<(Instant, f64, CubicBezier)>) {
        for slot in [&self.finger, &self.button] {
            if let Some(live) = slot.borrow().as_ref()
                && live.surface.key == key
                && live.paint != PressPaint::None
            {
                let fade = live
                    .fade_start
                    .map(|at| (at, live.surface.fade_ms, live.surface.ease));
                return (live.paint, fade);
            }
        }
        (PressPaint::None, None)
    }

    /// Runs one step on a slot and carries out what it answered.
    fn apply(self: &Rc<Self>, kind: SlotKind, step: PressStep, window: &mut Window, cx: &mut App) {
        let mut slot = self.slot(kind).borrow_mut();
        let Some(live) = slot.as_mut() else {
            return;
        };
        let moved = step.gesture.phase != live.gesture.phase;
        if step.timer.is_some() || moved {
            live.timer = None;
        }
        live.gesture = step.gesture;
        if step.paint != live.paint {
            live.paint = step.paint;
            window.refresh();
        }
        live.fade_start = (step.paint == PressPaint::Fading).then(|| {
            live.fade_start
                .unwrap_or_else(|| cx.background_executor().now())
        });
        if step.gesture.phase == PressPhase::Idle {
            *slot = None;
            return;
        }
        if let Some(timer) = step.timer {
            let (ms, event) = match timer {
                PressTimer::Onset => (PRESS_ONSET_MS, PressEvent::Onset),
                PressTimer::Flash => (PRESS_FLASH_MS, PressEvent::FlashEnd),
                PressTimer::Fade => (live.surface.fade_ms, PressEvent::FadeEnd),
            };
            let wait = cx
                .background_executor()
                .timer(Duration::from_secs_f64(ms.max(0.) / 1000.));
            let weak: Weak<Controller> = Rc::downgrade(self);
            live.timer = Some(window.spawn(cx, async move |cx| {
                wait.await;
                cx.update(|window, cx| {
                    if let Some(c) = weak.upgrade() {
                        c.timer_fired(kind, event, window, cx);
                    }
                })
                .ok();
            }));
        }
    }

    fn timer_fired(
        self: &Rc<Self>,
        kind: SlotKind,
        event: PressEvent,
        window: &mut Window,
        cx: &mut App,
    ) {
        let gesture = {
            let mut slot = self.slot(kind).borrow_mut();
            let Some(live) = slot.as_mut() else {
                return;
            };
            // The task is running: forget its handle without cancelling it.
            if let Some(task) = live.timer.take() {
                task.detach();
            }
            live.gesture
        };
        self.apply(kind, press_gesture_step(gesture, event), window, cx);
    }

    fn step(self: &Rc<Self>, kind: SlotKind, event: PressEvent, window: &mut Window, cx: &mut App) {
        let gesture = match self.slot(kind).borrow().as_ref() {
            Some(live) => live.gesture,
            None => return,
        };
        self.apply(kind, press_gesture_step(gesture, event), window, cx);
    }

    /// Drops the paint and forgets the gesture, without running the machine.
    fn finish(&self, kind: SlotKind, window: &mut Window) {
        if let Some(live) = self.slot(kind).borrow_mut().take()
            && live.paint != PressPaint::None
        {
            window.refresh();
        }
        if kind == SlotKind::Button {
            self.space_held.set(false);
        }
    }

    fn release_if_dead(&self, kind: SlotKind) {
        let mut slot = self.slot(kind).borrow_mut();
        if slot
            .as_ref()
            .is_some_and(|l| l.gesture.phase == PressPhase::Dead)
        {
            *slot = None;
        }
    }

    /// A new press on a surface owns its paint: the other slot's gesture there ends.
    fn take_over(&self, other: SlotKind, key: u64, window: &mut Window) {
        if self
            .slot(other)
            .borrow()
            .as_ref()
            .is_some_and(|l| l.surface.key == key)
        {
            self.finish(other, window);
        }
    }

    fn live_pointer(&self, kind: SlotKind) -> Option<Option<u64>> {
        self.slot(kind).borrow().as_ref().map(|l| l.pointer)
    }

    /// A finger's scroll moved this surface's scroller a moment ago: a touch now brakes it.
    fn is_braking(&self, surface: &SurfaceInfo, now: Instant) -> bool {
        self.last_finger_scroll.get().is_some_and(|(at, position)| {
            now.saturating_duration_since(at).as_secs_f64() * 1000. < PRESS_SCROLL_BRAKE_MS
                && surface.scroller.is_some_and(|s| s.contains(&position))
        })
    }

    // --- Fingers ---

    /// A surface under a new press: the innermost of this dispatch's takes it.
    fn candidate(
        self: &Rc<Self>,
        info: SurfaceInfo,
        candidate: Candidate,
        window: &Window,
        cx: &mut App,
    ) {
        let first = {
            let mut list = self.candidates.borrow_mut();
            list.push((info, candidate));
            list.len() == 1
        };
        if first {
            let weak = Rc::downgrade(self);
            window.defer(cx, move |window, cx| {
                let Some(c) = weak.upgrade() else {
                    return;
                };
                let list: Vec<_> = c.candidates.borrow_mut().drain(..).collect();
                let vetoed: Vec<u64> = c.vetoed.borrow_mut().drain(..).collect();
                let area = |b: &Bounds<Pixels>| f32::from(b.size.width) * f32::from(b.size.height);
                let Some((info, candidate)) = list
                    .into_iter()
                    .min_by(|a, b| area(&a.0.bounds).total_cmp(&area(&b.0.bounds)))
                else {
                    return;
                };
                // A press that began on a control inside the surface is that control's.
                if vetoed.contains(&info.key) {
                    return;
                }
                match candidate {
                    Candidate::Finger { id, x, y } => c.finger_down(info, id, x, y, window, cx),
                    Candidate::Mouse => c.press_button(info, Some(MOUSE), window, cx),
                }
            });
        }
    }

    fn finger_down(
        self: &Rc<Self>,
        surface: SurfaceInfo,
        id: u64,
        x: f64,
        y: f64,
        window: &mut Window,
        cx: &mut App,
    ) {
        let finger_up = self
            .finger
            .borrow()
            .as_ref()
            .map(|l| matches!(l.gesture.phase, PressPhase::Fading | PressPhase::Flash));
        if let Some(up) = finger_up {
            // A finger still down means a second finger: no press. One already up is the
            // last tap fading; the new touch owns the surface.
            self.finish(SlotKind::Finger, window);
            if !up {
                return;
            }
        }
        if surface.disabled {
            return;
        }
        let now = cx.background_executor().now();
        if surface.delayed && self.is_braking(&surface, now) {
            return;
        }
        self.take_over(SlotKind::Button, surface.key, window);
        *self.finger.borrow_mut() = Some(Live {
            surface,
            gesture: idle_press_gesture(),
            pointer: Some(id),
            timer: None,
            paint: PressPaint::None,
            fade_start: None,
        });
        self.apply(
            SlotKind::Finger,
            press_gesture_step(
                idle_press_gesture(),
                PressEvent::Down {
                    x,
                    y,
                    delayed: surface.delayed,
                },
            ),
            window,
            cx,
        );
    }

    fn finger_event(self: &Rc<Self>, event: &TouchEvent, window: &mut Window, cx: &mut App) {
        if self.live_pointer(SlotKind::Finger) != Some(Some(event.id.0)) {
            return;
        }
        match event.phase {
            TouchPhase::Moved => {
                let (x, y) = xy(event.position);
                self.step(SlotKind::Finger, PressEvent::Move { x, y }, window, cx);
            }
            TouchPhase::Ended => {
                self.step(SlotKind::Finger, PressEvent::Up, window, cx);
                self.release_if_dead(SlotKind::Finger);
            }
            TouchPhase::Cancelled => self.cancel_finger(window, cx),
            TouchPhase::Started => {}
        }
    }

    fn cancel_finger(self: &Rc<Self>, window: &mut Window, cx: &mut App) {
        self.step(SlotKind::Finger, PressEvent::Cancel, window, cx);
        // A scroll-claimed gesture sends no further events: free the slot.
        self.release_if_dead(SlotKind::Finger);
    }

    fn scroll(self: &Rc<Self>, event: &ScrollEvent, window: &mut Window, cx: &mut App) {
        if event.source == ScrollSource::Touch {
            self.last_finger_scroll
                .set(Some((cx.background_executor().now(), event.position)));
        }
        // Any scroll above the pressed element takes a finger's gesture, driven by a finger
        // or not; a scroll in another scroller does not. A mouse's or a key's press keeps
        // going.
        let reaches = self.finger.borrow().as_ref().is_some_and(|l| {
            l.surface.bounds.contains(&event.position)
                || l.surface
                    .scroller
                    .is_some_and(|s| s.contains(&event.position))
        });
        if reaches {
            self.cancel_finger(window, cx);
        }
    }

    // --- Buttons and the Space key ---

    fn press_button(
        self: &Rc<Self>,
        surface: SurfaceInfo,
        pointer: Option<u64>,
        window: &mut Window,
        cx: &mut App,
    ) {
        // A press still fading elsewhere goes now: the new one owns the slot.
        self.finish(SlotKind::Button, window);
        if surface.disabled || surface.in_menu {
            return;
        }
        self.take_over(SlotKind::Finger, surface.key, window);
        *self.button.borrow_mut() = Some(Live {
            surface,
            gesture: idle_press_gesture(),
            pointer,
            timer: None,
            paint: PressPaint::None,
            fade_start: None,
        });
        self.space_held.set(pointer.is_none());
        self.apply(
            SlotKind::Button,
            press_gesture_step(idle_press_gesture(), PressEvent::ButtonDown),
            window,
            cx,
        );
    }

    fn release_button(self: &Rc<Self>, window: &mut Window, cx: &mut App) {
        self.space_held.set(false);
        // Under Reduce Motion a fine pointer's release on a box ends at once, no fade.
        let at_once = self.button.borrow().as_ref().is_some_and(|l| {
            l.surface.owns_box && !l.surface.touch_first && prefers_reduced_motion(cx)
        });
        if at_once {
            self.step(SlotKind::Button, PressEvent::Cancel, window, cx);
            self.release_if_dead(SlotKind::Button);
            return;
        }
        self.step(SlotKind::Button, PressEvent::Up, window, cx);
    }

    /// Ends the press the controller follows on `key`, and its paint, at once.
    fn cancel_press(&self, key: u64, window: &mut Window) {
        for kind in [SlotKind::Finger, SlotKind::Button] {
            if self
                .slot(kind)
                .borrow()
                .as_ref()
                .is_some_and(|l| l.surface.key == key)
            {
                self.finish(kind, window);
            }
        }
    }
}

fn xy(p: gpui::Point<Pixels>) -> (f64, f64) {
    (f64::from(f32::from(p.x)), f64::from(f32::from(p.y)))
}

fn token<E>(event: &E) -> usize {
    event as *const E as usize
}

/// The surfaces the window's controller paints now, with their bounds (for tests and
/// for a component that must know where a press shows).
pub fn painted_surfaces(window: &Window, cx: &mut App) -> Vec<(Bounds<Pixels>, PressPaint)> {
    let c = controller(window, cx);
    [&c.finger, &c.button]
        .into_iter()
        .filter_map(|slot| {
            slot.borrow()
                .as_ref()
                .filter(|l| l.paint != PressPaint::None)
                .map(|l| (l.surface.bounds, l.paint))
        })
        .collect()
}

/// Declares the device's primary pointer coarse (a phone or a tablet) or fine, or with
/// `None` leaves it to the platform and the window's last pointer. On a touch-first device
/// the press controller paints a mouse's, a trackpad's and the Space key's presses, which
/// GPUI's active state would paint elsewhere.
///
/// The same declaration as [`astryx_primitives::modality::set_primary_pointer_coarse`]
/// (upstream reads `(pointer: coarse)`, the primary pointer, which a mouse or a trackpad
/// on a tablet does not change).
pub fn set_primary_pointer_coarse(coarse: Option<bool>, cx: &mut App) {
    astryx_primitives::modality::set_primary_pointer_coarse(coarse, cx);
}

/// Whether the primary pointer is a finger: the app's declaration, else the browser's
/// `(pointer: coarse)`, else the window's reading of its last pointer.
pub fn touch_first_device(window: &Window, cx: &mut App) -> bool {
    if let Some(coarse) = astryx_primitives::modality::declared_primary_pointer(cx) {
        return coarse;
    }
    platform_coarse() || coarse_primary_pointer(window, cx)
}

fn platform_coarse() -> bool {
    #[cfg(target_family = "wasm")]
    {
        web_sys::window()
            .and_then(|w| w.match_media("(pointer: coarse)").ok().flatten())
            .is_some_and(|list| list.matches())
    }
    #[cfg(not(target_family = "wasm"))]
    {
        false
    }
}

/// Upstream `pressFadeMs`: the release fade's clock, the theme's exit duration, no longer
/// than the Reduce Motion fade under Reduce Motion.
pub fn press_fade_ms(window: &Window, cx: &App) -> f64 {
    let theme = astryx_theme::current(window, cx);
    let exit = theme.tokens().duration.exit.as_secs_f64() * 1000.;
    if prefers_reduced_motion(cx) {
        exit.min(REDUCED_MOTION_CROSS_FADE_MS)
    } else {
        exit
    }
}

/// Upstream `usePressFeedback`: marks the element `id` names as a pressable surface of
/// the touch press model. Cheap to clone.
#[derive(Clone)]
pub struct PressFeedback {
    key: u64,
    controller: Rc<Controller>,
    disabled: bool,
    in_menu: bool,
    space: Option<FocusHandle>,
    owns_box: bool,
    skip_fingers: bool,
}

/// Upstream `usePressFeedback`: the press of the surface `id` names.
///
/// ```ignore
/// let press = use_press_feedback("save", window, cx);
/// let mut el = div().id("save").bg(rest);
/// if let Some(overlay) = press.overlay(t.color.overlay_pressed, window, cx) {
///     el = el.bg(composite(rest, overlay));
/// }
/// press.surface(el)
/// ```
pub fn use_press_feedback(
    id: impl Into<ElementId>,
    window: &mut Window,
    cx: &mut App,
) -> PressFeedback {
    let id = id.into();
    let marker = window.use_keyed_state(
        ElementId::Name(format!("{id}-press-feedback").into()),
        cx,
        |_, _| (),
    );
    PressFeedback {
        key: marker.entity_id().as_u64(),
        controller: controller(window, cx),
        disabled: false,
        in_menu: false,
        owns_box: false,
        space: None,
        skip_fingers: false,
    }
}

impl PressFeedback {
    /// A disabled surface takes a press and paints nothing.
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        if disabled {
            // A surface that turned disabled under a press shows no paint.
            self.controller
                .finger
                .borrow_mut()
                .take_if(|l| l.surface.key == self.key);
            self.controller
                .button
                .borrow_mut()
                .take_if(|l| l.surface.key == self.key);
        }
        self
    }

    /// A button the Space key activates: on a touch-first device its Space press paints
    /// here, as GPUI's active state would.
    pub fn space_activates(mut self, focus: &FocusHandle) -> Self {
        self.space = Some(focus.clone());
        self
    }

    /// The surface's key in the window's press controller (and its touch hit area).
    pub(crate) fn key(&self) -> u64 {
        self.key
    }

    /// What the surface paints now.
    pub fn paint(&self) -> PressPaint {
        self.controller.paint_of(self.key).0
    }

    /// The press's strength, 0 to 1: 1 while a press is believed, falling over the release
    /// on the theme's exit easing. Asks for frames while it falls.
    pub fn strength(&self, window: &mut Window, cx: &App) -> f32 {
        match self.controller.paint_of(self.key) {
            (PressPaint::On, _) => 1.,
            (PressPaint::Fading, Some((start, ms, ease))) => {
                let elapsed = cx
                    .background_executor()
                    .now()
                    .saturating_duration_since(start)
                    .as_secs_f64()
                    * 1000.;
                let t = if ms <= 0. { 1. } else { (elapsed / ms).min(1.) };
                window.request_animation_frame();
                1. - ease.ease(t as f32)
            }
            _ => 0.,
        }
    }

    /// The pressed overlay at the press's strength, or `None` with no press: composite it
    /// over the surface's resting fill.
    pub fn overlay(&self, pressed: Hsla, window: &mut Window, cx: &App) -> Option<Hsla> {
        let strength = self.strength(window, cx);
        (strength > 0.).then_some(Hsla {
            a: pressed.a * strength,
            ..pressed
        })
    }

    /// Whether the controller paints mouse and key presses here (a coarse primary pointer,
    /// where a component drops its own active style).
    pub fn paints_buttons(&self, window: &Window, cx: &mut App) -> bool {
        self.owns_box || touch_first_device(window, cx)
    }

    /// Upstream `pressBox`: the surface owns a control's box (a checkbox's or a radio's
    /// row), so a mouse's press paints at once on any primary pointer and fades on release
    /// (at once under Reduce Motion with a fine pointer); the active style yields.
    pub fn owns_box(mut self, owns: bool) -> Self {
        self.owns_box = owns;
        self
    }

    /// A group's press tracker paints a finger's press here (upstream's press-group item):
    /// the controller takes no finger on this surface.
    pub fn skip_fingers(mut self, skip: bool) -> Self {
        self.skip_fingers = skip;
        self
    }

    /// Ends this surface's press and its paint at once (a press that started on a control
    /// inside it).
    pub fn cancel(&self, window: &mut Window) {
        self.controller.cancel_press(self.key, window);
    }

    /// The surface as a child of the element that paints (absolute, its full size): for an
    /// element that must stay a `Div`.
    pub fn probe(&self) -> gpui::Canvas<(Hitbox, Option<Bounds<Pixels>>)> {
        let paint = self.clone();
        gpui::canvas(
            |bounds, window, _| {
                (
                    window.insert_hitbox(bounds, HitboxBehavior::Normal),
                    super::scroller::innermost_scroller(),
                )
            },
            move |bounds, (hitbox, scroller), window, cx| {
                paint.register(bounds, hitbox, scroller, true, window, cx)
            },
        )
    }

    /// The surface's grown touch area (a hit layer past its box, upstream
    /// `touchHitAreaStyles.layer`): a finger that lands there presses this surface and
    /// shows its paint. Absolute, the layer's full size; it notes no hit-area target.
    pub(crate) fn grown_area_probe(&self) -> gpui::Canvas<(Hitbox, Option<Bounds<Pixels>>)> {
        let paint = self.clone();
        gpui::canvas(
            |bounds, window, _| {
                (
                    window.insert_hitbox(bounds, HitboxBehavior::Normal),
                    super::scroller::innermost_scroller(),
                )
            },
            move |bounds, (hitbox, scroller), window, cx| {
                paint.register(bounds, hitbox, scroller, false, window, cx)
            },
        )
    }

    /// Upstream `cancelNestedPress`: a child of an interactive control nested inside this
    /// surface (absolute, its full size) that is not the surface's own action. A finger's
    /// press that begins on it (and a mouse's, on a surface that owns a control's box) does
    /// not activate the surface, so the surface shows no press for it.
    pub fn nested_control(&self) -> gpui::Canvas<Hitbox> {
        let (c, key, owns_box) = (self.controller.clone(), self.key, self.owns_box);
        gpui::canvas(
            |bounds, window, _| window.insert_hitbox(bounds, HitboxBehavior::Normal),
            move |_, hitbox, window, _| {
                let (touch, touch_hitbox) = (c.clone(), hitbox.clone());
                window.on_touch_event(move |event: &TouchEvent, window, _| {
                    if event.phase == TouchPhase::Started
                        && touch_hitbox.is_hovered_at(event.position, window)
                    {
                        touch.vetoed.borrow_mut().push(key);
                    }
                    false
                });
                if owns_box {
                    let mouse = c.clone();
                    window.on_mouse_event(move |event: &MouseDownEvent, phase, window, _| {
                        if phase == DispatchPhase::Capture
                            && event.button == MouseButton::Left
                            && hitbox.is_hovered(window)
                        {
                            mouse.vetoed.borrow_mut().push(key);
                        }
                    });
                }
            },
        )
    }

    /// Wraps the element that paints the press.
    pub fn surface(&self, child: impl IntoElement) -> PressSurface {
        PressSurface {
            feedback: self.clone(),
            child: child.into_any_element(),
        }
    }
}

/// The element [`PressFeedback::surface`] returns: it lays out as its child and tells the
/// window's controller where the surface is.
pub struct PressSurface {
    feedback: PressFeedback,
    child: AnyElement,
}

impl PressFeedback {
    /// Tells the window's controller where the surface is this frame (in paint).
    fn register(
        &self,
        bounds: Bounds<Pixels>,
        hitbox: Hitbox,
        scroller: Option<Bounds<Pixels>>,
        note: bool,
        window: &mut Window,
        cx: &mut App,
    ) {
        // The device's primary pointer as the surface is drawn (and the window's reading of
        // it kept current, for a key press).
        let touch_first = touch_first_device(window, cx);
        // Every pressable surface is a neighbour a touch hit area stops short of.
        if note && coarse_primary_pointer(window, cx) {
            super::touch_hit_area::note_target(
                window,
                self.key,
                bounds,
                cx.background_executor().now(),
            );
        }
        let theme = astryx_theme::current(window, cx);
        let info = SurfaceInfo {
            key: self.key,
            bounds,
            delayed: scroller.is_some(),
            scroller,
            disabled: self.disabled,
            fade_ms: press_fade_ms(window, cx),
            ease: theme.tokens().ease.exit,
            in_menu: self.in_menu,
            owns_box: self.owns_box,
            touch_first,
            skip_fingers: self.skip_fingers,
        };
        let c = self.controller.clone();
        {
            let (c, hitbox) = (c.clone(), hitbox.clone());
            window.on_touch_event(move |event: &TouchEvent, window, cx| {
                if event.phase == TouchPhase::Started {
                    if !info.skip_fingers && hitbox.is_hovered_at(event.position, window) {
                        let (x, y) = xy(event.position);
                        c.candidate(
                            info,
                            Candidate::Finger {
                                id: event.id.0,
                                x,
                                y,
                            },
                            window,
                            cx,
                        );
                    }
                } else if c.begin_dispatch(token(event), window, cx) {
                    c.finger_event(event, window, cx);
                }
                false
            });
        }
        {
            let c = c.clone();
            window.on_mouse_event(move |event: &ScrollEvent, phase, window, cx| {
                if phase == DispatchPhase::Capture && c.begin_dispatch(token(event), window, cx) {
                    c.scroll(event, window, cx);
                }
            });
        }
        {
            let (c, hitbox) = (c.clone(), hitbox.clone());
            window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
                // A finger's synthesized press is the finger's, not a button's.
                // A touch-first device paints every mouse press here; elsewhere only a
                // surface that owns a control's box does, and an Apple control-click is a
                // context click there.
                let context_click =
                    astryx_primitives::is_context_click(event.button, &event.modifiers, cx);
                if phase == DispatchPhase::Capture
                    && event.button == MouseButton::Left
                    && !window.last_input_was_touch()
                    && hitbox.is_hovered(window)
                    && (info.touch_first || (info.owns_box && !context_click))
                {
                    c.candidate(info, Candidate::Mouse, window, cx);
                }
            });
        }
        {
            let c = c.clone();
            window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
                if phase == DispatchPhase::Capture
                    && event.button == MouseButton::Left
                    && c.begin_dispatch(token(event), window, cx)
                    && c.live_pointer(SlotKind::Button) == Some(Some(MOUSE))
                {
                    c.release_button(window, cx);
                }
            });
        }
        if let Some(focus) = self.space.clone() {
            let (down, f) = (c.clone(), focus.clone());
            window.on_key_event(move |event: &KeyDownEvent, phase, window, cx| {
                if phase == DispatchPhase::Bubble
                    && event.keystroke.key == "space"
                    && !event.is_held
                    && f.is_focused(window)
                    && touch_first_device(window, cx)
                {
                    down.press_button(info, None, window, cx);
                }
            });
            let up = c.clone();
            window.on_key_event(move |event: &KeyUpEvent, phase, window, cx| {
                if phase == DispatchPhase::Bubble
                    && event.keystroke.key == "space"
                    && up.space_held.get()
                    && up.live_pointer(SlotKind::Button) == Some(None)
                {
                    up.release_button(window, cx);
                }
            });
            // Focus left the pressed button: the key's release may never come.
            if c.space_held.get()
                && c.button
                    .borrow()
                    .as_ref()
                    .is_some_and(|l| l.surface.key == info.key)
                && !focus.is_focused(window)
            {
                let c = c.clone();
                window.defer(cx, move |window, cx| c.release_button(window, cx));
            }
        }
    }
}

impl Element for PressSurface {
    type RequestLayoutState = ();
    type PrepaintState = (Hitbox, Option<Bounds<Pixels>>);

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
    ) -> (Hitbox, Option<Bounds<Pixels>>) {
        let hitbox = window.insert_hitbox(bounds, HitboxBehavior::Normal);
        let scroller = super::scroller::innermost_scroller();
        self.child.prepaint(window, cx);
        (hitbox, scroller)
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _state: &mut (),
        prepaint: &mut (Hitbox, Option<Bounds<Pixels>>),
        window: &mut Window,
        cx: &mut App,
    ) {
        let (hitbox, scroller) = (prepaint.0.clone(), prepaint.1);
        self.feedback
            .register(bounds, hitbox, scroller, true, window, cx);
        self.child.paint(window, cx);
    }
}
