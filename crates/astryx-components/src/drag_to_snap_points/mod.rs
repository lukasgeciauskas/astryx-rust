//! The public snap-point drag, upstream `dragToSnapPoints/createDragToSnapPoints.ts`
//! and `hooks/useDragToSnapPoints.ts` (spec:AST-107 FR2, FR6): a drag that moves an
//! element along one axis and settles it on snap points at the release.
//!
//! - [`create_drag_to_snap_points`] is the factory: it lets a drag move a
//!   [`SnapElement`] and hands back a [`DragToSnapPoints`] whose
//!   [`DragToSnapPoints::area`] draws and wraps the element each frame, with
//!   [`DragToSnapPoints::settle_to`] (the keyboard path) and
//!   [`DragToSnapPoints::destroy`].
//! - [`use_drag_to_snap_points`] is its thin hook: one drag per id, created again
//!   when the axis, the snap points or the flick speed change (the element stays
//!   where it is, a settle in flight included), gone with the element. The latest
//!   `on_settle` is the one called, and `settle_to` is the same each frame.
//!
//! The release settles on the nearest snap point. A release faster than
//! `flick_threshold` (1008 px/s by default, UIKit's sheet's flick speed) carries to
//! the next one in its direction, and a release moving back toward the drag's start
//! returns to the snap point it started from (spec:AST-094 FR37). The settle springs
//! on `springs.settle` from the release velocity. Under Reduce Motion the release
//! cross-fades after a touch drag or on a coarse primary pointer, and lands at once
//! elsewhere (design:reduced-motion DR1-DR3). A finger, a mouse and a pen all drive
//! it (AST-094 FR43). Snap points are px offsets from the resting position, sorted
//! before use; unlike BottomSheet's `snap_points`, they are not viewport fractions.
//! A component's own props (BottomSheet's `snap_points`, Carousel's `snap`) stay the
//! component path to the same behavior (FR4).

mod compositor_settle;
pub mod examples;
pub(crate) mod snap_drag;

#[cfg(test)]
mod tests;

use std::cell::RefCell;
use std::rc::Rc;

use astryx_primitives::gestures::host::{GestureHost, gesture_host};
use astryx_primitives::motion::Clock;
use astryx_primitives::{coarse_primary_pointer, prefers_reduced_motion};
use gpui::{AnyElement, App, ElementId, IntoElement, Styled, Window};

pub use snap_drag::{DEFAULT_FLICK_THRESHOLD, SnapElement};

use crate::doc::{ComponentDoc, PropMapping, RustApi};
pub use crate::gestures::pan::PanAxis;
use crate::gestures::pan::{PanConfig, PanEvent, PanGesture, create_pan};
use snap_drag::{SettleOptions, SnapDrag, SnapDragOptions, create_snap_drag};

/// A settle listener: the snap point's index, once the element settles on it.
pub type SettleListener = Rc<dyn Fn(usize, &mut Window, &mut App)>;

/// Upstream `DragToSnapPointsOptions`.
#[derive(Clone)]
pub struct DragToSnapPointsOptions {
    /// The axis the element moves along.
    pub axis: PanAxis,
    /// Offsets in px along `axis` from the element's resting position, sorted before
    /// use (`on_settle` and `settle_to` count in that order). An empty list makes the
    /// drag do nothing and warns in development.
    pub snap_points: Vec<f64>,
    /// A release faster than this, px/s, carries to the next snap point in its
    /// direction. Default 1008.
    pub flick_threshold: Option<f64>,
    /// Called with the snap point's index once the element settles on it. A settle
    /// a new drag or settle interrupts calls nothing.
    pub on_settle: SettleListener,
}

impl DragToSnapPointsOptions {
    /// A drag along `axis` that settles on `snap_points`.
    pub fn new(
        axis: PanAxis,
        snap_points: impl IntoIterator<Item = f64>,
        on_settle: impl Fn(usize, &mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            axis,
            snap_points: snap_points.into_iter().collect(),
            flick_threshold: None,
            on_settle: Rc::new(on_settle),
        }
    }

    /// Upstream `flickThreshold`.
    pub fn flick_threshold(mut self, speed: f64) -> Self {
        self.flick_threshold = Some(speed);
        self
    }
}

/// The engine's clock on the window's executor (fake under the test platform), ms.
pub(crate) fn executor_clock(cx: &App) -> Clock {
    let executor = cx.background_executor().clone();
    let origin = executor.now();
    Rc::new(move || {
        executor
            .now()
            .saturating_duration_since(origin)
            .as_secs_f64()
            * 1000.
    })
}

/// A snap drag with its pan, the internal drag a component attaches.
pub(crate) struct AttachedSnapDrag {
    pub snap: SnapDrag,
    pub pan: PanGesture,
    clock: Clock,
    /// `on_settle` calls the frame owes, made after it with the window.
    reports: Rc<RefCell<Vec<usize>>>,
    on_settle: Rc<RefCell<SettleListener>>,
}

impl AttachedSnapDrag {
    /// Moves the drag on to this frame and draws `child` where the element is,
    /// wrapped in the pan's area. Frames are requested while anything moves.
    pub(crate) fn render<E: IntoElement + Styled>(
        &self,
        child: E,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        let engine = &self.snap.animator;
        engine.set_reduced_motion(prefers_reduced_motion(cx));
        engine.set_coarse_pointer(coarse_primary_pointer(window, cx));
        if !self.snap.is_destroyed() {
            self.snap.frame((self.clock)());
            if self.snap.is_moving() {
                window.request_animation_frame();
            }
        }
        let reports: Vec<usize> = self.reports.borrow_mut().drain(..).collect();
        if !reports.is_empty() {
            let listener = self.on_settle.clone();
            window.defer(cx, move |window, cx| {
                let f = listener.borrow().clone();
                for index in reports {
                    f(index, window, cx);
                }
            });
        }
        let drawn = self.snap.element.draw(child);
        if self.pan.is_destroyed() {
            drawn.into_any_element()
        } else {
            self.pan.area(drawn).into_any_element()
        }
    }

    fn destroy(&self) {
        // Destroyed first, so the pan's cancel moves nothing.
        self.snap.destroy();
        self.pan.destroy();
        self.reports.borrow_mut().clear();
    }
}

/// Attaches a snap drag to `element` on `host`: the pan feeds it, and its
/// `on_settle` reports through `listener` after the frame that saw the settle end.
pub(crate) fn attach_snap_drag(
    element: &SnapElement,
    host: &GestureHost,
    options: SnapDragOptions,
    listener: Rc<RefCell<SettleListener>>,
) -> AttachedSnapDrag {
    let reports: Rc<RefCell<Vec<usize>>> = Rc::default();
    let queue = reports.clone();
    let clock = options.clock.clone();
    let axis = options.axis;
    let recognizer = options.pan.clone();
    let snap = create_snap_drag(
        element,
        SnapDragOptions {
            on_settle: Rc::new(move |index| queue.borrow_mut().push(index)),
            ..options
        },
    );
    let fed = snap.clone();
    let pan = create_pan(
        host,
        PanConfig {
            axis: Some(axis),
            on_pan: Rc::new(move |event: &PanEvent| fed.on_pan(event)),
            recognizer,
        },
    );
    AttachedSnapDrag {
        snap,
        pan,
        clock,
        reports,
        on_settle: listener,
    }
}

/// Upstream `DragToSnapPoints`: a drag that settles an element on snap points.
/// Cheap to clone; every clone is the same drag.
#[derive(Clone)]
pub struct DragToSnapPoints {
    element: SnapElement,
    attached: Option<Rc<AttachedSnapDrag>>,
}

impl DragToSnapPoints {
    /// Draws `child` where the element is and lets a drag move it. Render it each
    /// frame the element shows; after [`Self::destroy`] it draws the element where
    /// it stopped, and no drag moves it.
    pub fn area<E: IntoElement + Styled>(
        &self,
        child: E,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        match &self.attached {
            Some(attached) => attached.render(child, window, cx),
            None => self.element.draw(child).into_any_element(),
        }
    }

    /// Upstream `settleTo`: settles the element on the snap point at `index` without
    /// a drag (the keyboard path), then calls `on_settle`. An index outside the snap
    /// points does nothing and warns in development.
    pub fn settle_to(&self, index: usize, cx: &mut App) {
        if let Some(attached) = &self.attached {
            attached.snap.settle_to(index, SettleOptions::default());
            cx.refresh_windows();
        }
    }

    /// Upstream `destroy`: removes every listener and stops the element where it is.
    pub fn destroy(&self) {
        if let Some(attached) = &self.attached {
            attached.destroy();
        }
    }

    /// The element it moves.
    pub fn element(&self) -> &SnapElement {
        &self.element
    }

    #[cfg(test)]
    pub(crate) fn snap(&self) -> Option<&SnapDrag> {
        self.attached.as_ref().map(|a| &a.snap)
    }

    #[cfg(test)]
    pub(crate) fn pan(&self) -> Option<&PanGesture> {
        self.attached.as_ref().map(|a| &a.pan)
    }
}

/// Upstream `createDragToSnapPoints`: lets a drag on `host` move `element` along
/// `options.axis` and settles it on `options.snap_points` at the release, then calls
/// `on_settle(index)`. Render the element through [`DragToSnapPoints::area`] each
/// frame, and call [`DragToSnapPoints::destroy`] when it leaves.
///
/// ```ignore
/// let element = SnapElement::new();
/// let drawer = create_drag_to_snap_points(&element, &gesture_host(window, cx),
///     DragToSnapPointsOptions::new(PanAxis::Y, [0., 240., 480.], |index, _, _| {
///         println!("settled on {index}");
///     }), cx);
/// // Each frame: drawer.area(div().id("drawer"), window, cx)
/// // From a button: drawer.settle_to(0, cx)
/// ```
pub fn create_drag_to_snap_points(
    element: &SnapElement,
    host: &GestureHost,
    options: DragToSnapPointsOptions,
    cx: &App,
) -> DragToSnapPoints {
    let listener = Rc::new(RefCell::new(options.on_settle.clone()));
    create_with_listener(element, host, options, listener, cx)
}

fn create_with_listener(
    element: &SnapElement,
    host: &GestureHost,
    options: DragToSnapPointsOptions,
    listener: Rc<RefCell<SettleListener>>,
    cx: &App,
) -> DragToSnapPoints {
    if !options.snap_points.iter().any(|p| p.is_finite()) {
        snap_drag::dev_warn(
            "createDragToSnapPoints",
            "snapPoints is empty, so the drag does nothing.",
        );
        return DragToSnapPoints {
            element: element.clone(),
            attached: None,
        };
    }
    // The public model: one for one from where the element was caught, and Core's
    // one release policy.
    let attached = attach_snap_drag(
        element,
        host,
        SnapDragOptions {
            axis: options.axis,
            snap_points: options.snap_points,
            flick_threshold: options.flick_threshold,
            on_settle: Rc::new(|_| {}),
            pan: None,
            custom_property: None,
            compositor_parts: None,
            model: None,
            clock: executor_clock(cx),
        },
        listener,
    );
    DragToSnapPoints {
        element: element.clone(),
        attached: Some(Rc::new(attached)),
    }
}

/// What [`use_drag_to_snap_points`] returns (upstream `UseDragToSnapPointsResult`).
#[derive(Clone)]
pub struct UseDragToSnapPointsResult {
    drag: Rc<RefCell<DragToSnapPoints>>,
}

impl UseDragToSnapPointsResult {
    /// Upstream `settleTo`: settles the element on the snap point at `index` without
    /// a drag, the keyboard path. The same on every frame.
    pub fn settle_to(&self, index: usize, cx: &mut App) {
        let drag = self.drag.borrow().clone();
        drag.settle_to(index, cx);
    }

    /// Draws the element the hook's id names and lets a drag move it. Render it each
    /// frame the element shows.
    pub fn area<E: IntoElement + Styled>(
        &self,
        child: E,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        let drag = self.drag.borrow().clone();
        drag.area(child, window, cx)
    }

    #[cfg(test)]
    pub(crate) fn current(&self) -> DragToSnapPoints {
        self.drag.borrow().clone()
    }

    #[cfg(test)]
    pub(crate) fn shared(&self) -> Rc<RefCell<DragToSnapPoints>> {
        self.drag.clone()
    }
}

struct HookState {
    element: SnapElement,
    key: (PanAxis, Vec<u64>, Option<u64>),
    drag: Rc<RefCell<DragToSnapPoints>>,
    on_settle: Rc<RefCell<SettleListener>>,
}

impl Drop for HookState {
    fn drop(&mut self) {
        self.drag.borrow().destroy();
    }
}

fn hook_key(options: &DragToSnapPointsOptions) -> (PanAxis, Vec<u64>, Option<u64>) {
    (
        options.axis,
        options.snap_points.iter().map(|p| p.to_bits()).collect(),
        options.flick_threshold.map(f64::to_bits),
    )
}

/// Upstream `useDragToSnapPoints`: lets a drag move the element `id` names along
/// `options.axis` and settles it on `options.snap_points` at the release, through
/// [`create_drag_to_snap_points`]. Render the element through
/// [`UseDragToSnapPointsResult::area`] each frame; the drag ends when the element
/// stops rendering.
///
/// ```ignore
/// let snap = use_drag_to_snap_points("drawer",
///     DragToSnapPointsOptions::new(PanAxis::Y, [0., 240., 480.], move |i, _, cx| {
///         place.update(cx, |p, _| *p = i);
///     }), window, cx);
/// snap.area(div().id("drawer"), window, cx)
/// ```
pub fn use_drag_to_snap_points(
    id: impl Into<ElementId>,
    options: DragToSnapPointsOptions,
    window: &mut Window,
    cx: &mut App,
) -> UseDragToSnapPointsResult {
    let id = id.into();
    let host = gesture_host(window, cx);
    let key = hook_key(&options);
    let state = window.use_keyed_state(
        ElementId::Name(format!("{id}-drag-to-snap-points").into()),
        cx,
        {
            let (host, options, key) = (host.clone(), options.clone(), key.clone());
            move |_, cx| {
                let element = SnapElement::new();
                let on_settle = Rc::new(RefCell::new(options.on_settle.clone()));
                let drag = create_with_listener(&element, &host, options, on_settle.clone(), cx);
                HookState {
                    element,
                    key,
                    drag: Rc::new(RefCell::new(drag)),
                    on_settle,
                }
            }
        },
    );
    state.update(cx, |s, cx| {
        // The latest listener, without creating the drag again.
        *s.on_settle.borrow_mut() = options.on_settle.clone();
        if s.key != key {
            // A new axis, snap points or flick speed creates the drag again, and the
            // element stays where it is.
            s.drag.borrow().destroy();
            let drag = create_with_listener(&s.element, &host, options, s.on_settle.clone(), cx);
            *s.drag.borrow_mut() = drag;
            s.key = key;
        }
        UseDragToSnapPointsResult {
            drag: s.drag.clone(),
        }
    })
}

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

pub const CREATE_DRAG_TO_SNAP_POINTS_DOC: ComponentDoc = ComponentDoc {
    name: "createDragToSnapPoints",
    rust_type: "astryx::components::drag_to_snap_points::create_drag_to_snap_points -> DragToSnapPoints",
    props: &[
        p(
            "element",
            RustApi::Argument(
                "element (a SnapElement, drawn where the element renders through DragToSnapPoints::area)",
            ),
        ),
        p(
            "options.axis",
            RustApi::Argument("DragToSnapPointsOptions::new(axis, ..)"),
        ),
        p(
            "options.snapPoints",
            RustApi::Argument("DragToSnapPointsOptions::new(.., snap_points, ..)"),
        ),
        p(
            "options.flickThreshold",
            RustApi::Method("DragToSnapPointsOptions::flick_threshold"),
        ),
        p(
            "options.onSettle",
            RustApi::Argument("DragToSnapPointsOptions::new(.., on_settle)"),
        ),
    ],
    example: include_str!("examples.rs"),
};

pub const USE_DRAG_TO_SNAP_POINTS_DOC: ComponentDoc = ComponentDoc {
    name: "useDragToSnapPoints",
    rust_type: "astryx::components::drag_to_snap_points::use_drag_to_snap_points -> UseDragToSnapPointsResult",
    props: &[
        p(
            "ref",
            RustApi::Argument(
                "id (the element's id; render it through UseDragToSnapPointsResult::area)",
            ),
        ),
        p(
            "options.axis",
            RustApi::Argument("DragToSnapPointsOptions::new(axis, ..)"),
        ),
        p(
            "options.snapPoints",
            RustApi::Argument("DragToSnapPointsOptions::new(.., snap_points, ..)"),
        ),
        p(
            "options.flickThreshold",
            RustApi::Method("DragToSnapPointsOptions::flick_threshold"),
        ),
        p(
            "options.onSettle",
            RustApi::Argument("DragToSnapPointsOptions::new(.., on_settle)"),
        ),
    ],
    example: include_str!("examples.rs"),
};
