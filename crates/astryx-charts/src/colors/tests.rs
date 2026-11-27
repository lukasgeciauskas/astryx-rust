//! Upstream `getChartColors.test.ts` and `useChartColors.test.ts`. Upstream's
//! resolvers answer with CSS strings; here an `Hsla` stands for each token, and a
//! value upstream cannot parse (`var()`, an empty string) is a token the resolver
//! cannot answer.

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::sync::Arc;

use astryx_theme::{Appearance, Theme, ThemeMode, TokenValue, Tokens};
use gpui::{Hsla, Rgba, TestAppContext, rgb};

use super::*;

/// A color as upstream's `formatColor` writes it: `#RRGGBB` when opaque, else
/// `rgba(r, g, b, a)`.
fn css(color: Hsla) -> String {
    let c = color.to_rgb();
    let byte = |v: f32| (f64::from(v) * 255.).round() as u8;
    if c.a >= 1. {
        format!("#{:02X}{:02X}{:02X}", byte(c.r), byte(c.g), byte(c.b))
    } else {
        let a = (f64::from(c.a) * 10_000.).round() / 10_000.;
        format!("rgba({}, {}, {}, {a})", byte(c.r), byte(c.g), byte(c.b))
    }
}

fn hex(value: u32) -> Hsla {
    rgb(value).into()
}

fn css_all(colors: &[Hsla]) -> Vec<String> {
    colors.iter().copied().map(css).collect()
}

/// A hand-picked greyscale ramp, so interpolated values are easy to verify by
/// hand: 0x00, 0x40, 0x80, 0xC0, 0xFF from step 5 to 1.
fn ramp(name: &str) -> Option<Hsla> {
    Some(match name {
        "--color-data-gray-5" => hex(0x000000),
        "--color-data-gray-4" => hex(0x404040),
        "--color-data-gray-3" => hex(0x808080),
        "--color-data-gray-2" => hex(0xC0C0C0),
        "--color-data-gray-1" => hex(0xFFFFFF),
        _ => return None,
    })
}

#[test]
fn returns_exact_token_stops_when_n_fits_the_ramp() {
    let colors = ChartColors::from_resolver(ramp);
    assert_eq!(
        css_all(&colors.sequential(SequentialHue::Gray, 5)),
        ["#000000", "#404040", "#808080", "#C0C0C0", "#FFFFFF"]
    );
}

#[test]
fn interpolates_between_stops_when_n_exceeds_the_ramp() {
    let colors = ChartColors::from_resolver(ramp);
    // n = 9 over 5 stops: odd indices land halfway between adjacent stops.
    assert_eq!(
        css_all(&colors.sequential(SequentialHue::Gray, 9)),
        [
            "#000000", "#202020", "#404040", "#606060", "#808080", "#A0A0A0", "#C0C0C0", "#E0E0E0",
            "#FFFFFF"
        ]
    );
}

#[test]
fn falls_back_to_the_nearer_endpoint_when_a_stop_cannot_be_parsed() {
    let colors = ChartColors::from_resolver(|name| {
        if name == "--color-data-gray-5" {
            None
        } else {
            ramp(name)
        }
    });
    let nine = colors.sequential(SequentialHue::Gray, 9);
    // The unparseable first stop keeps its slot and is the nearer endpoint of
    // the first interpolated position's other side.
    assert_eq!(nine[0], gpui::transparent_black());
    assert_eq!(css(nine[1]), "#404040");
}

#[test]
fn alpha_applies_opacity_to_a_color() {
    let colors = ChartColors::from_resolver(ramp);
    assert_eq!(
        css(colors.alpha(hex(0x0064E0), 0.2)),
        "rgba(0, 100, 224, 0.2)"
    );
}

#[test]
fn alpha_overrides_the_alpha_a_color_has() {
    let colors = ChartColors::from_resolver(ramp);
    let translucent: Hsla = Rgba {
        r: 0.,
        g: 100. / 255.,
        b: 224. / 255.,
        a: 0.9,
    }
    .into();
    assert_eq!(
        css(colors.alpha(translucent, 0.2)),
        "rgba(0, 100, 224, 0.2)"
    );
}

#[test]
fn alpha_clamps_out_of_range_opacity_and_defaults_non_finite_opacity_to_one() {
    let colors = ChartColors::from_resolver(ramp);
    let black = hex(0x000000);
    assert_eq!(css(colors.alpha(black, 2.)), "#000000");
    assert_eq!(css(colors.alpha(black, -1.)), "rgba(0, 0, 0, 0)");
    assert_eq!(css(colors.alpha(black, f64::NAN)), "#000000");
    assert_eq!(css(colors.alpha(black, f64::INFINITY)), "#000000");
}

#[test]
fn alpha_rounds_to_four_decimals_as_to_fixed_does() {
    let colors = ChartColors::from_resolver(ramp);
    // 0.03125 is exactly halfway; toFixed takes the larger neighbour.
    assert_eq!(colors.alpha(hex(0x000000), 0.03125).a, 0.0313);
    assert_eq!(colors.alpha(hex(0x000000), 0.123_449).a, 0.1234);
    assert_eq!(colors.alpha(hex(0x000000), 0.99999).a, 1.);
    assert_eq!(to_fixed_4(0.00005), 0.0001);
}

/// Every token resolves to a color of its own, so each output names its source.
fn by_name(name: &str) -> Option<Hsla> {
    let i = Tokens::NAMES.iter().position(|n| *n == name)?;
    Some(
        Rgba {
            r: (i % 256) as f32 / 255.,
            g: (i / 256) as f32 / 255.,
            b: 0.5,
            a: 1.,
        }
        .into(),
    )
}

fn name_of(color: Hsla) -> &'static str {
    Tokens::NAMES
        .iter()
        .find(|n| by_name(n) == Some(color))
        .copied()
        .unwrap_or("")
}

fn names(colors: &[Hsla]) -> Vec<&'static str> {
    colors.iter().copied().map(name_of).collect()
}

/// The ten categorical data tokens, in the palette's declaration order.
const CATEGORICAL: [&str; 10] = [
    "--color-data-categorical-blue",
    "--color-data-categorical-orange",
    "--color-data-categorical-purple",
    "--color-data-categorical-green",
    "--color-data-categorical-pink",
    "--color-data-categorical-cyan",
    "--color-data-categorical-red",
    "--color-data-categorical-teal",
    "--color-data-categorical-brown",
    "--color-data-categorical-indigo",
];

const GRAY_1: &str = "--color-data-gray-1";

fn steps(hue: &str, numbers: &[u8]) -> Vec<String> {
    numbers
        .iter()
        .map(|step| format!("--color-data-{hue}-{step}"))
        .collect()
}

#[test]
fn never_reads_a_token_name_outside_the_token_vocabulary() {
    let requested = RefCell::new(BTreeSet::new());
    let colors = ChartColors::from_resolver(|name| {
        requested.borrow_mut().insert(name.to_string());
        by_name(name)
    });
    colors.categorical(CATEGORICAL.len());
    for hue in SequentialHue::ALL {
        colors.sequential(hue, 5);
    }
    colors.diverging().positive_negative(11);
    colors.diverging().cold_hot(11);
    let unknown: Vec<String> = requested
        .into_inner()
        .into_iter()
        .filter(|name| !Tokens::NAMES.contains(&name.as_str()))
        .collect();
    assert_eq!(unknown, Vec::<String>::new());
}

#[test]
fn categorical_returns_the_slots_in_order_as_a_prefix_for_smaller_counts() {
    let colors = ChartColors::from_resolver(by_name);
    assert_eq!(names(&colors.categorical(10)), CATEGORICAL);
    assert_eq!(names(&colors.categorical(3)), CATEGORICAL[..3]);
    assert!(colors.categorical(0).is_empty());
}

#[test]
fn categorical_wraps_past_the_tenth_slot_back_to_the_first() {
    let colors = ChartColors::from_resolver(by_name);
    let expected: Vec<&str> = CATEGORICAL
        .iter()
        .chain(&CATEGORICAL)
        .chain(&CATEGORICAL[..3])
        .copied()
        .collect();
    assert_eq!(names(&colors.categorical(23)), expected);
}

#[test]
fn sequential_reads_each_ramp_from_step_5_to_step_1() {
    let colors = ChartColors::from_resolver(by_name);
    for hue in SequentialHue::ALL {
        assert_eq!(
            names(&colors.sequential(hue, 5)),
            steps(hue.name(), &[5, 4, 3, 2, 1]),
            "{hue:?}"
        );
    }
}

#[test]
fn sequential_picks_evenly_spaced_stops_for_fewer_colors_and_the_middle_one_for_one() {
    let colors = ChartColors::from_resolver(by_name);
    let blue = |n| names(&colors.sequential(SequentialHue::Blue, n));
    assert_eq!(blue(1), steps("blue", &[3]));
    assert_eq!(blue(2), steps("blue", &[5, 1]));
    assert_eq!(blue(3), steps("blue", &[5, 3, 1]));
    assert_eq!(blue(4), steps("blue", &[5, 4, 2, 1]));
    assert!(blue(0).is_empty());
}

#[test]
fn cold_hot_runs_from_blue_to_red() {
    // Each side takes n / 2 colors, sampled like a sequential ramp: the first
    // hue runs from step 5 to step 1, the second from step 1 to step 5, and an
    // odd count puts gray-1 in the center.
    let join = |neg: &[u8], center: bool, pos: &[u8]| {
        let mut out = steps("blue", neg);
        if center {
            out.push(GRAY_1.to_string());
        }
        out.extend(steps("red", pos));
        out
    };
    let cases: [(usize, Vec<String>); 11] = [
        (0, Vec::new()),
        (1, vec![GRAY_1.to_string()]),
        (2, join(&[3], false, &[3])),
        (3, join(&[3], true, &[3])),
        (4, join(&[5, 1], false, &[1, 5])),
        (5, join(&[5, 1], true, &[1, 5])),
        (6, join(&[5, 3, 1], false, &[1, 3, 5])),
        (7, join(&[5, 3, 1], true, &[1, 3, 5])),
        (8, join(&[5, 4, 2, 1], false, &[1, 2, 4, 5])),
        (10, join(&[5, 4, 3, 2, 1], false, &[1, 2, 3, 4, 5])),
        (11, join(&[5, 4, 3, 2, 1], true, &[1, 2, 3, 4, 5])),
    ];
    let colors = ChartColors::from_resolver(by_name);
    for (n, expected) in cases {
        assert_eq!(
            names(&colors.diverging().cold_hot(n)),
            expected,
            "coldHot({n})"
        );
    }
}

#[test]
fn positive_negative_runs_from_shamrock_to_red() {
    let colors = ChartColors::from_resolver(by_name);
    let mut expected = steps("shamrock", &[5, 1]);
    expected.push(GRAY_1.to_string());
    expected.extend(steps("red", &[1, 5]));
    assert_eq!(names(&colors.diverging().positive_negative(5)), expected);
}

#[test]
fn custom_uses_the_given_hues_and_a_midpoint_replaces_gray_1() {
    let colors = ChartColors::from_resolver(by_name);
    let custom = |n, mid| {
        css_or_names(
            &colors
                .diverging()
                .custom(SequentialHue::Purple, SequentialHue::Teal, n, mid),
        )
    };
    let side = |mid: &str| {
        let mut out = steps("purple", &[5, 1]);
        out.push(mid.to_string());
        out.extend(steps("teal", &[1, 5]));
        out
    };
    assert_eq!(custom(5, None), side(GRAY_1));
    assert_eq!(custom(5, Some(hex(0xABCDEF))), side("#ABCDEF"));
    assert_eq!(custom(1, Some(hex(0xABCDEF))), ["#ABCDEF"]);
    let mut four = steps("purple", &[5, 1]);
    four.extend(steps("teal", &[1, 5]));
    assert_eq!(custom(4, Some(hex(0xABCDEF))), four);
}

/// Token names where a color is one, else the color written out.
fn css_or_names(colors: &[Hsla]) -> Vec<String> {
    colors
        .iter()
        .map(|&c| match name_of(c) {
            "" => css(c),
            name => name.to_string(),
        })
        .collect()
}

#[test]
fn semantic_roles_map_to_data_tokens() {
    let semantic = ChartColors::from_resolver(by_name).semantic;
    assert_eq!(
        [
            semantic.positive,
            semantic.negative,
            semantic.warning,
            semantic.neutral
        ]
        .map(name_of),
        [
            "--color-data-categorical-green",
            "--color-data-categorical-red",
            "--color-data-categorical-orange",
            "--color-data-neutral",
        ]
    );
}

#[test]
fn structural_roles_map_to_core_border_and_text_tokens() {
    let structural = ChartColors::from_resolver(by_name).structural;
    assert_eq!(
        [
            structural.axis,
            structural.grid,
            structural.tick,
            structural.label
        ]
        .map(name_of),
        [
            "--color-border-emphasized",
            "--color-border",
            "--color-border-emphasized",
            "--color-text-secondary",
        ]
    );
}

#[test]
fn returns_resolved_values_verbatim_without_normalizing_or_dropping_them() {
    let odd = gpui::hsla(0.123_456, 0.654_321, 0.333_333, 0.3);
    let border: Hsla = Rgba {
        r: 5. / 255.,
        g: 54. / 255.,
        b: 89. / 255.,
        a: 0.1,
    }
    .into();
    let values = |name: &str| -> Option<Hsla> {
        Some(match name {
            "--color-data-categorical-blue" => hex(0x0171E3),
            "--color-data-categorical-orange" => hex(0xEB6E00),
            "--color-data-categorical-green" => odd,
            "--color-data-teal-3" => odd,
            "--color-border" => border,
            _ => return None,
        })
    };
    let colors = ChartColors::from_resolver(values);
    assert_eq!(
        colors.categorical(5),
        [
            hex(0x0171E3),
            hex(0xEB6E00),
            gpui::transparent_black(),
            odd,
            gpui::transparent_black()
        ]
    );
    assert_eq!(colors.semantic.positive, odd);
    assert_eq!(colors.structural.grid, border);
    assert_eq!(colors.sequential(SequentialHue::Teal, 1), [odd]);
}

#[test]
fn token_values_of_another_kind_are_no_color() {
    let colors = ChartColors::from_resolver(|name| {
        Some(if name == "--color-data-neutral" {
            TokenValue::Number(1.)
        } else {
            TokenValue::Color(hex(0x123456))
        })
    });
    assert_eq!(colors.semantic.neutral, gpui::transparent_black());
    assert_eq!(css(colors.semantic.positive), "#123456");
}

fn test_theme() -> Theme {
    Theme::default()
        .with_token_values(
            Some(Appearance::Light),
            [(
                "--color-data-categorical-blue",
                TokenValue::Color(hex(0x111111)),
            )],
        )
        .and_then(|t| {
            t.with_token_values(
                Some(Appearance::Dark),
                [(
                    "--color-data-categorical-blue",
                    TokenValue::Color(hex(0x222222)),
                )],
            )
        })
        .and_then(|t| {
            t.with_token_values(
                Some(Appearance::Light),
                [("--color-data-red-5", TokenValue::Color(hex(0x333333)))],
            )
        })
        .and_then(|t| {
            t.with_token_values(
                Some(Appearance::Dark),
                [("--color-data-red-5", TokenValue::Color(hex(0x444444)))],
            )
        })
        .unwrap_or_else(|e| panic!("{e:?}"))
}

#[test]
fn theme_values_resolve_for_the_requested_appearance() {
    let theme = test_theme();
    let light = ChartColors::from_theme(&theme, Appearance::default());
    let dark = ChartColors::from_theme(&theme, Appearance::Dark);
    assert_eq!(css_all(&light.categorical(1)), ["#111111"]);
    assert_eq!(css_all(&dark.categorical(1)), ["#222222"]);
    assert_eq!(css(light.sequential(SequentialHue::Red, 5)[0]), "#333333");
    assert_eq!(css(dark.sequential(SequentialHue::Red, 5)[0]), "#444444");
}

#[test]
fn a_data_token_the_theme_does_not_set_reads_as_its_default() {
    // The palette default is light-dark(#8494A3, #8C939B).
    let theme = test_theme();
    assert_eq!(
        css(ChartColors::from_theme(&theme, Appearance::Light)
            .semantic
            .neutral),
        "#8494A3"
    );
    assert_eq!(
        css(ChartColors::from_theme(&theme, Appearance::Dark)
            .semantic
            .neutral),
        "#8C939B"
    );
}

#[gpui::test]
fn use_chart_colors_returns_distinct_wrapping_colors(cx: &mut TestAppContext) {
    cx.update(astryx_theme::init);
    let cx = cx.add_empty_window();
    let colors = cx.update(|window, cx| use_chart_colors(window, cx));
    let four = colors.categorical(4);
    assert_eq!(four.len(), 4);
    assert!(four.iter().all(|c| c.a > 0.), "{four:?}");
    // Distinct within the first palette cycle.
    let distinct: BTreeSet<String> = four.iter().copied().map(css).collect();
    assert_eq!(distinct.len(), 4);
    // Ten categorical tokens: slots 10 and 11 reuse slots 0 and 1.
    let twelve = colors.categorical(12);
    assert_eq!(twelve.len(), 12);
    assert_eq!(twelve[10], twelve[0]);
    assert_eq!(twelve[11], twelve[1]);
    assert!(colors.categorical(0).is_empty());
}

#[gpui::test]
fn use_chart_colors_reads_the_surrounding_theme_and_its_mode(cx: &mut TestAppContext) {
    cx.update(|cx| {
        astryx_theme::set_theme(Arc::new(test_theme()), cx);
        astryx_theme::set_mode(ThemeMode::Dark, cx);
    });
    let cx = cx.add_empty_window();
    let (first, second) =
        cx.update(|window, cx| (use_chart_colors(window, cx), use_chart_colors(window, cx)));
    assert_eq!(css_all(&first.categorical(1)), ["#222222"]);
    // The palette default for --color-data-neutral is light-dark(#8494A3, #8C939B).
    assert_eq!(css(first.semantic.neutral), "#8C939B");
    // The same theme and mode give the same palette.
    assert_eq!(first, second);
}

/// Palettes of the shipped themes, checked against upstream's
/// `getChartColorsFromResolver` under Node fed the same token values: sampled and
/// interpolated colors equal upstream's hex.
#[test]
fn shipped_theme_palettes_match_upstream() {
    let neutral = astryx_themes::neutral::theme();
    let butter = astryx_themes::butter::theme();
    let light = ChartColors::from_theme(&neutral, Appearance::Light);
    assert_eq!(
        css_all(&light.diverging().cold_hot(13)),
        [
            "#02165E", "#0041A9", "#1777E4", "#47A5FE", "#8CC7FF", "#DBECFF", "#F1F4F7", "#FEE4E6",
            "#FFBCC1", "#FD929B", "#EB5264", "#C80F2B", "#9D0519"
        ]
    );
    assert_eq!(
        css_all(&light.diverging().positive_negative(7)),
        [
            "#0B603D", "#24BB5E", "#D6FEE4", "#F1F4F7", "#FEE4E6", "#FB7D87", "#9D0519"
        ]
    );
    let dark = ChartColors::from_theme(&neutral, Appearance::Dark);
    assert_eq!(
        css_all(&dark.sequential(SequentialHue::Orange, 15)),
        [
            "#A13F04", "#B04903", "#BF5202", "#CE5C01", "#DC6808", "#E77718", "#F28627", "#FD9537",
            "#FD9F49", "#FDA95B", "#FDB36D", "#FDBF83", "#FECC9C", "#FED9B6", "#FFE6CF"
        ]
    );
    let butter_dark = ChartColors::from_theme(&butter, Appearance::Dark);
    assert_eq!(
        css_all(&butter_dark.sequential(SequentialHue::Blue, 12)),
        [
            "#02165E", "#012A80", "#013DA2", "#0353C2", "#116DDA", "#1F87F2", "#359CFE", "#53ABFF",
            "#71BAFF", "#93CBFF", "#B7DBFF", "#DBECFF"
        ]
    );
    assert_eq!(
        css_all(&ChartColors::from_theme(&butter, Appearance::Light).categorical(3)),
        ["#0171E3", "#EB6E00", "#6B1EFD"]
    );
}
