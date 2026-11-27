//! Upstream `__tests__/arenaContinuous.test.ts`, `arenaPinchSwipeEdge.test.ts`,
//! `arenaTapLongPress.test.ts`, `arenaTimersAndClaims.test.ts`,
//! `watcherRecognizer.test.ts`, `recognizerInternals.test.ts` and
//! `recognizerKind.test.ts`: the arena with the real recognizers.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use super::harness::*;
use crate::gestures::touch_action::derive_touch_action;
use crate::gestures::*;
use crate::motion::uikit::gestures::{pinch as PINCH, swipe as SWIPE, tap as TAP};

fn pan(name: &str) -> PanGestureRecognizer {
    PanGestureRecognizer::new(PanOptions::named(name))
}

fn pan_dir(name: &str, direction: PanDirection) -> PanGestureRecognizer {
    PanGestureRecognizer::new(PanOptions {
        direction,
        ..PanOptions::named(name)
    })
}

fn sheet(name: &str) -> PanGestureRecognizer {
    PanGestureRecognizer::new(PanOptions {
        direction: PanDirection::Down,
        scroll_edge: true,
        ..PanOptions::named(name)
    })
}

fn press(name: &str) -> LongPressGestureRecognizer {
    LongPressGestureRecognizer::new(LongPressOptions::named(name))
}

fn tap(name: &str) -> TapGestureRecognizer {
    TapGestureRecognizer::new(TapOptions::named(name))
}

fn double(name: &str) -> TapGestureRecognizer {
    TapGestureRecognizer::new(TapOptions {
        number_of_taps_required: 2,
        ..TapOptions::named(name)
    })
}

mod arena_continuous {
    use super::*;

    #[test]
    fn pan_vs_native_scroll_the_browser_taking_the_touch_fails_a_possible_pan() {
        let h = create_harness();
        let pan = pan_dir("pan", PanDirection::Vertical);
        h.finger(1)
            .down(0., 100., 0., on(&[&pan]))
            .move_to(0., 105., 16.);
        h.finger(1).taken_by_browser(24.);
        assert_eq!(h.log(), strings(&["pan:failed@24", "pan:possible@24"]));
        assert!(h.claims().is_empty());
    }

    #[test]
    fn pan_at_a_scroll_edge_begins_and_claims_when_the_content_cannot_scroll_that_way() {
        let at_top = create_harness();
        let sheet = sheet("sheet");
        let probe = scroll_probe(|_dx, dy| dy < 0., true);
        at_top
            .finger(1)
            .down(0., 100., 0., on(&[&sheet]).scroll(probe));
        at_top.finger(1).drag(0., 30., 50., 5);
        assert_eq!(at_top.log()[0], "sheet:began@20");
        assert_eq!(at_top.claims(), vec![1]);

        let mid = create_harness();
        let scrolled = self::sheet("sheet");
        let can_scroll_up = scroll_probe(|_, _| true, true);
        mid.finger(1)
            .down(0., 100., 0., on(&[&scrolled]).scroll(can_scroll_up));
        mid.finger(1).drag(0., 30., 50., 5);
        assert_eq!(mid.log(), strings(&["sheet:failed@20"]));
    }

    #[test]
    fn a_drag_that_began_as_a_scroll_stays_the_browsers_past_the_contents_edge_fr14() {
        let h = create_harness();
        let sheet = sheet("sheet");
        let scroll_top = Rc::new(Cell::new(30.));
        let top = scroll_top.clone();
        let finger = h.finger(1).down(
            0.,
            100.,
            0.,
            on(&[&sheet]).scroll(scroll_probe(move |_, _| top.get() > 0., true)),
        );
        let finger = finger.move_to(0., 120., 16.);
        finger.taken_by_browser(20.);
        scroll_top.set(0.);
        let finger = finger.move_to(0., 160., 32.).move_to(0., 200., 48.);
        finger.up(64.);
        assert_eq!(h.log(), strings(&["sheet:failed@16", "sheet:possible@20"]));
    }

    #[test]
    fn long_press_vs_pan_holding_still_wins_for_the_long_press() {
        let h = create_harness();
        let press = press("press");
        let pan = pan("pan");
        h.finger(1).down(0., 0., 0., on(&[&press, &pan]));
        h.advance(500.);
        assert_eq!(h.log(), strings(&["press:began@500", "pan:failed@500"]));
        h.finger(1).move_to(80., 0., 600.);
        assert_eq!(press.state(), GestureState::Changed);
        assert_eq!(pan.state(), GestureState::Failed);
        assert_eq!(h.claims(), vec![1]);
    }

    #[test]
    fn long_press_vs_pan_moving_first_wins_for_the_pan() {
        let h = create_harness();
        let press = press("press");
        let pan = pan("pan");
        h.finger(1).down(0., 0., 0., on(&[&press, &pan]));
        h.finger(1).move_to(5., 0., 100.).move_to(12., 0., 120.);
        assert_eq!(h.log(), strings(&["press:failed@120", "pan:began@120"]));
        h.advance(600.);
        assert_eq!(press.state(), GestureState::Failed);
    }

    #[test]
    fn nested_pans_that_both_qualify_on_the_same_move_the_innermost_wins() {
        let h = create_harness();
        let inner = pan("inner");
        let outer = pan("outer");
        h.finger(1)
            .down(0., 0., 0., on(&[&inner, &outer]))
            .move_to(20., 0., 16.);
        assert_eq!(h.log(), strings(&["inner:began@16", "outer:failed@16"]));
    }

    #[test]
    fn honors_should_begin_and_should_receive_touch() {
        let h = create_harness();
        let mut refusing = PanOptions::named("refused");
        refusing.base.should_begin = Some(Rc::new(|_: &AnyRecognizer| false));
        let refused = PanGestureRecognizer::new(refusing);
        let mut blind_options = TapOptions::named("blind");
        blind_options.base.should_receive_touch =
            Some(Rc::new(|_: &AnyRecognizer, _: &GestureTouch| false));
        let blind = TapGestureRecognizer::new(blind_options);
        h.finger(1)
            .down(0., 0., 0., on(&[&refused, &blind]))
            .move_to(30., 0., 16.);
        assert_eq!(h.log(), strings(&["refused:failed@16"]));
        assert_eq!(blind.number_of_touches(), 0);
    }

    #[test]
    fn two_fingers_on_two_unrelated_views_drive_two_pans_at_once() {
        let h = create_harness();
        let left = pan("left");
        let right = pan("right");
        h.finger(1).down(50., 300., 0., on(&[&left]));
        h.finger(2).down(300., 300., 0., on(&[&right]));
        h.finger(1).move_to(50., 340., 16.);
        h.finger(2).move_to(300., 260., 16.);
        assert_eq!(left.state(), GestureState::Began);
        assert_eq!(right.state(), GestureState::Began);
    }

    #[test]
    fn delivers_an_unclaimed_touch_to_the_consumer_and_allows_its_click() {
        let h = create_harness();
        let consumer = RecordingConsumer::new();
        let pan = pan("pan");
        let finger = h
            .finger(1)
            .down(0., 0., 0., on(&[&pan]).consumer(consumer.clone()));
        let finger = finger.move_to(3., 0., 16.);
        assert_eq!(finger.up(60.), Some(ClickDisposition::Allow));
        assert_eq!(
            consumer.events(),
            strings(&["began:1", "moved:1", "ended:1"])
        );
    }

    #[test]
    fn cancels_the_view_and_swallows_the_click_when_a_recognizer_takes_the_touch() {
        let h = create_harness();
        let consumer = RecordingConsumer::new();
        let pan = pan("pan");
        let finger = h
            .finger(1)
            .down(0., 0., 0., on(&[&pan]).consumer(consumer.clone()));
        let finger = finger.move_to(30., 0., 16.).move_to(40., 0., 32.);
        assert_eq!(consumer.events(), strings(&["began:1", "cancelled:1"]));
        assert_eq!(finger.up(60.), Some(ClickDisposition::Suppress));
    }

    #[test]
    fn keeps_the_view_and_the_click_with_cancels_touches_in_view_off() {
        let h = create_harness();
        let consumer = RecordingConsumer::new();
        let mut options = PanOptions::named("pan");
        options.base.cancels_touches_in_view = false;
        let pan = PanGestureRecognizer::new(options);
        let finger = h
            .finger(1)
            .down(0., 0., 0., on(&[&pan]).consumer(consumer.clone()));
        let finger = finger.move_to(30., 0., 16.);
        assert_eq!(finger.up(60.), Some(ClickDisposition::Allow));
        assert_eq!(
            consumer.events(),
            strings(&["began:1", "moved:1", "ended:1"])
        );
    }

    #[test]
    fn stop_cancels_an_active_recognizer_from_code() {
        let h = create_harness();
        let pan = pan("pan");
        h.finger(1)
            .down(0., 0., 0., on(&[&pan]))
            .move_to(30., 0., 16.);
        pan.cancel();
        assert_eq!(pan.state(), GestureState::Cancelled);
        h.finger(1).move_to(60., 0., 32.);
        assert_eq!(pan.state(), GestureState::Cancelled);
        h.finger(1).up(48.);
        assert_eq!(pan.state(), GestureState::Possible);
    }

    #[test]
    fn never_moves_the_event_clock_backwards() {
        let h = create_harness();
        let pan = pan("pan");
        h.finger(1).down(0., 0., 100., on(&[&pan]));
        h.finger(1).move_to(30., 0., 90.);
        assert_eq!(h.log(), strings(&["pan:began@100"]));
    }
}

/// A recognizer that stays possible, even past its finger's lift, until the
/// test fails it.
fn held(name: &str) -> AnyRecognizer {
    let held = scripted(
        name,
        Script {
            ended: step(|_, _| {}),
            ..Script::default()
        },
    );
    held.set_waiting(true);
    held
}

mod arena_pinch_swipe_edge {
    use super::*;

    fn pinch(name: &str) -> PinchGestureRecognizer {
        PinchGestureRecognizer::new(PinchOptions::named(name))
    }

    fn swipe(name: &str) -> SwipeGestureRecognizer {
        SwipeGestureRecognizer::new(SwipeOptions::named(name))
    }

    #[test]
    fn pinch_and_pan_recognize_simultaneously_when_allowed() {
        let h = create_harness();
        let pinch = pinch("pinch");
        let pan = pan("pan");
        pinch.recognize_simultaneously_with(&[&pan]);
        h.finger(1).down(100., 100., 0., on(&[&pinch, &pan]));
        h.finger(2).down(200., 100., 0., on(&[&pinch, &pan]));
        h.finger(1).move_to(80., 120., 16.);
        h.finger(2).move_to(240., 120., 16.);
        assert_eq!(h.log()[..2], strings(&["pinch:began@16", "pan:began@16"]));
        assert!((pinch.scale() - 160. / 108.).abs() < 1e-5);
        assert_eq!(pan.translation(), Point::new(20., 10.));
    }

    #[test]
    fn pinch_vs_pan_without_permission_the_first_to_recognize_wins() {
        let h = create_harness();
        let pinch = pinch("pinch");
        let pan = pan("pan");
        h.finger(1).down(100., 100., 0., on(&[&pinch, &pan]));
        h.finger(2).down(200., 100., 0., on(&[&pinch, &pan]));
        h.finger(1).move_to(80., 120., 16.);
        h.finger(2).move_to(240., 120., 16.);
        assert_eq!(h.log()[..2], strings(&["pinch:began@16", "pan:failed@16"]));
    }

    #[test]
    fn edge_pan_vs_inner_pan_in_the_band_the_edge_pan_wins_elsewhere_the_inner_pan_runs_undelayed()
    {
        let band = create_harness();
        let edge = ScreenEdgePanGestureRecognizer::new(ScreenEdgePanOptions::named("edge"));
        let inner = pan_dir("inner", PanDirection::Horizontal);
        inner.require_to_fail(&edge);
        band.env.view.set(Some(ViewRect {
            left: 0.,
            top: 0.,
            width: edge.behavior().edge_region,
            height: 844.,
        }));
        band.finger(1).down(8., 300., 0., on(&[&inner, &edge]));
        band.finger(1).drag(60., 2., 60., 6);
        assert_eq!(
            band.log()[..2],
            strings(&["edge:began@20", "inner:failed@20"])
        );

        let middle = create_harness();
        let edge2 = ScreenEdgePanGestureRecognizer::new(ScreenEdgePanOptions::named("edge"));
        let inner2 = pan_dir("inner", PanDirection::Horizontal);
        inner2.require_to_fail(&edge2);
        middle
            .finger(1)
            .down(150., 300., 0., on(&[&inner2, &edge2]));
        assert_eq!(middle.log(), strings(&["edge:failed@0"]));
        middle.finger(1).drag(60., 2., 60., 6);
        assert_eq!(middle.log()[1], "inner:began@10");
    }

    #[test]
    fn swipe_vs_pan_the_pan_wins_by_default_the_swipe_when_the_pan_requires_it_to_fail() {
        let plain = create_harness();
        let swipe1 = swipe("swipe");
        let pan1 = pan("pan");
        plain
            .finger(1)
            .down(0., 0., 0., on(&[&swipe1, &pan1]))
            .drag(80., 0., 60., 6);
        assert_eq!(
            plain.log()[..2],
            strings(&["pan:began@10", "swipe:failed@10"])
        );

        let ordered = create_harness();
        let swipe2 = swipe("swipe");
        let pan2 = pan("pan");
        pan2.require_to_fail(&swipe2);
        ordered
            .finger(1)
            .down(0., 0., 0., on(&[&swipe2, &pan2]))
            .drag(80., 0., 60., 8);
        assert_eq!(
            ordered.log()[..2],
            strings(&["swipe:ended@37.5", "pan:failed@37.5"])
        );

        let slow = create_harness();
        let swipe3 = swipe("swipe");
        let pan3 = pan("pan");
        pan3.require_to_fail(&swipe3);
        slow.finger(1)
            .down(0., 0., 0., on(&[&swipe3, &pan3]))
            .drag(30., 0., 300., 3);
        assert_eq!(pan3.state(), GestureState::Possible);
        slow.advance(500.);
        assert_eq!(
            slow.log()[..2],
            strings(&["swipe:failed@500", "pan:began@500"])
        );
    }

    #[test]
    fn a_swipe_holds_its_recognition_past_its_lift_and_its_deadline_fr15() {
        let h = create_harness();
        let held = held("held");
        let swipe = swipe("swipe");
        swipe.require_to_fail(&held);
        h.finger(1)
            .down(0., 0., 0., on(&[&swipe, &held]))
            .drag(80., 0., 60., 8)
            .up(70.);
        h.advance(SWIPE::MAXIMUM_DURATION + 100.);
        assert_eq!(swipe.state(), GestureState::Possible);
        assert!(!h.log().iter().any(|e| e.starts_with("swipe:failed")));
        held.fail();
        assert!(h.logged(&format!(
            "swipe:ended@{}",
            js(SWIPE::MAXIMUM_DURATION + 100.)
        )));
        assert_eq!(swipe.recognized_direction(), Some(SwipeDirection::Right));
    }

    #[test]
    fn a_pinch_keeps_its_start_once_it_passes_the_hysteresis_fr15() {
        let h = create_harness();
        let held = held("held");
        let pinch = pinch("pinch");
        pinch.require_to_fail(&held);
        h.finger(1).down(100., 100., 0., on(&[&pinch, &held]));
        h.finger(2).down(200., 100., 0., on(&[&pinch, &held]));
        h.finger(2).move_to(220., 100., 16.);
        h.finger(2).move_to(240., 100., 32.);
        h.finger(2).move_to(260., 100., 48.);
        assert_eq!(pinch.state(), GestureState::Possible);
        held.fail();
        assert!(h.logged("pinch:began@48"));
        assert!((pinch.scale() - 160. / (100. + PINCH::HYSTERESIS)).abs() < 1e-5);
    }

    #[test]
    fn holds_the_began_with_delays_touches_began_until_the_recognizer_fails() {
        let h = create_harness();
        let consumer = RecordingConsumer::new();
        let mut options = SwipeOptions::named("swipe");
        options.base.delays_touches_began = true;
        let swipe = SwipeGestureRecognizer::new(options);
        h.finger(1)
            .down(0., 0., 0., on(&[&swipe]).consumer(consumer.clone()));
        assert!(consumer.events().is_empty());
        h.advance(SWIPE::MAXIMUM_DURATION);
        assert_eq!(consumer.events(), strings(&["began:1"]));
    }
}

mod arena_tap_long_press {
    use super::*;

    const INTERVAL: f64 = TAP::MAXIMUM_INTERVAL_BETWEEN_TAPS;

    #[test]
    fn tap_vs_double_tap_single_requires_double_to_fail_the_single_waits_out_the_interval() {
        let h = create_harness();
        let single = tap("single");
        let double = double("double");
        single.require_to_fail(&double);
        h.finger(1).down(0., 0., 0., on(&[&single, &double]));
        h.finger(1).up(80.);
        assert_eq!(single.state(), GestureState::Possible);
        assert_eq!(single.pending(), Some(PendingRecognition::Ended));
        h.advance(80. + INTERVAL);
        let at = js(80. + INTERVAL);
        assert_eq!(
            h.log(),
            vec![
                format!("double:failed@{at}"),
                format!("single:ended@{at}"),
                format!("single:possible@{at}"),
                format!("double:possible@{at}"),
            ]
        );
    }

    #[test]
    fn a_requirement_stopped_inside_defer_resolution_leaves_the_recognition_waiting_on_it_to_a_pass_a_microtask_later()
     {
        let h = create_harness();
        let single = tap("single");
        let double = double("double");
        single.require_to_fail(&double);
        h.finger(1).down(0., 0., 0., on(&[&single, &double]));
        h.finger(1).up(80.);
        h.arena.defer_resolution(|| double.set_enabled(false));
        assert_eq!(h.log(), strings(&["double:failed@80"]));
        assert_eq!(single.pending(), Some(PendingRecognition::Ended));
        h.clock.run_microtasks();
        assert_eq!(
            h.log(),
            strings(&[
                "double:failed@80",
                "single:ended@80",
                "single:possible@80",
                "double:possible@80"
            ])
        );
    }

    #[test]
    fn tap_vs_double_tap_single_requires_double_to_fail_a_double_tap_fails_the_single() {
        let h = create_harness();
        let single = tap("single");
        let double = double("double");
        single.require_to_fail(&double);
        h.finger(1).down(0., 0., 0., on(&[&single, &double]));
        h.finger(1).up(80.);
        h.finger(2).down(2., 2., 200., on(&[&single, &double]));
        h.finger(2).up(260.);
        assert_eq!(
            h.log(),
            strings(&[
                "double:ended@260",
                "single:failed@260",
                "single:possible@260",
                "double:possible@260"
            ])
        );
    }

    #[test]
    fn tap_vs_double_tap_with_no_requirement_the_single_wins_at_once_and_fails_the_double() {
        let h = create_harness();
        let single = tap("single");
        let double = double("double");
        h.finger(1).down(0., 0., 0., on(&[&single, &double]));
        h.finger(1).up(80.);
        assert_eq!(
            h.log()[..2],
            strings(&["single:ended@80", "double:failed@80"])
        );
    }

    #[test]
    fn a_recognizer_that_is_not_involved_holds_nothing_up() {
        let h = create_harness();
        let idle = tap("idle");
        let tap = tap("tap");
        tap.require_to_fail(&idle);
        h.finger(1).down(0., 0., 0., on(&[&tap]));
        h.finger(1).up(50.);
        assert_eq!(h.log()[0], "tap:ended@50");
    }

    #[test]
    fn honors_delegate_failure_requirements() {
        let h = create_harness();
        let double = double("double");
        let weak = double.downgrade();
        let mut options = TapOptions::named("single");
        options.base.should_require_failure_of =
            Some(Rc::new(move |_: &AnyRecognizer, other: &AnyRecognizer| {
                weak.upgrade().is_some_and(|d| d == *other)
            }));
        let single = TapGestureRecognizer::new(options);
        h.finger(1).down(0., 0., 0., on(&[&single, &double]));
        h.finger(1).up(50.);
        assert_eq!(single.pending(), Some(PendingRecognition::Ended));
    }

    #[test]
    fn delivers_an_unclaimed_touch_to_the_consumer_and_allows_its_click() {
        let h = create_harness();
        let consumer = RecordingConsumer::new();
        let press = press("press");
        let finger = h
            .finger(1)
            .down(0., 0., 0., on(&[&press]).consumer(consumer.clone()));
        let finger = finger.move_to(3., 0., 16.);
        assert_eq!(finger.up(60.), Some(ClickDisposition::Allow));
        assert_eq!(
            consumer.events(),
            strings(&["began:1", "moved:1", "ended:1"])
        );
    }

    #[test]
    fn cancels_the_view_and_swallows_the_click_when_a_recognizer_takes_the_touch() {
        let h = create_harness();
        let consumer = RecordingConsumer::new();
        let press = press("press");
        let finger = h
            .finger(1)
            .down(0., 0., 0., on(&[&press]).consumer(consumer.clone()));
        h.advance(500.);
        assert_eq!(consumer.events(), strings(&["began:1", "cancelled:1"]));
        assert_eq!(h.claims(), vec![1]);
        assert_eq!(finger.up(600.), Some(ClickDisposition::Suppress));
    }

    #[test]
    fn keeps_the_view_and_the_click_with_cancels_touches_in_view_off() {
        let h = create_harness();
        let consumer = RecordingConsumer::new();
        let mut options = LongPressOptions::named("press");
        options.base.cancels_touches_in_view = false;
        let press = LongPressGestureRecognizer::new(options);
        let finger = h
            .finger(1)
            .down(0., 0., 0., on(&[&press]).consumer(consumer.clone()));
        h.advance(500.);
        assert_eq!(finger.up(600.), Some(ClickDisposition::Allow));
        assert_eq!(consumer.events(), strings(&["began:1", "ended:1"]));
    }

    #[test]
    fn defers_the_click_of_a_tap_a_double_tap_may_still_claim_then_replays_it() {
        let h = create_harness();
        let consumer = RecordingConsumer::new();
        let double = double("double");
        let finger = h
            .finger(1)
            .down(0., 0., 0., on(&[&double]).consumer(consumer.clone()));
        assert_eq!(finger.up(70.), Some(ClickDisposition::Defer));
        assert_eq!(consumer.events(), strings(&["began:1"]));
        h.advance(70. + INTERVAL);
        assert_eq!(consumer.events(), strings(&["began:1", "ended:1"]));
        assert_eq!(h.deferred(), vec![(1, true, 70. + INTERVAL)]);
    }

    #[test]
    fn drops_the_deferred_click_when_the_double_tap_is_recognized() {
        let h = create_harness();
        let consumer = RecordingConsumer::new();
        let double = double("double");
        h.finger(1)
            .down(0., 0., 0., on(&[&double]).consumer(consumer.clone()));
        h.finger(1).up(70.);
        h.finger(2)
            .down(1., 1., 150., on(&[&double]).consumer(consumer.clone()));
        assert_eq!(h.finger(2).up(210.), Some(ClickDisposition::Suppress));
        assert_eq!(h.deferred(), vec![(1, false, 210.)]);
        assert_eq!(
            consumer.events(),
            strings(&["began:1", "began:2", "cancelled:1", "cancelled:2"])
        );
    }

    #[test]
    fn cancels_the_view_when_the_browser_takes_the_touch() {
        let h = create_harness();
        let consumer = RecordingConsumer::new();
        let tap = tap("tap");
        let finger = h
            .finger(1)
            .down(0., 0., 0., on(&[&tap]).consumer(consumer.clone()));
        finger.taken_by_browser(30.);
        assert_eq!(consumer.events(), strings(&["began:1", "cancelled:1"]));
        assert_eq!(h.log(), strings(&["tap:failed@30", "tap:possible@30"]));
    }

    #[test]
    fn a_system_cancel_cancels_an_active_gesture_and_fails_a_possible_one() {
        let h = create_harness();
        let press = press("press");
        let tap = tap("tap");
        h.finger(1).down(0., 0., 0., on(&[&press]));
        h.advance(500.);
        h.finger(2).down(100., 0., 600., on(&[&tap]));
        h.arena.cancelled(&[1, 2], 650.);
        assert_eq!(
            h.log(),
            strings(&[
                "press:began@500",
                "press:cancelled@650",
                "tap:failed@650",
                "press:possible@650",
                "tap:possible@650",
            ])
        );
    }

    #[test]
    fn a_long_press_cancelled_before_its_duration_never_begins() {
        let h = create_harness();
        let press = press("press");
        h.finger(1).down(0., 0., 0., on(&[&press]));
        h.finger(1).cancel(200.);
        h.advance(1000.);
        assert_eq!(
            h.log(),
            strings(&["press:failed@200", "press:possible@200"])
        );
    }
}

mod arena_timers_and_claims {
    use super::*;

    /// A clock whose timers run one task at a time, as an event loop does.
    #[derive(Default)]
    struct TaskClock {
        time: Cell<f64>,
        next: Cell<u64>,
        #[allow(clippy::type_complexity)]
        tasks: RefCell<Vec<(u64, f64, Box<dyn FnOnce()>)>>,
    }

    impl GestureClock for TaskClock {
        fn now(&self) -> f64 {
            self.time.get()
        }
        fn set_timeout(&self, run: Box<dyn FnOnce()>, delay: f64) -> TimerHandle {
            let id = self.next.get() + 1;
            self.next.set(id);
            self.tasks
                .borrow_mut()
                .push((id, self.time.get() + delay, run));
            id
        }
        fn clear_timeout(&self, id: TimerHandle) {
            self.tasks.borrow_mut().retain(|(t, _, _)| *t != id);
        }
    }

    impl TaskClock {
        fn run_task(&self, time: f64) {
            self.time.set(time);
            let due = {
                let tasks = self.tasks.borrow();
                let mut best: Option<(usize, f64)> = None;
                for (index, (_, at, _)) in tasks.iter().enumerate() {
                    if *at <= time && best.is_none_or(|(_, b)| *at < b) {
                        best = Some((index, *at));
                    }
                }
                best
            };
            if let Some((index, _)) = due {
                let (_, _, run) = self.tasks.borrow_mut().remove(index);
                run();
            }
        }
    }

    fn press_arena() -> (Rc<TaskClock>, GestureArena, Rc<RefCell<Vec<String>>>) {
        let clock = Rc::new(TaskClock::default());
        let log = Rc::new(RefCell::new(Vec::new()));
        let arena = GestureArena::new(ArenaOptions {
            clock: Some(clock.clone()),
            ..ArenaOptions::default()
        });
        let weak = arena.downgrade();
        let entries = log.clone();
        arena.callbacks().on_transition =
            Some(Rc::new(move |r: &AnyRecognizer, _, to: GestureState| {
                let now = weak.upgrade().map_or(0., |a| a.now());
                entries
                    .borrow_mut()
                    .push(format!("{}:{}@{}", r.name(), to, js(now)));
            }));
        let press = press("press");
        arena.began(vec![on(&[&press]).input(1, 0., 0., 0.)]);
        (clock, arena, log)
    }

    #[test]
    fn lets_a_lift_queued_before_its_deadline_run_first() {
        let (clock, arena, log) = press_arena();
        clock.run_task(800.);
        arena.ended(vec![TouchInput::new(1, 0., 0., 300.)]);
        clock.run_task(800.);
        assert!(!log.borrow().iter().any(|e| e.starts_with("press:began")));
    }

    #[test]
    fn keeps_the_timestamps_of_queued_moves() {
        let (clock, arena, log) = press_arena();
        clock.run_task(800.);
        arena.moved(vec![TouchInput::new(1, 40., 0., 300.)]);
        clock.run_task(800.);
        assert!(!log.borrow().iter().any(|e| e.starts_with("press:began")));
        assert!(log.borrow().iter().any(|e| e == "press:failed@300"));
        arena.ended(vec![TouchInput::new(1, 40., 0., 820.)]);
    }

    #[test]
    fn still_fires_on_its_deadline_when_nothing_was_queued() {
        let (clock, arena, log) = press_arena();
        clock.run_task(800.);
        clock.run_task(800.);
        assert!(log.borrow().iter().any(|e| e == "press:began@500"));
        arena.ended(vec![TouchInput::new(1, 0., 0., 820.)]);
    }

    #[test]
    fn cancelling_an_idle_recognizer_leaves_it_possible_so_its_next_touch_counts() {
        let h = create_harness();
        let tap = tap("tap");
        tap.cancel();
        assert_eq!(tap.state(), GestureState::Possible);
        h.finger(1).down(10., 10., 0., on(&[&tap])).up(50.);
        assert!(h.logged("tap:ended@50"));
        tap.cancel();
        assert_eq!(tap.state(), GestureState::Possible);
        h.finger(2).down(10., 10., 1000., on(&[&tap])).up(1050.);
        assert!(h.logged("tap:ended@1050"));
    }

    #[test]
    fn a_waiting_recognition_that_fails_gives_its_claimed_touches_back() {
        let h = create_harness();
        let tap = tap("tap");
        let pan = pan("pan");
        pan.require_to_fail(&tap);
        let finger = h
            .finger(1)
            .down(0., 0., 0., on(&[&pan, &tap]))
            .move_to(20., 0., 16.);
        assert!(h.arena.is_claimed(1));
        pan.set_enabled(false);
        assert_eq!(pan.state(), GestureState::Failed);
        assert!(!h.arena.is_claimed(1));
        finger.up(40.);
    }
}

mod watcher_recognizer {
    use super::*;

    fn watcher(name: &str) -> PanGestureRecognizer {
        let mut options = PanOptions {
            direction: PanDirection::Vertical,
            ..PanOptions::named(name)
        };
        options.base.claims_touches = false;
        PanGestureRecognizer::new(options)
    }

    #[test]
    fn runs_alongside_a_claiming_pan_on_the_same_touch_and_claims_nothing() {
        let h = create_harness();
        let watcher = watcher("watcher");
        let sheet = pan_dir("sheet", PanDirection::Vertical);
        let finger = h
            .finger(1)
            .down(0., 100., 0., on(&[&sheet, &watcher]))
            .move_to(0., 115., 16.);
        assert_eq!(watcher.state(), GestureState::Began);
        assert_eq!(sheet.state(), GestureState::Began);
        assert_eq!(h.claims(), vec![1]);
        finger.up(32.);
    }

    #[test]
    fn cancels_no_press_under_it_when_it_begins_fr15_fr19() {
        let h = create_harness();
        let watcher = watcher("watcher");
        let press = RecordingConsumer::new();
        let finger = h
            .finger(1)
            .down(0., 100., 0., on(&[&watcher]).consumer(press.clone()))
            .move_to(0., 130., 16.);
        assert_eq!(watcher.state(), GestureState::Began);
        assert!(!press.events().contains(&"cancelled:1".to_string()));
        finger.up(32.);
        assert!(press.events().contains(&"ended:1".to_string()));
    }

    #[test]
    fn follows_a_touch_the_browser_took_for_a_scroll_until_the_lift() {
        let h = create_harness();
        let watcher = watcher("watcher");
        let finger = h
            .finger(1)
            .down(0., 100., 0., on(&[&watcher]))
            .move_to(0., 115., 16.);
        assert_eq!(watcher.state(), GestureState::Began);
        finger.taken_by_browser(20.);
        let finger = finger.move_to(0., 160., 32.);
        assert_eq!(watcher.state(), GestureState::Changed);
        finger.up(48.);
        assert!(h.logged("watcher:ended@48"));
    }

    #[test]
    fn leaves_the_browser_every_touch_behavior_its_direction_writes_no_touch_action() {
        let list = [watcher("watcher").any().clone()];
        assert_eq!(derive_touch_action(&list), None);
        let row = [
            watcher("watcher").any().clone(),
            pan_dir("swipe", PanDirection::Horizontal).any().clone(),
        ];
        assert_eq!(
            derive_touch_action(&row).as_deref(),
            Some("pan-y pinch-zoom")
        );
    }
}

mod recognizer_internals {
    use super::*;

    #[test]
    fn lets_the_gesture_system_reach_them() {
        let tap = tap("tap");
        assert!(tap.arena().is_none());
        assert!(tap.accepts_touch_input());
        assert!(tap.should_begin());
    }
}

mod recognizer_kind {
    use super::*;

    #[test]
    fn each_recognizer_reports_its_kind_a_screen_edge_pan_its_own_inv13() {
        let kinds: Vec<&str> = [
            TapGestureRecognizer::new(TapOptions::default())
                .any()
                .clone(),
            LongPressGestureRecognizer::new(LongPressOptions::default())
                .any()
                .clone(),
            PanGestureRecognizer::new(PanOptions::default())
                .any()
                .clone(),
            ScreenEdgePanGestureRecognizer::new(ScreenEdgePanOptions {
                edges: vec![ScreenEdge::Leading],
                ..ScreenEdgePanOptions::default()
            })
            .any()
            .clone(),
            SwipeGestureRecognizer::new(SwipeOptions::default())
                .any()
                .clone(),
            PinchGestureRecognizer::new(PinchOptions::default())
                .any()
                .clone(),
        ]
        .iter()
        .map(|r| r.kind().name())
        .collect();
        assert_eq!(
            kinds,
            ["tap", "longPress", "pan", "screenEdgePan", "swipe", "pinch"]
        );
    }
}
