//! A motion engine on the window's clock, for components that drive their own channels
//! frame by frame (SegmentedControl's lens, Lightbox's settles, the reorder shift), and
//! the springs a glide area can play: upstream's `springGlide`, a glide on a spring's
//! progress instead of the theme's curve tokens (a gesture's release, AST-105 FR4).

use std::rc::Rc;
use std::time::Duration;

use astryx_primitives::motion::{
    EngineOptions, MotionCurve, MotionEngine, MotionSpring, SETTLING_EPSILON,
    progress::progress_curve, resolve_spring,
};
use astryx_primitives::{coarse_primary_pointer, prefers_reduced_motion};
use gpui::{App, Window};
use web_time::Instant;

/// A [`MotionEngine`] kept across frames, its clock the executor's (which tests advance).
#[derive(Clone)]
pub(crate) struct FrameEngine {
    engine: MotionEngine,
    origin: Instant,
}

impl FrameEngine {
    /// A new engine at rest on `executor`'s clock.
    pub(crate) fn new(executor: gpui::BackgroundExecutor) -> Self {
        let origin = executor.now();
        let clock = executor;
        let now: Rc<dyn Fn() -> f64> =
            Rc::new(move || clock.now().saturating_duration_since(origin).as_secs_f64() * 1000.);
        FrameEngine {
            engine: MotionEngine::new(EngineOptions {
                now: Some(now),
                frames: true,
                ..Default::default()
            }),
            origin,
        }
    }

    /// The engine itself.
    pub(crate) fn engine(&self) -> &MotionEngine {
        &self.engine
    }

    /// The engine's time now, ms.
    pub(crate) fn now(&self, cx: &App) -> f64 {
        cx.background_executor()
            .now()
            .saturating_duration_since(self.origin)
            .as_secs_f64()
            * 1000.
    }

    /// Reduce Motion, the pointer and the theme's motion tempo, as the calls plan
    /// against them.
    pub(crate) fn sync_environment(&self, window: &Window, cx: &mut App) {
        self.engine.set_reduced_motion(prefers_reduced_motion(cx));
        self.engine
            .set_coarse_pointer(coarse_primary_pointer(window, cx));
        self.engine
            .set_tempo(astryx_theme::current(window, cx).motion_tempo());
    }

    /// A frame: runs what came due, starts the calls waiting for a frame, and asks for the
    /// next frame while anything moves. Returns whether anything moves.
    pub(crate) fn drive(&self, window: &mut Window, cx: &App) -> bool {
        let now = self.now(cx);
        self.engine.advance(now);
        self.engine.frame(now);
        let moving = !self.engine.is_settled();
        if moving {
            window.request_animation_frame();
        }
        moving
    }
}

/// A progress curve from 0 to 1 over a glide's time, as a glide area plays it.
pub(crate) type GlideEase = Rc<dyn Fn(f32) -> f32>;

/// A glide on `spring`'s progress (upstream `springGlide`): the duration it takes to
/// settle and its progress from 0 to 1 over that time, starting with `velocity` in
/// progress per second (a release's speed over the distance it has left). A role's
/// spring plays at the theme's motion `tempo` (`ResolvedTheme::motion_tempo`).
pub(crate) fn spring_glide(
    spring: MotionSpring,
    velocity: f64,
    tempo: f64,
) -> (Duration, GlideEase) {
    let resolved = resolve_spring(spring, tempo);
    let curve = progress_curve(resolved, velocity, Some(SETTLING_EPSILON));
    let seconds = curve.duration().max(0.);
    let ease: GlideEase = Rc::new(move |t: f32| {
        if seconds <= 0. {
            1.
        } else {
            curve.at(f64::from(t) * seconds) as f32
        }
    });
    (Duration::from_secs_f64(seconds), ease)
}
