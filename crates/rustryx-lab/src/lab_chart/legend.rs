//! `ChartLegend` (upstream lab `ChartLegend.tsx`): discrete swatches with labels, or a
//! continuous gradient bar with tick labels.

use std::rc::Rc;

use gpui::prelude::*;
use gpui::{
    App, Bounds, Corners, FontWeight, Hsla, Pixels, SharedString, Window, canvas, div,
    linear_color_stop, linear_gradient, px,
};
use rustryx_theme::ThemeStyled;

use super::context::use_chart;

/// One discrete item (upstream `ChartLegendItem`).
#[derive(Clone, Debug, PartialEq)]
pub struct ChartLegendItem {
    pub label: SharedString,
    pub color: Hsla,
}

impl ChartLegendItem {
    /// Upstream `label` and `color`.
    pub fn new(label: impl Into<SharedString>, color: Hsla) -> Self {
        ChartLegendItem {
            label: label.into(),
            color,
        }
    }
}

/// A gradient tick's text (upstream `tickFormat`).
pub type LegendTickFormat = Rc<dyn Fn(f64) -> SharedString>;

/// A legend (upstream lab `ChartLegend`): `items` make swatches; `gradient` with `domain`
/// makes a color bar with `ticks` labels.
#[derive(IntoElement)]
pub struct ChartLegend {
    items: Vec<ChartLegendItem>,
    gradient: Vec<Hsla>,
    domain: Option<[f64; 2]>,
    label: Option<SharedString>,
    ticks: usize,
    tick_format: Option<LegendTickFormat>,
}

impl Default for ChartLegend {
    fn default() -> Self {
        Self::new()
    }
}

impl ChartLegend {
    /// An empty legend (which draws nothing).
    pub fn new() -> Self {
        ChartLegend {
            items: Vec::new(),
            gradient: Vec::new(),
            domain: None,
            label: None,
            ticks: 3,
            tick_format: None,
        }
    }

    /// Upstream `items`: the discrete swatches.
    pub fn items(mut self, items: impl IntoIterator<Item = ChartLegendItem>) -> Self {
        self.items = items.into_iter().collect();
        self
    }

    /// Upstream `gradient`: the bar's colors from low to high (a sequential or diverging
    /// palette).
    pub fn gradient(mut self, colors: impl IntoIterator<Item = Hsla>) -> Self {
        self.gradient = colors.into_iter().collect();
        self
    }

    /// Upstream `domain`: the values the gradient spans, which it needs.
    pub fn domain(mut self, min: f64, max: f64) -> Self {
        self.domain = Some([min, max]);
        self
    }

    /// Upstream `label`: the gradient's title.
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Upstream `ticks`: how many labels under the gradient (3).
    pub fn ticks(mut self, ticks: usize) -> Self {
        self.ticks = ticks;
        self
    }

    /// Upstream `tickFormat` (`String` by default).
    pub fn tick_format(mut self, format: impl Fn(f64) -> SharedString + 'static) -> Self {
        self.tick_format = Some(Rc::new(format));
        self
    }
}

/// The gradient's tick values (upstream's `tickValues`): `ticks` values evenly from min to
/// max.
pub fn gradient_ticks(domain: [f64; 2], ticks: usize) -> Vec<f64> {
    let [min, max] = domain;
    (0..ticks)
        .map(|i| min + (i as f64 / (ticks as f64 - 1.)) * (max - min))
        .collect()
}

/// Paints `colors` as a horizontal gradient bar with rounded ends (upstream's CSS
/// `linear-gradient(to right, …)` with evenly spaced stops).
fn paint_gradient(window: &mut Window, bounds: Bounds<Pixels>, colors: &[Hsla], radius: Pixels) {
    match colors {
        [] => {}
        [one] => window.paint_quad(gpui::fill(bounds, *one).corner_radii(radius)),
        _ => {
            let n = colors.len() - 1;
            let w = bounds.size.width / n as f32;
            for i in 0..n {
                let seg = Bounds::new(
                    bounds.origin + gpui::point(w * i as f32, px(0.)),
                    gpui::size(w + px(0.5), bounds.size.height),
                );
                let mut corners = Corners::default();
                if i == 0 {
                    corners.top_left = radius;
                    corners.bottom_left = radius;
                }
                if i == n - 1 {
                    corners.top_right = radius;
                    corners.bottom_right = radius;
                }
                let fill = linear_gradient(
                    90.,
                    linear_color_stop(colors[i], 0.),
                    linear_color_stop(colors[i + 1], 1.),
                );
                window.paint_quad(gpui::fill(seg, fill).corner_radii(corners));
            }
        }
    }
}

impl RenderOnce for ChartLegend {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = rustryx_theme::current(window, cx).tokens().clone();
        let chart = use_chart(cx);
        // Inside a chart, upstream's foreignObject sits 4 px above the plot's top, as wide
        // as the whole chart (its "100%"), and overflows it.
        let place = |el: gpui::Div, height: f32| match &chart {
            Some(c) => el
                .absolute()
                .left_0()
                .top(px(-4.))
                .w(px((c.width + c.margin.left + c.margin.right) as f32))
                .h(px(height)),
            None => el.w_full(),
        };
        if let Some(domain) = self.domain.filter(|_| !self.gradient.is_empty()) {
            let format = self.tick_format.clone();
            let labels = gradient_ticks(domain, self.ticks)
                .into_iter()
                .map(move |v| {
                    let text = match &format {
                        Some(f) => f(v),
                        None => rustryx_theme::authoring::js::js_number_to_string(v).into(),
                    };
                    div().text_size(px(10.)).child(text)
                });
            let colors = self.gradient.clone();
            let bar = canvas(
                |_, _, _| {},
                move |bounds, _, window, _| paint_gradient(window, bounds, &colors, px(4.)),
            )
            .w_full()
            .h(px(10.));
            let body = div()
                .flex()
                .flex_col()
                .items_center()
                .gap(px(4.))
                .font_stack(&t.font.family.body)
                .text_size(px(12.))
                .text_color(t.color.text_secondary)
                .children(self.label.clone().map(|l| {
                    div()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(t.color.text_primary)
                        .child(l)
                }))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(2.))
                        .w(px(200.))
                        .child(bar)
                        .child(div().flex().justify_between().children(labels)),
                );
            return place(div(), 48.).child(body).into_any_element();
        }
        if self.items.is_empty() {
            return div().into_any_element();
        }
        let row = div()
            .flex()
            .gap(px(16.))
            .justify_center()
            .font_stack(&t.font.family.body)
            .text_size(px(12.))
            .text_color(t.color.text_secondary)
            .children(self.items.into_iter().map(|item| {
                div()
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .child(
                        div()
                            .size(px(10.))
                            .rounded(px(2.))
                            .bg(item.color)
                            .flex_shrink_0(),
                    )
                    .child(item.label)
            }));
        place(div(), 24.).child(row).into_any_element()
    }
}
