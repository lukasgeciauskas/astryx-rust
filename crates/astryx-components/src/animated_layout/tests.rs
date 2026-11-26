//! Animated layout tests: the glide reaches content after a reported change (FR3), nothing
//! changes without a handle (FR2), Reduce Motion with a mouse jumps (FR4), and the edge
//! reveal and a reversal (FR9).

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use gpui::{
    InteractiveElement as _, IntoElement, ParentElement, Pixels, StatefulInteractiveElement as _,
    Styled, TestAppContext, VisualTestContext, div, px,
};

use super::*;
use crate::segmented::tests::host;

fn settle(cx: &mut VisualTestContext) {
    for _ in 0..3 {
        cx.run_until_parked();
        cx.update(|window, _| window.refresh());
    }
    cx.run_until_parked();
}

fn frames(cx: &mut VisualTestContext, ms: u64) {
    for _ in 0..(ms / 16) {
        cx.executor().advance_clock(Duration::from_millis(16));
        settle(cx);
    }
}

#[derive(Clone)]
struct Setup {
    open: Rc<RefCell<bool>>,
    with_handle: bool,
}

#[derive(IntoElement)]
struct Page(Setup);

impl gpui::RenderOnce for Page {
    fn render(self, window: &mut gpui::Window, cx: &mut gpui::App) -> impl IntoElement {
        let layout = use_animated_layout("area", window, cx);
        let open = *self.0.open.borrow();
        let reveal = use_edge_reveal(
            "body",
            open,
            self.0.with_handle.then_some(&layout),
            window,
            cx,
        );
        let area = div()
            .flex()
            .flex_col()
            .child(div().h(px(20.)))
            .children(reveal.wrap(div().debug_selector(|| "body".into()).h(px(100.))))
            .child(div().debug_selector(|| "after".into()).h(px(10.)));
        layout.container(area, window, cx)
    }
}

fn page(cx: &mut TestAppContext, setup: Setup) -> &mut VisualTestContext {
    let cx = host(cx, move || Page(setup.clone()).into_any_element());
    settle(cx);
    cx
}

fn top(cx: &mut VisualTestContext, selector: &'static str) -> Pixels {
    cx.debug_bounds(selector).unwrap().top()
}

#[gpui::test]
fn content_after_a_reveal_follows_its_edge(cx: &mut TestAppContext) {
    let open = Rc::new(RefCell::new(false));
    let cx = page(
        cx,
        Setup {
            open: open.clone(),
            with_handle: true,
        },
    );
    let base = top(cx, "after");
    *open.borrow_mut() = true;
    settle(cx);
    frames(cx, 96);
    let mid = top(cx, "after") - base;
    assert!(
        mid > px(1.) && mid < px(99.),
        "moving with the edge: {mid:?}"
    );
    frames(cx, 600);
    assert_eq!(top(cx, "after") - base, px(100.));
    assert_eq!(top(cx, "body"), base);

    // A reversal mid-way starts from what it shows.
    *open.borrow_mut() = false;
    settle(cx);
    frames(cx, 64);
    let closing = top(cx, "after") - base;
    assert!(
        closing > px(1.) && closing < px(100.),
        "concealing: {closing:?}"
    );
    assert!(cx.debug_bounds("body").is_some(), "held while it conceals");
    frames(cx, 600);
    assert_eq!(top(cx, "after"), base);
    assert!(cx.debug_bounds("body").is_none(), "gone at rest");
}

#[gpui::test]
fn without_a_handle_the_change_jumps(cx: &mut TestAppContext) {
    let open = Rc::new(RefCell::new(false));
    let cx = page(
        cx,
        Setup {
            open: open.clone(),
            with_handle: false,
        },
    );
    let base = top(cx, "after");
    *open.borrow_mut() = true;
    settle(cx);
    assert_eq!(top(cx, "after") - base, px(100.));
}

#[gpui::test]
fn reduced_motion_with_a_mouse_jumps(cx: &mut TestAppContext) {
    cx.update(|cx| astryx_primitives::set_reduced_motion(true, cx));
    let open = Rc::new(RefCell::new(false));
    let cx = page(
        cx,
        Setup {
            open: open.clone(),
            with_handle: true,
        },
    );
    let base = top(cx, "after");
    *open.borrow_mut() = true;
    settle(cx);
    assert_eq!(top(cx, "after") - base, px(100.));
}

#[derive(IntoElement)]
struct Insert(Rc<RefCell<bool>>);

impl gpui::RenderOnce for Insert {
    fn render(self, window: &mut gpui::Window, cx: &mut gpui::App) -> impl IntoElement {
        let layout = use_animated_layout("list", window, cx);
        let shown = *self.0.borrow();
        let report = layout.clone();
        let flag = self.0.clone();
        let area = div()
            .w(px(200.))
            .flex()
            .flex_col()
            .child(
                div()
                    .id("add")
                    .debug_selector(|| "add".into())
                    .h(px(20.))
                    .on_click(move |_, window, cx| {
                        report.report_change(window, cx);
                        *flag.borrow_mut() = true;
                    }),
            )
            .when_some(shown.then_some(()), |el, _| el.child(div().h(px(40.))))
            .child(div().debug_selector(|| "row".into()).h(px(10.)));
        layout.container(area, window, cx)
    }
}

use gpui::prelude::FluentBuilder as _;

#[gpui::test]
fn a_reported_insertion_glides_what_follows(cx: &mut TestAppContext) {
    let shown = Rc::new(RefCell::new(false));
    let flag = shown.clone();
    let cx = host(cx, move || Insert(flag.clone()).into_any_element());
    settle(cx);
    let base = top(cx, "row");
    let add = cx.debug_bounds("add").unwrap().center();
    cx.simulate_click(add, gpui::Modifiers::none());
    settle(cx);
    assert_eq!(top(cx, "row"), base, "starts where it was");
    frames(cx, 160);
    let mid = top(cx, "row") - base;
    assert!(mid > px(1.) && mid < px(40.), "gliding: {mid:?}");
    frames(cx, 600);
    assert_eq!(top(cx, "row") - base, px(40.));
}
