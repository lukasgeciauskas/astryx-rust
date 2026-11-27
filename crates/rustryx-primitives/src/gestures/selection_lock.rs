//! The selection lock, upstream `gestures/selectionLock.ts` (a re-export of
//! `utils/selectionLock.ts`): for a hold that does something (a slider's
//! thumb, a switch's or a segmented control's drag, a stepper's hold-repeat),
//! text selection and the touch callout are off on the views it holds for
//! each held press (`spec:AST-094` FR41). GPUI has no long-press text
//! selection of its own, so the lock is a registry that selectable text in
//! this layer reads ([`is_selection_locked`]).

use std::cell::RefCell;
use std::rc::Rc;

use super::types::ViewKey;

/// Turns selection off on `views` (and on a non-text target inside one) for
/// one held press; returns the undo.
pub type SelectionLock = Rc<dyn Fn(&[ViewKey], Option<ViewKey>) -> Box<dyn FnOnce()>>;

thread_local! {
    static LOCKED: RefCell<Vec<ViewKey>> = const { RefCell::new(Vec::new()) };
}

thread_local! {
    static LOCK: SelectionLock = Rc::new(lock);
}

/// `lockSelection`: the one lock implementation (the same handle on every
/// call). Each call of the lock locks the views (and the target) until its
/// undo runs; locks stack.
pub fn lock_selection() -> SelectionLock {
    LOCK.with(Rc::clone)
}

fn lock(views: &[ViewKey], target: Option<ViewKey>) -> Box<dyn FnOnce()> {
    {
        let mut held: Vec<ViewKey> = views.to_vec();
        held.extend(target);
        LOCKED.with(|locked| locked.borrow_mut().extend(held.iter().copied()));
        Box::new(move || {
            LOCKED.with(|locked| {
                let mut locked = locked.borrow_mut();
                for view in &held {
                    if let Some(index) = locked.iter().position(|v| v == view) {
                        locked.remove(index);
                    }
                }
            });
        })
    }
}

/// `locksSelectionFor`: whether a press on `view` holds a selection lock now.
pub fn is_selection_locked(view: ViewKey) -> bool {
    LOCKED.with(|locked| locked.borrow().contains(&view))
}

/// Whether any held press locks selection now.
pub fn any_selection_locked() -> bool {
    LOCKED.with(|locked| !locked.borrow().is_empty())
}
