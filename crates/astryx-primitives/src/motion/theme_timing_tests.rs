//! Upstream `motion/themeTiming.test.ts`.

use std::collections::HashMap;

use super::MotionCurve;
use super::bezier::cubic_bezier;
use super::roles::parse_css_easing;
use super::theme_timing::*;

fn close(a: f64, b: f64, digits: i32) -> bool {
    (a - b).abs() < 10f64.powi(-digits) / 2.
}

const FAST: ThemeTimingTokens = ThemeTimingTokens {
    duration: "--duration-fast",
    easing: "--ease-standard",
};

/// The token values set where an element is.
fn element(styles: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
    let map: HashMap<String, String> = styles
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    move |name| map.get(name).cloned()
}

fn standard_ease(x: f64) -> f64 {
    let points = parse_css_easing(DECLARED_TIMING_TOKENS[10].1).unwrap();
    cubic_bezier(points).ease(x)
}

#[test]
fn declares_every_timing_token_as_the_token_file_does() {
    let names: Vec<&str> = DECLARED_TIMING_TOKENS
        .iter()
        .map(|(name, _)| *name)
        .collect();
    assert_eq!(names.len(), 14);
    assert!(names.contains(&"--duration-exit") && names.contains(&"--ease-exit"));
    assert_eq!(DECLARED_TIMING_TOKENS[1], ("--duration-fast", "175ms"));
}

#[test]
fn reads_the_elements_own_transition() {
    let timing = read_transition_timing("0.3s, 1s", "linear, ease-in").unwrap();
    assert!(close(timing.duration(), 0.3, 6));
    assert!(close(timing.at(0.15), 0.5, 6));
    assert!(read_transition_timing("0s", "ease").is_none());
}

#[test]
fn reads_tokens_set_where_the_element_is_in_ms_or_s() {
    let seconds = read_theme_timing(
        &element(&[("--duration-fast", "0.3s"), ("--ease-standard", "linear")]),
        FAST,
    )
    .unwrap();
    assert!(close(seconds.duration(), 0.3, 6));
    assert!(close(seconds.at(0.15), 0.5, 6));
    let ms = read_theme_timing(&element(&[("--duration-fast", "250ms")]), FAST).unwrap();
    assert!(close(ms.duration(), 0.25, 6));
}

#[test]
fn reads_a_token_with_no_value_at_the_declared_default() {
    let timing = read_theme_timing(&element(&[]), FAST).unwrap();
    assert!(close(timing.duration(), 0.175, 6));
    assert!(close(timing.at(0.0875), standard_ease(0.5), 6));
    assert_eq!(timing.at(0.175), 1.);
}

#[test]
fn reads_duration_exit_at_its_default_reference_resolved_where_the_element_is() {
    let exit = ThemeTimingTokens {
        duration: "--duration-exit",
        easing: "--ease-exit",
    };
    assert!(close(
        read_theme_timing(&element(&[]), exit).unwrap().duration(),
        0.175,
        6
    ));
    assert_eq!(
        read_theme_tokens(
            &element(&[("--duration-fast", "250ms")]),
            Some("--duration-exit"),
            None
        ),
        ResolvedTiming {
            ms: Some(250.),
            points: None
        }
    );
    assert_eq!(
        read_theme_tokens(
            &element(&[("--duration-exit", "90ms")]),
            Some(exit.duration),
            Some(exit.easing)
        )
        .ms,
        Some(90.)
    );
}

#[test]
fn gives_the_progress_velocity_per_second_and_none_once_it_ends() {
    let timing = read_theme_timing(
        &element(&[("--duration-fast", "0.3s"), ("--ease-standard", "linear")]),
        FAST,
    )
    .unwrap();
    assert!(close(timing.velocity(0.), 1. / 0.3, 4));
    assert!(close(timing.velocity(0.15), 1. / 0.3, 4));
    assert_eq!(timing.velocity(0.3), 0.);
    let eased = read_theme_timing(
        &element(&[]),
        ThemeTimingTokens {
            duration: "--duration-fast",
            easing: "--ease-exit",
        },
    )
    .unwrap();
    let (t, h) = (0.05, 1e-6);
    assert!(close(
        eased.velocity(t),
        (eased.at(t + h) - eased.at(t - h)) / (2. * h),
        3
    ));
}

#[test]
fn plays_an_easing_that_is_not_a_cubic_bezier_and_a_jump_hands_on_no_speed() {
    let curve =
        read_theme_timing(&element(&[("--ease-standard", "steps(4, jump-end)")]), FAST).unwrap();
    assert!(close(curve.duration(), 0.175, 6));
    assert_eq!(curve.at(0.05), 0.25);
    for t in [0.04375, 0.0875, 0.13125, 0.1749] {
        assert_eq!(curve.velocity(t), 0., "{t} s");
    }
}

#[test]
fn returns_no_curve_for_a_duration_that_is_not_positive_or_an_easing_it_cannot_read() {
    assert!(read_theme_timing(&element(&[("--duration-fast", "0ms")]), FAST).is_none());
    assert!(read_theme_timing(&element(&[("--duration-fast", "fast")]), FAST).is_none());
    assert!(read_theme_timing(&element(&[("--ease-standard", "bounce")]), FAST).is_none());
}

#[test]
fn or_instant_reads_the_timing_as_read_theme_timing_does() {
    let set = element(&[("--duration-fast", "200ms")]);
    let timing = read_theme_timing_or_instant(Some(&set), FAST);
    let read = read_theme_timing(&set, FAST).unwrap();
    assert!(close(timing.duration(), read.duration(), 6));
    assert!(close(timing.at(0.1), read.at(0.1), 6));
}

#[test]
fn or_instant_plays_no_motion_when_the_timing_does_not_resolve() {
    let zero = element(&[("--duration-fast", "0ms")]);
    for timing in [
        read_theme_timing_or_instant(Some(&zero), FAST),
        read_theme_timing_or_instant(None, FAST),
    ] {
        assert_eq!(timing.duration(), 0.);
        assert_eq!(timing.at(0.), 1.);
        assert_eq!(timing.velocity(0.), 0.);
    }
}

#[test]
fn resolve_timing_reads_a_time_and_a_cubic_bezier_and_none_else() {
    assert_eq!(
        resolve_timing(Some("0.3s"), Some("ease-in-out")),
        ResolvedTiming {
            ms: Some(300.),
            points: Some([0.42, 0., 0.58, 1.])
        }
    );
    assert_eq!(resolve_timing(Some("0ms"), None).ms, Some(0.));
    assert_eq!(resolve_timing(Some("fast"), None).ms, None);
    assert_eq!(resolve_timing(Some("-1ms"), None).ms, None);
    assert_eq!(resolve_timing(None, Some("steps(4)")).points, None);
    assert_eq!(resolve_timing(None, None), ResolvedTiming::default());
}

#[test]
fn eased_progress_eases_from_0_to_1_and_no_length_is_at_its_end() {
    let curve = eased_progress(0.2, |x| x * x, false);
    assert_eq!(curve.duration(), 0.2);
    assert_eq!(curve.at(0.), 0.);
    assert!(close(curve.at(0.1), 0.25, 6));
    assert_eq!(curve.at(0.2), 1.);
    assert!(close(curve.velocity(0.1), 5., 3));
    assert_eq!(curve.velocity(0.2), 0.);
    let none = eased_progress(0., |x| x, false);
    assert_eq!(none.at(0.), 1.);
    assert_eq!(none.velocity(0.), 0.);
}

#[test]
fn eased_progress_with_jumps_carries_no_speed_across_a_jump() {
    let steps = |x: f64| (4. * x).floor() / 4.;
    assert!(eased_progress(0.3, steps, false).velocity(0.075) > 100.);
    let jumping = eased_progress(0.3, steps, true);
    for t in [0., 0.0749, 0.075, 0.0751, 0.15, 0.225, 0.2995] {
        assert_eq!(jumping.velocity(t), 0., "{t} s");
    }
    assert!(close(
        eased_progress(0.2, |x| x * x, true).velocity(0.1),
        5.,
        1
    ));
}

#[test]
fn css_easing_plays_linear_stops() {
    let ease = css_easing("linear(0, 0.25 75%, 1)").unwrap();
    assert!(close(ease(0.375), 0.125, 9));
    assert!(close(ease(0.875), 0.625, 9));
    assert!(css_easing("bounce").is_none());
}
