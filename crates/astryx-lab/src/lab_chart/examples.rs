//! The lab chart gallery (upstream's `Chart`, `ChartAdvanced`, `ChartInteractions`, `ChartDotGL`,
//! `ChartDotGLInteractive`, `ChartHeatmapGL`, `ChartStreamGL` and `useChartRange` stories). The
//! stories load vega-datasets from a CDN; these build seeded data of the same shapes.

use std::rc::Rc;
use std::time::Duration;

use astryx_components::layout::{SpacingStep, Stack};
use astryx_components::text::{Text, TextType};
use gpui::prelude::*;
use gpui::{AnyElement, App, Context, ElementId, SharedString, Task, Window, div, px};

use super::{
    AxisPosition, BrushMode, CandlestickVariant, Chart, ChartArea, ChartAxis, ChartBar, ChartBrush,
    ChartCandlestick, ChartCrosshairMode, ChartDot, ChartDotGl, ChartDotGlInteractive,
    ChartErrorBar, ChartGrid, ChartHeatmapGl, ChartLegend, ChartLegendItem, ChartLine, ChartRange,
    ChartReferenceLine, ChartSelect, ChartStreamGl, ChartStreamGlHandle, ChartTooltip, ChartZoom,
    Datum, DotGlTooltipRender, ReferenceLabelPosition, SequentialHue, UseChartRangeOptions,
    YBaseline, compact_number, use_chart_colors, use_chart_range,
};

/// A seeded generator (the stories' `makeSeededRandom`).
struct Seeded(f64);

impl Seeded {
    fn next(&mut self) -> f64 {
        self.0 = (self.0 * 1_664_525. + 1_013_904_223.) % 4_294_967_296.;
        self.0 / 4_294_967_296.
    }
}

fn example(title: &'static str, body: impl IntoElement) -> AnyElement {
    Stack::vertical()
        .gap(SpacingStep::S2)
        .child(Text::new(title).text_type(TextType::Large))
        .child(body)
        .into_any_element()
}

fn varieties() -> Vec<Datum> {
    [
        ("Glabron", 42.4),
        ("Trebi", 39.4),
        ("Wisconsin No. 38", 38.0),
        ("Velvet", 35.4),
        ("No. 462", 35.3),
        ("Peatland", 34.9),
        ("Manchuria", 33.9),
        ("No. 457", 33.6),
        ("Svansota", 30.4),
        ("No. 475", 30.3),
    ]
    .into_iter()
    .map(|(variety, y)| Datum::new().with("variety", variety).with("avgYield", y))
    .collect()
}

fn stocks() -> Vec<Datum> {
    let months = [
        "Mar 09", "Apr 09", "May 09", "Jun 09", "Jul 09", "Aug 09", "Sep 09", "Oct 09", "Nov 09",
        "Dec 09", "Jan 10", "Feb 10",
    ];
    let aapl = [
        105.12, 125.83, 135.81, 142.43, 163.39, 168.21, 185.35, 188.5, 199.91, 210.73, 192.06,
        204.62,
    ];
    let goog = [
        348.06, 395.97, 417.23, 421.59, 443.05, 461.67, 495.85, 536.12, 583.0, 619.98, 529.94,
        526.8,
    ];
    months
        .iter()
        .enumerate()
        .map(|(i, m)| {
            Datum::new()
                .with("date", *m)
                .with("AAPL", aapl[i])
                .with("GOOG", goog[i])
        })
        .collect()
}

fn cars(n: usize) -> Vec<Datum> {
    let mut r = Seeded(7.);
    (0..n)
        .map(|_| {
            let hp = 46. + r.next() * 184.;
            let mpg = (46. - hp * 0.14 + (r.next() - 0.5) * 10.).max(9.);
            Datum::new()
                .with("hp", (hp * 10.).round() / 10.)
                .with("mpg", (mpg * 10.).round() / 10.)
        })
        .collect()
}

fn weather() -> Vec<Datum> {
    let mut r = Seeded(11.);
    (0..30)
        .map(|i| {
            let mean = 12. + 8. * (i as f64 / 5.).sin() + (r.next() - 0.5) * 2.;
            let spread = 2. + r.next() * 3.;
            Datum::new()
                .with("day", i as f64)
                .with("mean", (mean * 10.).round() / 10.)
                .with("upper", ((mean + spread) * 10.).round() / 10.)
                .with("lower", ((mean - spread) * 10.).round() / 10.)
        })
        .collect()
}

fn activity() -> Vec<Datum> {
    let days = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
    let mut r = Seeded(3.);
    let mut rows = Vec::new();
    for hour in 0..24 {
        for (d, day) in days.iter().enumerate() {
            let busy = if (9..18).contains(&hour) && d < 5 {
                60.
            } else {
                15.
            };
            rows.push(
                Datum::new()
                    .with("hour", format!("{hour:02}"))
                    .with("day", *day)
                    .with("value", (busy + r.next() * 40.).round()),
            );
        }
    }
    rows
}

fn ohlc() -> Vec<Datum> {
    let mut r = Seeded(5.);
    let mut close: f64 = 100.;
    (1..=20)
        .map(|i| {
            let open = close;
            close = (open + (r.next() - 0.5) * 8.).max(1.);
            let high = open.max(close) + r.next() * 4.;
            let low = open.min(close) - r.next() * 4.;
            Datum::new()
                .with("day", format!("D{i}"))
                .with("open", open)
                .with("close", close)
                .with("high", high)
                .with("low", low)
        })
        .collect()
}

fn sentiment() -> Vec<Datum> {
    [
        ("Q1", 12.),
        ("Q2", -8.),
        ("Q3", 21.),
        ("Q4", -15.),
        ("Q5", 4.),
        ("Q6", -2.),
    ]
    .into_iter()
    .map(|(q, v)| Datum::new().with("quarter", q).with("delta", v))
    .collect()
}

/// The interactive stories' state: a brushed range, zoomed domains and a selection.
#[derive(Default)]
struct Interactions {
    brushed: Option<[f64; 2]>,
    x_domain: Option<[f64; 2]>,
    y_domain: Option<[f64; 2]>,
    selected: Vec<usize>,
}

/// The streaming story's state: its range, its line's handle and the task pushing to it.
struct Stream {
    range: Option<ChartRange>,
    handle: ChartStreamGlHandle,
    t: f64,
    _tick: Option<Task<()>>,
}

impl Stream {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let tick = cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(50))
                    .await;
                let alive = this.update(cx, |s, cx| {
                    s.t += 1.;
                    let y = (s.t / 20.).sin() * 40. + (s.t / 7.).cos() * 10.;
                    if let Some(range) = s.range.clone() {
                        range.push(s.t, y, Some(&s.handle), cx);
                    }
                    cx.notify();
                });
                if alive.is_err() {
                    break;
                }
            }
        });
        Stream {
            range: None,
            handle: ChartStreamGlHandle::new(),
            t: 0.,
            _tick: Some(tick),
        }
    }
}

/// Upstream's stories, top to bottom.
pub fn examples(window: &mut Window, cx: &mut App) -> AnyElement {
    let colors = use_chart_colors(window, cx);
    let c = colors.categorical(3);
    let mut column = Stack::vertical().gap(SpacingStep::S8);

    column = column.child(example(
        "Bar chart",
        Chart::new("lab-chart-bars", varieties(), "variety", ["avgYield"])
            .child(ChartGrid::new())
            .child(
                ChartAxis::new(AxisPosition::Bottom)
                    .max_ticks(5)
                    .truncate(8),
            )
            .child(ChartAxis::new(AxisPosition::Left))
            .child(ChartBar::new("avgYield", c[0]))
            .child(ChartTooltip::new()),
    ));

    column = column.child(example(
        "Line chart",
        Chart::new("lab-chart-lines", stocks(), "date", ["AAPL", "GOOG"])
            .y_baseline(YBaseline::Data)
            .child(ChartGrid::new())
            .child(ChartAxis::new(AxisPosition::Bottom))
            .child(ChartAxis::new(AxisPosition::Left))
            .child(ChartLine::new("AAPL", c[0]).dots(true))
            .child(ChartLine::new("GOOG", c[1]).dots(true))
            .child(ChartLegend::new().items([
                ChartLegendItem::new("AAPL", c[0]),
                ChartLegendItem::new("GOOG", c[1]),
            ]))
            .child(ChartTooltip::new()),
    ));

    column = column.child(example(
        "Scatter plot",
        Chart::new("lab-chart-scatter", cars(406), "hp", ["mpg"])
            .y_baseline(YBaseline::Data)
            .height(350.)
            .child(ChartGrid::new().vertical(true))
            .child(ChartAxis::new(AxisPosition::Bottom))
            .child(ChartAxis::new(AxisPosition::Left))
            .child(ChartDot::new("mpg", c[0]).radius(3.))
            .child(ChartTooltip::new().crosshair(ChartCrosshairMode::Xy)),
    ));

    column = column.child(example(
        "WebGL scatter",
        Chart::new("lab-chart-gl", cars(10_000), "hp", ["mpg"])
            .y_baseline(YBaseline::Data)
            .child(ChartAxis::new(AxisPosition::Bottom))
            .child(ChartAxis::new(AxisPosition::Left))
            .child(ChartDotGl::new("mpg", c[0]).size(3.).opacity(0.4)),
    ));

    column = column.child(example(
        "Confidence band",
        Chart::new("lab-chart-band", weather(), "day", ["upper", "lower"])
            .y_baseline(YBaseline::Data)
            .child(ChartGrid::new())
            .child(ChartAxis::new(AxisPosition::Bottom))
            .child(ChartAxis::new(AxisPosition::Left))
            .child(
                ChartArea::new(c[0])
                    .y_upper("upper")
                    .y_lower("lower")
                    .stroke(true),
            )
            .child(ChartLine::new("mean", c[0])),
    ));

    let heat = colors
        .sequential(SequentialHue::Blue, 5)
        .into_iter()
        .rev()
        .collect::<Vec<_>>();
    column = column.child(example(
        "Heatmap",
        Chart::new("lab-chart-heatmap", activity(), "hour", ["value"])
            .height(260.)
            .child(ChartAxis::new(AxisPosition::Bottom).max_ticks(12))
            .child(ChartHeatmapGl::new("hour", "day", "value", heat.clone())),
    ));

    let mut means = Seeded(9.);
    let error_data: Vec<Datum> = ["A", "B", "C", "D", "E"]
        .iter()
        .map(|g| {
            let mean = 20. + means.next() * 30.;
            let err = 2. + means.next() * 6.;
            Datum::new()
                .with("group", *g)
                .with("mean", mean)
                .with("upper", mean + err)
                .with("lower", mean - err)
        })
        .collect();
    column = column.child(example(
        "Error bars",
        Chart::new("lab-chart-errors", error_data, "group", ["upper"])
            .child(ChartAxis::new(AxisPosition::Bottom))
            .child(ChartAxis::new(AxisPosition::Left))
            .child(ChartBar::new("mean", colors.alpha(c[0], 0.6)))
            .child(ChartErrorBar::new("upper", "lower")),
    ));

    column = column.child(example(
        "Candlestick",
        Chart::new("lab-chart-ohlc", ohlc(), "day", ["high", "low"])
            .y_baseline(YBaseline::Data)
            .child(ChartGrid::new())
            .child(ChartAxis::new(AxisPosition::Bottom).max_ticks(10))
            .child(ChartAxis::new(AxisPosition::Left))
            .child(
                ChartCandlestick::new("high", "low", "open", "close")
                    .up_color(colors.semantic.positive)
                    .down_color(colors.semantic.negative),
            ),
    ));

    column = column.child(example(
        "OHLC bars",
        Chart::new("lab-chart-ohlc-bars", ohlc(), "day", ["high", "low"])
            .y_baseline(YBaseline::Data)
            .child(ChartAxis::new(AxisPosition::Bottom).max_ticks(10))
            .child(ChartAxis::new(AxisPosition::Left))
            .child(
                ChartCandlestick::new("high", "low", "open", "close")
                    .variant(CandlestickVariant::Bar),
            ),
    ));

    column = column.child(example(
        "Zero-centered",
        Chart::new("lab-chart-zero", sentiment(), "quarter", ["delta"])
            .y_baseline(YBaseline::Zero)
            .child(ChartGrid::new())
            .child(ChartAxis::new(AxisPosition::Bottom))
            .child(ChartAxis::new(AxisPosition::Left))
            .child(ChartBar::new("delta", c[1]))
            .child(ChartReferenceLine::new().y(0.)),
    ));

    column = column.child(example(
        "Gradient legend",
        div().pt(px(8.)).child(
            ChartLegend::new()
                .gradient(colors.diverging().cold_hot(7))
                .domain(-50., 50.)
                .label("Change %")
                .ticks(5),
        ),
    ));

    // The interactive stories share one state.
    let state =
        window.use_keyed_state("lab-chart-interactions", cx, |_, _| Interactions::default());
    let (brushed, x_domain, y_domain, selected) = {
        let s = state.read(cx);
        (s.brushed, s.x_domain, s.y_domain, s.selected.clone())
    };
    let brush_state = state.clone();
    let caption: SharedString = match brushed {
        Some([a, b]) => format!(
            "Brushed {} to {}",
            compact_number(&a.into()),
            compact_number(&b.into())
        )
        .into(),
        None => "Drag across the chart to brush a range".into(),
    };
    column = column.child(example(
        "Brush",
        Stack::vertical()
            .gap(SpacingStep::S2)
            .child(Text::new(caption).text_type(TextType::Supporting))
            .child(
                Chart::new("lab-chart-brush", weather(), "day", ["mean"])
                    .interactive(true)
                    .child(ChartAxis::new(AxisPosition::Bottom))
                    .child(ChartAxis::new(AxisPosition::Left))
                    .child(ChartLine::new("mean", c[0]))
                    .child(ChartBrush::new().mode(BrushMode::X).on_brush(
                        move |range, _, _, cx| {
                            brush_state.update(cx, |s, cx| {
                                s.brushed = Some(range.x);
                                cx.notify();
                            })
                        },
                    )),
            ),
    ));

    column = column.child(example(
        "Crosshair",
        Chart::new("lab-chart-crosshair", stocks(), "date", ["AAPL"])
            .y_baseline(YBaseline::Data)
            .margin(super::MarginOverride {
                left: Some(56.),
                ..Default::default()
            })
            .child(ChartAxis::new(AxisPosition::Bottom))
            .child(ChartAxis::new(AxisPosition::Left))
            .child(ChartLine::new("AAPL", c[0]))
            .child(
                ChartTooltip::new()
                    .crosshair(ChartCrosshairMode::Xy)
                    .crosshair_labels(true),
            ),
    ));

    let (zx, zy) = (state.clone(), state.clone());
    let mut zoom_chart = Chart::new("lab-chart-zoom", cars(400), "hp", ["mpg"])
        .y_baseline(YBaseline::Data)
        .interactive(true);
    if let Some([a, b]) = x_domain {
        zoom_chart = zoom_chart.x_domain(a, b);
    }
    if let Some([a, b]) = y_domain {
        zoom_chart = zoom_chart.y_domain(a, b);
    }
    column = column.child(example(
        "Zoom and pan",
        zoom_chart
            .child(ChartGrid::new().vertical(true))
            .child(ChartAxis::new(AxisPosition::Bottom).animated(false))
            .child(ChartAxis::new(AxisPosition::Left).animated(false))
            .child(ChartDot::new("mpg", c[2]).radius(3.))
            .child(
                ChartZoom::new()
                    .on_x_domain_change(move |d, _, cx| {
                        zx.update(cx, |s, cx| {
                            s.x_domain = Some(d);
                            cx.notify();
                        })
                    })
                    .on_y_domain_change(move |d, _, cx| {
                        zy.update(cx, |s, cx| {
                            s.y_domain = Some(d);
                            cx.notify();
                        })
                    }),
            ),
    ));

    let select_state = state.clone();
    column = column.child(example(
        "Click to select",
        Chart::new("lab-chart-select", cars(60), "hp", ["mpg"])
            .y_baseline(YBaseline::Data)
            .child(ChartAxis::new(AxisPosition::Bottom))
            .child(ChartAxis::new(AxisPosition::Left))
            .child(ChartDot::new("mpg", c[0]))
            .child(ChartSelect::new().selected(selected).on_selection_change(
                move |next, _, cx| {
                    select_state.update(cx, |s, cx| {
                        s.selected = next;
                        cx.notify();
                    })
                },
            )),
    ));

    column = column.child(example(
        "Reference lines",
        Chart::new("lab-chart-reference", weather(), "day", ["upper", "lower"])
            .y_baseline(YBaseline::Data)
            .child(ChartAxis::new(AxisPosition::Bottom))
            .child(ChartAxis::new(AxisPosition::Left))
            .child(ChartLine::new("mean", c[0]))
            .child(ChartReferenceLine::new().y(12.).label("Average"))
            .child(
                ChartReferenceLine::new()
                    .x(20.)
                    .label("Release")
                    .label_position(ReferenceLabelPosition::Start),
            ),
    ));

    let tooltip: DotGlTooltipRender = Rc::new(|d, i, _, _| {
        Some(
            div()
                .text_size(px(12.))
                .child(format!(
                    "Car {i}: {} hp, {} mpg",
                    d.string("hp"),
                    d.string("mpg")
                ))
                .into_any_element(),
        )
    });
    column = column.child(example(
        "GPU picking",
        Chart::new("lab-chart-picking", cars(5_000), "hp", ["mpg"])
            .y_baseline(YBaseline::Data)
            .child(ChartAxis::new(AxisPosition::Bottom))
            .child(ChartAxis::new(AxisPosition::Left))
            .child(
                ChartDotGlInteractive::new("mpg", c[1])
                    .size(4.)
                    .render_tooltip(move |d, i, w, cx| tooltip(d, i, w, cx)),
            ),
    ));

    // A stream: a point every 50 ms, its window tracked by the range.
    let stream = window.use_keyed_state(
        ElementId::Name("lab-chart-stream-demo".into()),
        cx,
        Stream::new,
    );
    let ranged = use_chart_range(
        "lab-chart-stream-range",
        UseChartRangeOptions::new(300.),
        window,
        cx,
    );
    let handle = stream.update(cx, |s, _| {
        if s.range.is_none() {
            s.range = Some(ranged.range.clone());
        }
        s.handle.clone()
    });
    let [x0, x1] = ranged.x_domain;
    let [y0, y1] = ranged.y_domain;
    column = column.child(example(
        "Streaming line",
        Chart::new("lab-chart-stream", Vec::<Datum>::new(), "x", ["y"])
            .x_domain(x0, x1)
            .y_domain(y0, y1)
            .child(ChartGrid::new())
            .child(ChartAxis::new(AxisPosition::Bottom))
            .child(ChartAxis::new(AxisPosition::Left))
            .child(ChartStreamGl::new(c[0]).handle(handle).buffer_size(300)),
    ));

    column.into_any_element()
}
