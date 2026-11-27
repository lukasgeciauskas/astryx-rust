//! Upstream lab `Comments.test.tsx`, `CommentThread.test.tsx`,
//! `CommentComposer.test.tsx` and `icons.test.tsx`, the parts a GPUI window can
//! drive, and the helpers' rules.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;

use gpui::prelude::*;
use gpui::{
    AnyElement, App, Bounds, Context, Entity, Modifiers, Pixels, Render, SharedString, Task,
    TestAppContext, VisualTestContext, Window, div, px,
};
use rustryx_primitives::announce::{Politeness, announcement};

use super::context::{ControllerState, EditTarget, ReplyTarget};
use super::dismissal::{UnsavedDraft, shake_offset, unsaved_draft_of};
use super::*;

// Fixtures.

fn viewer() -> CommentUser {
    CommentUser::new("avery", "Avery Kim")
}

fn other() -> CommentUser {
    CommentUser::new("maya", "Maya Chen")
}

fn agent() -> CommentUser {
    CommentUser::agent("release-bot", "Release Assistant").avatar_url("/agent-logo.png")
}

/// Upstream's `COMMENTS`: one edited thread of the viewer's with a reaction and a
/// reply.
fn comments() -> Vec<CommentThreadData> {
    vec![
        CommentThreadData::new(
            "root-1",
            viewer(),
            "The final states are **ready** for review.",
            "2026-09-30T19:00:00Z",
        )
        .edited(true)
        .previous_versions([CommentVersion::new(
            "root-1-v1",
            "Earlier wording",
            "2026-09-30T18:00:00Z",
        )
        .author(viewer())])
        .reactions([CommentReaction::new("like", "\u{1F44D}", 2)
            .description("Like")
            .reacted_by_viewer(true)
            .actors([("avery", "Avery Kim"), ("maya", "Maya Chen")])])
        .replies([CommentData::new(
            "reply-1",
            other(),
            "A one-level reply",
            "2026-09-30T19:10:00Z",
        )]),
    ]
}

fn no_replies() -> Vec<CommentThreadData> {
    let mut threads = comments();
    threads[0].replies.clear();
    threads
}

fn ok() -> CommentTask {
    Task::ready(Ok(()))
}

fn fail() -> CommentTask {
    Task::ready(Err(()))
}

// The window.

type BuildFn = Rc<dyn Fn(&mut Window, &mut App) -> AnyElement>;

struct Host {
    build: BuildFn,
    width: f32,
}

impl Render for Host {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .w(px(self.width))
            .h(px(760.))
            .child(
                div()
                    .id("outside")
                    .debug_selector(|| "outside".into())
                    .h(px(40.))
                    .w_full(),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .h(px(600.))
                    .w_full()
                    .child((self.build)(window, cx)),
            )
    }
}

fn host_at(
    cx: &mut TestAppContext,
    width: f32,
    build: impl Fn(&mut Window, &mut App) -> AnyElement + 'static,
) -> &mut VisualTestContext {
    cx.update(|cx| {
        rustryx_theme::init(cx);
        rustryx_primitives::focus::init(cx);
        rustryx_primitives::editing::init(cx);
    });
    let (_, cx) = cx.add_window_view(move |_, _| Host {
        build: Rc::new(build),
        width,
    });
    cx.update(|window, _| window.activate_window());
    settle(cx);
    cx
}

fn host(
    cx: &mut TestAppContext,
    build: impl Fn(&mut Window, &mut App) -> AnyElement + 'static,
) -> &mut VisualTestContext {
    host_at(cx, 720., build)
}

fn settle(cx: &mut VisualTestContext) {
    for _ in 0..4 {
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
    }
    // Popovers and reveals hold input through their entrance.
    cx.executor().advance_clock(Duration::from_millis(400));
    cx.run_until_parked();
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
}

fn leak(selector: &str) -> &'static str {
    Box::leak(selector.to_string().into_boxed_str())
}

fn bounds(cx: &mut VisualTestContext, selector: &str) -> Option<Bounds<Pixels>> {
    cx.debug_bounds(leak(selector))
}

fn shown(cx: &mut VisualTestContext, selector: &str) -> bool {
    settle(cx);
    bounds(cx, selector).is_some()
}

fn click(cx: &mut VisualTestContext, selector: &str) {
    settle(cx);
    let b = bounds(cx, selector).unwrap_or_else(|| panic!("{selector} is drawn"));
    cx.simulate_click(b.center(), Modifiers::none());
    settle(cx);
}

/// Rests the pointer on a thread, so its hover-revealed actions show.
fn hover(cx: &mut VisualTestContext, thread: &str) {
    settle(cx);
    let b = bounds(cx, &format!("thread-{thread}"))
        .unwrap_or_else(|| panic!("thread {thread} is drawn"));
    cx.simulate_mouse_move(
        b.origin + gpui::point(px(4.), px(4.)),
        None,
        Modifiers::none(),
    );
    settle(cx);
    settle(cx);
}

/// A full key press: GPUI fires keyboard clicks on key-up, and
/// `simulate_keystrokes` sends only key-down.
fn key(cx: &mut VisualTestContext, key: &'static str) {
    let keystroke =
        gpui::Keystroke::parse(key).unwrap_or_else(|_| panic!("not a keystroke: {key}"));
    cx.simulate_event(gpui::KeyDownEvent {
        keystroke: keystroke.clone(),
        is_held: false,
        prefer_character_input: false,
    });
    cx.simulate_event(gpui::KeyUpEvent { keystroke });
    settle(cx);
}

/// Focuses a control by its element id and presses `k` on it.
fn press(cx: &mut VisualTestContext, id: &str, k: &'static str) {
    let id = gpui::ElementId::Name(id.to_string().into());
    cx.update(|_, cx| rustryx_primitives::request_focus(&id, cx));
    settle(cx);
    key(cx, k);
}

fn said(cx: &mut VisualTestContext, politeness: Politeness) -> SharedString {
    cx.update(|window, cx| announcement(politeness, window, cx))
}

fn y(cx: &mut VisualTestContext, selector: &str) -> f32 {
    f32::from(
        bounds(cx, selector)
            .unwrap_or_else(|| panic!("{selector} is drawn"))
            .origin
            .y,
    )
}

// A plain composer standing in for the editable one (upstream `TestComposer`).

#[derive(Clone, Debug, Default, PartialEq)]
struct Rendered {
    value: String,
    placeholder: String,
    error: Option<String>,
    context_label: Option<String>,
    cancellable: bool,
}

type Composers = Rc<RefCell<HashMap<String, Rendered>>>;

fn kind(context: &CommentComposerContext) -> &'static str {
    match context {
        CommentComposerContext::Comment { .. } => "comment",
        CommentComposerContext::Edit { .. } => "edit",
        CommentComposerContext::Reply { .. } => "reply",
    }
}

fn test_button(id: String, f: impl Fn(&mut Window, &mut App) + 'static) -> AnyElement {
    let selector = id.clone();
    div()
        .id(gpui::ElementId::Name(id.into()))
        .debug_selector(move || selector)
        .w(px(40.))
        .h(px(20.))
        .on_click(move |_, window, cx| f(window, cx))
        .into_any_element()
}

fn test_composer(
    log: &Composers,
) -> impl Fn(CommentComposerRenderProps, &mut Window, &mut App) -> AnyElement + 'static {
    let log = log.clone();
    move |props, _, _| {
        let k = kind(&props.context);
        log.borrow_mut().insert(
            k.to_string(),
            Rendered {
                value: props.value.to_string(),
                placeholder: props.placeholder.to_string(),
                error: props.error.as_ref().map(|e| e.to_string()),
                context_label: props.context_label.as_ref().map(|e| e.to_string()),
                cancellable: props.on_cancel.is_some(),
            },
        );
        let (change, value, submit) = (
            props.on_change.clone(),
            props.value.clone(),
            props.on_submit.clone(),
        );
        let next = match props.context {
            CommentComposerContext::Reply { .. } => format!("{value}Reply body"),
            CommentComposerContext::Edit { .. } => "Edited comment body".to_string(),
            CommentComposerContext::Comment { .. } => "New comment body".to_string(),
        };
        let submitted = props.value.clone();
        div()
            .flex()
            .gap(px(4.))
            .child(test_button(format!("set-draft-{k}"), move |w, cx| {
                change(next.clone().into(), w, cx)
            }))
            .child(test_button(format!("submit-{k}"), move |w, cx| {
                submit(submitted.clone(), w, cx)
            }))
            .children(
                props
                    .on_cancel
                    .clone()
                    .map(|cancel| test_button(format!("cancel-{k}"), move |w, cx| cancel(w, cx))),
            )
            .into_any_element()
    }
}

type Calls = Rc<RefCell<Vec<String>>>;

fn draft_log(
    calls: &Calls,
) -> impl Fn(SharedString, CommentComposerContext, &mut Window, &mut App) + 'static {
    let calls = calls.clone();
    move |value, context, _, _| {
        let context = match context {
            CommentComposerContext::Comment { anchor } => {
                format!(
                    "comment{}",
                    anchor.map(|a| format!(":{}", a.quote)).unwrap_or_default()
                )
            }
            CommentComposerContext::Edit { comment_id } => format!("edit:{comment_id}"),
            CommentComposerContext::Reply {
                parent_comment_id,
                replied_to_comment_id,
            } => format!("reply:{parent_comment_id}:{replied_to_comment_id}"),
        };
        calls.borrow_mut().push(format!("{value}|{context}"));
    }
}

fn last(calls: &Calls) -> String {
    calls.borrow().last().cloned().unwrap_or_default()
}

// The helpers' rules.

#[test]
fn derives_an_unsaved_draft_from_typed_replies_and_changed_edits_only() {
    let comment = CommentData::new("c1", viewer(), "Saved body", "2026-09-30T19:00:00Z");
    let mut state = ControllerState::default();
    assert_eq!(unsaved_draft_of(&state), None);
    state.reply_target = Some(ReplyTarget {
        parent_id: "c1".into(),
        comment: comment.clone(),
        focus_request: None,
        seed: Some("@maya ".into()),
    });
    // The seeded mention alone, or whitespace, is not a draft.
    state.reply_draft = "@maya ".into();
    assert_eq!(unsaved_draft_of(&state), None);
    state.reply_draft = "  \n ".into();
    assert_eq!(unsaved_draft_of(&state), None);
    state.reply_draft = "@maya hello".into();
    assert_eq!(
        unsaved_draft_of(&state),
        Some(UnsavedDraft::Reply {
            parent_id: "c1".into(),
            signature: "reply:c1:@maya hello".into()
        })
    );
    // An edit that only changes surrounding whitespace is not a draft, and an edit
    // that differs wins over the reply.
    state.edit_target = Some(EditTarget {
        comment: comment.clone(),
        value: " Saved body ".into(),
    });
    assert!(matches!(
        unsaved_draft_of(&state),
        Some(UnsavedDraft::Reply { .. })
    ));
    state.edit_target = Some(EditTarget {
        comment,
        value: "Changed body".into(),
    });
    assert_eq!(
        unsaved_draft_of(&state),
        Some(UnsavedDraft::Edit {
            comment_id: "c1".into(),
            signature: "edit:c1:Changed body".into()
        })
    );
}

#[test]
fn the_shake_is_symmetric_and_comes_to_rest() {
    let d = Duration::from_millis(410);
    let distance = px(4.);
    assert_eq!(shake_offset(Duration::ZERO, d, distance), Some(px(0.)));
    assert_eq!(shake_offset(d, d, distance), None);
    assert_eq!(
        shake_offset(Duration::from_millis(5), Duration::ZERO, distance),
        None
    );
    let mut low = 0f32;
    let mut high = 0f32;
    for ms in 0..410 {
        let x = f32::from(shake_offset(Duration::from_millis(ms), d, distance).unwrap());
        low = low.min(x);
        high = high.max(x);
    }
    assert!(
        (low + 4.).abs() < 0.2 && (high - 4.).abs() < 0.2,
        "{low} {high}"
    );
}

#[test]
fn names_reactions_in_the_viewers_language() {
    let i18n = rustryx_i18n::I18n::new("en-US");
    let like = CommentReactionOption::new("like", "\u{1F44D}").description("Like");
    assert_eq!(i18n::reaction_description(&i18n, &like), "Like");
    let defaults = default_comment_reactions();
    assert_eq!(
        defaults.iter().map(|o| o.key.as_ref()).collect::<Vec<_>>(),
        ["like", "love", "laugh", "wow", "sad", "celebrate"]
    );
    assert_eq!(i18n::reaction_description(&i18n, &defaults[5]), "Celebrate");
    let count = i18n.format(
        "@astryx.comments.reactionCount",
        &[
            ("reaction", rustryx_i18n::Arg::from("Like")),
            ("count", rustryx_i18n::Arg::Num(2.)),
        ],
    );
    assert_eq!(count, "Like: 2");
}

#[test]
fn draws_the_bundled_glyphs() {
    use super::icons::{CommentIconName, parse_path};
    for name in [
        CommentIconName::AddReaction,
        CommentIconName::Reply,
        CommentIconName::CopyLink,
        CommentIconName::Attach,
        CommentIconName::Emoji,
        CommentIconName::Event,
    ] {
        assert!(
            name.paths()
                .unwrap_or_default()
                .iter()
                .all(|d| !parse_path(d).is_empty()),
            "{:?}",
            name.key()
        );
    }
}

// Comments rendering.

#[gpui::test]
fn renders_threads_reactions_and_a_collapsed_reply(cx: &mut TestAppContext) {
    let cx = host(cx, |_, _| {
        Comments::new("c", comments())
            .current_user(viewer())
            .into_any_element()
    });
    assert!(shown(cx, "thread-root-1"));
    assert!(shown(cx, "comment-root-1"));
    assert!(shown(cx, "comment-reaction-root-1-like"));
    // Replies start hidden behind the disclosure link.
    assert!(shown(cx, "reply-disclosure-root-1"));
    assert!(!shown(cx, "comment-reply-1"));
    let i18n = rustryx_i18n::I18n::new("en-US");
    assert_eq!(
        i18n.format(
            "@astryx.comments.viewReplies",
            &[("count", rustryx_i18n::Arg::Num(1.))]
        ),
        "View 1 reply"
    );
}

#[gpui::test]
fn renders_the_empty_state_when_there_is_nothing_to_show(cx: &mut TestAppContext) {
    let cx = host(cx, |_, _| {
        Comments::new("c", Vec::<CommentThreadData>::new())
            .current_user(viewer())
            .on_submit(|_, _, _, _| ok())
            .into_any_element()
    });
    assert!(shown(cx, "c-empty"));
    assert!(shown(cx, "c-composer"));
}

#[gpui::test]
fn interleaves_activity_events_with_threads_by_time(cx: &mut TestAppContext) {
    let cx = host(cx, |_, _| {
        Comments::new(
            "c",
            [
                CommentThreadData::new("early", other(), "First", "2026-09-30T19:00:00Z"),
                CommentThreadData::new("late", viewer(), "Third", "2026-09-30T19:20:00Z"),
            ],
        )
        .events([CommentActivityEvent::new(
            "status",
            "Status changed to Ready for review",
            "2026-09-30T19:10:00Z",
        )])
        .into_any_element()
    });
    settle(cx);
    let (early, event, late) = (
        y(cx, "thread-early"),
        y(cx, "comments-event-status"),
        y(cx, "thread-late"),
    );
    assert!(early < event && event < late, "{early} {event} {late}");
}

#[gpui::test]
fn a_list_running_newest_first_keeps_that_direction(cx: &mut TestAppContext) {
    let cx = host(cx, |_, _| {
        Comments::new(
            "c",
            [
                CommentThreadData::new("late", viewer(), "Third", "2026-09-30T19:20:00Z"),
                CommentThreadData::new("early", other(), "First", "2026-09-30T19:00:00Z"),
            ],
        )
        .events([CommentActivityEvent::new(
            "status",
            "Status changed",
            "2026-09-30T19:10:00Z",
        )])
        .into_any_element()
    });
    settle(cx);
    let (late, event, early) = (
        y(cx, "thread-late"),
        y(cx, "comments-event-status"),
        y(cx, "thread-early"),
    );
    assert!(late < event && event < early, "{late} {event} {early}");
}

#[gpui::test]
fn a_comment_still_being_written_takes_no_replies_or_reactions(cx: &mut TestAppContext) {
    let cx = host(cx, |_, _| {
        Comments::new(
            "c",
            [
                CommentThreadData::new("agent-root", agent(), "", "2026-09-30T19:00:00Z").progress(
                    "Checking the rollout\u{2026}",
                    Some(("Open session".into(), "#session".into())),
                ),
            ],
        )
        .current_user(viewer())
        .on_submit(|_, _, _, _| ok())
        .on_react(|_, _, _, _, _| ok())
        .into_any_element()
    });
    assert!(shown(cx, "comment-agent-root"));
    hover(cx, "agent-root");
    assert!(!shown(cx, "comment-action-reply-agent-root"));
    assert!(!shown(cx, "comment-action-react-agent-root"));
}

#[gpui::test]
fn reveals_unread_replies_without_selecting_focusing_or_opening_a_composer(
    cx: &mut TestAppContext,
) {
    let changes: Calls = Rc::default();
    let log = changes.clone();
    let cx = host(cx, move |_, _| {
        let mut threads = comments();
        threads[0].replies = vec![
            CommentData::new(
                "unread-reply",
                other(),
                "A new unseen reply.",
                "2026-09-30T19:10:00Z",
            )
            .unread(true),
        ];
        let log = log.clone();
        Comments::new("c", threads)
            .current_user(viewer())
            .composer_position(CommentsComposerPosition::None)
            .on_submit(|_, _, _, _| ok())
            .on_selected_thread_change(move |id, _, _| log.borrow_mut().push(format!("{id:?}")))
            .into_any_element()
    });
    assert!(shown(cx, "unread-replies-revealed-root-1"));
    assert!(shown(cx, "comment-unread-reply"));
    assert!(!shown(cx, "reply-disclosure-root-1"), "no Hide replies");
    assert!(!shown(cx, "reply-composer-root-1"));
    assert!(changes.borrow().is_empty());
    assert!(cx.update(|window, cx| window.focused(cx).is_none()));
}

#[gpui::test]
fn shows_read_only_reactions_but_no_control_that_changes_data(cx: &mut TestAppContext) {
    let cx = host(cx, |_, _| {
        Comments::new("c", comments())
            .current_user(viewer())
            .mode(CommentMode::ReadOnly)
            .on_submit(|_, _, _, _| ok())
            .on_react(|_, _, _, _, _| ok())
            .on_save_edit(|_, _, _, _| ok())
            .on_delete(|_, _, _| ok())
            .into_any_element()
    });
    assert!(shown(cx, "comment-reaction-root-1-like"));
    hover(cx, "root-1");
    assert!(!shown(cx, "comment-action-reply-root-1"));
    assert!(!shown(cx, "comment-more-root-1"));
    assert!(!shown(cx, "c-composer"));
}

#[gpui::test]
fn keeps_reply_and_react_in_non_editable_mode_without_edit_or_delete(cx: &mut TestAppContext) {
    let cx = host(cx, |_, _| {
        Comments::new("c", comments())
            .current_user(viewer())
            .mode(CommentMode::NonEditable)
            .on_submit(|_, _, _, _| ok())
            .on_react(|_, _, _, _, _| ok())
            .on_save_edit(|_, _, _, _| ok())
            .on_delete(|_, _, _| ok())
            .into_any_element()
    });
    hover(cx, "root-1");
    assert!(shown(cx, "comment-action-reply-root-1"));
    assert!(shown(cx, "comment-action-react-root-1"));
    assert!(!shown(cx, "comment-more-root-1"));
}

#[gpui::test]
fn renders_embedded_at_content_height_and_a_pane_at_its_containers(cx: &mut TestAppContext) {
    let cx = host(cx, |_, _| {
        div()
            .flex()
            .h(px(600.))
            .child(
                div().w(px(340.)).h_full().child(
                    Comments::new("pane", comments())
                        .container(CommentContainer::Card)
                        .composer_position(CommentsComposerPosition::None),
                ),
            )
            .child(
                div().w(px(340.)).h_full().child(
                    Comments::new("embedded", comments())
                        .container(CommentContainer::Card)
                        .layout(CommentsLayout::Embedded)
                        .composer_position(CommentsComposerPosition::None),
                ),
            )
            .into_any_element()
    });
    settle(cx);
    let pane = f32::from(bounds(cx, "pane").unwrap().size.height);
    let embedded = f32::from(bounds(cx, "embedded").unwrap().size.height);
    assert!((pane - 600.).abs() < 1., "{pane}");
    assert!(embedded < 300., "{embedded}");
}

#[gpui::test]
fn shows_the_loading_shape_while_loading(cx: &mut TestAppContext) {
    let cx = host(cx, |_, _| {
        Comments::new("c", Vec::<CommentThreadData>::new())
            .loading(true)
            .current_user(viewer())
            .on_submit(|_, _, _, _| ok())
            .into_any_element()
    });
    assert!(shown(cx, "comments-loading-threads"));
    assert!(shown(cx, "comments-loading-composer"));
    assert!(!shown(cx, "c-empty"));
}

// Live collaboration seams.

#[gpui::test]
fn shows_typing_only_after_the_thread_opens_and_keeps_seen_by_hidden(cx: &mut TestAppContext) {
    let bodies: Rc<RefCell<Vec<usize>>> = Rc::default();
    let seen = bodies.clone();
    let cx = host(cx, move |_, _| {
        let mut threads = no_replies();
        threads[0] = threads[0]
            .clone()
            .typing_users([viewer(), other(), other(), agent()])
            .seen_by([other(), agent()]);
        let seen = seen.clone();
        Comments::new("c", threads)
            .current_user(viewer())
            .render_body(move |comment, _, _| {
                seen.borrow_mut().push(comment.seen_by.len());
                div()
                    .debug_selector(|| "custom-body".into())
                    .child(comment.body.clone())
                    .into_any_element()
            })
            .into_any_element()
    });
    assert!(!shown(cx, "thread-typing-root-1"));
    assert!(
        bodies.borrow().iter().all(|n| *n == 2),
        "render_body sees seen_by"
    );
    click(cx, "custom-body");
    assert!(shown(cx, "thread-typing-root-1"));
    let names: Vec<SharedString> = vec!["Maya Chen".into(), "Release Assistant".into()];
    let i18n = rustryx_i18n::I18n::new("en-US");
    assert_eq!(
        crate::chat::typing_label(&names, &i18n, "en-US").unwrap(),
        "Maya Chen and Release Assistant are typing\u{2026}"
    );
}

#[gpui::test]
fn reports_a_new_comment_draft_with_its_anchor_and_clears_it_after_posting(
    cx: &mut TestAppContext,
) {
    let calls: Calls = Rc::default();
    let composers: Composers = Rc::default();
    let (c, r) = (calls.clone(), composers.clone());
    let cx = host(cx, move |_, _| {
        Comments::new("c", comments())
            .current_user(viewer())
            .composer_anchor(CommentAnchor::new("The rollout completes on Friday."))
            .on_submit(|_, _, _, _| ok())
            .on_composer_draft_change(draft_log(&c))
            .render_composer(test_composer(&r))
            .into_any_element()
    });
    click(cx, "set-draft-comment");
    assert_eq!(
        last(&calls),
        "New comment body|comment:The rollout completes on Friday."
    );
    click(cx, "submit-comment");
    assert_eq!(last(&calls), "|comment:The rollout completes on Friday.");
    assert_eq!(composers.borrow()["comment"].value, "");
}

#[gpui::test]
fn restores_a_failed_draft_without_reporting_a_false_clear(cx: &mut TestAppContext) {
    let calls: Calls = Rc::default();
    let composers: Composers = Rc::default();
    let (c, r) = (calls.clone(), composers.clone());
    let cx = host(cx, move |_, _| {
        Comments::new("c", comments())
            .current_user(viewer())
            .on_submit(|_, _, _, _| fail())
            .on_composer_draft_change(draft_log(&c))
            .render_composer(test_composer(&r))
            .into_any_element()
    });
    click(cx, "set-draft-comment");
    click(cx, "submit-comment");
    assert_eq!(last(&calls), "New comment body|comment");
    assert!(!calls.borrow().iter().any(|c| c == "|comment"));
    let rendered = composers.borrow()["comment"].clone();
    assert_eq!(rendered.value, "New comment body");
    assert_eq!(
        rendered.error.as_deref(),
        Some("Your comment couldn't be posted.")
    );
}

// Threads.

#[gpui::test]
fn opens_a_pressed_thread_into_its_reply_composer_and_shows_its_replies(cx: &mut TestAppContext) {
    let composers: Composers = Rc::default();
    let r = composers.clone();
    let cx = host(cx, move |_, _| {
        Comments::new("c", comments())
            .current_user(viewer())
            .on_submit(|_, _, _, _| ok())
            .render_composer(test_composer(&r))
            .into_any_element()
    });
    click(cx, "comment-body-clamped-root-1");
    assert!(shown(cx, "comment-body-expanded-root-1"));
    assert!(shown(cx, "comment-reply-1"));
    assert!(shown(cx, "reply-composer-root-1"));
    let reply = composers.borrow()["reply"].clone();
    assert_eq!(reply.placeholder, "Reply to Avery Kim\u{2026}");
    assert_eq!(
        reply.context_label.as_deref(),
        Some("Replying to Avery Kim")
    );
}

#[gpui::test]
fn keeps_one_thread_selected_at_a_time(cx: &mut TestAppContext) {
    let cx = host(cx, |_, _| {
        Comments::new(
            "c",
            [
                CommentThreadData::new("a", other(), "First", "2026-09-30T19:00:00Z"),
                CommentThreadData::new("b", viewer(), "Second", "2026-09-30T19:05:00Z"),
            ],
        )
        .into_any_element()
    });
    click(cx, "comment-body-clamped-a");
    assert!(shown(cx, "comment-body-expanded-a"));
    click(cx, "comment-body-clamped-b");
    assert!(shown(cx, "comment-body-clamped-a"));
    assert!(shown(cx, "comment-body-expanded-b"));
}

#[gpui::test]
fn posts_a_reply_to_a_reply_under_the_thread_seeded_by_get_reply_mention(cx: &mut TestAppContext) {
    let calls: Calls = Rc::default();
    let posts: Calls = Rc::default();
    let composers: Composers = Rc::default();
    let (c, p, r) = (calls.clone(), posts.clone(), composers.clone());
    let cx = host(cx, move |_, _| {
        let p = p.clone();
        Comments::new("c", comments())
            .current_user(viewer())
            .on_submit(move |body, parent, _, _| {
                p.borrow_mut().push(format!("{body}|{parent:?}"));
                ok()
            })
            .on_composer_draft_change(draft_log(&c))
            .get_reply_mention(|author| Some(format!("@{} ", author.id).into()))
            .render_composer(test_composer(&r))
            .into_any_element()
    });
    click(cx, "reply-disclosure-root-1");
    hover(cx, "root-1");
    click(cx, "comment-action-reply-reply-1");
    let reply = composers.borrow()["reply"].clone();
    assert_eq!(reply.value, "@maya ");
    assert_eq!(
        reply.context_label.as_deref(),
        Some("Replying to Maya Chen")
    );
    assert_eq!(last(&calls), "@maya |reply:root-1:reply-1");
    click(cx, "set-draft-reply");
    assert_eq!(last(&calls), "@maya Reply body|reply:root-1:reply-1");
    click(cx, "submit-reply");
    assert_eq!(*posts.borrow(), ["@maya Reply body|Some(\"root-1\")"]);
    assert_eq!(last(&calls), "|reply:root-1:reply-1");
}

#[gpui::test]
fn posts_a_new_comment_and_clears_the_draft(cx: &mut TestAppContext) {
    let posts: Calls = Rc::default();
    let composers: Composers = Rc::default();
    let (p, r) = (posts.clone(), composers.clone());
    let cx = host(cx, move |_, _| {
        let p = p.clone();
        Comments::new("c", comments())
            .current_user(viewer())
            .on_submit(move |body, parent, _, _| {
                p.borrow_mut().push(format!("{body}|{parent:?}"));
                ok()
            })
            .render_composer(test_composer(&r))
            .into_any_element()
    });
    click(cx, "set-draft-comment");
    click(cx, "submit-comment");
    assert_eq!(*posts.borrow(), ["New comment body|None"]);
    assert_eq!(composers.borrow()["comment"].value, "");
}

#[gpui::test]
fn edits_the_viewers_own_comment_from_the_more_menu(cx: &mut TestAppContext) {
    let calls: Calls = Rc::default();
    let saves: Calls = Rc::default();
    let composers: Composers = Rc::default();
    let (c, s, r) = (calls.clone(), saves.clone(), composers.clone());
    let cx = host(cx, move |_, _| {
        let s = s.clone();
        Comments::new("c", comments())
            .current_user(viewer())
            .on_save_edit(move |id, body, _, _| {
                s.borrow_mut().push(format!("{id}|{body}"));
                ok()
            })
            .on_composer_draft_change(draft_log(&c))
            .on_delete(|_, _, _| ok())
            .render_composer(test_composer(&r))
            .into_any_element()
    });
    hover(cx, "root-1");
    click(cx, "comment-more-root-1");
    click(cx, "c-more-root-1-list-edit");
    assert_eq!(
        last(&calls),
        "The final states are **ready** for review.|edit:root-1"
    );
    assert_eq!(
        composers.borrow()["edit"].context_label.as_deref(),
        Some("Editing comment")
    );
    click(cx, "set-draft-edit");
    assert_eq!(last(&calls), "Edited comment body|edit:root-1");
    click(cx, "submit-edit");
    assert_eq!(*saves.borrow(), ["root-1|Edited comment body"]);
    assert_eq!(last(&calls), "|edit:root-1");
    composers.borrow_mut().clear();
    settle(cx);
    assert!(
        !composers.borrow().contains_key("edit"),
        "the editor closed"
    );
}

#[gpui::test]
fn returns_focus_to_the_comments_actions_when_an_edit_is_cancelled(cx: &mut TestAppContext) {
    let composers: Composers = Rc::default();
    let r = composers.clone();
    let cx = host(cx, move |_, _| {
        Comments::new("c", comments())
            .current_user(viewer())
            .on_save_edit(|_, _, _, _| ok())
            .render_composer(test_composer(&r))
            .into_any_element()
    });
    hover(cx, "root-1");
    click(cx, "comment-more-root-1");
    click(cx, "c-more-root-1-list-edit");
    assert!(composers.borrow()["edit"].cancellable);
    click(cx, "cancel-edit");
    // Focus is on the more menu: Enter opens it.
    key(cx, "enter");
    assert!(
        shown(cx, "c-more-root-1-list-edit"),
        "focus is on the more menu"
    );
}

#[gpui::test]
fn confirms_a_deletion_before_calling_the_host(cx: &mut TestAppContext) {
    let deletes: Calls = Rc::default();
    let d = deletes.clone();
    let cx = host(cx, move |_, _| {
        let d = d.clone();
        Comments::new("c", comments())
            .current_user(viewer())
            .on_delete(move |id, _, _| {
                d.borrow_mut().push(id.to_string());
                ok()
            })
            .into_any_element()
    });
    hover(cx, "root-1");
    click(cx, "comment-more-root-1");
    click(cx, "c-more-root-1-list-delete");
    assert!(deletes.borrow().is_empty());
    press(cx, "c-delete-dialog-action", "enter");
    assert_eq!(*deletes.borrow(), ["root-1"]);
}

#[gpui::test]
fn announces_a_failed_delete_once_and_keeps_the_confirmation(cx: &mut TestAppContext) {
    let cx = host(cx, |_, _| {
        Comments::new("c", comments())
            .current_user(viewer())
            .on_delete(|_, _, _| fail())
            .into_any_element()
    });
    hover(cx, "root-1");
    click(cx, "comment-more-root-1");
    click(cx, "c-more-root-1-list-delete");
    press(cx, "c-delete-dialog-action", "enter");
    let message = said(cx, Politeness::Assertive);
    assert!(message.contains("couldn't be deleted"), "{message}");
    // The row keeps the failure silently: the dialog already spoke it.
    assert!(shown(cx, "comment-root-1"));
}

#[gpui::test]
fn removes_the_fading_new_badge_once_it_has_shown(cx: &mut TestAppContext) {
    let cx = host(cx, |_, _| {
        let mut threads = no_replies();
        threads[0].comment.is_unread = true;
        Comments::new("c", threads).into_any_element()
    });
    assert!(shown(cx, "comment-unread-badge"));
    for _ in 0..30 {
        cx.executor().advance_clock(Duration::from_millis(250));
        settle(cx);
    }
    assert!(!shown(cx, "comment-unread-badge"));
}

#[gpui::test]
fn toggles_a_reaction_and_picks_a_new_one_from_add_reaction(cx: &mut TestAppContext) {
    let reacts: Calls = Rc::default();
    let r = reacts.clone();
    let cx = host(cx, move |_, _| {
        let r = r.clone();
        Comments::new("c", comments())
            .current_user(viewer())
            .on_react(move |id, key, option, _, _| {
                r.borrow_mut()
                    .push(format!("{id}|{key}|{:?}", option.map(|o| o.label)));
                ok()
            })
            .into_any_element()
    });
    click(cx, "comment-reaction-root-1-like");
    assert_eq!(*reacts.borrow(), ["root-1|like|None"]);
    hover(cx, "root-1");
    click(cx, "comment-action-react-root-1");
    assert!(shown(cx, "reaction-picker"));
    click(cx, "reaction-option-celebrate");
    assert_eq!(reacts.borrow()[1], "root-1|celebrate|Some(\"\u{1F389}\")");
    assert!(!shown(cx, "reaction-picker"), "the picker closes");
}

#[gpui::test]
fn shows_a_failed_reaction_as_an_error_banner_whose_retry_reacts_again(cx: &mut TestAppContext) {
    let count = Rc::new(RefCell::new(0));
    let n = count.clone();
    let cx = host(cx, move |_, _| {
        let n = n.clone();
        Comments::new("c", comments())
            .current_user(viewer())
            .on_react(move |_, _, _, _, _| {
                *n.borrow_mut() += 1;
                if *n.borrow() == 1 { fail() } else { ok() }
            })
            .into_any_element()
    });
    click(cx, "comment-reaction-root-1-like");
    assert!(shown(cx, "c-action-error-root-1"));
    assert!(said(cx, Politeness::Assertive).contains("Your reaction couldn't be saved."));
    click(cx, "c-action-error-root-1-action");
    assert_eq!(*count.borrow(), 2);
    assert!(!shown(cx, "c-action-error-root-1"));
}

#[gpui::test]
fn copies_a_link_to_the_comment(cx: &mut TestAppContext) {
    let cx = host(cx, |_, _| {
        Comments::new("c", no_replies())
            .current_user(other())
            .into_any_element()
    });
    hover(cx, "root-1");
    click(cx, "comment-action-link-root-1");
    let text = cx.update(|_, cx| cx.read_from_clipboard().and_then(|c| c.text()));
    assert!(
        text.as_deref()
            .is_some_and(|t| t.ends_with("#comment-root-1")),
        "{text:?}"
    );
}

#[gpui::test]
fn collapses_header_actions_into_the_more_menu_when_space_runs_low(cx: &mut TestAppContext) {
    let cx = host_at(cx, 260., |_, _| {
        Comments::new("c", no_replies())
            .current_user(other())
            .on_submit(|_, _, _, _| ok())
            .on_react(|_, _, _, _, _| ok())
            .into_any_element()
    });
    hover(cx, "root-1");
    assert!(!shown(cx, "comment-action-reply-root-1"), "Reply collapsed");
    click(cx, "comment-more-root-1");
    assert!(shown(cx, "c-more-root-1-list-reply"));
    assert!(shown(cx, "c-more-root-1-list-copy-link"));
}

#[gpui::test]
fn loads_and_shows_the_edit_history(cx: &mut TestAppContext) {
    let loads: Calls = Rc::default();
    let l = loads.clone();
    let cx = host(cx, move |_, _| {
        let l = l.clone();
        Comments::new("c", comments())
            .load_versions(move |id, _, _| {
                l.borrow_mut().push(id.to_string());
                Task::ready(Ok(vec![
                    CommentVersion::new("current", "Current wording", "2026-09-30T19:00:00Z")
                        .current(true),
                    CommentVersion::new("previous", "Previous wording", "2026-09-30T18:00:00Z"),
                ]))
            })
            .into_any_element()
    });
    click(cx, "c-history-root-1");
    assert_eq!(*loads.borrow(), ["root-1"]);
    assert!(shown(cx, "comment-version-previous"));
    assert!(shown(cx, "comment-version-current"));
}

#[gpui::test]
fn retries_a_comment_that_failed_to_send(cx: &mut TestAppContext) {
    let retries: Calls = Rc::default();
    let r = retries.clone();
    let cx = host(cx, move |_, _| {
        let r = r.clone();
        Comments::new(
            "c",
            [
                CommentThreadData::new("failed", viewer(), "Not sent", "2026-09-30T19:00:00Z")
                    .status(CommentStatus::Error)
                    .error_message("Couldn't post this comment."),
            ],
        )
        .current_user(viewer())
        .on_retry_comment(move |id, _, _| {
            r.borrow_mut().push(id.to_string());
            ok()
        })
        .into_any_element()
    });
    assert!(shown(cx, "c-delivery-error-failed"));
    click(cx, "c-delivery-error-failed-action");
    assert_eq!(*retries.borrow(), ["failed"]);
}

#[gpui::test]
fn loads_earlier_comments_and_offers_retry_when_that_fails(cx: &mut TestAppContext) {
    let loads = Rc::new(RefCell::new(0));
    let l = loads.clone();
    let cx = host(cx, move |_, _| {
        let l = l.clone();
        Comments::new("c", comments())
            .has_older_comments(true)
            .on_load_older(move |_, _| {
                *l.borrow_mut() += 1;
                fail()
            })
            .into_any_element()
    });
    click(cx, "c-load-older");
    assert_eq!(*loads.borrow(), 1);
    assert!(shown(cx, "c-older-error-1"));
    assert!(!shown(cx, "c-load-older"));
    click(cx, "c-older-error-1-action");
    assert_eq!(*loads.borrow(), 2);
    // A new Banner per failure, so a failed retry announces again.
    assert!(shown(cx, "c-older-error-2"));
}

#[gpui::test]
fn keeps_focus_on_the_earlier_comments_control_as_it_turns_into_retry_and_back(
    cx: &mut TestAppContext,
) {
    let fails = Rc::new(RefCell::new(true));
    let loads = Rc::new(RefCell::new(0));
    let (f, l) = (fails.clone(), loads.clone());
    let cx = host(cx, move |_, _| {
        let (f, l) = (f.clone(), l.clone());
        Comments::new("c", comments())
            .has_older_comments(true)
            .on_load_older(move |_, _| {
                *l.borrow_mut() += 1;
                if *f.borrow() { fail() } else { ok() }
            })
            .into_any_element()
    });
    press(cx, "c-load-older", "enter");
    assert!(shown(cx, "c-older-error-1"));
    // Focus moved to Retry: Enter runs it.
    *fails.borrow_mut() = false;
    key(cx, "enter");
    assert_eq!(*loads.borrow(), 2, "focus moved to Retry");
    assert!(shown(cx, "c-load-older"));
    // And back to the button.
    key(cx, "enter");
    assert_eq!(*loads.borrow(), 3, "focus moved back to the button");
}

#[gpui::test]
fn resolves_an_open_thread_and_reopens_a_resolved_one(cx: &mut TestAppContext) {
    let calls: Calls = Rc::default();
    let threads = Rc::new(RefCell::new(no_replies()));
    let (c, t) = (calls.clone(), threads.clone());
    let cx = host(cx, move |_, _| {
        let (c1, c2) = (c.clone(), c.clone());
        Comments::new("c", t.borrow().clone())
            .current_user(viewer())
            .on_resolve_thread(move |id, _, _| {
                c1.borrow_mut().push(format!("resolve:{id}"));
                ok()
            })
            .on_reopen_thread(move |id, _, _| {
                c2.borrow_mut().push(format!("reopen:{id}"));
                ok()
            })
            .into_any_element()
    });
    hover(cx, "root-1");
    click(cx, "comment-action-resolve-root-1");
    assert_eq!(*calls.borrow(), ["resolve:root-1"]);
    threads.borrow_mut()[0].is_resolved = true;
    hover(cx, "root-1");
    assert!(!shown(cx, "comment-action-resolve-root-1"));
    click(cx, "comment-action-reopen-root-1");
    assert_eq!(*calls.borrow(), ["resolve:root-1", "reopen:root-1"]);
}

#[gpui::test]
fn lets_the_host_own_selection_for_highlight_sync(cx: &mut TestAppContext) {
    let selected: Rc<RefCell<Option<SharedString>>> = Rc::default();
    let s = selected.clone();
    let cx = host(cx, move |_, _| {
        let s2 = s.clone();
        Comments::new("c", comments())
            .current_user(viewer())
            .selected_thread_id(s.borrow().clone())
            .on_selected_thread_change(move |id, _, _| *s2.borrow_mut() = id)
            .on_submit(|_, _, _, _| ok())
            .into_any_element()
    });
    *selected.borrow_mut() = Some("root-1".into());
    settle(cx);
    assert!(shown(cx, "comment-body-expanded-root-1"));
    assert!(shown(cx, "comment-reply-1"));
    // Host selection opens no composer and moves no focus.
    assert!(!shown(cx, "reply-composer-root-1"));
    assert!(cx.update(|window, cx| window.focused(cx).is_none()));
    click(cx, "comment-body-expanded-root-1");
    assert_eq!(*selected.borrow(), None);
}

#[gpui::test]
fn quotes_the_composer_anchor_and_lets_the_viewer_remove_it(cx: &mut TestAppContext) {
    let dismissed = Rc::new(RefCell::new(0));
    let d = dismissed.clone();
    let cx = host(cx, move |_, _| {
        let d = d.clone();
        Comments::new("c", comments())
            .current_user(viewer())
            .composer_anchor(CommentAnchor::new("The rollout completes on Friday."))
            .on_composer_anchor_dismiss(move |_, _| *d.borrow_mut() += 1)
            .on_submit(|_, _, _, _| ok())
            .into_any_element()
    });
    assert!(shown(cx, "composer-anchor-quote"));
    click(cx, "c-composer-remove-quote");
    assert_eq!(*dismissed.borrow(), 1);
}

#[gpui::test]
fn opens_a_thread_from_the_keyboard_leaving_focus_on_it(cx: &mut TestAppContext) {
    let composers: Composers = Rc::default();
    let r = composers.clone();
    let cx = host(cx, move |_, _| {
        Comments::new("c", comments())
            .current_user(viewer())
            .on_submit(|_, _, _, _| ok())
            .render_composer(test_composer(&r))
            .into_any_element()
    });
    press(cx, "c-toggle-root-1", "enter");
    assert!(shown(cx, "comment-body-expanded-root-1"));
    assert!(shown(cx, "reply-composer-root-1"), "the composer opens");
    // A key press leaves focus on the thread: Space closes it again.
    key(cx, "space");
    assert!(
        shown(cx, "comment-body-clamped-root-1"),
        "focus stays on the thread"
    );
}

#[gpui::test]
fn announces_a_load_failure_once_assertively(cx: &mut TestAppContext) {
    let cx = host(cx, |_, _| {
        Comments::new("c", comments())
            .load_error("Check your connection and try again.")
            .on_retry_load(|_, _| ok())
            .into_any_element()
    });
    assert!(shown(cx, "comments-load-error"));
    let message = said(cx, Politeness::Assertive);
    assert!(
        message.contains("Check your connection and try again."),
        "{message}"
    );
}

// CommentThread.

fn thread_fixture() -> CommentThreadData {
    CommentThreadData::new(
        "t1",
        other(),
        "Should this say three days?",
        "2026-09-30T19:00:00Z",
    )
    .anchor(CommentAnchor::new(
        "The final cohort completes rollout in two days.",
    ))
    .replies([CommentData::new(
        "t1-reply",
        viewer(),
        "Good catch.",
        "2026-09-30T19:20:00Z",
    )])
}

#[gpui::test]
fn renders_one_thread_with_its_quoted_passage_and_collapsed_replies(cx: &mut TestAppContext) {
    let cx = host(cx, |_, _| {
        CommentThread::new("ct", thread_fixture())
            .current_user(viewer())
            .container(CommentContainer::Card)
            .into_any_element()
    });
    assert!(shown(cx, "thread-t1"));
    assert!(shown(cx, "comment-anchor-anchored"));
    assert!(shown(cx, "reply-disclosure-t1"));
    assert!(!shown(cx, "comment-t1-reply"));
}

#[gpui::test]
fn a_thread_opens_on_a_press_showing_its_replies_and_reply_composer(cx: &mut TestAppContext) {
    let changes: Calls = Rc::default();
    let c = changes.clone();
    let cx = host(cx, move |_, _| {
        let c = c.clone();
        CommentThread::new("ct", thread_fixture())
            .current_user(viewer())
            .on_selected_change(move |selected, _, _| c.borrow_mut().push(selected.to_string()))
            .on_submit(|_, _, _, _| ok())
            .into_any_element()
    });
    click(cx, "comment-body-clamped-t1");
    assert!(shown(cx, "comment-t1-reply"));
    assert!(shown(cx, "reply-composer-t1"));
    assert!(changes.borrow().iter().all(|c| c == "true") && !changes.borrow().is_empty());
}

#[gpui::test]
fn a_thread_opens_when_the_host_selects_it(cx: &mut TestAppContext) {
    let selected = Rc::new(RefCell::new(false));
    let s = selected.clone();
    let cx = host(cx, move |_, _| {
        CommentThread::new("ct", thread_fixture())
            .current_user(viewer())
            .selected(*s.borrow())
            .on_submit(|_, _, _, _| ok())
            .into_any_element()
    });
    assert!(!shown(cx, "comment-t1-reply"));
    *selected.borrow_mut() = true;
    assert!(shown(cx, "comment-t1-reply"));
    assert!(
        !shown(cx, "reply-composer-t1"),
        "host selection opens no composer"
    );
}

#[gpui::test]
fn forwards_the_reply_draft_lifecycle_through_a_standalone_thread(cx: &mut TestAppContext) {
    let calls: Calls = Rc::default();
    let composers: Composers = Rc::default();
    let (c, r) = (calls.clone(), composers.clone());
    let cx = host(cx, move |_, _| {
        CommentThread::new("ct", thread_fixture())
            .current_user(viewer())
            .on_submit(|_, _, _, _| ok())
            .on_composer_draft_change(draft_log(&c))
            .render_composer(test_composer(&r))
            .into_any_element()
    });
    click(cx, "comment-body-clamped-t1");
    click(cx, "set-draft-reply");
    assert_eq!(last(&calls), "Reply body|reply:t1:t1");
    click(cx, "submit-reply");
    assert_eq!(last(&calls), "|reply:t1:t1");
}

/// A thread whose host light dismissal it owns, logging each dismissal.
fn dismissable<'a>(
    cx: &'a mut TestAppContext,
    dismissals: &Rc<RefCell<usize>>,
    composers: &Composers,
) -> &'a mut VisualTestContext {
    let (d, r) = (dismissals.clone(), composers.clone());
    host(cx, move |_, _| {
        let d = d.clone();
        CommentThread::new("ct", thread_fixture())
            .current_user(viewer())
            .on_dismiss(move |_, _| *d.borrow_mut() += 1)
            .on_submit(|_, _, _, _| ok())
            .render_composer(test_composer(&r))
            .into_any_element()
    })
}

#[gpui::test]
fn dismisses_on_an_outside_press_when_nothing_is_unsaved(cx: &mut TestAppContext) {
    let dismissals = Rc::new(RefCell::new(0));
    let composers: Composers = Rc::default();
    let cx = dismissable(cx, &dismissals, &composers);
    click(cx, "outside");
    assert_eq!(*dismissals.borrow(), 1);
}

#[gpui::test]
fn shakes_refocuses_and_announces_on_the_first_outside_press_over_a_draft_then_dismisses(
    cx: &mut TestAppContext,
) {
    let dismissals = Rc::new(RefCell::new(0));
    let composers: Composers = Rc::default();
    let cx = dismissable(cx, &dismissals, &composers);
    click(cx, "comment-body-clamped-t1");
    click(cx, "set-draft-reply");
    click(cx, "outside");
    assert_eq!(*dismissals.borrow(), 0, "the first press is guarded");
    assert!(
        said(cx, Politeness::Polite).contains("Unsaved draft"),
        "{}",
        said(cx, Politeness::Polite)
    );
    assert_eq!(
        composers.borrow()["reply"].value,
        "Reply body",
        "the draft is kept"
    );
    click(cx, "outside");
    assert_eq!(*dismissals.borrow(), 1, "the next press dismisses");
}

#[gpui::test]
fn re_arms_after_the_draft_changes_or_the_viewer_presses_inside(cx: &mut TestAppContext) {
    let dismissals = Rc::new(RefCell::new(0));
    let composers: Composers = Rc::default();
    let cx = dismissable(cx, &dismissals, &composers);
    click(cx, "comment-body-clamped-t1");
    click(cx, "set-draft-reply");
    click(cx, "outside");
    // A changed draft is guarded again.
    click(cx, "set-draft-reply");
    click(cx, "outside");
    assert_eq!(*dismissals.borrow(), 0, "a changed draft re-arms the guard");
    // So is one after a press inside.
    click(cx, "reply-composer-t1");
    click(cx, "outside");
    assert_eq!(*dismissals.borrow(), 0, "a press inside re-arms the guard");
    click(cx, "outside");
    assert_eq!(*dismissals.borrow(), 1);
}

#[gpui::test]
fn guards_a_draft_on_escape_and_dismisses_on_the_next_one(cx: &mut TestAppContext) {
    let dismissals = Rc::new(RefCell::new(0));
    let composers: Composers = Rc::default();
    let cx = dismissable(cx, &dismissals, &composers);
    cx.simulate_keystrokes("escape");
    settle(cx);
    assert_eq!(
        *dismissals.borrow(),
        1,
        "an Escape with nothing unsaved dismisses"
    );
    click(cx, "comment-body-clamped-t1");
    click(cx, "set-draft-reply");
    cx.simulate_keystrokes("escape");
    settle(cx);
    assert_eq!(*dismissals.borrow(), 1, "guarded");
    cx.simulate_keystrokes("escape");
    settle(cx);
    assert_eq!(*dismissals.borrow(), 2);
}

#[gpui::test]
fn does_not_move_under_reduced_motion_but_still_announces(cx: &mut TestAppContext) {
    let dismissals = Rc::new(RefCell::new(0));
    let composers: Composers = Rc::default();
    let cx = dismissable(cx, &dismissals, &composers);
    cx.update(|_, cx| rustryx_primitives::set_reduced_motion(true, cx));
    click(cx, "comment-body-clamped-t1");
    click(cx, "set-draft-reply");
    let before = bounds(cx, "ct").unwrap();
    let b = bounds(cx, "outside").unwrap();
    cx.simulate_click(b.center(), Modifiers::none());
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_millis(100));
    cx.run_until_parked();
    assert_eq!(bounds(cx, "ct").unwrap(), before, "no shake");
    assert!(said(cx, Politeness::Polite).contains("Unsaved draft"));
    assert_eq!(*dismissals.borrow(), 0);
}

/// A thread with its own composer whose host light dismissal it owns.
fn dismissable_with_composer<'a>(
    cx: &'a mut TestAppContext,
    dismissals: &Rc<RefCell<usize>>,
) -> &'a mut VisualTestContext {
    let d = dismissals.clone();
    host(cx, move |_, _| {
        let d = d.clone();
        CommentThread::new("ct", thread_fixture())
            .current_user(viewer())
            .on_dismiss(move |_, _| *d.borrow_mut() += 1)
            .on_submit(|_, _, _, _| ok())
            .into_any_element()
    })
}

#[gpui::test]
fn leaves_an_outside_press_to_an_open_emoji_picker_above_the_thread(cx: &mut TestAppContext) {
    let dismissals = Rc::new(RefCell::new(0));
    let cx = dismissable_with_composer(cx, &dismissals);
    click(cx, "comment-body-clamped-t1");
    click(cx, "ct-reply-composer-t1-input-editor");
    cx.simulate_input("Draft");
    settle(cx);
    click(cx, "ct-reply-composer-t1-emoji-button");
    assert!(
        shown(cx, "ct-reply-composer-t1-emoji-picker-cell-0"),
        "the picker is open"
    );
    click(cx, "outside");
    assert_eq!(*dismissals.borrow(), 0);
    assert!(
        !said(cx, Politeness::Polite).contains("Unsaved draft"),
        "the picker took the press"
    );
}

#[gpui::test]
fn closes_an_open_emoji_picker_on_escape_before_the_thread(cx: &mut TestAppContext) {
    let dismissals = Rc::new(RefCell::new(0));
    let cx = dismissable_with_composer(cx, &dismissals);
    click(cx, "comment-body-clamped-t1");
    click(cx, "ct-reply-composer-t1-emoji-button");
    assert!(shown(cx, "ct-reply-composer-t1-emoji-picker-cell-0"));
    cx.simulate_keystrokes("escape");
    settle(cx);
    assert!(
        !shown(cx, "ct-reply-composer-t1-emoji-picker-cell-0"),
        "the picker closed"
    );
    assert_eq!(*dismissals.borrow(), 0, "the thread stays");
    // Focus came back to Add emoji from the keyboard, which opens its tooltip at
    // once; that layer, above the thread, takes the next Escape.
    cx.simulate_keystrokes("escape");
    settle(cx);
    assert_eq!(*dismissals.borrow(), 0, "the tooltip closes first");
    cx.simulate_keystrokes("escape");
    settle(cx);
    assert_eq!(*dismissals.borrow(), 1);
}

/// A thread in a Popover that hands it the Popover's light dismissal (upstream's
/// `CommentThreadInPopover` and the trigger test's `Host`).
fn popover_host<'a>(
    cx: &'a mut TestAppContext,
    padding: rustryx_components::layout::SpacingStep,
    composers: &Composers,
) -> (&'a mut VisualTestContext, Rc<RefCell<bool>>) {
    let open = Rc::new(RefCell::new(false));
    let (o, r) = (open.clone(), composers.clone());
    let cx = host(cx, move |_, _| {
        let (set, dismiss) = (o.clone(), o.clone());
        div()
            .flex()
            .items_start()
            .p(px(40.))
            .child(
                div()
                    .flex_none()
                    .debug_selector(|| "open-comment".into())
                    .child(
                        rustryx_components::overlay::Popover::new("host-popover", "Comment")
                            .open(*o.borrow())
                            .on_open_change(move |next, _, _| *set.borrow_mut() = next)
                            .light_dismiss(false)
                            .escape_dismiss(false)
                            .auto_focus(false)
                            .padding(padding)
                            .width(px(340.))
                            .content(
                                CommentThread::new("ct", thread_fixture())
                                    .current_user(viewer())
                                    .selected(true)
                                    .container(CommentContainer::Card)
                                    .on_submit(|_, _, _, _| ok())
                                    .on_dismiss(move |_, _| *dismiss.borrow_mut() = false)
                                    .render_composer(test_composer(&r)),
                            )
                            .child(rustryx_components::button::Button::new(
                                "open-comment",
                                "Open comment",
                            )),
                    ),
            )
            .into_any_element()
    });
    (cx, open)
}

#[gpui::test]
fn keeps_a_popover_host_open_on_its_trigger_over_a_draft_and_closes_without_reopening_otherwise(
    cx: &mut TestAppContext,
) {
    let composers: Composers = Rc::default();
    let (cx, open) = popover_host(cx, rustryx_components::layout::SpacingStep::S0, &composers);
    click(cx, "open-comment");
    assert!(*open.borrow(), "the trigger opens the host");
    assert!(shown(cx, "thread-t1"));
    hover(cx, "t1");
    click(cx, "comment-action-reply-t1");
    click(cx, "set-draft-reply");
    click(cx, "open-comment");
    assert!(*open.borrow(), "the guarded press keeps the host open");
    assert!(said(cx, Politeness::Polite).contains("Unsaved draft"));
    click(cx, "open-comment");
    // Closed, and the same press's click did not toggle it back open.
    assert!(!*open.borrow());
    assert!(!shown(cx, "thread-t1"));
}

#[gpui::test]
fn a_press_on_the_hosts_padding_is_inside_the_thread(cx: &mut TestAppContext) {
    let composers: Composers = Rc::default();
    let (cx, open) = popover_host(cx, rustryx_components::layout::SpacingStep::S4, &composers);
    click(cx, "open-comment");
    let thread = bounds(cx, "ct").expect("the thread is drawn");
    // Just above the thread, on the Popover's padding.
    cx.simulate_click(
        thread.origin + gpui::point(px(20.), px(-6.)),
        Modifiers::none(),
    );
    settle(cx);
    assert!(*open.borrow(), "the host's own surface is inside");
    click(cx, "outside");
    assert!(!*open.borrow(), "outside the host dismisses");
}

#[gpui::test]
fn a_thread_inside_a_scroller_draws_in_flow_and_is_clipped(cx: &mut TestAppContext) {
    let dismissals = Rc::new(RefCell::new(0));
    let d = dismissals.clone();
    let cx = host(cx, move |_, _| {
        let d = d.clone();
        div()
            .id("scroller")
            .debug_selector(|| "scroller".into())
            .h(px(120.))
            .overflow_hidden()
            .child(
                CommentThread::new("ct", thread_fixture())
                    .current_user(viewer())
                    .on_dismiss(move |_, _| *d.borrow_mut() += 1)
                    .on_submit(|_, _, _, _| ok()),
            )
            .into_any_element()
    });
    let scroller = bounds(cx, "scroller").unwrap();
    let thread = bounds(cx, "ct").unwrap();
    assert!(
        thread.size.height > scroller.size.height,
        "the thread runs past the scroller's clip"
    );
    // Below the clip, over where the thread would be, a press is outside.
    let below = gpui::point(thread.origin.x + px(20.), scroller.bottom() + px(4.));
    assert!(thread.contains(&below));
    cx.simulate_click(below, Modifiers::none());
    settle(cx);
    assert_eq!(
        *dismissals.borrow(),
        1,
        "the clipped part is not the thread"
    );
}

#[gpui::test]
fn a_thread_without_on_dismiss_dismisses_nothing(cx: &mut TestAppContext) {
    let cx = host(cx, |_, _| {
        CommentThread::new("ct", thread_fixture())
            .current_user(viewer())
            .on_submit(|_, _, _, _| ok())
            .into_any_element()
    });
    click(cx, "comment-body-clamped-t1");
    click(cx, "outside");
    // Still drawn, and still open.
    assert!(shown(cx, "comment-t1-reply"));
}

// CommentComposer.

fn composer_host<'a>(
    cx: &'a mut TestAppContext,
    posts: &Calls,
    tweak: impl Fn(CommentComposer) -> CommentComposer + 'static,
) -> (&'a mut VisualTestContext, Entity<SharedString>) {
    cx.update(|cx| {
        rustryx_theme::init(cx);
        rustryx_primitives::focus::init(cx);
        rustryx_primitives::editing::init(cx);
    });
    let draft = cx.update(|cx| cx.new(|_| SharedString::default()));
    let (d, p) = (draft.clone(), posts.clone());
    let cx = host(cx, move |_, cx| {
        let (change, p) = (d.clone(), p.clone());
        tweak(CommentComposer::new(
            "cc",
            d.read(cx).clone(),
            move |next, _, cx| {
                change.update(cx, |v, cx| {
                    *v = next;
                    cx.notify();
                })
            },
            move |body, _, _| p.borrow_mut().push(body.to_string()),
        ))
        .into_any_element()
    });
    (cx, draft)
}

#[gpui::test]
fn posts_the_draft_from_post_and_never_an_empty_one(cx: &mut TestAppContext) {
    let posts: Calls = Rc::default();
    let (cx, draft) = composer_host(cx, &posts, |c| c);
    click(cx, "cc-post");
    assert!(posts.borrow().is_empty(), "an empty draft never posts");
    click(cx, "cc-input-editor");
    cx.simulate_input("Ship it");
    settle(cx);
    assert_eq!(draft.read_with(cx, |d, _| d.to_string()), "Ship it");
    click(cx, "cc-post");
    assert_eq!(*posts.borrow(), ["Ship it"]);
}

#[gpui::test]
fn appends_a_picked_emoji_to_the_draft(cx: &mut TestAppContext) {
    let posts: Calls = Rc::default();
    let (cx, draft) = composer_host(cx, &posts, |c| c);
    click(cx, "cc-input-editor");
    cx.simulate_input("Nice");
    settle(cx);
    click(cx, "cc-emoji-button");
    click(cx, "cc-emoji-picker-cell-0");
    let value = draft.read_with(cx, |d, _| d.to_string());
    assert!(
        value.starts_with("Nice ") && value.chars().count() > 5,
        "{value:?}"
    );
    assert!(!shown(cx, "cc-emoji-picker-cell-0"), "the picker closes");
}

#[gpui::test]
fn shows_an_error_with_retry_and_a_quoted_passage_the_viewer_can_remove(cx: &mut TestAppContext) {
    let posts: Calls = Rc::default();
    let retries = Rc::new(RefCell::new(0));
    let removed = Rc::new(RefCell::new(0));
    let (r, m) = (retries.clone(), removed.clone());
    let (cx, _) = composer_host(cx, &posts, move |c| {
        let (r, m) = (r.clone(), m.clone());
        c.error("Your comment couldn't be posted.")
            .on_retry(move |_, _| *r.borrow_mut() += 1)
            .anchor(CommentAnchor::new(
                "Guardrail alerts page the release oncall.",
            ))
            .on_anchor_dismiss(move |_, _| *m.borrow_mut() += 1)
    });
    assert!(shown(cx, "cc-composer-error-error"));
    click(cx, "cc-composer-error-error-action");
    assert_eq!(*retries.borrow(), 1);
    assert!(shown(cx, "composer-anchor-quote"));
    click(cx, "cc-remove-quote");
    assert_eq!(*removed.borrow(), 1);
}

#[gpui::test]
fn leaves_the_emoji_button_out_when_off_and_adds_controls(cx: &mut TestAppContext) {
    let posts: Calls = Rc::default();
    let (cx, _) = composer_host(cx, &posts, |c| {
        c.emoji_picker(false).controls(
            div()
                .debug_selector(|| "product-control".into())
                .w(px(20.))
                .h(px(20.)),
        )
    });
    assert!(!shown(cx, "cc-emoji-button"));
    assert!(shown(cx, "product-control"));
    assert!(
        !shown(cx, "cc-attach"),
        "no attach button without an uploader"
    );
}

#[gpui::test]
fn refuses_files_that_are_not_images(cx: &mut TestAppContext) {
    let posts: Calls = Rc::default();
    let uploads = Rc::new(RefCell::new(0));
    let u = uploads.clone();
    let (cx, _) = composer_host(cx, &posts, move |c| {
        let u = u.clone();
        c.on_upload_images(move |_, _, _| {
            *u.borrow_mut() += 1;
            Task::ready(Ok(Vec::new()))
        })
    });
    assert!(shown(cx, "cc-attach"));
    click(cx, "cc-input-editor");
    let item = gpui::ClipboardItem {
        entries: vec![gpui::ClipboardEntry::ExternalPaths(gpui::ExternalPaths(
            vec![std::path::PathBuf::from("notes.txt")].into(),
        ))],
    };
    cx.update(|_, cx| cx.write_to_clipboard(item));
    cx.simulate_keystrokes("secondary-v");
    settle(cx);
    assert_eq!(*uploads.borrow(), 0);
    assert!(shown(cx, "cc-composer-error-action-1"));
    assert!(
        said(cx, Politeness::Assertive).contains("image"),
        "{}",
        said(cx, Politeness::Assertive)
    );
}

#[gpui::test]
fn uploads_pasted_images_and_appends_them_as_markdown(cx: &mut TestAppContext) {
    let posts: Calls = Rc::default();
    let (cx, draft) = composer_host(cx, &posts, |c| {
        c.on_upload_images(|files, _, _| {
            Task::ready(Ok(files
                .into_iter()
                .map(|f| CommentUploadedImage {
                    url: format!("https://cdn.example/{}", f.name).into(),
                    alt: "Chart".into(),
                })
                .collect()))
        })
    });
    click(cx, "cc-input-editor");
    cx.simulate_input("See this");
    settle(cx);
    let item = gpui::ClipboardItem {
        entries: vec![gpui::ClipboardEntry::ExternalPaths(gpui::ExternalPaths(
            vec![std::path::PathBuf::from("chart.png")].into(),
        ))],
    };
    cx.update(|_, cx| cx.write_to_clipboard(item));
    cx.simulate_keystrokes("secondary-v");
    settle(cx);
    assert_eq!(
        draft.read_with(cx, |d, _| d.to_string()),
        "See this\n\n![Chart](https://cdn.example/chart.png)"
    );
}
