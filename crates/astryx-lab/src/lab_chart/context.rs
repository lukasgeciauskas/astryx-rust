//! The context a lab chart gives what it draws (upstream lab `types.ts` and
//! `ChartContext.ts`): the plot's size, margins, data and scales, and the conversions
//! from a pointer to data.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use astryx_charts::scale::LinearScale;
use astryx_charts::{ChartScale, ChartValue, Datum};
use gpui::{App, ElementId, Pixels, Point, SharedString, Window, point, px};

/// The space around the plot (upstream `ChartMargin`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChartMargin {
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
    pub left: f64,
}

/// The lab chart's margins: 16 above and right, 32 below, 48 left (upstream
/// `DEFAULT_MARGIN`).
pub const DEFAULT_MARGIN: ChartMargin = ChartMargin {
    top: 16.,
    right: 16.,
    bottom: 32.,
    left: 48.,
};

impl Default for ChartMargin {
    fn default() -> Self {
        DEFAULT_MARGIN
    }
}

/// Some sides of a margin (upstream `Partial<ChartMargin>`): the sides set replace the
/// default's.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MarginOverride {
    pub top: Option<f64>,
    pub right: Option<f64>,
    pub bottom: Option<f64>,
    pub left: Option<f64>,
}

impl MarginOverride {
    /// The default margin with these sides set (upstream `{...DEFAULT_MARGIN, ...margin}`).
    pub fn over(self, base: ChartMargin) -> ChartMargin {
        ChartMargin {
            top: self.top.unwrap_or(base.top),
            right: self.right.unwrap_or(base.right),
            bottom: self.bottom.unwrap_or(base.bottom),
            left: self.left.unwrap_or(base.left),
        }
    }
}

impl From<ChartMargin> for MarginOverride {
    fn from(m: ChartMargin) -> Self {
        MarginOverride {
            top: Some(m.top),
            right: Some(m.right),
            bottom: Some(m.bottom),
            left: Some(m.left),
        }
    }
}

/// A position in the plot read back as data (upstream `DataPoint`).
#[derive(Clone, Debug, PartialEq)]
pub struct DataPoint {
    /// The x value: a number on a linear scale, a category on a band scale, `None` where
    /// upstream has `null` (no chart drawn yet) or `undefined` (an empty band domain).
    pub x: Option<ChartValue>,
    /// The y value.
    pub y: f64,
    /// The x pixel within the plot.
    pub px: f64,
    /// The y pixel within the plot.
    pub py: f64,
}

/// What a GL mark paints after everything else in the plot (upstream's canvases mounted
/// over the chart's SVG).
pub(crate) type OverlayPaint = Box<dyn FnOnce(Point<Pixels>, &mut Window, &mut App)>;

/// What a lab chart gives its children (upstream `ChartContext`). Read it with
/// [`use_chart`] while rendering inside a [`Chart`](super::Chart).
#[derive(Clone)]
pub struct ChartContext {
    /// The chart's element id, which children key their own state under.
    pub id: ElementId,
    /// Upstream `width`: the plot's width (the chart's less its margins).
    pub width: f64,
    /// Upstream `height`: the plot's height.
    pub height: f64,
    /// Upstream `margin`.
    pub margin: ChartMargin,
    /// Upstream `xKey`.
    pub x_key: SharedString,
    /// Upstream `data`.
    pub data: Rc<[Datum]>,
    /// Upstream `xScale`: a band scale for categories, a linear one for numbers.
    pub x_scale: ChartScale,
    /// Upstream `yScale`.
    pub y_scale: LinearScale,
    /// The plot's top-left corner in the window, known once it is laid out.
    pub(crate) origin: Rc<Cell<Point<Pixels>>>,
    /// What the GL marks paint above the plot this frame.
    pub(crate) overlays: Rc<RefCell<Vec<OverlayPaint>>>,
    /// What children put in the chart's container, outside the plot's image (upstream's
    /// portals into the SVG's parent).
    pub(crate) portal: Rc<RefCell<Vec<gpui::AnyElement>>>,
}

impl ChartContext {
    /// Upstream `pixelToData`: a plot position as data. A band scale reads the band the
    /// pixel falls in by its step, clamped to the first and last.
    pub fn pixel_to_data(&self, px: f64, py: f64) -> DataPoint {
        let y = self.y_scale.invert(py);
        let x = match &self.x_scale {
            ChartScale::Band(band) => {
                let domain = band.get_domain();
                let step = band.step();
                let idx = (px / step).floor().max(0.).min(domain.len() as f64 - 1.);
                (idx >= 0.)
                    .then(|| domain.get(idx as usize).cloned())
                    .flatten()
                    .map(ChartValue::Text)
            }
            ChartScale::Linear(linear) => Some(ChartValue::Number(linear.invert(px))),
        };
        DataPoint { x, y, px, py }
    }

    /// Upstream `pointerToData`: a window position (a pointer's) as data.
    pub fn pointer_to_data(&self, position: Point<Pixels>) -> DataPoint {
        let local = self.to_local(position);
        self.pixel_to_data(local.0, local.1)
    }

    /// A window position relative to the plot's top-left corner.
    pub fn to_local(&self, position: Point<Pixels>) -> (f64, f64) {
        let origin = self.origin.get();
        (
            f64::from(position.x - origin.x),
            f64::from(position.y - origin.y),
        )
    }

    /// The plot's top-left corner in the window.
    pub fn plot_origin(&self) -> Point<Pixels> {
        self.origin.get()
    }

    /// Paints `paint` after the plot's other children (above its axes and interactions,
    /// as upstream's canvases sit over its SVG), from the plot's origin.
    pub(crate) fn paint_over(
        &self,
        paint: impl FnOnce(Point<Pixels>, &mut Window, &mut App) + 'static,
    ) {
        self.overlays.borrow_mut().push(Box::new(paint));
    }

    /// Puts `element` in the chart's container, over the whole chart and outside the
    /// plot's image (upstream's `createPortal` into the SVG's parent).
    pub(crate) fn portal(&self, element: impl gpui::IntoElement) {
        self.portal.borrow_mut().push(element.into_any_element());
    }

    /// A context for a plot of `width` by `height` at the window's corner, for code that
    /// computes geometry outside a chart (tests, a custom mark's preview).
    pub fn detached(
        data: Rc<[Datum]>,
        x_key: impl Into<SharedString>,
        x_scale: ChartScale,
        y_scale: LinearScale,
        width: f64,
        height: f64,
        margin: ChartMargin,
    ) -> Self {
        ChartContext {
            id: ElementId::Name("lab-chart".into()),
            width,
            height,
            margin,
            x_key: x_key.into(),
            data,
            x_scale,
            y_scale,
            origin: Rc::new(Cell::new(point(px(0.), px(0.)))),
            overlays: Rc::default(),
            portal: Rc::default(),
        }
    }
}

/// The chart drawing what is rendering (upstream `useChart`); `None` outside a chart,
/// where upstream throws "Chart components must be used inside <Chart>".
pub fn use_chart(cx: &App) -> Option<ChartContext> {
    astryx_primitives::use_context::<ChartContext>(cx)
}
