//! The drag under the public snap-point drag, upstream `createSnapDrag` in
//! `dragToSnapPoints/createDragToSnapPoints.ts`: it takes a model of where a drag
//! draws its element and where a release lands, the pan's own options, a custom
//! property to draw into and the parts a settle moves on the compositor. No public
//! path exports it; [`super::create_drag_to_snap_points`] is the public drag.
//!
//! GPUI has no element to write inline styles on, so the element is a
//! [`SnapElement`]: the values upstream's animator leaves on the element's style
//! (its `translate`, its custom properties, and a compositor animation's
//! `translate` on top), which the caller draws each frame. The engine runs on an
//! injected clock and moves on at each [`SnapDrag::frame`]: a settle's end, a
//! settle's `on_pass` and a compositor settle's frames are read there, as upstream
//! reads them in animation frames.

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::Rc;

use astryx_primitives::gestures::PanOptions;
use astryx_primitives::motion::uikit::springs::role as springs;
use astryx_primitives::motion::{
    AnimateOptions, Clock, EngineOptions, MotionEngine, MotionHandle, MotionProperty, MotionSpring,
    MotionValues, ReducedMotion, SpringTransition, StartMode, resolve_motion::resolve_spring,
    resolve_snap_point::SnapPointQuery, resolve_snap_point::resolve_snap_point, solve_spring,
};
use gpui::{SharedString, Styled, px};

use super::compositor_settle::{CompositorPart, CompositorSettle, OnPass, play_compositor_settle};
use crate::gestures::pan::{PanAxis, PanEvent, PanInput, PanState};

/// The release speed, px/s, above which a drag carries to the next snap point: the
/// flick speed measured for UIKit's sheet on iOS 26.2 (upstream
/// `DEFAULT_FLICK_THRESHOLD`).
pub const DEFAULT_FLICK_THRESHOLD: f64 = 1008.;

thread_local! {
    #[cfg(test)]
    pub(crate) static WARNINGS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

/// Upstream `devWarn`: a development warning, each time it is raised.
pub(crate) fn dev_warn(source: &str, message: &str) {
    let text = format!("{source}: {message}");
    #[cfg(test)]
    WARNINGS.with(|w| w.borrow_mut().push(text.clone()));
    if cfg!(debug_assertions) && !cfg!(test) {
        eprintln!("astryx: {text}");
    }
}

/// The element a drag moves, as upstream's animator leaves it: what its inline
/// style holds. Cheap to clone; every clone is the same element. Draw it where the
/// element renders with [`SnapElement::draw`].
#[derive(Clone, Default)]
pub struct SnapElement(Rc<ElementStyle>);

#[derive(Default)]
struct ElementStyle {
    /// The `translate` the animator wrote, `None` for none.
    translate: Cell<Option<(f64, f64)>>,
    /// Custom properties the animator wrote, by name.
    custom: RefCell<BTreeMap<SharedString, f64>>,
    /// A compositor animation's `translate` on top of the style, `None` for none.
    composited: Cell<Option<(f64, f64)>>,
}

impl SnapElement {
    /// An element with no inline style.
    pub fn new() -> Self {
        Self::default()
    }

    /// The inline `translate`, `None` when there is none.
    pub fn translate(&self) -> Option<(f64, f64)> {
        self.0.translate.get()
    }

    /// A custom property's value, `None` when it is not set.
    pub fn property(&self, name: &str) -> Option<f64> {
        self.0.custom.borrow().get(name).copied()
    }

    /// A compositor animation's `translate` playing on the element now.
    pub fn composited(&self) -> Option<(f64, f64)> {
        self.0.composited.get()
    }

    pub(crate) fn set_translate(&self, value: (f64, f64)) {
        self.0.translate.set((value != (0., 0.)).then_some(value));
    }

    pub(crate) fn set_property(&self, name: &SharedString, value: f64) {
        let mut custom = self.0.custom.borrow_mut();
        if value == 0. {
            custom.remove(name);
        } else {
            custom.insert(name.clone(), value);
        }
    }

    pub(crate) fn set_composited(&self, value: Option<(f64, f64)>) {
        self.0.composited.set(value);
    }

    /// Where the element is drawn: its translate and any compositor translate.
    pub fn offset(&self) -> (f64, f64) {
        let (x, y) = self.translate().unwrap_or_default();
        let (cx, cy) = self.composited().unwrap_or_default();
        (x + cx, y + cy)
    }

    /// Draws `el` where the element is drawn.
    pub fn draw<E: Styled>(&self, el: E) -> E {
        let (x, y) = self.offset();
        if x == 0. && y == 0. {
            el
        } else {
            el.translate(px(x as f32), px(y as f32))
        }
    }
}

/// A drag in progress, along the axis, as a model reads it (upstream `SnapDragMove`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SnapDragMove {
    /// Where the element was drawn when the drag began (it was caught there).
    pub start: f64,
    /// Travel since the drag began, px: the pan's claim is not part of it.
    pub translation: f64,
    /// Travel since the touch began, px, the pan's claim included.
    pub total: f64,
    /// What drives the drag.
    pub input: PanInput,
}

/// A release, along the axis, as a model reads it (upstream `SnapDragRelease`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SnapDragRelease {
    pub drag: SnapDragMove,
    /// The release velocity, px/s (0 when the contact rested).
    pub velocity: f64,
}

/// Where a model draws the element during a drag (upstream `SnapDragTarget`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SnapDragTarget {
    pub offset: f64,
    /// Follows on this spring from where the element is drawn, keeping its velocity;
    /// without one the element is set there, one for one.
    pub spring: Option<MotionSpring>,
}

/// Where a model lands a release (upstream `SnapDragLanding`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct SnapDragLanding {
    pub index: usize,
    /// The settle's starting velocity, px/s; the one the element is drawn with
    /// without it.
    pub velocity: Option<f64>,
    /// The settle's spring; the settle spring without one.
    pub spring: Option<MotionSpring>,
}

type BeginFn = Rc<dyn Fn(&SnapDragMove) -> bool>;
type MoveFn = Rc<dyn Fn(&SnapDragMove) -> Option<SnapDragTarget>>;
type ReleaseFn = Rc<dyn Fn(&SnapDragRelease) -> Option<SnapDragLanding>>;
type CancelFn = Rc<dyn Fn(&SnapDragMove) -> Option<SnapDragLanding>>;
type PartsFn = Rc<dyn Fn() -> Vec<CompositorPart>>;
/// A compositor settle that starts at the next frame.
type WaitingPlay = Box<dyn FnOnce(&SnapDrag)>;

/// A drag's model (upstream `SnapDragModel`): where it draws the element and where a
/// release lands.
#[derive(Clone, Default)]
pub(crate) struct SnapDragModel {
    /// A drag began; false leaves the element as it is for this drag.
    pub begin: Option<BeginFn>,
    /// Where the drag draws the element; `None` leaves it as it is (a dead zone).
    pub move_to: Option<MoveFn>,
    /// Where the release lands; `None` leaves the element as it is.
    pub release: Option<ReleaseFn>,
    /// Where a cancelled drag lands; `None` leaves the element as it is.
    pub cancel: Option<CancelFn>,
}

/// A companion drawn from the travel in the same frames (upstream
/// `customProperty.companion`).
#[derive(Clone)]
pub(crate) struct Companion {
    pub name: SharedString,
    pub map: Rc<dyn Fn(f64) -> f64>,
}

/// The custom property an x drag draws into instead of `translate`.
#[derive(Clone)]
pub(crate) struct CustomProperty {
    pub name: SharedString,
    pub companion: Option<Companion>,
}

/// Options of [`create_snap_drag`] (upstream `SnapDragOptions`).
#[derive(Clone)]
pub(crate) struct SnapDragOptions {
    pub axis: PanAxis,
    /// Offsets in px along `axis` from the element's resting position, sorted before
    /// use.
    pub snap_points: Vec<f64>,
    pub flick_threshold: Option<f64>,
    pub on_settle: Rc<dyn Fn(usize)>,
    /// The pan's own options: which pointers and scrolls start it, its claim.
    pub pan: Option<PanOptions>,
    pub custom_property: Option<CustomProperty>,
    pub compositor_parts: Option<PartsFn>,
    pub model: Option<SnapDragModel>,
    /// The engine's clock, ms.
    pub clock: Clock,
}

/// How [`SnapDrag::settle_to`] settles (upstream `SettleOptions`).
#[derive(Clone, Default)]
pub(crate) struct SettleOptions {
    pub velocity: Option<f64>,
    pub spring: Option<MotionSpring>,
    /// A touch drag's release starts it (Reduce Motion cross-fades).
    pub touch_release: bool,
    pub on_pass: Option<OnPass>,
    /// Asked for in an input event's own task: from rest, it starts at the first
    /// frame after it (upstream `start: 'next-frame'`).
    pub next_frame: bool,
}

/// One `animate` the drag made, for tests.
#[cfg(test)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct AnimateCall {
    pub target: f64,
    pub spring: MotionSpring,
    pub velocity: Option<f64>,
    pub options: AnimateOptions,
}

struct Drag {
    start: f64,
    start_index: usize,
    input: PanInput,
    /// The drag has drawn the element: it caught any settle in flight.
    caught: bool,
}

struct Flight {
    turn: u64,
    options: SettleOptions,
    /// Its `on_pass` has called back.
    passed: Rc<Cell<bool>>,
    /// The animator's motion, until it ends (none on the compositor).
    handle: Option<MotionHandle>,
    finished: bool,
}

struct State {
    options: SnapDragOptions,
    flick_threshold: f64,
    points: Vec<f64>,
    target_index: usize,
    turn: u64,
    drag: Option<Drag>,
    destroyed: bool,
    in_flight: Option<Flight>,
    compositor: Option<CompositorSettle>,
    /// A compositor settle waiting for its frame (`next_frame`).
    waiting_play: Option<WaitingPlay>,
    /// Calls owed to `on_settle`, made at the end of the frame or call.
    reports: Vec<usize>,
    #[cfg(test)]
    calls: Vec<AnimateCall>,
}

/// What [`create_snap_drag`] returns (upstream `SnapDrag`). Cheap to clone.
#[derive(Clone)]
pub(crate) struct SnapDrag {
    state: Rc<RefCell<State>>,
    /// The animator that draws the element.
    pub animator: MotionEngine,
    pub element: SnapElement,
}

fn axis_property(axis: PanAxis) -> MotionProperty {
    match axis {
        PanAxis::X => MotionProperty::X,
        PanAxis::Y => MotionProperty::Y,
    }
}

fn values(axis: PanAxis, value: f64) -> MotionValues {
    match axis {
        PanAxis::X => MotionValues::new().x(value),
        PanAxis::Y => MotionValues::new().y(value),
    }
}

/// Drops a point that is not a finite number, with a dev warning, and sorts.
fn sorted_snap_points(points: &[f64]) -> Vec<f64> {
    let mut usable: Vec<f64> = points.iter().copied().filter(|p| p.is_finite()).collect();
    if usable.len() < points.len() {
        dev_warn(
            "createDragToSnapPoints",
            "snapPoints must be numbers; the others are ignored.",
        );
    }
    usable.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    usable
}

fn nearest_index(points: &[f64], position: f64, flick_threshold: f64) -> usize {
    resolve_snap_point(&SnapPointQuery {
        points: points.to_vec(),
        position,
        velocity: 0.,
        flick_threshold,
        ..Default::default()
    })
    .map_or(0, |r| r.index)
}

/// Lets a drag move `element` along the axis and settles it on its snap points
/// (upstream `createSnapDrag`). It starts where the element is drawn, so a drag
/// created again to change its options moves on from there instead of jumping. Feed
/// it the pan's reports with [`SnapDrag::on_pan`] and call [`SnapDrag::frame`]
/// each frame.
pub(crate) fn create_snap_drag(element: &SnapElement, options: SnapDragOptions) -> SnapDrag {
    let axis = options.axis;
    if options.custom_property.is_some() && axis != PanAxis::X {
        dev_warn(
            "createSnapDrag",
            "customProperty draws the x axis only, so this y drag draws nothing.",
        );
    }
    let flick_threshold = options.flick_threshold.unwrap_or(DEFAULT_FLICK_THRESHOLD);
    let points = sorted_snap_points(&options.snap_points);
    // Where the element is drawn now: a drag destroyed and created again leaves it
    // where it stopped.
    let drawn = match &options.custom_property {
        Some(property) => (element.property(&property.name).unwrap_or(0.), 0.),
        None => element.translate().unwrap_or_default(),
    };
    let animator = MotionEngine::new(EngineOptions {
        initial: MotionValues::new().x(drawn.0).y(drawn.1),
        now: Some(options.clock.clone()),
        frames: true,
    });
    let along = if axis == PanAxis::X { drawn.0 } else { drawn.1 };
    let target_index = if points.is_empty() {
        0
    } else {
        nearest_index(&points, along, flick_threshold)
    };
    SnapDrag {
        state: Rc::new(RefCell::new(State {
            options,
            flick_threshold,
            points,
            target_index,
            turn: 0,
            drag: None,
            destroyed: false,
            in_flight: None,
            compositor: None,
            waiting_play: None,
            reports: Vec::new(),
            #[cfg(test)]
            calls: Vec::new(),
        })),
        animator,
        element: element.clone(),
    }
}

impl SnapDrag {
    fn axis(&self) -> PanAxis {
        self.state.borrow().options.axis
    }

    fn now(&self) -> f64 {
        self.animator.now()
    }

    /// Where the element is on screen now, along the axis.
    pub fn position(&self) -> f64 {
        let presented = self
            .state
            .borrow()
            .compositor
            .as_ref()
            .map(|c| c.presented(self.now()));
        presented.unwrap_or_else(|| self.animator.get(axis_property(self.axis()), None))
    }

    /// The snap points, ascending.
    #[allow(
        dead_code,
        reason = "createSnapDrag's API for a Core component with its own model"
    )]
    pub fn snap_points(&self) -> Vec<f64> {
        self.state.borrow().points.clone()
    }

    /// Whether [`Self::destroy`] ran.
    pub fn is_destroyed(&self) -> bool {
        self.state.borrow().destroyed
    }

    /// Whether anything still moves: the element, a waiting settle, an `on_pass`.
    pub fn is_moving(&self) -> bool {
        let state = self.state.borrow();
        !self.animator.is_settled()
            || state.compositor.is_some()
            || state.waiting_play.is_some()
            || state.in_flight.as_ref().is_some_and(|f| !f.finished)
            || !state.reports.is_empty()
    }

    #[cfg(test)]
    pub(crate) fn calls(&self) -> Vec<AnimateCall> {
        self.state.borrow().calls.clone()
    }

    #[cfg(test)]
    pub(crate) fn compositor_duration(&self) -> Option<f64> {
        self.state
            .borrow()
            .compositor
            .as_ref()
            .map(|c| c.duration_ms())
    }

    #[cfg(test)]
    pub(crate) fn clear_calls(&self) {
        self.state.borrow_mut().calls.clear();
    }

    /// Writes what the animator draws onto the element, as upstream's animator
    /// writes its inline style.
    fn draw(&self) {
        let (axis, property) = {
            let state = self.state.borrow();
            (state.options.axis, state.options.custom_property.clone())
        };
        let value = self.animator.get(axis_property(axis), None);
        match property {
            Some(property) => {
                if axis == PanAxis::X {
                    self.element.set_property(&property.name, value);
                    if let Some(companion) = &property.companion {
                        self.element
                            .set_property(&companion.name, (companion.map)(value));
                    }
                }
            }
            None => {
                let x = self.animator.get(MotionProperty::X, None);
                let y = self.animator.get(MotionProperty::Y, None);
                self.element.set_translate((x, y));
            }
        }
    }

    fn report(&self) {
        let (reports, on_settle) = {
            let mut state = self.state.borrow_mut();
            (
                std::mem::take(&mut state.reports),
                state.options.on_settle.clone(),
            )
        };
        for index in reports {
            on_settle(index);
        }
    }

    /// Stops a compositor settle where it is on screen, and draws it there.
    fn hold_compositor(&self) {
        let now = self.now();
        let held = {
            let mut state = self.state.borrow_mut();
            state.compositor.take().map(|c| c.stop(now))
        };
        if let Some(value) = held {
            self.animator.set(values(self.axis(), value));
        }
    }

    fn settle(&self, target: usize, settle: SettleOptions) -> Option<MotionHandle> {
        let now = self.now();
        let (mine, axis, parts, to) = {
            let mut state = self.state.borrow_mut();
            state.target_index = target;
            state.turn += 1;
            let mine = state.turn;
            state.in_flight = Some(Flight {
                turn: mine,
                options: settle.clone(),
                passed: Rc::new(Cell::new(false)),
                handle: None,
                finished: false,
            });
            state.waiting_play = None;
            (
                mine,
                state.options.axis,
                state.options.compositor_parts.clone(),
                state.points[target],
            )
        };
        let spring = settle.spring.unwrap_or(springs::SETTLE);
        let property = axis_property(axis);
        let touch_release = settle.touch_release;
        if let Some(parts) = parts
            && self.animator.reduced_motion(touch_release) == ReducedMotion::Off
        {
            // A zero velocity keeps the element's own, as on the animator.
            let velocity = match settle.velocity {
                Some(v) if v != 0. => v,
                _ => {
                    let state = self.state.borrow();
                    state
                        .compositor
                        .as_ref()
                        .map(|c| c.velocity(now))
                        .unwrap_or_else(|| self.animator.get_velocity(property, None))
                }
            };
            let from = self.position();
            self.hold_compositor();
            // At the theme's tempo, as the animator plays a role spring.
            let resolved = resolve_spring(spring, self.animator.tempo()).to_spring();
            let curve = solve_spring(resolved, from, to, velocity);
            let on_pass = settle.on_pass.clone();
            let passed = self
                .state
                .borrow()
                .in_flight
                .as_ref()
                .map(|f| f.passed.clone())
                .unwrap_or_default();
            let play = move |snap: &SnapDrag| {
                {
                    let state = snap.state.borrow();
                    if state.destroyed || mine != state.turn {
                        return;
                    }
                }
                snap.animator.stop(None);
                snap.animator.set(values(axis, to));
                snap.draw();
                let on_pass = on_pass.map(|p| {
                    let passed = passed.clone();
                    let call = p.call.clone();
                    OnPass {
                        call: Rc::new(move || {
                            passed.set(true);
                            call();
                        }),
                        ..p
                    }
                });
                let settle = play_compositor_settle(curve, parts(), on_pass, snap.now());
                snap.state.borrow_mut().compositor = Some(settle);
            };
            if settle.next_frame {
                self.state.borrow_mut().waiting_play = Some(Box::new(play));
            } else {
                play(self);
            }
            return None;
        }
        self.hold_compositor();
        let velocity = settle.velocity.filter(|v| *v != 0.);
        let mut transition = SpringTransition::new(spring);
        if let Some(v) = velocity {
            transition = transition.velocity(values(axis, v));
        }
        let options = AnimateOptions {
            touch_release,
            start: settle.next_frame.then_some(StartMode::NextFrame),
            ..Default::default()
        };
        #[cfg(test)]
        self.state.borrow_mut().calls.push(AnimateCall {
            target: to,
            spring,
            velocity,
            options,
        });
        let handle = self.animator.animate(values(axis, to), transition, options);
        if let Some(flight) = self.state.borrow_mut().in_flight.as_mut()
            && flight.turn == mine
        {
            flight.handle = Some(handle.clone());
        }
        self.draw();
        Some(handle)
    }

    /// Lands a model's landing (`None`: the element stays as it is).
    fn land(&self, landing: Option<SnapDragLanding>, touch_release: bool) {
        if let Some(landing) = landing {
            self.settle(
                landing.index,
                SettleOptions {
                    velocity: landing.velocity,
                    spring: landing.spring,
                    touch_release,
                    ..Default::default()
                },
            );
        }
    }

    /// A pan's report (upstream `createPan`'s `onPan`).
    pub(crate) fn on_pan(&self, event: &PanEvent) {
        let (axis, model) = {
            let state = self.state.borrow();
            if state.destroyed || state.points.is_empty() {
                return;
            }
            (
                state.options.axis,
                state.options.model.clone().unwrap_or_default(),
            )
        };
        let along = |p: astryx_primitives::gestures::Point| match axis {
            PanAxis::X => p.x,
            PanAxis::Y => p.y,
        };
        let (translation, total) = (along(event.translation), along(event.total_translation));
        if event.state == PanState::Began {
            let start = self.position();
            let index = self.state.borrow().target_index;
            self.state.borrow_mut().drag = Some(Drag {
                start,
                start_index: index,
                input: event.input,
                caught: false,
            });
            let reading = SnapDragMove {
                start,
                translation,
                total,
                input: event.input,
            };
            if let Some(begin) = &model.begin
                && !begin(&reading)
            {
                self.state.borrow_mut().drag = None;
                return;
            }
        }
        let Some((start, input)) = self
            .state
            .borrow()
            .drag
            .as_ref()
            .map(|d| (d.start, d.input))
        else {
            return;
        };
        let reading = SnapDragMove {
            start,
            translation,
            total,
            input,
        };
        if matches!(event.state, PanState::Began | PanState::Changed) {
            let target = match &model.move_to {
                Some(move_to) => move_to(&reading),
                None => Some(SnapDragTarget {
                    offset: start + translation,
                    spring: None,
                }),
            };
            let Some(target) = target else {
                return;
            };
            let catch = self.state.borrow().drag.as_ref().is_some_and(|d| !d.caught);
            if catch {
                // Catch a settle in flight where it is on screen: it reports nothing.
                {
                    let mut state = self.state.borrow_mut();
                    if let Some(drag) = state.drag.as_mut() {
                        drag.caught = true;
                    }
                    state.turn += 1;
                    state.waiting_play = None;
                }
                self.hold_compositor();
            }
            match target.spring {
                None => self.animator.set(values(axis, target.offset)),
                Some(spring) => {
                    // On the drag's own clock, now: a target that waited for a frame
                    // would be replaced by the next move before it starts.
                    let options = AnimateOptions {
                        at: Some(self.now()),
                        ..Default::default()
                    };
                    #[cfg(test)]
                    self.state.borrow_mut().calls.push(AnimateCall {
                        target: target.offset,
                        spring,
                        velocity: None,
                        options,
                    });
                    self.animator.animate(
                        values(axis, target.offset),
                        SpringTransition::new(spring),
                        options,
                    );
                }
            }
            self.draw();
            return;
        }
        let Some(drag) = self.state.borrow_mut().drag.take() else {
            return;
        };
        // Only a touch drag's release cross-fades under Reduce Motion.
        let touch_release = input == PanInput::Touch;
        if event.state == PanState::Cancelled {
            match &model.cancel {
                Some(cancel) => self.land(cancel(&reading), touch_release),
                None => {
                    self.settle(
                        drag.start_index,
                        SettleOptions {
                            velocity: Some(0.),
                            touch_release,
                            ..Default::default()
                        },
                    );
                }
            }
            self.report();
            return;
        }
        let velocity = along(event.velocity);
        if let Some(release) = &model.release {
            self.land(
                release(&SnapDragRelease {
                    drag: reading,
                    velocity,
                }),
                touch_release,
            );
            self.report();
            return;
        }
        // Core's one release policy. The drag's start lets a release that moves back
        // return to the snap point it started from.
        let (points, flick_threshold) = {
            let state = self.state.borrow();
            (state.points.clone(), state.flick_threshold)
        };
        let target = resolve_snap_point(&SnapPointQuery {
            points,
            position: self.position(),
            velocity,
            flick_threshold,
            start: Some(drag.start),
            ..Default::default()
        })
        .map_or(0, |r| r.index);
        self.settle(
            target,
            SettleOptions {
                velocity: Some(velocity),
                touch_release,
                ..Default::default()
            },
        );
        self.report();
    }

    /// Settles the element on the snap point at `index`, then calls `on_settle`.
    /// Returns the settle's motion, or `None` when it does nothing or the compositor
    /// plays it.
    pub fn settle_to(&self, index: usize, options: SettleOptions) -> Option<MotionHandle> {
        let count = {
            let state = self.state.borrow();
            if state.destroyed {
                return None;
            }
            state.points.len()
        };
        if index >= count {
            dev_warn(
                "createDragToSnapPoints",
                &format!(
                    "settleTo({index}) is outside the {count} snap points, so it does nothing."
                ),
            );
            return None;
        }
        self.state.borrow_mut().drag = None;
        let options = SettleOptions {
            velocity: Some(options.velocity.unwrap_or(0.)),
            ..options
        };
        self.settle(index, options)
    }

    /// Replaces the snap points (a layout change). A settle in flight re-aims at the
    /// new point nearest where it was heading; otherwise the element stays where it
    /// is. An empty list stops the element where it is and ends a drag in progress,
    /// and nothing reports. After `destroy` it does nothing.
    #[allow(
        dead_code,
        reason = "createSnapDrag's API for a Core component with its own model"
    )]
    pub fn set_snap_points(&self, snap_points: &[f64]) {
        let (flight, heading, start_point, start, flick_threshold) = {
            let state = self.state.borrow();
            if state.destroyed {
                return;
            }
            // A drag that has not caught the settle leaves it running.
            let flight = (state.drag.as_ref().is_none_or(|d| !d.caught))
                .then(|| {
                    state
                        .in_flight
                        .as_ref()
                        .filter(|f| f.turn == state.turn && !f.finished)
                        .map(|f| (f.options.clone(), f.passed.get()))
                })
                .flatten();
            let heading = flight.as_ref().map(|_| state.points[state.target_index]);
            let start_point = state.drag.as_ref().map(|d| state.points[d.start_index]);
            let start = state.drag.as_ref().map(|d| d.start);
            (flight, heading, start_point, start, state.flick_threshold)
        };
        let points = sorted_snap_points(snap_points);
        self.state.borrow_mut().points = points.clone();
        if points.is_empty() {
            // No point to land on: a running settle stops and never reports, and a
            // drag in progress ends.
            {
                let mut state = self.state.borrow_mut();
                state.turn += 1;
                state.drag = None;
                state.waiting_play = None;
                state.target_index = 0;
            }
            self.hold_compositor();
            self.animator.stop(None);
            self.draw();
            return;
        }
        let nearest = |position: f64| nearest_index(&points, position, flick_threshold);
        if let (Some((options, passed)), Some(heading)) = (flight, heading) {
            // The re-aim keeps the motion's own velocity, and an `on_pass` that has
            // called back never calls again.
            let target = nearest(heading);
            self.settle(
                target,
                SettleOptions {
                    velocity: Some(0.),
                    on_pass: if passed {
                        None
                    } else {
                        options.on_pass.clone()
                    },
                    ..options
                },
            );
            if let Some(start) = start {
                let index = nearest(start_point.unwrap_or(start));
                if let Some(drag) = self.state.borrow_mut().drag.as_mut() {
                    drag.start_index = index;
                }
            }
            return;
        }
        let index = nearest(start_point.unwrap_or_else(|| self.position()));
        let mut state = self.state.borrow_mut();
        state.target_index = index;
        if let Some(drag) = state.drag.as_mut() {
            drag.start_index = index;
        }
    }

    /// Ends a compositor settle in flight at its end now, and calls `on_settle`. It
    /// does nothing to a settle the animator draws.
    #[allow(
        dead_code,
        reason = "createSnapDrag's API for a Core component with its own model"
    )]
    pub fn finish_settle(&self) {
        let now = self.now();
        let finished = {
            let mut state = self.state.borrow_mut();
            match state.compositor.take() {
                None => false,
                Some(settle) => {
                    // The drawn value is already the end: only the parts stop.
                    settle.stop(now);
                    if state
                        .in_flight
                        .as_ref()
                        .is_some_and(|f| f.turn == state.turn)
                    {
                        state.in_flight = None;
                    }
                    if !state.destroyed {
                        let index = state.target_index;
                        state.reports.push(index);
                    }
                    true
                }
            }
        };
        if finished {
            self.report();
        }
    }

    /// A frame at `now` on the engine's clock: the engine moves on, a settle's
    /// `on_pass` and end are read, a compositor settle draws its parts, and the
    /// element is drawn.
    pub fn frame(&self, now: f64) {
        self.animator.advance(now);
        self.animator.frame(now);
        let waiting = self.state.borrow_mut().waiting_play.take();
        if let Some(play) = waiting {
            play(self);
        }
        let axis = self.axis();
        let (compositor, turn) = {
            let mut state = self.state.borrow_mut();
            (state.compositor.take(), state.turn)
        };
        if let Some(settle) = compositor {
            // Out of the state while it calls back: its `on_pass` may settle again.
            let ended = settle.frame(now);
            let mut state = self.state.borrow_mut();
            if state.turn != turn {
                // Another settle took over from the call: it draws the parts now.
                if state.compositor.is_none() {
                    drop(state);
                    settle.stop(now);
                }
            } else if ended {
                if state.in_flight.as_ref().is_some_and(|f| f.turn == turn) {
                    state.in_flight = None;
                }
                if !state.destroyed {
                    let index = state.target_index;
                    state.reports.push(index);
                }
            } else {
                state.compositor = Some(settle);
            }
        } else {
            // A settle the animator draws: its `on_pass`, then its end.
            let (pass, handle, flight_turn, destroyed) = {
                let state = self.state.borrow();
                match &state.in_flight {
                    Some(f) if !f.finished => (
                        f.options
                            .on_pass
                            .clone()
                            .filter(|_| !f.passed.get() && f.turn == state.turn)
                            .map(|p| (p, f.passed.clone())),
                        f.handle.clone(),
                        f.turn,
                        state.destroyed,
                    ),
                    _ => (None, None, 0, state.destroyed),
                }
            };
            if !destroyed && let Some((on_pass, passed)) = pass {
                let value = self.animator.get(axis_property(axis), Some(now));
                if (value - on_pass.offset) * on_pass.direction >= 0. {
                    passed.set(true);
                    (on_pass.call)();
                }
            }
            if let Some(handle) = handle
                && let Some(arrived) = handle.finished()
            {
                let mut state = self.state.borrow_mut();
                if state
                    .in_flight
                    .as_ref()
                    .is_some_and(|f| f.turn == flight_turn)
                {
                    state.in_flight = None;
                }
                if arrived && flight_turn == state.turn && !state.destroyed {
                    let index = state.target_index;
                    state.reports.push(index);
                }
            }
        }
        let destroyed = self.state.borrow().destroyed;
        if !destroyed {
            self.draw();
        }
        self.report();
    }

    /// Removes the drag and stops the element where it is.
    pub fn destroy(&self) {
        if self.state.borrow().destroyed {
            return;
        }
        // Held where it is: a running settle would otherwise leave the element where
        // that settle began.
        self.hold_compositor();
        self.animator.stop(None);
        self.draw();
        let mut state = self.state.borrow_mut();
        state.destroyed = true;
        state.drag = None;
        state.waiting_play = None;
        state.in_flight = None;
        state.reports.clear();
    }
}
