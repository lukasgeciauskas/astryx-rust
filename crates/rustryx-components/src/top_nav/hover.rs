//! The hover recognizer behind TopNav's menus: upstream `useMenuHover` with the hover
//! intent of `architecture:interaction-modality` INV11. An entry delay keeps a brush-over
//! from opening a menu; an exit guard, with the safe triangle toward the open surface,
//! keeps it open while the pointer travels there; and a group (the TopNav) shows the next
//! menu at once when the pointer moves from one open trigger to another. A click within
//! 500 ms of a hover-open confirms it rather than closing it. The keyboard opens at once,
//! and a finger is never served: it cannot hover.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use gpui::{App, Bounds, ElementId, Entity, IntoElement, Pixels, Point, Styled, Window, canvas};
use rustryx_primitives::coarse_primary_pointer;
use web_time::Instant;

use crate::overlay::in_safe_triangle;

pub(crate) type Action = Rc<dyn Fn(&mut Window, &mut App)>;
/// Opens the menu; the flag moves focus into it.
pub(crate) type ShowFn = Rc<dyn Fn(bool, &mut Window, &mut App)>;

/// Upstream `DEFAULT_CLICK_GUARD_MS`.
const CLICK_GUARD: Duration = Duration::from_millis(500);
/// Upstream `REOPEN_SUPPRESS_MS`: a surface that closes over its own trigger puts the
/// trigger back under a still pointer; that entry does not reopen it.
const REOPEN_SUPPRESS: Duration = Duration::from_millis(300);

/// A TopNav's hover group: which of its menus is open. Opening one closes the others
/// (upstream's auto popovers), and moving from an open one opens the next at once.
#[derive(Clone, Default)]
pub(crate) struct HoverGroup(Rc<RefCell<Option<ElementId>>>);

impl HoverGroup {
    fn open(&self) -> Option<ElementId> {
        self.0.borrow().clone()
    }

    fn set(&self, id: Option<ElementId>) {
        *self.0.borrow_mut() = id;
    }
}

#[derive(Default)]
pub(crate) struct HoverState {
    /// Opened by hover, so leaving may close it.
    hover_mode: bool,
    closed_at: Option<Instant>,
    /// When the current open began as a hover-open; None once confirmed or closed.
    hover_opened_at: Option<Instant>,
    /// Bumped to cancel pending timers.
    generation: u64,
    was_open: bool,
    /// Where the pointer left: the safe triangle's apex.
    apex: Option<Point<Pixels>>,
    /// The open surface's box.
    surface: Option<Bounds<Pixels>>,
}

/// How a menu opens and closes.
pub(crate) struct MenuHoverOptions {
    pub open: bool,
    pub show: ShowFn,
    /// Closes, returning focus to the trigger when it was inside.
    pub hide: Action,
    /// Moves focus to the first item of the open menu.
    pub focus_menu: Action,
    pub enter: Duration,
    pub exit: Duration,
    pub group: Option<HoverGroup>,
}

/// One menu's hover recognizer.
#[derive(Clone)]
pub(crate) struct MenuHover {
    id: ElementId,
    state: Entity<HoverState>,
    open: bool,
    show: ShowFn,
    hide: Action,
    focus_menu: Action,
    enter: Duration,
    exit: Duration,
    group: Option<HoverGroup>,
}

impl MenuHover {
    pub fn new(
        id: &ElementId,
        options: MenuHoverOptions,
        window: &mut Window,
        cx: &mut App,
    ) -> Self {
        let state = window.use_keyed_state(
            ElementId::Name(format!("{id:?}-menu-hover").into()),
            cx,
            |_, _| HoverState::default(),
        );
        let open = options.open;
        let closed = state.update(cx, |s, _| {
            let closed = s.was_open && !open;
            if closed {
                s.hover_mode = false;
                s.hover_opened_at = None;
                s.closed_at = Some(Instant::now());
                s.generation += 1;
                s.apex = None;
                s.surface = None;
            }
            s.was_open = open;
            closed
        });
        let hover = MenuHover {
            id: id.clone(),
            state,
            open,
            show: options.show,
            hide: options.hide,
            focus_menu: options.focus_menu,
            enter: options.enter,
            exit: options.exit,
            group: options.group,
        };
        if let Some(group) = &hover.group {
            let current = group.open();
            if closed && current.as_ref() == Some(id) {
                group.set(None);
            }
            match current {
                // Another menu of the group opened: this one closes.
                Some(other) if open && &other != id => {
                    let hide = hover.hide.clone();
                    window.defer(cx, move |window, cx| hide(window, cx));
                }
                None if open => group.set(Some(id.clone())),
                _ => {}
            }
        }
        hover
    }

    fn bump(&self, cx: &mut App) {
        self.state.update(cx, |s, _| s.generation += 1);
    }

    fn claim(&self) {
        if let Some(group) = &self.group {
            group.set(Some(self.id.clone()));
        }
    }

    fn show_by_hover(&self, window: &mut Window, cx: &mut App) {
        self.state
            .update(cx, |s, _| s.hover_opened_at = Some(Instant::now()));
        self.claim();
        (self.show)(false, window, cx);
    }

    fn show_and_focus(&self, window: &mut Window, cx: &mut App) {
        self.state.update(cx, |s, _| {
            s.closed_at = None;
            s.hover_mode = false;
            s.hover_opened_at = None;
        });
        self.claim();
        (self.show)(true, window, cx);
    }

    /// Upstream `handleMouseEnter`: the pointer entered the trigger.
    pub fn trigger_enter(&self, window: &mut Window, cx: &mut App) {
        if coarse_primary_pointer(window, cx) {
            return;
        }
        let suppressed = self
            .state
            .read(cx)
            .closed_at
            .is_some_and(|at| at.elapsed() < REOPEN_SUPPRESS);
        if suppressed {
            return;
        }
        if self.open {
            self.bump(cx);
            return;
        }
        let generation = self.state.update(cx, |s, _| {
            s.hover_mode = true;
            s.generation += 1;
            s.generation
        });
        // INV11's group: from another open menu of the nav, this one shows at once.
        let handoff = self
            .group
            .as_ref()
            .and_then(HoverGroup::open)
            .is_some_and(|other| other != self.id);
        if handoff || self.enter.is_zero() {
            self.show_by_hover(window, cx);
            return;
        }
        let this = self.clone();
        let timer = cx.background_executor().timer(self.enter);
        window
            .spawn(cx, async move |cx| {
                timer.await;
                cx.update(|window, cx| {
                    if this.state.read(cx).generation == generation {
                        this.show_by_hover(window, cx);
                    }
                })
                .ok();
            })
            .detach();
    }

    /// Upstream `handleMouseLeave`: the pointer left the trigger or the surface.
    pub fn leave(&self, window: &mut Window, cx: &mut App) {
        let apex = window.mouse_position();
        let generation = self.state.update(cx, |s, _| {
            s.closed_at = None;
            if !s.hover_mode {
                return None;
            }
            s.generation += 1;
            s.apex = Some(apex);
            Some(s.generation)
        });
        if let Some(generation) = generation {
            self.schedule_hide(generation, window, cx);
        }
    }

    fn schedule_hide(&self, generation: u64, window: &mut Window, cx: &mut App) {
        let this = self.clone();
        let timer = cx.background_executor().timer(self.exit);
        window
            .spawn(cx, async move |cx| {
                timer.await;
                cx.update(|window, cx| {
                    let s = this.state.read(cx);
                    if s.generation != generation {
                        return;
                    }
                    // Still inside the safe triangle toward the surface: heading there.
                    let heading = match (s.apex, s.surface) {
                        (Some(apex), Some(surface)) => {
                            in_safe_triangle(window.mouse_position(), apex, surface)
                        }
                        _ => false,
                    };
                    if heading {
                        this.schedule_hide(generation, window, cx);
                    } else {
                        (this.hide)(window, cx);
                    }
                })
                .ok();
            })
            .detach();
    }

    /// Upstream `handleContentMouseEnter`: the pointer reached the surface.
    pub fn content_enter(&self, cx: &mut App) {
        self.bump(cx);
    }

    /// Upstream `handleClick`; `keyboard` for Enter or Space.
    pub fn click(&self, keyboard: bool, window: &mut Window, cx: &mut App) {
        self.bump(cx);
        if keyboard {
            if self.open {
                self.state.update(cx, |s, _| {
                    s.closed_at = None;
                    s.hover_mode = false;
                    s.hover_opened_at = None;
                });
                (self.focus_menu)(window, cx);
            } else {
                self.show_and_focus(window, cx);
            }
            return;
        }
        if !self.open {
            self.show_and_focus(window, cx);
            return;
        }
        if self.confirm(cx) {
            (self.focus_menu)(window, cx);
            return;
        }
        (self.hide)(window, cx);
    }

    /// Upstream `confirmHoverOpen`: a click soon after a hover-open pins the menu.
    pub fn confirm(&self, cx: &mut App) -> bool {
        self.state.update(cx, |s, _| {
            let confirming = s
                .hover_opened_at
                .is_some_and(|at| at.elapsed() < CLICK_GUARD);
            if confirming {
                s.hover_mode = false;
                s.hover_opened_at = None;
            }
            confirming
        })
    }

    /// Upstream `close`: hides, returning focus to the trigger.
    pub fn close(&self, window: &mut Window, cx: &mut App) {
        self.bump(cx);
        (self.hide)(window, cx);
    }

    /// The close, as an action.
    pub fn close_action(&self) -> Action {
        let this = self.clone();
        Rc::new(move |window, cx| this.close(window, cx))
    }

    /// A probe for the open surface: its box feeds the safe triangle.
    pub fn surface_probe(&self) -> impl IntoElement {
        let state = self.state.clone();
        canvas(
            move |bounds, _, cx| state.update(cx, |s, _| s.surface = Some(bounds)),
            |_, _, _, _| {},
        )
        .absolute()
        .top_0()
        .left_0()
        .size_full()
    }
}
