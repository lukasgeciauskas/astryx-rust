//! TopNavItem, upstream `TopNav/TopNavItem.tsx`: a bar link with hover and selected states
//! (`aria-current="page"`), or an icon-only square named by its label (FR4). With an
//! on_click and no href it is a button (FR1). Disabled, it keeps its tab stop but does
//! nothing (FR3). In AppShell's drawer it is a full-width row whose activation closes
//! the drawer (FR5).

use std::rc::Rc;

use gpui::{
    AnyElement, App, AriaCurrent, ClickEvent, ElementId, FontWeight, IntoElement, ParentElement,
    RenderOnce, Role, SharedString, StyleRefinement, Styled, Window, div, prelude::*,
};
use rustryx_primitives::{
    ElementSize, control_focus, is_focus_visible, use_context, use_link_handler,
};
use rustryx_theme::{ThemeStyled, current};

use super::TopNavRenderMode;
use crate::icon::{Icon, IconColor, IconSize};
use crate::nav::{close_drawer_after, drawer_close, nav_item_row};

type Action = Rc<dyn Fn(&mut Window, &mut App)>;

/// Upstream `TopNavItem`.
#[derive(IntoElement)]
pub struct TopNavItem {
    id: ElementId,
    label: SharedString,
    href: Option<SharedString>,
    selected: bool,
    disabled: bool,
    icon_only: bool,
    icon: Option<&'static str>,
    size: ElementSize,
    children: Vec<AnyElement>,
    on_click: Option<Action>,
    style: StyleRefinement,
}

impl TopNavItem {
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        TopNavItem {
            id: id.into(),
            label: label.into(),
            href: None,
            selected: false,
            disabled: false,
            icon_only: false,
            icon: None,
            size: ElementSize::Md,
            children: Vec::new(),
            on_click: None,
            style: StyleRefinement::default(),
        }
    }

    /// Upstream `href`.
    pub fn href(mut self, href: impl Into<SharedString>) -> Self {
        self.href = Some(href.into());
        self
    }

    /// Upstream `isSelected`.
    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    /// Upstream `isDisabled`: no navigation, no interaction; the tab stop stays.
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Upstream `isIconOnly`: a square; the label names it.
    pub fn icon_only(mut self, icon_only: bool) -> Self {
        self.icon_only = icon_only;
        self
    }

    /// Upstream `icon`.
    pub fn icon(mut self, icon: &'static str) -> Self {
        self.icon = Some(icon);
        self
    }

    /// Upstream `size`: the drawer row's height. Default medium.
    pub fn size(mut self, size: ElementSize) -> Self {
        self.size = size;
        self
    }

    /// Upstream `onClick`.
    pub fn on_click(mut self, f: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Rc::new(f));
        self
    }
}

impl ParentElement for TopNavItem {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl Styled for TopNavItem {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for TopNavItem {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens().clone();
        let rem = window.rem_size();
        let mode = use_context::<TopNavRenderMode>(cx).unwrap_or_default();
        let in_drawer = mode == TopNavRenderMode::Drawer;
        let focus = control_focus(&self.id, window, cx);
        let links = use_link_handler(cx);
        let close = if in_drawer { drawer_close(cx) } else { None };
        let selector = format!("{}", self.id);
        let el = div()
            .id(self.id.clone())
            .debug_selector(move || selector)
            .track_focus(&focus)
            .relative()
            .role(if self.href.is_some() {
                Role::Link
            } else {
                Role::Button
            })
            .aria_label(self.label.clone())
            .aria_disabled(self.disabled);
        let mut el = if in_drawer {
            nav_item_row(el, &t, self.size, self.selected, self.disabled)
        } else {
            let el = el
                .flex()
                .items_center()
                .gap(t.spacing.s2)
                .py(t.spacing.s1_5)
                .px(if self.icon_only {
                    t.spacing.s2
                } else {
                    t.spacing.s3
                })
                .rounded(t.radius.element)
                .font_stack(&t.font.family.body)
                .text_style(&t.text.label)
                .font_weight(FontWeight::MEDIUM)
                .text_color(t.color.text_secondary);
            if self.selected {
                el.text_color(t.color.text_primary)
                    .font_weight(FontWeight::SEMIBOLD)
                    .bg(t.color.neutral)
            } else if self.disabled {
                el.text_color(t.color.text_disabled)
            } else {
                el
            }
        };
        if self.selected {
            el = el.aria_current(AriaCurrent::Page);
        }
        // FR2: the selected fill stays under hover and press, with no overlay.
        if !self.selected && !self.disabled {
            let (hover, pressed) = (t.color.overlay_hover, t.color.overlay_pressed);
            el = el.hover(move |s| s.bg(hover));
            el = crate::press::touch_press(
                el,
                ElementId::Name(format!("{:?}-press", self.id).into()),
                false,
                gpui::transparent_black(),
                pressed,
                window,
                cx,
            );
        }
        theme.refine(
            el.style(),
            "top-nav-item",
            &[
                ("mode", if in_drawer { "drawer" } else { "" }),
                ("selected", if self.selected { "selected" } else { "" }),
            ],
            None,
        );
        el.style().refine(&self.style);
        el = if self.disabled {
            // FR3: no input navigates or fires on_click.
            el.on_click(|_, _, cx| cx.stop_propagation())
        } else {
            let (href, on_click) = (self.href.clone(), self.on_click.clone());
            el.cursor_pointer()
                .on_click(move |event: &ClickEvent, window, cx| {
                    cx.stop_propagation();
                    if let Some(f) = &on_click {
                        f(window, cx);
                    }
                    if let Some(href) = &href {
                        links.open(href, None, window, cx);
                    }
                    close_drawer_after(&close, href.is_some(), &event.modifiers(), window, cx);
                })
        };
        let icon_color = if self.disabled {
            IconColor::Disabled
        } else {
            IconColor::Inherit
        };
        let el = el.children(
            self.icon
                .map(|i| Icon::new(i).size(IconSize::Sm).color(icon_color)),
        );
        // FR4: an icon-only item hides its text.
        let el = if self.icon_only {
            el
        } else if self.children.is_empty() {
            el.child(self.label.clone())
        } else {
            el.children(self.children)
        };
        el.children(
            is_focus_visible(&focus, window)
                .then(|| crate::focus::focus_ring_inset(&t, t.radius.element.to_pixels(rem), rem)),
        )
    }
}
