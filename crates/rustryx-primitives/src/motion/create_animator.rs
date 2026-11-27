//! The public animator, upstream `animator/createAnimator.ts`: five
//! channels (x, y, scale, rotate, opacity) over the motion engine, sprung on
//! the `springs.stateChange` role, coasted from a release velocity, set, read
//! and stopped. The UI layer's `use_animator` wraps it.

use super::animator::{
    AnimateOptions, DecayCurves, DecayOptions, EngineOptions, MotionCurves, MotionEngine,
    MotionHandle, MotionKey, MotionValues, SpringTransition,
};
use super::coast::{CoastOptions, CoastVelocity, coast};
use super::uikit::springs;
use super::{FnCurve, MotionProperty, SharedCurve, solve_decay};

/// A channel the animator drives (upstream `AnimatorChannel`): `x` and `y`
/// translate (px), `scale` scales about the transform origin, `rotate` turns
/// (degrees, clockwise) and `opacity` fades (0 to 1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AnimatorChannel {
    X,
    Y,
    Scale,
    Rotate,
    Opacity,
}

impl AnimatorChannel {
    /// Every channel, in upstream's order.
    pub const ALL: [AnimatorChannel; 5] =
        [Self::X, Self::Y, Self::Scale, Self::Rotate, Self::Opacity];

    fn key(self) -> MotionKey {
        match self {
            Self::X => MotionProperty::X.into(),
            Self::Y => MotionProperty::Y.into(),
            Self::Scale => MotionKey::Scale,
            Self::Rotate => MotionProperty::Rotate.into(),
            Self::Opacity => MotionProperty::Opacity.into(),
        }
    }
}

/// Values by channel; a velocity uses the same keys in units per second
/// (upstream `AnimatorValues`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AnimatorValues {
    pub x: Option<f64>,
    pub y: Option<f64>,
    pub scale: Option<f64>,
    pub rotate: Option<f64>,
    pub opacity: Option<f64>,
}

impl AnimatorValues {
    /// No values.
    pub fn new() -> Self {
        Self::default()
    }
    /// With `x`.
    pub fn x(mut self, value: f64) -> Self {
        self.x = Some(value);
        self
    }
    /// With `y`.
    pub fn y(mut self, value: f64) -> Self {
        self.y = Some(value);
        self
    }
    /// With `scale`.
    pub fn scale(mut self, value: f64) -> Self {
        self.scale = Some(value);
        self
    }
    /// With `rotate`.
    pub fn rotate(mut self, value: f64) -> Self {
        self.rotate = Some(value);
        self
    }
    /// With `opacity`.
    pub fn opacity(mut self, value: f64) -> Self {
        self.opacity = Some(value);
        self
    }

    fn get(&self, channel: AnimatorChannel) -> Option<f64> {
        match channel {
            AnimatorChannel::X => self.x,
            AnimatorChannel::Y => self.y,
            AnimatorChannel::Scale => self.scale,
            AnimatorChannel::Rotate => self.rotate,
            AnimatorChannel::Opacity => self.opacity,
        }
        .filter(|value| value.is_finite())
    }

    /// The channels' values as the engine's (upstream `motionValues`).
    fn motion_values(&self) -> MotionValues {
        MotionValues {
            x: self.get(AnimatorChannel::X),
            y: self.get(AnimatorChannel::Y),
            rotate: self.get(AnimatorChannel::Rotate),
            opacity: self.get(AnimatorChannel::Opacity),
            scale: self.get(AnimatorChannel::Scale),
            ..Default::default()
        }
    }

    fn is_empty(&self) -> bool {
        AnimatorChannel::ALL
            .iter()
            .all(|channel| self.get(*channel).is_none())
    }
}

/// Every channel's value (or velocity) (upstream `AnimatorSnapshot`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AnimatorSnapshot {
    pub x: f64,
    pub y: f64,
    pub scale: f64,
    pub rotate: f64,
    pub opacity: f64,
}

/// Options of [`Animator::animate`] (upstream `AnimatorAnimateOptions`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AnimatorAnimateOptions {
    /// Each channel's velocity at the start, such as a drag's release
    /// velocity; a channel without one keeps the velocity it has on screen.
    pub velocity: AnimatorValues,
}

/// One `animate` or `decay` call (upstream's returned promise): done once
/// every motion of the call finished, or as soon as a later call takes any
/// of its channels over.
#[derive(Clone, Debug, Default)]
pub struct AnimatorCall {
    moving: Vec<MotionHandle>,
}

impl AnimatorCall {
    /// Whether the call is over (upstream's promise resolved).
    pub fn is_done(&self) -> bool {
        self.moving.is_empty()
            || self
                .moving
                .iter()
                .any(|handle| handle.finished() == Some(false))
            || self
                .moving
                .iter()
                .all(|handle| handle.finished() == Some(true))
    }

    /// The engine handles of the call's motions.
    pub fn handles(&self) -> &[MotionHandle] {
        &self.moving
    }
}

/// A new or remounted element starts from these values.
const REST: MotionValues = MotionValues {
    x: Some(0.),
    y: Some(0.),
    rotate: Some(0.),
    scale_x: None,
    scale_y: None,
    opacity: Some(1.),
    scale: Some(1.),
};

/// A coast from `from` at `velocity` on a scroll view's momentum, ending at
/// rest; an opacity coast stops at 0 or 1 (upstream `momentum`).
fn momentum(from: f64, velocity: f64, threshold: f64, is_unit: bool) -> SharedCurve {
    let glide = solve_decay(from, velocity, None);
    let edge = if !is_unit {
        None
    } else if glide.rest < 0. {
        Some(0.)
    } else if glide.rest > 1. {
        Some(1.)
    } else {
        None
    };
    let hit = edge.and_then(|edge| glide.time_to_reach(edge));
    let duration = hit.unwrap_or_else(|| glide.settle_time(threshold));
    let end = edge.unwrap_or(glide.rest);
    FnCurve::new(
        duration,
        move |t| if t >= duration { end } else { glide.value(t) },
        move |t| if t >= duration { 0. } else { glide.velocity(t) },
    )
    .shared()
}

/// Animates one element (upstream `createAnimator`'s `Animator`): each call
/// acts on the channels it is given and leaves the others where they are.
/// Under Reduce Motion every call is planned: a move becomes a short
/// cross-fade on a touch screen and lands at once with a mouse.
#[derive(Clone, Debug)]
pub struct Animator {
    motion: MotionEngine,
    attached: bool,
    destroyed: bool,
}

impl Animator {
    /// An animator on an engine made with `options`, attached to its element
    /// (upstream `createAnimator(element)`).
    pub fn new(options: EngineOptions) -> Self {
        Self {
            motion: MotionEngine::new(options),
            attached: true,
            destroyed: false,
        }
    }

    /// An animator whose element attaches later (upstream
    /// `createAttachableAnimator`): calls do nothing until then.
    pub fn detached(options: EngineOptions) -> Self {
        Self {
            attached: false,
            ..Self::new(options)
        }
    }

    /// The engine it drives, for the frame loop (`advance`, `frame`) and the
    /// environment (`set_reduced_motion`, `set_tempo`, …).
    pub fn engine(&self) -> &MotionEngine {
        &self.motion
    }

    /// Attaches or lets go of the element. An element that leaves takes its
    /// values with it, so the next one, or a remount, starts at rest.
    pub fn set_attached(&mut self, attached: bool) {
        if self.destroyed || attached == self.attached {
            return;
        }
        if self.attached {
            self.motion.stop(None);
            self.motion.set(REST);
        }
        self.attached = attached;
    }

    fn inactive(&self) -> bool {
        self.destroyed || !self.attached
    }

    /// Springs the given channels to `target` on the `springs.stateChange`
    /// role, from wherever they are on screen.
    pub fn animate(&self, target: AnimatorValues, options: AnimatorAnimateOptions) -> AnimatorCall {
        if self.inactive() || target.is_empty() {
            return AnimatorCall::default();
        }
        let transition = SpringTransition::new(springs::role::STATE_CHANGE)
            .velocity(options.velocity.motion_values());
        let handle = self.motion.animate(
            target.motion_values(),
            transition,
            AnimateOptions::default(),
        );
        AnimatorCall {
            moving: vec![handle],
        }
    }

    /// Coasts the given channels from a release velocity, slowing as a scroll
    /// view's momentum does; opacity stops at 0 or 1. The translate axes
    /// coast as a throw, which Reduce Motion lets continue; the others coast
    /// as a transition, which it replaces.
    pub fn decay(&self, velocity: AnimatorValues) -> AnimatorCall {
        if self.inactive() {
            return AnimatorCall::default();
        }
        let translate: DecayCurves = coast(
            &self.motion,
            CoastVelocity {
                x: velocity.get(AnimatorChannel::X),
                y: velocity.get(AnimatorChannel::Y),
            },
            CoastOptions::default(),
        );
        let mut others = MotionCurves::new();
        let mut any_other = false;
        for (channel, key, property) in [
            (
                AnimatorChannel::Scale,
                MotionKey::Scale,
                MotionProperty::ScaleX,
            ),
            (
                AnimatorChannel::Rotate,
                MotionProperty::Rotate.into(),
                MotionProperty::Rotate,
            ),
            (
                AnimatorChannel::Opacity,
                MotionProperty::Opacity.into(),
                MotionProperty::Opacity,
            ),
        ] {
            if let Some(v0) = velocity.get(channel) {
                let from = self.motion.get(property, None);
                others = others.with(
                    key,
                    momentum(
                        from,
                        v0,
                        property.settle_threshold(),
                        channel == AnimatorChannel::Opacity,
                    ),
                );
                any_other = true;
            }
        }
        let mut moving = Vec::new();
        if translate.x.is_some() || translate.y.is_some() {
            moving.push(self.motion.decay(translate, DecayOptions::default()));
        }
        if any_other && let Ok(handle) = self.motion.play(others, AnimateOptions::default()) {
            moving.push(handle);
        }
        AnimatorCall { moving }
    }

    /// Puts the given channels at these values at once, ending their motion.
    pub fn set(&self, values: AnimatorValues) {
        if !self.inactive() {
            self.motion.set(values.motion_values());
        }
    }

    fn snapshot(&self, read: impl Fn(MotionKey) -> f64) -> AnimatorSnapshot {
        AnimatorSnapshot {
            x: read(AnimatorChannel::X.key()),
            y: read(AnimatorChannel::Y.key()),
            scale: read(AnimatorChannel::Scale.key()),
            rotate: read(AnimatorChannel::Rotate.key()),
            opacity: read(AnimatorChannel::Opacity.key()),
        }
    }

    /// Every channel's value on screen now.
    pub fn get(&self) -> AnimatorSnapshot {
        self.snapshot(|key| self.motion.get(key, None))
    }

    /// Every channel's velocity on screen now, in units per second.
    pub fn get_velocity(&self) -> AnimatorSnapshot {
        self.snapshot(|key| self.motion.get_velocity(key, None))
    }

    /// Halts every channel where it is on screen.
    pub fn stop(&self) {
        if !self.inactive() {
            self.motion.stop(None);
        }
    }

    /// Stops every motion and lets go of the element, which keeps the values
    /// it shows; the animator does nothing after this.
    pub fn destroy(&mut self) {
        if !self.destroyed {
            self.destroyed = true;
            self.motion.stop(None);
            self.motion.dispose();
        }
    }

    /// Whether [`destroy`](Self::destroy) ran.
    pub fn is_destroyed(&self) -> bool {
        self.destroyed
    }
}
