//! The contract between a popup and its trigger: upstream finds the button its
//! children hold and gives it `aria-expanded`, `aria-haspopup` and a click
//! handler (or hands a render-prop trigger those props). Here the popup provides a
//! [`Disclosure`] around its trigger. Button (and everything built on it) reads it: it
//! reports the popup's state and toggles it after its own click handler. Any other
//! element reads it with [`disclosure`] as it renders and wires itself with
//! [`Disclosure::trigger`]. A menu also hears the trigger's presses, hover and keys
//! (its press model and arrow keys).

use std::rc::Rc;

use astryx_primitives::use_context;
use gpui::prelude::*;
use gpui::{AnyElement, App, Div, HasPopup, KeyDownEvent, Stateful, Window};

use crate::gestures::menu_press::MenuPress;

type ToggleFn = Rc<dyn Fn(&mut Window, &mut App)>;
type HoverFn = Rc<dyn Fn(bool, &mut Window, &mut App)>;
/// Returns whether it handled the key.
type KeyFn = Rc<dyn Fn(&KeyDownEvent, &mut Window, &mut App) -> bool>;

/// What a popup tells the element that opens it (upstream `PopoverTriggerRenderProps`:
/// the trigger's `aria-haspopup`, `aria-expanded` and `onClick`). Read it with
/// [`disclosure`] while the trigger renders.
#[derive(Clone)]
pub struct Disclosure {
    /// Whether the popup is open (`aria-expanded`).
    pub(crate) expanded: bool,
    /// What kind of popup it is (`aria-haspopup`).
    pub(crate) popup: HasPopup,
    /// Opens or closes it; `None` while the popup ignores its trigger.
    pub(crate) toggle: Option<ToggleFn>,
    /// A menu's press model: the trigger is its trigger (upstream `triggerProps`).
    pub(crate) menu_press: Option<MenuPress>,
    /// The pointer entered (`true`) or left an enabled trigger.
    pub(crate) on_hover: Option<HoverFn>,
    /// A key down on an enabled, focused trigger.
    pub(crate) on_key: Option<KeyFn>,
    /// The trigger shows the pressed tint while the popup is open
    /// (upstream DropdownMenu's `triggerOpen`).
    pub(crate) open_tint: bool,
    /// Hides an enclosing trigger's disclosure from the controls inside: a
    /// field's clear button is no popup trigger.
    pub(crate) detached: bool,
}

impl Disclosure {
    /// A popup that only reports its state and toggles.
    pub(crate) fn new(expanded: bool, popup: HasPopup, toggle: Option<ToggleFn>) -> Self {
        Disclosure {
            expanded,
            popup,
            toggle,
            menu_press: None,
            on_hover: None,
            on_key: None,
            open_tint: false,
            detached: false,
        }
    }

    /// No disclosure: the controls inside are not the popup's trigger.
    pub(crate) fn detached() -> Self {
        Disclosure {
            detached: true,
            ..Self::new(false, HasPopup::Dialog, None)
        }
    }

    /// Whether the popup is open (upstream `aria-expanded`).
    pub fn is_expanded(&self) -> bool {
        self.expanded
    }

    /// What kind of popup it is (upstream `aria-haspopup`).
    pub fn has_popup(&self) -> HasPopup {
        self.popup
    }

    /// Opens the popup, or closes it while it is open (upstream `onClick`); nothing while
    /// the popup ignores its trigger.
    pub fn toggle(&self, window: &mut Window, cx: &mut App) {
        if let Some(toggle) = &self.toggle {
            toggle(window, cx);
        }
    }

    /// Wires `el` as the popup's trigger (upstream's `attachTrigger`, the props a popup
    /// gives the button its children hold): `aria-expanded` and `aria-haspopup`, a click
    /// that toggles the popup (Enter and Space too, once `el` tracks a focus handle), and
    /// for a menu its press model, hover and keys. `el` takes its own role and name.
    pub fn trigger(&self, el: Stateful<Div>) -> AnyElement {
        let mut el = el.aria_expanded(self.expanded).aria_has_popup(self.popup);
        if let Some(toggle) = self.toggle.clone() {
            el = el.on_click(move |_, window, cx| toggle(window, cx));
        }
        if let Some(hover) = self.on_hover.clone() {
            el = el.on_hover(move |hovered, window, cx| hover(*hovered, window, cx));
        }
        if let Some(key) = self.on_key.clone() {
            el = el.on_key_down(move |event, window, cx| {
                if key(event, window, cx) {
                    cx.stop_propagation();
                }
            });
        }
        match &self.menu_press {
            Some(press) => press.trigger(el).into_any_element(),
            None => el.into_any_element(),
        }
    }
}

/// The popup whose trigger the element rendering now is, if any: the popup around it
/// provides one (upstream's children-mode trigger and render-prop trigger props).
pub fn disclosure(cx: &App) -> Option<Disclosure> {
    use_context::<Disclosure>(cx).filter(|d| !d.detached)
}
