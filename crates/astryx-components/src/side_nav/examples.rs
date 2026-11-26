//! SideNav examples: the gallery's Shell page.

use astryx_theme::current;
use gpui::{AnyElement, App, IntoElement, ParentElement, Styled, Window, div, px};

use crate::layout::{SpacingStep, Stack};
use crate::nav::{NavHeadingMenu, NavHeadingMenuItem, NavIcon};
use crate::side_nav::{
    SideNav, SideNavCollapseConfig, SideNavHeading, SideNavItem, SideNavSection,
};
use crate::status::Badge;
use crate::text::{Text, TextType};

fn nav(id: &'static str, collapsed: bool) -> SideNav {
    SideNav::new(id)
        .collapsible_config(SideNavCollapseConfig {
            default_is_collapsed: Some(collapsed),
            ..Default::default()
        })
        .resizable(!collapsed)
        .header(
            SideNavHeading::new(format!("{id}-heading"), "Astryx")
                .subheading("Design system")
                .icon(NavIcon::new("viewColumns"))
                .menu(|| {
                    NavHeadingMenu::new("product-menu")
                        .child(NavHeadingMenuItem::new("p-docs", "Docs"))
                        .child(NavHeadingMenuItem::new("p-gallery", "Gallery"))
                        .into_any_element()
                }),
        )
        .child(
            SideNavSection::new(format!("{id}-main"), "Main")
                .child(
                    SideNavItem::new(format!("{id}-home"), "Overview")
                        .icon("info")
                        .selected(true),
                )
                .child(
                    SideNavItem::new(format!("{id}-search"), "Search")
                        .icon("search")
                        .end_content(Badge::new("3")),
                )
                .child(
                    SideNavItem::new(format!("{id}-tools"), "Tools")
                        .icon("wrench")
                        .collapsible(true)
                        .child(SideNavItem::new(format!("{id}-t1"), "Tokens"))
                        .child(SideNavItem::new(format!("{id}-t2"), "Themes")),
                ),
        )
        .child(
            SideNavSection::new(format!("{id}-more"), "More")
                .child(SideNavItem::new(format!("{id}-cal"), "Calendar").icon("calendar"))
                .child(
                    SideNavItem::new(format!("{id}-upload"), "Uploads")
                        .icon("upload")
                        .disabled(true),
                ),
        )
}

pub fn examples(window: &mut Window, cx: &mut App) -> AnyElement {
    let theme = current(window, cx);
    let t = theme.tokens().clone();
    let frame = |child: SideNav| {
        div()
            .h(px(460.))
            .flex()
            .border_1()
            .border_color(t.color.border)
            .rounded(t.radius.container)
            .overflow_hidden()
            .bg(t.color.background_surface)
            .child(child)
    };
    Stack::vertical()
        .gap(SpacingStep::S4)
        .child(
            Text::new("SideNav: expanded and resizable, and collapsed to its rail")
                .text_type(TextType::Supporting),
        )
        .child(
            Stack::horizontal()
                .gap(SpacingStep::S6)
                .child(frame(nav("side-nav-open", false)))
                .child(frame(nav("side-nav-rail", true))),
        )
        .into_any_element()
}
