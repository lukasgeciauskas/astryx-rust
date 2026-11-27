//! Upstream `DropdownMenu` and `MoreMenu`: a menu behind a button.
//!
//! - The trigger opens the menu on a mouse press-down (FR8), with the press
//!   model's settle rule on its release; a finger opens it as it presses (after the
//!   content-touch delay in a scroller) and picks after UIMenu's handoff; a click, Enter
//!   or Space open it too, and ArrowDown or ArrowUp open it on the first or last row
//!   (FR12).
//!   It reports `aria-expanded` and `aria-haspopup`, and keeps the pressed
//!   tint while the menu is open.
//! - The menu opens in a Popover below the trigger, at least as wide as it
//!   and at most 300px tall (`menuMaxHeight` lifts that); with a header or
//!   footer it is a modal dialog holding them around the menu (FR15).
//! - The sheet presentations hold the rows in a BottomSheet as touch rows,
//!   with submenus drilling in behind a Back button.

use std::rc::Rc;

use gpui::{
    AnyElement, App, ElementId, HasPopup, IntoElement, Length, ParentElement, Pixels, RenderOnce,
    SharedString, StyleRefinement, Styled, Window, div, prelude::*, px,
};
use rustryx_i18n::current as i18n;
use rustryx_primitives::{ElementSize, LayerAlign, Placement, provide};
use rustryx_theme::current;

use super::disclosure::Disclosure;
use super::menu::{
    Initial, MenuAction, MenuEntry, MenuList, MenuRoot, MenuSize, MenuTree, menu_block_cap,
    menu_root, surface_probe, sync_root,
};
use super::motion::viewport_cap;
use super::popover::{
    Popover, PopoverController, PopoverPresentation, PopoverRole, presents_as_sheet,
};
use crate::button::{Button, ButtonVariant, IconButton};
use crate::icon::{Icon, IconSize};
use crate::layout::SpacingStep;
use crate::text::{Heading, HeadingLevel};

type OpenFn = Rc<dyn Fn(bool, &mut Window, &mut App)>;

/// The trigger's contract with its menu: toggling, the press model's
/// press-down, hover and the opening arrow keys.
pub(crate) fn menu_disclosure(
    root: &gpui::Entity<MenuRoot>,
    controller: &PopoverController,
    expanded: bool,
    popup: HasPopup,
    cx: &App,
) -> Disclosure {
    let press = root.read(cx).press.clone();
    // The trigger's press (upstream `onTriggerPress`): a mouse press, or a finger's
    // UIMenu open, opens the menu under the held pointer, or closes an open one.
    {
        let (root, controller) = (root.clone(), controller.clone());
        press.set_trigger_press(move |_, window, cx| {
            if controller.is_open(cx) {
                controller.hide(window, cx);
                return false;
            }
            root.update(cx, |r, _| r.initial = Some(Initial::Container));
            controller.show(window, cx);
            true
        });
    }
    let toggle = {
        let (root, controller, press) = (root.clone(), controller.clone(), press.clone());
        Rc::new(move |window: &mut Window, cx: &mut App| {
            let keyboard = window.last_input_was_keyboard();
            // The press already opened or closed it.
            if press.is_trigger_click_from_press() && !keyboard {
                return;
            }
            if controller.is_open(cx) {
                controller.hide(window, cx);
            } else {
                root.update(cx, |r, _| {
                    r.initial = Some(if keyboard {
                        Initial::First
                    } else {
                        Initial::Container
                    });
                });
                controller.show(window, cx);
            }
        }) as Rc<dyn Fn(&mut Window, &mut App)>
    };
    let on_key = {
        let (root, controller) = (root.clone(), controller.clone());
        Rc::new(
            move |event: &gpui::KeyDownEvent, window: &mut Window, cx: &mut App| {
                let key = event.keystroke.key.as_str();
                if controller.is_open(cx) || !(key == "down" || key == "up") {
                    return false;
                }
                root.update(cx, |r, _| {
                    r.initial = Some(if key == "up" {
                        Initial::Last
                    } else {
                        Initial::First
                    });
                    r.opening_key = Some(key.to_string().into());
                });
                controller.show(window, cx);
                true
            },
        ) as Rc<dyn Fn(&gpui::KeyDownEvent, &mut Window, &mut App) -> bool>
    };
    Disclosure {
        expanded,
        popup,
        toggle: Some(toggle),
        menu_press: Some(press),
        on_hover: None,
        on_key: Some(on_key),
        open_tint: true,
        detached: false,
    }
}

/// A menu of actions behind a button, upstream `DropdownMenu`.
///
/// ```ignore
/// DropdownMenu::new("sort")
///     .button_label("Sort")
///     .entry(MenuItem::new("newest", "Newest"))
///     .entry(MenuItem::new("oldest", "Oldest"))
/// ```
#[derive(IntoElement)]
pub struct DropdownMenu {
    id: ElementId,
    button_label: Option<SharedString>,
    button_variant: ButtonVariant,
    button_size: Option<ElementSize>,
    button_icon: Option<AnyElement>,
    icon_only: bool,
    button: Option<Box<dyn FnOnce(Button) -> Button>>,
    trigger: Option<AnyElement>,
    entries: Vec<MenuEntry>,
    /// `None` when omitted: the anchored menu with the touch press model.
    presentation: Option<PopoverPresentation>,
    open: Option<bool>,
    on_open_change: Option<OpenFn>,
    menu_width: Option<Length>,
    menu_max_height: Option<Pixels>,
    header: Option<AnyElement>,
    footer: Option<AnyElement>,
    placement: Placement,
    alignment: LayerAlign,
    on_click: Option<MenuAction>,
    chevron: bool,
    size: MenuSize,
    label: Option<SharedString>,
    style: StyleRefinement,
}

impl DropdownMenu {
    pub fn new(id: impl Into<ElementId>) -> Self {
        DropdownMenu {
            id: id.into(),
            button_label: None,
            button_variant: ButtonVariant::Secondary,
            button_size: None,
            button_icon: None,
            icon_only: false,
            button: None,
            trigger: None,
            entries: Vec::new(),
            presentation: None,
            open: None,
            on_open_change: None,
            menu_width: None,
            menu_max_height: None,
            header: None,
            footer: None,
            placement: Placement::Below,
            alignment: LayerAlign::Start,
            on_click: None,
            chevron: true,
            size: MenuSize::Md,
            label: None,
            style: StyleRefinement::default(),
        }
    }

    /// Upstream `button.label`: the built-in trigger's text, and the menu's
    /// name ("Menu" by default).
    pub fn button_label(mut self, label: impl Into<SharedString>) -> Self {
        self.button_label = Some(label.into());
        self
    }

    /// Upstream `button.variant`: secondary by default.
    pub fn button_variant(mut self, variant: ButtonVariant) -> Self {
        self.button_variant = variant;
        self
    }

    /// Upstream `button.size`.
    pub fn button_size(mut self, size: ElementSize) -> Self {
        self.button_size = Some(size);
        self
    }

    /// Upstream `button.icon`.
    pub fn button_icon(mut self, icon: impl IntoElement) -> Self {
        self.button_icon = Some(icon.into_any_element());
        self
    }

    /// Upstream `button.isIconOnly`: the label names the button and shows
    /// as its tooltip; there is no chevron.
    pub fn icon_only(mut self, icon_only: bool) -> Self {
        self.icon_only = icon_only;
        self
    }

    /// Upstream `button`'s other Button props (disabled, tooltip, width,
    /// elevation...): `f` adjusts the built-in trigger.
    pub fn button(mut self, f: impl FnOnce(Button) -> Button + 'static) -> Self {
        self.button = Some(Box::new(f));
        self
    }

    /// Upstream `renderTrigger`: any Button-based control as the trigger. It
    /// takes the menu's `aria-expanded`, `aria-haspopup`, press model and
    /// keys (FR9); a disabled one never opens the menu.
    pub fn trigger(mut self, trigger: impl IntoElement) -> Self {
        self.trigger = Some(trigger.into_any_element());
        self
    }

    /// Upstream `items`.
    pub fn items(mut self, entries: impl IntoIterator<Item = MenuEntry>) -> Self {
        self.entries.extend(entries);
        self
    }

    /// Upstream compound `children`: one entry.
    pub fn entry(mut self, entry: impl Into<MenuEntry>) -> Self {
        self.entries.push(entry.into());
        self
    }

    /// Upstream `presentation`: `Popover` stays anchored, `BottomSheet` always holds
    /// the rows in a modal BottomSheet, and `Adaptive` uses the sheet on a compact touch
    /// screen and stays anchored elsewhere. Omitted, the menu is anchored, and under a
    /// finger or a pen its trigger opens as it is pressed (inside a scroller, after the
    /// 150 ms content-touch delay) and the opening finger picks after moving 10 px; an
    /// explicit `Popover` keeps the pointer behavior on every device, a 500 ms hold
    /// included.
    pub fn presentation(mut self, presentation: PopoverPresentation) -> Self {
        self.presentation = Some(presentation);
        self
    }

    /// Upstream `isMenuOpen`: a controlled menu.
    pub fn open(mut self, open: impl Into<Option<bool>>) -> Self {
        self.open = open.into();
        self
    }

    /// Upstream `onOpenChange`.
    pub fn on_open_change(mut self, f: impl Fn(bool, &mut Window, &mut App) + 'static) -> Self {
        self.on_open_change = Some(Rc::new(f));
        self
    }

    /// Upstream `menuWidth`: the least width, clamped to the viewport;
    /// without it the menu is at least as wide as its trigger.
    pub fn menu_width(mut self, width: impl Into<Length>) -> Self {
        self.menu_width = Some(width.into());
        self
    }

    /// Upstream `menuMaxHeight`: replaces the 300px cap (FR7).
    pub fn menu_max_height(mut self, height: impl Into<Pixels>) -> Self {
        self.menu_max_height = Some(height.into());
        self
    }

    /// Upstream `header`: above the rows, outside the menu (FR15).
    pub fn header(mut self, header: impl IntoElement) -> Self {
        self.header = Some(header.into_any_element());
        self
    }

    /// Upstream `footer`: below the rows; Tab moves into it.
    pub fn footer(mut self, footer: impl IntoElement) -> Self {
        self.footer = Some(footer.into_any_element());
        self
    }

    /// Upstream `placement`: below by default.
    pub fn placement(mut self, placement: Placement) -> Self {
        self.placement = placement;
        self
    }

    /// Upstream `alignment`: start by default.
    pub fn alignment(mut self, alignment: LayerAlign) -> Self {
        self.alignment = alignment;
        self
    }

    /// Upstream `onClick`: the built-in trigger was activated.
    pub fn on_click(mut self, f: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Rc::new(f));
        self
    }

    /// Upstream `hasChevron`: on by default.
    pub fn chevron(mut self, chevron: bool) -> Self {
        self.chevron = chevron;
        self
    }

    /// Upstream `size` (`dropdown-menu-item`'s): the rows' density.
    pub fn size(mut self, size: MenuSize) -> Self {
        self.size = size;
        self
    }

    /// The menu's accessible name, when it differs from the button label.
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }
}

impl Styled for DropdownMenu {
    /// Upstream `xstyle`: the menu surface.
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for DropdownMenu {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let button_label = self
            .button_label
            .clone()
            .unwrap_or_else(|| i18n(cx).t("@astryx.dropdownMenu.label"));
        let label = self.label.clone().unwrap_or_else(|| button_label.clone());
        let controller = PopoverController::new(self.id.clone(), window, cx);
        let root = menu_root(&self.id, window, cx);
        let open = self.open.unwrap_or_else(|| controller.is_open(cx));
        sync_root(&root, open, window, cx);
        let sheet = presents_as_sheet(
            self.presentation.unwrap_or(PopoverPresentation::Popover),
            window,
            cx,
        );
        // Upstream `pressTouchModel`: an explicit `Popover` keeps the pointer model, the
        // 500 ms hold before a finger opens it; otherwise a finger opens it as UIMenu does.
        let pointer_model = self.presentation == Some(PopoverPresentation::Popover);
        root.read(cx).press.set_long_press_delay(
            pointer_model.then_some(crate::gestures::menu_press::MENU_PRESS_LONG_PRESS_MS),
        );
        let regions = self.header.is_some() || self.footer.is_some();
        let close: MenuAction = {
            let controller = controller.clone();
            Rc::new(move |window, cx| controller.hide(window, cx))
        };
        let cap = self
            .menu_max_height
            .unwrap_or_else(menu_block_cap)
            .min(viewport_cap(t, window).height);
        let tree = MenuTree {
            root: root.clone(),
            close,
            size: self.size,
            tab_leaves: !regions && !sheet,
            touch: sheet,
            drill: sheet || root.read(cx).compact_touch,
            max_height: if sheet { px(100_000.) } else { cap },
            regions: true,
        };
        let list = MenuList::new(
            ElementId::Name(format!("{}-list", self.id).into()),
            self.entries,
            label.clone(),
            tree,
        );

        // Under a finger, where the primary pointer is coarse, a tap or a drag during the
        // entrance reaches nothing, as in UIMenu: the menu's own box is held (the page
        // stays live) under the UIMenu touch model. Keys and assistive technology stay
        // live (upstream `holdEntranceInput`).
        {
            use rustryx_primitives::gestures::input_hold::{
                HoldScope, hold_entrance_input, release_input_hold,
            };
            let hold_key = ElementId::Name(format!("{}-entrance-hold", self.id).into());
            let (opened_at, surface) = {
                let r = root.read(cx);
                let surface = r
                    .surfaces
                    .iter()
                    .find(|(depth, _)| *depth == 0)
                    .map(|(_, b)| *b);
                (r.opened_at, surface)
            };
            match (open && !sheet && !pointer_model, opened_at, surface) {
                (true, Some(since), Some(bounds)) => {
                    hold_entrance_input(
                        window,
                        hold_key,
                        HoldScope::Surface(bounds),
                        since,
                        t.duration.fast_max,
                        None,
                        cx,
                    );
                }
                _ => release_input_hold(window, &hold_key, cx),
            }
        }
        let drilled = root.read(cx).drilled > 0;
        let content = if sheet {
            let region = |el: AnyElement| div().px(t.spacing.s3).py(t.spacing.s2).child(el);
            div()
                .relative()
                .w_full()
                .px(t.spacing.s1)
                .flex()
                .flex_col()
                .child(surface_probe(&root, 0, true))
                .when(!drilled, |el| {
                    el.child(
                        div()
                            .ml(t.spacing.s3)
                            .mb(t.spacing.s2)
                            .child(Heading::new(HeadingLevel::H3, label.clone())),
                    )
                })
                .children(self.header.map(region))
                .child(list)
                .children(self.footer.map(region))
        } else {
            let region = |el: AnyElement| div().flex_none().p(t.spacing.s1).child(el);
            let mut frame = div()
                .relative()
                .flex()
                .flex_col()
                .child(surface_probe(&root, 0, true))
                .child(rustryx_primitives::gestures::input_hold::hold_probe())
                .children(self.header.map(region))
                .child(list)
                .children(self.footer.map(region));
            if let Some(width) = self.menu_width {
                frame = frame.min_w(width);
            }
            frame
        };

        let trigger = self.trigger.unwrap_or_else(|| {
            let id = ElementId::Name(format!("{}-trigger", self.id).into());
            let mut button = if self.icon_only {
                let icon = self
                    .button_icon
                    .unwrap_or_else(|| Icon::new("moreHorizontal").into_any_element());
                let mut b = IconButton::new(id, button_label.clone(), icon)
                    .variant(self.button_variant)
                    .tooltip(button_label.clone());
                if let Some(size) = self.button_size {
                    b = b.size(size);
                }
                if let Some(f) = self.on_click.clone() {
                    b = b.on_click(move |_, window, cx| f(window, cx));
                }
                return b.into_any_element();
            } else {
                Button::new(id, button_label.clone()).variant(self.button_variant)
            };
            if let Some(icon) = self.button_icon {
                button = button.icon(icon);
            }
            if let Some(size) = self.button_size {
                button = button.size(size);
            }
            if self.chevron {
                button = button.end_content(Icon::new("chevronDown").size(IconSize::Sm));
            }
            if let Some(f) = self.on_click.clone() {
                button = button.on_click(move |_, window, cx| f(window, cx));
            }
            if let Some(f) = self.button {
                button = f(button);
            }
            button.into_any_element()
        });
        let popup = if regions {
            HasPopup::Dialog
        } else {
            HasPopup::Menu
        };
        let trigger = provide(
            menu_disclosure(&root, &controller, open, popup, cx),
            trigger,
        );

        let mut popover = Popover::new(self.id, label)
            .controller(controller)
            .role(if regions {
                PopoverRole::Dialog
            } else {
                PopoverRole::None
            })
            .modal(regions)
            .padding(SpacingStep::S0)
            .close_button(false)
            .auto_focus(false)
            .escape_dismiss(!drilled)
            .placement(self.placement)
            .alignment(self.alignment)
            .presentation(if sheet {
                PopoverPresentation::BottomSheet
            } else {
                PopoverPresentation::Popover
            })
            .open(self.open)
            // A finger's open grows the menu out of its trigger under the UIMenu model
            // (upstream `surfaceMotion.present` with the touch open).
            .touch_motion(Some(!pointer_model && window.last_input_was_touch()))
            .content(content)
            .child(trigger);
        if self.menu_width.is_some() {
            // An explicit width stops matching the trigger.
            popover = popover.width(Length::Auto);
        }
        if let Some(f) = self.on_open_change {
            popover = popover.on_open_change(move |open, window, cx| f(open, window, cx));
        }
        *popover.style() = self.style;
        popover
    }
}

/// An overflow menu behind a three-dot button, upstream `MoreMenu`.
#[derive(IntoElement)]
pub struct MoreMenu {
    id: ElementId,
    entries: Vec<MenuEntry>,
    label: Option<SharedString>,
    variant: ButtonVariant,
    size: Option<ElementSize>,
    icon: Option<AnyElement>,
    disabled: bool,
    placement: Placement,
    alignment: LayerAlign,
    presentation: Option<PopoverPresentation>,
    open: Option<bool>,
    on_open_change: Option<OpenFn>,
    style: StyleRefinement,
}

impl MoreMenu {
    /// Upstream `items`.
    pub fn new(id: impl Into<ElementId>, entries: impl IntoIterator<Item = MenuEntry>) -> Self {
        MoreMenu {
            id: id.into(),
            entries: entries.into_iter().collect(),
            label: None,
            variant: ButtonVariant::Ghost,
            size: None,
            icon: None,
            disabled: false,
            placement: Placement::Below,
            alignment: LayerAlign::Start,
            presentation: None,
            open: None,
            on_open_change: None,
            style: StyleRefinement::default(),
        }
    }

    rustryx_primitives::setters! {
        /// Upstream `label`: the button's name and tooltip ("More options").
        ?label: SharedString,
        /// Upstream `variant`: ghost by default.
        variant: ButtonVariant,
        /// Upstream `size`.
        ?size: ElementSize,
        /// Upstream `isDisabled`.
        !disabled,
        /// Upstream `placement`.
        placement: Placement,
        /// Upstream `alignment`.
        alignment: LayerAlign,
        /// Upstream `presentation`, forwarded to DropdownMenu: omitted, the anchored
        /// menu with the touch press model; `Popover` keeps the pointer model.
        ?presentation: PopoverPresentation,
    }

    /// Upstream `icon`: instead of the three dots.
    pub fn icon(mut self, icon: impl IntoElement) -> Self {
        self.icon = Some(icon.into_any_element());
        self
    }

    /// Upstream `isMenuOpen`.
    pub fn open(mut self, open: impl Into<Option<bool>>) -> Self {
        self.open = open.into();
        self
    }

    /// Upstream `onOpenChange`.
    pub fn on_open_change(mut self, f: impl Fn(bool, &mut Window, &mut App) + 'static) -> Self {
        self.on_open_change = Some(Rc::new(f));
        self
    }
}

impl Styled for MoreMenu {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for MoreMenu {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let label = self
            .label
            .clone()
            .unwrap_or_else(|| i18n(cx).t("@astryx.moreMenu.label"));
        let icon = self
            .icon
            .unwrap_or_else(|| Icon::new("moreHorizontal").into_any_element());
        let mut button = IconButton::new(
            ElementId::Name(format!("{}-trigger", self.id).into()),
            label.clone(),
            icon,
        )
        .variant(self.variant)
        .tooltip(label.clone())
        .disabled(self.disabled);
        if let Some(size) = self.size {
            button = button.size(size);
        }
        let mut menu = DropdownMenu::new(self.id)
            .button_label(label)
            .trigger(button)
            .chevron(false)
            .items(self.entries)
            .placement(self.placement)
            .alignment(self.alignment)
            .open(self.open);
        if let Some(presentation) = self.presentation {
            menu = menu.presentation(presentation);
        }
        if let Some(f) = self.on_open_change {
            menu = menu.on_open_change(move |open, window, cx| f(open, window, cx));
        }
        *menu.style() = self.style;
        menu
    }
}

/// A menu trigger that is not a Button (a breadcrumb's link-styled one): it
/// takes the menu's Disclosure as Button does, its `aria-expanded`,
/// `aria-haspopup`, toggle, press model and keys.
#[derive(IntoElement)]
pub(crate) struct MenuTrigger {
    id: ElementId,
    label: SharedString,
    content: Box<dyn FnOnce(bool) -> AnyElement>,
    current: bool,
    underline: bool,
    focus: Option<gpui::FocusHandle>,
    arrow_keys: bool,
    hover_bg: bool,
    style: StyleRefinement,
}

impl MenuTrigger {
    /// `content` is built knowing whether the menu is open.
    pub(crate) fn new(
        id: impl Into<ElementId>,
        label: impl Into<SharedString>,
        content: impl FnOnce(bool) -> AnyElement + 'static,
    ) -> Self {
        MenuTrigger {
            id: id.into(),
            label: label.into(),
            content: Box::new(content),
            current: false,
            underline: false,
            focus: None,
            arrow_keys: true,
            hover_bg: false,
            style: StyleRefinement::default(),
        }
    }

    /// Takes focus through `handle` (a roving list's) instead of its own.
    pub(crate) fn focus_handle(mut self, handle: gpui::FocusHandle) -> Self {
        self.focus = Some(handle);
        self
    }

    /// Whether ArrowDown and ArrowUp open the menu; off where the arrows
    /// move between the trigger and its siblings (a tab strip).
    pub(crate) fn arrow_keys(mut self, arrow_keys: bool) -> Self {
        self.arrow_keys = arrow_keys;
        self
    }

    /// The hover and press tints of a tab.
    pub(crate) fn hover_tint(mut self) -> Self {
        self.hover_bg = true;
        self
    }

    /// The current page's item (`aria-current="page"`).
    pub(crate) fn current(mut self, current: bool) -> Self {
        self.current = current;
        self
    }

    /// A link's look: underlined under the pointer.
    pub(crate) fn underline_on_hover(mut self) -> Self {
        self.underline = true;
        self
    }
}

impl Styled for MenuTrigger {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for MenuTrigger {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let rem = window.rem_size();
        let focus = match self.focus {
            Some(focus) => focus,
            None => rustryx_primitives::control_focus(&self.id, window, cx),
        };
        // A finger's press paints as UIKit's controls do (upstream `usePressFeedback`).
        let feedback = crate::gestures::use_press_feedback(self.id.clone(), window, cx)
            .space_activates(&focus);
        let mut el = div()
            .id(self.id)
            .relative()
            .track_focus(&focus)
            .cursor_pointer()
            .role(gpui::Role::Button)
            .aria_label(self.label);
        el.style().refine(&self.style);
        if self.hover_bg {
            let (hover, pressed) = (t.color.overlay_hover, t.color.overlay_pressed);
            el = el.hover(move |s| s.bg(hover));
            if !feedback.paints_buttons(window, cx) {
                el = el.active(move |s| s.bg(pressed));
            }
            if let Some(overlay) = feedback.overlay(pressed, window, cx) {
                el = el.bg(overlay);
            }
        }
        let disclosure = super::disclosure::disclosure(cx);
        let expanded = disclosure.as_ref().is_some_and(|d| d.expanded);
        let menu_press = disclosure.as_ref().and_then(|d| d.menu_press.clone());
        if let Some(d) = disclosure {
            el = el.aria_expanded(d.expanded).aria_has_popup(d.popup);
            if let Some(toggle) = d.toggle {
                el = el.on_click(move |_, window, cx| {
                    cx.stop_propagation();
                    toggle(window, cx)
                });
            }
            if let Some(hover) = d.on_hover {
                el = el.on_hover(move |hovered, window, cx| hover(*hovered, window, cx));
            }
            if let Some(key) = d.on_key.filter(|_| self.arrow_keys) {
                el = el.on_key_down(move |event, window, cx| {
                    if key(event, window, cx) {
                        cx.stop_propagation();
                    }
                });
            }
        }
        if self.current {
            el = el.aria_current(gpui::AriaCurrent::Page);
        }
        if self.underline {
            el = el.hover(|s| s.underline());
        }
        let radius = el
            .style()
            .corner_radii
            .top_left
            .map(|r| r.to_pixels(rem))
            .unwrap_or(px(0.));
        if rustryx_primitives::is_focus_visible(&focus, window) {
            el = el.child(crate::focus::focus_ring(t, radius, rem));
        }
        let el = feedback.surface(el.child((self.content)(expanded)));
        match menu_press {
            Some(press) => press.trigger(el).into_any_element(),
            None => el.into_any_element(),
        }
    }
}
