//! The Chat family's lab parts (upstream lab `Chat/`): `ChatTypingIndicator`,
//! `ChatUnreadDivider`, `ChatVirtualizer`, `ChatEmojiPicker` and `ChatReactionBar`. They sit
//! beside Core's Chat (`rustryx_components::chat`) as upstream's do: the typing indicator
//! above the composer, the unread divider and the virtualized rows inside a ChatLayout's
//! transcript, the reaction bar under a ChatMessage, its emoji picker on its add button.

mod emoji_picker;
pub mod examples;
mod reaction_bar;
mod typing_indicator;
mod unread_divider;
mod virtualizer;
mod virtualizer_engine;
mod virtualizer_model;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod virtualizer_tests;

pub use emoji_picker::{
    ChatEmojiOption, ChatEmojiPicker, GRID_COLUMNS, default_chat_emojis, no_chat_emoji_match,
    visible_chat_emojis,
};
pub use reaction_bar::{ChatReaction, ChatReactionBar, pill_label};
pub use typing_indicator::{
    ChatTypingIndicator, DOT_DELAYS_MS, dot_frame, long_conjunction, typing_label,
};
pub use unread_divider::{ChatUnreadDivider, unread_divider_name};
pub use virtualizer::{ChatVirtualizer, ChatVirtualizerHandle, ItemSize};

use rustryx_components::doc::{ComponentDoc, PropMapping, RustApi};

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

/// Upstream lab `ChatTypingIndicator`.
pub const CHAT_TYPING_INDICATOR_DOC: ComponentDoc = ComponentDoc {
    name: "ChatTypingIndicator",
    rust_type: "rustryx::lab::chat::ChatTypingIndicator",
    props: &[p("names", RustApi::Method("names"))],
    example: include_str!("examples.rs"),
};

/// Upstream lab `ChatUnreadDivider`.
pub const CHAT_UNREAD_DIVIDER_DOC: ComponentDoc = ComponentDoc {
    name: "ChatUnreadDivider",
    rust_type: "rustryx::lab::chat::ChatUnreadDivider",
    props: &[p("label", RustApi::Method("label"))],
    example: include_str!("examples.rs"),
};

/// Upstream lab `ChatVirtualizer`.
pub const CHAT_VIRTUALIZER_DOC: ComponentDoc = ComponentDoc {
    name: "ChatVirtualizer",
    rust_type: "rustryx::lab::chat::ChatVirtualizer",
    props: &[
        p("data", RustApi::Argument("data")),
        p("getRowKey", RustApi::Argument("get_row_key")),
        p("renderItem", RustApi::Argument("render_item")),
        p("estimatedItemSize", RustApi::Method("estimated_item_size")),
        p("handleRef", RustApi::Method("handle")),
        p("getItemType", RustApi::Method("get_item_type")),
        p("scrollElement", RustApi::Method("scroll_element")),
        p("endThreshold", RustApi::Method("end_threshold")),
        p("overscanTop", RustApi::Method("overscan_top")),
        p("overscanBottom", RustApi::Method("overscan_bottom")),
        p("label", RustApi::Method("label")),
        p("style", RustApi::Trait("Styled")),
        p("className", RustApi::Trait("Styled")),
    ],
    example: include_str!("examples.rs"),
};

/// Upstream lab `ChatEmojiPicker`.
pub const CHAT_EMOJI_PICKER_DOC: ComponentDoc = ComponentDoc {
    name: "ChatEmojiPicker",
    rust_type: "rustryx::lab::chat::ChatEmojiPicker",
    props: &[
        p("onSelect", RustApi::Method("on_select")),
        p("emojis", RustApi::Method("emojis")),
        p("label", RustApi::Method("label")),
        p("searchLabel", RustApi::Method("search_label")),
        p("children", RustApi::Method("child")),
    ],
    example: include_str!("examples.rs"),
};

/// Upstream lab `ChatReactionBar`.
pub const CHAT_REACTION_BAR_DOC: ComponentDoc = ComponentDoc {
    name: "ChatReactionBar",
    rust_type: "rustryx::lab::chat::ChatReactionBar",
    props: &[
        p("reactions", RustApi::Argument("reactions")),
        p("onToggle", RustApi::Method("on_toggle")),
        p("onAdd", RustApi::Method("on_add")),
        p("emojis", RustApi::Method("emojis")),
        p("addLabel", RustApi::Method("add_label")),
        p("label", RustApi::Method("label")),
    ],
    example: include_str!("examples.rs"),
};
