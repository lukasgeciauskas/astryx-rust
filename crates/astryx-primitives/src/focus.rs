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
///
/// Inside a [`RovingScope`] (a Toolbar), the control registers with the
/// scope and is a tab stop only while it is the scope's active control.
pub fn control_focus(id: &gpui::ElementId, window: &mut Window, cx: &mut App) -> FocusHandle {
    let handle = window
        .use_keyed_state(
            gpui::ElementId::Name(format!("{id:?}-focus").into()),
            cx,
            |_, cx| cx.focus_handle().tab_stop(true),
        )
        .read(cx)
        .clone();
    match crate::use_context::<RovingScope>(cx) {
        Some(scope) => {
            let active = scope.register(&handle);
            handle.tab_stop(active)
        }
        None => handle,
    }
}

#[derive(Default)]
struct RovingState {
    /// The controls registered this frame, in render order.
    controls: Vec<FocusHandle>,
    /// The control that holds the tab stop.
    active: Option<FocusHandle>,
    /// Whether this frame's first registration still has to clear the
    /// last frame's list.
    fresh: bool,
}

/// One tab stop among the controls inside it, moved with the arrow keys:
/// upstream `oneTabStopAmong` with `useListFocus`'s roving tab index.
///
/// The owner calls [`RovingScope::begin`] in its render, wraps its children
/// in `provide(scope.clone(), ..)`, and puts [`RovingScope::key_down_listener`]
/// on its container. Every control built on [`control_focus`] joins.
#[derive(Clone)]
pub struct RovingScope {
    state: std::rc::Rc<std::cell::RefCell<RovingState>>,
    keys: crate::ListKeys,
}

impl RovingScope {
    /// The scope with id `id`, for this frame.
    pub fn begin(
        id: &gpui::ElementId,
        keys: crate::ListKeys,
        window: &mut Window,
        cx: &mut App,
    ) -> Self {
        let state = window
            .use_keyed_state(
                gpui::ElementId::Name(format!("{id:?}-roving").into()),
                cx,
                |_, _| std::rc::Rc::new(std::cell::RefCell::new(RovingState::default())),
            )
            .read(cx)
            .clone();
        {
            let mut s = state.borrow_mut();
            // The focused control keeps the stop; otherwise the last one.
            if let Some(focused) = s.controls.iter().find(|h| h.is_focused(window)) {
                s.active = Some(focused.clone());
            }
            s.fresh = true;
        }
        RovingScope { state, keys }
    }

    /// Registers a control; returns whether it holds the tab stop.
    fn register(&self, handle: &FocusHandle) -> bool {
        let mut s = self.state.borrow_mut();
        if s.fresh {
            s.controls.clear();
            s.fresh = false;
        }
        s.controls.push(handle.clone());
        match &s.active {
            Some(active) => active == handle,
            None => s.controls.len() == 1,
        }
    }

    /// The container's key-down listener: arrows (by the scope's keys),
    /// Home and End move focus among the controls.
    pub fn key_down_listener(
        &self,
    ) -> impl Fn(&gpui::KeyDownEvent, &mut Window, &mut App) + 'static {
        let scope = self.clone();
        move |event, window, cx| {
            let m = &event.keystroke.modifiers;
            if m.control || m.alt || m.platform {
                return;
            }
            let key = event.keystroke.key.as_str();
            if !scope.keys.handles(key) {
                return;
            }
            let controls = scope.state.borrow().controls.clone();
            let current = controls.iter().position(|h| h.is_focused(window));
            if let Some(target) = scope.keys.target(key, current, controls.len(), |_| false) {
                let handle = controls[target].clone();
                scope.state.borrow_mut().active = Some(handle.clone());
                handle.focus(window, cx);
            }
            cx.stop_propagation();
        }
    }
}
