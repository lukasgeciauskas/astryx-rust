//! The row every choice control shares: a control (a checkbox box, a radio
//! circle, a switch track) beside its FieldLabel, the whole row pressable,
//! the focus ring on the control, and a detached FieldStatus below.

use std::rc::Rc;

use astryx_primitives::{control_focus, is_focus_visible};
use astryx_theme::current;
use gpui::{
    AnyElement, App, ClickEvent, ElementId, InteractiveElement, KeyboardButton, Length,
    ParentElement, Pixels, Role, SharedString, Stateful, StatefulInteractiveElement, Styled,
    Toggled, Window, div, prelude::*,
};

use crate::field::{FieldLabel, FieldStatus, FieldStatusVariant, InputStatus};
use crate::focus::focus_ring;

pub(crate) type ToggleFn = Rc<dyn Fn(&mut Window, &mut App)>;

/// Upstream `labelPosition`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LabelPosition {
    /// The label before the control.
    Start,
    /// The label after the control.
    #[default]
    End,
}

/// Upstream `labelSpacing`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LabelSpacing {
    /// Label and control side by side.
    #[default]
    Hug,
    /// Label and control at opposite ends of the full width.
    Spread,
}

/// The label side of a choice row.
pub(crate) struct RowLabel {
    pub label: SharedString,
    pub description: Option<SharedString>,
    pub hidden: bool,
    pub optional: bool,
    pub required: bool,
    pub icon: Option<AnyElement>,
}

/// One choice row.
pub(crate) struct ChoiceRow {
    pub id: ElementId,
    /// The control, painted by its owner.
    pub control: AnyElement,
    /// The control's corner radius, which the focus ring follows.
    pub ring_radius: Pixels,
    pub label: RowLabel,
    pub disabled: bool,
    /// Busy or read-only: focusable at full opacity, but never toggles.
    pub inert: bool,
    pub position: LabelPosition,
    pub spacing: LabelSpacing,
    pub status: Option<InputStatus>,
    pub width: Option<Length>,
    pub role: Role,
    pub toggled: Toggled,
    pub on_toggle: Option<ToggleFn>,
}

/// The GPUI group of a row, which its control's hover and press follow.
pub(crate) fn row_group(id: &ElementId) -> SharedString {
    format!("{id:?}-row").into()
}

/// Whether a click toggles: a pointer, or Space. Like a native checkbox,
/// Enter does not toggle.
pub(crate) fn toggles(event: &ClickEvent) -> bool {
    !matches!(event, ClickEvent::Keyboard(key) if key.button == KeyboardButton::Enter)
}

impl ChoiceRow {
    pub fn render(self, window: &mut Window, cx: &mut App) -> Stateful<gpui::Div> {
        let theme = current(window, cx);
        let t = theme.tokens();
        let rem = window.rem_size();
        let focus = control_focus(&self.id, window, cx);
        let group = row_group(&self.id);

        let control = div()
            .relative()
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .child(self.control)
            .when(is_focus_visible(&focus, window), |el| {
                el.child(focus_ring(t, self.ring_radius, rem))
            });
        let label = FieldLabel::new(self.label.label.clone())
            .description_opt(self.label.description.clone())
            .label_hidden(self.label.hidden)
            .disabled(self.disabled)
            .optional(self.label.optional)
            .required(self.label.required)
            .label_icon_opt(self.label.icon);
        let label = div()
            .flex()
            .flex_col()
            .justify_center()
            .min_w_0()
            .child(label);

        let mut row = div()
            .id(self.id.clone())
            .group(group)
            .track_focus(&focus)
            .flex()
            .flex_row()
            .items_center();
        if !self.label.hidden {
            row = row.gap(t.spacing.s2);
        }
        if self.spacing == LabelSpacing::Spread {
            row = row.justify_between().w_full();
        }
        row = match self.position {
            LabelPosition::End => row.child(control).child(label),
            LabelPosition::Start => row.child(label).child(control),
        };
        match (self.disabled || self.inert, self.on_toggle) {
            (false, Some(toggle)) => {
                row = row.cursor_pointer().on_click(move |event, window, cx| {
                    if toggles(event) {
                        toggle(window, cx);
                    }
                });
            }
            _ => row = row.on_click(|_, _, cx| cx.stop_propagation()),
        }
        let mut name = self.label.label.to_string();
        if let Some(description) = &self.label.description {
            name = format!("{name}, {description}");
        }
        let row = row
            .role(self.role)
            .aria_label(SharedString::from(name))
            .aria_toggled(self.toggled);

        let status = self.status.and_then(|status| {
            status.message.map(|message| {
                FieldStatus::new(
                    ElementId::Name(format!("{:?}-status", self.id).into()),
                    status.status,
                    message,
                )
                .variant(FieldStatusVariant::Detached)
            })
        });
        let mut field = div()
            .id(ElementId::Name(format!("{:?}-field", self.id).into()))
            .flex()
            .flex_col()
            .max_w_full()
            .child(row)
            .children(status);
        if let Some(width) = self.width {
            field = field.w(width);
        }
        field
    }
}
