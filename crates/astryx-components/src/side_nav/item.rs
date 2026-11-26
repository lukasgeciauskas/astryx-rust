//! SideNavItem, upstream `SideNav/SideNavItem.tsx`: a navigation row with an icon, a
//! selected state (`aria-current="page"`), end content, row actions, and nested items
//! that indent one `spacing-6` per level and may expand and collapse. In a collapsed
//! rail a leaf shows its icon with a tooltip, and an item with children opens a
//! flyout of them.

use std::rc::Rc;

use astryx_i18n::current as i18n;
use astryx_primitives::{
    ElementSize, LayerAlign, Placement, control_focus, is_focus_visible, provide, use_context,
    use_link_handler,
};
use astryx_theme::{ThemeStyled, current};
use gpui::{
    AnyElement, App, AriaCurrent, ElementId, FontWeight, IntoElement, ParentElement, RenderOnce,
    Role, SharedString, StyleRefinement, Styled, Window, div, prelude::*, px,
};

use super::{Action, ItemDepth, SideNavCollapse, SideNavRenderMode};
use crate::button::{ButtonVariant, IconButton};
use crate::icon::{Icon, IconColor, IconSize};
use crate::nav::{close_drawer_after, drawer_close};
use crate::overlay::{Popover, PopoverController, Tooltip};

type BoolFn = Rc<dyn Fn(bool, &mut Window, &mut App)>;

/// Upstream `collapsible`'s object form for an item with children.
#[derive(Clone, Default)]
pub struct SideNavItemCollapsible {
    pub default_is_collapsed: Option<bool>,
    /// Controlled.
    pub is_collapsed: Option<bool>,
    pub on_collapsed_change: Option<BoolFn>,
}

/// Upstream `SideNavItem`.
#[derive(IntoElement)]
pub struct SideNavItem {
    id: ElementId,
    label: SharedString,
    icon: Option<&'static str>,
    selected_icon: Option<&'static str>,
    selected: bool,
    disabled: bool,
    href: Option<SharedString>,
    on_click: Option<Action>,
    end_content: Option<AnyElement>,
    actions: Option<AnyElement>,
    children: Vec<AnyElement>,
    collapsible: Option<SideNavItemCollapsible>,
    size: Option<ElementSize>,
    style: StyleRefinement,
}

impl SideNavItem {
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        SideNavItem {
            id: id.into(),
            label: label.into(),
            icon: None,
            selected_icon: None,
            selected: false,
            disabled: false,
            href: None,
            on_click: None,
            end_content: None,
            actions: None,
            children: Vec::new(),
            collapsible: None,
            size: None,
            style: StyleRefinement::default(),
        }
    }

    /// Upstream `icon`: shown unselected; the rail keeps only items with one.
    pub fn icon(mut self, icon: &'static str) -> Self {
        self.icon = Some(icon);
        self
    }

    /// Upstream `selectedIcon`: shown while selected.
    pub fn selected_icon(mut self, icon: &'static str) -> Self {
        self.selected_icon = Some(icon);
        self
    }

    /// Upstream `isSelected`: the current page.
    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    /// Upstream `isDisabled`.
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Upstream `href`: followed through the link provider.
    pub fn href(mut self, href: impl Into<SharedString>) -> Self {
        self.href = Some(href.into());
        self
    }

    /// Upstream `onClick`.
    pub fn on_click(mut self, f: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Rc::new(f));
        self
    }

    /// Upstream `endContent`: passive content at the row's end (badges, counts).
    pub fn end_content(mut self, content: impl IntoElement) -> Self {
        self.end_content = Some(content.into_any_element());
        self
    }

    /// Upstream `actions`: row controls after the expand toggle.
    pub fn actions(mut self, actions: impl IntoElement) -> Self {
        self.actions = Some(actions.into_any_element());
        self
    }

    /// Upstream `collapsible={true}`: its children expand and collapse.
    pub fn collapsible(mut self, collapsible: bool) -> Self {
        self.collapsible = collapsible.then(SideNavItemCollapsible::default);
        self
    }

    /// Upstream `collapsible`'s object form.
    pub fn collapsible_config(mut self, config: SideNavItemCollapsible) -> Self {
        self.collapsible = Some(config);
        self
    }

    /// Upstream `size`.
    pub fn size(mut self, size: ElementSize) -> Self {
        self.size = Some(size);
        self
    }
}

impl ParentElement for SideNavItem {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl Styled for SideNavItem {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for SideNavItem {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens().clone();
        let rem = window.rem_size();
        let i18n = i18n(cx);
        let collapse = use_context::<SideNavCollapse>(cx).unwrap_or_else(SideNavCollapse::expanded);
        let depth = use_context::<ItemDepth>(cx).unwrap_or_default().0;
        let size = self.size.unwrap_or(ElementSize::Md);
        let has_children = !self.children.is_empty();
        let links = use_link_handler(cx);
        let icon_name = if self.selected {
            self.selected_icon.or(self.icon)
        } else {
            self.icon
        };
        let icon_color = if self.selected {
            IconColor::Inherit
        } else if self.disabled {
            IconColor::Disabled
        } else {
            IconColor::Secondary
        };
        let element = match size {
            ElementSize::Sm => t.size.element_sm,
            ElementSize::Md => t.size.element_md,
            ElementSize::Lg => t.size.element_lg,
        };
        let pad = match size {
            ElementSize::Sm => t.spacing.s1,
            _ => t.spacing.s2,
        };
        let focus = control_focus(&self.id, window, cx);
        let hover = t.color.overlay_hover;
        // The row's look (upstream navItemStyles).
        let selector = format!("{}", self.id);
        let style_row = |el: gpui::Stateful<gpui::Div>| {
            let selector = selector.clone();
            let mut el = el
                .debug_selector(move || selector)
                .flex()
                .items_center()
                .gap(t.spacing.s2)
                .min_h(element)
                .rounded(t.radius.element)
                .font_stack(&t.font.family.body)
                .text_style(&t.text.label)
                .text_color(if self.disabled {
                    t.color.text_disabled
                } else {
                    t.color.text_primary
                });
            if self.selected {
                el = el
                    .bg(t.color.neutral)
                    .font_weight(FontWeight::MEDIUM)
                    .aria_current(AriaCurrent::Page);
            } else if !self.disabled {
                el = el.hover(move |s| s.bg(hover));
            }
            if !self.disabled {
                el = el.cursor_pointer();
            }
            el
        };
        let activate: Action = {
            let (on_click, href) = (self.on_click.clone(), self.href.clone());
            Rc::new(move |window, cx| {
                if let Some(f) = &on_click {
                    f(window, cx);
                }
                if let Some(href) = &href {
                    links.open(href, None, window, cx);
                }
            })
        };
        let ring = |window: &Window| {
            is_focus_visible(&focus, window)
                .then(|| crate::focus::focus_ring_inset(&t, t.radius.element.to_pixels(rem), rem))
        };

        // The collapsed rail.
        if collapse.collapsed {
            let Some(icon) = icon_name else {
                return div().into_any_element();
            };
            let mut square = style_row(div().id(self.id.clone()))
                .track_focus(&focus)
                .relative()
                .justify_center()
                .size(element)
                .aria_label(self.label.clone())
                .aria_disabled(self.disabled)
                .child(Icon::new(icon).size(IconSize::Sm).color(icon_color));
            square = square.children(ring(window));
            if has_children {
                let flyout = PopoverController::new(
                    ElementId::Name(format!("{:?}-flyout", self.id).into()),
                    window,
                    cx,
                );
                let (show, toggle) = (flyout.clone(), flyout.clone());
                let square = square
                    .role(Role::Button)
                    .on_hover(move |over, window, cx| {
                        if *over {
                            show.show(window, cx);
                        }
                    })
                    .on_click(move |_, window, cx| {
                        if toggle.is_open(cx) {
                            toggle.hide(window, cx);
                        } else {
                            toggle.show(window, cx);
                        }
                    });
                let label = self.label.clone();
                let header = div()
                    .px(t.spacing.s2)
                    .py(t.spacing.s1)
                    .text_style(&t.text.supporting)
                    .text_color(t.color.text_secondary)
                    .child(label.clone());
                let content = div()
                    .flex()
                    .flex_col()
                    .min_w(px(180.))
                    .gap(t.spacing.s0_5)
                    .child(header)
                    .child(provide(
                        SideNavCollapse::expanded(),
                        provide(
                            ItemDepth(0),
                            div()
                                .flex()
                                .flex_col()
                                .gap(t.spacing.s0_5)
                                .children(self.children),
                        ),
                    ));
                return Popover::new(
                    ElementId::Name(format!("{:?}-flyout-popover", self.id).into()),
                    i18n.format(
                        "@astryx.sideNavItem.submenuLabel",
                        &[("label", label.as_ref().into())],
                    ),
                )
                .controller(flyout)
                .placement(Placement::End)
                .alignment(LayerAlign::Start)
                .close_button(false)
                .light_dismiss(true)
                .padding(crate::layout::SpacingStep::S1)
                .content(content)
                .child(square)
                .into_any_element();
            }
            let square = square
                .role(if self.href.is_some() {
                    Role::Link
                } else {
                    Role::Button
                })
                .when(!self.disabled, |el| {
                    let activate = activate.clone();
                    el.on_click(move |_, window, cx| activate(window, cx))
                });
            return Tooltip::new(
                ElementId::Name(format!("{:?}-tip", self.id).into()),
                self.label.clone(),
            )
            .placement(Placement::End)
            .child(square)
            .into_any_element();
        }

        // The expanded row.
        let collapsible = has_children && self.collapsible.is_some();
        let config = self.collapsible.clone().unwrap_or_default();
        let state = window.use_keyed_state(
            ElementId::Name(format!("{:?}-expanded", self.id).into()),
            cx,
            |_, _| config.default_is_collapsed.unwrap_or(false),
        );
        let item_collapsed = config.is_collapsed.unwrap_or(*state.read(cx));
        let toggle_item: Action = {
            let (state, report, controlled) = (
                state.clone(),
                config.on_collapsed_change.clone(),
                config.is_collapsed.is_some(),
            );
            Rc::new(move |window, cx| {
                let next = !item_collapsed;
                if !controlled {
                    state.update(cx, |s, _| *s = next);
                }
                if let Some(f) = &report {
                    f(next, window, cx);
                }
                window.refresh();
            })
        };
        let has_primary = self.href.is_some() || self.on_click.is_some();
        let independent = collapsible && has_primary;
        let chevron = |expanded: bool| {
            let icon = Icon::new("chevronDown")
                .size(IconSize::Sm)
                .color(IconColor::Inherit);
            let el = div()
                .flex()
                .flex_none()
                .items_center()
                .justify_center()
                .w(t.spacing.s6)
                .child(icon);
            if expanded { el.scale_xy(1., -1.) } else { el }
        };
        // FR3-FR4: the indent is inside the row and part of its target.
        let indent = t.spacing.s6.to_pixels(rem) * depth as f32;
        let mut row = style_row(div().id(self.id.clone()))
            .track_focus(&focus)
            .relative()
            .w_full()
            .pr(pad)
            .pl(pad.to_pixels(rem) + indent)
            .role(if self.href.is_some() {
                Role::Link
            } else {
                Role::Button
            })
            .aria_label(self.label.clone())
            .aria_disabled(self.disabled)
            .children(icon_name.map(|i| Icon::new(i).size(IconSize::Sm).color(icon_color)))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .child(self.label.clone()),
            )
            .children(
                self.end_content
                    .map(|c| div().flex().flex_none().items_center().child(c)),
            );
        if collapsible && !independent {
            row = row
                .aria_expanded(!item_collapsed)
                .child(chevron(!item_collapsed));
        }
        if !self.disabled {
            let (toggle, activate) = (toggle_item.clone(), activate.clone());
            // In AppShell's drawer a row's activation closes it, except a modified
            // click on a link.
            let in_drawer = matches!(
                use_context::<SideNavRenderMode>(cx),
                Some(SideNavRenderMode::Drawer | SideNavRenderMode::DrawerContent)
            );
            let close = if in_drawer { drawer_close(cx) } else { None };
            let is_link = self.href.is_some();
            row = row.on_click(move |event, window, cx| {
                if collapsible && !independent {
                    toggle(window, cx);
                } else {
                    activate(window, cx);
                    close_drawer_after(&close, is_link, &event.modifiers(), window, cx);
                }
            });
        }
        row = row.children(ring(window));
        theme.refine(
            row.style(),
            "side-nav-item",
            &[
                ("size", size.as_str()),
                ("selected", if self.selected { "selected" } else { "" }),
                ("disabled", if self.disabled { "disabled" } else { "" }),
            ],
            None,
        );
        row.style().refine(&self.style);
        // The independent toggle and the actions sit at the row's end.
        let trailing = (independent || self.actions.is_some()).then(|| {
            let toggle_button = independent.then(|| {
                let toggle = toggle_item.clone();
                let label = if item_collapsed {
                    i18n.format(
                        "@astryx.sideNavItem.expand",
                        &[("label", self.label.as_ref().into())],
                    )
                } else {
                    i18n.format(
                        "@astryx.sideNavItem.collapse",
                        &[("label", self.label.as_ref().into())],
                    )
                };
                IconButton::new(
                    ElementId::Name(format!("{:?}-toggle", self.id).into()),
                    label,
                    chevron(!item_collapsed),
                )
                .variant(ButtonVariant::Ghost)
                .size(ElementSize::Sm)
                .on_click(move |_, window, cx| toggle(window, cx))
            });
            div()
                .absolute()
                .right(pad)
                .top_0()
                .bottom_0()
                .flex()
                .items_center()
                .gap(t.spacing.s1)
                .children(toggle_button)
                .children(self.actions)
        });
        let row = div().relative().w_full().child(row).children(trailing);
        let mut out = div().flex().flex_col().w_full().child(row);
        if has_children && (!collapsible || !item_collapsed) {
            out = out.child(provide(
                ItemDepth(depth + 1),
                div()
                    .id(ElementId::Name(format!("{:?}-group", self.id).into()))
                    .flex()
                    .flex_col()
                    .gap(t.spacing.s0_5)
                    .mt(t.spacing.s0_5)
                    .role(Role::Group)
                    .aria_label(self.label.clone())
                    .children(self.children),
            ));
        }
        out.into_any_element()
    }
}
