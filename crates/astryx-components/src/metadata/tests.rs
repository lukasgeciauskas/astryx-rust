//! MetadataList tests, numbered after the specs' FR items.

use std::rc::Rc;

use gpui::{
    AnyElement, Context, IntoElement, Modifiers, ParentElement, Render, Styled, TestAppContext,
    VisualTestContext, Window, div, px,
};

use super::*;

struct Host(Rc<dyn Fn() -> AnyElement>);

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().p(px(20.)).w(px(700.)).child((self.0)())
    }
}

fn host(
    cx: &mut TestAppContext,
    build: impl Fn() -> AnyElement + 'static,
) -> &mut VisualTestContext {
    cx.update(|cx| {
        astryx_theme::init(cx);
        astryx_primitives::focus::init(cx);
    });
    let (_, cx) = cx.add_window_view(move |_, _| Host(Rc::new(build)));
    cx.update(|window, _| window.activate_window());
    settle(cx);
    cx
}

fn settle(cx: &mut VisualTestContext) {
    for _ in 0..4 {
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
    }
}

fn bounds(cx: &mut VisualTestContext, selector: &str) -> Option<gpui::Bounds<gpui::Pixels>> {
    settle(cx);
    cx.debug_bounds(Box::leak(selector.to_string().into_boxed_str()))
}

fn items(n: usize) -> Vec<MetadataListItem> {
    (1..=n)
        .map(|i| {
            MetadataListItem::new(format!("Label {i}"), {
                let selector = format!("value-{i}");
                div()
                    .debug_selector(move || selector)
                    .child(format!("Value {i}"))
            })
        })
        .collect()
}

fn values(cx: &mut VisualTestContext, n: usize) -> Vec<gpui::Bounds<gpui::Pixels>> {
    (1..=n)
        .filter_map(|i| bounds(cx, &format!("value-{i}")))
        .collect()
}

#[gpui::test]
fn fr2_start_labels_put_values_in_one_column(cx: &mut TestAppContext) {
    let cx = host(cx, || {
        MetadataList::new("m").children(items(3)).into_any_element()
    });
    let v = values(cx, 3);
    assert_eq!(v.len(), 3);
    assert!(
        v.iter().all(|b| b.left() == v[0].left()),
        "one value column"
    );
    assert!(v[0].left() > px(40.), "beside the labels");
    assert!(v[1].top() > v[0].top());
}

#[gpui::test]
fn fr2_multi_fills_280px_columns(cx: &mut TestAppContext) {
    let cx = host(cx, || {
        MetadataList::new("m")
            .columns(MetadataColumns::Multi)
            .children(items(4))
            .into_any_element()
    });
    let v = values(cx, 4);
    // 700 wide (less padding): two 280px columns.
    assert_eq!(v[0].top(), v[1].top(), "two in a row");
    assert!(v[2].top() > v[0].top());
}

#[gpui::test]
fn fr2_a_count_makes_that_many_columns(cx: &mut TestAppContext) {
    let cx = host(cx, || {
        MetadataList::new("m")
            .columns(MetadataColumns::Count(3))
            .children(items(3))
            .into_any_element()
    });
    let v = values(cx, 3);
    assert!(
        v[0].top() == v[1].top() && v[1].top() == v[2].top(),
        "three in a row"
    );
}

#[gpui::test]
fn fr3_horizontal_lists_wrap_in_a_row(cx: &mut TestAppContext) {
    let cx = host(cx, || {
        MetadataList::new("m")
            .orientation(MetadataOrientation::Horizontal)
            .max_items(1)
            .children(items(2))
            .into_any_element()
    });
    let v = values(cx, 2);
    assert_eq!(v.len(), 2, "horizontal lists never collapse");
    assert_eq!(v[0].top(), v[1].top());
}

#[gpui::test]
fn fr4_show_more_and_less(cx: &mut TestAppContext) {
    let cx = host(cx, || {
        MetadataList::new("m")
            .max_items(2)
            .children(items(4))
            .into_any_element()
    });
    assert_eq!(values(cx, 4).len(), 2, "collapsed to two");
    let toggle = bounds(cx, "m-toggle").expect("a Show more toggle").center();
    cx.simulate_click(toggle, Modifiers::none());
    settle(cx);
    assert_eq!(values(cx, 4).len(), 4, "every entry");
    let toggle = bounds(cx, "m-toggle").unwrap().center();
    cx.simulate_click(toggle, Modifiers::none());
    settle(cx);
    assert_eq!(values(cx, 4).len(), 2, "collapsed again");
}
