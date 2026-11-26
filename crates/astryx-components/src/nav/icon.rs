//! NavIcon, upstream `NavIcon/` (component spec `NavIcon.spec.md`): a circular accent
//! badge around an icon, for navigation headers. Display only: no role, name or stop.

use astryx_theme::current;
use gpui::{
    AnyElement, App, IntoElement, ParentElement, Refineable, RenderOnce, StyleRefinement, Styled,
    Window, div,
};

use crate::icon::{Icon, IconColor, IconSize};

/// Upstream `NavIcon`.
#[derive(IntoElement)]
pub struct NavIcon {
    icon: AnyElement,
    style: StyleRefinement,
}

impl NavIcon {
    /// Upstream `icon`: an icon name, drawn on the accent color.
    pub fn new(icon: &'static str) -> Self {
        NavIcon {
            icon: Icon::new(icon)
                .size(IconSize::Sm)
                .color(IconColor::Inherit)
                .into_any_element(),
            style: StyleRefinement::default(),
        }
    }

    /// Upstream `icon` as an element.
    pub fn element(icon: impl IntoElement) -> Self {
        NavIcon {
            icon: icon.into_any_element(),
            style: StyleRefinement::default(),
        }
    }
}

impl Styled for NavIcon {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for NavIcon {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let size = t.size.element_md;
        let mut el = div()
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .size(size)
            .rounded_full()
            .bg(t.color.accent)
            .text_color(t.color.on_accent);
        theme.refine(el.style(), "nav-icon", &[], None);
        el.style().refine(&self.style);
        el.child(self.icon)
    }
}
