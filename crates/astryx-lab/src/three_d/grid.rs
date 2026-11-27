//! `ThreeDGrid` (upstream lab `ThreeDGrid.tsx`): a wireframe on the floor of the cube (y = 0).

use gpui::prelude::*;
use gpui::{App, Bounds, Pixels, Window, canvas, div};

use super::context::{ThreeDContext, use_3d};
use crate::lab_chart::paint::stroke_segment;

/// The floor grid (upstream lab `ThreeDGrid`).
#[derive(IntoElement)]
pub struct ThreeDGrid {
    divisions: u32,
}

impl Default for ThreeDGrid {
    fn default() -> Self {
        Self::new()
    }
}

impl ThreeDGrid {
    /// A grid of 5 divisions each way.
    pub fn new() -> Self {
        ThreeDGrid { divisions: 5 }
    }

    /// Upstream `divisions` (5).
    pub fn divisions(mut self, divisions: u32) -> Self {
        self.divisions = divisions;
        self
    }
}

/// One grid line on screen, with its middle's depth.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GridLine3D {
    /// Upstream `x1`: the start's x.
    pub x1: f64,
    /// Upstream `y1`: the start's y.
    pub y1: f64,
    /// Upstream `x2`: the end's x.
    pub x2: f64,
    /// Upstream `y2`: the end's y.
    pub y2: f64,
    /// Upstream `depth`: the depth of the line's middle.
    pub depth: f64,
}

/// The grid's lines far to near (upstream's `lines` memo): for each division boundary a line
/// along z and one along x, sorted by depth.
pub fn grid_lines(ctx: &ThreeDContext, divisions: u32) -> Vec<GridLine3D> {
    let mut lines = Vec::new();
    for i in 0..=divisions {
        let t = f64::from(i) / f64::from(divisions);
        let line = |a: super::ProjectedPoint, b: super::ProjectedPoint| GridLine3D {
            x1: a.px,
            y1: a.py,
            x2: b.px,
            y2: b.py,
            depth: (a.depth + b.depth) / 2.,
        };
        lines.push(line(ctx.project(t, 0., 0.), ctx.project(t, 0., 1.)));
        lines.push(line(ctx.project(0., 0., t), ctx.project(1., 0., t)));
    }
    super::sort_by_depth(&mut lines, |l| l.depth);
    lines
}

impl RenderOnce for ThreeDGrid {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let Some(ctx) = use_3d(cx) else {
            return div().into_any_element();
        };
        let color = astryx_theme::current(window, cx)
            .tokens()
            .color
            .border
            .opacity(0.3);
        let lines = grid_lines(&ctx, self.divisions);
        canvas(
            |_, _, _| {},
            move |bounds: Bounds<Pixels>, _, window, _| {
                for l in &lines {
                    stroke_segment(
                        window,
                        bounds.origin,
                        (l.x1, l.y1),
                        (l.x2, l.y2),
                        1.,
                        color,
                        &[],
                    );
                }
            },
        )
        .size_full()
        .into_any_element()
    }
}
