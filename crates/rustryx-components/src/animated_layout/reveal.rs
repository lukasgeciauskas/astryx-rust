//! The edge reveal (AST-105 FR9, block-end form): an element that appears or disappears
//! because of a reported change reveals or conceals from its block-end edge on the shared
//! motion, its children sliding under a still clip at its box while the content after it
//! follows that edge. A reversal starts from what the element shows. Without a handle the
//! change shows at once (FR2); under Reduce Motion it cross-fades with a coarse pointer and
//! jumps otherwise.

use gpui::{
    AnyElement, App, ElementId, IntoElement, ParentElement, Pixels, Styled, Window, div, px,
};
use web_time::Instant;

use super::{AnimatedLayout, LayoutMotion};

struct RevealState {
    open: bool,
    /// The fraction shown when the current motion started, and when it started.
    from: f32,
    since: Option<Instant>,
    /// The content's natural height, measured each frame it draws.
    height: Pixels,
}

/// What an element revealing from its edge shows this frame.
pub(crate) struct EdgeReveal {
    /// Whether the content is rendered at all (open, or concealing).
    pub mounted: bool,
    /// The fraction of the content shown, while it moves; `None` at rest.
    shown: Option<f32>,
    /// Cross-fading instead of travelling.
    fade: bool,
    height: Pixels,
    state: gpui::Entity<RevealState>,
}

/// The reveal of the element `id` names, `open` or not, on `layout`'s motion (none
/// without a handle). Call it where the element renders, every frame.
pub(crate) fn use_edge_reveal(
    id: impl Into<ElementId>,
    open: bool,
    layout: Option<&AnimatedLayout>,
    window: &mut Window,
    cx: &mut App,
) -> EdgeReveal {
    let id = id.into();
    let state = window.use_keyed_state(
        ElementId::Name(format!("{id}-edge-reveal").into()),
        cx,
        move |_, _| RevealState {
            open,
            from: if open { 1. } else { 0. },
            since: None,
            height: px(0.),
        },
    );
    let motion = LayoutMotion::read(window, cx);
    let now = cx.background_executor().now();
    let shown_at = |s: &RevealState| -> f32 {
        let target = if s.open { 1. } else { 0. };
        match s.since {
            Some(since) => s.from + (target - s.from) * motion.progress(since, now),
            None => target,
        }
    };
    let changed = state.read(cx).open != open;
    if changed {
        let animates = layout.is_some() && motion.animates();
        if let Some(layout) = layout.filter(|_| animates) {
            layout.report_change(window, cx);
        }
        state.update(cx, |s, _| {
            let shown = shown_at(s);
            s.open = open;
            s.from = shown;
            s.since = animates.then_some(now);
        });
    }
    let (shown, height) = {
        let s = state.read(cx);
        (shown_at(s), s.height)
    };
    let target = if open { 1. } else { 0. };
    let moving = state.read(cx).since.is_some() && (shown - target).abs() > f32::EPSILON;
    if moving {
        window.request_animation_frame();
    } else if state.read(cx).since.is_some() {
        state.update(cx, |s, _| s.since = None);
    }
    EdgeReveal {
        mounted: open || moving,
        shown: moving.then_some(shown),
        fade: motion.cross_fade.is_some(),
        height,
        state,
    }
}

impl EdgeReveal {
    /// Whether the element is revealing or concealing.
    pub fn is_moving(&self) -> bool {
        self.shown.is_some()
    }

    /// The fraction shown (1 open at rest, 0 closed at rest).
    pub fn shown(&self) -> f32 {
        self.shown.unwrap_or(if self.mounted { 1. } else { 0. })
    }

    /// The content inside the reveal's clip; nothing when it is closed at rest.
    pub fn wrap(self, content: impl IntoElement) -> Option<AnyElement> {
        if !self.mounted {
            return None;
        }
        let state = self.state.clone();
        let measure = move |bounds: Vec<gpui::Bounds<Pixels>>, _: &mut Window, cx: &mut App| {
            if let Some(b) = bounds.first() {
                let h = b.size.height;
                state.update(cx, |s, _| s.height = h);
            }
        };
        let el = match self.shown {
            // At rest: the content in flow.
            None => div().child(content).on_children_prepainted(measure),
            // Cross-fade: in flow, fading.
            Some(shown) if self.fade => div()
                .opacity(shown)
                .child(content)
                .on_children_prepainted(measure),
            // Travel: the clip takes the shown part of the height; the content slides under
            // it from the block-end edge.
            Some(shown) => div()
                .relative()
                .overflow_hidden()
                .h(self.height * shown)
                .child(
                    div()
                        .absolute()
                        .top_0()
                        .left_0()
                        .right_0()
                        .translate_y(-(self.height * (1. - shown)))
                        .child(content),
                )
                .on_children_prepainted(measure),
        };
        Some(el.into_any_element())
    }
}
