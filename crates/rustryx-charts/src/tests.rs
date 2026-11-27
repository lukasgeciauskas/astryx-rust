//! Chart tests (upstream `Chart.test.tsx`, `ChartGrid.test.tsx`, `ChartAxis.test.tsx`
//! and the layout cases of `marks.test.tsx`).

use std::rc::Rc;

use gpui::{
    AnyElement, Context, IntoElement, Modifiers, ParentElement, Render, Styled, TestAppContext,
    VisualTestContext, Window, div, point, px,
};

use crate::chart::{accessible_label, palette_colors, shows_data_table, table_keys};
use crate::layout::{LayoutInput, LayoutResult, compute_layout};
use crate::*;

fn data() -> Rc<[Datum]> {
    [("Jan", 100, 20), ("Feb", 200, 40), ("Mar", 150, 30)]
        .into_iter()
        .map(|(m, r, p)| datum! { "month" => m, "revenue" => r, "profit" => p })
        .collect()
}

fn layout(
    data: &Rc<[Datum]>,
    x_key: &str,
    series: &[SeriesDef],
    x_domain: Option<[f64; 2]>,
) -> LayoutResult {
    compute_layout(LayoutInput {
        data,
        x_key: &x_key.into(),
        series,
        width: 600.,
        height: 300.,
        y_baseline: None,
        y_domain: None,
        x_domain,
    })
}

#[test]
fn a_categorical_x_ignores_x_domain() {
    let data = data();
    let result = layout(
        &data,
        "month",
        &[bar("revenue", BarOptions::new())],
        Some([0., 10.]),
    );
    let band = result
        .x_scale
        .as_band()
        .expect("categories keep their band scale");
    assert_eq!(band.get_domain(), ["Jan", "Feb", "Mar"]);
    let points = &result.resolved[0].1;
    assert_eq!(points.len(), 3);
    assert!(points.iter().all(|p| p.px.is_finite() && p.py.is_finite()));
}

#[test]
fn x_domain_keeps_dates_and_numeric_strings_on_a_linear_window() {
    // Date.UTC(2026, 0, 1)
    let start = 1_767_225_600_000.;
    let hour = 3_600_000.;
    let as_date = |ms: f64| ChartValue::Date(ms);
    let as_text = |ms: f64| ChartValue::from(ChartValue::Number(ms).to_js_string());
    for to_x in [&as_date as &dyn Fn(f64) -> ChartValue, &as_text] {
        let data: Rc<[Datum]> = [(start, 1), (start + hour, 2), (start + 3. * hour, 3)]
            .into_iter()
            .map(|(t, v)| Datum::new().with("t", to_x(t)).with("v", v))
            .collect();
        let result = layout(
            &data,
            "t",
            &[line("v", LineOptions::new())],
            Some([start, start + 3. * hour]),
        );
        let linear = result
            .x_scale
            .as_linear()
            .expect("the window places readings by time");
        assert!((linear.scale(start + hour) - 200.).abs() < 1e-6);
    }
}

#[test]
fn a_linear_x_range_is_inset_by_the_widest_overhang() {
    let data: Rc<[Datum]> = [
        datum! { "i" => 0, "v" => 1 },
        datum! { "i" => 13, "v" => 2 },
    ]
    .into();
    let range = |series: &[SeriesDef]| {
        layout(&data, "i", series, Some([0., 13.]))
            .x_scale
            .as_linear()
            .unwrap()
            .get_range()
    };
    assert_eq!(range(&[line("v", LineOptions::new())]), [0., 600.]);
    assert_eq!(
        range(&[line("v", LineOptions::new().dots(true))]),
        [3., 597.]
    );
    assert_eq!(
        range(&[
            line("v", LineOptions::new().dots(true)),
            dot("v", DotOptions::new().radius(6.))
        ]),
        [6., 594.]
    );
}

#[test]
fn bars_pin_zero_and_lines_get_headroom() {
    let data = data();
    let bars = layout(&data, "month", &[bar("revenue", BarOptions::new())], None);
    assert_eq!(bars.y_scale.get_domain()[0], 0.);
    let lines = layout(&data, "month", &[line("revenue", LineOptions::new())], None);
    let [lo, hi] = lines.y_scale.get_domain();
    assert!(
        lo > 0. && lo < 100.,
        "headroom below the data, not zero: {lo}"
    );
    assert!(hi > 200.);
}

#[test]
fn stacked_series_span_their_running_total_and_the_top_rounds() {
    let data = data();
    let series = [
        bar("revenue", BarOptions::new().stack("s")),
        bar("profit", BarOptions::new().stack("s")),
    ];
    let result = layout(&data, "month", &series, None);
    assert!(result.y_scale.get_domain()[1] >= 240.);
    assert!(!result.info[0].is_top_of_stack);
    assert!(result.info[1].is_top_of_stack);
    // The top layer starts where the bottom one ends.
    let (bottom, top) = (&result.resolved[0].1, &result.resolved[1].1);
    for (b, t) in bottom.iter().zip(top) {
        assert_eq!(b.py, t.py0);
    }
}

#[test]
fn grouped_bars_take_slots_and_series_keep_unique_ids() {
    let data = data();
    let series = [
        bar("revenue", BarOptions::new().group("g")),
        bar("revenue", BarOptions::new().group("g").label("Again")),
        bar("profit", BarOptions::new().group("g")),
    ];
    let result = layout(&data, "month", &series, None);
    let uids: Vec<_> = result.info.iter().map(|i| i.uid.to_string()).collect();
    assert_eq!(uids, ["0:revenue", "1:revenue", "2:profit"]);
    // Two bars with one key share a slot, as upstream's slot key is the series key.
    let groups: Vec<_> = result.info.iter().map(|i| i.group.unwrap()).collect();
    assert_eq!(groups[0], GroupInfo { index: 0, count: 2 });
    assert_eq!(groups[1], GroupInfo { index: 0, count: 2 });
    assert_eq!(groups[2], GroupInfo { index: 1, count: 2 });
}

#[test]
fn y_baselines_and_an_exact_y_domain() {
    let data: Rc<[Datum]> = [
        datum! { "x" => "a", "v" => -20 },
        datum! { "x" => "b", "v" => 50 },
    ]
    .into();
    let with = |baseline: Option<YBaseline>, domain: Option<[f64; 2]>| {
        compute_layout(LayoutInput {
            data: &data,
            x_key: &"x".into(),
            series: &[line("v", LineOptions::new())],
            width: 600.,
            height: 300.,
            y_baseline: baseline,
            y_domain: domain,
            x_domain: None,
        })
        .y_scale
        .get_domain()
    };
    let [lo, hi] = with(Some(YBaseline::Zero), None);
    assert_eq!(lo, -hi, "symmetric around zero");
    assert_eq!(with(Some(YBaseline::Data), None), [-20., 50.]);
    assert_eq!(
        with(None, Some([-3.3, 7.7])),
        [-3.3, 7.7],
        "exact, not rounded"
    );
}

#[test]
fn the_accessible_name_is_the_title_or_derived() {
    let i18n = rustryx_i18n::english();
    let series = [
        bar("revenue", BarOptions::new()),
        bar("profit", BarOptions::new()),
    ];
    assert_eq!(
        accessible_label(None, &series, "month", &i18n),
        "Chart of revenue and profit by month"
    );
    let title: gpui::SharedString = "Quarterly revenue".into();
    assert_eq!(
        accessible_label(Some(&title), &series, "month", &i18n),
        "Quarterly revenue"
    );
    // Utility marks are not named.
    let only_ref = [reference_line(ReferenceLineOptions::y(1.))];
    assert_eq!(accessible_label(None, &only_ref, "month", &i18n), "Chart");
}

#[test]
fn the_data_table_mirrors_small_datasets_only() {
    let series = [
        bar("revenue", BarOptions::new()),
        bar("profit", BarOptions::new()),
    ];
    assert_eq!(table_keys(&series), ["revenue", "profit"]);
    assert!(shows_data_table(3, 2));
    assert!(!shows_data_table(101, 1));
    assert!(!shows_data_table(0, 2));
}

#[test]
fn series_without_a_color_take_distinct_palette_colors() {
    let red = gpui::red();
    let palette = |n: usize| {
        (0..n)
            .map(|i| gpui::hsla(i as f32 / 10., 0.5, 0.5, 1.))
            .collect()
    };
    let series = [
        line("a", LineOptions::new()),
        line("b", LineOptions::new().color(red)),
        line("c", LineOptions::new()),
        reference_line(ReferenceLineOptions::y(1.)),
    ];
    let colors = palette_colors(&series, palette);
    assert!(colors[0].is_some() && colors[2].is_some());
    assert_ne!(colors[0], colors[2]);
    assert_eq!(colors[1], None, "a static color keeps its own");
    assert_eq!(colors[3], None, "utility marks take no palette slot");
}

#[test]
fn legend_items_label_each_primary_series() {
    let series = [
        bar("revenue", BarOptions::new().label("Revenue")),
        line("profit", LineOptions::new()),
        reference_line(ReferenceLineOptions::y(1.)),
    ];
    let accent = gpui::blue();
    let items = derive_legend_items(&series, &[None, None, None], accent);
    let labels: Vec<_> = items.iter().map(|i| i.label.to_string()).collect();
    assert_eq!(labels, ["Revenue", "profit"]);
    assert_eq!(items[0].kind.as_deref(), Some("bar"));
}

#[test]
fn the_axis_thins_labels_keeping_the_last() {
    let ticks: Vec<u32> = (0..10).collect();
    assert_eq!(crate::axis::thin_ticks(&ticks, 4), [0, 3, 6, 9]);
    // The last replaces a kept tick closer than a step.
    assert_eq!(crate::axis::thin_ticks(&ticks[..8], 3), [0, 3, 7]);
    assert_eq!(crate::axis::thin_ticks(&ticks, 1), [0]);
    assert_eq!(crate::axis::thin_ticks(&ticks, 20), ticks);
}

struct Host(Rc<dyn Fn() -> AnyElement>);

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().w(px(600.)).child((self.0)())
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

#[gpui::test]
fn the_pointer_layer_fills_the_plot_inside_the_default_margins(cx: &mut TestAppContext) {
    let cx = host(cx, || {
        Chart::new("c", data(), "month")
            .series([bar("revenue", BarOptions::new())])
            .height(300.)
            .into_any_element()
    });
    cx.run_until_parked();
    let bounds = cx
        .debug_bounds("chart-pointer")
        .expect("the pointer layer is drawn");
    // 600 - 48 - 24 = 528 wide; 300 - 24 - 32 = 244 tall.
    assert_eq!(bounds.size, gpui::size(px(528.), px(244.)));
}

#[gpui::test]
fn hovering_the_plot_opens_the_tooltip_card(cx: &mut TestAppContext) {
    let cx = host(cx, || {
        Chart::new("c", data(), "month")
            .series([bar("revenue", BarOptions::new())])
            .tooltip(true)
            .into_any_element()
    });
    assert!(
        cx.debug_bounds("chart-tooltip-card").is_none(),
        "no card before a hover"
    );
    let plot = cx.debug_bounds("chart-pointer").unwrap();
    cx.simulate_mouse_move(plot.center(), None, Modifiers::none());
    cx.run_until_parked();
    assert!(
        cx.debug_bounds("chart-tooltip-card").is_some(),
        "the card follows the pointer"
    );
    cx.simulate_mouse_move(point(px(1.), px(1.)), None, Modifiers::none());
    cx.run_until_parked();
    assert!(
        cx.debug_bounds("chart-tooltip-card").is_none(),
        "leaving the plot closes it"
    );
}

#[gpui::test]
fn without_tooltip_no_card_opens(cx: &mut TestAppContext) {
    let cx = host(cx, || {
        Chart::new("c", data(), "month")
            .series([bar("revenue", BarOptions::new())])
            .into_any_element()
    });
    let plot = cx.debug_bounds("chart-pointer").unwrap();
    cx.simulate_mouse_move(plot.center(), None, Modifiers::none());
    cx.run_until_parked();
    assert!(cx.debug_bounds("chart-tooltip-card").is_none());
}
