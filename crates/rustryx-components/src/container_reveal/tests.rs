//! useContainerReveal tests.

use gpui::{
    InteractiveElement as _, IntoElement, Modifiers, ParentElement, Styled, TestAppContext,
    VisualTestContext, div, px,
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

#[derive(gpui::IntoElement)]
struct Row {
    options: ContainerRevealOptions,
    content: ContentRevealOptions,
    enabled: bool,
}

impl gpui::RenderOnce for Row {
    fn render(self, window: &mut gpui::Window, cx: &mut gpui::App) -> impl IntoElement {
        let reveal = super::reveal::use_container_reveal_with(
            "row",
            UseContainerRevealOptions {
                enabled: self.enabled,
            },
            self.options,
            window,
            cx,
        );
        div().p(px(20.)).child(
            reveal
                .container(self.options)
                .debug_selector(|| "row".into())
                .flex()
                .w(px(200.))
                .h(px(40.))
                .child(div().w(px(100.)).h(px(40.)))
                .child(
                    reveal
                        .content(self.content)
                        .debug_selector(|| "content".into())
                        .w(px(40.))
                        .h(px(40.)),
                ),
        )
    }
}

fn page(cx: &mut TestAppContext, row: impl Fn() -> Row + 'static) -> &mut VisualTestContext {
    let cx = host(cx, move || row().into_any_element());
    settle(cx);
    cx
}

fn hover(cx: &mut VisualTestContext) {
    let at = cx.debug_bounds("row").unwrap().center();
    cx.simulate_mouse_move(at, None, Modifiers::none());
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(400));
    settle(cx);
}

/// Hidden content leaves the layout (absolute); shown content sits in the row.
fn in_flow(cx: &mut VisualTestContext) -> bool {
    let row = cx.debug_bounds("row").unwrap();
    let content = cx.debug_bounds("content").unwrap();
    content.left() >= row.left() + px(100.)
}

#[gpui::test]
fn hover_reveals_and_leaving_hides(cx: &mut TestAppContext) {
    let cx = page(cx, || Row {
        options: Default::default(),
        content: Default::default(),
        enabled: true,
    });
    assert!(!in_flow(cx), "hidden at rest, out of the layout");
    hover(cx);
    assert!(in_flow(cx), "shown on hover");
    cx.simulate_mouse_move(gpui::point(px(1.), px(300.)), None, Modifiers::none());
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(400));
    settle(cx);
    assert!(!in_flow(cx), "hidden again");
}

#[gpui::test]
fn layout_preserved_content_keeps_its_box(cx: &mut TestAppContext) {
    let cx = page(cx, || Row {
        options: Default::default(),
        content: ContentRevealOptions {
            layout_preserved: true,
            ..Default::default()
        },
        enabled: true,
    });
    assert!(in_flow(cx));
}

#[gpui::test]
fn forced_states_and_disabled(cx: &mut TestAppContext) {
    {
        let cx = page(cx, || Row {
            options: ContainerRevealOptions {
                force_state: Some(RevealState::Active),
                ..Default::default()
            },
            content: Default::default(),
            enabled: true,
        });
        assert!(in_flow(cx), "forced active");
    }
    {
        let cx = page(cx, || Row {
            options: ContainerRevealOptions {
                force_state: Some(RevealState::Inactive),
                ..Default::default()
            },
            content: Default::default(),
            enabled: true,
        });
        hover(cx);
        assert!(!in_flow(cx), "hover does not reveal when inactive");
    }
    let cx = page(cx, || Row {
        options: Default::default(),
        content: Default::default(),
        enabled: false,
    });
    assert!(in_flow(cx), "disabled: always shown");
}

#[gpui::test]
fn a_hover_delay_waits(cx: &mut TestAppContext) {
    let cx = page(cx, || Row {
        options: ContainerRevealOptions {
            hover_delay: std::time::Duration::from_millis(1000),
            ..Default::default()
        },
        content: Default::default(),
        enabled: true,
    });
    hover(cx);
    assert!(!in_flow(cx), "still waiting after 400ms");
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(800));
    settle(cx);
    // The fade starts once the delay has passed.
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(400));
    settle(cx);
    assert!(in_flow(cx));
}
