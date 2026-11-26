//! Upstream `createDragToSnapPoints.test.ts`'s first suite: the release policy the
//! factory calls with the drag's start (FR2, AST-094 FR37).

use astryx_primitives::motion::resolve_snap_point::{SnapPointQuery, resolve_snap_point};

use crate::drag_to_snap_points::DEFAULT_FLICK_THRESHOLD;

const POINTS: [f64; 3] = [0., 240., 480.];

fn at(offset: f64, velocity: f64, start_index: usize) -> usize {
    resolve_snap_point(&SnapPointQuery {
        points: POINTS.to_vec(),
        position: offset,
        velocity,
        flick_threshold: DEFAULT_FLICK_THRESHOLD,
        start: Some(POINTS[start_index]),
        ..Default::default()
    })
    .unwrap()
    .index
}

#[test]
fn a_slow_release_settles_on_the_nearest_snap_point() {
    assert_eq!(at(100., 300., 0), 0);
    assert_eq!(at(130., 300., 0), 1);
    assert_eq!(at(400., 0., 1), 2);
}

#[test]
fn a_release_past_the_default_flick_speed_carries_to_the_next_snap_point_in_its_direction() {
    assert_eq!(DEFAULT_FLICK_THRESHOLD, 1008.);
    assert_eq!(at(30., 1009., 0), 1);
    assert_eq!(at(30., 1007., 0), 0);
    assert_eq!(at(260., -1500., 2), 1);
}

#[test]
fn flick_threshold_changes_the_speed_that_carries() {
    let index = resolve_snap_point(&SnapPointQuery {
        points: POINTS.to_vec(),
        position: 30.,
        velocity: 600.,
        flick_threshold: 500.,
        start: Some(0.),
        ..Default::default()
    })
    .unwrap()
    .index;
    assert_eq!(index, 1);
}

#[test]
fn a_release_moving_back_toward_the_start_returns_to_the_snap_point_it_started_from() {
    assert_eq!(at(200., -50., 0), 0);
    assert_eq!(at(470., -3000., 0), 0);
    assert_eq!(at(10., 2000., 2), 2);
}

#[test]
fn a_flick_past_the_last_snap_point_stays_on_the_last() {
    assert_eq!(at(500., 2000., 1), 2);
    assert_eq!(at(-20., -2000., 1), 0);
}
