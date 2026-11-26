//! Upstream `motion/progress.test.ts`.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use super::progress::*;
use super::{FnCurve, MotionCurve, MotionProperty, SpringDescription, solve_spring};

use MotionProperty::{ScaleX, X, Y};

fn close(a: f64, b: f64, digits: i32) -> bool {
    (a - b).abs() < 10f64.powi(-digits) / 2.
}

const SPRING: SpringDescription = SpringDescription::ResponseDamping {
    response: 0.4,
    damping_ratio: 0.85,
};

fn presenting(velocities: &[(MotionProperty, f64)]) -> impl Fn(MotionProperty) -> f64 {
    let map: BTreeMap<MotionProperty, f64> = velocities.iter().copied().collect();
    move |property| map.get(&property).copied().unwrap_or(0.)
}

fn line(
    from: &[(MotionProperty, f64)],
    to: &[(MotionProperty, f64)],
    weights: &[(MotionProperty, f64)],
) -> Vec<ProgressChannel> {
    to.iter()
        .map(|(property, end)| {
            let start = from
                .iter()
                .find(|(p, _)| p == property)
                .map_or(0., |(_, v)| *v);
            let end = *end;
            let weight = weights
                .iter()
                .find(|(p, _)| p == property)
                .map_or(1., |(_, w)| *w);
            ProgressChannel::new(*property, move |p| start + (end - start) * p).weight(weight)
        })
        .collect()
}

#[test]
fn progress_curve_follows_the_spring_and_lands_exactly_on_1() {
    let curve = progress_curve(SPRING, 2., None);
    let spring = solve_spring(SPRING.to_spring(), 0., 1., 2.);
    assert_eq!(curve.at(0.), 0.);
    assert!(close(curve.velocity(0.), 2., 9));
    for t in [0.05, 0.1, 0.2, 0.3] {
        assert!(close(curve.at(t), spring.value(t), 12));
        assert!(close(curve.velocity(t), spring.velocity(t), 12));
    }
    assert!(curve.duration() > 0.3);
    assert_eq!(curve.at(curve.duration()), 1.);
    assert_eq!(curve.velocity(curve.duration()), 0.);
}

#[test]
fn map_progress_maps_the_value_and_the_velocity_by_the_chain_rule() {
    let progress = progress_curve(SPRING, 0., None).shared();
    let f = |p: f64| 100. + 300. * p * p;
    let mapped = map_progress(progress.clone(), f);
    assert_eq!(mapped.duration(), progress.duration());
    for t in [0.02, 0.1, 0.25] {
        let p = progress.at(t);
        assert!(close(mapped.at(t), f(p), 9));
        assert!(close(
            mapped.velocity(t),
            600. * p * progress.velocity(t),
            4
        ));
    }
    assert_eq!(mapped.at(progress.duration()), 400.);
}

#[test]
fn map_range_maps_linearly_inside_exactly_at_its_ends() {
    let dim = map_range([600., 200.], [0., 0.4], true);
    assert_eq!(dim(600.), 0.);
    assert_eq!(dim(200.), 0.4);
    assert!(close(dim(400.), 0.2, 12));
    assert!(close(dim(500.), 0.1, 12));
}

#[test]
fn map_range_holds_its_ends_outside_or_runs_on_unclamped() {
    let dim = map_range([600., 200.], [0., 0.4], true);
    assert_eq!(dim(800.), 0.);
    assert_eq!(dim(0.), 0.4);
    let open = map_range([600., 200.], [0., 0.4], false);
    assert!(close(open(800.), -0.2, 12));
    assert!(close(open(0.), 0.6, 12));
}

#[test]
fn map_range_steps_at_an_empty_input() {
    let step = map_range([10., 10.], [0., 1.], true);
    assert_eq!(step(9.), 0.);
    assert_eq!(step(10.), 1.);
    assert_eq!(step(11.), 1.);
}

fn sheet_y() -> super::SharedCurve {
    FnCurve::new(0.6, |t| 700. - 1000. * t, |_| -1000.).shared()
}

#[test]
fn map_progress_follows_any_curve() {
    let dim = map_progress(sheet_y(), map_range([600., 200.], [0., 0.4], true));
    assert_eq!(dim.duration(), 0.6);
    assert!(close(dim.at(0.3), 0.4 * (200. / 400.), 12));
    assert!(close(dim.velocity(0.3), 1., 4));
}

#[test]
fn map_progress_is_still_outside_the_range() {
    let dim = map_progress(sheet_y(), map_range([600., 200.], [0., 0.4], true));
    for t in [0.05, 0.55] {
        assert_eq!(dim.velocity(t).abs(), 0.);
    }
    assert_eq!(dim.at(0.05), 0.);
    assert_eq!(dim.at(0.55), 0.4);
}

#[test]
fn presented_progress_velocity_along_a_parallel_path() {
    let channels = line(&[(X, 0.), (Y, 0.)], &[(X, 200.), (Y, 100.)], &[]);
    assert!(close(
        presented_progress_velocity(presenting(&[(X, 400.), (Y, 200.)]), &channels),
        2.,
        6
    ));
}

#[test]
fn presented_progress_velocity_keeps_only_the_part_along_the_path() {
    let channels = line(&[(X, 0.), (Y, 0.)], &[(X, 100.), (Y, 0.)], &[]);
    assert!(close(
        presented_progress_velocity(presenting(&[(X, 0.), (Y, 500.)]), &channels),
        0.,
        9
    ));
    assert!(close(
        presented_progress_velocity(presenting(&[(X, -50.), (Y, 500.)]), &channels),
        -0.5,
        6
    ));
}

#[test]
fn presented_progress_velocity_weighs_each_channel_into_one_unit() {
    let channels = line(
        &[(X, 0.), (ScaleX, 0.5)],
        &[(X, 50.), (ScaleX, 1.)],
        &[(ScaleX, 300.)],
    );
    assert!(close(
        presented_progress_velocity(presenting(&[(X, 100.), (ScaleX, 1.)]), &channels),
        2.,
        6
    ));
}

#[test]
fn presented_progress_velocity_is_0_on_a_still_path_or_from_rest() {
    assert_eq!(
        presented_progress_velocity(
            presenting(&[(X, 100.)]),
            &line(&[(X, 10.)], &[(X, 10.)], &[])
        ),
        0.
    );
    assert_eq!(
        presented_progress_velocity(presenting(&[]), &line(&[(X, 0.)], &[(X, 10.)], &[])),
        0.
    );
}

#[test]
fn presented_progress_velocity_reads_every_channel_at_the_one_time_it_is_given() {
    let times = Rc::new(RefCell::new(Vec::new()));
    let engine = super::animator::MotionEngine::with_clock(|| 0.);
    let channels = line(&[(X, 0.), (Y, 0.)], &[(X, 1.), (Y, 1.)], &[]);
    let seen = times.clone();
    let reader = |property| {
        seen.borrow_mut().push(1234.);
        engine.get_velocity(super::animator::MotionKey::from(property), Some(1234.))
    };
    presented_progress_velocity(reader, &channels);
    assert_eq!(*times.borrow(), vec![1234., 1234.]);
    assert_eq!(
        presented_progress_velocity_of(&engine, &channels, Some(1234.)),
        0.
    );
}
