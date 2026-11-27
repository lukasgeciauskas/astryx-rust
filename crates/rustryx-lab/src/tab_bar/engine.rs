//! A motion engine on the window's clock, for the lab components that drive their own
//! layers frame by frame (TabBar's lens, NavigationStack's cards): Core's animator
//! (`rustryx_primitives::motion::MotionEngine`, upstream `createMotionEngine`) with the
//! executor's clock, which tests advance.

use std::rc::Rc;

use gpui::{App, Window};
use rustryx_primitives::motion::{
    AnimateOptions, EngineOptions, MotionEngine, MotionProperty, MotionValues, SpringDescription,
    SpringTransition,
};
use rustryx_primitives::{coarse_primary_pointer, prefers_reduced_motion};
use web_time::Instant;

use super::model::UikitSpring;

/// A UIKit spring (response, damping ratio) as Core's animator takes it.
pub(crate) fn spring(s: UikitSpring) -> SpringDescription {
    SpringDescription::ResponseDamping {
        response: s.response,
        damping_ratio: s.damping_ratio,
    }
}

/// A [`MotionEngine`] kept across frames on the executor's clock.
#[derive(Clone)]
pub(crate) struct FrameEngine {
    engine: MotionEngine,
    origin: Instant,
}

impl FrameEngine {
    /// An engine at rest, starting from `initial`.
    pub(crate) fn new(executor: gpui::BackgroundExecutor, initial: MotionValues) -> Self {
        let origin = executor.now();
        let clock: Rc<dyn Fn() -> f64> = Rc::new(move || {
            executor
                .now()
                .saturating_duration_since(origin)
                .as_secs_f64()
                * 1000.
        });
        FrameEngine {
            engine: MotionEngine::new(EngineOptions {
                initial,
                now: Some(clock),
                frames: true,
            }),
            origin,
        }
    }

    /// The engine's time now, ms.
    pub(crate) fn now(&self, cx: &App) -> f64 {
        cx.background_executor()
            .now()
            .saturating_duration_since(self.origin)
            .as_secs_f64()
            * 1000.
    }

    /// Reduce Motion and the pointer, as the calls plan against them.
    pub(crate) fn sync(&self, window: &Window, cx: &mut App) {
        self.engine.set_reduced_motion(prefers_reduced_motion(cx));
        self.engine
            .set_coarse_pointer(coarse_primary_pointer(window, cx));
    }

    /// A frame: runs what came due and starts the calls waiting for one. Whether
    /// anything still moves.
    pub(crate) fn drive(&self, cx: &App) -> bool {
        let now = self.now(cx);
        self.engine.advance(now);
        self.engine.frame(now);
        !self.engine.is_settled()
    }

    /// A property's value now.
    pub(crate) fn get(&self, property: MotionProperty, cx: &App) -> f64 {
        self.engine.get(property, Some(self.now(cx)))
    }

    /// A property's velocity now, per second.
    pub(crate) fn velocity(&self, property: MotionProperty, cx: &App) -> f64 {
        self.engine.get_velocity(property, Some(self.now(cx)))
    }

    /// The opacity the engine shows now (a Reduce Motion cross-fade's).
    pub(crate) fn shown_opacity(&self, cx: &App) -> f64 {
        self.engine.shown_opacity(Some(self.now(cx)))
    }

    /// Sets values at once.
    pub(crate) fn set(&self, values: MotionValues) {
        self.engine.set(values);
    }

    /// Springs to `values`.
    pub(crate) fn animate(
        &self,
        values: MotionValues,
        transition: SpringTransition,
        options: AnimateOptions,
    ) {
        let _ = self.engine.animate(values, transition, options);
    }

    /// What Reduce Motion makes of a call now.
    pub(crate) fn reduced(&self, touch_release: bool) -> bool {
        self.engine.reduced_motion(touch_release) != rustryx_primitives::motion::ReducedMotion::Off
    }

    /// Whether everything rests.
    pub(crate) fn at_rest(&self) -> bool {
        self.engine.is_settled()
    }

    /// Stops every property where it is.
    pub(crate) fn stop(&self) {
        self.engine.stop(None);
    }
}
