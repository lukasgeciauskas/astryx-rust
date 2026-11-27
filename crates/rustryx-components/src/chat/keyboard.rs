//! ChatLayout's keyboard behavior, upstream `Chat/useChatKeyboard.ts`: the composer sits on
//! the software keyboard, as in Messages on iOS.
//!
//! The layout follows the keyboard height (AST-099 FR3): the dock's sticky offset and the
//! message area's end room take the part of the keyboard over the dock (the lift), so the
//! newest message stays in view. Where the layout guide publishes the keyboard's slide
//! (Chrome on Android, under a coarse pointer), the dock and a transcript following its
//! newest message ride it (FR8), and the layout takes the lift once the ride ends, its
//! translates handing over so nothing moves. The composer region is a keyboard target, so
//! its field moves nothing itself; when the keyboard's Done ends entry, focus parks on the
//! region (FR4) so the next tap starts from the composer. Pans the browser makes after a
//! report are put back.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use gpui::{App, ElementId, Entity, FocusHandle, ScrollHandle, Task, Window};
use rustryx_primitives::keyboard::{
    FocusTarget, GuideSubscription, KeyboardHeightEvent, KeyboardSubscription, KeyboardTargetId,
    KeyboardTransition, PanWatch, follows_keyboard, get_keyboard_height, keyboard_clock,
    keyboard_layout_guide, keyboard_now, keyboard_probe, keyboard_profile_of,
    layout_viewport_height, ride_keyboard, scroll_top_of, subscribe_keyboard_height,
};
use rustryx_primitives::motion::{EngineOptions, MotionEngine, MotionProperty, MotionValues};

use super::stream_scroll::{ChatScrollBehavior, ChatScrollToBottomOptions, ChatStreamScroll};

pub(crate) struct ChatKeyboard {
    target: KeyboardTargetId,
    self_scrolling: bool,
    scroll: Option<ChatStreamScroll>,
    handle: Option<ScrollHandle>,
    /// The lift the layout shows, and the one it showed last frame.
    lift: f64,
    applied: f64,
    /// Set while the guide's ride is under way: the layout waits for it.
    riding: bool,
    ride: u64,
    dock_target: Option<f64>,
    messages_target: Option<f64>,
    /// Whether the transcript followed its newest message, and its scroll, just before the
    /// layout's last change.
    at_end: bool,
    scroll_before: f64,
    dock: MotionEngine,
    messages: MotionEngine,
    region: FocusHandle,
    attached: Option<bool>,
    subscription: Option<KeyboardSubscription>,
    guide: Option<GuideSubscription>,
    pans: Option<PanWatch>,
    ride_end: Option<Task<()>>,
}

/// What the layout draws this frame.
pub(crate) struct ChatKeyboardFrame {
    pub lift: f32,
    pub dock_y: f32,
    pub messages_y: f32,
    pub target: KeyboardTargetId,
    /// The composer region, focusable for the park alone.
    pub region: FocusHandle,
}

fn target_of(id: &ElementId) -> KeyboardTargetId {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    ("chat-composer", id).hash(&mut hasher);
    KeyboardTargetId(hasher.finish())
}

fn engine(cx: &mut App) -> MotionEngine {
    MotionEngine::new(EngineOptions {
        now: Some(keyboard_clock(cx)),
        frames: true,
        ..Default::default()
    })
}

/// Upstream `useChatKeyboard`.
pub(crate) fn use_chat_keyboard(
    id: &ElementId,
    scroll: &ChatStreamScroll,
    handle: &ScrollHandle,
    self_scrolling: bool,
    window: &mut Window,
    cx: &mut App,
) -> ChatKeyboardFrame {
    let state = window.use_keyed_state(
        ElementId::Name(format!("{id}-chat-keyboard").into()),
        cx,
        |_, cx| ChatKeyboard {
            target: target_of(id),
            self_scrolling,
            scroll: None,
            handle: None,
            lift: 0.,
            applied: 0.,
            riding: false,
            ride: 0,
            dock_target: None,
            messages_target: None,
            at_end: false,
            scroll_before: 0.,
            dock: engine(cx),
            messages: engine(cx),
            region: cx.focus_handle().tab_stop(false),
            attached: None,
            subscription: None,
            guide: None,
            pans: None,
            ride_end: None,
        },
    );
    state.update(cx, |s, _| {
        s.self_scrolling = self_scrolling;
        s.scroll = Some(scroll.clone());
        s.handle = Some(handle.clone());
    });
    let touch = follows_keyboard(window, cx);
    if state.read(cx).attached != Some(touch) {
        attach(&state, touch, window, cx);
    }
    keyboard_probe(window, cx);
    hand_over(&state, window, cx);
    let now = keyboard_now(cx);
    let s = state.read(cx);
    for engine in [&s.dock, &s.messages] {
        engine.advance(now);
        engine.frame(now);
        if !engine.is_settled() {
            window.request_animation_frame();
        }
    }
    ChatKeyboardFrame {
        lift: s.lift as f32,
        dock_y: s.dock.get(MotionProperty::Y, None) as f32,
        messages_y: s.messages.get(MotionProperty::Y, None) as f32,
        target: s.target,
        region: s.region.clone(),
    }
}

fn attach(state: &Entity<ChatKeyboard>, touch: bool, window: &mut Window, cx: &mut App) {
    let old = state.update(cx, |s, _| {
        s.attached = Some(touch);
        (s.subscription.take(), s.guide.take(), s.pans.take())
    });
    drop(old);
    // The ride attaches only under a coarse primary pointer, where the guide publishes the
    // keyboard's own slide; elsewhere the layout follows the height alone.
    let guide = (touch && keyboard_profile_of(cx).is_some()).then(|| {
        let weak = state.downgrade();
        keyboard_layout_guide(window, cx).subscribe(window, cx, move |transition, window, cx| {
            if let Some(state) = weak.upgrade() {
                follow(&state, transition, window, cx);
            }
        })
    });
    let height = get_keyboard_height(window, cx);
    let target = state.read(cx).target;
    let pans = touch.then(|| PanWatch::new(height, move |field| field.is_in(target)));
    let weak = state.downgrade();
    let reported = Rc::new(Cell::new(height));
    let subscription = subscribe_keyboard_height(window, cx, true, move |event, window, cx| {
        let Some(state) = weak.upgrade() else { return };
        match event {
            KeyboardHeightEvent::Viewport { height, .. } => {
                // A height change the guide has no ride for (a resize, a rotation) takes
                // its lift at once; the guide's listener came first.
                if *height != reported.get() {
                    reported.set(*height);
                    commit(&state, window, cx);
                }
                let pans = state.update(cx, |s, _| s.pans.take());
                if let Some(mut pans) = pans {
                    pans.report(*height, window, cx);
                    state.update(cx, |s, _| s.pans = Some(pans));
                }
            }
            KeyboardHeightEvent::FocusOut {
                target, related, ..
            } => {
                // Done's park: focus that leaves the composer's text for nothing rests on
                // the composer region, under a finger only.
                let composer = state.read(cx).target;
                if matches!(related, FocusTarget::Nothing)
                    && target
                        .field()
                        .is_some_and(|f| f.is_in(composer) && f.kind.text_entry)
                    && follows_keyboard(window, cx)
                {
                    let region = state.read(cx).region.clone();
                    window.focus(&region, cx);
                }
            }
            KeyboardHeightEvent::FocusIn { .. } => {}
        }
    });
    state.update(cx, |s, _| {
        s.subscription = Some(subscription);
        s.guide = guide;
        s.pans = pans;
    });
    // Mounted under a keyboard that is already up: take its place at once.
    commit(state, window, cx);
}

/// The part of the keyboard over the dock: a sticky dock rests at the root's bottom
/// edge, an outer scroller's at the viewport's.
fn lift_for(state: &Entity<ChatKeyboard>, inset: f64, window: &Window, cx: &App) -> f64 {
    if inset <= 0. {
        return 0.;
    }
    let s = state.read(cx);
    let viewport_bottom = layout_viewport_height(window);
    // A self-scrolling root is its own scroller: the handle's box is the root's.
    let bottom = if s.self_scrolling {
        s.handle
            .as_ref()
            .map(|h| h.bounds())
            .filter(|b| b.size.height > gpui::px(0.))
            .map_or(viewport_bottom, |b| f64::from(b.bottom()))
    } else {
        viewport_bottom
    };
    (bottom - (viewport_bottom - inset)).max(0.)
}

fn is_at_end(state: &Entity<ChatKeyboard>, cx: &App) -> bool {
    state
        .read(cx)
        .scroll
        .as_ref()
        .is_some_and(|s| s.is_locked(cx))
}

/// The layout takes the keyboard height's lift once no ride is under way.
fn commit(state: &Entity<ChatKeyboard>, window: &mut Window, cx: &mut App) {
    if state.read(cx).riding {
        return;
    }
    let height = get_keyboard_height(window, cx);
    let next = lift_for(state, height, window, cx);
    if (next - state.read(cx).lift).abs() < 0.5 {
        return;
    }
    let at_end = is_at_end(state, cx);
    state.update(cx, |s, _| {
        s.at_end = at_end;
        s.scroll_before = s.handle.as_ref().map_or(0., scroll_top_of);
        s.lift = next;
    });
    window.refresh();
}

/// The frame after the layout took a new lift: the translates take the difference, so
/// nothing moves on screen, and the newest message stays in view.
fn hand_over(state: &Entity<ChatKeyboard>, window: &mut Window, cx: &mut App) {
    let (moved, dock_target, messages_target, at_end, before) = {
        let s = state.read(cx);
        (
            s.lift - s.applied,
            s.dock_target,
            s.messages_target,
            s.at_end,
            s.scroll_before,
        )
    };
    if moved.abs() < 0.5 {
        return;
    }
    state.update(cx, |s, _| {
        s.applied = s.lift;
        s.dock_target = None;
        s.messages_target = None;
        // A ride that ended hands its translate (its target) to the layout.
        if let Some(target) = dock_target {
            s.dock.set(MotionValues::new().y(target + moved));
        }
        if messages_target.is_some() {
            s.messages.set(MotionValues::new().y(0.));
        }
    });
    let scroll = state.read(cx).scroll.clone();
    if at_end && let Some(scroll) = scroll {
        scroll.scroll_to_bottom(
            ChatScrollToBottomOptions {
                behavior: ChatScrollBehavior::Instant,
            },
            window,
            cx,
        );
    }
    if let Some(target) = messages_target {
        let shifted = state.read(cx).handle.as_ref().map_or(0., scroll_top_of) - before;
        if (target + shifted).abs() >= 0.5 {
            state.update(cx, |s, _| {
                s.messages.set(MotionValues::new().y(target + shifted))
            });
        }
    }
    window.refresh();
}

/// The ride: the dock goes where the guide says the keyboard will be, from the keyboard's
/// start, relative to the layout it rests in now, on the keyboard's own motion.
fn follow(
    state: &Entity<ChatKeyboard>,
    transition: &KeyboardTransition,
    window: &mut Window,
    cx: &mut App,
) {
    let target = state.read(cx).lift - lift_for(state, transition.to, window, cx);
    let at_end = is_at_end(state, cx);
    let now = keyboard_now(cx);
    state.update(cx, |s, _| {
        s.dock_target = Some(target);
        s.messages_target = at_end.then_some(target);
    });
    if !transition.animated {
        state.update(cx, |s, _| {
            s.ride += 1;
            s.riding = false;
            s.dock.set(MotionValues::new().y(target));
            if at_end {
                s.messages.set(MotionValues::new().y(target));
            }
        });
        commit(state, window, cx);
        return;
    }
    let ride = state.update(cx, |s, _| {
        s.riding = true;
        s.ride += 1;
        let y = MotionValues::new().y(target);
        ride_keyboard(&s.dock, y, transition, now, false);
        if at_end {
            ride_keyboard(&s.messages, y, transition, now, false);
        }
        s.ride
    });
    let end = transition.start + transition.curve.map_or(0., |c| c.duration_ms);
    let wait = (end - now).max(0.);
    let weak = state.downgrade();
    let task = window.spawn(cx, async move |cx| {
        cx.background_executor()
            .timer(Duration::from_secs_f64(wait / 1000.))
            .await;
        cx.update(|window, cx| {
            let Some(state) = weak.upgrade() else { return };
            // A newer ride took over: it commits when it ends.
            if state.read(cx).ride != ride {
                return;
            }
            state.update(cx, |s, _| s.riding = false);
            commit(&state, window, cx);
        })
        .ok();
    });
    state.update(cx, |s, _| s.ride_end = Some(task));
    window.refresh();
}
