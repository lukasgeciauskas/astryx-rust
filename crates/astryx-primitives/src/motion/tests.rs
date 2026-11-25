use super::uikit::{picker, scroll};
use super::*;

fn close(a: f64, b: f64, eps: f64) -> bool {
    (a - b).abs() <= eps
}

#[test]
fn a_coast_rests_where_the_decay_says() {
    let coast = solve_decay(0., 2000., None);
    assert!(close(coast.rest, 999., 1.), "{}", coast.rest);
    assert!(close(coast.value(10.), coast.rest, 1e-3));
    assert!(close(coast.velocity(0.), 2000., 1e-9));
    let reach = coast.time_to_reach(500.).unwrap();
    assert!(close(coast.value(reach), 500., 1e-6));
    assert_eq!(coast.time_to_reach(-1.), None);
    assert_eq!(coast.time_to_reach(1000.), None);
}

#[test]
fn the_projection_is_r_over_one_minus_r_ms_of_the_speed() {
    // UIKit: a 0.9334 pt/ms release past the stop speed projects 461.33 pt.
    let speed = (0.9334 - picker::STOP_SPEED) * 1000.;
    assert!(close(
        project_distance(speed, scroll::DECELERATION_NORMAL),
        461.33,
        0.3
    ));
}

#[test]
fn the_rubber_band_and_its_inverse_agree() {
    // picker: 10, 50 and 110 pt past the end show 5.4, 25.1 and 50.1 pt.
    for (finger, shown) in [(10., 5.4), (50., 25.1), (110., 50.1)] {
        let banded = rubber_band(finger, picker::RUBBER_BAND_DIMENSION);
        assert!(close(banded, shown, 0.06), "{finger}: {banded}");
        assert!(close(
            rubber_band_inverse(banded, picker::RUBBER_BAND_DIMENSION),
            finger,
            1e-9
        ));
    }
    assert_eq!(rubber_band(-10., 291.), -rubber_band(10., 291.));
    assert_eq!(rubber_band_clamp(5., 0., 10., 100.), 5.);
    assert!(rubber_band_clamp(12., 0., 10., 100.) < 12.);
    assert!(close(
        rubber_band_unclamp(rubber_band_clamp(-7., 0., 10., 100.), 0., 10., 100.),
        -7.,
        1e-9
    ));
}

#[test]
fn springs_solve_in_every_regime() {
    let settle = uikit::springs::SETTLE.to_spring();
    assert!(
        close(settle.damping_ratio(), 1., 1e-9),
        "settle is critically damped"
    );
    for spring in [
        settle,
        SpringDescription::ResponseDamping {
            response: 0.4,
            damping_ratio: 0.5,
        }
        .to_spring(),
        SpringDescription::ResponseDamping {
            response: 0.4,
            damping_ratio: 1.5,
        }
        .to_spring(),
    ] {
        let curve = solve_spring(spring, 0., 100., 50.);
        assert!(close(curve.value(0.), 0., 1e-9));
        assert!(close(curve.velocity(0.), 50., 1e-6));
        let settle = curve.settle_time(0.01);
        assert!(settle.is_finite() && settle > 0.);
        for k in 0..50 {
            let t = settle + f64::from(k) * 0.05;
            assert!((curve.value(t) - 100.).abs() <= 0.01 + 1e-9);
        }
        // The velocity is the value's derivative.
        let h = 1e-6;
        let slope = (curve.value(0.1 + h) - curve.value(0.1 - h)) / (2. * h);
        assert!(close(slope, curve.velocity(0.1), 1e-3));
    }
}

#[test]
fn a_duration_bounce_spring_reads_as_swiftui_s() {
    let spring = SpringDescription::DurationBounce {
        duration: 0.5,
        bounce: 0.3,
    }
    .to_spring();
    assert!(close(spring.damping_ratio(), 0.7, 1e-9));
    assert!(close(
        spring.natural_frequency(),
        std::f64::consts::TAU / 0.5,
        1e-9
    ));
    assert!(settling_duration(uikit::springs::SETTLE.to_spring()) > 0.5);
}
