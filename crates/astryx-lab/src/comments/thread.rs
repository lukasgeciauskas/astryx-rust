//! One comment thread on its own (upstream lab `Comments/CommentThread.tsx`): its
//! first comment, one level of replies, reactions, actions, and the reply composer.
//! For a margin comment beside a document, a popover under a highlight, or any
//! surface that shows a single thread; Comments renders many with a composer for new
//! ones. With `on_dismiss`, it owns its anchored host's light dismissal and guards an
//! unsaved draft against it.

use std::rc::Rc;

use astryx_primitives::{EscapeBehavior, layer};
use gpui::prelude::*;
use gpui::{AnyElement, App, ElementId, SharedString, StyleRefinement, Window, div};

use super::context::{ControllerOptions, provide_scope, scoped, use_controller};
use super::dismissal::{DismissalState, layer_dismiss, rearm, shake_offset};
use super::row::CommentRow;
use super::shared::CommentThreadShared;
use super::types::{
    ActionFn, CommentContainer, CommentThreadData, CommentThreadSharedProps, FlagFn,
};

/// Upstream lab `CommentThread`: one comment thread, with edit history and a reply
/// composer. Controlled: the host owns the data and every mutation; the thread owns
/// presentation, selection and focus.
///
/// ```ignore
/// CommentThread::new("thread", thread)
///     .current_user(viewer)
///     .container(CommentContainer::Card)
///     .on_submit(|body, parent, _, cx| post_reply(parent, body, cx))
/// ```
pub struct CommentThread {
    id: ElementId,
    comment: CommentThreadData,
    is_selected: Option<bool>,
    on_selected_change: Option<FlagFn>,
    on_dismiss: Option<ActionFn>,
    shared: CommentThreadSharedProps,
    style: StyleRefinement,
}

impl CommentThread {
    /// Upstream `comment`: the thread, its first comment carrying its replies.
    pub fn new(id: impl Into<ElementId>, comment: impl Into<CommentThreadData>) -> Self {
        CommentThread {
            id: id.into(),
            comment: comment.into(),
            is_selected: None,
            on_selected_change: None,
            on_dismiss: None,
            shared: CommentThreadSharedProps::default(),
            style: StyleRefinement::default(),
        }
    }

    /// Upstream `isSelected`: whether the thread is open, its body unclamped and its
    /// replies shown. Leave it unset to let the thread own selection; set it to open a
    /// thread from elsewhere, such as its highlight in a document. Host selection does
    /// not move focus or open the reply composer; a viewer Reply action does.
    pub fn selected(mut self, selected: bool) -> Self {
        self.is_selected = Some(selected);
        self
    }

    /// Upstream `onSelectedChange`: the viewer opened or closed the thread.
    pub fn on_selected_change(mut self, f: impl Fn(bool, &mut Window, &mut App) + 'static) -> Self {
        self.on_selected_change = Some(Rc::new(f));
        self
    }

    /// Upstream `onDismiss`: hands the thread its anchored host's light dismissal,
    /// such as a Popover opened from a highlight. A press outside the thread, or an
    /// Escape no layer inside it handled, calls this, except that while a reply or
    /// edit holds unsaved text the first request shakes the thread and returns focus
    /// to the draft instead; the next consecutive request dismisses. Pair it with
    /// `light_dismiss(false)` and `escape_dismiss(false)` on the host Popover. Leave it
    /// unset and the thread never dismisses anything.
    pub fn on_dismiss(mut self, f: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_dismiss = Some(Rc::new(f));
        self
    }
}

impl CommentThreadShared for CommentThread {
    fn shared_props(&mut self) -> &mut CommentThreadSharedProps {
        &mut self.shared
    }
}

impl Styled for CommentThread {
    /// The root's style (upstream `xstyle`, `className`, `style`).
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl IntoElement for CommentThread {
    type Element = AnyElement;

    fn into_element(self) -> AnyElement {
        ThreadElement(std::cell::RefCell::new(Some(self))).into_any_element()
    }
}

#[derive(IntoElement)]
struct ThreadElement(std::cell::RefCell<Option<CommentThread>>);

impl RenderOnce for ThreadElement {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let thread = self.0.borrow_mut().take().expect("rendered once");
        render_thread(thread, window, cx)
    }
}

fn render_thread(thread: CommentThread, window: &mut Window, cx: &mut App) -> AnyElement {
    let theme = astryx_theme::current(window, cx);
    let t = theme.tokens();
    let scope_id: SharedString = thread.id.to_string().into();
    let comment = thread.comment.clone();
    let cid = comment.id.clone();
    let container = thread.shared.container;
    let on_selected_change = thread.on_selected_change.clone().map(|f| {
        let cid = cid.clone();
        Rc::new(
            move |id: Option<SharedString>, w: &mut Window, cx: &mut App| {
                f(id.as_ref() == Some(&cid), w, cx)
            },
        ) as super::context::SelectionFn
    });
    let scope = use_controller(
        ControllerOptions {
            id: scope_id.clone(),
            props: thread.shared,
            selected_thread_id: thread.is_selected.map(|s| s.then(|| cid.clone())),
            on_selected_thread_change: on_selected_change,
            roots: vec![comment.clone()],
        },
        window,
        cx,
    );
    let dialogs = scope.dialogs(window, cx);
    // A lone thread on a card parent sits inside the host's border, so it draws no
    // rules of its own.
    let content = div()
        .flex()
        .flex_col()
        .w_full()
        .child(CommentRow::thread(comment, Vec::new()))
        .child(dialogs);
    let mut root = div()
        .id(scoped(&scope_id, "thread-root"))
        .track_focus(&scope.root_focus)
        .flex()
        .flex_col()
        .w_full()
        .debug_selector({
            let s = scope_id.to_string();
            move || s
        });
    theme.refine(
        root.style(),
        "comment-thread",
        &[(
            "container",
            if container == CommentContainer::Card {
                "card"
            } else {
                "section"
            },
        )],
        None,
    );
    root.style().refine(&thread.style);
    let Some(on_dismiss) = thread.on_dismiss else {
        return root.child(provide_scope(scope, content)).into_any_element();
    };
    let state = window.use_keyed_state(scoped(&scope_id, "dismissal"), cx, |_, _| {
        DismissalState::default()
    });
    // The running shake moves the thread without moving its neighbors.
    let shake = state.read(cx).shake;
    if let Some((started, duration)) = shake {
        let elapsed = cx
            .background_executor()
            .now()
            .saturating_duration_since(started);
        match shake_offset(elapsed, duration, t.spacing.s1.to_pixels(window.rem_size())) {
            Some(offset) => {
                root = root.relative().left(offset);
                window.request_animation_frame();
            }
            None => state.update(cx, |s, _| s.shake = None),
        }
    }
    // The thread's content draws in its layer, so the press inside is caught there,
    // before any control inside keeps it.
    let rearm_state = state.clone();
    let content = content.capture_any_mouse_down(move |_, _, cx| rearm(&rearm_state, cx));
    // Layers the thread opens sit above it on the dismissal stack.
    let surface = layer(
        provide_scope(scope.clone(), content),
        layer_dismiss(scope, state, on_dismiss),
    )
    .escape(EscapeBehavior::Close);
    root.child(div().id("comment-thread-layer").w_full().child(surface))
        .into_any_element()
}
