//! Upstream `__tests__/PressTracker.test.ts`: UIControl semantics through the
//! arena, with the tracker's own timers on a manual clock (vitest's fake
//! timers upstream).

use std::cell::RefCell;
use std::rc::Rc;

use super::harness::*;
use crate::gestures::*;
use crate::motion::uikit::gestures::{control as CONTROL, pan as PAN, press as PRESS};

const BOUNDS: PressBounds = PressBounds {
    left: 100.,
    top: 100.,
    right: 200.,
    bottom: 140.,
};
const EXPANSION: f64 = CONTROL::TRACKING_EXPANSION;
const DELAY: f64 = PRESS::CONTENT_TOUCH_DELAY;
const FLASH: f64 = PRESS::QUICK_TAP_FLASH;

struct Setup {
    events: Rc<RefCell<Vec<ControlEvent>>>,
    highlights: Rc<RefCell<Vec<bool>>>,
    tracker: PressTracker,
    timers: Rc<ManualClock>,
    h: Harness,
}

fn setup_with(edit: impl FnOnce(PressTrackerOptions) -> PressTrackerOptions) -> Setup {
    let events = Rc::new(RefCell::new(Vec::new()));
    let highlights = Rc::new(RefCell::new(Vec::new()));
    let timers = ManualClock::new();
    let (e, l) = (events.clone(), highlights.clone());
    let mut options = PressTrackerOptions::new(|| Some(BOUNDS));
    options.on_control_event = Some(Rc::new(move |event, _touch: &GestureTouch| {
        e.borrow_mut().push(event)
    }));
    options.on_highlight_change = Some(Rc::new(move |value| l.borrow_mut().push(value)));
    let tracker = PressTracker::new(edit(options), timers.clone());
    Setup {
        events,
        highlights,
        tracker,
        timers,
        h: create_harness(),
    }
}

fn setup() -> Setup {
    setup_with(|o| o)
}

fn consumer(s: &Setup) -> Down {
    on(&[]).consumer(s.tracker.consumer())
}

use ControlEvent::*;

#[test]
fn a_keeps_touch_tracker_keeps_only_the_touch_whose_press_it_delivered_fr19() {
    let timers = ManualClock::new();
    let mut options = PressTrackerOptions::new(|| Some(BOUNDS));
    options.keeps_touch = true;
    options.highlight_delay = PressHighlightDelay::Always;
    let tracker = PressTracker::new(options, timers.clone());
    let h = create_harness();
    h.finger(1)
        .down(150., 120., 0., on(&[]).consumer(tracker.consumer()));
    assert_eq!(tracker.keeps_touch(1), Some(false));
    h.finger(1).up(DELAY / 2.);
    assert_eq!(tracker.keeps_touch(1), Some(false));
    timers.set_time(DELAY);
    h.finger(2)
        .down(150., 120., DELAY, on(&[]).consumer(tracker.consumer()));
    assert_eq!(tracker.keeps_touch(2), Some(false));
    timers.advance_by(DELAY);
    assert_eq!(tracker.keeps_touch(2), Some(true));
    h.finger(2).up(DELAY * 3.);
    assert_eq!(tracker.keeps_touch(2), Some(false));
}

#[test]
fn highlights_in_the_same_frame_outside_a_scroller_and_acts_on_touch_up_inside() {
    let s = setup();
    s.h.finger(1).down(150., 120., 0., consumer(&s));
    assert_eq!(*s.highlights.borrow(), [true]);
    s.h.finger(1).move_to(152., 121., 16.);
    s.h.finger(1).up(80.);
    assert_eq!(
        *s.events.borrow(),
        [TouchDown, TouchDragInside, TouchUpInside]
    );
    assert_eq!(*s.highlights.borrow(), [true, false]);
}

#[test]
fn brakes_only_a_finger_a_mouse_or_pen_press_right_after_a_finger_driven_scroll_still_presses_fr11()
{
    for kind in [PointerKind::Mouse, PointerKind::Pen] {
        let s = setup_with(|mut o| {
            o.is_braking = Some(Rc::new(|_: &GestureTouch| true));
            o
        });
        s.h.finger(1)
            .down(150., 120., 0., consumer(&s).pointer(kind));
        s.h.finger(1).up(80.);
        assert_eq!(*s.events.borrow(), [TouchDown, TouchUpInside]);
    }
    let s = setup_with(|mut o| {
        o.is_braking = Some(Rc::new(|_: &GestureTouch| true));
        o
    });
    s.h.finger(1).down(150., 120., 0., consumer(&s));
    s.h.finger(1).up(80.);
    assert!(s.events.borrow().is_empty());
}

#[test]
fn keeps_tracking_inside_the_expanded_bounds_and_exits_past_them() {
    let s = setup();
    let finger = s.h.finger(1).down(150., 120., 0., consumer(&s));
    let finger = finger.move_to(200. + EXPANSION, 120., 16.);
    assert!(s.tracker.is_touch_inside());
    let finger = finger.move_to(200. + EXPANSION + 1., 120., 32.);
    assert!(!s.tracker.is_touch_inside());
    let finger = finger.move_to(150., 120., 48.);
    finger.up_at(64., 200. + EXPANSION + 10., 120.);
    assert_eq!(
        *s.events.borrow(),
        [
            TouchDown,
            TouchDragInside,
            TouchDragExit,
            TouchDragOutside,
            TouchDragEnter,
            TouchDragInside,
            TouchUpOutside,
        ]
    );
    assert_eq!(*s.highlights.borrow(), [true, false, true, false]);
}

#[test]
fn counts_the_content_touch_delay_from_the_touchs_own_time_so_a_touch_delivered_late_still_highlights_on_time()
 {
    let s = setup();
    let probe = scroll_probe(|_, _| true, true);
    s.timers.advance_by(1000.);
    // A busy main thread delivers the touch 60 ms after it happened.
    s.h.finger(1)
        .down(150., 120., s.timers.now() - 60., consumer(&s).scroll(probe));
    s.timers.advance_by(DELAY - 60. - 1.);
    assert!(s.highlights.borrow().is_empty());
    s.timers.advance_by(1.);
    assert_eq!(*s.highlights.borrow(), [true]);
}

#[test]
fn waits_the_content_touch_delay_inside_a_scroller_and_flashes_a_quicker_tap() {
    let s = setup();
    let probe = scroll_probe(|_, _| true, true);
    s.h.finger(1)
        .down(150., 120., 0., consumer(&s).scroll(probe.clone()));
    assert!(s.highlights.borrow().is_empty());
    s.timers.advance_by(DELAY - 1.);
    assert!(s.highlights.borrow().is_empty());
    s.timers.advance_by(1.);
    assert_eq!(*s.highlights.borrow(), [true]);
    s.h.finger(1).up(400.);
    assert_eq!(*s.highlights.borrow(), [true, false]);

    let quick = setup();
    quick
        .h
        .finger(1)
        .down(150., 120., 0., consumer(&quick).scroll(probe));
    quick.h.finger(1).up(60.);
    assert_eq!(*quick.highlights.borrow(), [true]);
    quick.timers.advance_by(FLASH);
    assert_eq!(*quick.highlights.borrow(), [true, false]);
}

#[test]
fn never_paints_a_pending_press_whose_finger_travelled_a_flick() {
    let s = setup();
    let probe = scroll_probe(|_, _| true, true);
    s.h.finger(1)
        .down(150., 120., 0., consumer(&s).scroll(probe));
    s.h.finger(1).move_to(150., 140., 20.);
    s.timers.advance_by(DELAY * 2.);
    assert!(s.highlights.borrow().is_empty());
}

#[test]
fn is_cancelled_with_no_release_when_a_recognizer_takes_the_touch() {
    let s = setup();
    let pan = PanGestureRecognizer::new(PanOptions::named("pan"));
    s.h.finger(1)
        .down(150., 120., 0., on(&[&pan]).consumer(s.tracker.consumer()));
    s.h.finger(1).move_to(175., 120., 16.);
    assert_eq!(*s.events.borrow(), [TouchDown, TouchCancel]);
    assert_eq!(*s.highlights.borrow(), [true, false]);
}

#[test]
fn is_cancelled_when_the_browser_takes_the_touch_to_scroll() {
    let s = setup();
    s.h.finger(1).down(150., 120., 0., consumer(&s));
    s.h.finger(1).taken_by_browser(30.);
    assert_eq!(*s.events.borrow(), [TouchDown, TouchCancel]);
    assert!(!s.tracker.is_tracking());
}

#[test]
fn tracks_one_finger_at_a_time() {
    let s = setup();
    s.h.finger(1).down(150., 120., 0., consumer(&s));
    s.h.finger(2).down(160., 120., 10., consumer(&s));
    s.h.finger(2).up(20.);
    s.h.finger(1).up(30.);
    assert_eq!(*s.events.borrow(), [TouchDown, TouchUpInside]);
}

// --- the cell profile -------------------------------------------------------------

fn cell() -> Setup {
    setup_with(|o| o.with_profile(PRESS_PROFILES.cell))
}

#[test]
fn a_cell_highlights_at_once_outside_a_scroller_fr11() {
    let s = cell();
    s.h.finger(1).down(150., 120., 0., consumer(&s));
    assert_eq!(*s.highlights.borrow(), [true]);
    s.timers.advance_by(DELAY);
    assert_eq!(*s.highlights.borrow(), [true]);
    s.h.finger(1).up(DELAY + 40.);
    assert_eq!(*s.highlights.borrow(), [true, false]);
}

#[test]
fn a_cell_waits_the_content_touch_delay_inside_a_scroller() {
    let s = cell();
    s.h.finger(1).down(
        150.,
        120.,
        0.,
        consumer(&s).scroll(scroll_probe(|_, _| true, true)),
    );
    assert!(s.highlights.borrow().is_empty());
    s.timers.advance_by(DELAY);
    assert_eq!(*s.highlights.borrow(), [true]);
    s.h.finger(1).up(DELAY + 40.);
    assert_eq!(*s.highlights.borrow(), [true, false]);
}

#[test]
fn a_cell_is_cancelled_for_good_once_the_finger_travels_past_the_pan_hysteresis() {
    let s = cell();
    let finger = s.h.finger(1).down(150., 120., 0., consumer(&s));
    s.timers.advance_by(DELAY);
    let limit = PAN::HYSTERESIS;
    let finger = finger.move_to(150., 120. + limit, DELAY + 16.);
    assert!(s.tracker.is_tracking());
    let finger = finger.move_to(150., 120. + limit + 1., DELAY + 32.);
    assert!(!s.tracker.is_tracking());
    let finger = finger.move_to(150., 120., DELAY + 48.);
    finger.up(DELAY + 64.);
    assert_eq!(
        *s.events.borrow(),
        [TouchDown, TouchDragInside, TouchCancel]
    );
    assert_eq!(*s.highlights.borrow(), [true, false]);
}

#[test]
fn a_cell_shows_a_quicker_tap_at_the_lift_straight_into_the_release_with_no_flash() {
    let s = cell();
    s.h.finger(1).down(150., 120., 0., consumer(&s));
    s.h.finger(1).up(60.);
    assert_eq!(*s.events.borrow(), [TouchDown, TouchUpInside]);
    assert_eq!(*s.highlights.borrow(), [true, false]);
}

#[test]
fn keeps_the_control_profile_on_todays_defaults_with_the_button_release_clock() {
    assert_eq!(
        PRESS_PROFILES.control.highlight_delay,
        PressHighlightDelay::InScrollers
    );
    assert_eq!(PRESS_PROFILES.control.quick_tap_flash, FLASH);
    assert_eq!(PRESS_PROFILES.cell.cancel_travel, Some(PAN::HYSTERESIS));
}

// --- in the arena -----------------------------------------------------------------

#[test]
fn lets_a_control_veto_an_ancestors_single_tap_uicontrols_rule() {
    let h = create_harness();
    let mut options = PressTrackerOptions::new(|| None);
    options.view = Some(1);
    let button = PressTracker::new(options, ManualClock::new());
    let tap = TapGestureRecognizer::new(TapOptions::named("tap"));
    tap.set_view(Some(2));
    let double = TapGestureRecognizer::new(TapOptions {
        number_of_taps_required: 2,
        ..TapOptions::named("double")
    });
    double.set_view(Some(2));
    h.finger(1)
        .down(0., 0., 0., on(&[&tap, &double]).consumer(button.consumer()));
    assert_eq!(h.finger(1).up(50.), Some(ClickDisposition::Defer));
    assert_eq!(h.log()[0], "tap:failed@50");
    assert_eq!(double.state(), GestureState::Possible);
}

// --- under a fine pointer (FR11, DEC-21) ---------------------------------------------

#[test]
fn paints_a_mouse_or_a_pen_press_at_once_inside_a_scroller_too() {
    for kind in [PointerKind::Mouse, PointerKind::Pen] {
        let s = setup();
        s.h.finger(1).down(
            150.,
            120.,
            0.,
            consumer(&s)
                .pointer(kind)
                .scroll(scroll_probe(|_, _| true, true)),
        );
        assert_eq!(*s.highlights.borrow(), [true]);
        s.h.finger(1).up(80.);
        assert_eq!(*s.events.borrow(), [TouchDown, TouchUpInside]);
    }
}

#[test]
fn brakes_only_a_finger_a_mouse_press_on_a_scroller_a_flick_set_moving_still_presses() {
    let s = setup_with(|mut o| {
        o.is_braking = Some(Rc::new(|_: &GestureTouch| true));
        o
    });
    s.h.finger(1).down(150., 120., 0., consumer(&s));
    s.h.finger(1).up(80.);
    assert!(s.events.borrow().is_empty());
    s.h.finger(2)
        .down(150., 120., 400., consumer(&s).pointer(PointerKind::Mouse));
    assert_eq!(*s.highlights.borrow(), [true]);
    s.h.finger(2).up(480.);
    assert_eq!(*s.events.borrow(), [TouchDown, TouchUpInside]);
}
