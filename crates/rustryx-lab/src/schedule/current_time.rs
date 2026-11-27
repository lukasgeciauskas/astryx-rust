//! The clock the current-time affordances read (upstream `useCurrentTime.ts`): one store
//! every mounted Schedule shares, read when the first one mounts and once a minute while
//! any stays mounted; once the last one unmounts the next mount reads the clock again.

use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};
use std::time::Duration;

use gpui::{App, ElementId, Task, Window};

use super::types::Instant;

const UPDATE_INTERVAL: Duration = Duration::from_secs(60);

#[cfg(test)]
thread_local! {
    static TEST_NOW: Cell<Option<Instant>> = const { Cell::new(None) };
}

/// For tests: what the clock reads (none: the system's).
#[cfg(test)]
pub(crate) fn set_test_now(now: Option<Instant>) {
    TEST_NOW.with(|n| n.set(now));
}

/// The system clock, epoch milliseconds (upstream `Date.now()`).
pub(crate) fn now() -> Instant {
    #[cfg(test)]
    if let Some(now) = TEST_NOW.with(Cell::get) {
        return now;
    }
    web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .map_or(0., |d| d.as_millis() as f64)
}

struct Store {
    time: Cell<Instant>,
    tick: RefCell<Option<Task<()>>>,
}

thread_local! {
    static STORE: RefCell<Weak<Store>> = const { RefCell::new(Weak::new()) };
}

/// A mounted Schedule's hold on the store; the store lives while any is held.
struct Subscription(Rc<Store>);

fn subscribe(cx: &mut App) -> Rc<Store> {
    if let Some(store) = STORE.with(|s| s.borrow().upgrade()) {
        return store;
    }
    // The first subscriber reads the clock: the store may be long gone or never made.
    let store = Rc::new(Store {
        time: Cell::new(now()),
        tick: RefCell::new(None),
    });
    let weak = Rc::downgrade(&store);
    let task = cx.spawn(async move |cx| {
        loop {
            cx.background_executor().timer(UPDATE_INTERVAL).await;
            let Some(store) = weak.upgrade() else { break };
            store.time.set(now());
            drop(store);
            cx.refresh();
        }
    });
    *store.tick.borrow_mut() = Some(task);
    STORE.with(|s| *s.borrow_mut() = Rc::downgrade(&store));
    store
}

/// The shared ticking time, for the Schedule `id` names (upstream `useCurrentTime`).
pub(crate) fn use_current_time(id: &str, window: &mut Window, cx: &mut App) -> Instant {
    let store = subscribe(cx);
    let subscription =
        window.use_keyed_state(ElementId::Name(format!("{id}-clock").into()), cx, |_, _| {
            Subscription(store)
        });
    subscription.read(cx).0.time.get()
}
