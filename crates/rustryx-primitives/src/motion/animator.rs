//! The animator engine, upstream `motion/animator.ts` (`createMotionEngine`,
//! `animatorFor`): each animated value (x, y, rotate, scaleX, scaleY,
//! opacity) follows a closed-form curve (a spring, momentum curves from
//! `coast`, a curve handed to `play`, or a Reduce Motion cross-fade). Values
//! and velocities are read from the curves at a time on the engine's clock,
//! so a finger can grab anything mid-flight, and a new target starts from the
//! current value with the current velocity (spec:AST-094 FR6).
//!
//! The engine is pure state advanced on an injectable clock in ms: no GPUI
//! types, no element. What upstream hands the browser (keyframes, inline
//! styles, Web Animations, compositor promotion) is left out; what a caller
//! can observe in values over time is kept:
//!
//! - **Starts.** With a frame source ([`EngineOptions::frames`]), a call
//!   from rest starts at the next [`MotionEngine::frame`], as Core
//!   Animation starts an animation at its transaction's commit, so a block
//!   before that frame delays the motion instead of skipping into it; until
//!   then it holds its first value. A call that continues motion on screen (a
//!   retarget) starts at once from the value and velocity on screen, and if
//!   its first frame comes more than `LATE_START_MS` late, its spring
//!   restarts at that frame from the old curve's place. `AnimateOptions::at`
//!   puts a call on an explicit clock; `StartMode` changes the rule per call.
//!   Without a frame source every call starts at its own time.
//! - **Completion.** Each call's [`MotionHandle`] finishes `Some(true)` when
//!   every property reached the end of its curve, `Some(false)` as soon as
//!   any is retargeted, stopped or set first. A curve is over within
//!   `SETTLE_THRESHOLD` of its target; one that has not settled by 30 s snaps.
//! - **Reduce Motion** (design:reduced-motion): every `animate` and `play`
//!   becomes an opacity-only cross-fade of 200 ms on `timing.curves.move` on
//!   a coarse pointer or after a touch release (travel lands while the
//!   element is transparent), and completes at once elsewhere; momentum from
//!   `decay` keeps moving (DR2, DEC-2) unless a curve is a transition.
//! - **Roles.** Every spring passes through `resolve_spring` at the engine's
//!   tempo (`MotionEngine::set_tempo`).
//!
//! The engine's timers (a handle's end, a Reduce Motion landing) run as the
//! clock passes them: call [`MotionEngine::advance`] each frame (any call
//! runs the timers due by its time first).

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt;
use std::hash::Hash;
use std::rc::{Rc, Weak};

use super::bezier::cubic_bezier;
use super::reduced_motion_plan::{
    FadeIn, FadePoint, PlannedLanding, ReducedMotionView, plan_reduced_motion,
};
use super::resolve_motion::resolve_spring;
use super::uikit::timing;
use super::{
    LATE_START_MS, MotionCurve, MotionProperty, MotionSpring, SharedCurve, SpringDescription,
    solve_spring,
};

pub use super::reduced_motion_plan::CrossfadeAs;

/// Within this of its target a curve is over, by property (upstream
/// `SETTLE_THRESHOLD`).
pub use super::curve::SETTLE_THRESHOLD;

/// What linear interpolation between upstream's keyframes stays within; the
/// engine reads it to tell a curve whose tail shows as at rest.
const fn sample_tolerance(property: MotionProperty) -> f64 {
    match property {
        MotionProperty::X | MotionProperty::Y => 0.25,
        MotionProperty::Rotate => 0.05,
        MotionProperty::ScaleX | MotionProperty::ScaleY => 0.0005,
        MotionProperty::Opacity => 0.004,
    }
}

/// A curve within this many sample tolerances of its target can show as at
/// rest.
const REST_DISTANCE_TOLERANCES: f64 = 2.;
/// A curve shows as at rest only while it moves at most one sample
/// tolerance a frame at this rate.
const REST_FRAMES_PER_SECOND: f64 = 60.;
/// A curve that has not settled by then (an undamped spring) snaps, ms.
pub const MAX_DURATION_MS: f64 = 30_000.;
/// How soon a waiting end is looked at again, ms: a frame at 60 Hz.
const END_RECHECK_MS: f64 = 1000. / 60.;
/// How close a spring's target and a value must be to count as the same.
const SAME_VALUE: f64 = 1e-6;

/// The spring `decay` hands a Reduce Motion transition to: under Reduce
/// Motion `animate` plays no spring, so it is never solved.
const REDUCED_TRANSITION: SpringDescription = SpringDescription::DurationBounce {
    duration: 0.5,
    bounce: 0.,
};

/// A property, or `scale` for both scale axes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MotionKey {
    Property(MotionProperty),
    /// `scaleX` and `scaleY` together; reads `scaleX`.
    Scale,
}

impl From<MotionProperty> for MotionKey {
    fn from(property: MotionProperty) -> Self {
        Self::Property(property)
    }
}

impl MotionKey {
    fn read(self) -> MotionProperty {
        match self {
            Self::Property(property) => property,
            Self::Scale => MotionProperty::ScaleX,
        }
    }

    fn expand(self) -> Vec<MotionProperty> {
        match self {
            Self::Property(property) => vec![property],
            Self::Scale => vec![MotionProperty::ScaleX, MotionProperty::ScaleY],
        }
    }
}

fn expand_list(keys: &[MotionKey]) -> Vec<MotionProperty> {
    let mut out = Vec::new();
    for key in keys {
        for property in key.expand() {
            if !out.contains(&property) {
                out.push(property);
            }
        }
    }
    out
}

/// Values by property; `scale` sets both scale axes, and an explicit axis
/// wins over it (upstream `MotionValues`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MotionValues {
    pub x: Option<f64>,
    pub y: Option<f64>,
    pub rotate: Option<f64>,
    pub scale_x: Option<f64>,
    pub scale_y: Option<f64>,
    pub opacity: Option<f64>,
    pub scale: Option<f64>,
}

impl MotionValues {
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

    /// With `rotate`, degrees.
    pub fn rotate(mut self, value: f64) -> Self {
        self.rotate = Some(value);
        self
    }

    /// With `scaleX`.
    pub fn scale_x(mut self, value: f64) -> Self {
        self.scale_x = Some(value);
        self
    }

    /// With `scaleY`.
    pub fn scale_y(mut self, value: f64) -> Self {
        self.scale_y = Some(value);
        self
    }

    /// With both scale axes.
    pub fn scale(mut self, value: f64) -> Self {
        self.scale = Some(value);
        self
    }

    /// With `opacity`.
    pub fn opacity(mut self, value: f64) -> Self {
        self.opacity = Some(value);
        self
    }

    /// With `property` set to `value`.
    pub fn with(mut self, property: MotionProperty, value: f64) -> Self {
        *self.slot(property) = Some(value);
        self
    }

    fn slot(&mut self, property: MotionProperty) -> &mut Option<f64> {
        match property {
            MotionProperty::X => &mut self.x,
            MotionProperty::Y => &mut self.y,
            MotionProperty::Rotate => &mut self.rotate,
            MotionProperty::ScaleX => &mut self.scale_x,
            MotionProperty::ScaleY => &mut self.scale_y,
            MotionProperty::Opacity => &mut self.opacity,
        }
    }

    /// The values by property, `scale` expanded, non-finite values left out
    /// (upstream `expand`).
    pub fn expand(&self) -> BTreeMap<MotionProperty, f64> {
        let mut out = BTreeMap::new();
        let explicit = [
            (MotionProperty::X, self.x),
            (MotionProperty::Y, self.y),
            (MotionProperty::Rotate, self.rotate),
            (MotionProperty::ScaleX, self.scale_x),
            (MotionProperty::ScaleY, self.scale_y),
            (MotionProperty::Opacity, self.opacity),
        ];
        for (property, value) in explicit {
            if let Some(value) = value.filter(|v| v.is_finite()) {
                out.insert(property, value);
            }
        }
        if let Some(scale) = self.scale.filter(|v| v.is_finite()) {
            out.entry(MotionProperty::ScaleX).or_insert(scale);
            out.entry(MotionProperty::ScaleY).or_insert(scale);
        }
        out
    }
}

/// Curves by property for `play`; `scale` drives both axes, and an explicit
/// axis wins over it.
#[derive(Clone, Default)]
pub struct MotionCurves {
    pub x: Option<SharedCurve>,
    pub y: Option<SharedCurve>,
    pub rotate: Option<SharedCurve>,
    pub scale_x: Option<SharedCurve>,
    pub scale_y: Option<SharedCurve>,
    pub opacity: Option<SharedCurve>,
    pub scale: Option<SharedCurve>,
}

impl fmt::Debug for MotionCurves {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MotionCurves").finish_non_exhaustive()
    }
}

impl MotionCurves {
    /// No curves.
    pub fn new() -> Self {
        Self::default()
    }

    /// With `key` driven by `curve`.
    pub fn with(mut self, key: impl Into<MotionKey>, curve: SharedCurve) -> Self {
        let slot = match key.into() {
            MotionKey::Scale => &mut self.scale,
            MotionKey::Property(MotionProperty::X) => &mut self.x,
            MotionKey::Property(MotionProperty::Y) => &mut self.y,
            MotionKey::Property(MotionProperty::Rotate) => &mut self.rotate,
            MotionKey::Property(MotionProperty::ScaleX) => &mut self.scale_x,
            MotionKey::Property(MotionProperty::ScaleY) => &mut self.scale_y,
            MotionKey::Property(MotionProperty::Opacity) => &mut self.opacity,
        };
        *slot = Some(curve);
        self
    }

    fn expand(&self) -> BTreeMap<MotionProperty, SharedCurve> {
        let mut out = BTreeMap::new();
        if let Some(scale) = &self.scale {
            out.insert(MotionProperty::ScaleX, scale.clone());
            out.insert(MotionProperty::ScaleY, scale.clone());
        }
        let explicit = [
            (MotionProperty::X, &self.x),
            (MotionProperty::Y, &self.y),
            (MotionProperty::Rotate, &self.rotate),
            (MotionProperty::ScaleX, &self.scale_x),
            (MotionProperty::ScaleY, &self.scale_y),
            (MotionProperty::Opacity, &self.opacity),
        ];
        for (property, curve) in explicit {
            if let Some(curve) = curve {
                out.insert(property, curve.clone());
            }
        }
        out
    }
}

/// Momentum curves for `decay`: the translate axes only.
#[derive(Clone, Default)]
pub struct DecayCurves {
    pub x: Option<SharedCurve>,
    pub y: Option<SharedCurve>,
}

impl fmt::Debug for DecayCurves {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DecayCurves")
            .field("x", &self.x.is_some())
            .field("y", &self.y.is_some())
            .finish()
    }
}

impl DecayCurves {
    fn get(&self, property: MotionProperty) -> Option<&SharedCurve> {
        match property {
            MotionProperty::X => self.x.as_ref(),
            MotionProperty::Y => self.y.as_ref(),
            _ => None,
        }
    }
}

/// A spring and the velocity it starts with (upstream `SpringTransition`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpringTransition {
    /// The spring: a motion role's (tempo applies) or one written inline.
    pub spring: MotionSpring,
    /// Initial velocities, units per second (px/s, deg/s, 1/s), such as a
    /// gesture's release velocity. A property without one keeps the
    /// velocity it has on screen.
    pub velocity: MotionValues,
}

impl SpringTransition {
    /// `spring`, keeping the velocity on screen.
    pub fn new(spring: impl Into<MotionSpring>) -> Self {
        Self {
            spring: spring.into(),
            velocity: MotionValues::default(),
        }
    }

    /// With release velocities.
    pub fn velocity(mut self, velocity: MotionValues) -> Self {
        self.velocity = velocity;
        self
    }
}

impl From<MotionSpring> for SpringTransition {
    fn from(spring: MotionSpring) -> Self {
        Self::new(spring)
    }
}

impl From<SpringDescription> for SpringTransition {
    fn from(spring: SpringDescription) -> Self {
        Self::new(spring)
    }
}

/// When a call's motion starts (upstream `AnimateOptions.start`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StartMode {
    /// A call that continues motion also starts at the next frame.
    Commit,
    /// A call from rest leaves its element at rest until two frames have
    /// presented it there, and starts at the third. Any other change to
    /// the engine before then starts it at the next frame.
    Presented,
    /// The call is made in an input event's own task: a call from rest
    /// starts at the first frame after it (the engine's default for a call
    /// from rest).
    NextFrame,
}

/// Options of `animate` and `play` (upstream `AnimateOptions`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AnimateOptions {
    /// Start time on the engine's clock, ms; now by default. One value on
    /// several engines puts their motion on one clock.
    pub at: Option<f64>,
    /// The call starts from the release of a touch drag: under Reduce Motion
    /// it cross-fades on any pointer (DR2).
    pub touch_release: bool,
    /// Which way a substituted cross-fade goes when the call's own opacity
    /// does not say. Without Reduce Motion this changes nothing.
    pub crossfade_as: Option<CrossfadeAs>,
    /// When the call's motion starts (ignored with `at`).
    pub start: Option<StartMode>,
    /// A retarget adds a spring of the difference between the targets on top
    /// of the curve on screen instead of replacing it, as UIKit's animations
    /// do. Ignored for a property at rest or still waiting to start, for a
    /// property given a velocity, and under Reduce Motion.
    pub additive: bool,
}

/// What `decay` takes (upstream `Pick<AnimateOptions, 'at' | 'touchRelease'>`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DecayOptions {
    pub at: Option<f64>,
    pub touch_release: bool,
}

/// What `rebase` takes besides the amounts (upstream `RebaseOptions`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RebaseOptions {
    /// Time of the shift, ms; now by default.
    pub at: Option<f64>,
    /// Per property, the value its spring heads for after the shift: a
    /// spring whose target plus its amount lands there moves whole, keeping
    /// its start and velocity.
    pub towards: MotionValues,
}

/// What Reduce Motion makes of a call on this engine (upstream
/// `Animator.reducedMotion`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReducedMotion {
    /// Motion plays.
    Off,
    /// An opacity-only fade: a coarse pointer, or a touch release.
    Crossfade,
    /// Completes at once: any other pointer.
    Instant,
}

/// Why a call was refused.
#[derive(Clone, Debug, PartialEq)]
pub enum MotionError {
    /// `retarget` on a handle with no spring of its own (a played curve).
    NeedsSpring,
    /// A played curve without a finite duration ≥ 0.
    BadDuration(f64),
}

impl fmt::Display for MotionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NeedsSpring => f.write_str("animator: retarget needs a spring"),
            Self::BadDuration(duration) => write!(
                f,
                "animator: a curve needs a finite duration ≥ 0 (got {duration})"
            ),
        }
    }
}

impl std::error::Error for MotionError {}

/// The engine's clock: ms, any time base the caller keeps.
pub type Clock = Rc<dyn Fn() -> f64>;

/// Options of [`MotionEngine::new`] (upstream `AnimatorOptions`).
#[derive(Clone, Default)]
pub struct EngineOptions {
    /// Starting values; identity by default.
    pub initial: MotionValues,
    /// The clock, ms. Without one the engine keeps its own, which
    /// [`MotionEngine::advance`] moves.
    pub now: Option<Clock>,
    /// A frame source drives the engine: calls from rest start at the next
    /// [`MotionEngine::frame`], and a retarget whose frame comes late
    /// restarts there. Without it every call starts at its own time.
    pub frames: bool,
}

impl fmt::Debug for EngineOptions {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EngineOptions")
            .field("initial", &self.initial)
            .field("frames", &self.frames)
            .finish_non_exhaustive()
    }
}

// ---------------------------------------------------------------------------
// Internal state
// ---------------------------------------------------------------------------

type Curve1 = Rc<dyn Fn(f64) -> f64>;

struct OwnerState {
    settled: Cell<Option<bool>>,
    timer: Cell<Option<u64>>,
}

type Owner = Rc<OwnerState>;

fn new_owner() -> Owner {
    Rc::new(OwnerState {
        settled: Cell::new(None),
        timer: Cell::new(None),
    })
}

fn owner_key(owner: &Owner) -> usize {
    Rc::as_ptr(owner) as usize
}

/// A call from rest waiting for its frame (upstream `StartGroup`).
struct GroupState {
    provisional: f64,
    frames_left: Cell<u32>,
    start: Cell<Option<f64>>,
}

type Group = Rc<GroupState>;

impl GroupState {
    fn resolved(&self) -> bool {
        self.start.get().is_some()
    }
}

#[derive(Clone)]
struct Segment {
    id: u64,
    /// Start on the engine's clock, ms.
    start: f64,
    /// Length, ms.
    duration: f64,
    target: f64,
    /// Value `t` ms after the start.
    at: Curve1,
    /// Velocity `t` ms after the start, units/s.
    speed: Curve1,
    is_fade: bool,
    owner: Owner,
    group: Option<Group>,
    /// The resolved spring driving it, when one does.
    spring: Option<SpringDescription>,
    is_additive: bool,
    handed_velocity: Option<f64>,
}

impl Segment {
    fn end(&self) -> f64 {
        self.start + self.duration
    }

    fn is_held(&self) -> bool {
        self.group.as_ref().is_some_and(|group| !group.resolved())
    }

    fn shifted(&self, offset: f64) -> Segment {
        if offset == 0. {
            return self.clone();
        }
        let at = self.at.clone();
        Segment {
            target: self.target + offset,
            at: Rc::new(move |t| at(t) + offset),
            ..self.clone()
        }
    }
}

/// Reduce Motion travel that lands while the element is fully transparent
/// (upstream `Landing`).
struct Landing {
    id: u64,
    owner: Owner,
    at: f64,
    values: BTreeMap<MotionProperty, f64>,
    fade_in: Option<FadeIn>,
    restore: Option<f64>,
    restore_is_borrowed: bool,
    carried: Vec<Owner>,
    owners: HashMap<MotionProperty, Owner>,
    timer: Option<u64>,
    due: Option<f64>,
    group: Option<Group>,
}

/// What the screen shows of a property a continuing call changed, until
/// the frame that commits it (upstream `Shown`).
struct Shown {
    segment: Option<Segment>,
    resting: f64,
    offset: f64,
    since: f64,
}

#[derive(Clone)]
enum TimerKind {
    Settle(Owner),
    Land(u64),
    Retire(u64),
}

struct Timer {
    id: u64,
    due: f64,
    kind: TimerKind,
}

struct ReducedPlan {
    next: BTreeMap<MotionProperty, Option<Segment>>,
    holds: BTreeMap<MotionProperty, f64>,
    landing: Option<PlannedLanding>,
    borrowed: Option<f64>,
    joins: bool,
}

struct Adopted {
    carried: Vec<Owner>,
    owners: HashMap<MotionProperty, Owner>,
}

struct Engine {
    clock: Option<Clock>,
    manual_now: f64,
    frames: bool,
    reduced: bool,
    coarse: bool,
    tempo: f64,
    own_opacity: f64,
    resting: BTreeMap<MotionProperty, f64>,
    segments: BTreeMap<MotionProperty, Segment>,
    pending: Vec<Group>,
    borrowed_opacity: Option<f64>,
    opacity_inline: bool,
    landings: Vec<Landing>,
    carried_owners: HashSet<usize>,
    named_opacity: HashSet<usize>,
    heading_before_take_over: Option<f64>,
    shown: BTreeMap<MotionProperty, Shown>,
    timers: Vec<Timer>,
    next_id: u64,
    disposed: bool,
}

impl Engine {
    fn now(&self) -> f64 {
        self.clock.as_ref().map_or(self.manual_now, |clock| clock())
    }

    fn id(&mut self) -> u64 {
        self.next_id += 1;
        self.next_id
    }

    fn resting(&self, property: MotionProperty) -> f64 {
        self.resting[&property]
    }

    fn set_resting(&mut self, property: MotionProperty, value: f64) {
        self.resting.insert(property, value);
    }

    // -- timers ---------------------------------------------------------------

    fn set_timer(&mut self, due: f64, kind: TimerKind) -> u64 {
        let id = self.id();
        self.timers.push(Timer { id, due, kind });
        id
    }

    fn clear_timer(&mut self, id: Option<u64>) {
        if let Some(id) = id {
            self.timers.retain(|timer| timer.id != id);
        }
    }

    fn run_timers(&mut self, until: f64) {
        loop {
            let next = self
                .timers
                .iter()
                .enumerate()
                .filter(|(_, timer)| timer.due <= until)
                .min_by(|(_, a), (_, b)| a.due.total_cmp(&b.due).then(a.id.cmp(&b.id)))
                .map(|(index, _)| index);
            let Some(index) = next else {
                break;
            };
            let timer = self.timers.remove(index);
            match timer.kind {
                TimerKind::Settle(owner) => {
                    if owner.timer.get() == Some(timer.id) {
                        owner.timer.set(None);
                    }
                    // A curve whose call waits for the frame that commits it
                    // has not ended on screen: look again a frame later.
                    let waits = self.shown.keys().any(|property| {
                        self.segments
                            .get(property)
                            .is_some_and(|segment| Rc::ptr_eq(&segment.owner, &owner))
                    });
                    if waits && owner.settled.get().is_none() {
                        let id = self.set_timer(
                            timer.due + END_RECHECK_MS,
                            TimerKind::Settle(owner.clone()),
                        );
                        owner.timer.set(Some(id));
                        continue;
                    }
                    self.settle(&owner, true);
                }
                TimerKind::Land(id) => {
                    if let Some(index) = self.landings.iter().position(|l| l.id == id) {
                        self.landings[index].timer = None;
                        self.land(id);
                    }
                }
                TimerKind::Retire(segment) => self.retire_opacity(segment),
            }
        }
    }

    fn sync(&mut self) {
        let now = self.now();
        self.run_timers(now);
    }

    // -- reading the curves ---------------------------------------------------

    fn curve_at(&self, property: MotionProperty, time: f64) -> f64 {
        match self.segments.get(&property) {
            None => self.resting(property),
            Some(segment) if time >= segment.end() => segment.target,
            Some(segment) => (segment.at)((time - segment.start).max(0.)),
        }
    }

    /// The value on screen at `time`: a held curve shows its first value
    /// until its frame starts it.
    fn value_at(&self, property: MotionProperty, time: f64) -> f64 {
        if let Some(segment) = self.segments.get(&property)
            && segment.is_held()
            && time <= self.now()
        {
            return (segment.at)(0.);
        }
        self.curve_at(property, time)
    }

    fn segment_at(&self, property: MotionProperty, time: f64) -> Option<&Segment> {
        self.segments
            .get(&property)
            .filter(|segment| time < segment.end())
    }

    fn velocity_at(&self, property: MotionProperty, time: f64) -> f64 {
        if let Some(held) = self.segments.get(&property)
            && held.is_held()
        {
            return (held.speed)((time - held.start).max(0.));
        }
        match self.segment_at(property, time) {
            Some(segment) if time >= segment.start => (segment.speed)(time - segment.start),
            _ => 0.,
        }
    }

    fn shows_at_rest(&self, property: MotionProperty, segment: &Segment, time: f64) -> bool {
        (self.value_at(property, time) - segment.target).abs()
            <= REST_DISTANCE_TOLERANCES * sample_tolerance(property)
            && self.velocity_at(property, time).abs()
                <= sample_tolerance(property) * REST_FRAMES_PER_SECOND
    }

    fn final_value(&self, property: MotionProperty) -> f64 {
        self.segments
            .get(&property)
            .map_or_else(|| self.resting(property), |segment| segment.target)
    }

    // -- opacity ownership ----------------------------------------------------

    fn release_own_opacity(&mut self) {
        self.opacity_inline = false;
    }

    /// Notes what the engine writes of opacity after a change (upstream's
    /// inline write and `releaseIdentity`): a running curve or a value off
    /// identity is the engine's own; an identity stays the engine's only
    /// where the element's own opacity differs.
    fn note_opacity(&mut self, time: f64) {
        let opacity = MotionProperty::Opacity;
        if let Some(segment) = self.segments.get(&opacity)
            && time < segment.end()
        {
            self.opacity_inline = true;
            let (id, end) = (segment.id, segment.end());
            self.set_timer(end, TimerKind::Retire(id));
            return;
        }
        self.release_identity(time);
    }

    fn release_identity(&mut self, _time: f64) {
        let opacity = MotionProperty::Opacity;
        if self.borrowed_opacity.is_some() {
            if self
                .landings
                .iter()
                .any(|landing| landing.fade_in.is_some_and(|f| f.is_borrowed))
            {
                self.opacity_inline = true;
                return;
            }
            self.borrowed_opacity = None;
            self.segments.remove(&opacity);
            self.set_resting(opacity, 1.);
            self.release_own_opacity();
            return;
        }
        let value = self.final_value(opacity);
        self.opacity_inline = value != 1. || self.own_opacity != 1.;
    }

    fn retire_opacity(&mut self, segment: u64) {
        let opacity = MotionProperty::Opacity;
        let time = self.now();
        match self.segments.get(&opacity) {
            Some(current) if current.id == segment && time >= current.end() => {}
            Some(current) if current.id == segment => {
                // A frame moved its start later: look again at its end.
                let end = current.end();
                self.set_timer(end, TimerKind::Retire(segment));
                return;
            }
            _ => return,
        }
        self.release_identity(time);
    }

    // -- writing ----------------------------------------------------------------

    fn commit(&mut self, changed: &[MotionProperty], time: f64) {
        if changed.contains(&MotionProperty::Opacity) {
            self.borrowed_opacity = None;
            self.note_opacity(time);
        }
    }

    // -- starts ---------------------------------------------------------------

    fn begin_group(
        &self,
        time: f64,
        properties: &[MotionProperty],
        at: Option<f64>,
        start: Option<StartMode>,
    ) -> Option<Group> {
        if !self.frames {
            return None;
        }
        let channels: Vec<MotionProperty> = properties
            .iter()
            .flat_map(|property| channel_of(*property))
            .collect();
        if let Some(at) = at {
            for property in &channels {
                if let Some(group) = self.segments.get(property).and_then(|s| s.group.clone())
                    && !group.resolved()
                    && at <= group.provisional
                {
                    return Some(group);
                }
            }
            return None;
        }
        if start != Some(StartMode::Commit) {
            for property in &channels {
                if let Some(segment) = self.segments.get(property)
                    && time < segment.end()
                    && !segment.is_held()
                    && !self.shows_at_rest(*property, segment, time)
                {
                    return None;
                }
            }
        }
        Some(Rc::new(GroupState {
            provisional: time,
            frames_left: Cell::new(if start == Some(StartMode::Presented) {
                3
            } else {
                1
            }),
            start: Cell::new(None),
        }))
    }

    fn commit_call(
        &mut self,
        group: Option<Group>,
        owner: &Owner,
        changed: &[MotionProperty],
        time: f64,
        presented: bool,
    ) {
        if let Some(group) = &group {
            let mut any = false;
            for property in changed {
                if let Some(segment) = self.segments.get_mut(property)
                    && Rc::ptr_eq(&segment.owner, owner)
                {
                    segment.group = Some(group.clone());
                    any = true;
                }
            }
            let waiting = self
                .segments
                .values()
                .any(|segment| segment.group.as_ref().is_some_and(|g| Rc::ptr_eq(g, group)));
            if !presented {
                group.frames_left.set(group.frames_left.get().min(1));
            }
            if any || waiting {
                if !self.pending.iter().any(|g| Rc::ptr_eq(g, group)) {
                    self.pending.push(group.clone());
                }
            } else if !group.resolved() {
                self.resolve_group(group, group.provisional);
            }
        }
        self.commit(changed, time);
    }

    /// Starts every call `start: Presented` still holds at the next frame.
    fn flush_deferred(&mut self) {
        for group in &self.pending {
            group.frames_left.set(group.frames_left.get().min(1));
        }
    }

    fn resolve_group(&mut self, group: &Group, start: f64) {
        if group.resolved() {
            return;
        }
        group.start.set(Some(start));
        self.pending.retain(|g| !Rc::ptr_eq(g, group));
        let delta = start - group.provisional;
        let mut owners: Vec<Owner> = Vec::new();
        for segment in self.segments.values_mut() {
            if segment.group.as_ref().is_some_and(|g| Rc::ptr_eq(g, group)) {
                segment.start += delta;
                if !owners.iter().any(|o| Rc::ptr_eq(o, &segment.owner)) {
                    owners.push(segment.owner.clone());
                }
            }
        }
        if delta != 0. {
            for owner in owners {
                if owner.settled.get().is_some() || owner.timer.get().is_none() {
                    continue;
                }
                self.redate(&owner, start);
            }
            if self
                .segments
                .get(&MotionProperty::Opacity)
                .is_some_and(|s| s.group.as_ref().is_some_and(|g| Rc::ptr_eq(g, group)))
            {
                let now = self.now();
                self.note_opacity(now.min(start));
            }
        }
        let landings: Vec<u64> = self
            .landings
            .iter()
            .filter(|l| l.group.as_ref().is_some_and(|g| Rc::ptr_eq(g, group)))
            .map(|l| l.id)
            .collect();
        for id in landings {
            self.arm_landing(id, start);
        }
    }

    /// Moves an owner's end timer to the end of its curves now.
    fn redate(&mut self, owner: &Owner, from: f64) {
        self.clear_timer(owner.timer.take());
        let mut end = from;
        for segment in self.segments.values() {
            if Rc::ptr_eq(&segment.owner, owner) {
                end = end.max(segment.end());
            }
        }
        let id = self.set_timer(end, TimerKind::Settle(owner.clone()));
        owner.timer.set(Some(id));
    }

    fn note_shown(
        &mut self,
        properties: &[MotionProperty],
        time: f64,
        group: &Option<Group>,
        at: Option<f64>,
    ) {
        if group.is_some() || at.is_some() || !self.frames {
            return;
        }
        let held = properties.iter().any(|property| {
            channel_of(*property)
                .iter()
                .any(|other| self.segments.get(other).is_some_and(Segment::is_held))
        });
        if held {
            return;
        }
        for property in properties {
            if self.shown.contains_key(property) {
                continue;
            }
            let shown = Shown {
                segment: self.segments.get(property).cloned(),
                resting: self.resting(*property),
                offset: 0.,
                since: time,
            };
            self.shown.insert(*property, shown);
        }
    }

    fn shown_at(entry: &Shown, time: f64) -> (f64, f64) {
        let Some(segment) = &entry.segment else {
            return (entry.resting + entry.offset, 0.);
        };
        let elapsed = time - segment.start;
        if elapsed >= segment.duration {
            return (segment.target + entry.offset, 0.);
        }
        let t = elapsed.max(0.);
        ((segment.at)(t) + entry.offset, (segment.speed)(t))
    }

    /// The frame that commits calls that continue motion: when it comes
    /// more than `LATE_START_MS` after the first, each spring they left starts
    /// here, from the value on screen (upstream `lookAtShown`).
    fn look_at_shown(&mut self, at: f64) {
        let entries = std::mem::take(&mut self.shown);
        let Some(first) = entries.values().map(|entry| entry.since).reduce(f64::min) else {
            return;
        };
        if at - first <= LATE_START_MS {
            return;
        }
        let mut owners: Vec<Owner> = Vec::new();
        for (property, entry) in entries {
            let Some(current) = self.segments.get(&property).cloned() else {
                continue;
            };
            let Some(spring) = current.spring else {
                continue;
            };
            if current.is_fade || current.is_held() {
                continue;
            }
            let (value, velocity) = Self::shown_at(&entry, at);
            let next = match (&entry.segment, current.is_additive) {
                (Some(old), true) => Some(self.additive_segment(
                    &current.owner,
                    property,
                    at,
                    current.target,
                    MotionSpring::inline(spring),
                    &old.shifted(entry.offset),
                )),
                _ => self.spring_from(
                    &current.owner,
                    property,
                    at,
                    value,
                    current
                        .handed_velocity
                        .unwrap_or(if entry.segment.is_none() {
                            (current.speed)(0.)
                        } else {
                            velocity
                        }),
                    current.target,
                    spring,
                ),
            };
            match next {
                None => {
                    self.segments.remove(&property);
                    self.set_resting(property, current.target);
                }
                Some(mut next) => {
                    next.handed_velocity = current.handed_velocity;
                    self.segments.insert(property, next);
                }
            }
            if !owners.iter().any(|o| Rc::ptr_eq(o, &current.owner)) {
                owners.push(current.owner.clone());
            }
            if property == MotionProperty::Opacity {
                self.note_opacity(at);
            }
        }
        for owner in owners {
            if owner.settled.get().is_some() || owner.timer.get().is_none() {
                continue;
            }
            self.redate(&owner, at);
        }
    }

    // -- settling ---------------------------------------------------------------

    fn settle(&mut self, owner: &Owner, finished: bool) {
        if owner.settled.get().is_some() {
            return;
        }
        owner.settled.set(Some(finished));
        let timer = owner.timer.take();
        self.clear_timer(timer);
    }

    fn interrupt(&mut self, property: MotionProperty, time: f64) {
        self.cancel_landing(property);
        let Some(segment) = self.segments.get(&property) else {
            return;
        };
        if time < segment.end() && !self.carried_owners.contains(&owner_key(&segment.owner)) {
            let owner = segment.owner.clone();
            self.settle(&owner, false);
        }
    }

    // -- segments -------------------------------------------------------------

    fn spring_segment(
        &self,
        owner: &Owner,
        property: MotionProperty,
        start: f64,
        to: f64,
        spring: MotionSpring,
        velocity: f64,
    ) -> Option<Segment> {
        let from = self.value_at(property, start);
        self.spring_from(
            owner,
            property,
            start,
            from,
            velocity,
            to,
            resolve_spring(spring, self.tempo),
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn spring_from(
        &self,
        owner: &Owner,
        property: MotionProperty,
        start: f64,
        from: f64,
        velocity: f64,
        to: f64,
        resolved: SpringDescription,
    ) -> Option<Segment> {
        if from == to && velocity == 0. {
            return None;
        }
        let curve = solve_spring(resolved.to_spring(), from, to, velocity);
        let duration = MAX_DURATION_MS.min(curve.settle_time(property.settle_threshold()) * 1000.);
        Some(Segment {
            id: 0,
            start,
            duration,
            target: to,
            at: Rc::new(move |t| curve.value(t / 1000.)),
            speed: Rc::new(move |t| curve.velocity(t / 1000.)),
            is_fade: false,
            owner: owner.clone(),
            group: None,
            spring: Some(resolved),
            is_additive: false,
            handed_velocity: None,
        })
    }

    /// The old curve plus a spring of the difference (`additive`).
    fn additive_segment(
        &self,
        owner: &Owner,
        property: MotionProperty,
        start: f64,
        to: f64,
        spring: MotionSpring,
        old: &Segment,
    ) -> Segment {
        let old_end = old.end();
        let resolved = resolve_spring(spring, self.tempo);
        let delta = solve_spring(resolved.to_spring(), 0., to - old.target, 0.);
        let delta_ms = MAX_DURATION_MS.min(delta.settle_time(property.settle_threshold()) * 1000.);
        let (old_at, old_speed, old_start, old_target) =
            (old.at.clone(), old.speed.clone(), old.start, old.target);
        let old_value = move |time: f64| {
            if time >= old_end {
                old_target
            } else {
                old_at((time - old_start).max(0.))
            }
        };
        let span = to - old_target;
        Segment {
            id: 0,
            start,
            duration: MAX_DURATION_MS.min((old_end - start).max(delta_ms)),
            target: to,
            at: Rc::new(move |t| {
                old_value(start + t)
                    + if t >= delta_ms {
                        span
                    } else {
                        delta.value(t / 1000.)
                    }
            }),
            speed: Rc::new(move |t| {
                (if start + t >= old_end {
                    0.
                } else {
                    old_speed(start + t - old_start)
                }) + if t >= delta_ms {
                    0.
                } else {
                    delta.velocity(t / 1000.)
                }
            }),
            is_fade: false,
            owner: owner.clone(),
            group: None,
            spring: Some(resolved),
            is_additive: true,
            handed_velocity: None,
        }
    }

    /// `additive` for a played curve: the old curve plus the new curve's
    /// shape, scaled to the difference between the targets.
    fn additive_curve_segment(
        &self,
        owner: &Owner,
        start: f64,
        curve: SharedCurve,
        old: &Segment,
    ) -> Segment {
        let from = curve.at(0.);
        let to = curve.at(curve.duration());
        let scale = (to - old.target) / (to - from);
        let ms = MAX_DURATION_MS.min(curve.duration() * 1000.);
        let old_end = old.end();
        let (old_at, old_speed, old_start, old_target) =
            (old.at.clone(), old.speed.clone(), old.start, old.target);
        let old_value = move |time: f64| {
            if time >= old_end {
                old_target
            } else {
                old_at((time - old_start).max(0.))
            }
        };
        let (value_curve, speed_curve) = (curve.clone(), curve);
        Segment {
            id: 0,
            start,
            duration: MAX_DURATION_MS.min((old_end - start).max(ms)),
            target: to,
            at: Rc::new(move |t| {
                old_value(start + t)
                    + if t >= ms {
                        to - old_target
                    } else {
                        (value_curve.at(t / 1000.) - from) * scale
                    }
            }),
            speed: Rc::new(move |t| {
                (if start + t >= old_end {
                    0.
                } else {
                    old_speed(start + t - old_start)
                }) + if t >= ms {
                    0.
                } else {
                    speed_curve.velocity(t / 1000.) * scale
                }
            }),
            is_fade: false,
            owner: owner.clone(),
            group: None,
            spring: None,
            is_additive: false,
            handed_velocity: None,
        }
    }

    fn curve_segment(owner: &Owner, start: f64, curve: SharedCurve) -> Segment {
        let target = curve.at(curve.duration());
        let (value_curve, speed_curve) = (curve.clone(), curve.clone());
        Segment {
            id: 0,
            start,
            duration: MAX_DURATION_MS.min(curve.duration() * 1000.),
            target,
            at: Rc::new(move |t| value_curve.at(t / 1000.)),
            speed: Rc::new(move |t| speed_curve.velocity(t / 1000.)),
            is_fade: false,
            owner: owner.clone(),
            group: None,
            spring: None,
            is_additive: false,
            handed_velocity: None,
        }
    }

    /// The Reduce Motion fade through `stops` on `timing.curves.move`.
    fn fade_segment(owner: &Owner, start: f64, stops: Vec<FadePoint>) -> Segment {
        let ease = cubic_bezier(timing::REDUCED_MOTION_CROSS_FADE.curve);
        let duration = stops.last().map_or(0., |stop| stop.at);
        let target = stops.last().map_or(0., |stop| stop.value);
        let at = Rc::new(move |t: f64| {
            for i in 0..stops.len().saturating_sub(1) {
                let (a, b) = (stops[i], stops[i + 1]);
                if t < b.at || i + 2 == stops.len() {
                    if b.at == a.at {
                        return b.value;
                    }
                    let p = ((t - a.at) / (b.at - a.at)).clamp(0., 1.);
                    return a.value + (b.value - a.value) * ease.ease(p);
                }
            }
            stops.last().map_or(0., |stop| stop.value)
        });
        let value = at.clone();
        Segment {
            id: 0,
            start,
            duration,
            target,
            at,
            speed: Rc::new(move |t| {
                (value(t + 0.5) - value((t - 0.5).max(0.))) / (if t >= 0.5 { 1. } else { 0.5 + t })
                    * 1000.
            }),
            is_fade: true,
            owner: owner.clone(),
            group: None,
            spring: None,
            is_additive: false,
            handed_velocity: None,
        }
    }

    fn insert_segment(&mut self, property: MotionProperty, mut segment: Segment) {
        segment.id = self.id();
        self.segments.insert(property, segment);
    }

    // -- Reduce Motion ----------------------------------------------------------

    fn opacity_heading(&self) -> f64 {
        let mut to = self.final_value(MotionProperty::Opacity);
        for landing in &self.landings {
            if let Some(fade_in) = landing.fade_in {
                to = fade_in.to;
            } else if let Some(restore) = landing.restore {
                to = restore;
            }
        }
        to
    }

    fn landing_index(&self, id: u64) -> Option<usize> {
        self.landings.iter().position(|landing| landing.id == id)
    }

    fn end_landing(&mut self, id: u64) -> Option<Landing> {
        let index = self.landing_index(id)?;
        let landing = self.landings.remove(index);
        self.clear_timer(landing.timer);
        Some(landing)
    }

    fn cancel_landing(&mut self, property: MotionProperty) {
        let ids: Vec<u64> = self.landings.iter().map(|landing| landing.id).collect();
        for id in ids {
            if property == MotionProperty::Opacity {
                if let Some(landing) = self.end_landing(id) {
                    self.land_now(landing);
                }
            } else if let Some(index) = self.landing_index(id)
                && self.landings[index].values.remove(&property).is_some()
            {
                let landing = &mut self.landings[index];
                let moved = landing
                    .owners
                    .remove(&property)
                    .unwrap_or_else(|| landing.owner.clone());
                self.settle(&moved, false);
            }
        }
    }

    fn land_now(&mut self, landing: Landing) {
        let mut changed = Vec::new();
        for (property, value) in &landing.values {
            self.segments.remove(property);
            self.set_resting(*property, *value);
            changed.push(*property);
        }
        if !changed.is_empty() {
            let now = self.now();
            self.commit(&changed, now);
        }
        for owner in std::iter::once(&landing.owner).chain(&landing.carried) {
            self.carried_owners.remove(&owner_key(owner));
            self.settle(owner, false);
        }
    }

    fn adopt_landings(
        &mut self,
        plan: &mut PlannedLanding,
        properties: &[MotionProperty],
    ) -> Adopted {
        let mut carried = Vec::new();
        let mut owners = HashMap::new();
        for landing in std::mem::take(&mut self.landings) {
            for (property, value) in &landing.values {
                let moved = landing
                    .owners
                    .get(property)
                    .cloned()
                    .unwrap_or_else(|| landing.owner.clone());
                if !properties.contains(property) && !plan.values.contains_key(property) {
                    plan.values.insert(*property, *value);
                    owners.insert(*property, moved);
                } else {
                    self.settle(&moved, false);
                }
            }
            carried.push(landing.owner.clone());
            carried.extend(landing.carried.iter().cloned());
            self.clear_timer(landing.timer);
        }
        for owner in &carried {
            self.carried_owners.insert(owner_key(owner));
        }
        Adopted { carried, owners }
    }

    fn join_waiting_dip(&mut self, owner: &Owner) {
        let Some(index) = self
            .landings
            .iter()
            .rposition(|landing| landing.fade_in.is_some())
        else {
            self.settle(owner, true);
            return;
        };
        let dip = &mut self.landings[index];
        let previous = dip.owner.clone();
        for property in dip.values.keys() {
            dip.owners
                .entry(*property)
                .or_insert_with(|| previous.clone());
        }
        dip.carried.push(previous.clone());
        dip.owner = owner.clone();
        self.carried_owners.insert(owner_key(&previous));
    }

    fn schedule_landing(
        &mut self,
        owner: &Owner,
        group: Option<Group>,
        time: f64,
        plan: PlannedLanding,
        adopted: Option<Adopted>,
    ) {
        let mut owners = HashMap::new();
        for property in plan.values.keys() {
            let moved = adopted
                .as_ref()
                .and_then(|a| a.owners.get(property).cloned())
                .unwrap_or_else(|| owner.clone());
            owners.insert(*property, moved);
        }
        let id = self.id();
        let waiting = group.as_ref().is_some_and(|g| !g.resolved());
        let start = group.as_ref().and_then(|g| g.start.get()).unwrap_or(time);
        self.landings.push(Landing {
            id,
            owner: owner.clone(),
            at: plan.at,
            values: plan.values,
            fade_in: plan.fade_in,
            restore: plan.restore,
            restore_is_borrowed: plan.restore_is_borrowed,
            carried: adopted.map(|a| a.carried).unwrap_or_default(),
            owners,
            timer: None,
            due: None,
            group: if waiting { group } else { None },
        });
        let timer = owner.timer.take();
        self.clear_timer(timer);
        if !waiting {
            self.arm_landing(id, start);
        }
    }

    fn arm_landing(&mut self, id: u64, start: f64) {
        let Some(index) = self.landing_index(id) else {
            return;
        };
        let due = start + self.landings[index].at;
        self.landings[index].due = Some(due);
        let timer = self.set_timer(due, TimerKind::Land(id));
        if let Some(index) = self.landing_index(id) {
            self.landings[index].timer = Some(timer);
        }
    }

    /// Writes the travel while the element is transparent; in a dip the fade
    /// back in takes what is left of the cross-fade (upstream `land`).
    fn land(&mut self, id: u64) {
        let Some(index) = self.landing_index(id) else {
            return;
        };
        let landing = self.landings.remove(index);
        let time = self.now();
        let mut changed = Vec::new();
        for (property, value) in &landing.values {
            self.segments.remove(property);
            self.set_resting(*property, *value);
            changed.push(*property);
        }
        let mut end = time;
        let late = landing.due.map_or(0., |due| (time - due).max(0.));
        let appears = landing.fade_in.is_some_and(|fade_in| late >= fade_in.ms);
        let opacity = MotionProperty::Opacity;
        match (landing.fade_in, appears, landing.restore) {
            (Some(fade_in), false, _) => {
                let ms = fade_in.ms - late;
                let from = self.value_at(opacity, time);
                let fade = Self::fade_segment(
                    &landing.owner,
                    time,
                    vec![
                        FadePoint {
                            at: 0.,
                            value: from,
                        },
                        FadePoint {
                            at: ms,
                            value: fade_in.to,
                        },
                    ],
                );
                self.insert_segment(opacity, fade);
                changed.push(opacity);
                end = time + ms;
            }
            (Some(fade_in), true, _) => {
                self.segments.remove(&opacity);
                self.set_resting(opacity, if fade_in.is_borrowed { 1. } else { fade_in.to });
                changed.push(opacity);
            }
            (None, _, Some(restore)) => {
                self.segments.remove(&opacity);
                self.set_resting(
                    opacity,
                    if landing.restore_is_borrowed {
                        1.
                    } else {
                        restore
                    },
                );
                changed.push(opacity);
            }
            (None, _, None) => {}
        }
        self.commit(&changed, time);
        let borrowed_in = landing.fade_in.is_some_and(|f| f.is_borrowed);
        if landing.restore_is_borrowed || (appears && borrowed_in) {
            self.release_own_opacity();
        } else if let Some(fade_in) = landing.fade_in.filter(|f| f.is_borrowed) {
            self.borrowed_opacity = Some(fade_in.to);
        }
        for carried in &landing.carried {
            self.carried_owners.remove(&owner_key(carried));
            self.settle(carried, true);
        }
        let owner = landing.owner.clone();
        if owner.settled.get().is_some() {
            return;
        }
        if end <= time {
            self.settle(&owner, true);
        } else {
            let timer = self.set_timer(end, TimerKind::Settle(owner.clone()));
            owner.timer.set(Some(timer));
        }
    }

    /// Lands every move still waiting, at once (upstream `landPending`).
    fn land_pending(&mut self, finished: bool) {
        if self.landings.is_empty() {
            return;
        }
        let mut changed = Vec::new();
        let mut handed_back = false;
        let opacity = MotionProperty::Opacity;
        for landing in std::mem::take(&mut self.landings) {
            self.clear_timer(landing.timer);
            for (property, value) in &landing.values {
                self.segments.remove(property);
                self.set_resting(*property, *value);
                if !changed.contains(property) {
                    changed.push(*property);
                }
            }
            if landing.fade_in.is_some() || landing.restore.is_some() {
                let borrowed =
                    landing.fade_in.is_some_and(|f| f.is_borrowed) || landing.restore_is_borrowed;
                self.segments.remove(&opacity);
                let value = if borrowed {
                    1.
                } else {
                    landing
                        .fade_in
                        .map(|f| f.to)
                        .or(landing.restore)
                        .unwrap_or(1.)
                };
                self.set_resting(opacity, value);
                if !changed.contains(&opacity) {
                    changed.push(opacity);
                }
                if borrowed {
                    self.borrowed_opacity = None;
                    handed_back = true;
                }
            }
            for owner in std::iter::once(&landing.owner).chain(&landing.carried) {
                self.carried_owners.remove(&owner_key(owner));
                self.settle(owner, finished);
            }
        }
        let now = self.now();
        self.commit(&changed, now);
        if handed_back {
            self.release_own_opacity();
        }
    }

    fn hand_back_borrow(&mut self) {
        if self.borrowed_opacity.is_none() {
            return;
        }
        self.borrowed_opacity = None;
        self.segments.remove(&MotionProperty::Opacity);
        self.set_resting(MotionProperty::Opacity, 1.);
        self.release_own_opacity();
    }

    fn take_over_landings(&mut self, properties: &[MotionProperty], names_opacity: bool) {
        for property in properties {
            if *property != MotionProperty::Opacity {
                self.cancel_landing(*property);
            }
        }
        if !names_opacity {
            let empty: Vec<u64> = self
                .landings
                .iter()
                .filter(|landing| landing.values.is_empty() && landing.fade_in.is_some())
                .map(|landing| landing.id)
                .collect();
            for id in empty {
                if let Some(index) = self.landing_index(id) {
                    let timer = self.landings[index].timer.take();
                    self.clear_timer(timer);
                    self.land(id);
                }
            }
            return;
        }
        let ids: Vec<u64> = self.landings.iter().map(|landing| landing.id).collect();
        for id in ids {
            let Some(index) = self.landing_index(id) else {
                continue;
            };
            let owners: Vec<Owner> = std::iter::once(self.landings[index].owner.clone())
                .chain(self.landings[index].carried.iter().cloned())
                .collect();
            for owner in &owners {
                if self.named_opacity.contains(&owner_key(owner)) {
                    self.settle(owner, false);
                }
            }
            if !self.landings[index].values.is_empty() {
                continue;
            }
            let Some(landing) = self.end_landing(id) else {
                continue;
            };
            if landing.fade_in.is_some_and(|f| f.is_borrowed) || landing.restore_is_borrowed {
                self.borrowed_opacity = None;
            }
            for owner in &owners {
                self.carried_owners.remove(&owner_key(owner));
                self.settle(owner, false);
            }
        }
    }

    fn reduced_motion_plan(
        &mut self,
        owner: &Owner,
        time: f64,
        targets: &BTreeMap<MotionProperty, f64>,
        properties: &[MotionProperty],
        fade: bool,
        role: Option<CrossfadeAs>,
    ) -> ReducedPlan {
        let heading = self.opacity_heading();
        let names_opacity = targets.contains_key(&MotionProperty::Opacity) || role.is_some();
        self.take_over_landings(properties, names_opacity);
        if targets.contains_key(&MotionProperty::Opacity) {
            self.named_opacity.insert(owner_key(owner));
        }
        self.heading_before_take_over = Some(heading);
        let plan = plan_reduced_motion(&*self, time, targets, properties, fade, role);
        self.heading_before_take_over = None;
        let mut next = BTreeMap::new();
        for property in plan.at_once {
            next.insert(property, None);
        }
        if let Some(stops) = plan.opacity_fade {
            next.insert(
                MotionProperty::Opacity,
                Some(Self::fade_segment(owner, time, stops)),
            );
        }
        ReducedPlan {
            next,
            holds: plan.holds,
            landing: plan.landing,
            borrowed: plan.borrowed,
            joins: plan.joins,
        }
    }

    fn reduced_motion_mode(&self, touch_release: bool) -> ReducedMotion {
        if !self.reduced {
            ReducedMotion::Off
        } else if self.coarse || touch_release {
            ReducedMotion::Crossfade
        } else {
            ReducedMotion::Instant
        }
    }
}

impl ReducedMotionView for Engine {
    fn value_at(&self, property: MotionProperty, time: f64) -> f64 {
        Engine::value_at(self, property, time)
    }
    fn opacity_heading(&self) -> f64 {
        self.heading_before_take_over
            .unwrap_or_else(|| Engine::opacity_heading(self))
    }
    fn own_opacity(&self) -> f64 {
        self.own_opacity
    }
    fn writes_own_opacity(&self) -> bool {
        self.opacity_inline
    }
    fn borrowed_opacity(&self) -> Option<f64> {
        self.borrowed_opacity
    }
    fn has_opacity_curve(&self) -> bool {
        self.segments.contains_key(&MotionProperty::Opacity)
    }
    fn resting_opacity(&self) -> f64 {
        self.resting(MotionProperty::Opacity)
    }
    fn has_pending_landing(&self) -> bool {
        !self.landings.is_empty()
    }
    fn has_waiting_dip(&self) -> bool {
        self.landings
            .iter()
            .any(|landing| landing.fade_in.is_some())
    }
}

/// Upstream's channels: x and y share `translate`, the scale axes share
/// `scale`.
fn channel_of(property: MotionProperty) -> Vec<MotionProperty> {
    match property {
        MotionProperty::X | MotionProperty::Y => vec![MotionProperty::X, MotionProperty::Y],
        MotionProperty::ScaleX | MotionProperty::ScaleY => {
            vec![MotionProperty::ScaleX, MotionProperty::ScaleY]
        }
        other => vec![other],
    }
}

// ---------------------------------------------------------------------------
// The public engine
// ---------------------------------------------------------------------------

/// The animator engine for one element (upstream `createMotionEngine`'s
/// `Animator`). Cloning gives another handle to the same engine.
#[derive(Clone)]
pub struct MotionEngine {
    inner: Rc<RefCell<Engine>>,
}

impl fmt::Debug for MotionEngine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MotionEngine").finish_non_exhaustive()
    }
}

impl Default for MotionEngine {
    fn default() -> Self {
        Self::new(EngineOptions::default())
    }
}

impl MotionEngine {
    /// An engine at `options.initial` (identity elsewhere).
    pub fn new(options: EngineOptions) -> Self {
        let mut resting = BTreeMap::new();
        for property in MotionProperty::ALL {
            resting.insert(property, property.identity());
        }
        for (property, value) in options.initial.expand() {
            resting.insert(property, value);
        }
        let opacity_inline = resting[&MotionProperty::Opacity] != 1.;
        Self {
            inner: Rc::new(RefCell::new(Engine {
                clock: options.now,
                manual_now: 0.,
                frames: options.frames,
                reduced: false,
                coarse: false,
                tempo: 1.,
                own_opacity: 1.,
                resting,
                segments: BTreeMap::new(),
                pending: Vec::new(),
                borrowed_opacity: None,
                opacity_inline,
                landings: Vec::new(),
                carried_owners: HashSet::new(),
                named_opacity: HashSet::new(),
                heading_before_take_over: None,
                shown: BTreeMap::new(),
                timers: Vec::new(),
                next_id: 0,
                disposed: false,
            })),
        }
    }

    /// An engine on `clock` (ms).
    pub fn with_clock(clock: impl Fn() -> f64 + 'static) -> Self {
        Self::new(EngineOptions {
            now: Some(Rc::new(clock)),
            ..Default::default()
        })
    }

    /// The engine's time now, ms.
    pub fn now(&self) -> f64 {
        self.inner.borrow().now()
    }

    /// Moves the engine's own clock to `now` (an injected clock is read
    /// instead) and runs what came due: a handle's end, a Reduce Motion
    /// landing. Call it each frame.
    pub fn advance(&self, now: f64) {
        let mut engine = self.inner.borrow_mut();
        engine.manual_now = now;
        engine.run_timers(now);
    }

    /// A frame at `time` commits the calls waiting for one: a call from
    /// rest starts here, and a retarget whose frame comes late restarts its
    /// spring here (needs [`EngineOptions::frames`]).
    pub fn frame(&self, time: f64) {
        let mut engine = self.inner.borrow_mut();
        let pending = engine.pending.clone();
        for group in pending {
            let left = group.frames_left.get().saturating_sub(1);
            group.frames_left.set(left);
            if left > 0 {
                continue;
            }
            // A call whose curves all went before its frame (stopped, set)
            // never started: it resolves at its own time.
            let waiting = engine.segments.values().any(|segment| {
                segment
                    .group
                    .as_ref()
                    .is_some_and(|g| Rc::ptr_eq(g, &group))
            });
            let start = if waiting { time } else { group.provisional };
            engine.resolve_group(&group, start);
        }
        engine.look_at_shown(time);
        engine.sync();
    }

    /// Whether the person's Reduce Motion setting is on, as the caller reads
    /// it (`rustryx_primitives::prefers_reduced_motion`, or a theme's mark).
    pub fn set_reduced_motion(&self, reduced: bool) {
        self.inner.borrow_mut().reduced = reduced;
    }

    /// Whether the primary pointer is coarse (`(pointer: coarse)`).
    pub fn set_coarse_pointer(&self, coarse: bool) {
        self.inner.borrow_mut().coarse = coarse;
    }

    /// The tempo of the theme the element is in (`theme_tempo`): every
    /// spring role plays at it.
    pub fn set_tempo(&self, tempo: f64) {
        self.inner.borrow_mut().tempo = super::theme_tempo::theme_tempo(Some(tempo));
    }

    /// The tempo springs play at.
    pub fn tempo(&self) -> f64 {
        self.inner.borrow().tempo
    }

    /// The element's own opacity where the engine writes none (upstream
    /// reads the stylesheet's): a dip on a surface no call owns opacity on
    /// starts from and returns to it.
    pub fn set_own_opacity(&self, opacity: f64) {
        self.inner.borrow_mut().own_opacity = opacity.clamp(0., 1.);
    }

    /// Whether the engine holds an opacity of its own: when false, the
    /// element shows its own opacity, whatever `get(opacity)` reads.
    pub fn writes_opacity(&self) -> bool {
        self.inner.borrow().opacity_inline
    }

    /// The opacity the element shows at `time`: the engine's while it writes
    /// one, else the element's own.
    pub fn shown_opacity(&self, time: Option<f64>) -> f64 {
        let mut engine = self.inner.borrow_mut();
        engine.sync();
        let time = time.unwrap_or_else(|| engine.now());
        if engine.opacity_inline || engine.segment_at(MotionProperty::Opacity, time).is_some() {
            engine.value_at(MotionProperty::Opacity, time).clamp(0., 1.)
        } else {
            engine.own_opacity
        }
    }

    /// What Reduce Motion makes of a call right now (upstream
    /// `reducedMotion`): feedback with no fade of its own (a press lift)
    /// plays only when `Off`.
    pub fn reduced_motion(&self, touch_release: bool) -> ReducedMotion {
        self.inner.borrow().reduced_motion_mode(touch_release)
    }

    /// The presentation value at `time` (default now), from the curve.
    pub fn get(&self, key: impl Into<MotionKey>, time: Option<f64>) -> f64 {
        let mut engine = self.inner.borrow_mut();
        engine.sync();
        let time = time.unwrap_or_else(|| engine.now());
        engine.value_at(key.into().read(), time)
    }

    /// The presentation velocity at `time` (default now), units/s.
    pub fn get_velocity(&self, key: impl Into<MotionKey>, time: Option<f64>) -> f64 {
        let mut engine = self.inner.borrow_mut();
        engine.sync();
        let time = time.unwrap_or_else(|| engine.now());
        engine.velocity_at(key.into().read(), time)
    }

    /// Every value at `time` (default now).
    pub fn values(&self, time: Option<f64>) -> BTreeMap<MotionProperty, f64> {
        let mut engine = self.inner.borrow_mut();
        engine.sync();
        let time = time.unwrap_or_else(|| engine.now());
        MotionProperty::ALL
            .into_iter()
            .map(|property| (property, engine.value_at(property, time)))
            .collect()
    }

    /// Puts values in place now, ending their curves: a drag's per-frame
    /// write.
    pub fn set(&self, values: MotionValues) {
        let mut engine = self.inner.borrow_mut();
        engine.sync();
        engine.flush_deferred();
        let time = engine.now();
        let next = values.expand();
        let mut changed = Vec::new();
        for (property, value) in next {
            engine.interrupt(property, time);
            engine.segments.remove(&property);
            engine.shown.remove(&property);
            engine.set_resting(property, value);
            changed.push(property);
        }
        engine.commit(&changed, time);
    }

    /// Springs to `to` from the presentation values.
    pub fn animate(
        &self,
        to: MotionValues,
        transition: impl Into<SpringTransition>,
        options: AnimateOptions,
    ) -> MotionHandle {
        let transition = transition.into();
        let mut engine = self.inner.borrow_mut();
        engine.sync();
        if options.start != Some(StartMode::Presented) {
            engine.flush_deferred();
        }
        let time = options.at.unwrap_or_else(|| engine.now());
        let targets = to.expand();
        let velocities = transition.velocity.expand();
        let owner = new_owner();
        let properties: Vec<MotionProperty> = targets.keys().copied().collect();
        let mut next: BTreeMap<MotionProperty, Option<Segment>> = BTreeMap::new();
        let mut plan = None;
        if engine.reduced {
            let fade = engine.coarse || options.touch_release;
            let reduced = engine.reduced_motion_plan(
                &owner,
                time,
                &targets,
                &properties,
                fade,
                options.crossfade_as,
            );
            next.extend(reduced.next.clone());
            plan = Some(reduced);
        } else {
            for property in &properties {
                let target = targets[property];
                let handed = velocities.get(property).copied();
                let running = if options.additive && handed.is_none() {
                    engine.segment_at(*property, time).cloned()
                } else {
                    None
                };
                if let Some(running) = running.filter(|s| !s.is_held()) {
                    let segment = engine.additive_segment(
                        &owner,
                        *property,
                        time,
                        target,
                        transition.spring,
                        &running,
                    );
                    next.insert(*property, Some(segment));
                    continue;
                }
                let current = engine.velocity_at(*property, time);
                let segment = engine.spring_segment(
                    &owner,
                    *property,
                    time,
                    target,
                    transition.spring,
                    handed.unwrap_or(current),
                );
                let is_handed = handed.is_some_and(|handed| {
                    (handed - current).abs() > sample_tolerance(*property) * REST_FRAMES_PER_SECOND
                });
                next.insert(
                    *property,
                    segment.map(|mut segment| {
                        if is_handed {
                            segment.handed_velocity = handed;
                        }
                        segment
                    }),
                );
            }
        }
        let handle = engine.apply_call(
            &owner,
            time,
            next,
            &targets,
            plan,
            options,
            Some(transition.spring),
        );
        drop(engine);
        handle.into_handle(self)
    }

    /// Plays given curves (`scale` drives both axes), as a spring is played:
    /// values, velocities, stop and completion work the same, and a later
    /// `animate` starts from the curve's value and velocity. Its handle has no
    /// spring of its own: `retarget` needs one.
    pub fn play(
        &self,
        curves: MotionCurves,
        options: AnimateOptions,
    ) -> Result<MotionHandle, MotionError> {
        let by_property = curves.expand();
        for curve in by_property.values() {
            let duration = curve.duration();
            if !duration.is_finite() || duration < 0. {
                return Err(MotionError::BadDuration(duration));
            }
        }
        let mut engine = self.inner.borrow_mut();
        engine.sync();
        if options.start != Some(StartMode::Presented) {
            engine.flush_deferred();
        }
        let time = options.at.unwrap_or_else(|| engine.now());
        let owner = new_owner();
        let properties: Vec<MotionProperty> = by_property.keys().copied().collect();
        let targets: BTreeMap<MotionProperty, f64> = by_property
            .iter()
            .map(|(property, curve)| (*property, curve.at(curve.duration())))
            .collect();
        let plan = if engine.reduced {
            let fade = engine.coarse || options.touch_release;
            Some(engine.reduced_motion_plan(
                &owner,
                time,
                &targets,
                &properties,
                fade,
                options.crossfade_as,
            ))
        } else {
            None
        };
        let mut next: BTreeMap<MotionProperty, Option<Segment>> = plan
            .as_ref()
            .map(|plan| plan.next.clone())
            .unwrap_or_default();
        for (property, curve) in &by_property {
            if next.contains_key(property) {
                continue;
            }
            let running = if options.additive {
                engine.segment_at(*property, time).cloned()
            } else {
                None
            };
            if let Some(running) = running.filter(|s| !s.is_held())
                && curve.duration() > 0.
                && (curve.at(curve.duration()) - curve.at(0.)).abs() > 1e-9
            {
                let segment = engine.additive_curve_segment(&owner, time, curve.clone(), &running);
                next.insert(*property, Some(segment));
                continue;
            }
            next.insert(
                *property,
                (curve.duration() > 0.).then(|| Engine::curve_segment(&owner, time, curve.clone())),
            );
        }
        let handle = engine.apply_call(&owner, time, next, &targets, plan, options, None);
        drop(engine);
        Ok(handle.into_handle(self))
    }

    /// Plays momentum curves as given, on the translate axes only: a
    /// released scroll view's coast and edge bounce (`coast`). Under Reduce
    /// Motion momentum keeps its motion (DR2, DEC-2); a curve marked as a
    /// transition plays through the policy, as `animate` does.
    pub fn decay(&self, curves: DecayCurves, options: DecayOptions) -> MotionHandle {
        let (time, reduce) = {
            let mut engine = self.inner.borrow_mut();
            engine.sync();
            engine.flush_deferred();
            (options.at.unwrap_or_else(|| engine.now()), engine.reduced)
        };
        let axes = [MotionProperty::X, MotionProperty::Y];
        let mut transitions = MotionValues::new();
        for property in axes {
            if let Some(curve) = curves.get(property)
                && reduce
                && curve.is_transition()
            {
                transitions = transitions.with(property, curve.at(curve.duration()));
            }
        }
        let transition_targets = transitions.expand();
        let transitioned = (!transition_targets.is_empty()).then(|| {
            self.animate(
                transitions,
                SpringTransition::new(REDUCED_TRANSITION),
                AnimateOptions {
                    at: Some(time),
                    touch_release: options.touch_release,
                    ..Default::default()
                },
            )
        });
        let properties: Vec<MotionProperty> = axes
            .into_iter()
            .filter(|p| curves.get(*p).is_some() && !transition_targets.contains_key(p))
            .collect();
        if let Some(transitioned) = &transitioned
            && properties.is_empty()
        {
            return transitioned.clone();
        }
        let owner = new_owner();
        let mut engine = self.inner.borrow_mut();
        let group = engine.begin_group(time, &properties, options.at, None);
        engine.note_shown(&properties, time, &group, options.at);
        for property in &properties {
            let curve = curves.get(*property).cloned().expect("filtered above");
            engine.interrupt(*property, time);
            if curve.duration().is_nan() || curve.duration() <= 0. {
                engine.segments.remove(property);
                engine.set_resting(*property, curve.at(0.));
                continue;
            }
            let segment = Engine::curve_segment(&owner, time, curve);
            engine.insert_segment(*property, segment);
        }
        engine.commit_call(group.clone(), &owner, &properties, time, false);
        let handle = engine.make_handle(&owner, properties, time, group, false, None);
        drop(engine);
        let mut handle = handle.into_handle(self);
        if let Some(faded) = transitioned {
            handle.faded = Some(Box::new(faded));
        }
        handle
    }

    /// Moves properties by the given amounts at once, as when a layout shift
    /// moves the element under them: a spring keeps its spring, velocity and
    /// target (restarting from the shifted value); any other moving curve
    /// shifts whole; a resting value takes the offset.
    pub fn rebase(&self, deltas: MotionValues, options: RebaseOptions) {
        let mut engine = self.inner.borrow_mut();
        engine.sync();
        engine.flush_deferred();
        let time = options.at.unwrap_or_else(|| engine.now());
        let towards = options.towards.expand();
        let mut changed = Vec::new();
        let mut owners: Vec<Owner> = Vec::new();
        for (property, delta) in deltas.expand() {
            if delta == 0. {
                continue;
            }
            if let Some(noted) = engine.shown.get_mut(&property) {
                noted.offset += delta;
            }
            let Some(segment) = engine.segment_at(property, time).cloned() else {
                changed.push(property);
                let value = engine.value_at(property, time) + delta;
                engine.set_resting(property, value);
                engine.segments.remove(&property);
                continue;
            };
            if !owners.iter().any(|o| Rc::ptr_eq(o, &segment.owner)) {
                owners.push(segment.owner.clone());
            }
            let group = segment.group.clone().filter(|g| !g.resolved());
            changed.push(property);
            if let (Some(_), Some(heading)) = (segment.spring, towards.get(&property))
                && (segment.target + delta - heading).abs() <= SAME_VALUE
            {
                let mut whole = segment.shifted(delta);
                whole.target = *heading;
                engine.insert_segment(property, whole);
                continue;
            }
            if let Some(spring) = segment.spring {
                let from = engine.value_at(property, time) + delta;
                let velocity = engine.velocity_at(property, time);
                let start = group.as_ref().map_or(time, |g| g.provisional);
                let restarted = engine.spring_from(
                    &segment.owner,
                    property,
                    start,
                    from,
                    velocity,
                    segment.target,
                    spring,
                );
                match restarted {
                    Some(mut next) => {
                        next.group = group;
                        engine.insert_segment(property, next);
                    }
                    None => {
                        engine.segments.remove(&property);
                        engine.set_resting(property, segment.target);
                    }
                }
            } else {
                let shifted = segment.shifted(delta);
                engine.insert_segment(property, shifted);
            }
        }
        engine.commit(&changed, time);
        for owner in owners {
            if owner.settled.get().is_some() || owner.timer.get().is_none() {
                continue;
            }
            engine.redate(&owner, time);
        }
    }

    /// Freezes properties (all by default) at their presentation values. A
    /// Reduce Motion fade is never left half done: it finishes.
    pub fn stop(&self, keys: Option<&[MotionKey]>) {
        let mut engine = self.inner.borrow_mut();
        engine.sync();
        engine.stop(keys.map(expand_list));
    }

    /// Whether none of `keys` (all by default) is moving and no Reduce
    /// Motion landing waits (upstream `atRest` resolves then).
    pub fn at_rest(&self, keys: Option<&[MotionKey]>) -> bool {
        let mut engine = self.inner.borrow_mut();
        engine.sync();
        let time = engine.now();
        engine.disposed
            || (engine.landings.is_empty() && engine.rest_time(keys.map(expand_list)) <= time)
    }

    /// When the last curve on `keys` (all by default) ends, ms: the time
    /// `at_rest` turns true, Reduce Motion landings aside.
    pub fn rest_time(&self, keys: Option<&[MotionKey]>) -> f64 {
        let mut engine = self.inner.borrow_mut();
        engine.sync();
        engine.rest_time(keys.map(expand_list))
    }

    /// Whether nothing moves and nothing waits: no curve, no landing, no
    /// start. A frame loop stops asking for frames then.
    pub fn is_settled(&self) -> bool {
        let mut engine = self.inner.borrow_mut();
        engine.sync();
        let time = engine.now();
        engine.disposed
            || engine.pending.is_empty()
                && engine.landings.is_empty()
                && engine.rest_time(None) <= time
                && engine.shown.is_empty()
    }

    /// Cancels everything: a Reduce Motion move still waiting lands where it
    /// was going, and every call in flight finishes `Some(false)`.
    pub fn dispose(&self) {
        let mut engine = self.inner.borrow_mut();
        engine.sync();
        engine.flush_deferred();
        engine.land_pending(false);
        let time = engine.now();
        for property in MotionProperty::ALL {
            engine.interrupt(property, time);
        }
        engine.shown.clear();
        engine.pending.clear();
        engine.hand_back_borrow();
        engine.timers.clear();
        engine.disposed = true;
    }

    /// Whether [`dispose`](Self::dispose) ran.
    pub fn is_disposed(&self) -> bool {
        self.inner.borrow().disposed
    }
}

impl Engine {
    fn rest_time(&self, properties: Option<Vec<MotionProperty>>) -> f64 {
        let list = properties.unwrap_or_else(|| MotionProperty::ALL.to_vec());
        let mut end = self.now();
        for property in list {
            if let Some(segment) = self.segments.get(&property) {
                end = end.max(segment.end());
            }
        }
        end
    }

    fn stop(&mut self, properties: Option<Vec<MotionProperty>>) {
        self.flush_deferred();
        let time = self.now();
        let list = properties.unwrap_or_else(|| MotionProperty::ALL.to_vec());
        let was_borrowed =
            self.borrowed_opacity.is_some() || self.landings.iter().any(|l| l.restore_is_borrowed);
        let heading = self.opacity_heading();
        let mut changed = Vec::new();
        for property in list {
            self.cancel_landing(property);
            let Some(segment) = self.segments.get(&property).cloned() else {
                continue;
            };
            let noted = self.shown.remove(&property);
            let value = if !segment.is_fade {
                match noted {
                    Some(noted) if time - noted.since > LATE_START_MS => {
                        Self::shown_at(&noted, time).0
                    }
                    _ => self.value_at(property, time),
                }
            } else if was_borrowed {
                1.
            } else {
                heading
            };
            self.interrupt(property, time);
            self.segments.remove(&property);
            self.set_resting(property, value);
            changed.push(property);
        }
        self.commit(&changed, time);
        if was_borrowed && changed.contains(&MotionProperty::Opacity) {
            self.release_own_opacity();
        }
    }

    /// Applies one `animate` or `play` call's curves (upstream's shared tail
    /// of both).
    #[allow(clippy::too_many_arguments)]
    fn apply_call(
        &mut self,
        owner: &Owner,
        time: f64,
        next: BTreeMap<MotionProperty, Option<Segment>>,
        targets: &BTreeMap<MotionProperty, f64>,
        plan: Option<ReducedPlan>,
        options: AnimateOptions,
        spring: Option<MotionSpring>,
    ) -> PendingHandle {
        let keys: Vec<MotionProperty> = next.keys().copied().collect();
        let group = self.begin_group(time, &keys, options.at, options.start);
        self.note_shown(&keys, time, &group, options.at);
        let mut plan = plan;
        let adopted = plan
            .as_mut()
            .and_then(|plan| plan.landing.as_mut())
            .map(|landing| self.adopt_landings(landing, &keys));
        let mut changed = Vec::new();
        for (property, segment) in next {
            self.interrupt(property, time);
            changed.push(property);
            match segment {
                None => {
                    let value = plan
                        .as_ref()
                        .and_then(|plan| plan.holds.get(&property).copied())
                        .or_else(|| targets.get(&property).copied())
                        .unwrap_or_else(|| self.value_at(property, time));
                    self.segments.remove(&property);
                    self.set_resting(property, value);
                }
                Some(segment) => self.insert_segment(property, segment),
            }
        }
        let presented = options.start == Some(StartMode::Presented) && options.at.is_none();
        self.commit_call(group.clone(), owner, &changed, time, presented);
        let joins = plan.as_ref().is_some_and(|plan| plan.joins);
        if let Some(borrowed) = plan.as_ref().and_then(|plan| plan.borrowed) {
            self.borrowed_opacity = Some(borrowed);
        }
        if joins {
            self.join_waiting_dip(owner);
        }
        let properties = if joins {
            vec![MotionProperty::Opacity]
        } else {
            changed
        };
        let handle = self.make_handle(owner, properties, time, group.clone(), joins, spring);
        if let Some(landing) = plan.and_then(|plan| plan.landing) {
            self.schedule_landing(owner, group, time, landing, adopted);
        }
        handle
    }

    fn make_handle(
        &mut self,
        owner: &Owner,
        properties: Vec<MotionProperty>,
        time: f64,
        group: Option<Group>,
        settles_with_landing: bool,
        spring: Option<MotionSpring>,
    ) -> PendingHandle {
        let mut end = time;
        for property in &properties {
            if let Some(segment) = self.segments.get(property)
                && Rc::ptr_eq(&segment.owner, owner)
            {
                end = end.max(segment.end());
            }
        }
        if settles_with_landing {
        } else if properties.is_empty() || end <= time {
            self.settle(owner, true);
        } else {
            let id = self.set_timer(end, TimerKind::Settle(owner.clone()));
            owner.timer.set(Some(id));
        }
        let mut curves = BTreeMap::new();
        for property in &properties {
            if let Some(segment) = self.segments.get(property)
                && Rc::ptr_eq(&segment.owner, owner)
            {
                curves.insert(*property, segment.clone());
            }
        }
        PendingHandle {
            owner: owner.clone(),
            properties,
            time,
            group,
            curves,
            spring,
        }
    }
}

/// A handle before it knows its engine.
struct PendingHandle {
    owner: Owner,
    properties: Vec<MotionProperty>,
    time: f64,
    group: Option<Group>,
    curves: BTreeMap<MotionProperty, Segment>,
    spring: Option<MotionSpring>,
}

impl PendingHandle {
    fn into_handle(self, engine: &MotionEngine) -> MotionHandle {
        MotionHandle {
            engine: Rc::downgrade(&engine.inner),
            owner: self.owner,
            properties: self.properties,
            time: self.time,
            group: self.group,
            curves: Rc::new(self.curves),
            spring: self.spring,
            faded: None,
        }
    }
}

/// One `animate` / `play` / `decay` call's motion (upstream `MotionHandle`).
#[derive(Clone)]
pub struct MotionHandle {
    engine: Weak<RefCell<Engine>>,
    owner: Owner,
    properties: Vec<MotionProperty>,
    time: f64,
    group: Option<Group>,
    curves: Rc<BTreeMap<MotionProperty, Segment>>,
    spring: Option<MotionSpring>,
    /// A `decay` under Reduce Motion: the axes its transitions cross-faded.
    faded: Option<Box<MotionHandle>>,
}

impl fmt::Debug for MotionHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MotionHandle")
            .field("properties", &self.properties())
            .field("finished", &self.finished())
            .finish_non_exhaustive()
    }
}

impl MotionHandle {
    fn engine(&self) -> Option<Rc<RefCell<Engine>>> {
        self.engine.upgrade()
    }

    fn sync(&self) {
        if let Some(engine) = self.engine() {
            engine.borrow_mut().sync();
        }
    }

    /// The properties this call drives.
    pub fn properties(&self) -> Vec<MotionProperty> {
        let mut properties = self.properties.clone();
        if let Some(faded) = &self.faded {
            properties.extend(faded.properties());
        }
        properties
    }

    /// `Some(true)` once every property reached the end of this call's curve,
    /// `Some(false)` as soon as any of them is retargeted, stopped or set
    /// before that; `None` while it runs. Poll it after
    /// [`MotionEngine::advance`].
    pub fn finished(&self) -> Option<bool> {
        self.sync();
        let own = self.owner.settled.get();
        match &self.faded {
            None => own,
            Some(faded) => match (own, faded.finished()) {
                (Some(a), Some(b)) => Some(a && b),
                _ => None,
            },
        }
    }

    /// Whether [`finished`](Self::finished) has settled either way.
    pub fn is_settled(&self) -> bool {
        self.finished().is_some()
    }

    /// When the motion started on the engine's clock, ms: a call from rest
    /// once its frame starts it (`None` until then), otherwise the call's
    /// time or `at`.
    pub fn started(&self) -> Option<f64> {
        match &self.group {
            Some(group) => group.start.get(),
            None => Some(self.time),
        }
    }

    fn owns(&self, property: MotionProperty) -> bool {
        self.faded
            .as_ref()
            .is_some_and(|faded| faded.properties.contains(&property))
    }

    /// The presentation value of a property at `time` (default now).
    pub fn value(&self, key: impl Into<MotionKey>, time: Option<f64>) -> f64 {
        let property = key.into().read();
        if self.owns(property)
            && let Some(faded) = &self.faded
        {
            return faded.value(property, time);
        }
        let Some(engine) = self.engine() else {
            return property.identity();
        };
        let mut engine = engine.borrow_mut();
        engine.sync();
        let time = time.unwrap_or_else(|| engine.now());
        engine.value_at(property, time)
    }

    /// The presentation velocity of a property at `time`, units/s.
    pub fn velocity(&self, key: impl Into<MotionKey>, time: Option<f64>) -> f64 {
        let property = key.into().read();
        if self.owns(property)
            && let Some(faded) = &self.faded
        {
            return faded.velocity(property, time);
        }
        let Some(engine) = self.engine() else {
            return 0.;
        };
        let mut engine = engine.borrow_mut();
        engine.sync();
        let time = time.unwrap_or_else(|| engine.now());
        engine.velocity_at(property, time)
    }

    /// The curve this call started on `key` (`scale` reads `scaleX`), in seconds
    /// from the call's start: a companion follows it on the same clock.
    /// `None` for a property the call does not move, or moves without
    /// travel (Reduce Motion).
    pub fn curve(&self, key: impl Into<MotionKey>) -> Option<SharedCurve> {
        let segment = self.curves.get(&key.into().read())?.clone();
        Some(Rc::new(SegmentCurve(segment)))
    }

    /// New targets for this call's properties, keeping their velocity, on
    /// the call's spring unless `transition` gives one.
    pub fn retarget(
        &self,
        to: MotionValues,
        transition: Option<SpringTransition>,
        options: AnimateOptions,
    ) -> Result<MotionHandle, MotionError> {
        let transition = match (transition, self.spring) {
            (Some(transition), _) => transition,
            (None, Some(spring)) => SpringTransition::new(spring),
            (None, None) => return Err(MotionError::NeedsSpring),
        };
        let engine = self.engine().map(|inner| MotionEngine { inner });
        match engine {
            Some(engine) => Ok(engine.animate(to, transition, options)),
            None => Err(MotionError::NeedsSpring),
        }
    }

    /// Freezes this call's properties at their presentation values, and its
    /// travel still waiting to land under Reduce Motion (AST-094 FR6).
    pub fn stop(&self) {
        if let Some(inner) = self.engine() {
            let mut engine = inner.borrow_mut();
            engine.sync();
            let list: Vec<MotionProperty> = MotionProperty::ALL
                .into_iter()
                .filter(|property| {
                    self.properties.contains(property)
                        && (engine
                            .segments
                            .get(property)
                            .is_some_and(|s| Rc::ptr_eq(&s.owner, &self.owner))
                            || engine.landings.iter().any(|landing| {
                                landing.values.contains_key(property)
                                    && Rc::ptr_eq(
                                        landing.owners.get(property).unwrap_or(&landing.owner),
                                        &self.owner,
                                    )
                            }))
                })
                .collect();
            engine.stop(Some(list));
        }
        if let Some(faded) = &self.faded {
            faded.stop();
        }
    }
}

/// A call's segment as a [`MotionCurve`] in seconds from its start.
struct SegmentCurve(Segment);

impl MotionCurve for SegmentCurve {
    fn duration(&self) -> f64 {
        self.0.duration / 1000.
    }
    fn at(&self, t: f64) -> f64 {
        if t * 1000. >= self.0.duration {
            self.0.target
        } else {
            (self.0.at)((t * 1000.).max(0.))
        }
    }
    fn velocity(&self, t: f64) -> f64 {
        if t * 1000. >= self.0.duration {
            0.
        } else {
            (self.0.speed)(t * 1000.)
        }
    }
}

/// One engine per key, created on first use (upstream `animatorFor`): code
/// that animates one element from several places (a swipe on x, a removal
/// on y) shares it, so their curves compose and retargeting one axis leaves
/// the other on its curve. `options` apply when the engine is created.
#[derive(Debug)]
pub struct EngineRegistry<K> {
    engines: HashMap<K, MotionEngine>,
}

impl<K> Default for EngineRegistry<K> {
    fn default() -> Self {
        Self {
            engines: HashMap::new(),
        }
    }
}

impl<K: Eq + Hash> EngineRegistry<K> {
    /// An empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// The one engine for `key`.
    pub fn animator_for(&mut self, key: K, options: EngineOptions) -> MotionEngine {
        self.engines
            .entry(key)
            .or_insert_with(|| MotionEngine::new(options))
            .clone()
    }

    /// Forgets the engine of `key` (its element left).
    pub fn remove(&mut self, key: &K) -> Option<MotionEngine> {
        self.engines.remove(key)
    }
}
