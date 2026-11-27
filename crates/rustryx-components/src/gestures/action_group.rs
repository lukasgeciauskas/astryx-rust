//! The group press tracker, upstream `hooks/actionGroupTracker.ts`,
//! `hooks/useActionGroupTracking.ts` and `hooks/useGroupPressTracking.ts`: a held
//! finger that slides across a group of actions, which UIKit treats as one control.
//!
//! - The action under the finger highlights (the press paint); between or outside the
//!   actions nothing does. A one-axis group (`tracks_along`) reads the finger on its
//!   center line instead, so a drift off the group keeps its action, and Escape cancels.
//! - The lift runs the action under the finger, and its highlight fades on the press
//!   controller's clock (`press_fade_ms`). A cancel, or a second finger, cuts it and
//!   runs nothing.
//! - Layout is read only as a press begins (the members' boxes drawn last frame).
//!
//! [`use_action_group_tracking`] drives it with a long press (`highlight_onset_ms`;
//! inside a scroller a finger waits the content-touch delay and fails past the slop).
//! A [`PressGroup`] is provided to the group's content: each Button inside registers as
//! a member, takes its press paint from the tracker (the tracker is its only painter
//! under a finger), and runs its own activation at the lift.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use gpui::{
    AnyElement, App, ElementId, FocusHandle, IntoElement, ParentElement, SharedString, Styled,
    Window, canvas,
};
use rustryx_primitives::gestures::hit_area::{HitRect, MIN_TOUCH_TARGET, resolve_hit_target};
use rustryx_primitives::gestures::host::{
    GestureAttachment, after_gesture, gesture_area, gesture_host,
};
use rustryx_primitives::gestures::{
    GestureState, LongPressGestureRecognizer, LongPressOptions, PointerKind, RecognizerOptions,
};
use rustryx_primitives::motion::uikit::gestures::press as PRESS;
use rustryx_primitives::{provide, use_context};
use web_time::Instant;

// ---------------------------------------------------------------------------
// The tracker (upstream actionGroupTracker.ts)
// ---------------------------------------------------------------------------

/// Upstream `ActionGroupTarget`: an action the tracker measured.
#[derive(Clone, Debug, PartialEq)]
pub struct ActionTarget {
    pub key: SharedString,
    pub rect: HitRect,
}

/// The axis of a one-row or one-column group.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrackAxis {
    X,
    Y,
}

/// Picks the target for a finger at (x, y) from the targets measured at the begin.
pub type ResolveTargetFn = Rc<dyn Fn(f32, f32, &[ActionTarget]) -> Option<SharedString>>;
/// Decides, action by action, which take the press paint.
pub type PaintsFn = Rc<dyn Fn(&SharedString) -> bool>;

/// Upstream `ActionGroupTrackerOptions` (the parts that are not callbacks: the tracker
/// returns what changed instead).
#[derive(Clone, Default)]
pub struct TrackerOptions {
    /// How far outside an action the finger keeps it highlighted, px.
    pub highlight_expansion: f32,
    /// A one-axis group: the finger counts on its center line, clamped to its ends.
    pub tracks_along: Option<TrackAxis>,
    /// The owner's rule in place of the flat expansion.
    pub resolve_target: Option<ResolveTargetFn>,
    /// Which actions take the paint; every one by default.
    pub paints_highlight: Option<PaintsFn>,
}

/// A highlight change: the action highlighted now (`None`: none), and the one it replaces.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HighlightChange {
    pub next: Option<SharedString>,
    pub previous: Option<SharedString>,
}

/// An action's press paint.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TrackerPaint {
    /// Under the finger.
    On,
    /// Fading since the lift, over `fade_ms`.
    Fading { since: Instant, fade_ms: f64 },
}

/// Upstream `onCenterLine`: where a finger counts in a one-axis group: on the group's
/// center line across the axis, clamped to its ends along it.
pub fn on_center_line(band: &HitRect, axis: TrackAxis, x: f32, y: f32) -> (f32, f32) {
    match axis {
        TrackAxis::X => (
            x.clamp(band.left, band.right),
            (band.top + band.bottom) / 2.,
        ),
        TrackAxis::Y => (
            (band.left + band.right) / 2.,
            y.clamp(band.top, band.bottom),
        ),
    }
}

/// Where a press began (upstream `begin(x, y, {inert, landed})`): an inert press is
/// taken but tracks nothing; `landed` is the action the press landed on.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PressBegin {
    pub x: f32,
    pub y: f32,
    pub inert: bool,
    pub landed: Option<SharedString>,
}

/// Upstream `createActionGroupTracker`.
#[derive(Default)]
pub struct ActionGroupTracker {
    actions: Vec<ActionTarget>,
    highlighted: Option<SharedString>,
    began_on: Option<SharedString>,
    landed_on: Option<SharedString>,
    tracking: bool,
    inert: bool,
    last: (f32, f32),
    band: Option<HitRect>,
    paints: HashMap<SharedString, TrackerPaint>,
}

impl ActionGroupTracker {
    fn paints(&self, key: &SharedString, options: &TrackerOptions) -> bool {
        options.paints_highlight.as_ref().is_none_or(|f| f(key))
    }

    fn paint(&mut self, key: &SharedString, options: &TrackerOptions) {
        if self.paints(key, options) {
            self.paints.insert(key.clone(), TrackerPaint::On);
        }
    }

    fn cut(&mut self, key: &SharedString, options: &TrackerOptions) {
        if self.paints(key, options) {
            self.paints.remove(key);
        }
    }

    fn action_at(&self, fx: f32, fy: f32, options: &TrackerOptions) -> Option<SharedString> {
        let (x, y) = match (self.band, options.tracks_along) {
            (Some(band), Some(axis)) => on_center_line(&band, axis, fx, fy),
            _ => (fx, fy),
        };
        if let Some(resolve) = &options.resolve_target {
            return if self.actions.is_empty() {
                None
            } else {
                resolve(x, y, &self.actions)
            };
        }
        let margin = options.highlight_expansion;
        let mut first = None;
        for action in &self.actions {
            let r = action.rect;
            if x >= r.left - margin
                && x <= r.right + margin
                && y >= r.top - margin
                && y <= r.bottom + margin
            {
                // Rects that share an edge can both hold the point: the one the press
                // landed on wins.
                if self.landed_on.as_ref() == Some(&action.key) {
                    return Some(action.key.clone());
                }
                first.get_or_insert_with(|| action.key.clone());
            }
        }
        first
    }

    fn set_highlight(
        &mut self,
        next: Option<SharedString>,
        options: &TrackerOptions,
    ) -> Option<HighlightChange> {
        if next == self.highlighted {
            return None;
        }
        let previous = self.highlighted.take();
        if let Some(p) = &previous {
            self.cut(p, options);
        }
        if let Some(n) = &next {
            self.paint(n, options);
        }
        self.highlighted = next.clone();
        Some(HighlightChange { next, previous })
    }

    fn reset(&mut self) {
        self.actions.clear();
        self.highlighted = None;
        self.began_on = None;
        self.landed_on = None;
        self.tracking = false;
        self.inert = false;
        self.band = None;
    }

    /// A press began at (x, y): `actions` are the enabled actions, measured once, and
    /// `band` the group's box (for a one-axis group). An inert press is taken but tracks
    /// nothing. `landed` is the action the press landed on.
    pub fn begin(
        &mut self,
        press: PressBegin,
        actions: Vec<ActionTarget>,
        band: Option<HitRect>,
        options: &TrackerOptions,
    ) -> Option<HighlightChange> {
        let PressBegin {
            x,
            y,
            inert,
            landed,
        } = press;
        // A press can begin while another is tracked: the action the first one lit loses
        // its paint.
        let previous = self.highlighted.take();
        if let Some(p) = &previous {
            self.cut(p, options);
        }
        self.actions = if inert { Vec::new() } else { actions };
        self.band = if inert || options.tracks_along.is_none() {
            None
        } else {
            band
        };
        self.tracking = true;
        self.inert = inert;
        self.landed_on = landed;
        self.last = (x, y);
        let target = self.action_at(x, y, options);
        self.began_on = target.clone();
        if let Some(t) = &target {
            self.paint(t, options);
        }
        self.highlighted = target.clone();
        (target != previous).then_some(HighlightChange {
            next: target,
            previous,
        })
    }

    /// The finger moved to (x, y).
    pub fn move_to(&mut self, x: f32, y: f32, options: &TrackerOptions) -> Option<HighlightChange> {
        if !self.tracking {
            return None;
        }
        self.last = (x, y);
        let next = self.action_at(x, y, options);
        self.set_highlight(next, options)
    }

    /// The lift: the highlighted action's paint fades (from `now` over `fade_ms`) and it
    /// is returned to run. The highlight is left to the activation.
    pub fn end(
        &mut self,
        now: Instant,
        fade_ms: f64,
        options: &TrackerOptions,
    ) -> Option<SharedString> {
        let lifted = self.highlighted.clone();
        let was_tracking = self.tracking;
        self.reset();
        let lifted = lifted.filter(|_| was_tracking)?;
        if self.paints(&lifted, options) {
            self.paints.insert(
                lifted.clone(),
                TrackerPaint::Fading {
                    since: now,
                    fade_ms,
                },
            );
        }
        Some(lifted)
    }

    /// The press was taken away: the highlight is cut and nothing runs. Reports the
    /// highlight gone when one was tracked.
    pub fn cancel(&mut self, options: &TrackerOptions) -> Option<HighlightChange> {
        let current = self.highlighted.clone();
        let was_tracking = self.tracking;
        self.reset();
        let current = current?;
        self.cut(&current, options);
        was_tracking.then_some(HighlightChange {
            next: None,
            previous: Some(current),
        })
    }

    /// Escape: takes back a press in a one-axis group, where no slide-off can. Whether it
    /// cancelled (the key is then handled).
    pub fn escape(&mut self, options: &TrackerOptions) -> bool {
        if !self.tracking || self.band.is_none() {
            return false;
        }
        self.cancel(options);
        true
    }

    /// The content moved by (dx, dy) under the finger: shift the cached rects.
    pub fn offset_by(&mut self, dx: f32, dy: f32) {
        let shift = |r: &mut HitRect| {
            r.left += dx;
            r.right += dx;
            r.top += dy;
            r.bottom += dy;
        };
        for action in &mut self.actions {
            shift(&mut action.rect);
        }
        if let Some(band) = &mut self.band {
            shift(band);
        }
    }

    /// The targets moved: take their rects again and resolve the finger's last position.
    pub fn remeasure(
        &mut self,
        actions: Vec<ActionTarget>,
        band: Option<HitRect>,
        options: &TrackerOptions,
    ) -> Option<HighlightChange> {
        if !self.tracking || self.inert {
            return None;
        }
        self.actions = actions;
        if self.band.is_some() && band.is_some() {
            self.band = band;
        }
        let (x, y) = self.last;
        let next = self.action_at(x, y, options);
        self.set_highlight(next, options)
    }

    pub fn highlighted(&self) -> Option<&SharedString> {
        self.highlighted.as_ref()
    }

    pub fn began_on(&self) -> Option<&SharedString> {
        self.began_on.as_ref()
    }

    pub fn is_tracking(&self) -> bool {
        self.tracking
    }

    /// The paint of `key` at `now`; a finished fade comes off.
    pub fn paint_of(&mut self, key: &SharedString, now: Instant) -> Option<TrackerPaint> {
        let paint = *self.paints.get(key)?;
        if let TrackerPaint::Fading { since, fade_ms } = paint
            && now.saturating_duration_since(since).as_secs_f64() * 1000. >= fade_ms
        {
            self.paints.remove(key);
            return None;
        }
        Some(paint)
    }

    /// Finish every pending release fade now and forget the press (the owner's unmount).
    pub fn dispose(&mut self) {
        self.paints.clear();
        self.reset();
    }
}

/// Upstream `resolveNearestAction`: the nearest enabled action whose target, grown
/// toward 44 px, holds the point. A neighbour (a disabled action, an untracked member)
/// counts for nearness but resolves to none.
pub fn resolve_nearest_action(
    x: f32,
    y: f32,
    targets: &[ActionTarget],
    neighbours: &[HitRect],
) -> Option<SharedString> {
    let rects: Vec<HitRect> = targets
        .iter()
        .map(|t| t.rect)
        .chain(neighbours.iter().copied())
        .collect();
    resolve_hit_target(x, y, &rects, MIN_TOUCH_TARGET)
        .filter(|&i| i < targets.len())
        .map(|i| targets[i].key.clone())
}

// ---------------------------------------------------------------------------
// The hook (upstream useActionGroupTracking.ts and useGroupPressTracking.ts)
// ---------------------------------------------------------------------------

type ActivateFn = Rc<dyn Fn(&mut Window, &mut App)>;
type TrackFn = Rc<dyn Fn(PointerKind, f64, f64) -> bool>;
type PressFn = Rc<dyn Fn(Option<SharedString>, &mut Window, &mut App)>;
type CancelFn = Rc<dyn Fn(&mut Window, &mut App)>;

/// Upstream `ActionGroupTrackingOptions`.
#[derive(Clone)]
pub struct ActionGroupTrackingOptions {
    /// How long after touch-down the touched action highlights, ms.
    pub highlight_onset_ms: f64,
    /// How far outside an action a finger keeps it highlighted, px (a mouse or a pen
    /// hit-tests the exact boxes).
    pub highlight_expansion: f32,
    /// Picks a finger's action from the targets and their neighbours (disabled members),
    /// in place of the flat expansion: [`resolve_nearest_action`] grows each toward 44 px.
    pub nearest: bool,
    /// The pointers tracked; others keep their own click. @default every pointer
    pub pointer_types: Option<Vec<PointerKind>>,
    /// A press this rejects (its pointer and landing point) is not tracked.
    pub should_track: Option<TrackFn>,
    /// Off, nothing is tracked. @default true
    pub enabled: bool,
    /// Paint the tracked action; off, the group draws its own highlight. @default true
    pub paints_highlight: bool,
    /// A press began on an action (`None`: none).
    pub on_press_begin: Option<PressFn>,
    /// The press moved, now on an action.
    pub on_press_move: Option<PressFn>,
    /// The lift, before its action runs.
    pub on_press_end: Option<PressFn>,
    /// The press ended with nothing acting.
    pub on_press_cancel: Option<CancelFn>,
}

impl Default for ActionGroupTrackingOptions {
    fn default() -> Self {
        ActionGroupTrackingOptions {
            highlight_onset_ms: 0.,
            highlight_expansion: 0.,
            nearest: false,
            pointer_types: None,
            should_track: None,
            enabled: true,
            paints_highlight: true,
            on_press_begin: None,
            on_press_move: None,
            on_press_end: None,
            on_press_cancel: None,
        }
    }
}

/// Upstream `GROUP_PRESS_POINTERS`: the pointers a group's press slides for.
pub const GROUP_PRESS_POINTERS: [PointerKind; 2] = [PointerKind::Touch, PointerKind::Pen];

struct Member {
    rect: HitRect,
    enabled: bool,
    activate: ActivateFn,
    focus: Option<FocusHandle>,
}

struct GroupInner {
    tracker: RefCell<ActionGroupTracker>,
    /// The members drawn last frame, in order, and the ones being drawn now.
    members: RefCell<Vec<(SharedString, Member)>>,
    drawing: RefCell<Vec<(SharedString, Member)>>,
    options: RefCell<ActionGroupTrackingOptions>,
    pointer: RefCell<PointerKind>,
    landed: RefCell<Option<SharedString>>,
    neighbours: RefCell<Vec<HitRect>>,
    fade_ms: RefCell<f64>,
    ease: RefCell<rustryx_theme::CubicBezier>,
}

impl GroupInner {
    fn tracker_options(&self) -> TrackerOptions {
        let options = self.options.borrow();
        let finger = *self.pointer.borrow() == PointerKind::Touch;
        let resolve: Option<ResolveTargetFn> = (finger && options.nearest).then(|| {
            let neighbours = self.neighbours.borrow().clone();
            Rc::new(move |x: f32, y: f32, targets: &[ActionTarget]| {
                resolve_nearest_action(x, y, targets, &neighbours)
            }) as ResolveTargetFn
        });
        TrackerOptions {
            highlight_expansion: if finger {
                options.highlight_expansion
            } else {
                0.
            },
            tracks_along: None,
            resolve_target: resolve,
            paints_highlight: (!options.paints_highlight)
                .then(|| Rc::new(|_: &SharedString| false) as PaintsFn),
        }
    }

    fn targets(&self) -> Vec<ActionTarget> {
        self.members
            .borrow()
            .iter()
            .filter(|(_, m)| m.enabled && m.rect.has_area())
            .map(|(key, m)| ActionTarget {
                key: key.clone(),
                rect: m.rect,
            })
            .collect()
    }

    fn disabled_rects(&self) -> Vec<HitRect> {
        self.members
            .borrow()
            .iter()
            .filter(|(_, m)| !m.enabled && m.rect.has_area())
            .map(|(_, m)| m.rect)
            .collect()
    }

    fn member_at(&self, x: f32, y: f32) -> Option<SharedString> {
        self.members
            .borrow()
            .iter()
            .find(|(_, m)| m.rect.contains(x, y))
            .map(|(key, _)| key.clone())
    }
}

/// The tracking a group provides its members, upstream's press-group context.
#[derive(Clone)]
pub struct PressGroup(Rc<GroupInner>);

impl PressGroup {
    /// The group around the element being rendered that tracks presses now, if any.
    pub fn current(cx: &App) -> Option<PressGroup> {
        use_context::<PressGroup>(cx).filter(|g| g.0.options.borrow().enabled)
    }

    /// A member's probe, painted inside it: its box, whether it is enabled, its own
    /// activation and focus.
    pub fn member(
        &self,
        key: impl Into<SharedString>,
        enabled: bool,
        focus: Option<FocusHandle>,
        activate: impl Fn(&mut Window, &mut App) + 'static,
    ) -> impl IntoElement {
        let (inner, key) = (self.0.clone(), key.into());
        let activate: ActivateFn = Rc::new(activate);
        canvas(
            |_, _, _| {},
            move |bounds, _, _, _| {
                inner.drawing.borrow_mut().push((
                    key.clone(),
                    Member {
                        rect: HitRect::from_bounds(bounds),
                        enabled,
                        activate: activate.clone(),
                        focus: focus.clone(),
                    },
                ));
            },
        )
        .absolute()
        .size_full()
    }

    /// The press paint's strength on `key`, 0 to 1: 1 under the finger, falling over the
    /// release on the theme's exit easing. Asks for frames while it falls.
    pub fn strength(&self, key: &SharedString, window: &mut Window, cx: &App) -> f32 {
        let now = cx.background_executor().now();
        match self.0.tracker.borrow_mut().paint_of(key, now) {
            Some(TrackerPaint::On) => 1.,
            Some(TrackerPaint::Fading { since, fade_ms }) => {
                window.request_animation_frame();
                let t = if fade_ms <= 0. {
                    1.
                } else {
                    (now.saturating_duration_since(since).as_secs_f64() * 1000. / fade_ms).min(1.)
                };
                1. - self.0.ease.borrow().ease(t as f32)
            }
            None => 0.,
        }
    }

    /// The action highlighted now.
    pub fn highlighted(&self) -> Option<SharedString> {
        self.0.tracker.borrow().highlighted().cloned()
    }
}

/// What [`use_action_group_tracking`] returns: wrap the group in [`Self::group`].
#[derive(Clone)]
pub struct ActionGroupTracking {
    id: ElementId,
    group: PressGroup,
    attachment: GestureAttachment,
    enabled: bool,
}

impl ActionGroupTracking {
    /// The group's element (its members inside it): the press's area, and the context its
    /// members register in.
    pub fn group<E: ParentElement + IntoElement>(&self, el: E) -> AnyElement {
        // Painted after the members: the members drawn this frame become the ones a press
        // reads.
        let finish = {
            let inner = self.group.0.clone();
            canvas(
                |_, _, _| {},
                move |_, _, _, _| {
                    let drawn = std::mem::take(&mut *inner.drawing.borrow_mut());
                    *inner.members.borrow_mut() = drawn;
                },
            )
            .absolute()
            .size_0()
        };
        gesture_area(
            &self.attachment,
            provide(self.group.clone(), el.child(finish)),
        )
        .into_any_element()
    }

    /// [`Self::group`] with the group's own hit layer (upstream `useGroupPressTracking`'s
    /// `useTouchHitArea` with `groupGrowth`): under a coarse pointer each side of the
    /// group's box grows as far as its edge members would toward 44 px, stopping halfway
    /// to a neighbour, and a finger that lands there is the group's press. The element
    /// must be positioned.
    pub fn group_with_hit_area<E: ParentElement + IntoElement>(
        &self,
        el: E,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        use crate::gestures::touch_hit_area::{hit_area_with, key_of, noted_rect};
        let key = key_of(&self.id);
        let members: Vec<HitRect> = self
            .group
            .0
            .members
            .borrow()
            .iter()
            .filter(|(_, m)| m.enabled && m.rect.has_area())
            .map(|(_, m)| m.rect)
            .collect();
        let full = noted_rect(window, key).map(|own| {
            rustryx_primitives::gestures::hit_area::group_growth(&own, &members, MIN_TOUCH_TARGET)
        });
        let area = hit_area_with(
            &self.id,
            self.enabled && full.is_some(),
            None,
            full,
            crate::gestures::touch_hit_area::HitLinks {
                gestures: Some(&self.attachment),
                press: None,
            },
            window,
            cx,
        );
        let mut el = el;
        el.extend(area);
        self.group(el)
    }

    /// The press group its members read.
    pub fn press_group(&self) -> &PressGroup {
        &self.group
    }

    /// Upstream `tracker.isTracking`.
    pub fn is_tracking(&self) -> bool {
        self.group.0.tracker.borrow().is_tracking()
    }
}

fn run_change(inner: &GroupInner, window: &mut Window) {
    let _ = inner;
    window.refresh();
}

/// Upstream `useActionGroupTracking`: tracks a held pointer on the group across its
/// members, through the shared action-group tracker.
pub fn use_action_group_tracking(
    id: impl Into<ElementId>,
    options: ActionGroupTrackingOptions,
    window: &mut Window,
    cx: &mut App,
) -> ActionGroupTracking {
    let id: ElementId = id.into();
    let theme = rustryx_theme::current(window, cx);
    let fade_ms = super::press_feedback::press_fade_ms(window, cx);
    let state = window.use_keyed_state(
        ElementId::Name(format!("{id}-action-group").into()),
        cx,
        |window, cx| {
            let inner = Rc::new(GroupInner {
                tracker: RefCell::default(),
                members: RefCell::default(),
                drawing: RefCell::default(),
                options: RefCell::new(ActionGroupTrackingOptions::default()),
                pointer: RefCell::new(PointerKind::Touch),
                landed: RefCell::default(),
                neighbours: RefCell::default(),
                fade_ms: RefCell::new(0.),
                ease: RefCell::new(rustryx_theme::CubicBezier(0., 0., 1., 1.)),
            });
            let receive =
                {
                    let inner = Rc::downgrade(&inner);
                    Rc::new(move |r: &rustryx_primitives::gestures::AnyRecognizer,
                              touch: &rustryx_primitives::gestures::GestureTouch| {
                    let Some(inner) = inner.upgrade() else {
                        return false;
                    };
                    if r.number_of_touches() == 0 {
                        *inner.pointer.borrow_mut() = touch.pointer_type();
                        *inner.landed.borrow_mut() =
                            inner.member_at(touch.start_x() as f32, touch.start_y() as f32);
                    }
                    let (x, y) = (touch.start_x() as f32, touch.start_y() as f32);
                    if inner.options.borrow().nearest {
                        // Upstream `takesPress`: a press on an enabled member, or a finger's
                        // where a member's grown target holds it. A press on a disabled member
                        // keeps its click (its tooltip can open).
                        let on_member = inner
                            .members
                            .borrow()
                            .iter()
                            .find(|(_, m)| m.rect.contains(x, y))
                            .map(|(_, m)| m.enabled);
                        let takes = match on_member {
                            Some(enabled) => enabled,
                            None => {
                                touch.pointer_type() == PointerKind::Touch
                                    && resolve_nearest_action(
                                        x,
                                        y,
                                        &inner.targets(),
                                        &inner.disabled_rects(),
                                    )
                                    .is_some()
                            }
                        };
                        if !takes {
                            return false;
                        }
                    }
                    let track = inner.options.borrow().should_track.clone();
                    track.is_none_or(|f| f(touch.pointer_type(), touch.start_x(), touch.start_y()))
                })
                };
            let press = LongPressGestureRecognizer::new(LongPressOptions {
                base: RecognizerOptions {
                    name: Some("ActionGroupTracking".into()),
                    delays_touches_began: true,
                    // The hold picks an action: no text selection under it.
                    selection_lock: Some(Some(rustryx_primitives::gestures::lock_selection())),
                    should_receive_touch: Some(receive),
                    ..RecognizerOptions::default()
                },
                minimum_press_duration: 0.,
                allowable_movement: f64::INFINITY,
                cancels_on_extra_touch: true,
                scroller_press: Some((PRESS::CONTENT_TOUCH_DELAY, PRESS::SLOP)),
                ..LongPressOptions::default()
            });
            {
                let inner = Rc::downgrade(&inner);
                press.add_listener(move |r| {
                    let state = r.state();
                    let location = r.location();
                    let Some(inner) = inner.upgrade() else {
                        return;
                    };
                    after_gesture(move |window, cx| {
                        let (x, y) = (location.x as f32, location.y as f32);
                        let options = inner.options.borrow().clone();
                        match state {
                            GestureState::Began => {
                                let finger = *inner.pointer.borrow() == PointerKind::Touch;
                                *inner.neighbours.borrow_mut() = if finger && options.nearest {
                                    inner.disabled_rects()
                                } else {
                                    Vec::new()
                                };
                                let topts = inner.tracker_options();
                                let landed = inner.landed.borrow().clone();
                                let targets = inner.targets();
                                inner.tracker.borrow_mut().begin(
                                    PressBegin {
                                        x,
                                        y,
                                        inert: false,
                                        landed,
                                    },
                                    targets,
                                    None,
                                    &topts,
                                );
                                let lit = inner.tracker.borrow().highlighted().cloned();
                                if let Some(f) = &options.on_press_begin {
                                    f(lit, window, cx);
                                }
                                run_change(&inner, window);
                            }
                            GestureState::Changed => {
                                let topts = inner.tracker_options();
                                inner.tracker.borrow_mut().move_to(x, y, &topts);
                                let lit = inner.tracker.borrow().highlighted().cloned();
                                if let Some(f) = &options.on_press_move {
                                    f(lit, window, cx);
                                }
                                run_change(&inner, window);
                            }
                            GestureState::Ended => {
                                let lit = inner.tracker.borrow().highlighted().cloned();
                                if let Some(f) = &options.on_press_end {
                                    f(lit, window, cx);
                                }
                                let topts = inner.tracker_options();
                                let now = cx.background_executor().now();
                                let fade = *inner.fade_ms.borrow();
                                let lifted = inner.tracker.borrow_mut().end(now, fade, &topts);
                                inner.neighbours.borrow_mut().clear();
                                if let Some(key) = lifted {
                                    let member = inner
                                        .members
                                        .borrow()
                                        .iter()
                                        .find(|(k, m)| *k == key && m.enabled)
                                        .map(|(_, m)| (m.activate.clone(), m.focus.clone()));
                                    if let Some((activate, focus)) = member {
                                        // The lifted action takes focus first, as a press
                                        // gives it before the click.
                                        if let Some(focus) = focus {
                                            window.focus(&focus, cx);
                                        }
                                        activate(window, cx);
                                    }
                                }
                                run_change(&inner, window);
                            }
                            GestureState::Cancelled | GestureState::Failed => {
                                let topts = inner.tracker_options();
                                let was = inner.tracker.borrow().is_tracking();
                                inner.tracker.borrow_mut().cancel(&topts);
                                inner.neighbours.borrow_mut().clear();
                                if was && let Some(f) = &options.on_press_cancel {
                                    f(window, cx);
                                }
                                run_change(&inner, window);
                            }
                            GestureState::Possible => {}
                        }
                    });
                });
            }
            let attachment = gesture_host(window, cx).attach(&[press.any()]);
            (inner, attachment, press)
        },
    );
    let (inner, attachment) = {
        let s = state.read(cx);
        (s.0.clone(), s.1.clone())
    };
    let was_enabled = inner.options.borrow().enabled;
    let enabled = options.enabled;
    {
        let press = &state.read(cx).2;
        press.set_enabled(enabled);
        if let Some(types) = &options.pointer_types {
            press.set_allowed_pointer_types(Some(types.clone()));
        }
        let mut behavior = press.behavior_mut();
        behavior.plain_press.0 = options.highlight_onset_ms;
        if !press.state().is_active() {
            behavior.minimum_press_duration = options.highlight_onset_ms;
        }
    }
    *inner.options.borrow_mut() = options;
    *inner.fade_ms.borrow_mut() = fade_ms;
    *inner.ease.borrow_mut() = theme.tokens().ease.exit;
    // Turning tracking off drops a tracked press and its fades; the group hears it as a
    // cancel.
    if was_enabled && !enabled {
        let topts = inner.tracker_options();
        let was = inner.tracker.borrow().is_tracking();
        inner.tracker.borrow_mut().cancel(&topts);
        inner.tracker.borrow_mut().dispose();
        if was && let Some(f) = inner.options.borrow().on_press_cancel.clone() {
            f(window, cx);
        }
    }
    ActionGroupTracking {
        id,
        group: PressGroup(inner),
        attachment,
        enabled,
    }
}

/// Upstream `useGroupPressTracking`: a held finger or pen across the members of a
/// group (ButtonGroup, ToggleButtonGroup), while `enabled` (the touch presentation). The
/// member under it highlights at once, and the lift runs it; a finger picks the nearest
/// member whose target, grown toward 44 px, holds it.
pub fn use_group_press_tracking(
    id: impl Into<ElementId>,
    enabled: bool,
    window: &mut Window,
    cx: &mut App,
) -> ActionGroupTracking {
    use_action_group_tracking(
        id,
        ActionGroupTrackingOptions {
            highlight_onset_ms: 0.,
            highlight_expansion: 0.,
            nearest: true,
            pointer_types: Some(GROUP_PRESS_POINTERS.to_vec()),
            enabled,
            ..ActionGroupTrackingOptions::default()
        },
        window,
        cx,
    )
}

#[cfg(test)]
#[path = "action_group_tests.rs"]
mod tests;
