//! The public pan, upstream `gestures/createPanGesture.ts` and `hooks/usePanGesture.ts`
//! (spec:AST-107 FR1, FR6): a drag on an area reported as values, never as the
//! recognizer behind it.
//!
//! - [`create_pan_gesture`] is the factory: it attaches a pan to a window's gesture
//!   host and hands back a [`PanGesture`] whose [`PanGesture::area`] wraps the element
//!   that drags. [`PanGesture::destroy`] detaches it; a drag in progress then reports
//!   `Cancelled` once.
//! - [`use_pan_gesture`] is its thin hook: one pan per id, kept across frames,
//!   created again when the axis changes, gone with the element. `on_pan` is read
//!   through a cell, so a new closure each frame re-creates nothing, and the hook reports
//!   nothing after the element leaves or the axis changes (no callback after unmount).
//! - The drag claims its touch at UIKit's 10 pt, so a tap stays a tap; a finger, a mouse
//!   and a pen all drive it (AST-094 FR43). It recognizes no press or long press
//!   (`use_long_press` owns those). A cancelled drag reports zero velocity, so nothing
//!   flings on a cancel.
//!
//! `create_pan` is the pan under it, with the options only components pass (the
//! recognizer's own options), and reports the claim's travel and what drives the drag.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gpui::{App, ElementId, IntoElement, Window};
use rustryx_primitives::gestures::host::{
    GestureArea, GestureAttachment, GestureHost, gesture_area, gesture_host,
};
use rustryx_primitives::gestures::{
    GestureState, PanDirection, PanGestureRecognizer, PanOptions, Point, PointerKind,
};

/// Upstream `axis`: the one axis a drag may move along.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PanAxis {
    X,
    Y,
}

/// Upstream `PanGestureState`: where a drag is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PanState {
    /// The drag claimed its touch.
    Began,
    /// It moved.
    Changed,
    /// It ended at the release.
    Ended,
    /// It was taken away (a cancel, or the pan was destroyed mid-drag).
    Cancelled,
}

impl PanState {
    fn of(state: GestureState) -> Option<Self> {
        match state {
            GestureState::Began => Some(Self::Began),
            GestureState::Changed => Some(Self::Changed),
            GestureState::Ended => Some(Self::Ended),
            GestureState::Cancelled => Some(Self::Cancelled),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Began => "began",
            Self::Changed => "changed",
            Self::Ended => "ended",
            Self::Cancelled => "cancelled",
        }
    }
}

/// Upstream `PanInput` (internal): what drives a drag.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PanInput {
    Touch,
    Mouse,
    Pen,
    /// A trackpad's scroll.
    Scroll,
}

/// Upstream `PanGestureEvent`: one report of a drag.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PanGestureEvent {
    pub state: PanState,
    /// Movement since the drag began, px. The travel the drag needed to begin is not part
    /// of it, so an element that follows it does not jump.
    pub translation: Point,
    /// Velocity in px/s. At `Ended` the release velocity (0 when the finger rested before
    /// lifting); at `Cancelled` it is 0.
    pub velocity: Point,
}

/// Upstream `PanEvent` (internal): a report with the claim's travel and the input.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PanEvent {
    pub state: PanState,
    pub translation: Point,
    /// Movement since the touch began, the claim's travel included.
    pub total_translation: Point,
    pub velocity: Point,
    pub input: PanInput,
}

/// A pan listener.
pub type PanListener = Rc<dyn Fn(&PanGestureEvent)>;
type InternalListener = Rc<dyn Fn(&PanEvent)>;

/// Upstream `PanGestureOptions`.
#[derive(Clone)]
pub struct PanGestureOptions {
    /// Limit the drag to one axis: a drag along the other is not reported, and that
    /// component of every value reads 0.
    pub axis: Option<PanAxis>,
    /// Called as the drag begins, changes, ends or is cancelled. It runs inside the input;
    /// the window refreshes after it. Work that needs the window goes through
    /// `rustryx_primitives::gestures::host::after_gesture`.
    pub on_pan: PanListener,
}

impl PanGestureOptions {
    /// A drag on any axis.
    pub fn new(on_pan: impl Fn(&PanGestureEvent) + 'static) -> Self {
        Self {
            axis: None,
            on_pan: Rc::new(on_pan),
        }
    }

    /// Upstream `axis`.
    pub fn axis(mut self, axis: impl Into<Option<PanAxis>>) -> Self {
        self.axis = axis.into();
        self
    }
}

/// Upstream `PanOptions` (internal): the public options and the recognizer's own.
pub(crate) struct PanConfig {
    pub axis: Option<PanAxis>,
    pub on_pan: InternalListener,
    /// Which pointers and scrolls start the drag, its claim distance, a gate on its begin.
    /// The direction comes from `axis`.
    pub recognizer: Option<PanOptions>,
}

fn along(axis: Option<PanAxis>, p: Point) -> Point {
    match axis {
        Some(PanAxis::X) => Point::new(p.x, 0.),
        Some(PanAxis::Y) => Point::new(0., p.y),
        None => p,
    }
}

/// Upstream `Pan` and `PanGesture`: a pan attached to an area. Cheap to clone; every
/// clone is the same pan. It hands out no recognizer.
#[derive(Clone)]
pub struct PanGesture {
    attachment: GestureAttachment,
    destroyed: Rc<Cell<bool>>,
}

impl PanGesture {
    /// Wraps the element that drags. Render it each frame the element shows.
    pub fn area(&self, child: impl IntoElement) -> GestureArea {
        gesture_area(&self.attachment, child)
    }

    /// Upstream `destroy`: removes every listener the pan added. A drag in progress
    /// reports `Cancelled` once; a second call does nothing.
    pub fn destroy(&self) {
        if self.destroyed.replace(true) {
            return;
        }
        self.attachment.detach();
    }

    /// Whether [`Self::destroy`] ran.
    pub fn is_destroyed(&self) -> bool {
        self.destroyed.get()
    }

    pub(crate) fn attachment(&self) -> &GestureAttachment {
        &self.attachment
    }
}

/// Upstream `createPan`: reports drags on an area of `host` with the recognizer's
/// options.
pub(crate) fn create_pan(host: &GestureHost, config: PanConfig) -> PanGesture {
    let PanConfig {
        axis,
        on_pan,
        recognizer,
    } = config;
    let mut options = recognizer.unwrap_or_default();
    if options.base.name.is_none() {
        options.base.name = Some("pan".into());
    }
    options.direction = match axis {
        Some(PanAxis::X) => PanDirection::Horizontal,
        Some(PanAxis::Y) => PanDirection::Vertical,
        None => PanDirection::Any,
    };
    let pan = PanGestureRecognizer::new(options);
    let input = Cell::new(PanInput::Touch);
    pan.add_listener(move |r| {
        let Some(state) = PanState::of(r.state()) else {
            return;
        };
        if state == PanState::Began {
            input.set(match r.tracked_touches().first() {
                None => PanInput::Touch,
                Some(t) if t.scroll_type().is_some() => PanInput::Scroll,
                Some(t) => match t.pointer_type() {
                    PointerKind::Touch => PanInput::Touch,
                    PointerKind::Mouse => PanInput::Mouse,
                    PointerKind::Pen => PanInput::Pen,
                },
            });
        }
        on_pan(&PanEvent {
            state,
            translation: along(axis, r.translation()),
            total_translation: along(axis, r.total_translation()),
            velocity: if state == PanState::Cancelled {
                Point::ZERO
            } else {
                along(axis, r.velocity())
            },
            input: input.get(),
        });
    });
    PanGesture {
        attachment: host.attach(&[pan.any()]),
        destroyed: Rc::default(),
    }
}

/// Upstream `createPanGesture`: reports drags on an area of `host` as values. Render
/// the element that drags inside [`PanGesture::area`] and call [`PanGesture::destroy`]
/// when it leaves.
///
/// ```ignore
/// let offset = Rc::new(Cell::new(0.));
/// let pan = create_pan_gesture(&gesture_host(window, cx), PanGestureOptions::new({
///     let offset = offset.clone();
///     move |e| offset.set(if e.state == PanState::Changed { e.translation.x } else { 0. })
/// }).axis(PanAxis::X));
/// // Each frame: pan.area(div().id("card").left(px(offset.get() as f32)))
/// ```
pub fn create_pan_gesture(host: &GestureHost, options: PanGestureOptions) -> PanGesture {
    let PanGestureOptions { axis, on_pan } = options;
    // The claim's travel and the input stay internal.
    create_pan(
        host,
        PanConfig {
            axis,
            on_pan: Rc::new(move |e: &PanEvent| {
                on_pan(&PanGestureEvent {
                    state: e.state,
                    translation: e.translation,
                    velocity: e.velocity,
                })
            }),
            recognizer: None,
        },
    )
}

struct HookState {
    pan: PanGesture,
    axis: Option<PanAxis>,
    forwarding: Rc<Cell<bool>>,
    on_pan: Rc<RefCell<PanListener>>,
}

impl Drop for HookState {
    fn drop(&mut self) {
        // Unmount reports nothing: the drag in flight ends silently.
        self.forwarding.set(false);
        self.pan.destroy();
    }
}

fn hook_pan(
    host: &GestureHost,
    axis: Option<PanAxis>,
    on_pan: &Rc<RefCell<PanListener>>,
) -> (PanGesture, Rc<Cell<bool>>) {
    let forwarding = Rc::new(Cell::new(true));
    let (gate, listener) = (forwarding.clone(), on_pan.clone());
    let pan = create_pan_gesture(
        host,
        PanGestureOptions {
            axis,
            on_pan: Rc::new(move |e| {
                if gate.get() {
                    let f = listener.borrow().clone();
                    f(e);
                }
            }),
        },
    );
    (pan, forwarding)
}

/// Upstream `usePanGesture`: reports drags on the element `id` names as values, through
/// [`create_pan_gesture`]. Wrap the element in the returned pan's
/// [`PanGesture::area`] each frame; the pan goes when the element stops rendering.
///
/// ```ignore
/// let pan = use_pan_gesture("card", PanGestureOptions::new(move |e| offset.set(e.translation.x))
///     .axis(PanAxis::X), window, cx);
/// pan.area(div().id("card"))
/// ```
pub fn use_pan_gesture(
    id: impl Into<ElementId>,
    options: PanGestureOptions,
    window: &mut Window,
    cx: &mut App,
) -> PanGesture {
    let id = id.into();
    let host = gesture_host(window, cx);
    let PanGestureOptions { axis, on_pan } = options;
    let state = window.use_keyed_state(ElementId::Name(format!("{id}-pan-gesture").into()), cx, {
        let (host, on_pan) = (host.clone(), on_pan.clone());
        move |_, _| {
            let on_pan = Rc::new(RefCell::new(on_pan));
            let (pan, forwarding) = hook_pan(&host, axis, &on_pan);
            HookState {
                pan,
                axis,
                forwarding,
                on_pan,
            }
        }
    });
    state.update(cx, |s, _| {
        // The latest listener, without re-creating the pan.
        *s.on_pan.borrow_mut() = on_pan;
        if s.axis != axis {
            // A drag in progress ends with no event.
            s.forwarding.set(false);
            s.pan.destroy();
            let (pan, forwarding) = hook_pan(&host, axis, &s.on_pan);
            s.pan = pan;
            s.forwarding = forwarding;
            s.axis = axis;
        }
        s.pan.clone()
    })
}
