//! `ChartLine` (upstream lab `ChartLine.tsx`): a monotone line through a data key, with
//! optional dots.

use gpui::prelude::*;
use gpui::{App, Bounds, Hsla, Pixels, SharedString, Window, canvas, div};
use rustryx_charts::shape::{ChartPath, Curve, Line, PathStyle};

use super::context::{ChartContext, use_chart};
use super::paint::{fill_circle, paint_path};
use super::utils::{number_or_zero, x_pixel};

/// A line mark (upstream lab `ChartLine`), for band and linear x scales alike.
#[derive(IntoElement)]
pub struct ChartLine {
    data_key: SharedString,
    color: Hsla,
    stroke_width: f64,
    dots: bool,
    dot_radius: f64,
}

impl ChartLine {
    /// Upstream `dataKey` (the values drawn) and `color` (the stroke).
    pub fn new(data_key: impl Into<SharedString>, color: Hsla) -> Self {
        ChartLine {
            data_key: data_key.into(),
            color,
            stroke_width: 2.,
            dots: false,
            dot_radius: 3.,
        }
    }

    /// Upstream `strokeWidth` (2).
    pub fn stroke_width(mut self, width: f64) -> Self {
        self.stroke_width = width;
        self
    }

    /// Upstream `dots`: a dot at each datum (off).
    pub fn dots(mut self, dots: bool) -> Self {
        self.dots = dots;
        self
    }

    /// Upstream `dotRadius` (3).
    pub fn dot_radius(mut self, radius: f64) -> Self {
        self.dot_radius = radius;
        self
    }
}

/// The line's points and path (upstream's `points` and `pathD` memos): each datum at its x
/// pixel and its value's y (0 for a value that is not a number), joined by `curveMonotoneX`.
pub fn line_geometry(ctx: &ChartContext, data_key: &str) -> (Vec<(f64, f64)>, ChartPath) {
    let points: Vec<(f64, f64)> = ctx
        .data
        .iter()
        .map(|d| {
            (
                x_pixel(d, &ctx.x_key, &ctx.x_scale),
                ctx.y_scale.scale(number_or_zero(d, data_key)),
            )
        })
        .collect();
    let path = Line::new()
        .x(|p: &(f64, f64), _| p.0)
        .y(|p: &(f64, f64), _| p.1)
        .curve(Curve::MonotoneX)
        .generate(&points);
    (points, path)
}

impl RenderOnce for ChartLine {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let Some(ctx) = use_chart(cx) else {
            return div().into_any_element();
        };
        let (points, path) = line_geometry(&ctx, &self.data_key);
        canvas(
            |_, _, _| {},
            move |bounds: Bounds<Pixels>, _, window, _| {
                paint_path(
                    window,
                    bounds.origin,
                    &path,
                    PathStyle::Stroke {
                        width: self.stroke_width as f32,
                    },
                    self.color,
                );
                if self.dots {
                    for p in &points {
                        fill_circle(window, bounds.origin, *p, self.dot_radius, self.color);
                    }
                }
            },
        )
        .size_full()
        .into_any_element()
    }
}
