//! The Comments family's parts (upstream lab `Comments/CommentParts.tsx`): error
//! banners, the reply rail, metadata, quotes, a comment still being written, the
//! clamped body, the New badge, the Resolved label, author names and avatars, activity
//! events, the edit history, and the loading shapes.

use astryx_components::button::{Button, ButtonVariant};
use astryx_components::containers::{
    Card, CardVariant, Elevation, EmptyState, Section, SectionSide, SectionVariant,
};
use astryx_components::content::Blockquote;
use astryx_components::feedback::{Banner, BannerContainer, BannerStatus};
use astryx_components::layout::{SpacingStep, VisuallyHidden};
use astryx_components::link::Link;
use astryx_components::media::Thumbnail;
use astryx_components::overlay::{Dialog, DialogHeader, Tooltip};
use astryx_components::people::{Avatar, AvatarShape, AvatarSize};
use astryx_components::status::{
    Badge, BadgeVariant, Skeleton, SkeletonRadius, Spinner, SpinnerSize,
};
use astryx_components::text::{Text, TextColor, TextType, TextWeight};
use astryx_components::time::{Timestamp, TimestampFormat, TimestampValue};
use astryx_i18n::Arg;
use astryx_primitives::{ElementSize, Politeness, announce};
use gpui::prelude::*;
use gpui::{AnyElement, App, ElementId, Hsla, Pixels, SharedString, Window, div, px, relative};

use super::context::{HistoryState, Scope, scoped};
use super::icons::{CommentIcon, CommentIconName};
use super::types::{
    ActionFn, CommentActivityEvent, CommentAnchor, CommentAnchorStatus, CommentHighlightColor,
    CommentProgress, CommentTime, CommentUser, CommentUserKind, CommentVersion,
};
use super::utils::event_anchor_id;

/// The 28 px avatar column every comment row, reply and event aligns to
/// (`--spacing-6` plus `--spacing-1`).
pub(crate) fn avatar_size(window: &Window, cx: &App) -> Pixels {
    let theme = astryx_theme::current(window, cx);
    let t = theme.tokens();
    let rem = window.rem_size();
    t.spacing.s6.to_pixels(rem) + t.spacing.s1.to_pixels(rem)
}

/// A `created_at` value as Timestamp's.
pub(crate) fn timestamp_value(time: &CommentTime) -> TimestampValue {
    match time {
        CommentTime::Text(text) => TimestampValue::Iso(text.clone()),
        CommentTime::Number(n) => {
            let ms = if *n < 1_000_000_000_000. {
                n * 1_000.
            } else {
                *n
            };
            TimestampValue::Unix((ms / 1_000.).floor() as i64)
        }
    }
}

/// Announces `message` once each time it changes (upstream lab
/// `useStatusAnnouncement`): a status that mounts already holding its text carries
/// no live role, so the shared channel speaks it instead.
pub(crate) fn announce_status(
    key: ElementId,
    message: Option<SharedString>,
    politeness: Politeness,
    window: &mut Window,
    cx: &mut App,
) {
    let last = window.use_keyed_state(key, cx, |_, _| None::<SharedString>);
    let previous = last.read(cx).clone();
    if previous != message {
        last.update(cx, |m, _| *m = message.clone());
        if let Some(message) = message.filter(|m| !m.is_empty()) {
            window.defer(cx, move |window, cx| {
                announce(message, politeness, window, cx)
            });
        }
    }
}

/// A failed action as an error Banner with its one recovery action (upstream
/// `ErrorBannerContent`).
#[derive(Clone, Default)]
pub(crate) struct ErrorBannerContent {
    pub title: SharedString,
    pub description: Option<SharedString>,
    pub action_label: Option<SharedString>,
    pub on_action: Option<ActionFn>,
    pub is_action_disabled: bool,
    /// Its own retry is running: say nothing until the outcome.
    pub is_retrying: bool,
    /// The failure was already announced elsewhere; this Banner stays silent.
    pub is_announced: bool,
}

/// Upstream `ErrorBanner`: an inline failure as an error Banner, announced once,
/// assertively, through the shared channel.
pub(crate) fn error_banner(
    id: ElementId,
    content: ErrorBannerContent,
    on_before_action: Option<ActionFn>,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let i18n = astryx_i18n::current(cx);
    let spoken = if content.is_retrying || content.is_announced {
        None
    } else {
        Some(match &content.description {
            Some(description) => i18n.format(
                "@astryx.comments.errorAnnouncement",
                &[
                    ("title", Arg::from(content.title.as_ref())),
                    ("description", Arg::from(description.as_ref())),
                ],
            ),
            None => content.title.clone(),
        })
    };
    announce_status(
        ElementId::Name(format!("{id}-announced").into()),
        spoken,
        Politeness::Assertive,
        window,
        cx,
    );
    let mut banner = Banner::new(id.clone(), BannerStatus::Error, content.title.clone())
        .container(BannerContainer::Card)
        .w_full();
    if let Some(description) = content.description.clone() {
        banner = banner.description(description);
    }
    if let (Some(label), Some(action)) = (content.action_label.clone(), content.on_action.clone()) {
        let selector = format!("{id}-action");
        banner = banner.end_content(
            div().debug_selector(move || selector).child(
                Button::new(ElementId::Name(format!("{id}-action").into()), label)
                    .variant(ButtonVariant::Ghost)
                    .size(ElementSize::Sm)
                    .disabled(content.is_action_disabled)
                    .on_click(move |_, window, cx| {
                        if let Some(before) = &on_before_action {
                            before(window, cx);
                        }
                        action(window, cx);
                    }),
            ),
        );
    }
    let selector = id.to_string();
    div()
        .w_full()
        .debug_selector(move || selector)
        .child(banner)
        .into_any_element()
}

/// One stretch of the 2 px reply rail (upstream `ThreadRailSegment`): from just below
/// a row's avatar (or the top of a row without one), to just short of the next row's
/// avatar (or into a next row that carries it on).
pub(crate) fn rail_segment(
    from_avatar: bool,
    to_avatar: bool,
    window: &Window,
    cx: &App,
) -> AnyElement {
    let theme = astryx_theme::current(window, cx);
    let t = theme.tokens();
    let rem = window.rem_size();
    let avatar = avatar_size(window, cx);
    let gap = t.spacing.s1.to_pixels(rem);
    let row_gap = t.spacing.s2.to_pixels(rem);
    let left = t.spacing.s3.to_pixels(rem) + t.border.width.to_pixels(rem);
    div()
        .absolute()
        .left(left)
        .w(t.spacing.s0_5)
        .bg(t.color.border_emphasized)
        .top(if from_avatar { avatar + gap } else { px(0.) })
        .bottom(if to_avatar { gap - row_gap } else { -row_gap })
        .into_any_element()
}

/// The fading rail of a collapsed thread with hidden replies (upstream
/// `CollapsedThreadRail`).
pub(crate) fn collapsed_rail(window: &Window, cx: &App) -> AnyElement {
    let theme = astryx_theme::current(window, cx);
    let t = theme.tokens();
    let rem = window.rem_size();
    let avatar = avatar_size(window, cx);
    let left = t.spacing.s3.to_pixels(rem) + t.border.width.to_pixels(rem);
    let from = t.color.border_emphasized;
    div()
        .absolute()
        .top(avatar + t.spacing.s1.to_pixels(rem))
        .bottom_0()
        .left(left)
        .w(t.spacing.s0_5)
        .bg(gpui::linear_gradient(
            180.,
            gpui::linear_color_stop(from, 0.),
            gpui::linear_color_stop(from.opacity(0.), 1.),
        ))
        .into_any_element()
}

/// Upstream `MetadataSeparator`: a secondary `·`.
pub(crate) fn separator(text_type: TextType) -> AnyElement {
    Text::new("\u{b7}")
        .text_type(text_type)
        .color(TextColor::Secondary)
        .into_any_element()
}

/// Upstream `MetadataActionLink`: a secondary text link that runs an action, such as
/// Edited or Cancel; a disabled one stays in the tab order.
pub(crate) fn action_link(
    id: ElementId,
    label: SharedString,
    accessible_label: Option<SharedString>,
    disabled: bool,
    on_click: ActionFn,
) -> AnyElement {
    let selector = id.to_string();
    div()
        .debug_selector(move || selector)
        .child(
            Link::new(id, label.clone())
                .label(accessible_label.unwrap_or(label))
                .disabled(disabled)
                .on_click(move |_, window, cx| {
                    if !disabled {
                        on_click(window, cx);
                    }
                }),
        )
        .into_any_element()
}

/// A highlight color's border token (upstream `highlightRules`).
pub(crate) fn highlight_border(color: CommentHighlightColor, window: &Window, cx: &App) -> Hsla {
    let theme = astryx_theme::current(window, cx);
    let c = &theme.tokens().color;
    match color {
        CommentHighlightColor::Blue => c.border_blue,
        CommentHighlightColor::Cyan => c.border_cyan,
        CommentHighlightColor::Gray => c.border_gray,
        CommentHighlightColor::Green => c.border_green,
        CommentHighlightColor::Orange => c.border_orange,
        CommentHighlightColor::Pink => c.border_pink,
        CommentHighlightColor::Purple => c.border_purple,
        CommentHighlightColor::Red => c.border_red,
        CommentHighlightColor::Teal => c.border_teal,
        CommentHighlightColor::Yellow => c.border_yellow,
    }
}

/// Upstream `QuotedPassage`: a quoted passage in Markdown's blockquote style, italic,
/// two lines at most; a highlighted passage's rule takes the highlight's color.
pub(crate) fn quoted_passage(
    quote: &SharedString,
    highlight: Option<CommentHighlightColor>,
    window: &Window,
    cx: &App,
) -> AnyElement {
    let mut quote_el = Blockquote::new().child(
        Text::new(quote.clone())
            .text_type(TextType::Body)
            .color(TextColor::Inherit)
            .max_lines(2)
            .italic(),
    );
    if let Some(color) = highlight {
        quote_el = quote_el.border_color(highlight_border(color, window, cx));
    }
    quote_el.into_any_element()
}

/// Upstream `CommentAnchorQuote`: the passage an inline comment discusses, quoted
/// under its author's name; an orphaned one carries an Outdated badge.
pub(crate) fn anchor_quote(
    anchor: &CommentAnchor,
    highlight: CommentHighlightColor,
    resolved: bool,
    window: &Window,
    cx: &App,
) -> AnyElement {
    let theme = astryx_theme::current(window, cx);
    let t = theme.tokens();
    let i18n = astryx_i18n::current(cx);
    let orphaned = anchor.status == CommentAnchorStatus::Orphaned;
    let color = (!orphaned && !resolved).then_some(highlight);
    let quote = quoted_passage(&anchor.quote, color, window, cx);
    div()
        .flex()
        .items_center()
        .gap(t.spacing.s2)
        .w_full()
        .py(t.spacing.s1)
        .when(resolved, |d| d.text_color(t.color.text_secondary))
        .child(div().when(!orphaned, |d| d.flex_1()).min_w_0().child(quote))
        // The passage was edited away; the quote is what the comment saw.
        .children(orphaned.then(|| {
            div().flex_none().child(
                Badge::new(i18n.t("@astryx.comments.outdated")).variant(BadgeVariant::Neutral),
            )
        }))
        .debug_selector(move || {
            if orphaned {
                "comment-anchor-orphaned".into()
            } else {
                "comment-anchor-anchored".into()
            }
        })
        .into_any_element()
}

/// Upstream `CommentProgressStatus`: a comment its author is still writing: what it is
/// doing, shimmering while motion is allowed, and where to watch.
pub(crate) fn progress_status(
    id: ElementId,
    progress: &CommentProgress,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    announce_status(
        ElementId::Name(format!("{id}-progress").into()),
        Some(progress.label.clone()),
        Politeness::Polite,
        window,
        cx,
    );
    let theme = astryx_theme::current(window, cx);
    let t = theme.tokens();
    let reduced = astryx_primitives::prefers_reduced_motion(cx);
    let label = Text::new(progress.label.clone())
        .text_type(TextType::Body)
        .color(TextColor::Secondary);
    // The light band crosses the label left to right over twice the slow token.
    let label = if reduced {
        label.into_any_element()
    } else {
        let period = t.duration.slow_max.as_secs_f64() * 2.;
        let now = cx.background_executor().now();
        let start = *window
            .use_keyed_state(
                ElementId::Name(format!("{id}-shimmer").into()),
                cx,
                move |_, _| now,
            )
            .read(cx);
        let phase =
            (now.saturating_duration_since(start).as_secs_f64() / period.max(0.001)).fract() as f32;
        window.request_animation_frame();
        let (base, light) = (t.color.text_secondary, t.color.text_primary);
        div()
            .relative()
            .child(label)
            .child(
                div()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left(relative(phase * 1.5 - 0.5))
                    .w(relative(0.5))
                    .bg(gpui::linear_gradient(
                        90.,
                        gpui::linear_color_stop(base.opacity(0.), 0.),
                        gpui::linear_color_stop(light.opacity(0.25), 1.),
                    )),
            )
            .overflow_hidden()
            .into_any_element()
    };
    div()
        .flex()
        .flex_wrap()
        .items_center()
        .gap(t.spacing.s1)
        .w_full()
        .debug_selector(|| "comment-progress".into())
        .child(label)
        .children(progress.link.clone().map(|(label, href)| {
            Link::new(ElementId::Name(format!("{id}-progress-link").into()), label)
                .href(href)
                .target("_blank")
        }))
        .into_any_element()
}

/// Upstream `UnreadBadge`: New, which holds for five seconds then fades over the slow
/// token, and leaves the layout once it has faded.
pub(crate) fn unread_badge(id: ElementId, window: &mut Window, cx: &mut App) -> Option<AnyElement> {
    let theme = astryx_theme::current(window, cx);
    let t = theme.tokens();
    let i18n = astryx_i18n::current(cx);
    let now = cx.background_executor().now();
    let start = *window.use_keyed_state(id, cx, move |_, _| now).read(cx);
    let hold = std::time::Duration::from_secs(5);
    let fade = if astryx_primitives::prefers_reduced_motion(cx) {
        std::time::Duration::from_millis(1)
    } else {
        t.duration.slow_max
    };
    let elapsed = now.saturating_duration_since(start);
    if elapsed >= hold + fade {
        return None;
    }
    let opacity = if elapsed <= hold {
        1.
    } else {
        1. - t
            .ease
            .standard
            .ease(((elapsed - hold).as_secs_f32() / fade.as_secs_f32().max(0.001)).clamp(0., 1.))
    };
    let remaining = if elapsed < hold {
        hold - elapsed
    } else {
        std::time::Duration::ZERO
    };
    if remaining > std::time::Duration::ZERO {
        let refresh = cx.background_executor().timer(remaining);
        window
            .spawn(cx, async move |cx| {
                refresh.await;
                let _ = cx.update(|window, _| window.refresh());
            })
            .detach();
    } else {
        window.request_animation_frame();
    }
    Some(
        div()
            .flex()
            .items_center()
            .opacity(opacity)
            .debug_selector(|| "comment-unread-badge".into())
            .child(Badge::new(i18n.t("@astryx.comments.unread")).variant(BadgeVariant::Info))
            .into_any_element(),
    )
}

/// Upstream `ResolvedLabel`: Resolved, beside the time; names who resolved it when the
/// host knows.
pub(crate) fn resolved_label(
    id: ElementId,
    resolved_by: Option<&CommentUser>,
    cx: &App,
) -> AnyElement {
    let i18n = astryx_i18n::current(cx);
    let label = div()
        .whitespace_nowrap()
        .debug_selector(|| "comment-resolved-label".into())
        .child(
            Text::new(i18n.t("@astryx.comments.resolved"))
                .text_type(TextType::Body)
                .color(TextColor::Secondary),
        );
    match resolved_by {
        None => label.into_any_element(),
        Some(user) => Tooltip::new(
            id,
            i18n.format(
                "@astryx.comments.resolvedBy",
                &[("name", Arg::from(user.name.as_ref()))],
            ),
        )
        .child(label)
        .into_any_element(),
    }
}

/// Upstream `AuthorName`: the author's name, a link to them when the host has one.
pub(crate) fn author_name(id: ElementId, author: &CommentUser) -> AnyElement {
    let name = author.name.clone();
    let text: AnyElement = match &author.href {
        Some(href) => Link::new(id, name.clone())
            .href(href.clone())
            .into_any_element(),
        None => Text::new(name.clone())
            .text_type(TextType::Label)
            .color(TextColor::Primary)
            .weight(TextWeight::Medium)
            .max_lines(1)
            .into_any_element(),
    };
    div()
        .flex_shrink(1.)
        .min_w_0()
        .debug_selector(move || format!("comment-author-{name}"))
        .child(text)
        .into_any_element()
}

/// Upstream `CommentAvatarFrame`: a 28 × 28 avatar on an opaque disc: a circle for
/// people, a rounded square for agents, an agent's logo when it has one.
pub(crate) fn avatar_frame(
    id: ElementId,
    user: &CommentUser,
    window: &Window,
    cx: &App,
) -> AnyElement {
    let theme = astryx_theme::current(window, cx);
    let t = theme.tokens();
    let i18n = astryx_i18n::current(cx);
    let size = avatar_size(window, cx);
    let agent = user.kind == CommentUserKind::Agent;
    let logo = user
        .avatar_url
        .as_ref()
        .map(|u| astryx_theme::authoring::js::js_trim(u).to_string())
        .filter(|u| !u.is_empty());
    let inner: AnyElement = match (&logo, agent) {
        (Some(logo), true) => Thumbnail::new(id.clone())
            .src(logo.clone())
            .alt(i18n.format(
                "@astryx.comments.agentLogo",
                &[("name", Arg::from(user.name.as_ref()))],
            ))
            .size(size)
            .into_any_element(),
        _ => {
            let mut avatar = Avatar::new(id.clone())
                .name(user.name.clone())
                .size(AvatarSize::Px(f32::from(size)))
                .shape(if agent {
                    AvatarShape::Rounded
                } else {
                    AvatarShape::Circle
                });
            if let Some(logo) = logo {
                avatar = avatar.src(logo);
            }
            avatar.into_any_element()
        }
    };
    let radius = if agent {
        t.radius.element
    } else {
        t.radius.full
    };
    div()
        .relative()
        .flex()
        .items_center()
        .justify_center()
        .flex_none()
        .overflow_hidden()
        .size(size)
        .rounded(radius)
        // Avatars sit on an opaque card-surface disc in every state.
        .bg(t.color.background_card)
        .child(inner)
        .into_any_element()
}

/// Upstream `AvatarSpacer`: the empty 28 px avatar column.
pub(crate) fn avatar_spacer(window: &Window, cx: &App) -> AnyElement {
    div()
        .flex_none()
        .w(avatar_size(window, cx))
        .into_any_element()
}

/// Upstream `ActivityEventRow`: an event as a compact row: its glyph, what happened,
/// who did it, and its time linking where the host says.
pub(crate) fn activity_event_row(
    scope_id: &str,
    event: &CommentActivityEvent,
    render: Option<&super::RenderEventFn>,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let anchor = event_anchor_id(&event.id);
    let selector = format!("comments-event-{}", event.id);
    if let Some(render) = render {
        return div()
            .w_full()
            .debug_selector(move || selector)
            .child(render(event, window, cx))
            .into_any_element();
    }
    let theme = astryx_theme::current(window, cx);
    let t = theme.tokens();
    let href: SharedString = event
        .href
        .clone()
        .unwrap_or_else(|| format!("#{anchor}").into());
    let description = (event.actor.is_some() || event.description.is_some()).then(|| {
        div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(t.spacing.s1)
            .children(event.actor.as_ref().map(|actor| {
                author_name(scoped(scope_id, format!("event-actor-{}", event.id)), actor)
            }))
            .children(event.description.clone().map(|d| {
                Text::new(d)
                    .text_type(TextType::Supporting)
                    .color(TextColor::Secondary)
            }))
    });
    let time = Timestamp::new(
        scoped(scope_id, format!("event-time-{}", event.id)),
        timestamp_value(&event.created_at),
    )
    .format(TimestampFormat::RelativeShort)
    .text_type(TextType::Supporting)
    .color(TextColor::Secondary)
    .live(true);
    let time = Link::new(
        scoped(scope_id, format!("event-link-{}", event.id)),
        SharedString::default(),
    )
    .href(href)
    .child(time);
    div()
        .flex()
        .items_start()
        .gap(t.spacing.s2)
        .w_full()
        .pl(t.spacing.s3)
        .py(t.spacing.s1)
        .debug_selector(move || selector)
        .child(
            div()
                .flex()
                .items_center()
                .justify_center()
                .flex_none()
                .w(avatar_size(window, cx))
                .child(
                    CommentIcon::new(CommentIconName::Event)
                        .size(astryx_components::icon::IconSize::Md),
                ),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .child(Text::new(event.label.clone()).text_type(TextType::Body))
                .children(description),
        )
        .child(div().flex_none().child(time))
        .into_any_element()
}

/// Upstream `HistoryDialog`: a comment's versions, newest first, the current one
/// muted, with its loading and failure states.
pub(crate) fn history_dialog(
    scope: &Scope,
    history: HistoryState,
    on_close: ActionFn,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let i18n = astryx_i18n::current(cx);
    let theme = astryx_theme::current(window, cx);
    let t = theme.tokens();
    announce_status(
        scoped(&scope.id, "history-busy"),
        history
            .is_loading
            .then(|| i18n.t("@astryx.comments.historyLoading")),
        Politeness::Polite,
        window,
        cx,
    );
    let render_version = scope.props.render_version_body.clone();
    let agents = scope.props.agent_commands.clone();
    let body: AnyElement = if history.is_loading {
        div()
            .flex()
            .flex_col()
            .items_center()
            .py(t.spacing.s4)
            .child(
                Spinner::new(scoped(&scope.id, "history-spinner"))
                    .size(SpinnerSize::Lg)
                    .label(i18n.t("@astryx.comments.historyLoading")),
            )
            .into_any_element()
    } else if history.error.is_some() {
        div().into_any_element()
    } else if history.versions.is_empty() {
        EmptyState::new(i18n.t("@astryx.comments.historyEmptyTitle"))
            .description(i18n.t("@astryx.comments.historyEmptyDescription"))
            .compact(true)
            .into_any_element()
    } else {
        let mut list = div().flex().flex_col().gap(t.spacing.s3);
        for version in &history.versions {
            let content: AnyElement = match &render_version {
                Some(f) => f(version, window, cx),
                None => super::markdown::CommentMarkdown::new(
                    scoped(&scope.id, format!("version-{}", version.id)),
                    version.body.clone(),
                    agents.clone(),
                )
                .into_any_element(),
            };
            list = list.child(version_section(&scope.id, version, content, window, cx));
        }
        list.into_any_element()
    };
    // A failure is delivered by an alert slot inside the dialog (the shared channel is
    // not exposed while a modal is open).
    let alert = (!history.is_loading)
        .then(|| history.error.clone())
        .flatten()
        .map(|error| {
            Banner::new(
                scoped(&scope.id, "history-error"),
                BannerStatus::Error,
                i18n.t("@astryx.comments.historyFailed"),
            )
            .description(error)
            .container(BannerContainer::Section)
        });
    let close = on_close.clone();
    Dialog::new(scoped(&scope.id, "history-dialog"), true)
        .on_open_change(move |open, window, cx| {
            if !open {
                close(window, cx);
            }
        })
        .child(
            DialogHeader::new(i18n.t("@astryx.comments.historyTitle"))
                .subtitle(i18n.format(
                    "@astryx.comments.historySubtitle",
                    &[("name", Arg::from(history.comment.author.name.as_ref()))],
                ))
                .divider(true)
                .on_open_change(move |open, window, cx| {
                    if !open {
                        on_close(window, cx);
                    }
                }),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap(t.spacing.s3)
                .p(t.spacing.s4)
                .children(alert)
                .child(body),
        )
        .into_any_element()
}

fn version_section(
    scope_id: &str,
    version: &CommentVersion,
    content: AnyElement,
    window: &Window,
    cx: &App,
) -> AnyElement {
    let theme = astryx_theme::current(window, cx);
    let t = theme.tokens();
    let i18n = astryx_i18n::current(cx);
    let selector = format!("comment-version-{}", version.id);
    let section = Section::new()
        .variant(if version.is_current {
            SectionVariant::Muted
        } else {
            SectionVariant::Transparent
        })
        .padding(SpacingStep::S2)
        .child(
            div()
                .flex()
                .flex_col()
                .gap(t.spacing.s1_5)
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap(t.spacing.s1)
                        .child(
                            Timestamp::new(
                                scoped(scope_id, format!("version-time-{}", version.id)),
                                timestamp_value(&version.edited_at),
                            )
                            .format(TimestampFormat::DateTime),
                        )
                        .children(version.author.as_ref().map(|author| {
                            Text::new(i18n.format(
                                "@astryx.comments.versionAuthor",
                                &[("name", Arg::from(author.name.as_ref()))],
                            ))
                            .text_type(TextType::Supporting)
                            .color(TextColor::Secondary)
                        }))
                        .children(version.is_current.then(|| {
                            Text::new(i18n.t("@astryx.comments.currentVersion"))
                                .text_type(TextType::Supporting)
                                .weight(TextWeight::Semibold)
                        })),
                )
                .child(content),
        );
    div()
        .w_full()
        .debug_selector(move || selector)
        .child(section)
        .into_any_element()
}

fn loading_row(index: u32, card: bool, window: &Window, cx: &App) -> AnyElement {
    let theme = astryx_theme::current(window, cx);
    let t = theme.tokens();
    let rem = window.rem_size();
    let avatar = avatar_size(window, cx);
    let s4 = t.spacing.s4.to_pixels(rem);
    let bar = |width: gpui::Length, height: Pixels, radius: SkeletonRadius, n: u32| {
        Skeleton::new()
            .width(width)
            .height(height)
            .radius(radius)
            .index(index * 7 + n)
    };
    let content = div()
        .flex()
        .items_start()
        .gap(t.spacing.s2)
        .w_full()
        .child(
            Skeleton::new()
                .width(avatar)
                .height(avatar)
                .radius(SkeletonRadius::Rounded)
                .index(index * 7),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .gap(t.spacing.s1)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(t.spacing.s1)
                        .child(bar(
                            relative(if index == 0 { 0.38 } else { 0.3 }).into(),
                            s4,
                            SkeletonRadius::R1,
                            1,
                        ))
                        .child(bar(
                            relative(0.18).into(),
                            t.spacing.s3.to_pixels(rem),
                            SkeletonRadius::R1,
                            2,
                        )),
                )
                .child(bar(relative(0.96).into(), s4, SkeletonRadius::R1, 3))
                .child(bar(
                    relative(if index == 0 { 0.82 } else { 0.68 }).into(),
                    s4,
                    SkeletonRadius::R1,
                    4,
                ))
                .children(
                    (index == 0).then(|| bar(relative(0.54).into(), s4, SkeletonRadius::R1, 5)),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(t.spacing.s1)
                        .child(bar(
                            (t.spacing.s8.to_pixels(rem) + s4).into(),
                            t.spacing.s6.to_pixels(rem),
                            SkeletonRadius::R2,
                            5,
                        ))
                        .child(bar(
                            t.spacing.s8.to_pixels(rem).into(),
                            t.spacing.s6.to_pixels(rem),
                            SkeletonRadius::R2,
                            6,
                        )),
                ),
        );
    if card {
        Section::new()
            .variant(SectionVariant::Transparent)
            .dividers([SectionSide::Top, SectionSide::Bottom])
            .padding(SpacingStep::S3)
            .width(relative(1.))
            .child(content)
            .into_any_element()
    } else {
        Card::new()
            .variant(CardVariant::Default)
            .elevation(Elevation::None)
            .padding(SpacingStep::S3)
            .width(relative(1.))
            .child(content)
            .into_any_element()
    }
}

/// Upstream `CommentThreadLoadingSkeleton`: comment-shaped placeholders in the same
/// Card or Section the threads use.
pub(crate) fn thread_loading_skeleton(card: bool, window: &Window, cx: &App) -> AnyElement {
    let theme = astryx_theme::current(window, cx);
    let t = theme.tokens();
    let i18n = astryx_i18n::current(cx);
    div()
        .flex()
        .flex_col()
        .gap(if card {
            px(0.)
        } else {
            t.spacing.s2.to_pixels(window.rem_size())
        })
        .w_full()
        .debug_selector(|| "comments-loading-threads".into())
        .child(VisuallyHidden::new().child(i18n.t("@astryx.comments.loading")))
        .children((0..3).map(|i| loading_row(i, card, window, cx)))
        .into_any_element()
}

/// Upstream `CommentComposerLoadingSkeleton`: the two-row composer's shape.
pub(crate) fn composer_loading_skeleton(
    action_count: usize,
    window: &Window,
    cx: &App,
) -> AnyElement {
    let theme = astryx_theme::current(window, cx);
    let t = theme.tokens();
    let rem = window.rem_size();
    let action = t.spacing.s8.to_pixels(rem);
    Card::new()
        .variant(CardVariant::Default)
        .elevation(Elevation::None)
        .padding(SpacingStep::S2)
        .width(relative(1.))
        .rounded(t.radius.container)
        .child(
            div()
                .flex()
                .flex_col()
                .gap(t.spacing.s2)
                .w_full()
                .debug_selector(|| "comments-loading-composer".into())
                .child(
                    Skeleton::new()
                        .width(relative(0.42))
                        .height(t.spacing.s4.to_pixels(rem))
                        .radius(SkeletonRadius::R1)
                        .index(22),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .w_full()
                        .child(div().flex().items_center().gap(t.spacing.s0_5).children(
                            (0..action_count).map(|i| {
                                Skeleton::new()
                                    .width(action)
                                    .height(action)
                                    .radius(SkeletonRadius::R2)
                                    .index(23 + i as u32)
                            }),
                        ))
                        .child(
                            Skeleton::new()
                                .width(action)
                                .height(action)
                                .radius(SkeletonRadius::R2)
                                .index(23 + action_count as u32),
                        ),
                ),
        )
        .into_any_element()
}

/// A resolved thread's words step down to the secondary text color (upstream
/// `RESOLVED_TEXT_STYLE`): text that takes no color of its own inherits it.
pub(crate) fn muted<E: Styled>(el: E, muted: bool, window: &Window, cx: &App) -> E {
    if muted {
        let theme = astryx_theme::current(window, cx);
        el.text_color(theme.tokens().color.text_secondary)
    } else {
        el
    }
}
