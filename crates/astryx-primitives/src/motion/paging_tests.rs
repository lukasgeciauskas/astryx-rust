//! Upstream `motion/paging.test.ts` and `resolveSnapPoint.test.ts`.

use super::MotionCurve;
use super::paging::*;
use super::resolve_snap_point::*;
use super::uikit::paging as table;

fn close(a: f64, b: f64, digits: i32) -> bool {
    (a - b).abs() < 10f64.powi(-digits) / 2.
}

/// contentOffset per frame after a release (ms, px), UIKit lab, iOS 26.2
/// simulator, 402 pt pages: (from, velocity, to, frames).
#[allow(clippy::type_complexity)]
const SETTLES: [(&str, f64, f64, f64, &[[f64; 2]]); 6] = [
    (
        "forwardFrom60",
        633.33,
        0.,
        804.,
        &[
            [0.7, 633.33],
            [17.4, 639.],
            [34.0, 652.],
            [50.7, 668.67],
            [67.4, 686.67],
            [84.0, 704.67],
            [100.7, 721.33],
            [117.4, 736.33],
            [134.0, 749.67],
            [150.7, 761.],
            [167.4, 770.67],
            [184.0, 778.33],
            [200.7, 784.67],
            [217.4, 789.67],
            [234.0, 793.67],
            [250.7, 796.67],
            [267.4, 799.],
            [284.0, 800.67],
            [300.7, 802.],
            [317.4, 803.],
            [334.0, 803.67],
            [350.7, 804.],
        ],
    ),
    (
        "backFrom46",
        589.,
        0.,
        402.,
        &[
            [0.6, 589.],
            [17.3, 583.],
            [33.9, 569.],
            [50.6, 550.67],
            [67.3, 531.],
            [83.9, 511.33],
            [100.6, 493.],
            [117.3, 476.33],
            [133.9, 461.67],
            [150.6, 449.33],
            [167.3, 439.],
            [183.9, 430.33],
            [200.6, 423.33],
            [217.3, 417.67],
            [233.9, 413.33],
            [250.6, 410.],
            [267.3, 407.33],
            [283.9, 405.33],
            [300.6, 404.],
            [317.3, 403.],
            [333.9, 402.33],
            [350.6, 402.],
        ],
    ),
    (
        "forwardFromMinus23",
        311.33,
        0.,
        402.,
        &[
            [0.6, 311.33],
            [17.2, 314.33],
            [33.9, 321.],
            [50.6, 330.],
            [67.2, 339.67],
            [83.9, 349.33],
            [100.6, 358.33],
            [117.2, 366.33],
            [133.9, 373.33],
            [150.6, 379.33],
            [167.2, 384.33],
            [183.9, 388.33],
            [200.6, 391.67],
            [217.2, 394.33],
            [233.9, 396.33],
            [250.6, 398.],
            [267.2, 399.33],
            [283.9, 400.33],
            [300.6, 401.],
            [317.2, 401.33],
            [333.9, 401.67],
            [350.6, 402.],
        ],
    ),
    (
        "flick314From24",
        499.67,
        314.,
        804.,
        &[
            [15.0, 499.67],
            [31.7, 513.33],
            [48.4, 538.33],
            [65.0, 569.],
            [81.7, 601.67],
            [98.4, 633.33],
            [115.0, 662.67],
            [131.7, 689.],
            [148.4, 712.],
            [165.0, 731.67],
            [181.7, 748.],
            [198.4, 761.33],
            [215.0, 772.],
            [231.7, 780.33],
            [248.4, 787.],
            [265.0, 792.],
            [281.7, 796.],
            [298.4, 799.],
            [315.0, 801.],
            [331.7, 802.33],
            [348.4, 803.33],
            [365.0, 804.],
        ],
    ),
    (
        "flick565From24",
        497.33,
        565.,
        804.,
        &[
            [-3.2, 497.33],
            [13.5, 514.33],
            [30.2, 541.33],
            [46.8, 573.],
            [63.5, 606.],
            [80.2, 637.67],
            [96.8, 667.],
            [113.5, 693.],
            [130.2, 715.33],
            [146.8, 734.33],
            [163.5, 750.],
            [180.2, 763.],
            [196.8, 773.33],
            [213.5, 781.67],
            [230.2, 788.],
            [246.8, 793.],
            [263.5, 796.67],
            [280.2, 799.33],
            [296.8, 801.33],
            [313.5, 802.67],
            [330.2, 803.67],
            [346.8, 804.],
        ],
    ),
    (
        "overscrollReturn",
        -114.33,
        0.,
        0.,
        &[
            [0., -114.33],
            [10.6, -113.],
            [27.3, -105.67],
            [43.9, -95.33],
            [60.6, -83.33],
            [77.3, -71.],
            [93.9, -59.33],
            [110.6, -48.67],
            [127.3, -39.33],
            [143.9, -31.33],
            [160.6, -24.67],
            [177.3, -19.],
            [193.9, -14.33],
            [210.6, -10.67],
            [227.3, -7.67],
            [243.9, -5.33],
            [260.6, -3.67],
            [277.3, -2.33],
            [293.9, -1.33],
            [310.6, -0.67],
            [327.3, -0.33],
            [343.9, 0.],
        ],
    ),
];

#[allow(clippy::type_complexity)]
const ANIMATED: [(&str, f64, f64, &[[f64; 2]]); 2] = [
    (
        "twoPages",
        402.,
        1206.,
        &[
            [0., 402.],
            [16.7, 408.],
            [33.3, 426.33],
            [50., 456.],
            [66.7, 496.],
            [83.3, 545.67],
            [100., 603.],
            [116.7, 666.67],
            [133.3, 734.33],
            [150., 804.],
            [166.7, 873.67],
            [183.3, 941.33],
            [200., 1005.],
            [216.7, 1062.33],
            [233.3, 1112.],
            [250., 1152.],
            [266.7, 1181.67],
            [283.3, 1200.],
            [300., 1206.],
        ],
    ),
    (
        "fivePages",
        0.,
        2010.,
        &[
            [0., 0.],
            [16.7, 15.33],
            [33.3, 60.67],
            [50., 134.67],
            [66.7, 235.],
            [83.3, 359.],
            [100., 502.67],
            [116.7, 661.33],
            [133.3, 830.33],
            [150., 1005.],
            [166.7, 1179.67],
            [183.3, 1348.67],
            [200., 1507.67],
            [216.7, 1651.],
            [233.3, 1775.],
            [250., 1875.33],
            [266.7, 1949.33],
            [283.3, 1994.67],
            [300., 2010.],
        ],
    ),
];

/// The residuals' RMS and max: the integrator runs floor(elapsed ms) steps
/// on each frame (`steps`); a timed curve is read at the frame times.
fn errors(curve: &dyn MotionCurve, frames: &[[f64; 2]], steps_clock: bool) -> (f64, f64) {
    let mut steps = 0.;
    let residuals: Vec<f64> = frames
        .iter()
        .enumerate()
        .map(|(i, [ms, x])| {
            if steps_clock && i > 0 {
                steps += (ms - frames[i - 1][0] + 1e-6).floor();
            }
            let t = if steps_clock {
                steps / table::integrator::STEPS_PER_SECOND
            } else {
                ms / 1000.
            };
            curve.at(t) - x
        })
        .collect();
    let rms = (residuals.iter().map(|r| r * r).sum::<f64>() / residuals.len() as f64).sqrt();
    let max = residuals.iter().map(|r| r.abs()).fold(0., f64::max);
    (rms, max)
}

#[test]
fn paging_settle_velocity_zeroes_a_release_below_the_floor() {
    assert_eq!(paging_settle_velocity(200.), 0.);
    assert_eq!(paging_settle_velocity(-200.), 0.);
}

#[test]
fn paging_settle_velocity_keeps_a_velocity_at_or_above_the_floor() {
    assert_eq!(paging_settle_velocity(249.), 249.);
    assert_eq!(paging_settle_velocity(-260.), -260.);
}

#[test]
fn solve_paging_reproduces_the_recorded_settles_within_a_pixel() {
    for (name, from, velocity, to, frames) in SETTLES {
        let curve = solve_paging(from, velocity, to, 1.);
        let (rms, max) = errors(&curve, frames, true);
        assert!(rms < 0.35 && max < 0.8, "{name}: rms {rms} max {max}");
    }
}

#[test]
fn solve_paging_ends_exactly_on_the_page_about_0_33_s_after_a_release_at_rest() {
    let curve = solve_paging(633.33, 0., 804., 1.);
    assert_eq!(curve.at(curve.duration), 804.);
    assert_eq!(curve.velocity(curve.duration), 0.);
    assert!(curve.duration > 0.3 && curve.duration < 0.36);
}

#[test]
fn solve_paging_starts_at_the_release_with_uikits_step_velocity() {
    let curve = solve_paging(500., 1000., 804., 1.);
    assert_eq!(curve.at(0.), 500.);
    assert!(close(curve.velocity(0.), 960., 6));
}

#[test]
fn solve_paging_is_continuous_and_monotonic_from_rest() {
    let curve = solve_paging(589., 0., 402., 1.);
    let mut previous = curve.at(0.);
    let mut t = 0.001;
    while t <= curve.duration {
        let value = curve.at(t);
        assert!(value <= previous + 1e-9);
        assert!((value - previous).abs() < 3.);
        previous = value;
        t += 0.001;
    }
}

#[test]
fn solve_paging_carries_a_forward_flick_on_with_a_tiny_overshoot() {
    let curve = solve_paging(450., 600., 804., 1.);
    let mut max = f64::NEG_INFINITY;
    let mut t = 0.;
    while t <= curve.duration {
        max = max.max(curve.at(t));
        t += 0.004;
    }
    assert!(max < 806.);
    assert_eq!(curve.at(curve.duration), 804.);
}

#[test]
fn solve_paging_settles_at_once_on_the_page_at_rest() {
    let curve = solve_paging(804., 0., 804., 1.);
    assert_eq!(curve.steps, 0);
    assert_eq!(curve.duration, 0.);
    assert_eq!(curve.at(0.1), 804.);
}

#[test]
fn solve_paging_at_a_tempo_keeps_its_shape_and_takes_tempo_times_as_long() {
    for tempo in [0.75, 1.5, 3.] {
        let measured = solve_paging(633.33, 0., 804., 1.);
        let paced = solve_paging(633.33, 0., 804., tempo);
        assert_eq!(paced.steps, measured.steps);
        assert!(close(paced.duration, measured.duration * tempo, 9));
        let mut t = 0.;
        while t <= measured.duration {
            assert!(close(paced.at(t * tempo), measured.at(t), 6));
            t += 0.01;
        }
        assert_eq!(paced.at(paced.duration), 804.);
    }
}

#[test]
fn solve_paging_at_any_tempo_starts_at_uikits_step_velocity() {
    for tempo in [0.75, 1.5, 3.] {
        let curve = solve_paging(500., 1000., 804., tempo);
        assert!(close(curve.velocity(0.), 960., 6));
        assert_eq!(curve.at(curve.duration), 804.);
    }
}

#[test]
fn solve_paging_at_tempo_1_is_the_measured_settle() {
    let measured = solve_paging(450., 600., 804., 1.);
    let bad = solve_paging(450., 600., 804., f64::NAN);
    assert_eq!(bad.steps, measured.steps);
    let mut t = 0.;
    while t <= measured.duration {
        assert_eq!(bad.at(t), measured.at(t));
        t += 0.004;
    }
}

#[test]
fn solve_paging_runs_the_documented_integrator() {
    let curve = solve_paging(0., 0., 100., 1.);
    let one_step = 100. * table::integrator::SPRING_PULL * table::integrator::FRICTION;
    assert!(close(
        curve.at(1. / table::integrator::STEPS_PER_SECOND),
        one_step,
        9
    ));
}

#[test]
fn the_animated_page_scroll_reproduces_the_recorded_moves() {
    for (name, from, to, frames) in ANIMATED {
        let curve = solve_animated_page_scroll(from, to, None, None, None);
        let (rms, max) = errors(&curve, frames, false);
        assert!(rms < 0.5 && max < 1.2, "{name}: rms {rms} max {max}");
    }
}

#[test]
fn the_animated_page_scroll_lasts_0_3_s_and_ends_at_rest() {
    for distance in [402., 804., 2010.] {
        let curve = solve_animated_page_scroll(0., distance, None, None, None);
        assert_eq!(curve.duration(), 0.3);
        assert_eq!(curve.at(0.3), distance);
        assert_eq!(curve.velocity(0.3), 0.);
        assert_eq!(curve.velocity(0.), 0.);
    }
}

#[test]
fn a_page_scroll_retarget_starts_at_its_velocity_and_ends_on_time() {
    for velocity in [900., -900., 0.] {
        let curve = solve_animated_page_scroll(100., 804., None, None, Some(velocity));
        assert_eq!(curve.at(0.), 100.);
        assert_eq!(curve.velocity(0.), velocity);
        assert!(close(curve.velocity(1e-4), velocity, -1));
        assert_eq!(curve.at(0.3), 804.);
        assert_eq!(curve.velocity(0.3), 0.);
        assert!(close(curve.velocity(0.2999), 0., -1));
    }
}

#[test]
fn the_page_scroll_peaks_at_half_pi_times_the_average_speed() {
    let curve = solve_animated_page_scroll(0., 402., None, None, None);
    let ratio = curve.velocity(0.15) / (std::f64::consts::PI / 2. * (402. / 0.3));
    assert!(close(ratio, 1., 2));
}

#[test]
fn the_page_scroll_follows_the_sinusoid() {
    let curve = solve_animated_page_scroll(0., 402., None, None, None);
    let mut t = 0.;
    while t <= 0.3 {
        let sine = 402. * (1. - (std::f64::consts::PI * t / 0.3).cos()) / 2.;
        assert!((curve.at(t) - sine).abs() < 0.1);
        t += 0.01;
    }
}

#[test]
fn the_page_scroll_takes_a_themes_duration_and_curve() {
    let curve = solve_animated_page_scroll(0., 100., Some(500.), Some([0., 0., 1., 1.]), None);
    assert_eq!(curve.duration(), 0.5);
    assert!(close(curve.at(0.25), 50., 1));
}

fn sheet(points: &[f64], position: f64, velocity: f64) -> SnapPointQuery {
    SnapPointQuery {
        points: points.to_vec(),
        position,
        velocity,
        flick_threshold: 1008.,
        tolerance: 1.,
        projection: Some(SnapPointProjection {
            seconds: 0.1,
            min_speed: 250.,
        }),
        ..Default::default()
    }
}

fn plain(points: &[f64], position: f64, velocity: f64, flick_threshold: f64) -> SnapPointQuery {
    SnapPointQuery {
        points: points.to_vec(),
        position,
        velocity,
        flick_threshold,
        ..Default::default()
    }
}

fn land(query: &SnapPointQuery) -> SnapPointRelease {
    resolve_snap_point(query).unwrap()
}

#[test]
fn a_slow_release_lands_on_the_point_nearest_where_it_was_let_go() {
    assert_eq!(
        land(&plain(&[0., 300., 640.], 140., 0., 48.)),
        SnapPointRelease {
            index: 0,
            projected: 140.
        }
    );
    assert_eq!(land(&plain(&[0., 300., 640.], 160., 20., 48.)).index, 1);
}

#[test]
fn projects_nothing_below_the_projection_speed() {
    assert_eq!(
        land(&sheet(&[0., 300., 640.], 140., 240.)),
        SnapPointRelease {
            index: 0,
            projected: 140.
        }
    );
    let projected = land(&sheet(&[0., 300., 640.], 140., 600.));
    assert_eq!(projected.index, 1);
    assert!(close(projected.projected, 200., 6));
}

#[test]
fn a_flick_goes_at_least_to_the_next_point_beyond() {
    let flick = land(&sheet(&[0., 300., 640.], 300., 1200.));
    assert_eq!(flick.index, 2);
    assert!(close(flick.projected, 420., 6));
    assert_eq!(land(&plain(&[0., 300.], 2., 60., 48.)).index, 1);
    assert_eq!(land(&sheet(&[0., 300., 640.], 620., -1100.)).index, 1);
}

#[test]
fn flick_past_projection_lands_on_the_first_point_at_or_past_it() {
    let past = |position, velocity| {
        land(&SnapPointQuery {
            flick_past_projection: true,
            ..sheet(&[0., 200., 400.], position, velocity)
        })
        .index
    };
    assert_eq!(past(60., 1500.), 2);
    assert_eq!(past(60., 1200.), 1);
    assert_eq!(past(80., 1200.), 1);
    assert_eq!(past(60., 5000.), 2);
    assert_eq!(past(340., -1500.), 0);
    assert_eq!(past(60., 800.), 1);
    assert_eq!(land(&sheet(&[0., 200., 400.], 60., 1500.)).index, 1);
}

#[test]
fn a_point_within_the_tolerance_counts_as_reached() {
    let query = SnapPointQuery {
        tolerance: 1.,
        ..plain(&[0., 300., 640.], 300.5, 2000., 1008.)
    };
    assert_eq!(land(&query).index, 2);
    let last = SnapPointQuery {
        tolerance: 1.,
        ..plain(&[0., 300.], 310., 2000., 1008.)
    };
    assert_eq!(land(&last).index, 1);
}

#[test]
fn a_release_moving_back_toward_the_start_cancels() {
    let with_start = |position, velocity, start| SnapPointQuery {
        start: Some(start),
        ..sheet(&[0., 300., 640.], position, velocity)
    };
    assert_eq!(land(&with_start(280., -10., 0.)).index, 0);
    assert_eq!(land(&with_start(500., -1500., 300.)).index, 1);
    assert_eq!(land(&with_start(280., 10., 0.)).index, 1);
}

#[test]
fn a_tie_goes_the_way_a_projected_release_moves_else_to_the_earlier_point() {
    let at = |velocity: f64, projected: bool| {
        let projection = projected.then_some(SnapPointProjection {
            seconds: 0.1,
            min_speed: 250.,
        });
        land(&SnapPointQuery {
            projection,
            ..plain(
                &[0., 300.],
                150. - velocity * 0.1 * if projected { 1. } else { 0. },
                velocity,
                1008.,
            )
        })
        .index
    };
    assert_eq!(at(0., false), 0);
    assert_eq!(at(10., false), 0);
    assert_eq!(at(-10., false), 0);
    assert_eq!(at(400., true), 1);
    assert_eq!(at(-400., true), 0);
}

#[test]
fn resolve_snap_point_needs_at_least_one_point() {
    assert_eq!(resolve_snap_point(&plain(&[], 0., 0., 1.)), None);
}

#[test]
fn a_release_lands_at_most_max_steps_from_from_index() {
    let query = SnapPointQuery {
        from_index: Some(1),
        max_steps: Some(1),
        ..plain(&[0., 100., 200., 300., 400.], 110., 5000., 1008.)
    };
    assert_eq!(land(&query).index, 2);
}
