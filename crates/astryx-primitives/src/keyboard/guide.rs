//! Upstream `keyboard/keyboardLayoutGuide.ts`: the web's `UIKeyboardLayoutGuide`, one per
//! window, fed by the keyboard module: where the software keyboard covers the layout
//! viewport and how it moves. Touch presentations read it to time a ride on top of the
//! layout, which follows the keyboard height itself.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::{Rc, Weak};
use std::time::Duration;

use gpui::{App, Global, Task, Window, WindowId};

use super::height::{
    FocusTarget, KeyboardHeightEvent, KeyboardSubscription, focused_field, get_keyboard_height,
    has_virtual_keyboard, keyboard_now, subscribe_keyboard_height,
};
use super::model::{KeyboardModel, KeyboardProfile, KeyboardTransition, KeyboardTransitionKind};

/// Extra time after a predicted transition's end before checking it, ms.
const VERIFY_SLACK_MS: f64 = 150.;
/// How long after a field gains focus Chrome may still report the keyboard gone for a
/// moment, ms (measured: back within 50 ms of the move).
const FOCUS_HANDOFF_MS: f64 = 150.;

/// Upstream `keyboardProfileOf`: `Chromium` where the browser has the VirtualKeyboard
/// API, else none. Without the profile, no surface rides the guide. A capability, never
/// the user agent.
pub fn keyboard_profile_of(cx: &App) -> Option<KeyboardProfile> {
    has_virtual_keyboard(cx).then_some(KeyboardProfile::Chromium)
}

/// A guide listener: each transition, inside the report that revealed it.
pub type GuideListener = Rc<dyn Fn(&KeyboardTransition, &mut Window, &mut App)>;

struct GuideInner {
    profile: Option<KeyboardProfile>,
    model: KeyboardModel,
    listeners: Vec<(u64, GuideListener)>,
    next_id: u64,
    subscription: Option<KeyboardSubscription>,
    verify: Option<Task<()>>,
    handoff: Option<Task<()>>,
    /// When a field that brings up a keyboard last gained focus.
    keyboard_focus_at: f64,
}

/// A window's keyboard layout guide (upstream `KeyboardLayoutGuide`). Cheap to clone.
#[derive(Clone)]
pub struct KeyboardLayoutGuide(Rc<RefCell<GuideInner>>);

#[derive(Default)]
struct Guides(HashMap<WindowId, KeyboardLayoutGuide>);

impl Global for Guides {}

/// Upstream `getKeyboardLayoutGuide`: the window's one guide, created on first use. It
/// subscribes to the keyboard module only while something subscribes to it, owns no
/// listener and measures nothing. It feeds its model the module's reports and the focus
/// moves that start or turn back a hide, and hands each transition to its subscribers.
pub fn keyboard_layout_guide(window: &Window, cx: &mut App) -> KeyboardLayoutGuide {
    let id = window.window_handle().window_id();
    let guides = cx.default_global::<Guides>();
    if let Some(guide) = guides.0.get(&id) {
        return guide.clone();
    }
    let guide = KeyboardLayoutGuide(Rc::new(RefCell::new(GuideInner {
        profile: None,
        model: KeyboardModel::new(0.),
        listeners: Vec::new(),
        next_id: 0,
        subscription: None,
        verify: None,
        handoff: None,
        keyboard_focus_at: f64::NEG_INFINITY,
    })));
    cx.default_global::<Guides>().0.insert(id, guide.clone());
    guide
}

/// A subscription to the guide; dropping it unsubscribes, and the last one detaches the
/// guide from the keyboard module.
pub struct GuideSubscription {
    guide: Weak<RefCell<GuideInner>>,
    id: u64,
}

impl Drop for GuideSubscription {
    fn drop(&mut self) {
        let Some(guide) = self.guide.upgrade() else {
            return;
        };
        let detached = {
            let Ok(mut g) = guide.try_borrow_mut() else {
                return;
            };
            g.listeners.retain(|(id, _)| *id != self.id);
            if g.listeners.is_empty() {
                g.verify = None;
                g.handoff = None;
                g.subscription.take()
            } else {
                None
            }
        };
        drop(detached);
    }
}

impl KeyboardLayoutGuide {
    /// Whose keyboard schedule the guide reads.
    pub fn profile(&self) -> Option<KeyboardProfile> {
        self.0.borrow().profile
    }

    /// The inset the keyboard rests at (or is heading to), px.
    pub fn inset(&self) -> f64 {
        self.0.borrow().model.inset()
    }

    /// The latest transition.
    pub fn transition(&self) -> KeyboardTransition {
        self.0.borrow().model.transition()
    }

    /// The inset presented at `time` on the keyboard clock, px.
    pub fn presentation(&self, time: f64) -> f64 {
        self.0.borrow().model.presentation(time)
    }

    /// Calls `listener` with every transition until the subscription drops.
    pub fn subscribe(
        &self,
        window: &mut Window,
        cx: &mut App,
        listener: impl Fn(&KeyboardTransition, &mut Window, &mut App) + 'static,
    ) -> GuideSubscription {
        let attach = {
            let mut g = self.0.borrow_mut();
            let id = g.next_id;
            g.next_id += 1;
            g.listeners.push((id, Rc::new(listener)));
            (g.subscription.is_none(), id)
        };
        if attach.0 {
            self.attach(window, cx);
        }
        GuideSubscription {
            guide: Rc::downgrade(&self.0),
            id: attach.1,
        }
    }

    fn attach(&self, window: &mut Window, cx: &mut App) {
        let profile = super::guide::keyboard_profile_of(cx);
        let height = get_keyboard_height(window, cx);
        {
            let mut g = self.0.borrow_mut();
            g.profile = profile;
            g.model = KeyboardModel::new(height);
            g.keyboard_focus_at = f64::NEG_INFINITY;
        }
        let weak = Rc::downgrade(&self.0);
        let subscription = subscribe_keyboard_height(window, cx, true, move |event, window, cx| {
            if let Some(inner) = weak.upgrade() {
                KeyboardLayoutGuide(inner).on_report(event, window, cx);
            }
        });
        self.0.borrow_mut().subscription = Some(subscription);
    }

    /// Whether focusing `target` keeps an input view where the keyboard is. On iOS a
    /// select's or a date field's picker takes the keyboard's place; Chrome on Android
    /// opens it over the page, and the keyboard goes away.
    fn keeps_input_view(&self, target: &FocusTarget) -> bool {
        let chromium = self.profile() == Some(KeyboardProfile::Chromium);
        target
            .field()
            .is_some_and(|f| f.kind.brings_up_keyboard() && !(chromium && f.kind.picker_over_page))
    }

    /// Whether a hide predicted from a blur is under way with nothing focused that can keep
    /// a keyboard up: then only the report of the keyboard gone counts. (GPUI has no
    /// frames, shadow roots or custom elements that hide a field, upstream `hidesField`.)
    fn awaits_hide(&self, window: &mut Window, cx: &mut App) -> bool {
        let transition = self.transition();
        if transition.kind != KeyboardTransitionKind::Hide || !transition.predicted {
            return false;
        }
        let active = focused_field(window, cx).map_or(FocusTarget::Nothing, FocusTarget::Field);
        !self.keeps_input_view(&active)
    }

    fn publish(&self, transition: Option<KeyboardTransition>, window: &mut Window, cx: &mut App) {
        let Some(transition) = transition else { return };
        if transition.predicted {
            self.schedule_verify(&transition, window, cx);
        }
        let listeners: Vec<GuideListener> = self
            .0
            .borrow()
            .listeners
            .iter()
            .map(|(_, l)| l.clone())
            .collect();
        for listener in listeners {
            listener(&transition, window, cx);
        }
    }

    fn schedule_verify(&self, transition: &KeyboardTransition, window: &mut Window, cx: &mut App) {
        let due =
            transition.start + transition.curve.map_or(0., |c| c.duration_ms) + VERIFY_SLACK_MS;
        let wait = (due - keyboard_now(cx)).max(0.);
        let weak = Rc::downgrade(&self.0);
        let task = window.spawn(cx, async move |cx| {
            cx.background_executor()
                .timer(Duration::from_secs_f64(wait / 1000.))
                .await;
            cx.update(|window, cx| {
                let Some(inner) = weak.upgrade() else { return };
                let guide = KeyboardLayoutGuide(inner);
                guide.0.borrow_mut().verify = None;
                let inset = get_keyboard_height(window, cx);
                if inset > 0.5 && guide.awaits_hide(window, cx) {
                    return;
                }
                let now = keyboard_now(cx);
                let next = guide.0.borrow_mut().model.verify(Some(inset), now);
                guide.publish(next, window, cx);
            })
            .ok();
        });
        self.0.borrow_mut().verify = Some(task);
    }

    fn report(
        &self,
        height: f64,
        time: f64,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<KeyboardTransition> {
        self.0.borrow_mut().handoff = None;
        let (profile, inset, settled) = {
            let g = self.0.borrow();
            (
                g.profile,
                g.model.inset(),
                g.keyboard_focus_at + FOCUS_HANDOFF_MS,
            )
        };
        let active = focused_field(window, cx).map_or(FocusTarget::Nothing, FocusTarget::Field);
        if profile == Some(KeyboardProfile::Chromium)
            && height < 0.5
            && inset >= 0.5
            && time < settled
            && self.keeps_input_view(&active)
        {
            // Focus just moved to another field: Chrome reports the keyboard gone before
            // it reports it back. Hold the hide until the move settles, then believe
            // whatever the keyboard is doing.
            let weak = Rc::downgrade(&self.0);
            let wait = settled - time;
            let task = window.spawn(cx, async move |cx| {
                cx.background_executor()
                    .timer(Duration::from_secs_f64(wait / 1000.))
                    .await;
                cx.update(|window, cx| {
                    let Some(inner) = weak.upgrade() else { return };
                    let guide = KeyboardLayoutGuide(inner);
                    guide.0.borrow_mut().handoff = None;
                    let height = get_keyboard_height(window, cx);
                    let next = guide.0.borrow_mut().model.report(Some(height), settled);
                    guide.publish(next, window, cx);
                })
                .ok();
            });
            self.0.borrow_mut().handoff = Some(task);
            return None;
        }
        if height > 0.5 && self.awaits_hide(window, cx) {
            return None;
        }
        self.0.borrow_mut().model.report(Some(height), time)
    }

    fn on_report(&self, event: &KeyboardHeightEvent, window: &mut Window, cx: &mut App) {
        match event {
            KeyboardHeightEvent::Viewport { height, time } => {
                let next = self.report(*height, *time, window, cx);
                self.publish(next, window, cx);
            }
            KeyboardHeightEvent::FocusIn { target, time, .. } => {
                if self.keeps_input_view(target) {
                    let next = {
                        let mut g = self.0.borrow_mut();
                        g.keyboard_focus_at = *time;
                        g.model.focused(*time)
                    };
                    self.publish(next, window, cx);
                }
            }
            KeyboardHeightEvent::FocusOut {
                target,
                related,
                time,
                ..
            } => {
                if self.keeps_input_view(target) && !self.keeps_input_view(related) {
                    let next = self.0.borrow_mut().model.blurred(*time);
                    self.publish(next, window, cx);
                }
            }
        }
    }
}
