//! The context a 3D chart gives its marks (upstream lab `ThreeDContext.ts` and `types.ts`'s
//! `ThreeDContext`).

use std::cell::RefCell;
use std::rc::Rc;

use astryx_charts::Datum;
use gpui::{App, ElementId, Pixels, Point, SharedString, Window};

use super::projection::{Projection, normalize};
use super::types::{Camera, ProjectedPoint};

/// What a GL mark paints over the chart's other children (upstream's canvas mounted beside
/// the SVG).
pub(crate) type OverlayPaint = Box<dyn FnOnce(Point<Pixels>, &mut Window, &mut App)>;

/// What a `ThreeDChart` gives its children (upstream `ThreeDContext`). Read it with
/// [`use_3d`] while rendering inside one.
#[derive(Clone)]
pub struct ThreeDContext {
    /// The chart's element id, which children key their own state under.
    pub id: ElementId,
    /// Upstream `width`.
    pub width: f64,
    /// Upstream `height`.
    pub height: f64,
    /// Upstream `data`.
    pub data: Rc<[Datum]>,
    /// Upstream `xKey`.
    pub x_key: SharedString,
    /// Upstream `yKey`: the vertical dimension.
    pub y_key: SharedString,
    /// Upstream `zKey`: the depth dimension.
    pub z_key: SharedString,
    /// Upstream `xDomain`.
    pub x_domain: [f64; 2],
    /// Upstream `yDomain`.
    pub y_domain: [f64; 2],
    /// Upstream `zDomain`.
    pub z_domain: [f64; 2],
    /// Upstream `camera`.
    pub camera: Camera,
    /// The camera's projection onto the plot.
    pub projection: Projection,
    pub(crate) overlays: Rc<RefCell<Vec<OverlayPaint>>>,
}

impl ThreeDContext {
    /// Upstream `project(nx, ny, nz)`.
    pub fn project(&self, nx: f64, ny: f64, nz: f64) -> ProjectedPoint {
        self.projection.project(nx, ny, nz)
    }

    /// Upstream `normalize(value, domain)`.
    pub fn normalize(&self, value: f64, domain: [f64; 2]) -> f64 {
        normalize(value, domain)
    }

    /// A datum's place in the unit cube: each key's value read as upstream's `as number`
    /// reads it in arithmetic (`Number(v)`, NaN when missing), normalized in its domain.
    pub fn normalized(&self, d: &Datum) -> (f64, f64, f64) {
        let n =
            |key: &SharedString, domain| normalize(astryx_charts::js_number(d.get(key)), domain);
        (
            n(&self.x_key, self.x_domain),
            n(&self.y_key, self.y_domain),
            n(&self.z_key, self.z_domain),
        )
    }

    /// A context for a `width` by `height` plot, for code that computes geometry outside a
    /// chart (tests, previews).
    #[allow(clippy::too_many_arguments)]
    pub fn detached(
        data: Rc<[Datum]>,
        keys: [&str; 3],
        domains: [[f64; 2]; 3],
        camera: Camera,
        width: f64,
        height: f64,
    ) -> Self {
        ThreeDContext {
            id: ElementId::Name("three-d".into()),
            width,
            height,
            data,
            x_key: keys[0].to_string().into(),
            y_key: keys[1].to_string().into(),
            z_key: keys[2].to_string().into(),
            x_domain: domains[0],
            y_domain: domains[1],
            z_domain: domains[2],
            camera,
            projection: Projection::new(&camera, width, height),
            overlays: Rc::default(),
        }
    }

    /// Paints `paint` after the chart's other children, from the plot's origin.
    pub(crate) fn paint_over(
        &self,
        paint: impl FnOnce(Point<Pixels>, &mut Window, &mut App) + 'static,
    ) {
        self.overlays.borrow_mut().push(Box::new(paint));
    }
}

/// The 3D chart drawing what is rendering (upstream `use3D`); `None` outside one, where
/// upstream throws "3D components must be used inside <ThreeDChart>".
pub fn use_3d(cx: &App) -> Option<ThreeDContext> {
    astryx_primitives::use_context::<ThreeDContext>(cx)
}
