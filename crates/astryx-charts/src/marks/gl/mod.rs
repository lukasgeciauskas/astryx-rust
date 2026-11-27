//! The WebGL marks (upstream `dotGL`, `dotGLInteractive`, `heatmapGL` and
//! `streamGL`, with their shared `webgl.ts`), drawn with GPUI's own GPU-instanced
//! quads and paths: the geometry and colors upstream's shaders compute, with no GL
//! context of their own.
//!
//! Upstream mounts `dotGL`, `heatmapGL` and `streamGL` as canvases over the
//! chart's SVG, so they draw above the axes and tooltip (`Series::draws_above`);
//! `dotGLInteractive` draws inside the SVG's clipped marks, under them.

mod dot_gl;
mod dot_gl_interactive;
mod heatmap_gl;
mod pick;
mod stream_gl;

#[cfg(test)]
mod tests;

pub use dot_gl::{DotGlOptions, dot_gl};
pub use dot_gl_interactive::{DotGlInteractiveOptions, DotGlTooltipRender, dot_gl_interactive};
pub use heatmap_gl::{HeatmapGlOptions, heatmap_gl};
pub use stream_gl::{StreamGlHandle, StreamGlOptions, stream_gl};

use gpui::{Hsla, Rgba};

use crate::ChartValue;
use crate::types::{ChartScale, ResolvedPoint, SeriesContext};

/// Upstream `POINT_SIZE_COMPENSATION`: the visible circle of a point sprite ends at
/// 0.48 of the sprite (where the smoothstep edge starts), so sprites are drawn 1/0.96
/// as large to match an SVG circle's radius.
const POINT_SIZE_COMPENSATION: f64 = 1.0 / 0.96;

/// A color's red, green and blue as upstream's `hexToGL` reads them from a hex
/// string: each channel's byte over 255. Alpha is ignored, as upstream ignores a hex
/// alpha byte.
pub(crate) fn gl_rgb(color: Hsla) -> [f64; 3] {
    let c = color.to_rgb();
    [c.r, c.g, c.b].map(|v| (f64::from(v.clamp(0., 1.)) * 255.).round() / 255.)
}

/// The scale of upstream's 2x-supersampled canvases (upstream `getCanvasDPR`):
/// `(window.devicePixelRatio || 2) * 2`.
pub(crate) fn canvas_dpr(scale_factor: f32) -> f64 {
    let base = f64::from(scale_factor);
    (if base > 0. { base } else { 2. }) * 2.
}

/// The color a premultiplied fragment `(rgb * a, a)` leaves over what is below it,
/// as upstream's premultiplied canvases blend it (`ONE, ONE_MINUS_SRC_ALPHA`): GL
/// clamps each output channel to [0, 1], so an alpha past 1 brightens the color and
/// one at or below 0 draws nothing. `None` when nothing shows.
pub(crate) fn premultiplied(rgb: [f64; 3], a: f64) -> Option<Hsla> {
    if a.is_nan() || a <= 0. {
        return None;
    }
    let alpha = a.min(1.);
    let [r, g, b] = rgb.map(|c| (c * a).clamp(0., 1.) / alpha);
    Some(
        Rgba {
            r: r as f32,
            g: g as f32,
            b: b as f32,
            a: alpha as f32,
        }
        .into(),
    )
}

/// An opaque color from GL floats.
pub(crate) fn rgb_color([r, g, b]: [f64; 3]) -> Hsla {
    Rgba {
        r: r as f32,
        g: g as f32,
        b: b as f32,
        a: 1.,
    }
    .into()
}

/// d3's `+x` on a value a continuous scale is given: `null` and `undefined` are its
/// unknown (NaN), anything else is `Number(x)`.
pub(crate) fn d3_number(value: Option<&ChartValue>) -> f64 {
    match value {
        None | Some(ChartValue::Null) => f64::NAN,
        Some(value) => value.to_number(),
    }
}

/// The dot marks' points (upstream `dotGL` and `dotGLInteractive`'s `resolve`): a
/// band's center, or the raw x through the linear scale, which coerces it as d3 does;
/// y from the value when it is a number (NaN included) and 0 otherwise.
pub(crate) fn resolve_dots(ctx: &SeriesContext, key: &str) -> Vec<ResolvedPoint> {
    ctx.data
        .iter()
        .enumerate()
        .map(|(i, d)| {
            let px = match &ctx.x_scale {
                ChartScale::Band(band) => {
                    band.scale(&d.string(&ctx.x_key)).unwrap_or(0.) + band.bandwidth() / 2.
                }
                ChartScale::Linear(linear) => linear.scale(d3_number(d.get(&ctx.x_key))),
            };
            let v = match d.get(key) {
                Some(ChartValue::Number(n)) => *n,
                _ => 0.,
            };
            ResolvedPoint {
                px,
                py: ctx.y_scale.scale(v),
                py0: ctx.y_scale.scale(0.),
                data_index: i,
            }
        })
        .collect()
}

/// Fills a circle of radius `r` at (`x`, `y`) from `origin`, anti-aliased (the point
/// sprites' circle).
pub(crate) fn fill_circle(
    window: &mut gpui::Window,
    origin: gpui::Point<gpui::Pixels>,
    (x, y): (f64, f64),
    r: f64,
    color: Hsla,
) {
    if r.is_nan() || r <= 0. || !x.is_finite() || !y.is_finite() {
        return;
    }
    let bounds = gpui::Bounds::new(
        origin + gpui::point(gpui::px((x - r) as f32), gpui::px((y - r) as f32)),
        gpui::size(gpui::px((2. * r) as f32), gpui::px((2. * r) as f32)),
    );
    window.paint_quad(gpui::fill(bounds, color).corner_radii(gpui::px(r as f32)));
}

/// Strokes a circle of radius `r` at (`x`, `y`), `width` wide centered on its edge
/// as SVG strokes it, over a `fill` (transparent for none).
#[allow(clippy::too_many_arguments)]
pub(crate) fn stroke_circle(
    window: &mut gpui::Window,
    origin: gpui::Point<gpui::Pixels>,
    (x, y): (f64, f64),
    r: f64,
    width: f64,
    stroke: Hsla,
    fill: Hsla,
) {
    if r.is_nan() || r <= 0. || !x.is_finite() || !y.is_finite() {
        return;
    }
    let outer = r + width / 2.;
    let bounds = gpui::Bounds::new(
        origin + gpui::point(gpui::px((x - outer) as f32), gpui::px((y - outer) as f32)),
        gpui::size(gpui::px((2. * outer) as f32), gpui::px((2. * outer) as f32)),
    );
    window.paint_quad(
        gpui::fill(bounds, fill)
            .corner_radii(gpui::px(outer as f32))
            .border_widths(gpui::px(width as f32))
            .border_color(stroke),
    );
}
