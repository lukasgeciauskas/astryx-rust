//! Choice controls: CheckboxInput, Switch, RadioList and CheckboxList.
//! Upstream: `CheckboxInput/`, `Switch/`, `RadioList/`, `CheckboxList/`.

pub(crate) mod action;
pub mod examples;
mod lists;
mod row;

#[cfg(test)]
mod tests;

pub use lists::{CheckboxList, CheckboxListItem, RadioList, RadioListItem};
pub(crate) use row::toggles as row_toggles;
pub use row::{LabelPosition, LabelSpacing};

use std::rc::Rc;

use astryx_i18n::{current as i18n, use_direction};
use astryx_theme::color::{composite, mix_srgb};
use astryx_theme::current;
use gpui::{
    AnyElement, App, ElementId, IntoElement, Length, ParentElement, RenderOnce, Role, SharedString,
    StyleRefinement, Styled, Task, Toggled, Window, div, prelude::*, px,
};

use crate::doc::{ComponentDoc, PropMapping, RustApi};
use crate::field::InputStatus;
use crate::indicator::{CheckboxIndicator, ChoiceSize, IndicatorState};
use crate::motion::transition;
use crate::status::{Spinner, SpinnerShade, SpinnerSize};

use action::{ActionFn, ChangeFn, Optimistic};
use row::{ChoiceRow, RowLabel, row_group};

/// The label props every choice control shares.
struct LabelProps {
    label: SharedString,
    description: Option<SharedString>,
    label_hidden: bool,
    optional: bool,
    required: bool,
    label_icon: Option<AnyElement>,
}

impl LabelProps {
    fn new(label: SharedString) -> Self {
        LabelProps {
            label,
            description: None,
            label_hidden: false,
            optional: false,
            required: false,
            label_icon: None,
        }
    }

    fn row(self) -> RowLabel {
        RowLabel {
            label: self.label,
            description: self.description,
            hidden: self.label_hidden,
            optional: self.optional,
            required: self.required,
            icon: self.label_icon,
            tooltip: None,
        }
    }
}

macro_rules! label_setters {
    () => {
        /// Upstream `description`: supporting text under the label.
        pub fn description(mut self, description: impl Into<SharedString>) -> Self {
            self.label.description = Some(description.into());
            self
        }
        /// Upstream `isLabelHidden`: kept for assistive technology only.
        pub fn label_hidden(mut self, hidden: bool) -> Self {
            self.label.label_hidden = hidden;
            self
        }
        /// Upstream `isOptional`.
        pub fn optional(mut self, optional: bool) -> Self {
            self.label.optional = optional;
            self
        }
        /// Upstream `isRequired`.
        pub fn required(mut self, required: bool) -> Self {
            self.label.required = required;
            self
        }
        /// Upstream `labelIcon`: before the label.
        pub fn label_icon(mut self, icon: impl IntoElement) -> Self {
            self.label.label_icon = Some(icon.into_any_element());
            self
        }
    };
}

fn toggled(state: IndicatorState) -> Toggled {
    match state {
        IndicatorState::Unchecked => Toggled::False,
        IndicatorState::Checked => Toggled::True,
        IndicatorState::Indeterminate => Toggled::Mixed,
    }
}

// ----------------------------------------------------------------------------
// CheckboxInput

/// A single on/off choice, upstream `CheckboxInput`.
///
/// ```ignore
/// CheckboxInput::new("terms", "I accept the terms", accepted)
///     .on_change(|checked, _, _| { /* store it */ })
/// ```
#[derive(IntoElement)]
pub struct CheckboxInput {
    id: ElementId,
    label: LabelProps,
    value: IndicatorState,
    on_change: Option<ChangeFn<bool>>,
    change_action: Option<ActionFn<bool>>,
    loading: bool,
    disabled: bool,
    read_only: bool,
    size: ChoiceSize,
    status: Option<InputStatus>,
    width: Option<Length>,
    disabled_message: Option<SharedString>,
    style: StyleRefinement,
}

impl CheckboxInput {
    /// Upstream `label` and `value` (a bool, or an [`IndicatorState`] for
    /// indeterminate).
    pub fn new(
        id: impl Into<ElementId>,
        label: impl Into<SharedString>,
        value: impl Into<IndicatorState>,
    ) -> Self {
        CheckboxInput {
            id: id.into(),
            label: LabelProps::new(label.into()),
            value: value.into(),
            on_change: None,
            change_action: None,
            loading: false,
            disabled: false,
            read_only: false,
            size: ChoiceSize::Md,
            status: None,
            width: None,
            disabled_message: None,
            style: StyleRefinement::default(),
        }
    }

    label_setters!();

    astryx_primitives::setters! {
        /// Upstream `isLoading`: a spinner in the box; focusable but inert.
        !loading,
        /// Upstream `isDisabled`: focusable, greyed, never changes.
        !disabled,
        /// Upstream `isReadOnly`: full opacity, never changes.
        !read_only,
        /// Upstream `size`.
        size: ChoiceSize,
        /// Upstream `status`: a detached message below; an error marks it
        /// invalid.
        ?status: InputStatus,
        /// Upstream `width`.
        ?width: Length,
        /// Upstream `disabledMessage`: the reason, in a tooltip on hover and focus.
        ?disabled_message: SharedString,
    }

    /// Upstream `onChange`: the next checked state.
    pub fn on_change(mut self, f: impl Fn(bool, &mut Window, &mut App) + 'static) -> Self {
        self.on_change = Some(Rc::new(f));
        self
    }

    /// Upstream `changeAction`: async work after `on_change`; the box shows
    /// the new state and a spinner until the task finishes.
    pub fn change_action(
        mut self,
        f: impl Fn(bool, &mut Window, &mut App) -> Task<()> + 'static,
    ) -> Self {
        self.change_action = Some(Rc::new(f));
        self
    }
}

impl Styled for CheckboxInput {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for CheckboxInput {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let rem = window.rem_size();
        let optimistic = Optimistic::<bool>::new(&self.id, window, cx);
        let shown = match optimistic.shown(self.value.is_on(), cx) {
            _ if !optimistic.pending(cx) => self.value,
            on => IndicatorState::from(on),
        };
        let busy = self.loading || optimistic.pending(cx);
        optimistic.announce_busy(busy, i18n(cx).t("@astryx.spinner.loading"), window, cx);

        let mut indicator = CheckboxIndicator::new(shown)
            .size(self.size)
            .disabled(self.disabled)
            .hover_group(row_group(&self.id));
        if busy {
            indicator = indicator.child(
                Spinner::new(ElementId::Name(format!("{:?}-spinner", self.id).into()))
                    .size(SpinnerSize::Sm)
                    .shade(SpinnerShade::Inherit),
            );
        }
        // A press paints the overlay over the box (upstream indicator press): a mouse's
        // through the active style where the pointer is fine, a finger's as UIKit tracks
        // a control (upstream `usePressFeedback`).
        let group = row_group(&self.id);
        let overlay = t.color.overlay_pressed;
        let feedback = crate::gestures::use_press_feedback(
            ElementId::Name(format!("{group}-press").into()),
            window,
            cx,
        )
        .disabled(self.disabled || busy || self.read_only)
        .owns_box(true);
        let mouse_active = !feedback.paints_buttons(window, cx);
        let touch_overlay = feedback.overlay(overlay, window, cx);
        // Upstream `inputCoarse`: under a coarse pointer the box's target is at least
        // 24 px square, the indicator centered in it.
        let coarse = astryx_primitives::coarse_primary_pointer(window, cx);
        let control = div()
            .relative()
            .when(coarse, |el| {
                el.flex()
                    .items_center()
                    .justify_center()
                    .min_w(px(24.))
                    .min_h(px(24.))
            })
            .child(indicator)
            .when(!self.disabled && !busy && !self.read_only, |el| {
                let mut press = div()
                    .id("press")
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .rounded(t.radius.inner);
                if mouse_active {
                    press = press.group_active(group, move |s| s.bg(overlay));
                }
                if let Some(o) = touch_overlay {
                    press = press.bg(o);
                }
                el.child(press)
            });

        let next = shown != IndicatorState::Checked;
        let change = optimistic.change(self.on_change, self.change_action);
        let row = ChoiceRow {
            id: self.id.clone(),
            control: control.into_any_element(),
            ring_radius: t.radius.inner.to_pixels(rem),
            label: self.label.row(),
            disabled: self.disabled,
            inert: busy || self.read_only,
            busy,
            read_only: self.read_only,
            position: LabelPosition::End,
            spacing: LabelSpacing::Hug,
            status: self.status,
            width: self.width,
            role: Role::CheckBox,
            toggled: toggled(shown),
            on_toggle: Some(Rc::new(move |window, cx| change(next, window, cx))),
            press: Some(feedback),
        };
        let mut el = row.render(window, cx);
        theme.refine(
            el.style(),
            "checkbox-input",
            &[("size", self.size.as_str())],
            None,
        );
        el.style().refine(&self.style);
        crate::overlay::disabled_reason(
            &self.id,
            self.disabled,
            self.disabled_message.as_ref(),
            false,
            el,
        )
    }
}

// ----------------------------------------------------------------------------
// Switch

/// (width, height) of the track, (off, on) thumb diameters.
fn switch_geometry(size: ChoiceSize) -> ((f32, f32), (f32, f32)) {
    match size {
        ChoiceSize::Sm => ((32., 20.), (14., 16.)),
        ChoiceSize::Md => ((40., 24.), (16., 20.)),
    }
}

/// The thumb's (inline offset, diameter) at progress `p` toward on: its
/// center travels from half the track's height in from the start to half
/// its height in from the end, and it grows.
pub(crate) fn thumb_at(size: ChoiceSize, p: f32) -> (f32, f32) {
    let ((w, h), (off, on)) = switch_geometry(size);
    let diameter = off + (on - off) * p;
    let center = h / 2. + (w - h) * p;
    (center - diameter / 2., diameter)
}

/// A binary on/off toggle, upstream `Switch`.
///
/// ```ignore
/// Switch::new("wifi", "Wi-Fi", enabled).on_change(|on, _, _| { /* store it */ })
/// ```
#[derive(IntoElement)]
pub struct Switch {
    id: ElementId,
    label: LabelProps,
    value: bool,
    on_change: Option<ChangeFn<bool>>,
    change_action: Option<ActionFn<bool>>,
    loading: bool,
    disabled: bool,
    size: ChoiceSize,
    status: Option<InputStatus>,
    label_position: LabelPosition,
    label_spacing: LabelSpacing,
    width: Option<Length>,
    disabled_message: Option<SharedString>,
    label_tooltip: Option<SharedString>,
    style: StyleRefinement,
}

impl Switch {
    /// Upstream `label` and `value`.
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>, value: bool) -> Self {
        Switch {
            id: id.into(),
            label: LabelProps::new(label.into()),
            value,
            on_change: None,
            change_action: None,
            loading: false,
            disabled: false,
            size: ChoiceSize::Md,
            status: None,
            label_position: LabelPosition::End,
            label_spacing: LabelSpacing::Hug,
            width: None,
            disabled_message: None,
            label_tooltip: None,
            style: StyleRefinement::default(),
        }
    }

    label_setters!();

    astryx_primitives::setters! {
        /// Upstream `isLoading`: a spinner in the thumb; focusable but inert.
        !loading,
        /// Upstream `isDisabled`.
        !disabled,
        /// Upstream `size`: 32×20 or 40×24.
        size: ChoiceSize,
        /// Upstream `status`.
        ?status: InputStatus,
        /// Upstream `labelPosition`.
        label_position: LabelPosition,
        /// Upstream `labelSpacing`.
        label_spacing: LabelSpacing,
        /// Upstream `width`.
        ?width: Length,
        /// Upstream `disabledMessage`: the reason, in a tooltip on hover and focus.
        ?disabled_message: SharedString,
        /// Upstream `labelTooltip`: an InfoTip beside the label.
        ?label_tooltip: SharedString,
    }

    /// Upstream `onChange`: the next state.
    pub fn on_change(mut self, f: impl Fn(bool, &mut Window, &mut App) + 'static) -> Self {
        self.on_change = Some(Rc::new(f));
        self
    }

    /// Upstream `changeAction`: async work after `on_change`; the switch
    /// shows the new state and a spinner until the task finishes.
    pub fn change_action(
        mut self,
        f: impl Fn(bool, &mut Window, &mut App) -> Task<()> + 'static,
    ) -> Self {
        self.change_action = Some(Rc::new(f));
        self
    }
}

impl Styled for Switch {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Switch {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let dir = use_direction(cx);
        let optimistic = Optimistic::<bool>::new(&self.id, window, cx);
        let on = optimistic.shown(self.value, cx);
        let busy = self.loading || optimistic.pending(cx);
        optimistic.announce_busy(busy, i18n(cx).t("@astryx.switch.loading"), window, cx);

        let p = transition(
            format!("{:?}-thumb", self.id),
            on,
            t.duration.fast,
            t.ease.standard,
            window,
            cx,
        );
        let ((w, h), _) = switch_geometry(self.size);
        let (offset, diameter) = thumb_at(self.size, p);
        let start = if dir.is_rtl() {
            w - offset - diameter
        } else {
            offset
        };
        let c = &t.color;
        let track = mix_srgb(c.background_gray, c.accent, p);
        let hover = mix_srgb(track, c.tint_hover, if on { 0.15 } else { 0.05 });
        let group = row_group(&self.id);
        let overlay = c.overlay_pressed;
        let thumb_pressed = composite(c.background_surface, overlay);
        let inert = self.disabled || busy;
        // A finger's press paints the thumb and the track as UIKit tracks a control
        // (upstream `usePressFeedback`); a fine pointer keeps the active style.
        let feedback = crate::gestures::use_press_feedback(
            ElementId::Name(format!("{group}-press").into()),
            window,
            cx,
        )
        .disabled(inert);
        let mouse_active = !feedback.paints_buttons(window, cx);
        let touch_overlay = feedback.overlay(overlay, window, cx);

        let thumb = div()
            .id("thumb")
            .absolute()
            .left(px(start))
            .top(px((h - diameter) / 2.))
            .size(px(diameter))
            .flex()
            .items_center()
            .justify_center()
            .rounded_full()
            .bg(c.background_surface)
            .text_color(if on { c.accent } else { c.text_secondary })
            .when(!inert && mouse_active, |el| {
                el.group_active(group.clone(), move |s| s.bg(thumb_pressed))
            })
            .when_some(touch_overlay.filter(|_| !inert), |el, o| {
                el.bg(composite(c.background_surface, o))
            })
            .when(busy, |el| {
                el.child(
                    Spinner::new(ElementId::Name(format!("{:?}-spinner", self.id).into()))
                        .size(SpinnerSize::Sm)
                        .shade(SpinnerShade::Inherit),
                )
            });
        let mut control = div()
            .id("track")
            .relative()
            .flex_none()
            .w(px(w))
            .h(px(h))
            .rounded_full()
            .bg(track)
            .child(thumb);
        if self.disabled {
            control = control.opacity(0.5);
        } else if !busy {
            control = control.group_hover(group.clone(), move |s| s.bg(hover));
            if mouse_active {
                control = control.group_active(group, move |s| s.bg(composite(track, overlay)));
            }
            if let Some(o) = touch_overlay {
                control = control.bg(composite(track, o));
            }
        }
        theme.refine(
            control.style(),
            "switch-track",
            &[("checked", if on { "true" } else { "false" })],
            None,
        );

        let change = optimistic.change(self.on_change, self.change_action);
        let row = ChoiceRow {
            id: self.id.clone(),
            control: control.into_any_element(),
            ring_radius: px(h / 2.),
            label: RowLabel {
                tooltip: self.label_tooltip.clone(),
                ..self.label.row()
            },
            disabled: self.disabled,
            inert: busy,
            busy,
            read_only: false,
            position: self.label_position,
            spacing: self.label_spacing,
            status: self.status,
            width: self.width,
            role: Role::Switch,
            toggled: if on { Toggled::True } else { Toggled::False },
            on_toggle: Some(Rc::new(move |window, cx| change(!on, window, cx))),
            press: Some(feedback),
        };
        let mut el = row.render(window, cx);
        theme.refine(el.style(), "switch", &[("size", self.size.as_str())], None);
        el.style().refine(&self.style);
        crate::overlay::disabled_reason(
            &self.id,
            self.disabled,
            self.disabled_message.as_ref(),
            false,
            el,
        )
    }
}

// ----------------------------------------------------------------------------
// Docs

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

const NO_FORMS: &str = "GPUI has no HTML forms";
const NO_REFS: &str = "GPUI elements have no refs";
const FOCUS_EVENTS: &str =
    "observe the window's focus instead (GPUI has no per-element focus events here)";

pub const CHECKBOX_INPUT_DOC: ComponentDoc = ComponentDoc {
    name: "CheckboxInput",
    rust_type: "astryx::components::choice::CheckboxInput",
    props: &[
        p("ref", RustApi::Waived(NO_REFS)),
        p("label", RustApi::Argument("label")),
        p("isLabelHidden", RustApi::Method("label_hidden")),
        p("description", RustApi::Method("description")),
        p("value", RustApi::Argument("value")),
        p("onChange", RustApi::Method("on_change")),
        p("changeAction", RustApi::Method("change_action")),
        p("isLoading", RustApi::Method("loading")),
        p("isDisabled", RustApi::Method("disabled")),
        p("htmlName", RustApi::Waived(NO_FORMS)),
        p("disabledMessage", RustApi::Method("disabled_message")),
        p("isReadOnly", RustApi::Method("read_only")),
        p("isOptional", RustApi::Method("optional")),
        p("isRequired", RustApi::Method("required")),
        p("size", RustApi::Method("size")),
        p("onFocus", RustApi::Waived(FOCUS_EVENTS)),
        p("onBlur", RustApi::Waived(FOCUS_EVENTS)),
        p("labelIcon", RustApi::Method("label_icon")),
        p("status", RustApi::Method("status")),
        p("width", RustApi::Method("width")),
    ],
    example: include_str!("examples.rs"),
};

pub const RADIO_LIST_DOC: ComponentDoc = ComponentDoc {
    name: "RadioList",
    rust_type: "astryx::components::choice::RadioList",
    props: &[
        p("label", RustApi::Argument("label")),
        p("value", RustApi::Argument("value")),
        p("onChange", RustApi::Method("on_change")),
        p("children", RustApi::Method("item / items")),
        p("isLabelHidden", RustApi::Method("label_hidden")),
        p("description", RustApi::Method("description")),
        p("orientation", RustApi::Method("orientation")),
        p("isDisabled", RustApi::Method("disabled")),
        p("htmlName", RustApi::Waived(NO_FORMS)),
        p("disabledMessage", RustApi::Method("disabled_message")),
        p("isRequired", RustApi::Method("required")),
        p("isOptional", RustApi::Method("optional")),
        p("status", RustApi::Method("status")),
        p("size", RustApi::Method("size")),
        p("labelTooltip", RustApi::Method("label_tooltip")),
        p("width", RustApi::Method("width")),
        p("containerXstyle", RustApi::Method("container_style")),
        p("containerClassName", RustApi::Method("container_style")),
        p("containerStyle", RustApi::Method("container_style")),
        p("controlXstyle", RustApi::Trait("Styled")),
        p("controlClassName", RustApi::Trait("Styled")),
        p("controlStyle", RustApi::Trait("Styled")),
        p("ref", RustApi::Waived(NO_REFS)),
    ],
    example: include_str!("examples.rs"),
};

pub const RADIO_LIST_ITEM_DOC: ComponentDoc = ComponentDoc {
    name: "RadioListItem",
    rust_type: "astryx::components::choice::RadioListItem",
    props: &[
        p("label", RustApi::Argument("label")),
        p("aria-label", RustApi::Method("aria_label")),
        p("value", RustApi::Argument("value")),
        p("description", RustApi::Method("description")),
        p("align", RustApi::Method("align")),
        p("isDisabled", RustApi::Method("disabled")),
        p("startContent", RustApi::Method("start_content")),
        p("endContent", RustApi::Method("end_content")),
        p("ref", RustApi::Waived(NO_REFS)),
    ],
    example: include_str!("examples.rs"),
};

pub const CHECKBOX_LIST_DOC: ComponentDoc = ComponentDoc {
    name: "CheckboxList",
    rust_type: "astryx::components::choice::CheckboxList",
    props: &[
        p("label", RustApi::Argument("label")),
        p("children", RustApi::Method("item / items")),
        p("value", RustApi::Method("value")),
        p("onChange", RustApi::Method("on_change")),
        p("changeAction", RustApi::Method("change_action")),
        p("isLabelHidden", RustApi::Method("label_hidden")),
        p("description", RustApi::Method("description")),
        p("isOptional", RustApi::Method("optional")),
        p("isRequired", RustApi::Method("required")),
        p("density", RustApi::Method("density")),
        p("hasDividers", RustApi::Method("dividers")),
        p("isDisabled", RustApi::Method("disabled")),
        p("disabledMessage", RustApi::Method("disabled_message")),
        p("isReadOnly", RustApi::Method("read_only")),
        p("status", RustApi::Method("status")),
        p("width", RustApi::Method("width")),
        p("containerXstyle", RustApi::Method("container_style")),
        p("containerClassName", RustApi::Method("container_style")),
        p("containerStyle", RustApi::Method("container_style")),
        p("controlXstyle", RustApi::Trait("Styled")),
        p("controlClassName", RustApi::Trait("Styled")),
        p("controlStyle", RustApi::Trait("Styled")),
    ],
    example: include_str!("examples.rs"),
};

pub const CHECKBOX_LIST_ITEM_DOC: ComponentDoc = ComponentDoc {
    name: "CheckboxListItem",
    rust_type: "astryx::components::choice::CheckboxListItem",
    props: &[
        p("label", RustApi::Argument("label")),
        p("aria-label", RustApi::Method("aria_label")),
        p("value", RustApi::Method("value")),
        p("description", RustApi::Method("description")),
        p("endContent", RustApi::Method("end_content")),
        p("align", RustApi::Method("align")),
        p("isDisabled", RustApi::Method("disabled")),
        p("isLoading", RustApi::Method("loading")),
        p("isChecked", RustApi::Method("checked")),
        p("onCheck", RustApi::Method("on_check")),
    ],
    example: include_str!("examples.rs"),
};

pub const SWITCH_DOC: ComponentDoc = ComponentDoc {
    name: "Switch",
    rust_type: "astryx::components::choice::Switch",
    props: &[
        p("ref", RustApi::Waived(NO_REFS)),
        p("label", RustApi::Argument("label")),
        p("value", RustApi::Argument("value")),
        p("onChange", RustApi::Method("on_change")),
        p("changeAction", RustApi::Method("change_action")),
        p("isLoading", RustApi::Method("loading")),
        p("isLabelHidden", RustApi::Method("label_hidden")),
        p("description", RustApi::Method("description")),
        p("isDisabled", RustApi::Method("disabled")),
        p("size", RustApi::Method("size")),
        p("htmlName", RustApi::Waived(NO_FORMS)),
        p("disabledMessage", RustApi::Method("disabled_message")),
        p("isOptional", RustApi::Method("optional")),
        p("isRequired", RustApi::Method("required")),
        p("status", RustApi::Method("status")),
        p("onFocus", RustApi::Waived(FOCUS_EVENTS)),
        p("onBlur", RustApi::Waived(FOCUS_EVENTS)),
        p("labelIcon", RustApi::Method("label_icon")),
        p("labelTooltip", RustApi::Method("label_tooltip")),
        p("labelPosition", RustApi::Method("label_position")),
        p("labelSpacing", RustApi::Method("label_spacing")),
        p("width", RustApi::Method("width")),
    ],
    example: include_str!("examples.rs"),
};
