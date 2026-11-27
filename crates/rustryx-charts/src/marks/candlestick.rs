//! `candlestick`: an open/high/low/close series (upstream `marks/candlestick.tsx`), on
//! a band or a linear x; a candle missing a field is skipped rather than drawn to 0.

use std::rc::Rc;

use gpui::{AnyElement, App, Hsla, SharedString, Window};
use rustryx_theme::Tokens;

use super::paint::shapes_element;
use crate::Datum;
use crate::types::{
    ChartScale, GroupInfo, MarkShape, ResolvedPoint, Series, SeriesContext, SeriesDef, SeriesInfo,
    StackOffset,
};
use crate::utils::x_pixel;

/// A candlestick series' options (upstream `CandlestickOptions`): the four data keys
/// and the colors of rising and falling candles.
#[derive(Clone, Debug)]
pub struct CandlestickOptions {
    pub open: SharedString,
    pub high: SharedString,
    pub low: SharedString,
    pub close: SharedString,
    /// A rising candle's color: the success token by default.
    pub up_color: Option<Hsla>,
    /// A falling candle's color: the error token by default.
    pub down_color: Option<Hsla>,
}

impl CandlestickOptions {
    pub fn new(
        open: impl Into<SharedString>,
        high: impl Into<SharedString>,
        low: impl Into<SharedString>,
        close: impl Into<SharedString>,
    ) -> Self {
        CandlestickOptions {
            open: open.into(),
            high: high.into(),
            low: low.into(),
            close: close.into(),
            up_color: None,
            down_color: None,
        }
    }

    pub fn up_color(mut self, color: Hsla) -> Self {
        self.up_color = Some(color);
        self
    }

    pub fn down_color(mut self, color: Hsla) -> Self {
        self.down_color = Some(color);
        self
    }
}

struct Candlestick(CandlestickOptions);

/// A candlestick series (upstream `candlestick`).
pub fn candlestick(options: CandlestickOptions) -> SeriesDef {
    Rc::new(Candlestick(options))
}

/// A field as a finite number, else NaN (upstream `readNumber`).
fn read_number(d: &Datum, key: &str) -> f64 {
    d.number_or_nan(key)
}

/// One candle body's slot (upstream `candleSlot`): the bandwidth, else the smallest
/// gap between neighbouring points, else a modest width.
fn candle_slot(resolved: &[ResolvedPoint], ctx: &SeriesContext) -> f64 {
    if let ChartScale::Band(band) = &ctx.x_scale {
        return band.bandwidth();
    }
    let mut xs: Vec<f64> = resolved
        .iter()
        .map(|p| p.px)
        .filter(|x| x.is_finite())
        .collect();
    xs.sort_by(f64::total_cmp);
    let min_gap = xs
        .windows(2)
        .map(|w| w[1] - w[0])
        .filter(|gap| *gap > 0.)
        .fold(f64::INFINITY, f64::min);
    if min_gap.is_finite() {
        min_gap
    } else {
        ctx.width.min(40.)
    }
}

impl Series for Candlestick {
    fn kind(&self) -> &'static str {
        "candlestick"
    }

    fn key(&self) -> SharedString {
        format!("ohlc-{}", self.0.close).into()
    }

    fn data_keys(&self) -> Vec<SharedString> {
        vec![
            self.0.open.clone(),
            self.0.high.clone(),
            self.0.low.clone(),
            self.0.close.clone(),
        ]
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
                py: ctx.y_scale.scale(read_number(d, &self.0.close)),
                py0: ctx.y_scale.scale(0.),
                data_index: i,
            })
            .collect()
    }

    fn shapes(
        &self,
        resolved: &[ResolvedPoint],
        ctx: &SeriesContext,
        _: &SeriesInfo,
        tokens: &Tokens,
    ) -> Vec<MarkShape> {
        let up = self.0.up_color.unwrap_or(tokens.color.success);
        let down = self.0.down_color.unwrap_or(tokens.color.error);
        let body_width = (candle_slot(resolved, ctx) * 0.6).max(1.);
        let y = &ctx.y_scale;
        let mut shapes = Vec::new();
        for p in resolved {
            let Some(d) = ctx.data.get(p.data_index) else {
                continue;
            };
            let o = read_number(d, &self.0.open);
            let c = read_number(d, &self.0.close);
            let h = read_number(d, &self.0.high);
            let l = read_number(d, &self.0.low);
            // Drop candles missing any OHLC field (or an unplaceable x): a gap reads
            // more honestly than a full-height candle anchored at 0.
            if ![p.px, o, c, h, l].iter().all(|v| v.is_finite()) {
                continue;
            }
            let color = if c >= o { up } else { down };
            shapes.push(MarkShape::Segment {
                from: (p.px, y.scale(h)),
                to: (p.px, y.scale(l)),
                color,
                width: 1.,
                dash: Vec::new(),
            });
            shapes.push(MarkShape::Rect {
                x: p.px - body_width / 2.,
                y: y.scale(o.max(c)),
                width: body_width,
                height: (y.scale(o) - y.scale(c)).abs().max(1.),
                color,
                opacity: 1.,
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
        let theme = rustryx_theme::current(window, cx);
        Some(shapes_element(self.shapes(
            resolved,
            ctx,
            info,
            theme.tokens(),
        )))
    }
}
