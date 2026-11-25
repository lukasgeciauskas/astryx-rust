//! Feedback: ProgressBar and Banner. Upstream: `ProgressBar/`, `Banner/`.

pub mod examples;

use std::rc::Rc;
use std::time::Duration;

use astryx_i18n::{LogicalStyled, current as i18n, use_direction};
use astryx_primitives::{ElementSize, prefers_reduced_motion};
use astryx_theme::{ThemeStyled, Tokens, current};
use gpui::{
    AnyElement, App, ElementId, Hsla, IntoElement, ParentElement, RenderOnce, Role, SharedString,
    StyleRefinement, Styled, Window, div, prelude::*, px, relative,
};
use smallvec::SmallVec;

use crate::button::{ButtonVariant, IconButton};
use crate::containers::Elevation;
use crate::doc::{ComponentDoc, PropMapping, RustApi};
use crate::icon::{Icon, IconSize};
use crate::layout::VisuallyHidden;
use crate::motion::elapsed;

// ----------------------------------------------------------------------------
// ProgressBar

/// Upstream `ProgressBar` `variant`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ProgressVariant {
    #[default]
    Accent,
    Success,
    Warning,
    Error,
    Neutral,
}

impl ProgressVariant {
    /// (fill, mark on the fill) colors.
    fn colors(self, t: &Tokens) -> (Hsla, Hsla) {
        let c = &t.color;
        match self {
            ProgressVariant::Accent => (c.accent, c.on_accent),
            ProgressVariant::Success => (c.text_success, c.background_surface),
            ProgressVariant::Warning => (c.text_warning, c.background_surface),
            ProgressVariant::Error => (c.text_error, c.background_surface),
            ProgressVariant::Neutral => (c.text_secondary, c.background_surface),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            ProgressVariant::Accent => "accent",
            ProgressVariant::Success => "success",
            ProgressVariant::Warning => "warning",
            ProgressVariant::Error => "error",
            ProgressVariant::Neutral => "neutral",
        }
    }
}

/// A target drawn on the track, upstream `marks` item.
#[derive(Clone, Debug, PartialEq)]
pub struct ProgressMark {
    pub value: f64,
    pub label: SharedString,
}

type ValueFormatter = Rc<dyn Fn(f64, f64) -> String>;

/// Determinate or indeterminate progress, upstream `ProgressBar`.
#[derive(IntoElement)]
pub struct ProgressBar {
    id: ElementId,
    label: SharedString,
    value: f64,
    max: f64,
    label_hidden: bool,
    value_label: bool,
    format_value_label: Option<ValueFormatter>,
    variant: ProgressVariant,
    indeterminate: bool,
    marks: Vec<ProgressMark>,
    disabled: bool,
    style: StyleRefinement,
}

impl ProgressBar {
    /// Upstream `label`: the accessible name, shown above the track.
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        ProgressBar {
            id: id.into(),
            label: label.into(),
            value: 0.,
            max: 100.,
            label_hidden: false,
            value_label: false,
            format_value_label: None,
            variant: ProgressVariant::Accent,
            indeterminate: false,
            marks: Vec::new(),
            disabled: false,
            style: StyleRefinement::default(),
        }
    }

    astryx_primitives::setters! {
        /// Upstream `value`.
        value: f64,
        /// Upstream `max`.
        max: f64,
        /// Upstream `isLabelHidden`: the label stays the accessible name.
        !label_hidden,
        /// Upstream `hasValueLabel`: shows the value, a percentage by default.
        !value_label,
        /// Upstream `variant`.
        variant: ProgressVariant,
        /// Upstream `isIndeterminate`: progress of unknown length.
        !indeterminate,
        /// Upstream `isDisabled`.
        !disabled,
    }

    /// Upstream `formatValueLabel`: `(value, max) -> text`.
    pub fn format_value_label(mut self, format: impl Fn(f64, f64) -> String + 'static) -> Self {
        self.format_value_label = Some(Rc::new(format));
        self
    }

    /// Upstream `marks`: fixed targets on the track.
    pub fn marks(mut self, marks: impl IntoIterator<Item = ProgressMark>) -> Self {
        self.marks = marks.into_iter().collect();
        self
    }

    /// The fill as a fraction of the track, clamped.
    pub fn fraction(&self) -> f64 {
        if self.max > 0. {
            (self.value.clamp(0., self.max) / self.max).clamp(0., 1.)
        } else {
            0.
        }
    }
}

impl Styled for ProgressBar {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

/// Upstream's indeterminate slide: a 40% bar from -100% to 250% of its own
/// width over 1.5 s, eased in and out.
fn slide_offset(elapsed: Duration) -> f32 {
    let t = (elapsed.as_secs_f32() / 1.5).fract();
    // CSS `ease-in-out`: cubic-bezier(0.42, 0, 0.58, 1).
    let eased = astryx_theme::CubicBezier(0.42, 0., 0.58, 1.).ease(t);
    -1. + 3.5 * eased
}

impl RenderOnce for ProgressBar {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let dir = use_direction(cx);
        let i18n = i18n(cx);
        let fraction = self.fraction();
        let (mut fill_color, mark_on_fill) = self.variant.colors(t);
        if self.disabled {
            fill_color = t.color.text_disabled;
        }
        let value_text = match &self.format_value_label {
            Some(format) => format(self.value.clamp(0., self.max), self.max),
            None => i18n.percent((fraction * 100.).round()),
        };
        let text_color = |primary: Hsla| {
            if self.disabled {
                t.color.text_disabled
            } else {
                primary
            }
        };

        let header = (!self.label_hidden || (self.value_label && !self.indeterminate)).then(|| {
            div()
                .flex_row_logical(dir)
                .justify_between()
                .gap(t.spacing.s2)
                .when(!self.label_hidden, |row| {
                    row.child(
                        div()
                            .text_style(&t.text.body)
                            .font_weight(t.font.weight.medium)
                            .text_color(text_color(t.color.text_primary))
                            .child(self.label.clone()),
                    )
                })
                .when(self.value_label && !self.indeterminate, |row| {
                    row.child(
                        div()
                            .text_style(&t.text.body)
                            .text_color(text_color(t.color.text_secondary))
                            .child(value_text.clone()),
                    )
                })
        });

        // Themes re-define tokens for the part (neutral's track color).
        let part = theme.for_part("progress-bar", &[("variant", self.variant.as_str())]);
        let track_color = part.as_ref().map_or(t.color.background_muted, |p| {
            p.tokens().color.background_muted
        });
        let mut track = div()
            .relative()
            .w_full()
            .h(px(8.))
            .rounded(t.radius.full)
            .bg(track_color)
            .overflow_hidden();
        theme.refine(
            track.style(),
            "progress-bar",
            &[("variant", self.variant.as_str())],
            None,
        );

        let fill = if self.indeterminate {
            if prefers_reduced_motion(cx) {
                // Pulses in place, full width.
                let phase = (elapsed(cx).as_secs_f32() / 1.5).fract();
                let opacity = 1. - 0.6 * (1. - (phase * 2. - 1.).abs());
                window.request_animation_frame();
                div()
                    .size_full()
                    .rounded(t.radius.full)
                    .bg(fill_color)
                    .opacity(opacity)
            } else {
                let offset = slide_offset(elapsed(cx));
                window.request_animation_frame();
                let offset = if dir.is_rtl() { -offset } else { offset };
                div()
                    .absolute()
                    .top_0()
                    .h_full()
                    .w(relative(0.4))
                    .inset_start(dir, px(0.))
                    .rounded(t.radius.full)
                    .bg(fill_color)
                    .translate_x(relative(offset))
            }
        } else {
            div()
                .absolute()
                .top_0()
                .h_full()
                .w(relative(fraction as f32))
                .inset_start(dir, px(0.))
                .rounded(t.radius.full)
                .bg(fill_color)
        };

        let mark_id = self.id.clone();
        let marks = self.marks.iter().enumerate().map(|(i, mark)| {
            let at = if self.max > 0. {
                (mark.value / self.max).clamp(0., 1.)
            } else {
                0.
            };
            let on_fill = !self.indeterminate && at <= fraction;
            let tick = div()
                .absolute()
                .top(relative(0.5))
                .inset_start(dir, relative(at as f32))
                .w(px(2.))
                .h(px(8.))
                .translate(
                    relative(if dir.is_rtl() { 0.5 } else { -0.5 }),
                    relative(-0.5),
                )
                .bg(if on_fill {
                    mark_on_fill
                } else {
                    t.color.text_secondary
                })
                .id(ElementId::NamedInteger(
                    format!("{mark_id}-mark").into(),
                    i as u64,
                ));
            // Upstream: each mark names itself in a tooltip.
            crate::overlay::with_tooltip(
                &ElementId::NamedInteger(format!("{mark_id}-mark-tip").into(), i as u64),
                Some(mark.label.clone()),
                false,
                tick,
            )
        });

        let mut el = div().flex().flex_col().gap(t.spacing.s1).w_full();
        el.style().refine(&self.style);
        let mut root = el
            .id(self.id)
            .role(Role::ProgressIndicator)
            .aria_label(self.label.clone())
            .aria_min_numeric_value(0.)
            .aria_max_numeric_value(self.max);
        if !self.indeterminate {
            root = root
                .aria_numeric_value(self.value.clamp(0., self.max))
                .aria_value(value_text);
        }
        root.children(header)
            .when(self.label_hidden, |el| {
                el.child(VisuallyHidden::new().child(self.label.clone()))
            })
            .child(track.child(fill).children(marks))
    }
}

// ----------------------------------------------------------------------------
// Banner

/// Upstream `Banner` `status`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BannerStatus {
    Info,
    Warning,
    Error,
    Success,
}

impl BannerStatus {
    fn colors(self, t: &Tokens) -> (Hsla, Hsla) {
        let c = &t.color;
        match self {
            BannerStatus::Info => (c.info_muted, c.on_info_muted),
            BannerStatus::Warning => (c.warning_muted, c.on_warning_muted),
            BannerStatus::Error => (c.error_muted, c.on_error_muted),
            BannerStatus::Success => (c.success_muted, c.on_success_muted),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            BannerStatus::Info => "info",
            BannerStatus::Warning => "warning",
            BannerStatus::Error => "error",
            BannerStatus::Success => "success",
        }
    }
}

/// Upstream `container`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BannerContainer {
    /// Rounded, for a banner inside content.
    #[default]
    Card,
    /// Square and full width, for a page-level banner.
    Section,
}

/// Upstream `collapsible`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Collapsible {
    /// The children always show, with no toggle.
    No,
    /// A toggle, closed at first (`true`) or open (`{defaultIsOpen: true}`).
    Uncontrolled { default_open: bool },
    /// A toggle whose state the app owns (`{isOpen, onOpenChange}`).
    Controlled { open: bool },
}

type Callback = Rc<dyn Fn(&mut Window, &mut App)>;
type OpenChange = Rc<dyn Fn(bool, &mut Window, &mut App)>;

#[derive(Default)]
struct BannerState {
    dismissed: bool,
    open: Option<bool>,
}

/// A persistent message for errors, updates, warnings or confirmations,
/// upstream `Banner`.
#[derive(IntoElement)]
pub struct Banner {
    id: ElementId,
    status: BannerStatus,
    title: SharedString,
    description: Option<SharedString>,
    icon: Option<AnyElement>,
    dismissable: bool,
    on_dismiss: Option<Callback>,
    dismiss_label: Option<SharedString>,
    end_content: Option<AnyElement>,
    container: BannerContainer,
    elevation: Elevation,
    collapsible: Collapsible,
    on_open_change: Option<OpenChange>,
    style: StyleRefinement,
    children: SmallVec<[AnyElement; 2]>,
}

impl Banner {
    /// Upstream `status` and `title`.
    pub fn new(
        id: impl Into<ElementId>,
        status: BannerStatus,
        title: impl Into<SharedString>,
    ) -> Self {
        Banner {
            id: id.into(),
            status,
            title: title.into(),
            description: None,
            icon: None,
            dismissable: false,
            on_dismiss: None,
            dismiss_label: None,
            end_content: None,
            container: BannerContainer::Card,
            elevation: Elevation::None,
            collapsible: Collapsible::Uncontrolled {
                default_open: false,
            },
            on_open_change: None,
            style: StyleRefinement::default(),
            children: SmallVec::new(),
        }
    }

    astryx_primitives::setters! {
        /// Upstream `description`.
        ?description: SharedString,
        /// Upstream `isDismissable`.
        !dismissable,
        /// Upstream `dismissLabel`: the dismiss button's name.
        ?dismiss_label: SharedString,
        /// Upstream `container`.
        container: BannerContainer,
        /// Upstream `elevation`.
        elevation: Elevation,
        /// Upstream `collapsible`.
        collapsible: Collapsible,
    }

    /// Upstream `icon`: replaces the status icon.
    pub fn icon(mut self, icon: impl IntoElement) -> Self {
        self.icon = Some(icon.into_any_element());
        self
    }

    /// Upstream `onDismiss`. The banner hides itself either way.
    pub fn on_dismiss(mut self, f: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_dismiss = Some(Rc::new(f));
        self
    }

    /// Upstream `endContent`: an action at the header's end.
    pub fn end_content(mut self, content: impl IntoElement) -> Self {
        self.end_content = Some(content.into_any_element());
        self
    }

    /// Upstream `collapsible.onOpenChange`.
    pub fn on_open_change(mut self, f: impl Fn(bool, &mut Window, &mut App) + 'static) -> Self {
        self.on_open_change = Some(Rc::new(f));
        self
    }
}

impl Styled for Banner {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl ParentElement for Banner {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for Banner {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let dir = use_direction(cx);
        let i18n = i18n(cx);
        let state = window.use_keyed_state(self.id.clone(), cx, |_, _| BannerState::default());
        if state.read(cx).dismissed {
            return div().into_any_element();
        }
        let has_children = !self.children.is_empty();
        let open = match self.collapsible {
            Collapsible::No => true,
            Collapsible::Controlled { open } => open,
            Collapsible::Uncontrolled { default_open } => {
                state.read(cx).open.unwrap_or(default_open)
            }
        };
        let has_toggle = has_children && self.collapsible != Collapsible::No;
        let card = self.container == BannerContainer::Card;
        let container = if card { "card" } else { "section" };
        // Themes re-define each status's muted colors for the part.
        let part = theme.for_part(
            "banner",
            &[("container", container), ("status", self.status.as_str())],
        );
        let (bg, fg) = self.status.colors(part.as_ref().map_or(t, |p| p.tokens()));
        let radius = if card {
            t.radius.container
        } else {
            px(0.).into()
        };

        let status_icon = self.icon.unwrap_or_else(|| {
            Icon::new(match self.status {
                BannerStatus::Info => "info",
                BannerStatus::Warning => "warning",
                BannerStatus::Error => "error",
                BannerStatus::Success => "success",
            })
            .size(IconSize::Md)
            .into_any_element()
        });

        let dismiss = i18n.t("@astryx.banner.dismiss");
        let dismiss_name = self.dismiss_label.clone().unwrap_or_else(|| {
            i18n.format(
                "@astryx.banner.dismissTitled",
                &[
                    ("dismiss", dismiss.to_string().into()),
                    ("title", self.title.to_string().into()),
                ],
            )
        });

        let toggle = has_toggle.then(|| {
            let label = if open {
                i18n.t("@astryx.banner.collapse")
            } else {
                i18n.t("@astryx.banner.expand")
            };
            let state = state.clone();
            let controlled = matches!(self.collapsible, Collapsible::Controlled { .. });
            let on_open_change = self.on_open_change.clone();
            let chevron = Icon::new("chevronDown").size(IconSize::Sm);
            let chevron = if open {
                chevron.scale_xy(1., -1.)
            } else {
                chevron
            };
            IconButton::new(
                ElementId::Name(format!("{:?}-toggle", self.id).into()),
                label.clone(),
                chevron,
            )
            .variant(ButtonVariant::Ghost)
            .size(ElementSize::Sm)
            .tooltip(label)
            .expanded(open)
            .on_click(move |_, window, cx| {
                if !controlled {
                    state.update(cx, |s, _| s.open = Some(!open));
                }
                if let Some(f) = &on_open_change {
                    f(!open, window, cx);
                }
                window.refresh();
            })
            // Upstream re-defines text-primary as the status's on-muted color.
            .text_color(fg)
        });
        let dismiss_button = self.dismissable.then(|| {
            let state = state.clone();
            let on_dismiss = self.on_dismiss.clone();
            let mut button = IconButton::new(
                ElementId::Name(format!("{:?}-dismiss", self.id).into()),
                dismiss_name,
                Icon::new("close").size(IconSize::Sm),
            )
            .variant(ButtonVariant::Ghost)
            .size(ElementSize::Sm)
            .on_click(move |_, window, cx| {
                state.update(cx, |s, _| s.dismissed = true);
                if let Some(f) = &on_dismiss {
                    f(window, cx);
                }
                window.refresh();
            })
            .text_color(fg);
            theme.refine(button.style(), "banner-close", &[], None);
            button
        });

        let mut header = div()
            .flex_row_logical(dir)
            .items_start()
            .gap(t.spacing.s2)
            .py(t.spacing.s3)
            .px(t.spacing.s4)
            .bg(bg)
            .text_color(fg);
        header = if open && has_children {
            header.rounded_t(radius)
        } else {
            header.rounded(radius)
        };
        theme.refine(
            header.style(),
            "banner",
            &[
                ("container", if card { "card" } else { "section" }),
                ("status", self.status.as_str()),
            ],
            None,
        );
        let header = header
            .child(div().flex_none().pt(px(2.)).child(status_icon))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_grow(1.)
                    .min_w_0()
                    .child(
                        div()
                            .text_style(&t.text.label)
                            .font_weight(t.font.weight.semibold)
                            .text_color(fg)
                            .child(self.title.clone()),
                    )
                    .children(self.description.map(|d| {
                        div()
                            .text_style(&t.text.supporting)
                            .text_color(fg.opacity(0.85))
                            .child(d)
                    })),
            )
            .children(
                (self.end_content.is_some() || dismiss_button.is_some() || toggle.is_some()).then(
                    || {
                        div()
                            .flex()
                            .flex_none()
                            .items_center()
                            .gap(t.spacing.s2)
                            .children(self.end_content)
                            .children(toggle)
                            .children(dismiss_button)
                    },
                ),
            );

        let body = (has_children && open).then(|| {
            div()
                .bg(t.color.background_card)
                .py(t.spacing.s3)
                .px(t.spacing.s4)
                .border_l_1()
                .border_r_1()
                .border_b_1()
                .border_color(t.color.border)
                .rounded_b(radius)
                .text_color(t.color.text_primary)
                .children(self.children)
        });

        let mut frame = div().flex().flex_col().w_full();
        frame = self.elevation.apply(frame.rounded(radius), t);
        theme.refine(
            frame.style(),
            "banner-frame",
            &[
                ("container", if card { "card" } else { "section" }),
                ("elevation", self.elevation.as_str()),
            ],
            None,
        );
        frame.style().refine(&self.style);
        frame
            .id(self.id)
            .role(Role::Status)
            .aria_label(self.title)
            .child(header)
            .children(body)
            .into_any_element()
    }
}

// ----------------------------------------------------------------------------
// Docs

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

pub const PROGRESS_BAR_DOC: ComponentDoc = ComponentDoc {
    name: "ProgressBar",
    rust_type: "astryx::components::feedback::ProgressBar",
    props: &[
        p("label", RustApi::Argument("label")),
        p("value", RustApi::Method("value")),
        p("max", RustApi::Method("max")),
        p("isLabelHidden", RustApi::Method("label_hidden")),
        p("hasValueLabel", RustApi::Method("value_label")),
        p("formatValueLabel", RustApi::Method("format_value_label")),
        p("variant", RustApi::Method("variant")),
        p("isIndeterminate", RustApi::Method("indeterminate")),
        p("marks", RustApi::Method("marks")),
        p("isDisabled", RustApi::Method("disabled")),
        p("xstyle", RustApi::Trait("Styled")),
    ],
    example: include_str!("examples.rs"),
};

pub const BANNER_DOC: ComponentDoc = ComponentDoc {
    name: "Banner",
    rust_type: "astryx::components::feedback::Banner",
    props: &[
        p("status", RustApi::Argument("status")),
        p("title", RustApi::Argument("title")),
        p("description", RustApi::Method("description")),
        p("icon", RustApi::Method("icon")),
        p("isDismissable", RustApi::Method("dismissable")),
        p("onDismiss", RustApi::Method("on_dismiss")),
        p("dismissLabel", RustApi::Method("dismiss_label")),
        p("endContent", RustApi::Method("end_content")),
        p("container", RustApi::Method("container")),
        p("elevation", RustApi::Method("elevation")),
        p("children", RustApi::Trait("ParentElement")),
        p(
            "collapsible",
            RustApi::Method("collapsible (+ on_open_change)"),
        ),
        p("xstyle", RustApi::Trait("Styled")),
    ],
    example: include_str!("examples.rs"),
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_fraction_clamps() {
        let bar = |value, max| ProgressBar::new("p", "Upload").value(value).max(max);
        assert_eq!(bar(50., 100.).fraction(), 0.5);
        assert_eq!(bar(150., 100.).fraction(), 1.);
        assert_eq!(bar(-5., 100.).fraction(), 0.);
        assert_eq!(bar(5., 0.).fraction(), 0.);
    }

    #[test]
    fn indeterminate_slide_spans_upstreams_keyframes() {
        assert!((slide_offset(Duration::ZERO) + 1.).abs() < 1e-3);
        let end = slide_offset(Duration::from_millis(1499));
        assert!((end - 2.5).abs() < 0.01, "{end}");
    }
}
