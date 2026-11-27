//! `ChartDot` (upstream lab `ChartDot.tsx`): scatter dots for a data key.

use gpui::prelude::*;
use gpui::{App, Bounds, Hsla, Pixels, SharedString, Window, canvas, div};

use super::context::{ChartContext, use_chart};
use super::paint::fill_circle;
use super::utils::{number_or_zero, x_pixel};

/// Scatter dots (upstream lab `ChartDot`).
#[derive(IntoElement)]
pub struct ChartDot {
    data_key: SharedString,
    color: Hsla,
    radius: f64,
}

impl ChartDot {
    /// Upstream `dataKey` and `color`.
    pub fn new(data_key: impl Into<SharedString>, color: Hsla) -> Self {
        ChartDot {
            data_key: data_key.into(),
            color,
            radius: 4.,
        }
    }

    /// Upstream `radius` (4).
    pub fn radius(mut self, radius: f64) -> Self {
        self.radius = radius;
        self
    }
}

/// Each datum's dot center (upstream's `<circle cx cy>`s).
pub fn dot_geometry(ctx: &ChartContext, data_key: &str) -> Vec<(f64, f64)> {
    ctx.data
        .iter()
        .map(|d| {
            (
                x_pixel(d, &ctx.x_key, &ctx.x_scale),
                ctx.y_scale.scale(number_or_zero(d, data_key)),
            )
        })
        .collect()
}

impl RenderOnce for ChartDot {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let Some(ctx) = use_chart(cx) else {
            return div().into_any_element();
        };
        let dots = dot_geometry(&ctx, &self.data_key);
        canvas(
            |_, _, _| {},
            move |bounds: Bounds<Pixels>, _, window, _| {
                for p in &dots {
                    fill_circle(window, bounds.origin, *p, self.radius, self.color);
                }
            },
        )
        .size_full()
        .into_any_element()
    }
}
