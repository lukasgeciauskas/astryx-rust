//! What every row in AppShell's mobile drawer shares: upstream
//! `useCloseMobileDrawerOnActivate` (a row's activation closes the drawer) and
//! `navItemStyles` (the drawer row's look).

use std::rc::Rc;

use astryx_primitives::{ElementSize, use_context};
use astryx_theme::{ThemeStyled, Tokens};

use crate::app_shell::AppShellMobile;
use gpui::{App, Div, FontWeight, Modifiers, Stateful, Styled, Window};

type Action = Rc<dyn Fn(&mut Window, &mut App)>;

/// The AppShell drawer's close (upstream `closeMobileNav`), read while rendering a row;
/// None outside an AppShell.
pub(crate) fn drawer_close(cx: &App) -> Option<Action> {
    use_context::<AppShellMobile>(cx).map(|shell| {
        let set = shell.set_open.clone();
        Rc::new(move |window: &mut Window, cx: &mut App| set(false, window, cx)) as Action
    })
}

/// Upstream `isModifiedClick`: Ctrl, Cmd, Shift or Alt held.
pub(crate) fn is_modified(modifiers: &Modifiers) -> bool {
    modifiers.control || modifiers.platform || modifiers.shift || modifiers.alt
}

/// Upstream `useCloseMobileDrawerOnActivate`, after a row's own activation: the drawer
/// closes, except after a modified click on a link, which leaves it open for the next.
pub(crate) fn close_drawer_after(
    close: &Option<Action>,
    is_link: bool,
    modifiers: &Modifiers,
    window: &mut Window,
    cx: &mut App,
) {
    if is_link && is_modified(modifiers) {
        return;
    }
    if let Some(close) = close {
        close(window, cx);
    }
}

/// Upstream `navItemStyles`: a full-width row of the drawer, by size, selected and
/// disabled.
pub(crate) fn nav_item_row(
    el: Stateful<Div>,
    t: &Tokens,
    size: ElementSize,
    selected: bool,
    disabled: bool,
) -> Stateful<Div> {
    let (min_h, pad) = match size {
        ElementSize::Sm => (t.size.element_sm, t.spacing.s1),
        ElementSize::Md => (t.size.element_md, t.spacing.s2),
        ElementSize::Lg => (t.size.element_lg, t.spacing.s2),
    };
    let mut el = el
        .flex()
        .items_center()
        .gap(t.spacing.s2)
        .w_full()
        .min_h(min_h)
        .px(pad)
        .rounded(t.radius.element)
        .font_stack(&t.font.family.body)
        .text_style(&t.text.label)
        .font_weight(FontWeight::NORMAL)
        .text_color(t.color.text_primary);
    if selected {
        el = el.bg(t.color.neutral).font_weight(FontWeight::MEDIUM);
    }
    if disabled {
        el = el.text_color(t.color.text_disabled);
    }
    el
}
