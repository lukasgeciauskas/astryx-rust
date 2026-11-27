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
///
/// An app that knows its device (a phone or a tablet build, or a test) declares the
/// primary pointer with [`set_primary_pointer_coarse`], which wins over every reading.
pub fn coarse_primary_pointer(window: &Window, cx: &mut App) -> bool {
    let (coarse, store) = read_primary_pointer(window, cx);
    if let Some(store) = store {
        let id = window.window_handle().window_id();
        cx.default_global::<LastPointer>().0.insert(id, store);
        if store {
            cx.default_global::<SeenTouch>().0.insert(id);
        }
    }
    coarse
}

/// The windows that have seen a finger.
#[derive(Default)]
struct SeenTouch(std::collections::HashSet<WindowId>);

impl Global for SeenTouch {}

/// Whether any pointer the device has is coarse, upstream `(any-pointer: coarse)`: a
/// touch screen is present even where the primary pointer is a mouse or a trackpad (a
/// touchscreen laptop). On the web the browser's own query answers; natively a window
/// that has seen a finger counts, and on Windows a digitizer that reports touch
/// (`GetSystemMetrics(SM_DIGITIZER)` with `NID_INTEGRATED_TOUCH` or
/// `NID_EXTERNAL_TOUCH`). A declaration ([`set_primary_pointer_coarse`]) answers both
/// queries.
pub fn any_pointer_coarse(window: &Window, cx: &App) -> bool {
    use crate::gestures::PointerKind;

    if let Some(declared) = declared_primary_pointer(cx) {
        return declared;
    }
    if read_primary_pointer(window, cx).0 {
        return true;
    }
    let id = window.window_handle().window_id();
    if cx
        .try_global::<SeenTouch>()
        .is_some_and(|seen| seen.0.contains(&id))
        || crate::gestures::host::last_pointer_kind(id, cx) == Some(PointerKind::Touch)
    {
        return true;
    }
    platform_any_coarse_pointer()
}

/// The platform's `(any-pointer: coarse)`.
fn platform_any_coarse_pointer() -> bool {
    #[cfg(target_family = "wasm")]
    {
        web_sys::window()
            .and_then(|window| window.match_media("(any-pointer: coarse)").ok().flatten())
            .is_some_and(|list| list.matches())
    }
    #[cfg(windows)]
    {
        use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_DIGITIZER};
        /// `NID_INTEGRATED_TOUCH | NID_EXTERNAL_TOUCH` (winuser.h).
        const TOUCH_DIGITIZER: i32 = 0x01 | 0x02;
        // SAFETY: GetSystemMetrics reads a system value and has no preconditions.
        unsafe { GetSystemMetrics(SM_DIGITIZER) & TOUCH_DIGITIZER != 0 }
    }
    #[cfg(not(any(target_family = "wasm", windows)))]
    {
        false
    }
}

/// [`coarse_primary_pointer`] for a reader that holds only `&App` (the theme's
/// pointer adaptations): the same answer, without recording the window's reading.
pub fn coarse_primary_pointer_now(window: &Window, cx: &App) -> bool {
    read_primary_pointer(window, cx).0
}

/// The answer, and the reading to record for the window when the last input was a
/// pointer.
fn read_primary_pointer(window: &Window, cx: &App) -> (bool, Option<bool>) {
    use crate::gestures::PointerKind;

    if let Some(declared) = declared_primary_pointer(cx) {
        return (declared, None);
    }
    let id = window.window_handle().window_id();
    let stored = cx
        .try_global::<LastPointer>()
        .and_then(|last| last.0.get(&id).copied());
    if window.last_input_was_keyboard() {
        if let Some(kind) = crate::gestures::host::last_pointer_kind(id, cx) {
            return (kind == PointerKind::Touch, None);
        }
        return (stored.unwrap_or_else(platform_coarse_pointer), None);
    }
    let coarse = if window.last_input_was_touch() {
        true
    } else if let Some(kind) = crate::gestures::host::last_pointer_kind(id, cx) {
        kind == PointerKind::Touch
    } else if stored.is_none() {
        // No pointer input the window could tell from its first state.
        return (platform_coarse_pointer(), None);
    } else {
        false
    };
    (coarse, Some(coarse))
}

/// The app's declaration of its device's primary pointer.
#[derive(Default)]
struct DeclaredPointer(Option<bool>);

impl Global for DeclaredPointer {}

/// Declares the device's primary pointer coarse (`Some(true)`: a phone or a tablet) or
/// fine (`Some(false)`), the counterpart of the browser's `(pointer: coarse)` for a
/// platform that cannot tell; `None` leaves it to the platform and the window's last
/// pointer. Every touch presentation follows it: the adaptive sheets, the menus' touch
/// model, the press controller, Reduce Motion's cross-fades and the theme's pointer
/// adaptations. Tests use it to present the touch forms.
pub fn set_primary_pointer_coarse(coarse: Option<bool>, cx: &mut App) {
    cx.set_global(DeclaredPointer(coarse));
    cx.refresh_windows();
}

/// What [`set_primary_pointer_coarse`] declared, if anything.
pub fn declared_primary_pointer(cx: &App) -> Option<bool> {
    cx.try_global::<DeclaredPointer>().and_then(|d| d.0)
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
