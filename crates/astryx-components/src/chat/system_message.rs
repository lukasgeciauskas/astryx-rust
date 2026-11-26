//! ChatSystemMessage, upstream `Chat/ChatSystemMessage.tsx`: a centred notice that no
//! sender wrote, or a divider labelled with a date or section.

use astryx_theme::{ThemeStyled as _, current};
use gpui::Refineable as _;
use gpui::{
    AnyElement, App, IntoElement, ParentElement, RenderOnce, SharedString, StyleRefinement, Styled,
    Window, div,
};

use crate::layout::Divider;

/// Upstream `ChatSystemMessageVariant`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ChatSystemMessageVariant {
    /// Centred text.
    #[default]
    Default,
    /// A horizontal rule labelled with the message (FR2).
    Divider,
}

impl ChatSystemMessageVariant {
    pub fn as_str(self) -> &'static str {
        match self {
            ChatSystemMessageVariant::Default => "default",
            ChatSystemMessageVariant::Divider => "divider",
        }
    }
}

/// Upstream `ChatSystemMessage`.
///
/// ```ignore
/// ChatSystemMessage::new("Today").variant(ChatSystemMessageVariant::Divider)
/// ```
#[derive(IntoElement)]
pub struct ChatSystemMessage {
    content: SharedString,
    variant: ChatSystemMessageVariant,
    icon: Option<AnyElement>,
    style: StyleRefinement,
}

impl ChatSystemMessage {
    /// Upstream `children`: the notice, or the divider's label.
    pub fn new(content: impl Into<SharedString>) -> Self {
        ChatSystemMessage {
            content: content.into(),
            variant: ChatSystemMessageVariant::Default,
            icon: None,
            style: StyleRefinement::default(),
        }
    }

    astryx_primitives::setters! {
        /// Upstream `variant`.
        variant: ChatSystemMessageVariant,
    }

    /// Upstream `icon`: before the notice; the divider does not draw it (FR2).
    pub fn icon(mut self, icon: impl IntoElement) -> Self {
        self.icon = Some(icon.into_any_element());
        self
    }
}

impl Styled for ChatSystemMessage {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for ChatSystemMessage {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let props = [("variant", self.variant.as_str())];
        if self.variant == ChatSystemMessageVariant::Divider {
            let mut el = div().w_full();
            theme.refine(el.style(), "chat-system-message", &props, None);
            el.style().refine(&self.style);
            return el
                .child(Divider::new().label(self.content))
                .into_any_element();
        }
        let mut el = div()
            .flex()
            .items_center()
            .justify_center()
            .gap(t.spacing.s2)
            .py(t.spacing.s1)
            .font_stack(&t.font.family.body)
            .text_style(&t.text.supporting)
            .font_weight(t.font.weight.normal)
            .text_color(t.color.text_secondary)
            .text_center();
        theme.refine(el.style(), "chat-system-message", &props, None);
        el.style().refine(&self.style);
        el.child(
            div()
                .flex()
                .items_center()
                .gap(t.spacing.s1_5)
                .min_w_0()
                .children(
                    self.icon
                        .map(|icon| div().flex().flex_none().items_center().child(icon)),
                )
                .child(div().min_w_0().child(self.content)),
        )
        .into_any_element()
    }
}
