//! A plot that knows its width before it lays its content out: as wide as it is given
//! and a fixed height tall, it builds its content in prepaint once its width is
//! known (upstream measures the container with a ResizeObserver and renders nothing at
//! width 0). Shared by RadialChart and SankeyChart.

use std::panic::Location;

use gpui::{
    AnyElement, App, AvailableSpace, Bounds, Element, ElementId, GlobalElementId,
    InspectorElementId, LayoutId, Pixels, Style, Window, px, relative,
};

type BuildFn = Box<dyn FnOnce(Pixels, &mut Window, &mut App) -> Option<(AnyElement, Pixels)>>;

/// The element: `build` gets the measured width and returns the content and its
/// height, laid out at that width.
pub(crate) struct MeasuredPlot {
    id: ElementId,
    height: Pixels,
    build: Option<BuildFn>,
    child: Option<AnyElement>,
}

impl MeasuredPlot {
    /// A plot `height` tall (its content may be taller, as a scroll bar adds).
    pub(crate) fn new(
        id: impl Into<ElementId>,
        height: Pixels,
        build: impl FnOnce(Pixels, &mut Window, &mut App) -> Option<(AnyElement, Pixels)> + 'static,
    ) -> Self {
        MeasuredPlot {
            id: id.into(),
            height,
            build: Some(Box::new(build)),
            child: None,
        }
    }
}

impl Element for MeasuredPlot {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
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
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = self.height.into();
        (window.request_layout(style, [], cx), ())
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
        let Some(build) = self.build.take() else {
            return;
        };
        if bounds.size.width <= px(0.) {
            return;
        }
        let Some((mut element, height)) = build(bounds.size.width, window, cx) else {
            return;
        };
        element.layout_as_root(
            gpui::size(
                AvailableSpace::Definite(bounds.size.width),
                AvailableSpace::Definite(height),
            ),
            window,
            cx,
        );
        element.prepaint_at(bounds.origin, window, cx);
        self.child = Some(element);
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
