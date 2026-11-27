//! The Reduce Motion policy for motion beside the animator, upstream
//! `motion/reducedMotion.ts` (design:reduced-motion): a change becomes an
//! opacity-only cross-fade of `timing.reducedMotionCrossFadeMs` (200 ms) on
//! `timing.curves.move` on a coarse primary pointer or for a touch release
//! (DR2), and completes at once on any other pointer (DR3). Nothing
//! lengthens the fade (DR4). The person's setting is read by the caller
//! (`rustryx_primitives::prefers_reduced_motion`, or a theme's mark).

use super::bezier::{BezierPoints, cubic_bezier};
use super::uikit::timing::REDUCED_MOTION_CROSS_FADE;

/// Reduce Motion on a coarse primary pointer: a cross-fade (upstream
/// `REDUCED_MOTION_FADE_QUERY`).
pub const REDUCED_MOTION_FADE_QUERY: &str =
    "(prefers-reduced-motion: reduce) and (pointer: coarse)";
/// Reduce Motion on any other primary pointer: at once (upstream
/// `REDUCED_MOTION_INSTANT_QUERY`).
pub const REDUCED_MOTION_INSTANT_QUERY: &str =
    "(prefers-reduced-motion: reduce) and (not (pointer: coarse))";

/// The fade to play under Reduce Motion (upstream `ReducedMotionFade`):
/// `duration_ms` 0 means at once.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ReducedMotionFade {
    pub duration_ms: f64,
    /// The easing's control points (linear for an instant change).
    pub curve: BezierPoints,
}

impl ReducedMotionFade {
    /// The easing as CSS text, as upstream gives it.
    pub fn easing(&self) -> String {
        if self.duration_ms == 0. {
            return "linear".into();
        }
        let parts: Vec<String> = self.curve.iter().map(|n| format!("{n}")).collect();
        format!("cubic-bezier({})", parts.join(", "))
    }

    /// The eased progress at time progress `x`.
    pub fn ease(&self, x: f64) -> f64 {
        cubic_bezier(self.curve).ease(x)
    }
}

/// The Reduce Motion fade for a change now, for motion the animator does not
/// drive (upstream `reducedMotionFade`): `None` when motion is not reduced,
/// else the 200 ms cross-fade on a coarse pointer or for a touch release, or
/// an instant change on any other pointer.
pub fn reduced_motion_fade(
    reduced: bool,
    coarse_pointer: bool,
    touch_release: bool,
) -> Option<ReducedMotionFade> {
    if !reduced {
        return None;
    }
    if !(touch_release || coarse_pointer) {
        return Some(ReducedMotionFade {
            duration_ms: 0.,
            curve: [0., 0., 1., 1.],
        });
    }
    Some(ReducedMotionFade {
        duration_ms: REDUCED_MOTION_CROSS_FADE.duration_ms,
        curve: REDUCED_MOTION_CROSS_FADE.curve,
    })
}

/// A Reduce Motion dip (upstream `playReducedMotionDip`): a fade out over
/// half the fade, the change landing at its end, however late, then the fade
/// back in over the other half. The caller lands the change when
/// [`is_due`](Self::is_due) and reports it with [`landed`](Self::landed);
/// the fade out holds 0 until then.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ReducedMotionDip {
    fade: ReducedMotionFade,
    from: f64,
    /// When the fade out started, once it has.
    start: Option<f64>,
    landed: bool,
    fade_in_start: Option<f64>,
    cancelled: bool,
}

impl ReducedMotionDip {
    /// A dip on `fade` from opacity `from` (a leaving element fades out from
    /// where it is), its fade out starting at `start` (`None` until its frame).
    pub fn play(fade: ReducedMotionFade, from: f64, start: Option<f64>) -> Self {
        Self {
            fade,
            from,
            start,
            landed: false,
            fade_in_start: None,
            cancelled: false,
        }
    }

    fn half(&self) -> f64 {
        self.fade.duration_ms / 2.
    }

    /// Dates the fade out at its frame.
    pub fn started(&mut self, at: f64) {
        self.start.get_or_insert(at);
    }

    /// Whether the fade out has ended and the change should land now.
    pub fn is_due(&self, now: f64) -> bool {
        !self.cancelled
            && !self.landed
            && self.start.is_some_and(|start| now >= start + self.half())
    }

    /// The change landed; `now` is the clock after the landing's own work.
    /// The fade back in starts where the fade out ended, so the dip keeps
    /// its length, and after a long stall half of it still plays.
    pub fn landed(&mut self, now: f64) {
        if self.cancelled || self.landed {
            return;
        }
        self.landed = true;
        let half = self.half();
        self.fade_in_start = Some(match self.start {
            Some(start) => (start + half).max(now - half / 2.),
            None => now,
        });
    }

    /// When the fade back in starts, once the change has landed.
    pub fn fade_in_start(&self) -> Option<f64> {
        self.fade_in_start
    }

    /// Stops the dip: a change that has not landed never lands.
    pub fn cancel(&mut self) {
        self.cancelled = true;
    }

    /// Whether the dip was cancelled.
    pub fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    /// The opacity the dip shows at `time`: 1 once cancelled or done.
    pub fn opacity_at(&self, time: f64) -> f64 {
        if self.cancelled {
            return 1.;
        }
        let half = self.half();
        if let Some(fade_in) = self.fade_in_start {
            if time < fade_in {
                return 0.;
            }
            let p = if half > 0. {
                ((time - fade_in) / half).min(1.)
            } else {
                1.
            };
            return self.fade.ease(p);
        }
        match self.start {
            None => self.from,
            Some(start) => {
                let p = if half > 0. {
                    ((time - start) / half).clamp(0., 1.)
                } else {
                    1.
                };
                self.from * (1. - self.fade.ease(p))
            }
        }
    }
}
