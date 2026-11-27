//! Light dismissal for a CommentThread whose host passes `on_dismiss` (upstream lab
//! `Comments/useCommentThreadDismissal.ts`). A press outside the thread and an
//! Escape no nested layer claimed both request dismissal; while the thread holds an
//! unsaved draft, the first request shakes the thread, returns focus to the draft,
//! and announces instead, and the next consecutive one dismisses.
//!
//! The thread joins the window's layer stack in flow
//! ([`astryx_primitives::layer_dismissal`]), deeper than its host, and its content
//! nests a layer so layers it opens sit deeper still. An Escape goes to the top-most
//! layer only, and an outside press counts only while the thread is that layer: with
//! the emoji picker open, the picker's own light dismiss takes the press. The press
//! acts on its release, as native light dismiss does.

use std::time::Duration;

use web_time::Instant;

use astryx_primitives::motion::cubic_bezier;
use astryx_primitives::{LayerDismissalHandle, Politeness, announce, prefers_reduced_motion};
use astryx_theme::authoring::js::js_trim;
use gpui::prelude::*;
use gpui::{
    App, DispatchPhase, Entity, MouseButton, MouseDownEvent, MouseUpEvent, Pixels, Window, canvas,
    px,
};

use super::context::{ControllerState, Scope};
use super::types::ActionFn;

/// Upstream `UnsavedDraft`: a reply or edit holding text the viewer would lose.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum UnsavedDraft {
    Reply {
        parent_id: String,
        signature: String,
    },
    Edit {
        comment_id: String,
        signature: String,
    },
}

impl UnsavedDraft {
    fn signature(&self) -> &str {
        match self {
            Self::Reply { signature, .. } | Self::Edit { signature, .. } => signature,
        }
    }
}

/// Upstream `unsavedDraftOf`: a reply holding more than its seeded mention, or an
/// edit whose text differs from the saved body. Whitespace alone is never a draft.
pub(crate) fn unsaved_draft_of(state: &ControllerState) -> Option<UnsavedDraft> {
    if let Some(edit) = &state.edit_target
        && js_trim(&edit.value) != js_trim(&edit.comment.body)
    {
        return Some(UnsavedDraft::Edit {
            comment_id: edit.comment.id.to_string(),
            signature: format!("edit:{}:{}", edit.comment.id, edit.value),
        });
    }
    if let Some(reply) = &state.reply_target {
        let typed = js_trim(&state.reply_draft);
        if !typed.is_empty() && typed != js_trim(reply.seed.as_deref().unwrap_or("")) {
            return Some(UnsavedDraft::Reply {
                parent_id: reply.parent_id.to_string(),
                signature: format!("reply:{}:{}", reply.parent_id, state.reply_draft),
            });
        }
    }
    None
}

/// Upstream `RequestOutcome`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RequestOutcome {
    Dismissed,
    Guarded,
}

/// What the dismissal keeps across frames.
#[derive(Default)]
pub(crate) struct DismissalState {
    /// The draft the last request guarded. The next request dismisses only while the
    /// draft still matches it; an edit or a press inside clears it.
    pub guarded: Option<String>,
    /// The running shake: when it started and how long it lasts.
    pub shake: Option<(Instant, Duration)>,
    /// A primary press outside that will request dismissal on its release, and
    /// whether it landed on the host's trigger.
    pub pending: Option<bool>,
}

/// The shake's keyframes, symmetric about the resting position, so it reads the same
/// in RTL.
const SHAKE: [f32; 6] = [0., -1., 1., -0.5, 0.5, 0.];

/// The shake's offset at `elapsed` of `duration`: CSS `ease-in-out` over the
/// whole run, then linear between evenly spaced keyframes (the Web Animations
/// model), times `distance`. `None` once it has finished.
pub(crate) fn shake_offset(
    elapsed: Duration,
    duration: Duration,
    distance: Pixels,
) -> Option<Pixels> {
    if duration.is_zero() || elapsed >= duration {
        return None;
    }
    let p = cubic_bezier([0.42, 0., 0.58, 1.]).ease(elapsed.as_secs_f64() / duration.as_secs_f64())
        as f32;
    let at = (p * (SHAKE.len() - 1) as f32).clamp(0., (SHAKE.len() - 1) as f32);
    let i = (at.floor() as usize).min(SHAKE.len() - 2);
    let local = at - i as f32;
    Some(px(
        f32::from(distance) * (SHAKE[i] + (SHAKE[i + 1] - SHAKE[i]) * local)
    ))
}

/// Upstream `request`: dismisses, unless an unsaved draft the last request did not
/// guard turns it into a shake, a focus move to the draft, and a polite announcement.
pub(crate) fn request(
    scope: &Scope,
    state: &Entity<DismissalState>,
    on_dismiss: &ActionFn,
    window: &mut Window,
    cx: &mut App,
) -> RequestOutcome {
    let draft = unsaved_draft_of(scope.read(cx));
    let guarded = state.read(cx).guarded.clone();
    let Some(draft) = draft.filter(|d| guarded.as_deref() != Some(d.signature())) else {
        state.update(cx, |s, _| s.guarded = None);
        on_dismiss(window, cx);
        return RequestOutcome::Dismissed;
    };
    let theme = astryx_theme::current(window, cx);
    let duration = theme.tokens().duration.medium;
    let reduced = prefers_reduced_motion(cx);
    let now = cx.background_executor().now();
    state.update(cx, |s, cx| {
        s.guarded = Some(draft.signature().to_string());
        // Motion is progressive enhancement; focus and the announcement remain.
        s.shake = (!duration.is_zero() && !reduced).then_some((now, duration));
        cx.notify();
    });
    focus_draft_input(scope, &draft, window, cx);
    announce(
        astryx_i18n::current(cx).t("@astryx.comments.unsavedDraft"),
        Politeness::Polite,
        window,
        cx,
    );
    window.refresh();
    RequestOutcome::Guarded
}

/// Upstream `focusDraftInput`: focus the draft's input; the input keeps the viewer's
/// caret. A replaced composer has no input of ours, so the thread takes focus.
fn focus_draft_input(scope: &Scope, draft: &UnsavedDraft, window: &mut Window, cx: &mut App) {
    if scope.props.render_composer.is_some() {
        window.focus(&scope.root_focus, cx);
        return;
    }
    scope.state.update(cx, |s, cx| {
        match draft {
            UnsavedDraft::Reply { .. } => {
                s.reply_focus_requests += 1;
                let n = s.reply_focus_requests;
                if let Some(target) = &mut s.reply_target {
                    target.focus_request = Some(n);
                }
            }
            UnsavedDraft::Edit { .. } => s.edit_focus_requests += 1,
        }
        cx.notify();
    });
}

/// Upstream `onRootPointerDownCapture`: a press inside re-arms the guard.
fn rearm(state: &Entity<DismissalState>, cx: &mut App) {
    state.update(cx, |s, _| s.guarded = None);
}

/// Upstream `wouldGuard`: whether a request now would be guarded.
fn would_guard(scope: &Scope, state: &Entity<DismissalState>, cx: &App) -> bool {
    let guarded = state.read(cx).guarded.clone();
    unsaved_draft_of(scope.read(cx)).is_some_and(|d| guarded.as_deref() != Some(d.signature()))
}

/// The thread's watch on presses (upstream's document listeners), drawn as an
/// invisible layer over the thread root so it knows the root's box: a press inside
/// re-arms the guard; a primary press outside the thread, its host's surface and the
/// layers it opened requests dismissal on its release, while the thread is the
/// top-most layer.
pub(crate) fn press_watch(
    scope: Scope,
    state: Entity<DismissalState>,
    on_dismiss: ActionFn,
    handle: LayerDismissalHandle,
) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            // Where the thread shows: a scroller's clip cuts it down.
            let visible = bounds.intersect(&window.content_mask().bounds);
            let (down_scope, down_state, down_handle) =
                (scope.clone(), state.clone(), handle.clone());
            window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
                if phase != DispatchPhase::Capture {
                    return;
                }
                down_state.update(cx, |s, _| s.pending = None);
                if event.button != MouseButton::Left {
                    return;
                }
                let point = event.position;
                // Presses inside, and in the layers the thread opened (its picker, its
                // menus, its dialogs), are inside.
                if visible.contains(&point) || down_handle.nested_contains(point, window, cx) {
                    rearm(&down_state, cx);
                    return;
                }
                // So are presses on the host surface around the thread.
                if down_handle.host_contains(point) {
                    return;
                }
                // A layer above the thread takes this press through its own light
                // dismiss.
                if !down_handle.is_topmost(window, cx) {
                    return;
                }
                let trigger = down_handle.host_trigger_contains(point);
                down_state.update(cx, |s, _| s.pending = Some(trigger));
                // A guarded press keeps focus and selection where they are.
                if would_guard(&down_scope, &down_state, cx) {
                    cx.stop_propagation();
                }
            });
            let (scope, state, on_dismiss) = (scope.clone(), state.clone(), on_dismiss.clone());
            window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
                if phase != DispatchPhase::Capture || event.button != MouseButton::Left {
                    return;
                }
                let Some(trigger) = state.update(cx, |s, _| s.pending.take()) else {
                    return;
                };
                if request(&scope, &state, &on_dismiss, window, cx) == RequestOutcome::Guarded {
                    // The press was spent on the warning; it activates nothing.
                    cx.stop_propagation();
                } else if trigger {
                    // The host's trigger would reopen the surface this press just closed.
                    cx.stop_propagation();
                }
            });
        },
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full()
}
