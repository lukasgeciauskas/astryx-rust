//! Interaction modality and focus visibility, upstream
//! `utils/interactionModality.ts` and the `:focus-visible` rule behind
//! `focusOutlineStyles`.
//!
//! GPUI tracks the last input's modality itself
//! (`Window::last_input_was_keyboard`, used by its `focus_visible` styles),
//! so these read it. A focus ring shows for keyboard focus, not for a click.

use std::collections::HashMap;

use gpui::{App, FocusHandle, Global, Window, WindowId};

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

/// Each window's last pointer: whether it was a touch screen.
#[derive(Default)]
struct LastPointer(HashMap<WindowId, bool>);

impl Global for LastPointer {}

/// Whether the primary pointer is coarse, upstream `(pointer: coarse)` and
/// its input tracking: the pointer the window's gesture host saw last is a
/// finger (the host records every contact it hears: a finger's touch, a
/// real mouse's press or move, never GPUI's taps synthesized from a touch);
/// without a host's reading, the last pointer input was a finger. Before any
/// pointer input the answer is the platform's: on the web the browser's own
/// `(pointer: coarse)` media query, natively fine. A key press is no
/// pointer, so typing or tabbing keeps the answer (a tablet with a keyboard
/// stays coarse).
pub fn coarse_primary_pointer(window: &Window, cx: &mut App) -> bool {
    use crate::gestures::PointerKind;

    let id = window.window_handle().window_id();
    let stored = cx
        .try_global::<LastPointer>()
        .and_then(|last| last.0.get(&id).copied());
    if window.last_input_was_keyboard() {
        if let Some(kind) = crate::gestures::host::last_pointer_kind(id, cx) {
            return kind == PointerKind::Touch;
        }
        return stored.unwrap_or_else(platform_coarse_pointer);
    }
    let coarse = if window.last_input_was_touch() {
        true
    } else if let Some(kind) = crate::gestures::host::last_pointer_kind(id, cx) {
        kind == PointerKind::Touch
    } else if stored.is_none() {
        // No pointer input the window could tell from its first state.
        return platform_coarse_pointer();
    } else {
        false
    };
    cx.default_global::<LastPointer>().0.insert(id, coarse);
    coarse
}

/// The platform's primary pointer before any input: the browser's
/// `(pointer: coarse)` on the web; fine natively.
fn platform_coarse_pointer() -> bool {
    #[cfg(target_family = "wasm")]
    {
        web_sys::window()
            .and_then(|window| window.match_media("(pointer: coarse)").ok().flatten())
            .is_some_and(|list| list.matches())
    }
    #[cfg(not(target_family = "wasm"))]
    {
        false
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
