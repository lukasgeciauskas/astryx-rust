//! ChatMessageBubble, upstream `Chat/ChatMessageBubble.tsx`: a message's styled body,
//! painted for its sender, with optional name and metadata rows aligned to it.

use gpui::Refineable as _;
use gpui::{
    AnyElement, App, IntoElement, Length, ParentElement, RenderOnce, StyleRefinement, Styled,
    Window, div, px, relative,
};
use rustryx_theme::{ThemeStyled as _, current};
use smallvec::SmallVec;

use super::context::{ChatDensity, ChatMessageSender, use_chat_message_context};

/// Upstream `ChatMessageBubbleVariant`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ChatMessageBubbleVariant {
    /// The sender's fill.
    #[default]
    Filled,
    /// No fill and no block padding, keeping the inline alignment (FR4).
    Ghost,
}

impl ChatMessageBubbleVariant {
    pub fn as_str(self) -> &'static str {
        match self {
            ChatMessageBubbleVariant::Filled => "filled",
            ChatMessageBubbleVariant::Ghost => "ghost",
        }
    }
}

/// Upstream `group`: a bubble's place in a run of bubbles; its sender-side corners
/// tighten (FR5).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChatBubbleGroup {
    First,
    Middle,
    Last,
}

/// Upstream `ChatMessageBubble`.
///
/// ```ignore
/// ChatMessage::new("m1", ChatMessageSender::User).child(ChatMessageBubble::new().child("Hi!"))
/// ```
#[derive(IntoElement, Default)]
pub struct ChatMessageBubble {
    variant: ChatMessageBubbleVariant,
    name: Option<AnyElement>,
    metadata: Option<AnyElement>,
    group: Option<ChatBubbleGroup>,
    width: Option<Length>,
    style: StyleRefinement,
    children: SmallVec<[AnyElement; 2]>,
}

impl ChatMessageBubble {
    pub fn new() -> Self {
        Self::default()
    }

    rustryx_primitives::setters! {
        /// Upstream `variant`.
        variant: ChatMessageBubbleVariant,
        /// Upstream `group`.
        ?group: ChatBubbleGroup,
        /// Upstream `width`: replaces the default `max(80%, 280px)` cap (FR6).
        ?width: Length,
    }

    /// Upstream `name`: the sender's name above the bubble.
    pub fn name(mut self, name: impl IntoElement) -> Self {
        self.name = Some(name.into_any_element());
        self
    }

    /// Upstream `metadata`: a row below the bubble, typically ChatMessageMetadata.
    pub fn metadata(mut self, metadata: impl IntoElement) -> Self {
        self.metadata = Some(metadata.into_any_element());
        self
    }
}

impl Styled for ChatMessageBubble {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl ParentElement for ChatMessageBubble {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for ChatMessageBubble {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let rem = window.rem_size();
        let context = use_chat_message_context(cx);
        let sender = context
            .as_ref()
            .map_or(ChatMessageSender::Assistant, |c| c.sender);
        let density = context
            .as_ref()
            .map_or(ChatDensity::Balanced, |c| c.density);
        let user = sender == ChatMessageSender::User;
        let dir = rustryx_i18n::use_direction(cx);
        let (pad_block, pad_inline) = match density {
            ChatDensity::Spacious => (t.spacing.s4, t.spacing.s5),
            _ => (t.spacing.s3, t.spacing.s4),
        };
        let radius = if density == ChatDensity::Compact {
            t.radius.container
        } else {
            t.radius.chat
        };
        let mut el = div()
            .flex()
            .flex_col()
            .min_w_0()
            .rounded(radius)
            .font_stack(&t.font.family.body)
            .text_style(&t.text.body)
            .text_color(t.color.text_primary)
            .px(pad_inline)
            .py(pad_block);
        el = match self.width {
            Some(width) => el.w(width),
            // FR6: max(80%, 280px) of the column.
            None => {
                let column = context.as_ref().map_or(px(0.), |c| c.column.get());
                if column > px(0.) {
                    el.max_w((column * 0.8).max(px(280.)).min(column))
                } else {
                    el.max_w(relative(0.8))
                }
            }
        };
        if self.variant == ChatMessageBubbleVariant::Filled {
            el = el.bg(t.color.neutral);
        } else {
            el = el.py(px(0.));
        }
        // FR5: the sender side's corners tighten along a group.
        if let Some(group) = self.group {
            let inner = t.radius.inner;
            let (start_start, end_start) = match group {
                ChatBubbleGroup::First => (false, true),
                ChatBubbleGroup::Middle => (true, true),
                ChatBubbleGroup::Last => (true, false),
            };
            // The sender side: inline end for the user, inline start otherwise.
            let at_end = user != dir.is_rtl();
            if start_start {
                el = if at_end {
                    el.rounded_tr(inner)
                } else {
                    el.rounded_tl(inner)
                };
            }
            if end_start {
                el = if at_end {
                    el.rounded_br(inner)
                } else {
                    el.rounded_bl(inner)
                };
            }
        }
        theme.refine(
            el.style(),
            "chat-message-bubble",
            &[
                ("sender", sender.as_str()),
                ("variant", self.variant.as_str()),
                ("density", density.as_str()),
            ],
            None,
        );
        el.style().refine(&self.style);
        let meta_pad = if density == ChatDensity::Spacious {
            t.spacing.s5
        } else {
            t.spacing.s4
        };
        let reduced = t.spacing.s1_5.to_pixels(rem);
        let name = self.name.map(|name| {
            if let Some(context) = &context {
                context.name_row.set(true);
            }
            let row = div()
                .flex()
                .items_center()
                .h(t.spacing.s5)
                .px(meta_pad)
                .mb(-reduced);
            let row = if user { row.justify_end() } else { row };
            row.child(name)
        });
        let metadata = self.metadata.map(|metadata| {
            let row = div().px(meta_pad).mt(-reduced);
            let row = if user { row.flex().justify_end() } else { row };
            row.child(metadata)
        });
        let alignment = |d: gpui::Div| if user { d.items_end() } else { d.items_start() };
        alignment(div().flex().flex_col().w_full().min_w_0())
            .children(name)
            .child(el.children(self.children))
            .children(metadata)
    }
}
