//! Chat examples for the gallery.

use gpui::{
    AnyElement, App, ElementId, IntoElement, ParentElement, RenderOnce, Styled, Window, div, px,
};

use super::{
    ChatBubbleGroup, ChatDensity, ChatLayout, ChatMessage, ChatMessageBubble,
    ChatMessageBubbleVariant, ChatMessageList, ChatMessageMetadata, ChatMessageSender,
    ChatMessageStatus, ChatSystemMessage, ChatSystemMessageVariant,
};
use crate::button::Button;
use crate::layout::{SpacingStep, Stack};
use crate::markdown::Markdown;
use crate::people::Avatar;
use crate::text::{Text, TextType};

pub fn examples(_window: &mut Window, _cx: &mut App) -> AnyElement {
    let label = |text: &'static str| Text::new(text).text_type(TextType::Supporting);
    Stack::vertical()
        .gap(SpacingStep::S3)
        .child(label(
            "ChatLayout: messages follow the newest one; scroll up and add a reply to see the button",
        ))
        .child(Conversation { id: "chat-demo".into() })
        .child(label("Bubbles: grouped corners, a ghost bubble, names and metadata"))
        .child(
            div().w(px(560.)).child(
                ChatMessageList::new("chat-bubbles")
                    .density(ChatDensity::Compact)
                    .child(
                        ChatMessage::new("b1", ChatMessageSender::User)
                            .child(
                                ChatMessageBubble::new()
                                    .group(ChatBubbleGroup::First)
                                    .name("You")
                                    .child("First of three"),
                            )
                            .child(ChatMessageBubble::new().group(ChatBubbleGroup::Middle).child("Second"))
                            .child(
                                ChatMessageBubble::new()
                                    .group(ChatBubbleGroup::Last)
                                    .metadata(
                                        ChatMessageMetadata::new()
                                            .id("b1-meta")
                                            .timestamp("2:14 PM")
                                            .status(ChatMessageStatus::Read),
                                    )
                                    .child("Third, read"),
                            ),
                    )
                    .child(
                        ChatMessage::new("b2", ChatMessageSender::Assistant)
                            .avatar(Avatar::new("b2-avatar").name("Astryx"))
                            .child(
                                ChatMessageBubble::new()
                                    .variant(ChatMessageBubbleVariant::Ghost)
                                    .name("Astryx")
                                    .child("A ghost bubble keeps the alignment without a fill."),
                            ),
                    )
                    .child(ChatSystemMessage::new("Ada joined the conversation"))
                    .child(
                        ChatMessage::new("b3", ChatMessageSender::User).child(
                            ChatMessageBubble::new()
                                .metadata(
                                    ChatMessageMetadata::new()
                                        .id("b3-meta")
                                        .status(ChatMessageStatus::Sending),
                                )
                                .child("Sending…"),
                        ),
                    ),
            ),
        )
        .into_any_element()
}

const REPLIES: [&str; 3] = [
    "Here is a **Markdown** reply with a list:\n\n- one\n- two\n- three",
    "Messages follow the newest one while you are at the bottom.",
    "Scroll up first, and a new reply shows the *New messages* button instead.",
];

/// A conversation that grows by a reply each time the button is pressed.
#[derive(IntoElement)]
struct Conversation {
    id: ElementId,
}

impl RenderOnce for Conversation {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let count = window.use_keyed_state(
            ElementId::Name(format!("{}-count", self.id).into()),
            cx,
            |_, _| 4usize,
        );
        let n = *count.read(cx);
        let mut list = ChatMessageList::new(ElementId::Name(format!("{}-log", self.id).into()))
            .child(ChatSystemMessage::new("Today").variant(ChatSystemMessageVariant::Divider));
        for i in 0..n {
            let user = i % 2 == 0;
            let id = ElementId::Name(format!("{}-m{i}", self.id).into());
            let message = if user {
                ChatMessage::new(id, ChatMessageSender::User).child(
                    ChatMessageBubble::new()
                        .child(format!("Question {}: how does Chat scroll?", i / 2 + 1)),
                )
            } else {
                ChatMessage::new(id.clone(), ChatMessageSender::Assistant)
                    .avatar(
                        Avatar::new(ElementId::Name(format!("{}-a{i}", self.id).into()))
                            .name("Astryx"),
                    )
                    .child(ChatMessageBubble::new().child(Markdown::new(
                        ElementId::Name(format!("{}-md{i}", self.id).into()),
                        REPLIES[(i / 2) % REPLIES.len()],
                    )))
                    .metadata(ChatMessageMetadata::new().timestamp("Just now"))
            };
            list = list.child(message);
        }
        let composer = div()
            .flex()
            .justify_end()
            .p(px(8.))
            .rounded(px(12.))
            .border_1()
            .child(
                Button::new(
                    ElementId::Name(format!("{}-add", self.id).into()),
                    "Add a reply",
                )
                .on_click(move |_, window, cx| {
                    count.update(cx, |n, _| *n += 2);
                    window.refresh();
                }),
            );
        div()
            .h(px(420.))
            .w(px(640.))
            .flex()
            .flex_col()
            .border_1()
            .rounded(px(12.))
            .child(ChatLayout::new(self.id, composer).child(list))
    }
}
