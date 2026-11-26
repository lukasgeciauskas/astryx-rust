//! Upstream `motion/spring.test.ts`.

#![allow(clippy::excessive_precision)]

use std::f64::consts::{PI, TAU};

use super::*;

fn close(a: f64, b: f64, digits: i32) -> bool {
    (a - b).abs() < 10f64.powi(-digits) / 2.
}

struct Sample {
    t: f64,
    value: f64,
    velocity: f64,
}

/// Classic RK4 on m·x″ + c·x′ + k·(x − target(t)) = 0.
fn rk4(
    spring: Spring,
    from: f64,
    velocity: f64,
    target: impl Fn(f64) -> f64,
    until: f64,
) -> Vec<Sample> {
    let dt = 1e-4;
    let (m, k, c) = (spring.mass, spring.stiffness, spring.damping);
    let accel = |time: f64, x: f64, v: f64| (-k * (x - target(time)) - c * v) / m;
    let (mut x, mut v) = (from, velocity);
    let mut out = vec![Sample {
        t: 0.,
        value: x,
        velocity: v,
    }];
    let steps = (until / dt).round() as usize;
    for i in 1..=steps {
        let time = (i - 1) as f64 * dt;
        let k1x = v;
        let k1v = accel(time, x, v);
        let k2x = v + dt / 2. * k1v;
        let k2v = accel(time + dt / 2., x + dt / 2. * k1x, v + dt / 2. * k1v);
        let k3x = v + dt / 2. * k2v;
        let k3v = accel(time + dt / 2., x + dt / 2. * k2x, v + dt / 2. * k2v);
        let k4x = v + dt * k3v;
        let k4v = accel(time + dt, x + dt * k3x, v + dt * k3v);
        x += dt / 6. * (k1x + 2. * k2x + 2. * k3x + k4x);
        v += dt / 6. * (k1v + 2. * k2v + 2. * k3v + k4v);
        if i % 100 == 0 {
            out.push(Sample {
                t: i as f64 * dt,
                value: x,
                velocity: v,
            });
        }
    }
    out
}

const RATIOS: [f64; 13] = [
    0., 0.05, 0.3, 0.5, 0.75, 0.9, 0.99, 0.999999, 1., 1.000001, 1.2, 2., 5.,
];
const RESPONSES: [f64; 4] = [0.15, 0.35, 0.5, 1.];
/// (from, to, velocity).
const RELEASES: [(f64, f64, f64); 5] = [
    (0., 1., 0.),
    (0., 300., 0.),
    (300., 0., -2500.),
    (-120., 80., 4000.),
    (50., 50., 1800.),
];

fn response(response: f64, damping_ratio: f64) -> Spring {
    spring_from_response(response, damping_ratio).unwrap()
}

fn physics(stiffness: f64, damping: f64) -> Spring {
    spring_from_physics(1., stiffness, damping, false).unwrap()
}

#[test]
fn solve_spring_matches_an_rk4_integration_of_the_ode() {
    for ratio in RATIOS {
        for resp in RESPONSES {
            let spring = response(resp, ratio);
            for (from, to, velocity) in RELEASES {
                let curve = solve_spring(spring, from, to, velocity);
                let scale = 1f64.max((from - to).abs()).max(velocity.abs() * resp);
                for s in rk4(spring, from, velocity, |_| to, 1.5) {
                    assert!(
                        (curve.value(s.t) - s.value).abs() < 1e-6 * scale,
                        "ζ={ratio} r={resp}"
                    );
                    assert!(
                        (curve.velocity(s.t) - s.velocity).abs() < 1e-6 * scale * 2. * PI / resp,
                        "ζ={ratio} r={resp}"
                    );
                }
            }
        }
    }
}

#[test]
fn solve_spring_agrees_with_webkits_spring_solver() {
    let webkit = |m: f64, k: f64, c: f64, v0: f64, t: f64| {
        let w0 = (k / m).sqrt();
        let zeta = c / (2. * (k * m).sqrt());
        let x = if zeta < 1. {
            let wd = w0 * (1. - zeta * zeta).sqrt();
            let b = (zeta * w0 + -v0) / wd;
            (-t * zeta * w0).exp() * ((wd * t).cos() + b * (wd * t).sin())
        } else {
            (1. + (-v0 + w0) * t) * (-t * w0).exp()
        };
        1. - x
    };
    let cases = [
        [1., 438.64908449286042, 31.415926535897931, 0.],
        [1., 322.27279677026473, 31.2364069556928, 0.],
        [1., 100., 5., 0.],
        [1., 100., 20., 0.],
        [2., 300., 12., 4.],
        [1., 250., 31.6227766016838, -3.],
    ];
    for [m, k, c, v0] in cases {
        let curve = solve_spring(spring_from_physics(m, k, c, false).unwrap(), 0., 1., v0);
        let mut t = 0.;
        while t <= 1.5 {
            assert!(close(curve.value(t), webkit(m, k, c, v0, t), 6));
            t += 0.01;
        }
    }
}

#[test]
fn starts_at_the_release_value_and_velocity_in_every_regime() {
    for ratio in RATIOS {
        let curve = solve_spring(response(0.4, ratio), 10., 250., -700.);
        assert!(close(curve.value(0.), 10., 9));
        assert!(close(curve.velocity(0.), -700., 6));
    }
}

#[test]
fn is_continuous_across_critical_damping() {
    let critical = solve_spring(response(0.5, 1.), 0., 300., 1200.);
    for ratio in [1. - 1e-5, 1. - 1e-7, 1. + 1e-7, 1. + 1e-5] {
        let near = solve_spring(response(0.5, ratio), 0., 300., 1200.);
        let mut t = 0.;
        while t <= 1.5 {
            assert!((near.value(t) - critical.value(t)).abs() < 0.01);
            t += 0.01;
        }
    }
}

#[test]
fn comes_to_rest_at_the_target() {
    for ratio in RATIOS.into_iter().filter(|r| *r > 0.) {
        let curve = solve_spring(response(0.3, ratio), 0., 100., 0.);
        let settle = curve.settle_time(0.01);
        assert!((curve.value(settle + 5.) - 100.).abs() < 0.01);
    }
}

#[test]
fn settle_time_bounds_every_later_displacement() {
    for ratio in RATIOS.into_iter().filter(|r| *r > 0.) {
        for resp in RESPONSES {
            for (from, to, velocity) in RELEASES {
                let curve = solve_spring(response(resp, ratio), from, to, velocity);
                for threshold in [1., 0.1, 0.001] {
                    let settle = curve.settle_time(threshold);
                    assert!(settle.is_finite());
                    for i in 0..=400 {
                        let t = settle + f64::from(i) / 400. * 3. * resp;
                        assert!((curve.value(t) - to).abs() <= threshold * (1. + 1e-9));
                    }
                }
            }
        }
    }
}

#[test]
fn settle_time_is_0_when_settled_and_infinite_when_undamped() {
    let spring = response(0.4, 0.8);
    assert_eq!(solve_spring(spring, 5., 5., 0.).settle_time(0.5), 0.);
    assert_eq!(solve_spring(spring, 5.1, 5., 0.).settle_time(0.5), 0.);
    let undamped = response(0.4, 0.);
    assert_eq!(
        solve_spring(undamped, 0., 1., 0.).settle_time(0.001),
        f64::INFINITY
    );
}

#[test]
fn an_undamped_spring_oscillates_with_the_response_as_its_period() {
    let curve = solve_spring(spring_from_duration_bounce(0.5, 1.).unwrap(), 0., 1., 0.);
    let mut t = 0.;
    while t <= 2. {
        assert!(close(curve.value(t), 1. - (2. * PI * t / 0.5).cos(), 9));
        t += 0.05;
    }
}

#[test]
fn a_ramp_matches_an_rk4_integration() {
    let cases = [
        (response(0.32, 0.8), 0.16),
        (response(0.3, 1.), -40.),
        (spring_from_physics(1., 100., 40., true).unwrap(), 250.),
    ];
    for (spring, slope) in cases {
        let (from, velocity, target) = (1., 0.5, 1.02);
        let curve = solve_spring_toward_ramp(spring, from, velocity, target, slope);
        let scale = 1f64.max(slope.abs());
        for s in rk4(spring, from, velocity, |t| target + slope * t, 1.5) {
            assert!(
                (curve.value(s.t) - s.value).abs() < 1e-7 * scale,
                "slope {slope}"
            );
            assert!((curve.velocity(s.t) - s.velocity).abs() < 1e-6 * scale);
        }
    }
}

#[test]
fn a_ramp_settles_lag_behind_at_the_targets_speed() {
    let spring = response(0.3, 1.);
    let curve = solve_spring_toward_ramp(spring, 0., 2., 0., 10.);
    assert!(close(curve.value(0.), 0., 12));
    assert!(close(curve.velocity(0.), 2., 12));
    assert!(close(
        curve.lag,
        spring.damping * 10. / spring.stiffness,
        12
    ));
    assert!(close(curve.value(5.) - (10. * 5. - curve.lag), 0., 6));
    assert!(close(curve.velocity(5.), 10., 6));
}

#[test]
fn a_ramp_with_no_slope_is_the_ordinary_spring() {
    let spring = response(0.35, 0.85);
    let ramp = solve_spring_toward_ramp(spring, 0., 3., 1., 0.);
    let plain = solve_spring(spring, 0., 1., 3.);
    let mut t = 0.;
    while t <= 1. {
        assert!(close(ramp.value(t), plain.value(t), 12));
        assert!(close(ramp.velocity(t), plain.velocity(t), 12));
        t += 0.05;
    }
}

#[test]
fn duration_bounce_follows_the_wwdc23_equations() {
    let bouncy = spring_from_duration_bounce(0.5, 0.3).unwrap();
    assert_eq!(bouncy.mass, 1.);
    assert!(close(bouncy.stiffness, (TAU / 0.5).powi(2), 9));
    assert!(close(bouncy.damping, 0.7 * 4. * PI / 0.5, 9));
    let flattened = spring_from_duration_bounce(0.4, -0.5).unwrap();
    assert!(close(flattened.damping, 4. * PI / (0.4 * 0.5), 9));
    assert!(close(flattened.damping_ratio(), 2., 9));
}

#[test]
fn duration_bounce_round_trips_over_the_whole_range() {
    for duration in [0.15, 0.3, 0.5, 0.75, 1.2] {
        for bounce in [-0.9, -0.5, -0.1, 0., 0.15, 0.3, 0.6, 0.95, 1.] {
            let back = spring_from_duration_bounce(duration, bounce)
                .unwrap()
                .duration_bounce();
            assert!(close(back.duration, duration, 9));
            assert!(close(back.bounce, bounce, 9));
        }
    }
}

#[test]
fn swiftui_presets_map_to_their_damping_ratios() {
    for (bounce, ratio) in [(0., 1.), (0.15, 0.85), (0.3, 0.7)] {
        let spring = spring_from_duration_bounce(0.5, bounce).unwrap();
        assert!(close(spring.damping_ratio(), ratio, 12));
    }
}

#[test]
fn response_damping_reproduces_webkits_switch_springs() {
    let mv = response(0.3, 0.75);
    assert!(close(mv.stiffness, 438.64908449286042, 9));
    assert!(close(mv.damping, 31.415926535897931, 9));
    let press = response(0.35, 0.87);
    assert!(close(press.stiffness, 322.27279677026473, 9));
    assert!(close(press.damping, 31.2364069556928, 9));
}

#[test]
fn response_damping_and_physics_round_trip() {
    let spring = spring_from_physics(2., 300., 20., false).unwrap();
    let ResponseDamping {
        response: r,
        damping_ratio,
    } = spring.response_damping();
    assert!(close(r, TAU / 150f64.sqrt(), 12));
    assert!(close(damping_ratio, 20. / (2. * 600f64.sqrt()), 12));
    let unit = response(r, damping_ratio);
    let a = solve_spring(spring, 0., 1., 3.);
    let b = solve_spring(unit, 0., 1., 3.);
    let mut t = 0.;
    while t < 1. {
        assert!(close(a.value(t), b.value(t), 9));
        t += 0.05;
    }
    assert!(close(unit.response(), r, 12));
}

#[test]
fn an_overdamped_physics_spring_is_critical_unless_allowed() {
    assert!(close(physics(100., 50.).damping_ratio(), 1., 12));
    let kept = spring_from_physics(1., 100., 50., true).unwrap();
    assert!(close(kept.damping_ratio(), 2.5, 12));
}

#[test]
fn rejects_impossible_parameters() {
    assert!(spring_from_duration_bounce(0., 0.).is_err());
    assert!(spring_from_response(-1., 1.).is_err());
    assert!(spring_from_physics(1., 100., -1., false).is_err());
    assert!(spring_from_physics(0., 100., 1., false).is_err());
}

#[test]
fn relative_velocity_follows_uikits_documented_example() {
    assert_eq!(relative_velocity(100., 0., 200.), 0.5);
    assert_eq!(absolute_velocity(0.5, 0., 200.), 100.);
    assert_eq!(relative_velocity(-100., 300., 100.), 0.5);
}

#[test]
fn settling_duration_matches_apples_ca_spring_animation_example() {
    let loose = settling_duration(physics(100., 5.));
    assert!(loose > 2.845 && loose < 2.865);
    let tight = settling_duration(physics(100., 15.));
    assert!(tight > 1. && tight < 1.05);
}

#[test]
fn settling_duration_matches_webkits_ios_switch_durations() {
    assert!(close(
        settling_duration(response(0.3, 0.75)),
        0.4880138408543766,
        9
    ));
    assert!(close(
        settling_duration(response(0.35, 0.87)),
        0.5073965509413827,
        9
    ));
}

#[test]
fn settling_duration_is_shorter_for_stiffer_or_better_damped_springs() {
    let base = settling_duration(physics(100., 10.));
    assert!(settling_duration(physics(200., 10.)) < base);
    assert!(settling_duration(physics(100., 15.)) < base);
    assert!(settling_duration(spring_from_physics(2., 100., 10., false).unwrap()) > base);
}

#[test]
fn a_critically_damped_unit_move_settles_at_one_plus_u_e_minus_u() {
    let spring = response(0.5, 1.);
    let omega = TAU / 0.5;
    let u = settling_duration(spring) * omega;
    assert!(close((1. + u) * (-u).exp(), 0.001, 9));
}

#[test]
fn settling_duration_accounts_for_the_initial_velocity() {
    let spring = response(0.5, 0.8);
    let still = settling_duration(spring);
    assert!(settling_duration_with(spring, -8., SETTLING_EPSILON) > still);
    assert!(settling_duration_with(spring, 40., SETTLING_EPSILON) > still);
    assert!(settling_duration_with(spring, 8., SETTLING_EPSILON) < still);
}

#[test]
fn settling_duration_matches_ca_spring_animation_reads() {
    let cases = [
        (5., 0., 2.854974609888761),
        (5., 5., 2.854974609888761),
        (5., -5., 2.992531378997136),
        (10., 0., 1.4727003346780927),
        (10., 5., 1.3815510557964277),
        (10., -5., 1.5350814063145797),
        (15., 0., 1.0220937981854987),
        (15., 5., 0.9637816892907282),
        (15., -5., 1.0625267085113923),
    ];
    for (damping, velocity, settling) in cases {
        let got = settling_duration_with(physics(100., damping), velocity, SETTLING_EPSILON);
        assert!(close(got, settling, 12), "{damping} {velocity}: {got}");
    }
}

#[test]
fn settling_duration_matches_uikits_spring_durations() {
    let cases = [
        (0.3, 0., 0., 0.4409405178437803),
        (0.3, 0., 2., 0.43617534275714415),
        (0.3, 0.15, 0., 0.4419904194536484),
        (0.3, 0.15, 2., 0.4379526271453696),
        (0.3, 0.3, 0., 0.5177731699046524),
        (0.5, 0., 0., 0.734900863),
        (0.5, 0.15, 0., 0.736650699089414),
        (0.5, 0.3, 2., 0.8493819071116907),
        (1., 0., 0., 1.469801726),
        (1., 0., 2., 1.410252226),
        (1., 0.15, 2., 1.4240715395795775),
        (1., 0.3, 0., 1.725910566348841),
    ];
    for (duration, bounce, velocity, uikit) in cases {
        let spring = spring_from_duration_bounce(duration, bounce).unwrap();
        let mine = settling_duration_with(spring, velocity, SETTLING_EPSILON);
        if bounce > 0. {
            assert!(
                close(mine, uikit, 12),
                "{duration} {bounce} {velocity}: {mine}"
            );
        } else {
            assert!((mine - uikit).abs() / uikit < 3e-4);
        }
    }
}

#[test]
fn uikits_default_spring_clamps_overdamping_to_critical() {
    let spring = spring_from_physics(3., 1000., 500., false).unwrap();
    assert!(close(spring.damping_ratio(), 1., 12));
    assert!((settling_duration(spring) - 0.505824).abs() / 0.505824 < 3e-4);
}

#[test]
fn the_legacy_damping_duration_api_matches_uikits_reads() {
    let cases = [
        (0.3, 0.5, 1796.883, 42.3897),
        (0.3, 0.8, 898.86, 47.9696),
        (0.3, 1., 947.288, 61.5561),
        (0.5, 0.5, 646.878, 25.4338),
        (0.5, 0.8, 323.589, 28.7817),
        (0.5, 1., 341.024, 36.9337),
        (1., 0.5, 161.72, 12.7169),
        (1., 0.8, 80.897, 14.3909),
        (1., 1., 85.256, 18.4668),
    ];
    for (duration, ratio, stiffness, damping) in cases {
        let spring = spring_from_uikit_damping_duration(duration, ratio).unwrap();
        assert_eq!(spring.mass, 1.);
        assert!((spring.stiffness - stiffness).abs() / stiffness < 1e-4);
        assert!((spring.damping - damping).abs() / damping < 1e-4);
    }
}

#[test]
fn the_legacy_spring_settles_at_the_duration_at_ratio_1() {
    let spring = spring_from_uikit_damping_duration(0.4, 1.).unwrap();
    assert!(close(settling_duration(spring), 0.4, 3));
}

#[test]
fn the_legacy_spring_reproduces_the_wide_read() {
    for (ratio, omega) in [
        (0.1, 46.1019536228216),
        (0.3, 19.1698),
        (0.65, 10.3869),
        (0.95, 8.4425),
    ] {
        let spring = spring_from_uikit_damping_duration(1., ratio).unwrap();
        assert!(close(spring.stiffness.sqrt(), omega, 3));
    }
}

#[test]
fn the_legacy_spring_interpolates_between_the_measured_ratios() {
    let w = |ratio| {
        spring_from_uikit_damping_duration(1., ratio)
            .unwrap()
            .stiffness
            .sqrt()
    };
    assert!(close(w(0.625), (w(0.6) + w(0.65)) / 2., 9));
}
