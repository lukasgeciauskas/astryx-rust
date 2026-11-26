//! Chat examples for the gallery.

use gpui::SharedString;
use gpui::{
    AnyElement, App, ElementId, IntoElement, ParentElement, RenderOnce, Styled, Window, div, px,
};

use super::{
    ChatBubbleGroup, ChatDensity, ChatLayout, ChatMessage, ChatMessageBubble,
    ChatMessageBubbleVariant, ChatMessageList, ChatMessageMetadata, ChatMessageSender,
    ChatMessageStatus, ChatSystemMessage, ChatSystemMessageVariant,
};
use super::{
    ChatComposer, ChatComposerDrawer, ChatComposerElevation, ChatComposerInput, ChatComposerStatus,
    ChatComposerToken, ChatComposerTrigger, ChatComposerTriggerResult, ChatTokenizedText,
};
use super::{
    ChatDictationButton, ChatToolCallItem, ChatToolCallStatus, ChatToolCalls, use_chat_dictation,
};
use crate::button::{Button, ButtonVariant};
use crate::icon::Icon;
use crate::layout::{SpacingStep, Stack};
use crate::markdown::Markdown;
use crate::people::Avatar;
use crate::search::{SearchItem, Searchable as _, static_source};
use crate::status::{Badge, BadgeVariant};
use crate::text::{Text, TextType};

pub fn examples(window: &mut Window, cx: &mut App) -> AnyElement {
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
        .child(label(
            "ChatComposer: type @ for people or / for commands, paste a long text, open the drawer",
        ))
        .child(ComposerShowcase { id: "composer-demo".into() })
        .child(label("ChatToolCalls: one call, a group (press it to expand), and a result detail"))
        .child(tool_calls())
        .child(label(
            "ChatDictationButton: speech into the composer (hidden where no recognizer is installed)",
        ))
        .child(dictation(window, cx))
        .into_any_element()
}

fn tool_calls() -> AnyElement {
    div()
        .w(px(560.))
        .flex()
        .flex_col()
        .gap(px(8.))
        .child(ChatToolCalls::new(
            "tools-one",
            vec![
                ChatToolCallItem::new("search_docs")
                    .target("ChatToolCalls")
                    .duration("0.8s"),
            ],
        ))
        .child(ChatToolCalls::new(
            "tools-group",
            vec![
                ChatToolCallItem::new("search")
                    .target("astryx chat layout")
                    .duration("1.2s"),
                ChatToolCallItem::new("read_file")
                    .target("crates/astryx-components/src/chat/layout.rs"),
                ChatToolCallItem::new("edit_file")
                    .target("crates/astryx-components/src/chat/tool_calls.rs")
                    .additions(212)
                    .deletions(4)
                    .node("macbook")
                    .status(ChatToolCallStatus::Running),
            ],
        ))
        .child(ChatToolCalls::new(
            "tools-detail",
            vec![
                ChatToolCallItem::new("run_tests")
                    .target("cargo test -p astryx-components")
                    .status(ChatToolCallStatus::Error)
                    .error_message("2 tests failed")
                    .result_detail(|_, _| {
                        Text::new("chat::tests::follows … FAILED\nchat::tests::unlocks … FAILED")
                            .text_type(TextType::Supporting)
                            .into_any_element()
                    }),
            ],
        ))
        .into_any_element()
}

fn dictation(window: &mut Window, cx: &mut App) -> AnyElement {
    let speech = use_chat_dictation("gallery-dictation", Default::default(), window, cx);
    let heard = speech.interim_transcript(cx);
    div()
        .flex()
        .items_center()
        .gap(px(8.))
        .child(
            ChatDictationButton::new("gallery-mic", speech.clone()).hidden_when_unsupported(false),
        )
        .child(
            Text::new(if heard.is_empty() {
                SharedString::from(if speech.is_supported() {
                    "Press the microphone and speak"
                } else {
                    "No speech recognizer here: install one with set_speech_recognition_provider"
                })
            } else {
                heard
            })
            .text_type(TextType::Supporting),
        )
        .into_any_element()
}

const REPLIES: [&str; 3] = [
    "Here is a **Markdown** reply with a list:\n\n- one\n- two\n- three",
    "Messages follow the newest one while you are at the bottom.",
    "Scroll up first, and a new reply shows the *New messages* button instead.",
];

/// People the composer's `@` trigger suggests.
fn people() -> Vec<SearchItem> {
    [
        ("ada", "Ada Lovelace"),
        ("alan", "Alan Turing"),
        ("grace", "Grace Hopper"),
        ("katherine", "Katherine Johnson"),
        ("linus", "Linus Torvalds"),
    ]
    .into_iter()
    .map(|(id, name)| SearchItem::new(id, name).group("People"))
    .collect()
}

/// The token a mention becomes, in the composer and in sent messages alike.
fn mention_token(id: &str, name: &str) -> ChatComposerToken {
    ChatComposerToken::badge(format!("@{id}"), format!("@{name}")).variant(BadgeVariant::Blue)
}

fn mention_tokens() -> Vec<ChatComposerToken> {
    people()
        .iter()
        .map(|p| mention_token(&p.id(), &p.label()))
        .collect()
}

/// `@` mentions (tokens) and `/` commands (text).
fn triggers() -> Vec<ChatComposerTrigger> {
    let commands = static_source(
        [
            ("summarize", "/summarize — sum up the thread"),
            ("translate", "/translate — into another language"),
            ("explain", "/explain — the last answer, simpler"),
        ]
        .into_iter()
        .map(|(id, label)| SearchItem::new(id, label)),
    );
    vec![
        ChatComposerTrigger::new('@', static_source(people()), |item| {
            mention_token(&item.id(), &item.label()).into()
        })
        .deserialize(|text| {
            let id = text.strip_prefix('@')?;
            people()
                .into_iter()
                .find(|p| p.id().as_ref() == id)
                .map(|p| mention_token(&p.id(), &p.label()))
        })
        .menu_label("People"),
        ChatComposerTrigger::new('/', commands, |item| {
            ChatComposerTriggerResult::Text(format!("/{} ", item.id()).into())
        })
        .menu_label("Commands"),
    ]
}

/// A conversation: the composer's send appends the message and a canned reply.
#[derive(IntoElement)]
struct Conversation {
    id: ElementId,
}

impl RenderOnce for Conversation {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let log = window.use_keyed_state(
            ElementId::Name(format!("{}-log-state", self.id).into()),
            cx,
            |_, _| {
                (0..4)
                    .map(|i| {
                        let text: SharedString =
                            format!("Question {}: how does Chat scroll?", i / 2 + 1).into();
                        (i % 2 == 0, text)
                    })
                    .collect::<Vec<(bool, SharedString)>>()
            },
        );
        let entries = log.read(cx).clone();
        let mut list = ChatMessageList::new(ElementId::Name(format!("{}-log", self.id).into()))
            .child(ChatSystemMessage::new("Today").variant(ChatSystemMessageVariant::Divider));
        for (i, (user, text)) in entries.into_iter().enumerate() {
            let id = ElementId::Name(format!("{}-m{i}", self.id).into());
            let message = if user {
                ChatMessage::new(id, ChatMessageSender::User).child(
                    ChatMessageBubble::new()
                        .child(ChatTokenizedText::new(text).tokens(mention_tokens())),
                )
            } else {
                ChatMessage::new(id.clone(), ChatMessageSender::Assistant)
                    .avatar(
                        Avatar::new(ElementId::Name(format!("{}-a{i}", self.id).into()))
                            .name("Astryx"),
                    )
                    .child(ChatMessageBubble::new().child(Markdown::new(
                        ElementId::Name(format!("{}-md{i}", self.id).into()),
                        if i < 4 {
                            SharedString::from(REPLIES[(i / 2) % REPLIES.len()])
                        } else {
                            text
                        },
                    )))
                    .metadata(ChatMessageMetadata::new().timestamp("Just now"))
            };
            list = list.child(message);
        }
        let composer = ChatComposer::new(
            ElementId::Name(format!("{}-composer", self.id).into()),
            move |value, window, cx| {
                log.update(cx, |log, _| {
                    let n = log.len();
                    log.push((true, value));
                    log.push((false, REPLIES[(n / 2) % REPLIES.len()].into()));
                });
                window.refresh();
            },
        )
        .placeholder("Add a reply…")
        .input(
            ChatComposerInput::new(ElementId::Name(format!("{}-input", self.id).into()))
                .triggers(triggers()),
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

/// The composer's parts: tokens from triggers, a drawer, header and footer slots, and
/// the send button's states.
#[derive(IntoElement)]
struct ComposerShowcase {
    id: ElementId,
}

impl RenderOnce for ComposerShowcase {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let key = self.id.to_string();
        let streaming = window.use_keyed_state(
            ElementId::Name(format!("{key}-streaming").into()),
            cx,
            |_, _| false,
        );
        let sent =
            window.use_keyed_state(ElementId::Name(format!("{key}-sent").into()), cx, |_, _| {
                SharedString::from("Nothing sent yet")
            });
        let is_streaming = *streaming.read(cx);
        let drawer = ChatComposerDrawer::new(ElementId::Name(format!("{key}-drawer").into()))
            .count(2usize)
            .label("Attachments")
            .child(Badge::new("design-review.pdf"))
            .child(Badge::new("notes.md"));
        let (stop, start) = (streaming.clone(), streaming.clone());
        let shown = sent.clone();
        let composer = ChatComposer::new(ElementId::Name(format!("{key}-composer").into()), {
            let sent = sent.clone();
            move |value, window, cx| {
                sent.update(cx, |s, _| *s = format!("Sent: {value}").into());
                window.refresh();
            }
        })
        .stop_shown(is_streaming)
        .on_stop(move |window, cx| {
            stop.update(cx, |s, _| *s = false);
            window.refresh();
        })
        .drawer(drawer)
        .header_actions(
            Button::new(ElementId::Name(format!("{key}-attach").into()), "Attach")
                .icon(Icon::new("upload"))
                .icon_only(true)
                .variant(ButtonVariant::Ghost)
                .size(astryx_primitives::ElementSize::Sm),
        )
        .header_context(Text::new("2.1k of 8k tokens").text_type(TextType::Supporting))
        .footer_actions(
            Button::new(
                ElementId::Name(format!("{key}-stream").into()),
                "Simulate a streaming reply",
            )
            .variant(ButtonVariant::Ghost)
            .on_click(move |_, window, cx| {
                start.update(cx, |s, _| *s = true);
                window.refresh();
            }),
        )
        .input(
            ChatComposerInput::new(ElementId::Name(format!("{key}-input").into()))
                .triggers(triggers()),
        );
        let flat = ChatComposer::new(ElementId::Name(format!("{key}-flat").into()), |_, _, _| {})
            .density(ChatDensity::Compact)
            .elevation(ChatComposerElevation::None)
            .placeholder("A flat, compact composer with a status")
            .status(ChatComposerStatus::warning(
                "Responses may be slow right now",
            ));
        div()
            .w(px(560.))
            .flex()
            .flex_col()
            .gap(px(16.))
            .child(composer)
            .child(Text::new(shown.read(cx).clone()).text_type(TextType::Supporting))
            .child(flat)
    }
}
