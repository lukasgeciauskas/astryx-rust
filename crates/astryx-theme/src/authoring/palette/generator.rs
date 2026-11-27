//! The tonal palette engine: upstream `palette/generator.ts`.
//!
//! [`generate_tonal_palette`] grows each requested family into one ramp per
//! mode, stop by stop, in OKLCH: a stop's tone sets its OKLab lightness, the
//! seed's hue (darkened a little for warm hues at low tones) its hue, and the
//! seed's chroma, drawn toward a shared chroma so families sit together, is
//! shaped by the vibrancy, the hue, the tone and the mode, then cut back into
//! sRGB. Anchors pin colors at stops; their correction spreads to the
//! neighboring stops along a smoothstep, so the ramp stays smooth and in
//! order. The output is deterministic: the same request gives the same
//! `#rrggbb` strings upstream writes. [`generate_palette_set`] keeps the
//! per-ramp and cross-family diagnostics behind them.

use std::f64::consts::PI;

use super::super::color::{
    ColorTriple, InvalidColor, hex_to_oklch, luminance, max_oklch_chroma, normalize_color,
    oklab_coordinates, oklab_coordinates_to_hex, oklch_clamped_hex, tone_to_oklab_lightness,
};
use super::super::js::{is_js_whitespace, js_max, js_min, js_number_to_string, js_trim};
use super::types::{
    AnchorPolicy, FamilyKind, HueIdentityRisk, ModeStrategy, NeutralProfile, PaletteError,
    PaletteMode, PaletteRamp, PaletteRecipe, TonalPaletteAnchor, TonalPaletteAnchorResult,
    TonalPaletteCandidate, TonalPaletteCoordinationDiagnostics, TonalPaletteDarkChromaTaper,
    TonalPaletteFamily, TonalPaletteGenerationInput, TonalPaletteNormalizedDarkChromaTaper,
    TonalPaletteNormalizedFamily, TonalPaletteNormalizedRequest, TonalPaletteRampDiagnostics,
};

/// The recipe a request that names none runs: the first one. Upstream
/// `PALETTE_RECIPE` (`astryx-oklch-v1`).
pub const PALETTE_RECIPE: PaletteRecipe = PaletteRecipe::AstryxOklchV1;
/// The recipe whose pale stops follow the vibrancy and the seed's own chroma:
/// a stop whose gamut cannot hold the most vivid request gives every request
/// the same share of it, and coordination never raises a seed's chroma. A
/// request runs it by naming it. Upstream `PALETTE_RECIPE_V2`
/// (`astryx-oklch-v2`).
pub const PALETTE_RECIPE_V2: PaletteRecipe = PaletteRecipe::AstryxOklchV2;
/// Every recipe the generator runs. Upstream `PALETTE_RECIPES`.
pub const PALETTE_RECIPES: [PaletteRecipe; 2] = [PALETTE_RECIPE, PALETTE_RECIPE_V2];
/// Identity of this generator implementation. [`PALETTE_RECIPE`] names the
/// normative algorithm; this names the code that ran it, so a receipt can
/// tell two implementations of one recipe apart. Upstream
/// `PALETTE_GENERATOR_VERSION`; this port reproduces that implementation's
/// output, so it carries the same identity.
pub const PALETTE_GENERATOR_VERSION: &str = "palette-generator-v1";
/// Exact solid black. Upstream `PALETTE_BLACK`.
pub const PALETTE_BLACK: &str = "#000000";
/// Exact solid white. Upstream `PALETTE_WHITE`.
pub const PALETTE_WHITE: &str = "#ffffff";
/// The default stops, 0 through 100 in steps of 5. Upstream
/// `DEFAULT_21_STOPS`.
pub const DEFAULT_21_STOPS: [f64; 21] = [
    0.0, 5.0, 10.0, 15.0, 20.0, 25.0, 30.0, 35.0, 40.0, 45.0, 50.0, 55.0, 60.0, 65.0, 70.0, 75.0,
    80.0, 85.0, 90.0, 95.0, 100.0,
];
/// The compact stops, 0 through 100 in steps of 10. Upstream
/// `COMPACT_11_STOPS`.
pub const COMPACT_11_STOPS: [f64; 11] = [
    0.0, 10.0, 20.0, 30.0, 40.0, 50.0, 60.0, 70.0, 80.0, 90.0, 100.0,
];

/// The fields the generator reads from a request (see
/// [`TonalPaletteGenerationInput`]); any other field changes nothing.
/// Upstream `GENERATION_INPUT_KEYS`, for a reader of a JSON config.
pub const GENERATION_INPUT_KEYS: [&str; 7] = [
    "recipe",
    "vibrancy",
    "neutralProfile",
    "modeStrategy",
    "darkChromaTaper",
    "stops",
    "families",
];
/// The fields a dark chroma taper has. Upstream refuses a taper with any
/// other (`darkChromaTaper does not read …`), since the receipt records it;
/// a reader of a JSON config checks against these. Upstream
/// `DARK_CHROMA_TAPER_KEYS`.
pub const DARK_CHROMA_TAPER_KEYS: [&str; 4] = [
    "edgeMultiplier",
    "throughStop",
    "recoverAtStop",
    "familyMultipliers",
];
/// The fields the generator reads from a family. Upstream
/// `FAMILY_INPUT_KEYS`.
pub const FAMILY_INPUT_KEYS: [&str; 5] = ["id", "name", "seed", "kind", "anchors"];
/// The fields the generator reads from an anchor. Upstream
/// `ANCHOR_INPUT_KEYS`.
pub const ANCHOR_INPUT_KEYS: [&str; 5] = ["mode", "stop", "color", "policy", "maxDeltaE"];

const DARK_CHROMA_FACTOR: f64 = 0.85;
const RESERVED_FAMILY_IDS: [&str; 2] = ["black", "white"];

/// How narrow a stop's gamut is for v2 to share it: the most vivid seed at
/// the default vibrancy asks this many times what the stop holds. From the
/// first the stop starts to share its gamut in proportion; from the second
/// it shares it wholly.
const V2_SHARED_FROM: f64 = 1.3;
/// v2 shares only stops lighter than this tone: the pale side of a ramp.
const V2_SHARED_ABOVE: f64 = 50.0;
const V2_SHARED_FULL: f64 = 2.0;

const GRAY_FADE_FROM: f64 = 0.7;
const GRAY_FADE_TO: f64 = 0.97;
const GRAY_FADE_LEAST: f64 = 0.28;

/// `value` held to `minimum`–`maximum` (`Math.min(maximum, Math.max(minimum,
/// value))`). Upstream `clamp`.
fn clamp(value: f64, minimum: f64, maximum: f64) -> f64 {
    js_min(maximum, js_max(minimum, value))
}

/// A hue in degrees brought into 0–360. Upstream `normalizeHue`.
fn normalize_hue(hue: f64) -> f64 {
    ((hue % 360.0) + 360.0) % 360.0
}

/// The distance between two hues around the circle, 0 to 180 degrees.
/// Upstream `hueDistance`.
fn hue_distance(a: f64, b: f64) -> f64 {
    let difference = (normalize_hue(a) - normalize_hue(b)).abs();
    js_min(difference, 360.0 - difference)
}

/// The turn from one hue to another, -180 to 180 degrees. Upstream
/// `signedHueDelta`.
fn signed_hue_delta(from: f64, to: f64) -> f64 {
    ((normalize_hue(to) - normalize_hue(from) + 540.0) % 360.0) - 180.0
}

/// The hue `amount` of the way from `a` to `b` the short way round.
/// Upstream `interpolateHue`.
fn interpolate_hue(a: f64, b: f64, amount: f64) -> f64 {
    let delta = ((b - a + 540.0) % 360.0) - 180.0;
    normalize_hue(a + delta * amount)
}

/// The stops as given when they are a valid layout: at least one, each a
/// finite number from 0 to 100, strictly increasing. Upstream
/// `validateStops`.
pub fn validate_stops(stops: &[f64]) -> Result<Vec<f64>, PaletteError> {
    if stops.is_empty() {
        return Err(PaletteError::new("A palette requires at least one stop."));
    }
    for &stop in stops {
        if !stop.is_finite() || stop < 0.0 || stop > 100.0 {
            return Err(PaletteError(format!(
                "Stop {} must be a finite number from 0 to 100.",
                js_number_to_string(stop)
            )));
        }
    }
    if stops.windows(2).any(|pair| pair[1] <= pair[0]) {
        return Err(PaletteError::new(
            "Stops must be unique and strictly increasing.",
        ));
    }
    Ok(stops.to_vec())
}

/// JavaScript's `Number(string)`: the trimmed text as a decimal literal,
/// `Infinity` with an optional sign, or a `0x`/`0o`/`0b` integer; 0 for
/// empty text and NaN for anything else (`"12px"` is NaN, where `parseFloat`
/// reads 12).
fn js_string_to_number(text: &str) -> f64 {
    let text = text.trim_matches(is_js_whitespace);
    if text.is_empty() {
        return 0.0;
    }
    let radix = |prefix: [&str; 2], radix: u32| {
        let digits = text
            .strip_prefix(prefix[0])
            .or_else(|| text.strip_prefix(prefix[1]))?;
        if digits.is_empty() {
            return Some(f64::NAN);
        }
        let mut value = 0.0;
        for c in digits.chars() {
            value = value * f64::from(radix) + f64::from(c.to_digit(radix)?);
        }
        Some(value)
    };
    for (prefix, base) in [(["0x", "0X"], 16), (["0o", "0O"], 8), (["0b", "0B"], 2)] {
        if text.starts_with(prefix[0]) || text.starts_with(prefix[1]) {
            return radix(prefix, base).unwrap_or(f64::NAN);
        }
    }
    let unsigned = text
        .strip_prefix('+')
        .or_else(|| text.strip_prefix('-'))
        .unwrap_or(text);
    if unsigned == "Infinity" {
        return if text.starts_with('-') {
            f64::NEG_INFINITY
        } else {
            f64::INFINITY
        };
    }
    let bytes = unsigned.as_bytes();
    let digits = |from: usize| {
        let mut at = from;
        while at < bytes.len() && bytes[at].is_ascii_digit() {
            at += 1;
        }
        at
    };
    let whole = digits(0);
    let mut at = whole;
    let mut fraction = 0;
    if bytes.get(at) == Some(&b'.') {
        let end = digits(at + 1);
        fraction = end - (at + 1);
        at = end;
    }
    if whole + fraction == 0 {
        return f64::NAN;
    }
    if matches!(bytes.get(at), Some(b'e' | b'E')) {
        let mut exponent = at + 1;
        if matches!(bytes.get(exponent), Some(b'+' | b'-')) {
            exponent += 1;
        }
        let end = digits(exponent);
        if end == exponent {
            return f64::NAN;
        }
        at = end;
    }
    if at != bytes.len() {
        return f64::NAN;
    }
    text.parse().unwrap_or(f64::NAN)
}

/// A comma-separated stop list read into a validated layout (`"0, 50,
/// 100"`). Upstream `parseStopList`.
pub fn parse_stop_list(input: &str) -> Result<Vec<f64>, PaletteError> {
    let values: Vec<&str> = input.split(',').map(js_trim).collect();
    if values
        .iter()
        .any(|value| value.is_empty() || !js_string_to_number(value).is_finite())
    {
        return Err(PaletteError::new(
            "Every custom stop must be a number from 0 to 100.",
        ));
    }
    let stops: Vec<f64> = values
        .iter()
        .map(|value| js_string_to_number(value))
        .collect();
    validate_stops(&stops)
}

/// The modes a mode strategy generates, light first. Upstream
/// `modesForStrategy` (whose `Unknown mode strategy` error is
/// [`ModeStrategy::parse`]'s here).
pub fn modes_for_strategy(strategy: ModeStrategy) -> Vec<PaletteMode> {
    match strategy {
        ModeStrategy::LightOnly => vec![PaletteMode::Light],
        ModeStrategy::DarkOnly => vec![PaletteMode::Dark],
        ModeStrategy::LightAndDark => vec![PaletteMode::Light, PaletteMode::Dark],
    }
}

/// How much of its chroma a hue keeps, so families of every hue read as
/// equally strong: yellows, greens, cyans and purples give some up.
/// Upstream `hueBalanceFactor`.
fn hue_balance_factor(hue: f64) -> f64 {
    let normalized = normalize_hue(hue);
    if (70.0..115.0).contains(&normalized) {
        0.94
    } else if (115.0..175.0).contains(&normalized) {
        0.78
    } else if (175.0..230.0).contains(&normalized) {
        0.82
    } else if (285.0..340.0).contains(&normalized) {
        0.9
    } else {
        1.0
    }
}

/// The share of chroma greens and cyans keep above tone 60, falling along a
/// smoothstep to tone 95. Upstream `highToneChromaFactor`.
fn high_tone_chroma_factor(hue: f64, tone: f64) -> f64 {
    if tone <= 60.0 {
        return 1.0;
    }
    let taper = smoothstep((tone - 60.0) / 35.0);
    let normalized = normalize_hue(hue);
    if (115.0..175.0).contains(&normalized) {
        1.0 - 0.28 * taper
    } else if (175.0..200.0).contains(&normalized) {
        1.0 - 0.4 * taper
    } else {
        1.0
    }
}

/// The hue a stop takes: an orange (hue 40 to 70) turns up to 8 degrees
/// toward red below tone 50, so its dark stops do not read as brown.
/// Upstream `toneAdjustedHue`.
fn tone_adjusted_hue(hue: f64, tone: f64) -> f64 {
    let normalized = normalize_hue(hue);
    if !(40.0..70.0).contains(&normalized) || tone >= 50.0 {
        return normalized;
    }
    let darkening = clamp((50.0 - tone) / 40.0, 0.0, 1.0);
    normalize_hue(normalized - 8.0 * darkening.sqrt())
}

/// The share of chroma a tone carries: the most at the middle, 0.18 at the
/// ends. Upstream `toneChromaEnvelope`.
fn tone_chroma_envelope(tone: f64) -> f64 {
    let normalized = clamp(tone / 100.0, 0.0, 1.0);
    0.18 + 0.82 * js_max(0.0, (PI * normalized).sin()).sqrt()
}

/// The chroma multiplier of a vibrancy: 0 at 0, 0.72 at 25, 1 at 50 and 1.48
/// at 100. Upstream `vibrancyMultiplier`.
fn vibrancy_multiplier(vibrancy: f64) -> Result<f64, PaletteError> {
    if !vibrancy.is_finite() || !(0.0..=100.0).contains(&vibrancy) {
        return Err(PaletteError::new(
            "Vibrancy must be a number from 0 to 100.",
        ));
    }
    Ok(if vibrancy <= 25.0 {
        (vibrancy / 25.0) * 0.72
    } else if vibrancy <= 50.0 {
        0.72 + ((vibrancy - 25.0) / 25.0) * 0.28
    } else {
        1.0 + (vibrancy - 50.0) * 0.0096
    })
}

/// A family's chroma drawn toward the shared one, so its families sit
/// together. As v2 shares a narrow stop's gamut it is never raised above the
/// seed's own, so a dusty seed stays dusty in its pale stops. Upstream
/// `coordinatedChroma`.
fn coordinated_chroma(source_chroma: f64, recipe: PaletteRecipe) -> f64 {
    let drawn = source_chroma * 0.35 + 0.18 * 0.65;
    if recipe == PALETTE_RECIPE_V2 {
        js_min(source_chroma, drawn)
    } else {
        drawn
    }
}

/// The share of its chroma a v2 gray family keeps at an OKLab lightness: all
/// of it up to 0.7, then falling linearly to 0.28 at 0.97, as gray scales
/// fade their tint toward white. Upstream `grayFade`.
fn gray_fade(lightness: f64) -> f64 {
    let t = clamp(
        (lightness - GRAY_FADE_FROM) / (GRAY_FADE_TO - GRAY_FADE_FROM),
        0.0,
        1.0,
    );
    1.0 - (1.0 - GRAY_FADE_LEAST) * t
}

/// The chroma of the most vivid seed sRGB holds, as v2 coordinates it.
/// Upstream `MOST_VIVID_CHROMA`.
fn most_vivid_chroma() -> f64 {
    coordinated_chroma(0.32, PALETTE_RECIPE_V2)
}

/// An OKLCH color: OKLab lightness, chroma and hue in degrees. Upstream
/// `PolarColor`.
#[derive(Clone, Copy, Debug, PartialEq)]
struct PolarColor {
    lightness: f64,
    chroma: f64,
    hue: f64,
}

/// A hex color as OKLCH. Upstream `colorToPolar`.
fn color_to_polar(color: &str) -> Result<PolarColor, InvalidColor> {
    let value = hex_to_oklch(color)?;
    Ok(PolarColor {
        lightness: value.l,
        chroma: value.c,
        hue: value.h,
    })
}

/// The color a neutral family grows from: its seed under `custom`, else the
/// profile's own gray (achromatic, or a 0.018 tint at hue 75 or 250).
/// Upstream `neutralPolar`.
fn neutral_polar(profile: NeutralProfile, seed: &str) -> Result<PolarColor, InvalidColor> {
    let hue = match profile {
        NeutralProfile::Custom => return color_to_polar(seed),
        NeutralProfile::WarmV1 => 75.0,
        NeutralProfile::CoolV1 => 250.0,
        NeutralProfile::NeutralV1 => 0.0,
    };
    Ok(PolarColor {
        lightness: 0.5,
        chroma: if profile == NeutralProfile::NeutralV1 {
            0.0
        } else {
            0.018
        },
        hue,
    })
}

/// The color a stop grows from: the seed without anchors, else the anchor
/// colors interpolated in OKLCH between the anchors around the stop (the
/// nearest one beyond the first or last). Upstream `anchorPolarAtStop`.
fn anchor_polar_at_stop(
    seed: PolarColor,
    anchors: &[&TonalPaletteAnchor],
    stop: f64,
) -> Result<PolarColor, InvalidColor> {
    if anchors.is_empty() {
        return Ok(seed);
    }
    let mut sorted = anchors.to_vec();
    sorted.sort_by(|a, b| compare(a.stop - b.stop));
    let lower = sorted.iter().rev().find(|anchor| anchor.stop <= stop);
    let upper = sorted.iter().find(|anchor| anchor.stop >= stop);
    let (lower, upper) = match (lower, upper) {
        (Some(lower), Some(upper)) if lower.stop != upper.stop => (lower, upper),
        _ => {
            let anchor = lower.or(upper).unwrap_or(&sorted[0]);
            return color_to_polar(&normalize_color(&anchor.color)?);
        }
    };
    let start = color_to_polar(&normalize_color(&lower.color)?)?;
    let end = color_to_polar(&normalize_color(&upper.color)?)?;
    let amount = (stop - lower.stop) / (upper.stop - lower.stop);
    Ok(PolarColor {
        lightness: start.lightness + (end.lightness - start.lightness) * amount,
        chroma: start.chroma + (end.chroma - start.chroma) * amount,
        hue: interpolate_hue(start.hue, end.hue, amount),
    })
}

/// A JavaScript sort comparator's number as an ordering: below 0 is less,
/// above 0 greater, and 0 (or NaN) equal.
pub(crate) fn compare(value: f64) -> std::cmp::Ordering {
    if value < 0.0 {
        std::cmp::Ordering::Less
    } else if value > 0.0 {
        std::cmp::Ordering::Greater
    } else {
        std::cmp::Ordering::Equal
    }
}

/// One stop's generated color, and whether sRGB could not hold its chroma.
struct Candidate {
    hex: String,
    gamut_mapped: bool,
}

/// The color of one stop: the source color's hue at the stop's tone, its
/// chroma (drawn toward the shared one for a chromatic family) shaped by the
/// vibrancy, the hue, the tone and the mode, v2's gray fade and shared pale
/// gamut, then the taper's `chroma_multiplier` (the share of its in-gamut
/// chroma the stop keeps), cut into sRGB. Upstream `generateCandidate`.
fn generate_candidate(
    source: PolarColor,
    tone: f64,
    mode: PaletteMode,
    vibrancy: f64,
    coordinate_with_other_families: bool,
    chroma_multiplier: f64,
    recipe: PaletteRecipe,
) -> Result<Candidate, PaletteError> {
    let adjusted_tone = tone;
    let adjusted_hue = tone_adjusted_hue(source.hue, adjusted_tone);
    let dark_factor = if mode == PaletteMode::Dark {
        DARK_CHROMA_FACTOR
    } else {
        1.0
    };
    // Every recipe starts from v1's chroma, so a stop v2 does not share is
    // drawn exactly as v1 draws it.
    let base_chroma = if coordinate_with_other_families {
        coordinated_chroma(source.chroma, PALETTE_RECIPE)
    } else {
        source.chroma
    };
    let mut chroma = base_chroma
        * vibrancy_multiplier(vibrancy)?
        * hue_balance_factor(adjusted_hue)
        * high_tone_chroma_factor(adjusted_hue, adjusted_tone)
        * tone_chroma_envelope(adjusted_tone)
        * dark_factor;
    let lightness = tone_to_oklab_lightness(adjusted_tone);
    let max_chroma = max_oklch_chroma(adjusted_hue, lightness);
    // v2: a gray family's color fades toward white the way a gray scale's
    // does, so its pale stops carry only a faint tint of its hue.
    if recipe == PALETTE_RECIPE_V2 && !coordinate_with_other_families {
        chroma *= gray_fade(lightness);
    }
    // v2, on the pale side of a ramp: the narrower a stop's gamut is for the
    // most vivid seed at the default vibrancy, the more every request shares
    // that gamut in proportion, so such a stop follows the seed's chroma and
    // the vibrancy instead of every request landing on the gamut's edge.
    if recipe == PALETTE_RECIPE_V2
        && coordinate_with_other_families
        && adjusted_tone > V2_SHARED_ABOVE
    {
        let shape = hue_balance_factor(adjusted_hue)
            * high_tone_chroma_factor(adjusted_hue, adjusted_tone)
            * tone_chroma_envelope(adjusted_tone)
            * dark_factor;
        let narrow =
            (most_vivid_chroma() * vibrancy_multiplier(50.0)? * shape) / js_max(max_chroma, 1e-6);
        let share = smoothstep((narrow - V2_SHARED_FROM) / (V2_SHARED_FULL - V2_SHARED_FROM));
        if share > 0.0 {
            let most = most_vivid_chroma() * vibrancy_multiplier(100.0)? * shape;
            let own =
                coordinated_chroma(source.chroma, recipe) * vibrancy_multiplier(vibrancy)? * shape;
            let shared = (own / most) * max_chroma;
            chroma = (1.0 - share) * js_min(chroma, max_chroma) + share * shared;
        }
    }
    // A taper scales the chroma the stop shows after gamut mapping, so a stop
    // the gamut already limits loses the same share as any other stop.
    let shown_chroma = if chroma_multiplier == 1.0 {
        chroma
    } else {
        js_min(chroma, max_chroma) * chroma_multiplier
    };
    Ok(Candidate {
        hex: oklch_clamped_hex(lightness, shown_chroma, adjusted_hue),
        gamut_mapped: chroma > max_chroma + 0.000001,
    })
}

/// The perceptual distance between two hex colors: their OKLab distance
/// times 100 (ΔE; under 2 reads as one color). Fails on a color
/// [`normalize_color`] refuses. Upstream `perceptualDelta`.
pub fn perceptual_delta(color_a: &str, color_b: &str) -> Result<f64, InvalidColor> {
    let [lightness_a, a_a, b_a] = oklab_coordinates(color_a)?;
    let [lightness_b, a_b, b_b] = oklab_coordinates(color_b)?;
    let (dl, da, db) = (lightness_a - lightness_b, a_a - a_b, b_a - b_b);
    Ok((dl * dl + da * da + db * db).sqrt() * 100.0)
}

/// The color `amount` of the way from `color_a` to `color_b` in OKLCH, cut
/// into sRGB. Upstream `interpolateColor`.
fn interpolate_color(color_a: &str, color_b: &str, amount: f64) -> Result<String, InvalidColor> {
    let start = hex_to_oklch(color_a)?;
    let end = hex_to_oklch(color_b)?;
    Ok(oklch_clamped_hex(
        start.l + (end.l - start.l) * amount,
        start.c + (end.c - start.c) * amount,
        interpolate_hue(start.h, end.h, amount),
    ))
}

/// An anchor's policy applied to a stop's generated color: the exact color,
/// the nearest blend toward it (18 rounds of bisection) within a bounded
/// anchor's `max_delta_e`, or a flexible anchor's 35% blend. Upstream
/// `applyAnchor`.
fn apply_anchor(
    candidate: &str,
    anchor: &TonalPaletteAnchor,
) -> Result<TonalPaletteAnchorResult, PaletteError> {
    let target = normalize_color(&anchor.color)?;
    let generated_color = match anchor.policy {
        AnchorPolicy::Exact => target.clone(),
        AnchorPolicy::Bounded => {
            let tolerance = anchor.max_delta_e.unwrap_or(f64::NAN);
            if !tolerance.is_finite() || tolerance < 0.0 {
                return Err(PaletteError(format!(
                    "Bounded anchor at stop {} requires a non-negative maxDeltaE.",
                    js_number_to_string(anchor.stop)
                )));
            }
            if perceptual_delta(candidate, &target)? > tolerance {
                let mut low = 0.0;
                let mut high = 1.0;
                for _ in 0..18 {
                    let amount = (low + high) / 2.0;
                    let mixed = interpolate_color(&target, candidate, amount)?;
                    if perceptual_delta(&mixed, &target)? <= tolerance {
                        low = amount;
                    } else {
                        high = amount;
                    }
                }
                interpolate_color(&target, candidate, low)?
            } else {
                candidate.to_string()
            }
        }
        AnchorPolicy::Flexible => interpolate_color(candidate, &target, 0.35)?,
    };
    let delta_e = perceptual_delta(&generated_color, &target)?;
    Ok(TonalPaletteAnchorResult {
        mode: anchor.mode,
        stop: anchor.stop,
        color: target,
        policy: anchor.policy,
        max_delta_e: anchor.max_delta_e,
        generated_color,
        delta_e,
    })
}

/// 0 to 1 along `3t² − 2t³`, `amount` clamped to 0–1 first. Upstream
/// `smoothstep`.
fn smoothstep(amount: f64) -> f64 {
    let value = clamp(amount, 0.0, 1.0);
    value * value * (3.0 - 2.0 * value)
}

/// The OKLab offset `amount` of the way from `start` to `end`. Upstream
/// `interpolateDelta`.
fn interpolate_delta(start: ColorTriple, end: ColorTriple, amount: f64) -> ColorTriple {
    [
        start[0] + (end[0] - start[0]) * amount,
        start[1] + (end[1] - start[1]) * amount,
        start[2] + (end[2] - start[2]) * amount,
    ]
}

/// An anchor's correction: its stop and the OKLab offset from the generated
/// color to the color its policy gave.
struct Correction {
    stop: f64,
    delta: ColorTriple,
    result: TonalPaletteAnchorResult,
}

/// The OKLab correction at a stop: interpolated along a smoothstep between
/// the anchors around it, fading out within 25 stops beyond the first and
/// last (never past 0 or 100). Upstream `correctionAtStop`.
fn correction_at_stop(corrections: &[Correction], stop: f64) -> ColorTriple {
    let zero: ColorTriple = [0.0, 0.0, 0.0];
    let (Some(first), Some(last)) = (corrections.first(), corrections.last()) else {
        return zero;
    };
    if stop <= first.stop {
        let boundary = js_max(0.0, first.stop - 25.0);
        if stop <= boundary && boundary != first.stop {
            return zero;
        }
        let amount = if boundary == first.stop {
            1.0
        } else {
            smoothstep((stop - boundary) / (first.stop - boundary))
        };
        return interpolate_delta(zero, first.delta, amount);
    }
    if stop >= last.stop {
        let boundary = js_min(100.0, last.stop + 25.0);
        if stop >= boundary && boundary != last.stop {
            return zero;
        }
        let amount = if boundary == last.stop {
            1.0
        } else {
            smoothstep((stop - last.stop) / (boundary - last.stop))
        };
        return interpolate_delta(last.delta, zero, amount);
    }
    let upper_index = corrections
        .iter()
        .position(|item| item.stop > stop)
        .unwrap_or(corrections.len() - 1);
    let lower = &corrections[upper_index - 1];
    let upper = &corrections[upper_index];
    interpolate_delta(
        lower.delta,
        upper.delta,
        smoothstep((stop - lower.stop) / (upper.stop - lower.stop)),
    )
}

/// A ramp with its anchors applied: each anchored stop takes the color its
/// policy gives, and every other stop moves by the correction there.
/// Returns the colors and each anchor's result, in stop order. Upstream
/// `applyAnchorCorrections`.
fn apply_anchor_corrections(
    colors: PaletteRamp,
    stops: &[f64],
    anchors: &[&TonalPaletteAnchor],
) -> Result<(PaletteRamp, Vec<TonalPaletteAnchorResult>), PaletteError> {
    if anchors.is_empty() {
        return Ok((colors, Vec::new()));
    }
    let color_at = |stop: f64| colors.get(stop).unwrap_or_default().to_string();
    let mut corrections = Vec::with_capacity(anchors.len());
    for anchor in anchors {
        let candidate = color_at(anchor.stop);
        let result = apply_anchor(&candidate, anchor)?;
        let base = oklab_coordinates(&candidate)?;
        let target = oklab_coordinates(&result.generated_color)?;
        corrections.push(Correction {
            stop: anchor.stop,
            delta: [
                target[0] - base[0],
                target[1] - base[1],
                target[2] - base[2],
            ],
            result,
        });
    }
    corrections.sort_by(|a, b| compare(a.stop - b.stop));
    let mut corrected = PaletteRamp::new();
    for &stop in stops {
        let base = oklab_coordinates(&color_at(stop))?;
        let delta = correction_at_stop(&corrections, stop);
        corrected.set(
            stop,
            oklab_coordinates_to_hex([base[0] + delta[0], base[1] + delta[1], base[2] + delta[2]]),
        );
    }
    for correction in &corrections {
        corrected.set(correction.stop, correction.result.generated_color.clone());
    }
    Ok((
        corrected,
        corrections.into_iter().map(|c| c.result).collect(),
    ))
}

/// Refuses a family's anchor set that names a stop outside the layout, puts
/// anything but exact black at 0 or white at 100, or pins one mode's stop
/// twice. Upstream `assertAnchorSet`.
fn assert_anchor_set(anchors: &[TonalPaletteAnchor], stops: &[f64]) -> Result<(), PaletteError> {
    let mut seen: Vec<(PaletteMode, f64)> = Vec::new();
    for anchor in anchors {
        let color = normalize_color(&anchor.color)?;
        let stop = js_number_to_string(anchor.stop);
        if !stops.contains(&anchor.stop) {
            return Err(PaletteError(format!(
                "Anchor stop {stop} is not present in the requested stop layout."
            )));
        }
        if (anchor.stop == 0.0 && color != PALETTE_BLACK)
            || (anchor.stop == 100.0 && color != PALETTE_WHITE)
        {
            return Err(PaletteError(format!(
                "Anchor stop {stop} is reserved for exact {}; use an interior stop for a tinted endpoint.",
                if anchor.stop == 0.0 { "black" } else { "white" }
            )));
        }
        if seen.contains(&(anchor.mode, anchor.stop)) {
            return Err(PaletteError(format!(
                "Duplicate anchor for {} stop {stop}.",
                anchor.mode
            )));
        }
        seen.push((anchor.mode, anchor.stop));
    }
    Ok(())
}

/// A ramp's diagnostics: whether luminance rises at every stop, the
/// smallest and largest gaps between neighbors, the largest hue drift from
/// the reference hue (over stops of chroma 0.015 or more) and the drift a
/// blue or yellow ramp risks. Upstream `buildDiagnostics`.
fn build_diagnostics(
    colors: &PaletteRamp,
    stops: &[f64],
    source_hue: f64,
    gamut_mapped_stops: Vec<f64>,
    anchors: Vec<TonalPaletteAnchorResult>,
) -> Result<TonalPaletteRampDiagnostics, PaletteError> {
    let color_at = |stop: f64| colors.get(stop).unwrap_or_default();
    let mut monotonic = true;
    let mut minimum_adjacent_delta_e = f64::INFINITY;
    let mut maximum_adjacent_delta_e: f64 = 0.0;
    let mut maximum_hue_drift: f64 = 0.0;
    let mut hue_identity_risk = None;
    for (index, &stop) in stops.iter().enumerate() {
        let color = color_at(stop);
        let oklch = hex_to_oklch(color)?;
        if oklch.c >= 0.015 {
            let drift = signed_hue_delta(source_hue, oklch.h);
            maximum_hue_drift = js_max(maximum_hue_drift, hue_distance(source_hue, oklch.h));
            if (230.0..285.0).contains(&source_hue) && drift > 12.0 {
                hue_identity_risk = Some(HueIdentityRisk::BlueToPurple);
            }
            if (70.0..115.0).contains(&source_hue) && drift < -12.0 {
                hue_identity_risk = Some(HueIdentityRisk::YellowToBrown);
            }
        }
        if index == 0 {
            continue;
        }
        let previous = color_at(stops[index - 1]);
        if luminance(color) + 0.000001 < luminance(previous) {
            monotonic = false;
        }
        let adjacent_delta = perceptual_delta(previous, color)?;
        minimum_adjacent_delta_e = js_min(minimum_adjacent_delta_e, adjacent_delta);
        maximum_adjacent_delta_e = js_max(maximum_adjacent_delta_e, adjacent_delta);
    }
    Ok(TonalPaletteRampDiagnostics {
        monotonic,
        minimum_adjacent_delta_e: if minimum_adjacent_delta_e == f64::INFINITY {
            0.0
        } else {
            minimum_adjacent_delta_e
        },
        maximum_adjacent_delta_e,
        maximum_hue_drift,
        hue_identity_risk,
        gamut_mapped_stops,
        anchors,
    })
}

/// The share of its chroma a stop keeps under the request's dark chroma
/// taper: the edge multiplier through `through_stop`, a smoothstep back to 1
/// at `recover_at_stop`, and 1 from there on. Light ramps, neutral families,
/// and the black and white endpoints keep all of it. Upstream
/// `darkChromaMultiplier`.
fn dark_chroma_multiplier(
    taper: Option<&TonalPaletteNormalizedDarkChromaTaper>,
    family: &TonalPaletteNormalizedFamily,
    mode: PaletteMode,
    stop: f64,
) -> f64 {
    let Some(taper) = taper else {
        return 1.0;
    };
    if mode != PaletteMode::Dark || family.kind == FamilyKind::Neutral {
        return 1.0;
    }
    if stop <= 0.0 || stop >= 100.0 || stop >= taper.recover_at_stop {
        return 1.0;
    }
    let edge = taper
        .family_multipliers
        .iter()
        .find(|(id, _)| *id == family.id)
        .map_or(taper.edge_multiplier, |(_, value)| *value);
    if stop <= taper.through_stop {
        return edge;
    }
    let recovery =
        smoothstep((stop - taper.through_stop) / (taper.recover_at_stop - taper.through_stop));
    edge + (1.0 - edge) * recovery
}

/// One family's ramp in one mode, with its diagnostics. Upstream
/// `GeneratedRamp`.
#[derive(Clone, Debug, PartialEq)]
pub struct GeneratedRamp {
    /// The color at each stop.
    pub colors: PaletteRamp,
    /// The ramp's evidence.
    pub diagnostics: TonalPaletteRampDiagnostics,
}

/// One generated family. Upstream `GeneratedFamily`.
#[derive(Clone, Debug, PartialEq)]
pub struct GeneratedFamily {
    /// The family id.
    pub id: String,
    /// The display name.
    pub name: String,
    /// The normalized seed.
    pub seed: String,
    /// The light ramp, when generated.
    pub light: Option<GeneratedRamp>,
    /// The dark ramp, when generated.
    pub dark: Option<GeneratedRamp>,
}

impl GeneratedFamily {
    /// The family's ramp in `mode` (upstream `family[mode]`).
    pub fn ramp(&self, mode: PaletteMode) -> Option<&GeneratedRamp> {
        match mode {
            PaletteMode::Light => self.light.as_ref(),
            PaletteMode::Dark => self.dark.as_ref(),
        }
    }
}

/// A family the generator could not grow, and why. Upstream's
/// `{familyId, message}`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PaletteFamilyError {
    /// The family id. Upstream `familyId`.
    pub family_id: String,
    /// The error message.
    pub message: String,
}

/// Everything one generator run produced. Upstream
/// `PaletteGenerationResult`.
#[derive(Clone, Debug, PartialEq)]
pub struct PaletteGenerationResult {
    /// The recipe that ran.
    pub recipe: PaletteRecipe,
    /// Always `candidate`.
    pub status: &'static str,
    /// The request with every default filled in.
    pub request: TonalPaletteNormalizedRequest,
    /// The families that grew, in request order.
    pub families: Vec<GeneratedFamily>,
    /// Cross-family evidence, one entry per mode.
    pub coordination: Vec<TonalPaletteCoordinationDiagnostics>,
    /// The families that did not grow.
    pub errors: Vec<PaletteFamilyError>,
}

/// One family's ramp in one mode: each stop grown from the seed (or the
/// anchor colors around it) and corrected toward the anchors, refused when
/// the anchors leave it out of luminance order. Upstream `generateRamp`.
fn generate_ramp(
    request: &TonalPaletteNormalizedRequest,
    family: &TonalPaletteNormalizedFamily,
    mode: PaletteMode,
) -> Result<GeneratedRamp, PaletteError> {
    let seed_hex = normalize_color(&family.seed)?;
    let anchors: Vec<&TonalPaletteAnchor> = family
        .anchors
        .iter()
        .filter(|anchor| anchor.mode == mode)
        .collect();
    assert_anchor_set(&family.anchors, &request.stops)?;
    let chromatic = family.kind != FamilyKind::Neutral;
    let seed = if chromatic {
        color_to_polar(&seed_hex)?
    } else {
        neutral_polar(request.neutral_profile, &seed_hex)?
    };
    let taper = request.dark_chroma_taper.as_ref();
    let diagnostic_reference = generate_candidate(
        seed,
        50.0,
        mode,
        request.vibrancy,
        chromatic,
        dark_chroma_multiplier(taper, family, mode, 50.0),
        request.recipe,
    )?
    .hex;
    let mut colors = PaletteRamp::new();
    let mut gamut_mapped_stops = Vec::new();
    for &stop in &request.stops {
        let source = anchor_polar_at_stop(seed, &anchors, stop)?;
        let generated = generate_candidate(
            source,
            stop,
            mode,
            request.vibrancy,
            chromatic,
            dark_chroma_multiplier(taper, family, mode, stop),
            request.recipe,
        )?;
        colors.set(stop, generated.hex);
        if generated.gamut_mapped {
            gamut_mapped_stops.push(stop);
        }
    }
    let (colors, anchor_results) = apply_anchor_corrections(colors, &request.stops, &anchors)?;
    let diagnostics = build_diagnostics(
        &colors,
        &request.stops,
        hex_to_oklch(&diagnostic_reference)?.h,
        gamut_mapped_stops,
        anchor_results,
    )?;
    if !diagnostics.monotonic {
        return Err(PaletteError(format!(
            "{} {mode} ramp is not luminance-monotonic with the requested anchors.",
            family.name
        )));
    }
    Ok(GeneratedRamp {
        colors,
        diagnostics,
    })
}

/// The stop nearest `target`; the first on a tie. Upstream `nearestStop`
/// (generator.ts).
fn nearest_stop(stops: &[f64], target: f64) -> f64 {
    stops.iter().skip(1).fold(stops[0], |best, &stop| {
        if (stop - target).abs() < (best - target).abs() {
            stop
        } else {
            best
        }
    })
}

/// Cross-family evidence per mode at the stop nearest 50: the two chromatic
/// families hardest to tell apart and the spread of their chroma. Upstream
/// `buildCoordinationDiagnostics`.
fn build_coordination_diagnostics(
    request: &TonalPaletteNormalizedRequest,
    families: &[GeneratedFamily],
) -> Result<Vec<TonalPaletteCoordinationDiagnostics>, PaletteError> {
    struct Sample<'a> {
        name: &'a str,
        color: &'a str,
        chroma: f64,
    }
    let stop = nearest_stop(&request.stops, 50.0);
    let chromatic = |id: &str| {
        request
            .families
            .iter()
            .any(|family| family.id == id && family.kind != FamilyKind::Neutral)
    };
    let mut out = Vec::new();
    for mode in modes_for_strategy(request.mode_strategy) {
        let mut samples = Vec::new();
        for family in families {
            let Some(ramp) = family.ramp(mode) else {
                continue;
            };
            if !chromatic(&family.id) {
                continue;
            }
            let color = ramp.colors.get(stop).unwrap_or_default();
            samples.push(Sample {
                name: &family.name,
                color,
                chroma: hex_to_oklch(color)?.c,
            });
        }
        let mut closest_families = None;
        let mut minimum_family_delta_e = f64::INFINITY;
        for index in 0..samples.len() {
            for next in index + 1..samples.len() {
                let delta = perceptual_delta(samples[index].color, samples[next].color)?;
                if delta < minimum_family_delta_e {
                    minimum_family_delta_e = delta;
                    closest_families = Some((
                        samples[index].name.to_string(),
                        samples[next].name.to_string(),
                    ));
                }
            }
        }
        let mut by_chroma: Vec<&Sample> = samples.iter().collect();
        by_chroma.sort_by(|a, b| compare(a.chroma - b.chroma));
        let weakest = by_chroma.first();
        let strongest = by_chroma.last();
        out.push(TonalPaletteCoordinationDiagnostics {
            mode,
            stop,
            closest_families,
            minimum_family_delta_e: if minimum_family_delta_e == f64::INFINITY {
                None
            } else {
                Some(minimum_family_delta_e)
            },
            strongest_family: strongest.map(|s| s.name.to_string()),
            weakest_family: weakest.map(|s| s.name.to_string()),
            chroma_ratio: match (weakest, strongest) {
                (Some(weakest), Some(strongest)) if weakest.chroma > 0.0001 => {
                    Some(strongest.chroma / weakest.chroma)
                }
                _ => None,
            },
        });
    }
    Ok(out)
}

/// `value` when it is a number from 0 to 1, else `<label> must be a number
/// from 0 to 1.` Upstream `assertMultiplier`.
fn assert_multiplier(value: f64, label: &str) -> Result<f64, PaletteError> {
    if !value.is_finite() || !(0.0..=1.0).contains(&value) {
        return Err(PaletteError(format!(
            "{label} must be a number from 0 to 1."
        )));
    }
    Ok(value)
}

/// The request's dark chroma taper, checked against its stops and families,
/// or `None` when the request has none. Upstream `normalizeDarkChromaTaper`.
fn normalize_dark_chroma_taper(
    taper: Option<&TonalPaletteDarkChromaTaper>,
    stops: &[f64],
    families: &[TonalPaletteNormalizedFamily],
) -> Result<Option<TonalPaletteNormalizedDarkChromaTaper>, PaletteError> {
    let Some(taper) = taper else {
        return Ok(None);
    };
    let edge = assert_multiplier(taper.edge_multiplier, "darkChromaTaper.edgeMultiplier")?;
    if !stops.contains(&taper.through_stop) {
        return Err(PaletteError::new(
            "darkChromaTaper.throughStop must be a stop in the requested stop layout.",
        ));
    }
    if !stops.contains(&taper.recover_at_stop) || taper.recover_at_stop <= taper.through_stop {
        return Err(PaletteError::new(
            "darkChromaTaper.recoverAtStop must be a stop in the requested stop layout after throughStop.",
        ));
    }
    let mut multipliers: Vec<(String, f64)> = Vec::new();
    for (id, value) in taper.family_multipliers.iter().flatten() {
        let Some(family) = families.iter().find(|candidate| candidate.id == *id) else {
            return Err(PaletteError(format!(
                "darkChromaTaper.familyMultipliers names unknown family {id}."
            )));
        };
        if family.kind == FamilyKind::Neutral {
            return Err(PaletteError(format!(
                "darkChromaTaper.familyMultipliers names neutral family {id}; the taper changes only chromatic families."
            )));
        }
        let value = assert_multiplier(*value, &format!("darkChromaTaper.familyMultipliers.{id}"))?;
        match multipliers.iter_mut().find(|(known, _)| known == id) {
            Some(entry) => entry.1 = value,
            None => multipliers.push((id.clone(), value)),
        }
    }
    Ok(Some(TonalPaletteNormalizedDarkChromaTaper {
        edge_multiplier: edge,
        through_stop: taper.through_stop,
        recover_at_stop: taper.recover_at_stop,
        family_multipliers: multipliers,
    }))
}

/// Whether a family id is lower kebab case: a lowercase letter, then
/// lowercase letters, digits and hyphens (`/^[a-z][a-z0-9-]*$/`).
fn is_kebab_id(id: &str) -> bool {
    let mut chars = id.chars();
    chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// The request checked and with every default filled in: the v1 recipe,
/// both modes, the 21 default stops, vibrancy 50, the `neutral-v1` profile,
/// and each family's trimmed id, name, normalized seed, kind and anchors.
/// Upstream `normalizeGenerationRequest`.
pub fn normalize_generation_request(
    input: &TonalPaletteGenerationInput,
) -> Result<TonalPaletteNormalizedRequest, PaletteError> {
    if input.families.is_empty() {
        return Err(PaletteError::new(
            "Palette generation requires at least one family.",
        ));
    }
    // A request that names no recipe runs the first one, so a config written
    // before v2 regenerates the same colors.
    let recipe = input.recipe.unwrap_or(PALETTE_RECIPE);
    let mode_strategy = input.mode_strategy.unwrap_or(ModeStrategy::LightAndDark);
    let modes = modes_for_strategy(mode_strategy);
    let stops = validate_stops(input.stops.as_deref().unwrap_or(&DEFAULT_21_STOPS))?;
    let mut ids: Vec<String> = Vec::new();
    let mut families = Vec::with_capacity(input.families.len());
    for (index, family) in input.families.iter().enumerate() {
        let id = js_trim(&family.id).to_string();
        if !is_kebab_id(&id) {
            return Err(PaletteError(format!(
                "Family {index} id must use lower-kebab-case characters."
            )));
        }
        if RESERVED_FAMILY_IDS.contains(&id.as_str()) {
            return Err(PaletteError(format!(
                "Family id {id} is reserved for the standalone {id} palette value."
            )));
        }
        if ids.contains(&id) {
            return Err(PaletteError(format!("Duplicate family id: {id}.")));
        }
        ids.push(id.clone());
        let kind = family.kind.unwrap_or(FamilyKind::Chromatic);
        let mut anchors = Vec::new();
        for anchor in family.anchors.iter().flatten() {
            let stop = js_number_to_string(anchor.stop);
            if !modes.contains(&anchor.mode) {
                return Err(PaletteError(format!(
                    "Anchor mode {} is not generated by mode strategy {mode_strategy}.",
                    anchor.mode
                )));
            }
            if !anchor.stop.is_finite() || !stops.contains(&anchor.stop) {
                return Err(PaletteError(format!(
                    "Anchor stop {stop} is not present in the requested stop layout."
                )));
            }
            let bounded = anchor.policy == AnchorPolicy::Bounded;
            if bounded
                && !anchor
                    .max_delta_e
                    .is_some_and(|limit| limit.is_finite() && limit >= 0.0)
            {
                return Err(PaletteError(format!(
                    "Bounded anchor at stop {stop} requires a non-negative maxDeltaE."
                )));
            }
            if !bounded && anchor.max_delta_e.is_some() {
                return Err(PaletteError(format!(
                    "Anchor policy {} does not accept maxDeltaE.",
                    anchor.policy
                )));
            }
            let color = normalize_color(&anchor.color)?;
            if (anchor.stop == 0.0 && color != PALETTE_BLACK)
                || (anchor.stop == 100.0 && color != PALETTE_WHITE)
            {
                return Err(PaletteError(format!(
                    "Anchor stop {stop} is reserved for exact {}; use an interior stop for a tinted endpoint.",
                    if anchor.stop == 0.0 { "black" } else { "white" }
                )));
            }
            anchors.push(TonalPaletteAnchor {
                mode: anchor.mode,
                stop: anchor.stop,
                color,
                policy: anchor.policy,
                max_delta_e: if bounded { anchor.max_delta_e } else { None },
            });
        }
        families.push(TonalPaletteNormalizedFamily {
            name: family.name.clone().unwrap_or_else(|| id.clone()),
            id,
            seed: normalize_color(&family.seed)?,
            kind,
            anchors,
        });
    }
    let neutral_profile = input.neutral_profile.unwrap_or(NeutralProfile::NeutralV1);
    let dark_chroma_taper =
        normalize_dark_chroma_taper(input.dark_chroma_taper.as_ref(), &stops, &families)?;
    let vibrancy = input.vibrancy.unwrap_or(50.0);
    vibrancy_multiplier(vibrancy)?;
    Ok(TonalPaletteNormalizedRequest {
        recipe,
        vibrancy,
        neutral_profile,
        mode_strategy,
        dark_chroma_taper,
        stops,
        families,
    })
}

/// Every family of a request grown into its ramps, with the diagnostics. A
/// request the generator refuses is an error; a family that cannot grow is
/// listed in `errors` and the rest still grow. Upstream
/// `generatePaletteSet`.
pub fn generate_palette_set(
    input: &TonalPaletteGenerationInput,
) -> Result<PaletteGenerationResult, PaletteError> {
    let request = normalize_generation_request(input)?;
    let mut families = Vec::new();
    let mut errors = Vec::new();
    for family in &request.families {
        let mut generated = GeneratedFamily {
            id: family.id.clone(),
            name: family.name.clone(),
            seed: family.seed.clone(),
            light: None,
            dark: None,
        };
        let grown = modes_for_strategy(request.mode_strategy)
            .into_iter()
            .try_for_each(|mode| {
                let ramp = generate_ramp(&request, family, mode)?;
                match mode {
                    PaletteMode::Light => generated.light = Some(ramp),
                    PaletteMode::Dark => generated.dark = Some(ramp),
                }
                Ok::<(), PaletteError>(())
            });
        match grown {
            Ok(()) => families.push(generated),
            Err(error) => errors.push(PaletteFamilyError {
                family_id: family.id.clone(),
                message: error.0,
            }),
        }
    }
    let coordination = build_coordination_diagnostics(&request, &families)?;
    Ok(PaletteGenerationResult {
        recipe: request.recipe,
        status: "candidate",
        request,
        families,
        coordination,
        errors,
    })
}

/// A candidate palette for a request, without reading or writing files: each
/// family's ramp per mode, and exact black and white. Any family that cannot
/// grow fails the whole request, one `<id>: <message>` line each. Upstream
/// `generateTonalPalette`.
pub fn generate_tonal_palette(
    input: &TonalPaletteGenerationInput,
) -> Result<TonalPaletteCandidate, PaletteError> {
    let result = generate_palette_set(input)?;
    if !result.errors.is_empty() {
        return Err(PaletteError(
            result
                .errors
                .iter()
                .map(|error| format!("{}: {}", error.family_id, error.message))
                .collect::<Vec<_>>()
                .join("\n"),
        ));
    }
    Ok(TonalPaletteCandidate {
        schema_version: 1,
        status: "candidate",
        recipe: result.recipe,
        black: PALETTE_BLACK,
        white: PALETTE_WHITE,
        stops: result.request.stops.clone(),
        palette: result
            .families
            .into_iter()
            .map(|family| TonalPaletteFamily {
                id: family.id,
                name: family.name,
                light: family.light.map(|ramp| ramp.colors),
                dark: family.dark.map(|ramp| ramp.colors),
            })
            .collect(),
    })
}

/// A small JSON reader for the tests, which read upstream's exported palettes
/// and theme definitions from `spec/` (the crate has no JSON dependency
/// outside its `json` feature).
#[cfg(test)]
pub(crate) mod spec_json {
    use super::super::types::{
        AnchorPolicy, FamilyKind, ModeStrategy, NeutralProfile, PaletteMode, PaletteRecipe,
        TonalPaletteAnchor, TonalPaletteDarkChromaTaper, TonalPaletteFamilyInput,
        TonalPaletteGenerationInput,
    };

    /// A JSON value; an object keeps its keys in document order.
    #[derive(Clone, Debug, PartialEq)]
    pub(crate) enum Json {
        Null,
        Bool(bool),
        Number(f64),
        String(String),
        Array(Vec<Json>),
        Object(Vec<(String, Json)>),
    }

    impl Json {
        /// The document's value; panics on malformed JSON.
        pub(crate) fn parse(text: &str) -> Json {
            let mut reader = Reader {
                bytes: text.as_bytes(),
                at: 0,
            };
            let value = reader.value();
            reader.space();
            assert_eq!(reader.at, reader.bytes.len(), "trailing JSON");
            value
        }

        pub(crate) fn get(&self, key: &str) -> Option<&Json> {
            match self {
                Json::Object(entries) => entries.iter().find(|(k, _)| k == key).map(|(_, v)| v),
                _ => None,
            }
        }

        pub(crate) fn str(&self) -> &str {
            match self {
                Json::String(s) => s,
                other => panic!("not a string: {other:?}"),
            }
        }

        pub(crate) fn num(&self) -> f64 {
            match self {
                Json::Number(n) => *n,
                other => panic!("not a number: {other:?}"),
            }
        }

        pub(crate) fn arr(&self) -> &[Json] {
            match self {
                Json::Array(items) => items,
                other => panic!("not an array: {other:?}"),
            }
        }

        pub(crate) fn obj(&self) -> &[(String, Json)] {
            match self {
                Json::Object(entries) => entries,
                other => panic!("not an object: {other:?}"),
            }
        }
    }

    struct Reader<'a> {
        bytes: &'a [u8],
        at: usize,
    }

    impl Reader<'_> {
        fn space(&mut self) {
            while self.at < self.bytes.len() && self.bytes[self.at].is_ascii_whitespace() {
                self.at += 1;
            }
        }

        fn eat(&mut self, byte: u8) {
            self.space();
            assert_eq!(self.bytes[self.at], byte, "at {}", self.at);
            self.at += 1;
        }

        fn value(&mut self) -> Json {
            self.space();
            match self.bytes[self.at] {
                b'{' => {
                    self.at += 1;
                    let mut entries = Vec::new();
                    self.space();
                    if self.bytes[self.at] == b'}' {
                        self.at += 1;
                        return Json::Object(entries);
                    }
                    loop {
                        self.space();
                        let key = self.string();
                        self.eat(b':');
                        entries.push((key, self.value()));
                        self.space();
                        self.at += 1;
                        if self.bytes[self.at - 1] == b'}' {
                            return Json::Object(entries);
                        }
                    }
                }
                b'[' => {
                    self.at += 1;
                    let mut items = Vec::new();
                    self.space();
                    if self.bytes[self.at] == b']' {
                        self.at += 1;
                        return Json::Array(items);
                    }
                    loop {
                        items.push(self.value());
                        self.space();
                        self.at += 1;
                        if self.bytes[self.at - 1] == b']' {
                            return Json::Array(items);
                        }
                    }
                }
                b'"' => Json::String(self.string()),
                b't' => {
                    self.at += 4;
                    Json::Bool(true)
                }
                b'f' => {
                    self.at += 5;
                    Json::Bool(false)
                }
                b'n' => {
                    self.at += 4;
                    Json::Null
                }
                _ => {
                    let start = self.at;
                    while self.at < self.bytes.len()
                        && matches!(
                            self.bytes[self.at],
                            b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9'
                        )
                    {
                        self.at += 1;
                    }
                    let text = std::str::from_utf8(&self.bytes[start..self.at]).unwrap();
                    Json::Number(text.parse().unwrap())
                }
            }
        }

        fn string(&mut self) -> String {
            assert_eq!(self.bytes[self.at], b'"');
            self.at += 1;
            let mut units: Vec<u16> = Vec::new();
            loop {
                let byte = self.bytes[self.at];
                match byte {
                    b'"' => {
                        self.at += 1;
                        return String::from_utf16(&units).unwrap();
                    }
                    b'\\' => {
                        let escape = self.bytes[self.at + 1];
                        self.at += 2;
                        let unit = match escape {
                            b'n' => u16::from(b'\n'),
                            b't' => u16::from(b'\t'),
                            b'r' => u16::from(b'\r'),
                            b'b' => 8,
                            b'f' => 12,
                            b'u' => {
                                let hex = std::str::from_utf8(&self.bytes[self.at..self.at + 4]);
                                self.at += 4;
                                u16::from_str_radix(hex.unwrap(), 16).unwrap()
                            }
                            other => u16::from(other),
                        };
                        units.push(unit);
                    }
                    _ => {
                        let rest = std::str::from_utf8(&self.bytes[self.at..]).unwrap();
                        let c = rest.chars().next().unwrap();
                        let mut buffer = [0u16; 2];
                        units.extend_from_slice(c.encode_utf16(&mut buffer));
                        self.at += c.len_utf8();
                    }
                }
            }
        }
    }

    /// A generation request written as upstream's JSON.
    pub(crate) fn request_from_json(json: &Json) -> TonalPaletteGenerationInput {
        let anchor = |a: &Json| TonalPaletteAnchor {
            mode: PaletteMode::parse(a.get("mode").unwrap().str()).unwrap(),
            stop: a.get("stop").unwrap().num(),
            color: a.get("color").unwrap().str().to_string(),
            policy: AnchorPolicy::parse(a.get("policy").unwrap().str()).unwrap(),
            max_delta_e: a.get("maxDeltaE").map(Json::num),
        };
        let family = |f: &Json| TonalPaletteFamilyInput {
            id: f.get("id").unwrap().str().to_string(),
            seed: f.get("seed").unwrap().str().to_string(),
            name: f.get("name").map(|n| n.str().to_string()),
            kind: f.get("kind").map(|k| FamilyKind::parse(k.str()).unwrap()),
            anchors: f
                .get("anchors")
                .map(|a| a.arr().iter().map(anchor).collect()),
        };
        TonalPaletteGenerationInput {
            recipe: json
                .get("recipe")
                .map(|r| PaletteRecipe::parse(r.str()).unwrap()),
            families: json
                .get("families")
                .unwrap()
                .arr()
                .iter()
                .map(family)
                .collect(),
            vibrancy: json.get("vibrancy").map(Json::num),
            neutral_profile: json
                .get("neutralProfile")
                .map(|p| NeutralProfile::parse(p.str()).unwrap()),
            mode_strategy: json
                .get("modeStrategy")
                .map(|m| ModeStrategy::parse(m.str()).unwrap()),
            dark_chroma_taper: json
                .get("darkChromaTaper")
                .map(|t| TonalPaletteDarkChromaTaper {
                    edge_multiplier: t.get("edgeMultiplier").unwrap().num(),
                    through_stop: t.get("throughStop").unwrap().num(),
                    recover_at_stop: t.get("recoverAtStop").unwrap().num(),
                    family_multipliers: t.get("familyMultipliers").map(|m| {
                        m.obj()
                            .iter()
                            .map(|(id, value)| (id.clone(), value.num()))
                            .collect()
                    }),
                }),
            stops: json
                .get("stops")
                .map(|s| s.arr().iter().map(Json::num).collect()),
        }
    }

    /// The exported palette of an official theme, `spec/palettes/<theme>.json`.
    pub(crate) fn spec_palette(theme: &str) -> Json {
        let text = match theme {
            "butter" => include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../spec/palettes/butter.json"
            )),
            "gothic" => include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../spec/palettes/gothic.json"
            )),
            "matcha" => include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../spec/palettes/matcha.json"
            )),
            "neutral" => include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../spec/palettes/neutral.json"
            )),
            "stone" => include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../spec/palettes/stone.json"
            )),
            "y2k" => include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../spec/palettes/y2k.json"
            )),
            other => panic!("no palette for {other}"),
        };
        Json::parse(text)
    }

    /// An official theme's definition, `spec/theme-definitions/<theme>.json`.
    pub(crate) fn spec_theme_definition(theme: &str) -> Json {
        let text = match theme {
            "butter" => include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../spec/theme-definitions/butter.json"
            )),
            "neutral" => include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../spec/theme-definitions/neutral.json"
            )),
            "y2k" => include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../spec/theme-definitions/y2k.json"
            )),
            other => panic!("no theme definition for {other}"),
        };
        Json::parse(text)
    }
}

#[cfg(test)]
mod tests {
    use super::spec_json::{Json, request_from_json, spec_palette};
    use super::*;

    /// The official themes whose palettes upstream generates.
    const THEMES: [&str; 6] = ["butter", "gothic", "matcha", "neutral", "stone", "y2k"];

    /// A theme's palette, regenerated from the request its receipt records.
    fn regenerated(theme: &str) -> TonalPaletteCandidate {
        let spec = spec_palette(theme);
        generate_tonal_palette(&request_from_json(spec.get("request").unwrap()))
            .unwrap_or_else(|e| panic!("{theme}: {e}"))
    }

    /// A family's color at a stop of a mode.
    fn color(palette: &TonalPaletteCandidate, id: &str, mode: PaletteMode, stop: f64) -> String {
        palette
            .ramp(id, mode)
            .and_then(|ramp| ramp.get(stop))
            .unwrap_or_else(|| panic!("no {id} {mode} {stop}"))
            .to_string()
    }

    #[test]
    fn regenerates_every_official_palette_exactly() {
        // The decisive check: each theme's receipt request, run here, gives
        // the palette upstream's generator wrote, every family, mode and stop.
        let mut mismatches = Vec::new();
        for theme in THEMES {
            let spec = spec_palette(theme);
            let candidate = regenerated(theme);
            assert_eq!(
                candidate.recipe.as_str(),
                spec.get("recipe").unwrap().str(),
                "{theme}"
            );
            assert_eq!(
                PALETTE_GENERATOR_VERSION,
                spec.get("generatorVersion").unwrap().str(),
                "{theme}"
            );
            assert_eq!(candidate.black, spec.get("black").unwrap().str());
            assert_eq!(candidate.white, spec.get("white").unwrap().str());
            let expected = spec.get("palette").unwrap().obj();
            assert_eq!(candidate.palette.len(), expected.len(), "{theme} families");
            for (id, family) in expected {
                let ours = candidate
                    .family(id)
                    .unwrap_or_else(|| panic!("{theme}: no family {id}"));
                assert_eq!(ours.name, family.get("name").unwrap().str(), "{theme} {id}");
                for mode in PaletteMode::BOTH {
                    let Some(stops) = family.get(mode.as_str()) else {
                        assert!(ours.ramp(mode).is_none(), "{theme} {id} {mode}");
                        continue;
                    };
                    let ramp = ours.ramp(mode).unwrap();
                    assert_eq!(ramp.len(), stops.obj().len(), "{theme} {id} {mode}");
                    for (stop, hex) in stops.obj() {
                        let got = ramp.get(stop.parse().unwrap()).unwrap_or("missing");
                        if got != hex.str() {
                            mismatches.push(format!(
                                "{theme} {id} {mode} {stop}: {got} != {}",
                                hex.str()
                            ));
                        }
                    }
                }
            }
        }
        assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
    }

    /// The relative luminance the themes' palette tests compute.
    fn relative_luminance(hex: &str) -> f64 {
        let channel = |at: usize| {
            let value = f64::from(u8::from_str_radix(&hex[at..at + 2], 16).unwrap()) / 255.0;
            if value <= 0.04045 {
                value / 12.92
            } else {
                ((value + 0.055) / 1.055).powf(2.4)
            }
        };
        channel(1) * 0.2126 + channel(3) * 0.7152 + channel(5) * 0.0722
    }

    fn is_hex6(value: &str) -> bool {
        value.len() == 7
            && value.starts_with('#')
            && value[1..].bytes().all(|b| b.is_ascii_hexdigit())
    }

    /// Every family of a palette, in both modes, is a complete ramp from
    /// black to white whose luminance rises at every stop.
    fn assert_complete_monotonic(palette: &TonalPaletteCandidate, families: &[&str]) {
        let mut ids: Vec<&str> = palette.palette.iter().map(|f| f.id.as_str()).collect();
        let mut expected = families.to_vec();
        ids.sort_unstable();
        expected.sort_unstable();
        assert_eq!(ids, expected);
        for id in families {
            for mode in PaletteMode::BOTH {
                let values: Vec<String> = DEFAULT_21_STOPS
                    .iter()
                    .map(|&stop| color(palette, id, mode, stop))
                    .collect();
                assert!(values.iter().all(|v| is_hex6(v)), "{id} {mode}");
                assert_eq!(values[0], "#000000");
                assert_eq!(values[20], "#ffffff");
                let luminance: Vec<f64> = values.iter().map(|v| relative_luminance(v)).collect();
                assert!(
                    luminance.windows(2).all(|pair| pair[1] > pair[0]),
                    "{id} {mode} is not monotonic"
                );
            }
        }
    }

    #[test]
    fn butter_palette_contains_complete_monotonic_light_and_dark_ramps() {
        // Upstream butterPalettes.test.ts.
        assert_complete_monotonic(
            &regenerated("butter"),
            &[
                "neutral", "accent", "blue", "cyan", "green", "orange", "pink", "purple", "red",
                "teal", "yellow",
            ],
        );
    }

    #[test]
    fn butter_palette_keeps_its_signature_colors_and_warm_surfaces_exact() {
        // Upstream butterPalettes.test.ts.
        let p = regenerated("butter");
        assert_eq!(color(&p, "accent", PaletteMode::Light, 40.0), "#225bff");
        assert_eq!(color(&p, "yellow", PaletteMode::Dark, 90.0), "#fdee8c");
        assert_eq!(color(&p, "neutral", PaletteMode::Light, 95.0), "#fdfbe4");
        assert_eq!(color(&p, "neutral", PaletteMode::Dark, 10.0), "#261a13");
    }

    #[test]
    fn y2k_palette_contains_complete_monotonic_light_and_dark_ramps() {
        // Upstream y2kPalettes.test.ts.
        assert_complete_monotonic(
            &regenerated("y2k"),
            &[
                "neutral", "accent", "blue", "cyan", "green", "orange", "pink", "purple", "red",
                "teal", "yellow",
            ],
        );
    }

    #[test]
    fn y2k_palette_keeps_its_signature_colors_exact() {
        // Upstream y2kPalettes.test.ts.
        let p = regenerated("y2k");
        assert_eq!(color(&p, "accent", PaletteMode::Light, 80.0), "#cccffa");
        assert_eq!(color(&p, "green", PaletteMode::Light, 80.0), "#c5e17a");
        assert_eq!(color(&p, "green", PaletteMode::Dark, 80.0), "#c5e17a");
        assert_eq!(color(&p, "blue", PaletteMode::Light, 20.0), "#003454");
        assert_eq!(color(&p, "blue", PaletteMode::Light, 75.0), "#70c1ff");
        assert_eq!(color(&p, "blue", PaletteMode::Light, 85.0), "#addbff");
    }

    #[test]
    fn neutral_palette_keeps_the_approved_palette_structurally_valid() {
        // Upstream neutralPalettes.test.ts: the families in order, after the
        // standalone black and white.
        let p = regenerated("neutral");
        assert_eq!(p.black, "#000000");
        assert_eq!(p.white, "#ffffff");
        let ids: Vec<&str> = p.palette.iter().map(|f| f.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "neutral", "red", "orange", "yellow", "green", "teal", "cyan", "blue", "purple",
                "pink"
            ]
        );
        for id in ids {
            for mode in PaletteMode::BOTH {
                for &stop in &DEFAULT_21_STOPS {
                    assert!(is_hex6(&color(&p, id, mode, stop)), "{id} {mode} {stop}");
                }
                assert_eq!(color(&p, id, mode, 0.0), "#000000");
                assert_eq!(color(&p, id, mode, 100.0), "#ffffff");
            }
        }
    }

    #[test]
    fn neutral_palette_keeps_the_approved_muted_dark_edge_values() {
        // Upstream neutralPalettes.test.ts: the dark chroma taper's stop 25.
        let p = regenerated("neutral");
        let expected = [
            ("red", "#5b2b28"),
            ("orange", "#503424"),
            ("yellow", "#453a1c"),
            ("green", "#2b422b"),
            ("teal", "#28413c"),
            ("cyan", "#274046"),
            ("blue", "#253c5a"),
            ("purple", "#4a2f51"),
            ("pink", "#572b3d"),
        ];
        for (id, hex) in expected {
            assert_eq!(color(&p, id, PaletteMode::Dark, 25.0), hex, "{id}");
        }
    }

    #[test]
    fn matcha_palette_exposes_complete_reviewed_light_and_dark_ramps() {
        // Upstream matchaPalettes.test.ts.
        let p = regenerated("matcha");
        for family in &p.palette {
            for mode in PaletteMode::BOTH {
                for &stop in &DEFAULT_21_STOPS {
                    assert!(is_hex6(&color(&p, &family.id, mode, stop)));
                }
                assert_eq!(color(&p, &family.id, mode, 0.0), "#000000");
                assert_eq!(color(&p, &family.id, mode, 100.0), "#ffffff");
            }
        }
        assert_eq!(p.palette.len(), 10);
    }

    #[test]
    fn matcha_palette_keeps_its_semantic_categorical_and_tea_gold_values_exact() {
        // Upstream matchaPalettes.test.ts.
        let p = regenerated("matcha");
        assert_eq!(color(&p, "neutral", PaletteMode::Light, 20.0), "#3e481d");
        assert_eq!(color(&p, "neutral", PaletteMode::Light, 40.0), "#707e46");
        assert_eq!(color(&p, "neutral", PaletteMode::Light, 75.0), "#c0cba9");
        assert_eq!(color(&p, "neutral", PaletteMode::Light, 95.0), "#f0f0e0");
        assert_eq!(color(&p, "neutral", PaletteMode::Dark, 5.0), "#12140e");
        assert_eq!(color(&p, "neutral", PaletteMode::Dark, 75.0), "#c0cba9");
        assert_eq!(color(&p, "yellow", PaletteMode::Light, 80.0), "#f2c000");
        assert_eq!(color(&p, "yellow", PaletteMode::Dark, 85.0), "#f6d066");
    }

    #[test]
    fn gothic_palette_keeps_its_semantic_categorical_and_syntax_anchors_exact() {
        // Upstream gothicPalettes.test.ts (the generated, dark-only palette).
        let p = regenerated("gothic");
        assert_eq!(color(&p, "purple", PaletteMode::Dark, 80.0), "#c39adb");
        assert_eq!(color(&p, "green", PaletteMode::Dark, 70.0), "#a3c987");
        assert_eq!(color(&p, "blue", PaletteMode::Dark, 65.0), "#8aa1d8");
        assert_eq!(color(&p, "yellow", PaletteMode::Dark, 65.0), "#e6b85e");
        assert_eq!(color(&p, "yellow", PaletteMode::Dark, 70.0), "#dec074");
        assert_eq!(color(&p, "pink", PaletteMode::Dark, 65.0), "#d97580");
        assert_eq!(color(&p, "teal", PaletteMode::Dark, 70.0), "#7cc5b3");
        for family in p.palette.iter().filter(|f| f.id != "neutral") {
            assert!(family.light.is_none());
            for stop in [20.0, 30.0, 60.0, 75.0] {
                assert!(is_hex6(&color(&p, &family.id, PaletteMode::Dark, stop)));
            }
        }
    }

    #[test]
    fn stone_palette_keeps_every_family_and_mode_structurally_complete() {
        // Upstream stonePalettes.test.ts.
        let p = regenerated("stone");
        let ids: Vec<&str> = p.palette.iter().map(|f| f.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "neutral", "red", "orange", "yellow", "green", "teal", "cyan", "blue", "purple",
                "pink"
            ]
        );
        for id in ids {
            for mode in PaletteMode::BOTH {
                for &stop in &DEFAULT_21_STOPS {
                    assert!(is_hex6(&color(&p, id, mode, stop)));
                }
            }
        }
    }

    #[test]
    fn stone_palette_preserves_its_exact_anchors_in_both_modes() {
        // Upstream stonePalettes.test.ts.
        let p = regenerated("stone");
        for mode in PaletteMode::BOTH {
            assert_eq!(color(&p, "neutral", mode, 15.0), "#25252a");
            assert_eq!(color(&p, "neutral", mode, 50.0), "#77777c");
            assert_eq!(color(&p, "neutral", mode, 90.0), "#e2e2e8");
        }
        let midpoints = [
            ("red", "#8a716d"),
            ("orange", "#8f7154"),
            ("yellow", "#927f25"),
            ("green", "#528b49"),
            ("teal", "#588174"),
            ("cyan", "#507b86"),
            ("blue", "#6c7888"),
            ("purple", "#7b7486"),
            ("pink", "#83727c"),
        ];
        for (id, hex) in midpoints {
            assert_eq!(color(&p, id, PaletteMode::Light, 50.0), hex, "{id}");
            assert_eq!(color(&p, id, PaletteMode::Dark, 50.0), hex, "{id}");
        }
        assert_eq!(color(&p, "yellow", PaletteMode::Light, 90.0), "#f4e1b7");
        assert_eq!(color(&p, "yellow", PaletteMode::Dark, 90.0), "#f1e1b9");
        assert_eq!(color(&p, "orange", PaletteMode::Light, 90.0), "#ffdcbb");
        assert_eq!(color(&p, "orange", PaletteMode::Dark, 90.0), "#ffdcbb");
        assert_eq!(color(&p, "green", PaletteMode::Light, 85.0), "#bfdfbb");
        assert_eq!(color(&p, "green", PaletteMode::Dark, 85.0), "#c3ddbf");
    }

    /// A theme definition's `[light, dark]` value of a token.
    fn defined(theme: &Json, token: &str, mode: PaletteMode) -> String {
        let value = theme.get("tokens").unwrap().get(token).unwrap();
        let index = usize::from(mode == PaletteMode::Dark);
        match value {
            Json::Array(pair) => pair[index].str().to_string(),
            other => other.str().to_string(),
        }
    }

    #[test]
    fn butter_mapping_preserves_the_signature_accents_and_warm_body_surfaces() {
        // Upstream butterColorMapping.test.ts, against the theme definition.
        let p = regenerated("butter");
        let theme = super::spec_json::spec_theme_definition("butter");
        let (light, dark) = (PaletteMode::Light, PaletteMode::Dark);
        assert_eq!(
            defined(&theme, "--color-accent", light),
            color(&p, "accent", light, 40.0)
        );
        assert_eq!(
            defined(&theme, "--color-accent", dark),
            color(&p, "yellow", dark, 90.0)
        );
        assert_eq!(
            defined(&theme, "--color-background-body", light),
            color(&p, "neutral", light, 95.0)
        );
        assert_eq!(
            defined(&theme, "--color-background-body", dark),
            color(&p, "neutral", dark, 10.0)
        );
    }

    #[test]
    fn butter_mapping_maps_categorical_roles_to_reviewed_stops() {
        // Upstream butterColorMapping.test.ts.
        let p = regenerated("butter");
        let theme = super::spec_json::spec_theme_definition("butter");
        for mode in PaletteMode::BOTH {
            for family in [
                "blue", "cyan", "orange", "pink", "purple", "red", "teal", "yellow",
            ] {
                let (background, border) = if family == "yellow" {
                    (85.0, 70.0)
                } else {
                    (90.0, 80.0)
                };
                let at = |stop| color(&p, family, mode, stop);
                assert_eq!(
                    defined(&theme, &format!("--color-background-{family}"), mode),
                    at(background)
                );
                assert_eq!(
                    defined(&theme, &format!("--color-border-{family}"), mode),
                    at(border)
                );
                assert_eq!(
                    defined(&theme, &format!("--color-icon-{family}"), mode),
                    at(25.0)
                );
                assert_eq!(
                    defined(&theme, &format!("--color-text-{family}"), mode),
                    at(25.0)
                );
            }
        }
    }

    #[test]
    fn butter_mapping_uses_one_smooth_green_family_for_categorical_and_success_roles() {
        // Upstream butterColorMapping.test.ts.
        let p = regenerated("butter");
        let theme = super::spec_json::spec_theme_definition("butter");
        let (light, dark) = (PaletteMode::Light, PaletteMode::Dark);
        assert_eq!(
            defined(&theme, "--color-success", light),
            color(&p, "green", light, 25.0)
        );
        assert_eq!(
            defined(&theme, "--color-success", dark),
            color(&p, "green", dark, 70.0)
        );
        assert_eq!(
            defined(&theme, "--color-background-green", light),
            color(&p, "green", light, 90.0)
        );
        assert_eq!(
            defined(&theme, "--color-background-green", dark),
            color(&p, "green", dark, 90.0)
        );
    }

    #[test]
    fn y2k_mapping_preserves_the_periwinkle_body_and_reviewed_neutral_surfaces() {
        // Upstream y2kColorMapping.test.ts.
        let p = regenerated("y2k");
        let theme = super::spec_json::spec_theme_definition("y2k");
        let (light, dark) = (PaletteMode::Light, PaletteMode::Dark);
        assert_eq!(
            defined(&theme, "--color-background-body", light),
            color(&p, "accent", light, 80.0)
        );
        assert_eq!(
            defined(&theme, "--color-background-body", dark),
            color(&p, "neutral", dark, 5.0)
        );
        assert_eq!(
            defined(&theme, "--color-text-primary", light),
            color(&p, "neutral", light, 15.0)
        );
        assert_eq!(
            defined(&theme, "--color-text-primary", dark),
            color(&p, "neutral", dark, 95.0)
        );
    }

    #[test]
    fn y2k_mapping_maps_neon_green_and_success_to_the_shared_green_family() {
        // Upstream y2kColorMapping.test.ts.
        let p = regenerated("y2k");
        let theme = super::spec_json::spec_theme_definition("y2k");
        for mode in PaletteMode::BOTH {
            assert_eq!(
                defined(&theme, "--color-success", mode),
                color(&p, "green", mode, 80.0)
            );
            assert_eq!(
                defined(&theme, "--color-background-green", mode),
                color(&p, "green", mode, 80.0)
            );
        }
    }

    #[test]
    fn y2k_mapping_keeps_blue_airy_while_increasing_its_electric_saturation() {
        // Upstream y2kColorMapping.test.ts.
        let p = regenerated("y2k");
        let theme = super::spec_json::spec_theme_definition("y2k");
        let (light, dark) = (PaletteMode::Light, PaletteMode::Dark);
        assert_eq!(
            defined(&theme, "--color-background-blue", light),
            color(&p, "blue", light, 85.0)
        );
        assert_eq!(
            defined(&theme, "--color-background-blue", dark),
            color(&p, "blue", dark, 85.0)
        );
        assert_eq!(
            defined(&theme, "--color-border-blue", light),
            color(&p, "blue", light, 75.0)
        );
        assert_eq!(
            defined(&theme, "--color-text-blue", dark),
            color(&p, "blue", dark, 20.0)
        );
    }

    #[test]
    fn y2k_mapping_maps_the_remaining_categorical_roles_to_reviewed_stops() {
        // Upstream y2kColorMapping.test.ts.
        let p = regenerated("y2k");
        let theme = super::spec_json::spec_theme_definition("y2k");
        for mode in PaletteMode::BOTH {
            for family in [
                "cyan", "green", "orange", "pink", "purple", "red", "teal", "yellow",
            ] {
                let ramp: Vec<&str> = p
                    .ramp(family, mode)
                    .unwrap()
                    .iter()
                    .map(|(_, c)| c)
                    .collect();
                for role in ["background", "border", "icon"] {
                    assert!(is_hex6(&defined(
                        &theme,
                        &format!("--color-{role}-{family}"),
                        mode
                    )));
                }
                for role in ["background", "text"] {
                    let value = defined(&theme, &format!("--color-{role}-{family}"), mode);
                    assert!(ramp.contains(&value.as_str()), "{role} {family} {mode}");
                }
            }
        }
    }
}

#[cfg(test)]
mod generation_tests {
    use super::spec_json::{Json, request_from_json};
    use super::*;
    use crate::authoring::js::{js_number_to_string, js_to_fixed};

    // Expected values: generator.ts run under Node 24 on the same requests.

    fn request(json: &str) -> TonalPaletteGenerationInput {
        request_from_json(&Json::parse(json))
    }

    /// A candidate as the fixtures write it: one line per family and mode,
    /// its colors in stop order.
    fn dump(candidate: &TonalPaletteCandidate) -> String {
        let mut lines = Vec::new();
        for family in &candidate.palette {
            for mode in PaletteMode::BOTH {
                if let Some(ramp) = family.ramp(mode) {
                    let colors: Vec<&str> = ramp
                        .js_key_order()
                        .into_iter()
                        .filter_map(|s| ramp.get(s))
                        .collect();
                    lines.push(format!("{} {mode} {}", family.id, colors.join(",")));
                }
            }
        }
        lines.join("\n")
    }

    fn assert_lines(actual: &str, expected: &str) {
        for (index, (a, e)) in actual.lines().zip(expected.lines()).enumerate() {
            assert_eq!(a, e, "line {}", index + 1);
        }
        assert_eq!(actual.lines().count(), expected.lines().count());
    }

    const NEUTRAL_REQUEST: &str = r##"{"darkChromaTaper":{"edgeMultiplier":0.5,"familyMultipliers":{"yellow":0.65},"recoverAtStop":60,"throughStop":25},"families":[{"anchors":[],"id":"neutral","kind":"neutral","name":"Neutral","seed":"#777777"},{"anchors":[],"id":"red","kind":"chromatic","name":"Red","seed":"#d62830"},{"anchors":[],"id":"orange","kind":"chromatic","name":"Orange","seed":"#d57113"},{"anchors":[{"color":"#f8c723","mode":"light","policy":"flexible","stop":80}],"id":"yellow","kind":"chromatic","name":"Yellow","seed":"#f8c723"},{"anchors":[{"color":"#358a3a","mode":"light","policy":"flexible","stop":50}],"id":"green","kind":"chromatic","name":"Green","seed":"#358a3a"},{"anchors":[],"id":"teal","kind":"chromatic","name":"Teal","seed":"#0c7365"},{"anchors":[],"id":"cyan","kind":"chromatic","name":"Cyan","seed":"#0c6f82"},{"anchors":[{"color":"#0074e2","mode":"light","policy":"exact","stop":50}],"id":"blue","kind":"chromatic","name":"Blue","seed":"#0074e2"},{"anchors":[{"color":"#980fb2","mode":"light","policy":"flexible","stop":50}],"id":"purple","kind":"chromatic","name":"Purple","seed":"#980fb2"},{"anchors":[],"id":"pink","kind":"chromatic","name":"Pink","seed":"#b10e69"}],"modeStrategy":"light-and-dark","neutralProfile":"neutral-v1","recipe":"astryx-oklch-v1","stops":[0,5,10,15,20,25,30,35,40,45,50,55,60,65,70,75,80,85,90,95,100],"vibrancy":50}"##;
    const ANCHORS_REQUEST: &str = r##"{"recipe":"astryx-oklch-v1","modeStrategy":"light-only","neutralProfile":"cool-v1","stops":[0,10,20,30,40,50,60,70,80,90,100],"vibrancy":80,"families":[{"id":"neutral","seed":"#888888","kind":"neutral"},{"id":"brand","seed":"#e11d48","anchors":[{"mode":"light","stop":30,"color":"#9f1239","policy":"bounded","maxDeltaE":3},{"mode":"light","stop":70,"color":"fda4af","policy":"flexible"}]}]}"##;
    const DECIMAL_REQUEST: &str = r##"{"modeStrategy":"dark-only","neutralProfile":"warm-v1","stops":[0,12.5,33.3,50,66.7,87.5,100],"families":[{"id":"gray","seed":"#777","kind":"neutral"},{"id":"teal","seed":"#14B8A6","anchors":[{"mode":"dark","stop":33.3,"color":"#115e59","policy":"exact"}]}]}"##;
    const TAPER_REQUEST: &str = r##"{"recipe":"astryx-oklch-v2","vibrancy":100,"neutralProfile":"custom","darkChromaTaper":{"edgeMultiplier":0.4,"throughStop":20,"recoverAtStop":50,"familyMultipliers":{"yellow":0.8}},"families":[{"id":"neutral","seed":"#64748b","kind":"neutral"},{"id":"red","seed":"#ef4444"},{"id":"yellow","seed":"#eab308"},{"id":"blue","seed":"#3b82f6"}]}"##;
    const BAD_REQUEST: &str = r##"{"families":[{"id":"ok","seed":"#3366cc"},{"id":"zigzag","seed":"#3366cc","anchors":[{"mode":"light","stop":30,"color":"#eeeeee","policy":"exact"},{"mode":"light","stop":70,"color":"#111111","policy":"exact"}]},{"id":"dupe","seed":"#cc3366","anchors":[{"mode":"dark","stop":50,"color":"#cc3366","policy":"exact"},{"mode":"dark","stop":50,"color":"#aa2255","policy":"exact"}]}]}"##;

    #[test]
    fn v2_recipe_regenerates_the_neutral_request_as_upstream() {
        let mut input = request(NEUTRAL_REQUEST);
        input.recipe = Some(PALETTE_RECIPE_V2);
        let expected = r##"neutral light #000000,#111111,#1b1b1b,#262626,#303030,#3b3b3b,#474747,#525252,#5e5e5e,#6a6a6a,#777777,#848484,#919191,#9e9e9e,#ababab,#b9b9b9,#c6c6c6,#d4d4d4,#e2e2e2,#f1f1f1,#ffffff
neutral dark #000000,#111111,#1b1b1b,#262626,#303030,#3b3b3b,#474747,#525252,#5e5e5e,#6a6a6a,#777777,#848484,#919191,#9e9e9e,#ababab,#b9b9b9,#c6c6c6,#d4d4d4,#e2e2e2,#f1f1f1,#ffffff
red light #000000,#2c0000,#3e0002,#500004,#620008,#76000c,#8a0011,#9e0015,#af1320,#c0262c,#cf3738,#de4745,#ed5753,#fa6762,#ff7f77,#f59e97,#e8b9b5,#edcbc8,#f3ddda,#f9eeed,#ffffff
red dark #000000,#200908,#2f1110,#3d1917,#4c221f,#5b2b28,#6b3431,#7e3c38,#92423e,#a84743,#be4e49,#d15752,#e1645e,#ee736c,#fb837b,#ff9890,#f4b4ae,#edcbc8,#f3ddda,#f9eeed,#ffffff
orange light #000000,#250500,#340d00,#431600,#521f00,#622700,#733100,#833a00,#944400,#a54f00,#b45c00,#c06a22,#d87109,#e97e1d,#f58d3a,#fe9e57,#f1b994,#e9cfbe,#f0dfd4,#f7efe9,#ffffff
orange dark #000000,#1c0c05,#29160d,#362014,#43291c,#503424,#603d2a,#72462d,#864f2c,#9b5627,#af5f1d,#c2691b,#d1762b,#df843f,#eb9352,#f7a266,#fdb481,#efcdb6,#f0dfd4,#f7efe9,#ffffff
yellow light #000000,#190f00,#251a00,#312400,#3e2e00,#4b3900,#584400,#664f00,#745b00,#836700,#927300,#988142,#ad8d33,#c19a0f,#d2a700,#e2b623,#eec448,#f8d36a,#ffe193,#f9f0dc,#ffffff
yellow dark #000000,#161002,#221b08,#2d250e,#392f15,#453a1c,#524521,#615124,#705c24,#80681f,#907316,#9f801c,#b18c00,#be9921,#cba73d,#d7b554,#e3c36c,#efd284,#fae19e,#fbf0d4,#ffffff
green light #000000,#001800,#002400,#003002,#003d05,#00490b,#0b5615,#17631e,#237028,#2f7d33,#3c8a3f,#4a984c,#59a55b,#6ab26b,#7dbe7e,#90ca90,#a4d6a3,#b7e2b6,#cbeeca,#defadd,#ffffff
green dark #000000,#091409,#112011,#192b19,#223622,#2b422b,#344e34,#3c5c3c,#436a43,#497849,#508651,#59945a,#65a266,#74af75,#85bb85,#97c897,#aad4a9,#bce0bb,#ceecce,#e1f8e0,#ffffff
teal light #000000,#001612,#00221c,#002e27,#003a32,#00463d,#005348,#006154,#006e60,#007c6d,#008b7a,#559185,#43a493,#3ab4a1,#56c0ae,#74ccbb,#90d7c8,#a9e2d6,#c1eee4,#d7f9f2,#ffffff
teal dark #000000,#071411,#0f1f1c,#172a26,#1f3531,#28413c,#2f4e48,#325b53,#326a60,#2d796c,#248979,#349686,#39a593,#4fb1a0,#68bdad,#81c9bb,#99d5c8,#b1e0d6,#c6ece3,#dbf8f1,#ffffff
cyan light #000000,#00151b,#002028,#002c35,#003743,#004351,#00505f,#005d6e,#006a7d,#00788d,#00859d,#5b8d9a,#4a9eb2,#3caec7,#45bdd7,#5ecae3,#75d7ef,#8ee4f9,#cee8ef,#ebf2f4,#ffffff
cyan dark #000000,#071316,#0f1e22,#17292e,#1f343a,#274046,#2e4c54,#325963,#326774,#2d7586,#228499,#3691a6,#36a0b7,#49adc4,#5dbad1,#71c7dd,#85d5e9,#9ae2f4,#baedfa,#ebf2f4,#ffffff
blue light #000000,#000f30,#001a41,#002452,#002f65,#003978,#00458c,#0050a2,#005cb8,#0067ce,#0074e2,#1e81f0,#348ffd,#529fff,#77aef4,#9bbbe6,#b3c9e5,#c6d6ec,#d9e4f2,#ecf1f8,#ffffff
blue dark #000000,#051121,#0d1c2f,#15263d,#1d314b,#253c5a,#2b486c,#2f5382,#315f99,#346bb1,#3778c7,#3e84dc,#4a92eb,#5aa0f8,#6eaeff,#8ebcf8,#afc9eb,#c6d6ec,#d9e4f2,#ecf1f8,#ffffff
purple light #000000,#210028,#31003b,#3f004c,#4e045c,#5c0e6c,#6b187c,#781f8a,#842699,#9130a7,#9f3cb5,#ad4dc3,#bb5fd1,#ca72de,#d885eb,#e496f6,#efa8ff,#f1c0fd,#efdbf4,#f7edf9,#ffffff
purple dark #000000,#180b1b,#251329,#311c36,#3e2544,#4a2f51,#58395f,#674270,#784983,#895196,#9a59a9,#aa63ba,#b870c9,#c57ed5,#d28de2,#de9ced,#eaacf8,#f3bfff,#f3d8f9,#f7edf9,#ffffff
pink light #000000,#290013,#3b001e,#4b0028,#5d0034,#70003f,#83004b,#950457,#a61863,#b52970,#c5387c,#d44789,#e25796,#ef67a3,#fc78b1,#fe91bd,#f1b2c9,#edc9d6,#f3dbe3,#f9edf1,#ffffff
pink dark #000000,#1e0912,#2d111d,#3b1927,#492132,#572b3d,#663449,#783b55,#8c4161,#a1476e,#b54e7b,#c85688,#d76395,#e572a3,#f181b0,#fd92bd,#fdaaca,#f0c8d6,#f3dbe3,#f9edf1,#ffffff"##;
        assert_lines(&dump(&generate_tonal_palette(&input).unwrap()), expected);
    }

    #[test]
    fn bounded_and_flexible_anchors_on_compact_cool_stops_match_upstream() {
        let expected = r##"neutral light #000000,#161c22,#29313a,#3e4852,#55606b,#6d7885,#87929e,#a1adb9,#bec8d3,#dbe4ec,#ffffff
brand light #000000,#3e000e,#680320,#930832,#bb1244,#dd315b,#f45c78,#ff8a9b,#feb5bd,#ffd8dc,#ffffff"##;
        assert_lines(
            &dump(&generate_tonal_palette(&request(ANCHORS_REQUEST)).unwrap()),
            expected,
        );
    }

    #[test]
    fn decimal_stops_on_a_warm_dark_only_palette_match_upstream() {
        // Decimal stops come after the whole ones in JavaScript's key order.
        let expected = r##"gray dark #000000,#7c766e,#ffffff,#23201b,#534d46,#a7a199,#dfdbd4
teal dark #000000,#098a83,#ffffff,#002725,#115e59,#57b5ad,#bce5e1"##;
        assert_lines(
            &dump(&generate_tonal_palette(&request(DECIMAL_REQUEST)).unwrap()),
            expected,
        );
    }

    #[test]
    fn a_tapered_v2_palette_at_full_vibrancy_matches_upstream() {
        let expected = r##"neutral light #000000,#08111e,#101c2d,#18263b,#203149,#2a3c57,#344864,#3e5372,#496080,#556c8d,#61789b,#6d85a8,#7a92b5,#88a0c2,#98adcb,#a9bad3,#bac8dc,#cbd5e5,#dce3ee,#edf1f7,#ffffff
neutral dark #000000,#0a111c,#121c2b,#1a2638,#233145,#2c3c53,#374860,#41536e,#4c607b,#586c88,#647896,#7185a3,#7e92b0,#8b9fbd,#9badc6,#abbacf,#bcc8d9,#ccd5e2,#dde3ec,#edf1f6,#ffffff
red light #000000,#2c0000,#3e0002,#500004,#620007,#76000b,#8a0010,#9e0014,#b30019,#c9001d,#df0022,#f60027,#ff3a3f,#ff625d,#ff7f77,#fb9a92,#f7b3ac,#f8c7c1,#fadad6,#fdedeb,#ffffff
red dark #000000,#1d0b09,#2b1412,#391c1a,#472523,#592d2a,#70302d,#8b2f2c,#a82628,#c61323,#df0022,#f11c2e,#ff3a3f,#ff625d,#ff7f77,#ff988f,#fbb1a9,#f8c7c1,#fadad6,#fdedeb,#ffffff
yellow light #000000,#1a0f00,#261900,#322300,#3f2d00,#4d3800,#5a4300,#694e00,#775900,#866500,#967100,#9f7f30,#b28a25,#c49611,#d6a300,#e6b000,#f7bd00,#ffcd54,#fedf99,#fbf0d8,#ffffff
yellow dark #000000,#180f00,#241a03,#302406,#3c2e0c,#4a3910,#584412,#674f10,#765a0b,#866502,#967100,#a47d0e,#b58900,#c59600,#d6a300,#e6b000,#f6bd16,#ffcd54,#ffdf97,#fbf0d8,#ffffff
blue light #000000,#00083c,#00154b,#001f5d,#002a71,#003487,#003e9d,#0049b3,#0054cb,#005fe3,#006bfc,#2a7cff,#478dff,#619cfe,#7cacfa,#96baf4,#adc8f4,#c1d6f7,#d6e4f9,#eaf1fc,#ffffff
blue dark #000000,#061122,#0f1b2f,#18263d,#20304b,#263b5e,#284677,#245097,#1b58ba,#0a61de,#006bfb,#2a7cff,#478dff,#609cff,#78abff,#92bafb,#acc8f6,#c1d6f7,#d6e4f9,#eaf1fc,#ffffff"##;
        assert_lines(
            &dump(&generate_tonal_palette(&request(TAPER_REQUEST)).unwrap()),
            expected,
        );
    }

    /// A generation result as the fixtures write it: the normalized request,
    /// each ramp's diagnostics (to eight decimals), the coordination and the
    /// errors.
    fn render_set(result: &PaletteGenerationResult) -> String {
        let f8 = |x: f64| js_to_fixed(x, 8);
        let n = js_number_to_string;
        let r = &result.request;
        let stops: Vec<String> = r.stops.iter().map(|&s| n(s)).collect();
        let taper = r.dark_chroma_taper.as_ref().map_or("-".to_string(), |t| {
            let families: Vec<String> = t
                .family_multipliers
                .iter()
                .map(|(k, v)| format!("{k}={}", n(*v)))
                .collect();
            format!(
                "{}:{}:{}:{}",
                n(t.edge_multiplier),
                n(t.through_stop),
                n(t.recover_at_stop),
                families.join(",")
            )
        });
        let mut lines = vec![format!(
            "request {} {} {} {} {} {taper}",
            r.recipe,
            n(r.vibrancy),
            r.neutral_profile,
            r.mode_strategy,
            stops.join(",")
        )];
        for f in &r.families {
            let anchors: Vec<String> = f
                .anchors
                .iter()
                .map(|a| {
                    format!(
                        "{}:{}:{}:{}:{}",
                        a.mode,
                        n(a.stop),
                        a.color,
                        a.policy,
                        a.max_delta_e.map_or("-".to_string(), n)
                    )
                })
                .collect();
            lines.push(format!(
                "family {} {} {} {} {}",
                f.id,
                f.name,
                f.seed,
                f.kind,
                anchors.join(";")
            ));
        }
        for f in &result.families {
            for mode in PaletteMode::BOTH {
                let Some(ramp) = f.ramp(mode) else { continue };
                let d = &ramp.diagnostics;
                let gamut: Vec<String> = d.gamut_mapped_stops.iter().map(|&s| n(s)).collect();
                let anchors: Vec<String> = d
                    .anchors
                    .iter()
                    .map(|a| {
                        format!(
                            "{}:{}:{}:{}:{}:{}:{}",
                            a.mode,
                            n(a.stop),
                            a.color,
                            a.policy,
                            a.max_delta_e.map_or("-".to_string(), n),
                            a.generated_color,
                            f8(a.delta_e)
                        )
                    })
                    .collect();
                lines.push(format!(
                    "ramp {} {mode} {} {} {} {} {} {} {}",
                    f.id,
                    d.monotonic,
                    f8(d.minimum_adjacent_delta_e),
                    f8(d.maximum_adjacent_delta_e),
                    f8(d.maximum_hue_drift),
                    d.hue_identity_risk.map_or("-", |r| r.as_str()),
                    gamut.join(","),
                    anchors.join(";")
                ));
            }
        }
        for c in &result.coordination {
            lines.push(format!(
                "coordination {} {} {} {} {} {} {}",
                c.mode,
                n(c.stop),
                c.closest_families
                    .as_ref()
                    .map_or("-".to_string(), |(a, b)| format!("{a}/{b}")),
                c.minimum_family_delta_e.map_or("-".to_string(), f8),
                c.strongest_family.as_deref().unwrap_or("-"),
                c.weakest_family.as_deref().unwrap_or("-"),
                c.chroma_ratio.map_or("-".to_string(), f8)
            ));
        }
        for e in &result.errors {
            lines.push(format!("error {} {}", e.family_id, e.message));
        }
        lines.join("\n")
    }

    #[test]
    fn diagnoses_the_neutral_palette_as_upstream() {
        let expected = r##"request astryx-oklch-v1 50 neutral-v1 light-and-dark 0,5,10,15,20,25,30,35,40,45,50,55,60,65,70,75,80,85,90,95,100 0.5:25:60:yellow=0.65
family neutral Neutral #777777 neutral 
family red Red #d62830 chromatic 
family orange Orange #d57113 chromatic 
family yellow Yellow #f8c723 chromatic light:80:#f8c723:flexible:-
family green Green #358a3a chromatic light:50:#358a3a:flexible:-
family teal Teal #0c7365 chromatic 
family cyan Cyan #0c6f82 chromatic 
family blue Blue #0074e2 chromatic light:50:#0074e2:exact:-
family purple Purple #980fb2 chromatic light:50:#980fb2:flexible:-
family pink Pink #b10e69 chromatic 
ramp neutral light true 4.05673565 17.76377719 0.00000000 -  
ramp neutral dark true 4.05673565 17.76377719 0.00000000 -  
ramp red light true 4.20409941 19.89667947 3.97814685 - 5,10,15,20,25,30,35,70,75,80,85,90,95,100 
ramp red dark true 4.24935783 18.24843667 2.99492620 - 10,15,20,75,80,85,90,95,100 
ramp orange light true 4.20575596 18.69531493 17.28335370 - 5,10,15,20,25,30,35,40,45,50,55,60,75,80,85,90,95,100 
ramp orange dark true 4.14246638 17.90221110 8.91595357 - 5,10,15,20,25,30,35,40,45,50,80,85,90,95,100 
ramp yellow light true 4.29473924 18.10880423 8.27746356 - 5,10,15,20,25,30,35,40,45,50,55,60,65,95,100 light:80:#f8c723:flexible:-:#eec448:2.55574858
ramp yellow dark true 4.16329695 17.88830392 0.97400630 - 5,10,15,20,25,30,35,40,45,50,55,60,95,100 
ramp green light true 4.10768403 19.13154978 1.86357344 - 10,15,20,100 light:50:#358a3a:flexible:-:#3c8a3f:0.85619187
ramp green dark true 4.20490567 17.87399418 0.60214372 - 100 
ramp teal light true 4.16674847 18.23096659 1.67367183 - 5,10,15,20,25,30,35,40,45,50,55,100 
ramp teal dark true 4.14420051 17.88768023 1.99016197 - 5,10,15,20,25,30,35,40,45,100 
ramp cyan light true 4.16384654 18.34169775 1.44856497 - 5,10,15,20,25,30,35,40,45,50,55,60,90,95,100 
ramp cyan dark true 4.25295370 17.77833630 1.92057646 - 5,10,15,20,25,30,35,40,45,50,95,100 
ramp blue light true 4.11445192 19.35463405 4.26107795 - 5,10,15,20,25,30,35,40,45,65,70,75,80,85,90,95,100 light:50:#0074e2:exact:-:#0074e2:0.00000000
ramp blue dark true 4.27858028 18.00411825 1.38051913 - 5,10,15,20,25,30,35,70,75,80,85,90,95,100 
ramp purple light true 3.58159755 19.96161232 0.64238669 - 80,85,90,95,100 light:50:#980fb2:flexible:-:#9f3cb5:5.67003936
ramp purple dark true 4.24596930 18.07185709 0.69763918 - 85,90,95,100 
ramp pink light true 4.24426148 19.77180069 1.13082471 - 5,10,15,20,25,30,75,80,85,90,95,100 
ramp pink dark true 4.17368352 18.20409862 0.65312165 - 10,15,80,85,90,95,100 
coordination light 50 Teal/Cyan 6.35949027 Purple Cyan 1.94639551
coordination dark 50 Teal/Cyan 5.72238405 Red Cyan 1.59467203"##;
        assert_lines(
            &render_set(&generate_palette_set(&request(NEUTRAL_REQUEST)).unwrap()),
            expected,
        );
    }

    #[test]
    fn diagnoses_bounded_and_flexible_anchors_as_upstream() {
        let expected = r##"request astryx-oklch-v1 80 cool-v1 light-only 0,10,20,30,40,50,60,70,80,90,100 -
family neutral neutral #888888 neutral 
family brand brand #e11d48 chromatic light:30:#9f1239:bounded:3;light:70:#fda4af:flexible:-
ramp neutral light true 8.44545313 22.35898572 4.49733427 - 100 
ramp brand light true 8.37250744 24.89911756 6.78226438 - 10,20,30,40,70,80,90,100 light:30:#9f1239:bounded:3:#930832:2.97452186;light:70:#fda4af:flexible:-:#ff8a9b:5.69955250
coordination light 50 - - brand brand 1.00000000"##;
        assert_lines(
            &render_set(&generate_palette_set(&request(ANCHORS_REQUEST)).unwrap()),
            expected,
        );
    }

    #[test]
    fn diagnoses_a_tapered_v2_palette_as_upstream() {
        let expected = r##"request astryx-oklch-v2 100 custom light-and-dark 0,5,10,15,20,25,30,35,40,45,50,55,60,65,70,75,80,85,90,95,100 0.4:20:50:yellow=0.8
family neutral neutral #64748b neutral 
family red red #ef4444 chromatic 
family yellow yellow #eab308 chromatic 
family blue blue #3b82f6 chromatic 
ramp neutral light true 4.11109211 17.88930396 1.90189482 - 100 
ramp neutral dark true 4.11813125 17.84157360 2.30461019 - 100 
ramp red light true 4.41275471 19.89667947 3.77062403 - 5,10,15,20,25,30,35,40,45,50,55,60,65 
ramp red dark true 4.26041185 17.97520638 2.02575511 - 5,10,15,20,25,30,35,40,45,50,60,65,70 
ramp yellow light true 4.22314773 18.27785122 6.70775632 - 5,10,15,20,25,30,35,40,45,50,70,75,80,85 
ramp yellow dark true 4.16637862 17.94408125 1.48496226 - 5,10,15,20,25,30,35,40,45,50,65,70,75,85,90 
ramp blue light true 4.39184428 20.49323588 3.86919096 - 5,10,15,20,25,30,35,40,45,50,55,60 
ramp blue dark true 4.11727152 18.18512392 1.91453923 - 5,10,15,20,25,30,35,40,45,55,60,65 
coordination light 50 red/yellow 20.05529071 blue yellow 1.98320690
coordination dark 50 red/yellow 20.05529071 red yellow 1.97934743"##;
        assert_lines(
            &render_set(&generate_palette_set(&request(TAPER_REQUEST)).unwrap()),
            expected,
        );
    }

    #[test]
    fn lists_the_families_that_cannot_grow_and_keeps_the_rest() {
        let expected = r##"request astryx-oklch-v1 50 neutral-v1 light-and-dark 0,5,10,15,20,25,30,35,40,45,50,55,60,65,70,75,80,85,90,95,100 -
family ok ok #3366cc chromatic 
family zigzag zigzag #3366cc chromatic light:30:#eeeeee:exact:-;light:70:#111111:exact:-
family dupe dupe #cc3366 chromatic dark:50:#cc3366:exact:-;dark:50:#aa2255:exact:-
ramp ok light true 4.22680611 19.79115132 0.65363377 - 65,70,75,80,85,90,95,100 
ramp ok dark true 4.20512524 19.21471062 0.69353707 - 70,75,80,85,90,95,100 
coordination light 50 - - ok ok 1.00000000
coordination dark 50 - - ok ok 1.00000000
error zigzag zigzag light ramp is not luminance-monotonic with the requested anchors.
error dupe Duplicate anchor for dark stop 50."##;
        assert_lines(
            &render_set(&generate_palette_set(&request(BAD_REQUEST)).unwrap()),
            expected,
        );
        assert_eq!(
            generate_tonal_palette(&request(BAD_REQUEST)).unwrap_err().0,
            "zigzag: zigzag light ramp is not luminance-monotonic with the requested anchors.\ndupe: Duplicate anchor for dark stop 50."
        );
    }

    #[test]
    fn refuses_a_malformed_request_with_upstreams_message() {
        let cases: [(&str, &str); 23] = [
            (
                r##"{"families":[]}"##,
                "Palette generation requires at least one family.",
            ),
            (
                r##"{"families":[{"id":"brand","seed":"#3366cc"}],"stops":[]}"##,
                "A palette requires at least one stop.",
            ),
            (
                r##"{"families":[{"id":"brand","seed":"#3366cc"}],"stops":[0,120]}"##,
                "Stop 120 must be a finite number from 0 to 100.",
            ),
            (
                r##"{"families":[{"id":"brand","seed":"#3366cc"}],"stops":[0,50,50]}"##,
                "Stops must be unique and strictly increasing.",
            ),
            (
                r##"{"families":[{"id":" Brand","seed":"#3366cc"}]}"##,
                "Family 0 id must use lower-kebab-case characters.",
            ),
            (
                r##"{"families":[{"id":"black","seed":"#000000"}]}"##,
                "Family id black is reserved for the standalone black palette value.",
            ),
            (
                r##"{"families":[{"id":"brand","seed":"#3366cc"},{"id":" brand ","seed":"#3366cc"}]}"##,
                "Duplicate family id: brand.",
            ),
            (
                r##"{"modeStrategy":"light-only","families":[{"id":"brand","seed":"#3366cc","anchors":[{"mode":"dark","stop":50,"color":"#3366cc","policy":"exact"}]}]}"##,
                "Anchor mode dark is not generated by mode strategy light-only.",
            ),
            (
                r##"{"families":[{"id":"brand","seed":"#3366cc","anchors":[{"mode":"light","stop":16.666666666666668,"color":"#3366cc","policy":"exact"}]}]}"##,
                "Anchor stop 16.666666666666668 is not present in the requested stop layout.",
            ),
            (
                r##"{"families":[{"id":"brand","seed":"#3366cc","anchors":[{"mode":"light","stop":50,"color":"#3366cc","policy":"bounded"}]}]}"##,
                "Bounded anchor at stop 50 requires a non-negative maxDeltaE.",
            ),
            (
                r##"{"families":[{"id":"brand","seed":"#3366cc","anchors":[{"mode":"light","stop":50,"color":"#3366cc","policy":"bounded","maxDeltaE":-1}]}]}"##,
                "Bounded anchor at stop 50 requires a non-negative maxDeltaE.",
            ),
            (
                r##"{"families":[{"id":"brand","seed":"#3366cc","anchors":[{"mode":"light","stop":50,"color":"#3366cc","policy":"exact","maxDeltaE":2}]}]}"##,
                "Anchor policy exact does not accept maxDeltaE.",
            ),
            (
                r##"{"families":[{"id":"brand","seed":"#3366cc","anchors":[{"mode":"light","stop":0,"color":"#111111","policy":"exact"}]}]}"##,
                "Anchor stop 0 is reserved for exact black; use an interior stop for a tinted endpoint.",
            ),
            (
                r##"{"families":[{"id":"brand","seed":"#3366cc","anchors":[{"mode":"light","stop":100,"color":"#eeeeee","policy":"exact"}]}]}"##,
                "Anchor stop 100 is reserved for exact white; use an interior stop for a tinted endpoint.",
            ),
            (
                r##"{"families":[{"id":"brand","seed":"red"}]}"##,
                "Invalid color: red",
            ),
            (
                r##"{"families":[{"id":"brand","seed":"#3366cc","anchors":[{"mode":"light","stop":50,"color":"#33","policy":"exact"}]}]}"##,
                "Invalid color: #33",
            ),
            (
                r##"{"families":[{"id":"brand","seed":"#3366cc"}],"vibrancy":101}"##,
                "Vibrancy must be a number from 0 to 100.",
            ),
            (
                r##"{"families":[{"id":"brand","seed":"#3366cc"}],"darkChromaTaper":{"edgeMultiplier":1.5,"throughStop":20,"recoverAtStop":50}}"##,
                "darkChromaTaper.edgeMultiplier must be a number from 0 to 1.",
            ),
            (
                r##"{"families":[{"id":"brand","seed":"#3366cc"}],"darkChromaTaper":{"edgeMultiplier":0.5,"throughStop":42,"recoverAtStop":50}}"##,
                "darkChromaTaper.throughStop must be a stop in the requested stop layout.",
            ),
            (
                r##"{"families":[{"id":"brand","seed":"#3366cc"}],"darkChromaTaper":{"edgeMultiplier":0.5,"throughStop":50,"recoverAtStop":50}}"##,
                "darkChromaTaper.recoverAtStop must be a stop in the requested stop layout after throughStop.",
            ),
            (
                r##"{"families":[{"id":"brand","seed":"#3366cc"}],"darkChromaTaper":{"edgeMultiplier":0.5,"throughStop":20,"recoverAtStop":50,"familyMultipliers":{"ghost":0.5}}}"##,
                "darkChromaTaper.familyMultipliers names unknown family ghost.",
            ),
            (
                r##"{"families":[{"id":"gray","seed":"#777777","kind":"neutral"}],"darkChromaTaper":{"edgeMultiplier":0.5,"throughStop":20,"recoverAtStop":50,"familyMultipliers":{"gray":0.5}}}"##,
                "darkChromaTaper.familyMultipliers names neutral family gray; the taper changes only chromatic families.",
            ),
            (
                r##"{"families":[{"id":"brand","seed":"#3366cc"}],"darkChromaTaper":{"edgeMultiplier":0.5,"throughStop":20,"recoverAtStop":50,"familyMultipliers":{"brand":2}}}"##,
                "darkChromaTaper.familyMultipliers.brand must be a number from 0 to 1.",
            ),
        ];
        for (json, message) in cases {
            let error = generate_tonal_palette(&request(json)).unwrap_err();
            assert_eq!(error.0, message, "{json}");
        }
    }

    #[test]
    fn parse_stop_list_reads_numbers_as_javascript_does() {
        assert_eq!(parse_stop_list("0, 50, 100"), Ok(vec![0.0, 50.0, 100.0]));
        assert_eq!(
            parse_stop_list("0,,100").unwrap_err().0,
            "Every custom stop must be a number from 0 to 100."
        );
        assert_eq!(
            parse_stop_list("0, 0x10, 1e1").unwrap_err().0,
            "Stops must be unique and strictly increasing."
        );
        assert_eq!(parse_stop_list(" 5 , 10.5 "), Ok(vec![5.0, 10.5]));
        assert_eq!(
            parse_stop_list("Infinity").unwrap_err().0,
            "Every custom stop must be a number from 0 to 100."
        );
        assert_eq!(
            parse_stop_list("0, 12px").unwrap_err().0,
            "Every custom stop must be a number from 0 to 100."
        );
        assert_eq!(parse_stop_list("0b11, 0o7"), Ok(vec![3.0, 7.0]));
        assert_eq!(
            parse_stop_list("0, .5, 5., 1e2"),
            Ok(vec![0.0, 0.5, 5.0, 100.0])
        );
        assert_eq!(parse_stop_list("-0, 1"), Ok(vec![0.0, 1.0]));
        assert_eq!(parse_stop_list("0, +5"), Ok(vec![0.0, 5.0]));
    }

    #[test]
    fn exports_upstreams_constants() {
        assert_eq!(PALETTE_RECIPE.as_str(), "astryx-oklch-v1");
        assert_eq!(PALETTE_RECIPE_V2.as_str(), "astryx-oklch-v2");
        assert_eq!(PALETTE_RECIPES, [PALETTE_RECIPE, PALETTE_RECIPE_V2]);
        assert_eq!(PALETTE_GENERATOR_VERSION, "palette-generator-v1");
        assert_eq!((PALETTE_BLACK, PALETTE_WHITE), ("#000000", "#ffffff"));
        assert_eq!(DEFAULT_21_STOPS.len(), 21);
        assert!(
            DEFAULT_21_STOPS
                .iter()
                .enumerate()
                .all(|(i, &s)| s == 5.0 * i as f64)
        );
        assert!(
            COMPACT_11_STOPS
                .iter()
                .enumerate()
                .all(|(i, &s)| s == 10.0 * i as f64)
        );
        assert_eq!(
            modes_for_strategy(ModeStrategy::DarkOnly),
            [PaletteMode::Dark]
        );
        assert_eq!(
            modes_for_strategy(ModeStrategy::LightAndDark),
            PaletteMode::BOTH
        );
    }

    #[test]
    fn perceptual_delta_is_oklab_distance_times_100() {
        assert_eq!(perceptual_delta("#ffffff", "#ffffff"), Ok(0.0));
        let black_white = perceptual_delta("#000", "#fff").unwrap();
        assert!((black_white - 100.0).abs() < 1e-6, "{black_white}");
        assert!(perceptual_delta("#fff", "white").is_err());
    }

    #[test]
    fn a_request_without_defaults_resolves_to_upstreams_defaults() {
        let resolved = normalize_generation_request(&request(
            r#"{"families":[{"id":" brand ","seed":"ABC"}]}"#,
        ))
        .unwrap();
        assert_eq!(resolved.recipe, PALETTE_RECIPE);
        assert_eq!(resolved.vibrancy, 50.0);
        assert_eq!(resolved.neutral_profile, NeutralProfile::NeutralV1);
        assert_eq!(resolved.mode_strategy, ModeStrategy::LightAndDark);
        assert_eq!(resolved.stops, DEFAULT_21_STOPS);
        assert_eq!(resolved.dark_chroma_taper, None);
        let family = &resolved.families[0];
        assert_eq!(
            (
                family.id.as_str(),
                family.name.as_str(),
                family.seed.as_str()
            ),
            ("brand", "brand", "#aabbcc")
        );
        assert_eq!(family.kind, FamilyKind::Chromatic);
    }
}
