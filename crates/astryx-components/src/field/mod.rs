//! Field parts shared by form controls: FieldLabel, FieldStatus and the
//! Field container, and FormLayout. Upstream: `Field/`, `FieldStatus/`,
//! `FormLayout/`.

mod form_layout;

#[cfg(test)]
pub(crate) use form_layout::resolved;
pub use form_layout::{EntryErrorBehavior, FormDirection, FormLayout, Optionality};
pub(crate) use form_layout::{FormContext, form_context, resolve};

use astryx_i18n::current as i18n;
use astryx_primitives::{Politeness, announce};
use astryx_theme::{ThemeStyled, Tokens, current};
use gpui::{
    AnyElement, App, ElementId, Hsla, IntoElement, ParentElement, RenderOnce, SharedString,
    StyleRefinement, Styled, Window, div, prelude::*, relative,
};

use crate::doc::{ComponentDoc, PropMapping, RustApi};
use crate::icon::{Icon, IconDefaultSize, IconSize};
use crate::layout::VisuallyHidden;

/// Upstream `InputStatusType`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatusType {
    Error,
    Warning,
    Success,
}

impl StatusType {
    pub fn as_str(self) -> &'static str {
        match self {
            StatusType::Error => "error",
            StatusType::Warning => "warning",
            StatusType::Success => "success",
        }
    }

    /// (background, foreground): the status's muted pair.
    pub fn colors(self, t: &Tokens) -> (Hsla, Hsla) {
        let c = &t.color;
        match self {
            StatusType::Error => (c.error_muted, c.on_error_muted),
            StatusType::Warning => (c.warning_muted, c.on_warning_muted),
            StatusType::Success => (c.success_muted, c.on_success_muted),
        }
    }

    fn icon(self) -> &'static str {
        self.as_str()
    }
}

/// Upstream `InputStatus`: a field's validation state, `{type, message}`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InputStatus {
    pub status: StatusType,
    pub message: Option<SharedString>,
}

impl InputStatus {
    pub fn error(message: impl Into<SharedString>) -> Self {
        InputStatus {
            status: StatusType::Error,
            message: Some(message.into()),
        }
    }

    pub fn warning(message: impl Into<SharedString>) -> Self {
        InputStatus {
            status: StatusType::Warning,
            message: Some(message.into()),
        }
    }

    pub fn success(message: impl Into<SharedString>) -> Self {
        InputStatus {
            status: StatusType::Success,
            message: Some(message.into()),
        }
    }

    /// A status with no message: it still marks the field (an error sets
    /// it invalid) but shows nothing.
    pub fn bare(status: StatusType) -> Self {
        InputStatus {
            status,
            message: None,
        }
    }
}

// ----------------------------------------------------------------------------
// FieldLabel

/// A field's label, optional/required marker and description, upstream
/// `FieldLabel`.
#[derive(IntoElement)]
pub struct FieldLabel {
    label: SharedString,
    description: Option<SharedString>,
    label_hidden: bool,
    disabled: bool,
    optional: bool,
    required: bool,
    label_icon: Option<AnyElement>,
    style: StyleRefinement,
}

impl FieldLabel {
    pub fn new(label: impl Into<SharedString>) -> Self {
        FieldLabel {
            label: label.into(),
            description: None,
            label_hidden: false,
            disabled: false,
            optional: false,
            required: false,
            label_icon: None,
            style: StyleRefinement::default(),
        }
    }

    astryx_primitives::setters! {
        /// Upstream `description`: supporting text under the label.
        ?description: SharedString,
        /// Upstream `isLabelHidden`: kept for assistive technology only.
        !label_hidden,
        /// Upstream `isDisabled`: the label greys out.
        !disabled,
        /// Upstream `isOptional`: "∙ Optional" after the label.
        !optional,
        /// Upstream `isRequired`: "∙ Required" after the label.
        !required,
    }

    /// Upstream `labelIcon`: before the label, `sm` unless sized.
    pub fn label_icon(mut self, icon: impl IntoElement) -> Self {
        self.label_icon = Some(icon.into_any_element());
        self
    }

    /// Takes an optional icon, for controls that forward theirs.
    pub(crate) fn label_icon_opt(mut self, icon: Option<AnyElement>) -> Self {
        self.label_icon = icon;
        self
    }

    /// Takes an optional description, for controls that forward theirs.
    pub(crate) fn description_opt(mut self, description: Option<SharedString>) -> Self {
        self.description = description;
        self
    }
}

impl Styled for FieldLabel {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for FieldLabel {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let i18n = i18n(cx);
        if self.label_hidden {
            let text = match &self.description {
                Some(d) => format!("{}, {d}", self.label),
                None => self.label.to_string(),
            };
            return VisuallyHidden::new().child(text).into_any_element();
        }
        // FR5 of FormLayout: the marker that restates the form's default
        // is left out; an optional marker wins over a required one.
        let resolved = resolve(self.required, self.optional, cx);
        let marker = if resolved.shows_optional {
            Some(i18n.t("@astryx.field.optional"))
        } else if resolved.shows_required {
            Some(i18n.t("@astryx.field.required"))
        } else {
            None
        };
        let color = if self.disabled {
            t.color.text_disabled
        } else {
            t.color.text_primary
        };
        let mut label = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(t.spacing.s1)
            .text_size(t.text.label.size)
            .line_height(relative(t.text.label.leading))
            .font_weight(t.font.weight.semibold)
            .text_color(color)
            .children(
                self.label_icon
                    .map(|icon| astryx_primitives::provide(IconDefaultSize(IconSize::Sm), icon)),
            )
            .child(self.label)
            .children(marker.map(|marker| {
                div()
                    .text_size(t.text.supporting.size)
                    .line_height(relative(t.text.supporting.leading))
                    .font_weight(t.font.weight.normal)
                    .text_color(t.color.text_secondary)
                    .child(format!("∙ {marker}"))
            }));
        theme.refine(label.style(), "field-label", &[], None);
        label.style().refine(&self.style);
        div()
            .flex()
            .flex_col()
            .min_w_0()
            .child(label)
            .children(self.description.map(|description| {
                div()
                    .text_style(&t.text.supporting)
                    .font_weight(t.font.weight.normal)
                    .text_color(t.color.text_secondary)
                    .child(description)
            }))
            .into_any_element()
    }
}

// ----------------------------------------------------------------------------
// FieldStatus

/// Upstream FieldStatus `variant`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FieldStatusVariant {
    /// Tucked under a bordered input, overlapping its bottom edge.
    #[default]
    Attached,
    /// Its own box with a status icon, for checkboxes, switches and custom
    /// controls.
    Detached,
}

impl FieldStatusVariant {
    fn as_str(self) -> &'static str {
        match self {
            FieldStatusVariant::Attached => "attached",
            FieldStatusVariant::Detached => "detached",
        }
    }
}

#[derive(Default)]
struct Announced(Option<(StatusType, SharedString)>);

/// A field's validation message, upstream `FieldStatus`. It announces
/// itself when it appears and whenever its message or type changes:
/// assertively for errors, politely otherwise.
#[derive(IntoElement)]
pub struct FieldStatus {
    id: ElementId,
    status: StatusType,
    message: SharedString,
    variant: FieldStatusVariant,
    /// Announces without drawing a box (`tooltip` placement).
    announce_only: bool,
    style: StyleRefinement,
}

impl FieldStatus {
    /// Upstream `id`, `type` and `message`.
    pub fn new(
        id: impl Into<ElementId>,
        status: StatusType,
        message: impl Into<SharedString>,
    ) -> Self {
        FieldStatus {
            id: id.into(),
            status,
            message: message.into(),
            variant: FieldStatusVariant::Attached,
            announce_only: false,
            style: StyleRefinement::default(),
        }
    }

    pub(crate) fn announce_only(mut self) -> Self {
        self.announce_only = true;
        self
    }

    astryx_primitives::setters! {
        /// Upstream `variant`.
        variant: FieldStatusVariant,
    }
}

impl Styled for FieldStatus {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for FieldStatus {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let rem = window.rem_size();

        let announced = window.use_keyed_state(
            ElementId::Name(format!("{:?}-announced", self.id).into()),
            cx,
            |_, _| Announced::default(),
        );
        let now = (self.status, self.message.clone());
        if announced.read(cx).0.as_ref() != Some(&now) {
            announced.update(cx, |a, _| a.0 = Some(now));
            let politeness = if self.status == StatusType::Error {
                Politeness::Assertive
            } else {
                Politeness::Polite
            };
            announce(self.message.clone(), politeness, window, cx);
        }
        if self.announce_only {
            return div().id(self.id).into_any_element();
        }

        let (bg, fg) = self.status.colors(t);
        let mut el = div()
            .text_size(t.text.supporting.size)
            .line_height(relative(t.text.supporting.leading))
            .bg(bg)
            .text_color(fg)
            .px(t.spacing.s2);
        el = match self.variant {
            FieldStatusVariant::Attached => {
                // It slides under the input's bottom edge by the overlap.
                let overlap = t.spacing.s1_5.to_pixels(rem);
                el.mt(-overlap)
                    .pt(overlap + t.spacing.s2.to_pixels(rem))
                    .pb(t.spacing.s2)
                    .rounded_b(t.radius.element)
                    .child(self.message)
            }
            FieldStatusVariant::Detached => {
                let line = t.text.supporting.size.to_pixels(rem) * t.text.supporting.leading;
                let mut icon = Icon::new(self.status.icon()).size(IconSize::Sm);
                theme.refine(
                    icon.style(),
                    "field-status-icon",
                    &[("type", self.status.as_str())],
                    None,
                );
                el.mt(t.spacing.s1)
                    .py(t.spacing.s2)
                    .rounded(t.radius.element)
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_start()
                            .gap(t.spacing.s1)
                            .child(div().flex().flex_none().items_center().h(line).child(icon))
                            .child(div().min_w_0().child(self.message)),
                    )
            }
        };
        theme.refine(
            el.style(),
            "field-status",
            &[
                ("type", self.status.as_str()),
                ("variant", self.variant.as_str()),
            ],
            None,
        );
        el.style().refine(&self.style);
        el.id(self.id).into_any_element()
    }
}

// ----------------------------------------------------------------------------
// Field

/// Upstream Field `statusVariant`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum StatusPlacement {
    /// Tucked under a bordered, fixed-height control.
    #[default]
    Attached,
    /// Its own box below the control.
    Detached,
    /// No message box: the control's status icon carries it, and the
    /// message describes the field and announces itself. The icon's tooltip
    /// lands with Tooltip (M5).
    Tooltip,
}

/// The shell of a custom control: a label above, the control, and a status
/// below, upstream `Field`.
#[derive(IntoElement)]
pub struct Field {
    id: ElementId,
    label: SharedString,
    description: Option<SharedString>,
    label_hidden: bool,
    disabled: bool,
    optional: bool,
    required: bool,
    label_icon: Option<AnyElement>,
    label_tooltip: Option<SharedString>,
    status: Option<InputStatus>,
    status_placement: StatusPlacement,
    width: Option<gpui::Length>,
    style: StyleRefinement,
    children: smallvec::SmallVec<[AnyElement; 1]>,
}

impl Field {
    /// Upstream `label`.
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        Field {
            id: id.into(),
            label: label.into(),
            description: None,
            label_hidden: false,
            disabled: false,
            optional: false,
            required: false,
            label_icon: None,
            label_tooltip: None,
            status: None,
            status_placement: StatusPlacement::Attached,
            width: None,
            style: StyleRefinement::default(),
            children: smallvec::SmallVec::new(),
        }
    }

    astryx_primitives::setters! {
        /// Upstream `description`: between the label and the control.
        ?description: SharedString,
        /// Upstream `isLabelHidden`.
        !label_hidden,
        /// Upstream `isDisabled`: greys the label.
        !disabled,
        /// Upstream `isOptional`.
        !optional,
        /// Upstream `isRequired`.
        !required,
        /// Upstream `labelTooltip`. The InfoTip lands with Tooltip (M5).
        ?label_tooltip: SharedString,
        /// Upstream `status`.
        ?status: InputStatus,
        /// Upstream `statusVariant`.
        status_placement: StatusPlacement,
        /// Upstream `width`: the whole field.
        ?width: gpui::Length,
    }

    /// Upstream `labelIcon`.
    pub fn label_icon(mut self, icon: impl IntoElement) -> Self {
        self.label_icon = Some(icon.into_any_element());
        self
    }

    pub(crate) fn description_opt(mut self, description: Option<SharedString>) -> Self {
        self.description = description;
        self
    }
}

impl Styled for Field {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl ParentElement for Field {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for Field {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let form = form_context(cx);
        let beside = form.direction == FormDirection::HorizontalLabels;
        // Beside its control, a label goes without its description, which
        // moves above the control.
        let (label_description, description) = if beside {
            (None, self.description.clone())
        } else {
            (self.description.clone(), None)
        };
        let label = FieldLabel::new(self.label)
            .description_opt(label_description)
            .label_hidden(self.label_hidden)
            .disabled(self.disabled)
            .optional(self.optional)
            .required(self.required)
            .label_icon_opt(self.label_icon);
        let status = self.status.and_then(|status| {
            let message = status.message?;
            Some(
                FieldStatus::new(
                    ElementId::Name(format!("{:?}-status", self.id).into()),
                    status.status,
                    message,
                )
                .variant(match self.status_placement {
                    StatusPlacement::Attached => FieldStatusVariant::Attached,
                    StatusPlacement::Detached | StatusPlacement::Tooltip => {
                        FieldStatusVariant::Detached
                    }
                }),
            )
            .map(|status| {
                if self.status_placement == StatusPlacement::Tooltip {
                    status.announce_only()
                } else {
                    status
                }
            })
        });
        let control = match self.status_placement {
            // The status slides under the control: listed first and laid
            // out in reverse, so the control paints over the overlap.
            StatusPlacement::Attached => div()
                .flex()
                .flex_col_reverse()
                .children(status)
                .child(div().flex().flex_col().children(self.children))
                .into_any_element(),
            StatusPlacement::Detached | StatusPlacement::Tooltip => div()
                .flex()
                .flex_col()
                .children(self.children)
                .children(status)
                .into_any_element(),
        };
        if beside {
            return horizontal_labels(
                self.id,
                label,
                description,
                self.label_hidden,
                control,
                &form,
                &theme,
                &self.style,
                form.labels.as_ref().and_then(|l| l.read(cx).width),
                window,
            )
            .into_any_element();
        }
        let mut el = div().flex().flex_col().min_w_0();
        if !self.label_hidden {
            el = el.gap(t.spacing.s1);
        }
        if let Some(width) = self.width {
            el = el.w(width);
        }
        theme.refine(el.style(), "field", &[], None);
        el.style().refine(&self.style);
        el.id(self.id)
            .child(label)
            .child(control)
            .into_any_element()
    }
}

/// A Field in a `horizontal-labels` FormLayout: its label in the layout's
/// shared label column, then the description, control and status.
#[allow(clippy::too_many_arguments)]
fn horizontal_labels(
    id: ElementId,
    label: FieldLabel,
    description: Option<SharedString>,
    label_hidden: bool,
    control: AnyElement,
    form: &FormContext,
    theme: &astryx_theme::ResolvedTheme,
    style: &StyleRefinement,
    label_width: Option<gpui::Pixels>,
    window: &mut Window,
) -> impl IntoElement {
    let t = theme.tokens();
    let rem = window.rem_size();
    // The label's first line meets the control's text line.
    let mut cell = div()
        .flex_none()
        .pt(t.border.width.to_pixels(rem) + t.spacing.s1.to_pixels(rem))
        .child(div().w_max().child(label));
    if let Some(labels) = &form.labels {
        if let Some(width) = label_width {
            cell = cell.w(width);
        }
        let labels = labels.clone();
        cell = cell.on_children_prepainted(move |bounds, _, cx| {
            if let Some(natural) = bounds.first().map(|b| b.size.width) {
                labels.update(cx, |l, _| l.measured = l.measured.max(natural));
            }
        });
    }
    let description = description.map(|description| {
        if label_hidden {
            VisuallyHidden::new().child(description).into_any_element()
        } else {
            div()
                .text_style(&t.text.supporting)
                .text_color(t.color.text_secondary)
                .child(description)
                .into_any_element()
        }
    });
    let column = div()
        .flex()
        .flex_col()
        .flex_1()
        .min_w_0()
        .children(description)
        .child(control);
    let mut el = div().id(id).flex().min_w_0();
    // Collapsed, the label stacks above its control (upstream's grid cells
    // in one flex column).
    el = if form.collapsed {
        el.flex_col().gap(t.spacing.s4)
    } else {
        el.flex_row().items_start().gap(t.spacing.s4)
    };
    theme.refine(
        el.style(),
        "field",
        &[("layout", "horizontal-labels")],
        None,
    );
    el.style().refine(style);
    el.child(cell).child(column)
}

// ----------------------------------------------------------------------------
// Docs

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

const NO_IDS: &str = "GPUI ties labels to controls through the accessibility tree, not ids";

pub const FIELD_DOC: ComponentDoc = ComponentDoc {
    name: "Field",
    rust_type: "astryx::components::field::Field",
    props: &[
        p("label", RustApi::Argument("label")),
        p("inputID", RustApi::Waived(NO_IDS)),
        p("labelID", RustApi::Waived(NO_IDS)),
        p("isGroupLabel", RustApi::Waived(NO_IDS)),
        p("children", RustApi::Trait("ParentElement")),
        p("isLabelHidden", RustApi::Method("label_hidden")),
        p("isDisabled", RustApi::Method("disabled")),
        p("description", RustApi::Method("description")),
        p("descriptionID", RustApi::Waived(NO_IDS)),
        p("isOptional", RustApi::Method("optional")),
        p("isRequired", RustApi::Method("required")),
        p("labelIcon", RustApi::Method("label_icon")),
        p(
            "labelTooltip",
            RustApi::Pending("label_tooltip", "the InfoTip lands with Tooltip in M5"),
        ),
        p("status", RustApi::Method("status")),
        p(
            "statusVariant",
            RustApi::Method("status_placement (tooltip lands with Tooltip in M5)"),
        ),
        p("width", RustApi::Method("width")),
        p("ref", RustApi::Waived("GPUI elements have no refs")),
        p("xstyle", RustApi::Trait("Styled")),
        p("className", RustApi::Trait("Styled")),
        p("style", RustApi::Trait("Styled")),
    ],
    example: include_str!("../choice/examples.rs"),
};

pub const FIELD_STATUS_DOC: ComponentDoc = ComponentDoc {
    name: "FieldStatus",
    rust_type: "astryx::components::field::FieldStatus",
    props: &[
        p("type", RustApi::Argument("status")),
        p("message", RustApi::Argument("message")),
        p("id", RustApi::Argument("id")),
        p("variant", RustApi::Method("variant")),
    ],
    example: include_str!("../choice/examples.rs"),
};

pub const FORM_LAYOUT_DOC: ComponentDoc = ComponentDoc {
    name: "FormLayout",
    rust_type: "astryx::components::field::FormLayout",
    props: &[
        p("direction", RustApi::Method("direction")),
        p("defaultOptionality", RustApi::Method("optionality")),
        p(
            "defaultEntryErrorBehavior",
            RustApi::Method("entry_error_behavior"),
        ),
        p("children", RustApi::Trait("ParentElement")),
        p("xstyle", RustApi::Trait("Styled")),
    ],
    example: include_str!("../input/examples.rs"),
};
