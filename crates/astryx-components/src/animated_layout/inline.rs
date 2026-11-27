//! The two-state inline edge reveal (AST-105 FR9's inline two-state form), upstream
//! `holdLayoutEdge` and `playLayoutEdge`: an element whose two states differ in extent
//! along the inline axis (the SideNav rail, collapsed and expanded) snaps its layout to
//! the new extent at once, and its moving edge travels from the extent it showed to the
//! new one on the area's one motion (FR4), riding the glide of the content beside it frame
//! by frame: the edge's extent is the same sum of glide segments the area plays for that
//! content (fork patch R19), so the two never part, also when a change interrupts a
//! running one. A reversal starts from what the element shows.
//!
//! The old state is held for the motion (FR8's transient copy): the element presents its
//! old state for the part of the motion where upstream's copy covers it
//! ([`EdgeMotion::presents_old`]), and the caller draws the copy itself where the two
//! cross-fade, as a snapshot of the element's last frame of the old state (fork patch R3's
//! kept offscreen layers; SideNav's rail keeps its frames for it). The caller's own fades
//! (the rail's labels and icons) play on [`EdgeMotion::progress`]. Under Reduce Motion
//! with a coarse primary pointer nothing travels and only opacity changes (DR2); with a
//! fine one the change lands at once (DR3).

use gpui::{App, ElementId, Pixels, Window, px};
use web_time::Instant;

use super::{AnimatedLayout, LayoutMotion};

pub(crate) struct EdgeState {
    state: bool,
    /// The latest change conceals: its new extent is smaller.
    conceal: bool,
    /// The extent laid out for the state, last frame.
    extent: Pixels,
    /// The edge's glide: each change's offset from the new extent, and its start.
    segments: Vec<(Pixels, Instant)>,
    /// When the latest change began, the state before it, and what the caller's own
    /// fades showed then.
    since: Option<Instant>,
    old: bool,
    carried: [f32; 2],
    /// What the caller's fades show now (recorded each frame).
    shown: [f32; 2],
}

/// What a two-state edge shows this frame.
#[derive(Clone)]
pub(crate) struct EdgeMotion {
    edge: gpui::Entity<EdgeState>,
    /// The latest change conceals.
    pub conceals: bool,
    /// The state now, and the one before the latest change.
    pub state: bool,
    pub old: bool,
    /// The latest change's progress, 0 to 1 (1 at rest).
    pub progress: f32,
    /// Whether a change plays.
    pub moving: bool,
    /// The extent the moving edge shows (the laid-out extent at rest).
    pub extent: Pixels,
    /// Reduce Motion under a coarse pointer: opacity only.
    pub cross_fade: bool,
    /// What the caller's fades showed when the latest change began.
    pub carried: [f32; 2],
}

impl EdgeMotion {
    /// Records what the caller's own fades show this frame, so a reversal starts from
    /// them.
    pub fn record(&self, shown: [f32; 2], cx: &mut App) {
        self.edge.update(cx, |s, _| s.shown = shown);
    }

    /// Whether the element presents its old state now: upstream's copy of the old state
    /// covers a conceal until its fade begins (`fade_from` of the progress).
    pub fn presents_old(&self, fade_from: f32) -> bool {
        self.moving && self.old != self.state && self.progress < fade_from
    }
}

/// The edge of the element `id` names, in `state` with `extent` laid out, moving on
/// `layout`'s area (or the same motion on its own without one). Call it where the element
/// renders, every frame; a change of `state` starts a motion.
pub(crate) fn use_two_state_edge(
    id: impl Into<ElementId>,
    state: bool,
    extent: Pixels,
    layout: Option<&AnimatedLayout>,
    window: &mut Window,
    cx: &mut App,
) -> EdgeMotion {
    let id = id.into();
    let edge = window.use_keyed_state(
        ElementId::Name(format!("{id}-two-state-edge").into()),
        cx,
        move |_, _| EdgeState {
            state,
            conceal: false,
            extent,
            segments: Vec::new(),
            since: None,
            old: state,
            carried: [1., 1.],
            shown: [1., 1.],
        },
    );
    let motion = LayoutMotion::read(window, cx);
    let now = cx.background_executor().now();
    let (was, last_extent) = {
        let s = edge.read(cx);
        (s.state, s.extent)
    };
    if was != state {
        let animates = motion.animates();
        if animates && let Some(layout) = layout {
            layout.report_change(window, cx);
        }
        edge.update(cx, |s, _| {
            s.old = was;
            s.state = state;
            s.conceal = extent < last_extent;
            s.carried = s.shown;
            if animates {
                s.since = Some(now);
                if motion.cross_fade.is_none() {
                    s.segments.push((last_extent - extent, now));
                }
            } else {
                s.since = None;
                s.segments.clear();
            }
        });
    }
    let (shown, progress, moving, old, carried, conceals) = edge.update(cx, |s, _| {
        s.extent = extent;
        let duration = motion.duration;
        s.segments.retain(|(_, start)| {
            motion.cross_fade.is_none() && motion.progress(*start, now) < 1. && !duration.is_zero()
        });
        let mut shown = extent;
        for (offset, start) in &s.segments {
            let rest = 1. - motion.progress(*start, now);
            shown += *offset * rest;
        }
        let progress = s.since.map_or(1., |since| motion.progress(since, now));
        if progress >= 1. && s.segments.is_empty() {
            s.since = None;
        }
        let moving = s.since.is_some();
        (
            shown.max(px(0.)),
            progress,
            moving,
            s.old,
            s.carried,
            s.conceal,
        )
    });
    if moving {
        window.request_animation_frame();
    }
    EdgeMotion {
        edge,
        conceals,
        state,
        old,
        progress: if moving { progress } else { 1. },
        moving,
        extent: shown,
        cross_fade: motion.cross_fade.is_some(),
        carried,
    }
}
