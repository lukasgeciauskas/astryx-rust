//! RadioList and CheckboxList. Upstream: `RadioList/`, `CheckboxList/`.

use std::rc::Rc;

use astryx_i18n::{current as i18n, use_direction};
use astryx_primitives::{ListFocus, ListKeys, is_focus_visible};
use astryx_theme::color::composite;
use astryx_theme::current;
use gpui::{
    AnyElement, App, ElementId, IntoElement, Length, ParentElement, RenderOnce, Role, SharedString,
    StyleRefinement, Styled, Task, Toggled, Window, div, prelude::*,
};

use super::action::{ActionFn, ChangeFn, Optimistic};
use super::row::{ToggleFn, toggles};
use crate::field::{Field, InputStatus, StatusPlacement};
use crate::focus::focus_ring;
use crate::indicator::{CheckboxIndicator, ChoiceSize, IndicatorState, RadioIndicator};
use crate::item::{Item, ItemAlign, ItemDensity};
use crate::layout::Orientation;
use crate::status::{Spinner, SpinnerShade, SpinnerSize};

/// The group-level field props both lists share.
struct GroupField {
    label: SharedString,
    description: Option<SharedString>,
    label_hidden: bool,
    optional: bool,
    required: bool,
    disabled: bool,
    status: Option<InputStatus>,
    width: Option<Length>,
    label_tooltip: Option<SharedString>,
    disabled_message: Option<SharedString>,
}

impl GroupField {
    fn new(label: SharedString) -> Self {
        GroupField {
            label,
            description: None,
            label_hidden: false,
            optional: false,
            required: false,
            disabled: false,
            status: None,
            width: None,
            label_tooltip: None,
            disabled_message: None,
        }
    }

    fn field(&self, id: &ElementId) -> Field {
        let mut field = Field::new(
            ElementId::Name(format!("{id:?}-field").into()),
            self.label.clone(),
        )
        .description_opt(self.description.clone())
        .label_hidden(self.label_hidden)
        .optional(self.optional)
        .required(self.required)
        .disabled(self.disabled)
        .status_placement(StatusPlacement::Detached);
        if let Some(status) = self.status.clone() {
            field = field.status(status);
        }
        if let Some(width) = self.width {
            field = field.width(width);
        }
        field
    }
}

macro_rules! group_setters {
    () => {
        /// Upstream `description`.
        pub fn description(mut self, description: impl Into<SharedString>) -> Self {
            self.field.description = Some(description.into());
            self
        }
        /// Upstream `isLabelHidden`.
        pub fn label_hidden(mut self, hidden: bool) -> Self {
            self.field.label_hidden = hidden;
            self
        }
        /// Upstream `isOptional`.
        pub fn optional(mut self, optional: bool) -> Self {
            self.field.optional = optional;
            self
        }
        /// Upstream `isRequired`.
        pub fn required(mut self, required: bool) -> Self {
            self.field.required = required;
            self
        }
        /// Upstream `isDisabled`: every item, still focusable.
        pub fn disabled(mut self, disabled: bool) -> Self {
            self.field.disabled = disabled;
            self
        }
        /// Upstream `status`: a detached message under the list.
        pub fn status(mut self, status: InputStatus) -> Self {
            self.field.status = Some(status);
            self
        }
        /// Upstream `width`: the whole field.
        pub fn width(mut self, width: impl Into<Length>) -> Self {
            self.field.width = Some(width.into());
            self
        }
        /// Upstream `disabledMessage`. The tooltip lands with Tooltip (M5).
        pub fn disabled_message(mut self, message: impl Into<SharedString>) -> Self {
            self.field.disabled_message = Some(message.into());
            self
        }
    };
}

// ----------------------------------------------------------------------------
// RadioList

/// One option of a [`RadioList`], upstream `RadioListItem`.
pub struct RadioListItem {
    value: SharedString,
    label: SharedString,
    aria_label: Option<SharedString>,
    description: Option<SharedString>,
    align: ItemAlign,
    disabled: bool,
    start_content: Option<AnyElement>,
    end_content: Option<AnyElement>,
}

impl RadioListItem {
    /// Upstream `value` and `label`.
    pub fn new(value: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        RadioListItem {
            value: value.into(),
            label: label.into(),
            aria_label: None,
            description: None,
            align: ItemAlign::Center,
            disabled: false,
            start_content: None,
            end_content: None,
        }
    }

    astryx_primitives::setters! {
        /// Upstream `aria-label`: a name replacing the label's text.
        ?aria_label: SharedString,
        /// Upstream `description`.
        ?description: SharedString,
        /// Upstream `align`.
        align: ItemAlign,
        /// Upstream `isDisabled`.
        !disabled,
    }

    /// Upstream `startContent`: after the circle, before the label.
    pub fn start_content(mut self, content: impl IntoElement) -> Self {
        self.start_content = Some(content.into_any_element());
        self
    }

    /// Upstream `endContent`.
    pub fn end_content(mut self, content: impl IntoElement) -> Self {
        self.end_content = Some(content.into_any_element());
        self
    }
}

type SelectFn = Rc<dyn Fn(SharedString, &mut Window, &mut App)>;

/// Single choice from a list, upstream `RadioList`.
///
/// ```ignore
/// RadioList::new("plan", "Plan", "pro")
///     .item(RadioListItem::new("free", "Free"))
///     .item(RadioListItem::new("pro", "Pro"))
///     .on_change(|value, _, _| { /* store it */ })
/// ```
#[derive(IntoElement)]
pub struct RadioList {
    id: ElementId,
    field: GroupField,
    value: SharedString,
    on_change: Option<SelectFn>,
    items: Vec<RadioListItem>,
    orientation: Orientation,
    size: ChoiceSize,
    style: StyleRefinement,
}

impl RadioList {
    /// Upstream `label` and `value` (empty selects nothing).
    pub fn new(
        id: impl Into<ElementId>,
        label: impl Into<SharedString>,
        value: impl Into<SharedString>,
    ) -> Self {
        RadioList {
            id: id.into(),
            field: GroupField::new(label.into()),
            value: value.into(),
            on_change: None,
            items: Vec::new(),
            orientation: Orientation::Vertical,
            size: ChoiceSize::Md,
            style: StyleRefinement::default(),
        }
    }

    group_setters!();

    astryx_primitives::setters! {
        /// Upstream `orientation`.
        orientation: Orientation,
        /// Upstream `size`: the circles.
        size: ChoiceSize,
    }

    /// Upstream `labelTooltip`. The InfoTip lands with Tooltip (M5).
    pub fn label_tooltip(mut self, tooltip: impl Into<SharedString>) -> Self {
        self.field.label_tooltip = Some(tooltip.into());
        self
    }

    /// Upstream `onChange`: the newly selected value.
    pub fn on_change(mut self, f: impl Fn(SharedString, &mut Window, &mut App) + 'static) -> Self {
        self.on_change = Some(Rc::new(f));
        self
    }

    /// Upstream `children`: one option.
    pub fn item(mut self, item: RadioListItem) -> Self {
        self.items.push(item);
        self
    }

    /// Upstream `children`: options.
    pub fn items(mut self, items: impl IntoIterator<Item = RadioListItem>) -> Self {
        self.items.extend(items);
        self
    }
}

impl Styled for RadioList {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for RadioList {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let rem = window.rem_size();
        let dir = use_direction(cx);
        let group_disabled = self.field.disabled;
        let disabled: Vec<bool> = self
            .items
            .iter()
            .map(|i| i.disabled || group_disabled)
            .collect();
        let values: Rc<[SharedString]> = self.items.iter().map(|i| i.value.clone()).collect();
        let selected = values.iter().position(|v| *v == self.value);

        // Arrows move and select, wrapping, skipping disabled options; a
        // disabled group still lets the keyboard read every option.
        let on_change = self.on_change.clone();
        let move_values = values.clone();
        let move_disabled = disabled.clone();
        let list = ListFocus::new(
            &self.id,
            self.items.len(),
            ListKeys::both()
                .home_end(false)
                .reach_disabled(group_disabled),
            window,
            cx,
        )
        .disabled(disabled.iter().copied().map(|d| d && !group_disabled))
        .rtl(dir.is_rtl())
        .on_move(move |index, window, cx| {
            if !move_disabled[index]
                && let Some(f) = &on_change
            {
                f(move_values[index].clone(), window, cx);
            }
        });
        let tab_stop = list
            .focused(window)
            .or(selected)
            .or_else(|| disabled.iter().position(|d| !d))
            .or((!values.is_empty()).then_some(0));

        let rows = self.items.into_iter().enumerate().map(|(i, item)| {
            let checked = selected == Some(i);
            let item_disabled = disabled[i];
            let handle = list.item_handle(i, tab_stop);
            let row_id = ElementId::Name(format!("{:?}-{}", self.id, item.value).into());
            let group: SharedString = format!("{row_id:?}-row").into();
            let overlay = t.color.overlay_pressed;
            let circle = div()
                .relative()
                .flex()
                .flex_none()
                .child(
                    RadioIndicator::new(checked)
                        .size(self.size)
                        .disabled(item_disabled)
                        .hover_group(group.clone()),
                )
                .when(!item_disabled, |el| {
                    el.child(
                        div()
                            .id("press")
                            .absolute()
                            .top_0()
                            .left_0()
                            .size_full()
                            .rounded_full()
                            .group_active(group.clone(), move |s| s.bg(overlay)),
                    )
                })
                .when(is_focus_visible(&handle, window), |el| {
                    el.child(focus_ring(t, self.size.box_size() / 2., rem))
                });
            let start = div()
                .flex()
                .flex_row()
                .items_center()
                .gap(t.spacing.s2)
                .child(circle)
                .children(item.start_content);
            let mut row_item = Item::new(row_id.clone(), item.label.clone())
                .bare(true)
                .disabled(item_disabled)
                .align(item.align)
                .start_content(start)
                .end_content_opt(item.end_content)
                .description_opt(item.description.map(|d| d.into_any_element()));
            theme.refine(
                row_item.style(),
                "radio-list-item",
                &[
                    ("size", self.size.as_str()),
                    ("selected", if checked { "selected" } else { "" }),
                    ("disabled", if item_disabled { "disabled" } else { "" }),
                ],
                None,
            );
            let select = self.on_change.clone();
            let value = item.value.clone();
            let mut row = div()
                .id(row_id)
                .group(group)
                .track_focus(&handle)
                .min_w_0()
                .child(row_item);
            if item_disabled {
                row = row.on_click(|_, _, cx| cx.stop_propagation());
            } else {
                row = row.cursor_pointer().on_click(move |event, window, cx| {
                    if toggles(event)
                        && !checked
                        && let Some(f) = &select
                    {
                        f(value.clone(), window, cx);
                    }
                });
            }
            row.role(Role::RadioButton)
                .aria_label(item.aria_label.unwrap_or(item.label))
                .aria_toggled(if checked {
                    Toggled::True
                } else {
                    Toggled::False
                })
        });

        let mut group = match self.orientation {
            Orientation::Vertical => div().flex().flex_col().gap(t.spacing.s2),
            Orientation::Horizontal => div().flex().flex_row().flex_wrap().gap(t.spacing.s5),
        };
        theme.refine(
            group.style(),
            "radio-list",
            &[
                ("orientation", self.orientation.as_str()),
                ("size", self.size.as_str()),
            ],
            None,
        );
        group.style().refine(&self.style);
        let group = group
            .id(ElementId::Name(format!("{:?}-group", self.id).into()))
            .on_key_down(list.key_down_listener())
            .role(Role::RadioGroup)
            .aria_label(self.field.label.clone())
            .children(rows);
        self.field.field(&self.id).child(group)
    }
}

// ----------------------------------------------------------------------------
// CheckboxList

type CheckFn = Rc<dyn Fn(bool, &mut Window, &mut App)>;

/// One box of a [`CheckboxList`], upstream `CheckboxListItem`.
pub struct CheckboxListItem {
    value: Option<SharedString>,
    label: SharedString,
    aria_label: Option<SharedString>,
    description: Option<SharedString>,
    end_content: Option<AnyElement>,
    align: ItemAlign,
    disabled: bool,
    loading: bool,
    checked: IndicatorState,
    on_check: Option<CheckFn>,
}

impl CheckboxListItem {
    /// Upstream `label`.
    pub fn new(label: impl Into<SharedString>) -> Self {
        CheckboxListItem {
            value: None,
            label: label.into(),
            aria_label: None,
            description: None,
            end_content: None,
            align: ItemAlign::Center,
            disabled: false,
            loading: false,
            checked: IndicatorState::Unchecked,
            on_check: None,
        }
    }

    astryx_primitives::setters! {
        /// Upstream `value`: its key in the list's `value`.
        ?value: SharedString,
        /// Upstream `aria-label`.
        ?aria_label: SharedString,
        /// Upstream `description`: one line, cut with an ellipsis.
        ?description: SharedString,
        /// Upstream `align`.
        align: ItemAlign,
        /// Upstream `isDisabled`.
        !disabled,
        /// Upstream `isLoading`.
        !loading,
    }

    /// Upstream `isChecked`, without a list `value`.
    pub fn checked(mut self, checked: impl Into<IndicatorState>) -> Self {
        self.checked = checked.into();
        self
    }

    /// Upstream `onCheck`, without a list `value`.
    pub fn on_check(mut self, f: impl Fn(bool, &mut Window, &mut App) + 'static) -> Self {
        self.on_check = Some(Rc::new(f));
        self
    }

    /// Upstream `endContent`.
    pub fn end_content(mut self, content: impl IntoElement) -> Self {
        self.end_content = Some(content.into_any_element());
        self
    }
}

/// Multiple choices from a list, upstream `CheckboxList`.
///
/// ```ignore
/// CheckboxList::new("toppings", "Toppings")
///     .value(selected.clone())
///     .item(CheckboxListItem::new("Cheese").value("cheese"))
///     .on_change(|values, _, _| { /* store them */ })
/// ```
#[derive(IntoElement)]
pub struct CheckboxList {
    id: ElementId,
    field: GroupField,
    value: Option<Vec<SharedString>>,
    on_change: Option<ChangeFn<Vec<SharedString>>>,
    change_action: Option<ActionFn<Vec<SharedString>>>,
    items: Vec<CheckboxListItem>,
    density: ItemDensity,
    dividers: bool,
    read_only: bool,
    style: StyleRefinement,
}

impl CheckboxList {
    /// Upstream `label`.
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        CheckboxList {
            id: id.into(),
            field: GroupField::new(label.into()),
            value: None,
            on_change: None,
            change_action: None,
            items: Vec::new(),
            density: ItemDensity::Balanced,
            dividers: false,
            read_only: false,
            style: StyleRefinement::default(),
        }
    }

    group_setters!();

    astryx_primitives::setters! {
        /// Upstream `density`.
        density: ItemDensity,
        /// Upstream `hasDividers`.
        !dividers,
        /// Upstream `isReadOnly`.
        !read_only,
    }

    /// Upstream `value`: the checked items' values (collection mode).
    pub fn value(mut self, value: impl IntoIterator<Item = impl Into<SharedString>>) -> Self {
        self.value = Some(value.into_iter().map(Into::into).collect());
        self
    }

    /// Upstream `onChange`: the next checked values.
    pub fn on_change(
        mut self,
        f: impl Fn(Vec<SharedString>, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_change = Some(Rc::new(f));
        self
    }

    /// Upstream `changeAction`: async work after `on_change`; the toggled
    /// item shows its new state and a spinner until the task finishes.
    pub fn change_action(
        mut self,
        f: impl Fn(Vec<SharedString>, &mut Window, &mut App) -> Task<()> + 'static,
    ) -> Self {
        self.change_action = Some(Rc::new(f));
        self
    }

    /// Upstream `children`: one box.
    pub fn item(mut self, item: CheckboxListItem) -> Self {
        self.items.push(item);
        self
    }

    /// Upstream `children`: boxes.
    pub fn items(mut self, items: impl IntoIterator<Item = CheckboxListItem>) -> Self {
        self.items.extend(items);
        self
    }
}

impl Styled for CheckboxList {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

/// The values after toggling `value`.
pub(crate) fn toggle_value(values: &[SharedString], value: &SharedString) -> Vec<SharedString> {
    if values.contains(value) {
        values.iter().filter(|v| *v != value).cloned().collect()
    } else {
        let mut next = values.to_vec();
        next.push(value.clone());
        next
    }
}

impl RenderOnce for CheckboxList {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let rem = window.rem_size();
        let optimistic = Optimistic::<Vec<SharedString>>::new(&self.id, window, cx);
        let pending = optimistic.pending(cx);
        let committed = self.value.clone();
        let shown = committed.clone().map(|v| optimistic.shown(v, cx));
        optimistic.announce_busy(pending, i18n(cx).t("@astryx.spinner.loading"), window, cx);
        let change: ChangeFn<Vec<SharedString>> =
            Rc::new(optimistic.change(self.on_change.clone(), self.change_action.clone()));

        let count = self.items.len();
        let (pressed, hover, muted) = (
            t.color.overlay_pressed,
            t.color.overlay_hover,
            t.color.accent_muted,
        );
        let rows = self.items.into_iter().enumerate().map(|(i, item)| {
            let state = match (&shown, &item.value) {
                (Some(values), Some(value)) => IndicatorState::from(values.contains(value)),
                _ => item.checked,
            };
            // The item an action is saving is the one whose shown state
            // differs from the committed one.
            let saving = pending
                && match (&committed, &item.value) {
                    (Some(values), Some(value)) => values.contains(value) != state.is_on(),
                    _ => false,
                };
            let busy = item.loading || saving;
            let disabled = item.disabled || self.field.disabled;
            let inert = disabled || busy || self.read_only;
            let row_id = ElementId::Name(format!("{:?}-{i}", self.id).into());
            let group: SharedString = format!("{row_id:?}-row").into();
            let focus = astryx_primitives::control_focus(&row_id, window, cx);

            let mut indicator = CheckboxIndicator::new(state)
                .disabled(disabled)
                .hover_group(group.clone());
            if busy {
                indicator = indicator.child(
                    Spinner::new(ElementId::Name(format!("{row_id:?}-spinner").into()))
                        .size(SpinnerSize::Sm)
                        .shade(SpinnerShade::Inherit),
                );
            }
            let indicator = div()
                .relative()
                .flex()
                .flex_none()
                .child(indicator)
                .when(is_focus_visible(&focus, window), |el| {
                    el.child(focus_ring(t, t.radius.inner.to_pixels(rem), rem))
                });
            let row_item = Item::new(row_id.clone(), item.label.clone())
                .bare(true)
                .disabled(disabled)
                .align(item.align)
                .label_lines(1usize)
                .description_lines(1usize)
                .start_content(indicator)
                .end_content_opt(item.end_content)
                .description_opt(item.description.map(|d| d.into_any_element()));

            let (block, inline) = match self.density {
                ItemDensity::Compact => (t.spacing.s1, t.spacing.s2),
                ItemDensity::Balanced => (t.spacing.s2, t.spacing.s2),
                ItemDensity::Spacious => (t.spacing.s3, t.spacing.s3),
            };
            let checked = state.is_on();
            let rest = if checked {
                muted
            } else {
                gpui::transparent_black()
            };
            let mut row = div()
                .id(row_id)
                .group(group)
                .track_focus(&focus)
                .py(block)
                .px(inline)
                .rounded(t.radius.element)
                .bg(rest)
                .child(row_item);
            if self.dividers && i + 1 < count {
                row = row.border_b(t.border.width).border_color(t.color.border);
            }
            if inert {
                row = row.on_click(|_, _, cx| cx.stop_propagation());
            } else {
                let next = !(state == IndicatorState::Checked);
                let toggle: ToggleFn = match (&shown, item.value.clone(), item.on_check.clone()) {
                    (Some(values), Some(value), _) => {
                        let (values, change) = (values.clone(), change.clone());
                        Rc::new(move |window, cx| change(toggle_value(&values, &value), window, cx))
                    }
                    (_, _, Some(on_check)) => Rc::new(move |window, cx| on_check(next, window, cx)),
                    _ => Rc::new(|_, _| {}),
                };
                row = row
                    .cursor_pointer()
                    .hover(move |s| s.bg(composite(rest, hover)))
                    .active(move |s| s.bg(composite(rest, pressed)))
                    .on_click(move |event, window, cx| {
                        if toggles(event) {
                            toggle(window, cx);
                        }
                    });
            }
            row.role(Role::CheckBox)
                .aria_label(item.aria_label.unwrap_or(item.label))
                .aria_toggled(match state {
                    IndicatorState::Unchecked => Toggled::False,
                    IndicatorState::Checked => Toggled::True,
                    IndicatorState::Indeterminate => Toggled::Mixed,
                })
        });

        let mut list = div().flex().flex_col();
        if !self.dividers {
            list = list.gap(t.spacing.s0_5);
        }
        theme.refine(
            list.style(),
            "checkbox-list",
            &[("density", self.density.as_str())],
            None,
        );
        list.style().refine(&self.style);
        let list = list
            .id(ElementId::Name(format!("{:?}-group", self.id).into()))
            .role(Role::Group)
            .aria_label(self.field.label.clone())
            .children(rows);
        self.field.field(&self.id).child(list)
    }
}
