//! Stepper and Step. Upstream: `Stepper/`.
//!
//! The separated layout (each step's bar above or beside its label row), in
//! both orientations, with clickable steps. Upstream's on-track indicators
//! and the collapsed horizontal mode for narrow widths are not ported yet.

#[cfg(test)]
mod tests;

use std::rc::Rc;

use astryx_i18n::{LogicalStyled, current as i18n, use_direction};
use astryx_primitives::{control_focus, is_focus_visible};
use astryx_theme::{Tokens, current};
use gpui::{
    AnyElement, App, ElementId, Hsla, IntoElement, ParentElement, RenderOnce, Role, SharedString,
    StyleRefinement, Styled, Window, div, prelude::*, px, relative, svg,
};
use smallvec::SmallVec;

use crate::doc::{ComponentDoc, PropMapping, RustApi};
use crate::focus::focus_ring;
use crate::icon::{Icon, IconColor, IconSize};
use crate::item::ItemDensity;
use crate::layout::Orientation;

/// Upstream Step `status`: the step's semantic color.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepStatus {
    Accent,
    Success,
    Warning,
    Error,
}

/// Upstream Step `indicator`.
#[derive(Default)]
pub enum StepIndicator {
    /// A number until completed, then a check; the active step a ring.
    #[default]
    Auto,
    /// Always the number.
    Number,
    /// No indicator.
    None,
    /// Any element.
    Custom(AnyElement),
}

/// Where a step stands against the active one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StepState {
    Completed,
    Active,
    NotStarted,
}

pub(crate) fn progress(step: usize, active: usize) -> StepState {
    match step.cmp(&active) {
        std::cmp::Ordering::Less => StepState::Completed,
        std::cmp::Ordering::Equal => StepState::Active,
        std::cmp::Ordering::Greater => StepState::NotStarted,
    }
}

/// One step of a [`Stepper`], upstream `Step`.
pub struct Step {
    label: SharedString,
    description: Option<SharedString>,
    status: Option<StepStatus>,
    indicator: StepIndicator,
    disabled: bool,
    optional: bool,
    end_content: Option<AnyElement>,
    density: Option<ItemDensity>,
    children: SmallVec<[AnyElement; 1]>,
}

impl Step {
    /// Upstream `label`. Its index is its place in the stepper.
    pub fn new(label: impl Into<SharedString>) -> Self {
        Step {
            label: label.into(),
            description: None,
            status: None,
            indicator: StepIndicator::Auto,
            disabled: false,
            optional: false,
            end_content: None,
            density: None,
            children: SmallVec::new(),
        }
    }

    astryx_primitives::setters! {
        /// Upstream `description`.
        ?description: SharedString,
        /// Upstream `status`.
        ?status: StepStatus,
        /// Upstream `indicator`.
        indicator: StepIndicator,
        /// Upstream `isDisabled`.
        !disabled,
        /// Upstream `isOptional`: "∙ Optional" after the label.
        !optional,
        /// Upstream `density`: else the stepper's.
        ?density: ItemDensity,
    }

    /// Upstream `endContent`: at the end of the label row.
    pub fn end_content(mut self, content: impl IntoElement) -> Self {
        self.end_content = Some(content.into_any_element());
        self
    }
}

impl ParentElement for Step {
    /// Upstream `children`: content under the label (vertical steppers).
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

/// The color a reached step takes for its status.
fn status_color(status: StepStatus, t: &Tokens) -> (Hsla, Hsla) {
    let c = &t.color;
    match status {
        StepStatus::Accent => (c.accent, c.on_accent),
        StepStatus::Success => (c.success, c.on_success),
        StepStatus::Warning => (c.warning, c.on_warning),
        StepStatus::Error => (c.error, c.on_error),
    }
}

/// Upstream's check, drawn in a 10-unit box.
const CHECK_SVG: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10"><path d="M8.5 2.5L4 7.5L1.5 5" stroke="currentColor" stroke-width="1.5" fill="none" stroke-linecap="round" stroke-linejoin="round"/></svg>"#;

type ClickFn = Rc<dyn Fn(usize, &mut Window, &mut App)>;

/// A numbered sequence of steps, upstream `Stepper`.
///
/// ```ignore
/// Stepper::new("checkout", 1)
///     .step(Step::new("Cart"))
///     .step(Step::new("Shipping"))
///     .step(Step::new("Payment"))
/// ```
#[derive(IntoElement)]
pub struct Stepper {
    id: ElementId,
    active_step: usize,
    steps: Vec<Step>,
    orientation: Orientation,
    on_step_click: Option<ClickFn>,
    label: Option<SharedString>,
    density: ItemDensity,
    style: StyleRefinement,
}

impl Stepper {
    /// Upstream `activeStep`: zero-based.
    pub fn new(id: impl Into<ElementId>, active_step: usize) -> Self {
        Stepper {
            id: id.into(),
            active_step,
            steps: Vec::new(),
            orientation: Orientation::Horizontal,
            on_step_click: None,
            label: None,
            density: ItemDensity::Balanced,
            style: StyleRefinement::default(),
        }
    }

    astryx_primitives::setters! {
        /// Upstream `orientation`.
        orientation: Orientation,
        /// Upstream `label`: "Progress" by default.
        ?label: SharedString,
        /// Upstream `density`.
        density: ItemDensity,
    }

    /// Upstream `onStepClick`: steps become buttons.
    pub fn on_step_click(mut self, f: impl Fn(usize, &mut Window, &mut App) + 'static) -> Self {
        self.on_step_click = Some(Rc::new(f));
        self
    }

    /// Upstream `children`: one step.
    pub fn step(mut self, step: Step) -> Self {
        self.steps.push(step);
        self
    }

    /// Upstream `children`: steps.
    pub fn steps(mut self, steps: impl IntoIterator<Item = Step>) -> Self {
        self.steps.extend(steps);
        self
    }
}

impl Styled for Stepper {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Stepper {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let rem = window.rem_size();
        let dir = use_direction(cx);
        let i18n = i18n(cx);
        let vertical = self.orientation == Orientation::Vertical;
        let bar = t.spacing.s1;
        let indicator_size = t.spacing.s4;
        let gutter = t.spacing.s6;
        let id = self.id.clone();

        let steps = self
            .steps
            .into_iter()
            .enumerate()
            .map(|(i, step)| {
                let progress = progress(i, self.active_step);
                let reached = progress != StepState::NotStarted;
                let c = &t.color;

                // The indicator.
                let indicator: Option<AnyElement> = match step.indicator {
                    StepIndicator::None => None,
                    StepIndicator::Custom(el) => {
                        let color = if step.disabled {
                            c.icon_disabled
                        } else {
                            match step.status {
                                Some(status) => status_color(status, t).0,
                                None if reached => c.accent,
                                None => c.icon_secondary,
                            }
                        };
                        Some(
                            div()
                                .flex()
                                .flex_none()
                                .items_center()
                                .justify_center()
                                .size(indicator_size)
                                .text_color(color)
                                .child(el)
                                .into_any_element(),
                        )
                    }
                    mode => {
                        let glyph = match step.status {
                            Some(
                                s @ (StepStatus::Success | StepStatus::Warning | StepStatus::Error),
                            ) if matches!(mode, StepIndicator::Auto)
                                && progress != StepState::Active =>
                            {
                                Some(s)
                            }
                            _ => None,
                        };
                        let number = glyph.is_none()
                            && (matches!(mode, StepIndicator::Number)
                                || progress == StepState::NotStarted);
                        let el = if let Some(glyph) = glyph {
                            let (icon, color) = match glyph {
                                StepStatus::Success => ("success", IconColor::Success),
                                StepStatus::Warning => ("warning", IconColor::Warning),
                                _ => ("error", IconColor::Error),
                            };
                            let color = if step.disabled {
                                IconColor::Disabled
                            } else {
                                color
                            };
                            Icon::new(icon)
                                .size(IconSize::Sm)
                                .color(color)
                                .into_any_element()
                        } else if number {
                            let (bg, fg) = if step.disabled {
                                (c.background_muted, c.text_disabled)
                            } else if reached {
                                match step.status {
                                    Some(status) => status_color(status, t),
                                    None => (c.accent, c.background_surface),
                                }
                            } else {
                                (c.background_muted, c.text_secondary)
                            };
                            div()
                                .flex()
                                .items_center()
                                .justify_center()
                                .size(indicator_size)
                                .rounded_full()
                                .bg(bg)
                                .text_color(fg)
                                .text_size(t.font.size.xs)
                                .font_weight(t.font.weight.semibold)
                                .child((i + 1).to_string())
                                .into_any_element()
                        } else {
                            let color = if step.disabled {
                                c.icon_disabled
                            } else {
                                c.accent
                            };
                            if progress == StepState::Completed {
                                // A filled circle around a check in the surface color.
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .size(indicator_size)
                                    .rounded_full()
                                    .bg(color)
                                    .child(
                                        svg()
                                            .data(CHECK_SVG)
                                            .size(px(10.))
                                            .text_color(c.background_surface),
                                    )
                                    .into_any_element()
                            } else {
                                // The active step: a filled dot in a ring.
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .size(indicator_size)
                                    .rounded_full()
                                    .border_2()
                                    .border_color(color)
                                    .child(div().size(px(8.)).rounded_full().bg(color))
                                    .into_any_element()
                            }
                        };
                        let mut wrap = div()
                            .flex()
                            .flex_none()
                            .items_center()
                            .justify_center()
                            .size(indicator_size);
                        if step.disabled && !number {
                            wrap = wrap.opacity(0.5);
                        }
                        Some(wrap.child(el).into_any_element())
                    }
                };
                let has_indicator = indicator.is_some();

                let label_color = if step.disabled {
                    c.text_disabled
                } else if progress == StepState::NotStarted {
                    c.text_secondary
                } else {
                    c.text_primary
                };
                let optional = step.optional.then(|| {
                    div()
                        .flex_none()
                        .text_color(c.text_secondary)
                        .child(format!("∙ {}", i18n.t("@astryx.step.optional")))
                });
                let label_row = div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(t.spacing.s2)
                    .min_w_0()
                    .children(indicator)
                    .child(
                        div()
                            .min_w_0()
                            .overflow_hidden()
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .text_size(t.text.body.size)
                            .line_height(relative(t.text.body.leading))
                            .font_weight(if progress == StepState::Active {
                                t.font.weight.semibold
                            } else {
                                t.font.weight.normal
                            })
                            .text_color(label_color)
                            .child(step.label.clone()),
                    )
                    .children(optional)
                    .children(step.end_content.map(|c| {
                        div()
                            .flex_none()
                            .margin_start(dir, gpui::Length::Auto)
                            .child(c)
                    }));
                let indent = if has_indicator { gutter } else { px(0.).into() };
                let description = step.description.clone().map(|d| {
                    div()
                        .padding_start(dir, indent)
                        .text_size(t.text.supporting.size)
                        .line_height(t.spacing.s4)
                        .text_color(c.text_secondary)
                        .child(d)
                });
                let content = (!step.children.is_empty()).then(|| {
                    div()
                        .pt(t.spacing.s2)
                        .padding_start(dir, indent)
                        .children(step.children)
                });
                let density = step.density.unwrap_or(self.density);
                let pad = match density {
                    ItemDensity::Compact => t.spacing.s1,
                    ItemDensity::Balanced => t.spacing.s2,
                    ItemDensity::Spacious => t.spacing.s3,
                };
                let mut body = div()
                    .relative()
                    .flex()
                    .flex_col()
                    .min_w_0()
                    .py(pad)
                    .rounded(t.radius.element)
                    .child(label_row)
                    .children(description)
                    .children(content);
                if vertical {
                    body = body.flex_1();
                }
                theme.refine(
                    body.style(),
                    "step",
                    &[(
                        "progress",
                        match progress {
                            StepState::Completed => "completed",
                            StepState::Active => "in-progress",
                            StepState::NotStarted => "not-started",
                        },
                    )],
                    None,
                );

                // The step's bar: filled once the step is reached.
                let bar_color = if reached { c.accent } else { c.border };
                let bar_el = if vertical {
                    div()
                        .w(bar)
                        .flex_none()
                        .self_stretch()
                        .rounded_full()
                        .bg(bar_color)
                } else {
                    div()
                        .w_full()
                        .h(bar)
                        .flex_none()
                        .rounded_full()
                        .bg(bar_color)
                        .mb(t.spacing.s0_5)
                };

                let step_id = ElementId::Name(format!("{id:?}-step-{i}").into());
                let body = match (&self.on_step_click, step.disabled) {
                    (Some(on_click), false) => {
                        let focus = control_focus(&step_id, window, cx);
                        let on_click = on_click.clone();
                        let (hover, pressed) = (c.overlay_hover, c.overlay_pressed);
                        let name = match step.status {
                            Some(status)
                                if status != StepStatus::Accent
                                    || progress == StepState::Completed =>
                            {
                                let word = match (status, progress) {
                                    (StepStatus::Warning, _) => {
                                        i18n.t("@astryx.step.status.warning")
                                    }
                                    (StepStatus::Error, _) => i18n.t("@astryx.step.status.error"),
                                    _ => i18n.t("@astryx.step.status.completed"),
                                };
                                i18n.format(
                                    "@astryx.step.goToStepWithStatus",
                                    &[
                                        ("stepNumber", (i + 1).into()),
                                        ("label", step.label.to_string().into()),
                                        ("status", word.to_string().into()),
                                    ],
                                )
                            }
                            _ => i18n.format(
                                "@astryx.step.goToStep",
                                &[
                                    ("stepNumber", (i + 1).into()),
                                    ("label", step.label.to_string().into()),
                                ],
                            ),
                        };
                        let mut el = body
                            .id(step_id.clone())
                            .track_focus(&focus)
                            .cursor_pointer()
                            .hover(move |s| s.bg(hover))
                            .active(move |s| s.bg(pressed))
                            .on_click(move |_, window, cx| {
                                cx.stop_propagation();
                                on_click(i, window, cx);
                            });
                        if is_focus_visible(&focus, window) {
                            el = el.child(focus_ring(t, t.radius.element.to_pixels(rem), rem));
                        }
                        el.role(Role::Button).aria_label(name).into_any_element()
                    }
                    _ => body.into_any_element(),
                };
                let step_el = if vertical {
                    div()
                        .flex()
                        .flex_row()
                        .items_stretch()
                        .gap(t.spacing.s0_5)
                        .child(bar_el)
                        .child(body)
                } else {
                    div()
                        .flex()
                        .flex_col()
                        .items_stretch()
                        .flex_1()
                        .min_w_0()
                        .child(bar_el)
                        .child(body)
                };
                step_el.id(step_id).role(Role::ListItem)
            })
            .collect::<Vec<_>>();

        let mut el = div().flex().w_full().gap(t.spacing.s1);
        el = if vertical {
            el.flex_col()
        } else {
            el.flex_row().items_start()
        };
        theme.refine(
            el.style(),
            "stepper",
            &[("orientation", self.orientation.as_str())],
            None,
        );
        el.style().refine(&self.style);
        let name = self
            .label
            .unwrap_or_else(|| i18n.t("@astryx.stepper.label"));
        el.id(self.id)
            .role(Role::List)
            .aria_label(name)
            .children(steps)
    }
}

// ----------------------------------------------------------------------------
// Docs

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

pub const STEPPER_DOC: ComponentDoc = ComponentDoc {
    name: "Stepper",
    rust_type: "astryx::components::stepper::Stepper",
    props: &[
        p("ref", RustApi::Waived("GPUI elements have no refs")),
        p("activeStep", RustApi::Argument("active_step")),
        p("children", RustApi::Method("step / steps")),
        p("orientation", RustApi::Method("orientation")),
        p("onStepClick", RustApi::Method("on_step_click")),
        p("label", RustApi::Method("label")),
        p("density", RustApi::Method("density")),
        p(
            "indicatorPosition",
            RustApi::Waived("not yet: the on-track indicator layout is not ported"),
        ),
        p(
            "horizontalOptions",
            RustApi::Waived("not yet: the collapsed horizontal mode is not ported"),
        ),
    ],
    example: include_str!("../pagination/examples.rs"),
};

pub const STEP_DOC: ComponentDoc = ComponentDoc {
    name: "Step",
    rust_type: "astryx::components::stepper::Step",
    props: &[
        p(
            "step",
            RustApi::Waived("a step's index is its place in the stepper"),
        ),
        p("label", RustApi::Argument("label")),
        p("description", RustApi::Method("description")),
        p("children", RustApi::Trait("ParentElement")),
        p("status", RustApi::Method("status")),
        p("indicator", RustApi::Method("indicator")),
        p("isDisabled", RustApi::Method("disabled")),
        p("isOptional", RustApi::Method("optional")),
        p("endContent", RustApi::Method("end_content")),
        p("density", RustApi::Method("density")),
    ],
    example: include_str!("../pagination/examples.rs"),
};
