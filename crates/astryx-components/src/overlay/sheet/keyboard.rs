//! A Tall BottomSheet riding the virtual keyboard: upstream `keyboard/useMobileKeyboard.ts`
//! as `BottomSheetPanel.tsx` uses it (`move: 'ride'`, `inset: true`, AST-099 FR3-FR5,
//! FR8).
//!
//! While a presented Tall sheet is fully expanded and the keyboard covers its body, the
//! keyboard's inset goes at the end of the body's content (so its last field can scroll
//! above the keyboard), the focused field is scrolled into the part of the body the
//! keyboard leaves, 48 px clear of it for the suggestion strip, and the body's scroll goes
//! back once the keyboard has gone. Where the guide publishes the keyboard's slide (Chrome
//! on Android, a coarse pointer), the content rides it: the body lands at once and its
//! content rides up from where it was; through the hide the inset holds while the content
//! rides down to where the body's scroll rests, and at rest the inset drops, the scroll
//! goes back and the translate clears in one frame. Without a slide (iOS), a blur that takes
//! the keyboard down rides the content on UIKit's keyboard spring while the inset holds.
//! The sheet is a keyboard target: a field inside moves nothing itself.

use std::time::Duration;

use astryx_primitives::keyboard::{
    FIELD_CLEARANCE_AT_REPORT, GuideSubscription, KEYBOARD_EPSILON, KeyboardHeightEvent,
    KeyboardSubscription, KeyboardTargetId, KeyboardTransition, KeyboardTransitionKind,
    distance_into_band, focused_field, get_keyboard_height, keyboard_clock, keyboard_layout_guide,
    keyboard_now, keyboard_probe, keyboard_profile_of, layout_viewport_height, ride_keyboard,
    scroll_range_of, scroll_top_of, scroll_top_to, subscribe_keyboard_height,
};
use astryx_primitives::motion::uikit::springs;
use astryx_primitives::motion::{
    AnimateOptions, EngineOptions, MotionEngine, MotionProperty, MotionSpring, MotionValues,
    SpringTransition,
};
use gpui::{App, ElementId, Entity, ScrollHandle, Task, Window};

/// Upstream `MOBILE_KEYBOARD_BOTTOM_CLEARANCE`: the room a sheet's focused field keeps
/// above the keyboard for its suggestion strip, px.
pub(crate) const MOBILE_KEYBOARD_BOTTOM_CLEARANCE: f64 = 48.;

/// What the hook is given (upstream `MobileKeyboardOptions`).
#[derive(Clone, Copy)]
pub(crate) struct SheetKeyboardOptions {
    /// A presented Tall sheet.
    pub enabled: bool,
    /// At its full height, where it reveals fields.
    pub expanded: bool,
    /// Under a coarse primary pointer: the ride attaches.
    pub touch: bool,
}

/// A hide the content rides down through, holding the inset.
enum Held {
    /// On the guide's slide; lands at its end.
    Slide { landed: bool, _timer: Task<()> },
    /// On UIKit's keyboard spring, from the blur.
    Spring,
}

pub(crate) struct SheetKeyboard {
    target: KeyboardTargetId,
    /// The sheet's own focus stop: focus that leaves a field for nothing parks here.
    surface: Option<gpui::FocusHandle>,
    body: Option<ScrollHandle>,
    options: SheetKeyboardOptions,
    attached: Option<(bool, bool)>,
    subscription: Option<KeyboardSubscription>,
    guide: Option<GuideSubscription>,
    /// The inset laid out at the end of the body's content, px.
    inset: f64,
    has_layout: bool,
    retain: bool,
    /// The keyboard's top and the body's scroll when the keyboard came up.
    obstruction_top_at_keyboard: Option<f64>,
    scroll_at_keyboard: Option<f64>,
    /// The body's bottom while the keyboard covered it.
    body_bottom: Option<f64>,
    last_height: f64,
    held: Option<Held>,
    /// The content's ride (y), on the keyboard clock.
    content: MotionEngine,
}

/// What the panel lays out this frame.
pub(crate) struct SheetKeyboardFrame {
    pub inset: f32,
    pub content_y: f32,
    pub target: KeyboardTargetId,
}

fn target_of(id: &ElementId) -> KeyboardTargetId {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    id.hash(&mut hasher);
    KeyboardTargetId(hasher.finish())
}

/// Upstream `useMobileKeyboard(panelRef, {move: 'ride', inset: true, ...})`.
pub(crate) fn use_sheet_keyboard(
    id: &ElementId,
    body: &ScrollHandle,
    surface: &gpui::FocusHandle,
    options: SheetKeyboardOptions,
    window: &mut Window,
    cx: &mut App,
) -> SheetKeyboardFrame {
    let target = target_of(id);
    let state = window.use_keyed_state(
        ElementId::Name(format!("{id}-sheet-keyboard").into()),
        cx,
        move |_, cx| SheetKeyboard {
            target,
            surface: None,
            body: None,
            options,
            attached: None,
            subscription: None,
            guide: None,
            inset: 0.,
            has_layout: false,
            retain: false,
            obstruction_top_at_keyboard: None,
            scroll_at_keyboard: None,
            body_bottom: None,
            last_height: 0.,
            held: None,
            content: MotionEngine::new(EngineOptions {
                now: Some(keyboard_clock(cx)),
                frames: true,
                ..Default::default()
            }),
        },
    );
    state.update(cx, |s, _| {
        s.body = Some(body.clone());
        s.surface = Some(surface.clone());
        s.options = options;
    });
    let key = (options.enabled, options.touch);
    if state.read(cx).attached != Some(key) {
        attach(&state, key, window, cx);
    }
    if options.enabled {
        keyboard_probe(window, cx);
    }
    let now = keyboard_now(cx);
    let s = state.read(cx);
    s.content.advance(now);
    s.content.frame(now);
    if !s.content.is_settled() {
        window.request_animation_frame();
    }
    SheetKeyboardFrame {
        inset: s.inset as f32,
        content_y: s.content.get(MotionProperty::Y, None) as f32,
        target,
    }
}

fn attach(state: &Entity<SheetKeyboard>, key: (bool, bool), window: &mut Window, cx: &mut App) {
    let (enabled, touch) = key;
    let (old_sub, old_guide) = state.update(cx, |s, _| {
        s.attached = Some(key);
        (s.subscription.take(), s.guide.take())
    });
    drop((old_sub, old_guide));
    if !enabled {
        state.update(cx, |s, _| {
            s.inset = 0.;
            s.has_layout = false;
            s.retain = false;
            s.held = None;
            s.content.set(MotionValues::new().y(0.));
        });
        return;
    }
    // The guide first, so it publishes a report's transition before the reveal reads it.
    let guide = (touch && keyboard_profile_of(cx).is_some()).then(|| {
        let weak = state.downgrade();
        keyboard_layout_guide(window, cx).subscribe(window, cx, move |transition, window, cx| {
            if let Some(state) = weak.upgrade() {
                on_transition(&state, transition, window, cx);
            }
        })
    });
    let weak = state.downgrade();
    let subscription = subscribe_keyboard_height(window, cx, true, move |event, window, cx| {
        if let Some(state) = weak.upgrade() {
            on_event(&state, event, window, cx);
        }
    });
    let height = get_keyboard_height(window, cx);
    state.update(cx, |s, _| {
        s.subscription = Some(subscription);
        s.guide = guide;
        s.last_height = height;
    });
    reveal(state, false, window, cx);
}

fn inside(
    state: &Entity<SheetKeyboard>,
    event_target: &astryx_primitives::keyboard::FocusTarget,
    cx: &App,
) -> bool {
    let target = state.read(cx).target;
    event_target.field().is_some_and(|f| f.is_in(target))
}

fn on_event(
    state: &Entity<SheetKeyboard>,
    event: &KeyboardHeightEvent,
    window: &mut Window,
    cx: &mut App,
) {
    match event {
        KeyboardHeightEvent::Viewport { height, .. } => {
            let changed = *height != state.read(cx).last_height;
            state.update(cx, |s, _| s.last_height = *height);
            reveal(state, changed, window, cx);
        }
        KeyboardHeightEvent::FocusIn { target, .. } => {
            if inside(state, target, cx) {
                reveal(state, false, window, cx);
            }
        }
        KeyboardHeightEvent::FocusOut {
            target, related, ..
        } => {
            if inside(state, target, cx) {
                // Keep the added scroll range while the keyboard animates away.
                state.update(cx, |s, _| s.retain = s.has_layout);
                park(state, target, related, window, cx);
                reveal(state, false, window, cx);
            }
        }
    }
}

/// Upstream `refuseFocusReveal`'s `park`: the keyboard's Done leaves focus on nothing.
/// While the sheet is open and fully expanded, under a coarse primary pointer, focus parks
/// on the sheet instead, so the next tap on the same field is again a focus move the sheet
/// sees (AST-099 FR4).
fn park(
    state: &Entity<SheetKeyboard>,
    target: &astryx_primitives::keyboard::FocusTarget,
    related: &astryx_primitives::keyboard::FocusTarget,
    window: &mut Window,
    cx: &mut App,
) {
    let s = state.read(cx);
    let options = s.options;
    let Some(surface) = s.surface.clone() else {
        return;
    };
    if !matches!(related, astryx_primitives::keyboard::FocusTarget::Nothing)
        || !target.is_text_entry()
        || !(options.enabled && options.expanded && options.touch)
        || !astryx_primitives::keyboard::follows_keyboard(window, cx)
    {
        return;
    }
    window.focus(&surface, cx);
}

fn obstruction_top(window: &mut Window, cx: &mut App) -> f64 {
    layout_viewport_height(window) - get_keyboard_height(window, cx)
}

/// How far the body runs under the keyboard's top: an overlap under a px is rounding.
fn overlap_of(body_bottom: f64, obstruction_top: f64) -> f64 {
    let overlap = body_bottom - obstruction_top;
    if overlap > KEYBOARD_EPSILON {
        overlap
    } else {
        0.
    }
}

fn recovered(window: &mut Window, cx: &mut App) -> bool {
    get_keyboard_height(window, cx) <= 0.5
}

fn clear_layout(state: &Entity<SheetKeyboard>, window: &mut Window, cx: &mut App) {
    state.update(cx, |s, _| {
        s.inset = 0.;
        if let (Some(top), Some(body)) = (s.scroll_at_keyboard, &s.body) {
            scroll_top_to(body, top);
        }
        s.obstruction_top_at_keyboard = None;
        s.scroll_at_keyboard = None;
        s.body_bottom = None;
        s.has_layout = false;
        s.retain = false;
    });
    window.refresh();
}

/// The keyboard's show on its slide now (a report reads one whose slide has ended too).
fn show_on_slide(
    state: &Entity<SheetKeyboard>,
    at_report: bool,
    window: &Window,
    cx: &mut App,
) -> Option<KeyboardTransition> {
    state.read(cx).guide.as_ref()?;
    let transition = keyboard_layout_guide(window, cx).transition();
    let curve = transition.curve?;
    (transition.kind == KeyboardTransitionKind::Show
        && transition.animated
        && (at_report || keyboard_now(cx) < transition.start + curve.duration_ms))
        .then_some(transition)
}

/// Scrolls the focused field into the part of the body the keyboard leaves visible.
fn scroll_into_safe_area(
    state: &Entity<SheetKeyboard>,
    field: gpui::Bounds<gpui::Pixels>,
    ride: Option<KeyboardTransition>,
    window: &mut Window,
    cx: &mut App,
) {
    let Some(body) = state.read(cx).body.clone() else {
        return;
    };
    let bounds = body.bounds();
    let top_of = obstruction_top(window, cx);
    let overlap = overlap_of(f64::from(bounds.bottom()), top_of);
    let clearance = if overlap > 0. {
        MOBILE_KEYBOARD_BOTTOM_CLEARANCE
    } else {
        0.
    };
    let safe_top = f64::from(bounds.top());
    let safe_bottom = f64::from(bounds.bottom()).min(top_of - clearance);
    // A ride's translate is not where the layout puts the field.
    let ride_offset = state.read(cx).content.get(MotionProperty::Y, None);
    let distance = distance_into_band(
        f64::from(field.top()) - ride_offset,
        f64::from(field.bottom()) - ride_offset,
        safe_top,
        safe_bottom,
    );
    if distance == 0. {
        return;
    }
    let before = scroll_top_of(&body);
    // The inset lays out in the frame this scroll is drawn in, so the range it adds is
    // there: the body's own range plus the inset.
    let range = scroll_range_of(&body) + state.read(cx).inset;
    let after = (before + distance).clamp(0., range.max(before));
    scroll_top_to(&body, after);
    let moved = after - before;
    if let Some(ride) = ride
        && moved != 0.
    {
        // The body lands at once and its content rides the keyboard's slide up from the
        // slide's start, no lower than where the field clears the keyboard now.
        let within = if distance > 0. {
            Some((top_of - safe_bottom - FIELD_CLEARANCE_AT_REPORT).max(0.))
        } else {
            None
        };
        let now = keyboard_now(cx);
        state.update(cx, |s, _| {
            let from = s.content.get(MotionProperty::Y, Some(ride.start)) + moved;
            let y = within.map_or(from, |w| from.min(w));
            s.content.set(MotionValues::new().y(y));
            ride_keyboard(
                &s.content,
                MotionValues::new().y(0.),
                &ride,
                now,
                within.is_some(),
            );
        });
    }
    window.refresh();
}

fn resting_scroll(state: &Entity<SheetKeyboard>, cx: &App) -> f64 {
    let s = state.read(cx);
    let Some(body) = &s.body else { return 0. };
    let max_at_rest = (scroll_range_of(body) - s.inset).max(0.);
    max_at_rest.min(s.scroll_at_keyboard.unwrap_or_else(|| scroll_top_of(body)))
}

fn reveal(state: &Entity<SheetKeyboard>, at_report: bool, window: &mut Window, cx: &mut App) {
    if !state.read(cx).options.enabled {
        return;
    }
    // Through a held hide, the layout holds until the end.
    if state.read(cx).held.is_some() {
        let focused = focused_field(window, cx);
        let target = state.read(cx).target;
        let back = focused.is_some_and(|f| f.is_in(target) && f.kind.brings_up_keyboard());
        let is_recovered = recovered(window, cx);
        let spring = matches!(&state.read(cx).held, Some(Held::Spring));
        let landed = matches!(&state.read(cx).held, Some(Held::Slide { landed: true, .. }));
        match () {
            _ if spring && back => {
                // A field that takes focus first keeps the keyboard up: ride back up.
                state.update(cx, |s, _| {
                    s.held = None;
                    s.content.animate(
                        MotionValues::new().y(0.),
                        SpringTransition::new(MotionSpring::inline(springs::KEYBOARD)),
                        AnimateOptions::default(),
                    );
                });
            }
            _ if spring && is_recovered => {
                // Gone: the inset drops and the scroll goes back with the translate.
                state.update(cx, |s, _| s.held = None);
                commit_rest(state, window, cx);
                return;
            }
            _ if landed && is_recovered => {
                state.update(cx, |s, _| s.held = None);
                commit_rest(state, window, cx);
                return;
            }
            _ => return,
        }
    }
    let top_of = obstruction_top(window, cx);
    let target = state.read(cx).target;
    let expanded = state.read(cx).options.expanded;
    let focused = focused_field(window, cx).filter(|f| f.is_in(target) && f.kind.text_entry);
    if state.read(cx).has_layout && recovered(window, cx) {
        clear_layout(state, window, cx);
        if expanded && let Some(field) = focused {
            scroll_into_safe_area(state, field.bounds, None, window, cx);
        }
        return;
    }
    let Some(field) = focused.filter(|_| expanded) else {
        let (retain, bottom) = {
            let s = state.read(cx);
            (s.retain, s.body_bottom)
        };
        if retain && let Some(bottom) = bottom {
            // Keep the inset while the keyboard goes.
            let inset = (bottom - (top_of - MOBILE_KEYBOARD_BOTTOM_CLEARANCE)).max(0.);
            if overlap_of(bottom, top_of) == 0. {
                clear_layout(state, window, cx);
                return;
            }
            state.update(cx, |s, _| s.inset = inset.max(s.inset));
            // Focus left for a control that brings up no keyboard: the keyboard goes
            // down now. Without the guide's slide, the content rides its spring.
            let keeps = focused_field(window, cx).is_some_and(|f| f.kind.brings_up_keyboard());
            if expanded && !keeps {
                hold_through_unguided_hide(state, cx);
            }
            window.refresh();
            return;
        }
        if state.read(cx).has_layout || state.read(cx).inset != 0. {
            clear_layout(state, window, cx);
        }
        return;
    };
    let Some(body) = state.read(cx).body.clone() else {
        return;
    };
    let bounds = body.bounds();
    let body_bottom = f64::from(bounds.bottom());
    // An iOS keyboard dismissed by a drag can leave its accessory bar: the body goes
    // back to its scroll from before the keyboard as soon as the viewport grows.
    let grew = state
        .read(cx)
        .obstruction_top_at_keyboard
        .is_some_and(|t| top_of > t + 0.5);
    if state.read(cx).has_layout && grew {
        state.update(cx, |s, _| {
            if let (Some(top), Some(body)) = (s.scroll_at_keyboard, &s.body) {
                scroll_top_to(body, top);
            }
            s.obstruction_top_at_keyboard = Some(top_of);
        });
    }
    let overlap = overlap_of(body_bottom, top_of);
    let clearance = if overlap > 0. {
        MOBILE_KEYBOARD_BOTTOM_CLEARANCE
    } else {
        0.
    };
    state.update(cx, |s, _| {
        s.retain = false;
        if overlap > 0. && !s.has_layout {
            s.obstruction_top_at_keyboard = Some(top_of);
            s.scroll_at_keyboard = Some(scroll_top_of(&body));
        }
        s.has_layout = overlap > 0.;
        s.body_bottom = (overlap > 0.).then_some(body_bottom);
        s.inset = if overlap > 0. {
            (body_bottom - (top_of - clearance)).max(0.)
        } else {
            0.
        };
    });
    let ride = if overlap > 0. {
        show_on_slide(state, at_report, window, cx)
    } else {
        None
    };
    scroll_into_safe_area(state, field.bounds, ride, window, cx);
    window.refresh();
}

/// At rest: the inset drops, the scroll goes back and the translate clears together, so
/// nothing moves.
fn commit_rest(state: &Entity<SheetKeyboard>, window: &mut Window, cx: &mut App) {
    clear_layout(state, window, cx);
    state.update(cx, |s, _| s.content.set(MotionValues::new().y(0.)));
    reveal(state, false, window, cx);
}

/// Without the guide (iOS Safari, Firefox for Android): a blur that takes the keyboard
/// down rides the content to the body's resting scroll on UIKit's keyboard spring, while
/// the inset stays (FR3).
fn hold_through_unguided_hide(state: &Entity<SheetKeyboard>, cx: &mut App) {
    let s = state.read(cx);
    if s.guide.is_some() || !s.options.touch || s.held.is_some() {
        return;
    }
    let Some(body) = s.body.clone() else { return };
    let drop = scroll_top_of(&body) - resting_scroll(state, cx);
    if drop == 0. {
        return;
    }
    state.update(cx, |s, _| {
        s.held = Some(Held::Spring);
        s.content.animate(
            MotionValues::new().y(drop),
            SpringTransition::new(MotionSpring::inline(springs::KEYBOARD)),
            AnimateOptions::default(),
        );
    });
}

/// The guide's transitions: a hide holds the inset through its slide; a show during a held
/// hide rides the content back (FR8).
fn on_transition(
    state: &Entity<SheetKeyboard>,
    transition: &KeyboardTransition,
    window: &mut Window,
    cx: &mut App,
) {
    let Some(curve) = transition.curve else {
        return;
    };
    let now = keyboard_now(cx);
    if transition.kind == KeyboardTransitionKind::Hide {
        if !(state.read(cx).has_layout && transition.animated) {
            return;
        }
        let Some(body) = state.read(cx).body.clone() else {
            return;
        };
        let drop = scroll_top_of(&body) - resting_scroll(state, cx);
        state.update(cx, |s, _| {
            if drop != 0. {
                ride_keyboard(
                    &s.content,
                    MotionValues::new().y(drop),
                    transition,
                    now,
                    false,
                );
            }
        });
        let wait = (transition.start + curve.duration_ms - now).max(0.);
        let weak = state.downgrade();
        let timer = window.spawn(cx, async move |cx| {
            cx.background_executor()
                .timer(Duration::from_secs_f64(wait / 1000.))
                .await;
            cx.update(|window, cx| {
                let Some(state) = weak.upgrade() else { return };
                // Landed: at rest once the keyboard reports it has gone (a busy page reads
                // that report after the slide; until then the content stays).
                if recovered(window, cx) || !state.read(cx).has_layout {
                    state.update(cx, |s, _| s.held = None);
                    commit_rest(&state, window, cx);
                } else {
                    state.update(cx, |s, _| {
                        if let Some(Held::Slide { landed, .. }) = &mut s.held {
                            *landed = true;
                        }
                    });
                }
            })
            .ok();
        });
        state.update(cx, |s, _| {
            s.held = Some(Held::Slide {
                landed: false,
                _timer: timer,
            })
        });
        return;
    }
    if state.read(cx).held.is_some() {
        // The keyboard comes back before it is down: the inset never left, so the content
        // rides back to its place with it.
        state.update(cx, |s, _| {
            s.held = None;
            ride_keyboard(
                &s.content,
                MotionValues::new().y(0.),
                transition,
                now,
                false,
            );
        });
        window.refresh();
    }
}
