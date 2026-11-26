//! The Chat family's shared values, upstream `Chat/ChatContext.tsx`.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use astryx_primitives::{use_context, use_layer_context};
use gpui::{App, Bounds, ElementId, Pixels, SharedString, Window};

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

// =============================================================================
// Composer context: shared between ChatComposer, its input and its send button

/// A composer value callback: the draft, or the value to submit.
pub type ChatValueFn = Rc<dyn Fn(SharedString, &mut Window, &mut App)>;
/// A composer action without arguments (upstream `onStop`).
pub type ChatActionFn = Rc<dyn Fn(&mut Window, &mut App)>;

/// Upstream `ChatComposerInputControl`: what the composer shell can ask of its input
/// slot. The body's click-to-focus calls `focus`.
#[derive(Clone)]
pub struct ChatComposerInputControl {
    /// Moves keyboard focus into the input, the caret after the draft.
    pub focus: ChatActionFn,
    /// The editor's focus, when the input has one: keyboard focus on it draws the
    /// body's focus ring (FR8).
    pub focus_handle: Option<gpui::FocusHandle>,
}

/// Upstream `ChatComposerContextValue`: the composer's draft and operations, for
/// ChatComposerInput, ChatSendButton and custom inputs.
#[derive(Clone)]
pub struct ChatComposerContext {
    /// Upstream `value`: the current serialized draft.
    pub value: SharedString,
    /// Upstream `onChange`: reports a new draft through the composer's channel.
    pub on_change: ChatValueFn,
    /// Upstream `onSubmit`: trims the value, ignores an empty or disabled one,
    /// calls the composer's `on_submit` and clears the draft.
    pub on_submit: ChatValueFn,
    /// Upstream `placeholder`.
    pub placeholder: SharedString,
    /// Upstream `isDisabled`.
    pub disabled: bool,
    /// Upstream `isStopShown`.
    pub stop_shown: bool,
    /// Upstream `canSend`: a non-empty draft in an enabled composer.
    pub can_send: bool,
    /// Upstream `onStop`.
    pub on_stop: Option<ChatActionFn>,
    /// Upstream `inputControlRef`: the input slot registers its control here each
    /// frame it renders; the shell reads it to focus the input.
    pub input_control: Rc<RefCell<Option<ChatComposerInputControl>>>,
}

/// Upstream `useChatComposerContext`: the enclosing ChatComposer's context, if any.
pub fn use_chat_composer_context(cx: &App) -> Option<ChatComposerContext> {
    use_context::<ChatComposerContext>(cx)
}
