//! Development diagnostics, upstream's deduplicated `devWarning` and
//! `warnOnce`.

use std::cell::RefCell;
use std::collections::HashSet;

thread_local! {
    static SEEN: RefCell<HashSet<String>> = RefCell::default();
}

/// Prints a development diagnostic once per `key`, in debug builds only.
pub fn dev_warn_once(key: &str, message: impl FnOnce() -> String) {
    if !cfg!(debug_assertions) {
        return;
    }
    if SEEN.with(|s| s.borrow_mut().insert(key.to_string())) {
        eprintln!("astryx: {}", message());
    }
}
