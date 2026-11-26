//! ChatToolCalls, upstream `Chat/ChatToolCalls.tsx`: the tools an assistant called, one
//! row each, folded into a group disclosure when there are several.

use std::rc::Rc;

use astryx_primitives::{control_focus, is_focus_visible};
use astryx_theme::{ThemeStyled as _, current};
use gpui::Refineable as _;
use gpui::prelude::*;
use gpui::{AnyElement, App, ElementId, SharedString, StyleRefinement, Window, div, px};

use crate::focus::focus_ring_inset;
use crate::icon::{Icon, IconSize, IconSource};
use crate::layout::VisuallyHidden;
use crate::status::{Badge, BadgeVariant, Spinner, SpinnerShade, SpinnerSize};
use crate::text::{Text, TextType};

/// Upstream `ChatToolCallStatus`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ChatToolCallStatus {
    Pending,
    Running,
    #[default]
    Complete,
    Error,
}

type Slot = Rc<dyn Fn(&mut Window, &mut App) -> AnyElement>;
type ExpandFn = Rc<dyn Fn(bool, &mut Window, &mut App)>;

/// Upstream `ChatToolCallItem`: one call. Element slots are builders, since a row can
/// render in both the collapsed header and the list.
#[derive(Clone, Default)]
pub struct ChatToolCallItem {
    name: SharedString,
    status: ChatToolCallStatus,
    target: Option<SharedString>,
    duration: Option<SharedString>,
    node: Option<SharedString>,
    additions: Option<i64>,
    deletions: Option<i64>,
    stats: Option<Slot>,
    error_message: Option<SharedString>,
    key: Option<SharedString>,
    result_detail: Option<Slot>,
}

impl ChatToolCallItem {
    /// Upstream `name`: the tool's name.
    pub fn new(name: impl Into<SharedString>) -> Self {
        ChatToolCallItem {
            name: name.into(),
            ..Default::default()
        }
    }

    astryx_primitives::setters! {
        /// Upstream `status`, `complete` by default.
        status: ChatToolCallStatus,
        /// Upstream `target`: what the tool acted on (a file, a query).
        ?target: SharedString,
        /// Upstream `duration`: shown once complete.
        ?duration: SharedString,
        /// Upstream `node`: a badge, such as the machine it ran on.
        ?node: SharedString,
        /// Upstream `additions`: `+n` in the success color.
        ?additions: i64,
        /// Upstream `deletions`: `-n` in the error color.
        ?deletions: i64,
        /// Upstream `errorMessage`: the failure, read with the status and as its title.
        ?error_message: SharedString,
        /// Upstream `key`: the row's identity; else built from its fields.
        ?key: SharedString,
    }

    /// Upstream `stats`: more statistics after the counts.
    pub fn stats(mut self, stats: impl Fn(&mut Window, &mut App) -> AnyElement + 'static) -> Self {
        self.stats = Some(Rc::new(stats));
        self
    }

    /// Upstream `resultDetail`: the row becomes a disclosure of this content.
    pub fn result_detail(
        mut self,
        detail: impl Fn(&mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        self.result_detail = Some(Rc::new(detail));
        self
    }

    /// Upstream `getToolCallKey`.
    fn row_key(&self) -> String {
        match &self.key {
            Some(key) => key.to_string(),
            None => [
                self.name.to_string(),
                format!("{:?}", self.status),
                self.target.clone().unwrap_or_default().to_string(),
                self.node.clone().unwrap_or_default().to_string(),
                self.duration.clone().unwrap_or_default().to_string(),
                self.additions.map(|n| n.to_string()).unwrap_or_default(),
                self.deletions.map(|n| n.to_string()).unwrap_or_default(),
                self.error_message.clone().unwrap_or_default().to_string(),
            ]
            .join("\u{1F}"),
        }
    }
}

/// Upstream `ChatToolCalls`.
///
/// ```ignore
/// ChatToolCalls::new("tools", vec![
///     ChatToolCallItem::new("search").target("astryx chat"),
///     ChatToolCallItem::new("edit_file").target("src/lib.rs").additions(12).deletions(3),
/// ])
/// ```
#[derive(IntoElement)]
pub struct ChatToolCalls {
    id: ElementId,
    calls: Vec<ChatToolCallItem>,
    label: Option<SharedString>,
    expanded: Option<bool>,
    default_expanded: bool,
    on_expanded_change: Option<ExpandFn>,
    style: StyleRefinement,
}

impl ChatToolCalls {
    /// Upstream `calls`, in order.
    pub fn new(
        id: impl Into<ElementId>,
        calls: impl IntoIterator<Item = ChatToolCallItem>,
    ) -> Self {
        ChatToolCalls {
            id: id.into(),
            calls: calls.into_iter().collect(),
            label: None,
            expanded: None,
            default_expanded: false,
            on_expanded_change: None,
            style: StyleRefinement::default(),
        }
    }

    astryx_primitives::setters! {
        /// Upstream `label`: replaces the expanded group's count (FR2).
        ?label: SharedString,
        /// Upstream `isExpanded`: controlled expansion.
        ?expanded: bool,
        /// Upstream `defaultIsExpanded`.
        !default_expanded,
    }

    /// Upstream `onExpandedChange`.
    pub fn on_expanded_change(mut self, f: impl Fn(bool, &mut Window, &mut App) + 'static) -> Self {
        self.on_expanded_change = Some(Rc::new(f));
        self
    }
}

impl Styled for ChatToolCalls {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

/// The status's announcement (upstream `getStatusAnnouncement`).
fn announcement(
    status: ChatToolCallStatus,
    error: Option<&SharedString>,
    cx: &App,
) -> SharedString {
    let i18n = astryx_i18n::current(cx);
    match (status, error) {
        (ChatToolCallStatus::Error, Some(message)) => i18n.format(
            "@astryx.chatToolCalls.error",
            &[("message", message.to_string().into())],
        ),
        (ChatToolCallStatus::Pending, _) => i18n.t("@astryx.chatToolCalls.status.pending"),
        (ChatToolCallStatus::Running, _) => i18n.t("@astryx.chatToolCalls.status.running"),
        (ChatToolCallStatus::Complete, _) => i18n.t("@astryx.chatToolCalls.status.complete"),
        (ChatToolCallStatus::Error, None) => i18n.t("@astryx.chatToolCalls.status.error"),
    }
}

/// The status mark: a spinner while pending or running, else its icon; its color; and
/// its announcement for screen readers.
fn status_icon(
    id: &str,
    status: ChatToolCallStatus,
    error: Option<&SharedString>,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let theme = current(window, cx);
    let t = theme.tokens();
    let color = match status {
        ChatToolCallStatus::Pending => t.color.text_disabled,
        ChatToolCallStatus::Running => t.color.accent,
        ChatToolCallStatus::Complete => t.color.icon_success,
        ChatToolCallStatus::Error => t.color.icon_error,
    };
    let mark = match status {
        ChatToolCallStatus::Pending | ChatToolCallStatus::Running => {
            Spinner::new(ElementId::Name(format!("{id}-spinner").into()))
                .size(SpinnerSize::Sm)
                .shade(SpinnerShade::Subtle)
                .into_any_element()
        }
        ChatToolCallStatus::Complete => Icon::new(IconSource::Named("success".into()))
            .size(IconSize::Xsm)
            .into_any_element(),
        ChatToolCallStatus::Error => Icon::new(IconSource::Named("error".into()))
            .size(IconSize::Xsm)
            .into_any_element(),
    };
    let el = div()
        .relative()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size(px(16.))
        .rounded_full()
        .text_color(color)
        .child(mark)
        .child(VisuallyHidden::new().child(announcement(status, error, cx)));
    match (status, error) {
        // Upstream's `title`: the error message.
        (ChatToolCallStatus::Error, Some(message)) => crate::overlay::with_tooltip(
            &ElementId::Name(format!("{id}-status").into()),
            Some(message.clone()),
            false,
            el,
        ),
        _ => el.into_any_element(),
    }
}

/// Upstream `cutText`: one line, cut with an ellipsis, its full text in a tooltip.
fn cut(text: SharedString, code: bool, window: &mut Window, cx: &mut App) -> AnyElement {
    let theme = current(window, cx);
    let t = theme.tokens();
    let mut el = Text::new(text)
        .text_type(TextType::Supporting)
        .max_lines(1usize);
    if code {
        el = el
            .font_stack(&t.font.family.code)
            .font_weight(t.font.weight.medium);
    }
    el.into_any_element()
}

/// The parts of a row after its status: name, node, target, counts, duration.
fn row_parts(
    id: &str,
    call: &ChatToolCallItem,
    window: &mut Window,
    cx: &mut App,
) -> Vec<AnyElement> {
    let theme = current(window, cx);
    let t = theme.tokens();
    let mut parts = vec![
        status_icon(id, call.status, call.error_message.as_ref(), window, cx),
        div()
            .flex_shrink(1.)
            .min_w(px(32.))
            .overflow_hidden()
            .child(cut(call.name.clone(), true, window, cx))
            .into_any_element(),
    ];
    if let Some(node) = &call.node {
        parts.push(
            div()
                .flex_none()
                .child(Badge::new(node.clone()).variant(BadgeVariant::Neutral))
                .into_any_element(),
        );
    }
    if let Some(target) = &call.target {
        parts.push(
            div()
                .flex_shrink(10.)
                .min_w_0()
                .overflow_hidden()
                .child(cut(target.clone(), false, window, cx))
                .into_any_element(),
        );
    }
    if call.additions.is_some() || call.deletions.is_some() || call.stats.is_some() {
        let stats = call.stats.as_ref().map(|f| f(window, cx));
        parts.push(
            div()
                .flex()
                .flex_none()
                .items_center()
                .gap(t.spacing.s1)
                .text_style(&t.text.supporting)
                .text_color(t.color.text_secondary)
                .children(call.additions.map(|n| {
                    div()
                        .text_color(t.color.text_success)
                        .child(format!("+{n}"))
                }))
                .children(
                    call.deletions
                        .map(|n| div().text_color(t.color.text_error).child(format!("-{n}"))),
                )
                .children(stats)
                .into_any_element(),
        );
    }
    if let (Some(duration), ChatToolCallStatus::Complete) = (&call.duration, call.status) {
        parts.push(
            div()
                .flex_none()
                .whitespace_nowrap()
                .text_style(&t.text.supporting)
                .text_color(t.color.text_secondary)
                .child(duration.clone())
                .into_any_element(),
        );
    }
    parts
}

/// The chevron of a disclosure, turned while open.
fn chevron(open: bool) -> AnyElement {
    let icon = Icon::new(IconSource::Named("chevronDown".into())).size(IconSize::Xsm);
    let icon = if open {
        icon.rotation(std::f32::consts::PI)
    } else {
        icon
    };
    div()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size(px(14.))
        .child(icon)
        .into_any_element()
}

/// Upstream `CallRow`: a call; with a result detail, a disclosure of it.
fn call_row(
    id: ElementId,
    call: &ChatToolCallItem,
    clipped_ring: bool,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let theme = current(window, cx);
    let t = theme.tokens();
    let rem = window.rem_size();
    let key = format!("{id}");
    let mut row = div()
        .flex()
        .items_center()
        .gap(t.spacing.s1_5)
        .min_h(px(24.))
        .py(t.spacing.s0_5)
        .min_w_0()
        .text_color(t.color.text_secondary)
        .children(row_parts(&key, call, window, cx));
    let Some(detail) = call.result_detail.clone() else {
        return row.into_any_element();
    };
    let open = window.use_keyed_state(ElementId::Name(format!("{key}-open").into()), cx, |_, _| {
        false
    });
    let is_open = *open.read(cx);
    let focus = control_focus(&id, window, cx);
    row = row.child(div().ml_auto().child(chevron(is_open)));
    let toggle = open.clone();
    let hover = t.color.overlay_hover;
    let ring = is_focus_visible(&focus, window).then(|| {
        if clipped_ring {
            focus_ring_inset(t, t.radius.element.to_pixels(rem), rem)
        } else {
            crate::focus::focus_ring(t, t.radius.element.to_pixels(rem), rem)
        }
    });
    let button = row
        .id(id.clone())
        .debug_selector(move || key.clone())
        .relative()
        .track_focus(&focus)
        .role(gpui::Role::Button)
        .aria_expanded(is_open)
        .cursor_pointer()
        .rounded(t.radius.element)
        .px(t.spacing.s1)
        .mx(-t.spacing.s1.to_pixels(rem))
        .hover(move |s| s.bg(hover))
        .on_click(move |_, window, cx| {
            toggle.update(cx, |o, _| *o = !*o);
            window.refresh();
        })
        .children(ring);
    div()
        .flex()
        .flex_col()
        .child(button)
        .children(is_open.then(|| {
            div()
                .pl(px(16.) + t.spacing.s1_5.to_pixels(rem))
                .pb(t.spacing.s2)
                .child(detail(window, cx))
        }))
        .into_any_element()
}

impl RenderOnce for ChatToolCalls {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        // FR1: no calls, no root.
        if self.calls.is_empty() {
            return div().into_any_element();
        }
        let theme = current(window, cx);
        let t = theme.tokens();
        let rem = window.rem_size();
        let mut root = div().flex().flex_col().max_w_full().mt(t.spacing.s2);
        theme.refine(root.style(), "chat-tool-calls", &[], None);
        root.style().refine(&self.style);
        if self.calls.len() == 1 {
            let id = ElementId::Name(format!("{}-call-0", self.id).into());
            return root
                .child(call_row(id, &self.calls[0], false, window, cx))
                .into_any_element();
        }

        // A group: collapsed, the latest call and the count; expanded, every call.
        let internal = window.use_keyed_state(
            ElementId::Name(format!("{}-expanded", self.id).into()),
            cx,
            |_, _| self.default_expanded,
        );
        let expanded = self.expanded.unwrap_or(*internal.read(cx));
        let focus = control_focus(&self.id, window, cx);
        let latest = self.calls.last().expect("two or more calls");
        let i18n = astryx_i18n::current(cx);
        let header_key = format!("{}-header", self.id);
        let mut header = div()
            .id(ElementId::Name(header_key.clone().into()))
            .debug_selector(move || header_key.clone())
            .relative()
            .track_focus(&focus)
            .role(gpui::Role::Button)
            .aria_expanded(expanded)
            .cursor_pointer()
            .flex()
            .items_center()
            .gap(t.spacing.s1_5)
            .min_h(px(24.))
            .py(t.spacing.s0_5)
            .text_color(t.color.text_secondary);
        if expanded {
            let label = self.label.clone().unwrap_or_else(|| {
                i18n.format(
                    "@astryx.chatToolCalls.groupLabel",
                    &[("count", (self.calls.len() as i64).into())],
                )
            });
            header = header
                .child(
                    div()
                        .flex()
                        .flex_none()
                        .items_center()
                        .justify_center()
                        .size(px(16.))
                        .child(Icon::new(IconSource::Named("wrench".into())).size(IconSize::Sm)),
                )
                .child(
                    div()
                        .text_style(&t.text.supporting)
                        .font_weight(t.font.weight.medium)
                        .child(label),
                );
        } else {
            let key = format!("{}-latest", self.id);
            header = header.children(row_parts(&key, latest, window, cx)).child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap(t.spacing.s0_5)
                    .text_style(&t.text.supporting)
                    .child(Icon::new(IconSource::Named("wrench".into())).size(IconSize::Xsm))
                    .child(self.calls.len().to_string()),
            );
        }
        let (on_change, controlled) = (self.on_expanded_change.clone(), self.expanded.is_some());
        let toggle_state = internal.clone();
        header = header
            .child(chevron(expanded))
            .on_click(move |_, window, cx| {
                let next = !expanded;
                if !controlled {
                    toggle_state.update(cx, |e, _| *e = next);
                }
                if let Some(f) = &on_change {
                    f(next, window, cx);
                }
                window.refresh();
            })
            .children(
                is_focus_visible(&focus, window)
                    .then(|| crate::focus::focus_ring(t, t.radius.element.to_pixels(rem), rem)),
            );
        // FR3: collapsed, the calls are not rendered, so none takes focus.
        let list = expanded.then(|| {
            let mut list = div().flex().flex_col();
            for call in &self.calls {
                let id = ElementId::Name(format!("{}-call-{}", self.id, call.row_key()).into());
                list = list.child(call_row(id, call, true, window, cx));
            }
            list
        });
        root.child(header).children(list).into_any_element()
    }
}
