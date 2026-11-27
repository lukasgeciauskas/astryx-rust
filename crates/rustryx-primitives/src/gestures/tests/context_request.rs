//! Upstream `__tests__/contextRequest.test.ts` (spec:AST-094 FR43): the
//! request's logic through an arena (the hold) and its pointer and key
//! entries (the host routes GPUI's right clicks and context-menu keys there).

use std::cell::RefCell;
use std::rc::Rc;

use super::harness::*;
use crate::gestures::context_request::*;
use crate::gestures::*;

struct Requests {
    core: Rc<ContextRequestCore>,
    seen: Rc<RefCell<Vec<ContextRequest>>>,
    log: Rc<RefCell<Vec<String>>>,
}

fn requests_with(is_enabled: Option<Rc<dyn Fn() -> bool>>) -> Requests {
    let seen = Rc::new(RefCell::new(Vec::new()));
    let log = Rc::new(RefCell::new(Vec::new()));
    let (s, l, h) = (seen.clone(), log.clone(), log.clone());
    let mut options = ContextRequestOptions::new(move |request: &ContextRequest| {
        s.borrow_mut().push(*request);
        l.borrow_mut().push("request".into());
    });
    options.is_enabled = is_enabled;
    options.on_hold = Some(Rc::new(move |phase: HoldPhase| {
        h.borrow_mut().push(
            match phase {
                HoldPhase::Committed => "committed",
                HoldPhase::Ended => "ended",
                HoldPhase::Cancelled => "cancelled",
            }
            .into(),
        );
    }));
    Requests {
        core: ContextRequestCore::new(options, None),
        seen,
        log,
    }
}

fn requests() -> Requests {
    requests_with(None)
}

impl Requests {
    fn hold(&self) -> AnyRecognizer {
        self.core.recognizer().any().clone()
    }
    fn kinds(&self) -> Vec<(ContextRequestKind, Option<PointerKind>)> {
        self.seen
            .borrow()
            .iter()
            .map(|r| (r.kind, r.pointer_type))
            .collect()
    }
}

use ContextRequestKind::*;

#[test]
fn makes_one_request_from_a_right_click_at_the_pointer() {
    let r = requests();
    assert!(
        r.core
            .pointer_request(Point::new(30., 12.), PointerKind::Mouse, 0.)
    );
    let seen = r.seen.borrow();
    assert_eq!(seen.len(), 1);
    assert_eq!(
        (seen[0].kind, seen[0].pointer_type, seen[0].point),
        (
            Pointer,
            Some(PointerKind::Mouse),
            Some(Point::new(30., 12.))
        )
    );
}

#[test]
fn makes_one_request_from_shift_f10_and_from_the_context_menu_key_with_no_point_and_none_from_the_menu_event_the_key_also_fires()
 {
    let r = requests();
    assert!(r.core.key_request(false, 0.));
    // The key's own menu event follows at once: its echo.
    assert!(r.core.pointer_request(Point::ZERO, PointerKind::Mouse, 0.));
    assert!(r.core.key_request(false, 2000.));
    let seen: Vec<_> = r.seen.borrow().iter().map(|r| (r.kind, r.point)).collect();
    assert_eq!(seen, [(Keyboard, None), (Keyboard, None)]);
}

#[test]
fn makes_one_request_from_a_held_shift_f10_its_repeats_make_none_and_the_keys_late_menu_event_is_still_its_echo()
 {
    let outer = requests();
    let r = requests();
    r.core.key_request(false, 0.);
    let mut time = 100.;
    while time <= 1200. {
        assert!(r.core.key_request(true, time));
        time += 100.;
    }
    assert!(
        r.core
            .pointer_request(Point::ZERO, PointerKind::Mouse, 1200.)
    );
    assert_eq!(r.kinds(), [(Keyboard, None)]);
    assert!(outer.seen.borrow().is_empty());
}

#[test]
fn reports_a_hold_that_requests_committed_as_it_requests_then_ended_at_its_lift() {
    let h = create_harness();
    let r = requests();
    h.finger(1).down(20., 10., 0., on(&[&r.hold()]));
    h.advance(600.);
    assert_eq!(*r.log.borrow(), strings(&["committed", "request"]));
    h.finger(1).up(700.);
    assert_eq!(*r.log.borrow(), strings(&["committed", "request", "ended"]));
}

#[test]
fn reports_nothing_of_a_hold_whose_request_is_disabled_before_it_begins() {
    let h = create_harness();
    let enabled = Rc::new(std::cell::Cell::new(true));
    let flag = enabled.clone();
    let r = requests_with(Some(Rc::new(move || flag.get())));
    h.finger(1).down(20., 10., 0., on(&[&r.hold()]));
    enabled.set(false);
    h.advance(600.);
    h.finger(1).up(700.);
    assert!(r.log.borrow().is_empty());
}

#[test]
fn makes_one_request_from_a_finger_held_still_at_the_hold_and_none_from_the_menu_that_hold_also_fires()
 {
    let h = create_harness();
    let r = requests();
    h.finger(1).down(20., 10., 0., on(&[&r.hold()]));
    assert!(
        r.core
            .pointer_request(Point::new(20., 10.), PointerKind::Touch, 100.)
    );
    assert!(r.seen.borrow().is_empty());
    h.advance(600.);
    let seen = r.seen.borrow().clone();
    assert_eq!(seen.len(), 1);
    assert_eq!(
        (seen[0].kind, seen[0].pointer_type, seen[0].point),
        (Hold, Some(PointerKind::Touch), Some(Point::new(20., 10.)))
    );
    h.finger(1).up(700.);
    assert_eq!(r.seen.borrow().len(), 1);
}

#[test]
fn takes_a_pen_held_still_too_and_no_mouse_held_still() {
    let h = create_harness();
    let r = requests();
    h.finger(1)
        .down(20., 10., 0., on(&[&r.hold()]).pointer(PointerKind::Pen));
    h.advance(600.);
    h.finger(1).up(700.);
    assert_eq!(r.kinds(), [(Hold, Some(PointerKind::Pen))]);
    h.finger(2).down(
        20.,
        10.,
        2000.,
        on(&[&r.hold()]).pointer(PointerKind::Mouse),
    );
    h.advance(2600.);
    h.finger(2).up(2700.);
    assert_eq!(r.seen.borrow().len(), 1);
}

#[test]
fn makes_one_request_from_a_pen_held_still_and_none_from_the_menu_that_pens_hold_also_fires_during_it_or_at_the_lift()
 {
    let h = create_harness();
    let r = requests();
    h.finger(1)
        .down(20., 10., 0., on(&[&r.hold()]).pointer(PointerKind::Pen));
    h.advance(600.);
    assert!(
        r.core
            .pointer_request(Point::new(20., 10.), PointerKind::Pen, 600.)
    );
    h.finger(1).up(700.);
    assert!(
        r.core
            .pointer_request(Point::new(20., 10.), PointerKind::Pen, 700.)
    );
    assert_eq!(r.kinds(), [(Hold, Some(PointerKind::Pen))]);
}

#[test]
fn makes_a_request_from_a_pens_barrel_right_click_right_after_a_tap_only_a_holds_lift_refuses_that_pens_menu()
 {
    let h = create_harness();
    let r = requests();
    h.finger(1)
        .down(20., 10., 0., on(&[&r.hold()]).pointer(PointerKind::Pen));
    h.finger(1).up(100.);
    h.advance(300.);
    assert!(
        r.core
            .pointer_request(Point::new(20., 10.), PointerKind::Pen, 300.)
    );
    assert_eq!(r.kinds(), [(Pointer, Some(PointerKind::Pen))]);
}

#[test]
fn makes_a_request_from_a_pens_barrel_right_click_right_after_a_drag_past_the_long_presss_tolerance()
 {
    let h = create_harness();
    let r = requests();
    h.finger(1)
        .down(20., 10., 0., on(&[&r.hold()]).pointer(PointerKind::Pen));
    h.finger(1).move_to(60., 10., 100.);
    h.advance(600.);
    h.finger(1).up(600.);
    h.advance(800.);
    assert!(
        r.core
            .pointer_request(Point::new(60., 10.), PointerKind::Pen, 800.)
    );
    assert_eq!(r.kinds(), [(Pointer, Some(PointerKind::Pen))]);
}

#[test]
fn makes_no_request_for_a_finger_that_lifts_before_the_hold_or_travels_past_its_tolerance() {
    let h = create_harness();
    let r = requests();
    h.finger(1).down(20., 10., 0., on(&[&r.hold()]));
    h.finger(1).up(200.);
    h.finger(2).down(20., 10., 1000., on(&[&r.hold()]));
    h.finger(2).move_to(20., 40., 1100.);
    h.advance(1700.);
    assert!(r.seen.borrow().is_empty());
}

#[test]
fn loses_the_hold_to_a_claim_already_made_on_the_finger_a_right_click_there_still_requests() {
    let h = create_harness();
    let r = requests();
    let reorder = LongPressGestureRecognizer::new(LongPressOptions {
        minimum_press_duration: 150.,
        ..LongPressOptions::named("reorder")
    });
    h.finger(1).down(5., 5., 0., on(&[&reorder, &r.hold()]));
    h.advance(200.);
    assert_eq!(reorder.state(), GestureState::Began);
    h.advance(900.);
    assert!(
        r.core
            .pointer_request(Point::new(5., 5.), PointerKind::Touch, 900.)
    );
    h.finger(1).up(1000.);
    assert!(r.seen.borrow().is_empty());
    assert!(
        r.core
            .pointer_request(Point::new(5., 5.), PointerKind::Mouse, 1000.)
    );
    assert_eq!(r.kinds(), [(Pointer, Some(PointerKind::Mouse))]);
}

#[test]
fn a_right_click_while_a_finger_rests_on_another_gesture_still_requests() {
    let h = create_harness();
    let r = requests();
    let elsewhere = LongPressGestureRecognizer::new(LongPressOptions::named("elsewhere"));
    h.finger(1).down(5., 40., 0., on(&[&elsewhere]));
    h.advance(100.);
    assert!(
        r.core
            .pointer_request(Point::new(5., 5.), PointerKind::Mouse, 100.)
    );
    assert_eq!(r.kinds(), [(Pointer, Some(PointerKind::Mouse))]);
    h.finger(1).up(200.);
}
