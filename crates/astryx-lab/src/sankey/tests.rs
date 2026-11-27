//! Upstream `SankeyChart.test.tsx`, `SankeyGrid.test.tsx`, `SankeyLabel.test.tsx`,
//! `SankeyLink.test.tsx`, `SankeyNode.test.tsx` and `useSankey.test.tsx`.

use std::rc::Rc;

use gpui::{AnyElement, Context, Render, TestAppContext, VisualTestContext, Window, div, px};

use super::*;

/// Three nodes of 8 in a chain, colored when `colored`.
fn chain(colored: bool) -> (Vec<SankeyNodeDatum>, Vec<SankeyLinkDatum>) {
    let colors = [[0.5, 0.1, 100.], [0.6, 0.2, 200.], [0.7, 0.3, 300.]];
    let nodes = ["a", "b", "c"]
        .iter()
        .zip(colors)
        .map(|(id, color)| {
            let node = SankeyNodeDatum::new(*id, id.to_uppercase(), 8.);
            if colored { node.color(color) } else { node }
        })
        .collect();
    let links = vec![
        SankeyLinkDatum::new("a", "b", 4.),
        SankeyLinkDatum::new("b", "c", 4.),
    ];
    (nodes, links)
}

fn rich_columns() -> Vec<SankeyColumn> {
    vec![
        SankeyColumn::labeled("Source", ["a"]),
        SankeyColumn::labeled("Middle", ["b"]),
        SankeyColumn::labeled("Target", ["c"]),
    ]
}

/// The context a chart of `height` in a container `width` wide gives its marks.
fn context(
    nodes: &[SankeyNodeDatum],
    links: &[SankeyLinkDatum],
    columns: Option<&[SankeyColumn]>,
    width: f64,
    height: f64,
) -> SankeyContext {
    let (width, _) = chart_width(width, column_count(columns, nodes, links), 160.);
    let layout = compute_layout(
        nodes,
        links,
        &LayoutOptions {
            width,
            height,
            node_width: 20.,
            node_gap: 14.,
            label_margin: 28.,
            columns,
        },
    );
    SankeyContext {
        nodes: layout.nodes,
        links: layout.links,
        columns: layout.columns,
        width,
        height,
        value_scale: layout.value_scale,
        max_value: layout.max_value,
        node_width: 20.,
        node_color: None,
    }
}

fn en(value: f64) -> String {
    astryx_i18n::I18n::new("en-US").number(value)
}

fn texts(labels: &[NodeLabel]) -> Vec<String> {
    labels
        .iter()
        .flat_map(|l| {
            std::iter::once(l.text.to_string()).chain(l.percent.as_ref().map(|p| p.2.to_string()))
        })
        .collect()
}

#[test]
fn grid_lines_sit_at_each_bar_center() {
    let (nodes, links) = chain(false);
    let columns = rich_columns();
    let ctx = context(&nodes, &links, Some(&columns), 1000., 320.);
    let centers: Vec<f64> = ctx
        .columns
        .iter()
        .map(|c| c.x + ctx.node_width / 2.)
        .collect();
    assert_eq!(centers, [10., 500., 990.]);
    let headers: Vec<_> = ctx.columns.iter().filter_map(|c| c.label.clone()).collect();
    assert_eq!(headers, ["Source", "Middle", "Target"]);
    // Plain id lists have no headers.
    let plain = [
        SankeyColumn::ids(["a"]),
        SankeyColumn::ids(["b"]),
        SankeyColumn::ids(["c"]),
    ];
    let ctx = context(&nodes, &links, Some(&plain), 1000., 320.);
    assert_eq!(ctx.columns.len(), 3);
    assert!(ctx.columns.iter().all(|c| c.label.is_none()));
}

#[test]
fn the_dash_pattern_reads_like_svg() {
    assert_eq!(marks::dash_pattern("4 4"), Some(vec![px(4.), px(4.)]));
    assert_eq!(marks::dash_pattern("2, 6"), Some(vec![px(2.), px(6.)]));
    assert_eq!(marks::dash_pattern("none"), None);
    assert_eq!(marks::dash_pattern("0 0"), None);
    assert_eq!(marks::dash_pattern(""), None);
}

#[test]
fn ribbons_follow_the_layout() {
    let (nodes, links) = chain(true);
    let ctx = context(&nodes, &links, None, 1000., 300.);
    let d: Vec<String> = ctx
        .links
        .iter()
        .map(|l| link_path(l, &ctx.nodes, 0.5))
        .collect();
    assert_eq!(
        d,
        [
            "M20,28 C255,28 255,28 490,28 L490,156 C255,156 255,156 20,156 Z",
            "M510,28 C745,28 745,28 980,28 L980,156 C745,156 745,156 510,156 Z",
        ]
    );
    // No tension: straight, the control points on the ends.
    assert_eq!(
        link_path(&ctx.links[0], &ctx.nodes, 0.),
        "M20,28 C20,28 490,28 490,28 L490,156 C490,156 20,156 20,156 Z"
    );
}

#[test]
fn the_gradient_holds_each_color_near_its_end() {
    let pct = |o: [f64; 4]| o.map(|v| (v * 100.).round());
    assert_eq!(pct(gradient_offsets(0.5)), [0., 35., 65., 100.]);
    assert_eq!(pct(gradient_offsets(0.75)), [0., 60., 90., 100.]);
    assert_eq!(gradient_offsets(0.1), [0., 0., 0.25, 1.]);
    assert_eq!(gradient_offsets(0.95), [0., 0.7999999999999999, 1., 1.]);
}

#[test]
fn link_colors_come_from_the_nodes() {
    // `oklch(0.5 0.1 100 / 0.7)`: the alpha travels with the color.
    let c = oklch_color([0.5, 0.1, 100.], 0.7);
    assert!((c.a - 0.7).abs() < 1e-6);
    let hex = astryx_theme::authoring::color::oklch_clamped_hex(0.5, 0.1, 100.);
    let rgb = astryx_theme::authoring::color::parse_color(&hex).expect("a color");
    let expected: gpui::Hsla = gpui::Rgba {
        r: (rgb.r / 255.) as f32,
        g: (rgb.g / 255.) as f32,
        b: (rgb.b / 255.) as f32,
        a: 0.7,
    }
    .into();
    assert_eq!(c, expected);
}

#[test]
fn bars_and_glows_take_the_layout_geometry() {
    let (nodes, links) = chain(true);
    let ctx = context(&nodes, &links, None, 1000., 300.);
    let a = &ctx.nodes[0];
    assert_eq!((a.x, a.y, a.width, a.height), (0., 28., 20., 256.));
    assert_eq!((ctx.nodes[1].x, ctx.nodes[2].x), (490., 980.));
    assert_eq!(a.color, [0.5, 0.1, 100.]);
}

#[test]
fn tall_bars_carry_their_labels_turned() {
    let (nodes, links) = chain(true);
    let ctx = context(&nodes, &links, None, 1000., 300.);
    let format = |v: f64| default_format(v, en);
    let labels = node_labels(&ctx, true, &format);
    assert_eq!(texts(&labels), ["A = 8", "B = 8", "100%", "C = 8", "100%"]);
    assert_eq!(
        labels[0].placement,
        LabelPlacement::Rotated {
            cx: 10.,
            cy: 156.,
            dark: true
        }
    );
    // The first column has no share; the rest sit below the bar, kept on the chart.
    assert!(labels[0].percent.is_none());
    let (x, y, _) = labels[1].percent.clone().expect("a share");
    assert_eq!((x, y), (500., 296.));
    assert_eq!(labels[2].percent.as_ref().map(|p| p.0), Some(990.));
    // Light bars take the on-light color.
    assert!(matches!(
        labels[2].placement,
        LabelPlacement::Rotated { dark: false, .. }
    ));
    let plain = node_labels(&ctx, false, &format);
    assert_eq!(texts(&plain), ["A = 8", "B = 8", "C = 8"]);
    let custom = node_labels(&ctx, false, &|v| format!("{v}pts"));
    assert_eq!(texts(&custom), ["A = 8pts", "B = 8pts", "C = 8pts"]);
}

#[test]
fn short_bars_put_their_labels_beside_on_a_pill() {
    let (mut nodes, links) = chain(true);
    for (node, label) in nodes.iter_mut().zip(["Alpha", "Bravo", "Gamma"]) {
        node.label = label.into();
    }
    let ctx = context(&nodes, &links, None, 1000., 100.);
    let labels = node_labels(&ctx, false, &|v| default_format(v, en));
    assert_eq!(labels[0].text, "Alpha = 8");
    assert_eq!(
        labels[0].placement,
        LabelPlacement::Beside {
            label_x: 26.,
            cy: 56.,
            anchor: TextAnchor::Start,
            pill_x: 22.,
            pill_width: 64.,
        }
    );
    // The last column's label ends before its bar.
    assert_eq!(
        labels[2].placement,
        LabelPlacement::Beside {
            label_x: 974.,
            cy: 56.,
            anchor: TextAnchor::End,
            pill_x: 914.,
            pill_width: 64.,
        }
    );
}

#[test]
fn values_compact_and_small_shares_keep_a_decimal() {
    let nodes = vec![
        SankeyNodeDatum::new("a", "A", 12000.).color([0.5, 0.1, 100.]),
        SankeyNodeDatum::new("b", "B", 1500.).color([0.6, 0.2, 200.]),
        SankeyNodeDatum::new("c", "C", 600.).color([0.7, 0.3, 300.]),
    ];
    let links = vec![
        SankeyLinkDatum::new("a", "b", 1500.),
        SankeyLinkDatum::new("a", "c", 600.),
    ];
    let ctx = context(&nodes, &links, None, 1000., 300.);
    let labels = node_labels(&ctx, true, &|v| default_format(v, en));
    assert_eq!(
        texts(&labels),
        ["A = 12k", "B = 1.5k", "13%", "C = 600", "5.0%"]
    );
    assert_eq!(default_format(1234567., en), "1235k");
    assert_eq!(default_format(999.5, en), "999.5");
}

#[test]
fn text_on_an_overridden_bar_reads_the_css_color() {
    // The answers upstream's heuristic gives, checked under Node.
    let cases = [
        ("black", true),
        ("BLACK", true),
        (" black ", true),
        ("#000", true),
        ("#1a1a1e", true),
        ("#3ff", true),
        ("#4f0", false),
        ("#f2a", true),
        ("#f9a", false),
        ("#fff", false),
        ("rgb(0,0,0)", true),
        ("rgb( 79 ,1,1)", true),
        ("rgb(80,0,0)", false),
        ("rgb(7 , 1,1)", true),
        ("rgb(700,1,1)", false),
        ("rgb(8,0,0)", false),
        ("rgba(0,0,0,1)", false),
        ("rgb(  5,", true),
        ("white", false),
        ("#", false),
        ("#a", false),
        ("oklch(0.2 0 0)", false),
        ("rgb(1)", false),
        ("rgb(12,3,4)", true),
        ("rgb(79,1,1)", true),
        ("rgb(7x,1,1)", false),
        ("#a0", true),
        ("Rgb(0, 0, 0)", true),
        ("\u{a0}black", true),
    ];
    for (css, dark) in cases {
        assert_eq!(on_surface_dark([0.7, 0., 0.], Some(css)), dark, "{css:?}");
    }
    // No override (or an empty one): the lightness decides.
    assert!(on_surface_dark([0.59, 0., 0.], None));
    assert!(!on_surface_dark([0.6, 0., 0.], None));
    assert!(!on_surface_dark([f64::NAN, 0., 0.], None));
    assert!(!on_surface_dark([0.7, 0., 0.], Some("")));
}

#[test]
fn the_table_lists_each_flow_by_node_label() {
    let nodes = vec![
        SankeyNodeDatum::new("a", "A", 10.),
        SankeyNodeDatum::new("b", "B", 10.),
        SankeyNodeDatum::new("c", "C", 10.),
    ];
    let links = vec![
        SankeyLinkDatum::new("a", "b", 5.),
        SankeyLinkDatum::new("b", "c", 5.),
    ];
    let t = sankey_table(&nodes, &links, "Sankey chart").expect("a table");
    assert_eq!(t.caption, "Sankey chart data");
    assert_eq!(t.columns, ["From", "To", "Value"]);
    assert_eq!(t.rows, [["A", "B", "5"], ["B", "C", "5"]]);
    let many_nodes: Vec<_> = (0..102)
        .map(|i| SankeyNodeDatum::new(format!("n{i}"), format!("N{i}"), 1.))
        .collect();
    let many_links: Vec<_> = (0..101)
        .map(|i| SankeyLinkDatum::new(format!("n{i}"), format!("n{}", i + 1), 1.))
        .collect();
    assert!(
        sankey_table(&many_nodes, &many_links, "x").is_none(),
        "past the cutoff"
    );
}

#[test]
fn a_chart_scrolls_when_its_columns_need_more_room() {
    assert_eq!(chart_width(300., 3, 400.), (1200., true));
    assert_eq!(chart_width(1000., 3, 50.), (1000., false));
}

#[test]
#[should_panic(expected = "Sankey components must be used inside <SankeyChart>")]
fn use_sankey_panics_outside_a_chart() {
    gpui::TestAppContext::single().update(|cx| {
        use_sankey(cx);
    });
}

struct Host(f32, Rc<dyn Fn() -> AnyElement>);

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().w(px(self.0)).child((self.1)())
    }
}

fn host(
    cx: &mut TestAppContext,
    width: f32,
    build: impl Fn() -> AnyElement + 'static,
) -> &mut VisualTestContext {
    cx.update(|cx| {
        astryx_theme::init(cx);
        astryx_primitives::focus::init(cx);
    });
    let (_, cx) = cx.add_window_view(move |_, _| Host(width, Rc::new(build)));
    for _ in 0..3 {
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
    }
    cx
}

fn chart(min_column_width: f64) -> impl Fn() -> AnyElement {
    move || {
        let (nodes, links) = chain(false);
        SankeyChart::new("flow", nodes, links)
            .min_column_width(min_column_width)
            .child(SankeyGrid::new())
            .child(SankeyLink::new())
            .child(SankeyNode::new())
            .child(SankeyLabel::new())
            .into_any_element()
    }
}

#[gpui::test]
fn a_chart_wider_than_its_container_scrolls_in_a_named_region(cx: &mut TestAppContext) {
    let cx = host(cx, 300., chart(400.));
    let plot = cx.debug_bounds("flow-sankey-plot").expect("drawn");
    assert_eq!(
        (f32::from(plot.size.width), f32::from(plot.size.height)),
        (1200., 320.)
    );
    let region = cx
        .debug_bounds("flow-sankey-scroll")
        .expect("a scroll region");
    assert_eq!(f32::from(region.size.width), 300.);
}

#[gpui::test]
fn a_chart_that_fits_has_no_scroll_region(cx: &mut TestAppContext) {
    let cx = host(cx, 1000., chart(50.));
    let plot = cx.debug_bounds("flow-sankey-plot").expect("drawn");
    assert_eq!(f32::from(plot.size.width), 1000.);
    assert!(cx.debug_bounds("flow-sankey-scroll").is_none());
    assert!(
        cx.debug_bounds("flow-sankey-table").is_some(),
        "the hidden table"
    );
}

#[gpui::test]
fn nothing_draws_before_the_container_has_a_width(cx: &mut TestAppContext) {
    let cx = host(cx, 0., chart(160.));
    assert!(cx.debug_bounds("flow-sankey-plot").is_none());
}
