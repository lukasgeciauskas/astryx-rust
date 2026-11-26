//! Upstream `hooks/useDragToSnapPoints.test.tsx` (spec:AST-107 FR2, FR6): `settle_to`
//! from a key settles and calls the latest `on_settle`, and is the same each frame; an
//! unmount stops reporting; an empty list stays empty; an option change creates the
//! drag again and leaves the element where it is, a settle in flight included. Left
//! out: the render React throws away (GPUI has no discarded renders) and the source
//! check that the hook module holds no behavior (it calls the factory's own
//! constructor).

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gpui::{IntoElement, TestAppContext, VisualTestContext};

use super::harness::*;
use crate::drag_to_snap_points::{
    DragToSnapPointsOptions, PanAxis, UseDragToSnapPointsResult, use_drag_to_snap_points,
};

struct Case {
    shown: Cell<bool>,
    points: RefCell<Vec<f64>>,
    generation: Cell<u32>,
    settled: RefCell<Vec<(u32, usize)>>,
    result: RefCell<Option<UseDragToSnapPointsResult>>,
}

fn drawer(cx: &mut TestAppContext, points: Vec<f64>) -> (&mut VisualTestContext, Rc<Case>) {
    let case = Rc::new(Case {
        shown: Cell::new(true),
        points: RefCell::new(points),
        generation: Cell::new(0),
        settled: RefCell::default(),
        result: RefCell::default(),
    });
    let c = case.clone();
    let cx = host(cx, move |window, cx| {
        if !c.shown.get() {
            return gpui::div().into_any_element();
        }
        let (log, generation) = (c.clone(), c.generation.get());
        let snap = use_drag_to_snap_points(
            "drawer",
            DragToSnapPointsOptions::new(PanAxis::Y, c.points.borrow().clone(), move |i, _, _| {
                log.settled.borrow_mut().push((generation, i))
            }),
            window,
            cx,
        );
        *c.result.borrow_mut() = Some(snap.clone());
        snap.area(target("drawer"), window, cx).into_any_element()
    });
    (cx, case)
}

fn y(case: &Case) -> f64 {
    let drag = case.result.borrow().as_ref().unwrap().current();
    drag.element().translate().map_or(0., |(_, y)| y)
}

#[gpui::test]
fn settle_to_settles_from_a_key_and_calls_the_latest_on_settle_and_is_stable(
    cx: &mut TestAppContext,
) {
    let (cx, case) = drawer(cx, vec![0., 240., 480.]);
    let first = case.result.borrow().clone().unwrap();
    case.generation.set(1);
    settle(cx);
    let second = case.result.borrow().clone().unwrap();
    assert!(
        Rc::ptr_eq(&first_drag(&first), &first_drag(&second)),
        "the same settle_to"
    );
    cx.update(|_, cx| first.settle_to(2, cx));
    frames(cx, 3000);
    assert_eq!(
        *case.settled.borrow(),
        [(1, 2)],
        "only the latest on_settle"
    );
}

fn first_drag(
    result: &UseDragToSnapPointsResult,
) -> Rc<RefCell<crate::drag_to_snap_points::DragToSnapPoints>> {
    // The result holds one shared drag across frames.
    let _ = result.current();
    result_drag(result)
}

fn result_drag(
    result: &UseDragToSnapPointsResult,
) -> Rc<RefCell<crate::drag_to_snap_points::DragToSnapPoints>> {
    result.shared()
}

#[gpui::test]
fn after_an_unmount_nothing_reports(cx: &mut TestAppContext) {
    let (cx, case) = drawer(cx, vec![0., 240., 480.]);
    let snap = case.result.borrow().clone().unwrap();
    cx.update(|_, cx| snap.settle_to(2, cx));
    frames(cx, 50);
    case.shown.set(false);
    settle(cx);
    frames(cx, 3000);
    assert!(case.settled.borrow().is_empty());
}

#[gpui::test]
fn an_empty_list_stays_empty_and_settle_to_does_nothing(cx: &mut TestAppContext) {
    take_warnings();
    let (cx, case) = drawer(cx, vec![]);
    let snap = case.result.borrow().clone().unwrap();
    cx.update(|_, cx| snap.settle_to(0, cx));
    frames(cx, 3000);
    assert!(case.settled.borrow().is_empty());
    // A new frame with the same empty list makes nothing again.
    settle(cx);
    assert_eq!(take_warnings().len(), 1);
}

#[gpui::test]
fn an_option_changed_mid_use_leaves_the_element_where_it_is_and_the_next_settle_starts_there(
    cx: &mut TestAppContext,
) {
    let (cx, case) = drawer(cx, vec![0., 240., 480.]);
    let snap = case.result.borrow().clone().unwrap();
    cx.update(|_, cx| snap.settle_to(1, cx));
    frames(cx, 3000);
    assert_eq!(y(&case), 240.);
    *case.points.borrow_mut() = vec![0., 240., 480., 720.];
    settle(cx);
    assert_eq!(y(&case), 240., "it stayed");
    cx.update(|_, cx| snap.settle_to(3, cx));
    frames(cx, 32);
    let moving = y(&case);
    assert!(
        moving > 240. && moving < 720.,
        "from where it was: {moving}"
    );
    frames(cx, 3000);
    assert_eq!(case.settled.borrow().last(), Some(&(0, 3)));
}

#[gpui::test]
fn an_option_changed_during_a_settle_stops_it_where_it_is(cx: &mut TestAppContext) {
    let (cx, case) = drawer(cx, vec![0., 240., 480.]);
    let snap = case.result.borrow().clone().unwrap();
    cx.update(|_, cx| snap.settle_to(2, cx));
    frames(cx, 100);
    *case.points.borrow_mut() = vec![0., 300., 480.];
    settle(cx);
    let held = y(&case);
    assert!(held > 0. && held < 480.);
    frames(cx, 1000);
    assert_eq!(y(&case), held, "stopped where it was");
    assert!(case.settled.borrow().is_empty());
}
