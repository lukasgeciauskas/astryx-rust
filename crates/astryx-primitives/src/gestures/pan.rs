//! `UIPanGestureRecognizer`, upstream `gestures/PanGestureRecognizer.ts`.
//!
//! Begins once the touches' centroid has travelled `hysteresis` px along
//! either axis (per axis, not as a distance) with between
//! `minimum_number_of_touches` and `maximum_number_of_touches` fingers
//! down, in the allowed `direction`. As in UIKit, `translation()` is reset
//! when the pan begins, so the travel spent on the threshold is not part of
//! it; `total_translation()` is the movement since the fingers landed.
//! `velocity()` is UIKit's own filter, fed one sample per event; the lift
//! updates neither. Adding or lifting a finger never jumps the translation.
//! Once moving, fingers past the maximum are ignored.
//!
//! Native scroll coordination, for a pan over content that scrolls (a
//! sheet over its own list): `scroll_edge` begins only if no scroller under
//! the touch could scroll for that movement; `first_move_decides` holds
//! the touch from a first move its own way and fails on any other. Who gets
//! the drag is decided once, when it begins (`spec:AST-094` FR14).

use std::rc::Rc;

use super::geometry::{TAP_WOBBLE, centroid, sign};
use super::recognizer::{
    ALL_NATIVE_TOUCH, AnyRecognizer, NativeTouchBehavior, PanVerdict, Recognizer,
    RecognizerBehavior, RecognizerFamily, RecognizerKind, RecognizerOptions,
};
use super::types::{GestureEventContext, GestureState, GestureTouch, Point, PointerKind};
use super::velocity_tracker::VelocityTracker;
use crate::motion::uikit::gestures::pan as PAN;

/// Movement that may begin a pan.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PanDirection {
    Any,
    Horizontal,
    Vertical,
    Left,
    Right,
    Up,
    Down,
}

impl PanDirection {
    fn vector(self) -> Option<Point> {
        match self {
            Self::Left => Some(Point::new(-1., 0.)),
            Self::Right => Some(Point::new(1., 0.)),
            Self::Up => Some(Point::new(0., -1.)),
            Self::Down => Some(Point::new(0., 1.)),
            _ => None,
        }
    }

    /// Whether the direction lies along the horizontal axis.
    pub fn is_horizontal(self) -> bool {
        matches!(self, Self::Horizontal | Self::Left | Self::Right)
    }

    /// Whether the direction lies along the vertical axis.
    pub fn is_vertical(self) -> bool {
        matches!(self, Self::Vertical | Self::Up | Self::Down)
    }
}

/// A threshold read from a press's first touch.
pub type PerPress = Rc<dyn Fn(&GestureTouch) -> f64>;

/// The travel that begins a pan: one number, or one per press read from its
/// first touch.
#[derive(Clone)]
pub enum Hysteresis {
    Fixed(f64),
    PerPress(PerPress),
}

/// Upstream `PanGestureRecognizerOptions`.
#[derive(Clone)]
pub struct PanOptions {
    pub base: RecognizerOptions,
    /// @default 1
    pub minimum_number_of_touches: usize,
    /// @default unbounded
    pub maximum_number_of_touches: usize,
    /// @default UIKit's 10
    pub hysteresis: Hysteresis,
    /// @default Any
    pub direction: PanDirection,
    /// For a pan limited to an axis: how much the movement along it must
    /// exceed the movement across it. @default 1
    pub axis_ratio: f64,
    /// With `Any`, report only the axis that dominated at the start.
    pub lock_axis: bool,
    /// Begin only when the native scrollers under the touch are at the edge.
    pub scroll_edge: bool,
    /// A one-way pan decides on the touch's first move.
    pub first_move_decides: bool,
}

impl Default for PanOptions {
    fn default() -> Self {
        Self {
            base: RecognizerOptions::default(),
            minimum_number_of_touches: 1,
            maximum_number_of_touches: usize::MAX,
            hysteresis: Hysteresis::Fixed(PAN::HYSTERESIS),
            direction: PanDirection::Any,
            axis_ratio: 1.,
            lock_axis: false,
            scroll_edge: false,
            first_move_decides: false,
        }
    }
}

impl PanOptions {
    /// Default options with a name.
    pub fn named(name: impl Into<String>) -> Self {
        Self {
            base: RecognizerOptions::named(name),
            ..Self::default()
        }
    }
}

/// A locked axis.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    X,
    Y,
}

/// The pan's behavior.
pub struct Pan {
    pub minimum_number_of_touches: usize,
    pub maximum_number_of_touches: usize,
    pub hysteresis: f64,
    pub direction: PanDirection,
    pub axis_ratio: f64,
    pub lock_axis: bool,
    pub scroll_edge: bool,
    pub first_move_decides: bool,
    /// The axis a `lock_axis` pan locked to when it began.
    pub locked_axis: Option<Axis>,
    /// The movement the begin threshold took.
    pub travel_at_began: Point,
    tracker: VelocityTracker,
    hysteresis_for: Option<PerPress>,
    release_time: Option<f64>,
    origin: Point,
    adjust: Point,
    raw: Point,
    offset: Point,
    first_move: Option<PanVerdict>,
}

/// `PanGestureRecognizer`.
pub type PanGestureRecognizer = Recognizer<Pan>;

/// A behavior built on the pan (the pan itself, the screen-edge pan).
pub trait PanLike: RecognizerBehavior {
    fn pan(&self) -> &Pan;
    fn pan_mut(&mut self) -> &mut Pan;
}

impl PanLike for Pan {
    fn pan(&self) -> &Pan {
        self
    }
    fn pan_mut(&mut self) -> &mut Pan {
        self
    }
}

impl Pan {
    pub(crate) fn new(options: &PanOptions) -> Self {
        let (hysteresis, hysteresis_for) = match &options.hysteresis {
            Hysteresis::Fixed(value) => (*value, None),
            Hysteresis::PerPress(f) => (PAN::HYSTERESIS, Some(f.clone())),
        };
        Self {
            minimum_number_of_touches: options.minimum_number_of_touches,
            maximum_number_of_touches: options.maximum_number_of_touches,
            hysteresis,
            direction: options.direction,
            axis_ratio: options.axis_ratio,
            lock_axis: options.lock_axis,
            scroll_edge: options.scroll_edge,
            first_move_decides: options.first_move_decides,
            locked_axis: None,
            travel_at_began: Point::ZERO,
            tracker: VelocityTracker::default(),
            hysteresis_for,
            release_time: None,
            origin: Point::ZERO,
            adjust: Point::ZERO,
            raw: Point::ZERO,
            offset: Point::ZERO,
            first_move: None,
        }
    }

    /// Movement since the pan began (or since `set_translation`); the
    /// movement since touch-down while it is still possible.
    pub fn translation(&self) -> Point {
        self.locked(Point::new(
            self.raw.x + self.offset.x,
            self.raw.y + self.offset.y,
        ))
    }

    /// UIKit's `setTranslation(_:in:)`.
    pub fn set_translation(&mut self, value: Point) {
        self.offset = Point::new(value.x - self.raw.x, value.y - self.raw.y);
    }

    /// Movement since the fingers landed, threshold included.
    pub fn total_translation(&self) -> Point {
        self.raw
    }

    /// Velocity in px/s for a dragged surface; at the release, 0 after a
    /// rest past the stillness interval or with fewer than two moves.
    pub fn velocity(&self) -> Point {
        self.locked(self.tracker.velocity(self.release_time))
    }

    /// A scrolling surface's release velocity, px/s (INV10).
    pub fn scroll_velocity(&self) -> Point {
        let time = self
            .release_time
            .or_else(|| self.tracker.last_time())
            .unwrap_or(0.);
        self.locked(self.tracker.scroll_velocity(time))
    }

    fn locked(&self, value: Point) -> Point {
        match self.locked_axis {
            Some(Axis::X) => Point::new(value.x, 0.),
            Some(Axis::Y) => Point::new(0., value.y),
            None => value,
        }
    }

    /// What this movement means for a pan limited to a direction.
    pub fn classify_translation(&self, translation: Point) -> PanVerdict {
        if self.direction == PanDirection::Any {
            return PanVerdict::Begin;
        }
        let horizontal = self.direction.is_horizontal();
        let along = if horizontal {
            translation.x
        } else {
            translation.y
        };
        let across = if horizontal {
            translation.y
        } else {
            translation.x
        }
        .abs();
        let magnitude = along.abs();
        if magnitude > self.axis_ratio * across {
            let sign = self.direction.vector().map_or(0., |v| v.x + v.y);
            return if sign == 0. || self::sign(along) == sign {
                PanVerdict::Begin
            } else {
                PanVerdict::Fail
            };
        }
        if across * self.axis_ratio.min(1.) > magnitude {
            PanVerdict::Fail
        } else {
            PanVerdict::Wait
        }
    }

    pub(crate) fn native_behavior(&self) -> NativeTouchBehavior {
        if self.scroll_edge || self.direction.vector().is_some() {
            return ALL_NATIVE_TOUCH;
        }
        NativeTouchBehavior {
            pan_x: self.direction.is_vertical(),
            pan_y: self.direction.is_horizontal(),
            pinch_zoom: true,
            double_tap_zoom: false,
        }
    }

    pub(crate) fn accepts(&self, r: &AnyRecognizer, touch: &GestureTouch) -> bool {
        if r.state().is_active() && r.number_of_touches() >= self.maximum_number_of_touches {
            // A moving pan keeps its fingers: an extra one never fails it.
            return false;
        }
        r.base_accepts_touch(touch)
    }

    pub(crate) fn began(
        &mut self,
        r: &AnyRecognizer,
        touches: &[GestureTouch],
        event: &GestureEventContext,
    ) {
        if r.number_of_touches() == touches.len() {
            if let (Some(f), Some(first)) = (&self.hysteresis_for, touches.first()) {
                self.hysteresis = f(first);
            }
            self.origin = centroid(&r.tracked_touches(), &[]);
            self.adjust = Point::ZERO;
            self.raw = Point::ZERO;
            self.offset = Point::ZERO;
            self.tracker.reset();
            self.tracker.add(self.origin.x, self.origin.y, event.time);
        } else {
            let ids: Vec<u64> = touches.iter().map(GestureTouch::id).collect();
            self.rebase(r, &ids, true);
        }
        if r.state() == GestureState::Possible
            && r.number_of_touches() > self.maximum_number_of_touches
        {
            r.set_state(GestureState::Failed);
        }
    }

    pub(crate) fn moved(&mut self, r: &AnyRecognizer, event: &GestureEventContext) {
        self.track(r, event.time);
        let state = r.state();
        if state == GestureState::Possible {
            if self.first_move_decides
                && self.first_move.is_none()
                && (self.raw.x != 0. || self.raw.y != 0.)
            {
                let verdict = self.classify_translation(self.raw);
                self.first_move = Some(verdict);
                if verdict != PanVerdict::Begin {
                    r.set_state(GestureState::Failed);
                    return;
                }
            }
            self.try_begin(r);
        } else if state.is_active() {
            r.set_state(GestureState::Changed);
        }
    }

    pub(crate) fn ended(
        &mut self,
        r: &AnyRecognizer,
        touches: &[GestureTouch],
        event: &GestureEventContext,
    ) {
        let remaining = r.number_of_touches() - touches.len();
        let ids: Vec<u64> = touches.iter().map(GestureTouch::id).collect();
        if r.state().is_active() {
            if remaining < self.minimum_number_of_touches {
                self.release_time = Some(event.time);
                r.set_state(GestureState::Ended);
                return;
            }
            self.rebase(r, &ids, false);
            return;
        }
        if remaining == 0 || remaining < self.minimum_number_of_touches {
            r.set_state(GestureState::Failed);
            return;
        }
        self.rebase(r, &ids, false);
    }

    pub(crate) fn reset_pan(&mut self) {
        self.locked_axis = None;
        self.travel_at_began = Point::ZERO;
        self.origin = Point::ZERO;
        self.adjust = Point::ZERO;
        self.raw = Point::ZERO;
        self.offset = Point::ZERO;
        self.release_time = None;
        self.first_move = None;
        self.tracker.reset();
    }

    fn try_begin(&mut self, r: &AnyRecognizer) {
        let count = r.number_of_touches();
        if count < self.minimum_number_of_touches {
            return;
        }
        if count > self.maximum_number_of_touches {
            r.set_state(GestureState::Failed);
            return;
        }
        let moved = self.raw;
        if moved.x.abs().max(moved.y.abs()) < self.hysteresis {
            return;
        }
        match self.classify_translation(moved) {
            // Past its begin with no clear axis, a pan that competes with
            // native scrolling leaves the touch to the window (FR19).
            PanVerdict::Fail | PanVerdict::Wait => {
                r.set_state(GestureState::Failed);
                return;
            }
            PanVerdict::Begin => {}
        }
        if self.scroll_edge && native_can_scroll(r, moved.x, moved.y) {
            // The content under the finger still scrolls this way (FR14).
            r.set_state(GestureState::Failed);
            return;
        }
        if self.lock_axis && self.direction == PanDirection::Any {
            self.locked_axis = Some(if moved.x.abs() >= moved.y.abs() {
                Axis::X
            } else {
                Axis::Y
            });
        }
        self.start_from_here();
        r.set_state(GestureState::Began);
    }

    /// Leaves the threshold's travel out of the translation, as UIKit does
    /// at began: up to `hysteresis` px per axis.
    fn start_from_here(&mut self) {
        let limit = self.hysteresis;
        let swallowed = Point::new(
            sign(self.raw.x) * self.raw.x.abs().min(limit),
            sign(self.raw.y) * self.raw.y.abs().min(limit),
        );
        self.travel_at_began = swallowed;
        self.offset = Point::new(-swallowed.x, -swallowed.y);
    }

    pub(crate) fn keeps(&self, r: &AnyRecognizer, touch_id: u64) -> bool {
        if !self.scroll_edge && !self.first_move_decides {
            return false;
        }
        let Some(touch) = r.tracked_touch(touch_id) else {
            return false;
        };
        if r.state() != GestureState::Possible
            // A lone finger only: two fingers before a claim are the window's
            // pinch (FR10).
            || r.number_of_touches() != 1
            || touch.pointer_type() != PointerKind::Touch
            || touch.taken_by_browser()
        {
            return false;
        }
        if self.first_move_decides {
            return self.first_move == Some(PanVerdict::Begin);
        }
        let moved = self.raw;
        self.travel_along_axis(moved) >= TAP_WOBBLE
            && self.classify_translation(moved) == PanVerdict::Begin
            && !native_can_scroll(r, moved.x, moved.y)
    }

    fn travel_along_axis(&self, moved: Point) -> f64 {
        if self.direction == PanDirection::Any {
            return moved.x.abs().max(moved.y.abs());
        }
        if self.direction.is_horizontal() {
            moved.x.abs()
        } else {
            moved.y.abs()
        }
    }

    fn track(&mut self, r: &AnyRecognizer, time: f64) {
        let current = centroid(&r.tracked_touches(), &[]);
        let virtual_point = Point::new(current.x + self.adjust.x, current.y + self.adjust.y);
        // UIKit's filter runs once per event.
        self.tracker.add(virtual_point.x, virtual_point.y, time);
        self.raw = Point::new(
            virtual_point.x - self.origin.x,
            virtual_point.y - self.origin.y,
        );
    }

    /// Keeps the translation continuous when fingers join or leave.
    fn rebase(&mut self, r: &AnyRecognizer, changed: &[u64], added: bool) {
        let touches = r.tracked_touches();
        let all = centroid(&touches, &[]);
        let others = centroid(&touches, changed);
        let (before, after) = if added { (others, all) } else { (all, others) };
        self.adjust = Point::new(
            self.adjust.x + before.x - after.x,
            self.adjust.y + before.y - after.y,
        );
    }
}

pub(crate) fn native_can_scroll(r: &AnyRecognizer, dx: f64, dy: f64) -> bool {
    r.tracked_touches()
        .iter()
        .any(|touch| touch.scroll().is_some_and(|probe| probe.can_scroll(dx, dy)))
}

impl Recognizer<Pan> {
    /// A pan recognizer.
    pub fn new(options: PanOptions) -> Self {
        let behavior = Pan::new(&options);
        Self::with_behavior(options.base, behavior, "PanGestureRecognizer")
    }
}

impl<B: PanLike> Recognizer<B> {
    /// Movement since the pan began (or since `set_translation`), px.
    pub fn translation(&self) -> Point {
        self.behavior().pan().translation()
    }

    /// Movement since the fingers landed, threshold included, px.
    pub fn total_translation(&self) -> Point {
        self.behavior().pan().total_translation()
    }

    /// UIKit's `setTranslation(_:in:)`.
    pub fn set_translation(&self, value: Point) {
        self.behavior_mut().pan_mut().set_translation(value);
    }

    /// Velocity in px/s, UIKit's way, for a dragged surface.
    pub fn velocity(&self) -> Point {
        self.behavior().pan().velocity()
    }

    /// A scrolling surface's release velocity in px/s.
    pub fn scroll_velocity(&self) -> Point {
        self.behavior().pan().scroll_velocity()
    }

    /// Changes the allowed direction for the next gesture.
    pub fn set_direction(&self, direction: PanDirection) {
        self.behavior_mut().pan_mut().direction = direction;
    }

    /// The direction.
    pub fn direction(&self) -> PanDirection {
        self.behavior().pan().direction
    }

    /// The axis a `lock_axis` pan locked to.
    pub fn locked_axis(&self) -> Option<Axis> {
        self.behavior().pan().locked_axis
    }

    /// The movement the begin threshold took.
    pub fn travel_at_began(&self) -> Point {
        self.behavior().pan().travel_at_began
    }

    /// The begin threshold, px.
    pub fn hysteresis(&self) -> f64 {
        self.behavior().pan().hysteresis
    }
}

impl RecognizerBehavior for Pan {
    fn kind(&self) -> RecognizerKind {
        RecognizerKind::Pan
    }
    fn family(&self) -> Option<RecognizerFamily> {
        Some(RecognizerFamily::Pan)
    }
    fn native_touch_behavior(&self, _r: &AnyRecognizer) -> NativeTouchBehavior {
        self.native_behavior()
    }
    fn accepts_touch(&self, r: &AnyRecognizer, touch: &GestureTouch) -> bool {
        self.accepts(r, touch)
    }
    fn touches_began(
        &mut self,
        r: &AnyRecognizer,
        touches: &[GestureTouch],
        event: &GestureEventContext,
    ) {
        self.began(r, touches, event);
    }
    fn touches_moved(
        &mut self,
        r: &AnyRecognizer,
        _touches: &[GestureTouch],
        event: &GestureEventContext,
    ) {
        self.moved(r, event);
    }
    fn touches_ended(
        &mut self,
        r: &AnyRecognizer,
        touches: &[GestureTouch],
        event: &GestureEventContext,
    ) {
        self.ended(r, touches, event);
    }
    fn reset(&mut self) {
        self.reset_pan();
    }
    fn keeps_touch(&self, r: &AnyRecognizer, touch_id: u64) -> bool {
        self.keeps(r, touch_id)
    }
    fn classify(&self, translation: Point) -> Option<PanVerdict> {
        Some(self.classify_translation(translation))
    }
}
