//! TopNavHeading, upstream `TopNav/TopNavHeading.tsx`: the product heading at a TopNav's
//! start. A logo, then a text column (superheading, the heading row, subheading), then
//! end content (FR1). Which props are set picks its configuration (FR2): a logo only; one
//! link for the whole block; independent links per line; with a `menu` and no links the
//! whole block opens it, and with links the chevron and the rest of the block do. The
//! menu is a popover overlapping the block from below, headed by a button that closes it,
//! holding a named menu (FR7); it opens on hover after 150 ms, at once from another open
//! menu of the nav (FR5), and a choice or Escape closes it (FR8, FR9).

use std::any::TypeId;
use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use astryx_i18n::current as i18n;
use astryx_primitives::{
    LayerAlign, Placement, control_focus, is_focus_visible, provide, use_context, use_link_handler,
};
use astryx_theme::{ThemeStyled, current};
use gpui::{
    AnyElement, App, Bounds, Div, ElementId, FontWeight, IntoElement, ParentElement, Pixels,
    RenderOnce, Role, SharedString, Stateful, StyleRefinement, Styled, Window, canvas, div,
    prelude::*, px,
};

use super::hover::{MenuHover, MenuHoverOptions};
use super::top_nav::{TopNavFrame, TopNavRenderMode};
use crate::icon::{Icon, IconColor, IconSize};
use crate::link::Link;
use crate::nav::{NavHeadingClose, NavHeadingMenuHost, NavIcon};
use crate::overlay::{Popover, PopoverController};
use crate::text::{TextColor, TextSize};

/// The hover delay of a first open (FR5), as in TopNavMenu.
const HOVER_DELAY: Duration = Duration::from_millis(150);
const HIDE_DELAY: Duration = Duration::from_millis(200);

/// Upstream `TopNavHeading`.
#[derive(IntoElement)]
pub struct TopNavHeading {
    id: ElementId,
    heading: Option<SharedString>,
    logo: Option<Rc<dyn Fn() -> AnyElement>>,
    /// The logo is a NavIcon: the block drops its start padding.
    logo_is_nav_icon: bool,
    logo_label: Option<SharedString>,
    heading_href: Option<SharedString>,
    superheading: Option<SharedString>,
    superheading_href: Option<SharedString>,
    subheading: Option<SharedString>,
    subheading_href: Option<SharedString>,
    header_end_content: Option<AnyElement>,
    menu: Option<Rc<dyn Fn() -> AnyElement>>,
    style: StyleRefinement,
}

impl TopNavHeading {
    pub fn new(id: impl Into<ElementId>) -> Self {
        TopNavHeading {
            id: id.into(),
            heading: None,
            logo: None,
            logo_is_nav_icon: false,
            logo_label: None,
            heading_href: None,
            superheading: None,
            superheading_href: None,
            subheading: None,
            subheading_href: None,
            header_end_content: None,
            menu: None,
            style: StyleRefinement::default(),
        }
    }

    /// Upstream `heading`: the product or app name.
    pub fn heading(mut self, heading: impl Into<SharedString>) -> Self {
        self.heading = Some(heading.into());
        self
    }

    /// Upstream `logo`: an image, a NavIcon, or any element, built for the block and
    /// again for the menu's heading. It never shrinks.
    pub fn logo<E: IntoElement + 'static>(mut self, logo: impl Fn() -> E + 'static) -> Self {
        self.logo_is_nav_icon = TypeId::of::<E>() == TypeId::of::<NavIcon>();
        self.logo = Some(Rc::new(move || logo().into_any_element()));
        self
    }

    /// Upstream `logoLabel`: names a linked logo with no text beside it. Default the heading.
    pub fn logo_label(mut self, label: impl Into<SharedString>) -> Self {
        self.logo_label = Some(label.into());
        self
    }

    /// Upstream `headingHref`: alone and with no menu, the whole block is this link.
    pub fn heading_href(mut self, href: impl Into<SharedString>) -> Self {
        self.heading_href = Some(href.into());
        self
    }

    /// Upstream `superheading`: the suite name, above the heading.
    pub fn superheading(mut self, text: impl Into<SharedString>) -> Self {
        self.superheading = Some(text.into());
        self
    }

    /// Upstream `superheadingHref`.
    pub fn superheading_href(mut self, href: impl Into<SharedString>) -> Self {
        self.superheading_href = Some(href.into());
        self
    }

    /// Upstream `subheading`: the account context, below the heading.
    pub fn subheading(mut self, text: impl Into<SharedString>) -> Self {
        self.subheading = Some(text.into());
        self
    }

    /// Upstream `subheadingHref`.
    pub fn subheading_href(mut self, href: impl Into<SharedString>) -> Self {
        self.subheading_href = Some(href.into());
        self
    }

    /// Upstream `headerEndContent`: at the block's inline end.
    pub fn header_end_content(mut self, content: impl IntoElement) -> Self {
        self.header_end_content = Some(content.into_any_element());
        self
    }

    /// Upstream `menu`: built while the popover is open (a NavHeadingMenu, whose rows
    /// join this heading's menu). A chevron shows.
    pub fn menu(mut self, menu: impl Fn() -> AnyElement + 'static) -> Self {
        self.menu = Some(Rc::new(menu));
        self
    }
}

impl Styled for TopNavHeading {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

/// Which line of the text column.
#[derive(Clone, Copy, PartialEq)]
enum Line {
    Super,
    Heading,
    Sub,
}

impl RenderOnce for TopNavHeading {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens().clone();
        let rem = window.rem_size();
        let i18n = i18n(cx);
        let mode = use_context::<TopNavRenderMode>(cx).unwrap_or_default();
        let frame = use_context::<TopNavFrame>(cx);
        let links = use_link_handler(cx);
        let has_menu = self.menu.is_some();
        let has_href = self.heading_href.is_some()
            || self.superheading_href.is_some()
            || self.subheading_href.is_some();
        let whole_link = self.heading_href.is_some()
            && !has_menu
            && self.superheading_href.is_none()
            && self.subheading_href.is_none();
        let heading = self.heading.clone().filter(|h| !h.is_empty());
        let logo_link_label = self.logo_label.clone().or_else(|| heading.clone());
        let hover_color = t.color.overlay_hover;
        // FR12: a theme keys the drawer placement with mode:drawer.
        let theme_props: &[(&str, &str)] = if mode == TopNavRenderMode::Drawer {
            &[("mode", "drawer")]
        } else {
            &[]
        };
        let root = |id: ElementId| -> Stateful<Div> {
            let mut el = div()
                .id(id.clone())
                .debug_selector({
                    let s = format!("{id}");
                    move || s
                })
                .relative()
                .flex()
                .items_center()
                .gap(t.spacing.s2)
                .min_h(t.spacing.s8)
                .min_w_0()
                .pl(if self.logo_is_nav_icon {
                    px(0.)
                } else {
                    t.spacing.s2.to_pixels(rem)
                })
                .pr(t.spacing.s2)
                .rounded(t.radius.element)
                .font_stack(&t.font.family.body)
                .text_color(t.color.text_primary);
            theme.refine(el.style(), "top-nav-heading", theme_props, None);
            el.style().refine(&self.style);
            el
        };
        let logo_box = |logo: AnyElement| {
            div()
                .flex()
                .flex_none()
                .items_center()
                .justify_center()
                .child(logo)
        };

        // FR1, FR3: a line of the text column. Plain lines stay on one line with an
        // ellipsis; lines through the Link component wrap.
        let line = |text: SharedString, which: Line, href: Option<SharedString>, link: bool| {
            let id = ElementId::Name(
                format!(
                    "{}-{}",
                    self.id,
                    match which {
                        Line::Super => "super",
                        Line::Heading => "heading",
                        Line::Sub => "sub",
                    }
                )
                .into(),
            );
            match (href, link) {
                // The independent links' lines, and the supporting lines beside a menu.
                (Some(href), true) if which != Line::Heading || !has_menu => {
                    let mut l = Link::new(id, text).href(href);
                    l = if which == Line::Heading {
                        l.size(TextSize::Lg)
                            .weight(crate::text::TextWeight::Semibold)
                    } else {
                        l.color(TextColor::Secondary).size(TextSize::Xsm)
                    };
                    div()
                        .min_w_0()
                        .max_w_full()
                        .whitespace_normal()
                        .on_mouse_up(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .child(l)
                        .into_any_element()
                }
                // The heading link beside a menu: one line with an ellipsis.
                (Some(href), true) => {
                    let links = links.clone();
                    div()
                        .id(id)
                        .min_w_0()
                        .truncate()
                        .text_style(&t.text.large)
                        .font_weight(FontWeight::SEMIBOLD)
                        .cursor_pointer()
                        .role(Role::Link)
                        .on_click(move |_, window, cx| {
                            cx.stop_propagation();
                            links.open(&href, None, window, cx);
                        })
                        .child(text)
                        .into_any_element()
                }
                _ => {
                    let el = div().min_w_0().truncate();
                    let el = if which == Line::Heading {
                        el.text_style(&t.text.large)
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(t.color.text_primary)
                    } else {
                        el.text_style(&t.text.supporting)
                            .text_color(t.color.text_secondary)
                    };
                    el.child(text).into_any_element()
                }
            }
        };
        // The text column; `links` renders each line with an href as a link.
        let texts = |chevron: Option<AnyElement>, links: bool| {
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .children(
                    self.superheading
                        .clone()
                        .map(|s| line(s, Line::Super, self.superheading_href.clone(), links)),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(t.spacing.s1)
                        .min_w_0()
                        .children(
                            heading
                                .clone()
                                .map(|h| line(h, Line::Heading, self.heading_href.clone(), links)),
                        )
                        .children(chevron),
                )
                .children(
                    self.subheading
                        .clone()
                        .map(|s| line(s, Line::Sub, self.subheading_href.clone(), links)),
                )
        };
        let end = self
            .header_end_content
            .map(|c| div().flex().flex_none().items_center().ml_auto().child(c));

        // FR2: no heading and no menu: the logo alone, a link when it has an href.
        if heading.is_none() && !has_menu {
            let mut el = root(self.id.clone()).children(self.logo.as_ref().map(|l| logo_box(l())));
            if let Some(href) = self.heading_href.clone() {
                let links = links.clone();
                el = el
                    .role(Role::Link)
                    .cursor_pointer()
                    .when_some(self.logo_label.clone(), |el, l| el.aria_label(l))
                    .on_click(move |_, window, cx| links.open(&href, None, window, cx));
            }
            return el.into_any_element();
        }
        // FR2: the heading's href alone, with no menu: the whole block is one link.
        if whole_link {
            let href = self.heading_href.clone().unwrap_or_default();
            let links = links.clone();
            return root(self.id.clone())
                .role(Role::Link)
                .cursor_pointer()
                .when_some(heading.clone(), |el, h| el.aria_label(h))
                .on_click(move |_, window, cx| links.open(&href, None, window, cx))
                .children(self.logo.as_ref().map(|l| logo_box(l())))
                .child(texts(None, false))
                .children(end)
                .into_any_element();
        }
        let Some(build) = self.menu.clone() else {
            // Independent links, or a plain block.
            let logo =
                self.logo
                    .as_ref()
                    .map(|l| l())
                    .map(|logo| match self.heading_href.clone() {
                        Some(href) => {
                            let links = links.clone();
                            logo_box(logo)
                                .id(ElementId::Name(format!("{}-logo", self.id).into()))
                                .role(Role::Link)
                                .cursor_pointer()
                                .when_some(logo_link_label.clone(), |el, l| el.aria_label(l))
                                .on_click(move |_, window, cx| links.open(&href, None, window, cx))
                                .into_any_element()
                        }
                        None => logo_box(logo).into_any_element(),
                    });
            return root(self.id.clone())
                .children(logo)
                .child(texts(None, has_href))
                .children(end)
                .into_any_element();
        };

        // The menu configurations.
        let popover_id = ElementId::Name(format!("{:?}-menu", self.id).into());
        let controller = PopoverController::new(popover_id.clone(), window, cx);
        let open = controller.is_open(cx);
        let chevron_id = ElementId::Name(format!("{}-chevron", self.id).into());
        let chevron_focus = control_focus(&chevron_id, window, cx);
        let hover = MenuHover::new(
            &self.id,
            MenuHoverOptions {
                open,
                show: {
                    let c = controller.clone();
                    Rc::new(move |focus, window, cx| {
                        if focus {
                            c.show(window, cx)
                        } else {
                            c.show_without_focus(window, cx)
                        }
                    })
                },
                hide: {
                    let c = controller.clone();
                    Rc::new(move |window, cx| c.hide(window, cx))
                },
                focus_menu: {
                    let c = controller.clone();
                    Rc::new(move |window, cx| c.focus_surface(window, cx))
                },
                enter: HOVER_DELAY,
                exit: HIDE_DELAY,
                group: frame.as_ref().map(|f| f.group.clone()),
            },
            window,
            cx,
        );
        // FR4, FR11: the chevron button, named with the visible heading.
        let open_label = i18n.format(
            "@astryx.topNav.heading.openMenu",
            &[
                ("heading", heading.as_deref().unwrap_or("").into()),
                (
                    "hasHeading",
                    if heading.is_some() { "true" } else { "false" }.into(),
                ),
            ],
        );
        let chevron_button = {
            let hover = hover.clone();
            let selector = format!("{chevron_id}");
            div()
                .id(chevron_id)
                .debug_selector(move || selector)
                .track_focus(&chevron_focus)
                .relative()
                .flex()
                .flex_none()
                .items_center()
                .justify_center()
                .min_w(t.spacing.s7)
                .min_h(t.spacing.s7)
                .rounded(t.radius.element)
                .text_color(t.color.icon_secondary)
                .cursor_pointer()
                .hover(move |s| s.bg(hover_color))
                .role(Role::Button)
                .aria_label(open_label)
                .aria_expanded(open)
                .aria_has_popup(gpui::HasPopup::Menu)
                .on_click(move |event, window, cx| {
                    cx.stop_propagation();
                    hover.click(event.is_keyboard(), window, cx);
                })
                .child(
                    Icon::new("chevronDown")
                        .size(IconSize::Sm)
                        .color(IconColor::Inherit),
                )
                .children(is_focus_visible(&chevron_focus, window).then(|| {
                    crate::focus::focus_ring_inset(&t, t.radius.element.to_pixels(rem), rem)
                }))
                .into_any_element()
        };
        // The block's size, for the popover's overlap (FR7).
        let size = window
            .use_keyed_state(
                ElementId::Name(format!("{:?}-heading-size", self.id).into()),
                cx,
                |_, _| Rc::new(Cell::new(None::<Bounds<Pixels>>)),
            )
            .read(cx)
            .clone();
        let probe = {
            let size = size.clone();
            canvas(move |bounds, _, _| size.set(Some(bounds)), |_, _, _, _| {})
                .absolute()
                .top_0()
                .left_0()
                .size_full()
        };
        let logo_link = |logo: AnyElement| match self.heading_href.clone() {
            Some(href) => {
                let links = links.clone();
                logo_box(logo)
                    .id(ElementId::Name(format!("{}-logo", self.id).into()))
                    .role(Role::Link)
                    .cursor_pointer()
                    .when_some(logo_link_label.clone(), |el, l| el.aria_label(l))
                    .on_click(move |_, window, cx| {
                        cx.stop_propagation();
                        links.open(&href, None, window, cx);
                    })
                    .into_any_element()
            }
            None => logo_box(logo).into_any_element(),
        };
        let (enter, leave, click) = (hover.clone(), hover.clone(), hover.clone());
        // With no links the whole block is the trigger; with links, the rest of it.
        let block = root(self.id.clone())
            .cursor_pointer()
            .on_hover(move |over, window, cx| {
                if *over {
                    enter.trigger_enter(window, cx)
                } else {
                    leave.leave(window, cx)
                }
            })
            .on_click(move |event, window, cx| click.click(event.is_keyboard(), window, cx))
            .children(self.logo.as_ref().map(|l| l()).map(|logo| {
                if has_href {
                    logo_link(logo)
                } else {
                    logo_box(logo).into_any_element()
                }
            }))
            .child(texts(Some(chevron_button), has_href))
            .children(end)
            .child(probe);

        // FR7: the popover overlaps the block from below, headed by a button that
        // closes it, then the named menu holding `menu`.
        let block_size = size.get().map(|b| b.size).unwrap_or_default();
        let menu_label = heading
            .clone()
            .unwrap_or_else(|| i18n.t("@astryx.topNav.heading.dialogLabel"));
        let popover_heading = {
            let hover = hover.clone();
            let logo = self.logo.as_ref().map(|l| logo_box(l()));
            div()
                .id(ElementId::Name(
                    format!("{}-popover-heading", self.id).into(),
                ))
                .flex()
                .items_center()
                .gap(t.spacing.s2)
                .w_full()
                .min_h(t.spacing.s8)
                .pl(if self.logo_is_nav_icon {
                    px(0.)
                } else {
                    t.spacing.s2.to_pixels(rem)
                })
                .pr(t.spacing.s2)
                .mt(t.spacing.s1)
                .mb(t.spacing.s2)
                .mx(t.spacing.s1)
                .cursor_pointer()
                .role(Role::Button)
                .aria_label(heading.clone().unwrap_or_default())
                .on_click(move |_, window, cx| hover.close(window, cx))
                .children(logo)
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .flex_1()
                        .min_w_0()
                        .children(self.superheading.clone().map(|s| {
                            div()
                                .truncate()
                                .text_style(&t.text.supporting)
                                .text_color(t.color.text_secondary)
                                .child(s)
                        }))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(t.spacing.s1)
                                .children(heading.clone().map(|h| {
                                    div()
                                        .truncate()
                                        .text_style(&t.text.large)
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child(h)
                                }))
                                .child(
                                    div().flex().min_w(t.spacing.s7).justify_center().child(
                                        div().scale_xy(1., -1.).child(
                                            Icon::new("chevronDown")
                                                .size(IconSize::Sm)
                                                .color(IconColor::Secondary),
                                        ),
                                    ),
                                ),
                        )
                        .children(self.subheading.clone().map(|s| {
                            div()
                                .truncate()
                                .text_style(&t.text.supporting)
                                .text_color(t.color.text_secondary)
                                .child(s)
                        })),
                )
        };
        let popover_heading = Rc::new(std::cell::RefCell::new(Some(popover_heading)));
        let close = hover.close_action();
        Popover::new(popover_id, i18n.t("@astryx.topNav.heading.dialogLabel"))
            // Built on the popover runtime, not upstream's Popover component: no touch
            // growth.
            .touch_motion(Some(false))
            .controller(controller)
            .enabled(false)
            .placement(Placement::Below)
            .alignment(LayerAlign::Start)
            .close_button(false)
            .light_dismiss(true)
            .return_focus(chevron_focus)
            .padding(crate::layout::SpacingStep::S0)
            .min_w(block_size.width + px(16.))
            .mt(-(block_size.height + px(8.)))
            .ml(px(-8.))
            .content_fn(move |window, cx| {
                let theme = current(window, cx);
                let t = theme.tokens().clone();
                let (enter, leave) = (hover.clone(), hover.clone());
                let menu = div()
                    .id("top-nav-heading-menu")
                    .role(Role::Menu)
                    .aria_label(menu_label.clone())
                    .child(build());
                div()
                    .id("top-nav-heading-popover")
                    .relative()
                    .p(t.spacing.s1)
                    .overflow_hidden()
                    .on_hover(move |over, window, cx| {
                        if *over {
                            enter.content_enter(cx)
                        } else {
                            leave.leave(window, cx)
                        }
                    })
                    .child(hover.surface_probe())
                    .children(popover_heading.borrow_mut().take())
                    .child(provide(
                        NavHeadingClose(close.clone()),
                        provide(NavHeadingMenuHost, menu),
                    ))
                    .into_any_element()
            })
            .child(block)
            .into_any_element()
    }
}
