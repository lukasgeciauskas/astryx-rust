//! NavHeadingMenu and NavHeadingMenuItem, upstream `NavMenu/` (component spec
//! `NavHeadingMenu.spec.md`): the menu a SideNavHeading or TopNavHeading opens. One
//! `menu` of `menuitem` rows with the APG keyboard model (arrows that wrap, Home,
//! End, typeahead, Enter and Space), disabled rows that stay reachable but inert, and a
//! press that slides across the rows and acts on the one it lifts on.

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{
    AnyElement, App, ElementId, FocusHandle, IntoElement, KeyDownEvent, Length, MouseButton,
    ParentElement, Pixels, RenderOnce, Role, SharedString, StyleRefinement, Styled, Window, div,
    prelude::*, px,
};
use rustryx_primitives::typeahead::Typeahead;
use rustryx_primitives::{is_focus_visible, provide, use_context, use_link_handler};
use rustryx_theme::{ThemeStyled, current};

use crate::gestures::menu_press::{MenuPress, MenuPressOptions, MenuPressRow, use_menu_press};
use crate::icon::{Icon, IconColor, IconSize};
use crate::text::{Text, TextType};

type Action = Rc<dyn Fn(&mut Window, &mut App)>;

/// Upstream `NavHeadingMenuSize`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NavHeadingMenuSize {
    Sm,
    #[default]
    Md,
    Lg,
}

impl NavHeadingMenuSize {
    fn as_str(self) -> &'static str {
        match self {
            NavHeadingMenuSize::Sm => "sm",
            NavHeadingMenuSize::Md => "md",
            NavHeadingMenuSize::Lg => "lg",
        }
    }

    fn min_width(self) -> Pixels {
        match self {
            NavHeadingMenuSize::Sm => px(160.),
            NavHeadingMenuSize::Md => px(200.),
            NavHeadingMenuSize::Lg => px(240.),
        }
    }
}

/// Upstream `NavHeadingCloseContext`: the heading's popover closes on a choice and on
/// Escape.
#[derive(Clone)]
pub(crate) struct NavHeadingClose(pub Action);

/// Upstream `NavHeadingDismissContext`: where a choice does not close (an expanded
/// SideNavHeading), a mouse released outside still dismisses.
#[derive(Clone)]
pub(crate) struct NavHeadingDismiss(pub Action);

/// Upstream `NavHeadingMenuHostContext`: inside a heading's own named menu, a
/// NavHeadingMenu draws no menu of its own.
#[derive(Clone, Copy)]
pub(crate) struct NavHeadingMenuHost;

/// One registered row.
#[derive(Clone)]
struct Row {
    focus: FocusHandle,
    label: SharedString,
    disabled: bool,
    activate: Action,
}

#[derive(Default)]
struct MenuState {
    rows: Vec<Row>,
    fresh: bool,
    typeahead: Typeahead,
    /// The row that holds the menu's one tab stop (the roving tab index).
    active: usize,
}

/// What a menu shares with its rows.
#[derive(Clone)]
struct MenuScope {
    state: Rc<RefCell<MenuState>>,
    size: NavHeadingMenuSize,
    close: Option<Action>,
    /// FR8: the menu press model (upstream `useMenuPress`).
    press: MenuPress,
}

impl MenuScope {
    /// Adds a row; its index.
    fn register(&self, row: Row) -> usize {
        let mut s = self.state.borrow_mut();
        if s.fresh {
            s.rows.clear();
            s.fresh = false;
        }
        s.rows.push(row);
        s.rows.len() - 1
    }
}

/// Upstream `NavHeadingMenu`.
#[derive(IntoElement)]
pub struct NavHeadingMenu {
    id: ElementId,
    children: Vec<AnyElement>,
    size: NavHeadingMenuSize,
    min_width: Option<Length>,
    label: Option<SharedString>,
    style: StyleRefinement,
}

impl NavHeadingMenu {
    pub fn new(id: impl Into<ElementId>) -> Self {
        NavHeadingMenu {
            id: id.into(),
            children: Vec::new(),
            size: NavHeadingMenuSize::Md,
            min_width: None,
            label: None,
            style: StyleRefinement::default(),
        }
    }

    /// Upstream `size`: the minimum width and the rows' padding.
    pub fn size(mut self, size: NavHeadingMenuSize) -> Self {
        self.size = size;
        self
    }

    /// Upstream `minWidth`: wins over the size's.
    pub fn min_width(mut self, width: impl Into<Length>) -> Self {
        self.min_width = Some(width.into());
        self
    }

    /// A standalone menu's name (upstream's `aria-label`).
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }
}

impl ParentElement for NavHeadingMenu {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl Styled for NavHeadingMenu {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for NavHeadingMenu {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens().clone();
        let state = window
            .use_keyed_state(
                ElementId::Name(format!("{:?}-nav-menu", self.id).into()),
                cx,
                |_, _| Rc::new(RefCell::new(MenuState::default())),
            )
            .read(cx)
            .clone();
        state.borrow_mut().fresh = true;
        let close = use_context::<NavHeadingClose>(cx).map(|c| c.0);
        let dismiss = use_context::<NavHeadingDismiss>(cx).map(|d| d.0);
        let hosted = use_context::<NavHeadingMenuHost>(cx).is_some();
        // FR8: a press that slides across the rows acts on the row it lifts on, the
        // highlight following the pointer; a mouse released outside dismisses. A press on
        // caller content beside the rows stays that content's.
        let mut options = MenuPressOptions::default().press_on_items_only(true);
        if let Some(dismiss) = dismiss.or(close.clone()) {
            options = options.on_dismiss(move |window, cx| dismiss(window, cx));
        }
        let press = use_menu_press(
            ElementId::Name(format!("{:?}-nav-menu-press", self.id).into()),
            options,
            window,
            cx,
        );
        let scope = MenuScope {
            state: state.clone(),
            size: self.size,
            close: close.clone(),
            press: press.clone(),
        };
        let mut menu = div()
            .id(self.id.clone())
            .flex()
            .flex_col()
            .gap(t.spacing.s0_5)
            .min_w(self.min_width.unwrap_or(self.size.min_width().into()));
        // FR1: one menu; inside a heading's own menu, none.
        if !hosted {
            menu = menu.role(Role::Menu);
            if let Some(label) = self.label {
                menu = menu.aria_label(label);
            }
        }
        theme.refine(
            menu.style(),
            "nav-heading-menu",
            &[("size", self.size.as_str())],
            None,
        );
        menu.style().refine(&self.style);
        // FR5-FR7: the keyboard model over the rows, disabled ones included.
        let keys = (state.clone(), close.clone());
        menu = menu.on_key_down(move |event: &KeyDownEvent, window, cx| {
            let (state, close) = (&keys.0, &keys.1);
            let stroke = &event.keystroke;
            let rows = state.borrow().rows.clone();
            let current = rows.iter().position(|r| r.focus.is_focused(window));
            let focus = |index: usize, window: &mut Window, cx: &mut App| {
                state.borrow_mut().active = index;
                rows[index].focus.focus(window, cx);
            };
            match stroke.key.as_str() {
                "enter" | "space"
                    if state.borrow().typeahead.buffer().is_empty() || stroke.key == "enter" =>
                {
                    if let Some(row) = current.map(|i| rows[i].clone()) {
                        cx.stop_propagation();
                        if !row.disabled {
                            (row.activate)(window, cx);
                        }
                    }
                }
                "down" | "up" if !rows.is_empty() => {
                    cx.stop_propagation();
                    let n = rows.len();
                    let next = match (stroke.key.as_str(), current) {
                        ("down", Some(i)) => (i + 1) % n,
                        ("up", Some(i)) => (i + n - 1) % n,
                        ("down", None) => 0,
                        _ => n - 1,
                    };
                    focus(next, window, cx);
                }
                "home" if !rows.is_empty() => {
                    cx.stop_propagation();
                    focus(0, window, cx);
                }
                "end" if !rows.is_empty() => {
                    cx.stop_propagation();
                    focus(rows.len() - 1, window, cx);
                }
                "escape" => {
                    if let Some(close) = close {
                        cx.stop_propagation();
                        close(window, cx);
                    }
                }
                _ => {
                    let labels: Vec<SharedString> = rows.iter().map(|r| r.label.clone()).collect();
                    let found = state.borrow_mut().typeahead.key(
                        stroke,
                        rows.len(),
                        |i| labels.get(i).map(|l| l.as_ref()),
                        current,
                        |_| false,
                    );
                    if let Some(found) = found {
                        cx.stop_propagation();
                        if let Some(index) = found {
                            focus(index, window, cx);
                        }
                    }
                }
            }
        });
        press.menu(
            menu.child(provide(
                scope,
                div()
                    .flex()
                    .flex_col()
                    .gap(t.spacing.s0_5)
                    .children(self.children),
            )),
            true,
        )
    }
}

/// Upstream `NavHeadingMenuItem`.
#[derive(IntoElement)]
pub struct NavHeadingMenuItem {
    id: ElementId,
    label: SharedString,
    custom_label: Option<AnyElement>,
    description: Option<SharedString>,
    icon: Option<AnyElement>,
    href: Option<SharedString>,
    on_click: Option<Action>,
    disabled: bool,
    keeps_open: bool,
    boxed_icon: bool,
    style: StyleRefinement,
}

impl NavHeadingMenuItem {
    /// `label` renders as one line of body text, and is what typeahead reads.
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        NavHeadingMenuItem {
            id: id.into(),
            label: label.into(),
            custom_label: None,
            description: None,
            icon: None,
            href: None,
            on_click: None,
            disabled: false,
            keeps_open: false,
            boxed_icon: false,
            style: StyleRefinement::default(),
        }
    }

    /// A choice leaves the menu open (TopNavMenu's `hasCloseOnSelect: false`).
    pub(crate) fn keeps_open(mut self, keeps_open: bool) -> Self {
        self.keeps_open = keeps_open;
        self
    }

    /// TopNavMenu's row: the icon in a 40 px box, a semibold title.
    pub(crate) fn boxed_icon(mut self) -> Self {
        self.boxed_icon = true;
        self
    }

    /// A non-string `label`: rendered as given (the string still names it).
    pub fn label_element(mut self, label: impl IntoElement) -> Self {
        self.custom_label = Some(label.into_any_element());
        self
    }

    /// Upstream `description`: one line under the label.
    pub fn description(mut self, description: impl Into<SharedString>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Upstream `icon`: an icon name.
    pub fn icon(mut self, icon: &'static str) -> Self {
        self.icon = Some(
            Icon::new(icon)
                .size(IconSize::Sm)
                .color(IconColor::Secondary)
                .into_any_element(),
        );
        self
    }

    /// Upstream `icon` as an element.
    pub fn icon_element(mut self, icon: impl IntoElement) -> Self {
        self.icon = Some(icon.into_any_element());
        self
    }

    /// Upstream `href`: the row navigates through the link provider.
    pub fn href(mut self, href: impl Into<SharedString>) -> Self {
        self.href = Some(href.into());
        self
    }

    /// Upstream `onClick`: runs when the enabled row is chosen, before the menu closes.
    pub fn on_click(mut self, f: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Rc::new(f));
        self
    }

    /// Upstream `isDisabled`: reachable by keys and typeahead, but inert.
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl Styled for NavHeadingMenuItem {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for NavHeadingMenuItem {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens().clone();
        let scope = use_context::<MenuScope>(cx);
        let size = scope.as_ref().map_or(NavHeadingMenuSize::Md, |s| s.size);
        let focus = window
            .use_keyed_state(
                ElementId::Name(format!("{:?}-nav-item", self.id).into()),
                cx,
                |_, cx| cx.focus_handle().tab_stop(false),
            )
            .read(cx)
            .clone();
        let links = use_link_handler(cx);
        // FR6: an enabled choice runs onClick, follows its href, then closes.
        let activate: Action = {
            let (on_click, href, close) = (
                self.on_click.clone(),
                self.href.clone(),
                scope
                    .as_ref()
                    .and_then(|s| s.close.clone())
                    .filter(|_| !self.keeps_open),
            );
            Rc::new(move |window, cx| {
                if let Some(f) = &on_click {
                    f(window, cx);
                }
                if let Some(href) = &href {
                    links.open(href, None, window, cx);
                }
                if let Some(close) = &close {
                    close(window, cx);
                }
            })
        };
        // One tab stop: the focused row, else the last one focused, else the first.
        let index = scope.as_ref().map(|scope| {
            let index = scope.register(Row {
                focus: focus.clone(),
                label: self.label.clone(),
                disabled: self.disabled,
                activate: activate.clone(),
            });
            let mut s = scope.state.borrow_mut();
            if focus.is_focused(window) {
                s.active = index;
            }
            let _ = focus.clone().tab_stop(s.active == index);
            index
        });
        let (pad_y, pad_x) = match size {
            NavHeadingMenuSize::Sm => (t.spacing.s1, t.spacing.s2),
            NavHeadingMenuSize::Md => (t.spacing.s2, t.spacing.s2),
            NavHeadingMenuSize::Lg => (t.spacing.s3, t.spacing.s3),
        };
        let focused = focus.is_focused(window);
        let hover = t.color.overlay_hover;
        let row_key = SharedString::from(format!("{}", self.id));
        let mut row = div()
            .id(self.id.clone())
            .debug_selector({
                let s = format!("{}", self.id);
                move || s
            })
            .track_focus(&focus)
            .relative()
            .flex()
            .items_center()
            .gap(t.spacing.s2)
            .w_full()
            .py(pad_y)
            .px(pad_x)
            .rounded(t.radius.element)
            .font_stack(&t.font.family.body)
            .text_style(&t.text.label)
            .text_color(t.color.text_primary)
            .role(Role::MenuItem)
            .aria_label(self.label.clone())
            .aria_disabled(self.disabled);
        if focused {
            row = row.bg(hover);
        }
        if self.disabled {
            row = row.opacity(0.5);
        } else {
            row = row.cursor_pointer().hover(move |s| s.bg(hover));
        }
        theme.refine(
            row.style(),
            "nav-heading-menu-item",
            &[("size", size.as_str())],
            None,
        );
        row.style().refine(&self.style);
        // FR8: the press model picks the row under the release; a press on a row
        // focuses it, as a browser focuses a tabindex=-1 row it is pressed on, and a
        // disabled row keeps focus where it is.
        if let Some(scope) = &scope {
            let (disabled, press_focus) = (self.disabled, focus.clone());
            row = row.on_mouse_down(MouseButton::Left, move |_, window, cx| {
                if disabled {
                    window.prevent_default();
                    cx.stop_propagation();
                    return;
                }
                window.focus(&press_focus, cx);
                window.refresh();
            });
            // A held finger lights the row it is over.
            if scope.press.lit_row().as_deref() == Some(row_key.as_ref()) {
                row = row.bg(t.color.overlay_pressed);
            }
            let _ = index;
        } else if !self.disabled {
            let lift = activate.clone();
            row = row.on_click(move |_, window, cx| lift(window, cx));
        }
        let label = match self.custom_label {
            Some(custom) => custom,
            None if self.boxed_icon => div()
                .text_style(&t.text.label)
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(t.color.text_primary)
                .child(self.label.clone())
                .into_any_element(),
            None => Text::new(self.label.clone())
                .text_type(TextType::Body)
                .max_lines(1)
                .into_any_element(),
        };
        let content = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .child(label)
            .children(
                self.description
                    .map(|d| Text::new(d).text_type(TextType::Supporting).max_lines(1)),
            );
        let (row, content, icon) = if self.boxed_icon {
            let icon = self.icon.map(|icon| {
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .justify_center()
                    .size(px(40.))
                    .rounded(t.radius.element)
                    .bg(t.color.neutral)
                    .child(icon)
                    .into_any_element()
            });
            (row.gap(t.spacing.s3), content.gap(t.spacing.s1), icon)
        } else {
            (row, content, self.icon)
        };
        let mut row = row.children(icon).child(content);
        if focused && is_focus_visible(&focus, window) {
            let ring = format!("{}-focus-ring", self.id);
            row = row.child(
                crate::focus::focus_ring_inset(
                    &t,
                    t.radius.element.to_pixels(window.rem_size()),
                    window.rem_size(),
                )
                .debug_selector(move || ring),
            );
        }
        match scope.filter(|_| !self.disabled) {
            Some(scope) => {
                let lift = activate.clone();
                scope
                    .press
                    .row(
                        MenuPressRow::new(row_key)
                            .focus(&focus)
                            .on_activate(move |_, window, cx| lift(window, cx)),
                        row,
                    )
                    .into_any_element()
            }
            None => row.into_any_element(),
        }
    }
}
