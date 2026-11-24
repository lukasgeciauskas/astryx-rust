//! ToggleButton and ToggleButtonGroup. Upstream: `ToggleButton/`.

use std::rc::Rc;

use astryx_i18n::{LogicalStyled, use_direction};
use astryx_primitives::{ElementSize, provide, resolve_size, use_context};
use astryx_theme::current;
use gpui::{
    AnyElement, App, ClickEvent, ElementId, IntoElement, ParentElement, RenderOnce, Role,
    SharedString, StyleRefinement, Styled, Task, Window, div, prelude::*,
};
use smallvec::SmallVec;

use super::{Button, ButtonVariant};
use crate::containers::Elevation;
use crate::layout::Orientation;

type PressedChange = Rc<dyn Fn(bool, &ClickEvent, &mut Window, &mut App)>;
type ToggleFn = Rc<dyn Fn(&SharedString, &mut Window, &mut App)>;
type PressedAction = Rc<dyn Fn(bool, &mut Window, &mut App) -> Task<()>>;

/// What a [`ToggleButtonGroup`] gives its members.
#[derive(Clone)]
struct GroupContext {
    selected: Rc<[SharedString]>,
    toggle: ToggleFn,
    disabled: bool,
}

/// A button with pressed and unpressed states, upstream `ToggleButton`.
#[derive(IntoElement)]
pub struct ToggleButton {
    id: ElementId,
    label: SharedString,
    pressed: bool,
    on_pressed_change: Option<PressedChange>,
    pressed_change_action: Option<PressedAction>,
    size: Option<ElementSize>,
    elevation: Elevation,
    disabled: bool,
    loading: bool,
    icon: Option<AnyElement>,
    icon_only: bool,
    pressed_icon: Option<AnyElement>,
    tooltip: Option<SharedString>,
    value: Option<SharedString>,
    style: StyleRefinement,
    children: SmallVec<[AnyElement; 1]>,
}

impl ToggleButton {
    /// Upstream `label`: the visible text, or the name of an icon-only toggle.
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        ToggleButton {
            id: id.into(),
            label: label.into(),
            pressed: false,
            on_pressed_change: None,
            pressed_change_action: None,
            size: None,
            elevation: Elevation::None,
            disabled: false,
            loading: false,
            icon: None,
            icon_only: false,
            pressed_icon: None,
            tooltip: None,
            value: None,
            style: StyleRefinement::default(),
            children: SmallVec::new(),
        }
    }

    astryx_primitives::setters! {
        /// Upstream `isPressed`. In a group, the group's value decides.
        !pressed,
        /// Upstream `size`: else the group's, else `md`.
        ?size: ElementSize,
        /// Upstream `elevation`.
        elevation: Elevation,
        /// Upstream `isDisabled`.
        !disabled,
        /// Upstream `isLoading`.
        !loading,
        /// Upstream `isIconOnly`.
        !icon_only,
        /// Upstream `tooltip`. The tooltip itself lands with Tooltip (M5).
        ?tooltip: SharedString,
        /// Upstream `value`: this member's value in a ToggleButtonGroup.
        ?value: SharedString,
    }

    /// Upstream `onPressedChange`: the next state and the click.
    pub fn on_pressed_change(
        mut self,
        f: impl Fn(bool, &ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_pressed_change = Some(Rc::new(f));
        self
    }

    /// Upstream `pressedChangeAction`: async work after `on_pressed_change`;
    /// the toggle is busy until the returned task finishes. A group member
    /// ignores it, since the group owns the state.
    pub fn pressed_change_action(
        mut self,
        f: impl Fn(bool, &mut Window, &mut App) -> Task<()> + 'static,
    ) -> Self {
        self.pressed_change_action = Some(Rc::new(f));
        self
    }

    /// Upstream `icon`. Without children, the toggle is icon-only.
    pub fn icon(mut self, icon: impl IntoElement) -> Self {
        self.icon = Some(icon.into_any_element());
        self
    }

    /// Upstream `pressedIcon`: shown while pressed.
    pub fn pressed_icon(mut self, icon: impl IntoElement) -> Self {
        self.pressed_icon = Some(icon.into_any_element());
        self
    }
}

impl Styled for ToggleButton {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl ParentElement for ToggleButton {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for ToggleButton {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let group = use_context::<GroupContext>(cx);
        let in_group = group.as_ref().zip(self.value.clone());
        let pressed = match &in_group {
            Some((group, value)) => group.selected.contains(value),
            None => self.pressed,
        };
        let disabled = self.disabled || group.as_ref().is_some_and(|g| g.disabled);
        let icon_only = self.icon_only || (self.icon.is_some() && self.children.is_empty());
        let icon = if pressed {
            self.pressed_icon.or(self.icon)
        } else {
            self.icon
        };
        // Upstream renders a ghost Button that stays operable while its
        // action runs.
        let mut button = Button::new(self.id, self.label.clone())
            .variant(ButtonVariant::Ghost)
            .interruptible(true)
            .pressed(pressed)
            .elevation(self.elevation)
            .disabled(disabled)
            .loading(self.loading)
            .icon_only(icon_only);
        if let Some(size) = self.size {
            button = button.size(size);
        }
        if let Some(icon) = icon {
            button = button.icon(icon);
        }
        if let Some(tooltip) = self.tooltip {
            button = button.tooltip(tooltip);
        }
        button = match in_group {
            Some((group, value)) => {
                let toggle = group.toggle.clone();
                button.on_click(move |_, window, cx| toggle(&value, window, cx))
            }
            None => {
                let change = self.on_pressed_change.clone();
                let button = button.on_click(move |event, window, cx| {
                    if let Some(change) = &change {
                        change(!pressed, event, window, cx);
                    }
                });
                match self.pressed_change_action {
                    Some(action) => {
                        button.click_action(move |_, window, cx| action(!pressed, window, cx))
                    }
                    None => button,
                }
            }
        };
        let mut style = StyleRefinement::default();
        theme.refine(
            &mut style,
            "toggle-button",
            &[
                ("isPressed", if pressed { "true" } else { "false" }),
                ("elevation", self.elevation.as_str()),
                ("grouped", if group.is_some() { "grouped" } else { "" }),
            ],
            None,
        );
        style.refine(&self.style);
        *button.style() = style;
        if !self.children.is_empty() {
            return button.children(self.children);
        }
        if icon_only {
            return button;
        }
        // The label, plus a hidden semibold copy that reserves the pressed
        // width, so pressing never shifts the row (upstream `labelStyles`).
        let label = |text: SharedString| {
            div()
                .max_w_full()
                .min_w_0()
                .overflow_hidden()
                .text_ellipsis()
                .whitespace_nowrap()
                .child(text)
        };
        button.child(
            div()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .max_w_full()
                .min_w_0()
                .child(label(self.label.clone()))
                .child(
                    label(self.label)
                        .h_0()
                        .invisible()
                        .font_weight(t.font.weight.semibold),
                ),
        )
    }
}

/// Upstream ToggleButtonGroup `type`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SelectionMode {
    /// One value at most; pressing the pressed one clears it.
    #[default]
    Single,
    /// Any number of values.
    Multiple,
}

type ChangeFn = Rc<dyn Fn(Vec<SharedString>, &mut Window, &mut App)>;

/// Toggle buttons sharing one selection, upstream `ToggleButtonGroup`.
#[derive(IntoElement)]
pub struct ToggleButtonGroup {
    id: ElementId,
    label: SharedString,
    mode: SelectionMode,
    value: Vec<SharedString>,
    on_change: Option<ChangeFn>,
    orientation: Orientation,
    size: Option<ElementSize>,
    disabled: bool,
    style: StyleRefinement,
    children: SmallVec<[AnyElement; 4]>,
}

impl ToggleButtonGroup {
    /// Upstream `label` and `type`.
    pub fn new(
        id: impl Into<ElementId>,
        label: impl Into<SharedString>,
        mode: SelectionMode,
    ) -> Self {
        ToggleButtonGroup {
            id: id.into(),
            label: label.into(),
            mode,
            value: Vec::new(),
            on_change: None,
            orientation: Orientation::Horizontal,
            size: None,
            disabled: false,
            style: StyleRefinement::default(),
            children: SmallVec::new(),
        }
    }

    /// Upstream `value`: the selected values (one at most in single mode).
    pub fn value(mut self, value: impl IntoIterator<Item = impl Into<SharedString>>) -> Self {
        self.value = value.into_iter().map(Into::into).collect();
        self
    }

    /// Upstream `onChange`: the next selection.
    pub fn on_change(
        mut self,
        f: impl Fn(Vec<SharedString>, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_change = Some(Rc::new(f));
        self
    }

    astryx_primitives::setters! {
        /// Upstream `orientation`.
        orientation: Orientation,
        /// Upstream `size`: the members' default.
        ?size: ElementSize,
        /// Upstream `isDisabled`.
        !disabled,
    }
}

/// The selection after toggling `value`.
pub(crate) fn toggled(
    mode: SelectionMode,
    selected: &[SharedString],
    value: &SharedString,
) -> Vec<SharedString> {
    let on = selected.contains(value);
    match (mode, on) {
        (SelectionMode::Single, true) => Vec::new(),
        (SelectionMode::Single, false) => vec![value.clone()],
        (SelectionMode::Multiple, true) => {
            selected.iter().filter(|v| *v != value).cloned().collect()
        }
        (SelectionMode::Multiple, false) => {
            let mut next = selected.to_vec();
            next.push(value.clone());
            next
        }
    }
}

impl Styled for ToggleButtonGroup {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl ParentElement for ToggleButtonGroup {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for ToggleButtonGroup {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let dir = use_direction(cx);
        let size = resolve_size(self.size, ElementSize::Md, cx);
        let selected: Rc<[SharedString]> = self.value.into();
        let mode = self.mode;
        let on_change = self.on_change.clone();
        let current_selection = selected.clone();
        let context = GroupContext {
            selected,
            disabled: self.disabled,
            toggle: Rc::new(move |value, window, cx| {
                if let Some(f) = &on_change {
                    f(toggled(mode, &current_selection, value), window, cx);
                }
            }),
        };
        let mut el = match self.orientation {
            Orientation::Horizontal => div().flex_row_logical(dir).flex_wrap().items_center(),
            Orientation::Vertical => div().flex().flex_col(),
        };
        el = el.gap(t.spacing.s1).max_w_full();
        theme.refine(
            el.style(),
            "toggle-button-group",
            &[
                ("size", size.as_str()),
                ("orientation", self.orientation.as_str()),
            ],
            None,
        );
        el.style().refine(&self.style);
        el.id(self.id)
            .role(Role::Group)
            .aria_label(self.label)
            .children(
                self.children
                    .into_iter()
                    .map(move |child| provide(context.clone(), provide(size, child))),
            )
    }
}
