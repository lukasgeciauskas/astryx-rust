//! Ordered, environment-conditioned theme adaptations: upstream
//! `theme/themeAdaptations.ts`.
//!
//! An adaptation rule changes a theme's values while the environment matches
//! its condition: a viewport-width range between named width points, the
//! primary pointer's precision, the contrast preference or the reduced-motion
//! preference, every field the condition sets ANDed. The vocabulary is closed
//! and the rules are ordered: every matching rule writes after the theme's root
//! values, and a later matching write wins.
//!
//! [`normalize_theme_adaptations`] validates a definition's adaptations over
//! the ones it inherits: the base's width points and rules first, the theme's
//! own width points over them and its own rules appended.
//! [`resolve_theme_adaptation_rules`] lowers each rule to the concrete token,
//! local-token and component writes it makes. A partial scale in a rule is
//! completed from the theme's effective root axes
//! ([`resolve_theme_generative_axes`]), so a rule a child theme inherits
//! follows the child's own scale. Upstream's stylesheet compiler
//! (`generateAdaptationCSS`) reads those writes; here the typed theme does
//! ([`to_theme`](super::to_theme::to_theme)).
//!
//! The width and height edges are spelled by [`width_below_feature`],
//! [`height_below_feature`], [`width_from_feature`] and [`below_style_key`],
//! and the compact screen by [`compact_definition`] (`spec:AST-012` FR9).

use std::collections::HashSet;
use std::sync::LazyLock;

use super::color_scale::{AccentSeed, ColorContrast, ColorScaleConfig, NeutralStyle};
use super::define::{CssMap, DefineThemeError, OrderedMap, TokenInputs, merge_motion};
use super::js::js_number_to_string;
use super::local_tokens::{assert_no_token_cycles, resolve_adaptation_local_tokens};
use super::merge_components::{ComponentStyleMap, StyleMap, StyleValue, without_unset_components};
use super::radius_scale::RadiusScaleConfig;
use super::resolve_values::{
    ResolveThemeValuesOptions, ThemeValuesInput, TypographyConfig, TypographyRole, TypographyScale,
    resolve_theme_values,
};
use super::status_roles::AdaptationTokenWrites;
use super::theme_motion::ThemeMotionConfig;

// =============================================================================
// Width points and size edges
// =============================================================================

/// The names of the viewport-width tier start points, narrowest first.
/// Upstream `WIDTH_BREAKPOINT_NAMES`.
pub const WIDTH_BREAKPOINT_NAMES: [&str; 5] = ["sm", "md", "lg", "xl", "2xl"];

/// A complete map of the viewport-width tier start points, in CSS px.
/// Upstream `WidthBreakpoints`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WidthBreakpoints {
    /// `sm`.
    pub sm: f64,
    /// `md`.
    pub md: f64,
    /// `lg`.
    pub lg: f64,
    /// `xl`.
    pub xl: f64,
    /// `2xl`.
    pub xl2: f64,
}

impl WidthBreakpoints {
    /// The point named `name` (one of [`WIDTH_BREAKPOINT_NAMES`]).
    pub fn get(&self, name: &str) -> Option<f64> {
        match name {
            "sm" => Some(self.sm),
            "md" => Some(self.md),
            "lg" => Some(self.lg),
            "xl" => Some(self.xl),
            "2xl" => Some(self.xl2),
            _ => None,
        }
    }

    /// The point named `name`, to change it.
    fn get_mut(&mut self, name: &str) -> Option<&mut f64> {
        match name {
            "sm" => Some(&mut self.sm),
            "md" => Some(&mut self.md),
            "lg" => Some(&mut self.lg),
            "xl" => Some(&mut self.xl),
            "2xl" => Some(&mut self.xl2),
            _ => None,
        }
    }

    /// Each point with its name, narrowest first.
    pub fn entries(&self) -> [(&'static str, f64); 5] {
        [
            ("sm", self.sm),
            ("md", self.md),
            ("lg", self.lg),
            ("xl", self.xl),
            ("2xl", self.xl2),
        ]
    }
}

impl Default for WidthBreakpoints {
    /// [`DEFAULT_WIDTH_BREAKPOINTS`].
    fn default() -> Self {
        DEFAULT_WIDTH_BREAKPOINTS
    }
}

/// Astryx's own width points: 640, 768, 1024, 1280 and 1536 px. Upstream
/// `DEFAULT_WIDTH_BREAKPOINTS`.
pub const DEFAULT_WIDTH_BREAKPOINTS: WidthBreakpoints = WidthBreakpoints {
    sm: 640.0,
    md: 768.0,
    lg: 1024.0,
    xl: 1280.0,
    xl2: 1536.0,
};

/// A width strictly below `px` as a media feature for script or plain CSS:
/// `(width < 768px)`. Upstream `widthBelowFeature`.
pub fn width_below_feature(px: f64) -> String {
    format!("(width < {}px)", js_number_to_string(px))
}

/// A height strictly below `px` as a media feature for script or plain CSS:
/// `(height < 500px)`. Upstream `heightBelowFeature`.
pub fn height_below_feature(px: f64) -> String {
    format!("(height < {}px)", js_number_to_string(px))
}

/// A width at or above `px` as a media feature for script or plain CSS:
/// `(width >= 1024px)`. Upstream `widthFromFeature`.
pub fn width_from_feature(px: f64) -> String {
    format!("(width >= {}px)", js_number_to_string(px))
}

/// The edges of a "below" condition: below the width, or below the height.
/// Upstream `BelowEdges`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BelowEdges {
    /// Matches below this width.
    pub width: f64,
    /// Also matches below this height.
    pub height: Option<f64>,
}

/// The StyleX media key for "narrower than `width`, or shorter than
/// `height`", written as the negation of the inclusive lower edges:
/// `@media (not (min-width: 768px))`, or `@media (not ((min-width: 768px) and
/// (min-height: 500px)))` with a height. StyleX lowers a range feature such as
/// `(width < 768px)` to `(max-width: 767.99px)`, which Chromium rounds up to
/// match at exactly 768px too. Upstream `belowStyleKey`.
pub fn below_style_key(edges: &BelowEdges) -> String {
    let width_edge = format!("(min-width: {}px)", js_number_to_string(edges.width));
    match edges.height {
        None => format!("@media (not {width_edge})"),
        Some(height) => format!(
            "@media (not ({width_edge} and (min-height: {}px)))",
            js_number_to_string(height)
        ),
    }
}

/// When a screen is compact, for a component that picks its presentation by
/// screen size (`spec:AST-012` FR9): narrower than the theme's `md` point, or
/// shorter than 500. Upstream `CompactDefinition`.
#[derive(Clone, Debug, PartialEq)]
pub struct CompactDefinition {
    /// Compact below this viewport width: the effective `md` point.
    pub width_below: f64,
    /// Compact below this viewport height.
    pub height_below: f64,
    /// The definition as a media query list, without `@media`.
    pub query: String,
    /// Compact with a coarse primary pointer, without `@media`.
    pub coarse_pointer_query: String,
}

/// The viewport height below which a screen is compact. Upstream
/// `COMPACT_HEIGHT_BELOW`.
const COMPACT_HEIGHT_BELOW: f64 = 500.0;

/// The compact definition at a theme's width points: below its `md`, or below
/// 500 px high. Upstream `compactDefinition`.
pub fn compact_definition(points: &WidthBreakpoints) -> CompactDefinition {
    let width = width_below_feature(points.md);
    let height = height_below_feature(COMPACT_HEIGHT_BELOW);
    CompactDefinition {
        width_below: points.md,
        height_below: COMPACT_HEIGHT_BELOW,
        query: format!("{width}, {height}"),
        coarse_pointer_query: format!(
            "{width} and (pointer: coarse), {height} and (pointer: coarse)"
        ),
    }
}

/// The compact definition at [`DEFAULT_WIDTH_BREAKPOINTS`]. Upstream
/// `DEFAULT_COMPACT_DEFINITION`.
pub static DEFAULT_COMPACT_DEFINITION: LazyLock<CompactDefinition> =
    LazyLock::new(|| compact_definition(&DEFAULT_WIDTH_BREAKPOINTS));

// =============================================================================
// Adaptations as written
// =============================================================================

/// The width range of an adaptation rule, by named width point (`sm`, `md`,
/// `lg`, `xl`, `2xl`): at and above `from`, strictly below `below`. Upstream
/// `ThemeAdaptationWidthCondition`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ThemeAdaptationWidthCondition {
    /// Match at and above this named point.
    pub from: Option<String>,
    /// Match strictly below this named point.
    pub below: Option<String>,
}

/// When an adaptation rule applies; every field it sets must hold. Upstream
/// `ThemeAdaptationCondition`, its values kept as written.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ThemeAdaptationCondition {
    /// The viewport-width range.
    pub width: Option<ThemeAdaptationWidthCondition>,
    /// The primary pointer's precision: `coarse` or `fine`.
    pub pointer: Option<String>,
    /// The contrast preference: `more`, `less` or `no-preference`.
    pub contrast: Option<String>,
    /// The reduced-motion preference: `reduce` or `no-preference`.
    pub motion: Option<String>,
}

/// A partial type scale in an adaptation rule, completed from the theme's
/// own. Upstream `Partial<TypographyConfig['scale']>`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ThemeAdaptationScale {
    /// The base size in px.
    pub base: Option<f64>,
    /// The ratio.
    pub ratio: Option<f64>,
}

/// Typography in an adaptation rule. Upstream
/// `ThemeAdaptationTypographyConfig`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ThemeAdaptationTypographyConfig {
    /// A partial type scale.
    pub scale: Option<ThemeAdaptationScale>,
    /// Body text.
    pub body: Option<TypographyRole>,
    /// Headings.
    pub heading: Option<TypographyRole>,
    /// Code.
    pub code: Option<TypographyRole>,
}

/// A partial color scale in an adaptation rule. Upstream
/// `Partial<ColorScaleConfig>`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ThemeAdaptationColorConfig {
    /// The accent seed.
    pub accent: Option<AccentSeed>,
    /// The neutral style.
    pub neutral_style: Option<NeutralStyle>,
    /// The contrast level.
    pub contrast: Option<ColorContrast>,
}

/// A partial radius scale in an adaptation rule. Upstream
/// `Partial<RadiusScaleConfig>`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ThemeAdaptationRadiusConfig {
    /// The base unit in px.
    pub base: Option<f64>,
    /// The multiplier.
    pub multiplier: Option<f64>,
}

/// What an adaptation rule writes while its condition holds. Upstream
/// `ThemeAdaptationValue`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ThemeAdaptationValue {
    /// Typography.
    pub typography: Option<ThemeAdaptationTypographyConfig>,
    /// The color scale.
    pub color: Option<ThemeAdaptationColorConfig>,
    /// The radius scale.
    pub radius: Option<ThemeAdaptationRadiusConfig>,
    /// Motion: scale fields, `tempo` or `character`.
    pub motion: Option<ThemeMotionConfig>,
    /// Token writes.
    pub tokens: Option<TokenInputs>,
    /// Replacements for theme-local tokens the theme already declares.
    pub local_tokens: Option<TokenInputs>,
    /// Component style writes.
    pub components: Option<ComponentStyleMap>,
}

/// One ordered adaptation rule. Upstream `ThemeAdaptationRule`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ThemeAdaptationRule {
    /// When it applies.
    pub when: ThemeAdaptationCondition,
    /// What it writes.
    pub value: ThemeAdaptationValue,
}

/// A definition's adaptations, as written. Upstream `ThemeAdaptations`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ThemeAdaptations {
    /// Overrides of the named width points, in CSS px. They alone write
    /// nothing.
    pub width_breakpoints: Option<OrderedMap<f64>>,
    /// The rules, in order.
    pub rules: Option<Vec<ThemeAdaptationRule>>,
}

// =============================================================================
// Normalized and resolved adaptations
// =============================================================================

/// A theme's adaptations validated, with what it inherits (upstream
/// `NormalizedThemeAdaptations`, a defined theme's `__adaptations`): kept so a
/// theme extending it re-resolves the same rules against its own axes.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NormalizedThemeAdaptations {
    /// The effective width points: the defaults, the inherited overrides, then
    /// the theme's own.
    pub width_breakpoints: WidthBreakpoints,
    /// The inherited rules, then the theme's own, in order.
    pub rules: Vec<ThemeAdaptationRule>,
}

/// The generative axes a theme resolves (upstream `ThemeGenerativeAxes`): its
/// own over those of the theme it extends. A theme extending this one inherits
/// its motion from here, field by field, and an adaptation rule completes a
/// partial scale from here.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ThemeGenerativeAxes {
    /// Typography.
    pub typography: Option<TypographyConfig>,
    /// The color scale.
    pub color: Option<ColorScaleConfig>,
    /// The radius scale.
    pub radius: Option<RadiusScaleConfig>,
    /// Motion.
    pub motion: Option<ThemeMotionConfig>,
}

/// One rule lowered to concrete writes (upstream
/// `ResolvedThemeAdaptationRule`, an entry of a defined theme's
/// `__adaptationRules`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ResolvedThemeAdaptationRule {
    /// The validated condition.
    pub when: ThemeAdaptationCondition,
    /// The condition as a CSS media query, without `@media`:
    /// `(width < 1024px) and (pointer: coarse)`.
    pub query: String,
    /// The token values the rule writes, generated and explicit.
    pub tokens: CssMap,
    /// The theme-local token values it writes, if any.
    pub local_tokens: Option<CssMap>,
    /// The component styles it writes, if any.
    pub components: Option<ComponentStyleMap>,
}

impl ResolvedThemeAdaptationRule {
    /// The rule's condition and token writes, as the status-role defaults read
    /// them ([`generated_scoped_status_role_defaults`](super::status_roles::generated_scoped_status_role_defaults)).
    pub fn token_writes(&self) -> AdaptationTokenWrites {
        AdaptationTokenWrites {
            when: self.when.clone(),
            tokens: self.tokens.clone(),
            local_tokens: self.local_tokens.clone(),
        }
    }
}

/// Every rule's [`ResolvedThemeAdaptationRule::token_writes`], in order: what
/// upstream's stylesheet compiler hands
/// [`generated_scoped_status_role_defaults`](super::status_roles::generated_scoped_status_role_defaults)
/// so a root status-role alias that a matching rule would turn into a
/// reference cycle is left out.
pub fn adaptation_token_writes(
    rules: &[ResolvedThemeAdaptationRule],
) -> Vec<AdaptationTokenWrites> {
    rules
        .iter()
        .map(ResolvedThemeAdaptationRule::token_writes)
        .collect()
}

// =============================================================================
// Validation
// =============================================================================

/// Fails unless every point in a width-point override map has a known name
/// and is a finite positive number of CSS px. The points, by name, in
/// [`WIDTH_BREAKPOINT_NAMES`] order. Upstream `normalizeBreakpointOverrides`.
fn normalize_breakpoint_overrides(
    value: &OrderedMap<f64>,
    path: &str,
) -> Result<Vec<(&'static str, f64)>, DefineThemeError> {
    for key in value.keys() {
        if !WIDTH_BREAKPOINT_NAMES.contains(&key) {
            return Err(DefineThemeError(format!("{path}.{key} is not supported.")));
        }
    }
    let mut result = Vec::new();
    for name in WIDTH_BREAKPOINT_NAMES {
        let Some(&point) = value.get(name) else {
            continue;
        };
        assert_positive_point(point, &format!("{path}.{name}"))?;
        result.push((name, point));
    }
    Ok(result)
}

/// Fails unless `point` is a finite positive number.
fn assert_positive_point(point: f64, path: &str) -> Result<(), DefineThemeError> {
    if !point.is_finite() || point <= 0.0 {
        return Err(DefineThemeError(format!(
            "{path} must be a finite positive number of CSS pixels."
        )));
    }
    Ok(())
}

/// Fails unless every point of an inherited width map is a finite positive
/// number of CSS px (a complete map, which upstream also checks, is the only
/// kind a [`WidthBreakpoints`] holds). Upstream `assertCompleteBreakpoints`.
fn assert_complete_breakpoints(
    points: &WidthBreakpoints,
    path: &str,
) -> Result<WidthBreakpoints, DefineThemeError> {
    for (name, point) in points.entries() {
        assert_positive_point(point, &format!("{path}.{name}"))?;
    }
    Ok(*points)
}

/// Fails unless each width point is above the one before it. Upstream
/// `assertIncreasingBreakpoints`.
fn assert_increasing_breakpoints(
    points: &WidthBreakpoints,
    path: &str,
) -> Result<(), DefineThemeError> {
    let entries = points.entries();
    for pair in entries.windows(2) {
        let [(previous, below), (name, point)] = [pair[0], pair[1]];
        if point <= below {
            return Err(DefineThemeError(format!(
                "{path} must be strictly increasing: {name} ({}) is not above {previous} ({}).",
                js_number_to_string(point),
                js_number_to_string(below)
            )));
        }
    }
    Ok(())
}

/// A condition, refused unless it sets at least one field, a width range with
/// at least one named edge, and only the values upstream's vocabulary has.
/// Upstream `normalizeCondition`.
fn normalize_condition(
    condition: &ThemeAdaptationCondition,
    path: &str,
) -> Result<ThemeAdaptationCondition, DefineThemeError> {
    if condition.width.is_none()
        && condition.pointer.is_none()
        && condition.contrast.is_none()
        && condition.motion.is_none()
    {
        return Err(DefineThemeError(format!(
            "{path} must contain at least one condition."
        )));
    }
    if let Some(width) = &condition.width {
        if width.from.is_none() && width.below.is_none() {
            return Err(DefineThemeError(format!(
                "{path}.width must contain `from`, `below`, or both."
            )));
        }
        for (edge, name) in [("from", &width.from), ("below", &width.below)] {
            if let Some(name) = name
                && !WIDTH_BREAKPOINT_NAMES.contains(&name.as_str())
            {
                return Err(DefineThemeError(format!(
                    "{path}.width.{edge} must be one of {}.",
                    WIDTH_BREAKPOINT_NAMES.join(", ")
                )));
            }
        }
    }
    if condition
        .pointer
        .as_deref()
        .is_some_and(|pointer| pointer != "coarse" && pointer != "fine")
    {
        return Err(DefineThemeError(format!(
            "{path}.pointer must be 'coarse' or 'fine'."
        )));
    }
    if condition
        .contrast
        .as_deref()
        .is_some_and(|contrast| !["more", "less", "no-preference"].contains(&contrast))
    {
        return Err(DefineThemeError(format!(
            "{path}.contrast must be 'more', 'less', or 'no-preference'."
        )));
    }
    if condition
        .motion
        .as_deref()
        .is_some_and(|motion| motion != "reduce" && motion != "no-preference")
    {
        return Err(DefineThemeError(format!(
            "{path}.motion must be 'reduce' or 'no-preference'."
        )));
    }
    Ok(condition.clone())
}

/// Rules with their conditions validated, the rule at `index` named
/// `path[index]`. Upstream `normalizeRules` and `normalizeRule`.
fn normalize_rules(
    rules: &[ThemeAdaptationRule],
    path: &str,
) -> Result<Vec<ThemeAdaptationRule>, DefineThemeError> {
    rules
        .iter()
        .enumerate()
        .map(|(index, rule)| {
            Ok(ThemeAdaptationRule {
                when: normalize_condition(&rule.when, &format!("{path}[{index}].when"))?,
                value: rule.value.clone(),
            })
        })
        .collect()
}

/// A theme's adaptations, validated over the ones it inherits (upstream
/// `normalizeThemeAdaptations`): the defaults' width points, the inherited
/// ones over them and the theme's own over those, which must rise strictly
/// from `sm` to `2xl`; the inherited rules in their order, then the theme's
/// own.
///
/// Fails, with upstream's message, on an unknown width point, a point that is
/// not a finite positive number, points that do not rise, and a rule condition
/// that sets nothing, a width range with no edge, or a value outside the
/// vocabulary. An inherited map (from a built theme, say) is checked too.
pub fn normalize_theme_adaptations(
    theme_name: &str,
    inherited: Option<&NormalizedThemeAdaptations>,
    input: Option<&ThemeAdaptations>,
) -> Result<NormalizedThemeAdaptations, DefineThemeError> {
    let root_path = format!("defineTheme(\"{theme_name}\").adaptations");

    let mut width_breakpoints = DEFAULT_WIDTH_BREAKPOINTS;
    let mut rules = Vec::new();
    if let Some(inherited) = inherited {
        let path = format!("defineTheme(\"{theme_name}\").extends.__adaptations");
        width_breakpoints = assert_complete_breakpoints(
            &inherited.width_breakpoints,
            &format!("{path}.widthBreakpoints"),
        )?;
        rules = normalize_rules(&inherited.rules, &format!("{path}.rules"))?;
    }

    if let Some(input) = input {
        if let Some(points) = &input.width_breakpoints {
            let overrides =
                normalize_breakpoint_overrides(points, &format!("{root_path}.widthBreakpoints"))?;
            for (name, point) in overrides {
                if let Some(slot) = width_breakpoints.get_mut(name) {
                    *slot = point;
                }
            }
        }
        if let Some(own) = &input.rules {
            rules.extend(normalize_rules(own, &format!("{root_path}.rules"))?);
        }
    }

    assert_increasing_breakpoints(&width_breakpoints, &format!("{root_path}.widthBreakpoints"))?;
    Ok(NormalizedThemeAdaptations {
        width_breakpoints,
        rules,
    })
}

// =============================================================================
// Generative axes
// =============================================================================

/// `{...root, ...rule}` of two heading weight maps, by level. Levels list in
/// ascending order, as a JavaScript object lists integer keys.
fn merge_weights(
    root: Option<&[(u8, String)]>,
    rule: Option<&[(u8, String)]>,
) -> Option<Vec<(u8, String)>> {
    if root.is_none() && rule.is_none() {
        return None;
    }
    let mut merged: Vec<(u8, String)> = Vec::new();
    for (level, weight) in root.into_iter().flatten().chain(rule.into_iter().flatten()) {
        match merged.iter_mut().find(|(l, _)| l == level) {
            Some(entry) => entry.1 = weight.clone(),
            None => merged.push((*level, weight.clone())),
        }
    }
    merged.sort_by_key(|(level, _)| *level);
    Some(merged)
}

/// One typography role over another: the rule's fields over the root's, with
/// their heading weights merged by level. Upstream `mergeRole`.
fn merge_role(
    root: Option<&TypographyRole>,
    rule: Option<&TypographyRole>,
) -> Option<TypographyRole> {
    let (root, rule) = match (root, rule) {
        (None, rule) => return rule.cloned(),
        (root, None) => return root.cloned(),
        (Some(root), Some(rule)) => (root, rule),
    };
    Some(TypographyRole {
        family: rule.family.clone().or_else(|| root.family.clone()),
        fallbacks: rule.fallbacks.clone().or_else(|| root.fallbacks.clone()),
        weight: rule.weight.clone().or_else(|| root.weight.clone()),
        weights: merge_weights(root.weights.as_deref(), rule.weights.as_deref()),
    })
}

/// Whether a role names a family, as JavaScript reads `role?.family` (an empty
/// name is none).
fn has_family(role: Option<&TypographyRole>) -> bool {
    role.and_then(|role| role.family.as_deref())
        .is_some_and(|family| !family.is_empty())
}

/// A theme's effective generative axes through `extends` (upstream
/// `resolveThemeGenerativeAxes`): an axis the theme sets replaces the
/// inherited one, and one it leaves out is inherited.
///
/// Typography merges role by role. A role's family and fallbacks come whole
/// from the theme that names a family (a heading follows the theme's own body
/// when only the body names one); its weights come from the most recent theme
/// that sets a type scale, since weights reach tokens only through a scale.
/// Radius and motion merge field by field; a color config replaces the
/// inherited one whole, since it regenerates the neutrals even without an
/// accent.
pub fn resolve_theme_generative_axes(
    inherited: Option<&ThemeGenerativeAxes>,
    own: &ThemeGenerativeAxes,
) -> ThemeGenerativeAxes {
    let inherited_typography = inherited.and_then(|axes| axes.typography.as_ref());
    let own_typography = own.typography.as_ref();
    let resolve_family = |inherited_role: Option<&TypographyRole>,
                          own_role: Option<&TypographyRole>,
                          follows_own_body: bool|
     -> Option<TypographyRole> {
        let merged = merge_role(inherited_role, own_role)?;
        if let Some(own_role) = own_role.filter(|role| has_family(Some(role))) {
            return Some(TypographyRole {
                family: own_role.family.clone(),
                fallbacks: own_role.fallbacks.clone(),
                ..merged
            });
        }
        if follows_own_body {
            return Some(TypographyRole {
                family: None,
                fallbacks: None,
                ..merged
            });
        }
        // A fallback without a family is inert in buildFontFamilyTokens(), so
        // it cannot replace the fallback of an inherited family.
        Some(TypographyRole {
            family: inherited_role.and_then(|role| role.family.clone()),
            fallbacks: inherited_role.and_then(|role| role.fallbacks.clone()),
            ..merged
        })
    };

    let body = resolve_family(
        inherited_typography.and_then(|t| t.body.as_ref()),
        own_typography.and_then(|t| t.body.as_ref()),
        false,
    );
    let heading = resolve_family(
        inherited_typography.and_then(|t| t.heading.as_ref()),
        own_typography.and_then(|t| t.heading.as_ref()),
        has_family(own_typography.and_then(|t| t.body.as_ref()))
            && !has_family(own_typography.and_then(|t| t.heading.as_ref())),
    );
    let code = resolve_family(
        inherited_typography.and_then(|t| t.code.as_ref()),
        own_typography.and_then(|t| t.code.as_ref()),
        false,
    );

    // Weights reach tokens only when a scale is expanded. The most recent
    // config that declares a scale owns every weight; omitted weights become
    // the expander defaults rather than values from an earlier scale.
    let weight_owner = if own_typography.is_some_and(|t| t.scale.is_some()) {
        own_typography
    } else if inherited_typography.is_some_and(|t| t.scale.is_some()) {
        inherited_typography
    } else {
        None
    };
    let apply_weights = |role: Option<TypographyRole>, owner: Option<&TypographyRole>| {
        role.map(|role| TypographyRole {
            weight: owner.and_then(|owner| owner.weight.clone()),
            weights: owner.and_then(|owner| owner.weights.clone()),
            ..role
        })
    };
    let body = apply_weights(body, weight_owner.and_then(|t| t.body.as_ref()));
    let heading = apply_weights(heading, weight_owner.and_then(|t| t.heading.as_ref()));
    let code = apply_weights(code, weight_owner.and_then(|t| t.code.as_ref()));

    let typography =
        (inherited_typography.is_some() || own_typography.is_some()).then(|| TypographyConfig {
            scale: own_typography
                .and_then(|t| t.scale)
                .or_else(|| inherited_typography.and_then(|t| t.scale)),
            body,
            heading,
            code,
        });
    let color = own
        .color
        .clone()
        .or_else(|| inherited.and_then(|axes| axes.color.clone()));
    let radius = own
        .radius
        .or_else(|| inherited.and_then(|axes| axes.radius));
    let motion = match (&own.motion, inherited.and_then(|axes| axes.motion.as_ref())) {
        (Some(own), Some(base)) => Some(merge_motion(base, own)),
        (Some(own), None) => Some(own.clone()),
        (None, base) => base.cloned(),
    };
    ThemeGenerativeAxes {
        typography,
        color,
        radius,
        motion,
    }
}

// =============================================================================
// Completing a rule's partial axes
// =============================================================================

/// Fails unless `value` is finite. Upstream `assertFiniteNumber`.
fn assert_finite_number(value: f64, path: &str) -> Result<(), DefineThemeError> {
    if !value.is_finite() {
        return Err(DefineThemeError(format!("{path} must be a finite number.")));
    }
    Ok(())
}

/// A rule's typography completed from the root's (upstream
/// `completeTypography`): a partial scale takes the root's missing field, a
/// rule that sets weights takes the root's scale (the weights reach tokens only
/// through one), and each role merges over the root's. A rule that writes
/// neither keeps no scale, so it writes font families alone.
fn complete_typography(
    root: Option<&TypographyConfig>,
    rule: &ThemeAdaptationTypographyConfig,
    path: &str,
) -> Result<TypographyConfig, DefineThemeError> {
    let writes_scale = rule.scale.is_some()
        || [&rule.body, &rule.heading, &rule.code].iter().any(|role| {
            role.as_ref()
                .is_some_and(|role| role.weight.is_some() || role.weights.is_some())
        });
    let root_scale = root.and_then(|root| root.scale);
    let mut scale = if writes_scale { root_scale } else { None };
    if let Some(rule_scale) = &rule.scale {
        let base = rule_scale.base.or(root_scale.map(|scale| scale.base));
        let ratio = rule_scale.ratio.or(root_scale.map(|scale| scale.ratio));
        let (Some(base), Some(ratio)) = (base, ratio) else {
            return Err(DefineThemeError(format!(
                "{path}.scale must supply both `base` and `ratio` unless the missing field exists on the effective root typography axis."
            )));
        };
        assert_finite_number(base, &format!("{path}.scale.base"))?;
        assert_finite_number(ratio, &format!("{path}.scale.ratio"))?;
        scale = Some(TypographyScale { base, ratio });
    } else if writes_scale && scale.is_none() {
        return Err(DefineThemeError(format!(
            "{path} sets typography weights without an effective root scale. Supply `scale.base` and `scale.ratio` in the same rule."
        )));
    }
    Ok(TypographyConfig {
        scale,
        body: merge_role(root.and_then(|root| root.body.as_ref()), rule.body.as_ref()),
        heading: merge_role(
            root.and_then(|root| root.heading.as_ref()),
            rule.heading.as_ref(),
        ),
        code: merge_role(root.and_then(|root| root.code.as_ref()), rule.code.as_ref()),
    })
}

/// A rule's color config over the root's, field by field
/// (`{...axes.color, ...value.color}`); a field neither sets takes its default.
fn complete_color(
    root: Option<&ColorScaleConfig>,
    rule: &ThemeAdaptationColorConfig,
) -> ColorScaleConfig {
    ColorScaleConfig {
        accent: rule
            .accent
            .clone()
            .or_else(|| root.and_then(|root| root.accent.clone())),
        neutral_style: rule
            .neutral_style
            .or(root.map(|root| root.neutral_style))
            .unwrap_or_default(),
        contrast: rule
            .contrast
            .or(root.map(|root| root.contrast))
            .unwrap_or_default(),
    }
}

/// A rule's radius scale completed from the root's, field by field. Upstream
/// `completeRadius`.
fn complete_radius(
    root: Option<&RadiusScaleConfig>,
    rule: &ThemeAdaptationRadiusConfig,
    path: &str,
) -> Result<RadiusScaleConfig, DefineThemeError> {
    let base = rule.base.or(root.map(|root| root.base));
    let multiplier = rule.multiplier.or(root.map(|root| root.multiplier));
    let (Some(base), Some(multiplier)) = (base, multiplier) else {
        return Err(DefineThemeError(format!(
            "{path} must supply both `base` and `multiplier` unless the missing field exists on the effective root radius axis."
        )));
    };
    assert_finite_number(base, &format!("{path}.base"))?;
    assert_finite_number(multiplier, &format!("{path}.multiplier"))?;
    Ok(RadiusScaleConfig { base, multiplier })
}

/// A rule's motion completed from the root's (upstream `completeMotion`): the
/// knobs it leaves out are the root's; with no scale field of its own over a
/// root with no scale, the knobs alone; otherwise a whole duration scale, each
/// field it leaves out the root's.
fn complete_motion(
    root: Option<&ThemeMotionConfig>,
    rule: &ThemeMotionConfig,
    path: &str,
) -> Result<ThemeMotionConfig, DefineThemeError> {
    let tempo = rule.tempo.or(root.and_then(|root| root.tempo));
    let character = rule.character.or(root.and_then(|root| root.character));
    if let Some(tempo) = tempo {
        assert_finite_number(tempo, &format!("{path}.tempo"))?;
    }
    // `'fast' in root`: the root's motion is a scale.
    let root_scale = root.filter(|root| root.fast.is_some());
    let writes_scale = rule.fast.is_some()
        || rule.medium.is_some()
        || rule.ratio.is_some()
        || rule.slow.is_some()
        || rule.easing.is_some();
    if !writes_scale && root_scale.is_none() {
        // Knobs only, over a root with no scale: the knobs generate the tokens.
        return Ok(ThemeMotionConfig {
            tempo,
            character,
            ..ThemeMotionConfig::default()
        });
    }
    let fast = rule.fast.or(root_scale.and_then(|root| root.fast));
    let medium = rule.medium.or(root_scale.and_then(|root| root.medium));
    let ratio = rule.ratio.or(root_scale.and_then(|root| root.ratio));
    let (Some(fast), Some(medium), Some(ratio)) = (fast, medium, ratio) else {
        return Err(DefineThemeError(format!(
            "{path} must supply `fast`, `medium`, and `ratio` unless each missing field exists on the effective root motion axis."
        )));
    };
    assert_finite_number(fast, &format!("{path}.fast"))?;
    assert_finite_number(medium, &format!("{path}.medium"))?;
    assert_finite_number(ratio, &format!("{path}.ratio"))?;
    let slow = rule.slow.or(root_scale.and_then(|root| root.slow));
    if let Some(slow) = slow {
        assert_finite_number(slow, &format!("{path}.slow"))?;
    }
    let easing = rule
        .easing
        .clone()
        .or_else(|| root_scale.and_then(|root| root.easing.clone()));
    Ok(ThemeMotionConfig {
        fast: Some(fast),
        medium: Some(medium),
        slow,
        ratio: Some(ratio),
        easing,
        tempo,
        character,
    })
}

/// A rule's value as a theme layer, its partial axes completed from the
/// theme's. Upstream `valueToThemeInput`.
fn value_to_theme_input(
    theme_name: &str,
    rule_index: usize,
    value: &ThemeAdaptationValue,
    axes: &ThemeGenerativeAxes,
) -> Result<ThemeValuesInput, DefineThemeError> {
    let path = format!("defineTheme(\"{theme_name}\").adaptations.rules[{rule_index}].value");
    Ok(ThemeValuesInput {
        typography: value
            .typography
            .as_ref()
            .map(|typography| {
                complete_typography(
                    axes.typography.as_ref(),
                    typography,
                    &format!("{path}.typography"),
                )
            })
            .transpose()?,
        color: value
            .color
            .as_ref()
            .map(|color| complete_color(axes.color.as_ref(), color)),
        radius: value
            .radius
            .as_ref()
            .map(|radius| complete_radius(axes.radius.as_ref(), radius, &format!("{path}.radius")))
            .transpose()?,
        motion: value
            .motion
            .as_ref()
            .map(|motion| complete_motion(axes.motion.as_ref(), motion, &format!("{path}.motion")))
            .transpose()?,
        syntax: None,
        tokens: value.tokens.clone(),
        components: value.components.clone(),
    })
}

// =============================================================================
// Lowering
// =============================================================================

/// Whether `value` holds `NaN` or `undefined` as a whole word, as the
/// JavaScript pattern `/\b(?:NaN|undefined)\b/` finds it.
fn holds_invalid_word(value: &str) -> bool {
    let bytes = value.as_bytes();
    let is_word = |at: usize| {
        bytes
            .get(at)
            .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
    };
    ["NaN", "undefined"].iter().any(|word| {
        value.match_indices(word).any(|(start, _)| {
            let end = start + word.len();
            (start == 0 || !is_word(start - 1)) && !is_word(end)
        })
    })
}

/// Fails unless a resolved token value is concrete CSS. Upstream
/// `assertConcreteLeaf` on a string.
fn assert_concrete_css(value: &str, path: &str) -> Result<(), DefineThemeError> {
    if holds_invalid_word(value) {
        return Err(DefineThemeError(format!(
            "{path} resolved to the invalid CSS value \"{value}\"."
        )));
    }
    Ok(())
}

/// Fails unless every leaf of a resolved component style is concrete CSS: a
/// `null` or `undefined` leaf is refused. Upstream `assertConcreteLeaf`.
fn assert_concrete_leaf(value: &StyleValue, path: &str) -> Result<(), DefineThemeError> {
    match value {
        StyleValue::Css(css) => assert_concrete_css(css, path),
        StyleValue::Block(map) => assert_concrete_map(map, path),
        StyleValue::Null | StyleValue::Unset => Err(DefineThemeError(format!(
            "{path} must resolve to a concrete CSS string."
        ))),
    }
}

/// [`assert_concrete_leaf`] of every entry of a block, each named
/// `path.key`.
fn assert_concrete_map(map: &StyleMap, path: &str) -> Result<(), DefineThemeError> {
    for (key, nested) in map.iter() {
        assert_concrete_leaf(nested, &format!("{path}.{key}"))?;
    }
    Ok(())
}

/// A condition as a CSS media query, without `@media`: its width edges at the
/// theme's points, then its pointer, contrast and motion features, ANDed.
/// Fails on a width range whose `from` point is not below its `below` one.
/// Upstream `mediaQueryForCondition`.
fn media_query_for_condition(
    theme_name: &str,
    rule_index: usize,
    condition: &ThemeAdaptationCondition,
    points: &WidthBreakpoints,
) -> Result<String, DefineThemeError> {
    let path = format!("defineTheme(\"{theme_name}\").adaptations.rules[{rule_index}].when");
    let mut parts: Vec<String> = Vec::new();

    if let Some(width) = &condition.width {
        let from = width
            .from
            .as_deref()
            .and_then(|name| Some((name, points.get(name)?)));
        let below = width
            .below
            .as_deref()
            .and_then(|name| Some((name, points.get(name)?)));
        if let (Some((from, from_px)), Some((below, below_px))) = (from, below)
            && from_px >= below_px
        {
            return Err(DefineThemeError(format!(
                "{path}.width must resolve to `from < below`; {from} is {}px and {below} is {}px.",
                js_number_to_string(from_px),
                js_number_to_string(below_px)
            )));
        }
        if let Some((_, px)) = from {
            parts.push(width_from_feature(px));
        }
        if let Some((_, px)) = below {
            parts.push(width_below_feature(px));
        }
    }
    if let Some(pointer) = &condition.pointer {
        parts.push(format!("(pointer: {pointer})"));
    }
    if let Some(contrast) = &condition.contrast {
        parts.push(format!("(prefers-contrast: {contrast})"));
    }
    if let Some(motion) = &condition.motion {
        parts.push(format!("(prefers-reduced-motion: {motion})"));
    }

    if parts.is_empty() {
        return Err(DefineThemeError(format!(
            "{path} must contain at least one concrete condition."
        )));
    }
    Ok(parts.join(" and "))
}

/// The pointers, contrast preferences and motion preferences a browser can
/// report to the supported queries. Upstream `ENVIRONMENT_VALUES`.
const POINTERS: [&str; 3] = ["coarse", "fine", "none"];
const CONTRASTS: [&str; 4] = ["more", "less", "no-preference", "custom"];
const MOTIONS: [&str; 2] = ["reduce", "no-preference"];

/// Whether a validated condition holds at one representative environment.
/// Upstream `conditionMatchesEnvironment`.
fn condition_matches_environment(
    condition: &ThemeAdaptationCondition,
    points: &WidthBreakpoints,
    width: f64,
    pointer: &str,
    contrast: &str,
    motion: &str,
) -> bool {
    let edge = |name: Option<&String>| name.and_then(|name| points.get(name));
    let range = condition.width.as_ref();
    if let Some(from) = edge(range.and_then(|w| w.from.as_ref()))
        && width < from
    {
        return false;
    }
    if let Some(below) = edge(range.and_then(|w| w.below.as_ref()))
        && width >= below
    {
        return false;
    }
    condition.pointer.as_deref().is_none_or(|p| p == pointer)
        && condition.contrast.as_deref().is_none_or(|c| c == contrast)
        && condition.motion.as_deref().is_none_or(|m| m == motion)
}

/// Fails on a token reference cycle reachable where some set of the rules
/// matches together (upstream `assertNoReachableTokenCycles`).
///
/// Conditions use a closed vocabulary, so zero and each width point sample
/// every distinct width region, and the pointers, contrast and motion
/// preferences enumerate every relevant state. For each distinct set of
/// token-writing rules that can match, the root's tokens and local tokens take
/// the rules' writes in order before the graph is checked, so a later matching
/// rule can repair an earlier rule's cycle as it does in CSS. Only a cycle
/// through a name a matching rule writes fails, named by the rules and their
/// queries.
fn assert_no_reachable_token_cycles(
    theme_name: &str,
    points: &WidthBreakpoints,
    root_tokens: &CssMap,
    root_local_tokens: Option<&CssMap>,
    rules: &[ResolvedThemeAdaptationRule],
) -> Result<(), DefineThemeError> {
    let token_rule_indexes: Vec<usize> = rules
        .iter()
        .enumerate()
        .filter(|(_, rule)| {
            !rule.tokens.is_empty() || rule.local_tokens.as_ref().is_some_and(|l| !l.is_empty())
        })
        .map(|(index, _)| index)
        .collect();
    if token_rule_indexes.is_empty() {
        return Ok(());
    }

    let mut widths = vec![0.0];
    widths.extend(points.entries().map(|(_, px)| px));
    let mut checked: HashSet<Vec<usize>> = HashSet::new();

    for &width in &widths {
        for pointer in POINTERS {
            for contrast in CONTRASTS {
                for motion in MOTIONS {
                    let matching: Vec<usize> = token_rule_indexes
                        .iter()
                        .copied()
                        .filter(|&index| {
                            condition_matches_environment(
                                &rules[index].when,
                                points,
                                width,
                                pointer,
                                contrast,
                                motion,
                            )
                        })
                        .collect();
                    if matching.is_empty() || !checked.insert(matching.clone()) {
                        continue;
                    }

                    let mut effective = root_tokens.clone();
                    if let Some(local) = root_local_tokens {
                        effective.assign(local);
                    }
                    let mut relevant: HashSet<String> = HashSet::new();
                    for &index in &matching {
                        let rule = &rules[index];
                        effective.assign(&rule.tokens);
                        relevant.extend(rule.tokens.keys().map(str::to_string));
                        if let Some(local) = &rule.local_tokens {
                            effective.assign(local);
                            relevant.extend(local.keys().map(str::to_string));
                        }
                    }

                    let indexes: Vec<String> =
                        matching.iter().map(|index| index.to_string()).collect();
                    let queries: Vec<&str> = matching
                        .iter()
                        .map(|&index| rules[index].query.as_str())
                        .collect();
                    let context = format!(
                        "defineTheme(\"{theme_name}\").adaptations {} [{}] ({})",
                        if matching.len() == 1 {
                            "rule"
                        } else {
                            "overlapping rules"
                        },
                        indexes.join(", "),
                        queries.join("; ")
                    );
                    assert_no_token_cycles(&effective, Some(&context), Some(&relevant))?;
                }
            }
        }
    }
    Ok(())
}

/// A scale rule's generated component styles without the ones the root already
/// declares (upstream `omitRootTypeScaleDefaults`). The type scale's component
/// wiring is the same `var()`s at every scale, so a rule that only changes the
/// scale need not restate the root's; a property the root does not declare
/// stays, and so does every property the rule writes itself, even one equal to
/// the root's, since it may undo an earlier matching rule. `None` when nothing
/// is left.
fn omit_root_type_scale_defaults(
    resolved: Option<ComponentStyleMap>,
    root: Option<&ComponentStyleMap>,
    authored: Option<&ComponentStyleMap>,
) -> Option<ComponentStyleMap> {
    let (Some(resolved), Some(root)) = (resolved.as_ref(), root) else {
        return resolved;
    };
    let style = |map: Option<&ComponentStyleMap>, component: &str, key: &str| {
        map.and_then(|map| map.get(component)?.block()?.get(key)?.block())
            .cloned()
    };

    let mut components = ComponentStyleMap::new();
    for (component, rules) in resolved.iter() {
        for (key, styles) in rules.block().into_iter().flat_map(|rules| rules.iter()) {
            let root_style = style(Some(root), component, key);
            let authored_style = style(authored, component, key);
            let writes: StyleMap = styles
                .block()
                .into_iter()
                .flat_map(|styles| styles.iter())
                .filter(|(property, _)| {
                    root_style
                        .as_ref()
                        .and_then(|root| root.get(property))
                        .is_none_or(|value| matches!(value, StyleValue::Unset))
                        || authored_style
                            .as_ref()
                            .is_some_and(|authored| authored.contains_key(property))
                })
                .map(|(property, value)| (property, value.clone()))
                .collect();
            if writes.is_empty() {
                continue;
            }
            if components.get(component).is_none() {
                components.insert(component, StyleValue::Block(StyleMap::new()));
            }
            if let Some(StyleValue::Block(keys)) = components.get_mut(component) {
                keys.insert(key, StyleValue::Block(writes));
            }
        }
    }
    (!components.is_empty()).then_some(components)
}

/// A theme's normalized rules lowered to concrete writes, in order (upstream
/// `resolveThemeAdaptationRules`); `None` when it has none.
///
/// Each rule's value resolves as a theme layer of its own, with no seed: its
/// partial axes completed from `axes` (a type scale's missing field, a radius
/// or motion field, the color config's), then the generated tokens and
/// components, the explicit `tokens` over them. A rule without motion of its
/// own keeps the theme's motion character, and one at `standard` matches the
/// theme's `--ease-standard`. A rule that sets a type scale leaves out the
/// generated component styles the root already declares. `query` is the
/// condition at the theme's width points.
///
/// Fails, with upstream's message, on a partial axis the root cannot
/// complete, a value that resolves to `NaN` or `undefined`, a `null` component
/// value, a token that is one of the theme's local tokens (it goes in
/// `local_tokens`), a local token the theme does not declare, a width range
/// whose edges do not rise, and a token cycle where some set of rules matches
/// together.
pub fn resolve_theme_adaptation_rules(
    theme_name: &str,
    adaptations: &NormalizedThemeAdaptations,
    axes: &ThemeGenerativeAxes,
    root_tokens: &CssMap,
    root_local_tokens: Option<&CssMap>,
    root_components: Option<&ComponentStyleMap>,
) -> Result<Option<Vec<ResolvedThemeAdaptationRule>>, DefineThemeError> {
    if adaptations.rules.is_empty() {
        return Ok(None);
    }

    let mut resolved_rules = Vec::with_capacity(adaptations.rules.len());
    for (index, rule) in adaptations.rules.iter().enumerate() {
        let input = value_to_theme_input(theme_name, index, &rule.value, axes)?;
        // Upstream also refuses a malformed token value here (`strictTokens`),
        // which a TokenInput cannot hold.
        let resolved = resolve_theme_values(
            &input,
            None,
            &ResolveThemeValuesOptions {
                inherited_character: axes.motion.as_ref().and_then(|motion| motion.character),
                standard_beneath: root_tokens.get("--ease-standard").cloned(),
            },
        );
        let path = format!("defineTheme(\"{theme_name}\").adaptations.rules[{index}].value");
        for (name, value) in resolved.tokens.iter() {
            assert_concrete_css(value, &format!("{path}.tokens[\"{name}\"]"))?;
        }
        if let Some(components) = &resolved.components {
            assert_concrete_map(components, &format!("{path}.components"))?;
        }

        for name in rule.value.tokens.iter().flat_map(|tokens| tokens.keys()) {
            if root_local_tokens.is_some_and(|local| local.contains_key(name)) {
                return Err(DefineThemeError(format!(
                    "{path}.tokens[\"{name}\"] matches an enrolled theme-local declaration; write it through value.localTokens instead."
                )));
            }
        }

        let local_tokens = resolve_adaptation_local_tokens(
            theme_name,
            index,
            rule.value.local_tokens.as_ref(),
            root_local_tokens,
        )?;
        let query = media_query_for_condition(
            theme_name,
            index,
            &rule.when,
            &adaptations.width_breakpoints,
        )?;
        let components = if input
            .typography
            .as_ref()
            .is_some_and(|typography| typography.scale.is_some())
        {
            // A value the rule leaves undefined is no write of its own.
            let authored = without_unset_components(rule.value.components.as_ref());
            omit_root_type_scale_defaults(resolved.components, root_components, authored.as_ref())
        } else {
            resolved.components
        };
        resolved_rules.push(ResolvedThemeAdaptationRule {
            when: rule.when.clone(),
            query,
            tokens: resolved.tokens,
            local_tokens,
            components,
        });
    }

    assert_no_reachable_token_cycles(
        theme_name,
        &adaptations.width_breakpoints,
        root_tokens,
        root_local_tokens,
        &resolved_rules,
    )?;
    Ok(Some(resolved_rules))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::super::define::{DefineThemeInput, DefinedTheme, TokenInput, define_theme};
    use super::super::status_roles::generated_scoped_status_role_defaults;
    use super::*;

    // The expected values are upstream's tests' (themeAdaptations.test.ts,
    // themeAdaptations.edges.test.ts, themeMotion.test.ts, defineTheme.test.ts
    // and tokenValueCompat.test.ts). Where an upstream test matches a pattern
    // or compares two of its own results, the exact text or value here is what
    // upstream's built defineTheme (packages/core/dist) gives for the same
    // input.

    fn define(input: DefineThemeInput) -> DefinedTheme {
        define_theme(&input).unwrap()
    }

    fn error(input: DefineThemeInput) -> String {
        define_theme(&input).unwrap_err().0
    }

    fn rules(theme: &DefinedTheme) -> &[ResolvedThemeAdaptationRule] {
        theme.adaptation_rules.as_deref().unwrap_or(&[])
    }

    fn token<'a>(rule: &'a ResolvedThemeAdaptationRule, name: &str) -> Option<&'a str> {
        rule.tokens.get(name).map(String::as_str)
    }

    fn pointer(pointer: &str) -> ThemeAdaptationCondition {
        ThemeAdaptationCondition {
            pointer: Some(pointer.to_string()),
            ..ThemeAdaptationCondition::default()
        }
    }

    fn contrast(contrast: &str) -> ThemeAdaptationCondition {
        ThemeAdaptationCondition {
            contrast: Some(contrast.to_string()),
            ..ThemeAdaptationCondition::default()
        }
    }

    fn motion(motion: &str) -> ThemeAdaptationCondition {
        ThemeAdaptationCondition {
            motion: Some(motion.to_string()),
            ..ThemeAdaptationCondition::default()
        }
    }

    fn range(from: Option<&str>, below: Option<&str>) -> Option<ThemeAdaptationWidthCondition> {
        Some(ThemeAdaptationWidthCondition {
            from: from.map(str::to_string),
            below: below.map(str::to_string),
        })
    }

    fn width(from: Option<&str>, below: Option<&str>) -> ThemeAdaptationCondition {
        ThemeAdaptationCondition {
            width: range(from, below),
            ..ThemeAdaptationCondition::default()
        }
    }

    fn tokens<const N: usize>(entries: [(&str, &str); N]) -> TokenInputs {
        entries
            .into_iter()
            .map(|(name, value)| (name, TokenInput::Css(value.to_string())))
            .collect()
    }

    fn rule(when: ThemeAdaptationCondition, value: ThemeAdaptationValue) -> ThemeAdaptationRule {
        ThemeAdaptationRule { when, value }
    }

    fn writes<const N: usize>(
        when: ThemeAdaptationCondition,
        entries: [(&str, &str); N],
    ) -> ThemeAdaptationRule {
        rule(
            when,
            ThemeAdaptationValue {
                tokens: Some(tokens(entries)),
                ..ThemeAdaptationValue::default()
            },
        )
    }

    fn local_writes<const N: usize>(
        when: ThemeAdaptationCondition,
        entries: [(&str, &str); N],
    ) -> ThemeAdaptationRule {
        rule(
            when,
            ThemeAdaptationValue {
                local_tokens: Some(tokens(entries)),
                ..ThemeAdaptationValue::default()
            },
        )
    }

    fn scale_rule(
        when: ThemeAdaptationCondition,
        base: Option<f64>,
        ratio: Option<f64>,
    ) -> ThemeAdaptationRule {
        rule(when, scale_value(base, ratio))
    }

    fn scale_value(base: Option<f64>, ratio: Option<f64>) -> ThemeAdaptationValue {
        ThemeAdaptationValue {
            typography: Some(ThemeAdaptationTypographyConfig {
                scale: Some(ThemeAdaptationScale { base, ratio }),
                ..ThemeAdaptationTypographyConfig::default()
            }),
            ..ThemeAdaptationValue::default()
        }
    }

    fn with_rules(name: &str, rules: Vec<ThemeAdaptationRule>) -> DefineThemeInput {
        DefineThemeInput {
            adaptations: Some(ThemeAdaptations {
                rules: Some(rules),
                ..ThemeAdaptations::default()
            }),
            ..DefineThemeInput::new(name)
        }
    }

    fn adaptive(rules: Vec<ThemeAdaptationRule>) -> DefineThemeInput {
        with_rules("adaptive", rules)
    }

    fn points<const N: usize>(entries: [(&str, f64); N]) -> Option<OrderedMap<f64>> {
        Some(entries.into_iter().collect())
    }

    fn type_scale(base: f64, ratio: f64) -> Option<TypographyConfig> {
        Some(TypographyConfig {
            scale: Some(TypographyScale { base, ratio }),
            ..TypographyConfig::default()
        })
    }

    fn family(family: &str, fallbacks: Option<&str>) -> Option<TypographyRole> {
        Some(TypographyRole {
            family: Some(family.to_string()),
            fallbacks: fallbacks.map(str::to_string),
            ..TypographyRole::default()
        })
    }

    fn weight(weight: &str) -> Option<TypographyRole> {
        Some(TypographyRole {
            weight: Some(weight.to_string()),
            ..TypographyRole::default()
        })
    }

    fn css(value: &str) -> StyleValue {
        StyleValue::Css(value.to_string())
    }

    fn block<const N: usize>(entries: [(&str, StyleValue); N]) -> StyleValue {
        StyleValue::Block(entries.into())
    }

    fn components<const N: usize>(entries: [(&str, StyleValue); N]) -> Option<ComponentStyleMap> {
        Some(entries.into())
    }

    /// A style map as compact JSON in its key order, as `JSON.stringify`
    /// writes upstream's objects.
    fn json(map: Option<&StyleMap>) -> String {
        fn text(value: &str) -> String {
            format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
        }
        fn value(value: &StyleValue) -> String {
            match value {
                StyleValue::Css(css) => text(css),
                StyleValue::Block(map) => object(map),
                StyleValue::Null => "null".to_string(),
                StyleValue::Unset => "undefined".to_string(),
            }
        }
        fn object(map: &StyleMap) -> String {
            let entries: Vec<String> = map
                .iter()
                .map(|(key, entry)| format!("{}:{}", text(key), value(entry)))
                .collect();
            format!("{{{}}}", entries.join(","))
        }
        map.map_or_else(|| "null".to_string(), object)
    }

    fn entries(map: &CssMap) -> Vec<(&str, &str)> {
        map.iter()
            .map(|(name, value)| (name, value.as_str()))
            .collect()
    }

    const DISPLAY_FAMILY: &str = r#"Sarina, "Brush Script MT", "Snell Roundhand", cursive"#;

    // =========================================================================
    // Size edges (themeAdaptations.edges.test.ts)
    // =========================================================================

    #[test]
    fn spells_range_features_for_script_and_plain_css() {
        assert_eq!(width_below_feature(768.0), "(width < 768px)");
        assert_eq!(height_below_feature(500.0), "(height < 500px)");
        assert_eq!(width_from_feature(1024.0), "(width >= 1024px)");
    }

    #[test]
    fn spells_below_style_keys_as_the_negated_lower_edges() {
        assert_eq!(
            below_style_key(&BelowEdges {
                width: 768.0,
                height: None
            }),
            "@media (not (min-width: 768px))"
        );
        assert_eq!(
            below_style_key(&BelowEdges {
                width: 768.0,
                height: Some(500.0)
            }),
            "@media (not ((min-width: 768px) and (min-height: 500px)))"
        );
    }

    // =========================================================================
    // Width points
    // =========================================================================

    #[test]
    fn provides_the_fixed_width_point_names_and_defaults() {
        assert_eq!(WIDTH_BREAKPOINT_NAMES, ["sm", "md", "lg", "xl", "2xl"]);
        assert_eq!(
            DEFAULT_WIDTH_BREAKPOINTS.entries(),
            [
                ("sm", 640.0),
                ("md", 768.0),
                ("lg", 1024.0),
                ("xl", 1280.0),
                ("2xl", 1536.0)
            ]
        );
        let theme = define(DefineThemeInput::new("defaults"));
        assert_eq!(
            theme.adaptations,
            Some(NormalizedThemeAdaptations {
                width_breakpoints: DEFAULT_WIDTH_BREAKPOINTS,
                rules: Vec::new(),
            })
        );
        assert_eq!(theme.adaptation_rules, None);
    }

    #[test]
    fn a_theme_overrides_width_points_without_writing_a_rule() {
        let theme = define(DefineThemeInput {
            adaptations: Some(ThemeAdaptations {
                width_breakpoints: points([("md", 800.0), ("lg", 1100.0)]),
                rules: None,
            }),
            ..DefineThemeInput::new("points-only")
        });
        assert_eq!(
            theme.adaptations.unwrap().width_breakpoints,
            WidthBreakpoints {
                md: 800.0,
                lg: 1100.0,
                ..DEFAULT_WIDTH_BREAKPOINTS
            }
        );
        assert_eq!(theme.adaptation_rules, None);
    }

    #[test]
    fn validates_inherited_width_points_even_with_no_rules() {
        // Upstream's stylesheet compiler reads a built theme's metadata as the
        // adaptations it inherits; the rise is checked under the theme's own
        // path.
        let built = NormalizedThemeAdaptations {
            width_breakpoints: WidthBreakpoints {
                lg: 700.0,
                ..DEFAULT_WIDTH_BREAKPOINTS
            },
            rules: Vec::new(),
        };
        assert_eq!(
            normalize_theme_adaptations("stale-built-points", Some(&built), None)
                .unwrap_err()
                .0,
            "defineTheme(\"stale-built-points\").adaptations.widthBreakpoints must be strictly increasing: lg (700) is not above md (768)."
        );
        // A point that is not positive is named by the inherited metadata.
        let negative = NormalizedThemeAdaptations {
            width_breakpoints: WidthBreakpoints {
                sm: -1.0,
                ..DEFAULT_WIDTH_BREAKPOINTS
            },
            rules: Vec::new(),
        };
        assert_eq!(
            normalize_theme_adaptations("child", Some(&negative), None)
                .unwrap_err()
                .0,
            "defineTheme(\"child\").extends.__adaptations.widthBreakpoints.sm must be a finite positive number of CSS pixels."
        );
    }

    #[test]
    fn refuses_an_invalid_width_point_map() {
        let cases: [(Option<OrderedMap<f64>>, &str); 4] = [
            (
                points([("phone", 500.0)]),
                "defineTheme(\"bad-points\").adaptations.widthBreakpoints.phone is not supported.",
            ),
            (
                points([("md", 0.0)]),
                "defineTheme(\"bad-points\").adaptations.widthBreakpoints.md must be a finite positive number of CSS pixels.",
            ),
            (
                points([("md", f64::NAN)]),
                "defineTheme(\"bad-points\").adaptations.widthBreakpoints.md must be a finite positive number of CSS pixels.",
            ),
            (
                points([("md", 1100.0), ("lg", 1000.0)]),
                "defineTheme(\"bad-points\").adaptations.widthBreakpoints must be strictly increasing: lg (1000) is not above md (1100).",
            ),
        ];
        for (width_breakpoints, message) in cases {
            assert_eq!(
                error(DefineThemeInput {
                    adaptations: Some(ThemeAdaptations {
                        width_breakpoints,
                        rules: None,
                    }),
                    ..DefineThemeInput::new("bad-points")
                }),
                message
            );
        }
    }

    // =========================================================================
    // The compact definition (spec:AST-012 FR9)
    // =========================================================================

    #[test]
    fn the_compact_definition_is_below_md_or_below_500_high_at_the_default_points() {
        assert_eq!(
            *DEFAULT_COMPACT_DEFINITION,
            CompactDefinition {
                width_below: 768.0,
                height_below: 500.0,
                query: "(width < 768px), (height < 500px)".to_string(),
                coarse_pointer_query:
                    "(width < 768px) and (pointer: coarse), (height < 500px) and (pointer: coarse)"
                        .to_string(),
            }
        );
        assert_eq!(
            compact_definition(&DEFAULT_WIDTH_BREAKPOINTS),
            *DEFAULT_COMPACT_DEFINITION
        );
    }

    #[test]
    fn the_compact_definition_follows_a_themes_md_point_and_keeps_the_height_edge() {
        let theme = define(DefineThemeInput {
            adaptations: Some(ThemeAdaptations {
                width_breakpoints: points([("md", 800.0), ("lg", 1100.0)]),
                rules: None,
            }),
            ..DefineThemeInput::new("wide-md")
        });
        assert_eq!(
            compact_definition(&theme.adaptations.unwrap().width_breakpoints),
            CompactDefinition {
                width_below: 800.0,
                height_below: 500.0,
                query: "(width < 800px), (height < 500px)".to_string(),
                coarse_pointer_query:
                    "(width < 800px) and (pointer: coarse), (height < 500px) and (pointer: coarse)"
                        .to_string(),
            }
        );
    }

    // =========================================================================
    // Closed conditions and media queries
    // =========================================================================

    #[test]
    fn lowers_inclusive_from_and_exclusive_below_edges() {
        let theme = define(adaptive(vec![
            writes(width(None, Some("sm")), [("--spacing-4", "12px")]),
            writes(width(Some("lg"), Some("xl")), [("--spacing-4", "20px")]),
            writes(width(Some("2xl"), None), [("--spacing-4", "24px")]),
        ]));
        let queries: Vec<&str> = rules(&theme).iter().map(|r| r.query.as_str()).collect();
        assert_eq!(
            queries,
            [
                "(width < 640px)",
                "(width >= 1024px) and (width < 1280px)",
                "(width >= 1536px)"
            ]
        );
    }

    #[test]
    fn ands_width_pointer_contrast_and_motion_in_a_stable_order() {
        let when = ThemeAdaptationCondition {
            motion: Some("reduce".into()),
            pointer: Some("coarse".into()),
            width: range(Some("md"), Some("xl")),
            contrast: Some("more".into()),
        };
        let theme = define(adaptive(vec![writes(
            when.clone(),
            [("--duration-fast", "0ms")],
        )]));
        let rule = &rules(&theme)[0];
        assert_eq!(rule.when, when);
        assert_eq!(
            rule.query,
            "(width >= 768px) and (width < 1280px) and (pointer: coarse) and (prefers-contrast: more) and (prefers-reduced-motion: reduce)"
        );
        // An explicit duration writes the copy busy loops read.
        assert_eq!(
            entries(&rule.tokens),
            [
                ("--duration-fast", "0ms"),
                ("--_astryx-pre-tempo-duration-fast", "0ms")
            ]
        );
    }

    #[test]
    fn refuses_an_invalid_condition() {
        let cases = [
            (
                ThemeAdaptationCondition::default(),
                "defineTheme(\"adaptive\").adaptations.rules[0].when must contain at least one condition.",
            ),
            (
                width(None, None),
                "defineTheme(\"adaptive\").adaptations.rules[0].when.width must contain `from`, `below`, or both.",
            ),
            (
                width(Some("lg"), Some("md")),
                "defineTheme(\"adaptive\").adaptations.rules[0].when.width must resolve to `from < below`; lg is 1024px and md is 768px.",
            ),
            (
                pointer("any"),
                "defineTheme(\"adaptive\").adaptations.rules[0].when.pointer must be 'coarse' or 'fine'.",
            ),
            (
                width(Some("phone"), None),
                "defineTheme(\"adaptive\").adaptations.rules[0].when.width.from must be one of sm, md, lg, xl, 2xl.",
            ),
            (
                contrast("high"),
                "defineTheme(\"adaptive\").adaptations.rules[0].when.contrast must be 'more', 'less', or 'no-preference'.",
            ),
            (
                motion("fast"),
                "defineTheme(\"adaptive\").adaptations.rules[0].when.motion must be 'reduce' or 'no-preference'.",
            ),
        ];
        for (when, message) in cases {
            assert_eq!(
                error(adaptive(vec![writes(when, [("--spacing-4", "12px")])])),
                message
            );
        }
    }

    // =========================================================================
    // Rule order and writes
    // =========================================================================

    #[test]
    fn keeps_duplicate_conditions_and_a_later_root_restoring_write() {
        let theme = define(DefineThemeInput {
            tokens: Some(tokens([("--spacing-4", "16px")])),
            ..with_rules(
                "ordered",
                vec![
                    writes(pointer("coarse"), [("--spacing-4", "12px")]),
                    writes(pointer("coarse"), [("--spacing-4", "16px")]),
                ],
            )
        });
        let rules = rules(&theme);
        assert_eq!(rules.len(), 2);
        for (rule, value) in rules.iter().zip(["12px", "16px"]) {
            assert_eq!(rule.query, "(pointer: coarse)");
            assert_eq!(entries(&rule.tokens), [("--spacing-4", value)]);
        }
    }

    #[test]
    fn keeps_declaration_order_rather_than_inferred_condition_specificity() {
        let theme = define(adaptive(vec![
            writes(
                ThemeAdaptationCondition {
                    width: range(Some("md"), Some("lg")),
                    pointer: Some("coarse".into()),
                    ..ThemeAdaptationCondition::default()
                },
                [("--spacing-4", "12px")],
            ),
            writes(pointer("coarse"), [("--spacing-4", "20px")]),
        ]));
        let rules = rules(&theme);
        assert_eq!(
            rules[0].query,
            "(width >= 768px) and (width < 1024px) and (pointer: coarse)"
        );
        assert_eq!(token(&rules[0], "--spacing-4"), Some("12px"));
        assert_eq!(rules[1].query, "(pointer: coarse)");
        assert_eq!(token(&rules[1], "--spacing-4"), Some("20px"));
    }

    #[test]
    fn keeps_a_rule_with_an_empty_value_and_it_writes_nothing() {
        let theme = define(adaptive(vec![
            rule(pointer("coarse"), ThemeAdaptationValue::default()),
            writes(pointer("fine"), [("--spacing-4", "18px")]),
        ]));
        assert_eq!(theme.adaptations.as_ref().unwrap().rules.len(), 2);
        let rules = rules(&theme);
        assert_eq!(
            rules[0],
            ResolvedThemeAdaptationRule {
                when: pointer("coarse"),
                query: "(pointer: coarse)".to_string(),
                ..ResolvedThemeAdaptationRule::default()
            }
        );
        assert_eq!(rules[1].query, "(pointer: fine)");
        assert_eq!(entries(&rules[1].tokens), [("--spacing-4", "18px")]);
    }

    #[test]
    fn a_part_value_a_rule_leaves_undefined_is_no_write_and_null_is_refused() {
        let theme = define(adaptive(vec![rule(
            pointer("coarse"),
            ThemeAdaptationValue {
                components: components([(
                    "card",
                    block([(
                        "base",
                        block([("padding", StyleValue::Unset), ("margin", css("4px"))]),
                    )]),
                )]),
                ..ThemeAdaptationValue::default()
            },
        )]));
        assert_eq!(
            json(rules(&theme)[0].components.as_ref()),
            r#"{"card":{"base":{"margin":"4px"}}}"#
        );
        // A component or style key a rule leaves undefined is not set either.
        let keys = define(adaptive(vec![rule(
            pointer("coarse"),
            ThemeAdaptationValue {
                components: components([
                    ("card", block([("base", StyleValue::Unset)])),
                    ("button", StyleValue::Unset),
                    ("link", block([("base", block([("color", css("red"))]))])),
                ]),
                ..ThemeAdaptationValue::default()
            },
        )]));
        assert_eq!(
            json(rules(&keys)[0].components.as_ref()),
            r#"{"link":{"base":{"color":"red"}}}"#
        );
        assert_eq!(
            error(adaptive(vec![rule(
                pointer("coarse"),
                ThemeAdaptationValue {
                    components: components([(
                        "card",
                        block([("base", block([("padding", StyleValue::Null)]))]),
                    )]),
                    ..ThemeAdaptationValue::default()
                },
            )])),
            "defineTheme(\"adaptive\").adaptations.rules[0].value.components.card.base.padding must resolve to a concrete CSS string."
        );
    }

    #[test]
    fn a_type_scale_value_a_scale_rule_leaves_undefined_is_no_write() {
        let theme = define(DefineThemeInput {
            typography: type_scale(16.0, 1.2),
            ..with_rules(
                "scaled",
                vec![
                    rule(
                        pointer("coarse"),
                        ThemeAdaptationValue {
                            components: components([(
                                "text",
                                block([("type:body", block([("fontSize", css("20px"))]))]),
                            )]),
                            ..ThemeAdaptationValue::default()
                        },
                    ),
                    rule(
                        pointer("coarse"),
                        ThemeAdaptationValue {
                            components: components([(
                                "text",
                                block([("type:body", block([("fontSize", StyleValue::Unset)]))]),
                            )]),
                            ..scale_value(Some(16.0), Some(1.3))
                        },
                    ),
                ],
            )
        });
        let rules = rules(&theme);
        assert_eq!(
            json(rules[0].components.as_ref()),
            r#"{"text":{"type:body":{"fontSize":"20px"}}}"#
        );
        // So the earlier rule's size holds where both match.
        assert_eq!(rules[1].components, None);
    }

    #[test]
    fn explicit_values_beat_generated_values_within_one_rule() {
        let theme = define(DefineThemeInput {
            color: Some(ColorScaleConfig {
                accent: Some(AccentSeed::Single("#0064E0".into())),
                ..ColorScaleConfig::default()
            }),
            ..with_rules(
                "rule-precedence",
                vec![rule(
                    contrast("more"),
                    ThemeAdaptationValue {
                        color: Some(ThemeAdaptationColorConfig {
                            accent: Some(AccentSeed::Single("#00AA00".into())),
                            ..ThemeAdaptationColorConfig::default()
                        }),
                        tokens: Some(tokens([("--color-accent", "#FF0000")])),
                        ..ThemeAdaptationValue::default()
                    },
                )],
            )
        });
        assert_eq!(token(&rules(&theme)[0], "--color-accent"), Some("#FF0000"));
    }

    #[test]
    fn generated_leaves_are_rule_writes_above_root_token_pins() {
        let theme = define(DefineThemeInput {
            typography: type_scale(14.0, 1.2),
            tokens: Some(tokens([("--font-size-base", "2rem")])),
            ..with_rules(
                "generated-write",
                vec![scale_rule(width(None, Some("sm")), Some(16.0), None)],
            )
        });
        assert_eq!(
            theme.tokens.get("--font-size-base").map(String::as_str),
            Some("2rem")
        );
        assert_eq!(token(&rules(&theme)[0], "--font-size-base"), Some("1rem"));
    }

    #[test]
    fn writes_font_families_without_re_expanding_an_untouched_root_scale() {
        let theme = define(DefineThemeInput {
            typography: type_scale(14.0, 1.2),
            ..with_rules(
                "family-only",
                vec![rule(
                    pointer("coarse"),
                    ThemeAdaptationValue {
                        typography: Some(ThemeAdaptationTypographyConfig {
                            body: family("Geist", None),
                            ..ThemeAdaptationTypographyConfig::default()
                        }),
                        ..ThemeAdaptationValue::default()
                    },
                )],
            )
        });
        let rule = &rules(&theme)[0];
        // The heading has no family of its own, so it takes the body's.
        assert_eq!(
            entries(&rule.tokens),
            [
                ("--font-family-body", "Geist"),
                ("--font-family-heading", "Geist")
            ]
        );
        assert_eq!(rule.components, None);
    }

    #[test]
    fn completes_partial_scales_from_the_root_and_refuses_what_it_cannot_complete() {
        let theme = define(DefineThemeInput {
            typography: type_scale(14.0, 1.25),
            radius: Some(RadiusScaleConfig {
                base: 6.0,
                multiplier: 1.0,
            }),
            ..with_rules(
                "complete-scale",
                vec![rule(
                    width(None, Some("md")),
                    ThemeAdaptationValue {
                        radius: Some(ThemeAdaptationRadiusConfig {
                            base: None,
                            multiplier: Some(2.0),
                        }),
                        ..scale_value(Some(16.0), None)
                    },
                )],
            )
        });
        let adapted = &rules(&theme)[0];
        assert_eq!(token(adapted, "--font-size-lg"), Some("1.25rem"));
        assert_eq!(token(adapted, "--radius-element"), Some("24px"));

        let motion_rule = |name: &str, motion: ThemeMotionConfig| {
            with_rules(
                name,
                vec![rule(
                    pointer("coarse"),
                    ThemeAdaptationValue {
                        motion: Some(motion),
                        ..ThemeAdaptationValue::default()
                    },
                )],
            )
        };
        assert_eq!(
            error(motion_rule(
                "missing-scale",
                ThemeMotionConfig {
                    fast: Some(100.0),
                    ..ThemeMotionConfig::default()
                }
            )),
            "defineTheme(\"missing-scale\").adaptations.rules[0].value.motion must supply `fast`, `medium`, and `ratio` unless each missing field exists on the effective root motion axis."
        );
        assert_eq!(
            error(motion_rule(
                "nan-tempo",
                ThemeMotionConfig {
                    tempo: Some(f64::NAN),
                    ..ThemeMotionConfig::default()
                }
            )),
            "defineTheme(\"nan-tempo\").adaptations.rules[0].value.motion.tempo must be a finite number."
        );
        assert_eq!(
            error(with_rules(
                "missing-type-scale",
                vec![scale_rule(pointer("coarse"), Some(16.0), None)]
            )),
            "defineTheme(\"missing-type-scale\").adaptations.rules[0].value.typography.scale must supply both `base` and `ratio` unless the missing field exists on the effective root typography axis."
        );
        assert_eq!(
            error(with_rules(
                "weights-no-scale",
                vec![rule(
                    pointer("coarse"),
                    ThemeAdaptationValue {
                        typography: Some(ThemeAdaptationTypographyConfig {
                            heading: weight("bold"),
                            ..ThemeAdaptationTypographyConfig::default()
                        }),
                        ..ThemeAdaptationValue::default()
                    }
                )]
            )),
            "defineTheme(\"weights-no-scale\").adaptations.rules[0].value.typography sets typography weights without an effective root scale. Supply `scale.base` and `scale.ratio` in the same rule."
        );
        assert_eq!(
            error(with_rules(
                "missing-radius",
                vec![rule(
                    pointer("coarse"),
                    ThemeAdaptationValue {
                        radius: Some(ThemeAdaptationRadiusConfig {
                            base: None,
                            multiplier: Some(2.0),
                        }),
                        ..ThemeAdaptationValue::default()
                    }
                )]
            )),
            "defineTheme(\"missing-radius\").adaptations.rules[0].value.radius must supply both `base` and `multiplier` unless the missing field exists on the effective root radius axis."
        );
    }

    #[test]
    fn refuses_a_rule_value_that_resolves_to_nan() {
        assert_eq!(
            error(with_rules(
                "invalid-css",
                vec![writes(
                    pointer("coarse"),
                    [("--spacing-4", "calc(NaN * 1px)")]
                )]
            )),
            "defineTheme(\"invalid-css\").adaptations.rules[0].value.tokens[\"--spacing-4\"] resolved to the invalid CSS value \"calc(NaN * 1px)\"."
        );
        // Only the whole word: a name holding the letters is fine.
        assert!(!holds_invalid_word("--undefinedish NaNo xNaN"));
        assert!(holds_invalid_word("1px undefined"));
    }

    #[test]
    fn passes_component_writes_through() {
        let theme = define(adaptive(vec![rule(
            pointer("coarse"),
            ThemeAdaptationValue {
                components: components([(
                    "button",
                    block([("base", block([("borderRadius", css("20px"))]))]),
                )]),
                ..ThemeAdaptationValue::default()
            },
        )]));
        let rule = &rules(&theme)[0];
        assert!(rule.tokens.is_empty());
        assert_eq!(
            json(rule.components.as_ref()),
            r#"{"button":{"base":{"borderRadius":"20px"}}}"#
        );
    }

    // =========================================================================
    // Root status-role aliases with the rules' token writes
    // =========================================================================

    /// Whether the root declares `--color-text-info: var(--color-text-blue)`,
    /// as upstream's root rules do (generateThemeRules: the theme's tokens and
    /// generatedScopedStatusRoleDefaults with the rules).
    fn root_aliases_info(theme: &DefinedTheme) -> bool {
        let empty = CssMap::new();
        let writes = adaptation_token_writes(rules(theme));
        let local = theme.local_tokens.as_ref().unwrap_or(&empty);
        theme.tokens.get("--color-text-info").map(String::as_str) == Some("var(--color-text-blue)")
            || generated_scoped_status_role_defaults(&theme.tokens, local, &writes)
                .iter()
                .any(|(role, value)| {
                    role == "--color-text-info" && value == "var(--color-text-blue)"
                })
    }

    fn blue_theme(name: &str, rules: Vec<ThemeAdaptationRule>) -> DefinedTheme {
        define(DefineThemeInput {
            tokens: Some(tokens([("--color-text-blue", "#0066cc")])),
            ..with_rules(name, rules)
        })
    }

    #[test]
    fn leaves_out_a_root_alias_an_adaptation_would_turn_into_a_cycle() {
        let theme = define(DefineThemeInput {
            tokens: Some(tokens([("--color-text-blue", "#0074e2")])),
            ..with_rules(
                "adaptation-closes-root-role-cycle",
                vec![writes(
                    contrast("more"),
                    [("--color-text-blue", "var(--color-text-info)")],
                )],
            )
        });
        assert!(!root_aliases_info(&theme));
        // Without the rules the alias is there.
        assert!(
            generated_scoped_status_role_defaults(&theme.tokens, &CssMap::new(), &[])
                .contains(&("--color-text-info".into(), "var(--color-text-blue)".into()))
        );
    }

    #[test]
    fn root_aliases_follow_the_cascade_where_rules_match_together() {
        // Mutually exclusive conditions never match together.
        assert!(root_aliases_info(&blue_theme(
            "mutually-exclusive-status-references",
            vec![
                writes(
                    pointer("coarse"),
                    [("--color-text-blue", "var(--color-text-success)")]
                ),
                writes(
                    pointer("fine"),
                    [("--color-text-green", "var(--color-text-info)")]
                ),
            ]
        )));
        // A later matching rule repairs the cycle.
        assert!(root_aliases_info(&blue_theme(
            "repaired-status-reference",
            vec![
                writes(
                    pointer("coarse"),
                    [("--color-text-blue", "var(--color-text-info)")]
                ),
                writes(pointer("coarse"), [("--color-text-blue", "#abcdef")]),
            ]
        )));
        // An important declaration survives the later plain one.
        assert!(!root_aliases_info(&blue_theme(
            "important-status-reference",
            vec![
                writes(
                    pointer("coarse"),
                    [("--color-text-blue", "var(--color-text-info) !important")]
                ),
                writes(pointer("coarse"), [("--color-text-blue", "#123456")]),
            ]
        )));
        // An escaped bang is no declaration priority.
        assert!(root_aliases_info(&blue_theme(
            "escaped-bang-status-reference",
            vec![
                writes(
                    pointer("coarse"),
                    [("--color-text-blue", r"var(--color-text-info) \!important")]
                ),
                writes(pointer("coarse"), [("--color-text-blue", "#123456")]),
            ]
        )));
        // Nor is an important() function.
        assert!(!root_aliases_info(&blue_theme(
            "important-function-status-reference",
            vec![
                writes(
                    pointer("coarse"),
                    [("--color-text-blue", "red !important()")]
                ),
                writes(
                    pointer("coarse"),
                    [("--color-text-blue", "var(--color-text-info)")]
                ),
            ]
        )));
        // A declaration the stylesheet drops repairs nothing.
        assert!(!root_aliases_info(&blue_theme(
            "dropped-status-repair",
            vec![
                writes(
                    pointer("coarse"),
                    [("--color-text-blue", "var(--color-text-info)")]
                ),
                writes(pointer("coarse"), [("--color-text-blue", "#abcdef;")]),
            ]
        )));
    }

    #[test]
    fn keeps_a_root_alias_when_a_matching_rule_overwrites_that_alias() {
        let mut theme = blue_theme(
            "overwritten-root-status-alias",
            vec![writes(
                pointer("coarse"),
                [("--color-text-info", "#123456")],
            )],
        );
        theme.adaptation_rules.as_mut().unwrap()[0]
            .tokens
            .insert("--color-text-info", "var(--color-text-info)".to_string());
        assert!(root_aliases_info(&theme));
    }

    #[test]
    fn token_writes_carry_each_rules_condition_and_writes() {
        let theme = define(adaptive(vec![writes(
            pointer("coarse"),
            [("--spacing-4", "12px")],
        )]));
        assert_eq!(
            adaptation_token_writes(rules(&theme)),
            [AdaptationTokenWrites {
                when: pointer("coarse"),
                tokens: [("--spacing-4", "12px".to_string())].into(),
                local_tokens: None,
            }]
        );
    }

    // =========================================================================
    // Adapted type-scale component defaults
    // =========================================================================

    fn butter_shaped() -> DefineThemeInput {
        let display = |level: &str| {
            (
                format!("type:{level}"),
                block([("fontFamily", css(DISPLAY_FAMILY))]),
            )
        };
        DefineThemeInput {
            typography: Some(TypographyConfig {
                scale: Some(TypographyScale {
                    base: 16.0,
                    ratio: 1.25,
                }),
                body: family("Outfit", Some("sans-serif")),
                ..TypographyConfig::default()
            }),
            components: Some(
                [
                    (
                        "text".to_string(),
                        StyleValue::Block(
                            ["display-1", "display-2", "display-3"]
                                .map(display)
                                .into_iter()
                                .collect(),
                        ),
                    ),
                    (
                        "button".to_string(),
                        block([("base", block([("borderRadius", css("12px"))]))]),
                    ),
                ]
                .into_iter()
                .collect(),
            ),
            ..with_rules("butter-shaped", vec![butter_rule()])
        }
    }

    fn butter_rule() -> ThemeAdaptationRule {
        scale_rule(
            ThemeAdaptationCondition {
                width: range(None, Some("md")),
                pointer: Some("coarse".into()),
                ..ThemeAdaptationCondition::default()
            },
            Some(14.0),
            Some(1.2),
        )
    }

    #[test]
    fn keeps_butter_shaped_display_families_while_adapting_scale_tokens_only() {
        let theme = define(butter_shaped());
        let rule = &rules(&theme)[0];
        assert_eq!(rule.query, "(width < 768px) and (pointer: coarse)");
        assert_eq!(
            theme.tokens.get("--font-size-base").map(String::as_str),
            Some("1rem")
        );
        assert_eq!(token(rule, "--font-size-base"), Some("0.875rem"));
        assert_eq!(
            theme.tokens.get("--font-size-5xl").map(String::as_str),
            Some("3.8125rem")
        );
        assert_eq!(token(rule, "--font-size-5xl"), Some("2.625rem"));
        // The root owns the var() wiring and its authored overrides.
        assert_eq!(rule.components, None);
        let text = theme.components.as_ref().unwrap().get("text").unwrap();
        for level in ["display-1", "display-2", "display-3"] {
            let style = text.block().unwrap().get(&format!("type:{level}")).unwrap();
            assert_eq!(
                style.block().unwrap().get("fontFamily"),
                Some(&css(DISPLAY_FAMILY))
            );
        }
    }

    #[test]
    fn fills_only_missing_generated_leaves_when_the_root_has_no_scale() {
        let own = components([
            (
                "heading",
                block([
                    (
                        "level:1",
                        block([("fontFamily", css("serif")), ("fontWeight", css("900"))]),
                    ),
                    ("type:display-1", block([("fontSize", css("5rem"))])),
                ]),
            ),
            (
                "text",
                block([
                    (
                        "type:display-1",
                        block([
                            ("fontFamily", css(DISPLAY_FAMILY)),
                            ("lineHeight", css("1.1")),
                        ]),
                    ),
                    ("type:body", block([("fontSize", css("18px"))])),
                    ("type:custom", block([("fontFamily", css("monospace"))])),
                ]),
            ),
            (
                "button",
                block([("base", block([("borderRadius", css("12px"))]))]),
            ),
        ]);
        let before = own.clone();
        let theme = define(DefineThemeInput {
            typography: None,
            components: own.clone(),
            ..butter_shaped()
        });
        assert_eq!(
            json(rules(&theme)[0].components.as_ref()),
            r#"{"heading":{"level:1":{"fontSize":"var(--text-heading-1-size)","lineHeight":"var(--text-heading-1-leading)"},"level:2":{"fontFamily":"var(--font-family-heading)","fontSize":"var(--text-heading-2-size)","fontWeight":"var(--text-heading-2-weight)","lineHeight":"var(--text-heading-2-leading)"},"level:3":{"fontFamily":"var(--font-family-heading)","fontSize":"var(--text-heading-3-size)","fontWeight":"var(--text-heading-3-weight)","lineHeight":"var(--text-heading-3-leading)"},"level:4":{"fontFamily":"var(--font-family-heading)","fontSize":"var(--text-heading-4-size)","fontWeight":"var(--text-heading-4-weight)","lineHeight":"var(--text-heading-4-leading)"},"level:5":{"fontFamily":"var(--font-family-heading)","fontSize":"var(--text-heading-5-size)","fontWeight":"var(--text-heading-5-weight)","lineHeight":"var(--text-heading-5-leading)"},"level:6":{"fontFamily":"var(--font-family-heading)","fontSize":"var(--text-heading-6-size)","fontWeight":"var(--text-heading-6-weight)","lineHeight":"var(--text-heading-6-leading)"},"type:display-1":{"fontFamily":"var(--font-family-heading)","lineHeight":"var(--text-display-1-leading)"},"type:display-2":{"fontFamily":"var(--font-family-heading)","fontSize":"var(--text-display-2-size)","lineHeight":"var(--text-display-2-leading)"},"type:display-3":{"fontFamily":"var(--font-family-heading)","fontSize":"var(--text-display-3-size)","lineHeight":"var(--text-display-3-leading)"}},"text":{"type:body":{"fontFamily":"var(--font-family-body)","lineHeight":"var(--text-body-leading)"},"type:large":{"fontFamily":"var(--font-family-body)","fontSize":"var(--text-large-size)","lineHeight":"var(--text-large-leading)"},"type:label":{"fontFamily":"var(--font-family-body)","fontSize":"var(--text-label-size)","lineHeight":"var(--text-label-leading)"},"type:code":{"fontFamily":"var(--font-family-code)","fontSize":"var(--text-code-size)","lineHeight":"var(--text-code-leading)"},"type:supporting":{"fontFamily":"var(--font-family-body)","fontSize":"var(--text-supporting-size)","lineHeight":"var(--text-supporting-leading)"},"type:display-1":{"fontSize":"var(--text-display-1-size)"},"type:display-2":{"fontFamily":"var(--font-family-heading)","fontSize":"var(--text-display-2-size)","lineHeight":"var(--text-display-2-leading)"},"type:display-3":{"fontFamily":"var(--font-family-heading)","fontSize":"var(--text-display-3-size)","lineHeight":"var(--text-display-3-leading)"}}}"#
        );
        assert_eq!(own, before);
        assert_eq!(theme.components, before);
    }

    #[test]
    fn keeps_explicit_rule_writes_including_a_later_root_restoring_value() {
        let theme = define(DefineThemeInput {
            adaptations: Some(ThemeAdaptations {
                rules: Some(vec![
                    rule(
                        pointer("coarse"),
                        ThemeAdaptationValue {
                            components: components([
                                (
                                    "text",
                                    block([(
                                        "type:display-1",
                                        block([("fontFamily", css("monospace"))]),
                                    )]),
                                ),
                                (
                                    "heading",
                                    block([("level:1", block([("fontWeight", css("300"))]))]),
                                ),
                            ]),
                            ..scale_value(Some(15.0), None)
                        },
                    ),
                    butter_rule(),
                    rule(
                        contrast("more"),
                        ThemeAdaptationValue {
                            components: components([(
                                "text",
                                block([(
                                    "type:display-1",
                                    block([("fontFamily", css(DISPLAY_FAMILY))]),
                                )]),
                            )]),
                            ..scale_value(Some(14.0), None)
                        },
                    ),
                ]),
                width_breakpoints: None,
            }),
            ..butter_shaped()
        });
        let [first, scale_only, restore] = rules(&theme) else {
            panic!("three rules");
        };
        assert_eq!(
            json(first.components.as_ref()),
            r#"{"heading":{"level:1":{"fontWeight":"300"}},"text":{"type:display-1":{"fontFamily":"monospace"}}}"#
        );
        assert_eq!(scale_only.components, None);
        assert_eq!(
            json(restore.components.as_ref()),
            r#"{"text":{"type:display-1":{"fontFamily":"Sarina, \"Brush Script MT\", \"Snell Roundhand\", cursive"}}}"#
        );
    }

    #[test]
    fn a_later_generated_leaf_is_written_when_its_root_path_is_absent() {
        let theme = define(adaptive(vec![
            rule(
                pointer("coarse"),
                ThemeAdaptationValue {
                    components: components([(
                        "text",
                        block([("type:body", block([("fontSize", css("2rem"))]))]),
                    )]),
                    ..ThemeAdaptationValue::default()
                },
            ),
            scale_rule(pointer("coarse"), Some(14.0), Some(1.2)),
        ]));
        assert_eq!(theme.axes.as_ref().unwrap().typography, None);
        assert_eq!(theme.components, None);
        let [authored, generated] = rules(&theme) else {
            panic!("two rules");
        };
        assert_eq!(
            json(authored.components.as_ref()),
            r#"{"text":{"type:body":{"fontSize":"2rem"}}}"#
        );
        assert_eq!(
            json(generated.components.as_ref()),
            r#"{"heading":{"level:1":{"fontFamily":"var(--font-family-heading)","fontSize":"var(--text-heading-1-size)","fontWeight":"var(--text-heading-1-weight)","lineHeight":"var(--text-heading-1-leading)"},"level:2":{"fontFamily":"var(--font-family-heading)","fontSize":"var(--text-heading-2-size)","fontWeight":"var(--text-heading-2-weight)","lineHeight":"var(--text-heading-2-leading)"},"level:3":{"fontFamily":"var(--font-family-heading)","fontSize":"var(--text-heading-3-size)","fontWeight":"var(--text-heading-3-weight)","lineHeight":"var(--text-heading-3-leading)"},"level:4":{"fontFamily":"var(--font-family-heading)","fontSize":"var(--text-heading-4-size)","fontWeight":"var(--text-heading-4-weight)","lineHeight":"var(--text-heading-4-leading)"},"level:5":{"fontFamily":"var(--font-family-heading)","fontSize":"var(--text-heading-5-size)","fontWeight":"var(--text-heading-5-weight)","lineHeight":"var(--text-heading-5-leading)"},"level:6":{"fontFamily":"var(--font-family-heading)","fontSize":"var(--text-heading-6-size)","fontWeight":"var(--text-heading-6-weight)","lineHeight":"var(--text-heading-6-leading)"},"type:display-1":{"fontFamily":"var(--font-family-heading)","fontSize":"var(--text-display-1-size)","lineHeight":"var(--text-display-1-leading)"},"type:display-2":{"fontFamily":"var(--font-family-heading)","fontSize":"var(--text-display-2-size)","lineHeight":"var(--text-display-2-leading)"},"type:display-3":{"fontFamily":"var(--font-family-heading)","fontSize":"var(--text-display-3-size)","lineHeight":"var(--text-display-3-leading)"}},"text":{"type:body":{"fontFamily":"var(--font-family-body)","fontSize":"var(--text-body-size)","lineHeight":"var(--text-body-leading)"},"type:large":{"fontFamily":"var(--font-family-body)","fontSize":"var(--text-large-size)","lineHeight":"var(--text-large-leading)"},"type:label":{"fontFamily":"var(--font-family-body)","fontSize":"var(--text-label-size)","lineHeight":"var(--text-label-leading)"},"type:code":{"fontFamily":"var(--font-family-code)","fontSize":"var(--text-code-size)","lineHeight":"var(--text-code-leading)"},"type:supporting":{"fontFamily":"var(--font-family-body)","fontSize":"var(--text-supporting-size)","lineHeight":"var(--text-supporting-leading)"},"type:display-1":{"fontFamily":"var(--font-family-heading)","fontSize":"var(--text-display-1-size)","lineHeight":"var(--text-display-1-leading)"},"type:display-2":{"fontFamily":"var(--font-family-heading)","fontSize":"var(--text-display-2-size)","lineHeight":"var(--text-display-2-leading)"},"type:display-3":{"fontFamily":"var(--font-family-heading)","fontSize":"var(--text-display-3-size)","lineHeight":"var(--text-display-3-leading)"}}}"#
        );
    }

    #[test]
    fn resolves_against_the_effective_child_root_without_changing_the_base() {
        let base = define(butter_shaped());
        let before = base.components.clone();
        let base = Arc::new(base);
        let child = define(DefineThemeInput {
            extends: Some(base.clone()),
            components: components([
                (
                    "text",
                    block([("type:display-2", block([("fontFamily", css("serif"))]))]),
                ),
                (
                    "heading",
                    block([("level:2", block([("fontFamily", css("monospace"))]))]),
                ),
            ]),
            ..DefineThemeInput::new("child-display")
        });
        assert_eq!(rules(&child)[0].components, None);
        let family = |component: &str, key: &str| {
            child
                .components
                .as_ref()
                .unwrap()
                .get(component)?
                .block()?
                .get(key)?
                .block()?
                .get("fontFamily")
                .cloned()
        };
        assert_eq!(family("text", "type:display-1"), Some(css(DISPLAY_FAMILY)));
        assert_eq!(family("text", "type:display-2"), Some(css("serif")));
        assert_eq!(family("heading", "level:2"), Some(css("monospace")));
        assert_eq!(base.components, before);
    }

    // =========================================================================
    // Theme-local adaptation values
    // =========================================================================

    const LOCAL_NAME: &str = "--rustryx-theme-local-root-control-height";

    #[test]
    fn a_rule_replaces_an_enrolled_local_token_and_may_read_it() {
        let theme = define(DefineThemeInput {
            local_tokens: Some(tokens([(LOCAL_NAME, "32px")])),
            ..with_rules(
                "local-root",
                vec![rule(
                    pointer("coarse"),
                    ThemeAdaptationValue {
                        local_tokens: Some(tokens([(LOCAL_NAME, "44px")])),
                        components: components([(
                            "button",
                            block([(
                                "base",
                                block([("minHeight", css(&format!("var({LOCAL_NAME})")))]),
                            )]),
                        )]),
                        ..ThemeAdaptationValue::default()
                    },
                )],
            )
        });
        let rule = &rules(&theme)[0];
        assert_eq!(
            rule.local_tokens,
            Some([(LOCAL_NAME, "44px".to_string())].into())
        );
        assert!(rule.tokens.is_empty());
        assert_eq!(
            json(rule.components.as_ref()),
            r#"{"button":{"base":{"minHeight":"var(--rustryx-theme-local-root-control-height)"}}}"#
        );
    }

    #[test]
    fn a_rule_cannot_enroll_or_misroute_a_local_token() {
        assert_eq!(
            error(with_rules(
                "rule-enroll",
                vec![local_writes(
                    pointer("coarse"),
                    [("--rustryx-theme-rule-enroll-new-name", "44px")]
                )]
            )),
            "defineTheme(\"rule-enroll\").adaptations.rules[0].value.localTokens[\"--rustryx-theme-rule-enroll-new-name\"] cannot enroll a theme-local token. Declare it in the root theme or an exact enrolled base first."
        );
        assert_eq!(
            error(DefineThemeInput {
                local_tokens: Some(tokens([(LOCAL_NAME, "32px")])),
                ..with_rules(
                    "local-root",
                    vec![writes(pointer("coarse"), [(LOCAL_NAME, "44px")])]
                )
            }),
            "defineTheme(\"local-root\").adaptations.rules[0].value.tokens[\"--rustryx-theme-local-root-control-height\"] matches an enrolled theme-local declaration; write it through value.localTokens instead."
        );
        // defineTheme.test.ts: a local token the root leaves undefined is not
        // declared, so a rule cannot write it.
        assert_eq!(
            error(DefineThemeInput {
                local_tokens: Some([("--brand", TokenInput::Unset)].into()),
                ..with_rules(
                    "lost-local",
                    vec![local_writes(pointer("coarse"), [("--brand", "blue")])]
                )
            }),
            "defineTheme(\"lost-local\").adaptations.rules[0].value.localTokens[\"--brand\"] cannot enroll a theme-local token. Declare it in the root theme or an exact enrolled base first."
        );
    }

    #[test]
    fn treats_an_unenrolled_prefix_like_rule_token_as_portable() {
        let name = "--rustryx-theme-unrelated-owner-control-height";
        let theme = define(with_rules(
            "external-rule-token",
            vec![writes(pointer("coarse"), [(name, "44px")])],
        ));
        assert_eq!(entries(&rules(&theme)[0].tokens), [(name, "44px")]);
    }

    #[test]
    fn refuses_a_token_cycle_where_rules_match_together() {
        let local = |name: &str, root: Vec<(&str, &str)>, rules: Vec<ThemeAdaptationRule>| {
            error(DefineThemeInput {
                local_tokens: Some(
                    root.into_iter()
                        .map(|(name, value)| (name, TokenInput::Css(value.into())))
                        .collect(),
                ),
                ..with_rules(name, rules)
            })
        };
        let (a, b) = (
            "--rustryx-theme-cycle-local-a",
            "--rustryx-theme-cycle-local-b",
        );
        assert_eq!(
            local(
                "cycle-local",
                vec![(a, "1px"), (b, "var(--rustryx-theme-cycle-local-a)")],
                vec![local_writes(
                    pointer("coarse"),
                    [(a, "var(--rustryx-theme-cycle-local-b)")]
                )],
            ),
            "defineTheme(\"cycle-local\").adaptations rule [0] ((pointer: coarse)): Theme token cycle detected: --rustryx-theme-cycle-local-a -> --rustryx-theme-cycle-local-b -> --rustryx-theme-cycle-local-a."
        );

        // Only both rules together close it.
        let (a, b) = (
            "--rustryx-theme-overlap-cycle-a",
            "--rustryx-theme-overlap-cycle-b",
        );
        assert_eq!(
            local(
                "overlap-cycle",
                vec![(a, "1px"), (b, "2px")],
                vec![
                    local_writes(
                        pointer("coarse"),
                        [(a, "var(--rustryx-theme-overlap-cycle-b)")]
                    ),
                    local_writes(
                        contrast("more"),
                        [(b, "var(--rustryx-theme-overlap-cycle-a)")]
                    ),
                ],
            ),
            "defineTheme(\"overlap-cycle\").adaptations overlapping rules [0, 1] ((pointer: coarse); (prefers-contrast: more)): Theme token cycle detected: --rustryx-theme-overlap-cycle-b -> --rustryx-theme-overlap-cycle-a -> --rustryx-theme-overlap-cycle-b."
        );

        // Through a portable token.
        let a = "--rustryx-theme-portable-cycle-a";
        assert_eq!(
            error(DefineThemeInput {
                local_tokens: Some(tokens([(a, "1px")])),
                tokens: Some(tokens([("--color-background-body", "white")])),
                ..with_rules(
                    "portable-cycle",
                    vec![
                        writes(
                            pointer("coarse"),
                            [(
                                "--color-background-body",
                                "var(--rustryx-theme-portable-cycle-a)"
                            )]
                        ),
                        local_writes(contrast("more"), [(a, "var(--color-background-body)")]),
                    ]
                )
            }),
            "defineTheme(\"portable-cycle\").adaptations overlapping rules [0, 1] ((pointer: coarse); (prefers-contrast: more)): Theme token cycle detected: --rustryx-theme-portable-cycle-a -> --color-background-body -> --rustryx-theme-portable-cycle-a."
        );
        let a = "--rustryx-theme-single-portable-cycle-a";
        assert_eq!(
            error(DefineThemeInput {
                local_tokens: Some(tokens([(a, "1px")])),
                tokens: Some(tokens([("--color-background-body", "white")])),
                ..with_rules(
                    "single-portable-cycle",
                    vec![rule(
                        pointer("coarse"),
                        ThemeAdaptationValue {
                            tokens: Some(tokens([(
                                "--color-background-body",
                                "var(--rustryx-theme-single-portable-cycle-a)"
                            )])),
                            local_tokens: Some(tokens([(a, "var(--color-background-body)")])),
                            ..ThemeAdaptationValue::default()
                        }
                    )]
                )
            }),
            "defineTheme(\"single-portable-cycle\").adaptations rule [0] ((pointer: coarse)): Theme token cycle detected: --rustryx-theme-single-portable-cycle-a -> --color-background-body -> --rustryx-theme-single-portable-cycle-a."
        );
        assert_eq!(
            error(DefineThemeInput {
                tokens: Some(tokens([
                    ("--color-background-body", "white"),
                    ("--color-background-surface", "gray"),
                ])),
                ..with_rules(
                    "portable-only-cycle",
                    vec![
                        writes(
                            pointer("coarse"),
                            [("--color-background-body", "var(--color-background-surface)")]
                        ),
                        writes(
                            contrast("more"),
                            [("--color-background-surface", "var(--color-background-body)")]
                        ),
                    ]
                )
            }),
            "defineTheme(\"portable-only-cycle\").adaptations overlapping rules [0, 1] ((pointer: coarse); (prefers-contrast: more)): Theme token cycle detected: --color-background-surface -> --color-background-body -> --color-background-surface."
        );

        // A rule cycle that shares an edge with a root-only cycle.
        assert_eq!(
            error(DefineThemeInput {
                tokens: Some(tokens([
                    (
                        "--color-background-body",
                        "linear-gradient(var(--color-background-surface), var(--color-background-muted))"
                    ),
                    ("--color-background-surface", "var(--color-background-card)"),
                    ("--color-background-card", "var(--color-background-body)"),
                    ("--color-background-muted", "white"),
                ])),
                ..with_rules(
                    "shared-cycle-edge",
                    vec![writes(
                        pointer("coarse"),
                        [("--color-background-muted", "var(--color-background-card)")]
                    )]
                )
            }),
            "defineTheme(\"shared-cycle-edge\").adaptations rule [0] ((pointer: coarse)): Theme token cycle detected: --color-background-muted -> --color-background-card -> --color-background-body -> --color-background-muted."
        );

        // Three rules that no pair closes.
        let (a, b, c) = (
            "--rustryx-theme-three-way-cycle-a",
            "--rustryx-theme-three-way-cycle-b",
            "--rustryx-theme-three-way-cycle-c",
        );
        assert_eq!(
            local(
                "three-way-cycle",
                vec![(a, "1px"), (b, "2px"), (c, "3px")],
                vec![
                    local_writes(
                        pointer("coarse"),
                        [(a, "var(--rustryx-theme-three-way-cycle-b)")]
                    ),
                    local_writes(
                        contrast("more"),
                        [(b, "var(--rustryx-theme-three-way-cycle-c)")]
                    ),
                    local_writes(
                        motion("reduce"),
                        [(c, "var(--rustryx-theme-three-way-cycle-a)")]
                    ),
                ],
            ),
            "defineTheme(\"three-way-cycle\").adaptations overlapping rules [0, 1, 2] ((pointer: coarse); (prefers-contrast: more); (prefers-reduced-motion: reduce)): Theme token cycle detected: --rustryx-theme-three-way-cycle-c -> --rustryx-theme-three-way-cycle-a -> --rustryx-theme-three-way-cycle-b -> --rustryx-theme-three-way-cycle-c."
        );
    }

    #[test]
    fn does_not_attribute_an_unrelated_root_cycle_to_a_rule() {
        define(DefineThemeInput {
            tokens: Some(tokens([
                ("--color-background-body", "var(--color-background-surface)"),
                ("--color-background-surface", "var(--color-background-body)"),
            ])),
            ..with_rules(
                "unrelated-root-cycle",
                vec![writes(pointer("coarse"), [("--spacing-4", "12px")])],
            )
        });
    }

    #[test]
    fn allows_mutually_exclusive_writes_and_a_later_matching_repair() {
        let local = |name: &str, root: [(&str, &str); 2], rules: Vec<ThemeAdaptationRule>| {
            define(DefineThemeInput {
                local_tokens: Some(tokens(root)),
                ..with_rules(name, rules)
            })
        };
        let (a, b) = (
            "--rustryx-theme-exclusive-cycle-a",
            "--rustryx-theme-exclusive-cycle-b",
        );
        local(
            "exclusive-cycle",
            [(a, "1px"), (b, "2px")],
            vec![
                local_writes(
                    pointer("coarse"),
                    [(a, "var(--rustryx-theme-exclusive-cycle-b)")],
                ),
                local_writes(
                    pointer("fine"),
                    [(b, "var(--rustryx-theme-exclusive-cycle-a)")],
                ),
            ],
        );
        let (a, b) = (
            "--rustryx-theme-width-exclusive-cycle-a",
            "--rustryx-theme-width-exclusive-cycle-b",
        );
        local(
            "width-exclusive-cycle",
            [(a, "1px"), (b, "2px")],
            vec![
                local_writes(
                    width(None, Some("md")),
                    [(a, "var(--rustryx-theme-width-exclusive-cycle-b)")],
                ),
                local_writes(
                    width(Some("md"), None),
                    [(b, "var(--rustryx-theme-width-exclusive-cycle-a)")],
                ),
            ],
        );
        let (a, b) = (
            "--rustryx-theme-repaired-cycle-a",
            "--rustryx-theme-repaired-cycle-b",
        );
        local(
            "repaired-cycle",
            [(a, "1px"), (b, "2px")],
            vec![
                local_writes(
                    pointer("coarse"),
                    [(a, "var(--rustryx-theme-repaired-cycle-b)")],
                ),
                local_writes(
                    contrast("more"),
                    [(b, "var(--rustryx-theme-repaired-cycle-a)")],
                ),
                local_writes(
                    ThemeAdaptationCondition {
                        pointer: Some("coarse".into()),
                        contrast: Some("more".into()),
                        ..ThemeAdaptationCondition::default()
                    },
                    [(b, "4px")],
                ),
            ],
        );
        // A later co-matching rule repairs a cycle one rule closes.
        let (a, b) = (
            "--rustryx-theme-direct-repair-a",
            "--rustryx-theme-direct-repair-b",
        );
        local(
            "direct-repair",
            [(a, "1px"), (b, "var(--rustryx-theme-direct-repair-a)")],
            vec![
                local_writes(
                    pointer("coarse"),
                    [(a, "var(--rustryx-theme-direct-repair-b)")],
                ),
                local_writes(pointer("coarse"), [(b, "2px")]),
            ],
        );
    }

    #[test]
    fn checks_inherited_and_child_rules_in_one_reachable_cascade() {
        let (a, b) = (
            "--rustryx-theme-cycle-base-a",
            "--rustryx-theme-cycle-base-b",
        );
        let base = define(DefineThemeInput {
            local_tokens: Some(tokens([(a, "1px"), (b, "2px")])),
            ..with_rules(
                "cycle-base",
                vec![local_writes(
                    pointer("coarse"),
                    [(a, "var(--rustryx-theme-cycle-base-b)")],
                )],
            )
        });
        assert_eq!(
            error(DefineThemeInput {
                extends: Some(Arc::new(base)),
                ..with_rules(
                    "cycle-child",
                    vec![local_writes(
                        contrast("more"),
                        [(b, "var(--rustryx-theme-cycle-base-a)")]
                    )]
                )
            }),
            "defineTheme(\"cycle-child\").adaptations overlapping rules [0, 1] ((pointer: coarse); (prefers-contrast: more)): Theme token cycle detected: --rustryx-theme-cycle-base-b -> --rustryx-theme-cycle-base-a -> --rustryx-theme-cycle-base-b."
        );
    }

    // =========================================================================
    // Extension and built metadata
    // =========================================================================

    #[test]
    fn inherits_width_point_overrides_and_appends_child_rules() {
        let base = define(DefineThemeInput {
            adaptations: Some(ThemeAdaptations {
                width_breakpoints: points([("md", 800.0)]),
                rules: Some(vec![writes(
                    width(Some("md"), None),
                    [("--spacing-4", "18px")],
                )]),
            }),
            ..DefineThemeInput::new("base-order")
        });
        let child = define(DefineThemeInput {
            extends: Some(Arc::new(base)),
            adaptations: Some(ThemeAdaptations {
                width_breakpoints: points([("md", 820.0)]),
                rules: Some(vec![writes(
                    pointer("coarse"),
                    [("--size-element-md", "44px")],
                )]),
            }),
            ..DefineThemeInput::new("child-order")
        });
        let adaptations = child.adaptations.as_ref().unwrap();
        assert_eq!(adaptations.width_breakpoints.md, 820.0);
        assert_eq!(adaptations.rules.len(), 2);
        let queries: Vec<&str> = rules(&child).iter().map(|r| r.query.as_str()).collect();
        assert_eq!(queries, ["(width >= 820px)", "(pointer: coarse)"]);
    }

    #[test]
    fn re_resolves_inherited_rules_against_the_childs_root_axis() {
        let base = define(DefineThemeInput {
            typography: type_scale(14.0, 1.2),
            ..with_rules(
                "base-axis",
                vec![scale_rule(width(None, Some("sm")), Some(16.0), None)],
            )
        });
        assert_eq!(token(&rules(&base)[0], "--font-size-lg"), Some("1.1875rem"));
        let child = define(DefineThemeInput {
            extends: Some(Arc::new(base)),
            typography: type_scale(15.0, 1.25),
            ..DefineThemeInput::new("child-axis")
        });
        assert_eq!(token(&rules(&child)[0], "--font-size-lg"), Some("1.25rem"));
    }

    #[test]
    fn merges_a_childs_partial_typography_before_re_resolving_inherited_rules() {
        let base = define(DefineThemeInput {
            typography: Some(TypographyConfig {
                scale: Some(TypographyScale {
                    base: 14.0,
                    ratio: 1.25,
                }),
                body: family("Inter", Some("sans-serif")),
                heading: family("Playfair", Some("serif")),
                ..TypographyConfig::default()
            }),
            ..with_rules(
                "partial-typography-base",
                vec![scale_rule(width(Some("lg"), None), Some(18.0), None)],
            )
        });
        let child = define(DefineThemeInput {
            extends: Some(Arc::new(base)),
            typography: Some(TypographyConfig {
                body: family("Georgia", None),
                ..TypographyConfig::default()
            }),
            ..DefineThemeInput::new("partial-typography-child")
        });
        let typography = child.axes.as_ref().unwrap().typography.as_ref().unwrap();
        assert_eq!(
            typography.scale,
            Some(TypographyScale {
                base: 14.0,
                ratio: 1.25
            })
        );
        assert_eq!(typography.body, family("Georgia", None));
        assert_eq!(typography.heading, Some(TypographyRole::default()));
        assert_eq!(
            child
                .tokens
                .get("--font-family-heading")
                .map(String::as_str),
            Some("Georgia")
        );
        let rule = &rules(&child)[0];
        assert_eq!(token(rule, "--font-family-body"), Some("Georgia"));
        assert_eq!(token(rule, "--font-family-heading"), Some("Georgia"));
        assert_eq!(token(rule, "--font-size-lg"), Some("1.4375rem"));
    }

    fn lg_rule() -> ThemeAdaptationRule {
        scale_rule(width(Some("lg"), None), Some(18.0), None)
    }

    #[test]
    fn typography_weights_belong_to_the_last_config_that_declares_a_scale() {
        let weighted = define(DefineThemeInput {
            typography: Some(TypographyConfig {
                body: weight("medium"),
                heading: weight("bold"),
                ..type_scale(14.0, 1.25).unwrap()
            }),
            ..with_rules("weight-owner-base", vec![lg_rule()])
        });
        assert_eq!(
            token(&rules(&weighted)[0], "--text-body-weight"),
            Some("var(--font-weight-medium)")
        );
        let new_scale = define(DefineThemeInput {
            extends: Some(Arc::new(weighted)),
            typography: type_scale(16.0, 1.2),
            ..DefineThemeInput::new("weight-owner-new-scale")
        });
        let rule = &rules(&new_scale)[0];
        for (name, value) in [
            ("--text-body-weight", "var(--font-weight-normal)"),
            ("--text-heading-1-weight", "var(--font-weight-semibold)"),
        ] {
            assert_eq!(token(rule, name), Some(value), "{name}");
            assert_eq!(new_scale.tokens.get(name).map(String::as_str), Some(value));
        }

        let default_base = define(DefineThemeInput {
            typography: type_scale(14.0, 1.25),
            ..with_rules("weight-owner-default-base", vec![lg_rule()])
        });
        let weight_without_scale = define(DefineThemeInput {
            extends: Some(Arc::new(default_base)),
            typography: Some(TypographyConfig {
                heading: weight("bold"),
                ..TypographyConfig::default()
            }),
            ..DefineThemeInput::new("weight-owner-no-scale")
        });
        let value = Some("var(--font-weight-semibold)");
        assert_eq!(
            token(&rules(&weight_without_scale)[0], "--text-heading-1-weight"),
            value
        );
        assert_eq!(
            weight_without_scale
                .tokens
                .get("--text-heading-1-weight")
                .map(String::as_str),
            value
        );
    }

    #[test]
    fn takes_raw_typography_weights_through_an_extended_bases_rules() {
        // Upstream's test passes the weights as numbers, which the JSON reader
        // reads as their text; here they are that text.
        let base = define(DefineThemeInput {
            typography: type_scale(14.0, 1.25),
            ..with_rules("numeric-weight-base", vec![lg_rule()])
        });
        let child = define(DefineThemeInput {
            extends: Some(Arc::new(base)),
            typography: Some(TypographyConfig {
                heading: Some(TypographyRole {
                    weight: Some("700".into()),
                    weights: Some(vec![(2, "800".into())]),
                    ..TypographyRole::default()
                }),
                body: weight("450"),
                ..type_scale(16.0, 1.2).unwrap()
            }),
            ..DefineThemeInput::new("numeric-weight-child")
        });
        let rule = &rules(&child)[0];
        for (name, value) in [
            ("--text-heading-1-weight", "700"),
            ("--text-heading-2-weight", "800"),
            ("--text-body-weight", "450"),
        ] {
            assert_eq!(child.tokens.get(name).map(String::as_str), Some(value));
            assert_eq!(token(rule, name), Some(value), "{name}");
        }
    }

    #[test]
    fn ignores_a_child_fallback_with_no_family_as_the_root_does() {
        let base = define(DefineThemeInput {
            typography: Some(TypographyConfig {
                heading: family("Playfair", Some("sans-serif")),
                ..type_scale(14.0, 1.25).unwrap()
            }),
            ..with_rules("fallback-only-base", vec![lg_rule()])
        });
        let child = define(DefineThemeInput {
            extends: Some(Arc::new(base)),
            typography: Some(TypographyConfig {
                heading: Some(TypographyRole {
                    fallbacks: Some("serif".into()),
                    ..TypographyRole::default()
                }),
                ..TypographyConfig::default()
            }),
            ..DefineThemeInput::new("fallback-only-child")
        });
        let value = Some("Playfair, sans-serif");
        assert_eq!(
            child
                .tokens
                .get("--font-family-heading")
                .map(String::as_str),
            value
        );
        assert_eq!(token(&rules(&child)[0], "--font-family-heading"), value);
    }

    #[test]
    fn re_resolves_inherited_rules_with_the_childs_own_color_axis() {
        let base = define(DefineThemeInput {
            color: Some(ColorScaleConfig {
                accent: Some(AccentSeed::Single("#B7410E".into())),
                ..ColorScaleConfig::default()
            }),
            ..with_rules(
                "partial-color-base",
                vec![rule(
                    contrast("more"),
                    ThemeAdaptationValue {
                        color: Some(ThemeAdaptationColorConfig {
                            contrast: Some(ColorContrast::High),
                            ..ThemeAdaptationColorConfig::default()
                        }),
                        ..ThemeAdaptationValue::default()
                    },
                )],
            )
        });
        let warm = ColorScaleConfig {
            neutral_style: NeutralStyle::Warm,
            ..ColorScaleConfig::default()
        };
        let child = define(DefineThemeInput {
            extends: Some(Arc::new(base)),
            color: Some(warm.clone()),
            ..DefineThemeInput::new("partial-color-child")
        });
        let expected = define(DefineThemeInput {
            color: Some(ColorScaleConfig {
                contrast: ColorContrast::High,
                ..warm.clone()
            }),
            ..DefineThemeInput::new("partial-color-expected")
        });
        assert_eq!(child.axes.as_ref().unwrap().color, Some(warm));
        // The same 24 tokens in the same order.
        assert_eq!(expected.tokens.len(), 24);
        assert_eq!(entries(&rules(&child)[0].tokens), entries(&expected.tokens));
    }

    #[test]
    fn keeps_inherited_rules_above_the_childs_root_values() {
        let base = define(with_rules(
            "base-rule",
            vec![writes(pointer("coarse"), [("--spacing-4", "12px")])],
        ));
        let child = define(DefineThemeInput {
            extends: Some(Arc::new(base)),
            tokens: Some(tokens([("--spacing-4", "20px")])),
            ..DefineThemeInput::new("child-root")
        });
        assert_eq!(
            child.tokens.get("--spacing-4").map(String::as_str),
            Some("20px")
        );
        assert_eq!(token(&rules(&child)[0], "--spacing-4"), Some("12px"));
    }

    #[test]
    fn re_resolves_inherited_rules_from_built_theme_metadata() {
        let live = define(DefineThemeInput {
            typography: type_scale(14.0, 1.25),
            adaptations: Some(ThemeAdaptations {
                width_breakpoints: points([("sm", 700.0)]),
                rules: Some(vec![scale_rule(width(None, Some("sm")), Some(16.0), None)]),
            }),
            ..DefineThemeInput::new("built-base")
        });
        // A built module carries the normalized rules and the axes, not the
        // resolved rule writes.
        let built = DefinedTheme {
            name: live.name.clone(),
            built: true,
            tokens: live.tokens.clone(),
            components: live.components.clone(),
            adaptations: live.adaptations.clone(),
            axes: live.axes.clone(),
            ..DefinedTheme::default()
        };
        let child = define(DefineThemeInput {
            extends: Some(Arc::new(built)),
            ..DefineThemeInput::new("built-child")
        });
        let rule = &rules(&child)[0];
        assert_eq!(rule.query, "(width < 700px)");
        assert_eq!(token(rule, "--font-size-lg"), Some("1.25rem"));
    }

    #[test]
    fn keeps_script_token_reads_on_root_values() {
        let theme = define(DefineThemeInput {
            tokens: Some(tokens([("--spacing-4", "16px")])),
            ..with_rules(
                "css-first",
                vec![writes(pointer("coarse"), [("--spacing-4", "12px")])],
            )
        });
        assert_eq!(
            theme.tokens.get("--spacing-4").map(String::as_str),
            Some("16px")
        );
        assert_eq!(token(&rules(&theme)[0], "--spacing-4"), Some("12px"));
    }

    // =========================================================================
    // Motion in a rule (themeMotion.test.ts)
    // =========================================================================

    fn motion_rule(motion: ThemeMotionConfig) -> ThemeAdaptationRule {
        rule(
            pointer("coarse"),
            ThemeAdaptationValue {
                motion: Some(motion),
                ..ThemeAdaptationValue::default()
            },
        )
    }

    #[test]
    fn a_rule_sets_tempo_like_any_motion_value() {
        let theme = define(with_rules(
            "Adaptive",
            vec![motion_rule(ThemeMotionConfig {
                tempo: Some(0.8),
                ..ThemeMotionConfig::default()
            })],
        ));
        // Math.round(410 * 0.8) = 328 for the medium duration.
        assert_eq!(
            entries(&rules(&theme)[0].tokens),
            [
                ("--duration-fast-min", "104ms"),
                ("--duration-fast", "140ms"),
                ("--duration-fast-max", "184ms"),
                ("--duration-medium-min", "248ms"),
                ("--duration-medium", "328ms"),
                ("--duration-medium-max", "440ms"),
                ("--duration-slow-min", "584ms"),
                ("--duration-slow", "780ms"),
                ("--duration-slow-max", "1040ms"),
                ("--_astryx-pre-tempo-duration-fast-min", "130ms"),
                ("--_astryx-pre-tempo-duration-fast", "175ms"),
                ("--_astryx-pre-tempo-duration-fast-max", "230ms"),
                ("--_astryx-pre-tempo-duration-medium-min", "310ms"),
                ("--_astryx-pre-tempo-duration-medium", "410ms"),
                ("--_astryx-pre-tempo-duration-medium-max", "550ms"),
                ("--_astryx-pre-tempo-duration-slow-min", "730ms"),
                ("--_astryx-pre-tempo-duration-slow", "975ms"),
                ("--_astryx-pre-tempo-duration-slow-max", "1300ms"),
                ("--_astryx-motion-tempo", "0.8"),
            ]
        );
    }

    #[test]
    fn a_rules_own_durations_write_their_pre_tempo_copies() {
        let theme = define(with_rules(
            "AdaptiveDurations",
            vec![writes(pointer("coarse"), [("--duration-medium", "300ms")])],
        ));
        assert_eq!(
            entries(&rules(&theme)[0].tokens),
            [
                ("--duration-medium", "300ms"),
                ("--_astryx-pre-tempo-duration-medium", "300ms")
            ]
        );
    }

    #[test]
    fn a_rules_duration_scale_writes_the_tempo_it_plays_at() {
        let tempo = |name: &str, tempo: Option<f64>| {
            let theme = define(DefineThemeInput {
                motion: tempo.map(|tempo| ThemeMotionConfig {
                    tempo: Some(tempo),
                    ..ThemeMotionConfig::default()
                }),
                ..with_rules(
                    name,
                    vec![motion_rule(ThemeMotionConfig {
                        fast: Some(100.0),
                        medium: Some(250.0),
                        ratio: Some(0.75),
                        ..ThemeMotionConfig::default()
                    })],
                )
            });
            token(&rules(&theme)[0], "--_astryx-motion-tempo").map(str::to_string)
        };
        assert_eq!(tempo("AdaptiveScale", None).as_deref(), Some("1"));
        // Over a theme with a tempo, the rule keeps the theme's.
        assert_eq!(
            tempo("AdaptiveScaleTempo", Some(1.5)).as_deref(),
            Some("1.5")
        );
    }

    #[test]
    fn a_rule_at_standard_matches_the_themes_ease_standard() {
        let curve = "cubic-bezier(0.3, 0, 0.2, 1)";
        let theme = define(DefineThemeInput {
            tokens: Some(tokens([("--ease-standard", curve)])),
            ..with_rules(
                "EaseRule",
                vec![motion_rule(ThemeMotionConfig {
                    character: Some(super::super::theme_motion::MotionCharacter::Standard),
                    ..ThemeMotionConfig::default()
                })],
            )
        });
        let rule = &rules(&theme)[0];
        assert_eq!(token(rule, "--ease-entrance"), Some(curve));
        assert_eq!(token(rule, "--ease-move"), Some(curve));
        assert_eq!(token(rule, "--ease-standard"), None);
    }

    #[test]
    fn a_rules_explicit_ease_standard_keeps_the_themes_character() {
        let theme = define(DefineThemeInput {
            motion: Some(ThemeMotionConfig {
                character: Some(super::super::theme_motion::MotionCharacter::Expressive),
                ..ThemeMotionConfig::default()
            }),
            ..with_rules(
                "EaseRule",
                vec![writes(
                    pointer("coarse"),
                    [("--ease-standard", "cubic-bezier(0.4, 0, 0.6, 1)")],
                )],
            )
        });
        assert_eq!(
            entries(&rules(&theme)[0].tokens),
            [("--ease-standard", "cubic-bezier(0.4, 0, 0.6, 1)")]
        );
    }

    // =========================================================================
    // Token values in a rule (tokenValueCompat.test.ts)
    // =========================================================================

    #[test]
    fn accepts_the_documented_token_shapes_inside_a_rule() {
        let theme = define(DefineThemeInput {
            tokens: Some(tokens([("--color-background-body", "#fff")])),
            ..with_rules(
                "adaptive-ok",
                vec![rule(
                    pointer("coarse"),
                    ThemeAdaptationValue {
                        tokens: Some(
                            [
                                ("--size-element-md", TokenInput::Css("44px".into())),
                                ("--color-border", ["#000", "#fff"].into()),
                            ]
                            .into(),
                        ),
                        ..ThemeAdaptationValue::default()
                    },
                )],
            )
        });
        assert_eq!(
            entries(&rules(&theme)[0].tokens),
            [
                ("--size-element-md", "44px"),
                ("--color-border", "light-dark(#000, #fff)")
            ]
        );
    }

    // =========================================================================
    // The JSON reader's shape checks (upstream's normalization)
    // =========================================================================

    #[cfg(feature = "json")]
    #[test]
    fn the_json_reader_refuses_what_upstream_normalization_refuses() {
        let read = |json: serde_json::Value| {
            DefineThemeInput::from_json(&json)
                .and_then(|input| define_theme(&input))
                .unwrap_err()
                .0
        };
        let rule = |name: &str, rule: serde_json::Value| {
            read(serde_json::json!({"name": name, "adaptations": {"rules": [rule]}}))
        };
        assert_eq!(
            rule(
                "adaptive",
                serde_json::json!({"when": {"hover": true}, "value": {}})
            ),
            "defineTheme(\"adaptive\").adaptations.rules[0].when.hover is not supported."
        );
        assert_eq!(
            rule(
                "tuple-rule",
                serde_json::json!([{"pointer": "coarse"}, {"tokens": {"--spacing-4": "12px"}}])
            ),
            "defineTheme(\"tuple-rule\").adaptations.rules[0] must be an object."
        );
        assert_eq!(
            rule(
                "identity-in-rule",
                serde_json::json!({"when": {"pointer": "coarse"}, "value": {"icons": {}}})
            ),
            "defineTheme(\"identity-in-rule\").adaptations.rules[0].value.icons is not supported."
        );
        assert_eq!(
            rule("no-when", serde_json::json!({"value": {}})),
            "defineTheme(\"no-when\").adaptations.rules[0].when is required."
        );
        assert_eq!(
            rule(
                "no-value",
                serde_json::json!({"when": {"pointer": "coarse"}})
            ),
            "defineTheme(\"no-value\").adaptations.rules[0].value is required."
        );
        assert_eq!(
            rule(
                "Bad",
                serde_json::json!({"when": {"pointer": "coarse"}, "value": {"motion": {"reducedMotion": "system"}}})
            ),
            "defineTheme(\"Bad\").adaptations.rules[0].value.motion.reducedMotion is a theme-wide setting: set it on the theme's own motion, not in an adaptation rule."
        );
        // A value that is not a name is refused by the vocabulary, by name.
        assert_eq!(
            rule(
                "adaptive",
                serde_json::json!({"when": {"pointer": 1}, "value": {}})
            ),
            "defineTheme(\"adaptive\").adaptations.rules[0].when.pointer must be 'coarse' or 'fine'."
        );
        assert_eq!(
            read(serde_json::json!({"name": "x", "adaptations": {"rule": []}})),
            "defineTheme(\"x\").adaptations.rule is not supported."
        );
        assert_eq!(
            read(serde_json::json!({"name": "x", "adaptations": {"rules": {}}})),
            "defineTheme(\"x\").adaptations.rules must be an array of {when, value} objects."
        );
        // Unknown point names first, then the values.
        assert_eq!(
            read(
                serde_json::json!({"name": "bad-points", "adaptations": {"widthBreakpoints": {"md": "wide", "phone": 500}}})
            ),
            "defineTheme(\"bad-points\").adaptations.widthBreakpoints.phone is not supported."
        );
        assert_eq!(
            read(
                serde_json::json!({"name": "bad-points", "adaptations": {"widthBreakpoints": {"md": "wide"}}})
            ),
            "defineTheme(\"bad-points\").adaptations.widthBreakpoints.md must be a finite positive number of CSS pixels."
        );
    }
}
