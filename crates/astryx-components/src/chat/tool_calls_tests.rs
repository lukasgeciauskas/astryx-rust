//! ChatToolCalls tests, after its spec's FR items.

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{
    InteractiveElement as _, IntoElement, Modifiers, ParentElement, Styled, TestAppContext,
    VisualTestContext, div, px,
};

use super::*;
use crate::segmented::tests::{host, press};

fn settle(cx: &mut VisualTestContext) {
    for _ in 0..3 {
        cx.run_until_parked();
        cx.update(|window, _| window.refresh());
    }
    cx.run_until_parked();
}

fn calls() -> Vec<ChatToolCallItem> {
    vec![
        ChatToolCallItem::new("search")
            .target("astryx chat")
            .duration("1.2s"),
        ChatToolCallItem::new("read_file")
            .target("src/lib.rs")
            .key("read"),
        ChatToolCallItem::new("edit_file")
            .target("src/chat.rs")
            .additions(12)
            .deletions(3)
            .status(ChatToolCallStatus::Running)
            .key("edit"),
    ]
}

fn page(
    cx: &mut TestAppContext,
    build: impl Fn() -> ChatToolCalls + 'static,
) -> &mut VisualTestContext {
    let cx = host(cx, move || {
        div()
            .w(px(500.))
            .child(div().debug_selector(|| "root".into()).child(build()))
            .into_any_element()
    });
    settle(cx);
    cx
}

#[gpui::test]
fn fr1_no_calls_render_nothing_one_renders_directly(cx: &mut TestAppContext) {
    {
        let cx = page(cx, || ChatToolCalls::new("t", Vec::new()));
        assert_eq!(cx.debug_bounds("root").unwrap().size.height, px(0.));
    }
    let cx = page(cx, || {
        ChatToolCalls::new("t", vec![ChatToolCallItem::new("search")])
    });
    assert!(cx.debug_bounds("root").unwrap().size.height > px(0.));
    assert!(cx.debug_bounds("t-header").is_none(), "no group disclosure");
}

#[gpui::test]
fn fr1_fr3_a_group_collapses_and_expands(cx: &mut TestAppContext) {
    let cx = page(cx, || ChatToolCalls::new("t", calls()));
    let collapsed = cx.debug_bounds("root").unwrap().size.height;
    let header = cx.debug_bounds("t-header").unwrap();
    cx.simulate_click(header.center(), Modifiers::none());
    settle(cx);
    let expanded = cx.debug_bounds("root").unwrap().size.height;
    assert!(
        expanded > collapsed * 3.,
        "every call listed: {collapsed:?} {expanded:?}"
    );
}

#[gpui::test]
fn fr2_a_label_and_controlled_expansion(cx: &mut TestAppContext) {
    let changes = Rc::new(RefCell::new(Vec::new()));
    let log = changes.clone();
    let cx = page(cx, move || {
        let log = log.clone();
        ChatToolCalls::new("t", calls())
            .label("Worked on the chat")
            .expanded(true)
            .on_expanded_change(move |next, _, _| log.borrow_mut().push(next))
    });
    let open = cx.debug_bounds("root").unwrap().size.height;
    let header = cx.debug_bounds("t-header").unwrap();
    cx.simulate_click(header.center(), Modifiers::none());
    settle(cx);
    assert_eq!(*changes.borrow(), [false], "reports the request");
    assert_eq!(
        cx.debug_bounds("root").unwrap().size.height,
        open,
        "the caller decides"
    );
}

#[gpui::test]
fn fr4_a_result_detail_opens_with_pointer_and_keys(cx: &mut TestAppContext) {
    let cx = page(cx, || {
        ChatToolCalls::new(
            "t",
            vec![
                ChatToolCallItem::new("search")
                    .result_detail(|_, _| div().h(px(50.)).child("3 results").into_any_element()),
            ],
        )
    });
    let closed = cx.debug_bounds("root").unwrap().size.height;
    let row = cx.debug_bounds("t-call-0").unwrap();
    cx.simulate_click(row.center(), Modifiers::none());
    settle(cx);
    let open = cx.debug_bounds("root").unwrap().size.height;
    assert!(open >= closed + px(50.), "{closed:?} {open:?}");
    // Focused by the click, Enter closes it again.
    press(cx, "enter");
    settle(cx);
    assert_eq!(cx.debug_bounds("root").unwrap().size.height, closed);
}
