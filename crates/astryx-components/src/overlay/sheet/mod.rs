//! BottomSheet and BottomSheetSwitcher, upstream `BottomSheet/`: a touch sheet that
//! rises from the bottom edge.
//!
//! - **The panel** (`BottomSheetPanel.tsx`, FR1-FR4, FR25, FR26): up to 640 px wide,
//!   centred, rounded top corners, a hairline on the three edges that face the scrim
//!   and a high shadow, in the surface's text colour. A grab bar floats over the top
//!   of the scrolling body, which keeps a band clear under it at rest, and the
//!   content box pads by the sheet's inset (`spacing-4` by default) and publishes it
//!   to a Layout inside (`bottom-sheet`). The surface reaches below the window (the
//!   reserve: 40% of the window on a touch device, 48 px otherwise), so a pull past
//!   the top never shows the page.
//! - **Heights and detents** (`snapOffsets.ts`): `Hug` fits its content up to 92% of
//!   the window, `Capped` is 62%, `Tall` 92%, or any length; `snap_points` are extra
//!   resting heights, the sheet's own the tallest. A detent of a quarter of the sheet
//!   or less is a peek: it slides below the window instead of reflowing, and thins
//!   the scrim. A resize or new snap points re-anchor the sheet to the same detent,
//!   a settle in flight included.
//! - **Gestures** (`useSheetGestures.ts`, FR18, FR19, FR27): the grab bar drags the
//!   sheet under every pointer, and the body does once its content cannot scroll that
//!   way (a pull down at the top, a pull up at the end), including a list that runs
//!   out of content mid-pull; a release settles on the springs of UIKit's sheet from
//!   the release velocity, or closes an `Info` sheet, which leaves on its close
//!   spring from the release speed with its scrim fading alongside.
//! - **The stack** (`bottomSheetStack.ts`, FR5-FR13): see [`stack`].
//! - **Reduce Motion** (FR15): no transform travels; a coarse pointer or a touch
//!   drag's dismissal cross-fades within 200 ms, and anything else changes at once.

pub(crate) mod gestures;
pub(crate) mod snap_offsets;
pub(crate) mod stack;
mod switcher;

#[cfg(test)]
mod tests;

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use astryx_primitives::gestures::host::{GestureHost, after_gesture, gesture_host};
use astryx_primitives::gestures::native_scroll::NativeScroller;
use astryx_primitives::gestures::{Hysteresis, PanOptions, PointerKind, VelocityTracker};
use astryx_primitives::layer::MODAL_DEPTH;
use astryx_primitives::motion::uikit::sheet::release::FAST_SPEED;
use astryx_primitives::motion::uikit::springs::{self, role};
use astryx_primitives::motion::{
    AnimateOptions, EngineOptions, MotionEngine, MotionHandle, MotionProperty, MotionSpring,
    MotionValues, SpringMove, SpringTransition, resolve_settle_ms,
};
use astryx_primitives::scroll_area::{
    ScrollAxis, ScrollKeyboardAccess, ScrollOverscroll, ScrollRole, ScrollStickyContainment,
    UseScrollableAreaOptions, use_scrollable_area,
};
use astryx_primitives::{
    EscapeBehavior, coarse_primary_pointer, layer, prefers_reduced_motion, provide, use_context,
};
use astryx_theme::{ThemeStyled, current};
use gpui::{
    AnyElement, App, Bounds, ElementId, Entity, FocusHandle, IntoElement, MouseButton,
    ParentElement, Pixels, RenderOnce, Role, ScrollHandle, SharedString, StyleRefinement, Styled,
    TouchEvent, Window, div, point, prelude::*, px, relative,
};
use smallvec::SmallVec;
use web_time::Instant;

pub use snap_offsets::SnapPoint;
pub use switcher::BottomSheetSwitcher;

use super::dialog::DialogPurpose;
use super::modal::{faded, modal_state};
use crate::drag_to_snap_points::executor_clock;
use crate::gestures::pan::{
    PanAxis, PanConfig, PanEvent, PanGesture, PanInput, PanState, create_pan,
};
use crate::layout::SpacingStep;
use gestures::{CONTENT_END_HANDOFF_SLOP, Release, SheetDragExit, SheetGestures};
use snap_offsets::{is_valid_snap_point, resolve_snap_points};

type OpenFn = Rc<dyn Fn(bool, &mut Window, &mut App)>;

/// The smallest reserve the sheet keeps below the window, px (never less than the
/// mouse's stretch).
const MIN_RESERVE: f32 = 48.;
/// A finger can pull past the top by at most the window's height: the rubber band
/// over a sheet as tall as the window then shows 35.5% of it.
const RESERVE_VIEWPORT_SHARE: f32 = 0.4;
/// The longest a reduced-motion cross-fade runs, ms.
const CROSS_FADE_MS: u64 = 200;
/// The coarse pointer's close curve.
const COARSE_CLOSE: astryx_theme::CubicBezier = astryx_theme::CubicBezier(0.3, 0., 0.6, 0.6);

/// Upstream `height`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum SheetHeight {
    /// Fits its content, up to 92% of the window.
    Hug,
    /// 62% of the window, scrolling.
    #[default]
    Capped,
    /// 92% of the window.
    Tall,
    /// An absolute height (upstream's number).
    Length(Pixels),
    /// A fraction of the window's height (upstream's `'50dvh'`-like string).
    Fraction(f32),
}

/// The open state of the sheet whose content renders a sheet (upstream
/// `BottomSheetHostContext`): a sheet in another's content is presented only while
/// its host is open (FR8). At the root there is none, which reads as open.
#[derive(Clone)]
pub(crate) struct SheetHost {
    pub key: String,
    pub open: bool,
}

/// Whether Escape may dismiss the sheet below (upstream `BottomSheetEscapeContext`):
/// Popover's sheet turns it off; `Required` still blocks it.
#[derive(Clone, Copy)]
pub(crate) struct SheetEscape(pub bool);

/// A sheet rising from the bottom edge, upstream `BottomSheet`.
#[derive(IntoElement)]
pub struct BottomSheet {
    id: ElementId,
    open: bool,
    label: SharedString,
    on_open_change: Option<OpenFn>,
    children: SmallVec<[AnyElement; 2]>,
    purpose: DialogPurpose,
    padding: Option<SpacingStep>,
    height: SheetHeight,
    snap_points: Vec<SnapPoint>,
    scrim: bool,
    sheet_id: Option<SharedString>,
    /// The switcher flow it is a step of.
    flow: Option<SharedString>,
    escape: Option<bool>,
    final_focus: Option<FocusHandle>,
    final_named: Option<ElementId>,
    on_cancel: Option<super::modal::CloseFn>,
    style: StyleRefinement,
}

impl BottomSheet {
    /// Upstream `isOpen` and `label`, the sheet's accessible name.
    pub fn new(id: impl Into<ElementId>, open: bool, label: impl Into<SharedString>) -> Self {
        BottomSheet {
            id: id.into(),
            open,
            label: label.into(),
            on_open_change: None,
            children: SmallVec::new(),
            purpose: DialogPurpose::Info,
            padding: None,
            height: SheetHeight::Capped,
            snap_points: Vec::new(),
            scrim: true,
            sheet_id: None,
            flow: None,
            escape: None,
            final_focus: None,
            final_named: None,
            on_cancel: None,
            style: StyleRefinement::default(),
        }
    }

    astryx_primitives::setters! {
        /// Upstream `purpose`: `Info` allows Escape, a scrim press and a swipe; `Form`
        /// allows Escape; `Required` blocks all three.
        purpose: DialogPurpose,
        /// Upstream `padding`: `spacing-4` by default. The block-start inset is never
        /// less than the grab bar's band (`spacing-6`).
        ?padding: SpacingStep,
        /// Upstream `height`.
        height: SheetHeight,
        /// Upstream `hasScrim`: without one the sheet is non-modal and the page stays
        /// interactive.
        scrim: bool,
        /// Upstream `sheetId`, for a BottomSheetSwitcher (deprecated).
        ?sheet_id: SharedString,
    }

    /// Upstream `snapPoints`: extra visible heights the sheet can rest at.
    pub fn snap_points(mut self, points: impl IntoIterator<Item = SnapPoint>) -> Self {
        self.snap_points = points.into_iter().collect();
        self
    }

    /// Upstream `onOpenChange`.
    pub fn on_open_change(mut self, f: impl Fn(bool, &mut Window, &mut App) + 'static) -> Self {
        self.on_open_change = Some(Rc::new(f));
        self
    }

    /// Upstream `finalFocusRef`: what takes focus after the sheet closes. For a sheet
    /// on its own, or the last of a stack, it is focused wherever focus was; with a
    /// modal sheet still open beneath, focus goes into that sheet instead (to this if
    /// it is inside it, else the revealed sheet's panel, FR9).
    pub fn final_focus(mut self, handle: FocusHandle) -> Self {
        self.final_focus = Some(handle);
        self
    }

    /// `finalFocusRef` naming a control by its id, for an owner that holds no handle to
    /// it (PowerSearch's step sheets): the control takes focus when the exit ends.
    pub(crate) fn final_focus_named(mut self, id: Option<ElementId>) -> Self {
        self.final_named = id;
        self
    }

    /// A BottomSheetSwitcher's `onCancel`: called when Escape reaches this step.
    pub(crate) fn on_cancel(mut self, f: super::modal::CloseFn) -> Self {
        self.on_cancel = Some(f);
        self
    }

    pub(crate) fn opened(mut self, open: bool) -> Self {
        self.open = open;
        self
    }

    pub(crate) fn flow(mut self, flow: SharedString) -> Self {
        self.flow = Some(flow);
        self
    }

    /// Popover's sheet presentation: whether Escape dismisses (upstream
    /// `hasEscapeDismiss` through `BottomSheetEscapeContext`).
    pub(crate) fn escape_dismiss(mut self, allowed: bool) -> Self {
        self.escape = Some(allowed);
        self
    }

    pub(crate) fn sheet_key(&self) -> Option<&SharedString> {
        self.sheet_id.as_ref()
    }
}

impl ParentElement for BottomSheet {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl Styled for BottomSheet {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

/// The surface the sheet keeps below the window, px.
fn sheet_reserve(viewport: f32, tracks_viewport: bool) -> f32 {
    if tracks_viewport {
        MIN_RESERVE.max((viewport * RESERVE_VIEWPORT_SHARE).ceil())
    } else {
        MIN_RESERVE
    }
}

/// A finger on the body, which the sheet follows for the scroll handoff (upstream
/// `touchDragRef`).
#[derive(Clone)]
struct BodyTouch {
    id: u64,
    start: (f64, f64),
    last: (f64, f64),
    /// Armed at the top: a pull down is the sheet's.
    top: bool,
    /// Armed at the end: a pull up is the sheet's.
    bottom: bool,
    /// Where the content ran out, while the finger is past it.
    content_end: Option<f64>,
    /// The handoff drives a drag.
    promoted: bool,
    /// The drag began where the content ran out: it claims nothing.
    at_content_end: bool,
    /// The finger's scroll reached the body: no content under it took the touch.
    scrolled: bool,
    tracker: VelocityTracker,
    /// The boxes this touch can scroll, innermost first and the body last (upstream
    /// `scrollChainUnder`): their edges decide the handoff for the rest of the touch.
    chain: Vec<ScrollHandle>,
}

/// A mouse or pen press on the body, armed at the top (upstream `armedBodyRef`).
#[derive(Clone)]
struct BodyMouse {
    start: f64,
    tracker: VelocityTracker,
    promoted: bool,
    chain: Vec<ScrollHandle>,
}

/// A drag's close in flight: the close spring from the release speed.
#[derive(Clone)]
struct DragExitMotion {
    handle: MotionHandle,
    from: f64,
    distance: f64,
    duration: Duration,
    scrim: f32,
}

/// What a sheet keeps between frames beside its modal state.
pub(crate) struct SheetState {
    gestures: SheetGestures,
    /// The sheet's offset (y, px from fully open) and its Reduce Motion fades.
    engine: MotionEngine,
    clock: astryx_primitives::motion::Clock,
    settle: Option<MotionHandle>,
    exit: Option<DragExitMotion>,
    /// The scrim's opacity the drag set (1 = full).
    scrim: f32,
    presented: bool,
    /// The first frame of a close is held in the open pose, until the stack has
    /// paired it (a replace) or not.
    close_frames: u8,
    handle_pan: Option<PanGesture>,
    /// The body's key among the window's native scrollers.
    body_key: Option<u64>,
    body_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    /// Where the panel was drawn last frame (the stack reads which sheet a press
    /// landed in).
    panel_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    /// The body's hitbox, to tell whether a finger landed on it (nothing above).
    body_hitbox: Rc<std::cell::RefCell<Option<gpui::Hitbox>>>,
    /// The handle's pan in progress, and whether the sheet takes it.
    pan_input: Option<(PointerKind, bool)>,
    body_touch: Option<BodyTouch>,
    body_mouse: Option<BodyMouse>,
    /// Whether the sheet is the one a gesture is for (the top of its stack).
    takes_gestures: bool,
    /// The fingers down, as the raw touches tell.
    fingers: Vec<u64>,
    /// Alive while the sheet renders: the stack forgets a sheet that unmounted.
    token: Rc<()>,
    key: String,
    /// Hug content's natural height, px.
    content_height: Rc<Cell<Option<f32>>>,
    last_geometry: Option<(u32, Vec<u64>, u32)>,
    recede: (f32, f32, Option<Instant>),
    host: Option<SheetHost>,
    /// The sheet's opener, as the stack hands it on.
    opener: Option<FocusHandle>,
    /// Whether focus was inside the sheet as it began to close.
    had_focus: bool,
    on_open_change: Option<OpenFn>,
    /// A frame's fully open height, for the gestures' callers.
    full_height: f32,
}

impl Drop for SheetState {
    fn drop(&mut self) {
        #[cfg(test)]
        probe::forget_owned(&self.key, Rc::as_ptr(&self.token) as usize);
    }
}

impl SheetState {
    fn offset(&self) -> f64 {
        self.engine.get(MotionProperty::Y, None)
    }
}

fn sheet_state(id: &ElementId, window: &mut Window, cx: &mut App) -> Entity<SheetState> {
    window.use_keyed_state(
        ElementId::Name(format!("{id}-sheet-state").into()),
        cx,
        |_, cx| {
            let clock = executor_clock(cx);
            SheetState {
                gestures: SheetGestures::default(),
                engine: MotionEngine::new(EngineOptions {
                    initial: MotionValues::default(),
                    now: Some(clock.clone()),
                    frames: true,
                }),
                clock,
                settle: None,
                exit: None,
                scrim: 1.,
                presented: false,
                close_frames: 0,
                handle_pan: None,
                body_key: None,
                body_bounds: Rc::default(),
                body_hitbox: Rc::default(),
                panel_bounds: Rc::default(),
                pan_input: None,
                body_touch: None,
                body_mouse: None,
                takes_gestures: false,
                fingers: Vec::new(),
                token: Rc::new(()),
                key: String::new(),
                content_height: Rc::default(),
                last_geometry: None,
                recede: (0., 0., None),
                host: None,
                opener: None,
                had_focus: false,
                on_open_change: None,
                full_height: 0.,
            }
        },
    )
}

/// The body's remaining scroll toward its end, px.
fn natural_end_gap(handle: &ScrollHandle, inset: f64) -> f64 {
    let max = f64::from(f32::from(handle.max_offset().y)) - inset;
    let top = -f64::from(f32::from(handle.offset().y));
    max.max(0.) - top
}

fn body_at_end(handle: &ScrollHandle) -> bool {
    let max = f64::from(f32::from(handle.max_offset().y));
    let top = -f64::from(f32::from(handle.offset().y));
    top >= max - 1.
}

/// A release's settle spring: UIKit's critically damped detent below the flick
/// speed, its 80% fast release from it.
fn settle_spring(velocity: f64, touch: bool) -> MotionSpring {
    if touch && velocity.abs() >= FAST_SPEED {
        MotionSpring::inline(springs::SHEET_RELEASE_FAST)
    } else {
        MotionSpring::inline(springs::SHEET_DETENT)
    }
}

impl SheetState {
    /// Moves the sheet to `offset` on a spring from `velocity`.
    fn settle_to(&mut self, offset: f64, velocity: f64, touch: bool) {
        let transition = SpringTransition::new(settle_spring(velocity, touch))
            .velocity(MotionValues::new().y(velocity));
        self.settle = Some(self.engine.animate(
            MotionValues::new().y(offset),
            transition,
            AnimateOptions {
                touch_release: touch,
                ..Default::default()
            },
        ));
    }

    fn begin_drag(
        &mut self,
        pointer: PointerKind,
        coord: f64,
        start: Option<f64>,
        tracker: Option<VelocityTracker>,
        gap: f64,
    ) {
        let now = (self.clock)();
        // A drag catches a settle where it is on screen.
        let at = self.offset();
        if let Some(settle) = self.settle.take() {
            settle.stop();
        }
        self.engine.set(MotionValues::new().y(at));
        self.gestures.active_offset = at;
        self.gestures
            .begin(pointer, coord, now, start, tracker, gap);
    }

    /// A drag moved to `coord`; returns whether travel started (the keyboard goes).
    fn drag_to(&mut self, coord: f64) -> bool {
        let now = (self.clock)();
        match self.gestures.move_to(coord, now) {
            Some(moved) => {
                self.engine.set(MotionValues::new().y(moved.offset));
                self.scrim = moved.scrim as f32;
                moved.travel_started
            }
            None => false,
        }
    }

    /// A drag ended at `coord`: a settle, or a close request for the owner.
    fn end_drag(&mut self, coord: f64) -> Option<SheetDragExit> {
        let now = (self.clock)();
        let base = self.gestures.drag.as_ref().map(|d| d.base_offset);
        let release = self.gestures.end(coord, now)?;
        let moved_detent =
            matches!(release, Release::Settle { offset, .. } if Some(offset) != base);
        match release {
            Release::Settle {
                offset,
                velocity,
                scrim,
                touch,
            } => {
                self.scrim = scrim as f32;
                self.settle_to(offset, velocity, touch);
                // A short tick on a settle onto another detent, skipped under Reduce
                // Motion.
                let reduced = self.engine.reduced_motion(false)
                    != astryx_primitives::motion::ReducedMotion::Off;
                if moved_detent && !reduced {
                    astryx_primitives::gestures::haptics::play_haptic(
                        astryx_primitives::gestures::haptics::HapticFeedbackKind::ImpactLight,
                    );
                }
                None
            }
            Release::Dismiss { exit } => Some(exit.unwrap_or(SheetDragExit {
                distance: 0.,
                velocity: 0.,
            })),
        }
    }

    fn cancel_drag(&mut self) {
        if let Some(scrim) = self.gestures.cancel() {
            self.scrim = scrim as f32;
            let to = self.gestures.settled_offset;
            self.settle_to(to, 0., false);
        }
    }
}

/// The grab bar's pan report, applied to the sheet.
fn on_sheet_pan(
    state: &Entity<SheetState>,
    event: PanEvent,
    scroll: ScrollHandle,
    surface: FocusHandle,
) {
    let state = state.clone();
    after_gesture(move |window, cx| {
        let pointer = match event.input {
            PanInput::Touch => PointerKind::Touch,
            PanInput::Pen => PointerKind::Pen,
            _ => PointerKind::Mouse,
        };
        let coord = event.total_translation.y;
        let mut close: Option<(SheetDragExit, OpenFn)> = None;
        let mut travel = false;
        state.update(cx, |s, _| match event.state {
            PanState::Began => {
                // A drag already in flight, a covered sheet or an exit takes nothing.
                let takes = !s.gestures.is_dragging() && s.exit.is_none() && s.takes_gestures;
                s.pan_input = Some((pointer, takes));
                if takes {
                    let gap = natural_end_gap(&scroll, s.gestures.scroll_preservation_inset);
                    s.begin_drag(pointer, 0., Some(0.), None, gap);
                    travel = s.drag_to(coord);
                }
            }
            PanState::Changed => {
                if s.pan_input.is_some_and(|(_, takes)| takes) {
                    travel = s.drag_to(coord);
                }
            }
            PanState::Ended => {
                if s.pan_input.take().is_some_and(|(_, takes)| takes)
                    && let Some(exit) = s.end_drag(coord)
                {
                    close = s.on_open_change.clone().map(|f| (exit, f));
                }
            }
            PanState::Cancelled => {
                if s.pan_input.take().is_some_and(|(_, takes)| takes) {
                    s.cancel_drag();
                }
            }
        });
        if travel {
            // Focusing the sheet dismisses the keyboard as the sheet starts to move.
            surface.focus(window, cx);
        }
        if let Some((_, report)) = close {
            report(false, window, cx);
        }
        window.refresh();
    });
}

/// Attaches the grab bar's pan once: a finger, a mouse and a pen all drag it.
fn ensure_pans(
    state: &Entity<SheetState>,
    host: &GestureHost,
    scroll: &ScrollHandle,
    surface: &FocusHandle,
    cx: &mut App,
) {
    if state.read(cx).handle_pan.is_some() {
        return;
    }
    // Weak: the pan lives in the state, so a strong handle would keep it forever.
    let (weak, scroll, surface) = (state.downgrade(), scroll.clone(), surface.clone());
    let mut options = PanOptions::named("sheet-handle");
    options.maximum_number_of_touches = 1;
    options.hysteresis = Hysteresis::Fixed(1.);
    let handle = create_pan(
        host,
        PanConfig {
            axis: Some(PanAxis::Y),
            on_pan: Rc::new(move |event: &PanEvent| {
                if let Some(state) = weak.upgrade() {
                    on_sheet_pan(&state, *event, scroll.clone(), surface.clone())
                }
            }),
            recognizer: Some(options),
        },
    );
    let key = handle.attachment().key();
    state.update(cx, |s, _| {
        // The body registers as a native scroller under a key of its own.
        s.body_key = Some(key ^ (1 << 40));
        s.handle_pan = Some(handle);
    });
}

fn at_top(handle: &ScrollHandle) -> bool {
    -f64::from(f32::from(handle.offset().y)) <= 0.
}

fn scrolls(handle: &ScrollHandle) -> bool {
    f64::from(f32::from(handle.max_offset().y)) > 1.
}

/// Nothing in the chain can scroll up any further, so a pull down is the sheet's.
fn chain_at_top(chain: &[ScrollHandle]) -> bool {
    chain.iter().all(at_top)
}

/// Nothing in the chain can scroll down any further, so a pull up is the sheet's.
fn chain_at_bottom(chain: &[ScrollHandle]) -> bool {
    chain.iter().all(body_at_end)
}

fn chain_scrolls(chain: &[ScrollHandle]) -> bool {
    chain.iter().any(scrolls)
}

/// The scrollers a gesture that lands at `point` can scroll, innermost first and the
/// body last (upstream `scrollChainUnder`): every registered scroller under the point
/// that lies inside the body (scrolling containers register with the window's gesture
/// host as they prepaint, `gestures::scroller`), then the body itself. A scroller of
/// the page behind the sheet is not inside the body, so it never counts.
fn scroll_chain(
    host: &GestureHost,
    body_key: Option<u64>,
    body: Option<Bounds<Pixels>>,
    point: gpui::Point<Pixels>,
    scroll: &ScrollHandle,
) -> Vec<ScrollHandle> {
    let mut chain: Vec<ScrollHandle> = Vec::new();
    if let Some(body) = body {
        let at = astryx_primitives::gestures::Point::new(
            f64::from(f32::from(point.x)),
            f64::from(f32::from(point.y)),
        );
        let inside = |b: &Bounds<Pixels>| {
            let slack = px(0.5);
            b.left() >= body.left() - slack
                && b.top() >= body.top() - slack
                && b.right() <= body.right() + slack
                && b.bottom() <= body.bottom() + slack
        };
        for scroller in host.scrollers().chain_at(at) {
            if Some(scroller.key) == body_key || !scroller.scrolls_y || !inside(&scroller.bounds) {
                continue;
            }
            chain.push(scroller.handle.clone());
        }
    }
    chain.push(scroll.clone());
    chain
}

/// What a body touch's move does (upstream `onTouchMove` before a drag): promote it
/// to a sheet drag against the armed edge, give an edge back to the content, or
/// notice the content running out mid-pull.
fn consider_body_touch(s: &mut SheetState, scroll: &ScrollHandle) -> bool {
    let Some(touch) = s.body_touch.clone() else {
        return false;
    };
    // Only a finger whose scroll reached the body: content that takes the touch
    // itself (no one-finger pan left to the sheet, FR27) never scrolls it.
    if touch.promoted || !touch.scrolled || s.gestures.is_dragging() || !s.takes_gestures {
        return false;
    }
    let (x, y) = touch.last;
    let delta = y - touch.start.1;
    let sideways = (x - touch.start.0).abs();
    let mostly_vertical = delta.abs() > sideways;
    let at_top = chain_at_top(&touch.chain);
    let at_end = chain_at_bottom(&touch.chain);
    let opposes_top = touch.top && delta > 0. && at_top;
    let opposes_bottom = touch.bottom && delta < 0. && at_end;
    let slop = gestures::DRAG_PROMOTION_SLOP;
    let gap = natural_end_gap(scroll, s.gestures.scroll_preservation_inset);
    if (opposes_top && mostly_vertical && delta > slop)
        || (opposes_bottom && mostly_vertical && delta < -slop)
    {
        // Against the armed edge: the pull is the sheet's, from where the finger
        // landed, so its whole travel carries over.
        let tracker = touch.tracker.clone();
        s.begin_drag(
            PointerKind::Touch,
            y,
            Some(touch.start.1),
            Some(tracker),
            gap,
        );
        if let Some(t) = s.body_touch.as_mut() {
            t.promoted = true;
            t.at_content_end = false;
        }
        return s.drag_to(y);
    }
    let Some(t) = s.body_touch.as_mut() else {
        return false;
    };
    // The first move past the slop on either axis decides the edges, once: a mostly
    // sideways move is a row's, so the sheet stops following the touch.
    if delta.abs().max(sideways) > slop && (t.top || t.bottom) {
        if !mostly_vertical {
            s.body_touch = None;
            return false;
        }
        t.top = false;
        t.bottom = false;
    }
    let wobble = astryx_primitives::gestures::TAP_WOBBLE;
    if t.top && delta <= -wobble && !at_end {
        t.top = false;
    }
    if t.bottom && delta >= wobble && !at_top {
        t.bottom = false;
    }
    // The content ran out mid-pull: the sheet takes the travel past that point, and
    // the content holds still at its end (FR18).
    if at_end {
        match t.content_end {
            None => t.content_end = Some(y),
            Some(end) if end - y >= CONTENT_END_HANDOFF_SLOP => {
                let tracker = t.tracker.clone();
                t.promoted = true;
                t.at_content_end = true;
                s.begin_drag(PointerKind::Touch, y, Some(end), Some(tracker), gap);
                return s.drag_to(y);
            }
            Some(_) => {}
        }
    } else {
        t.content_end = None;
    }
    false
}

/// A raw finger (fork patch R23's listener sees every one): the body's scroll
/// handoff, and a second finger, which is a pinch the window keeps (any drag ends
/// on its detent). Returns whether the sheet claims the touch from GPUI.
fn on_raw_touch(
    state: &Entity<SheetState>,
    host: &GestureHost,
    scroll: &ScrollHandle,
    surface: &FocusHandle,
    event: &TouchEvent,
    window: &mut Window,
    cx: &mut App,
) -> bool {
    let id = event.id.0;
    let (x, y) = (
        f64::from(f32::from(event.position.x)),
        f64::from(f32::from(event.position.y)),
    );
    let now = host.now();
    let fingers = host.fingers_down();
    let claimed_by_content = host.is_touch_claimed(id);
    let mut changed = false;
    let mut travel = false;
    let mut close: Option<OpenFn> = None;
    let mut claim = false;
    state.update(cx, |s, _| match event.phase {
        gpui::TouchPhase::Started => {
            if !s.fingers.contains(&id) {
                s.fingers.push(id);
            }
            if fingers > 1
                || s.fingers.len() > 1
                || s.body_touch.as_ref().is_some_and(|t| t.id != id)
            {
                if s.gestures.is_dragging() {
                    s.cancel_drag();
                    s.pan_input = s.pan_input.map(|(p, _)| (p, false));
                    changed = true;
                }
                s.body_touch = None;
                return;
            }
            let inside = s
                .body_hitbox
                .borrow()
                .as_ref()
                .is_some_and(|h| h.is_hovered_at(event.position, window));
            if !inside || !s.takes_gestures || s.gestures.is_dragging() {
                s.body_touch = None;
                return;
            }
            let mut tracker = VelocityTracker::default();
            tracker.add(0., y, now);
            // At the end a pull up is claimed only below the tallest detent, or over
            // content that cannot scroll; at the tallest detent a list at its end
            // keeps its touch until the content-end handoff.
            let chain = scroll_chain(
                host,
                s.body_key,
                s.body_bounds.get(),
                event.position,
                scroll,
            );
            let bottom = chain_at_bottom(&chain)
                && (s.gestures.settled_offset > 0. || !chain_scrolls(&chain));
            s.body_touch = Some(BodyTouch {
                id,
                start: (x, y),
                last: (x, y),
                top: chain_at_top(&chain),
                bottom,
                content_end: None,
                promoted: false,
                at_content_end: false,
                scrolled: false,
                tracker,
                chain,
            });
        }
        gpui::TouchPhase::Moved => {
            let Some(t) = s.body_touch.as_mut().filter(|t| t.id == id) else {
                return;
            };
            t.last = (x, y);
            t.tracker.add(0., y, now);
            if t.promoted {
                if t.at_content_end && t.content_end.is_some_and(|end| y >= end) {
                    // Back where the content ran out: the gesture is the content's.
                    t.promoted = false;
                    t.at_content_end = false;
                    t.content_end = None;
                    s.cancel_drag();
                } else {
                    claim = !t.at_content_end;
                    travel = s.drag_to(y);
                }
                changed = true;
                return;
            }
            if claimed_by_content && !t.scrolled {
                // A recognizer in the content took the touch.
                s.body_touch = None;
                return;
            }
            if consider_body_touch(s, scroll) {
                travel = true;
            }
            changed |= s.gestures.is_dragging();
        }
        gpui::TouchPhase::Ended | gpui::TouchPhase::Cancelled => {
            s.fingers.retain(|f| *f != id);
            if s.body_touch.as_ref().is_some_and(|t| t.id != id) {
                return;
            }
            let Some(t) = s.body_touch.take() else {
                return;
            };
            if t.promoted && s.gestures.is_dragging() {
                changed = true;
                claim = !t.at_content_end;
                if event.phase == gpui::TouchPhase::Cancelled {
                    s.cancel_drag();
                } else if s.end_drag(y).is_some() {
                    close = s.on_open_change.clone();
                }
            }
        }
    });
    if travel {
        surface.focus(window, cx);
    }
    if let Some(report) = close {
        let report = report.clone();
        window.defer(cx, move |window, cx| report(false, window, cx));
    }
    if changed {
        window.refresh();
    }
    claim
}

/// The finger's scroll reached the body: the touch is a scroll, which the sheet may
/// take at an edge.
fn on_body_scroll(
    state: &Entity<SheetState>,
    scroll: &ScrollHandle,
    surface: &FocusHandle,
    window: &mut Window,
    cx: &mut App,
) {
    let travel = state.update(cx, |s, _| {
        let Some(t) = s.body_touch.as_mut() else {
            return false;
        };
        t.scrolled = true;
        consider_body_touch(s, scroll)
    });
    if travel {
        surface.focus(window, cx);
    }
    if state.read(cx).gestures.is_dragging() {
        window.refresh();
    }
}

/// A mouse or pen press on the body at its top arms a pull down (upstream's pointer
/// path): past the slop it becomes a drag, and a move up is a scroll.
fn on_body_mouse(
    state: &Entity<SheetState>,
    scroll: &ScrollHandle,
    at: gpui::Point<Pixels>,
    phase: u8,
    window: &mut Window,
    cx: &mut App,
) {
    let y = f64::from(f32::from(at.y));
    let now = (state.read(cx).clock)();
    let host = gesture_host(window, cx);
    let mut close: Option<OpenFn> = None;
    let changed = state.update(cx, |s, _| match phase {
        0 => {
            s.body_mouse = None;
            let chain = scroll_chain(&host, s.body_key, s.body_bounds.get(), at, scroll);
            if chain_at_top(&chain) && s.takes_gestures && !s.gestures.is_dragging() {
                let mut tracker = VelocityTracker::default();
                tracker.add(0., y, now);
                s.body_mouse = Some(BodyMouse {
                    start: y,
                    tracker,
                    promoted: false,
                    chain,
                });
            }
            false
        }
        1 => {
            let Some(m) = s.body_mouse.as_mut() else {
                return false;
            };
            m.tracker.add(0., y, now);
            if m.promoted {
                s.drag_to(y);
                return true;
            }
            let delta = y - m.start;
            if delta > gestures::DRAG_PROMOTION_SLOP && chain_at_top(&m.chain) {
                m.promoted = true;
                let (start, tracker) = (m.start, m.tracker.clone());
                let gap = natural_end_gap(scroll, s.gestures.scroll_preservation_inset);
                s.begin_drag(PointerKind::Mouse, y, Some(start), Some(tracker), gap);
                s.drag_to(y);
                true
            } else {
                if delta < 0. {
                    s.body_mouse = None;
                }
                false
            }
        }
        _ => {
            let Some(m) = s.body_mouse.take() else {
                return false;
            };
            if m.promoted && s.end_drag(y).is_some() {
                close = s.on_open_change.clone();
            }
            m.promoted
        }
    });
    if let Some(report) = close {
        window.defer(cx, move |window, cx| report(false, window, cx));
    }
    if changed {
        window.refresh();
    }
}

impl RenderOnce for BottomSheet {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        render_sheet(self, window, cx)
    }
}

/// How the panel is presented this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
enum PanelMotion {
    /// Entering, 0 to 1.
    Entering(f32),
    Open,
    /// Leaving on the stylesheet's exit, 0 to 1.
    Exiting(f32),
    /// Leaving on a drag's close spring, 0 to 1 of its travel.
    DragExit(f32),
}

fn render_sheet(sheet: BottomSheet, window: &mut Window, cx: &mut App) -> AnyElement {
    let theme = current(window, cx);
    let t = theme.tokens().clone();
    let rem = window.rem_size();
    let viewport = window.viewport_size();
    let reduced = prefers_reduced_motion(cx);
    let coarse = coarse_primary_pointer(window, cx);
    let key = format!("{:?}", sheet.id);
    let modal = modal_state(&sheet.id, window, cx);
    let state = sheet_state(&sheet.id, window, cx);
    let host_ctx = use_context::<SheetHost>(cx);
    let escape_ctx = sheet
        .escape
        .or_else(|| use_context::<SheetEscape>(cx).map(|e| e.0))
        .unwrap_or(true);
    let host_open = host_ctx.as_ref().is_none_or(|h| h.open);
    let open = sheet.open && host_open;
    let gesture = gesture_host(window, cx);
    let now = cx.background_executor().now();

    let token = state.update(cx, |s, _| {
        s.on_open_change = sheet.on_open_change.clone();
        s.host = host_ctx.clone();
        s.key = key.clone();
        let engine = &s.engine;
        engine.set_reduced_motion(reduced);
        engine.set_coarse_pointer(coarse);
        Rc::downgrade(&s.token)
    });

    // Opening and closing (the stack decides where focus goes when it leaves).
    let was_open = modal.read(cx).open;
    if open && !was_open {
        let focused = window.focused(cx);
        modal.update(cx, |s, _| {
            s.open = true;
            s.return_to = focused.clone();
            s.shown_at = None;
            s.closed_at = None;
            s.pending_focus = true;
            s.focus = super::dialog::FocusScope::default();
        });
        state.update(cx, |s, _| {
            let reopened = s.presented;
            s.presented = true;
            s.close_frames = 0;
            s.opener = focused;
            s.exit = None;
            if !reopened {
                s.gestures.reset();
                s.engine.set(MotionValues::new().y(0.).opacity(1.));
                s.scrim = 1.;
                s.settle = None;
            } else {
                // A reopen during the exit takes the sheet back from where it is.
                let to = s.gestures.settled_offset;
                s.settle_to(to, 0., false);
                s.scrim = 1.;
            }
        });
    } else if !open && was_open {
        let surface = modal.read(cx).surface.clone();
        let inside = surface.contains_focused(window, cx) || window.focused(cx).is_none();
        modal.update(cx, |s, _| {
            s.open = false;
            s.pending_focus = false;
            s.closed_at = Some(now);
        });
        state.update(cx, |s, _| {
            s.had_focus = inside;
            s.close_frames = 1;
            // A drag in flight ends with the close.
            s.gestures.drag = None;
            s.pan_input = None;
            s.body_touch = None;
            s.body_mouse = None;
        });
    }
    // The owner's answer to a close the release asked for.
    let requested = sheet.open;
    state.update(cx, |s, _| {
        if let Some(scrim) = s.gestures.answer(requested) {
            // Refused: back to the detent the drag left, its scrim with it.
            s.scrim = scrim as f32;
            let to = s.gestures.settled_offset;
            s.settle_to(to, 0., true);
        }
        if !requested
            && s.exit.is_none()
            && let Some(exit) = s.gestures.drag_exit()
            && !reduced
            && exit.distance > 0.
        {
            // The close spring from the release speed (FR19, AST-094 FR38).
            let from = s.offset();
            if let Some(settle) = s.settle.take() {
                settle.stop();
            }
            let handle = s.engine.animate(
                MotionValues::new().y(from + exit.distance),
                SpringTransition::new(role::SHEET_DISMISS_SWIPE)
                    .velocity(MotionValues::new().y(exit.velocity)),
                AnimateOptions {
                    touch_release: true,
                    ..Default::default()
                },
            );
            let ms = resolve_settle_ms(
                role::SHEET_DISMISS_SWIPE,
                s.engine.tempo(),
                Some(SpringMove {
                    from: 0.,
                    to: exit.distance,
                    velocity: exit.velocity,
                    epsilon: MotionProperty::Y.settle_threshold(),
                }),
            );
            s.exit = Some(DragExitMotion {
                handle,
                from,
                distance: exit.distance,
                duration: Duration::from_secs_f64(ms.max(0.) / 1000.),
                scrim: s.scrim,
            });
        }
    });

    // The motion: entering, open, or leaving.
    let dismissed_by_touch = state.read(cx).gestures.is_dismissed_by_touch_drag();
    let cross_fade = reduced && (coarse || dismissed_by_touch);
    let enter = if reduced {
        if coarse {
            Duration::from_millis(CROSS_FADE_MS)
        } else {
            Duration::from_millis(10)
        }
    } else {
        t.duration.medium
    };
    let exit_duration = match &state.read(cx).exit {
        Some(exit) => exit.duration,
        None if cross_fade => Duration::from_millis(CROSS_FADE_MS),
        None if reduced => Duration::from_millis(10),
        None => t.duration.medium,
    };
    let held = state.read(cx).close_frames > 0;
    let phase = super::modal::phase(&modal, enter, exit_duration, window, cx);
    let motion = match phase {
        super::modal::Phase::Open { progress } if progress < 1. => PanelMotion::Entering(progress),
        super::modal::Phase::Open { .. } => PanelMotion::Open,
        _ if held => PanelMotion::Open,
        super::modal::Phase::Closing { progress } => {
            if state.read(cx).exit.is_some() {
                PanelMotion::DragExit(progress)
            } else {
                PanelMotion::Exiting(progress)
            }
        }
        super::modal::Phase::Hidden => {
            return hide(
                &sheet.id,
                &key,
                &state,
                &modal.read(cx).surface.clone(),
                &gesture,
                window,
                cx,
            );
        }
    };
    if held {
        window.request_animation_frame();
        state.update(cx, |s, _| s.close_frames = s.close_frames.saturating_sub(1));
    }

    // The geometry: the budget, the reserve, the detents.
    let vh = f32::from(viewport.height);
    let sized_by_caller = sheet.style.size.height.is_some()
        || sheet.style.min_size.height.is_some()
        || sheet.style.max_size.height.is_some();
    // Only a finger stretches the sheet past the smallest reserve, so a fine pointer
    // keeps it; so does a sheet the caller sizes, whose stretch stops there.
    let touch_device = coarse;
    let reserve = sheet_reserve(vh, touch_device && !sized_by_caller);
    let budget = match sheet.height {
        SheetHeight::Hug | SheetHeight::Tall => vh * 0.92,
        SheetHeight::Capped => vh * 0.62,
        SheetHeight::Length(l) => f32::from(l).min(vh),
        SheetHeight::Fraction(f) => vh * f.clamp(0., 1.),
    };
    let content_height = state.read(cx).content_height.get();
    let open_height = match sheet.height {
        SheetHeight::Hug => content_height.map_or(budget, |h| h.min(budget)),
        _ => budget,
    };
    let full = open_height + reserve;
    let ignored: Vec<&SnapPoint> = sheet
        .snap_points
        .iter()
        .filter(|p| !is_valid_snap_point(p))
        .collect();
    if !ignored.is_empty() {
        astryx_primitives::dev_warn_once(&format!("sheet-snap-{key}"), || {
            format!(
                "BottomSheet: snapPoints ignored {ignored:?}. A snap point is a viewport fraction above 0 and up to 1 (0.5 is half the screen), a px length ('320px'), or a percentage ('50%')."
            )
        });
    }
    let snap_heights = resolve_snap_points(&sheet.snap_points, f64::from(vh));
    let geometry = (
        full.to_bits(),
        snap_heights.iter().map(|h| h.to_bits()).collect::<Vec<_>>(),
        reserve.to_bits(),
    );
    let scroll_area = use_scrollable_area(
        &ElementId::Name(format!("{}-body", sheet.id).into()),
        UseScrollableAreaOptions {
            axis: ScrollAxis::Both,
            keyboard_access: ScrollKeyboardAccess::ContentOrViewport {
                label: sheet.label.clone(),
                role: ScrollRole::Group,
            },
            overscroll: ScrollOverscroll::Contain,
            sticky_containment: ScrollStickyContainment::Always,
        },
        window,
        cx,
    );
    let scroll = scroll_area.scroll_handle().clone();
    let interactive = open;
    state.update(cx, |s, _| {
        s.gestures.can_dismiss = sheet.purpose == DialogPurpose::Info;
        s.gestures.reserve = f64::from(reserve);
        s.full_height = full;
        let changed = s.last_geometry.as_ref() != Some(&geometry);
        s.gestures.snap_heights = snap_heights.clone();
        if changed {
            let first = s.last_geometry.is_none() || s.gestures.sheet_height <= 0.;
            s.last_geometry = Some(geometry);
            if first || !interactive {
                s.gestures.sheet_height = f64::from(full);
            } else {
                // Re-anchored to the same detent, without animating; a settle in flight
                // keeps moving toward it in the new geometry.
                let gap = natural_end_gap(&scroll, s.gestures.scroll_preservation_inset);
                if let Some((offset, scrim)) = s.gestures.reanchor(f64::from(full), gap) {
                    s.scrim = scrim as f32;
                    match s.settle.clone().filter(|h| h.finished().is_none()) {
                        Some(_) => {
                            s.settle = Some(s.engine.animate(
                                MotionValues::new().y(offset),
                                SpringTransition::new(MotionSpring::inline(springs::SHEET_DETENT)),
                                AnimateOptions::default(),
                            ));
                        }
                        None => s.engine.set(MotionValues::new().y(offset)),
                    }
                }
                s.gestures.sheet_height = f64::from(full);
            }
        }
        // A window that lost its focus (a blur) cancels a drag.
        if s.gestures.is_dragging() && !window.is_window_active() {
            s.cancel_drag();
        }
    });

    // The engine moves on; a settle that landed swaps in its layout.
    let engine_now = (state.read(cx).clock)();
    let (offset, moving) = state.update(cx, |s, _| {
        s.engine.advance(engine_now);
        s.engine.frame(engine_now);
        if let Some(settle) = s.settle.clone()
            && settle.finished().is_some()
        {
            s.settle = None;
            s.gestures.complete_settle();
        }
        let moving = !s.engine.is_settled() || s.gestures.is_dragging();
        (s.offset() as f32, moving)
    });
    if moving {
        window.request_animation_frame();
    }

    // The stack.
    let surface_focus = modal.read(cx).surface.clone();
    let (opener, had_focus) = {
        let s = state.read(cx);
        (
            sheet.final_focus.clone().or_else(|| s.opener.clone()),
            s.had_focus,
        )
    };
    let resting_top = vh - (full - reserve - offset.max(0.));
    let view = stack::sync(
        stack::Report {
            key: key.clone(),
            modal: sheet.scrim,
            open,
            flow: sheet.flow.clone(),
            host: host_ctx.as_ref().map(|h| h.key.clone()),
            surface: surface_focus.clone(),
            opener: opener.clone(),
            final_named: sheet.final_named.clone(),
            had_focus,
            bounds: state.read(cx).panel_bounds.get(),
            top_edge: resting_top,
            duration: t.duration.medium,
            reduced,
            coarse,
            alive: token.clone(),
        },
        window,
        cx,
    );
    if let Some(adopted) = view.adopted_opener.clone() {
        // The entering sheet returns focus where the sheet it replaced would have.
        state.update(cx, |s, _| s.opener = Some(adopted));
    }
    if view.retained.is_some_and(|r| r.done) {
        return hide(
            &sheet.id,
            &key,
            &state,
            &modal.read(cx).surface.clone(),
            &gesture,
            window,
            cx,
        );
    }
    let retained = view.retained;

    // The recede: a step per covered level, moving over the medium duration.
    let steps = view.depth.min(stack::MAX_RECEDE_STEPS) as f32;
    let recede = state.update(cx, |s, _| {
        let (from, to, since) = s.recede;
        if to != steps {
            let current = recede_at(s.recede, now, t.duration.medium, reduced);
            s.recede = (current, steps, Some(now));
        } else if since.is_none() {
            s.recede = (steps, steps, Some(now));
        }
        let _ = from;
        recede_at(s.recede, now, t.duration.medium, reduced)
    });
    if (recede - steps).abs() > 1e-3 {
        window.request_animation_frame();
    }

    // Pans, once, and the body as a native scroller of the window.
    ensure_pans(&state, &gesture, &scroll, &surface_focus, cx);
    let takes_gestures = open && view.is_top && retained.is_none();
    let (handle_pan, body_key, body_bounds, content_cell) = state.update(cx, |s, _| {
        s.takes_gestures = takes_gestures;
        if !takes_gestures && s.gestures.is_dragging() {
            s.cancel_drag();
        }
        (
            s.handle_pan.clone().expect("attached"),
            s.body_key,
            s.body_bounds.clone(),
            s.content_height.clone(),
        )
    });

    let gestures_view = state.read(cx).gestures.clone();
    let layout_offset = gestures_view.layout_offset() as f32;
    let dragging = gestures_view.is_dragging();
    let settling = gestures_view.is_settling();
    let mut translation = offset - layout_offset;
    let traveling = dragging || settling;
    let measured = gestures_view.sheet_height > 0.;
    let resized_height = (measured && (layout_offset > 0. || traveling))
        .then(|| px((full - layout_offset.max(0.)).max(0.)));

    // Entering and leaving.
    let engine_opacity = state.read(cx).engine.shown_opacity(None) as f32;
    let mut opacity = engine_opacity;
    let mut scrim = state.read(cx).scrim;
    match motion {
        PanelMotion::Entering(p) => {
            if reduced {
                if coarse {
                    opacity *= p;
                }
            } else {
                translation += (1. - t.ease.standard.ease(p)) * full;
            }
            scrim *= t.ease.standard.ease(p);
        }
        PanelMotion::Open => {}
        PanelMotion::Exiting(p) => {
            let eased = if coarse {
                COARSE_CLOSE.ease(p)
            } else {
                t.ease.exit.ease(p)
            };
            if cross_fade {
                opacity *= 1. - p;
            } else if !reduced {
                translation += eased * (full - translation.max(0.));
            }
            scrim *= 1. - p;
        }
        PanelMotion::DragExit(_) => {
            let exit = state.read(cx).exit.clone();
            if let Some(exit) = exit {
                let travelled =
                    ((f64::from(offset) - exit.from) / exit.distance.max(1.)).clamp(0., 1.) as f32;
                scrim = exit.scrim * (1. - travelled);
                if exit.handle.finished().is_some() {
                    scrim = 0.;
                }
            }
        }
    }
    if let Some(r) = retained {
        translation += r.alignment;
        opacity *= r.opacity;
    }

    let padding = sheet
        .padding
        .map_or(t.spacing.s4.to_pixels(rem), |s| s.token(&t).to_pixels(rem));
    let handle_band = t.spacing.s6.to_pixels(rem);
    let report = sheet.on_open_change.clone();
    let close: super::modal::CloseFn = Rc::new(move |window: &mut Window, cx: &mut App| {
        if let Some(f) = &report {
            f(false, window, cx);
        }
    });

    // The grab bar: a pill on a band that fades the content under it.
    let handle = handle_pan.area(
        div()
            .id(ElementId::Name(format!("{}-handle", sheet.id).into()))
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .h(handle_band)
            .flex()
            .items_center()
            .justify_center()
            .cursor_grab()
            .bg(gpui::linear_gradient(
                180.,
                gpui::linear_color_stop(t.color.background_surface, 0.6),
                gpui::linear_color_stop(faded(t.color.background_surface, 0.), 1.),
            ))
            .on_mouse_down(MouseButton::Left, |_, window, cx| {
                // The handle has no focus action: a press keeps focus where it is.
                window.prevent_default();
                cx.stop_propagation();
            })
            .child(
                div()
                    .w(t.size.element_lg)
                    .h(t.spacing.s1)
                    .rounded_full()
                    .bg(t.color.border),
            ),
    );
    let mut handle = div().absolute().top_0().left_0().right_0().child(handle);
    theme.refine(handle.style(), "bottom-sheet-handle", &[], None);

    // The scrolling body and its content box (FR3, FR4).
    let block_start = padding.max(handle_band);
    let published = crate::layout::ContainerPadding {
        inline_start: padding,
        inline_end: padding,
        // The band under the grab bar is not published: only the inset past it.
        block_start: (padding - handle_band).max(px(0.)),
        block_end: padding,
    };
    let inset = gestures_view.scroll_preservation_inset as f32;
    let focus_scope = modal.read(cx).focus.clone();
    let host_key = key.clone();
    let content = div()
        .pt(block_start)
        .px(padding)
        .pb(padding + px(inset))
        .child(provide(
            SheetHost {
                key: host_key,
                open,
            },
            provide(
                SheetEscape(true),
                provide(
                    focus_scope,
                    crate::layout::padding_container(
                        published,
                        Some(crate::layout::LayoutContainerKind::BottomSheet),
                        crate::layout::host_children(
                            sheet.children,
                            published,
                            crate::layout::ContainerCorners {
                                start_start: t.radius.container.to_pixels(rem),
                                start_end: t.radius.container.to_pixels(rem),
                                ..Default::default()
                            },
                        ),
                        cx,
                    ),
                ),
            ),
        ));
    let measure = {
        let cell = content_cell.clone();
        gpui::canvas(
            move |bounds, _, _| cell.set(Some(f32::from(bounds.size.height))),
            |_, _, _, _| {},
        )
        .absolute()
        .top_0()
        .left_0()
        .w_full()
        .h_full()
    };
    let content = scroll_area.content(div().relative().child(content).child(measure));
    let body = {
        let (press_state, press_handle) = (state.clone(), scroll.clone());
        // The body's bounds, and the body as one of the window's native scrollers.
        let probe = {
            let (gesture, scroll, body_bounds) =
                (gesture.clone(), scroll.clone(), body_bounds.clone());
            let hitbox_cell = state.read(cx).body_hitbox.clone();
            gpui::canvas(
                move |bounds, window, _| {
                    body_bounds.set(Some(bounds));
                    *hitbox_cell.borrow_mut() =
                        Some(window.insert_hitbox(bounds, gpui::HitboxBehavior::Normal));
                    if let Some(key) = body_key {
                        let mut scroller = NativeScroller::new(key, bounds, scroll.clone());
                        scroller.scrolls_x = false;
                        scroller.stops_x = true;
                        scroller.stops_y = true;
                        gesture.register_native_scroller(scroller);
                    }
                },
                |_, _, _, _| {},
            )
            .absolute()
            .top_0()
            .left_0()
            .size_full()
        };
        div()
            .relative()
            .flex()
            .flex_col()
            .flex_grow(1.)
            .min_h_0()
            .w_full()
            .child(
                scroll_area
                    .viewport(
                        div()
                            .flex_grow(1.)
                            .min_h_0()
                            .w_full()
                            .bg(t.color.background_surface),
                    )
                    .on_mouse_down(MouseButton::Left, move |event, window, cx| {
                        on_body_mouse(&press_state, &press_handle, event.position, 0, window, cx);
                    })
                    .child(content),
            )
            .child(probe)
    };
    // The raw fingers, for the body's scroll handoff and a pinch; a mouse armed on
    // the body follows the pointer anywhere.
    let raw = {
        let (state, gesture, scroll, surface) = (
            state.clone(),
            gesture.clone(),
            scroll.clone(),
            surface_focus.clone(),
        );
        gpui::canvas(
            |_, _, _| {},
            move |_, _, window, cx| {
                let (touch_state, gesture, touch_scroll, surface) = (
                    state.clone(),
                    gesture.clone(),
                    scroll.clone(),
                    surface.clone(),
                );
                let (scroll_state, scroll_handle, scroll_surface) =
                    (touch_state.clone(), touch_scroll.clone(), surface.clone());
                // Who acts on the sheets (upstream `listenForPresses`): a press, a
                // finger's touch-down, or a key with what has focus.
                window.on_mouse_event(|event: &gpui::MouseDownEvent, phase, window, cx| {
                    if phase == gpui::DispatchPhase::Capture {
                        stack::record_press(stack::Actor::At(event.position), window, cx);
                    }
                });
                window.on_key_event(|_: &gpui::KeyDownEvent, phase, window, cx| {
                    if phase == gpui::DispatchPhase::Capture {
                        let actor = window
                            .focused(cx)
                            .map_or(stack::Actor::Nobody, stack::Actor::Focus);
                        stack::record_press(actor, window, cx);
                    }
                });
                window.on_touch_event(move |event, window, cx| {
                    if event.phase == gpui::TouchPhase::Started {
                        stack::record_press(stack::Actor::At(event.position), window, cx);
                    }
                    on_raw_touch(
                        &touch_state,
                        &gesture,
                        &touch_scroll,
                        &surface,
                        event,
                        window,
                        cx,
                    )
                });
                // A finger's scroll over the body, seen before the content scrolls it.
                let hitbox = state.read(cx).body_hitbox.clone();
                window.on_mouse_event(move |event: &gpui::ScrollEvent, phase, window, cx| {
                    if phase == gpui::DispatchPhase::Capture
                        && event.source == gpui::ScrollSource::Touch
                        && hitbox
                            .borrow()
                            .as_ref()
                            .is_some_and(|h| h.is_hovered_at(event.position, window))
                    {
                        on_body_scroll(&scroll_state, &scroll_handle, &scroll_surface, window, cx);
                    }
                });
                if state.read(cx).body_mouse.is_some() {
                    let (moving, move_scroll) = (state.clone(), scroll.clone());
                    window.on_mouse_event(
                        move |event: &gpui::MouseMoveEvent, phase, window, cx| {
                            if phase == gpui::DispatchPhase::Capture
                                && event.pressed_button.is_some()
                            {
                                on_body_mouse(&moving, &move_scroll, event.position, 1, window, cx);
                            }
                        },
                    );
                    let (lifting, lift_scroll) = (state.clone(), scroll.clone());
                    window.on_mouse_event(move |event: &gpui::MouseUpEvent, phase, window, cx| {
                        if phase == gpui::DispatchPhase::Capture {
                            on_body_mouse(&lifting, &lift_scroll, event.position, 2, window, cx);
                        }
                    });
                }
            },
        )
        .absolute()
        .size_0()
    };

    let mut panel = div()
        .id(ElementId::Name(format!("{}-sheet", sheet.id).into()))
        .track_focus(&surface_focus)
        .occlude()
        .relative()
        .flex()
        .flex_col()
        .min_h_0()
        .w_full()
        .max_w(px(640.))
        .overflow_hidden()
        .bg(t.color.background_surface)
        .text_color(t.color.text_primary)
        .font_stack(&t.font.family.body)
        .text_style(&t.text.body)
        .border_t_1()
        .border_l_1()
        .border_r_1()
        .border_color(t.color.border)
        .rounded_t(t.radius.container)
        .shadow_token(&t.shadow.high)
        .pb(px(reserve))
        .mb(px(-reserve))
        .role(if sheet.purpose == DialogPurpose::Required {
            Role::AlertDialog
        } else {
            Role::Dialog
        })
        .aria_label(sheet.label.clone())
        .aria_modal(sheet.scrim && open)
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation());
    panel = match (sheet.height, resized_height) {
        (_, Some(h)) => panel.h(h),
        (SheetHeight::Hug, None) => panel.max_h(px(full)),
        (_, None) => panel.h(px(full)),
    };
    if sheet.scrim && open && view.is_top {
        panel = panel.tab_trap();
    }
    if translation.abs() > 0.01 {
        panel = panel.translate(px(0.), px(translation));
    } else if !measured && matches!(motion, PanelMotion::Entering(_)) && !reduced {
        panel = panel.translate(px(0.), relative(1.));
    }
    if opacity < 0.999 {
        panel = panel.opacity(opacity.clamp(0., 1.));
    }
    theme.refine(panel.style(), "bottom-sheet", &[], None);
    panel.style().refine(&sheet.style);
    let panel = panel
        .children(super::modal::probes(&modal, cx))
        .child(body)
        .child(handle)
        .child(raw)
        .child({
            let cell = state.read(cx).panel_bounds.clone();
            gpui::canvas(
                move |bounds, window, _| {
                    cell.set(Some(window.element_transform().apply_bounds(bounds)))
                },
                |_, _, _, _| {},
            )
            .absolute()
            .top_0()
            .left_0()
            .size_full()
        });

    // The positioner: centred at the bottom; a covered sheet recedes as one, shrinking
    // a step about its top edge and lifting.
    let exiting = !open;
    let inert = view.blocked
        || retained.is_some()
        || (exiting && !surface_focus.contains_focused(window, cx));
    let mut positioner = div()
        .absolute()
        .top_0()
        .left_0()
        .w(viewport.width)
        .h(viewport.height)
        .flex()
        .justify_center()
        .items_end();
    if inert {
        positioner = positioner.inert(true);
    }
    if recede > 0. && !reduced {
        positioner = positioner
            .scale(1. - 0.04 * recede)
            .translate(px(0.), -(t.spacing.s2.to_pixels(rem) * recede));
    } else if recede > 0. {
        positioner = positioner
            .scale(1. - 0.04 * steps)
            .translate(px(0.), -(t.spacing.s2.to_pixels(rem) * steps));
    }
    let positioner = positioner.child(panel).child(edge_tint(&t, viewport));

    let info = sheet.purpose == DialogPurpose::Info;
    #[cfg(test)]
    probe::record(
        &key,
        probe::SheetFrame {
            offset,
            translation,
            layout: layout_offset,
            height: resized_height.map(f32::from),
            full,
            reserve,
            scrim,
            opacity,
            open,
            depth: view.depth,
            recede,
            owns_scrim: view.owns_scrim,
            is_stacked: view.is_stacked,
            blocked: view.blocked,
            inert,
            retained: retained.is_some(),
            dragging,
            motion: format!("{motion:?}"),
            owner: token.upgrade().map_or(0, |t| Rc::as_ptr(&t) as usize),
        },
    );
    let holder = if sheet.scrim {
        let press: Option<super::modal::CloseFn> =
            (info && open && view.is_top).then(|| close.clone());
        // One dim for the stack: the bottom open sheet's (FR6).
        let dim = if view.owns_scrim || !view.is_stacked {
            faded(t.color.overlay, scrim.clamp(0., 1.))
        } else {
            faded(t.color.overlay, 0.)
        };
        super::modal::backdrop(&sheet.id, Some(dim), press, window)
            .child(positioner)
            .into_any_element()
    } else {
        div()
            .absolute()
            .top_0()
            .left_0()
            .w(viewport.width)
            .h(viewport.height)
            .child(positioner)
            .into_any_element()
    };
    let escape =
        if open && sheet.purpose != DialogPurpose::Required && escape_ctx && retained.is_none() {
            EscapeBehavior::Close
        } else {
            EscapeBehavior::Block
        };
    let close_layer = close.clone();
    let on_cancel = sheet.on_cancel.clone();
    let mut layer = layer(holder, move |window, cx| {
        if let Some(cancel) = &on_cancel {
            cancel(window, cx);
        }
        close_layer(window, cx)
    })
    .anchor(point(px(0.), px(0.)), gpui::Anchor::TopLeft)
    .light_dismiss(false)
    .escape(escape);
    if sheet.scrim {
        layer = layer.modal().min_depth(MODAL_DEPTH + 10 * view.rank);
    } else {
        layer = layer.min_depth(1 + view.rank);
    }
    div()
        .id(sheet.id.clone())
        .display_contents()
        .child(layer)
        .into_any_element()
}

/// The recede's steps now, from `from` toward `to` since `since`.
fn recede_at(
    (from, to, since): (f32, f32, Option<Instant>),
    now: Instant,
    over: Duration,
    reduced: bool,
) -> f32 {
    let Some(since) = since else { return to };
    if reduced || over.is_zero() {
        return to;
    }
    let p = (now.saturating_duration_since(since).as_secs_f32() / over.as_secs_f32()).min(1.);
    from + (to - from) * p
}

/// A hidden sheet: it leaves the stack, and focus returns where the stack says.
fn hide(
    id: &ElementId,
    key: &str,
    state: &Entity<SheetState>,
    surface: &FocusHandle,
    host: &GestureHost,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let (presented, body_key) = {
        let s = state.read(cx);
        (s.presented, s.body_key)
    };
    #[cfg(test)]
    probe::forget(key);
    if presented {
        state.update(cx, |s, _| {
            s.presented = false;
            s.exit = None;
            s.settle = None;
            s.body_touch = None;
            s.body_mouse = None;
            s.gestures.reset();
            s.engine.set(MotionValues::new().y(0.).opacity(1.));
            s.scrim = 1.;
            s.recede = (0., 0., None);
        });
        if let Some(key) = body_key {
            host.scrollers().unregister(key);
        }
        // When the exit ends, focus returns as the stack says (FR9, FR11).
        let _ = surface;
        match stack::leave(key, window, cx) {
            stack::FocusReturn::To(target) => {
                window.defer(cx, move |window, cx| target.focus(window, cx));
            }
            stack::FocusReturn::Named(name) => {
                astryx_primitives::request_focus(&name, cx);
                window.refresh();
            }
            stack::FocusReturn::Keep => {}
        }
    }
    div().id(id.clone()).display_contents().into_any_element()
}

/// iOS Safari colours its toolbar by sampling the page's fixed elements at the
/// window's bottom edge (upstream `BottomSheetEdgeTint`): a 12 px strip in the
/// surface's colour, invisible and taking no pointer. In GPUI's browser build the
/// page is one canvas, which Safari samples as a whole, so the strip only keeps the
/// anatomy; natively there is no toolbar.
fn edge_tint(t: &astryx_theme::Tokens, viewport: gpui::Size<Pixels>) -> impl IntoElement {
    div()
        .absolute()
        .left_0()
        .top(viewport.height - px(12.))
        .w(viewport.width)
        .h(px(12.))
        .bg(faded(t.color.background_surface, 0.))
}

/// What each sheet drew last frame, for tests.
#[cfg(test)]
pub(crate) mod probe {
    use std::cell::RefCell;
    use std::collections::HashMap;

    #[derive(Clone, Debug, Default)]
    pub(crate) struct SheetFrame {
        /// The sheet's offset from fully open, px.
        pub offset: f32,
        /// The panel's translation, px.
        pub translation: f32,
        pub layout: f32,
        pub height: Option<f32>,
        pub full: f32,
        pub reserve: f32,
        /// The scrim's opacity (1 = full), before the stack's ownership.
        pub scrim: f32,
        pub opacity: f32,
        pub open: bool,
        pub depth: usize,
        pub recede: f32,
        pub owns_scrim: bool,
        pub is_stacked: bool,
        pub blocked: bool,
        pub inert: bool,
        pub retained: bool,
        pub dragging: bool,
        pub motion: String,
        /// The sheet state that drew it.
        pub owner: usize,
    }

    thread_local! {
        static FRAMES: RefCell<HashMap<String, SheetFrame>> = RefCell::default();
    }

    pub(crate) fn record(key: &str, frame: SheetFrame) {
        FRAMES.with(|f| f.borrow_mut().insert(key.to_string(), frame));
    }

    pub(crate) fn forget(key: &str) {
        FRAMES.with(|f| f.borrow_mut().remove(key));
    }

    /// Forgets the frame only if `owner` drew it (a state dropped after its
    /// successor drew).
    pub(crate) fn forget_owned(key: &str, owner: usize) {
        FRAMES.with(|f| {
            let mut frames = f.borrow_mut();
            if frames.get(key).is_some_and(|frame| frame.owner == owner) {
                frames.remove(key);
            }
        });
    }

    /// The last frame of the sheet whose id is the name `name`, `None` once hidden.
    pub(crate) fn frame(name: &str) -> Option<SheetFrame> {
        let key = format!("{:?}", gpui::ElementId::Name(name.to_string().into()));
        FRAMES.with(|f| f.borrow().get(&key).cloned())
    }
}
