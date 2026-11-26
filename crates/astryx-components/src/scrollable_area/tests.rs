//! ScrollableArea tests, numbered after `ScrollableArea.spec.md` and AST-025.

use std::cell::Cell;
use std::rc::Rc;

use gpui::{
    AnyElement, Bounds, InteractiveElement, IntoElement, Modifiers, ParentElement, Pixels,
    ScrollDelta, ScrollEvent, StatefulInteractiveElement, Styled, TestAppContext, TouchPhase,
    VisualTestContext, div, point, px,
};

use super::*;
use crate::layout::SpacingStep;
use crate::segmented::tests::{host, press};

fn rows(count: usize) -> Vec<AnyElement> {
    (0..count)
        .map(|i| {
            div()
                .debug_selector(move || format!("row-{i}"))
                .h(px(20.))
                .w_full()
                .into_any_element()
        })
        .collect()
}

/// A 100-tall viewport of `count` 20-tall rows (the count can change).
fn area(cx: &mut TestAppContext, count: Rc<Cell<usize>>) -> &mut VisualTestContext {
    let cx = host(cx, move || {
        ScrollableArea::new("area", "History")
            .height(px(100.))
            .width(px(200.))
            .children(rows(count.get()))
            .into_any_element()
    });
    settle(cx);
    cx
}

fn settle(cx: &mut VisualTestContext) {
    for _ in 0..3 {
        cx.run_until_parked();
        cx.update(|window, _| window.refresh());
    }
    cx.run_until_parked();
}

/// The viewport shows its keyboard focus ring.
fn focused(cx: &mut VisualTestContext, id: &'static str) -> bool {
    let selector: &'static str = Box::leak(format!("{id}-focus-ring").into_boxed_str());
    cx.debug_bounds(selector).is_some()
}

fn top_of(cx: &mut VisualTestContext, selector: &'static str) -> Pixels {
    cx.debug_bounds(selector).expect(selector).origin.y
}

fn bounds(cx: &mut VisualTestContext, selector: &'static str) -> Bounds<Pixels> {
    cx.debug_bounds(selector).expect(selector)
}

#[gpui::test]
fn ar1_a_fitting_viewport_is_no_tab_stop(cx: &mut TestAppContext) {
    let cx = area(cx, Rc::new(Cell::new(2)));
    press(cx, "tab");
    assert!(!focused(cx, "area"));
}

#[gpui::test]
fn fr12_an_overflowing_viewport_is_a_named_tab_stop_its_keys_scroll(cx: &mut TestAppContext) {
    let cx = area(cx, Rc::new(Cell::new(20)));
    let top = top_of(cx, "row-0");
    press(cx, "tab");
    assert!(focused(cx, "area"));
    press(cx, "down");
    assert_eq!(top_of(cx, "row-0"), top - px(40.));
    press(cx, "pagedown");
    assert_eq!(top_of(cx, "row-0"), top - px(40.) - px(87.5));
    press(cx, "end");
    assert_eq!(top_of(cx, "row-0"), top - px(300.), "400 tall in 100");
    press(cx, "home");
    assert_eq!(top_of(cx, "row-0"), top);
    press(cx, "space");
    assert_eq!(
        top_of(cx, "row-0"),
        top - px(87.5),
        "Space pages from the viewport"
    );
}

#[gpui::test]
fn ar2_losing_overflow_keeps_focus(cx: &mut TestAppContext) {
    let count = Rc::new(Cell::new(20));
    let cx = area(cx, count.clone());
    press(cx, "tab");
    assert!(focused(cx, "area"));
    count.set(2);
    settle(cx);
    assert!(focused(cx, "area"), "focus stays where it was");
}

#[gpui::test]
fn fr14_contain_keeps_a_scroll_at_the_edge_and_allow_chains(cx: &mut TestAppContext) {
    let contain = Rc::new(Cell::new(true));
    let flag = contain.clone();
    let cx = host(cx, move || {
        let overscroll = if flag.get() {
            ScrollOverscroll::Contain
        } else {
            ScrollOverscroll::Allow
        };
        div()
            .id("page")
            .h(px(300.))
            .w(px(300.))
            .overflow_y_scroll()
            .child(
                ScrollableArea::new("inner", "Inner")
                    .height(px(100.))
                    .width(px(200.))
                    .overscroll(overscroll)
                    .children(rows(10)),
            )
            .child(div().debug_selector(|| "after".to_string()).h(px(600.)))
            .into_any_element()
    });
    settle(cx);
    let wheel = |cx: &mut VisualTestContext, dy: f32| {
        cx.simulate_event(ScrollEvent {
            position: point(px(50.), px(50.)),
            delta: ScrollDelta::Pixels(point(px(0.), px(dy))),
            modifiers: Modifiers::default(),
            touch_phase: TouchPhase::Moved,
            ..Default::default()
        });
        settle(cx);
    };
    let after = top_of(cx, "after");
    wheel(cx, -500.);
    wheel(cx, -50.);
    assert_eq!(top_of(cx, "after"), after, "contained at the inner edge");
    contain.set(false);
    settle(cx);
    wheel(cx, -50.);
    assert_eq!(
        top_of(cx, "after"),
        after - px(50.),
        "allow chains to the page"
    );
}

#[gpui::test]
fn fr5_padding_is_published_and_full_bleed_reaches_the_container_edges(cx: &mut TestAppContext) {
    let cx = host(cx, || {
        ScrollableArea::new("outer", "Outer")
            .height(px(200.))
            .width(px(300.))
            .padding(SpacingStep::S4)
            .child(
                ScrollableArea::new("bleed", "Bleed")
                    .full_bleed(true)
                    .height(px(60.))
                    .children(rows(2)),
            )
            .child(
                ScrollableArea::new("inset", "Inset")
                    .height(px(60.))
                    .children(rows(2)),
            )
            .into_any_element()
    });
    settle(cx);
    let outer = bounds(cx, "outer");
    let bleed = bounds(cx, "bleed");
    let inset = bounds(cx, "inset");
    assert_eq!(bleed.left(), outer.left());
    assert_eq!(bleed.size.width, outer.size.width);
    assert_eq!(
        bleed.top(),
        outer.top(),
        "the first child bleeds into the top inset"
    );
    assert_eq!(
        inset.left(),
        outer.left() + px(16.),
        "the content box is padded"
    );
}

#[gpui::test]
fn fr2_inline_scrolling_keeps_its_content_width(cx: &mut TestAppContext) {
    let cx = host(cx, || {
        ScrollableArea::new("strip", "Strip")
            .axis(ScrollAxis::Inline)
            .width(px(200.))
            .child(div().flex().children((0..10).map(|i| {
                div()
                    .debug_selector(move || format!("cell-{i}"))
                    .w(px(60.))
                    .h(px(20.))
                    .flex_none()
            })))
            .into_any_element()
    });
    settle(cx);
    let left = bounds(cx, "cell-0").left();
    press(cx, "tab");
    assert!(focused(cx, "strip"), "600 wide in 200 scrolls");
    press(cx, "right");
    assert_eq!(bounds(cx, "cell-0").left(), left - px(40.));
    press(cx, "end");
    assert_eq!(bounds(cx, "cell-0").left(), left - px(400.));
}
