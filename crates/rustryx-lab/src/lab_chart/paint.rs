//! Painting what upstream's SVG draws: lines, rectangles, circles, paths and text, from
//! the plot's origin.

use gpui::{App, Bounds, Hsla, Pixels, Point, SharedString, TextAlign, Window, point, px, size};
pub(crate) use rustryx_charts::marks::{fill_circle, stroke_circle};
use rustryx_charts::shape::{ChartPath, PathStyle};

/// A point in the plot as a window position.
pub(crate) fn at(origin: Point<Pixels>, x: f64, y: f64) -> Point<Pixels> {
    origin + point(px(x as f32), px(y as f32))
}

/// Strokes the segment from (`x1`, `y1`) to (`x2`, `y2`) (an SVG `<line>`), dashed by
/// `dash` (SVG `stroke-dasharray`) when it has a positive length.
#[allow(clippy::too_many_arguments)]
pub(crate) fn stroke_segment(
    window: &mut Window,
    origin: Point<Pixels>,
    (x1, y1): (f64, f64),
    (x2, y2): (f64, f64),
    width: f64,
    color: Hsla,
    dash: &[f64],
) {
    if ![x1, y1, x2, y2].iter().all(|v| v.is_finite())
        || width.is_nan()
        || width <= 0.
        || color.a <= 0.
    {
        return;
    }
    let mut b = gpui::PathBuilder::stroke(px(width as f32));
    if dash.iter().any(|d| *d > 0.) && dash.iter().all(|d| d.is_finite() && *d >= 0.) {
        let dash: Vec<Pixels> = dash.iter().map(|d| px(*d as f32)).collect();
        b = b.dash_array(&dash);
    }
    b.move_to(at(origin, x1, y1));
    b.line_to(at(origin, x2, y2));
    if let Ok(path) = b.build() {
        window.paint_path(path, color);
    }
}

/// Upstream's `stroke-dasharray` text ("6 3") as lengths; anything unreadable is solid.
pub(crate) fn parse_dash(dash: &str) -> Vec<f64> {
    let parts: Vec<f64> = dash
        .split(|c: char| c == ',' || c.is_whitespace())
        .filter(|p| !p.is_empty())
        .map(|p| p.parse::<f64>().unwrap_or(f64::NAN))
        .collect();
    if parts.iter().all(|p| p.is_finite() && *p >= 0.) {
        parts
    } else {
        Vec::new()
    }
}

/// Fills the rectangle at (`x`, `y`), `w` by `h`, with corners of radius `r` (an SVG
/// `<rect rx ry>`, whose radius is at most half of each side), and an optional border.
#[allow(clippy::too_many_arguments)]
pub(crate) fn fill_rect(
    window: &mut Window,
    origin: Point<Pixels>,
    (x, y, w, h): (f64, f64, f64, f64),
    r: f64,
    fill: Hsla,
    border: Option<(f64, Hsla)>,
) {
    if ![x, y, w, h].iter().all(|v| v.is_finite()) || w <= 0. || h <= 0. {
        return;
    }
    let r = if r.is_finite() {
        r.max(0.).min(w / 2.).min(h / 2.)
    } else {
        0.
    };
    let bounds = Bounds::new(at(origin, x, y), size(px(w as f32), px(h as f32)));
    let mut quad = gpui::fill(bounds, fill).corner_radii(px(r as f32));
    if let Some((width, color)) = border.filter(|(w, c)| *w > 0. && c.a > 0.) {
        quad = quad.border_widths(px(width as f32)).border_color(color);
    }
    window.paint_quad(quad);
}

/// Fills or strokes a path at `opacity` (an SVG `<path>`).
pub(crate) fn paint_path(
    window: &mut Window,
    origin: Point<Pixels>,
    path: &ChartPath,
    style: PathStyle,
    color: Hsla,
) {
    if color.a <= 0. {
        return;
    }
    if let PathStyle::Stroke { width } = style
        && (width.is_nan() || width <= 0.)
    {
        return;
    }
    if let Some(p) = path.build(origin, style) {
        window.paint_path(p, color);
    }
}

/// Where text sits against its x (SVG `text-anchor`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Anchor {
    Start,
    Middle,
    End,
}

/// How text sits on its y: SVG's default alphabetic baseline, or shifted down by a
/// share of the font size (SVG `dy="0.32em"`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Baseline {
    Alphabetic,
    Em(f64),
    /// SVG `dominant-baseline="central"`: the text's middle on its y.
    Central,
}

/// Paints `text` at `font_size` with its baseline at (`x`, `y`) from `origin` (an SVG
/// `<text>`), anchored by `anchor`, in `color` and `weight`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn paint_text(
    window: &mut Window,
    cx: &mut App,
    origin: Point<Pixels>,
    text: &SharedString,
    (x, y): (f64, f64),
    anchor: Anchor,
    baseline: Baseline,
    font_size: f64,
    weight: Option<gpui::FontWeight>,
    color: Hsla,
) {
    if text.is_empty() || !x.is_finite() || !y.is_finite() || color.a <= 0. {
        return;
    }
    let mut style = window.text_style();
    style.color = color;
    if let Some(weight) = weight {
        style.font_weight = weight;
    }
    let run = style.to_run(text.len());
    let line = window
        .text_system()
        .shape_line(text.clone(), px(font_size as f32), &[run], None);
    let left = match anchor {
        Anchor::Start => x,
        Anchor::Middle => x - f64::from(line.width) / 2.,
        Anchor::End => x - f64::from(line.width),
    };
    let line_height = line.ascent + line.descent;
    let top = match baseline {
        Baseline::Alphabetic => at(origin, left, y) - point(px(0.), line.ascent),
        Baseline::Em(em) => at(origin, left, y + em * font_size) - point(px(0.), line.ascent),
        Baseline::Central => at(origin, left, y) - point(px(0.), line_height / 2.),
    };
    let _ = line.paint(top, line_height, TextAlign::Left, None, window, cx);
}
