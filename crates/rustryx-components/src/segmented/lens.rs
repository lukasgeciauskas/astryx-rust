//! The selected surface's slide, upstream `useSegmentedSelectionMotion.ts` (FR9, FR10):
//! when the selection changes, one surface (the lens) travels from the old segment to the
//! new one, carrying its width across segments of different widths. Under a touch
//! presentation it settles on the `segmented.selectionSpring` role from rest; under a fine
//! pointer it slides on the theme's duration and easing, as the segments' own transition
//! does (`--duration-fast` on `--ease-standard`). A change during a slide starts from
//! where the lens shows. Under Reduce Motion nothing travels: with a coarse pointer the
//! old surface fades out in place while the new one fades in (the animator's cross-fade),
//! and with a fine one the change lands at once.
//!
//! A press (FR11-FR14, `useSegmentedTouchTracking.ts` and the motion's `begin`, `lift`,
//! `drag`, `release` and `cancel`): a press on the selected segment lifts the lens under
//! the touch presentation (it grows by `liftGrowth` on the `lift` spring and settles back
//! after the release); a drag from it carries the lens, its center chasing the pointer at
//! most `dragOvershoot` past the end segments, on `springs.fingerTracking` under the touch
//! presentation and directly otherwise and under Reduce Motion; the selected face follows
//! the lens's center with hysteresis; the lift commits the segment nearest the pointer. A
//! press elsewhere leaves the lens in place, and a drag from it lights the segment it would
//! commit. A cancel puts the lens back.

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{App, Bounds, ElementId, Entity, Pixels, Window};
use rustryx_primitives::motion::progress::{map_progress, map_range};
use rustryx_primitives::motion::theme_timing::eased_progress;
use rustryx_primitives::motion::uikit::segmented::role::{LIFT, SELECTION_SPRING};
use rustryx_primitives::motion::uikit::segmented::{DRAG_OVERSHOOT, LIFT_GROWTH};
use rustryx_primitives::motion::uikit::springs::role::FINGER_TRACKING;
use rustryx_primitives::motion::{
    AnimateOptions, CrossfadeAs, MotionCurves, MotionProperty, MotionValues, ReducedMotion,
    SpringTransition,
};

use super::touch::{Area, SegmentSpan, follow_face, lift_target_at, segment_target_at};
use crate::animated_layout::engine::FrameEngine;

/// A box along the track, relative to the track's padding box.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct SegmentBox {
    pub left: f32,
    pub top: f32,
    pub width: f32,
    pub height: f32,
}

impl SegmentBox {
    fn center(&self) -> f32 {
        self.left + self.width / 2.
    }

    fn is_laid_out(&self) -> bool {
        self.width > 0. && self.height > 0.
    }

    fn span(&self, disabled: bool) -> SegmentSpan {
        SegmentSpan {
            left: self.left,
            right: self.left + self.width,
            top: Some(self.top),
            bottom: Some(self.top + self.height),
            band: None,
            disabled,
        }
    }
}

/// A press on the control, from landing to lift (upstream's `Touch`).
#[derive(Clone, Debug)]
pub(crate) struct Press {
    /// The segment the press landed on, and the selected one.
    pub origin: Option<usize>,
    pub selected: Option<usize>,
    /// It landed on the selected segment: the lens shows it.
    pub carries: bool,
    /// A finger (or a pen): a release under Reduce Motion cross-fades.
    pub finger: bool,
    /// The horizontal pan began: a drag, not a tap.
    pub dragging: bool,
    /// The segments as the press found them, and which are disabled.
    pub boxes: Vec<SegmentBox>,
    pub disabled: Vec<bool>,
    /// The track's own box, for the lift's area.
    pub track: Area,
}

/// What the lens shows this frame.
#[derive(Clone, Copy, Debug)]
pub(crate) struct LensFrame {
    pub lens: Option<(SegmentBox, f32)>,
    /// Under Reduce Motion with a coarse pointer, the surface fading out where it was.
    pub leaving: Option<(SegmentBox, f32)>,
}

pub(crate) struct LensState {
    lens: FrameEngine,
    after: FrameEngine,
    /// The segment the lens rests on.
    shown: Option<usize>,
    /// The leaving surface's box while it fades.
    leaving: Option<SegmentBox>,
    /// The segments' boxes as the last frame laid them out.
    pub boxes: Rc<RefCell<Vec<SegmentBox>>>,
    /// The press in progress.
    pub press: Option<Press>,
    /// The lens is bound to the selected segment under a press: it shows at rest too.
    pub held: bool,
    /// The held lens grew by `liftGrowth`.
    pub lifted: bool,
    /// The segment showing the selected face under a dragging lens (FR12).
    pub face: Option<usize>,
    /// The segment a drag from elsewhere would commit: it shows the pressed overlay.
    pub drag_target: Option<usize>,
    /// The click the platform still sends for a press the lift already answered (a drag's
    /// commit, a cancelled press): the segment swallows it.
    pub swallow_click: bool,
    /// The drag's velocity along the track at its latest move, px/s: the release settles
    /// from it (FR10, FR13).
    pub release_velocity: Option<f64>,
}

/// The lens of the control `id` names, kept across frames.
pub(crate) fn use_lens(id: &ElementId, window: &mut Window, cx: &mut App) -> Entity<LensState> {
    let executor = cx.background_executor().clone();
    let state = window.use_keyed_state(
        ElementId::Name(format!("{id:?}-lens").into()),
        cx,
        move |_, _| LensState {
            lens: FrameEngine::new(executor.clone()),
            after: FrameEngine::new(executor),
            shown: None,
            leaving: None,
            boxes: Rc::default(),
            press: None,
            held: false,
            lifted: false,
            face: None,
            drag_target: None,
            swallow_click: false,
            release_velocity: None,
        },
    );
    let (lens, after) = {
        let s = state.read(cx);
        (s.lens.clone(), s.after.clone())
    };
    lens.sync_environment(window, cx);
    after.sync_environment(window, cx);
    state
}

/// Where the lens shows now on `at`'s box, from its channels.
fn presented(engine: &FrameEngine, at: SegmentBox) -> SegmentBox {
    let e = engine.engine();
    let x = e.get(MotionProperty::X, None) as f32;
    let scale = e.get(MotionProperty::ScaleX, None) as f32;
    let scale_y = e.get(MotionProperty::ScaleY, None) as f32;
    let width = at.width * scale;
    let height = at.height * scale_y;
    SegmentBox {
        left: at.center() + x - width / 2.,
        width,
        top: at.top + (at.height - height) / 2.,
        height,
    }
}

/// The selection is `selected` this frame: a change from the segment the lens rests on
/// starts its motion. `touch` is the touch presentation (a coarse primary pointer); `fast`
/// and `ease` the theme's transition.
pub(crate) fn select(
    state: &Entity<LensState>,
    selected: Option<usize>,
    touch: bool,
    fast: std::time::Duration,
    ease: rustryx_theme::CubicBezier,
    cx: &mut App,
) {
    let (shown, boxes) = {
        let s = state.read(cx);
        (s.shown, s.boxes.borrow().clone())
    };
    if shown == selected {
        return;
    }
    state.update(cx, |s, _| s.shown = selected);
    let (Some(from), Some(to)) = (shown, selected) else {
        return;
    };
    let (Some(from_box), Some(to_box)) = (boxes.get(from).copied(), boxes.get(to).copied()) else {
        return;
    };
    if !from_box.is_laid_out() || !to_box.is_laid_out() {
        return;
    }
    let s = state.read(cx);
    let (lens, after) = (s.lens.clone(), s.after.clone());
    let moving = !lens.engine().is_settled();
    // What is on screen before the lens is rebound: the lens where it shows, or the old
    // segment's resting surface.
    let leaving = if moving {
        presented(&lens, from_box)
    } else {
        from_box
    };
    let opacity = if moving {
        lens.engine().get(MotionProperty::Opacity, None)
    } else {
        1.
    };
    match lens.engine().reduced_motion(false) {
        ReducedMotion::Off => {
            // Carry: the lens stays where it shows, bound to the new segment.
            lens.engine().set(
                MotionValues::new()
                    .x(f64::from(leaving.center() - to_box.center()))
                    .scale_x(f64::from(leaving.width / to_box.width))
                    .opacity(1.),
            );
            if touch {
                // A drag's release settles from its velocity; any other change from rest.
                let velocity = state
                    .update(cx, |s, _| s.release_velocity.take())
                    .unwrap_or(0.);
                lens.engine().animate(
                    MotionValues::new().x(0.).scale_x(1.).scale_y(1.),
                    SpringTransition::new(SELECTION_SPRING)
                        .velocity(MotionValues::new().x(velocity)),
                    AnimateOptions::default(),
                );
            } else {
                // The segments' own transition: the theme's duration and easing,
                // unclamped so an easing that overshoots carries the lens.
                let progress = eased_progress(
                    fast.as_secs_f64(),
                    move |t| f64::from(ease.ease(t as f32)),
                    false,
                )
                .shared();
                let toward = |from: f64, to: f64| {
                    map_progress(progress.clone(), map_range([0., 1.], [from, to], false)).shared()
                };
                let e = lens.engine();
                let curves = MotionCurves::new()
                    .with(
                        MotionProperty::X,
                        toward(e.get(MotionProperty::X, None), 0.),
                    )
                    .with(
                        MotionProperty::ScaleX,
                        toward(e.get(MotionProperty::ScaleX, None), 1.),
                    )
                    .with(
                        MotionProperty::ScaleY,
                        toward(e.get(MotionProperty::ScaleY, None), 1.),
                    );
                let _ = e.play(curves, AnimateOptions::default());
            }
            state.update(cx, |s, _| s.leaving = None);
        }
        reduced => {
            // Reduce Motion: no travel. The lens lands on the new segment and fades in
            // there as an entrance; the surface on screen fades out where it is.
            lens.engine()
                .set(MotionValues::new().x(0.).scale_x(1.).opacity(1.));
            let fades = reduced == ReducedMotion::Crossfade && opacity > 0.;
            if fades {
                after
                    .engine()
                    .set(MotionValues::new().x(0.).opacity(opacity));
                after.engine().animate(
                    MotionValues::new()
                        .x(f64::from(to_box.center() - leaving.center()))
                        .opacity(0.),
                    SELECTION_SPRING,
                    AnimateOptions::default(),
                );
            }
            lens.engine().animate(
                MotionValues::new().x(0.).scale_x(1.),
                SELECTION_SPRING,
                AnimateOptions {
                    crossfade_as: Some(CrossfadeAs::Entrance),
                    ..Default::default()
                },
            );
            state.update(cx, |s, _| s.leaving = fades.then_some(leaving));
        }
    }
}

/// This frame's lens and leaving surface; asks for frames while either moves.
pub(crate) fn frame(
    state: &Entity<LensState>,
    selected: Option<usize>,
    window: &mut Window,
    cx: &mut App,
) -> LensFrame {
    let s = state.read(cx);
    let (lens, after, leaving, held) = (s.lens.clone(), s.after.clone(), s.leaving, s.held);
    let boxes = s.boxes.borrow().clone();
    let lens_moving = lens.drive(window, cx) || held;
    let after_moving = leaving.is_some() && after.drive(window, cx);
    if leaving.is_some() && !after_moving {
        state.update(cx, |s, _| s.leaving = None);
    }
    let shown_box = selected.and_then(|i| boxes.get(i).copied());
    LensFrame {
        lens: shown_box.filter(|_| lens_moving).map(|b| {
            let opacity = lens.engine().shown_opacity(None) as f32;
            (presented(&lens, b), opacity)
        }),
        leaving: leaving.filter(|_| after_moving).map(|b| {
            let e = after.engine();
            let x = e.get(MotionProperty::X, None) as f32;
            let opacity = e.shown_opacity(None) as f32;
            (
                SegmentBox {
                    left: b.left + x,
                    ..b
                },
                opacity,
            )
        }),
    }
}

/// Records the segments' boxes (the last `count` children of the track) relative to the
/// track's padding box, `origin`.
pub(crate) fn measure(
    boxes: &Rc<RefCell<Vec<SegmentBox>>>,
    origin: gpui::Point<Pixels>,
    children: &[Bounds<Pixels>],
    count: usize,
) -> bool {
    let start = children.len().saturating_sub(count);
    let next: Vec<SegmentBox> = children[start..]
        .iter()
        .map(|b| SegmentBox {
            left: f32::from(b.origin.x - origin.x),
            top: f32::from(b.origin.y - origin.y),
            width: f32::from(b.size.width),
            height: f32::from(b.size.height),
        })
        .collect();
    let changed = *boxes.borrow() != next;
    if changed {
        *boxes.borrow_mut() = next;
    }
    changed
}

// --- The press (FR11-FR14) --------------------------------------------------

/// A press landed at `x` (track coordinates) on a control showing `selected`. The
/// segments and the track are read once.
/// Where a press landed and what the control showed then.
pub(crate) struct Landing {
    /// The point, in track coordinates.
    pub x: f32,
    pub y: f32,
    pub track: Area,
    pub selected: Option<usize>,
    pub disabled: Vec<bool>,
    /// A finger or a pen, not a mouse.
    pub finger: bool,
}

pub(crate) fn begin(state: &Entity<LensState>, landing: Landing, cx: &mut App) {
    let Landing {
        x,
        y,
        track,
        selected,
        disabled,
        finger,
    } = landing;
    let boxes = state.read(cx).boxes.borrow().clone();
    let spans: Vec<SegmentSpan> = boxes
        .iter()
        .zip(disabled.iter())
        .map(|(b, d)| b.span(*d))
        .collect();
    let origin =
        lift_target_at(x, y, &[track], &spans, None).or_else(|| segment_target_at(x, &spans));
    state.update(cx, |s, _| {
        s.release_velocity = None;
        s.press = Some(Press {
            origin,
            selected,
            carries: origin.is_some() && origin == selected,
            finger,
            dragging: false,
            boxes,
            disabled,
            track,
        });
        s.face = None;
        s.drag_target = None;
        s.swallow_click = false;
    });
}

/// The press tracker highlighted the press: a press on the selected segment lifts the
/// lens (under the touch presentation, without Reduce Motion).
pub(crate) fn highlighted(state: &Entity<LensState>, presenting: bool, cx: &mut App) {
    if state.read(cx).press.as_ref().is_some_and(|p| p.carries) {
        lift(state, presenting, cx);
    }
}

/// Binds the lens on the pressed selected segment, where it rests.
fn hold(state: &Entity<LensState>, cx: &mut App) -> Option<SegmentBox> {
    let s = state.read(cx);
    let press = s.press.as_ref()?;
    let origin = press.origin?;
    let at = *press.boxes.get(origin)?;
    if !at.is_laid_out() {
        return None;
    }
    if !s.held {
        let lens = s.lens.clone();
        if lens.engine().is_settled() {
            lens.engine().set(
                MotionValues::new()
                    .x(0.)
                    .scale_x(1.)
                    .scale_y(1.)
                    .opacity(1.),
            );
        }
        state.update(cx, |s, _| s.held = true);
    }
    Some(at)
}

fn lift(state: &Entity<LensState>, presenting: bool, cx: &mut App) {
    let (lifted, lens) = {
        let s = state.read(cx);
        (s.lifted, s.lens.clone())
    };
    if lifted || !presenting || lens.engine().reduced_motion(false) != ReducedMotion::Off {
        return;
    }
    let Some(at) = hold(state, cx) else {
        return;
    };
    let [width, height] = LIFT_GROWTH;
    state.update(cx, |s, _| s.lifted = true);
    lens.engine().animate(
        MotionValues::new()
            .scale_x((f64::from(at.width) + width) / f64::from(at.width))
            .scale_y((f64::from(at.height) + height) / f64::from(at.height)),
        LIFT,
        AnimateOptions::default(),
    );
}

/// The pointer moved to `x` with the drag begun: a drag from the selected segment carries
/// the lens and its face; a drag from elsewhere lights the segment it would commit.
pub(crate) fn drag(state: &Entity<LensState>, x: f32, presenting: bool, cx: &mut App) {
    let Some(press) = state.read(cx).press.clone() else {
        return;
    };
    if !press.dragging {
        state.update(cx, |s, _| {
            if let Some(p) = &mut s.press {
                p.dragging = true;
            }
        });
    }
    let spans: Vec<SegmentSpan> = press
        .boxes
        .iter()
        .zip(press.disabled.iter())
        .map(|(b, d)| b.span(*d))
        .collect();
    if !press.carries {
        // The selected segment shows its own face, never the pressed overlay.
        let target = segment_target_at(x, &spans).filter(|t| Some(*t) != press.selected);
        state.update(cx, |s, _| s.drag_target = target);
        return;
    }
    // A drag that starts before the highlight lifts the lens as well.
    lift(state, presenting, cx);
    let Some(at) = hold(state, cx) else {
        return;
    };
    let centers = press.boxes.iter().map(|b| b.center());
    let (low, high) = centers.fold((f32::INFINITY, f32::NEG_INFINITY), |(l, h), c| {
        (l.min(c), h.max(c))
    });
    let reach = DRAG_OVERSHOOT as f32;
    let target = x.clamp(low - reach, high + reach);
    let offset = f64::from(target - at.center());
    let lens = state.read(cx).lens.clone();
    if !presenting || lens.engine().reduced_motion(false) != ReducedMotion::Off {
        // A mouse or a pen (and a finger under Reduce Motion) holds the lens directly.
        lens.engine().set(MotionValues::new().x(offset));
    } else {
        lens.engine().animate(
            MotionValues::new().x(offset),
            FINGER_TRACKING,
            AnimateOptions::default(),
        );
    }
    follow(state, cx);
}

/// FR12: the selected face goes to the segment under the lens's presented center.
pub(crate) fn follow(state: &Entity<LensState>, cx: &mut App) {
    let s = state.read(cx);
    let (Some(press), true) = (s.press.as_ref(), s.held) else {
        return;
    };
    let Some(origin) = press.origin.filter(|_| press.carries) else {
        return;
    };
    let at = press.boxes[origin];
    let center = presented(&s.lens, at).center();
    let spans: Vec<SegmentSpan> = press
        .boxes
        .iter()
        .zip(press.disabled.iter())
        .map(|(b, d)| b.span(*d))
        .collect();
    let current = s.face.unwrap_or(origin);
    let next = follow_face(current, center, &spans);
    if next != current || (s.face.is_some() && next == origin) {
        state.update(cx, |s, _| s.face = (next != origin).then_some(next));
    }
}

/// The pointer let go at (`x`, `y`). The held lens settles back on the selection
/// (a commit then carries it to the new value); returns the segment to commit when it is
/// not the selected one and the press dragged (a tap commits through its own click).
pub(crate) fn release(
    state: &Entity<LensState>,
    x: f32,
    y: f32,
    finger: (f32, f32),
    cx: &mut App,
) -> Option<usize> {
    let (press, held) = state.update(cx, |s, _| {
        let press = s.press.take();
        let held = std::mem::take(&mut s.held);
        s.lifted = false;
        s.face = None;
        s.drag_target = None;
        (press, held)
    });
    let press = press?;
    if held {
        settle_back(state, press.finger, cx);
    }
    let spans: Vec<SegmentSpan> = press
        .boxes
        .iter()
        .zip(press.disabled.iter())
        .map(|(b, d)| b.span(*d))
        .collect();
    // The pointer counts on the control's center line, clamped to its ends.
    let line = (press.track.top + press.track.bottom) / 2.;
    let cx_ = x.clamp(press.track.left, press.track.right);
    let _ = y;
    let target = lift_target_at(cx_, line, &[press.track], &spans, Some(finger))?;
    let selected = press.selected;
    let is_tap = !press.dragging && Some(target) == press.origin;
    if !is_tap {
        // No click comes for a drag, and none may act: the lift commits it.
        state.update(cx, |s, _| s.swallow_click = true);
    }
    (!is_tap && Some(target) != selected).then_some(target)
}

/// The press was taken away (a cancel, or Escape): nothing commits; the lens goes back.
pub(crate) fn cancel(state: &Entity<LensState>, cx: &mut App) {
    let (press, held) = state.update(cx, |s, _| {
        // The click a cancelled press may still send selects nothing.
        s.swallow_click = s.press.is_some();
        s.lifted = false;
        s.face = None;
        s.drag_target = None;
        (s.press.take(), std::mem::take(&mut s.held))
    });
    if let (Some(press), true) = (press, held) {
        settle_back(state, press.finger, cx);
    }
}

/// The held lens settles back to rest on its segment: its size on the lift spring, its
/// place on the selection spring, from where it shows.
fn settle_back(state: &Entity<LensState>, finger: bool, cx: &mut App) {
    let lens = state.read(cx).lens.clone();
    let velocity = state.read(cx).release_velocity.unwrap_or(0.);
    let e = lens.engine();
    e.animate(
        MotionValues::new().scale_x(1.).scale_y(1.),
        LIFT,
        AnimateOptions {
            touch_release: finger,
            ..Default::default()
        },
    );
    e.animate(
        MotionValues::new().x(0.),
        SpringTransition::new(SELECTION_SPRING).velocity(MotionValues::new().x(velocity)),
        AnimateOptions {
            touch_release: finger,
            ..Default::default()
        },
    );
}

/// A drag of the lens moved at `velocity` px/s along the track (its release, `ended`).
pub(crate) fn drag_velocity(state: &Entity<LensState>, velocity: f64, ended: bool, cx: &mut App) {
    state.update(cx, |s, _| {
        let carried = s.press.as_ref().is_some_and(|p| p.carries);
        if carried || (ended && s.release_velocity.is_some()) {
            s.release_velocity = Some(velocity);
        }
    });
}

/// Whether a press is in progress.
pub(crate) fn pressing(state: &Entity<LensState>, cx: &App) -> bool {
    state.read(cx).press.is_some()
}

/// Whether the click arriving now is one a press's lift already answered; reading it
/// spends it.
pub(crate) fn take_stray_click(state: &Entity<LensState>, cx: &mut App) -> bool {
    state.update(cx, |s, _| std::mem::take(&mut s.swallow_click))
}
