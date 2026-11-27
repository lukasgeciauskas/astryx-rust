//! Type scale expansion: upstream `theme/expandTypeScale.ts`.
//!
//! A base size and a ratio expand into the typography tokens by a geometric
//! progression, size = base × ratio^step, in two layers:
//!
//! 1. The raw sizes, `--font-size-4xs` (step -5) through `--font-size-5xl` (step
//!    +6), in rem on a 16px root. `--font-size-base` (step 0) carries h4, body,
//!    label and code; `--font-size-2xl` (step +3) carries h1.
//! 2. The semantic tokens, `--text-heading-{1..6}-*` and `--text-{type}-*`: each
//!    size is a `var()` of its raw size, each line height a unitless ratio
//!    computed for the size and snapped to the 4px grid, each weight a `var()`
//!    of a font-weight token.
//!
//! The line height aims at 1.5 below 20px, 1.4 from 20px to 31px and 1.25 from
//! 32px up, rounded to a multiple of 4px and at least the size + 4px. The named
//! leading tokens (`--leading-tight` … `--leading-relaxed`) are not touched.

use super::js::{js_number_to_string as js_number_string, js_round};

/// A font weight: a CSS value such as `600` or a reference such as
/// `var(--font-weight-bold)` (upstream `FontWeightValue`).
pub type FontWeightValue = String;

/// Weight overrides per heading level, 1 to 6 (upstream
/// `HeadingWeightOverrides`). A level listed twice takes its last weight; a
/// level left out keeps the default.
pub type HeadingWeightOverrides = Vec<(u8, FontWeightValue)>;

/// Weight overrides per text type (upstream `TextWeightOverrides`): the built-in
/// `body`, `large`, `label`, `code`, `supporting` and `display-1` to
/// `display-3`, or a theme's own type name. A type left out keeps the default.
pub type TextWeightOverrides = Vec<(String, FontWeightValue)>;

/// The weight overrides of a type scale (upstream `TypeScaleConfig['weights']`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TypeScaleWeights {
    /// Per-level heading weights. Unset levels use the defaults.
    pub heading: Option<HeadingWeightOverrides>,
    /// Per-type text weights. Unset types use the defaults.
    pub text: Option<TextWeightOverrides>,
}

/// A type scale (upstream `TypeScaleConfig`). Upstream suggests `{ base: 12,
/// ratio: 1.125 }` for a dense theme, `{ base: 14, ratio: 1.2 }` (Astryx's own)
/// and `{ base: 16, ratio: 1.25 }` for an airy, editorial one.
#[derive(Clone, Debug, PartialEq)]
pub struct TypeScaleConfig {
    /// The base font size in px, the size of h4 and body text.
    pub base: f64,
    /// The ratio of the geometric progression.
    pub ratio: f64,
    /// Weight overrides for headings and text types.
    pub weights: Option<TypeScaleWeights>,
}

impl TypeScaleConfig {
    /// A scale of `base` px at `ratio`, with the default weights.
    pub fn new(base: f64, ratio: f64) -> Self {
        Self {
            base,
            ratio,
            weights: None,
        }
    }
}

/// Generated typography tokens (upstream `TypeScaleTokens`): CSS custom property
/// names and their CSS values, in upstream's order.
pub type TypeScaleTokens = Vec<(String, String)>;

/// One component's generated rules: a visual-prop key (`level:1`, `type:body`)
/// and its CSS properties, camelCase as upstream's style objects name them
/// (`fontSize`), with their values.
pub type TypeScaleRules = Vec<(String, Vec<(String, String)>)>;

/// The component rules [`generate_type_scale_components`] writes: a component
/// name (`heading`, `text`) and its rules (upstream's `Record<string,
/// Record<string, Record<string, string>>>`, the shape of a theme's
/// `components`).
pub type TypeScaleComponents = Vec<(String, TypeScaleRules)>;

/// Each step's raw size token, steps -5 to +6 (upstream `STEP_TO_SIZE_TOKEN`).
const STEP_TO_SIZE_TOKEN: [(i32, &str); 12] = [
    (-5, "--font-size-4xs"),
    (-4, "--font-size-3xs"),
    (-3, "--font-size-2xs"),
    (-2, "--font-size-xs"),
    (-1, "--font-size-sm"),
    (0, "--font-size-base"),
    (1, "--font-size-lg"),
    (2, "--font-size-xl"),
    (3, "--font-size-2xl"),
    (4, "--font-size-3xl"),
    (5, "--font-size-4xl"),
    (6, "--font-size-5xl"),
];

/// Each heading level's step from the base (upstream `HEADING_STEPS`): h1 is
/// three steps up, h4 the base, h6 two steps down.
const HEADING_STEPS: [(u8, i32); 6] = [(1, 3), (2, 2), (3, 1), (4, 0), (5, -1), (6, -2)];

/// Each text type's step from the base (upstream `TEXT_STEPS`): the display
/// types continue the progression above h1, `display-1` the largest.
const TEXT_STEPS: [(&str, i32); 8] = [
    ("body", 0),
    ("large", 1),
    ("label", 0),
    ("code", 0),
    ("supporting", -1),
    ("display-1", 6),
    ("display-2", 5),
    ("display-3", 4),
];

/// The default weight of every heading level (upstream
/// `DEFAULT_HEADING_WEIGHTS`).
const DEFAULT_HEADING_WEIGHT: &str = "var(--font-weight-semibold)";

/// The default weight of each text type (upstream `DEFAULT_TEXT_WEIGHTS`).
const DEFAULT_TEXT_WEIGHTS: [(&str, &str); 8] = [
    ("body", "var(--font-weight-normal)"),
    ("large", "var(--font-weight-semibold)"),
    ("label", "var(--font-weight-medium)"),
    ("code", "var(--font-weight-normal)"),
    ("supporting", "var(--font-weight-normal)"),
    ("display-1", "var(--font-weight-normal)"),
    ("display-2", "var(--font-weight-normal)"),
    ("display-3", "var(--font-weight-normal)"),
];

/// The font family of each text type's rule (upstream `TEXT_FONT_FAMILIES`).
const TEXT_FONT_FAMILIES: [(&str, &str); 8] = [
    ("body", "var(--font-family-body)"),
    ("large", "var(--font-family-body)"),
    ("label", "var(--font-family-body)"),
    ("code", "var(--font-family-code)"),
    ("supporting", "var(--font-family-body)"),
    ("display-1", "var(--font-family-heading)"),
    ("display-2", "var(--font-family-heading)"),
    ("display-3", "var(--font-family-heading)"),
];

/// The raw size token of `step`.
fn size_token(step: i32) -> &'static str {
    STEP_TO_SIZE_TOKEN
        .iter()
        .find(|(s, _)| *s == step)
        .map(|(_, token)| *token)
        .expect("every step of the type scale has a size token")
}

/// The font size at `step`, base × ratio^step rounded to the px (upstream
/// `computeSize`).
fn compute_size(base: f64, ratio: f64, step: i32) -> f64 {
    js_round(base * ratio.powf(f64::from(step)))
}

/// `px` in rem on the standard 16px root, to four decimals (upstream
/// `pxToRem`): `14` is `0.875rem`.
fn px_to_rem(px: f64) -> String {
    let rem = js_round((px / 16.0) * 10000.0) / 10000.0;
    format!("{}rem", js_number_string(rem))
}

/// The line height a size aims at (upstream `targetLeadingRatio`): 1.5 below
/// 20px (body text, small UI), 1.4 below 32px (medium headings), 1.25 from 32px
/// (large display headings).
fn target_leading_ratio(font_size: f64) -> f64 {
    if font_size < 20.0 {
        1.5
    } else if font_size < 32.0 {
        1.4
    } else {
        1.25
    }
}

/// The unitless line height of `font_size`, to four decimals (upstream
/// `computeLeading`): the target ratio's line height rounded to a multiple of
/// 4px, and at least the size + 4px rounded up to one.
fn compute_leading(font_size: f64) -> f64 {
    let target = target_leading_ratio(font_size);
    let raw_lh = font_size * target;
    let snapped_lh = (js_round(raw_lh / 4.0) * 4.0).max(((font_size + 4.0) / 4.0).ceil() * 4.0);
    js_round((snapped_lh / font_size) * 10000.0) / 10000.0
}

/// The typography tokens of `config` (upstream `expandTypeScale`): the twelve
/// raw sizes in rem, then each heading level's and each text type's size,
/// weight and line height, 54 tokens in all.
///
/// `{ base: 14, ratio: 1.2 }` gives `--font-size-base: 0.875rem`,
/// `--font-size-2xl: 1.5rem`, `--text-heading-1-size: var(--font-size-2xl)`,
/// `--text-heading-1-leading: 1.3333` and `--text-body-leading: 1.4286`.
pub fn expand_type_scale(config: &TypeScaleConfig) -> TypeScaleTokens {
    let TypeScaleConfig {
        base,
        ratio,
        ref weights,
    } = *config;
    let mut tokens = TypeScaleTokens::with_capacity(54);

    // The overrides over the defaults; a key listed twice takes its last value.
    let heading_overrides = weights
        .as_ref()
        .and_then(|weights| weights.heading.as_ref());
    let heading_weight = |level: u8| -> String {
        heading_overrides
            .and_then(|overrides| overrides.iter().rev().find(|(l, _)| *l == level))
            .map_or(DEFAULT_HEADING_WEIGHT, |(_, weight)| weight.as_str())
            .to_string()
    };
    let text_overrides = weights.as_ref().and_then(|weights| weights.text.as_ref());
    let text_weight = |kind: &str| -> String {
        text_overrides
            .and_then(|overrides| overrides.iter().rev().find(|(k, _)| k == kind))
            .map(|(_, weight)| weight.as_str())
            .or_else(|| {
                DEFAULT_TEXT_WEIGHTS
                    .iter()
                    .find(|(k, _)| *k == kind)
                    .map(|(_, weight)| *weight)
            })
            .unwrap_or_default()
            .to_string()
    };

    // Layer 1: the raw sizes, in rem.
    for (step, token) in STEP_TO_SIZE_TOKEN {
        let size = compute_size(base, ratio, step);
        tokens.push((token.to_string(), px_to_rem(size)));
    }

    // Layer 2: the semantic tokens. Sizes are references to layer 1, line
    // heights computed values on the 4px grid, weights references.
    for (level, step) in HEADING_STEPS {
        let size = compute_size(base, ratio, step);
        let leading = compute_leading(size);
        tokens.push((
            format!("--text-heading-{level}-size"),
            format!("var({})", size_token(step)),
        ));
        tokens.push((
            format!("--text-heading-{level}-weight"),
            heading_weight(level),
        ));
        tokens.push((
            format!("--text-heading-{level}-leading"),
            js_number_string(leading),
        ));
    }

    for (kind, step) in TEXT_STEPS {
        let size = compute_size(base, ratio, step);
        let leading = compute_leading(size);
        tokens.push((
            format!("--text-{kind}-size"),
            format!("var({})", size_token(step)),
        ));
        tokens.push((format!("--text-{kind}-weight"), text_weight(kind)));
        tokens.push((format!("--text-{kind}-leading"), js_number_string(leading)));
    }

    tokens
}

/// The heading and text component rules a type scale gives a theme (upstream
/// `generateTypeScaleComponents`; like upstream it reads nothing of the
/// config). Each rule points the component's font family, size, line height
/// and, for heading levels, weight at the semantic tokens.
///
/// `heading` lists `level:1` to `level:6`, then `type:display-1` to
/// `type:display-3`: a heading renders both a level and a type class, the type's
/// sizing wins, and rules of equal specificity resolve by order, so the type
/// rules come last. They leave out the weight, as the `text` rules do.
pub fn generate_type_scale_components(_config: &TypeScaleConfig) -> TypeScaleComponents {
    let rule = |properties: &[(&str, String)]| -> Vec<(String, String)> {
        properties
            .iter()
            .map(|(property, value)| (property.to_string(), value.clone()))
            .collect()
    };

    let mut heading_rules = TypeScaleRules::new();
    for level in 1..=6 {
        heading_rules.push((
            format!("level:{level}"),
            rule(&[
                ("fontFamily", "var(--font-family-heading)".to_string()),
                ("fontSize", format!("var(--text-heading-{level}-size)")),
                ("fontWeight", format!("var(--text-heading-{level}-weight)")),
                ("lineHeight", format!("var(--text-heading-{level}-leading)")),
            ]),
        ));
    }
    for kind in ["display-1", "display-2", "display-3"] {
        heading_rules.push((
            format!("type:{kind}"),
            rule(&[
                ("fontFamily", "var(--font-family-heading)".to_string()),
                ("fontSize", format!("var(--text-{kind}-size)")),
                ("lineHeight", format!("var(--text-{kind}-leading)")),
            ]),
        ));
    }

    let mut text_rules = TypeScaleRules::new();
    for (kind, family) in TEXT_FONT_FAMILIES {
        text_rules.push((
            format!("type:{kind}"),
            rule(&[
                ("fontFamily", family.to_string()),
                ("fontSize", format!("var(--text-{kind}-size)")),
                ("lineHeight", format!("var(--text-{kind}-leading)")),
            ]),
        ));
    }

    vec![
        ("heading".to_string(), heading_rules),
        ("text".to_string(), text_rules),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn get<'a>(tokens: &'a TypeScaleTokens, name: &str) -> Option<&'a str> {
        tokens
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }

    fn default_scale() -> TypeScaleTokens {
        expand_type_scale(&TypeScaleConfig::new(14.0, 1.2))
    }

    /// `parseFloat` of a leading token.
    fn leading(tokens: &TypeScaleTokens, name: &str) -> f64 {
        get(tokens, name).unwrap().parse().unwrap()
    }

    fn rules<'a>(components: &'a TypeScaleComponents, name: &str) -> &'a TypeScaleRules {
        &components.iter().find(|(key, _)| key == name).unwrap().1
    }

    fn rule<'a>(rules: &'a TypeScaleRules, key: &str) -> Option<&'a Vec<(String, String)>> {
        rules.iter().find(|(k, _)| k == key).map(|(_, rule)| rule)
    }

    fn property<'a>(rule: &'a [(String, String)], name: &str) -> Option<&'a str> {
        rule.iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }

    // Upstream expandTypeScale.test.ts, Layer 1: "emits 11 raw size tokens in
    // rem" (it lists twelve, through --font-size-5xl).
    #[test]
    fn raw_sizes_are_in_rem() {
        let tokens = default_scale();
        for name in [
            "--font-size-4xs",
            "--font-size-3xs",
            "--font-size-2xs",
            "--font-size-xs",
            "--font-size-sm",
            "--font-size-base",
            "--font-size-lg",
            "--font-size-xl",
            "--font-size-2xl",
            "--font-size-3xl",
            "--font-size-4xl",
            "--font-size-5xl",
        ] {
            assert!(get(&tokens, name).unwrap().ends_with("rem"), "{name}");
        }
    }

    // Upstream: "anchors --font-size-base to the base size".
    #[test]
    fn base_size_anchors_the_scale() {
        assert_eq!(get(&default_scale(), "--font-size-base"), Some("0.875rem"));
    }

    // Upstream: "computes geometric progression for sub-scale tokens" and
    // "computes geometric progression".
    #[test]
    fn raw_sizes_follow_the_geometric_progression() {
        let tokens = default_scale();
        assert_eq!(get(&tokens, "--font-size-4xs"), Some("0.375rem")); // 6/16
        assert_eq!(get(&tokens, "--font-size-3xs"), Some("0.4375rem")); // 7/16
        assert_eq!(get(&tokens, "--font-size-2xs"), Some("0.5rem")); // 8/16
        assert_eq!(get(&tokens, "--font-size-xs"), Some("0.625rem")); // 10/16
        assert_eq!(get(&tokens, "--font-size-sm"), Some("0.75rem")); // 12/16
        assert_eq!(get(&tokens, "--font-size-lg"), Some("1.0625rem")); // 17/16
        assert_eq!(get(&tokens, "--font-size-xl"), Some("1.25rem")); // 20/16
        assert_eq!(get(&tokens, "--font-size-2xl"), Some("1.5rem")); // 24/16
        assert_eq!(get(&tokens, "--font-size-3xl"), Some("1.8125rem")); // 29/16
        assert_eq!(get(&tokens, "--font-size-4xl"), Some("2.1875rem")); // 35/16
    }

    // Upstream Layer 2: "heading sizes are var() references to raw tokens".
    #[test]
    fn heading_sizes_reference_raw_sizes() {
        let tokens = default_scale();
        assert_eq!(
            get(&tokens, "--text-heading-1-size"),
            Some("var(--font-size-2xl)")
        );
        assert_eq!(
            get(&tokens, "--text-heading-2-size"),
            Some("var(--font-size-xl)")
        );
        assert_eq!(
            get(&tokens, "--text-heading-3-size"),
            Some("var(--font-size-lg)")
        );
        assert_eq!(
            get(&tokens, "--text-heading-4-size"),
            Some("var(--font-size-base)")
        );
        assert_eq!(
            get(&tokens, "--text-heading-5-size"),
            Some("var(--font-size-sm)")
        );
        assert_eq!(
            get(&tokens, "--text-heading-6-size"),
            Some("var(--font-size-xs)")
        );
    }

    // Upstream: "heading leadings are hardcoded computed values".
    #[test]
    fn heading_leadings_are_computed() {
        let tokens = default_scale();
        assert_eq!(get(&tokens, "--text-heading-1-leading"), Some("1.3333")); // 24px → 32px
        assert_eq!(get(&tokens, "--text-heading-2-leading"), Some("1.4")); // 20px → 28px
        assert_eq!(get(&tokens, "--text-heading-3-leading"), Some("1.4118")); // 17px → 24px
        assert_eq!(get(&tokens, "--text-heading-4-leading"), Some("1.4286")); // 14px → 20px
        assert_eq!(get(&tokens, "--text-heading-5-leading"), Some("1.6667")); // 12px → 20px
        assert_eq!(get(&tokens, "--text-heading-6-leading"), Some("1.6")); // 10px → 16px
    }

    // Upstream: "text type sizes are var() references".
    #[test]
    fn text_sizes_reference_raw_sizes() {
        let tokens = default_scale();
        assert_eq!(
            get(&tokens, "--text-body-size"),
            Some("var(--font-size-base)")
        );
        assert_eq!(
            get(&tokens, "--text-large-size"),
            Some("var(--font-size-lg)")
        );
        assert_eq!(
            get(&tokens, "--text-label-size"),
            Some("var(--font-size-base)")
        );
        assert_eq!(
            get(&tokens, "--text-code-size"),
            Some("var(--font-size-base)")
        );
        assert_eq!(
            get(&tokens, "--text-supporting-size"),
            Some("var(--font-size-sm)")
        );
    }

    // Upstream: "text type leadings are hardcoded computed values".
    #[test]
    fn text_leadings_are_computed() {
        let tokens = default_scale();
        assert_eq!(get(&tokens, "--text-body-leading"), Some("1.4286")); // 14px → 20px
        assert_eq!(get(&tokens, "--text-large-leading"), Some("1.4118")); // 17px → 24px
        assert_eq!(get(&tokens, "--text-label-leading"), Some("1.4286"));
        assert_eq!(get(&tokens, "--text-code-leading"), Some("1.4286"));
        assert_eq!(get(&tokens, "--text-supporting-leading"), Some("1.6667")); // 12px → 20px
    }

    // Upstream: "weight tokens are var() references (unchanged)".
    #[test]
    fn weights_reference_font_weight_tokens() {
        let tokens = default_scale();
        let semibold = Some("var(--font-weight-semibold)");
        let normal = Some("var(--font-weight-normal)");
        assert_eq!(get(&tokens, "--text-heading-1-weight"), semibold);
        assert_eq!(get(&tokens, "--text-body-weight"), normal);
        assert_eq!(get(&tokens, "--text-large-weight"), semibold);
        assert_eq!(
            get(&tokens, "--text-label-weight"),
            Some("var(--font-weight-medium)")
        );
        assert_eq!(get(&tokens, "--text-code-weight"), normal);
        assert_eq!(get(&tokens, "--text-supporting-weight"), normal);
    }

    // Upstream: "computes display sizes continuing the progression above h1".
    #[test]
    fn display_sizes_continue_above_h1() {
        let tokens = default_scale();
        // 14 × 1.2⁶ ≈ 41.80 → 42px, 14 × 1.2⁵ ≈ 34.84 → 35px, 14 × 1.2⁴ ≈ 29.03 → 29px.
        assert_eq!(
            get(&tokens, "--text-display-1-size"),
            Some("var(--font-size-5xl)")
        );
        assert_eq!(
            get(&tokens, "--text-display-2-size"),
            Some("var(--font-size-4xl)")
        );
        assert_eq!(
            get(&tokens, "--text-display-3-size"),
            Some("var(--font-size-3xl)")
        );
        assert_eq!(get(&tokens, "--font-size-5xl"), Some("2.625rem"));
    }

    // Upstream: "assigns normal weight to display types by default".
    #[test]
    fn display_types_default_to_normal_weight() {
        let tokens = default_scale();
        for kind in ["display-1", "display-2", "display-3"] {
            assert_eq!(
                get(&tokens, &format!("--text-{kind}-weight")),
                Some("var(--font-weight-normal)")
            );
        }
    }

    // Upstream: "all line heights snap to 4px grid" and "uses tiered target
    // ratio based on font size".
    #[test]
    fn line_heights_snap_to_the_4px_grid() {
        let tokens = default_scale();
        let pairs = [
            ("--text-heading-1-leading", 24.0),
            ("--text-heading-2-leading", 20.0),
            ("--text-heading-3-leading", 17.0),
            ("--text-heading-4-leading", 14.0),
            ("--text-heading-5-leading", 12.0),
            ("--text-heading-6-leading", 10.0),
            ("--text-body-leading", 14.0),
            ("--text-large-leading", 17.0),
            ("--text-supporting-leading", 12.0),
        ];
        for (name, font_size) in pairs {
            let lh_px = js_round(font_size * leading(&tokens, name));
            assert_eq!(lh_px % 4.0, 0.0, "{name}");
            assert!(lh_px >= font_size + 4.0, "{name}");
        }
        // Below 20px the target is 1.5, from 20px to 31px 1.4.
        assert_eq!(
            js_round(14.0 * leading(&tokens, "--text-heading-4-leading")),
            20.0
        );
        assert_eq!(
            js_round(20.0 * leading(&tokens, "--text-heading-2-leading")),
            28.0
        );
        assert_eq!(
            js_round(24.0 * leading(&tokens, "--text-heading-1-leading")),
            32.0
        );
    }

    // Upstream: "does not emit --leading-* tokens".
    #[test]
    fn named_leading_tokens_are_left_alone() {
        assert!(
            !default_scale()
                .iter()
                .any(|(name, _)| name.starts_with("--leading-"))
        );
    }

    // Upstream: "generates 54 tokens (12 size + 42 semantic)".
    #[test]
    fn writes_54_tokens() {
        assert_eq!(default_scale().len(), 54);
    }

    // Upstream: "applies heading weight overrides" and "applies text weight
    // overrides".
    #[test]
    fn weight_overrides_replace_only_their_own_defaults() {
        let headed = expand_type_scale(&TypeScaleConfig {
            weights: Some(TypeScaleWeights {
                heading: Some(vec![(1, "var(--font-weight-bold)".to_string())]),
                text: None,
            }),
            ..TypeScaleConfig::new(14.0, 1.2)
        });
        assert_eq!(
            get(&headed, "--text-heading-1-weight"),
            Some("var(--font-weight-bold)")
        );
        assert_eq!(
            get(&headed, "--text-heading-2-weight"),
            Some("var(--font-weight-semibold)")
        );

        let texted = expand_type_scale(&TypeScaleConfig {
            weights: Some(TypeScaleWeights {
                heading: None,
                text: Some(vec![(
                    "large".to_string(),
                    "var(--font-weight-normal)".to_string(),
                )]),
            }),
            ..TypeScaleConfig::new(14.0, 1.2)
        });
        assert_eq!(
            get(&texted, "--text-large-weight"),
            Some("var(--font-weight-normal)")
        );
        assert_eq!(
            get(&texted, "--text-body-weight"),
            Some("var(--font-weight-normal)")
        );
    }

    // Upstream: "dense scale (base=12, ratio=1.125)" and "airy scale (base=16,
    // ratio=1.25)".
    #[test]
    fn alternate_scales_anchor_on_their_base() {
        let dense = expand_type_scale(&TypeScaleConfig::new(12.0, 1.125));
        assert_eq!(get(&dense, "--font-size-base"), Some("0.75rem"));
        assert_eq!(
            get(&dense, "--text-heading-4-size"),
            Some("var(--font-size-base)")
        );
        let airy = expand_type_scale(&TypeScaleConfig::new(16.0, 1.25));
        assert_eq!(get(&airy, "--font-size-base"), Some("1rem"));
        assert_eq!(get(&airy, "--font-size-lg"), Some("1.25rem"));
        assert_eq!(
            get(&airy, "--text-heading-1-size"),
            Some("var(--font-size-2xl)")
        );
    }

    // Upstream: "all scales produce 4px-grid-aligned line heights".
    #[test]
    fn every_scale_keeps_heading_line_heights_on_the_grid() {
        for (base, ratio) in [(12.0, 1.125), (14.0, 1.2), (16.0, 1.25), (18.0, 1.333)] {
            let tokens = expand_type_scale(&TypeScaleConfig::new(base, ratio));
            for (level, step) in HEADING_STEPS {
                let font_size = js_round(base * f64::powf(ratio, f64::from(step)));
                let line_ratio = leading(&tokens, &format!("--text-heading-{level}-leading"));
                let lh_px = js_round(font_size * line_ratio);
                assert_eq!(lh_px % 4.0, 0.0, "{base}/{ratio} h{level}");
                assert!(lh_px >= font_size + 4.0, "{base}/{ratio} h{level}");
            }
        }
    }

    // The shipped themes' scales, from `spec/themes/y2k.json` ({ base: 16, ratio:
    // 1.25 }) and `spec/themes/butter.json` ({ base: 14, ratio: 1.25 }):
    // upstream's resolved sizes in px and line heights.
    #[test]
    fn matches_the_shipped_themes() {
        let cases: [(f64, f64, [f64; 12], [&str; 9]); 2] = [
            (
                16.0,
                1.25,
                [5., 7., 8., 10., 13., 16., 20., 25., 31., 39., 49., 61.],
                [
                    "1.4194", "1.44", "1.4", "1.5", "1.5385", "1.6", "1.2459", "1.2245", "1.2308",
                ],
            ),
            (
                14.0,
                1.25,
                [5., 6., 7., 9., 11., 14., 18., 22., 27., 34., 43., 53.],
                [
                    "1.3333", "1.4545", "1.5556", "1.4286", "1.4545", "1.7778", "1.283", "1.2093",
                    "1.2941",
                ],
            ),
        ];
        for (base, ratio, sizes, leadings) in cases {
            let tokens = expand_type_scale(&TypeScaleConfig::new(base, ratio));
            for ((_, token), px) in STEP_TO_SIZE_TOKEN.iter().zip(sizes) {
                assert_eq!(get(&tokens, token), Some(px_to_rem(px).as_str()), "{token}");
            }
            let names = (1..=6)
                .map(|level| format!("--text-heading-{level}-leading"))
                .chain((1..=3).map(|n| format!("--text-display-{n}-leading")));
            for (name, expected) in names.zip(leadings) {
                assert_eq!(get(&tokens, &name), Some(expected), "{base}/{ratio} {name}");
            }
        }
    }

    // Order, read from the TS: the raw sizes from step -5 up, then size, weight
    // and leading per heading level, then per text type.
    #[test]
    fn tokens_keep_upstream_order() {
        let tokens = default_scale();
        let names: Vec<&str> = tokens.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(names[0], "--font-size-4xs");
        assert_eq!(names[11], "--font-size-5xl");
        assert_eq!(
            &names[12..15],
            [
                "--text-heading-1-size",
                "--text-heading-1-weight",
                "--text-heading-1-leading"
            ]
        );
        assert_eq!(names[30], "--text-body-size");
        assert_eq!(names[53], "--text-display-3-leading");
    }

    // Upstream generateTypeScaleComponents tests: "generates heading and text
    // component keys", "generates rules for all 6 heading levels", "generates
    // rules for all 8 text types (including display)" and "generates rules for
    // all 3 heading display types".
    #[test]
    fn components_cover_every_level_and_type() {
        let components = generate_type_scale_components(&TypeScaleConfig::new(14.0, 1.2));
        let names: Vec<&str> = components.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(names, ["heading", "text"]);
        let heading = rules(&components, "heading");
        for level in 1..=6 {
            assert!(
                rule(heading, &format!("level:{level}")).is_some(),
                "level:{level}"
            );
        }
        for kind in ["display-1", "display-2", "display-3"] {
            assert!(
                rule(heading, &format!("type:{kind}")).is_some(),
                "type:{kind}"
            );
        }
        let text = rules(&components, "text");
        for kind in [
            "body",
            "large",
            "label",
            "code",
            "supporting",
            "display-1",
            "display-2",
            "display-3",
        ] {
            assert!(rule(text, &format!("type:{kind}")).is_some(), "type:{kind}");
        }
    }

    // Upstream: "heading rules include fontFamily, fontSize, fontWeight,
    // lineHeight", "heading overrides reference semantic tokens" and "text
    // overrides reference semantic tokens".
    #[test]
    fn component_rules_reference_the_semantic_tokens() {
        let components = generate_type_scale_components(&TypeScaleConfig::new(14.0, 1.2));
        let h1 = rule(rules(&components, "heading"), "level:1").unwrap();
        assert_eq!(
            property(h1, "fontFamily"),
            Some("var(--font-family-heading)")
        );
        assert_eq!(property(h1, "fontSize"), Some("var(--text-heading-1-size)"));
        assert_eq!(
            property(h1, "fontWeight"),
            Some("var(--text-heading-1-weight)")
        );
        assert_eq!(
            property(h1, "lineHeight"),
            Some("var(--text-heading-1-leading)")
        );
        let body = rule(rules(&components, "text"), "type:body").unwrap();
        assert_eq!(property(body, "fontSize"), Some("var(--text-body-size)"));
        assert_eq!(
            property(body, "lineHeight"),
            Some("var(--text-body-leading)")
        );
        let code = rule(rules(&components, "text"), "type:code").unwrap();
        assert_eq!(
            property(code, "fontFamily"),
            Some("var(--font-family-code)")
        );
    }

    // Upstream: "heading display-type rules reference the display semantic
    // tokens".
    #[test]
    fn heading_display_rules_leave_out_the_weight() {
        let components = generate_type_scale_components(&TypeScaleConfig::new(14.0, 1.2));
        let display1 = rule(rules(&components, "heading"), "type:display-1").unwrap();
        assert_eq!(
            property(display1, "fontFamily"),
            Some("var(--font-family-heading)")
        );
        assert_eq!(
            property(display1, "fontSize"),
            Some("var(--text-display-1-size)")
        );
        assert_eq!(
            property(display1, "lineHeight"),
            Some("var(--text-display-1-leading)")
        );
        assert_eq!(property(display1, "fontWeight"), None);
    }

    // Upstream: "emits heading type:display-N rules after level:N rules, so type
    // wins at equal specificity".
    #[test]
    fn heading_type_rules_follow_the_level_rules() {
        let components = generate_type_scale_components(&TypeScaleConfig::new(14.0, 1.2));
        let keys: Vec<&str> = rules(&components, "heading")
            .iter()
            .map(|(key, _)| key.as_str())
            .collect();
        let index = |key: &str| keys.iter().position(|k| *k == key).unwrap();
        let last_level = (1..=6)
            .map(|level| index(&format!("level:{level}")))
            .max()
            .unwrap();
        let first_type = ["display-1", "display-2", "display-3"]
            .iter()
            .map(|kind| index(&format!("type:{kind}")))
            .min()
            .unwrap();
        assert!(first_type > last_level);
    }
}
