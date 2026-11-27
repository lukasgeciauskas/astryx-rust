//! `define_theme`: a theme from a definition, the way upstream's `defineTheme`
//! builds one (`theme/defineTheme.ts`).
//!
//! A definition ([`DefineThemeInput`]) names a theme and gives only what it
//! changes: generative axes (a type scale and font roles, a color scale, a
//! radius scale, motion), explicit tokens, theme-local tokens, component
//! styles, a syntax theme and the overrides of its media surfaces.
//! [`define_theme`] resolves them, over the theme it `extends` when it has one,
//! into a [`DefinedTheme`]: the CSS values of the tokens the theme sets (core's
//! defaults fill the rest when the theme is read), its component styles, its
//! `onDark` and `onLight` surfaces, and what a theme extending it inherits.
//! [`resolve_theme_tokens`](super::theme_tokens::resolve_theme_tokens) reads its
//! tokens per mode, and [`DefinedTheme::to_theme`] makes it this crate's typed
//! [`crate::Theme`].
//!
//! Maps keep JavaScript's key order ([`OrderedMap`]): upstream's objects list
//! their keys in the order they were first set, and that order shows in the
//! resolved tokens and in the order of a theme's component rules.
//!
//! Upstream's icons and component icon slots are React nodes and functions; a
//! definition here carries them as values ([`ThemeIconEntry`], an SVG asset path
//! per name and variant, and [`ThemeComponentIcon`], a request per slot and state).
//! Indicators are not carried, the theme registry is [`super::registry`], and the
//! stylesheet is upstream's CSS runtime. Adaptations are validated over the
//! base's ([`DefinedTheme::adaptations`]) and every rule is lowered to the
//! writes it makes ([`DefinedTheme::adaptation_rules`], see
//! [`super::adaptations`]).

use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

use super::TOKEN_DEFAULTS;
use super::adaptations::{normalize_theme_adaptations, resolve_theme_adaptation_rules};
use super::color_scale::ColorScaleConfig;
use super::local_tokens::resolve_local_token_contract;
use super::merge_components::{ComponentStyleMap, StyleValue, deep_merge_components};
use super::on_media::{OnMediaOverrides, ResolvedOnMedia, resolve_on_media};
use super::palette::types::ThemePalette;
use super::radius_scale::RadiusScaleConfig;
use super::resolve_values::{
    ResolveThemeValuesOptions, ThemeValuesInput, ThemeValuesSeed, TypographyConfig,
    resolve_theme_values,
};
use super::theme_motion::ThemeMotionConfig;
use super::token_values::{ResolvedThemeMode, ThemeTokenValue};

// =============================================================================
// Ordered maps
// =============================================================================

/// A map that lists its keys in the order they were first set, as a
/// JavaScript object lists the keys a theme uses (none of which is an array
/// index). Setting a key that is already there replaces its value in place; a
/// new key goes last.
///
/// Two maps are equal when they hold the same keys with equal values, in any
/// order, as upstream's tests compare objects (`toEqual`).
#[derive(Clone)]
pub struct OrderedMap<V> {
    entries: Vec<(String, V)>,
    index: HashMap<String, usize>,
}

impl<V> Default for OrderedMap<V> {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            index: HashMap::new(),
        }
    }
}

impl<V> OrderedMap<V> {
    /// An empty map.
    pub fn new() -> Self {
        Self::default()
    }

    /// `map[key] = value`: a key already there keeps its place and takes the
    /// new value; a new key goes last.
    pub fn insert(&mut self, key: impl Into<String>, value: V) {
        let key = key.into();
        match self.index.get(&key) {
            Some(&at) => self.entries[at].1 = value,
            None => {
                self.index.insert(key.clone(), self.entries.len());
                self.entries.push((key, value));
            }
        }
    }

    /// The value at `key`.
    pub fn get(&self, key: &str) -> Option<&V> {
        self.index.get(key).map(|&at| &self.entries[at].1)
    }

    /// The value at `key`, to change.
    pub fn get_mut(&mut self, key: &str) -> Option<&mut V> {
        self.index.get(key).map(|&at| &mut self.entries[at].1)
    }

    /// Whether the map has `key` (upstream's `Object.hasOwn`).
    pub fn contains_key(&self, key: &str) -> bool {
        self.index.contains_key(key)
    }

    /// `delete map[key]`: the value that was there. The keys after it move up.
    pub fn remove(&mut self, key: &str) -> Option<V> {
        let at = self.index.remove(key)?;
        let (_, value) = self.entries.remove(at);
        for position in self.index.values_mut() {
            if *position > at {
                *position -= 1;
            }
        }
        Some(value)
    }

    /// The number of keys.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the map has no keys.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The keys and values, in order.
    pub fn iter(&self) -> impl DoubleEndedIterator<Item = (&str, &V)> + ExactSizeIterator {
        self.entries
            .iter()
            .map(|(key, value)| (key.as_str(), value))
    }

    /// The keys, in order.
    pub fn keys(&self) -> impl DoubleEndedIterator<Item = &str> + ExactSizeIterator {
        self.entries.iter().map(|(key, _)| key.as_str())
    }

    /// The values, in order.
    pub fn values(&self) -> impl DoubleEndedIterator<Item = &V> + ExactSizeIterator {
        self.entries.iter().map(|(_, value)| value)
    }

    /// The entries as pairs, in order.
    pub fn as_slice(&self) -> &[(String, V)] {
        &self.entries
    }

    /// Keeps the entries `keep` accepts, in order.
    pub fn retain(&mut self, mut keep: impl FnMut(&str, &V) -> bool) {
        let entries = std::mem::take(&mut self.entries);
        self.index.clear();
        for (key, value) in entries {
            if keep(&key, &value) {
                self.insert(key, value);
            }
        }
    }
}

impl<V: Clone> OrderedMap<V> {
    /// `Object.assign(map, other)`: every entry of `other` set on this map, in
    /// `other`'s order.
    pub fn assign(&mut self, other: &OrderedMap<V>) {
        for (key, value) in other.iter() {
            self.insert(key, value.clone());
        }
    }
}

impl<V: PartialEq> PartialEq for OrderedMap<V> {
    fn eq(&self, other: &Self) -> bool {
        self.len() == other.len()
            && self
                .iter()
                .all(|(key, value)| other.get(key) == Some(value))
    }
}

impl<V: fmt::Debug> fmt::Debug for OrderedMap<V> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_map().entries(self.iter()).finish()
    }
}

impl<K: Into<String>, V> FromIterator<(K, V)> for OrderedMap<V> {
    fn from_iter<I: IntoIterator<Item = (K, V)>>(iter: I) -> Self {
        let mut map = Self::new();
        map.extend(iter);
        map
    }
}

impl<K: Into<String>, V> Extend<(K, V)> for OrderedMap<V> {
    fn extend<I: IntoIterator<Item = (K, V)>>(&mut self, iter: I) {
        for (key, value) in iter {
            self.insert(key, value);
        }
    }
}

impl<K: Into<String>, V, const N: usize> From<[(K, V); N]> for OrderedMap<V> {
    fn from(entries: [(K, V); N]) -> Self {
        entries.into_iter().collect()
    }
}

impl<V> IntoIterator for OrderedMap<V> {
    type Item = (String, V);
    type IntoIter = std::vec::IntoIter<(String, V)>;

    fn into_iter(self) -> Self::IntoIter {
        self.entries.into_iter()
    }
}

/// Token names and CSS values in order: upstream's `Record<string, string>` of
/// a theme's resolved tokens.
pub type CssMap = OrderedMap<String>;

/// A token value as a definition writes it (upstream `TokenValue`, `string |
/// [light, dark]`), or a key left `undefined`, which upstream skips where it
/// reads a map but keeps where it spreads one map over another.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TokenInput {
    /// One CSS value for both modes (which may itself be a `light-dark()`).
    Css(String),
    /// A `[light, dark]` tuple.
    Pair(String, String),
    /// Upstream `undefined`: the key is there but sets nothing.
    Unset,
}

impl TokenInput {
    /// The value as a [`ThemeTokenValue`], or `None` for [`TokenInput::Unset`].
    pub fn value(&self) -> Option<ThemeTokenValue<'_>> {
        match self {
            TokenInput::Css(css) => Some(ThemeTokenValue::Css(css)),
            TokenInput::Pair(light, dark) => Some(ThemeTokenValue::Pair(light, dark)),
            TokenInput::Unset => None,
        }
    }
}

impl From<&str> for TokenInput {
    fn from(css: &str) -> Self {
        TokenInput::Css(css.to_string())
    }
}

impl From<String> for TokenInput {
    fn from(css: String) -> Self {
        TokenInput::Css(css)
    }
}

impl From<[&str; 2]> for TokenInput {
    fn from([light, dark]: [&str; 2]) -> Self {
        TokenInput::Pair(light.to_string(), dark.to_string())
    }
}

impl From<(&str, &str)> for TokenInput {
    fn from((light, dark): (&str, &str)) -> Self {
        TokenInput::Pair(light.to_string(), dark.to_string())
    }
}

/// Token names and the values a definition writes, in order: upstream's
/// `Partial<Record<TokenName, TokenValue>>`.
pub type TokenInputs = OrderedMap<TokenInput>;

/// Why a definition makes no theme: upstream's error message, word for word.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DefineThemeError(pub String);

impl DefineThemeError {
    /// An error with `message`.
    pub fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl fmt::Display for DefineThemeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for DefineThemeError {}

// =============================================================================
// Adaptations and generative axes
// =============================================================================

// The adaptation input types and the generative axes live with the rest of
// upstream's `theme/themeAdaptations.ts`; a definition names them from here.
pub use super::adaptations::{
    NormalizedThemeAdaptations, ResolvedThemeAdaptationRule, ThemeAdaptationColorConfig,
    ThemeAdaptationCondition, ThemeAdaptationRadiusConfig, ThemeAdaptationRule,
    ThemeAdaptationScale, ThemeAdaptationTypographyConfig, ThemeAdaptationValue,
    ThemeAdaptationWidthCondition, ThemeAdaptations, ThemeGenerativeAxes,
    resolve_theme_generative_axes,
};

/// `{...base, ...own}` of two motion configs: each field `own` sets, else
/// `base`'s.
pub(crate) fn merge_motion(base: &ThemeMotionConfig, own: &ThemeMotionConfig) -> ThemeMotionConfig {
    ThemeMotionConfig {
        fast: own.fast.or(base.fast),
        medium: own.medium.or(base.medium),
        slow: own.slow.or(base.slow),
        ratio: own.ratio.or(base.ratio),
        easing: own.easing.clone().or_else(|| base.easing.clone()),
        tempo: own.tempo.or(base.tempo),
        character: own.character.or(base.character),
    }
}

// =============================================================================
// Icons
// =============================================================================

/// The artwork a theme gives a semantic icon name (upstream `IconEntry`, the part of
/// it that is values): an SVG asset path, as the Icon component draws one
/// (`icons/lucide/x.svg`, or any path the app's asset source serves). Names are the
/// built-in ones (`close`, `chevronDown`) and namespaced ones
/// (`numberInput:stepperDown`).
#[derive(Clone, Debug, PartialEq)]
pub enum ThemeIconEntry {
    /// No artwork: the icon falls back to the built-in one (upstream a `null` entry).
    Fallback,
    /// One artwork for every request (upstream a node).
    Icon(String),
    /// Artwork by variant (upstream an entry function that draws by `variant`):
    /// `icon` for a request with no variant or one the entry cannot receive, and
    /// `variants` for the variants it draws differently. The entry receives a
    /// variant only when the theme's `icon_variants` lists it and, for an entry the
    /// theme inherits, so does the list of the theme that defined it.
    Variants {
        icon: String,
        variants: OrderedMap<String>,
    },
}

impl From<&str> for ThemeIconEntry {
    fn from(path: &str) -> Self {
        Self::Icon(path.to_string())
    }
}

impl From<String> for ThemeIconEntry {
    fn from(path: String) -> Self {
        Self::Icon(path)
    }
}

/// A theme's icon entries by name.
pub type ThemeIcons = OrderedMap<ThemeIconEntry>;

/// A partial request for a component icon slot (upstream `ComponentIconRequest`),
/// applied to the component's own icon field by field; a field left out keeps the
/// component's.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ThemeComponentIconRequest {
    /// The shared icon name to draw; `Some(None)` hides the icon.
    pub icon: Option<Option<String>>,
    /// The icon variant, one the theme's `icon_variants` lists.
    pub variant: Option<String>,
    /// The icon size: `xsm`, `sm`, `md` or `lg`.
    pub size: Option<String>,
}

/// What a theme gives a component's icon slot (upstream `StoredComponentIconValue`,
/// the part of it that is values), by the slot names `rustryx-components` lists
/// (`table-sort-indicator`, `table-row-expand`).
#[derive(Clone, Debug, PartialEq)]
pub enum ThemeComponentIcon {
    /// A shared icon name, which replaces the component's own (upstream a string).
    Icon(String),
    /// No icon (upstream `null`).
    Hidden,
    /// A partial request (upstream `{icon, variant, size}`).
    Request(ThemeComponentIconRequest),
    /// By the slot's state (upstream a slot function of its state): a request for
    /// each state it lists, `None` to hide the icon; a state it does not list
    /// changes nothing (the function returning `undefined`).
    ByState(OrderedMap<Option<ThemeComponentIconRequest>>),
}

/// A theme's component icon slots by slot name.
pub type ThemeComponentIcons = OrderedMap<ThemeComponentIcon>;

/// What [`define_theme`] resolves of a definition's icons.
struct ResolvedIcons {
    icons: Option<ThemeIcons>,
    icon_entry_variants: Option<OrderedMap<Option<Vec<String>>>>,
    component_icons: Option<ThemeComponentIcons>,
}

/// A definition's icons over its base's: entries by name, the definition's own
/// winning (upstream `defineTheme`'s `{...baseIcons, ...input.icons}`), the variant
/// lists inherited by-variant entries keep, and the component icon slots by slot.
fn resolve_icons(
    input: &DefineThemeInput,
    base: Option<&DefinedTheme>,
    icon_variants: Option<&Vec<String>>,
) -> ResolvedIcons {
    let base_icons = base.and_then(|base| base.icons.as_ref());
    let icons = match (&input.icons, base_icons) {
        (Some(own), Some(base_icons)) => {
            let mut merged = base_icons.clone();
            merged.assign(own);
            Some(merged)
        }
        (own, base_icons) => own.clone().or_else(|| base_icons.cloned()),
    };
    let icon_entry_variants =
        resolve_icon_entry_variants(icons.as_ref(), input.icons.as_ref(), base, icon_variants);
    let component_icons = merge_component_icons(
        base.and_then(|base| base.component_icons.as_ref()),
        input.component_icons.as_ref(),
    );
    ResolvedIcons {
        icons,
        icon_entry_variants,
        component_icons,
    }
}

/// For each by-variant entry this theme inherits, the variant list of the theme that
/// defined it, where that list is not this theme's own (upstream
/// `resolveIconEntryVariants`). Entries this theme defines use its own list, so they
/// need no record. Upstream compares the lists by identity; they are compared by
/// value here, which receives the same variants.
fn resolve_icon_entry_variants(
    entries: Option<&ThemeIcons>,
    own_entries: Option<&ThemeIcons>,
    base: Option<&DefinedTheme>,
    icon_variants: Option<&Vec<String>>,
) -> Option<OrderedMap<Option<Vec<String>>>> {
    let (entries, base) = (entries?, base?);
    let base_record = base.icon_entry_variants.as_ref();
    let mut record: Option<OrderedMap<Option<Vec<String>>>> = None;
    for (name, entry) in entries.iter() {
        if !matches!(entry, ThemeIconEntry::Variants { .. })
            || own_entries.is_some_and(|own| own.contains_key(name))
        {
            continue;
        }
        let defining = match base_record.and_then(|record| record.get(name)) {
            Some(list) => list.as_ref(),
            None => base.icon_variants.as_ref(),
        };
        if defining != icon_variants {
            record
                .get_or_insert_with(OrderedMap::new)
                .insert(name, defining.cloned());
        }
    }
    record
}

/// A theme's slot values: the base's, then this theme's own, by slot (upstream
/// `mergeComponentIcons`). An own entry replaces the inherited one whole.
fn merge_component_icons(
    inherited: Option<&ThemeComponentIcons>,
    own: Option<&ThemeComponentIcons>,
) -> Option<ThemeComponentIcons> {
    let Some(own) = own else {
        return inherited.cloned();
    };
    let mut merged = inherited.cloned().unwrap_or_default();
    merged.assign(own);
    Some(merged)
}

// =============================================================================
// Input and output
// =============================================================================

/// A theme definition: upstream `DefineThemeInput`, the part of it that is
/// values. Every field but `name` is optional; set only what the theme changes.
#[derive(Clone, Debug, Default)]
pub struct DefineThemeInput {
    /// The theme's name (upstream's `data-rustryx-theme` value).
    pub name: String,
    /// The theme this one starts from: everything it resolved to (tokens,
    /// component styles, surfaces, local tokens), under this definition.
    pub extends: Option<Arc<DefinedTheme>>,
    /// Fonts, type scale and weights.
    pub typography: Option<TypographyConfig>,
    /// Motion: the `tempo` and `character` knobs, a duration scale, or both.
    /// It inherits field by field from the theme it extends.
    pub motion: Option<ThemeMotionConfig>,
    /// The radius scale.
    pub radius: Option<RadiusScaleConfig>,
    /// The color scale, from an accent seed.
    pub color: Option<ColorScaleConfig>,
    /// Token overrides, the highest precedence of the theme's values.
    pub tokens: Option<TokenInputs>,
    /// Theme-family-local tokens: custom properties of the theme's own, which
    /// Astryx's components do not read.
    pub local_tokens: Option<TokenInputs>,
    /// Component style overrides, by component, style key and property.
    pub components: Option<ComponentStyleMap>,
    /// The icon variant names the theme supports; a theme that leaves them out
    /// inherits its base's.
    pub icon_variants: Option<Vec<String>>,
    /// The artwork for semantic icon names, by name (upstream `icons`); a name the
    /// theme sets replaces its base's, the others are inherited.
    pub icons: Option<ThemeIcons>,
    /// Which icon each component icon slot draws, by slot (upstream
    /// `componentIcons`); a slot the theme sets replaces its base's.
    pub component_icons: Option<ThemeComponentIcons>,
    /// The syntax theme's tokens (upstream `SyntaxThemeDefinition['tokens']`),
    /// by name without the `--color-syntax-` prefix.
    pub syntax: Option<CssMap>,
    /// Overrides for content on a dark surface.
    pub on_dark: Option<OnMediaOverrides>,
    /// Overrides for content on a light surface.
    pub on_light: Option<OnMediaOverrides>,
    /// Ordered environment-conditioned adaptations: width points and rules
    /// that change the theme's values where the environment matches.
    pub adaptations: Option<ThemeAdaptations>,
    /// Colors made by the palette generator: tokens and part styles under the
    /// theme's own `tokens` and `components`.
    pub palette: Option<ThemePalette>,
}

impl DefineThemeInput {
    /// A definition of the theme `name` that changes nothing.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ..Self::default()
        }
    }
}

/// A defined theme (upstream `DefinedTheme`, as `defineTheme` returns it).
///
/// [`define_theme`] makes one. A built theme, one upstream's theme build
/// compiled, carries only its name, its tokens and what it was built with:
/// make one from [`DefinedTheme::default`], setting [`DefinedTheme::built`].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DefinedTheme {
    /// The theme's name.
    pub name: String,
    /// The CSS value of every token the theme sets, in upstream's key order.
    /// Tokens it leaves out keep core's defaults.
    pub tokens: CssMap,
    /// The theme-local tokens, resolved (upstream `localTokens`).
    pub local_tokens: Option<CssMap>,
    /// The theme that first declared each local token (upstream
    /// `__localTokenOwners`).
    pub local_token_owners: Option<CssMap>,
    /// The themes of the local-token lineage, base first and this theme last
    /// (upstream `__localTokenLineage`); set once any theme in it declares
    /// local tokens.
    pub local_token_lineage: Option<Vec<String>>,
    /// Component style overrides.
    pub components: Option<ComponentStyleMap>,
    /// The icon variant names: the theme's own, or inherited.
    pub icon_variants: Option<Vec<String>>,
    /// The icon entries, the base's merged under the theme's own by name (upstream
    /// `__icons`, or `icons` when no entry is a function).
    pub icons: Option<ThemeIcons>,
    /// For each by-variant entry the theme inherits under another variant list, the
    /// list of the theme that defined it (upstream `__iconEntryVariants`).
    pub icon_entry_variants: Option<OrderedMap<Option<Vec<String>>>>,
    /// The component icon slots, the base's merged under the theme's own by slot
    /// (upstream `__componentIcons`, or `componentIcons`).
    pub component_icons: Option<ThemeComponentIcons>,
    /// Whether upstream's theme build compiled this theme (upstream `__built`).
    pub built: bool,
    /// The token values the definitions wrote, the base's, then the palette's,
    /// then the theme's own, with `[light, dark]` tuples kept (upstream
    /// `__inputTokens`).
    pub input_tokens: Option<TokenInputs>,
    /// The resolved dark surface (upstream `__onDark`).
    pub on_dark: Option<ResolvedOnMedia>,
    /// The resolved light surface (upstream `__onLight`).
    pub on_light: Option<ResolvedOnMedia>,
    /// The effective width points and every rule, the inherited ones first,
    /// validated (upstream `__adaptations`). [`define_theme`] always sets
    /// them; a built theme carries its build's, which a theme extending it
    /// re-resolves.
    pub adaptations: Option<NormalizedThemeAdaptations>,
    /// Every rule lowered to its concrete writes, in order (upstream
    /// `__adaptationRules`); `None` when there are no rules.
    pub adaptation_rules: Option<Vec<ResolvedThemeAdaptationRule>>,
    /// The effective generative axes (upstream `__axes`).
    pub axes: Option<ThemeGenerativeAxes>,
    /// Whether [`define_theme`] made this theme: upstream's `'__inputTokens' in
    /// theme`, which holds for every theme `defineTheme` returns.
    pub(crate) defined: bool,
}

// =============================================================================
// defineTheme
// =============================================================================

/// Whether a theme came from [`define_theme`] or a theme build, so it can be a
/// base. A plain object with a name and tokens is theme input, not a theme.
/// Upstream `isBuiltTheme`.
fn is_built_theme(theme: &DefinedTheme) -> bool {
    theme.defined || theme.built
}

/// The slow band's duration tokens. Upstream `SLOW_BAND`.
const SLOW_BAND: [&str; 3] = [
    "--duration-slow-min",
    "--duration-slow",
    "--duration-slow-max",
];

/// The base's tokens for the duration-scale fields a child leaves unset
/// (`spec:AST-108` FR5, field by field). Upstream `inheritedScaleTokens`.
///
/// A child that sets no `slow` of its own regenerates only the fast and medium
/// bands, so an inherited slow band stays as the base's tokens hold it, at the
/// ratio it was made with; a child that sets no `easing` keeps the base's
/// `--ease-standard`, an explicit one included. `None` when neither applies.
fn inherited_scale_tokens(
    base: &DefinedTheme,
    base_motion: &ThemeMotionConfig,
    own: &ThemeMotionConfig,
) -> Option<CssMap> {
    let mut names: Vec<&str> = Vec::new();
    if base_motion.slow.is_some() && own.slow.is_none() {
        names.extend(SLOW_BAND);
    }
    if base_motion.easing.is_some() && own.easing.is_none() {
        names.push("--ease-standard");
    }
    if names.is_empty() {
        return None;
    }
    Some(
        names
            .into_iter()
            .filter_map(|name| base.tokens.get(name).map(|value| (name, value.clone())))
            .collect(),
    )
}

/// Refuses a duration scale missing `fast`, `medium` or `ratio` after
/// inheritance. The scale applies only when all three are present, so a partial
/// one beside a knob would otherwise be dropped silently. Upstream
/// `assertCompleteMotionScale`.
fn assert_complete_motion_scale(
    motion: &ThemeMotionConfig,
    theme_name: &str,
) -> Result<(), DefineThemeError> {
    let writes_scale = motion.fast.is_some()
        || motion.medium.is_some()
        || motion.ratio.is_some()
        || motion.slow.is_some()
        || motion.easing.is_some();
    if writes_scale && (motion.fast.is_none() || motion.medium.is_none() || motion.ratio.is_none())
    {
        return Err(DefineThemeError(format!(
            "defineTheme(\"{theme_name}\"): `motion` sets a duration scale field, so it must supply `fast`, `medium`, and `ratio` unless each missing field is inherited from `extends`."
        )));
    }
    Ok(())
}

/// The definition's palette, refused unless the palette generator made it
/// (its `kind` is `theme-palette`). Upstream `readPalette`.
fn read_palette(input: &DefineThemeInput) -> Result<Option<&ThemePalette>, DefineThemeError> {
    match &input.palette {
        None => Ok(None),
        Some(palette) if palette.kind == "theme-palette" => Ok(Some(palette)),
        Some(_) => Err(DefineThemeError(format!(
            "defineTheme(\"{}\"): `palette` must be made by themePalette() from @astryxdesign/core/palette, got a plain object. Wrap the seeds: palette: themePalette({{accent: '#5b3cc4'}}).",
            input.name
        ))),
    }
}

/// The palette's tokens as definition values, each a `[light, dark]` tuple.
fn palette_tokens(palette: &ThemePalette) -> TokenInputs {
    palette
        .tokens
        .iter()
        .map(|(name, [light, dark])| (name.clone(), TokenInput::Pair(light.clone(), dark.clone())))
        .collect()
}

/// The palette's part styles under the theme's own: a part style the theme
/// sets wins, property by property. The palette writes each mode in its own
/// color-mode block, so a property the theme sets for both modes leaves those
/// blocks. Upstream `withPaletteComponents`.
fn with_palette_components(
    palette: &ThemePalette,
    own: Option<&ComponentStyleMap>,
) -> Option<ComponentStyleMap> {
    const PROPERTY: &str = "--color-text-secondary";
    let mut lifted = ComponentStyleMap::new();
    for (part, blocks) in &palette.components {
        let mut part_styles = ComponentStyleMap::new();
        for (block, lift) in blocks {
            let own_style = own
                .and_then(|own| own.get(part)?.block()?.get(block))
                .and_then(StyleValue::block);
            let set = own_style
                .and_then(|style| style.get(PROPERTY))
                .is_some_and(|value| !matches!(value, StyleValue::Unset));
            let mut style = ComponentStyleMap::new();
            if !set {
                for (mode, value) in [("@light", &lift.light), ("@dark", &lift.dark)] {
                    style.insert(
                        mode,
                        StyleValue::Block([(PROPERTY, StyleValue::Css(value.clone()))].into()),
                    );
                }
            }
            part_styles.insert(block.clone(), StyleValue::Block(style));
        }
        lifted.insert(part.clone(), StyleValue::Block(part_styles));
    }
    deep_merge_components(Some(&lifted), own)
}

/// `{...a, ...b}` of token maps.
fn spread(maps: &[Option<&TokenInputs>]) -> TokenInputs {
    let mut out = TokenInputs::new();
    for map in maps.iter().flatten() {
        out.assign(map);
    }
    out
}

/// A theme from a definition (upstream `defineTheme`).
///
/// Values resolve lowest first: the base theme's (when the definition
/// `extends` one), then the color scale, the type scale, the radius scale,
/// motion, the font families, the syntax theme, the palette and last the
/// definition's own `tokens` (see
/// [`resolve_theme_values`](super::resolve_values::resolve_theme_values)).
/// Components merge the same way, the type scale's generated heading and text
/// rules under the definition's own. Motion inherits field by field: a
/// duration-scale field or knob the definition leaves out keeps its base's.
///
/// Adaptations inherit the base's width points and rules, the definition's own
/// appended; every rule is re-resolved against this theme's effective axes
/// ([`normalize_theme_adaptations`], [`resolve_theme_adaptation_rules`]).
///
/// Fails, with upstream's message, on a base that is not a theme, a palette
/// the generator did not make, a duration scale missing `fast`, `medium` or
/// `ratio`, a local token that is misnamed, also in `tokens` or in a
/// reference cycle, and adaptations that do not validate or resolve.
pub fn define_theme(input: &DefineThemeInput) -> Result<DefinedTheme, DefineThemeError> {
    // A base that is not a theme is refused rather than ignored.
    if let Some(base) = &input.extends
        && !is_built_theme(base)
    {
        return Err(DefineThemeError(format!(
            "defineTheme(\"{}\"): `extends` must be a theme from defineTheme(), got a plain object named \"{}\", which is theme input, not a theme. Pass it through defineTheme() first (extends: defineTheme({{...}})), or import the theme its own file defines.",
            input.name, base.name
        )));
    }
    let base = input.extends.as_deref();
    let palette = read_palette(input)?;

    // Motion inherits field by field (spec:AST-108 FR5): what the child
    // generates replaces the base's tokens; an inherited slow band and an
    // inherited easing keep the base's tokens.
    let base_motion = base.and_then(|base| base.axes.as_ref()?.motion.as_ref());
    let motion = match (&input.motion, base_motion) {
        (Some(own), Some(base_motion)) => Some(merge_motion(base_motion, own)),
        (own, _) => own.clone(),
    };
    if let Some(motion) = &motion {
        assert_complete_motion_scale(motion, &input.name)?;
    }
    let inherited_scale = match (&input.motion, base, base_motion) {
        (Some(own), Some(base), Some(base_motion)) => {
            inherited_scale_tokens(base, base_motion, own)
        }
        _ => None,
    };
    let palette_inputs = palette.map(palette_tokens);
    let tokens = match &inherited_scale {
        Some(inherited) if !inherited.is_empty() => {
            let inherited: TokenInputs = inherited
                .iter()
                .map(|(name, value)| (name, TokenInput::Css(value.clone())))
                .collect();
            Some(spread(&[
                Some(&inherited),
                palette_inputs.as_ref(),
                input.tokens.as_ref(),
            ]))
        }
        _ if palette_inputs.is_some() => {
            Some(spread(&[palette_inputs.as_ref(), input.tokens.as_ref()]))
        }
        _ => input.tokens.clone(),
    };
    let components = match palette {
        Some(palette) => with_palette_components(palette, input.components.as_ref()),
        None => input.components.clone(),
    };
    let own_values = ThemeValuesInput {
        typography: input.typography.clone(),
        color: input.color.clone(),
        radius: input.radius,
        motion,
        syntax: input.syntax.clone(),
        tokens,
        components,
    };
    let own_axes = ThemeGenerativeAxes {
        typography: input.typography.clone(),
        color: input.color.clone(),
        radius: input.radius,
        motion: input.motion.clone(),
    };
    let seed = base.map(|base| ThemeValuesSeed {
        tokens: Some(base.tokens.clone()),
        components: base.components.clone(),
    });

    // An explicit --ease-standard follows the character the theme inherits
    // when it sets no motion of its own (FR2).
    let resolved = resolve_theme_values(
        &own_values,
        seed.as_ref(),
        &ResolveThemeValuesOptions {
            inherited_character: base_motion.and_then(|motion| motion.character),
            ..ResolveThemeValuesOptions::default()
        },
    );

    // The media surfaces: the defaults, then the base's resolved surface, then
    // this theme's own overrides.
    let on_dark = resolve_on_media(
        ResolvedThemeMode::Dark,
        input.on_dark.as_ref(),
        base.and_then(|base| base.on_dark.as_ref()),
    );
    let on_light = resolve_on_media(
        ResolvedThemeMode::Light,
        input.on_light.as_ref(),
        base.and_then(|base| base.on_light.as_ref()),
    );

    let local_token_contract = resolve_local_token_contract(
        &input.name,
        input.local_tokens.as_ref(),
        base,
        &resolved.tokens,
        TOKEN_DEFAULTS,
    )?;

    // Adaptations inherit their width points and ordered rules. Every rule is
    // re-resolved against this theme's effective root axes, so a child can
    // change the root scale while keeping the base rule's authored intent.
    let axes = resolve_theme_generative_axes(base.and_then(|base| base.axes.as_ref()), &own_axes);
    let adaptations = normalize_theme_adaptations(
        &input.name,
        base.and_then(|base| base.adaptations.as_ref()),
        input.adaptations.as_ref(),
    )?;
    let adaptation_rules = resolve_theme_adaptation_rules(
        &input.name,
        &adaptations,
        &axes,
        &resolved.tokens,
        local_token_contract
            .as_ref()
            .map(|contract| &contract.local_tokens),
        resolved.components.as_ref(),
    )?;

    // The variant list is theme-level: a list this theme sets replaces the
    // inherited one; an omitted list is inherited.
    let icon_variants = input
        .icon_variants
        .clone()
        .or_else(|| base.and_then(|base| base.icon_variants.clone()));

    // Icons: the definition's entries over the base's by name, and its component
    // icon slots over the base's by slot.
    let ResolvedIcons {
        icons,
        icon_entry_variants,
        component_icons,
    } = resolve_icons(input, base, icon_variants.as_ref());

    let base_input_tokens = base.and_then(|base| base.input_tokens.as_ref());
    let input_tokens = (base_input_tokens.is_some() || palette.is_some() || input.tokens.is_some())
        .then(|| {
            spread(&[
                base_input_tokens,
                palette_inputs.as_ref(),
                input.tokens.as_ref(),
            ])
        });

    let (local_tokens, local_token_owners, local_token_lineage) = match local_token_contract {
        Some(contract) => (
            Some(contract.local_tokens),
            Some(contract.owners),
            Some(contract.lineage),
        ),
        None => (None, None, None),
    };
    Ok(DefinedTheme {
        name: input.name.clone(),
        tokens: resolved.tokens,
        local_tokens,
        local_token_owners,
        local_token_lineage,
        components: resolved.components,
        icon_variants,
        icons,
        icon_entry_variants,
        component_icons,
        built: false,
        input_tokens,
        on_dark: Some(on_dark),
        on_light: Some(on_light),
        adaptations: Some(adaptations),
        adaptation_rules,
        axes: Some(axes),
        defined: true,
    })
}

// =============================================================================
// Definitions as JSON
// =============================================================================

#[cfg(feature = "json")]
mod json {
    use std::sync::Arc;

    use serde_json::{Map, Value};

    use super::super::color_scale::{AccentSeed, ColorContrast, ColorScaleConfig, NeutralStyle};
    use super::super::js::js_number_to_string;
    use super::super::merge_components::{ComponentStyleMap, StyleMap, StyleValue};
    use super::super::on_media::OnMediaOverrides;
    use super::super::palette::types::{PaletteLift, PaletteRecipe, ThemePalette};
    use super::super::radius_scale::RadiusScaleConfig;
    use super::super::resolve_values::{TypographyConfig, TypographyRole, TypographyScale};
    use super::super::theme_motion::{MotionCharacter, ThemeMotionConfig};
    use super::{
        CssMap, DefineThemeError, DefineThemeInput, DefinedTheme, OrderedMap,
        ThemeAdaptationColorConfig, ThemeAdaptationCondition, ThemeAdaptationRadiusConfig,
        ThemeAdaptationRule, ThemeAdaptationScale, ThemeAdaptationTypographyConfig,
        ThemeAdaptationValue, ThemeAdaptationWidthCondition, ThemeAdaptations, TokenInput,
        TokenInputs,
    };

    type Result<T> = std::result::Result<T, DefineThemeError>;

    /// The error for a value of the wrong shape at `path`.
    fn shape(path: &str, expected: &str) -> DefineThemeError {
        DefineThemeError(format!("{path} must be {expected}."))
    }

    /// `object[key]`, `None` when it is missing or `null` (as JavaScript's
    /// `if (value)` reads an object field).
    fn field<'a>(object: &'a Map<String, Value>, key: &str) -> Option<&'a Value> {
        object.get(key).filter(|value| !value.is_null())
    }

    fn object<'a>(value: &'a Value, path: &str) -> Result<&'a Map<String, Value>> {
        value.as_object().ok_or_else(|| shape(path, "an object"))
    }

    fn string(value: &Value, path: &str) -> Result<String> {
        value
            .as_str()
            .map(str::to_string)
            .ok_or_else(|| shape(path, "a string"))
    }

    fn number(value: &Value, path: &str) -> Result<f64> {
        value.as_f64().ok_or_else(|| shape(path, "a number"))
    }

    fn opt<T>(
        object: &Map<String, Value>,
        key: &str,
        path: &str,
        read: impl FnOnce(&Value, &str) -> Result<T>,
    ) -> Result<Option<T>> {
        field(object, key)
            .map(|value| read(value, &format!("{path}.{key}")))
            .transpose()
    }

    /// A token value: a string, or a `[light, dark]` pair of strings.
    fn token_input(value: &Value, path: &str) -> Result<TokenInput> {
        match value {
            Value::String(css) => Ok(TokenInput::Css(css.clone())),
            Value::Array(pair) => match pair.as_slice() {
                [Value::String(light), Value::String(dark)] => {
                    Ok(TokenInput::Pair(light.clone(), dark.clone()))
                }
                _ => Err(shape(path, "a CSS string or a [light, dark] string tuple")),
            },
            _ => Err(shape(path, "a CSS string or a [light, dark] string tuple")),
        }
    }

    fn token_inputs(value: &Value, path: &str) -> Result<TokenInputs> {
        object(value, path)?
            .iter()
            .map(|(name, value)| {
                Ok((
                    name.clone(),
                    token_input(value, &format!("{path}[\"{name}\"]"))?,
                ))
            })
            .collect()
    }

    fn css_map(value: &Value, path: &str) -> Result<CssMap> {
        object(value, path)?
            .iter()
            .map(|(name, value)| Ok((name.clone(), string(value, &format!("{path}[\"{name}\"]"))?)))
            .collect()
    }

    /// A component style value: a string (a number or boolean as its
    /// JavaScript text), `null`, or a block.
    fn style_value(value: &Value, path: &str) -> Result<StyleValue> {
        Ok(match value {
            Value::String(css) => StyleValue::Css(css.clone()),
            Value::Number(n) => {
                StyleValue::Css(js_number_to_string(n.as_f64().unwrap_or(f64::NAN)))
            }
            Value::Bool(b) => StyleValue::Css(b.to_string()),
            Value::Null => StyleValue::Null,
            Value::Object(map) => StyleValue::Block(style_map(map, path)?),
            Value::Array(_) => return Err(shape(path, "a CSS value or a style block")),
        })
    }

    fn style_map(map: &Map<String, Value>, path: &str) -> Result<StyleMap> {
        map.iter()
            .map(|(key, value)| {
                Ok((
                    key.clone(),
                    style_value(value, &format!("{path}[\"{key}\"]"))?,
                ))
            })
            .collect()
    }

    fn components(value: &Value, path: &str) -> Result<ComponentStyleMap> {
        style_map(object(value, path)?, path)
    }

    /// A font weight: a name or CSS value, or a number as its text.
    fn weight(value: &Value, path: &str) -> Result<String> {
        match value {
            Value::Number(n) => Ok(js_number_to_string(n.as_f64().unwrap_or(f64::NAN))),
            other => string(other, path),
        }
    }

    fn role(value: &Value, path: &str) -> Result<TypographyRole> {
        let role = object(value, path)?;
        let weights = opt(role, "weights", path, |value, path| {
            object(value, path)?
                .iter()
                .map(|(level, w)| {
                    let at = format!("{path}[\"{level}\"]");
                    let level = level
                        .parse::<u8>()
                        .map_err(|_| shape(&at, "keyed by a heading level"))?;
                    Ok((level, weight(w, &at)?))
                })
                .collect::<Result<Vec<_>>>()
        })?;
        Ok(TypographyRole {
            family: opt(role, "family", path, string)?,
            fallbacks: opt(role, "fallbacks", path, string)?,
            weight: opt(role, "weight", path, weight)?,
            weights,
        })
    }

    fn typography(value: &Value, path: &str) -> Result<TypographyConfig> {
        let typo = object(value, path)?;
        let scale = opt(typo, "scale", path, |value, path| {
            let scale = object(value, path)?;
            Ok(TypographyScale {
                base: number(
                    scale.get("base").unwrap_or(&Value::Null),
                    &format!("{path}.base"),
                )?,
                ratio: number(
                    scale.get("ratio").unwrap_or(&Value::Null),
                    &format!("{path}.ratio"),
                )?,
            })
        })?;
        Ok(TypographyConfig {
            scale,
            body: opt(typo, "body", path, role)?,
            heading: opt(typo, "heading", path, role)?,
            code: opt(typo, "code", path, role)?,
        })
    }

    /// A motion config. A `character` upstream does not name counts as unset,
    /// as upstream's knobs read it.
    fn motion(value: &Value, path: &str) -> Result<ThemeMotionConfig> {
        let motion = object(value, path)?;
        let character = opt(motion, "character", path, string)?;
        Ok(ThemeMotionConfig {
            fast: opt(motion, "fast", path, number)?,
            medium: opt(motion, "medium", path, number)?,
            slow: opt(motion, "slow", path, number)?,
            ratio: opt(motion, "ratio", path, number)?,
            easing: opt(motion, "easing", path, string)?,
            tempo: opt(motion, "tempo", path, number)?,
            character: character.as_deref().and_then(MotionCharacter::from_name),
        })
    }

    /// An accent seed: a hex string, or a `[light, dark]` pair.
    fn accent(value: &Value, path: &str) -> Result<AccentSeed> {
        match value {
            Value::String(seed) => Ok(AccentSeed::Single(seed.clone())),
            Value::Array(pair) => match pair.as_slice() {
                [Value::String(light), Value::String(dark)] => {
                    Ok(AccentSeed::Pair(light.clone(), dark.clone()))
                }
                _ => Err(shape(path, "a hex string or a [light, dark] pair of them")),
            },
            _ => Err(shape(path, "a hex string or a [light, dark] pair of them")),
        }
    }

    /// A neutral style; a name upstream does not list expands as `cool` does.
    fn neutral_style(value: &Value, path: &str) -> Result<NeutralStyle> {
        Ok(match string(value, path)?.as_str() {
            "warm" => NeutralStyle::Warm,
            "neutral" => NeutralStyle::Neutral,
            _ => NeutralStyle::Cool,
        })
    }

    /// A contrast level; anything but `high` expands as `standard`.
    fn contrast(value: &Value, path: &str) -> Result<ColorContrast> {
        Ok(match string(value, path)?.as_str() {
            "high" => ColorContrast::High,
            _ => ColorContrast::Standard,
        })
    }

    fn color(value: &Value, path: &str) -> Result<ThemeAdaptationColorConfig> {
        let color = object(value, path)?;
        Ok(ThemeAdaptationColorConfig {
            accent: opt(color, "accent", path, accent)?,
            neutral_style: opt(color, "neutralStyle", path, neutral_style)?,
            contrast: opt(color, "contrast", path, contrast)?,
        })
    }

    fn radius(value: &Value, path: &str) -> Result<ThemeAdaptationRadiusConfig> {
        let radius = object(value, path)?;
        Ok(ThemeAdaptationRadiusConfig {
            base: opt(radius, "base", path, number)?,
            multiplier: opt(radius, "multiplier", path, number)?,
        })
    }

    fn surface(value: &Value, path: &str) -> Result<OnMediaOverrides> {
        let surface = object(value, path)?;
        Ok(OnMediaOverrides {
            tokens: opt(surface, "tokens", path, token_inputs)?,
            components: opt(surface, "components", path, components)?,
        })
    }

    /// Fails on a key of `map` that is not one of `allowed`, as upstream's
    /// `assertAllowedKeys` refuses one.
    fn allowed_keys(map: &Map<String, Value>, allowed: &[&str], path: &str) -> Result<()> {
        match map.keys().find(|key| !allowed.contains(&key.as_str())) {
            Some(key) => Err(DefineThemeError(format!("{path}.{key} is not supported."))),
            None => Ok(()),
        }
    }

    /// A condition's named value: a string as it is, anything else as its
    /// JSON text, which the vocabulary check refuses by name.
    fn condition_value(value: &Value, _: &str) -> Result<String> {
        Ok(match value {
            Value::String(text) => text.clone(),
            other => other.to_string(),
        })
    }

    fn condition(value: &Value, path: &str) -> Result<ThemeAdaptationCondition> {
        let when = object(value, path)?;
        allowed_keys(when, &["width", "pointer", "contrast", "motion"], path)?;
        Ok(ThemeAdaptationCondition {
            width: opt(when, "width", path, |value, path| {
                let width = object(value, path)?;
                allowed_keys(width, &["from", "below"], path)?;
                Ok(ThemeAdaptationWidthCondition {
                    from: opt(width, "from", path, condition_value)?,
                    below: opt(width, "below", path, condition_value)?,
                })
            })?,
            pointer: opt(when, "pointer", path, condition_value)?,
            contrast: opt(when, "contrast", path, condition_value)?,
            motion: opt(when, "motion", path, condition_value)?,
        })
    }

    fn adaptation_value(value: &Value, path: &str) -> Result<ThemeAdaptationValue> {
        let rule = object(value, path)?;
        allowed_keys(
            rule,
            &[
                "typography",
                "color",
                "radius",
                "motion",
                "tokens",
                "localTokens",
                "components",
            ],
            path,
        )?;
        let typography = opt(rule, "typography", path, |value, path| {
            let typo = object(value, path)?;
            Ok(ThemeAdaptationTypographyConfig {
                scale: opt(typo, "scale", path, |value, path| {
                    let scale = object(value, path)?;
                    Ok(ThemeAdaptationScale {
                        base: opt(scale, "base", path, number)?,
                        ratio: opt(scale, "ratio", path, number)?,
                    })
                })?,
                body: opt(typo, "body", path, role)?,
                heading: opt(typo, "heading", path, role)?,
                code: opt(typo, "code", path, role)?,
            })
        })?;
        let motion = opt(rule, "motion", path, |value, path| {
            // Not a field a rule's motion has, but plain JavaScript may pass
            // it; upstream's completeMotion refuses it by name.
            if field(object(value, path)?, "reducedMotion").is_some() {
                return Err(DefineThemeError(format!(
                    "{path}.reducedMotion is a theme-wide setting: set it on the theme's own motion, not in an adaptation rule."
                )));
            }
            motion(value, path)
        })?;
        Ok(ThemeAdaptationValue {
            typography,
            color: opt(rule, "color", path, color)?,
            radius: opt(rule, "radius", path, radius)?,
            motion,
            tokens: opt(rule, "tokens", path, token_inputs)?,
            local_tokens: opt(rule, "localTokens", path, token_inputs)?,
            components: opt(rule, "components", path, components)?,
        })
    }

    /// A definition's adaptations, refused where upstream's normalization
    /// refuses their shape: an unknown key, a rule that is not a `{when,
    /// value}` object or lacks either. A width point that is not a number
    /// reads as `NaN`, which [`normalize_theme_adaptations`] refuses after
    /// the unknown names, as upstream checks them.
    ///
    /// [`normalize_theme_adaptations`]: super::super::adaptations::normalize_theme_adaptations
    fn adaptations(value: &Value, path: &str) -> Result<ThemeAdaptations> {
        let adaptations = object(value, path)?;
        allowed_keys(adaptations, &["widthBreakpoints", "rules"], path)?;
        let width_breakpoints = opt(adaptations, "widthBreakpoints", path, |value, path| {
            Ok(object(value, path)?
                .iter()
                .filter(|(_, px)| !px.is_null())
                .map(|(name, px)| (name.clone(), px.as_f64().unwrap_or(f64::NAN)))
                .collect::<OrderedMap<f64>>())
        })?;
        let rules = opt(adaptations, "rules", path, |value, path| {
            value
                .as_array()
                .ok_or_else(|| shape(path, "an array of {when, value} objects"))?
                .iter()
                .enumerate()
                .map(|(index, rule)| {
                    let at = format!("{path}[{index}]");
                    let rule_object = object(rule, &at)?;
                    allowed_keys(rule_object, &["when", "value"], &at)?;
                    let required = |key: &str| {
                        rule_object
                            .get(key)
                            .ok_or_else(|| DefineThemeError(format!("{at}.{key} is required.")))
                    };
                    Ok(ThemeAdaptationRule {
                        when: condition(required("when")?, &format!("{at}.when"))?,
                        value: adaptation_value(required("value")?, &format!("{at}.value"))?,
                    })
                })
                .collect::<Result<Vec<_>>>()
        })?;
        Ok(ThemeAdaptations {
            width_breakpoints,
            rules,
        })
    }

    fn palette(value: &Value, path: &str, name: &str) -> Result<ThemePalette> {
        let palette = object(value, path)?;
        if palette.get("kind").and_then(Value::as_str) != Some("theme-palette")
            || !palette.get("tokens").is_some_and(Value::is_object)
        {
            return Err(DefineThemeError(format!(
                "defineTheme(\"{name}\"): `palette` must be made by themePalette() from @astryxdesign/core/palette, got a plain object. Wrap the seeds: palette: themePalette({{accent: '#5b3cc4'}})."
            )));
        }
        let recipe = PaletteRecipe::parse(&string(
            palette.get("recipe").unwrap_or(&Value::Null),
            &format!("{path}.recipe"),
        )?)
        .map_err(|error| DefineThemeError(error.0))?;
        let tokens = object(&palette["tokens"], &format!("{path}.tokens"))?
            .iter()
            .map(|(token, value)| {
                match token_input(value, &format!("{path}.tokens[\"{token}\"]"))? {
                    TokenInput::Pair(light, dark) => Ok((token.clone(), [light, dark])),
                    _ => Err(shape(
                        &format!("{path}.tokens[\"{token}\"]"),
                        "a [light, dark] string tuple",
                    )),
                }
            })
            .collect::<Result<Vec<_>>>()?;
        let mut parts = Vec::new();
        if let Some(components) = field(palette, "components") {
            for (part, blocks) in object(components, &format!("{path}.components"))? {
                let mut lifts = Vec::new();
                for (block, lift) in object(blocks, &format!("{path}.components.{part}"))? {
                    let at = format!("{path}.components.{part}[\"{block}\"]");
                    let side = |mode: &str| -> Result<String> {
                        string(
                            &lift[mode]["--color-text-secondary"],
                            &format!("{at}[\"{mode}\"][\"--color-text-secondary\"]"),
                        )
                    };
                    lifts.push((
                        block.clone(),
                        PaletteLift {
                            light: side("@light")?,
                            dark: side("@dark")?,
                        },
                    ));
                }
                parts.push((part.clone(), lifts));
            }
        }
        Ok(ThemePalette {
            kind: "theme-palette",
            recipe,
            tokens,
            components: parts,
        })
    }

    impl DefineThemeInput {
        /// A definition read from the JSON the theme export writes to
        /// `spec/theme-definitions/*.json`: the JSON-able part of upstream's
        /// `defineTheme` input. Functions and React nodes (icons, indicators)
        /// are not in it, and a definition that `extends` another names it, as
        /// `{"name": …}`; this fails for one, since it has no base to hand over
        /// (see [`DefineThemeInput::from_json_with_bases`]).
        ///
        /// Keys are read in the JSON's order, which in the export's files is
        /// sorted. A number in a style or a weight reads as its JavaScript
        /// text. `"localTokens": null` fails as upstream refuses a
        /// `localTokens` field that holds `undefined`.
        pub fn from_json(json: &Value) -> Result<Self> {
            Self::from_json_with_bases(json, |_| None)
        }

        /// [`DefineThemeInput::from_json`], with `base` looking up the theme
        /// a definition's `extends` names.
        pub fn from_json_with_bases(
            json: &Value,
            base: impl Fn(&str) -> Option<Arc<DefinedTheme>>,
        ) -> Result<Self> {
            let root = object(json, "theme definition")?;
            let name = string(
                root.get("name").unwrap_or(&Value::Null),
                "theme definition.name",
            )?;
            let path = format!("defineTheme(\"{name}\")");
            let extends = match field(root, "extends") {
                None => None,
                Some(value) => {
                    let base_name = string(
                        object(value, &format!("{path}.extends"))?
                            .get("name")
                            .unwrap_or(&Value::Null),
                        &format!("{path}.extends.name"),
                    )?;
                    Some(base(&base_name).ok_or_else(|| {
                        DefineThemeError(format!(
                            "{path}: `extends` names the theme \"{base_name}\", which is not available here."
                        ))
                    })?)
                }
            };
            if root.get("localTokens").is_some_and(Value::is_null) {
                return Err(DefineThemeError(format!(
                    "{path}: localTokens is undefined: omit the field or pass an object."
                )));
            }
            let color = opt(root, "color", &path, color)?.map(|color| ColorScaleConfig {
                accent: color.accent,
                neutral_style: color.neutral_style.unwrap_or_default(),
                contrast: color.contrast.unwrap_or_default(),
            });
            let radius = opt(root, "radius", &path, |value, path| {
                let radius = object(value, path)?;
                Ok(RadiusScaleConfig {
                    base: number(
                        radius.get("base").unwrap_or(&Value::Null),
                        &format!("{path}.base"),
                    )?,
                    multiplier: number(
                        radius.get("multiplier").unwrap_or(&Value::Null),
                        &format!("{path}.multiplier"),
                    )?,
                })
            })?;
            let syntax = opt(root, "syntax", &path, |value, path| {
                let syntax = object(value, path)?;
                css_map(
                    syntax.get("tokens").unwrap_or(&Value::Null),
                    &format!("{path}.tokens"),
                )
            })?;
            let icon_variants = opt(root, "iconVariants", &path, |value, path| {
                value
                    .as_array()
                    .ok_or_else(|| shape(path, "an array"))?
                    .iter()
                    .map(|variant| string(variant, path))
                    .collect::<Result<Vec<_>>>()
            })?;
            Ok(DefineThemeInput {
                extends,
                typography: opt(root, "typography", &path, typography)?,
                motion: opt(root, "motion", &path, motion)?,
                radius,
                color,
                tokens: opt(root, "tokens", &path, token_inputs)?,
                local_tokens: opt(root, "localTokens", &path, token_inputs)?,
                components: opt(root, "components", &path, components)?,
                icon_variants,
                // The export writes no artwork: upstream's icons are React nodes.
                icons: None,
                component_icons: None,
                syntax,
                on_dark: opt(root, "onDark", &path, surface)?,
                on_light: opt(root, "onLight", &path, surface)?,
                adaptations: opt(root, "adaptations", &path, adaptations)?,
                palette: opt(root, "palette", &path, |value, path| {
                    palette(value, path, &name)
                })?,
                name,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::color_scale::{AccentSeed, ColorContrast, NeutralStyle};
    use super::super::merge_components::StyleMap;
    use super::super::resolve_values::{TypographyRole, TypographyScale};
    use super::super::status_roles::generated_scoped_status_role_defaults;
    use super::super::theme_motion::MotionCharacter;
    use super::super::theme_tokens::{resolve_theme_token, resolve_theme_tokens};
    use super::*;

    fn define(input: DefineThemeInput) -> DefinedTheme {
        define_theme(&input).unwrap()
    }

    fn error(input: DefineThemeInput) -> String {
        define_theme(&input).unwrap_err().0
    }

    fn with_tokens<const N: usize>(
        name: &str,
        tokens: [(&str, TokenInput); N],
    ) -> DefineThemeInput {
        DefineThemeInput {
            tokens: Some(tokens.into()),
            ..DefineThemeInput::new(name)
        }
    }

    fn css(value: &str) -> TokenInput {
        TokenInput::Css(value.to_string())
    }

    fn pair(light: &str, dark: &str) -> TokenInput {
        TokenInput::Pair(light.to_string(), dark.to_string())
    }

    fn style(value: &str) -> StyleValue {
        StyleValue::Css(value.to_string())
    }

    fn block<const N: usize>(entries: [(&str, StyleValue); N]) -> StyleValue {
        StyleValue::Block(entries.into())
    }

    fn components<const N: usize>(entries: [(&str, StyleValue); N]) -> Option<ComponentStyleMap> {
        Some(entries.into())
    }

    /// `theme.components?.[component]?.[key]`.
    fn rule<'a>(theme: &'a DefinedTheme, component: &str, key: &str) -> Option<&'a StyleMap> {
        theme
            .components
            .as_ref()?
            .get(component)?
            .block()?
            .get(key)?
            .block()
    }

    /// `theme.components?.[component]?.[key]?.[property]`.
    fn property<'a>(
        theme: &'a DefinedTheme,
        component: &str,
        key: &str,
        name: &str,
    ) -> Option<&'a str> {
        rule(theme, component, key)?.get(name)?.css()
    }

    fn token<'a>(theme: &'a DefinedTheme, name: &str) -> Option<&'a str> {
        theme.tokens.get(name).map(String::as_str)
    }

    fn scale(base: f64, ratio: f64) -> Option<TypographyConfig> {
        Some(TypographyConfig {
            scale: Some(TypographyScale { base, ratio }),
            ..TypographyConfig::default()
        })
    }

    fn role(family: Option<&str>, fallbacks: Option<&str>) -> Option<TypographyRole> {
        Some(TypographyRole {
            family: family.map(str::to_string),
            fallbacks: fallbacks.map(str::to_string),
            ..TypographyRole::default()
        })
    }

    fn weighted(weight: Option<&str>, weights: &[(u8, &str)]) -> Option<TypographyRole> {
        Some(TypographyRole {
            weight: weight.map(str::to_string),
            weights: (!weights.is_empty())
                .then(|| weights.iter().map(|(l, w)| (*l, w.to_string())).collect()),
            ..TypographyRole::default()
        })
    }

    // defineTheme.test.ts: defineTheme

    #[test]
    fn creates_a_theme_with_its_name() {
        assert_eq!(define(DefineThemeInput::new("test")).name, "test");
    }

    #[test]
    fn stores_only_authored_tokens_while_the_scoped_role_defaults_alias_them() {
        let theme = define(with_tokens(
            "custom",
            [("--color-text-blue", css("#CC0000"))],
        ));
        assert_eq!(token(&theme, "--color-text-blue"), Some("#CC0000"));
        assert_eq!(token(&theme, "--color-text-info"), None);
        // The stylesheet declares the role as an alias of its source.
        let roles = generated_scoped_status_role_defaults(&theme.tokens, &CssMap::new(), &[]);
        assert!(roles.contains(&("--color-text-info".into(), "var(--color-text-blue)".into())));
        // Unrelated defaults stay core's rather than copied into the theme.
        assert_eq!(token(&theme, "--color-background-surface"), None);
    }

    #[test]
    fn keeps_an_explicit_semantic_role_above_its_scoped_default() {
        let theme = define(with_tokens(
            "custom-info",
            [
                ("--color-text-blue", css("#FF0000")),
                ("--color-text-info", css("#0066CC")),
            ],
        ));
        assert_eq!(token(&theme, "--color-text-info"), Some("#0066CC"));
    }

    #[test]
    fn converts_light_dark_tuples_to_light_dark() {
        let theme = define(with_tokens(
            "tuple-test",
            [("--color-accent", pair("#0077B6", "#48CAE4"))],
        ));
        assert_eq!(
            token(&theme, "--color-accent"),
            Some("light-dark(#0077B6, #48CAE4)")
        );
    }

    #[test]
    fn refuses_a_plain_object_base_which_define_theme_has_not_built() {
        let mut plain = DefinedTheme {
            name: "plain-base".into(),
            ..DefinedTheme::default()
        };
        plain
            .tokens
            .insert("--color-accent", "light-dark(#ffffff, #000000)".to_string());
        let message = error(DefineThemeInput {
            extends: Some(Arc::new(plain)),
            ..DefineThemeInput::new("plain-base-child")
        });
        assert_eq!(
            message,
            "defineTheme(\"plain-base-child\"): `extends` must be a theme from defineTheme(), got a plain object named \"plain-base\", which is theme input, not a theme. Pass it through defineTheme() first (extends: defineTheme({...})), or import the theme its own file defines."
        );
        let base = define(with_tokens(
            "plain-base",
            [("--color-accent", pair("#ffffff", "#000000"))],
        ));
        let child = define(DefineThemeInput {
            extends: Some(Arc::new(base)),
            ..DefineThemeInput::new("defined-base-child")
        });
        assert_eq!(
            token(&child, "--color-accent"),
            Some("light-dark(#ffffff, #000000)")
        );
    }

    #[test]
    fn passes_string_values_through_and_mixes_them_with_tuples() {
        let theme = define(with_tokens(
            "mixed",
            [
                ("--color-accent", pair("#0077B6", "#48CAE4")),
                ("--radius-container", css("16px")),
                ("--font-family-heading", css("\"Georgia\", serif")),
            ],
        ));
        assert_eq!(
            token(&theme, "--color-accent"),
            Some("light-dark(#0077B6, #48CAE4)")
        );
        assert_eq!(token(&theme, "--radius-container"), Some("16px"));
        assert_eq!(
            token(&theme, "--font-family-heading"),
            Some("\"Georgia\", serif")
        );
    }

    #[test]
    fn accepts_unknown_token_names() {
        let theme = define(with_tokens(
            "custom",
            [("--color-does-not-exist", css("#FF0000"))],
        ));
        assert_eq!(token(&theme, "--color-does-not-exist"), Some("#FF0000"));
    }

    #[test]
    fn accepts_valid_local_token_names_and_keeps_their_exact_bytes() {
        for (theme_name, local) in [
            ("ocean-theme", "--ac-selection-ink"),
            ("Theme.Owner", "--"),
            (
                "ocean-theme",
                "--rustryx-theme-ocean-theme-color-status-fill-accent",
            ),
        ] {
            let var = format!("var({local})");
            let theme = define(DefineThemeInput {
                local_tokens: Some([(local, pair("#0077b6", "#48cae4"))].into()),
                components: components([(
                    "badge",
                    block([("variant:info", block([("backgroundColor", style(&var))]))]),
                )]),
                ..DefineThemeInput::new(theme_name)
            });
            assert!(!theme.tokens.contains_key(local));
            assert_eq!(
                theme.local_tokens,
                Some([(local, "light-dark(#0077b6, #48cae4)".to_string())].into())
            );
            assert_eq!(
                theme.local_token_owners,
                Some([(local, theme_name.to_string())].into())
            );
            assert_eq!(
                theme.local_token_lineage,
                Some(vec![theme_name.to_string()])
            );
        }
    }

    #[test]
    fn rejects_a_name_declared_in_both_tokens_and_local_tokens() {
        let token_name = "--rustryx-theme-ocean-theme-color-status-fill-accent";
        let legacy = define(with_tokens("ocean-theme", [(token_name, css("#0077b6"))]));
        assert_eq!(
            resolve_theme_token(Some(&legacy), token_name, ResolvedThemeMode::Light, None),
            "#0077b6"
        );
        let both = error(DefineThemeInput {
            local_tokens: Some([(token_name, css("#48cae4"))].into()),
            ..with_tokens("ocean-theme", [(token_name, css("#0077b6"))])
        });
        assert!(both.contains("both tokens and localTokens"), "{both}");
        let portable = error(DefineThemeInput {
            local_tokens: Some([("--color-accent", css("#48cae4"))].into()),
            ..DefineThemeInput::new("ocean-theme")
        });
        assert_eq!(
            portable,
            "defineTheme(\"ocean-theme\"): token \"--color-accent\" cannot be declared in both tokens and localTokens."
        );
    }

    #[test]
    fn does_not_reinterpret_unenrolled_custom_property_references() {
        let theme = define(DefineThemeInput {
            components: components([(
                "badge",
                block([(
                    "base",
                    block([("color", style("var(--rustryx-theme-missing-color-old)"))]),
                )]),
            )]),
            ..with_tokens(
                "legacy",
                [("--rustryx-theme-legacy-color-old", css("#123456"))],
            )
        });
        assert_eq!(
            token(&theme, "--rustryx-theme-legacy-color-old"),
            Some("#123456")
        );
        assert_eq!(theme.local_tokens, None);
        assert_eq!(theme.local_token_owners, None);
        assert_eq!(theme.local_token_lineage, None);
    }

    #[test]
    fn rejects_an_invalid_css_custom_property_name() {
        let message = error(DefineThemeInput {
            local_tokens: Some([("selection-ink", css("#123456"))].into()),
            ..DefineThemeInput::new("ocean")
        });
        assert_eq!(
            message,
            "defineTheme(\"ocean\"): local token \"selection-ink\" must be a valid CSS custom-property name."
        );
    }

    #[test]
    fn treats_case_only_and_unrelated_custom_property_references_as_external() {
        let result = define_theme(&DefineThemeInput {
            local_tokens: Some([("--Selection-Ink", css("#123456"))].into()),
            components: components([(
                "button",
                block([(
                    "base",
                    block([
                        ("color", style("var(--selection-ink)")),
                        ("backgroundColor", style("var(--product-surface)")),
                    ]),
                )]),
            )]),
            ..DefineThemeInput::new("ocean")
        });
        assert!(result.is_ok());
    }

    #[test]
    fn rejects_cycles_between_local_tokens_in_any_letter_case() {
        for function in ["VAR", "vAr"] {
            let escaped = r"--\66 oo";
            let other = "--rustryx-theme-cycle-color-b";
            let message = error(DefineThemeInput {
                local_tokens: Some(
                    [
                        (escaped, css(&format!("{function}({other})"))),
                        (other, css(&format!("{function}({escaped})"))),
                    ]
                    .into(),
                ),
                ..DefineThemeInput::new("cycle")
            });
            // Tarjan's algorithm pops the component last member first, so the
            // path starts at the token declared second.
            assert_eq!(
                message,
                format!(
                    "defineTheme(\"cycle\").localTokens: Theme token cycle detected: {other} -> {escaped} -> {other}."
                )
            );
        }
    }

    #[test]
    fn works_with_no_tokens() {
        let theme = define(DefineThemeInput::new("bare"));
        assert!(theme.tokens.is_empty());
        // Without a type scale there are no generated components either.
        assert_eq!(theme.components, None);
    }

    // defineTheme.test.ts: component overrides

    #[test]
    fn passes_components_through_to_the_theme() {
        let theme = define(DefineThemeInput {
            components: components([
                (
                    "card",
                    block([(
                        "base",
                        block([
                            ("borderWidth", style("2px")),
                            ("borderColor", style("var(--color-accent)")),
                        ]),
                    )]),
                ),
                (
                    "button",
                    block([("base", block([("borderRadius", style("999px"))]))]),
                ),
            ]),
            ..DefineThemeInput::new("styled")
        });
        assert_eq!(
            rule(&theme, "card", "base"),
            Some(
                &[
                    ("borderWidth", style("2px")),
                    ("borderColor", style("var(--color-accent)"))
                ]
                .into()
            )
        );
        assert_eq!(
            property(&theme, "button", "base", "borderRadius"),
            Some("999px")
        );
    }

    #[test]
    fn passes_a_custom_status_or_variant_through_as_a_style_key() {
        let theme = define(DefineThemeInput {
            components: components([
                (
                    "banner",
                    block([(
                        "status:neutral",
                        block([
                            ("backgroundColor", style("var(--color-background-muted)")),
                            ("color", style("gray")),
                        ]),
                    )]),
                ),
                (
                    "button",
                    block([(
                        "variant:primary-muted",
                        block([("backgroundColor", style("#ECF5FF"))]),
                    )]),
                ),
            ]),
            ..with_tokens("combo", [("--color-accent", css("#FF0000"))])
        });
        assert_eq!(token(&theme, "--color-accent"), Some("#FF0000"));
        assert_eq!(
            rule(&theme, "banner", "status:neutral"),
            Some(
                &[
                    ("backgroundColor", style("var(--color-background-muted)")),
                    ("color", style("gray")),
                ]
                .into()
            )
        );
        assert_eq!(
            property(&theme, "button", "variant:primary-muted", "backgroundColor"),
            Some("#ECF5FF")
        );
    }

    // defineTheme.test.ts: typography.scale

    #[test]
    fn a_type_scale_generates_typography_tokens() {
        let theme = define(DefineThemeInput {
            typography: scale(12.0, 1.125),
            ..DefineThemeInput::new("dense")
        });
        assert_eq!(
            token(&theme, "--text-heading-4-size"),
            Some("var(--font-size-base)")
        );
        assert_eq!(token(&theme, "--font-size-base"), Some("0.75rem"));
        assert_eq!(
            token(&theme, "--text-body-size"),
            Some("var(--font-size-base)")
        );
    }

    #[test]
    fn a_type_scale_generates_54_tokens() {
        let theme = define(DefineThemeInput {
            typography: scale(14.0, 1.2),
            ..DefineThemeInput::new("custom")
        });
        // 12 raw sizes + 18 heading + 24 text.
        let count = theme
            .tokens
            .keys()
            .filter(|k| k.starts_with("--font-size-") || k.starts_with("--text-"))
            .count();
        assert_eq!(count, 54);
    }

    #[test]
    fn explicit_tokens_win_over_scale_generated_ones() {
        let theme = define(DefineThemeInput {
            typography: scale(14.0, 1.2),
            ..with_tokens("override-test", [("--text-heading-1-size", css("40px"))])
        });
        assert_eq!(token(&theme, "--text-heading-1-size"), Some("40px"));
        assert_eq!(
            token(&theme, "--text-heading-4-size"),
            Some("var(--font-size-base)")
        );
    }

    #[test]
    fn combines_a_type_scale_and_a_radius_scale_with_other_tokens() {
        let theme = define(DefineThemeInput {
            typography: scale(16.0, 1.25),
            radius: Some(RadiusScaleConfig {
                base: 4.0,
                multiplier: 1.0,
            }),
            ..with_tokens("combo", [("--color-accent", css("#CC0000"))])
        });
        assert_eq!(token(&theme, "--color-accent"), Some("#CC0000"));
        assert_eq!(
            token(&theme, "--text-heading-4-size"),
            Some("var(--font-size-base)")
        );
        assert_eq!(token(&theme, "--font-size-base"), Some("1rem"));
        assert_eq!(token(&theme, "--radius-element"), Some("8px"));
        assert_eq!(
            token(
                &define(DefineThemeInput::new("no-scale")),
                "--text-heading-1-size"
            ),
            None
        );
    }

    // defineTheme.test.ts: typography.scale component auto-generation

    #[test]
    fn a_type_scale_generates_heading_and_text_rules_without_color() {
        let theme = define(DefineThemeInput {
            typography: scale(14.0, 1.2),
            ..DefineThemeInput::new("auto")
        });
        assert_eq!(
            property(&theme, "heading", "level:1", "fontSize"),
            Some("var(--text-heading-1-size)")
        );
        assert_eq!(
            property(&theme, "text", "type:body", "fontSize"),
            Some("var(--text-body-size)")
        );
        assert_eq!(property(&theme, "heading", "level:1", "color"), None);
        assert_eq!(property(&theme, "text", "type:supporting", "color"), None);
    }

    #[test]
    fn explicit_components_deep_merge_on_top_of_the_generated_rules() {
        let theme = define(DefineThemeInput {
            typography: scale(14.0, 1.2),
            components: components([
                (
                    "heading",
                    block([(
                        "level:1",
                        block([
                            ("letterSpacing", style("-0.02em")),
                            ("fontFamily", style("\"Playfair Display\", serif")),
                        ]),
                    )]),
                ),
                (
                    "button",
                    block([("base", block([("borderRadius", style("999px"))]))]),
                ),
            ]),
            ..DefineThemeInput::new("custom")
        });
        assert_eq!(
            property(&theme, "heading", "level:1", "fontSize"),
            Some("var(--text-heading-1-size)")
        );
        assert_eq!(
            property(&theme, "heading", "level:1", "letterSpacing"),
            Some("-0.02em")
        );
        assert_eq!(
            property(&theme, "heading", "level:1", "fontFamily"),
            Some("\"Playfair Display\", serif")
        );
        assert_eq!(
            property(&theme, "button", "base", "borderRadius"),
            Some("999px")
        );
    }

    // defineTheme.test.ts: radius

    #[test]
    fn a_radius_scale_generates_radius_tokens() {
        let radius = |multiplier| {
            define(DefineThemeInput {
                radius: Some(RadiusScaleConfig {
                    base: 4.0,
                    multiplier,
                }),
                ..DefineThemeInput::new("rounded")
            })
        };
        let rounded = radius(1.5);
        for (name, value) in [
            ("--radius-inner", "6px"),
            ("--radius-element", "12px"),
            ("--radius-container", "18px"),
            ("--radius-page", "42px"),
            ("--radius-none", "0px"),
            ("--radius-full", "9999px"),
        ] {
            assert_eq!(token(&rounded, name), Some(value), "{name}");
        }
        let sharp = radius(0.0);
        for name in [
            "--radius-inner",
            "--radius-element",
            "--radius-container",
            "--radius-page",
        ] {
            assert_eq!(token(&sharp, name), Some("0px"), "{name}");
        }
        assert_eq!(token(&sharp, "--radius-full"), Some("9999px"));
        assert_eq!(
            token(&define(DefineThemeInput::new("none")), "--radius-none"),
            None
        );
    }

    #[test]
    fn explicit_tokens_win_over_the_radius_scale() {
        let theme = define(DefineThemeInput {
            radius: Some(RadiusScaleConfig::default()),
            ..with_tokens("custom", [("--radius-container", css("20px"))])
        });
        assert_eq!(token(&theme, "--radius-container"), Some("20px"));
        assert_eq!(token(&theme, "--radius-element"), Some("8px"));
    }

    // defineTheme.test.ts: typography font family derivation

    #[test]
    fn derives_font_family_tokens_from_typography_roles() {
        let theme = define(DefineThemeInput {
            typography: Some(TypographyConfig {
                body: role(Some("Geist"), Some("-apple-system, sans-serif")),
                code: role(Some("Geist Mono"), Some("\"SF Mono\", monospace")),
                ..TypographyConfig::default()
            }),
            ..DefineThemeInput::new("family-tokens")
        });
        assert_eq!(
            token(&theme, "--font-family-body"),
            Some("Geist, -apple-system, sans-serif")
        );
        // The heading inherits the body's.
        assert_eq!(
            token(&theme, "--font-family-heading"),
            Some("Geist, -apple-system, sans-serif")
        );
        assert_eq!(
            token(&theme, "--font-family-code"),
            Some("\"Geist Mono\", \"SF Mono\", monospace")
        );
    }

    #[test]
    fn a_heading_family_overrides_the_body_with_its_own_fallbacks_only() {
        let theme = define(DefineThemeInput {
            typography: Some(TypographyConfig {
                body: role(Some("Inter"), Some("sans-serif")),
                heading: role(Some("Playfair Display"), Some("serif")),
                ..TypographyConfig::default()
            }),
            ..DefineThemeInput::new("heading-override")
        });
        assert_eq!(
            token(&theme, "--font-family-body"),
            Some("Inter, sans-serif")
        );
        assert_eq!(
            token(&theme, "--font-family-heading"),
            Some("\"Playfair Display\", serif")
        );

        let theme = define(DefineThemeInput {
            typography: Some(TypographyConfig {
                body: role(Some("Inter"), Some("sans-serif")),
                heading: role(Some("Lora"), None),
                code: role(None, Some("monospace")),
                ..TypographyConfig::default()
            }),
            ..DefineThemeInput::new("heading-family-only")
        });
        assert_eq!(token(&theme, "--font-family-heading"), Some("Lora"));
        assert_eq!(token(&theme, "--font-family-code"), None);
    }

    #[test]
    fn explicit_tokens_win_over_typography_derived_font_tokens() {
        let theme = define(DefineThemeInput {
            typography: Some(TypographyConfig {
                scale: Some(TypographyScale {
                    base: 14.0,
                    ratio: 1.2,
                }),
                body: role(Some("Geist"), Some("sans-serif")),
                ..TypographyConfig::default()
            }),
            ..with_tokens(
                "token-wins",
                [("--font-family-heading", css("\"Custom\", serif"))],
            )
        });
        assert_eq!(
            token(&theme, "--font-family-heading"),
            Some("\"Custom\", serif")
        );
        assert_eq!(
            token(&theme, "--font-family-body"),
            Some("Geist, sans-serif")
        );
        assert!(theme.tokens.contains_key("--text-heading-4-size"));
    }

    // defineTheme.test.ts: typography weight derivation

    fn weights(
        heading: Option<TypographyRole>,
        body: Option<TypographyRole>,
        code: Option<TypographyRole>,
    ) -> DefinedTheme {
        define(DefineThemeInput {
            typography: Some(TypographyConfig {
                scale: Some(TypographyScale {
                    base: 14.0,
                    ratio: 1.2,
                }),
                body,
                heading,
                code,
            }),
            ..DefineThemeInput::new("weights")
        })
    }

    #[test]
    fn the_heading_role_weight_applies_to_every_level() {
        let theme = weights(weighted(Some("bold"), &[]), None, None);
        assert_eq!(
            token(&theme, "--text-heading-1-weight"),
            Some("var(--font-weight-bold)")
        );
        assert_eq!(
            token(&theme, "--text-heading-4-weight"),
            Some("var(--font-weight-bold)")
        );
    }

    #[test]
    fn per_level_heading_weights_override_the_default_heading_weight() {
        let theme = weights(
            weighted(Some("semibold"), &[(3, "bold"), (4, "bold")]),
            None,
            None,
        );
        assert_eq!(
            token(&theme, "--text-heading-1-weight"),
            Some("var(--font-weight-semibold)")
        );
        assert_eq!(
            token(&theme, "--text-heading-3-weight"),
            Some("var(--font-weight-bold)")
        );
        assert_eq!(
            token(&theme, "--text-heading-4-weight"),
            Some("var(--font-weight-bold)")
        );
    }

    #[test]
    fn body_and_code_weights_flow_to_their_text_tokens() {
        let theme = weights(
            None,
            weighted(Some("medium"), &[]),
            weighted(Some("medium"), &[]),
        );
        assert_eq!(
            token(&theme, "--text-body-weight"),
            Some("var(--font-weight-medium)")
        );
        assert_eq!(
            token(&theme, "--text-code-weight"),
            Some("var(--font-weight-medium)")
        );
    }

    #[test]
    fn named_weights_map_to_references_and_raw_ones_pass_through() {
        let normal = weights(weighted(Some("normal"), &[]), None, None);
        assert_eq!(
            token(&normal, "--text-heading-1-weight"),
            Some("var(--font-weight-normal)")
        );
        let raw = weights(weighted(Some("900"), &[]), None, None);
        assert_eq!(token(&raw, "--text-heading-1-weight"), Some("900"));
    }

    // defineTheme.test.ts: defineTheme extends

    #[test]
    fn a_child_inherits_local_token_enrollment_replaces_exactly_and_owns_new_names() {
        let inherited = "--rustryx-theme-base-theme-color-status-fill";
        let child_owned = "--child-surface-raised";
        let base = define(DefineThemeInput {
            local_tokens: Some([(inherited, css("#123456"))].into()),
            ..DefineThemeInput::new("base-theme")
        });
        let child = define(DefineThemeInput {
            extends: Some(Arc::new(base)),
            local_tokens: Some([(inherited, css("#654321")), (child_owned, css("#abcdef"))].into()),
            ..DefineThemeInput::new("child-theme")
        });
        assert_eq!(
            child.local_tokens,
            Some(
                [
                    (inherited, "#654321".to_string()),
                    (child_owned, "#abcdef".to_string())
                ]
                .into()
            )
        );
        assert_eq!(
            child.local_token_owners,
            Some(
                [
                    (inherited, "base-theme".to_string()),
                    (child_owned, "child-theme".to_string())
                ]
                .into()
            )
        );
        assert_eq!(
            child.local_token_lineage,
            Some(vec!["base-theme".to_string(), "child-theme".to_string()])
        );
    }

    #[test]
    fn a_child_inherits_enrollment_when_it_declares_no_local_tokens() {
        let base = define(DefineThemeInput {
            local_tokens: Some(TokenInputs::new()),
            ..DefineThemeInput::new("base-theme")
        });
        let child = define(DefineThemeInput {
            extends: Some(Arc::new(base)),
            ..DefineThemeInput::new("child-theme")
        });
        assert_eq!(child.local_tokens, Some(CssMap::new()));
        assert_eq!(
            child.local_token_lineage,
            Some(vec!["base-theme".to_string(), "child-theme".to_string()])
        );
    }

    #[test]
    fn legacy_descendant_names_keep_their_lineage_when_they_only_inherit() {
        let base = define(DefineThemeInput {
            local_tokens: Some(
                [(
                    "--rustryx-theme-base-theme-color-status-fill",
                    css("#123456"),
                )]
                .into(),
            ),
            ..DefineThemeInput::new("base-theme")
        });
        let base_locals = base.local_tokens.clone();
        let legacy = Arc::new(define(DefineThemeInput {
            extends: Some(Arc::new(base)),
            ..DefineThemeInput::new("Legacy.Theme")
        }));
        let grandchild = define(DefineThemeInput {
            extends: Some(legacy.clone()),
            ..DefineThemeInput::new("grandchild")
        });
        assert_eq!(legacy.local_tokens, base_locals);
        assert_eq!(
            legacy.local_token_lineage,
            Some(vec!["base-theme".to_string(), "Legacy.Theme".to_string()])
        );
        assert_eq!(grandchild.local_tokens, base_locals);
        assert_eq!(
            grandchild.local_token_lineage,
            Some(vec![
                "base-theme".into(),
                "Legacy.Theme".into(),
                "grandchild".into()
            ])
        );
    }

    #[test]
    fn a_child_inherits_and_overrides_base_tokens() {
        let base = Arc::new(define(with_tokens(
            "base",
            [
                ("--color-accent", css("#0077B6")),
                ("--radius-container", css("16px")),
            ],
        )));
        let child = define(DefineThemeInput {
            extends: Some(base.clone()),
            ..DefineThemeInput::new("child")
        });
        assert_eq!(token(&child, "--color-accent"), Some("#0077B6"));
        assert_eq!(token(&child, "--radius-container"), Some("16px"));
        assert_eq!(child.name, "child");
        let child = define(DefineThemeInput {
            extends: Some(base),
            ..with_tokens("child", [("--color-accent", css("#FF0000"))])
        });
        assert_eq!(token(&child, "--color-accent"), Some("#FF0000"));
        assert_eq!(token(&child, "--radius-container"), Some("16px"));
    }

    #[test]
    fn motion_inherits_field_by_field() {
        // spec:AST-108 FR5: a field the child leaves unset keeps the base's.
        let base = define(DefineThemeInput {
            motion: Some(ThemeMotionConfig {
                fast: Some(100.0),
                medium: Some(200.0),
                slow: Some(400.0),
                ratio: Some(0.75),
                easing: Some("cubic-bezier(0.2, 0, 0, 1)".into()),
                ..ThemeMotionConfig::default()
            }),
            ..DefineThemeInput::new("base")
        });
        let child = define(DefineThemeInput {
            extends: Some(Arc::new(base)),
            motion: Some(ThemeMotionConfig {
                fast: Some(120.0),
                medium: Some(240.0),
                ratio: Some(0.8),
                ..ThemeMotionConfig::default()
            }),
            ..DefineThemeInput::new("child")
        });
        // The fields the child sets regenerate their tokens.
        assert_eq!(token(&child, "--duration-fast"), Some("120ms"));
        assert_eq!(token(&child, "--duration-medium-min"), Some("190ms"));
        // `slow` and `easing` keep the base's tokens, the slow band at the
        // base's ratio (400 × 0.75), not the child's.
        assert_eq!(token(&child, "--duration-slow"), Some("400ms"));
        assert_eq!(token(&child, "--duration-slow-min"), Some("300ms"));
        assert_eq!(
            token(&child, "--ease-standard"),
            Some("cubic-bezier(0.2, 0, 0, 1)")
        );
    }

    #[test]
    fn refuses_a_partial_duration_scale() {
        // assertCompleteMotionScale: a scale field without fast, medium and
        // ratio, after inheritance.
        let message = error(DefineThemeInput {
            motion: Some(ThemeMotionConfig {
                fast: Some(100.0),
                tempo: Some(1.2),
                ..ThemeMotionConfig::default()
            }),
            ..DefineThemeInput::new("partial")
        });
        assert_eq!(
            message,
            "defineTheme(\"partial\"): `motion` sets a duration scale field, so it must supply `fast`, `medium`, and `ratio` unless each missing field is inherited from `extends`."
        );
        // Knobs alone write no scale.
        assert!(
            define_theme(&DefineThemeInput {
                motion: Some(ThemeMotionConfig {
                    tempo: Some(1.2),
                    character: Some(MotionCharacter::Crisp),
                    ..ThemeMotionConfig::default()
                }),
                ..DefineThemeInput::new("knobs")
            })
            .is_ok()
        );
    }

    #[test]
    fn a_child_inherits_and_merges_component_overrides() {
        let base = Arc::new(define(DefineThemeInput {
            components: components([(
                "button",
                block([
                    (
                        "base",
                        block([("fontWeight", style("600")), ("borderRadius", style("4px"))]),
                    ),
                    (
                        "variant:secondary",
                        block([("backgroundColor", style("rgba(0,0,0,0.06)"))]),
                    ),
                ]),
            )]),
            ..DefineThemeInput::new("base")
        }));
        let inherited = define(DefineThemeInput {
            extends: Some(base.clone()),
            ..DefineThemeInput::new("child")
        });
        assert_eq!(
            property(&inherited, "button", "variant:secondary", "backgroundColor"),
            Some("rgba(0,0,0,0.06)")
        );
        let merged = define(DefineThemeInput {
            extends: Some(base),
            components: components([(
                "button",
                block([("base", block([("fontWeight", style("700"))]))]),
            )]),
            ..DefineThemeInput::new("child")
        });
        assert_eq!(
            rule(&merged, "button", "base"),
            Some(&[("fontWeight", style("700")), ("borderRadius", style("4px"))].into())
        );
    }

    #[test]
    fn icon_variants_are_kept_inherited_or_replaced() {
        let listed = |names: &[&str]| Some(names.iter().map(|n| n.to_string()).collect::<Vec<_>>());
        let base = Arc::new(define(DefineThemeInput {
            icon_variants: listed(&["outline", "filled"]),
            ..DefineThemeInput::new("variants-base")
        }));
        assert_eq!(base.icon_variants, listed(&["outline", "filled"]));
        let inherit = define(DefineThemeInput {
            extends: Some(base.clone()),
            ..DefineThemeInput::new("variants-inherit")
        });
        assert_eq!(inherit.icon_variants, listed(&["outline", "filled"]));
        let replace = define(DefineThemeInput {
            extends: Some(base.clone()),
            icon_variants: listed(&["bold"]),
            ..DefineThemeInput::new("variants-replace")
        });
        assert_eq!(replace.icon_variants, listed(&["bold"]));
        let emptied = define(DefineThemeInput {
            extends: Some(base),
            icon_variants: listed(&[]),
            ..DefineThemeInput::new("variants-empty")
        });
        assert_eq!(emptied.icon_variants, listed(&[]));
        let none = Arc::new(define(DefineThemeInput::new("no-variants")));
        let none_child = define(DefineThemeInput {
            extends: Some(none.clone()),
            ..DefineThemeInput::new("no-variants-child")
        });
        assert_eq!(none.icon_variants, None);
        assert_eq!(none_child.icon_variants, None);
    }

    #[test]
    fn a_child_inherits_font_family_tokens_and_overrides_the_type_scale() {
        let base = Arc::new(define(DefineThemeInput {
            typography: Some(TypographyConfig {
                scale: Some(TypographyScale {
                    base: 14.0,
                    ratio: 1.2,
                }),
                body: role(Some("Geist"), Some("sans-serif")),
                ..TypographyConfig::default()
            }),
            ..DefineThemeInput::new("base")
        }));
        let child = define(DefineThemeInput {
            extends: Some(base.clone()),
            ..DefineThemeInput::new("child")
        });
        assert_eq!(
            token(&child, "--font-family-body"),
            Some("Geist, sans-serif")
        );
        let rescaled = define(DefineThemeInput {
            extends: Some(base.clone()),
            typography: scale(16.0, 1.25),
            ..DefineThemeInput::new("child")
        });
        assert_ne!(
            token(&rescaled, "--font-size-base"),
            token(&base, "--font-size-base")
        );
    }

    #[test]
    fn a_child_inherits_and_overrides_its_base_surfaces() {
        let on_dark = |tokens: TokenInputs| {
            Some(OnMediaOverrides {
                tokens: Some(tokens),
                components: None,
            })
        };
        let base = Arc::new(define(DefineThemeInput {
            on_dark: on_dark(
                [
                    ("--color-border", css("#ffffff")),
                    ("--color-track", css("#eee")),
                ]
                .into(),
            ),
            on_light: Some(OnMediaOverrides {
                tokens: Some([("--color-border", css("#000000"))].into()),
                components: None,
            }),
            ..DefineThemeInput::new("base")
        }));
        let child = define(DefineThemeInput {
            extends: Some(base.clone()),
            ..DefineThemeInput::new("child")
        });
        let surface_token = |theme: &DefinedTheme, dark: bool, name: &str| {
            let surface = if dark {
                &theme.on_dark
            } else {
                &theme.on_light
            };
            surface.as_ref().and_then(|s| s.tokens.get(name)).cloned()
        };
        assert_eq!(
            surface_token(&child, true, "--color-border").as_deref(),
            Some("#ffffff")
        );
        assert_eq!(
            surface_token(&child, false, "--color-border").as_deref(),
            Some("#000000")
        );
        let overriding = define(DefineThemeInput {
            extends: Some(base),
            on_dark: on_dark([("--color-border", css("#cccccc"))].into()),
            ..DefineThemeInput::new("child")
        });
        assert_eq!(
            surface_token(&overriding, true, "--color-border").as_deref(),
            Some("#cccccc")
        );
        assert_eq!(
            surface_token(&overriding, true, "--color-track").as_deref(),
            Some("#eee")
        );
    }

    #[test]
    fn a_child_inherits_and_deep_merges_on_dark_components() {
        let base = Arc::new(define(DefineThemeInput {
            on_dark: Some(OnMediaOverrides {
                tokens: None,
                components: components([
                    (
                        "card",
                        block([(
                            "base",
                            block([
                                ("borderColor", style("#fff")),
                                ("backgroundColor", style("#111")),
                            ]),
                        )]),
                    ),
                    (
                        "badge",
                        block([("base", block([("color", style("#fff"))]))]),
                    ),
                ]),
            }),
            ..DefineThemeInput::new("base")
        }));
        let child = define(DefineThemeInput {
            extends: Some(base),
            on_dark: Some(OnMediaOverrides {
                tokens: None,
                components: components([(
                    "card",
                    block([("base", block([("borderColor", style("#ccc"))]))]),
                )]),
            }),
            ..DefineThemeInput::new("child")
        });
        let surface = child.on_dark.as_ref().unwrap().components.as_ref().unwrap();
        let base_of = |component: &str| {
            surface
                .get(component)?
                .block()?
                .get("base")?
                .block()
                .cloned()
        };
        assert_eq!(
            base_of("card"),
            Some(
                [
                    ("borderColor", style("#ccc")),
                    ("backgroundColor", style("#111"))
                ]
                .into()
            )
        );
        assert_eq!(base_of("badge"), Some([("color", style("#fff"))].into()));
    }

    #[test]
    fn input_tokens_are_inherited_and_the_childs_win() {
        let base = Arc::new(define(with_tokens(
            "base",
            [("--color-accent", pair("#111111", "#eeeeee"))],
        )));
        let child = define(DefineThemeInput {
            extends: Some(base.clone()),
            ..with_tokens("child", [("--color-border", css("#cccccc"))])
        });
        let input = child.input_tokens.unwrap();
        assert_eq!(
            input.get("--color-accent"),
            Some(&pair("#111111", "#eeeeee"))
        );
        assert_eq!(input.get("--color-border"), Some(&css("#cccccc")));
        let child = define(DefineThemeInput {
            extends: Some(base),
            ..with_tokens("child", [("--color-accent", css("#ff0000"))])
        });
        assert_eq!(
            child.input_tokens.unwrap().get("--color-accent"),
            Some(&css("#ff0000"))
        );
    }

    #[test]
    fn accepts_a_pre_built_theme_module_as_a_base() {
        let mut built = DefinedTheme {
            name: "built-base".into(),
            built: true,
            ..DefinedTheme::default()
        };
        built.tokens.insert("--color-accent", "#111111".to_string());
        let child = define(DefineThemeInput {
            extends: Some(Arc::new(built)),
            ..DefineThemeInput::new("child")
        });
        assert_eq!(token(&child, "--color-accent"), Some("#111111"));
    }

    // defineTheme.test.ts: local tokens left undefined

    #[test]
    fn a_local_token_left_unset_is_not_set() {
        let theme = define(DefineThemeInput {
            local_tokens: Some([("--brand", css("red")), ("--brand-2", TokenInput::Unset)].into()),
            ..DefineThemeInput::new("some-local")
        });
        assert_eq!(
            theme.local_tokens,
            Some([("--brand", "red".to_string())].into())
        );
    }

    #[cfg(feature = "json")]
    #[test]
    fn refuses_a_local_tokens_field_that_holds_undefined_by_name() {
        let json = serde_json::json!({"name": "no-local", "localTokens": null});
        assert_eq!(
            DefineThemeInput::from_json(&json).unwrap_err().0,
            "defineTheme(\"no-local\"): localTokens is undefined: omit the field or pass an object."
        );
    }

    // onMediaTokens.test.ts: defineTheme with onDark/onLight

    #[test]
    fn stores_the_resolved_surfaces_on_the_theme() {
        let theme = define(DefineThemeInput {
            on_dark: Some(OnMediaOverrides {
                tokens: Some([("--color-accent", css("#90CAF9"))].into()),
                components: components([(
                    "button",
                    block([("variant:ghost", block([("borderWidth", style("1px"))]))]),
                )]),
            }),
            on_light: Some(OnMediaOverrides {
                tokens: Some([("--color-accent", css("#333"))].into()),
                components: None,
            }),
            ..DefineThemeInput::new("test")
        });
        let dark = theme.on_dark.as_ref().unwrap();
        assert_eq!(
            dark.tokens.get("--color-accent").map(String::as_str),
            Some("#90CAF9")
        );
        assert_eq!(
            dark.tokens.get("--color-text-primary").map(String::as_str),
            Some("var(--color-on-dark)")
        );
        let ghost = dark
            .components
            .as_ref()
            .unwrap()
            .get("button")
            .unwrap()
            .block()
            .unwrap();
        assert_eq!(
            ghost.get("variant:ghost"),
            Some(&block([("borderWidth", style("1px"))]))
        );
        let light = theme.on_light.as_ref().unwrap();
        assert_eq!(
            light.tokens.get("--color-accent").map(String::as_str),
            Some("#333")
        );
        // Defaults even without overrides.
        let bare = define(DefineThemeInput::new("test"));
        assert_eq!(
            bare.on_dark
                .unwrap()
                .tokens
                .get("color-scheme")
                .map(String::as_str),
            Some("dark")
        );
        assert_eq!(
            bare.on_light
                .unwrap()
                .tokens
                .get("color-scheme")
                .map(String::as_str),
            Some("light")
        );
    }

    // expandColorScale.test.ts: defineTheme integration

    const CONFIG_ACCENT: (&str, &str) = ("#B7410E", "#48CAE4");
    const TOKEN_ACCENT: (&str, &str) = ("#AA0000", "#FF5555");

    fn color(accent: Option<AccentSeed>, neutral_style: NeutralStyle) -> Option<ColorScaleConfig> {
        Some(ColorScaleConfig {
            accent,
            neutral_style,
            contrast: ColorContrast::Standard,
        })
    }

    fn config_accent() -> Option<ColorScaleConfig> {
        color(
            Some(AccentSeed::Pair(
                CONFIG_ACCENT.0.into(),
                CONFIG_ACCENT.1.into(),
            )),
            NeutralStyle::Cool,
        )
    }

    #[test]
    fn a_tokens_accent_wins_over_the_color_generated_one_and_references_follow_it() {
        let theme = define(DefineThemeInput {
            color: config_accent(),
            ..with_tokens(
                "precedence",
                [("--color-accent", pair(TOKEN_ACCENT.0, TOKEN_ACCENT.1))],
            )
        });
        assert_eq!(
            token(&theme, "--color-accent"),
            Some("light-dark(#AA0000, #FF5555)")
        );
        let light = resolve_theme_tokens(Some(&theme), ResolvedThemeMode::Light);
        assert_eq!(
            light.get("--color-text-accent").map(String::as_str),
            Some(TOKEN_ACCENT.0)
        );
        assert_eq!(
            light.get("--color-icon-accent").map(String::as_str),
            Some(TOKEN_ACCENT.0)
        );
        let dark = resolve_theme_tokens(Some(&theme), ResolvedThemeMode::Dark);
        assert_eq!(
            dark.get("--color-text-accent").map(String::as_str),
            Some(TOKEN_ACCENT.1)
        );
        // The baked --color-on-accent stays derived from the color.accent seed.
        let generated = define(DefineThemeInput {
            color: config_accent(),
            ..DefineThemeInput::new("generated-on-accent")
        });
        assert_eq!(
            token(&theme, "--color-on-accent"),
            token(&generated, "--color-on-accent")
        );
    }

    #[test]
    fn explicit_tokens_win_over_generated_color_tokens() {
        let theme = define(DefineThemeInput {
            color: config_accent(),
            ..with_tokens("explicit", [("--color-on-accent", css("#FFFFFF"))])
        });
        assert_eq!(token(&theme, "--color-on-accent"), Some("#FFFFFF"));
        let theme = define(DefineThemeInput {
            color: color(
                Some(AccentSeed::Single("#0064E0".into())),
                NeutralStyle::Cool,
            ),
            ..with_tokens("test-override", [("--color-accent", css("red"))])
        });
        assert_eq!(token(&theme, "--color-accent"), Some("red"));
    }

    #[test]
    fn a_neutral_only_theme_leaves_the_accent_tokens_at_their_defaults() {
        let theme = define(DefineThemeInput {
            color: color(None, NeutralStyle::Warm),
            ..DefineThemeInput::new("test-neutral-only")
        });
        for key in [
            "--color-accent",
            "--color-accent-muted",
            "--color-on-accent",
        ] {
            assert!(!theme.tokens.contains_key(key), "{key}");
        }
        assert!(
            token(&theme, "--color-background-surface")
                .unwrap()
                .starts_with("light-dark(#")
        );
        let light = resolve_theme_tokens(Some(&theme), ResolvedThemeMode::Light);
        let dark = resolve_theme_tokens(Some(&theme), ResolvedThemeMode::Dark);
        for key in [
            "--color-accent",
            "--color-text-accent",
            "--color-icon-accent",
        ] {
            assert_eq!(light.get(key).map(String::as_str), Some("#0064E0"), "{key}");
        }
        assert_eq!(
            dark.get("--color-accent").map(String::as_str),
            Some("#0171E3")
        );
        assert_eq!(
            dark.get("--color-text-accent").map(String::as_str),
            Some("#0171E3")
        );
        // An explicit accent token still wins.
        let explicit = define(DefineThemeInput {
            color: color(None, NeutralStyle::Warm),
            ..with_tokens(
                "explicit-accent",
                [("--color-accent", pair("#AA0000", "#FF5555"))],
            )
        });
        assert_eq!(
            resolve_theme_token(
                Some(&explicit),
                "--color-text-accent",
                ResolvedThemeMode::Light,
                None
            ),
            "#AA0000"
        );
    }

    #[test]
    fn a_tuple_accent_derives_each_mode_from_its_own_seed() {
        let seeded = |accent: AccentSeed| {
            define(DefineThemeInput {
                color: color(Some(accent), NeutralStyle::Cool),
                ..DefineThemeInput::new("seeded")
            })
        };
        let tuple = seeded(AccentSeed::Pair("#B7410E".into(), "#48CAE4".into()));
        let from_light = seeded(AccentSeed::Single("#B7410E".into()));
        let from_dark = seeded(AccentSeed::Single("#48CAE4".into()));
        let generated = super::super::color_scale::expand_color_scale(&ColorScaleConfig {
            accent: Some(AccentSeed::Pair("#B7410E".into(), "#48CAE4".into())),
            ..ColorScaleConfig::default()
        });
        let at = |theme: &DefinedTheme, mode| resolve_theme_tokens(Some(theme), mode);
        let (tuple_light, ref_light) = (
            at(&tuple, ResolvedThemeMode::Light),
            at(&from_light, ResolvedThemeMode::Light),
        );
        let (tuple_dark, ref_dark) = (
            at(&tuple, ResolvedThemeMode::Dark),
            at(&from_dark, ResolvedThemeMode::Dark),
        );
        for (key, _) in &generated {
            assert_eq!(tuple_light.get(key), ref_light.get(key), "{key} (light)");
            assert_eq!(tuple_dark.get(key), ref_dark.get(key), "{key} (dark)");
        }
        // The dark half does not reuse the light seed.
        assert_ne!(
            tuple_dark.get("--color-accent"),
            at(&from_light, ResolvedThemeMode::Dark).get("--color-accent")
        );
    }

    // Palettes: readPalette and withPaletteComponents.

    fn palette() -> ThemePalette {
        ThemePalette {
            kind: "theme-palette",
            recipe: super::super::palette::types::PaletteRecipe::AstryxOklchV1,
            tokens: vec![(
                "--color-accent".into(),
                ["#5b3cc4".into(), "#a08cf0".into()],
            )],
            components: vec![(
                "card".into(),
                vec![(
                    "variant:blue".into(),
                    super::super::palette::types::PaletteLift {
                        light: "#333333".into(),
                        dark: "#cccccc".into(),
                    },
                )],
            )],
        }
    }

    #[test]
    fn a_palette_writes_its_tokens_and_part_styles_under_the_themes_own() {
        let theme = define(DefineThemeInput {
            palette: Some(palette()),
            ..DefineThemeInput::new("acme")
        });
        assert_eq!(
            token(&theme, "--color-accent"),
            Some("light-dark(#5b3cc4, #a08cf0)")
        );
        assert_eq!(
            theme
                .input_tokens
                .as_ref()
                .and_then(|t| t.get("--color-accent")),
            Some(&pair("#5b3cc4", "#a08cf0"))
        );
        assert_eq!(
            rule(&theme, "card", "variant:blue"),
            Some(
                &[
                    (
                        "@light",
                        block([("--color-text-secondary", style("#333333"))])
                    ),
                    (
                        "@dark",
                        block([("--color-text-secondary", style("#cccccc"))])
                    ),
                ]
                .into()
            )
        );
        // A property the theme sets for both modes leaves the palette's blocks.
        let own = define(DefineThemeInput {
            palette: Some(palette()),
            components: components([(
                "card",
                block([(
                    "variant:blue",
                    block([("--color-text-secondary", style("#000000"))]),
                )]),
            )]),
            ..with_tokens("acme", [("--color-accent", css("#ff0000"))])
        });
        assert_eq!(
            rule(&own, "card", "variant:blue"),
            Some(&[("--color-text-secondary", style("#000000"))].into())
        );
        assert_eq!(token(&own, "--color-accent"), Some("#ff0000"));
    }

    #[test]
    fn refuses_a_palette_the_generator_did_not_make() {
        let message = error(DefineThemeInput {
            palette: Some(ThemePalette {
                kind: "seeds",
                ..palette()
            }),
            ..DefineThemeInput::new("acme")
        });
        assert_eq!(
            message,
            "defineTheme(\"acme\"): `palette` must be made by themePalette() from @astryxdesign/core/palette, got a plain object. Wrap the seeds: palette: themePalette({accent: '#5b3cc4'})."
        );
    }

    // resolveThemeGenerativeAxes.

    #[test]
    fn axes_inherit_what_a_child_leaves_out() {
        let base = define(DefineThemeInput {
            typography: Some(TypographyConfig {
                scale: Some(TypographyScale {
                    base: 14.0,
                    ratio: 1.2,
                }),
                body: role(Some("Inter"), Some("sans-serif")),
                heading: weighted(Some("bold"), &[(1, "normal")]),
                ..TypographyConfig::default()
            }),
            radius: Some(RadiusScaleConfig::default()),
            ..DefineThemeInput::new("base")
        });
        let child = define(DefineThemeInput {
            extends: Some(Arc::new(base)),
            typography: Some(TypographyConfig {
                heading: role(Some("Lora"), None),
                ..TypographyConfig::default()
            }),
            ..DefineThemeInput::new("child")
        });
        let axes = child.axes.unwrap();
        let typography = axes.typography.unwrap();
        // The inherited scale owns the weights; the child's heading family
        // replaces the body-following family whole.
        assert_eq!(
            typography.scale,
            Some(TypographyScale {
                base: 14.0,
                ratio: 1.2
            })
        );
        assert_eq!(
            typography.heading,
            Some(TypographyRole {
                family: Some("Lora".into()),
                fallbacks: None,
                weight: Some("bold".into()),
                weights: Some(vec![(1, "normal".into())]),
            })
        );
        assert_eq!(typography.body, role(Some("Inter"), Some("sans-serif")));
        assert_eq!(axes.radius, Some(RadiusScaleConfig::default()));
    }
}

#[cfg(test)]
mod icon_tests {
    use super::*;

    fn define(input: DefineThemeInput) -> Arc<DefinedTheme> {
        Arc::new(define_theme(&input).unwrap())
    }

    fn icons<const N: usize>(entries: [(&str, ThemeIconEntry); N]) -> Option<ThemeIcons> {
        Some(entries.into())
    }

    fn listed(names: &[&str]) -> Option<Vec<String>> {
        Some(names.iter().map(|name| name.to_string()).collect())
    }

    fn by_variant(icon: &str, variants: &[(&str, &str)]) -> ThemeIconEntry {
        ThemeIconEntry::Variants {
            icon: icon.to_string(),
            variants: variants
                .iter()
                .map(|(variant, path)| (*variant, path.to_string()))
                .collect(),
        }
    }

    // Upstream defineTheme.test.ts: "includes icons in the theme".
    #[test]
    fn includes_icons_in_the_theme() {
        let theme = define(DefineThemeInput {
            icons: icons([("close", "X".into())]),
            ..DefineThemeInput::new("icons")
        });
        assert_eq!(theme.icons, icons([("close", "X".into())]));
    }

    // Upstream: "reads function icon entries at md in the public icons view". A
    // by-variant entry and a fallback (`null`) entry are kept as written.
    #[test]
    fn keeps_by_variant_and_fallback_entries() {
        let entries = icons([
            ("info", by_variant("info-md", &[])),
            ("close", "X".into()),
            ("menu", ThemeIconEntry::Fallback),
        ]);
        let theme = define(DefineThemeInput {
            icons: entries.clone(),
            ..DefineThemeInput::new("fn-icons")
        });
        assert_eq!(theme.icons, entries);
        assert_eq!(theme.icon_entry_variants, None);
    }

    // Upstream: "merges icons — child overrides base".
    #[test]
    fn a_childs_icons_override_its_bases_by_name() {
        let base = define(DefineThemeInput {
            icons: icons([("close", "X".into()), ("menu", "M".into())]),
            ..DefineThemeInput::new("base")
        });
        let child = define(DefineThemeInput {
            extends: Some(base),
            icons: icons([("close", "Y".into())]),
            ..DefineThemeInput::new("child")
        });
        assert_eq!(
            child.icons,
            icons([("close", "Y".into()), ("menu", "M".into())])
        );
    }

    // Upstream: "inherits icons when child has none".
    #[test]
    fn a_child_with_no_icons_inherits_its_bases() {
        let base = define(DefineThemeInput {
            icons: icons([("close", "X".into())]),
            ..DefineThemeInput::new("base")
        });
        let child = define(DefineThemeInput {
            extends: Some(base),
            ..DefineThemeInput::new("child")
        });
        assert_eq!(child.icons, icons([("close", "X".into())]));
        assert_eq!(define(DefineThemeInput::new("bare")).icons, None);
    }

    // Upstream: "merges function icon entries by key and reads them at md".
    #[test]
    fn by_variant_entries_merge_by_name() {
        let base = define(DefineThemeInput {
            icons: icons([
                ("close", by_variant("base-close", &[])),
                ("menu", "M".into()),
            ]),
            ..DefineThemeInput::new("base")
        });
        let child = define(DefineThemeInput {
            extends: Some(base),
            icons: icons([("menu", by_variant("child-menu", &[]))]),
            ..DefineThemeInput::new("child")
        });
        assert_eq!(
            child.icons,
            icons([
                ("close", by_variant("base-close", &[])),
                ("menu", by_variant("child-menu", &[])),
            ])
        );
    }

    // Upstream `resolveIconEntryVariants`: an inherited by-variant entry keeps the list
    // of the theme that defined it when the child's list differs; an entry the child
    // defines, a static one, or one under the same list needs no record, and a
    // grandchild keeps its parent's record.
    #[test]
    fn an_inherited_by_variant_entry_keeps_its_defining_list() {
        let base = define(DefineThemeInput {
            icon_variants: listed(&["outline", "filled"]),
            icons: icons([
                ("info", by_variant("info", &[("outline", "info-outline")])),
                ("close", "X".into()),
            ]),
            ..DefineThemeInput::new("variants-base")
        });
        let same = define(DefineThemeInput {
            extends: Some(base.clone()),
            ..DefineThemeInput::new("variants-same")
        });
        assert_eq!(same.icon_entry_variants, None);
        let replaced = define(DefineThemeInput {
            extends: Some(base.clone()),
            icon_variants: listed(&["bold"]),
            icons: icons([("menu", by_variant("menu", &[("bold", "menu-bold")]))]),
            ..DefineThemeInput::new("variants-replace")
        });
        let record: OrderedMap<Option<Vec<String>>> =
            [("info", listed(&["outline", "filled"]))].into();
        assert_eq!(replaced.icon_entry_variants, Some(record.clone()));
        let grandchild = define(DefineThemeInput {
            extends: Some(Arc::new((*replaced).clone())),
            ..DefineThemeInput::new("variants-grandchild")
        });
        assert_eq!(grandchild.icon_entry_variants, Some(record));
        // A base with no list records `None` under a child that sets one.
        let unlisted = define(DefineThemeInput {
            icons: icons([("info", by_variant("info", &[]))]),
            ..DefineThemeInput::new("variants-none")
        });
        let listing = define(DefineThemeInput {
            extends: Some(unlisted),
            icon_variants: listed(&["outline"]),
            ..DefineThemeInput::new("variants-listing")
        });
        assert_eq!(listing.icon_entry_variants, Some([("info", None)].into()));
    }

    // Upstream `mergeComponentIcons`: the base's slots, then the theme's own, by slot;
    // an own value replaces the inherited one whole.
    #[test]
    fn component_icons_merge_by_slot() {
        let sort = ThemeComponentIcon::ByState(
            [
                (
                    "ascending",
                    Some(ThemeComponentIconRequest {
                        icon: Some(Some("arrowUp".into())),
                        ..ThemeComponentIconRequest::default()
                    }),
                ),
                ("none", None),
            ]
            .into(),
        );
        let base = define(DefineThemeInput {
            component_icons: Some(
                [
                    ("table-sort-indicator", sort.clone()),
                    (
                        "table-row-expand",
                        ThemeComponentIcon::Icon("chevronDown".into()),
                    ),
                ]
                .into(),
            ),
            ..DefineThemeInput::new("slots-base")
        });
        let child = define(DefineThemeInput {
            extends: Some(base.clone()),
            component_icons: Some([("table-row-expand", ThemeComponentIcon::Hidden)].into()),
            ..DefineThemeInput::new("slots-child")
        });
        assert_eq!(
            child.component_icons,
            Some(
                [
                    ("table-sort-indicator", sort),
                    ("table-row-expand", ThemeComponentIcon::Hidden),
                ]
                .into()
            )
        );
        let inherit = define(DefineThemeInput {
            extends: Some(base.clone()),
            ..DefineThemeInput::new("slots-inherit")
        });
        assert_eq!(inherit.component_icons, base.component_icons);
        let own = define(DefineThemeInput {
            component_icons: Some(
                [(
                    "table-row-expand",
                    ThemeComponentIcon::Request(ThemeComponentIconRequest {
                        size: Some("sm".into()),
                        ..ThemeComponentIconRequest::default()
                    }),
                )]
                .into(),
            ),
            ..DefineThemeInput::new("slots-own")
        });
        assert_eq!(
            own.component_icons.as_ref().map(|slots| slots.len()),
            Some(1)
        );
        assert_eq!(
            define(DefineThemeInput::new("slots-none")).component_icons,
            None
        );
    }
}
