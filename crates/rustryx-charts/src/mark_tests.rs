//! Mark tests: upstream `marks/marks.test.tsx`'s SVG marks (bar, line, dot, area, band,
//! candlestick, error bar, reference line), read from each mark's [`Series::shapes`].
//! Geometry, colors and opacities were checked against upstream's marks rendered
//! under Node on the same layout (a 528 by 244 plot, a 600 px chart's); the exact
//! numbers below come from there.

use std::rc::Rc;

use gpui::Hsla;
use rustryx_theme::Tokens;

use crate::layout::{LayoutInput, compute_layout};
use crate::shape::{ChartPath, PathCommand};
use crate::*;

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-9 * a.abs().max(1.)
}

fn hex(value: u32) -> Hsla {
    gpui::rgb(value).into()
}

fn tokens() -> Tokens {
    rustryx_theme::Theme::default().light
}

fn row(m: &str, fields: &[(&str, f64)]) -> Datum {
    fields
        .iter()
        .fold(Datum::new().with("m", m.to_string()), |d, (k, v)| {
            d.with(k.to_string(), *v)
        })
}

/// `{m: 'A', v: 4}, {m: 'B', v: 7}, {m: 'C', v: 2}`
fn data() -> Rc<[Datum]> {
    [
        row("A", &[("v", 4.)]),
        row("B", &[("v", 7.)]),
        row("C", &[("v", 2.)]),
    ]
    .into()
}

/// One missing value in the middle: marks must gap, not draw at the baseline.
fn gappy() -> Rc<[Datum]> {
    [
        row("A", &[("v", 4.)]),
        Datum::new().with("m", "B").with("v", ChartValue::Null),
        row("C", &[("v", 2.)]),
    ]
    .into()
}

fn bounds() -> Rc<[Datum]> {
    [
        row("A", &[("hi", 5.), ("lo", 1.)]),
        row("B", &[("hi", 8.), ("lo", 4.)]),
        row("C", &[("hi", 6.), ("lo", 2.)]),
    ]
    .into()
}

/// Each series' shapes, laid out as the chart lays them out, with no palette color.
fn shapes_on(data: Rc<[Datum]>, x_key: &str, series: &[SeriesDef]) -> Vec<Vec<MarkShape>> {
    let result = compute_layout(LayoutInput {
        data: &data,
        x_key: &x_key.into(),
        series,
        width: 528.,
        height: 244.,
        y_baseline: None,
        y_domain: None,
        x_domain: None,
    });
    let ctx = SeriesContext {
        data: data.clone(),
        x_key: x_key.into(),
        x_scale: result.x_scale.clone(),
        y_scale: result.y_scale,
        y_band_scale: result.y_band_scale.clone(),
        width: 528.,
        height: 244.,
    };
    let tokens = tokens();
    series
        .iter()
        .zip(&result.info)
        .zip(&result.resolved)
        .map(|((s, info), (_, points))| s.shapes(points, &ctx, info, &tokens))
        .collect()
}

fn shapes(data: Rc<[Datum]>, series: &[SeriesDef]) -> Vec<Vec<MarkShape>> {
    shapes_on(data, "m", series)
}

fn path_of(shape: &MarkShape) -> &ChartPath {
    match shape {
        MarkShape::Fill { path, .. }
        | MarkShape::GradientFill { path, .. }
        | MarkShape::Stroke { path, .. } => path,
        other => panic!("not a path: {other:?}"),
    }
}

fn starts_with_move(path: &ChartPath) -> bool {
    matches!(path.commands().first(), Some(PathCommand::MoveTo { .. }))
}

fn has_quadratic(path: &ChartPath) -> bool {
    path.commands()
        .iter()
        .any(|c| matches!(c, PathCommand::QuadraticCurveTo { .. }))
}

fn has_cubic(path: &ChartPath) -> bool {
    path.commands()
        .iter()
        .any(|c| matches!(c, PathCommand::BezierCurveTo { .. }))
}

fn first_x(path: &ChartPath) -> f64 {
    match path.commands().first() {
        Some(PathCommand::MoveTo { x, .. }) => *x,
        _ => f64::NAN,
    }
}

#[test]
fn bars_draw_one_top_rounded_path_per_datum_in_their_color() {
    let all = shapes(data(), &[bar("v", BarOptions::new().color(hex(0x123456)))]);
    let bars = &all[0];
    assert_eq!(bars.len(), 3);
    for shape in bars {
        let MarkShape::Fill {
            path,
            color,
            opacity,
        } = shape
        else {
            panic!("{shape:?}");
        };
        assert_eq!(*color, hex(0x123456));
        assert_eq!(*opacity, 1.);
        assert!(starts_with_move(path));
        // Positive bars round their top corners (radius 4 by default).
        assert!(has_quadratic(path));
    }
    // Feb's bar: upstream `M198,244V4Q198,0 202,0H326Q330,0 330,4V244Z`.
    assert_eq!(
        path_of(&bars[1]).to_svg(None),
        "M198,244L198,4Q198,0,202,0L326,0Q330,0,330,4L330,244Z"
    );
}

#[test]
fn bars_skip_missing_values_instead_of_drawing_them_at_the_baseline() {
    assert_eq!(shapes(gappy(), &[bar("v", BarOptions::new())])[0].len(), 2);
}

fn two_columns() -> Rc<[Datum]> {
    [
        row("A", &[("a", 2.), ("b", 3.)]),
        row("B", &[("a", 4.), ("b", 1.)]),
    ]
    .into()
}

#[test]
fn stacked_bars_round_only_the_topmost_layer() {
    let all = shapes(
        two_columns(),
        &[
            bar("a", BarOptions::new().stack("total")),
            bar("b", BarOptions::new().stack("total")),
        ],
    );
    let (lower, upper) = (&all[0], &all[1]);
    assert_eq!((lower.len(), upper.len()), (2, 2));
    // The lower layer is drawn square; only the stack's top gets rounded corners.
    assert!(lower.iter().all(|s| !has_quadratic(path_of(s))));
    assert!(upper.iter().all(|s| has_quadratic(path_of(s))));
}

#[test]
fn grouped_bars_sit_side_by_side_within_the_band() {
    let all = shapes(
        two_columns(),
        &[
            bar("a", BarOptions::new().group("g")),
            bar("b", BarOptions::new().group("g")),
        ],
    );
    // For one category, the second series' bar starts further right.
    let (first, second) = (first_x(path_of(&all[0][0])), first_x(path_of(&all[1][0])));
    assert!(second > first, "{second} > {first}");
    assert!(close(first, 48.) && close(second, 148.8));
}

#[test]
fn negative_bars_grow_down_with_their_bottom_corners_rounded() {
    let signed: Rc<[Datum]> = [
        row("Q1", &[("v", -12.)]),
        row("Q2", &[("v", 30.)]),
        row("Q3", &[("v", -4.5)]),
        row("Q4", &[("v", 18.)]),
    ]
    .into();
    let all = shapes(
        signed,
        &[bar("v", BarOptions::new().opacity(0.5).radius(6.))],
    );
    let MarkShape::Fill { path, opacity, .. } = &all[0][0] else {
        panic!();
    };
    assert_eq!(*opacity, 0.5);
    // From the zero line down: upstream
    // M25.14…,162.67V221.73Q25.14…,227.73 31.14…,227.73H119.71…Q125.71…,227.73 125.71…,221.73V162.67Z.
    let c = path.commands();
    assert!(
        matches!(c[0], PathCommand::MoveTo { x, y } if close(x, 25.14285714285716) && close(y, 162.66666666666669))
    );
    assert!(matches!(c[1], PathCommand::LineTo { y, .. } if close(y, 221.73333333333335)));
    assert!(
        matches!(c[2], PathCommand::QuadraticCurveTo { y1, x, .. } if close(y1, 227.73333333333335) && close(x, 31.14285714285716))
    );
}

#[test]
fn a_line_draws_one_stroke_in_its_color_and_width() {
    let all = shapes(
        data(),
        &[line(
            "v",
            LineOptions::new().color(hex(0xff0000)).stroke_width(3.),
        )],
    );
    assert_eq!(all[0].len(), 1);
    let MarkShape::Stroke { path, color, width } = &all[0][0] else {
        panic!("{:?}", all[0][0]);
    };
    assert_eq!((*color, *width), (hex(0xff0000), 3.));
    // d3 writes three decimals.
    assert_eq!(path.to_svg(Some(3)), "M99,142.333L264,20.333L429,223.667");
}

#[test]
fn line_dots_draw_only_where_the_value_is_finite() {
    let all = shapes(gappy(), &[line("v", LineOptions::new().dots(true))]);
    let dots = all[0]
        .iter()
        .filter(|s| matches!(s, MarkShape::Circle { r, .. } if *r == 3.))
        .count();
    assert_eq!(dots, 2);
}

#[test]
fn lines_areas_and_bands_join_points_with_straight_segments() {
    let line_path = shapes(data(), &[line("v", LineOptions::new())]);
    assert!(starts_with_move(path_of(&line_path[0][0])));
    assert!(!has_cubic(path_of(&line_path[0][0])));
    let area_paths = shapes(data(), &[area("v", AreaOptions::new())]);
    assert!(!area_paths[0].is_empty());
    for shape in &area_paths[0] {
        assert!(starts_with_move(path_of(shape)) && !has_cubic(path_of(shape)));
    }
    let band_path = shapes(bounds(), &[band(BandOptions::new("hi", "lo"))]);
    assert!(starts_with_move(path_of(&band_path[0][0])));
    assert!(!has_cubic(path_of(&band_path[0][0])));
}

#[test]
fn a_line_still_smooths_when_asked() {
    let all = shapes(
        data(),
        &[line("v", LineOptions::new().curve(CurveType::Monotone))],
    );
    assert!(has_cubic(path_of(&all[0][0])));
}

#[test]
fn smoothed_and_stepped_lines_match_d3() {
    // Twelve readings and a second series with a gap and dots, whose radius insets
    // the x range by 3.
    let readings: [(f64, f64, Option<f64>); 12] = [
        (0., -5., Some(40.)),
        (1.5, 33.33, Some(32.95)),
        (3., 47.95, None),
        (4.5, 12.1, Some(-5.15)),
        (6., -15.7, Some(-18.27)),
        (7.5, -30.1, Some(-18.09)),
        (9., -35.91, None),
        (10.5, 2.67, Some(15.6)),
        (12., 40.75, Some(33.27)),
        (13.5, 33.8, Some(40.)),
        (15., 18.48, None),
        (16.5, -9.3, Some(14.6)),
    ];
    let wavy: Rc<[Datum]> = readings
        .iter()
        .map(|(t, v, w)| {
            Datum::new()
                .with("t", *t)
                .with("v", *v)
                .with("w", w.map_or(ChartValue::Null, ChartValue::Number))
        })
        .collect();
    let curves = [CurveType::Monotone, CurveType::Natural, CurveType::Step];
    let mut series: Vec<SeriesDef> = curves
        .iter()
        .map(|c| line("v", LineOptions::new().curve(*c)))
        .collect();
    series.push(line("w", LineOptions::new().dots(true)));
    let all = shapes_on(wavy, "t", &series);
    let svg: Vec<String> = all.iter().map(|s| path_of(&s[0]).to_svg(Some(3))).collect();
    assert!(svg[0].starts_with(
        "M3,144.182C17.5,111.458,32,78.734,46.5,59.159C61,39.583,75.5,26.729,90,26.729"
    ));
    assert!(svg[1].starts_with("M3,144.182C17.5,113.884,32,83.585,46.5,59.159"));
    assert!(svg[2].starts_with("M3,144.182L24.75,144.182L24.75,59.159L68.25,59.159"));
}

#[test]
fn dots_draw_one_circle_per_datum_at_their_radius() {
    let all = shapes(data(), &[dot("v", DotOptions::new().radius(5.))]);
    assert_eq!(all[0].len(), 3);
    for shape in &all[0] {
        let MarkShape::Circle { r, opacity, .. } = shape else {
            panic!();
        };
        // 0.8 by default.
        assert_eq!((*r, *opacity), (5., 0.8));
    }
}

#[test]
fn dots_skip_missing_values() {
    assert_eq!(shapes(gappy(), &[dot("v", DotOptions::new())])[0].len(), 2);
}

#[test]
fn dodge_spreads_coincident_points_around_their_shared_x() {
    let coincident: Rc<[Datum]> = [
        row("A", &[("v", 4.)]),
        row("A", &[("v", 6.)]),
        row("B", &[("v", 2.)]),
    ]
    .into();
    let all = shapes(
        coincident,
        &[dot("v", DotOptions::new().dodge(true).radius(4.))],
    );
    let xs: Vec<f64> = all[0]
        .iter()
        .map(|s| match s {
            MarkShape::Circle { cx, .. } => *cx,
            _ => f64::NAN,
        })
        .collect();
    assert_eq!(xs.len(), 3);
    // The spacing is radius * 2 + 1 = 9, split around the band's center (144).
    assert!(close(xs[1] - xs[0], 9.));
    let center = (xs[0] + xs[1]) / 2.;
    assert!(close(xs[0], center - 4.5));
    assert!(close(xs[0], 139.5) && close(xs[1], 148.5) && xs[2] == 384.);
}

#[test]
fn an_area_fills_translucently_and_strokes_its_top() {
    let green = hex(0x00aa00);
    let all = shapes(data(), &[area("v", AreaOptions::new().color(green))]);
    assert_eq!(all[0].len(), 2);
    let MarkShape::Fill {
        path,
        color,
        opacity,
    } = &all[0][0]
    else {
        panic!();
    };
    assert_eq!((*color, *opacity), (green, 0.3));
    assert_eq!(
        path.to_svg(Some(3)),
        "M99,104.571L264,0L429,174.286L429,244L264,244L99,244Z"
    );
    let MarkShape::Stroke { color, width, .. } = &all[0][1] else {
        panic!();
    };
    assert_eq!((*color, *width), (green, 2.));
}

#[test]
fn an_area_without_stroke_draws_only_its_fill() {
    let all = shapes(data(), &[area("v", AreaOptions::new().stroke(false))]);
    assert_eq!(all[0].len(), 1);
}

#[test]
fn a_gradient_area_fades_its_fill() {
    let all = shapes(data(), &[area("v", AreaOptions::new().gradient(true))]);
    assert!(matches!(
        all[0][0],
        MarkShape::GradientFill { opacity, .. } if opacity == 0.3
    ));
}

#[test]
fn a_band_fills_between_its_bounds() {
    let all = shapes(bounds(), &[band(BandOptions::new("hi", "lo"))]);
    assert_eq!(all[0].len(), 1);
    let MarkShape::Fill {
        path,
        color,
        opacity,
    } = &all[0][0]
    else {
        panic!();
    };
    // 0.15 and the accent color by default.
    assert_eq!((*color, *opacity), (tokens().color.accent, 0.15));
    assert_eq!(
        path.to_svg(Some(3)),
        "M99,108.444L264,27.111L429,81.333L429,189.778L264,135.556L99,216.889Z"
    );
}

#[test]
fn a_band_draws_nothing_when_no_point_has_both_bounds() {
    let missing: Rc<[Datum]> = [row("A", &[("lo", 1.)]), row("B", &[("lo", 4.)])].into();
    assert!(shapes(missing, &[band(BandOptions::new("hi", "lo"))])[0].is_empty());
}

fn ohlc() -> Rc<[Datum]> {
    [
        // Up: the close is at or above the open.
        row("A", &[("o", 2.), ("h", 6.), ("l", 1.), ("c", 5.)]),
        // Down.
        row("B", &[("o", 6.), ("h", 7.), ("l", 2.), ("c", 3.)]),
        row("C", &[("o", 3.), ("h", 5.), ("l", 2.), ("c", 4.)]),
    ]
    .into()
}

fn candles() -> SeriesDef {
    candlestick(CandlestickOptions::new("o", "h", "l", "c"))
}

#[test]
fn candles_draw_a_wick_and_a_body_colored_by_direction() {
    let all = shapes(ohlc(), &[candles()]);
    let t = tokens();
    let rects: Vec<&MarkShape> = all[0]
        .iter()
        .filter(|s| matches!(s, MarkShape::Rect { .. }))
        .collect();
    let wicks = all[0]
        .iter()
        .filter(|s| matches!(s, MarkShape::Segment { .. }))
        .count();
    assert_eq!((rects.len(), wicks), (3, 3));
    let colors: Vec<Hsla> = rects
        .iter()
        .map(|s| match s {
            MarkShape::Rect { color, .. } => *color,
            _ => unreachable!(),
        })
        .collect();
    assert_eq!(colors, [t.color.success, t.color.error, t.color.success]);
    let MarkShape::Rect {
        x,
        y,
        width,
        height,
        ..
    } = rects[0]
    else {
        unreachable!();
    };
    // 60% of the band.
    assert!(close(*x, 59.40000000000003) && close(*y, 87.14285714285712));
    assert!(close(*width, 79.2) && close(*height, 104.5714285714286));
}

#[test]
fn candles_missing_a_field_are_skipped() {
    let gappy: Rc<[Datum]> = [
        ohlc()[0].clone(),
        row("B", &[("o", 6.), ("l", 2.), ("c", 3.)]),
        ohlc()[2].clone(),
    ]
    .into();
    assert_eq!(shapes(gappy, &[candles()])[0].len(), 4);
}

#[test]
fn candles_on_a_linear_x_take_the_smallest_gap() {
    let linear: Rc<[Datum]> = [
        (0., [2., 6., 1., 5.]),
        (3., [6., 7., 2., 3.]),
        (4., [3., 5., 2., 4.]),
    ]
    .iter()
    .map(|(x, [o, h, l, c])| {
        Datum::new()
            .with("x", *x)
            .with("o", *o)
            .with("h", *h)
            .with("l", *l)
            .with("c", *c)
    })
    .collect();
    let all = shapes_on(linear, "x", &[candles()]);
    let MarkShape::Rect { x, width, .. } = &all[0][1] else {
        panic!();
    };
    // The smallest gap is 132 px (x 3 to 4 of 0 to 4 over 528): bodies 79.2 wide.
    assert!(close(*x, -39.6) && close(*width, 79.2));
}

#[test]
fn error_bars_draw_a_stem_and_two_caps_per_point() {
    let all = shapes(bounds(), &[error_bar(ErrorBarOptions::new("hi", "lo"))]);
    assert_eq!(all[0].len(), 9);
    let t = tokens();
    for shape in &all[0] {
        let MarkShape::Segment { color, width, .. } = shape else {
            panic!();
        };
        assert_eq!((*color, *width), (t.color.text_primary, 1.5));
    }
    let MarkShape::Segment { from, to, .. } = &all[0][1] else {
        unreachable!();
    };
    // The upper cap, 8 px wide around the stem.
    assert!(close(from.0, 95.00000000000003) && close(to.0, 103.00000000000003));
    assert!(close(from.1, 108.44444444444444) && from.1 == to.1);
}

#[test]
fn error_bars_missing_a_bound_are_skipped() {
    let gappy: Rc<[Datum]> = [
        bounds()[0].clone(),
        row("B", &[("hi", 8.)]),
        bounds()[2].clone(),
    ]
    .into();
    assert_eq!(
        shapes(gappy, &[error_bar(ErrorBarOptions::new("hi", "lo"))])[0].len(),
        6
    );
}

fn dashed(shapes: &[MarkShape]) -> Vec<((f64, f64), (f64, f64))> {
    shapes
        .iter()
        .filter_map(|s| match s {
            MarkShape::Segment { from, to, dash, .. } if dash.as_slice() == [6., 3.] => {
                Some((*from, *to))
            }
            _ => None,
        })
        .collect()
}

#[test]
fn a_reference_line_draws_a_dashed_line_across_the_plot_with_a_badge() {
    let target = reference_line(ReferenceLineOptions {
        label: Some("Target".into()),
        ..ReferenceLineOptions::y(5.)
    });
    let all = shapes(data(), &[line("v", LineOptions::new()), target]);
    let lines = dashed(&all[1]);
    assert_eq!(lines.len(), 1);
    let ((x1, y1), (x2, y2)) = lines[0];
    assert_eq!((x1, x2), (0., 528.));
    assert_eq!(y1, y2);
    let badge = all[1].iter().find_map(|s| match s {
        MarkShape::Badge { text, height, .. } => Some((text.clone(), *height)),
        _ => None,
    });
    assert_eq!(badge, Some(("Target".into(), 14.)));
}

#[test]
fn a_reference_band_shades_between_two_dashed_lines() {
    let range = reference_line(ReferenceLineOptions::y(2.).y2(6.));
    let all = shapes(data(), &[line("v", LineOptions::new()), range]);
    assert_eq!(dashed(&all[1]).len(), 2);
    let shade = all[1].iter().find_map(|s| match s {
        MarkShape::Rect { width, opacity, .. } => Some((*width, *opacity)),
        _ => None,
    });
    assert_eq!(shade, Some((528., 0.1)));
}

fn numeric() -> Rc<[Datum]> {
    [(0., 1.), (5., 7.), (10., 3.)]
        .iter()
        .map(|(x, v)| Datum::new().with("x", *x).with("v", *v))
        .collect()
}

#[test]
fn a_vertical_reference_line_sits_at_a_numeric_x() {
    let all = shapes_on(
        numeric(),
        "x",
        &[
            line("v", LineOptions::new()),
            reference_line(ReferenceLineOptions::x(5.)),
        ],
    );
    let lines = dashed(&all[1]);
    assert_eq!(lines.len(), 1);
    let ((x1, y1), (x2, y2)) = lines[0];
    assert_eq!(x1, x2);
    assert_eq!((y1, y2), (0., 244.));
    // A band scale has no pixel for an arbitrary number: nothing draws.
    let on_band = shapes(
        data(),
        &[
            bar("v", BarOptions::new()),
            reference_line(ReferenceLineOptions::x(1.)),
        ],
    );
    assert!(on_band[1].is_empty());
}

#[test]
fn reference_badges_stay_inside_the_plot() {
    let badge = |options: ReferenceLineOptions| {
        let all = shapes_on(
            numeric(),
            "x",
            &[line("v", LineOptions::new()), reference_line(options)],
        );
        all[1].iter().find_map(|s| match s {
            MarkShape::Badge { x, y, width, .. } => Some((*x, *y, *width)),
            _ => None,
        })
    };
    // At the start of a horizontal line; the estimated width is 15 × 6.5 + 12.
    assert_eq!(
        badge(ReferenceLineOptions {
            label: Some("High water mark".into()),
            label_position: ReferenceLabelPosition::Start,
            ..ReferenceLineOptions::y(6.5)
        }),
        Some((2., 27.85714285714287, 109.5))
    );
    // Near the right edge of a vertical line, pulled back inside.
    let (x, y, w) = badge(ReferenceLineOptions {
        label: Some("Launch day!".into()),
        label_position: ReferenceLabelPosition::Start,
        ..ReferenceLineOptions::x(9.9)
    })
    .unwrap();
    assert_eq!((x, y, w), (442.5, 226., 83.5));
    // At the left edge, kept 2 px in.
    assert_eq!(
        badge(ReferenceLineOptions {
            label: Some("Start".into()),
            ..ReferenceLineOptions::x(0.)
        }),
        Some((2., 4., 44.5))
    );
    // Near the top, clamped to the plot.
    let (x, y, _) = badge(ReferenceLineOptions {
        label: Some("Top".into()),
        ..ReferenceLineOptions::y(7.2)
    })
    .unwrap();
    assert!(close(x, 494.5) && close(y, 3.4571428571428466));
}

#[test]
fn an_empty_label_draws_no_badge() {
    let all = shapes(
        data(),
        &[
            line("v", LineOptions::new()),
            reference_line(ReferenceLineOptions {
                label: Some("".into()),
                ..ReferenceLineOptions::y(3.)
            }),
        ],
    );
    assert_eq!(all[1].len(), 1);
    assert!(matches!(all[1][0], MarkShape::Segment { .. }));
}

#[test]
fn a_non_finite_reference_draws_nothing() {
    let all = shapes(
        data(),
        &[
            line("v", LineOptions::new()),
            reference_line(ReferenceLineOptions::y(f64::NAN)),
        ],
    );
    assert!(all[1].is_empty());
}

#[test]
fn marks_with_no_data_draw_nothing() {
    let empty: Rc<[Datum]> = Rc::from(Vec::<Datum>::new());
    let series = [
        bar("v", BarOptions::new()),
        line("v", LineOptions::new()),
        area("v", AreaOptions::new()),
        dot("v", DotOptions::new()),
        band(BandOptions::new("a", "b")),
        candles(),
        error_bar(ErrorBarOptions::new("a", "b")),
    ];
    assert!(shapes(empty, &series).iter().all(Vec::is_empty));
}
