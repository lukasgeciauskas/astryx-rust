//! Upstream `createDragToSnapPoints.test.ts` (spec:AST-107 FR2, FR6), the factory
//! driven by touches through the window's gesture host: a slow release settles on
//! the nearest snap point, a flick past `flick_threshold` (1008 px/s by default) on
//! the next, and the settle starts at the release velocity; `settle_to` from a key,
//! `on_settle` only after a settle that arrives, bad snap points refused, and nothing
//! left after `destroy`.

use std::cell::RefCell;
use std::rc::Rc;

use astryx_primitives::gestures::host::gesture_host;
use astryx_primitives::motion::uikit::springs::role as springs;
use gpui::{IntoElement, TestAppContext, TouchPhase, VisualTestContext};

use super::harness::*;
use crate::drag_to_snap_points::{
    DEFAULT_FLICK_THRESHOLD, DragToSnapPoints, DragToSnapPointsOptions, PanAxis, SnapElement,
    create_drag_to_snap_points,
};

const POINTS: [f64; 3] = [0., 240., 480.];

type Settled = Rc<RefCell<Vec<usize>>>;
type Slot = Rc<RefCell<Option<DragToSnapPoints>>>;

struct Page<'a> {
    cx: &'a mut VisualTestContext,
    slot: Slot,
    settled: Settled,
    element: SnapElement,
}

impl Page<'_> {
    fn drag(&self) -> DragToSnapPoints {
        self.slot.borrow().clone().unwrap()
    }

    fn y(&self) -> Option<f64> {
        self.element.translate().map(|(_, y)| y)
    }

    /// Makes the drag again on the same element with new options.
    fn recreate(&mut self, points: Vec<f64>, flick: Option<f64>) {
        let (slot, settled, element) = (
            self.slot.clone(),
            self.settled.clone(),
            self.element.clone(),
        );
        self.cx.update(|window, cx| {
            let s = settled.clone();
            let mut options = DragToSnapPointsOptions::new(PanAxis::Y, points, move |i, _, _| {
                s.borrow_mut().push(i)
            });
            if let Some(flick) = flick {
                options = options.flick_threshold(flick);
            }
            *slot.borrow_mut() = Some(create_drag_to_snap_points(
                &element,
                &gesture_host(window, cx),
                options,
                cx,
            ));
        });
        settle(self.cx);
    }
}

fn page(cx: &mut TestAppContext, points: Vec<f64>, flick: Option<f64>) -> Page<'_> {
    let slot: Slot = Rc::default();
    let settled: Settled = Rc::default();
    let element = SnapElement::new();
    let (s, made, e) = (settled.clone(), slot.clone(), element.clone());
    let cx = host(cx, move |window, cx| {
        let drag = made
            .borrow_mut()
            .get_or_insert_with(|| {
                let s = s.clone();
                let mut options =
                    DragToSnapPointsOptions::new(PanAxis::Y, points.clone(), move |i, _, _| {
                        s.borrow_mut().push(i)
                    });
                if let Some(flick) = flick {
                    options = options.flick_threshold(flick);
                }
                create_drag_to_snap_points(&e, &gesture_host(window, cx), options, cx)
            })
            .clone();
        drag.area(target("sheet"), window, cx).into_any_element()
    });
    Page {
        cx,
        slot,
        settled,
        element,
    }
}

fn release_velocity(page: &Page) -> f64 {
    page.drag().snap().unwrap().calls()[0]
        .velocity
        .unwrap_or(0.)
}

#[gpui::test]
fn a_slow_drag_settles_on_the_nearest_snap_point_and_reports_it_once_settled(
    cx: &mut TestAppContext,
) {
    let page = page(cx, POINTS.to_vec(), None);
    // 300 px over a second: slow, nearer 240 than 480.
    drag(page.cx, PanAxis::Y, 300., 20, 50);
    assert!(page.settled.borrow().is_empty());
    frames(page.cx, 3000);
    assert_eq!(*page.settled.borrow(), [1]);
}

#[gpui::test]
fn a_quick_flick_carries_to_the_next_snap_point(cx: &mut TestAppContext) {
    let page = page(cx, POINTS.to_vec(), None);
    // 100 px in 64 ms: about 1500 px/s, past 1008, yet nearer 0 than 240.
    drag(page.cx, PanAxis::Y, 100., 4, 16);
    frames(page.cx, 3000);
    assert_eq!(page.settled.borrow().last(), Some(&1));
}

#[gpui::test]
fn flick_threshold_sets_the_speed_that_carries_a_drag_to_the_next_snap_point(
    cx: &mut TestAppContext,
) {
    // The same quick, short drag down from 0, released under the default flick speed
    // and past 600 px/s.
    let mut page = page(cx, POINTS.to_vec(), None);
    drag(page.cx, PanAxis::Y, 30., 3, 12);
    let by_default = release_velocity(&page);
    frames(page.cx, 3000);
    let settled_default = page.settled.borrow().last().copied();
    page.drag().destroy();
    page.element.set_translate((0., 0.));
    page.recreate(POINTS.to_vec(), Some(600.));
    page.settled.borrow_mut().clear();
    drag(page.cx, PanAxis::Y, 30., 3, 12);
    let at_600 = release_velocity(&page);
    frames(page.cx, 3000);
    assert!(by_default > 600., "{by_default}");
    assert!(by_default < DEFAULT_FLICK_THRESHOLD, "{by_default}");
    assert!((at_600 - by_default).abs() < 1e-6);
    // Nearer 0 and under 1008 px/s: back to 0. Past 600: on to 240.
    assert_eq!(settled_default, Some(0));
    assert_eq!(page.settled.borrow().last(), Some(&1));
}

#[gpui::test]
fn the_settle_starts_at_the_release_velocity_a_commit_and_a_spring_back_alike(
    cx: &mut TestAppContext,
) {
    // spec:AST-094 FR38.
    let page = page(cx, POINTS.to_vec(), None);
    let snap = page.drag().snap().unwrap().clone();
    // A flick down commits to the next snap point.
    drag(page.cx, PanAxis::Y, 100., 4, 16);
    let calls = snap.calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].target, 240.);
    assert_eq!(calls[0].spring, springs::SETTLE);
    assert!(calls[0].velocity.unwrap() > DEFAULT_FLICK_THRESHOLD);
    frames(page.cx, 3000);
    // From 240, down 120 px and back up 60 quickly: the spring-back to 240 starts at
    // that upward speed.
    snap.clear_calls();
    touch(page.cx, TouchPhase::Started, 10., 10.);
    for step in 1..=6 {
        wait(page.cx, 50);
        touch(page.cx, TouchPhase::Moved, 10., 10. + 20. * step as f32);
    }
    wait(page.cx, 16);
    touch(page.cx, TouchPhase::Moved, 10., 90.);
    wait(page.cx, 16);
    touch(page.cx, TouchPhase::Moved, 10., 70.);
    wait(page.cx, 4);
    touch(page.cx, TouchPhase::Ended, 10., 70.);
    let calls = snap.calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].target, 240.);
    assert_eq!(calls[0].spring, springs::SETTLE);
    assert!(calls[0].velocity.unwrap() < 0.);
}

#[gpui::test]
fn settle_to_settles_without_a_drag_the_keyboard_path(cx: &mut TestAppContext) {
    let page = page(cx, POINTS.to_vec(), None);
    let drag = page.drag();
    page.cx.update(|_, cx| drag.settle_to(2, cx));
    frames(page.cx, 3000);
    assert_eq!(*page.settled.borrow(), [2]);
    assert_eq!(page.y(), Some(480.));
}

#[gpui::test]
fn a_settle_interrupted_by_another_reports_only_the_last(cx: &mut TestAppContext) {
    let page = page(cx, POINTS.to_vec(), None);
    let drag = page.drag();
    page.cx.update(|_, cx| drag.settle_to(2, cx));
    frames(page.cx, 50);
    page.cx.update(|_, cx| drag.settle_to(1, cx));
    frames(page.cx, 3000);
    assert_eq!(*page.settled.borrow(), [1]);
}

#[gpui::test]
fn sorts_the_snap_points_before_use(cx: &mut TestAppContext) {
    let page = page(cx, vec![480., 0., 240.], None);
    // 300 px over about a second: slow, nearer 240, the second snap point.
    drag(page.cx, PanAxis::Y, 300., 20, 50);
    frames(page.cx, 3000);
    assert_eq!(*page.settled.borrow(), [1]);
}

#[gpui::test]
fn an_empty_list_does_nothing_and_warns_in_development(cx: &mut TestAppContext) {
    take_warnings();
    let page = page(cx, vec![], None);
    assert_eq!(take_warnings().len(), 1);
    drag(page.cx, PanAxis::Y, 300., 20, 50);
    let snap = page.drag();
    page.cx.update(|_, cx| snap.settle_to(0, cx));
    frames(page.cx, 3000);
    assert!(page.settled.borrow().is_empty());
    assert_eq!(page.element.translate(), None);
    snap.destroy();
}

#[gpui::test]
fn settle_to_with_an_index_outside_the_snap_points_does_nothing_and_warns(cx: &mut TestAppContext) {
    let page = page(cx, POINTS.to_vec(), None);
    take_warnings();
    let drag = page.drag();
    page.cx.update(|_, cx| {
        drag.settle_to(9, cx);
        drag.settle_to(usize::MAX, cx);
    });
    frames(page.cx, 3000);
    assert!(page.settled.borrow().is_empty());
    assert_eq!(take_warnings().len(), 2);
}

#[gpui::test]
fn a_drag_created_again_starts_where_its_predecessor_left_the_element(cx: &mut TestAppContext) {
    // spec:AST-107 FR2.
    let mut page = page(cx, POINTS.to_vec(), None);
    let first = page.drag();
    page.cx.update(|_, cx| first.settle_to(2, cx));
    frames(page.cx, 3000);
    first.destroy();
    // Created again with new snap points. A slow 30 px drag moves on from 480, so it
    // settles back on 480, not 0.
    page.recreate(vec![0., 240., 480., 720.], None);
    page.settled.borrow_mut().clear();
    drag(page.cx, PanAxis::Y, 30., 10, 60);
    let calls = page.drag().snap().unwrap().calls();
    assert_eq!(calls.last().unwrap().target, 480.);
    frames(page.cx, 3000);
    assert_eq!(*page.settled.borrow(), [2]);
}

#[gpui::test]
fn destroyed_during_a_settle_the_element_stays_where_it_was(cx: &mut TestAppContext) {
    // spec:AST-107 FR6: not where the settle began.
    let page = page(cx, POINTS.to_vec(), None);
    let drag = page.drag();
    page.cx.update(|_, cx| drag.settle_to(2, cx));
    frames(page.cx, 150);
    drag.destroy();
    frames(page.cx, 100);
    let at = page.y().unwrap();
    // Partway to 480, held there: neither back at 0 nor jumped to 480.
    assert!(at > 0. && at < 480., "{at}");
}

#[gpui::test]
fn destroyed_mid_drag_the_finger_moves_nothing_and_nothing_reports(cx: &mut TestAppContext) {
    // spec:AST-107 FR6.
    let page = page(cx, POINTS.to_vec(), None);
    touch(page.cx, TouchPhase::Started, 10., 10.);
    for step in 1..=4 {
        wait(page.cx, 16);
        touch(page.cx, TouchPhase::Moved, 10., 10. + step as f32 * 20.);
    }
    let during = page.element.translate();
    assert!(during.is_some());
    page.drag().destroy();
    for step in 5..=8 {
        wait(page.cx, 16);
        touch(page.cx, TouchPhase::Moved, 10., 10. + step as f32 * 20.);
    }
    wait(page.cx, 4);
    touch(page.cx, TouchPhase::Ended, 10., 170.);
    frames(page.cx, 3000);
    assert_eq!(page.element.translate(), during);
    assert!(page.settled.borrow().is_empty());
}

#[gpui::test]
fn destroy_removes_every_listener_it_added(cx: &mut TestAppContext) {
    // spec:AST-107 FR6: the pan it attached is detached, with no recognizer left.
    let page = page(cx, POINTS.to_vec(), None);
    let drag = page.drag();
    let pan = drag.pan().unwrap().clone();
    assert!(!pan.is_destroyed());
    drag.destroy();
    assert!(pan.is_destroyed());
    assert!(pan.attachment().recognizers().is_empty());
}

#[gpui::test]
fn after_destroy_no_drag_moves_the_element_and_nothing_reports(cx: &mut TestAppContext) {
    let page = page(cx, POINTS.to_vec(), None);
    let snap = page.drag();
    snap.destroy();
    drag(page.cx, PanAxis::Y, 300., 20, 50);
    page.cx.update(|_, cx| snap.settle_to(2, cx));
    frames(page.cx, 3000);
    assert!(page.settled.borrow().is_empty());
    assert_eq!(page.element.translate(), None);
}
