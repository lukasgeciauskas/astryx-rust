//! SideNavSection, upstream `SideNav/SideNavSection.tsx`: a labelled group of items
//! with a title, subtitle and end content. The header hides in a collapsed rail and
//! can be hidden visually while it still names the group.

use gpui::{
    AnyElement, App, ElementId, FontWeight, IntoElement, ParentElement, RenderOnce, Role,
    SharedString, StyleRefinement, Styled, Window, div, prelude::*,
};
use rustryx_theme::{ThemeStyled, current};

use super::SideNavCollapse;
use rustryx_primitives::use_context;

/// Upstream `SideNavSection`.
#[derive(IntoElement)]
pub struct SideNavSection {
    id: ElementId,
    title: SharedString,
    subtitle: Option<SharedString>,
    end_content: Option<AnyElement>,
    header_hidden: bool,
    children: Vec<AnyElement>,
    style: StyleRefinement,
}

impl SideNavSection {
    pub fn new(id: impl Into<ElementId>, title: impl Into<SharedString>) -> Self {
        SideNavSection {
            id: id.into(),
            title: title.into(),
            subtitle: None,
            end_content: None,
            header_hidden: false,
            children: Vec::new(),
            style: StyleRefinement::default(),
        }
    }

    /// Upstream `subtitle`.
    pub fn subtitle(mut self, subtitle: impl Into<SharedString>) -> Self {
        self.subtitle = Some(subtitle.into());
        self
    }

    /// Upstream `endContent`.
    pub fn end_content(mut self, content: impl IntoElement) -> Self {
        self.end_content = Some(content.into_any_element());
        self
    }

    /// Upstream `isHeaderHidden`: hidden visually, still the group's name.
    pub fn header_hidden(mut self, hidden: bool) -> Self {
        self.header_hidden = hidden;
        self
    }
}

impl ParentElement for SideNavSection {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl Styled for SideNavSection {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for SideNavSection {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens().clone();
        let rail = use_context::<SideNavCollapse>(cx);
        let collapsed = rail.as_ref().is_some_and(|c| c.collapsed);
        // The rail's labels fade as it collapses or expands (SideNav FR6).
        let labels = rail.as_ref().map_or(1., |c| c.labels);
        let line = |text: SharedString, weight: FontWeight| {
            div()
                .opacity(labels)
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .text_style(&t.text.supporting)
                .font_weight(weight)
                .text_color(t.color.text_secondary)
                .child(text)
        };
        let header = (!self.header_hidden && !collapsed).then(|| {
            div()
                .flex()
                .items_center()
                .gap(t.spacing.s2)
                .px(t.spacing.s2)
                .py(t.spacing.s1)
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .flex_1()
                        .min_w_0()
                        .child(line(self.title.clone(), FontWeight::SEMIBOLD))
                        .children(self.subtitle.clone().map(|s| line(s, FontWeight::NORMAL))),
                )
                .children(
                    self.end_content
                        .map(|c| div().flex().flex_none().items_center().child(c)),
                )
        });
        let mut root = div()
            .id(self.id.clone())
            .flex()
            .flex_col()
            .py(t.spacing.s1)
            .role(Role::Group)
            .aria_label(self.title.clone());
        theme.refine(root.style(), "side-nav-section", &[], None);
        root.style().refine(&self.style);
        root.children(header).child(
            div()
                .flex()
                .flex_col()
                .gap(t.spacing.s0_5)
                .when(collapsed, |d| d.items_center())
                .children(self.children),
        )
    }
}
