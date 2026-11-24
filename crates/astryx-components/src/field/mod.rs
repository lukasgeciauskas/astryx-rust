//! Field parts shared by form controls: FieldLabel and FieldStatus.
//! Upstream: `Field/FieldLabel`, `FieldStatus/`. The full Field container
//! lands with the text inputs (M4).

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
        let marker = if self.required {
            Some(i18n.t("@astryx.field.required"))
        } else if self.optional {
            Some(i18n.t("@astryx.field.optional"))
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
            style: StyleRefinement::default(),
        }
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
        el.id(self.id)
    }
}

// ----------------------------------------------------------------------------
// Docs

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

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
