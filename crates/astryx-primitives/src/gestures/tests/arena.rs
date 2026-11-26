//! Upstream `__tests__/arena.test.ts`: the arena's own rules with scripted
//! recognizers and recording consumers.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use super::harness::*;
use crate::gestures::*;

fn named(name: &str) -> RecognizerOptions {
    RecognizerOptions::named(name)
}

// --- who wins ---------------------------------------------------------------

#[test]
fn is_exclusive_by_default_the_first_to_recognize_fails_the_others() {
    let h = create_harness();
    let a = scripted("a", on_move());
    let b = scripted("b", Script::default());
    h.finger(1)
        .down(0., 0., 0., on(&[&a, &b]))
        .move_to(5., 0., 16.);
    assert_eq!(h.log(), strings(&["a:began@16", "b:failed@16"]));
    assert_eq!(h.claims(), vec![1]);
}

#[test]
fn lets_two_recognize_together_only_when_allowed() {
    let h = create_harness();
    let a = scripted("a", on_move());
    let b = scripted("b", on_move());
    a.recognize_simultaneously_with(&[&b]);
    h.finger(1)
        .down(0., 0., 0., on(&[&a, &b]))
        .move_to(5., 0., 16.);
    assert_eq!(h.log(), strings(&["a:began@16", "b:began@16"]));
}

#[test]
fn holds_a_recognizer_that_requires_another_to_fail_until_it_fails() {
    let h = create_harness();
    let single = at_lift("single", named("single"));
    let double = waits_after_lift("double", 400.);
    single.require_to_fail(&double);
    h.finger(1).down(0., 0., 0., on(&[&single, &double]));
    h.finger(1).up(50.);
    assert_eq!(single.state(), GestureState::Possible);
    assert_eq!(single.pending(), Some(PendingRecognition::Ended));
    h.advance(400.);
    assert_eq!(
        h.log(),
        strings(&[
            "double:failed@400",
            "single:ended@400",
            "single:possible@400",
            "double:possible@400",
        ])
    );
}

#[test]
fn fails_the_waiting_recognizer_when_the_required_one_recognizes() {
    let h = create_harness();
    let single = at_lift("single", named("single"));
    let double = scripted_with(
        "double",
        Script::default(),
        ScriptOptions {
            discrete: true,
            ..ScriptOptions::default()
        },
    );
    single.require_to_fail(&double);
    h.finger(1).down(0., 0., 0., on(&[&single, &double]));
    double.recognize();
    assert_eq!(h.log(), strings(&["double:ended@0", "single:failed@0"]));
}

#[test]
fn is_not_held_up_by_a_required_recognizer_that_is_not_involved() {
    let h = create_harness();
    let idle = scripted("idle", Script::default());
    let tap = at_lift("tap", named("tap"));
    tap.require_to_fail(&idle);
    h.finger(1).down(0., 0., 0., on(&[&tap]));
    h.finger(1).up(50.);
    assert_eq!(h.log()[0], "tap:ended@50");
}

#[test]
fn honors_failure_requirements_from_the_delegate() {
    let h = create_harness();
    let double = waits_after_lift("double", 400.);
    let weak = double.downgrade();
    let mut options = named("single");
    options.should_require_failure_of = Some(Rc::new(
        move |_me: &AnyRecognizer, other: &AnyRecognizer| {
            weak.upgrade().is_some_and(|d| d == *other)
        },
    ));
    let single = at_lift("single", options);
    h.finger(1).down(0., 0., 0., on(&[&single, &double]));
    h.finger(1).up(50.);
    assert_eq!(single.pending(), Some(PendingRecognition::Ended));
}

#[test]
fn honors_should_begin_and_should_receive_touch() {
    let h = create_harness();
    let mut refusing = ScriptOptions::default();
    refusing.base.should_begin = Some(Rc::new(|_: &AnyRecognizer| false));
    let refused = scripted_with("refused", on_move(), refusing);
    let mut blind_options = ScriptOptions::default();
    blind_options.base.should_receive_touch =
        Some(Rc::new(|_: &AnyRecognizer, _: &GestureTouch| false));
    let blind = scripted_with("blind", Script::default(), blind_options);
    h.finger(1)
        .down(0., 0., 0., on(&[&refused, &blind]))
        .move_to(5., 0., 16.);
    assert_eq!(h.log(), strings(&["refused:failed@16"]));
    assert_eq!(blind.number_of_touches(), 0);
}

#[test]
fn gives_a_tie_on_the_same_event_to_the_innermost_recognizer() {
    let h = create_harness();
    let inner = scripted("inner", on_move());
    let outer = scripted("outer", on_move());
    h.finger(1)
        .down(0., 0., 0., on(&[&inner, &outer]))
        .move_to(5., 0., 16.);
    assert_eq!(h.log(), strings(&["inner:began@16", "outer:failed@16"]));
}

#[test]
fn lets_fingers_on_unrelated_views_drive_their_own_recognizers() {
    let h = create_harness();
    let left = scripted("left", on_move());
    let right = scripted("right", on_move());
    h.finger(1).down(0., 0., 0., on(&[&left]));
    h.finger(2).down(300., 0., 0., on(&[&right]));
    h.finger(1).move_to(5., 0., 16.);
    h.finger(2).move_to(305., 0., 16.);
    assert_eq!(left.state(), GestureState::Began);
    assert_eq!(right.state(), GestureState::Began);
}

// --- delivery to the view ---------------------------------------------------------

#[test]
fn delivers_an_unclaimed_touch_and_allows_its_click() {
    let h = create_harness();
    let consumer = RecordingConsumer::new();
    let r = scripted("r", Script::default());
    let finger = h
        .finger(1)
        .down(0., 0., 0., on(&[&r]).consumer(consumer.clone()));
    let finger = finger.move_to(3., 0., 16.);
    assert_eq!(finger.up(60.), Some(ClickDisposition::Allow));
    assert_eq!(
        consumer.events(),
        strings(&["began:1", "moved:1", "ended:1"])
    );
}

#[test]
fn cancels_the_view_and_swallows_the_click_once_a_recognizer_takes_the_touch() {
    let h = create_harness();
    let consumer = RecordingConsumer::new();
    let r = scripted("r", on_move());
    let finger = h
        .finger(1)
        .down(0., 0., 0., on(&[&r]).consumer(consumer.clone()));
    let finger = finger.move_to(5., 0., 16.);
    assert_eq!(consumer.events(), strings(&["began:1", "cancelled:1"]));
    assert_eq!(finger.up(60.), Some(ClickDisposition::Suppress));
}

#[test]
fn keeps_the_view_and_the_click_with_cancels_touches_in_view_off() {
    let h = create_harness();
    let consumer = RecordingConsumer::new();
    let mut options = ScriptOptions::default();
    options.base.cancels_touches_in_view = false;
    let r = scripted_with("r", on_move(), options);
    let finger = h
        .finger(1)
        .down(0., 0., 0., on(&[&r]).consumer(consumer.clone()));
    let finger = finger.move_to(5., 0., 16.);
    assert_eq!(finger.up(60.), Some(ClickDisposition::Allow));
    assert_eq!(
        consumer.events(),
        strings(&["began:1", "moved:1", "ended:1"])
    );
}

#[test]
fn holds_the_view_until_the_recognizer_fails_with_delays_touches_began() {
    let h = create_harness();
    let consumer = RecordingConsumer::new();
    let mut options = ScriptOptions::default();
    options.base.delays_touches_began = true;
    let r = scripted_with(
        "r",
        Script {
            began: step(|r, event| r.wait_until(event.time + 100.)),
            ..Script::default()
        },
        options,
    );
    h.finger(1)
        .down(0., 0., 0., on(&[&r]).consumer(consumer.clone()));
    assert!(consumer.events().is_empty());
    h.advance(100.);
    assert_eq!(consumer.events(), strings(&["began:1"]));
}

#[test]
fn defers_the_click_a_waiting_recognizer_may_still_claim_then_replays_it() {
    let h = create_harness();
    let consumer = RecordingConsumer::new();
    let double = waits_after_lift("double", 400.);
    let finger = h
        .finger(1)
        .down(0., 0., 0., on(&[&double]).consumer(consumer.clone()));
    assert_eq!(finger.up(70.), Some(ClickDisposition::Defer));
    assert_eq!(consumer.events(), strings(&["began:1"]));
    h.advance(400.);
    assert_eq!(consumer.events(), strings(&["began:1", "ended:1"]));
    assert_eq!(h.deferred(), vec![(1, true, 400.)]);
}

#[test]
fn drops_the_deferred_click_when_the_waiting_recognizer_recognizes() {
    let h = create_harness();
    let consumer = RecordingConsumer::new();
    let lifts = Rc::new(Cell::new(0));
    let counted = lifts.clone();
    // Waits after the first lift, recognizes at the second.
    let double = scripted_with(
        "double",
        Script {
            ended: step(move |r, _| {
                let n = counted.get();
                counted.set(n + 1);
                if n == 0 {
                    r.wait_until(400.)
                } else {
                    r.recognize()
                }
            }),
            ..Script::default()
        },
        ScriptOptions {
            discrete: true,
            ..ScriptOptions::default()
        },
    );
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
fn fails_a_possible_recognizer_and_cancels_the_view_when_the_browser_takes_the_touch() {
    let h = create_harness();
    let consumer = RecordingConsumer::new();
    let r = scripted("r", Script::default());
    h.finger(1)
        .down(0., 0., 0., on(&[&r]).consumer(consumer.clone()));
    h.finger(1).taken_by_browser(30.);
    assert_eq!(consumer.events(), strings(&["began:1", "cancelled:1"]));
    assert_eq!(h.log(), strings(&["r:failed@30", "r:possible@30"]));
}

// --- cancellation and time ------------------------------------------------------

#[test]
fn cancels_an_active_gesture_and_fails_a_possible_one_on_a_system_cancel() {
    let h = create_harness();
    let active = scripted("active", on_move());
    let waiting = scripted("waiting", Script::default());
    h.finger(1)
        .down(0., 0., 0., on(&[&active]))
        .move_to(5., 0., 16.);
    h.finger(2).down(100., 0., 20., on(&[&waiting]));
    h.arena.cancelled(&[1, 2], 50.);
    assert_eq!(
        h.log(),
        strings(&[
            "active:began@16",
            "active:cancelled@50",
            "waiting:failed@50",
            "active:possible@50",
            "waiting:possible@50",
        ])
    );
}

#[test]
fn cancels_an_active_recognizer_from_code_which_stays_cancelled_until_the_lift() {
    let h = create_harness();
    let r = scripted("r", on_move());
    h.finger(1).down(0., 0., 0., on(&[&r])).move_to(5., 0., 16.);
    r.cancel();
    assert_eq!(r.state(), GestureState::Cancelled);
    h.finger(1).move_to(10., 0., 32.);
    assert_eq!(r.state(), GestureState::Cancelled);
    h.finger(1).up(48.);
    assert_eq!(r.state(), GestureState::Possible);
}

#[test]
fn runs_a_deadline_due_before_a_late_input_first_at_its_own_time() {
    let h = create_harness();
    let r = scripted(
        "r",
        Script {
            began: step(|r, event| r.wait_until(event.time + 500.)),
            ..Script::default()
        },
    );
    h.finger(1).down(0., 0., 1000., on(&[&r]));
    // The main thread was blocked: the next input is processed at 1900.
    h.finger(1).move_to(5., 0., 1900.);
    assert_eq!(h.log()[0], "r:failed@1500");
}

#[test]
fn never_moves_the_event_clock_backwards() {
    let h = create_harness();
    let r = scripted("r", on_move());
    h.finger(1).down(0., 0., 100., on(&[&r]));
    h.finger(1).move_to(5., 0., 90.);
    assert_eq!(h.log(), strings(&["r:began@100"]));
}

/// A clock whose timers run one task at a time, as an event loop does.
#[derive(Default)]
struct TaskClock {
    time: Cell<f64>,
    next: Cell<u64>,
    #[allow(clippy::type_complexity)]
    tasks: RefCell<Vec<(u64, f64, Option<Box<dyn FnOnce()>>)>>,
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
            .push((id, self.time.get() + delay, Some(run)));
        id
    }
    fn clear_timeout(&self, id: TimerHandle) {
        self.tasks.borrow_mut().retain(|(t, _, _)| *t != id);
    }
}

impl TaskClock {
    /// Runs the earliest task due by `time`, if any.
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
            if let Some(run) = run {
                run();
            }
        }
    }
}

/// A long press: recognizes 500 ms after touch-down unless the touch lifts.
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
    let press = scripted(
        "press",
        Script {
            began: step(|r, event| r.recognize_at(event.time + 500.)),
            ..Script::default()
        },
    );
    arena.began(vec![on(&[&press]).input(1, 0., 0., 0.)]);
    (clock, arena, log)
}

#[test]
fn a_late_timer_lets_a_lift_queued_before_its_deadline_run_first() {
    let (clock, arena, log) = press_arena();
    // A long task held the main thread from 100 to 800 ms; the finger lifted
    // at 300. The timer task runs first, then the queued lift.
    clock.run_task(800.);
    arena.ended(vec![TouchInput::new(1, 0., 0., 300.)]);
    clock.run_task(800.);
    assert!(!log.borrow().iter().any(|e| e.starts_with("press:began")));
}

#[test]
fn a_late_timer_still_fires_on_its_deadline_when_nothing_was_queued() {
    let (clock, arena, log) = press_arena();
    clock.run_task(800.);
    clock.run_task(800.);
    assert!(log.borrow().iter().any(|e| e == "press:began@500"));
    arena.ended(vec![TouchInput::new(1, 0., 0., 820.)]);
}

// --- claims ----------------------------------------------------------------------

#[test]
fn keeps_an_idle_recognizer_usable_after_cancel() {
    let h = create_harness();
    let r = scripted("r", on_move());
    r.cancel();
    assert_eq!(r.state(), GestureState::Possible);
    h.finger(1).down(0., 0., 0., on(&[&r])).move_to(5., 0., 16.);
    assert!(h.logged("r:began@16"));
    h.finger(1).up(32.);
}

#[test]
fn gives_back_the_claim_of_a_waiting_recognition_that_fails() {
    let h = create_harness();
    let waiting = scripted("waiting", on_move());
    let blocker = scripted("blocker", Script::default());
    waiting.require_to_fail(&blocker);
    h.finger(1)
        .down(0., 0., 0., on(&[&waiting, &blocker]))
        .move_to(5., 0., 16.);
    assert!(h.arena.is_claimed(1));
    waiting.cancel();
    assert_eq!(waiting.state(), GestureState::Failed);
    assert!(!h.arena.is_claimed(1));
    h.finger(1).up(32.);
}

#[test]
fn gives_a_resting_finger_back_when_its_gesture_is_cancelled_so_a_pinch_can_zoom_the_page() {
    let h = create_harness();
    let r = scripted("r", on_move());
    let first = h.finger(1).down(0., 0., 0., on(&[&r])).move_to(5., 0., 16.);
    assert!(h.arena.is_claimed(1));
    r.cancel();
    assert!(!h.arena.is_claimed(1));
    // A second finger lands with nothing on its path: the window owns both.
    let second = h.finger(2).down(200., 200., 40., on(&[]));
    assert!(!h.arena.is_claimed(2));
    let first = first.move_to(0., 0., 56.);
    let second = second.move_to(220., 220., 56.);
    assert!(!h.arena.is_claimed(1));
    first.up(80.);
    second.up(80.);
}

#[test]
fn keeps_a_moving_finger_claimed_until_it_lifts_when_its_gesture_cancels_itself_on_that_move_fr14()
{
    let h = create_harness();
    // A long press's shape: it begins while the finger rests, then cancels
    // itself on the first move past its tolerance.
    let press = scripted(
        "press",
        Script {
            began: step(|r, event| r.recognize_at(event.time + 500.)),
            moved: step(|r, event| {
                if let Some(touch) = event.all_touches.first()
                    && (touch.y() - touch.start_y()).abs() > 10.
                    && r.state().is_active()
                {
                    r.cancel_self();
                }
            }),
            ..Script::default()
        },
    );
    let finger = h.finger(1).down(0., 0., 0., on(&[&press]));
    h.advance(600.);
    assert_eq!(press.state(), GestureState::Began);
    assert!(h.arena.is_claimed(1));
    let finger = finger.move_to(0., -11., 616.);
    assert_eq!(press.state(), GestureState::Cancelled);
    assert!(h.arena.is_claimed(1));
    let finger = finger.move_to(0., -60., 632.);
    assert!(h.arena.is_claimed(1));
    finger.up(700.);
    assert!(!h.arena.is_claimed(1));
}

// --- a touch that begins off an active recognizer's hit path ----------------------

fn recorder(name: &str) -> (AnyRecognizer, Rc<RefCell<Vec<u64>>>) {
    let record = Rc::new(RefCell::new(Vec::new()));
    let r = scripted_with(
        name,
        Script::default(),
        ScriptOptions {
            elsewhere: Some(record.clone()),
            ..ScriptOptions::default()
        },
    );
    (r, record)
}

#[test]
fn a_touch_off_the_path_is_reported_to_the_recognizer_not_delivered_as_its_own() {
    let h = create_harness();
    let (recorder, elsewhere) = recorder("recorder");
    let first = h.finger(1).down(0., 0., 0., on(&[&recorder]));
    assert!(elsewhere.borrow().is_empty());
    let second = h.finger(2).down(300., 300., 16., on(&[]));
    assert_eq!(*elsewhere.borrow(), vec![2]);
    assert_eq!(recorder.number_of_touches(), 1);
    first.up(40.);
    second.up(40.);
}

#[test]
fn a_touch_off_the_path_is_not_reported_to_a_recognizer_tracking_nothing() {
    let h = create_harness();
    let (recorder, elsewhere) = recorder("recorder");
    h.finger(1).down(0., 0., 0., on(&[&recorder])).up(20.);
    h.finger(2).down(300., 300., 100., on(&[])).up(120.);
    assert!(elsewhere.borrow().is_empty());
}

// --- resolution order across fingers ------------------------------------------------

#[test]
fn lets_the_inner_recognizer_win_a_tie_even_when_another_finger_brought_the_outer_one_first() {
    let h = create_harness();
    let inner = scripted("inner", on_move());
    let outer = scripted("outer", on_move());
    // Finger 1 lands on the outer view only; finger 2 on the inner view.
    h.arena.began(vec![
        on(&[&outer]).input(1, 0., 0., 0.),
        on(&[&inner, &outer]).input(2, 50., 0., 0.),
    ]);
    h.arena.moved(vec![
        TouchInput::new(1, 10., 0., 16.),
        TouchInput::new(2, 60., 0., 16.),
    ]);
    assert_eq!(inner.state(), GestureState::Began);
    assert_eq!(outer.state(), GestureState::Failed);
    h.arena.ended(vec![
        TouchInput::new(1, 10., 0., 32.),
        TouchInput::new(2, 60., 0., 32.),
    ]);
}

// --- a held view ----------------------------------------------------------------------

#[test]
fn a_held_view_is_released_in_the_same_settle_as_the_recognizer_that_held_it_finishes() {
    let h = create_harness();
    let consumer = RecordingConsumer::new();
    // Delays the view's touch, then lifts without deciding.
    let mut options = ScriptOptions::default();
    options.base.delays_touches_began = true;
    let r = scripted_with(
        "r",
        Script {
            ended: step(|_, _| {}),
            ..Script::default()
        },
        options,
    );
    h.finger(1)
        .down(0., 0., 0., on(&[&r]).consumer(consumer.clone()));
    assert!(consumer.events().is_empty());
    h.finger(1).up(40.);
    assert_eq!(consumer.events(), strings(&["began:1", "ended:1"]));
}

// --- review fixes -----------------------------------------------------------------------

#[test]
fn runs_a_cancel_from_code_at_the_last_event_time_not_wall_time() {
    let h = create_harness();
    let other = scripted("other", Script::default());
    h.finger(9).down(500., 500., 0., on(&[&other])).up(10.);
    let press = scripted(
        "press",
        Script {
            began: step(|r, event| r.recognize_at(event.time + 500.)),
            moved: step(|r, event| {
                if let Some(touch) = event.all_touches.first()
                    && (touch.x() - touch.start_x()).abs() > 10.
                {
                    r.fail();
                }
            }),
            ..Script::default()
        },
    );
    h.finger(1).down(0., 0., 1000., on(&[&press]));
    // Wall time is 1800; a 40 px move stamped 1300 is still queued.
    h.clock.set_time(1800.);
    other.cancel();
    h.arena.moved(vec![TouchInput::new(1, 40., 0., 1300.)]);
    assert!(h.logged("press:failed@1300"));
    assert!(!h.log().iter().any(|e| e.starts_with("press:began")));
    h.finger(1).up_at(1900., 40., 0.);
}

#[test]
fn keeps_two_recognizers_exclusive_when_only_one_delegate_allows_both() {
    let h = create_harness();
    let mut options = ScriptOptions::default();
    options.base.should_recognize_simultaneously =
        Some(Rc::new(|_: &AnyRecognizer, _: &AnyRecognizer| true));
    let a = scripted_with("a", on_move(), options);
    let b = scripted("b", on_move());
    h.finger(1)
        .down(0., 0., 0., on(&[&a, &b]))
        .move_to(5., 0., 16.);
    assert!(h.logged("a:began@16"));
    assert_eq!(b.state(), GestureState::Failed);
    h.finger(1).up(32.);
}

#[test]
fn gives_the_click_with_the_touch_when_the_views_hold_ends_on_the_lift() {
    let h = create_harness();
    let consumer = RecordingConsumer::new();
    let mut options = ScriptOptions::default();
    options.base.delays_touches_began = true;
    let r = scripted_with("r", Script::default(), options);
    h.finger(1)
        .down(0., 0., 0., on(&[&r]).consumer(consumer.clone()));
    assert!(consumer.events().is_empty());
    let click = h.finger(1).up(40.);
    assert_eq!(click, Some(ClickDisposition::Allow));
    assert_eq!(consumer.events(), strings(&["began:1", "ended:1"]));
    assert!(h.deferred().is_empty());
}

#[test]
fn keeps_a_recognized_discrete_gestures_claim_when_a_partner_is_cancelled() {
    let h = create_harness();
    let swipe = scripted_with(
        "swipe",
        Script {
            moved: step(|r, _| {
                if r.state() == GestureState::Possible {
                    r.recognize();
                }
            }),
            ..Script::default()
        },
        ScriptOptions {
            discrete: true,
            ..ScriptOptions::default()
        },
    );
    let pan = scripted("pan", on_move());
    swipe.recognize_simultaneously_with(&[&pan]);
    h.finger(1)
        .down(0., 0., 0., on(&[&swipe, &pan]))
        .move_to(5., 0., 16.);
    assert_eq!(swipe.state(), GestureState::Ended);
    assert!(h.arena.is_claimed(1));
    pan.cancel();
    assert!(h.arena.is_claimed(1));
    h.finger(1).up(32.);
}

// --- a lift that decides a waiting recognition ----------------------------------------

#[test]
fn answers_the_click_with_the_final_outcome_not_a_defer_it_already_resolved() {
    let h = create_harness();
    let consumer = RecordingConsumer::new();
    // r recognizes at the lift but waits for q to fail; q lifts undecided.
    let q = scripted(
        "q",
        Script {
            ended: step(|_, _| {}),
            ..Script::default()
        },
    );
    let r = at_lift("r", named("r"));
    r.require_to_fail(&q);
    h.finger(1)
        .down(0., 0., 0., on(&[&r, &q]).consumer(consumer.clone()));
    let click = h.finger(1).up(40.);
    assert!(h.logged("r:ended@40"));
    assert_eq!(click, Some(ClickDisposition::Suppress));
    assert!(h.deferred().is_empty());
}

// --- round-five review fixes ----------------------------------------------------------

#[test]
fn never_claims_a_touch_the_browser_took_even_for_a_watcher_that_begins_later_fr14() {
    let h = create_harness();
    let watcher = scripted_with(
        "watcher",
        on_move(),
        ScriptOptions {
            observes: true,
            ..ScriptOptions::default()
        },
    );
    let finger = h.finger(1).down(0., 0., 0., on(&[&watcher]));
    finger.taken_by_browser(10.);
    let finger = finger.move_to(0., 40., 20.);
    assert!(h.logged("watcher:began@20"));
    assert!(h.claims().is_empty());
    assert!(!h.arena.is_claimed(1));
    finger.up(40.);
}

#[test]
fn settles_once_with_the_outermost_call_when_a_listener_cancels_during_a_touch_down() {
    let h = create_harness();
    let elsewhere = scripted("elsewhere", Script::default());
    h.finger(9).down(500., 500., 0., on(&[&elsewhere]));
    let consumer = RecordingConsumer::new();
    let target = elsewhere.clone();
    let first = scripted(
        "first",
        Script {
            began: step(move |_, _| target.cancel()),
            ..Script::default()
        },
    );
    let mut options = ScriptOptions::default();
    options.base.delays_touches_began = true;
    let holder = scripted_with("holder", Script::default(), options);
    h.finger(1).down(
        0.,
        0.,
        10.,
        on(&[&first, &holder]).consumer(consumer.clone()),
    );
    // The holder heard the touch and fails at the lift: the click goes.
    assert_eq!(h.finger(1).up(40.), Some(ClickDisposition::Allow));
    assert_eq!(consumer.events(), strings(&["began:1", "ended:1"]));
    h.finger(9).up(50.);
}

#[test]
fn keeps_a_discrete_winner_exclusive_against_a_waiter_its_win_releases_fr15() {
    let h = create_harness();
    let winner = scripted_with(
        "R",
        Script {
            moved: step(|r, event| {
                if event.time >= 32. {
                    r.recognize();
                }
            }),
            ..Script::default()
        },
        ScriptOptions {
            discrete: true,
            ..ScriptOptions::default()
        },
    );
    let blocker = scripted(
        "O1",
        Script {
            moved: step(|_, _| {}),
            ..Script::default()
        },
    );
    let waiter = scripted("P", on_move());
    waiter.require_to_fail(&blocker);
    h.finger(1)
        .down(0., 0., 0., on(&[&winner, &blocker, &waiter]))
        .move_to(5., 0., 16.)
        .move_to(10., 0., 32.);
    assert!(h.logged("R:ended@32"));
    assert!(!h.logged("P:began@32"));
    assert_eq!(waiter.state(), GestureState::Failed);
    h.finger(1).up(48.);
}

#[test]
fn lets_the_innermost_deadline_win_a_tie_at_the_same_instant_fr15() {
    let h = create_harness();
    let inner = scripted(
        "inner",
        Script {
            moved: step(|r, event| {
                if r.state() == GestureState::Possible && event.time == 10. {
                    r.recognize_at(500.);
                }
            }),
            ..Script::default()
        },
    );
    let outer = scripted(
        "outer",
        Script {
            began: step(|r, _| r.recognize_at(500.)),
            moved: step(|_, _| {}),
            ..Script::default()
        },
    );
    h.finger(1)
        .down(0., 0., 0., on(&[&inner, &outer]))
        .move_to(1., 0., 10.);
    h.arena.advance_time(500.);
    assert!(h.logged("inner:began@500"));
    assert_eq!(outer.state(), GestureState::Failed);
    h.finger(1).up(600.);
}

// --- a waiting recognition that fails on its re-attempt --------------------------------

#[test]
fn gives_its_claim_back_when_should_begin_refuses_once_the_requirement_fails_fr19() {
    let h = create_harness();
    let may_begin = Rc::new(Cell::new(true));
    let blocker = scripted(
        "q",
        Script {
            moved: step(|r, event| {
                if event.time >= 32. {
                    r.fail();
                }
            }),
            ..Script::default()
        },
    );
    let flag = may_begin.clone();
    let mut options = ScriptOptions::default();
    options.base.should_begin = Some(Rc::new(move |_: &AnyRecognizer| flag.get()));
    let waiter = scripted_with("r", on_move(), options);
    waiter.require_to_fail(&blocker);
    let finger = h
        .finger(1)
        .down(0., 0., 0., on(&[&waiter, &blocker]))
        .move_to(20., 0., 16.);
    // r waits for q and holds the touch meanwhile.
    assert!(h.arena.is_claimed(1));
    may_begin.set(false);
    let finger = finger.move_to(40., 0., 32.);
    assert!(h.logged("q:failed@32"));
    assert_eq!(waiter.state(), GestureState::Failed);
    assert!(!h.arena.is_claimed(1));
    finger.up(48.);
}

// --- a deadline inside a re-entrant entry ------------------------------------------------

#[test]
fn settles_with_the_outermost_call_so_a_listener_feeding_input_mid_delivery_orphans_nothing() {
    let h = create_harness();
    let timed = scripted(
        "timed",
        Script {
            began: step(|r, event| r.recognize_at(event.time + 180.)),
            ..Script::default()
        },
    );
    h.finger(9).down(500., 500., 0., on(&[&timed]));
    let consumer = RecordingConsumer::new();
    // A listener that feeds input while the touch-down is being delivered:
    // the nested entry runs timed's deadline (180).
    let arena = h.arena.clone();
    let first = scripted(
        "first",
        Script {
            began: step(move |_, _| arena.moved(vec![TouchInput::new(9, 500., 500., 200.)])),
            ..Script::default()
        },
    );
    let mut options = ScriptOptions::default();
    options.base.delays_touches_began = true;
    let holder = scripted_with("holder", Script::default(), options);
    h.finger(1).down(
        0.,
        0.,
        150.,
        on(&[&first, &holder]).consumer(consumer.clone()),
    );
    assert!(h.logged("timed:began@180"));
    assert_eq!(h.finger(1).up(250.), Some(ClickDisposition::Allow));
    assert_eq!(consumer.events(), strings(&["began:1", "ended:1"]));
    h.finger(9).up(300.);
}
