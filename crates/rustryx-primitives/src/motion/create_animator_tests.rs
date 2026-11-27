//! Upstream `animator/createAnimator.test.ts`.

use std::cell::Cell;
use std::rc::Rc;

use super::animator::EngineOptions;
use super::create_animator::*;
use super::uikit::springs;
use super::{solve_decay, solve_spring};

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

const FRAME_MS: f64 = 16.;

struct Rig {
    clock: Rc<Cell<f64>>,
    animator: Animator,
}

fn rig_with(reduced: bool, coarse: bool) -> Rig {
    let clock = Rc::new(Cell::new(0.));
    let read = clock.clone();
    let animator = Animator::new(EngineOptions {
        now: Some(Rc::new(move || read.get())),
        frames: true,
        ..Default::default()
    });
    animator.engine().set_reduced_motion(reduced);
    animator.engine().set_coarse_pointer(coarse);
    Rig { clock, animator }
}

fn rig() -> Rig {
    rig_with(false, false)
}

impl Rig {
    fn now(&self) -> f64 {
        self.clock.get()
    }

    /// Time passes; a frame runs every 16 ms on the way.
    fn advance(&self, ms: f64) {
        let end = self.now() + ms;
        loop {
            let next = (self.now() + FRAME_MS).min(end);
            self.clock.set(next);
            self.animator.engine().frame(next);
            self.animator.engine().advance(next);
            if next >= end {
                break;
            }
        }
    }

    /// The frame after the call starts its motion; its time.
    fn started(&self) -> f64 {
        self.advance(FRAME_MS);
        self.now()
    }
}

fn state_change() -> super::Spring {
    springs::STATE_CHANGE.to_spring()
}

#[test]
fn reads_every_channel_at_rest_identity() {
    let r = rig();
    assert_eq!(
        r.animator.get(),
        AnimatorSnapshot {
            x: 0.,
            y: 0.,
            scale: 1.,
            rotate: 0.,
            opacity: 1.
        }
    );
    assert_eq!(
        r.animator.get_velocity(),
        AnimatorSnapshot {
            x: 0.,
            y: 0.,
            scale: 0.,
            rotate: 0.,
            opacity: 0.
        }
    );
}

#[test]
fn animate_plays_the_state_change_role_on_each_channel() {
    let r = rig();
    let target = AnimatorValues::new()
        .x(120.)
        .y(-80.)
        .scale(1.5)
        .rotate(45.)
        .opacity(0.25);
    r.animator
        .animate(target, AnimatorAnimateOptions::default());
    let start = r.started();
    r.advance(150.);
    let t = (r.now() - start) / 1000.;
    let now = r.animator.get();
    let velocity = r.animator.get_velocity();
    for (from, to, value, speed) in [
        (0., 120., now.x, velocity.x),
        (0., -80., now.y, velocity.y),
        (1., 1.5, now.scale, velocity.scale),
        (0., 45., now.rotate, velocity.rotate),
        (1., 0.25, now.opacity, velocity.opacity),
    ] {
        let curve = solve_spring(state_change(), from, to, 0.);
        assert_close!(value, curve.value(t), 6);
        assert_close!(speed, curve.velocity(t), 4);
        assert!(!close(value, to, 2));
    }
}

#[test]
fn animate_starts_from_a_given_velocity_and_otherwise_from_the_one_on_screen() {
    let r = rig();
    r.animator.animate(
        AnimatorValues::new().x(100.),
        AnimatorAnimateOptions {
            velocity: AnimatorValues::new().x(900.),
        },
    );
    let start = r.started();
    r.advance(100.);
    let t = (r.now() - start) / 1000.;
    assert_close!(
        r.animator.get().x,
        solve_spring(state_change(), 0., 100., 900.).value(t),
        6
    );
    let before = r.animator.get_velocity().x;
    assert_ne!(before, 0.);
    r.animator.animate(
        AnimatorValues::new().x(0.),
        AnimatorAnimateOptions::default(),
    );
    assert_close!(r.animator.get_velocity().x, before, 4);
}

#[test]
fn animate_resolves_once_the_channels_arrive() {
    let r = rig();
    let call = r.animator.animate(
        AnimatorValues::new().x(60.).opacity(0.5),
        AnimatorAnimateOptions::default(),
    );
    r.started();
    r.advance(200.);
    assert!(!call.is_done());
    r.advance(3000.);
    assert!(call.is_done());
    assert_close!(r.animator.get().x, 60., 3);
    assert_close!(r.animator.get().opacity, 0.5, 3);
}

#[test]
fn animate_resolves_early_when_a_later_call_takes_its_channels_over() {
    let r = rig();
    let call = r.animator.animate(
        AnimatorValues::new().x(300.),
        AnimatorAnimateOptions::default(),
    );
    r.started();
    r.advance(50.);
    r.animator.set(AnimatorValues::new().x(10.));
    assert!(call.is_done());
    assert_eq!(r.animator.get().x, 10.);
}

#[test]
fn a_later_animate_or_decay_resolves_its_earlier_call_where_it_stopped() {
    let r = rig();
    let first = r.animator.animate(
        AnimatorValues::new().x(300.),
        AnimatorAnimateOptions::default(),
    );
    r.started();
    r.advance(60.);
    let before = r.animator.get().x;
    let second = r.animator.animate(
        AnimatorValues::new().x(-100.),
        AnimatorAnimateOptions::default(),
    );
    assert!(first.is_done() && !second.is_done());
    assert_close!(r.animator.get().x, before, 0);
    r.advance(40.);
    let decay = r.animator.decay(AnimatorValues::new().x(800.));
    assert!(second.is_done() && !decay.is_done());
    r.advance(40.);
    r.animator.animate(
        AnimatorValues::new().x(0.),
        AnimatorAnimateOptions::default(),
    );
    assert!(decay.is_done());
}

#[test]
fn decay_coasts_x_and_y_on_the_normal_deceleration() {
    let r = rig();
    let call = r.animator.decay(AnimatorValues::new().x(1500.).y(-600.));
    let start = r.started();
    r.advance(250.);
    let t = (r.now() - start) / 1000.;
    let (x, y) = (solve_decay(0., 1500., None), solve_decay(0., -600., None));
    assert_close!(r.animator.get().x, x.value(t), 4);
    assert_close!(r.animator.get().y, y.value(t), 4);
    assert_close!(r.animator.get_velocity().x, x.velocity(t), 2);
    r.advance(10_000.);
    assert!(call.is_done());
    assert_close!(r.animator.get().x, x.rest, 0);
    assert_close!(r.animator.get().y, y.rest, 0);
}

#[test]
fn decay_coasts_every_channel_on_the_same_momentum_decay() {
    let r = rig();
    let call = r
        .animator
        .decay(AnimatorValues::new().scale(0.8).rotate(90.));
    let start = r.started();
    r.advance(250.);
    let t = (r.now() - start) / 1000.;
    let (scale, rotate) = (solve_decay(1., 0.8, None), solve_decay(0., 90., None));
    assert_close!(r.animator.get().scale, scale.value(t), 4);
    assert_close!(r.animator.get().rotate, rotate.value(t), 3);
    r.advance(10_000.);
    assert!(call.is_done());
    assert_close!(r.animator.get().scale, scale.rest, 3);
    assert_close!(r.animator.get().rotate, rotate.rest, 1);
}

#[test]
fn a_decay_of_x_with_rotate_resolves_as_soon_as_x_is_taken_over() {
    let r = rig();
    let call = r.animator.decay(AnimatorValues::new().x(1200.).rotate(90.));
    r.started();
    r.advance(50.);
    r.animator.set(AnimatorValues::new().x(0.));
    assert!(call.is_done());
    assert_eq!(r.animator.get().x, 0.);
}

#[test]
fn a_decay_of_x_with_rotate_left_alone_resolves_once_both_stop() {
    let r = rig();
    let call = r.animator.decay(AnimatorValues::new().x(1200.).rotate(90.));
    r.started();
    r.advance(500.);
    assert!(!call.is_done());
    r.advance(10_000.);
    assert!(call.is_done());
}

#[test]
fn an_opacity_coast_stops_at_0_or_1() {
    let r = rig();
    r.animator.set(AnimatorValues::new().opacity(0.5));
    r.animator.decay(AnimatorValues::new().opacity(-5.));
    r.started();
    r.advance(10_000.);
    assert_eq!(r.animator.get().opacity, 0.);
}

#[test]
fn set_puts_values_in_place_at_once() {
    let r = rig();
    r.animator.set(
        AnimatorValues::new()
            .x(40.)
            .y(8.)
            .scale(0.5)
            .rotate(90.)
            .opacity(0.3),
    );
    assert_eq!(
        r.animator.get(),
        AnimatorSnapshot {
            x: 40.,
            y: 8.,
            scale: 0.5,
            rotate: 90.,
            opacity: 0.3
        }
    );
}

#[test]
fn a_call_leaves_the_channels_it_is_not_given_where_they_are() {
    let r = rig();
    r.animator.set(AnimatorValues::new().y(30.).opacity(0.5));
    r.animator.animate(
        AnimatorValues::new().x(100.),
        AnimatorAnimateOptions::default(),
    );
    r.started();
    r.advance(5000.);
    let now = r.animator.get();
    assert_eq!((now.x, now.y, now.opacity), (100., 30., 0.5));
}

#[test]
fn stop_halts_every_channel_where_it_is_on_screen() {
    let r = rig();
    let call = r.animator.animate(
        AnimatorValues::new().x(200.).rotate(30.),
        AnimatorAnimateOptions::default(),
    );
    r.started();
    r.advance(120.);
    r.animator.stop();
    let held = r.animator.get();
    r.advance(2000.);
    assert_eq!(r.animator.get(), held);
    assert!(held.x > 0. && held.x < 200.);
    assert_eq!(r.animator.get_velocity().x, 0.);
    assert!(call.is_done());
}

#[test]
fn destroy_stops_everything_and_the_animator_does_nothing_after_it() {
    let mut r = rig();
    r.animator.animate(
        AnimatorValues::new().x(200.),
        AnimatorAnimateOptions::default(),
    );
    r.started();
    r.advance(100.);
    r.animator.destroy();
    assert!(
        r.animator
            .animate(
                AnimatorValues::new().x(0.),
                AnimatorAnimateOptions::default()
            )
            .is_done()
    );
    assert!(r.animator.decay(AnimatorValues::new().x(500.)).is_done());
    assert!(r.animator.decay(AnimatorValues::new().scale(1.)).is_done());
    r.animator.set(AnimatorValues::new().y(50.));
    r.animator.stop();
    r.animator.destroy();
    assert_eq!(r.animator.get().y, 0.);
    assert_eq!(r.animator.get_velocity().x, 0.);
}

#[test]
fn a_call_with_nothing_resolves_at_once_and_moves_nothing() {
    let r = rig();
    assert!(
        r.animator
            .animate(AnimatorValues::new(), AnimatorAnimateOptions::default())
            .is_done()
    );
    assert!(r.animator.decay(AnimatorValues::new()).is_done());
    assert!(r.animator.engine().is_settled());
}

#[test]
fn a_non_finite_value_is_ignored() {
    let r = rig();
    r.animator.set(AnimatorValues::new().x(30.).y(f64::NAN));
    assert_eq!(r.animator.get().x, 30.);
    assert_eq!(r.animator.get().y, 0.);
}

#[test]
fn a_detached_animator_does_nothing_until_attached_and_a_new_element_starts_at_rest() {
    let mut a = Animator::detached(EngineOptions::default());
    a.set(AnimatorValues::new().x(10.));
    assert_eq!(a.get().x, 0.);
    a.set_attached(true);
    a.set(AnimatorValues::new().x(10.).opacity(0.5));
    assert_eq!(a.get().x, 10.);
    a.set_attached(false);
    assert_eq!(
        a.get(),
        AnimatorSnapshot {
            x: 0.,
            y: 0.,
            scale: 1.,
            rotate: 0.,
            opacity: 1.
        }
    );
}

#[test]
fn on_a_coarse_pointer_under_reduce_motion_a_move_becomes_a_cross_fade() {
    let r = rig_with(true, true);
    let call = r.animator.animate(
        AnimatorValues::new().x(200.),
        AnimatorAnimateOptions::default(),
    );
    r.started();
    let mut samples = Vec::new();
    for _ in 0..40 {
        r.advance(FRAME_MS);
        let now = r.animator.get();
        samples.push((now.x, now.opacity));
    }
    assert!(samples.iter().all(|(x, _)| *x == 0. || *x == 200.));
    assert!(samples.iter().map(|(_, o)| *o).fold(1., f64::min) < 0.05);
    let jump = samples.iter().position(|(x, _)| *x == 200.).unwrap();
    assert!(jump > 0);
    assert!(samples[jump].1 < 0.05);
    r.advance(1000.);
    assert!(call.is_done());
    let now = r.animator.get();
    assert_eq!((now.x, now.opacity), (200., 1.));
}

#[test]
fn on_a_fine_pointer_under_reduce_motion_a_move_lands_at_once() {
    let r = rig_with(true, false);
    let call = r.animator.animate(
        AnimatorValues::new().x(200.).scale(2.),
        AnimatorAnimateOptions::default(),
    );
    let now = r.animator.get();
    assert_eq!((now.x, now.scale), (200., 2.));
    r.advance(FRAME_MS);
    assert!(call.is_done());
}

#[test]
fn under_reduce_motion_a_release_coast_keeps_its_momentum() {
    let r = rig_with(true, true);
    r.animator.decay(AnimatorValues::new().x(1200.));
    let start = r.started();
    r.advance(200.);
    let t = (r.now() - start) / 1000.;
    assert_close!(r.animator.get().x, solve_decay(0., 1200., None).value(t), 4);
}
