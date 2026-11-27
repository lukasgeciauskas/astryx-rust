//! A comment row (upstream lab `Comments/CommentRow.tsx`): at depth 0 the whole
//! thread on a selectable surface (its first comment, the replies behind View replies,
//! and the reply composer); at depth 1 one reply inside it.

use std::rc::Rc;

use gpui::prelude::*;
use gpui::{AnyElement, App, ElementId, MouseButton, SharedString, Window, div, px, relative};
use rustryx_components::button::ToggleButton;
use rustryx_components::container_reveal::{
    ContainerReveal, ContentRevealOptions, RevealVisibility, UseContainerRevealOptions,
    use_container_reveal,
};
use rustryx_components::containers::{
    Card, CardVariant, Elevation, Section, SectionSide, SectionVariant,
};
use rustryx_components::layout::{SpacingStep, VisuallyHidden};
use rustryx_components::link::Link;
use rustryx_components::overlay::{MenuEntry, MenuItem};
use rustryx_components::status::{Badge, BadgeVariant};
use rustryx_components::text::{Text, TextColor, TextType};
use rustryx_components::time::{Timestamp, TimestampFormat};
use rustryx_i18n::Arg;
use rustryx_primitives::is_focus_within_visible;

use super::actions::{ActionsProps, ResolveAction, comment_actions, reaction_summary};
use super::composer::ComposerBase;
use super::context::{Scope, scoped, use_scope};
use super::markdown::CommentMarkdown;
use super::parts::{
    ErrorBannerContent, action_link, anchor_quote, author_name, avatar_frame, avatar_spacer,
    collapsed_rail, error_banner, muted, progress_status, rail_segment, resolved_label, separator,
    timestamp_value, unread_badge,
};
use super::types::{
    CommentComposerContext, CommentData, CommentMode, CommentStatus, CommentThreadData,
    CommentUser, CommentUserKind,
};
use super::utils::{ActivationSource, activation_source_of};

/// Opens (`true`) or closes a thread, from a pointer or the keyboard.
type SelectionChangeFn = Rc<dyn Fn(bool, ActivationSource, &mut Window, &mut App)>;

/// Upstream `ThreadDivider`: a card-parent thread's rules.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ThreadDivider {
    Top,
    Bottom,
}

/// Upstream `THREAD_DIVIDERS`.
pub(crate) const THREAD_DIVIDERS: [ThreadDivider; 2] = [ThreadDivider::Top, ThreadDivider::Bottom];

fn unique_users(users: &[CommentUser]) -> Vec<CommentUser> {
    let mut seen = Vec::new();
    users
        .iter()
        .filter(|u| {
            if seen.contains(&u.id) {
                false
            } else {
                seen.push(u.id.clone());
                true
            }
        })
        .cloned()
        .collect()
}

/// Keeps a press on a control from reaching the thread surface (upstream
/// `useClickableContainer`'s inner-control rule).
fn control<E: InteractiveElement>(el: E) -> E {
    el.on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
}

/// One comment row (upstream `CommentRow`).
#[derive(IntoElement)]
pub(crate) struct CommentRow {
    pub comment: CommentData,
    /// The thread, at depth 0.
    pub thread: Option<CommentThreadData>,
    pub dividers: Vec<ThreadDivider>,
    pub is_parent_thread_active: bool,
    /// A reply in a resolved thread takes the thread's muted text.
    pub is_parent_thread_resolved: bool,
    /// The thread this row belongs to: its own id at depth 0.
    pub thread_id: SharedString,
    /// The thread surface's reveal, which a reply's actions follow.
    pub reveal: Option<ContainerReveal>,
}

impl CommentRow {
    /// A thread row.
    pub fn thread(thread: CommentThreadData, dividers: Vec<ThreadDivider>) -> Self {
        CommentRow {
            comment: thread.comment.clone(),
            thread_id: thread.id.clone(),
            thread: Some(thread),
            dividers,
            is_parent_thread_active: false,
            is_parent_thread_resolved: false,
            reveal: None,
        }
    }
}

impl RenderOnce for CommentRow {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let scope = use_scope(cx);
        render_row(&scope, self, window, cx)
    }
}

fn render_row(scope: &Scope, row: CommentRow, window: &mut Window, cx: &mut App) -> AnyElement {
    let theme = rustryx_theme::current(window, cx);
    let t = theme.tokens();
    let i18n = rustryx_i18n::current(cx);
    let props = scope.props.clone();
    let comment = row.comment.clone();
    let thread = row.thread.clone();
    let depth0 = thread.is_some();
    let sid = scope.id.clone();
    let cid = comment.id.clone();
    let state = scope.read(cx);
    let reply_target = state.reply_target.clone();
    let reply_draft = state.reply_draft.clone();
    let reply_error = state.reply_error.clone();
    let edit_target = state.edit_target.clone();
    let is_busy = state.pending.contains(&cid);
    let action_failure = state.action_errors.get(&cid).cloned();
    let is_submitting = scope.is_submitting(cx);
    let composer_pending = scope.is_composer_action_pending(cx);

    // The article's own focus, for hand-offs after a deletion or an edit.
    let article_focus = window
        .use_keyed_state(scoped(&sid, format!("article-{cid}")), cx, |_, cx| {
            cx.focus_handle()
        })
        .read(cx)
        .clone();
    scope.state.update(cx, |s, _| {
        s.articles.insert(cid.clone(), article_focus.clone());
    });

    let status = comment.status;
    let is_ready = status == CommentStatus::Sent;
    let in_progress = comment.progress.is_some();
    let is_own = props
        .current_user
        .as_ref()
        .is_some_and(|u| u.id == comment.author.id);
    let can_mutate = props.mode != CommentMode::ReadOnly && is_ready && !in_progress;
    let can_reply = can_mutate && props.on_submit.is_some() && comment.can_reply.unwrap_or(true);
    let can_react = can_mutate && props.on_react.is_some() && comment.can_react.unwrap_or(true);
    let can_edit = can_mutate
        && props.on_save_edit.is_some()
        && props.mode == CommentMode::Interactive
        && comment.can_edit.unwrap_or(is_own);
    let can_delete = status != CommentStatus::Sending
        && !in_progress
        && props.on_delete.is_some()
        && props.mode == CommentMode::Interactive
        && comment.can_delete.unwrap_or(is_own);
    let row_busy = is_busy || is_submitting || composer_pending;
    let is_resolved = thread.as_ref().is_some_and(|t| t.is_resolved);
    let is_muted = is_resolved || row.is_parent_thread_resolved;
    let resolve = thread
        .as_ref()
        .filter(|t| t.can_resolve != Some(false) && can_mutate)
        .and_then(|_| {
            if is_resolved {
                props.on_reopen_thread.as_ref().map(|_| {
                    let (scope, c) = (scope.clone(), comment.clone());
                    ResolveAction {
                        reopen: true,
                        on_click: Rc::new(move |w: &mut Window, cx: &mut App| {
                            scope.reopen(&c, w, cx)
                        }),
                    }
                })
            } else {
                props.on_resolve_thread.as_ref().map(|_| {
                    let (scope, c) = (scope.clone(), comment.clone());
                    ResolveAction {
                        reopen: false,
                        on_click: Rc::new(move |w: &mut Window, cx: &mut App| {
                            scope.resolve(&c, w, cx)
                        }),
                    }
                })
            }
        });
    let has_history = comment.is_edited
        && (!comment.previous_versions.is_empty() || props.load_versions.is_some());
    let editor_open = edit_target.as_ref().is_some_and(|e| e.comment.id == cid);
    let replies: Vec<CommentData> = thread
        .as_ref()
        .map(|t| t.replies.clone())
        .unwrap_or_default();
    let typing: Vec<CommentUser> = unique_users(
        &thread
            .as_ref()
            .map(|t| t.typing_users.clone())
            .unwrap_or_default(),
    )
    .into_iter()
    .filter(|u| props.current_user.as_ref().is_none_or(|me| me.id != u.id))
    .collect();
    let is_selected = depth0 && scope.selected_thread_id.as_ref() == Some(&cid);
    let unread_replies = replies.iter().any(|r| r.is_unread);
    let unread_reveal = !is_selected && unread_replies;
    // Selection is the open state; reply visibility derives from it.
    let replies_visible = unread_reveal || is_selected;
    let visible_replies: Vec<CommentData> = if replies_visible {
        replies.clone()
    } else {
        Vec::new()
    };
    // An unread reveal is content, not a selected disclosure.
    let show_disclosure = !replies.is_empty() && edit_target.is_none() && !unread_reveal;
    let disclosure_busy = composer_pending || is_submitting;
    let disclosure_label = if replies_visible {
        i18n.t("@astryx.comments.hideReplies")
    } else {
        i18n.format(
            "@astryx.comments.viewReplies",
            &[("count", Arg::Num(replies.len() as f64))],
        )
    };
    let root_reply_open = reply_target
        .as_ref()
        .is_some_and(|r| r.parent_id == cid && r.comment.id == cid);
    let can_open_root_reply = can_reply && !root_reply_open;
    let can_select = depth0 && edit_target.is_none() && !composer_pending && !is_submitting;
    let body_expanded = if depth0 {
        is_selected
    } else {
        row.is_parent_thread_active
    };

    // The thread surface's reveal, which every row in it follows.
    let reveal = match &row.reveal {
        Some(r) => r.clone(),
        None => use_container_reveal(
            scoped(&sid, format!("thread-reveal-{cid}")),
            UseContainerRevealOptions::default(),
            window,
            cx,
        ),
    };
    let menu_open =
        window.use_keyed_state(scoped(&sid, format!("menu-open-{cid}")), cx, |_, _| false);
    let is_menu_open = *menu_open.read(cx);

    // Opening a thread puts the viewer in its reply composer, every time; a composer
    // already open for a reply in it keeps its target and draft.
    let focus_thread_reply = {
        let (scope, comment, rt) = (scope.clone(), comment.clone(), reply_target.clone());
        move |window: &mut Window, cx: &mut App| {
            if !can_reply {
                return;
            }
            let target = rt
                .as_ref()
                .filter(|r| r.parent_id == comment.id)
                .map(|r| r.comment.clone())
                .unwrap_or_else(|| comment.clone());
            scope.start_reply(comment.id.clone(), target, true, window, cx);
        }
    };
    let change_selection: SelectionChangeFn = {
        let (scope, comment, rt) = (scope.clone(), comment.clone(), reply_target.clone());
        Rc::new(move |selected, source, window, cx| {
            if selected {
                scope.select_thread(Some(comment.id.clone()), window, cx);
                if source == ActivationSource::Pointer {
                    focus_thread_reply(window, cx);
                } else if can_open_root_reply {
                    scope.start_reply(comment.id.clone(), comment.clone(), false, window, cx);
                }
                return;
            }
            if is_selected {
                scope.select_thread(None, window, cx);
            }
            if rt.as_ref().is_some_and(|r| r.parent_id == comment.id) {
                scope.cancel_reply(false, window, cx);
            }
        })
    };

    let mut menu_items: Vec<MenuEntry> = Vec::new();
    if can_edit {
        let (scope, c) = (scope.clone(), comment.clone());
        menu_items.push(MenuEntry::Item(
            MenuItem::new("edit", i18n.t("@astryx.comments.edit"))
                .on_click(move |w, cx| scope.start_edit(c.clone(), w, cx)),
        ));
    }
    if can_delete {
        let (scope, c) = (scope.clone(), comment.clone());
        menu_items.push(MenuEntry::Item(
            MenuItem::new("delete", i18n.t("@astryx.comments.delete"))
                .on_click(move |_, cx| scope.request_delete(c.clone(), cx)),
        ));
    }
    let sending = status == CommentStatus::Sending;
    let has_reactions = !comment.reactions.is_empty();
    let show_actions = !editor_open
        && (is_ready || !menu_items.is_empty() || props.render_additional_actions.is_some());
    let has_control_row = sending || has_reactions;
    let focus_article: super::types::ActionFn = {
        let f = article_focus.clone();
        Rc::new(move |w: &mut Window, cx: &mut App| f.focus(w, cx))
    };

    // A failed comment's Banner keeps a retry failure as its description.
    let footer: Option<AnyElement> = if status == CommentStatus::Error {
        let retry = props.on_retry_comment.as_ref().map(|_| {
            let (scope, c) = (scope.clone(), comment.clone());
            Rc::new(move |w: &mut Window, cx: &mut App| scope.retry(&c, w, cx))
                as super::types::ActionFn
        });
        Some(error_banner(
            scoped(&sid, format!("delivery-error-{cid}")),
            ErrorBannerContent {
                title: comment
                    .error_message
                    .clone()
                    .unwrap_or_else(|| i18n.t("@astryx.comments.sendFailed")),
                description: action_failure.as_ref().map(|f| f.message.clone()),
                action_label: retry.as_ref().map(|_| i18n.t("@astryx.comments.retry")),
                on_action: retry,
                is_action_disabled: row_busy,
                is_retrying: is_busy,
                is_announced: action_failure.as_ref().is_some_and(|f| f.is_announced),
            },
            Some(focus_article.clone()),
            window,
            cx,
        ))
    } else if has_control_row || (action_failure.is_some() && !editor_open) {
        let mut col = div()
            .flex()
            .flex_col()
            .items_start()
            .gap(t.spacing.s1)
            .w_full();
        if has_control_row {
            let summary = reaction_summary(scope, &comment, can_react, row_busy, cx);
            col = col.child(
                control(div().id(scoped(&sid, format!("control-row-{cid}"))))
                    .flex()
                    .items_center()
                    .gap(t.spacing.s1)
                    .when(is_ready, |d| {
                        d.ml(-t.spacing.s2.to_pixels(window.rem_size()))
                    })
                    .children(sending.then(|| {
                        rustryx_components::chat::ChatMessageMetadata::new()
                            .status(rustryx_components::chat::ChatMessageStatus::Sending)
                    }))
                    .children(summary),
            );
        }
        if let Some(failure) = action_failure.clone().filter(|_| !editor_open) {
            col = col.child(error_banner(
                scoped(&sid, format!("action-error-{cid}")),
                ErrorBannerContent {
                    title: failure.message.clone(),
                    action_label: failure
                        .retry
                        .as_ref()
                        .map(|_| i18n.t("@astryx.comments.retry")),
                    on_action: failure.retry.clone(),
                    is_announced: failure.is_announced,
                    ..ErrorBannerContent::default()
                },
                Some(focus_article.clone()),
                window,
                cx,
            ));
        }
        Some(col.into_any_element())
    } else {
        None
    };

    // The header: author, badge, time, Edited, Resolved, the actions, New.
    let rem = window.rem_size();
    let avatar = super::parts::avatar_size(window, cx);
    let mut meta = div()
        .flex()
        .items_center()
        .gap(t.spacing.s1)
        .flex_shrink(1.)
        .min_w_0();
    meta = muted(meta, is_muted, window, cx);
    meta = meta.child(author_name(
        scoped(&sid, format!("author-{cid}")),
        &comment.author,
    ));
    if comment.author.kind == CommentUserKind::Agent {
        meta = meta.child(div().flex_none().child(
            Badge::new(i18n.t("@astryx.comments.agentBadge")).variant(BadgeVariant::Neutral),
        ));
    }
    meta = meta.child(separator(TextType::Body)).child(
        Timestamp::new(
            scoped(&sid, format!("time-{cid}")),
            timestamp_value(&comment.created_at),
        )
        .format(TimestampFormat::RelativeShort)
        .text_type(TextType::Body)
        .color(TextColor::Secondary)
        .live(true),
    );
    if comment.is_edited {
        meta = meta.child(separator(TextType::Body));
        meta = meta.child(if has_history {
            let (scope, c) = (scope.clone(), comment.clone());
            control(div().id(scoped(&sid, format!("history-wrap-{cid}"))))
                .child(action_link(
                    scoped(&sid, format!("history-{cid}")),
                    i18n.t("@astryx.comments.edited"),
                    Some(i18n.t("@astryx.comments.viewHistory")),
                    false,
                    Rc::new(move |w, cx| scope.open_history(c.clone(), w, cx)),
                ))
                .into_any_element()
        } else {
            div()
                .whitespace_nowrap()
                .child(
                    Text::new(i18n.t("@astryx.comments.edited"))
                        .text_type(TextType::Body)
                        .color(TextColor::Secondary),
                )
                .into_any_element()
        });
    }
    if is_resolved {
        meta = meta.child(separator(TextType::Body)).child(resolved_label(
            scoped(&sid, format!("resolved-{cid}")),
            thread.as_ref().and_then(|t| t.resolved_by.as_ref()),
            cx,
        ));
    }
    let actions: AnyElement = if show_actions {
        let (scope2, c2, tid) = (scope.clone(), comment.clone(), row.thread_id.clone());
        let menu_state = menu_open.clone();
        let el = comment_actions(
            scope,
            ActionsProps {
                comment: comment.clone(),
                can_copy_link: is_ready && !in_progress,
                can_react,
                can_reply,
                busy: row_busy,
                menu_items,
                menu_label: i18n.format(
                    "@astryx.comments.actionsLabel",
                    &[("name", Arg::from(comment.author.name.as_ref()))],
                ),
                on_open_change: Rc::new(move |open, _, cx| {
                    menu_state.update(cx, |m, cx| {
                        *m = open;
                        cx.notify();
                    })
                }),
                on_reply: Rc::new(move |w, cx| {
                    scope2.start_reply(tid.clone(), c2.clone(), true, w, cx)
                }),
                resolve: resolve.clone(),
                additional: props
                    .render_additional_actions
                    .as_ref()
                    .map(|f| f(&comment, window, cx)),
            },
            window,
            cx,
        );
        // The header's actions take whatever room the name and time leave, and never
        // less than the more menu's button (upstream `metadataBesideActions`).
        control(div().id(scoped(&sid, format!("actions-wrap-{cid}"))))
            .flex()
            .flex_1()
            .min_w(avatar)
            .child(
                reveal
                    .content(ContentRevealOptions {
                        layout_preserved: true,
                        force_visibility: is_menu_open.then_some(RevealVisibility::Shown),
                        ..ContentRevealOptions::default()
                    })
                    .flex()
                    .flex_1()
                    .min_w_0()
                    .debug_selector({
                        let s = format!("comment-actions-reveal-{cid}");
                        move || s
                    })
                    .child(el),
            )
            .into_any_element()
    } else {
        div().flex_1().into_any_element()
    };
    let header = div()
        .flex()
        .items_center()
        .gap(t.spacing.s1)
        .w_full()
        .h(avatar)
        .min_h(avatar)
        .child(meta)
        .child(actions)
        .children(
            comment
                .is_unread
                .then(|| unread_badge(scoped(&sid, format!("unread-{cid}")), window, cx))
                .flatten(),
        );

    let anchor = thread.as_ref().and_then(|th| {
        th.anchor.as_ref().map(|a| {
            let default = anchor_quote(a, props.highlight_color, is_resolved, window, cx);
            match &props.render_anchor {
                Some(f) => f(th, default, window, cx),
                None => default,
            }
        })
    });
    let body: AnyElement = if let (true, Some(edit)) = (editor_open, edit_target.clone()) {
        let (s1, s2, s3, c1, c2) = (
            scope.clone(),
            scope.clone(),
            scope.clone(),
            comment.clone(),
            comment.clone(),
        );
        let context = CommentComposerContext::Edit {
            comment_id: cid.clone(),
        };
        let mut composer = ComposerBase::new(
            scoped(&sid, format!("edit-composer-{cid}")),
            edit.value.clone(),
            Rc::new(move |v, w, cx| s1.change_edit(c1.clone(), v, w, cx)),
            Rc::new(move |v, w, cx| s2.save_edit(c2.clone(), v, w, cx)),
            i18n.t("@astryx.comments.editPlaceholder"),
            context.clone(),
        );
        composer.context_label = Some(i18n.t("@astryx.comments.editingContext"));
        composer.on_cancel = Some(Rc::new(move |w, cx| s3.cancel_edit(true, w, cx)));
        composer.focus_key =
            Some(format!("edit-{cid}-{}", scope.read(cx).edit_focus_requests).into());
        composer.is_submitting = is_busy;
        composer.error = action_failure.as_ref().map(|f| f.message.clone());
        composer.on_retry_error = action_failure.as_ref().and_then(|f| f.retry.clone());
        configure_composer(&mut composer, scope, context);
        control(div().id(scoped(&sid, format!("edit-wrap-{cid}"))))
            .w_full()
            .child(composer)
            .into_any_element()
    } else {
        let content: AnyElement = if let Some(progress) = &comment.progress {
            progress_status(
                scoped(&sid, format!("progress-{cid}")),
                progress,
                window,
                cx,
            )
        } else if let Some(render) = &props.render_body {
            render(&comment, window, cx)
        } else {
            CommentMarkdown::new(
                scoped(&sid, format!("body-{cid}")),
                comment.body.clone(),
                props.agent_commands.clone(),
            )
            .plugins(props.markdown_plugins.clone())
            .inline_plugins(props.markdown_inline_plugins.clone())
            .into_any_element()
        };
        // A comment body, clamped to three lines until its thread opens.
        let line = f32::from(t.text.body.size.to_pixels(rem)) * t.text.body.leading;
        let body_selector = format!(
            "comment-body-{}-{cid}",
            if body_expanded { "expanded" } else { "clamped" }
        );
        let mut clamp = div().w_full().debug_selector(move || body_selector);
        if !body_expanded {
            clamp = clamp.max_h(px(line * 3.)).overflow_hidden();
        }
        muted(clamp, is_muted, window, cx)
            .child(content)
            .into_any_element()
    };

    let article = div()
        .id(scoped(&sid, format!("article-el-{cid}")))
        .track_focus(&article_focus)
        .relative()
        .flex()
        .flex_col()
        .w_full()
        .role(gpui::Role::Article)
        .aria_label(comment.author.name.clone())
        .debug_selector({
            let s = format!("comment-{cid}");
            move || s
        })
        .children(
            (!visible_replies.is_empty()).then(|| rail_segment(true, !show_disclosure, window, cx)),
        )
        .child(
            div()
                .flex()
                .items_start()
                .gap(t.spacing.s2)
                .w_full()
                .child(avatar_frame(
                    scoped(&sid, format!("avatar-{cid}")),
                    &comment.author,
                    window,
                    cx,
                ))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .flex_1()
                        .min_w_0()
                        .gap(t.spacing.s1)
                        .child(header)
                        .children(anchor)
                        .child(body)
                        .children(footer),
                ),
        );

    let mut thread_rows = div()
        .relative()
        .flex()
        .flex_col()
        .gap(t.spacing.s2)
        .w_full();
    if visible_replies.is_empty() && !replies.is_empty() {
        thread_rows = thread_rows.child(collapsed_rail(window, cx));
    }
    thread_rows = thread_rows.child(article);
    if depth0 && !replies.is_empty() {
        let mut region = div().flex().flex_col().gap(t.spacing.s2).w_full();
        if unread_reveal {
            let s = format!("unread-replies-revealed-{cid}");
            region = region.debug_selector(move || s);
        }
        if show_disclosure {
            let change = change_selection.clone();
            let id = scoped(&sid, format!("disclosure-{cid}"));
            let selector = format!("reply-disclosure-{cid}");
            region = region.child(
                div()
                    .relative()
                    .flex()
                    .items_center()
                    .gap(t.spacing.s2)
                    .w_full()
                    .children(
                        (!visible_replies.is_empty())
                            .then(|| rail_segment(false, true, window, cx)),
                    )
                    .child(avatar_spacer(window, cx))
                    .child(
                        control(div().id(ElementId::Name(format!("{id}-wrap").into())))
                            .debug_selector(move || selector)
                            .whitespace_nowrap()
                            .child(
                                Link::new(id, disclosure_label.clone())
                                    .disabled(disclosure_busy)
                                    .on_click(move |event, window, cx| {
                                        if !disclosure_busy {
                                            change(
                                                !replies_visible,
                                                activation_source_of(event),
                                                window,
                                                cx,
                                            );
                                        }
                                    }),
                            ),
                    ),
            );
        }
        let count = visible_replies.len();
        for (index, reply) in visible_replies.iter().enumerate() {
            let child = CommentRow {
                comment: reply.clone(),
                thread: None,
                dividers: Vec::new(),
                is_parent_thread_active: is_selected,
                is_parent_thread_resolved: is_resolved,
                thread_id: cid.clone(),
                reveal: Some(reveal.clone()),
            };
            region = region.child(
                div()
                    .relative()
                    .flex()
                    .flex_col()
                    .gap(t.spacing.s2)
                    .w_full()
                    .children((index + 1 < count).then(|| rail_segment(true, true, window, cx)))
                    .child(render_row(scope, child, window, cx)),
            );
        }
        thread_rows = thread_rows.child(region);
    }
    if depth0 && is_selected && !typing.is_empty() {
        thread_rows = thread_rows.child(
            div()
                .w_full()
                .debug_selector({
                    let s = format!("thread-typing-{cid}");
                    move || s
                })
                .child(
                    crate::chat::ChatTypingIndicator::new(scoped(&sid, format!("typing-{cid}")))
                        .names(typing.iter().map(|u| u.name.clone())),
                ),
        );
    }
    let mut rows = div()
        .flex()
        .flex_col()
        .gap(t.spacing.s8)
        .w_full()
        .child(thread_rows);
    if depth0
        && is_selected
        && let Some(target) = reply_target.clone().filter(|r| r.parent_id == cid)
    {
        let (s1, s2, s3) = (scope.clone(), scope.clone(), scope.clone());
        let parent = cid.clone();
        let parent2 = cid.clone();
        let context = CommentComposerContext::Reply {
            parent_comment_id: cid.clone(),
            replied_to_comment_id: target.comment.id.clone(),
        };
        let mut composer = ComposerBase::new(
            scoped(&sid, format!("reply-composer-{cid}")),
            reply_draft.clone(),
            Rc::new(move |v, w, cx| s1.change_reply_draft(v, w, cx)),
            Rc::new(move |v, w, cx| s2.submit_reply(parent.clone(), v, w, cx)),
            props.composer_placeholder.clone().unwrap_or_else(|| {
                i18n.format(
                    "@astryx.comments.replyPlaceholder",
                    &[("name", Arg::from(target.comment.author.name.as_ref()))],
                )
            }),
            context.clone(),
        );
        composer.context_label = Some(i18n.format(
            "@astryx.comments.replyingContext",
            &[("name", Arg::from(target.comment.author.name.as_ref()))],
        ));
        composer.focus_key = target
            .focus_request
            .map(|n| format!("reply-{}-{n}", target.comment.id).into());
        composer.is_submitting = is_submitting;
        composer.error = reply_error.clone();
        let draft = reply_draft.clone();
        composer.on_retry_error = reply_error.as_ref().map(|_| {
            Rc::new(move |w: &mut Window, cx: &mut App| {
                s3.submit_reply(parent2.clone(), draft.clone(), w, cx)
            }) as super::types::ActionFn
        });
        configure_composer(&mut composer, scope, context);
        let me = props
            .current_user
            .as_ref()
            .map(|u| avatar_frame(scoped(&sid, format!("me-avatar-{cid}")), u, window, cx));
        rows = rows.child(
            control(div().id(scoped(&sid, format!("reply-row-{cid}"))))
                .flex()
                .items_start()
                .gap(t.spacing.s2)
                .w_full()
                .debug_selector({
                    let s = format!("reply-composer-{cid}");
                    move || s
                })
                .child(me.unwrap_or_else(|| avatar_spacer(window, cx)))
                .child(div().flex_1().min_w_0().child(composer)),
        );
    }
    if !depth0 {
        return rows.into_any_element();
    }
    let label = i18n.format(
        "@astryx.comments.threadLabel",
        &[("name", Arg::from(comment.author.name.as_ref()))],
    );
    thread_surface(
        scope,
        &comment,
        rows.into_any_element(),
        &row.dividers,
        is_selected || thread.as_ref().is_some_and(|t| t.is_highlighted),
        can_select,
        is_selected,
        label,
        change_selection,
        &reveal,
        window,
        cx,
    )
}

/// Gives a composer the scope's shared composer settings.
pub(crate) fn configure_composer(
    composer: &mut ComposerBase,
    scope: &Scope,
    context: CommentComposerContext,
) {
    let props = &scope.props;
    composer.highlight_color = props.highlight_color;
    composer.triggers = props.composer_triggers.clone();
    composer.agent_commands = props.agent_commands.clone();
    composer.has_emoji_picker = props.has_emoji_picker.unwrap_or(true);
    composer.render_composer = props.render_composer.clone();
    let pending = scope.clone();
    composer.on_action_pending_change = Some(Rc::new(move |p, _, cx| {
        pending.composer_action_pending_change(p, cx)
    }));
    composer.on_upload_images = props.on_upload_images.clone().map(|upload| {
        Rc::new(move |files, window: &mut Window, cx: &mut App| {
            upload(files, context.clone(), window, cx)
        }) as super::composer::BoundUploader
    });
}

/// Upstream `ThreadSurface`: a basic Card or Section made selectable: a press on the
/// surface toggles selection while inner controls keep their own presses, and a
/// visually hidden toggle carries the accessible name and pressed state.
#[allow(clippy::too_many_arguments)]
fn thread_surface(
    scope: &Scope,
    comment: &CommentData,
    children: AnyElement,
    dividers: &[ThreadDivider],
    filled: bool,
    selectable: bool,
    selected: bool,
    label: SharedString,
    on_change: SelectionChangeFn,
    reveal: &ContainerReveal,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let theme = rustryx_theme::current(window, cx);
    let t = theme.tokens();
    let sid = scope.id.clone();
    let cid = comment.id.clone();
    let toggle_id = scoped(&sid, format!("toggle-{cid}"));
    // The hidden toggle's focus shows on the whole surface.
    let toggle_region = window
        .use_keyed_state(scoped(&sid, format!("toggle-region-{cid}")), cx, |_, cx| {
            cx.focus_handle()
        })
        .read(cx)
        .clone();
    let focused = selectable && is_focus_within_visible(&toggle_region, window, cx);
    let hidden_toggle = selectable.then(|| {
        let change = on_change.clone();
        VisuallyHidden::new().child(
            div().track_focus(&toggle_region).child(
                ToggleButton::new(toggle_id.clone(), label.clone())
                    .pressed(selected)
                    .on_pressed_change(move |next, event, window, cx| {
                        change(next, activation_source_of(event), window, cx)
                    }),
            ),
        )
    });
    let card = scope.props.container == super::types::CommentContainer::Card;
    let surface: AnyElement = if card {
        Section::new()
            .variant(if filled {
                SectionVariant::Muted
            } else {
                SectionVariant::Transparent
            })
            .dividers(dividers.iter().map(|d| match d {
                ThreadDivider::Top => SectionSide::Top,
                ThreadDivider::Bottom => SectionSide::Bottom,
            }))
            .padding(SpacingStep::S3)
            .width(relative(1.))
            .children(hidden_toggle)
            .child(children)
            .into_any_element()
    } else {
        let mut card = Card::new()
            .variant(CardVariant::Default)
            .elevation(Elevation::None)
            .padding(SpacingStep::S3)
            .width(relative(1.));
        if filled {
            card = card.bg(t.color.background_muted);
        }
        card.children(hidden_toggle)
            .child(children)
            .into_any_element()
    };
    let change = on_change.clone();
    let mut el = reveal
        .container(Default::default())
        .w_full()
        .relative()
        .debug_selector({
            let s = format!("thread-{cid}");
            move || s
        })
        .child(surface);
    if card {
        // Neighboring Sections each draw a top and a bottom divider: overlapping them
        // by one border width leaves a single line between threads.
        el = el.mt(-t.border.width.to_pixels(window.rem_size()));
    }
    if focused {
        el = el.border_2().border_color(t.focus.outline_color);
    }
    if selectable {
        el = el.cursor_pointer().on_click(move |event, window, cx| {
            change(!selected, activation_source_of(event), window, cx)
        });
    }
    el.into_any_element()
}
