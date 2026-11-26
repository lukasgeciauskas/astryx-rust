//! In-page anchors, the analogue of the DOM ids an in-page link (`#id`) targets. An element
//! marks itself with an id through [`anchor_probe`], and its box, with the scroll root it
//! sits in, is kept for the next frame: Outline's scroll spy and navigation read them.

use std::collections::HashMap;

use gpui::{
    App, Bounds, Global, IntoElement, Pixels, ScrollHandle, SharedString, Styled, Window, WindowId,
    canvas,
};

use crate::use_context;

/// The scroll region an anchor sits in, provided by a scrolling container to its content
/// (upstream: the nearest scrollable ancestor).
#[derive(Clone)]
pub struct ScrollRoot(pub ScrollHandle);

/// Where an anchored element was drawn last frame.
#[derive(Clone)]
pub struct Anchor {
    pub bounds: Bounds<Pixels>,
    pub scroll_root: Option<ScrollHandle>,
}

#[derive(Default)]
struct Anchors(HashMap<(WindowId, SharedString), Anchor>);

impl Global for Anchors {}

/// A probe recording the box of the element it fills as anchor `id`: add it as an
/// absolutely positioned, full-size child.
pub fn anchor_probe(id: SharedString, cx: &App) -> impl IntoElement {
    let root = use_context::<ScrollRoot>(cx).map(|r| r.0);
    canvas(
        move |bounds, window, cx| {
            let key = (window.window_handle().window_id(), id);
            cx.default_global::<Anchors>().0.insert(
                key,
                Anchor {
                    bounds,
                    scroll_root: root,
                },
            );
        },
        |_, _, _, _| {},
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full()
}

/// The anchor `id` in `window`, as drawn last frame.
pub fn anchor(id: &str, window: &Window, cx: &App) -> Option<Anchor> {
    let key = (
        window.window_handle().window_id(),
        SharedString::from(id.to_string()),
    );
    cx.try_global::<Anchors>()
        .and_then(|anchors| anchors.0.get(&key).cloned())
}
