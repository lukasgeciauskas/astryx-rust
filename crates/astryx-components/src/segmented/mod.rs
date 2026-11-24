//! SegmentedControl. Upstream: `SegmentedControl/`.
//!
//! With a pointer, selection restyles the chosen segment. Upstream's
//! sliding lens is part of its touch presentation and lands with the touch
//! layer (M11).

pub mod examples;

#[cfg(test)]
pub(crate) mod tests;

use std::rc::Rc;

use astryx_i18n::use_direction;
use astryx_primitives::{
    ElementSize, ListFocus, ListKeys, is_focus_visible, provide, resolve_size,
};
use astryx_theme::{ThemeStyled, current};
use gpui::{
    AnyElement, App, ElementId, IntoElement, ParentElement, RenderOnce, Role, SharedString,
    StyleRefinement, Styled, Toggled, Window, div, prelude::*, px, relative,
};

use crate::button::element_height;
use crate::choice::row_toggles;
use crate::doc::{ComponentDoc, PropMapping, RustApi};
use crate::focus::focus_ring;
use crate::icon::{IconDefaultSize, IconSize};

/// Upstream `layout`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SegmentLayout {
    /// Each segment as wide as its content, capped at the container.
    #[default]
    Hug,
    /// Segments share the full width equally.
    Fill,
}

impl SegmentLayout {
    pub fn as_str(self) -> &'static str {
        match self {
            SegmentLayout::Hug => "hug",
            SegmentLayout::Fill => "fill",
        }
    }
}

/// One segment, upstream `SegmentedControlItem`.
pub struct SegmentedControlItem {
    value: SharedString,
    label: SharedString,
    label_hidden: bool,
    icon: Option<AnyElement>,
    disabled: bool,
}

impl SegmentedControlItem {
    /// Upstream `value` and `label`.
    pub fn new(value: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        SegmentedControlItem {
            value: value.into(),
            label: label.into(),
            label_hidden: false,
            icon: None,
            disabled: false,
        }
    }

    astryx_primitives::setters! {
        /// Upstream `isLabelHidden`: icon only; the label names it.
        !label_hidden,
        /// Upstream `isDisabled`.
        !disabled,
    }

    /// Upstream `icon`: before the label.
    pub fn icon(mut self, icon: impl IntoElement) -> Self {
        self.icon = Some(icon.into_any_element());
        self
    }
}

type SelectFn = Rc<dyn Fn(SharedString, &mut Window, &mut App)>;

/// A single choice among a few options shown side by side, upstream
/// `SegmentedControl`.
///
/// ```ignore
/// SegmentedControl::new("view", "View", "list")
///     .item(SegmentedControlItem::new("list", "List"))
///     .item(SegmentedControlItem::new("grid", "Grid"))
///     .on_change(|value, _, _| { /* store it */ })
/// ```
#[derive(IntoElement)]
pub struct SegmentedControl {
    id: ElementId,
    label: SharedString,
    value: SharedString,
    on_change: Option<SelectFn>,
    size: Option<ElementSize>,
    layout: SegmentLayout,
    disabled: bool,
    disabled_message: Option<SharedString>,
    items: Vec<SegmentedControlItem>,
    style: StyleRefinement,
}

impl SegmentedControl {
    /// Upstream `label` (its accessible name, never shown) and `value`.
    pub fn new(
        id: impl Into<ElementId>,
        label: impl Into<SharedString>,
        value: impl Into<SharedString>,
    ) -> Self {
        SegmentedControl {
            id: id.into(),
            label: label.into(),
            value: value.into(),
            on_change: None,
            size: None,
            layout: SegmentLayout::Hug,
            disabled: false,
            disabled_message: None,
            items: Vec::new(),
            style: StyleRefinement::default(),
        }
    }

    astryx_primitives::setters! {
        /// Upstream `size`: else the nearest size provider, else `md`.
        ?size: ElementSize,
        /// Upstream `layout`.
        layout: SegmentLayout,
        /// Upstream `isDisabled`: every segment.
        !disabled,
        /// Upstream `disabledMessage`. The tooltip lands with Tooltip (M5).
        ?disabled_message: SharedString,
    }

    /// Upstream `onChange`: the newly selected value.
    pub fn on_change(mut self, f: impl Fn(SharedString, &mut Window, &mut App) + 'static) -> Self {
        self.on_change = Some(Rc::new(f));
        self
    }

    /// Upstream `children`: one segment.
    pub fn item(mut self, item: SegmentedControlItem) -> Self {
        self.items.push(item);
        self
    }

    /// Upstream `children`: segments.
    pub fn items(mut self, items: impl IntoIterator<Item = SegmentedControlItem>) -> Self {
        self.items.extend(items);
        self
    }
}

impl Styled for SegmentedControl {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for SegmentedControl {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let rem = window.rem_size();
        let dir = use_direction(cx);
        let size = resolve_size(self.size, ElementSize::Md, cx);
        let disabled: Vec<bool> = self
            .items
            .iter()
            .map(|i| i.disabled || self.disabled)
            .collect();
        let values: Rc<[SharedString]> = self.items.iter().map(|i| i.value.clone()).collect();
        let selected = values.iter().position(|v| *v == self.value);

        // Arrows move focus and select (upstream: selection follows focus).
        let on_change = self.on_change.clone();
        let move_values = values.clone();
        let list = ListFocus::new(
            &self.id,
            self.items.len(),
            ListKeys::horizontal(),
            window,
            cx,
        )
        .disabled(disabled.iter().copied())
        .rtl(dir.is_rtl())
        .on_move(move |index, window, cx| {
            if let Some(f) = &on_change {
                f(move_values[index].clone(), window, cx);
            }
        });
        let tab_stop = list.tab_stop(selected, window);

        let pad = t.spacing.s0_5.to_pixels(rem);
        let radius = t.radius.element.to_pixels(rem);
        let inner_radius = (radius - pad).max(px(0.));
        let height = element_height(size, t).to_pixels(rem) - px(4.);
        let (text, inline, icon_size) = match size {
            ElementSize::Sm => (&t.text.supporting, t.spacing.s2, px(14.)),
            ElementSize::Md => (&t.text.label, t.spacing.s3, px(16.)),
            ElementSize::Lg => (&t.text.label, t.spacing.s3, px(18.)),
        };
        let fill = self.layout == SegmentLayout::Fill;
        let (hover, pressed) = (t.color.overlay_hover, t.color.overlay_pressed);

        let segments = self.items.into_iter().enumerate().map(|(i, item)| {
            let is_selected = selected == Some(i);
            let item_disabled = disabled[i];
            let handle = list.item_handle(i, tab_stop);
            let line = text.size.to_pixels(rem) * text.leading;
            let mut seg = div()
                .relative()
                .flex()
                .flex_row()
                .items_center()
                .justify_center()
                .gap(t.spacing.s1)
                .h(height.max(line))
                .rounded(inner_radius)
                .text_size(text.size)
                .line_height(relative(text.leading))
                .font_weight(t.font.weight.medium)
                .text_color(t.color.text_secondary)
                .whitespace_nowrap()
                .min_w_0();
            seg = if item.label_hidden {
                seg.aspect_ratio(1.).flex_none()
            } else {
                seg.px(inline)
            };
            if fill {
                seg = seg.flex_1();
            }
            if is_selected {
                seg = seg
                    .text_color(t.color.text_primary)
                    .font_weight(t.font.weight.semibold)
                    .bg(t.color.background_surface)
                    .shadow_token(&t.shadow.low);
            }
            if item_disabled {
                seg = seg.text_color(t.color.text_disabled);
            }
            theme.refine(
                seg.style(),
                "segmented-control-item",
                &[
                    ("size", size.as_str()),
                    ("selected", if is_selected { "selected" } else { "" }),
                    ("disabled", if item_disabled { "disabled" } else { "" }),
                ],
                None,
            );
            let icon_el = item.icon.map(|icon| {
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .justify_center()
                    .size(icon_size)
                    .child(provide(IconDefaultSize(IconSize::Sm), icon))
            });
            let label = (!item.label_hidden).then(|| {
                div()
                    .min_w_0()
                    .overflow_hidden()
                    .text_ellipsis()
                    .child(item.label.clone())
            });
            let mut seg = seg
                .id(ElementId::Name(
                    format!("{:?}-{}", self.id, item.value).into(),
                ))
                .track_focus(&handle)
                .children(icon_el)
                .children(label);
            if item_disabled {
                seg = seg.on_click(|_, _, cx| cx.stop_propagation());
            } else {
                if !is_selected {
                    seg = seg
                        .hover(move |s| s.bg(hover))
                        .active(move |s| s.bg(pressed));
                }
                let select = self.on_change.clone();
                let value = item.value.clone();
                seg = seg.cursor_pointer().on_click(move |event, window, cx| {
                    cx.stop_propagation();
                    if row_toggles(event)
                        && !is_selected
                        && let Some(f) = &select
                    {
                        f(value.clone(), window, cx);
                    }
                });
            }
            if is_focus_visible(&handle, window) {
                seg = seg.child(focus_ring(t, inner_radius, rem));
            }
            seg.role(Role::RadioButton)
                .aria_label(item.label)
                .aria_toggled(if is_selected {
                    Toggled::True
                } else {
                    Toggled::False
                })
        });

        let mut el = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(t.spacing.s0_5)
            .p(t.spacing.s0_5)
            .rounded(t.radius.element)
            .bg(t.color.neutral);
        el = if fill {
            el.w_full()
        } else {
            el.flex_none().max_w_full()
        };
        if self.disabled {
            el = el.opacity(0.5);
        }
        theme.refine(
            el.style(),
            "segmented-control",
            &[("size", size.as_str()), ("layout", self.layout.as_str())],
            None,
        );
        el.style().refine(&self.style);
        let el = el
            .id(self.id.clone())
            .on_key_down(list.key_down_listener())
            .role(Role::RadioGroup)
            .aria_label(self.label)
            .children(segments);
        // Upstream sizes a hugging control `fit-content`: a flex row around
        // it keeps it to its content even where the parent stretches.
        div()
            .flex()
            .flex_row()
            .max_w_full()
            .when(fill, |w| w.w_full())
            .child(el)
    }
}

// ----------------------------------------------------------------------------
// Docs

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

pub const SEGMENTED_CONTROL_DOC: ComponentDoc = ComponentDoc {
    name: "SegmentedControl",
    rust_type: "astryx::components::segmented::SegmentedControl",
    props: &[
        p("value", RustApi::Argument("value")),
        p("onChange", RustApi::Method("on_change")),
        p("label", RustApi::Argument("label")),
        p("size", RustApi::Method("size")),
        p("layout", RustApi::Method("layout")),
        p("isDisabled", RustApi::Method("disabled")),
        p(
            "disabledMessage",
            RustApi::Pending("disabled_message", "the tooltip lands with Tooltip in M5"),
        ),
        p("children", RustApi::Method("item / items")),
        p("xstyle", RustApi::Trait("Styled")),
    ],
    example: include_str!("examples.rs"),
};

pub const SEGMENTED_CONTROL_ITEM_DOC: ComponentDoc = ComponentDoc {
    name: "SegmentedControlItem",
    rust_type: "astryx::components::segmented::SegmentedControlItem",
    props: &[
        p("value", RustApi::Argument("value")),
        p("label", RustApi::Argument("label")),
        p("isLabelHidden", RustApi::Method("label_hidden")),
        p("icon", RustApi::Method("icon")),
        p("isDisabled", RustApi::Method("disabled")),
    ],
    example: include_str!("examples.rs"),
};
