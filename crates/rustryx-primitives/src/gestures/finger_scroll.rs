//! A finger's scroll, upstream `gestures/fingerScroll.ts` and
//! `useFingerScroll.ts`: whether a watched scroller's scroll is a finger's.
//!
//! A watched scroller's scroll is a finger's from a touch scroll while a
//! finger that landed inside it is down, until it ends: GPUI reports no
//! `scrollend`, so a quiet spell of 150 ms after its last touch scroll
//! ends it (the momentum after the lift is still the finger's). A scroll
//! that ends while its finger still rests there stays the finger's until the
//! lift. A scroll a program starts while no finger is down there is not.
//! Scrollers are keys the host knows the bounds of: a gesture area's, or a
//! registered native scroller's.

use std::rc::{Rc, Weak};

use super::host::{GestureHost, HostInner};
use super::types::{Point, ViewKey};

const SCROLL_END_FALLBACK_MS: f64 = 150.;

/// Watches one scroller until dropped (upstream `watchFingerScrolls`'s
/// stop function).
pub struct FingerScrollWatch {
    host: Weak<HostInner>,
    scroller: ViewKey,
    listener: Option<u64>,
}

impl Drop for FingerScrollWatch {
    fn drop(&mut self) {
        let Some(host) = GestureHost::from_weak(&self.host) else {
            return;
        };
        let state = host.watch_state();
        let mut watch = state.borrow_mut();
        if let Some(id) = self.listener {
            watch.end_listeners.retain(|(_, l, _)| *l != id);
        }
        let key = self.scroller;
        let mut count = 0;
        for entry in watch.scrollers.iter_mut() {
            if entry.0 == key {
                entry.1 -= 1;
                count = entry.1;
            }
        }
        if count > 0 {
            return;
        }
        watch.scrollers.retain(|(k, _)| *k != key);
        watch.finger_scrollers.retain(|k| *k != key);
        watch.ended_while_down.retain(|k| *k != key);
        watch.landings.retain(|(k, _)| *k != key);
        watch.starts.retain(|(k, _)| *k != key);
        watch.last_spans.retain(|(k, _, _)| *k != key);
    }
}

/// Keeps track of whether the scrolls of `scroller` are a finger's until the
/// watch drops. `on_end` runs each time a finger's scroll there ends, after
/// [`is_finger_scroll`] turns false; dropping the watch does not call it.
pub fn watch_finger_scrolls(
    host: &GestureHost,
    scroller: ViewKey,
    on_end: Option<Rc<dyn Fn()>>,
) -> FingerScrollWatch {
    let state = host.watch_state();
    let mut watch = state.borrow_mut();
    match watch.scrollers.iter_mut().find(|(k, _)| *k == scroller) {
        Some(entry) => entry.1 += 1,
        None => watch.scrollers.push((scroller, 1)),
    }
    let listener = on_end.map(|on_end| {
        let id = host.next_listener_id();
        watch.end_listeners.push((scroller, id, on_end));
        id
    });
    FingerScrollWatch {
        host: host.downgrade(),
        scroller,
        listener,
    }
}

/// Whether the scroll now moving a watched `scroller` is a finger's; given a
/// time on the host's clock, whether the scroll at that time was.
pub fn is_finger_scroll(host: &GestureHost, scroller: ViewKey, at: Option<f64>) -> bool {
    let state = host.watch_state();
    let watch = state.borrow();
    let Some(at) = at else {
        return watch.finger_scrollers.contains(&scroller);
    };
    if watch
        .starts
        .iter()
        .any(|(k, start)| *k == scroller && at >= *start)
    {
        return true;
    }
    watch
        .last_spans
        .iter()
        .any(|(k, start, end)| *k == scroller && at >= *start && at <= *end)
}

fn end_finger_scroll(host: &GestureHost, scroller: ViewKey, at: f64) {
    let listeners: Vec<Rc<dyn Fn()>> = {
        let state = host.watch_state();
        let mut watch = state.borrow_mut();
        if !watch.finger_scrollers.contains(&scroller) {
            return;
        }
        watch.finger_scrollers.retain(|k| *k != scroller);
        let start = watch
            .starts
            .iter()
            .find(|(k, _)| *k == scroller)
            .map_or(at, |(_, s)| *s);
        watch.last_spans.retain(|(k, _, _)| *k != scroller);
        watch.last_spans.push((scroller, start, at));
        watch.starts.retain(|(k, _)| *k != scroller);
        watch
            .end_listeners
            .iter()
            .filter(|(k, _, _)| *k == scroller)
            .map(|(_, _, l)| l.clone())
            .collect()
    };
    for listener in listeners {
        listener();
    }
}

fn end_scroll(host: &GestureHost, scroller: ViewKey) {
    let is_finger = host
        .watch_state()
        .borrow()
        .finger_scrollers
        .contains(&scroller);
    if !is_finger {
        return;
    }
    if host.is_finger_down_in(scroller) {
        // The finger still rests there: the scroll is its until the lift.
        let state = host.watch_state();
        let mut watch = state.borrow_mut();
        if !watch.ended_while_down.contains(&scroller) {
            watch.ended_while_down.push(scroller);
        }
    } else {
        end_finger_scroll(host, scroller, host.now());
    }
}

/// The host heard a touch scroll at `position` (a pan's start).
pub(crate) fn on_scroll(host: &GestureHost, position: Point, _phase: gpui::TouchPhase, time: f64) {
    let watched: Vec<ViewKey> = host
        .watch_state()
        .borrow()
        .scrollers
        .iter()
        .map(|(k, _)| *k)
        .collect();
    if watched.is_empty() {
        return;
    }
    let mut any = false;
    for scroller in watched {
        let Some(bounds) = host.bounds_of(scroller) else {
            continue;
        };
        if !bounds.contains(&gpui::point(
            gpui::px(position.x as f32),
            gpui::px(position.y as f32),
        )) {
            continue;
        }
        let state = host.watch_state();
        let mut watch = state.borrow_mut();
        watch.ended_while_down.retain(|k| *k != scroller);
        if host.is_finger_down_in(scroller) && !watch.finger_scrollers.contains(&scroller) {
            let start = watch
                .landings
                .iter()
                .find(|(k, _)| *k == scroller)
                .map_or(time, |(_, t)| *t);
            watch.starts.retain(|(k, _)| *k != scroller);
            watch.starts.push((scroller, start));
            watch.finger_scrollers.push(scroller);
        }
        any |= watch.finger_scrollers.contains(&scroller);
    }
    if any {
        // GPUI reports no scrollend: a quiet spell ends the finger's scroll.
        let previous = host.watch_state().borrow_mut().fallback.take();
        let clock = host.clock();
        if let Some(previous) = previous {
            clock.clear_timeout(previous);
        }
        let weak = host.downgrade();
        let handle = clock.set_timeout(
            Box::new(move || {
                let Some(host) = GestureHost::from_weak(&weak) else {
                    return;
                };
                host.watch_state().borrow_mut().fallback = None;
                let scrollers = host.watch_state().borrow().finger_scrollers.clone();
                for scroller in scrollers {
                    end_scroll(&host, scroller);
                }
            }),
            SCROLL_END_FALLBACK_MS,
        );
        host.watch_state().borrow_mut().fallback = Some(handle);
    }
}

/// The host's finger record changed.
pub(crate) fn on_fingers(host: &GestureHost) {
    let now = host.now();
    let (scrollers, ended): (Vec<ViewKey>, Vec<ViewKey>) = {
        let watch = host.watch_state().borrow();
        (
            watch.scrollers.iter().map(|(k, _)| *k).collect(),
            watch.ended_while_down.clone(),
        )
    };
    for scroller in scrollers {
        let down = host.is_finger_down_in(scroller);
        let state = host.watch_state();
        let mut watch = state.borrow_mut();
        if !down {
            watch.landings.retain(|(k, _)| *k != scroller);
        } else if !watch.landings.iter().any(|(k, _)| *k == scroller) {
            watch.landings.push((scroller, now));
        }
    }
    // A scroll that ended while its finger rested ends with the lift.
    for scroller in ended {
        if !host.is_finger_down_in(scroller) {
            host.watch_state()
                .borrow_mut()
                .ended_while_down
                .retain(|k| *k != scroller);
            end_finger_scroll(host, scroller, now);
        }
    }
}
