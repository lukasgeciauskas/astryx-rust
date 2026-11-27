//! The animator in GPUI: upstream `animator/createAnimator.ts` and `hooks/useAnimator.ts`
//! over the motion engine (`astryx_primitives::motion`), and `hooks/useEntryAnimation.ts`.

mod entry;
pub mod examples;

#[cfg(test)]
mod tests;

use std::rc::Rc;
use std::time::Duration;

use astryx_primitives::motion::{
    Animator, AnimatorAnimateOptions, AnimatorCall, AnimatorSnapshot, AnimatorValues, EngineOptions,
};
use astryx_primitives::{coarse_primary_pointer, prefers_reduced_motion};
use gpui::{App, ElementId, Styled, Task, Window, px};
use web_time::Instant;

pub use entry::{EntryAnimation, EntryAnimationPreset, use_entry_animation};

use crate::doc::{ComponentDoc, PropMapping, RustApi};

/// What an animated element keeps: its animator, and the clock's origin.
struct AnimatorState {
    animator: Animator,
    origin: Instant,
}

/// Upstream `AnimatorHandle`: animates one element's x, y, scale, rotate and opacity with
/// Astryx's motion. Cheap to clone; every clone drives the same element. Apply it where
/// the element renders with [`AnimatorHandle::apply`].
#[derive(Clone)]
pub struct AnimatorHandle {
    animator: Animator,
    origin: Instant,
}

/// Upstream `useAnimator`: the animator of the element `id` names, kept across frames.
///
/// ```ignore
/// let card = use_animator("card", window, cx);
/// card.apply(div().id("card").on_click({
///     let card = card.clone();
///     move |_, _, cx| { card.animate(AnimatorValues::new().x(240.), Default::default(), cx); }
/// }), window, cx)
/// ```
pub fn use_animator(id: impl Into<ElementId>, window: &mut Window, cx: &mut App) -> AnimatorHandle {
    let id = id.into();
    let executor = cx.background_executor().clone();
    let origin = executor.now();
    let state = window.use_keyed_state(
        ElementId::Name(format!("{id}-animator").into()),
        cx,
        move |_, _| {
            let clock = executor.clone();
            let now: Rc<dyn Fn() -> f64> = Rc::new(move || {
                clock.now().saturating_duration_since(origin).as_secs_f64() * 1000.
            });
            AnimatorState {
                animator: Animator::new(EngineOptions {
                    now: Some(now),
                    frames: true,
                    ..Default::default()
                }),
                origin,
            }
        },
    );
    let (animator, origin) = {
        let s = state.read(cx);
        (s.animator.clone(), s.origin)
    };
    // The environment each call plans against (upstream reads these from the page).
    let engine = animator.engine();
    engine.set_reduced_motion(prefers_reduced_motion(cx));
    engine.set_coarse_pointer(coarse_primary_pointer(window, cx));
    AnimatorHandle { animator, origin }
}

impl AnimatorHandle {
    fn now(&self, cx: &App) -> f64 {
        cx.background_executor()
            .now()
            .saturating_duration_since(self.origin)
            .as_secs_f64()
            * 1000.
    }

    /// Upstream `animate`: springs the given channels to `target` on the
    /// `springs.stateChange` role, from wherever they are on screen, carrying their
    /// velocity (or the one given). Wait for it with [`AnimatorHandle::finished`].
    pub fn animate(
        &self,
        target: AnimatorValues,
        options: AnimatorAnimateOptions,
        cx: &mut App,
    ) -> AnimatorCall {
        let call = self.animator.animate(target, options);
        cx.refresh_windows();
        call
    }

    /// Upstream `decay`: coasts the given channels from a release velocity, slowing as a
    /// scroll view's momentum does.
    pub fn decay(&self, velocity: AnimatorValues, cx: &mut App) -> AnimatorCall {
        let call = self.animator.decay(velocity);
        cx.refresh_windows();
        call
    }

    /// Upstream `set`: puts the given channels at these values at once (a drag's write).
    pub fn set(&self, values: AnimatorValues, cx: &mut App) {
        self.animator.set(values);
        cx.refresh_windows();
    }

    /// Upstream `get`: every channel's value on screen now.
    pub fn get(&self, cx: &App) -> AnimatorSnapshot {
        self.animator.engine().advance(self.now(cx));
        self.animator.get()
    }

    /// Upstream `getVelocity`: every channel's velocity now, in units per second.
    pub fn get_velocity(&self, cx: &App) -> AnimatorSnapshot {
        self.animator.engine().advance(self.now(cx));
        self.animator.get_velocity()
    }

    /// Upstream `stop`: halts every channel where it is on screen.
    pub fn stop(&self, cx: &mut App) {
        self.animator.stop();
        cx.refresh_windows();
    }

    /// Upstream's promise: resolves when the call's channels arrive, or as soon as a later
    /// call takes any of them over.
    pub fn finished(&self, call: AnimatorCall, cx: &mut App) -> Task<()> {
        cx.spawn(async move |cx| {
            while !call.is_done() {
                cx.background_executor()
                    .timer(Duration::from_millis(16))
                    .await;
            }
        })
    }

    /// Draws the element at its animated values, as upstream writes them on the element's
    /// style: x and y translate it, `scale` scales it about its transform origin (fork
    /// patch R1), `rotate` turns it clockwise about the same origin, in degrees, and
    /// `opacity` fades it as one group, as CSS `opacity` does. A rotation or a fade draws
    /// the element through an offscreen layer (fork patch R3), and hit testing follows the
    /// rotation; a renderer without offscreen layers draws it unrotated and fades
    /// each primitive. Frames are requested while anything moves.
    pub fn apply<E: Styled>(&self, el: E, window: &mut Window, cx: &App) -> E {
        let now = self.now(cx);
        let engine = self.animator.engine();
        engine.advance(now);
        engine.frame(now);
        if !engine.is_settled() {
            window.request_animation_frame();
        }
        let v = self.animator.get();
        let mut el = el;
        if v.x != 0. || v.y != 0. {
            el = el.translate(px(v.x as f32), px(v.y as f32));
        }
        if v.scale != 1. {
            el = el.scale(v.scale as f32);
        }
        if v.rotate != 0. {
            el = el.rotate_degrees(v.rotate as f32);
        }
        if v.opacity != 1. {
            el = el.layer_opacity(v.opacity.clamp(0., 1.) as f32);
        }
        el
    }
}

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

pub const CREATE_ANIMATOR_DOC: ComponentDoc = ComponentDoc {
    name: "createAnimator",
    rust_type: "astryx::primitives::motion::Animator (GPUI: astryx::components::animator::use_animator)",
    props: &[p(
        "element",
        RustApi::Waived(
            "GPUI has no element handles: apply the animator where the element renders (AnimatorHandle::apply)",
        ),
    )],
    example: include_str!("examples.rs"),
};

pub const USE_ANIMATOR_DOC: ComponentDoc = ComponentDoc {
    name: "useAnimator",
    rust_type: "astryx::components::animator::use_animator -> AnimatorHandle",
    props: &[p(
        "ref",
        RustApi::Argument("id (the element's id; apply the handle where it renders)"),
    )],
    example: include_str!("examples.rs"),
};

pub const USE_ENTRY_ANIMATION_DOC: ComponentDoc = ComponentDoc {
    name: "useEntryAnimation",
    rust_type: "astryx::components::animator::use_entry_animation -> EntryAnimation",
    props: &[p("preset", RustApi::Argument("preset"))],
    example: include_str!("examples.rs"),
};
