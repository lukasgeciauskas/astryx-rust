//! Upstream `__tests__/TapGestureRecognizer.test.ts`,
//! `LongPressGestureRecognizer.test.ts`, `PinchGestureRecognizer.test.ts` and
//! `SwipeGestureRecognizer.test.ts`: each recognizer's state machine on
//! synthetic touches with explicit timestamps.

use std::cell::{Cell, RefCell};
use std::f64::consts::PI;
use std::rc::Rc;

use super::harness::*;
use crate::gestures::*;
use crate::motion::uikit::gestures::{long_press as LONG_PRESS, tap as TAP};

fn tap(name: &str) -> TapGestureRecognizer {
    TapGestureRecognizer::new(TapOptions::named(name))
}

fn taps(name: &str, taps: u32, touches: usize) -> TapGestureRecognizer {
    TapGestureRecognizer::new(TapOptions {
        number_of_taps_required: taps,
        number_of_touches_required: touches,
        ..TapOptions::named(name)
    })
}

mod tap_recognizer {
    use super::*;

    #[test]
    fn recognizes_a_single_tap_at_the_lift() {
        let h = create_harness();
        let tap = tap("tap");
        let actions = Rc::new(RefCell::new(Vec::new()));
        let record = actions.clone();
        tap.add_listener(move |r| {
            let now = r.now();
            record
                .borrow_mut()
                .push(format!("{}@{}", r.state(), js(now)));
        });
        h.finger(1)
            .down(100., 100., 0., on(&[&tap]))
            .move_to(103., 101., 40.);
        assert_eq!(tap.state(), GestureState::Possible);
        h.finger(1).up_at(80., 103., 101.);
        assert_eq!(*actions.borrow(), strings(&["ended@80"]));
        assert_eq!(h.log(), strings(&["tap:ended@80", "tap:possible@80"]));
        assert_eq!(tap.location(), Point::new(103., 101.));
    }

    #[test]
    fn fails_once_a_finger_travels_past_the_allowable_movement() {
        let h = create_harness();
        let tap = tap("tap");
        let allowed = TAP::ALLOWABLE_MOVEMENT;
        h.finger(1)
            .down(0., 0., 0., on(&[&tap]))
            .move_to(allowed, 0., 16.);
        assert_eq!(tap.state(), GestureState::Possible);
        h.finger(1).move_to(allowed + 0.5, 0., 32.);
        assert_eq!(tap.state(), GestureState::Failed);
        h.finger(1).up(48.);
        assert_eq!(h.log(), strings(&["tap:failed@32", "tap:possible@48"]));
    }

    #[test]
    fn fails_when_more_fingers_land_than_required_and_taps_with_two() {
        let h = create_harness();
        let one = tap("one");
        let two = taps("two", 1, 2);
        h.finger(1).down(0., 0., 0., on(&[&one, &two]));
        h.finger(2).down(40., 0., 20., on(&[&one, &two]));
        assert_eq!(one.state(), GestureState::Failed);
        h.finger(1).up(60.);
        assert_eq!(two.state(), GestureState::Possible);
        h.finger(2).up(70.);
        assert_eq!(
            h.log(),
            strings(&[
                "one:failed@20",
                "two:ended@70",
                "one:possible@70",
                "two:possible@70"
            ])
        );
    }

    #[test]
    fn recognizes_a_double_tap_inside_the_interval_and_fails_outside_it() {
        let interval = TAP::MAXIMUM_INTERVAL_BETWEEN_TAPS;
        let h = create_harness();
        let double = taps("double", 2, 1);
        h.finger(1).down(0., 0., 0., on(&[&double]));
        h.finger(1).up(60.);
        assert_eq!(double.state(), GestureState::Possible);
        assert_eq!(double.completed_taps(), 1);
        h.finger(2)
            .down(4., 2., 60. + interval - 1., on(&[&double]));
        h.finger(2).up(60. + interval + 50.);
        let at = js(60. + interval + 50.);
        assert_eq!(
            h.log(),
            vec![
                format!("double:ended@{at}"),
                format!("double:possible@{at}")
            ]
        );

        let late = create_harness();
        let slow = taps("slow", 2, 1);
        late.finger(1).down(0., 0., 0., on(&[&slow]));
        late.finger(1).up(60.);
        late.finger(2)
            .down(0., 0., 60. + interval + 1., on(&[&slow]));
        // The interval timer fired at its own deadline, before the late touch.
        assert_eq!(late.log()[0], format!("slow:failed@{}", js(60. + interval)));
    }

    #[test]
    fn fails_a_second_tap_that_lands_too_far_from_the_first() {
        let h = create_harness();
        let double = taps("double", 2, 1);
        let separation = TAP::ALLOWABLE_SEPARATION;
        h.finger(1).down(0., 0., 0., on(&[&double]));
        h.finger(1).up(50.);
        h.finger(2).down(separation + 1., 0., 100., on(&[&double]));
        assert_eq!(double.state(), GestureState::Failed);
    }

    #[test]
    fn recognizes_a_two_finger_double_tap_placing_each_tap_at_its_fingers_centroid() {
        let separation = TAP::ALLOWABLE_SEPARATION;
        let make = || {
            let h = create_harness();
            let tap = taps("two", 2, 2);
            // First tap: fingers 80 px apart, the right one lifting last.
            h.finger(1).down(0., 0., 0., on(&[&tap]));
            h.finger(2).down(80., 0., 10., on(&[&tap]));
            h.finger(1).up(50.);
            h.finger(2).up(60.);
            assert_eq!(tap.completed_taps(), 1);
            (h, tap)
        };
        let (h, tap) = make();
        h.finger(3).down(2., 1., 150., on(&[&tap]));
        h.finger(4).down(82., 1., 160., on(&[&tap]));
        h.finger(3).up(200.);
        h.finger(4).up(210.);
        assert_eq!(h.log(), strings(&["two:ended@210", "two:possible@210"]));
        let (far, tap) = make();
        far.finger(3).down(separation + 1., 0., 150., on(&[&tap]));
        assert_eq!(tap.state(), GestureState::Possible);
        far.finger(4).down(separation + 81., 0., 160., on(&[&tap]));
        assert_eq!(tap.state(), GestureState::Failed);
    }

    #[test]
    fn fails_a_hold_past_a_finite_maximum_tap_duration() {
        let h = create_harness();
        let tap = TapGestureRecognizer::new(TapOptions {
            maximum_tap_duration: 400.,
            ..TapOptions::named("tap")
        });
        h.finger(1).down(0., 0., 0., on(&[&tap]));
        h.advance(399.);
        assert_eq!(tap.state(), GestureState::Possible);
        h.advance(400.);
        assert_eq!(tap.state(), GestureState::Failed);
    }

    #[test]
    fn recognizes_a_two_finger_double_tap_at_the_same_two_points() {
        let h = create_harness();
        let tap = taps("double2", 2, 2);
        for start in [0., 200.] {
            let id = start as u64;
            let left = h.finger(id + 1).down(100., 100., start, on(&[&tap]));
            let right = h.finger(id + 2).down(200., 100., start + 10., on(&[&tap]));
            left.up(start + 60.);
            right.up(start + 80.);
        }
        assert!(h.logged("double2:ended@280"));
    }
}

fn press(name: &str) -> LongPressGestureRecognizer {
    LongPressGestureRecognizer::new(LongPressOptions::named(name))
}

mod long_press_recognizer {
    use super::*;

    #[test]
    fn begins_at_exactly_touch_down_plus_500_ms_even_when_the_next_input_is_late() {
        let h = create_harness();
        let press = press("press");
        h.finger(1).down(50., 50., 1000., on(&[&press]));
        h.finger(1).move_to(80., 50., 1900.);
        assert_eq!(press.began_at(), Some(1500.));
        assert_eq!(
            h.log(),
            strings(&["press:began@1500", "press:changed@1900"])
        );
        h.finger(1).up(2000.);
        assert_eq!(
            h.log()[2..],
            strings(&["press:ended@2000", "press:possible@2000"])
        );
    }

    #[test]
    fn uses_uikit_defaults_and_allows_unlimited_movement_after_it_began() {
        let press = LongPressGestureRecognizer::new(LongPressOptions::default());
        assert_eq!(press.behavior().minimum_press_duration, 500.);
        assert_eq!(press.behavior().allowable_movement, 10.);
        let h = create_harness();
        h.finger(1).down(0., 0., 0., on(&[&press]));
        h.advance(500.);
        assert_eq!(press.state(), GestureState::Began);
        h.finger(1).move_to(300., 300., 600.);
        assert_eq!(press.state(), GestureState::Changed);
        assert_eq!(press.location(), Point::new(300., 300.));
    }

    #[test]
    fn measures_the_movement_tolerance_as_a_distance_not_per_axis() {
        let inside = create_harness();
        let a = press("a");
        inside
            .finger(1)
            .down(0., 0., 0., on(&[&a]))
            .move_to(7., 7., 100.);
        assert_eq!(a.state(), GestureState::Possible);
        let outside = create_harness();
        let b = press("b");
        outside
            .finger(1)
            .down(0., 0., 0., on(&[&b]))
            .move_to(7.2, 7.2, 100.);
        assert_eq!(b.state(), GestureState::Failed);
    }

    #[test]
    fn fails_on_a_lift_before_the_duration() {
        let h = create_harness();
        let press = press("press");
        h.finger(1).down(0., 0., 0., on(&[&press]));
        h.finger(1).up(499.);
        assert_eq!(
            h.log(),
            strings(&["press:failed@499", "press:possible@499"])
        );
        h.advance(600.);
        assert_eq!(h.log().len(), 2);
    }

    #[test]
    fn begins_on_the_touch_itself_with_a_zero_duration_and_claims_it() {
        let h = create_harness();
        let press = LongPressGestureRecognizer::new(LongPressOptions {
            minimum_press_duration: 0.,
            allowable_movement: f64::INFINITY,
            ..LongPressOptions::named("press")
        });
        h.finger(1).down(10., 10., 40., on(&[&press]));
        assert_eq!(press.state(), GestureState::Began);
        assert_eq!(press.began_at(), Some(40.));
        assert_eq!(h.log(), strings(&["press:began@40"]));
        assert_eq!(h.claims(), vec![1]);
        h.finger(1).move_to(10., 120., 80.);
        assert_eq!(press.location(), Point::new(10., 120.));
        h.finger(1).up(120.);
        assert_eq!(
            h.log()[1..],
            strings(&["press:changed@80", "press:ended@120", "press:possible@120"])
        );
    }

    #[test]
    fn supports_a_tap_before_the_press_number_of_taps_required() {
        let h = create_harness();
        let press = LongPressGestureRecognizer::new(LongPressOptions {
            number_of_taps_required: 1,
            ..LongPressOptions::named("press")
        });
        h.finger(1).down(0., 0., 0., on(&[&press]));
        h.finger(1).up(80.);
        h.finger(2).down(2., 0., 200., on(&[&press]));
        h.advance(700.);
        assert_eq!(press.state(), GestureState::Began);
    }

    #[test]
    fn counts_only_quick_taps_with_the_required_fingers_in_the_taps_phase() {
        let tap_limit = TAP::MAXIMUM_TAP_DURATION;
        let held = create_harness();
        let slow = LongPressGestureRecognizer::new(LongPressOptions {
            number_of_taps_required: 1,
            ..LongPressOptions::named("slow")
        });
        held.finger(1).down(0., 0., 0., on(&[&slow]));
        held.finger(1).up(tap_limit + 100.);
        assert!(held.logged(&format!("slow:failed@{}", js(tap_limit + 100.))));
        let fingers = create_harness();
        let two = LongPressGestureRecognizer::new(LongPressOptions {
            number_of_taps_required: 1,
            number_of_touches_required: 2,
            ..LongPressOptions::named("two")
        });
        fingers.finger(1).down(0., 0., 0., on(&[&two]));
        fingers.finger(1).up(80.);
        assert!(fingers.logged("two:failed@80"));
    }

    #[test]
    fn keeps_a_begun_press_through_a_second_finger_and_ends_it_at_the_first_fingers_lift_fr10() {
        let h = create_harness();
        let press = press("press");
        h.finger(1).down(0., 0., 0., on(&[&press]));
        h.advance(600.);
        assert_eq!(press.state(), GestureState::Began);
        h.finger(2).down(80., 0., 700., on(&[&press]));
        assert_eq!(press.state(), GestureState::Began);
        assert_eq!(press.number_of_touches(), 1);
        h.finger(1).up(800.);
        assert!(h.logged("press:ended@800"));
    }

    #[test]
    fn brings_the_lock_for_a_hold_none_for_a_touch_down_detector_and_none_when_told_null_fr41() {
        let lock = lock_selection();
        let is_lock = |r: &LongPressGestureRecognizer| {
            r.selection_lock().is_some_and(|l| Rc::ptr_eq(&l, &lock))
        };
        assert!(is_lock(&LongPressGestureRecognizer::new(
            LongPressOptions::default()
        )));
        assert!(is_lock(&LongPressGestureRecognizer::new(
            LongPressOptions {
                minimum_press_duration: 300.,
                ..LongPressOptions::default()
            }
        )));
        assert!(
            LongPressGestureRecognizer::new(LongPressOptions {
                minimum_press_duration: 0.,
                ..LongPressOptions::default()
            })
            .selection_lock()
            .is_none()
        );
        let mut none = LongPressOptions::default();
        none.base.selection_lock = Some(None);
        assert!(
            LongPressGestureRecognizer::new(none)
                .selection_lock()
                .is_none()
        );
        let own: SelectionLock =
            Rc::new(|_: &[ViewKey], _: Option<ViewKey>| Box::new(|| {}) as Box<dyn FnOnce()>);
        let mut options = LongPressOptions {
            minimum_press_duration: 0.,
            ..LongPressOptions::default()
        };
        options.base.selection_lock = Some(Some(own.clone()));
        let with_own = LongPressGestureRecognizer::new(options);
        assert!(
            with_own
                .selection_lock()
                .is_some_and(|l| Rc::ptr_eq(&l, &own))
        );
        let _ = LONG_PRESS::MINIMUM_PRESS_DURATION;
    }
}

fn pinch(name: &str) -> PinchGestureRecognizer {
    PinchGestureRecognizer::new(PinchOptions::named(name))
}

mod pinch_recognizer {
    use super::*;

    #[test]
    fn begins_after_the_hysteresis_and_reports_scale_and_velocity() {
        let h = create_harness();
        let pinch = pinch("pinch");
        h.finger(1).down(100., 100., 0., on(&[&pinch]));
        h.finger(2).down(200., 100., 0., on(&[&pinch]));
        h.finger(2).move_to(205., 100., 16.);
        assert_eq!(pinch.state(), GestureState::Possible);
        h.finger(2).move_to(210., 100., 32.);
        assert_eq!(pinch.state(), GestureState::Began);
        assert!((pinch.scale() - 110. / 108.).abs() < 1e-5);
        let mut t = 48.;
        while t <= 160. {
            h.finger(2).move_to(210. + (t - 32.) * 0.1, 100., t);
            t += 16.;
        }
        assert!((pinch.velocity() - 100. / 108.).abs() < 1e-5);
        h.finger(1).up(170.);
        assert!(h.logged("pinch:ended@170"));
    }

    #[test]
    fn ignores_a_third_finger_and_can_be_re_based() {
        let h = create_harness();
        let pinch = pinch("pinch");
        h.finger(1).down(0., 0., 0., on(&[&pinch]));
        h.finger(2).down(100., 0., 0., on(&[&pinch]));
        h.finger(3).down(50., 50., 5., on(&[&pinch]));
        assert_eq!(pinch.number_of_touches(), 2);
        h.finger(2).move_to(150., 0., 16.);
        pinch.set_scale(1.);
        h.finger(2).move_to(300., 0., 32.);
        assert!((pinch.scale() - 2.).abs() < 1e-5);
    }

    #[test]
    fn keeps_its_release_velocity_when_a_listener_re_bases_on_every_change() {
        let h = create_harness();
        let pinch = pinch("pinch");
        pinch.add_listener(|r| {
            if r.state() == GestureState::Changed {
                r.set_scale(1.);
            }
        });
        let released = Rc::new(Cell::new(f64::NAN));
        let record = released.clone();
        pinch.add_listener(move |r| {
            if r.state() == GestureState::Ended {
                record.set(r.velocity());
            }
        });
        h.finger(1).down(100., 100., 0., on(&[&pinch]));
        h.finger(2).down(200., 100., 0., on(&[&pinch]));
        let mut t = 0.;
        for _ in 0..10 {
            t += 16.;
            h.finger(2).move_to(200. + t * 0.5, 100., t);
        }
        h.finger(1).up(t + 8.);
        assert!(released.get() > 1., "{}", released.get());
    }

    #[test]
    fn lets_the_velocity_settle_when_the_span_holds_while_the_fingers_move() {
        let h = create_harness();
        let pinch = pinch("pinch");
        h.finger(1).down(100., 100., 0., on(&[&pinch]));
        h.finger(2).down(200., 100., 0., on(&[&pinch]));
        let mut time = 0.;
        let mut x = 210.;
        while x <= 260. {
            time += 16.;
            h.finger(2).move_to(x, 100., time);
            x += 10.;
        }
        let spreading = pinch.velocity();
        assert!(spreading > 0.);
        for (dx, dy) in [
            (128., 96.),
            (96., 128.),
            (0., 160.),
            (-96., 128.),
            (-128., 96.),
        ] {
            time += 16.;
            h.finger(2).move_to(100. + dx, 100. + dy, time);
        }
        assert!(pinch.velocity().abs() < spreading * 0.25);
    }
}

fn swipe(name: &str, direction: &[SwipeDirection]) -> SwipeGestureRecognizer {
    SwipeGestureRecognizer::new(SwipeOptions {
        direction: direction.to_vec(),
        ..SwipeOptions::named(name)
    })
}

mod swipe_recognizer {
    use super::*;
    use SwipeDirection::*;

    #[test]
    fn is_recognized_during_the_motion_once_the_distance_is_covered() {
        let h = create_harness();
        let swipe = swipe("swipe", &[Right]);
        let finger = h.finger(1).down(0., 0., 0., on(&[&swipe]));
        let finger = finger.move_to(30., 2., 30.);
        assert_eq!(swipe.state(), GestureState::Possible);
        finger.move_to(50., 4., 50.);
        assert_eq!(swipe.recognized_direction(), Some(Right));
        assert_eq!(h.log()[0], "swipe:ended@50");
    }

    #[test]
    fn fails_when_too_slow_off_axis_or_the_wrong_way() {
        let slow = create_harness();
        let a = swipe("a", &[Right]);
        slow.finger(1)
            .down(0., 0., 0., on(&[&a]))
            .move_to(30., 0., 400.);
        slow.advance(500.);
        assert_eq!(a.state(), GestureState::Failed);

        let diagonal = create_harness();
        let b = swipe("b", &[Right]);
        diagonal
            .finger(1)
            .down(0., 0., 0., on(&[&b]))
            .move_to(45., 55., 60.);
        assert_eq!(b.state(), GestureState::Failed);

        let wrong = create_harness();
        let c = swipe("c", &[Left]);
        wrong
            .finger(1)
            .down(100., 0., 0., on(&[&c]))
            .move_to(160., 0., 60.);
        assert_eq!(c.state(), GestureState::Failed);
    }

    #[test]
    fn takes_several_directions_and_fingers() {
        let h = create_harness();
        let swipe = SwipeGestureRecognizer::new(SwipeOptions {
            direction: vec![Up, Down],
            number_of_touches_required: 2,
            ..SwipeOptions::named("swipe")
        });
        h.finger(1).down(0., 200., 0., on(&[&swipe]));
        h.finger(2).down(40., 200., 5., on(&[&swipe]));
        h.finger(1).move_to(0., 140., 50.);
        h.finger(2).move_to(40., 140., 50.);
        assert_eq!(swipe.recognized_direction(), Some(Up));
    }

    #[test]
    fn gives_a_20_degree_drag_the_same_answer_in_small_and_large_steps() {
        for step in [4., 9.] {
            let h = create_harness();
            let swipe = swipe("swipe", &[Right]);
            let mut finger = h.finger(1).down(0., 0., 0., on(&[&swipe]));
            let angle = 20. * PI / 180.;
            let (mut travelled, mut time) = (0., 0.);
            while swipe.state() == GestureState::Possible && travelled < 80. {
                travelled += step;
                time += 8.;
                finger = finger.move_to(travelled * angle.cos(), travelled * angle.sin(), time);
            }
            assert_eq!(swipe.state(), GestureState::Ended);
            finger.up(time + 8.);
        }
    }

    #[test]
    fn gives_the_same_verdict_for_the_same_path_however_its_events_are_spaced() {
        let sparse = create_harness();
        let a = swipe("a", &[Right]);
        sparse
            .finger(1)
            .down(0., 0., 0., on(&[&a]))
            .move_to(55., 40., 40.);
        assert_eq!(a.state(), GestureState::Ended);

        let dense = create_harness();
        let b = swipe("b", &[Right]);
        let mut finger = dense.finger(1).down(0., 0., 0., on(&[&b]));
        for step in 1..=10 {
            let s = f64::from(step);
            finger = finger.move_to(5.5 * s, 4. * s, 4. * s);
        }
        assert_eq!(b.state(), GestureState::Ended);
        assert_eq!(b.recognized_direction(), Some(Right));
    }

    #[test]
    fn recognizes_a_40_degree_swipe_at_16_px_per_event_as_at_small_steps() {
        let angle = 40. * PI / 180.;
        for step in [2., 16.] {
            let h = create_harness();
            let swipe = swipe("swipe", &[Right]);
            let mut finger = h.finger(1).down(0., 0., 0., on(&[&swipe]));
            let (mut d, mut t) = (step, 4.);
            while d <= 96. {
                finger = finger.move_to(d * angle.cos(), d * angle.sin(), t);
                if swipe.state() != GestureState::Possible {
                    break;
                }
                d += step;
                t += 4.;
            }
            assert_eq!(swipe.state(), GestureState::Ended);
            assert_eq!(swipe.recognized_direction(), Some(Right));
        }
    }
}
