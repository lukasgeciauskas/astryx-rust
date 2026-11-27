//! One token value per color mode, and the CSS color functions the theme
//! resolver evaluates: upstream `theme/tokenValues.ts`.
//!
//! A token's default is one CSS value or a light and dark pair, written as
//! `light-dark(light, dark)` or as a `[light, dark]` tuple, and
//! [`resolve_xds_token_value`] picks the side for a mode. Generated themes write
//! derived tokens as `var()` references and `color-mix()` expressions, so a
//! scoped override of a base token re-themes everything derived from it. The
//! CSS cascade resolves those; a consumer that is not CSS needs the concrete
//! value, so [`resolve_default_tokens`] follows the references (through
//! [`resolve_token_references_with`]) and [`evaluate_color_functions`] evaluates
//! the `srgb` mixes the theme generator emits, as upstream's resolver does.

use super::TOKEN_DEFAULTS;
use super::color::{Rgba, format_color, parse_color};
use super::js::{is_js_whitespace, js_parse_float, js_trim};
use super::token_references::{
    find_matching_paren, resolve_token_references_with, split_top_level_comma,
};
use crate::types::Appearance;

/// The color mode a token value is resolved in: the side of a light and dark
/// pair that is used. Upstream `ResolvedThemeMode`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ResolvedThemeMode {
    /// Upstream `'light'`.
    #[default]
    Light,
    /// Upstream `'dark'`.
    Dark,
}

impl From<Appearance> for ResolvedThemeMode {
    fn from(appearance: Appearance) -> Self {
        match appearance {
            Appearance::Light => ResolvedThemeMode::Light,
            Appearance::Dark => ResolvedThemeMode::Dark,
        }
    }
}

impl From<ResolvedThemeMode> for Appearance {
    fn from(mode: ResolvedThemeMode) -> Self {
        match mode {
            ResolvedThemeMode::Light => Appearance::Light,
            ResolvedThemeMode::Dark => Appearance::Dark,
        }
    }
}

/// A token value as a theme definition writes it: one CSS value, which may be
/// a `light-dark(light, dark)` pair, or a `[light, dark]` tuple. Upstream
/// `TokenValue` (`theme/defineTheme.ts`), `string | [light, dark]`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThemeTokenValue<'a> {
    /// One CSS value for both modes, or a `light-dark()` pair.
    Css(&'a str),
    /// A `[light, dark]` tuple.
    Pair(&'a str, &'a str),
}

impl<'a> From<&'a str> for ThemeTokenValue<'a> {
    fn from(value: &'a str) -> Self {
        ThemeTokenValue::Css(value)
    }
}

impl<'a> From<&'a String> for ThemeTokenValue<'a> {
    fn from(value: &'a String) -> Self {
        ThemeTokenValue::Css(value)
    }
}

impl<'a> From<[&'a str; 2]> for ThemeTokenValue<'a> {
    fn from([light, dark]: [&'a str; 2]) -> Self {
        ThemeTokenValue::Pair(light, dark)
    }
}

impl<'a> From<(&'a str, &'a str)> for ThemeTokenValue<'a> {
    fn from((light, dark): (&'a str, &'a str)) -> Self {
        ThemeTokenValue::Pair(light, dark)
    }
}

/// The two arguments of a CSS `light-dark(light, dark)`, each trimmed, or
/// `None` when the value (trimmed) is not one. The arguments split on the first
/// top-level comma, so a nested `rgba()` stays whole. Upstream
/// `parseLightDark`.
fn parse_light_dark(value: &str) -> Option<(&str, &str)> {
    let trimmed = js_trim(value);
    let inner = trimmed.strip_prefix("light-dark(")?.strip_suffix(')')?;
    split_top_level_comma(inner)
}

/// A token value in one mode (upstream `resolveXDSTokenValue`):
///
/// - a `[light, dark]` tuple gives the side for `mode`;
/// - a `light-dark(light, dark)` string gives that side, trimmed (nested CSS
///   values such as `rgba(…)` stay whole);
/// - any other string is returned unchanged.
///
/// Only a whole-value `light-dark()` is split; one nested inside another value
/// stays, as upstream leaves it for the CSS reader.
pub fn resolve_xds_token_value<'a>(
    value: impl Into<ThemeTokenValue<'a>>,
    mode: ResolvedThemeMode,
) -> &'a str {
    let pick = |light: &'a str, dark: &'a str| match mode {
        ResolvedThemeMode::Light => light,
        ResolvedThemeMode::Dark => dark,
    };
    match value.into() {
        ThemeTokenValue::Pair(light, dark) => pick(light, dark),
        ThemeTokenValue::Css(css) => match parse_light_dark(css) {
            Some((light, dark)) => pick(light, dark),
            None => css,
        },
    }
}

// =============================================================================
// Reference resolution: token expressions to concrete values
// =============================================================================

/// One `color-mix()` argument: a color and its percentage, if it has one.
struct MixComponent<'a> {
    color: &'a str,
    percentage: Option<f64>,
}

/// A `color-mix()` argument split into its color and optional trailing
/// percentage (`red 20%`), as upstream's `/\s+([\d.]+)%$/` splits it: the
/// percentage is the digits and dots after the last whitespace, read with
/// `parseFloat` (so `.%` reads as NaN). Upstream `parseMixComponent`, whose
/// `null` result never happens.
fn parse_mix_component(part: &str) -> MixComponent<'_> {
    let trimmed = js_trim(part);
    if let Some(body) = trimmed.strip_suffix('%') {
        let digits_at = body
            .trim_end_matches(|c: char| c.is_ascii_digit() || c == '.')
            .len();
        let space_at = body[..digits_at].trim_end_matches(is_js_whitespace).len();
        if digits_at < body.len() && space_at < digits_at {
            return MixComponent {
                color: js_trim(&trimmed[..space_at]),
                percentage: Some(js_parse_float(&body[digits_at..])),
            };
        }
    }
    MixComponent {
        color: trimmed,
        percentage: None,
    }
}

/// The body of `color-mix(in <space>, c1 [p1], c2 [p2])` as a concrete color
/// (upstream `evaluateColorMix`).
///
/// Only the `srgb` space, the one the theme generator emits, is evaluated,
/// with the CSS Color 5 algorithm: omitted percentages filled in and the two
/// normalized to sum to 100%, the channels interpolated premultiplied by alpha,
/// and the alpha scaled down when the percentages sum to less than 100%. The
/// colors are the ones [`parse_color`] reads (hex, `rgb()`/`rgba()`,
/// `transparent`, `black`, `white`). `None` for anything else, so the caller
/// keeps the expression rather than guess.
fn evaluate_color_mix(body: &str) -> Option<String> {
    // /^in\s+([\w-]+)\s*,\s*(.+)$/s
    let after_in = body.strip_prefix("in")?;
    let space_at = after_in.trim_start_matches(is_js_whitespace);
    if space_at.len() == after_in.len() {
        return None;
    }
    let space_len = space_at
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '-'))
        .unwrap_or(space_at.len());
    if space_len == 0 {
        return None;
    }
    let color_space = &space_at[..space_len];
    let after_comma = space_at[space_len..]
        .trim_start_matches(is_js_whitespace)
        .strip_prefix(',')?;
    // Whitespace the regex gives back to `(.+)` holds no comma, so a rest of
    // only whitespace fails the split below either way.
    let rest = after_comma.trim_start_matches(is_js_whitespace);
    if rest.is_empty() || !color_space.eq_ignore_ascii_case("srgb") {
        return None;
    }

    let (first, second) = split_top_level_comma(rest)?;
    let first = parse_mix_component(first);
    let second = parse_mix_component(second);
    let c1 = parse_color(first.color)?;
    let c2 = parse_color(second.color)?;

    // Fill in omitted percentages, then normalize so they sum to 100%.
    let (p1, p2) = match (first.percentage, second.percentage) {
        (Some(p1), Some(p2)) => (p1, p2),
        (Some(p1), None) => (p1, 100.0 - p1),
        (None, Some(p2)) => (100.0 - p2, p2),
        (None, None) => (50.0, 50.0),
    };
    let sum = p1 + p2;
    if sum <= 0.0 {
        return None;
    }
    let w1 = p1 / sum;
    let w2 = p2 / sum;
    let alpha_multiplier = if sum < 100.0 { sum / 100.0 } else { 1.0 };

    // Interpolate in premultiplied sRGB, then un-premultiply.
    let mixed_a = w1 * c1.a + w2 * c2.a;
    let premix = |k1: f64, k2: f64| w1 * k1 * c1.a + w2 * k2 * c2.a;
    let rp = premix(c1.r, c2.r);
    let gp = premix(c1.g, c2.g);
    let bp = premix(c1.b, c2.b);
    let (r, g, b) = if mixed_a == 0.0 {
        (0.0, 0.0, 0.0)
    } else {
        (rp / mixed_a, gp / mixed_a, bp / mixed_a)
    };

    Some(format_color(Rgba {
        r,
        g,
        b,
        a: mixed_a * alpha_multiplier,
    }))
}

/// `expr` with every `color-mix()` it can evaluate replaced by its color,
/// innermost first, and the rest kept as written (upstream
/// `evaluateColorFunctions`). A mix in another space, or of colors that are not
/// concrete, stays a `color-mix()` expression, with any mix nested in it
/// evaluated.
///
/// `color-mix(in srgb, #0058D2 20%, transparent)` is `rgba(0, 88, 210, 0.2)`;
/// `color-mix(in srgb, #000000 25%, #FFFFFF)` is `#BFBFBF`.
pub fn evaluate_color_functions(expr: &str) -> String {
    let Some(idx) = expr.find("color-mix(") else {
        return expr.to_string();
    };
    let open_index = idx + "color-mix".len();
    let Some(close_index) = find_matching_paren(expr, open_index) else {
        return expr.to_string();
    };
    let body = evaluate_color_functions(&expr[open_index + 1..close_index]);
    let replacement = evaluate_color_mix(&body).unwrap_or_else(|| format!("color-mix({body})"));
    format!(
        "{}{replacement}{}",
        &expr[..idx],
        evaluate_color_functions(&expr[close_index + 1..])
    )
}

/// Every Astryx token's default in `mode`, references followed and color
/// functions evaluated (upstream `resolveDefaultTokens`): each
/// [`TOKEN_DEFAULTS`] value resolved with [`resolve_xds_token_value`], then the
/// map resolved with [`resolve_token_references_with`] and
/// [`evaluate_color_functions`]. The pairs are in upstream's key order, a token
/// another references first; `--color-text-primary` is `#0A1317` in light mode.
pub fn resolve_default_tokens(mode: ResolvedThemeMode) -> Vec<(String, String)> {
    let resolved: Vec<(&str, &str)> = TOKEN_DEFAULTS
        .iter()
        .map(|&(name, value)| (name, resolve_xds_token_value(value, mode)))
        .collect();
    resolve_token_references_with(&resolved, evaluate_color_functions)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Tokens;
    use crate::authoring::css_values::{
        TypedContext, substitute_vars, to_points, token_kind, typed, value_for_mode,
    };
    use crate::generated::default_theme;
    use crate::types::{AbsoluteLength, Hsla, TokenValue};

    const LIGHT: ResolvedThemeMode = ResolvedThemeMode::Light;
    const DARK: ResolvedThemeMode = ResolvedThemeMode::Dark;

    fn get<'a>(tokens: &'a [(String, String)], name: &str) -> Option<&'a str> {
        tokens
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }

    // Expected values from reading resolveXDSTokenValue.
    #[test]
    fn a_tuple_gives_the_side_for_the_mode() {
        assert_eq!(
            resolve_xds_token_value(["#AA0000", "#FF5555"], LIGHT),
            "#AA0000"
        );
        assert_eq!(
            resolve_xds_token_value(("#AA0000", "#FF5555"), DARK),
            "#FF5555"
        );
    }

    // Upstream tokens.test.ts: "resolves built theme light-dark() strings
    // without __inputTokens" and "resolves tuple overrides with nested comma
    // values".
    #[test]
    fn a_light_dark_string_gives_its_side_with_nested_commas_whole() {
        let value = "light-dark(rgba(5, 54, 89, 0.1), rgba(223, 226, 229, 0.2))";
        assert_eq!(
            resolve_xds_token_value(value, LIGHT),
            "rgba(5, 54, 89, 0.1)"
        );
        assert_eq!(
            resolve_xds_token_value(value, DARK),
            "rgba(223, 226, 229, 0.2)"
        );
        assert_eq!(
            resolve_xds_token_value(["rgba(5, 54, 89, 0.1)", "rgba(223, 226, 229, 0.2)"], DARK),
            "rgba(223, 226, 229, 0.2)"
        );
    }

    // Expected values from reading parseLightDark: the value is trimmed, the
    // halves are trimmed, the split is at the first top-level comma, and a
    // value that is not one whole light-dark() is returned unchanged.
    #[test]
    fn only_a_whole_light_dark_value_splits() {
        assert_eq!(
            resolve_xds_token_value("  light-dark( a ,  b )  ", DARK),
            "b"
        );
        assert_eq!(resolve_xds_token_value("light-dark(a, b, c)", DARK), "b, c");
        assert_eq!(
            resolve_xds_token_value("light-dark(a)", DARK),
            "light-dark(a)"
        );
        assert_eq!(resolve_xds_token_value("20px", DARK), "20px");
        assert_eq!(
            resolve_xds_token_value("0 1px light-dark(red, blue)", DARK),
            "0 1px light-dark(red, blue)"
        );
        assert_eq!(
            resolve_xds_token_value("LIGHT-DARK(a, b)", DARK),
            "LIGHT-DARK(a, b)"
        );
    }

    #[test]
    fn modes_convert_to_and_from_appearances() {
        assert_eq!(ResolvedThemeMode::from(Appearance::Dark), DARK);
        assert_eq!(Appearance::from(LIGHT), Appearance::Light);
    }

    // Upstream tokens.test.ts, "resolveThemeTokens — CSS color functions".
    #[test]
    fn srgb_mixes_evaluate_to_concrete_colors() {
        let cases = [
            (
                "color-mix(in srgb, #0058D2 20%, transparent)",
                "rgba(0, 88, 210, 0.2)",
            ),
            (
                "color-mix(in srgb, #BBC2FF 20%, transparent)",
                "rgba(187, 194, 255, 0.2)",
            ),
            (
                "color-mix(in srgb, #FF0000 20%, transparent)",
                "rgba(255, 0, 0, 0.2)",
            ),
            (
                "color-mix(in srgb, #00FF00 25%, transparent)",
                "rgba(0, 255, 0, 0.25)",
            ),
            ("color-mix(in srgb, #000000, #FFFFFF)", "#808080"),
            ("color-mix(in srgb, #000000 25%, #FFFFFF)", "#BFBFBF"),
        ];
        for (expr, expected) in cases {
            assert_eq!(evaluate_color_functions(expr), expected, "{expr}");
        }
    }

    // Upstream tokens.test.ts: "preserves an unsupported color space rather
    // than guessing".
    #[test]
    fn a_mix_in_another_space_stays_an_expression() {
        let expr = "color-mix(in oklab, #0058D2, black 15%)";
        assert_eq!(evaluate_color_functions(expr), expr);
    }

    // Expected values from reading evaluateColorMix and evaluateColorFunctions:
    // the inner mix evaluates first (#FF0000 and #0000FF at 50% are #800080),
    // text around a mix stays, a mix of a value parseColor cannot read stays
    // with its inner mixes evaluated, and percentages summing under 100% scale
    // the alpha (40% of an opaque mix is 0.4).
    #[test]
    fn mixes_evaluate_innermost_first_and_keep_what_they_cannot_read() {
        assert_eq!(
            evaluate_color_functions(
                "color-mix(in srgb, color-mix(in srgb, #FF0000, #0000FF) 50%, white)"
            ),
            "#C080C0"
        );
        assert_eq!(
            evaluate_color_functions("0 1px color-mix(in srgb, #000000, #FFFFFF) inset"),
            "0 1px #808080 inset"
        );
        assert_eq!(
            evaluate_color_functions(
                "color-mix(in srgb, var(--x), color-mix(in srgb, #000000, #FFFFFF))"
            ),
            "color-mix(in srgb, var(--x), #808080)"
        );
        assert_eq!(
            evaluate_color_functions("color-mix(in srgb, #000000 20%, #FFFFFF 20%)"),
            "rgba(128, 128, 128, 0.4)"
        );
        assert_eq!(
            evaluate_color_functions("color-mix(in SRGB, #000000 0%, #FFFFFF 0%)"),
            "color-mix(in SRGB, #000000 0%, #FFFFFF 0%)"
        );
        assert_eq!(
            evaluate_color_functions("color-mix(in srgb, #000"),
            "color-mix(in srgb, #000"
        );
    }

    // Upstream tokens.test.ts: "evaluates color-mix with transparent to an
    // rgba() value", through the reference resolver as resolveThemeTokens runs it.
    #[test]
    fn references_resolve_before_the_mix_evaluates() {
        let resolved = resolve_token_references_with(
            &[
                ("--color-accent", "#0058D2"),
                (
                    "--color-accent-muted",
                    "color-mix(in srgb, var(--color-accent) 20%, transparent)",
                ),
            ],
            evaluate_color_functions,
        );
        assert_eq!(
            get(&resolved, "--color-accent-muted"),
            Some("rgba(0, 88, 210, 0.2)")
        );
    }

    // Upstream tokens.test.ts: "resolves defaults when no theme is provided",
    // "resolves the exit curve to its accelerating default" and "focus outline
    // tokens default to the accent ring".
    #[test]
    fn defaults_resolve_to_concrete_values() {
        let tokens = resolve_default_tokens(LIGHT);
        assert_eq!(get(&tokens, "--color-text-primary"), Some("#0A1317"));
        assert_eq!(get(&tokens, "--spacing-1"), Some("0.25rem"));
        assert_eq!(get(&tokens, "--border-width"), Some("1px"));
        assert_eq!(
            get(&tokens, "--ease-exit"),
            Some("cubic-bezier(0.3, 0, 0.6, 0.6)")
        );
        assert_eq!(get(&tokens, "--focus-outline-width"), Some("2px"));
        assert_eq!(get(&tokens, "--focus-outline-style"), Some("solid"));
        assert_eq!(get(&tokens, "--focus-outline-offset"), Some("3px"));
        assert_eq!(
            get(&tokens, "--focus-outline-color"),
            get(&tokens, "--color-accent")
        );
    }

    // Expected values from running upstream's resolveDefaultTokens
    // (packages/core/dist) under Node 24, which agrees with this port on every
    // token in both modes and on their order.
    #[test]
    fn defaults_resolve_as_upstream_resolves_them() {
        let light = resolve_default_tokens(LIGHT);
        let dark = resolve_default_tokens(DARK);
        assert_eq!(light.len(), TOKEN_DEFAULTS.len());
        assert_eq!(
            light[0],
            ("--color-accent".to_string(), "#0064E0".to_string())
        );
        assert_eq!(get(&dark, "--color-accent"), Some("#0171E3"));
        assert_eq!(get(&light, "--color-neutral"), Some("rgba(5, 54, 89, 0.1)"));
        assert_eq!(get(&dark, "--color-text-info"), Some("#AFD7FF"));
        assert_eq!(get(&dark, "--color-shadow"), Some("rgba(0, 0, 0, 0.3)"));
        assert_eq!(get(&light, "--color-shadow"), Some("rgba(5, 54, 89, 0.1)"));
        assert_eq!(
            get(&dark, "--shadow-high"),
            Some(
                "0px 2px 2px light-dark(rgba(0, 0, 0, 0.1), rgba(0, 0, 0, 0.2)), 0px 8px 24px light-dark(rgba(0, 0, 0, 0.1), rgba(0, 0, 0, 0.3))"
            )
        );
        assert_eq!(
            get(&dark, "--shadow-inset-selected"),
            Some("inset 0px 0px 0px 2px rgba(1, 113, 227, 0.5)")
        );
    }

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() <= 1e-4
    }

    fn close_color(a: &Hsla, b: &Hsla) -> bool {
        close(a.h, b.h) && close(a.s, b.s) && close(a.l, b.l) && close(a.a, b.a)
    }

    fn close_length(a: &AbsoluteLength, b: &AbsoluteLength) -> bool {
        match (a, b) {
            (AbsoluteLength::Pixels(a), AbsoluteLength::Pixels(b)) => {
                close(f32::from(*a), f32::from(*b))
            }
            (AbsoluteLength::Rems(a), AbsoluteLength::Rems(b)) => close(a.0, b.0),
            _ => false,
        }
    }

    /// Whether two token values agree: floats within 1e-4, durations, names
    /// and keywords exactly.
    fn same(a: &TokenValue, b: &TokenValue) -> bool {
        match (a, b) {
            (TokenValue::Color(a), TokenValue::Color(b)) => close_color(a, b),
            (TokenValue::Length(a), TokenValue::Length(b)) => close_length(a, b),
            (TokenValue::Weight(a), TokenValue::Weight(b)) => close(a.0, b.0),
            (TokenValue::Leading(a), TokenValue::Leading(b))
            | (TokenValue::Number(a), TokenValue::Number(b)) => close(*a, *b),
            (TokenValue::Duration(a), TokenValue::Duration(b)) => a == b,
            (TokenValue::Ease(a), TokenValue::Ease(b)) => {
                close(a.0, b.0) && close(a.1, b.1) && close(a.2, b.2) && close(a.3, b.3)
            }
            (TokenValue::Families(a), TokenValue::Families(b)) => a == b,
            (TokenValue::Keyword(a), TokenValue::Keyword(b)) => a == b,
            (TokenValue::Shadow(a), TokenValue::Shadow(b)) => {
                a.0.len() == b.0.len()
                    && a.0.iter().zip(b.0.iter()).all(|(a, b)| {
                        close_color(&a.color, &b.color)
                            && close(f32::from(a.offset.x), f32::from(b.offset.x))
                            && close(f32::from(a.offset.y), f32::from(b.offset.y))
                            && close(f32::from(a.blur_radius), f32::from(b.blur_radius))
                            && close(f32::from(a.spread_radius), f32::from(b.spread_radius))
                            && a.inset == b.inset
                    })
            }
            _ => false,
        }
    }

    /// The decisive check of the resolver and the readers together: every
    /// default token, resolved for a mode as upstream's resolveDefaultTokens
    /// does and read as tools/export/export-themes.mjs reads it (var()
    /// substituted, light-dark() split, typed by its name, a length line height
    /// over its style's size), equals the same token of core's generated
    /// default theme, in both modes.
    #[test]
    fn resolved_defaults_read_as_cores_default_theme() {
        for (mode, theme) in [(LIGHT, &default_theme::LIGHT), (DARK, &default_theme::DARK)] {
            let resolved = resolve_default_tokens(mode);
            let read = |value: &str| {
                let substituted = substitute_vars(value, &resolved).expect("var() resolves");
                value_for_mode(&substituted, mode).expect("light-dark() splits")
            };
            let mut mismatches = Vec::new();
            // A length line height divides by its style's size, so sizes go first.
            let (leading, rest): (Vec<_>, Vec<_>) = resolved
                .iter()
                .partition(|(name, _)| name.ends_with("-leading"));
            for (name, value) in rest.into_iter().chain(leading) {
                let font_size = name.strip_suffix("-leading").map(|stem| {
                    let size =
                        get(&resolved, &format!("{stem}-size")).expect("a leading has a size");
                    to_points(js_trim(&read(size)), &mut Vec::new(), false)
                        .expect("a size is a length")
                });
                let mut notes = Vec::new();
                let context = TypedContext {
                    font_size,
                    property: None,
                };
                let ours = typed(token_kind(name), &read(value), &mut notes, &context)
                    .expect("a default reads")
                    .into_token()
                    .expect("a token value");
                assert!(notes.is_empty(), "{mode:?} {name}: {notes:?}");
                let theirs = theme
                    .get(name)
                    .expect("core's theme has every default token");
                if !same(&ours, &theirs) {
                    mismatches.push(format!("{mode:?} {name}: {ours:?} != {theirs:?}"));
                }
            }
            assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
        }
    }

    // The default theme names exactly the default tokens, so the check above
    // covers every token core's theme has.
    #[test]
    fn the_default_theme_names_exactly_the_default_tokens() {
        let theme: &Tokens = &default_theme::LIGHT;
        for (name, _) in TOKEN_DEFAULTS {
            assert!(theme.get(name).is_some(), "{name}");
        }
        assert_eq!(resolve_default_tokens(DARK).len(), TOKEN_DEFAULTS.len());
    }
}
