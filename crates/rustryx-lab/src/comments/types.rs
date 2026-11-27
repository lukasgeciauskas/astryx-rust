//! The Comments family's data and callback types (upstream lab `Comments/types.ts`).

use std::rc::Rc;

use gpui::{AnyElement, App, SharedString, Task, Window};
use rustryx_components::chat::ChatComposerTrigger;
use rustryx_components::input::PickedFile;
use rustryx_components::markdown::MarkdownInlinePlugin;
use rustryx_markdown::plugins::MarkdownPlugin;

/// Upstream `CommentMode`: `Interactive` enables every permitted action;
/// `NonEditable` keeps create, reply and react but removes edit and delete; `ReadOnly`
/// removes every control that changes data.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CommentMode {
    #[default]
    Interactive,
    NonEditable,
    ReadOnly,
}

/// Upstream `CommentStatus`: where a comment is in being posted, in Chat's
/// message-status vocabulary.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CommentStatus {
    /// The post is in flight.
    Sending,
    /// It has posted (the default).
    #[default]
    Sent,
    /// It couldn't be posted.
    Error,
}

/// Upstream `CommentUser.kind`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CommentUserKind {
    #[default]
    Person,
    /// An agent shows its logo, or square-cornered initials without one, and an Agent
    /// badge beside its name.
    Agent,
}

/// Upstream `CommentUser`: a person or agent who writes comments, reacts, or acts on
/// the item.
#[derive(Clone, Debug, PartialEq)]
pub struct CommentUser {
    /// Stable host-owned id, used for equality ("is this my comment?").
    pub id: SharedString,
    /// Full display name. Never a viewer-relative label such as "You".
    pub name: SharedString,
    pub kind: CommentUserKind,
    /// Where the author's name links, such as their profile.
    pub href: Option<SharedString>,
    /// Profile picture, or an agent's square logo. Initials without one.
    pub avatar_url: Option<SharedString>,
}

impl CommentUser {
    /// A person with an id and a name.
    pub fn new(id: impl Into<SharedString>, name: impl Into<SharedString>) -> Self {
        CommentUser {
            id: id.into(),
            name: name.into(),
            kind: CommentUserKind::Person,
            href: None,
            avatar_url: None,
        }
    }

    /// An agent with an id and a name.
    pub fn agent(id: impl Into<SharedString>, name: impl Into<SharedString>) -> Self {
        CommentUser {
            kind: CommentUserKind::Agent,
            ..Self::new(id, name)
        }
    }

    /// Upstream `href`.
    pub fn href(mut self, href: impl Into<SharedString>) -> Self {
        self.href = Some(href.into());
        self
    }

    /// Upstream `avatarUrl`.
    pub fn avatar_url(mut self, url: impl Into<SharedString>) -> Self {
        self.avatar_url = Some(url.into());
        self
    }
}

/// Upstream `CommentReactionOption`: a selectable reaction.
#[derive(Clone, Debug, PartialEq)]
pub struct CommentReactionOption {
    /// Stable key the host stores, such as `like`.
    pub key: SharedString,
    /// Emoji or compact glyph shown in the picker and on the reaction.
    pub label: SharedString,
    /// Human-readable name for the accessible label and tooltip; the keys of
    /// [`DEFAULT_COMMENT_REACTIONS`] take their localized names without one.
    pub description: Option<SharedString>,
}

impl CommentReactionOption {
    /// A reaction with a key and its glyph.
    pub fn new(key: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        CommentReactionOption {
            key: key.into(),
            label: label.into(),
            description: None,
        }
    }

    /// Upstream `description`.
    pub fn description(mut self, description: impl Into<SharedString>) -> Self {
        self.description = Some(description.into());
        self
    }
}

/// Upstream `CommentReaction`: a reaction summary on a comment.
#[derive(Clone, Debug, PartialEq)]
pub struct CommentReaction {
    pub option: CommentReactionOption,
    pub count: u64,
    /// Whether the viewer is one of the people who reacted.
    pub reacted_by_viewer: bool,
    /// Who reacted (id and name), named in the reaction's tooltip.
    pub actors: Vec<(SharedString, SharedString)>,
}

impl CommentReaction {
    /// A reaction with its count.
    pub fn new(key: impl Into<SharedString>, label: impl Into<SharedString>, count: u64) -> Self {
        CommentReaction {
            option: CommentReactionOption::new(key, label),
            count,
            reacted_by_viewer: false,
            actors: Vec::new(),
        }
    }

    /// Upstream `reactedByViewer`.
    pub fn reacted_by_viewer(mut self, reacted: bool) -> Self {
        self.reacted_by_viewer = reacted;
        self
    }

    /// Upstream `actors`.
    pub fn actors(
        mut self,
        actors: impl IntoIterator<Item = (impl Into<SharedString>, impl Into<SharedString>)>,
    ) -> Self {
        self.actors = actors
            .into_iter()
            .map(|(id, name)| (id.into(), name.into()))
            .collect();
        self
    }

    /// Upstream `description`.
    pub fn description(mut self, description: impl Into<SharedString>) -> Self {
        self.option.description = Some(description.into());
        self
    }
}

/// Upstream `CommentAnchor.status`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CommentAnchorStatus {
    #[default]
    Anchored,
    /// Edits removed the passage: the thread stays, and its quote shows with an
    /// Outdated badge.
    Orphaned,
}

/// Upstream `CommentAnchor`: the passage an inline comment discusses. The host owns
/// anchoring; the thread shows the quote and its state.
#[derive(Clone, Debug, PartialEq)]
pub struct CommentAnchor {
    /// The selected text, quoted above the comment.
    pub quote: SharedString,
    pub status: CommentAnchorStatus,
}

impl CommentAnchor {
    /// An anchored passage.
    pub fn new(quote: impl Into<SharedString>) -> Self {
        CommentAnchor {
            quote: quote.into(),
            status: CommentAnchorStatus::Anchored,
        }
    }

    /// A passage edits removed.
    pub fn orphaned(mut self) -> Self {
        self.status = CommentAnchorStatus::Orphaned;
        self
    }
}

/// Upstream `CommentHighlightColor`: a color of the Astryx palette, as a host
/// highlights inline comments' passages in it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CommentHighlightColor {
    Blue,
    Cyan,
    Gray,
    Green,
    Orange,
    Pink,
    Purple,
    Red,
    Teal,
    #[default]
    Yellow,
}

impl CommentHighlightColor {
    /// Upstream's name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Blue => "blue",
            Self::Cyan => "cyan",
            Self::Gray => "gray",
            Self::Green => "green",
            Self::Orange => "orange",
            Self::Pink => "pink",
            Self::Purple => "purple",
            Self::Red => "red",
            Self::Teal => "teal",
            Self::Yellow => "yellow",
        }
    }
}

/// Upstream `CommentUploadedImage`: an image the host's upload flow stored.
#[derive(Clone, Debug, PartialEq)]
pub struct CommentUploadedImage {
    /// URL of the stored image.
    pub url: SharedString,
    /// Description used as the image's alt text.
    pub alt: SharedString,
}

/// A `createdAt` or `editedAt` value: a date string, or a number of milliseconds (or
/// seconds, below 10^12).
#[derive(Clone, Debug, PartialEq)]
pub enum CommentTime {
    Text(SharedString),
    Number(f64),
}

impl From<&str> for CommentTime {
    fn from(value: &str) -> Self {
        CommentTime::Text(value.to_string().into())
    }
}

impl From<String> for CommentTime {
    fn from(value: String) -> Self {
        CommentTime::Text(value.into())
    }
}

impl From<f64> for CommentTime {
    fn from(value: f64) -> Self {
        CommentTime::Number(value)
    }
}

impl From<i64> for CommentTime {
    fn from(value: i64) -> Self {
        CommentTime::Number(value as f64)
    }
}

/// Upstream `CommentVersion`: one version of a comment in its edit history.
#[derive(Clone, Debug, PartialEq)]
pub struct CommentVersion {
    pub id: SharedString,
    pub body: SharedString,
    pub edited_at: CommentTime,
    pub author: Option<CommentUser>,
    pub is_current: bool,
}

impl CommentVersion {
    /// A version with its body and edit time.
    pub fn new(
        id: impl Into<SharedString>,
        body: impl Into<SharedString>,
        edited_at: impl Into<CommentTime>,
    ) -> Self {
        CommentVersion {
            id: id.into(),
            body: body.into(),
            edited_at: edited_at.into(),
            author: None,
            is_current: false,
        }
    }

    /// Upstream `author`.
    pub fn author(mut self, author: CommentUser) -> Self {
        self.author = Some(author);
        self
    }

    /// Upstream `isCurrent`.
    pub fn current(mut self, current: bool) -> Self {
        self.is_current = current;
        self
    }
}

/// Upstream `CommentProgress`: what a comment's author, typically an agent, is doing
/// while it still writes the comment, and where to watch the work.
#[derive(Clone, Debug, PartialEq)]
pub struct CommentProgress {
    /// What the author is doing, as a reader would say it: "Reading the file…".
    pub label: SharedString,
    /// Where to watch the work (label and href), such as the agent's session.
    pub link: Option<(SharedString, SharedString)>,
}

/// Upstream `CommentData`: a comment, a thread's first comment or one of its replies.
#[derive(Clone, Debug, PartialEq)]
pub struct CommentData {
    pub id: SharedString,
    pub author: CommentUser,
    /// Markdown.
    pub body: SharedString,
    pub created_at: CommentTime,
    pub updated_at: Option<CommentTime>,
    /// Shows Edited beside the time; opens the edit history when there is one.
    pub is_edited: bool,
    /// Host-owned unread state: a New badge that fades after five seconds.
    pub is_unread: bool,
    /// Earlier versions, newest first, when the host already has them.
    pub previous_versions: Vec<CommentVersion>,
    /// Host-controlled metadata for people who have seen this comment; not rendered
    /// by default.
    pub seen_by: Vec<CommentUser>,
    pub reactions: Vec<CommentReaction>,
    pub status: CommentStatus,
    /// Shown when `status` is `Error`.
    pub error_message: Option<SharedString>,
    /// Marks a comment its author is still writing.
    pub progress: Option<CommentProgress>,
    /// Per-comment permission (`true` by default); the host stays authoritative.
    pub can_reply: Option<bool>,
    pub can_react: Option<bool>,
    /// The viewer wrote it, by default.
    pub can_edit: Option<bool>,
    /// The viewer wrote it, by default.
    pub can_delete: Option<bool>,
}

impl CommentData {
    /// A comment by `author` with a Markdown `body`.
    pub fn new(
        id: impl Into<SharedString>,
        author: CommentUser,
        body: impl Into<SharedString>,
        created_at: impl Into<CommentTime>,
    ) -> Self {
        CommentData {
            id: id.into(),
            author,
            body: body.into(),
            created_at: created_at.into(),
            updated_at: None,
            is_edited: false,
            is_unread: false,
            previous_versions: Vec::new(),
            seen_by: Vec::new(),
            reactions: Vec::new(),
            status: CommentStatus::Sent,
            error_message: None,
            progress: None,
            can_reply: None,
            can_react: None,
            can_edit: None,
            can_delete: None,
        }
    }
}

/// Builder methods every comment shape shares.
macro_rules! comment_setters {
    () => {
        /// Upstream `isEdited`.
        pub fn edited(mut self, edited: bool) -> Self {
            self.fields().is_edited = edited;
            self
        }
        /// Upstream `isUnread`.
        pub fn unread(mut self, unread: bool) -> Self {
            self.fields().is_unread = unread;
            self
        }
        /// Upstream `previousVersions`.
        pub fn previous_versions(
            mut self,
            versions: impl IntoIterator<Item = CommentVersion>,
        ) -> Self {
            self.fields().previous_versions = versions.into_iter().collect();
            self
        }
        /// Upstream `seenBy`.
        pub fn seen_by(mut self, users: impl IntoIterator<Item = CommentUser>) -> Self {
            self.fields().seen_by = users.into_iter().collect();
            self
        }
        /// Upstream `reactions`.
        pub fn reactions(mut self, reactions: impl IntoIterator<Item = CommentReaction>) -> Self {
            self.fields().reactions = reactions.into_iter().collect();
            self
        }
        /// Upstream `status`.
        pub fn status(mut self, status: CommentStatus) -> Self {
            self.fields().status = status;
            self
        }
        /// Upstream `errorMessage`.
        pub fn error_message(mut self, message: impl Into<SharedString>) -> Self {
            self.fields().error_message = Some(message.into());
            self
        }
        /// Upstream `progress`.
        pub fn progress(
            mut self,
            label: impl Into<SharedString>,
            link: Option<(SharedString, SharedString)>,
        ) -> Self {
            self.fields().progress = Some(CommentProgress {
                label: label.into(),
                link,
            });
            self
        }
        /// Upstream `canReply`.
        pub fn can_reply(mut self, can: bool) -> Self {
            self.fields().can_reply = Some(can);
            self
        }
        /// Upstream `canReact`.
        pub fn can_react(mut self, can: bool) -> Self {
            self.fields().can_react = Some(can);
            self
        }
        /// Upstream `canEdit`.
        pub fn can_edit(mut self, can: bool) -> Self {
            self.fields().can_edit = Some(can);
            self
        }
        /// Upstream `canDelete`.
        pub fn can_delete(mut self, can: bool) -> Self {
            self.fields().can_delete = Some(can);
            self
        }
    };
}

/// The comment a builder writes to.
trait CommentFields {
    fn fields(&mut self) -> &mut CommentData;
}

impl CommentFields for CommentData {
    fn fields(&mut self) -> &mut CommentData {
        self
    }
}

impl CommentFields for CommentThreadData {
    fn fields(&mut self) -> &mut CommentData {
        &mut self.comment
    }
}

impl CommentData {
    comment_setters!();
}

/// Upstream `CommentThreadData`: a thread, its first comment, which carries the
/// thread's replies.
#[derive(Clone, Debug, PartialEq)]
pub struct CommentThreadData {
    /// The first comment.
    pub comment: CommentData,
    /// Other people composing in this thread now (controlled, transient).
    pub typing_users: Vec<CommentUser>,
    /// One level of replies, oldest first.
    pub replies: Vec<CommentData>,
    /// Makes this an inline comment: the passage its thread discusses.
    pub anchor: Option<CommentAnchor>,
    /// A resolved thread: Resolved beside the time, muted text, and Reopen.
    pub is_resolved: bool,
    /// Who resolved the thread, named in the Resolved label's tooltip.
    pub resolved_by: Option<CommentUser>,
    /// Calls the thread out without selecting it: the selected thread's muted fill.
    pub is_highlighted: bool,
    /// Whether the viewer may resolve or reopen this thread (`true` by default).
    pub can_resolve: Option<bool>,
}

impl std::ops::Deref for CommentThreadData {
    type Target = CommentData;

    fn deref(&self) -> &CommentData {
        &self.comment
    }
}

impl CommentThreadData {
    /// A thread from its first comment.
    pub fn new(
        id: impl Into<SharedString>,
        author: CommentUser,
        body: impl Into<SharedString>,
        created_at: impl Into<CommentTime>,
    ) -> Self {
        Self::from(CommentData::new(id, author, body, created_at))
    }

    comment_setters!();

    /// Upstream `typingUsers`.
    pub fn typing_users(mut self, users: impl IntoIterator<Item = CommentUser>) -> Self {
        self.typing_users = users.into_iter().collect();
        self
    }

    /// Upstream `replies`.
    pub fn replies(mut self, replies: impl IntoIterator<Item = CommentData>) -> Self {
        self.replies = replies.into_iter().collect();
        self
    }

    /// Upstream `anchor`.
    pub fn anchor(mut self, anchor: CommentAnchor) -> Self {
        self.anchor = Some(anchor);
        self
    }

    /// Upstream `isResolved`.
    pub fn resolved(mut self, resolved: bool) -> Self {
        self.is_resolved = resolved;
        self
    }

    /// Upstream `resolvedBy`.
    pub fn resolved_by(mut self, user: CommentUser) -> Self {
        self.resolved_by = Some(user);
        self
    }

    /// Upstream `isHighlighted`.
    pub fn highlighted(mut self, highlighted: bool) -> Self {
        self.is_highlighted = highlighted;
        self
    }

    /// Upstream `canResolve`.
    pub fn can_resolve(mut self, can: bool) -> Self {
        self.can_resolve = Some(can);
        self
    }
}

impl From<CommentData> for CommentThreadData {
    fn from(comment: CommentData) -> Self {
        CommentThreadData {
            comment,
            typing_users: Vec::new(),
            replies: Vec::new(),
            anchor: None,
            is_resolved: false,
            resolved_by: None,
            is_highlighted: false,
            can_resolve: None,
        }
    }
}

/// Upstream `CommentActivityEvent`: a timeline row for a status change or other
/// activity on the item.
#[derive(Clone, Debug, PartialEq)]
pub struct CommentActivityEvent {
    pub id: SharedString,
    /// What happened, as a sentence: "Status changed to Ready for review".
    pub label: SharedString,
    pub description: Option<SharedString>,
    pub created_at: CommentTime,
    pub actor: Option<CommentUser>,
    /// Where the event's time links.
    pub href: Option<SharedString>,
}

impl CommentActivityEvent {
    /// An event with what happened and when.
    pub fn new(
        id: impl Into<SharedString>,
        label: impl Into<SharedString>,
        created_at: impl Into<CommentTime>,
    ) -> Self {
        CommentActivityEvent {
            id: id.into(),
            label: label.into(),
            description: None,
            created_at: created_at.into(),
            actor: None,
            href: None,
        }
    }

    /// Upstream `description`.
    pub fn description(mut self, description: impl Into<SharedString>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Upstream `actor`.
    pub fn actor(mut self, actor: CommentUser) -> Self {
        self.actor = Some(actor);
        self
    }

    /// Upstream `href`.
    pub fn href(mut self, href: impl Into<SharedString>) -> Self {
        self.href = Some(href.into());
        self
    }
}

/// Upstream `CommentComposerContext`: which composer a callback came from.
#[derive(Clone, Debug, PartialEq)]
pub enum CommentComposerContext {
    /// A new comment, with the passage it will attach to when there is one.
    Comment { anchor: Option<CommentAnchor> },
    /// An edit of `comment_id`.
    Edit { comment_id: SharedString },
    /// A reply in the thread `parent_comment_id`, answering `replied_to_comment_id`.
    Reply {
        parent_comment_id: SharedString,
        replied_to_comment_id: SharedString,
    },
}

/// A draft callback.
pub type DraftFn = Rc<dyn Fn(SharedString, &mut Window, &mut App)>;
/// An action callback with no arguments.
pub type ActionFn = Rc<dyn Fn(&mut Window, &mut App)>;
/// A callback with a flag: an open state, a selection.
pub type FlagFn = Rc<dyn Fn(bool, &mut Window, &mut App)>;
/// A host task with no outcome but its success (`onLoadOlder`, `onRetryLoad`, a
/// comment action).
pub type TaskFn = Rc<dyn Fn(&mut Window, &mut App) -> CommentTask>;

/// Upstream `CommentComposerRenderProps`: what a `render_composer` replacement
/// receives.
#[derive(Clone)]
pub struct CommentComposerRenderProps {
    pub value: SharedString,
    pub on_change: DraftFn,
    pub on_submit: DraftFn,
    pub placeholder: SharedString,
    pub is_submitting: bool,
    pub error: Option<SharedString>,
    pub context_label: Option<SharedString>,
    pub context: CommentComposerContext,
    pub on_cancel: Option<ActionFn>,
}

/// Upstream `CommentAgentCommand`: an agent a comment can address with a slash
/// command.
#[derive(Clone, Debug, PartialEq)]
pub struct CommentAgentCommand {
    /// The command after the slash: `ask` for `/ask`. Letters, digits, `-` and `_`.
    pub command: SharedString,
    /// The agent's name, on its chip.
    pub name: SharedString,
    /// What asking it does, in the `/` menu.
    pub description: SharedString,
    /// Square agent logo; initials without one.
    pub logo_url: Option<SharedString>,
}

impl CommentAgentCommand {
    /// An agent's command.
    pub fn new(
        command: impl Into<SharedString>,
        name: impl Into<SharedString>,
        description: impl Into<SharedString>,
    ) -> Self {
        CommentAgentCommand {
            command: command.into(),
            name: name.into(),
            description: description.into(),
            logo_url: None,
        }
    }

    /// Upstream `logoUrl`.
    pub fn logo_url(mut self, url: impl Into<SharedString>) -> Self {
        self.logo_url = Some(url.into());
        self
    }
}

/// What a host's mutation returns (upstream's `void | Promise<void>`): a task that
/// resolves `Ok` when it succeeded and `Err` when it failed; `Task::ready(Ok(()))`
/// for a synchronous success.
pub type CommentTask = Task<Result<(), ()>>;

/// Upstream `CommentImageUploader`: uploads images for a composer and returns where
/// they were stored.
pub type CommentImageUploader = Rc<
    dyn Fn(
        Vec<PickedFile>,
        CommentComposerContext,
        &mut Window,
        &mut App,
    ) -> Task<Result<Vec<CommentUploadedImage>, ()>>,
>;

type MaybeTask<A> = Rc<A>;

/// Upstream `onSubmit`: posts a comment, or a reply under the thread's first comment.
pub type SubmitFn =
    MaybeTask<dyn Fn(SharedString, Option<SharedString>, &mut Window, &mut App) -> CommentTask>;
/// A mutation of one comment by id.
pub type CommentIdFn = MaybeTask<dyn Fn(SharedString, &mut Window, &mut App) -> CommentTask>;
/// Upstream `onSaveEdit`.
pub type SaveEditFn =
    MaybeTask<dyn Fn(SharedString, SharedString, &mut Window, &mut App) -> CommentTask>;
/// Upstream `onReact`.
pub type ReactFn = MaybeTask<
    dyn Fn(
        SharedString,
        SharedString,
        Option<CommentReactionOption>,
        &mut Window,
        &mut App,
    ) -> CommentTask,
>;
/// Upstream `loadVersions`.
pub type LoadVersionsFn =
    Rc<dyn Fn(SharedString, &mut Window, &mut App) -> Task<Result<Vec<CommentVersion>, ()>>>;
/// Upstream `onComposerDraftChange`.
pub type DraftChangeFn = Rc<dyn Fn(SharedString, CommentComposerContext, &mut Window, &mut App)>;
/// Upstream `getReplyMention`.
pub type ReplyMentionFn = Rc<dyn Fn(&CommentUser) -> Option<SharedString>>;
/// Upstream `renderAnchor`.
pub type RenderAnchorFn =
    Rc<dyn Fn(&CommentThreadData, AnyElement, &mut Window, &mut App) -> AnyElement>;
/// Upstream `renderBody` and `renderAdditionalActions`.
pub type RenderCommentFn = Rc<dyn Fn(&CommentData, &mut Window, &mut App) -> AnyElement>;
/// Upstream `renderVersionBody`.
pub type RenderVersionFn = Rc<dyn Fn(&CommentVersion, &mut Window, &mut App) -> AnyElement>;
/// Upstream `renderComposer`.
pub type RenderComposerFn =
    Rc<dyn Fn(CommentComposerRenderProps, &mut Window, &mut App) -> AnyElement>;

/// Upstream `CommentThreadSharedProps`: behavior and rendering shared by Comments and
/// a standalone CommentThread. Inside a Comments panel, its threads take these from
/// the Comments.
#[derive(Clone, Default)]
pub struct CommentThreadSharedProps {
    pub current_user: Option<CommentUser>,
    pub mode: CommentMode,
    pub container: CommentContainer,
    pub composer_placeholder: Option<SharedString>,
    pub highlight_color: CommentHighlightColor,
    pub reaction_options: Option<Vec<CommentReactionOption>>,
    pub agent_commands: Vec<CommentAgentCommand>,
    pub composer_triggers: Vec<ChatComposerTrigger>,
    pub markdown_plugins: Vec<MarkdownPlugin>,
    pub markdown_inline_plugins: Vec<MarkdownInlinePlugin>,
    pub get_reply_mention: Option<ReplyMentionFn>,
    /// `true` by default.
    pub has_emoji_picker: Option<bool>,
    pub is_submitting: bool,
    pub on_composer_draft_change: Option<DraftChangeFn>,
    pub on_submit: Option<SubmitFn>,
    pub on_save_edit: Option<SaveEditFn>,
    pub on_delete: Option<CommentIdFn>,
    pub on_react: Option<ReactFn>,
    pub on_retry_comment: Option<CommentIdFn>,
    pub on_resolve_thread: Option<CommentIdFn>,
    pub on_reopen_thread: Option<CommentIdFn>,
    pub load_versions: Option<LoadVersionsFn>,
    pub on_upload_images: Option<CommentImageUploader>,
    pub render_anchor: Option<RenderAnchorFn>,
    pub render_body: Option<RenderCommentFn>,
    pub render_version_body: Option<RenderVersionFn>,
    pub render_additional_actions: Option<RenderCommentFn>,
    pub render_composer: Option<RenderComposerFn>,
}

/// Upstream `container`: the parent surface.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CommentContainer {
    /// Each thread is an outlined Card (the default).
    #[default]
    Section,
    /// Divider-bounded Sections, so cards never nest.
    Card,
}

/// Upstream `DEFAULT_COMMENT_REACTIONS`: Like, Love, Haha, Wow, Sad and Celebrate,
/// named in the viewer's language.
pub fn default_comment_reactions() -> Vec<CommentReactionOption> {
    [
        ("like", "\u{1F44D}"),
        ("love", "\u{2764}\u{FE0F}"),
        ("laugh", "\u{1F602}"),
        ("wow", "\u{1F62E}"),
        ("sad", "\u{1F622}"),
        ("celebrate", "\u{1F389}"),
    ]
    .into_iter()
    .map(|(key, label)| CommentReactionOption::new(key, label))
    .collect()
}
