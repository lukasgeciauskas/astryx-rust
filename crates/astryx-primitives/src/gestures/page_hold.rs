//! Holding the page still under a finger, upstream `gestures/pageHold.ts`.
//!
//! A scroll lock (an overlay's) holds the page with [`hold_page_scroll`]:
//! while a hold is held, once per one-finger touch that no recognizer owns,
//! after the finger clears a 5 px slop, `decide(origin, dx, dy)` says whether
//! that pan would scroll the page (`origin` the innermost gesture area under
//! the finger); if so the touch's scroll (and the coasting after it) moves
//! nothing for its whole length. A second finger is never held, so a pinch
//! keeps working. Holds stack. GPUI has no page scroll of its own: the hold
//! keeps a finger's touch scrolls from every scroller under it. The host's
//! listeners come with any painted gesture area (a holder with none paints
//! an empty one).

use std::rc::Weak;

use super::host::{GestureHost, HostInner, PageScrollDecision};
use super::types::ViewKey;

/// A held page, released when dropped.
pub struct PageHold {
    host: Weak<HostInner>,
    id: u64,
}

impl Drop for PageHold {
    fn drop(&mut self) {
        if let Some(host) = GestureHost::from_weak(&self.host) {
            host.remove_page_hold(self.id);
        }
    }
}

impl PageHold {
    /// Releases the hold now.
    pub fn release(self) {}
}

/// Holds the page still under a finger until the returned hold drops.
pub fn hold_page_scroll(
    host: &GestureHost,
    decide: impl Fn(Option<ViewKey>, f64, f64) -> bool + 'static,
) -> PageHold {
    let decide: PageScrollDecision = std::rc::Rc::new(decide);
    PageHold {
        host: host.downgrade(),
        id: host.add_page_hold(decide),
    }
}
