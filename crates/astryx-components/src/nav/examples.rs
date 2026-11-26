//! Navigation part examples: the gallery's Shell page.

use astryx_theme::current;
use gpui::{AnyElement, App, IntoElement, ParentElement, Styled, Window, div, px};

use crate::layout::{SpacingStep, Stack};
use crate::nav::{NavHeadingMenu, NavHeadingMenuItem, NavHeadingMenuSize, NavIcon};
use crate::text::{Text, TextType};

pub fn examples(window: &mut Window, cx: &mut App) -> AnyElement {
    let theme = current(window, cx);
    let t = theme.tokens().clone();
    Stack::vertical()
        .gap(SpacingStep::S4)
        .child(Text::new("NavIcon").text_type(TextType::Supporting))
        .child(
            Stack::horizontal()
                .gap(SpacingStep::S2)
                .child(NavIcon::new("search"))
                .child(NavIcon::new("wrench"))
                .child(NavIcon::new("calendar")),
        )
        .child(
            Text::new("NavHeadingMenu: the arrows, Home, End and typing move; Enter chooses")
                .text_type(TextType::Supporting),
        )
        .child(
            div()
                .w(px(260.))
                .p(t.spacing.s1)
                .rounded(t.radius.container)
                .border_1()
                .border_color(t.color.border)
                .bg(t.color.background_popover)
                .child(
                    NavHeadingMenu::new("workspace-menu")
                        .label("Switch workspace")
                        .size(NavHeadingMenuSize::Lg)
                        .child(
                            NavHeadingMenuItem::new("ws-design", "Design")
                                .description("12 projects")
                                .icon("viewColumns"),
                        )
                        .child(
                            NavHeadingMenuItem::new("ws-engineering", "Engineering")
                                .description("31 projects")
                                .icon("viewColumns"),
                        )
                        .child(
                            NavHeadingMenuItem::new("ws-archive", "Archive")
                                .description("Read only")
                                .icon("eyeSlash")
                                .disabled(true),
                        ),
                ),
        )
        .into_any_element()
}
