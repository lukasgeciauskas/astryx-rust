//! ChatComposerDrawer, upstream `Chat/ChatComposerDrawer.tsx`: a panel above a chat
//! composer's input for attachments, context chips or previews, which a `count` makes
//! collapsible to a one-line summary.

use std::rc::Rc;

use gpui::Refineable as _;
use gpui::prelude::*;
use gpui::{AnyElement, App, ElementId, Role, SharedString, StyleRefinement, Window, div};
use rustryx_primitives::{OutOfTabOrder, control_focus, is_focus_visible, provide};
use rustryx_theme::color::composite;
use rustryx_theme::{ThemeStyled as _, current};
use smallvec::SmallVec;

use crate::focus::focus_ring_inset;
use crate::status::{Badge, BadgeVariant};

type CollapsedFn = Rc<dyn Fn(bool, &mut Window, &mut App)>;

/// The items' fade: from what it showed when the last change began.
struct ItemsFade {
    shown: bool,
    from: f32,
    since: Option<web_time::Instant>,
}

/// The items' opacity now: in over `medium` after a `fast` delay, out over `fast`.
fn items_fade_value(
    fade: &ItemsFade,
    now: web_time::Instant,
    t: &rustryx_theme::Tokens,
    reduced: bool,
) -> (f32, bool) {
    let target = if fade.shown { 1. } else { 0. };
    let Some(since) = fade.since.filter(|_| !reduced) else {
        return (target, false);
    };
    let (delay, duration) = if fade.shown {
        (t.duration.fast, t.duration.medium)
    } else {
        (std::time::Duration::ZERO, t.duration.fast)
    };
    let elapsed = now.saturating_duration_since(since);
    if elapsed < delay {
        return (fade.from, true);
    }
    if duration.is_zero() {
        return (target, false);
    }
    let p = ((elapsed - delay).as_secs_f32() / duration.as_secs_f32()).min(1.);
    let value = fade.from + (target - fade.from) * t.ease.standard.ease(p);
    (value, p < 1.)
}

/// Upstream `ChatComposerDrawer`.
///
/// Elements it draws carry debug selectors after its id: `<id>` (the root),
/// `<id>-toggle`, `<id>-summary`, `<id>-label` (the default summary's label),
/// `<id>-handle` and `<id>-content`.
///
/// The collapse eases the content's height over `--duration-medium` on `--ease-standard`
/// (upstream's grid row) while the items fade: out over `--duration-fast` as it collapses,
/// in over `--duration-medium` after a `--duration-fast` delay as it expands; the summary
/// and the handle cross-fade over `--duration-fast`. Reduce Motion changes it at once.
/// Collapsing content is inert (fork patch R20), as upstream's: out of the tab order, the
/// pointer's targets and the accessibility tree (FR4); once the collapse ends it is not
/// drawn at all.
///
/// ```ignore
/// ChatComposerDrawer::new("attachments")
///     .count(2)
///     .child(Badge::new("report.pdf"))
///     .child(Badge::new("notes.md"))
/// ```
#[derive(IntoElement)]
pub struct ChatComposerDrawer {
    id: ElementId,
    children: SmallVec<[AnyElement; 2]>,
    count: Option<usize>,
    label: Option<SharedString>,
    collapsed_summary: Option<AnyElement>,
    collapsed: Option<bool>,
    default_collapsed: bool,
    on_collapsed_change: Option<CollapsedFn>,
    style: StyleRefinement,
}

impl ChatComposerDrawer {
    /// A drawer; `id` keys its uncontrolled state and names its parts.
    pub fn new(id: impl Into<ElementId>) -> Self {
        ChatComposerDrawer {
            id: id.into(),
            children: SmallVec::new(),
            count: None,
            label: None,
            collapsed_summary: None,
            collapsed: None,
            default_collapsed: false,
            on_collapsed_change: None,
            style: StyleRefinement::default(),
        }
    }

    rustryx_primitives::setters! {
        /// Upstream `count`: the collapsed badge's number. Any count, 0 included,
        /// makes the drawer collapsible; without one it shows its content only.
        ?count: usize,
        /// Upstream `label`: after the count in the default summary, and in the
        /// toggle's accessible name. Default "Items" (`@astryx.chat.composerDrawer.label`).
        ?label: SharedString,
        /// Upstream `isCollapsed`: controlled collapse.
        ?collapsed: bool,
        /// Upstream `defaultIsCollapsed`: the uncontrolled initial state.
        !default_collapsed,
    }

    /// Upstream `collapsedSummary`: replaces the whole default summary (the count
    /// Badge and the label) while collapsed. It is visual only: it never takes focus,
    /// and the toggle keeps its own name.
    pub fn collapsed_summary(mut self, summary: impl IntoElement) -> Self {
        self.collapsed_summary = Some(summary.into_any_element());
        self
    }

    /// Upstream `onCollapsedChange`: the requested state on each toggle.
    pub fn on_collapsed_change(
        mut self,
        f: impl Fn(bool, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_collapsed_change = Some(Rc::new(f));
        self
    }
}

impl Styled for ChatComposerDrawer {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl ParentElement for ChatComposerDrawer {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

/// The toggle's accessible name (upstream's expand and collapse messages).
pub(crate) fn toggle_name(collapsed: bool, label: &str, cx: &App) -> SharedString {
    let key = if collapsed {
        "@astryx.chatComposerDrawer.expand"
    } else {
        "@astryx.chatComposerDrawer.collapse"
    };
    rustryx_i18n::current(cx).format(key, &[("label", label.to_string().into())])
}

impl RenderOnce for ChatComposerDrawer {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let rem = window.rem_size();
        let key = self.id.to_string();
        let label = self
            .label
            .clone()
            .unwrap_or_else(|| rustryx_i18n::current(cx).t("@astryx.chat.composerDrawer.label"));
        let internal = window.use_keyed_state(
            ElementId::Name(format!("{key}-collapsed").into()),
            cx,
            |_, _| self.default_collapsed,
        );
        let controlled = self.collapsed.is_some();
        let collapsed = self.collapsed.unwrap_or(*internal.read(cx));
        let can_collapse = self.count.is_some();
        let shown = !(can_collapse && collapsed);
        // The motion: the content's height, the items' fade and the summary's cross-fade.
        let height = crate::animated_layout::adopt::use_height_transition(
            format!("{key}-content"),
            shown,
            t.duration.medium,
            t.ease.standard,
            window,
            cx,
        );
        let reduced = rustryx_primitives::prefers_reduced_motion(cx);
        let now = cx.background_executor().now();
        let fade = window.use_keyed_state(
            ElementId::Name(format!("{key}-items-fade").into()),
            cx,
            |_, _| ItemsFade {
                shown,
                from: if shown { 1. } else { 0. },
                since: None,
            },
        );
        if fade.read(cx).shown != shown {
            let (value, _) = items_fade_value(fade.read(cx), now, t, reduced);
            fade.update(cx, |f, _| {
                f.from = value;
                f.shown = shown;
                f.since = Some(now);
            });
        }
        let (items_opacity, fading) = items_fade_value(fade.read(cx), now, t, reduced);
        if fading {
            window.request_animation_frame();
        }
        let summary_shown = crate::motion::transition(
            format!("{key}-summary-fade"),
            collapsed,
            t.duration.fast,
            t.ease.standard,
            window,
            cx,
        );

        // Upstream reads the composer's copy of --radius-chat; a theme that changes the
        // composer's corner changes the token the drawer reads here too.
        let radius = t.radius.chat.to_pixels(rem);
        let s3 = t.spacing.s3.to_pixels(rem);
        let props: &[(&str, &str)] = if collapsed {
            &[("collapsed", "collapsed")]
        } else {
            &[]
        };
        let root_key = key.clone();
        let mut root = div()
            .id(self.id.clone())
            .debug_selector(move || root_key)
            .relative()
            .flex()
            .flex_col()
            .overflow_hidden()
            .px(t.spacing.s4)
            .pt(t.spacing.s3)
            // It tucks behind the composer body by one corner radius.
            .pb(s3 + radius)
            .mb(-radius)
            // The surface with the muted tint over it, composited so a translucent
            // muted color still sits on the surface (upstream's background-image layer).
            .bg(composite(
                t.color.background_surface,
                t.color.background_muted,
            ))
            .rounded_t(radius);
        theme.refine(root.style(), "chat-composer-drawer", props, None);
        root.style().refine(&self.style);

        let content_name = SharedString::from(format!("{key}-content"));
        if let Some(count) = self.count {
            let toggle_key = format!("{key}-toggle");
            let toggle_id = ElementId::Name(toggle_key.clone().into());
            let focus = control_focus(&toggle_id, window, cx);
            let group = SharedString::from(toggle_key.clone());
            let (primary, icon_primary) = (t.color.text_primary, t.color.icon_primary);
            let summary = (summary_shown > 0.).then(|| {
                let summary_key = format!("{key}-summary");
                let content = match self.collapsed_summary {
                    Some(custom) => custom,
                    None => {
                        let label_key = format!("{key}-label");
                        div()
                            .flex()
                            .items_center()
                            .gap(t.spacing.s2)
                            .child(Badge::new(count.to_string()).variant(BadgeVariant::Neutral))
                            .child(
                                div()
                                    .debug_selector(move || label_key)
                                    .font_stack(&t.font.family.body)
                                    .text_size(t.text.body.size)
                                    .line_height(t.spacing.s5)
                                    .text_color(t.color.text_secondary)
                                    .group_hover(group.clone(), move |s| s.text_color(primary))
                                    .child(label.clone()),
                            )
                            .into_any_element()
                    }
                };
                // Presentation only (FR10): nothing inside takes a tab stop.
                provide(
                    OutOfTabOrder,
                    div()
                        .debug_selector(move || summary_key)
                        .flex()
                        .items_center()
                        .h(t.spacing.s5)
                        .gap(t.spacing.s2)
                        .rounded(t.radius.full)
                        .when(summary_shown < 1., |el| el.opacity(summary_shown))
                        .child(content),
                )
            });
            let handle = (summary_shown < 1.).then(|| {
                let handle_key = format!("{key}-handle");
                // Centred at the top of the row, in the summary's cell.
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .right_0()
                    .flex()
                    .justify_center()
                    .when(summary_shown > 0., |el| el.opacity(1. - summary_shown))
                    .child(
                        div()
                            .debug_selector(move || handle_key)
                            .w(t.spacing.s5)
                            .h(t.spacing.s0_5)
                            .rounded(t.radius.full)
                            .bg(t.color.icon_secondary)
                            .group_hover(group.clone(), move |s| s.bg(icon_primary)),
                    )
            });
            let ring = is_focus_visible(&focus, window)
                .then(|| focus_ring_inset(t, t.radius.element.to_pixels(rem), rem));
            let (on_change, state) = (self.on_collapsed_change.clone(), internal.clone());
            let s4 = t.spacing.s4.to_pixels(rem);
            let toggle = div()
                .id(toggle_id)
                .debug_selector(move || toggle_key)
                .group(group.clone())
                .relative()
                .track_focus(&focus)
                .role(Role::Button)
                .aria_label(toggle_name(collapsed, &label, cx))
                .aria_expanded(!collapsed)
                .aria_controls([content_name.clone()])
                .cursor_pointer()
                .flex()
                .flex_none()
                .h(t.spacing.s5)
                .px(s4)
                .mx(-s4)
                .items_center();
            // A pointer click, and Enter or Space as a keyboard click (FR2).
            let toggle = toggle
                .on_click(move |_, window, cx| {
                    let next = !collapsed;
                    if !controlled {
                        state.update(cx, |c, _| *c = next);
                    }
                    if let Some(f) = &on_change {
                        f(next, window, cx);
                    }
                    window.refresh();
                })
                .children(summary)
                .children(handle)
                .children(ring);
            root = root.child(toggle);
        }

        let content_key = format!("{key}-content");
        let mounted = height.mounted();
        let content = div()
            .id(ElementId::Name(content_name.clone()))
            .debug_selector(move || content_key)
            // A named node, so the toggle's aria-controls resolves even while collapsed.
            .role(Role::Group)
            .aria_id(content_name)
            .flex()
            .flex_wrap()
            .items_start()
            .gap(t.spacing.s1)
            .when(items_opacity < 1., |el| el.opacity(items_opacity))
            .when(mounted, |el| el.children(self.children))
            // Collapsing, the items are inert.
            .inert(mounted && !shown);
        root.child(height.wrap_kept(content))
    }
}
