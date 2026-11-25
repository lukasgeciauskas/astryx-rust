//! What every anchored layer shares: its entrance, its gutters and its
//! clearance (upstream `layerAnimations`, `layerViewportInset`).

use astryx_primitives::{Placement, prefers_reduced_motion};
use astryx_theme::Tokens;
use gpui::{App, Div, Pixels, Stateful, Styled, Window, px};
use web_time::Instant;

/// The gap a layer keeps from its anchor: the system's standard clearance,
/// `--spacing-1`.
pub(crate) fn clearance(t: &Tokens, window: &Window) -> Pixels {
    t.spacing.s1.to_pixels(window.rem_size())
}

/// The gutter a layer keeps from the viewport's edges, `--spacing-4`.
pub(crate) fn gutter(t: &Tokens, window: &Window) -> Pixels {
    t.spacing.s4.to_pixels(window.rem_size())
}

/// The largest a layer may be: the viewport less its gutters.
pub(crate) fn viewport_cap(t: &Tokens, window: &Window) -> gpui::Size<Pixels> {
    let gutter = gutter(t, window);
    let viewport = window.viewport_size();
    gpui::size(
        (viewport.width - gutter * 2.).max(px(0.)),
        (viewport.height - gutter * 2.).max(px(0.)),
    )
}

/// A layer's entrance from `since`: a fade with a short slide from its
/// anchor's side and a grow from 95%, over `--duration-fast-max`. Under
/// reduced motion a pointer sees no motion and a finger a fade.
pub(crate) fn entrance(
    el: Stateful<Div>,
    since: Instant,
    side: Placement,
    rtl: bool,
    t: &Tokens,
    window: &mut Window,
    cx: &App,
) -> Stateful<Div> {
    let reduced = prefers_reduced_motion(cx);
    let touch = window.last_input_was_touch();
    if reduced && !touch {
        return el;
    }
    let now = cx.background_executor().now();
    let duration = t.duration.fast_max;
    let progress = if duration.is_zero() {
        1.
    } else {
        (now.saturating_duration_since(since).as_secs_f32() / duration.as_secs_f32()).min(1.)
    };
    if progress < 1. {
        window.request_animation_frame();
    }
    let eased = t.ease.standard.ease(progress);
    let el = el.opacity(eased);
    if reduced {
        return el;
    }
    let step = t.spacing.s2.to_pixels(window.rem_size()) * (1. - eased);
    let (dx, dy) = match (side, rtl) {
        (Placement::Above, _) => (px(0.), step),
        (Placement::Below, _) => (px(0.), -step),
        (Placement::End, false) | (Placement::Start, true) => (-step, px(0.)),
        (Placement::Start, false) | (Placement::End, true) => (step, px(0.)),
    };
    el.translate(dx, dy).scale(0.95 + 0.05 * eased)
}
