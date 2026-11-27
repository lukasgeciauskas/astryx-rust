//! The engine's shared vocabulary, upstream `motion/animator.ts`'s
//! `MotionProperty`, `MotionCurve` and `SETTLE_THRESHOLD`, with
//! `motion/frameBlock.ts` and `motion/opacityFloor.ts`.

use std::fmt;
use std::rc::Rc;

use super::SpringDescription;

/// A value the animator can drive (upstream `MotionProperty`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum MotionProperty {
    X,
    Y,
    Rotate,
    ScaleX,
    ScaleY,
    Opacity,
}

impl MotionProperty {
    /// Every property, in upstream's order (`PROPERTIES`).
    pub const ALL: [MotionProperty; 6] = [
        Self::X,
        Self::Y,
        Self::Rotate,
        Self::ScaleX,
        Self::ScaleY,
        Self::Opacity,
    ];

    /// The properties that move an element rather than fade it
    /// (upstream `TRAVEL_PROPERTIES`, `reducedMotionPlan.ts`).
    pub const TRAVEL: [MotionProperty; 5] =
        [Self::X, Self::Y, Self::Rotate, Self::ScaleX, Self::ScaleY];

    /// The value at rest before any call (upstream `IDENTITY`).
    pub const fn identity(self) -> f64 {
        match self {
            Self::ScaleX | Self::ScaleY | Self::Opacity => 1.,
            _ => 0.,
        }
    }

    /// Within this of its target a curve is over, in the property's own unit
    /// (upstream `SETTLE_THRESHOLD`).
    pub const fn settle_threshold(self) -> f64 {
        match self {
            Self::X | Self::Y => 0.05,
            Self::Rotate => 0.01,
            Self::ScaleX | Self::ScaleY => 0.0001,
            Self::Opacity => 0.001,
        }
    }

    /// Whether the property moves the element (x, y, rotate, scale).
    pub const fn is_travel(self) -> bool {
        !matches!(self, Self::Opacity)
    }

    /// Upstream's name for the property.
    pub const fn name(self) -> &'static str {
        match self {
            Self::X => "x",
            Self::Y => "y",
            Self::Rotate => "rotate",
            Self::ScaleX => "scaleX",
            Self::ScaleY => "scaleY",
            Self::Opacity => "opacity",
        }
    }
}

impl fmt::Display for MotionProperty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// Upstream `SETTLE_THRESHOLD`, by property.
pub const SETTLE_THRESHOLD: [(MotionProperty, f64); 6] = [
    (MotionProperty::X, 0.05),
    (MotionProperty::Y, 0.05),
    (MotionProperty::Rotate, 0.01),
    (MotionProperty::ScaleX, 0.0001),
    (MotionProperty::ScaleY, 0.0001),
    (MotionProperty::Opacity, 0.001),
];

/// Any closed-form curve the animator can play (upstream `MotionCurve`):
/// times are seconds from the curve's start, and the value at
/// `duration` is the end state.
pub trait MotionCurve {
    /// Length, s.
    fn duration(&self) -> f64;
    /// Value `t` seconds after the start (0 ≤ t ≤ duration).
    fn at(&self, t: f64) -> f64;
    /// Velocity `t` seconds after the start, units/s.
    fn velocity(&self, t: f64) -> f64;
    /// A transition started on release rather than a throw's momentum (a
    /// spring back from past an edge): under Reduce Motion it is no coast
    /// and cross-fades like `animate` (design:reduced-motion DR2).
    fn is_transition(&self) -> bool {
        false
    }
}

/// A curve shared between the caller and the animator.
pub type SharedCurve = Rc<dyn MotionCurve>;

type CurveFn = Rc<dyn Fn(f64) -> f64>;

/// A [`MotionCurve`] made of closures, as upstream writes its curves as
/// object literals.
#[derive(Clone)]
pub struct FnCurve {
    pub duration: f64,
    pub at: CurveFn,
    pub velocity: CurveFn,
    pub is_transition: bool,
}

impl FnCurve {
    /// A curve of `duration` seconds with the given value and velocity.
    pub fn new(
        duration: f64,
        at: impl Fn(f64) -> f64 + 'static,
        velocity: impl Fn(f64) -> f64 + 'static,
    ) -> Self {
        Self {
            duration,
            at: Rc::new(at),
            velocity: Rc::new(velocity),
            is_transition: false,
        }
    }

    /// Marks the curve as a transition (`isTransition`).
    pub fn transition(mut self) -> Self {
        self.is_transition = true;
        self
    }

    /// The curve as a [`SharedCurve`].
    pub fn shared(self) -> SharedCurve {
        Rc::new(self)
    }
}

impl fmt::Debug for FnCurve {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FnCurve")
            .field("duration", &self.duration)
            .field("is_transition", &self.is_transition)
            .finish_non_exhaustive()
    }
}

impl MotionCurve for FnCurve {
    fn duration(&self) -> f64 {
        self.duration
    }
    fn at(&self, t: f64) -> f64 {
        (self.at)(t)
    }
    fn velocity(&self, t: f64) -> f64 {
        (self.velocity)(t)
    }
    fn is_transition(&self) -> bool {
        self.is_transition
    }
}

/// A spring a call plays: a motion role's spring (`role` names its table
/// path, such as `springs.settle`, and the theme's tempo scales it), or one
/// written inline at the call site (`role: None`), which plays as given.
/// Upstream tells the two apart by the leaf object's identity.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MotionSpring {
    pub spring: SpringDescription,
    pub role: Option<&'static str>,
}

impl MotionSpring {
    /// A spring written at the call site: no role, so no tempo.
    pub const fn inline(spring: SpringDescription) -> Self {
        Self { spring, role: None }
    }

    /// The spring of the motion role `id`.
    pub const fn role(id: &'static str, spring: SpringDescription) -> Self {
        Self {
            spring,
            role: Some(id),
        }
    }
}

impl From<SpringDescription> for MotionSpring {
    fn from(spring: SpringDescription) -> Self {
        Self::inline(spring)
    }
}

/// How long a task must run on past a frame to count as a block, ms
/// (upstream `LATE_START_MS`, `motion/frameBlock.ts`): two frames at 60 Hz.
pub const LATE_START_MS: f64 = 34.;

/// The lowest opacity a compositor entrance fades in from (upstream
/// `ENTRANCE_OPACITY_FLOOR`, `motion/opacityFloor.ts`): a quarter of one
/// step of an 8-bit channel, so it changes no pixel.
pub const ENTRANCE_OPACITY_FLOOR: f64 = 0.001;
