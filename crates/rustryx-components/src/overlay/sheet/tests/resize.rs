//! Upstream `BottomSheetResizeDuringSettle.test.tsx` and the viewport-resize suite of
//! `useSheetGestures.test.ts` (FR19): a resize re-anchors the sheet to the same detent
//! in the new geometry, at rest at once, and a settle in flight keeps moving toward
//! that detent.

use gpui::{
    InteractiveElement as _, IntoElement, ParentElement, Styled, TestAppContext, div, px, size,
};

use super::harness::*;
use crate::overlay::{BottomSheet, SheetHeight, SnapPoint};

fn half(
    cx: &mut TestAppContext,
) -> (
    &mut gpui::VisualTestContext,
    std::rc::Rc<std::cell::Cell<bool>>,
) {
    let flag = flag();
    let f = flag.clone();
    let cx = page(cx, move |_, _| {
        BottomSheet::new("s", f.get(), "Half")
            .height(SheetHeight::Tall)
            .snap_points([SnapPoint::Fraction(0.5)])
            .on_open_change(writer(&f))
            .child(
                div()
                    .debug_selector(|| "content".into())
                    .h(px(80.))
                    .child("Half"),
            )
            .into_any_element()
    });
    cx.simulate_resize(size(px(800.), px(1000.)));
    settle(cx);
    open(cx, &flag);
    (cx, flag)
}

fn half_offset(_: &SheetFrame, vh: f32) -> f32 {
    // Fully open shows 92%; the detent shows half the window.
    vh * 0.92 - vh * 0.5
}

#[gpui::test]
fn a_resize_at_rest_re_anchors_to_the_same_detent(cx: &mut TestAppContext) {
    let (cx, _) = half(cx);
    let handle = handle_of(cx, "content");
    finger(cx, handle, 400., 20, 50, true);
    frames(cx, 1500);
    let s = sheet("s");
    assert!(
        (s.offset - half_offset(&s, 1000.)).abs() < 1.,
        "{}",
        s.offset
    );
    cx.simulate_resize(size(px(800.), px(700.)));
    settle(cx);
    let s = sheet("s");
    assert!(
        (s.offset - half_offset(&s, 700.)).abs() < 1.,
        "the half detent in the new window: {}",
        s.offset
    );
}

#[gpui::test]
fn a_resize_while_a_release_settles_keeps_it_moving_to_the_same_detent(cx: &mut TestAppContext) {
    let (cx, _) = half(cx);
    let handle = handle_of(cx, "content");
    finger(cx, handle, 300., 10, 30, true);
    frames(cx, 48);
    assert!(sheet("s").offset < half_offset(&sheet("s"), 1000.) + 50.);
    cx.simulate_resize(size(px(800.), px(700.)));
    settle(cx);
    frames(cx, 1500);
    let s = sheet("s");
    assert!(
        (s.offset - half_offset(&s, 700.)).abs() < 1.,
        "landed on the detent in the new window: {}",
        s.offset
    );
}
