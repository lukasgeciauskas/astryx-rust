//! The ChatReasoning gallery (upstream's `Lab/ChatReasoning` stories).

use std::time::Duration;

use web_time::Instant;

use astryx_components::chat::{ChatMessage, ChatMessageBubble, ChatMessageList, ChatMessageSender};
use astryx_components::layout::{SpacingStep, Stack};
use astryx_components::markdown::{Markdown, MarkdownDensity};
use astryx_components::people::{Avatar, AvatarSize};
use astryx_components::text::{Text, TextColor, TextSize, TextType};
use gpui::{AnyElement, App, ElementId, IntoElement, ParentElement, Styled, Window, div, px};

use super::ChatReasoning;

fn example(title: &'static str, body: impl IntoElement) -> AnyElement {
    Stack::vertical()
        .gap(SpacingStep::S2)
        .child(Text::new(title).text_type(TextType::Large))
        .child(div().w(px(600.)).max_w_full().child(body))
        .into_any_element()
}

/// The ChatReasoning examples: collapsed, expanded, streaming, a custom label and inside
/// a message.
pub fn examples(window: &mut Window, cx: &mut App) -> AnyElement {
    // Streaming: the shimmer stops 5 s after the gallery first shows it.
    let started = *window
        .use_keyed_state(
            ElementId::Name("chat-reasoning-streaming-start".into()),
            cx,
            |_, _| Instant::now(),
        )
        .read(cx);
    let streaming = started.elapsed() < Duration::from_secs(5);
    if streaming {
        window.request_animation_frame();
    }
    Stack::vertical()
        .gap(SpacingStep::S8)
        .child(example(
            "Collapsed",
            ChatReasoning::new("reasoning-collapsed").duration("12s").text(
                "Let me work through the constraints systematically. The farmer has 3 fields and \
                 rotates wheat, corn, soy. No same crop in adjacent fields and no same crop in \
                 the same field two years in a row...",
            ),
        ))
        .child(example(
            "Expanded",
            ChatReasoning::new("reasoning-expanded")
                .duration("8s")
                .default_expanded(true)
                .child(
                    Markdown::new(
                        "reasoning-expanded-md",
                        "First, I need to understand the constraints:\n1. Three fields, three \
                         crops (wheat, corn, soy)\n2. No adjacent fields can have the same \
                         crop\n3. No field can repeat its crop from the previous year\n\nFor \
                         **Year 1**: 3 \u{d7} 2 \u{d7} 2 = 12 arrangements...",
                    )
                    .density(MarkdownDensity::Compact),
                ),
        ))
        .child(example(
            "Streaming",
            div()
                .child(
                    ChatReasoning::new("reasoning-streaming")
                        .streaming(streaming)
                        .label("Thinking")
                        .text("Working through the combinatorial constraints..."),
                )
                .children((!streaming).then(|| {
                    div().mt(px(8.)).child(
                        Text::new("(Shimmer stopped after 5s)")
                            .size(TextSize::Sm)
                            .color(TextColor::Secondary),
                    )
                })),
        ))
        .child(example(
            "Custom label",
            ChatReasoning::new("reasoning-label")
                .label("Analyzing")
                .duration("3s")
                .text("Checking the codebase for similar patterns..."),
        ))
        .child(example(
            "In a message",
            ChatMessageList::new("reasoning-list")
                .child(
                    ChatMessage::new("reasoning-user", ChatMessageSender::User).child(
                        ChatMessageBubble::new().child(
                            "How many valid planting arrangements are possible over 3 years?",
                        ),
                    ),
                )
                .child(
                    ChatMessage::new("reasoning-assistant", ChatMessageSender::Assistant)
                        .avatar(Avatar::new("reasoning-ai").name("AI").size(AvatarSize::Md))
                        .child(
                            ChatReasoning::new("reasoning-in-message")
                                .duration("12s")
                                .text("Let me work through the constraints systematically..."),
                        )
                        .child(
                            Markdown::new(
                                "reasoning-answer",
                                "There are **42** valid planting arrangements over 3 years.",
                            )
                            .density(MarkdownDensity::Compact),
                        ),
                ),
        ))
        .into_any_element()
}
