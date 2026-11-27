//! Chat tests, after the specs' FR items.

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{
    InteractiveElement as _, IntoElement, Modifiers, ParentElement, ScrollDelta, ScrollEvent,
    Styled, TestAppContext, VisualTestContext, div, point, px,
};

use super::*;
use crate::segmented::tests::host;

fn settle(cx: &mut VisualTestContext) {
    for _ in 0..4 {
        cx.run_until_parked();
        cx.update(|window, _| window.refresh());
    }
    cx.run_until_parked();
}

fn messages(n: usize) -> ChatMessageList {
    let mut list = ChatMessageList::new("log");
    for i in 0..n {
        let sender = if i % 2 == 0 {
            ChatMessageSender::User
        } else {
            ChatMessageSender::Assistant
        };
        list = list.child(
            ChatMessage::new(gpui::ElementId::Name(format!("m{i}").into()), sender)
                .child(ChatMessageBubble::new().child(format!("Message {i}"))),
        );
    }
    list
}

/// A chat of `count` messages in a 400px tall box.
fn chat(cx: &mut TestAppContext, count: Rc<RefCell<usize>>) -> &mut VisualTestContext {
    let cx = host(cx, move || {
        div()
            .w(px(600.))
            .h(px(400.))
            .flex()
            .flex_col()
            .child(ChatLayout::new("chat", div().h(px(40.))).child(messages(*count.borrow())))
            .into_any_element()
    });
    settle(cx);
    cx
}

#[gpui::test]
fn the_view_starts_at_the_newest_message(cx: &mut TestAppContext) {
    let cx = chat(cx, Rc::new(RefCell::new(30)));
    let dock = cx.debug_bounds("chat-dock").unwrap();
    let area = cx.debug_bounds("chat-messages").unwrap();
    assert!(
        (area.bottom() - dock.top()).abs() < px(2.),
        "scrolled to the end: {area:?} {dock:?}"
    );
    assert!(area.top() < px(0.), "the start is scrolled away");
}

#[gpui::test]
fn new_content_is_followed_while_locked(cx: &mut TestAppContext) {
    let count = Rc::new(RefCell::new(30));
    let cx = chat(cx, count.clone());
    *count.borrow_mut() = 40;
    for _ in 0..120 {
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(16));
        settle(cx);
    }
    let dock = cx.debug_bounds("chat-dock").unwrap();
    let area = cx.debug_bounds("chat-messages").unwrap();
    assert!(
        (area.bottom() - dock.top()).abs() < px(2.),
        "followed to the end"
    );
}

#[gpui::test]
fn scrolling_up_unlocks_and_shows_the_button_and_new_messages(cx: &mut TestAppContext) {
    let count = Rc::new(RefCell::new(30));
    let cx = chat(cx, count.clone());
    cx.simulate_event(ScrollEvent {
        position: point(px(300.), px(100.)),
        delta: ScrollDelta::Pixels(point(px(0.), px(600.))),
        ..Default::default()
    });
    settle(cx);
    let button = cx.debug_bounds("chat-scroll-button").unwrap();
    assert!(button.size.width > px(0.));
    let before = cx.debug_bounds("chat-messages").unwrap();
    *count.borrow_mut() = 32;
    for _ in 0..10 {
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(16));
        settle(cx);
    }
    let after = cx.debug_bounds("chat-messages").unwrap();
    assert_eq!(before.top(), after.top(), "unlocked: the view stays put");
    // The button widens to its label for the new messages.
    let labelled = cx.debug_bounds("chat-scroll-button").unwrap();
    assert!(
        labelled.size.width > button.size.width,
        "{labelled:?} {button:?}"
    );
    // Pressing it returns to the bottom and follows again.
    cx.simulate_click(labelled.center(), Modifiers::none());
    for _ in 0..120 {
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(16));
        settle(cx);
    }
    let dock = cx.debug_bounds("chat-dock").unwrap();
    let area = cx.debug_bounds("chat-messages").unwrap();
    assert!(
        (area.bottom() - dock.top()).abs() < px(2.),
        "back at the end"
    );
}

#[gpui::test]
fn metadata_renders_nothing_without_content(cx: &mut TestAppContext) {
    let cx = host(cx, || {
        div()
            .child(
                div()
                    .debug_selector(|| "empty".into())
                    .child(ChatMessageMetadata::new()),
            )
            .child(
                div().debug_selector(|| "full".into()).child(
                    ChatMessageMetadata::new()
                        .timestamp("now")
                        .status(ChatMessageStatus::Sent),
                ),
            )
            .into_any_element()
    });
    settle(cx);
    assert_eq!(cx.debug_bounds("empty").unwrap().size.height, px(0.));
    assert!(cx.debug_bounds("full").unwrap().size.height > px(0.));
}

#[gpui::test]
fn user_messages_sit_at_the_inline_end(cx: &mut TestAppContext) {
    let cx = host(cx, || {
        div()
            .w(px(600.))
            .child(
                ChatMessageList::new("log")
                    .child(
                        div().debug_selector(|| "user".into()).child(
                            ChatMessage::new("u", ChatMessageSender::User).child(
                                ChatMessageBubble::new()
                                    .child(div().debug_selector(|| "u-text".into()).child("Hi")),
                            ),
                        ),
                    )
                    .child(
                        div().debug_selector(|| "assistant".into()).child(
                            ChatMessage::new("a", ChatMessageSender::Assistant)
                                .child(ChatMessageBubble::new().child(
                                    div().debug_selector(|| "a-text".into()).child("Hello"),
                                )),
                        ),
                    ),
            )
            .into_any_element()
    });
    settle(cx);
    let user = cx.debug_bounds("u-text").unwrap();
    let assistant = cx.debug_bounds("a-text").unwrap();
    assert!(user.left() > px(400.), "user at the end: {user:?}");
    assert!(
        assistant.left() < px(100.),
        "assistant at the start: {assistant:?}"
    );
}

#[gpui::test]
fn fr4_fr5_the_list_shows_its_empty_state_and_rests_at_its_end(cx: &mut TestAppContext) {
    let cx = host(cx, || {
        div()
            .h(px(300.))
            .flex()
            .flex_col()
            .child(
                ChatMessageList::new("empty").empty_state(
                    div()
                        .debug_selector(|| "empty-state".into())
                        .child("Say hello"),
                ),
            )
            .child(
                div()
                    .debug_selector(|| "short-box".into())
                    .h(px(200.))
                    .flex()
                    .flex_col()
                    .child(
                        ChatMessageList::new("short").child(
                            div()
                                .debug_selector(|| "short-msg".into())
                                .child(ChatSystemMessage::new("Only one")),
                        ),
                    ),
            )
            .into_any_element()
    });
    settle(cx);
    assert!(cx.debug_bounds("empty-state").is_some());
    let msg = cx.debug_bounds("short-msg").unwrap();
    let frame = cx.debug_bounds("short-box").unwrap();
    assert!(
        frame.bottom() - msg.bottom() < px(20.),
        "a short list rests at its end: {msg:?} {frame:?}"
    );
    assert!(msg.top() - frame.top() > px(100.));
}

#[gpui::test]
fn fr6_the_top_of_the_list_loads_older_messages(cx: &mut TestAppContext) {
    let loads = Rc::new(RefCell::new(0));
    let cx = host(cx, {
        let loads = loads.clone();
        move || {
            let loads = loads.clone();
            div()
                .h(px(300.))
                .flex()
                .flex_col()
                .child(
                    ChatMessageList::new("log")
                        .align(ChatListAlign::Top)
                        .scroll_to_top_action(move |_, cx| {
                            *loads.borrow_mut() += 1;
                            cx.background_executor()
                                .timer(std::time::Duration::from_millis(50))
                        })
                        .child(ChatSystemMessage::new("Hi")),
                )
                .into_any_element()
        }
    });
    settle(cx);
    assert_eq!(*loads.borrow(), 1, "once, as the top comes into view");
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(100));
    settle(cx);
    assert_eq!(*loads.borrow(), 1, "not again while it stays in view");
}

#[gpui::test]
fn fr6_a_bubble_is_capped_at_max_80_percent_280px(cx: &mut TestAppContext) {
    let long = "word ".repeat(80);
    let cx = host(cx, move || {
        div()
            .w(px(600.))
            .child(
                ChatMessage::new("m", ChatMessageSender::Assistant).child(
                    div()
                        .debug_selector(|| "bubble".into())
                        .child(ChatMessageBubble::new().child(long.clone())),
                ),
            )
            .child(
                div().w(px(300.)).child(
                    ChatMessage::new("n", ChatMessageSender::Assistant).child(
                        div()
                            .debug_selector(|| "narrow".into())
                            .child(ChatMessageBubble::new().child("word ".repeat(80))),
                    ),
                ),
            )
            .into_any_element()
    });
    settle(cx);
    let wide = cx.debug_bounds("bubble").unwrap();
    assert!(wide.size.width <= px(600.));
    let narrow = cx.debug_bounds("narrow").unwrap();
    assert!(narrow.size.width <= px(300.));
}

#[gpui::test]
fn fr2_a_divider_system_message_draws_its_label_on_a_rule(cx: &mut TestAppContext) {
    let cx =
        host(cx, || {
            div()
                .w(px(400.))
                .child(div().debug_selector(|| "divider".into()).child(
                    ChatSystemMessage::new("Today").variant(ChatSystemMessageVariant::Divider),
                ))
                .into_any_element()
        });
    settle(cx);
    assert_eq!(
        cx.debug_bounds("divider").unwrap().size.width,
        px(400.),
        "the full width"
    );
}

/// Records whether it renders inside a live container.
#[derive(IntoElement)]
struct LiveProbe(Rc<RefCell<Option<bool>>>);

impl gpui::RenderOnce for LiveProbe {
    fn render(self, _: &mut gpui::Window, cx: &mut gpui::App) -> impl IntoElement {
        *self.0.borrow_mut() = Some(astryx_primitives::in_live_container(cx));
        div()
    }
}

#[gpui::test]
fn the_log_is_a_live_container_for_what_renders_inside_it(cx: &mut TestAppContext) {
    let (inside, outside) = (Rc::new(RefCell::new(None)), Rc::new(RefCell::new(None)));
    let (i, o) = (inside.clone(), outside.clone());
    let cx = host(cx, move || {
        div()
            .child(ChatMessageList::new("log").child(LiveProbe(i.clone())))
            .child(LiveProbe(o.clone()))
            .into_any_element()
    });
    settle(cx);
    assert_eq!(*inside.borrow(), Some(true));
    assert_eq!(*outside.borrow(), Some(false));
}
