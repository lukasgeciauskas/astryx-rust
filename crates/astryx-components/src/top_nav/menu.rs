//! TopNavMenu, upstream `TopNav/TopNavMenu.tsx`: a bar item whose menu of rich rows (an icon
//! box, a title, a description) opens on hover after `delay` and closes `hide_delay` after
//! the pointer leaves; a click opens it and moves focus to the first row (FR2). The
//! popover sits below the trigger, aligned to its TopNav slot, holding one menu and a
//! hidden close button (FR3). The rows are NavHeadingMenu's: one tab stop, the arrow
//! keys and typeahead, and the slide-to-choose press (FR5, FR11). A keyboard, finger or
//! pen pick closes the menu and returns focus to the trigger; a mouse pick leaves it
//! open (FR4). In AppShell's drawer it is a disclosure group (FR7, FR8); in the phone's
//! bar, nothing (FR9).

use crate::press::TouchPress as _;
use std::rc::Rc;
use std::time::Duration;

use astryx_primitives::{
    InteractionModality, LayerAlign, Placement, control_focus, interaction_modality,
    is_focus_visible, provide, use_context, use_link_handler,
};
use astryx_theme::{ThemeStyled, current};
use gpui::{
    AnyElement, App, ClickEvent, ElementId, FontWeight, IntoElement, ParentElement, RenderOnce,
    Role, SharedString, Styled, Window, div, prelude::*, px,
};

use super::hover::{MenuHover, MenuHoverOptions};
use super::top_nav::{TopNavFrame, TopNavRenderMode, TopNavSlot};
use crate::icon::{Icon, IconColor, IconSize};
use crate::nav::{
    NavHeadingClose, NavHeadingDismiss, NavHeadingMenu, NavHeadingMenuItem, NavHeadingMenuSize,
    close_drawer_after, drawer_close, nav_item_row,
};
use crate::overlay::{Popover, PopoverController};

type Action = Rc<dyn Fn(&mut Window, &mut App)>;

/// Upstream `TopNavMenuItemData`.
#[derive(Clone)]
pub struct TopNavMenuItemData {
    pub title: SharedString,
    pub description: Option<SharedString>,
    pub icon: Option<&'static str>,
    pub href: Option<SharedString>,
    pub on_click: Option<Action>,
    /// Upstream `hasCloseOnSelect`: a keyboard, finger or pen pick closes the menu and
    /// returns focus to the trigger. Default true.
    pub close_on_select: bool,
}

impl TopNavMenuItemData {
    pub fn new(title: impl Into<SharedString>) -> Self {
        TopNavMenuItemData {
            title: title.into(),
            description: None,
            icon: None,
            href: None,
            on_click: None,
            close_on_select: true,
        }
    }

    pub fn description(mut self, description: impl Into<SharedString>) -> Self {
        self.description = Some(description.into());
        self
    }

    pub fn icon(mut self, icon: &'static str) -> Self {
        self.icon = Some(icon);
        self
    }

    pub fn href(mut self, href: impl Into<SharedString>) -> Self {
        self.href = Some(href.into());
        self
    }

    pub fn on_click(mut self, f: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Rc::new(f));
        self
    }

    pub fn close_on_select(mut self, close: bool) -> Self {
        self.close_on_select = close;
        self
    }
}

/// A pick closes for a keyboard, finger or pen; a mouse or trackpad leaves it open.
pub(crate) fn pick_closes(window: &Window) -> bool {
    interaction_modality(window) != InteractionModality::Pointer
}

/// The chevron that turns half over when open, on `--duration-fast` and
/// `--ease-standard` (at once under Reduce Motion); `key` names its turn.
pub(crate) fn chevron(key: String, open: bool, window: &mut Window, cx: &mut App) -> gpui::Div {
    let t = current(window, cx).tokens().clone();
    let turn = crate::motion::transition(key, open, t.duration.fast, t.ease.standard, window, cx);
    div().flex().items_center().child(
        Icon::new("chevronDown")
            .size(IconSize::Sm)
            .color(IconColor::Inherit)
            .rotation(std::f32::consts::PI * turn),
    )
}

/// The popover alignment for a slot (FR3).
pub(crate) fn slot_alignment(slot: TopNavSlot) -> LayerAlign {
    match slot {
        TopNavSlot::Start => LayerAlign::Start,
        TopNavSlot::Center => LayerAlign::Center,
        TopNavSlot::End => LayerAlign::End,
    }
}

/// Upstream `TopNavMenu`.
#[derive(IntoElement)]
pub struct TopNavMenu {
    id: ElementId,
    label: SharedString,
    items: Vec<TopNavMenuItemData>,
    delay: Duration,
    hide_delay: Duration,
}

impl TopNavMenu {
    pub fn new(
        id: impl Into<ElementId>,
        label: impl Into<SharedString>,
        items: impl IntoIterator<Item = TopNavMenuItemData>,
    ) -> Self {
        TopNavMenu {
            id: id.into(),
            label: label.into(),
            items: items.into_iter().collect(),
            delay: Duration::from_millis(150),
            hide_delay: Duration::from_millis(200),
        }
    }

    /// Upstream `delay`: before a hover opens it. Default 150 ms.
    pub fn delay(mut self, delay: Duration) -> Self {
        self.delay = delay;
        self
    }

    /// Upstream `hideDelay`: before leaving closes it. Default 200 ms.
    pub fn hide_delay(mut self, delay: Duration) -> Self {
        self.hide_delay = delay;
        self
    }
}

impl RenderOnce for TopNavMenu {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        match use_context::<TopNavRenderMode>(cx).unwrap_or_default() {
            TopNavRenderMode::MobileBar => div().into_any_element(),
            TopNavRenderMode::Drawer => self.render_drawer(window, cx),
            TopNavRenderMode::Default => self.render_bar(window, cx),
        }
    }
}

impl TopNavMenu {
    fn render_bar(self, window: &mut Window, cx: &mut App) -> AnyElement {
        let theme = current(window, cx);
        let t = theme.tokens().clone();
        let rem = window.rem_size();
        let frame = use_context::<TopNavFrame>(cx);
        let popover_id = ElementId::Name(format!("{:?}-popover", self.id).into());
        let controller = PopoverController::new(popover_id.clone(), window, cx);
        let open = controller.is_open(cx);
        let trigger_focus = control_focus(&self.id, window, cx);
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
                enter: self.delay,
                exit: self.hide_delay,
                group: frame.as_ref().map(|f| f.group.clone()),
            },
            window,
            cx,
        );
        let hover_color = t.color.overlay_hover;
        let (on_hover, on_click) = (hover.clone(), hover.clone());
        let selector = format!("{}", self.id);
        let chevron_el = chevron(format!("{:?}-chevron", self.id), open, window, cx);
        let mut trigger =
            div()
                .id(self.id.clone())
                .debug_selector(move || selector)
                .track_focus(&trigger_focus)
                .relative()
                .flex()
                .items_center()
                .gap(t.spacing.s2)
                .py(t.spacing.s1_5)
                .px(t.spacing.s3)
                .rounded(t.radius.element)
                .font_stack(&t.font.family.body)
                .text_style(&t.text.label)
                .font_weight(FontWeight::MEDIUM)
                .text_color(if open {
                    t.color.text_primary
                } else {
                    t.color.text_secondary
                })
                .when(open, |el| el.bg(hover_color))
                .hover(move |s| s.bg(hover_color))
                .cursor_pointer()
                .role(Role::Button)
                .aria_label(self.label.clone())
                .aria_expanded(open)
                .aria_has_popup(gpui::HasPopup::Menu)
                .child(self.label.clone())
                .child(chevron_el)
                .on_hover(move |over, window, cx| {
                    if *over {
                        on_hover.trigger_enter(window, cx)
                    } else {
                        on_hover.leave(window, cx)
                    }
                })
                .on_click(move |event, window, cx| on_click.click(event.is_keyboard(), window, cx))
                .children(is_focus_visible(&trigger_focus, window).then(|| {
                    crate::focus::focus_ring_inset(&t, t.radius.element.to_pixels(rem), rem)
                }));
        theme.refine(trigger.style(), "top-nav-menu", &[], None);

        let items = self.items.clone();
        let (id, label) = (self.id.clone(), self.label.clone());
        let alignment = slot_alignment(frame.as_ref().map_or(TopNavSlot::Start, |f| f.slot));
        Popover::new(popover_id, self.label.clone())
            .controller(controller)
            .enabled(false)
            .placement(Placement::Below)
            .alignment(alignment)
            .light_dismiss(true)
            .return_focus(trigger_focus)
            .padding(crate::layout::SpacingStep::S0)
            .content_fn(move |window, cx| {
                let theme = current(window, cx);
                let t = theme.tokens().clone();
                let menu = NavHeadingMenu::new(ElementId::Name(format!("{id}-list").into()))
                    .label(label.clone())
                    .size(NavHeadingMenuSize::Lg)
                    .min_width(px(280.))
                    .gap(t.spacing.s1)
                    .children(items.iter().enumerate().map(|(i, item)| {
                        let mut row = NavHeadingMenuItem::new(
                            ElementId::Name(format!("{id}-item-{i}").into()),
                            item.title.clone(),
                        )
                        .boxed_icon()
                        .keeps_open(!item.close_on_select);
                        if let Some(d) = &item.description {
                            row = row.description(d.clone());
                        }
                        if let Some(icon) = item.icon {
                            row = row.icon(icon);
                        }
                        if let Some(href) = &item.href {
                            row = row.href(href.clone());
                        }
                        if let Some(f) = item.on_click.clone() {
                            row = row.on_click(move |window, cx| f(window, cx));
                        }
                        row
                    }));
                // FR4: a keyboard, finger or pen pick closes; a mouse release outside
                // the rows always does (FR11).
                let pick: Action = {
                    let hover = hover.clone();
                    Rc::new(move |window, cx| {
                        if pick_closes(window) {
                            hover.close(window, cx)
                        }
                    })
                };
                let (enter, leave) = (hover.clone(), hover.clone());
                div()
                    .id("top-nav-menu-content")
                    .relative()
                    .p(t.spacing.s1)
                    .on_hover(move |over, window, cx| {
                        if *over {
                            enter.content_enter(cx)
                        } else {
                            leave.leave(window, cx)
                        }
                    })
                    .child(hover.surface_probe())
                    .child(provide(
                        NavHeadingDismiss(hover.close_action()),
                        provide(NavHeadingClose(pick), menu),
                    ))
                    .into_any_element()
            })
            .child(trigger)
            .into_any_element()
    }

    /// FR7, FR8: a full-width disclosure; its rows close the drawer when chosen.
    fn render_drawer(self, window: &mut Window, cx: &mut App) -> AnyElement {
        let theme = current(window, cx);
        let t = theme.tokens().clone();
        let rem = window.rem_size();
        let expanded = window.use_keyed_state(
            ElementId::Name(format!("{:?}-drawer", self.id).into()),
            cx,
            |_, _| false,
        );
        let is_expanded = *expanded.read(cx);
        let chevron_el = chevron(
            format!("{:?}-drawer-chevron", self.id),
            is_expanded,
            window,
            cx,
        );
        // FR7: the group's height eases open and closed over --duration-medium on
        // --ease-standard, clipped until it is fully open; at once under Reduce Motion.
        let height = crate::animated_layout::adopt::use_height_transition(
            format!("{:?}-drawer-items", self.id),
            is_expanded,
            t.duration.medium,
            t.ease.standard,
            window,
            cx,
        );
        let focus = control_focus(&self.id, window, cx);
        let (hover, pressed) = (t.color.overlay_hover, t.color.overlay_pressed);
        let selector = format!("{}", self.id);
        let header = nav_item_row(
            div()
                .id(self.id.clone())
                .debug_selector(move || selector)
                .track_focus(&focus)
                .relative(),
            &t,
            astryx_primitives::ElementSize::Md,
            false,
            false,
        )
        .justify_between()
        .cursor_pointer()
        .hover(move |s| s.bg(hover))
        .touch_press(
            ElementId::Name(format!("{}-press", self.id).into()),
            gpui::transparent_black(),
            pressed,
            window,
            cx,
        )
        .role(Role::Button)
        .aria_label(self.label.clone())
        .aria_expanded(is_expanded)
        .child(self.label.clone())
        .child(chevron_el)
        .on_click(move |_, _, cx| {
            expanded.update(cx, |e, cx| {
                *e = !*e;
                cx.notify();
            })
        })
        .children(
            is_focus_visible(&focus, window)
                .then(|| crate::focus::focus_ring_inset(&t, t.radius.element.to_pixels(rem), rem)),
        );
        let links = use_link_handler(cx);
        let close = drawer_close(cx);
        // Collapsed rows are out of the tree (FR7); collapsing, they are inert.
        let rows = height.wrap(|| {
            div()
                .flex()
                .flex_col()
                .children(self.items.iter().enumerate().map(|(i, item)| {
                    let id = ElementId::Name(format!("{}-drawer-item-{i}", self.id).into());
                    let focus = control_focus(&id, window, cx);
                    let (href, on_click, close, links) = (
                        item.href.clone(),
                        item.on_click.clone(),
                        close.clone(),
                        links.clone(),
                    );
                    let selector = format!("{id}");
                    let press_id = ElementId::Name(format!("{id}-press").into());
                    nav_item_row(
                        div()
                            .id(id)
                            .debug_selector(move || selector)
                            .track_focus(&focus)
                            .relative(),
                        &t,
                        astryx_primitives::ElementSize::Md,
                        false,
                        false,
                    )
                    .pl(t.spacing.s6)
                    .cursor_pointer()
                    .hover(move |s| s.bg(hover))
                    .touch_press(press_id, gpui::transparent_black(), pressed, window, cx)
                    .role(if href.is_some() {
                        Role::Link
                    } else {
                        Role::Button
                    })
                    .aria_label(item.title.clone())
                    .children(item.icon.map(|icon| {
                        div()
                            .flex()
                            .flex_none()
                            .items_center()
                            .justify_center()
                            .size(px(20.))
                            .child(Icon::new(icon).size(IconSize::Sm).color(IconColor::Inherit))
                    }))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(t.spacing.s0_5)
                            .child(item.title.clone())
                            .children(item.description.clone().map(|d| {
                                div()
                                    .text_style(&t.text.supporting)
                                    .font_weight(FontWeight::NORMAL)
                                    .text_color(t.color.text_secondary)
                                    .child(d)
                            })),
                    )
                    .on_click(move |event: &ClickEvent, window, cx| {
                        if let Some(f) = &on_click {
                            f(window, cx);
                        }
                        if let Some(href) = &href {
                            links.open(href, None, window, cx);
                        }
                        close_drawer_after(&close, href.is_some(), &event.modifiers(), window, cx);
                    })
                    .children(is_focus_visible(&focus, window).then(|| {
                        crate::focus::focus_ring_inset(&t, t.radius.element.to_pixels(rem), rem)
                    }))
                }))
                // Collapsing, the group is inert (fork patch R20).
                .inert(!is_expanded)
                .into_any_element()
        });
        let mut section = div().flex().flex_col().child(header).child(rows);
        theme.refine(section.style(), "top-nav-menu", &[("mode", "drawer")], None);
        section.into_any_element()
    }
}
