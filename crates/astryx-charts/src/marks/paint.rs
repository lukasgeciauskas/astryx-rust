//! What the marks share: option sanitizing and painting a mark's shapes into the plot.

use gpui::prelude::*;
use gpui::{AnyElement, Bounds, Pixels, canvas, linear_color_stop, linear_gradient};

use crate::shape::PathStyle;
use crate::types::MarkShape;

/// A user opacity in [0, 1], or `fallback` when it is not a finite number (upstream
/// `clampOpacity`).
pub(crate) fn clamp_opacity(value: Option<f64>, fallback: f64) -> f64 {
    let n = value.filter(|v| v.is_finite()).unwrap_or(fallback);
    n.clamp(0., 1.)
}

/// A user length (a radius, a width) as a finite non-negative number, or `fallback`
/// (upstream `nonNegative`).
pub(crate) fn non_negative(value: Option<f64>, fallback: f64) -> f64 {
    value
        .filter(|v| v.is_finite())
        .map_or(fallback, |v| v.max(0.))
}

/// Strokes the segment from (`x1`, `y1`) to (`x2`, `y2`) at `origin` (an SVG
/// `<line>`), dashed by `dash` when it is not empty.
#[allow(clippy::too_many_arguments)]
pub(crate) fn stroke_segment(
    window: &mut gpui::Window,
    origin: gpui::Point<gpui::Pixels>,
    (x1, y1): (f64, f64),
    (x2, y2): (f64, f64),
    width: f64,
    color: gpui::Hsla,
    dash: &[f64],
) {
    if ![x1, y1, x2, y2].iter().all(|v| v.is_finite()) || width.is_nan() || width <= 0. {
        return;
    }
    let at = |x: f64, y: f64| origin + gpui::point(gpui::px(x as f32), gpui::px(y as f32));
    let mut b = gpui::PathBuilder::stroke(gpui::px(width as f32));
    if dash.iter().any(|d| *d > 0.) && dash.iter().all(|d| d.is_finite() && *d >= 0.) {
        let dash: Vec<gpui::Pixels> = dash.iter().map(|d| gpui::px(*d as f32)).collect();
        b = b.dash_array(&dash);
    }
    b.move_to(at(x1, y1));
    b.line_to(at(x2, y2));
    if let Ok(path) = b.build() {
        window.paint_path(path, color);
    }
}

/// Fills the rectangle at (`x`, `y`) of `w` by `h` from `origin` (an SVG `<rect>`).
pub(crate) fn fill_rect(
    window: &mut gpui::Window,
    origin: gpui::Point<gpui::Pixels>,
    (x, y, w, h): (f64, f64, f64, f64),
    color: gpui::Hsla,
) {
    if ![x, y, w, h].iter().all(|v| v.is_finite()) || w <= 0. || h <= 0. {
        return;
    }
    let bounds = gpui::Bounds::new(
        origin + gpui::point(gpui::px(x as f32), gpui::px(y as f32)),
        gpui::size(gpui::px(w as f32), gpui::px(h as f32)),
    );
    window.paint_quad(gpui::fill(bounds, color));
}

/// JavaScript's `Math.max(lo, Math.min(v, hi))`: `lo` wins when the range is empty.
pub(crate) fn clamp_js(v: f64, lo: f64, hi: f64) -> f64 {
    let m = if v.is_nan() || hi.is_nan() {
        f64::NAN
    } else {
        v.min(hi)
    };
    if m.is_nan() || lo.is_nan() {
        f64::NAN
    } else {
        lo.max(m)
    }
}

/// Fills a circle of radius `r` at (`x`, `y`) from `origin` (an SVG `<circle>`).
pub(crate) fn paint_circle(
    window: &mut gpui::Window,
    origin: gpui::Point<Pixels>,
    x: f64,
    y: f64,
    r: f64,
    fill: gpui::Hsla,
) {
    if ![x, y, r].iter().all(|v| v.is_finite()) || r <= 0. {
        return;
    }
    let bounds = Bounds::new(
        origin + gpui::point(gpui::px((x - r) as f32), gpui::px((y - r) as f32)),
        gpui::size(gpui::px((2. * r) as f32), gpui::px((2. * r) as f32)),
    );
    window.paint_quad(gpui::fill(bounds, fill).corner_radii(gpui::px(r as f32)));
}

/// Paints a mark's shapes at `origin` (the plot's), in order. A badge is left to its
/// mark, which draws it as an element for its text.
pub(crate) fn paint_shapes(
    window: &mut gpui::Window,
    origin: gpui::Point<Pixels>,
    shapes: &[MarkShape],
) {
    for shape in shapes {
        match shape {
            MarkShape::Fill {
                path,
                color,
                opacity,
            } => {
                if let Some(p) = path.build(origin, PathStyle::Fill) {
                    window.paint_path(p, color.opacity(*opacity as f32));
                }
            }
            MarkShape::GradientFill {
                path,
                color,
                opacity,
            } => {
                if let Some(p) = path.build(origin, PathStyle::Fill) {
                    // Top to bottom over the path's own box (SVG's objectBoundingBox).
                    let fill = linear_gradient(
                        180.,
                        linear_color_stop(color.opacity(*opacity as f32), 0.),
                        linear_color_stop(color.opacity(0.), 1.),
                    );
                    window.paint_path(p, fill);
                }
            }
            MarkShape::Stroke { path, color, width } => {
                if *width > 0.
                    && let Some(p) = path.build(
                        origin,
                        PathStyle::Stroke {
                            width: *width as f32,
                        },
                    )
                {
                    window.paint_path(p, *color);
                }
            }
            MarkShape::Circle {
                cx,
                cy,
                r,
                color,
                opacity,
            } => paint_circle(window, origin, *cx, *cy, *r, color.opacity(*opacity as f32)),
            MarkShape::Segment {
                from,
                to,
                color,
                width,
                dash,
            } => stroke_segment(window, origin, *from, *to, *width, *color, dash),
            MarkShape::Rect {
                x,
                y,
                width,
                height,
                color,
                opacity,
            } => fill_rect(
                window,
                origin,
                (*x, *y, *width, *height),
                color.opacity(*opacity as f32),
            ),
            MarkShape::Badge { .. } => {}
        }
    }
}

/// A canvas filling the plot that paints `shapes`.
pub(crate) fn shapes_element(shapes: Vec<MarkShape>) -> AnyElement {
    canvas(
        |_, _, _| {},
        move |bounds: Bounds<Pixels>, _, window, _| paint_shapes(window, bounds.origin, &shapes),
    )
    .size_full()
    .into_any_element()
}
