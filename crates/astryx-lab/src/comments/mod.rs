//! Upstream lab `Comments`: the comments on one item (`Comments`), one thread on
//! its own (`CommentThread`) and the comment composer (`CommentComposer`), split
//! into files as upstream's `Comments/` folder is.

pub mod actions;
// Upstream names the panel's file after the folder: `Comments/Comments.tsx`.
#[allow(clippy::module_inception)]
pub mod comments;
pub mod composer;
pub mod context;
pub(crate) mod dismissal;
pub mod examples;
pub mod i18n;
pub mod icons;
pub mod markdown;
pub mod parts;
pub(crate) mod row;
pub mod shared;
pub mod thread;
pub mod types;
pub mod utils;

#[cfg(test)]
mod tests;

use astryx_components::doc::{ComponentDoc, PropMapping, RustApi};

pub use comments::{
    Comments, CommentsActionFn, CommentsComposerPosition, CommentsDensity, CommentsLayout,
};
pub use composer::CommentComposer;
pub use shared::CommentThreadShared;
pub use thread::CommentThread;
pub use types::*;

/// Upstream `renderEvent`: replaces the default activity event row.
pub type RenderEventFn = std::rc::Rc<
    dyn Fn(&types::CommentActivityEvent, &mut gpui::Window, &mut gpui::App) -> gpui::AnyElement,
>;

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

/// The props every surface shares (upstream `CommentThreadSharedProps`), through
/// [`CommentThreadShared`].
macro_rules! shared {
    (current_user) => {
        p(
            "currentUser",
            RustApi::Method("current_user (CommentThreadShared)"),
        )
    };
    (mode) => {
        p("mode", RustApi::Method("mode (CommentThreadShared)"))
    };
    (container) => {
        p(
            "container",
            RustApi::Method("container (CommentThreadShared)"),
        )
    };
    (composer_placeholder) => {
        p(
            "composerPlaceholder",
            RustApi::Method("composer_placeholder (CommentThreadShared)"),
        )
    };
    (highlight_color) => {
        p(
            "highlightColor",
            RustApi::Method("highlight_color (CommentThreadShared)"),
        )
    };
    (submitting) => {
        p(
            "isSubmitting",
            RustApi::Method("submitting (CommentThreadShared)"),
        )
    };
    (on_composer_draft_change) => {
        p(
            "onComposerDraftChange",
            RustApi::Method("on_composer_draft_change (CommentThreadShared)"),
        )
    };
    (reaction_options) => {
        p(
            "reactionOptions",
            RustApi::Method("reaction_options (CommentThreadShared)"),
        )
    };
    (on_submit) => {
        p(
            "onSubmit",
            RustApi::Method("on_submit (CommentThreadShared)"),
        )
    };
    (on_save_edit) => {
        p(
            "onSaveEdit",
            RustApi::Method("on_save_edit (CommentThreadShared)"),
        )
    };
    (on_delete) => {
        p(
            "onDelete",
            RustApi::Method("on_delete (CommentThreadShared)"),
        )
    };
    (on_retry_comment) => {
        p(
            "onRetryComment",
            RustApi::Method("on_retry_comment (CommentThreadShared)"),
        )
    };
    (on_react) => {
        p("onReact", RustApi::Method("on_react (CommentThreadShared)"))
    };
    (on_resolve_thread) => {
        p(
            "onResolveThread",
            RustApi::Method("on_resolve_thread (CommentThreadShared)"),
        )
    };
    (on_reopen_thread) => {
        p(
            "onReopenThread",
            RustApi::Method("on_reopen_thread (CommentThreadShared)"),
        )
    };
    (load_versions) => {
        p(
            "loadVersions",
            RustApi::Method("load_versions (CommentThreadShared)"),
        )
    };
    (on_upload_images) => {
        p(
            "onUploadImages",
            RustApi::Method("on_upload_images (CommentThreadShared)"),
        )
    };
    (composer_triggers) => {
        p(
            "composerTriggers",
            RustApi::Method("composer_triggers (CommentThreadShared)"),
        )
    };
    (agent_commands) => {
        p(
            "agentCommands",
            RustApi::Method("agent_commands (CommentThreadShared)"),
        )
    };
    (markdown_plugins) => {
        p(
            "markdownPlugins",
            RustApi::Method("markdown_plugins (CommentThreadShared)"),
        )
    };
    (markdown_inline_plugins) => {
        p(
            "markdownInlinePlugins",
            RustApi::Method("markdown_inline_plugins (CommentThreadShared)"),
        )
    };
    (get_reply_mention) => {
        p(
            "getReplyMention",
            RustApi::Method("get_reply_mention (CommentThreadShared)"),
        )
    };
    (emoji_picker) => {
        p(
            "hasEmojiPicker",
            RustApi::Method("emoji_picker (CommentThreadShared)"),
        )
    };
    (render_anchor) => {
        p(
            "renderAnchor",
            RustApi::Method("render_anchor (CommentThreadShared)"),
        )
    };
    (render_body) => {
        p(
            "renderBody",
            RustApi::Method("render_body (CommentThreadShared)"),
        )
    };
    (render_version_body) => {
        p(
            "renderVersionBody",
            RustApi::Method("render_version_body (CommentThreadShared)"),
        )
    };
    (render_additional_actions) => {
        p(
            "renderAdditionalActions",
            RustApi::Method("render_additional_actions (CommentThreadShared)"),
        )
    };
    (render_composer) => {
        p(
            "renderComposer",
            RustApi::Method("render_composer (CommentThreadShared)"),
        )
    };
}

/// Upstream lab `Comments`'s Rust documentation.
pub const COMMENTS_DOC: ComponentDoc = ComponentDoc {
    name: "Comments",
    rust_type: "astryx::lab::comments::Comments",
    props: &[
        p(
            "comments",
            RustApi::Argument("comments (CommentThreadData)"),
        ),
        p("events", RustApi::Method("events")),
        shared!(current_user),
        shared!(mode),
        p("label", RustApi::Method("label")),
        p("composerPosition", RustApi::Method("composer_position")),
        p("density", RustApi::Method("density")),
        shared!(container),
        p("layout", RustApi::Method("layout")),
        shared!(composer_placeholder),
        p("selectedThreadId", RustApi::Method("selected_thread_id")),
        p(
            "onSelectedThreadChange",
            RustApi::Method("on_selected_thread_change"),
        ),
        p("composerAnchor", RustApi::Method("composer_anchor")),
        p(
            "onComposerAnchorDismiss",
            RustApi::Method("on_composer_anchor_dismiss"),
        ),
        p("composerControls", RustApi::Method("composer_controls")),
        shared!(highlight_color),
        p("isLoading", RustApi::Method("loading")),
        p("loadError", RustApi::Method("load_error")),
        p("onRetryLoad", RustApi::Method("on_retry_load")),
        p("hasOlderComments", RustApi::Method("has_older_comments")),
        p("isLoadingOlder", RustApi::Method("loading_older")),
        p("onLoadOlder", RustApi::Method("on_load_older")),
        shared!(submitting),
        shared!(on_composer_draft_change),
        p("composerError", RustApi::Method("composer_error")),
        shared!(reaction_options),
        shared!(on_submit),
        shared!(on_save_edit),
        shared!(on_delete),
        shared!(on_retry_comment),
        shared!(on_react),
        shared!(on_resolve_thread),
        shared!(on_reopen_thread),
        shared!(load_versions),
        shared!(on_upload_images),
        shared!(composer_triggers),
        shared!(agent_commands),
        shared!(markdown_plugins),
        shared!(markdown_inline_plugins),
        shared!(get_reply_mention),
        shared!(emoji_picker),
        p("emptyState", RustApi::Method("empty_state")),
        shared!(render_anchor),
        shared!(render_body),
        shared!(render_version_body),
        p("renderEvent", RustApi::Method("render_event")),
        shared!(render_additional_actions),
        shared!(render_composer),
    ],
    example: include_str!("examples.rs"),
};

/// Upstream lab `CommentThread`'s Rust documentation.
pub const COMMENT_THREAD_DOC: ComponentDoc = ComponentDoc {
    name: "CommentThread",
    rust_type: "astryx::lab::comments::CommentThread",
    props: &[
        p("comment", RustApi::Argument("comment (CommentThreadData)")),
        p("isSelected", RustApi::Method("selected")),
        p("onSelectedChange", RustApi::Method("on_selected_change")),
        p("onDismiss", RustApi::Method("on_dismiss")),
        shared!(current_user),
        shared!(mode),
        shared!(container),
        shared!(on_submit),
        shared!(on_composer_draft_change),
        shared!(on_save_edit),
        shared!(on_delete),
        shared!(on_react),
        shared!(on_retry_comment),
        shared!(on_resolve_thread),
        shared!(on_reopen_thread),
        shared!(load_versions),
        shared!(on_upload_images),
        shared!(submitting),
        shared!(composer_placeholder),
        shared!(highlight_color),
        shared!(reaction_options),
        shared!(agent_commands),
        shared!(composer_triggers),
        shared!(markdown_plugins),
        shared!(markdown_inline_plugins),
        shared!(get_reply_mention),
        shared!(emoji_picker),
        shared!(render_anchor),
        shared!(render_body),
        shared!(render_version_body),
        shared!(render_additional_actions),
        shared!(render_composer),
    ],
    example: include_str!("examples.rs"),
};

/// Upstream lab `CommentComposer`'s Rust documentation.
pub const COMMENT_COMPOSER_DOC: ComponentDoc = ComponentDoc {
    name: "CommentComposer",
    rust_type: "astryx::lab::comments::CommentComposer",
    props: &[
        p("value", RustApi::Argument("value")),
        p("onChange", RustApi::Argument("on_change")),
        p("onSubmit", RustApi::Argument("on_submit")),
        p("placeholder", RustApi::Method("placeholder")),
        p("label", RustApi::Method("label")),
        p("isSubmitting", RustApi::Method("submitting")),
        p("error", RustApi::Method("error")),
        p("onRetry", RustApi::Method("on_retry")),
        p("anchor", RustApi::Method("anchor")),
        p("onAnchorDismiss", RustApi::Method("on_anchor_dismiss")),
        p("highlightColor", RustApi::Method("highlight_color")),
        p("onUploadImages", RustApi::Method("on_upload_images")),
        p("triggers", RustApi::Method("triggers")),
        p("agentCommands", RustApi::Method("agent_commands")),
        p("hasEmojiPicker", RustApi::Method("emoji_picker")),
        p("controls", RustApi::Method("controls")),
    ],
    example: include_str!("examples.rs"),
};
