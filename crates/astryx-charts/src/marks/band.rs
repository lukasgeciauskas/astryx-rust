//! `band`: a confidence band, the area between an upper and a lower bound (upstream
//! `marks/band.tsx`). A missing bound is a gap, not a drop to the axis.

use std::rc::Rc;

use astryx_theme::Tokens;
use gpui::{AnyElement, App, Hsla, SharedString, Window};

use super::paint::shapes_element;
use crate::shape::{Area, Curve};
use crate::types::{
    GroupInfo, MarkShape, ResolvedPoint, Series, SeriesContext, SeriesDef, SeriesInfo, StackOffset,
};
use crate::utils::x_pixel;

/// A band's options (upstream `BandOptions`).
#[derive(Clone, Debug)]
pub struct BandOptions {
    /// The upper bound's data key.
    pub upper: SharedString,
    /// The lower bound's data key.
    pub lower: SharedString,
    /// The accent color by default.
    pub color: Option<Hsla>,
    /// 0.15 by default.
    pub opacity: Option<f64>,
}

impl BandOptions {
    pub fn new(upper: impl Into<SharedString>, lower: impl Into<SharedString>) -> Self {
        BandOptions {
            upper: upper.into(),
            lower: lower.into(),
            color: None,
            opacity: None,
        }
    }

    pub fn color(mut self, color: Hsla) -> Self {
        self.color = Some(color);
        self
    }

    pub fn opacity(mut self, opacity: f64) -> Self {
        self.opacity = Some(opacity);
        self
    }
}

struct BandMark(BandOptions);

/// A band between two bounds (upstream `band`).
pub fn band(options: BandOptions) -> SeriesDef {
    Rc::new(BandMark(options))
}

impl Series for BandMark {
    fn kind(&self) -> &'static str {
        "band"
    }

    fn key(&self) -> SharedString {
        format!("band-{}-{}", self.0.upper, self.0.lower).into()
    }

    fn data_keys(&self) -> Vec<SharedString> {
        vec![self.0.upper.clone(), self.0.lower.clone()]
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
                py: ctx.y_scale.scale(d.number_or_nan(&self.0.upper)),
                py0: ctx.y_scale.scale(d.number_or_nan(&self.0.lower)),
                data_index: i,
            })
            .collect()
    }

    fn shapes(
        &self,
        resolved: &[ResolvedPoint],
        _: &SeriesContext,
        _: &SeriesInfo,
        tokens: &Tokens,
    ) -> Vec<MarkShape> {
        if resolved.is_empty() {
            return Vec::new();
        }
        let path = Area::new()
            .defined(|p: &ResolvedPoint, _| {
                p.px.is_finite() && p.py.is_finite() && p.py0.is_finite()
            })
            .x(|p: &ResolvedPoint, _| p.px)
            .y0(|p: &ResolvedPoint, _| p.py0)
            .y1(|p: &ResolvedPoint, _| p.py)
            .curve(Curve::Linear)
            .generate(resolved);
        if path.is_empty() {
            return Vec::new();
        }
        vec![MarkShape::Fill {
            path,
            color: self.0.color.unwrap_or(tokens.color.accent),
            opacity: self.0.opacity.unwrap_or(0.15),
        }]
    }

    fn render(
        &self,
        resolved: &[ResolvedPoint],
        ctx: &SeriesContext,
        info: &SeriesInfo,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<AnyElement> {
        let theme = astryx_theme::current(window, cx);
        let shapes = self.shapes(resolved, ctx, info, theme.tokens());
        (!shapes.is_empty()).then(|| shapes_element(shapes))
    }
}
