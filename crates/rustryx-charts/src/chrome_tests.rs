//! Chart chrome tests: upstream `ChartAxis.test.tsx`, `ChartGrid.test.tsx`,
//! `ChartLegend.test.tsx`, `ChartSwatch.test.tsx`, `ChartTooltip.test.tsx` and `tooltip.ts`'s
//! behaviour. Positions and labels were checked against upstream's components rendered
//! under Node on the same scales (plot 528 by 244, as a 600 px chart's).
//!
//! A GPUI test cannot read painted shapes or the accessibility tree, so these test
//! what the chrome computes (ticks, lines, rows, hover marks, card text and
//! placement) and, through debug selectors, which elements a frame draws.

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{
    AnyElement, Context, Hsla, InteractiveElement, IntoElement, ParentElement, Render,
    SharedString, Styled, TestAppContext, VisualTestContext, Window, div, px,
};

use crate::axis::{axis_label, axis_line, axis_line_y, axis_origin, label_place, tick_mark};
use crate::context::{ChartContext, PointerHandler};
use crate::label::Anchor;
use crate::layout::{LayoutInput, compute_layout};
use crate::legend::legend_label;
use crate::scale::{BandScale, LinearScale};
use crate::tooltip::{DefaultCard, HoverDot, Indicator, place_card};
use crate::*;

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-9 * a.abs().max(1.)
}

fn rows(values: &[(&str, f64)]) -> Rc<[Datum]> {
    values
        .iter()
        .map(|(m, s)| Datum::new().with("month", m.to_string()).with("sales", *s))
        .collect()
}

fn months() -> Rc<[Datum]> {
    rows(&[("Jan", 4.), ("Feb", 10.), ("Mar", 2.)])
}

/// A chart context laid out by the chart's own pass, as the chrome reads it.
fn chart_of(data: Rc<[Datum]>, series: Vec<SeriesDef>, width: f64) -> ChartContext {
    let result = compute_layout(LayoutInput {
        data: &data,
        x_key: &"month".into(),
        series: &series,
        width,
        height: 244.,
        y_baseline: None,
        y_domain: None,
        x_domain: None,
    });
    ChartContext {
        id: "chart".into(),
        width,
        height: 244.,
        margin: ChartMargin::default(),
        data,
        x_key: "month".into(),
        x_scale: result.x_scale.clone(),
        y_scale: result.y_scale,
        y_band_scale: result.y_band_scale.clone(),
        resolved: Rc::new(result.resolved.clone()),
        series: series
            .iter()
            .cloned()
            .zip(result.info.iter().cloned())
            .collect(),
        handlers: Default::default(),
    }
}

fn bar_chart(data: Rc<[Datum]>, width: f64) -> ChartContext {
    chart_of(data, vec![bar("sales", BarOptions::new())], width)
}

fn labels(axis: &ChartAxis, chart: &ChartContext) -> Vec<String> {
    axis.ticks(chart)
        .iter()
        .map(|t| t.label.to_string())
        .collect()
}

#[test]
fn a_bottom_axis_labels_each_category() {
    let chart = bar_chart(months(), 528.);
    assert_eq!(axis_label(AxisPosition::Bottom), "bottom axis");
    assert_eq!(
        labels(&ChartAxis::new(AxisPosition::Bottom), &chart),
        ["Jan", "Feb", "Mar"]
    );
}

#[test]
fn a_bottom_axis_sits_on_the_plot_bottom_and_draws_its_edge_line() {
    let chart = bar_chart(months(), 528.);
    let axis = ChartAxis::new(AxisPosition::Bottom);
    // The plot of a 300 px chart is 300 - 24 - 32 = 244 tall.
    assert_eq!(axis_origin(AxisPosition::Bottom, 528., 244.), (0., 244.));
    assert!(axis.draws_axis_line());
    let y = axis_line_y(AxisPosition::Bottom, &chart);
    assert_eq!(
        axis_line(AxisPosition::Bottom, 528., 244., y),
        ((0., 0.), (528., 0.))
    );
}

#[test]
fn a_tick_format_returning_none_keeps_the_category_text() {
    let chart = bar_chart(months(), 528.);
    let axis = ChartAxis::new(AxisPosition::Bottom).tick_format(|_| None);
    assert_eq!(labels(&axis, &chart), ["Jan", "Feb", "Mar"]);
}

#[test]
fn max_ticks_caps_labels_by_evenly_skipping_categories() {
    let chart = bar_chart(months(), 528.);
    // 3 categories capped at 2: a step of 2 keeps the first and the last.
    let axis = ChartAxis::new(AxisPosition::Bottom).max_ticks(2);
    assert_eq!(labels(&axis, &chart), ["Jan", "Mar"]);
}

#[test]
fn thinning_keeps_the_last_label() {
    let data: Rc<[Datum]> = (0..14)
        .map(|i| {
            Datum::new()
                .with("month", format!("Day {}", i + 1))
                .with("sales", i)
        })
        .collect();
    let chart = bar_chart(data, 528.);
    // 14 labels capped at 7: a step of 2. Day 13 would be the last kept label;
    // Day 14 replaces it because it sits closer than one step.
    let axis = ChartAxis::new(AxisPosition::Bottom).max_ticks(7);
    assert_eq!(
        labels(&axis, &chart),
        [
            "Day 1", "Day 3", "Day 5", "Day 7", "Day 9", "Day 11", "Day 14"
        ]
    );
}

fn periods() -> Rc<[Datum]> {
    (0..20)
        .map(|i| {
            Datum::new()
                .with("month", format!("Period {}", i + 1))
                .with("sales", i)
        })
        .collect()
}

#[test]
fn categorical_labels_thin_as_the_plot_narrows() {
    let axis = ChartAxis::new(AxisPosition::Bottom);
    let wide = labels(&axis, &bar_chart(periods(), 528.));
    let narrow = labels(&axis, &bar_chart(periods(), 48.));
    assert_eq!(
        wide,
        ["Period 1", "Period 5", "Period 9", "Period 13", "Period 20"]
    );
    assert_eq!(narrow, ["Period 1"]);
    assert!(!narrow.is_empty() && narrow.len() < wide.len());
}

#[test]
fn ascii_and_unicode_truncation_keep_the_same_thinned_labels() {
    let clusters = [
        "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}\u{200D}\u{1F466}",
        "\u{1F1EF}\u{1F1F5}",
        "\u{1F469}\u{1F3FD}\u{200D}\u{1F680}",
    ];
    let make = |label: &dyn Fn(usize) -> String| -> Rc<[Datum]> {
        (0..9)
            .map(|i| Datum::new().with("month", label(i)).with("sales", i))
            .collect()
    };
    let ascii = make(&|i| format!("{}AB", i + 1));
    let unicode = make(&|i| format!("{}{}X", i + 1, clusters[i % 3]));
    let axis = ChartAxis::new(AxisPosition::Bottom).truncate(2);
    // A 300 px chart: a 228 px plot.
    let indexes = |data: Rc<[Datum]>| -> Vec<u32> {
        labels(&axis, &bar_chart(data, 228.))
            .iter()
            .map(|l| {
                l.chars()
                    .take_while(char::is_ascii_digit)
                    .collect::<String>()
                    .parse()
                    .unwrap()
            })
            .collect()
    };
    assert_eq!(indexes(unicode.clone()), indexes(ascii.clone()));
    assert_eq!(
        labels(&axis, &bar_chart(ascii, 228.)),
        [
            "1A\u{2026}",
            "3A\u{2026}",
            "5A\u{2026}",
            "7A\u{2026}",
            "9A\u{2026}"
        ]
    );
    assert_eq!(
        labels(&axis, &bar_chart(unicode, 228.))[1],
        format!("3{}\u{2026}", clusters[2])
    );
}

#[test]
fn long_category_labels_truncate_with_an_ellipsis() {
    let chart = bar_chart(rows(&[("January", 4.), ("Feb", 10.)]), 528.);
    let axis = ChartAxis::new(AxisPosition::Bottom).truncate(4);
    // Short labels are left alone.
    assert_eq!(labels(&axis, &chart), ["Janu\u{2026}", "Feb"]);
}

#[test]
fn truncation_keeps_whole_grapheme_clusters() {
    let family = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}\u{200D}\u{1F466}";
    let data: Rc<[Datum]> = [Datum::new()
        .with("month", format!("{family}Q"))
        .with("sales", 4)]
    .into();
    let axis = ChartAxis::new(AxisPosition::Bottom).truncate(1);
    assert_eq!(
        labels(&axis, &bar_chart(data, 528.)),
        [format!("{family}\u{2026}")]
    );
}

#[test]
fn a_left_axis_writes_d3_value_ticks_without_an_edge_line() {
    let chart = bar_chart(months(), 528.);
    let axis = ChartAxis::new(AxisPosition::Left);
    // A bar pins zero: domain [0, 10], ticks 0, 2, ..., 10.
    assert_eq!(labels(&axis, &chart), ["0", "2", "4", "6", "8", "10"]);
    let offsets: Vec<f64> = axis.ticks(&chart).iter().map(|t| t.offset).collect();
    assert!(close(offsets[1], 195.2) && offsets[0] == 244. && offsets[5] == 0.);
    // Only the bottom axis draws its edge line by default.
    assert!(!axis.draws_axis_line());
}

#[test]
fn a_custom_tick_formatter_writes_the_labels() {
    let chart = bar_chart(months(), 528.);
    let axis = ChartAxis::new(AxisPosition::Left)
        .tick_format(|v| Some(format!("\u{24}{}", v.to_js_string()).into()));
    assert_eq!(
        labels(&axis, &chart),
        [
            "\u{24}0", "\u{24}2", "\u{24}4", "\u{24}6", "\u{24}8", "\u{24}10"
        ]
    );
}

#[test]
fn a_tick_format_returning_none_shows_the_default_text() {
    let big = rows(&[("Jan", 1_200_000.), ("Feb", 1_500_000.)]);
    let chart = bar_chart(big, 528.);
    let defaults = labels(&ChartAxis::new(AxisPosition::Left), &chart);
    let none = labels(
        &ChartAxis::new(AxisPosition::Left).tick_format(|_| None),
        &chart,
    );
    assert_eq!(none, defaults);
    // d3's default groups digits; String(1000000) would not.
    assert_eq!(defaults, ["0", "500,000", "1,000,000", "1,500,000"]);
}

#[test]
fn a_currency_format_is_a_tick_format() {
    let dollars = crate::format::currency("\u{24}", Some("en-US"));
    let axis = ChartAxis::new(AxisPosition::Left).tick_format(move |v| {
        dollars.format(Some(v), "en-US", rustryx_i18n::CalendarSystem::Gregory)
    });
    assert!(labels(&axis, &bar_chart(months(), 528.)).contains(&"\u{24}10".to_string()));
    let big = bar_chart(rows(&[("Jan", 1_200_000.), ("Feb", 1_500_000.)]), 528.);
    assert_eq!(
        labels(&axis, &big),
        ["\u{24}0", "\u{24}500K", "\u{24}1M", "\u{24}1.5M"]
    );
}

#[test]
fn show_ticks_draws_a_mark_per_tick_and_forces_the_axis_line_on() {
    let chart = bar_chart(months(), 528.);
    let axis = ChartAxis::new(AxisPosition::Left).show_ticks(true);
    let ticks = axis.ticks(&chart);
    // 6 ticks (0 to 10 by 2), each with its mark, and the forced edge line.
    assert_eq!(ticks.len(), 6);
    assert!(axis.draws_axis_line());
    assert_eq!(
        axis_line(AxisPosition::Left, 528., 244., 0.),
        ((0., 0.), (0., 244.))
    );
    for t in &ticks {
        assert_eq!(
            tick_mark(AxisPosition::Left, t.offset),
            ((0., t.offset), (-6., t.offset))
        );
    }
}

#[test]
fn top_and_right_axes_sit_on_their_edges_with_outward_labels() {
    assert_eq!(axis_origin(AxisPosition::Top, 528., 244.), (0., 0.));
    assert_eq!(
        label_place(AxisPosition::Top, 99.),
        ((99., -10.), Anchor::Middle)
    );
    assert_eq!(tick_mark(AxisPosition::Top, 99.), ((99., 0.), (99., -6.)));
    assert_eq!(axis_origin(AxisPosition::Right, 528., 244.), (528., 0.));
    let ((x, baseline), anchor) = label_place(AxisPosition::Right, 244.);
    assert_eq!((x, anchor), (10., Anchor::Start));
    // SVG's dy="0.32em" of a 12 px label.
    assert!(close(baseline, 244. + 3.84));
    assert_eq!(
        label_place(AxisPosition::Bottom, 99.),
        ((99., 18.), Anchor::Middle)
    );
    assert_eq!(label_place(AxisPosition::Left, 0.).1, Anchor::End);
}

#[test]
fn tick_counts_are_sanitized_before_they_reach_d3() {
    let chart = bar_chart(months(), 528.);
    let count = |n: f64| {
        ChartAxis::new(AxisPosition::Left)
            .tick_count(n)
            .ticks(&chart)
            .len()
    };
    // Zero asks for no ticks; a non-finite or negative count is the default 5.
    assert_eq!(count(0.), 0);
    assert_eq!(count(3.), 3);
    assert_eq!(count(10.), 11);
    assert_eq!(count(f64::NAN), 6);
    assert_eq!(count(-1.), 6);
    // A fractional count floors: 2 ticks' worth on [0, 10] is 0, 5, 10.
    assert_eq!(count(2.7), 3);
}

#[test]
fn the_bottom_line_marks_zero_when_the_domain_spans_it() {
    let chart = bar_chart(rows(&[("a", -20.), ("b", 50.), ("c", 13.)]), 528.);
    assert!(close(
        axis_line_y(AxisPosition::Bottom, &chart),
        -69.71428571428572
    ));
    assert_eq!(axis_line_y(AxisPosition::Left, &chart), 0.);
}

#[test]
fn a_linear_axis_thins_to_its_width() {
    let data: Rc<[Datum]> = [(0., 1.), (5., 7.), (10., 3.)]
        .iter()
        .map(|(m, s)| Datum::new().with("month", *m).with("sales", *s))
        .collect();
    let axis = ChartAxis::new(AxisPosition::Bottom);
    let line_chart = |width| chart_of(data.clone(), vec![line("sales", LineOptions::new())], width);
    assert_eq!(
        labels(&axis, &line_chart(528.)),
        ["0", "2", "4", "6", "8", "10"]
    );
    assert_eq!(labels(&axis, &line_chart(60.)), ["0", "10"]);
}

#[test]
fn grid_lines_cross_the_plot_at_every_y_tick_but_zero() {
    let chart = bar_chart(months(), 528.);
    let (h, v) = ChartGrid::new().lines(&chart);
    // Domain [0, 10]: ticks 0, 2, ..., 10, with the 0 line left to the axis.
    assert_eq!(h.len(), 5);
    assert!(v.is_empty());
    assert!(h.iter().all(|y| *y != 244.));
    let expected = [195.2, 146.4, 97.6, 48.8, 0.];
    assert!(h.iter().zip(expected).all(|(a, b)| close(*a, b)), "{h:?}");
}

#[test]
fn vertical_grid_lines_sit_at_band_centers() {
    let chart = bar_chart(months(), 528.);
    let (h, v) = ChartGrid::new()
        .horizontal(false)
        .vertical(true)
        .lines(&chart);
    assert!(h.is_empty());
    assert_eq!(v.len(), 3);
    assert!(close(v[0], 99.) && v[1] == 264. && v[2] == 429.);
    let (h, v) = ChartGrid::new().vertical(true).lines(&chart);
    assert_eq!(h.len() + v.len(), 8);
}

#[test]
fn grid_tick_counts_are_sanitized() {
    let chart = bar_chart(months(), 528.);
    let total = |grid: ChartGrid| {
        let (h, v) = grid.lines(&chart);
        h.len() + v.len()
    };
    assert_eq!(total(ChartGrid::new().tick_count(0.)), 0);
    assert_eq!(total(ChartGrid::new().tick_count(f64::INFINITY)), 5);
    let huge = total(ChartGrid::new().tick_count(1e9));
    assert!(huge > 0 && huge <= 1001, "{huge}");
}

#[test]
fn vertical_grid_lines_follow_d3_ticks_on_a_linear_x() {
    let data: Rc<[Datum]> = [(0., 1.), (5., 7.), (10., 3.)]
        .iter()
        .map(|(m, s)| Datum::new().with("month", *m).with("sales", *s))
        .collect();
    let chart = bar_chart(data, 528.);
    let (_, v) = ChartGrid::new()
        .horizontal(false)
        .vertical(true)
        .lines(&chart);
    // Domain [0, 10]: ticks 0, 2, ..., 10, evenly spaced.
    assert_eq!(v.len(), 6);
    assert_eq!(v[0], 0.);
    assert_eq!(v[5], 528.);
    assert!(close(v[1] - v[0], 528. / 5.));
}

struct Host(Rc<dyn Fn() -> AnyElement>);

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().w(px(600.)).h(px(400.)).child((self.0)())
    }
}

fn host(
    cx: &mut TestAppContext,
    build: impl Fn() -> AnyElement + 'static,
) -> &mut VisualTestContext {
    cx.update(|cx| {
        rustryx_theme::init(cx);
        rustryx_primitives::focus::init(cx);
    });
    let (_, cx) = cx.add_window_view(move |_, _| Host(Rc::new(build)));
    cx.run_until_parked();
    cx
}

fn hex(value: u32) -> Hsla {
    gpui::rgb(value).into()
}

fn legend_items() -> Vec<LegendItem> {
    vec![
        LegendItem {
            label: "Revenue".into(),
            color: hex(0xff0000),
            kind: Some("bar".into()),
        },
        LegendItem {
            label: "Profit".into(),
            color: hex(0x00ff00),
            kind: Some("line".into()),
        },
    ]
}

#[gpui::test]
fn a_legend_lists_one_row_per_item(cx: &mut TestAppContext) {
    let cx = host(cx, || {
        ChartLegend::new().items(legend_items()).into_any_element()
    });
    assert!(cx.debug_bounds("chart-legend").is_some());
    assert!(cx.debug_bounds("chart-legend-item-0").is_some());
    assert!(cx.debug_bounds("chart-legend-item-1").is_some());
    assert!(cx.debug_bounds("chart-legend-item-2").is_none());
}

#[test]
fn the_legend_label_is_localized() {
    assert_eq!(legend_label(&rustryx_i18n::english()), "Chart legend");
    let french = rustryx_i18n::I18n::new("fr").messages(
        "fr",
        [("@astryx.chartLegend.label", "L\u{e9}gende du graphique")],
    );
    assert_eq!(legend_label(&french), "L\u{e9}gende du graphique");
}

#[gpui::test]
fn a_legend_with_no_items_renders_nothing(cx: &mut TestAppContext) {
    let cx = host(cx, || {
        div()
            .child(ChartLegend::new().items([]))
            .child(ChartLegend::new())
            .into_any_element()
    });
    assert!(cx.debug_bounds("chart-legend").is_none());
}

#[gpui::test]
fn a_vertical_legend_keeps_its_rows(cx: &mut TestAppContext) {
    let cx = host(cx, || {
        ChartLegend::new()
            .items(legend_items())
            .position(LegendPosition::End)
            .into_any_element()
    });
    let first = cx.debug_bounds("chart-legend-item-0").unwrap();
    let second = cx.debug_bounds("chart-legend-item-1").unwrap();
    assert!(second.origin.y > first.origin.y, "a column, not a row");
}

#[test]
fn legend_items_take_the_series_colors_and_swatch_shapes() {
    let series = [
        bar(
            "revenue",
            BarOptions::new().label("Revenue").color(hex(0xff0000)),
        ),
        line("profit", LineOptions::new().label("Profit")),
    ];
    let items = derive_legend_items(&series, &[None, Some(hex(0x00ff00))], gpui::blue());
    assert_eq!(items[0].color, hex(0xff0000));
    assert_eq!(items[1].color, hex(0x00ff00));
    assert_eq!(
        swatch_variant_for(items[0].kind.as_ref()),
        ChartSwatchVariant::Square
    );
    assert_eq!(
        swatch_variant_for(items[1].kind.as_ref()),
        ChartSwatchVariant::Line
    );
}

#[gpui::test]
fn a_square_swatch_is_an_8_px_chip(cx: &mut TestAppContext) {
    let cx = host(cx, || {
        div()
            .flex()
            .child(ChartSwatch::new(hex(0xff0000)))
            .into_any_element()
    });
    let square = cx.debug_bounds("chart-swatch").unwrap();
    assert_eq!(square.size, gpui::size(px(8.), px(8.)));
}

#[gpui::test]
fn a_line_swatch_is_a_short_stroke(cx: &mut TestAppContext) {
    let cx = host(cx, || {
        div()
            .flex()
            .child(ChartSwatch::new(hex(0x000000)).variant(ChartSwatchVariant::Line))
            .into_any_element()
    });
    let line = cx.debug_bounds("chart-swatch").unwrap();
    assert_eq!(line.size, gpui::size(px(10.), px(3.)));
}

#[test]
fn swatch_variants_follow_the_mark_type() {
    assert_eq!(
        swatch_variant_for_type(Some("bar")),
        ChartSwatchVariant::Square
    );
    for kind in [Some("line"), Some("dot"), Some("area"), None] {
        assert_eq!(
            swatch_variant_for_type(kind),
            ChartSwatchVariant::Line,
            "{kind:?}"
        );
    }
}

// Tooltip.

/// Upstream's tooltip harness data: three months of sales and profit.
fn tooltip_data() -> Rc<[Datum]> {
    [("Jan", 3., 1.), ("Feb", 7., 2.), ("Mar", 5., 4.)]
        .iter()
        .map(|(m, s, p)| {
            Datum::new()
                .with("month", *m)
                .with("sales", *s)
                .with("profit", *p)
        })
        .collect()
}

const SALES_POINTS: [(f64, f64); 3] = [(50., 120.), (250., 40.), (450., 80.)];

fn sales_points() -> Vec<ResolvedPoint> {
    SALES_POINTS
        .iter()
        .enumerate()
        .map(|(i, (px, py))| ResolvedPoint {
            px: *px,
            py: *py,
            py0: 200.,
            data_index: i,
        })
        .collect()
}

fn red() -> Hsla {
    hex(0xff0000)
}

/// A post-layout series, as upstream's `makeSeries` annotates one.
fn sales_line() -> SeriesDef {
    line("sales", LineOptions::new().label("Sales").color(red()))
}

fn profit_line() -> SeriesDef {
    line("profit", LineOptions::new().label("Profit").color(red()))
}

fn info(uid: &str) -> SeriesInfo {
    SeriesInfo {
        uid: uid.to_string().into(),
        is_top_of_stack: true,
        resolved_color: None,
        group: None,
    }
}

/// Upstream's hand-built context: a 500 by 200 plot, a padded band x and a [0, 10] y.
fn tooltip_chart(
    series: &[(SeriesDef, &str)],
    resolved: Vec<(&str, Vec<ResolvedPoint>)>,
    handlers: Rc<RefCell<Vec<PointerHandler>>>,
) -> ChartContext {
    ChartContext {
        id: "tooltip-chart".into(),
        width: 500.,
        height: 200.,
        margin: ChartMargin::default(),
        data: tooltip_data(),
        x_key: "month".into(),
        x_scale: ChartScale::Band(
            BandScale::new()
                .domain(["Jan", "Feb", "Mar"])
                .range(0., 500.)
                .padding(0.2),
        ),
        y_scale: LinearScale::new().domain(0., 10.).range(200., 0.),
        y_band_scale: None,
        resolved: Rc::new(
            resolved
                .into_iter()
                .map(|(uid, points)| (SharedString::from(uid.to_string()), points))
                .collect(),
        ),
        series: series
            .iter()
            .map(|(s, uid)| (s.clone(), info(uid)))
            .collect(),
        handlers,
    }
}

fn simple_chart(series: SeriesDef) -> ChartContext {
    tooltip_chart(
        &[(series, "0:sales")],
        vec![("0:sales", sales_points())],
        Default::default(),
    )
}

fn card_text(series: &[SeriesDef], chart: &ChartContext, index: usize) -> String {
    let rows = derive_tooltip_series_values(series, chart, chart.data.get(index), gpui::blue());
    let x = chart.data[index].get("month");
    DefaultCard::new(x, &rows, has_multiple_primary_series(series)).text()
}

#[test]
fn the_card_shows_the_hovered_x_and_the_bare_value_of_one_series() {
    let series = sales_line();
    let chart = simple_chart(series.clone());
    assert_eq!(card_text(&[series], &chart, 1), "Feb7");
}

#[test]
fn a_multi_series_card_labels_each_row_in_series_order() {
    let profit_points: Vec<ResolvedPoint> = sales_points()
        .into_iter()
        .map(|p| ResolvedPoint {
            py: p.py + 30.,
            ..p
        })
        .collect();
    let (sales, profit) = (sales_line(), profit_line());
    let chart = tooltip_chart(
        &[(sales.clone(), "0:sales"), (profit.clone(), "1:profit")],
        vec![("0:sales", sales_points()), ("1:profit", profit_points)],
        Default::default(),
    );
    assert_eq!(
        card_text(&[sales.clone(), profit.clone()], &chart, 1),
        "FebSales7Profit2"
    );
    assert_eq!(card_text(&[profit, sales], &chart, 1), "FebProfit2Sales7");
}

#[test]
fn one_series_beside_a_utility_mark_shows_the_bare_value() {
    let sales = bar("sales", BarOptions::new().label("Sales").color(red()));
    let target = reference_line(ReferenceLineOptions {
        label: Some("Target".into()),
        ..ReferenceLineOptions::y(5.)
    });
    let chart = tooltip_chart(
        &[(sales.clone(), "0:sales"), (target.clone(), "1:ref-5")],
        vec![("0:sales", sales_points()), ("1:ref-5", Vec::new())],
        Default::default(),
    );
    assert_eq!(card_text(&[sales, target], &chart, 1), "Feb7");
}

/// A series that reads no value key (upstream's heatmapGL or streamGL beside a bar).
struct Keyless;

impl Series for Keyless {
    fn kind(&self) -> &'static str {
        "streamGL"
    }

    fn key(&self) -> SharedString {
        "stream".into()
    }

    fn data_keys(&self) -> Vec<SharedString> {
        Vec::new()
    }

    fn resolve(
        &self,
        _: &SeriesContext,
        _: Option<&[StackOffset]>,
        _: Option<GroupInfo>,
    ) -> Vec<ResolvedPoint> {
        Vec::new()
    }

    fn render(
        &self,
        _: &[ResolvedPoint],
        _: &SeriesContext,
        _: &SeriesInfo,
        _: &mut Window,
        _: &mut gpui::App,
    ) -> Option<AnyElement> {
        None
    }
}

#[test]
fn a_series_without_a_value_key_has_no_row() {
    for valued in [
        bar("sales", BarOptions::new().label("Sales").color(red())),
        sales_line(),
    ] {
        let keyless: SeriesDef = Rc::new(Keyless);
        let chart = tooltip_chart(
            &[(valued.clone(), "0:sales"), (keyless.clone(), "1:stream")],
            vec![("0:sales", sales_points()), ("1:stream", Vec::new())],
            Default::default(),
        );
        let series = [valued.clone(), keyless];
        let rows = derive_tooltip_series_values(&series, &chart, chart.data.get(1), gpui::blue());
        assert_eq!(
            rows,
            [TooltipSeriesValue {
                key: "sales".into(),
                label: "Sales".into(),
                color: red(),
                kind: valued.kind().into(),
                value: Some(ChartValue::Number(7.)),
            }]
        );
        // Two primary series label the one remaining row.
        assert_eq!(card_text(&series, &chart, 1), "FebSales7");
    }
}

#[test]
fn a_series_the_layout_did_not_resolve_has_no_row() {
    let sales = sales_line();
    let chart = tooltip_chart(
        &[(sales.clone(), "0:sales")],
        Vec::new(),
        Default::default(),
    );
    assert!(
        derive_tooltip_series_values(&[sales], &chart, chart.data.get(1), gpui::blue()).is_empty()
    );
}

fn tokens() -> rustryx_theme::Tokens {
    rustryx_theme::Theme::default().light
}

#[test]
fn a_hover_dot_marks_each_series_point_in_its_color() {
    let series = sales_line();
    let chart = simple_chart(series.clone());
    let tooltip = ChartTooltip::new().series([series]);
    let (indicator, dots) = tooltip.hover_marks(&chart, Some(1), &tokens());
    assert_eq!(
        dots,
        [HoverDot {
            x: 250.,
            y: 40.,
            stroke: red()
        }]
    );
    // A line chart draws a crosshair through the hovered x, the band's center.
    assert_eq!(indicator, Some(Indicator::Crosshair(250.)));
}

#[test]
fn no_hover_dot_where_the_value_is_missing() {
    let series = sales_line();
    let mut gappy = sales_points();
    gappy[1].py = f64::NAN;
    let chart = tooltip_chart(
        &[(series.clone(), "0:sales")],
        vec![("0:sales", gappy)],
        Default::default(),
    );
    let (_, dots) = ChartTooltip::new()
        .series([series])
        .hover_marks(&chart, Some(1), &tokens());
    assert!(dots.is_empty());
}

#[test]
fn bars_get_a_band_highlight_instead_of_a_crosshair_and_no_dot() {
    let series = bar("sales", BarOptions::new().label("Sales").color(red()));
    let chart = simple_chart(series.clone());
    let (indicator, dots) =
        ChartTooltip::new()
            .series([series])
            .hover_marks(&chart, Some(1), &tokens());
    assert!(dots.is_empty(), "bars take the highlight, not a dot");
    // Feb's band starts at 187.5 and is 125 wide; the column pads 8 on each side and
    // reaches 8 above the plot.
    assert_eq!(
        indicator,
        Some(Indicator::Band {
            x: 179.5,
            width: 141.,
            top: 8.
        })
    );
}

#[test]
fn the_indicator_and_dots_can_be_turned_off() {
    let series = sales_line();
    let chart = simple_chart(series.clone());
    let tooltip = ChartTooltip::new()
        .series([series])
        .hover_indicator(false)
        .show_hover_dots(false);
    assert_eq!(
        tooltip.hover_marks(&chart, Some(1), &tokens()),
        (None, Vec::new())
    );
}

#[test]
fn a_stale_index_has_no_rows_and_no_marks() {
    let series = sales_line();
    let chart = simple_chart(series.clone());
    let tooltip = ChartTooltip::new().series([series.clone()]);
    assert_eq!(
        tooltip.hover_marks(&chart, Some(99), &tokens()),
        (None, Vec::new())
    );
    assert!(
        derive_tooltip_series_values(&[series], &chart, chart.data.get(99), gpui::blue())
            .is_empty()
    );
}

#[test]
fn the_card_sits_beside_or_above_the_point_inside_the_viewport() {
    let viewport = gpui::Bounds::new(gpui::point(px(0.), px(0.)), gpui::size(px(1024.), px(768.)));
    // The plot's left margin (48) plus the hovered point's x (250).
    let anchor = gpui::Bounds::new(gpui::point(px(298.), px(24.)), gpui::size(px(0.), px(0.)));
    let card = gpui::size(px(120.), px(40.));
    let x = |placement| f32::from(place_card(anchor, card, viewport, placement).origin.x);
    // Left: 298 - 120 - 8; right: 298 + 8.
    assert_eq!(x(ChartTooltipPlacement::Left), 170.);
    assert_eq!(x(ChartTooltipPlacement::Right), 306.);
    assert_eq!(x(ChartTooltipPlacement::Auto), 306.);
    let top = place_card(anchor, card, viewport, ChartTooltipPlacement::Top);
    // Centered above the point, kept 8 px inside the viewport.
    assert_eq!(f32::from(top.origin.x), 238.);
    assert_eq!(f32::from(top.origin.y), 8.);
    // Auto flips to the left where the right side would overflow.
    let near_edge = gpui::Bounds::new(gpui::point(px(980.), px(24.)), gpui::size(px(0.), px(0.)));
    assert_eq!(
        f32::from(
            place_card(near_edge, card, viewport, ChartTooltipPlacement::Auto)
                .origin
                .x
        ),
        852.
    );
}

/// The calls a tooltip's custom render saw: the x value and the rows.
type Calls = Rc<RefCell<Vec<(Option<ChartValue>, Vec<TooltipSeriesValue>)>>>;

struct TooltipHost {
    tooltip: ChartTooltip,
    handlers: Rc<RefCell<Vec<PointerHandler>>>,
}

impl Render for TooltipHost {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        // Listeners register as the chrome renders, one frame's worth at a time.
        self.handlers.borrow_mut().clear();
        let series: Vec<(SeriesDef, &str)> = self
            .tooltip
            .series
            .iter()
            .map(|s| (s.clone(), "0:sales"))
            .collect();
        let chart = tooltip_chart(
            &series,
            vec![("0:sales", sales_points())],
            self.handlers.clone(),
        );
        div()
            .w(px(600.))
            .h(px(400.))
            .child(rustryx_primitives::provide(
                chart,
                div().w(px(500.)).h(px(200.)).child(self.tooltip.clone()),
            ))
    }
}

fn tooltip_host(
    cx: &mut TestAppContext,
    tooltip: ChartTooltip,
) -> (&mut VisualTestContext, Rc<RefCell<Vec<PointerHandler>>>) {
    cx.update(|cx| {
        rustryx_theme::init(cx);
        rustryx_primitives::focus::init(cx);
    });
    let handlers: Rc<RefCell<Vec<PointerHandler>>> = Default::default();
    let shared = handlers.clone();
    let (_, cx) = cx.add_window_view(move |_, _| TooltipHost {
        tooltip,
        handlers: shared,
    });
    cx.run_until_parked();
    (cx, handlers)
}

fn dispatch(
    cx: &mut VisualTestContext,
    handlers: &Rc<RefCell<Vec<PointerHandler>>>,
    event: ChartPointerEvent,
) {
    cx.update(|window, cx| {
        let listeners = handlers.borrow().clone();
        for handler in listeners {
            handler(&event, window, cx);
        }
    });
    cx.run_until_parked();
}

fn hover_at(index: usize) -> ChartPointerEvent {
    let point = sales_points()[index];
    ChartPointerEvent {
        x: point.px,
        y: 100.,
        nearest: Some(NearestPoint {
            point,
            series_key: "sales".into(),
        }),
        active: false,
    }
}

fn stale_hover() -> ChartPointerEvent {
    ChartPointerEvent {
        x: 0.,
        y: 0.,
        nearest: Some(NearestPoint {
            point: ResolvedPoint {
                px: 50.,
                py: 120.,
                py0: 200.,
                data_index: 99,
            },
            series_key: "sales".into(),
        }),
        active: false,
    }
}

fn pointer_leave() -> ChartPointerEvent {
    ChartPointerEvent {
        x: -1.,
        y: -1.,
        nearest: None,
        active: false,
    }
}

fn card_open(cx: &mut VisualTestContext) -> bool {
    cx.debug_bounds("chart-tooltip-card").is_some()
}

#[gpui::test]
fn hovering_opens_the_card_and_leaving_closes_it(cx: &mut TestAppContext) {
    let (cx, handlers) = tooltip_host(cx, ChartTooltip::new().series([sales_line()]));
    assert!(!card_open(cx), "the card starts closed");
    dispatch(cx, &handlers, hover_at(0));
    assert!(card_open(cx));
    dispatch(cx, &handlers, pointer_leave());
    assert!(!card_open(cx));
}

#[gpui::test]
fn moving_between_points_keeps_the_card_open(cx: &mut TestAppContext) {
    let (cx, handlers) = tooltip_host(cx, ChartTooltip::new().series([sales_line()]));
    dispatch(cx, &handlers, hover_at(0));
    assert!(card_open(cx));
    dispatch(cx, &handlers, hover_at(1));
    assert!(card_open(cx));
    dispatch(cx, &handlers, pointer_leave());
    assert!(!card_open(cx));
    dispatch(cx, &handlers, hover_at(2));
    assert!(card_open(cx));
}

#[gpui::test]
fn a_stale_index_never_opens_the_card(cx: &mut TestAppContext) {
    let (cx, handlers) = tooltip_host(cx, ChartTooltip::new().series([sales_line()]));
    dispatch(cx, &handlers, stale_hover());
    assert!(!card_open(cx));
}

#[gpui::test]
fn a_stale_index_closes_an_open_card_until_valid_data_returns(cx: &mut TestAppContext) {
    let (cx, handlers) = tooltip_host(cx, ChartTooltip::new().series([sales_line()]));
    dispatch(cx, &handlers, hover_at(0));
    assert!(card_open(cx));
    dispatch(cx, &handlers, stale_hover());
    assert!(!card_open(cx));
    dispatch(cx, &handlers, hover_at(1));
    assert!(card_open(cx));
}

fn recording(calls: &Calls, hide_feb: bool) -> ChartTooltip {
    let calls = calls.clone();
    ChartTooltip::new()
        .series([sales_line()])
        .render(move |x, rows, _, _| {
            calls.borrow_mut().push((x.cloned(), rows.to_vec()));
            let hidden = hide_feb && x.map(|v| v.to_js_string()) == Some("Feb".into());
            (!hidden).then(|| {
                div()
                    .debug_selector(|| "custom-body".into())
                    .child("custom body")
                    .into_any_element()
            })
        })
}

#[gpui::test]
fn a_custom_render_replaces_the_body_and_receives_the_x_and_rows(cx: &mut TestAppContext) {
    let calls: Calls = Default::default();
    let (cx, handlers) = tooltip_host(cx, recording(&calls, false));
    dispatch(cx, &handlers, hover_at(1));
    assert!(card_open(cx));
    assert!(cx.debug_bounds("custom-body").is_some());
    let (x, rows) = calls.borrow().last().cloned().unwrap();
    assert_eq!(x, Some(ChartValue::from("Feb")));
    assert_eq!(rows.len(), 1);
    assert_eq!(
        (rows[0].key.as_ref(), rows[0].label.as_ref()),
        ("sales", "Sales")
    );
    assert_eq!(rows[0].value, Some(ChartValue::Number(7.)));
}

#[gpui::test]
fn a_custom_render_returning_none_never_opens_the_card(cx: &mut TestAppContext) {
    let tooltip = ChartTooltip::new()
        .series([sales_line()])
        .render(|_, _, _, _| None);
    let (cx, handlers) = tooltip_host(cx, tooltip);
    dispatch(cx, &handlers, hover_at(1));
    assert!(!card_open(cx));
}

#[gpui::test]
fn a_custom_render_closes_for_none_and_reopens_for_later_content(cx: &mut TestAppContext) {
    let calls: Calls = Default::default();
    let (cx, handlers) = tooltip_host(cx, recording(&calls, true));
    dispatch(cx, &handlers, hover_at(0));
    assert!(card_open(cx));
    dispatch(cx, &handlers, hover_at(1));
    assert!(!card_open(cx));
    dispatch(cx, &handlers, hover_at(2));
    assert!(card_open(cx));
}
