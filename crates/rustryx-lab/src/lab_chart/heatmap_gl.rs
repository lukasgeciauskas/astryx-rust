//! `ChartHeatmapGL` (upstream lab `ChartHeatmapGL.tsx`): a grid of colored cells, columns on
//! the chart's x band scale and rows on a band scale of its own.

use gpui::prelude::*;
use gpui::{App, Bounds, Hsla, SharedString, Window, div, px, size};
use rustryx_charts::scale::BandScale;
use rustryx_charts::{ChartValue, js_string};

use super::context::{ChartContext, use_chart};
use super::utils::number_or_zero;
use super::webgl::{hex_to_gl, rgb_color};

/// A WebGL heatmap (upstream lab `ChartHeatmapGL`), drawn above the chart's SVG content.
#[derive(IntoElement)]
pub struct ChartHeatmapGl {
    x_key: SharedString,
    y_key: SharedString,
    value_key: SharedString,
    color_range: Vec<Hsla>,
    domain: Option<[f64; 2]>,
    cell_gap: f64,
}

impl ChartHeatmapGl {
    /// Upstream `xKey` (a cell's column on the chart's x band scale), `yKey` (its row),
    /// `valueKey` (what colors it) and `colorRange` (the ramp from low to high).
    pub fn new(
        x_key: impl Into<SharedString>,
        y_key: impl Into<SharedString>,
        value_key: impl Into<SharedString>,
        color_range: impl IntoIterator<Item = Hsla>,
    ) -> Self {
        ChartHeatmapGl {
            x_key: x_key.into(),
            y_key: y_key.into(),
            value_key: value_key.into(),
            color_range: color_range.into_iter().collect(),
            domain: None,
            cell_gap: 1.,
        }
    }

    /// Upstream `domain`: the values the ramp spans (the data's extent).
    pub fn domain(mut self, min: f64, max: f64) -> Self {
        self.domain = Some([min, max]);
        self
    }

    /// Upstream `cellGap`: the space between cells (1).
    pub fn cell_gap(mut self, gap: f64) -> Self {
        self.cell_gap = gap;
        self
    }
}

/// The ramp's color at `t` (upstream `sampleRamp`): clamped to 0 to 1 and interpolated
/// between the two nearest stops.
pub fn sample_ramp(ramp: &[[f64; 3]], t: f64) -> Option<[f64; 3]> {
    let c = t.clamp(0., 1.);
    match ramp {
        [] => None,
        [one] => Some(*one),
        _ if c.is_nan() => None,
        _ => {
            let s = c * (ramp.len() - 1) as f64;
            let lo = s.floor() as usize;
            let hi = (lo + 1).min(ramp.len() - 1);
            let f = s - lo as f64;
            Some([0, 1, 2].map(|i| ramp[lo][i] + f * (ramp[hi][i] - ramp[lo][i])))
        }
    }
}

/// One cell: its rectangle (x0, y0, x1, y1) and color.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HeatmapCell {
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
    pub color: [f64; 3],
}

/// The rows' band scale (upstream's `yBandScale`): each distinct row in order, over the plot's
/// height, padded 0.05.
pub fn heatmap_rows(ctx: &ChartContext, y_key: &str) -> BandScale {
    BandScale::new()
        .domain(ctx.data.iter().map(|d| js_string(d.get(y_key))))
        .range(0., ctx.height)
        .padding(0.05)
}

/// The value domain (upstream's `domain` memo): as given, else the numbers' extent.
pub fn heatmap_domain(ctx: &ChartContext, value_key: &str, domain: Option<[f64; 2]>) -> [f64; 2] {
    if let Some(d) = domain {
        return d;
    }
    let (mut min, mut max) = (f64::INFINITY, f64::NEG_INFINITY);
    for d in ctx.data.iter() {
        if let Some(ChartValue::Number(v)) = d.get(value_key) {
            if *v < min {
                min = *v;
            }
            if *v > max {
                max = *v;
            }
        }
    }
    [min, max]
}

/// The cells (upstream's draw effect): none unless the chart's x scale is a band scale. A
/// cell fills its column's and row's whole step, less the gap, centered on the band. A
/// value that is NaN has no color upstream (its ramp lookup throws), and no cell here.
pub fn heatmap_cells(
    ctx: &ChartContext,
    x_key: &str,
    y_key: &str,
    value_key: &str,
    ramp: &[[f64; 3]],
    domain: [f64; 2],
    gap: f64,
) -> Vec<HeatmapCell> {
    let Some(x_band) = ctx.x_scale.as_band() else {
        return Vec::new();
    };
    let rows = heatmap_rows(ctx, y_key);
    let [d_min, d_max] = domain;
    let span = d_max - d_min;
    let range = if span == 0. || span.is_nan() {
        1.
    } else {
        span
    };
    let (x_bw, y_bw) = (x_band.bandwidth(), rows.bandwidth());
    let (x_step, y_step) = (x_band.step(), rows.step());
    ctx.data
        .iter()
        .filter_map(|d| {
            let x = x_band.scale(&js_string(d.get(x_key)))?;
            let y = rows.scale(&js_string(d.get(y_key)))?;
            let v = number_or_zero(d, value_key);
            let color = sample_ramp(ramp, (v - d_min) / range)?;
            let (xc, yc) = (x + x_bw / 2., y + y_bw / 2.);
            Some(HeatmapCell {
                x0: xc - x_step / 2. + gap / 2.,
                x1: xc + x_step / 2. - gap / 2.,
                y0: yc - y_step / 2. + gap / 2.,
                y1: yc + y_step / 2. - gap / 2.,
                color,
            })
        })
        .collect()
}

impl RenderOnce for ChartHeatmapGl {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let Some(ctx) = use_chart(cx) else {
            return div();
        };
        if ctx.width <= 0. || ctx.height <= 0. {
            return div();
        }
        let ramp: Vec<[f64; 3]> = self.color_range.iter().map(|c| hex_to_gl(*c)).collect();
        let domain = heatmap_domain(&ctx, &self.value_key, self.domain);
        let cells = heatmap_cells(
            &ctx,
            &self.x_key,
            &self.y_key,
            &self.value_key,
            &ramp,
            domain,
            self.cell_gap,
        );
        ctx.paint_over(move |origin, window, _| {
            for c in &cells {
                // Two triangles from (x0, y0) to (x1, y1), drawn whichever way the corners lie.
                let (l, r) = (c.x0.min(c.x1), c.x0.max(c.x1));
                let (t, b) = (c.y0.min(c.y1), c.y0.max(c.y1));
                if !(r > l && b > t) {
                    continue;
                }
                let bounds = Bounds::new(
                    origin + gpui::point(px(l as f32), px(t as f32)),
                    size(px((r - l) as f32), px((b - t) as f32)),
                );
                window.paint_quad(gpui::fill(bounds, rgb_color(c.color)));
            }
        });
        div()
    }
}
