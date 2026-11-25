//! OverflowList tests, numbered after the spec's FR items.

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{
    AnyElement, Context, InteractiveElement, IntoElement, ParentElement, Render, Styled,
    TestAppContext, VisualTestContext, Window, div, px,
};

use super::*;

type Width = Rc<RefCell<f32>>;

struct Host(Rc<dyn Fn() -> AnyElement>, Width);

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().p(px(20.)).flex().flex_col().child(
            div()
                .w(px(*self.1.borrow()))
                .flex()
                .flex_col()
                .child((self.0)()),
        )
    }
}

fn host<'a>(
    cx: &'a mut TestAppContext,
    width: &Width,
    build: impl Fn() -> AnyElement + 'static,
) -> &'a mut VisualTestContext {
    cx.update(astryx_theme::init);
    let width = width.clone();
    let (_, cx) = cx.add_window_view(move |_, _| Host(Rc::new(build), width));
    settle(cx);
    cx
}

fn settle(cx: &mut VisualTestContext) {
    for _ in 0..6 {
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
    }
}

/// An item 50px wide.
fn item(i: usize) -> impl Fn(&mut Window, &mut App) -> AnyElement + 'static {
    move |_, _| {
        let selector = format!("item-{i}");
        div()
            .debug_selector(move || selector.clone())
            .w(px(50.))
            .h(px(20.))
            .into_any_element()
    }
}

fn list(n: usize) -> OverflowList {
    let mut list = OverflowList::new("o").gap(SpacingStep::S2);
    for i in 0..n {
        list = list.keyed_item(format!("k{i}"), item(i));
    }
    list
}

/// The items drawn in the visible row (the hidden copy has no size).
fn shown(cx: &mut VisualTestContext, n: usize) -> Vec<usize> {
    settle(cx);
    (0..n)
        .filter(|i| {
            let selector: &'static str = Box::leak(format!("item-{i}").into_boxed_str());
            cx.debug_bounds(selector)
                .is_some_and(|b| b.size.height > px(0.))
        })
        .collect()
}

fn indicator(hidden: &[OverflowItem], _: &mut Window, _: &mut App) -> AnyElement {
    let count = hidden.len();
    div()
        .debug_selector(move || format!("more-{count}"))
        .w(px(30.))
        .h(px(20.))
        .into_any_element()
}

#[gpui::test]
fn fr2_the_row_shows_what_fits(cx: &mut TestAppContext) {
    let width: Width = Rc::new(RefCell::new(300.));
    let cx = host(cx, &width, || {
        list(8).overflow_renderer(indicator).into_any_element()
    });
    // 50 + 8 + 50 + 8 + 50 + 8 + 50 = 224; with 8 + 30 for the indicator.
    let visible = shown(cx, 8);
    assert_eq!(visible, [0, 1, 2, 3], "{visible:?}");
    assert!(
        cx.debug_bounds("more-4").is_some(),
        "FR3: the indicator for four"
    );
    *width.borrow_mut() = 1000.;
    assert_eq!(shown(cx, 8).len(), 8, "it unfolds as room returns");
}

#[gpui::test]
fn fr2_collapsing_from_the_start_keeps_the_last(cx: &mut TestAppContext) {
    let width: Width = Rc::new(RefCell::new(200.));
    let cx = host(cx, &width, || {
        list(6)
            .collapse_from(CollapseFrom::Start)
            .overflow_renderer(indicator)
            .into_any_element()
    });
    let visible = shown(cx, 6);
    assert_eq!(visible.last(), Some(&5), "{visible:?}");
    assert!(!visible.contains(&0));
}

#[gpui::test]
fn fr2_the_floor_and_the_cap(cx: &mut TestAppContext) {
    let width: Width = Rc::new(RefCell::new(1000.));
    let cx = host(cx, &width, || {
        list(6)
            .max_visible_items(2)
            .overflow_renderer(indicator)
            .into_any_element()
    });
    assert_eq!(shown(cx, 6), [0, 1], "capped at two");
    let mut app = TestAppContext::single();
    let narrow: Width = Rc::new(RefCell::new(60.));
    let cx = host(&mut app, &narrow, || {
        list(6).min_visible_items(3).into_any_element()
    });
    assert_eq!(shown(cx, 6), [0, 1, 2], "at least three");
}

#[gpui::test]
fn fr4_rows_wrap_up_to_the_most(cx: &mut TestAppContext) {
    let width: Width = Rc::new(RefCell::new(130.));
    let cx = host(cx, &width, || {
        list(6)
            .max_rows(2)
            .overflow_renderer(indicator)
            .into_any_element()
    });
    let visible = shown(cx, 6);
    assert_eq!(
        visible.len(),
        3,
        "two rows, room for the indicator: {visible:?}"
    );
}

#[gpui::test]
fn fr5_changes_to_the_collapsed_set_are_reported(cx: &mut TestAppContext) {
    let width: Width = Rc::new(RefCell::new(1000.));
    let reports = Rc::new(RefCell::new(Vec::<Vec<usize>>::new()));
    let cx = host(cx, &width, {
        let reports = reports.clone();
        move || {
            let reports = reports.clone();
            list(4)
                .on_overflow_change(move |hidden, _, _| {
                    reports
                        .borrow_mut()
                        .push(hidden.iter().map(|h| h.index).collect())
                })
                .into_any_element()
        }
    });
    settle(cx);
    assert!(
        reports.borrow().is_empty(),
        "never while nothing overflowed"
    );
    *width.borrow_mut() = 120.;
    settle(cx);
    assert_eq!(reports.borrow().last().cloned(), Some(vec![2, 3]));
    let count = reports.borrow().len();
    settle(cx);
    assert_eq!(reports.borrow().len(), count, "only on a change");
    *width.borrow_mut() = 1000.;
    settle(cx);
    assert_eq!(
        reports.borrow().last().cloned(),
        Some(vec![]),
        "back to empty"
    );
}
