//! `ChartCandlestick` (upstream lab `ChartCandlestick.tsx`): candlestick and box-whisker
//! marks for OHLC or statistical data, on a band x.

use gpui::prelude::*;
use gpui::{App, Bounds, Hsla, Pixels, SharedString, Window, canvas, div};

use super::context::{ChartContext, use_chart};
use super::error_bar::Segment;
use super::paint::{fill_rect, stroke_segment};
use super::utils::{js_max, js_min, number_or_zero};

/// The candlestick's look (upstream `variant`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum CandlestickVariant {
    /// `'default'`: a filled body with whiskers.
    #[default]
    Default,
    /// `'bar'`: an OHLC bar, a high-to-low line with an open tick left and a close tick right.
    Bar,
}

/// Candlestick and box-whisker marks (upstream lab `ChartCandlestick`).
#[derive(IntoElement)]
pub struct ChartCandlestick {
    high: SharedString,
    low: SharedString,
    open: SharedString,
    close: SharedString,
    variant: CandlestickVariant,
    up_color: Option<Hsla>,
    down_color: Option<Hsla>,
    color: Option<Hsla>,
    body_width: f64,
    stroke_width: f64,
    radius: f64,
}

impl ChartCandlestick {
    /// Upstream `high`, `low`, `open` and `close`: the keys of the whisker's top and bottom
    /// and the body's ends (open or Q1, close or Q3).
    pub fn new(
        high: impl Into<SharedString>,
        low: impl Into<SharedString>,
        open: impl Into<SharedString>,
        close: impl Into<SharedString>,
    ) -> Self {
        ChartCandlestick {
            high: high.into(),
            low: low.into(),
            open: open.into(),
            close: close.into(),
            variant: CandlestickVariant::Default,
            up_color: None,
            down_color: None,
            color: None,
            body_width: 0.6,
            stroke_width: 1.5,
            radius: 2.,
        }
    }

    /// Upstream `variant`.
    pub fn variant(mut self, variant: CandlestickVariant) -> Self {
        self.variant = variant;
        self
    }

    /// Upstream `upColor`: a rising candle's (close at or above open), the categorical green
    /// data color by default.
    pub fn up_color(mut self, color: Hsla) -> Self {
        self.up_color = Some(color);
        self
    }

    /// Upstream `downColor`: a falling candle's, the categorical red data color by default.
    pub fn down_color(mut self, color: Hsla) -> Self {
        self.down_color = Some(color);
        self
    }

    /// Upstream `color`: one color for every candle, over the up and down colors (box plots).
    pub fn color(mut self, color: Hsla) -> Self {
        self.color = Some(color);
        self
    }

    /// Upstream `bodyWidth`: the body's share of the band (0.6).
    pub fn body_width(mut self, width: f64) -> Self {
        self.body_width = width;
        self
    }

    /// Upstream `strokeWidth`: the whiskers' and bar lines' width (1.5).
    pub fn stroke_width(mut self, width: f64) -> Self {
        self.stroke_width = width;
        self
    }

    /// Upstream `radius`: the body's corner radius (2, default variant only).
    pub fn radius(mut self, radius: f64) -> Self {
        self.radius = radius;
        self
    }
}

/// One candle: whether it rose, its center, its body's top, bottom, left and width, and
/// the high, low, open and close pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Candle {
    pub is_up: bool,
    pub center_x: f64,
    pub y_high: f64,
    pub y_low: f64,
    pub y_open: f64,
    pub y_close: f64,
    pub body_top: f64,
    pub body_bottom: f64,
    pub band_width: f64,
}

impl Candle {
    /// The default variant's whiskers (high to the body, the body to low) and body
    /// rectangle (x, y, width, height at least 1).
    pub fn default_shapes(&self, body_width: f64) -> ([Segment; 2], (f64, f64, f64, f64)) {
        let body_w = self.band_width * body_width;
        let body_x = self.center_x - body_w / 2.;
        let body_h = js_max(1., self.body_bottom - self.body_top);
        (
            [
                ((self.center_x, self.y_high), (self.center_x, self.body_top)),
                (
                    (self.center_x, self.body_bottom),
                    (self.center_x, self.y_low),
                ),
            ],
            (body_x, self.body_top, body_w, body_h),
        )
    }

    /// The bar variant's lines: high to low, the open tick to the left and the close tick
    /// to the right.
    pub fn bar_lines(&self, body_width: f64) -> [Segment; 3] {
        let tick = self.band_width * body_width * 0.4;
        [
            ((self.center_x, self.y_high), (self.center_x, self.y_low)),
            (
                (self.center_x - tick, self.y_open),
                (self.center_x, self.y_open),
            ),
            (
                (self.center_x, self.y_close),
                (self.center_x + tick, self.y_close),
            ),
        ]
    }
}

/// The candles (none on a linear x; a datum whose category the band lacks has none).
pub fn candlestick_geometry(
    ctx: &ChartContext,
    high: &str,
    low: &str,
    open: &str,
    close: &str,
) -> Vec<Candle> {
    let Some(band) = ctx.x_scale.as_band() else {
        return Vec::new();
    };
    let bw = band.bandwidth();
    ctx.data
        .iter()
        .filter_map(|d| {
            let x = band.scale(&astryx_charts::js_string(d.get(&ctx.x_key)))?;
            let (h, l, o, c) = (
                number_or_zero(d, high),
                number_or_zero(d, low),
                number_or_zero(d, open),
                number_or_zero(d, close),
            );
            let (y_open, y_close) = (ctx.y_scale.scale(o), ctx.y_scale.scale(c));
            Some(Candle {
                is_up: c >= o,
                center_x: x + bw / 2.,
                y_high: ctx.y_scale.scale(h),
                y_low: ctx.y_scale.scale(l),
                y_open,
                y_close,
                body_top: js_min(y_open, y_close),
                body_bottom: js_max(y_open, y_close),
                band_width: bw,
            })
        })
        .collect()
}

impl RenderOnce for ChartCandlestick {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let Some(ctx) = use_chart(cx) else {
            return div().into_any_element();
        };
        let t = astryx_theme::current(window, cx).tokens().clone();
        let up = self.up_color.unwrap_or(t.color.data_categorical_green);
        let down = self.down_color.unwrap_or(t.color.data_categorical_red);
        let candles = candlestick_geometry(&ctx, &self.high, &self.low, &self.open, &self.close);
        canvas(
            |_, _, _| {},
            move |bounds: Bounds<Pixels>, _, window, _| {
                let o = bounds.origin;
                for candle in &candles {
                    let fill = self.color.unwrap_or(if candle.is_up { up } else { down });
                    match self.variant {
                        CandlestickVariant::Bar => {
                            for (from, to) in candle.bar_lines(self.body_width) {
                                stroke_segment(window, o, from, to, self.stroke_width, fill, &[]);
                            }
                        }
                        CandlestickVariant::Default => {
                            let (whiskers, body) = candle.default_shapes(self.body_width);
                            for (from, to) in whiskers {
                                stroke_segment(window, o, from, to, self.stroke_width, fill, &[]);
                            }
                            fill_rect(window, o, body, self.radius, fill, None);
                        }
                    }
                }
            },
        )
        .size_full()
        .into_any_element()
    }
}
