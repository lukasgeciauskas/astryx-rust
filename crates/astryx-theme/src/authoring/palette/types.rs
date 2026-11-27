//! The palette engine's types: upstream `palette/types.ts`.
//!
//! A request names its families (a seed each, and the colors pinned at some of
//! their stops), a vibrancy, a neutral profile, the modes and the stops. The
//! generator answers with a candidate palette of `#rrggbb` ramps and the
//! evidence behind them. Upstream's string unions are enums here: each one's
//! `as_str` is upstream's spelling, and each `parse` reads it and refuses
//! anything else with the message upstream's generator throws.

use std::fmt;

use super::super::color::InvalidColor;

/// Why the palette engine refused a request, or could not map a palette:
/// upstream's thrown message, word for word (`Stop 120 must be a finite number
/// from 0 to 100.`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PaletteError(pub String);

impl PaletteError {
    pub(crate) fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl fmt::Display for PaletteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for PaletteError {}

impl From<InvalidColor> for PaletteError {
    fn from(error: InvalidColor) -> Self {
        Self(error.to_string())
    }
}

/// A color mode: upstream's `'light' | 'dark'` (`PaletteMode` in
/// `mapping.ts`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PaletteMode {
    /// `light`.
    Light,
    /// `dark`.
    Dark,
}

impl PaletteMode {
    /// Both modes, light first, as upstream lists them.
    pub const BOTH: [PaletteMode; 2] = [PaletteMode::Light, PaletteMode::Dark];

    /// Upstream's spelling: `light` or `dark`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }

    /// The mode upstream spells `name`.
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "light" => Some(Self::Light),
            "dark" => Some(Self::Dark),
            _ => None,
        }
    }

    /// The mode upstream spells `name`, or the generator's
    /// `Unknown anchor mode: <name>`.
    pub fn parse(name: &str) -> Result<Self, PaletteError> {
        Self::from_name(name).ok_or_else(|| PaletteError(format!("Unknown anchor mode: {name}")))
    }
}

impl fmt::Display for PaletteMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A palette recipe: the normative algorithm a request runs. Upstream
/// `'astryx-oklch-v1' | 'astryx-oklch-v2'`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PaletteRecipe {
    /// `astryx-oklch-v1`: the first recipe, which a request that names none runs.
    AstryxOklchV1,
    /// `astryx-oklch-v2`: pale stops follow the vibrancy and the seed's own chroma.
    AstryxOklchV2,
}

impl PaletteRecipe {
    /// Upstream's spelling, such as `astryx-oklch-v1`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AstryxOklchV1 => "astryx-oklch-v1",
            Self::AstryxOklchV2 => "astryx-oklch-v2",
        }
    }

    /// The recipe upstream spells `name`.
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "astryx-oklch-v1" => Some(Self::AstryxOklchV1),
            "astryx-oklch-v2" => Some(Self::AstryxOklchV2),
            _ => None,
        }
    }

    /// The recipe upstream spells `name`, or the generator's
    /// `Unknown palette recipe: <name> (known: astryx-oklch-v1, astryx-oklch-v2).`
    pub fn parse(name: &str) -> Result<Self, PaletteError> {
        Self::from_name(name).ok_or_else(|| {
            PaletteError(format!(
                "Unknown palette recipe: {name} (known: astryx-oklch-v1, astryx-oklch-v2)."
            ))
        })
    }
}

impl fmt::Display for PaletteRecipe {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// How an anchor holds its stop. Upstream `TonalPaletteAnchor['policy']`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AnchorPolicy {
    /// `exact`: the stop takes the chosen color.
    Exact,
    /// `bounded`: the stop moves toward the color until it is within
    /// `max_delta_e` of it.
    Bounded,
    /// `flexible`: the stop blends 35% of the way toward the color.
    Flexible,
}

impl AnchorPolicy {
    /// Upstream's spelling: `exact`, `bounded` or `flexible`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Exact => "exact",
            Self::Bounded => "bounded",
            Self::Flexible => "flexible",
        }
    }

    /// The policy upstream spells `name`, or the generator's
    /// `Unknown anchor policy: <name>`.
    pub fn parse(name: &str) -> Result<Self, PaletteError> {
        match name {
            "exact" => Ok(Self::Exact),
            "bounded" => Ok(Self::Bounded),
            "flexible" => Ok(Self::Flexible),
            _ => Err(PaletteError(format!("Unknown anchor policy: {name}"))),
        }
    }
}

impl fmt::Display for AnchorPolicy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// What a family's ramp grows from. Upstream
/// `TonalPaletteFamilyInput['kind']`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FamilyKind {
    /// `chromatic`: the seed's own hue and chroma.
    Chromatic,
    /// `neutral`: the request's neutral profile.
    Neutral,
}

impl FamilyKind {
    /// Upstream's spelling: `chromatic` or `neutral`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Chromatic => "chromatic",
            Self::Neutral => "neutral",
        }
    }

    /// The kind upstream spells `name`, or the generator's
    /// `Unknown family kind: <name>`.
    pub fn parse(name: &str) -> Result<Self, PaletteError> {
        match name {
            "chromatic" => Ok(Self::Chromatic),
            "neutral" => Ok(Self::Neutral),
            _ => Err(PaletteError(format!("Unknown family kind: {name}"))),
        }
    }
}

impl fmt::Display for FamilyKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The hue a `neutral` family takes. Upstream
/// `TonalPaletteGenerationInput['neutralProfile']`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NeutralProfile {
    /// `neutral-v1`: fully achromatic.
    NeutralV1,
    /// `warm-v1`: a slight tint at hue 75.
    WarmV1,
    /// `cool-v1`: a slight tint at hue 250.
    CoolV1,
    /// `custom`: the family's own seed.
    Custom,
}

impl NeutralProfile {
    /// Upstream's spelling, such as `neutral-v1`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NeutralV1 => "neutral-v1",
            Self::WarmV1 => "warm-v1",
            Self::CoolV1 => "cool-v1",
            Self::Custom => "custom",
        }
    }

    /// The profile upstream spells `name`, or the generator's
    /// `Unknown neutral profile: <name>`.
    pub fn parse(name: &str) -> Result<Self, PaletteError> {
        match name {
            "neutral-v1" => Ok(Self::NeutralV1),
            "warm-v1" => Ok(Self::WarmV1),
            "cool-v1" => Ok(Self::CoolV1),
            "custom" => Ok(Self::Custom),
            _ => Err(PaletteError(format!("Unknown neutral profile: {name}"))),
        }
    }
}

impl fmt::Display for NeutralProfile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Which modes a request generates. Upstream
/// `TonalPaletteGenerationInput['modeStrategy']`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ModeStrategy {
    /// `light-only`.
    LightOnly,
    /// `dark-only`.
    DarkOnly,
    /// `light-and-dark`, the default.
    LightAndDark,
}

impl ModeStrategy {
    /// Upstream's spelling, such as `light-and-dark`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::LightOnly => "light-only",
            Self::DarkOnly => "dark-only",
            Self::LightAndDark => "light-and-dark",
        }
    }

    /// The strategy upstream spells `name`, or the generator's
    /// `Unknown mode strategy: <name>`.
    pub fn parse(name: &str) -> Result<Self, PaletteError> {
        match name {
            "light-only" => Ok(Self::LightOnly),
            "dark-only" => Ok(Self::DarkOnly),
            "light-and-dark" => Ok(Self::LightAndDark),
            _ => Err(PaletteError(format!("Unknown mode strategy: {name}"))),
        }
    }
}

impl fmt::Display for ModeStrategy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A color an author pins at one stop of one mode, constraining generation.
/// Upstream `TonalPaletteAnchor`.
#[derive(Clone, Debug, PartialEq)]
pub struct TonalPaletteAnchor {
    /// Mode containing the anchored stop.
    pub mode: PaletteMode,
    /// Existing requested stop where the anchor applies.
    pub stop: f64,
    /// sRGB hex the stop is pulled toward: three or six digits, with or
    /// without `#`; the generator normalizes it.
    pub color: String,
    /// How the stop holds the color.
    pub policy: AnchorPolicy,
    /// Required non-negative perceptual-distance limit for a `bounded` anchor.
    /// Upstream `maxDeltaE`.
    pub max_delta_e: Option<f64>,
}

/// One requested family: a seed color and the constraints applied to its
/// ramp. Upstream `TonalPaletteFamilyInput`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TonalPaletteFamilyInput {
    /// Lower-kebab-case key for the family in the generated palette. `black`
    /// and `white` are reserved for the standalone values.
    pub id: String,
    /// sRGB hex the ramp is generated from: three or six digits, with or
    /// without `#`; the generator normalizes it.
    pub seed: String,
    /// Display name for review artifacts; defaults to `id`.
    pub name: Option<String>,
    /// `Neutral` derives the ramp from the neutral profile instead of the seed
    /// hue; defaults to `Chromatic`.
    pub kind: Option<FamilyKind>,
    /// Colors pinned at specific stops.
    pub anchors: Option<Vec<TonalPaletteAnchor>>,
}

/// Mutes the dark edge of every chromatic family. Their dark ramps keep
/// `edge_multiplier` of their in-gamut chroma through `through_stop`, then
/// recover along a smoothstep to the standard recipe at `recover_at_stop`.
/// Light ramps, neutral families, and the black and white endpoints do not
/// change. Upstream `TonalPaletteDarkChromaTaper`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TonalPaletteDarkChromaTaper {
    /// Share of chroma, from 0 to 1, that the dark edge keeps.
    pub edge_multiplier: f64,
    /// Last stop that takes the full edge multiplier; must be a requested stop.
    pub through_stop: f64,
    /// First stop back on the standard recipe; must be a requested stop after
    /// `through_stop`.
    pub recover_at_stop: f64,
    /// Edge multipliers, from 0 to 1, by chromatic family id, in place of
    /// `edge_multiplier`, in the order written.
    pub family_multipliers: Option<Vec<(String, f64)>>,
}

/// A tonal palette request. Upstream `TonalPaletteGenerationInput`; a
/// field left `None` is one the request leaves out.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TonalPaletteGenerationInput {
    /// The recipe to run: astryx-oklch-v1 when none is named, so a request or
    /// config made before v2 keeps its colors; astryx-oklch-v2 makes pale
    /// stops follow the vibrancy and the seed's own chroma.
    pub recipe: Option<PaletteRecipe>,
    /// The families, in palette order.
    pub families: Vec<TonalPaletteFamilyInput>,
    /// Chroma control from 0 (most muted) through 50 (default) to 100 (most
    /// vivid).
    pub vibrancy: Option<f64>,
    /// Hue treatment for `neutral` families; defaults to `neutral-v1`.
    pub neutral_profile: Option<NeutralProfile>,
    /// The modes generated; defaults to `light-and-dark`.
    pub mode_strategy: Option<ModeStrategy>,
    /// Mutes the dark edge of the chromatic families.
    pub dark_chroma_taper: Option<TonalPaletteDarkChromaTaper>,
    /// Ordered stops shared by every requested family; defaults to 0 through
    /// 100 in increments of 5. Decimal stops are supported.
    pub stops: Option<Vec<f64>>,
}

/// One family's colors in one mode: a `#rrggbb` per stop, in stop order.
/// Upstream's `Record<string, string>` keyed by the stop as text; two stops
/// are one key exactly when they are equal numbers.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PaletteRamp {
    entries: Vec<(f64, String)>,
}

impl PaletteRamp {
    /// An empty ramp.
    pub fn new() -> Self {
        Self::default()
    }

    /// The color at `stop`, if the ramp has that stop.
    pub fn get(&self, stop: f64) -> Option<&str> {
        self.entries
            .iter()
            .find(|(s, _)| *s == stop)
            .map(|(_, color)| color.as_str())
    }

    /// Sets the color at `stop`: in place when the ramp has the stop (as
    /// assigning an existing key keeps its place), else at the end.
    pub fn set(&mut self, stop: f64, color: String) {
        match self.entries.iter_mut().find(|(s, _)| *s == stop) {
            Some(entry) => entry.1 = color,
            None => self.entries.push((stop, color)),
        }
    }

    /// Each stop and its color, in the order they were set.
    pub fn iter(&self) -> impl Iterator<Item = (f64, &str)> {
        self.entries.iter().map(|(s, c)| (*s, c.as_str()))
    }

    /// The stops, in the order they were set.
    pub fn stops(&self) -> impl Iterator<Item = f64> + '_ {
        self.entries.iter().map(|(s, _)| *s)
    }

    /// How many stops the ramp has.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the ramp has no stop.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The stops in JavaScript's key order, as `Object.keys` lists upstream's
    /// record: the whole-number stops first, ascending, then the decimal
    /// ones in the order they were set.
    pub fn js_key_order(&self) -> Vec<f64> {
        let mut whole: Vec<f64> = self.stops().filter(|s| s.fract() == 0.0).collect();
        whole.sort_by(f64::total_cmp);
        whole.extend(self.stops().filter(|s| s.fract() != 0.0));
        whole
    }
}

impl FromIterator<(f64, String)> for PaletteRamp {
    fn from_iter<I: IntoIterator<Item = (f64, String)>>(iter: I) -> Self {
        let mut ramp = Self::new();
        for (stop, color) in iter {
            ramp.set(stop, color);
        }
        ramp
    }
}

/// One family of a candidate palette: upstream's
/// `TonalPaletteCandidate['palette'][id]` with its key.
#[derive(Clone, Debug, PartialEq)]
pub struct TonalPaletteFamily {
    /// The family id, the palette's key for it.
    pub id: String,
    /// Display name.
    pub name: String,
    /// The light ramp, when the request generates light.
    pub light: Option<PaletteRamp>,
    /// The dark ramp, when the request generates dark.
    pub dark: Option<PaletteRamp>,
}

impl TonalPaletteFamily {
    /// The family's ramp in `mode`, if generated.
    pub fn ramp(&self, mode: PaletteMode) -> Option<&PaletteRamp> {
        match mode {
            PaletteMode::Light => self.light.as_ref(),
            PaletteMode::Dark => self.dark.as_ref(),
        }
    }
}

/// A generated palette candidate. The palette is still subject to author
/// review and is not connected to runtime theme values. Upstream
/// `TonalPaletteCandidate`.
#[derive(Clone, Debug, PartialEq)]
pub struct TonalPaletteCandidate {
    /// Always 1.
    pub schema_version: u32,
    /// Always `candidate`.
    pub status: &'static str,
    /// The recipe that ran.
    pub recipe: PaletteRecipe,
    /// Exact solid black for theme authoring outside a tonal family.
    pub black: &'static str,
    /// Exact solid white for theme authoring outside a tonal family.
    pub white: &'static str,
    /// Canonical order for iterating the palette's stop lookup maps.
    pub stops: Vec<f64>,
    /// The families, in request order.
    pub palette: Vec<TonalPaletteFamily>,
}

impl TonalPaletteCandidate {
    /// The family with `id` (upstream `palette[id]`).
    pub fn family(&self, id: &str) -> Option<&TonalPaletteFamily> {
        self.palette.iter().find(|family| family.id == id)
    }

    /// The ramp of family `id` in `mode` (upstream `palette[id]?.[mode]`).
    pub fn ramp(&self, id: &str, mode: PaletteMode) -> Option<&PaletteRamp> {
        self.family(id).and_then(|family| family.ramp(mode))
    }
}

/// The named drift a ramp is at risk of. Upstream
/// `TonalPaletteRampDiagnostics['hueIdentityRisk']`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HueIdentityRisk {
    /// `blue-to-purple`.
    BlueToPurple,
    /// `yellow-to-brown`.
    YellowToBrown,
}

impl HueIdentityRisk {
    /// Upstream's spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::BlueToPurple => "blue-to-purple",
            Self::YellowToBrown => "yellow-to-brown",
        }
    }
}

/// An anchor with what its policy did: upstream's
/// `TonalPaletteAnchor & {generatedColor, deltaE}`, the color normalized.
#[derive(Clone, Debug, PartialEq)]
pub struct TonalPaletteAnchorResult {
    /// Mode containing the anchored stop.
    pub mode: PaletteMode,
    /// The anchored stop.
    pub stop: f64,
    /// The requested color, normalized to lowercase `#rrggbb`.
    pub color: String,
    /// The anchor's policy.
    pub policy: AnchorPolicy,
    /// A `bounded` anchor's limit.
    pub max_delta_e: Option<f64>,
    /// The color the stop received after the policy was applied. Upstream
    /// `generatedColor`.
    pub generated_color: String,
    /// The perceptual distance that color still has from the requested one.
    /// Upstream `deltaE`.
    pub delta_e: f64,
}

/// Per-ramp evidence recorded for one family in one mode. Upstream
/// `TonalPaletteRampDiagnostics`.
#[derive(Clone, Debug, PartialEq)]
pub struct TonalPaletteRampDiagnostics {
    /// Whether luminance rises across every stop.
    pub monotonic: bool,
    /// Smallest perceptual gap between neighboring stops; a small value
    /// means two stops read as one color.
    pub minimum_adjacent_delta_e: f64,
    /// Largest gap between neighboring stops.
    pub maximum_adjacent_delta_e: f64,
    /// Largest hue distance, in degrees, between a stop and the family's
    /// reference hue.
    pub maximum_hue_drift: f64,
    /// Named drift the ramp is at risk of.
    pub hue_identity_risk: Option<HueIdentityRisk>,
    /// Stops whose ideal color fell outside sRGB and was mapped back into it.
    pub gamut_mapped_stops: Vec<f64>,
    /// Each anchor with the color the stop received and its distance from
    /// the requested one.
    pub anchors: Vec<TonalPaletteAnchorResult>,
}

/// Cross-family evidence for one mode, sampled at the stop nearest 50.
/// Upstream `TonalPaletteCoordinationDiagnostics`.
#[derive(Clone, Debug, PartialEq)]
pub struct TonalPaletteCoordinationDiagnostics {
    /// Mode these samples come from.
    pub mode: PaletteMode,
    /// Sampled stop.
    pub stop: f64,
    /// The names of the two chromatic families hardest to tell apart, or
    /// `None` with fewer than two.
    pub closest_families: Option<(String, String)>,
    /// Perceptual distance between them.
    pub minimum_family_delta_e: Option<f64>,
    /// Most saturated family at this stop.
    pub strongest_family: Option<String>,
    /// Least saturated family at this stop.
    pub weakest_family: Option<String>,
    /// Strongest chroma over weakest; a large ratio means the families are
    /// unbalanced.
    pub chroma_ratio: Option<f64>,
}

/// A request's taper with every field filled in: upstream
/// `Required<TonalPaletteDarkChromaTaper>`.
#[derive(Clone, Debug, PartialEq)]
pub struct TonalPaletteNormalizedDarkChromaTaper {
    /// Share of chroma, from 0 to 1, that the dark edge keeps.
    pub edge_multiplier: f64,
    /// Last stop that takes the full edge multiplier.
    pub through_stop: f64,
    /// First stop back on the standard recipe.
    pub recover_at_stop: f64,
    /// Edge multipliers by chromatic family id, in the order written; empty
    /// when the request gives none.
    pub family_multipliers: Vec<(String, f64)>,
}

/// One family as the generator resolved it. Upstream
/// `TonalPaletteNormalizedRequest['families'][number]`.
#[derive(Clone, Debug, PartialEq)]
pub struct TonalPaletteNormalizedFamily {
    /// The trimmed id.
    pub id: String,
    /// The name, or the id.
    pub name: String,
    /// The normalized seed.
    pub seed: String,
    /// The kind, `Chromatic` when the request named none.
    pub kind: FamilyKind,
    /// The anchors, colors normalized; `max_delta_e` kept only on a bounded
    /// one.
    pub anchors: Vec<TonalPaletteAnchor>,
}

/// The request as the generator resolved it, with every default filled in.
/// Upstream `TonalPaletteNormalizedRequest`.
#[derive(Clone, Debug, PartialEq)]
pub struct TonalPaletteNormalizedRequest {
    /// The recipe.
    pub recipe: PaletteRecipe,
    /// The vibrancy.
    pub vibrancy: f64,
    /// The neutral profile.
    pub neutral_profile: NeutralProfile,
    /// The mode strategy.
    pub mode_strategy: ModeStrategy,
    /// The requested taper, with `family_multipliers` filled in; present only
    /// when the request has a taper.
    pub dark_chroma_taper: Option<TonalPaletteNormalizedDarkChromaTaper>,
    /// The stops.
    pub stops: Vec<f64>,
    /// The families.
    pub families: Vec<TonalPaletteNormalizedFamily>,
}

/// A part style's lifted secondary text, as color-mode blocks: upstream
/// `PaletteLift`, `{'@light': {'--color-text-secondary': light}, '@dark':
/// {'--color-text-secondary': dark}}`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PaletteLift {
    /// The `@light` block's `--color-text-secondary`.
    pub light: String,
    /// The `@dark` block's `--color-text-secondary`.
    pub dark: String,
}

/// The seeds `themePalette()` grows a theme's colors from. Upstream
/// `ThemePaletteInput`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ThemePaletteInput {
    /// The accent: buttons, links, focus rings, selection.
    pub accent: Option<String>,
    /// The grays of surfaces, text, borders and icons.
    pub neutral: Option<String>,
    /// The success status color.
    pub success: Option<String>,
    /// The warning status color.
    pub warning: Option<String>,
    /// The danger status color (the error tokens).
    pub danger: Option<String>,
    /// The info status color.
    pub info: Option<String>,
    /// Chroma as a whole number from 0 (most muted) through 50 (default) to
    /// 100 (most vivid).
    pub vibrancy: Option<f64>,
    /// The recipe: `astryx-oklch-v1` when none is named.
    pub recipe: Option<PaletteRecipe>,
}

/// A theme's colors made by `themePalette()`, for `defineTheme({palette})`.
/// Upstream `ThemePalette`.
#[derive(Clone, Debug, PartialEq)]
pub struct ThemePalette {
    /// Always `theme-palette`.
    pub kind: &'static str,
    /// The recipe that made it.
    pub recipe: PaletteRecipe,
    /// Each color token's `[light, dark]` value, in mapping order.
    pub tokens: Vec<(String, [String; 2])>,
    /// Part styles that lift secondary text on the accent tint, the neutral
    /// track and the coloured cards, as the official themes do, by part and
    /// style key.
    pub components: Vec<(String, Vec<(String, PaletteLift)>)>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enums_read_and_write_upstream_spellings() {
        for recipe in [PaletteRecipe::AstryxOklchV1, PaletteRecipe::AstryxOklchV2] {
            assert_eq!(PaletteRecipe::parse(recipe.as_str()), Ok(recipe));
        }
        for mode in PaletteMode::BOTH {
            assert_eq!(PaletteMode::parse(mode.as_str()), Ok(mode));
        }
        for strategy in [
            ModeStrategy::LightOnly,
            ModeStrategy::DarkOnly,
            ModeStrategy::LightAndDark,
        ] {
            assert_eq!(ModeStrategy::parse(strategy.as_str()), Ok(strategy));
        }
        for profile in [
            NeutralProfile::NeutralV1,
            NeutralProfile::WarmV1,
            NeutralProfile::CoolV1,
            NeutralProfile::Custom,
        ] {
            assert_eq!(NeutralProfile::parse(profile.as_str()), Ok(profile));
        }
        for policy in [
            AnchorPolicy::Exact,
            AnchorPolicy::Bounded,
            AnchorPolicy::Flexible,
        ] {
            assert_eq!(AnchorPolicy::parse(policy.as_str()), Ok(policy));
        }
        for kind in [FamilyKind::Chromatic, FamilyKind::Neutral] {
            assert_eq!(FamilyKind::parse(kind.as_str()), Ok(kind));
        }
    }

    #[test]
    fn enums_refuse_other_spellings_with_the_generators_messages() {
        // The messages generator.ts throws for each union.
        let message = |e: PaletteError| e.0;
        assert_eq!(
            message(PaletteRecipe::parse("astryx-oklch-v9").unwrap_err()),
            "Unknown palette recipe: astryx-oklch-v9 (known: astryx-oklch-v1, astryx-oklch-v2)."
        );
        assert_eq!(
            message(ModeStrategy::parse("both").unwrap_err()),
            "Unknown mode strategy: both"
        );
        assert_eq!(
            message(NeutralProfile::parse("sepia").unwrap_err()),
            "Unknown neutral profile: sepia"
        );
        assert_eq!(
            message(AnchorPolicy::parse("loose").unwrap_err()),
            "Unknown anchor policy: loose"
        );
        assert_eq!(
            message(FamilyKind::parse("gray").unwrap_err()),
            "Unknown family kind: gray"
        );
        assert_eq!(
            message(PaletteMode::parse("dim").unwrap_err()),
            "Unknown anchor mode: dim"
        );
    }

    #[test]
    fn ramp_lists_stops_in_javascript_key_order() {
        // Object.keys puts integer keys first, ascending, then the others in
        // insertion order: {0, 33.3, 50, 66.7, 100} lists 0, 50, 100 first.
        let ramp: PaletteRamp = [0.0, 33.3, 50.0, 66.7, 100.0]
            .into_iter()
            .map(|s| (s, format!("#{s}")))
            .collect();
        assert_eq!(ramp.js_key_order(), vec![0.0, 50.0, 100.0, 33.3, 66.7]);
        assert_eq!(ramp.get(33.3), Some("#33.3"));
        assert_eq!(ramp.get(40.0), None);
    }

    #[test]
    fn ramp_set_keeps_an_existing_stop_in_place() {
        let mut ramp = PaletteRamp::new();
        ramp.set(10.0, "#111111".into());
        ramp.set(20.0, "#222222".into());
        ramp.set(10.0, "#333333".into());
        assert_eq!(
            ramp.iter().collect::<Vec<_>>(),
            vec![(10.0, "#333333"), (20.0, "#222222")]
        );
        assert_eq!(ramp.len(), 2);
    }

    #[test]
    fn invalid_color_becomes_upstreams_message() {
        let error: PaletteError = InvalidColor("nope".into()).into();
        assert_eq!(error.to_string(), "Invalid color: nope");
    }
}
