//! `line`: a line series (upstream `marks/line.tsx`). A missing or non-finite value
//! is a gap (d3's `defined`) rather than a drop to the baseline, and the optional dots
//! draw only at finite points.

use std::rc::Rc;

use astryx_theme::Tokens;
use gpui::{AnyElement, App, Hsla, SharedString, Window};

use super::paint::shapes_element;
use crate::mark_color::series_fill;
use crate::shape::{Curve, Line};
use crate::types::{
    ColorAccessor, GroupInfo, MarkShape, ResolvedPoint, Series, SeriesContext, SeriesDef,
    SeriesInfo, SeriesLayout, StackOffset,
};
use crate::utils::x_pixel;

/// How a line or area joins its points (upstream `CurveType`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CurveType {
    /// Straight segments, which measured data keeps so each value reads as a point.
    #[default]
    Linear,
    /// Smoothed values between the points, monotone in y.
    Monotone,
    /// A natural spline.
    Natural,
    Step,
}

impl CurveType {
    pub(crate) fn curve(self) -> Curve {
        match self {
            CurveType::Linear => Curve::Linear,
            CurveType::Monotone => Curve::MonotoneX,
            CurveType::Natural => Curve::Natural,
            CurveType::Step => Curve::Step,
        }
    }
}

/// A line series' options (upstream `LineOptions`).
#[derive(Clone, Debug, Default)]
pub struct LineOptions {
    pub color: Option<Hsla>,
    pub curve: CurveType,
    /// 2 by default.
    pub stroke_width: Option<f64>,
    /// A dot at each point.
    pub dots: bool,
    pub label: Option<SharedString>,
}

impl LineOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn color(mut self, color: Hsla) -> Self {
        self.color = Some(color);
        self
    }

    pub fn curve(mut self, curve: CurveType) -> Self {
        self.curve = curve;
        self
    }

    pub fn stroke_width(mut self, width: f64) -> Self {
        self.stroke_width = Some(width);
        self
    }

    pub fn dots(mut self, dots: bool) -> Self {
        self.dots = dots;
        self
    }

    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }
}

/// The dots' radius.
const DOT_RADIUS: f64 = 3.;

struct LineMark {
    key: SharedString,
    color: Option<Hsla>,
    curve: CurveType,
    stroke_width: f64,
    dots: bool,
    label: SharedString,
}

/// A line series of `data_key` (upstream `line`).
pub fn line(data_key: impl Into<SharedString>, options: LineOptions) -> SeriesDef {
    let key = data_key.into();
    Rc::new(LineMark {
        label: options.label.unwrap_or_else(|| key.clone()),
        key,
        color: options.color,
        curve: options.curve,
        stroke_width: options
            .stroke_width
            .filter(|w| w.is_finite())
            .map_or(2., |w| w.max(0.)),
        dots: options.dots,
    })
}

/// Both plotted coordinates are finite.
pub(crate) fn is_finite_point(p: &ResolvedPoint) -> bool {
    p.px.is_finite() && p.py.is_finite()
}

impl Series for LineMark {
    fn kind(&self) -> &'static str {
        "line"
    }

    fn key(&self) -> SharedString {
        self.key.clone()
    }

    fn data_keys(&self) -> Vec<SharedString> {
        vec![self.key.clone()]
    }

    fn color(&self) -> Option<Hsla> {
        self.color
    }

    fn label(&self) -> Option<SharedString> {
        Some(self.label.clone())
    }

    fn layout(&self) -> SeriesLayout {
        SeriesLayout {
            x_overhang: self.dots.then_some(DOT_RADIUS),
            ..SeriesLayout::default()
        }
    }

    fn resolve(
        &self,
        ctx: &SeriesContext,
        _: Option<&[StackOffset]>,
        _: Option<GroupInfo>,
    ) -> Vec<ResolvedPoint> {
        ctx.data
            .iter()
            .enumerate()
            .map(|(i, d)| ResolvedPoint {
                px: x_pixel(d, &ctx.x_key, &ctx.x_scale),
                py: ctx.y_scale.scale(d.number_or_nan(&self.key)),
                py0: ctx.y_scale.scale(0.),
                data_index: i,
            })
            .collect()
    }

    fn shapes(
        &self,
        resolved: &[ResolvedPoint],
        _: &SeriesContext,
        info: &SeriesInfo,
        tokens: &Tokens,
    ) -> Vec<MarkShape> {
        if resolved.is_empty() {
            return Vec::new();
        }
        let color = series_fill(
            info,
            self.color.map(ColorAccessor::Color).as_ref(),
            tokens.color.accent,
        );
        let path = Line::new()
            .defined(|p: &ResolvedPoint, _| is_finite_point(p))
            .x(|p: &ResolvedPoint, _| p.px)
            .y(|p: &ResolvedPoint, _| p.py)
            .curve(self.curve.curve())
            .generate(resolved);
        let mut shapes = vec![MarkShape::Stroke {
            path,
            color,
            width: self.stroke_width,
        }];
        if self.dots {
            shapes.extend(resolved.iter().filter(|p| is_finite_point(p)).map(|p| {
                MarkShape::Circle {
                    cx: p.px,
                    cy: p.py,
                    r: DOT_RADIUS,
                    color,
                    opacity: 1.,
                }
            }));
        }
        shapes
    }

    fn render(
        &self,
        resolved: &[ResolvedPoint],
        ctx: &SeriesContext,
        info: &SeriesInfo,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<AnyElement> {
        if resolved.is_empty() {
            return None;
        }
        let theme = astryx_theme::current(window, cx);
        Some(shapes_element(self.shapes(
            resolved,
            ctx,
            info,
            theme.tokens(),
        )))
    }
}
