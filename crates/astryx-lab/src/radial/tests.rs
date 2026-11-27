//! Upstream `RadialChart.test.tsx`, `RadialArea.test.tsx`, `RadialAxis.test.tsx`,
//! `RadialGrid.test.tsx`, `RadialSlice.test.tsx` and `RadialTooltip.test.tsx`. Every
//! chart is 600 × 400 unless said: center (300, 200), outer radius 160.

use std::cell::Cell;
use std::rc::Rc;

use astryx_charts::{Datum, datum};
use gpui::{
    AnyElement, Context, Modifiers, Render, TestAppContext, TouchEvent, TouchId, TouchPhase,
    VisualTestContext, Window, div, point, px,
};

use super::*;

fn spider_data() -> Rc<[Datum]> {
    vec![
        datum! { "model" => "A", "speed" => 10, "handling" => 5, "comfort" => 8 },
        datum! { "model" => "B", "speed" => 5, "handling" => 10, "comfort" => 4 },
    ]
    .into()
}

fn pie_data() -> Rc<[Datum]> {
    vec![
        datum! { "region" => "NA", "revenue" => 500 },
        datum! { "region" => "EU", "revenue" => 300 },
        datum! { "region" => "APAC", "revenue" => 200 },
    ]
    .into()
}

fn spider(data: Rc<[Datum]>) -> RadialContext {
    let options = RadialOptions {
        axes: Some(vec!["speed".into(), "handling".into(), "comfort".into()]),
        ..RadialOptions::default()
    };
    radial_context(data, 600., 400., &options)
}

fn pie(data: Rc<[Datum]>, inner: f64) -> RadialContext {
    let options = RadialOptions {
        value_key: Some("revenue".into()),
        label_key: Some("region".into()),
        inner_radius: inner,
        ..RadialOptions::default()
    };
    radial_context(data, 600., 400., &options)
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

#[test]
fn the_context_centers_the_chart_and_leaves_room_for_labels() {
    let ctx = spider(spider_data());
    assert_eq!(
        (ctx.cx, ctx.cy, ctx.radius, ctx.inner_radius),
        (300., 200., 160., 0.)
    );
    assert_eq!(ctx.mode, RadialMode::Spider);
    // From the top, clockwise.
    assert!(close(
        ctx.angle("speed").unwrap(),
        -std::f64::consts::FRAC_PI_2
    ));
    // Domains are floored at zero.
    assert_eq!(ctx.domain("speed"), Some([0., 10.]));
}

#[test]
fn names_the_chart_from_its_axes_its_value_key_or_a_label() {
    let axes = [
        SharedString::from("speed"),
        "handling".into(),
        "comfort".into(),
    ];
    assert_eq!(
        radial_label(None, Some(&axes), None),
        "Radar chart of speed, handling, comfort"
    );
    assert_eq!(
        radial_label(None, None, Some("revenue")),
        "Pie chart of revenue"
    );
    assert_eq!(radial_label(None, None, None), "Pie chart of values");
    assert_eq!(
        radial_label(Some("Revenue by region"), None, Some("revenue")),
        "Revenue by region"
    );
}

#[test]
fn the_pie_table_lists_labels_values_and_percentages() {
    let data: Rc<[Datum]> = vec![
        datum! { "region" => "NA", "revenue" => 60 },
        datum! { "region" => "EU", "revenue" => 40 },
    ]
    .into();
    let ctx = pie(data, 0.);
    let t = radial_table(
        &ctx,
        "Pie chart of revenue",
        Some("region"),
        Some("revenue"),
    )
    .expect("a table");
    assert_eq!(t.caption, "Pie chart of revenue data");
    assert_eq!(t.columns, ["region", "revenue", "Percentage"]);
    assert_eq!(t.rows[0], ["NA", "60", "60.0%"]);
    assert_eq!(t.rows[1], ["EU", "40", "40.0%"]);
}

#[test]
fn the_spider_table_has_a_row_per_series() {
    let data: Rc<[Datum]> = vec![
        datum! { "name" => "modelA", "speed" => 8, "handling" => 6, "comfort" => 7 },
        datum! { "name" => "modelB", "speed" => 5, "handling" => 9, "comfort" => 6 },
    ]
    .into();
    let ctx = spider(data);
    let t = radial_table(&ctx, "x", None, None).expect("a table");
    assert_eq!(t.columns, ["Series", "speed", "handling", "comfort"]);
    assert_eq!(t.rows[0], ["modelA", "8", "6", "7"]);
    assert_eq!(t.rows[1], ["modelB", "5", "9", "6"]);
}

#[test]
fn skips_the_table_past_the_size_cutoff() {
    let big: Rc<[Datum]> = (0..101)
        .map(|i| datum! { "region" => format!("r{i}"), "revenue" => i + 1 })
        .collect::<Vec<_>>()
        .into();
    let ctx = pie(big, 0.);
    assert!(radial_table(&ctx, "x", Some("region"), Some("revenue")).is_none());
}

#[test]
fn an_area_has_a_vertex_per_axis_on_its_row() {
    let ctx = spider(spider_data());
    let a = area_points(&ctx, "A");
    assert_eq!(a.len(), 3);
    // Row A has the speed maximum: the outer radius, straight up.
    assert!(close(a[0].1, 300.) && close(a[0].2, 40.));
    // Row B's speed is half: r = 80.
    let b = area_points(&ctx, "B");
    assert!(close(b[0].1, 300.) && close(b[0].2, 120.));
    // No match: the first row.
    assert!(close(area_points(&ctx, "does-not-exist")[0].2, 40.));
    assert!(
        area_points(&spider(Vec::new().into()), "A").is_empty(),
        "no data, no polygon"
    );
    assert!(
        area_points(&pie(spider_data(), 0.), "A").is_empty(),
        "nothing in pie mode"
    );
}

#[test]
fn axis_labels_sit_beyond_the_ring_and_anchor_by_side() {
    let ctx = spider(spider_data());
    let labels = axis_labels(&ctx, 16.);
    assert_eq!(labels.len(), 3);
    assert_eq!(labels[0].anchor, TextAnchor::Middle);
    assert_eq!(labels[1].anchor, TextAnchor::Start);
    assert_eq!(labels[2].anchor, TextAnchor::End);
    assert!(close(labels[0].x, 300.) && close(labels[0].y, 24.));
    assert!(axis_labels(&pie(spider_data(), 0.), 16.).is_empty());
}

#[test]
fn the_grid_has_a_ring_per_step_and_a_spoke_per_axis() {
    let ctx = spider(spider_data());
    assert_eq!(grid_geometry(&ctx, 4.).unwrap().rings.len(), 4);
    let g = grid_geometry(&ctx, 5.).unwrap();
    assert_eq!(g.rings.len(), 5);
    assert!(g.rings.iter().all(|r| r.len() == 3));
    let outer = g.rings.last().unwrap()[0];
    assert!(close(outer.0, 300.) && close(outer.1, 40.));
    assert_eq!(g.spokes.len(), 3);
    let [from, to] = g.spokes[0];
    assert!(close(from.0, 300.) && close(from.1, 200.) && close(to.1, 40.));
    assert!(grid_geometry(&pie(spider_data(), 0.), 5.).is_none());
    // A fractional count floors (Array.from's length).
    assert_eq!(grid_geometry(&ctx, 2.7).unwrap().rings.len(), 2);
}

#[test]
fn slices_draw_wedges_or_annular_sectors() {
    let ctx = pie(pie_data(), 0.);
    let s = slice_geometry(&ctx, true, 5.);
    assert_eq!(s.len(), 3);
    for slice in &s {
        let a = slice.arc;
        let d = arc_path(a.cx, a.cy, a.inner, a.outer, a.start, a.end);
        assert!(d.starts_with("M 300 200"));
        assert_eq!(d.split("A ").count() - 1, 1);
    }
    let labels: Vec<_> = s
        .iter()
        .filter_map(|s| s.label.clone())
        .map(|l| l.2)
        .collect();
    assert_eq!(labels, ["50%", "30%", "20%"]);
    let donut = pie(pie_data(), 0.6);
    for slice in slice_geometry(&donut, true, 5.) {
        let a = slice.arc;
        let d = arc_path(a.cx, a.cy, a.inner, a.outer, a.start, a.end);
        assert!(!d.starts_with("M 300 200"));
        assert_eq!(d.split("A ").count() - 1, 2);
    }
}

#[test]
fn small_slices_lose_their_labels_and_zeros_draw_nothing() {
    let data: Rc<[Datum]> = vec![
        datum! { "region" => "big", "revenue" => 97 },
        datum! { "region" => "tiny", "revenue" => 3 },
    ]
    .into();
    let labels: Vec<_> = slice_geometry(&pie(data, 0.), true, 5.)
        .into_iter()
        .filter_map(|s| s.label.map(|l| l.2))
        .collect();
    assert_eq!(labels, ["97%"]);
    let zeros: Rc<[Datum]> = vec![
        datum! { "region" => "a", "revenue" => 0 },
        datum! { "region" => "b", "revenue" => 0 },
    ]
    .into();
    assert!(slice_geometry(&pie(zeros, 0.), true, 5.).is_empty());
    assert!(
        slice_geometry(&spider(pie_data()), true, 5.).is_empty(),
        "nothing in spider mode"
    );
}

#[test]
fn coordinates_match_v8_to_the_last_digit() {
    // V8's fdlibm cosine, where a platform libm can be a unit in the last place off
    // (the label of the fourth slice of the stories' regions).
    let data: Rc<[Datum]> = [
        ("North America", 42),
        ("Europe", 28),
        ("Asia Pacific", 18),
        ("Latin America", 8),
        ("Africa", 4),
    ]
    .into_iter()
    .map(|(r, v)| datum! { "region" => r, "revenue" => v })
    .collect::<Vec<_>>()
    .into();
    let labels = slice_geometry(&pie(data, 0.), true, 5.);
    assert_eq!(
        labels[3].label.as_ref().map(|l| l.0),
        Some(251.0837697147317)
    );
    assert_eq!(trig::js_cos(0.), 1.);
    assert_eq!(trig::js_sin(-std::f64::consts::FRAC_PI_2), -1.);
}

#[test]
fn the_pie_hit_test_reads_distance_and_angle() {
    let data: Rc<[Datum]> = vec![
        datum! { "region" => "NA", "revenue" => 500 },
        datum! { "region" => "EU", "revenue" => 300 },
    ]
    .into();
    let ctx = pie(data, 0.);
    assert_eq!(
        hit_test_pie(&ctx, 400., 200.).map(|d| d.label),
        Some("NA".into())
    );
    assert_eq!(
        hit_test_pie(&ctx, 200., 200.).map(|d| d.label),
        Some("EU".into())
    );
    assert!(
        hit_test_pie(&ctx, 300., 30.).is_none(),
        "past the outer radius"
    );
    let donut = pie(pie_data(), 0.6);
    assert!(hit_test_pie(&donut, 310., 200.).is_none(), "in the hole");
}

#[test]
fn the_spider_hit_test_finds_the_nearest_axis_and_its_largest_value() {
    let data: Rc<[Datum]> = vec![
        datum! { "model" => "A", "speed" => 10, "handling" => 5 },
        datum! { "model" => "B", "speed" => 5, "handling" => 10 },
    ]
    .into();
    let options = RadialOptions {
        axes: Some(vec!["speed".into(), "handling".into()]),
        ..RadialOptions::default()
    };
    let ctx = radial_context(data, 600., 400., &options);
    let top = hit_test_spider(&ctx, 300., 100.).expect("near speed");
    assert_eq!(
        (top.label.as_ref(), top.value, top.index),
        ("speed", 10., 0)
    );
    let bottom = hit_test_spider(&ctx, 300., 300.).expect("near handling");
    assert_eq!(
        (bottom.label.as_ref(), bottom.value, bottom.index),
        ("handling", 10., 1)
    );
    assert!(
        hit_test_spider(&ctx, 300., 0.).is_none(),
        "beyond 20 px past the ring"
    );
}

#[test]
#[should_panic(expected = "Radial components must be used inside <RadialChart>")]
fn use_radial_panics_outside_a_chart() {
    gpui::TestAppContext::single().update(|cx| {
        use_radial(cx);
    });
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
        astryx_theme::init(cx);
        astryx_primitives::focus::init(cx);
    });
    let (_, cx) = cx.add_window_view(move |_, _| Host(Rc::new(build)));
    settle(cx);
    cx
}

fn settle(cx: &mut VisualTestContext) {
    for _ in 0..3 {
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
    }
}

#[gpui::test]
fn the_plot_takes_the_container_width_and_the_height(cx: &mut TestAppContext) {
    let cx = host(cx, || {
        div()
            .child(
                RadialChart::new("pie", pie_data())
                    .value_key("revenue")
                    .label_key("region"),
            )
            .child(
                RadialChart::new("radar", spider_data())
                    .axes(["speed", "handling", "comfort"])
                    .height(300.),
            )
            .into_any_element()
    });
    let pie = cx.debug_bounds("pie-radial-plot").expect("drawn");
    assert_eq!(
        (f32::from(pie.size.width), f32::from(pie.size.height)),
        (600., 400.)
    );
    let radar = cx.debug_bounds("radar-radial-plot").expect("drawn");
    assert_eq!(f32::from(radar.size.height), 300.);
    assert!(
        cx.debug_bounds("pie-radial-table").is_some(),
        "the hidden table"
    );
}

fn tooltip_chart(interactive: Rc<Cell<bool>>) -> impl Fn() -> AnyElement {
    move || {
        let data: Rc<[Datum]> = vec![
            datum! { "region" => "NA", "revenue" => 500 },
            datum! { "region" => "EU", "revenue" => 300 },
        ]
        .into();
        RadialChart::new("tip", data)
            .value_key("revenue")
            .label_key("region")
            .interactive(interactive.get())
            .child(RadialSlice::new([gpui::black(), gpui::white()]))
            .child(RadialTooltip::new().render(|d, _, _| {
                let name = format!("tip-{}-{}", d.label, d.value);
                div()
                    .debug_selector(move || name)
                    .child(d.label.clone())
                    .into_any_element()
            }))
            .into_any_element()
    }
}

fn plot_origin(cx: &mut VisualTestContext) -> gpui::Point<gpui::Pixels> {
    cx.debug_bounds("tip-radial-plot").expect("drawn").origin
}

fn touch(cx: &mut VisualTestContext, phase: TouchPhase, at: gpui::Point<gpui::Pixels>) {
    cx.simulate_event(TouchEvent {
        id: TouchId(1),
        phase,
        position: at,
        ..TouchEvent::default()
    });
    settle(cx);
}

#[gpui::test]
fn the_mouse_hovers_slices(cx: &mut TestAppContext) {
    let cx = host(cx, tooltip_chart(Rc::new(Cell::new(false))));
    let o = plot_origin(cx);
    cx.simulate_mouse_move(o + point(px(400.), px(200.)), None, Modifiers::none());
    settle(cx);
    assert!(cx.debug_bounds("tip-NA-500").is_some());
    cx.simulate_mouse_move(o + point(px(200.), px(200.)), None, Modifiers::none());
    settle(cx);
    assert!(cx.debug_bounds("tip-EU-300").is_some());
    // Out of the capture circle (radius + 10): gone.
    cx.simulate_mouse_move(o + point(px(10.), px(10.)), None, Modifiers::none());
    settle(cx);
    assert!(cx.debug_bounds("tip-EU-300").is_none() && cx.debug_bounds("tip-NA-500").is_none());
}

#[gpui::test]
fn a_finger_is_left_alone_unless_the_chart_is_interactive(cx: &mut TestAppContext) {
    let cx = host(cx, tooltip_chart(Rc::new(Cell::new(false))));
    let o = plot_origin(cx);
    touch(cx, TouchPhase::Started, o + point(px(400.), px(200.)));
    assert!(cx.debug_bounds("tip-NA-500").is_none());
    touch(cx, TouchPhase::Ended, o + point(px(400.), px(200.)));
}

#[gpui::test]
fn a_finger_scrubs_an_interactive_chart_and_the_lift_dismisses(cx: &mut TestAppContext) {
    let interactive = Rc::new(Cell::new(true));
    let cx = host(cx, tooltip_chart(interactive.clone()));
    let o = plot_origin(cx);
    touch(cx, TouchPhase::Started, o + point(px(400.), px(200.)));
    assert!(cx.debug_bounds("tip-NA-500").is_some());
    touch(cx, TouchPhase::Moved, o + point(px(200.), px(200.)));
    assert!(cx.debug_bounds("tip-EU-300").is_some());
    touch(cx, TouchPhase::Ended, o + point(px(200.), px(200.)));
    assert!(cx.debug_bounds("tip-EU-300").is_none());
    // Turning interaction off ends an active scrub.
    touch(cx, TouchPhase::Started, o + point(px(400.), px(200.)));
    assert!(cx.debug_bounds("tip-NA-500").is_some());
    interactive.set(false);
    settle(cx);
    assert!(cx.debug_bounds("tip-NA-500").is_none());
}
