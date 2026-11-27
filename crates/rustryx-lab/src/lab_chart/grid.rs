//! `ChartGrid` (upstream lab `ChartGrid.tsx`): dashed grid lines behind the marks.

use gpui::prelude::*;
use gpui::{App, Bounds, Pixels, Window, canvas, div};
use rustryx_charts::ChartScale;

use super::context::{ChartContext, use_chart};
use super::paint::stroke_segment;

/// Grid lines (upstream lab `ChartGrid`): horizontal ones at the y ticks, vertical ones at the
/// band centers or x ticks.
#[derive(IntoElement)]
pub struct ChartGrid {
    horizontal: bool,
    vertical: bool,
}

impl Default for ChartGrid {
    fn default() -> Self {
        Self::new()
    }
}

impl ChartGrid {
    /// Horizontal lines only (upstream's defaults).
    pub fn new() -> Self {
        ChartGrid {
            horizontal: true,
            vertical: false,
        }
    }

    /// Upstream `horizontal` (on).
    pub fn horizontal(mut self, horizontal: bool) -> Self {
        self.horizontal = horizontal;
        self
    }

    /// Upstream `vertical` (off).
    pub fn vertical(mut self, vertical: bool) -> Self {
        self.vertical = vertical;
        self
    }
}

/// The lines' positions (upstream's `hLines` and `vLines`): the y pixels of five y ticks,
/// and the x pixels of the band centers or of five x ticks.
pub fn grid_geometry(ctx: &ChartContext, horizontal: bool, vertical: bool) -> (Vec<f64>, Vec<f64>) {
    let h = if horizontal {
        ctx.y_scale
            .ticks(5.)
            .into_iter()
            .map(|t| ctx.y_scale.scale(t))
            .collect()
    } else {
        Vec::new()
    };
    let v = if !vertical {
        Vec::new()
    } else {
        match &ctx.x_scale {
            ChartScale::Band(band) => band
                .get_domain()
                .iter()
                .map(|d| band.scale(d).unwrap_or(0.) + band.bandwidth() / 2.)
                .collect(),
            ChartScale::Linear(linear) => linear
                .ticks(5.)
                .into_iter()
                .map(|t| linear.scale(t))
                .collect(),
        }
    };
    (h, v)
}

impl RenderOnce for ChartGrid {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let Some(ctx) = use_chart(cx) else {
            return div().into_any_element();
        };
        let color = rustryx_theme::current(window, cx)
            .tokens()
            .color
            .border
            .opacity(0.5);
        let (h, v) = grid_geometry(&ctx, self.horizontal, self.vertical);
        let (width, height) = (ctx.width, ctx.height);
        canvas(
            |_, _, _| {},
            move |bounds: Bounds<Pixels>, _, window, _| {
                for y in &h {
                    stroke_segment(
                        window,
                        bounds.origin,
                        (0., *y),
                        (width, *y),
                        1.,
                        color,
                        &[4., 4.],
                    );
                }
                for x in &v {
                    stroke_segment(
                        window,
                        bounds.origin,
                        (*x, 0.),
                        (*x, height),
                        1.,
                        color,
                        &[4., 4.],
                    );
                }
            },
        )
        .size_full()
        .into_any_element()
    }
}
