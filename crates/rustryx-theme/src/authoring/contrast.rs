//! WCAG 2.x contrast for theme authoring: upstream `theme/contrast.ts`.
//!
//! Backs the contrast guarantees of generated color tokens (WCAG 1.4.3 text
//! contrast of at least 4.5:1, WCAG 1.4.11 non-text contrast of at least
//! 3:1). A translucent foreground has no contrast of its own, only against
//! what it paints on, so it is composited over the background first, in
//! gamma-encoded sRGB as CSS composites it.

use std::fmt;

use super::color::{Rgba, parse_color, srgb_to_linear};
use super::js::{js_max, js_min};

/// The WCAG 2.x relative luminance of an sRGB color, alpha ignored: 0 is
/// black and 1 white. Upstream `relativeLuminance` (`theme/contrast.ts`).
///
/// <https://www.w3.org/WAI/WCAG22/Techniques/general/G18>
pub fn relative_luminance(color: Rgba) -> f64 {
    0.2126 * srgb_to_linear(color.r)
        + 0.7152 * srgb_to_linear(color.g)
        + 0.0722 * srgb_to_linear(color.b)
}

/// A possibly translucent foreground painted over an opaque backdrop with
/// source-over blending in gamma-encoded sRGB, as CSS paints translucent
/// tokens. The result is opaque. Upstream `compositeOver`
/// (`theme/contrast.ts`).
pub fn composite_over(foreground: Rgba, backdrop: Rgba) -> Rgba {
    let a = foreground.a;
    Rgba {
        r: foreground.r * a + backdrop.r * (1.0 - a),
        g: foreground.g * a + backdrop.g * (1.0 - a),
        b: foreground.b * a + backdrop.b * (1.0 - a),
        a: 1.0,
    }
}

/// A color [`contrast_ratio`] takes: a CSS color string, parsed with
/// [`parse_color`], or an already parsed [`Rgba`]. Upstream's
/// `string | RGBA`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ColorInput<'a> {
    /// A CSS color string (hex, `rgb()`/`rgba()` or a named color).
    Css(&'a str),
    /// A parsed color.
    Rgba(Rgba),
}

impl<'a> From<&'a str> for ColorInput<'a> {
    fn from(value: &'a str) -> Self {
        ColorInput::Css(value)
    }
}

impl<'a> From<&'a String> for ColorInput<'a> {
    fn from(value: &'a String) -> Self {
        ColorInput::Css(value)
    }
}

impl From<Rgba> for ColorInput<'_> {
    fn from(value: Rgba) -> Self {
        ColorInput::Rgba(value)
    }
}

/// Why [`contrast_ratio`] has no answer. Each displays as the message of the
/// `TypeError` upstream throws.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ContrastError {
    /// A color string [`parse_color`] can't read. `label` is `"foreground"`
    /// or `"background"`.
    Unparseable {
        /// Which argument it was: `"foreground"` or `"background"`.
        label: &'static str,
        /// The string as given.
        value: String,
    },
    /// The background is translucent, so the color it renders as is unknown
    /// here; composite it over its own backdrop first.
    TranslucentBackground,
}

impl fmt::Display for ContrastError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ContrastError::Unparseable { label, value } => {
                write!(f, "contrastRatio: could not parse {label} \"{value}\"")
            }
            ContrastError::TranslucentBackground => f.write_str(
                "contrastRatio: background must be opaque — composite it over its backdrop first",
            ),
        }
    }
}

impl std::error::Error for ContrastError {}

/// A [`ColorInput`] as a color, or the error for one that doesn't parse.
/// Upstream `resolve`.
fn resolve(value: ColorInput<'_>, label: &'static str) -> Result<Rgba, ContrastError> {
    match value {
        ColorInput::Rgba(color) => Ok(color),
        ColorInput::Css(css) => parse_color(css).ok_or_else(|| ContrastError::Unparseable {
            label,
            value: css.to_string(),
        }),
    }
}

/// The WCAG 2.x contrast ratio of a foreground on an opaque background, 1 to
/// 21 (`contrast_ratio("#000000", "#FFFFFF")` is 21). A translucent
/// foreground is composited over the background first; a translucent
/// background is an error, since what it renders as depends on its own
/// backdrop. The background is checked first, as upstream checks it.
/// Upstream `contrastRatio` (`theme/contrast.ts`).
pub fn contrast_ratio<'a, 'b>(
    foreground: impl Into<ColorInput<'a>>,
    background: impl Into<ColorInput<'b>>,
) -> Result<f64, ContrastError> {
    let bg = resolve(background.into(), "background")?;
    if bg.a < 1.0 {
        return Err(ContrastError::TranslucentBackground);
    }
    let mut fg = resolve(foreground.into(), "foreground")?;
    if fg.a < 1.0 {
        fg = composite_over(fg, bg);
    }
    let lum_a = relative_luminance(fg);
    let lum_b = relative_luminance(bg);
    let lighter = js_max(lum_a, lum_b);
    let darker = js_min(lum_a, lum_b);
    Ok((lighter + 0.05) / (darker + 0.05))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::authoring::color::format_color;

    fn rgba(r: f64, g: f64, b: f64, a: f64) -> Rgba {
        Rgba { r, g, b, a }
    }

    /// Jest's `toBeCloseTo(expected, digits)`.
    fn close(actual: f64, expected: f64, digits: i32) -> bool {
        (actual - expected).abs() < 10f64.powi(-digits) / 2.0
    }

    fn ratio(foreground: &str, background: &str) -> f64 {
        contrast_ratio(foreground, background).expect("colors parse")
    }

    // theme/contrast.test.ts: relativeLuminance

    #[test]
    fn relative_luminance_is_0_for_black_and_1_for_white() {
        assert_eq!(relative_luminance(rgba(0.0, 0.0, 0.0, 1.0)), 0.0);
        assert!(close(
            relative_luminance(rgba(255.0, 255.0, 255.0, 1.0)),
            1.0,
            10
        ));
    }

    #[test]
    fn relative_luminance_weights_channels_per_wcag() {
        assert!(close(
            relative_luminance(rgba(255.0, 0.0, 0.0, 1.0)),
            0.2126,
            4
        ));
        assert!(close(
            relative_luminance(rgba(0.0, 255.0, 0.0, 1.0)),
            0.7152,
            4
        ));
        assert!(close(
            relative_luminance(rgba(0.0, 0.0, 255.0, 1.0)),
            0.0722,
            4
        ));
    }

    // theme/contrast.test.ts: contrastRatio

    #[test]
    fn contrast_ratio_is_21_for_black_on_white_and_1_for_identical_colors() {
        assert!(close(ratio("#000000", "#FFFFFF"), 21.0, 5));
        assert_eq!(ratio("#3B82F6", "#3B82F6"), 1.0);
    }

    #[test]
    fn contrast_ratio_is_symmetric_for_opaque_colors() {
        assert!(close(
            ratio("#0064E0", "#FCFDFE"),
            ratio("#FCFDFE", "#0064E0"),
            10
        ));
    }

    #[test]
    fn contrast_ratio_matches_the_canonical_boundary_gray() {
        let boundary = ratio("#767676", "#FFFFFF");
        assert!(boundary >= 4.5);
        assert!(boundary < 4.6);
        // Upstream's value under Node 24.
        assert!(close(boundary, 4.542224959605253, 12));
    }

    #[test]
    fn contrast_ratio_composites_a_translucent_foreground_over_the_background() {
        // 50% black over white paints as mid-gray, not black.
        let composited = ratio("rgba(0, 0, 0, 0.5)", "#FFFFFF");
        assert!(composited < ratio("#000000", "#FFFFFF"));
        assert!(close(
            composited,
            ratio("rgb(127.5, 127.5, 127.5)", "#FFFFFF"),
            10
        ));
    }

    #[test]
    fn contrast_ratio_rejects_a_translucent_background() {
        let error = contrast_ratio("#000000", "#FFFFFF80").unwrap_err();
        assert_eq!(error, ContrastError::TranslucentBackground);
        assert!(error.to_string().contains("background must be opaque"));
    }

    #[test]
    fn contrast_ratio_rejects_unparseable_colors() {
        let foreground = contrast_ratio("var(--color-accent)", "#FFFFFF").unwrap_err();
        assert!(
            foreground
                .to_string()
                .contains("could not parse foreground")
        );
        let background = contrast_ratio("#000000", "oklch(0.5 0.1 200)").unwrap_err();
        assert!(
            background
                .to_string()
                .contains("could not parse background")
        );
        // Upstream's full messages under Node 24.
        assert_eq!(
            foreground.to_string(),
            "contrastRatio: could not parse foreground \"var(--color-accent)\""
        );
        assert_eq!(
            contrast_ratio("#000000", "#FFFFFF80")
                .unwrap_err()
                .to_string(),
            "contrastRatio: background must be opaque — composite it over its backdrop first"
        );
    }

    #[test]
    fn contrast_ratio_checks_the_background_before_the_foreground() {
        // Upstream resolves the background first, so its error wins.
        assert_eq!(
            contrast_ratio("var(--x)", "#FFFFFF80"),
            Err(ContrastError::TranslucentBackground)
        );
    }

    #[test]
    fn contrast_ratio_takes_parsed_colors() {
        let black = rgba(0.0, 0.0, 0.0, 1.0);
        assert_eq!(
            contrast_ratio(black, "#FFFFFF"),
            contrast_ratio("#000000", "#FFFFFF")
        );
        let white = String::from("#FFFFFF");
        assert_eq!(
            contrast_ratio(black, &white),
            contrast_ratio(black, "#FFFFFF")
        );
    }

    // theme/contrast.test.ts: compositeOver

    #[test]
    fn composite_over_returns_the_foreground_when_opaque_and_the_backdrop_at_alpha_0() {
        let fg = parse_color("#123456").expect("fixture parses");
        let bg = parse_color("#FFFFFF").expect("fixture parses");
        assert_eq!(composite_over(fg, bg), Rgba { a: 1.0, ..fg });
        assert_eq!(
            composite_over(Rgba { a: 0.0, ..fg }, bg),
            Rgba { a: 1.0, ..bg }
        );
    }

    #[test]
    fn composite_over_blends_in_gamma_encoded_srgb_like_css() {
        let fg = parse_color("rgba(0, 0, 0, 0.5)").expect("fixture parses");
        let bg = parse_color("#FFFFFF").expect("fixture parses");
        let out = composite_over(fg, bg);
        assert!(close(out.r, 127.5, 5));
        assert!(close(out.g, 127.5, 5));
        assert!(close(out.b, 127.5, 5));
        assert_eq!(out.a, 1.0);
    }

    // theme/contrast.test.ts: the stock theme checks. Upstream resolves Core's
    // default tokens with `resolveThemeToken`; these are the light and dark
    // sides of their `light-dark()` values in upstream `tokens.stylex.ts`
    // (`colorDefaults`).

    struct Mode {
        name: &'static str,
        accent: &'static str,
        on_accent: &'static str,
        neutral: &'static str,
        surface: &'static str,
        card: &'static str,
        body: &'static str,
        text_secondary: &'static str,
        tints: [&'static str; 10],
    }

    const MODES: [Mode; 2] = [
        Mode {
            name: "light",
            accent: "#0064E0",
            on_accent: "#FFFFFF",
            neutral: "rgba(5, 54, 89, 0.1)",
            surface: "#FFFFFF",
            card: "#FFFFFF",
            body: "#F1F4F7",
            text_secondary: "#445564",
            tints: [
                "#0171E333",
                "#03A7D733",
                "#0A131733",
                "#24BB5E33",
                "#F2790233",
                "#E638B333",
                "#7952FF33",
                "#E3193B33",
                "#0DB7AF33",
                "#E2A40033",
            ],
        },
        Mode {
            name: "dark",
            accent: "#0171E3",
            on_accent: "#FFFFFF",
            neutral: "rgba(223, 226, 229, 0.2)",
            surface: "#1F1F22",
            card: "#1F1F22",
            body: "#111112",
            text_secondary: "#B0B5BB",
            tints: [
                "#0171E333",
                "#03A7D733",
                "#666A724C",
                "#24BB5E33",
                "#F2790233",
                "#E638B333",
                "#7952FF33",
                "#E3193B33",
                "#0DB7AF33",
                "#E2A40033",
            ],
        },
    ];

    #[test]
    fn the_stock_accent_pair_meets_4_5_to_1() {
        // --color-on-accent labels every --color-accent fill (WCAG 1.4.3).
        for mode in &MODES {
            assert!(ratio(mode.on_accent, mode.accent) >= 4.5, "{}", mode.name);
        }
    }

    #[test]
    fn secondary_text_on_the_neutral_fill_meets_4_5_to_1() {
        // The translucent --color-neutral fill lifts the page under it, so
        // the text is measured on the composited fill.
        for mode in &MODES {
            for page in [mode.surface, mode.card, mode.body] {
                let fill = parse_color(mode.neutral).expect("neutral parses");
                let backdrop = parse_color(page).expect("page parses");
                let painted = format_color(composite_over(fill, backdrop));
                assert!(
                    ratio(mode.text_secondary, &painted) >= 4.5,
                    "{} over {page}",
                    mode.name
                );
            }
        }
    }

    #[test]
    fn secondary_text_on_the_stock_palette_tints_meets_4_5_to_1() {
        for mode in &MODES {
            for page in [mode.surface, mode.body] {
                let backdrop = parse_color(page).expect("page parses");
                for tint in mode.tints {
                    let fill = parse_color(tint).expect("tint parses");
                    let painted = format_color(composite_over(fill, backdrop));
                    assert!(
                        ratio(mode.text_secondary, &painted) >= 4.5,
                        "{tint} over {page} ({})",
                        mode.name
                    );
                }
            }
        }
    }
}
