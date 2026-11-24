//! Interaction modality and focus visibility, upstream
//! `utils/interactionModality.ts` and the `:focus-visible` rule behind
//! `focusOutlineStyles`.
//!
//! GPUI tracks the last input's modality itself
//! (`Window::last_input_was_keyboard`, used by its `focus_visible` styles),
//! so these read it. A focus ring shows for keyboard focus, not for a click.

use gpui::{App, FocusHandle, Global, Window};

/// How the user last interacted, upstream `InteractionModality`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InteractionModality {
    /// A key press, including Tab navigation.
    Keyboard,
    /// A mouse, pen or trackpad.
    Pointer,
    /// A touch screen. Upstream folds it into `pointer`; it is kept apart
    /// because touch drops hover and press scale (`@media (pointer: coarse)`).
    Touch,
}

/// The modality of the last input event in `window`.
pub fn interaction_modality(window: &Window) -> InteractionModality {
    if window.last_input_was_keyboard() {
        InteractionModality::Keyboard
    } else if window.last_input_was_touch() {
        InteractionModality::Touch
    } else {
        InteractionModality::Pointer
    }
}

/// Whether `handle` has focus that should show a ring: CSS `:focus-visible`.
pub fn is_focus_visible(handle: &FocusHandle, window: &Window) -> bool {
    handle.is_focused(window) && window.last_input_was_keyboard()
}

/// Whether focus that should show a ring is on `handle` or inside it: CSS
/// `:has(:focus-visible)`, upstream `focusOutlineStyles.focusWithin`.
pub fn is_focus_within_visible(handle: &FocusHandle, window: &Window, cx: &App) -> bool {
    handle.contains_focused(window, cx) && window.last_input_was_keyboard()
}

/// The key context identifier text inputs add to their `key_context`.
/// Global shortcuts ([`crate::hotkeys`]) skip key events typed into one,
/// as upstream skips `input`, `textarea`, `select` and `contenteditable`.
pub const TYPING_TARGET: &str = "AstryxTypingTarget";

/// Whether the focused element takes typed text.
pub fn is_typing_target_focused(window: &Window) -> bool {
    window
        .context_stack()
        .iter()
        .any(|context| context.contains(TYPING_TARGET))
}

#[derive(Default)]
struct MotionPreference {
    reduced: bool,
}

impl Global for MotionPreference {}

/// Asks components to drop motion: press scale, entrance transforms and
/// non-essential animation, as `prefers-reduced-motion: reduce` does
/// upstream. Apps set it from the platform's setting or their own.
pub fn set_reduced_motion(reduced: bool, cx: &mut App) {
    cx.set_global(MotionPreference { reduced });
    cx.refresh_windows();
}

/// Whether motion should be reduced.
pub fn prefers_reduced_motion(cx: &App) -> bool {
    cx.try_global::<MotionPreference>()
        .is_some_and(|preference| preference.reduced)
}
