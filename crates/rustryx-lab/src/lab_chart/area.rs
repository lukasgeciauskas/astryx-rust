//! `ChartArea` (upstream lab `ChartArea.tsx`): a filled band between two keys' values, for
//! confidence intervals and ranges.

use gpui::prelude::*;
use gpui::{App, Bounds, Hsla, Pixels, SharedString, Window, canvas, div};
use rustryx_charts::Datum;
use rustryx_charts::shape::{Area, ChartPath, Curve, PathStyle};

use super::context::{ChartContext, use_chart};
use super::paint::paint_path;
use super::utils::{number_or_zero, x_pixel};

/// An area band between two y values (upstream lab `ChartArea`).
#[derive(IntoElement)]
pub struct ChartArea {
    y_upper: Option<SharedString>,
    y_lower: Option<SharedString>,
    baseline: Option<SharedString>,
    color: Hsla,
    opacity: f64,
    stroke: bool,
    stroke_width: f64,
}

impl ChartArea {
    /// Upstream `color`: the fill.
    pub fn new(color: Hsla) -> Self {
        ChartArea {
            y_upper: None,
            y_lower: None,
            baseline: None,
            color,
            opacity: 0.2,
            stroke: false,
            stroke_width: 1.,
        }
    }

    /// Upstream `yUpper`: the upper bound's key (the baseline's without it).
    pub fn y_upper(mut self, key: impl Into<SharedString>) -> Self {
        self.y_upper = Some(key.into());
        self
    }

    /// Upstream `yLower`: the lower bound's key (the baseline's without it).
    pub fn y_lower(mut self, key: impl Into<SharedString>) -> Self {
        self.y_lower = Some(key.into());
        self
    }

    /// Upstream `baseline`: the key for a bound that is not given.
    pub fn baseline(mut self, key: impl Into<SharedString>) -> Self {
        self.baseline = Some(key.into());
        self
    }

    /// Upstream `opacity`: the fill's (0.2); the edge stroke has twice it.
    pub fn opacity(mut self, opacity: f64) -> Self {
        self.opacity = opacity;
        self
    }

    /// Upstream `stroke`: strokes the band's edges (off).
    pub fn stroke(mut self, stroke: bool) -> Self {
        self.stroke = stroke;
        self
    }

    /// Upstream `strokeWidth`: the edge stroke's width (1).
    pub fn stroke_width(mut self, width: f64) -> Self {
        self.stroke_width = width;
        self
    }
}

/// The band's path (upstream's `pathD`): the area from the lower key's values up to the
/// upper key's, joined by `curveMonotoneX`; empty without both keys.
pub fn area_geometry(ctx: &ChartContext, upper: Option<&str>, lower: Option<&str>) -> ChartPath {
    let (Some(upper), Some(lower)) = (upper, lower) else {
        return ChartPath::new();
    };
    Area::new()
        .x(|d: &Datum, _| x_pixel(d, &ctx.x_key, &ctx.x_scale))
        .y0(|d: &Datum, _| ctx.y_scale.scale(number_or_zero(d, lower)))
        .y1(|d: &Datum, _| ctx.y_scale.scale(number_or_zero(d, upper)))
        .curve(Curve::MonotoneX)
        .generate(&ctx.data)
}

/// An SVG opacity attribute as browsers apply it: clamped to 0 to 1.
pub(crate) fn svg_opacity(opacity: f64) -> f32 {
    if opacity.is_nan() {
        1.
    } else {
        opacity.clamp(0., 1.) as f32
    }
}

impl RenderOnce for ChartArea {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let Some(ctx) = use_chart(cx) else {
            return div().into_any_element();
        };
        let upper = self.y_upper.clone().or_else(|| self.baseline.clone());
        let lower = self.y_lower.clone().or_else(|| self.baseline.clone());
        let path = area_geometry(&ctx, upper.as_deref(), lower.as_deref());
        if path.is_empty() {
            return div().into_any_element();
        }
        canvas(
            |_, _, _| {},
            move |bounds: Bounds<Pixels>, _, window, _| {
                let fill = self.color.opacity(svg_opacity(self.opacity));
                paint_path(window, bounds.origin, &path, PathStyle::Fill, fill);
                if self.stroke {
                    let stroke = self.color.opacity(svg_opacity(self.opacity * 2.));
                    paint_path(
                        window,
                        bounds.origin,
                        &path,
                        PathStyle::Stroke {
                            width: self.stroke_width as f32,
                        },
                        stroke,
                    );
                }
            },
        )
        .size_full()
        .into_any_element()
    }
}
