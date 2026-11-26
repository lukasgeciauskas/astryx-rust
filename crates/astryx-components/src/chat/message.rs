//! ChatMessage, upstream `Chat/ChatMessage.tsx`: one message, aligned by its sender, with
//! an avatar, a name, its content and metadata.

use std::cell::Cell;
use std::rc::Rc;

use astryx_primitives::provide_in_layer;
use astryx_theme::ThemeStyled as _;
use astryx_theme::current;
use gpui::Refineable as _;
use gpui::prelude::*;
use gpui::{
    AnyElement, App, ElementId, IntoElement, ParentElement, Pixels, RenderOnce, Role,
    StyleRefinement, Styled, Window, canvas, div, px, relative,
};
use smallvec::SmallVec;

use super::context::{
    ChatDensity, ChatMessageContext, ChatMessageSender, use_chat_layout_context,
    use_chat_list_context,
};

/// Upstream `ChatMessage`.
///
/// ```ignore
/// ChatMessage::new("m1", ChatMessageSender::Assistant)
///     .avatar(Avatar::new("ai").name("Assistant"))
///     .child(ChatMessageBubble::new().child(Markdown::new("m1-md", reply)))
/// ```
#[derive(IntoElement)]
pub struct ChatMessage {
    id: ElementId,
    sender: ChatMessageSender,
    avatar: Option<AnyElement>,
    name: Option<AnyElement>,
    metadata: Option<AnyElement>,
    density: Option<ChatDensity>,
    style: StyleRefinement,
    children: SmallVec<[AnyElement; 2]>,
}

impl ChatMessage {
    /// Upstream `sender`; the id names the message (its state, and the layout's record of
    /// which message is newest).
    pub fn new(id: impl Into<ElementId>, sender: ChatMessageSender) -> Self {
        ChatMessage {
            id: id.into(),
            sender,
            avatar: None,
            name: None,
            metadata: None,
            density: None,
            style: StyleRefinement::default(),
            children: SmallVec::new(),
        }
    }

    /// Upstream `avatar`: beside the message; a system message shows none (FR2).
    pub fn avatar(mut self, avatar: impl IntoElement) -> Self {
        self.avatar = Some(avatar.into_any_element());
        self
    }

    /// Upstream `name`: above the content.
    pub fn name(mut self, name: impl IntoElement) -> Self {
        self.name = Some(name.into_any_element());
        self
    }

    /// Upstream `metadata`: below the content.
    pub fn metadata(mut self, metadata: impl IntoElement) -> Self {
        self.metadata = Some(metadata.into_any_element());
        self
    }

    /// Upstream `density`: else the list's, else balanced (FR4).
    pub fn density(mut self, density: ChatDensity) -> Self {
        self.density = Some(density);
        self
    }
}

impl Styled for ChatMessage {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl ParentElement for ChatMessage {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

/// What a message keeps between frames.
struct MessageState {
    name_row: Rc<Cell<bool>>,
    column: Rc<Cell<Pixels>>,
}

impl RenderOnce for ChatMessage {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let i18n = astryx_i18n::current(cx);
        let density = self
            .density
            .or_else(|| use_chat_list_context(cx).map(|c| c.density))
            .unwrap_or_default();
        let sender = self.sender;
        let system = sender == ChatMessageSender::System;
        let state = window.use_keyed_state(
            ElementId::Name(format!("{}-chat-message", self.id).into()),
            cx,
            |_, _| MessageState {
                name_row: Rc::new(Cell::new(false)),
                column: Rc::new(Cell::new(px(0.))),
            },
        );
        let (name_row, column) = {
            let s = state.read(cx);
            (s.name_row.clone(), s.column.clone())
        };
        // A bubble's name row lowers the avatar to the bubble; it is known once the
        // bubbles have rendered, so this frame uses the last frame's answer.
        let had_name_row = name_row.replace(false);

        // The layout's record of its messages, in order (useChatNewMessages,
        // scrollToLastMessage).
        let registry = use_chat_layout_context(cx).map(|layout| {
            let mut messages = layout.messages.borrow_mut();
            messages.ids.push(self.id.clone());
            messages.bounds.push(Default::default());
            (layout.messages.clone(), messages.ids.len() - 1)
        });

        let avatar = self.avatar.filter(|_| !system);
        let name = self.name.filter(|_| !system);
        let metadata = self.metadata.filter(|_| !system);
        let has_avatar = avatar.is_some();

        let mut root = div()
            .id(self.id.clone())
            .role(Role::Article)
            .aria_label(i18n.format(
                "@astryx.chatMessage.messageFrom",
                &[("sender", sender.as_str().into())],
            ))
            .relative()
            .flex()
            .items_start()
            .max_w_full();
        let dir = astryx_i18n::use_direction(cx);
        root = match sender {
            ChatMessageSender::System => root.flex_row().justify_center(),
            // The user's message sits at the inline end.
            ChatMessageSender::User if !dir.is_rtl() => root.flex_row_reverse(),
            ChatMessageSender::User => root.flex_row(),
            ChatMessageSender::Assistant if dir.is_rtl() => root.flex_row_reverse(),
            ChatMessageSender::Assistant => root.flex_row(),
        };
        if has_avatar {
            root = root.gap(match density {
                ChatDensity::Compact => t.spacing.s1_5,
                ChatDensity::Balanced => t.spacing.s2,
                ChatDensity::Spacious => t.spacing.s3,
            });
        }
        theme.refine(
            root.style(),
            "chat-message",
            &[("sender", sender.as_str()), ("density", density.as_str())],
            None,
        );
        root.style().refine(&self.style);

        let align = |d: gpui::Div| match sender {
            ChatMessageSender::System => d.items_center(),
            ChatMessageSender::User => d.items_end(),
            ChatMessageSender::Assistant => d.items_start(),
        };
        let mut column_el = align(div().flex().flex_col().flex_1().min_w_0());
        if system {
            column_el = column_el.max_w(relative(0.9));
        }
        let measure = column.clone();
        let check = name_row.clone();
        let content = column_el
            .children(name.map(|name| {
                div()
                    .text_style(&t.text.supporting)
                    .font_weight(t.font.weight.semibold)
                    .text_color(t.color.text_secondary)
                    .mb(t.spacing.s1)
                    .child(name)
            }))
            .child(
                align(div().flex().flex_col().min_w_0().w_full())
                    .gap(match density {
                        ChatDensity::Compact => t.spacing.s0_5,
                        ChatDensity::Balanced => t.spacing.s1,
                        ChatDensity::Spacious => t.spacing.s1_5,
                    })
                    .children(self.children),
            )
            .children(metadata)
            .child(
                canvas(
                    move |bounds, window, _| {
                        if measure.get() != bounds.size.width {
                            measure.set(bounds.size.width);
                            window.refresh();
                        }
                        if check.get() != had_name_row {
                            window.refresh();
                        }
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            );
        let root = root
            .children(avatar.map(|avatar| {
                div()
                    .flex_none()
                    .when(had_name_row, |d| d.mt(t.spacing.s5))
                    .child(avatar)
            }))
            .child(content.relative())
            .children(registry.map(|(messages, index)| {
                canvas(
                    move |bounds, _, _| {
                        if let Some(slot) = messages.borrow_mut().bounds.get_mut(index) {
                            *slot = bounds;
                        }
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full()
            }));
        let context = ChatMessageContext {
            sender,
            density,
            name_row,
            column,
        };
        provide_in_layer(context, root, cx)
    }
}
