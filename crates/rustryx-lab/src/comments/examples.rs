//! The Comments gallery (upstream's `Lab/Comments` stories, with the
//! `CommentsShowcase`, `CommentThreadShowcase`, `CommentThreadInPopover` and
//! `CommentComposerShowcase` blocks they render).

use web_time::{SystemTime, UNIX_EPOCH};

use gpui::{
    AnyElement, App, Entity, IntoElement, ParentElement, SharedString, Styled, Task, Window, div,
    px, relative,
};
use rustryx_components::button::{Button, ButtonVariant};
use rustryx_components::containers::Card;
use rustryx_components::layout::{SpacingStep, Stack};
use rustryx_components::overlay::Popover;
use rustryx_components::text::{Heading, HeadingLevel, Text, TextColor, TextType};
use rustryx_primitives::ElementSize;

use super::comments::{Comments, CommentsDensity, CommentsLayout};
use super::composer::CommentComposer;
use super::shared::CommentThreadShared;
use super::thread::CommentThread;
use super::types::{
    CommentActivityEvent, CommentAgentCommand, CommentAnchor, CommentContainer, CommentData,
    CommentMode, CommentReaction, CommentReactionOption, CommentStatus, CommentThreadData,
    CommentUploadedImage, CommentUser, CommentVersion, default_comment_reactions,
};

fn viewer() -> CommentUser {
    CommentUser::new("avery", "Avery Kim")
}

fn maya() -> CommentUser {
    CommentUser::new("maya", "Maya Chen")
}

fn noah() -> CommentUser {
    CommentUser::new("noah", "Noah Williams")
}

fn jordan() -> CommentUser {
    CommentUser::new("jordan", "Jordan Lee")
}

fn sam() -> CommentUser {
    CommentUser::new("sam", "Sam Rivera")
}

/// The current time, as `new Date().toISOString()` stands in for.
fn now_ms() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0., |d| d.as_millis() as f64)
}

/// The stories' two threads.
fn story_comments() -> Vec<CommentThreadData> {
    vec![
        CommentThreadData::new(
            "root",
            maya(),
            "The rollout plan is ready for review.",
            "2026-09-30T19:00:00Z",
        )
        .reactions([CommentReaction::new("like", "\u{1F44D}", 2)
            .reacted_by_viewer(true)
            .actors([("avery", "Avery Kim"), ("maya", "Maya Chen")])])
        .replies([CommentData::new(
            "reply",
            viewer(),
            "Looks good to me.",
            "2026-09-30T19:10:00Z",
        )]),
        CommentThreadData::new(
            "second",
            viewer(),
            "I added the guardrail dashboard to the summary.",
            "2026-09-30T19:20:00Z",
        ),
    ]
}

fn ok() -> super::types::CommentTask {
    Task::ready(Ok(()))
}

/// The showcase's activity events.
fn showcase_events() -> Vec<CommentActivityEvent> {
    vec![
        CommentActivityEvent::new(
            "checklist",
            "Rollout checklist attached",
            "2026-09-30T18:46:00Z",
        )
        .description("Added the launch checklist to the discussion.")
        .actor(maya()),
        CommentActivityEvent::new(
            "status",
            "Status changed to Ready for review",
            "2026-09-30T19:02:00Z",
        ),
    ]
}

/// The showcase's threads: an edited, unread thread with a typing teammate and an
/// agent's reply in progress, an inline comment with an unread reply, a resolved
/// orphaned comment, and a comment that failed to post.
fn showcase_comments() -> Vec<CommentThreadData> {
    let assistant = CommentUser::agent("release-assistant", "Release Assistant");
    vec![
        CommentThreadData::new(
            "plan",
            maya(),
            "The revised rollout plan is ready. **Avery**, could you check the final cohort?",
            "2026-09-30T18:30:00Z",
        )
        .edited(true)
        .unread(true)
        .typing_users([noah()])
        .seen_by([noah()])
        .previous_versions([CommentVersion::new(
            "plan-v1",
            "The revised rollout plan is ready for review.",
            "2026-09-30T18:10:00Z",
        )
        .author(maya())])
        .reactions([CommentReaction::new("like", "\u{1F44D}", 2)
            .description("Like")
            .reacted_by_viewer(true)
            .actors([("avery", "Avery Kim"), ("noah", "Noah Williams")])])
        .replies([
            CommentData::new(
                "plan-reply-1",
                noah(),
                "I checked the cohort and the guardrail query. Both look good.",
                "2026-09-30T18:42:00Z",
            ),
            CommentData::new("plan-reply-2", assistant, "", "2026-09-30T18:45:00Z")
                .progress("Checking the rollout dependencies\u{2026}", None),
        ]),
        CommentThreadData::new(
            "inline",
            noah(),
            "Should this say three days? The dry run adds one.",
            "2026-09-30T18:52:00Z",
        )
        .anchor(CommentAnchor::new(
            "The final cohort completes rollout in two days.",
        ))
        .replies([CommentData::new(
            "inline-reply-1",
            maya(),
            "Yes \u{2014} I updated the rollout window to three days.",
            "2026-09-30T19:04:00Z",
        )
        .unread(true)]),
        CommentThreadData::new("orphaned", maya(), "Typo: \"Teh\".", "2026-09-30T18:55:00Z")
            .anchor(CommentAnchor::new("Teh guardrail query runs hourly.").orphaned())
            .resolved(true)
            .resolved_by(viewer()),
        CommentThreadData::new(
            "failed",
            viewer(),
            "Posting this update failed, but the text is kept.",
            "2026-09-30T19:10:00Z",
        )
        .status(CommentStatus::Error)
        .error_message("Couldn't post this comment."),
    ]
}

/// Upstream `updateComment`: applies `update` to the comment `id`, a thread's first
/// comment or a reply.
fn update_comment(threads: &mut [CommentThreadData], id: &str, update: impl Fn(&mut CommentData)) {
    for thread in threads {
        if thread.comment.id.as_ref() == id {
            update(&mut thread.comment);
        }
        for reply in &mut thread.replies {
            if reply.id.as_ref() == id {
                update(reply);
            }
        }
    }
}

/// Upstream `findComment`.
fn find_comment(threads: &[CommentThreadData], id: &str) -> Option<CommentData> {
    threads.iter().find_map(|thread| {
        if thread.comment.id.as_ref() == id {
            Some(thread.comment.clone())
        } else {
            thread.replies.iter().find(|r| r.id.as_ref() == id).cloned()
        }
    })
}

fn set(
    state: &Entity<Vec<CommentThreadData>>,
    cx: &mut App,
    f: impl FnOnce(&mut Vec<CommentThreadData>),
) {
    state.update(cx, |threads, cx| {
        f(threads);
        cx.notify();
    });
}

/// Upstream `CommentsShowcase`: every feature, with working callbacks.
fn comments_showcase(window: &mut Window, cx: &mut App) -> AnyElement {
    let state = window.use_keyed_state("comments-showcase", cx, |_, _| showcase_comments());
    let selected = window.use_keyed_state("comments-showcase-selected", cx, |_, _| {
        Some(SharedString::from("plan"))
    });
    let has_older = window.use_keyed_state("comments-showcase-older", cx, |_, _| true);
    let next_id = window.use_keyed_state("comments-showcase-next", cx, |_, _| 1u64);
    let threads = state.read(cx).clone();
    let (post, edit, remove, react, resolve, reopen, retry, versions, older, select) = (
        state.clone(),
        state.clone(),
        state.clone(),
        state.clone(),
        state.clone(),
        state.clone(),
        state.clone(),
        state.clone(),
        state.clone(),
        selected.clone(),
    );
    let older_flag = has_older.clone();
    Comments::new("comments-showcase-panel", threads)
        .events(showcase_events())
        .current_user(viewer())
        .selected_thread_id(selected.read(cx).clone())
        .on_selected_thread_change(move |id, _, cx| {
            select.update(cx, |s, cx| {
                *s = id;
                cx.notify();
            })
        })
        .density(CommentsDensity::Compact)
        .has_older_comments(*has_older.read(cx))
        .on_load_older(move |_, cx| {
            set(&older, cx, |threads| {
                threads.insert(
                    0,
                    CommentThreadData::new(
                        "older",
                        noah(),
                        "Earlier context loads above the threads.",
                        "2026-09-30T17:45:00Z",
                    ),
                )
            });
            older_flag.update(cx, |h, cx| {
                *h = false;
                cx.notify();
            });
            ok()
        })
        .agent_commands([CommentAgentCommand::new(
            "ask",
            "Release Assistant",
            "Ask about the rollout",
        )])
        .on_upload_images(|files, _, _, _| {
            Task::ready(Ok(files
                .into_iter()
                .map(|file| CommentUploadedImage {
                    url: file.path.to_string_lossy().to_string().into(),
                    alt: file.name,
                })
                .collect()))
        })
        .on_submit(move |body, parent, _, cx| {
            let n = next_id.update(cx, |n, _| {
                *n += 1;
                *n - 1
            });
            let comment = CommentData::new(format!("demo-{n}"), viewer(), body, now_ms());
            set(&post, cx, |threads| match parent {
                None => threads.push(comment.into()),
                Some(parent) => {
                    if let Some(root) = threads.iter_mut().find(|t| t.comment.id == parent) {
                        root.replies.push(comment);
                    }
                }
            });
            ok()
        })
        .on_save_edit(move |id, body, _, cx| {
            set(&edit, cx, |threads| {
                update_comment(threads, &id, |comment| {
                    let previous = CommentVersion::new(
                        format!("{}-{}", comment.id, now_ms()),
                        comment.body.clone(),
                        now_ms(),
                    )
                    .author(comment.author.clone());
                    comment.previous_versions.insert(0, previous);
                    comment.body = body.clone();
                    comment.is_edited = true;
                })
            });
            ok()
        })
        .on_delete(move |id, _, cx| {
            set(&remove, cx, |threads| {
                threads.retain(|t| t.comment.id != id);
                for thread in threads.iter_mut() {
                    thread.replies.retain(|r| r.id != id);
                }
            });
            ok()
        })
        .on_react(move |id, key, picked, _, cx| {
            let option: Option<CommentReactionOption> = picked.or_else(|| {
                default_comment_reactions()
                    .into_iter()
                    .find(|o| o.key == key)
            });
            let Some(option) = option else { return ok() };
            set(&react, cx, |threads| {
                update_comment(threads, &id, |comment| {
                    match comment.reactions.iter_mut().find(|r| r.option.key == key) {
                        None => comment.reactions.push(CommentReaction {
                            option: option.clone(),
                            count: 1,
                            reacted_by_viewer: true,
                            actors: vec![("avery".into(), "Avery Kim".into())],
                        }),
                        Some(reaction) => {
                            reaction.count = if reaction.reacted_by_viewer {
                                reaction.count - 1
                            } else {
                                reaction.count + 1
                            };
                            reaction.reacted_by_viewer = !reaction.reacted_by_viewer;
                        }
                    }
                    comment.reactions.retain(|r| r.count > 0);
                })
            });
            ok()
        })
        .on_resolve_thread(move |id, _, cx| {
            set(&resolve, cx, |threads| {
                if let Some(t) = threads.iter_mut().find(|t| t.comment.id == id) {
                    t.is_resolved = true;
                    t.resolved_by = Some(viewer());
                }
            });
            ok()
        })
        .on_reopen_thread(move |id, _, cx| {
            set(&reopen, cx, |threads| {
                if let Some(t) = threads.iter_mut().find(|t| t.comment.id == id) {
                    t.is_resolved = false;
                    t.resolved_by = None;
                }
            });
            ok()
        })
        .on_retry_comment(move |id, _, cx| {
            set(&retry, cx, |threads| {
                update_comment(threads, &id, |comment| {
                    comment.status = CommentStatus::Sent;
                    comment.error_message = None;
                })
            });
            ok()
        })
        .load_versions(move |id, _, cx| {
            let Some(comment) = find_comment(versions.read(cx), &id) else {
                return Task::ready(Ok(Vec::new()));
            };
            let mut list = vec![
                CommentVersion::new(
                    format!("{}-current", comment.id),
                    comment.body.clone(),
                    comment
                        .updated_at
                        .clone()
                        .unwrap_or_else(|| comment.created_at.clone()),
                )
                .author(comment.author.clone())
                .current(true),
            ];
            list.extend(comment.previous_versions.iter().cloned());
            Task::ready(Ok(list))
        })
        .into_any_element()
}

/// Upstream `CommentThreadShowcase`: a margin comment, one thread quoting its
/// passage, inside a Card.
fn thread_showcase(window: &mut Window, cx: &mut App) -> AnyElement {
    let state = window.use_keyed_state("comment-thread-showcase", cx, |_, _| {
        CommentThreadData::new(
            "margin",
            maya(),
            "Should this say three days? The dry run adds one.",
            "2026-09-30T19:00:00Z",
        )
        .anchor(CommentAnchor::new(
            "The final cohort completes rollout in two days.",
        ))
        .replies([CommentData::new(
            "margin-reply",
            viewer(),
            "Good catch, updating it.",
            "2026-09-30T19:20:00Z",
        )])
    });
    let (post, resolve, reopen) = (state.clone(), state.clone(), state.clone());
    Card::new()
        .padding(SpacingStep::S0)
        .width(relative(1.))
        .max_w(px(340.))
        .child(
            CommentThread::new("comment-thread-showcase-thread", state.read(cx).clone())
                .current_user(viewer())
                .container(CommentContainer::Card)
                .on_submit(move |body, _, _, cx| {
                    post.update(cx, |t, cx| {
                        t.replies.push(CommentData::new(
                            format!("reply-{}", now_ms()),
                            viewer(),
                            body,
                            now_ms(),
                        ));
                        cx.notify();
                    });
                    ok()
                })
                .on_resolve_thread(move |_, _, cx| {
                    resolve.update(cx, |t, cx| {
                        t.is_resolved = true;
                        cx.notify();
                    });
                    ok()
                })
                .on_reopen_thread(move |_, _, cx| {
                    reopen.update(cx, |t, cx| {
                        t.is_resolved = false;
                        cx.notify();
                    });
                    ok()
                }),
        )
        .into_any_element()
}

/// Upstream `CommentThreadInPopover`: a passage whose comment button opens one
/// thread in a Popover; the thread owns the Popover's dismissal and keeps an unsaved
/// reply on the first press outside or Escape.
fn thread_in_popover(window: &mut Window, cx: &mut App) -> AnyElement {
    let open = window.use_keyed_state("comment-thread-popover-open", cx, |_, _| false);
    let state = window.use_keyed_state("comment-thread-popover", cx, |_, _| {
        CommentThreadData::new(
            "passage",
            maya(),
            "Should this say three days? The dry run adds one.",
            "2026-09-30T19:00:00Z",
        )
        .anchor(CommentAnchor::new(
            "The final cohort completes rollout in two days.",
        ))
    });
    let thread = state.read(cx).clone();
    let replies = thread.replies.len();
    let (set_open, dismiss, post) = (open.clone(), open.clone(), state.clone());
    Card::new()
        .padding(SpacingStep::S4)
        .width(px(420.))
        .child(
            Stack::vertical()
                .gap(SpacingStep::S3)
                .child(Text::new(
                    "Every region moves to the new build in three cohorts. The final cohort completes rollout in two days.",
                ))
                .child(
                    // The thread owns dismissal: it keeps an unsaved reply on the first
                    // press outside or Escape, which a light-dismissing popover cannot.
                    Popover::new("comment-thread-popover-surface", "Comment thread")
                        .open(*open.read(cx))
                        .on_open_change(move |next, _, cx| {
                            set_open.update(cx, |o, cx| {
                                *o = next;
                                cx.notify();
                            })
                        })
                        .light_dismiss(false)
                        .escape_dismiss(false)
                        .padding(SpacingStep::S0)
                        .width(px(340.))
                        .content(
                            CommentThread::new("comment-thread-popover-thread", thread)
                                .current_user(viewer())
                                .container(CommentContainer::Card)
                                .on_dismiss(move |_, cx| {
                                    dismiss.update(cx, |o, cx| {
                                        *o = false;
                                        cx.notify();
                                    })
                                })
                                .on_submit(move |body, _, _, cx| {
                                    post.update(cx, |t, cx| {
                                        t.replies.push(CommentData::new(format!("reply-{}", now_ms()), viewer(), body, now_ms()));
                                        cx.notify();
                                    });
                                    ok()
                                }),
                        )
                        .child(
                            Button::new(
                                "comment-thread-popover-trigger",
                                if replies == 0 { "1 comment".to_string() } else { format!("{} comments", replies + 1) },
                            )
                            .variant(ButtonVariant::Secondary)
                            .size(ElementSize::Sm),
                        ),
                ),
        )
        .into_any_element()
}

/// Upstream `CommentComposerShowcase`: a comment composer that posts into a short
/// list above it.
fn composer_showcase(window: &mut Window, cx: &mut App) -> AnyElement {
    let draft = window.use_keyed_state("comment-composer-showcase-draft", cx, |_, _| {
        SharedString::default()
    });
    let posted = window.use_keyed_state("comment-composer-showcase-posted", cx, |_, _| {
        Vec::<SharedString>::new()
    });
    let list = posted.read(cx).clone();
    let (change, submit_draft, submit_posted) = (draft.clone(), draft.clone(), posted.clone());
    Stack::vertical()
        .gap(SpacingStep::S3)
        .w(px(420.))
        .children(if list.is_empty() {
            vec![
                Text::new("Nothing posted yet.")
                    .text_type(TextType::Supporting)
                    .color(TextColor::Secondary)
                    .into_any_element(),
            ]
        } else {
            list.into_iter()
                .map(|body| Text::new(body).text_type(TextType::Body).into_any_element())
                .collect()
        })
        .child(
            CommentComposer::new(
                "comment-composer-showcase",
                draft.read(cx).clone(),
                move |next, _, cx| {
                    change.update(cx, |d, cx| {
                        *d = next;
                        cx.notify();
                    })
                },
                move |body, _, cx| {
                    submit_posted.update(cx, |p, cx| {
                        p.push(body);
                        cx.notify();
                    });
                    submit_draft.update(cx, |d, cx| {
                        *d = SharedString::default();
                        cx.notify();
                    });
                },
            )
            .anchor(CommentAnchor::new(
                "Guardrail alerts page the release oncall.",
            ))
            .on_anchor_dismiss(|_, _| {}),
        )
        .into_any_element()
}

fn story(title: &'static str, description: &'static str, body: impl IntoElement) -> AnyElement {
    Stack::vertical()
        .gap(SpacingStep::S3)
        .child(Heading::new(HeadingLevel::H3, title))
        .children(
            (!description.is_empty()).then(|| Text::new(description).color(TextColor::Secondary)),
        )
        .child(body)
        .into_any_element()
}

fn sized(width: f32, height: Option<f32>, body: impl IntoElement) -> gpui::Div {
    let el = div().flex().flex_col().w(px(width));
    match height {
        Some(h) => el.h(px(h)),
        None => el,
    }
    .child(body)
}

/// The Comments examples: the full panel, inside a card, read-only, loading, empty, a
/// margin comment, a thread in a popover, an open thread, live state, the composer, a
/// load error, long content, narrow, saving an edit, and embedded in a popover.
pub fn examples(window: &mut Window, cx: &mut App) -> AnyElement {
    let long_name = CommentUser::new("long", "Maximiliana Alejandra Rodr\u{ed}guez-Vanderberg");
    Stack::vertical()
        .gap(SpacingStep::S8)
        .child(story("Panel", "Every feature, with working callbacks.", sized(920., Some(640.), comments_showcase(window, cx))))
        .child(story(
            "Inside a card",
            "Inside a card, threads are divider-bounded Sections so cards never nest.",
            Card::new().padding(SpacingStep::S0).width(px(380.)).h(px(440.)).child(
                Comments::new("comments-inside-card", story_comments())
                    .current_user(viewer())
                    .container(CommentContainer::Card)
                    .density(CommentsDensity::Compact)
                    .on_submit(|_, _, _, _| ok()),
            ),
        ))
        .child(story(
            "Read only",
            "Readers see reactions and replies, and no control that changes data.",
            sized(
                560.,
                Some(320.),
                Comments::new("comments-read-only", story_comments()).mode(CommentMode::ReadOnly).density(CommentsDensity::Compact),
            ),
        ))
        .child(story(
            "Loading",
            "",
            sized(
                560.,
                Some(420.),
                Comments::new("comments-loading", Vec::<CommentThreadData>::new())
                    .loading(true)
                    .current_user(viewer())
                    .density(CommentsDensity::Compact)
                    .on_submit(|_, _, _, _| ok()),
            ),
        ))
        .child(story(
            "Empty",
            "",
            sized(
                560.,
                Some(320.),
                Comments::new("comments-empty", Vec::<CommentThreadData>::new()).current_user(viewer()).on_submit(|_, _, _, _| ok()),
            ),
        ))
        .child(story("Margin comment", "One thread on its own: a margin comment beside a document.", thread_showcase(window, cx)))
        .child(story(
            "In popover",
            "A thread in a Popover that owns its dismissal: with an unsaved reply, the first press outside or Escape shakes the thread and keeps the draft.",
            thread_in_popover(window, cx),
        ))
        .child(story(
            "Open thread",
            "A lone thread a reader opened, with its replies showing.",
            sized(
                480.,
                None,
                CommentThread::new("comments-open-thread", story_comments().remove(0))
                    .current_user(viewer())
                    .selected(true)
                    .on_submit(|_, _, _, _| ok())
                    .on_react(|_, _, _, _, _| ok()),
            ),
        ))
        .child(story(
            "Live state",
            "Controlled typing plus hidden seen metadata, ready for a transport adapter.",
            sized(
                480.,
                None,
                CommentThread::new(
                    "comments-live-state",
                    story_comments()
                        .remove(0)
                        .typing_users([jordan(), sam()])
                        .seen_by([jordan(), sam()])
                        .replies([CommentData::new(
                            "live-reply",
                            viewer(),
                            "The review notes are incorporated.",
                            "2026-09-30T19:10:00Z",
                        )
                        .seen_by([maya(), jordan(), sam()])]),
                )
                .current_user(viewer())
                .selected(true)
                .on_submit(|_, _, _, _| ok()),
            ),
        ))
        .child(story("Composer", "", composer_showcase(window, cx)))
        .child(story(
            "Load error",
            "Loading failed: an error Banner with Retry above the threads.",
            sized(
                560.,
                Some(360.),
                Comments::new("comments-load-error", story_comments())
                    .current_user(viewer())
                    .load_error("Check your connection and try again.")
                    .on_retry_load(|_, cx| {
                        let timer = cx.background_executor().timer(std::time::Duration::from_millis(800));
                        cx.background_executor().spawn(async move {
                            timer.await;
                            Ok(())
                        })
                    })
                    .has_older_comments(true)
                    .on_load_older(|_, _| Task::ready(Err(())))
                    .density(CommentsDensity::Compact)
                    .on_submit(|_, _, _, _| ok()),
            ),
        ))
        .child(story(
            "Long content",
            "Long names, unbroken words, and long bodies wrap rather than overflow.",
            sized(
                420.,
                Some(560.),
                Comments::new(
                    "comments-long-content",
                    [CommentThreadData::new(
                        "long-root",
                        long_name,
                        "Supercalifragilisticexpialidocious-configuration-flag-name-that-never-breaks is still on in the canary. The rollout notes say the final cohort completes in two days, but the dashboard shows three, and the guardrail metrics have not settled for the last two regions, so I would hold the next step until the on-call confirms the numbers and the review queue is clear.",
                        "2026-09-30T19:00:00Z",
                    )
                    .edited(true)
                    .anchor(CommentAnchor::new(
                        "The final cohort completes rollout in two days, after the guardrail metrics settle in every region and the review queue is clear.",
                    ))
                    .replies([CommentData::new(
                        "long-reply",
                        viewer(),
                        "https://example.com/a/very/long/link/that/keeps/going/and/going/without/any/spaces/at/all",
                        "2026-09-30T19:10:00Z",
                    )])],
                )
                .current_user(viewer())
                .density(CommentsDensity::Compact)
                .selected_thread_id(Some("long-root"))
                .on_selected_thread_change(|_, _, _| {})
                .on_submit(|_, _, _, _| ok()),
            ),
        ))
        .child(story(
            "Narrow",
            "At 320 pixels the header actions collapse into the more menu.",
            sized(
                320.,
                Some(520.),
                Comments::new("comments-narrow", story_comments())
                    .current_user(viewer())
                    .density(CommentsDensity::Compact)
                    .on_submit(|_, _, _, _| ok())
                    .on_react(|_, _, _, _, _| ok())
                    .on_resolve_thread(|_, _, _| ok())
                    .on_save_edit(|_, _, _, _| ok())
                    .on_delete(|_, _, _| ok()),
            ),
        ))
        .child(story(
            "Saving an edit",
            "Saving an edit that has not finished: the editor stays busy.",
            sized(
                480.,
                Some(520.),
                Comments::new("comments-saving-edit", story_comments())
                    .current_user(viewer())
                    .on_submit(|_, _, _, _| ok())
                    .on_save_edit(|_, _, _, cx| cx.background_executor().spawn(std::future::pending()))
                    .on_delete(|_, _, _| ok()),
            ),
        ))
        .child(story(
            "Embedded in a popover",
            "Embedded at its content height inside a popover: no scroll area or dock.",
            Popover::new("comments-embedded-popover", "Comments")
                .width(px(380.))
                .padding(SpacingStep::S0)
                .content(
                    Comments::new("comments-embedded", story_comments())
                        .current_user(viewer())
                        .layout(CommentsLayout::Embedded)
                        .container(CommentContainer::Card)
                        .density(CommentsDensity::Compact)
                        .on_submit(|_, _, _, _| ok()),
                )
                .child(Button::new("comments-embedded-trigger", "Show comments")),
        ))
        .into_any_element()
}
