//! `ChatUnreadDivider`: a thin error-coloured rule with a trailing label ("New" by
//! default) marking where unread messages begin in a chat thread (upstream lab
//! `ChatUnreadDivider`), a separator with an accessible label.

use gpui::prelude::*;
use gpui::{App, ElementId, IntoElement, Role, SharedString, StyleRefinement, Window, div, px};
use rustryx_theme::{ThemeStyled, current};

/// The divider's accessible name for a label (upstream: "{label} messages below").
pub fn unread_divider_name(label: &str) -> SharedString {
    format!("{label} messages below").into()
}

/// The rule marking the first unread message (upstream lab `ChatUnreadDivider`).
///
/// ```ignore
/// ChatUnreadDivider::new("unread").label("Unread")
/// ```
#[derive(IntoElement)]
pub struct ChatUnreadDivider {
    id: ElementId,
    label: SharedString,
    style: StyleRefinement,
}

impl ChatUnreadDivider {
    /// A divider labelled "New".
    pub fn new(id: impl Into<ElementId>) -> Self {
        ChatUnreadDivider {
            id: id.into(),
            label: "New".into(),
            style: StyleRefinement::default(),
        }
    }

    /// Upstream `label` ("New" by default).
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = label.into();
        self
    }
}

impl Styled for ChatUnreadDivider {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

// What a divider drew, for tests: its label and its accessible name.
#[cfg(test)]
thread_local! {
    pub(crate) static DIVIDERS: std::cell::RefCell<std::collections::HashMap<String, (SharedString, SharedString)>> = Default::default();
}

impl RenderOnce for ChatUnreadDivider {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let rem = window.rem_size();
        let mut root = div()
            .flex()
            .items_center()
            .gap(t.spacing.s2)
            .w_full()
            .py(t.spacing.s1);
        theme.refine(root.style(), "chat-unread-divider", &[], None);
        root.style().refine(&self.style);
        let selector = format!("{}", self.id);
        let leading = t.text.supporting.size.to_pixels(rem) * t.text.supporting.leading;
        #[cfg(test)]
        DIVIDERS.with(|d| {
            d.borrow_mut().insert(
                self.id.to_string(),
                (self.label.clone(), unread_divider_name(&self.label)),
            )
        });
        root.id(self.id.clone())
            .debug_selector(move || selector)
            .role(Role::Splitter)
            .aria_label(unread_divider_name(&self.label))
            .child(div().flex_1().h(px(1.)).bg(t.color.error))
            .child(
                div()
                    .flex_none()
                    .font_stack(&t.font.family.body)
                    .text_size(t.text.supporting.size)
                    .line_height(leading)
                    .font_weight(t.font.weight.bold)
                    .text_color(t.color.text_error)
                    .child(self.label.clone()),
            )
    }
}
