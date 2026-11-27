//! The generators against d3's own output: every expected path and stack was produced
//! by running d3-shape 3.2.0 (with d3-path 3.1.0) under Node.

use gpui::{point, px};

use super::*;

const NAN: f64 = f64::NAN;
const INF: f64 = f64::INFINITY;

/// A series with a gap (the NaN), a flat run and a rise.
const POINTS: [(f64, f64); 8] = [
    (0., 10.),
    (20., 30.5),
    (40., NAN),
    (60., 20.),
    (80., 25.123456),
    (100., 5.),
    (120., 5.),
    (140., 40.),
];

/// A stack case: rows of values by key, the key count, the stacked series.
type StackCase<'a> = (&'a [&'a [f64]], usize, &'a [&'a [[f64; 2]]]);

fn finite(d: &(f64, f64), _: usize) -> bool {
    d.0.is_finite() && d.1.is_finite()
}

fn line(curve: Curve) -> Line<'static, (f64, f64)> {
    Line::new()
        .x(|d: &(f64, f64), _| d.0)
        .y(|d, _| d.1)
        .curve(curve)
}

fn area(curve: Curve) -> Area<'static, (f64, f64)> {
    Area::new()
        .x(|d: &(f64, f64), _| d.0)
        .y0(|_, _| 50.)
        .y1(|d, _| d.1)
        .curve(curve)
}

#[test]
fn curves_match_d3() {
    // (curve, line with gaps, area with gaps, one-point line, one-point area,
    // two-point line, three-point area)
    let cases: &[(Curve, &str, &str, &str, &str, &str, &str)] = &[
        (
            Curve::Linear,
            "M0,10L20,30.5M60,20L80,25.123L100,5L120,5L140,40",
            "M0,10L20,30.5L20,50L0,50ZM60,20L80,25.123L100,5L120,5L140,40L140,50L120,50L100,50L80,50L60,50Z",
            "M0,10Z",
            "M0,10L0,50Z",
            "M0,10L20,30.5",
            "M60,20L80,25.123L100,5L100,50L80,50L60,50Z",
        ),
        (
            Curve::MonotoneX,
            "M0,10L20,30.5M60,20C66.667,22.562,73.333,25.123,80,25.123C86.667,25.123,93.333,5,100,5C106.667,5,113.333,5,120,5C126.667,5,133.333,22.5,140,40",
            "M0,10L20,30.5L20,50L0,50ZM60,20C66.667,22.562,73.333,25.123,80,25.123C86.667,25.123,93.333,5,100,5C106.667,5,113.333,5,120,5C126.667,5,133.333,22.5,140,40L140,50C133.333,50,126.667,50,120,50C113.333,50,106.667,50,100,50C93.333,50,86.667,50,80,50C73.333,50,66.667,50,60,50Z",
            "M0,10Z",
            "M0,10L0,50Z",
            "M0,10L20,30.5",
            "M60,20C66.667,22.562,73.333,25.123,80,25.123C86.667,25.123,93.333,15.062,100,5L100,50C93.333,50,86.667,50,80,50C73.333,50,66.667,50,60,50Z",
        ),
        (
            Curve::Natural,
            "M0,10L20,30.5M60,20C66.667,24.233,73.333,28.466,80,25.123C86.667,21.781,93.333,10.864,100,5C106.667,-0.864,113.333,-1.675,120,5C126.667,11.675,133.333,25.838,140,40",
            "M0,10L20,30.5L20,50L0,50ZM60,20C66.667,24.233,73.333,28.466,80,25.123C86.667,21.781,93.333,10.864,100,5C106.667,-0.864,113.333,-1.675,120,5C126.667,11.675,133.333,25.838,140,40L140,50C133.333,50,126.667,50,120,50C113.333,50,106.667,50,100,50C93.333,50,86.667,50,80,50C73.333,50,66.667,50,60,50Z",
            "M0,10Z",
            "M0,10L0,50Z",
            "M0,10L20,30.5",
            "M60,20C66.667,23.812,73.333,27.623,80,25.123C86.667,22.623,93.333,13.812,100,5L100,50C93.333,50,86.667,50,80,50C73.333,50,66.667,50,60,50Z",
        ),
        (
            Curve::Step,
            "M0,10L10,10L10,30.5L20,30.5M60,20L70,20L70,25.123L90,25.123L90,5L110,5L110,5L130,5L130,40L140,40",
            "M0,10L10,10L10,30.5L20,30.5L20,50L10,50L10,50L0,50ZM60,20L70,20L70,25.123L90,25.123L90,5L110,5L110,5L130,5L130,40L140,40L140,50L130,50L130,50L110,50L110,50L90,50L90,50L70,50L70,50L60,50Z",
            "M0,10Z",
            "M0,10L0,50Z",
            "M0,10L10,10L10,30.5L20,30.5",
            "M60,20L70,20L70,25.123L90,25.123L90,5L100,5L100,50L90,50L90,50L70,50L70,50L60,50Z",
        ),
        (
            Curve::StepBefore,
            "M0,10L0,30.5L20,30.5M60,20L60,25.123L80,25.123L80,5L100,5L100,5L120,5L120,40L140,40",
            "M0,10L0,30.5L20,30.5L20,50L0,50L0,50ZM60,20L60,25.123L80,25.123L80,5L100,5L100,5L120,5L120,40L140,40L140,50L120,50L120,50L100,50L100,50L80,50L80,50L60,50L60,50Z",
            "M0,10Z",
            "M0,10L0,50Z",
            "M0,10L0,30.5L20,30.5",
            "M60,20L60,25.123L80,25.123L80,5L100,5L100,50L80,50L80,50L60,50L60,50Z",
        ),
        (
            Curve::StepAfter,
            "M0,10L20,10L20,30.5M60,20L80,20L80,25.123L100,25.123L100,5L120,5L120,5L140,5L140,40",
            "M0,10L20,10L20,30.5L20,50L20,50L0,50ZM60,20L80,20L80,25.123L100,25.123L100,5L120,5L120,5L140,5L140,40L140,50L140,50L120,50L120,50L100,50L100,50L80,50L80,50L60,50Z",
            "M0,10Z",
            "M0,10L0,50Z",
            "M0,10L20,10L20,30.5",
            "M60,20L80,20L80,25.123L100,25.123L100,5L100,50L100,50L80,50L80,50L60,50Z",
        ),
    ];
    for &(curve, gaps, area_gaps, one, area_one, two, area_three) in cases {
        let svg = |path: ChartPath| path.to_svg(Some(3));
        assert_eq!(
            svg(line(curve).defined(finite).generate(&POINTS)),
            gaps,
            "{curve:?}"
        );
        assert_eq!(
            svg(area(curve).defined(finite).generate(&POINTS)),
            area_gaps,
            "{curve:?}"
        );
        assert_eq!(svg(line(curve).generate(&POINTS[..1])), one, "{curve:?}");
        assert_eq!(
            svg(area(curve).generate(&POINTS[..1])),
            area_one,
            "{curve:?}"
        );
        assert_eq!(svg(line(curve).generate(&POINTS[..2])), two, "{curve:?}");
        assert_eq!(
            svg(area(curve).generate(&POINTS[3..6])),
            area_three,
            "{curve:?}"
        );
    }
}

#[test]
fn monotone_skips_coincident_points() {
    let data = [(0., 0.), (0., 0.), (10., 5.), (10., 8.), (20., 2.)];
    let path = line(Curve::MonotoneX).generate(&data);
    assert_eq!(
        path.to_svg(Some(3)),
        "M0,0C3.333,0.833,6.667,1.667,10,5C10,5,10,8,10,8C13.333,8,16.667,5,20,2"
    );
}

#[test]
fn a_line_without_defined_writes_nan() {
    assert_eq!(
        line(Curve::Linear).generate(&POINTS).to_svg(Some(3)),
        "M0,10L20,30.5L40,NaNL60,20L80,25.123L100,5L120,5L140,40"
    );
}

#[test]
fn area_accessors_match_d3() {
    let x0_x1 = Area::new()
        .x0(|d: &(f64, f64), _| d.0)
        .x1(|d, _| d.0 + 5.)
        .y0(|_, _| 0.)
        .y1(|d, _| d.1)
        .defined(finite)
        .generate(&POINTS);
    assert_eq!(
        x0_x1.to_svg(Some(3)),
        "M5,10L25,30.5L20,0L0,0ZM65,20L85,25.123L105,5L125,5L145,40L140,0L120,0L100,0L80,0L60,0Z"
    );
    // `y` clears y1, so the topline is the baseline.
    let flat = Area::new()
        .x(|d: &(f64, f64), _| d.0)
        .y(|d, _| d.1)
        .generate(&POINTS[..2]);
    assert_eq!(flat.to_svg(Some(3)), "M0,10L20,30.5L20,30.5L0,10Z");
    // The baseline defaults to 0.
    let zero = Area::new()
        .x(|d: &(f64, f64), _| d.0)
        .y1(|d, _| d.1)
        .generate(&POINTS[..2]);
    assert_eq!(zero.to_svg(Some(3)), "M0,10L20,30.5L20,0L0,0Z");
}

#[test]
fn path_digits_match_d3() {
    let data = [(1. / 3., -0.0004), (2.5, 1e21), (-1.23456789, 1e-7)];
    let path = line(Curve::Linear).generate(&data);
    // (digits, line.digits(digits)(data))
    let cases: &[(Option<u32>, &str)] = &[
        (
            None,
            "M0.3333333333333333,-0.0004L2.5,1e+21L-1.23456789,1e-7",
        ),
        (Some(0), "M0,0L3,1e+21L-1,0"),
        (Some(1), "M0.3,0L2.5,1e+21L-1.2,0"),
        (
            Some(16),
            "M0.3333333333333333,-0.0004L2.5,1e+21L-1.23456789,1e-7",
        ),
    ];
    for &(digits, want) in cases {
        assert_eq!(path.to_svg(digits), want, "{digits:?}");
    }
}

#[test]
fn nothing_drawn_is_an_empty_path() {
    // d3 returns null for each of these.
    let none: [(f64, f64); 0] = [];
    assert!(line(Curve::Linear).generate(&none).is_empty());
    assert!(area(Curve::Natural).generate(&none).is_empty());
    assert!(
        line(Curve::Step)
            .defined(|_, _| false)
            .generate(&POINTS)
            .is_empty()
    );
    assert!(
        area(Curve::MonotoneX)
            .defined(|_, _| false)
            .generate(&POINTS)
            .is_empty()
    );
    let mut path = ChartPath::new();
    path.close_path();
    assert!(path.is_empty());
    assert_eq!(path.to_svg(Some(3)), "");
}

#[test]
fn stack_matches_d3() {
    // (rows of values by key, key count, stack().keys(keys)(rows))
    let cases: &[StackCase] = &[
        (
            &[&[1.0, 2.0, 3.0], &[4.0, 5.0, 6.0]],
            3,
            &[
                &[[0.0, 1.0], [0.0, 4.0]],
                &[[1.0, 3.0], [4.0, 9.0]],
                &[[3.0, 6.0], [9.0, 15.0]],
            ],
        ),
        (
            &[&[10.0, -4.0], &[-3.0, 8.0], &[0.0, 0.0]],
            2,
            &[
                &[[0.0, 10.0], [0.0, -3.0], [0.0, 0.0]],
                &[[10.0, 6.0], [-3.0, 5.0], [0.0, 0.0]],
            ],
        ),
        (
            &[&[1.0, NAN, 2.0], &[NAN, NAN, 3.0], &[5.0, 1.0, NAN]],
            3,
            &[
                &[[0.0, 1.0], [0.0, NAN], [0.0, 5.0]],
                &[[1.0, NAN], [0.0, NAN], [5.0, 6.0]],
                &[[1.0, 3.0], [0.0, 3.0], [6.0, NAN]],
            ],
        ),
        (&[], 2, &[&[], &[]]),
        (&[&[], &[]], 0, &[]),
        (
            &[&[INF, 1.0], &[1e+308, 1e+308]],
            2,
            &[&[[0.0, INF], [0.0, 1e+308]], &[[INF, INF], [1e+308, INF]]],
        ),
    ];
    for &(rows, keys, want) in cases {
        let got = stack(rows.len(), keys, |row, key| rows[row][key]);
        assert_eq!(got.len(), want.len());
        for (layer, want) in got.iter().zip(want) {
            assert_eq!(layer.len(), want.len());
            for (a, b) in layer.iter().zip(*want) {
                let same =
                    |x: f64, y: f64| (x.is_nan() && y.is_nan()) || x.to_bits() == y.to_bits();
                assert!(same(a[0], b[0]) && same(a[1], b[1]), "{got:?} != {want:?}");
            }
        }
    }
    assert_eq!(order_none(3), vec![0, 1, 2]);
}

#[test]
fn paths_build_for_gpui() {
    let origin = point(px(10.), px(20.));
    let mut path = ChartPath::new();
    assert!(path.build(origin, PathStyle::Fill).is_none());
    path.move_to(0., 0.);
    path.quadratic_curve_to(5., 10., 10., 0.);
    path.bezier_curve_to(12., 5., 14., 5., 16., 0.);
    path.line_to(16., 20.);
    path.close_path();
    assert_eq!(path.to_svg(Some(3)), "M0,0Q5,10,10,0C12,5,14,5,16,0L16,20Z");
    assert!(path.build(origin, PathStyle::Fill).is_some());
    assert!(
        path.build(origin, PathStyle::Stroke { width: 2. })
            .is_some()
    );
    assert_eq!(path.commands().len(), 5);
    assert_eq!(path.commands()[0], PathCommand::MoveTo { x: 0., y: 0. });
}

#[test]
fn a_path_builds_up_to_its_first_unreadable_command() {
    // A browser stops at the first command it cannot read, as d3 writes NaN as "NaN".
    let origin = point(px(0.), px(0.));
    let mut readable = ChartPath::new();
    readable.move_to(0., 0.);
    readable.line_to(10., 0.);
    readable.line_to(10., 10.);
    let mut broken = readable.clone();
    broken.line_to(NAN, 5.);
    broken.line_to(0., 10.);
    broken.close_path();
    let build = |path: &ChartPath| format!("{:?}", path.build(origin, PathStyle::Fill));
    assert_eq!(build(&broken), build(&readable));
    let mut unreadable = ChartPath::new();
    unreadable.move_to(INF, 0.);
    unreadable.line_to(1., 1.);
    assert!(unreadable.build(origin, PathStyle::Fill).is_none());
    // Past f32's range is past what the pixels can hold.
    let mut huge = ChartPath::new();
    huge.move_to(1e39, 0.);
    assert!(
        huge.build(origin, PathStyle::Stroke { width: 1. })
            .is_none()
    );
}
