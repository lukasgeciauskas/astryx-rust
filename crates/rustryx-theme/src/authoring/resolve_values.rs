//! A theme layer's declarative values resolved into its flat token map and
//! component styles: upstream `theme/resolveThemeValues.ts`.
//!
//! One pipeline turns a theme's axes into concrete values, for
//! [`define_theme`](super::define::define_theme) over an optional base and for
//! an adaptation rule's own value. Inside one layer the precedence is, lowest
//! to highest: the seed (the base theme's resolved values), the color scale, the
//! type scale, the radius scale, motion, the font families, the syntax theme,
//! then the explicit `tokens`.

use super::color_scale::{ColorScaleConfig, expand_color_scale};
use super::define::{CssMap, TokenInputs};
use super::merge_components::{
    ComponentStyleMap, StyleValue, deep_merge_components, without_unset_components,
};
use super::radius_scale::{RadiusScaleConfig, expand_radius_scale};
use super::theme_motion::{
    EASE_DEFAULTS, MOTION_TEMPO_PROPERTY, MotionCharacter, ThemeMotionConfig, expand_theme_motion,
    pre_tempo_name,
};
use super::token_values::ThemeTokenValue;
use super::type_scale::{
    TypeScaleConfig, TypeScaleWeights, expand_type_scale, generate_type_scale_components,
};

// =============================================================================
// Typography config (upstream theme/types.ts)
// =============================================================================

/// A type scale's base size and ratio, as a typography config writes it
/// (upstream `TypographyConfig['scale']`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TypographyScale {
    /// The base font size in px.
    pub base: f64,
    /// The ratio of the geometric progression.
    pub ratio: f64,
}

/// One typography role: body, heading or code (upstream `TypographyRole`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TypographyRole {
    /// The primary font name.
    pub family: Option<String>,
    /// The CSS fallback stack written after the family.
    pub fallbacks: Option<String>,
    /// The role's default weight: `normal`, `medium`, `semibold`, `bold` or a
    /// raw CSS value such as `800` (upstream `FontWeight`).
    pub weight: Option<String>,
    /// Per-level weights, headings only: levels 1 to 6.
    pub weights: Option<Vec<(u8, String)>>,
}

/// A theme's typography: fonts, scale and weights (upstream
/// `TypographyConfig`). A heading with no `family` takes the body's whole font
/// list; with its own `family`, only its own `fallbacks`; `fallbacks` with no
/// `family` build nothing.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TypographyConfig {
    /// The type scale, which generates the text size tokens.
    pub scale: Option<TypographyScale>,
    /// Body text.
    pub body: Option<TypographyRole>,
    /// Headings.
    pub heading: Option<TypographyRole>,
    /// Code.
    pub code: Option<TypographyRole>,
}

// =============================================================================
// Inputs and outputs
// =============================================================================

/// The value-bearing axes of a theme layer (upstream `ThemeValuesInput`):
/// everything that resolves to tokens or component styles.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ThemeValuesInput {
    /// Fonts, scale and weights.
    pub typography: Option<TypographyConfig>,
    /// The color scale.
    pub color: Option<ColorScaleConfig>,
    /// The radius scale.
    pub radius: Option<RadiusScaleConfig>,
    /// Motion: the knobs, a duration scale, or both.
    pub motion: Option<ThemeMotionConfig>,
    /// The syntax theme's tokens, without the `--color-syntax-` prefix.
    pub syntax: Option<CssMap>,
    /// Explicit token overrides, the highest precedence.
    pub tokens: Option<TokenInputs>,
    /// Component style overrides.
    pub components: Option<ComponentStyleMap>,
}

/// Already-resolved values to start from, the lowest layer (upstream
/// `ThemeValuesSeed`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ThemeValuesSeed {
    /// Resolved token values.
    pub tokens: Option<CssMap>,
    /// Resolved component styles.
    pub components: Option<ComponentStyleMap>,
}

/// One theme layer, resolved (upstream `ResolvedThemeValues`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ResolvedThemeValues {
    /// Token CSS values.
    pub tokens: CssMap,
    /// Component style overrides, if any.
    pub components: Option<ComponentStyleMap>,
}

/// What [`resolve_theme_values`] reads besides the layer and its seed.
///
/// Upstream's `strictTokens` is not here: it refuses token values that are
/// neither strings nor `[light, dark]` string tuples, which a
/// [`TokenInputs`](super::define::TokenInputs) cannot hold.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ResolveThemeValuesOptions {
    /// The motion character in effect beneath this layer (a base theme's, or
    /// the theme an adaptation rule belongs to), for a layer that sets none.
    pub inherited_character: Option<MotionCharacter>,
    /// The `--ease-standard` beneath a layer resolved without a seed: a rule's
    /// theme's.
    pub standard_beneath: Option<String>,
}

// =============================================================================
// Value helpers
// =============================================================================

/// A token value as CSS (upstream `resolveTokenValue`): a string as it is, a
/// `[light, dark]` tuple as `light-dark(light, dark)`.
pub fn resolve_token_value(value: ThemeTokenValue<'_>) -> String {
    match value {
        ThemeTokenValue::Css(css) => css.to_string(),
        ThemeTokenValue::Pair(light, dark) => format!("light-dark({light}, {dark})"),
    }
}

/// A named weight as its token reference (upstream `resolveFontWeight`):
/// `normal`, `medium`, `semibold` and `bold` become `var(--font-weight-*)` and
/// any other value is returned as it is. Upstream also takes a number and
/// writes its text; read one with `js_number_to_string` first.
pub fn resolve_font_weight(weight: &str) -> String {
    match weight {
        "normal" => "var(--font-weight-normal)".to_string(),
        "medium" => "var(--font-weight-medium)".to_string(),
        "semibold" => "var(--font-weight-semibold)".to_string(),
        "bold" => "var(--font-weight-bold)".to_string(),
        other => other.to_string(),
    }
}

/// The CSS `font-family` value of a family and its fallbacks (upstream
/// `buildFontFamily`): the family quoted when it holds a space, then the
/// fallbacks. `None` without a family (an empty one included).
///
/// `("Geist Mono", "\"SF Mono\", monospace")` is `"Geist Mono", "SF Mono",
/// monospace`.
pub fn build_font_family(family: Option<&str>, fallbacks: Option<&str>) -> Option<String> {
    let family = family.filter(|family| !family.is_empty())?;
    let quoted = if family.contains(' ') {
        format!("\"{family}\"")
    } else {
        family.to_string()
    };
    Some(match fallbacks.filter(|fallbacks| !fallbacks.is_empty()) {
        Some(fallbacks) => format!("{quoted}, {fallbacks}"),
        None => quoted,
    })
}

/// Core's default `--ease-standard` (upstream `easeDefaults['--ease-standard']`).
fn ease_standard_default() -> &'static str {
    EASE_DEFAULTS
        .iter()
        .find(|(name, _)| *name == "--ease-standard")
        .map_or("", |(_, value)| value)
}

/// A weight as JavaScript reads `if (weight)`: set and not empty.
fn set_weight(weight: Option<&String>) -> Option<&str> {
    weight
        .map(String::as_str)
        .filter(|weight| !weight.is_empty())
}

// =============================================================================
// Axis builders
// =============================================================================

/// The type scale of a typography config, with the weights its roles imply
/// (upstream `buildTypeScaleConfig`), or `None` when it sets no scale: font
/// families and weights alone make none.
///
/// The heading role's `weights` set their levels and its `weight` every other
/// level; the body's and code's `weight` set those text types. Named weights
/// become `var(--font-weight-*)` references.
pub fn build_type_scale_config(typo: &TypographyConfig) -> Option<TypeScaleConfig> {
    let scale = typo.scale?;

    // Weight overrides from the typography roles.
    let mut heading_weights: Vec<(u8, String)> = Vec::new();
    let heading_role = typo.heading.as_ref();
    for (level, weight) in heading_role
        .and_then(|role| role.weights.as_ref())
        .into_iter()
        .flatten()
    {
        if !weight.is_empty() {
            let weight = resolve_font_weight(weight);
            match heading_weights.iter_mut().find(|(l, _)| l == level) {
                Some(entry) => entry.1 = weight,
                None => heading_weights.push((*level, weight)),
            }
        }
    }
    // The heading role's weight for every level it does not list.
    if let Some(default_weight) = set_weight(heading_role.and_then(|role| role.weight.as_ref())) {
        let default_weight = resolve_font_weight(default_weight);
        for level in 1..=6 {
            if !heading_weights.iter().any(|(l, _)| *l == level) {
                heading_weights.push((level, default_weight.clone()));
            }
        }
    }
    // A JavaScript object lists integer keys in ascending order.
    heading_weights.sort_by_key(|(level, _)| *level);

    // Text weight overrides from the roles.
    let mut text_weights: Vec<(String, String)> = Vec::new();
    if let Some(weight) = set_weight(typo.body.as_ref().and_then(|role| role.weight.as_ref())) {
        text_weights.push(("body".to_string(), resolve_font_weight(weight)));
    }
    if let Some(weight) = set_weight(typo.code.as_ref().and_then(|role| role.weight.as_ref())) {
        text_weights.push(("code".to_string(), resolve_font_weight(weight)));
    }

    Some(TypeScaleConfig {
        base: scale.base,
        ratio: scale.ratio,
        weights: Some(TypeScaleWeights {
            heading: (!heading_weights.is_empty()).then_some(heading_weights),
            text: (!text_weights.is_empty()).then_some(text_weights),
        }),
    })
}

/// The `--font-family-*` tokens a typography config implies (upstream
/// `buildFontFamilyTokens`). The heading takes the body's family when it names
/// none of its own.
pub fn build_font_family_tokens(typo: &TypographyConfig) -> CssMap {
    let family = |role: Option<&TypographyRole>| {
        build_font_family(
            role.and_then(|role| role.family.as_deref()),
            role.and_then(|role| role.fallbacks.as_deref()),
        )
    };
    let body = family(typo.body.as_ref());
    let heading = family(typo.heading.as_ref()).or_else(|| body.clone());
    let code = family(typo.code.as_ref());

    let mut tokens = CssMap::new();
    if let Some(body) = body {
        tokens.insert("--font-family-body", body);
    }
    if let Some(heading) = heading {
        tokens.insert("--font-family-heading", heading);
    }
    if let Some(code) = code {
        tokens.insert("--font-family-code", code);
    }
    tokens
}

/// The type scale's generated heading and text rules as a component style map.
fn type_scale_components(config: &TypeScaleConfig) -> ComponentStyleMap {
    generate_type_scale_components(config)
        .into_iter()
        .map(|(component, rules)| {
            let rules: ComponentStyleMap = rules
                .into_iter()
                .map(|(key, properties)| {
                    let properties: ComponentStyleMap = properties
                        .into_iter()
                        .map(|(property, value)| (property, StyleValue::Css(value)))
                        .collect();
                    (key, StyleValue::Block(properties))
                })
                .collect();
            (component, StyleValue::Block(rules))
        })
        .collect()
}

// =============================================================================
// Resolution
// =============================================================================

/// One theme layer's axes resolved into tokens and component styles (upstream
/// `resolveThemeValues`), over `seed`, the resolved layer underneath (a base
/// theme for `extends`, or none for a root theme).
///
/// Tokens, lowest precedence first: the seed's, the color scale's, the type
/// scale's, the radius scale's, motion's, the font families, the syntax theme
/// (each key as `--color-syntax-<key>`), then the explicit `tokens`:
///
/// - A layer whose motion writes durations also writes the pre-tempo copies
///   busy loops read, and a tempo of `1` unless it sets one (`spec:AST-108`
///   FR4); an explicit duration token writes its copy too.
/// - At the `standard` character, `--ease-entrance` and `--ease-move` equal the
///   resolved `--ease-standard` (FR2), unless the layer sets them itself.
///
/// Components: the type scale's generated rules, then the layer's own, then
/// all of it over the seed's.
pub fn resolve_theme_values(
    input: &ThemeValuesInput,
    seed: Option<&ThemeValuesSeed>,
    options: &ResolveThemeValuesOptions,
) -> ResolvedThemeValues {
    let mut tokens = seed
        .and_then(|seed| seed.tokens.clone())
        .unwrap_or_default();

    let typo = input.typography.as_ref();
    let type_scale_config = typo.and_then(build_type_scale_config);

    // 1. Color-generated tokens (the lowest precedence for colors).
    if let Some(color) = &input.color {
        tokens.extend(expand_color_scale(color));
    }

    // 2. Type-scale-generated tokens.
    if let Some(config) = &type_scale_config {
        tokens.extend(expand_type_scale(config));
    }

    // 3. Radius-generated tokens.
    if let Some(radius) = &input.radius {
        tokens.extend(expand_radius_scale(radius));
    }

    // 4. Motion-generated tokens (spec:AST-108 FR2, FR5). A layer that writes
    // durations also writes the copies busy loops read (FR4), and a tempo of
    // 1, so its springs keep the pace of its durations inside a theme with a
    // tempo.
    if let Some(motion_config) = &input.motion {
        let motion: CssMap = expand_theme_motion(motion_config).into_iter().collect();
        tokens.assign(&motion);
        let mut writes_durations = false;
        for (name, value) in motion.iter() {
            let pre_tempo = pre_tempo_name(name);
            if pre_tempo != name {
                writes_durations = true;
                if !motion.contains_key(&pre_tempo) {
                    tokens.insert(pre_tempo, value.clone());
                }
            }
        }
        if writes_durations && !motion.contains_key(MOTION_TEMPO_PROPERTY) {
            tokens.insert(MOTION_TEMPO_PROPERTY, "1".to_string());
        }
    }

    // 5. Font family tokens.
    if let Some(typo) = typo {
        tokens.assign(&build_font_family_tokens(typo));
    }

    // 6. Syntax theme tokens.
    if let Some(syntax) = &input.syntax {
        for (key, value) in syntax.iter() {
            tokens.insert(format!("--color-syntax-{key}"), value.clone());
        }
    }

    // 7. Explicit token overrides, the highest precedence within this layer.
    let own_set = |name: &str| {
        input
            .tokens
            .as_ref()
            .and_then(|own| own.get(name))
            .is_some_and(|value| value.value().is_some())
    };
    if let Some(own) = &input.tokens {
        for (key, value) in own.iter() {
            let Some(value) = value.value() else {
                continue;
            };
            let value = resolve_token_value(value);
            tokens.insert(key, value.clone());
            // An explicit duration beats tempo, so a busy loop reading the
            // pre-tempo value reads it too (spec:AST-108 FR4).
            let pre_tempo = pre_tempo_name(key);
            if pre_tempo != key && !own_set(&pre_tempo) {
                tokens.insert(pre_tempo, value);
            }
        }
    }

    // At 'standard', --ease-entrance and --ease-move equal the resolved
    // --ease-standard (spec:AST-108 FR2): the layer's own, else the one
    // beneath it. A layer that sets them itself, or whose character, its own
    // or the one it inherits, picks its own curve, keeps them.
    let own_character = input.motion.as_ref().and_then(|motion| motion.character);
    let character = own_character.or(options.inherited_character);
    if own_character == Some(MotionCharacter::Standard)
        || (own_set("--ease-standard")
            && matches!(character, None | Some(MotionCharacter::Standard)))
    {
        let standard = tokens
            .get("--ease-standard")
            .cloned()
            .or_else(|| options.standard_beneath.clone())
            .unwrap_or_else(|| ease_standard_default().to_string());
        for name in ["--ease-entrance", "--ease-move"] {
            if !own_set(name) {
                tokens.insert(name, standard.clone());
            }
        }
    }

    // Components: the generated type-scale rules (lowest), then this layer's
    // own, then all of it over the seed's.
    let mut components = without_unset_components(input.components.as_ref());
    if let Some(config) = &type_scale_config {
        components = deep_merge_components(
            Some(&type_scale_components(config)),
            input.components.as_ref(),
        );
    }
    if let Some(seed_components) = seed.and_then(|seed| seed.components.as_ref()) {
        components = deep_merge_components(Some(seed_components), components.as_ref());
    }

    ResolvedThemeValues { tokens, components }
}

#[cfg(test)]
mod tests {
    use super::super::define::TokenInput;
    use super::*;

    fn css_map<const N: usize>(entries: [(&str, &str); N]) -> CssMap {
        entries
            .into_iter()
            .map(|(k, v)| (k, v.to_string()))
            .collect()
    }

    fn values(input: ThemeValuesInput) -> ResolvedThemeValues {
        resolve_theme_values(&input, None, &ResolveThemeValuesOptions::default())
    }

    #[test]
    fn a_token_value_is_its_string_or_a_light_dark_pair() {
        assert_eq!(resolve_token_value(ThemeTokenValue::Css("16px")), "16px");
        assert_eq!(
            resolve_token_value(ThemeTokenValue::Pair("#0077B6", "#48CAE4")),
            "light-dark(#0077B6, #48CAE4)"
        );
    }

    #[test]
    fn named_weights_become_references_and_raw_ones_pass_through() {
        assert_eq!(resolve_font_weight("normal"), "var(--font-weight-normal)");
        assert_eq!(resolve_font_weight("medium"), "var(--font-weight-medium)");
        assert_eq!(
            resolve_font_weight("semibold"),
            "var(--font-weight-semibold)"
        );
        assert_eq!(resolve_font_weight("bold"), "var(--font-weight-bold)");
        assert_eq!(resolve_font_weight("800"), "800");
    }

    #[test]
    fn a_font_family_is_quoted_when_it_holds_a_space() {
        assert_eq!(
            build_font_family(Some("Geist Mono"), Some("\"SF Mono\", monospace")),
            Some("\"Geist Mono\", \"SF Mono\", monospace".to_string())
        );
        assert_eq!(
            build_font_family(Some("Lora"), None),
            Some("Lora".to_string())
        );
        assert_eq!(
            build_font_family(Some("Lora"), Some("")),
            Some("Lora".to_string())
        );
        assert_eq!(build_font_family(None, Some("monospace")), None);
        assert_eq!(build_font_family(Some(""), Some("monospace")), None);
    }

    #[test]
    fn a_type_scale_config_collects_the_weights_its_roles_imply() {
        let role = |weight: Option<&str>, weights: &[(u8, &str)]| {
            Some(TypographyRole {
                weight: weight.map(str::to_string),
                weights: Some(weights.iter().map(|(l, w)| (*l, w.to_string())).collect()),
                ..TypographyRole::default()
            })
        };
        let typo = TypographyConfig {
            scale: Some(TypographyScale {
                base: 14.0,
                ratio: 1.2,
            }),
            body: role(Some("medium"), &[]),
            heading: role(Some("semibold"), &[(4, "bold"), (3, "bold"), (2, "")]),
            code: role(Some("700"), &[]),
        };
        let config = build_type_scale_config(&typo).unwrap();
        assert_eq!((config.base, config.ratio), (14.0, 1.2));
        let weights = config.weights.unwrap();
        // Levels in ascending order; an empty weight is skipped and takes the
        // role's default.
        let semibold = "var(--font-weight-semibold)".to_string();
        let bold = "var(--font-weight-bold)".to_string();
        assert_eq!(
            weights.heading,
            Some(vec![
                (1, semibold.clone()),
                (2, semibold.clone()),
                (3, bold.clone()),
                (4, bold),
                (5, semibold.clone()),
                (6, semibold),
            ])
        );
        assert_eq!(
            weights.text,
            Some(vec![
                ("body".to_string(), "var(--font-weight-medium)".to_string()),
                ("code".to_string(), "700".to_string()),
            ])
        );
        // Families and weights without a scale make no scale.
        let unscaled = TypographyConfig {
            scale: None,
            ..typo
        };
        assert_eq!(build_type_scale_config(&unscaled), None);
    }

    #[test]
    fn the_heading_family_follows_the_body_when_it_names_none() {
        let typo = TypographyConfig {
            body: Some(TypographyRole {
                family: Some("Figtree".into()),
                fallbacks: Some("sans-serif".into()),
                ..TypographyRole::default()
            }),
            ..TypographyConfig::default()
        };
        assert_eq!(
            build_font_family_tokens(&typo),
            css_map([
                ("--font-family-body", "Figtree, sans-serif"),
                ("--font-family-heading", "Figtree, sans-serif"),
            ])
        );
    }

    #[test]
    fn explicit_tokens_win_over_every_generated_axis_and_the_seed() {
        let seed = ThemeValuesSeed {
            tokens: Some(css_map([
                ("--radius-inner", "1px"),
                ("--color-syntax-keyword", "red"),
            ])),
            components: None,
        };
        let input = ThemeValuesInput {
            radius: Some(RadiusScaleConfig::default()),
            syntax: Some(css_map([("keyword", "blue"), ("string", "green")])),
            tokens: Some(
                [
                    ("--radius-element", TokenInput::Css("3px".into())),
                    (
                        "--color-syntax-string",
                        TokenInput::Pair("#000".into(), "#fff".into()),
                    ),
                    ("--radius-page", TokenInput::Unset),
                ]
                .into(),
            ),
            ..ThemeValuesInput::default()
        };
        let resolved =
            resolve_theme_values(&input, Some(&seed), &ResolveThemeValuesOptions::default());
        let token = |name: &str| resolved.tokens.get(name).map(String::as_str);
        // The radius scale beats the seed; explicit tokens beat the scale.
        assert_eq!(token("--radius-inner"), Some("4px"));
        assert_eq!(token("--radius-element"), Some("3px"));
        // An unset explicit token leaves the generated value.
        assert_eq!(token("--radius-page"), Some("28px"));
        // Syntax tokens take the prefix and beat the seed; explicit ones win.
        assert_eq!(token("--color-syntax-keyword"), Some("blue"));
        assert_eq!(
            token("--color-syntax-string"),
            Some("light-dark(#000, #fff)")
        );
        // The seed's keys stay first.
        assert_eq!(resolved.tokens.keys().next(), Some("--radius-inner"));
    }

    #[test]
    fn a_layer_writing_durations_writes_their_pre_tempo_copies_and_a_tempo_of_one() {
        let resolved = values(ThemeValuesInput {
            motion: Some(ThemeMotionConfig {
                fast: Some(125.0),
                medium: Some(300.0),
                slow: Some(700.0),
                ratio: Some(0.75),
                ..ThemeMotionConfig::default()
            }),
            ..ThemeValuesInput::default()
        });
        let token = |name: &str| resolved.tokens.get(name).map(String::as_str);
        assert_eq!(token("--duration-fast"), Some("125ms"));
        for name in [
            "--duration-fast",
            "--duration-medium-max",
            "--duration-slow-min",
        ] {
            assert_eq!(token(&pre_tempo_name(name)), token(name), "{name}");
        }
        assert_eq!(token(MOTION_TEMPO_PROPERTY), Some("1"));
    }

    #[test]
    fn an_explicit_duration_writes_its_pre_tempo_copy_unless_that_is_set_too() {
        let resolved = values(ThemeValuesInput {
            tokens: Some(
                [
                    ("--duration-fast", TokenInput::Css("90ms".into())),
                    ("--duration-slow", TokenInput::Css("900ms".into())),
                    (
                        "--_astryx-pre-tempo-duration-slow",
                        TokenInput::Css("800ms".into()),
                    ),
                ]
                .into(),
            ),
            ..ThemeValuesInput::default()
        });
        let token = |name: &str| resolved.tokens.get(name).map(String::as_str);
        assert_eq!(token("--_astryx-pre-tempo-duration-fast"), Some("90ms"));
        assert_eq!(token("--_astryx-pre-tempo-duration-slow"), Some("800ms"));
        // The token goes in before its copy, as upstream assigns them.
        let keys: Vec<&str> = resolved.tokens.keys().collect();
        assert_eq!(
            keys[..2],
            ["--duration-fast", "--_astryx-pre-tempo-duration-fast"]
        );
        // No motion axis, so no tempo is written.
        assert_eq!(token(MOTION_TEMPO_PROPERTY), None);
    }

    #[test]
    fn at_standard_the_entrance_and_move_curves_follow_the_resolved_standard() {
        let standard = "cubic-bezier(0.2, 0, 0, 1)";
        let own_standard = || {
            Some(TokenInputs::from([(
                "--ease-standard",
                TokenInput::Css(standard.into()),
            )]))
        };
        // An explicit --ease-standard with no character of its own.
        let resolved = values(ThemeValuesInput {
            tokens: own_standard(),
            ..ThemeValuesInput::default()
        });
        assert_eq!(
            resolved.tokens.get("--ease-entrance").map(String::as_str),
            Some(standard)
        );
        assert_eq!(
            resolved.tokens.get("--ease-move").map(String::as_str),
            Some(standard)
        );
        // An inherited character that picks its own curve keeps them.
        let crisp = resolve_theme_values(
            &ThemeValuesInput {
                tokens: own_standard(),
                ..ThemeValuesInput::default()
            },
            None,
            &ResolveThemeValuesOptions {
                inherited_character: Some(MotionCharacter::Crisp),
                ..ResolveThemeValuesOptions::default()
            },
        );
        assert_eq!(crisp.tokens.get("--ease-entrance"), None);
        // An own 'standard' character takes the standard beneath, else core's.
        let character = |standard_beneath: Option<&str>| {
            resolve_theme_values(
                &ThemeValuesInput {
                    motion: Some(ThemeMotionConfig {
                        character: Some(MotionCharacter::Standard),
                        ..ThemeMotionConfig::default()
                    }),
                    ..ThemeValuesInput::default()
                },
                None,
                &ResolveThemeValuesOptions {
                    standard_beneath: standard_beneath.map(str::to_string),
                    ..ResolveThemeValuesOptions::default()
                },
            )
        };
        assert_eq!(
            character(Some(standard))
                .tokens
                .get("--ease-move")
                .map(String::as_str),
            Some(standard)
        );
        assert_eq!(
            character(None)
                .tokens
                .get("--ease-move")
                .map(String::as_str),
            Some("cubic-bezier(0.24, 1, 0.4, 1)")
        );
        // A layer that sets the curves itself keeps them.
        let own = values(ThemeValuesInput {
            tokens: Some(
                [
                    ("--ease-standard", TokenInput::Css(standard.into())),
                    ("--ease-move", TokenInput::Css("linear".into())),
                ]
                .into(),
            ),
            ..ThemeValuesInput::default()
        });
        assert_eq!(
            own.tokens.get("--ease-move").map(String::as_str),
            Some("linear")
        );
        assert_eq!(
            own.tokens.get("--ease-entrance").map(String::as_str),
            Some(standard)
        );
    }

    #[test]
    fn components_merge_generated_rules_under_the_layers_own_over_the_seeds() {
        let block = |entries: &[(&str, &str)]| {
            StyleValue::Block(
                entries
                    .iter()
                    .map(|(k, v)| (*k, StyleValue::Css(v.to_string())))
                    .collect(),
            )
        };
        let seed = ThemeValuesSeed {
            tokens: None,
            components: Some(
                [(
                    "button",
                    StyleValue::Block([("base", block(&[("color", "red")]))].into()),
                )]
                .into(),
            ),
        };
        let input = ThemeValuesInput {
            typography: Some(TypographyConfig {
                scale: Some(TypographyScale {
                    base: 14.0,
                    ratio: 1.2,
                }),
                ..TypographyConfig::default()
            }),
            components: Some(
                [(
                    "heading",
                    StyleValue::Block([("level:1", block(&[("fontSize", "40px")]))].into()),
                )]
                .into(),
            ),
            ..ThemeValuesInput::default()
        };
        let resolved =
            resolve_theme_values(&input, Some(&seed), &ResolveThemeValuesOptions::default());
        let components = resolved.components.unwrap();
        let keys: Vec<&str> = components.keys().collect();
        assert_eq!(keys, ["button", "heading", "text"]);
        let level_1 = components
            .get("heading")
            .and_then(StyleValue::block)
            .and_then(|heading| heading.get("level:1"))
            .and_then(StyleValue::block)
            .unwrap();
        assert_eq!(
            level_1.get("fontSize").and_then(StyleValue::css),
            Some("40px")
        );
        assert_eq!(
            level_1.get("lineHeight").and_then(StyleValue::css),
            Some("var(--text-heading-1-leading)")
        );
    }
}
