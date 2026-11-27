//! Radius scale expansion: upstream `theme/expandRadiusScale.ts`.
//!
//! A base unit and a multiplier expand into the radius tokens:
//!
//! | Token | Value |
//! |---|---|
//! | `--radius-none` | `0px` (fixed) |
//! | `--radius-inner` | base × 1 × multiplier (internal corners) |
//! | `--radius-element` | base × 2 × multiplier (buttons, inputs) |
//! | `--radius-container` | base × 3 × multiplier (cards, panels) |
//! | `--radius-page` | base × 7 × multiplier (page-level containers) |
//! | `--radius-full` | `9999px` (fixed, pill shapes) |
//!
//! `--radius-chat` is not on the scale: it follows `--radius-container`.

use super::js::{js_number_to_string as js_number_string, js_round};

/// A radius scale (upstream `RadiusScaleConfig`), such as `{ base: 4,
/// multiplier: 1 }` for Astryx's own, `multiplier: 0` for a sharp theme where
/// every scalable radius is `0px`, or `multiplier: 1.5` for an extra-rounded
/// one.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RadiusScaleConfig {
    /// The base radius unit in px. Default 4.
    pub base: f64,
    /// The multiplier applied to the scalable tokens (inner through page).
    /// Default 1, range 0 to 2.
    pub multiplier: f64,
}

impl Default for RadiusScaleConfig {
    /// The defaults upstream documents: base 4, multiplier 1.
    fn default() -> Self {
        Self {
            base: 4.0,
            multiplier: 1.0,
        }
    }
}

/// Generated radius tokens (upstream `RadiusScaleTokens`): CSS custom property
/// names and their CSS values, in upstream's order.
pub type RadiusScaleTokens = Vec<(String, String)>;

/// The radius tokens of `config` (upstream `expandRadiusScale`).
/// `--radius-none` and `--radius-full` are fixed; `--radius-inner` through
/// `--radius-page` are base × step × multiplier, rounded to the px.
///
/// `{ base: 4, multiplier: 1 }` gives `0px`, `4px`, `8px`, `12px`, `28px` and
/// `9999px`.
pub fn expand_radius_scale(config: &RadiusScaleConfig) -> RadiusScaleTokens {
    let RadiusScaleConfig { base, multiplier } = *config;
    let scaled = |step: f64| format!("{}px", js_number_string(js_round(base * step * multiplier)));
    vec![
        ("--radius-none".to_string(), "0px".to_string()),
        ("--radius-inner".to_string(), scaled(1.0)),
        ("--radius-element".to_string(), scaled(2.0)),
        ("--radius-container".to_string(), scaled(3.0)),
        ("--radius-page".to_string(), scaled(7.0)),
        ("--radius-full".to_string(), "9999px".to_string()),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn get<'a>(tokens: &'a RadiusScaleTokens, name: &str) -> Option<&'a str> {
        tokens
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }

    fn expand(base: f64, multiplier: f64) -> RadiusScaleTokens {
        expand_radius_scale(&RadiusScaleConfig { base, multiplier })
    }

    // Upstream expandRadiusScale.test.ts: "generates default scale".
    #[test]
    fn default_scale_is_astryx_radii() {
        let tokens = expand_radius_scale(&RadiusScaleConfig::default());
        assert_eq!(get(&tokens, "--radius-none"), Some("0px"));
        assert_eq!(get(&tokens, "--radius-inner"), Some("4px"));
        assert_eq!(get(&tokens, "--radius-element"), Some("8px"));
        assert_eq!(get(&tokens, "--radius-container"), Some("12px"));
        assert_eq!(get(&tokens, "--radius-page"), Some("28px"));
        assert_eq!(get(&tokens, "--radius-chat"), None);
        assert_eq!(get(&tokens, "--radius-full"), Some("9999px"));
        assert_eq!(tokens.len(), 6);
    }

    // Upstream: "applies multiplier".
    #[test]
    fn multiplier_scales_the_scalable_radii() {
        let tokens = expand(4.0, 1.5);
        assert_eq!(get(&tokens, "--radius-inner"), Some("6px"));
        assert_eq!(get(&tokens, "--radius-element"), Some("12px"));
        assert_eq!(get(&tokens, "--radius-container"), Some("18px"));
        assert_eq!(get(&tokens, "--radius-page"), Some("42px"));
    }

    // Upstream: "multiplier 0 produces all zeros".
    #[test]
    fn multiplier_zero_makes_every_scalable_radius_zero() {
        let tokens = expand(4.0, 0.0);
        assert_eq!(get(&tokens, "--radius-none"), Some("0px"));
        assert_eq!(get(&tokens, "--radius-inner"), Some("0px"));
        assert_eq!(get(&tokens, "--radius-element"), Some("0px"));
        assert_eq!(get(&tokens, "--radius-container"), Some("0px"));
        assert_eq!(get(&tokens, "--radius-page"), Some("0px"));
        assert_eq!(get(&tokens, "--radius-full"), Some("9999px"));
    }

    // Upstream: "fixed tokens are unaffected by multiplier".
    #[test]
    fn fixed_radii_ignore_the_multiplier() {
        let tokens = expand(4.0, 2.0);
        assert_eq!(get(&tokens, "--radius-none"), Some("0px"));
        assert_eq!(get(&tokens, "--radius-full"), Some("9999px"));
    }

    // Upstream: "rounds fractional results to nearest integer".
    #[test]
    fn fractional_radii_round_to_the_px() {
        let tokens = expand(3.0, 1.3);
        // 3 × 1 × 1.3 = 3.9 → 4px; 3 × 2 × 1.3 = 7.8 → 8px.
        assert_eq!(get(&tokens, "--radius-inner"), Some("4px"));
        assert_eq!(get(&tokens, "--radius-element"), Some("8px"));
    }

    // Upstream: "works with non-standard base".
    #[test]
    fn non_standard_base_scales_every_step() {
        let tokens = expand(6.0, 1.0);
        assert_eq!(get(&tokens, "--radius-inner"), Some("6px"));
        assert_eq!(get(&tokens, "--radius-element"), Some("12px"));
        assert_eq!(get(&tokens, "--radius-container"), Some("18px"));
        assert_eq!(get(&tokens, "--radius-page"), Some("42px"));
    }

    // Expected values from reading the TS: `Math.round` rounds a half up, and
    // `${-0}` is `0`. 2.5 × 1 × 1 = 2.5 → 3px; -0.1 × 1 rounds to -0 → 0px.
    #[test]
    fn rounding_follows_javascript() {
        assert_eq!(get(&expand(2.5, 1.0), "--radius-inner"), Some("3px"));
        assert_eq!(get(&expand(-0.1, 1.0), "--radius-inner"), Some("0px"));
    }
}
