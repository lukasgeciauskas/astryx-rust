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
    /// Upstream `labelTooltip`: an InfoTip after the label.
    pub tooltip: Option<SharedString>,
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
    /// Busy, for assistive technology (`aria-busy`).
    pub busy: bool,
    /// Read-only, for assistive technology (`aria-readonly`).
    pub read_only: bool,
    pub position: LabelPosition,
    pub spacing: LabelSpacing,
    pub status: Option<InputStatus>,
    pub width: Option<Length>,
    pub role: Role,
    pub toggled: Toggled,
    pub on_toggle: Option<ToggleFn>,
    /// The touch press model's surface (upstream `usePressFeedback` on the row): its
    /// paint is the control's to draw.
    pub press: Option<crate::gestures::PressFeedback>,
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
            .label_icon_opt(self.label.icon)
            .label_tooltip_opt(self.label.tooltip.clone());
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
        if let Some(press) = &self.press {
            row = row
                .relative()
                .child(press.probe().absolute().top_0().left_0().size_full());
        }
        // Under a coarse pointer the row's target grows toward 44 px (upstream
        // `useTouchHitArea` on CheckboxInput's, RadioListItem's and Switch's rows).
        let toggle_for_area = self
            .on_toggle
            .clone()
            .filter(|_| !(self.disabled || self.inert));
        row = row
            .relative()
            .children(crate::gestures::touch_hit_area::hit_area_with(
                &self.id,
                toggle_for_area.is_some(),
                toggle_for_area.map(|toggle| {
                    Rc::new(move |window: &mut Window, cx: &mut App| toggle(window, cx))
                        as crate::gestures::touch_hit_area::ActivateFn
                }),
                None,
                // A finger on the grown area shows the row's press paint.
                crate::gestures::touch_hit_area::HitLinks {
                    gestures: None,
                    press: self.press.as_ref(),
                },
                window,
                cx,
            ));
        match (self.disabled || self.inert, self.on_toggle) {
            (false, Some(toggle)) => {
                row = row.cursor_pointer().on_click(move |event, window, cx| {
                    cx.stop_propagation();
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
        let invalid = self
            .status
            .as_ref()
            .is_some_and(|s| s.status == crate::field::StatusType::Error);
        let row = row
            .role(self.role)
            .aria_label(SharedString::from(name))
            .aria_toggled(self.toggled)
            .aria_disabled(self.disabled)
            .aria_busy(self.busy)
            .aria_read_only(self.read_only)
            .aria_required(self.label.required && !self.label.optional)
            .aria_invalid(invalid);

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
