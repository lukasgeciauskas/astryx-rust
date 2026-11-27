//! `heatmap_gl`: a grid of colored cells (upstream `marks/heatmapGL.tsx`), upstream's
//! WebGL heatmap drawn as GPUI quads. Columns sit on the chart's x band scale and rows
//! on the y band scale the chart builds over `y_key`.

use std::rc::Rc;

use gpui::prelude::*;
use gpui::{
    AnyElement, App, Bounds, Hsla, IntoElement, Pixels, SharedString, Window, canvas, point, px,
    size,
};

use super::{gl_rgb, rgb_color};
use crate::scale::BandScale;
use crate::types::{
    ChartScale, GroupInfo, ResolvedPoint, Series, SeriesContext, SeriesDef, SeriesInfo,
    SeriesLayout, StackOffset,
};
use crate::{ChartValue, Datum};

/// A WebGL heatmap's options (upstream `HeatmapGLOptions`).
#[derive(Clone, Debug)]
pub struct HeatmapGlOptions {
    /// The field that places a cell's column on the chart's x band scale (upstream
    /// `xKey`).
    pub x_key: SharedString,
    /// The field whose values are the rows (upstream `yKey`).
    pub y_key: SharedString,
    /// The field whose value colors a cell (upstream `valueKey`).
    pub value_key: SharedString,
    /// The color ramp from the domain's low end to its high end (upstream
    /// `colorRange`), sampled linearly in sRGB.
    pub color_range: Vec<Hsla>,
    /// The values the ramp spans (upstream `domain`); the data's finite extent by
    /// default.
    pub domain: Option<[f64; 2]>,
    /// The space between cells, in pixels (upstream `cellGap`), 1 by default.
    pub cell_gap: Option<f64>,
}

impl HeatmapGlOptions {
    /// Options for cells at `x_key` and `y_key` colored by `value_key` along
    /// `color_range` (upstream `{xKey, yKey, valueKey, colorRange}`).
    pub fn new(
        x_key: impl Into<SharedString>,
        y_key: impl Into<SharedString>,
        value_key: impl Into<SharedString>,
        color_range: impl IntoIterator<Item = Hsla>,
    ) -> Self {
        Self {
            x_key: x_key.into(),
            y_key: y_key.into(),
            value_key: value_key.into(),
            color_range: color_range.into_iter().collect(),
            domain: None,
            cell_gap: None,
        }
    }

    /// Sets the values the ramp spans (upstream `domain`).
    pub fn domain(mut self, min: f64, max: f64) -> Self {
        self.domain = Some([min, max]);
        self
    }

    /// Sets the space between cells (upstream `cellGap`).
    pub fn cell_gap(mut self, gap: f64) -> Self {
        self.cell_gap = Some(gap);
        self
    }
}

struct HeatmapGl {
    key: SharedString,
    x_key: SharedString,
    y_key: SharedString,
    value_key: SharedString,
    ramp: Vec<[f64; 3]>,
    domain: Option<[f64; 2]>,
    cell_gap: f64,
}

/// A WebGL heatmap (upstream `heatmapGL`): a cell per row of the data, its column on
/// the chart's x band scale and its row on the y band scale over `y_key`, colored by
/// its value along the ramp. It reads no data keys, so the value stays out of the y
/// domain, and draws above the chart's axes inside the plot.
pub fn heatmap_gl(options: HeatmapGlOptions) -> SeriesDef {
    Rc::new(HeatmapGl {
        key: format!("heatmap-{}", options.value_key).into(),
        x_key: options.x_key,
        y_key: options.y_key,
        value_key: options.value_key,
        ramp: options.color_range.into_iter().map(gl_rgb).collect(),
        domain: options.domain,
        cell_gap: options.cell_gap.unwrap_or(1.),
    })
}

/// The ramp's color at `t` (upstream `sampleRamp`): black for no stops, `t` held in
/// [0, 1] (0 when it is not finite), linear between the two nearest stops.
pub(crate) fn sample_ramp(ramp: &[[f64; 3]], t: f64) -> [f64; 3] {
    if ramp.is_empty() {
        return [0., 0., 0.];
    }
    let c = 0f64.max(1f64.min(if t.is_finite() { t } else { 0. }));
    if ramp.len() == 1 {
        return ramp[0];
    }
    let s = c * (ramp.len() - 1) as f64;
    let lo = s.floor();
    let hi = (lo + 1.).min((ramp.len() - 1) as f64);
    let f = s - lo;
    let (lo, hi) = (ramp[lo as usize], ramp[hi as usize]);
    [0, 1, 2].map(|k| lo[k] + f * (hi[k] - lo[k]))
}

/// The values the ramp spans (upstream's `domain` memo): the given domain, else the
/// finite numbers' extent, else [0, 1] so the ramp math never makes NaN.
pub(crate) fn heatmap_domain(
    data: &[Datum],
    value_key: &str,
    domain: Option<[f64; 2]>,
) -> [f64; 2] {
    if let Some(domain) = domain {
        return domain;
    }
    let (mut min, mut max) = (f64::INFINITY, f64::NEG_INFINITY);
    for d in data {
        if let Some(ChartValue::Number(v)) = d.get(value_key)
            && v.is_finite()
        {
            if *v < min {
                min = *v;
            }
            if *v > max {
                max = *v;
            }
        }
    }
    if !min.is_finite() || !max.is_finite() {
        return [0., 1.];
    }
    [min, max]
}

/// One cell: its corners, as the 32-bit floats of its two triangles, and its color.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct HeatCell {
    pub(crate) x0: f32,
    pub(crate) y0: f32,
    pub(crate) x1: f32,
    pub(crate) y1: f32,
    pub(crate) color: [f32; 3],
}

/// The cells (upstream's `geometry` memo). Heatmap cells fill their full slot (the
/// step), not just the band width, centred on the band, so the grid stays contiguous
/// whatever padding the shared x scale carries; `gap` alone spaces them. A row whose
/// column or row is not in its scale's domain draws no cell, and a value that is not
/// a finite number colors as 0.
#[allow(clippy::too_many_arguments)]
pub(crate) fn heatmap_cells(
    data: &[Datum],
    x_key: &str,
    y_key: &str,
    value_key: &str,
    x_scale: &BandScale,
    y_scale: &BandScale,
    ramp: &[[f64; 3]],
    domain: [f64; 2],
    gap: f64,
) -> Vec<HeatCell> {
    let [d_min, d_max] = domain;
    let span = d_max - d_min;
    // `dMax - dMin || 1`.
    let range = if span != 0. && !span.is_nan() {
        span
    } else {
        1.
    };
    let (x_bw, y_bw) = (x_scale.bandwidth(), y_scale.bandwidth());
    let (x_step, y_step) = (x_scale.step(), y_scale.step());
    let mut cells = Vec::new();
    for d in data {
        let (Some(x_val), Some(y_val)) = (
            x_scale.scale(&d.string(x_key)),
            y_scale.scale(&d.string(y_key)),
        ) else {
            continue;
        };
        let v = match d.get(value_key) {
            Some(ChartValue::Number(v)) if v.is_finite() => *v,
            _ => 0.,
        };
        let t = (v - d_min) / range;
        let [r, g, b] = sample_ramp(ramp, t);
        let xc = x_val + x_bw / 2.;
        let yc = y_val + y_bw / 2.;
        cells.push(HeatCell {
            x0: (xc - x_step / 2. + gap / 2.) as f32,
            x1: (xc + x_step / 2. - gap / 2.) as f32,
            y0: (yc - y_step / 2. + gap / 2.) as f32,
            y1: (yc + y_step / 2. - gap / 2.) as f32,
            color: [r as f32, g as f32, b as f32],
        });
    }
    cells
}

impl Series for HeatmapGl {
    fn kind(&self) -> &'static str {
        "heatmapGL"
    }

    fn key(&self) -> SharedString {
        self.key.clone()
    }

    // The value drives cell color, not a linear y domain: no data keys keeps the
    // heatmap out of the shared linear y scale, the left axis and the value tooltip.
    fn data_keys(&self) -> Vec<SharedString> {
        Vec::new()
    }

    // `y_band_key` makes the chart build a categorical y band scale over the rows, so
    // the left axis shows the row labels aligned to each cell.
    fn layout(&self) -> SeriesLayout {
        SeriesLayout {
            y_band_key: Some(self.y_key.clone()),
            ..SeriesLayout::default()
        }
    }

    fn draws_above(&self) -> bool {
        true
    }

    // The cells are laid out at render; resolve only exposes the row and column
    // centers (for hover) on the shared band scales.
    fn resolve(
        &self,
        ctx: &SeriesContext,
        _: Option<&[StackOffset]>,
        _: Option<GroupInfo>,
    ) -> Vec<ResolvedPoint> {
        let (ChartScale::Band(xs), Some(ys)) = (&ctx.x_scale, &ctx.y_band_scale) else {
            return Vec::new();
        };
        ctx.data
            .iter()
            .enumerate()
            .map(|(i, d)| {
                let px = xs.scale(&d.string(&self.x_key)).unwrap_or(0.) + xs.bandwidth() / 2.;
                let py = ys.scale(&d.string(&self.y_key)).unwrap_or(0.) + ys.bandwidth() / 2.;
                ResolvedPoint {
                    px,
                    py,
                    py0: py,
                    data_index: i,
                }
            })
            .collect()
    }

    fn render(
        &self,
        _: &[ResolvedPoint],
        ctx: &SeriesContext,
        _: &SeriesInfo,
        _: &mut Window,
        _: &mut App,
    ) -> Option<AnyElement> {
        // Needs a categorical x (columns) and the shared y band scale (rows).
        let (ChartScale::Band(xs), Some(ys)) = (&ctx.x_scale, &ctx.y_band_scale) else {
            return None;
        };
        if ctx.width <= 0. || ctx.height <= 0. {
            return None;
        }
        let domain = heatmap_domain(&ctx.data, &self.value_key, self.domain);
        let cells = heatmap_cells(
            &ctx.data,
            &self.x_key,
            &self.y_key,
            &self.value_key,
            xs,
            ys,
            &self.ramp,
            domain,
            self.cell_gap,
        );
        Some(
            canvas(
                |_, _, _| {},
                move |bounds: Bounds<Pixels>, _, window, _| {
                    for cell in &cells {
                        paint_cell(window, bounds.origin, cell);
                    }
                },
            )
            .size_full()
            .into_any_element(),
        )
    }
}

/// Fills a cell's two triangles: the rectangle between its corners, whichever way
/// round they are (a gap wider than the step turns a cell inside out, which GL still
/// fills).
fn paint_cell(window: &mut Window, origin: gpui::Point<Pixels>, cell: &HeatCell) {
    let HeatCell {
        x0,
        y0,
        x1,
        y1,
        color,
    } = *cell;
    if ![x0, y0, x1, y1].iter().all(|v| v.is_finite()) {
        return;
    }
    let (left, right) = (x0.min(x1), x0.max(x1));
    let (top, bottom) = (y0.min(y1), y0.max(y1));
    if right <= left || bottom <= top {
        return;
    }
    let bounds = Bounds::new(
        origin + point(px(left), px(top)),
        size(px(right - left), px(bottom - top)),
    );
    let color = rgb_color(color.map(f64::from));
    window.paint_quad(gpui::fill(bounds, color));
}
