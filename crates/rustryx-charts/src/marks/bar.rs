//! `bar`: a bar series (upstream `marks/bar.tsx`). Bars with no finite height are
//! skipped rather than drawn at the baseline, grouped widths never go negative, and a
//! negative value grows down from zero with its bottom corners rounded.

use std::rc::Rc;

use gpui::{AnyElement, App, SharedString, Window};
use rustryx_theme::Tokens;

use super::paint::shapes_element;
use super::{clamp_opacity, non_negative};
use crate::mark_color::point_fill;
use crate::shape::ChartPath;
use crate::types::{
    ChartScale, ColorAccessor, GroupInfo, MarkShape, ResolvedPoint, Series, SeriesContext,
    SeriesDef, SeriesInfo, SeriesLayout, StackOffset,
};

/// A bar series' options (upstream `BarOptions`).
#[derive(Clone, Debug, Default)]
pub struct BarOptions {
    pub color: Option<ColorAccessor>,
    /// 1 by default.
    pub opacity: Option<f64>,
    /// The top corners' radius, 4 by default (the top of a stack only).
    pub radius: Option<f64>,
    /// Bars with the same stack accumulate.
    pub stack: Option<SharedString>,
    /// Bars with the same group share the band side by side.
    pub group: Option<SharedString>,
    pub label: Option<SharedString>,
}

impl BarOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn color(mut self, color: impl Into<ColorAccessor>) -> Self {
        self.color = Some(color.into());
        self
    }

    pub fn opacity(mut self, opacity: f64) -> Self {
        self.opacity = Some(opacity);
        self
    }

    pub fn radius(mut self, radius: f64) -> Self {
        self.radius = Some(radius);
        self
    }

    pub fn stack(mut self, stack: impl Into<SharedString>) -> Self {
        self.stack = Some(stack.into());
        self
    }

    pub fn group(mut self, group: impl Into<SharedString>) -> Self {
        self.group = Some(group.into());
        self
    }

    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }
}

struct Bar {
    key: SharedString,
    color: Option<ColorAccessor>,
    opacity: f64,
    radius: f64,
    stack: Option<SharedString>,
    group: Option<SharedString>,
    label: SharedString,
}

/// A bar series of `data_key` (upstream `bar`), on a band x scale.
pub fn bar(data_key: impl Into<SharedString>, options: BarOptions) -> SeriesDef {
    let key = data_key.into();
    Rc::new(Bar {
        label: options.label.unwrap_or_else(|| key.clone()),
        key,
        color: options.color,
        opacity: clamp_opacity(options.opacity, 1.),
        radius: non_negative(options.radius, 4.),
        stack: options.stack,
        group: options.group,
    })
}

impl Bar {
    /// The bar's width and its offset in the band, from its group slot.
    fn geometry(bandwidth: f64, group: Option<GroupInfo>) -> (f64, f64) {
        match group {
            Some(g) if g.count > 0 => {
                let gutter = bandwidth * 0.05;
                let gutters = gutter * (g.count as f64 - 1.);
                // With many groups in a narrow band the gutters can exceed it; a
                // negative width would mirror the bar.
                let width = ((bandwidth - gutters) / g.count as f64).max(0.);
                (width, g.index as f64 * (width + gutter))
            }
            _ => (bandwidth, 0.),
        }
    }
}

impl Series for Bar {
    fn kind(&self) -> &'static str {
        "bar"
    }

    fn key(&self) -> SharedString {
        self.key.clone()
    }

    fn data_keys(&self) -> Vec<SharedString> {
        vec![self.key.clone()]
    }

    fn color(&self) -> Option<gpui::Hsla> {
        self.color.as_ref().and_then(ColorAccessor::fixed)
    }

    fn label(&self) -> Option<SharedString> {
        Some(self.label.clone())
    }

    fn layout(&self) -> SeriesLayout {
        SeriesLayout {
            stack: self.stack.clone(),
            group: self.group.clone(),
            include_zero: true,
            ..SeriesLayout::default()
        }
    }

    fn resolve(
        &self,
        ctx: &SeriesContext,
        stack: Option<&[StackOffset]>,
        group: Option<GroupInfo>,
    ) -> Vec<ResolvedPoint> {
        let ChartScale::Band(band) = &ctx.x_scale else {
            return Vec::new();
        };
        let (width, offset) = Self::geometry(band.bandwidth(), group);
        ctx.data
            .iter()
            .enumerate()
            .map(|(i, d)| {
                // A category outside the band domain gets a NaN x, so it draws nothing
                // rather than a phantom bar at the origin.
                let px = band
                    .scale(&d.string(&ctx.x_key))
                    .map_or(f64::NAN, |x| x + offset + width / 2.);
                let (py, py0) = match stack {
                    Some(offsets) => match offsets.get(i) {
                        Some(o) => (ctx.y_scale.scale(o.y1), ctx.y_scale.scale(o.y0)),
                        None => (f64::NAN, f64::NAN),
                    },
                    None => (
                        ctx.y_scale.scale(d.number_or_nan(&self.key)),
                        ctx.y_scale.scale(0.),
                    ),
                };
                ResolvedPoint {
                    px,
                    py,
                    py0,
                    data_index: i,
                }
            })
            .collect()
    }

    fn shapes(
        &self,
        resolved: &[ResolvedPoint],
        ctx: &SeriesContext,
        info: &SeriesInfo,
        tokens: &Tokens,
    ) -> Vec<MarkShape> {
        let ChartScale::Band(band) = &ctx.x_scale else {
            return Vec::new();
        };
        let (width, _) = Self::geometry(band.bandwidth(), info.group);
        if width.is_nan() || width <= 0. {
            return Vec::new();
        }
        let radius = if info.is_top_of_stack {
            self.radius
        } else {
            0.
        };
        let accent = tokens.color.accent;
        resolved
            .iter()
            .filter_map(|p| {
                let d = ctx.data.get(p.data_index)?;
                if !p.px.is_finite() || !p.py.is_finite() || !p.py0.is_finite() {
                    return None;
                }
                let fill = point_fill(info, self.color.as_ref(), d, p.data_index, accent);
                let x = p.px - width / 2.;
                let y = p.py.min(p.py0);
                let h = (p.py0 - p.py).abs().max(0.);
                Some(MarkShape::Fill {
                    path: bar_path(x, y, width, h, radius, p.py <= p.py0),
                    color: fill,
                    opacity: self.opacity,
                })
            })
            .collect()
    }

    fn render(
        &self,
        resolved: &[ResolvedPoint],
        ctx: &SeriesContext,
        info: &SeriesInfo,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<AnyElement> {
        let ChartScale::Band(band) = &ctx.x_scale else {
            return None;
        };
        let (width, _) = Self::geometry(band.bandwidth(), info.group);
        if width.is_nan() || width <= 0. {
            return None;
        }
        let tokens = rustryx_theme::current(window, cx);
        Some(shapes_element(self.shapes(
            resolved,
            ctx,
            info,
            tokens.tokens(),
        )))
    }
}

/// One bar's outline: a rectangle with its two corners away from the baseline rounded
/// (upstream `roundedTopRect` for a bar that grows up, `roundedBottomRect` for one that
/// grows down). The radius is at most half the smaller side; an empty path draws
/// nothing when a coordinate is not finite.
pub(crate) fn bar_path(x: f64, y: f64, w: f64, h: f64, r: f64, grows_up: bool) -> ChartPath {
    let mut path = ChartPath::new();
    if ![x, y, w, h].iter().all(|v| v.is_finite()) {
        return path;
    }
    let cr = r.min(w / 2.).min(h / 2.);
    let rounded = cr > 0.;
    if grows_up {
        path.move_to(x, y + h);
        if rounded {
            path.line_to(x, y + cr);
            path.quadratic_curve_to(x, y, x + cr, y);
            path.line_to(x + w - cr, y);
            path.quadratic_curve_to(x + w, y, x + w, y + cr);
        } else {
            path.line_to(x, y);
            path.line_to(x + w, y);
        }
        path.line_to(x + w, y + h);
    } else {
        path.move_to(x, y);
        if rounded {
            path.line_to(x, y + h - cr);
            path.quadratic_curve_to(x, y + h, x + cr, y + h);
            path.line_to(x + w - cr, y + h);
            path.quadratic_curve_to(x + w, y + h, x + w, y + h - cr);
        } else {
            path.line_to(x, y + h);
            path.line_to(x + w, y + h);
        }
        path.line_to(x + w, y);
    }
    path.close_path();
    path
}
