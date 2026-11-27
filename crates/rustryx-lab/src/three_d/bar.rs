//! `ThreeDBar` (upstream lab `ThreeDBar.tsx`): bars from the floor, far to near, with a front, a
//! right and a top face shaded apart.

use gpui::prelude::*;
use gpui::{App, Bounds, Hsla, Pixels, Window, canvas, div};
use rustryx_charts::shape::{ChartPath, PathStyle};

use super::context::{ThreeDContext, use_3d};
use super::scatter::svg_opacity;
use super::types::ProjectedPoint;
use crate::lab_chart::paint::paint_path;

/// A 3D bar chart (upstream lab `ThreeDBar`).
#[derive(IntoElement)]
pub struct ThreeDBar {
    color: Hsla,
    bar_width: f64,
    bar_depth: f64,
}

impl ThreeDBar {
    /// Upstream `color`.
    pub fn new(color: Hsla) -> Self {
        ThreeDBar {
            color,
            bar_width: 0.06,
            bar_depth: 0.06,
        }
    }

    /// Upstream `barWidth`: the bars' x size in the unit cube (0.06).
    pub fn bar_width(mut self, width: f64) -> Self {
        self.bar_width = width;
        self
    }

    /// Upstream `barDepth`: their z size (0.06).
    pub fn bar_depth(mut self, depth: f64) -> Self {
        self.bar_depth = depth;
        self
    }
}

/// One bar: its eight projected corners (top and bottom, front and back, left and right),
/// its average depth and its datum.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bar3D {
    /// Upstream `topFL`: the top's front left corner.
    pub top_fl: ProjectedPoint,
    /// Upstream `topFR`: the top's front right corner.
    pub top_fr: ProjectedPoint,
    /// Upstream `topBL`: the top's back left corner.
    pub top_bl: ProjectedPoint,
    /// Upstream `topBR`: the top's back right corner.
    pub top_br: ProjectedPoint,
    /// Upstream `botFL`: the base's front left corner.
    pub bot_fl: ProjectedPoint,
    /// Upstream `botFR`: the base's front right corner.
    pub bot_fr: ProjectedPoint,
    /// Upstream `botBL`: the base's back left corner.
    pub bot_bl: ProjectedPoint,
    /// Upstream `botBR`: the base's back right corner.
    pub bot_br: ProjectedPoint,
    /// Upstream `avgDepth`: the mean depth of four corners, for sorting.
    pub avg_depth: f64,
    /// Upstream `index`: the datum's position in the data.
    pub index: usize,
}

impl Bar3D {
    /// The faces drawn (upstream's front, right and top polygons) with their fill opacities.
    pub fn faces(&self) -> [([ProjectedPoint; 4], f64); 3] {
        [
            ([self.bot_fl, self.bot_fr, self.top_fr, self.top_fl], 0.9),
            ([self.bot_fr, self.bot_br, self.top_br, self.top_fr], 0.7),
            ([self.top_fl, self.top_fr, self.top_br, self.top_bl], 1.),
        ]
    }
}

/// The bars far to near (upstream's `bars` memo).
pub fn bar_geometry(ctx: &ThreeDContext, bar_width: f64, bar_depth: f64) -> Vec<Bar3D> {
    let (hw, hd) = (bar_width / 2., bar_depth / 2.);
    let mut bars: Vec<Bar3D> = ctx
        .data
        .iter()
        .enumerate()
        .map(|(index, d)| {
            let (nx, ny, nz) = ctx.normalized(d);
            let p = |x: f64, y: f64, z: f64| ctx.project(x, y, z);
            let (top_fl, top_br, bot_fl, bot_br) = (
                p(nx - hw, ny, nz - hd),
                p(nx + hw, ny, nz + hd),
                p(nx - hw, 0., nz - hd),
                p(nx + hw, 0., nz + hd),
            );
            Bar3D {
                top_fl,
                top_fr: p(nx + hw, ny, nz - hd),
                top_bl: p(nx - hw, ny, nz + hd),
                top_br,
                bot_fl,
                bot_fr: p(nx + hw, 0., nz - hd),
                bot_bl: p(nx - hw, 0., nz + hd),
                bot_br,
                avg_depth: (top_fl.depth + top_br.depth + bot_fl.depth + bot_br.depth) / 4.,
                index,
            }
        })
        .collect();
    super::sort_by_depth(&mut bars, |b| b.avg_depth);
    bars
}

/// A closed polygon through projected points (an SVG `<polygon points>`).
pub(crate) fn polygon(points: &[ProjectedPoint]) -> ChartPath {
    let mut path = ChartPath::new();
    for (i, p) in points.iter().enumerate() {
        if i == 0 {
            path.move_to(p.px, p.py);
        } else {
            path.line_to(p.px, p.py);
        }
    }
    path.close_path();
    path
}

impl RenderOnce for ThreeDBar {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let Some(ctx) = use_3d(cx) else {
            return div().into_any_element();
        };
        let bars = bar_geometry(&ctx, self.bar_width, self.bar_depth);
        let color = self.color;
        canvas(
            |_, _, _| {},
            move |bounds: Bounds<Pixels>, _, window, _| {
                for bar in &bars {
                    for (corners, opacity) in bar.faces() {
                        let path = polygon(&corners);
                        paint_path(
                            window,
                            bounds.origin,
                            &path,
                            PathStyle::Fill,
                            color.opacity(svg_opacity(opacity)),
                        );
                        paint_path(
                            window,
                            bounds.origin,
                            &path,
                            PathStyle::Stroke { width: 0.5 },
                            color,
                        );
                    }
                }
            },
        )
        .size_full()
        .into_any_element()
    }
}
