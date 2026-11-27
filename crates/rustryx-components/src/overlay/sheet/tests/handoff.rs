//! Upstream `useSheetGestures.test.ts`'s body suites (FR18, FR27): a finger's pull on
//! the body is the sheet's once the content cannot scroll that way, at the top, at the
//! end, and when a list runs out mid-pull; content that takes the touch itself keeps
//! it; a second finger is a pinch. Through raw touches, the window's touch scroll and
//! a mouse.

use std::cell::Cell;
use std::rc::Rc;

use gpui::{
    DispatchPhase, InteractiveElement as _, IntoElement, ParentElement, Styled, TestAppContext,
    TouchDragEvent, TouchPhase, VisualTestContext, div, point, px,
};

use super::harness::*;
use crate::overlay::{BottomSheet, SheetHeight};

/// A sheet whose content is `rows` rows of 40 px, opened.
fn with_rows(
    cx: &mut TestAppContext,
    rows: usize,
    height: SheetHeight,
) -> (&mut VisualTestContext, Rc<Cell<bool>>) {
    let flag = flag();
    let f = flag.clone();
    let cx = page(cx, move |_, _| {
        BottomSheet::new("s", f.get(), "List")
            .height(height)
            .on_open_change(writer(&f))
            .child(
                div()
                    .debug_selector(|| "content".into())
                    .children((0..rows).map(|i| div().h(px(40.)).child(format!("Row {i}")))),
            )
            .into_any_element()
    });
    open(cx, &flag);
    (cx, flag)
}

fn body_point(cx: &mut VisualTestContext) -> gpui::Point<gpui::Pixels> {
    let content = cx.debug_bounds("content").unwrap();
    point(content.center().x, content.top() + px(60.))
}

#[gpui::test]
fn fr18_a_finger_pull_down_on_the_body_at_its_top_drags_the_sheet(cx: &mut TestAppContext) {
    let (cx, flag) = with_rows(cx, 3, SheetHeight::Capped);
    let at = body_point(cx);
    finger(cx, at, 120., 6, 40, false);
    let held = sheet("s").offset;
    assert!(held > 60., "the pull moved the sheet: {held}");
    wait(cx, 300);
    touch(cx, 1, TouchPhase::Ended, point(at.x, at.y + px(120.)));
    frames(cx, 1500);
    assert!(flag.get(), "a slow pull short of halfway springs back");
    assert!(sheet("s").offset.abs() < 0.5);
}

#[gpui::test]
fn fr18_a_flick_down_on_the_body_closes_an_info_sheet(cx: &mut TestAppContext) {
    let (cx, flag) = with_rows(cx, 3, SheetHeight::Capped);
    let at = body_point(cx);
    finger(cx, at, 120., 4, 8, true);
    assert!(!flag.get());
}

#[gpui::test]
fn fr18_a_pull_down_scrolls_content_that_is_scrolled_down_before_it_moves_the_sheet(
    cx: &mut TestAppContext,
) {
    let (cx, _) = with_rows(cx, 60, SheetHeight::Capped);
    let at = body_point(cx);
    // Scroll the list down first.
    finger(cx, point(at.x, at.y + px(200.)), -200., 8, 16, true);
    frames(cx, 1500);
    assert!(sheet("s").offset.abs() < 0.5, "a scroll, not a drag");
    // A short pull down scrolls the list back; the sheet stays.
    finger(cx, at, 60., 6, 40, false);
    assert!(sheet("s").offset.abs() < 0.5, "{}", sheet("s").offset);
    touch(cx, 1, TouchPhase::Ended, point(at.x, at.y + px(60.)));
    settle(cx);
}

#[gpui::test]
fn fr18_a_list_that_runs_out_mid_pull_holds_still_while_the_sheet_stretches(
    cx: &mut TestAppContext,
) {
    let (cx, _) = with_rows(cx, 20, SheetHeight::Capped);
    let at = body_point(cx);
    let from = point(at.x, at.y + px(300.));
    // One long pull up: the list scrolls to its end, then the sheet takes the rest.
    touch(cx, 1, TouchPhase::Started, from);
    let mut stretched = 0.;
    for step in 1..=40 {
        wait(cx, 16);
        touch(
            cx,
            1,
            TouchPhase::Moved,
            point(from.x, from.y - px(20. * step as f32)),
        );
        settle(cx);
        stretched = sheet("s").offset;
        eprintln!("step {step} offset {stretched} {:?}", sheet("s").motion);
    }
    assert!(
        stretched < -5.,
        "the sheet stretched past the top: {stretched}"
    );
    // Back where the content ran out, the list has the gesture again.
    touch(cx, 1, TouchPhase::Ended, point(from.x, from.y - px(800.)));
    frames(cx, 1500);
    assert!(sheet("s").offset.abs() < 0.5, "and it springs back");
}

#[gpui::test]
fn fr27_content_that_takes_the_touch_itself_keeps_it(cx: &mut TestAppContext) {
    let flag = flag();
    let f = flag.clone();
    let cx = page(cx, move |_, _| {
        // A picker-like box that claims its own touch drags, as the wheels do.
        let claim = gpui::canvas(
            |_, _, _| {},
            |bounds, _, window, _| {
                window.on_mouse_event(move |event: &TouchDragEvent, phase, window, _| {
                    if phase == DispatchPhase::Bubble
                        && event.phase == TouchPhase::Started
                        && bounds.contains(&event.start_position)
                    {
                        window.prevent_default();
                    }
                });
            },
        )
        .absolute()
        .size_full();
        BottomSheet::new("s", f.get(), "Picker")
            .on_open_change(writer(&f))
            .child(
                div()
                    .debug_selector(|| "content".into())
                    .relative()
                    .h(px(200.))
                    .child(claim),
            )
            .into_any_element()
    });
    open(cx, &flag);
    let at = body_point(cx);
    finger(cx, at, 120., 6, 40, false);
    assert!(sheet("s").offset.abs() < 0.5, "the content kept the touch");
    touch(cx, 1, TouchPhase::Ended, point(at.x, at.y + px(120.)));
    settle(cx);
    assert!(flag.get());
}

#[gpui::test]
fn fr18_a_second_finger_is_a_pinch_and_returns_the_sheet_to_its_detent(cx: &mut TestAppContext) {
    let (cx, flag) = with_rows(cx, 3, SheetHeight::Capped);
    let handle = handle_of(cx, "content");
    finger(cx, handle, 100., 5, 30, false);
    assert!(sheet("s").offset > 50.);
    touch(
        cx,
        2,
        TouchPhase::Started,
        point(handle.x + px(80.), handle.y + px(40.)),
    );
    settle(cx);
    frames(cx, 1500);
    assert!(sheet("s").offset.abs() < 0.5, "back on its detent");
    touch(
        cx,
        2,
        TouchPhase::Ended,
        point(handle.x + px(80.), handle.y + px(40.)),
    );
    touch(
        cx,
        1,
        TouchPhase::Ended,
        point(handle.x, handle.y + px(100.)),
    );
    settle(cx);
    assert!(flag.get(), "a pinch closes nothing");
}

#[gpui::test]
fn fr18_a_mouse_pull_down_from_the_bodys_top_drags_the_sheet(cx: &mut TestAppContext) {
    let (cx, flag) = with_rows(cx, 3, SheetHeight::Capped);
    let at = body_point(cx);
    mouse_drag(cx, at, 500., 5, 16);
    assert!(!flag.get(), "pulled away, it closed");
}

/// A sheet whose content fits, holding a 200 px ScrollableArea with a long list.
fn with_nested_list(cx: &mut TestAppContext) -> (&mut VisualTestContext, Rc<Cell<bool>>) {
    let flag = flag();
    let f = flag.clone();
    let cx =
        page(cx, move |_, _| {
            BottomSheet::new("s", f.get(), "Nested")
                .on_open_change(writer(&f))
                .child(
                    div().debug_selector(|| "content".into()).child(
                        crate::scrollable_area::ScrollableArea::new("nested", "Nested list")
                            .height(px(200.))
                            .child(div().debug_selector(|| "nested".into()).children(
                                (0..40).map(|i| div().h(px(40.)).child(format!("Row {i}"))),
                            )),
                    ),
                )
                .into_any_element()
        });
    open(cx, &flag);
    (cx, flag)
}

#[gpui::test]
fn fr18_a_pull_down_on_a_nested_list_scrolled_down_scrolls_it_and_leaves_the_sheet(
    cx: &mut TestAppContext,
) {
    let (cx, _) = with_nested_list(cx);
    let nested = cx.debug_bounds("content").unwrap();
    let at = point(nested.center().x, nested.top() + px(100.));
    // Scroll the nested list down first: the body has nothing to scroll.
    finger(cx, point(at.x, at.y + px(80.)), -150., 8, 16, true);
    frames(cx, 1500);
    assert!(
        sheet("s").offset.abs() < 0.5,
        "a scroll of the list, not a drag"
    );
    // A pull down scrolls the list back toward its top; the sheet stays.
    finger(cx, at, 60., 6, 40, false);
    assert!(
        sheet("s").offset.abs() < 0.5,
        "the list, not at its top, keeps the pull: {}",
        sheet("s").offset
    );
    touch(cx, 1, TouchPhase::Ended, point(at.x, at.y + px(60.)));
    frames(cx, 1500);
}

#[gpui::test]
fn fr18_a_pull_down_on_a_nested_list_at_its_top_drags_the_sheet(cx: &mut TestAppContext) {
    let (cx, flag) = with_nested_list(cx);
    let nested = cx.debug_bounds("content").unwrap();
    let at = point(nested.center().x, nested.top() + px(60.));
    finger(cx, at, 120., 6, 40, false);
    assert!(
        sheet("s").offset > 60.,
        "every scroller under the finger is at its top: {}",
        sheet("s").offset
    );
    wait(cx, 300);
    touch(cx, 1, TouchPhase::Ended, point(at.x, at.y + px(120.)));
    frames(cx, 1500);
    assert!(flag.get());
}

#[gpui::test]
fn fr18_a_pull_up_on_a_nested_list_with_content_below_scrolls_it(cx: &mut TestAppContext) {
    let (cx, _) = with_nested_list(cx);
    let nested = cx.debug_bounds("content").unwrap();
    let at = point(nested.center().x, nested.top() + px(150.));
    // The body's content fits, but the list has more below: the pull is the list's.
    finger(cx, at, -60., 6, 40, false);
    assert!(sheet("s").offset.abs() < 0.5, "{}", sheet("s").offset);
    touch(cx, 1, TouchPhase::Ended, point(at.x, at.y - px(60.)));
    frames(cx, 1500);
}
