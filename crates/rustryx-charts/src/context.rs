//! The context a chart gives what it draws (upstream `ChartContext` and `useChart`):
//! its plot's size and scales, the resolved points, and its pointer events.

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{App, ElementId, SharedString, Window};

use crate::Datum;
use crate::scale::{BandScale, LinearScale};
use crate::types::{
    ChartMargin, ChartPointerEvent, ChartScale, ResolvedPoint, SeriesDef, SeriesInfo,
};

/// A pointer event listener (upstream `onPointer`'s handler).
pub type PointerHandler = Rc<dyn Fn(&ChartPointerEvent, &mut Window, &mut App)>;

/// What a chart gives its grid, axes, interactions, tooltip and children (upstream
/// `ChartContext`). Read it with [`use_chart`] while rendering inside a chart.
#[derive(Clone)]
pub struct ChartContext {
    /// The chart's element id, which chrome keys its own state under.
    pub id: ElementId,
    /// The plot's width.
    pub width: f64,
    /// The plot's height.
    pub height: f64,
    pub margin: ChartMargin,
    pub data: Rc<[Datum]>,
    pub x_key: SharedString,
    pub x_scale: ChartScale,
    pub y_scale: LinearScale,
    /// The categorical y scale (a heatmap's rows): a y axis prefers it to `y_scale`.
    pub y_band_scale: Option<BandScale>,
    /// Each series' resolved points by uid, in series order.
    pub resolved: Rc<Vec<(SharedString, Vec<ResolvedPoint>)>>,
    /// The chart's series with what the layout pass gave each (upstream writes it on
    /// the definitions): find a definition's with [`ChartContext::info`].
    pub series: Rc<[(SeriesDef, SeriesInfo)]>,
    pub(crate) handlers: Rc<RefCell<Vec<PointerHandler>>>,
}

impl ChartContext {
    /// Listens to the plot's pointer events this frame (upstream `onPointer`, whose
    /// unsubscribe is the next frame here: listeners are registered as the chart's
    /// layers render). A pointer leaving the plot sends `x` and `y` of -1 and no
    /// nearest point.
    pub fn on_pointer(
        &self,
        handler: impl Fn(&ChartPointerEvent, &mut Window, &mut App) + 'static,
    ) {
        self.handlers.borrow_mut().push(Rc::new(handler));
    }

    /// The points series `uid` resolved.
    pub fn resolved_for(&self, uid: &str) -> Option<&[ResolvedPoint]> {
        self.resolved
            .iter()
            .find(|(k, _)| k == uid)
            .map(|(_, p)| p.as_slice())
    }

    /// What the layout pass gave the definition `series` (by identity), when it is one
    /// of the chart's.
    pub fn info(&self, series: &SeriesDef) -> Option<&SeriesInfo> {
        self.series
            .iter()
            .find(|(s, _)| Rc::ptr_eq(s, series))
            .map(|(_, info)| info)
    }
}

/// The chart drawing what is rendering (upstream `useChart`); `None` outside a chart,
/// where upstream throws.
pub fn use_chart(cx: &App) -> Option<ChartContext> {
    rustryx_primitives::use_context::<ChartContext>(cx)
}
