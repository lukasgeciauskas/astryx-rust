//! TopNavMegaMenu, upstream `TopNav/TopNavMegaMenu.tsx`, with `TopNavMegaMenuItem` and
//! `TopNavMegaMenuFeaturedCard`: a bar item whose panel hangs below the nav, aligned to the
//! trigger's TopNav slot (FR2): its items in two columns, then featured content beside
//! them, wrapping on narrow widths; at most `min(960px, window − 16px)` wide and the
//! space below the nav tall, scrolling inside (FR6). Hover opens it after `delay` and
//! leaving closes it after `hide_delay`; a click pins it (FR3); Enter and Space open it,
//! Escape closes it and returns focus (FR4); a press outside or a sibling opening closes
//! it (FR2). The panel is a group named by the label, then a hidden close button; a
//! click-open focuses its first link or button, and a keyboard, finger or pen pick
//! closes it (FR5). In AppShell's drawer it is a disclosure group (FR9); in the phone's
//! bar, nothing (FR10).

use crate::press::TouchPress as _;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use astryx_i18n::{current as i18n, use_direction};
use astryx_primitives::{
    ElementSize, control_focus, is_focus_visible, prefers_reduced_motion, provide, use_context,
    use_link_handler,
};
use astryx_theme::{ThemeStyled, current};
use gpui::{
    AnyElement, App, Bounds, ClickEvent, DispatchPhase, ElementId, Entity, FocusHandle, FontWeight,
    IntoElement, KeyDownEvent, MouseButton, MouseDownEvent, ParentElement, Pixels, RenderOnce,
    Role, SharedString, Styled, Window, anchored, canvas, deferred, div, img, point, prelude::*,
    px,
};

use web_time::Instant;

use super::hover::{MenuHover, MenuHoverOptions};
use super::menu::{chevron, pick_closes};
use super::top_nav::{TopNavFrame, TopNavRenderMode, TopNavSlot};
use crate::button::{Button, ButtonVariant};
use crate::nav::{close_drawer_after, drawer_close, nav_item_row};

type Action = Rc<dyn Fn(&mut Window, &mut App)>;
type OpenFn = Rc<dyn Fn(bool, &mut Window, &mut App)>;

/// A mega menu's state between frames.
struct MegaState {
    open: bool,
    trigger: Option<Bounds<Pixels>>,
    /// Focus enters the panel on its next frame (a click or key open).
    focus_first: bool,
    /// The panel's focus region.
    panel: FocusHandle,
    /// The panel's width last frame, for alignment to an end or center slot.
    panel_width: Option<Pixels>,
    /// When the panel last opened, for its entrance.
    opened_at: Option<Instant>,
}

/// What a panel tells its items: how a pick closes it, and where they register.
#[derive(Clone)]
struct MegaScope {
    close: Action,
    items: Rc<RefCell<Vec<FocusHandle>>>,
}

/// Upstream `TopNavMegaMenu`.
#[derive(IntoElement)]
pub struct TopNavMegaMenu {
    id: ElementId,
    label: SharedString,
    items: Vec<AnyElement>,
    featured: Option<AnyElement>,
    delay: Duration,
    hide_delay: Duration,
    on_open_change: Option<OpenFn>,
}

impl TopNavMegaMenu {
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        TopNavMegaMenu {
            id: id.into(),
            label: label.into(),
            items: Vec::new(),
            featured: None,
            delay: Duration::from_millis(150),
            hide_delay: Duration::from_millis(250),
            on_open_change: None,
        }
    }

    /// Upstream `items`: one item (a TopNavMegaMenuItem, typically).
    pub fn item(mut self, item: impl IntoElement) -> Self {
        self.items.push(item.into_any_element());
        self
    }

    /// Upstream `featured`: beside the items (below them in the drawer).
    pub fn featured(mut self, featured: impl IntoElement) -> Self {
        self.featured = Some(featured.into_any_element());
        self
    }

    /// Upstream `delay`. Default 150 ms.
    pub fn delay(mut self, delay: Duration) -> Self {
        self.delay = delay;
        self
    }

    /// Upstream `hideDelay`. Default 250 ms.
    pub fn hide_delay(mut self, delay: Duration) -> Self {
        self.hide_delay = delay;
        self
    }

    /// Upstream `onOpenChange`: whenever the panel shows or hides, whatever the cause (FR7).
    pub fn on_open_change(mut self, f: impl Fn(bool, &mut Window, &mut App) + 'static) -> Self {
        self.on_open_change = Some(Rc::new(f));
        self
    }
}

/// FR6: the panel fades and slides in from 4 px above. Under reduced motion a finger
/// sees a 200 ms fade and a pointer none.
fn panel_entrance(
    el: gpui::Stateful<gpui::Div>,
    since: Instant,
    t: &astryx_theme::Tokens,
    window: &mut Window,
    cx: &App,
) -> gpui::Stateful<gpui::Div> {
    let reduced = prefers_reduced_motion(cx);
    let (duration, ease) = match (
        reduced,
        astryx_primitives::modality::coarse_primary_pointer_now(window, cx),
    ) {
        (false, _) => (t.duration.medium_min, t.ease.standard),
        (true, true) => (
            Duration::from_millis(200),
            astryx_theme::CubicBezier(0.42, 0., 0.58, 1.),
        ),
        (true, false) => return el,
    };
    let now = cx.background_executor().now();
    let progress = if duration.is_zero() {
        1.
    } else {
        (now.saturating_duration_since(since).as_secs_f32() / duration.as_secs_f32()).min(1.)
    };
    if progress < 1. {
        window.request_animation_frame();
    }
    let eased = ease.ease(progress);
    let el = el.opacity(eased);
    if reduced {
        el
    } else {
        el.translate(px(0.), px(-4.) * (1. - eased))
    }
}

/// Opens or closes, reporting a change (FR7).
fn set_open(
    state: &Entity<MegaState>,
    open: bool,
    report: &Option<OpenFn>,
    window: &mut Window,
    cx: &mut App,
) {
    let now = cx.background_executor().now();
    let changed = state.update(cx, |s, _| {
        let changed = std::mem::replace(&mut s.open, open) != open;
        if changed && open {
            s.opened_at = Some(now);
        }
        changed
    });
    if changed && let Some(f) = report {
        f(open, window, cx);
    }
    window.refresh();
}

impl RenderOnce for TopNavMegaMenu {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        match use_context::<TopNavRenderMode>(cx).unwrap_or_default() {
            TopNavRenderMode::MobileBar => div().into_any_element(),
            TopNavRenderMode::Drawer => self.render_drawer(window, cx),
            TopNavRenderMode::Default => self.render_bar(window, cx),
        }
    }
}

impl TopNavMegaMenu {
    fn render_bar(self, window: &mut Window, cx: &mut App) -> AnyElement {
        let theme = current(window, cx);
        let t = theme.tokens().clone();
        let rem = window.rem_size();
        let frame = use_context::<TopNavFrame>(cx);
        let state = window.use_keyed_state(
            ElementId::Name(format!("{:?}-mega", self.id).into()),
            cx,
            |_, cx| MegaState {
                open: false,
                trigger: None,
                focus_first: false,
                panel: cx.focus_handle().tab_stop(false),
                panel_width: None,
                opened_at: None,
            },
        );
        let report = self.on_open_change.clone();
        let open = state.read(cx).open;
        let trigger_focus = control_focus(&self.id, window, cx);
        let hover = MenuHover::new(
            &self.id,
            MenuHoverOptions {
                open,
                show: {
                    let (state, report) = (state.clone(), report.clone());
                    Rc::new(move |focus, window, cx| {
                        state.update(cx, |s, _| s.focus_first = focus);
                        set_open(&state, true, &report, window, cx);
                    })
                },
                hide: {
                    let (state, report, trigger) =
                        (state.clone(), report.clone(), trigger_focus.clone());
                    Rc::new(move |window, cx| {
                        let had_focus = state.read(cx).panel.contains_focused(window, cx);
                        set_open(&state, false, &report, window, cx);
                        if had_focus {
                            window.focus(&trigger, cx);
                        }
                    })
                },
                focus_menu: {
                    let state = state.clone();
                    Rc::new(move |window, cx| {
                        state.update(cx, |s, _| s.focus_first = true);
                        window.refresh();
                    })
                },
                enter: self.delay,
                exit: self.hide_delay,
                group: frame.as_ref().map(|f| f.group.clone()),
            },
            window,
            cx,
        );
        let hover_color = t.color.overlay_hover;
        let (on_hover, on_click, on_escape) = (hover.clone(), hover.clone(), hover.clone());
        let probe_state = state.clone();
        let selector = format!("{}", self.id);
        let chevron_el = chevron(format!("{:?}-chevron", self.id), open, window, cx);
        let mut trigger = div()
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
            .child(self.label.clone())
            .child(chevron_el)
            .on_hover(move |over, window, cx| {
                if *over {
                    on_hover.trigger_enter(window, cx)
                } else {
                    on_hover.leave(window, cx)
                }
            })
            // FR3, FR4: a click pins or toggles; Enter and Space open, never close.
            .on_click(move |event, window, cx| on_click.click(event.is_keyboard(), window, cx))
            .on_key_down(move |event: &KeyDownEvent, window, cx| {
                if event.keystroke.key == "escape" && open {
                    cx.stop_propagation();
                    on_escape.close(window, cx);
                }
            })
            .child(
                canvas(
                    move |bounds, _, cx| probe_state.update(cx, |s, _| s.trigger = Some(bounds)),
                    |_, _, _, _| {},
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            )
            .children(
                is_focus_visible(&trigger_focus, window).then(|| {
                    crate::focus::focus_ring_inset(&t, t.radius.element.to_pixels(rem), rem)
                }),
            );
        theme.refine(trigger.style(), "top-nav-mega-menu", &[], None);
        if !open {
            return trigger.into_any_element();
        }

        // The panel: below the nav, aligned to the trigger's slot (FR2, FR6).
        let viewport = window.viewport_size();
        let max_width = px(960.).min(viewport.width - t.spacing.s4.to_pixels(rem));
        let nav = frame.as_ref().and_then(|f| f.bounds.get());
        let trigger_bounds = state.read(cx).trigger.unwrap_or_default();
        let slot = frame.as_ref().map_or(TopNavSlot::Start, |f| f.slot);
        let top = nav.map_or(trigger_bounds.bottom(), |n| n.bottom());
        let edge = px(8.);
        let width = state.read(cx).panel_width.unwrap_or(max_width);
        let left = match slot {
            TopNavSlot::Start => trigger_bounds.left(),
            TopNavSlot::Center => trigger_bounds.center().x - width / 2.,
            TopNavSlot::End => trigger_bounds.right() - width,
        }
        .min(viewport.width - width - edge)
        .max(edge);
        let items = Rc::new(RefCell::new(Vec::new()));
        let pick: Action = {
            let hover = hover.clone();
            Rc::new(move |window, cx| {
                if pick_closes(window) {
                    hover.close(window, cx)
                }
            })
        };
        let scope = MegaScope {
            close: pick,
            items: items.clone(),
        };
        let panel_focus = state.read(cx).panel.clone();
        if state.read(cx).focus_first {
            state.update(cx, |s, _| s.focus_first = false);
            let (items, panel_focus) = (items.clone(), panel_focus.clone());
            window.defer(cx, move |window, cx| {
                match items.borrow().first().cloned() {
                    Some(first) => window.focus(&first, cx),
                    None => window.focus(&panel_focus, cx),
                }
            });
        }
        // The hidden close button after the group (FR5).
        let close_id = ElementId::Name(format!("{:?}-close", self.id).into());
        let close_focus = control_focus(&close_id, window, cx);
        let close_shown = close_focus.is_focused(window);
        let close_button = {
            let hover = hover.clone();
            let button = Button::new(close_id, i18n(cx).t("@astryx.popover.close"))
                .variant(ButtonVariant::Secondary)
                .on_click(move |_, window, cx| hover.close(window, cx));
            let wrap = div().flex().justify_center();
            if close_shown {
                wrap.pb(t.spacing.s2).child(button)
            } else {
                wrap.h(px(1.)).overflow_hidden().opacity(0.).child(button)
            }
        };
        let (enter, leave, escape) = (hover.clone(), hover.clone(), hover.clone());
        let (d_state, d_report) = (state.clone(), report.clone());
        let width_state = state.clone();
        let panel = div()
            .id(ElementId::Name(format!("{:?}-panel", self.id).into()))
            .debug_selector({
                let s = format!("{}-panel", self.id);
                move || s
            })
            .track_focus(&panel_focus)
            .occlude()
            .flex()
            .flex_col()
            .min_h_0()
            .max_w(max_width)
            .max_h((viewport.height - top - edge).max(px(120.)))
            .overflow_hidden()
            .rounded(t.radius.container)
            .bg(t.color.background_popover)
            .border_t_1()
            .border_color(t.color.border)
            .shadow_token(&t.shadow.low)
            .font_stack(&t.font.family.body)
            .text_color(t.color.text_primary)
            .on_hover(move |over, window, cx| {
                if *over {
                    enter.content_enter(cx)
                } else {
                    leave.leave(window, cx)
                }
            })
            .on_key_down(move |event: &KeyDownEvent, window, cx| {
                if event.keystroke.key == "escape" {
                    cx.stop_propagation();
                    escape.close(window, cx);
                }
            })
            .child(hover.surface_probe())
            .child(
                div()
                    .id(ElementId::Name(
                        format!("{:?}-panel-content", self.id).into(),
                    ))
                    .role(Role::Group)
                    .aria_label(self.label.clone())
                    .flex()
                    .flex_wrap()
                    .gap(t.spacing.s6)
                    .p(t.spacing.s3)
                    .overflow_y_scroll()
                    .overscroll_contain()
                    .child(provide(
                        scope.clone(),
                        div()
                            .flex_grow(2.)
                            .flex_shrink(1.)
                            .flex_basis(px(300.))
                            .min_w_0()
                            .grid()
                            .grid_cols(2)
                            .gap(t.spacing.s1)
                            .children(self.items),
                    ))
                    .children(self.featured.map(|f| {
                        provide(
                            scope,
                            div()
                                .flex()
                                .flex_col()
                                .flex_grow(1.)
                                .flex_shrink(1.)
                                .flex_basis(px(200.))
                                .rounded(t.radius.container)
                                .bg(t.color.background_muted)
                                .overflow_hidden()
                                .child(f),
                        )
                    })),
            )
            .child(close_button)
            // The panel's width, for alignment to an end or center slot.
            .child(
                canvas(
                    move |bounds, _, cx| {
                        width_state.update(cx, |s, _| s.panel_width = Some(bounds.size.width))
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
            // Light dismiss: a press outside the panel and the trigger closes it.
            .child(
                canvas(
                    |_, _, _| {},
                    move |bounds, _, window, _| {
                        let (state, report) = (d_state.clone(), d_report.clone());
                        window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
                            if phase != DispatchPhase::Capture || event.button != MouseButton::Left
                            {
                                return;
                            }
                            let on_trigger = state
                                .read(cx)
                                .trigger
                                .is_some_and(|b| b.contains(&event.position));
                            if !bounds.contains(&event.position) && !on_trigger {
                                set_open(&state, false, &report, window, cx);
                            }
                        });
                    },
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            );
        let since = state.read(cx).opened_at;
        let panel = match since {
            Some(since) => panel_entrance(panel, since, &t, window, cx),
            None => panel,
        };
        div()
            .child(trigger)
            .child(deferred(anchored().position(point(left, top)).child(panel)).with_priority(2))
            .into_any_element()
    }

    /// FR9: a full-width disclosure holding the items' drawer forms, then the featured
    /// content.
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
        // FR9: the group's height eases open and closed over --duration-medium on
        // --ease-standard, clipped until it is fully open; at once under Reduce Motion.
        let height = crate::animated_layout::adopt::use_height_transition(
            format!("{:?}-drawer-group", self.id),
            is_expanded,
            t.duration.medium,
            t.ease.standard,
            window,
            cx,
        );
        let focus = control_focus(&self.id, window, cx);
        let (hover, pressed) = (t.color.overlay_hover, t.color.overlay_pressed);
        let selector = format!("{}", self.id);
        let mut header = nav_item_row(
            div()
                .id(self.id.clone())
                .debug_selector(move || selector)
                .track_focus(&focus)
                .relative(),
            &t,
            ElementSize::Md,
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
        theme.refine(
            header.style(),
            "top-nav-mega-menu",
            &[("mode", "drawer")],
            None,
        );
        // Collapsed content is out of the tree (FR9); collapsing, it is inert.
        let (items, featured) = (self.items, self.featured);
        let group = height.wrap(move || {
            div()
                .flex()
                .flex_col()
                .children(items)
                .children(featured.map(|f| {
                    div()
                        .mt(t.spacing.s2)
                        .ml(t.spacing.s6)
                        .rounded(t.radius.container)
                        .bg(t.color.background_muted)
                        .overflow_hidden()
                        .child(f)
                }))
                // Collapsing, the group is inert (fork patch R20).
                .inert(!is_expanded)
                .into_any_element()
        });
        div()
            .flex()
            .flex_col()
            .child(header)
            .child(group)
            .into_any_element()
    }
}

/// Upstream `TopNavMegaMenuItem`.
#[derive(IntoElement)]
pub struct TopNavMegaMenuItem {
    id: ElementId,
    title: SharedString,
    description: Option<SharedString>,
    icon: Option<AnyElement>,
    href: Option<SharedString>,
    on_click: Option<Action>,
    close_on_select: bool,
}

impl TopNavMegaMenuItem {
    pub fn new(id: impl Into<ElementId>, title: impl Into<SharedString>) -> Self {
        TopNavMegaMenuItem {
            id: id.into(),
            title: title.into(),
            description: None,
            icon: None,
            href: None,
            on_click: None,
            close_on_select: true,
        }
    }

    /// Upstream `description`.
    pub fn description(mut self, description: impl Into<SharedString>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Upstream `icon`: an icon name, in the icon box.
    pub fn icon(mut self, icon: &'static str) -> Self {
        self.icon = Some(
            crate::icon::Icon::new(icon)
                .size(crate::icon::IconSize::Sm)
                .color(crate::icon::IconColor::Inherit)
                .into_any_element(),
        );
        self
    }

    /// Upstream `icon` as an element.
    pub fn icon_element(mut self, icon: impl IntoElement) -> Self {
        self.icon = Some(icon.into_any_element());
        self
    }

    /// Upstream `href`.
    pub fn href(mut self, href: impl Into<SharedString>) -> Self {
        self.href = Some(href.into());
        self
    }

    /// Upstream `onClick`.
    pub fn on_click(mut self, f: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Rc::new(f));
        self
    }

    /// Upstream `hasCloseOnSelect`: a keyboard, finger or pen pick closes the panel and
    /// returns focus to the trigger. Default true.
    pub fn close_on_select(mut self, close: bool) -> Self {
        self.close_on_select = close;
        self
    }
}

impl RenderOnce for TopNavMegaMenuItem {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens().clone();
        let rem = window.rem_size();
        let in_drawer = use_context::<TopNavRenderMode>(cx) == Some(TopNavRenderMode::Drawer);
        let scope = use_context::<MegaScope>(cx);
        let links = use_link_handler(cx);
        // FR1: a link with an href, a button with on_click, else a plain block.
        let actionable = in_drawer || self.href.is_some() || self.on_click.is_some();
        let focus = actionable.then(|| control_focus(&self.id, window, cx));
        if let (Some(scope), Some(focus)) = (&scope, &focus) {
            scope.items.borrow_mut().push(focus.clone());
        }
        let (hover, pressed) = (t.color.overlay_hover, t.color.overlay_pressed);
        let selector = format!("{}", self.id);
        let el = div()
            .id(self.id.clone())
            .debug_selector(move || selector)
            .relative()
            .when_some(focus.as_ref(), |el, f| el.track_focus(f));
        let close = if in_drawer { drawer_close(cx) } else { None };
        let (href, on_click) = (self.href.clone(), self.on_click.clone());
        let closes = self.close_on_select;
        let pick = scope.map(|s| s.close);
        let description = self.description.clone().map(|d| {
            div()
                .text_style(&t.text.supporting)
                .font_weight(FontWeight::NORMAL)
                .text_color(t.color.text_secondary)
                .child(d)
        });
        let mut el = if in_drawer {
            nav_item_row(el, &t, ElementSize::Md, false, false)
                .pl(t.spacing.s6)
                .items_start()
                .children(self.icon.map(|icon| {
                    div()
                        .flex()
                        .flex_none()
                        .items_center()
                        .justify_center()
                        .size(px(32.))
                        .mt(t.spacing.s0_5)
                        .rounded(t.radius.element)
                        .bg(t.color.neutral)
                        .text_color(t.color.icon_secondary)
                        .child(icon)
                }))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(t.spacing.s0_5)
                        .min_w_0()
                        .child(self.title.clone())
                        .children(description),
                )
        } else {
            el.flex()
                .items_start()
                .gap(t.spacing.s3)
                .w_full()
                .p(t.spacing.s3)
                .rounded(t.radius.element)
                .font_stack(&t.font.family.body)
                .children(self.icon.map(|icon| {
                    div()
                        .flex()
                        .flex_none()
                        .items_center()
                        .justify_center()
                        .size(px(40.))
                        .rounded(t.radius.element)
                        .bg(t.color.neutral)
                        .text_color(t.color.icon_secondary)
                        .child(icon)
                }))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(t.spacing.s1)
                        .min_w_0()
                        .child(
                            div()
                                .text_style(&t.text.label)
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(t.color.text_primary)
                                .child(self.title.clone()),
                        )
                        .children(description),
                )
        };
        theme.refine(
            el.style(),
            "top-nav-mega-menu-item",
            &[("mode", if in_drawer { "drawer" } else { "" })],
            None,
        );
        if actionable {
            el = el
                .cursor_pointer()
                .hover(move |s| s.bg(hover))
                .touch_press(
                    ElementId::Name(format!("{}-press", self.id).into()),
                    gpui::transparent_black(),
                    pressed,
                    window,
                    cx,
                )
                .role(if href.is_some() {
                    Role::Link
                } else {
                    Role::Button
                })
                .aria_label(self.title.clone())
                .on_click(move |event: &ClickEvent, window, cx| {
                    if let Some(f) = &on_click {
                        f(window, cx);
                    }
                    if let Some(href) = &href {
                        links.open(href, None, window, cx);
                    }
                    if in_drawer {
                        close_drawer_after(&close, href.is_some(), &event.modifiers(), window, cx);
                    } else if closes && let Some(pick) = &pick {
                        pick(window, cx);
                    }
                });
        }
        el.children(
            focus
                .filter(|f| is_focus_visible(f, window))
                .map(|_| crate::focus::focus_ring_inset(&t, t.radius.element.to_pixels(rem), rem)),
        )
    }
}

/// Upstream `TopNavMegaMenuFeaturedCard`.
#[derive(IntoElement)]
pub struct TopNavMegaMenuFeaturedCard {
    id: ElementId,
    title: SharedString,
    description: Option<SharedString>,
    image: Option<SharedString>,
    image_alt: Option<SharedString>,
    link_label: Option<SharedString>,
    link_href: Option<SharedString>,
    children: Vec<AnyElement>,
}

impl TopNavMegaMenuFeaturedCard {
    pub fn new(id: impl Into<ElementId>, title: impl Into<SharedString>) -> Self {
        TopNavMegaMenuFeaturedCard {
            id: id.into(),
            title: title.into(),
            description: None,
            image: None,
            image_alt: None,
            link_label: None,
            link_href: None,
            children: Vec::new(),
        }
    }

    /// Upstream `description`.
    pub fn description(mut self, description: impl Into<SharedString>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Upstream `image`: an image URL above the body, full width and 140 px tall.
    pub fn image(mut self, url: impl Into<SharedString>) -> Self {
        self.image = Some(url.into());
        self
    }

    /// Upstream `imageAlt`: omitted, the image is decorative (FR2).
    pub fn image_alt(mut self, alt: impl Into<SharedString>) -> Self {
        self.image_alt = Some(alt.into());
        self
    }

    /// Upstream `linkLabel`: the link shows only with `link_href` too (FR3).
    pub fn link_label(mut self, label: impl Into<SharedString>) -> Self {
        self.link_label = Some(label.into());
        self
    }

    /// Upstream `linkHref`.
    pub fn link_href(mut self, href: impl Into<SharedString>) -> Self {
        self.link_href = Some(href.into());
        self
    }
}

impl ParentElement for TopNavMegaMenuFeaturedCard {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for TopNavMegaMenuFeaturedCard {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens().clone();
        let rem = window.rem_size();
        let scope = use_context::<MegaScope>(cx);
        let links = use_link_handler(cx);
        let rtl = use_direction(cx).is_rtl();
        // FR2: without alt text the image is decorative (no role, no name).
        let image = self.image.map(|src| {
            let el = div()
                .id(ElementId::Name(format!("{}-image", self.id).into()))
                .w_full()
                .h(px(140.))
                .flex_none()
                .overflow_hidden()
                .child(
                    img(src.to_string())
                        .size_full()
                        .object_fit(gpui::ObjectFit::Cover),
                );
            match self.image_alt {
                Some(alt) => el.role(Role::Image).aria_label(alt),
                None => el,
            }
        });
        // FR3: "label →"; the arrow mirrors once in right-to-left layouts.
        let link = match (self.link_label, self.link_href) {
            (Some(label), Some(href)) => {
                let id = ElementId::Name(format!("{}-link", self.id).into());
                let focus = control_focus(&id, window, cx);
                if let Some(scope) = &scope {
                    scope.items.borrow_mut().push(focus.clone());
                }
                let pick = scope.map(|s| s.close);
                let selector = format!("{id}");
                let arrow = div().child("→");
                let arrow = if rtl { arrow.scale_xy(-1., 1.) } else { arrow };
                Some(
                    div()
                        .id(id)
                        .debug_selector(move || selector)
                        .track_focus(&focus)
                        .relative()
                        .flex()
                        .gap(px(4.))
                        .text_style(&t.text.supporting)
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(t.color.text_accent)
                        .cursor_pointer()
                        .role(Role::Link)
                        .aria_label(label.clone())
                        .child(label)
                        .child(arrow)
                        .on_click(move |_, window, cx| {
                            links.open(&href, None, window, cx);
                            if let Some(pick) = &pick {
                                pick(window, cx);
                            }
                        })
                        .children(is_focus_visible(&focus, window).then(|| {
                            crate::focus::focus_ring_inset(&t, t.radius.element.to_pixels(rem), rem)
                        })),
                )
            }
            _ => None,
        };
        let mut root = div().flex().flex_col();
        theme.refine(root.style(), "top-nav-mega-menu-featured-card", &[], None);
        root.children(image)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(t.spacing.s2)
                    .p(t.spacing.s4)
                    .font_stack(&t.font.family.body)
                    .child(
                        div()
                            .text_style(&t.text.label)
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(t.color.text_primary)
                            .child(self.title),
                    )
                    .children(self.description.map(|d| {
                        div()
                            .text_style(&t.text.supporting)
                            .text_color(t.color.text_secondary)
                            .child(d)
                    }))
                    .children(link),
            )
            .children(self.children)
    }
}
