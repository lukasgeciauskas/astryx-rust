//! A window's timers for the gesture hooks: a [`GestureClock`] whose callbacks run in the
//! window's context, followed by the hook's own flush (the work its callbacks queued for
//! the window and the app). The time is the executor's (fake under the test platform),
//! counted from the clock's creation, in ms.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;

use astryx_primitives::gestures::{GestureClock, TimerHandle};
use gpui::{App, AsyncWindowContext, BackgroundExecutor, ForegroundExecutor, Task, Window};
use web_time::Instant;

/// Work a hook runs with the window after its callbacks.
pub(crate) type Flush = Rc<dyn Fn(&mut Window, &mut App)>;

pub(crate) struct WindowClock {
    executor: BackgroundExecutor,
    foreground: ForegroundExecutor,
    origin: Instant,
    window: AsyncWindowContext,
    tasks: Rc<RefCell<HashMap<TimerHandle, Task<()>>>>,
    next: Cell<TimerHandle>,
    flush: Flush,
}

impl WindowClock {
    pub(crate) fn new(window: &mut Window, cx: &mut App, flush: Flush) -> Rc<Self> {
        Rc::new(Self {
            executor: cx.background_executor().clone(),
            foreground: cx.foreground_executor().clone(),
            origin: cx.background_executor().now(),
            window: window.to_async(cx),
            tasks: Rc::default(),
            next: Cell::new(0),
            flush,
        })
    }
}

impl GestureClock for WindowClock {
    fn now(&self) -> f64 {
        self.executor
            .now()
            .saturating_duration_since(self.origin)
            .as_secs_f64()
            * 1000.
    }

    fn set_timeout(&self, callback: Box<dyn FnOnce()>, ms: f64) -> TimerHandle {
        let handle = self.next.get() + 1;
        self.next.set(handle);
        let timer = self
            .executor
            .timer(Duration::from_secs_f64(ms.max(0.) / 1000.));
        let mut window = self.window.clone();
        let tasks = Rc::downgrade(&self.tasks);
        let flush = self.flush.clone();
        let task = self.foreground.spawn(async move {
            timer.await;
            if let Some(tasks) = tasks.upgrade() {
                tasks.borrow_mut().remove(&handle);
            }
            window
                .update(|window, cx| {
                    callback();
                    flush(window, cx);
                })
                .ok();
        });
        self.tasks.borrow_mut().insert(handle, task);
        handle
    }

    fn clear_timeout(&self, handle: TimerHandle) {
        self.tasks.borrow_mut().remove(&handle);
    }
}
