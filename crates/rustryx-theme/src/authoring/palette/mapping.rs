//! A palette mapped to theme color tokens: upstream `palette/mapping.ts`.
//!
//! Given color settings (an accent, a neutral and the status colors, each a
//! [`PaletteDial`]), [`plan_palette_request`] writes the generator request
//! the official themes would: one family per setting, its seed anchored
//! exactly at its nearest light stop (and in dark too where it reads there).
//! [`map_palette`] then points each color token at a stop of the generated
//! palette in light and dark, as the official Neutral theme maps its
//! families, moving a stop only where WCAG contrast needs it (4.5:1 for
//! text, 3:1 for fills, focus rings and icons) and recording each check
//! behind a choice, with the sentence the theme editor reports.
//!
//! The mapping reads the theme around the palette through a
//! [`PaletteMappingContext`]: the base theme's resolved colors, whether a
//! token exists, a color reader, and the values set exactly. Upstream's
//! records are insertion-ordered vectors here where their order shows in
//! the output.

use std::collections::HashMap;

pub use super::super::color::{ColorTriple, composite_over};
use super::super::color::{
    InvalidColor, contrast_ratio, hex_to_oklch, normalize_color, ratio_down,
};
use super::super::js::{
    js_max, js_min, js_number_to_string, js_parse_int_hex, js_round, js_to_hex_string, js_trim,
};
use super::derive_look::CvdVision;
use super::editor_look::{EditorLookInput, EditorLookResult, PaletteEditorRules};
use super::generator::{
    PALETTE_BLACK, PALETTE_WHITE, compare, generate_tonal_palette, perceptual_delta,
};
pub use super::types::PaletteMode;
use super::types::{
    AnchorPolicy, FamilyKind, ModeStrategy, NeutralProfile, PaletteError, PaletteRamp,
    PaletteRecipe, TonalPaletteAnchor, TonalPaletteCandidate, TonalPaletteFamilyInput,
    TonalPaletteGenerationInput,
};

/// A mode's index in a `[light, dark]` pair.
pub(crate) fn mode_index(mode: PaletteMode) -> usize {
    match mode {
        PaletteMode::Light => 0,
        PaletteMode::Dark => 1,
    }
}

/// One token pointed at a palette stop in one mode. Upstream `StopRef`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StopRef {
    /// The stop.
    pub stop: f64,
    /// The stop's color, with `alpha` appended when set.
    pub color: String,
    /// Two hex digits of opacity.
    pub alpha: Option<String>,
    /// The family the stop is from when it is not the token's own: `neutral`
    /// for text on a fill, `black` or `white` for the palette's own ends,
    /// `literal` for a color outside the palette.
    pub family: Option<String>,
    /// The stop holds the seed's exact color (an exact anchor); upstream
    /// leaves the field out when false.
    pub exact: bool,
    /// The half is the base theme's own value in that mode (family
    /// `literal`); upstream leaves the field out when false.
    pub base: bool,
}

/// The nearest setting that passes a check. Upstream `ContrastCheck['fix']`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContrastFix {
    /// The setting or token to change.
    pub thing: String,
    /// Its value.
    pub value: String,
    /// The mode the value is for, when it is for one.
    pub mode: Option<PaletteMode>,
}

/// One contrast check behind a stop choice. Upstream `ContrastCheck`.
#[derive(Clone, Debug, PartialEq)]
pub struct ContrastCheck {
    /// The token checked.
    pub token: String,
    /// The mode.
    pub mode: PaletteMode,
    /// The stop chosen.
    pub stop: f64,
    /// The stop the mapping starts from.
    pub preferred: f64,
    /// What it was measured on.
    pub against: String,
    /// The lowest ratio over everything it was measured on.
    pub ratio: f64,
    /// The ratio it needs.
    pub needs: f64,
    /// Whether it reaches it.
    pub pass: bool,
    /// The whole sentence to report, when the check is not a plain stop move.
    pub note: Option<String>,
    /// The color settings whose notes report it, when its token belongs to
    /// none of them.
    pub dials: Option<Vec<String>>,
    /// The nearest setting that passes it, when one does: a darker exact
    /// color for a pale fill.
    pub fix: Option<ContrastFix>,
}

/// How the stop of an accent or status color's exact fill was settled.
/// Upstream `PaletteFill`.
#[derive(Clone, Debug, PartialEq)]
pub struct PaletteFill {
    /// The light stop the seed is anchored at.
    pub stop: f64,
    /// Whether the seed is anchored in dark too.
    pub dark: bool,
    /// Why not, when it is not.
    pub why: String,
    /// The ratio the exact color's text reads at in dark (or stands out at).
    pub ratio: f64,
}

/// Each color setting's fill, by setting id, in setting order. Upstream's
/// `Record<string, PaletteFill>`.
pub type PaletteFills = Vec<(String, PaletteFill)>;

/// Reads a fixed CSS color: its RGB (0–255) and its opacity (0–1).
/// Upstream `PaletteColorReader`.
pub trait PaletteColorReader {
    /// The color's red, green and blue, or `None` when it is not one fixed
    /// color.
    fn rgb(&self, value: &str) -> Option<ColorTriple>;
    /// The color's opacity, 0 to 1.
    fn alpha(&self, value: &str) -> f64;
}

/// A part-style value, as a theme's `components` hold it: a CSS value, a
/// `[light, dark]` pair, a block of further keys, or a number. Upstream reads
/// part styles as plain objects (`PaletteBase['components']`).
#[derive(Clone, Debug, PartialEq)]
pub enum StyleValue {
    /// A CSS value.
    Text(String),
    /// A list of values: a `[light, dark]` pair.
    List(Vec<String>),
    /// A block of keys and values, in the order written.
    Block(Vec<(String, StyleValue)>),
    /// A number.
    Number(f64),
}

impl StyleValue {
    /// A block's value at `key` (the last one written, as in an object
    /// literal); `None` for anything but a block.
    pub fn get(&self, key: &str) -> Option<&StyleValue> {
        match self {
            StyleValue::Block(entries) => {
                entries.iter().rev().find(|(k, _)| k == key).map(|(_, v)| v)
            }
            _ => None,
        }
    }
}

/// The resolved colors the mapping measures against, per mode. Upstream
/// `PaletteBase`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PaletteBase {
    /// Each token's resolved light value.
    pub light: HashMap<String, String>,
    /// Each token's resolved dark value.
    pub dark: HashMap<String, String>,
    /// Part styles, which may lift secondary text on some parts: each part's
    /// rule blocks by key, as [`StyleValue::Block`]s.
    pub components: Option<Vec<(String, StyleValue)>>,
}

impl PaletteBase {
    /// The resolved values of `mode`.
    pub fn mode(&self, mode: PaletteMode) -> &HashMap<String, String> {
        match mode {
            PaletteMode::Light => &self.light,
            PaletteMode::Dark => &self.dark,
        }
    }
}

/// A context's reading of a token or value in a mode: the value a token is
/// set to exactly, or the fixed color a written value gives. Upstream
/// `PaletteMappingContext['settingOf']` and `['colorIn']`.
pub type ModeLookup<'a> = &'a dyn Fn(&str, PaletteMode) -> Option<String>;

/// A context's line that sets a value (`thing`, `value`, the mode it is for).
/// Upstream `PaletteMappingContext['setLine']`.
pub type SetLine<'a> = &'a dyn Fn(&str, &str, Option<PaletteMode>) -> String;

/// Called with each anchor of the author's a dark fill replaces: the family
/// id and the stop. Upstream `anchorDarkFills`'s `replaced`.
pub type ReplacedAnchor<'r> = &'r mut dyn FnMut(&str, f64);

/// What the mapping reads around the palette. Upstream
/// `PaletteMappingContext`; a field left `None` is one the context leaves out.
#[derive(Clone, Copy)]
pub struct PaletteMappingContext<'a> {
    /// The base theme's resolved colors.
    pub base: Option<&'a PaletteBase>,
    /// The theme editor's own rules, beyond `themePalette()`'s mapping: the
    /// colors a look grows (category and chart colors, the status roles a
    /// theme leaves unset, code colors) and a base theme's own pairs kept
    /// readable for those colors. They never change a token `themePalette()`
    /// maps. The editor passes [`EditorLook`](super::editor_look::EditorLook).
    pub editor: Option<&'a dyn PaletteEditorRules>,
    /// Whether a token exists, so the mapping sets it.
    pub has: &'a dyn Fn(&str) -> bool,
    /// Core's default value of each token, as written: a token the official
    /// theme leaves to Core follows the family at Core's lightness.
    pub defaults: Option<&'a [(&'a str, &'a str)]>,
    /// The color reader.
    pub reader: &'a dyn PaletteColorReader,
    /// The value a token is set to exactly in a mode, as written.
    pub setting_of: Option<ModeLookup<'a>>,
    /// The fixed color a written value gives in a mode, or `None`.
    pub color_in: Option<ModeLookup<'a>>,
    /// Whether a token is set exactly in any mode.
    pub set_exactly: Option<&'a dyn Fn(&str) -> bool>,
    /// The line that sets a value, for a note that offers one; without it,
    /// the note offers none.
    pub set_line: Option<SetLine<'a>>,
    /// The modes the theme shows: the derived colors' notes name only these.
    pub shown: Option<&'a [PaletteMode]>,
}

impl<'a> PaletteMappingContext<'a> {
    /// A context with only what every mapping needs: which tokens exist, and
    /// a color reader.
    pub fn new(has: &'a dyn Fn(&str) -> bool, reader: &'a dyn PaletteColorReader) -> Self {
        Self {
            base: None,
            editor: None,
            has,
            defaults: None,
            reader,
            setting_of: None,
            color_in: None,
            set_exactly: None,
            set_line: None,
            shown: None,
        }
    }
}

/// One color setting: a palette family of the same id. Upstream
/// `PaletteDial`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PaletteDial {
    /// The setting, one of [`PALETTE_DIALS`].
    pub id: String,
    /// Its seed color.
    pub seed: String,
    /// Set for one mode only (an accent token): the editor keeps the other
    /// mode as it was.
    pub only: Option<PaletteMode>,
}

/// A family of a palette config: the generator's family input. Upstream
/// `ConfigFamily`, which also carries any key of the author's; a Rust config
/// holds the fields the generator reads.
pub type ConfigFamily = TonalPaletteFamilyInput;

/// A palette config: the generator's request. Upstream `PaletteConfig`.
pub type PaletteConfig = TonalPaletteGenerationInput;

/// One family of a planned request, with its exact seed anchors. Upstream
/// `PaletteRequestFamily`.
pub type PaletteRequestFamily = TonalPaletteFamilyInput;

/// A token's stops in both modes, and its family. Upstream's value of
/// `PaletteMapping['tokens']`.
#[derive(Clone, Debug, PartialEq)]
pub struct MappedToken {
    /// The family the token follows.
    pub family: String,
    /// The light stop.
    pub light: StopRef,
    /// The dark stop.
    pub dark: StopRef,
}

impl MappedToken {
    /// The stop of `mode`.
    pub fn get(&self, mode: PaletteMode) -> &StopRef {
        match mode {
            PaletteMode::Light => &self.light,
            PaletteMode::Dark => &self.dark,
        }
    }

    /// Replaces the stop of `mode`.
    pub fn set(&mut self, mode: PaletteMode, stop: StopRef) {
        match mode {
            PaletteMode::Light => self.light = stop,
            PaletteMode::Dark => self.dark = stop,
        }
    }
}

/// Mapped tokens by name, in the order first set: upstream's
/// `Record<string, {family, light, dark}>`. Setting a token again keeps its
/// place.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MappedTokens {
    entries: Vec<(String, MappedToken)>,
}

impl MappedTokens {
    /// No tokens.
    pub fn new() -> Self {
        Self::default()
    }

    /// The token named `name`.
    pub fn get(&self, name: &str) -> Option<&MappedToken> {
        self.entries.iter().find(|(k, _)| k == name).map(|(_, v)| v)
    }

    /// Whether the token is mapped (upstream `name in tokens`).
    pub fn contains(&self, name: &str) -> bool {
        self.get(name).is_some()
    }

    /// Sets a token: in place when it is already mapped, else at the end.
    pub fn insert(&mut self, name: impl Into<String>, token: MappedToken) {
        let name = name.into();
        match self.entries.iter_mut().find(|(k, _)| *k == name) {
            Some(entry) => entry.1 = token,
            None => self.entries.push((name, token)),
        }
    }

    /// The color a token takes in a mode (upstream `tokens[name]?.[mode]?.color`).
    pub fn color(&self, name: &str, mode: PaletteMode) -> Option<&str> {
        self.get(name).map(|token| token.get(mode).color.as_str())
    }

    /// Each token and its stops, in order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &MappedToken)> {
        self.entries.iter().map(|(k, v)| (k.as_str(), v))
    }

    /// How many tokens are mapped.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether no token is mapped.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// A value per mode, either one possibly missing. Upstream's
/// `Partial<Record<PaletteMode, string>>`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ModeValues {
    /// The light value.
    pub light: Option<String>,
    /// The dark value.
    pub dark: Option<String>,
}

/// A stop in each mode: the lifted secondary text on one background.
/// Upstream's `Partial<Record<PaletteMode, StopRef>>` of
/// `PaletteMapping['lift']`, which holds a background only with both modes.
#[derive(Clone, Debug, PartialEq)]
pub struct ModeStopRefs {
    /// The light stop.
    pub light: StopRef,
    /// The dark stop.
    pub dark: StopRef,
}

/// Two status fills someone red-weak or green-weak may not tell apart.
/// Upstream's `PaletteMapping['cvd'][number]`.
#[derive(Clone, Debug, PartialEq)]
pub struct CvdClash {
    /// One status word.
    pub a: String,
    /// The other.
    pub b: String,
    /// The vision that may confuse them.
    pub vision: CvdVision,
    /// The mode.
    pub mode: PaletteMode,
    /// Their fills.
    pub colors: [String; 2],
    /// The page they show on in that mode.
    pub page: Option<String>,
    /// Every mode's page.
    pub pages: Option<Vec<String>>,
}

/// A text/fill pair a base theme draws: a category's text on its fill, a
/// status's text on its fill or its tint, or the text a part's rule block
/// sets on its fill. The editor words it. Upstream `BasePair`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BasePair {
    /// A category's text on its fill.
    Category {
        /// The category hue.
        hue: String,
    },
    /// A status's text on its fill, or on its tint.
    Status {
        /// The status word.
        role: String,
        /// Whether the fill is the tint.
        tint: bool,
    },
    /// A part's text color on its background.
    Part {
        /// The part.
        part: String,
        /// The rule block's key.
        block: String,
        /// The text property.
        prop: String,
    },
}

/// A look color kept at the base theme's in a mode, because a text/fill pair
/// the base theme draws with it would not read. Upstream's
/// `PaletteMapping['kept'][number]`.
#[derive(Clone, Debug, PartialEq)]
pub struct KeptAtBase {
    /// The token kept.
    pub token: String,
    /// The mode.
    pub mode: PaletteMode,
    /// The pair it would have broken.
    pub pair: BasePair,
    /// The ratio the pair would have read at, to two decimals.
    pub ratio: f64,
}

/// The status roles the theme leaves unset, each derived from the look.
/// Upstream `PaletteMapping['statuses']`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StatusMapping {
    /// The roles derived, by dial id (upstream's `Record<string, 'derived'>`).
    pub roles: Vec<String>,
    /// What they gave up to stay written.
    pub why: Vec<String>,
    /// The roles that gave something up.
    pub short: Vec<String>,
}

/// Whether a set of colors follows the look: upstream's `'derived' | 'none'`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DerivedState {
    /// `derived`: written from the palette.
    Derived,
    /// `none`: left to the base theme.
    #[default]
    None,
}

impl DerivedState {
    /// Upstream's spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Derived => "derived",
            Self::None => "none",
        }
    }
}

/// Whether the category colors (Badge, Token, Card tints) and the chart
/// colors follow the look, and what they gave up to stay written. Upstream
/// `PaletteMapping['categories']`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CategoryMapping {
    /// The category colors.
    pub badges: DerivedState,
    /// The chart colors.
    pub chart: DerivedState,
    /// Each limit no shade meets.
    pub limits: Vec<String>,
    /// Each color moved apart from an earlier one.
    pub moved: Vec<String>,
    /// Whether a color is still drawn like an earlier one.
    pub alike: bool,
    /// Whether a color is under its contrast at its most readable shade.
    pub unread: bool,
}

/// The token mapping of a palette. Upstream `PaletteMapping`.
#[derive(Clone, Debug, PartialEq)]
pub struct PaletteMapping {
    /// Each token's stops.
    pub tokens: MappedTokens,
    /// The checks behind the choices.
    pub contrast: Vec<ContrastCheck>,
    /// Main tokens set exactly, per mode: the color's fill gives way to them.
    pub exact: Vec<(String, ModeValues)>,
    /// Main tokens set exactly to a value that is not one readable color in
    /// some mode, as written.
    pub unread: Vec<(String, String)>,
    /// Lifted secondary text per background token of [`LIFT_PARTS`]; `None`
    /// without one.
    pub lift: Option<Vec<(String, ModeStopRefs)>>,
    /// Status fills someone red-weak or green-weak may not tell apart.
    pub cvd: Vec<CvdClash>,
    /// The code colors that keep the base theme's because none of their
    /// family's stops reads on the code background and the page.
    pub code: Vec<String>,
    /// The look's colors kept at the base theme's in a mode.
    pub kept: Vec<KeptAtBase>,
    /// The status roles the theme leaves unset.
    pub statuses: StatusMapping,
    /// The category and chart colors.
    pub categories: CategoryMapping,
}

/// The color settings. Each one is a palette family of the same id.
/// Upstream `PALETTE_DIALS`.
pub const PALETTE_DIALS: [&str; 6] = ["accent", "neutral", "success", "warning", "danger", "info"];

/// The generator's vibrancy when the theme sets none. Upstream
/// `VIBRANCY_DEFAULT`.
pub const VIBRANCY_DEFAULT: f64 = 50.0;

/// The Core token word of each status family. Upstream `STATUS_TOKEN`.
pub const STATUS_TOKEN: [(&str, &str); 4] = [
    ("success", "success"),
    ("warning", "warning"),
    ("danger", "error"),
    ("info", "info"),
];

/// The Core token word of a status family (`danger` is `error`). Upstream
/// `STATUS_TOKEN[dial]`.
pub fn status_token(dial: &str) -> Option<&'static str> {
    STATUS_TOKEN
        .iter()
        .find(|(id, _)| *id == dial)
        .map(|(_, word)| *word)
}

/// The main token a color setting feeds: the exact-token escape hatch for
/// it. Upstream `paletteToken` (an unknown setting gives
/// `--color-undefined`, as there).
pub fn palette_token(dial: &str) -> String {
    match dial {
        "accent" => "--color-accent".to_string(),
        "neutral" => "--color-background-surface".to_string(),
        _ => format!("--color-{}", status_token(dial).unwrap_or("undefined")),
    }
}

/// Text contrast (WCAG 2.2 SC 1.4.3). Upstream `TEXT`.
pub const TEXT: f64 = 4.5;
/// Non-text contrast: fills, focus rings, icons (SC 1.4.11). Upstream
/// `NON_TEXT`.
pub const NON_TEXT: f64 = 3.0;

/// A stop of the official Neutral theme's mapping, with an opacity where it
/// has one. Upstream's `{stop, alpha?}`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OfficialStop {
    /// The stop.
    pub stop: u8,
    /// Two hex digits of opacity.
    pub alpha: Option<&'static str>,
}

const fn stop_at(stop: u8) -> OfficialStop {
    OfficialStop { stop, alpha: None }
}

const fn stop_tint(stop: u8, alpha: &'static str) -> OfficialStop {
    OfficialStop {
        stop,
        alpha: Some(alpha),
    }
}

/// The official Neutral theme's color mapping: each token's stops,
/// `[light, dark]`, with an opacity where it has one. A test keeps it equal
/// to the Neutral theme's definition. Upstream `OFFICIAL_NEUTRAL_STOPS`.
pub const OFFICIAL_NEUTRAL_STOPS: [(&str, [OfficialStop; 2]); 23] = [
    ("--color-background-body", [stop_at(95), stop_at(10)]),
    ("--color-background-surface", [stop_at(100), stop_at(15)]),
    ("--color-background-card", [stop_at(100), stop_at(10)]),
    ("--color-background-popover", [stop_at(100), stop_at(10)]),
    ("--color-background-muted", [stop_at(95), stop_at(10)]),
    ("--color-text-primary", [stop_at(0), stop_at(100)]),
    ("--color-text-secondary", [stop_at(30), stop_at(65)]),
    ("--color-text-disabled", [stop_at(60), stop_at(35)]),
    ("--color-icon-primary", [stop_at(0), stop_at(100)]),
    ("--color-icon-secondary", [stop_at(45), stop_at(65)]),
    ("--color-icon-disabled", [stop_at(60), stop_at(35)]),
    ("--color-border-emphasized", [stop_at(85), stop_at(35)]),
    ("--color-skeleton", [stop_at(85), stop_at(35)]),
    ("--color-on-dark", [stop_at(100), stop_at(100)]),
    ("--color-on-light", [stop_at(5), stop_at(5)]),
    ("--color-border", [stop_tint(0, "14"), stop_tint(100, "1a")]),
    (
        "--color-overlay-hover",
        [stop_tint(0, "0d"), stop_tint(100, "0d")],
    ),
    (
        "--color-overlay-pressed",
        [stop_tint(0, "1a"), stop_tint(100, "1a")],
    ),
    ("--color-text-info", [stop_at(45), stop_at(80)]),
    ("--color-accent-muted", [stop_at(85), stop_at(30)]),
    ("--color-info-muted", [stop_at(85), stop_tint(75, "3d")]),
    ("--color-on-info-muted", [stop_at(25), stop_at(80)]),
    ("--color-on-accent", [stop_at(100), stop_at(5)]),
];

/// The parts whose secondary text the official Neutral theme lifts,
/// `(part, block)`. Upstream `OFFICIAL_NEUTRAL_LIFTED`.
pub const OFFICIAL_NEUTRAL_LIFTED: [(&str, &str); 14] = [
    ("item", "selected"),
    ("tree-list-item", "selected"),
    ("file-input", "mode:dropzone"),
    ("citation", "variant:number"),
    ("segmented-control-item", "base"),
    ("card", "variant:blue"),
    ("card", "variant:cyan"),
    ("card", "variant:green"),
    ("card", "variant:orange"),
    ("card", "variant:pink"),
    ("card", "variant:purple"),
    ("card", "variant:red"),
    ("card", "variant:teal"),
    ("card", "variant:yellow"),
];

/// A token's official `[light, dark]` stops. Upstream `officialStops`.
pub fn official_stops(token: &str) -> Option<[f64; 2]> {
    OFFICIAL_NEUTRAL_STOPS
        .iter()
        .find(|(name, _)| *name == token)
        .map(|(_, [light, dark])| [f64::from(light.stop), f64::from(dark.stop)])
}

/// The tokens the neutral color sets at the official theme's stops,
/// `[light, dark]`: the page background and the surfaces on it, text and
/// icons, and the inverted surface (tooltips, toasts), which is the card
/// surface of the other mode, kept inside the ramp. A layout without one of
/// these stops uses the nearest one it has. Upstream `NEUTRAL_STOPS`, which
/// derives them from [`OFFICIAL_NEUTRAL_STOPS`] (a test keeps them equal).
pub const NEUTRAL_STOPS: [(&str, [f64; 2]); 16] = [
    ("--color-background-body", [95.0, 10.0]),
    ("--color-background-surface", [100.0, 15.0]),
    ("--color-background-card", [100.0, 10.0]),
    ("--color-background-popover", [100.0, 10.0]),
    ("--color-background-muted", [95.0, 10.0]),
    ("--color-text-primary", [0.0, 100.0]),
    ("--color-text-secondary", [30.0, 65.0]),
    ("--color-text-disabled", [60.0, 35.0]),
    ("--color-icon-primary", [0.0, 100.0]),
    ("--color-icon-secondary", [45.0, 65.0]),
    ("--color-icon-disabled", [60.0, 35.0]),
    ("--color-border-emphasized", [85.0, 35.0]),
    ("--color-skeleton", [85.0, 35.0]),
    ("--color-on-dark", [100.0, 100.0]),
    ("--color-on-light", [5.0, 5.0]),
    ("--color-background-inverted", [10.0, 100.0]),
];

/// A token's [`NEUTRAL_STOPS`] pair.
fn neutral_stops(token: &str) -> Option<[f64; 2]> {
    NEUTRAL_STOPS
        .iter()
        .find(|(name, _)| *name == token)
        .map(|(_, pair)| *pair)
}

/// Neutral tokens whose shade stays inside the ramp, off its white and black
/// ends. Upstream `NEUTRAL_INSIDE`.
pub const NEUTRAL_INSIDE: [&str; 1] = ["--color-background-inverted"];

/// Neutral tints: a stop with an opacity, `[(stop, alpha), (stop, alpha)]`.
/// Upstream `NEUTRAL_TINTS`.
pub const NEUTRAL_TINTS: [(&str, [(f64, &str); 2]); 3] = [
    ("--color-border", [(0.0, "14"), (100.0, "1a")]),
    ("--color-overlay-hover", [(0.0, "0d"), (100.0, "0d")]),
    ("--color-overlay-pressed", [(0.0, "1a"), (100.0, "1a")]),
];

/// The dark fill's starting stop when the exact color does not read in dark.
/// Upstream `DARK_FILL`.
pub const DARK_FILL: f64 = 70.0;
/// Accent and status text, `[light, dark]`, as the official Neutral theme
/// maps `--color-text-info`. Upstream `TEXT_STOP`.
pub const TEXT_STOP: [f64; 2] = [45.0, 80.0];
/// The accent tint, `[light, dark]`. Upstream `ACCENT_MUTED`.
pub const ACCENT_MUTED: [f64; 2] = [85.0, 30.0];
/// A status tint, `[light, dark]`: translucent in dark. Upstream
/// `STATUS_MUTED`.
pub const STATUS_MUTED: [OfficialStop; 2] = [stop_at(85), stop_tint(75, "3d")];
/// Text on a status tint, `[light, dark]`. Upstream `ON_MUTED`.
pub const ON_MUTED: [f64; 2] = [25.0, 80.0];
/// A status border, `[light, dark]`. Upstream `BORDER`.
pub const BORDER: [f64; 2] = [80.0, 65.0];

/// The parts whose secondary text the official Neutral theme lifts, with the
/// block it lifts it in and what the text sits on: a card's own fill for a
/// card colour, the translucent neutral track under segmented-control
/// labels, else the accent tint. Upstream `LIFT_PARTS` (a test keeps it equal
/// to [`OFFICIAL_NEUTRAL_LIFTED`] read through `liftGround`).
pub const LIFT_PARTS: [(&str, &str, &str); 14] = [
    ("item", "selected", "--color-accent-muted"),
    ("tree-list-item", "selected", "--color-accent-muted"),
    ("file-input", "mode:dropzone", "--color-accent-muted"),
    ("citation", "variant:number", "--color-accent-muted"),
    ("segmented-control-item", "base", "--color-neutral"),
    ("card", "variant:blue", "--color-background-blue"),
    ("card", "variant:cyan", "--color-background-cyan"),
    ("card", "variant:green", "--color-background-green"),
    ("card", "variant:orange", "--color-background-orange"),
    ("card", "variant:pink", "--color-background-pink"),
    ("card", "variant:purple", "--color-background-purple"),
    ("card", "variant:red", "--color-background-red"),
    ("card", "variant:teal", "--color-background-teal"),
    ("card", "variant:yellow", "--color-background-yellow"),
];

/// What a lifted block's secondary text sits on: a card's own fill for a
/// card colour (`variant:<color>`), the translucent neutral track under
/// segmented-control labels, else the accent tint. Upstream `liftGround`.
pub fn lift_ground(part: &str, block: &str) -> String {
    if part == "segmented-control-item" {
        return "--color-neutral".to_string();
    }
    let color = if part == "card" {
        block.strip_prefix("variant:").filter(|c| {
            !c.is_empty()
                && c.chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        })
    } else {
        None
    };
    match color {
        Some(color) => format!("--color-background-{color}"),
        None => "--color-accent-muted".to_string(),
    }
}

/// What a lifted background is, and the parts on it, for notes. Upstream
/// `LIFT_GROUNDS[ground]`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LiftGround {
    /// What the background is (`the accent tint`).
    pub what: &'static str,
    /// The parts on it.
    pub parts: &'static str,
}

/// Each lifted background and what it is. Upstream `LIFT_GROUNDS`.
pub const LIFT_GROUNDS: [(&str, LiftGround); 11] = [
    (
        "--color-accent-muted",
        LiftGround {
            what: "the accent tint",
            parts: "selected items and rows, a dropzone, numbered citations",
        },
    ),
    (
        "--color-neutral",
        LiftGround {
            what: "the neutral track",
            parts: "segmented-control labels",
        },
    ),
    (
        "--color-background-blue",
        LiftGround {
            what: "the blue card fill",
            parts: "blue cards",
        },
    ),
    (
        "--color-background-cyan",
        LiftGround {
            what: "the cyan card fill",
            parts: "cyan cards",
        },
    ),
    (
        "--color-background-green",
        LiftGround {
            what: "the green card fill",
            parts: "green cards",
        },
    ),
    (
        "--color-background-orange",
        LiftGround {
            what: "the orange card fill",
            parts: "orange cards",
        },
    ),
    (
        "--color-background-pink",
        LiftGround {
            what: "the pink card fill",
            parts: "pink cards",
        },
    ),
    (
        "--color-background-purple",
        LiftGround {
            what: "the purple card fill",
            parts: "purple cards",
        },
    ),
    (
        "--color-background-red",
        LiftGround {
            what: "the red card fill",
            parts: "red cards",
        },
    ),
    (
        "--color-background-teal",
        LiftGround {
            what: "the teal card fill",
            parts: "teal cards",
        },
    ),
    (
        "--color-background-yellow",
        LiftGround {
            what: "the yellow card fill",
            parts: "yellow cards",
        },
    ),
];

/// Text on a fill, from the neutral family as the official themes take it:
/// white (100) first in light, near-black (5) first in dark. Upstream
/// `ON_FILL[mode]`.
pub fn on_fill(mode: PaletteMode) -> [f64; 2] {
    match mode {
        PaletteMode::Light => [100.0, 5.0],
        PaletteMode::Dark => [5.0, 100.0],
    }
}

/// Which way text on surfaces moves for contrast: darker (-1) in light mode,
/// lighter (1) in dark mode. Upstream `TOWARD_CONTRAST[mode]`.
pub fn toward_contrast(mode: PaletteMode) -> f64 {
    match mode {
        PaletteMode::Light => -1.0,
        PaletteMode::Dark => 1.0,
    }
}

/// Below this OKLCH chroma a seed reads as gray: its family is generated as a
/// neutral, so no hue is invented for it in either mode.
const GRAY_CHROMA: f64 = 0.02;

/// Whether a seed reads as gray. Upstream `isGray`.
pub fn is_gray(seed: &str) -> Result<bool, InvalidColor> {
    Ok(hex_to_oklch(seed)?.c < GRAY_CHROMA)
}

/// Whether two values are one color: both normalize to the same hex.
/// Upstream `sameColor`.
pub fn same_color(a: &str, b: &str) -> bool {
    match (normalize_color(a), normalize_color(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

/// RGB channels (0–255; the first three, each clamped and rounded) as a
/// lowercase six-digit hex. Upstream `hex`.
pub fn hex(rgb: &[f64]) -> String {
    let mut out = String::from("#");
    for &x in rgb.iter().take(3) {
        let digits = js_to_hex_string(js_round(js_max(0.0, js_min(255.0, x))));
        out.push_str(&format!("{digits:0>2}"));
    }
    out
}

/// The WCAG contrast ratio of two RGB colors, each rounded to a hex first.
/// Upstream `ratio`.
pub fn ratio(a: &[f64], b: &[f64]) -> f64 {
    contrast_ratio(&hex(a), &hex(b))
}

/// A ratio cut down to two decimals, never rounded up past a need. Upstream
/// `round` (`ratioDown`).
pub fn round(ratio: f64) -> f64 {
    ratio_down(ratio)
}

/// A stop as text: a layout may hold 16.666666666666668. Upstream `shade`.
pub fn shade(stop: f64) -> String {
    js_number_to_string(round(stop))
}

/// The stop of a ramp whose shade is nearest a color, as the eye compares
/// them; never an end stop (pure black or white, which every ramp shares).
/// `None` for a ramp with no inner stop. Upstream `nearestShade`.
pub fn nearest_shade(ramp: &PaletteRamp, color: &str) -> Result<Option<f64>, InvalidColor> {
    let mut best: Option<(f64, f64)> = None;
    for stop in ramp.js_key_order() {
        if stop <= 0.0 || stop >= 100.0 {
            continue;
        }
        let delta = perceptual_delta(ramp.get(stop).unwrap_or_default(), color)?;
        match best {
            Some((_, nearest)) if delta >= nearest => {}
            _ => best = Some((stop, delta)),
        }
    }
    Ok(best.map(|(stop, _)| stop))
}

/// The stop of a layout nearest `n`; on a tie, the one nearer the middle.
/// `None` for an empty layout (where upstream's `reduce` throws). Upstream
/// `nearestStop` (mapping.ts).
pub fn nearest_stop(stops: &[f64], n: f64) -> Option<f64> {
    let (&first, rest) = stops.split_first()?;
    Some(rest.iter().fold(first, |best, &s| {
        let d = (s - n).abs() - (best - n).abs();
        if d < 0.0 || (d == 0.0 && (s - 50.0).abs() < (best - 50.0).abs()) {
            s
        } else {
            best
        }
    }))
}

/// The light stop nearest a seed color in the ramp the generator grows from
/// it (with the config's stops and vibrancy; a neutral seed under the
/// `custom` profile). The official themes anchor a brand color there,
/// exactly, so the fill keeps the color as given. The end stops never hold
/// an anchor; a ramp with no inner stop gives 0. Upstream `seedStop`.
pub fn seed_stop(
    seed: &str,
    config: &PaletteConfig,
    kind: FamilyKind,
) -> Result<f64, PaletteError> {
    let candidate = generate_tonal_palette(&TonalPaletteGenerationInput {
        stops: config.stops.clone(),
        vibrancy: config.vibrancy,
        neutral_profile: (kind == FamilyKind::Neutral).then_some(NeutralProfile::Custom),
        mode_strategy: Some(ModeStrategy::LightOnly),
        families: vec![TonalPaletteFamilyInput {
            id: "seed".to_string(),
            seed: seed.to_string(),
            kind: Some(kind),
            ..TonalPaletteFamilyInput::default()
        }],
        ..TonalPaletteGenerationInput::default()
    })?;
    let ramp = candidate
        .ramp("seed", PaletteMode::Light)
        .cloned()
        .unwrap_or_default();
    Ok(nearest_shade(&ramp, seed)?.unwrap_or(0.0))
}

/// A gray ramp the generator grows from a gray of the theme (its secondary
/// text), for a mode: the grays of a theme whose palette has no neutral
/// family. `None` when the seed does not grow. Upstream `grownGrays`.
pub fn grown_grays(seed: &str, mode: PaletteMode) -> Option<PaletteRamp> {
    let candidate = generate_tonal_palette(&TonalPaletteGenerationInput {
        mode_strategy: Some(match mode {
            PaletteMode::Light => ModeStrategy::LightOnly,
            PaletteMode::Dark => ModeStrategy::DarkOnly,
        }),
        neutral_profile: Some(NeutralProfile::Custom),
        families: vec![TonalPaletteFamilyInput {
            id: "gray".to_string(),
            seed: seed.to_string(),
            kind: Some(FamilyKind::Neutral),
            ..TonalPaletteFamilyInput::default()
        }],
        ..TonalPaletteGenerationInput::default()
    })
    .ok()?;
    candidate.ramp("gray", mode).cloned()
}

/// A text shade, named for a sentence: `white`, `near-black` or
/// `shade <stop>`. Upstream `textName`.
pub fn text_name(r: &StopRef) -> String {
    let family = r.family.as_deref();
    if family == Some("white") || (family == Some("neutral") && r.stop >= 95.0) {
        "white".to_string()
    } else if family == Some("black") || (family == Some("neutral") && r.stop <= 10.0) {
        "near-black".to_string()
    } else {
        format!("shade {}", js_number_to_string(r.stop))
    }
}

/// What [`PaletteTools::on_for`] settles: the text on a fill, the ratio it
/// reads at, the other choice and its ratio, and the preferred one. Upstream
/// `onFor`'s result.
#[derive(Clone, Debug, PartialEq)]
pub struct OnFor {
    /// The text taken.
    pub on: StopRef,
    /// The ratio it reads at.
    pub r: f64,
    /// The other choice.
    pub other: StopRef,
    /// The ratio the other choice reads at. Upstream `otherR`.
    pub other_r: f64,
    /// The preferred choice.
    pub first: StopRef,
    /// Whether `on` is the preferred choice itself (upstream compares the
    /// objects, `on === first`).
    pub is_first: bool,
}

/// Look-ups shared by the request planner and the mapping, on one
/// candidate: the nearest stop, a stop's color, the surfaces text and fills
/// sit on, the text that reads on a fill, and the theme's exact colors.
/// Upstream `PaletteTools`, the object `paletteTools` returns.
#[derive(Clone, Copy)]
pub struct PaletteTools<'a> {
    /// The palette.
    pub candidate: &'a TonalPaletteCandidate,
    /// What the mapping reads around it.
    pub context: &'a PaletteMappingContext<'a>,
    /// The palette's stops.
    pub stops: &'a [f64],
    /// Whether the palette has a `neutral` family.
    pub neutral: bool,
}

/// The look-ups of one candidate in one context. Upstream `paletteTools`.
pub fn palette_tools<'a>(
    candidate: &'a TonalPaletteCandidate,
    context: &'a PaletteMappingContext<'a>,
) -> PaletteTools<'a> {
    PaletteTools {
        candidate,
        context,
        stops: &candidate.stops,
        neutral: candidate.family("neutral").is_some(),
    }
}

impl<'a> PaletteTools<'a> {
    /// A value's RGB through the context's reader. Upstream `parseColor`.
    pub fn parse_color(&self, value: &str) -> Option<ColorTriple> {
        self.context.reader.rgb(value)
    }

    /// A value's opacity through the context's reader. Upstream `alphaOf`.
    pub fn alpha_of(&self, value: &str) -> f64 {
        self.context.reader.alpha(value)
    }

    /// The contrast ratio of two colors the reader reads, each rounded to a
    /// hex; an error for a color it cannot read. Upstream `contrast`.
    pub fn contrast(&self, a: &str, b: &str) -> Result<f64, PaletteError> {
        match (self.parse_color(a), self.parse_color(b)) {
            (Some(x), Some(y)) => Ok(ratio(&x, &y)),
            _ => Err(PaletteError(format!(
                "No contrast between \"{a}\" and \"{b}\"."
            ))),
        }
    }

    /// The stop of the layout nearest `n`. Upstream `at`.
    pub fn at(&self, n: f64) -> f64 {
        // The generator refuses an empty layout, so a candidate has a stop.
        nearest_stop(self.stops, n).unwrap_or(f64::NAN)
    }

    /// The stop nearest `n` off the white and black ends. Upstream `atInside`.
    pub fn at_inside(&self, n: f64) -> Result<f64, PaletteError> {
        let inside: Vec<f64> = self
            .stops
            .iter()
            .copied()
            .filter(|&s| s > 0.0 && s < 100.0)
            .collect();
        nearest_stop(&inside, n).ok_or_else(|| {
            PaletteError::new("The palette has no stop between its black and white ends.")
        })
    }

    /// A family's color at a stop of a mode, if the palette has it.
    fn color_at(&self, id: &str, mode: PaletteMode, stop: f64) -> Option<String> {
        self.candidate
            .ramp(id, mode)
            .and_then(|ramp| ramp.get(stop))
            .map(str::to_string)
    }

    /// A family's stop in a mode, with `alpha` appended to its color when
    /// given. The color is empty when the palette lacks the family or stop
    /// (upstream: `undefined`). Upstream `ref`.
    pub fn stop_ref(&self, id: &str, mode: PaletteMode, stop: f64, alpha: Option<&str>) -> StopRef {
        let color = self.color_at(id, mode, stop).unwrap_or_default();
        match alpha.filter(|a| !a.is_empty()) {
            Some(alpha) => StopRef {
                stop,
                color: format!("{color}{alpha}"),
                alpha: Some(alpha.to_string()),
                ..StopRef::default()
            },
            None => StopRef {
                stop,
                color,
                ..StopRef::default()
            },
        }
    }

    /// The value a token is set to exactly in a mode, as written. Upstream
    /// `settingOf`.
    pub fn setting_of(&self, token: &str, mode: PaletteMode) -> Option<String> {
        self.context.setting_of.and_then(|f| f(token, mode))
    }

    /// The fixed color a written value gives in a mode: the context's
    /// reading, else the trimmed value when the reader reads it. Upstream
    /// `colorIn`.
    pub fn color_in(&self, value: Option<&str>, mode: PaletteMode) -> Option<String> {
        let value = value?;
        if let Some(color_in) = self.context.color_in {
            return color_in(value, mode);
        }
        let v = js_trim(value);
        self.parse_color(v).map(|_| v.to_string())
    }

    /// The exact opaque color a token is set to in a mode, as a hex. Upstream
    /// `exactOf`.
    pub fn exact_of(&self, token: &str, mode: PaletteMode) -> Option<String> {
        let v = self.color_in(self.setting_of(token, mode).as_deref(), mode)?;
        let rgb = self.parse_color(&v)?;
        (self.alpha_of(&v) >= 1.0).then(|| hex(&rgb))
    }

    /// The exact color a token is set to in a mode, see-through or not, as
    /// written; `None` when there is none or it is not a fixed color.
    /// Upstream `rawOf`.
    pub fn raw_of(&self, token: &str, mode: PaletteMode) -> Option<String> {
        self.color_in(self.setting_of(token, mode).as_deref(), mode)
    }

    /// Which column of a `[light, dark]` mapping a mode takes: the light one
    /// (0) on a light page and the dark one (1) on a dark page, by the base
    /// theme's page, the one its surfaces were made for. Upstream `side`.
    pub fn side(&self, mode: PaletteMode) -> usize {
        let page = self
            .context
            .base
            .and_then(|base| base.mode(mode).get("--color-background-body"));
        let rgb = page.and_then(|p| self.parse_color(p));
        match (page, rgb) {
            (Some(page), Some(rgb)) if self.alpha_of(page) >= 1.0 => {
                usize::from(ratio(&rgb, &[255.0, 255.0, 255.0]) > ratio(&rgb, &[0.0, 0.0, 0.0]))
            }
            _ => mode_index(mode),
        }
    }

    /// What text and fills sit on in a mode, as `(name, hex)`: the theme's
    /// exact surface colors, else the neutral color's surfaces, else the base
    /// theme's; the family's own surface stop when nothing else is known.
    /// Upstream `surfaces`.
    pub fn surfaces(&self, mode: PaletteMode, id: &str) -> Vec<(String, String)> {
        let mut out = Vec::new();
        for n in [
            "--color-background-surface",
            "--color-background-body",
            "--color-background-card",
        ] {
            let own = if self.neutral {
                neutral_stops(n)
                    .and_then(|pair| self.color_at("neutral", mode, self.at(pair[self.side(mode)])))
            } else {
                None
            };
            let v = self
                .exact_of(n, mode)
                .or(own)
                .or_else(|| self.context.base.and_then(|b| b.mode(mode).get(n).cloned()));
            if let Some(v) = v.filter(|v| !v.is_empty())
                && let Some(rgb) = self.parse_color(&v)
                && self.alpha_of(&v) >= 1.0
            {
                out.push((n.to_string(), hex(&rgb)));
            }
        }
        if out.is_empty() {
            let pair = neutral_stops("--color-background-surface").unwrap_or([100.0, 15.0]);
            let s = self.at(pair[self.side(mode)]);
            out.push((
                format!("{id} shade {}", js_number_to_string(s)),
                self.stop_ref(id, mode, s, None).color,
            ));
        }
        out
    }

    /// The text shades for a fill, preferred first: the neutral family's
    /// (near-black first on warning), else black and white. Upstream
    /// `onChoices`.
    pub fn on_choices(&self, id: &str, mode: PaletteMode) -> [StopRef; 2] {
        let stops = if id == "warning" {
            [5.0, 100.0]
        } else {
            on_fill(mode)
        };
        stops.map(|s| {
            if self.neutral {
                StopRef {
                    family: Some("neutral".to_string()),
                    ..self.stop_ref("neutral", mode, self.at(s), None)
                }
            } else if s >= 50.0 {
                StopRef {
                    stop: 100.0,
                    color: PALETTE_WHITE.to_string(),
                    family: Some("white".to_string()),
                    ..StopRef::default()
                }
            } else {
                StopRef {
                    stop: 0.0,
                    color: PALETTE_BLACK.to_string(),
                    family: Some("black".to_string()),
                    ..StopRef::default()
                }
            }
        })
    }

    /// The text on a fill: the preferred shade when it reads at 4.5:1, else
    /// the one that reads better; when neither reads, the end of the gray
    /// ramp on the better side if it reads better still. Upstream `onFor`.
    pub fn on_for(&self, id: &str, mode: PaletteMode, fill: &str) -> Result<OnFor, PaletteError> {
        let [first, second] = self.on_choices(id, mode);
        let a_r = self.contrast(&first.color, fill)?;
        let b_r = self.contrast(&second.color, fill)?;
        let take_first = a_r >= TEXT || a_r >= b_r;
        let (mut on, mut r, other, other_r) = if take_first {
            (first.clone(), a_r, second, b_r)
        } else {
            (second, b_r, first.clone(), a_r)
        };
        let mut is_first = take_first;
        // Neither reads: the end of the gray ramp on the better side (black
        // past near-black, white past near-white) when it reads better, so
        // the text picked is never one a darker or lighter gray would beat.
        if r < TEXT {
            let dark =
                self.contrast(&on.color, PALETTE_WHITE)? > self.contrast(fill, PALETTE_WHITE)?;
            let end = if self.neutral {
                StopRef {
                    family: Some("neutral".to_string()),
                    ..self.stop_ref(
                        "neutral",
                        mode,
                        self.at(if dark { 0.0 } else { 100.0 }),
                        None,
                    )
                }
            } else if dark {
                StopRef {
                    stop: 0.0,
                    color: PALETTE_BLACK.to_string(),
                    family: Some("black".to_string()),
                    ..StopRef::default()
                }
            } else {
                StopRef {
                    stop: 100.0,
                    color: PALETTE_WHITE.to_string(),
                    family: Some("white".to_string()),
                    ..StopRef::default()
                }
            };
            let end_r = self.contrast(&end.color, fill)?;
            if end_r > r {
                on = end;
                r = end_r;
                is_first = false;
            }
        }
        Ok(OnFor {
            on,
            r,
            other,
            other_r,
            first,
            is_first,
        })
    }

    /// The lowest contrast of a color on the surfaces. Upstream `page`.
    pub fn page(&self, color: &str, mode: PaletteMode, id: &str) -> Result<f64, PaletteError> {
        let mut lowest = f64::INFINITY;
        for (_, bg) in self.surfaces(mode, id) {
            lowest = js_min(lowest, self.contrast(color, &bg)?);
        }
        Ok(lowest)
    }

    /// A color as the eye sees it on the first surface in a mode, as a hex.
    /// Upstream `solid`.
    pub fn solid(&self, color: &str, mode: PaletteMode, id: &str) -> Result<String, PaletteError> {
        let rgb = self
            .parse_color(color)
            .ok_or_else(|| PaletteError(format!("No color in \"{color}\".")))?;
        let a = self.alpha_of(color);
        if a >= 1.0 {
            return Ok(hex(&rgb));
        }
        let surface = self.surfaces(mode, id).swap_remove(0).1;
        let under = self
            .parse_color(&surface)
            .ok_or_else(|| PaletteError(format!("No color in \"{surface}\".")))?;
        Ok(hex(&composite_over(rgb, a, under)))
    }

    /// A translucent stop drawn over each surface, named `<name> over
    /// <surface>`; an opaque stop as it is. Upstream `over`.
    pub fn over(
        &self,
        name: &str,
        r: &StopRef,
        under: &[(String, String)],
    ) -> Result<Vec<(String, String)>, PaletteError> {
        let Some(alpha) = r.alpha.as_deref().filter(|a| !a.is_empty()) else {
            return Ok(vec![(name.to_string(), r.color.clone())]);
        };
        let solid = r.color.get(..7).unwrap_or(&r.color);
        let top = self
            .parse_color(solid)
            .ok_or_else(|| PaletteError(format!("No color in \"{solid}\".")))?;
        let units: Vec<u16> = alpha.encode_utf16().collect();
        let a = js_parse_int_hex(&units) / 255.0;
        under
            .iter()
            .map(|(n, bg)| {
                let bg_rgb = self
                    .parse_color(bg)
                    .ok_or_else(|| PaletteError(format!("No color in \"{bg}\".")))?;
                Ok((
                    format!("{name} over {n}"),
                    hex(&composite_over(top, a, bg_rgb)),
                ))
            })
            .collect()
    }
}

/// Whether the exact seed color works as the dark fill: text on it reads at
/// 4.5:1 and it stands out from the dark surfaces at 3:1; why not otherwise.
/// Upstream `darkExact`'s result.
#[derive(Clone, Debug, PartialEq)]
pub struct DarkExact {
    /// Whether it works.
    pub ok: bool,
    /// The ratio its text reads at, or the one it stands out at.
    pub ratio: f64,
    /// Why not, when it does not.
    pub why: String,
}

/// Whether the exact seed color works as the dark fill. Upstream `darkExact`.
pub fn dark_exact(t: &PaletteTools<'_>, id: &str, seed: &str) -> Result<DarkExact, PaletteError> {
    let on = t.on_for(id, PaletteMode::Dark, seed)?;
    if on.r < TEXT {
        let r = js_number_to_string(round(on.r));
        return Ok(DarkExact {
            ok: false,
            ratio: round(on.r),
            why: format!(
                "your exact color reads at only {r}:1 with any text shade, and text needs {}:1",
                js_number_to_string(TEXT)
            ),
        });
    }
    let p = t.page(seed, PaletteMode::Dark, id)?;
    if p < NON_TEXT {
        let r = js_number_to_string(round(p));
        return Ok(DarkExact {
            ok: false,
            ratio: round(p),
            why: format!(
                "your exact color stands out from the dark page at only {r}:1, and a fill needs {}:1",
                js_number_to_string(NON_TEXT)
            ),
        });
    }
    Ok(DarkExact {
        ok: true,
        ratio: round(on.r),
        why: String::new(),
    })
}

/// The family a color setting makes, and the stop its seed is anchored at:
/// a gray seed is generated as a neutral, and the seed's exact color is
/// anchored at its nearest light stop (both modes for the neutral). White or
/// black as neutral holds no anchor: they already end every ramp. Upstream
/// `dialFamily`.
pub fn dial_family(
    dial: &PaletteDial,
    config: &PaletteConfig,
    modes: &[PaletteMode],
) -> Result<(PaletteRequestFamily, f64), PaletteError> {
    let neutral = dial.id == "neutral";
    let kind = (neutral || is_gray(&dial.seed)?).then_some(FamilyKind::Neutral);
    let stop = seed_stop(&dial.seed, config, kind.unwrap_or(FamilyKind::Chromatic))?;
    let ends =
        neutral && (same_color(&dial.seed, PALETTE_WHITE) || same_color(&dial.seed, PALETTE_BLACK));
    let anchors: Vec<TonalPaletteAnchor> = if ends {
        Vec::new()
    } else {
        modes
            .iter()
            .copied()
            .filter(|&m| neutral || m == PaletteMode::Light)
            .map(|mode| TonalPaletteAnchor {
                mode,
                stop,
                color: dial.seed.clone(),
                policy: AnchorPolicy::Exact,
                max_delta_e: None,
            })
            .collect()
    };
    let mut name = dial.id.clone();
    if let Some(first) = name.get_mut(..1) {
        first.make_ascii_uppercase();
    }
    Ok((
        TonalPaletteFamilyInput {
            id: dial.id.clone(),
            name: Some(name),
            seed: dial.seed.clone(),
            kind,
            anchors: (!anchors.is_empty()).then_some(anchors),
        },
        stop,
    ))
}

/// Anchors each accent and status seed in dark too where it reads there, as
/// the official themes anchor a brand color in both modes. Settles each
/// fill's `dark`, `why` and `ratio`; returns the families with the dark
/// anchors the generator can hold, and calls `replaced` with each anchor of
/// the author's that one replaces. Upstream `anchorDarkFills`.
pub fn anchor_dark_fills(
    config: &PaletteConfig,
    dials: &[PaletteDial],
    fills: &mut PaletteFills,
    context: &PaletteMappingContext<'_>,
    mut replaced: Option<ReplacedAnchor<'_>>,
) -> Result<Vec<ConfigFamily>, PaletteError> {
    let candidate = generate_tonal_palette(config)?;
    let t = palette_tools(&candidate, context);
    let mut add: Vec<String> = Vec::new();
    for (id, fill) in fills.iter_mut() {
        let seed = dials
            .iter()
            .find(|f| f.id == *id)
            .map(|f| f.seed.as_str())
            .unwrap_or_default();
        let check = dark_exact(&t, id, seed)?;
        fill.why = check.why;
        fill.ratio = check.ratio;
        if check.ok {
            add.push(id.clone());
        }
    }
    let mut next = config.clone();
    for id in add {
        let Some(stop) = fills
            .iter()
            .find(|(f, _)| *f == id)
            .map(|(_, fill)| fill.stop)
        else {
            continue;
        };
        let mut tried = next.clone();
        for family in tried.families.iter_mut().filter(|f| f.id == id) {
            let anchor = TonalPaletteAnchor {
                mode: PaletteMode::Dark,
                stop,
                color: family.seed.clone(),
                policy: AnchorPolicy::Exact,
                max_delta_e: None,
            };
            let own = family.anchors.take().unwrap_or_default();
            let count = own.len();
            let mut anchors: Vec<TonalPaletteAnchor> = own
                .into_iter()
                .filter(|a| !(a.mode == PaletteMode::Dark && a.stop == anchor.stop))
                .collect();
            if anchors.len() < count
                && let Some(replaced) = replaced.as_mut()
            {
                replaced(&id, anchor.stop);
            }
            anchors.push(anchor);
            anchors.sort_by(|a, b| {
                if a.mode == b.mode {
                    compare(a.stop - b.stop)
                } else if a.mode == PaletteMode::Light {
                    std::cmp::Ordering::Less
                } else {
                    std::cmp::Ordering::Greater
                }
            });
            family.anchors = Some(anchors);
        }
        let fill = fills
            .iter_mut()
            .find(|(f, _)| *f == id)
            .map(|(_, fill)| fill);
        match generate_tonal_palette(&tried) {
            Ok(_) => {
                next = tried;
                if let Some(fill) = fill {
                    fill.dark = true;
                }
            }
            Err(_) => {
                if let Some(fill) = fill {
                    fill.why = "the dark shades cannot hold your exact color and stay in order from dark to light".to_string();
                }
            }
        }
    }
    Ok(next.families)
}

/// An accent or status fill at its seed's stop, before dark mode is settled.
/// Upstream `fillAt`.
pub fn fill_at(stop: f64) -> PaletteFill {
    PaletteFill {
        stop,
        dark: false,
        why: String::new(),
        ratio: 0.0,
    }
}

/// What [`plan_palette_request`] reads besides the settings. Upstream's
/// `options` argument.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PalettePlanOptions {
    /// The recipe to name in the request.
    pub recipe: Option<PaletteRecipe>,
    /// The vibrancy to name in the request.
    pub vibrancy: Option<f64>,
}

/// What [`plan_palette_request`] settles for a set of color settings.
/// Upstream `PalettePlan`.
#[derive(Clone, Debug, PartialEq)]
pub struct PalettePlan {
    /// The generation request (always `light-and-dark`).
    pub config: PaletteConfig,
    /// Each accent and status setting's fill.
    pub fills: PaletteFills,
}

/// The generation request for color settings with no palette config of the
/// author's: one family per setting in setting order, each seed anchored
/// exactly, in dark too where it reads there. `None` without a setting.
/// Upstream `planPaletteRequest`.
pub fn plan_palette_request(
    dials: &[PaletteDial],
    options: PalettePlanOptions,
    context: &PaletteMappingContext<'_>,
) -> Result<Option<PalettePlan>, PaletteError> {
    if dials.is_empty() {
        return Ok(None);
    }
    let modes = PaletteMode::BOTH;
    let mut config = PaletteConfig {
        recipe: options.recipe,
        vibrancy: options.vibrancy,
        mode_strategy: Some(ModeStrategy::LightAndDark),
        ..PaletteConfig::default()
    };
    let mut fills: PaletteFills = Vec::new();
    for dial in dials {
        let (family, stop) = dial_family(dial, &config, &modes)?;
        if dial.id != "neutral" {
            match fills.iter_mut().find(|(id, _)| *id == dial.id) {
                Some(entry) => entry.1 = fill_at(stop),
                None => fills.push((dial.id.clone(), fill_at(stop))),
            }
        }
        config.families.push(family);
    }
    if !fills.is_empty() {
        config.families = anchor_dark_fills(&config, dials, &mut fills, context, None)?;
    }
    Ok(Some(PalettePlan { config, fills }))
}

/// The category hue words of Core's tokens.
const LOOK_HUES: [&str; 10] = [
    "red", "orange", "yellow", "green", "teal", "cyan", "blue", "purple", "pink", "gray",
];

/// The syntax roles of Core's code tokens.
const SYNTAX_ROLES: [&str; 14] = [
    "keyword",
    "string",
    "comment",
    "number",
    "function",
    "type",
    "variable",
    "operator",
    "constant",
    "tag",
    "attribute",
    "property",
    "punctuation",
    "background",
];

/// Whether a token is one the look derives: a category color (Badge, Token,
/// Card tint), a chart color, a code color or the overlay scrim. Upstream
/// `isLookToken`.
pub fn is_look_token(token: &str) -> bool {
    let Some(rest) = token.strip_prefix("--color-") else {
        return false;
    };
    if rest == "overlay" {
        return true;
    }
    if let Some(data) = rest.strip_prefix("data-") {
        return !data.is_empty()
            && data
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
    }
    if let Some(role) = rest.strip_prefix("syntax-") {
        return SYNTAX_ROLES.contains(&role);
    }
    ["background-", "text-", "icon-", "border-"]
        .iter()
        .any(|prefix| {
            rest.strip_prefix(prefix)
                .is_some_and(|hue| LOOK_HUES.contains(&hue))
        })
}

/// The text roles a rule block can set for what it draws. Upstream
/// `TEXT_PROPS`.
pub const TEXT_PROPS: [&str; 4] = [
    "color",
    "--color-text-primary",
    "--color-text-secondary",
    "--color-text-accent",
];

/// Two decimals, a half rounding up. Upstream `round2`.
pub fn round2(n: f64) -> f64 {
    js_round(n * 100.0) / 100.0
}

/// A part-style value's light and dark halves: a `[light, dark]` pair, a
/// `light-dark()` value split at its top-level commas, or one value for
/// both; `None` for anything else. Upstream `sides`.
fn sides(v: Option<&StyleValue>) -> Option<Vec<String>> {
    match v? {
        StyleValue::List(items) => Some(items.clone()),
        StyleValue::Text(text) => {
            let trimmed = js_trim(text);
            let inner = trimmed
                .strip_prefix("light-dark(")
                .and_then(|rest| rest.strip_suffix(')'))
                .filter(|inner| !inner.is_empty());
            Some(match inner {
                Some(inner) => split_top_comma(inner),
                None => vec![text.clone(), text.clone()],
            })
        }
        _ => None,
    }
}

/// Splits a CSS value at its top-level commas (outside brackets). Upstream
/// `splitTopComma`.
pub fn split_top_comma(value: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth: i64 = 0;
    let mut current = String::new();
    for c in value.chars() {
        if c == '(' {
            depth += 1;
        }
        if c == ')' {
            depth -= 1;
        }
        if depth == 0 && c == ',' {
            out.push(std::mem::take(&mut current));
        } else {
            current.push(c);
        }
    }
    out.push(current);
    out
}

/// One block of a part's styles. Upstream `partStyle`.
fn part_style<'s>(
    components: Option<&'s [(String, StyleValue)]>,
    part: &str,
    block: &str,
) -> Option<&'s StyleValue> {
    let blocks = components?
        .iter()
        .rev()
        .find(|(k, _)| k == part)
        .map(|(_, v)| v)?;
    blocks
        .get(block)
        .filter(|style| matches!(style, StyleValue::Block(_)))
}

/// The tokens a color setting maps: its fill, the text on it, its text and
/// icons, its tint, and (by kind) the text on the tint, its border and the
/// focus ring.
struct DialNames {
    fill: String,
    on: String,
    text: String,
    icon: String,
    muted: String,
    on_muted: Option<String>,
    border: Option<String>,
    focus: Option<String>,
}

impl DialNames {
    /// A status setting's tokens by its Core word, or the accent's.
    fn new(word: Option<&str>) -> Self {
        match word {
            Some(word) => Self {
                fill: format!("--color-{word}"),
                on: format!("--color-on-{word}"),
                text: format!("--color-text-{word}"),
                icon: format!("--color-icon-{word}"),
                muted: format!("--color-{word}-muted"),
                on_muted: Some(format!("--color-on-{word}-muted")),
                border: Some(format!("--color-border-{word}")),
                focus: None,
            },
            None => Self {
                fill: "--color-accent".to_string(),
                on: "--color-on-accent".to_string(),
                text: "--color-text-accent".to_string(),
                icon: "--color-icon-accent".to_string(),
                muted: "--color-accent-muted".to_string(),
                on_muted: None,
                border: None,
                focus: Some("--focus-outline-color".to_string()),
            },
        }
    }
}

/// The `(stop, r)` of the first score that reaches `needs`, else the highest
/// (the first of equals): upstream's `scored.find(x => x.r >= needs) ??
/// scored.reduce((x, y) => (y.r > x.r ? y : x))`.
pub(crate) fn first_passing(scored: &[(f64, f64)], needs: f64) -> (f64, f64) {
    if let Some(found) = scored.iter().find(|(_, r)| *r >= needs) {
        return *found;
    }
    scored.iter().skip(1).fold(
        scored[0],
        |best, &next| if next.1 > best.1 { next } else { best },
    )
}

/// The `||` tie-break upstream's sorts use: `(Math.sign(b - a) === usual ? 1 :
/// -1)`, as an ordering (equal for one stop compared with itself).
fn toward(a: f64, b: f64, usual: f64) -> std::cmp::Ordering {
    if a == b {
        std::cmp::Ordering::Equal
    } else if (b - a).signum() == usual {
        std::cmp::Ordering::Greater
    } else {
        std::cmp::Ordering::Less
    }
}

/// The stops sorted by distance from `start`, ties broken by `toward`.
fn by_distance(stops: &[f64], start: f64, usual: f64) -> Vec<f64> {
    let mut order = stops.to_vec();
    order.sort_by(|&a, &b| {
        compare((a - start).abs() - (b - start).abs()).then_with(|| toward(a, b, usual))
    });
    order
}

/// The inner stops sorted by distance from `start`, the higher first on a tie
/// (`|| b - a`).
fn inner_by_distance(stops: &[f64], start: f64) -> Vec<f64> {
    let mut order: Vec<f64> = stops
        .iter()
        .copied()
        .filter(|&s| s > 0.0 && s < 100.0)
        .collect();
    order.sort_by(|&a, &b| {
        compare((a - start).abs() - (b - start).abs()).then_with(|| compare(b - a))
    });
    order
}

/// The mapping's working state: the look-ups, the tokens so far and the
/// checks behind them.
struct Mapper<'a> {
    t: PaletteTools<'a>,
    tokens: MappedTokens,
    checks: Vec<ContrastCheck>,
}

impl Mapper<'_> {
    fn has(&self, token: &str) -> bool {
        (self.t.context.has)(token)
    }

    fn set_exactly(&self, token: &str) -> bool {
        self.t.context.set_exactly.is_some_and(|f| f(token))
    }

    /// Whether a token's checks are reported: it exists and the theme does
    /// not set it exactly. Upstream `reports`.
    fn reports(&self, token: &str) -> bool {
        self.has(token) && !self.set_exactly(token)
    }

    /// Points a token at its stops, when it exists. Upstream `put`.
    fn put(&mut self, token: &str, id: &str, [light, dark]: [StopRef; 2]) {
        if !self.has(token) {
            return;
        }
        self.tokens.insert(
            token,
            MappedToken {
                family: id.to_string(),
                light,
                dark,
            },
        );
    }

    /// The lowest contrast of a color on every ground.
    fn lowest(&self, color: &str, on: &[(String, String)]) -> Result<f64, PaletteError> {
        let mut lowest = f64::INFINITY;
        for (_, bg) in on {
            lowest = js_min(lowest, self.t.contrast(color, bg)?);
        }
        Ok(lowest)
    }

    /// The stop nearest `start` that reads at `needs` on every color in `on`,
    /// searched both ways (the usual way first on a tie: darker in light,
    /// lighter in dark); the most contrasting stop of all when none does.
    /// Upstream `pick` (mapPalette).
    fn pick(
        &mut self,
        token: &str,
        id: &str,
        mode: PaletteMode,
        start: f64,
        on: &[(String, String)],
        needs: f64,
    ) -> Result<StopRef, PaletteError> {
        let order = by_distance(self.t.stops, start, toward_contrast(mode));
        let mut scored = Vec::with_capacity(order.len());
        for stop in order {
            let color = self.t.stop_ref(id, mode, stop, None).color;
            scored.push((stop, self.lowest(&color, on)?));
        }
        let (stop, r) = first_passing(&scored, needs);
        if self.reports(token) {
            self.checks.push(ContrastCheck {
                token: token.to_string(),
                mode,
                stop,
                preferred: start,
                against: on
                    .iter()
                    .map(|(n, _)| n.as_str())
                    .collect::<Vec<_>>()
                    .join(", "),
                ratio: round(r),
                needs,
                pass: r >= needs,
                note: None,
                dials: None,
                fix: None,
            });
        }
        Ok(self.t.stop_ref(id, mode, stop, None))
    }

    /// A base theme value of a mode.
    fn base_value(&self, mode: PaletteMode, token: &str) -> Option<String> {
        self.t
            .context
            .base
            .and_then(|b| b.mode(mode).get(token).cloned())
    }

    /// The value a ground or token shows in a mode: an exact setting, else
    /// the mapping's, else the base theme's; `None` unless the reader reads
    /// it. Upstream `valueOf` (the lift).
    fn value_of(&self, mode: PaletteMode, token: &str) -> Option<String> {
        let v = self
            .t
            .raw_of(token, mode)
            .or_else(|| self.tokens.color(token, mode).map(str::to_string))
            .or_else(|| self.base_value(mode, token))?;
        (!v.is_empty() && self.t.parse_color(&v).is_some()).then_some(v)
    }

    /// The secondary text a mode starts from, as a hex. Upstream `startText`.
    fn start_text(&self, mode: PaletteMode) -> Option<String> {
        let v = self
            .tokens
            .color("--color-text-secondary", mode)
            .map(str::to_string)
            .or_else(|| self.base_value(mode, "--color-text-secondary"))?;
        if v.is_empty() {
            return None;
        }
        self.t.parse_color(&v).map(|rgb| hex(&rgb))
    }

    /// A color drawn over each of `under`, as the eye sees it, named
    /// `<name> over <under>`. Upstream `drawn`.
    fn drawn(
        &self,
        name: &str,
        color: &str,
        under: &[(String, String)],
    ) -> Result<Vec<(String, String)>, PaletteError> {
        let top = self
            .t
            .parse_color(color)
            .ok_or_else(|| PaletteError(format!("No color in \"{color}\".")))?;
        let a = self.t.alpha_of(color);
        under
            .iter()
            .map(|(n, c)| {
                let bg = self
                    .t
                    .parse_color(c)
                    .ok_or_else(|| PaletteError(format!("No color in \"{c}\".")))?;
                Ok((format!("{name} over {n}"), hex(&composite_over(top, a, bg))))
            })
            .collect()
    }
}

/// The color a reader reads, or an error naming the value.
fn read(t: &PaletteTools<'_>, value: &str) -> Result<ColorTriple, PaletteError> {
    t.parse_color(value)
        .ok_or_else(|| PaletteError(format!("No color in \"{value}\".")))
}

/// The tint the theme really shows for a setting in a mode: an exact setting
/// of it over each surface, else its stop over each surface. Upstream
/// `mutedSeen`.
fn muted_seen(
    t: &PaletteTools<'_>,
    names: &DialNames,
    id: &str,
    muted: &[StopRef; 2],
    mode: PaletteMode,
) -> Result<Vec<(String, String)>, PaletteError> {
    match t.raw_of(&names.muted, mode) {
        Some(own) => {
            let top = read(t, &own)?;
            let a = t.alpha_of(&own);
            t.surfaces(mode, id)
                .into_iter()
                .map(|(n, bg)| {
                    Ok((
                        format!("your exact {} over {n}", names.muted),
                        hex(&composite_over(top, a, read(t, &bg)?)),
                    ))
                })
                .collect()
        }
        None => t.over(
            &names.muted,
            &muted[mode_index(mode)],
            &t.surfaces(mode, id),
        ),
    }
}

/// The token mapping of a generated palette: each color token's stop in
/// light and dark, as the official Neutral theme maps its families, with the
/// contrast checks behind each choice, then the colors the editor's rules
/// grow (`context.editor`). Upstream `mapPalette`.
pub fn map_palette(
    candidate: &TonalPaletteCandidate,
    dials: &[PaletteDial],
    fills: &[(String, PaletteFill)],
    context: &PaletteMappingContext<'_>,
) -> Result<PaletteMapping, PaletteError> {
    let t = palette_tools(candidate, context);
    // Both halves, for a one-mode theme too.
    let modes = PaletteMode::BOTH;
    let mut mp = Mapper {
        t,
        tokens: MappedTokens::new(),
        checks: Vec::new(),
    };
    let mut exact: Vec<(String, ModeValues)> = Vec::new();
    let mut unread: Vec<(String, String)> = Vec::new();
    let set_entry = |list: &mut Vec<(String, ModeValues)>, key: &str, value: ModeValues| match list
        .iter_mut()
        .find(|(k, _)| k == key)
    {
        Some(entry) => entry.1 = value,
        None => list.push((key.to_string(), value)),
    };

    let neutral = dials.iter().find(|f| f.id == "neutral");
    if neutral.is_some() {
        for (token, pair) in NEUTRAL_STOPS {
            if token == "--color-text-secondary" || token == "--color-icon-secondary" {
                continue;
            }
            let inside = NEUTRAL_INSIDE.contains(&token);
            let mut refs = Vec::with_capacity(2);
            for m in modes {
                let n = pair[t.side(m)];
                let stop = if inside { t.at_inside(n)? } else { t.at(n) };
                refs.push(t.stop_ref("neutral", m, stop, None));
            }
            let dark = refs.pop().unwrap_or_default();
            let light = refs.pop().unwrap_or_default();
            mp.put(token, "neutral", [light, dark]);
        }
        for (token, pair) in NEUTRAL_TINTS {
            let refs = modes.map(|m| {
                let (s, a) = pair[t.side(m)];
                t.stop_ref("neutral", m, t.at(s), Some(a))
            });
            mp.put(token, "neutral", refs);
        }
        for (token, needs) in [
            ("--color-text-secondary", TEXT),
            ("--color-icon-secondary", NON_TEXT),
        ] {
            let pair = neutral_stops(token).unwrap_or_default();
            let mut refs = Vec::with_capacity(2);
            for m in modes {
                let start = t.at(pair[t.side(m)]);
                refs.push(mp.pick(token, "neutral", m, start, &t.surfaces(m, "neutral"), needs)?);
            }
            let dark = refs.pop().unwrap_or_default();
            let light = refs.pop().unwrap_or_default();
            mp.put(token, "neutral", [light, dark]);
        }
        let surface = palette_token("neutral");
        let set = ModeValues {
            light: t.exact_of(&surface, PaletteMode::Light),
            dark: t.exact_of(&surface, PaletteMode::Dark),
        };
        if set.light.is_some() || set.dark.is_some() {
            set_entry(&mut exact, &surface, set);
        }
    }

    for f in dials {
        if f.id == "neutral" {
            continue;
        }
        let word = if f.id == "accent" {
            None
        } else {
            status_token(&f.id)
        };
        let names = DialNames::new(word);
        let plain = fills
            .iter()
            .find(|(id, _)| *id == f.id)
            .map(|(_, fill)| fill.clone())
            .ok_or_else(|| PaletteError(format!("No fill for {}.", f.id)))?;
        let s = plain.stop;
        let fill_word = f.id.as_str();
        let mut exact_fill: [Option<String>; 2] = [None, None];
        for m in modes {
            if let Some(v) = t.raw_of(&names.fill, m) {
                exact_fill[mode_index(m)] = Some(v);
            } else if let Some(setting) = t.setting_of(&names.fill, m) {
                let value = js_trim(&setting).to_string();
                match unread.iter_mut().find(|(k, _)| *k == names.fill) {
                    Some(entry) => entry.1 = value,
                    None => unread.push((names.fill.clone(), value)),
                }
            }
        }
        let has_exact_fill = exact_fill.iter().any(Option::is_some);
        if has_exact_fill {
            let shown = |m: PaletteMode| -> Option<String> {
                let v = exact_fill[mode_index(m)].clone()?;
                // A var() is named with the color it reads, so the note says
                // which color the text was picked for.
                let written = t.setting_of(&names.fill, m).unwrap_or_default();
                let written = js_trim(&written);
                let color = match t.parse_color(&v) {
                    Some(rgb) if t.alpha_of(&v) >= 1.0 => hex(&rgb),
                    _ => v.clone(),
                };
                Some(if written.to_ascii_lowercase().contains("var(") {
                    format!("{written} = {color}")
                } else {
                    v
                })
            };
            let value = ModeValues {
                light: shown(PaletteMode::Light),
                dark: shown(PaletteMode::Dark),
            };
            set_entry(&mut exact, &names.fill, value);
        }

        // The fill: the exact color in light, and in dark where it reads.
        let passes = |c: &str, m: PaletteMode| -> Result<bool, PaletteError> {
            Ok(t.page(c, m, &f.id)? >= NON_TEXT && t.on_for(&f.id, m, c)?.r >= TEXT)
        };
        let mut fill: [StopRef; 2] = Default::default();
        for m in modes {
            if m == PaletteMode::Light || plain.dark {
                fill[mode_index(m)] = StopRef {
                    exact: true,
                    ..t.stop_ref(&f.id, m, s, None)
                };
                continue;
            }
            // The dark fill when the exact color does not read there: shade
            // 70, else the shade nearest 70 that passes, else 70.
            let start = t.at(DARK_FILL);
            let mut found = start;
            for stop in inner_by_distance(t.stops, start) {
                if passes(&t.stop_ref(&f.id, m, stop, None).color, m)? {
                    found = stop;
                    break;
                }
            }
            fill[mode_index(m)] = t.stop_ref(&f.id, m, found, None);
            // The shade nearest your color that passes, offered as the
            // theme's own dark fill when it is closer to your color than the
            // one used.
            let mut near = None;
            for stop in inner_by_distance(t.stops, s) {
                if passes(&t.stop_ref(&f.id, m, stop, None).color, m)? {
                    near = Some(stop);
                    break;
                }
            }
            let closer = match (near, context.set_line) {
                (Some(near), Some(set_line)) if (near - s).abs() < (found - s).abs() => {
                    let c = t.stop_ref(&f.id, m, near, None).color;
                    let o = t.on_for(&f.id, m, &c)?;
                    format!(
                        "; for a fill closer to your color in dark: {} (shade {}, the nearest that passes, with {} text at {}:1)",
                        set_line(&names.fill, &c, Some(PaletteMode::Dark)),
                        shade(near),
                        text_name(&o.on),
                        js_number_to_string(round(o.r))
                    )
                }
                _ => String::new(),
            };
            if mp.reports(&names.fill) {
                let either = if found == start {
                    String::new()
                } else {
                    format!(
                        "; shade {} does not pass either, so it is the nearest shade that does",
                        shade(start)
                    )
                };
                mp.checks.push(ContrastCheck {
                    token: names.fill.clone(),
                    mode: m,
                    stop: found,
                    preferred: s,
                    against: "the dark page and the text on it".to_string(),
                    ratio: plain.ratio,
                    needs: TEXT,
                    pass: true,
                    note: Some(format!(
                        "in dark, {} is shade {}, not your exact color: {}{either}{closer}",
                        names.fill,
                        shade(found),
                        plain.why
                    )),
                    dials: None,
                    fix: None,
                });
            }
        }
        mp.put(&names.fill, &f.id, fill.clone());

        // Text on the fill reads on the fill the theme really uses: the exact
        // token setting when there is one. The exact color never moves; the
        // text changes instead.
        let mut on: [StopRef; 2] = Default::default();
        for m in modes {
            let i = mode_index(m);
            let used = exact_fill[i]
                .clone()
                .unwrap_or_else(|| fill[i].color.clone());
            // A see-through exact fill is measured as it shows on the surface.
            let seen = t.solid(&used, m, &f.id)?;
            let o = t.on_for(fill_word, m, &seen)?;
            let what = if exact_fill[i].is_some() {
                let shows = if t.alpha_of(&used) < 1.0 {
                    format!(", as it shows: {}", t.solid(&used, m, &f.id)?)
                } else {
                    String::new()
                };
                format!("your exact {} ({used}{shows})", names.fill)
            } else {
                format!("{} (shade {})", names.fill, shade(fill[i].stop))
            };
            let moved = !o.is_first;
            if mp.reports(&names.on) {
                let r = js_number_to_string(round(o.r));
                let text = js_number_to_string(TEXT);
                let note = if o.r < TEXT {
                    Some(format!(
                        "text on {what} reads at only {r}:1 in {m} with white or near-black text (needs {text}:1); the fill keeps your color: pick a lighter or darker {}, or set {} exactly",
                        f.id, names.on
                    ))
                } else if moved {
                    Some(format!(
                        "{} uses {} text in {m}, not {}, so it reads at {r}:1 on {what}, where {} text reads at only {}:1 (button and badge labels are normal-size text, which needs {text}:1); to keep {} text anyway, set {} exactly",
                        names.on,
                        text_name(&o.on),
                        text_name(&o.first),
                        text_name(&o.other),
                        js_number_to_string(round(o.other_r)),
                        text_name(&o.other),
                        names.on
                    ))
                } else {
                    None
                };
                mp.checks.push(ContrastCheck {
                    token: names.on.clone(),
                    mode: m,
                    stop: o.on.stop,
                    preferred: o.first.stop,
                    against: what,
                    ratio: round(o.r),
                    needs: TEXT,
                    pass: o.r >= TEXT,
                    note,
                    dials: None,
                    fix: None,
                });
            }
            on[i] = o.on;
        }
        mp.put(&names.on, &f.id, on);

        // A pale fill in light can disappear into the page: say so, and keep
        // the exact color, the setting's or the token's.
        if mp.has(&names.fill) {
            let light = PaletteMode::Light;
            let used = exact_fill[0]
                .clone()
                .unwrap_or_else(|| fill[0].color.clone());
            let p = t.page(&t.solid(&used, light, &f.id)?, light, &f.id)?;
            // The nearest darker color that stands out from the light page.
            let mut darker: Option<String> = None;
            let from = t.parse_color(&t.solid(&used, light, &f.id)?);
            if let Some(from) = from {
                for i in 1..=100 {
                    let c = hex(&from.map(|v| js_round(v * (1.0 - f64::from(i) / 100.0))));
                    if t.page(&c, light, &f.id)? >= NON_TEXT {
                        darker = Some(c);
                        break;
                    }
                }
            }
            if p < NON_TEXT {
                let r = js_number_to_string(round(p));
                let non_text = js_number_to_string(NON_TEXT);
                let note = if exact_fill[0].is_some() {
                    format!(
                        "in light, your exact {} ({used}) stands out from the page at only {r}:1, and a fill needs {non_text}:1, so its buttons and badges may not show: give it a darker color",
                        names.fill
                    )
                } else {
                    format!(
                        "in light, your exact {} stands out from the page at only {r}:1, and a fill needs {non_text}:1, so {} buttons and badges may not show: pick a darker {}",
                        f.id, names.fill, f.id
                    )
                };
                let fix = darker.map(|value| {
                    if exact_fill[0].is_some() {
                        ContrastFix {
                            thing: names.fill.clone(),
                            value,
                            mode: Some(light),
                        }
                    } else {
                        ContrastFix {
                            thing: f.id.clone(),
                            value,
                            mode: None,
                        }
                    }
                });
                mp.checks.push(ContrastCheck {
                    token: names.fill.clone(),
                    mode: light,
                    stop: s,
                    preferred: s,
                    against: "the light page".to_string(),
                    ratio: round(p),
                    needs: NON_TEXT,
                    pass: false,
                    note: Some(note),
                    dials: None,
                    fix,
                });
            }
        }

        // The tint: the official stop, never the fill's own shade, see-through
        // or not.
        let muted: [StopRef; 2] = modes.map(|m| {
            let i = t.side(m);
            let (spec, alpha) = if word.is_some() {
                (f64::from(STATUS_MUTED[i].stop), STATUS_MUTED[i].alpha)
            } else {
                (ACCENT_MUTED[i], None)
            };
            // Nearest the official stop first, both ways; on a tie (a layout
            // without that stop) lighter first in light, darker in dark.
            let usual = if i == 0 { 1.0 } else { -1.0 };
            let order = by_distance(t.stops, spec, usual);
            let fill_stop = fill[mode_index(m)].stop;
            let stop = order
                .iter()
                .copied()
                .find(|&x| x != fill_stop)
                .unwrap_or(order[0]);
            t.stop_ref(&f.id, m, stop, alpha)
        });
        mp.put(&names.muted, &f.id, muted.clone());

        // Accent text and icons are the color given where it reads on what
        // they sit on (the surfaces and the tint), in each mode its fill is
        // the exact color; only where it does not do they move, to the
        // nearest shade that reads, and the check says so.
        let mut text: [StopRef; 2] = Default::default();
        for m in modes {
            let i = mode_index(m);
            let mut grounds = t.surfaces(m, &f.id);
            grounds.extend(muted_seen(&t, &names, &f.id, &muted, m)?);
            // The fill the theme really shows in this mode: an exact token
            // setting of it, else the setting's own color at its shade. The
            // text starts from that color's nearest shade.
            let shown = match &exact_fill[i] {
                Some(v) => Some(t.solid(v, m, &f.id)?),
                None => None,
            };
            let start = match &shown {
                Some(shown) => {
                    let mut best = t.stops[0];
                    for &y in &t.stops[1..] {
                        let dy = perceptual_delta(&t.stop_ref(&f.id, m, y, None).color, shown)?;
                        let dx = perceptual_delta(&t.stop_ref(&f.id, m, best, None).color, shown)?;
                        if dy < dx {
                            best = y;
                        }
                    }
                    best
                }
                None => s,
            };
            let own = match &shown {
                Some(shown) => Some(StopRef {
                    color: shown.clone(),
                    ..t.stop_ref(&f.id, m, start, None)
                }),
                None if fill[i].exact => Some(t.stop_ref(&f.id, m, s, None)),
                None => None,
            };
            text[i] = match own {
                Some(own) => {
                    let r = mp.lowest(&own.color, &grounds)?;
                    if r >= TEXT && shown.is_none() {
                        if mp.reports(&names.text) {
                            mp.checks.push(ContrastCheck {
                                token: names.text.clone(),
                                mode: m,
                                stop: s,
                                preferred: s,
                                against: grounds
                                    .iter()
                                    .map(|(n, _)| n.as_str())
                                    .collect::<Vec<_>>()
                                    .join(", "),
                                ratio: round(r),
                                needs: TEXT,
                                pass: true,
                                note: None,
                                dials: None,
                                fix: None,
                            });
                        }
                        own
                    } else {
                        let before = mp.checks.len();
                        let moved = mp.pick(&names.text, &f.id, m, start, &grounds, TEXT)?;
                        if r < TEXT
                            && mp.checks.len() > before
                            && (moved.stop != start || shown.is_some())
                        {
                            let mut low = Vec::new();
                            for (n, bg) in &grounds {
                                let x = t.contrast(&own.color, bg)?;
                                if x < TEXT {
                                    low.push(format!("{}:1 on {n}", js_number_to_string(round(x))));
                                }
                            }
                            let who = if f.id == "accent" {
                                "links and accent text".to_string()
                            } else {
                                format!("{} text and icons", f.id)
                            };
                            let note = format!(
                                "{} {m}: your exact color ({}) reads at only {} (text needs {}:1), so {who} use shade {} ({}), the nearest that reads; the fill keeps your color, and an exact {} keeps any color you give it",
                                names.text,
                                own.color,
                                low.join(", "),
                                js_number_to_string(TEXT),
                                shade(moved.stop),
                                moved.color,
                                names.text
                            );
                            if let Some(last) = mp.checks.last_mut() {
                                last.note = Some(note);
                            }
                        }
                        moved
                    }
                }
                None => {
                    let start = t.at(TEXT_STOP[t.side(m)]);
                    mp.pick(&names.text, &f.id, m, start, &grounds, TEXT)?
                }
            };
        }
        mp.put(&names.text, &f.id, text.clone());
        mp.put(&names.icon, &f.id, text);
        if let Some(on_muted) = &names.on_muted {
            let mut refs: [StopRef; 2] = Default::default();
            for m in modes {
                let start = t.at(ON_MUTED[t.side(m)]);
                let seen = muted_seen(&t, &names, &f.id, &muted, m)?;
                refs[mode_index(m)] = mp.pick(on_muted, &f.id, m, start, &seen, TEXT)?;
            }
            mp.put(on_muted, &f.id, refs);
        }
        if let Some(border) = &names.border {
            let refs = modes.map(|m| t.stop_ref(&f.id, m, t.at(BORDER[t.side(m)]), None));
            mp.put(border, &f.id, refs);
        }
        // The focus ring follows the fill; with an exact fill it keeps
        // following that (Core's ring reads --color-accent).
        if let Some(focus) = names.focus.as_deref().filter(|_| !has_exact_fill) {
            let mut refs: [StopRef; 2] = Default::default();
            for m in modes {
                let i = mode_index(m);
                refs[i] = mp.pick(
                    focus,
                    &f.id,
                    m,
                    fill[i].stop,
                    &t.surfaces(m, &f.id),
                    NON_TEXT,
                )?;
            }
            mp.put(focus, &f.id, refs);
        }
    }

    // Secondary text on the parts the official Neutral theme lifts it on
    // (LIFT_PARTS), lifted the same way: for each background, the gray shade
    // nearest the secondary text's own that reads at 4.5:1 on it and on it
    // under a hover, over every surface. The grays are the neutral color's,
    // else a gray ramp grown from the base theme's own secondary text. A mode
    // where the theme sets secondary text exactly takes that value instead,
    // as written.
    let lift = map_lift(&mut mp, candidate, neutral.is_some())?;

    // The colors the theme editor grows beyond this mapping, from its own
    // rules (context.editor, which themePalette() never passes).
    let EditorLookResult {
        categories,
        statuses,
        code,
        cvd,
        kept_at_base,
    } = match context.editor {
        Some(editor) => {
            let set_exactly = |k: &str| context.set_exactly.is_some_and(|f| f(k));
            editor.grow_look(EditorLookInput {
                candidate,
                dials,
                context,
                tokens: &mut mp.tokens,
                modes: &modes,
                set_exactly: &set_exactly,
                t: &t,
            })?
        }
        None => EditorLookResult::default(),
    };
    Ok(PaletteMapping {
        tokens: mp.tokens,
        contrast: mp
            .checks
            .into_iter()
            .filter(|c| c.note.as_deref() != Some(""))
            .collect(),
        exact,
        unread,
        lift,
        cvd,
        code,
        kept: kept_at_base,
        statuses,
        categories,
    })
}

/// The lifted secondary text of [`map_palette`], per background.
fn map_lift(
    mp: &mut Mapper<'_>,
    candidate: &TonalPaletteCandidate,
    neutral: bool,
) -> Result<Option<Vec<(String, ModeStopRefs)>>, PaletteError> {
    let t = mp.t;
    let base_components = t.context.base.and_then(|b| b.components.as_deref());
    let mut grounds: Vec<&str> = Vec::new();
    for (_, _, ground) in LIFT_PARTS {
        if !grounds.contains(&ground) {
            grounds.push(ground);
        }
    }
    let mut found: Vec<(String, ModeStopRefs)> = Vec::new();
    for ground in grounds {
        let Some(info) = LIFT_GROUNDS
            .iter()
            .find(|(g, _)| *g == ground)
            .map(|(_, info)| *info)
        else {
            continue;
        };
        // The shade the base theme itself lifts this ground's text to in a
        // mode, when it gives one: the search starts there, so a change of
        // color moves it only when that shade no longer reads.
        let base_lift = |m: PaletteMode| -> Option<String> {
            for (part, block, g) in LIFT_PARTS {
                if g != ground {
                    continue;
                }
                let style = part_style(base_components, part, block);
                let pair = sides(style.and_then(|s| s.get("--color-text-secondary")));
                let c = pair.and_then(|p| p.get(mode_index(m)).map(|c| js_trim(c).to_string()));
                if let Some(rgb) = c.filter(|c| !c.is_empty()).and_then(|c| t.parse_color(&c)) {
                    return Some(hex(&rgb));
                }
            }
            None
        };
        let mut by_mode: [Option<StopRef>; 2] = [None, None];
        for m in PaletteMode::BOTH {
            let i = mode_index(m);
            let own = t.raw_of("--color-text-secondary", m);
            let literal = |own: &str| StopRef {
                stop: 0.0,
                color: own.to_string(),
                family: Some("literal".to_string()),
                ..StopRef::default()
            };
            let fill_color = mp.value_of(m, ground);
            let text_color = mp.start_text(m);
            let grays: Option<(PaletteRamp, bool)> = if neutral {
                Some((
                    candidate.ramp("neutral", m).cloned().unwrap_or_default(),
                    true,
                ))
            } else {
                text_color
                    .as_deref()
                    .and_then(|seed| grown_grays(seed, m))
                    .map(|ramp| (ramp, false))
            };
            let (Some(fill_color), Some((ramp, palette)), Some(text_color)) =
                (fill_color, grays, text_color)
            else {
                if let Some(own) = own.filter(|o| !o.is_empty()) {
                    by_mode[i] = Some(literal(&own));
                    continue;
                }
                break;
            };
            let seen = mp.drawn(info.what, &fill_color, &t.surfaces(m, "neutral"))?;
            let hover = mp.value_of(m, "--color-overlay-hover");
            let mut on = seen.clone();
            if let Some(hover) = hover.filter(|h| t.alpha_of(h) > 0.0) {
                for (n, c) in mp.drawn("a hover", &hover, &seen)? {
                    let renamed = match n.strip_prefix("a hover over ") {
                        Some(rest) if !rest.contains(['\n', '\r', '\u{2028}', '\u{2029}']) => {
                            format!("{rest} under a hover")
                        }
                        _ => n,
                    };
                    on.push((renamed, c));
                }
            }
            let mut keys: Vec<f64> = ramp.stops().collect();
            keys.sort_by(|a, b| compare(a - b));
            if let Some(own) = own.as_deref().filter(|o| !o.is_empty())
                && t.parse_color(own).is_some()
                && mp.lowest(own, &on)? >= TEXT
            {
                by_mode[i] = Some(literal(own));
                continue;
            }
            let own_rgb = own
                .as_deref()
                .filter(|o| !o.is_empty())
                .and_then(|o| t.parse_color(o));
            let from = own_rgb
                .map(|rgb| hex(&rgb))
                .or_else(|| base_lift(m))
                .unwrap_or(text_color);
            let color_of = |stop: f64| ramp.get(stop).unwrap_or_default().to_string();
            let Some(&first_key) = keys.first() else {
                return Err(PaletteError::new("The gray ramp has no stop."));
            };
            let mut start = first_key;
            for &y in &keys[1..] {
                if perceptual_delta(&color_of(y), &from)?
                    < perceptual_delta(&color_of(start), &from)?
                {
                    start = y;
                }
            }
            let order = by_distance(&keys, start, toward_contrast(m));
            let mut scored = Vec::with_capacity(order.len());
            for stop in order {
                scored.push((stop, mp.lowest(&color_of(stop), &on)?));
            }
            let (stop, r) = first_passing(&scored, TEXT);
            by_mode[i] = Some(if palette {
                t.stop_ref("neutral", m, stop, None)
            } else {
                StopRef {
                    stop,
                    color: color_of(stop),
                    family: Some("literal".to_string()),
                    ..StopRef::default()
                }
            });
            let shown = t
                .parse_color(&color_of(stop))
                .map(|rgb| hex(&rgb))
                .unwrap_or_default();
            let moved = shown != from;
            if r < TEXT || moved {
                let ratio = js_number_to_string(round(r));
                let note = if r >= TEXT {
                    format!(
                        "--color-text-secondary on {} uses {} in {m}, not {from}, so it stays readable ({ratio}:1) on {} and under a hover",
                        info.parts,
                        color_of(stop),
                        info.what
                    )
                } else {
                    format!(
                        "--color-text-secondary on {} reads at only {ratio}:1 in {m} on {} even at {}, the most contrasting gray (needs {}:1): pick another accent or neutral, or set --color-text-secondary on those parts exactly",
                        info.parts,
                        info.what,
                        color_of(stop),
                        js_number_to_string(TEXT)
                    )
                };
                mp.checks.push(ContrastCheck {
                    token: "--color-text-secondary".to_string(),
                    mode: m,
                    stop,
                    preferred: start,
                    against: format!("{} over the surfaces", info.what),
                    ratio: round(r),
                    needs: TEXT,
                    pass: r >= TEXT,
                    note: Some(note),
                    dials: Some(vec!["accent".to_string(), "neutral".to_string()]),
                    fix: None,
                });
            }
        }
        if let [Some(light), Some(dark)] = by_mode {
            found.push((ground.to_string(), ModeStopRefs { light, dark }));
        }
    }
    Ok((!found.is_empty()).then_some(found))
}

/// What the palette tests share: Core's color reader and resolved defaults
/// (as upstream's `themePalette` reads them), and a text rendering of a
/// mapping that upstream's output, rendered the same way, is compared with.
#[cfg(test)]
pub(crate) mod test_support {
    use std::collections::HashMap;

    use super::super::generator::spec_json::Json;
    use super::*;
    use crate::authoring::TOKEN_DEFAULTS;
    use crate::authoring::color::parse_color;
    use crate::authoring::token_references::{resolve_token_references, split_top_level_comma};

    /// Core's color reader: hex, `rgb()` and `rgba()`, black, white and
    /// transparent. Upstream `coreReader` (themePalette.ts).
    pub(crate) struct CoreReader;

    impl PaletteColorReader for CoreReader {
        fn rgb(&self, value: &str) -> Option<ColorTriple> {
            parse_color(value).map(|c| [c.r, c.g, c.b])
        }

        fn alpha(&self, value: &str) -> f64 {
            if js_trim(value).eq_ignore_ascii_case("transparent") {
                0.0
            } else {
                parse_color(value).map_or(1.0, |c| c.a)
            }
        }
    }

    /// Whether Core has a token (upstream `Object.hasOwn(tokenDefaults, token)`).
    pub(crate) fn has_default(token: &str) -> bool {
        TOKEN_DEFAULTS.iter().any(|(name, _)| *name == token)
    }

    /// Core's default tokens resolved in a mode: each `light-dark()` split,
    /// each `var()` followed (upstream `resolveDefaultTokens`, which also
    /// evaluates `color-mix()`; no token the mapping reads uses it).
    fn resolved_defaults(mode: PaletteMode) -> HashMap<String, String> {
        let raw: Vec<(&str, String)> = TOKEN_DEFAULTS
            .iter()
            .map(|(name, value)| {
                let value = js_trim(value);
                let half = value
                    .strip_prefix("light-dark(")
                    .and_then(|v| v.strip_suffix(')'))
                    .and_then(split_top_level_comma)
                    .map(|(light, dark)| match mode {
                        PaletteMode::Light => light.to_string(),
                        PaletteMode::Dark => dark.to_string(),
                    });
                (*name, half.unwrap_or_else(|| value.to_string()))
            })
            .collect();
        resolve_token_references(&raw).into_iter().collect()
    }

    /// Core's defaults as a mapping's base, with these part styles.
    pub(crate) fn default_base(components: Vec<(String, StyleValue)>) -> PaletteBase {
        PaletteBase {
            light: resolved_defaults(PaletteMode::Light),
            dark: resolved_defaults(PaletteMode::Dark),
            components: Some(components),
        }
    }

    /// A theme definition's `components` as part styles.
    pub(crate) fn styles_from_json(json: &Json) -> Vec<(String, StyleValue)> {
        fn value(json: &Json) -> StyleValue {
            match json {
                Json::String(s) => StyleValue::Text(s.clone()),
                Json::Number(n) => StyleValue::Number(*n),
                Json::Array(items) => {
                    StyleValue::List(items.iter().map(|i| i.str().to_string()).collect())
                }
                Json::Object(entries) => {
                    StyleValue::Block(entries.iter().map(|(k, v)| (k.clone(), value(v))).collect())
                }
                Json::Bool(_) | Json::Null => StyleValue::Text(String::new()),
            }
        }
        json.obj()
            .iter()
            .map(|(k, v)| (k.clone(), value(v)))
            .collect()
    }

    fn n(x: f64) -> String {
        js_number_to_string(x)
    }

    /// A stop as the renderings write it: stop, color, family, alpha, exact.
    pub(crate) fn stop_line(r: &StopRef) -> String {
        format!(
            "{} {} {} {} {}",
            n(r.stop),
            r.color,
            r.family.as_deref().unwrap_or("-"),
            r.alpha.as_deref().unwrap_or("-"),
            u8::from(r.exact)
        )
    }

    /// A token as the renderings write it.
    pub(crate) fn token_line(prefix: &str, name: &str, t: &MappedToken) -> String {
        format!(
            "{prefix} {name} {} {} | {}",
            t.family,
            stop_line(&t.light),
            stop_line(&t.dark)
        )
    }

    fn pair(p: &BasePair) -> String {
        match p {
            BasePair::Category { hue } => format!("category:{hue}"),
            BasePair::Status { role, tint } => format!("status:{role}:{tint}"),
            BasePair::Part { part, block, prop } => format!("part:{part}:{block}:{prop}"),
        }
    }

    /// A planned request as the fixtures write it: each family's id, kind,
    /// seed and anchors, then the fills.
    pub(crate) fn render_plan(families: &[ConfigFamily], fills: &PaletteFills) -> String {
        let mut out: Vec<String> = families
            .iter()
            .map(|f| {
                let anchors: Vec<String> = f
                    .anchors
                    .iter()
                    .flatten()
                    .map(|a| format!("{}:{}:{}", a.mode, n(a.stop), a.color))
                    .collect();
                format!(
                    "{} {} {} {}",
                    f.id,
                    f.kind.map_or("-", |k| k.as_str()),
                    f.seed,
                    anchors.join(",")
                )
            })
            .collect();
        let fills: Vec<String> = fills
            .iter()
            .map(|(id, f)| format!("{id}:{}:{}:{}:{}", n(f.stop), f.dark, n(f.ratio), f.why))
            .collect();
        out.push(format!("fills {}", fills.join(" ; ")));
        out.join("\n")
    }

    /// A mapping as the fixtures write it, line by line.
    pub(crate) fn render(m: &PaletteMapping) -> String {
        let mut out = Vec::new();
        for (name, t) in m.tokens.iter() {
            out.push(token_line("token", name, t));
        }
        for c in &m.contrast {
            let fix = c.fix.as_ref().map_or("-".to_string(), |f| {
                format!(
                    "{},{},{}",
                    f.thing,
                    f.value,
                    f.mode.map_or("-", |m| m.as_str())
                )
            });
            out.push(format!(
                "check {} {} {} {} {} {} {} [{}] note={} dials={} fix={fix}",
                c.token,
                c.mode,
                n(c.stop),
                n(c.preferred),
                n(c.ratio),
                n(c.needs),
                c.pass,
                c.against,
                c.note.as_deref().unwrap_or("-"),
                c.dials.as_ref().map_or("-".to_string(), |d| d.join(","))
            ));
        }
        for (name, v) in &m.exact {
            out.push(format!(
                "exact {name} {} {}",
                v.light.as_deref().unwrap_or("-"),
                v.dark.as_deref().unwrap_or("-")
            ));
        }
        for (name, v) in &m.unread {
            out.push(format!("unread {name} {v}"));
        }
        for (ground, v) in m.lift.iter().flatten() {
            out.push(format!(
                "lift {ground} {} | {}",
                stop_line(&v.light),
                stop_line(&v.dark)
            ));
        }
        for c in &m.cvd {
            out.push(format!(
                "cvd {} {} {} {} {} {} {}",
                c.a,
                c.b,
                c.vision.as_str(),
                c.mode,
                c.colors.join(","),
                c.page.as_deref().unwrap_or("-"),
                c.pages.as_ref().map_or("-".to_string(), |p| p.join(","))
            ));
        }
        for c in &m.code {
            out.push(format!("code {c}"));
        }
        for k in &m.kept {
            out.push(format!(
                "kept {} {} {} {}",
                k.token,
                k.mode,
                pair(&k.pair),
                n(k.ratio)
            ));
        }
        let s = &m.statuses;
        out.push(format!(
            "statuses {} | {} | {}",
            s.roles.join(","),
            s.why.join(" ; "),
            s.short.join(",")
        ));
        let c = &m.categories;
        out.push(format!(
            "categories {} {} {} {}",
            c.badges.as_str(),
            c.chart.as_str(),
            c.alike,
            c.unread
        ));
        for l in &c.limits {
            out.push(format!("limit {l}"));
        }
        for l in &c.moved {
            out.push(format!("moved {l}"));
        }
        out.join("\n")
    }

    /// FNV-1a, 64 bits: a fixture too long to keep whole is kept as this
    /// digest of its rendering.
    pub(crate) fn fnv64(text: &str) -> u64 {
        text.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
        })
    }

    /// Two renderings equal line for line, the first difference named.
    pub(crate) fn assert_rendered(actual: &str, expected: &str) {
        for (index, (a, e)) in actual.lines().zip(expected.lines()).enumerate() {
            assert_eq!(a, e, "line {}", index + 1);
        }
        assert_eq!(
            actual.lines().count(),
            expected.lines().count(),
            "line count\n{actual}"
        );
    }

    /// A rendering whose digest and line count are upstream's; on a
    /// mismatch the rendering is printed, to diff against upstream's.
    pub(crate) fn assert_digest(actual: &str, (digest, lines): (u64, usize)) {
        assert!(
            fnv64(actual) == digest && actual.lines().count() == lines,
            "rendering differs from upstream's (digest {digest:#018x}, {lines} lines):\n{actual}"
        );
    }

    /// The seed each setting of upstream's `themePalette` input reads:
    /// Core's reading of the color as a hex.
    pub(crate) fn dials(input: &[(&str, &str)]) -> Vec<PaletteDial> {
        PALETTE_DIALS
            .iter()
            .filter_map(|id| {
                let (_, value) = input.iter().find(|(k, _)| k == id)?;
                let read = parse_color(value)?;
                Some(PaletteDial {
                    id: id.to_string(),
                    seed: hex(&[read.r, read.g, read.b]),
                    only: None,
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::super::generator::spec_json::{
        Json, request_from_json, spec_palette, spec_theme_definition,
    };
    use super::super::generator::{PALETTE_RECIPE, PALETTE_RECIPE_V2};
    use super::super::types::TonalPaletteFamily;
    use super::test_support::*;
    use super::*;

    /// What a scenario's context sets exactly: the written value of a token
    /// in a mode.
    type Settings<'s> = &'s [(&'s str, PaletteMode, &'s str)];

    /// Upstream `themePalette`'s plan and mapping for its input, against
    /// Core's defaults, with optional exact settings and a set line; rendered
    /// as the fixtures are.
    fn theme_scenario(
        input: &[(&str, &str)],
        recipe: Option<PaletteRecipe>,
        vibrancy: Option<f64>,
        settings: Option<Settings<'_>>,
        set_line: Option<SetLine<'_>>,
    ) -> String {
        let base = default_base(Vec::new());
        let reader = CoreReader;
        let setting_of = |token: &str, mode: PaletteMode| {
            settings?
                .iter()
                .find(|(t, m, _)| *t == token && *m == mode)
                .map(|(_, _, v)| v.to_string())
        };
        let set_exactly =
            |token: &str| settings.is_some_and(|s| s.iter().any(|(t, _, _)| *t == token));
        let mut context = PaletteMappingContext::new(&has_default, &reader);
        context.base = Some(&base);
        if settings.is_some() {
            context.setting_of = Some(&setting_of);
            context.set_exactly = Some(&set_exactly);
        }
        context.set_line = set_line;
        let dials = dials(input);
        let plan = plan_palette_request(
            &dials,
            PalettePlanOptions {
                recipe: Some(recipe.unwrap_or(PALETTE_RECIPE)),
                vibrancy,
            },
            &context,
        )
        .unwrap()
        .unwrap();
        let candidate = generate_tonal_palette(&plan.config).unwrap();
        let mapping = map_palette(&candidate, &dials, &plan.fills, &context).unwrap();
        format!(
            "{}\n----\n{}",
            render_plan(&plan.config.families, &plan.fills),
            render(&mapping)
        )
    }

    /// The mapping alone of a scenario without settings.
    fn mapping_of(
        input: &[(&str, &str)],
        recipe: Option<PaletteRecipe>,
        vibrancy: Option<f64>,
    ) -> PaletteMapping {
        let base = default_base(Vec::new());
        let reader = CoreReader;
        let mut context = PaletteMappingContext::new(&has_default, &reader);
        context.base = Some(&base);
        let dials = dials(input);
        let plan = plan_palette_request(
            &dials,
            PalettePlanOptions {
                recipe: Some(recipe.unwrap_or(PALETTE_RECIPE)),
                vibrancy,
            },
            &context,
        )
        .unwrap()
        .unwrap();
        let candidate = generate_tonal_palette(&plan.config).unwrap();
        map_palette(&candidate, &dials, &plan.fills, &context).unwrap()
    }

    const SIX_SEEDS: [(&str, &str); 6] = [
        ("accent", "#5b3cc4"),
        ("neutral", "#6b6b70"),
        ("success", "#1f8a3b"),
        ("warning", "#e0a100"),
        ("danger", "#d1242f"),
        ("info", "#0a6cd6"),
    ];

    fn is_token_hex(value: &str) -> bool {
        let digits = value.strip_prefix('#').unwrap_or("");
        (digits.len() == 6 || digits.len() == 8)
            && digits
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    }

    /// Every token a mapping sets is one Core has, and a lowercase hex in
    /// both modes (upstream `expectTokensApplied`, short of a theme).
    fn assert_tokens_applied(mapping: &PaletteMapping, label: &str) {
        for (token, t) in mapping.tokens.iter() {
            assert!(has_default(token), "{label}: {token}");
            assert!(
                is_token_hex(&t.light.color),
                "{label}: {token} light {}",
                t.light.color
            );
            assert!(
                is_token_hex(&t.dark.color),
                "{label}: {token} dark {}",
                t.dark.color
            );
        }
    }

    /// Upstream's plan and mapping for this scenario (palette/mapping.ts run
    /// under Node 24 against Core's resolved defaults).
    const SIX: &str = r##"accent - #5b3cc4 light:40:#5b3cc4
neutral neutral #6b6b70 light:45:#6b6b70,dark:45:#6b6b70
success - #1f8a3b light:50:#1f8a3b
warning - #e0a100 light:70:#e0a100,dark:70:#e0a100
danger - #d1242f light:50:#d1242f
info - #0a6cd6 light:45:#0a6cd6,dark:45:#0a6cd6
fills accent:40:false:2.1:your exact color stands out from the dark page at only 2.1:1, and a fill needs 3:1 ; success:50:false:4.41:your exact color reads at only 4.41:1 with any text shade, and text needs 4.5:1 ; warning:70:true:8.32: ; danger:50:false:2.91:your exact color stands out from the dark page at only 2.91:1, and a fill needs 3:1 ; info:45:true:5.08:
----
token --color-background-body neutral 95 #f0f0f3 - - 0 | 10 #1b1b1d - - 0
token --color-background-surface neutral 100 #ffffff - - 0 | 15 #252528 - - 0
token --color-background-card neutral 100 #ffffff - - 0 | 10 #1b1b1d - - 0
token --color-background-popover neutral 100 #ffffff - - 0 | 10 #1b1b1d - - 0
token --color-background-muted neutral 95 #f0f0f3 - - 0 | 10 #1b1b1d - - 0
token --color-text-primary neutral 0 #000000 - - 0 | 100 #ffffff - - 0
token --color-text-disabled neutral 60 #909096 - - 0 | 35 #535356 - - 0
token --color-icon-primary neutral 0 #000000 - - 0 | 100 #ffffff - - 0
token --color-icon-disabled neutral 60 #909096 - - 0 | 35 #535356 - - 0
token --color-border-emphasized neutral 85 #d4d4d8 - - 0 | 35 #535356 - - 0
token --color-skeleton neutral 85 #d4d4d8 - - 0 | 35 #535356 - - 0
token --color-on-dark neutral 100 #ffffff - - 0 | 100 #ffffff - - 0
token --color-on-light neutral 5 #111112 - - 0 | 5 #111112 - - 0
token --color-background-inverted neutral 10 #1b1b1e - - 0 | 95 #f0f0f3 - - 0
token --color-border neutral 0 #00000014 - 14 0 | 100 #ffffff1a - 1a 0
token --color-overlay-hover neutral 0 #0000000d - 0d 0 | 100 #ffffff0d - 0d 0
token --color-overlay-pressed neutral 0 #0000001a - 1a 0 | 100 #ffffff1a - 1a 0
token --color-text-secondary neutral 30 #46464b - - 0 | 65 #9d9da1 - - 0
token --color-icon-secondary neutral 45 #6b6b70 - - 0 | 65 #9d9da1 - - 0
token --color-accent accent 40 #5b3cc4 - - 1 | 70 #a59efa - - 0
token --color-on-accent accent 100 #ffffff neutral - 0 | 5 #111112 neutral - 0
token --color-accent-muted accent 85 #d0cfff - - 0 | 30 #443688 - - 0
token --color-text-accent accent 40 #5b3cc4 - - 0 | 80 #c2beff - - 0
token --color-icon-accent accent 40 #5b3cc4 - - 0 | 80 #c2beff - - 0
token --focus-outline-color accent 40 #5b3cc4 - - 0 | 70 #a59efa - - 0
token --color-success success 50 #1f8a3b - - 1 | 70 #81bc88 - - 0
token --color-on-success success 100 #ffffff neutral - 0 | 5 #111112 neutral - 0
token --color-success-muted success 85 #b4e3b9 - - 0 | 75 #93c8993d - 3d 0
token --color-text-success success 35 #006321 - - 0 | 80 #a6d5ab - - 0
token --color-icon-success success 35 #006321 - - 0 | 80 #a6d5ab - - 0
token --color-on-success-muted success 25 #004916 - - 0 | 80 #a6d5ab - - 0
token --color-border-success success 80 #a0d7a6 - - 0 | 65 #6fb078 - - 0
token --color-warning warning 70 #e0a100 - - 1 | 70 #e0a100 - - 1
token --color-on-warning warning 5 #111112 neutral - 0 | 5 #111112 neutral - 0
token --color-warning-muted warning 85 #ffcd74 - - 0 | 75 #ebaf393d - 3d 0
token --color-text-warning warning 35 #6d4c00 - - 0 | 75 #ebaf39 - - 0
token --color-icon-warning warning 35 #6d4c00 - - 0 | 75 #ebaf39 - - 0
token --color-on-warning-muted warning 25 #503600 - - 0 | 80 #f4bf5c - - 0
token --color-border-warning warning 80 #f5be57 - - 0 | 65 #cf9400 - - 0
token --color-error danger 50 #d1242f - - 1 | 70 #fa837c - - 0
token --color-on-error danger 100 #ffffff neutral - 0 | 5 #111112 neutral - 0
token --color-error-muted danger 85 #ffc3be - - 0 | 75 #ff98903d - 3d 0
token --color-text-error danger 40 #af001b - - 0 | 80 #ffaea8 - - 0
token --color-icon-error danger 40 #af001b - - 0 | 80 #ffaea8 - - 0
token --color-on-error-muted danger 25 #75000e - - 0 | 80 #ffaea8 - - 0
token --color-border-error danger 80 #ffaea8 - - 0 | 65 #ee736d - - 0
token --color-info info 45 #0a6cd6 - - 1 | 45 #0a6cd6 - - 1
token --color-on-info info 100 #ffffff neutral - 0 | 100 #ffffff neutral - 0
token --color-info-muted info 85 #b9d7ff - - 0 | 75 #89bbff3d - 3d 0
token --color-text-info info 35 #0352a8 - - 0 | 75 #89bbff - - 0
token --color-icon-info info 35 #0352a8 - - 0 | 75 #89bbff - - 0
token --color-on-info-muted info 25 #00397c - - 0 | 80 #a2c9ff - - 0
token --color-border-info info 80 #a2c9ff - - 0 | 65 #5c9ff9 - - 0
check --color-text-secondary light 30 30 8.24 4.5 true [--color-background-surface, --color-background-body, --color-background-card] note=- dials=- fix=-
check --color-text-secondary dark 65 65 5.65 4.5 true [--color-background-surface, --color-background-body, --color-background-card] note=- dials=- fix=-
check --color-icon-secondary light 45 45 4.65 3 true [--color-background-surface, --color-background-body, --color-background-card] note=- dials=- fix=-
check --color-icon-secondary dark 65 65 5.65 3 true [--color-background-surface, --color-background-body, --color-background-card] note=- dials=- fix=-
check --color-accent dark 70 40 2.1 4.5 true [the dark page and the text on it] note=in dark, --color-accent is shade 70, not your exact color: your exact color stands out from the dark page at only 2.1:1, and a fill needs 3:1 dials=- fix=-
check --color-on-accent light 100 100 7.27 4.5 true [--color-accent (shade 40)] note=- dials=- fix=-
check --color-on-accent dark 5 5 7.97 4.5 true [--color-accent (shade 70)] note=- dials=- fix=-
check --color-text-accent light 40 40 4.86 4.5 true [--color-background-surface, --color-background-body, --color-background-card, --color-accent-muted] note=- dials=- fix=-
check --color-text-accent dark 80 80 5.68 4.5 true [--color-background-surface, --color-background-body, --color-background-card, --color-accent-muted] note=- dials=- fix=-
check --focus-outline-color light 40 40 6.39 3 true [--color-background-surface, --color-background-body, --color-background-card] note=- dials=- fix=-
check --focus-outline-color dark 70 70 6.45 3 true [--color-background-surface, --color-background-body, --color-background-card] note=- dials=- fix=-
check --color-success dark 70 50 4.41 4.5 true [the dark page and the text on it] note=in dark, --color-success is shade 70, not your exact color: your exact color reads at only 4.41:1 with any text shade, and text needs 4.5:1 dials=- fix=-
check --color-on-success light 100 100 4.41 4.5 false [--color-success (shade 50)] note=text on --color-success (shade 50) reads at only 4.41:1 in light with white or near-black text (needs 4.5:1); the fill keeps your color: pick a lighter or darker success, or set --color-on-success exactly dials=- fix=-
check --color-on-success dark 5 5 8.52 4.5 true [--color-success (shade 70)] note=- dials=- fix=-
check --color-text-success light 35 50 5.21 4.5 true [--color-background-surface, --color-background-body, --color-background-card, --color-success-muted] note=--color-text-success light: your exact color (#1f8a3b) reads at only 4.41:1 on --color-background-surface, 3.88:1 on --color-background-body, 4.41:1 on --color-background-card, 3.07:1 on --color-success-muted (text needs 4.5:1), so success text and icons use shade 35 (#006321), the nearest that reads; the fill keeps your color, and an exact --color-text-success keeps any color you give it dials=- fix=-
check --color-text-success dark 80 80 5.47 4.5 true [--color-background-surface, --color-background-body, --color-background-card, --color-success-muted over --color-background-surface, --color-success-muted over --color-background-body, --color-success-muted over --color-background-card] note=- dials=- fix=-
check --color-on-success-muted light 25 25 7.44 4.5 true [--color-success-muted] note=- dials=- fix=-
check --color-on-success-muted dark 80 80 5.47 4.5 true [--color-success-muted over --color-background-surface, --color-success-muted over --color-background-body, --color-success-muted over --color-background-card] note=- dials=- fix=-
check --color-on-warning light 5 5 8.32 4.5 true [--color-warning (shade 70)] note=- dials=- fix=-
check --color-on-warning dark 5 5 8.32 4.5 true [--color-warning (shade 70)] note=- dials=- fix=-
check --color-warning light 70 70 1.99 3 false [the light page] note=in light, your exact warning stands out from the page at only 1.99:1, and a fill needs 3:1, so --color-warning buttons and badges may not show: pick a darker warning dials=- fix=warning,#b38100,-
check --color-text-warning light 35 70 5.3 4.5 true [--color-background-surface, --color-background-body, --color-background-card, --color-warning-muted] note=--color-text-warning light: your exact color (#e0a100) reads at only 2.26:1 on --color-background-surface, 1.99:1 on --color-background-body, 2.26:1 on --color-background-card, 1.53:1 on --color-warning-muted (text needs 4.5:1), so warning text and icons use shade 35 (#6d4c00), the nearest that reads; the fill keeps your color, and an exact --color-text-warning keeps any color you give it dials=- fix=-
check --color-text-warning dark 75 70 4.68 4.5 true [--color-background-surface, --color-background-body, --color-background-card, --color-warning-muted over --color-background-surface, --color-warning-muted over --color-background-body, --color-warning-muted over --color-background-card] note=--color-text-warning dark: your exact color (#e0a100) reads at only 4.04:1 on --color-warning-muted over --color-background-surface (text needs 4.5:1), so warning text and icons use shade 75 (#ebaf39), the nearest that reads; the fill keeps your color, and an exact --color-text-warning keeps any color you give it dials=- fix=-
check --color-on-warning-muted light 25 25 7.61 4.5 true [--color-warning-muted] note=- dials=- fix=-
check --color-on-warning-muted dark 80 80 5.43 4.5 true [--color-warning-muted over --color-background-surface, --color-warning-muted over --color-background-body, --color-warning-muted over --color-background-card] note=- dials=- fix=-
check --color-error dark 70 50 2.91 4.5 true [the dark page and the text on it] note=in dark, --color-error is shade 70, not your exact color: your exact color stands out from the dark page at only 2.91:1, and a fill needs 3:1 dials=- fix=-
check --color-on-error light 100 100 5.24 4.5 true [--color-error (shade 50)] note=- dials=- fix=-
check --color-on-error dark 5 5 7.73 4.5 true [--color-error (shade 70)] note=- dials=- fix=-
check --color-text-error light 40 50 4.86 4.5 true [--color-background-surface, --color-background-body, --color-background-card, --color-error-muted] note=--color-text-error light: your exact color (#d1242f) reads at only 3.44:1 on --color-error-muted (text needs 4.5:1), so danger text and icons use shade 40 (#af001b), the nearest that reads; the fill keeps your color, and an exact --color-text-error keeps any color you give it dials=- fix=-
check --color-text-error dark 80 80 5.25 4.5 true [--color-background-surface, --color-background-body, --color-background-card, --color-error-muted over --color-background-surface, --color-error-muted over --color-background-body, --color-error-muted over --color-background-card] note=- dials=- fix=-
check --color-on-error-muted light 25 25 7.82 4.5 true [--color-error-muted] note=- dials=- fix=-
check --color-on-error-muted dark 80 80 5.25 4.5 true [--color-error-muted over --color-background-surface, --color-error-muted over --color-background-body, --color-error-muted over --color-background-card] note=- dials=- fix=-
check --color-on-info light 100 100 5.08 4.5 true [--color-info (shade 45)] note=- dials=- fix=-
check --color-on-info dark 100 5 5.08 4.5 true [--color-info (shade 45)] note=--color-on-info uses white text in dark, not near-black, so it reads at 5.08:1 on --color-info (shade 45), where near-black text reads at only 3.71:1 (button and badge labels are normal-size text, which needs 4.5:1); to keep near-black text anyway, set --color-on-info exactly dials=- fix=-
check --color-text-info light 35 45 5.12 4.5 true [--color-background-surface, --color-background-body, --color-background-card, --color-info-muted] note=--color-text-info light: your exact color (#0a6cd6) reads at only 4.47:1 on --color-background-body, 3.44:1 on --color-info-muted (text needs 4.5:1), so info text and icons use shade 35 (#0352a8), the nearest that reads; the fill keeps your color, and an exact --color-text-info keeps any color you give it dials=- fix=-
check --color-text-info dark 75 45 4.61 4.5 true [--color-background-surface, --color-background-body, --color-background-card, --color-info-muted over --color-background-surface, --color-info-muted over --color-background-body, --color-info-muted over --color-background-card] note=--color-text-info dark: your exact color (#0a6cd6) reads at only 3:1 on --color-background-surface, 3.38:1 on --color-background-body, 3.38:1 on --color-background-card, 1.79:1 on --color-info-muted over --color-background-surface, 2.03:1 on --color-info-muted over --color-background-body, 2.03:1 on --color-info-muted over --color-background-card (text needs 4.5:1), so info text and icons use shade 75 (#89bbff), the nearest that reads; the fill keeps your color, and an exact --color-text-info keeps any color you give it dials=- fix=-
check --color-on-info-muted light 25 25 7.58 4.5 true [--color-info-muted] note=- dials=- fix=-
check --color-on-info-muted dark 80 80 5.35 4.5 true [--color-info-muted over --color-background-surface, --color-info-muted over --color-background-body, --color-info-muted over --color-background-card] note=- dials=- fix=-
check --color-text-secondary dark 80 65 5.03 4.5 true [the accent tint over the surfaces] note=--color-text-secondary on selected items and rows, a dropzone, numbered citations uses #c6c6c9 in dark, not #9d9da1, so it stays readable (5.03:1) on the accent tint and under a hover dials=accent,neutral fix=-
check --color-text-secondary dark 85 65 5.11 4.5 true [the neutral track over the surfaces] note=--color-text-secondary on segmented-control labels uses #d4d4d7 in dark, not #9d9da1, so it stays readable (5.11:1) on the neutral track and under a hover dials=accent,neutral fix=-
check --color-text-secondary dark 70 65 4.78 4.5 true [the blue card fill over the surfaces] note=--color-text-secondary on blue cards uses #ababaf in dark, not #9d9da1, so it stays readable (4.78:1) on the blue card fill and under a hover dials=accent,neutral fix=-
check --color-text-secondary dark 75 65 4.88 4.5 true [the cyan card fill over the surfaces] note=--color-text-secondary on cyan cards uses #b8b8bc in dark, not #9d9da1, so it stays readable (4.88:1) on the cyan card fill and under a hover dials=accent,neutral fix=-
check --color-text-secondary dark 75 65 4.71 4.5 true [the green card fill over the surfaces] note=--color-text-secondary on green cards uses #b8b8bc in dark, not #9d9da1, so it stays readable (4.71:1) on the green card fill and under a hover dials=accent,neutral fix=-
check --color-text-secondary dark 75 65 4.88 4.5 true [the orange card fill over the surfaces] note=--color-text-secondary on orange cards uses #b8b8bc in dark, not #9d9da1, so it stays readable (4.88:1) on the orange card fill and under a hover dials=accent,neutral fix=-
check --color-text-secondary dark 70 65 4.61 4.5 true [the pink card fill over the surfaces] note=--color-text-secondary on pink cards uses #ababaf in dark, not #9d9da1, so it stays readable (4.61:1) on the pink card fill and under a hover dials=accent,neutral fix=-
check --color-text-secondary dark 70 65 4.68 4.5 true [the purple card fill over the surfaces] note=--color-text-secondary on purple cards uses #ababaf in dark, not #9d9da1, so it stays readable (4.68:1) on the purple card fill and under a hover dials=accent,neutral fix=-
check --color-text-secondary dark 70 65 5.03 4.5 true [the red card fill over the surfaces] note=--color-text-secondary on red cards uses #ababaf in dark, not #9d9da1, so it stays readable (5.03:1) on the red card fill and under a hover dials=accent,neutral fix=-
check --color-text-secondary dark 75 65 4.73 4.5 true [the teal card fill over the surfaces] note=--color-text-secondary on teal cards uses #b8b8bc in dark, not #9d9da1, so it stays readable (4.73:1) on the teal card fill and under a hover dials=accent,neutral fix=-
check --color-text-secondary dark 75 65 4.54 4.5 true [the yellow card fill over the surfaces] note=--color-text-secondary on yellow cards uses #b8b8bc in dark, not #9d9da1, so it stays readable (4.54:1) on the yellow card fill and under a hover dials=accent,neutral fix=-
lift --color-accent-muted 30 #46464b - - 0 | 80 #c6c6c9 - - 0
lift --color-neutral 30 #46464b - - 0 | 85 #d4d4d7 - - 0
lift --color-background-blue 30 #46464b - - 0 | 70 #ababaf - - 0
lift --color-background-cyan 30 #46464b - - 0 | 75 #b8b8bc - - 0
lift --color-background-green 30 #46464b - - 0 | 75 #b8b8bc - - 0
lift --color-background-orange 30 #46464b - - 0 | 75 #b8b8bc - - 0
lift --color-background-pink 30 #46464b - - 0 | 70 #ababaf - - 0
lift --color-background-purple 30 #46464b - - 0 | 70 #ababaf - - 0
lift --color-background-red 30 #46464b - - 0 | 70 #ababaf - - 0
lift --color-background-teal 30 #46464b - - 0 | 75 #b8b8bc - - 0
lift --color-background-yellow 30 #46464b - - 0 | 75 #b8b8bc - - 0
statuses  |  | 
categories none none false false"##;
    /// Upstream's plan and mapping for this scenario (palette/mapping.ts run
    /// under Node 24 against Core's resolved defaults).
    const EXACT: &str = r##"accent - #fde047 light:85:#fde047,dark:85:#fde047
neutral neutral #6b6b70 light:45:#6b6b70,dark:45:#6b6b70
danger - #d1242f light:50:#d1242f
fills accent:85:true:14.31: ; danger:50:false:2.91:your exact color stands out from the dark page at only 2.91:1, and a fill needs 3:1
----
token --color-background-body neutral 95 #f0f0f3 - - 0 | 10 #1b1b1d - - 0
token --color-background-surface neutral 100 #ffffff - - 0 | 15 #252528 - - 0
token --color-background-card neutral 100 #ffffff - - 0 | 10 #1b1b1d - - 0
token --color-background-popover neutral 100 #ffffff - - 0 | 10 #1b1b1d - - 0
token --color-background-muted neutral 95 #f0f0f3 - - 0 | 10 #1b1b1d - - 0
token --color-text-primary neutral 0 #000000 - - 0 | 100 #ffffff - - 0
token --color-text-disabled neutral 60 #909096 - - 0 | 35 #535356 - - 0
token --color-icon-primary neutral 0 #000000 - - 0 | 100 #ffffff - - 0
token --color-icon-disabled neutral 60 #909096 - - 0 | 35 #535356 - - 0
token --color-border-emphasized neutral 85 #d4d4d8 - - 0 | 35 #535356 - - 0
token --color-skeleton neutral 85 #d4d4d8 - - 0 | 35 #535356 - - 0
token --color-on-dark neutral 100 #ffffff - - 0 | 100 #ffffff - - 0
token --color-on-light neutral 5 #111112 - - 0 | 5 #111112 - - 0
token --color-background-inverted neutral 10 #1b1b1e - - 0 | 95 #f0f0f3 - - 0
token --color-border neutral 0 #00000014 - 14 0 | 100 #ffffff1a - 1a 0
token --color-overlay-hover neutral 0 #0000000d - 0d 0 | 100 #ffffff0d - 0d 0
token --color-overlay-pressed neutral 0 #0000001a - 1a 0 | 100 #ffffff1a - 1a 0
token --color-text-secondary neutral 30 #46464b - - 0 | 65 #9d9da1 - - 0
token --color-icon-secondary neutral 45 #6b6b70 - - 0 | 65 #9d9da1 - - 0
token --color-accent accent 85 #fde047 - - 1 | 85 #fde047 - - 1
token --color-on-accent accent 100 #ffffff neutral - 0 | 5 #111112 neutral - 0
token --color-accent-muted accent 90 #ffed92 - - 0 | 30 #534700 - - 0
token --color-text-accent accent 35 #605200 - - 0 | 85 #fde047 - - 0
token --color-icon-accent accent 35 #605200 - - 0 | 85 #fde047 - - 0
token --color-error danger 50 #d1242f - - 1 | 70 #fa837c - - 0
token --color-on-error danger 100 #ffffff neutral - 0 | 100 #ffffff neutral - 0
token --color-error-muted danger 85 #ffc3be - - 0 | 75 #ff98903d - 3d 0
token --color-text-error danger 40 #af001b - - 0 | 80 #ffaea8 - - 0
token --color-icon-error danger 40 #af001b - - 0 | 80 #ffaea8 - - 0
token --color-on-error-muted danger 25 #75000e - - 0 | 80 #ffaea8 - - 0
token --color-border-error danger 80 #ffaea8 - - 0 | 65 #ee736d - - 0
check --color-icon-secondary light 45 45 4.65 3 true [--color-background-surface, --color-background-body, --color-background-card] note=- dials=- fix=-
check --color-icon-secondary dark 65 65 5.65 3 true [--color-background-surface, --color-background-body, --color-background-card] note=- dials=- fix=-
check --color-on-accent light 100 100 5.83 4.5 true [your exact --color-accent (#2255ee)] note=- dials=- fix=-
check --color-on-accent dark 5 5 14.31 4.5 true [--color-accent (shade 85)] note=- dials=- fix=-
check --color-text-accent light 35 35 6.57 4.5 true [--color-background-surface, --color-background-body, --color-background-card, --color-accent-muted] note=- dials=- fix=-
check --color-text-accent dark 85 85 7.02 4.5 true [--color-background-surface, --color-background-body, --color-background-card, --color-accent-muted] note=- dials=- fix=-
check --color-on-error light 100 100 5.24 4.5 true [--color-error (shade 50)] note=- dials=- fix=-
check --color-on-error dark 100 5 9.05 4.5 true [your exact --color-error (#ff000080, as it shows: #921214)] note=--color-on-error uses white text in dark, not near-black, so it reads at 9.05:1 on your exact --color-error (#ff000080, as it shows: #921214), where near-black text reads at only 2.08:1 (button and badge labels are normal-size text, which needs 4.5:1); to keep near-black text anyway, set --color-on-error exactly dials=- fix=-
check --color-text-error light 40 50 4.86 4.5 true [--color-background-surface, --color-background-body, --color-background-card, --color-error-muted] note=--color-text-error light: your exact color (#d1242f) reads at only 3.44:1 on --color-error-muted (text needs 4.5:1), so danger text and icons use shade 40 (#af001b), the nearest that reads; the fill keeps your color, and an exact --color-text-error keeps any color you give it dials=- fix=-
check --color-text-error dark 80 35 5.25 4.5 true [--color-background-surface, --color-background-body, --color-background-card, --color-error-muted over --color-background-surface, --color-error-muted over --color-background-body, --color-error-muted over --color-background-card] note=--color-text-error dark: your exact color (#921214) reads at only 1.68:1 on --color-background-surface, 1.89:1 on --color-background-body, 1.89:1 on --color-background-card, 1.02:1 on --color-error-muted over --color-background-surface, 1.15:1 on --color-error-muted over --color-background-body, 1.15:1 on --color-error-muted over --color-background-card (text needs 4.5:1), so danger text and icons use shade 80 (#ffaea8), the nearest that reads; the fill keeps your color, and an exact --color-text-error keeps any color you give it dials=- fix=-
check --color-on-error-muted light 25 25 7.82 4.5 true [--color-error-muted] note=- dials=- fix=-
check --color-on-error-muted dark 80 80 5.25 4.5 true [--color-error-muted over --color-background-surface, --color-error-muted over --color-background-body, --color-error-muted over --color-background-card] note=- dials=- fix=-
check --color-text-secondary dark 80 75 4.72 4.5 true [the accent tint over the surfaces] note=--color-text-secondary on selected items and rows, a dropzone, numbered citations uses #c6c6c9 in dark, not #bbbbbb, so it stays readable (4.72:1) on the accent tint and under a hover dials=accent,neutral fix=-
check --color-text-secondary dark 85 75 5.11 4.5 true [the neutral track over the surfaces] note=--color-text-secondary on segmented-control labels uses #d4d4d7 in dark, not #bbbbbb, so it stays readable (5.11:1) on the neutral track and under a hover dials=accent,neutral fix=-
exact --color-background-surface #fafafa -
exact --color-accent #2255ee -
exact --color-error - #ff000080
unread --color-accent var(--brand)
lift --color-accent-muted 30 #46464b - - 0 | 80 #c6c6c9 - - 0
lift --color-neutral 30 #46464b - - 0 | 85 #d4d4d7 - - 0
lift --color-background-blue 30 #46464b - - 0 | 0 #bbbbbb literal - 0
lift --color-background-cyan 30 #46464b - - 0 | 0 #bbbbbb literal - 0
lift --color-background-green 30 #46464b - - 0 | 0 #bbbbbb literal - 0
lift --color-background-orange 30 #46464b - - 0 | 0 #bbbbbb literal - 0
lift --color-background-pink 30 #46464b - - 0 | 0 #bbbbbb literal - 0
lift --color-background-purple 30 #46464b - - 0 | 0 #bbbbbb literal - 0
lift --color-background-red 30 #46464b - - 0 | 0 #bbbbbb literal - 0
lift --color-background-teal 30 #46464b - - 0 | 0 #bbbbbb literal - 0
lift --color-background-yellow 30 #46464b - - 0 | 0 #bbbbbb literal - 0
statuses  |  | 
categories none none false false"##;
    /// The FNV-1a digest and line count of upstream's plan and mapping for
    /// this scenario, run under Node 24 and rendered as [`render`] renders
    /// it (a mismatch prints the rendering to diff against upstream's).
    const PALE: (u64, usize) = (0x89a8575bc2a119ea, 66);
    /// The FNV-1a digest and line count of upstream's plan and mapping for
    /// this scenario, run under Node 24 and rendered as [`render`] renders
    /// it (a mismatch prints the rendering to diff against upstream's).
    const GRAYS: (u64, usize) = (0x9d9abdfcaea2ed94, 64);
    /// The FNV-1a digest and line count of upstream's plan and mapping for
    /// this scenario, run under Node 24 and rendered as [`render`] renders
    /// it (a mismatch prints the rendering to diff against upstream's).
    const V1: (u64, usize) = (0xcbf2f5de4b1b56a1, 64);
    /// The FNV-1a digest and line count of upstream's plan and mapping for
    /// this scenario, run under Node 24 and rendered as [`render`] renders
    /// it (a mismatch prints the rendering to diff against upstream's).
    const V2: (u64, usize) = (0x64445ba6e6cea3c9, 64);
    /// The FNV-1a digest and line count of upstream's plan and mapping for
    /// this scenario, run under Node 24 and rendered as [`render`] renders
    /// it (a mismatch prints the rendering to diff against upstream's).
    const YELLOW: (u64, usize) = (0x53f0202bfeb331ca, 124);
    /// The FNV-1a digest and line count of upstream's plan and mapping for
    /// this scenario, run under Node 24 and rendered as [`render`] renders
    /// it (a mismatch prints the rendering to diff against upstream's).
    const BLACK: (u64, usize) = (0xf666dbb287360132, 124);

    #[test]
    fn plans_and_maps_six_settings_as_upstream() {
        assert_rendered(&theme_scenario(&SIX_SEEDS, None, None, None, None), SIX);
    }

    #[test]
    fn maps_the_seeds_to_the_full_color_token_set_status_included() {
        // Upstream themePalette.test.ts.
        let mapping = mapping_of(&SIX_SEEDS, None, None);
        assert_tokens_applied(&mapping, "six");
        for token in [
            "--color-background-body",
            "--color-background-surface",
            "--color-text-primary",
            "--color-text-secondary",
            "--color-border",
            "--color-accent",
            "--color-on-accent",
            "--color-text-accent",
            "--color-accent-muted",
            "--focus-outline-color",
            "--color-success",
            "--color-on-warning",
            "--color-text-error",
            "--color-border-info",
            "--color-info-muted",
        ] {
            assert!(mapping.tokens.contains(token), "{token}");
        }
        // The accent keeps the exact color given, as the editor anchors it.
        assert_eq!(
            mapping.tokens.color("--color-accent", PaletteMode::Light),
            Some("#5b3cc4")
        );
        // The lift that becomes item/selected's part style.
        let lift = mapping.lift.as_ref().unwrap();
        assert!(
            lift.iter()
                .any(|(ground, _)| ground == "--color-accent-muted")
        );
    }

    #[test]
    fn runs_v1_when_no_recipe_is_named_and_v2_only_when_named() {
        // Upstream themePalette.test.ts: naming v1 is not naming one, and v2
        // changes the tokens.
        let seeds = [("accent", "#5b3cc4"), ("neutral", "#706a62")];
        let plain = mapping_of(&seeds, None, None);
        assert_eq!(plain, mapping_of(&seeds, Some(PALETTE_RECIPE), None));
        let v2 = mapping_of(&seeds, Some(PALETTE_RECIPE_V2), None);
        assert_ne!(v2.tokens, plain.tokens);
        assert_digest(&theme_scenario(&seeds, None, None, None, None), V1);
        assert_digest(
            &theme_scenario(&seeds, Some(PALETTE_RECIPE_V2), None, None, None),
            V2,
        );
    }

    #[test]
    fn gives_the_same_result_for_the_same_input() {
        // Upstream themePalette.test.ts.
        let input = [("accent", "#c43c7a"), ("info", "#2b6cb0")];
        assert_eq!(
            mapping_of(&input, None, Some(20.0)),
            mapping_of(&input, None, Some(20.0))
        );
    }

    #[test]
    fn lifts_secondary_text_on_grays_grown_from_the_base_without_a_neutral() {
        let input = [("accent", "#c43c7a"), ("info", "#2b6cb0")];
        assert_digest(&theme_scenario(&input, None, Some(20.0), None, None), GRAYS);
    }

    #[test]
    fn reads_a_seed_in_any_form_core_reads_a_color_in() {
        // Upstream themePalette.test.ts.
        assert_eq!(
            mapping_of(&[("accent", "rgb(91, 60, 196)")], None, None),
            mapping_of(&[("accent", "#5B3CC4")], None, None)
        );
    }

    /// Upstream themePalette.test.ts's `seeded` generator: a linear
    /// congruential sequence and colors drawn from it.
    struct Seeded(f64);

    impl Seeded {
        fn next(&mut self) -> f64 {
            self.0 = (self.0 * 1664525.0 + 1013904223.0) % 4294967296.0;
            self.0 / 4294967296.0
        }

        fn color(&mut self) -> String {
            format!(
                "#{:06x}",
                (self.next() * f64::from(0xffffff)).floor() as u32
            )
        }
    }

    #[test]
    fn builds_a_mapping_for_random_seeds_settings_and_recipes() {
        // Upstream themePalette.test.ts, short of a theme: 40 random inputs.
        let mut seeded = Seeded(20261116.0);
        for i in 0..40 {
            let mut input: Vec<(&str, String)> = Vec::new();
            for dial in PALETTE_DIALS {
                if seeded.next() < 0.6 {
                    input.push((dial, seeded.color()));
                }
            }
            if input.is_empty() {
                input.push(("accent", seeded.color()));
            }
            let vibrancy = (seeded.next() < 0.5).then(|| (seeded.next() * 101.0).floor());
            let recipe = (seeded.next() < 0.5).then_some(PALETTE_RECIPE_V2);
            let input: Vec<(&str, &str)> = input.iter().map(|(k, v)| (*k, v.as_str())).collect();
            let mapping = mapping_of(&input, recipe, vibrancy);
            assert_tokens_applied(&mapping, &format!("random {i}"));
        }
    }

    #[test]
    fn builds_a_mapping_for_extreme_seeds_in_every_role_both_recipes_and_the_vibrancy_ends() {
        // Upstream themePalette.test.ts.
        for seed in [
            "#000000", "#ffffff", "#ff0000", "#00ff00", "#0000ff", "#ffff00", "#808080", "#010101",
            "#fefefe",
        ] {
            let input: Vec<(&str, &str)> = PALETTE_DIALS.iter().map(|d| (*d, seed)).collect();
            for recipe in [PALETTE_RECIPE, PALETTE_RECIPE_V2] {
                for vibrancy in [0.0, 100.0] {
                    let mapping = mapping_of(&input, Some(recipe), Some(vibrancy));
                    assert_tokens_applied(&mapping, seed);
                }
            }
        }
        let every = |seed: &'static str| -> Vec<(&'static str, &'static str)> {
            PALETTE_DIALS.iter().map(|d| (*d, seed)).collect()
        };
        assert_digest(
            &theme_scenario(
                &every("#ffff00"),
                Some(PALETTE_RECIPE_V2),
                Some(100.0),
                None,
                None,
            ),
            YELLOW,
        );
        assert_digest(
            &theme_scenario(&every("#000000"), None, Some(0.0), None, None),
            BLACK,
        );
    }

    #[test]
    fn keeps_exact_settings_and_reports_what_they_give_up() {
        // An exact light accent, an unreadable dark one, a translucent dark
        // error, an exact dark secondary text and an exact light surface.
        let settings: Settings<'_> = &[
            ("--color-accent", PaletteMode::Light, "#2255ee"),
            ("--color-accent", PaletteMode::Dark, "var(--brand)"),
            ("--color-error", PaletteMode::Dark, "#ff000080"),
            ("--color-text-secondary", PaletteMode::Dark, "#bbbbbb"),
            ("--color-background-surface", PaletteMode::Light, "#fafafa"),
        ];
        let set_line = |thing: &str, value: &str, mode: Option<PaletteMode>| {
            format!(
                "{thing}: {value}{}",
                mode.map_or(String::new(), |m| format!(" ({m})"))
            )
        };
        let input = [
            ("accent", "#fde047"),
            ("neutral", "#6b6b70"),
            ("danger", "#d1242f"),
        ];
        assert_rendered(
            &theme_scenario(&input, None, None, Some(settings), Some(&set_line)),
            EXACT,
        );
    }

    #[test]
    fn reports_pale_fills_with_the_nearest_darker_color() {
        let set_line = |thing: &str, value: &str, mode: Option<PaletteMode>| {
            format!(
                "{thing}={value}{}",
                mode.map_or(String::new(), |m| format!("@{m}"))
            )
        };
        let input = [("accent", "#fde047"), ("success", "#a7f3d0")];
        assert_digest(
            &theme_scenario(&input, None, None, None, Some(&set_line)),
            PALE,
        );
    }

    /// The FNV-1a digest and line count of upstream's plan and mapping for
    /// a dark accent whose dark fill has a shade nearer its color (run under Node 24 and rendered as [`render`] renders it; a
    /// mismatch prints the rendering to diff against upstream's).
    const CLOSER: (u64, usize) = (0x5523735d70c073da, 51);
    /// The FNV-1a digest and line count of upstream's plan and mapping for
    /// mid-gray dark surfaces and a pale exact light accent (run under Node 24 and rendered as [`render`] renders it; a
    /// mismatch prints the rendering to diff against upstream's).
    const MIDGRAY: (u64, usize) = (0x5cf416e2dc029986, 64);
    /// The FNV-1a digest and line count of upstream's plan and mapping for
    /// a see-through exact tint over a black card, where no gray lifts (run under Node 24 and rendered as [`render`] renders it; a
    /// mismatch prints the rendering to diff against upstream's).
    const LOST_LIFT: (u64, usize) = (0x4d5d7d2fa1b0345a, 76);

    #[test]
    fn offers_a_dark_fill_nearer_the_exact_color_as_upstream() {
        let set_line = |thing: &str, value: &str, mode: Option<PaletteMode>| {
            format!(
                "{thing}={value}{}",
                mode.map_or(String::new(), |m| format!("@{m}"))
            )
        };
        assert_digest(
            &theme_scenario(&[("accent", "#1e1b4b")], None, None, None, Some(&set_line)),
            CLOSER,
        );
    }

    #[test]
    fn moves_the_dark_fill_off_mid_gray_surfaces_and_fixes_a_pale_exact_fill_as_upstream() {
        let set_line = |thing: &str, value: &str, mode: Option<PaletteMode>| {
            format!(
                "{thing}={value}{}",
                mode.map_or(String::new(), |m| format!("@{m}"))
            )
        };
        let settings: Settings<'_> = &[
            ("--color-background-surface", PaletteMode::Dark, "#808080"),
            ("--color-background-body", PaletteMode::Dark, "#808080"),
            ("--color-background-card", PaletteMode::Dark, "#808080"),
            ("--color-accent", PaletteMode::Light, "#fef9c3"),
        ];
        let input = [("accent", "#5b3cc4"), ("success", "#1f8a3b")];
        assert_digest(
            &theme_scenario(&input, None, None, Some(settings), Some(&set_line)),
            MIDGRAY,
        );
    }

    #[test]
    fn reports_secondary_text_no_gray_lifts_as_upstream() {
        let settings: Settings<'_> = &[
            ("--color-accent-muted", PaletteMode::Light, "#80808080"),
            ("--color-background-card", PaletteMode::Light, "#000000"),
        ];
        let input = [("accent", "#5b3cc4"), ("neutral", "#6b6b70")];
        assert_digest(
            &theme_scenario(&input, None, None, Some(settings), None),
            LOST_LIFT,
        );
    }

    #[test]
    fn official_neutral_stops_equal_the_neutral_theme_definition() {
        // Upstream officialNeutral.test.ts reads the Neutral theme's source;
        // here its definition: each token's [light, dark] value is a family's
        // stop, with the alpha, in the regenerated Neutral palette.
        let spec = spec_palette("neutral");
        let palette =
            generate_tonal_palette(&request_from_json(spec.get("request").unwrap())).unwrap();
        let theme = spec_theme_definition("neutral");
        let tokens = theme.get("tokens").unwrap();
        for (token, [light, dark]) in OFFICIAL_NEUTRAL_STOPS {
            let (l, d) = match tokens.get(token).unwrap() {
                Json::Array(pair) => (pair[0].str(), pair[1].str()),
                other => (other.str(), other.str()),
            };
            let shows = |family: &TonalPaletteFamily,
                         mode: PaletteMode,
                         stop: OfficialStop,
                         value: &str| {
                family
                    .ramp(mode)
                    .and_then(|r| r.get(f64::from(stop.stop)))
                    .is_some_and(|c| {
                        format!("{c}{}", stop.alpha.unwrap_or("")).eq_ignore_ascii_case(value)
                    })
            };
            assert!(
                palette
                    .palette
                    .iter()
                    .any(|f| shows(f, PaletteMode::Light, light, l)
                        && shows(f, PaletteMode::Dark, dark, d)),
                "{token}"
            );
        }
        // The parts whose secondary text it lifts.
        let mut lifted: Vec<(String, String)> = Vec::new();
        for (part, blocks) in theme.get("components").unwrap().obj() {
            for (block, style) in blocks.obj() {
                if style.get("--color-text-secondary").is_some() {
                    lifted.push((part.clone(), block.clone()));
                }
            }
        }
        let mut official: Vec<(String, String)> = OFFICIAL_NEUTRAL_LIFTED
            .iter()
            .map(|(p, b)| (p.to_string(), b.to_string()))
            .collect();
        lifted.sort();
        official.sort();
        assert_eq!(lifted, official);
    }

    #[test]
    fn derived_tables_equal_what_upstream_derives_them_from() {
        // NEUTRAL_STOPS, NEUTRAL_TINTS, TEXT_STOP, ACCENT_MUTED, STATUS_MUTED,
        // ON_MUTED and ON_FILL are read from OFFICIAL_NEUTRAL_STOPS upstream.
        for (token, pair) in NEUTRAL_STOPS {
            let expected = if token == "--color-background-inverted" {
                let [l, d] = official_stops("--color-background-card").unwrap();
                [d, l]
            } else {
                official_stops(token).unwrap()
            };
            assert_eq!(pair, expected, "{token}");
        }
        for (token, pair) in NEUTRAL_TINTS {
            let official = OFFICIAL_NEUTRAL_STOPS
                .iter()
                .find(|(t, _)| *t == token)
                .unwrap()
                .1;
            for (i, (stop, alpha)) in pair.iter().enumerate() {
                assert_eq!(*stop, f64::from(official[i].stop));
                assert_eq!(Some(*alpha), official[i].alpha);
            }
        }
        assert_eq!(TEXT_STOP, official_stops("--color-text-info").unwrap());
        assert_eq!(
            ACCENT_MUTED,
            official_stops("--color-accent-muted").unwrap()
        );
        assert_eq!(
            STATUS_MUTED,
            OFFICIAL_NEUTRAL_STOPS
                .iter()
                .find(|(t, _)| *t == "--color-info-muted")
                .unwrap()
                .1
        );
        assert_eq!(ON_MUTED, official_stops("--color-on-info-muted").unwrap());
        let [on_light, on_dark] = official_stops("--color-on-accent").unwrap();
        assert_eq!(on_fill(PaletteMode::Light), [on_light, on_dark]);
        assert_eq!(on_fill(PaletteMode::Dark), [on_dark, on_light]);
        // LIFT_PARTS is OFFICIAL_NEUTRAL_LIFTED with each block's ground.
        for ((part, block, ground), (p, b)) in LIFT_PARTS.iter().zip(OFFICIAL_NEUTRAL_LIFTED) {
            assert_eq!((*part, *block), (p, b));
            assert_eq!(*ground, lift_ground(part, block));
        }
        for (ground, info) in LIFT_GROUNDS.iter().skip(2) {
            let color = ground.strip_prefix("--color-background-").unwrap();
            assert_eq!(info.what, format!("the {color} card fill"));
            assert_eq!(info.parts, format!("{color} cards"));
        }
    }

    #[test]
    fn lift_ground_reads_the_background_a_block_sits_on() {
        assert_eq!(
            lift_ground("segmented-control-item", "base"),
            "--color-neutral"
        );
        assert_eq!(
            lift_ground("card", "variant:teal"),
            "--color-background-teal"
        );
        assert_eq!(lift_ground("card", "base"), "--color-accent-muted");
        assert_eq!(lift_ground("item", "variant:teal"), "--color-accent-muted");
    }

    #[test]
    fn small_helpers_match_upstream() {
        // Values from mapping.ts run under Node 24.
        assert_eq!(hex(&[255.4, -3.0, 127.5]), "#ff0080");
        assert_eq!(hex(&[f64::NAN, 1.0, 2.0]), "#NaN0102");
        assert_eq!(hex(&[]), "#");
        assert_eq!(ratio(&[0.0, 0.0, 0.0], &[255.0, 255.0, 255.0]), 21.0);
        assert_eq!(round(4.4999), 4.49);
        assert_eq!(round2(1.005), 1.0);
        assert_eq!(round2(2.675), 2.68);
        assert_eq!(shade(16.666666666666668), "16.66");
        assert_eq!(nearest_stop(&[0.0, 40.0, 60.0, 100.0], 50.0), Some(40.0));
        assert_eq!(nearest_stop(&[0.0, 30.0, 70.0, 100.0], 50.0), Some(30.0));
        assert_eq!(nearest_stop(&[10.0, 20.0], 15.0), Some(20.0));
        assert_eq!(nearest_stop(&[], 15.0), None);
        let ramp: PaletteRamp = [(0.0, "#000000"), (50.0, "#777777"), (100.0, "#ffffff")]
            .into_iter()
            .map(|(s, c)| (s, c.to_string()))
            .collect();
        assert_eq!(nearest_shade(&ramp, "#ffffff"), Ok(Some(50.0)));
        let ends: PaletteRamp = [(0.0, "#000000"), (100.0, "#ffffff")]
            .into_iter()
            .map(|(s, c)| (s, c.to_string()))
            .collect();
        assert_eq!(nearest_shade(&ends, "#777777"), Ok(None));
        let named = |stop: f64, family: Option<&str>| {
            text_name(&StopRef {
                stop,
                family: family.map(str::to_string),
                ..StopRef::default()
            })
        };
        assert_eq!(named(95.0, Some("neutral")), "white");
        assert_eq!(named(10.0, Some("neutral")), "near-black");
        assert_eq!(named(100.0, Some("white")), "white");
        assert_eq!(named(0.0, Some("black")), "near-black");
        assert_eq!(named(16.5, Some("neutral")), "shade 16.5");
        assert_eq!(named(40.0, None), "shade 40");
        assert_eq!(is_gray("#6b6b70"), Ok(true));
        assert_eq!(is_gray("#5b3cc4"), Ok(false));
        assert!(same_color("#FFF", "ffffff"));
        assert!(!same_color("red", "red"));
        assert_eq!(palette_token("accent"), "--color-accent");
        assert_eq!(palette_token("neutral"), "--color-background-surface");
        assert_eq!(palette_token("danger"), "--color-error");
        assert_eq!(palette_token("primary"), "--color-undefined");
        assert_eq!(
            split_top_comma("rgb(1, 2, 3), #fff, var(--a, b)"),
            ["rgb(1, 2, 3)", " #fff", " var(--a, b)"]
        );
        assert_eq!(split_top_comma("a)b,c"), ["a)b,c"]);
    }

    #[test]
    fn seed_stop_is_the_nearest_inner_light_stop_of_the_seeds_ramp() {
        // Values from mapping.ts run under Node 24.
        let none = PaletteConfig::default();
        assert_eq!(seed_stop("#5b3cc4", &none, FamilyKind::Chromatic), Ok(40.0));
        assert_eq!(seed_stop("#6b6b70", &none, FamilyKind::Neutral), Ok(45.0));
        let coarse = PaletteConfig {
            stops: Some(vec![0.0, 25.0, 50.0, 75.0, 100.0]),
            vibrancy: Some(80.0),
            ..PaletteConfig::default()
        };
        assert_eq!(
            seed_stop("#fde047", &coarse, FamilyKind::Chromatic),
            Ok(75.0)
        );
        assert_eq!(seed_stop("#ffffff", &none, FamilyKind::Neutral), Ok(95.0));
        let ends = PaletteConfig {
            stops: Some(vec![0.0, 100.0]),
            ..PaletteConfig::default()
        };
        assert_eq!(seed_stop("#123456", &ends, FamilyKind::Chromatic), Ok(0.0));
    }

    #[test]
    fn grown_grays_grow_a_neutral_ramp_from_a_gray() {
        // grownGrays('#445564', 'light') and ('#B0B5BB', 'dark') under Node 24.
        let light = grown_grays("#445564", PaletteMode::Light).unwrap();
        let expected = [
            "#000000", "#0b1218", "#131d25", "#1c2731", "#25323d", "#2f3d4a", "#394956", "#445563",
            "#506170", "#5b6d7d", "#687989", "#748696", "#8193a3", "#8ea0b0", "#9caebd", "#aabbca",
            "#b9c9d7", "#c8d7e4", "#d8e4f0", "#e8f2fb", "#ffffff",
        ];
        assert_eq!(light.iter().map(|(_, c)| c).collect::<Vec<_>>(), expected);
        let dark = grown_grays("#B0B5BB", PaletteMode::Dark).unwrap();
        assert_eq!(dark.get(45.0), Some("#676b70"));
        assert_eq!(dark.get(95.0), Some("#eff1f4"));
        assert_eq!(grown_grays("not a color", PaletteMode::Light), None);
    }

    #[test]
    fn is_look_token_names_the_colors_the_look_derives() {
        for token in [
            "--color-background-red",
            "--color-text-gray",
            "--color-icon-teal",
            "--color-border-pink",
            "--color-data-categorical-blue",
            "--color-data-blue-3",
            "--color-syntax-keyword",
            "--color-syntax-background",
            "--color-overlay",
        ] {
            assert!(is_look_token(token), "{token}");
        }
        for token in [
            "--color-background-body",
            "--color-text-secondary",
            "--color-data-",
            "--color-data-Blue",
            "--color-syntax-added",
            "--color-overlay-hover",
            "--color-success",
        ] {
            assert!(!is_look_token(token), "{token}");
        }
    }

    #[test]
    fn dial_family_anchors_a_seed_at_its_stop() {
        let config = PaletteConfig::default();
        let accent = PaletteDial {
            id: "accent".into(),
            seed: "#5b3cc4".into(),
            only: None,
        };
        let (family, stop) = dial_family(&accent, &config, &PaletteMode::BOTH).unwrap();
        assert_eq!(stop, 40.0);
        assert_eq!(family.name.as_deref(), Some("Accent"));
        assert_eq!(family.kind, None);
        let anchors = family.anchors.unwrap();
        assert_eq!(anchors.len(), 1);
        assert_eq!(
            (anchors[0].mode, anchors[0].stop),
            (PaletteMode::Light, 40.0)
        );
        // White as the neutral holds no anchor; a gray accent grows as a
        // neutral.
        let white = PaletteDial {
            id: "neutral".into(),
            seed: "#ffffff".into(),
            only: None,
        };
        let (family, _) = dial_family(&white, &config, &PaletteMode::BOTH).unwrap();
        assert_eq!(family.anchors, None);
        assert_eq!(family.kind, Some(FamilyKind::Neutral));
        let gray = PaletteDial {
            id: "accent".into(),
            seed: "#777777".into(),
            only: None,
        };
        assert_eq!(
            dial_family(&gray, &config, &PaletteMode::BOTH)
                .unwrap()
                .0
                .kind,
            Some(FamilyKind::Neutral)
        );
    }

    #[test]
    fn plan_palette_request_is_none_without_a_setting() {
        let reader = CoreReader;
        let context = PaletteMappingContext::new(&has_default, &reader);
        assert_eq!(
            plan_palette_request(&[], PalettePlanOptions::default(), &context),
            Ok(None)
        );
    }
}
