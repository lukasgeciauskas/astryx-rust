//! ChatMessageMetadata, upstream `Chat/ChatMessageMetadata.tsx`: a message's timestamp,
//! footer and delivery status in one row.

use std::time::Duration;

use astryx_theme::ThemeStyled as _;
use astryx_theme::current;
use gpui::Refineable as _;
use gpui::prelude::*;
use gpui::{
    Animation, AnimationExt as _, AnyElement, App, ElementId, IntoElement, ParentElement,
    RenderOnce, SharedString, StyleRefinement, Styled, Window, div,
};

use super::context::{ChatMessageSender, use_chat_message_context};
use crate::icon::{Icon, IconSize, IconSource};

/// Upstream `ChatMessageStatus`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChatMessageStatus {
    Sending,
    Sent,
    Delivered,
    Read,
    Error,
}

impl ChatMessageStatus {
    /// The icon and the label's message key (FR4).
    fn config(self) -> (&'static str, &'static str) {
        match self {
            ChatMessageStatus::Sending => ("clock", "@astryx.chat.status.sending"),
            ChatMessageStatus::Sent => ("check", "@astryx.chat.status.sent"),
            ChatMessageStatus::Delivered => ("checkDouble", "@astryx.chat.status.delivered"),
            ChatMessageStatus::Read => ("checkDouble", "@astryx.chat.status.read"),
            ChatMessageStatus::Error => ("error", "@astryx.chat.status.failed"),
        }
    }
}

/// Upstream `ChatMessageMetadata`. It renders nothing when every slot is empty (FR1).
///
/// ```ignore
/// ChatMessageMetadata::new().timestamp("2:14 PM").status(ChatMessageStatus::Read)
/// ```
#[derive(IntoElement, Default)]
pub struct ChatMessageMetadata {
    id: Option<ElementId>,
    timestamp: Option<AnyElement>,
    footer: Option<AnyElement>,
    status: Option<ChatMessageStatus>,
    style: StyleRefinement,
}

impl ChatMessageMetadata {
    pub fn new() -> Self {
        Self::default()
    }

    /// Upstream `timestamp`: text or a Timestamp.
    pub fn timestamp(mut self, timestamp: impl IntoElement) -> Self {
        self.timestamp = Some(timestamp.into_any_element());
        self
    }

    /// Upstream `footer`: model info, reaction or copy buttons.
    pub fn footer(mut self, footer: impl IntoElement) -> Self {
        self.footer = Some(footer.into_any_element());
        self
    }

    /// Upstream `status`: an icon and a label.
    pub fn status(mut self, status: ChatMessageStatus) -> Self {
        self.status = Some(status);
        self
    }

    /// An element id, for the status's accessible name (several rows on a page).
    pub fn id(mut self, id: impl Into<ElementId>) -> Self {
        self.id = Some(id.into());
        self
    }
}

impl Styled for ChatMessageMetadata {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for ChatMessageMetadata {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        if self.timestamp.is_none() && self.footer.is_none() && self.status.is_none() {
            return div().into_any_element();
        }
        let theme = current(window, cx);
        let t = theme.tokens();
        let i18n = astryx_i18n::current(cx);
        let sender =
            use_chat_message_context(cx).map_or(ChatMessageSender::Assistant, |c| c.sender);
        let dot = || div().child("\u{00B7}");
        let mut el = div()
            .flex()
            .items_center()
            .gap(t.spacing.s1)
            .mt(t.spacing.s1)
            .text_style(&t.text.supporting)
            .font_weight(t.font.weight.normal)
            .text_color(t.color.text_secondary);
        // FR3: a user's row reads from the end.
        if sender == ChatMessageSender::User {
            el = el.flex_row_reverse();
        }
        theme.refine(el.style(), "chat-message-metadata", &[], None);
        el.style().refine(&self.style);
        let has_timestamp = self.timestamp.is_some();
        let has_footer = self.footer.is_some();
        let has_status = self.status.is_some();
        let mut el = el;
        if let Some(timestamp) = self.timestamp {
            el = el.child(div().child(timestamp));
        }
        if has_timestamp && (has_footer || has_status) {
            el = el.child(dot());
        }
        if let Some(footer) = self.footer {
            el = el.child(footer);
        }
        if has_footer && has_status {
            el = el.child(dot());
        }
        if let Some(status) = self.status {
            let (icon, key) = status.config();
            let label = i18n.t(key);
            let name: SharedString = i18n.format(
                "@astryx.chat.messageAriaLabel",
                &[("status", label.to_lowercase().into())],
            );
            let id = self
                .id
                .clone()
                .map(|id| ElementId::Name(format!("{id}-status").into()))
                .unwrap_or_else(|| ElementId::Name(format!("chat-status-{}", label).into()));
            let mut row = div()
                .id(id.clone())
                .flex()
                .items_center()
                .gap(t.spacing.s1)
                .aria_label(name)
                .child(Icon::new(IconSource::Named(icon.into())).size(IconSize::Xsm))
                .child(div().child(label.clone()));
            if status == ChatMessageStatus::Error {
                row = row.text_color(t.color.text_error);
            }
            let tooltip_id = id.clone();
            let row = if status == ChatMessageStatus::Sending
                && !astryx_primitives::prefers_reduced_motion(cx)
            {
                // FR4: sending pulses between full and half opacity every 1.5s.
                row.with_animation(
                    id,
                    Animation::new(Duration::from_millis(1500)).repeat(),
                    |row, delta| {
                        let wave = (delta * std::f32::consts::TAU).cos();
                        row.opacity(0.75 + 0.25 * wave)
                    },
                )
                .into_any_element()
            } else {
                row.into_any_element()
            };
            // The status's title, upstream's `title` attribute.
            let row = crate::overlay::with_tooltip(&tooltip_id, Some(label), false, row);
            el = el.child(row);
        }
        el.into_any_element()
    }
}
