//! A step's target (upstream `targetRef`): GPUI elements have no refs, so the element a
//! step points at is wrapped, layout-transparently, and its box is kept for the window
//! each frame it is drawn.

use std::collections::HashMap;
use std::panic::Location;

use gpui::{
    AnyElement, App, Bounds, Element, ElementId, FocusHandle, Global, GlobalElementId,
    InspectorElementId, IntoElement, LayoutId, Pixels, SharedString, Window, WindowId, px,
};

/// Where a target was drawn last, and what the tour needs from it.
#[derive(Clone, Debug)]
pub(crate) struct TargetFrame {
    /// Its box in window coordinates, as laid out (a press scale does not move it).
    pub bounds: Bounds<Pixels>,
    /// The corner radius the highlight ring follows.
    pub radius: Pixels,
    /// Where focus returns to it, when it is in the tab order.
    pub focus: Option<ReturnTarget>,
}

/// Where focus returns when a tour ends: a focus handle, or an Astryx control by id
/// (which focuses itself the next time it renders).
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ReturnTarget {
    Handle(FocusHandle),
    Control(ElementId),
}

impl ReturnTarget {
    pub(crate) fn focus(&self, window: &mut Window, cx: &mut App) {
        match self {
            ReturnTarget::Handle(handle) => handle.focus(window, cx),
            ReturnTarget::Control(id) => {
                rustryx_primitives::request_focus(id, cx);
                window.refresh();
            }
        }
    }
}

#[derive(Default)]
struct Targets(HashMap<(WindowId, SharedString), TargetFrame>);

impl Global for Targets {}

/// The element a tour step points at (upstream `TourStep` `targetRef`): any element,
/// such as a button, a card, a table cell or a chart region, named by an id and wrapped
/// with [`TourTarget::wrap`]. The target keeps its own behavior and styles.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TourTarget(SharedString);

impl TourTarget {
    /// A target named `id`, unique in the window.
    pub fn new(id: impl Into<SharedString>) -> Self {
        TourTarget(id.into())
    }

    /// The target's id.
    pub fn id(&self) -> &SharedString {
        &self.0
    }

    /// Wraps the element this target names: it lays out as if unwrapped, and its box is
    /// recorded each frame it is drawn.
    pub fn wrap(&self, element: impl IntoElement) -> TourTargetElement {
        TourTargetElement {
            target: self.clone(),
            child: element.into_any_element(),
            radius: px(0.),
            focus: None,
            control: None,
        }
    }

    /// Where the target was drawn last in `window`, if it ever was.
    pub(crate) fn frame(&self, window: &Window, cx: &App) -> Option<TargetFrame> {
        let key = (window.window_handle().window_id(), self.0.clone());
        cx.try_global::<Targets>()
            .and_then(|t| t.0.get(&key).cloned())
    }
}

/// The element [`TourTarget::wrap`] returns.
pub struct TourTargetElement {
    target: TourTarget,
    child: AnyElement,
    radius: Pixels,
    focus: Option<FocusHandle>,
    control: Option<ElementId>,
}

impl TourTargetElement {
    /// The target's corner radius, which the highlight ring follows (upstream reads the
    /// target's computed `border-radius`; GPUI cannot read an element's style).
    pub fn radius(mut self, radius: impl Into<Pixels>) -> Self {
        self.radius = radius.into();
        self
    }

    /// The target's focus handle when it is in the tab order (upstream's `tabIndex >=
    /// 0`): focus returns to it when the tour ends from its step. For an element whose
    /// focus the caller tracks.
    pub fn focus(mut self, focus: &FocusHandle) -> Self {
        self.focus = Some(focus.clone());
        self
    }

    /// The target is the Astryx control with this `id` (a Button, say), whose tab stop
    /// focus returns to when the tour ends from its step.
    pub fn control(mut self, id: impl Into<ElementId>) -> Self {
        self.control = Some(id.into());
        self
    }
}

impl Element for TourTargetElement {
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
        // Transparent: the child's node stands in the parent's layout.
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
        let key = (window.window_handle().window_id(), self.target.0.clone());
        cx.default_global::<Targets>().0.insert(
            key,
            TargetFrame {
                bounds,
                radius: self.radius,
                focus: self
                    .control
                    .clone()
                    .map(ReturnTarget::Control)
                    .or_else(|| self.focus.clone().map(ReturnTarget::Handle)),
            },
        );
        self.child.prepaint(window, cx);
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
