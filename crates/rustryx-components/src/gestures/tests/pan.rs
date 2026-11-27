//! Upstream `gestures/__tests__/createPanGesture.test.tsx` (spec:AST-107 FR1, FR6): a drag
//! reported as values on each axis and as cancelled, nothing for a press or a long press,
//! one API at two layers, nothing left after destroy, and the internal pan's input.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gpui::{IntoElement, TestAppContext, TouchPhase};
use rustryx_primitives::gestures::Point;
use rustryx_primitives::gestures::host::gesture_host;

use super::harness::*;
use crate::gestures::pan::{PanConfig, PanEvent, PanInput, create_pan};
use crate::gestures::*;

type Events = Rc<RefCell<Vec<PanGestureEvent>>>;

/// The states in order, a run of changed counted once.
fn phases(events: &[PanGestureEvent]) -> Vec<&'static str> {
    let mut out: Vec<&'static str> = Vec::new();
    for e in events {
        if out.last() != Some(&e.state.as_str()) {
            out.push(e.state.as_str());
        }
    }
    out
}

/// A window with one pan made by the factory, and its events.
fn factory(
    cx: &mut TestAppContext,
    axis: Option<PanAxis>,
) -> (
    &mut gpui::VisualTestContext,
    Events,
    Rc<RefCell<Option<PanGesture>>>,
) {
    let events: Events = Rc::default();
    let pan: Rc<RefCell<Option<PanGesture>>> = Rc::default();
    let (e, p) = (events.clone(), pan.clone());
    let cx = host(cx, move |window, cx| {
        let made = p
            .borrow_mut()
            .get_or_insert_with(|| {
                let e = e.clone();
                create_pan_gesture(
                    &gesture_host(window, cx),
                    PanGestureOptions::new(move |event| e.borrow_mut().push(*event)).axis(axis),
                )
            })
            .clone();
        made.area(target("surface")).into_any_element()
    });
    (cx, events, pan)
}

#[gpui::test]
fn reports_a_horizontal_drag_as_began_changed_and_ended_values(cx: &mut TestAppContext) {
    let (cx, events, _) = factory(cx, Some(PanAxis::X));
    drag(cx, 1, 40., 0., true);
    let events = events.borrow();
    assert_eq!(phases(&events), ["began", "changed", "ended"]);
    for e in events.iter() {
        assert_eq!(e.translation.y, 0.);
        assert_eq!(e.velocity.y, 0.);
    }
    // The threshold's travel is not part of the translation.
    assert_eq!(events[0].translation.x, 0.);
    let last = events.last().unwrap();
    assert_eq!(last.translation.x, 30.);
    assert!(last.velocity.x > 0.);
}

#[gpui::test]
fn is_driven_by_a_finger_and_a_mouse_alike(cx: &mut TestAppContext) {
    let (cx, events, _) = factory(cx, Some(PanAxis::X));
    drag(cx, 1, 60., 0., true);
    assert_eq!(phases(&events.borrow()), ["began", "changed", "ended"]);
    events.borrow_mut().clear();
    mouse_drag(cx, 60.);
    assert_eq!(phases(&events.borrow()), ["began", "changed", "ended"]);
}

#[gpui::test]
fn reports_a_vertical_drag_on_the_y_axis_with_nothing_across_it(cx: &mut TestAppContext) {
    let (cx, events, _) = factory(cx, Some(PanAxis::Y));
    drag(cx, 1, 4., 40., true);
    let events = events.borrow();
    assert_eq!(phases(&events), ["began", "changed", "ended"]);
    for e in events.iter() {
        assert_eq!(e.translation.x, 0.);
        assert_eq!(e.velocity.x, 0.);
    }
    assert_eq!(events.last().unwrap().translation.y, 30.);
    assert!(events.last().unwrap().velocity.y > 0.);
}

#[gpui::test]
fn reports_nothing_for_a_drag_off_its_axis(cx: &mut TestAppContext) {
    let (cx, events, _) = factory(cx, Some(PanAxis::X));
    drag(cx, 1, 0., 40., true);
    assert!(events.borrow().is_empty());
}

#[gpui::test]
fn reports_both_axes_without_an_axis(cx: &mut TestAppContext) {
    let (cx, events, _) = factory(cx, None);
    drag(cx, 1, 40., 20., true);
    let events = events.borrow();
    let last = events.last().unwrap();
    assert_eq!(last.state, PanState::Ended);
    assert!(last.translation.x > 0. && last.translation.y > 0.);
}

#[gpui::test]
fn reports_a_cancelled_drag_as_cancelled_with_no_velocity(cx: &mut TestAppContext) {
    let (cx, events, _) = factory(cx, Some(PanAxis::X));
    drag(cx, 1, 40., 0., false);
    wait(cx, 16);
    touch(cx, 1, TouchPhase::Cancelled, 50., 10.);
    let events = events.borrow();
    assert_eq!(phases(&events), ["began", "changed", "cancelled"]);
    assert_eq!(events.last().unwrap().velocity, Point::ZERO);
}

#[gpui::test]
fn reports_nothing_for_a_press_or_a_long_press(cx: &mut TestAppContext) {
    let (cx, events, _) = factory(cx, Some(PanAxis::X));
    touch(cx, 1, TouchPhase::Started, 10., 10.);
    wait(cx, 80);
    touch(cx, 1, TouchPhase::Ended, 10., 10.);
    touch(cx, 2, TouchPhase::Started, 10., 10.);
    wait(cx, 1000);
    touch(cx, 2, TouchPhase::Ended, 11., 10.);
    assert!(events.borrow().is_empty());
}

#[gpui::test]
fn destroy_mid_drag_cancels_once_and_nothing_reaches_it_after(cx: &mut TestAppContext) {
    let (cx, events, pan) = factory(cx, Some(PanAxis::X));
    drag(cx, 1, 40., 0., false);
    let gesture = pan.borrow().clone().unwrap();
    gesture.destroy();
    assert_eq!(phases(&events.borrow()), ["began", "changed", "cancelled"]);
    assert!(gesture.is_destroyed());
    assert!(gesture.attachment().recognizers().is_empty());
    touch(cx, 1, TouchPhase::Moved, 80., 10.);
    touch(cx, 1, TouchPhase::Ended, 80., 10.);
    let reported = events.borrow().len();
    drag(cx, 3, 40., 0., true);
    assert_eq!(events.borrow().len(), reported);
    // A second destroy does nothing.
    gesture.destroy();
    assert_eq!(events.borrow().len(), reported);
}

#[gpui::test]
fn the_internal_pan_says_which_pointer_drove_each_drag(cx: &mut TestAppContext) {
    let inputs: Rc<RefCell<Vec<PanInput>>> = Rc::default();
    let made: Rc<RefCell<Option<PanGesture>>> = Rc::default();
    let (i, m) = (inputs.clone(), made.clone());
    let cx = host(cx, move |window, cx| {
        let pan = m
            .borrow_mut()
            .get_or_insert_with(|| {
                let i = i.clone();
                create_pan(
                    &gesture_host(window, cx),
                    PanConfig {
                        axis: Some(PanAxis::X),
                        on_pan: Rc::new(move |e: &PanEvent| i.borrow_mut().push(e.input)),
                        recognizer: None,
                    },
                )
            })
            .clone();
        pan.area(target("surface")).into_any_element()
    });
    drag(cx, 1, 60., 0., true);
    assert!(!inputs.borrow().is_empty());
    assert!(inputs.borrow().iter().all(|i| *i == PanInput::Touch));
    inputs.borrow_mut().clear();
    mouse_drag(cx, 60.);
    assert!(!inputs.borrow().is_empty());
    assert!(inputs.borrow().iter().all(|i| *i == PanInput::Mouse));
}

// --- usePanGesture ---------------------------------------------------------

struct HookCase {
    shown: Cell<bool>,
    axis: Cell<Option<PanAxis>>,
    id: Cell<&'static str>,
    generation: Cell<u32>,
    events: RefCell<Vec<(u32, PanGestureEvent)>>,
}

fn hook(cx: &mut TestAppContext) -> (&mut gpui::VisualTestContext, Rc<HookCase>) {
    let case = Rc::new(HookCase {
        shown: Cell::new(true),
        axis: Cell::new(Some(PanAxis::X)),
        id: Cell::new("a"),
        generation: Cell::new(0),
        events: RefCell::default(),
    });
    let c = case.clone();
    let cx = host(cx, move |window, cx| {
        if !c.shown.get() {
            return gpui::div().into_any_element();
        }
        let (log, generation) = (c.clone(), c.generation.get());
        let pan = use_pan_gesture(
            c.id.get(),
            PanGestureOptions::new(move |e| log.events.borrow_mut().push((generation, *e)))
                .axis(c.axis.get()),
            window,
            cx,
        );
        pan.area(target(c.id.get())).into_any_element()
    });
    (cx, case)
}

fn rerender(cx: &mut gpui::VisualTestContext) {
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
}

fn hook_phases(case: &HookCase) -> Vec<&'static str> {
    let events: Vec<PanGestureEvent> = case.events.borrow().iter().map(|(_, e)| *e).collect();
    phases(&events)
}

#[gpui::test]
fn the_hook_reports_what_its_factory_reports_through_the_latest_on_pan(cx: &mut TestAppContext) {
    let (cx, case) = hook(cx);
    case.generation.set(1);
    rerender(cx);
    drag(cx, 1, 40., 0., true);
    let events = case.events.borrow();
    assert!(
        events.iter().all(|(g, _)| *g == 1),
        "the first listener heard nothing"
    );
    drop(events);
    assert_eq!(hook_phases(&case), ["began", "changed", "ended"]);
    assert_eq!(
        case.events.borrow().last().unwrap().1.translation,
        Point::new(30., 0.)
    );
}

#[gpui::test]
fn a_drag_in_progress_at_unmount_ends_without_a_cancelled_event(cx: &mut TestAppContext) {
    let (cx, case) = hook(cx);
    drag(cx, 1, 40., 0., false);
    assert_eq!(hook_phases(&case), ["began", "changed"]);
    case.shown.set(false);
    rerender(cx);
    rerender(cx);
    assert_eq!(hook_phases(&case), ["began", "changed"]);
    touch(cx, 1, TouchPhase::Ended, 50., 10.);
    assert_eq!(hook_phases(&case), ["began", "changed"]);
}

#[gpui::test]
fn a_drag_in_progress_when_the_axis_changes_ends_without_a_cancelled_event(
    cx: &mut TestAppContext,
) {
    let (cx, case) = hook(cx);
    drag(cx, 1, 40., 0., false);
    assert_eq!(hook_phases(&case), ["began", "changed"]);
    case.axis.set(Some(PanAxis::Y));
    rerender(cx);
    assert_eq!(hook_phases(&case), ["began", "changed"]);
    // The new pan takes the next drag on its axis.
    touch(cx, 1, TouchPhase::Ended, 50., 10.);
    case.events.borrow_mut().clear();
    drag(cx, 2, 0., 40., true);
    assert_eq!(hook_phases(&case), ["began", "changed", "ended"]);
}

#[gpui::test]
fn the_hook_follows_the_element_when_it_moves_to_a_new_node(cx: &mut TestAppContext) {
    let (cx, case) = hook(cx);
    case.id.set("b");
    rerender(cx);
    rerender(cx);
    drag(cx, 1, 40., 0., true);
    assert_eq!(hook_phases(&case), ["began", "changed", "ended"]);
}
