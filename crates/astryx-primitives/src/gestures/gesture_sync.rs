//! The gesture-sync depth, upstream `gestures/gestureSync.ts`: while code
//! outside input drives a recognizer (an `is_enabled` sync, a detach that
//! cancels a gesture in flight), no hook's listener calls its owner back
//! (react-component-runtime INV2, INV3). The binding's hooks read it.

use std::cell::Cell;

thread_local! {
    static DEPTH: Cell<u32> = const { Cell::new(0) };
}

/// Runs `body` as a gesture sync.
pub fn syncing_gestures<T>(body: impl FnOnce() -> T) -> T {
    DEPTH.with(|depth| depth.set(depth.get() + 1));
    let result = body();
    DEPTH.with(|depth| depth.set(depth.get() - 1));
    result
}

/// True while any gesture sync runs.
pub fn is_syncing_gestures() -> bool {
    DEPTH.with(|depth| depth.get() > 0)
}
