//! The layer runtime's input hold for touch presentations, upstream
//! `Layer/inputHold.ts` and `Layer/holdEntranceInput.ts`.
//!
//! Under a coarse primary pointer a layer that presents (a dialog growing in, a drawer
//! sliding in, a menu opening under the finger) takes no tap or drag until its entrance
//! ends, as in UIKit: [`hold_entrance_input`] holds a finger's input away from it for
//! that long. Keys and assistive technology stay live, and each event is judged by its
//! own pointer: a mouse's press is never held. A modal layer holds input anywhere in the
//! window ([`HoldScope::Viewport`]); a non-modal one only what lands on its own box
//! ([`HoldScope::Surface`]), so the page around it stays live.
//!
//! A held touch is held to its end, even past the hold's own end. A touch that began
//! before the hold is not held. The layer paints [`hold_probe`], which claims the held
//! touches from GPUI's recognizer (no taps, scrolls or long presses come of them), and
//! the gesture host ignores them, so no recognizer hears them either.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use gpui::{
    App, Bounds, ElementId, Global, IntoElement, Pixels, Point, Styled, TouchEvent, TouchPhase,
    Window, WindowId, canvas,
};
use web_time::Instant;

/// What a hold covers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum HoldScope {
    /// The whole window: a modal layer.
    Viewport,
    /// The layer's own box: a non-modal layer, whose page stays live.
    Surface(Bounds<Pixels>),
}

#[derive(Clone, Debug)]
struct Hold {
    key: ElementId,
    scope: HoldScope,
    until: Instant,
}

#[derive(Default)]
struct WindowHolds {
    holds: Vec<Hold>,
    /// The fingers judged held as they landed, held to their end.
    held_touches: HashSet<u64>,
}

#[derive(Default)]
struct InputHolds(HashMap<WindowId, WindowHolds>);

impl Global for InputHolds {}

/// Upstream `holdInput`: holds a finger's input away from the layer `key` until
/// `until`, over `scope`. A layer has one hold at a time: a new one replaces it.
pub fn hold_input(
    window: &Window,
    key: impl Into<ElementId>,
    scope: HoldScope,
    until: Instant,
    cx: &mut App,
) {
    let key = key.into();
    let id = window.window_handle().window_id();
    let holds = &mut cx
        .default_global::<InputHolds>()
        .0
        .entry(id)
        .or_default()
        .holds;
    holds.retain(|h| h.key != key);
    holds.push(Hold { key, scope, until });
}

/// Ends the layer's hold early (a close, or an unmount during the entrance).
pub fn release_input_hold(window: &Window, key: &ElementId, cx: &mut App) {
    let id = window.window_handle().window_id();
    if let Some(holds) = cx.try_global::<InputHolds>().and_then(|h| h.0.get(&id))
        && !holds.holds.iter().any(|h| &h.key == key)
    {
        return;
    }
    if let Some(w) = cx.default_global::<InputHolds>().0.get_mut(&id) {
        w.holds.retain(|h| &h.key != key);
    }
}

/// Upstream `holdEntranceInput`: under a coarse primary pointer, holds a finger's input
/// away from the layer `key` until its entrance, which began at `since` and lasts
/// `duration`, ends (at most `cap` when given). Read as the layer opens: a finger on a
/// device whose primary pointer is fine (a touchscreen laptop) is not held. Returns
/// whether it holds.
pub fn hold_entrance_input(
    window: &Window,
    key: impl Into<ElementId>,
    scope: HoldScope,
    since: Instant,
    duration: Duration,
    cap: Option<Duration>,
    cx: &mut App,
) -> bool {
    let key = key.into();
    let length = cap.map_or(duration, |cap| duration.min(cap));
    let until = since + length;
    if length.is_zero()
        || cx.background_executor().now() >= until
        || !crate::coarse_primary_pointer(window, cx)
    {
        release_input_hold(window, &key, cx);
        return false;
    }
    hold_input(window, key, scope, until, cx);
    true
}

/// [`hold_entrance_input`] for an entrance known by its progress (0 to 1) through
/// `duration`: holds for the rest of it.
pub fn hold_entrance_progress(
    window: &Window,
    key: impl Into<ElementId>,
    scope: HoldScope,
    progress: f32,
    duration: Duration,
    cx: &mut App,
) -> bool {
    let now = cx.background_executor().now();
    let elapsed = duration.mul_f32(progress.clamp(0., 1.));
    let since = now.checked_sub(elapsed).unwrap_or(now);
    hold_entrance_input(window, key, scope, since, duration, None, cx)
}

/// Whether the layer `key` holds input now.
pub fn is_holding(window: &Window, key: &ElementId, cx: &App) -> bool {
    let id = window.window_handle().window_id();
    let now = cx.background_executor().now();
    cx.try_global::<InputHolds>()
        .and_then(|h| h.0.get(&id))
        .is_some_and(|w| w.holds.iter().any(|h| &h.key == key && h.until > now))
}

/// Upstream `holdsTouch`: whether `event`'s finger is held. A finger is judged as it
/// lands (inside a live hold's scope, it is held) and keeps that verdict to its end.
pub fn holds_touch(window: WindowId, event: &TouchEvent, cx: &mut App) -> bool {
    let finger = event.id.0;
    let now = cx.background_executor().now();
    let Some(w) = cx.try_global::<InputHolds>().and_then(|h| h.0.get(&window)) else {
        return false;
    };
    if event.phase != TouchPhase::Started {
        return w.held_touches.contains(&finger);
    }
    let held = w
        .holds
        .iter()
        .any(|h| h.until > now && covers(h.scope, event.position));
    let w = cx
        .default_global::<InputHolds>()
        .0
        .get_mut(&window)
        .expect("checked above");
    w.holds.retain(|h| h.until > now);
    if held {
        w.held_touches.insert(finger);
    } else {
        w.held_touches.remove(&finger);
    }
    held
}

fn covers(scope: HoldScope, point: Point<Pixels>) -> bool {
    match scope {
        HoldScope::Viewport => true,
        HoldScope::Surface(bounds) => bounds.contains(&point),
    }
}

/// The layer's half of the hold: a probe that claims held fingers from GPUI's
/// recognizer. Paint it inside every layer that holds input.
pub fn hold_probe() -> impl IntoElement {
    canvas(
        |_, _, _| {},
        |_, _, window, _| {
            let id = window.window_handle().window_id();
            window.on_touch_event(move |event, _, cx| holds_touch(id, event, cx));
        },
    )
    .absolute()
    .size_0()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_surface_hold_covers_only_its_box_and_a_viewport_hold_everything() {
        let bounds = Bounds::new(
            gpui::point(gpui::px(10.), gpui::px(10.)),
            gpui::size(gpui::px(100.), gpui::px(50.)),
        );
        assert!(covers(
            HoldScope::Surface(bounds),
            gpui::point(gpui::px(20.), gpui::px(20.))
        ));
        assert!(!covers(
            HoldScope::Surface(bounds),
            gpui::point(gpui::px(200.), gpui::px(20.))
        ));
        assert!(covers(
            HoldScope::Viewport,
            gpui::point(gpui::px(900.), gpui::px(700.))
        ));
    }
}
