//! What Dialog, Drawer and BottomSheet share: the state of a controlled
//! modal surface between frames, its open and close transitions with their
//! focus moves, its motion phase, the backdrop, and the layer it draws in.

use std::time::Duration;

use astryx_primitives::{EscapeBehavior, layer};
use gpui::{
    AnyElement, App, Div, ElementId, Entity, FocusHandle, Hsla, IntoElement, MouseButton,
    ParentElement, Pixels, Point, SharedString, Stateful, Styled, Window, div, point, prelude::*,
    px,
};
use web_time::Instant;

use super::dialog::{DialogPurpose, FocusScope};

/// A press or close handler shared by a backdrop and a layer.
pub(crate) type CloseFn = std::rc::Rc<dyn Fn(&mut Window, &mut App)>;

/// A modal surface's state between frames.
pub(crate) struct ModalState {
    pub open: bool,
    /// What had focus when it opened, to return focus to.
    pub return_to: Option<FocusHandle>,
    /// The surface: the focus region, and the last resort for initial focus.
    pub surface: FocusHandle,
    pub shown_at: Option<Instant>,
    /// When it last closed, for its exit.
    pub closed_at: Option<Instant>,
    /// Where the entrance comes from: toward the opener.
    pub direction: Point<Pixels>,
    pub pending_focus: bool,
    pub focus: FocusScope,
    /// The header's title, the surface's name, as of last frame.
    pub title: Option<SharedString>,
    /// A drag in progress (BottomSheet): where it started and how far it is.
    pub drag: Option<(Pixels, Pixels)>,
    /// The height a dragged sheet settled at.
    pub rest: Option<Pixels>,
}

pub(crate) fn modal_state(id: &ElementId, window: &mut Window, cx: &mut App) -> Entity<ModalState> {
    window.use_keyed_state(
        ElementId::Name(format!("{id}-modal").into()),
        cx,
        |_, cx| ModalState {
            open: false,
            return_to: None,
            surface: cx.focus_handle().tab_stop(false),
            shown_at: None,
            closed_at: None,
            direction: point(px(0.), px(16.)),
            pending_focus: false,
            focus: FocusScope::default(),
            title: None,
            drag: None,
            rest: None,
        },
    )
}

/// Follows the controlled `open`: on opening, remembers what had focus and
/// asks for focus to enter; on closing, returns focus that would be lost.
pub(crate) fn sync_open(state: &Entity<ModalState>, open: bool, window: &mut Window, cx: &mut App) {
    if state.read(cx).open == open {
        return;
    }
    let now = cx.background_executor().now();
    if open {
        let focused = window.focused(cx);
        let viewport = window.viewport_size();
        // The entrance comes from the opener's side: toward the pointer
        // when a press opened it.
        let direction = if window.last_input_was_keyboard() {
            point(px(0.), px(16.))
        } else {
            let mouse = window.mouse_position();
            let dx = f32::from(mouse.x - viewport.width / 2.);
            let dy = f32::from(mouse.y - viewport.height / 2.);
            let dist = (dx * dx + dy * dy).sqrt().max(1.);
            point(px(dx / dist * 16.), px(dy / dist * 16.))
        };
        state.update(cx, |s, _| {
            s.open = true;
            s.return_to = focused;
            s.shown_at = None;
            s.closed_at = None;
            s.direction = direction;
            s.pending_focus = true;
            s.focus = FocusScope::default();
            s.drag = None;
            s.rest = None;
        });
    } else {
        let (surface, return_to) = state.update(cx, |s, _| {
            s.open = false;
            s.pending_focus = false;
            s.closed_at = Some(now);
            s.drag = None;
            (s.surface.clone(), s.return_to.take())
        });
        let lost = window.focused(cx).is_none() || surface.contains_focused(window, cx);
        if lost && let Some(target) = return_to {
            window.defer(cx, move |window, cx| target.focus(window, cx));
        }
    }
}

/// Where a surface is in its motion.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Phase {
    /// Not drawn.
    Hidden,
    /// Shown; `progress` runs 0 to 1 over the entrance.
    Open { progress: f32 },
    /// Closing; `progress` runs 0 to 1 over the exit.
    Closing { progress: f32 },
}

/// The phase now, asking for frames while it moves. `exit` zero closes at
/// once.
pub(crate) fn phase(
    state: &Entity<ModalState>,
    enter: Duration,
    exit: Duration,
    window: &mut Window,
    cx: &mut App,
) -> Phase {
    let now = cx.background_executor().now();
    let (open, since, closed_at) = state.update(cx, |s, _| {
        let since = s.open.then(|| *s.shown_at.get_or_insert(now));
        (s.open, since, s.closed_at)
    });
    let fraction = |from: Instant, over: Duration| {
        if over.is_zero() {
            1.
        } else {
            (now.saturating_duration_since(from).as_secs_f32() / over.as_secs_f32()).min(1.)
        }
    };
    if open {
        let progress = fraction(since.unwrap_or(now), enter);
        if progress < 1. {
            window.request_animation_frame();
        }
        return Phase::Open { progress };
    }
    match closed_at {
        Some(at) if !exit.is_zero() => {
            let progress = fraction(at, exit);
            if progress < 1. {
                window.request_animation_frame();
                Phase::Closing { progress }
            } else {
                Phase::Hidden
            }
        }
        _ => Phase::Hidden,
    }
}

/// Moves focus in, right after the frame that first drew the surface: a
/// control that focused itself keeps it, else an [`super::AutoFocus`]
/// request, else the header's title, else the first control, else the
/// surface.
pub(crate) fn enter_focus(state: &Entity<ModalState>, window: &mut Window, cx: &mut App) {
    let Some((surface, scope)) = state.update(cx, |s, _| {
        std::mem::take(&mut s.pending_focus).then(|| (s.surface.clone(), s.focus.clone()))
    }) else {
        return;
    };
    if surface.contains_focused(window, cx) && window.focused(cx).as_ref() != Some(&surface) {
        return;
    }
    if let Some(requested) = scope.requested() {
        requested.focus(window, cx);
        return;
    }
    if let Some(title) = scope.title_focus() {
        title.focus(window, cx);
        return;
    }
    surface.focus(window, cx);
    window.focus_next(cx);
    if window.focused(cx).is_none() {
        surface.focus(window, cx);
    }
}

/// The probes a surface carries: one that moves focus in after the frame
/// that first draws it, and one that keeps the header's title as its name.
pub(crate) fn probes(state: &Entity<ModalState>, cx: &App) -> Vec<AnyElement> {
    let mut out = Vec::new();
    if state.read(cx).pending_focus {
        let state = state.clone();
        out.push(
            gpui::canvas(
                |_, _, _| {},
                move |_, _, window, cx| {
                    let state = state.clone();
                    window.defer(cx, move |window, cx| enter_focus(&state, window, cx));
                },
            )
            .absolute()
            .size_0()
            .into_any_element(),
        );
    }
    let state = state.clone();
    out.push(
        gpui::canvas(
            move |_, _, cx| {
                let title = state.read(cx).focus.title_text();
                state.update(cx, |s, _| s.title = title);
            },
            |_, _, _, _| {},
        )
        .absolute()
        .size_0()
        .into_any_element(),
    );
    out
}

/// A full-window backdrop: dims the page with `dim`, takes its presses
/// (calling `on_press`), and holds the surface.
pub(crate) fn backdrop(
    id: &ElementId,
    dim: Option<Hsla>,
    on_press: Option<CloseFn>,
    window: &Window,
) -> Stateful<Div> {
    let viewport = window.viewport_size();
    let mut el = div()
        .id(ElementId::Name(format!("{id}-backdrop").into()))
        .absolute()
        .top_0()
        .left_0()
        .w(viewport.width)
        .h(viewport.height)
        .occlude();
    if let Some(dim) = dim {
        el = el.bg(dim);
    }
    if let Some(on_press) = on_press {
        el = el.on_mouse_down(MouseButton::Left, move |_, window, cx| on_press(window, cx));
    }
    el
}

/// Draws `content` as a layer anchored to the window's corner: modal (above
/// every layer opened before it) or not, closing per `purpose` on Escape.
pub(crate) fn modal_layer(
    id: &ElementId,
    content: impl IntoElement,
    modal: bool,
    purpose: DialogPurpose,
    close: impl Fn(&mut Window, &mut App) + 'static,
) -> AnyElement {
    let mut layer = layer(content, close)
        .anchor(point(px(0.), px(0.)), gpui::Anchor::TopLeft)
        .light_dismiss(false)
        .escape(if purpose == DialogPurpose::Required {
            EscapeBehavior::Block
        } else {
            EscapeBehavior::Close
        });
    if modal {
        layer = layer.modal();
    }
    div()
        .id(id.clone())
        .display_contents()
        .child(layer)
        .into_any_element()
}

/// `color` with its alpha scaled by `opacity`.
pub(crate) fn faded(color: Hsla, opacity: f32) -> Hsla {
    Hsla {
        a: color.a * opacity,
        ..color
    }
}
