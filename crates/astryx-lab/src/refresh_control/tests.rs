//! Upstream `refreshModel.test.ts`, `refreshPull.test.ts` and the portable parts of
//! `useRefreshControl.test.tsx` (the pull, the end, controlled and programmatic
//! refreshes, the announcement and the disabled state).

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use astryx_primitives::announce::{Politeness, announcement};
use gpui::{
    Context, Render, ScrollHandle, TestAppContext, TouchEvent, TouchId, TouchPhase,
    VisualTestContext, Window, div, point, px,
};

use super::*;

fn run(events: &[RefreshEvent], from: RefreshState) -> (RefreshState, Vec<RefreshEffect>) {
    let mut state = from;
    let mut effects = Vec::new();
    for event in events {
        let (next, more) = reduce_refresh(state, *event);
        state = next;
        effects.extend(more);
    }
    (state, effects)
}

const fn mv(overscroll: f64) -> RefreshEvent {
    RefreshEvent::PullMove {
        overscroll,
        trigger: 74.,
    }
}

use RefreshEffect::{Begin, End, Settle, Trigger};
use RefreshEvent::{PullCancel, PullEnd, PullStart};

#[test]
fn triggers_once_with_the_finger_down_when_the_pull_reaches_the_trigger_distance() {
    let (state, effects) = run(
        &[PullStart, mv(40.), mv(73.33), mv(74.), mv(95.)],
        INITIAL_REFRESH_STATE,
    );
    assert_eq!(effects, [Trigger]);
    assert_eq!(
        state,
        RefreshState {
            is_refreshing: true,
            is_pulling: true,
            has_triggered: true,
        }
    );
}

#[test]
fn settles_onto_the_inset_when_released_while_refreshing() {
    let (state, effects) = run(&[PullStart, mv(80.), PullEnd], INITIAL_REFRESH_STATE);
    assert_eq!(effects, [Trigger, Settle(SettleTarget::Inset)]);
    assert!(!state.is_pulling && state.is_refreshing);
}

#[test]
fn settles_back_to_the_top_when_released_below_the_trigger() {
    let (state, effects) = run(&[PullStart, mv(50.), PullEnd], INITIAL_REFRESH_STATE);
    assert_eq!(effects, [Settle(SettleTarget::Top)]);
    assert!(!state.is_refreshing);
}

#[test]
fn keeps_refreshing_when_a_pull_that_triggered_is_pushed_back_before_the_lift() {
    let (state, effects) = run(
        &[PullStart, mv(90.), mv(10.), PullEnd],
        INITIAL_REFRESH_STATE,
    );
    assert_eq!(effects, [Trigger, Settle(SettleTarget::Inset)]);
    assert!(state.is_refreshing);
}

#[test]
fn settles_a_cancelled_pull_like_a_release() {
    assert_eq!(
        run(&[PullStart, mv(30.), PullCancel], INITIAL_REFRESH_STATE).1,
        [Settle(SettleTarget::Top)]
    );
    assert_eq!(
        run(&[PullStart, mv(99.), PullCancel], INITIAL_REFRESH_STATE).1,
        [Trigger, Settle(SettleTarget::Inset)]
    );
}

#[test]
fn never_triggers_from_movement_that_no_finger_is_pulling() {
    assert!(run(&[mv(120.)], INITIAL_REFRESH_STATE).1.is_empty());
}

#[test]
fn never_triggers_again_on_a_pull_while_refreshing() {
    let refreshing = run(&[PullStart, mv(80.), PullEnd], INITIAL_REFRESH_STATE).0;
    // Far past the trigger, and past it beyond the inset too.
    let (state, effects) = run(&[PullStart, mv(134.), mv(200.), PullEnd], refreshing);
    assert_eq!(effects, [Settle(SettleTarget::Inset)]);
    assert!(state.is_refreshing);
}

#[test]
fn begins_programmatically_calling_the_handler_only_for_the_users_request() {
    let user = RefreshEvent::Begin(BeginSource::User);
    let program = RefreshEvent::Begin(BeginSource::Program);
    assert_eq!(
        run(&[user], INITIAL_REFRESH_STATE).1,
        [Begin { call_handler: true }]
    );
    assert_eq!(
        run(&[program], INITIAL_REFRESH_STATE).1,
        [Begin {
            call_handler: false
        }]
    );
    // Already refreshing: nothing new starts.
    assert_eq!(
        run(&[program, user], INITIAL_REFRESH_STATE).1,
        [Begin {
            call_handler: false
        }]
    );
}

#[test]
fn ends_a_running_refresh_once_and_ignores_an_end_with_nothing_running() {
    assert!(
        run(&[RefreshEvent::End], INITIAL_REFRESH_STATE)
            .1
            .is_empty()
    );
    let (state, effects) = run(
        &[
            RefreshEvent::Begin(BeginSource::Program),
            RefreshEvent::End,
            RefreshEvent::End,
        ],
        INITIAL_REFRESH_STATE,
    );
    assert_eq!(
        effects,
        [
            Begin {
                call_handler: false
            },
            End
        ]
    );
    assert!(!state.is_refreshing);
}

#[test]
fn ends_a_refresh_under_the_finger_without_settling_the_lift_then_settles_to_the_top() {
    let (_, effects) = run(
        &[PullStart, mv(80.), RefreshEvent::End, PullEnd],
        INITIAL_REFRESH_STATE,
    );
    assert_eq!(effects, [Trigger, End, Settle(SettleTarget::Top)]);
}

#[test]
fn ignores_a_release_with_no_pull_in_progress() {
    assert!(run(&[PullEnd], INITIAL_REFRESH_STATE).1.is_empty());
}

/// UIKit lab capture: a UIRefreshControl on a 334 pt scroll view, iOS 26.2 simulator,
/// HID drags at about 150 pt/s. Each row is [finger travel since touch-down, overscroll
/// shown, whether the pull triggered]. The pan's 10 pt hysteresis is part of the travel.
const PULL_120: [(f64, f64, bool); 28] = [
    (12.67, 2.67, false),
    (15., 5., false),
    (20., 10., false),
    (25., 15., false),
    (27.67, 17.67, false),
    (32.33, 22.33, false),
    (35., 25., false),
    (40., 30., false),
    (45., 35., false),
    (50., 40., false),
    (52.67, 42.67, false),
    (55., 45., false),
    (60., 50., false),
    (62.33, 52.33, false),
    (67.67, 57.67, false),
    (70., 60., false),
    (75., 62.67, false),
    (77.33, 64., false),
    (82.67, 66.67, false),
    (87.67, 69.33, false),
    (92.33, 72., false),
    (95., 73.33, false),
    (100., 75.67, false),
    (105., 95., true),
    (110., 100., true),
    (115., 105., true),
    (117.67, 107.67, true),
    (120., 110., true),
];
const PULL_200: [(f64, f64, bool); 28] = [
    (15., 5., false),
    (20., 10., false),
    (25., 15., false),
    (30., 20., false),
    (35., 25., false),
    (37.67, 27.67, false),
    (40., 30., false),
    (42.67, 32.67, false),
    (47.33, 37.33, false),
    (52.67, 42.67, false),
    (57.67, 47.67, false),
    (62.33, 52.33, false),
    (67.67, 57.67, false),
    (72.67, 61.33, false),
    (75., 62.67, false),
    (80., 65.33, false),
    (85., 68., false),
    (90., 70.67, false),
    (95., 73.33, false),
    (97.67, 74.67, false),
    (102.67, 92.67, true),
    (107.33, 97.33, true),
    (110., 100., true),
    (112.67, 102.67, true),
    (115., 105., true),
    (120., 110., true),
    (125., 115., true),
    (127.67, 117.67, true),
];
const VIEW: f64 = 334.;
const HYSTERESIS: f64 = 10.;

#[test]
fn reproduces_uikits_pull_frame_by_frame_before_and_after_the_trigger() {
    for (name, rows) in [("pull120", PULL_120), ("pull200", PULL_200)] {
        for (travel, shown, triggered) in rows {
            let t = travel - HYSTERESIS;
            let mode = if triggered {
                mode_after_trigger(t)
            } else {
                PullMode::Reveal
            };
            let model = pull_overscroll(t, VIEW, mode);
            let tolerance = if triggered { 0.01 } else { 0.2 };
            assert!(
                (model - shown).abs() <= tolerance,
                "{name} at {travel}: {model} vs {shown}"
            );
        }
    }
}

#[test]
fn tracks_the_finger_one_to_one_up_to_the_controls_height() {
    assert_eq!(REFRESH_HEIGHT, 60.);
    for travel in [0.5, 10., 30., 59.9, 60.] {
        assert_eq!(pull_overscroll(travel, 800., PullMode::Reveal), travel);
    }
}

#[test]
fn rubber_bands_the_excess_past_the_height_never_reaching_the_views_height() {
    let at = |travel| pull_overscroll(travel, 800., PullMode::Reveal);
    assert!(at(61.) > 60. && at(61.) - 60. < 1.);
    assert!(at(2000.) < 60. + 800.);
    assert!(at(200.) > at(150.));
}

#[test]
fn is_one_to_one_up_to_twice_the_height_in_the_band() {
    assert_eq!(pull_overscroll(95., 800., PullMode::Band), 95.);
    assert_eq!(pull_overscroll(120., 800., PullMode::Band), 120.);
    assert!(pull_overscroll(121., 800., PullMode::Band) < 121.);
}

#[test]
fn steps_onto_the_finger_only_for_a_trigger_within_the_band() {
    assert_eq!(mode_after_trigger(90.), PullMode::Band);
    assert_eq!(mode_after_trigger(120.), PullMode::Band);
    // A tall view triggers past the band: the pull keeps its curve.
    assert_eq!(mode_after_trigger(296.), PullMode::Reveal);
}

#[test]
fn shows_nothing_for_a_pull_that_has_not_started_or_went_the_other_way() {
    assert_eq!(pull_overscroll(0., 800., PullMode::Reveal), 0.);
    assert_eq!(pull_overscroll(-25., 800., PullMode::Reveal), 0.);
    assert_eq!(pull_overscroll(f64::NAN, 800., PullMode::Reveal), 0.);
}

#[test]
fn pull_travel_inverts_pull_overscroll_in_both_modes() {
    for mode in [PullMode::Reveal, PullMode::Band] {
        for travel in [0., 12., 60., 61., 90., 140., 400.] {
            let shown = pull_overscroll(travel, 874., mode);
            assert!((pull_travel(shown, 874., mode) - travel).abs() < 1e-6);
        }
    }
}

#[test]
fn triggers_at_uikits_distance_for_each_view_height() {
    assert!((trigger_distance(334.) - 73.).abs() < 0.5);
    assert!((79. ..=79.3).contains(&trigger_distance(400.)));
    assert!((168.3..=173.7).contains(&trigger_distance(874.)));
    assert!((trigger_distance(1366.) - 271.).abs() < 2.);
    // Never below the floor, even for a view with no height yet.
    assert_eq!(trigger_distance(0.), 73.);
}

#[test]
fn triggers_on_uikits_frames_at_most_one_frame_ahead_of_uikits_own_report() {
    // UIKit reports valueChanged on the frame after the crossing.
    let trigger = trigger_distance(VIEW);
    for rows in [PULL_120, PULL_200] {
        let reported = rows.iter().position(|r| r.2).expect("a trigger");
        let shown = |travel: f64| pull_overscroll(travel - HYSTERESIS, VIEW, PullMode::Reveal);
        assert!(shown(rows[reported - 1].0) >= trigger);
        assert!(shown(rows[reported - 3].0) < trigger);
    }
}

#[test]
fn reveals_in_proportion_to_the_pull_fully_at_the_trigger() {
    let t = trigger_distance(874.);
    assert_eq!(reveal_fraction(0., 874.), 0.);
    assert!((reveal_fraction(t / 2., 874.) - 0.5).abs() < 1e-6);
    assert_eq!(reveal_fraction(t, 874.), 1.);
    assert_eq!(reveal_fraction(500., 874.), 1.);
    assert_eq!(reveal_fraction(-5., 874.), 0.);
}

#[test]
fn eases_the_reveal_on_uikits_reveal_curve() {
    let t = trigger_distance(334.);
    assert_eq!(reveal_progress(0., 334.), 0.);
    assert!((reveal_progress(t, 334.) - 1.).abs() < 1e-6);
    // The ease-in-out curve is symmetric: halfway through is halfway.
    assert!((reveal_progress(t / 2., 334.) - 0.5).abs() < 1e-3);
    assert!(reveal_progress(t / 4., 334.) < 0.25);
}

#[test]
fn releases_onto_the_inset_while_refreshing_and_back_to_the_top_otherwise() {
    assert_eq!(release_target(true), 60.);
    assert_eq!(release_target(false), 0.);
}

#[test]
fn the_end_closes_a_visible_gap_and_moves_nothing_once_the_control_scrolled_away() {
    assert_eq!(
        end_motion(0.),
        EndMotion {
            animate_to: Some(0.),
            scroll_by: -0.
        }
    );
    assert_eq!(
        end_motion(25.),
        EndMotion {
            animate_to: Some(25.),
            scroll_by: -25.
        }
    );
    assert_eq!(
        end_motion(60.),
        EndMotion {
            animate_to: None,
            scroll_by: -60.
        }
    );
    assert_eq!(
        end_motion(500.),
        EndMotion {
            animate_to: None,
            scroll_by: -60.
        }
    );
}

#[test]
fn the_end_plays_uikits_ease_in_ease_out_over_the_themes_duration() {
    assert_eq!(
        end_timing(Some(300.)),
        Some(EndTiming {
            ms: 300.,
            points: [0.42, 0., 0.58, 1.]
        })
    );
    // A theme's tempo reaches the end through the token's value.
    assert_eq!(end_timing(Some(450.)).map(|t| t.ms), Some(450.));
    assert_eq!(end_timing(None), None);
}

#[test]
fn the_slope_is_one_where_the_pull_tracks_the_finger_and_the_bands_past_it() {
    assert_eq!(pull_slope(0., 334., PullMode::Reveal), 0.);
    assert!((pull_slope(30., 334., PullMode::Reveal) - 1.).abs() < 1e-6);
    let past = 100.;
    let slope = pull_slope(past, 334., PullMode::Reveal);
    assert!(slope > 0. && slope < 1.);
    let step = 0.5;
    let numeric = (pull_overscroll(past + step, 334., PullMode::Reveal)
        - pull_overscroll(past - step, 334., PullMode::Reveal))
        / (2. * step);
    assert!((slope - numeric).abs() < 1e-3);
    assert!(pull_slope(past + 100., 334., PullMode::Reveal) < slope);
    // The band after a trigger tracks one to one for twice as long.
    assert!((pull_slope(100., 334., PullMode::Band) - 1.).abs() < 1e-6);
}

// The hook in a window.

type Options = Rc<dyn Fn(ScrollHandle) -> RefreshControlOptions>;

struct Host {
    handle: ScrollHandle,
    options: Options,
    out: Rc<RefCell<Option<RefreshControl>>>,
}

impl Render for Host {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let control = use_refresh_control("pull", (self.options)(self.handle.clone()), window, cx);
        *self.out.borrow_mut() = Some(control.clone());
        div().size_full().child(
            div()
                .id("list")
                .w(px(400.))
                .h(px(334.))
                .overflow_y_scroll()
                .track_scroll(&self.handle)
                .child(control.content(div().h(px(1000.)).child(control.indicator(div())))),
        )
    }
}

struct Rig<'a> {
    cx: &'a mut VisualTestContext,
    handle: ScrollHandle,
    out: Rc<RefCell<Option<RefreshControl>>>,
}

impl Rig<'_> {
    fn control(&self) -> RefreshControl {
        self.out.borrow().clone().expect("rendered")
    }

    fn frame(&mut self) {
        self.cx.update(|window, _| window.refresh());
        self.cx.run_until_parked();
    }

    /// Frames over `ms`, 16 ms apart.
    fn frames(&mut self, ms: u64) {
        for _ in 0..ms.div_ceil(16) {
            self.cx.executor().advance_clock(Duration::from_millis(16));
            self.frame();
        }
    }

    fn scroll_top(&self) -> f64 {
        -f64::from(f32::from(self.handle.offset().y))
    }

    fn refresh(&mut self) {
        let control = self.control();
        self.cx.update(|window, cx| control.refresh(window, cx));
        self.frame();
    }

    fn finger(&mut self, phase: TouchPhase, y: f32) {
        self.cx.simulate_event(TouchEvent {
            id: TouchId(1),
            phase,
            position: point(px(100.), px(y)),
            predicted_position: None,
            force: None,
        });
        self.cx.executor().advance_clock(Duration::from_millis(16));
        self.frame();
    }

    /// A one-finger drag from y 100 down by each step in turn.
    fn pull(&mut self, steps: &[f32], lift: bool) {
        let mut y = 100.;
        self.finger(TouchPhase::Started, y);
        for step in steps {
            y += step;
            self.finger(TouchPhase::Moved, y);
        }
        if lift {
            self.finger(TouchPhase::Ended, y);
        }
    }
}

fn rig(
    cx: &mut TestAppContext,
    options: impl Fn(ScrollHandle) -> RefreshControlOptions + 'static,
) -> Rig<'_> {
    cx.update(|cx| {
        astryx_theme::init(cx);
        astryx_primitives::focus::init(cx);
    });
    let handle = ScrollHandle::new();
    let out = Rc::new(RefCell::new(None));
    let (h, o) = (handle.clone(), out.clone());
    let options: Options = Rc::new(options);
    let (_, cx) = cx.add_window_view(move |_, _| Host {
        handle: h,
        options,
        out: o,
    });
    let mut rig = Rig { cx, handle, out };
    rig.frame();
    rig.frame();
    rig
}

/// A handler that counts its calls and returns a task of `ms`.
fn counted(
    calls: &Rc<Cell<usize>>,
    ms: Option<u64>,
) -> impl Fn(&mut Window, &mut App) -> Option<Task<()>> + 'static {
    let calls = calls.clone();
    move |_, cx| {
        calls.set(calls.get() + 1);
        ms.map(|ms| {
            let timer = cx.background_executor().timer(Duration::from_millis(ms));
            cx.foreground_executor().spawn(timer)
        })
    }
}

#[gpui::test]
fn refresh_begins_announces_and_ends_when_the_handlers_task_completes(cx: &mut TestAppContext) {
    let calls = Rc::new(Cell::new(0));
    let c = calls.clone();
    let mut r = rig(cx, move |h| {
        RefreshControlOptions::new()
            .scroll_handle(h)
            .announce("Refreshing")
            .on_refresh(counted(&c, Some(500)))
    });
    r.refresh();
    assert!(r.control().is_refreshing());
    assert_eq!(calls.get(), 1);
    let said =
        r.cx.update(|window, cx| announcement(Politeness::Polite, window, cx));
    assert_eq!(said, "Refreshing");
    // The inset arrives with no visible motion: the content moves down by it and the
    // container scrolls by it.
    assert_eq!(r.control().offset(), 60.);
    assert_eq!(r.scroll_top(), 60.);
    assert_eq!(r.control().reveal(), 1., "the indicator in full");
    // A second refresh while one runs starts nothing.
    r.refresh();
    assert_eq!(calls.get(), 1);
    r.frames(520);
    assert!(!r.control().is_refreshing());
    // Scrolled past the gap: the inset and the offset drop at once.
    assert_eq!(r.control().offset(), 0.);
    assert_eq!(r.scroll_top(), 0.);
    assert_eq!(r.control().reveal(), 0.);
}

#[gpui::test]
fn a_handler_with_no_task_ends_the_refresh_right_after(cx: &mut TestAppContext) {
    let calls = Rc::new(Cell::new(0));
    let c = calls.clone();
    let mut r = rig(cx, move |h| {
        RefreshControlOptions::new()
            .scroll_handle(h)
            .on_refresh(counted(&c, None))
    });
    r.refresh();
    r.frame();
    assert_eq!(calls.get(), 1);
    assert!(!r.control().is_refreshing());
}

#[gpui::test]
fn the_end_slides_the_content_up_while_the_gap_shows(cx: &mut TestAppContext) {
    let calls = Rc::new(Cell::new(0));
    let c = calls.clone();
    let mut r = rig(cx, move |h| {
        RefreshControlOptions::new()
            .scroll_handle(h)
            .on_refresh(counted(&c, Some(200)))
    });
    r.refresh();
    // Scrolled back to the top while refreshing: the gap with the indicator shows.
    r.handle.set_offset(point(px(0.), px(0.)));
    r.frame();
    r.frames(220);
    let mid = r.control().offset();
    assert!(mid > 0. && mid <= 60., "sliding: {mid}");
    r.frames(600);
    assert_eq!(r.control().offset(), 0.);
    assert_eq!(r.scroll_top(), 0.);
}

#[gpui::test]
fn follows_a_controlled_is_refreshing_without_calling_the_handler(cx: &mut TestAppContext) {
    let calls = Rc::new(Cell::new(0));
    let refreshing = Rc::new(Cell::new(true));
    let (c, on) = (calls.clone(), refreshing.clone());
    let mut r = rig(cx, move |h| {
        RefreshControlOptions::new()
            .scroll_handle(h)
            .is_refreshing(on.get())
            .on_refresh(counted(&c, None))
    });
    assert!(r.control().is_refreshing(), "from the first render");
    assert_eq!(r.control().offset(), 60.);
    refreshing.set(false);
    r.frame();
    assert!(!r.control().is_refreshing());
    r.frames(100);
    assert_eq!(r.control().offset(), 0.);
    assert_eq!(calls.get(), 0);
}

#[gpui::test]
fn a_refresh_the_parent_does_not_take_ends_on_the_next_render(cx: &mut TestAppContext) {
    let calls = Rc::new(Cell::new(0));
    let c = calls.clone();
    let mut r = rig(cx, move |h| {
        RefreshControlOptions::new()
            .scroll_handle(h)
            .is_refreshing(false)
            .on_refresh(counted(&c, None))
    });
    r.refresh();
    r.frame();
    assert_eq!(calls.get(), 1);
    assert!(!r.control().is_refreshing());
    assert_eq!(r.control().offset(), 0., "the inset is gone");
}

#[gpui::test]
fn a_finger_pulls_one_to_one_past_the_hysteresis_then_triggers_and_settles_onto_the_inset(
    cx: &mut TestAppContext,
) {
    let calls = Rc::new(Cell::new(0));
    let c = calls.clone();
    let mut r = rig(cx, move |h| {
        RefreshControlOptions::new()
            .scroll_handle(h)
            .on_refresh(counted(&c, Some(5000)))
    });
    r.pull(&[5., 5., 5., 5., 5., 5., 5., 5.], false);
    // 40 px of travel, 10 of them the pan's hysteresis.
    assert_eq!(r.control().offset(), 30.);
    assert!(r.control().reveal() > 0. && r.control().reveal() < 1.);
    assert_eq!(calls.get(), 0);
    let mut y = 140.;
    for _ in 0..16 {
        y += 5.;
        r.finger(TouchPhase::Moved, y);
    }
    // Past the trigger: the refresh starts with the finger down, once.
    assert_eq!(calls.get(), 1);
    assert!(r.control().is_refreshing());
    r.finger(TouchPhase::Ended, y);
    r.frames(1500);
    assert!(
        (r.control().offset() - 60.).abs() < 0.5,
        "on the inset: {}",
        r.control().offset()
    );
}

#[gpui::test]
fn a_pull_below_the_trigger_settles_back_to_the_top(cx: &mut TestAppContext) {
    let calls = Rc::new(Cell::new(0));
    let c = calls.clone();
    let mut r = rig(cx, move |h| {
        RefreshControlOptions::new()
            .scroll_handle(h)
            .on_refresh(counted(&c, None))
    });
    r.pull(&[10., 10., 10., 10.], true);
    r.frames(1500);
    assert_eq!(calls.get(), 0);
    assert!(r.control().offset().abs() < 0.5);
    assert_eq!(r.control().reveal(), 0.);
}

#[gpui::test]
fn does_not_pull_while_disabled(cx: &mut TestAppContext) {
    let calls = Rc::new(Cell::new(0));
    let c = calls.clone();
    let mut r = rig(cx, move |h| {
        RefreshControlOptions::new()
            .scroll_handle(h)
            .disabled(true)
            .on_refresh(counted(&c, None))
    });
    r.pull(&[20., 20., 20., 20., 20., 20.], false);
    assert_eq!(r.control().offset(), 0.);
    assert_eq!(calls.get(), 0);
    // A refresh still runs.
    r.refresh();
    assert_eq!(calls.get(), 1);
}

#[gpui::test]
fn never_scrolls_without_a_container_that_overflows(cx: &mut TestAppContext) {
    let calls = Rc::new(Cell::new(0));
    let c = calls.clone();
    let mut r = rig(cx, move |_| {
        RefreshControlOptions::new().on_refresh(counted(&c, Some(100)))
    });
    r.pull(&[20., 20., 20., 20., 20., 20.], true);
    assert_eq!(calls.get(), 0, "no pull without a scroll container");
    r.refresh();
    assert_eq!(r.scroll_top(), 0.);
}

#[gpui::test]
fn a_finger_moving_up_scrolls_the_container_natively(cx: &mut TestAppContext) {
    let calls = Rc::new(Cell::new(0));
    let c = calls.clone();
    let mut r = rig(cx, move |h| {
        RefreshControlOptions::new()
            .scroll_handle(h)
            .on_refresh(counted(&c, None))
    });
    r.pull(&[-10., -10., -10., -10., -10., -10.], true);
    assert!(r.scroll_top() > 0., "scrolled: {}", r.scroll_top());
    assert_eq!(r.control().offset(), 0.);
    assert_eq!(calls.get(), 0);
}

#[gpui::test]
fn calls_on_refresh_once_a_pull_while_refreshing_only_stretches(cx: &mut TestAppContext) {
    let calls = Rc::new(Cell::new(0));
    let c = calls.clone();
    let mut r = rig(cx, move |h| {
        RefreshControlOptions::new()
            .scroll_handle(h)
            .on_refresh(counted(&c, Some(10_000)))
    });
    let far = [8.; 20];
    r.pull(&far, true);
    r.frames(1500);
    assert_eq!(calls.get(), 1);
    assert!(r.control().is_refreshing());
    // From the inset, a second pull stretches the band and starts nothing.
    r.pull(&far, false);
    assert!(r.control().offset() > 60.);
    r.finger(TouchPhase::Ended, 100. + 160.);
    assert_eq!(calls.get(), 1);
    r.frames(1500);
    assert!((r.control().offset() - 60.).abs() < 0.5);
}

#[gpui::test]
fn a_drag_that_begins_as_a_scroll_pulls_once_the_content_reaches_its_top(cx: &mut TestAppContext) {
    let calls = Rc::new(Cell::new(0));
    let c = calls.clone();
    let mut r = rig(cx, move |h| {
        RefreshControlOptions::new()
            .scroll_handle(h)
            .on_refresh(counted(&c, Some(5000)))
    });
    // Scrolled 30 px into the content.
    r.handle.set_offset(point(px(0.), px(-30.)));
    r.frame();
    let mut y = 100.;
    r.finger(TouchPhase::Started, y);
    // The finger scrolls the content back to its top, then pulls on.
    for _ in 0..12 {
        y += 10.;
        r.finger(TouchPhase::Moved, y);
    }
    assert_eq!(r.scroll_top(), 0., "scrolled to the top");
    assert!(
        r.control().offset() > 0.,
        "then pulled: {}",
        r.control().offset()
    );
    r.finger(TouchPhase::Ended, y);
}
