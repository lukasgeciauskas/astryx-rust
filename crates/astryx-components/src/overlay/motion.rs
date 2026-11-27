//! What every anchored layer shares: its entrance, its gutters and its
//! clearance (upstream `layerAnimations`, `layerViewportInset`).

use astryx_primitives::{Placement, prefers_reduced_motion};
use astryx_theme::Tokens;
use gpui::{App, Div, Pixels, Stateful, Styled, Window, px};
use web_time::Instant;

/// The gap a layer keeps from its anchor: the system's standard clearance,
/// `--spacing-1`.
pub(crate) fn clearance(t: &Tokens, window: &Window) -> Pixels {
    t.spacing.s1.to_pixels(window.rem_size())
}

/// The gutter a layer keeps from the viewport's edges, `--spacing-4`.
pub(crate) fn gutter(t: &Tokens, window: &Window) -> Pixels {
    t.spacing.s4.to_pixels(window.rem_size())
}

/// The largest a layer may be: the viewport less its gutters.
pub(crate) fn viewport_cap(t: &Tokens, window: &Window) -> gpui::Size<Pixels> {
    let gutter = gutter(t, window);
    let viewport = window.viewport_size();
    gpui::size(
        (viewport.width - gutter * 2.).max(px(0.)),
        (viewport.height - gutter * 2.).max(px(0.)),
    )
}

/// A layer's entrance from `since`: a fade with a short slide from its
/// anchor's side and a grow from 95%, over `--duration-fast-max`. Under
/// reduced motion a pointer sees no motion and a finger a fade.
pub(crate) fn entrance(
    el: Stateful<Div>,
    since: Instant,
    side: Placement,
    rtl: bool,
    t: &Tokens,
    window: &mut Window,
    cx: &App,
) -> Stateful<Div> {
    let reduced = prefers_reduced_motion(cx);
    // Under Reduce Motion a coarse primary pointer cross-fades (upstream
    // `(prefers-reduced-motion: reduce) and (pointer: coarse)`).
    let touch = astryx_primitives::modality::coarse_primary_pointer_now(window, cx);
    if reduced && !touch {
        return el;
    }
    let now = cx.background_executor().now();
    let duration = t.duration.fast_max;
    let progress = if duration.is_zero() {
        1.
    } else {
        (now.saturating_duration_since(since).as_secs_f32() / duration.as_secs_f32()).min(1.)
    };
    if progress < 1. {
        window.request_animation_frame();
    }
    let eased = t.ease.standard.ease(progress);
    let el = el.opacity(eased);
    if reduced {
        return el;
    }
    let step = t.spacing.s2.to_pixels(window.rem_size()) * (1. - eased);
    let (dx, dy) = match (side, rtl) {
        (Placement::Above, _) => (px(0.), step),
        (Placement::Below, _) => (px(0.), -step),
        (Placement::End, false) | (Placement::Start, true) => (-step, px(0.)),
        (Placement::Start, false) | (Placement::End, true) => (step, px(0.)),
    };
    el.translate(dx, dy).scale(0.95 + 0.05 * eased)
}

// ---------------------------------------------------------------------------
// The touch presentation's growth (upstream Popover/popoverSurfaceFrame.ts,
// popoverSurfaceMotion.ts and Layer/layerEntranceSide.ts)
// ---------------------------------------------------------------------------

/// Upstream `popover.collapsedScale`: the uniform scale a popover grows from (UIKit).
pub(crate) const POPOVER_COLLAPSED_SCALE: f32 = 0.01;
/// Upstream `popover.collapsedLengthPx`: its length away from the source as it starts.
pub(crate) const POPOVER_COLLAPSED_LENGTH_PX: f32 = 100.;
/// Upstream `ENTRANCE_OPACITY_FLOOR`: the lowest opacity a growth fades in from.
pub(crate) const ENTRANCE_OPACITY_FLOOR: f32 = 0.001;

/// A box in window px.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SurfaceBox {
    pub left: f32,
    pub top: f32,
    pub width: f32,
    pub height: f32,
}

impl SurfaceBox {
    pub(crate) fn from_bounds(b: gpui::Bounds<Pixels>) -> Self {
        SurfaceBox {
            left: f32::from(b.origin.x),
            top: f32::from(b.origin.y),
            width: f32::from(b.size.width),
            height: f32::from(b.size.height),
        }
    }
}

/// Upstream `LayerEntranceSide`: a step toward the anchor on each axis, and where the
/// layer meets it, px from the layer's top-left.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct EntranceSide {
    pub x: i8,
    pub y: i8,
    pub origin_x: f32,
    pub origin_y: f32,
}

/// Upstream `layerEntranceSide`: the side a layer sits on against its anchor; `None`
/// for a layer without size. A layer over its anchor's center has no step.
pub(crate) fn layer_entrance_side(layer: SurfaceBox, anchor: SurfaceBox) -> Option<EntranceSide> {
    if layer.width <= 0. || layer.height <= 0. {
        return None;
    }
    let anchor_x = anchor.left + anchor.width / 2.;
    let anchor_y = anchor.top + anchor.height / 2.;
    let origin_x = (anchor_x - layer.left).clamp(0., layer.width);
    let origin_y = (anchor_y - layer.top).clamp(0., layer.height);
    let covers =
        origin_x > 0. && origin_x < layer.width && origin_y > 0. && origin_y < layer.height;
    if covers {
        return Some(EntranceSide {
            x: 0,
            y: 0,
            origin_x,
            origin_y,
        });
    }
    let apart_x = (layer.left + layer.width / 2. - anchor_x) / ((layer.width + anchor.width) / 2.);
    let apart_y =
        (layer.top + layer.height / 2. - anchor_y) / ((layer.height + anchor.height) / 2.);
    if apart_y.abs() >= apart_x.abs() {
        let below = apart_y > 0.;
        return Some(EntranceSide {
            x: 0,
            y: if below { -1 } else { 1 },
            origin_x,
            origin_y: if below { 0. } else { layer.height },
        });
    }
    let right = apart_x > 0.;
    Some(EntranceSide {
        x: if right { -1 } else { 1 },
        y: 0,
        origin_x: if right { 0. } else { layer.width },
        origin_y,
    })
}

/// The scale axis that also carries the length.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GrowAxis {
    X,
    Y,
}

/// Upstream `PopoverCollapsedState`: where the popover grows from, and its first shape.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct CollapsedState {
    pub origin_x: f32,
    pub origin_y: f32,
    pub axis: GrowAxis,
    pub scale: f32,
    pub length: f32,
}

/// Upstream `popoverCollapsedState`: the middle of the source on the popover's edge
/// facing it, kept on that edge; a popover over its source's center grows from that
/// center by scale only.
pub(crate) fn popover_collapsed_state(surface: SurfaceBox, source: SurfaceBox) -> CollapsedState {
    let side = layer_entrance_side(surface, source);
    let horizontal = side.is_some_and(|s| s.x != 0);
    let over_source = side.is_some_and(|s| s.x == 0 && s.y == 0);
    let full = if horizontal {
        surface.width
    } else {
        surface.height
    };
    CollapsedState {
        origin_x: side.map_or(0., |s| s.origin_x),
        origin_y: side.map_or(0., |s| s.origin_y),
        axis: if horizontal { GrowAxis::X } else { GrowAxis::Y },
        scale: POPOVER_COLLAPSED_SCALE,
        length: if !over_source && full > 0. {
            (POPOVER_COLLAPSED_LENGTH_PX / full).min(1.)
        } else {
            1.
        },
    }
}

/// Upstream `SurfaceState`: a scale, a share of the full length, an opacity.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SurfaceState {
    pub scale: f32,
    pub length: f32,
    pub opacity: f32,
}

/// Upstream `PRESENTED`.
pub(crate) const PRESENTED: SurfaceState = SurfaceState {
    scale: 1.,
    length: 1.,
    opacity: 1.,
};

/// Upstream `PopoverFrame`: the surface's scale on each axis and its opacity.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PopoverFrame {
    pub scale_x: f32,
    pub scale_y: f32,
    pub opacity: f32,
}

/// Upstream `frameOf`.
pub(crate) fn frame_of(state: SurfaceState, axis: GrowAxis) -> PopoverFrame {
    let along = state.scale * state.length;
    PopoverFrame {
        scale_x: if axis == GrowAxis::X {
            along
        } else {
            state.scale
        },
        scale_y: if axis == GrowAxis::Y {
            along
        } else {
            state.scale
        },
        opacity: state.opacity,
    }
}

/// Upstream `popoverFrame`: the surface at progress `p` of the growth, UIKit's path
/// (the frames [`SurfaceMotion`] plays, checked against UIKit's).
#[cfg(test)]
pub(crate) fn popover_frame(state: CollapsedState, p: f32) -> PopoverFrame {
    frame_of(
        SurfaceState {
            scale: state.scale + (1. - state.scale) * p,
            length: state.length + (1. - state.length) * p,
            opacity: p,
        },
        state.axis,
    )
}

/// One step between two surface states at eased progress `e` (unclamped, so an
/// overshooting easing carries the channels), with an opacity floor.
pub(crate) fn surface_between(
    from: SurfaceState,
    to: SurfaceState,
    e: f32,
    floor: f32,
) -> SurfaceState {
    let lerp = |a: f32, b: f32| a + (b - a) * e;
    SurfaceState {
        scale: lerp(from.scale, to.scale),
        length: lerp(from.length, to.length),
        opacity: lerp(from.opacity, to.opacity).max(floor),
    }
}

/// Upstream `createPopoverSurfaceMotion`'s state for one popover: the growth under a
/// finger out of its source, and the shrink back into it. Under Reduce Motion on a coarse
/// pointer it cross-fades in at full size and out where it stands.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct SurfaceMotion {
    /// The growth's start, with the state it starts from (a reopen during the shrink
    /// turns back from the frame on screen).
    pub growing: Option<(Instant, SurfaceState)>,
    /// The shrink's start, with the state it starts from.
    pub shrinking: Option<(Instant, SurfaceState)>,
    pub collapsed: Option<CollapsedState>,
    /// What the last frame drew.
    pub shown: Option<SurfaceState>,
}

/// The timing a growth or a shrink plays.
#[derive(Clone, Copy, Debug)]
pub(crate) struct SurfaceTiming {
    pub enter: (std::time::Duration, astryx_theme::CubicBezier),
    pub exit: (std::time::Duration, astryx_theme::CubicBezier),
    /// Under Reduce Motion: a 200 ms cross-fade at full size.
    pub reduced: bool,
}

fn eased(
    since: Instant,
    now: Instant,
    (duration, ease): (std::time::Duration, astryx_theme::CubicBezier),
) -> (f32, bool) {
    if duration.is_zero() {
        return (1., false);
    }
    let p = (now.saturating_duration_since(since).as_secs_f32() / duration.as_secs_f32()).min(1.);
    (ease.ease(p), p < 1.)
}

impl SurfaceMotion {
    /// The popover opened under a finger at `now`.
    pub(crate) fn enter(&mut self, now: Instant) {
        let from = match (self.shrinking.take(), self.shown) {
            (Some(_), Some(shown)) => shown,
            _ => SurfaceState {
                scale: POPOVER_COLLAPSED_SCALE,
                length: 1.,
                opacity: 0.,
            },
        };
        let reopen = self.shown.is_some() && from.opacity > 0.;
        if !reopen {
            self.collapsed = None;
        }
        self.growing = Some((
            now,
            if reopen {
                from
            } else {
                SurfaceState {
                    opacity: 0.,
                    ..from
                }
            },
        ));
    }

    /// The popover closed at `now`: shrink from the frame on screen. False when it never
    /// presented (nothing to dismiss).
    pub(crate) fn exit(&mut self, now: Instant) -> bool {
        self.growing = None;
        match (self.shown, self.collapsed) {
            (Some(shown), Some(_)) if self.shrinking.is_none() => {
                self.shrinking = Some((now, shown));
                true
            }
            _ => self.shrinking.is_some(),
        }
    }

    /// The frame at `now`, the surface's box measured (`geometry`: the surface and its
    /// source), and whether it still moves. `None` while nothing is to be drawn (a shrink
    /// that ended).
    pub(crate) fn frame(
        &mut self,
        geometry: Option<(SurfaceBox, SurfaceBox)>,
        timing: SurfaceTiming,
        now: Instant,
    ) -> Option<(PopoverFrame, (f32, f32), bool)> {
        if self.collapsed.is_none()
            && let Some((surface, source)) = geometry
        {
            self.collapsed = Some(popover_collapsed_state(surface, source));
        }
        let origin = |c: Option<CollapsedState>, g: Option<(SurfaceBox, SurfaceBox)>| match (c, g) {
            (Some(c), Some((s, _))) if s.width > 0. && s.height > 0. => {
                (c.origin_x / s.width, c.origin_y / s.height)
            }
            _ => (0.5, 0.5),
        };
        if let Some((since, from)) = self.shrinking {
            let collapsed = self.collapsed?;
            let (e, moving) = eased(since, now, timing.exit);
            let to = if timing.reduced {
                SurfaceState {
                    opacity: 0.,
                    ..from
                }
            } else {
                SurfaceState {
                    scale: collapsed.scale,
                    length: collapsed.length,
                    opacity: 0.,
                }
            };
            let state = surface_between(from, to, e, 0.);
            if !moving {
                *self = SurfaceMotion::default();
                return None;
            }
            self.shown = Some(state);
            return Some((
                frame_of(state, collapsed.axis),
                origin(self.collapsed, geometry),
                true,
            ));
        }
        let (since, from) = self.growing?;
        let Some(collapsed) = self.collapsed else {
            // Not measured yet: nothing shows until the frame that measures it.
            return Some((
                PopoverFrame {
                    scale_x: 1.,
                    scale_y: 1.,
                    opacity: 0.,
                },
                (0.5, 0.5),
                true,
            ));
        };
        let from = if from.opacity == 0. && !timing.reduced {
            SurfaceState {
                scale: collapsed.scale,
                length: collapsed.length,
                opacity: 0.,
            }
        } else if timing.reduced {
            SurfaceState {
                scale: 1.,
                length: 1.,
                ..from
            }
        } else {
            from
        };
        let (e, moving) = eased(since, now, timing.enter);
        let state = surface_between(from, PRESENTED, e, ENTRANCE_OPACITY_FLOOR);
        self.shown = Some(state);
        Some((
            frame_of(state, collapsed.axis),
            origin(self.collapsed, geometry),
            moving,
        ))
    }
}

#[cfg(test)]
#[path = "surface_motion_tests.rs"]
mod surface_motion_tests;
