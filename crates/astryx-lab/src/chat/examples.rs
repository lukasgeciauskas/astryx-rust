//! The Chat lab parts' gallery (upstream's `ChatAdditions` and `ChatVirtualizer` stories):
//! reactions under a message that toggle and add, an unread divider and a typing hint in
//! a thread, the reaction emoji picker on a button, and ten thousand messages windowed in
//! a scroller with a header above them.

use std::rc::Rc;

use astryx_components::button::{Button, ButtonVariant};
use astryx_components::chat::{ChatMessage, ChatMessageBubble, ChatMessageList, ChatMessageSender};
use astryx_components::layout::{SpacingStep, Stack};
use astryx_components::text::{Heading, HeadingLevel, Text, TextColor, TextType};
use gpui::prelude::*;
use gpui::{AnyElement, App, ElementId, ScrollHandle, SharedString, Window, div, px};

use super::{
    ChatEmojiPicker, ChatReaction, ChatReactionBar, ChatTypingIndicator, ChatUnreadDivider,
    ChatVirtualizer, ChatVirtualizerHandle, ItemSize, provide_live_container,
};

fn example(title: &'static str, body: impl IntoElement) -> AnyElement {
    Stack::vertical()
        .gap(SpacingStep::S2)
        .child(Text::new(title).text_type(TextType::Large))
        .child(body)
        .into_any_element()
}

fn initial_reactions() -> Vec<ChatReaction> {
    vec![
        ChatReaction::new("🎉", 4)
            .selected(true)
            .label("You, Dana, Lee, and Mia reacted with 🎉"),
        ChatReaction::new("👀", 2).label("Dana and Lee reacted with 👀"),
    ]
}

/// Upstream's `Reactions`: a toggle flips the user's own reaction, and a pick from the
/// picker adds one (or joins an existing one).
fn reactions(window: &mut Window, cx: &mut App) -> AnyElement {
    let state = window.use_keyed_state("chat-example-reactions", cx, |_, _| initial_reactions());
    let current = state.read(cx).clone();
    let (toggle, add) = (state.clone(), state.clone());
    let bar = ChatReactionBar::new("chat-example-bar", current)
        .on_toggle(move |emoji, _, cx| {
            toggle.update(cx, |reactions, cx| {
                for r in reactions.iter_mut().filter(|r| r.emoji == emoji) {
                    r.count = if r.selected { r.count - 1 } else { r.count + 1 };
                    r.selected = !r.selected;
                }
                reactions.retain(|r| r.count > 0);
                cx.notify();
            })
        })
        .on_add(move |emoji, _, cx| {
            add.update(cx, |reactions, cx| {
                match reactions.iter_mut().find(|r| r.emoji == emoji) {
                    Some(r) if !r.selected => {
                        r.selected = true;
                        r.count += 1;
                    }
                    Some(_) => {}
                    None => reactions.push(ChatReaction::new(emoji, 1).selected(true)),
                }
                cx.notify();
            })
        });
    ChatMessageList::new("chat-example-reactions-list")
        .max_w(px(600.))
        .child(
            ChatMessage::new("chat-example-reactions-message", ChatMessageSender::Assistant)
                .child(ChatMessageBubble::new().child(
                    "The design review went great. Tokens are approved and we can start testing the new chat affordances next sprint.",
                ))
                .child(bar),
        )
        .into_any_element()
}

/// Upstream's `TypingAndUnread`: a thread with an unread divider and three people typing.
/// The transcript is a live log, which delivers the typing hint itself.
fn typing_and_unread() -> AnyElement {
    ChatMessageList::new("chat-example-thread")
        .max_w(px(600.))
        .child(
            ChatMessage::new("chat-example-user", ChatMessageSender::User).child(
                ChatMessageBubble::new().child("Sounds good. I'll take the migration notes."),
            ),
        )
        .child(ChatUnreadDivider::new("chat-example-unread"))
        .child(
            ChatMessage::new("chat-example-assistant", ChatMessageSender::Assistant).child(
                ChatMessageBubble::new().child(
                    "Perfect. I've drafted the rollout checklist and shared it with the team.",
                ),
            ),
        )
        .child(provide_live_container(
            ChatTypingIndicator::new("chat-example-typing").names(["Ana", "Ben", "Casey"]),
        ))
        .into_any_element()
}

/// Upstream's `EmojiPicker`: the picker on a button that names the last pick.
fn emoji_picker(window: &mut Window, cx: &mut App) -> AnyElement {
    let selected =
        window.use_keyed_state("chat-example-picked", cx, |_, _| SharedString::from("🎉"));
    let shown = selected.read(cx).clone();
    ChatEmojiPicker::new("chat-example-picker")
        .on_select(move |emoji, _, cx| {
            selected.update(cx, |s, cx| {
                *s = emoji;
                cx.notify();
            })
        })
        .child(
            Button::new("chat-example-pick", format!("Pick emoji {shown}"))
                .variant(ButtonVariant::Secondary),
        )
        .into_any_element()
}

/// One message of upstream's story data.
#[derive(Clone)]
struct Message {
    id: usize,
    text: SharedString,
}

const WORDS: &str = "the quick brown fox jumps over a lazy dog while the river runs past the old mill and a bell rings twice";

/// Ten thousand messages of one to six lines, the same on every load (upstream's
/// `MESSAGES`).
fn messages() -> Rc<Vec<Message>> {
    let words: Vec<&str> = WORDS.split(' ').collect();
    Rc::new(
        (0..10_000)
            .map(|id| {
                let count = 4 + (id * 37) % 60;
                let text: Vec<&str> = (0..count).map(|w| words[(id + w) % words.len()]).collect();
                Message {
                    id,
                    text: format!("Message {}: {}", id + 1, text.join(" ")).into(),
                }
            })
            .collect(),
    )
}

/// Upstream's `PageScroller`: attach mode on a scroller the gallery owns, with a header
/// above the list inside the same scroller. Anchored to the first message, it opens at the
/// top with the header and the first messages in view.
fn virtualizer(window: &mut Window, cx: &mut App) -> AnyElement {
    let data = window
        .use_keyed_state("chat-example-messages", cx, |_, _| messages())
        .read(cx)
        .clone();
    let scroller = window
        .use_keyed_state("chat-example-scroller", cx, |_, _| ScrollHandle::new())
        .read(cx)
        .clone();
    let handle = window.use_keyed_state("chat-example-handle", cx, |_, _| {
        (ChatVirtualizerHandle::new(), false)
    });
    let (list_handle, declared) = handle.read(cx).clone();
    if !declared {
        handle.update(cx, |h, _| h.1 = true);
        list_handle.anchor_to_key("0", 0., window);
    }
    let list = ChatVirtualizer::new(
        "chat-example-virtualizer",
        data,
        |m: &Message, _| m.id.to_string().into(),
        |m: &Message, _, _, _| {
            Stack::vertical()
                .padding(SpacingStep::S3)
                .child(Text::new(m.text.clone()))
                .into_any_element()
        },
    )
    .estimated_item_size(ItemSize::Fixed(48.))
    .handle(&list_handle)
    .scroll_element(Some(&scroller));
    div()
        .id(ElementId::Name("chat-example-page".into()))
        .h(px(480.))
        .w_full()
        .overflow_y_scroll()
        .track_scroll(&scroller)
        .child(
            Stack::vertical()
                .gap(SpacingStep::S2)
                .padding(SpacingStep::S4)
                .child(Heading::new(HeadingLevel::H1, "Conversation history"))
                .child(Text::new("The page scrolls. This header sits above the list, inside the same scroller.").color(TextColor::Secondary)),
        )
        .child(list)
        .into_any_element()
}

/// The Chat lab parts' examples.
pub fn examples(window: &mut Window, cx: &mut App) -> AnyElement {
    Stack::vertical()
        .gap(SpacingStep::S8)
        .padding(SpacingStep::S6)
        .w(px(640.))
        .child(example("Reactions", reactions(window, cx)))
        .child(example("Typing and unread", typing_and_unread()))
        .child(example("Emoji picker", emoji_picker(window, cx)))
        .child(example("Virtualized history", virtualizer(window, cx)))
        .into_any_element()
}
