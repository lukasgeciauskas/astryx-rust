//! `ChartErrorBar` (upstream lab `ChartErrorBar.tsx`): whiskers with caps at each datum.

use gpui::prelude::*;
use gpui::{App, Bounds, Hsla, Pixels, SharedString, Window, canvas, div};

use super::context::{ChartContext, use_chart};
use super::paint::stroke_segment;
use super::utils::{number_or_zero, x_pixel};

/// Error bars (upstream lab `ChartErrorBar`): a stem from the lower bound to the upper, capped
/// at each end. Pair it with `ChartBar` or `ChartDot`.
#[derive(IntoElement)]
pub struct ChartErrorBar {
    y_upper: SharedString,
    y_lower: SharedString,
    color: Option<Hsla>,
    stroke_width: f64,
    cap_width: f64,
}

impl ChartErrorBar {
    /// Upstream `yUpper` and `yLower`: the bounds' keys.
    pub fn new(y_upper: impl Into<SharedString>, y_lower: impl Into<SharedString>) -> Self {
        ChartErrorBar {
            y_upper: y_upper.into(),
            y_lower: y_lower.into(),
            color: None,
            stroke_width: 1.5,
            cap_width: 8.,
        }
    }

    /// Upstream `color` (the primary text color).
    pub fn color(mut self, color: Hsla) -> Self {
        self.color = Some(color);
        self
    }

    /// Upstream `strokeWidth` (1.5).
    pub fn stroke_width(mut self, width: f64) -> Self {
        self.stroke_width = width;
        self
    }

    /// Upstream `capWidth`: the caps' width in pixels (8).
    pub fn cap_width(mut self, width: f64) -> Self {
        self.cap_width = width;
        self
    }
}

/// A line segment's two ends.
pub type Segment = ((f64, f64), (f64, f64));

/// One error bar: its x, the upper bound's y and the lower's.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ErrorBarGeometry {
    pub x: f64,
    pub y_top: f64,
    pub y_bottom: f64,
}

impl ErrorBarGeometry {
    /// The stem and the two caps (upstream's three `<line>`s), as segments.
    pub fn segments(&self, cap_width: f64) -> [Segment; 3] {
        let half = cap_width / 2.;
        [
            ((self.x, self.y_top), (self.x, self.y_bottom)),
            ((self.x - half, self.y_top), (self.x + half, self.y_top)),
            (
                (self.x - half, self.y_bottom),
                (self.x + half, self.y_bottom),
            ),
        ]
    }
}

/// Each datum's error bar.
pub fn error_bar_geometry(ctx: &ChartContext, upper: &str, lower: &str) -> Vec<ErrorBarGeometry> {
    ctx.data
        .iter()
        .map(|d| ErrorBarGeometry {
            x: x_pixel(d, &ctx.x_key, &ctx.x_scale),
            y_top: ctx.y_scale.scale(number_or_zero(d, upper)),
            y_bottom: ctx.y_scale.scale(number_or_zero(d, lower)),
        })
        .collect()
}

impl RenderOnce for ChartErrorBar {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let Some(ctx) = use_chart(cx) else {
            return div().into_any_element();
        };
        let color = self.color.unwrap_or(
            rustryx_theme::current(window, cx)
                .tokens()
                .color
                .text_primary,
        );
        let bars = error_bar_geometry(&ctx, &self.y_upper, &self.y_lower);
        canvas(
            |_, _, _| {},
            move |bounds: Bounds<Pixels>, _, window, _| {
                for bar in &bars {
                    for (from, to) in bar.segments(self.cap_width) {
                        stroke_segment(
                            window,
                            bounds.origin,
                            from,
                            to,
                            self.stroke_width,
                            color,
                            &[],
                        );
                    }
                }
            },
        )
        .size_full()
        .into_any_element()
    }
}
