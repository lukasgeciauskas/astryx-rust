//! The Chat family's shared values, upstream `Chat/ChatContext.tsx`.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use astryx_primitives::{use_context, use_layer_context};
use gpui::{App, Bounds, ElementId, Pixels};

use super::stream_scroll::ChatStreamScroll;

/// Upstream `ChatMessageSender`: who sent a message; it sets the message's alignment.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ChatMessageSender {
    User,
    #[default]
    Assistant,
    System,
}

impl ChatMessageSender {
    pub fn as_str(self) -> &'static str {
        match self {
            ChatMessageSender::User => "user",
            ChatMessageSender::Assistant => "assistant",
            ChatMessageSender::System => "system",
        }
    }
}

/// Upstream `ChatDensity`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ChatDensity {
    Compact,
    #[default]
    Balanced,
    Spacious,
}

impl ChatDensity {
    pub fn as_str(self) -> &'static str {
        match self {
            ChatDensity::Compact => "compact",
            ChatDensity::Balanced => "balanced",
            ChatDensity::Spacious => "spacious",
        }
    }
}

/// Upstream `ChatMessageContext`: a message's sender and density, for its bubbles and
/// metadata. `name_row` notes that a bubble drew a name row, so the message can lower its
/// avatar to the bubble (upstream's `:has([data-chat-name])`).
#[derive(Clone)]
pub(crate) struct ChatMessageContext {
    pub sender: ChatMessageSender,
    pub density: ChatDensity,
    pub name_row: Rc<Cell<bool>>,
    /// The width of the message's content column at the last layout, for the bubbles'
    /// `max(80%, 280px)` cap.
    pub column: Rc<Cell<Pixels>>,
}

pub(crate) fn use_chat_message_context(cx: &App) -> Option<ChatMessageContext> {
    use_layer_context::<ChatMessageContext>(cx)
}

/// Upstream `ChatListContext`: the list's density, for its messages.
#[derive(Clone, Copy)]
pub(crate) struct ChatListContext {
    pub density: ChatDensity,
}

pub(crate) fn use_chat_list_context(cx: &App) -> Option<ChatListContext> {
    use_layer_context::<ChatListContext>(cx)
}

/// The messages a layout's content rendered this frame, in order, with where each
/// was laid out: upstream reads them from the DOM (`.astryx-chat-message`).
#[derive(Default)]
pub(crate) struct MessageRegistry {
    pub ids: Vec<ElementId>,
    pub bounds: Vec<Bounds<Pixels>>,
}

/// Upstream `ChatLayoutContext`: the layout's scroller, for a list inside it.
#[derive(Clone)]
pub struct ChatLayoutContext {
    /// The scroll that follows the newest message.
    pub scroll: ChatStreamScroll,
    pub(crate) messages: Rc<RefCell<MessageRegistry>>,
}

/// Upstream `useChatLayoutContext`: the enclosing ChatLayout's context, if any.
pub fn use_chat_layout_context(cx: &App) -> Option<ChatLayoutContext> {
    use_context::<ChatLayoutContext>(cx)
}
