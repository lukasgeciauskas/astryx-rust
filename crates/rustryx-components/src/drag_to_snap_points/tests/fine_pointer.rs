//! Upstream `createDragToSnapPoints.finePointer.test.ts` (spec:AST-094 FR43): where the
//! primary pointer is fine, a mouse drag moves the element and settles it on a snap
//! point, as a finger's does. GPUI's test platform has no pen; the pen's path is the
//! pan's, which treats a pen as it treats a mouse (`gestures/tests/pan.rs`).

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{IntoElement, TestAppContext};
use rustryx_primitives::gestures::host::gesture_host;

use super::harness::*;
use crate::drag_to_snap_points::{
    DragToSnapPoints, DragToSnapPointsOptions, PanAxis, SnapElement, create_drag_to_snap_points,
};

#[gpui::test]
fn a_mouse_drag_moves_the_element_and_settles_it_on_the_nearest_snap_point(
    cx: &mut TestAppContext,
) {
    let settled: Rc<RefCell<Vec<usize>>> = Rc::default();
    let slot: Rc<RefCell<Option<DragToSnapPoints>>> = Rc::default();
    let element = SnapElement::new();
    let (s, made, e) = (settled.clone(), slot.clone(), element.clone());
    let cx = host(cx, move |window, cx| {
        let drag = made
            .borrow_mut()
            .get_or_insert_with(|| {
                let s = s.clone();
                create_drag_to_snap_points(
                    &e,
                    &gesture_host(window, cx),
                    DragToSnapPointsOptions::new(PanAxis::Y, [0., 240., 480.], move |i, _, _| {
                        s.borrow_mut().push(i)
                    }),
                    cx,
                )
            })
            .clone();
        drag.area(target("sheet"), window, cx).into_any_element()
    });
    assert!(!cx.update(|window, cx| rustryx_primitives::coarse_primary_pointer(window, cx)));
    // 300 px over a second: slow, nearer 240 than 480.
    mouse_drag_y(cx, 300., 20, 50);
    assert!(element.translate().is_some(), "the mouse moved it");
    frames(cx, 3000);
    assert_eq!(*settled.borrow(), [1]);
}
