//! Upstream `motion/animator.test.ts` (the engine's observable values; the
//! cases about keyframes, inline styles, promotion and pseudo-elements test
//! DOM plumbing and are skipped), with the animator cases of
//! `resolveMotion.test.ts` and `resolveMotion.tempo.test.ts`.

use std::cell::Cell;
use std::rc::Rc;

use super::animator::*;
use super::bezier::cubic_bezier;
use super::coast::{CoastBounds, CoastOptions, CoastVelocity, coast};
use super::resolve_motion::{SpringMove, resolve_move_timing, resolve_settle_ms, resolve_spring};
use super::uikit::{scroll, springs, timing};
use super::{
    FnCurve, MotionProperty, MotionSpring, SharedCurve, SpringCurve, SpringDescription,
    solve_decay, solve_spring,
};

use MotionProperty::{Opacity, Rotate, ScaleX, ScaleY, X, Y};

fn close(a: f64, b: f64, digits: i32) -> bool {
    (a - b).abs() < 10f64.powi(-digits) / 2.
}

macro_rules! assert_close {
    ($a:expr, $b:expr, $digits:expr) => {{
        let (a, b) = ($a, $b);
        assert!(
            close(a, b, $digits),
            "{} = {a}, expected {b} to {} digits",
            stringify!($a),
            $digits
        );
    }};
}

/// A clock at 1000 ms and an engine on it, driven by frames as the GPUI
/// layer drives it (upstream's test harness: a recording `requestFrame`).
struct Rig {
    clock: Rc<Cell<f64>>,
    engine: MotionEngine,
}

fn rig_with(reduced: bool, coarse: bool, initial: MotionValues) -> Rig {
    let clock = Rc::new(Cell::new(1000.));
    let read = clock.clone();
    let engine = MotionEngine::new(EngineOptions {
        initial,
        now: Some(Rc::new(move || read.get())),
        frames: true,
    });
    engine.set_reduced_motion(reduced);
    engine.set_coarse_pointer(coarse);
    Rig { clock, engine }
}

fn rig() -> Rig {
    rig_with(false, false, MotionValues::new())
}

impl Rig {
    fn now(&self) -> f64 {
        self.clock.get()
    }

    fn set_clock(&self, t: f64) {
        self.clock.set(t);
    }

    fn tick(&self, ms: f64) {
        self.clock.set(self.clock.get() + ms);
    }

    /// The frame at `t` commits what waits for one (upstream `commitFrame`).
    fn frame(&self, t: f64) {
        self.engine.frame(t);
    }

    /// A frame now, then the clock and the timers move on (upstream
    /// `advance`).
    fn advance(&self, ms: f64) {
        self.engine.frame(self.now());
        self.tick(ms);
        self.engine.advance(self.now());
    }

    fn animate(&self, to: MotionValues, spring: impl Into<MotionSpring>) -> MotionHandle {
        self.engine
            .animate(to, SpringTransition::new(spring), AnimateOptions::default())
    }

    fn get(&self, key: impl Into<MotionKey>, t: f64) -> f64 {
        self.engine.get(key, Some(t))
    }

    fn get_now(&self, key: impl Into<MotionKey>) -> f64 {
        self.engine.get(key, None)
    }

    fn velocity(&self, key: impl Into<MotionKey>, t: f64) -> f64 {
        self.engine.get_velocity(key, Some(t))
    }

    fn velocity_now(&self, key: impl Into<MotionKey>) -> f64 {
        self.engine.get_velocity(key, None)
    }
}

fn settle() -> MotionSpring {
    springs::role::SETTLE
}

fn spring_curve(spring: SpringDescription, from: f64, to: f64, velocity: f64) -> SpringCurve {
    solve_spring(spring.to_spring(), from, to, velocity)
}

fn settle_curve(from: f64, to: f64, velocity: f64) -> SpringCurve {
    spring_curve(springs::SETTLE, from, to, velocity)
}

fn at_opts(at: f64) -> AnimateOptions {
    AnimateOptions {
        at: Some(at),
        ..Default::default()
    }
}

fn linear(duration: f64, from: f64, to: f64) -> SharedCurve {
    let slope = (to - from) / duration;
    FnCurve::new(
        duration,
        move |t| from + (to - from) * t / duration,
        move |_| slope,
    )
    .shared()
}

// -- one call, one clock ----------------------------------------------------

#[test]
fn puts_everything_one_call_starts_on_one_clock() {
    let r = rig();
    let handle = r.animate(MotionValues::new().x(100.).opacity(0.5), settle());
    r.frame(1000.);
    assert_eq!(handle.started(), Some(1000.));
    let rotate = r.engine.animate(
        MotionValues::new().rotate(90.),
        SpringTransition::new(settle()),
        at_opts(1234.5),
    );
    assert_eq!(rotate.started(), Some(1234.5));
}

#[test]
fn expands_scale_onto_both_axes_and_reads_it_back() {
    let r = rig();
    r.animate(MotionValues::new().scale(0.5), settle());
    r.frame(r.now());
    r.tick(5000.);
    assert_eq!(r.get_now(MotionKey::Scale), 0.5);
    assert_eq!(r.get_now(ScaleY), 0.5);
}

#[test]
fn does_nothing_for_a_value_already_at_its_target_and_at_rest() {
    let r = rig();
    let handle = r.animate(MotionValues::new().x(0.), settle());
    assert_eq!(handle.finished(), Some(true));
    assert!(handle.curve(X).is_none());
}

// -- a call from rest starts at the commit -----------------------------------

#[test]
fn a_block_before_the_first_frame_delays_the_motion_instead_of_skipping_into_it() {
    let r = rig();
    let curve = settle_curve(0., 300., 0.);
    let handle = r.animate(MotionValues::new().x(300.), settle());
    assert_eq!(handle.started(), None);
    assert_eq!(r.get(X, 1000.), 0.);
    r.set_clock(1300.);
    r.frame(1300.);
    assert_eq!(handle.started(), Some(1300.));
    assert_close!(r.get(X, 1300.), 0., 9);
    assert_close!(r.get(X, 1400.), curve.value(0.1), 9);
    assert_close!(handle.value(X, Some(1400.)), curve.value(0.1), 9);
}

#[test]
fn a_call_on_a_settle_tail_starts_at_the_commit_too() {
    let r = rig();
    let curve = settle_curve(0., 300., 0.);
    r.animate(MotionValues::new().x(300.), settle());
    r.frame(1000.);
    let mut t = 0.;
    while ((curve.value(t) - 300.).abs() > 0.5 || curve.velocity(t).abs() > 15.) && t < 5. {
        t += 0.001;
    }
    assert!(t < 5.);
    let at = 1000. + (t * 1000.).ceil();
    r.set_clock(at);
    let from = r.get_now(X);
    let handle = r.animate(MotionValues::new().x(0.), settle());
    assert_eq!(handle.started(), None);
    r.set_clock(at + 80.);
    r.frame(r.now());
    assert_eq!(handle.started(), Some(r.now()));
    assert_close!(r.get(X, r.now()), from, 6);
}

#[test]
fn a_call_while_the_curve_still_moves_starts_at_the_call() {
    let r = rig();
    r.animate(MotionValues::new().x(300.), settle());
    r.frame(1000.);
    r.set_clock(1100.);
    let handle = r.animate(MotionValues::new().x(0.), settle());
    r.frame(1100.);
    assert_eq!(handle.started(), Some(1100.));
}

#[test]
fn next_frame_starts_at_the_first_frame_after_it_however_long_its_task_ran() {
    let r = rig();
    let handle = r.engine.animate(
        MotionValues::new().x(300.),
        SpringTransition::new(settle()),
        AnimateOptions {
            start: Some(StartMode::NextFrame),
            ..Default::default()
        },
    );
    r.tick(800.);
    r.tick(16.);
    r.frame(r.now());
    assert_eq!(handle.started(), Some(r.now()));
}

#[test]
fn an_explicit_start_is_left_to_itself() {
    let r = rig();
    let at = r.now();
    let handle = r.engine.animate(
        MotionValues::new().x(300.),
        SpringTransition::new(settle()),
        AnimateOptions {
            at: Some(at),
            start: Some(StartMode::NextFrame),
            ..Default::default()
        },
    );
    assert_eq!(handle.started(), Some(at));
    r.tick(816.);
    r.frame(r.now());
    assert_eq!(handle.started(), Some(at));
}

#[test]
fn next_frame_starts_a_call_that_continues_motion_now() {
    let r = rig();
    r.animate(MotionValues::new().x(300.), settle());
    r.set_clock(1016.);
    r.frame(1016.);
    r.set_clock(1100.);
    let handle = r.engine.animate(
        MotionValues::new().x(400.),
        SpringTransition::new(settle()),
        AnimateOptions {
            start: Some(StartMode::NextFrame),
            ..Default::default()
        },
    );
    r.frame(1100.);
    assert_eq!(handle.started(), Some(1100.));
}

#[test]
fn holds_its_start_until_its_frame_starts_it() {
    let r = rig();
    let curve = settle_curve(0., 300., 0.);
    r.animate(MotionValues::new().x(300.), settle());
    assert_eq!(r.get_now(X), 0.);
    assert_close!(r.get(X, 1100.), curve.value(0.1), 9);
    r.set_clock(1100.);
    assert_eq!(r.get_now(X), 0.);
    assert_close!(r.velocity(X, 1100.), curve.velocity(0.1), 9);
    r.set_clock(1250.);
    r.frame(1250.);
    assert_close!(r.get(X, 1350.), curve.value(0.1), 9);
}

#[test]
fn a_companion_sampling_a_waiting_curve_ahead_follows_its_shape() {
    let r = rig();
    let curve = settle_curve(0., 300., 0.);
    r.animate(MotionValues::new().x(300.), settle());
    let at = r.now();
    let samples: Vec<f64> = [0., 50., 100., 200.]
        .iter()
        .map(|t| r.get(X, at + t))
        .collect();
    assert_eq!(samples[0], 0.);
    assert!(samples.windows(2).all(|w| w[1] > w[0]));
    assert_close!(samples[2], curve.value(0.1), 9);
}

#[test]
fn finishes_on_the_committed_clock() {
    let r = rig();
    let handle = r.animate(MotionValues::new().x(300.), settle());
    let duration = handle.curve(X).unwrap().duration() * 1000.;
    r.set_clock(1300.);
    r.frame(1300.);
    r.advance(duration - 1.);
    assert_eq!(handle.finished(), None);
    r.advance(2.);
    assert_eq!(handle.finished(), Some(true));
}

#[test]
fn a_retarget_mid_flight_keeps_an_explicit_start() {
    let r = rig();
    let first = r.animate(MotionValues::new().x(300.), settle());
    r.advance(100.);
    assert_eq!(first.started(), Some(1000.));
    let second = r.animate(MotionValues::new().x(0.), settle());
    r.frame(1100.);
    assert_eq!(second.started(), Some(1100.));
}

#[test]
fn a_call_stopped_before_its_first_frame_resolves_at_its_own_time() {
    let r = rig();
    let handle = r.animate(MotionValues::new().x(300.), settle());
    r.engine.stop(None);
    r.set_clock(1300.);
    r.frame(1300.);
    assert_eq!(handle.started(), Some(1000.));
    assert_eq!(handle.finished(), Some(false));
}

#[test]
fn a_second_call_before_the_first_frame_starts_with_the_same_commit() {
    let r = rig();
    let curve = settle_curve(0., 300., 0.);
    r.animate(MotionValues::new().x(300.), settle());
    r.animate(MotionValues::new().y(300.), settle());
    r.set_clock(1300.);
    r.frame(1300.);
    assert_close!(r.get(X, 1400.), curve.value(0.1), 9);
    assert_close!(r.get(Y, 1400.), curve.value(0.1), 9);
}

#[test]
fn a_retarget_before_the_first_frame_starts_from_what_is_on_screen() {
    let r = rig();
    r.animate(MotionValues::new().x(300.), settle());
    r.set_clock(1150.);
    r.animate(MotionValues::new().x(100.), settle());
    assert_eq!(r.get_now(X), 0.);
    r.frame(1150.);
    let first = settle_curve(0., 300., 0.);
    let from0 = settle_curve(0., 100., first.velocity(0.15));
    assert_close!(r.get(X, 1250.), from0.value(0.1), 9);
}

#[test]
fn retargets_not_started_yet_keep_a_flicks_velocity() {
    let r = rig();
    let tracking = SpringDescription::ResponseDamping {
        response: 0.08,
        damping_ratio: 1.,
    };
    for i in 1..=10 {
        r.set_clock(1000. + f64::from(i) * 1000. / 120.);
        r.animate(MotionValues::new().x(-12. * f64::from(i)), tracking);
    }
    assert_eq!(r.get_now(X), 0.);
    let velocity = r.velocity_now(X);
    assert!(velocity < -100.);
    r.animate(MotionValues::new().x(-200.), settle());
    r.frame(r.now());
    let release = settle_curve(0., -200., velocity);
    assert_close!(r.get(X, r.now() + 50.), release.value(0.05), 6);
}

#[test]
fn a_pager_settle_after_held_drags_is_already_moving() {
    let r = rig();
    let tracking = SpringDescription::ResponseDamping {
        response: 0.08,
        damping_ratio: 1.,
    };
    for i in 1..=6 {
        r.set_clock(1000. + f64::from(i) * 1000. / 60.);
        r.animate(MotionValues::new().x(-20. * f64::from(i)), tracking);
    }
    let velocity = r.velocity_now(X);
    assert!(velocity < 0.);
    r.animate(MotionValues::new().x(-400.), settle());
    assert_close!(r.velocity_now(X), velocity, 6);
    r.frame(r.now());
    let first = r.get(X, r.now() + 16.);
    assert_close!(first, settle_curve(0., -400., velocity).value(0.016), 6);
    assert!(first < settle_curve(0., -400., 0.).value(0.016));
}

#[test]
fn calls_from_rest_before_one_frame_start_together() {
    let r = rig();
    let a = r.animate(MotionValues::new().x(300.), settle());
    let b = r.animate(MotionValues::new().y(300.), settle());
    r.set_clock(1300.);
    r.frame(1300.);
    assert_eq!(b.started(), Some(1300.));
    assert_eq!(a.started(), Some(1300.));
}

#[test]
fn rebasing_a_curve_that_waits_for_its_first_frame_keeps_it_in_its_call() {
    let r = rig();
    let curves = MotionCurves::new()
        .with(Opacity, linear(0.3, 1., 0.7))
        .with(X, linear(0.3, 0., 30.))
        .with(Rotate, linear(0.3, 0., 27.));
    r.engine.play(curves, AnimateOptions::default()).unwrap();
    r.engine
        .rebase(MotionValues::new().x(-20.), RebaseOptions::default());
    r.animate(MotionValues::new().y(50.), settle());
    r.set_clock(1300.);
    r.frame(1300.);
    assert_close!(r.get(Rotate, 1400.), 9., 6);
    assert_close!(r.get(X, 1400.), -10., 6);
}

#[test]
fn a_rebase_and_a_retarget_in_the_task_of_a_drop_start_on_the_frames_clock() {
    let r = rig();
    r.engine.set(MotionValues::new().y(30.));
    r.engine.animate(
        MotionValues::new().y(0.),
        SpringTransition::new(settle()).velocity(MotionValues::new().y(0.)),
        AnimateOptions::default(),
    );
    r.engine.rebase(
        MotionValues::new().y(56.),
        RebaseOptions {
            at: Some(1000.),
            ..Default::default()
        },
    );
    let home = r.engine.animate(
        MotionValues::new().y(0.),
        SpringTransition::new(settle()),
        at_opts(1000.),
    );
    r.set_clock(1004.);
    r.frame(1004.);
    assert_eq!(home.started(), Some(1004.));
    let curve = settle_curve(86., 0., 0.);
    for t in [0., 16., 50., 100., 200.] {
        assert_close!(r.get(Y, 1004. + t), curve.value(t / 1000.), 6);
    }
}

#[test]
fn writing_another_axis_before_the_first_frame_keeps_the_call_on_its_start() {
    let r = rig();
    let handle = r.animate(MotionValues::new().opacity(0.5).x(100.), settle());
    r.set_clock(1100.);
    r.engine.set(MotionValues::new().y(10.));
    r.set_clock(1300.);
    r.frame(1300.);
    assert_eq!(handle.started(), Some(1300.));
    assert_close!(r.get(X, 1400.), settle_curve(0., 100., 0.).value(0.1), 6);
}

#[test]
fn a_call_from_rest_and_work_after_it_start_at_the_commit() {
    let r = rig();
    let first = r.animate(MotionValues::new().x(100.), settle());
    r.set_clock(1300.);
    r.animate(MotionValues::new().y(50.), settle());
    r.set_clock(1310.);
    r.frame(1310.);
    assert_eq!(first.started(), Some(1310.));
    assert_close!(r.get(X, 1310.), 0., 6);
}

#[test]
fn a_later_at_than_a_waiting_call_keeps_its_own_clock() {
    let r = rig();
    r.animate(MotionValues::new().opacity(0.), settle());
    r.set_clock(1011.);
    let fade = r.engine.animate(
        MotionValues::new().opacity(1.),
        SpringTransition::new(settle()),
        at_opts(1011.),
    );
    assert_eq!(fade.started(), Some(1011.));
}

#[test]
fn an_earlier_at_joins_a_waiting_call_on_its_channel() {
    let r = rig();
    r.set_clock(1020.);
    let x = r.animate(MotionValues::new().x(100.), settle());
    let y = r.engine.animate(
        MotionValues::new().y(50.),
        SpringTransition::new(settle()),
        at_opts(1000.),
    );
    r.set_clock(1300.);
    r.frame(1300.);
    assert_eq!(x.started(), Some(1300.));
    assert_eq!(y.started(), Some(1300.));
    assert_close!(r.get(Y, 1316.), settle_curve(0., 50., 0.).value(0.036), 6);
}

#[test]
fn under_reduced_motion_on_a_fine_pointer_a_move_leaves_a_finished_fade_alone() {
    let r = rig_with(true, false, MotionValues::new());
    r.engine.set(MotionValues::new().opacity(0.));
    r.animate(MotionValues::new().opacity(1.), settle());
    r.advance(10_000.);
    r.animate(MotionValues::new().x(40.), settle());
    assert_eq!(r.get_now(Opacity), 1.);
    assert_eq!(r.get_now(X), 40.);
}

// -- additive ------------------------------------------------------------------

fn old_at(t: f64) -> f64 {
    let old = settle_curve(0., 300., 0.);
    let end = old.settle_time(0.05);
    if t >= end { 300. } else { old.value(t) }
}

fn additive() -> AnimateOptions {
    AnimateOptions {
        additive: true,
        ..Default::default()
    }
}

#[test]
fn additive_starts_from_the_old_curve_and_reads_as_their_sum() {
    let r = rig();
    r.animate(MotionValues::new().x(300.), settle());
    r.frame(1000.);
    r.set_clock(1150.);
    let (x, v) = (r.get_now(X), r.velocity_now(X));
    r.engine.animate(
        MotionValues::new().x(100.),
        SpringTransition::new(settle()),
        additive(),
    );
    assert_close!(r.get_now(X), x, 9);
    assert_close!(r.velocity_now(X), v, 6);
    let delta = settle_curve(0., -200., 0.);
    for t in [0.02, 0.1, 0.4] {
        assert_close!(
            r.get(X, 1150. + t * 1000.),
            old_at(0.15 + t) + delta.value(t),
            6
        );
    }
    r.advance(10_000.);
    assert_eq!(r.get_now(X), 100.);
}

#[test]
fn an_additive_played_reversal_keeps_the_old_velocity() {
    let ease = cubic_bezier(timing::curves::MOVE);
    let reversal = |from: f64, to: f64| -> SharedCurve {
        FnCurve::new(
            0.35,
            move |t| from + (to - from) * ease.ease((t / 0.35).min(1.)),
            move |t| {
                (to - from) / 0.35
                    * ((ease.ease((t / 0.35 + 1e-4).min(1.))
                        - ease.ease((t / 0.35 - 1e-4).max(0.)))
                        / 2e-4)
            },
        )
        .shared()
    };
    let steps = |additive: bool| -> Vec<f64> {
        let r = rig();
        r.engine.animate(
            MotionValues::new().x(300.),
            SpringTransition::new(settle()),
            at_opts(1000.),
        );
        r.set_clock(1120.);
        let from = r.get_now(X);
        r.engine
            .play(
                MotionCurves::new().with(X, reversal(from, 0.)),
                AnimateOptions {
                    at: Some(1120.),
                    additive,
                    ..Default::default()
                },
            )
            .unwrap();
        [30., 60., 100.]
            .iter()
            .map(|d| (r.get(X, 1120. + d) - old_at((120. + d) / 1000.)).abs())
            .collect()
    };
    let replaced = steps(false);
    let added = steps(true);
    for i in 0..added.len() {
        assert!(added[i] < replaced[i] / 2.);
    }
    assert_close!(added[0], 300. * ease.ease(30. / 350.), 6);
}

#[test]
fn additive_nests() {
    let r = rig();
    r.animate(MotionValues::new().x(300.), settle());
    r.frame(1000.);
    r.set_clock(1100.);
    r.engine.animate(
        MotionValues::new().x(100.),
        SpringTransition::new(settle()),
        additive(),
    );
    r.set_clock(1200.);
    let (x, v) = (r.get_now(X), r.velocity_now(X));
    r.engine.animate(
        MotionValues::new().x(250.),
        SpringTransition::new(settle()),
        additive(),
    );
    assert_close!(r.get_now(X), x, 9);
    assert_close!(r.velocity_now(X), v, 6);
    let first = settle_curve(0., -200., 0.);
    let second = settle_curve(0., 150., 0.);
    assert_close!(
        r.get(X, 1300.),
        old_at(0.3) + first.value(0.2) + second.value(0.1),
        6
    );
    r.advance(10_000.);
    assert_eq!(r.get_now(X), 250.);
}

#[test]
fn additive_replaces_for_a_property_at_rest_or_given_a_velocity() {
    let r = rig();
    r.engine.animate(
        MotionValues::new().x(300.),
        SpringTransition::new(settle()),
        additive(),
    );
    r.frame(1000.);
    assert_close!(r.get(X, 1100.), settle_curve(0., 300., 0.).value(0.1), 9);
    r.set_clock(1100.);
    let x = r.get_now(X);
    r.engine.animate(
        MotionValues::new().x(0.),
        SpringTransition::new(settle()).velocity(MotionValues::new().x(0.)),
        additive(),
    );
    assert_close!(r.get(X, 1200.), settle_curve(x, 0., 0.).value(0.1), 9);
}

// -- start modes ------------------------------------------------------------------

#[test]
fn presented_leaves_the_element_at_rest_and_starts_two_frames_later() {
    let r = rig();
    r.engine.set(MotionValues::new().x(20.));
    let handle = r.engine.animate(
        MotionValues::new().x(100.),
        SpringTransition::new(settle()),
        AnimateOptions {
            start: Some(StartMode::Presented),
            ..Default::default()
        },
    );
    assert_eq!(r.get_now(X), 20.);
    r.set_clock(1016.);
    r.frame(1016.);
    assert_eq!(r.get_now(X), 20.);
    r.set_clock(1033.);
    r.frame(1033.);
    assert_eq!(handle.started(), None);
    r.set_clock(1050.);
    r.frame(1050.);
    assert_eq!(handle.started(), Some(1050.));
    assert_close!(r.get(X, 1150.), settle_curve(20., 100., 0.).value(0.1), 9);
}

#[test]
fn presented_another_change_before_then_starts_it_first() {
    let r = rig();
    r.engine.animate(
        MotionValues::new().x(100.),
        SpringTransition::new(settle()),
        AnimateOptions {
            start: Some(StartMode::Presented),
            ..Default::default()
        },
    );
    r.animate(MotionValues::new().y(50.), settle());
    r.frame(r.now());
    assert_close!(
        r.get(X, r.now() + 100.),
        settle_curve(0., 100., 0.).value(0.1),
        9
    );
}

#[test]
fn presented_a_retarget_of_a_moving_property_starts_now() {
    let r = rig();
    r.animate(MotionValues::new().x(300.), settle());
    r.frame(1000.);
    r.set_clock(1100.);
    let (from, velocity) = (r.get_now(X), r.velocity_now(X));
    let handle = r.engine.animate(
        MotionValues::new().x(0.),
        SpringTransition::new(settle()),
        AnimateOptions {
            start: Some(StartMode::Presented),
            ..Default::default()
        },
    );
    r.frame(1100.);
    assert_eq!(handle.started(), Some(1100.));
    assert_close!(r.get(X, 1100.), from, 6);
    assert_close!(r.velocity(X, 1100.), velocity, 3);
    assert_close!(
        r.get(X, 1200.),
        settle_curve(from, 0., velocity).value(0.1),
        6
    );
}

#[test]
fn commit_a_retarget_is_dated_by_the_frame_from_the_value_on_screen() {
    let r = rig();
    r.animate(MotionValues::new().x(300.), settle());
    r.frame(1000.);
    r.set_clock(1100.);
    let (from, velocity) = (r.get_now(X), r.velocity_now(X));
    let handle = r.engine.animate(
        MotionValues::new().x(0.),
        SpringTransition::new(settle()),
        AnimateOptions {
            start: Some(StartMode::Commit),
            ..Default::default()
        },
    );
    assert_close!(r.get_now(X), from, 9);
    r.set_clock(1130.);
    r.frame(1130.);
    assert_eq!(handle.started(), Some(1130.));
    assert_close!(
        r.get(X, 1230.),
        settle_curve(from, 0., velocity).value(0.1),
        6
    );
}

#[test]
fn a_handle_hands_out_the_curve_it_plays() {
    let r = rig();
    let handle = r.animate(MotionValues::new().y(400.), springs::SHEET_PRESENT);
    let curve = handle.curve(Y).unwrap();
    assert!(handle.curve(X).is_none());
    r.frame(1000.);
    let y = spring_curve(springs::SHEET_PRESENT, 0., 400., 0.);
    for t in [0., 0.05, 0.2] {
        assert_close!(curve.at(t), y.value(t), 9);
        assert_close!(curve.velocity(t), y.velocity(t), 6);
        assert_close!(r.get(Y, 1000. + t * 1000.), y.value(t), 9);
    }
    assert_eq!(curve.at(curve.duration()), 400.);
}

// -- animatorFor ----------------------------------------------------------------

#[test]
fn animator_for_gives_every_caller_the_same_engine() {
    let mut registry = EngineRegistry::new();
    let a = registry.animator_for("a", EngineOptions::default());
    a.set(MotionValues::new().x(5.));
    assert_eq!(
        registry
            .animator_for("a", EngineOptions::default())
            .get(X, None),
        5.
    );
    assert_eq!(
        registry
            .animator_for("b", EngineOptions::default())
            .get(X, None),
        0.
    );
}

#[test]
fn animator_for_composes_two_callers_axes() {
    let clock = Rc::new(Cell::new(1000.));
    let mut registry = EngineRegistry::new();
    let options = || {
        let read = clock.clone();
        EngineOptions {
            now: Some(Rc::new(move || read.get())),
            frames: true,
            ..Default::default()
        }
    };
    let swipe = registry.animator_for("row", options());
    swipe.animate(
        MotionValues::new().x(-120.),
        SpringTransition::new(settle()),
        AnimateOptions::default(),
    );
    swipe.frame(clock.get());
    clock.set(1050.);
    swipe.advance(1050.);
    let x = settle_curve(0., -120., 0.);
    let removal = registry.animator_for("row", options());
    removal.animate(
        MotionValues::new().y(-44.),
        SpringTransition::new(settle()),
        AnimateOptions::default(),
    );
    removal.frame(clock.get());
    assert_close!(removal.get(X, Some(clock.get() + 30.)), x.value(0.08), 9);
    let y = settle_curve(0., -44., 0.);
    assert_close!(swipe.get(Y, Some(clock.get() + 30.)), y.value(0.03), 9);
}

// -- rebase ------------------------------------------------------------------------

#[test]
fn rebase_keeps_a_springs_spring_velocity_and_target() {
    let r = rig();
    let handle = r.animate(MotionValues::new().y(100.), settle());
    r.advance(60.);
    let (value, velocity) = (r.get_now(Y), r.velocity_now(Y));
    r.engine
        .rebase(MotionValues::new().y(-40.), RebaseOptions::default());
    assert_close!(r.get(Y, r.now()), value - 40., 9);
    assert_close!(r.velocity(Y, r.now()), velocity, 6);
    let curve = settle_curve(value - 40., 100., velocity);
    assert_close!(r.get(Y, r.now() + 30.), curve.value(0.03), 9);
    r.advance(5000.);
    assert_eq!(r.get_now(Y), 100.);
    assert_eq!(handle.finished(), Some(true));
}

#[test]
fn rebase_offsets_a_resting_value() {
    let r = rig();
    r.engine.set(MotionValues::new().y(10.));
    r.engine
        .rebase(MotionValues::new().y(-20.), RebaseOptions::default());
    assert_eq!(r.get_now(Y), -10.);
    assert!(r.engine.at_rest(None));
}

// -- atRest ----------------------------------------------------------------------

#[test]
fn at_rest_follows_the_last_curve_retargets_included() {
    let r = rig();
    r.animate(MotionValues::new().y(100.), settle());
    let y = [MotionKey::from(Y)];
    r.advance(100.);
    let second = r.animate(MotionValues::new().y(0.), settle());
    r.frame(r.now());
    let end = second.curve(Y).unwrap().duration() * 1000.;
    r.advance(end - 1.);
    assert!(!r.engine.at_rest(Some(&y)));
    r.advance(2.);
    assert!(r.engine.at_rest(Some(&y)));
    assert_eq!(second.finished(), Some(true));
}

#[test]
fn at_rest_at_once_at_rest() {
    assert!(rig().engine.at_rest(None));
    assert!(rig().engine.is_settled());
}

// -- presentation values, retargeting, velocity (spec:AST-094 FR6) ----------------

#[test]
fn hands_a_release_velocity_to_the_spring() {
    let r = rig();
    r.engine.set(MotionValues::new().y(200.));
    r.engine.animate(
        MotionValues::new().y(0.),
        SpringTransition::new(springs::SHEET_DETENT).velocity(MotionValues::new().y(1500.)),
        AnimateOptions::default(),
    );
    assert_eq!(r.get_now(Y), 200.);
    assert_close!(r.velocity_now(Y), 1500., 6);
}

fn emphasis_moving() -> Rig {
    let r = rig();
    r.engine.animate(
        MotionValues::new().x(300.),
        SpringTransition::new(springs::role::EMPHASIS).velocity(MotionValues::new().x(800.)),
        AnimateOptions::default(),
    );
    r.advance(120.);
    r
}

fn emphasis_shown(time: f64) -> (f64, f64) {
    let old = spring_curve(springs::EMPHASIS, 0., 300., 800.);
    let t = (time - 1000.) / 1000.;
    (old.value(t), old.velocity(t))
}

#[test]
fn a_retarget_starts_from_the_value_and_the_velocity_on_screen() {
    let r = emphasis_moving();
    let (value, velocity) = (r.get_now(X), r.velocity_now(X));
    let handle = r.animate(MotionValues::new().x(-100.), springs::role::EMPHASIS);
    r.frame(r.now());
    assert_eq!(handle.started(), Some(r.now()));
    assert_close!(r.get(X, r.now()), value, 9);
    assert_close!(r.velocity(X, r.now()), velocity, 6);
    let curve = spring_curve(springs::EMPHASIS, value, -100., velocity);
    assert_close!(r.get(X, r.now() + 90.), curve.value(0.09), 9);
}

#[test]
fn a_retarget_whose_frame_comes_late_starts_there_from_the_screen() {
    let r = emphasis_moving();
    r.animate(MotionValues::new().x(-100.), springs::role::EMPHASIS);
    r.tick(200.);
    r.frame(r.now());
    let (shown, shown_velocity) = emphasis_shown(r.now());
    assert_close!(r.get(X, r.now()), shown, 6);
    assert_close!(r.velocity(X, r.now()), shown_velocity, 4);
    let curve = spring_curve(springs::EMPHASIS, shown, -100., shown_velocity);
    assert_close!(r.get(X, r.now() + 90.), curve.value(0.09), 6);
}

#[test]
fn a_retarget_whose_frame_comes_on_time_keeps_its_own_clock() {
    let r = emphasis_moving();
    let call = r.now();
    r.animate(MotionValues::new().x(-100.), springs::role::EMPHASIS);
    r.tick(30.);
    r.frame(r.now());
    let (shown, shown_velocity) = emphasis_shown(call);
    let curve = spring_curve(springs::EMPHASIS, shown, -100., shown_velocity);
    assert_close!(
        r.get(X, r.now() + 90.),
        curve.value((r.now() + 90. - call) / 1000.),
        6
    );
}

#[test]
fn calls_before_a_late_frame_start_there_from_the_curve_the_screen_shows() {
    let r = emphasis_moving();
    r.animate(MotionValues::new().x(-100.), springs::role::EMPHASIS);
    r.tick(10.);
    r.animate(MotionValues::new().x(40.), settle());
    r.tick(190.);
    r.frame(r.now());
    let (shown, shown_velocity) = emphasis_shown(r.now());
    let curve = settle_curve(shown, 40., shown_velocity);
    assert_close!(r.get(X, r.now() + 90.), curve.value(0.09), 6);
}

#[test]
fn a_rebase_before_the_late_frame_moves_the_place_on_screen_with_the_element() {
    let r = emphasis_moving();
    r.animate(MotionValues::new().x(-100.), springs::role::EMPHASIS);
    r.engine
        .rebase(MotionValues::new().x(-40.), RebaseOptions::default());
    r.tick(200.);
    r.frame(r.now());
    let (shown, velocity) = emphasis_shown(r.now());
    assert_close!(r.get(X, r.now()), shown - 40., 6);
    assert_close!(r.velocity(X, r.now()), velocity, 4);
}

#[test]
fn an_additive_retarget_whose_frame_comes_late_adds_to_the_curve_on_screen() {
    let r = emphasis_moving();
    r.engine.animate(
        MotionValues::new().x(-100.),
        SpringTransition::new(settle()),
        additive(),
    );
    r.tick(200.);
    r.frame(r.now());
    let now = r.now();
    assert_close!(r.get(X, now), emphasis_shown(now).0, 6);
    let difference = settle_curve(0., -400., 0.);
    let (shown, velocity) = emphasis_shown(now);
    let replaced = settle_curve(shown, -100., velocity);
    let mut apart: f64 = 0.;
    let mut ms = 50.;
    while ms <= 600. {
        let added = emphasis_shown(now + ms).0 + difference.value(ms / 1000.);
        assert_close!(r.get(X, now + ms), added, 6);
        apart = apart.max((added - replaced.value(ms / 1000.)).abs());
        ms += 50.;
    }
    assert!(apart > 1.);
}

#[test]
fn stop_before_a_late_frame_freezes_where_the_screen_shows() {
    let r = emphasis_moving();
    r.animate(MotionValues::new().x(-100.), springs::role::EMPHASIS);
    r.tick(200.);
    r.engine.stop(None);
    assert_close!(r.get_now(X), emphasis_shown(r.now()).0, 6);
}

#[test]
fn a_retarget_that_starts_at_its_late_frame_finishes_at_the_end_of_that_curve() {
    let r = emphasis_moving();
    let handle = r.animate(MotionValues::new().x(250.), springs::role::QUICK_CHANGE);
    let own = handle.curve(X).unwrap().duration() * 1000.;
    r.tick(own + 50.);
    r.engine.advance(r.now());
    assert_eq!(handle.finished(), None);
    r.frame(r.now());
    let left = r.engine.rest_time(None) - r.now();
    r.advance(left - 20.);
    assert_eq!(handle.finished(), None);
    r.advance(40.);
    assert_eq!(handle.finished(), Some(true));
}

#[test]
fn a_played_curve_keeps_its_own_clock_when_its_frame_comes_late() {
    let r = emphasis_moving();
    let call = r.now();
    let handle = r
        .engine
        .play(
            MotionCurves::new().with(X, linear(0.3, 0., 30.)),
            AnimateOptions::default(),
        )
        .unwrap();
    r.tick(200.);
    r.frame(r.now());
    assert_eq!(handle.started(), Some(call));
    assert_close!(r.get(X, call + 100.), 10., 9);
}

#[test]
fn a_second_propertys_retarget_before_the_same_late_frame_starts_there_too() {
    let r = rig();
    r.engine.animate(
        MotionValues::new().x(300.).y(300.),
        SpringTransition::new(springs::role::EMPHASIS)
            .velocity(MotionValues::new().x(800.).y(800.)),
        AnimateOptions::default(),
    );
    r.advance(120.);
    r.animate(MotionValues::new().x(-100.), springs::role::EMPHASIS);
    r.tick(20.);
    r.animate(MotionValues::new().y(-100.), springs::role::EMPHASIS);
    r.tick(25.);
    r.frame(r.now());
    let (shown, velocity) = emphasis_shown(r.now());
    for property in [X, Y] {
        assert_close!(r.get(property, r.now()), shown, 6);
        assert_close!(r.velocity(property, r.now()), velocity, 4);
    }
    let curve = spring_curve(springs::EMPHASIS, shown, -100., velocity);
    assert_close!(r.get(Y, r.now() + 90.), curve.value(0.09), 6);
}

#[test]
fn a_late_frame_keeps_a_velocity_the_call_handed_the_spring() {
    let r = emphasis_moving();
    r.engine.animate(
        MotionValues::new().x(-100.),
        SpringTransition::new(springs::role::EMPHASIS).velocity(MotionValues::new().x(-900.)),
        AnimateOptions::default(),
    );
    r.tick(200.);
    r.frame(r.now());
    let shown = emphasis_shown(r.now()).0;
    assert_close!(r.get(X, r.now()), shown, 6);
    assert_close!(r.velocity(X, r.now()), -900., 4);
    let curve = spring_curve(springs::EMPHASIS, shown, -100., -900.);
    assert_close!(r.get(X, r.now() + 90.), curve.value(0.09), 6);
}

#[test]
fn a_late_frame_takes_the_velocity_on_screen_when_the_call_hands_the_curves_own() {
    let r = emphasis_moving();
    let own = r.velocity_now(X);
    r.engine.animate(
        MotionValues::new().x(-100.),
        SpringTransition::new(springs::role::EMPHASIS).velocity(MotionValues::new().x(own)),
        AnimateOptions::default(),
    );
    r.tick(200.);
    r.frame(r.now());
    let (shown, velocity) = emphasis_shown(r.now());
    assert_close!(r.get(X, r.now()), shown, 6);
    assert_close!(r.velocity(X, r.now()), velocity, 4);
}

#[test]
fn velocity_is_continuous_across_a_retarget() {
    for spring in [
        springs::SETTLE,
        springs::QUICK_CHANGE,
        springs::SWITCH_THUMB_MOVE,
        springs::NAVIGATION_PUSH,
    ] {
        let r = rig();
        r.animate(MotionValues::new().x(200.), spring);
        for step in [16., 50., 120.] {
            r.advance(step);
            let before = r.velocity(X, r.now());
            let value = r.get(X, r.now());
            r.animate(
                MotionValues::new().x(if value > 100. { 0. } else { 200. }),
                spring,
            );
            assert_close!(r.velocity(X, r.now()), before, 6);
            assert_close!(r.get(X, r.now()), value, 9);
        }
    }
}

#[test]
fn a_value_that_keeps_moving_through_a_retarget_of_its_other_axis_stays_on_its_curve() {
    let r = rig();
    r.animate(MotionValues::new().x(300.).y(200.), settle());
    let y = settle_curve(0., 200., 0.);
    r.advance(100.);
    r.animate(MotionValues::new().x(0.), settle());
    let mut t = r.now();
    while t < r.now() + 400. {
        assert_close!(r.get(Y, t), y.value((t - 1000.) / 1000.), 9);
        t += 7.;
    }
}

#[test]
fn stop_freezes_at_the_presentation_value() {
    let r = rig();
    r.animate(MotionValues::new().x(300.), settle());
    r.advance(80.);
    let value = r.get_now(X);
    r.engine.stop(None);
    r.advance(500.);
    assert_eq!(r.get_now(X), value);
    assert_eq!(r.velocity_now(X), 0.);
}

#[test]
fn a_handle_reads_retargets_and_stops_its_own_properties() {
    let r = rig();
    let handle = r.animate(MotionValues::new().x(100.), settle());
    r.advance(50.);
    assert_eq!(handle.value(X, None), r.get_now(X));
    assert_eq!(handle.velocity(X, None), r.velocity_now(X));
    let next = handle
        .retarget(MotionValues::new().x(50.), None, AnimateOptions::default())
        .unwrap();
    assert_eq!(handle.finished(), Some(false));
    next.stop();
    assert_eq!(next.finished(), Some(false));
}

#[test]
fn a_drag_on_translate_leaves_a_running_scale_alone() {
    let r = rig();
    let scale = r.animate(MotionValues::new().scale(0.95), settle());
    r.engine.set(MotionValues::new().x(12.));
    r.engine.set(MotionValues::new().x(20.));
    assert_eq!(scale.finished(), None);
    assert_eq!(r.get_now(X), 20.);
}

#[test]
fn finished_resolves_true_when_the_curve_ends_untouched() {
    let r = rig();
    let handle = r.animate(MotionValues::new().x(40.), springs::role::QUICK_CHANGE);
    r.advance(100.);
    assert_eq!(handle.finished(), None);
    r.advance(5000.);
    assert_eq!(handle.finished(), Some(true));
}

#[test]
fn finished_resolves_false_as_soon_as_a_property_is_retargeted_set_or_stopped() {
    let r = rig();
    let a = r.animate(MotionValues::new().x(40.), springs::role::QUICK_CHANGE);
    r.animate(MotionValues::new().x(80.), springs::role::QUICK_CHANGE);
    assert_eq!(a.finished(), Some(false));
    let b = r.animate(MotionValues::new().y(40.), springs::role::QUICK_CHANGE);
    r.engine.set(MotionValues::new().y(3.));
    assert_eq!(b.finished(), Some(false));
    let c = r.animate(MotionValues::new().rotate(40.), springs::role::QUICK_CHANGE);
    r.engine.stop(Some(&[Rotate.into()]));
    assert_eq!(c.finished(), Some(false));
}

#[test]
fn finished_resolves_on_time() {
    let r = rig();
    let handle = r.animate(MotionValues::new().x(40.), springs::role::QUICK_CHANGE);
    let end = handle.curve(X).unwrap().duration() * 1000.;
    r.advance(end - 50.);
    assert_eq!(handle.finished(), None);
    r.advance(100.);
    assert_eq!(handle.finished(), Some(true));
}

#[test]
fn dispose_cancels_and_settles_everything() {
    let r = rig();
    let handle = r.animate(
        MotionValues::new().x(40.).opacity(0.),
        springs::role::QUICK_CHANGE,
    );
    r.engine.dispose();
    assert_eq!(handle.finished(), Some(false));
    assert!(r.engine.is_disposed());
    assert!(r.engine.at_rest(None));
}

// -- Reduced Motion (design:reduced-motion) ------------------------------------

const FADE: f64 = timing::REDUCED_MOTION_CROSS_FADE_MS;

fn reduced(coarse: bool) -> Rig {
    rig_with(true, coarse, MotionValues::new())
}

fn push() -> MotionSpring {
    MotionSpring::inline(springs::NAVIGATION_PUSH)
}

#[test]
fn a_call_naming_the_opacity_a_running_dip_heads_to_keeps_the_dip() {
    let r = reduced(true);
    r.animate(MotionValues::new().x(100.), push());
    r.frame(r.now());
    let start = r.now();
    r.advance(FADE / 4.);
    let handle = r.animate(MotionValues::new().opacity(1.), settle());
    assert_close!(r.get(Opacity, start + FADE / 2.), 0., 6);
    r.advance(FADE / 4. + 1.);
    assert_eq!(r.get_now(X), 100.);
    assert_close!(r.get(Opacity, start + FADE), 1., 6);
    r.advance(FADE + 1.);
    assert_eq!(r.get_now(X), 100.);
    assert_eq!(r.get_now(Opacity), 1.);
    assert_eq!(handle.finished(), Some(true));
}

#[test]
fn a_release_played_as_a_dip_then_its_state_layer_fades_once() {
    let r = reduced(true);
    let touch = AnimateOptions {
        touch_release: true,
        ..Default::default()
    };
    let mv = r
        .engine
        .play(MotionCurves::new().with(X, linear(0.3, 0., 100.)), touch)
        .unwrap();
    let state = r.engine.animate(
        MotionValues::new().opacity(1.),
        SpringTransition::new(settle()),
        touch,
    );
    r.frame(r.now());
    r.advance(FADE + 1.);
    assert_eq!(r.get_now(X), 100.);
    assert_eq!(r.get_now(Opacity), 1.);
    assert_eq!(mv.finished(), Some(true));
    assert_eq!(state.finished(), Some(true));
}

#[test]
fn presented_a_call_that_moves_and_fades_in_from_transparent_lands_its_travel() {
    let r = reduced(true);
    r.engine.set(MotionValues::new().x(44.).opacity(0.));
    r.engine.animate(
        MotionValues::new().x(0.).opacity(1.),
        SpringTransition::new(settle()),
        AnimateOptions {
            start: Some(StartMode::Presented),
            ..Default::default()
        },
    );
    r.frame(r.now());
    r.frame(r.now());
    r.advance(FADE + 16.);
    assert_eq!(r.get_now(X), 0.);
    assert_close!(r.get(Opacity, r.now()), 1., 9);
}

#[test]
fn fades_for_200_ms_at_most() {
    const { assert!(FADE > 0. && FADE <= 200.) };
}

#[test]
fn never_animates_a_transform_on_either_pointer() {
    for coarse in [true, false] {
        let r = reduced(coarse);
        let handles = [
            r.animate(MotionValues::new().x(100.), push()),
            r.animate(
                MotionValues::new().y(400.).opacity(0.),
                springs::SHEET_DISMISS,
            ),
            r.engine.animate(
                MotionValues::new().scale(0.9),
                SpringTransition::new(springs::role::QUICK_CHANGE),
                AnimateOptions {
                    crossfade_as: Some(CrossfadeAs::Exit),
                    ..Default::default()
                },
            ),
            r.engine
                .play(
                    MotionCurves::new().with(Rotate, linear(0.3, 0., 9.)),
                    AnimateOptions::default(),
                )
                .unwrap(),
        ];
        for handle in &handles {
            for property in MotionProperty::TRAVEL {
                assert!(handle.curve(property).is_none());
            }
        }
        // Every travel value is at its start or its end at every time.
        for t in [1000., 1010., 1050., 1100., 1150., 1300.] {
            let x = r.get(X, t);
            assert!(x == 0. || x == 100., "{t}: {x}");
        }
        r.advance(1000.);
        assert_eq!(r.get_now(X), 100.);
        assert_eq!(r.get_now(Rotate), 9.);
    }
}

#[test]
fn a_move_with_no_opacity_dips_on_the_elements_own_opacity_then_hands_it_back() {
    let r = reduced(true);
    r.engine.set_own_opacity(0.6);
    r.animate(MotionValues::new().x(100.), push());
    r.frame(r.now());
    assert_close!(r.get(Opacity, 1000.), 0.6, 9);
    r.advance(FADE / 2.);
    assert_eq!(r.get_now(X), 100.);
    assert_close!(r.get(Opacity, r.now() + FADE / 2.), 0.6, 9);
    r.advance(FADE / 2. + 1.);
    assert!(!r.engine.writes_opacity());
    assert_eq!(r.get_now(Opacity), 1.);
    assert_close!(r.engine.shown_opacity(None), 0.6, 9);
}

#[test]
fn overlapping_dips_on_the_elements_own_opacity_never_leave_it_pinned() {
    for fading_out in [true, false] {
        let r = reduced(true);
        r.engine.set_own_opacity(0.6);
        r.animate(MotionValues::new().x(100.), push());
        r.frame(r.now());
        r.advance(if fading_out {
            FADE / 4.
        } else {
            FADE / 2. + 20.
        });
        r.animate(MotionValues::new().x(200.), push());
        r.frame(r.now());
        r.advance(FADE * 2. + 1.);
        assert_eq!(r.get_now(X), 200.);
        assert!(!r.engine.writes_opacity());
        assert_eq!(r.get_now(Opacity), 1.);
    }
}

#[test]
fn a_move_that_overlaps_a_running_fade_lands_only_once_invisible() {
    let r = reduced(true);
    r.engine.set(MotionValues::new().opacity(0.));
    r.animate(MotionValues::new().opacity(1.), push());
    r.advance(FADE / 3.);
    let start = r.now();
    assert!(r.get_now(Opacity) > 0.1);
    r.animate(MotionValues::new().x(100.), push());
    r.advance(FADE / 2. - 2.);
    assert_close!(r.now() - start, FADE / 2. - 2., 9);
    assert_eq!(r.get_now(X), 0.);
    r.advance(4.);
    assert_eq!(r.get_now(X), 100.);
    assert!(r.get_now(Opacity) < 0.1);
    r.advance(FADE);
    assert_close!(r.get_now(Opacity), 1., 9);
}

#[test]
fn a_second_calls_fade_re_times_the_first_calls_pending_landing() {
    let r = reduced(true);
    r.animate(MotionValues::new().x(100.), push());
    r.advance(FADE / 4.);
    let start = r.now();
    r.animate(MotionValues::new().y(50.), push());
    r.advance(FADE / 2. - 2.);
    assert_close!(r.now() - start, FADE / 2. - 2., 9);
    assert_eq!(r.get_now(X), 0.);
    assert_eq!(r.get_now(Y), 0.);
    r.advance(4.);
    assert_eq!(r.get_now(X), 100.);
    assert_eq!(r.get_now(Y), 50.);
    assert!(r.get_now(Opacity) < 0.1);
}

#[test]
fn a_stop_during_a_dip_finishes_the_fade() {
    let r = reduced(true);
    r.animate(MotionValues::new().x(100.), push());
    r.advance(FADE / 2. - 5.);
    assert!(r.get_now(Opacity) < 0.1);
    r.engine.stop(None);
    assert_eq!(r.get_now(Opacity), 1.);
}

#[test]
fn a_stop_during_a_dip_on_the_elements_own_opacity_hands_it_back() {
    let r = reduced(true);
    r.engine.set_own_opacity(0.6);
    r.animate(MotionValues::new().x(100.), push());
    r.advance(FADE / 4.);
    r.engine.stop(None);
    assert!(!r.engine.writes_opacity());
    assert_eq!(r.get_now(Opacity), 1.);
}

#[test]
fn a_call_that_does_not_own_opacity_keeps_a_borrowed_dip_handing_it_back() {
    let still = reduced(true);
    still.engine.set_own_opacity(0.6);
    still.animate(MotionValues::new().x(100.), push());
    still.advance(FADE / 4.);
    still.animate(MotionValues::new().x(100.), push());
    still.advance(FADE + 1.);
    assert!(!still.engine.writes_opacity());

    let changed = reduced(true);
    changed.engine.set_own_opacity(0.6);
    changed.animate(MotionValues::new().x(100.), push());
    changed.advance(FADE / 4.);
    changed.engine.set_reduced_motion(false);
    changed.animate(MotionValues::new().y(20.), push());
    changed.advance(2000.);
    assert!(!changed.engine.writes_opacity());
}

#[test]
fn an_opacity_the_engine_wrote_is_owned_a_dip_returns_to_it() {
    let r = reduced(true);
    r.engine.set_own_opacity(0.);
    r.engine.set(MotionValues::new().opacity(1.));
    assert!(r.engine.writes_opacity());
    r.animate(MotionValues::new().x(100.), push());
    r.advance(FADE + 1.);
    assert!(r.engine.writes_opacity());
    assert_eq!(r.get_now(Opacity), 1.);
}

#[test]
fn borrows_nothing_from_a_hidden_element_so_a_later_move_still_dips() {
    let r = reduced(true);
    r.engine.set_own_opacity(0.);
    r.animate(MotionValues::new().x(100.), push());
    assert_eq!(r.get_now(X), 100.);
    r.advance(FADE + 1.);
    r.engine.set_own_opacity(1.);
    r.animate(MotionValues::new().x(200.), push());
    let start = r.now();
    let mut moved = false;
    while r.now() <= start + FADE {
        r.advance(2.);
        if r.get_now(X) != 100. {
            assert!(r.get_now(Opacity) < 0.1);
            moved = true;
            break;
        }
    }
    assert!(moved);
}

#[test]
fn a_stop_that_finishes_a_dips_fade_lands_the_move_with_it() {
    let r = reduced(true);
    r.animate(MotionValues::new().x(100.), push());
    r.advance(FADE / 4.);
    r.engine.stop(Some(&[Opacity.into()]));
    assert_eq!(r.get_now(X), 100.);
    assert_eq!(r.get_now(Opacity), 1.);
}

fn spring_back(coarse: bool, touch_release: bool) {
    let r = reduced(coarse);
    r.engine.set(MotionValues::new().x(-80.));
    let start = r.now();
    let curves = coast(
        &r.engine,
        CoastVelocity {
            x: Some(0.),
            y: None,
        },
        CoastOptions {
            bounds_x: Some(CoastBounds { min: 0., max: 100. }),
            at: Some(start),
            ..Default::default()
        },
    );
    r.engine.decay(
        curves,
        DecayOptions {
            at: Some(start),
            touch_release,
        },
    );
    assert_eq!(r.get_now(X), -80.);
    let mut moved = false;
    while r.now() <= start + FADE {
        r.advance(2.);
        if !moved && r.get_now(X) != -80. {
            moved = true;
            assert!(r.get_now(Opacity) < 0.1);
        }
    }
    assert!(moved);
    assert_eq!(r.get_now(X), 0.);
}

#[test]
fn a_release_past_an_edge_springs_back_as_a_cross_fade_not_momentum() {
    spring_back(true, false);
}

#[test]
fn a_spring_back_a_touch_release_starts_cross_fades_on_a_fine_pointer_too() {
    spring_back(false, true);
}

#[test]
fn a_release_with_one_axis_past_an_edge_is_one_handle() {
    let r = reduced(true);
    r.engine.set(MotionValues::new().x(-80.).y(0.));
    let start = r.now();
    let curves = coast(
        &r.engine,
        CoastVelocity {
            x: Some(0.),
            y: Some(500.),
        },
        CoastOptions {
            bounds_x: Some(CoastBounds { min: 0., max: 100. }),
            at: Some(start),
            ..Default::default()
        },
    );
    let handle = r.engine.decay(
        curves,
        DecayOptions {
            at: Some(start),
            touch_release: false,
        },
    );
    let properties = handle.properties();
    assert!(properties.contains(&X) && properties.contains(&Y));
    r.advance(FADE / 4.);
    handle.stop();
    let stopped_y = r.get_now(Y);
    assert_eq!(r.get_now(X), -80.);
    assert_eq!(r.get_now(Opacity), 1.);
    r.advance(FADE + 1.);
    assert_eq!(r.get_now(X), -80.);
    assert_eq!(r.get_now(Y), stopped_y);
}

#[test]
fn a_carried_move_keeps_its_own_call() {
    for set in [true, false] {
        let r = reduced(true);
        let a = r.animate(MotionValues::new().x(100.), push());
        r.advance(FADE / 4.);
        let b = r.animate(MotionValues::new().y(50.), push());
        if set {
            r.engine.set(MotionValues::new().x(40.));
        } else {
            a.stop();
        }
        r.advance(FADE * 2. + 1.);
        assert_eq!(a.finished(), Some(false));
        assert_eq!(b.finished(), Some(true));
        assert_eq!(r.get_now(X), if set { 40. } else { 0. });
        assert_eq!(r.get_now(Y), 50.);
    }
}

#[test]
fn an_earlier_handles_stop_leaves_a_later_calls_move_alone() {
    let r = reduced(true);
    let a = r.animate(MotionValues::new().x(100.), push());
    r.advance(FADE / 4.);
    let b = r.animate(MotionValues::new().x(200.), push());
    a.stop();
    r.advance(FADE * 2. + 1.);
    assert_eq!(r.get_now(X), 200.);
    assert_eq!(a.finished(), Some(false));
    assert_eq!(b.finished(), Some(true));
}

#[test]
fn a_retargeted_call_settles_interrupted_and_its_retarget_lands() {
    let r = reduced(true);
    let a = r.animate(MotionValues::new().x(100.), push());
    r.advance(FADE / 4.);
    let b = a
        .retarget(MotionValues::new().x(150.), None, AnimateOptions::default())
        .unwrap();
    r.advance(FADE * 2. + 1.);
    assert_eq!(r.get_now(X), 150.);
    assert_eq!(a.finished(), Some(false));
    assert_eq!(b.finished(), Some(true));
}

#[test]
fn a_waiting_call_whose_own_opacity_a_later_call_retargets_settles_interrupted() {
    let r = reduced(true);
    let dismiss = r.animate(
        MotionValues::new().y(800.).opacity(0.),
        springs::SHEET_DISMISS,
    );
    r.advance(FADE / 4.);
    let show = r.animate(MotionValues::new().opacity(1.), settle());
    for _ in 0..12 {
        r.advance(FADE / 8.);
    }
    assert_eq!(r.get_now(Opacity), 1.);
    assert_eq!(dismiss.finished(), Some(false));
    assert_eq!(show.finished(), Some(true));
}

#[test]
fn a_move_taken_back_mid_dip_fades_straight_back_up() {
    let r = reduced(true);
    let mv = r.animate(MotionValues::new().x(100.), push());
    r.advance(FADE / 4.);
    let shown = r.get_now(Opacity);
    assert!(shown < 1.);
    let back = r.animate(MotionValues::new().x(0.), push());
    let mut lowest = shown;
    for _ in 0..12 {
        r.advance(FADE / 8.);
        lowest = lowest.min(r.get_now(Opacity));
    }
    assert!(lowest >= shown - 1e-6);
    assert_eq!(r.get_now(X), 0.);
    assert_eq!(r.get_now(Opacity), 1.);
    assert_eq!(mv.finished(), Some(false));
    assert_eq!(back.finished(), Some(true));
}

fn role_opts(role: CrossfadeAs) -> AnimateOptions {
    AnimateOptions {
        crossfade_as: Some(role),
        ..Default::default()
    }
}

#[test]
fn an_entrance_by_role_caught_mid_exit_fades_in_from_where_the_exit_has_it() {
    let r = reduced(true);
    r.engine.animate(
        MotionValues::new().y(40.),
        SpringTransition::new(push()),
        role_opts(CrossfadeAs::Exit),
    );
    r.advance(FADE / 4.);
    let shown = r.get_now(Opacity);
    assert!(shown < 1.);
    r.engine.animate(
        MotionValues::new().y(0.),
        SpringTransition::new(push()),
        role_opts(CrossfadeAs::Entrance),
    );
    let mut lowest = shown;
    for _ in 0..12 {
        r.advance(FADE / 8.);
        lowest = lowest.min(r.get_now(Opacity));
    }
    assert!(lowest >= shown - 1e-6);
    assert_eq!(r.get_now(Y), 0.);
    assert_eq!(r.get_now(Opacity), 1.);
}

#[test]
fn a_caught_dismissal_fades_straight_back_up() {
    let r = reduced(true);
    let dismiss = r.animate(
        MotionValues::new().y(800.).opacity(0.),
        springs::SHEET_DISMISS,
    );
    r.advance(FADE / 4.);
    let shown = r.get_now(Opacity);
    let present = r.animate(
        MotionValues::new().y(0.).opacity(1.),
        springs::SHEET_PRESENT,
    );
    r.frame(r.now());
    let mut lowest = shown;
    let start = r.now();
    while r.now() <= start + FADE * 2. {
        r.advance(2.);
        lowest = lowest.min(r.get_now(Opacity));
    }
    assert!(lowest >= shown - 1e-6);
    assert_eq!(r.get_now(Y), 0.);
    assert_eq!(r.get_now(Opacity), 1.);
    assert_eq!(dismiss.finished(), Some(false));
    assert_eq!(present.finished(), Some(true));
}

#[test]
fn a_handles_stop_mid_dip_freezes_its_waiting_travel() {
    let r = reduced(true);
    let handle = r.animate(MotionValues::new().x(100.), push());
    r.advance(FADE / 4.);
    handle.stop();
    assert_eq!(r.get_now(X), 0.);
    assert_eq!(r.get_now(Opacity), 1.);
    r.advance(FADE + 1.);
    assert_eq!(r.get_now(X), 0.);
    assert_eq!(handle.finished(), Some(false));
}

#[test]
fn a_second_move_during_a_borrowed_dip_keeps_the_borrow() {
    let r = reduced(true);
    r.engine.set_own_opacity(0.6);
    r.animate(MotionValues::new().x(100.), push());
    r.advance(FADE / 4.);
    r.animate(MotionValues::new().y(50.), push());
    r.advance(FADE / 2.);
    assert_close!(r.get(Opacity, r.now() + FADE / 2.), 0.6, 9);
    r.advance(FADE + 1.);
    assert!(!r.engine.writes_opacity());
    assert_eq!(r.get_now(Opacity), 1.);
}

#[test]
fn a_stop_during_a_dip_whose_opacity_a_call_owns_ends_where_the_fade_was_heading() {
    let r = reduced(true);
    r.engine.set(MotionValues::new().opacity(0.8));
    r.animate(MotionValues::new().x(100.), push());
    r.frame(r.now());
    r.advance(50.);
    r.engine.stop(None);
    assert_close!(r.get_now(Opacity), 0.8, 9);
}

#[test]
fn an_opacity_change_mid_dip_lands_the_waiting_move_only_once_invisible() {
    let r = reduced(true);
    r.animate(MotionValues::new().x(100.), push());
    r.frame(r.now());
    r.advance(20.);
    r.animate(MotionValues::new().opacity(0.), push());
    assert_eq!(r.get_now(X), 0.);
    r.advance(FADE - 2.);
    assert_eq!(r.get_now(X), 0.);
    r.advance(4.);
    assert_eq!(r.get_now(X), 100.);
    assert_close!(r.get_now(Opacity), 0., 9);
}

#[test]
fn a_second_borrowed_dip_opens_at_the_opacity_on_screen() {
    let r = reduced(true);
    r.engine.set_own_opacity(0.6);
    r.animate(MotionValues::new().x(100.), push());
    r.frame(r.now());
    r.advance(FADE / 4.);
    let before = r.get_now(Opacity);
    assert!(before < 0.6);
    r.animate(MotionValues::new().x(200.), push());
    assert_close!(r.get_now(Opacity), before, 6);
}

#[test]
fn a_fade_by_role_on_the_elements_own_opacity_ends_on_it_and_hands_it_back() {
    for role in [CrossfadeAs::Entrance, CrossfadeAs::Exit] {
        let r = reduced(true);
        r.engine.set_own_opacity(0.6);
        r.engine.animate(
            MotionValues::new().x(100.),
            SpringTransition::new(push()),
            role_opts(role),
        );
        r.frame(r.now());
        let mut highest: f64 = 0.;
        let mut t = r.now();
        while t <= r.now() + FADE {
            highest = highest.max(r.get(Opacity, t));
            t += 5.;
        }
        assert_close!(highest, 0.6, 9);
        r.advance(FADE * 2. + 1.);
        assert_eq!(r.get_now(X), 100.);
        assert!(!r.engine.writes_opacity());
    }
}

#[test]
fn a_move_set_before_its_dip_lands_does_not_report_finished() {
    let r = reduced(true);
    let handle = r.animate(MotionValues::new().x(100.), push());
    r.frame(r.now());
    r.advance(20.);
    r.engine.set(MotionValues::new().x(40.));
    assert_eq!(handle.finished(), Some(false));
    r.advance(FADE + 1.);
    assert_eq!(r.get_now(X), 40.);
}

#[test]
fn dispose_settles_a_move_still_waiting_to_land_and_the_calls_it_carries() {
    let r = reduced(true);
    let first = r.animate(MotionValues::new().x(100.), push());
    r.frame(r.now());
    r.advance(10.);
    let second = r.animate(MotionValues::new().y(50.), push());
    r.advance(10.);
    r.engine.dispose();
    assert_eq!(first.finished(), Some(false));
    assert_eq!(second.finished(), Some(false));
}

#[test]
fn a_caught_exit_by_role_hands_the_elements_own_opacity_back() {
    let r = reduced(true);
    r.engine.set_own_opacity(0.6);
    r.engine.animate(
        MotionValues::new().x(100.),
        SpringTransition::new(push()),
        role_opts(CrossfadeAs::Exit),
    );
    r.frame(r.now());
    r.advance(50.);
    r.engine.stop(None);
    assert!(!r.engine.writes_opacity());
}

#[test]
fn dispose_mid_dip_or_mid_exit_leaves_the_element_visible_where_it_was_going() {
    let dip = reduced(true);
    dip.engine.set(MotionValues::new().opacity(0.8));
    dip.animate(MotionValues::new().x(100.), push());
    dip.frame(dip.now());
    dip.advance(50.);
    dip.engine.dispose();
    assert_close!(dip.get_now(Opacity), 0.8, 9);
    assert_eq!(dip.get_now(X), 100.);
    let exit = reduced(true);
    exit.engine.animate(
        MotionValues::new().x(100.),
        SpringTransition::new(push()),
        role_opts(CrossfadeAs::Exit),
    );
    exit.frame(exit.now());
    exit.advance(50.);
    exit.engine.dispose();
    assert_eq!(exit.get_now(Opacity), 1.);
    assert_eq!(exit.get_now(X), 100.);
}

fn move_ease(x: f64) -> f64 {
    cubic_bezier(timing::curves::MOVE).ease(x)
}

#[test]
fn a_move_dips_fades_out_lands_while_transparent_fades_back_in() {
    let r = reduced(true);
    let handle = r.animate(MotionValues::new().x(100.), push());
    r.frame(r.now());
    assert_eq!(r.get_now(X), 0.);
    assert_close!(r.get(Opacity, 1000. + FADE / 4.), 1. - move_ease(0.5), 9);
    assert!(handle.curve(X).is_none());
    r.advance(FADE / 2.);
    assert_eq!(r.get_now(X), 100.);
    assert_close!(r.get_now(Opacity), 0., 9);
    assert_close!(r.get(Opacity, r.now() + FADE / 4.), move_ease(0.5), 9);
}

#[test]
fn a_landing_run_late_still_ends_the_dip_on_time() {
    let r = reduced(true);
    r.animate(MotionValues::new().x(100.), push());
    r.frame(1000.);
    r.tick(FADE / 2. + 80.);
    r.engine.advance(1000. + FADE / 2.);
    assert_eq!(r.get_now(X), 100.);
    assert_close!(r.get_now(Opacity), 0., 9);
    assert_eq!(r.get(Opacity, 1000. + FADE), 1.);
}

#[test]
fn a_busy_main_thread_keeps_the_element_transparent_longer_never_in_the_wrong_place() {
    let r = reduced(true);
    let handle = r.animate(MotionValues::new().x(100.), push());
    r.frame(1000.);
    // Upstream reads the old place before the late timer runs; here a read
    // runs what came due first, so the landing is read already late.
    r.tick(FADE / 2. + 300.);
    r.engine.advance(1000. + FADE / 2. + 300.);
    assert_eq!(r.get_now(X), 100.);
    r.advance(FADE);
    assert_eq!(r.get_now(Opacity), 1.);
    assert_eq!(handle.finished(), Some(true));
}

#[test]
fn two_calls_in_one_frame_both_land() {
    let r = reduced(true);
    r.engine.set(MotionValues::new().x(6.3).y(84.).scale(0.969));
    let touch = AnimateOptions {
        touch_release: true,
        ..Default::default()
    };
    let position = r.engine.animate(
        MotionValues::new().x(0.).y(0.),
        SpringTransition::new(springs::SHEET_PRESENT),
        touch,
    );
    let scale = r.engine.animate(
        MotionValues::new().scale(1.),
        SpringTransition::new(springs::role::QUICK_CHANGE),
        touch,
    );
    r.frame(r.now());
    r.advance(FADE / 2.);
    assert_eq!(r.get_now(X), 0.);
    assert_eq!(r.get_now(ScaleX), 1.);
    assert_close!(r.get_now(Opacity), 0., 9);
    r.advance(FADE);
    assert_eq!(r.get_now(Y), 0.);
    assert_eq!(r.get_now(Opacity), 1.);
    assert_eq!(position.finished(), Some(true));
    assert_eq!(scale.finished(), Some(true));
}

#[test]
fn a_move_during_a_dips_fade_out_dips_once_more_and_ends_visible() {
    let r = reduced(true);
    r.animate(MotionValues::new().y(100.), springs::SHEET_PRESENT);
    r.frame(r.now());
    r.advance(FADE / 4.);
    r.animate(MotionValues::new().y(120.), springs::SHEET_PRESENT);
    r.frame(r.now());
    r.advance(FADE / 2.);
    r.advance(FADE);
    assert_eq!(r.get_now(Y), 120.);
    assert_eq!(r.get_now(Opacity), 1.);
}

#[test]
fn a_drop_its_rebase_and_the_move_to_the_new_slot_in_one_task_dip_once() {
    let r = reduced(true);
    r.engine.set(MotionValues::new().y(30.));
    let at = r.now();
    r.engine.animate(
        MotionValues::new().y(0.),
        SpringTransition::new(settle()).velocity(MotionValues::new().y(0.)),
        AnimateOptions {
            at: Some(at),
            touch_release: true,
            ..Default::default()
        },
    );
    r.tick(8.);
    let at2 = r.now();
    r.engine.rebase(
        MotionValues::new().y(56.),
        RebaseOptions {
            at: Some(at2),
            ..Default::default()
        },
    );
    r.engine.animate(
        MotionValues::new().y(0.),
        SpringTransition::new(settle()),
        AnimateOptions {
            at: Some(at2),
            touch_release: true,
            ..Default::default()
        },
    );
    r.frame(r.now());
    let mut lowest: f64 = 1.;
    let mut t = 0.;
    while t <= FADE / 2. {
        let opacity = r.get(Opacity, at2 + t);
        lowest = lowest.min(opacity);
        if opacity > 0.01 {
            assert_eq!(r.get(Y, at2 + t), 86.);
        }
        t += 10.;
    }
    assert!(lowest <= 0.1);
    r.advance(FADE / 2.);
    assert_eq!(r.get_now(Y), 0.);
    r.advance(FADE);
    assert_eq!(r.get_now(Y), 0.);
    assert_eq!(r.get_now(Opacity), 1.);
}

fn lands_while_transparent(r: &Rig, from_x: f64) -> f64 {
    let start = r.now();
    let mut landed_at = None;
    while r.now() <= start + FADE * 2. {
        r.advance(2.);
        if landed_at.is_none() && r.get_now(X) != from_x {
            landed_at = Some(r.get_now(Opacity));
        }
    }
    landed_at.expect("the move lands")
}

#[test]
fn a_later_opacity_change_carries_a_waiting_move_to_its_own_invisible_moment() {
    let r = reduced(true);
    let mv = r.animate(MotionValues::new().x(100.), push());
    r.frame(r.now());
    r.animate(MotionValues::new().opacity(0.5), settle());
    assert_eq!(r.get_now(X), 0.);
    assert!(lands_while_transparent(&r, 0.) < 0.05);
    assert_eq!(r.get_now(X), 100.);
    assert_close!(r.get_now(Opacity), 0.5, 9);
    assert_eq!(mv.finished(), Some(true));
}

#[test]
fn a_move_whose_opacity_rises_from_or_falls_to_a_visible_value_lands_only_while_transparent() {
    for (from, to) in [(0.5, 1.), (1., 0.5)] {
        let r = reduced(true);
        r.engine.set(MotionValues::new().opacity(from));
        r.animate(MotionValues::new().x(100.).opacity(to), push());
        assert_eq!(r.get_now(X), 0.);
        assert!(lands_while_transparent(&r, 0.) < 0.05);
        assert_eq!(r.get_now(X), 100.);
        assert_close!(r.get_now(Opacity), to, 9);
    }
}

#[test]
fn an_entrance_lands_in_place_and_fades_in() {
    let r = reduced(true);
    r.engine.set(MotionValues::new().y(400.).opacity(0.));
    let handle = r.animate(
        MotionValues::new().y(0.).opacity(1.),
        springs::SHEET_PRESENT,
    );
    r.frame(r.now());
    assert_eq!(r.get_now(Y), 0.);
    assert!(handle.curve(Y).is_none());
    assert_close!(r.get(Opacity, 1000. + FADE / 2.), move_ease(0.5), 9);
    assert_close!(handle.curve(Opacity).unwrap().duration() * 1000., FADE, 6);
}

#[test]
fn an_exit_fades_out_in_place_then_lands() {
    let r = reduced(true);
    r.animate(
        MotionValues::new().y(400.).opacity(0.),
        springs::SHEET_DISMISS,
    );
    r.frame(r.now());
    assert_close!(r.get(Opacity, 1000. + FADE / 2.), 1. - move_ease(0.5), 9);
    r.advance(FADE - 1.);
    assert_eq!(r.get_now(Y), 0.);
    r.advance(1.);
    assert_eq!(r.get_now(Y), 400.);
    assert_eq!(r.get_now(Opacity), 0.);
}

#[test]
fn an_opacity_change_on_a_spring_becomes_the_timed_fade() {
    let r = reduced(true);
    let handle = r.animate(MotionValues::new().opacity(0.), springs::ALERT_DISMISS);
    r.frame(r.now());
    assert_close!(r.get(Opacity, 1000. + FADE / 2.), 1. - move_ease(0.5), 9);
    assert_eq!(r.get(Opacity, 1000. + FADE), 0.);
    assert_close!(handle.curve(Opacity).unwrap().duration() * 1000., FADE, 6);
}

#[test]
fn a_played_curve_opacity_alone_included_becomes_the_timed_fade() {
    let r = reduced(true);
    let handle = r
        .engine
        .play(
            MotionCurves::new().with(Opacity, linear(0.6, 1., 0.)),
            AnimateOptions::default(),
        )
        .unwrap();
    r.frame(r.now());
    assert_eq!(r.get(Opacity, 1000. + FADE), 0.);
    assert_close!(handle.curve(Opacity).unwrap().duration() * 1000., FADE, 6);
}

#[test]
fn a_slide_without_opacity_takes_its_direction_from_crossfade_as() {
    let entrance = reduced(true);
    entrance.engine.set(MotionValues::new().x(300.));
    entrance.engine.animate(
        MotionValues::new().x(0.),
        SpringTransition::new(push()),
        role_opts(CrossfadeAs::Entrance),
    );
    entrance.frame(entrance.now());
    assert_eq!(entrance.get(X, 1000.), 0.);
    assert_eq!(entrance.get(Opacity, 1000.), 0.);
    assert_close!(entrance.get(Opacity, 1000. + FADE / 2.), move_ease(0.5), 9);
    assert_eq!(entrance.get(Opacity, 1000. + FADE), 1.);

    let exit = reduced(true);
    exit.engine.animate(
        MotionValues::new().y(800.),
        SpringTransition::new(springs::SHEET_DISMISS),
        role_opts(CrossfadeAs::Exit),
    );
    exit.frame(exit.now());
    assert_close!(
        exit.get(Opacity, exit.now() + FADE / 2.),
        1. - move_ease(0.5),
        9
    );
    exit.advance(FADE);
    assert_eq!(exit.get_now(Y), 800.);
    assert_eq!(exit.get_now(Opacity), 1.);
}

#[test]
fn a_later_call_before_the_landing_takes_the_property_over() {
    let r = reduced(true);
    r.animate(MotionValues::new().x(100.), push());
    r.advance(FADE / 4.);
    r.engine.set(MotionValues::new().x(40.));
    r.advance(FADE);
    assert_eq!(r.get_now(X), 40.);
}

#[test]
fn completes_at_once_on_a_fine_pointer_opacity_included() {
    let r = reduced(false);
    r.animate(MotionValues::new().x(100.).opacity(0.5), push());
    r.engine
        .play(
            MotionCurves::new().with(Y, linear(0.4, 0., 40.)),
            AnimateOptions::default(),
        )
        .unwrap();
    assert_eq!(r.get_now(X), 100.);
    assert_eq!(r.get_now(Opacity), 0.5);
    assert_close!(r.get_now(Y), 40., 9);
}

#[test]
fn fades_on_a_fine_pointer_when_a_touch_release_started_the_call() {
    let r = reduced(false);
    r.engine.animate(
        MotionValues::new().y(400.).opacity(0.),
        SpringTransition::new(springs::SHEET_DISMISS),
        AnimateOptions {
            touch_release: true,
            ..Default::default()
        },
    );
    r.frame(r.now());
    assert_close!(r.get(Opacity, 1000. + FADE / 2.), 1. - move_ease(0.5), 9);
    r.advance(FADE);
    assert_eq!(r.get_now(Y), 400.);
}

#[test]
fn no_tempo_lengthens_the_fade() {
    let r = reduced(true);
    r.engine.set_tempo(3.);
    r.animate(MotionValues::new().opacity(0.), settle());
    r.frame(r.now());
    assert_eq!(r.get(Opacity, 1000. + FADE), 0.);
}

#[test]
fn says_what_it_makes_of_a_call() {
    assert_eq!(rig().engine.reduced_motion(false), ReducedMotion::Off);
    assert_eq!(
        reduced(true).engine.reduced_motion(false),
        ReducedMotion::Crossfade
    );
    assert_eq!(
        reduced(false).engine.reduced_motion(false),
        ReducedMotion::Instant
    );
    assert_eq!(
        reduced(false).engine.reduced_motion(true),
        ReducedMotion::Crossfade
    );
}

#[test]
fn crossfade_as_changes_nothing_without_reduced_motion() {
    let r = rig();
    r.engine.set(MotionValues::new().y(800.));
    let handle = r.engine.animate(
        MotionValues::new().y(0.),
        SpringTransition::new(springs::SHEET_PRESENT),
        role_opts(CrossfadeAs::Entrance),
    );
    r.frame(r.now());
    assert!(handle.curve(Opacity).is_none());
    assert!(r.velocity(Y, 1010.) < 0.);
}

// -- play: any closed-form curve ------------------------------------------------

fn press() -> super::SpringRampCurve {
    super::solve_spring_toward_ramp(springs::CONTEXT_MENU_LIFT.to_spring(), 1., 0., 1., -0.16)
}

fn press_curve() -> SharedCurve {
    let (a, b) = (press(), press());
    FnCurve::new(0.266, move |t| a.value(t), move |t| b.velocity(t)).shared()
}

#[test]
fn play_reads_the_curve_back() {
    let r = rig();
    let handle = r
        .engine
        .play(
            MotionCurves::new().with(MotionKey::Scale, press_curve()),
            AnimateOptions::default(),
        )
        .unwrap();
    r.frame(1000.);
    assert_eq!(handle.started(), Some(1000.));
    let press = press();
    let mut t = 0.;
    while t < 266. {
        assert_close!(
            r.get(MotionKey::Scale, 1000. + t),
            press.value(t / 1000.),
            12
        );
        assert_close!(r.velocity(ScaleY, 1000. + t), press.velocity(t / 1000.), 12);
        t += 0.5;
    }
    assert_close!(r.get(MotionKey::Scale, 1266.), press.value(0.266), 12);
    assert_eq!(r.velocity(MotionKey::Scale, 1266.), 0.);
}

#[test]
fn a_curve_played_and_rebased_in_one_task_plays_shifted() {
    let r = rig();
    r.engine
        .play(
            MotionCurves::new().with(X, linear(0.4, 0., 48.)),
            AnimateOptions::default(),
        )
        .unwrap();
    r.engine
        .rebase(MotionValues::new().x(-16.), RebaseOptions::default());
    r.frame(r.now());
    assert_close!(r.get(X, r.now()), -16., 6);
    assert_close!(r.get(X, r.now() + 400.), 32., 6);
}

#[test]
fn play_finishes_true_at_its_end_false_when_interrupted() {
    let r = rig();
    let done = r
        .engine
        .play(
            MotionCurves::new().with(X, linear(0.2, 0., 20.)),
            AnimateOptions::default(),
        )
        .unwrap();
    r.advance(250.);
    assert_eq!(done.finished(), Some(true));
    let cut = r
        .engine
        .play(
            MotionCurves::new().with(X, linear(0.2, 20., 40.)),
            AnimateOptions::default(),
        )
        .unwrap();
    r.advance(50.);
    r.engine.set(MotionValues::new().x(0.));
    assert_eq!(cut.finished(), Some(false));
}

#[test]
fn a_later_animate_starts_from_the_played_value_and_velocity() {
    let r = rig();
    r.engine
        .play(
            MotionCurves::new().with(MotionKey::Scale, press_curve()),
            AnimateOptions::default(),
        )
        .unwrap();
    r.advance(120.);
    let (value, velocity) = (
        r.get_now(MotionKey::Scale),
        r.velocity_now(MotionKey::Scale),
    );
    assert_close!(value, press().value(0.12), 12);
    r.animate(
        MotionValues::new().scale(1.06),
        springs::role::CONTEXT_MENU_LIFT,
    );
    assert_close!(r.get_now(MotionKey::Scale), value, 9);
    assert_close!(r.velocity_now(MotionKey::Scale), velocity, 9);
}

fn flat(value: f64) -> SharedCurve {
    FnCurve::new(0., move |_| value, |_| 0.).shared()
}

#[test]
fn a_zero_length_curve_puts_the_value_in_place() {
    let r = rig();
    r.engine
        .play(
            MotionCurves::new().with(Opacity, flat(0.4)),
            AnimateOptions::default(),
        )
        .unwrap();
    assert_eq!(r.get_now(Opacity), 0.4);
    assert!(r.engine.writes_opacity());
}

#[test]
fn under_reduced_motion_a_played_travel_cross_fades_to_its_end() {
    let r = reduced(true);
    let handle = r
        .engine
        .play(
            MotionCurves::new().with(X, linear(0.3, 0., 90.)),
            AnimateOptions::default(),
        )
        .unwrap();
    r.frame(r.now());
    assert!(handle.curve(Opacity).is_some());
    assert!(handle.curve(X).is_none());
    r.advance(FADE);
    assert_close!(r.get_now(X), 90., 9);
}

#[test]
fn a_played_handle_retargets_only_with_a_spring() {
    let r = rig();
    let handle = r
        .engine
        .play(
            MotionCurves::new().with(X, linear(0.3, 0., 90.)),
            AnimateOptions::default(),
        )
        .unwrap();
    assert_eq!(
        handle
            .retarget(MotionValues::new().x(0.), None, AnimateOptions::default())
            .unwrap_err(),
        MotionError::NeedsSpring
    );
    handle
        .retarget(
            MotionValues::new().x(0.),
            Some(SpringTransition::new(settle())),
            AnimateOptions::default(),
        )
        .unwrap();
    assert_close!(r.velocity_now(X), 300., 9);
}

#[test]
fn an_explicit_axis_wins_over_scale_in_either_order() {
    let r = rig();
    let first = MotionCurves {
        scale_y: Some(flat(0.5)),
        scale: Some(flat(2.)),
        ..Default::default()
    };
    r.engine.play(first, AnimateOptions::default()).unwrap();
    assert_eq!(r.get_now(ScaleX), 2.);
    assert_eq!(r.get_now(ScaleY), 0.5);
    let second = MotionCurves::new()
        .with(MotionKey::Scale, flat(3.))
        .with(ScaleX, flat(0.25));
    r.engine.play(second, AnimateOptions::default()).unwrap();
    assert_eq!(r.get_now(ScaleX), 0.25);
    assert_eq!(r.get_now(ScaleY), 3.);
}

#[test]
fn play_rejects_a_curve_without_a_finite_duration() {
    let r = rig();
    let endless = FnCurve::new(f64::INFINITY, |_| 0., |_| 0.).shared();
    assert!(matches!(
        r.engine.play(
            MotionCurves::new().with(X, endless),
            AnimateOptions::default()
        ),
        Err(MotionError::BadDuration(_))
    ));
}

// -- decay: momentum curves -----------------------------------------------------

fn velocity_y(v: f64) -> CoastVelocity {
    CoastVelocity {
        x: None,
        y: Some(v),
    }
}

fn velocity_x(v: f64) -> CoastVelocity {
    CoastVelocity {
        x: Some(v),
        y: None,
    }
}

#[test]
fn decay_plays_a_coast_and_comes_to_rest_where_it_projects() {
    let r = rig();
    let curves = coast(&r.engine, velocity_y(-2400.), CoastOptions::default());
    r.engine.decay(curves, DecayOptions::default());
    r.frame(r.now());
    let glide = solve_decay(0., -2400., None);
    for t in [0., 16., 100., 400., 1200.] {
        assert_close!(r.get(Y, 1000. + t), glide.value(t / 1000.), 9);
    }
    assert_close!(r.get(Y, 1e7), glide.rest, 9);
}

#[test]
fn a_coast_that_reaches_an_edge_hands_its_velocity_to_the_edge_spring() {
    let r = rig();
    r.engine.set(MotionValues::new().y(-100.));
    let options = CoastOptions {
        bounds_y: Some(CoastBounds {
            min: -2000.,
            max: 0.,
        }),
        ..Default::default()
    };
    let curves = coast(&r.engine, velocity_y(3000.), options);
    r.engine.decay(curves, DecayOptions::default());
    r.frame(r.now());
    let glide = solve_decay(-100., 3000., None);
    let contact = 1000. + glide.time_to_reach(0.).unwrap() * 1000.;
    assert_close!(r.get(Y, contact), 0., 6);
    assert_close!(
        r.velocity(Y, contact - 1e-6),
        r.velocity(Y, contact + 1e-6),
        2
    );
    assert!(r.get(Y, contact + 60.) > 0.);
    assert_eq!(r.get(Y, 1e7), 0.);
}

#[test]
fn a_release_past_an_edge_springs_straight_back_with_the_release_velocity() {
    let r = rig();
    r.engine.set(MotionValues::new().x(60.));
    let options = CoastOptions {
        bounds_x: Some(CoastBounds {
            min: -500.,
            max: 0.,
        }),
        ..Default::default()
    };
    let curves = coast(&r.engine, velocity_x(400.), options);
    r.engine.decay(curves, DecayOptions::default());
    r.frame(r.now());
    assert_close!(r.velocity_now(X), 400., 6);
    let curve = spring_curve(scroll::BOUNCE, 60., 0., 400.);
    assert_close!(r.get(X, 1100.), curve.value(0.1), 9);
}

#[test]
fn a_coast_continues_a_running_spring_from_its_presentation_value() {
    let r = rig();
    r.animate(MotionValues::new().x(300.), settle());
    r.frame(r.now());
    r.tick(80.);
    let from = r.get_now(X);
    assert!(from > 0.);
    let curves = coast(&r.engine, velocity_x(900.), CoastOptions::default());
    r.engine.decay(curves, DecayOptions::default());
    r.frame(r.now());
    assert_close!(r.get(X, r.now()), from, 9);
    assert_close!(r.velocity(X, r.now()), 900., 6);
}

#[test]
fn decay_plays_any_curve_as_given_and_a_zero_length_one_rests() {
    let r = rig();
    r.engine.decay(
        DecayCurves {
            x: Some(linear(0.5, 0., 100.)),
            y: Some(flat(5.)),
        },
        DecayOptions::default(),
    );
    r.frame(r.now());
    assert_close!(r.get(X, 1250.), 50., 9);
    assert_close!(r.get(X, 1e7), 100., 9);
    assert_eq!(r.get_now(Y), 5.);
}

#[test]
fn decay_keeps_moving_under_reduced_motion() {
    let r = reduced(false);
    let curves = coast(&r.engine, velocity_x(1000.), CoastOptions::default());
    let handle = r.engine.decay(curves, DecayOptions::default());
    r.frame(r.now());
    assert!(r.velocity(X, 1016.) > 900.);
    assert!(handle.curve(Opacity).is_none());
    assert!(!r.engine.writes_opacity());
}

#[test]
fn a_coast_plays_the_table_edge_spring_whatever_the_tempo() {
    let r = rig();
    r.engine.set_tempo(1.5);
    r.engine.set(MotionValues::new().x(60.));
    let options = CoastOptions {
        bounds_x: Some(CoastBounds {
            min: -500.,
            max: 0.,
        }),
        at: Some(1000.),
        ..Default::default()
    };
    let curves = coast(&r.engine, velocity_x(0.), options);
    r.engine.decay(
        curves,
        DecayOptions {
            at: Some(1000.),
            ..Default::default()
        },
    );
    let expected = spring_curve(scroll::BOUNCE, 60., 0., 0.);
    for ms in [50., 150., 400.] {
        assert_close!(r.get(X, 1000. + ms), expected.value(ms / 1000.), 3);
    }
}

// -- the theme's reduced-motion mark, tempo ------------------------------------

#[test]
fn reduced_motion_lands_a_move_at_once_on_a_fine_pointer() {
    let r = reduced(false);
    r.animate(MotionValues::new().x(100.), settle());
    assert_eq!(r.get_now(X), 100.);
    let on = rig();
    on.animate(MotionValues::new().x(100.), settle());
    assert_eq!(on.get_now(X), 0.);
}

#[test]
fn the_engine_plays_the_tables_spring_at_tempo_1() {
    let r = rig();
    r.engine.set(MotionValues::new().y(300.));
    r.engine.animate(
        MotionValues::new().y(0.),
        SpringTransition::new(settle()),
        at_opts(0.),
    );
    let table = settle_curve(300., 0., 0.);
    for ms in [50., 150., 400.] {
        assert_close!(r.get(Y, ms), table.value(ms / 1000.), 3);
    }
}

#[test]
fn the_engine_plays_a_roles_spring_at_the_themes_tempo() {
    let r = rig();
    r.engine.set_tempo(1.5);
    r.engine.set(MotionValues::new().y(300.));
    r.engine.animate(
        MotionValues::new().y(0.),
        SpringTransition::new(settle()),
        at_opts(0.),
    );
    let table = settle_curve(300., 0., 0.);
    for ms in [75., 225., 600.] {
        assert_close!(r.get(Y, ms), table.value(ms / 1000. / 1.5), 3);
    }
}

#[test]
fn a_css_part_lasts_as_long_as_the_engine_plays_one_move_on_the_spring() {
    let r = rig();
    r.engine.set_tempo(1.5);
    let role = springs::role::SHEET_DISMISS_SWIPE;
    let close_handle = r.engine.animate(
        MotionValues::new().y(400.),
        SpringTransition::new(role).velocity(MotionValues::new().y(3000.)),
        at_opts(0.),
    );
    let motion = SpringMove {
        from: 0.,
        to: 400.,
        velocity: 3000.,
        epsilon: Y.settle_threshold(),
    };
    let curve = close_handle.curve(Y).unwrap();
    let curve_ms = curve.duration() * 1000.;
    assert_close!(resolve_settle_ms(role, 1.5, Some(motion)), curve_ms, 6);
    assert!(curve_ms > resolve_settle_ms(role, 1.5, None));
    let timing = resolve_move_timing(role, 1.5, motion, None);
    assert_eq!(
        timing.duration_ms,
        resolve_settle_ms(role, 1.5, Some(motion))
    );
    let stops = &timing.stops;
    assert_eq!(stops[0], 0.);
    assert_close!(*stops.last().unwrap(), 1., 2);
    for i in [1, stops.len() / 4, stops.len() - 2] {
        let t = i as f64 / (stops.len() - 1) as f64 * curve.duration();
        assert_close!(stops[i], curve.at(t) / 400., 3);
    }
    let _ = resolve_spring(role, 1.5);
}

// -- rebase towards a new rest ----------------------------------------------------

fn towards_origin() -> RebaseOptions {
    RebaseOptions {
        at: None,
        towards: MotionValues::new().x(0.),
    }
}

#[test]
fn before_its_first_frame_a_spring_heading_for_the_new_rest_keeps_its_own_velocity() {
    let r = rig();
    let curve = settle_curve(0., 300., 0.);
    r.animate(MotionValues::new().x(300.), settle());
    r.set_clock(1033.);
    r.engine
        .rebase(MotionValues::new().x(-300.), towards_origin());
    assert_close!(r.get_now(X), -300., 6);
    assert_close!(r.velocity_now(X), curve.velocity(0.033), 3);
}

#[test]
fn a_spring_with_a_known_start_moves_whole_on_its_own_course() {
    let r = rig();
    let curve = settle_curve(0., 300., 0.);
    r.animate(MotionValues::new().x(300.), settle());
    r.set_clock(1016.);
    r.frame(1016.);
    r.set_clock(1116.);
    r.engine
        .rebase(MotionValues::new().x(-300.), towards_origin());
    r.set_clock(1216.);
    assert_close!(r.get_now(X), curve.value(0.2) - 300., 6);
}

#[test]
fn a_target_the_shift_does_not_bring_to_towards_rebases_as_without_it() {
    let run = |options: RebaseOptions| {
        let r = rig();
        r.animate(MotionValues::new().x(300.), settle());
        r.set_clock(1070.);
        r.frame(1070.);
        r.set_clock(1170.);
        r.engine.rebase(MotionValues::new().x(-16.), options);
        (r.get_now(X), r.get(X, 1300.))
    };
    assert_eq!(run(RebaseOptions::default()), run(towards_origin()));
}
