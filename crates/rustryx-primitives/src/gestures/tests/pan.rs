//! Upstream `__tests__/PanGestureRecognizer.test.ts`.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use super::harness::*;
use crate::gestures::*;
use crate::motion::uikit::gestures::{pan as PAN, velocity as VELOCITY};

fn pan(name: &str) -> PanGestureRecognizer {
    PanGestureRecognizer::new(PanOptions::named(name))
}

fn pan_with(name: &str, options: PanOptions) -> PanGestureRecognizer {
    let mut options = options;
    options.base.name = Some(name.into());
    PanGestureRecognizer::new(options)
}

fn close(a: f64, b: f64, digits: i32) {
    assert!(
        (a - b).abs() < 10f64.powi(-digits) / 2.,
        "{a} is not close to {b}"
    );
}

/// Swipes on a UIPickerView column (iOS 26.2 simulator, HID touches): [ms
/// since touch-down, y in pt] per event, and the velocity UIKit handed
/// `scrollViewWillEndDragging`, in offset px/s.
/// A named recorded stream and its release, px/s.
type Release = (&'static str, &'static [(f64, f64)], f64);

const PICKER_RELEASES: [Release; 4] = [
    (
        "200 pt in 50 ms",
        &[(0., 368.), (46.48, 334.67), (72.92, 201.33)],
        933.4,
    ),
    (
        "200 pt in 100 ms",
        &[
            (0., 368.),
            (49.2, 351.33),
            (65.27, 301.33),
            (88.47, 251.33),
            (104.68, 218.),
            (129.4, 168.),
        ],
        2114.,
    ),
    (
        "200 pt in 200 ms",
        &[
            (0., 368.),
            (46.01, 359.67),
            (63.5, 334.67),
            (81.36, 318.),
            (97.69, 301.33),
            (120.12, 276.33),
            (137.2, 259.67),
            (171.83, 226.33),
            (196.13, 201.33),
            (212.66, 184.67),
            (230.83, 168.),
        ],
        1016.2,
    ),
    (
        "a 90 pt flick",
        &[(0., 353.), (41.5, 335.), (62.02, 281.)],
        543.6,
    ),
];

#[test]
fn begins_past_the_hysteresis_and_leaves_the_threshold_out_of_the_translation() {
    let h = create_harness();
    let pan = pan("pan");
    let hysteresis = PAN::HYSTERESIS;
    h.finger(1).down(100., 100., 0., on(&[&pan]));
    h.finger(1).move_to(100. + hysteresis - 0.1, 100., 16.);
    assert_eq!(pan.state(), GestureState::Possible);
    h.finger(1).move_to(100. + hysteresis + 2., 100., 32.);
    assert_eq!(pan.state(), GestureState::Began);
    assert_eq!(pan.translation(), Point::new(2., 0.));
    assert_eq!(pan.travel_at_began(), Point::new(hysteresis, 0.));
    assert_eq!(pan.total_translation(), Point::new(hysteresis + 2., 0.));
    h.finger(1).move_to(150., 120., 48.);
    assert_eq!(pan.state(), GestureState::Changed);
    assert_eq!(pan.translation(), Point::new(40., 20.));
    assert_eq!(pan.total_translation(), Point::new(50., 20.));
}

#[test]
fn reads_each_presss_begin_threshold_from_its_first_touch() {
    let h = create_harness();
    let mut options = PanOptions {
        hysteresis: Hysteresis::PerPress(Rc::new(|touch: &GestureTouch| {
            if touch.pointer_type() == PointerKind::Mouse {
                0.
            } else {
                10.
            }
        })),
        ..PanOptions::default()
    };
    options.base.allowed_pointer_types = Some(vec![PointerKind::Touch, PointerKind::Mouse]);
    let pan = pan_with("pan", options);
    h.finger(1)
        .down(100., 100., 0., on(&[&pan]).pointer(PointerKind::Mouse))
        .move_to(101., 100., 16.);
    assert_eq!(pan.state(), GestureState::Began);
    h.finger(1).up(32.);
    h.finger(2)
        .down(100., 100., 100., on(&[&pan]))
        .move_to(105., 100., 116.);
    assert_eq!(pan.state(), GestureState::Possible);
    h.finger(2).move_to(111., 100., 132.);
    assert_eq!(pan.state(), GestureState::Began);
}

#[test]
fn applies_the_hysteresis_per_axis_as_uikit_does() {
    let h = create_harness();
    let pan = pan("pan");
    h.finger(1)
        .down(0., 0., 0., on(&[&pan]))
        .move_to(7.5, 7.5, 16.);
    assert_eq!(pan.state(), GestureState::Possible);
    h.finger(1).move_to(10.5, 10.5, 32.);
    assert_eq!(pan.state(), GestureState::Began);
    assert_eq!(pan.translation(), Point::new(0.5, 0.5));
    let slope = create_harness();
    let other = pan_with("other", PanOptions::default());
    slope
        .finger(1)
        .down(0., 0., 0., on(&[&other]))
        .move_to(9., 3., 16.)
        .move_to(11., 3.5, 32.);
    assert_eq!(other.translation(), Point::new(1., 0.));
}

#[test]
fn ends_with_the_release_velocity_in_px_per_s() {
    let h = create_harness();
    let pan = pan("pan");
    let release = Rc::new(Cell::new(Point::ZERO));
    let record = release.clone();
    pan.add_listener(move |r| {
        if r.state() == GestureState::Ended {
            record.set(r.velocity());
        }
    });
    let mut finger = h.finger(1).down(0., 0., 0., on(&[&pan]));
    let mut t = 8.;
    while t <= 160. {
        finger = finger.move_to(t, 0., t);
        t += 8.;
    }
    finger.up(168.);
    close(release.get().x, 1000., 0);
    close(release.get().y, 0., 5);
}

#[test]
fn releases_at_0_after_a_rest_longer_than_the_stillness_interval_ast_100_fr2() {
    let stillness = VELOCITY::STILLNESS_MS;
    let release = |lift_at: f64| {
        let h = create_harness();
        let pan = pan("pan");
        let release = Rc::new(Cell::new(Point::new(-1., -1.)));
        let record = release.clone();
        pan.add_listener(move |r| {
            if r.state() == GestureState::Ended {
                record.set(r.velocity());
            }
        });
        let finger = h.finger(1).down(0., 0., 0., on(&[&pan]));
        let finger = finger.drag(200., 0., 100., 10);
        finger.up(lift_at);
        release.get()
    };
    close(release(100. + stillness).x, 2000., 6);
    assert_eq!(release(100. + stillness + 60.), Point::ZERO);
}

#[test]
fn releases_a_scrolling_surface_as_uiscrollview_does() {
    for (name, stream, expected) in PICKER_RELEASES {
        let h = create_harness();
        let pan = pan_with(
            "pan",
            PanOptions {
                direction: PanDirection::Vertical,
                ..PanOptions::default()
            },
        );
        let scroll = Rc::new(Cell::new(f64::NAN));
        let record = scroll.clone();
        pan.add_listener(move |r| {
            if r.state() == GestureState::Ended {
                record.set(-r.scroll_velocity().y);
            }
        });
        let (first, rest) = stream.split_first().expect("a stream");
        let mut finger = h.finger(1).down(100., first.1, first.0, on(&[&pan]));
        for (time, y) in rest {
            finger = finger.move_to(100., *y, *time);
        }
        finger.up(rest[rest.len() - 1].0);
        assert!(
            (scroll.get() - expected).abs() < 0.5,
            "{name}: {}",
            scroll.get()
        );
    }
}

#[test]
fn reads_a_dragged_surfaces_release_as_velocity_in_not_the_scroll_view_blend() {
    let h = create_harness();
    let pan = pan_with(
        "pan",
        PanOptions {
            direction: PanDirection::Vertical,
            ..PanOptions::default()
        },
    );
    let drag = Rc::new(Cell::new(f64::NAN));
    let scroll = Rc::new(Cell::new(f64::NAN));
    let (d, s) = (drag.clone(), scroll.clone());
    pan.add_listener(move |r| {
        if r.state() == GestureState::Ended {
            d.set(-r.velocity().y);
            s.set(-r.scroll_velocity().y);
        }
    });
    let stream = PICKER_RELEASES[0].1;
    let (first, rest) = stream.split_first().expect("a stream");
    let mut finger = h.finger(1).down(100., first.1, first.0, on(&[&pan]));
    for (time, y) in rest {
        finger = finger.move_to(100., *y, *time);
    }
    finger.up(rest[rest.len() - 1].0);
    assert!((drag.get() - 1582.3).abs() < 0.5, "{}", drag.get());
    assert!((scroll.get() - 933.4).abs() < 0.5, "{}", scroll.get());
}

#[test]
fn fails_a_drag_against_its_direction() {
    let vertical = create_harness();
    let horizontal = pan_with(
        "h",
        PanOptions {
            direction: PanDirection::Horizontal,
            ..PanOptions::default()
        },
    );
    vertical
        .finger(1)
        .down(0., 0., 0., on(&[&horizontal]))
        .drag(3., 20., 50., 8);
    assert_eq!(horizontal.state(), GestureState::Failed);
    let up = create_harness();
    let down = pan_with(
        "down",
        PanOptions {
            direction: PanDirection::Down,
            ..PanOptions::default()
        },
    );
    up.finger(1)
        .down(0., 100., 0., on(&[&down]))
        .drag(0., -30., 50., 8);
    assert_eq!(down.state(), GestureState::Failed);
}

fn vertical_ratio(name: &str, ratio: f64) -> PanGestureRecognizer {
    pan_with(
        name,
        PanOptions {
            direction: PanDirection::Vertical,
            axis_ratio: ratio,
            ..PanOptions::default()
        },
    )
}

#[test]
fn decides_by_10_px_with_an_axis_ratio_above_1_between_the_lines_it_fails_on_its_axis_it_begins() {
    let between = create_harness();
    let pan = vertical_ratio("pan", 2.);
    between
        .finger(1)
        .down(0., 0., 0., on(&[&pan]))
        .move_to(8., 12., 16.);
    assert_eq!(pan.state(), GestureState::Failed);
    assert!(between.claims().is_empty());
    let along = create_harness();
    let steep = vertical_ratio("steep", 2.);
    along
        .finger(1)
        .down(0., 0., 0., on(&[&steep]))
        .move_to(4., 12., 16.);
    assert_eq!(steep.state(), GestureState::Began);
    let diagonal = create_harness();
    let side = vertical_ratio("side", 2.);
    diagonal
        .finger(1)
        .down(0., 0., 0., on(&[&side]))
        .move_to(10., 9., 16.);
    assert_eq!(side.state(), GestureState::Failed);
}

#[test]
fn claims_drags_up_to_atan_of_1_over_axis_ratio_off_its_axis_below_1() {
    let horizontal = |name: &str| {
        pan_with(
            name,
            PanOptions {
                direction: PanDirection::Horizontal,
                axis_ratio: 0.5,
                ..PanOptions::default()
            },
        )
    };
    let steep = create_harness();
    let pan = horizontal("pan");
    steep
        .finger(1)
        .down(0., 0., 0., on(&[&pan]))
        .move_to(10., 17., 16.);
    assert_eq!(pan.state(), GestureState::Began);
    let steeper = create_harness();
    let other = horizontal("other");
    steeper
        .finger(1)
        .down(0., 0., 0., on(&[&other]))
        .move_to(8., 20., 16.);
    assert_eq!(other.state(), GestureState::Failed);
}

#[test]
fn decides_the_sign_of_a_one_way_pan_once_its_axis_is_decided() {
    let h = create_harness();
    let down = pan_with(
        "down",
        PanOptions {
            direction: PanDirection::Down,
            axis_ratio: 1.2,
            ..PanOptions::default()
        },
    );
    h.finger(1)
        .down(0., 100., 0., on(&[&down]))
        .move_to(8., 109., 16.);
    assert_eq!(down.state(), GestureState::Possible);
    h.finger(1).move_to(8., 70., 32.);
    assert_eq!(down.state(), GestureState::Failed);
}

#[test]
fn locks_to_the_dominant_axis_when_asked() {
    let h = create_harness();
    let pan = pan_with(
        "pan",
        PanOptions {
            lock_axis: true,
            ..PanOptions::default()
        },
    );
    h.finger(1)
        .down(0., 0., 0., on(&[&pan]))
        .move_to(12., 4., 16.)
        .move_to(40., 30., 32.);
    assert_eq!(pan.locked_axis(), Some(crate::gestures::pan::Axis::X));
    assert_eq!(pan.translation(), Point::new(30., 0.));
}

fn one_finger(name: &str) -> PanGestureRecognizer {
    pan_with(
        name,
        PanOptions {
            maximum_number_of_touches: 1,
            ..PanOptions::default()
        },
    )
}

#[test]
fn ignores_a_second_finger_once_moving_so_it_never_breaks_the_first() {
    let h = create_harness();
    let pan = one_finger("pan");
    h.finger(1)
        .down(0., 0., 0., on(&[&pan]))
        .move_to(20., 0., 16.);
    assert_eq!(pan.state(), GestureState::Began);
    h.finger(2).down(200., 200., 30., on(&[&pan]));
    assert_eq!(pan.number_of_touches(), 1);
    h.finger(2).move_to(260., 260., 40.);
    h.finger(1).move_to(30., 0., 48.);
    assert_eq!(pan.translation(), Point::new(20., 0.));
    h.finger(2).up(60.);
    assert_eq!(pan.state(), GestureState::Changed);
    h.finger(1).up(70.);
    assert_eq!(pan.state(), GestureState::Possible);
    assert!(h.logged("pan:ended@70"));
}

#[test]
fn fails_when_a_second_finger_lands_before_it_began() {
    let h = create_harness();
    let pan = one_finger("pan");
    h.finger(1).down(0., 0., 0., on(&[&pan]));
    h.finger(2).down(50., 0., 10., on(&[&pan]));
    assert_eq!(pan.state(), GestureState::Failed);
}

#[test]
fn keeps_the_translation_continuous_when_fingers_join_and_leave() {
    let h = create_harness();
    let pan = pan("pan");
    h.finger(1)
        .down(0., 0., 0., on(&[&pan]))
        .move_to(30., 0., 16.);
    assert_eq!(pan.translation(), Point::new(20., 0.));
    h.finger(2).down(230., 0., 20., on(&[&pan]));
    assert_eq!(pan.translation(), Point::new(20., 0.));
    h.finger(1).move_to(40., 0., 32.);
    h.finger(2).move_to(240., 0., 32.);
    assert_eq!(pan.translation(), Point::new(30., 0.));
    h.finger(2).up(40.);
    assert_eq!(pan.translation(), Point::new(30., 0.));
    h.finger(1).move_to(50., 0., 48.);
    assert_eq!(pan.translation(), Point::new(40., 0.));
}

#[test]
fn waits_for_the_minimum_number_of_touches() {
    let h = create_harness();
    let pan = pan_with(
        "pan",
        PanOptions {
            minimum_number_of_touches: 2,
            ..PanOptions::default()
        },
    );
    h.finger(1)
        .down(0., 0., 0., on(&[&pan]))
        .move_to(40., 0., 16.);
    assert_eq!(pan.state(), GestureState::Possible);
    h.finger(2).down(100., 0., 20., on(&[&pan]));
    h.finger(1).move_to(60., 0., 32.);
    assert_eq!(pan.state(), GestureState::Began);
    assert_eq!(h.log(), strings(&["pan:began@32"]));
}

#[test]
fn can_re_base_its_translation_set_translation() {
    let h = create_harness();
    let pan = pan("pan");
    h.finger(1)
        .down(0., 0., 0., on(&[&pan]))
        .move_to(30., 0., 16.);
    pan.set_translation(Point::ZERO);
    h.finger(1).move_to(35., 2., 32.);
    assert_eq!(pan.translation(), Point::new(5., 2.));
}

#[test]
fn is_cancelled_by_the_system() {
    let h = create_harness();
    let pan = pan("pan");
    h.finger(1)
        .down(0., 0., 0., on(&[&pan]))
        .move_to(30., 0., 16.);
    h.finger(1).cancel(32.);
    assert_eq!(
        h.log(),
        strings(&["pan:began@16", "pan:cancelled@32", "pan:possible@32"])
    );
}

#[test]
fn claims_nothing_before_it_begins_at_10_pt_a_5_to_9_pt_drag_leaves_the_touch_to_the_browser_fr17()
{
    let h = create_harness();
    let pan = pan_with(
        "pan",
        PanOptions {
            direction: PanDirection::Up,
            ..PanOptions::default()
        },
    );
    let mut finger = h.finger(1).down(0., 100., 0., on(&[&pan]));
    for (y, time) in [(95., 16.), (93., 32.), (91., 48.)] {
        finger = finger.move_to(0., y, time);
        assert_eq!(pan.state(), GestureState::Possible);
        assert!(h.claims().is_empty());
    }
    let finger = finger.move_to(0., 90., 64.);
    assert_eq!(pan.state(), GestureState::Began);
    assert_eq!(h.claims(), vec![1]);
    finger.up(80.);
    let _ = RefCell::new(0);
}
