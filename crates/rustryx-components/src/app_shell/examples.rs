//! AppShell examples: the gallery's Shell page.

use gpui::{AnyElement, App, IntoElement, ParentElement, Styled, Window, div, prelude::*, px};

use super::{AppShell, AppShellMobileNav, AppShellVariant, MobileNavConfig};
use crate::layout::{LayoutHeight, SpacingStep, Stack};
use crate::nav::NavIcon;
use crate::side_nav::{SideNav, SideNavItem, SideNavSection};
use crate::text::{Text, TextType};
use crate::top_nav::{TopNav, TopNavHeading, TopNavItem};

fn top_nav(id: &'static str) -> impl Fn() -> TopNav + 'static {
    move || {
        TopNav::new(format!("{id}-top"))
            .heading(
                TopNavHeading::new(format!("{id}-heading"))
                    .logo(|| NavIcon::new("viewColumns"))
                    .heading("Console"),
            )
            .start_content(TopNavItem::new(format!("{id}-overview"), "Overview").selected(true))
            .start_content(TopNavItem::new(format!("{id}-usage"), "Usage"))
    }
}

fn side_nav(id: &'static str) -> impl Fn() -> SideNav + 'static {
    move || {
        SideNav::new(format!("{id}-side")).collapsible(true).child(
            SideNavSection::new(format!("{id}-main"), "Projects")
                .child(
                    SideNavItem::new(format!("{id}-a"), "Atlas")
                        .icon("info")
                        .selected(true),
                )
                .child(SideNavItem::new(format!("{id}-b"), "Beacon").icon("search"))
                .child(SideNavItem::new(format!("{id}-c"), "Comet").icon("calendar")),
        )
    }
}

fn content() -> impl IntoElement {
    Stack::vertical()
        .gap(SpacingStep::S3)
        .child(Text::new("Atlas").text_type(TextType::Label))
        .children((1..=12).map(|i| {
            Text::new(format!(
                "Row {i}: the main region scrolls on its own in fill."
            ))
            .text_type(TextType::Body)
        }))
}

pub fn examples(window: &mut Window, cx: &mut App) -> AnyElement {
    let theme = rustryx_theme::current(window, cx);
    let t = theme.tokens().clone();
    let frame = |height: f32, child: AnyElement| {
        div()
            .w_full()
            .h(px(height))
            .border_1()
            .border_color(t.color.border)
            .rounded(t.radius.container)
            .overflow_hidden()
            .child(child)
    };
    Stack::vertical()
        .gap(SpacingStep::S4)
        .child(
            Text::new(
                "AppShell: elevated, with a top nav and a side nav (fill). Collapse the side nav: the content glides beside its edge",
            )
                .text_type(TextType::Supporting),
        )
        .child(frame(
            420.,
            AppShell::new("shell-example")
                .top_nav(top_nav("ex"))
                .side_nav(side_nav("ex"))
                .content_padding(SpacingStep::S4)
                .mobile_nav(AppShellMobileNav::Auto(MobileNavConfig::default()))
                .child(content())
                .into_any_element(),
        ))
        .child(
            Text::new("AppShell: section, auto height in a scrolling page (the header sticks)")
                .text_type(TextType::Supporting),
        )
        .child(frame(
            320.,
            div()
                .id("shell-auto-page")
                .size_full()
                .overflow_y_scroll()
                .child(
                    AppShell::new("shell-auto")
                        .variant(AppShellVariant::Section)
                        .height(LayoutHeight::Auto)
                        .top_nav(top_nav("auto"))
                        .side_nav(side_nav("auto"))
                        .content_padding(SpacingStep::S4)
                        .child(content()),
                )
                .into_any_element(),
        ))
        .into_any_element()
}
