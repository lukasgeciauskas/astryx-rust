//! SelectorOption, upstream `Selector/SelectorOption.tsx`: the content row
//! a Selector draws for each option, and that `render_option` and
//! `render_value` can return to look like the built-in rows.

use gpui::{
    AnyElement, App, ElementId, IntoElement, RenderOnce, SharedString, StyleRefinement, Styled,
    Window, prelude::*, px,
};
use rustryx_primitives::use_context;
use rustryx_theme::current;

use crate::icon::{Icon, IconColor, IconSize, IconSource};
use crate::item::{Item, ItemLayout};

/// The row layout a host imposes on the options it draws, upstream
/// `SelectorRowLayoutContext`: a Selector trigger inside an InputGroup is
/// one line tall, so its value folds inline.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SelectorRowLayout(pub ItemLayout);

/// A Selector option's content: an icon, a label, a description and end
/// content on an Item row, upstream `SelectorOption`.
///
/// ```ignore
/// Selector::new("owner", "Owner", owners)
///     .render_option(|option, _, _| {
///         SelectorOption::new(option.label.clone())
///             .description(option.description.clone().unwrap_or_default())
///             .into_any_element()
///     })
/// ```
#[derive(IntoElement)]
pub struct SelectorOption {
    label: AnyElement,
    label_text: Option<SharedString>,
    icon: Option<AnyElement>,
    description: Option<AnyElement>,
    layout: Option<ItemLayout>,
    end_content: Option<AnyElement>,
    style: StyleRefinement,
}

impl SelectorOption {
    /// Upstream `label` as text: one line, ellipsized in a narrow host.
    pub fn new(label: impl Into<SharedString>) -> Self {
        let label: SharedString = label.into();
        SelectorOption {
            label: label.clone().into_any_element(),
            label_text: Some(label),
            icon: None,
            description: None,
            layout: None,
            end_content: None,
            style: StyleRefinement::default(),
        }
    }

    /// Upstream `label` as any element, which wraps rather than ellipsizes.
    pub fn new_element(label: impl IntoElement) -> Self {
        SelectorOption {
            label: label.into_any_element(),
            label_text: None,
            icon: None,
            description: None,
            layout: None,
            end_content: None,
            style: StyleRefinement::default(),
        }
    }

    /// Upstream `icon`: a built-in name or SVG path, drawn small and
    /// secondary before the label.
    pub fn icon(mut self, icon: impl Into<IconSource>) -> Self {
        self.icon = Some(
            Icon::new(icon)
                .size(IconSize::Sm)
                .color(IconColor::Secondary)
                .into_any_element(),
        );
        self
    }

    /// Upstream `icon` as any other element, drawn as given.
    pub fn icon_element(mut self, icon: impl IntoElement) -> Self {
        self.icon = Some(icon.into_any_element());
        self
    }

    /// Upstream `description`: secondary text under (or after) the label.
    pub fn description(mut self, description: impl IntoElement) -> Self {
        self.description = Some(description.into_any_element());
        self
    }

    /// Upstream `layout`: stacked by default. A host that pins the row's
    /// height (a trigger in an InputGroup) makes it inline whatever is set
    /// here (FR2).
    pub fn layout(mut self, layout: ItemLayout) -> Self {
        self.layout = Some(layout);
        self
    }

    /// Upstream `endContent`: after the label and description.
    pub fn end_content(mut self, content: impl IntoElement) -> Self {
        self.end_content = Some(content.into_any_element());
        self
    }
}

impl Styled for SelectorOption {
    /// Upstream `xstyle`, `className` and `style`: after the row's own
    /// reset (ORD2).
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for SelectorOption {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        // ORD1: the host's inline, else the caller's layout, else stacked.
        let layout = match use_context::<SelectorRowLayout>(cx) {
            Some(SelectorRowLayout(ItemLayout::Inline)) => ItemLayout::Inline,
            _ => self.layout.unwrap_or_default(),
        };
        let id = ElementId::Name(
            format!(
                "selector-option-{}",
                self.label_text.as_deref().unwrap_or("content")
            )
            .into(),
        );
        let mut item = Item::new(id, self.label).layout(layout);
        if self.label_text.is_some() {
            item = item.label_lines(1usize);
        }
        if let Some(icon) = self.icon {
            item = item.start_content(icon);
        }
        if let Some(description) = self.description {
            item = item.description(description);
        }
        if let Some(end) = self.end_content {
            item = item.end_content(end);
        }
        // FR3: no padding, corners or least width of its own, so a long label
        // ellipsizes inside a narrow host; then the theme, then the caller.
        item = item.p(px(0.)).rounded(px(0.)).min_w_0();
        theme.refine(item.style(), "selector-option", &[], None);
        item.style().refine(&self.style);
        item
    }
}
