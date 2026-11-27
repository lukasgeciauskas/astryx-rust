//! TabList and Tab. Upstream: `TabList/`.
//!
//! A strip of tabs that scrolls when it is wider than its container, with
//! round arrows and edge fades (an alpha mask, fork patch R3) at the overflowing edges. A TabMenu is a tab that opens a
//! menu of more tabs: it shows the selected one's label, and selects through
//! the TabList's `onChange`.

#[cfg(test)]
mod tests;

use std::rc::Rc;

use gpui::{
    AnyElement, App, ElementId, IntoElement, ParentElement, RenderOnce, Role, ScrollHandle,
    SharedString, StyleRefinement, Styled, Window, div, point, prelude::*, px, relative,
};
use rustryx_i18n::{LogicalStyled, current as i18n, use_direction};
use rustryx_primitives::{
    ElementSize, ListFocus, ListKeys, is_focus_visible, provide, resolve_size, use_context,
    use_link_handler,
};
use rustryx_theme::{ThemeStyled, current};

use crate::button::element_height;
use crate::doc::{ComponentDoc, PropMapping, RustApi};
use crate::focus::focus_ring;
use crate::icon::IconSource;
use crate::icon::{Icon, IconDefaultSize, IconSize};
use crate::layout::ContainerPadding;
use crate::overlay::{DropdownMenu, MenuRadioGroup, MenuRadioItem, MenuTrigger};

/// Upstream TabList `layout`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TabLayout {
    /// Each tab as wide as its content.
    #[default]
    Hug,
    /// Tabs share the full width equally.
    Fill,
}

/// Upstream TabList `role`: which accessibility pattern the strip speaks.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TabPattern {
    /// A navigation landmark of links or buttons; the selected one is the
    /// current page. Arrows of both axes move.
    #[default]
    Navigation,
    /// The WAI-ARIA tabs pattern (`role="tablist"`): tabs swap a panel.
    /// Only the horizontal arrows move.
    TabList,
}

/// Upstream TabList `overflow`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TabOverflow {
    /// The component picks; today it scrolls.
    #[default]
    Auto,
    /// The tabs scroll horizontally.
    Scroll,
    /// No overflow handling: tabs spill out of the strip.
    Visible,
}

/// Upstream TabList `onChange`.
type SelectFn = Rc<dyn Fn(SharedString, &mut Window, &mut App)>;

/// One tab, upstream `Tab`.
pub struct Tab {
    value: SharedString,
    label: SharedString,
    label_hidden: bool,
    href: Option<SharedString>,
    icon: Option<AnyElement>,
    selected_icon: Option<AnyElement>,
    end_content: Option<AnyElement>,
    style: StyleRefinement,
}

impl Tab {
    /// Upstream `value` and `label`.
    pub fn new(value: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        Tab {
            value: value.into(),
            label: label.into(),
            label_hidden: false,
            href: None,
            icon: None,
            selected_icon: None,
            end_content: None,
            style: StyleRefinement::default(),
        }
    }

    rustryx_primitives::setters! {
        /// Upstream `isLabelHidden`: icon only; the label names it.
        !label_hidden,
        /// Upstream `href`: the tab is a link (ignored in the tabs pattern).
        ?href: SharedString,
    }

    /// Upstream `icon`: shown while not selected.
    pub fn icon(mut self, icon: impl IntoElement) -> Self {
        self.icon = Some(icon.into_any_element());
        self
    }

    /// Upstream `selectedIcon`: shown while selected; else `icon`.
    pub fn selected_icon(mut self, icon: impl IntoElement) -> Self {
        self.selected_icon = Some(icon.into_any_element());
        self
    }

    /// Upstream `endContent`: a badge or status dot after the label.
    pub fn end_content(mut self, content: impl IntoElement) -> Self {
        self.end_content = Some(content.into_any_element());
        self
    }
}

impl Styled for Tab {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

/// One option of a [`TabMenu`], upstream `TabMenuOption`.
#[derive(Clone)]
pub struct TabMenuOption {
    value: SharedString,
    label: SharedString,
    icon: Option<IconSource>,
}

impl TabMenuOption {
    /// Upstream `value` and `label`.
    pub fn new(value: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        TabMenuOption {
            value: value.into(),
            label: label.into(),
            icon: None,
        }
    }

    /// Upstream `icon`.
    pub fn icon(mut self, icon: impl Into<IconSource>) -> Self {
        self.icon = Some(icon.into());
        self
    }
}

/// A tab that opens a menu of more tabs, upstream `TabMenu`. Its trigger
/// shows the selected option's label (else `label`), carries the selection
/// indicator while one of its options is selected, and picks through the
/// TabList's `onChange`.
pub struct TabMenu {
    label: SharedString,
    options: Vec<TabMenuOption>,
    style: StyleRefinement,
}

impl TabMenu {
    /// Upstream `label` (the trigger's text when no option is selected, and
    /// the menu's heading) and `options`.
    pub fn new(
        label: impl Into<SharedString>,
        options: impl IntoIterator<Item = TabMenuOption>,
    ) -> Self {
        TabMenu {
            label: label.into(),
            options: options.into_iter().collect(),
            style: StyleRefinement::default(),
        }
    }
}

impl Styled for TabMenu {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

/// A tab or a TabMenu in the strip.
enum TabItem {
    Tab(Tab),
    Menu(TabMenu),
}

impl TabItem {
    fn holds(&self, value: &SharedString) -> bool {
        match self {
            TabItem::Tab(tab) => tab.value == *value,
            TabItem::Menu(menu) => menu.options.iter().any(|o| o.value == *value),
        }
    }
}

/// A strip of tabs, upstream `TabList`.
///
/// ```ignore
/// TabList::new("sections", "overview")
///     .tab(Tab::new("overview", "Overview"))
///     .tab(Tab::new("activity", "Activity"))
///     .on_change(|value, _, _| { /* store it */ })
/// ```
#[derive(IntoElement)]
pub struct TabList {
    id: ElementId,
    value: SharedString,
    on_change: Option<SelectFn>,
    size: Option<ElementSize>,
    layout: TabLayout,
    divider: bool,
    full_bleed: bool,
    pattern: TabPattern,
    overflow: TabOverflow,
    label: Option<SharedString>,
    tabs: Vec<TabItem>,
    style: StyleRefinement,
}

impl TabList {
    /// Upstream `value`: the selected tab.
    pub fn new(id: impl Into<ElementId>, value: impl Into<SharedString>) -> Self {
        TabList {
            id: id.into(),
            value: value.into(),
            on_change: None,
            size: None,
            layout: TabLayout::Hug,
            divider: false,
            full_bleed: false,
            pattern: TabPattern::Navigation,
            overflow: TabOverflow::Auto,
            label: None,
            tabs: Vec::new(),
            style: StyleRefinement::default(),
        }
    }

    rustryx_primitives::setters! {
        /// Upstream `size`: every tab's.
        ?size: ElementSize,
        /// Upstream `layout`.
        layout: TabLayout,
        /// Upstream `hasDivider`: a border under the strip.
        !divider,
        /// Upstream `isFullBleed`: the strip reaches its container's edges.
        !full_bleed,
        /// Upstream `role`.
        pattern: TabPattern,
        /// Upstream `overflow`.
        overflow: TabOverflow,
        /// Upstream `aria-label`: the strip's name; "Tabs" by default.
        ?label: SharedString,
    }

    /// Upstream `onChange`: the tab activated.
    pub fn on_change(mut self, f: impl Fn(SharedString, &mut Window, &mut App) + 'static) -> Self {
        self.on_change = Some(Rc::new(f));
        self
    }

    /// Upstream `children`: one tab.
    pub fn tab(mut self, tab: Tab) -> Self {
        self.tabs.push(TabItem::Tab(tab));
        self
    }

    /// Upstream `children`: tabs.
    pub fn tabs(mut self, tabs: impl IntoIterator<Item = Tab>) -> Self {
        self.tabs.extend(tabs.into_iter().map(TabItem::Tab));
        self
    }

    /// Upstream `children`: a TabMenu.
    pub fn menu(mut self, menu: TabMenu) -> Self {
        self.tabs.push(TabItem::Menu(menu));
        self
    }
}

impl Styled for TabList {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

/// Upstream `SCROLL_PAGE_RATIO`: an arrow scrolls this share of the strip.
const SCROLL_PAGE_RATIO: f32 = 0.8;

/// The scroll offset after paging `direction` (1 forward, -1 back), from
/// `offset` (0 at the start, negative as it scrolls) in a strip `visible`
/// wide that can scroll `max` far.
pub(crate) fn paged_offset(offset: f32, visible: f32, max: f32, direction: f32) -> f32 {
    (offset - direction * visible * SCROLL_PAGE_RATIO).clamp(-max, 0.)
}

impl RenderOnce for TabList {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let rem = window.rem_size();
        let dir = use_direction(cx);
        let size = resolve_size(self.size, ElementSize::Md, cx);
        let selected = self.tabs.iter().position(|item| item.holds(&self.value));
        let any_tabs = !self.tabs.is_empty();
        let is_tablist = self.pattern == TabPattern::TabList;
        let scrolls = self.overflow != TabOverflow::Visible;

        let keys = if is_tablist {
            ListKeys::horizontal()
        } else {
            ListKeys::both()
        };
        let scroll = window
            .use_keyed_state(
                ElementId::Name(format!("{:?}-scroll", self.id).into()),
                cx,
                |_, _| ScrollHandle::new(),
            )
            .read(cx)
            .clone();
        let follow = scroll.clone();
        let list = ListFocus::new(&self.id, self.tabs.len(), keys, window, cx)
            .rtl(dir.is_rtl())
            .on_move(move |index, _, _| follow.scroll_to_item(index));
        let tab_stop = list.tab_stop(selected, window);
        // Core's keyboard hint, on keyboard entry (upstream useKeyboardHint()).
        let hint = crate::keyboard_hint::use_keyboard_hint(
            &self.id,
            crate::keyboard_hint::KeyboardHintOptions::default(),
            list.focused(window).is_some(),
            window,
            cx,
        );

        let indicator_bottom = if self.divider {
            -(t.spacing.s1.to_pixels(rem) + t.border.width.to_pixels(rem))
        } else {
            px(-1.)
        };
        let fill = self.layout == TabLayout::Fill;
        let height = element_height(size, t);
        let (hover, pressed) = (t.color.overlay_hover, t.color.overlay_pressed);
        let radius = t.radius.element.to_pixels(rem);
        let t_gap = t.spacing.s1;

        let tabs = self.tabs.into_iter().enumerate().map(|(i, item)| {
            let is_selected = selected == Some(i);
            let handle = list.item_handle(i, tab_stop);
            let tab = match item {
                TabItem::Tab(tab) => tab,
                TabItem::Menu(menu) => {
                    let shown = menu
                        .options
                        .iter()
                        .find(|o| o.value == self.value)
                        .map(|o| o.label.clone())
                        .unwrap_or_else(|| menu.label.clone());
                    let semibold = t.font.weight.semibold;
                    let label_el = div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .child(div().child(shown.clone()))
                        .child(
                            div()
                                .h_0()
                                .invisible()
                                .font_weight(semibold)
                                .child(shown.clone()),
                        );
                    let indicator = div()
                        .absolute()
                        .bottom(indicator_bottom)
                        .inset_start(dir, t.spacing.s3)
                        .inset_end(dir, t.spacing.s3)
                        .h(px(2.))
                        .rounded_full()
                        .when(is_selected, |el| el.bg(t.color.accent));
                    let content = move |open: bool| {
                        let chevron = Icon::new("chevronDown").size(IconSize::Sm);
                        // Open, the chevron turns over.
                        let chevron = if open {
                            chevron.scale_xy(1., -1.)
                        } else {
                            chevron
                        };
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(t_gap)
                            .child(label_el)
                            .child(chevron)
                            .child(indicator)
                            .into_any_element()
                    };
                    let mut trigger = MenuTrigger::new(
                        ElementId::Name(format!("{:?}-menu-{i}", self.id).into()),
                        shown,
                        content,
                    )
                    .focus_handle(handle)
                    .arrow_keys(false)
                    .hover_tint()
                    .flex()
                    .flex_row()
                    .flex_none()
                    .items_center()
                    .justify_center()
                    .px(t.spacing.s3)
                    .min_h(height)
                    .rounded(t.radius.element)
                    .text_size(t.text.label.size)
                    .line_height(relative(t.text.label.leading))
                    .font_weight(t.font.weight.normal)
                    .text_color(t.color.text_secondary)
                    .whitespace_nowrap();
                    if fill {
                        trigger = trigger.flex_1();
                    }
                    if is_selected {
                        trigger = trigger
                            .text_color(t.color.text_primary)
                            .font_weight(t.font.weight.semibold);
                    }
                    theme.refine(trigger.style(), "tab-menu", &[], None);
                    trigger.style().refine(&menu.style);
                    let select = self.on_change.clone();
                    let mut group = MenuRadioGroup::new(self.value.clone())
                        .title(menu.label.clone())
                        .check_marks()
                        .on_change(move |value, window, cx| {
                            if let Some(f) = &select {
                                f(value, window, cx);
                            }
                        });
                    for option in &menu.options {
                        let mut row =
                            MenuRadioItem::new(option.value.clone(), option.label.clone());
                        if let Some(icon) = &option.icon {
                            row = row.icon(icon.clone());
                        }
                        group = group.item(row);
                    }
                    return DropdownMenu::new(ElementId::Name(
                        format!("{:?}-tab-menu-{i}", self.id).into(),
                    ))
                    .button_label(menu.label.clone())
                    .trigger(trigger)
                    .entry(group)
                    .into_any_element();
                }
            };
            let icon = if is_selected {
                tab.selected_icon.or(tab.icon)
            } else {
                tab.icon
            };
            let icon = icon.map(|icon| {
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .justify_center()
                    .child(provide(IconDefaultSize(IconSize::Sm), icon))
            });
            // The label, plus a hidden semibold copy that reserves the
            // selected width (upstream `labelSizer`).
            let label = (!tab.label_hidden).then(|| {
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .child(div().child(tab.label.clone()))
                    .child(
                        div()
                            .h_0()
                            .invisible()
                            .font_weight(t.font.weight.semibold)
                            .child(tab.label.clone()),
                    )
            });
            let mut el = div()
                .relative()
                .flex()
                .flex_row()
                .flex_none()
                .items_center()
                .justify_center()
                .gap(t.spacing.s1)
                .px(t.spacing.s3)
                .min_h(height)
                .rounded(t.radius.element)
                .text_size(t.text.label.size)
                .line_height(relative(t.text.label.leading))
                .font_weight(t.font.weight.normal)
                .text_color(t.color.text_secondary)
                .whitespace_nowrap();
            if fill {
                el = el.flex_1();
            }
            if is_selected {
                el = el
                    .text_color(t.color.text_primary)
                    .font_weight(t.font.weight.semibold);
            }
            theme.refine(
                el.style(),
                "tab",
                &[
                    ("size", size.as_str()),
                    ("selected", if is_selected { "selected" } else { "" }),
                ],
                None,
            );
            el.style().refine(&tab.style);
            let indicator = div()
                .absolute()
                .bottom(indicator_bottom)
                .inset_start(dir, t.spacing.s3)
                .inset_end(dir, t.spacing.s3)
                .h(px(2.))
                .rounded_full()
                .when(is_selected, |el| el.bg(t.color.accent));
            let select = self.on_change.clone();
            let value = tab.value.clone();
            let navigation = tab
                .href
                .clone()
                .filter(|_| !is_tablist)
                .map(|href| (href, use_link_handler(cx)));
            let mut el = el
                .id(ElementId::Name(
                    format!("{:?}-{}", self.id, tab.value).into(),
                ))
                .track_focus(&handle)
                .cursor_pointer()
                .hover(move |s| s.bg(hover))
                .on_click(move |_, window, cx| {
                    cx.stop_propagation();
                    if let Some(f) = &select {
                        f(value.clone(), window, cx);
                    }
                    if let Some((href, handler)) = &navigation {
                        handler.open(href, None, window, cx);
                    }
                })
                .children(icon)
                .children(label)
                .children(
                    tab.end_content
                        .map(|c| div().flex().flex_none().items_center().child(c)),
                )
                .child(indicator);
            // A finger's press paints as UIKit tracks a control (upstream `usePressFeedback`).
            el = crate::press::touch_press(
                el,
                ElementId::Name(format!("{:?}-{}-press", self.id, tab.value).into()),
                false,
                gpui::transparent_black(),
                pressed,
                window,
                cx,
            );
            if is_focus_visible(&handle, window) {
                el = el.child(focus_ring(t, radius, rem));
            }
            let role = match (is_tablist, tab.href.is_some()) {
                (true, _) => Role::Tab,
                (false, true) => Role::Link,
                (false, false) => Role::Button,
            };
            let el = el.role(role).aria_label(tab.label);
            if is_tablist {
                el.aria_selected(is_selected).into_any_element()
            } else {
                el.into_any_element()
            }
        });

        let mut strip = div()
            .id(ElementId::Name(format!("{:?}-strip", self.id).into()))
            .flex()
            .flex_row()
            .items_stretch()
            .gap(t.spacing.s0_5)
            .flex_1()
            .min_w_0()
            .on_key_down(list.key_down_listener())
            .children(tabs);
        if scrolls {
            // The scroll clip would cut the focus rings and the indicator
            // that hangs below the tabs: pad the strip by that much and pull
            // it back (upstream's bleed).
            let ring = t.focus.outline_width.to_pixels(rem) + t.focus.outline_offset.to_pixels(rem);
            let below = ring.max(-indicator_bottom);
            strip = strip
                .pt(ring)
                .mt(-ring)
                .pb(below)
                .mb(-below)
                .px(ring)
                .mx(-ring)
                .overflow_x_scroll()
                // Upstream hides the strip's scrollbar; the arrows and fades show it.
                .scrollbar_none()
                .track_scroll(&scroll);
        }

        // Arrows cap the edges the strip can scroll toward.
        let offset = scroll.offset().x;
        let max = scroll.max_offset().x;
        let visible = scroll.bounds().size.width;
        if scrolls && visible == px(0.) && any_tabs {
            // Measured after the first layout: draw again to place arrows.
            window.request_animation_frame();
        }
        let arrow = |direction: f32, icon: &'static str| {
            let scroll = scroll.clone();
            let mut el = div()
                .id(ElementId::Name(
                    format!("{:?}-arrow-{icon}", self.id).into(),
                ))
                .absolute()
                .top_0()
                .flex()
                .items_center()
                .justify_center()
                .size(height)
                .rounded_full()
                .bg(t.color.background_popover)
                .shadow_token(&t.shadow.low)
                .text_color(t.color.text_secondary)
                .cursor_pointer()
                .hover(|s| s.text_color(t.color.text_primary))
                .on_click(move |_, window, _| {
                    let current = scroll.offset();
                    let x = paged_offset(
                        current.x.as_f32(),
                        scroll.bounds().size.width.as_f32(),
                        scroll.max_offset().x.as_f32(),
                        direction,
                    );
                    scroll.set_offset(point(px(x), current.y));
                    window.refresh();
                })
                .child(Icon::new(icon).size(IconSize::Sm).mirror_in_rtl(dir));
            el = if direction < 0. {
                el.inset_start(dir, px(0.))
            } else {
                el.inset_end(dir, px(0.))
            };
            theme.refine(el.style(), "tab-scroll-button", &[], None);
            el
        };
        let at_start = scrolls && offset < px(-0.5);
        let at_end = scrolls && offset > -max + px(0.5);
        // The edges with hidden tabs fade (upstream's `mask-image`, fork patch R3).
        if window.supports_offscreen_layers()
            && let Some(mask) = strip_fade(
                at_start,
                at_end,
                t.focus.outline_width.to_pixels(rem) + t.focus.outline_offset.to_pixels(rem),
                t.spacing.s8.to_pixels(rem),
                dir.is_rtl(),
            )
        {
            strip = strip.mask_linear(mask);
        }

        let mut el = div()
            .relative()
            .flex()
            .flex_row()
            .items_stretch()
            .max_w_full()
            .min_w_0();
        if fill {
            el = el.w_full();
        }
        if self.divider {
            el = el
                .border_b(t.border.width)
                .border_color(t.color.border)
                .pb(t.spacing.s1);
        }
        if self.full_bleed
            && let Some(padding) = use_context::<ContainerPadding>(cx)
        {
            el = el
                .margin_start(dir, -padding.inline_start)
                .margin_end(dir, -padding.inline_end)
                .padding_start(dir, padding.inline_start)
                .padding_end(dir, padding.inline_end);
        }
        theme.refine(el.style(), "tab-list", &[("size", size.as_str())], None);
        el.style().refine(&self.style);
        let name = self
            .label
            .unwrap_or_else(|| i18n(cx).t("@astryx.tabList.label"));
        el.id(self.id.clone())
            .role(if is_tablist {
                Role::TabList
            } else {
                Role::Navigation
            })
            .aria_label(name)
            .child(hint.wrap(strip, window, cx))
            .when(at_start, |el| el.child(arrow(-1., "chevronLeft")))
            .when(at_end, |el| el.child(arrow(1., "chevronRight")))
    }
}

/// The strip's edge fade (upstream `fadeStart`, `fadeEnd`, `fadeBoth`): each edge that
/// hides tabs fades from transparent at the strip's own edge (`bleed` in from the scroll
/// box, so tabs never paint past the TabList's box or the arrow that caps the edge) to
/// opaque `fade` further in, wide enough to sit under an arrow. `None` when no edge
/// hides tabs.
pub(crate) fn strip_fade(
    overflow_start: bool,
    overflow_end: bool,
    bleed: gpui::Pixels,
    fade: gpui::Pixels,
    rtl: bool,
) -> Option<gpui::LinearMask> {
    if !overflow_start && !overflow_end {
        return None;
    }
    let stops = rustryx_effects::edge_fade_stops(bleed, fade);
    let axis = if rtl {
        rustryx_effects::MaskAxis::RightToLeft
    } else {
        rustryx_effects::MaskAxis::LeftToRight
    };
    Some(rustryx_effects::edge_mask(
        axis,
        overflow_start.then_some(&stops[..]),
        overflow_end.then_some(&stops[..]),
    ))
}

// ----------------------------------------------------------------------------
// Docs

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

pub const TAB_LIST_DOC: ComponentDoc = ComponentDoc {
    name: "TabList",
    rust_type: "rustryx::components::tabs::TabList",
    props: &[
        p("value", RustApi::Argument("value")),
        p("onChange", RustApi::Method("on_change")),
        p("size", RustApi::Method("size")),
        p("layout", RustApi::Method("layout")),
        p("hasDivider", RustApi::Method("divider")),
        p("isFullBleed", RustApi::Method("full_bleed")),
        p("role", RustApi::Method("pattern")),
        p("overflow", RustApi::Method("overflow")),
        p("children", RustApi::Method("tab / tabs / menu")),
        p("xstyle", RustApi::Trait("Styled")),
    ],
    example: include_str!("../segmented/examples.rs"),
};

pub const TAB_DOC: ComponentDoc = ComponentDoc {
    name: "Tab",
    rust_type: "rustryx::components::tabs::Tab",
    props: &[
        p("value", RustApi::Argument("value")),
        p("label", RustApi::Argument("label")),
        p("isLabelHidden", RustApi::Method("label_hidden")),
        p("href", RustApi::Method("href")),
        p(
            "panelId",
            RustApi::Waived("GPUI's accessibility tree has no panel ids to point at"),
        ),
        p(
            "as",
            RustApi::Waived("the link handler (link_provider) routes every href"),
        ),
        p("icon", RustApi::Method("icon")),
        p("selectedIcon", RustApi::Method("selected_icon")),
        p("endContent", RustApi::Method("end_content")),
        p("xstyle", RustApi::Trait("Styled")),
    ],
    example: include_str!("../segmented/examples.rs"),
};

pub const TAB_MENU_DOC: ComponentDoc = ComponentDoc {
    name: "TabMenu",
    rust_type: "rustryx::components::tabs::TabMenu",
    props: &[
        p("label", RustApi::Argument("label")),
        p("options", RustApi::Argument("options")),
    ],
    example: include_str!("../segmented/examples.rs"),
};
