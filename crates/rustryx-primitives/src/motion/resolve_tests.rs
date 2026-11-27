//! Upstream `motion/resolveMotion.test.ts`, `resolveMotion.tempo.test.ts`
//! and `themeTempo.test.ts` (the cases that need the animator are in
//! `animator_tests.rs`).

use super::resolve_motion::*;
use super::theme_tempo::{theme_tempo, theme_tempo_from_text};
use super::uikit::{progress_bar, springs, switch_control, timing};
use super::{MotionProperty, MotionSpring, SpringDescription, settling_duration, solve_spring};

fn close(a: f64, b: f64, digits: i32) -> bool {
    (a - b).abs() < 10f64.powi(-digits) / 2.
}

#[test]
fn the_tempo_is_what_the_theme_writes() {
    assert_eq!(theme_tempo_from_text("1.5"), 1.5);
    assert_eq!(theme_tempo_from_text("0.85"), 0.85);
}

#[test]
fn the_tempo_is_1_where_no_theme_sets_one() {
    assert_eq!(theme_tempo(None), 1.);
    assert_eq!(theme_tempo_from_text(""), 1.);
}

#[test]
fn a_tempo_that_is_not_a_positive_number_is_1() {
    for value in ["0", "-1", "fast"] {
        assert_eq!(theme_tempo_from_text(value), 1.);
    }
}

#[test]
fn plays_the_table_value_at_tempo_1() {
    assert_eq!(resolve_spring(springs::role::SETTLE, 1.), springs::SETTLE);
    assert_eq!(
        resolve_duration_ms(
            Some(timing::STATE_CHANGE_MS_ROLE),
            timing::STATE_CHANGE_MS,
            1.
        ),
        timing::STATE_CHANGE_MS
    );
}

#[test]
fn leaves_values_that_are_not_roles_alone() {
    for tempo in [1., 1.5] {
        let inline = SpringDescription::ResponseDamping {
            response: 0.4,
            damping_ratio: 0.9,
        };
        assert_eq!(resolve_spring(MotionSpring::inline(inline), tempo), inline);
        assert_eq!(
            resolve_spring(springs::SHEET_PRESENT.into(), tempo),
            springs::SHEET_PRESENT
        );
        assert_eq!(
            resolve_duration_ms(
                Some("timing.press.scrollBrakeMs"),
                timing::press::SCROLL_BRAKE_MS,
                tempo
            ),
            timing::press::SCROLL_BRAKE_MS
        );
        // A role id no table registers plays as given.
        let unregistered = MotionSpring::role("springs.notARole", inline);
        assert_eq!(resolve_spring(unregistered, tempo), inline);
    }
}

#[test]
fn stretches_a_spring_role_in_each_of_its_forms() {
    for role in [
        springs::role::SETTLE,
        springs::role::MENU_OPEN,
        springs::role::VIEW_CHANGE,
    ] {
        let base = solve_spring(role.spring.to_spring(), 0., 1., 0.);
        let slowed = solve_spring(resolve_spring(role, 1.5).to_spring(), 0., 1., 0.);
        assert!(close(
            slowed.settle_time(0.001),
            base.settle_time(0.001) * 1.5,
            6
        ));
        for seconds in [0.05, 0.15, 0.3] {
            assert!(close(slowed.value(seconds * 1.5), base.value(seconds), 6));
        }
    }
}

#[test]
fn scales_a_duration_role() {
    let ms = |tempo| {
        resolve_duration_ms(
            Some(timing::STATE_CHANGE_MS_ROLE),
            timing::STATE_CHANGE_MS,
            tempo,
        )
    };
    assert_eq!(ms(1.5), timing::STATE_CHANGE_MS * 1.5);
    assert_eq!(ms(0.75), timing::STATE_CHANGE_MS * 0.75);
}

#[test]
fn leaves_the_parts_that_follow_a_finger_and_the_busy_loop_as_measured() {
    assert_eq!(
        resolve_spring(springs::role::FINGER_TRACKING, 1.5),
        springs::FINGER_TRACKING
    );
    assert_eq!(
        resolve_spring(switch_control::role::TRACKING, 1.5),
        switch_control::TRACKING
    );
    assert_eq!(
        resolve_duration_ms(
            Some("progress.reducedMotionPulseMs"),
            progress_bar::REDUCED_MOTION_PULSE_MS,
            1.5
        ),
        progress_bar::REDUCED_MOTION_PULSE_MS
    );
    assert_ne!(
        resolve_spring(switch_control::role::SETTLE, 1.5),
        switch_control::SETTLE
    );
    assert_eq!(
        resolve_duration_ms(
            Some("progress.fillTrackMs"),
            progress_bar::FILL_TRACK_MS,
            1.5
        ),
        progress_bar::FILL_TRACK_MS * 1.5
    );
}

#[test]
fn a_resolved_spring_is_not_scaled_again() {
    let once = resolve_spring(springs::role::SETTLE, 1.5);
    assert_ne!(once, springs::SETTLE);
    assert_eq!(resolve_spring(springs::role::SETTLE, 1.5), once);
    assert_eq!(resolve_spring(MotionSpring::inline(once), 1.5), once);
}

fn settle_ms(spring: SpringDescription) -> f64 {
    settling_duration(spring.to_spring()) * 1000.
}

#[test]
fn a_css_part_lasts_the_roles_settling_duration() {
    let ms = resolve_settle_ms(springs::role::SETTLE, 1., None);
    assert_eq!(ms, settle_ms(springs::SETTLE));
    assert!(ms > 400. && ms < 900.);
}

#[test]
fn a_css_part_takes_tempo_times_as_long() {
    let plain = resolve_settle_ms(springs::role::SETTLE, 1., None);
    let slowed = resolve_settle_ms(springs::role::SETTLE, 1.5, None);
    assert!(close(slowed, plain * 1.5, 6));
    assert_eq!(
        slowed,
        settle_ms(resolve_spring(springs::role::SETTLE, 1.5))
    );
}

#[test]
fn a_css_part_stays_as_measured_for_a_finger_role_and_an_inline_spring() {
    assert_eq!(
        resolve_settle_ms(springs::role::FINGER_TRACKING, 1.5, None),
        settle_ms(springs::FINGER_TRACKING)
    );
    let inline = SpringDescription::ResponseDamping {
        response: 0.4,
        damping_ratio: 0.9,
    };
    assert_eq!(
        resolve_settle_ms(inline.into(), 1.5, None),
        settle_ms(inline)
    );
}

#[test]
fn the_easing_ends_where_the_move_gets_to_done_at_and_stays_there() {
    let motion = SpringMove {
        from: 0.,
        to: 700.,
        velocity: 3000.,
        epsilon: MotionProperty::Y.settle_threshold(),
    };
    let done_at = 400.;
    let timing = resolve_move_timing(
        springs::role::SHEET_DISMISS_SWIPE,
        1.,
        motion,
        Some(done_at),
    );
    assert_eq!(
        timing.duration_ms,
        resolve_settle_ms(springs::role::SHEET_DISMISS_SWIPE, 1., Some(motion))
    );
    let stops = timing.stops;
    let curve = solve_spring(
        resolve_spring(springs::role::SHEET_DISMISS_SWIPE, 1.).to_spring(),
        motion.from,
        motion.to,
        motion.velocity,
    );
    let at_stop =
        |i: usize| curve.value(i as f64 / (stops.len() - 1) as f64 * timing.duration_ms / 1000.);
    let reached = stops.iter().position(|stop| *stop == 1.).unwrap();
    assert!(reached > 0);
    for (i, stop) in stops.iter().enumerate().take(reached).skip(1) {
        assert!(close(*stop, at_stop(i) / done_at, 3));
        assert!(at_stop(i) < done_at);
    }
    assert!(at_stop(reached) >= done_at - 1.);
    assert!(stops[reached..].iter().all(|stop| *stop == 1.));
    assert!(timing.easing.starts_with("linear(0, "));
}
