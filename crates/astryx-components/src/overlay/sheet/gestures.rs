//! The sheet's drag and snap machinery, upstream `BottomSheet/useSheetGestures.ts`, as
//! a pure model the panel feeds with its pointer and touch input and reads each frame.
//!
//! A drag moves the sheet live. On release it settles on the detent nearest where
//! the sheet would come to rest, or it dismisses: under a finger by UIKit's sheet
//! values (closed counted as one position, a 0.1 s projection above 250 px/s, a flick
//! from 1008 px/s landing at or past its projection, FR19, DEC-4), and under a mouse
//! or pen by a scroll view's projection and the dismiss line. A pull past fully open
//! stretches on the rubber band under a finger (FR18, DEC-3), and a resisted 48 px
//! at most under a mouse or pen. Between detents the drag is eased toward a nearby
//! one so it clicks into place.
//!
//! A settled detent is split in two: the layout height the scrolling area gives up
//! (`settled_layout_offset`), and the rest, a transform. A peek keeps the full height
//! and slides below the window. A settle moves only the transform; the layout swaps
//! at rest (`complete_settle`), with the same visible geometry. A drag above its
//! base detent restores the full layout height. The scrolling area's end padding
//! keeps its scroll position across a height change (`scroll_preservation_inset`).
//!
//! A resting sheet re-resolves its detent by index on a resize or new snap points
//! (`reanchor`): the pixels change meaning, but the chosen stop does not.

use astryx_primitives::gestures::{PointerKind, VelocityTracker};
use astryx_primitives::motion::resolve_snap_point::{
    SnapPointProjection, SnapPointQuery, resolve_snap_point,
};
use astryx_primitives::motion::rubber_band_with;
use astryx_primitives::motion::uikit::{scroll, sheet};

use super::snap_offsets::{
    DETENT_DEDUP_PX, compute_detent_offsets, peek_offset_for, resolve_settle_offset,
    scrim_opacity_for_offset,
};

/// UIKit's normal scroll deceleration, per ms.
const DECELERATION_RATE: f64 = 0.998;
/// How far ahead a mouse release projects, ms of its velocity.
const PROJECTION_FACTOR: f64 = DECELERATION_RATE / (1. - DECELERATION_RATE);
/// A mouse or pen gesture shorter than this projects nothing, px.
const PROJECTION_MIN_TRAVEL: f64 = 48.;
/// A slow drag below the shortest detent dismisses past this share of its height.
pub(crate) const DISMISS_OVERSHOOT_RATIO: f64 = 0.4;
/// Within this of a detent the live drag is eased toward it, px.
const MAGNET_RANGE: f64 = 40.;
/// A mouse's or pen's stretch past fully open: a resisted share of the drag...
const OVERSCROLL_RESISTANCE: f64 = 0.35;
/// ...capped here, px. Never more than the smallest reserve below the window.
pub(crate) const OVERSCROLL_MAX: f64 = 48.;
/// How far a finger resting on the body moves before the pull becomes a drag, px.
pub(crate) const DRAG_PROMOTION_SLOP: f64 = 8.;
/// Travel past where a scroll ran out of content before the sheet takes the rest
/// of the pull, px.
pub(crate) const CONTENT_END_HANDOFF_SLOP: f64 = 4.;

/// A drag's close: the distance left to the closed position and the speed
/// (upstream `SheetDragExit`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SheetDragExit {
    /// px from the release position to closed.
    pub distance: f64,
    /// The release velocity, px/s, positive toward closed.
    pub velocity: f64,
}

/// A drag in progress.
#[derive(Clone, Debug)]
pub(crate) struct DragState {
    pub pointer: PointerKind,
    pub start: f64,
    pub tracker: VelocityTracker,
    pub height: f64,
    /// The surface kept below the window when the drag began.
    pub reserve: f64,
    pub base_offset: f64,
    pub base_layout_offset: f64,
    pub rendered_offset: f64,
    pub layout_offset: f64,
    pub natural_end_gap: f64,
    pub has_traveled: bool,
}

/// What a move did.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Moved {
    pub offset: f64,
    /// The scrim's opacity for the live drag.
    pub scrim: f64,
    /// The first move off the base detent: the panel dismisses the keyboard.
    pub travel_started: bool,
}

/// Where a release leaves the sheet.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Release {
    /// Settles on the detent at `offset`, from `velocity` (px/s), with this scrim.
    Settle {
        offset: f64,
        velocity: f64,
        scrim: f64,
        touch: bool,
    },
    /// Asks to close; `exit` is set when a touch release asked.
    Dismiss { exit: Option<SheetDragExit> },
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct TouchDismissal {
    is_touch: bool,
    pending: bool,
    exit: Option<SheetDragExit>,
}

/// What a settle's landing reads to swap its layout split: the base it left, where
/// it lands, and what the drag rendered.
#[derive(Clone, Copy, Debug)]
struct Split {
    base_layout: f64,
    target_offset: f64,
    target_layout: f64,
    rendered_offset: f64,
    layout_offset: f64,
    natural_end_gap: f64,
}

/// The detents of a sheet of a given height.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Detents {
    pub offsets: Vec<f64>,
    pub peek: Option<f64>,
}

/// The sheet's drag state (upstream `useSheetGestures`'s refs and state).
#[derive(Clone, Debug, Default)]
pub(crate) struct SheetGestures {
    /// Whether a downward swipe may dismiss (`purpose` `Info`).
    pub can_dismiss: bool,
    /// The border box kept below the window, excluded from detent heights.
    pub reserve: f64,
    /// Candidate visible detent heights below fully open, px.
    pub snap_heights: Vec<f64>,
    /// The fully open height, measured (border box, the reserve included).
    pub sheet_height: f64,
    pub settled_offset: f64,
    pub settled_layout_offset: f64,
    /// Which detent the sheet rests on, an index into the resolved list.
    pub settled_index: usize,
    /// The layout split a settle keeps until it lands.
    pub settling_layout_offset: Option<f64>,
    pub scroll_preservation_inset: f64,
    pending_inset: Option<f64>,
    /// What a settle computes its landing inset from: (base layout, end gap).
    settle_basis: Option<(f64, f64)>,
    pub drag: Option<DragState>,
    /// The offset now: the settled detent at rest, the live drag position.
    pub active_offset: f64,
    touch_dismissal: Option<TouchDismissal>,
    /// The scrim of the detent a close left, until the owner answers.
    resting_scrim: Option<f64>,
}

fn visible_height_for_offset(height: f64, offset: f64, reserve: f64) -> f64 {
    (height - reserve - offset).max(0.)
}

fn preservation_inset(base_layout: f64, target_layout: f64, natural_end_gap: f64) -> f64 {
    (base_layout - target_layout - natural_end_gap).max(0.)
}

/// Pulls a value toward the nearest of `targets` within [`MAGNET_RANGE`], easing the
/// last stretch.
pub(crate) fn magnetize(value: f64, targets: &[f64]) -> f64 {
    let mut nearest = targets[0];
    let mut distance = (value - nearest).abs();
    for t in targets {
        let d = (value - t).abs();
        if d < distance {
            distance = d;
            nearest = *t;
        }
    }
    if distance >= MAGNET_RANGE {
        return value;
    }
    let t = distance / MAGNET_RANGE;
    let pull = 1. - t * t;
    value + (nearest - value) * pull
}

impl SheetGestures {
    /// The detents of a sheet `height` tall.
    pub fn detents(&self, height: f64) -> Detents {
        let visible = visible_height_for_offset(height, 0., self.reserve);
        let offsets = compute_detent_offsets(visible, &self.snap_heights, DETENT_DEDUP_PX);
        let peek = peek_offset_for(&offsets, visible);
        Detents { offsets, peek }
    }

    /// The dismiss line: past the shortest detent by 40% of its height.
    fn dismiss_offset(&self, detents: &Detents, height: f64) -> f64 {
        let max = *detents.offsets.last().unwrap_or(&0.);
        let shortest = visible_height_for_offset(height, max, self.reserve);
        max + shortest * DISMISS_OVERSHOOT_RATIO
    }

    /// The scrim's opacity at `offset` for a sheet `height` tall.
    pub fn scrim_at(&self, offset: f64, height: f64) -> f64 {
        let detents = self.detents(height);
        let dismiss = self.dismiss_offset(&detents, height);
        scrim_opacity_for_offset(offset, &detents.offsets, dismiss, detents.peek)
    }

    /// The sheet opened again: back to the tallest detent.
    pub fn reset(&mut self) {
        self.settled_offset = 0.;
        self.settled_layout_offset = 0.;
        self.settled_index = 0;
        self.settling_layout_offset = None;
        self.scroll_preservation_inset = 0.;
        self.pending_inset = None;
        self.settle_basis = None;
        self.drag = None;
        self.active_offset = 0.;
        self.touch_dismissal = None;
        self.resting_scrim = None;
    }

    /// Whether a drag is in progress.
    pub fn is_dragging(&self) -> bool {
        self.drag.is_some()
    }

    /// The layout split to render now.
    pub fn layout_offset(&self) -> f64 {
        match &self.drag {
            Some(drag) => drag.layout_offset,
            None => self
                .settling_layout_offset
                .unwrap_or(self.settled_layout_offset),
        }
    }

    fn prepare_settle(&mut self, split: Split, animate: bool) {
        let Split {
            base_layout,
            target_offset,
            target_layout,
            rendered_offset,
            layout_offset,
            natural_end_gap,
        } = split;
        let inset = preservation_inset(base_layout, target_layout, natural_end_gap);
        if animate && (rendered_offset - target_offset).abs() > 0.5 {
            self.pending_inset = Some(inset);
            self.settle_basis = Some((base_layout, natural_end_gap));
            self.settling_layout_offset = Some(layout_offset);
            return;
        }
        self.pending_inset = None;
        self.settle_basis = None;
        self.settling_layout_offset = None;
        self.scroll_preservation_inset = inset;
    }

    /// The transform-only settle landed: the layout takes the detent's split.
    pub fn complete_settle(&mut self) {
        self.settle_basis = None;
        self.settling_layout_offset = None;
        if let Some(inset) = self.pending_inset.take() {
            self.scroll_preservation_inset = inset;
        }
    }

    /// A drag begins at `coord` (the pointer's y) at `time`: from `start` when a body
    /// pull carries its travel over, with the contact's samples so far.
    pub fn begin(
        &mut self,
        pointer: PointerKind,
        coord: f64,
        time: f64,
        start: Option<f64>,
        tracker: Option<VelocityTracker>,
        natural_end_gap: f64,
    ) {
        let mut tracker = tracker.unwrap_or_default();
        tracker.add(0., coord, time);
        let base_layout = self.settled_layout_offset;
        self.drag = Some(DragState {
            pointer,
            start: start.unwrap_or(coord),
            tracker,
            height: self.sheet_height,
            reserve: self.reserve,
            base_offset: self.settled_offset,
            base_layout_offset: base_layout,
            rendered_offset: self.settled_offset,
            layout_offset: base_layout,
            natural_end_gap,
            has_traveled: false,
        });
        self.scroll_preservation_inset =
            preservation_inset(base_layout, base_layout, natural_end_gap);
        self.active_offset = self.settled_offset;
    }

    /// The pointer moved to `coord` at `time`.
    pub fn move_to(&mut self, coord: f64, time: f64) -> Option<Moved> {
        let detents = {
            let drag = self.drag.as_ref()?;
            self.detents(drag.height)
        };
        let reserve_now = self.reserve;
        let drag = self.drag.as_mut()?;
        let delta = coord - drag.start;
        drag.tracker.add(0., coord, time);
        let raw = drag.base_offset + delta;
        let max = *detents.offsets.last().unwrap_or(&0.);
        let next = if raw < 0. {
            if drag.pointer == PointerKind::Touch {
                // A touch device's reserve covers the whole stretch; the clamp binds
                // only where the reserve is short.
                (-drag.reserve).max(rubber_band_with(
                    raw,
                    drag.height - drag.reserve,
                    scroll::RUBBER_BAND_COEFFICIENT,
                ))
            } else {
                (-OVERSCROLL_MAX).max(raw * OVERSCROLL_RESISTANCE)
            }
        } else if raw > max {
            // In the dismiss zone: no magnet, so it doesn't fight a drag-to-close.
            raw
        } else {
            magnetize(raw, &detents.offsets)
        };
        // Above the base the full layout height comes back below the window.
        let layout = if next < drag.base_offset {
            0.
        } else {
            drag.base_layout_offset
        };
        drag.rendered_offset = next;
        drag.layout_offset = layout;
        let travel_started = !drag.has_traveled && next != drag.base_offset;
        if travel_started {
            drag.has_traveled = true;
        }
        let (base_layout, gap, height) =
            (drag.base_layout_offset, drag.natural_end_gap, drag.height);
        self.active_offset = next;
        self.scroll_preservation_inset = preservation_inset(base_layout, layout, gap);
        let _ = reserve_now;
        let dismiss = self.dismiss_offset(&detents, height);
        Some(Moved {
            offset: next,
            scrim: scrim_opacity_for_offset(next, &detents.offsets, dismiss, detents.peek),
            travel_started,
        })
    }

    /// The drag was interrupted (a pinch, a blur, a cancel): back to its detent.
    /// Returns the detent's scrim.
    pub fn cancel(&mut self) -> Option<f64> {
        let drag = self.drag.take()?;
        self.prepare_settle(
            Split {
                base_layout: drag.base_layout_offset,
                target_offset: drag.base_offset,
                target_layout: drag.base_layout_offset,
                rendered_offset: drag.rendered_offset,
                layout_offset: drag.layout_offset,
                natural_end_gap: drag.natural_end_gap,
            },
            true,
        );
        self.active_offset = drag.base_offset;
        Some(self.scrim_at(drag.base_offset, drag.height))
    }

    fn settle_at(&mut self, target: f64, drag: &DragState, detents: &Detents) -> f64 {
        let target_layout = if Some(target) == detents.peek {
            0.
        } else {
            target
        };
        self.prepare_settle(
            Split {
                base_layout: drag.base_layout_offset,
                target_offset: target,
                target_layout,
                rendered_offset: drag.rendered_offset,
                layout_offset: drag.layout_offset,
                natural_end_gap: drag.natural_end_gap,
            },
            true,
        );
        self.settled_layout_offset = target_layout.max(0.);
        self.settled_index = detents
            .offsets
            .iter()
            .position(|o| *o == target)
            .unwrap_or(0);
        self.settled_offset = target;
        self.active_offset = target;
        let dismiss = self.dismiss_offset(detents, drag.height);
        scrim_opacity_for_offset(target, &detents.offsets, dismiss, detents.peek)
    }

    /// The pointer lifted at `coord` at `time`.
    pub fn end(&mut self, coord: f64, time: f64) -> Option<Release> {
        let mut drag = self.drag.take()?;
        let delta = coord - drag.start;
        let offset = (drag.base_offset + delta).max(0.);
        let dir = if delta == 0. {
            0
        } else if delta > 0. {
            1
        } else {
            -1
        };
        // A lift at a new position is the last sample.
        if drag.tracker.last_sample().map(|s| s.y) != Some(coord) {
            drag.tracker.add(0., coord, time);
        }
        let velocity = drag.tracker.velocity(Some(time)).y;
        let height = if drag.height > 0. { drag.height } else { 1. };
        let detents = self.detents(height);
        let max = *detents.offsets.last().unwrap_or(&0.);
        let shortest = visible_height_for_offset(height, max, self.reserve);
        let dismiss = max + shortest * DISMISS_OVERSHOOT_RATIO;
        let travel = delta.abs();
        let is_touch = drag.pointer == PointerKind::Touch;
        let close = |this: &mut Self, exit: Option<SheetDragExit>| -> Release {
            if !this.can_dismiss {
                let scrim = this.settle_at(max, &drag, &detents);
                return Release::Settle {
                    offset: max,
                    velocity,
                    scrim,
                    touch: is_touch,
                };
            }
            this.prepare_settle(
                Split {
                    base_layout: drag.base_layout_offset,
                    target_offset: drag.base_offset,
                    target_layout: drag.base_layout_offset,
                    rendered_offset: drag.base_layout_offset,
                    layout_offset: drag.base_layout_offset,
                    natural_end_gap: drag.natural_end_gap,
                },
                false,
            );
            this.resting_scrim = Some(scrim_opacity_for_offset(
                drag.base_offset,
                &detents.offsets,
                dismiss,
                detents.peek,
            ));
            this.touch_dismissal = Some(TouchDismissal {
                is_touch,
                pending: true,
                exit,
            });
            this.active_offset = offset;
            Release::Dismiss { exit }
        };
        if is_touch {
            // UIKit's sheet values, with closed counted as one position. A release
            // that moves back toward where the drag began cancels to that detent.
            let mut points = detents.offsets.clone();
            points.push(max + shortest);
            let index = resolve_snap_point(&SnapPointQuery {
                points: points.clone(),
                position: offset,
                velocity,
                flick_threshold: if travel > DRAG_PROMOTION_SLOP {
                    sheet::release::FAST_SPEED
                } else {
                    f64::INFINITY
                },
                tolerance: sheet::release::DETENT_TOLERANCE,
                flick_past_projection: true,
                projection: Some(SnapPointProjection {
                    seconds: sheet::release::PROJECTION_SECONDS,
                    min_speed: sheet::release::MIN_PROJECTION_SPEED,
                }),
                start: Some(drag.base_offset),
                ..Default::default()
            })
            .map_or(0, |r| r.index);
            if index == points.len() - 1 {
                return Some(close(
                    self,
                    Some(SheetDragExit {
                        distance: (height - offset).max(0.),
                        velocity,
                    }),
                ));
            }
            let target = points[index];
            let scrim = self.settle_at(target, &drag, &detents);
            return Some(Release::Settle {
                offset: target,
                velocity,
                scrim,
                touch: true,
            });
        }
        let projected = if travel > PROJECTION_MIN_TRAVEL {
            offset + velocity / 1000. * PROJECTION_FACTOR
        } else {
            offset
        };
        // A release that would coast past the dismiss line closes the sheet.
        if dir > 0 && projected > dismiss {
            return Some(close(self, None));
        }
        let target = resolve_settle_offset(
            projected.clamp(0., max),
            &detents.offsets,
            dir,
            drag.base_offset,
        );
        let scrim = self.settle_at(target, &drag, &detents);
        Some(Release::Settle {
            offset: target,
            velocity,
            scrim,
            touch: false,
        })
    }

    /// The owner's answer to the last close request, read each frame: `requested` is
    /// its controlled open state. Returns the scrim to show again when it refused.
    pub fn answer(&mut self, requested: bool) -> Option<f64> {
        let dismissal = self.touch_dismissal?;
        if dismissal.pending {
            if requested {
                // Refused: the sheet springs back, its scrim with it.
                self.touch_dismissal = None;
                self.active_offset = self.settled_offset;
                return self.resting_scrim.take();
            }
            self.touch_dismissal = Some(TouchDismissal {
                pending: false,
                ..dismissal
            });
        } else if requested {
            self.touch_dismissal = None;
        }
        None
    }

    /// Whether the sheet's current dismissal came from a touch drag's release.
    pub fn is_dismissed_by_touch_drag(&self) -> bool {
        self.touch_dismissal
            .is_some_and(|d| !d.pending && d.is_touch)
    }

    /// The exit a touch drag's release started, once the owner took the close.
    pub fn drag_exit(&self) -> Option<SheetDragExit> {
        self.touch_dismissal
            .filter(|d| !d.pending)
            .and_then(|d| d.exit)
    }

    /// Re-resolves the settled detent by index after a resize or new snap points,
    /// with no animation: `height` is the fully open height now. Returns the new
    /// offset and scrim when anything changed.
    pub fn reanchor(&mut self, height: f64, natural_end_gap: f64) -> Option<(f64, f64)> {
        if self.drag.is_some() || height <= 0. {
            return None;
        }
        let detents = self.detents(height);
        let index = self.settled_index.min(detents.offsets.len() - 1);
        let target = detents.offsets[index];
        let target_layout = if Some(target) == detents.peek {
            0.
        } else {
            target
        };
        let base_layout = self.settled_layout_offset;
        if height == self.sheet_height
            && (target - self.active_offset).abs() <= 0.5
            && (target_layout - base_layout).abs() <= 0.5
        {
            return None;
        }
        self.sheet_height = height;
        self.settled_index = index;
        let basis = self.settle_basis;
        let settling = self.settling_layout_offset;
        // The settle keeps its layout split until it lands; under that split the
        // sheet can rise only as far as the reserve below the window.
        let can_retarget =
            matches!((settling, basis), (Some(s), Some(_)) if target - s >= -self.reserve);
        if can_retarget {
            let (b, gap) = basis.unwrap_or_default();
            self.pending_inset = Some(preservation_inset(b, target_layout, gap));
        } else {
            let (b, gap) = basis.unwrap_or((base_layout, natural_end_gap));
            self.prepare_settle(
                Split {
                    base_layout: b,
                    target_offset: target,
                    target_layout,
                    rendered_offset: target,
                    layout_offset: settling.unwrap_or(base_layout),
                    natural_end_gap: gap,
                },
                false,
            );
        }
        self.settled_layout_offset = target_layout.max(0.);
        self.settled_offset = target;
        self.active_offset = target;
        let dismiss = self.dismiss_offset(&detents, height);
        Some((
            target,
            scrim_opacity_for_offset(target, &detents.offsets, dismiss, detents.peek),
        ))
    }

    /// A settle that no drag started (`settle_to`-like moves within the model).
    pub fn is_settling(&self) -> bool {
        self.settling_layout_offset.is_some()
    }
}
