//! `UIScreenEdgePanGestureRecognizer`, upstream
//! `gestures/ScreenEdgePanGestureRecognizer.ts`: a pan that only starts from
//! a band at the edge of the viewport.
//!
//! A touch that lands outside every requested band fails at once, so a
//! recognizer that requires this one to fail is not held up by a touch in
//! the middle of the screen. Inside a band the pan begins on movement away
//! from that edge. `Leading` and `Trailing` resolve against the view's
//! writing direction. On a view that lies within its edge band (the edge
//! zone) the pan takes the horizontal axis and decides at UIKit's 15 px; on
//! any other view it leaves the window every touch behavior and decides at
//! FR19's 10 px (`spec:AST-094` FR18, DEC-22). A finger's only: no mouse,
//! trackpad or pen drives it (FR43).

use std::cell::RefCell;
use std::rc::Rc;

use super::pan::{Hysteresis, Pan, PanDirection, PanLike, PanOptions};
use super::recognizer::{
    ALL_NATIVE_TOUCH, AnyRecognizer, NativeTouchBehavior, PanVerdict, Recognizer,
    RecognizerBehavior, RecognizerFamily, RecognizerKind, RecognizerOptions,
};
use super::types::{GestureEventContext, GestureState, GestureTouch, Point, PointerKind};
use crate::motion::uikit::gestures::{pan as PAN, screen_edge_pan as EDGE};

/// How far past `edge_region` a view still counts as the edge zone, px.
const EDGE_ZONE_TOLERANCE: f64 = 0.5;

/// An edge by writing direction: under RTL the leading edge is the right one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScreenEdge {
    Leading,
    Trailing,
}

/// A physical horizontal edge of the viewport.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PhysicalEdge {
    Left,
    Right,
}

#[derive(Clone, Debug, PartialEq)]
struct EdgeLayout {
    is_rtl: bool,
    watched: Vec<PhysicalEdge>,
    zone: Vec<PhysicalEdge>,
}

/// Upstream `ScreenEdgePanGestureRecognizerOptions`: a pan's options
/// without a direction, pointer types, scroll types or a per-press
/// hysteresis.
#[derive(Clone)]
pub struct ScreenEdgePanOptions {
    pub base: RecognizerOptions,
    /// @default [Leading]
    pub edges: Vec<ScreenEdge>,
    /// Width of the band at each edge, px.
    pub edge_region: f64,
    /// The travel that begins the pan in its edge zone. @default UIKit's 15
    pub hysteresis: f64,
    /// @default 1
    pub minimum_number_of_touches: usize,
    /// @default 1
    pub maximum_number_of_touches: usize,
    pub axis_ratio: f64,
    pub lock_axis: bool,
    pub scroll_edge: bool,
    pub first_move_decides: bool,
}

impl Default for ScreenEdgePanOptions {
    fn default() -> Self {
        Self {
            base: RecognizerOptions::default(),
            edges: vec![ScreenEdge::Leading],
            edge_region: EDGE::EDGE_REGION,
            hysteresis: EDGE::HYSTERESIS,
            minimum_number_of_touches: 1,
            maximum_number_of_touches: 1,
            axis_ratio: 1.,
            lock_axis: false,
            scroll_edge: false,
            first_move_decides: false,
        }
    }
}

impl ScreenEdgePanOptions {
    /// Default options with a name.
    pub fn named(name: impl Into<String>) -> Self {
        Self {
            base: RecognizerOptions::named(name),
            ..Self::default()
        }
    }
}

/// The screen-edge pan's behavior: a pan with edges.
pub struct ScreenEdgePan {
    pub pan: Pan,
    pub edges: Vec<ScreenEdge>,
    pub edge_region: f64,
    /// The physical edge the current gesture started from.
    pub active_edge: Option<PhysicalEdge>,
    /// Set by the host while attached: called when the edge-zone answer
    /// changes, so the view's touch behavior follows it (FR19, DEC-22).
    pub on_touch_behavior_change: Option<Rc<dyn Fn()>>,
    edge_hysteresis: f64,
    layout: RefCell<Option<EdgeLayout>>,
}

/// `ScreenEdgePanGestureRecognizer`.
pub type ScreenEdgePanGestureRecognizer = Recognizer<ScreenEdgePan>;

impl Recognizer<ScreenEdgePan> {
    /// A screen-edge pan recognizer.
    pub fn new(options: ScreenEdgePanOptions) -> Self {
        let mut base = options.base;
        // A finger's only (FR43), whatever a caller passes.
        base.allowed_pointer_types = Some(vec![PointerKind::Touch]);
        base.allowed_scroll_types = Some(Vec::new());
        let pan = Pan::new(&PanOptions {
            base: RecognizerOptions::default(),
            minimum_number_of_touches: options.minimum_number_of_touches,
            maximum_number_of_touches: options.maximum_number_of_touches,
            hysteresis: Hysteresis::Fixed(options.hysteresis),
            direction: PanDirection::Any,
            axis_ratio: options.axis_ratio,
            lock_axis: options.lock_axis,
            scroll_edge: options.scroll_edge,
            first_move_decides: options.first_move_decides,
        });
        let behavior = ScreenEdgePan {
            pan,
            edges: options.edges,
            edge_region: options.edge_region,
            active_edge: None,
            on_touch_behavior_change: None,
            edge_hysteresis: options.hysteresis,
            layout: RefCell::new(None),
        };
        Self::with_behavior(base, behavior, "ScreenEdgePanGestureRecognizer")
    }

    /// The view's box may have changed: its edges' sides and the edge zone
    /// are measured again, and a changed answer calls the host's hook.
    pub fn layout_changed(&self) {
        let hook = {
            let edge = self.behavior();
            edge.layout_changed(self)
                .then(|| edge.on_touch_behavior_change.clone())
                .flatten()
        };
        if let Some(hook) = hook {
            hook();
        }
    }

    /// Whether the edge zone depends on the viewport's width (the pan
    /// watches the right side).
    pub fn watches_viewport(&self) -> bool {
        self.behavior()
            .measured(self)
            .watched
            .contains(&PhysicalEdge::Right)
    }

    /// The physical edge the current gesture started from.
    pub fn active_edge(&self) -> Option<PhysicalEdge> {
        self.behavior().active_edge
    }

    /// Sets the host's hook for a changed edge-zone answer.
    pub fn set_on_touch_behavior_change(&self, hook: Option<Rc<dyn Fn()>>) {
        self.behavior_mut().on_touch_behavior_change = hook;
    }
}

impl ScreenEdgePan {
    /// Measures the layout again; returns whether the answer changed.
    fn layout_changed(&self, r: &AnyRecognizer) -> bool {
        let before = self.layout.borrow_mut().take();
        let after = self.measured(r);
        before.is_none_or(|before| {
            before.watched != after.watched
                || before.zone.len() != after.zone.len()
                || after.zone.iter().any(|edge| !before.zone.contains(edge))
        })
    }

    fn measured(&self, r: &AnyRecognizer) -> EdgeLayout {
        if let Some(layout) = self.layout.borrow().as_ref() {
            return layout.clone();
        }
        let environment = r.environment();
        let view = r.view();
        let is_rtl = environment.is_rtl(view);
        let watched: Vec<PhysicalEdge> = self
            .edges
            .iter()
            .map(|edge| {
                if (*edge == ScreenEdge::Leading) != is_rtl {
                    PhysicalEdge::Left
                } else {
                    PhysicalEdge::Right
                }
            })
            .collect();
        let mut zone = Vec::new();
        if let Some(rect) = environment.view_rect(view)
            && rect.width > 0.
        {
            let (width, _) = environment.viewport_size();
            let right = rect.left + rect.width;
            let band = self.edge_region + EDGE_ZONE_TOLERANCE;
            if rect.left >= -EDGE_ZONE_TOLERANCE && right <= band {
                zone.push(PhysicalEdge::Left);
            }
            if rect.left >= width - band && right <= width + EDGE_ZONE_TOLERANCE {
                zone.push(PhysicalEdge::Right);
            }
        }
        let layout = EdgeLayout {
            is_rtl,
            watched,
            zone,
        };
        *self.layout.borrow_mut() = Some(layout.clone());
        layout
    }

    fn edge_at(&self, r: &AnyRecognizer, touch: &GestureTouch) -> Option<PhysicalEdge> {
        let (width, _) = r.environment().viewport_size();
        self.measured(r)
            .watched
            .into_iter()
            .find(|physical| match physical {
                PhysicalEdge::Left => touch.start_x() <= self.edge_region,
                PhysicalEdge::Right => touch.start_x() >= width - self.edge_region,
            })
    }
}

impl PanLike for ScreenEdgePan {
    fn pan(&self) -> &Pan {
        &self.pan
    }
    fn pan_mut(&mut self) -> &mut Pan {
        &mut self.pan
    }
}

impl RecognizerBehavior for ScreenEdgePan {
    fn kind(&self) -> RecognizerKind {
        RecognizerKind::ScreenEdgePan
    }
    fn family(&self) -> Option<RecognizerFamily> {
        Some(RecognizerFamily::Pan)
    }

    /// The pan takes the horizontal axis only in its edge zone.
    fn native_touch_behavior(&self, r: &AnyRecognizer) -> NativeTouchBehavior {
        let layout = self.measured(r);
        if !layout.watched.iter().any(|edge| layout.zone.contains(edge)) {
            return ALL_NATIVE_TOUCH;
        }
        NativeTouchBehavior {
            pan_x: false,
            pan_y: true,
            pinch_zoom: true,
            double_tap_zoom: false,
        }
    }

    fn accepts_touch(&self, r: &AnyRecognizer, touch: &GestureTouch) -> bool {
        self.pan.accepts(r, touch)
    }

    fn touches_began(
        &mut self,
        r: &AnyRecognizer,
        touches: &[GestureTouch],
        event: &GestureEventContext,
    ) {
        if r.number_of_touches() == touches.len() {
            // The touch behavior in force as it landed: whether it claimed
            // the axis.
            let in_force = self.measured(r);
            let claimed_axis = in_force
                .watched
                .iter()
                .any(|side| in_force.zone.contains(side));
            // A direction switched in place moves the leading edge: measure
            // again, and announce it (DR13).
            if in_force.is_rtl != r.environment().is_rtl(r.view())
                && self.layout_changed(r)
                && let Some(hook) = self.on_touch_behavior_change.clone()
            {
                hook();
            }
            let Some(edge) = touches.first().and_then(|first| self.edge_at(r, first)) else {
                r.set_state(GestureState::Failed);
                return;
            };
            self.active_edge = Some(edge);
            let off_zone = !claimed_axis || !self.measured(r).zone.contains(&edge);
            self.pan.hysteresis = if off_zone {
                self.edge_hysteresis.min(PAN::HYSTERESIS)
            } else {
                self.edge_hysteresis
            };
            self.pan.direction = if edge == PhysicalEdge::Left {
                PanDirection::Right
            } else {
                PanDirection::Left
            };
        }
        self.pan.began(r, touches, event);
    }

    fn touches_moved(
        &mut self,
        r: &AnyRecognizer,
        _touches: &[GestureTouch],
        event: &GestureEventContext,
    ) {
        self.pan.moved(r, event);
    }

    fn touches_ended(
        &mut self,
        r: &AnyRecognizer,
        touches: &[GestureTouch],
        event: &GestureEventContext,
    ) {
        self.pan.ended(r, touches, event);
    }

    fn reset(&mut self) {
        self.pan.reset_pan();
        self.active_edge = None;
    }

    fn keeps_touch(&self, r: &AnyRecognizer, touch_id: u64) -> bool {
        self.pan.keeps(r, touch_id)
    }

    fn classify(&self, translation: Point) -> Option<PanVerdict> {
        Some(self.pan.classify_translation(translation))
    }
}
