//! TopNav examples: the gallery's Shell page.

use astryx_primitives::provide;
use astryx_theme::current;
use gpui::{AnyElement, App, IntoElement, ParentElement, Styled, Window, div};

use crate::app_shell::AppShellMobile;

use super::{
    TopNav, TopNavHeading, TopNavItem, TopNavMegaMenu, TopNavMegaMenuFeaturedCard,
    TopNavMegaMenuItem, TopNavMenu, TopNavMenuItemData, TopNavRenderMode,
};
use crate::button::{Button, ButtonVariant};
use crate::layout::{SpacingStep, Stack};
use crate::nav::{NavHeadingMenu, NavHeadingMenuItem, NavIcon};
use crate::text::{Text, TextType};

fn heading(id: &str) -> TopNavHeading {
    TopNavHeading::new(format!("{id}-heading"))
        .logo(|| NavIcon::new("viewColumns"))
        .heading("Astryx")
        .menu(|| {
            NavHeadingMenu::new("product-switcher")
                .child(NavHeadingMenuItem::new("p-design", "Design system"))
                .child(NavHeadingMenuItem::new("p-gallery", "Gallery"))
                .into_any_element()
        })
}

fn product_menu(id: &str) -> TopNavMenu {
    TopNavMenu::new(
        format!("{id}-resources"),
        "Resources",
        [
            TopNavMenuItemData::new("Documentation")
                .description("Guides and API references")
                .icon("info"),
            TopNavMenuItemData::new("Search")
                .description("Find any component")
                .icon("search"),
            TopNavMenuItemData::new("Tools")
                .description("Codegen and the gallery")
                .icon("wrench"),
        ],
    )
}

fn mega(id: &str) -> TopNavMegaMenu {
    TopNavMegaMenu::new(format!("{id}-products"), "Products")
        .item(
            TopNavMegaMenuItem::new(format!("{id}-m-cal"), "Calendar")
                .description("Dates and ranges")
                .icon("calendar"),
        )
        .item(
            TopNavMegaMenuItem::new(format!("{id}-m-time"), "Clock")
                .description("Times and durations")
                .icon("clock"),
        )
        .item(
            TopNavMegaMenuItem::new(format!("{id}-m-up"), "Uploads")
                .description("Files and media")
                .icon("upload"),
        )
        .item(
            TopNavMegaMenuItem::new(format!("{id}-m-search"), "Search")
                .description("Typeahead and filters")
                .icon("search"),
        )
        .featured(
            TopNavMegaMenuFeaturedCard::new(format!("{id}-featured"), "What's new")
                .description("Navigation landed: SideNav and TopNav.")
                .link_label("Read more")
                .link_href("https://example.com/changelog"),
        )
}

fn bar(id: &str) -> TopNav {
    TopNav::new(id.to_string())
        .heading(heading(id))
        .start_content(TopNavItem::new(format!("{id}-home"), "Home").selected(true))
        .start_content(TopNavItem::new(format!("{id}-docs"), "Docs"))
        .start_content(product_menu(id))
        .start_content(mega(id))
        .start_content(TopNavItem::new(format!("{id}-old"), "Archive").disabled(true))
        .end_content(
            div()
                .flex()
                .items_center()
                .gap(gpui::px(4.))
                .child(
                    TopNavItem::new(format!("{id}-search"), "Search")
                        .icon("search")
                        .icon_only(true),
                )
                .child(
                    Button::new(format!("{id}-sign-in"), "Sign in").variant(ButtonVariant::Primary),
                ),
        )
}

pub fn examples(window: &mut Window, cx: &mut App) -> AnyElement {
    let theme = current(window, cx);
    let t = theme.tokens().clone();
    let frame = |child: AnyElement| {
        div()
            .w_full()
            .border_1()
            .border_color(t.color.border)
            .rounded(t.radius.container)
            .bg(t.color.background_surface)
            .child(child)
    };
    Stack::vertical()
        .gap(SpacingStep::S4)
        .child(
            Text::new("TopNav: a heading with a menu, items, a menu and a mega menu")
                .text_type(TextType::Supporting),
        )
        .child(frame(bar("top-nav").into_any_element()))
        .child(Text::new("TopNav with center content").text_type(TextType::Supporting))
        .child(frame(
            TopNav::new("top-nav-center")
                .heading(
                    TopNavHeading::new("center-heading")
                        .heading("Console")
                        .heading_href("https://example.com"),
                )
                .center_content(
                    div()
                        .flex()
                        .child(TopNavItem::new("c-overview", "Overview").selected(true))
                        .child(TopNavItem::new("c-usage", "Usage"))
                        .child(TopNavItem::new("c-billing", "Billing")),
                )
                .end_content(
                    TopNavItem::new("c-help", "Help")
                        .icon("info")
                        .icon_only(true),
                )
                .into_any_element(),
        ))
        .child(
            Text::new("The phone's bar: its toggle opens the drawer")
                .text_type(TextType::Supporting),
        )
        .child(frame({
            let state = window.use_keyed_state("top-nav-example-drawer", cx, |_, _| false);
            let is_open = *state.read(cx);
            let shell = AppShellMobile::new(is_open, move |open, _, cx| {
                state.update(cx, |s, cx| {
                    *s = open;
                    cx.notify();
                })
            });
            provide(
                shell,
                div()
                    .child(provide(TopNavRenderMode::MobileBar, bar("top-nav-phone")))
                    .child(provide(TopNavRenderMode::Drawer, bar("top-nav-drawer"))),
            )
            .into_any_element()
        }))
        .into_any_element()
}
