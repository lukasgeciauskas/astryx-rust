//! `error_bar`: whiskers between a high and a low bound (upstream `marks/errorBar.tsx`).
//! The bounds are absolute, so an interval can be asymmetric; a point missing a bound
//! is skipped rather than drawn down to the axis.

use std::rc::Rc;

use gpui::{AnyElement, App, Hsla, SharedString, Window};
use rustryx_theme::Tokens;

use super::paint::shapes_element;
use crate::types::{
    GroupInfo, MarkShape, ResolvedPoint, Series, SeriesContext, SeriesDef, SeriesInfo, StackOffset,
};
use crate::utils::x_pixel;

/// An error bar series' options (upstream `ErrorBarOptions`).
#[derive(Clone, Debug)]
pub struct ErrorBarOptions {
    /// The upper bound's data key.
    pub high: SharedString,
    /// The lower bound's data key.
    pub low: SharedString,
    /// The stroke: the primary text color by default.
    pub color: Option<Hsla>,
    /// 1.5 by default.
    pub stroke_width: Option<f64>,
    /// The caps' width in pixels, 8 by default.
    pub cap_width: Option<f64>,
}

impl ErrorBarOptions {
    pub fn new(high: impl Into<SharedString>, low: impl Into<SharedString>) -> Self {
        ErrorBarOptions {
            high: high.into(),
            low: low.into(),
            color: None,
            stroke_width: None,
            cap_width: None,
        }
    }

    pub fn color(mut self, color: Hsla) -> Self {
        self.color = Some(color);
        self
    }

    pub fn stroke_width(mut self, width: f64) -> Self {
        self.stroke_width = Some(width);
        self
    }

    pub fn cap_width(mut self, width: f64) -> Self {
        self.cap_width = Some(width);
        self
    }
}

struct ErrorBar(ErrorBarOptions);

/// Error bars with caps (upstream `errorBar`); pair them with a bar or dot series.
pub fn error_bar(options: ErrorBarOptions) -> SeriesDef {
    Rc::new(ErrorBar(options))
}

impl Series for ErrorBar {
    fn kind(&self) -> &'static str {
        "errorBar"
    }

    fn key(&self) -> SharedString {
        format!("errorBar-{}-{}", self.0.high, self.0.low).into()
    }

    fn data_keys(&self) -> Vec<SharedString> {
        vec![self.0.high.clone(), self.0.low.clone()]
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
                py: ctx.y_scale.scale(d.number_or_nan(&self.0.high)),
                py0: ctx.y_scale.scale(d.number_or_nan(&self.0.low)),
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
        let color = self.0.color.unwrap_or(tokens.color.text_primary);
        let width = self.0.stroke_width.unwrap_or(1.5);
        let cap = self.0.cap_width.unwrap_or(8.);
        // A non-finite cap width (NaN, ±Infinity) would leak into the caps' x: treat it
        // like a non-positive width, drawing the stem without caps.
        let half = if cap.is_finite() {
            cap.max(0.) / 2.
        } else {
            0.
        };
        let segment = |from, to| MarkShape::Segment {
            from,
            to,
            color,
            width,
            dash: Vec::new(),
        };
        resolved
            .iter()
            .filter(|p| p.px.is_finite() && p.py.is_finite() && p.py0.is_finite())
            .flat_map(|p| {
                [
                    // The stem, then the upper and lower caps.
                    segment((p.px, p.py), (p.px, p.py0)),
                    segment((p.px - half, p.py), (p.px + half, p.py)),
                    segment((p.px - half, p.py0), (p.px + half, p.py0)),
                ]
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
        let theme = rustryx_theme::current(window, cx);
        Some(shapes_element(self.shapes(
            resolved,
            ctx,
            info,
            theme.tokens(),
        )))
    }
}
