//! The setters every surface of the family shares (upstream lab
//! `Comments/types.ts`'s `CommentThreadSharedProps`), for Comments and a standalone
//! CommentThread.

use std::rc::Rc;

use gpui::{AnyElement, App, SharedString, Task, Window};
use rustryx_components::chat::ChatComposerTrigger;
use rustryx_components::input::PickedFile;
use rustryx_components::markdown::MarkdownInlinePlugin;
use rustryx_markdown::plugins::MarkdownPlugin;

use super::types::{
    CommentComposerContext, CommentComposerRenderProps, CommentContainer, CommentData,
    CommentHighlightColor, CommentMode, CommentReactionOption, CommentTask, CommentThreadData,
    CommentThreadSharedProps, CommentUploadedImage, CommentUser, CommentVersion,
};

/// The builder methods for upstream `CommentThreadSharedProps`, which Comments and
/// CommentThread both take: import it to call them.
pub trait CommentThreadShared: Sized {
    /// The props these setters fill.
    fn shared_props(&mut self) -> &mut CommentThreadSharedProps;

    /// Upstream `currentUser`: the viewer, whose comments they may edit and delete.
    fn current_user(mut self, user: CommentUser) -> Self {
        self.shared_props().current_user = Some(user);
        self
    }

    /// Upstream `mode` (`Interactive` by default).
    fn mode(mut self, mode: CommentMode) -> Self {
        self.shared_props().mode = mode;
        self
    }

    /// Upstream `container`: the parent surface (`Section` by default).
    fn container(mut self, container: CommentContainer) -> Self {
        self.shared_props().container = container;
        self
    }

    /// Upstream `composerPlaceholder`: the reply composer's placeholder (Comments' main
    /// composer too).
    fn composer_placeholder(mut self, placeholder: impl Into<SharedString>) -> Self {
        self.shared_props().composer_placeholder = Some(placeholder.into());
        self
    }

    /// Upstream `highlightColor`: anchored passages' highlight (yellow by default).
    fn highlight_color(mut self, color: CommentHighlightColor) -> Self {
        self.shared_props().highlight_color = color;
        self
    }

    /// Upstream `reactionOptions`: the picker's reactions
    /// ([`default_comment_reactions`](super::types::default_comment_reactions) by
    /// default).
    fn reaction_options(
        mut self,
        options: impl IntoIterator<Item = CommentReactionOption>,
    ) -> Self {
        self.shared_props().reaction_options = Some(options.into_iter().collect());
        self
    }

    /// Upstream `agentCommands`: agents a comment can address with `/`.
    fn agent_commands(
        mut self,
        agents: impl IntoIterator<Item = super::types::CommentAgentCommand>,
    ) -> Self {
        self.shared_props().agent_commands = agents.into_iter().collect();
        self
    }

    /// Upstream `composerTriggers`: typeahead triggers for every composer.
    fn composer_triggers(
        mut self,
        triggers: impl IntoIterator<Item = ChatComposerTrigger>,
    ) -> Self {
        self.shared_props().composer_triggers = triggers.into_iter().collect();
        self
    }

    /// Upstream `markdownPlugins`: block plugins for comment bodies.
    fn markdown_plugins(mut self, plugins: impl IntoIterator<Item = MarkdownPlugin>) -> Self {
        self.shared_props().markdown_plugins = plugins.into_iter().collect();
        self
    }

    /// Upstream `markdownInlinePlugins`: inline plugins for comment bodies.
    fn markdown_inline_plugins(
        mut self,
        plugins: impl IntoIterator<Item = MarkdownInlinePlugin>,
    ) -> Self {
        self.shared_props().markdown_inline_plugins = plugins.into_iter().collect();
        self
    }

    /// Upstream `getReplyMention`: the mention a reply to a reply starts with.
    fn get_reply_mention(
        mut self,
        f: impl Fn(&CommentUser) -> Option<SharedString> + 'static,
    ) -> Self {
        self.shared_props().get_reply_mention = Some(Rc::new(f));
        self
    }

    /// Upstream `hasEmojiPicker`: the composers' emoji button (on by default).
    fn emoji_picker(mut self, on: bool) -> Self {
        self.shared_props().has_emoji_picker = Some(on);
        self
    }

    /// Upstream `isSubmitting`: a post in flight the host tracks.
    fn submitting(mut self, submitting: bool) -> Self {
        self.shared_props().is_submitting = submitting;
        self
    }

    /// Upstream `onComposerDraftChange`: every composer draft change, with its context.
    fn on_composer_draft_change(
        mut self,
        f: impl Fn(SharedString, CommentComposerContext, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.shared_props().on_composer_draft_change = Some(Rc::new(f));
        self
    }

    /// Upstream `onSubmit`: posts a comment (`None` parent) or a reply under a thread's
    /// first comment. A task that fails keeps the draft and shows an error.
    fn on_submit(
        mut self,
        f: impl Fn(SharedString, Option<SharedString>, &mut Window, &mut App) -> CommentTask + 'static,
    ) -> Self {
        self.shared_props().on_submit = Some(Rc::new(f));
        self
    }

    /// Upstream `onSaveEdit`: saves an edited comment's body (id, body).
    fn on_save_edit(
        mut self,
        f: impl Fn(SharedString, SharedString, &mut Window, &mut App) -> CommentTask + 'static,
    ) -> Self {
        self.shared_props().on_save_edit = Some(Rc::new(f));
        self
    }

    /// Upstream `onDelete`: deletes a comment, after the viewer confirms.
    fn on_delete(
        mut self,
        f: impl Fn(SharedString, &mut Window, &mut App) -> CommentTask + 'static,
    ) -> Self {
        self.shared_props().on_delete = Some(Rc::new(f));
        self
    }

    /// Upstream `onReact`: toggles the viewer's reaction (comment id, reaction key, and
    /// the option when picked).
    fn on_react(
        mut self,
        f: impl Fn(
            SharedString,
            SharedString,
            Option<CommentReactionOption>,
            &mut Window,
            &mut App,
        ) -> CommentTask
        + 'static,
    ) -> Self {
        self.shared_props().on_react = Some(Rc::new(f));
        self
    }

    /// Upstream `onRetryComment`: sends a failed comment again.
    fn on_retry_comment(
        mut self,
        f: impl Fn(SharedString, &mut Window, &mut App) -> CommentTask + 'static,
    ) -> Self {
        self.shared_props().on_retry_comment = Some(Rc::new(f));
        self
    }

    /// Upstream `onResolveThread`: adds Mark as resolved to open threads.
    fn on_resolve_thread(
        mut self,
        f: impl Fn(SharedString, &mut Window, &mut App) -> CommentTask + 'static,
    ) -> Self {
        self.shared_props().on_resolve_thread = Some(Rc::new(f));
        self
    }

    /// Upstream `onReopenThread`: adds Reopen to resolved threads.
    fn on_reopen_thread(
        mut self,
        f: impl Fn(SharedString, &mut Window, &mut App) -> CommentTask + 'static,
    ) -> Self {
        self.shared_props().on_reopen_thread = Some(Rc::new(f));
        self
    }

    /// Upstream `loadVersions`: an edited comment's earlier versions, newest first, for
    /// its history.
    fn load_versions(
        mut self,
        f: impl Fn(SharedString, &mut Window, &mut App) -> Task<Result<Vec<CommentVersion>, ()>>
        + 'static,
    ) -> Self {
        self.shared_props().load_versions = Some(Rc::new(f));
        self
    }

    /// Upstream `onUploadImages`: uploads images a composer attached, pasted or
    /// dropped, and adds the attach button.
    fn on_upload_images(
        mut self,
        f: impl Fn(
            Vec<PickedFile>,
            CommentComposerContext,
            &mut Window,
            &mut App,
        ) -> Task<Result<Vec<CommentUploadedImage>, ()>>
        + 'static,
    ) -> Self {
        self.shared_props().on_upload_images = Some(Rc::new(f));
        self
    }

    /// Upstream `renderAnchor`: replaces a thread's quoted passage (given the default).
    fn render_anchor(
        mut self,
        f: impl Fn(&CommentThreadData, AnyElement, &mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        self.shared_props().render_anchor = Some(Rc::new(f));
        self
    }

    /// Upstream `renderBody`: replaces a comment's Markdown body.
    fn render_body(
        mut self,
        f: impl Fn(&CommentData, &mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        self.shared_props().render_body = Some(Rc::new(f));
        self
    }

    /// Upstream `renderVersionBody`: replaces a version's body in the history.
    fn render_version_body(
        mut self,
        f: impl Fn(&CommentVersion, &mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        self.shared_props().render_version_body = Some(Rc::new(f));
        self
    }

    /// Upstream `renderAdditionalActions`: product actions before a comment's more
    /// menu.
    fn render_additional_actions(
        mut self,
        f: impl Fn(&CommentData, &mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        self.shared_props().render_additional_actions = Some(Rc::new(f));
        self
    }

    /// Upstream `renderComposer`: replaces every composer's input surface.
    fn render_composer(
        mut self,
        f: impl Fn(CommentComposerRenderProps, &mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        self.shared_props().render_composer = Some(Rc::new(f));
        self
    }
}
