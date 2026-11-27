//! The chart gallery's examples (upstream's chart stories): bars on a band scale,
//! grouped and stacked bars, lines and areas with their curves, a scatter, candles,
//! error bars and annotations, the legend's positions and the tooltip.

use std::rc::Rc;

use astryx_components::layout::{SpacingStep, Stack};
use astryx_components::text::{Text, TextType};
use std::time::Duration;

use gpui::{AnyElement, App, IntoElement, ParentElement, Styled, Task, Window, div, px, rgb};

use crate::marks::{
    AreaOptions, BandOptions, BarOptions, CandlestickOptions, CurveType, DotGlInteractiveOptions,
    DotGlOptions, DotOptions, ErrorBarOptions, HeatmapGlOptions, LineOptions, ReferenceLineOptions,
    StreamGlHandle, StreamGlOptions, area, band, bar, candlestick, dot, dot_gl, dot_gl_interactive,
    error_bar, heatmap_gl, line, reference_line, stream_gl,
};
use crate::{AxisPosition, Chart, ChartAxis, ChartGrid, ChartLegend, Datum, LegendPosition, datum};

/// Upstream's `monthlyData`.
fn monthly() -> Rc<[Datum]> {
    [
        ("Jan", 45, 30, 38),
        ("Feb", 52, 35, 42),
        ("Mar", 48, 32, 40),
        ("Apr", 61, 38, 48),
        ("May", 55, 34, 45),
        ("Jun", 70, 40, 52),
    ]
    .into_iter()
    .map(|(month, revenue, costs, trend)| datum! { "month" => month, "revenue" => revenue, "costs" => costs, "trend" => trend })
    .collect()
}

/// Upstream's `salesData`.
fn sales() -> Rc<[Datum]> {
    [
        ("Jan", 45, 48, 42),
        ("Feb", 52, 56, 48),
        ("Mar", 48, 52, 44),
        ("Apr", 61, 66, 56),
        ("May", 55, 59, 51),
        ("Jun", 70, 76, 64),
    ]
    .into_iter()
    .map(|(month, sales, high, low)| datum! { "month" => month, "sales" => sales, "errorHigh" => high, "errorLow" => low })
    .collect()
}

/// The trend with a band six either side (upstream's Chart story `trendBand`).
fn banded() -> Rc<[Datum]> {
    monthly()
        .iter()
        .map(|d| {
            let trend = d.finite("trend").unwrap_or(0.);
            d.clone().with("high", trend + 6.).with("low", trend - 6.)
        })
        .collect()
}

/// Upstream's seeded generator (mulberry32), so every load draws the same data.
fn seeded_random(seed: u32) -> impl FnMut() -> f64 {
    let mut state = seed;
    move || {
        state = state.wrapping_add(0x6d2b_79f5);
        let mut t = state;
        t = (t ^ (t >> 15)).wrapping_mul(t | 1);
        t ^= t.wrapping_add((t ^ (t >> 7)).wrapping_mul(t | 61));
        f64::from(t ^ (t >> 14)) / 4_294_967_296.
    }
}

/// Upstream's `scatterData`: 200 seeded points.
fn scatter() -> Rc<[Datum]> {
    let mut random = seeded_random(2);
    (0..200)
        .map(|_| {
            let x = random() * 100.;
            let y = random() * 100.;
            datum! { "x" => x, "y" => y }
        })
        .collect()
}

/// Two weeks of daily prices.
fn prices() -> Rc<[Datum]> {
    let mut random = seeded_random(7);
    let mut close = 100.;
    (1..=14)
        .map(|day| {
            let open: f64 = close;
            close = open + (random() - 0.48) * 8.;
            let high = open.max(close) + random() * 3.;
            let low = open.min(close) - random() * 3.;
            datum! { "day" => format!("Jun {day}"), "open" => open, "high" => high, "low" => low, "close" => close }
        })
        .collect()
}

/// Upstream's `heatmapData`: a week of hourly traffic from a seeded generator.
fn heatmap() -> Rc<[Datum]> {
    let mut random = seeded_random(3);
    let days = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
    let mut rows = Vec::new();
    for day in days {
        for hour in 0..24 {
            let weekend = if day == "Sat" || day == "Sun" {
                -20.
            } else {
                10.
            };
            let traffic =
                (50. + (f64::from(hour) / 4.).sin() * 30. + weekend + random() * 20.).round();
            rows.push(datum! { "hour" => hour.to_string(), "day" => day, "traffic" => traffic });
        }
    }
    rows.into()
}

/// A noisy sine fed into a stream chart every frame, for as long as it is drawn
/// (upstream's `createStreamSamples`).
struct Feed {
    handle: StreamGlHandle,
    _task: Task<()>,
}

fn axes() -> [ChartAxis; 2] {
    [
        ChartAxis::new(AxisPosition::Bottom),
        ChartAxis::new(AxisPosition::Left),
    ]
}

fn example(title: &'static str, chart: impl IntoElement) -> AnyElement {
    Stack::vertical()
        .gap(SpacingStep::S2)
        .child(Text::new(title).text_type(TextType::Large))
        .child(div().w_full().max_w(px(720.)).child(chart))
        .into_any_element()
}

/// The chart gallery.
pub fn examples(window: &mut Window, cx: &mut App) -> AnyElement {
    let showcase = Chart::new("chart-showcase", monthly(), "month")
        .series([bar("revenue", BarOptions::new().label("Revenue"))])
        .tooltip(true)
        .grid(ChartGrid::new())
        .axes(axes())
        .height(260.);

    let grouped = Chart::new("chart-grouped", monthly(), "month")
        .title("Revenue and costs")
        .subtitle("Grouped by month")
        .series([
            bar("revenue", BarOptions::new().group("money").label("Revenue")),
            bar("costs", BarOptions::new().group("money").label("Costs")),
        ])
        .grid(ChartGrid::new())
        .axes(axes())
        .legend(true)
        .tooltip(true)
        .height(260.);

    let stacked = Chart::new("chart-stacked", monthly(), "month")
        .series([
            bar("costs", BarOptions::new().stack("total").label("Costs")),
            bar("revenue", BarOptions::new().stack("total").label("Revenue")),
        ])
        .grid(ChartGrid::new())
        .axes(axes())
        .legend_config(ChartLegend::new().position(LegendPosition::End))
        .tooltip(true)
        .height(260.);

    let lines = Chart::new("chart-lines", monthly(), "month")
        .title("Revenue against trend")
        .series([
            line("revenue", LineOptions::new().label("Revenue").dots(true)),
            line(
                "trend",
                LineOptions::new().label("Trend").curve(CurveType::Monotone),
            ),
        ])
        .grid(ChartGrid::new())
        .axes(axes())
        .legend(true)
        .tooltip(true)
        .height(260.);

    let areas = Chart::new("chart-areas", monthly(), "month")
        .series([
            area("costs", AreaOptions::new().stack("total").label("Costs")),
            area(
                "revenue",
                AreaOptions::new().stack("total").label("Revenue"),
            ),
        ])
        .grid(ChartGrid::new())
        .axes(axes())
        .legend(true)
        .tooltip(true)
        .height(260.);

    let banded = Chart::new("chart-band", banded(), "month")
        .series([
            band(BandOptions::new("high", "low")),
            area(
                "trend",
                AreaOptions::new()
                    .gradient(true)
                    .curve(CurveType::Natural)
                    .label("Trend"),
            ),
        ])
        .grid(ChartGrid::new())
        .axes(axes())
        .tooltip(true)
        .height(260.);

    let scatter_chart = Chart::new("chart-scatter", scatter(), "x")
        .series([dot("y", DotOptions::new().label("Samples"))])
        .grid(ChartGrid::new().vertical(true))
        .axes(axes())
        .tooltip(true)
        .height(280.);

    let candles = Chart::new("chart-candles", prices(), "day")
        .title("Daily price")
        .series([candlestick(CandlestickOptions::new(
            "open", "high", "low", "close",
        ))])
        .y_baseline(crate::YBaseline::Data)
        .grid(ChartGrid::new())
        .axes(axes())
        .height(260.);

    let annotated = Chart::new("chart-annotations", sales(), "month")
        .series([
            bar("sales", BarOptions::new().label("Sales")),
            error_bar(ErrorBarOptions::new("errorHigh", "errorLow")),
            reference_line(ReferenceLineOptions::y(50.).label("Target")),
            reference_line(ReferenceLineOptions::y(20.).y2(30.).label("Floor")),
        ])
        .grid(ChartGrid::new())
        .axes(axes())
        .tooltip(true)
        .height(260.);

    let violet: gpui::Hsla = rgb(0x4f46e5).into();
    let scatter_gl = Chart::new("chart-dot-gl", scatter(), "x")
        .series([dot_gl("y", DotGlOptions::new(violet))])
        .grid(ChartGrid::new())
        .axes(axes())
        .height(260.);
    let scatter_gl_hover = Chart::new("chart-dot-gl-interactive", scatter(), "x")
        .series([dot_gl_interactive(
            "y",
            DotGlInteractiveOptions::new(violet),
        )])
        .grid(ChartGrid::new())
        .axes(axes())
        .height(260.);
    let heat = Chart::new("chart-heatmap-gl", heatmap(), "hour")
        .series([heatmap_gl(HeatmapGlOptions::new(
            "hour",
            "day",
            "traffic",
            [rgb(0xf0f9ff).into(), rgb(0x1e40af).into()],
        ))])
        .axes(axes())
        .height(240.);
    let feed = window.use_keyed_state("chart-stream-gl-feed", cx, |_, cx| {
        let handle = StreamGlHandle::new();
        let pushes = handle.clone();
        let task = cx.spawn(async move |_, cx| {
            let mut random = seeded_random(4);
            let mut x = 0.;
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(16))
                    .await;
                if x >= 500. {
                    pushes.clear();
                    x = 0.;
                }
                pushes.push(x, 50. + (x / 10.).sin() * 30. + random() * 10.);
                x += 1.;
            }
        });
        Feed {
            handle,
            _task: task,
        }
    });
    let handle = feed.read(cx).handle.clone();
    let stream = Chart::new("chart-stream-gl", Vec::<Datum>::new(), "x")
        .x_domain(0., 500.)
        .y_domain(0., 100.)
        .series([stream_gl(
            StreamGlOptions::new(rgb(0x10b981).into()).handle(handle),
        )])
        .grid(ChartGrid::new())
        .axes(axes())
        .height(220.);

    Stack::vertical()
        .gap(SpacingStep::S8)
        .child(example("Monthly revenue", showcase))
        .child(example("Grouped bars with a legend", grouped))
        .child(example("Stacked bars, legend at the end", stacked))
        .child(example("Lines", lines))
        .child(example("Stacked areas", areas))
        .child(example("A band around a gradient area", banded))
        .child(example("Scatter", scatter_chart))
        .child(example("Candlestick", candles))
        .child(example("Error bars and reference lines", annotated))
        .child(example("WebGL scatter", scatter_gl))
        .child(example("WebGL scatter with hover", scatter_gl_hover))
        .child(example("WebGL heatmap", heat))
        .child(example("WebGL stream", stream))
        .into_any_element()
}
