//! The contract between a popup and its trigger: upstream clones the
//! trigger button to add `aria-expanded`, `aria-haspopup` and a click
//! handler. Here the popup provides a [`Disclosure`] around its trigger, and
//! Button (and everything built on it) reads it: it reports the popup's state
//! and toggles it after its own click handler. A menu also hears the
//! trigger's press-down, hover and keys (its press model and arrow keys).

use std::rc::Rc;

use astryx_primitives::use_context;
use gpui::{App, HasPopup, KeyDownEvent, Window};

type ToggleFn = Rc<dyn Fn(&mut Window, &mut App)>;
type HoverFn = Rc<dyn Fn(bool, &mut Window, &mut App)>;
/// Returns whether it handled the key.
type KeyFn = Rc<dyn Fn(&KeyDownEvent, &mut Window, &mut App) -> bool>;

/// What a popup tells the button that opens it.
#[derive(Clone)]
pub(crate) struct Disclosure {
    /// Whether the popup is open (`aria-expanded`).
    pub expanded: bool,
    /// What kind of popup it is (`aria-haspopup`).
    pub popup: HasPopup,
    /// Opens or closes it; `None` while the popup ignores its trigger.
    pub toggle: Option<ToggleFn>,
    /// A left press-down on an enabled trigger.
    pub on_press: Option<ToggleFn>,
    /// The pointer entered (`true`) or left an enabled trigger.
    pub on_hover: Option<HoverFn>,
    /// A key down on an enabled, focused trigger.
    pub on_key: Option<KeyFn>,
    /// The trigger shows the pressed tint while the popup is open
    /// (upstream DropdownMenu's `triggerOpen`).
    pub open_tint: bool,
    /// Hides an enclosing trigger's disclosure from the controls inside: a
    /// field's clear button is no popup trigger.
    pub detached: bool,
}

impl Disclosure {
    /// A popup that only reports its state and toggles.
    pub fn new(expanded: bool, popup: HasPopup, toggle: Option<ToggleFn>) -> Self {
        Disclosure {
            expanded,
            popup,
            toggle,
            on_press: None,
            on_hover: None,
            on_key: None,
            open_tint: false,
            detached: false,
        }
    }

    /// No disclosure: the controls inside are not the popup's trigger.
    pub fn detached() -> Self {
        Disclosure {
            detached: true,
            ..Self::new(false, HasPopup::Dialog, None)
        }
    }
}

/// The popup a button opens, if it is one's trigger.
pub(crate) fn disclosure(cx: &App) -> Option<Disclosure> {
    use_context::<Disclosure>(cx).filter(|d| !d.detached)
}
