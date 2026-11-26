//! NavHeadingMenu and NavHeadingMenuItem, upstream `NavMenu/` (component spec
//! `NavHeadingMenu.spec.md`): the menu a SideNavHeading or TopNavHeading opens. One
//! `menu` of `menuitem` rows with the APG keyboard model (arrows that wrap, Home,
//! End, typeahead, Enter and Space), disabled rows that stay reachable but inert, and a
//! press that slides across the rows and acts on the one it lifts on.

use std::cell::RefCell;
use std::rc::Rc;

use astryx_primitives::typeahead::Typeahead;
use astryx_primitives::{is_focus_visible, provide, use_context, use_link_handler};
use astryx_theme::{ThemeStyled, current};
use gpui::{
    AnyElement, App, Bounds, ElementId, FocusHandle, IntoElement, KeyDownEvent, Length,
    MouseButton, MouseUpEvent, ParentElement, Pixels, RenderOnce, Role, SharedString,
    StyleRefinement, Styled, Window, canvas, div, prelude::*, px,
};

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
    /// A press on an enabled row is held.
    pressing: bool,
    /// The rows' places this frame, for a lift's hit test.
    bounds: Vec<Bounds<Pixels>>,
}

/// What a menu shares with its rows.
#[derive(Clone)]
struct MenuScope {
    state: Rc<RefCell<MenuState>>,
    size: NavHeadingMenuSize,
    close: Option<Action>,
}

impl MenuScope {
    fn register(&self, row: Row) {
        let mut s = self.state.borrow_mut();
        if s.fresh {
            s.rows.clear();
            s.fresh = false;
        }
        s.rows.push(row);
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
        {
            let mut s = state.borrow_mut();
            s.fresh = true;
            s.bounds.clear();
        }
        let close = use_context::<NavHeadingClose>(cx).map(|c| c.0);
        let dismiss = use_context::<NavHeadingDismiss>(cx).map(|d| d.0);
        let hosted = use_context::<NavHeadingMenuHost>(cx).is_some();
        let scope = MenuScope {
            state: state.clone(),
            size: self.size,
            close: close.clone(),
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
        // FR8: a mouse released off the rows, after a press on one, dismisses.
        if state.borrow().pressing {
            let (state, dismiss) = (state.clone(), close.or(dismiss));
            menu = menu.child(
                canvas(
                    |_, _, _| {},
                    move |_, _, window, _| {
                        let (state, dismiss) = (state.clone(), dismiss.clone());
                        window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
                            if phase != gpui::DispatchPhase::Capture
                                || event.button != MouseButton::Left
                            {
                                return;
                            }
                            // A lift over a row is that row's (its own handler, in the
                            // bubble phase); a lift anywhere else ends the press and
                            // dismisses.
                            let hit = state
                                .borrow()
                                .bounds
                                .iter()
                                .any(|b| b.contains(&event.position));
                            if !hit {
                                state.borrow_mut().pressing = false;
                                if let Some(dismiss) = &dismiss {
                                    dismiss(window, cx);
                                }
                                window.refresh();
                            }
                        });
                    },
                )
                .absolute()
                .size_0(),
            );
        }
        menu.child(provide(
            scope,
            div()
                .flex()
                .flex_col()
                .gap(t.spacing.s0_5)
                .children(self.children),
        ))
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
            style: StyleRefinement::default(),
        }
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
                scope.as_ref().and_then(|s| s.close.clone()),
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
        if let Some(scope) = &scope {
            scope.register(Row {
                focus: focus.clone(),
                label: self.label.clone(),
                disabled: self.disabled,
                activate: activate.clone(),
            });
        }
        let (pad_y, pad_x) = match size {
            NavHeadingMenuSize::Sm => (t.spacing.s1, t.spacing.s2),
            NavHeadingMenuSize::Md => (t.spacing.s2, t.spacing.s2),
            NavHeadingMenuSize::Lg => (t.spacing.s3, t.spacing.s3),
        };
        let focused = focus.is_focused(window);
        let hover = t.color.overlay_hover;
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
        // FR8: a press on an enabled row begins the slide; the row under the lift acts.
        if let Some(scope) = &scope {
            let (press_state, lift_state, disabled) =
                (scope.state.clone(), scope.state.clone(), self.disabled);
            let lift = activate.clone();
            let press_focus = focus.clone();
            row = row
                .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                    if disabled {
                        // A disabled row keeps focus where it is.
                        window.prevent_default();
                        cx.stop_propagation();
                        return;
                    }
                    // As a browser focuses a tabindex=-1 row it is pressed on.
                    window.focus(&press_focus, cx);
                    press_state.borrow_mut().pressing = true;
                    window.refresh();
                })
                .on_mouse_up(MouseButton::Left, move |_, window, cx| {
                    // Only a press that began on a row acts (not the one that
                    // opened the menu).
                    let pressing = std::mem::replace(&mut lift_state.borrow_mut().pressing, false);
                    if pressing && !disabled {
                        cx.stop_propagation();
                        lift(window, cx);
                    }
                });
        } else if !self.disabled {
            let lift = activate.clone();
            row = row.on_click(move |_, window, cx| lift(window, cx));
        }
        // The row's place, for a lift's hit test.
        if let Some(scope) = &scope {
            let state = scope.state.clone();
            row = row.child(
                canvas(
                    move |bounds, _, _| state.borrow_mut().bounds.push(bounds),
                    |_, _, _, _| {},
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            );
        }
        let label = match self.custom_label {
            Some(custom) => custom,
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
        let mut row = row.children(self.icon).child(content);
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
        row
    }
}
