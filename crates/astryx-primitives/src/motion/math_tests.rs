//! Upstream `motion/decay.test.ts`, `projection.test.ts` and
//! `rubberBand.test.ts`.

use super::projection::{SnapRelease, select_snap_point};
use super::rubber_band::{rubber_band_slope, rubber_band_with};
use super::uikit::scroll;
use super::*;

fn close(a: f64, b: f64, digits: i32) -> bool {
    (a - b).abs() < 10f64.powi(-digits) / 2.
}

/// The rate applied literally, once per millisecond.
fn per_millisecond(from: f64, velocity: f64, rate: f64, ms: u32) -> (f64, f64) {
    let mut x = from;
    let mut v = velocity / 1000.;
    let k = -rate.ln();
    for _ in 0..ms {
        x += v * (1. - rate) / k;
        v *= rate;
    }
    (x, v * 1000.)
}

#[test]
fn decay_matches_a_per_millisecond_simulation() {
    for rate in [scroll::DECELERATION_NORMAL, scroll::DECELERATION_FAST] {
        for velocity in [-3000., -400., 150., 2500., 8000.] {
            let coast = solve_decay(100., velocity, Some(rate));
            for ms in [0, 1, 16, 100, 250, 600, 1500, 4000] {
                let (value, speed) = per_millisecond(100., velocity, rate, ms);
                let t = f64::from(ms) / 1000.;
                assert!(close(coast.value(t), value, 6), "{rate} {velocity} {ms}");
                assert!(close(coast.velocity(t), speed, 6));
            }
        }
    }
}

#[test]
fn decay_rests_just_past_the_geometric_projection() {
    let coast = solve_decay(0., 2000., None);
    assert!(close(coast.rest, 2000. / (-1000. * 0.998f64.ln()), 9));
    assert!(close(coast.value(60.), coast.rest, 6));
    assert!(close(
        coast.rest / project_distance(2000., scroll::DECELERATION_NORMAL),
        499.5 / 499.,
        3
    ));
    assert!(close(
        decay_rest_distance(2000., scroll::DECELERATION_NORMAL),
        coast.rest,
        9
    ));
}

#[test]
fn a_fast_rate_stops_sooner_and_shorter() {
    let normal = solve_decay(0., 2000., None);
    let fast = solve_decay(0., 2000., Some(scroll::DECELERATION_FAST));
    assert!(fast.rest < normal.rest);
    assert!(fast.settle_time(0.5) < normal.settle_time(0.5));
}

#[test]
fn decay_settle_time_bounds_every_later_distance_to_rest() {
    let coast = solve_decay(0., -5000., None);
    for threshold in [5., 0.5, 0.01] {
        let settle = coast.settle_time(threshold);
        assert!(close(
            (coast.value(settle) - coast.rest).abs(),
            threshold,
            9
        ));
        assert!((coast.value(settle + 0.5) - coast.rest).abs() < threshold);
    }
    assert_eq!(solve_decay(0., 0.1, None).settle_time(0.5), 0.);
}

#[test]
fn time_to_speed_lands_on_the_asked_speed() {
    let coast = solve_decay(0., 3000., None);
    let t = coast.time_to_speed(100.);
    assert!(close(coast.velocity(t), 100., 9));
    assert_eq!(coast.time_to_speed(5000.), 0.);
    assert_eq!(coast.time_to_speed(0.), f64::INFINITY);
}

#[test]
fn time_to_reach_finds_the_crossing_or_none() {
    let coast = solve_decay(0., 2000., None);
    let t = coast.time_to_reach(400.).unwrap();
    assert!(close(coast.value(t), 400., 9));
    assert_eq!(coast.time_to_reach(-10.), None);
    assert_eq!(coast.time_to_reach(coast.rest), None);
    assert_eq!(coast.time_to_reach(coast.rest + 1.), None);
    let back = solve_decay(0., -2000., None);
    assert!(close(
        back.value(back.time_to_reach(-400.).unwrap()),
        -400.,
        9
    ));
    assert_eq!(solve_decay(3., 0., None).time_to_reach(3.), Some(0.));
}

#[test]
#[should_panic]
fn decay_rejects_a_rate_of_1() {
    solve_decay(0., 1., Some(1.));
}

#[test]
#[should_panic]
fn decay_rejects_a_rate_of_0() {
    solve_decay(0., 1., Some(0.));
}

#[test]
fn project_distance_is_wwdc18s_formula() {
    for velocity in [-2500., -1., 0., 640., 3000.] {
        for rate in [0.998, 0.99] {
            assert!(close(
                project_distance(velocity, rate),
                velocity / 1000. * rate / (1. - rate),
                9
            ));
        }
    }
}

#[test]
fn projection_covers_499_ms_of_the_release_speed() {
    assert!(close(
        project_distance(1000., scroll::DECELERATION_NORMAL),
        499.,
        9
    ));
    assert!(close(
        project_distance(1000., scroll::DECELERATION_FAST),
        99.,
        9
    ));
}

#[test]
fn a_release_at_rest_projects_onto_itself() {
    assert_eq!(project(321., 0., scroll::DECELERATION_NORMAL), 321.);
}

#[test]
#[should_panic]
fn projection_rejects_a_rate_outside_0_1() {
    project_distance(10., 1.);
}

fn snap(points: &[f64], position: f64, velocity: f64) -> f64 {
    select_snap_point(points, SnapRelease::new(position, velocity))
        .unwrap()
        .value
}

#[test]
fn a_snap_goes_to_the_point_nearest_the_projected_rest() {
    let detents = [0., 300., 600.];
    assert_eq!(snap(&detents, 320., 900.), 600.);
    assert_eq!(snap(&detents, 320., 20.), 300.);
    assert_eq!(snap(&detents, 320., -900.), 0.);
}

#[test]
fn a_projection_past_the_ends_lands_on_the_end() {
    let detents = [0., 300., 600.];
    assert_eq!(snap(&detents, 580., 6000.), 600.);
    let low = select_snap_point(&detents, SnapRelease::new(10., -6000.)).unwrap();
    assert_eq!(low.index, 0);
}

#[test]
fn a_snap_reports_the_projection_it_chose_from() {
    let choice = select_snap_point(&[0., 300., 600.], SnapRelease::new(100., 400.)).unwrap();
    assert!(close(choice.projected, 100. + 0.4 * 499., 9));
}

#[test]
fn a_snap_travels_at_most_max_steps_from_from_index() {
    let pages = [0., 100., 200., 300., 400.];
    assert_eq!(snap(&pages, 110., 5000.), 400.);
    let limited = SnapRelease {
        from_index: Some(1),
        max_steps: Some(1),
        ..SnapRelease::new(110., 5000.)
    };
    assert_eq!(select_snap_point(&pages, limited).unwrap().value, 200.);
}

#[test]
fn an_exact_tie_breaks_toward_the_direction_of_travel() {
    let at = |velocity| {
        let release = SnapRelease {
            rate: 0.5,
            ..SnapRelease::new(50., velocity)
        };
        select_snap_point(&[0., 100.], release).unwrap().value
    };
    assert_eq!(at(0.0001), 100.);
    assert_eq!(at(-0.0001), 0.);
}

#[test]
fn a_snap_needs_at_least_one_point() {
    assert_eq!(select_snap_point(&[], SnapRelease::new(0., 0.)), None);
}

#[test]
fn the_rubber_band_reproduces_the_published_uiscrollview_measurement() {
    assert!((rubber_band(500., 812.) - 205.).abs() < 0.5);
    assert!((rubber_band(1000., 812.) - 328.).abs() < 0.5);
    assert!((rubber_band(1500., 812.) - 409.).abs() < 0.5);
}

#[test]
fn the_rubber_band_is_the_published_formula() {
    for x in [0., 1., 10., 80., 300., 2000.] {
        assert!(close(
            rubber_band(x, 600.),
            (1. - 1. / (x * 0.55 / 600. + 1.)) * 600.,
            9
        ));
    }
    assert!(close(
        rubber_band_with(100., 600., 0.3),
        (1. - 1. / (100. * 0.3 / 600. + 1.)) * 600.,
        9
    ));
}

#[test]
fn the_rubber_band_keeps_the_sign_and_starts_at_slope_c() {
    assert!(close(
        rubber_band(-120., 700.),
        -rubber_band(120., 700.),
        12
    ));
    assert!(rubber_band(1e9, 700.) < 700.);
    assert!(close(rubber_band_slope(0., 700.), 0.55, 12));
    for x in [0., 50., 400., 3000.] {
        let h = 1e-4;
        let numeric = (rubber_band(x + h, 700.) - rubber_band(x - h, 700.)) / (2. * h);
        assert!(close(rubber_band_slope(x, 700.), numeric, 6));
    }
}

#[test]
fn the_rubber_band_inverts_exactly() {
    for x in [-900., -40., 0., 3., 75., 640., 5000.] {
        assert!(close(rubber_band_inverse(rubber_band(x, 812.), 812.), x, 6));
    }
    assert_eq!(rubber_band_inverse(812., 812.), f64::INFINITY);
    assert_eq!(rubber_band_inverse(-900., 812.), f64::NEG_INFINITY);
}

#[test]
fn a_degenerate_dimension_bands_nothing() {
    assert_eq!(rubber_band(50., 0.), 0.);
    assert_eq!(rubber_band_inverse(50., 0.), 0.);
    assert_eq!(rubber_band_slope(50., 0.), 0.);
}

#[test]
fn the_clamp_passes_values_inside_and_bands_past_either_edge() {
    assert_eq!(rubber_band_clamp(120., 0., 500., 800.), 120.);
    assert!(close(
        rubber_band_clamp(-100., 0., 500., 800.),
        rubber_band(-100., 800.),
        12
    ));
    assert!(close(
        rubber_band_clamp(650., 0., 500., 800.),
        500. + rubber_band(150., 800.),
        12
    ));
}

#[test]
fn the_clamp_round_trips_through_unclamp() {
    for raw in [-600., -12., 0., 250., 500., 510., 1400.] {
        let shown = rubber_band_clamp(raw, 0., 500., 800.);
        assert!(close(rubber_band_unclamp(shown, 0., 500., 800.), raw, 6));
    }
}
