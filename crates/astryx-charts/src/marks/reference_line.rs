//! `reference_line`: a horizontal or vertical reference line, or a shaded band
//! between two y values, with an optional label badge kept inside the plot (upstream
//! `marks/referenceLine.tsx`). A non-finite value draws nothing.

use std::rc::Rc;

use astryx_theme::{ThemeStyled, Tokens};
use gpui::prelude::*;
use gpui::{AnyElement, App, FontWeight, Hsla, SharedString, Window, div, px};

use super::paint::{clamp_js, shapes_element};
use crate::types::{
    GroupInfo, MarkShape, ResolvedPoint, Series, SeriesContext, SeriesDef, SeriesInfo, StackOffset,
};

/// The badge's height and corner radius, matching the crosshair label.
const BADGE_H: f64 = 14.;
const BADGE_RX: f64 = 3.;
const LABEL_FONT_SIZE: f64 = 10.;
/// A rough glyph advance and the badge's padding, to estimate its width.
const LABEL_CHAR_W: f64 = 6.5;
const LABEL_PAD: f64 = 12.;

/// Which end of the line the label sits at (upstream `labelPosition`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ReferenceLabelPosition {
    Start,
    #[default]
    End,
}

/// A reference line's options (upstream `ReferenceLineOptions`).
#[derive(Clone, Debug, Default)]
pub struct ReferenceLineOptions {
    /// A horizontal reference at this y.
    pub y: Option<f64>,
    /// A second y: a shaded band between `y` and it.
    pub y2: Option<f64>,
    /// A vertical reference at this x (a linear x only).
    pub x: Option<f64>,
    /// The emphasized border color by default.
    pub color: Option<Hsla>,
    /// 1 by default.
    pub stroke_width: Option<f64>,
    /// The dash pattern, `[6, 3]` by default (upstream `strokeDasharray`); empty for a
    /// solid line.
    pub stroke_dasharray: Option<Vec<f64>>,
    pub label: Option<SharedString>,
    pub label_position: ReferenceLabelPosition,
    /// The band's opacity, 0.1 by default.
    pub band_opacity: Option<f64>,
}

impl ReferenceLineOptions {
    /// A horizontal line at `y`.
    pub fn y(y: f64) -> Self {
        ReferenceLineOptions {
            y: Some(y),
            ..Self::default()
        }
    }

    /// A vertical line at `x`.
    pub fn x(x: f64) -> Self {
        ReferenceLineOptions {
            x: Some(x),
            ..Self::default()
        }
    }

    pub fn y2(mut self, y2: f64) -> Self {
        self.y2 = Some(y2);
        self
    }

    pub fn color(mut self, color: Hsla) -> Self {
        self.color = Some(color);
        self
    }

    pub fn stroke_width(mut self, width: f64) -> Self {
        self.stroke_width = Some(width);
        self
    }

    pub fn stroke_dasharray(mut self, dash: impl Into<Vec<f64>>) -> Self {
        self.stroke_dasharray = Some(dash.into());
        self
    }

    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn label_position(mut self, position: ReferenceLabelPosition) -> Self {
        self.label_position = position;
        self
    }

    pub fn band_opacity(mut self, opacity: f64) -> Self {
        self.band_opacity = Some(opacity);
        self
    }
}

struct ReferenceLine(ReferenceLineOptions);

/// A reference line or band (upstream `referenceLine`): an annotation at fixed values,
/// not a data series.
pub fn reference_line(options: ReferenceLineOptions) -> SeriesDef {
    Rc::new(ReferenceLine(options))
}

/// The label, when it shows: upstream draws a badge for a non-empty label only.
fn shown_label(label: Option<&SharedString>) -> Option<&SharedString> {
    label.filter(|l| !l.is_empty())
}

/// The badge's estimated width (0 without a label), upstream `labelWidth`.
fn label_width(label: Option<&SharedString>) -> f64 {
    shown_label(label).map_or(0., |l| {
        l.encode_utf16().count() as f64 * LABEL_CHAR_W + LABEL_PAD
    })
}

impl Series for ReferenceLine {
    fn kind(&self) -> &'static str {
        "referenceLine"
    }

    fn key(&self) -> SharedString {
        let at = self.0.y.or(self.0.x);
        match at {
            Some(v) => format!(
                "ref-{}",
                astryx_theme::authoring::js::js_number_to_string(v)
            )
            .into(),
            None => "ref-none".into(),
        }
    }

    fn data_keys(&self) -> Vec<SharedString> {
        Vec::new()
    }

    /// No points: the reference sits at fixed values.
    fn resolve(
        &self,
        _: &SeriesContext,
        _: Option<&[StackOffset]>,
        _: Option<GroupInfo>,
    ) -> Vec<ResolvedPoint> {
        Vec::new()
    }

    fn shapes(
        &self,
        _: &[ResolvedPoint],
        ctx: &SeriesContext,
        _: &SeriesInfo,
        tokens: &Tokens,
    ) -> Vec<MarkShape> {
        let o = &self.0;
        let color = o.color.unwrap_or(tokens.color.border_emphasized);
        let stroke_width = o.stroke_width.unwrap_or(1.);
        let dash = o.stroke_dasharray.clone().unwrap_or_else(|| vec![6., 3.]);
        let band_opacity = o.band_opacity.unwrap_or(0.1);
        let label = shown_label(o.label.as_ref());
        let text_w = label_width(label);
        let (width, height) = (ctx.width, ctx.height);
        // Keep the badge fully inside the (clipped) plot on both axes.
        let bx_max = width - text_w - 2.;
        let segment = |from, to| MarkShape::Segment {
            from,
            to,
            color,
            width: stroke_width,
            dash: dash.clone(),
        };
        let mut shapes = Vec::new();
        let badge: (f64, f64);
        if let Some(y) = o.y {
            // Horizontal reference line or band.
            let py = ctx.y_scale.scale(y);
            if !y.is_finite() || !py.is_finite() {
                return Vec::new();
            }
            let desired = if o.label_position == ReferenceLabelPosition::End {
                bx_max
            } else {
                2.
            };
            badge = (
                clamp_js(desired, 2., bx_max),
                clamp_js(py - BADGE_H / 2., 0., height - BADGE_H),
            );
            match o.y2.filter(|y2| y2.is_finite()) {
                // Band mode: the shaded region between y and y2.
                Some(y2) => {
                    let py2 = ctx.y_scale.scale(y2);
                    shapes.push(MarkShape::Rect {
                        x: 0.,
                        y: py.min(py2),
                        width,
                        height: (py2 - py).abs(),
                        color,
                        opacity: band_opacity,
                    });
                    shapes.push(segment((0., py), (width, py)));
                    shapes.push(segment((0., py2), (width, py2)));
                }
                None => shapes.push(segment((0., py), (width, py))),
            }
        } else {
            // A vertical reference needs a linear x: a band scale has no pixel for an
            // arbitrary number.
            let Some(x) = o.x.filter(|_| !ctx.x_scale.is_band()) else {
                return Vec::new();
            };
            let px = ctx.x_scale.as_linear().map_or(f64::NAN, |s| s.scale(x));
            if !x.is_finite() || !px.is_finite() {
                return Vec::new();
            }
            let desired = if o.label_position == ReferenceLabelPosition::End {
                4.
            } else {
                height - BADGE_H - 4.
            };
            badge = (
                clamp_js(px - text_w / 2., 2., bx_max),
                clamp_js(desired, 0., height - BADGE_H),
            );
            shapes.push(segment((px, 0.), (px, height)));
        }
        if let Some(text) = label {
            shapes.push(MarkShape::Badge {
                x: badge.0,
                y: badge.1,
                width: text_w,
                height: BADGE_H,
                text: text.clone(),
                color,
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
        let theme = astryx_theme::current(window, cx);
        let t = theme.tokens();
        let shapes = self.shapes(resolved, ctx, info, t);
        if shapes.is_empty() {
            return None;
        }
        let badges: Vec<AnyElement> = shapes
            .iter()
            .filter_map(|shape| {
                let MarkShape::Badge {
                    x,
                    y,
                    width,
                    height,
                    text,
                    color,
                } = shape
                else {
                    return None;
                };
                let mut fill = t.color.background_popover;
                fill.a *= 0.85;
                Some(
                    div()
                        .absolute()
                        .left(px(*x as f32))
                        .top(px(*y as f32))
                        .w(px(*width as f32))
                        .h(px(*height as f32))
                        .rounded(px(BADGE_RX as f32))
                        .bg(fill)
                        .border(px(0.5))
                        .border_color(*color)
                        .flex()
                        .items_center()
                        .justify_center()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .font_stack(&t.font.family.body)
                        .text_size(px(LABEL_FONT_SIZE as f32))
                        .line_height(px(BADGE_H as f32))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(*color)
                        .child(text.clone())
                        .into_any_element(),
                )
            })
            .collect();
        Some(
            div()
                .relative()
                .size_full()
                .child(div().absolute().size_full().child(shapes_element(shapes)))
                .children(badges)
                .into_any_element(),
        )
    }
}
