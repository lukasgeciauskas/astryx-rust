//! The Chat family. Upstream: `Chat/` (component specs under `Chat/`): the layout and its
//! messages, the composer (with inline tokens and trigger menus), tool calls and dictation.
//! The family's lab parts (typing indicator, unread divider, virtualizer, emoji picker,
//! reaction bar) land with the lab in M13.

mod bubble;
mod composer;
mod composer_drawer;
mod composer_input;
mod composer_token;
mod context;
mod dictation;
pub mod examples;
mod keyboard;
mod layout;
mod message;
mod message_list;
mod metadata;
mod new_messages;
mod scroll_button;
mod send_button;
mod speech;
#[cfg(target_family = "wasm")]
mod speech_web;
mod stream_scroll;
mod system_message;
mod tokenized_text;
mod tool_calls;
mod trigger_menu;

#[cfg(test)]
mod composer_parts_tests;
#[cfg(test)]
mod composer_tests;
#[cfg(test)]
mod dictation_tests;
#[cfg(test)]
mod keyboard_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tool_calls_tests;

pub use bubble::{ChatBubbleGroup, ChatMessageBubble, ChatMessageBubbleVariant};
pub use composer::{
    ChatComposer, ChatComposerElevation, ChatComposerStatus, ChatComposerStatusPosition,
    ChatComposerStatusType,
};
pub use composer_drawer::ChatComposerDrawer;
pub use composer_input::{
    ChatComposerInput, ChatComposerInputHandle, FilesFn as ChatComposerFilesFn,
    KeyDownFn as ChatComposerKeyDownFn, PasteFn as ChatComposerPasteFn,
};
pub use composer_token::{
    ChatComposerToken, ChatComposerTokenElement, ChatPasteAsToken, ChatPastedTextToken, ToTokenFn,
    TokenRenderFn, UseChatPasteAsTokenOptions, format_pasted_text_label, use_chat_paste_as_token,
};
pub use context::{
    ChatActionFn, ChatComposerContext, ChatComposerInputControl, ChatDensity, ChatLayoutContext,
    ChatMessageSender, ChatValueFn, use_chat_composer_context, use_chat_layout_context,
};
pub use dictation::{
    ChatDictationButton, ChatDictationButtonSize, DictationTarget, UseChatDictationOptions,
    use_chat_dictation,
};
pub use layout::ChatLayout;
pub use message::ChatMessage;
pub use message_list::{ChatListAlign, ChatMessageList};
pub use metadata::{ChatMessageMetadata, ChatMessageStatus};
pub use new_messages::{ChatNewMessages, use_chat_new_messages};
pub use scroll_button::ChatLayoutScrollButton;
pub use send_button::{ChatSendButton, ChatSendButtonSize};
pub use speech::{
    SpeechEvent, SpeechRecognition, SpeechRecognitionProvider, SpeechRequest, SpeechResult,
    SpeechSink, UseSpeechRecognitionOptions, set_speech_recognition_provider,
    use_speech_recognition,
};
pub use stream_scroll::{
    ChatScrollBehavior, ChatScrollToBottomOptions, ChatStreamScroll, UseChatStreamScrollOptions,
    use_chat_stream_scroll,
};
pub use system_message::{ChatSystemMessage, ChatSystemMessageVariant};
pub use tokenized_text::ChatTokenizedText;
pub use tool_calls::{ChatToolCallItem, ChatToolCallStatus, ChatToolCalls};
pub use trigger_menu::{ChatComposerTrigger, ChatComposerTriggerResult};

use crate::doc::{ComponentDoc, PropMapping, RustApi};

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

const REFS: RustApi = RustApi::Waived("GPUI elements have no refs");
const TEST_IDS: RustApi = RustApi::Waived("GPUI has no DOM test ids; tests use debug selectors");

pub const CHAT_LAYOUT_DOC: ComponentDoc = ComponentDoc {
    name: "ChatLayout",
    rust_type: "astryx::components::chat::ChatLayout",
    props: &[
        p("children", RustApi::Trait("ParentElement")),
        p("composer", RustApi::Argument("composer")),
        p("emptyState", RustApi::Method("empty_state")),
        p(
            "scrollButton",
            RustApi::Method("scroll_button / no_scroll_button"),
        ),
        p("scrollRef", RustApi::Method("scroll_handle")),
        p("density", RustApi::Method("density")),
    ],
    example: include_str!("examples.rs"),
};

pub const CHAT_LAYOUT_SCROLL_BUTTON_DOC: ComponentDoc = ComponentDoc {
    name: "ChatLayoutScrollButton",
    rust_type: "astryx::components::chat::ChatLayoutScrollButton",
    props: &[
        p("isVisible", RustApi::Argument("visible")),
        p("label", RustApi::Method("label")),
        p("onClick", RustApi::Argument("on_click")),
    ],
    example: include_str!("examples.rs"),
};

pub const CHAT_MESSAGE_LIST_DOC: ComponentDoc = ComponentDoc {
    name: "ChatMessageList",
    rust_type: "astryx::components::chat::ChatMessageList",
    props: &[
        p("children", RustApi::Trait("ParentElement")),
        p("emptyState", RustApi::Method("empty_state")),
        p("scrollToTopAction", RustApi::Method("scroll_to_top_action")),
        p("density", RustApi::Method("density")),
        p("gap", RustApi::Method("gap")),
        p("align", RustApi::Method("align")),
        p("isStreaming", RustApi::Method("streaming")),
    ],
    example: include_str!("examples.rs"),
};

pub const CHAT_MESSAGE_DOC: ComponentDoc = ComponentDoc {
    name: "ChatMessage",
    rust_type: "astryx::components::chat::ChatMessage",
    props: &[
        p("sender", RustApi::Argument("sender")),
        p("children", RustApi::Trait("ParentElement")),
        p("avatar", RustApi::Method("avatar")),
        p("name", RustApi::Method("name")),
        p("metadata", RustApi::Method("metadata")),
        p("density", RustApi::Method("density")),
    ],
    example: include_str!("examples.rs"),
};

pub const CHAT_MESSAGE_BUBBLE_DOC: ComponentDoc = ComponentDoc {
    name: "ChatMessageBubble",
    rust_type: "astryx::components::chat::ChatMessageBubble",
    props: &[
        p("children", RustApi::Trait("ParentElement")),
        p("variant", RustApi::Method("variant")),
        p("name", RustApi::Method("name")),
        p("metadata", RustApi::Method("metadata")),
        p("group", RustApi::Method("group")),
        p("width", RustApi::Method("width")),
    ],
    example: include_str!("examples.rs"),
};

pub const CHAT_MESSAGE_METADATA_DOC: ComponentDoc = ComponentDoc {
    name: "ChatMessageMetadata",
    rust_type: "astryx::components::chat::ChatMessageMetadata",
    props: &[
        p("timestamp", RustApi::Method("timestamp")),
        p("footer", RustApi::Method("footer")),
        p("status", RustApi::Method("status")),
    ],
    example: include_str!("examples.rs"),
};

pub const CHAT_SYSTEM_MESSAGE_DOC: ComponentDoc = ComponentDoc {
    name: "ChatSystemMessage",
    rust_type: "astryx::components::chat::ChatSystemMessage",
    props: &[
        p("children", RustApi::Argument("content")),
        p("variant", RustApi::Method("variant")),
        p("icon", RustApi::Method("icon")),
        p("xstyle", RustApi::Trait("Styled")),
        p("ref", REFS),
        p("data-testid", TEST_IDS),
    ],
    example: include_str!("examples.rs"),
};

pub const CHAT_TOOL_CALLS_DOC: ComponentDoc = ComponentDoc {
    name: "ChatToolCalls",
    rust_type: "astryx::components::chat::ChatToolCalls",
    props: &[
        p("calls", RustApi::Argument("calls")),
        p("label", RustApi::Method("label")),
        p("isExpanded", RustApi::Method("expanded")),
        p("defaultIsExpanded", RustApi::Method("default_expanded")),
        p("onExpandedChange", RustApi::Method("on_expanded_change")),
        p("xstyle", RustApi::Trait("Styled")),
        p("ref", REFS),
    ],
    example: include_str!("examples.rs"),
};

pub const CHAT_DICTATION_BUTTON_DOC: ComponentDoc = ComponentDoc {
    name: "ChatDictationButton",
    rust_type: "astryx::components::chat::ChatDictationButton",
    props: &[
        p("dictation", RustApi::Argument("dictation")),
        p("size", RustApi::Method("size")),
        p(
            "isHiddenWhenUnsupported",
            RustApi::Method("hidden_when_unsupported"),
        ),
        p("label", RustApi::Method("label")),
        p("xstyle", RustApi::Trait("Styled")),
        p("ref", REFS),
    ],
    example: include_str!("examples.rs"),
};

pub const CHAT_COMPOSER_DOC: ComponentDoc = ComponentDoc {
    name: "ChatComposer",
    rust_type: "astryx::components::chat::ChatComposer",
    props: &[
        p("onSubmit", RustApi::Argument("on_submit")),
        p("onStop", RustApi::Method("on_stop")),
        p("isStopShown", RustApi::Method("stop_shown")),
        p("value", RustApi::Method("value")),
        p("onChange", RustApi::Method("on_change")),
        p("placeholder", RustApi::Method("placeholder")),
        p("isDisabled", RustApi::Method("disabled")),
        p("density", RustApi::Method("density")),
        p("elevation", RustApi::Method("elevation")),
        p("drawer", RustApi::Method("drawer")),
        p("headerActions", RustApi::Method("header_actions")),
        p("headerContext", RustApi::Method("header_context")),
        p("input", RustApi::Method("input")),
        p("footerActions", RustApi::Method("footer_actions")),
        p("sendActions", RustApi::Method("send_actions")),
        p("sendButton", RustApi::Method("send_button")),
        p("status", RustApi::Method("status (a ChatComposerStatus)")),
        p("statusPosition", RustApi::Method("status_position")),
        p("xstyle", RustApi::Trait("Styled (the painted body)")),
        p("ref", REFS),
    ],
    example: include_str!("examples.rs"),
};

pub const CHAT_COMPOSER_INPUT_DOC: ComponentDoc = ComponentDoc {
    name: "ChatComposerInput",
    rust_type: "astryx::components::chat::ChatComposerInput",
    props: &[
        p(
            "handleRef",
            RustApi::Method("handle (a ChatComposerInputHandle)"),
        ),
        p("value", RustApi::Method("value")),
        p("onChange", RustApi::Method("on_change")),
        p("placeholder", RustApi::Method("placeholder")),
        p("maxRows", RustApi::Method("max_rows")),
        p(
            "triggers",
            RustApi::Method("triggers (ChatComposerTrigger)"),
        ),
        p("debounceMs", RustApi::Method("debounce")),
        p("hasHistory", RustApi::Method("history")),
        p("label", RustApi::Method("label")),
        p("isDisabled", RustApi::Method("disabled")),
        p("onPaste", RustApi::Method("on_paste")),
        p(
            "pasteAsToken",
            RustApi::Method("paste_as_token (use_chat_paste_as_token) / no_paste_as_token"),
        ),
        p("onFiles", RustApi::Method("on_files (Vec<PickedFile>)")),
        p("onSubmit", RustApi::Method("on_submit")),
        p(
            "onKeyDown",
            RustApi::Method("on_key_down (window.prevent_default() cancels the built-in keys)"),
        ),
        p(
            "enterKeyHint",
            RustApi::Method("enter_key_hint (gpui::TextInputAction)"),
        ),
        p("xstyle", RustApi::Trait("Styled")),
        p("ref", REFS),
    ],
    example: include_str!("examples.rs"),
};

pub const CHAT_COMPOSER_TOKEN_ELEMENT_DOC: ComponentDoc = ComponentDoc {
    name: "ChatComposerTokenElement",
    rust_type: "astryx::components::chat::ChatComposerTokenElement",
    props: &[
        p(
            "token",
            RustApi::Argument("token (ChatComposerToken::badge or ::custom)"),
        ),
        p("xstyle", RustApi::Trait("Styled")),
        p("ref", REFS),
    ],
    example: include_str!("examples.rs"),
};

pub const CHAT_COMPOSER_DRAWER_DOC: ComponentDoc = ComponentDoc {
    name: "ChatComposerDrawer",
    rust_type: "astryx::components::chat::ChatComposerDrawer",
    props: &[
        p("children", RustApi::Trait("ParentElement")),
        p("count", RustApi::Method("count")),
        p("label", RustApi::Method("label")),
        p("collapsedSummary", RustApi::Method("collapsed_summary")),
        p("isCollapsed", RustApi::Method("collapsed")),
        p("defaultIsCollapsed", RustApi::Method("default_collapsed")),
        p("onCollapsedChange", RustApi::Method("on_collapsed_change")),
        p("xstyle", RustApi::Trait("Styled")),
        p("ref", REFS),
        p("data-testid", TEST_IDS),
    ],
    example: include_str!("examples.rs"),
};

pub const CHAT_SEND_BUTTON_DOC: ComponentDoc = ComponentDoc {
    name: "ChatSendButton",
    rust_type: "astryx::components::chat::ChatSendButton",
    props: &[
        p("isStopShown", RustApi::Method("stop_shown")),
        p("isDisabled", RustApi::Method("disabled")),
        p("onSend", RustApi::Method("on_send")),
        p("onStop", RustApi::Method("on_stop")),
        p("sendIcon", RustApi::Method("send_icon")),
        p("stopIcon", RustApi::Method("stop_icon")),
        p("size", RustApi::Method("size")),
        p("xstyle", RustApi::Trait("Styled")),
        p("onClick", RustApi::Method("on_click")),
        p("ref", REFS),
    ],
    example: include_str!("examples.rs"),
};

pub const CHAT_TOKENIZED_TEXT_DOC: ComponentDoc = ComponentDoc {
    name: "ChatTokenizedText",
    rust_type: "astryx::components::chat::ChatTokenizedText",
    props: &[
        p("children", RustApi::Argument("text")),
        p("tokens", RustApi::Method("tokens")),
        p("xstyle", RustApi::Trait("Styled")),
        p("ref", REFS),
    ],
    example: include_str!("examples.rs"),
};

pub const USE_CHAT_PASTE_AS_TOKEN_DOC: ComponentDoc = ComponentDoc {
    name: "useChatPasteAsToken",
    rust_type: "astryx::components::chat::use_chat_paste_as_token",
    props: &[],
    example: include_str!("examples.rs"),
};

/// The Chat family's own doc: its parts are the components above.
pub const CHAT_DOC: ComponentDoc = ComponentDoc {
    name: "Chat",
    rust_type: "astryx::components::chat",
    props: &[],
    example: include_str!("examples.rs"),
};
