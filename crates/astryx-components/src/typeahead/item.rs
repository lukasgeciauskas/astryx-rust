//! TypeaheadItem, upstream `Typeahead/TypeaheadItem.tsx`: a result's default
//! content, an icon before its label and description.

use astryx_theme::current;
use gpui::{
    AnyElement, App, IntoElement, ParentElement, RenderOnce, SharedString, StyleRefinement, Styled,
    Window, div, prelude::*, relative,
};

use crate::search::Searchable;

/// A result's content, upstream `TypeaheadItem`.
#[derive(IntoElement)]
pub struct TypeaheadItem {
    label: SharedString,
    element: Option<AnyElement>,
    icon: Option<AnyElement>,
    description: Option<SharedString>,
    disabled: bool,
    style: StyleRefinement,
}

impl TypeaheadItem {
    /// Upstream `item`'s `label`.
    pub fn new(label: impl Into<SharedString>) -> Self {
        TypeaheadItem {
            label: label.into(),
            element: None,
            icon: None,
            description: None,
            disabled: false,
            style: StyleRefinement::default(),
        }
    }

    /// Upstream `item`: its label.
    pub fn for_item(item: &impl Searchable) -> Self {
        Self::new(item.label())
    }

    /// Upstream `item.element`: drawn raw in place of the layout.
    pub fn element(mut self, element: impl IntoElement) -> Self {
        self.element = Some(element.into_any_element());
        self
    }

    /// Upstream `icon`: an icon or avatar before the label.
    pub fn icon(mut self, icon: impl IntoElement) -> Self {
        self.icon = Some(icon.into_any_element());
        self
    }

    astryx_primitives::setters! {
        /// Upstream `description`: under the label.
        ?description: SharedString,
        /// Upstream `isDisabled`: half opacity.
        !disabled,
    }
}

impl Styled for TypeaheadItem {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for TypeaheadItem {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        if let Some(element) = self.element {
            return element;
        }
        let text = div()
            .flex()
            .flex_col()
            .min_w_0()
            .child(
                div()
                    .overflow_hidden()
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .text_size(t.text.body.size)
                    .line_height(relative(t.text.body.leading))
                    .text_color(t.color.text_primary)
                    .child(self.label),
            )
            .children(self.description.map(|d| {
                div()
                    .text_size(t.text.supporting.size)
                    .line_height(relative(t.text.supporting.leading))
                    .text_color(t.color.text_secondary)
                    .child(d)
            }));
        let mut el = div()
            .flex()
            .items_center()
            .gap(t.spacing.s2)
            .min_w_0()
            .w_full()
            .children(self.icon)
            .child(text);
        if self.disabled {
            el = el.opacity(0.5);
        }
        theme.refine(el.style(), "typeahead-item", &[], None);
        el.style().refine(&self.style);
        el.into_any_element()
    }
}
