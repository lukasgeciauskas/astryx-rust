//! Shared timing for looping animations. Upstream pins spinners and
//! skeletons to the document timeline's origin, so every instance on screen
//! moves in phase; [`elapsed`] is that timeline here.

use std::time::Duration;

use web_time::Instant;

use gpui::{App, Global};

struct Timeline(Instant);

impl Global for Timeline {}

/// Time since the app's animation timeline began.
pub fn elapsed(cx: &mut App) -> Duration {
    // The executor's clock, which tests can advance.
    let now = cx.background_executor().now();
    if !cx.has_global::<Timeline>() {
        cx.set_global(Timeline(now));
    }
    now.saturating_duration_since(cx.global::<Timeline>().0)
}

struct TransitionState {
    target: bool,
    /// Progress toward "on" when the current transition began.
    from: f32,
    start: Option<Instant>,
}

/// A boolean eased between 0 ("off") and 1 ("on") over `duration`, like a
/// CSS transition on a property that follows it. Mounting starts at rest;
/// each change animates from wherever the last one had got to, so quick
/// reversals never jump. Under reduced motion it snaps. Requests frames
/// while it moves.
pub(crate) fn transition(
    key: impl Into<String>,
    on: bool,
    duration: Duration,
    ease: astryx_theme::CubicBezier,
    window: &mut gpui::Window,
    cx: &mut App,
) -> f32 {
    let now = cx.background_executor().now();
    let reduced = astryx_primitives::prefers_reduced_motion(cx);
    let state = window.use_keyed_state(gpui::ElementId::Name(key.into().into()), cx, |_, _| {
        TransitionState {
            target: on,
            from: if on { 1. } else { 0. },
            start: None,
        }
    });
    let progress = |s: &TransitionState| -> (f32, bool) {
        let end = if s.target { 1. } else { 0. };
        match s.start {
            Some(start) if !reduced && !duration.is_zero() => {
                let t = now.saturating_duration_since(start).as_secs_f32() / duration.as_secs_f32();
                if t >= 1. {
                    (end, false)
                } else {
                    (s.from + (end - s.from) * ease.ease(t), true)
                }
            }
            _ => (end, false),
        }
    };
    let (mut value, mut moving) = progress(state.read(cx));
    if state.read(cx).target != on {
        state.update(cx, |s, _| {
            s.from = value;
            s.target = on;
            s.start = Some(now);
        });
        (value, moving) = progress(state.read(cx));
    }
    if moving {
        window.request_animation_frame();
    }
    value
}
