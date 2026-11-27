//! The comments on one item (upstream lab `Comments/Comments.tsx`): its comment
//! threads and activity events in time order, with a composer for new comments. A
//! pane that fills its container and scrolls, or an embedded block at its content's
//! height.

use std::rc::Rc;

use astryx_components::button::{Button, ButtonVariant};
use astryx_components::chat::{ChatDensity, ChatLayout, ChatListAlign, ChatMessageList};
use astryx_components::containers::EmptyState;
use astryx_components::feedback::{Banner, BannerContainer, BannerStatus};
use astryx_components::layout::SpacingStep;
use astryx_i18n::Arg;
use astryx_primitives::{ElementSize, Politeness, request_focus};
use astryx_theme::authoring::js::js_trim;
use gpui::prelude::*;
use gpui::{AnyElement, App, ElementId, SharedString, StyleRefinement, Window, div};

use super::RenderEventFn;
use super::composer::ComposerBase;
use super::context::{
    ControllerOptions, SelectionFn, SubmitResult, provide_scope, scoped, use_controller,
};
use super::parts::{
    ErrorBannerContent, activity_event_row, announce_status, composer_loading_skeleton,
    error_banner, thread_loading_skeleton,
};
use super::row::{CommentRow, THREAD_DIVIDERS, ThreadDivider, configure_composer};
use super::shared::CommentThreadShared;
use super::types::{
    ActionFn, CommentActivityEvent, CommentAnchor, CommentComposerContext, CommentContainer,
    CommentMode, CommentTask, CommentThreadData, CommentThreadSharedProps,
};
use super::utils::timeline_time;

/// Upstream `composerPosition`: where the composer for new comments renders.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CommentsComposerPosition {
    Top,
    /// The default.
    #[default]
    Bottom,
    None,
}

/// Upstream `density`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CommentsDensity {
    Compact,
    /// The default.
    #[default]
    Balanced,
}

impl CommentsDensity {
    fn as_str(self) -> &'static str {
        match self {
            Self::Compact => "compact",
            Self::Balanced => "balanced",
        }
    }
}

/// Upstream `layout`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CommentsLayout {
    /// Fills its container and scrolls, with the composer docked at the bottom: a
    /// comments panel (the default).
    #[default]
    Pane,
    /// Takes its content's height inside a host surface such as a Card or Popover: no
    /// scroll area or dock, and on a card parent no rule above the first thread or
    /// below the last.
    Embedded,
}

/// A host task with no outcome but its success (`onLoadOlder`, `onRetryLoad`).
pub type CommentsActionFn = super::types::TaskFn;

/// Upstream `TimelineEntry`.
#[derive(Clone)]
enum TimelineEntry {
    Comment(Box<CommentThreadData>),
    Event(Box<CommentActivityEvent>),
}

impl TimelineEntry {
    fn time(&self) -> f64 {
        match self {
            Self::Comment(c) => timeline_time(&c.created_at),
            Self::Event(e) => timeline_time(&e.created_at),
        }
    }
}

/// Upstream's timeline order: threads and events by time, oldest first unless the
/// threads themselves run newest first. A stable sort, as `Array.prototype.sort`.
fn timeline(comments: &[CommentThreadData], events: &[CommentActivityEvent]) -> Vec<TimelineEntry> {
    let direction = if comments.len() > 1
        && timeline_time(&comments[0].created_at)
            > timeline_time(&comments[comments.len() - 1].created_at)
    {
        -1.
    } else {
        1.
    };
    let mut entries: Vec<TimelineEntry> = comments
        .iter()
        .cloned()
        .map(|c| TimelineEntry::Comment(Box::new(c)))
        .chain(
            events
                .iter()
                .cloned()
                .map(|e| TimelineEntry::Event(Box::new(e))),
        )
        .collect();
    entries.sort_by(|a, b| {
        // A comparator that returns NaN keeps the pair's order, as V8's does.
        let d = direction * (a.time() - b.time());
        d.partial_cmp(&0.).unwrap_or(std::cmp::Ordering::Equal)
    });
    entries
}

/// Where focus goes once a load of older comments settles.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OlderFocus {
    Load,
    Retry,
}

/// What the panel keeps across frames.
#[derive(Default)]
struct PanelState {
    draft: SharedString,
    top_composer_error: Option<SharedString>,
    loading_older: bool,
    older_error: Option<SharedString>,
    older_failure_count: u64,
    older_focus: Option<OlderFocus>,
    /// The anchor quote the panel mounted with: restored state, shown without moving
    /// focus. `None` before the first frame.
    mounted_anchor_quote: Option<Option<SharedString>>,
}

/// Upstream lab `Comments`: the comments on one item: comment threads with one level
/// of replies, reactions, edit history, inline-comment quotes and activity events,
/// with a composer for new comments. Controlled: the host owns the data and every
/// mutation; the panel owns presentation, selection and focus.
///
/// ```ignore
/// Comments::new("comments", threads)
///     .events(events)
///     .current_user(viewer)
///     .on_submit(|body, parent, _, cx| post(body, parent, cx))
/// ```
pub struct Comments {
    id: ElementId,
    comments: Vec<CommentThreadData>,
    events: Vec<CommentActivityEvent>,
    label: Option<SharedString>,
    composer_position: CommentsComposerPosition,
    density: CommentsDensity,
    layout: CommentsLayout,
    selected_thread_id: Option<Option<SharedString>>,
    on_selected_thread_change: Option<SelectionFn>,
    composer_anchor: Option<CommentAnchor>,
    on_composer_anchor_dismiss: Option<ActionFn>,
    composer_controls: Option<AnyElement>,
    is_loading: bool,
    load_error: Option<SharedString>,
    on_retry_load: Option<CommentsActionFn>,
    has_older_comments: bool,
    is_loading_older: bool,
    on_load_older: Option<CommentsActionFn>,
    composer_error: Option<SharedString>,
    empty_state: Option<AnyElement>,
    render_event: Option<RenderEventFn>,
    shared: CommentThreadSharedProps,
    style: StyleRefinement,
}

impl Comments {
    /// Upstream `comments`: the threads, controlled: each thread's first comment,
    /// carrying its replies. Shown in time order with `events`, oldest first unless the
    /// list itself runs newest first.
    pub fn new(
        id: impl Into<ElementId>,
        comments: impl IntoIterator<Item = impl Into<CommentThreadData>>,
    ) -> Self {
        Comments {
            id: id.into(),
            comments: comments.into_iter().map(Into::into).collect(),
            events: Vec::new(),
            label: None,
            composer_position: CommentsComposerPosition::Bottom,
            density: CommentsDensity::Balanced,
            layout: CommentsLayout::Pane,
            selected_thread_id: None,
            on_selected_thread_change: None,
            composer_anchor: None,
            on_composer_anchor_dismiss: None,
            composer_controls: None,
            is_loading: false,
            load_error: None,
            on_retry_load: None,
            has_older_comments: false,
            is_loading_older: false,
            on_load_older: None,
            composer_error: None,
            empty_state: None,
            render_event: None,
            shared: CommentThreadSharedProps::default(),
            style: StyleRefinement::default(),
        }
    }

    /// Upstream `events`: activity on the item, interleaved with the threads by time.
    pub fn events(mut self, events: impl IntoIterator<Item = CommentActivityEvent>) -> Self {
        self.events = events.into_iter().collect();
        self
    }

    /// Upstream `label`: the region's accessible name (the localized "Comments").
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Upstream `composerPosition` (`Bottom` by default).
    pub fn composer_position(mut self, position: CommentsComposerPosition) -> Self {
        self.composer_position = position;
        self
    }

    /// Upstream `density` (`Balanced` by default).
    pub fn density(mut self, density: CommentsDensity) -> Self {
        self.density = density;
        self
    }

    /// Upstream `layout` (`Pane` by default).
    pub fn layout(mut self, layout: CommentsLayout) -> Self {
        self.layout = layout;
        self
    }

    /// Upstream `selectedThreadId`: the selected thread, for hosts that sync selection
    /// with a document (`None` selects no thread). Host selection opens the body and
    /// replies without moving focus or opening the reply composer. Leave it unset to
    /// let the panel own selection.
    pub fn selected_thread_id(mut self, id: Option<impl Into<SharedString>>) -> Self {
        self.selected_thread_id = Some(id.map(Into::into));
        self
    }

    /// Upstream `onSelectedThreadChange`: the viewer selected a thread, or none.
    pub fn on_selected_thread_change(
        mut self,
        f: impl Fn(Option<SharedString>, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_selected_thread_change = Some(Rc::new(f));
        self
    }

    /// Upstream `composerAnchor`: the passage a new comment will attach to, such as the
    /// viewer's text selection. The composer quotes it and takes focus; the host
    /// attaches it on submit and clears it afterwards.
    pub fn composer_anchor(mut self, anchor: CommentAnchor) -> Self {
        self.composer_anchor = Some(anchor);
        self
    }

    /// Upstream `onComposerAnchorDismiss`: adds Remove quoted text to the quote.
    pub fn on_composer_anchor_dismiss(
        mut self,
        f: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_composer_anchor_dismiss = Some(Rc::new(f));
        self
    }

    /// Upstream `composerControls`: product-owned controls in the new-comment
    /// composer's footer, after its attach and emoji buttons. Keep it to one short
    /// control.
    pub fn composer_controls(mut self, controls: impl IntoElement) -> Self {
        self.composer_controls = Some(controls.into_any_element());
        self
    }

    /// Upstream `isLoading`: comment-shaped placeholders instead of the threads.
    pub fn loading(mut self, loading: bool) -> Self {
        self.is_loading = loading;
        self
    }

    /// Upstream `loadError`: an error Banner; with `on_retry_load`, a Retry action.
    pub fn load_error(mut self, error: impl Into<SharedString>) -> Self {
        self.load_error = Some(error.into());
        self
    }

    /// Upstream `onRetryLoad`: the load error Banner's Retry, busy while it runs.
    pub fn on_retry_load(
        mut self,
        f: impl Fn(&mut Window, &mut App) -> CommentTask + 'static,
    ) -> Self {
        self.on_retry_load = Some(Rc::new(f));
        self
    }

    /// Upstream `hasOlderComments`: adds Load earlier comments above the threads.
    pub fn has_older_comments(mut self, has: bool) -> Self {
        self.has_older_comments = has;
        self
    }

    /// Upstream `isLoadingOlder`: Load earlier comments shows it is busy.
    pub fn loading_older(mut self, loading: bool) -> Self {
        self.is_loading_older = loading;
        self
    }

    /// Upstream `onLoadOlder`: loads the previous page; the host prepends it to
    /// `comments`. A task that fails shows a Banner with Retry.
    pub fn on_load_older(
        mut self,
        f: impl Fn(&mut Window, &mut App) -> CommentTask + 'static,
    ) -> Self {
        self.on_load_older = Some(Rc::new(f));
        self
    }

    /// Upstream `composerError`: a host-owned error for the new-comment composer.
    pub fn composer_error(mut self, error: impl Into<SharedString>) -> Self {
        self.composer_error = Some(error.into());
        self
    }

    /// Upstream `emptyState`: replaces the default no-comments state.
    pub fn empty_state(mut self, empty: impl IntoElement) -> Self {
        self.empty_state = Some(empty.into_any_element());
        self
    }

    /// Upstream `renderEvent`: replaces the default activity event row.
    pub fn render_event(
        mut self,
        f: impl Fn(&CommentActivityEvent, &mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        self.render_event = Some(Rc::new(f));
        self
    }
}

impl CommentThreadShared for Comments {
    fn shared_props(&mut self) -> &mut CommentThreadSharedProps {
        &mut self.shared
    }
}

impl Styled for Comments {
    /// The root's style (upstream `xstyle`, `className`, `style`).
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl IntoElement for Comments {
    type Element = AnyElement;

    fn into_element(self) -> AnyElement {
        CommentsElement(std::cell::RefCell::new(Some(self))).into_any_element()
    }
}

#[derive(IntoElement)]
struct CommentsElement(std::cell::RefCell<Option<Comments>>);

impl RenderOnce for CommentsElement {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let comments = self.0.borrow_mut().take().expect("rendered once");
        render_comments(comments, window, cx)
    }
}

/// Upstream `loadOlder`: one load at a time; focus follows whichever of the button
/// and the failure Banner's Retry shows, so it never drops.
fn load_older(
    region: &gpui::FocusHandle,
    state: &gpui::Entity<PanelState>,
    on_load_older: Option<CommentsActionFn>,
    window: &mut Window,
    cx: &mut App,
) {
    let Some(load) = on_load_older else { return };
    if state.read(cx).loading_older {
        return;
    }
    // The button and the failure Banner's Retry are the region's only controls.
    let had_focus = region.contains_focused(window, cx);
    state.update(cx, |s, cx| {
        s.loading_older = true;
        cx.notify();
    });
    let task = load(window, cx);
    let state = state.clone();
    window
        .spawn(cx, async move |cx| {
            let ok = task.await.is_ok();
            let _ = cx.update(|window, cx| {
                let failed = astryx_i18n::current(cx).t("@astryx.comments.olderFailed");
                state.update(cx, |s, cx| {
                    s.loading_older = false;
                    if ok {
                        s.older_error = None;
                    } else {
                        s.older_error = Some(failed);
                        // A new Banner per failure, so a failed retry announces again.
                        s.older_failure_count += 1;
                    }
                    if had_focus {
                        s.older_focus = Some(if ok {
                            OlderFocus::Load
                        } else {
                            OlderFocus::Retry
                        });
                    }
                    cx.notify();
                });
                window.refresh();
            });
        })
        .detach();
}

fn older_retry_id(sid: &str, failures: u64) -> ElementId {
    ElementId::Name(format!("{}-action", scoped(sid, format!("older-error-{failures}"))).into())
}

fn render_comments(props: Comments, window: &mut Window, cx: &mut App) -> AnyElement {
    let theme = astryx_theme::current(window, cx);
    let t = theme.tokens();
    let i18n = astryx_i18n::current(cx);
    let sid: SharedString = props.id.to_string().into();
    let label = props
        .label
        .clone()
        .unwrap_or_else(|| i18n.t("@astryx.comments.label"));
    let state = window.use_keyed_state(scoped(&sid, "panel"), cx, |_, _| PanelState::default());
    // The load-failure Banner carries no live role: it mounts already holding its
    // text. It is announced once, assertively, through the shared channel.
    announce_status(
        scoped(&sid, "load-error-announced"),
        props.load_error.as_ref().map(|error| {
            i18n.format(
                "@astryx.comments.errorAnnouncement",
                &[
                    (
                        "title",
                        Arg::from(i18n.t("@astryx.comments.loadFailed").as_ref()),
                    ),
                    ("description", Arg::from(error.as_ref())),
                ],
            )
        }),
        Politeness::Assertive,
        window,
        cx,
    );
    let entries = timeline(&props.comments, &props.events);
    let roots: Vec<CommentThreadData> = entries
        .iter()
        .filter_map(|e| match e {
            TimelineEntry::Comment(c) => Some((**c).clone()),
            TimelineEntry::Event(_) => None,
        })
        .collect();
    let top_context = CommentComposerContext::Comment {
        anchor: props.composer_anchor.clone(),
    };
    let shared = props.shared.clone();
    let mode = shared.mode;
    let container = shared.container;
    let has_emoji_picker = shared.has_emoji_picker.unwrap_or(true);
    let has_uploads = shared.on_upload_images.is_some();
    let can_submit = shared.on_submit.is_some();
    let on_draft_change = shared.on_composer_draft_change.clone();
    let scope = use_controller(
        ControllerOptions {
            id: sid.clone(),
            props: props.shared,
            selected_thread_id: props.selected_thread_id.clone(),
            on_selected_thread_change: props.on_selected_thread_change.clone(),
            roots,
        },
        window,
        cx,
    );

    // An anchor the panel mounts with is restored state, not a selection the viewer
    // just made, so it shows without moving focus; an anchor that arrives while the
    // panel is on screen takes focus.
    let quote = props.composer_anchor.as_ref().map(|a| a.quote.clone());
    let mounted = match state.read(cx).mounted_anchor_quote.clone() {
        None => {
            state.update(cx, |s, _| s.mounted_anchor_quote = Some(quote.clone()));
            quote.clone()
        }
        Some(mounted) => {
            if mounted.is_some() && quote != mounted {
                state.update(cx, |s, _| s.mounted_anchor_quote = Some(None));
                None
            } else {
                mounted
            }
        }
    };
    let anchor_focus_key = quote
        .as_ref()
        .filter(|q| Some(*q) != mounted.as_ref())
        .map(|q| SharedString::from(format!("anchor:{q}")));

    let submit_comment: super::types::DraftFn = {
        let (scope, state, context, report) = (
            scope.clone(),
            state.clone(),
            top_context.clone(),
            on_draft_change.clone(),
        );
        Rc::new(move |content, window, cx| {
            let submitted: SharedString = js_trim(&content).to_string().into();
            state.update(cx, |s, cx| {
                s.top_composer_error = None;
                cx.notify();
            });
            let task = scope.submit(content, None, window, cx);
            let (state, context, report) = (state.clone(), context.clone(), report.clone());
            window
                .spawn(cx, async move |cx| {
                    let result = task.await;
                    let _ = cx.update(|window, cx| match result {
                        SubmitResult::Posted => {
                            // Text typed while the post was in flight stays in the draft.
                            if js_trim(&state.read(cx).draft) == submitted.as_ref() {
                                state.update(cx, |s, cx| {
                                    s.draft = SharedString::default();
                                    cx.notify();
                                });
                                if let Some(f) = &report {
                                    f(SharedString::default(), context.clone(), window, cx);
                                }
                            }
                        }
                        SubmitResult::Failed => {
                            let message = astryx_i18n::current(cx).t("@astryx.comments.postFailed");
                            state.update(cx, |s, cx| {
                                s.draft = submitted.clone();
                                s.top_composer_error = Some(message);
                                cx.notify();
                            });
                            if let Some(f) = &report {
                                f(submitted.clone(), context.clone(), window, cx);
                            }
                        }
                        SubmitResult::Skipped => {}
                    });
                })
                .detach();
        })
    };

    let can_compose = mode != CommentMode::ReadOnly
        && props.composer_position != CommentsComposerPosition::None
        && can_submit;
    let (draft, top_error) = {
        let s = state.read(cx);
        (s.draft.clone(), s.top_composer_error.clone())
    };
    let composer: Option<AnyElement> = can_compose.then(|| {
        let (st, context, report) = (state.clone(), top_context.clone(), on_draft_change.clone());
        let on_change: super::types::DraftFn = Rc::new(move |next, window, cx| {
            st.update(cx, |s, cx| {
                s.draft = next.clone();
                s.top_composer_error = None;
                cx.notify();
            });
            if let Some(f) = &report {
                f(next, context.clone(), window, cx);
            }
        });
        let submit = submit_comment.clone();
        let mut composer = ComposerBase::new(
            scoped(&sid, "composer"),
            draft.clone(),
            on_change,
            Rc::new(move |next, window, cx| submit(next, window, cx)),
            scope
                .props
                .composer_placeholder
                .clone()
                .unwrap_or_else(|| i18n.t("@astryx.commentComposer.placeholder")),
            top_context.clone(),
        );
        configure_composer(&mut composer, &scope, top_context.clone());
        composer.anchor = props.composer_anchor.clone();
        composer.on_anchor_dismiss = props.on_composer_anchor_dismiss.clone();
        composer.focus_key = anchor_focus_key.clone();
        composer.is_submitting = scope.is_submitting(cx);
        composer.error = top_error.clone().or(props.composer_error.clone());
        composer.on_retry_error = top_error.as_ref().map(|_| {
            let (submit, draft) = (submit_comment.clone(), draft.clone());
            Rc::new(move |window: &mut Window, cx: &mut App| submit(draft.clone(), window, cx))
                as ActionFn
        });
        composer.controls = props.composer_controls;
        composer.into_any_element()
    });
    let loading_composer = (props.is_loading
        && mode != CommentMode::ReadOnly
        && props.composer_position != CommentsComposerPosition::None)
        .then(|| {
            composer_loading_skeleton(
                usize::from(has_uploads) + usize::from(has_emoji_picker),
                window,
                cx,
            )
        });
    let mut visible_composer = if props.is_loading {
        loading_composer
    } else {
        composer
    };

    let is_embedded = props.layout == CommentsLayout::Embedded;
    let card = container == CommentContainer::Card;
    // A pane bleeds a card parent's Sections to the panel edge through the message
    // list's padding; an embedded list has no list padding to cross.
    let bleeds = card && !is_embedded;
    let count = entries.len();
    let has_older = props.has_older_comments;
    // Inside a host's Card or Popover, its border bounds the list, so the first and
    // last threads drop their outer rules.
    let dividers_at = |index: usize| -> Vec<ThreadDivider> {
        THREAD_DIVIDERS
            .iter()
            .copied()
            .filter(|d| {
                !(is_embedded
                    && card
                    && ((*d == ThreadDivider::Top && index == 0 && !has_older)
                        || (*d == ThreadDivider::Bottom && index + 1 == count)))
            })
            .collect()
    };
    let rem = window.rem_size();
    let bleed = |el: gpui::Div| -> gpui::Div {
        if !bleeds {
            return el.w_full();
        }
        let (inline, block) = match props.density {
            CommentsDensity::Compact => (t.spacing.s3, t.spacing.s2),
            CommentsDensity::Balanced => (t.spacing.s4, t.spacing.s4),
        };
        el.mx(-inline.to_pixels(rem)).mt(-block.to_pixels(rem))
    };
    let empty_selector = format!("{sid}-empty");
    let empty_state: AnyElement = props.empty_state.unwrap_or_else(|| {
        EmptyState::new(i18n.t("@astryx.comments.emptyTitle"))
            .description(if can_compose {
                i18n.t("@astryx.comments.emptyDescription")
            } else {
                i18n.t("@astryx.comments.emptyReadOnlyDescription")
            })
            .compact(true)
            .into_any_element()
    });
    let empty_state = div()
        .w_full()
        .debug_selector(move || empty_selector)
        .child(empty_state)
        .into_any_element();
    let mut loading_stack = props
        .is_loading
        .then(|| bleed(div().flex().flex_col()).child(thread_loading_skeleton(card, window, cx)));

    // Focus follows the control that replaced the one the viewer used.
    let (older_error, failures, older_focus) = {
        let s = state.read(cx);
        (s.older_error.clone(), s.older_failure_count, s.older_focus)
    };
    if let Some(target) = older_focus {
        state.update(cx, |s, _| s.older_focus = None);
        let id = match target {
            OlderFocus::Retry if has_older && older_error.is_some() => {
                Some(older_retry_id(&sid, failures))
            }
            OlderFocus::Load if has_older && older_error.is_none() => {
                Some(scoped(&sid, "load-older"))
            }
            // With no earlier page left the button is gone; the region keeps focus.
            _ => None,
        };
        match id {
            Some(id) => request_focus(&id, cx),
            None => {
                let root = scope.root_focus.clone();
                window.defer(cx, move |window, cx| window.focus(&root, cx));
            }
        }
    }
    let older_region = window
        .use_keyed_state(scoped(&sid, "older-region"), cx, |_, cx| cx.focus_handle())
        .read(cx)
        .clone();
    let older = has_older.then(|| {
        let (st, load) = (state.clone(), props.on_load_older.clone());
        let row = div()
            .flex()
            .flex_col()
            .gap(t.spacing.s1)
            .items_center()
            .w_full()
            .track_focus(&older_region);
        match older_error.clone() {
            Some(error) => row.child(error_banner(
                scoped(&sid, format!("older-error-{failures}")),
                ErrorBannerContent {
                    title: error,
                    action_label: Some(i18n.t("@astryx.comments.retry")),
                    on_action: Some({
                        let region = older_region.clone();
                        Rc::new(move |window, cx| {
                            load_older(&region, &st, load.clone(), window, cx)
                        })
                    }),
                    ..ErrorBannerContent::default()
                },
                None,
                window,
                cx,
            )),
            None => {
                let region = older_region.clone();
                let busy = props.is_loading_older || state.read(cx).loading_older;
                let selector = format!("{sid}-load-older");
                row.child(
                    div().debug_selector(move || selector).child(
                        Button::new(
                            scoped(&sid, "load-older"),
                            i18n.t("@astryx.comments.loadOlder"),
                        )
                        .variant(ButtonVariant::Ghost)
                        .size(ElementSize::Sm)
                        .loading(busy)
                        .disabled(props.on_load_older.is_none())
                        .on_click(move |_, window, cx| {
                            load_older(&region, &st, load.clone(), window, cx)
                        }),
                    ),
                )
            }
        }
    });
    let gap = if card {
        gpui::px(0.)
    } else {
        match props.density {
            CommentsDensity::Compact => t.spacing.s2.to_pixels(rem),
            CommentsDensity::Balanced => t.spacing.s4.to_pixels(rem),
        }
    };
    let render_event = props.render_event.clone();
    let mut timeline_stack = bleed(div().flex().flex_col().gap(gap)).children(older);
    for (index, entry) in entries.iter().enumerate() {
        timeline_stack = timeline_stack.child(match entry {
            TimelineEntry::Event(event) => {
                activity_event_row(&sid, event, render_event.as_ref(), window, cx)
            }
            TimelineEntry::Comment(thread) => {
                CommentRow::thread((**thread).clone(), dividers_at(index)).into_any_element()
            }
        });
    }
    let list_density = match props.density {
        CommentsDensity::Compact => ChatDensity::Compact,
        CommentsDensity::Balanced => ChatDensity::Balanced,
    };
    let is_empty = entries.is_empty() && !has_older;

    let mut root = div()
        .id(scoped(&sid, "root"))
        .track_focus(&scope.root_focus)
        .role(gpui::Role::Region)
        .aria_label(label.clone())
        .aria_busy(props.is_loading)
        .flex()
        .flex_col()
        .gap(t.spacing.s3)
        .w_full()
        .debug_selector({
            let s = sid.to_string();
            move || s
        });
    if !is_embedded {
        root = root.h_full().min_h_0().overflow_hidden();
    }
    theme.refine(
        root.style(),
        "comments",
        &[
            ("layout", if is_embedded { "embedded" } else { "pane" }),
            ("density", props.density.as_str()),
            ("container", if card { "card" } else { "section" }),
        ],
        None,
    );
    root.style().refine(&props.style);
    if let Some(error) = props.load_error.clone() {
        let mut banner = Banner::new(
            scoped(&sid, "load-error"),
            BannerStatus::Error,
            i18n.t("@astryx.comments.loadFailed"),
        )
        .description(error)
        .container(BannerContainer::Section);
        if let Some(retry) = props.on_retry_load.clone() {
            banner = banner.end_content(
                Button::new(scoped(&sid, "retry-load"), i18n.t("@astryx.comments.retry"))
                    .variant(ButtonVariant::Ghost)
                    .size(ElementSize::Sm)
                    .click_action(move |_, window, cx| {
                        let task = retry(window, cx);
                        cx.background_executor().spawn(async move {
                            let _ = task.await;
                        })
                    }),
            );
        }
        root = root.child(
            div()
                .w_full()
                .debug_selector(|| "comments-load-error".into())
                .child(banner),
        );
    }
    if props.composer_position == CommentsComposerPosition::Top {
        root = root.children(visible_composer.take());
    }
    let dialogs = scope.dialogs(window, cx);
    let has_load_error = props.load_error.is_some();
    if is_embedded {
        root = root.child(if let Some(stack) = loading_stack.take() {
            stack.into_any_element()
        } else if is_empty {
            if has_load_error {
                div().into_any_element()
            } else {
                empty_state
            }
        } else {
            timeline_stack.into_any_element()
        });
        if props.composer_position == CommentsComposerPosition::Bottom {
            root = root.children(visible_composer.take());
        }
    } else {
        // ChatLayout's dock pads one spacing step less than ChatMessageList, so a
        // docked composer would stick out past the threads' edge. One step of inset
        // restores it.
        let dock = match (props.composer_position, visible_composer.take()) {
            (CommentsComposerPosition::Bottom, Some(composer)) => div()
                .flex()
                .flex_col()
                .w_full()
                .px(t.spacing.s1)
                .child(composer)
                .into_any_element(),
            _ => div().into_any_element(),
        };
        let mut chat = ChatLayout::new(scoped(&sid, "layout"), dock).density(list_density);
        if !has_load_error {
            chat = chat.empty_state(empty_state);
        }
        if let Some(stack) = loading_stack.take() {
            chat = chat.child(
                ChatMessageList::new(scoped(&sid, "list"))
                    .align(ChatListAlign::Top)
                    .density(list_density)
                    .gap(SpacingStep::S0)
                    .streaming(true)
                    .child(stack),
            );
        } else if !is_empty {
            chat = chat.child(
                ChatMessageList::new(scoped(&sid, "list"))
                    .align(ChatListAlign::Top)
                    .density(list_density)
                    .gap(SpacingStep::S0)
                    .child(timeline_stack),
            );
        }
        root = root.child(chat);
    }
    provide_scope(scope, root.child(dialogs)).into_any_element()
}
