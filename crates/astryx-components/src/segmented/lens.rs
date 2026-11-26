//! The selected surface's slide, upstream `useSegmentedSelectionMotion.ts` (FR9, FR10):
//! when the selection changes, one surface (the lens) travels from the old segment to the
//! new one, carrying its width across segments of different widths. Under a touch
//! presentation it settles on the `segmented.selectionSpring` role from rest; under a fine
//! pointer it slides on the theme's duration and easing, as the segments' own transition
//! does (`--duration-fast` on `--ease-standard`). A change during a slide starts from
//! where the lens shows. Under Reduce Motion nothing travels: with a coarse pointer the
//! old surface fades out in place while the new one fades in (the animator's cross-fade),
//! and with a fine one the change lands at once.

use std::cell::RefCell;
use std::rc::Rc;

use astryx_primitives::motion::progress::{map_progress, map_range};
use astryx_primitives::motion::theme_timing::eased_progress;
use astryx_primitives::motion::uikit::segmented::role::SELECTION_SPRING;
use astryx_primitives::motion::{
    AnimateOptions, CrossfadeAs, MotionCurves, MotionProperty, MotionValues, ReducedMotion,
};
use gpui::{App, Bounds, ElementId, Entity, Pixels, Window};

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
    let width = at.width * scale;
    SegmentBox {
        left: at.center() + x - width / 2.,
        width,
        ..at
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
    ease: astryx_theme::CubicBezier,
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
                lens.engine().animate(
                    MotionValues::new().x(0.).scale_x(1.),
                    SELECTION_SPRING,
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
    let (lens, after, leaving) = (s.lens.clone(), s.after.clone(), s.leaving);
    let boxes = s.boxes.borrow().clone();
    let lens_moving = lens.drive(window, cx);
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
