//! MobileNav examples: the gallery's Shell page.

use gpui::{AnyElement, App, IntoElement, ParentElement, Styled, Window};

use super::MobileNav;
use crate::button::Button;
use crate::layout::{SpacingStep, Stack};
use crate::side_nav::{SideNavItem, SideNavSection};
use crate::text::{Text, TextType};

pub fn examples(window: &mut Window, cx: &mut App) -> AnyElement {
    let open = window.use_keyed_state("mobile-nav-example-open", cx, |_, _| false);
    let is_open = *open.read(cx);
    let (show, change) = (open.clone(), open.clone());
    Stack::vertical()
        .gap(SpacingStep::S4)
        .child(
            Text::new("MobileNav: the phone's navigation drawer").text_type(TextType::Supporting),
        )
        .child(gpui::div().flex().child(
            Button::new("open-mobile-nav", "Open navigation").on_click(move |_, _, cx| {
                show.update(cx, |v, cx| {
                    *v = true;
                    cx.notify();
                })
            }),
        ))
        .child(
            MobileNav::new("mobile-nav-example")
                .open(is_open)
                .header_text("Navigation")
                .on_open_change(move |v, _, cx| {
                    change.update(cx, |s, cx| {
                        *s = v;
                        cx.notify();
                    })
                })
                .child(
                    SideNavSection::new("mn-main", "Main")
                        .child(
                            SideNavItem::new("mn-home", "Overview")
                                .icon("info")
                                .selected(true),
                        )
                        .child(SideNavItem::new("mn-search", "Search").icon("search"))
                        .child(SideNavItem::new("mn-cal", "Calendar").icon("calendar")),
                ),
        )
        .into_any_element()
}
