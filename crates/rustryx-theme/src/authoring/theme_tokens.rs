//! A defined theme's tokens resolved per mode: upstream `theme/tokens.ts`.
//!
//! [`resolve_theme_tokens`] gives every Astryx token's concrete value in one
//! mode, as upstream's `useTheme()` reads it without React: core's defaults,
//! then the theme's derived status roles and its own tokens, with `var()`
//! references followed and the CSS color functions the generator writes
//! evaluated, so a derived token such as `--color-text-accent` comes back as a
//! color, not `var(--color-accent)`. [`token_var`] and [`token_vars`] build
//! the `var()` references that stay connected to the cascade.

use std::collections::HashSet;

use super::TOKEN_DEFAULTS;
use super::define::{CssMap, DefinedTheme};
use super::status_roles::generated_scoped_status_role_defaults;
use super::token_references::resolve_token_references_with;
use super::token_values::{
    ResolvedThemeMode, evaluate_color_functions, resolve_default_tokens, resolve_xds_token_value,
};

/// A CSS custom property reference to a token: `var(--name)` (upstream
/// `tokenVar`). Any name works, an app's own included.
pub fn token_var(name: &str) -> String {
    format!("var({name})")
}

/// Every Astryx token name and its [`token_var`] reference, in
/// [`TOKEN_DEFAULTS`] order (upstream `tokenVars`).
pub fn token_vars() -> CssMap {
    TOKEN_DEFAULTS
        .iter()
        .map(|(name, _)| (*name, token_var(name)))
        .collect()
}

/// Every Astryx token's concrete value for `theme` in `mode` (upstream
/// `resolveThemeTokens`); core's defaults alone for no theme.
///
/// The values start from [`TOKEN_DEFAULTS`] in `mode`, then take the theme's
/// local tokens, the status roles it leaves unset as its stylesheet declares
/// them ([`generated_scoped_status_role_defaults`], so script and CSS agree),
/// its tokens, and its input tokens, so a `[light, dark]` tuple keeps its own
/// sides. Then every `var()` is followed and every `srgb` `color-mix()`
/// evaluated ([`resolve_token_references_with`],
/// [`evaluate_color_functions`]). The local tokens resolve the references to
/// them and are left out: they are the theme's names, not Astryx tokens.
///
/// The map lists tokens in the order their values resolved, upstream's key
/// order.
pub fn resolve_theme_tokens(theme: Option<&DefinedTheme>, mode: ResolvedThemeMode) -> CssMap {
    let Some(theme) = theme else {
        return resolve_default_tokens(mode).into_iter().collect();
    };
    let mut resolved = CssMap::new();
    for &(key, value) in TOKEN_DEFAULTS {
        resolved.insert(key, resolve_xds_token_value(value, mode).to_string());
    }

    let empty = CssMap::new();
    let local_tokens = theme.local_tokens.as_ref().unwrap_or(&empty);
    let local_keys: HashSet<&str> = local_tokens.keys().collect();
    for (key, value) in local_tokens.iter() {
        resolved.insert(key, resolve_xds_token_value(value, mode).to_string());
    }

    // Status roles the theme leaves unset resolve as its generated root CSS
    // declares them (architecture:theme-compilation INV16 and INV17). Upstream
    // passes no adaptation rules here: script reads the root's values, and the
    // rules' token writes reach the role defaults only in the stylesheet
    // compiler (`adaptations::adaptation_token_writes`).
    for (key, value) in generated_scoped_status_role_defaults(&theme.tokens, local_tokens, &[]) {
        let value = resolve_xds_token_value(&value, mode).to_string();
        resolved.insert(key, value);
    }

    for (key, value) in theme.tokens.iter() {
        resolved.insert(key, resolve_xds_token_value(value, mode).to_string());
    }

    for (key, value) in theme.input_tokens.iter().flat_map(|tokens| tokens.iter()) {
        if let Some(value) = value.value() {
            resolved.insert(key, resolve_xds_token_value(value, mode).to_string());
        }
    }

    resolve_token_references_with(resolved.as_slice(), evaluate_color_functions)
        .into_iter()
        .filter(|(key, _)| !local_keys.contains(key.as_str()))
        .collect()
}

/// One token's concrete value for `theme` in `mode` (upstream
/// `resolveThemeToken`): `fallback` for a name that is not a token, or the
/// empty string without one.
pub fn resolve_theme_token(
    theme: Option<&DefinedTheme>,
    name: &str,
    mode: ResolvedThemeMode,
    fallback: Option<&str>,
) -> String {
    resolve_theme_tokens(theme, mode)
        .get(name)
        .cloned()
        .or_else(|| fallback.map(str::to_string))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::super::color_scale::{AccentSeed, ColorScaleConfig};
    use super::super::define::{DefineThemeInput, TokenInput, define_theme};
    use super::*;

    const LIGHT: ResolvedThemeMode = ResolvedThemeMode::Light;
    const DARK: ResolvedThemeMode = ResolvedThemeMode::Dark;

    fn define<const N: usize>(name: &str, tokens: [(&str, TokenInput); N]) -> DefinedTheme {
        define_theme(&DefineThemeInput {
            tokens: Some(tokens.into()),
            ..DefineThemeInput::new(name)
        })
        .unwrap()
    }

    fn css(value: &str) -> TokenInput {
        TokenInput::Css(value.to_string())
    }

    fn pair(light: &str, dark: &str) -> TokenInput {
        TokenInput::Pair(light.to_string(), dark.to_string())
    }

    /// Upstream's `testTheme` (tokens.test.ts).
    fn test_theme() -> DefinedTheme {
        define(
            "test",
            [
                ("--color-accent", pair("#AA0000", "#FF5555")),
                (
                    "--color-neutral",
                    pair("rgba(5, 54, 89, 0.1)", "rgba(223, 226, 229, 0.2)"),
                ),
                ("--spacing-4", css("20px")),
            ],
        )
    }

    fn get(tokens: &CssMap, name: &str) -> String {
        tokens.get(name).cloned().unwrap_or_default()
    }

    #[test]
    fn token_var_references_any_token_name() {
        assert_eq!(
            token_var("--color-text-primary"),
            "var(--color-text-primary)"
        );
        assert_eq!(token_var("--app-custom-token"), "var(--app-custom-token)");
        let vars = token_vars();
        assert_eq!(
            get(&vars, "--color-text-primary"),
            "var(--color-text-primary)"
        );
        assert_eq!(get(&vars, "--spacing-4"), "var(--spacing-4)");
        assert_eq!(vars.len(), TOKEN_DEFAULTS.len());
    }

    #[test]
    fn resolves_the_defaults_without_a_theme() {
        let tokens = resolve_theme_tokens(None, LIGHT);
        assert_eq!(get(&tokens, "--color-text-primary"), "#0A1317");
        assert_eq!(get(&tokens, "--spacing-1"), "0.25rem");
        assert_eq!(get(&tokens, "--border-width"), "1px");
        // Focus outline defaults: the accent ring.
        assert_eq!(get(&tokens, "--focus-outline-width"), "2px");
        assert_eq!(get(&tokens, "--focus-outline-style"), "solid");
        assert_eq!(get(&tokens, "--focus-outline-offset"), "3px");
        assert_eq!(
            get(&tokens, "--focus-outline-color"),
            get(&tokens, "--color-accent")
        );
    }

    #[test]
    fn resolves_overrides_and_tuples_with_nested_commas_per_mode() {
        let arcade = define("arcade", [("--border-width", css("2px"))]);
        assert_eq!(
            get(
                &resolve_theme_tokens(Some(&arcade), LIGHT),
                "--border-width"
            ),
            "2px"
        );
        let theme = test_theme();
        let light = resolve_theme_tokens(Some(&theme), LIGHT);
        let dark = resolve_theme_tokens(Some(&theme), DARK);
        assert_eq!(get(&light, "--color-accent"), "#AA0000");
        assert_eq!(get(&dark, "--color-accent"), "#FF5555");
        assert_eq!(get(&light, "--color-neutral"), "rgba(5, 54, 89, 0.1)");
        assert_eq!(get(&dark, "--color-neutral"), "rgba(223, 226, 229, 0.2)");
        assert_eq!(get(&light, "--spacing-4"), "20px");
    }

    #[test]
    fn resolves_a_built_themes_light_dark_strings_without_input_tokens() {
        let mut built = DefinedTheme {
            name: "built".into(),
            built: true,
            ..DefinedTheme::default()
        };
        built.tokens.insert(
            "--color-neutral",
            "light-dark(rgba(5, 54, 89, 0.1), rgba(223, 226, 229, 0.2))".to_string(),
        );
        assert_eq!(
            get(
                &resolve_theme_tokens(Some(&built), LIGHT),
                "--color-neutral"
            ),
            "rgba(5, 54, 89, 0.1)"
        );
        assert_eq!(
            get(&resolve_theme_tokens(Some(&built), DARK), "--color-neutral"),
            "rgba(223, 226, 229, 0.2)"
        );
    }

    #[test]
    fn resolve_theme_token_reads_one_token_or_its_fallback() {
        let theme = test_theme();
        assert_eq!(
            resolve_theme_token(Some(&theme), "--color-accent", DARK, None),
            "#FF5555"
        );
        assert_eq!(
            resolve_theme_token(None, "--ease-exit", LIGHT, None),
            "cubic-bezier(0.3, 0, 0.6, 0.6)"
        );
        let snappy = define("snappy-exit", [("--ease-exit", css("ease-in"))]);
        assert_eq!(
            resolve_theme_token(Some(&snappy), "--ease-exit", DARK, None),
            "ease-in"
        );
        assert_eq!(
            resolve_theme_token(Some(&theme), "--missing-token", DARK, Some("fallback")),
            "fallback"
        );
        assert_eq!(
            resolve_theme_token(Some(&theme), "--missing-token", DARK, None),
            ""
        );
    }

    #[test]
    fn follows_references_to_raw_values() {
        let theme = define(
            "ref",
            [
                ("--color-accent", pair("#0058D2", "#BBC2FF")),
                ("--color-text-accent", css("var(--color-accent)")),
            ],
        );
        assert_eq!(
            get(
                &resolve_theme_tokens(Some(&theme), LIGHT),
                "--color-text-accent"
            ),
            "#0058D2"
        );
        assert_eq!(
            get(
                &resolve_theme_tokens(Some(&theme), DARK),
                "--color-text-accent"
            ),
            "#BBC2FF"
        );
        let chain = define(
            "chain",
            [
                ("--color-accent", pair("#112233", "#445566")),
                ("--color-icon-accent", css("var(--color-accent)")),
                ("--color-text-accent", css("var(--color-icon-accent)")),
            ],
        );
        let light = resolve_theme_tokens(Some(&chain), LIGHT);
        assert_eq!(get(&light, "--color-icon-accent"), "#112233");
        assert_eq!(get(&light, "--color-text-accent"), "#112233");
    }

    #[test]
    fn an_unknown_reference_takes_its_fallback_or_stays_literal_and_a_cycle_ends() {
        let fallback = define(
            "fallback",
            [("--color-text-accent", css("var(--not-a-token, #ABCDEF)"))],
        );
        assert_eq!(
            get(
                &resolve_theme_tokens(Some(&fallback), LIGHT),
                "--color-text-accent"
            ),
            "#ABCDEF"
        );
        let unresolvable = define(
            "unresolvable",
            [("--color-text-accent", css("var(--not-a-token)"))],
        );
        assert_eq!(
            get(
                &resolve_theme_tokens(Some(&unresolvable), LIGHT),
                "--color-text-accent"
            ),
            "var(--not-a-token)"
        );
        let cycle = define(
            "cycle",
            [
                ("--color-text-accent", css("var(--color-icon-accent)")),
                ("--color-icon-accent", css("var(--color-text-accent)")),
            ],
        );
        let tokens = resolve_theme_tokens(Some(&cycle), LIGHT);
        assert!(get(&tokens, "--color-text-accent").contains("var("));
        assert!(get(&tokens, "--color-icon-accent").contains("var("));
    }

    #[test]
    fn evaluates_srgb_color_mixes() {
        let mix = define(
            "mix",
            [
                ("--color-accent", pair("#0058D2", "#BBC2FF")),
                (
                    "--color-accent-muted",
                    css("color-mix(in srgb, var(--color-accent) 20%, transparent)"),
                ),
            ],
        );
        assert_eq!(
            get(
                &resolve_theme_tokens(Some(&mix), LIGHT),
                "--color-accent-muted"
            ),
            "rgba(0, 88, 210, 0.2)"
        );
        assert_eq!(
            get(
                &resolve_theme_tokens(Some(&mix), DARK),
                "--color-accent-muted"
            ),
            "rgba(187, 194, 255, 0.2)"
        );
        let per_mode = define(
            "ld-mix",
            [
                ("--color-accent", pair("#FF0000", "#00FF00")),
                (
                    "--color-accent-muted",
                    pair(
                        "color-mix(in srgb, var(--color-accent) 20%, transparent)",
                        "color-mix(in srgb, var(--color-accent) 25%, transparent)",
                    ),
                ),
            ],
        );
        assert_eq!(
            get(
                &resolve_theme_tokens(Some(&per_mode), LIGHT),
                "--color-accent-muted"
            ),
            "rgba(255, 0, 0, 0.2)"
        );
        assert_eq!(
            get(
                &resolve_theme_tokens(Some(&per_mode), DARK),
                "--color-accent-muted"
            ),
            "rgba(0, 255, 0, 0.25)"
        );
        let even = define(
            "two-color",
            [(
                "--color-neutral",
                css("color-mix(in srgb, #000000, #FFFFFF)"),
            )],
        );
        assert_eq!(
            get(&resolve_theme_tokens(Some(&even), LIGHT), "--color-neutral"),
            "#808080"
        );
        let weighted = define(
            "weighted",
            [(
                "--color-neutral",
                css("color-mix(in srgb, #000000 25%, #FFFFFF)"),
            )],
        );
        assert_eq!(
            get(
                &resolve_theme_tokens(Some(&weighted), LIGHT),
                "--color-neutral"
            ),
            "#BFBFBF"
        );
        let oklab = define(
            "oklab",
            [
                ("--color-accent", pair("#0058D2", "#BBC2FF")),
                (
                    "--color-neutral",
                    css("color-mix(in oklab, var(--color-accent), black 15%)"),
                ),
            ],
        );
        let neutral = get(
            &resolve_theme_tokens(Some(&oklab), LIGHT),
            "--color-neutral",
        );
        assert!(neutral.contains("color-mix(in oklab"), "{neutral}");
        assert!(neutral.contains("#0058D2"), "{neutral}");
    }

    #[test]
    fn resolves_a_generated_themes_derived_accent_tokens_to_raw_values() {
        let theme = define_theme(&DefineThemeInput {
            color: Some(ColorScaleConfig {
                accent: Some(AccentSeed::Single("#0064E0".into())),
                ..ColorScaleConfig::default()
            }),
            ..DefineThemeInput::new("brand")
        })
        .unwrap();
        let light = resolve_theme_tokens(Some(&theme), LIGHT);
        let dark = resolve_theme_tokens(Some(&theme), DARK);
        for key in [
            "--color-accent",
            "--color-text-accent",
            "--color-icon-accent",
            "--color-accent-muted",
        ] {
            for tokens in [&light, &dark] {
                let value = get(tokens, key);
                assert!(
                    value.starts_with('#') || value.starts_with("rgb"),
                    "{key}: {value}"
                );
            }
        }
        assert_eq!(
            get(&light, "--color-text-accent"),
            get(&light, "--color-accent")
        );
        assert_eq!(
            get(&light, "--color-icon-accent"),
            get(&light, "--color-accent")
        );
        // The focus ring follows the theme's accent.
        assert_eq!(
            get(&light, "--focus-outline-color"),
            get(&light, "--color-accent")
        );
    }

    #[test]
    fn an_explicit_focus_ring_wins_over_the_accent() {
        let theme = define_theme(&DefineThemeInput {
            color: Some(ColorScaleConfig {
                accent: Some(AccentSeed::Single("#AA0000".into())),
                ..ColorScaleConfig::default()
            }),
            tokens: Some(
                [
                    ("--focus-outline-color", css("#00FF00")),
                    ("--focus-outline-width", css("4px")),
                    ("--focus-outline-offset", css("0px")),
                ]
                .into(),
            ),
            ..DefineThemeInput::new("brand")
        })
        .unwrap();
        let tokens = resolve_theme_tokens(Some(&theme), LIGHT);
        assert_eq!(get(&tokens, "--focus-outline-color"), "#00FF00");
        assert_eq!(get(&tokens, "--focus-outline-width"), "4px");
        assert_eq!(get(&tokens, "--focus-outline-offset"), "0px");
    }

    #[test]
    fn an_unset_status_role_resolves_to_the_value_the_stylesheet_derives() {
        // INV17: the derived --color-text-success the root declares.
        let theme = define("derived-status", [("--color-success", css("#3FB950"))]);
        let roles = generated_scoped_status_role_defaults(&theme.tokens, &CssMap::new(), &[]);
        let emitted = roles
            .iter()
            .find(|(role, _)| role == "--color-text-success")
            .map(|(_, value)| value.clone())
            .unwrap();
        let (light, dark) = emitted
            .strip_prefix("light-dark(")
            .and_then(|v| v.strip_suffix(')'))
            .and_then(|v| v.split_once(", "))
            .unwrap();
        assert_eq!(
            resolve_theme_token(Some(&theme), "--color-text-success", LIGHT, None),
            light
        );
        assert_eq!(
            resolve_theme_token(Some(&theme), "--color-text-success", DARK, None),
            dark
        );
        // Icon and border follow the derived text role.
        assert_eq!(
            resolve_theme_token(Some(&theme), "--color-icon-success", LIGHT, None),
            light
        );
        let set = define(
            "set-status",
            [
                ("--color-success", css("#3FB950")),
                ("--color-text-success", css("#123456")),
            ],
        );
        assert_eq!(
            resolve_theme_token(Some(&set), "--color-text-success", LIGHT, None),
            "#123456"
        );
    }

    #[test]
    fn local_tokens_resolve_references_to_them_and_are_left_out() {
        let theme = define_theme(&DefineThemeInput {
            local_tokens: Some([("--brand", pair("#123456", "#654321"))].into()),
            tokens: Some([("--color-accent", css("var(--brand)"))].into()),
            ..DefineThemeInput::new("local")
        })
        .unwrap();
        let light = resolve_theme_tokens(Some(&theme), LIGHT);
        assert_eq!(get(&light, "--color-accent"), "#123456");
        assert!(!light.contains_key("--brand"));
        assert_eq!(
            get(&resolve_theme_tokens(Some(&theme), DARK), "--color-accent"),
            "#654321"
        );
    }
}
