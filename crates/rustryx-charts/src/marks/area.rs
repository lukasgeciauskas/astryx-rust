//! `area`: an area series, stacked or not, filled flat or with a fading gradient and
//! edged with its line (upstream `marks/area.tsx`). A missing value, a gap in a
//! stacked baseline included, is a hole rather than a drop to zero.

use std::rc::Rc;

use gpui::{AnyElement, App, Hsla, SharedString, Window};
use rustryx_theme::Tokens;

use super::line::CurveType;
use super::paint::shapes_element;
use crate::mark_color::series_fill;
use crate::shape::{Area, Line};
use crate::types::{
    ColorAccessor, GroupInfo, MarkShape, ResolvedPoint, Series, SeriesContext, SeriesDef,
    SeriesInfo, SeriesLayout, StackOffset,
};
use crate::utils::x_pixel;

/// An area series' options (upstream `AreaOptions`).
#[derive(Clone, Debug, Default)]
pub struct AreaOptions {
    pub color: Option<Hsla>,
    /// The fill's opacity, 0.3 by default.
    pub opacity: Option<f64>,
    pub curve: CurveType,
    /// Areas with the same stack accumulate.
    pub stack: Option<SharedString>,
    /// Fades the fill from `opacity` at the top to nothing at the bottom.
    pub gradient: bool,
    /// Draws the area's top line (on by default).
    pub stroke: Option<bool>,
    pub label: Option<SharedString>,
}

impl AreaOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn color(mut self, color: Hsla) -> Self {
        self.color = Some(color);
        self
    }

    pub fn opacity(mut self, opacity: f64) -> Self {
        self.opacity = Some(opacity);
        self
    }

    pub fn curve(mut self, curve: CurveType) -> Self {
        self.curve = curve;
        self
    }

    pub fn stack(mut self, stack: impl Into<SharedString>) -> Self {
        self.stack = Some(stack.into());
        self
    }

    pub fn gradient(mut self, gradient: bool) -> Self {
        self.gradient = gradient;
        self
    }

    pub fn stroke(mut self, stroke: bool) -> Self {
        self.stroke = Some(stroke);
        self
    }

    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }
}

struct AreaMark {
    key: SharedString,
    color: Option<Hsla>,
    opacity: f64,
    curve: CurveType,
    stack: Option<SharedString>,
    gradient: bool,
    stroke: bool,
    label: SharedString,
}

/// An area series of `data_key` (upstream `area`).
pub fn area(data_key: impl Into<SharedString>, options: AreaOptions) -> SeriesDef {
    let key = data_key.into();
    Rc::new(AreaMark {
        label: options.label.unwrap_or_else(|| key.clone()),
        key,
        color: options.color,
        opacity: super::clamp_opacity(options.opacity, 0.3),
        curve: options.curve,
        stack: options.stack,
        gradient: options.gradient,
        stroke: options.stroke.unwrap_or(true),
    })
}

/// Both edges of the point are finite.
fn is_finite_point(p: &ResolvedPoint) -> bool {
    p.px.is_finite() && p.py.is_finite() && p.py0.is_finite()
}

impl Series for AreaMark {
    fn kind(&self) -> &'static str {
        "area"
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
            stack: self.stack.clone(),
            include_zero: true,
            ..SeriesLayout::default()
        }
    }

    fn resolve(
        &self,
        ctx: &SeriesContext,
        stack: Option<&[StackOffset]>,
        _: Option<GroupInfo>,
    ) -> Vec<ResolvedPoint> {
        ctx.data
            .iter()
            .enumerate()
            .map(|(i, d)| {
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
                    px: x_pixel(d, &ctx.x_key, &ctx.x_scale),
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
        let curve = self.curve.curve();
        let path = Area::new()
            .defined(|p: &ResolvedPoint, _| is_finite_point(p))
            .x(|p: &ResolvedPoint, _| p.px)
            .y0(|p: &ResolvedPoint, _| p.py0)
            .y1(|p: &ResolvedPoint, _| p.py)
            .curve(curve)
            .generate(resolved);
        let mut shapes = vec![if self.gradient {
            MarkShape::GradientFill {
                path,
                color,
                opacity: self.opacity,
            }
        } else {
            MarkShape::Fill {
                path,
                color,
                opacity: self.opacity,
            }
        }];
        if self.stroke {
            let path = Line::new()
                .defined(|p: &ResolvedPoint, _| is_finite_point(p))
                .x(|p: &ResolvedPoint, _| p.px)
                .y(|p: &ResolvedPoint, _| p.py)
                .curve(curve)
                .generate(resolved);
            shapes.push(MarkShape::Stroke {
                path,
                color,
                width: 2.,
            });
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
        let theme = rustryx_theme::current(window, cx);
        Some(shapes_element(self.shapes(
            resolved,
            ctx,
            info,
            theme.tokens(),
        )))
    }
}
