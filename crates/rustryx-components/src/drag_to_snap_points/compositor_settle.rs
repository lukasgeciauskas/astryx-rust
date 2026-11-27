//! A settle the compositor plays, upstream `dragToSnapPoints/compositorSettle.ts`:
//! one `translate` per moving part, with its callbacks timed from the same curve.
//!
//! The caller writes the settle's end on the drawn value first, so each part's own
//! style draws the end pose; each part then plays a `translate` from its offset at
//! the start back to none. Upstream hands those to the browser's compositor as
//! keyframes sampled at 120 Hz (plus a keyframe at each bend of a part's offset);
//! GPUI draws every frame from the curve itself, so each part reads the curve at the
//! frame's own time, which no keyframe can cut a corner of.

use std::rc::Rc;

use rustryx_primitives::motion::{MotionProperty, SpringCurve};

use super::snap_drag::SnapElement;

/// The longest a settle plays, ms.
const MAX_MS: f64 = 3000.;

/// A moving part, and its own offset (px) at a value of the drag's axis (upstream
/// `CompositorPart`).
#[derive(Clone)]
pub(crate) struct CompositorPart {
    pub element: SnapElement,
    pub offset: Rc<dyn Fn(f64) -> f64>,
}

/// A call made once, on the first frame whose value is past `offset` in
/// `direction` (1: at or above it, -1: at or below it).
#[derive(Clone)]
pub(crate) struct OnPass {
    pub offset: f64,
    pub direction: f64,
    pub call: Rc<dyn Fn()>,
}

/// A settle in flight (upstream `CompositorSettle`).
pub(crate) struct CompositorSettle {
    curve: SpringCurve,
    parts: Vec<CompositorPart>,
    on_pass: Option<OnPass>,
    passed: std::cell::Cell<bool>,
    started: f64,
    duration_ms: f64,
    stopped: std::cell::Cell<bool>,
}

/// Plays `curve` on `parts` from `now` (upstream `playCompositorSettle`).
pub(crate) fn play_compositor_settle(
    curve: SpringCurve,
    parts: Vec<CompositorPart>,
    on_pass: Option<OnPass>,
    now: f64,
) -> CompositorSettle {
    // The animator's own rest threshold for a length.
    let duration_ms =
        (curve.settle_time(MotionProperty::X.settle_threshold()) * 1000.).clamp(0., MAX_MS);
    let settle = CompositorSettle {
        passed: std::cell::Cell::new(on_pass.is_none()),
        curve,
        parts,
        on_pass,
        started: now,
        duration_ms,
        stopped: std::cell::Cell::new(false),
    };
    // Until the first frame, each part holds its start pose, not the end its style
    // already draws (upstream's `fill: 'backwards'`).
    settle.draw_parts(0.);
    settle
}

impl CompositorSettle {
    fn elapsed_seconds(&self, now: f64) -> f64 {
        (now - self.started).clamp(0., self.duration_ms) / 1000.
    }

    fn draw_parts(&self, seconds: f64) {
        let value = self.curve.value(seconds);
        let end = self.curve.to;
        for part in &self.parts {
            let shift = (part.offset)(value) - (part.offset)(end);
            part.element
                .set_composited((shift != 0.).then_some((shift, 0.)));
        }
    }

    fn clear_parts(&self) {
        for part in &self.parts {
            part.element.set_composited(None);
        }
    }

    /// The value on screen at `now`.
    pub fn presented(&self, now: f64) -> f64 {
        self.curve.value(self.elapsed_seconds(now))
    }

    /// The value's velocity on screen at `now` (units/s).
    pub fn velocity(&self, now: f64) -> f64 {
        self.curve.velocity(self.elapsed_seconds(now))
    }

    /// The settle's whole duration, ms.
    #[cfg(test)]
    pub fn duration_ms(&self) -> f64 {
        self.duration_ms
    }

    /// Stops the settle where it is on screen, and returns that value.
    pub fn stop(&self, now: f64) -> f64 {
        let value = self.presented(now);
        self.stopped.set(true);
        self.clear_parts();
        value
    }

    /// A frame at `now`: `on_pass` on the first frame past its offset, the parts
    /// drawn, and true on the first frame after the curve ends (the parts stop).
    pub fn frame(&self, now: f64) -> bool {
        if self.stopped.get() {
            return false;
        }
        let seconds = self.elapsed_seconds(now);
        if !self.passed.get()
            && let Some(on_pass) = &self.on_pass
        {
            let value = self.curve.value(seconds);
            if (value - on_pass.offset) * on_pass.direction >= 0. {
                self.passed.set(true);
                (on_pass.call)();
                if self.stopped.get() {
                    return false;
                }
            }
        }
        if seconds * 1000. >= self.duration_ms {
            self.stopped.set(true);
            self.clear_parts();
            return true;
        }
        self.draw_parts(seconds);
        false
    }
}
