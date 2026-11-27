//! The lab chart's tests: upstream lab `Chart.test.tsx`, the marks', chrome's and interactions'
//! tests, `formatters.test.ts`, `getChartColors.test.ts`, `m4.test.ts` and `useChartRange.test.ts`.
//! Upstream reads the rendered SVG; these read the geometry the marks paint, and drive the
//! interactions in a window.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gpui::prelude::*;
use gpui::{
    AnyElement, Context, Modifiers, MouseButton, Render, TestAppContext, VisualTestContext, Window,
    div, point, px,
};

use super::chart::CHARTS;
use super::*;

fn row(fields: &[(&str, ChartValue)]) -> Datum {
    fields
        .iter()
        .fold(Datum::new(), |d, (k, v)| d.with(k.to_string(), v.clone()))
}

fn months() -> Vec<Datum> {
    [("Jan", 10.), ("Feb", 20.), ("Mar", 30.)]
        .iter()
        .map(|(m, r)| row(&[("month", (*m).into()), ("revenue", (*r).into())]))
        .collect()
}

/// A chart's context at upstream's test width (464, a 400 by 252 plot).
fn ctx_of(
    data: Vec<Datum>,
    x_key: &str,
    y_keys: &[&str],
    y_domain: Option<[f64; 2]>,
    x_domain: Option<[f64; 2]>,
) -> ChartContext {
    let y_keys: Vec<gpui::SharedString> = y_keys.iter().map(|k| k.to_string().into()).collect();
    let (width, height) = (464. - 48. - 16., 300. - 16. - 32.);
    let (x, y) = chart_scales(
        &data,
        x_key,
        &y_keys,
        width,
        height,
        YBaseline::Auto,
        y_domain,
        x_domain,
    );
    ChartContext::detached(data.into(), x_key, x, y, width, height, DEFAULT_MARGIN)
}

// ---- Chart ---------------------------------------------------------------------------

#[test]
fn chart_names_its_plot_with_a_default_or_custom_label() {
    let keys: Vec<gpui::SharedString> = vec!["revenue".into(), "profit".into()];
    assert_eq!(
        accessible_label(None, &keys, "month"),
        "Chart of revenue, profit by month"
    );
    assert_eq!(
        accessible_label(Some("Q1 revenue"), &keys[..1], "month"),
        "Q1 revenue"
    );
}

#[test]
fn chart_mirrors_small_data_in_a_table_and_skips_large_data() {
    assert!(shows_data_table(3, 2));
    assert!(shows_data_table(100, 1));
    assert!(!shows_data_table(51, 2), "more than 100 points");
    assert!(!shows_data_table(0, 1), "no rows");
    assert!(shows_data_table(10, 0), "no keys count as one");
}

#[test]
fn chart_builds_band_and_linear_scales_as_upstream() {
    let ctx = ctx_of(months(), "month", &["revenue"], None, None);
    let band = ctx.x_scale.as_band().expect("strings make a band scale");
    assert_eq!(band.get_domain().len(), 3);
    assert_eq!(
        ctx.y_scale.get_domain(),
        [0., 30.],
        "auto includes zero, then nice"
    );
    let numeric: Vec<Datum> = (0..3)
        .map(|i| row(&[("x", f64::from(i).into()), ("y", f64::from(i + 1).into())]))
        .collect();
    let ctx = ctx_of(numeric, "x", &["y"], Some([0., 4.]), Some([0., 2.]));
    assert_eq!(ctx.x_scale.as_linear().unwrap().get_domain(), [0., 2.]);
    assert_eq!(ctx.y_scale.get_domain(), [0., 4.]);
}

#[test]
fn use_chart_reads_nothing_outside_a_chart() {
    let app = gpui::TestAppContext::single();
    app.update(|cx| assert!(use_chart(cx).is_none()));
}

type Build = Rc<dyn Fn() -> AnyElement>;

struct Host(Build);

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().w(px(464.)).child((self.0)())
    }
}

fn host(
    cx: &mut TestAppContext,
    build: impl Fn() -> AnyElement + 'static,
) -> (gpui::Entity<Host>, &mut VisualTestContext) {
    cx.update(|cx| {
        rustryx_theme::init(cx);
        rustryx_primitives::focus::init(cx);
    });
    let (view, cx) = cx.add_window_view(move |_, _| Host(Rc::new(build)));
    cx.update(|window, _| window.activate_window());
    settle(cx);
    (view, cx)
}

fn settle(cx: &mut VisualTestContext) {
    for _ in 0..3 {
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
    }
}

/// Keeps the context a chart gives its children.
#[derive(IntoElement)]
struct Probe(Rc<RefCell<Option<ChartContext>>>);

impl RenderOnce for Probe {
    fn render(self, _: &mut Window, cx: &mut gpui::App) -> impl IntoElement {
        *self.0.borrow_mut() = use_chart(cx);
        div()
    }
}

#[gpui::test]
fn chart_renders_its_plot_inside_the_margins_with_a_label_and_table(cx: &mut TestAppContext) {
    let seen: Rc<RefCell<Option<ChartContext>>> = Rc::default();
    let probe = seen.clone();
    let (_, _cx) = host(cx, move || {
        Chart::new("c", months(), "month", ["revenue"])
            .child(Probe(probe.clone()))
            .into_any_element()
    });
    let ctx = seen
        .borrow()
        .clone()
        .expect("children read the chart's context");
    assert_eq!(
        (ctx.width, ctx.height),
        (400., 252.),
        "the default height less the margins"
    );
    let drawn = CHARTS.with(|c| c.borrow().get("c").cloned()).unwrap();
    assert_eq!(drawn.label, "Chart of revenue by month");
    let (caption, rows) = drawn.table.unwrap();
    assert_eq!(caption, "Chart of revenue by month data");
    assert_eq!(
        rows,
        [
            ["month", "revenue"],
            ["Jan", "10"],
            ["Feb", "20"],
            ["Mar", "30"]
        ]
    );
    let origin = ctx.plot_origin();
    assert_eq!(
        (f64::from(origin.x), f64::from(origin.y)),
        (48., 16.),
        "children draw inside the margin-translated group"
    );
}

#[gpui::test]
fn chart_skips_the_table_for_large_data(cx: &mut TestAppContext) {
    let big: Vec<Datum> = (0..120)
        .map(|i| row(&[("x", f64::from(i).into()), ("y", 1.0.into())]))
        .collect();
    let (_, _cx) = host(cx, move || {
        Chart::new("big", big.clone(), "x", ["y"]).into_any_element()
    });
    assert!(
        CHARTS
            .with(|c| c.borrow().get("big").cloned())
            .unwrap()
            .table
            .is_none()
    );
}

// ---- marks -----------------------------------------------------------------------------

#[test]
fn line_draws_one_path_and_a_dot_per_datum() {
    let ctx = ctx_of(months(), "month", &["revenue"], None, None);
    let (points, path) = line_geometry(&ctx, "revenue");
    assert!(path.to_svg(Some(3)).starts_with('M'));
    assert_eq!(points.len(), 3, "dots, when on, mark each datum");
}

#[test]
fn bar_draws_a_rect_per_datum_from_the_zero_line() {
    let ctx = ctx_of(months(), "month", &["revenue"], None, None);
    assert_eq!(bar_geometry(&ctx, "revenue").len(), 3);
    let pos_neg = vec![
        row(&[("month", "Jan".into()), ("delta", 10.0.into())]),
        row(&[("month", "Feb".into()), ("delta", (-10.0).into())]),
    ];
    let ctx = ctx_of(pos_neg, "month", &["delta"], Some([-20., 20.]), None);
    let bars = bar_geometry(&ctx, "delta");
    assert_eq!(
        (bars[0].y, bars[0].height),
        (63., 63.),
        "a positive bar ends at the zero line"
    );
    assert_eq!(
        (bars[1].y, bars[1].height),
        (126., 63.),
        "a negative bar starts there"
    );
}

#[test]
fn bar_draws_nothing_on_a_linear_x() {
    let numeric: Vec<Datum> = (0..3)
        .map(|i| row(&[("x", f64::from(i).into()), ("y", f64::from(i + 1).into())]))
        .collect();
    assert!(bar_geometry(&ctx_of(numeric, "x", &["y"], None, None), "y").is_empty());
}

#[test]
fn area_fills_between_its_bounds_and_falls_back_to_the_baseline() {
    let data: Vec<Datum> = [(10., 4., 7.), (20., 8., 14.), (30., 12., 21.)]
        .iter()
        .enumerate()
        .map(|(i, (u, l, m))| {
            row(&[
                ("month", ["Jan", "Feb", "Mar"][i].into()),
                ("upper", (*u).into()),
                ("lower", (*l).into()),
                ("mean", (*m).into()),
            ])
        })
        .collect();
    let ctx = ctx_of(data, "month", &["upper", "lower"], None, None);
    assert!(
        area_geometry(&ctx, Some("upper"), Some("lower"))
            .to_svg(Some(3))
            .starts_with('M')
    );
    assert!(
        !area_geometry(&ctx, Some("upper"), Some("mean")).is_empty(),
        "the baseline stands in for the lower bound"
    );
    assert!(
        area_geometry(&ctx, None, None).is_empty(),
        "no bound keys draw nothing"
    );
}

#[test]
fn dots_sit_on_the_linear_scales() {
    let numeric: Vec<Datum> = [(0., 0.), (1., 5.), (2., 10.)]
        .iter()
        .map(|(x, y)| row(&[("x", (*x).into()), ("y", (*y).into())]))
        .collect();
    let ctx = ctx_of(numeric, "x", &["y"], Some([0., 10.]), Some([0., 2.]));
    assert_eq!(
        dot_geometry(&ctx, "y"),
        [(0., 252.), (200., 126.), (400., 0.)]
    );
}

#[test]
fn error_bars_span_the_bounds_with_two_caps() {
    let single = vec![row(&[
        ("month", "Jan".into()),
        ("upper", 75.0.into()),
        ("lower", 25.0.into()),
    ])];
    let ctx = ctx_of(single, "month", &["upper", "lower"], Some([0., 100.]), None);
    let bars = error_bar_geometry(&ctx, "upper", "lower");
    let [stem, top, bottom] = bars[0].segments(8.);
    assert_eq!(((stem.0).1, (stem.1).1), (63., 189.));
    assert_eq!(((top.0).1, (top.1).1), (63., 63.));
    assert_eq!(((bottom.0).1, (bottom.1).1), (189., 189.));
    assert_eq!((top.1).0 - (top.0).0, 8., "caps are cap_width wide");
}

fn ohlc() -> Vec<Datum> {
    [
        ("Mon", 90., 10., 30., 70.),
        ("Tue", 80., 20., 60., 40.),
        ("Wed", 95., 5., 50., 50.),
    ]
    .iter()
    .map(|(d, h, l, o, c)| {
        row(&[
            ("day", (*d).into()),
            ("high", (*h).into()),
            ("low", (*l).into()),
            ("open", (*o).into()),
            ("close", (*c).into()),
        ])
    })
    .collect()
}

#[test]
fn candles_rise_and_fall_with_a_body_and_two_whiskers() {
    let ctx = ctx_of(ohlc(), "day", &["high", "low"], None, None);
    let candles = candlestick_geometry(&ctx, "high", "low", "open", "close");
    assert_eq!(candles.len(), 3);
    assert_eq!(
        candles.iter().map(|c| c.is_up).collect::<Vec<_>>(),
        [true, false, true],
        "a flat candle counts as up"
    );
    let (whiskers, body) = candles[0].default_shapes(0.6);
    assert_eq!(whiskers.len(), 2);
    assert!(body.3 >= 1., "a body is at least a pixel tall");
    assert_eq!(
        candles[1].bar_lines(0.6).len(),
        3,
        "the bar variant: range line and two ticks"
    );
}

#[test]
fn candles_need_a_band_x() {
    let numeric = vec![row(&[
        ("t", 0.0.into()),
        ("high", 90.0.into()),
        ("low", 10.0.into()),
        ("open", 30.0.into()),
        ("close", 70.0.into()),
    ])];
    assert!(
        candlestick_geometry(
            &ctx_of(numeric, "t", &["high", "low"], None, None),
            "high",
            "low",
            "open",
            "close"
        )
        .is_empty()
    );
}

#[test]
fn reference_lines_cross_the_plot_with_an_optional_badge() {
    let ctx = ctx_of(months(), "month", &["revenue"], Some([0., 100.]), None);
    let g =
        reference_line_geometry(&ctx, Some(50.), None, None, ReferenceLabelPosition::End).unwrap();
    assert_eq!((g.from, g.to), ((0., 126.), (400., 126.)));
    assert!(g.badge.is_none());
    let g = reference_line_geometry(
        &ctx,
        Some(50.),
        None,
        Some("Target"),
        ReferenceLabelPosition::End,
    )
    .unwrap();
    let ((x, _, w, h), _) = g.badge.unwrap();
    assert_eq!((w, h), (6. * 5.5 + 8., 14.));
    assert_eq!(x, 400. - w - 2., "the end of the line");
    assert!(
        reference_line_geometry(&ctx, None, Some(1.), None, ReferenceLabelPosition::End).is_none(),
        "x on a band scale draws nothing"
    );
    let numeric: Vec<Datum> = (0..3)
        .map(|i| row(&[("x", f64::from(i).into()), ("y", f64::from(i + 1).into())]))
        .collect();
    let ctx = ctx_of(numeric, "x", &["y"], None, Some([0., 2.]));
    let g =
        reference_line_geometry(&ctx, None, Some(1.), None, ReferenceLabelPosition::End).unwrap();
    assert_eq!((g.from, g.to), ((200., 0.), (200., 252.)));
}

// ---- chrome ----------------------------------------------------------------------------

#[test]
fn grid_lines_follow_the_y_ticks_and_band_centers() {
    let ctx = ctx_of(months(), "month", &["revenue"], Some([0., 4.]), None);
    let (h, v) = grid_geometry(&ctx, true, false);
    assert_eq!(h.len(), 5);
    assert!(v.is_empty());
    let (h, v) = grid_geometry(&ctx, false, true);
    assert!(h.is_empty());
    assert_eq!(v.len(), 3);
    assert_eq!(grid_geometry(&ctx, false, false), (vec![], vec![]));
}

#[test]
fn axis_ticks_label_categories_and_numbers() {
    let ctx = ctx_of(months(), "month", &["revenue"], None, None);
    let labels = |ticks: Vec<AxisTick>| {
        ticks
            .iter()
            .map(|t| t.value.to_js_string().to_string())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        labels(axis_ticks(&ctx, AxisPosition::Bottom, 5., None)),
        ["Jan", "Feb", "Mar"]
    );
    let ctx = ctx_of(months(), "month", &["revenue"], Some([0., 4.]), None);
    assert_eq!(
        labels(axis_ticks(&ctx, AxisPosition::Left, 5., None)),
        ["0", "1", "2", "3", "4"]
    );
    let wide: Vec<Datum> = ["A", "B", "C", "D", "E"]
        .iter()
        .map(|c| row(&[("cat", (*c).into()), ("value", 1.0.into())]))
        .collect();
    let ctx = ctx_of(wide, "cat", &["value"], None, None);
    assert_eq!(
        labels(axis_ticks(&ctx, AxisPosition::Bottom, 5., Some(3))),
        ["A", "C", "E"]
    );
    assert_eq!(truncate_label("January", Some(3)), "Jan\u{2026}");
    assert_eq!(truncate_label("Jan", Some(3)), "Jan");
    assert_eq!(
        truncate_label("January", Some(0)),
        "January",
        "a truncate of 0 is off"
    );
}

#[test]
fn bottom_axis_line_sits_on_the_zero_line_when_y_spans_it() {
    let ctx = ctx_of(months(), "month", &["revenue"], Some([-20., 20.]), None);
    assert_eq!(axis_line_y(&ctx, AxisPosition::Bottom), 126. - 252.);
    assert_eq!(axis_line_y(&ctx, AxisPosition::Left), 0.);
}

#[test]
fn legend_gradient_ticks_are_evenly_spaced() {
    assert_eq!(gradient_ticks([0., 100.], 3), [0., 50., 100.]);
    assert_eq!(gradient_ticks([-50., 50.], 5), [-50., -25., 0., 25., 50.]);
}

#[test]
fn tooltip_snaps_to_the_nearest_datum() {
    let ctx = ctx_of(months(), "month", &["revenue"], Some([0., 30.]), None);
    let band = ctx.x_scale.as_band().unwrap();
    let feb = band.scale("Feb").unwrap() + band.bandwidth() / 2.;
    let (index, p) = find_nearest(&ctx, feb + 3., 0.).unwrap();
    assert_eq!(index, 1);
    assert_eq!((p.px, p.y), (feb, 20.));
    assert_eq!(p.x, Some(ChartValue::Text("Feb".into())));
    assert!(find_nearest(&ctx_of(vec![], "month", &["revenue"], None, None), 0., 0.).is_none());
}

// ---- interactions ----------------------------------------------------------------------

#[test]
fn brush_rects_and_outcomes() {
    let numeric: Vec<Datum> = (0..=10)
        .map(|i| row(&[("x", f64::from(i).into()), ("y", f64::from(i).into())]))
        .collect();
    let ctx = ctx_of(numeric, "x", &["y"], Some([0., 10.]), Some([0., 10.]));
    assert_eq!(
        brush_rect(BrushMode::X, (100., 50.), (40., 80.), 252.),
        BrushRect {
            x0: 40.,
            y0: 0.,
            x1: 100.,
            y1: 252.
        }
    );
    assert_eq!(
        brush_outcome(
            &ctx,
            BrushMode::X,
            &BrushRect {
                x0: 10.,
                y0: 0.,
                x1: 12.,
                y1: 252.
            }
        ),
        BrushOutcome::Clear
    );
    let BrushOutcome::Brush(range, selected) = brush_outcome(
        &ctx,
        BrushMode::X,
        &BrushRect {
            x0: 80.,
            y0: 0.,
            x1: 200.,
            y1: 252.,
        },
    ) else {
        panic!("a range");
    };
    assert_eq!(range.x, [2., 5.]);
    assert_eq!(selected.len(), 4, "x 2 to 5");
    let BrushOutcome::Brush(range, selected) = brush_outcome(
        &ctx,
        BrushMode::Xy,
        &BrushRect {
            x0: 0.,
            y0: 126.,
            x1: 400.,
            y1: 252.,
        },
    ) else {
        panic!("a rectangle");
    };
    assert_eq!(range.y, Some([0., 5.]));
    assert_eq!(selected.len(), 6, "y 0 to 5");
    let band = ctx_of(months(), "month", &["revenue"], None, None);
    assert_eq!(
        brush_outcome(
            &band,
            BrushMode::X,
            &BrushRect {
                x0: 0.,
                y0: 0.,
                x1: 100.,
                y1: 252.
            }
        ),
        BrushOutcome::Ignored
    );
}

#[test]
fn select_picks_within_30px_and_toggles_with_shift() {
    let ctx = ctx_of(months(), "month", &["revenue"], Some([0., 30.]), None);
    let rings = selection_rings(&ctx, &[0, 2]);
    assert_eq!(rings.len(), 2);
    assert!(
        selection_rings(&ctx, &[5]).is_empty(),
        "an out-of-range index rings nothing"
    );
    let (x, y) = rings[0];
    assert_eq!(select_nearest(&ctx, x + 5., y + 5.), Some(0));
    assert_eq!(
        select_nearest(&ctx, x, y + 40.),
        None,
        "more than 30 px away"
    );
    assert_eq!(next_selection(&[0], 2, false), [2]);
    assert_eq!(next_selection(&[0], 2, true), [0, 2]);
    assert_eq!(next_selection(&[0, 2], 2, true), [0]);
}

#[test]
fn zoom_scales_around_the_pivot_and_pans_with_the_drag() {
    assert_eq!(zoom_axis([0., 10.], 0.5, 5.), [2.5, 7.5]);
    assert_eq!(
        zoom_axis([0., 10.], 2., 0.),
        [0., 20.],
        "the pivot stays put"
    );
    assert_eq!(
        wheel_factor(100., 0.1),
        1.1,
        "a delta past 50 px zooms one tick"
    );
    assert_eq!(wheel_factor(-25., 0.1), 0.95);
    assert_eq!(
        pan_domain([0., 10.], 40., 400., true),
        [-1., 9.],
        "x moves against the drag"
    );
    assert_eq!(
        pan_domain([0., 10.], 126., 252., false),
        [5., 15.],
        "y moves with it"
    );
}

#[test]
fn gl_dots_and_picking() {
    let ctx = ctx_of(months(), "month", &["revenue"], None, None);
    assert_eq!(dot_gl_positions(&ctx, "revenue").len(), 3);
    assert_eq!(dot_gl_radius(6., 1.), Some(6. * 4. / 0.96 * 0.48 / 4.));
    assert_eq!(dot_gl_radius(0., 1.), None);
    assert_eq!(index_to_color(0), [0, 0, 1]);
    assert_eq!(color_to_index(0, 0, 1), Some(0));
    assert_eq!(color_to_index(0, 0, 0), None, "the background");
    assert_eq!(color_to_index(1, 0, 0), Some(65_535));
    let points = [[10., 10.], [12., 10.]];
    assert_eq!(
        pick_point(&points, 6., 1., (11., 10.)),
        Some(1),
        "the last point drawn wins"
    );
    assert_eq!(pick_point(&points, 6., 1., (40., 40.)), None);
}

#[test]
fn heatmap_cells_fill_their_steps_on_the_ramp() {
    let data: Vec<Datum> = [("a", "r1", 0.), ("b", "r1", 5.), ("a", "r2", 10.)]
        .iter()
        .map(|(x, y, v)| {
            row(&[
                ("col", (*x).into()),
                ("row", (*y).into()),
                ("v", (*v).into()),
            ])
        })
        .collect();
    let ctx = ctx_of(data, "col", &["v"], None, None);
    let ramp = [[0., 0., 0.], [1., 1., 1.]];
    let domain = heatmap_domain(&ctx, "v", None);
    assert_eq!(domain, [0., 10.]);
    let cells = heatmap_cells(&ctx, "col", "row", "v", &ramp, domain, 1.);
    assert_eq!(cells.len(), 3);
    assert_eq!(cells[1].color, [0.5, 0.5, 0.5]);
    let band = ctx.x_scale.as_band().unwrap();
    assert!(
        (cells[0].x1 - cells[0].x0 - (band.step() - 1.)).abs() < 1e-9,
        "a step wide, less the gap"
    );
    assert_eq!(sample_ramp(&ramp, 2.), Some([1., 1., 1.]), "clamped");
    assert_eq!(sample_ramp(&[], 0.5), None);
}

#[test]
fn stream_handle_keeps_the_newest_points() {
    let mut ring = StreamRing::new(3);
    for i in 0..5 {
        ring.push(f64::from(i), f64::from(i * 10));
    }
    assert_eq!(ring.points(), [(2., 20.), (3., 30.), (4., 40.)]);
    ring.clear();
    assert!(ring.is_empty());
    let handle = ChartStreamGlHandle::new();
    handle.push(1., 1.);
    assert!(
        handle.is_empty(),
        "pushes before the line draws are dropped, as upstream's ref is empty"
    );
}

// ---- utilities -------------------------------------------------------------------------

#[test]
fn m4_reduces_to_four_points_a_column() {
    assert!(m4_reduce(&[], 100., None).is_empty());
    let small = [
        M4Point::new(0., 1.),
        M4Point::new(1., 2.),
        M4Point::new(2., 3.),
    ];
    assert_eq!(m4_reduce(&small, 100., None), small);
    let big: Vec<M4Point> = (0..10_000)
        .map(|i| M4Point::new(f64::from(i), (f64::from(i) * 0.01).sin() * 100.))
        .collect();
    let reduced = m4_reduce(&big, 100., None);
    assert!(!reduced.is_empty() && reduced.len() <= 400);
    let spike: Vec<M4Point> = (0..1000)
        .map(|i| M4Point::new(f64::from(i), if i == 500 { 999. } else { 0. }))
        .collect();
    assert_eq!(
        m4_reduce(&spike, 10., None)
            .iter()
            .map(|p| p.y)
            .fold(f64::MIN, f64::max),
        999.
    );
    let ramp: Vec<M4Point> = (0..1000)
        .map(|i| M4Point::new(f64::from(i), f64::from(i)))
        .collect();
    let reduced = m4_reduce(&ramp, 10., None);
    assert_eq!((reduced[0].x, reduced[reduced.len() - 1].x), (0., 999.));
    assert_eq!(
        m4_reduce(&[M4Point::new(5., 10.)], 100., None),
        [M4Point::new(5., 10.)]
    );
    let shifted: Vec<M4Point> = (0..1000)
        .map(|i| M4Point::new(f64::from(i + 500), f64::from(i)))
        .collect();
    let reduced = m4_reduce(&shifted, 10., Some([500., 1499.]));
    assert!(!reduced.is_empty() && reduced.len() <= 40);
}

#[test]
fn chart_range_warms_up_slides_tracks_and_resets() {
    let mut r = ChartRangeState::new(UseChartRangeOptions::new(300.));
    assert_eq!((r.x_domain(), r.y_domain()), ([0., 0.], [0., 1.]));
    let fixed = ChartRangeState::new(UseChartRangeOptions {
        y_domain: Some([0., 100.]),
        ..UseChartRangeOptions::new(300.)
    });
    assert_eq!(fixed.y_domain(), [0., 100.]);
    let mut w = ChartRangeState::new(UseChartRangeOptions::new(100.));
    w.push(50., 10.);
    assert_eq!(w.x_domain(), [0., 50.]);
    w.push(80., 20.);
    assert_eq!(w.x_domain(), [0., 80.]);
    w.push(150., 20.);
    assert_eq!(w.x_domain(), [50., 150.]);
    let mut y = ChartRangeState::new(UseChartRangeOptions::new(100.));
    y.push(1., 50.);
    assert!(y.y_domain()[0] < 50. && y.y_domain()[1] > 50.);
    y.push(2., 100.);
    assert!((y.y_domain()[0] - 45.).abs() < 0.5 && (y.y_domain()[1] - 105.).abs() < 0.5);
    let mut f = ChartRangeState::new(UseChartRangeOptions {
        y_domain: Some([0., 100.]),
        ..UseChartRangeOptions::new(100.)
    });
    f.push(1., 200.);
    assert_eq!(f.y_domain(), [0., 100.]);
    let mut c = ChartRangeState::new(UseChartRangeOptions {
        y_center: true,
        y_padding: 0.,
        ..UseChartRangeOptions::new(100.)
    });
    c.push(1., -30.);
    c.push(2., 10.);
    assert_eq!(c.y_domain(), [-30., 30.]);
    r.push(500., 99.);
    assert_eq!(r.x_domain()[1], 500.);
    r.reset();
    assert_eq!((r.x_domain(), r.y_domain()), ([0., 0.], [0., 1.]));
}

#[test]
fn formatters_abridge_and_write_dates() {
    assert_eq!(compact_number(&1200.0.into()), "1.2K");
    assert_eq!(compact_number(&1_500_000.0.into()), "1.5M");
    assert_eq!(compact_number(&"abc".into()), "abc");
    assert_eq!(percent(&0.45.into()), "45%");
    assert_eq!(currency("$", None)(&1500.0.into()), "$1.5K");
    assert_eq!(
        currency("\u{20ac}", Some("de-DE"))(&12.5.into()),
        "\u{20ac}12,5"
    );
    assert_eq!(currency("\u{20ac}", None)(&12.5.into()), "\u{20ac}12.5");
    let iso = rustryx_i18n::CalendarSystem::from_identifier("iso8601");
    let day = ChartValue::Date(1_793_361_600_000.);
    assert_eq!(
        month_year(&day, Some("en"), iso),
        month_year(&day, Some("en"), None),
        "iso8601 is written as Gregorian"
    );
}

#[test]
fn colors_alpha_and_ramps() {
    let colors = get_chart_colors_from_resolver(|name: &str| (!name.is_empty()).then(gpui::black));
    let c = colors.alpha(gpui::rgb(0x0064e0).into(), 0.2);
    assert!((c.a - 0.2).abs() < 1e-4);
    assert_eq!(
        colors.categorical(12).len(),
        10,
        "the lab's palette does not wrap"
    );
    assert_eq!(
        colors.sequential(SequentialHue::Blue, 7).len(),
        5,
        "nor interpolate past its five stops"
    );
    assert_eq!(colors.diverging().positive_negative(3).len(), 3);
}

// ---- rendered interactions --------------------------------------------------------------

fn numeric_chart(
    id: &'static str,
    child: impl Fn() -> AnyElement + 'static,
) -> impl Fn() -> AnyElement + 'static {
    move || {
        let data: Vec<Datum> = (0..=10)
            .map(|i| row(&[("x", f64::from(i).into()), ("y", f64::from(i).into())]))
            .collect();
        Chart::new(id, data, "x", ["y"])
            .x_domain(0., 10.)
            .y_domain(0., 10.)
            .interactive(true)
            .child(child())
            .into_any_element()
    }
}

#[gpui::test]
fn tooltip_shows_a_card_for_the_hovered_point(cx: &mut TestAppContext) {
    let (_, cx) = host(
        cx,
        numeric_chart("tip", || ChartTooltip::new().into_any_element()),
    );
    let surface = cx
        .debug_bounds("lab-chart-tooltip-capture")
        .expect("a full-plot pointer capture surface");
    assert_eq!(
        (
            f64::from(surface.size.width),
            f64::from(surface.size.height)
        ),
        (400., 252.)
    );
    assert!(
        cx.debug_bounds("lab-chart-tooltip-card").is_none(),
        "no card before the pointer"
    );
    cx.simulate_mouse_move(surface.center(), None, Modifiers::none());
    settle(cx);
    assert!(cx.debug_bounds("lab-chart-tooltip-card").is_some());
}

#[gpui::test]
fn brush_drag_reports_the_range(cx: &mut TestAppContext) {
    let got: Rc<RefCell<Option<(BrushRange, usize)>>> = Rc::default();
    let sink = got.clone();
    let (_, cx) = host(
        cx,
        numeric_chart("brush", move || {
            let sink = sink.clone();
            ChartBrush::new()
                .on_brush(move |r, d, _, _| *sink.borrow_mut() = Some((*r, d.len())))
                .into_any_element()
        }),
    );
    let surface = cx.debug_bounds("lab-chart-brush-capture").unwrap();
    let at = |x: f32| point(surface.origin.x + px(x), surface.center().y);
    cx.simulate_mouse_down(at(80.), MouseButton::Left, Modifiers::none());
    settle(cx);
    cx.simulate_mouse_move(at(200.), Some(MouseButton::Left), Modifiers::none());
    settle(cx);
    cx.simulate_mouse_up(at(200.), MouseButton::Left, Modifiers::none());
    settle(cx);
    let (range, n) = got.borrow().expect("a finished drag brushes");
    assert_eq!(range.x, [2., 5.]);
    assert_eq!(n, 4);
}

#[gpui::test]
fn zoom_toolbar_resets_to_the_first_domains(cx: &mut TestAppContext) {
    let domains: Rc<RefCell<Vec<[f64; 2]>>> = Rc::default();
    let sink = domains.clone();
    let zoomed = Rc::new(Cell::new(false));
    let shown = zoomed.clone();
    let (view, cx) = host(cx, move || {
        let data: Vec<Datum> = (0..3)
            .map(|i| row(&[("x", f64::from(i).into()), ("y", f64::from(i + 1).into())]))
            .collect();
        let (xd, yd) = if shown.get() {
            ([0.5, 1.5], [1., 3.])
        } else {
            ([0., 2.], [0., 4.])
        };
        let (sx, sy) = (sink.clone(), sink.clone());
        Chart::new("zoom", data, "x", ["y"])
            .x_domain(xd[0], xd[1])
            .y_domain(yd[0], yd[1])
            .child(
                ChartZoom::new()
                    .on_x_domain_change(move |d, _, _| sx.borrow_mut().push(d))
                    .on_y_domain_change(move |d, _, _| sy.borrow_mut().push(d)),
            )
            .into_any_element()
    });
    assert!(cx.debug_bounds("lab-chart-zoom-surface").is_some());
    let bar = cx
        .debug_bounds("lab-chart-zoom-toolbar")
        .expect("the toolbar is in the chart's container");
    zoomed.set(true);
    view.update(cx, |_, cx| cx.notify());
    settle(cx);
    // The third button: Reset zoom.
    let h = f32::from(bar.size.height);
    let each = (h - 8.) / 3.;
    cx.simulate_click(
        point(
            bar.center().x,
            bar.origin.y + px(2. * each + 8. + each / 2.),
        ),
        Modifiers::none(),
    );
    settle(cx);
    let seen = domains.borrow();
    assert_eq!(
        seen[seen.len() - 2..],
        [[0., 2.], [0., 4.]],
        "the first render's domains"
    );
}

#[gpui::test]
fn zoom_without_a_toolbar_has_no_buttons(cx: &mut TestAppContext) {
    let (_, cx) = host(
        cx,
        numeric_chart("bare", || ChartZoom::new().toolbar(None).into_any_element()),
    );
    assert!(cx.debug_bounds("lab-chart-zoom-toolbar").is_none());
}

#[gpui::test]
fn select_press_selects_the_nearest_point(cx: &mut TestAppContext) {
    let got: Rc<RefCell<Vec<Vec<usize>>>> = Rc::default();
    let sink = got.clone();
    let (_, cx) = host(
        cx,
        numeric_chart("select", move || {
            let sink = sink.clone();
            ChartSelect::new()
                .on_selection_change(move |s, _, _| sink.borrow_mut().push(s))
                .into_any_element()
        }),
    );
    let surface = cx.debug_bounds("lab-chart-select-capture").unwrap();
    // x 5, y 5: the plot's center.
    cx.simulate_click(surface.center(), Modifiers::none());
    settle(cx);
    assert_eq!(*got.borrow(), [vec![5]]);
}
