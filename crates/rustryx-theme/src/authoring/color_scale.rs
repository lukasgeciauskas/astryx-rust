//! Color scale expansion from an accent seed: upstream
//! `theme/expandColorScale.ts`.
//!
//! An accent seed expands into the color tokens that derive from it, through
//! the HCT model ([`super::hct`]): a primary palette from the seed's hue and
//! chroma (at least 48), and neutral and neutral-variant palettes from its hue
//! at a low chroma `neutral_style` sets. Only tokens that derive from the accent
//! are produced; status colors, categorical hues and the fixed
//! `--color-on-dark`/`--color-on-light` fall through to the defaults, and so do
//! the accent tokens themselves when the config has no accent.
//!
//! A `[light, dark]` accent pair seeds each color scheme from its own half: the
//! light side of every `light-dark()` pair derives from the light seed's
//! palettes, the dark side from the dark seed's. A single accent seeds both
//! sides alike.
//!
//! WCAG contrast guarantees (asserted in the tests):
//!
//! - Text tones reach 4.5:1 against their surfaces by tone spacing alone (HCT
//!   tone is CIE L*, which fixes relative luminance whatever the hue and
//!   chroma), so the fixed tones hold for any accent and neutral style (WCAG
//!   1.4.3).
//! - `--color-border-emphasized` (form-control boundaries) is tone-bumped until
//!   it reaches 3:1 against the generated surface (WCAG 1.4.11).
//! - `--color-border`, `--color-skeleton` and `--color-track` are decorative or
//!   redundant cues and are not held to 3:1.

use std::collections::BTreeMap;

use super::contrast::{ContrastError, contrast_ratio};
use super::hct::{Hct, hct_to_hex, hex_to_hct, hex_with_alpha, tonal_palette};
use super::js::{js_max, js_number_to_string};

/// How much of the seed's hue bleeds into the neutral and background colors.
/// Upstream `neutralStyle`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum NeutralStyle {
    /// `'warm'`: the most hue (neutral chroma 7, neutral-variant chroma 10).
    Warm,
    /// `'cool'`, the default (5 and 8).
    #[default]
    Cool,
    /// `'neutral'`: the least hue (3 and 6).
    Neutral,
}

/// The contrast level. `High` pulls text, icons and borders toward stronger
/// tones (and doubles the subtle border's alpha) so structural and textual
/// boundaries stay perceivable. Upstream `contrast`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ColorContrast {
    /// `'standard'`, the default.
    #[default]
    Standard,
    /// `'high'`.
    High,
}

/// The seed accent: one hex color (`#RRGGBB`) for both color schemes, or a
/// `[light, dark]` pair whose light seed derives the light scheme's palettes
/// and dark seed the dark scheme's. Upstream `string | [light, dark]`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AccentSeed {
    /// One seed for both schemes.
    Single(String),
    /// A light and a dark seed.
    Pair(String, String),
}

/// A color scale (upstream `ColorScaleConfig`): `{ accent: '#0064E0' }`, a
/// per-scheme `{ accent: ['#0064E0', '#48CAE4'] }`, one with `neutral_style`
/// and `contrast` set, or a neutral-only `{ neutralStyle: 'warm' }` that keeps
/// the default accent and themes the neutrals.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ColorScaleConfig {
    /// The seed everything derives from. Without one, the neutral palettes
    /// are seeded from the default accent's hue and the accent tokens
    /// (`--color-accent`, `--color-accent-muted`, `--color-on-accent`) are not
    /// generated, so they keep their defaults.
    pub accent: Option<AccentSeed>,
    /// Neutral tone warmth. Default [`NeutralStyle::Cool`].
    pub neutral_style: NeutralStyle,
    /// Contrast level. Default [`ColorContrast::Standard`].
    pub contrast: ColorContrast,
}

/// Generated color tokens (upstream `ColorScaleTokens`): CSS custom property
/// names and their CSS values, in upstream's order.
pub type ColorScaleTokens = Vec<(String, String)>;

impl NeutralStyle {
    /// The neutral palette's chroma. Upstream `NEUTRAL_CHROMA`.
    fn chroma(self) -> f64 {
        match self {
            NeutralStyle::Warm => 7.0,
            NeutralStyle::Cool => 5.0,
            NeutralStyle::Neutral => 3.0,
        }
    }

    /// The neutral-variant palette's chroma. Upstream
    /// `NEUTRAL_VARIANT_CHROMA`.
    fn variant_chroma(self) -> f64 {
        match self {
            NeutralStyle::Warm => 10.0,
            NeutralStyle::Cool => 8.0,
            NeutralStyle::Neutral => 6.0,
        }
    }
}

/// The hue source of a config without an accent: the light half of the
/// default `--color-accent` (a test guards the two against drift). Only its hue
/// reaches the output, since the accent tokens are not generated then.
/// Upstream `DEFAULT_ACCENT_SEED`.
const DEFAULT_ACCENT_SEED: &str = "#0064E0";

/// WCAG 1.4.11's minimum contrast for non-text UI boundaries. Upstream
/// `NON_TEXT_MIN_CONTRAST`.
const NON_TEXT_MIN_CONTRAST: f64 = 3.0;

/// `light-dark(light, dark)`. Upstream `ld`.
fn ld(light: &str, dark: &str) -> String {
    format!("light-dark({light}, {dark})")
}

/// `--color-accent` at an alpha, as a `color-mix()` that follows a scoped
/// override of the accent. Upstream `accentWithAlpha`.
fn accent_with_alpha(alpha: f64) -> String {
    format!(
        "color-mix(in srgb, var(--color-accent) {}%, transparent)",
        js_number_to_string(alpha * 100.0)
    )
}

/// The hex color at `tone`, walking tone by `step` (upstream types it `-1 |
/// 1`) from `start_tone` until it reaches `min_ratio` against `background`, or
/// tone 0 or 100 (upstream `ensureContrastTone`). HCT tone is CIE L*, so each
/// step moves luminance one way and the walk ends, at worst at pure black or
/// white.
///
/// Used for tokens whose preferred tone is not held by tone spacing alone,
/// such as `--color-border-emphasized`. Fails when `background` is not an
/// opaque color [`contrast_ratio`] reads, as upstream throws.
pub fn ensure_contrast_tone(
    hue: f64,
    chroma: f64,
    start_tone: f64,
    step: f64,
    background: &str,
    min_ratio: f64,
) -> Result<String, ContrastError> {
    let mut tone = start_tone;
    let mut hex = hct_to_hex(Hct { hue, chroma, tone });
    while contrast_ratio(hex.as_str(), background)? < min_ratio
        && (0.0..=100.0).contains(&(tone + step))
    {
        tone += step;
        hex = hct_to_hex(Hct { hue, chroma, tone });
    }
    Ok(hex)
}

/// A tonal palette's hex at one of its standard tones.
fn at(palette: &BTreeMap<u32, String>, tone: u32) -> &str {
    palette
        .get(&tone)
        .map(String::as_str)
        .expect("a tonal palette has every standard tone")
}

/// The color tokens `config` derives (upstream `expandColorScale`), in
/// upstream's order: the accent tokens (with an accent), then the neutrals,
/// surfaces, text, icons, borders and effects.
///
/// `--color-text-accent` and `--color-icon-accent` are `var(--color-accent)`,
/// and `--color-accent-muted` mixes it with transparent, so a scoped override
/// of the accent re-accents them; `--color-on-accent` is computed against the
/// accent and stays a color. `{ accent: '#0064E0' }` gives `--color-accent`
/// `light-dark(#0058D2, #BBC2FF)`.
pub fn expand_color_scale(config: &ColorScaleConfig) -> ColorScaleTokens {
    // A single accent (or none) seeds both halves alike, which keeps its
    // output identical to the pre-tuple implementation.
    let (light_accent, dark_accent) = match &config.accent {
        Some(AccentSeed::Single(accent)) => (Some(accent.as_str()), Some(accent.as_str())),
        Some(AccentSeed::Pair(light, dark)) => (Some(light.as_str()), Some(dark.as_str())),
        None => (None, None),
    };

    let light_seed = hex_to_hct(light_accent.unwrap_or(DEFAULT_ACCENT_SEED));
    let same_seed = dark_accent == light_accent;
    let dark_seed = if same_seed {
        light_seed
    } else {
        hex_to_hct(dark_accent.unwrap_or(DEFAULT_ACCENT_SEED))
    };

    let neutral_chroma = config.neutral_style.chroma();
    let neutral_variant_chroma = config.neutral_style.variant_chroma();

    // The *l palettes feed the light (first) half of each ld() pair, the *d
    // palettes the dark half; with one seed the d palettes are the l ones.
    let pl = tonal_palette(light_seed.hue, js_max(light_seed.chroma, 48.0));
    let nl = tonal_palette(light_seed.hue, neutral_chroma);
    let nvl = tonal_palette(light_seed.hue, neutral_variant_chroma);
    let dark_palettes = (!same_seed).then(|| {
        (
            tonal_palette(dark_seed.hue, js_max(dark_seed.chroma, 48.0)),
            tonal_palette(dark_seed.hue, neutral_chroma),
            tonal_palette(dark_seed.hue, neutral_variant_chroma),
        )
    });
    let (pd, nd, nvd) = match &dark_palettes {
        Some((pd, nd, nvd)) => (pd, nd, nvd),
        None => (&pl, &nl, &nvl),
    };

    let is_high = config.contrast == ColorContrast::High;

    let text_primary_light_tone = if is_high { 0 } else { 10 };
    let text_primary_dark_tone = if is_high { 99 } else { 90 };
    let text_secondary_light_tone = if is_high { 20 } else { 30 };
    let text_secondary_dark_tone = if is_high { 80 } else { 70 };

    // High contrast thickens the otherwise decorative subtle hairline by
    // doubling its alpha.
    let border_subtle_alpha = if is_high { 0.2 } else { 0.1 };

    // Emphasized borders outline form controls, so they are non-text UI
    // boundaries under WCAG 1.4.11 and must reach 3:1 against their surface.
    // High contrast starts at a stronger tone (50 against 70/30).
    let border_emphasized_start_light = if is_high { 50.0 } else { 70.0 };
    let border_emphasized_start_dark = if is_high { 50.0 } else { 30.0 };
    let border_emphasized = ld(
        &ensure_contrast_tone(
            light_seed.hue,
            neutral_variant_chroma,
            border_emphasized_start_light,
            -1.0,
            at(&nl, 99),
            NON_TEXT_MIN_CONTRAST,
        )
        .expect("a tonal palette tone is an opaque hex color"),
        &ensure_contrast_tone(
            dark_seed.hue,
            neutral_variant_chroma,
            border_emphasized_start_dark,
            1.0,
            at(nd, 10),
            NON_TEXT_MIN_CONTRAST,
        )
        .expect("a tonal palette tone is an opaque hex color"),
    );

    let mut tokens: Vec<(&str, String)> = Vec::with_capacity(27);
    // Only with a seed accent: without one these fall through to the
    // defaults, whose accent is not what the default seed derives. An empty
    // accent counts as given, as upstream's nullish check has it.
    if config.accent.is_some() {
        tokens.extend([
            ("--color-accent", ld(at(&pl, 40), at(pd, 80))),
            (
                "--color-accent-muted",
                ld(&accent_with_alpha(0.2), &accent_with_alpha(0.25)),
            ),
            ("--color-on-accent", ld(at(&pl, 100), at(pd, 20))),
        ]);
    }
    tokens.extend([
        (
            "--color-neutral",
            ld(
                &hex_with_alpha(at(&nl, 10), 0.1),
                &hex_with_alpha(at(nd, 90), 0.2),
            ),
        ),
        ("--color-background-surface", ld(at(&nl, 99), at(nd, 10))),
        ("--color-background-body", ld(at(&nl, 95), at(nd, 5))),
        (
            "--color-overlay",
            ld(
                &hex_with_alpha(at(&nl, 10), 0.4),
                &hex_with_alpha(at(nd, 10), 0.6),
            ),
        ),
        (
            "--color-overlay-hover",
            ld(
                &hex_with_alpha(at(&nl, 10), 0.05),
                &hex_with_alpha(at(nd, 100), 0.05),
            ),
        ),
        (
            "--color-overlay-pressed",
            ld(
                &hex_with_alpha(at(&nl, 10), 0.1),
                &hex_with_alpha(at(nd, 100), 0.1),
            ),
        ),
        (
            "--color-background-muted",
            ld(
                &hex_with_alpha(at(&nl, 10), 0.05),
                &hex_with_alpha(at(nd, 10), 0.5),
            ),
        ),
        // Text
        (
            "--color-text-primary",
            ld(
                at(&nl, text_primary_light_tone),
                at(nd, text_primary_dark_tone),
            ),
        ),
        (
            "--color-text-secondary",
            ld(
                at(&nvl, text_secondary_light_tone),
                at(nvd, text_secondary_dark_tone),
            ),
        ),
        ("--color-text-disabled", ld(at(&nvl, 60), at(nvd, 40))),
        ("--color-text-accent", "var(--color-accent)".to_string()),
        // Icon
        ("--color-icon-accent", "var(--color-accent)".to_string()),
        (
            "--color-icon-primary",
            ld(
                at(&nl, text_primary_light_tone),
                at(nd, text_primary_dark_tone),
            ),
        ),
        (
            "--color-icon-secondary",
            ld(
                at(&nvl, text_secondary_light_tone),
                at(nvd, text_secondary_dark_tone),
            ),
        ),
        ("--color-icon-disabled", ld(at(&nvl, 60), at(nvd, 40))),
        // Surface variants
        ("--color-background-card", ld(at(&nl, 99), at(nd, 10))),
        ("--color-background-popover", ld(at(&nl, 99), at(nd, 20))),
        ("--color-background-inverted", ld(at(&nl, 10), at(nd, 99))),
        // Border: a decorative hairline, not a WCAG 1.4.11 boundary.
        (
            "--color-border",
            ld(
                &hex_with_alpha(at(&nl, 10), border_subtle_alpha),
                &hex_with_alpha(at(nd, 95), border_subtle_alpha),
            ),
        ),
        ("--color-border-emphasized", border_emphasized),
        // Effects
        ("--color-skeleton", ld(at(&nvl, 70), at(nvd, 30))),
        // The channel-on-body surface (ProgressBar and Slider tracks, Switch
        // off), the same ramp stop as --color-skeleton.
        ("--color-track", ld(at(&nvl, 70), at(nvd, 30))),
        (
            "--color-shadow",
            ld(
                &hex_with_alpha(at(&nl, 0), 0.1),
                &hex_with_alpha(at(nd, 0), 0.3),
            ),
        ),
        ("--color-tint-hover", ld("black", "white")),
    ]);
    tokens
        .into_iter()
        .map(|(name, value)| (name.to_string(), value))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::authoring::TOKEN_DEFAULTS;
    use crate::authoring::color::{format_color, parse_color};
    use crate::authoring::contrast::composite_over;
    use crate::authoring::token_references::resolve_token_references_with;
    use crate::authoring::token_values::{
        ResolvedThemeMode, evaluate_color_functions, resolve_xds_token_value,
    };

    const LIGHT: ResolvedThemeMode = ResolvedThemeMode::Light;
    const DARK: ResolvedThemeMode = ResolvedThemeMode::Dark;

    /// The light half of the default `--color-accent`, the hue an accent-less
    /// config seeds from, and its dark half.
    const DEFAULT_ACCENT: &str = "#0064E0";
    const DEFAULT_ACCENT_DARK: &str = "#0171E3";

    /// The three tokens that exist only when a seed accent is given.
    const ACCENT_TOKENS: [&str; 3] = [
        "--color-accent",
        "--color-accent-muted",
        "--color-on-accent",
    ];

    fn expand(
        accent: Option<AccentSeed>,
        neutral_style: NeutralStyle,
        contrast: ColorContrast,
    ) -> ColorScaleTokens {
        expand_color_scale(&ColorScaleConfig {
            accent,
            neutral_style,
            contrast,
        })
    }

    fn single(hex: &str) -> Option<AccentSeed> {
        Some(AccentSeed::Single(hex.to_string()))
    }

    fn pair(light: &str, dark: &str) -> Option<AccentSeed> {
        Some(AccentSeed::Pair(light.to_string(), dark.to_string()))
    }

    fn accent(hex: &str) -> ColorScaleTokens {
        expand(single(hex), NeutralStyle::Cool, ColorContrast::Standard)
    }

    fn get<'a>(tokens: &'a [(String, String)], name: &str) -> Option<&'a str> {
        tokens
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }

    fn token<'a>(tokens: &'a [(String, String)], name: &str) -> &'a str {
        get(tokens, name).unwrap_or_else(|| panic!("{name} is generated"))
    }

    fn is_hex_pair(value: &str) -> bool {
        value.starts_with("light-dark(#")
    }

    fn default_token(name: &str) -> &'static str {
        TOKEN_DEFAULTS
            .iter()
            .find(|(key, _)| *key == name)
            .map(|(_, value)| *value)
            .unwrap_or_else(|| panic!("{name} has a default"))
    }

    /// The test's `splitLightDark`: a `light-dark(a, b)` token's halves (split at
    /// the first comma), or the value for both modes.
    fn split_light_dark(value: &str) -> (&str, &str) {
        value
            .strip_prefix("light-dark(")
            .and_then(|inner| inner.strip_suffix(')'))
            .and_then(|inner| inner.split_once(','))
            .map_or((value, value), |(light, dark)| (light, dark.trim_start()))
    }

    /// The test's `resolveToken`: one mode of a token.
    fn resolve_token<'a>(
        tokens: &'a [(String, String)],
        name: &str,
        mode: ResolvedThemeMode,
    ) -> &'a str {
        let (light, dark) = split_light_dark(token(tokens, name));
        match mode {
            ResolvedThemeMode::Light => light,
            ResolvedThemeMode::Dark => dark,
        }
    }

    /// The test's `resolveComposited`: one mode of an alpha color painted over
    /// its backdrop.
    fn composited(value: &str, backdrop: &str) -> String {
        let fg = parse_color(value).expect("token parses");
        let bg = parse_color(backdrop).expect("backdrop parses");
        format_color(composite_over(fg, bg))
    }

    fn ratio(foreground: &str, background: &str) -> f64 {
        contrast_ratio(foreground, background).expect("colors parse")
    }

    /// A theme of these color tokens resolved in one mode, as upstream's
    /// `resolveThemeTokens(defineTheme({color}), {mode})` resolves the color
    /// tokens: the defaults with the expansion over them, one side picked, then
    /// references and color functions resolved.
    fn resolve_theme(
        tokens: &[(String, String)],
        mode: ResolvedThemeMode,
    ) -> Vec<(String, String)> {
        let mut raw: Vec<(&str, &str)> = TOKEN_DEFAULTS
            .iter()
            .map(|&(name, value)| (name, resolve_xds_token_value(value, mode)))
            .collect();
        raw.extend(
            tokens
                .iter()
                .map(|(name, value)| (name.as_str(), resolve_xds_token_value(value, mode))),
        );
        resolve_token_references_with(&raw, evaluate_color_functions)
    }

    fn assert_snapshot(tokens: &ColorScaleTokens, expected: &[(&str, &str)]) {
        let actual: Vec<(&str, &str)> = tokens
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        assert_eq!(actual, expected);
    }

    // Expected values from running upstream's expandColorScale
    // (packages/core/dist) under Node 24.
    #[test]
    fn default_accent_expands_to_upstreams_tokens() {
        assert_snapshot(
            &accent("#0064E0"),
            &[
                ("--color-accent", "light-dark(#0058D2, #BBC2FF)"),
                (
                    "--color-accent-muted",
                    "light-dark(color-mix(in srgb, var(--color-accent) 20%, transparent), color-mix(in srgb, var(--color-accent) 25%, transparent))",
                ),
                ("--color-on-accent", "light-dark(#FFFFFF, #002D80)"),
                ("--color-neutral", "light-dark(#1B1B221A, #E1E2EB33)"),
                ("--color-background-surface", "light-dark(#FCFCFF, #1B1B22)"),
                ("--color-background-body", "light-dark(#F0F0FA, #101018)"),
                ("--color-overlay", "light-dark(#1B1B2266, #1B1B2299)"),
                ("--color-overlay-hover", "light-dark(#1B1B220D, #FFFFFF0D)"),
                (
                    "--color-overlay-pressed",
                    "light-dark(#1B1B221A, #FFFFFF1A)",
                ),
                (
                    "--color-background-muted",
                    "light-dark(#1B1B220D, #1B1B2280)",
                ),
                ("--color-text-primary", "light-dark(#1B1B22, #E1E2EB)"),
                ("--color-text-secondary", "light-dark(#454652, #A9AAB9)"),
                ("--color-text-disabled", "light-dark(#8F909E, #5D5D6A)"),
                ("--color-text-accent", "var(--color-accent)"),
                ("--color-icon-accent", "var(--color-accent)"),
                ("--color-icon-primary", "light-dark(#1B1B22, #E1E2EB)"),
                ("--color-icon-secondary", "light-dark(#454652, #A9AAB9)"),
                ("--color-icon-disabled", "light-dark(#8F909E, #5D5D6A)"),
                ("--color-background-card", "light-dark(#FCFCFF, #1B1B22)"),
                ("--color-background-popover", "light-dark(#FCFCFF, #2F3037)"),
                (
                    "--color-background-inverted",
                    "light-dark(#1B1B22, #FCFCFF)",
                ),
                ("--color-border", "light-dark(#1B1B221A, #F0F0FA1A)"),
                ("--color-border-emphasized", "light-dark(#8F909E, #666774)"),
                ("--color-skeleton", "light-dark(#A9AAB9, #454652)"),
                ("--color-track", "light-dark(#A9AAB9, #454652)"),
                ("--color-shadow", "light-dark(#0000001A, #0000004D)"),
                ("--color-tint-hover", "light-dark(black, white)"),
            ],
        );
    }

    // Expected values from running upstream's expandColorScale under Node 24.
    #[test]
    fn tuple_accent_expands_to_upstreams_tokens() {
        assert_snapshot(
            &expand(
                pair("#B7410E", "#48CAE4"),
                NeutralStyle::Cool,
                ColorContrast::Standard,
            ),
            &[
                ("--color-accent", "light-dark(#AA3602, #00DAFB)"),
                (
                    "--color-accent-muted",
                    "light-dark(color-mix(in srgb, var(--color-accent) 20%, transparent), color-mix(in srgb, var(--color-accent) 25%, transparent))",
                ),
                ("--color-on-accent", "light-dark(#FFFFFF, #00394A)"),
                ("--color-neutral", "light-dark(#221A171A, #D7E5E833)"),
                ("--color-background-surface", "light-dark(#FFFBF9, #131D20)"),
                ("--color-background-body", "light-dark(#FAEEEA, #051316)"),
                ("--color-overlay", "light-dark(#221A1766, #131D2099)"),
                ("--color-overlay-hover", "light-dark(#221A170D, #FFFFFF0D)"),
                (
                    "--color-overlay-pressed",
                    "light-dark(#221A171A, #FFFFFF1A)",
                ),
                (
                    "--color-background-muted",
                    "light-dark(#221A170D, #131D2080)",
                ),
                ("--color-text-primary", "light-dark(#221A17, #D7E5E8)"),
                ("--color-text-secondary", "light-dark(#52443E, #9AAFB4)"),
                ("--color-text-disabled", "light-dark(#9E8D86, #4E6266)"),
                ("--color-text-accent", "var(--color-accent)"),
                ("--color-icon-accent", "var(--color-accent)"),
                ("--color-icon-primary", "light-dark(#221A17, #D7E5E8)"),
                ("--color-icon-secondary", "light-dark(#52443E, #9AAFB4)"),
                ("--color-icon-disabled", "light-dark(#9E8D86, #4E6266)"),
                ("--color-background-card", "light-dark(#FFFBF9, #131D20)"),
                ("--color-background-popover", "light-dark(#FFFBF9, #273235)"),
                (
                    "--color-background-inverted",
                    "light-dark(#221A17, #F3FEFF)",
                ),
                ("--color-border", "light-dark(#221A171A, #E6F3F71A)"),
                ("--color-border-emphasized", "light-dark(#9E8D86, #586B70)"),
                ("--color-skeleton", "light-dark(#BAA7A1, #374A4E)"),
                ("--color-track", "light-dark(#BAA7A1, #374A4E)"),
                ("--color-shadow", "light-dark(#0000001A, #0000004D)"),
                ("--color-tint-hover", "light-dark(black, white)"),
            ],
        );
    }

    // Expected values from running upstream's expandColorScale under Node 24.
    #[test]
    fn warm_high_contrast_expands_to_upstreams_tokens() {
        assert_snapshot(
            &expand(single("#B7410E"), NeutralStyle::Warm, ColorContrast::High),
            &[
                ("--color-accent", "light-dark(#AA3602, #FFB495)"),
                (
                    "--color-accent-muted",
                    "light-dark(color-mix(in srgb, var(--color-accent) 20%, transparent), color-mix(in srgb, var(--color-accent) 25%, transparent))",
                ),
                ("--color-on-accent", "light-dark(#FFFFFF, #6F0000)"),
                ("--color-neutral", "light-dark(#2419151A, #F0DFD933)"),
                ("--color-background-surface", "light-dark(#FFFBF9, #241915)"),
                ("--color-background-body", "light-dark(#FEEDE7, #1B0E06)"),
                ("--color-overlay", "light-dark(#24191566, #24191599)"),
                ("--color-overlay-hover", "light-dark(#2419150D, #FFFFFF0D)"),
                (
                    "--color-overlay-pressed",
                    "light-dark(#2419151A, #FFFFFF1A)",
                ),
                (
                    "--color-background-muted",
                    "light-dark(#2419150D, #24191580)",
                ),
                ("--color-text-primary", "light-dark(#000000, #FFFBF9)"),
                ("--color-text-secondary", "light-dark(#3E2D26, #D9C2B9)"),
                ("--color-text-disabled", "light-dark(#A28C84, #6E5A52)"),
                ("--color-text-accent", "var(--color-accent)"),
                ("--color-icon-accent", "var(--color-accent)"),
                ("--color-icon-primary", "light-dark(#000000, #FFFBF9)"),
                ("--color-icon-secondary", "light-dark(#3E2D26, #D9C2B9)"),
                ("--color-icon-disabled", "light-dark(#A28C84, #6E5A52)"),
                ("--color-background-card", "light-dark(#FFFBF9, #241915)"),
                ("--color-background-popover", "light-dark(#FFFBF9, #3A2E29)"),
                (
                    "--color-background-inverted",
                    "light-dark(#241915, #FFFBF9)",
                ),
                ("--color-border", "light-dark(#24191533, #FEEDE733)"),
                ("--color-border-emphasized", "light-dark(#87736B, #87736B)"),
                ("--color-skeleton", "light-dark(#BDA79E, #55433B)"),
                ("--color-track", "light-dark(#BDA79E, #55433B)"),
                ("--color-shadow", "light-dark(#0000001A, #0000004D)"),
                ("--color-tint-hover", "light-dark(black, white)"),
            ],
        );
    }

    // Expected values from running upstream's expandColorScale under Node 24.
    #[test]
    fn neutral_only_config_expands_to_upstreams_tokens() {
        assert_snapshot(
            &expand(None, NeutralStyle::Warm, ColorContrast::Standard),
            &[
                ("--color-neutral", "light-dark(#1A1B241A, #E1E1EF33)"),
                ("--color-background-surface", "light-dark(#FCFCFF, #1A1B24)"),
                ("--color-background-body", "light-dark(#EFF0FD, #10101A)"),
                ("--color-overlay", "light-dark(#1A1B2466, #1A1B2499)"),
                ("--color-overlay-hover", "light-dark(#1A1B240D, #FFFFFF0D)"),
                (
                    "--color-overlay-pressed",
                    "light-dark(#1A1B241A, #FFFFFF1A)",
                ),
                (
                    "--color-background-muted",
                    "light-dark(#1A1B240D, #1A1B2480)",
                ),
                ("--color-text-primary", "light-dark(#1A1B24, #E1E1EF)"),
                ("--color-text-secondary", "light-dark(#444655, #A9AABC)"),
                ("--color-text-disabled", "light-dark(#8E8FA1, #5C5D6E)"),
                ("--color-text-accent", "var(--color-accent)"),
                ("--color-icon-accent", "var(--color-accent)"),
                ("--color-icon-primary", "light-dark(#1A1B24, #E1E1EF)"),
                ("--color-icon-secondary", "light-dark(#444655, #A9AABC)"),
                ("--color-icon-disabled", "light-dark(#8E8FA1, #5C5D6E)"),
                ("--color-background-card", "light-dark(#FCFCFF, #1A1B24)"),
                ("--color-background-popover", "light-dark(#FCFCFF, #2F303A)"),
                (
                    "--color-background-inverted",
                    "light-dark(#1A1B24, #FCFCFF)",
                ),
                ("--color-border", "light-dark(#1A1B241A, #EFF0FD1A)"),
                ("--color-border-emphasized", "light-dark(#8E8FA1, #666778)"),
                ("--color-skeleton", "light-dark(#A9AABC, #444655)"),
                ("--color-track", "light-dark(#A9AABC, #444655)"),
                ("--color-shadow", "light-dark(#0000001A, #0000004D)"),
                ("--color-tint-hover", "light-dark(black, white)"),
            ],
        );
    }

    // Upstream expandColorScale.test.ts: "produces all expected token keys".
    // ("all values are strings" holds by the type.)
    #[test]
    fn produces_every_expected_token() {
        let tokens = accent("#0064E0");
        let expected = [
            "--color-accent",
            "--color-accent-muted",
            "--color-on-accent",
            "--color-neutral",
            "--color-background-surface",
            "--color-background-body",
            "--color-overlay",
            "--color-overlay-hover",
            "--color-overlay-pressed",
            "--color-background-muted",
            "--color-text-primary",
            "--color-text-secondary",
            "--color-text-disabled",
            "--color-text-accent",
            "--color-icon-accent",
            "--color-icon-primary",
            "--color-icon-secondary",
            "--color-icon-disabled",
            "--color-background-card",
            "--color-background-popover",
            "--color-background-inverted",
            "--color-border",
            "--color-border-emphasized",
            "--color-skeleton",
            "--color-track",
            "--color-shadow",
            "--color-tint-hover",
        ];
        for key in expected {
            assert!(get(&tokens, key).is_some(), "{key}");
        }
    }

    // Upstream: "neutralStyle variants produce different --color-neutral values".
    #[test]
    fn neutral_styles_differ_in_the_neutral_token() {
        let neutral = |style| {
            token(
                &expand(single("#0064E0"), style, ColorContrast::Standard),
                "--color-neutral",
            )
            .to_string()
        };
        let (warm, cool, plain) = (
            neutral(NeutralStyle::Warm),
            neutral(NeutralStyle::Cool),
            neutral(NeutralStyle::Neutral),
        );
        assert_ne!(warm, cool);
        assert_ne!(cool, plain);
        assert_ne!(warm, plain);
    }

    // Upstream: "emits derived accent tokens as references to --color-accent".
    #[test]
    fn derived_accent_tokens_reference_the_accent() {
        let tokens = accent("#0064E0");
        assert_eq!(token(&tokens, "--color-text-accent"), "var(--color-accent)");
        assert_eq!(token(&tokens, "--color-icon-accent"), "var(--color-accent)");
        assert_eq!(
            token(&tokens, "--color-accent-muted"),
            "light-dark(color-mix(in srgb, var(--color-accent) 20%, transparent), color-mix(in srgb, var(--color-accent) 25%, transparent))"
        );
        assert!(is_hex_pair(token(&tokens, "--color-accent")));
        assert!(is_hex_pair(token(&tokens, "--color-on-accent")));
    }

    // Upstream: "contrast high produces different --color-text-primary than
    // standard" and "contrast high strengthens both border tokens".
    #[test]
    fn high_contrast_changes_text_and_both_borders() {
        let standard = accent("#0064E0");
        let high = expand(single("#0064E0"), NeutralStyle::Cool, ColorContrast::High);
        for key in [
            "--color-text-primary",
            "--color-border",
            "--color-border-emphasized",
        ] {
            assert_ne!(token(&high, key), token(&standard, key), "{key}");
        }
    }

    // Upstream: "accent is optional — a config without one expands" and
    // "expands an empty config".
    #[test]
    fn a_config_without_an_accent_expands() {
        let warm = expand(None, NeutralStyle::Warm, ColorContrast::Standard);
        assert!(is_hex_pair(token(&warm, "--color-background-surface")));
        let empty = expand_color_scale(&ColorScaleConfig::default());
        assert!(is_hex_pair(token(&empty, "--color-background-surface")));
        assert_eq!(get(&empty, "--color-accent"), None);
    }

    // Upstream: "omits the accent tokens so they fall through to colorDefaults".
    #[test]
    fn neutral_only_omits_the_accent_tokens() {
        let tokens = expand(None, NeutralStyle::Warm, ColorContrast::Standard);
        for key in ACCENT_TOKENS {
            assert_eq!(get(&tokens, key), None, "{key}");
        }
        assert_eq!(token(&tokens, "--color-text-accent"), "var(--color-accent)");
        assert_eq!(token(&tokens, "--color-icon-accent"), "var(--color-accent)");
    }

    // Upstream: "seeds the neutral palettes from the light half of the default
    // accent", with its drift guard on the default accent.
    #[test]
    fn neutral_only_seeds_from_the_default_accent() {
        assert_eq!(
            default_token("--color-accent"),
            format!("light-dark({DEFAULT_ACCENT}, {DEFAULT_ACCENT_DARK})")
        );
        let neutral_only = expand(None, NeutralStyle::Warm, ColorContrast::Standard);
        let seeded = expand(
            single(DEFAULT_ACCENT),
            NeutralStyle::Warm,
            ColorContrast::Standard,
        );
        for (key, value) in &seeded {
            if ACCENT_TOKENS.contains(&key.as_str()) {
                continue;
            }
            assert_eq!(get(&neutral_only, key), Some(value.as_str()), "{key}");
        }
    }

    // Upstream: "honours neutralStyle and contrast without an accent".
    #[test]
    fn neutral_only_honours_style_and_contrast() {
        let neutral = |style| {
            token(
                &expand(None, style, ColorContrast::Standard),
                "--color-neutral",
            )
            .to_string()
        };
        assert_ne!(neutral(NeutralStyle::Warm), neutral(NeutralStyle::Cool));
        let text = |contrast| {
            token(
                &expand(None, NeutralStyle::Cool, contrast),
                "--color-text-primary",
            )
            .to_string()
        };
        assert_ne!(text(ColorContrast::High), text(ColorContrast::Standard));
    }

    // Upstream: "does not re-color themes that do pass an accent".
    #[test]
    fn an_accent_seed_derives_its_own_accent() {
        let tokens = accent(DEFAULT_ACCENT);
        assert!(is_hex_pair(token(&tokens, "--color-accent")));
        assert_ne!(
            token(&tokens, "--color-accent"),
            default_token("--color-accent")
        );
    }

    // Upstream: "treats an empty accent as supplied, not absent".
    #[test]
    fn an_empty_accent_counts_as_given() {
        let tokens = accent("");
        for key in ACCENT_TOKENS {
            assert!(get(&tokens, key).is_some(), "{key}");
        }
        // Expected value from running upstream under Node 24: the hex parser
        // reads "" as black, whose hue is 0.
        assert_eq!(
            token(&tokens, "--color-accent"),
            "light-dark(#A23760, #FFAFC8)"
        );
    }

    const LIGHT_SEED: &str = "#B7410E";
    const DARK_SEED: &str = "#48CAE4";

    // Upstream: "accepts a [light, dark] tuple and generates the accent tokens".
    #[test]
    fn a_tuple_accent_generates_the_accent_tokens() {
        let tokens = expand(
            pair(LIGHT_SEED, DARK_SEED),
            NeutralStyle::Cool,
            ColorContrast::Standard,
        );
        for key in ACCENT_TOKENS {
            assert!(get(&tokens, key).is_some(), "{key}");
        }
        assert!(is_hex_pair(token(&tokens, "--color-accent")));
        assert!(is_hex_pair(token(&tokens, "--color-on-accent")));
    }

    // Upstream: "a same-halves tuple matches the single-string expansion, token
    // for token".
    #[test]
    fn a_same_halves_tuple_matches_one_accent() {
        assert_eq!(
            expand(
                pair(LIGHT_SEED, LIGHT_SEED),
                NeutralStyle::Cool,
                ColorContrast::Standard
            ),
            accent(LIGHT_SEED)
        );
    }

    // Upstream: "derives the light palette from the light seed and the dark
    // palette from the dark seed", with the theme resolved as
    // resolveThemeTokens resolves its color tokens.
    #[test]
    fn each_half_of_a_tuple_derives_from_its_own_seed() {
        let tuple = expand(
            pair(LIGHT_SEED, DARK_SEED),
            NeutralStyle::Cool,
            ColorContrast::Standard,
        );
        let from_light = accent(LIGHT_SEED);
        let from_dark = accent(DARK_SEED);
        let tuple_light = resolve_theme(&tuple, LIGHT);
        let ref_light = resolve_theme(&from_light, LIGHT);
        let tuple_dark = resolve_theme(&tuple, DARK);
        let ref_dark = resolve_theme(&from_dark, DARK);
        for (key, _) in &tuple {
            assert_eq!(
                get(&tuple_light, key),
                get(&ref_light, key),
                "{key} (light)"
            );
            assert_eq!(get(&tuple_dark, key), get(&ref_dark, key), "{key} (dark)");
        }
    }

    // Upstream: "differing seeds produce a dark half the light seed would not".
    #[test]
    fn a_tuple_dark_half_does_not_reuse_the_light_seed() {
        let tuple = expand(
            pair(LIGHT_SEED, DARK_SEED),
            NeutralStyle::Cool,
            ColorContrast::Standard,
        );
        let single = accent(LIGHT_SEED);
        assert_ne!(
            get(&resolve_theme(&tuple, DARK), "--color-accent"),
            get(&resolve_theme(&single, DARK), "--color-accent")
        );
    }

    // Upstream: "keeps the derived accent tokens as references with a tuple
    // accent" and "honours neutralStyle with a tuple accent".
    #[test]
    fn a_tuple_accent_keeps_references_and_honours_the_neutral_style() {
        let tokens = expand(
            pair(LIGHT_SEED, DARK_SEED),
            NeutralStyle::Warm,
            ColorContrast::Standard,
        );
        assert_eq!(token(&tokens, "--color-text-accent"), "var(--color-accent)");
        assert_eq!(token(&tokens, "--color-icon-accent"), "var(--color-accent)");
        assert!(token(&tokens, "--color-accent-muted").contains("var(--color-accent)"));
        let cool = expand(
            pair(LIGHT_SEED, DARK_SEED),
            NeutralStyle::Cool,
            ColorContrast::Standard,
        );
        assert_ne!(
            token(&tokens, "--color-neutral"),
            token(&cool, "--color-neutral")
        );
    }

    // Upstream: "a neutral-only theme resolves accent tokens to the defaults at
    // runtime (#2279)", on the color tokens resolveThemeTokens resolves.
    #[test]
    fn a_neutral_only_theme_resolves_the_default_accent() {
        let tokens = expand(None, NeutralStyle::Warm, ColorContrast::Standard);
        let light = resolve_theme(&tokens, LIGHT);
        assert_eq!(get(&light, "--color-accent"), Some(DEFAULT_ACCENT));
        assert_eq!(get(&light, "--color-text-accent"), Some(DEFAULT_ACCENT));
        assert_eq!(get(&light, "--color-icon-accent"), Some(DEFAULT_ACCENT));
        let dark = resolve_theme(&tokens, DARK);
        assert_eq!(get(&dark, "--color-accent"), Some(DEFAULT_ACCENT_DARK));
        assert_eq!(get(&dark, "--color-text-accent"), Some(DEFAULT_ACCENT_DARK));
    }

    /// Upstream's representative WCAG configs: the docs' default accent, both
    /// contrast levels, every neutral style, saturated brand accents across the
    /// hue wheel, a gray seed, and black and white seeds.
    fn wcag_configs() -> Vec<ColorScaleConfig> {
        use ColorContrast::{High, Standard};
        use NeutralStyle::{Cool, Neutral, Warm};
        [
            ("#0064E0", Cool, Standard),
            ("#0064E0", Cool, High),
            ("#DC2626", Warm, Standard),
            ("#B7410E", Warm, High),
            ("#12B76A", Neutral, Standard),
            ("#D9006E", Warm, Standard),
            ("#657100", Cool, Standard),
            ("#6B7280", Cool, Standard),
            ("#000000", Cool, Standard),
            ("#FFFFFF", Neutral, High),
        ]
        .into_iter()
        .map(|(hex, neutral_style, contrast)| ColorScaleConfig {
            accent: single(hex),
            neutral_style,
            contrast,
        })
        .collect()
    }

    // Upstream: "text tokens meet 4.5:1 on the surfaces the system pairs them
    // with", for every config and mode.
    #[test]
    fn text_tokens_meet_4_5_to_1_on_their_surfaces() {
        for config in wcag_configs() {
            let tokens = expand_color_scale(&config);
            for mode in [LIGHT, DARK] {
                let at = |name| resolve_token(&tokens, name, mode);
                let surface = at("--color-background-surface");
                let muted = composited(at("--color-background-muted"), surface);
                let neutral = composited(at("--color-neutral"), surface);
                let backgrounds = [
                    surface,
                    at("--color-background-body"),
                    at("--color-background-card"),
                    at("--color-background-popover"),
                ];
                let text_primary = at("--color-text-primary");
                let text_secondary = at("--color-text-secondary");
                let accent = at("--color-accent");
                let on_accent = at("--color-on-accent");
                for bg in backgrounds
                    .iter()
                    .copied()
                    .chain([muted.as_str(), neutral.as_str()])
                {
                    assert!(
                        ratio(text_primary, bg) >= 4.5,
                        "{config:?} {mode:?} primary on {bg}"
                    );
                }
                for bg in backgrounds {
                    assert!(
                        ratio(text_secondary, bg) >= 4.5,
                        "{config:?} {mode:?} secondary on {bg}"
                    );
                }
                assert!(ratio(accent, surface) >= 4.5, "{config:?} {mode:?} accent");
                assert!(
                    ratio(on_accent, accent) >= 4.5,
                    "{config:?} {mode:?} on-accent"
                );
            }
        }
    }

    // Upstream: "secondary text meets 4.5:1 on the default palette tints over
    // the surface and body".
    #[test]
    fn secondary_text_meets_4_5_to_1_on_the_default_tints() {
        let tints: Vec<&str> = TOKEN_DEFAULTS
            .iter()
            .map(|(name, _)| *name)
            .filter(|name| {
                name.strip_prefix("--color-background-").is_some_and(|hue| {
                    matches!(
                        hue,
                        "blue"
                            | "cyan"
                            | "gray"
                            | "green"
                            | "orange"
                            | "pink"
                            | "purple"
                            | "red"
                            | "teal"
                            | "yellow"
                    )
                })
            })
            .collect();
        assert_eq!(tints.len(), 10);
        for config in wcag_configs() {
            let tokens = expand_color_scale(&config);
            for mode in [LIGHT, DARK] {
                let text_secondary = resolve_token(&tokens, "--color-text-secondary", mode);
                for backdrop_name in ["--color-background-surface", "--color-background-body"] {
                    let backdrop = resolve_token(&tokens, backdrop_name, mode);
                    for tint in &tints {
                        let (light, dark) = split_light_dark(default_token(tint));
                        let side = if mode == LIGHT { light } else { dark };
                        let painted = composited(side, backdrop);
                        assert!(
                            ratio(text_secondary, &painted) >= 4.5,
                            "{config:?} {mode:?}: {tint} over {backdrop_name}"
                        );
                    }
                }
            }
        }
    }

    // Upstream: "non-text UI tokens meet 3:1 against the surface".
    #[test]
    fn non_text_tokens_meet_3_to_1_on_the_surface() {
        for config in wcag_configs() {
            let tokens = expand_color_scale(&config);
            for mode in [LIGHT, DARK] {
                let surface = resolve_token(&tokens, "--color-background-surface", mode);
                for key in [
                    "--color-accent",
                    "--color-icon-primary",
                    "--color-icon-secondary",
                    "--color-border-emphasized",
                ] {
                    let value = resolve_token(&tokens, key, mode);
                    assert!(ratio(value, surface) >= 3.0, "{config:?} {mode:?} {key}");
                }
            }
        }
    }

    // Upstream: "documents the intentionally decorative tokens excluded from
    // 3:1": the subtle border stays a translucent hairline.
    #[test]
    fn the_subtle_border_stays_a_decorative_alpha() {
        let tokens = accent("#0064E0");
        let (light, dark) = split_light_dark(token(&tokens, "--color-border"));
        for half in [light, dark] {
            let parsed = parse_color(half).expect("border parses");
            assert!(parsed.a <= 0.2, "{half}");
        }
    }

    // Upstream: "ensureContrastTone returns the starting tone when it already
    // meets the ratio" (#181B26 from running upstream under Node 24).
    #[test]
    fn ensure_contrast_tone_keeps_a_tone_that_already_passes() {
        let hex = ensure_contrast_tone(282.0, 8.0, 10.0, -1.0, "#FCFDFE", 4.5).unwrap();
        assert_eq!(
            hex,
            ensure_contrast_tone(282.0, 8.0, 10.0, 1.0, "#FCFDFE", 1.0).unwrap()
        );
        assert_eq!(hex, "#181B26");
    }

    // Upstream: "bumps the tone until the ratio passes" (#A7ABB9 and #8F93A1
    // from running upstream under Node 24).
    #[test]
    fn ensure_contrast_tone_bumps_until_the_ratio_passes() {
        let before = ensure_contrast_tone(282.0, 8.0, 70.0, -1.0, "#FCFDFE", 1.0).unwrap();
        assert!(ratio(&before, "#FCFDFE") < 3.0);
        assert_eq!(before, "#A7ABB9");
        let after = ensure_contrast_tone(282.0, 8.0, 70.0, -1.0, "#FCFDFE", 3.0).unwrap();
        assert!(ratio(&after, "#FCFDFE") >= 3.0);
        assert_eq!(after, "#8F93A1");
    }

    // Upstream: "terminates at the tone floor/ceiling for impossible ratios".
    #[test]
    fn ensure_contrast_tone_stops_at_black_or_white() {
        assert_eq!(
            ensure_contrast_tone(282.0, 8.0, 70.0, -1.0, "#FFFFFF", 22.0).unwrap(),
            "#000000"
        );
        assert_eq!(
            ensure_contrast_tone(282.0, 8.0, 30.0, 1.0, "#000000", 22.0).unwrap(),
            "#FFFFFF"
        );
    }

    // Expected from reading ensureContrastTone: contrastRatio throws for a
    // background it cannot parse.
    #[test]
    fn ensure_contrast_tone_fails_on_an_unparseable_background() {
        assert!(ensure_contrast_tone(282.0, 8.0, 70.0, -1.0, "var(--x)", 3.0).is_err());
    }

    /// Jest's `toBeCloseTo(expected, 1)`.
    fn close_to_one_digit(actual: f64, expected: f64) -> bool {
        (actual - expected).abs() < 0.05
    }

    // Upstream: "--color-border-emphasized WCAG 1.4.11 correction
    // (before/after)", light and dark.
    #[test]
    fn border_emphasized_is_corrected_to_3_to_1() {
        let tokens = accent("#0064E0");
        let hue = hex_to_hct("#0064E0").hue;
        // What the generator emitted before the correction: NV[70] light, NV[30] dark.
        let nv = tonal_palette(hue, 8.0);
        for (mode, tone, old_ratio) in [(LIGHT, 70, 2.24), (DARK, 30, 1.84)] {
            let surface = resolve_token(&tokens, "--color-background-surface", mode);
            let old = at(&nv, tone);
            let new = resolve_token(&tokens, "--color-border-emphasized", mode);
            assert!(
                close_to_one_digit(ratio(old, surface), old_ratio),
                "{mode:?}"
            );
            assert!(ratio(old, surface) < 3.0);
            assert_ne!(new, old);
            assert!(ratio(new, surface) >= 3.0);
        }
    }
}
