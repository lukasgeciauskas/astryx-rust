//! useChatStreamScroll, upstream `Chat/useChatStreamScroll.ts`: a scroller that follows the
//! newest message with a spring while it is locked to the bottom, lets go when the reader
//! scrolls up, and locks again when they come back down.

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{App, Bounds, ElementId, Entity, Pixels, ScrollHandle, Window, point, px};
use rustryx_primitives::prefers_reduced_motion;
use web_time::Instant;

use super::context::MessageRegistry;

/// Upstream `behavior`: how `scroll_to_bottom` moves.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ChatScrollBehavior {
    /// Jump there.
    Instant,
    /// Glide there on the spring (a jump with reduced motion).
    #[default]
    Spring,
}

/// Upstream `ChatScrollToBottomOptions`.
#[derive(Clone, Copy, Debug, Default)]
pub struct ChatScrollToBottomOptions {
    pub behavior: ChatScrollBehavior,
}

/// Upstream `UseChatStreamScrollOptions`, less the scroller, which is an argument.
#[derive(Clone, Copy, Debug)]
pub struct UseChatStreamScrollOptions {
    /// Upstream `enabled`: when false, new content never scrolls the view.
    pub enabled: bool,
    /// Upstream `lockThreshold`: how near the bottom (px) a scroll must end to lock.
    pub lock_threshold: f32,
    /// Upstream `buttonThreshold`: how far above the bottom (px) counts as scrolled up.
    pub button_threshold: f32,
    /// The spring: upstream `damping`, `stiffness` and `mass`.
    pub damping: f32,
    pub stiffness: f32,
    pub mass: f32,
}

impl Default for UseChatStreamScrollOptions {
    fn default() -> Self {
        UseChatStreamScrollOptions {
            enabled: true,
            lock_threshold: 10.,
            button_threshold: 100.,
            damping: 0.7,
            stiffness: 0.05,
            mass: 1.25,
        }
    }
}

/// Upstream's 60 frames a second, the spring's unit of time.
const FRAME_MS: f32 = 1000. / 60.;

struct ScrollState {
    options: UseChatStreamScrollOptions,
    locked: bool,
    scrolled_up: bool,
    /// Until content first overflows, it jumps to the bottom instead of gliding.
    initial_fill_pending: bool,
    animating: bool,
    velocity: f32,
    spring_position: Option<f32>,
    last_tick: Option<Instant>,
    /// Where the view was at the last frame, to tell a reader's scroll from the
    /// spring's.
    last_scroll_top: f32,
    last_max: f32,
}

/// Upstream `UseChatStreamScrollReturn`: the scroller's state and its controls.
#[derive(Clone)]
pub struct ChatStreamScroll {
    handle: ScrollHandle,
    state: Entity<ScrollState>,
    messages: Option<Rc<RefCell<MessageRegistry>>>,
}

/// Upstream `useChatStreamScroll`, over `handle`, the scroller's GPUI scroll handle.
/// ChatLayout calls it for its own scroller; call it to build a chat on another
/// scroller, and call [`ChatStreamScroll::observe`] from that scroller's prepaint each
/// frame.
pub fn use_chat_stream_scroll(
    id: impl Into<ElementId>,
    handle: ScrollHandle,
    options: UseChatStreamScrollOptions,
    window: &mut Window,
    cx: &mut App,
) -> ChatStreamScroll {
    let id = id.into();
    let state = window.use_keyed_state(
        ElementId::Name(format!("{id}-chat-stream-scroll").into()),
        cx,
        |_, _| ScrollState {
            options,
            locked: true,
            scrolled_up: false,
            initial_fill_pending: true,
            animating: false,
            velocity: 0.,
            spring_position: None,
            last_tick: None,
            last_scroll_top: 0.,
            last_max: 0.,
        },
    );
    state.update(cx, |s, _| s.options = options);
    ChatStreamScroll {
        handle,
        state,
        messages: None,
    }
}

impl ChatStreamScroll {
    pub(crate) fn with_messages(mut self, messages: Rc<RefCell<MessageRegistry>>) -> Self {
        self.messages = Some(messages);
        self
    }

    fn scroll_top(&self) -> f32 {
        f32::from(-self.handle.offset().y)
    }

    fn max_top(&self) -> f32 {
        f32::from(self.handle.max_offset().y).max(0.)
    }

    fn set_scroll_top(&self, top: f32) {
        let x = self.handle.offset().x;
        self.handle.set_offset(point(x, px(-top)));
    }

    /// The scroller's visible box at the last layout.
    pub fn scrollport(&self) -> Bounds<Pixels> {
        self.handle.bounds()
    }

    /// Upstream `isScrolledUp`: the reader is far enough above the bottom for the
    /// scroll-to-bottom button.
    pub fn is_scrolled_up(&self, cx: &App) -> bool {
        self.state.read(cx).scrolled_up
    }

    /// Upstream `isLocked`: new content scrolls into view.
    pub fn is_locked(&self, cx: &App) -> bool {
        self.state.read(cx).locked
    }

    fn jump_to_bottom(&self, s: &mut ScrollState) {
        s.animating = false;
        s.velocity = 0.;
        s.last_tick = None;
        let max = self.max_top();
        self.set_scroll_top(max);
        s.last_scroll_top = max;
    }

    fn start_animation(&self, s: &mut ScrollState, reduced: bool) {
        if !s.locked {
            return;
        }
        if reduced {
            self.jump_to_bottom(s);
            return;
        }
        if !s.animating {
            s.animating = true;
            s.last_tick = None;
            s.spring_position = Some(self.scroll_top());
        }
    }

    /// Upstream `scrollToBottom`: locks, and moves to the newest content.
    pub fn scroll_to_bottom(
        &self,
        options: ChatScrollToBottomOptions,
        window: &mut Window,
        cx: &mut App,
    ) {
        let reduced = prefers_reduced_motion(cx);
        self.state.update(cx, |s, _| {
            s.locked = true;
            s.scrolled_up = false;
            s.initial_fill_pending = false;
            if options.behavior == ChatScrollBehavior::Instant {
                self.jump_to_bottom(s);
            } else {
                self.start_animation(s, reduced);
            }
        });
        window.refresh();
    }

    /// Upstream `scrollToMessage`: brings a message's top (its laid-out bounds) to the
    /// top of the scroller, at once.
    pub fn scroll_to_message(&self, message: Bounds<Pixels>, window: &mut Window, cx: &mut App) {
        let scrollport = self.handle.bounds();
        let top = f32::from(message.top() - scrollport.top()) + self.scroll_top();
        let top = top.clamp(0., self.max_top());
        self.set_scroll_top(top);
        self.state.update(cx, |s, _| s.last_scroll_top = top);
        window.refresh();
    }

    /// Upstream `scrollToLastMessage`: the newest ChatMessage's top at the scroller's top.
    pub fn scroll_to_last_message(&self, window: &mut Window, cx: &mut App) {
        let last = self
            .messages
            .as_ref()
            .and_then(|messages| messages.borrow().bounds.last().copied());
        if let Some(bounds) = last {
            self.scroll_to_message(bounds, window, cx);
        }
    }

    /// Upstream `lock`: follow new content again, from here.
    pub fn lock(&self, window: &mut Window, cx: &mut App) {
        let reduced = prefers_reduced_motion(cx);
        self.state.update(cx, |s, _| {
            s.locked = true;
            s.scrolled_up = false;
            self.start_animation(s, reduced);
        });
        window.refresh();
    }

    /// Upstream `unlock`: stop following new content.
    pub fn unlock(&self, cx: &mut App) {
        self.state.update(cx, |s, _| {
            s.locked = false;
            s.animating = false;
        });
    }

    /// Upstream `scrollIfLocked`: content grew; follow it if locked. The first time it
    /// overflows, it jumps.
    pub fn scroll_if_locked(&self, window: &mut Window, cx: &mut App) {
        let reduced = prefers_reduced_motion(cx);
        self.state.update(cx, |s, _| self.follow(s, reduced));
        window.refresh();
    }

    fn follow(&self, s: &mut ScrollState, reduced: bool) {
        if !s.options.enabled || !s.locked {
            return;
        }
        if s.initial_fill_pending && self.max_top() > 0. {
            s.initial_fill_pending = false;
            self.jump_to_bottom(s);
            return;
        }
        self.start_animation(s, reduced);
    }

    /// Reads the scroller once a frame, after layout: a reader's scroll locks or unlocks
    /// (upstream's `scroll` and `scrollend` listeners), grown content is followed, and the
    /// spring takes its next step.
    pub fn observe(&self, window: &mut Window, cx: &mut App) {
        let reduced = prefers_reduced_motion(cx);
        let now = cx.background_executor().now();
        let (refresh, frame) = self.state.update(cx, |s, _| {
            if !s.options.enabled {
                return (false, false);
            }
            let top = self.scroll_top();
            let max = self.max_top();
            let was_up = s.scrolled_up;
            let was_locked = s.locked;
            if (top - s.last_scroll_top).abs() > 0.5 {
                let dist = max - top;
                s.scrolled_up = dist > s.options.button_threshold;
                let scrolling_up = top < s.last_scroll_top;
                let at_bottom = max - top < 1.;
                if scrolling_up && !at_bottom && s.locked {
                    s.locked = false;
                    s.animating = false;
                }
                s.last_scroll_top = top;
            }
            // Upstream's `scrollend`: a scroll that rests near the bottom locks.
            if !s.locked && !s.animating && max - top <= s.options.lock_threshold {
                s.locked = true;
            }
            if max > s.last_max + 0.5 {
                self.follow(s, reduced);
            }
            s.last_max = max;
            if s.animating {
                self.step(s, now);
            }
            (
                s.scrolled_up != was_up || s.locked != was_locked,
                s.animating,
            )
        });
        if refresh {
            window.refresh();
        }
        if frame {
            window.request_animation_frame();
        }
    }

    /// One step of upstream's spring toward the bottom.
    fn step(&self, s: &mut ScrollState, now: Instant) {
        let target = self.max_top();
        if !s.locked || target <= 0. {
            s.animating = false;
            s.last_tick = None;
            s.velocity = 0.;
            return;
        }
        let position = s
            .spring_position
            .unwrap_or_else(|| self.scroll_top())
            .clamp(0., target);
        let diff = target - position;
        if diff.abs() < 0.5 && s.velocity.abs() < 0.1 {
            self.set_scroll_top(target);
            s.last_scroll_top = target;
            s.animating = false;
            s.last_tick = None;
            s.velocity = 0.;
            return;
        }
        let ticks = s.last_tick.map_or(1., |last| {
            now.duration_since(last).as_secs_f32() * 1000. / FRAME_MS
        });
        // One step a frame, as upstream's animation frames: a second read in the same
        // frame waits.
        if ticks <= 0. {
            return;
        }
        s.last_tick = Some(now);
        s.velocity = (s.options.damping * s.velocity + s.options.stiffness * diff) / s.options.mass;
        let next = position + s.velocity * ticks;
        s.spring_position = Some(next);
        self.set_scroll_top(next.clamp(0., target));
        s.last_scroll_top = next.clamp(0., target);
    }
}
