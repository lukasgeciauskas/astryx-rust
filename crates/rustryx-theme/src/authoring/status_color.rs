//! Status color math for theme authoring: upstream `theme/statusColor.ts`.
//!
//! The pure color math behind status-role derivation (upstream
//! architecture:theme-compilation INV17) and the status contrast checks.
//! Contrast is WCAG 2.x relative luminance. Lightness adjustments move a
//! color along OKLCH lightness at its own hue, cutting chroma only as far as
//! sRGB needs, so a derived role keeps its status hue.

use super::color::{
    Rgba, format_hex, linear_rgb_to_oklab, oklab_to_linear_rgb, parse_color, srgb_to_linear,
};
use super::js::{js_hypot, js_max, js_min, js_round, js_to_hex_string};

/// An OKLCH color with the hue in radians (as `Math.atan2` returns it, -π to
/// π). Upstream `OKLCH`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Oklch {
    /// Lightness, 0 to 1.
    pub l: f64,
    /// Chroma, 0 to about 0.4.
    pub c: f64,
    /// Hue in radians.
    pub h: f64,
}

/// A linear-light channel (0 to 1) as an sRGB channel (0 to 255), neither
/// rounded nor clamped. Upstream `fromLinear`.
fn from_linear(v: f64) -> f64 {
    let c = if v <= 0.0031308 {
        12.92 * v
    } else {
        1.055 * v.powf(1.0 / 2.4) - 0.055
    };
    c * 255.0
}

/// A possibly translucent color painted over an opaque backdrop
/// (source-over in gamma-encoded sRGB); the result is opaque. Upstream
/// `composite`.
pub fn composite(top: Rgba, under: Rgba) -> Rgba {
    let a = top.a;
    Rgba {
        r: top.r * a + under.r * (1.0 - a),
        g: top.g * a + under.g * (1.0 - a),
        b: top.b * a + under.b * (1.0 - a),
        a: 1.0,
    }
}

/// The WCAG relative luminance of an opaque color, 0 (black) to 1 (white).
/// Upstream `relativeLuminance` (`theme/statusColor.ts`).
pub fn relative_luminance(color: Rgba) -> f64 {
    let Rgba { r, g, b, .. } = color;
    0.2126 * srgb_to_linear(r) + 0.7152 * srgb_to_linear(g) + 0.0722 * srgb_to_linear(b)
}

/// The WCAG contrast ratio of a foreground on a background, 1 to 21. A
/// translucent background is first composited over white and a translucent
/// foreground over the background, so this never fails, unlike
/// [`contrast::contrast_ratio`](super::contrast::contrast_ratio). Upstream
/// `contrastRatio` (`theme/statusColor.ts`).
pub fn contrast_ratio(foreground: Rgba, background: Rgba) -> f64 {
    let bg = if background.a < 1.0 {
        composite(
            background,
            Rgba {
                r: 255.0,
                g: 255.0,
                b: 255.0,
                a: 1.0,
            },
        )
    } else {
        background
    };
    let fg = if foreground.a < 1.0 {
        composite(foreground, bg)
    } else {
        foreground
    };
    let a = relative_luminance(fg);
    let b = relative_luminance(bg);
    (js_max(a, b) + 0.05) / (js_min(a, b) + 0.05)
}

/// An opaque sRGB color as OKLCH, hue in radians. Alpha is ignored. Upstream
/// `toOklch`.
pub fn to_oklch(color: Rgba) -> Oklch {
    // Upstream repeats `linearRgbToOklab`'s math here, term for term.
    let [l, a, b] = linear_rgb_to_oklab(
        srgb_to_linear(color.r),
        srgb_to_linear(color.g),
        srgb_to_linear(color.b),
    );
    Oklch {
        l,
        c: js_hypot(a, b),
        h: b.atan2(a),
    }
}

/// An OKLCH color as linear-light red, green and blue, unclamped. Upstream
/// `oklchToLinear`.
fn oklch_to_linear(color: Oklch) -> [f64; 3] {
    let Oklch { l, c, h } = color;
    oklab_to_linear_rgb(l, c * h.cos(), c * h.sin())
}

/// An OKLCH color as opaque sRGB with fractional channels. Outside sRGB, the
/// chroma shrinks by a tenth at a time, at the same lightness and hue, until
/// every linear channel is within 1e-4 of 0–1 (40 tries, then grey).
/// Upstream `fromOklch`.
pub fn from_oklch(color: Oklch) -> Rgba {
    let to_rgba = |linear: [f64; 3]| {
        let [r, g, b] = linear.map(|v| from_linear(js_min(1.0, js_max(0.0, v))));
        Rgba { r, g, b, a: 1.0 }
    };
    let mut c = color.c;
    for _ in 0..40 {
        let rgb = oklch_to_linear(Oklch { c, ..color });
        if rgb.iter().all(|v| (-1e-4..=1.0 + 1e-4).contains(v)) {
            return to_rgba(rgb);
        }
        c *= 0.9;
    }
    to_rgba(oklch_to_linear(Oklch { c: 0.0, ..color }))
}

/// The lowest OKLCH chroma a status color may have and still read as a hue.
/// Upstream `STATUS_CHROMA_FLOOR`.
pub const STATUS_CHROMA_FLOOR: f64 = 0.03;

/// The OKLCH lightness band a status color must sit in to read as a hue
/// rather than near-black or near-white. Upstream `STATUS_LIGHTNESS_RANGE`.
pub const STATUS_LIGHTNESS_RANGE: [f64; 2] = [0.2, 0.95];

/// Whether a color reads as a hue rather than a neutral grey, near-black or
/// near-white (upstream architecture:theme-tokens INV11): chroma at least
/// [`STATUS_CHROMA_FLOOR`] and lightness within [`STATUS_LIGHTNESS_RANGE`].
/// A translucent color is judged on its own channels. Upstream
/// `isChromatic`.
pub fn is_chromatic(color: Rgba) -> bool {
    let Oklch { l, c, .. } = to_oklch(color);
    let [min, max] = STATUS_LIGHTNESS_RANGE;
    c >= STATUS_CHROMA_FLOOR && (min..=max).contains(&l)
}

/// `color`, made opaque and moved along OKLCH lightness at its own hue until
/// it reaches `ratio` against every backdrop. It moves darker when the
/// backdrops' mean luminance is above 0.18 and lighter otherwise, and stops at
/// the passing lightness closest to the original (32 rounds of bisection).
/// The color comes back unchanged (but opaque) when it already passes, and
/// `None` when no lightness at its hue reaches the ratio. Upstream
/// `solveLightness`.
pub fn solve_lightness(color: Rgba, backdrops: &[Rgba], ratio: f64) -> Option<Rgba> {
    let passes = |candidate: Rgba| {
        backdrops
            .iter()
            .all(|backdrop| contrast_ratio(candidate, *backdrop) >= ratio)
    };
    let opaque = Rgba { a: 1.0, ..color };
    if passes(opaque) {
        return Some(opaque);
    }
    let mean_luminance = backdrops
        .iter()
        .fold(0.0, |sum, backdrop| sum + relative_luminance(*backdrop))
        / backdrops.len() as f64;
    let base = to_oklch(opaque);
    // Light backdrops need a darker color; dark backdrops a lighter one.
    let darker = mean_luminance > 0.18;
    let (mut lo, mut hi) = if darker { (0.0, base.l) } else { (base.l, 1.0) };
    let at = |l: f64| from_oklch(Oklch { l, ..base });
    let target = if darker { at(0.0) } else { at(1.0) };
    if !passes(target) {
        return None;
    }
    // Binary search for the passing lightness closest to the original.
    for _ in 0..32 {
        let mid = (lo + hi) / 2.0;
        let ok = passes(at(mid));
        if darker {
            if ok {
                lo = mid;
            } else {
                hi = mid;
            }
        } else if ok {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    let solved = at(if darker { lo } else { hi });
    Some(if passes(solved) { solved } else { target })
}

/// A concrete color, or `None` for a missing value or one [`parse_color`]
/// can't read (a `var()` reference, say). Upstream `parseConcreteColor`.
pub fn parse_concrete_color(value: Option<&str>) -> Option<Rgba> {
    value.and_then(parse_color)
}

/// A color as an uppercase `#RRGGBB`, or `#RRGGBBAA` when translucent.
/// Upstream `toHex`.
pub fn to_hex(color: Rgba) -> String {
    let hex = format_hex(color.r, color.g, color.b);
    if color.a >= 1.0 {
        return hex;
    }
    let alpha = format!("{:0>2}", js_to_hex_string(js_round(color.a * 255.0))).to_uppercase();
    format!("{hex}{alpha}")
}

#[cfg(test)]
mod tests {
    use super::*;

    // upstream `statusColor.ts` has no tests of its own. Expected values come
    // from running it under Node 24.

    fn rgba(r: f64, g: f64, b: f64, a: f64) -> Rgba {
        Rgba { r, g, b, a }
    }

    const WHITE: Rgba = Rgba {
        r: 255.0,
        g: 255.0,
        b: 255.0,
        a: 1.0,
    };

    /// Agreement with upstream's value to 12 significant digits: Rust and V8
    /// use different `cbrt`, `pow`, `sin`/`cos` and `atan2`, which can
    /// differ in the last bits.
    fn approx(actual: f64, expected: f64) -> bool {
        (actual - expected).abs() <= 1e-12 * expected.abs().max(1.0)
    }

    /// Agreement to 1e-9 of a channel, for outputs of a bisection, where a
    /// last-bit difference can move the final step.
    fn assert_channels(actual: Rgba, expected: [f64; 3]) {
        let got = [actual.r, actual.g, actual.b];
        for (a, e) in got.into_iter().zip(expected) {
            assert!((a - e).abs() < 1e-9, "{actual:?} != {expected:?}");
        }
        assert_eq!(actual.a, 1.0);
    }

    #[test]
    fn composite_paints_a_translucent_color_over_its_backdrop() {
        assert_eq!(
            composite(rgba(0.0, 0.0, 0.0, 0.25), WHITE),
            rgba(191.25, 191.25, 191.25, 1.0)
        );
    }

    #[test]
    fn relative_luminance_matches_upstream() {
        assert!(approx(
            relative_luminance(rgba(0.0, 100.0, 224.0, 1.0)),
            0.144961612976391
        ));
        assert_eq!(relative_luminance(rgba(0.0, 0.0, 0.0, 1.0)), 0.0);
    }

    #[test]
    fn contrast_ratio_composites_a_translucent_background_over_white() {
        let black = rgba(0.0, 0.0, 0.0, 1.0);
        assert!(approx(
            contrast_ratio(black, rgba(0.0, 0.0, 0.0, 0.5)),
            5.280822809644651
        ));
    }

    #[test]
    fn contrast_ratio_composites_a_translucent_foreground_over_the_background() {
        assert!(approx(
            contrast_ratio(rgba(0.0, 0.0, 0.0, 0.5), WHITE),
            3.976653024912438
        ));
    }

    #[test]
    fn to_oklch_measures_hue_in_radians() {
        let blue = to_oklch(rgba(0.0, 100.0, 224.0, 1.0));
        assert!(approx(blue.l, 0.5334360733851473), "{blue:?}");
        assert!(approx(blue.c, 0.20490477709448957), "{blue:?}");
        assert!(approx(blue.h, -1.7660972560881507), "{blue:?}");
        let gray = to_oklch(rgba(128.0, 128.0, 128.0, 1.0));
        assert!(approx(gray.l, 0.5998708017071177), "{gray:?}");
        assert!(gray.c < 1e-7, "{gray:?}");
    }

    #[test]
    fn from_oklch_round_trips_an_srgb_color() {
        let back = from_oklch(to_oklch(rgba(0.0, 100.0, 224.0, 1.0)));
        assert_channels(
            back,
            [
                0.00002469212713567814,
                100.00000166790028,
                223.99998861528368,
            ],
        );
    }

    #[test]
    fn from_oklch_reduces_chroma_until_the_color_fits() {
        assert_channels(
            from_oklch(Oklch {
                l: 0.7,
                c: 0.4,
                h: 2.5,
            }),
            [45.98525748369823, 189.48572044699236, 50.704075661132016],
        );
        assert_channels(
            from_oklch(Oklch {
                l: 0.5,
                c: 5.0,
                h: 1.0,
            }),
            [145.9036938209752, 79.87174463237943, 17.48992413903691],
        );
    }

    #[test]
    fn is_chromatic_tells_hues_from_greys_and_extremes() {
        assert!(is_chromatic(rgba(0.0, 100.0, 224.0, 1.0)));
        assert!(!is_chromatic(rgba(128.0, 128.0, 128.0, 1.0)));
        assert!(!is_chromatic(rgba(0.0, 0.0, 0.0, 1.0)));
        assert!(!is_chromatic(rgba(255.0, 250.0, 250.0, 1.0)));
        // A translucent color is judged on its own channels.
        assert!(is_chromatic(rgba(0.0, 100.0, 224.0, 0.1)));
        assert_eq!(STATUS_CHROMA_FLOOR, 0.03);
        assert_eq!(STATUS_LIGHTNESS_RANGE, [0.2, 0.95]);
    }

    #[test]
    fn solve_lightness_returns_a_passing_color_unchanged() {
        let red = rgba(227.0, 25.0, 59.0, 1.0);
        assert_eq!(solve_lightness(red, &[WHITE], 4.5), Some(red));
    }

    #[test]
    fn solve_lightness_lightens_on_a_dark_backdrop() {
        let red = rgba(227.0, 25.0, 59.0, 1.0);
        let dark = rgba(31.0, 31.0, 34.0, 1.0);
        let solved = solve_lightness(red, &[dark], 4.5).expect("reachable");
        assert_channels(
            solved,
            [249.7104202426877, 57.059057709440054, 77.14286094123854],
        );
        assert_eq!(to_hex(solved), "#FA394D");
        assert!(contrast_ratio(solved, dark) >= 4.5);
    }

    #[test]
    fn solve_lightness_darkens_against_every_light_backdrop() {
        let green = rgba(36.0, 187.0, 94.0, 1.0);
        let body = rgba(241.0, 244.0, 247.0, 1.0);
        let solved = solve_lightness(green, &[WHITE, body], 4.5).expect("reachable");
        assert_channels(
            solved,
            [30.583508418040278, 128.12093179913063, 65.0677107706055],
        );
    }

    #[test]
    fn solve_lightness_makes_the_color_opaque() {
        let translucent = rgba(0.0, 0.0, 0.0, 0.5);
        assert_eq!(
            solve_lightness(translucent, &[WHITE], 4.5),
            Some(rgba(0.0, 0.0, 0.0, 1.0))
        );
        // With no backdrops every candidate passes.
        assert_eq!(
            solve_lightness(rgba(1.0, 2.0, 3.0, 0.5), &[], 4.5),
            Some(rgba(1.0, 2.0, 3.0, 1.0))
        );
    }

    #[test]
    fn solve_lightness_returns_none_when_no_lightness_reaches_the_ratio() {
        let red = rgba(227.0, 25.0, 59.0, 1.0);
        let gray = rgba(128.0, 128.0, 128.0, 1.0);
        assert_eq!(solve_lightness(red, &[gray], 7.0), None);
    }

    #[test]
    fn parse_concrete_color_returns_none_for_missing_or_unresolved_values() {
        assert_eq!(parse_concrete_color(None), None);
        assert_eq!(parse_concrete_color(Some("var(--x)")), None);
        assert_eq!(
            parse_concrete_color(Some("#0064E0")),
            Some(rgba(0.0, 100.0, 224.0, 1.0))
        );
    }

    #[test]
    fn to_hex_appends_alpha_only_when_translucent() {
        assert_eq!(to_hex(rgba(0.0, 100.0, 224.0, 1.0)), "#0064E0");
        assert_eq!(to_hex(rgba(0.0, 100.0, 224.0, 0.5)), "#0064E080");
        assert_eq!(to_hex(rgba(0.4, 100.5, 224.0, 0.2)), "#0065E033");
    }
}
