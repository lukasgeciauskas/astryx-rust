//! A layout that answers its own width before it paints (upstream's container
//! queries, `@container (max-width: …)`): the content builds for the width it had last
//! frame, and on a frame whose width crosses the threshold it builds and lays out again
//! for the new width before it prepaints, so the content never paints the other layout.
//! The surroundings measured the other layout's height on that frame and catch up on the
//! next.

use gpui::{
    AnyElement, App, AvailableSpace, Bounds, Element, ElementId, GlobalElementId,
    InspectorElementId, LayoutId, Pixels, Window, size,
};

type BuildFn = Box<dyn Fn(bool, &mut Window, &mut App) -> AnyElement>;

/// Content built for whether its width is below `threshold` (`narrow`).
pub(crate) struct Responsive {
    threshold: Pixels,
    guess: bool,
    build: BuildFn,
    child: Option<AnyElement>,
}

impl Responsive {
    /// Content that builds with `build(narrow)`; `last` is the answer of the frame
    /// before.
    pub(crate) fn new(
        threshold: Pixels,
        last: bool,
        build: impl Fn(bool, &mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        Responsive {
            threshold,
            guess: last,
            build: Box::new(build),
            child: None,
        }
    }
}

impl Element for Responsive {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let mut child = (self.build)(self.guess, window, cx);
        let layout = child.request_layout(window, cx);
        self.child = Some(child);
        (layout, ())
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
        let narrow = bounds.size.width < self.threshold;
        if narrow == self.guess {
            if let Some(child) = self.child.as_mut() {
                child.prepaint(window, cx);
            }
            return;
        }
        let mut child = (self.build)(narrow, window, cx);
        child.layout_as_root(
            size(
                AvailableSpace::Definite(bounds.size.width),
                AvailableSpace::MinContent,
            ),
            window,
            cx,
        );
        child.prepaint_at(bounds.origin, window, cx);
        self.child = Some(child);
        // The surroundings measured the other layout: the next frame lays them out again.
        window.refresh();
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
        if let Some(child) = self.child.as_mut() {
            child.paint(window, cx);
        }
    }
}
