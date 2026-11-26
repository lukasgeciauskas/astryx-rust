//! The native scrollers under a touch, upstream `gestures/nativeScroll.ts`.
//!
//! Answers one question for a recognizer without letting it read layout:
//! would a native scroller move if this finger moved that way? GPUI has no
//! DOM to walk, so scrollers register with the window's host each frame
//! ([`register_native_scroller`], from prepaint) with their bounds, scroll
//! handle and what a finger may pan; the probe chains them innermost first,
//! as the browser chains scroll, with three narrowings that match it: the
//! axes a finger may pan there, a scroller that stops the chain
//! (`overscroll_contain`), and a finger moving down scrolling content
//! toward its start.

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{Bounds, Pixels, ScrollHandle};

use super::types::{NativeScrollProbe, Point, ScrollProbe, ViewKey};

const EDGE_TOLERANCE: f64 = 1.;

/// One scroller, as it registers for a frame.
#[derive(Clone)]
pub struct NativeScroller {
    /// A stable key for the scroller (a gesture area id, or any id).
    pub key: ViewKey,
    pub bounds: Bounds<Pixels>,
    pub handle: ScrollHandle,
    /// Whether it scrolls horizontally and vertically.
    pub scrolls_x: bool,
    pub scrolls_y: bool,
    /// Whether a finger may pan it on each axis (its touch behavior).
    pub finger_x: bool,
    pub finger_y: bool,
    /// Whether it stops scroll chaining on each axis (`overscroll_contain`).
    pub stops_x: bool,
    pub stops_y: bool,
}

impl NativeScroller {
    /// A scroller that scrolls and chains on both axes.
    pub fn new(key: ViewKey, bounds: Bounds<Pixels>, handle: ScrollHandle) -> Self {
        Self {
            key,
            bounds,
            handle,
            scrolls_x: true,
            scrolls_y: true,
            finger_x: true,
            finger_y: true,
            stops_x: false,
            stops_y: false,
        }
    }

    fn contains(&self, point: Point) -> bool {
        self.bounds.contains(&gpui::point(
            gpui::px(point.x as f32),
            gpui::px(point.y as f32),
        ))
    }

    fn can_move_y(&self, dy: f64) -> bool {
        let max = f64::from(f32::from(self.handle.max_offset().y));
        if max <= EDGE_TOLERANCE {
            return false;
        }
        // GPUI's offset runs from 0 to minus the overflow.
        let top = -f64::from(f32::from(self.handle.offset().y));
        if dy > 0. {
            top > EDGE_TOLERANCE - 0.5
        } else {
            top < max - EDGE_TOLERANCE + 0.5
        }
    }

    fn can_move_x(&self, dx: f64) -> bool {
        let max = f64::from(f32::from(self.handle.max_offset().x));
        if max <= EDGE_TOLERANCE {
            return false;
        }
        let left = -f64::from(f32::from(self.handle.offset().x));
        // A finger moving right drags the content right: the offset falls.
        if dx > 0. {
            left > EDGE_TOLERANCE - 0.5
        } else {
            left < max - EDGE_TOLERANCE + 0.5
        }
    }

    /// `canElementScroll`: whether this scroller moves now for a finger
    /// moving by (dx, dy). A wheel moves content the other way: pass its
    /// deltas negated.
    pub fn can_scroll(&self, dx: f64, dy: f64) -> bool {
        (dy != 0. && self.scrolls_y && self.can_move_y(dy))
            || (dx != 0. && self.scrolls_x && self.can_move_x(dx))
    }
}

/// The scrollers a window registered for its current frame.
#[derive(Clone, Default)]
pub struct ScrollerRegistry(Rc<RefCell<Vec<NativeScroller>>>);

impl ScrollerRegistry {
    /// Registers (or re-registers) a scroller; later registrations are
    /// inner.
    pub fn register(&self, scroller: NativeScroller) {
        let mut list = self.0.borrow_mut();
        list.retain(|s| s.key != scroller.key);
        list.push(scroller);
    }

    /// Forgets a scroller.
    pub fn unregister(&self, key: ViewKey) {
        self.0.borrow_mut().retain(|s| s.key != key);
    }

    /// The scroller with this key.
    pub fn get(&self, key: ViewKey) -> Option<NativeScroller> {
        self.0.borrow().iter().find(|s| s.key == key).cloned()
    }

    /// The scrollers under `point`, innermost first.
    pub fn chain_at(&self, point: Point) -> Vec<NativeScroller> {
        self.0
            .borrow()
            .iter()
            .rev()
            .filter(|s| s.contains(point))
            .cloned()
            .collect()
    }

    /// A probe of the scrollers under `point` (upstream
    /// `createNativeScrollProbe`). A wheel's probe ignores what a finger may
    /// pan (`reads_touch_action: false`).
    pub fn probe(&self, point: Point, reads_touch_action: bool) -> ScrollProbe {
        Rc::new(ChainProbe {
            chain: self.chain_at(point),
            reads_touch_action,
        })
    }
}

struct ChainProbe {
    chain: Vec<NativeScroller>,
    reads_touch_action: bool,
}

impl NativeScrollProbe for ChainProbe {
    fn can_scroll(&self, dx: f64, dy: f64) -> bool {
        let (ax, ay) = (dx.abs(), dy.abs());
        // The minor axis of a mostly straight drag is the hand's wobble.
        let consider_x = ax > 0. && ax >= ay / 2.;
        let consider_y = ay > 0. && ay >= ax / 2.;
        let mut allow_x = true;
        let mut allow_y = true;
        for entry in &self.chain {
            if self.reads_touch_action {
                allow_x &= entry.finger_x;
                allow_y &= entry.finger_y;
            }
            if consider_y && entry.scrolls_y && allow_y && entry.can_move_y(dy) {
                return true;
            }
            if consider_x && entry.scrolls_x && allow_x && entry.can_move_x(dx) {
                return true;
            }
            let stops = (!consider_x || entry.stops_x) && (!consider_y || entry.stops_y);
            if stops && (entry.stops_x || entry.stops_y) {
                return false;
            }
        }
        false
    }

    fn is_inside_scroller(&self) -> bool {
        !self.chain.is_empty()
    }

    fn allows_native_pan(&self) -> bool {
        !self.reads_touch_action || self.chain.iter().all(|s| s.finger_x || s.finger_y)
    }
}

/// A probe with a scripted answer, for tests and hosts that know better.
pub struct ScriptedProbe<F: Fn(f64, f64) -> bool> {
    pub can_scroll: F,
    pub is_inside_scroller: bool,
}

impl<F: Fn(f64, f64) -> bool> NativeScrollProbe for ScriptedProbe<F> {
    fn can_scroll(&self, dx: f64, dy: f64) -> bool {
        (self.can_scroll)(dx, dy)
    }
    fn is_inside_scroller(&self) -> bool {
        self.is_inside_scroller
    }
}
