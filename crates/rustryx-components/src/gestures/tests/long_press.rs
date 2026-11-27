//! Upstream `hooks/longPressDetector.test.ts` (the detector with no window: a hand start, a
//! touch press on the events' clock, a second finger, a late timer, a start handled late,
//! the tolerance, disabled, the latest options, a touch with no time, and the claim) and
//! `hooks/useLongPress.test.tsx` (the hook: the delay, disabled, multi-touch, the
//! tolerance on each axis, the lift, the latest options, unmount, the public start, and
//! the events' clock), with the area's fingers in a window.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gpui::{IntoElement, StatefulInteractiveElement as _, TestAppContext, TouchPhase};
use rustryx_primitives::gestures::{GestureClock, ManualClock, Point, TimerHandle};

use super::harness::*;
use crate::gestures::long_press::{
    DetectorOptions, LongPressDetector, LongPressTolerance, unwatch_press_claims,
    watch_press_claims,
};
use crate::gestures::*;

/// A clock whose `now` can read ahead of its timers: a busy window.
struct BusyClock {
    timers: Rc<ManualClock>,
    now: Cell<Option<f64>>,
}

impl GestureClock for BusyClock {
    fn now(&self) -> f64 {
        self.now.get().unwrap_or_else(|| self.timers.now())
    }
    fn set_timeout(&self, callback: Box<dyn FnOnce()>, ms: f64) -> TimerHandle {
        self.timers.set_timeout(callback, ms)
    }
    fn clear_timeout(&self, handle: TimerHandle) {
        self.timers.clear_timeout(handle)
    }
}

type Fired = Rc<RefCell<Vec<Point>>>;

fn detector(at: f64) -> (LongPressDetector, Rc<BusyClock>, Fired) {
    let timers = ManualClock::new();
    timers.set_time(at);
    let clock = Rc::new(BusyClock {
        timers,
        now: Cell::new(None),
    });
    let fired: Fired = Rc::default();
    let f = fired.clone();
    let d = LongPressDetector::new(
        DetectorOptions::new(move |p| f.borrow_mut().push(p)),
        clock.clone(),
    );
    (d, clock, fired)
}

fn advance(clock: &BusyClock, ms: f64) {
    clock.timers.advance_by(ms);
}

fn at(x: f64, y: f64) -> Vec<Point> {
    vec![Point::new(x, y)]
}

#[test]
fn fires_at_the_start_point_after_the_delay_when_started_by_hand_and_not_after_a_cancel() {
    let (d, clock, fired) = detector(0.);
    d.start(Point::new(3., 4.), None);
    advance(&clock, 499.);
    assert!(fired.borrow().is_empty());
    advance(&clock, 1.);
    assert_eq!(*fired.borrow(), [Point::new(3., 4.)]);
    d.start(Point::ZERO, None);
    d.cancel();
    advance(&clock, 1000.);
    assert_eq!(fired.borrow().len(), 1);
}

#[test]
fn claims_the_press_as_it_fires_and_nothing_for_a_press_it_drops() {
    let claims = Rc::new(Cell::new(0));
    let c = claims.clone();
    let watch = watch_press_claims(move || c.set(c.get() + 1));
    let (d, clock, _) = detector(0.);
    d.start(Point::ZERO, None);
    d.cancel();
    advance(&clock, 1000.);
    assert_eq!(claims.get(), 0);
    d.start(Point::ZERO, None);
    advance(&clock, 500.);
    assert_eq!(claims.get(), 1);
    unwatch_press_claims(watch);
}

#[test]
fn reports_a_touch_press_first_when_its_lift_is_stamped_past_the_deadline() {
    let (d, _, fired) = detector(1000.);
    d.touch_start(&LongPressTouch::new(at(10., 20.), Some(1000.)));
    d.touch_end(Some(&LongPressTouch::new(vec![], Some(1800.))));
    assert_eq!(*fired.borrow(), [Point::new(10., 20.)]);
    assert!(!d.is_pending());
}

#[test]
fn ends_the_press_on_a_second_finger_and_reports_it_first_when_stamped_past_the_deadline() {
    let finger = Point::new(5., 6.);
    let (early, clock, fired) = detector(1000.);
    early.touch_start(&LongPressTouch::new(vec![finger], Some(1000.)));
    early.touch_start(&LongPressTouch::new(vec![finger, finger], Some(1200.)));
    assert!(!early.is_pending());
    advance(&clock, 1000.);
    assert!(fired.borrow().is_empty());

    let (late, busy, fired) = detector(1000.);
    late.touch_start(&LongPressTouch::new(vec![finger], Some(1000.)));
    busy.now.set(Some(1600.));
    late.touch_start(&LongPressTouch::new(vec![finger, finger], Some(1600.)));
    assert_eq!(*fired.borrow(), [finger]);
    assert!(!late.is_pending());
}

#[test]
fn lets_a_lift_stamped_before_the_deadline_cancel_the_press_when_its_timer_runs_late() {
    let (d, busy, fired) = detector(1000.);
    d.touch_start(&LongPressTouch::new(at(0., 0.), Some(1000.)));
    // Busy past the deadline: the timer runs late and yields once to the queued lift.
    busy.now.set(Some(1700.));
    advance(&busy, 500.);
    assert!(fired.borrow().is_empty());
    d.touch_end(Some(&LongPressTouch::new(vec![], Some(1400.))));
    advance(&busy, 1000.);
    assert!(fired.borrow().is_empty());

    busy.now.set(Some(1700.));
    d.touch_start(&LongPressTouch::new(at(0., 0.), Some(1700.)));
    busy.now.set(Some(2400.));
    advance(&busy, 500.);
    assert!(fired.borrow().is_empty());
    advance(&busy, 1.);
    assert_eq!(fired.borrow().len(), 1);
}

#[test]
fn waits_only_for_the_rest_of_the_hold_when_its_start_is_handled_late() {
    let (d, clock, fired) = detector(1300.);
    d.touch_start(&LongPressTouch::new(at(0., 0.), Some(1000.)));
    advance(&clock, 199.);
    assert!(fired.borrow().is_empty());
    advance(&clock, 1.);
    assert_eq!(fired.borrow().len(), 1);
}

#[test]
fn drops_the_press_past_the_tolerance_or_past_a_per_press_tolerance() {
    let (d, clock, fired) = detector(0.);
    d.start(Point::ZERO, None);
    assert!(d.move_to(Point::new(10., 0.)));
    assert!(!d.move_to(Point::new(11., 0.)));
    d.start(
        Point::ZERO,
        Some(LongPressTolerance {
            move_cancel_px: Some(13.),
        }),
    );
    assert!(d.move_to(Point::new(13., 0.)));
    advance(&clock, 500.);
    assert_eq!(fired.borrow().len(), 1);
}

#[test]
fn starts_nothing_while_disabled_and_reads_the_latest_options_at_each_start_and_fire() {
    let (d, clock, first) = detector(0.);
    let mut disabled = DetectorOptions::new(|_| {});
    disabled.disabled = true;
    d.set_options(disabled);
    d.start(Point::ZERO, None);
    assert!(!d.is_pending());
    let f = first.clone();
    let mut options = DetectorOptions::new(move |p| f.borrow_mut().push(p));
    options.delay_ms = Some(200.);
    d.set_options(options);
    d.start(Point::new(1., 1.), None);
    let latest: Fired = Rc::default();
    let l = latest.clone();
    let mut options = DetectorOptions::new(move |p| l.borrow_mut().push(p));
    options.delay_ms = Some(200.);
    d.set_options(options);
    advance(&clock, 200.);
    assert!(first.borrow().is_empty());
    assert_eq!(*latest.borrow(), [Point::new(1., 1.)]);
}

#[test]
fn waits_the_full_delay_for_a_touch_that_carries_no_time() {
    let (d, clock, fired) = detector(10_000.);
    d.touch_start(&LongPressTouch::new(at(0., 0.), Some(0.)));
    advance(&clock, 499.);
    assert!(fired.borrow().is_empty());
    advance(&clock, 1.);
    assert_eq!(fired.borrow().len(), 1);
}

#[test]
fn reports_the_press_first_when_a_move_within_the_allowance_is_stamped_past_the_deadline() {
    let (d, _, fired) = detector(1000.);
    d.touch_start(&LongPressTouch::new(at(10., 20.), Some(1000.)));
    d.touch_move(&LongPressTouch::new(at(12., 21.), Some(1600.)));
    assert_eq!(*fired.borrow(), [Point::new(10., 20.)]);
}

#[test]
fn reports_the_press_when_a_cancel_is_stamped_past_the_deadline_and_drops_it_before() {
    let (d, clock, fired) = detector(1000.);
    d.touch_start(&LongPressTouch::new(at(0., 0.), Some(1000.)));
    d.touch_end(Some(&LongPressTouch::new(vec![], Some(1300.))));
    advance(&clock, 1000.);
    assert!(fired.borrow().is_empty());
    clock.timers.set_time(5000.);
    d.touch_start(&LongPressTouch::new(at(0., 0.), Some(5000.)));
    d.touch_end(Some(&LongPressTouch::new(vec![], Some(5600.))));
    assert_eq!(fired.borrow().len(), 1);
}

#[test]
fn never_waits_longer_than_the_delay_when_a_touch_is_stamped_ahead_of_its_clock() {
    let (d, clock, fired) = detector(1000.);
    d.touch_start(&LongPressTouch::new(at(0., 0.), Some(1400.)));
    advance(&clock, 499.);
    assert!(fired.borrow().is_empty());
    advance(&clock, 1.);
    assert_eq!(fired.borrow().len(), 1);
}

#[test]
fn measures_the_travel_on_each_axis() {
    let (d, clock, fired) = detector(0.);
    d.touch_start(&LongPressTouch::new(at(100., 100.), None));
    d.touch_move(&LongPressTouch::new(at(109., 109.), None));
    advance(&clock, 500.);
    assert_eq!(fired.borrow().len(), 1);
    d.touch_start(&LongPressTouch::new(at(100., 100.), None));
    d.touch_move(&LongPressTouch::new(at(100., 111.), None));
    advance(&clock, 500.);
    assert_eq!(fired.borrow().len(), 1);
}

// --- useLongPress ----------------------------------------------------------

struct Case {
    disabled: Cell<bool>,
    delay: Cell<Option<f64>>,
    shown: Cell<bool>,
    generation: Cell<u32>,
    fired: RefCell<Vec<(u32, Point)>>,
    clicks: Cell<u32>,
    handlers: RefCell<Option<LongPressHandlers>>,
}

fn mount(cx: &mut TestAppContext) -> (&mut gpui::VisualTestContext, Rc<Case>) {
    let case = Rc::new(Case {
        disabled: Cell::new(false),
        delay: Cell::new(None),
        shown: Cell::new(true),
        generation: Cell::new(0),
        fired: RefCell::default(),
        clicks: Cell::new(0),
        handlers: RefCell::default(),
    });
    let c = case.clone();
    let cx = host(cx, move |window, cx| {
        if !c.shown.get() {
            return gpui::div().into_any_element();
        }
        let (log, generation) = (c.clone(), c.generation.get());
        let mut options =
            LongPressOptions::new(move |p, _, _| log.fired.borrow_mut().push((generation, p)))
                .disabled(c.disabled.get());
        if let Some(ms) = c.delay.get() {
            options = options.delay_ms(ms);
        }
        let handlers = use_long_press("held", options, window, cx);
        *c.handlers.borrow_mut() = Some(handlers.clone());
        let clicks = c.clone();
        handlers
            .area(
                target("held").on_click(move |_, _, _| clicks.clicks.set(clicks.clicks.get() + 1)),
            )
            .into_any_element()
    });
    (cx, case)
}

fn rerender(cx: &mut gpui::VisualTestContext) {
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
}

#[gpui::test]
fn fires_on_long_press_with_the_start_point_after_the_delay(cx: &mut TestAppContext) {
    let (cx, case) = mount(cx);
    touch(cx, 1, TouchPhase::Started, 10., 20.);
    assert!(case.fired.borrow().is_empty());
    wait(cx, 500);
    assert_eq!(case.fired.borrow().len(), 1);
    assert_eq!(case.fired.borrow()[0].1, Point::new(10., 20.));
    // The press acted, so its lift is no tap.
    touch(cx, 1, TouchPhase::Ended, 10., 20.);
    assert_eq!(case.clicks.get(), 0);
}

#[gpui::test]
fn a_quick_tap_is_still_a_tap(cx: &mut TestAppContext) {
    let (cx, case) = mount(cx);
    touch(cx, 1, TouchPhase::Started, 10., 20.);
    wait(cx, 100);
    touch(cx, 1, TouchPhase::Ended, 10., 20.);
    wait(cx, 1000);
    assert!(case.fired.borrow().is_empty());
    assert_eq!(case.clicks.get(), 1);
}

#[gpui::test]
fn respects_a_custom_delay(cx: &mut TestAppContext) {
    let (cx, case) = mount(cx);
    case.delay.set(Some(1000.));
    rerender(cx);
    touch(cx, 1, TouchPhase::Started, 1., 2.);
    wait(cx, 999);
    assert!(case.fired.borrow().is_empty());
    wait(cx, 1);
    assert_eq!(case.fired.borrow().len(), 1);
}

#[gpui::test]
fn does_nothing_when_disabled(cx: &mut TestAppContext) {
    let (cx, case) = mount(cx);
    case.disabled.set(true);
    rerender(cx);
    touch(cx, 1, TouchPhase::Started, 0., 0.);
    wait(cx, 500);
    assert!(case.fired.borrow().is_empty());
}

#[gpui::test]
fn cancels_when_a_second_finger_touches_down_mid_press(cx: &mut TestAppContext) {
    let (cx, case) = mount(cx);
    touch(cx, 1, TouchPhase::Started, 0., 0.);
    touch(cx, 2, TouchPhase::Started, 50., 50.);
    wait(cx, 500);
    assert!(case.fired.borrow().is_empty());
}

#[gpui::test]
fn cancels_when_the_finger_moves_past_the_threshold_and_not_within_it(cx: &mut TestAppContext) {
    let (cx, case) = mount(cx);
    touch(cx, 1, TouchPhase::Started, 0., 0.);
    touch(cx, 1, TouchPhase::Moved, 15., 0.);
    wait(cx, 500);
    assert!(case.fired.borrow().is_empty());
    touch(cx, 1, TouchPhase::Ended, 15., 0.);
    touch(cx, 2, TouchPhase::Started, 0., 0.);
    touch(cx, 2, TouchPhase::Moved, 5., 5.);
    wait(cx, 500);
    assert_eq!(case.fired.borrow().len(), 1);
}

#[gpui::test]
fn cancels_on_touch_end(cx: &mut TestAppContext) {
    let (cx, case) = mount(cx);
    touch(cx, 1, TouchPhase::Started, 0., 0.);
    touch(cx, 1, TouchPhase::Ended, 0., 0.);
    wait(cx, 500);
    assert!(case.fired.borrow().is_empty());
}

#[gpui::test]
fn uses_the_options_of_the_latest_frame(cx: &mut TestAppContext) {
    let (cx, case) = mount(cx);
    case.disabled.set(true);
    rerender(cx);
    case.disabled.set(false);
    case.generation.set(1);
    rerender(cx);
    let handlers = case.handlers.borrow().clone().unwrap();
    handlers.start(Point::ZERO);
    wait(cx, 500);
    let fired = case.fired.borrow();
    assert_eq!(fired.len(), 1);
    assert_eq!(fired[0].0, 1);
}

#[gpui::test]
fn cancels_the_pending_timer_when_the_element_leaves(cx: &mut TestAppContext) {
    let (cx, case) = mount(cx);
    touch(cx, 1, TouchPhase::Started, 0., 0.);
    case.shown.set(false);
    rerender(cx);
    rerender(cx);
    wait(cx, 500);
    assert!(case.fired.borrow().is_empty());
}

#[gpui::test]
fn takes_only_the_point_in_its_public_start(cx: &mut TestAppContext) {
    let (cx, case) = mount(cx);
    let handlers = case.handlers.borrow().clone().unwrap();
    // The public tolerance (10 px) stands.
    handlers.start(Point::ZERO);
    assert!(!handlers.move_to(Point::new(20., 0.)));
    wait(cx, 600);
    assert!(case.fired.borrow().is_empty());
}
