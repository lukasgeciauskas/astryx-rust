//! `ChartGrid`: grid lines behind the marks (upstream `ChartGrid`), read from the
//! chart's scales. Pair it with a `ChartAxis` so an axis line always sits beside it.

use gpui::prelude::*;
use gpui::{App, Bounds, Pixels, Window, canvas, div};

use crate::context::{ChartContext, use_chart};
use crate::marks::stroke_segment;
use crate::types::ChartScale;

/// Grid lines (upstream `ChartGrid`).
///
/// ```ignore
/// ChartGrid::new()                          // horizontal lines (the default)
/// ChartGrid::new().vertical(true)           // both
/// ```
#[derive(IntoElement, Clone, Copy, Debug)]
pub struct ChartGrid {
    horizontal: bool,
    vertical: bool,
    tick_count: f64,
}

impl Default for ChartGrid {
    fn default() -> Self {
        ChartGrid {
            horizontal: true,
            vertical: false,
            tick_count: 5.,
        }
    }
}

impl ChartGrid {
    pub fn new() -> Self {
        Self::default()
    }

    /// Upstream `horizontal`: lines at the y ticks (on by default).
    pub fn horizontal(mut self, horizontal: bool) -> Self {
        self.horizontal = horizontal;
        self
    }

    /// Upstream `vertical`: lines at the x ticks, or each band's center (off by default).
    pub fn vertical(mut self, vertical: bool) -> Self {
        self.vertical = vertical;
        self
    }

    /// Upstream `tickCount`: about how many lines per axis (5 by default). Match the
    /// axes' tick count so lines and labels land on the same values.
    pub fn tick_count(mut self, count: f64) -> Self {
        self.tick_count = count;
        self
    }
}

/// A tick count d3 can take: a whole number up to 1000, 5 for an invalid one (a huge
/// count would allocate that many ticks).
pub(crate) fn safe_tick_count(count: f64) -> usize {
    if count.is_finite() && count >= 0. {
        count.floor().min(1000.) as usize
    } else {
        5
    }
}

impl ChartGrid {
    /// Where the lines sit (upstream's `hLines` and `vLines`): the y of each
    /// horizontal line and the x of each vertical one, in the plot. The y = 0 line is
    /// left to the axis, which emphasizes it, so it is never drawn twice.
    pub(crate) fn lines(&self, chart: &ChartContext) -> (Vec<f64>, Vec<f64>) {
        let count = safe_tick_count(self.tick_count);
        let horizontal = if self.horizontal {
            chart
                .y_scale
                .ticks(count as f64)
                .into_iter()
                .filter(|tick| *tick != 0.)
                .map(|tick| chart.y_scale.scale(tick))
                .collect()
        } else {
            Vec::new()
        };
        let vertical = if !self.vertical {
            Vec::new()
        } else {
            match &chart.x_scale {
                ChartScale::Band(band) => band
                    .get_domain()
                    .iter()
                    .map(|d| band.scale(d).unwrap_or(0.) + band.bandwidth() / 2.)
                    .collect(),
                ChartScale::Linear(linear) => linear
                    .ticks(count as f64)
                    .into_iter()
                    .map(|d| linear.scale(d))
                    .collect(),
            }
        };
        (horizontal, vertical)
    }
}

impl RenderOnce for ChartGrid {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let Some(chart) = use_chart(cx) else {
            return div().into_any_element();
        };
        let color = rustryx_theme::current(window, cx)
            .tokens()
            .color
            .border_emphasized;
        let (h_lines, v_lines) = self.lines(&chart);
        let (width, height) = (chart.width, chart.height);
        canvas(
            |_, _, _| {},
            move |bounds: Bounds<Pixels>, _, window, _| {
                for y in &h_lines {
                    stroke_segment(window, bounds.origin, (0., *y), (width, *y), 1., color, &[]);
                }
                for x in &v_lines {
                    stroke_segment(
                        window,
                        bounds.origin,
                        (*x, 0.),
                        (*x, height),
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
