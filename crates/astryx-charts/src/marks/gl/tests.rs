//! The WebGL marks' geometry, colors, hover pick and stream, against the values
//! upstream computes (from d3-scale 4.0.2 under Node), and the marks inside a chart
//! (upstream `marks.test.tsx`'s "GL marks without WebGL" and `glEffectReplay.test.tsx`).

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{
    AnyElement, Context, IntoElement, Modifiers, ParentElement, Render, Styled, TestAppContext,
    VisualTestContext, Window, div, point, px, rgb,
};

use super::dot_gl::{dot_gl_positions, dot_gl_radius};
use super::dot_gl_interactive::{
    card_origin, interactive_color, interactive_positions, interactive_radius,
};
use super::heatmap_gl::{HeatCell, heatmap_cells, heatmap_domain, sample_ramp};
use super::pick::PickBuffer;
use super::stream_gl::{stream_stroke_width, stream_vertices};
use super::*;
use crate::layout::{LayoutInput, compute_layout};
use crate::scale::{BandScale, LinearScale};
use crate::{Chart, ChartValue, Datum, datum};

const NAN: f64 = f64::NAN;

#[track_caller]
fn assert_same(got: &[f64], want: &[f64]) {
    let same = |a: f64, b: f64| (a.is_nan() && b.is_nan()) || a.to_bits() == b.to_bits();
    assert!(
        got.len() == want.len() && got.iter().zip(want).all(|(a, b)| same(*a, *b)),
        "{got:?} != {want:?}"
    );
}

fn ctx(data: Vec<Datum>, x_scale: ChartScale, y_scale: LinearScale) -> SeriesContext {
    SeriesContext {
        data: data.into(),
        x_key: "x".into(),
        x_scale,
        y_scale,
        y_band_scale: None,
        width: 500.,
        height: 200.,
    }
}

#[test]
fn dots_resolve_with_upstreams_coercions() {
    // x goes through the linear scale as d3 coerces it (null and a missing value are
    // unknown); a value that is not a number reads 0, NaN stays NaN.
    let data = vec![
        datum! { "x" => 5, "v" => 50 },
        datum! { "x" => "7", "v" => "80" },
        datum! { "x" => ChartValue::Null, "v" => 20 },
        datum! { "v" => 30 },
        datum! { "x" => true, "v" => NAN },
        datum! { "x" => ChartValue::Date(4.), "v" => 100 },
        datum! { "x" => "x", "v" => f64::INFINITY },
        datum! { "x" => 2 },
    ];
    let c = ctx(
        data,
        ChartScale::Linear(LinearScale::new().domain(0., 10.).range(0., 500.)),
        LinearScale::new().domain(0., 100.).range(200., 0.),
    );
    let got: Vec<f64> = resolve_dots(&c, "v")
        .iter()
        .flat_map(|p| [p.px, p.py, p.py0])
        .collect();
    assert_same(
        &got,
        &[
            250.,
            100.,
            200.,
            350.,
            200.,
            200.,
            f64::NAN,
            160.,
            200.,
            f64::NAN,
            140.,
            200.,
            50.,
            f64::NAN,
            200.,
            200.,
            0.,
            200.,
            f64::NAN,
            f64::NAN,
            200.,
            100.,
            200.,
            200.,
        ],
    );
}

#[test]
fn dots_on_a_band_scale_sit_at_band_centers() {
    let data = vec![
        datum! { "x" => "b", "v" => 10 },
        datum! { "x" => "z", "v" => 10 },
        datum! { "v" => 10 },
        datum! { "x" => ChartValue::Null, "v" => 10 },
    ];
    let band = BandScale::new()
        .domain(["a", "b", "c"])
        .range(0., 300.)
        .padding(0.2);
    let c = ctx(
        data,
        ChartScale::Band(band),
        LinearScale::new().domain(0., 100.).range(200., 0.),
    );
    let got: Vec<f64> = resolve_dots(&c, "v")
        .iter()
        .flat_map(|p| [p.px, p.py])
        .collect();
    // A value outside the domain sits at the first band's center, as `?? 0` puts it.
    assert_same(
        &got,
        &[
            150.00000000000003,
            180.,
            37.50000000000001,
            180.,
            37.50000000000001,
            180.,
            37.50000000000001,
            180.,
        ],
    );
}

#[test]
fn dot_gl_draws_finite_points_at_their_size() {
    let p = |px, py| ResolvedPoint {
        px,
        py,
        py0: 0.,
        data_index: 0,
    };
    let positions = dot_gl_positions(&[p(1.5, 2.), p(NAN, 3.), p(4., f64::INFINITY), p(0.1, 0.2)]);
    assert_eq!(positions, vec![[1.5, 2.], [0.1f32, 0.2f32]]);
    // A 6 px point is 6 px across on any display.
    assert_eq!(dot_gl_radius(6., 1.), Some(3.));
    assert_eq!(dot_gl_radius(6., 2.), Some(3.));
    // GL draws at least one framebuffer pixel, and nothing at a size of 0 or NaN.
    assert_eq!(dot_gl_radius(0.01, 1.), Some(0.48 / 2.));
    assert_eq!(dot_gl_radius(0., 1.), None);
    assert_eq!(dot_gl_radius(NAN, 1.), None);
    assert_eq!(canvas_dpr(0.), 4., "no device pixel ratio reads as 2");
}

#[test]
fn colors_read_as_upstreams_hex_parse() {
    for hex in [0x000000, 0xffffff, 0x4f46e5, 0x10b981, 0x123456, 0xfe0102] {
        let got = gl_rgb(rgb(hex).into());
        let want = [(hex >> 16) & 0xff, (hex >> 8) & 0xff, hex & 0xff].map(|b| f64::from(b) / 255.);
        assert_eq!(got, want, "{hex:06x}");
    }
    // Every 97th hex color survives the trip through Hsla.
    for hex in (0..0x100_0000u32).step_by(97) {
        let want = [(hex >> 16) & 0xff, (hex >> 8) & 0xff, hex & 0xff].map(|b| f64::from(b) / 255.);
        assert_eq!(gl_rgb(rgb(hex).into()), want, "{hex:06x}");
    }
}

#[test]
fn premultiplied_fragments_blend_as_gl_clamps_them() {
    let rgb = [0.2, 0.4, 0.6];
    let c = premultiplied(rgb, 0.5).unwrap().to_rgb();
    assert!((c.r - 0.2).abs() < 1e-6 && (c.b - 0.6).abs() < 1e-6 && (c.a - 0.5).abs() < 1e-6);
    // Past 1 the color brightens; at or below 0 nothing draws.
    let c = premultiplied(rgb, 2.).unwrap().to_rgb();
    assert!((c.r - 0.4).abs() < 1e-6 && (c.b - 1.).abs() < 1e-6 && (c.a - 1.).abs() < 1e-6);
    assert!(premultiplied(rgb, 0.).is_none());
    assert!(premultiplied(rgb, -1.).is_none());
    assert!(premultiplied(rgb, NAN).is_none());
}

#[test]
fn interactive_points_keep_every_index() {
    let p = |px, py, i| ResolvedPoint {
        px,
        py,
        py0: 0.,
        data_index: i,
    };
    let positions = interactive_positions(&[p(1., 2., 0), p(NAN, 3., 1)]);
    assert_eq!(positions.len(), 2, "aligned with the data for the pick");
    assert!(positions[1][0].is_nan());
    assert_eq!(interactive_radius(6., 2.), Some(3.));
    assert_eq!(interactive_radius(-30., 1.), None);
    // One point alone stores color * a at alpha a * a in a canvas that is not
    // premultiplied.
    let c = interactive_color([1., 0.5, 0.], 0.8).unwrap().to_rgb();
    assert!((c.r - 0.8).abs() < 1e-6 && (c.g - 0.4).abs() < 1e-6 && (c.a - 0.64).abs() < 1e-6);
    assert!(interactive_color([1., 1., 1.], 0.).is_none());
    assert_eq!(card_origin((100., 30.)), (112., 0.));
    assert_eq!(card_origin((100., 70.)), (112., 30.));
}

/// A small deterministic generator (mulberry32).
struct Rng(u32);

impl Rng {
    fn next(&mut self) -> f64 {
        self.0 = self.0.wrapping_add(0x6d2b_79f5);
        let mut t = self.0;
        t = (t ^ (t >> 15)).wrapping_mul(t | 1);
        t ^= t.wrapping_add((t ^ (t >> 7)).wrapping_mul(t | 61));
        f64::from(t ^ (t >> 14)) / 4_294_967_296.
    }
}

#[test]
fn the_pick_grid_finds_what_drawing_every_point_finds() {
    let mut rng = Rng(7);
    for case in 0..40 {
        let (width, height) = (50. + rng.next() * 500., 40. + rng.next() * 300.);
        let n = (rng.next() * 400.) as usize;
        let mut positions: Vec<[f32; 2]> = (0..n)
            .map(|_| {
                let r = rng.next();
                if r < 0.05 {
                    [f32::NAN, 1.]
                } else {
                    // Some points just off the plot, whose sprites reach in.
                    [
                        (rng.next() * (width + 60.) - 30.) as f32,
                        (rng.next() * (height + 60.) - 30.) as f32,
                    ]
                }
            })
            .collect();
        if n > 2 {
            positions[n - 1] = positions[0];
        }
        let size = [6., 1., 0., -10., 20., 3.5][case % 6];
        let dpr = [1., 2., 1.5, 3.][case % 4];
        let pick = PickBuffer::new(&positions, width, height, size, dpr);
        for _ in 0..300 {
            let (x, y) = (
                (rng.next() * pick.tex_w).floor(),
                (rng.next() * pick.tex_h).floor(),
            );
            assert_eq!(
                pick.pixel_index(x, y),
                pick.pixel_index_brute_force(x, y),
                "case {case} at {x},{y}"
            );
        }
    }
}

#[test]
fn the_last_drawn_point_wins_a_pixel_within_its_padded_radius() {
    // A 6 px point picks 22 px across at a device pixel ratio of 1: radius 11.
    let positions = [[10.5f32, 10.5], [14.5, 10.5]];
    let pick = PickBuffer::new(&positions, 100., 50., 6., 1.);
    assert_eq!(pick.hover(12., 10.), 1, "both cover it; the later wins");
    assert_eq!(pick.hover(0., 10.), 0, "only the first reaches it");
    // Pixel 21's center is 10.5 from the first point and 6.5 from the second.
    assert_eq!(pick.hover(25.9, 10.), 1);
    assert_eq!(pick.hover(26., 10.), -1, "11.5 from the second: outside");
    assert_eq!(pick.hover(-0.1, 10.), -1, "off the buffer");
    assert_eq!(pick.hover(100., 10.), -1, "past its right edge");
    // The buffer is `Math.round(width * dpr)` pixels: a hover at x 99.6 is pixel 99.
    let pick = PickBuffer::new(&positions, 99.6, 50., 6., 1.);
    assert_eq!(pick.tex_w, 100.);
}

#[test]
fn heatmap_ramps_sample_as_upstream() {
    let ramp = [[1., 0., 0.], [0., 1., 0.], [0., 0., 1.]];
    let ts = [-1., 0., 0.25, 0.5, 0.6, 1., 2., NAN, f64::INFINITY];
    let got: Vec<f64> = ts.iter().flat_map(|t| sample_ramp(&ramp, *t)).collect();
    assert_same(
        &got,
        &[
            1.,
            0.,
            0.,
            1.,
            0.,
            0.,
            0.5,
            0.5,
            0.,
            0.,
            1.,
            0.,
            0.,
            0.8,
            0.19999999999999996,
            0.,
            0.,
            1.,
            0.,
            0.,
            1.,
            1.,
            0.,
            0.,
            1.,
            0.,
            0.,
        ],
    );
    assert_eq!(sample_ramp(&[], 0.5), [0., 0., 0.]);
    assert_eq!(sample_ramp(&[[0.1, 0.2, 0.3]], 0.9), [0.1, 0.2, 0.3]);
}

#[test]
fn the_heatmap_domain_is_the_finite_extent() {
    let data = vec![
        datum! { "c" => 3 },
        datum! { "c" => NAN },
        datum! { "c" => "9" },
        datum! { "c" => -2 },
        datum! { "c" => f64::INFINITY },
    ];
    assert_eq!(heatmap_domain(&data, "c", None), [-2., 3.]);
    assert_eq!(heatmap_domain(&data, "c", Some([0., 50.])), [0., 50.]);
    assert_eq!(
        heatmap_domain(&data[1..3], "c", None),
        [0., 1.],
        "no finite values"
    );
}

fn heat() -> Vec<Datum> {
    [
        ("0", "Mon", 1),
        ("1", "Mon", 5),
        ("0", "Tue", 3),
        ("1", "Tue", 2),
    ]
    .into_iter()
    .map(|(h, d, c)| datum! { "hour" => h, "day" => d, "count" => c })
    .collect()
}

#[test]
fn heatmap_cells_fill_their_slots_on_the_charts_band_scales() {
    let data: Rc<[Datum]> = heat().into();
    let series = [heatmap_gl(HeatmapGlOptions::new(
        "hour",
        "day",
        "count",
        [rgb(0x000000).into(), rgb(0xffffff).into()],
    ))];
    // A 600 by 300 chart's plot: both axes categorical, so neither band is padded.
    let layout = compute_layout(LayoutInput {
        data: &data,
        x_key: &"hour".into(),
        series: &series,
        width: 528.,
        height: 244.,
        y_baseline: None,
        y_domain: None,
        x_domain: None,
    });
    let (ChartScale::Band(xs), Some(ys)) = (&layout.x_scale, &layout.y_band_scale) else {
        panic!("a heatmap's chart has band scales on both axes");
    };
    let ramp = [[0., 0., 0.], [1., 1., 1.]];
    let domain = heatmap_domain(&data, "count", None);
    let cells = heatmap_cells(&data, "hour", "day", "count", xs, ys, &ramp, domain, 1.);
    let got: Vec<f64> = cells
        .iter()
        .flat_map(|c| [c.x0, c.y0, c.x1, c.y1, c.color[0]].map(f64::from))
        .collect();
    assert_same(
        &got,
        &[
            0.5, 0.5, 263.5, 121.5, 0., 264.5, 0.5, 527.5, 121.5, 1., 0.5, 122.5, 263.5, 243.5,
            0.5, 264.5, 122.5, 527.5, 243.5, 0.25,
        ],
    );
    // The resolved points are the cells' centers.
    let points = &layout.resolved[0].1;
    assert_eq!((points[1].px, points[1].py), (396., 61.));
}

#[test]
fn heatmap_rows_outside_a_scale_draw_no_cell() {
    let xs = BandScale::new().domain(["a"]).range(0., 100.);
    let ys = BandScale::new().domain(["r"]).range(0., 50.);
    let data = vec![
        datum! { "h" => "a", "d" => "r", "c" => 1 },
        datum! { "h" => "b", "d" => "r", "c" => 1 },
        datum! { "h" => "a", "d" => "q", "c" => 1 },
    ];
    let cells = heatmap_cells(&data, "h", "d", "c", &xs, &ys, &[], [0., 1.], 30.);
    assert_eq!(
        cells,
        vec![HeatCell {
            x0: 15.,
            y0: 15.,
            x1: 85.,
            y1: 35.,
            color: [0., 0., 0.]
        }]
    );
    // A gap past the step turns the cell inside out; GL still fills it.
    let cells = heatmap_cells(&data[..1], "h", "d", "c", &xs, &ys, &[], [0., 1.], 120.);
    assert_eq!((cells[0].x0, cells[0].x1), (60., 40.));
}

#[test]
fn the_gl_marks_describe_themselves_as_upstream() {
    let dots = dot_gl("v", DotGlOptions::new(gpui::blue()));
    assert_eq!(
        (dots.kind(), dots.key().as_ref(), dots.data_keys()),
        ("dotGL", "v", vec!["v".into()])
    );
    assert!(dots.draws_above());
    assert_eq!(
        dots.color(),
        None,
        "upstream's mark has no static color for the legend"
    );
    let interactive = dot_gl_interactive("v", DotGlInteractiveOptions::new(gpui::blue()));
    assert_eq!(interactive.kind(), "dotGLInteractive");
    assert!(!interactive.draws_above(), "drawn inside the chart's marks");
    let heatmap = heatmap_gl(HeatmapGlOptions::new("h", "d", "count", []));
    assert_eq!(
        (heatmap.kind(), heatmap.key().as_ref()),
        ("heatmapGL", "heatmap-count")
    );
    assert!(heatmap.data_keys().is_empty());
    assert_eq!(heatmap.layout().y_band_key.as_deref(), Some("d"));
    let stream = stream_gl(StreamGlOptions::new(gpui::blue()));
    assert_eq!(
        (stream.kind(), stream.key().as_ref()),
        ("streamGL", "stream")
    );
    assert!(stream.data_keys().is_empty() && stream.draws_above());
}

#[test]
fn a_heatmap_needs_band_scales_on_both_axes() {
    let series = heatmap_gl(HeatmapGlOptions::new("x", "d", "c", []));
    let c = ctx(
        vec![datum! { "x" => 1, "d" => "r", "c" => 1 }],
        ChartScale::Linear(LinearScale::new()),
        LinearScale::new(),
    );
    assert!(series.resolve(&c, None, None).is_empty());
}

#[test]
fn the_stream_keeps_the_newest_points_as_32_bit_floats() {
    let handle = StreamGlHandle::new();
    handle.set_capacity(3);
    for i in 0..5 {
        handle.push(f64::from(i), f64::from(i) * 10.);
    }
    assert_eq!(handle.points(), vec![[2., 20.], [3., 30.], [4., 40.]]);
    handle.push(NAN, 1.);
    handle.push(1., f64::INFINITY);
    assert_eq!(handle.points().len(), 3, "non-finite points are ignored");
    // A timestamp loses its milliseconds in a Float32Array.
    handle.clear();
    handle.push(1_700_000_000_123., 1e300);
    assert_eq!(handle.points(), vec![[1_700_000_000_000f32, f32::INFINITY]]);
    // A new size empties the stream; a buffer of 0 keeps nothing.
    handle.set_capacity(4);
    assert!(handle.points().is_empty());
    handle.set_capacity(0);
    handle.push(1., 1.);
    assert!(handle.points().is_empty());
}

#[test]
fn stream_vertices_follow_the_charts_scales() {
    let x = ChartScale::Linear(LinearScale::new().domain(0., 10.).range(0., 100.));
    let y = LinearScale::new().domain(0., 10.).range(50., 0.);
    assert_eq!(
        stream_vertices(&[[1., 2.], [5., 10.]], &x, &y),
        vec![[10., 40.], [50., 0.]]
    );
    // A band scale of a number is d3's undefined.
    let band = ChartScale::Band(BandScale::new().domain(["1"]));
    assert!(stream_vertices(&[[1., 2.]], &band, &y)[0][0].is_nan());
    // GL widths are framebuffer pixels of the supersampled canvas; one GL rejects
    // keeps the default of 1.
    assert_eq!(stream_stroke_width(2., 1.), 1.);
    assert_eq!(stream_stroke_width(2., 2.), 0.5);
    assert_eq!(stream_stroke_width(0., 1.), 0.5);
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
    cx.run_until_parked();
    cx
}

fn dots_data() -> Rc<[Datum]> {
    [("A", 4), ("B", 7), ("C", 2)]
        .into_iter()
        .map(|(m, v)| datum! { "m" => m, "v" => v })
        .collect()
}

#[gpui::test]
fn every_gl_mark_draws_in_a_chart(cx: &mut TestAppContext) {
    let handle = StreamGlHandle::new();
    let cx =
        host(cx, {
            let handle = handle.clone();
            move || {
                div()
                    .child(
                        Chart::new("dots", dots_data(), "m")
                            .series([dot_gl("v", DotGlOptions::new(rgb(0x4f46e5).into()))]),
                    )
                    .child(Chart::new("interactive", dots_data(), "m").series([
                        dot_gl_interactive("v", DotGlInteractiveOptions::new(rgb(0x111111).into())),
                    ]))
                    .child(Chart::new("heat", heat(), "hour").series([heatmap_gl(
                        HeatmapGlOptions::new(
                            "hour",
                            "day",
                            "count",
                            [rgb(0x000000).into(), rgb(0xffffff).into()],
                        ),
                    )]))
                    .child(
                        Chart::new("stream", Vec::<Datum>::new(), "x")
                            .x_domain(0., 10.)
                            .y_domain(0., 10.)
                            .series([stream_gl(
                                StreamGlOptions::new(rgb(0x10b981).into()).handle(handle.clone()),
                            )]),
                    )
                    .into_any_element()
            }
        });
    assert!(handle.paints() > 0, "the stream's canvas is painted");
    handle.push(1., 2.);
    handle.push(2., 4.);
    handle.clear();
    cx.run_until_parked();
    assert!(handle.points().is_empty());
}

#[gpui::test]
fn pushes_coalesce_into_one_redraw(cx: &mut TestAppContext) {
    let handle = StreamGlHandle::new();
    let cx = host(cx, {
        let handle = handle.clone();
        move || {
            Chart::new("stream", Vec::<Datum>::new(), "x")
                .x_domain(0., 10.)
                .y_domain(0., 10.)
                .series([stream_gl(
                    StreamGlOptions::new(rgb(0x10b981).into())
                        .handle(handle.clone())
                        .buffer_size(4),
                )])
                .into_any_element()
        }
    });
    let paints = handle.paints();
    let redraws = handle.redraws();
    for i in 0..10 {
        handle.push(f64::from(i), 5.);
    }
    assert_eq!(handle.redraws(), redraws + 1, "one redraw for the burst");
    cx.run_until_parked();
    assert_eq!(handle.paints(), paints + 1, "drawn once");
    assert_eq!(handle.points().len(), 4, "the newest four");
    handle.push(1., 1.);
    assert_eq!(
        handle.redraws(),
        redraws + 2,
        "the next push schedules the next frame"
    );
    cx.run_until_parked();
    assert_eq!(handle.paints(), paints + 2);
}

#[gpui::test]
fn hovering_a_point_opens_its_card(cx: &mut TestAppContext) {
    let seen: Rc<RefCell<Vec<usize>>> = Rc::default();
    let data: Rc<[Datum]> = [(1., 2.), (5., 5.), (5., 5.), (9., 9.5)]
        .into_iter()
        .map(|(x, y)| datum! { "x" => x, "y" => y })
        .collect();
    let cx = host(cx, {
        let seen = seen.clone();
        move || {
            let seen = seen.clone();
            Chart::new("scatter", data.clone(), "x")
                .x_domain(0., 10.)
                .y_domain(0., 10.)
                .series([dot_gl_interactive(
                    "y",
                    DotGlInteractiveOptions::new(rgb(0x4f46e5).into()).render_tooltip(
                        move |d, i, _, _| {
                            seen.borrow_mut().push(i);
                            Some(div().child(d.string("y")).into_any_element())
                        },
                    ),
                )])
                .into_any_element()
        }
    });
    let plot = cx.debug_bounds("chart-pointer").expect("the plot is drawn");
    let (w, h) = (f64::from(plot.size.width), f64::from(plot.size.height));
    let at = |x: f64, y: f64| {
        plot.origin + point(px((x / 10. * w) as f32), px((h - y / 10. * h) as f32))
    };
    assert!(
        cx.debug_bounds("dot-gl-tooltip").is_none(),
        "no card before a hover"
    );

    // Two points share (5, 5): the later one is on top.
    cx.simulate_mouse_move(at(5., 5.), None, Modifiers::none());
    cx.run_until_parked();
    assert_eq!(seen.borrow().last(), Some(&2));
    let card = cx.debug_bounds("dot-gl-tooltip").expect("the card opens");
    let mouse = at(5., 5.);
    assert_eq!(card.origin.x, mouse.x + px(12.));
    assert_eq!(card.origin.y, mouse.y - px(40.));

    // Near the top, the card stays inside the plot's top.
    cx.simulate_mouse_move(at(9., 9.5) + point(px(3.), px(0.)), None, Modifiers::none());
    cx.run_until_parked();
    assert_eq!(seen.borrow().last(), Some(&3));
    assert_eq!(
        cx.debug_bounds("dot-gl-tooltip").unwrap().origin.y,
        plot.origin.y
    );

    // Away from every point, and off the plot, the card closes.
    cx.simulate_mouse_move(at(3., 9.), None, Modifiers::none());
    cx.run_until_parked();
    assert!(cx.debug_bounds("dot-gl-tooltip").is_none());
    cx.simulate_mouse_move(at(5., 5.), None, Modifiers::none());
    cx.run_until_parked();
    assert!(cx.debug_bounds("dot-gl-tooltip").is_some());
    cx.simulate_mouse_move(point(px(1.), px(1.)), None, Modifiers::none());
    cx.run_until_parked();
    assert!(
        cx.debug_bounds("dot-gl-tooltip").is_none(),
        "leaving the plot closes it"
    );
}
