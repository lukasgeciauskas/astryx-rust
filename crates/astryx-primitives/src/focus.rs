//! Keyboard focus traversal, the browser's Tab and Shift+Tab.
//!
//! GPUI leaves Tab to the app: focus handles opt into the tab order
//! (`tab_stop`) and `Window::focus_next` moves through it, but nothing binds
//! the keys. [`init`] binds Tab and Shift+Tab to [`FocusNext`] and
//! [`FocusPrevious`] and handles them app-wide, so every Astryx app has
//! keyboard traversal. An element that wants Tab for itself handles the
//! action first (or binds Tab in its own key context).

use gpui::{App, FocusHandle, Global, KeyBinding, Window, actions};

actions!(
    astryx,
    [
        /// Moves focus to the next tab stop.
        FocusNext,
        /// Moves focus to the previous tab stop.
        FocusPrevious,
    ]
);

/// A traversal the app-wide action handlers asked for, made by the
/// keystroke observer once dispatch has finished. Action handlers only get
/// `App`, and the window is checked out while its key event dispatches, so
/// they can't move focus themselves.
#[derive(Default)]
struct PendingTraversal(Option<bool>);

impl Global for PendingTraversal {}

/// Binds Tab and Shift+Tab and moves focus in the window that received them.
pub fn init(cx: &mut App) {
    cx.set_global(PendingTraversal::default());
    cx.bind_keys([
        KeyBinding::new("tab", FocusNext, None),
        KeyBinding::new("shift-tab", FocusPrevious, None),
    ]);
    // These run last, after any element's handler: an element that handles
    // the action (a focus trap, an editor taking Tab) stops it first.
    cx.on_action(|_: &FocusNext, cx: &mut App| {
        cx.global_mut::<PendingTraversal>().0 = Some(true);
    });
    cx.on_action(|_: &FocusPrevious, cx: &mut App| {
        cx.global_mut::<PendingTraversal>().0 = Some(false);
    });
    cx.observe_keystrokes(
        |_, window, cx| match cx.global_mut::<PendingTraversal>().0.take() {
            Some(true) => window.focus_next(cx),
            Some(false) => window.focus_prev(cx),
            None => {}
        },
    )
    .detach();
}

/// The focus handle of a control with id `id`, kept between frames and in
/// the tab order. Keyed by the id, so sibling controls never share one.
pub fn control_focus(id: &gpui::ElementId, window: &mut Window, cx: &mut App) -> FocusHandle {
    window
        .use_keyed_state(
            gpui::ElementId::Name(format!("{id:?}-focus").into()),
            cx,
            |_, cx| cx.focus_handle().tab_stop(true),
        )
        .read(cx)
        .clone()
}
