//! `ChartReferenceLine` (upstream lab `ChartReferenceLine.tsx`): a horizontal or vertical
//! dashed line with a crosshair-style badge label.

use gpui::prelude::*;
use gpui::{App, Bounds, FontWeight, Hsla, Pixels, SharedString, Window, canvas, div};

use super::context::{ChartContext, use_chart};
use super::paint::{Anchor, Baseline, fill_rect, paint_text, parse_dash, stroke_segment};

/// Where the badge sits along the line (upstream `labelPosition`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ReferenceLabelPosition {
    /// `'start'`: the left end of a horizontal line, the bottom of a vertical one.
    Start,
    /// `'end'`: the right end of a horizontal line, the top of a vertical one (the default).
    #[default]
    End,
}

/// A reference line (upstream lab `ChartReferenceLine`): at `y` across the plot, or at `x`
/// down it on a linear x scale.
#[derive(IntoElement)]
pub struct ChartReferenceLine {
    y: Option<f64>,
    x: Option<f64>,
    color: Option<Hsla>,
    stroke_width: f64,
    stroke_dasharray: SharedString,
    label: Option<SharedString>,
    label_position: ReferenceLabelPosition,
}

impl Default for ChartReferenceLine {
    fn default() -> Self {
        Self::new()
    }
}

impl ChartReferenceLine {
    /// A reference line with neither `y` nor `x` yet (which draws nothing).
    pub fn new() -> Self {
        ChartReferenceLine {
            y: None,
            x: None,
            color: None,
            stroke_width: 1.,
            stroke_dasharray: "6 3".into(),
            label: None,
            label_position: ReferenceLabelPosition::End,
        }
    }

    /// Upstream `y`: a horizontal line at this value (it wins over `x`).
    pub fn y(mut self, y: f64) -> Self {
        self.y = Some(y);
        self
    }

    /// Upstream `x`: a vertical line at this value, on a linear x scale only.
    pub fn x(mut self, x: f64) -> Self {
        self.x = Some(x);
        self
    }

    /// Upstream `color` (the emphasized border color).
    pub fn color(mut self, color: Hsla) -> Self {
        self.color = Some(color);
        self
    }

    /// Upstream `strokeWidth` (1).
    pub fn stroke_width(mut self, width: f64) -> Self {
        self.stroke_width = width;
        self
    }

    /// Upstream `strokeDasharray` ("6 3").
    pub fn stroke_dasharray(mut self, dash: impl Into<SharedString>) -> Self {
        self.stroke_dasharray = dash.into();
        self
    }

    /// Upstream `label`: the badge's text.
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Upstream `labelPosition`.
    pub fn label_position(mut self, position: ReferenceLabelPosition) -> Self {
        self.label_position = position;
        self
    }
}

/// The badge's height, corner radius and font size (upstream's crosshair label style).
pub(crate) const BADGE_HEIGHT: f64 = 14.;
const BADGE_RADIUS: f64 = 3.;
const BADGE_FONT_SIZE: f64 = 10.;

/// A badge's rectangle (x, y, width, height) and its text's center.
pub type Badge = ((f64, f64, f64, f64), (f64, f64));

/// A reference line's geometry: its segment, and its badge when it has a label.
#[derive(Clone, Debug, PartialEq)]
pub struct ReferenceLineGeometry {
    pub from: (f64, f64),
    pub to: (f64, f64),
    pub badge: Option<Badge>,
}

/// Where the line and its badge go (upstream's two branches): the badge is the label's
/// length × 5.5 + 8 wide. `None` without `y`, or for `x` on a band scale.
pub fn reference_line_geometry(
    ctx: &ChartContext,
    y: Option<f64>,
    x: Option<f64>,
    label: Option<&str>,
    position: ReferenceLabelPosition,
) -> Option<ReferenceLineGeometry> {
    let text_w = label.map_or(0., |l| l.encode_utf16().count() as f64 * 5.5 + 8.);
    if let Some(y) = y {
        let py = ctx.y_scale.scale(y);
        let bx = match position {
            ReferenceLabelPosition::End => ctx.width - text_w - 2.,
            ReferenceLabelPosition::Start => 2.,
        };
        return Some(ReferenceLineGeometry {
            from: (0., py),
            to: (ctx.width, py),
            badge: label.map(|_| {
                (
                    (bx, py - BADGE_HEIGHT / 2., text_w, BADGE_HEIGHT),
                    (bx + text_w / 2., py),
                )
            }),
        });
    }
    let x = x?;
    let linear = ctx.x_scale.as_linear()?;
    let px = linear.scale(x);
    let by = match position {
        ReferenceLabelPosition::End => 4.,
        ReferenceLabelPosition::Start => ctx.height - BADGE_HEIGHT - 4.,
    };
    Some(ReferenceLineGeometry {
        from: (px, 0.),
        to: (px, ctx.height),
        badge: label.map(|_| {
            let bx = px - text_w / 2.;
            (
                (bx, by, text_w, BADGE_HEIGHT),
                (bx + text_w / 2., by + BADGE_HEIGHT / 2.),
            )
        }),
    })
}

impl RenderOnce for ChartReferenceLine {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let Some(ctx) = use_chart(cx) else {
            return div().into_any_element();
        };
        let t = astryx_theme::current(window, cx).tokens().clone();
        let color = self.color.unwrap_or(t.color.border_emphasized);
        let Some(geometry) = reference_line_geometry(
            &ctx,
            self.y,
            self.x,
            self.label.as_deref(),
            self.label_position,
        ) else {
            return div().into_any_element();
        };
        let dash = parse_dash(&self.stroke_dasharray);
        let popover = t.color.background_popover;
        let label = self.label.clone();
        canvas(
            |_, _, _| {},
            move |bounds: Bounds<Pixels>, _, window, cx| {
                let o = bounds.origin;
                stroke_segment(
                    window,
                    o,
                    geometry.from,
                    geometry.to,
                    self.stroke_width,
                    color,
                    &dash,
                );
                if let (Some((rect, center)), Some(label)) = (geometry.badge, label.as_ref()) {
                    fill_rect(
                        window,
                        o,
                        rect,
                        BADGE_RADIUS,
                        popover.opacity(0.85),
                        Some((0.5, color)),
                    );
                    paint_text(
                        window,
                        cx,
                        o,
                        label,
                        center,
                        Anchor::Middle,
                        Baseline::Em(0.35),
                        BADGE_FONT_SIZE,
                        Some(FontWeight::MEDIUM),
                        color,
                    );
                }
            },
        )
        .size_full()
        .into_any_element()
    }
}
