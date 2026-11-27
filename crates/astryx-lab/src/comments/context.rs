//! The Comments family's shared state (upstream lab `Comments/CommentsContext.tsx`):
//! the controller that owns what the viewer is doing with the host's data (selection,
//! reply and edit targets, the draft lifecycle reports, pending actions, failures,
//! focus hand-offs, the delete confirmation and the edit history), the dialogs it
//! opens, and the scope comment rows read instead of taking dozens of props.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use astryx_components::overlay::AlertDialog;
use astryx_i18n::Arg;
use astryx_primitives::{Politeness, announce, provide, request_focus, use_context};
use gpui::prelude::*;
use gpui::{AnyElement, App, ElementId, Entity, FocusHandle, SharedString, Task, Window, div};

use super::parts::history_dialog;
use super::types::{
    ActionFn, CommentComposerContext, CommentData, CommentReactionOption, CommentThreadData,
    CommentThreadSharedProps, CommentVersion, TaskFn, default_comment_reactions,
};

/// The thread a reply composer is open in, and the comment it answers (upstream
/// `ReplyTarget`).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ReplyTarget {
    pub parent_id: SharedString,
    pub comment: CommentData,
    /// Set when the composer should take focus; each request has its own.
    pub focus_request: Option<u64>,
    /// The mention the draft was seeded with, which the viewer did not type.
    pub seed: Option<SharedString>,
}

/// Upstream `EditTarget`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct EditTarget {
    pub comment: CommentData,
    pub value: SharedString,
}

/// A failed comment action and how to run it again (upstream `ActionFailure`).
#[derive(Clone)]
pub(crate) struct ActionFailure {
    pub message: SharedString,
    pub retry: Option<ActionFn>,
    /// Already spoken by whoever raised it, so the row's Banner stays silent.
    pub is_announced: bool,
}

/// The edit history being shown (upstream `HistoryState`).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct HistoryState {
    pub comment: CommentData,
    pub versions: Vec<CommentVersion>,
    pub is_loading: bool,
    pub error: Option<SharedString>,
}

/// Where focus goes after an interaction target closes (upstream `FocusTarget`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FocusTarget {
    Reply,
    Actions,
    History,
    Article,
    Thread,
}

/// Upstream `SubmitResult`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SubmitResult {
    Posted,
    Failed,
    Skipped,
}

/// What the controller keeps across frames.
#[derive(Default)]
pub(crate) struct ControllerState {
    pub uncontrolled_selected: Option<SharedString>,
    /// The last selection made by this surface.
    pub viewer_selection: Option<Option<SharedString>>,
    pub reply_target: Option<ReplyTarget>,
    pub reply_draft: SharedString,
    pub reply_error: Option<SharedString>,
    pub edit_target: Option<EditTarget>,
    pub pending: HashSet<SharedString>,
    pub local_submit_pending: bool,
    pub composer_actions_pending: usize,
    pub action_errors: HashMap<SharedString, ActionFailure>,
    pub delete_target: Option<CommentData>,
    pub delete_error: Option<SharedString>,
    pub history: Option<HistoryState>,
    pub reply_focus_requests: u64,
    /// Bumped to put focus back in the edit composer.
    pub edit_focus_requests: u64,
    /// Each comment's article focus, registered as its row renders.
    pub articles: HashMap<SharedString, FocusHandle>,
}

/// A selection-change callback.
pub(crate) type SelectionFn = Rc<dyn Fn(Option<SharedString>, &mut Window, &mut App)>;

/// The nearest Comments or standalone CommentThread scope (upstream
/// `CommentsContextValue`), cheap to clone.
#[derive(Clone)]
pub(crate) struct Scope {
    /// The scope's id: element ids within it start with it.
    pub id: SharedString,
    pub props: Rc<CommentThreadSharedProps>,
    pub state: Entity<ControllerState>,
    pub selected_thread_id: Option<SharedString>,
    pub is_selection_controlled: bool,
    pub on_selected_thread_change: Option<SelectionFn>,
    /// Threads in display order, to find who takes focus after a deletion.
    pub roots: Rc<Vec<CommentThreadData>>,
    /// Takes focus when nothing nearer can, such as after the last deletion.
    pub root_focus: FocusHandle,
    pub reaction_options: Rc<Vec<CommentReactionOption>>,
}

/// The scope's context.
pub(crate) fn use_scope(cx: &App) -> Scope {
    use_context::<Scope>(cx)
        .expect("Comment rows render inside a Comments panel or a CommentThread.")
}

/// Wraps a scope's content so its rows find it.
pub(crate) fn provide_scope(scope: Scope, child: impl IntoElement) -> impl IntoElement {
    provide(scope, child)
}

fn reply_context(target: &ReplyTarget) -> CommentComposerContext {
    CommentComposerContext::Reply {
        parent_comment_id: target.parent_id.clone(),
        replied_to_comment_id: target.comment.id.clone(),
    }
}

fn edit_context(comment: &CommentData) -> CommentComposerContext {
    CommentComposerContext::Edit {
        comment_id: comment.id.clone(),
    }
}

/// An element id inside a scope.
pub(crate) fn scoped(scope: &str, part: impl std::fmt::Display) -> ElementId {
    ElementId::Name(format!("{scope}-{part}").into())
}

impl Scope {
    /// The state's current value.
    pub fn read<'a>(&self, cx: &'a App) -> &'a ControllerState {
        self.state.read(cx)
    }

    fn update(&self, cx: &mut App, f: impl FnOnce(&mut ControllerState)) {
        self.state.update(cx, |s, cx| {
            f(s);
            cx.notify();
        });
    }

    /// The scope's composers report drafts through the host's callback.
    fn report(
        &self,
        value: SharedString,
        context: CommentComposerContext,
        window: &mut Window,
        cx: &mut App,
    ) {
        if let Some(f) = &self.props.on_composer_draft_change {
            f(value, context, window, cx);
        }
    }

    fn clear_reply_draft(&self, target: Option<ReplyTarget>, window: &mut Window, cx: &mut App) {
        let target = target.or_else(|| self.read(cx).reply_target.clone());
        if let Some(target) = &target {
            self.report(SharedString::default(), reply_context(target), window, cx);
        }
        self.update(cx, |s| {
            s.reply_target = None;
            s.reply_draft = SharedString::default();
            s.reply_error = None;
        });
    }

    fn clear_edit_draft(&self, target: Option<EditTarget>, window: &mut Window, cx: &mut App) {
        let target = target.or_else(|| self.read(cx).edit_target.clone());
        if let Some(target) = &target {
            self.report(
                SharedString::default(),
                edit_context(&target.comment),
                window,
                cx,
            );
        }
        self.update(cx, |s| s.edit_target = None);
    }

    fn set_selected_thread_id(
        &self,
        thread: Option<SharedString>,
        window: &mut Window,
        cx: &mut App,
    ) {
        let controlled = self.is_selection_controlled;
        let previous = self.selected_thread_id.clone();
        self.update(cx, |s| {
            s.viewer_selection = Some(thread.clone());
            if !controlled {
                s.uncontrolled_selected = thread.clone();
            }
        });
        if thread != previous
            && let Some(f) = &self.on_selected_thread_change
        {
            f(thread, window, cx);
        }
        window.refresh();
    }

    /// Focuses the control that replaces an interaction target (upstream
    /// `focusCommentTarget`).
    pub fn focus_target(
        &self,
        comment_id: Option<&SharedString>,
        target: FocusTarget,
        window: &mut Window,
        cx: &mut App,
    ) {
        let Some(comment_id) = comment_id.filter(|_| target != FocusTarget::Thread) else {
            window.focus(&self.root_focus, cx);
            return;
        };
        match target {
            FocusTarget::Reply => {
                request_focus(&scoped(&self.id, format!("reply-{comment_id}")), cx)
            }
            FocusTarget::Actions => {
                request_focus(&scoped(&self.id, format!("more-{comment_id}-trigger")), cx)
            }
            FocusTarget::History => {
                request_focus(&scoped(&self.id, format!("history-{comment_id}")), cx)
            }
            FocusTarget::Article | FocusTarget::Thread => {
                let handle = self.read(cx).articles.get(comment_id).cloned();
                window.focus(handle.as_ref().unwrap_or(&self.root_focus), cx);
            }
        }
        window.refresh();
    }

    /// Runs a host action for one comment: pending while it runs, its failure kept for
    /// the row with a retry (upstream `runCommentAction`). Resolves whether it
    /// succeeded.
    #[allow(clippy::too_many_arguments)]
    pub fn run_action(
        &self,
        comment_id: SharedString,
        action: TaskFn,
        failure: SharedString,
        retry: Option<ActionFn>,
        is_announced: bool,
        window: &mut Window,
        cx: &mut App,
    ) -> Task<bool> {
        self.update(cx, |s| {
            s.pending.insert(comment_id.clone());
            s.action_errors.remove(&comment_id);
        });
        let task = action(window, cx);
        let scope = self.clone();
        window.spawn(cx, async move |cx| {
            let ok = task.await.is_ok();
            let _ = cx.update(|_, cx| {
                scope.update(cx, |s| {
                    s.pending.remove(&comment_id);
                    if !ok {
                        let retry = retry.clone().unwrap_or_else(|| {
                            let (scope, id, action, failure) = (
                                scope.clone(),
                                comment_id.clone(),
                                action.clone(),
                                failure.clone(),
                            );
                            Rc::new(move |window: &mut Window, cx: &mut App| {
                                scope
                                    .run_action(
                                        id.clone(),
                                        action.clone(),
                                        failure.clone(),
                                        None,
                                        false,
                                        window,
                                        cx,
                                    )
                                    .detach();
                            })
                        });
                        s.action_errors.insert(
                            comment_id.clone(),
                            ActionFailure {
                                message: failure.clone(),
                                retry: Some(retry),
                                is_announced,
                            },
                        );
                    }
                });
            });
            ok
        })
    }

    /// Posts a comment, or a reply when `parent_id` names its thread (upstream
    /// `submit`).
    pub fn submit(
        &self,
        content: SharedString,
        parent_id: Option<SharedString>,
        window: &mut Window,
        cx: &mut App,
    ) -> Task<SubmitResult> {
        let Some(on_submit) = self.props.on_submit.clone() else {
            return Task::ready(SubmitResult::Skipped);
        };
        let submitted: SharedString = astryx_theme::authoring::js::js_trim(&content)
            .to_string()
            .into();
        if submitted.is_empty() {
            return Task::ready(SubmitResult::Skipped);
        }
        let reply_target = parent_id
            .as_ref()
            .and_then(|_| self.read(cx).reply_target.clone());
        self.update(cx, |s| {
            s.local_submit_pending = true;
            if parent_id.is_some() {
                s.reply_error = None;
            }
        });
        let task = on_submit(submitted.clone(), parent_id.clone(), window, cx);
        let scope = self.clone();
        window.spawn(cx, async move |cx| {
            let ok = task.await.is_ok();
            cx.update(|window, cx| {
                scope.update(cx, |s| s.local_submit_pending = false);
                if let Some(parent) = &parent_id {
                    if ok {
                        scope.clear_reply_draft(reply_target.clone(), window, cx);
                        scope.focus_target(Some(parent), FocusTarget::Reply, window, cx);
                    } else {
                        let message = astryx_i18n::current(cx).t("@astryx.comments.replyFailed");
                        scope.update(cx, |s| {
                            s.reply_draft = submitted.clone();
                            s.reply_error = Some(message);
                        });
                        if let Some(target) = &reply_target {
                            scope.report(submitted.clone(), reply_context(target), window, cx);
                        }
                    }
                }
                if ok {
                    SubmitResult::Posted
                } else {
                    SubmitResult::Failed
                }
            })
            .unwrap_or(SubmitResult::Failed)
        })
    }

    /// Selects a thread, or none (upstream `selectThread`): a reply open in another
    /// thread closes.
    pub fn select_thread(&self, thread: Option<SharedString>, window: &mut Window, cx: &mut App) {
        self.set_selected_thread_id(thread.clone(), window, cx);
        let other = self
            .read(cx)
            .reply_target
            .as_ref()
            .is_some_and(|t| Some(&t.parent_id) != thread.as_ref());
        if other {
            self.clear_reply_draft(None, window, cx);
        }
    }

    /// Upstream `startReply`: opens the reply composer in `parent_id` answering
    /// `target`, seeded by `get_reply_mention` for a reply to a reply.
    pub fn start_reply(
        &self,
        parent_id: SharedString,
        target: CommentData,
        focus: bool,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.set_selected_thread_id(Some(parent_id.clone()), window, cx);
        if self.read(cx).edit_target.is_some() {
            self.clear_edit_draft(None, window, cx);
        }
        let current = self.read(cx).reply_target.clone();
        let is_new = current
            .as_ref()
            .is_none_or(|t| t.parent_id != parent_id || t.comment.id != target.id);
        let seeded: SharedString = if parent_id == target.id
            || target.author.kind == super::types::CommentUserKind::Agent
        {
            SharedString::default()
        } else {
            self.props
                .get_reply_mention
                .as_ref()
                .and_then(|f| f(&target.author))
                .unwrap_or_default()
        };
        let focus_request = focus.then(|| {
            let mut n = 0;
            self.update(cx, |s| {
                s.reply_focus_requests += 1;
                n = s.reply_focus_requests;
            });
            n
        });
        let next = ReplyTarget {
            parent_id,
            comment: target,
            focus_request,
            seed: if is_new {
                Some(seeded.clone())
            } else {
                current.as_ref().and_then(|t| t.seed.clone())
            },
        };
        if is_new {
            if current.is_some() {
                self.clear_reply_draft(None, window, cx);
            }
            self.update(cx, |s| s.reply_draft = seeded.clone());
            if !seeded.is_empty() {
                self.report(seeded, reply_context(&next), window, cx);
            }
        }
        self.update(cx, |s| {
            s.reply_error = None;
            s.reply_target = Some(next);
        });
    }

    /// Upstream `changeReplyDraft`.
    pub fn change_reply_draft(&self, value: SharedString, window: &mut Window, cx: &mut App) {
        self.update(cx, |s| {
            s.reply_draft = value.clone();
            s.reply_error = None;
        });
        if let Some(target) = self.read(cx).reply_target.clone() {
            self.report(value, reply_context(&target), window, cx);
        }
    }

    /// Upstream `submitReply`.
    pub fn submit_reply(
        &self,
        parent_id: SharedString,
        value: SharedString,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.submit(value, Some(parent_id), window, cx).detach();
    }

    /// Upstream `cancelReply`.
    pub fn cancel_reply(&self, restore_focus: bool, window: &mut Window, cx: &mut App) {
        let parent = self
            .read(cx)
            .reply_target
            .as_ref()
            .map(|t| t.parent_id.clone());
        self.clear_reply_draft(None, window, cx);
        if restore_focus && let Some(parent) = parent {
            self.focus_target(Some(&parent), FocusTarget::Reply, window, cx);
        }
    }

    /// Upstream `startEdit`.
    pub fn start_edit(&self, comment: CommentData, window: &mut Window, cx: &mut App) {
        if self.read(cx).reply_target.is_some() {
            self.clear_reply_draft(None, window, cx);
        }
        let current = self.read(cx).edit_target.clone();
        if current.as_ref().is_some_and(|t| t.comment.id != comment.id) {
            self.clear_edit_draft(None, window, cx);
        }
        let is_new = current.as_ref().is_none_or(|t| t.comment.id != comment.id);
        let body = comment.body.clone();
        self.update(cx, |s| {
            s.action_errors.remove(&comment.id);
            s.edit_target = Some(EditTarget {
                comment: comment.clone(),
                value: body.clone(),
            });
        });
        if is_new {
            self.report(body, edit_context(&comment), window, cx);
        }
    }

    /// Upstream `changeEdit`.
    pub fn change_edit(
        &self,
        comment: CommentData,
        value: SharedString,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.update(cx, |s| {
            s.edit_target = Some(EditTarget {
                comment: comment.clone(),
                value: value.clone(),
            });
            s.action_errors.remove(&comment.id);
        });
        self.report(value, edit_context(&comment), window, cx);
    }

    /// Upstream `saveEdit`.
    pub fn save_edit(
        &self,
        comment: CommentData,
        content: SharedString,
        window: &mut Window,
        cx: &mut App,
    ) {
        let Some(on_save) = self.props.on_save_edit.clone() else {
            return;
        };
        let saved: SharedString = astryx_theme::authoring::js::js_trim(&content)
            .to_string()
            .into();
        if saved.is_empty() {
            return;
        }
        let id = comment.id.clone();
        let action: TaskFn = {
            let (id, saved) = (id.clone(), saved.clone());
            Rc::new(move |window, cx| on_save(id.clone(), saved.clone(), window, cx))
        };
        let retry: ActionFn = {
            let (scope, comment, saved) = (self.clone(), comment.clone(), saved.clone());
            Rc::new(move |window, cx| scope.save_edit(comment.clone(), saved.clone(), window, cx))
        };
        let failure = astryx_i18n::current(cx).t("@astryx.comments.editFailed");
        let task = self.run_action(id.clone(), action, failure, Some(retry), false, window, cx);
        let scope = self.clone();
        window
            .spawn(cx, async move |cx| {
                let ok = task.await;
                let _ = cx.update(|window, cx| {
                    let target = EditTarget {
                        comment: comment.clone(),
                        value: saved.clone(),
                    };
                    if ok {
                        scope.clear_edit_draft(Some(target), window, cx);
                        scope.focus_target(Some(&id), FocusTarget::Actions, window, cx);
                    } else {
                        scope.update(cx, |s| s.edit_target = Some(target));
                        scope.report(saved.clone(), edit_context(&comment), window, cx);
                    }
                });
            })
            .detach();
    }

    /// Upstream `cancelEdit`.
    pub fn cancel_edit(&self, restore_focus: bool, window: &mut Window, cx: &mut App) {
        let edited = self
            .read(cx)
            .edit_target
            .as_ref()
            .map(|t| t.comment.id.clone());
        self.clear_edit_draft(None, window, cx);
        if restore_focus && let Some(edited) = edited {
            self.focus_target(Some(&edited), FocusTarget::Actions, window, cx);
        }
    }

    fn simple_action(
        &self,
        comment: &CommentData,
        f: Option<super::types::CommentIdFn>,
        failure_key: &str,
        window: &mut Window,
        cx: &mut App,
    ) {
        let Some(f) = f else {
            return;
        };
        let id = comment.id.clone();
        let action: TaskFn = {
            let id = id.clone();
            Rc::new(move |window, cx| f(id.clone(), window, cx))
        };
        let failure = astryx_i18n::current(cx).t(failure_key);
        self.run_action(id, action, failure, None, false, window, cx)
            .detach();
    }

    /// Upstream `react`: toggles the viewer's reaction.
    pub fn react(
        &self,
        comment: &CommentData,
        key: SharedString,
        option: Option<CommentReactionOption>,
        window: &mut Window,
        cx: &mut App,
    ) {
        let Some(on_react) = self.props.on_react.clone() else {
            return;
        };
        let id = comment.id.clone();
        let action: TaskFn = {
            let id = id.clone();
            Rc::new(move |window, cx| on_react(id.clone(), key.clone(), option.clone(), window, cx))
        };
        let failure = astryx_i18n::current(cx).t("@astryx.comments.reactionFailed");
        self.run_action(id, action, failure, None, false, window, cx)
            .detach();
    }

    /// Upstream `resolve`.
    pub fn resolve(&self, comment: &CommentData, window: &mut Window, cx: &mut App) {
        self.simple_action(
            comment,
            self.props.on_resolve_thread.clone(),
            "@astryx.comments.resolveFailed",
            window,
            cx,
        );
    }

    /// Upstream `reopen`.
    pub fn reopen(&self, comment: &CommentData, window: &mut Window, cx: &mut App) {
        self.simple_action(
            comment,
            self.props.on_reopen_thread.clone(),
            "@astryx.comments.reopenFailed",
            window,
            cx,
        );
    }

    /// Upstream `retry`.
    pub fn retry(&self, comment: &CommentData, window: &mut Window, cx: &mut App) {
        self.simple_action(
            comment,
            self.props.on_retry_comment.clone(),
            "@astryx.comments.retryFailed",
            window,
            cx,
        );
    }

    /// Upstream `requestDelete`: asks for confirmation.
    pub fn request_delete(&self, comment: CommentData, cx: &mut App) {
        self.update(cx, |s| s.delete_target = Some(comment));
    }

    /// The neighbor that takes focus once `target` is deleted: a reply's thread, else
    /// the next thread, else the previous one (upstream `focusTargetAfterDelete`).
    fn focus_after_delete(&self, target: &CommentData) -> Option<SharedString> {
        match self.roots.iter().position(|root| root.id == target.id) {
            None => self
                .roots
                .iter()
                .find(|root| root.replies.iter().any(|reply| reply.id == target.id))
                .map(|root| root.id.clone()),
            Some(index) => self
                .roots
                .get(index + 1)
                .or_else(|| index.checked_sub(1).and_then(|i| self.roots.get(i)))
                .map(|root| root.id.clone()),
        }
    }

    /// Upstream `confirmDelete`.
    pub fn confirm_delete(&self, window: &mut Window, cx: &mut App) {
        let Some(target) = self.read(cx).delete_target.clone() else {
            return;
        };
        let Some(on_delete) = self.props.on_delete.clone() else {
            return;
        };
        let neighbor = self.focus_after_delete(&target);
        self.update(cx, |s| s.delete_error = None);
        let id = target.id.clone();
        let action: TaskFn = {
            let id = id.clone();
            Rc::new(move |window, cx| on_delete(id.clone(), window, cx))
        };
        // Retrying a delete asks for confirmation again.
        let retry: ActionFn = {
            let (scope, target) = (self.clone(), target.clone());
            Rc::new(move |_, cx| scope.request_delete(target.clone(), cx))
        };
        let failure = astryx_i18n::current(cx).t("@astryx.comments.deleteFailed");
        // The dialog announces a failed delete, so the stored failure the row shows is
        // marked before it first renders.
        let task = self.run_action(id, action, failure, Some(retry), true, window, cx);
        let scope = self.clone();
        window
            .spawn(cx, async move |cx| {
                let ok = task.await;
                let _ = cx.update(|window, cx| {
                    if ok {
                        scope.update(cx, |s| s.delete_target = None);
                        let target = if neighbor.is_none() {
                            FocusTarget::Thread
                        } else {
                            FocusTarget::Article
                        };
                        scope.focus_target(neighbor.as_ref(), target, window, cx);
                    } else {
                        // Announced from the failure itself, once, as the dialog's text
                        // changes.
                        let message =
                            astryx_i18n::current(cx).t("@astryx.comments.deleteFailedRetry");
                        scope.update(cx, |s| s.delete_error = Some(message.clone()));
                        announce(message, Politeness::Assertive, window, cx);
                    }
                });
            })
            .detach();
    }

    /// Upstream `openHistory`: shows the versions the host has, and loads the full
    /// history when it can.
    pub fn open_history(&self, comment: CommentData, window: &mut Window, cx: &mut App) {
        let loader = self.props.load_versions.clone();
        self.update(cx, |s| {
            s.history = Some(HistoryState {
                comment: comment.clone(),
                versions: comment.previous_versions.clone(),
                is_loading: loader.is_some(),
                error: None,
            })
        });
        let Some(loader) = loader else {
            return;
        };
        let task = loader(comment.id.clone(), window, cx);
        let scope = self.clone();
        window
            .spawn(cx, async move |cx| {
                let result = task.await;
                let _ = cx.update(|_, cx| {
                    let message = astryx_i18n::current(cx).t("@astryx.comments.historyLoadFailed");
                    scope.update(cx, |s| {
                        if let Some(history) =
                            s.history.as_mut().filter(|h| h.comment.id == comment.id)
                        {
                            history.is_loading = false;
                            match &result {
                                Ok(versions) => history.versions = versions.clone(),
                                Err(()) => history.error = Some(message.clone()),
                            }
                        }
                    });
                });
            })
            .detach();
    }

    /// Upstream `composerActionPendingChange`.
    pub fn composer_action_pending_change(&self, pending: bool, cx: &mut App) {
        self.update(cx, |s| {
            s.composer_actions_pending = if pending {
                s.composer_actions_pending + 1
            } else {
                s.composer_actions_pending.saturating_sub(1)
            };
        });
    }

    /// Whether a post is in flight: the host's, or this scope's own.
    pub fn is_submitting(&self, cx: &App) -> bool {
        self.props.is_submitting || self.read(cx).local_submit_pending
    }

    /// Whether a composer's upload is in flight.
    pub fn is_composer_action_pending(&self, cx: &App) -> bool {
        self.read(cx).composer_actions_pending > 0
    }

    /// The delete confirmation and the edit history, when open (upstream `dialogs`).
    pub fn dialogs(&self, window: &mut Window, cx: &mut App) -> AnyElement {
        let i18n = astryx_i18n::current(cx);
        let s = self.read(cx);
        let delete = s.delete_target.clone();
        let delete_error = s.delete_error.clone();
        let history = s.history.clone();
        let busy_delete = delete.as_ref().is_some_and(|d| s.pending.contains(&d.id));
        let mut out = div();
        if let Some(target) = delete {
            let description = delete_error.unwrap_or_else(|| {
                i18n.format(
                    "@astryx.comments.deleteDescription",
                    &[("name", Arg::from(target.author.name.as_ref()))],
                )
            });
            let (scope, close_scope, id) = (self.clone(), self.clone(), target.id.clone());
            out = out.child(
                AlertDialog::new(
                    scoped(&self.id, "delete-dialog"),
                    true,
                    i18n.t("@astryx.comments.deleteTitle"),
                    description,
                    i18n.t("@astryx.comments.delete"),
                )
                .action_loading(busy_delete)
                .on_action(move |window, cx| scope.confirm_delete(window, cx))
                .on_open_change(move |open, window, cx| {
                    if !open && !close_scope.read(cx).pending.contains(&id) {
                        // The dialog closes: focus returns to the comment's actions.
                        close_scope.update(cx, |s| {
                            s.delete_target = None;
                            s.delete_error = None;
                        });
                        close_scope.focus_target(Some(&id), FocusTarget::Actions, window, cx);
                    }
                }),
            );
        }
        if let Some(history) = history {
            let scope = self.clone();
            let id = history.comment.id.clone();
            let on_close: ActionFn = Rc::new(move |window, cx| {
                scope.update(cx, |s| s.history = None);
                scope.focus_target(Some(&id), FocusTarget::History, window, cx);
            });
            out = out.child(history_dialog(self, history, on_close, window, cx));
        }
        out.into_any_element()
    }
}

/// What the controller is given (upstream `CommentsControllerOptions`).
pub(crate) struct ControllerOptions {
    pub id: SharedString,
    pub props: CommentThreadSharedProps,
    /// Controlled selection by thread id; `None` lets the scope own it.
    pub selected_thread_id: Option<Option<SharedString>>,
    pub on_selected_thread_change: Option<SelectionFn>,
    pub roots: Vec<CommentThreadData>,
}

/// Upstream `useCommentsController`: the interaction state behind a Comments panel
/// or a standalone CommentThread.
pub(crate) fn use_controller(
    options: ControllerOptions,
    window: &mut Window,
    cx: &mut App,
) -> Scope {
    let id = options.id.clone();
    let state = window.use_keyed_state(scoped(&id, "comments-state"), cx, |_, _| {
        ControllerState::default()
    });
    let root_focus = window
        .use_keyed_state(scoped(&id, "comments-root-focus"), cx, |_, cx| {
            cx.focus_handle()
        })
        .read(cx)
        .clone();
    let is_selection_controlled = options.selected_thread_id.is_some();
    let selected = match &options.selected_thread_id {
        Some(selected) => selected.clone(),
        None => state.read(cx).uncontrolled_selected.clone(),
    };
    let reaction_options = options
        .props
        .reaction_options
        .clone()
        .unwrap_or_else(default_comment_reactions);
    let scope = Scope {
        id,
        props: Rc::new(options.props),
        state,
        selected_thread_id: selected.clone(),
        is_selection_controlled,
        on_selected_thread_change: options.on_selected_thread_change,
        roots: Rc::new(options.roots),
        root_focus,
        reaction_options: Rc::new(reaction_options),
    };
    // A host-driven selection scrolls its thread into view (upstream's effect); the
    // rows scroll themselves, so this only records the selection.
    let viewer = scope.read(cx).viewer_selection.clone();
    if viewer.as_ref() != Some(&selected) {
        scope
            .state
            .update(cx, |s, _| s.viewer_selection = Some(selected.clone()));
        SCROLL_REQUESTS.with(|r| r.borrow_mut().insert(scope.id.to_string(), selected));
    }
    scope
}

thread_local! {
    /// A host's selection to scroll into view, by scope.
    pub(crate) static SCROLL_REQUESTS: RefCell<HashMap<String, Option<SharedString>>> = RefCell::new(HashMap::new());
}
