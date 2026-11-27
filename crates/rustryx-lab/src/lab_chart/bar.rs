//! `ChartBar` (upstream lab `ChartBar.tsx`): vertical bars from the zero line, on a band x.

use gpui::prelude::*;
use gpui::{App, Bounds, Hsla, Pixels, SharedString, Window, canvas, div};

use super::context::{ChartContext, use_chart};
use super::paint::fill_rect;
use super::utils::{js_min, number_or_zero};

/// Bar marks (upstream lab `ChartBar`): a band x scale is required; bars grow from the zero
/// line, up for positive values and down for negative ones.
#[derive(IntoElement)]
pub struct ChartBar {
    data_key: SharedString,
    color: Hsla,
    radius: f64,
}

impl ChartBar {
    /// Upstream `dataKey` and `color` (the fill).
    pub fn new(data_key: impl Into<SharedString>, color: Hsla) -> Self {
        ChartBar {
            data_key: data_key.into(),
            color,
            radius: 4.,
        }
    }

    /// Upstream `radius`: the corners' radius (4).
    pub fn radius(mut self, radius: f64) -> Self {
        self.radius = radius;
        self
    }
}

/// One bar: its rectangle (x, y, width, height) and corner radius.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BarRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// The bars of a data key (upstream's `<rect>`s): none on a linear x; a datum whose
/// category the band scale lacks has none.
pub fn bar_geometry(ctx: &ChartContext, data_key: &str) -> Vec<BarRect> {
    let Some(band) = ctx.x_scale.as_band() else {
        return Vec::new();
    };
    let zero_y = ctx.y_scale.scale(0.);
    ctx.data
        .iter()
        .filter_map(|d| {
            let x = band.scale(&rustryx_charts::js_string(d.get(&ctx.x_key)))?;
            let y_pos = ctx.y_scale.scale(number_or_zero(d, data_key));
            let height = (y_pos - zero_y).abs();
            Some(BarRect {
                x,
                y: js_min(y_pos, zero_y),
                width: band.bandwidth(),
                height: if height.is_nan() {
                    f64::NAN
                } else {
                    height.max(0.)
                },
            })
        })
        .collect()
}

impl RenderOnce for ChartBar {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let Some(ctx) = use_chart(cx) else {
            return div().into_any_element();
        };
        let bars = bar_geometry(&ctx, &self.data_key);
        canvas(
            |_, _, _| {},
            move |bounds: Bounds<Pixels>, _, window, _| {
                for b in &bars {
                    fill_rect(
                        window,
                        bounds.origin,
                        (b.x, b.y, b.width, b.height),
                        self.radius,
                        self.color,
                        None,
                    );
                }
            },
        )
        .size_full()
        .into_any_element()
    }
}
