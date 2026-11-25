//! The contract between a popup and its trigger: upstream clones the
//! trigger button to add `aria-expanded`, `aria-haspopup` and a click
//! handler. Here the popup provides a [`Disclosure`] around its trigger, and
//! Button (and everything built on it) reads it: it reports the popup's state
//! and toggles it after its own click handler.

use std::rc::Rc;

use astryx_primitives::use_context;
use gpui::{App, HasPopup, Window};

type ToggleFn = Rc<dyn Fn(&mut Window, &mut App)>;

/// What a popup tells the button that opens it.
#[derive(Clone)]
pub(crate) struct Disclosure {
    /// Whether the popup is open (`aria-expanded`).
    pub expanded: bool,
    /// What kind of popup it is (`aria-haspopup`).
    pub popup: HasPopup,
    /// Opens or closes it; `None` while the popup ignores its trigger.
    pub toggle: Option<ToggleFn>,
}

/// The popup a button opens, if it is one's trigger.
pub(crate) fn disclosure(cx: &App) -> Option<Disclosure> {
    use_context::<Disclosure>(cx)
}
