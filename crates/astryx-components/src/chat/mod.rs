//! The Chat family. Upstream: `Chat/` (component specs under `Chat/`). M10 stage 5 ports
//! the layout and messages; the composer, tool calls and dictation follow in stages 6
//! and 7.

mod bubble;
mod context;
pub mod examples;
mod layout;
mod message;
mod message_list;
mod metadata;
mod new_messages;
mod scroll_button;
mod stream_scroll;
mod system_message;

#[cfg(test)]
mod tests;

pub use bubble::{ChatBubbleGroup, ChatMessageBubble, ChatMessageBubbleVariant};
pub use context::{ChatDensity, ChatLayoutContext, ChatMessageSender, use_chat_layout_context};
pub use layout::ChatLayout;
pub use message::ChatMessage;
pub use message_list::{ChatListAlign, ChatMessageList};
pub use metadata::{ChatMessageMetadata, ChatMessageStatus};
pub use new_messages::{ChatNewMessages, use_chat_new_messages};
pub use scroll_button::ChatLayoutScrollButton;
pub use stream_scroll::{
    ChatScrollBehavior, ChatScrollToBottomOptions, ChatStreamScroll, UseChatStreamScrollOptions,
    use_chat_stream_scroll,
};
pub use system_message::{ChatSystemMessage, ChatSystemMessageVariant};

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
