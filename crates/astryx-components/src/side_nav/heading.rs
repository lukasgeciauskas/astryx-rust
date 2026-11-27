//! SideNavHeading, upstream `SideNav/SideNavHeading.tsx`: the product or account heading
//! at the top of a SideNav. An icon and a heading, with an optional superheading and
//! subheading, each of which may be a link. A `menu` opens in a popover: from the whole
//! heading when it has no links, else from its own trigger. Expanded, a choice leaves
//! the menu open (NavHeadingMenu FR4); in a collapsed rail the icon opens it and a
//! choice closes it.

use std::rc::Rc;

use astryx_i18n::current as i18n;
use astryx_primitives::{LayerAlign, Placement, provide, use_context, use_link_handler};
use astryx_theme::{ThemeStyled, current};
use gpui::{
    AnyElement, App, ElementId, FontWeight, IntoElement, ParentElement, RenderOnce, Role,
    SharedString, Styled, Window, div, prelude::*,
};

use super::SideNavCollapse;
use crate::button::{ButtonVariant, IconButton};
use crate::icon::{Icon, IconColor, IconSize};
use crate::nav::{NavHeadingClose, NavHeadingDismiss, NavHeadingMenuHost};
use crate::overlay::{Popover, PopoverController};

/// Upstream `SideNavHeading`.
#[derive(IntoElement)]
pub struct SideNavHeading {
    id: ElementId,
    heading: SharedString,
    icon: Option<AnyElement>,
    heading_href: Option<SharedString>,
    superheading: Option<SharedString>,
    superheading_href: Option<SharedString>,
    subheading: Option<SharedString>,
    subheading_href: Option<SharedString>,
    menu: Option<Rc<dyn Fn() -> AnyElement>>,
    header_end_content: Option<AnyElement>,
}

impl SideNavHeading {
    pub fn new(id: impl Into<ElementId>, heading: impl Into<SharedString>) -> Self {
        SideNavHeading {
            id: id.into(),
            heading: heading.into(),
            icon: None,
            heading_href: None,
            superheading: None,
            superheading_href: None,
            subheading: None,
            subheading_href: None,
            menu: None,
            header_end_content: None,
        }
    }

    /// Upstream `icon`: the product's icon (a NavIcon, say).
    pub fn icon(mut self, icon: impl IntoElement) -> Self {
        self.icon = Some(icon.into_any_element());
        self
    }

    /// Upstream `headingHref`.
    pub fn heading_href(mut self, href: impl Into<SharedString>) -> Self {
        self.heading_href = Some(href.into());
        self
    }

    /// Upstream `superheading`: text above the heading.
    pub fn superheading(mut self, text: impl Into<SharedString>) -> Self {
        self.superheading = Some(text.into());
        self
    }

    /// Upstream `superheadingHref`.
    pub fn superheading_href(mut self, href: impl Into<SharedString>) -> Self {
        self.superheading_href = Some(href.into());
        self
    }

    /// Upstream `subheading`: text below the heading.
    pub fn subheading(mut self, text: impl Into<SharedString>) -> Self {
        self.subheading = Some(text.into());
        self
    }

    /// Upstream `subheadingHref`.
    pub fn subheading_href(mut self, href: impl Into<SharedString>) -> Self {
        self.subheading_href = Some(href.into());
        self
    }

    /// Upstream `menu`: built while the popover is open (a NavHeadingMenu).
    pub fn menu(mut self, menu: impl Fn() -> AnyElement + 'static) -> Self {
        self.menu = Some(Rc::new(menu));
        self
    }

    /// Upstream `headerEndContent`: at the row's end; hidden when collapsed.
    pub fn header_end_content(mut self, content: impl IntoElement) -> Self {
        self.header_end_content = Some(content.into_any_element());
        self
    }
}

impl RenderOnce for SideNavHeading {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens().clone();
        let i18n = i18n(cx);
        let links = use_link_handler(cx);
        let rail = use_context::<SideNavCollapse>(cx);
        let collapsed = rail.as_ref().is_some_and(|c| c.collapsed);
        // The rail's labels fade as it collapses or expands (SideNav FR6).
        let labels = rail.as_ref().map_or(1., |c| c.labels);
        let text = |text: SharedString, href: Option<SharedString>, heading: bool, id: &str| {
            let mut el = div()
                .id(ElementId::Name(format!("{:?}-{id}", self.id).into()))
                .opacity(labels)
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .text_style(if heading {
                    &t.text.label
                } else {
                    &t.text.supporting
                })
                .text_color(if heading {
                    t.color.text_primary
                } else {
                    t.color.text_secondary
                })
                .when(heading, |e| e.font_weight(FontWeight::SEMIBOLD))
                .child(text);
            if let Some(href) = href {
                let links = links.clone();
                el = el
                    .role(Role::Link)
                    .cursor_pointer()
                    .on_click(move |_, window, cx| links.open(&href, None, window, cx));
            }
            el
        };
        let has_links = self.heading_href.is_some()
            || self.superheading_href.is_some()
            || self.subheading_href.is_some();
        let texts = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .children(
                self.superheading
                    .clone()
                    .map(|s| text(s, self.superheading_href.clone(), false, "super")),
            )
            .child(text(
                self.heading.clone(),
                self.heading_href.clone(),
                true,
                "heading",
            ))
            .children(
                self.subheading
                    .clone()
                    .map(|s| text(s, self.subheading_href.clone(), false, "sub")),
            );
        let icon = self.icon.map(|i| div().flex().flex_none().child(i));

        // The menu's popover: a named menu region holding the NavHeadingMenu's rows.
        let menu = self.menu.clone().map(|build| {
            let controller = PopoverController::new(
                ElementId::Name(format!("{:?}-menu", self.id).into()),
                window,
                cx,
            );
            (controller, build)
        });
        let popover = |trigger: AnyElement| -> AnyElement {
            let Some((controller, build)) = menu.clone() else {
                return trigger;
            };
            let label = i18n.t("@astryx.sideNav.heading.dialogLabel");
            let close = controller.clone();
            Popover::new(
                ElementId::Name(format!("{:?}-menu-popover", self.id).into()),
                label.clone(),
            )
            // Built on the popover runtime, not upstream's Popover component: no touch
            // growth.
            .touch_motion(Some(false))
            .controller(controller)
            .placement(if collapsed {
                Placement::End
            } else {
                Placement::Below
            })
            .alignment(LayerAlign::Start)
            .close_button(false)
            .light_dismiss(true)
            .padding(crate::layout::SpacingStep::S1)
            .content_fn(move |window, cx| {
                let _ = (window, cx);
                let close = close.clone();
                let action: super::Action = Rc::new(move |window, cx| close.hide(window, cx));
                let menu = div()
                    .id("side-nav-heading-menu")
                    .role(Role::Menu)
                    .aria_label(label.clone())
                    .child(build());
                let menu = provide(NavHeadingMenuHost, menu);
                // Collapsed, a choice closes; expanded, only a release outside dismisses.
                if collapsed {
                    provide(NavHeadingClose(action), menu).into_any_element()
                } else {
                    provide(NavHeadingDismiss(action), menu).into_any_element()
                }
            })
            .child(trigger)
            .into_any_element()
        };

        if collapsed {
            let trigger = div()
                .id(ElementId::Name(format!("{:?}-icon", self.id).into()))
                .flex()
                .justify_center()
                .aria_label(self.heading.clone())
                .children(icon)
                .when(menu.is_some(), |d| d.role(Role::Button).cursor_pointer())
                .into_any_element();
            return popover(trigger);
        }
        let mut row = div()
            .id(self.id.clone())
            .flex()
            .items_center()
            .gap(t.spacing.s2)
            .w_full()
            .children(icon)
            .child(texts);
        theme.refine(row.style(), "side-nav-heading", &[], None);
        let row = if menu.is_some() && !has_links {
            // No links: the whole heading opens the menu.
            popover(
                row.role(Role::Button)
                    .cursor_pointer()
                    .aria_label(i18n.format(
                        "@astryx.sideNav.heading.openMenu",
                        &[("heading", self.heading.as_ref().into())],
                    ))
                    .child(
                        Icon::new("chevronDown")
                            .size(IconSize::Sm)
                            .color(IconColor::Secondary),
                    )
                    .into_any_element(),
            )
        } else if menu.is_some() {
            let trigger = IconButton::new(
                ElementId::Name(format!("{:?}-menu-trigger", self.id).into()),
                i18n.format(
                    "@astryx.sideNav.heading.openMenu",
                    &[("heading", self.heading.as_ref().into())],
                ),
                Icon::new("chevronDown").size(IconSize::Sm),
            )
            .variant(ButtonVariant::Ghost);
            row.child(popover(trigger.into_any_element()))
                .into_any_element()
        } else {
            row.into_any_element()
        };
        div()
            .flex()
            .items_center()
            .gap(t.spacing.s2)
            .w_full()
            .child(div().flex_1().min_w_0().child(row))
            .children(
                self.header_end_content
                    .map(|c| div().flex().flex_none().child(c)),
            )
            .into_any_element()
    }
}
