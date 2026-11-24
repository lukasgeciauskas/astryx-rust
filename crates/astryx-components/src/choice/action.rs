//! The optimistic async change shared by CheckboxInput, Switch and the
//! lists (upstream `changeAction` with `useOptimistic`): while an action
//! runs, the control shows the value it asked for and is busy; when it
//! settles, the control shows its `value` prop again.

use std::rc::Rc;

use astryx_primitives::{Politeness, announce};
use gpui::{App, ElementId, Entity, SharedString, Task, Window};

pub(crate) struct ActionState<T> {
    pub optimistic: Option<T>,
    pending: usize,
    announced: bool,
}

pub(crate) type ChangeFn<T> = Rc<dyn Fn(T, &mut Window, &mut App)>;
pub(crate) type ActionFn<T> = Rc<dyn Fn(T, &mut Window, &mut App) -> Task<()>>;

pub(crate) struct Optimistic<T: 'static> {
    state: Entity<ActionState<T>>,
}

impl<T: Clone + 'static> Optimistic<T> {
    pub fn new(id: &ElementId, window: &mut Window, cx: &mut App) -> Self {
        let state = window.use_keyed_state(
            ElementId::Name(format!("{id:?}-action").into()),
            cx,
            |_, _| ActionState {
                optimistic: None,
                pending: 0,
                announced: false,
            },
        );
        Optimistic { state }
    }

    /// The value to show: the one an action asked for while it runs.
    pub fn shown(&self, value: T, cx: &App) -> T {
        self.state.read(cx).optimistic.clone().unwrap_or(value)
    }

    pub fn pending(&self, cx: &App) -> bool {
        self.state.read(cx).pending > 0
    }

    /// Announces each busy start once (upstream `useBusyAnnouncement`).
    pub fn announce_busy(&self, busy: bool, text: SharedString, window: &mut Window, cx: &mut App) {
        let announced = self.state.read(cx).announced;
        if busy && !announced {
            self.state.update(cx, |s, _| s.announced = true);
            announce(text, Politeness::Polite, window, cx);
        } else if !busy && announced {
            self.state.update(cx, |s, _| s.announced = false);
        }
    }

    /// The change handler: `on_change` first, then the action with the
    /// value shown optimistically until it settles.
    pub fn change(
        &self,
        on_change: Option<ChangeFn<T>>,
        action: Option<ActionFn<T>>,
    ) -> impl Fn(T, &mut Window, &mut App) + 'static {
        let state = self.state.clone();
        move |next: T, window: &mut Window, cx: &mut App| {
            if let Some(f) = &on_change {
                f(next.clone(), window, cx);
            }
            let Some(action) = &action else { return };
            let task = action(next.clone(), window, cx);
            state.update(cx, |s, cx| {
                s.optimistic = Some(next);
                s.pending += 1;
                cx.notify();
            });
            let state = state.clone();
            window
                .spawn(cx, async move |cx| {
                    task.await;
                    cx.update(|_, cx| {
                        state.update(cx, |s, cx| {
                            s.pending = s.pending.saturating_sub(1);
                            if s.pending == 0 {
                                s.optimistic = None;
                            }
                            cx.notify();
                        });
                    })
                    .ok();
                })
                .detach();
        }
    }
}
