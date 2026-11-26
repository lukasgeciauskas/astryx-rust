//! Wheel rules and physics, after upstream's `wheelOptions` and
//! `wheelPhysics` cases.

use super::options::*;
use super::physics::*;

fn rows(n: i64, enabled: impl Fn(i64) -> bool) -> Vec<WheelOption> {
    (0..n)
        .map(|v| WheelOption::new(v, v.to_string()).disabled(!enabled(v)))
        .collect()
}

#[test]
fn fr13_a_rest_on_a_disabled_row_goes_to_the_nearest_enabled_one() {
    // Hours 0-23 with 09:00-17:00 bounds: stopping on 7 rests on 9.
    let hours = rows(24, |h| (9..=17).contains(&h));
    assert_eq!(nearest_enabled_index(&hours, 7, 10), 9);
    assert_eq!(nearest_enabled_index(&hours, 12, 10), 12);
    // A tie goes toward the committed row.
    let gaps = rows(5, |v| v != 2);
    assert_eq!(nearest_enabled_index(&gaps, 2, 0), 1);
    assert_eq!(nearest_enabled_index(&gaps, 2, 4), 3);
    assert_eq!(nearest_enabled_index(&rows(3, |_| false), 1, 2), 2);
}

#[test]
fn fr13_a_looping_rest_turns_the_short_way() {
    // Months with only January and February enabled: a rest on December
    // turns one row on to January.
    assert_eq!(resting_loop_step(|p| p.rem_euclid(12) < 2, 11, 1, 12), 1);
    assert_eq!(resting_loop_step(|_| true, 5, 0, 12), 0);
}

#[test]
fn fr17_keys_move_through_enabled_rows_in_time_order() {
    // A 12:00-17:00 bound on a 12-hour wheel: rows 1..12, ordered by hour.
    let half: Vec<WheelOption> = (1..=12)
        .map(|row| {
            let hour = if row == 12 { 12 } else { row + 12 };
            WheelOption::new(row, row.to_string())
                .order(hour)
                .disabled(!(12..=17).contains(&hour))
        })
        .collect();
    // From 12 PM (index 11), ArrowDown goes to 1 PM (index 0).
    assert_eq!(stepped_index(&half, 11, 1), Some(0));
    // From 5 PM (index 4) nothing is after it.
    assert_eq!(stepped_index(&half, 4, 1), None);
    assert_eq!(end_index(&half, false), Some(11));
    assert_eq!(end_index(&half, true), Some(4));
}

#[test]
fn fr17_a_looping_key_crosses_the_seam_never_a_lap() {
    // Minutes: ArrowDown on :59 moves one row on, to :00.
    assert_eq!(looped_step(|_| true, 59, 1, 60), Some(1));
    // PageDown from :55 moves ten rows on, across the seam.
    assert_eq!(looped_step(|_| true, 55, 10, 60), Some(10));
    // One enabled row: nothing moves.
    assert_eq!(looped_step(|p| p.rem_euclid(60) == 7, 7, 1, 60), None);
    assert_eq!(looped_step(|_| true, 3, 60, 60), None);
}

#[test]
fn loop_copies_turn_the_short_way() {
    assert_eq!(nearest_loop_copy(0, 59., 60), 60);
    assert_eq!(nearest_loop_copy(59, 0., 60), -1);
    assert_eq!(nearest_loop_copy(5, 4., 60), 5);
}

#[test]
fn a_release_coasts_onto_the_row_uikit_projects() {
    let layout = WheelLayout {
        row_height: 32.,
        row_count: 60,
        looping: false,
    };
    let plan = plan_release(963.67, 933.4, &layout);
    assert_eq!(nearest_index(plan.curve.to(), &layout), 45);
    assert_eq!(plan.kind, PlanKind::Coast);
    assert!(
        (plan.curve.duration() - 2.3).abs() < 0.2,
        "{}",
        plan.curve.duration()
    );
    assert!((plan.curve.value(plan.curve.duration()) - 45. * 32.).abs() < 1e-9);
    // A slow release snaps to the nearest row.
    let slow = plan_release(100., 100., &layout);
    assert_eq!(slow.kind, PlanKind::Snap);
    assert_eq!(nearest_index(slow.curve.to(), &layout), 3);
    // Past an end it returns.
    let back = plan_release(-20., 0., &layout);
    assert_eq!(back.kind, PlanKind::Return);
    assert_eq!(nearest_index(back.curve.to(), &layout), 0);
    // A fling past the end coasts into it and bounces.
    let edge = plan_release(1800., 3000., &layout);
    assert_eq!(edge.kind, PlanKind::Edge);
    assert_eq!(nearest_index(edge.curve.to(), &layout), 59);
}

#[test]
fn a_drag_past_an_end_is_banded_and_reads_back() {
    let layout = WheelLayout {
        row_height: 28.,
        row_count: 12,
        looping: false,
    };
    let shown = drag_offset(-40., &layout);
    assert!(shown > -40. && shown < 0.);
    assert!((finger_offset(shown, &layout) + 40.).abs() < 1e-9);
    assert_eq!(drag_offset(50., &layout), 50.);
}

#[test]
fn a_row_reaches_the_band_as_its_centre_crosses_either_way() {
    assert!(reaches_row(31., 32., 32.));
    assert!(!reaches_row(1., 31., 32.));
    assert!(reaches_row(33., 32., 32.));
    assert!(!reaches_row(63., 33., 32.));
    assert!(!reaches_row(40., 40., 32.));
}

#[test]
fn a_release_ticks_once_per_row_it_passes_no_faster_than_the_pickers_interval() {
    let layout = WheelLayout {
        row_height: 32.,
        row_count: 60,
        looping: false,
    };
    let plan = plan_release(963.67, 933.4, &layout);
    let ticks = tick_times(&plan.curve, 32.);
    let rows = ((plan.curve.to() - plan.curve.value(0.)) / 32.).abs();
    assert!(
        !ticks.is_empty() && ticks.len() as f64 <= rows + 1.,
        "{} ticks over {rows} rows",
        ticks.len()
    );
    let interval = astryx_primitives::motion::uikit::picker::SELECTION_TICK_INTERVAL_MS / 1000.;
    assert!(ticks.windows(2).all(|w| w[1] - w[0] >= interval - 1e-9));
    assert!(ticks.iter().all(|t| *t > 0. && *t <= plan.curve.duration()));
    // A curve that never leaves its row ticks nothing.
    assert!(tick_times(&WheelCurve::Rest(64.), 32.).is_empty());
}
