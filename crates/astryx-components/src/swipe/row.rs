//! A row's swipe, upstream `useSwipeAction.ts`: the stops, the actions and the finger's
//! model behind a row's swipe actions, over the shared snap-point drag (the pan, the
//! snap-point drag and the animator).
//!
//! What a drag does depends on what drives it:
//!
//! - A trackpad's horizontal swipe follows the fingers one for one from its first event
//!   past the pan's 8 px start; its release settles to the nearest stop by position and
//!   velocity (Core's one release policy).
//! - A finger's or a pen's drag begins at the row's 8 px claim; from then the row is at
//!   the finger's travel since touch-down. The full swipe confirms on UIKit's measure,
//!   from UIKit's 25 px dead zone: the row then runs ahead to the confirmation offset on
//!   UIKit's tracking spring and keeps that lead. The release goes by UIKit's rule
//!   ([`super::release::resolve_release`]) on UIKit's settle spring.
//!
//! Either way a drag stays on its side. Dragged past the confirm point, when the side's
//! outermost entry can fire, a full swipe confirms; on a side with two or more entries,
//! only a drag from the open row, or one that lands on its outermost entry, confirms
//! (DEC-9). Whenever the row slides out, the entry fires once the row's content has left
//! its box; then the row springs back, or holds out when the entry has `has_removal`.
//! Under Reduce Motion the drag follows one for one, and what a release, a confirm or a
//! close starts never travels: it cross-fades the row as one group.
//!
//! GPUI keeps no element from one frame to the next, so the row's swipe lives in keyed
//! state ([`use_swipe_row`]) and the row draws every part of its panels from the travel
//! on screen each frame, which is what upstream's compositor parts play.

use std::cell::RefCell;
use std::rc::{Rc, Weak};

use astryx_primitives::gestures::host::{GestureAttachment, GestureHost, gesture_host};
use astryx_primitives::gestures::{
    GestureState, Hysteresis, Pan, PanOptions, PointerKind, RecognizerOptions, ScrollType,
    TapGestureRecognizer, TapOptions, lock_selection,
};
use astryx_primitives::motion::resolve_snap_point::{SnapPointQuery, resolve_snap_point};
use astryx_primitives::motion::uikit::swipe_actions as SWIPE;
use astryx_primitives::motion::{
    MotionProperty, MotionSpring, SpringCurve, reduced_motion::reduced_motion_fade,
    resolve_motion::resolve_spring, solve_spring,
};
use astryx_primitives::{coarse_primary_pointer, prefers_reduced_motion};
use gpui::{App, Bounds, ElementId, FocusHandle, Pixels, Point, Window};

use super::release::{
    ReleaseQuery, Stop, SwipeLayout, SwipeRelease, SwipeSide, confirm_point, confirmed_offset,
    physical_sign, resolve_release, side_for, stop_at, stop_index, stops_of,
};
use super::{EntryFn, ItemSwipeBehavior, SwipeHost};
use crate::drag_to_snap_points::compositor_settle::OnPass;
use crate::drag_to_snap_points::snap_drag::{
    Companion, CustomProperty, SettleOptions, SnapDragLanding, SnapDragModel, SnapDragMove,
    SnapDragOptions, SnapDragRelease, SnapDragTarget, SnapElement,
};
use crate::drag_to_snap_points::{
    AttachedSnapDrag, DEFAULT_FLICK_THRESHOLD, SettleListener, attach_snap_drag, executor_clock,
};
use crate::gestures::pan::{PanAxis, PanInput};
use crate::reorderable::ItemKey;

/// The row's travel, physical px (upstream `--_item-swipe-travel`).
pub(crate) const SWIPE_TRAVEL_PROPERTY: &str = "--_item-swipe-travel";
/// How far a full swipe has confirmed, 0 to 1 (upstream `--_item-swipe-confirm`).
pub(crate) const SWIPE_CONFIRM_PROPERTY: &str = "--_item-swipe-confirm";

/// Upstream `SwipePhase`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SwipePhase {
    /// At rest, closed.
    Idle,
    /// A drag drives the row.
    Dragging,
    /// Settling where the release, a tap or a close sent it.
    Settling,
    /// Resting open with the side's panel tappable (`reveal` only).
    Resting,
    /// Out, after an entry with `has_removal` fired.
    Held,
}

/// Where a settle in progress lands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Landing {
    Idle,
    Resting,
    Held,
}

/// Upstream `SwipeState`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SwipeState {
    pub phase: SwipePhase,
    /// The side whose panel is uncovered, or will be.
    pub side: SwipeSide,
    /// A confirmed full swipe: letting go now fires the outermost entry.
    pub armed: bool,
    /// The entry that covers the side's panel, by its index on that side.
    pub cover: Option<usize>,
    /// Where the settle in progress lands (`Settling` only).
    pub landing: Option<Landing>,
}

const IDLE_STATE: SwipeState = SwipeState {
    phase: SwipePhase::Idle,
    side: SwipeSide::Leading,
    armed: false,
    cover: None,
    landing: None,
};

/// Upstream `isSwipeOpen`: resting open, or settling to rest open. UIKit counts a row
/// open from the release that opens it.
pub(crate) fn is_swipe_open(state: &SwipeState) -> bool {
    state.phase == SwipePhase::Resting
        || (state.phase == SwipePhase::Settling && state.landing == Some(Landing::Resting))
}

/// A side's geometry, written when a drag begins (upstream's pin, sign, open width and
/// slide custom properties on the side's window), with the fallbacks CSS reads.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SideGeometry {
    /// Where the entries sit against the track, physical px.
    pub pin: f64,
    /// 1 on the physical left, -1 on the right.
    pub sign: f64,
    pub open: f64,
    /// The outermost entry's slide on a confirmed full swipe, physical px.
    pub slide: f64,
}

impl Default for SideGeometry {
    fn default() -> Self {
        Self {
            pin: 0.,
            sign: 1.,
            open: 0.,
            slide: 0.,
        }
    }
}

impl SideGeometry {
    /// Where the entries sit against the track at a travel: pinned at the outer edge
    /// until the row has uncovered them all, at the open width; past it they ride.
    pub fn entries_x(&self, travel: f64) -> f64 {
        self.pin - self.sign * (self.sign * travel).min(self.open)
    }
}

/// The finger's travel from touch-down at which its swipe begins: the row's claim.
const FINGER_SWIPE_START: f64 = SWIPE::CLAIM;
/// How close to its target a lead in flight counts as grown, px.
const LEAD_REST: f64 = 0.25;
/// Below this the row is at rest, closed.
const AT_REST_PX: f64 = 0.5;

struct LeadMotion {
    curve: SpringCurve,
    start: f64,
}

struct Drag {
    layout: SwipeLayout,
    confirm_at: f64,
    can_confirm: bool,
    is_scroll: bool,
    started: bool,
    start_sign: f64,
    base_travel: f64,
    raw: f64,
    held: f64,
    lead: f64,
    lead_motion: Option<LeadMotion>,
    armed: bool,
    was_confirmed: bool,
    dip_until: Option<f64>,
}

fn lead_now(drag: &Drag, now: f64) -> f64 {
    match &drag.lead_motion {
        None => drag.lead,
        Some(m) => {
            let t = (now - m.start) / 1000.;
            if t < m.curve.settle_time(LEAD_REST) {
                m.curve.value(t)
            } else {
                drag.lead
            }
        }
    }
}

fn lead_velocity(drag: &Drag, now: f64) -> f64 {
    drag.lead_motion
        .as_ref()
        .map_or(0., |m| m.curve.velocity((now - m.start) / 1000.))
}

/// What started a settle: a touch drag ending, or anything else.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SettleCause {
    Release,
    Other,
}

#[derive(Clone, Default)]
struct Settle {
    cause: Option<SettleCause>,
    velocity: Option<f64>,
    /// `None`: UIKit's settle spring; `Some(None)`: the shared drag's own.
    spring: Option<Option<MotionSpring>>,
    on_pass: Option<OnPass>,
    next_frame: bool,
    cover: Option<usize>,
}

/// A fire the row owes, run with the window after the frame that saw it.
type Fire = (EntryFn, Option<ItemKey>);

pub(crate) struct RowState {
    pub state: SwipeState,
    layout: Option<SwipeLayout>,
    drag: Option<Drag>,
    /// The side whose outermost entry the last press landed on, with the row open on it.
    landed_on: Option<SwipeSide>,
    pub host: Rc<SwipeHost>,
    ids_key: String,
    /// Each side's entries' natural widths, in order, from the last layout.
    pub widths: [Vec<f64>; 2],
    pub geometry: [SideGeometry; 2],
    /// The row's resting box, window px.
    pub bounds: Option<Bounds<Pixels>>,
    /// Where the row's box was when it opened, to tell a scroll that moves it.
    rest_origin: Option<Point<Pixels>>,
    pub inline_sign: f64,
    reduced: bool,
    coarse: bool,
    fires: Vec<Fire>,
    finish: bool,
    refocus: bool,
    /// Each side's entries' focus handles.
    pub entry_focus: [Vec<FocusHandle>; 2],
}

struct RowInner {
    snap: RefCell<Option<Rc<AttachedSnapDrag>>>,
    dismiss: RefCell<Option<(GestureAttachment, TapGestureRecognizer)>>,
    st: RefCell<RowState>,
}

impl Drop for RowInner {
    fn drop(&mut self) {
        // A row that loses its swipe (or leaves) drops its gesture.
        if let Some(snap) = self.snap.borrow_mut().take() {
            snap.destroy();
        }
        if let Some((attachment, _)) = self.dismiss.borrow_mut().take() {
            attachment.detach();
        }
    }
}

/// A row's swipe. Cheap to clone; every clone is the same row's.
#[derive(Clone)]
pub(crate) struct SwipeRow(Rc<RowInner>);

/// Upstream `useSwipeAction`: the swipe of the row `id` names, kept across frames, with
/// the latest actions. A new host with the same ids keeps the row as it is; one with
/// other ids closes an open row (FR11).
pub(crate) fn use_swipe_row(
    id: &ElementId,
    host: Rc<SwipeHost>,
    window: &mut Window,
    cx: &mut App,
) -> SwipeRow {
    let gestures = gesture_host(window, cx);
    let state = window.use_keyed_state(ElementId::Name(format!("{id:?}-swipe").into()), cx, {
        let host = host.clone();
        move |_, cx| SwipeRow::new(&gestures, host, cx)
    });
    let row = state.read(cx).clone();
    let ids = host.ids_key();
    let changed = {
        let mut st = row.0.st.borrow_mut();
        st.inline_sign = if astryx_i18n::use_direction(cx).is_rtl() {
            -1.
        } else {
            1.
        };
        st.reduced = prefers_reduced_motion(cx);
        st.coarse = coarse_primary_pointer(window, cx);
        st.host = host;
        let changed = st.ids_key != ids;
        st.ids_key = ids;
        changed
    };
    if changed {
        // Other actions: an open row closes.
        row.close(false);
    }
    row
}

impl SwipeRow {
    fn new(gestures: &GestureHost, host: Rc<SwipeHost>, cx: &App) -> Self {
        let ids_key = host.ids_key();
        let row = SwipeRow(Rc::new(RowInner {
            snap: RefCell::new(None),
            dismiss: RefCell::new(None),
            st: RefCell::new(RowState {
                state: IDLE_STATE,
                layout: None,
                drag: None,
                landed_on: None,
                host,
                ids_key,
                widths: [Vec::new(), Vec::new()],
                geometry: [SideGeometry::default(); 2],
                bounds: None,
                rest_origin: None,
                inline_sign: 1.,
                reduced: false,
                coarse: false,
                fires: Vec::new(),
                finish: false,
                refocus: false,
                entry_focus: [Vec::new(), Vec::new()],
            }),
        }));
        let weak = Rc::downgrade(&row.0);
        let with = move |f: &dyn Fn(&SwipeRow)| {
            if let Some(inner) = weak.upgrade() {
                f(&SwipeRow(inner));
            }
        };
        let w = Rc::downgrade(&row.0);
        let up = move || w.upgrade().map(SwipeRow);
        let model = {
            let (a, b, c, d) = (up.clone(), up.clone(), up.clone(), up.clone());
            SnapDragModel {
                begin: Some(Rc::new(move |m: &SnapDragMove| {
                    a().is_some_and(|r| r.begin(m))
                })),
                move_to: Some(Rc::new(move |m: &SnapDragMove| {
                    b().and_then(|r| r.track(m))
                })),
                release: Some(Rc::new(move |e: &SnapDragRelease| {
                    c().and_then(|r| r.release(e))
                })),
                cancel: Some(Rc::new(move |_: &SnapDragMove| {
                    d().and_then(|r| r.cancel())
                })),
            }
        };
        let confirm = {
            let up = up.clone();
            move |x: f64| up().map_or(0., |r| r.confirm_of(x))
        };
        let pan = {
            let (receive, begin) = (up.clone(), up.clone());
            let fingers = gestures.clone();
            let mut options = PanOptions::named("Item.swipe");
            options.base = RecognizerOptions {
                name: Some("Item.swipe".into()),
                // A finger, a pen and a trackpad's two-finger scroll swipe a row; a mouse
                // drag and a mouse wheel's notches never do.
                allowed_pointer_types: Some(vec![PointerKind::Touch, PointerKind::Pen]),
                allowed_scroll_types: Some(vec![ScrollType::Continuous]),
                // The drag moves the row: text selection stays off for the press.
                selection_lock: Some(Some(lock_selection())),
                // Offered each touch as it lands: notes a press on the outermost entry
                // of a row open on that side, so that press can confirm a full swipe.
                should_receive_touch: Some(Rc::new(move |_, touch| {
                    if let Some(row) = receive() {
                        row.note_press(
                            touch.scroll_type().is_none(),
                            touch.position().x,
                            touch.position().y,
                        );
                    }
                    true
                })),
                should_begin: Some(Rc::new(move |r| {
                    // A finger that is not the only one down is part of a pinch: the
                    // page zooms.
                    if fingers.fingers_down() > 1 {
                        return false;
                    }
                    let dx = r
                        .downcast::<Pan>()
                        .map_or(0., |pan| pan.total_translation().x);
                    begin().is_some_and(|row| row.should_begin(dx))
                })),
                ..Default::default()
            };
            options.maximum_number_of_touches = 1;
            options.hysteresis = Hysteresis::Fixed(SWIPE::CLAIM);
            options
        };
        let listener: SettleListener = {
            let with = with.clone();
            Rc::new(move |index, _, _| with(&|row: &SwipeRow| row.landed(index)))
        };
        let attached = attach_snap_drag(
            &SnapElement::new(),
            gestures,
            SnapDragOptions {
                axis: PanAxis::X,
                snap_points: vec![0.],
                flick_threshold: None,
                on_settle: Rc::new(|_| {}),
                pan: Some(pan),
                // The travel, and the full swipe's confirm drawn from it in the same
                // frames.
                custom_property: Some(CustomProperty {
                    name: SWIPE_TRAVEL_PROPERTY.into(),
                    companion: Some(Companion {
                        name: SWIPE_CONFIRM_PROPERTY.into(),
                        map: Rc::new(confirm),
                    }),
                }),
                // The settles play every part of the row from one curve, read each
                // frame (upstream's compositor parts).
                compositor_parts: Some(Rc::new(Vec::new)),
                model: Some(model),
                clock: executor_clock(cx),
            },
            Rc::new(RefCell::new(listener)),
        );
        *row.0.snap.borrow_mut() = Some(Rc::new(attached));
        // A touch tap on an open row's own content closes it and activates nothing, as
        // UIKit's dismissal does; the host keeps the click it would have made.
        let tap = {
            let receive = up.clone();
            TapGestureRecognizer::new(TapOptions {
                base: RecognizerOptions {
                    name: Some("item-swipe-dismiss".into()),
                    allowed_pointer_types: Some(vec![PointerKind::Touch]),
                    should_receive_touch: Some(Rc::new(move |_, touch| {
                        receive().is_some_and(|row| {
                            row.is_open()
                                && !row.on_open_panel(touch.position().x, touch.position().y)
                        })
                    })),
                    ..Default::default()
                },
                ..Default::default()
            })
        };
        {
            let up = up.clone();
            tap.add_listener(move |r| {
                if r.any().state() == GestureState::Ended
                    && let Some(row) = up()
                {
                    row.close(true);
                }
            });
        }
        let attachment = gestures.attach(&[tap.any()]);
        *row.0.dismiss.borrow_mut() = Some((attachment, tap));
        row
    }

    pub(crate) fn snap(&self) -> Option<Rc<AttachedSnapDrag>> {
        self.0.snap.borrow().clone()
    }

    pub(crate) fn dismiss_attachment(&self) -> Option<GestureAttachment> {
        self.0.dismiss.borrow().as_ref().map(|(a, _)| a.clone())
    }

    pub(crate) fn st(&self) -> std::cell::Ref<'_, RowState> {
        self.0.st.borrow()
    }

    pub(crate) fn st_mut(&self) -> std::cell::RefMut<'_, RowState> {
        self.0.st.borrow_mut()
    }

    pub(crate) fn state(&self) -> SwipeState {
        self.0.st.borrow().state
    }

    pub(crate) fn is_open(&self) -> bool {
        is_swipe_open(&self.state())
    }

    fn set_state(&self, next: SwipeState) {
        self.0.st.borrow_mut().state = next;
    }

    /// The row's travel on screen now, physical px.
    pub(crate) fn presented_x(&self) -> f64 {
        self.snap().map_or(0., |s| s.snap.position())
    }

    /// The row's opacity on screen now: 1, or a Reduce Motion cross-fade's.
    pub(crate) fn opacity(&self) -> f64 {
        self.snap().map_or(1., |s| {
            s.snap
                .animator
                .get(MotionProperty::Opacity, None)
                .clamp(0., 1.)
        })
    }

    fn travel_now(&self, layout: &SwipeLayout) -> f64 {
        (self.presented_x() * physical_sign(layout.side, layout.inline_sign)).max(0.)
    }

    fn sides(&self, side: SwipeSide) -> usize {
        self.0.st.borrow().host.entries(side).len()
    }

    /// Whether the side's outermost entry can fire: a disabled one never confirms.
    fn can_confirm(&self, side: SwipeSide) -> bool {
        self.0
            .st
            .borrow()
            .host
            .entries(side)
            .last()
            .is_some_and(|e| !e.disabled)
    }

    /// Upstream `confirmOf`: the full swipe's confirm for a travel, 0 short of the confirm
    /// point, 1 at the confirmation offset, in step with the row between them; 0 while
    /// the side's outermost entry does not cover the panel.
    pub(crate) fn confirm_of(&self, x: f64) -> f64 {
        let (layout, state) = {
            let st = self.0.st.borrow();
            (st.layout, st.state)
        };
        let Some(layout) = layout else {
            return 0.;
        };
        let count = self.sides(layout.side);
        if state.side != layout.side
            || count == 0
            || state.cover != Some(count - 1)
            || !self.can_confirm(layout.side)
        {
            return 0.;
        }
        let travel = x * physical_sign(layout.side, layout.inline_sign);
        let from = confirm_point(layout.open_width, layout.row_width);
        let to = confirmed_offset(layout.row_width);
        if to <= from {
            return if travel >= to { 1. } else { 0. };
        }
        ((travel - from) / (to - from)).clamp(0., 1.)
    }

    /// Fires the side's outermost entry (after the frame, with the window); returns
    /// whether the row holds out afterwards (its `has_removal`).
    fn fire_outermost(&self, side: SwipeSide) -> bool {
        let mut st = self.0.st.borrow_mut();
        let Some(entry) = st.host.entries(side).last().cloned() else {
            return false;
        };
        if entry.disabled {
            return false;
        }
        let key = st.host.item_key.clone();
        st.fires.push((entry.on_activate.clone(), key));
        entry.has_removal
    }

    /// The shared drag settled on a stop: the row lands there.
    fn landed(&self, index: usize) {
        let (layout, state) = {
            let st = self.0.st.borrow();
            (st.layout, st.state)
        };
        let Some(layout) = layout else {
            return;
        };
        match stop_at(&layout, index) {
            Stop::Out => {
                // Out after a fire: holding, unless the fire already started the
                // spring back.
                if state.phase == SwipePhase::Settling {
                    self.set_state(SwipeState {
                        phase: SwipePhase::Held,
                        side: layout.side,
                        armed: false,
                        cover: state.cover,
                        landing: None,
                    });
                }
            }
            Stop::Rest => self.set_state(IDLE_STATE),
            Stop::Open => self.set_state(SwipeState {
                phase: SwipePhase::Resting,
                side: layout.side,
                armed: false,
                cover: None,
                landing: None,
            }),
        }
    }

    /// Settles the row on a stop and lands in `phase`.
    fn settle(&self, layout: SwipeLayout, stop: Stop, phase: Landing, options: Settle) {
        let Some(snap) = self.snap() else {
            return;
        };
        {
            let mut st = self.0.st.borrow_mut();
            st.layout = Some(layout);
            st.state = SwipeState {
                phase: SwipePhase::Settling,
                side: layout.side,
                armed: false,
                cover: options.cover,
                landing: Some(phase),
            };
        }
        let spring = match options.spring {
            None => Some(MotionSpring::from(SWIPE::SETTLE)),
            Some(spring) => spring,
        };
        snap.snap.set_snap_points(&stops_of(&layout));
        let index = stop_index(&layout, stop);
        snap.snap.settle_to(
            index,
            SettleOptions {
                velocity: options.velocity,
                spring,
                touch_release: options.cause == Some(SettleCause::Release),
                on_pass: options.on_pass,
                next_frame: options.next_frame,
            },
        );
    }

    /// Slides the row out toward the edge it travelled toward, and fires once its
    /// content has left its box. A settle or a drag that replaces the slide-out first
    /// fires nothing.
    fn slide_out_then_fire(
        &self,
        layout: SwipeLayout,
        fire: Rc<dyn Fn() -> bool>,
        options: Settle,
    ) {
        let sign = physical_sign(layout.side, layout.inline_sign);
        let weak: Weak<RowInner> = Rc::downgrade(&self.0);
        self.settle(
            layout,
            Stop::Out,
            Landing::Held,
            Settle {
                on_pass: Some(OnPass {
                    offset: sign * layout.row_width,
                    direction: sign,
                    call: Rc::new(move || {
                        // True when the row holds out: past its box it is out of sight,
                        // so its settle ends there now.
                        if fire()
                            && let Some(inner) = weak.upgrade()
                        {
                            inner.st.borrow_mut().finish = true;
                        }
                    }),
                }),
                ..options
            },
        );
    }

    /// A confirmed full swipe: out with the outermost entry covering the row's place.
    fn slide_out_and_fire(
        &self,
        layout: SwipeLayout,
        cause: SettleCause,
        velocity: Option<f64>,
        spring: Option<Option<MotionSpring>>,
    ) {
        let cover = self.sides(layout.side).checked_sub(1);
        let weak: Weak<RowInner> = Rc::downgrade(&self.0);
        self.slide_out_then_fire(
            layout,
            Rc::new(move || {
                let Some(inner) = weak.upgrade() else {
                    return false;
                };
                let row = SwipeRow(inner);
                if row.fire_outermost(layout.side) {
                    return true;
                }
                row.settle(
                    layout,
                    Stop::Rest,
                    Landing::Idle,
                    Settle {
                        cause: Some(cause),
                        velocity,
                        spring,
                        cover,
                        ..Default::default()
                    },
                );
                false
            }),
            Settle {
                cause: Some(cause),
                velocity,
                spring,
                cover,
                ..Default::default()
            },
        );
    }

    /// Upstream `close`: closes an open row. `next_frame` when asked for in an input
    /// event's own task (a key, a press, a tap). Focus inside the row moves to its
    /// content.
    pub(crate) fn close(&self, next_frame: bool) {
        let layout = self.0.st.borrow().layout;
        let Some(layout) = layout else {
            return;
        };
        if !self.is_open() {
            return;
        }
        self.0.st.borrow_mut().refocus = true;
        self.settle(
            layout,
            Stop::Rest,
            Landing::Idle,
            Settle {
                next_frame,
                ..Default::default()
            },
        );
    }

    /// Upstream `activateEntry`: a resting panel's entry was tapped. Without
    /// `has_removal` it fires at once and the row closes; with it the row slides out,
    /// the entry fires once the row's content has left its box, and the row holds out.
    /// The tapped entry covers the panel meanwhile. Returns whether `fire` ran now.
    pub(crate) fn activate_entry(&self, entry: EntryFn, holds_out: bool, cover: usize) -> bool {
        let layout = self.0.st.borrow().layout;
        let Some(layout) = layout else {
            return false;
        };
        if !self.is_open() {
            return false;
        }
        self.0.st.borrow_mut().refocus = true;
        if holds_out {
            let weak: Weak<RowInner> = Rc::downgrade(&self.0);
            self.slide_out_then_fire(
                layout,
                Rc::new(move || {
                    if let Some(inner) = weak.upgrade() {
                        let mut st = inner.st.borrow_mut();
                        let key = st.host.item_key.clone();
                        st.fires.push((entry.clone(), key));
                    }
                    true
                }),
                Settle {
                    cause: Some(SettleCause::Other),
                    next_frame: true,
                    cover: Some(cover),
                    ..Default::default()
                },
            );
            return false;
        }
        self.settle(
            layout,
            Stop::Rest,
            Landing::Idle,
            Settle {
                next_frame: true,
                cover: Some(cover),
                ..Default::default()
            },
        );
        true
    }

    // --- The drag's model ---------------------------------------------------------

    /// A touch landed: note a press on the outermost entry of a row open on that side.
    fn note_press(&self, is_touch: bool, x: f64, y: f64) {
        let state = self.state();
        let side = (is_touch && is_swipe_open(&state) && self.on_outer_entry(state.side, x, y))
            .then_some(state.side);
        self.0.st.borrow_mut().landed_on = side;
    }

    /// Whether a point is on the side's outermost entry while the row rests open:
    /// pinned at the row's outer edge.
    fn on_outer_entry(&self, side: SwipeSide, x: f64, y: f64) -> bool {
        let st = self.0.st.borrow();
        let (Some(bounds), Some(outer)) = (st.bounds, st.widths[side.index()].last().copied())
        else {
            return false;
        };
        let (left, right) = (f64::from(bounds.left()), f64::from(bounds.right()));
        let (top, bottom) = (f64::from(bounds.top()), f64::from(bounds.bottom()));
        if y < top || y > bottom {
            return false;
        }
        if physical_sign(side, st.inline_sign) > 0. {
            x >= left && x <= left + outer
        } else {
            x >= right - outer && x <= right
        }
    }

    /// Whether a point is on the open side's uncovered panel rather than the row's
    /// content, which has moved by the travel.
    pub(crate) fn on_open_panel(&self, x: f64, y: f64) -> bool {
        let travel = self.presented_x();
        let st = self.0.st.borrow();
        let Some(bounds) = st.bounds else {
            return false;
        };
        let (top, bottom) = (f64::from(bounds.top()), f64::from(bounds.bottom()));
        if y < top || y > bottom {
            return false;
        }
        let (left, right) = (f64::from(bounds.left()), f64::from(bounds.right()));
        let on_content = x >= left + travel && x <= right + travel;
        let in_box = x >= left && x <= right;
        in_box && !on_content
    }

    fn should_begin(&self, dx: f64) -> bool {
        let (phase, inline_sign) = {
            let st = self.0.st.borrow();
            (st.state.phase, st.inline_sign)
        };
        if phase == SwipePhase::Held {
            return false;
        }
        self.presented_x().abs() > AT_REST_PX || self.sides(side_for(dx, inline_sign)) > 0
    }

    fn begin(&self, m: &SnapDragMove) -> bool {
        let (inline_sign, bounds) = {
            let st = self.0.st.borrow();
            (st.inline_sign, st.bounds)
        };
        let Some(bounds) = bounds else {
            return false;
        };
        // An open or moving row continues on its side; a closed one takes the side the
        // drag moves toward.
        let side = if m.start.abs() > AT_REST_PX {
            if m.start * inline_sign > 0. {
                SwipeSide::Leading
            } else {
                SwipeSide::Trailing
            }
        } else {
            side_for(m.total, inline_sign)
        };
        let (open_width, outer_width) = {
            let st = self.0.st.borrow();
            let widths = &st.widths[side.index()];
            (
                widths.iter().sum::<f64>(),
                widths.last().copied().unwrap_or(0.),
            )
        };
        let layout = SwipeLayout {
            side,
            inline_sign,
            open_width,
            outer_width,
            row_width: f64::from(bounds.size.width),
        };
        let sign = physical_sign(side, inline_sign);
        let count = self.sides(side);
        let can_confirm = self.can_confirm(side);
        {
            let mut st = self.0.st.borrow_mut();
            // The entries keep their place against the track for the geometry written
            // last; a first drag, or new entries, writes it now.
            st.geometry[side.index()] = SideGeometry {
                pin: sign * open_width,
                sign,
                open: open_width,
                slide: sign * (open_width - outer_width),
            };
            st.layout = Some(layout);
        }
        if let Some(snap) = self.snap() {
            snap.snap.set_snap_points(&stops_of(&layout));
        }
        // With two or more entries, a side confirms a full swipe only from the row open
        // on that side, a press on its outermost entry while it is open, or a panel its
        // outermost entry covers (DEC-9).
        let (current, landed_on) = {
            let st = self.0.st.borrow();
            (st.state, st.landed_on)
        };
        let deliberate = (m.input != PanInput::Scroll && landed_on == Some(side))
            || (current.side == side
                && (is_swipe_open(&current) || (count > 0 && current.cover == Some(count - 1))));
        self.0.st.borrow_mut().drag = Some(Drag {
            layout,
            confirm_at: confirm_point(open_width, layout.row_width),
            can_confirm: can_confirm && (count < 2 || deliberate),
            is_scroll: m.input == PanInput::Scroll,
            started: false,
            start_sign: 1.,
            base_travel: 0.,
            raw: 0.,
            held: 0.,
            lead: 0.,
            lead_motion: None,
            armed: false,
            was_confirmed: false,
            dip_until: None,
        });
        true
    }

    fn now(&self) -> f64 {
        self.snap().map_or(0., |s| s.snap.animator.now())
    }

    fn track(&self, m: &SnapDragMove) -> Option<SnapDragTarget> {
        let layout = self.0.st.borrow().drag.as_ref()?.layout;
        let sign = physical_sign(layout.side, layout.inline_sign);
        let count = self.sides(layout.side);
        let outer = count.checked_sub(1);
        let started = self.0.st.borrow().drag.as_ref()?.started;
        if !started {
            let is_scroll = self.0.st.borrow().drag.as_ref()?.is_scroll;
            // A finger's swipe begins at its claim, counted from where it landed; before
            // that the row stays where it was caught.
            if !is_scroll && m.total.abs() < FINGER_SWIPE_START {
                return None;
            }
            let base = self.travel_now(&layout);
            let armed = {
                let mut st = self.0.st.borrow_mut();
                let drag = st.drag.as_mut()?;
                drag.started = true;
                drag.start_sign = if m.total * sign == 0. {
                    1.
                } else {
                    (m.total * sign).signum()
                };
                drag.base_travel = base;
                drag.armed
            };
            self.set_state(SwipeState {
                phase: SwipePhase::Dragging,
                side: layout.side,
                armed,
                cover: if armed { outer } else { None },
                landing: None,
            });
        }
        let (reduced, coarse) = {
            let st = self.0.st.borrow();
            (st.reduced, st.coarse)
        };
        let fade = reduced_motion_fade(reduced, coarse, false);
        let now = self.now();
        let tempo = self.snap().map_or(1., |s| s.snap.animator.tempo());
        let (offset, flipped, armed, is_scroll, dipping) = {
            let mut st = self.0.st.borrow_mut();
            let drag = st.drag.as_mut()?;
            // On UIKit's measure a finger's swipe counts from UIKit's dead zone, and a
            // trackpad's from the pan's start.
            let swipe = if drag.is_scroll {
                m.translation * sign
            } else {
                m.total * sign - drag.start_sign * SWIPE::HYSTERESIS
            };
            // Within one gesture the row stays on its side: never past rest.
            let raw = (drag.base_travel + swipe).max(0.);
            drag.raw = raw;
            // A finger's row is at the finger's travel since touch-down.
            let held = if drag.is_scroll {
                raw
            } else {
                (drag.base_travel + m.total * sign).max(0.)
            };
            drag.held = held;
            let armed = drag.can_confirm && raw >= drag.confirm_at;
            let flipped = armed != drag.armed;
            if flipped {
                // Confirmed, a finger's row gains a lead that grows to the confirmation
                // offset on UIKit's tracking spring; backed out, it shrinks on it. Under
                // Reduce Motion the lead changes at once, inside the cross-fade.
                let from = lead_now(drag, now);
                let velocity = lead_velocity(drag, now);
                drag.lead = if armed {
                    (confirmed_offset(layout.row_width) - held).max(0.)
                } else {
                    0.
                };
                drag.lead_motion = if drag.is_scroll || fade.is_some() {
                    None
                } else {
                    let spring =
                        resolve_spring(MotionSpring::from(SWIPE::TRACKING), tempo).to_spring();
                    Some(LeadMotion {
                        curve: solve_spring(spring, from, drag.lead, velocity),
                        start: now,
                    })
                };
                drag.armed = armed;
                drag.was_confirmed |= armed;
            }
            // A confirmed trackpad's row runs ahead to the confirmation offset and waits
            // there for its fingers.
            let travel = if drag.is_scroll {
                if armed {
                    raw.max(confirmed_offset(layout.row_width))
                } else {
                    raw
                }
            } else {
                held + lead_now(drag, now)
            };
            let dipping = drag.dip_until.is_some_and(|until| now < until);
            if fade.is_some() && flipped {
                drag.dip_until = fade.map(|f| now + f.duration_ms);
            } else if !dipping {
                drag.dip_until = None;
            }
            (sign * travel, flipped, armed, drag.is_scroll, dipping)
        };
        if flipped {
            self.set_state(SwipeState {
                phase: SwipePhase::Dragging,
                side: layout.side,
                armed,
                cover: if armed { outer } else { None },
                landing: None,
            });
        }
        let tracking = Some(MotionSpring::from(SWIPE::TRACKING));
        if fade.is_none() {
            // A finger's row is drawn where it is, with no spring; a trackpad's confirm
            // or back-out springs the row to its new place.
            return Some(SnapDragTarget {
                offset,
                spring: if is_scroll && flipped { tracking } else { None },
            });
        }
        if flipped {
            // Confirming and backing out are changes of state, not the drag: under
            // Reduce Motion they cross-fade, and the moves leave that fade alone.
            return Some(SnapDragTarget {
                offset,
                spring: tracking,
            });
        }
        if dipping {
            return None;
        }
        Some(SnapDragTarget {
            offset,
            spring: None,
        })
    }

    /// Draws a finger's row while its lead grows or shrinks, also on frames with no
    /// move; returns whether it needs another frame.
    pub(crate) fn draw_lead_frame(&self) -> bool {
        let now = self.now();
        let x = {
            let mut st = self.0.st.borrow_mut();
            let Some(drag) = st.drag.as_mut() else {
                return false;
            };
            let Some(motion) = &drag.lead_motion else {
                return false;
            };
            let more = (now - motion.start) / 1000. < motion.curve.settle_time(LEAD_REST);
            let sign = physical_sign(drag.layout.side, drag.layout.inline_sign);
            let x = sign * (drag.held + lead_now(drag, now));
            if !more {
                drag.lead_motion = None;
            }
            (x, more)
        };
        if let Some(snap) = self.snap() {
            snap.snap
                .animator
                .set(astryx_primitives::motion::MotionValues::new().x(x.0));
        }
        x.1
    }

    fn release(&self, end: &SnapDragRelease) -> Option<SnapDragLanding> {
        let drag = self.0.st.borrow_mut().drag.take()?;
        // A drag that never became a swipe changed nothing.
        if !drag.started {
            return None;
        }
        let layout = drag.layout;
        let sign = physical_sign(layout.side, layout.inline_sign);
        let velocity = end.velocity * sign;
        let cause = if end.drag.input == PanInput::Touch {
            SettleCause::Release
        } else {
            SettleCause::Other
        };
        let behavior = self.0.st.borrow().host.behavior;
        let landing = if drag.is_scroll {
            // A trackpad's release settles to the nearest stop by position and
            // velocity; the full swipe is reached by position only.
            if drag.armed {
                SwipeRelease::Perform
            } else {
                let points = if behavior == ItemSwipeBehavior::Commit {
                    vec![0.]
                } else {
                    vec![0., layout.open_width]
                };
                let index = resolve_snap_point(&SnapPointQuery {
                    points,
                    position: drag.raw,
                    velocity,
                    flick_threshold: DEFAULT_FLICK_THRESHOLD,
                    start: Some(drag.base_travel),
                    ..Default::default()
                })
                .map_or(0, |r| r.index);
                if index == 1 {
                    SwipeRelease::Open
                } else {
                    SwipeRelease::Closed
                }
            }
        } else {
            resolve_release(ReleaseQuery {
                travel: drag.held,
                velocity,
                open_width: layout.open_width,
                is_confirmed: drag.armed,
                was_confirmed: drag.was_confirmed,
                behavior,
            })
        };
        // A finger's settle starts from the row's own velocity on UIKit's spring; a
        // trackpad's on the shared settle spring.
        let spring = if drag.is_scroll { Some(None) } else { None };
        let velocity = Some(end.velocity);
        match landing {
            SwipeRelease::Perform => self.slide_out_and_fire(layout, cause, velocity, spring),
            SwipeRelease::Open => self.settle(
                layout,
                Stop::Open,
                Landing::Resting,
                Settle {
                    cause: Some(cause),
                    velocity,
                    spring,
                    ..Default::default()
                },
            ),
            SwipeRelease::Closed => self.settle(
                layout,
                Stop::Rest,
                Landing::Idle,
                Settle {
                    cause: Some(cause),
                    velocity,
                    spring,
                    ..Default::default()
                },
            ),
        }
        // The settle above is the row's; the shared drag starts none of its own.
        None
    }

    fn cancel(&self) -> Option<SnapDragLanding> {
        let drag = self.0.st.borrow_mut().drag.take();
        // The platform took a touch the row was following: it closes.
        if let Some(drag) = drag
            && drag.started
        {
            self.settle(
                drag.layout,
                Stop::Rest,
                Landing::Idle,
                Settle {
                    cause: Some(SettleCause::Release),
                    spring: if drag.is_scroll { Some(None) } else { None },
                    ..Default::default()
                },
            );
        }
        None
    }

    // --- Each frame ---------------------------------------------------------------

    /// Moves the row on to this frame: the drag's frame and its landings, a lead in
    /// flight, a slide-out that ended past the row's edge, and the fires it owes.
    pub(crate) fn advance(&self, window: &mut Window, cx: &mut App) {
        let Some(snap) = self.snap() else {
            return;
        };
        snap.advance(window, cx);
        if self.draw_lead_frame() {
            window.request_animation_frame();
        }
        let finish = std::mem::take(&mut self.0.st.borrow_mut().finish);
        if finish {
            snap.snap.finish_settle();
            window.request_animation_frame();
        }
        let fires: Vec<Fire> = std::mem::take(&mut self.0.st.borrow_mut().fires);
        if !fires.is_empty() {
            window.defer(cx, move |window, cx| {
                for (fire, key) in fires {
                    fire(key.as_ref(), window, cx);
                }
            });
        }
    }

    /// Takes the request to move focus out of a closing panel.
    pub(crate) fn take_refocus(&self) -> bool {
        std::mem::take(&mut self.0.st.borrow_mut().refocus)
    }

    /// Records the row's resting box from its layout, and closes an open row that a
    /// scroll moved while focus is outside its open panel.
    pub(crate) fn record_bounds(&self, bounds: Bounds<Pixels>, window: &Window) {
        let open = self.is_open();
        let moved = {
            let mut st = self.0.st.borrow_mut();
            st.bounds = Some(bounds);
            if !open {
                st.rest_origin = Some(bounds.origin);
                false
            } else {
                let moved = st.rest_origin.is_some_and(|o| o != bounds.origin);
                let side = st.state.side.index();
                let focus_in_panel = st.entry_focus[side].iter().any(|f| f.is_focused(window));
                if moved && focus_in_panel {
                    // The scroll may be the platform bringing a focused entry into view.
                    st.rest_origin = Some(bounds.origin);
                }
                moved && !focus_in_panel
            }
        };
        if moved {
            self.close(false);
        }
    }

    #[cfg(test)]
    pub(crate) fn armed(&self) -> bool {
        self.0.st.borrow().drag.as_ref().is_some_and(|d| d.armed)
    }
}
