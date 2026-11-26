//! The rows' slides (`spec:AST-103` FR9): under shift feedback the neighbours a drag passes
//! slide aside to open the gap, and a drop, a cancel or a reorder-mode move lands every
//! row it moved from where it showed. Every glide plays the shared settle role
//! (`springs.settle`, upstream `motion/uikit/reorder.ts`), retargeting from where the row
//! shows at the speed it has. The offset is a translate applied at prepaint, so the
//! layout is never read per frame and hit testing moves with the drawing. Under Reduce
//! Motion nothing travels: with a coarse pointer a shifted row cross-fades into its new
//! place, and otherwise rows take it at once.

use std::cell::RefCell;
use std::collections::HashMap;
use std::panic::Location;
use std::rc::Rc;
use std::time::Duration;

use astryx_primitives::motion::uikit::springs::role::SETTLE;
use astryx_primitives::motion::uikit::timing::REDUCED_MOTION_CROSS_FADE_MS;
use astryx_primitives::motion::{FnCurve, SharedCurve, resolve_spring, solve_spring};
use gpui::{
    AnyElement, App, Bounds, Element, ElementId, GlobalElementId, InspectorElementId, IntoElement,
    LayoutId, Pixels, Point, Window, point, px,
};
use web_time::Instant;

use super::ItemKey;

/// Within this of its target a slide is at rest, px (upstream `SETTLE_THRESHOLD`).
const EPSILON: f64 = 0.05;
/// The touch cross-fade's cap (`design:reduced-motion` DR2).
const CROSS_FADE_CAP_MS: f64 = 200.;

/// One axis of a row's offset.
#[derive(Default)]
struct Axis {
    curve: Option<(SharedCurve, Instant)>,
    rest: f32,
}

impl Axis {
    fn value(&self, now: Instant) -> f32 {
        match &self.curve {
            Some((curve, start)) => {
                let t = now.saturating_duration_since(*start).as_secs_f64();
                if t >= curve.duration() {
                    self.rest
                } else {
                    curve.at(t) as f32
                }
            }
            None => self.rest,
        }
    }

    fn velocity(&self, now: Instant) -> f64 {
        self.curve.as_ref().map_or(0., |(curve, start)| {
            let t = now.saturating_duration_since(*start).as_secs_f64();
            if t >= curve.duration() {
                0.
            } else {
                curve.velocity(t)
            }
        })
    }

    fn is_moving(&self, now: Instant) -> bool {
        self.curve.as_ref().is_some_and(|(curve, start)| {
            now.saturating_duration_since(*start).as_secs_f64() < curve.duration()
        })
    }

    /// Springs to `to` from where it shows plus `moved` (the layout's own move), at
    /// the speed it has.
    fn spring_to(&mut self, to: f32, moved: f32, now: Instant) {
        let velocity = self.velocity(now);
        self.spring_with(to, moved, velocity, now);
    }

    /// [`Axis::spring_to`], starting at `velocity` (px/s).
    fn spring_with(&mut self, to: f32, moved: f32, velocity: f64, now: Instant) {
        let from = f64::from(self.value(now) + moved);
        let to64 = f64::from(to);
        self.rest = to;
        if (from - to64).abs() < EPSILON && velocity.abs() < EPSILON {
            self.curve = None;
            return;
        }
        let curve = solve_spring(resolve_spring(SETTLE, 1.).to_spring(), from, to64, velocity);
        let duration = curve.settle_time(EPSILON);
        let curve = FnCurve::new(
            duration,
            move |t| if t >= duration { to64 } else { curve.value(t) },
            move |t| if t >= duration { 0. } else { curve.velocity(t) },
        )
        .shared();
        self.curve = Some((curve, now));
    }

    fn jump(&mut self, to: f32) {
        self.rest = to;
        self.curve = None;
    }
}

/// A row's slide, kept by its key across frames and orders.
#[derive(Default)]
pub(crate) struct Slide {
    x: Axis,
    y: Axis,
    target: (f32, f32),
    /// Its layout origin last frame: a landing glides it from there.
    origin: Option<Point<Pixels>>,
    /// A dropped row's release velocity (px/s), which its landing starts from.
    release: Option<(f64, f64)>,
    /// Under Reduce Motion with a coarse pointer: when it cross-faded into its place.
    fade: Option<Instant>,
}

impl Slide {
    fn offset(&self, now: Instant) -> Point<Pixels> {
        point(px(self.x.value(now)), px(self.y.value(now)))
    }

    fn is_moving(&self, now: Instant) -> bool {
        self.x.is_moving(now) || self.y.is_moving(now)
    }
}

/// How rows move this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum SlideMotion {
    /// They glide on `springs.settle`.
    #[default]
    Glide,
    /// Reduce Motion under a coarse pointer: an in-place cross-fade.
    Fade,
    /// Reduce Motion: at once.
    Jump,
}

/// Every row's slide in one collection.
#[derive(Clone, Default)]
pub(crate) struct Slides {
    rows: Rc<RefCell<HashMap<ItemKey, Rc<RefCell<Slide>>>>>,
    /// A drop, a cancel or a move is landing: rows the layout moves glide from where
    /// they showed. Set until the collection's children are laid out.
    landing: Rc<std::cell::Cell<bool>>,
}

impl Slides {
    fn slide(&self, key: &ItemKey) -> Rc<RefCell<Slide>> {
        self.rows
            .borrow_mut()
            .entry(key.clone())
            .or_default()
            .clone()
    }

    /// The next frame lands a change: what it moves glides from where it showed.
    pub(crate) fn land(&self) {
        self.landing.set(true);
    }

    /// The collection's children are laid out: the landing is over.
    pub(crate) fn landed(&self) {
        self.landing.set(false);
    }

    /// The dragged row was shown at `origin` (window coordinates) when it was let go:
    /// it lands from there.
    /// `velocity` is the drag's release velocity (px/s), which the landing starts from.
    pub(crate) fn let_go_at(&self, key: &ItemKey, origin: Point<Pixels>, velocity: (f64, f64)) {
        let slide = self.slide(key);
        let mut s = slide.borrow_mut();
        s.x.jump(0.);
        s.y.jump(0.);
        s.target = (0., 0.);
        s.origin = Some(origin);
        s.release = Some(velocity);
    }

    /// Forgets rows no longer rendered.
    pub(crate) fn retain(&self, keys: &[ItemKey]) {
        self.rows.borrow_mut().retain(|k, _| keys.contains(k));
    }

    /// The row `key`, offset toward `target` (its shift while a drag passes it, else
    /// none), moving as `motion` says.
    pub(crate) fn place(
        &self,
        key: &ItemKey,
        target: (f32, f32),
        motion: SlideMotion,
        element: AnyElement,
        cx: &App,
    ) -> AnyElement {
        let now = cx.background_executor().now();
        let slide = self.slide(key);
        let fade_opacity = {
            let mut s = slide.borrow_mut();
            if s.target != target {
                s.target = target;
                match motion {
                    SlideMotion::Glide => {
                        s.x.spring_to(target.0, 0., now);
                        s.y.spring_to(target.1, 0., now);
                    }
                    SlideMotion::Fade => {
                        s.x.jump(target.0);
                        s.y.jump(target.1);
                        s.fade = Some(now);
                    }
                    SlideMotion::Jump => {
                        s.x.jump(target.0);
                        s.y.jump(target.1);
                    }
                }
            }
            let fade = Duration::from_secs_f64(
                REDUCED_MOTION_CROSS_FADE_MS.min(CROSS_FADE_CAP_MS) / 1000.,
            );
            match s.fade {
                Some(since) => {
                    let p = now.saturating_duration_since(since).as_secs_f32() / fade.as_secs_f32();
                    if p >= 1. {
                        s.fade = None;
                        None
                    } else {
                        Some(p)
                    }
                }
                None => None,
            }
        };
        let element = match fade_opacity {
            Some(opacity) => {
                use gpui::{ParentElement as _, Styled as _};
                gpui::div()
                    .opacity(opacity)
                    .child(element)
                    .into_any_element()
            }
            None => element,
        };
        SlideElement {
            slide,
            child: element,
            landing: self.landing.get() && motion == SlideMotion::Glide,
            fading: fade_opacity.is_some(),
        }
        .into_any_element()
    }
}

/// A row drawn at its slide's offset, measured where it is laid out.
struct SlideElement {
    slide: Rc<RefCell<Slide>>,
    child: AnyElement,
    landing: bool,
    fading: bool,
}

impl Element for SlideElement {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        (self.child.request_layout(window, cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        let now = cx.background_executor().now();
        let offset = {
            let mut s = self.slide.borrow_mut();
            // A landing: the row glides from where it showed into its new place.
            if self.landing
                && let Some(last) = s.origin
                && last != bounds.origin
            {
                let moved = last - bounds.origin;
                let target = s.target;
                match s.release.take() {
                    Some((vx, vy)) => {
                        s.x.spring_with(target.0, f32::from(moved.x), vx, now);
                        s.y.spring_with(target.1, f32::from(moved.y), vy, now);
                    }
                    None => {
                        s.x.spring_to(target.0, f32::from(moved.x), now);
                        s.y.spring_to(target.1, f32::from(moved.y), now);
                    }
                }
            }
            if !self.landing {
                s.release = None;
            }
            s.origin = Some(bounds.origin);
            if s.is_moving(now) || self.fading {
                window.request_animation_frame();
            }
            s.offset(now)
        };
        window.with_element_offset(offset, |window| self.child.prepaint(window, cx));
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        self.child.paint(window, cx);
    }
}
