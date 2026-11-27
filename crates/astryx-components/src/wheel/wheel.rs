//! One picker column, upstream `Wheel`: under a finger it is
//! UIPickerView's column (`PickerWheel.tsx`), with the pan, the coast, the
//! snap and the spring turns of `wheelPhysics`; under a fine pointer it is
//! the snapping scroll column (`ScrollWheel`), which a mouse wheel, a
//! trackpad or a mouse drag scrolls and which commits the row it rests on.
//!
//! Both draw a strip of rows moved by an offset: row `k` is under the band
//! at an offset of `k` rows. The touch column loops when asked, drawing the
//! copies a motion passes. Either way a rest on a disabled row travels on to
//! the nearest enabled one (DateInput FR13), a changed value turns the
//! wheel rather than jump (FR14), keys step through enabled rows in time
//! order (FR17), and no row is marked while the touch column moves (FR18).

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use astryx_primitives::gestures::VelocityTracker;
use astryx_primitives::gestures::haptics::{HapticFeedbackKind, can_vibrate, play_haptic};
use astryx_primitives::motion::uikit::{PAN_HYSTERESIS, picker, springs, timing};
use astryx_primitives::{is_focus_visible, prefers_reduced_motion};
use astryx_theme::{ThemeStyled, current};
use gpui::{
    App, Bounds, DispatchPhase, ElementId, Entity, FocusHandle, Hsla, IntoElement, KeyDownEvent,
    MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, RenderOnce, Role,
    ScrollDelta, ScrollEvent, ScrollSource, SharedString, Task, TouchDragEvent, TouchPhase, Window,
    div, prelude::*, px,
};
use web_time::Instant;

use super::options::{
    PAGE_ROWS, PANE_HEIGHT, ROW_HEIGHT, WheelOption, can_spin_to, end_index, looped_step, modulo,
    nearest_enabled_index, nearest_loop_copy, resting_loop_step, stepped_index,
};
use super::physics::{
    PlanKind, WheelCurve, WheelLayout, bounce_curve, drag_offset, finger_offset, nearest_index,
    plan_release, scale_for,
};
use super::physics::{reaches_row, tick_times};
use crate::focus::focus_ring;

/// A row's value committed by a rest, a key or a click.
pub(crate) type ChangeFn = Rc<dyn Fn(i64, &mut Window, &mut App)>;
/// A spin's end: whether its row became the committed one.
pub(crate) type SpinDone = Box<dyn FnOnce(bool, &mut Window, &mut App)>;

/// The band's top inside the column, px.
const BAND_TOP: f64 = (PANE_HEIGHT - ROW_HEIGHT) / 2.;
/// A mouse moving less than this stays a click on the column (`DRAG_SLOP`).
const DRAG_SLOP: f64 = 4.;
/// A column quiet this long is at rest (`SCROLL_QUIET_MS`).
const SCROLL_QUIET_MS: u64 = 220;
/// A mouse wheel notch's travel on the column, px (a browser's 100 px per
/// three-line notch).
const LINE_PX: f32 = 100. / 3.;
/// A trackpad with no lift phase lifts after this pause (the wheel
/// segmenter's momentum gap).
const TRACKPAD_LIFT_MS: u64 = 50;
/// The rows' opacity at the touch column's edges (`FADE_FLOOR`).
const FADE_FLOOR: f32 = 0.7;
/// The scrolling column's tip at its edges: rotateX and scale.
const TIP_DEGREES: f32 = 52.;
const TIP_SCALE: f32 = 0.86;
/// The scrolling column's snap after a scroll, seconds.
const COLUMN_SNAP_SECONDS: f64 = 0.2;

/// Turns a wheel from outside, upstream `WheelHandle.spinTo`.
#[derive(Clone, Default)]
pub(crate) struct WheelHandle(Rc<RefCell<Option<Entity<WheelState>>>>);

impl WheelHandle {
    /// Turns onto row `index` and commits it as a rest would; `done` hears
    /// whether that row became the committed one. A disabled or missing row
    /// answers false at once, and nothing turns.
    pub fn spin_to(&self, index: usize, window: &mut Window, cx: &mut App, done: SpinDone) {
        let state = self.0.borrow().clone();
        match state {
            Some(state) => spin_to(&state, index, done, window, cx),
            None => done(false, window, cx),
        }
    }
}

/// One picker column.
#[derive(IntoElement)]
pub(crate) struct Wheel {
    id: ElementId,
    label: SharedString,
    options: Rc<[WheelOption]>,
    value: i64,
    on_change: ChangeFn,
    active: bool,
    looping: bool,
    handle: Option<WheelHandle>,
}

impl Wheel {
    pub fn new(
        id: impl Into<ElementId>,
        label: impl Into<SharedString>,
        options: Rc<[WheelOption]>,
        value: i64,
        on_change: impl Fn(i64, &mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            options,
            value,
            on_change: Rc::new(on_change),
            active: true,
            looping: false,
            handle: None,
        }
    }

    /// Upstream `isActive`: false while hidden.
    pub fn active(mut self, active: bool) -> Self {
        self.active = active;
        self
    }

    /// Upstream `isLooping`: under a finger the rows repeat without end.
    pub fn looping(mut self, looping: bool) -> Self {
        self.looping = looping;
        self
    }

    /// Upstream `ref`: programmatic turns.
    pub fn handle(mut self, handle: WheelHandle) -> Self {
        self.handle = Some(handle);
        self
    }
}

#[derive(Clone)]
struct Latest {
    options: Rc<[WheelOption]>,
    value: i64,
    on_change: ChangeFn,
    layout: WheelLayout,
    touch: bool,
}

impl Latest {
    fn selected(&self) -> i64 {
        self.options
            .iter()
            .position(|o| o.value == self.value)
            .unwrap_or(0) as i64
    }

    fn option_at(&self, position: i64) -> Option<&WheelOption> {
        let count = self.options.len() as i64;
        if self.layout.looping && count > 0 {
            self.options.get(modulo(position, count) as usize)
        } else if (0..count).contains(&position) {
            self.options.get(position as usize)
        } else {
            None
        }
    }

    fn enabled_at(&self, position: i64) -> bool {
        self.option_at(position).is_some_and(|o| !o.disabled)
    }
}

/// What happens when a motion arrives.
enum End {
    None,
    /// A rest: commit, or turn on to an enabled row.
    Arrive,
    /// A park's turn: settle onto the wheel's own rows.
    Park(u64),
    /// A spin: arrive, then answer.
    Spin(SpinDone),
    /// The column's snap or a correction: settle again.
    ColumnSettle,
}

struct Motion {
    id: u64,
    curve: WheelCurve,
    start: Instant,
    /// Reduce Motion's cross-fade, seconds: the rows land at its middle.
    fade: Option<f64>,
    end: End,
    _timer: Task<()>,
    /// The haptic ticks the motion plays as rows reach the band.
    _ticks: Vec<Task<()>>,
}

#[derive(Clone, Copy)]
enum Drag {
    /// A finger, a pen or a mouse on the touch column: `start` is the finger
    /// offset at the pan's start, `origin` where the pointer was then.
    Pan { start: f64, origin: f64 },
    /// A trackpad's two-finger pan on the touch column.
    Trackpad { start: f64, travel: f64 },
    /// A mouse dragging the scrolling column.
    Column { origin: f64, from: f64 },
}

#[derive(Clone, Copy)]
struct Press {
    y: f64,
    /// A finger's press, claimed as a touch drag.
    touch: bool,
}

/// One column's state.
pub(crate) struct WheelState {
    focus: FocusHandle,
    latest: Option<Latest>,
    /// The offset at rest, or where a drag or the last motion left it.
    offset: f64,
    motion: Option<Motion>,
    drag: Option<Drag>,
    press: Option<Press>,
    /// The committed row's position on the strip (a copy, when looping).
    committed: i64,
    parked: bool,
    /// The park inputs last seen: the selected row and the layout.
    park_key: Option<(i64, usize, bool)>,
    park_turn: Option<u64>,
    /// A finger landed on a moving wheel and stopped it.
    caught: bool,
    tracker: VelocityTracker,
    next_id: u64,
    quiet: Option<Task<()>>,
    lift: Option<Task<()>>,
    /// A trackpad told its lift: its momentum is the platform's, not ours.
    phased_lift: bool,
    /// A mouse drag on the column: the click it ends with is not a row's.
    dragged: bool,
    bounds: Option<Bounds<Pixels>>,
    /// The last selection tick, for the picker's minimum interval.
    last_tick: Option<Instant>,
}

impl WheelState {
    fn new(cx: &mut App) -> Self {
        Self {
            focus: cx.focus_handle(),
            latest: None,
            offset: 0.,
            motion: None,
            drag: None,
            press: None,
            committed: 0,
            parked: false,
            park_key: None,
            park_turn: None,
            caught: false,
            tracker: VelocityTracker::default(),
            next_id: 0,
            quiet: None,
            lift: None,
            phased_lift: false,
            dragged: false,
            bounds: None,
            last_tick: None,
        }
    }

    fn elapsed(motion: &Motion, now: Instant) -> f64 {
        now.saturating_duration_since(motion.start).as_secs_f64()
    }

    /// The offset on screen at `now`.
    fn shown(&self, now: Instant) -> f64 {
        match &self.motion {
            Some(m) => {
                let t = Self::elapsed(m, now);
                match m.fade {
                    Some(fade) if t < fade / 2. => m.curve.value(0.),
                    Some(_) => m.curve.to(),
                    None => m.curve.value(t),
                }
            }
            None => self.offset,
        }
    }

    /// Where the wheel will rest: the motion's end, or the offset now.
    fn target(&self) -> f64 {
        self.motion.as_ref().map_or(self.offset, |m| m.curve.to())
    }

    fn latest(&self) -> Latest {
        self.latest
            .clone()
            .expect("a wheel renders before it moves")
    }
}

fn now(cx: &App) -> Instant {
    cx.background_executor().now()
}

fn ms(now: Instant, cx: &App) -> f64 {
    let _ = cx;
    // Milliseconds on one clock for the velocity tracker.
    thread_local! {
        static ORIGIN: std::cell::Cell<Option<Instant>> = const { std::cell::Cell::new(None) };
    }
    ORIGIN.with(|origin| {
        let start = origin.get().unwrap_or_else(|| {
            origin.set(Some(now));
            now
        });
        now.saturating_duration_since(start).as_secs_f64() * 1000.
    })
}

/// Stops the motion where it shows now. A spin stopped short answers false.
fn stop(state: &Entity<WheelState>, window: &mut Window, cx: &mut App) -> f64 {
    let at = now(cx);
    let (offset, end) = state.update(cx, |s, _| {
        let offset = s.shown(at);
        let end = s.motion.take().map(|m| m.end);
        s.offset = offset;
        (offset, end)
    });
    if let Some(End::Spin(done)) = end {
        done(false, window, cx);
    }
    offset
}

/// A selection tick (upstream `wheelStrip`'s `tick`): UIPickerView's haptic as a row
/// reaches the band, at most one per the picker's interval.
fn tick(state: &Entity<WheelState>, cx: &mut App) {
    let at = now(cx);
    let due = state.read(cx).last_tick.is_none_or(|last| {
        at.saturating_duration_since(last).as_secs_f64() * 1000.
            >= picker::SELECTION_TICK_INTERVAL_MS
    });
    if due {
        state.update(cx, |s, _| s.last_tick = Some(at));
        play_haptic(HapticFeedbackKind::Selection);
    }
}

/// Plays `curve` from where the wheel shows. Under Reduce Motion a
/// transition cross-fades after a touch or on a coarse pointer and lands at
/// once otherwise; momentum (a coast, its bounce) always plays.
fn play(
    state: &Entity<WheelState>,
    curve: WheelCurve,
    end: End,
    momentum: bool,
    touch_release: bool,
    window: &mut Window,
    cx: &mut App,
) {
    stop(state, window, cx);
    let reduced = !momentum && prefers_reduced_motion(cx);
    let fade = (reduced && (touch_release || window.last_input_was_touch()))
        .then_some(timing::REDUCED_MOTION_CROSS_FADE_MS / 1000.);
    let duration = fade.unwrap_or_else(|| if reduced { 0. } else { curve.duration() });
    if duration <= 0. {
        state.update(cx, |s, _| s.offset = curve.to());
        arrive_with(state, end, curve.to(), window, cx);
        window.refresh();
        return;
    }
    let id = state.update(cx, |s, _| {
        s.next_id += 1;
        s.next_id
    });
    let timer = cx
        .background_executor()
        .timer(Duration::from_secs_f64(duration));
    let entity = state.clone();
    let task = window.spawn(cx, async move |cx| {
        timer.await;
        cx.update(|window, cx| finish(&entity, id, window, cx)).ok();
    });
    // A finger's release ticks through the rows it passes; a cross-fade has no travel.
    let ticks: Vec<Task<()>> = if touch_release && fade.is_none() && can_vibrate() {
        tick_times(&curve, ROW_HEIGHT)
            .into_iter()
            .map(|t| {
                let timer = cx.background_executor().timer(Duration::from_secs_f64(t));
                let entity = state.clone();
                window.spawn(cx, async move |cx| {
                    timer.await;
                    cx.update(|_, cx| tick(&entity, cx)).ok();
                })
            })
            .collect()
    } else {
        Vec::new()
    };
    let start = now(cx);
    state.update(cx, |s, _| {
        s.motion = Some(Motion {
            id,
            curve,
            start,
            fade,
            end,
            _timer: task,
            _ticks: ticks,
        });
    });
    window.refresh();
}

/// A motion's timer: it arrived.
fn finish(state: &Entity<WheelState>, id: u64, window: &mut Window, cx: &mut App) {
    let arrived = state.update(cx, |s, _| {
        if s.motion.as_ref().is_some_and(|m| m.id == id) {
            let motion = s.motion.take().expect("the motion");
            s.offset = motion.curve.to();
            Some((motion.end, s.offset))
        } else {
            None
        }
    });
    if let Some((end, offset)) = arrived {
        arrive_with(state, end, offset, window, cx);
        window.refresh();
    }
}

fn arrive_with(
    state: &Entity<WheelState>,
    end: End,
    offset: f64,
    window: &mut Window,
    cx: &mut App,
) {
    match end {
        End::None => {}
        End::Arrive => arrive(state, offset, window, cx),
        End::Park(turn) => {
            state.update(cx, |s, _| {
                if s.park_turn == Some(turn) {
                    s.park_turn = None;
                }
            });
            settle_loop(state, offset, cx);
        }
        End::Spin(done) => {
            arrive(state, offset, window, cx);
            done(true, window, cx);
        }
        End::ColumnSettle => column_settle(state, window, cx),
    }
}

/// On a loop, a rest on a copy moves onto the wheel's own row.
fn settle_loop(state: &Entity<WheelState>, offset: f64, cx: &mut App) -> f64 {
    state.update(cx, |s, _| {
        let latest = s.latest();
        let count = latest.options.len() as i64;
        if !latest.layout.looping || count == 0 {
            return offset;
        }
        let position = nearest_index(offset, &latest.layout);
        let shift = position - modulo(position, count);
        if shift != 0 && s.motion.is_none() {
            s.offset = offset - shift as f64 * ROW_HEIGHT;
            s.committed -= shift;
        }
        offset - shift as f64 * ROW_HEIGHT
    })
}

/// A turn from rest onto `to`: every turn springs (`springs.settle`).
fn turn_curve(from: f64, to: f64) -> WheelCurve {
    bounce_curve(from, 0., to, scale_for(ROW_HEIGHT), springs::SETTLE)
}

fn commit(latest: &Latest, option: Option<WheelOption>, window: &mut Window, cx: &mut App) {
    if let Some(option) = option
        && !option.disabled
        && option.value != latest.value
    {
        (latest.on_change)(option.value, window, cx);
    }
}

/// The touch column came to rest at `offset`: commit, or turn on to the
/// row the rest rule names (FR13).
fn arrive(state: &Entity<WheelState>, offset: f64, window: &mut Window, cx: &mut App) {
    let (latest, committed) = state.read_with(cx, |s, _| (s.latest(), s.committed));
    let count = latest.options.len() as i64;
    let position = nearest_index(offset, &latest.layout);
    let step = if latest.layout.looping && count > 0 {
        let committed = nearest_loop_copy(committed, position as f64, count);
        if position == committed {
            0
        } else {
            resting_loop_step(|at| latest.enabled_at(at), position, committed, count)
        }
    } else {
        let selected = latest.options.iter().position(|o| o.value == latest.value);
        if Some(position) == selected.map(|s| s as i64) {
            0
        } else {
            nearest_enabled_index(&latest.options, position, selected.unwrap_or(0) as i64)
                - position
        }
    };
    if step != 0 {
        let target = position + step;
        play(
            state,
            turn_curve(offset, target as f64 * ROW_HEIGHT),
            End::Arrive,
            false,
            false,
            window,
            cx,
        );
        return;
    }
    let option = latest.option_at(position).cloned();
    let settled = settle_loop(state, offset, cx);
    state.update(cx, |s, _| {
        s.committed = nearest_index(settled, &latest.layout)
    });
    commit(&latest, option, window, cx);
}

/// A release at `velocity` (px/s, rows moving up).
fn release(state: &Entity<WheelState>, velocity: f64, window: &mut Window, cx: &mut App) {
    let at = now(cx);
    let (offset, layout) = state.read_with(cx, |s, _| (s.shown(at), s.latest().layout));
    let plan = plan_release(offset, velocity, &layout);
    let momentum = matches!(plan.kind, PlanKind::Coast | PlanKind::Edge);
    play(state, plan.curve, End::Arrive, momentum, true, window, cx);
}

/// A key or click turns the touch column onto `position` and commits it at
/// once; on a loop the target's lap moves onto the wheel's own rows first.
fn turn_to_position(
    state: &Entity<WheelState>,
    position: i64,
    offset: f64,
    window: &mut Window,
    cx: &mut App,
) {
    let latest = state.read_with(cx, |s, _| s.latest());
    let count = latest.options.len() as i64;
    let option = latest.option_at(position).cloned();
    let shift = if latest.layout.looping && count > 0 {
        position - modulo(position, count)
    } else {
        0
    };
    let from = offset - shift as f64 * ROW_HEIGHT;
    let to = position - shift;
    state.update(cx, |s, _| {
        s.offset = from;
        s.committed = to;
    });
    play(
        state,
        turn_curve(from, to as f64 * ROW_HEIGHT),
        End::None,
        false,
        false,
        window,
        cx,
    );
    commit(&latest, option, window, cx);
}

/// The scrolling column scrolls to `index` (a browser's smooth scroll).
fn column_scroll(
    state: &Entity<WheelState>,
    index: i64,
    end: End,
    window: &mut Window,
    cx: &mut App,
) {
    let at = now(cx);
    let from = state.read_with(cx, |s, _| s.shown(at));
    let to = index as f64 * ROW_HEIGHT;
    let curve = if (from - to).abs() < 0.5 {
        WheelCurve::Rest(to)
    } else {
        WheelCurve::Ease {
            from,
            to,
            duration: picker::TAP_SPIN_MS / 1000.,
        }
    };
    play(state, curve, end, false, false, window, cx);
}

/// The scrolling column moves to a row and commits it at once.
fn column_move_to(
    state: &Entity<WheelState>,
    row: Option<usize>,
    window: &mut Window,
    cx: &mut App,
) {
    let latest = state.read_with(cx, |s, _| s.latest());
    let Some(row) = row else { return };
    let Some(option) = latest.options.get(row).cloned() else {
        return;
    };
    if option.disabled {
        return;
    }
    column_scroll(state, row as i64, End::None, window, cx);
    commit(&latest, Some(option), window, cx);
}

/// The scrolling column is at rest: snap onto a row, travel on past a
/// disabled one, then commit the row it shows (`useScrollSettle`).
fn column_settle(state: &Entity<WheelState>, window: &mut Window, cx: &mut App) {
    let (latest, offset, held) = state.update(cx, |s, _| {
        s.quiet = None;
        (s.latest(), s.offset, s.drag.is_some() || s.motion.is_some())
    });
    if held || latest.options.is_empty() {
        return;
    }
    let count = latest.options.len() as i64;
    let index = ((offset / ROW_HEIGHT).round() as i64).clamp(0, count - 1);
    if (offset - index as f64 * ROW_HEIGHT).abs() >= 0.5 {
        let to = index as f64 * ROW_HEIGHT;
        let curve = WheelCurve::Ease {
            from: offset,
            to,
            duration: COLUMN_SNAP_SECONDS,
        };
        play(state, curve, End::ColumnSettle, false, false, window, cx);
        return;
    }
    let selected = latest.selected();
    let rest = if index == selected {
        index
    } else {
        nearest_enabled_index(&latest.options, index, selected)
    };
    if rest != index {
        column_scroll(state, rest, End::ColumnSettle, window, cx);
        return;
    }
    let option = latest.options.get(index as usize).cloned();
    commit(&latest, option, window, cx);
}

/// Waits for the scrolling column to go quiet, then settles it.
fn arm_quiet(state: &Entity<WheelState>, window: &mut Window, cx: &mut App) {
    let timer = cx
        .background_executor()
        .timer(Duration::from_millis(SCROLL_QUIET_MS));
    let entity = state.clone();
    let task = window.spawn(cx, async move |cx| {
        timer.await;
        cx.update(|window, cx| column_settle(&entity, window, cx))
            .ok();
    });
    state.update(cx, |s, _| s.quiet = Some(task));
}

/// An Arrow or Page key: a step of `delta` rows.
fn move_by(state: &Entity<WheelState>, delta: i64, window: &mut Window, cx: &mut App) {
    let (latest, target) = state.read_with(cx, |s, _| (s.latest(), s.target()));
    if latest.options.is_empty() || delta == 0 {
        return;
    }
    if !latest.touch {
        // The column steps from the row it is heading to (its active row).
        let active =
            ((target / ROW_HEIGHT).round() as i64).clamp(0, latest.options.len() as i64 - 1);
        let row = stepped_index(&latest.options, active as usize, delta);
        column_move_to(state, row, window, cx);
        return;
    }
    // Keys step from the row the wheel heads to: two quick presses, two rows.
    let heading = nearest_index(target, &latest.layout);
    if latest.layout.looping {
        let count = latest.options.len() as i64;
        let Some(travel) = looped_step(|at| latest.enabled_at(at), heading, delta, count) else {
            return;
        };
        let offset = stop(state, window, cx);
        turn_to_position(state, heading + travel, offset, window, cx);
        return;
    }
    let Some(row) = stepped_index(&latest.options, heading.max(0) as usize, delta) else {
        return;
    };
    let offset = stop(state, window, cx);
    turn_to_position(state, row as i64, offset, window, cx);
}

/// The lap of `row` that holds `position`.
fn lap_position(row: i64, position: i64, layout: &WheelLayout) -> i64 {
    let count = layout.row_count as i64;
    if !layout.looping || count == 0 {
        return row;
    }
    row + position.div_euclid(count) * count
}

/// Home or End: the first or last enabled row, within the committed lap.
fn to_end(state: &Entity<WheelState>, last: bool, window: &mut Window, cx: &mut App) {
    let (latest, committed) = state.read_with(cx, |s, _| (s.latest(), s.committed));
    let row = end_index(&latest.options, last);
    if !latest.touch {
        column_move_to(state, row, window, cx);
        return;
    }
    let Some(row) = row else { return };
    let offset = stop(state, window, cx);
    turn_to_position(
        state,
        lap_position(row as i64, committed, &latest.layout),
        offset,
        window,
        cx,
    );
}

/// A click on row `row` (assistive technology, a keyboard, a mouse).
fn activate(state: &Entity<WheelState>, row: usize, window: &mut Window, cx: &mut App) {
    let (latest, committed) = state.read_with(cx, |s, _| (s.latest(), s.committed));
    if !latest.touch {
        column_move_to(state, Some(row), window, cx);
        return;
    }
    let target = lap_position(row as i64, committed, &latest.layout);
    if !latest.enabled_at(target) {
        return;
    }
    let offset = stop(state, window, cx);
    turn_to_position(state, target, offset, window, cx);
}

fn spin_to(
    state: &Entity<WheelState>,
    index: usize,
    done: SpinDone,
    window: &mut Window,
    cx: &mut App,
) {
    let latest = state.read_with(cx, |s, _| s.latest());
    if !can_spin_to(&latest.options, index) {
        done(false, window, cx);
        return;
    }
    if !latest.touch {
        let option = latest.options.get(index).cloned();
        commit(&latest, option, window, cx);
        done(true, window, cx);
        return;
    }
    let offset = stop(state, window, cx);
    let target = nearest_position(index as i64, offset, &latest.layout);
    state.update(cx, |s, _| s.committed = target);
    play(
        state,
        turn_curve(offset, target as f64 * ROW_HEIGHT),
        End::Spin(done),
        false,
        false,
        window,
        cx,
    );
}

/// The copy of `row` nearest `offset` on a loop.
fn nearest_position(row: i64, offset: f64, layout: &WheelLayout) -> i64 {
    if !layout.looping || layout.row_count == 0 {
        return row;
    }
    nearest_loop_copy(row, offset / layout.row_height, layout.row_count as i64)
}

/// A press lands on the column.
fn press(state: &Entity<WheelState>, y: f64, touch: bool, window: &mut Window, cx: &mut App) {
    let latest_touch = state.read_with(cx, |s, _| s.latest().touch);
    let moving = state.read_with(cx, |s, _| s.motion.is_some());
    if latest_touch {
        // A finger landing on a moving wheel stops it where it shows.
        if moving {
            stop(state, window, cx);
        }
    }
    let t = ms(now(cx), cx);
    state.update(cx, |s, _| {
        s.caught = latest_touch && moving;
        s.press = Some(Press { y, touch });
        s.drag = None;
        s.dragged = false;
        s.tracker.reset();
        s.tracker.add(0., y, t);
    });
}

/// The pointer moved while pressed.
fn pointer_moved(state: &Entity<WheelState>, y: f64, window: &mut Window, cx: &mut App) {
    let t = ms(now(cx), cx);
    let (latest, press, drag) = state.update(cx, |s, _| {
        s.tracker.add(0., y, t);
        (s.latest(), s.press, s.drag)
    });
    let Some(press) = press else { return };
    let dy = y - press.y;
    if !latest.touch {
        let drag = match drag {
            Some(drag) => drag,
            None => {
                if dy.abs() < DRAG_SLOP {
                    return;
                }
                let from = stop(state, window, cx);
                state.update(cx, |s, _| s.quiet = None);
                Drag::Column {
                    origin: press.y,
                    from,
                }
            }
        };
        if let Drag::Column { origin, from } = drag {
            let max = latest.options.len().saturating_sub(1) as f64 * ROW_HEIGHT;
            state.update(cx, |s, _| {
                s.drag = Some(drag);
                s.offset = (from - (y - origin)).clamp(0., max);
            });
            window.refresh();
        }
        return;
    }
    let drag = match drag {
        Some(drag) => drag,
        None => {
            if dy.abs() < PAN_HYSTERESIS {
                return;
            }
            // The pan begins; the travel spent on the threshold is not applied.
            let shown = stop(state, window, cx);
            Drag::Pan {
                start: finger_offset(shown, &latest.layout),
                origin: y,
            }
        }
    };
    if let Drag::Pan { start, origin } = drag {
        let next = drag_offset(start - (y - origin), &latest.layout);
        let previous = state.update(cx, |s, _| {
            let previous = s.drag.is_some().then_some(s.offset);
            s.drag = Some(drag);
            s.offset = next;
            previous
        });
        // A drag frame that brings a row onto the band ticks.
        if previous.is_some_and(|p| reaches_row(p, next, ROW_HEIGHT)) && can_vibrate() {
            tick(state, cx);
        }
        window.refresh();
    }
}

/// The pointer lifted.
fn pointer_up(state: &Entity<WheelState>, y: f64, window: &mut Window, cx: &mut App) {
    let t = ms(now(cx), cx);
    let (latest, press, drag, caught, velocity) = state.update(cx, |s, _| {
        let press = s.press.take();
        let drag = s.drag.take();
        let velocity = -s.tracker.scroll_velocity(t).y;
        (s.latest(), press, drag, s.caught, velocity)
    });
    if press.is_none() {
        return;
    }
    if !latest.touch {
        if drag.is_some() {
            state.update(cx, |s, _| s.dragged = true);
            column_settle(state, window, cx);
        }
        return;
    }
    match drag {
        Some(_) => release(state, velocity, window, cx),
        None if caught => release(state, 0., window, cx),
        None => tap(state, y, window, cx),
    }
}

/// A tap on the touch column turns it onto the row under the finger.
fn tap(state: &Entity<WheelState>, y: f64, window: &mut Window, cx: &mut App) {
    let at = now(cx);
    let (latest, offset, bounds) = state.read_with(cx, |s, _| (s.latest(), s.shown(at), s.bounds));
    let Some(bounds) = bounds else { return };
    let center = f64::from(bounds.top()) + f64::from(bounds.size.height) / 2.;
    let position = ((offset + y - center) / ROW_HEIGHT).round() as i64;
    let count = latest.options.len() as i64;
    if !latest.layout.looping && !(0..count).contains(&position) {
        return;
    }
    if position == nearest_index(offset, &latest.layout) {
        return;
    }
    play(
        state,
        turn_curve(offset, position as f64 * ROW_HEIGHT),
        End::Arrive,
        false,
        false,
        window,
        cx,
    );
}

/// A scroll over the column: a mouse wheel's notch, a trackpad, a finger.
fn scrolled(
    state: &Entity<WheelState>,
    event: &ScrollEvent,
    window: &mut Window,
    cx: &mut App,
) -> bool {
    let latest = state.read_with(cx, |s, _| s.latest());
    if event.modifiers.control || event.modifiers.platform {
        // A ctrl-scroll stays the page's zoom (FR15).
        return false;
    }
    if !latest.touch {
        let delta = event.delta.pixel_delta(px(LINE_PX));
        if f32::from(delta.y) == 0. {
            return false;
        }
        let max = latest.options.len().saturating_sub(1) as f64 * ROW_HEIGHT;
        let from = stop(state, window, cx);
        state.update(cx, |s, _| {
            s.offset = (from - f64::from(delta.y)).clamp(0., max)
        });
        arm_quiet(state, window, cx);
        window.refresh();
        return true;
    }
    match event.delta {
        ScrollDelta::Lines(lines) => {
            // A notch turns one row, as an arrow key does, with no coast.
            if event.modifiers.shift || lines.y == 0. || lines.x.abs() > lines.y.abs() {
                return false;
            }
            move_by(state, if lines.y < 0. { 1 } else { -1 }, window, cx);
            true
        }
        ScrollDelta::Pixels(delta) => {
            let dy = f64::from(delta.y);
            let source_touch = event.source == ScrollSource::Touch;
            let phase = event.touch_phase;
            let (drag, phased_lift) = state.read_with(cx, |s, _| (s.drag, s.phased_lift));
            if phase == TouchPhase::Started {
                state.update(cx, |s, _| s.phased_lift = false);
            } else if phased_lift || (source_touch && drag.is_none()) {
                // The platform's own momentum after a lift: the wheel coasts
                // by itself.
                return true;
            }
            let t = ms(now(cx), cx);
            let (start, travel) = match drag {
                Some(Drag::Trackpad { start, travel }) => (start, travel),
                _ => {
                    let shown = stop(state, window, cx);
                    state.update(cx, |s, _| {
                        s.tracker.reset();
                        s.tracker.add(0., 0., t);
                    });
                    (finger_offset(shown, &latest.layout), 0.)
                }
            };
            let travel = travel + dy;
            state.update(cx, |s, _| {
                s.drag = Some(Drag::Trackpad { start, travel });
                s.offset = drag_offset(start - travel, &latest.layout);
                s.tracker.add(0., travel, t);
            });
            window.refresh();
            if matches!(phase, TouchPhase::Ended | TouchPhase::Cancelled) {
                trackpad_lift(state, phase == TouchPhase::Ended, window, cx);
                state.update(cx, |s, _| s.phased_lift = true);
            } else {
                let timer = cx
                    .background_executor()
                    .timer(Duration::from_millis(TRACKPAD_LIFT_MS));
                let entity = state.clone();
                let task = window.spawn(cx, async move |cx| {
                    timer.await;
                    cx.update(|window, cx| trackpad_lift(&entity, true, window, cx))
                        .ok();
                });
                state.update(cx, |s, _| s.lift = Some(task));
            }
            true
        }
    }
}

fn trackpad_lift(state: &Entity<WheelState>, carry: bool, window: &mut Window, cx: &mut App) {
    let t = ms(now(cx), cx);
    let velocity = state.update(cx, |s, _| {
        s.lift = None;
        match s.drag.take() {
            Some(Drag::Trackpad { .. }) => Some(-s.tracker.scroll_velocity(t).y),
            other => {
                s.drag = other;
                None
            }
        }
    });
    if let Some(velocity) = velocity {
        release(state, if carry { velocity } else { 0. }, window, cx);
    }
}

/// Parks the committed row under the band when the wheel shows, and turns
/// onto a value that changes from outside (FR14).
fn park(
    state: &Entity<WheelState>,
    latest: &Latest,
    active: bool,
    window: &mut Window,
    cx: &mut App,
) {
    let selected = latest.selected();
    let key = (selected, latest.layout.row_count, latest.layout.looping);
    if !active {
        let had = state.update(cx, |s, _| {
            s.parked = false;
            s.park_key = None;
            s.drag = None;
            s.press = None;
            s.motion.is_some()
        });
        if had {
            stop(state, window, cx);
        }
        return;
    }
    let (seen, held, moving, park_turn, parked) = state.read_with(cx, |s, _| {
        (
            s.park_key == Some(key),
            s.drag.is_some(),
            s.motion.is_some(),
            s.park_turn,
            s.parked,
        )
    });
    if seen {
        return;
    }
    state.update(cx, |s, _| s.park_key = Some(key));
    if latest.touch {
        if held || (moving && park_turn.is_none()) {
            return;
        }
        let shown = stop(state, window, cx);
        let position = nearest_position(selected, shown, &latest.layout);
        let target = position as f64 * ROW_HEIGHT;
        let away = (shown - target).abs() > 0.01;
        state.update(cx, |s, _| s.committed = position);
        if away && parked {
            let turn = state.update(cx, |s, _| {
                s.next_id += 1;
                s.park_turn = Some(s.next_id);
                s.next_id
            });
            play(
                state,
                turn_curve(shown, target),
                End::Park(turn),
                false,
                false,
                window,
                cx,
            );
            return;
        }
        state.update(cx, |s, _| {
            if away {
                s.offset = selected as f64 * ROW_HEIGHT;
                s.committed = selected;
            }
            s.parked = true;
        });
        return;
    }
    let quiet = state.read_with(cx, |s, _| s.quiet.is_some());
    if held || moving || quiet {
        return;
    }
    let offset = state.read_with(cx, |s, _| s.offset);
    if (offset / ROW_HEIGHT).round() as i64 != selected {
        if parked {
            column_scroll(state, selected, End::None, window, cx);
        } else {
            state.update(cx, |s, _| s.offset = selected as f64 * ROW_HEIGHT);
        }
    }
    state.update(cx, |s, _| s.parked = true);
}

fn mix(a: Hsla, b: Hsla, t: f32) -> Hsla {
    let (a, b) = (a.to_rgb(), b.to_rgb());
    gpui::Rgba {
        r: a.r + (b.r - a.r) * t,
        g: a.g + (b.g - a.g) * t,
        b: a.b + (b.b - a.b) * t,
        a: a.a + (b.a - a.a) * t,
    }
    .into()
}

impl RenderOnce for Wheel {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens().clone();
        let rem = window.rem_size();
        let touch = astryx_primitives::coarse_primary_pointer(window, cx);
        let state = window.use_keyed_state(self.id.clone(), cx, |_, cx| WheelState::new(cx));
        if let Some(handle) = &self.handle {
            *handle.0.borrow_mut() = Some(state.clone());
        }
        let count = self.options.len();
        let layout = WheelLayout {
            row_height: ROW_HEIGHT,
            row_count: count,
            looping: touch && self.looping && count > 1,
        };
        let latest = Latest {
            options: self.options.clone(),
            value: self.value,
            on_change: self.on_change.clone(),
            layout,
            touch,
        };
        state.update(cx, |s, _| s.latest = Some(latest.clone()));
        park(&state, &latest, self.active, window, cx);

        let at = now(cx);
        let (offset, moving, held, fade_opacity, focus) = state.read_with(cx, |s, _| {
            let fade = s.motion.as_ref().and_then(|m| {
                m.fade.map(|fade| {
                    let p = (WheelState::elapsed(m, at) / fade).clamp(0., 1.) as f32;
                    (2. * p - 1.).abs()
                })
            });
            (
                s.shown(at),
                s.motion.is_some(),
                s.drag.is_some(),
                fade,
                s.focus.clone(),
            )
        });
        if moving {
            window.request_animation_frame();
        }
        let selected = latest.selected();
        let column_count = count as i64;
        // The mark: the row under the band at rest or under a drag; none
        // while the touch column moves (FR18). The scrolling column's mark
        // follows its scroll.
        let nearest = nearest_index(offset, &layout);
        let mark = if touch && moving && !held {
            None
        } else {
            Some(nearest)
        };

        let first = ((offset - BAND_TOP) / ROW_HEIGHT).floor() as i64 - 1;
        let last = ((offset + PANE_HEIGHT - BAND_TOP) / ROW_HEIGHT).ceil() as i64 + 1;
        let (first, last) = if layout.looping {
            (first, last)
        } else {
            (first.max(0), last.min(column_count - 1))
        };
        let id = self.id.clone();
        let layers = window.supports_offscreen_layers();
        let mut strip = div().absolute().inset_0();
        if touch && layers {
            // Upstream's static mask: the rows fade toward the column's edges, to
            // `FADE_FLOOR` (fork patch R3).
            strip = strip.mask_linear(
                gpui::LinearMask::new(180.)
                    .stop(gpui::MaskStop::new(0., FADE_FLOOR))
                    .stop(gpui::MaskStop::new(0.5, 1.))
                    .stop(gpui::MaskStop::new(1., FADE_FLOOR)),
            );
        }
        if let Some(opacity) = fade_opacity {
            strip = strip.opacity(opacity);
        }
        let mixed = mix(t.color.text_primary, t.color.text_secondary, 0.5);
        let reduced = prefers_reduced_motion(cx);
        for position in first..=last {
            let Some(option) = latest.option_at(position).cloned() else {
                continue;
            };
            let own = (0..column_count).contains(&position);
            let top = BAND_TOP + position as f64 * ROW_HEIGHT - offset;
            let lit = mark == Some(position);
            // How far the row's centre is from the band's, 0 to 1 at the edges.
            let distance = ((top + ROW_HEIGHT / 2. - PANE_HEIGHT / 2.).abs()
                / (PANE_HEIGHT / 2. + ROW_HEIGHT / 2.))
                .min(1.) as f32;
            let color = if lit && !option.disabled {
                t.color.text_accent
            } else if option.disabled {
                t.color.text_disabled
            } else if touch {
                t.color.text_primary
            } else {
                mix(t.color.text_primary, mixed, distance)
            };
            let row_id = ElementId::Name(format!("{id}-row-{position}").into());
            let mut row = div()
                .id(row_id)
                .absolute()
                .left_0()
                .right_0()
                .top(px(top as f32))
                .h(px(ROW_HEIGHT as f32))
                .flex()
                .items_center()
                .justify_center()
                .px(t.spacing.s2)
                .overflow_hidden()
                .whitespace_nowrap()
                .text_style(&t.text.large)
                .font_weight(if lit {
                    t.font.weight.semibold
                } else {
                    t.font.weight.normal
                })
                .text_color(color)
                .child(option.label.clone());
            if touch {
                // The touch column's rows are flat, under the column's fade mask; a
                // renderer without offscreen layers fades each row by its distance.
                if !layers {
                    row = row.opacity(1. - (1. - FADE_FLOOR) * distance);
                }
            } else {
                // The scrolling column's cylinder (`tip`): a row tips away
                // from the band, rotateX up to 52° and scale 0.86 at the
                // edges, projected flat; Reduce Motion keeps the colour fade.
                if !reduced {
                    let angle = TIP_DEGREES.to_radians() * distance;
                    let scale = 1. - (1. - TIP_SCALE) * distance;
                    row = row.scale_xy(scale, scale * angle.cos());
                }
                if option.disabled {
                    // `tipAndDim` and `dim`: a disabled row fades by opacity.
                    row = row.opacity(1. - 0.7 * distance);
                }
            }
            if own {
                let selector = format!("{id}-option-{}", option.value);
                row = row
                    .debug_selector(move || selector.clone())
                    .role(Role::ListBoxOption)
                    .aria_label(option.label.clone())
                    .aria_selected(option.value == self.value)
                    .aria_disabled(option.disabled);
                if position == selected {
                    row = row.aria_active_descendant_of(&focus);
                }
                if !touch {
                    let state = state.clone();
                    let index = position as usize;
                    row = row
                        .cursor(if option.disabled {
                            gpui::CursorStyle::Arrow
                        } else {
                            gpui::CursorStyle::PointingHand
                        })
                        .on_click(move |_, window, cx| {
                            let dragged = state.update(cx, |s, _| std::mem::take(&mut s.dragged));
                            if !dragged {
                                activate(&state, index, window, cx);
                            }
                        });
                }
            }
            strip = strip.child(row);
        }

        let band = div()
            .absolute()
            .left_0()
            .right_0()
            .top(px(BAND_TOP as f32))
            .h(px(ROW_HEIGHT as f32))
            .rounded(t.radius.element)
            .bg(t.color.neutral);
        let mut column = div()
            .id(self.id.clone())
            .debug_selector({
                let selector = format!("{}", self.id);
                move || selector.clone()
            })
            .track_focus(&focus)
            .relative()
            .flex_1()
            .min_w_0()
            .h(px(PANE_HEIGHT as f32))
            .overflow_hidden()
            .role(Role::ListBox)
            .aria_label(self.label.clone())
            .child(band)
            .child(strip);
        if self.active {
            let keys = state.clone();
            column = column.on_key_down(move |event: &KeyDownEvent, window, cx| {
                let key = event.keystroke.key.as_str();
                let handled = match key {
                    "down" => Some((Some(1), None)),
                    "up" => Some((Some(-1), None)),
                    "pagedown" => Some((Some(PAGE_ROWS), None)),
                    "pageup" => Some((Some(-PAGE_ROWS), None)),
                    "home" => Some((None, Some(false))),
                    "end" => Some((None, Some(true))),
                    _ => None,
                };
                if let Some((delta, end)) = handled {
                    cx.stop_propagation();
                    if let Some(delta) = delta {
                        move_by(&keys, delta, window, cx);
                    }
                    if let Some(last) = end {
                        to_end(&keys, last, window, cx);
                    }
                }
            });
            let down = state.clone();
            column = column.on_mouse_down(
                MouseButton::Left,
                move |event: &MouseDownEvent, window, cx| {
                    // The press is the wheel's: a sheet around it never reads it
                    // as its own drag.
                    cx.stop_propagation();
                    let focus = down.read(cx).focus.clone();
                    window.focus(&focus, cx);
                    press(&down, f64::from(event.position.y), false, window, cx);
                },
            );
            let scroll = state.clone();
            column = column.on_scroll(move |event: &ScrollEvent, window, cx| {
                if scrolled(&scroll, event, window, cx) {
                    cx.stop_propagation();
                }
            });
            // The probe: the column's bounds for taps, the touch claim, and
            // the pointer anywhere while a press holds the wheel.
            let probe = state.clone();
            let pressed = state.read_with(cx, |s, _| s.press.is_some());
            column = column.child(
                gpui::canvas(
                    {
                        let probe = probe.clone();
                        move |bounds, _, cx| {
                            probe.update(cx, |s, _| s.bounds = Some(bounds));
                        }
                    },
                    move |bounds, _, window, _| {
                        let claim = probe.clone();
                        window.on_mouse_event(move |event: &TouchDragEvent, phase, window, cx| {
                            if phase != DispatchPhase::Bubble {
                                return;
                            }
                            let y = f64::from(event.position.y);
                            match event.phase {
                                TouchPhase::Started => {
                                    if bounds.contains(&event.start_position) {
                                        window.prevent_default();
                                        let focus = claim.read(cx).focus.clone();
                                        window.focus(&focus, cx);
                                        press(&claim, y, true, window, cx);
                                    }
                                }
                                TouchPhase::Moved => {
                                    if claim.read(cx).press.is_some_and(|p| p.touch) {
                                        pointer_moved(&claim, y, window, cx);
                                    }
                                }
                                TouchPhase::Ended | TouchPhase::Cancelled => {
                                    if claim.read(cx).press.is_some_and(|p| p.touch) {
                                        pointer_up(&claim, y, window, cx);
                                    }
                                }
                            }
                        });
                        if pressed {
                            let moving = probe.clone();
                            window.on_mouse_event(
                                move |event: &MouseMoveEvent, phase, window, cx| {
                                    if phase == DispatchPhase::Capture
                                        && moving.read(cx).press.is_some_and(|p| !p.touch)
                                    {
                                        pointer_moved(
                                            &moving,
                                            f64::from(event.position.y),
                                            window,
                                            cx,
                                        );
                                    }
                                },
                            );
                            let up = probe.clone();
                            window.on_mouse_event(
                                move |event: &MouseUpEvent, phase, window, cx| {
                                    if phase == DispatchPhase::Capture
                                        && up.read(cx).press.is_some_and(|p| !p.touch)
                                    {
                                        pointer_up(&up, f64::from(event.position.y), window, cx);
                                    }
                                },
                            );
                        }
                    },
                )
                .absolute()
                .size_full(),
            );
        }
        if is_focus_visible(&focus, window) {
            column = column.child(focus_ring(&t, t.radius.element.to_pixels(rem), rem));
        }
        column
    }
}
