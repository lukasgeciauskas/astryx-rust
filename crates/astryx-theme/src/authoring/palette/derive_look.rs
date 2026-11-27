//! The colors a palette derives beyond its own families: upstream
//! `palette/derive-look.ts`.
//!
//! Core's category colors (Badge, Token and Card tints), the chart colors,
//! the status roles a theme leaves unset and the code colors, each grown
//! from a family of the palette. Each hue grows from Core's own hue at the
//! strength of the theme's accent (the palette's vibrancy then shapes it as
//! it shapes every color). Each token starts at the stop the official
//! neutral theme maps it to: a card tint pale enough that the theme's text
//! and secondary text read on it, text on it and on the page at 4.5:1, icons
//! at 3:1 (WCAG 2.2 SC 1.4.3, 1.4.11), and chart series at 3:1 on the page.
//! Every set is written. A chart series drawn like an earlier one takes the
//! nearest shade apart that still reads. A card tint drawn like an earlier
//! one, or like the page, takes the nearest quieter tint apart, else the
//! nearest stronger one. A status fill stays apart from the accent where a
//! shade does. A status role whose text reads at no shade keeps the base
//! theme's colors. A limit no shade meets is a note.

use std::f64::consts::PI;

pub use super::super::color::ratio_down;
use super::super::color::{
    InvalidColor, composite_over, contrast_ratio, hex_to_linear_rgb, hex_to_oklch,
    hex_to_rgb_triple, linear_rgb_to_oklab, max_oklch_chroma, oklch_clamped_hex, rgb_triple_to_hex,
};
use super::super::js::{js_max, js_min, js_number_to_string, js_parse_int_hex, js_round};
use super::generator::{compare, perceptual_delta};
use super::mapping::{MappedToken, MappedTokens, StopRef, mode_index};
use super::types::{PaletteError, PaletteMode, TonalPaletteFamily, TonalPaletteFamilyInput};

/// A shade of a family: its stop and color, and the family when it is not
/// the token's own. Upstream `Stop`, the shape of the mapping's [`StopRef`].
pub type Stop = StopRef;

/// A derived token: its family and its shade in each mode. Upstream
/// `DerivedToken`, the shape of the mapping's tokens.
pub type DerivedToken = MappedToken;

/// The stops a category fill may take, `[lowest, highest]` by mode: it is
/// also a card's tint, so it stays pale in light and deep in dark. Upstream
/// `TINT_STOPS`.
fn tint_range(mode: PaletteMode) -> [f64; 2] {
    match mode {
        PaletteMode::Light => [70.0, 95.0],
        PaletteMode::Dark => [10.0, 35.0],
    }
}

/// The category and chart colors a palette derives. Each set is always
/// written when the palette has its hues. Upstream `DerivedLook`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DerivedLook {
    /// The derived tokens, in the order written.
    pub tokens: MappedTokens,
    /// Whether the category colors were derived.
    pub badges: super::mapping::DerivedState,
    /// Whether the chart colors were derived.
    pub chart: super::mapping::DerivedState,
    /// Each limit no shade meets: a color still drawn alike, or one under its
    /// contrast at its most readable shade.
    pub limits: Vec<String>,
    /// Each color moved apart from an earlier one (a tint also from the page).
    pub moved: Vec<String>,
    /// Whether a color is still drawn like an earlier one.
    pub alike: bool,
    /// Whether a color is under its contrast at its most readable shade.
    pub unread: bool,
}

/// Core's category hues; gray is the neutral family's. Upstream
/// `CATEGORY_HUES`.
pub const CATEGORY_HUES: [&str; 10] = [
    "red", "orange", "yellow", "green", "teal", "cyan", "blue", "purple", "pink", "gray",
];

/// The chart's series hues, in Core's series order. Upstream
/// `CHART_SERIES`.
pub const CHART_SERIES: [&str; 10] = [
    "blue", "orange", "purple", "green", "pink", "cyan", "red", "teal", "brown", "indigo",
];

/// Each chart scale and the family it is drawn from. Upstream
/// `CHART_SCALES`.
pub const CHART_SCALES: [(&str, &str); 9] = [
    ("blue", "blue"),
    ("shamrock", "green"),
    ("orange", "orange"),
    ("pink", "pink"),
    ("purple", "purple"),
    ("red", "red"),
    ("teal", "teal"),
    ("yellow", "yellow"),
    ("gray", "neutral"),
];

/// The stops of a scale's five steps, pale to deep.
const SCALE_STOPS: [f64; 5] = [92.0, 75.0, 55.0, 35.0, 15.0];

/// The seed each derived hue grows from: the official neutral palette's
/// category seeds (a test keeps them equal), and Core's own chart colors
/// for the two series no category has. Upstream `HUE_SEEDS`.
pub const HUE_SEEDS: [(&str, &str); 11] = [
    ("red", "#d62830"),
    ("orange", "#d57113"),
    ("yellow", "#f8c723"),
    ("green", "#358a3a"),
    ("teal", "#0c7365"),
    ("cyan", "#0c6f82"),
    ("blue", "#0074e2"),
    ("purple", "#980fb2"),
    ("pink", "#b10e69"),
    ("brown", "#965E03"),
    ("indigo", "#6F8AFF"),
];

/// A hue's seed in [`HUE_SEEDS`].
fn hue_seed(id: &str) -> Option<&'static str> {
    HUE_SEEDS
        .iter()
        .find(|(h, _)| *h == id)
        .map(|(_, seed)| *seed)
}

/// Where each category token starts, `[light, dark]`, as the official
/// neutral theme maps it (a test keeps them equal). Upstream
/// `CATEGORY_STOPS`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CategoryStops {
    /// A category fill (and card tint).
    pub background: [f64; 2],
    /// A category border.
    pub border: [f64; 2],
    /// A category icon.
    pub icon: [f64; 2],
    /// A category's text.
    pub text: [f64; 2],
    /// The gray category's fill.
    pub gray_background: [f64; 2],
    /// The gray category's border.
    pub gray_border: [f64; 2],
    /// The gray category's icon.
    pub gray_icon: [f64; 2],
    /// The gray category's text.
    pub gray_text: [f64; 2],
}

/// Upstream `CATEGORY_STOPS`.
pub const CATEGORY_STOPS: CategoryStops = CategoryStops {
    background: [85.0, 25.0],
    border: [80.0, 65.0],
    icon: [25.0, 75.0],
    text: [25.0, 80.0],
    gray_background: [90.0, 20.0],
    gray_border: [85.0, 15.0],
    gray_icon: [30.0, 65.0],
    gray_text: [10.0, 85.0],
};

/// WCAG 2.2 AA by role.
const TEXT: f64 = 4.5;
const NON_TEXT: f64 = 3.0;
/// Two colors under this ΔE (×100) read as one.
const ALIKE: f64 = 2.0;
/// How close a card tint can be to another tint, or to a page, before they
/// read as one. Core's own red and orange tints sit about ΔE 2 apart.
const TINT_ALIKE: f64 = 1.5;
/// Below this chroma a seed is a gray.
const GRAY_CHROMA: f64 = 0.02;

/// How strong a color is for its lightness and hue, 0 to 1: its chroma over
/// the most sRGB holds there. Upstream `strength`.
fn strength(color: &str) -> Result<f64, InvalidColor> {
    let value = hex_to_oklch(color)?;
    let most = max_oklch_chroma(value.h, value.l);
    Ok(if most > 0.0 {
        js_min(1.0, value.c / most)
    } else {
        0.0
    })
}

/// How strong the look's colors are against Core's: the accent's strength
/// over the median of Core's hues, held to 0.25–1.2, in steps of 0.05 (so an
/// accent of about the same strength leaves them as they are); 1 without a
/// colored accent. Upstream `strengthScale`.
fn strength_scale(accent: Option<&str>) -> Result<f64, InvalidColor> {
    let mut core = Vec::with_capacity(HUE_SEEDS.len());
    for (_, seed) in HUE_SEEDS {
        core.push(strength(seed)?);
    }
    core.sort_by(|a, b| compare(a - b));
    let median = core[core.len() / 2];
    let Some(accent) = accent.filter(|a| !a.is_empty()) else {
        return Ok(1.0);
    };
    if hex_to_oklch(accent)?.c >= GRAY_CHROMA && median > 0.0 {
        Ok(js_round(js_min(1.2, js_max(0.25, strength(accent)? / median)) * 20.0) / 20.0)
    } else {
        Ok(1.0)
    }
}

/// The seeds of the derived hues, in [`HUE_SEEDS`] order: Core's hues at the
/// strength of the accent, turned a little toward the accent's and a tinted
/// neutral's hue; Core's strength without a colored accent. Upstream
/// `lookSeeds`.
pub fn look_seeds(
    accent: Option<&str>,
    neutral: Option<&str>,
) -> Result<Vec<(String, String)>, InvalidColor> {
    let scale = strength_scale(accent)?;
    let lean = hue_lean(accent, neutral)?;
    let mut out = Vec::with_capacity(HUE_SEEDS.len());
    for (h, seed) in HUE_SEEDS {
        let value = hex_to_oklch(seed)?;
        out.push((
            h.to_string(),
            oklch_clamped_hex(value.l, value.c * scale, lean(value.h)),
        ));
    }
    Ok(out)
}

/// The most a look turns a hue toward its accent, and toward its neutral.
const LEAN_TO_ACCENT: f64 = 10.0;
const LEAN_TO_NEUTRAL: f64 = 6.0;

/// How the look turns each hue: 15% of the way toward the accent's hue (at
/// most 10°) and 10% toward a tinted neutral's (at most 6°), so a slate
/// theme's colors run cool and a warm gray's warm, rounded to whole degrees
/// so a nearby color leaves the hues as they are. Upstream `hueLean`.
fn hue_lean(
    accent: Option<&str>,
    neutral: Option<&str>,
) -> Result<impl Fn(f64) -> f64, InvalidColor> {
    let toward =
        |c: Option<&str>, share: f64, most: f64| -> Result<Option<(f64, f64, f64)>, InvalidColor> {
            let Some(c) = c.filter(|c| !c.is_empty()) else {
                return Ok(None);
            };
            let value = hex_to_oklch(c)?;
            Ok((value.c >= GRAY_CHROMA).then_some((value.h, share, most)))
        };
    let turn = |lean: Option<(f64, f64, f64)>, hue: f64| match lean {
        None => 0.0,
        Some((h, share, most)) => {
            let gap = ((h - hue + 540.0) % 360.0) - 180.0;
            js_max(-most, js_min(most, gap * share))
        }
    };
    let a = toward(accent, 0.15, LEAN_TO_ACCENT)?;
    let n = toward(neutral, 0.1, LEAN_TO_NEUTRAL)?;
    Ok(move |hue: f64| (js_round(hue + turn(a, hue) + turn(n, hue)) + 360.0) % 360.0)
}

/// Where a look's card tints start, `[light, dark]`: a strong or dark accent
/// draws them a step deeper, a pale accent a step paler; the official
/// neutral theme's stops otherwise. Upstream `tintStops`.
pub fn tint_stops(accent: Option<&str>) -> Result<[f64; 2], InvalidColor> {
    let Some(accent) = accent.filter(|a| !a.is_empty()) else {
        return Ok(CATEGORY_STOPS.background);
    };
    let value = hex_to_oklch(accent)?;
    if value.c < GRAY_CHROMA {
        return Ok(CATEGORY_STOPS.background);
    }
    if strength_scale(Some(accent))? >= 1.1 || value.l < 0.4 {
        return Ok([80.0, 30.0]);
    }
    if value.l > 0.8 {
        return Ok([90.0, 20.0]);
    }
    Ok(CATEGORY_STOPS.background)
}

/// Whether a family is one [`look_seeds`] grows: an id of [`HUE_SEEDS`], a
/// name and a seed at Core's own lightness and hue for that id (only its
/// strength follows the look, its hue within the lean), and no other field.
/// Upstream `isLookFamily`.
pub fn is_look_family(family: &TonalPaletteFamilyInput) -> bool {
    let Some(core) = hue_seed(&family.id) else {
        return false;
    };
    if family.kind.is_some() || family.anchors.is_some() {
        return false;
    }
    let Ok(a) = hex_to_oklch(&family.seed) else {
        return false;
    };
    let Ok(b) = hex_to_oklch(core) else {
        return false;
    };
    let turn = (((a.h - b.h + 540.0) % 360.0) - 180.0).abs();
    // A weak seed's hue moves a little when written as hex.
    let slack =
        2.0 + LEAN_TO_ACCENT + LEAN_TO_NEUTRAL + (0.004_f64.atan2(js_max(a.c, 1e-6)) * 180.0) / PI;
    (a.l - b.l).abs() < 0.02 && (turn < slack || a.c < GRAY_CHROMA)
}

/// A family's color at a stop of a mode; an error where upstream reads
/// `undefined` (a family or mode the palette lacks).
fn shade_of(
    palette: &[TonalPaletteFamily],
    id: &str,
    mode: PaletteMode,
    stop: f64,
) -> Result<Stop, PaletteError> {
    let color = palette
        .iter()
        .find(|f| f.id == id)
        .and_then(|f| f.ramp(mode))
        .and_then(|ramp| ramp.get(stop))
        .ok_or_else(|| {
            PaletteError(format!(
                "The palette has no {id} {mode} shade at {}.",
                js_number_to_string(stop)
            ))
        })?;
    Ok(Stop {
        stop,
        color: color.to_string(),
        ..Stop::default()
    })
}

/// Whether the palette has a family.
fn has_family(palette: &[TonalPaletteFamily], id: &str) -> bool {
    palette.iter().any(|f| f.id == id)
}

/// The lowest contrast of a color on every ground (Infinity on none).
fn lowest(color: &str, on: &[String]) -> f64 {
    on.iter().fold(f64::INFINITY, |low, bg| {
        js_min(low, contrast_ratio(color, bg))
    })
}

/// The stop of `sorted` nearest `n`, the first on a tie.
fn near_in(sorted: &[f64], n: f64) -> f64 {
    sorted
        .iter()
        .fold(sorted.first().copied().unwrap_or(f64::NAN), |best, &s| {
            if (s - n).abs() < (best - n).abs() {
                s
            } else {
                best
            }
        })
}

/// The stops by distance from `s0`; ties go darker first in light and
/// lighter first in dark. Upstream `outward`.
fn outward(sorted: &[f64], mode: PaletteMode, s0: f64) -> Vec<f64> {
    let mut order = sorted.to_vec();
    order.sort_by(|&a, &b| {
        compare((a - s0).abs() - (b - s0).abs()).then_with(|| match mode {
            PaletteMode::Light => compare(a - b),
            PaletteMode::Dark => compare(b - a),
        })
    });
    order
}

/// The first `(stop, ratio)` that reaches `needs`, else the highest.
fn first_passing(scored: &[(f64, f64)], needs: f64) -> (f64, f64) {
    super::mapping::first_passing(scored, needs)
}

/// Both modes' values from one per mode the palette has: a mode it lacks
/// takes its first mode's. Upstream `each`.
fn each<T>(
    modes: &[PaletteMode],
    mut f: impl FnMut(PaletteMode) -> Result<T, PaletteError>,
) -> Result<[T; 2], PaletteError> {
    let pick = |m: PaletteMode| if modes.contains(&m) { m } else { modes[0] };
    let light = f(pick(PaletteMode::Light))?;
    let dark = f(pick(PaletteMode::Dark))?;
    Ok([light, dark])
}

/// What [`derive_look_colors`] reads. Upstream `DeriveLookInput`.
pub struct DeriveLookInput<'a> {
    /// Generated families.
    pub palette: &'a [TonalPaletteFamily],
    /// The palette's stops.
    pub stops: &'a [f64],
    /// The modes generated.
    pub modes: &'a [PaletteMode],
    /// The theme's text and secondary text colors (opaque hex) in a mode.
    pub text: &'a dyn Fn(PaletteMode) -> Vec<String>,
    /// The look's accent seed: how deep its card tints sit ([`tint_stops`]).
    pub accent: Option<&'a str>,
    /// The surfaces text sits on in a mode: page, surface, card (opaque hex).
    pub grounds: &'a dyn Fn(PaletteMode) -> Vec<String>,
    /// Whether a token is written (Core has it and the theme does not set it
    /// by hand); every token when `None`.
    pub wanted: Option<&'a dyn Fn(&str) -> bool>,
    /// The modes the theme shows: notes name only these. Every mode when
    /// `None`.
    pub shown: Option<&'a [PaletteMode]>,
}

/// A category's fill and the text and icon on it, with their ratios.
#[derive(Clone)]
struct Badge {
    fill: Stop,
    ink: (Stop, f64),
    icon: (Stop, f64),
}

/// The category and chart colors a palette derives. Upstream
/// `deriveLookColors`.
pub fn derive_look_colors(input: &DeriveLookInput<'_>) -> Result<DerivedLook, PaletteError> {
    let DeriveLookInput {
        palette,
        stops,
        modes,
        text,
        accent,
        grounds,
        wanted,
        shown,
    } = *input;
    let shown = shown.unwrap_or(modes);
    let wanted = |k: &str| wanted.is_none_or(|f| f(k));
    let tint = tint_stops(accent)?;
    let mut out = DerivedLook::default();
    let mut sorted = stops.to_vec();
    sorted.sort_by(|a, b| compare(a - b));
    let near = |n: f64| near_in(&sorted, n);
    let at = |id: &str, m: PaletteMode, stop: f64| shade_of(palette, id, m, stop);
    /// A limit no shade meets in a mode the theme shows: one note, however
    /// many times it is met.
    fn unmet(
        out: &mut DerivedLook,
        shown: &[PaletteMode],
        line: String,
        alike: bool,
        m: PaletteMode,
    ) {
        if !shown.contains(&m) {
            return;
        }
        if alike {
            out.alike = true;
        } else {
            out.unread = true;
        }
        if !out.limits.contains(&line) {
            out.limits.push(line);
        }
    }
    // The stop nearest start that reads at needs on every ground, searched
    // both ways, or the most contrasting one when none does; and its ratio.
    let nearest = |id: &str,
                   m: PaletteMode,
                   start: f64,
                   on: &[String],
                   needs: f64|
     -> Result<(Stop, f64), PaletteError> {
        let mut scored = Vec::new();
        for stop in outward(&sorted, m, near(start)) {
            scored.push((stop, lowest(&at(id, m, stop)?.color, on)));
        }
        let (stop, r) = first_passing(&scored, needs);
        Ok((at(id, m, stop)?, r))
    };
    // A color under its contrast at its most readable shade is a note.
    let note_short = |out: &mut DerivedLook, token: &str, m: PaletteMode, r: f64, needs: f64| {
        if r < needs {
            unmet(
                out,
                shown,
                format!(
                    "{token} reads at only {}:1 in {m} at its most readable shade (needs {}:1)",
                    js_number_to_string(ratio_down(r)),
                    js_number_to_string(needs)
                ),
                false,
                m,
            );
        }
    };
    let shift = |from: f64, to: f64| if to < from { "deeper" } else { "paler" };

    // Category colors: Badge and Token fill and text, Card tints. A hue is
    // drawn alike when its fill and text are both under ΔE 2 from an earlier
    // hue's, or its tint is under TINT_ALIKE from an earlier tint or a page.
    let has_neutral = has_family(palette, "neutral");
    let hues: Vec<&str> = CATEGORY_HUES
        .iter()
        .copied()
        .filter(|&h| h != "gray" || has_neutral)
        .collect();
    let fam_of = |h: &str| {
        if h == "gray" {
            "neutral".to_string()
        } else {
            h.to_string()
        }
    };
    if hues.iter().all(|h| has_family(palette, &fam_of(h))) {
        let start = |h: &str, role: &str, m: PaletteMode| -> f64 {
            let gray = h == "gray";
            let pair = match role {
                "background" if gray => CATEGORY_STOPS.gray_background,
                "border" if gray => CATEGORY_STOPS.gray_border,
                "icon" if gray => CATEGORY_STOPS.gray_icon,
                "text" if gray => CATEGORY_STOPS.gray_text,
                "background" => CATEGORY_STOPS.background,
                "border" => CATEGORY_STOPS.border,
                "icon" => CATEGORY_STOPS.icon,
                _ => CATEGORY_STOPS.text,
            };
            pair[mode_index(m)]
        };
        let badge_on = |h: &str, m: PaletteMode, fill_stop: f64| -> Result<Badge, PaletteError> {
            let id = fam_of(h);
            let fill = at(&id, m, fill_stop)?;
            let mut on = vec![fill.color.clone()];
            on.extend(grounds(m));
            Ok(Badge {
                ink: nearest(&id, m, start(h, "text", m), &on, TEXT)?,
                icon: nearest(&id, m, start(h, "icon", m), &on, NON_TEXT)?,
                fill,
            })
        };
        let mut drawn: Vec<[Option<Badge>; 2]> = vec![[None, None]; hues.len()];
        for &m in modes {
            // (hue, fill, ink) of the hues placed so far.
            let mut placed: Vec<(String, String, String)> = Vec::new();
            for (index, &h) in hues.iter().enumerate() {
                let id = fam_of(h);
                let s0 = near(if h == "gray" {
                    start(h, "background", m)
                } else {
                    tint[mode_index(m)]
                });
                // Paler first in light, darker first in dark, inside the
                // tint's stops: a tint, never a solid fill, since it is also
                // the card's.
                let [lo, hi] = tint_range(m);
                let mut range: Vec<f64> = sorted
                    .iter()
                    .copied()
                    .filter(|&x| x >= lo && x <= hi)
                    .collect();
                range.sort_by(|&a, &b| {
                    compare((a - s0).abs() - (b - s0).abs()).then_with(|| match m {
                        PaletteMode::Light => compare(b - a),
                        PaletteMode::Dark => compare(a - b),
                    })
                });
                let mut carrying = Vec::new();
                for x in range {
                    let tint_color = at(&id, m, x)?.color;
                    if text(m)
                        .iter()
                        .all(|c| contrast_ratio(c, &tint_color) >= TEXT)
                    {
                        carrying.push(x);
                    }
                }
                if carrying.is_empty() {
                    let s0_color = at(&id, m, s0)?.color;
                    let worst = text(m).iter().fold(f64::INFINITY, |low, c| {
                        js_min(low, contrast_ratio(c, &s0_color))
                    });
                    unmet(
                        &mut out,
                        shown,
                        format!(
                            "--color-background-{h} carries the theme's text at only {}:1 in {m} (needs {}:1)",
                            js_number_to_string(ratio_down(worst)),
                            js_number_to_string(TEXT)
                        ),
                        false,
                        m,
                    );
                }
                let like = |b: &Badge| -> Result<Option<String>, PaletteError> {
                    for (hue, fill, ink) in &placed {
                        let fill_delta = perceptual_delta(fill, &b.fill.color)?;
                        if fill_delta < TINT_ALIKE
                            || (fill_delta < ALIKE
                                && perceptual_delta(ink, &b.ink.0.color)? < ALIKE)
                        {
                            return Ok(Some(hue.clone()));
                        }
                    }
                    for g in grounds(m) {
                        if perceptual_delta(&g, &b.fill.color)? < TINT_ALIKE {
                            return Ok(Some("the page".to_string()));
                        }
                    }
                    Ok(None)
                };
                let first = carrying.first().copied().unwrap_or(s0);
                let mut badge = badge_on(h, m, first)?;
                if let Some(twin) = like(&badge)? {
                    // Paler first in light, darker first in dark: a moved tint
                    // stays as quiet as the look; a stronger one only when no
                    // quieter one stands apart.
                    let quieter = |x: f64| match m {
                        PaletteMode::Light => x > first,
                        PaletteMode::Dark => x < first,
                    };
                    let mut tries: Vec<f64> =
                        carrying.iter().copied().filter(|&x| quieter(x)).collect();
                    tries.extend(carrying.iter().skip(1).copied().filter(|&x| !quieter(x)));
                    let mut apart = None;
                    for s in tries {
                        let b = badge_on(h, m, s)?;
                        if like(&b)?.is_none() {
                            apart = Some(b);
                            break;
                        }
                    }
                    match apart {
                        None => unmet(
                            &mut out,
                            shown,
                            format!(
                                "--color-background-{h} is drawn like {twin} in {m}: no tint of it that carries the text stands apart"
                            ),
                            true,
                            m,
                        ),
                        Some(apart) => {
                            if shown.contains(&m) {
                                out.moved.push(format!(
                                    "--color-background-{h} takes a {} tint in {m}, apart from {twin}",
                                    shift(badge.fill.stop, apart.fill.stop)
                                ));
                            }
                            badge = apart;
                        }
                    }
                }
                note_short(&mut out, &format!("--color-text-{h}"), m, badge.ink.1, TEXT);
                note_short(
                    &mut out,
                    &format!("--color-icon-{h}"),
                    m,
                    badge.icon.1,
                    NON_TEXT,
                );
                placed.push((
                    h.to_string(),
                    badge.fill.color.clone(),
                    badge.ink.0.color.clone(),
                ));
                drawn[index][mode_index(m)] = Some(badge);
            }
        }
        for (index, &h) in hues.iter().enumerate() {
            let id = fam_of(h);
            let of = |m: PaletteMode| -> Result<Badge, PaletteError> {
                drawn[index][mode_index(m)]
                    .clone()
                    .ok_or_else(|| PaletteError(format!("No {h} badge in {m}.")))
            };
            let set = [
                (
                    format!("--color-background-{h}"),
                    each(modes, |m| Ok(of(m)?.fill))?,
                ),
                (
                    format!("--color-text-{h}"),
                    each(modes, |m| Ok(of(m)?.ink.0))?,
                ),
                (
                    format!("--color-icon-{h}"),
                    each(modes, |m| Ok(of(m)?.icon.0))?,
                ),
                (
                    format!("--color-border-{h}"),
                    each(modes, |m| at(&id, m, near(start(h, "border", m))))?,
                ),
            ];
            for (k, [light, dark]) in set {
                if wanted(&k) {
                    out.tokens.insert(
                        k,
                        DerivedToken {
                            family: id.clone(),
                            light,
                            dark,
                        },
                    );
                }
            }
        }
        out.badges = super::mapping::DerivedState::Derived;
    }

    // Chart colors: series at 3:1 on the page (graphics), a gray, and the
    // five-step scales. A series drawn like an earlier one takes the nearest
    // shade that reads at 3:1 and stands apart from every earlier series.
    if CHART_SERIES.iter().all(|h| has_family(palette, h)) {
        let mut chart = MappedTokens::new();
        let mut picked: Vec<[Stop; 2]> = Vec::with_capacity(CHART_SERIES.len());
        for h in CHART_SERIES {
            let token = format!("--color-data-categorical-{h}");
            picked.push(each(modes, |m| {
                let start = if m == PaletteMode::Light { 50.0 } else { 60.0 };
                let (stop, r) = nearest(h, m, start, &grounds(m), NON_TEXT)?;
                note_short(&mut out, &token, m, r, NON_TEXT);
                Ok(stop)
            })?);
        }
        for &m in modes {
            let mut placed: Vec<(&str, String)> = Vec::new();
            for (index, h) in CHART_SERIES.iter().enumerate() {
                let own = picked[index][mode_index(m)].clone();
                let mut like = None;
                for (hue, color) in &placed {
                    if perceptual_delta(color, &own.color)? < ALIKE {
                        like = Some(*hue);
                        break;
                    }
                }
                if let Some(like) = like {
                    let mut apart = None;
                    for stop in outward(&sorted, m, own.stop) {
                        let c = at(h, m, stop)?.color;
                        if lowest(&c, &grounds(m)) >= NON_TEXT {
                            let mut every = true;
                            for (_, color) in &placed {
                                if perceptual_delta(color, &c)? < ALIKE {
                                    every = false;
                                    break;
                                }
                            }
                            if every {
                                apart = Some(stop);
                                break;
                            }
                        }
                    }
                    match apart {
                        None => unmet(
                            &mut out,
                            shown,
                            format!(
                                "--color-data-categorical-{h} is drawn like {like} in {m}: no shade of it that reads at {}:1 stands apart",
                                js_number_to_string(NON_TEXT)
                            ),
                            true,
                            m,
                        ),
                        Some(apart) => {
                            picked[index][mode_index(m)] = at(h, m, apart)?;
                            if shown.contains(&m) {
                                out.moved.push(format!(
                                    "--color-data-categorical-{h} takes a {} shade in {m}, apart from {like}",
                                    shift(own.stop, apart)
                                ));
                            }
                        }
                    }
                }
                placed.push((h, picked[index][mode_index(m)].color.clone()));
            }
        }
        for (index, h) in CHART_SERIES.iter().enumerate() {
            // A one-mode theme writes its mode on both sides.
            let [light, dark] = each(modes, |m| Ok(picked[index][mode_index(m)].clone()))?;
            chart.insert(
                format!("--color-data-categorical-{h}"),
                DerivedToken {
                    family: h.to_string(),
                    light,
                    dark,
                },
            );
        }
        if has_neutral {
            let [light, dark] = each(modes, |m| {
                let (stop, r) = nearest("neutral", m, 55.0, &grounds(m), NON_TEXT)?;
                note_short(&mut out, "--color-data-neutral", m, r, NON_TEXT);
                Ok(stop)
            })?;
            chart.insert(
                "--color-data-neutral",
                DerivedToken {
                    family: "neutral".to_string(),
                    light,
                    dark,
                },
            );
        }
        for (scale, id) in CHART_SCALES {
            if has_family(palette, id) {
                for (i, stop) in SCALE_STOPS.iter().enumerate() {
                    let [light, dark] = each(modes, |m| at(id, m, near(*stop)))?;
                    chart.insert(
                        format!("--color-data-{scale}-{}", i + 1),
                        DerivedToken {
                            family: id.to_string(),
                            light,
                            dark,
                        },
                    );
                }
            }
        }
        for (k, v) in chart.iter() {
            if wanted(k) {
                out.tokens.insert(k, v.clone());
            }
        }
        out.chart = super::mapping::DerivedState::Derived;
    }
    Ok(out)
}

/// The status roles a palette derives for a look that leaves them unset:
/// each role's dial id and the word its tokens carry. Upstream
/// `STATUS_ROLES`.
pub const STATUS_ROLES: [(&str, &str); 4] = [
    ("info", "info"),
    ("success", "success"),
    ("warning", "warning"),
    ("danger", "error"),
];

/// A status fill under this ΔE (×100, an OKLab distance of 0.1) from the
/// accent's reads as the accent at a glance, as two button fills do.
const APART_FROM_ACCENT: f64 = 10.0;

/// The seeds of the status roles a look derives, in `base`'s order: each
/// role's own hue and lightness (the base theme's color for it, by dial id)
/// at the accent's strength. A role keeps its hue whatever the accent; only
/// its strength follows the look. Upstream `statusSeeds`.
pub fn status_seeds(
    base: &[(String, String)],
    accent: Option<&str>,
) -> Result<Vec<(String, String)>, InvalidColor> {
    let scale = strength_scale(accent)?;
    let mut out: Vec<(String, String)> = Vec::new();
    for (id, color) in base {
        let value = hex_to_oklch(color)?;
        let seed = oklch_clamped_hex(value.l, value.c * scale, value.h);
        match out.iter_mut().find(|(k, _)| k == id) {
            Some(entry) => entry.1 = seed,
            None => out.push((id.clone(), seed)),
        }
    }
    Ok(out)
}

/// Where each status token starts, `[light, dark]`: the fill at its middle,
/// the text, tint and text on the tint where the official neutral theme
/// maps them, icons and borders at the fill. Upstream `STATUS_STOPS`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StatusStops {
    /// A status fill.
    pub fill: [f64; 2],
    /// The warning fill, a bright one.
    pub warning_fill: [f64; 2],
    /// Status text.
    pub text: [f64; 2],
    /// A status tint.
    pub muted: [f64; 2],
    /// The dark tint's opacity: it is see-through, as the official neutral
    /// theme's is.
    pub muted_dark_alpha: &'static str,
    /// Text on a status tint.
    pub on_muted: [f64; 2],
}

/// Upstream `STATUS_STOPS`.
pub const STATUS_STOPS: StatusStops = StatusStops {
    fill: [50.0, 70.0],
    warning_fill: [85.0, 85.0],
    text: [45.0, 80.0],
    muted: [85.0, 75.0],
    muted_dark_alpha: "3D",
    on_muted: [25.0, 80.0],
};

/// What [`derive_status_colors`] reads. Upstream `DeriveStatusInput`.
pub struct DeriveStatusInput<'a> {
    /// Generated families; a role's family has the role's dial id.
    pub palette: &'a [TonalPaletteFamily],
    /// The palette's stops.
    pub stops: &'a [f64],
    /// The modes generated.
    pub modes: &'a [PaletteMode],
    /// The dial ids to derive (info, success, warning, danger).
    pub roles: &'a [String],
    /// Page, surface, card (opaque hex).
    pub grounds: &'a dyn Fn(PaletteMode) -> Vec<String>,
    /// The text the theme draws on a role's fill in a mode, preferred first
    /// (white or near-black), each with its family.
    pub inks: &'a dyn Fn(PaletteMode, &str) -> Vec<Stop>,
    /// The accent fill in a mode, kept apart from.
    pub accent: &'a dyn Fn(PaletteMode) -> Option<String>,
    /// Whether a token is written; every token when `None`.
    pub wanted: Option<&'a dyn Fn(&str) -> bool>,
    /// The modes the theme shows: notes name only these. Every mode when
    /// `None`.
    pub shown: Option<&'a [PaletteMode]>,
}

/// The status colors a palette derives, and each role's state. A token's
/// stop may name its own family (the text on a fill is the neutral color's,
/// or white or black). Upstream `DerivedStatus`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DerivedStatus {
    /// The derived tokens.
    pub tokens: MappedTokens,
    /// The roles derived and written, by dial id.
    pub roles: Vec<String>,
    /// What the roles gave up to stay written, and each role that keeps the
    /// base theme's colors because its text, icon or border reads at no
    /// shade.
    pub why: Vec<String>,
    /// The roles `why` names, by dial id.
    pub short: Vec<String>,
}

/// A status role's note-taker: the notes so far, the mode, the line, and
/// whether it is about text that reads at no shade.
type Fail<'f> = dyn FnMut(&mut Vec<String>, PaletteMode, String, bool) + 'f;

/// The status colors a palette derives for the roles a theme leaves unset.
/// Upstream `deriveStatusColors`.
pub fn derive_status_colors(input: &DeriveStatusInput<'_>) -> Result<DerivedStatus, PaletteError> {
    let DeriveStatusInput {
        palette,
        stops,
        modes,
        roles,
        grounds,
        inks,
        accent,
        wanted,
        shown,
    } = *input;
    let shown = shown.unwrap_or(modes);
    let wanted = |k: &str| wanted.is_none_or(|f| f(k));
    let mut out = DerivedStatus::default();
    let mut sorted: Vec<f64> = stops
        .iter()
        .copied()
        .filter(|&s| s > 0.0 && s < 100.0)
        .collect();
    sorted.sort_by(|a, b| compare(a - b));
    let near = |n: f64| near_in(&sorted, n);
    for id in roles {
        let Some(w) = STATUS_ROLES.iter().find(|(r, _)| r == id).map(|(_, w)| *w) else {
            continue;
        };
        if !has_family(palette, id) {
            continue;
        }
        let at = |m: PaletteMode, stop: f64| shade_of(palette, id, m, stop);
        let mut fails: Vec<String> = Vec::new();
        // Text or an icon that reads at no shade: the role keeps the base
        // theme's colors, which read.
        let mut unread = false;
        let mut fail = |fails: &mut Vec<String>, m: PaletteMode, line: String, text: bool| {
            if !shown.contains(&m) {
                return;
            }
            fails.push(line);
            if text {
                unread = true;
            }
        };
        // The stop nearest start that reads at needs on every color in bg.
        let pick = |fails: &mut Vec<String>,
                    fail: &mut Fail<'_>,
                    token: &str,
                    m: PaletteMode,
                    start: f64,
                    bg: &[String],
                    needs: f64|
         -> Result<Stop, PaletteError> {
            let order = outward(&sorted, m, near(start));
            let mut scored = Vec::with_capacity(order.len());
            for stop in order {
                scored.push((stop, lowest(&at(m, stop)?.color, bg)));
            }
            let (stop, r) = first_passing(&scored, needs);
            if r < needs {
                fail(
                    fails,
                    m,
                    format!(
                        "{token} reads at only {}:1 in {m} at its most readable shade (needs {}:1)",
                        js_number_to_string(ratio_down(r)),
                        js_number_to_string(needs)
                    ),
                    true,
                );
            }
            at(m, stop)
        };
        // The fill: nearest its middle that stands out from the page at 3:1
        // and carries white or near-black text at 4.5:1 (near-black first on
        // warning, white first otherwise). Warning is a bright fill that
        // carries near-black text, as Core's is: its text, not its edge,
        // makes it read, so it is not held to 3:1 on the page.
        let bright = id == "warning";
        let mut on_fill: [Option<Stop>; 2] = [None, None];
        let fill = each(modes, |m| {
            let starts = if bright {
                STATUS_STOPS.warning_fill
            } else {
                STATUS_STOPS.fill
            };
            let s0 = near(starts[mode_index(m)]);
            let mut order = sorted.clone();
            order.sort_by(|&a, &b| {
                compare((a - s0).abs() - (b - s0).abs()).then_with(|| compare(b - a))
            });
            let choices = inks(m, id);
            let a = accent(m);
            let stands_out = |c: &str| {
                bright
                    || grounds(m)
                        .iter()
                        .fold(f64::INFINITY, |low, g| js_min(low, contrast_ratio(c, g)))
                        >= NON_TEXT
            };
            let apart = |c: &str| -> Result<bool, PaletteError> {
                Ok(match a.as_deref().filter(|a| !a.is_empty()) {
                    None => true,
                    Some(a) => perceptual_delta(c, a)? >= APART_FROM_ACCENT,
                })
            };
            // (A bright fill already starts at the palest stop that reads as
            // a fill on a light page: one kept apart from the accent moves
            // deeper, never paler toward the page.)
            let allowed: Vec<f64> = order
                .into_iter()
                .filter(|&s| !(bright && m == PaletteMode::Light && s > s0))
                .collect();
            // The fill at a stop, with a note for each limit it gives up.
            let mut take = |fails: &mut Vec<String>,
                            s: f64,
                            ink: &Stop|
             -> Result<Stop, PaletteError> {
                let c = at(m, s)?.color;
                if !stands_out(&c) {
                    fail(
                        fails,
                        m,
                        format!(
                            "--color-{w} stands out from the page at under {}:1 in {m}",
                            js_number_to_string(NON_TEXT)
                        ),
                        false,
                    );
                }
                if !apart(&c)? {
                    fail(
                        fails,
                        m,
                        format!("--color-{w} is drawn like your accent in {m}"),
                        false,
                    );
                }
                let r = contrast_ratio(&ink.color, &c);
                if r < TEXT {
                    fail(
                        fails,
                        m,
                        format!(
                            "--color-on-{w} reads at only {}:1 on --color-{w} in {m} at its most readable shade (needs {}:1)",
                            js_number_to_string(ratio_down(r)),
                            js_number_to_string(TEXT)
                        ),
                        true,
                    );
                }
                on_fill[mode_index(m)] = Some(ink.clone());
                at(m, s)
            };
            // The limits, dropped one at a time when no stop meets them all:
            // standing out from the page first, then standing apart from the
            // accent. Text that reads on no stop keeps the role at the base
            // theme's colors.
            for keep in [2, 1, 0] {
                // The role's usual text first: the nearest stop it reads on,
                // before the other text on a nearer stop.
                for ink in &choices {
                    for &s in &allowed {
                        let c = at(m, s)?.color;
                        let ok = match keep {
                            2 => stands_out(&c) && apart(&c)?,
                            1 => apart(&c)?,
                            _ => true,
                        };
                        if ok && contrast_ratio(&ink.color, &c) >= TEXT {
                            return take(&mut fails, s, ink);
                        }
                    }
                }
            }
            // No stop carries readable text: the one that carries it best.
            let first_ink = choices
                .first()
                .cloned()
                .ok_or_else(|| PaletteError(format!("No text for --color-{w}.")))?;
            let mut best = allowed[0];
            for &y in &allowed[1..] {
                if contrast_ratio(&first_ink.color, &at(m, y)?.color)
                    > contrast_ratio(&first_ink.color, &at(m, best)?.color)
                {
                    best = y;
                }
            }
            take(&mut fails, best, &first_ink)
        })?;
        let muted = each(modes, |m| {
            let mut stop = near(STATUS_STOPS.muted[mode_index(m)]);
            // (A light tint is paler than its fill: where the fill already
            // sits at the tint's stop or paler, the tint takes the next paler
            // stop that is another color.)
            if m == PaletteMode::Light {
                let fill_here = &fill[mode_index(m)];
                let mut paler = Vec::new();
                for &s in &sorted {
                    if s > fill_here.stop && at(m, s)?.color != fill_here.color {
                        paler.push(s);
                    }
                }
                if stop <= fill_here.stop || at(m, stop)?.color == fill_here.color {
                    stop = paler.first().copied().unwrap_or(stop);
                }
            }
            let solid = at(m, stop)?;
            Ok(if m == PaletteMode::Dark {
                let alpha = STATUS_STOPS.muted_dark_alpha.to_lowercase();
                Stop {
                    stop,
                    color: format!("{}{alpha}", solid.color),
                    alpha: Some(alpha),
                    ..Stop::default()
                }
            } else {
                solid
            })
        })?;
        // The tint as it shows over each surface.
        let tint_seen = |m: PaletteMode| -> Vec<String> {
            let tint = &muted[mode_index(m)];
            match tint.alpha.as_deref() {
                Some(alpha) => {
                    let units: Vec<u16> = alpha.encode_utf16().collect();
                    let a = js_parse_int_hex(&units) / 255.0;
                    let top = hex_to_rgb_triple(tint.color.get(..7).unwrap_or(&tint.color));
                    grounds(m)
                        .iter()
                        .map(|g| rgb_triple_to_hex(composite_over(top, a, hex_to_rgb_triple(g))))
                        .collect()
                }
                None => vec![tint.color.clone()],
            }
        };
        let text = each(modes, |m| {
            let mut bg = grounds(m);
            bg.extend(tint_seen(m));
            pick(
                &mut fails,
                &mut fail,
                &format!("--color-text-{w}"),
                m,
                STATUS_STOPS.text[mode_index(m)],
                &bg,
                TEXT,
            )
        })?;
        let on_muted = each(modes, |m| {
            pick(
                &mut fails,
                &mut fail,
                &format!("--color-on-{w}-muted"),
                m,
                STATUS_STOPS.on_muted[mode_index(m)],
                &tint_seen(m),
                TEXT,
            )
        })?;
        let icon = each(modes, |m| {
            pick(
                &mut fails,
                &mut fail,
                &format!("--color-icon-{w}"),
                m,
                fill[mode_index(m)].stop,
                &grounds(m),
                NON_TEXT,
            )
        })?;
        let border = each(modes, |m| {
            pick(
                &mut fails,
                &mut fail,
                &format!("--color-border-{w}"),
                m,
                fill[mode_index(m)].stop,
                &grounds(m),
                NON_TEXT,
            )
        })?;
        if unread {
            out.why.push(format!(
                "{id} keeps the base theme's colors: no shade of its family reads"
            ));
            out.short.push(id.clone());
            continue;
        }
        // A one-mode theme measures its mode for both sides: one note each.
        for line in &fails {
            if !out.why.contains(line) {
                out.why.push(line.clone());
            }
        }
        if !fails.is_empty() {
            out.short.push(id.clone());
        }
        if !out.roles.contains(id) {
            out.roles.push(id.clone());
        }
        let set = [
            (format!("--color-{w}"), fill),
            (format!("--color-{w}-muted"), muted),
            (format!("--color-text-{w}"), text),
            (format!("--color-on-{w}-muted"), on_muted),
            (format!("--color-icon-{w}"), icon),
            (format!("--color-border-{w}"), border),
        ];
        for (k, [light, dark]) in set {
            if wanted(&k) {
                out.tokens.insert(
                    k,
                    DerivedToken {
                        family: id.clone(),
                        light,
                        dark,
                    },
                );
            }
        }
        let ink_token = format!("--color-on-{w}");
        if wanted(&ink_token) {
            let [Some(light), Some(dark)] = on_fill else {
                return Err(PaletteError(format!("No {ink_token} in both modes.")));
            };
            out.tokens.insert(
                ink_token,
                DerivedToken {
                    family: light.family.clone().unwrap_or_default(),
                    light,
                    dark,
                },
            );
        }
    }
    Ok(out)
}

/// Code colors, `(family, light stop, dark stop)` by syntax role, as the
/// official neutral theme maps them. The gray roles read the neutral.
/// Upstream `SYNTAX_STOPS`.
pub const SYNTAX_STOPS: [(&str, (&str, f64, f64)); 13] = [
    ("keyword", ("purple", 30.0, 80.0)),
    ("string", ("green", 30.0, 80.0)),
    ("comment", ("neutral", 35.0, 75.0)),
    ("number", ("orange", 30.0, 80.0)),
    ("function", ("blue", 30.0, 80.0)),
    ("type", ("purple", 30.0, 80.0)),
    ("variable", ("neutral", 5.0, 90.0)),
    ("operator", ("neutral", 35.0, 75.0)),
    ("constant", ("orange", 30.0, 80.0)),
    ("tag", ("red", 30.0, 80.0)),
    ("attribute", ("yellow", 30.0, 80.0)),
    ("property", ("teal", 30.0, 80.0)),
    ("punctuation", ("neutral", 35.0, 75.0)),
];

/// The code background's neutral stop by mode. Upstream `CODE_GROUND`.
fn code_ground(mode: PaletteMode) -> f64 {
    match mode {
        PaletteMode::Light => 100.0,
        PaletteMode::Dark => 5.0,
    }
}

/// The overlay scrim's neutral stop and opacity by mode. Upstream `SCRIM`.
fn scrim(mode: PaletteMode) -> (f64, &'static str) {
    match mode {
        PaletteMode::Light => (10.0, "80"),
        PaletteMode::Dark => (5.0, "cc"),
    }
}

/// What [`derive_code_colors`] reads. Upstream `DeriveCodeInput`.
pub struct DeriveCodeInput<'a> {
    /// Generated families.
    pub palette: &'a [TonalPaletteFamily],
    /// The palette's stops.
    pub stops: &'a [f64],
    /// The modes generated.
    pub modes: &'a [PaletteMode],
    /// The page colors code sits on, opaque hex.
    pub grounds: &'a dyn Fn(PaletteMode) -> Vec<String>,
    /// The code background the theme sets itself in a mode (opaque hex),
    /// which code colors read on instead of the neutral's.
    pub code_ground: Option<&'a dyn Fn(PaletteMode) -> Option<String>>,
    /// Whether a token is written; every token when `None`.
    pub wanted: Option<&'a dyn Fn(&str) -> bool>,
}

/// The code colors [`derive_code_colors`] derives, and the roles that cannot
/// read. Upstream's `{tokens, unreadable}`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DerivedCode {
    /// The derived tokens.
    pub tokens: MappedTokens,
    /// Each role that reads at no stop, as `<token> (<mode>)`.
    pub unreadable: Vec<String>,
}

/// The code colors and the overlay scrim a palette derives: each syntax role
/// from its hue's family, the nearest stop that reads at 4.5:1 on the code
/// background and on the page; the code background and the scrim from the
/// neutral. A role that cannot read keeps the base theme's. Upstream
/// `deriveCodeColors`, which writes a token's modes as `modes` lists them;
/// here a mode `modes` leaves out takes its first mode's shade, as
/// [`derive_look_colors`] does (the mapping always passes both).
pub fn derive_code_colors(input: &DeriveCodeInput<'_>) -> Result<DerivedCode, PaletteError> {
    let DeriveCodeInput {
        palette,
        stops,
        modes,
        grounds,
        code_ground: own_ground,
        wanted,
    } = *input;
    let wanted = |k: &str| wanted.is_none_or(|f| f(k));
    let mut out = DerivedCode::default();
    let mut sorted = stops.to_vec();
    sorted.sort_by(|a, b| compare(a - b));
    let near = |n: f64| near_in(&sorted, n);
    let at = |id: &str, m: PaletteMode, stop: f64| shade_of(palette, id, m, stop);
    let mut ground: [Option<Stop>; 2] = [None, None];
    if has_family(palette, "neutral") {
        for &m in modes {
            ground[mode_index(m)] = Some(at("neutral", m, near(code_ground(m)))?);
        }
        if wanted("--color-syntax-background") {
            let [light, dark] = each(modes, |m| {
                ground[mode_index(m)]
                    .clone()
                    .ok_or_else(|| PaletteError::new("No code background."))
            })?;
            out.tokens.insert(
                "--color-syntax-background",
                DerivedToken {
                    family: "neutral".to_string(),
                    light,
                    dark,
                },
            );
        }
        if wanted("--color-overlay") {
            let [light, dark] = each(modes, |m| {
                let (stop, alpha) = scrim(m);
                let s = near(stop);
                let c = at("neutral", m, s)?.color;
                Ok(Stop {
                    stop: s,
                    color: format!("{c}{alpha}"),
                    alpha: Some(alpha.to_string()),
                    ..Stop::default()
                })
            })?;
            out.tokens.insert(
                "--color-overlay",
                DerivedToken {
                    family: "neutral".to_string(),
                    light,
                    dark,
                },
            );
        }
    }
    for (role, (id, light, dark)) in SYNTAX_STOPS {
        let k = format!("--color-syntax-{role}");
        if !has_family(palette, id) || !wanted(&k) {
            continue;
        }
        let mut refs: [Option<Stop>; 2] = [None, None];
        let mut ok = true;
        for &m in modes {
            // A code background the theme sets itself is the one code shows on.
            let mut on: Vec<String> = match own_ground.and_then(|f| f(m)).filter(|g| !g.is_empty())
            {
                Some(own) => vec![own],
                None => ground[mode_index(m)]
                    .iter()
                    .map(|g| g.color.clone())
                    .collect(),
            };
            on.extend(grounds(m));
            let s0 = near(if m == PaletteMode::Light { light } else { dark });
            let mut found = None;
            for x in outward(&sorted, m, s0) {
                let c = at(id, m, x)?.color;
                if on.iter().all(|bg| contrast_ratio(&c, bg) >= TEXT) {
                    found = Some(x);
                    break;
                }
            }
            let Some(s) = found else {
                ok = false;
                out.unreadable.push(format!("{k} ({m})"));
                break;
            };
            refs[mode_index(m)] = Some(at(id, m, s)?);
        }
        if ok {
            let [light, dark] = each(modes, |m| {
                refs[mode_index(m)]
                    .clone()
                    .ok_or_else(|| PaletteError(format!("No {k} in {m}.")))
            })?;
            out.tokens.insert(
                k,
                DerivedToken {
                    family: id.to_string(),
                    light,
                    dark,
                },
            );
        }
    }
    Ok(out)
}

/// Which color vision a pair of fills may be confused under: upstream's
/// `'red-weak' | 'green-weak'`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CvdVision {
    /// `red-weak`: toward protanopia.
    RedWeak,
    /// `green-weak`: toward deuteranopia.
    GreenWeak,
}

impl CvdVision {
    /// Upstream's spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::RedWeak => "red-weak",
            Self::GreenWeak => "green-weak",
        }
    }
}

/// How full red-blind (protanopia) and green-blind (deuteranopia) vision maps
/// linear sRGB (Machado, Oliveira and Fernandes, 2009, severity 1). Anyone
/// red-weak or green-weak sees between these and normal vision. Upstream
/// `CVD`.
const CVD: [(CvdVision, [[f64; 3]; 3]); 2] = [
    (
        CvdVision::RedWeak,
        [
            [0.152286, 1.052583, -0.204868],
            [0.114503, 0.786281, 0.099216],
            [-0.003882, -0.048116, 1.051998],
        ],
    ),
    (
        CvdVision::GreenWeak,
        [
            [0.367322, 0.860646, -0.227968],
            [0.280085, 0.672501, 0.047413],
            [-0.01182, 0.04294, 0.968881],
        ],
    ),
];

/// How far toward full red- or green-blindness the check looks.
const CVD_SEVERITY: f64 = 0.6;

/// Two fills closer than this (OKLab distance) read as one color.
const CVD_ALIKE: f64 = 0.06;

/// `Math.hypot` of any count of numbers as V8 computes it: each scaled by
/// the largest and the squares summed with Kahan compensation (the
/// any-count form of `js::js_hypot`).
fn js_hypot_all(values: &[f64]) -> f64 {
    let mut max: f64 = 0.0;
    let mut any_nan = false;
    for value in values {
        if value.is_nan() {
            any_nan = true;
        } else if value.abs() > max {
            max = value.abs();
        }
    }
    if max == f64::INFINITY {
        return f64::INFINITY;
    }
    if any_nan {
        return f64::NAN;
    }
    if max == 0.0 {
        return 0.0;
    }
    let mut sum: f64 = 0.0;
    let mut compensation = 0.0;
    for value in values {
        let n = value.abs() / max;
        let summand = n * n - compensation;
        let preliminary = sum + summand;
        compensation = (preliminary - sum) - summand;
        sum = preliminary;
    }
    sum.sqrt() * max
}

/// A pair of status fills someone red-weak or green-weak may not tell apart.
/// Upstream's `{a, b, vision}`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CvdPair {
    /// One role.
    pub a: String,
    /// The other.
    pub b: String,
    /// The vision that may confuse them.
    pub vision: CvdVision,
}

/// The pairs of status fills that someone red-weak or green-weak may not
/// tell apart: their colors as that vision sees them sit closer than
/// `CVD_ALIKE`. `fills` holds opaque hex by role; a value that is not a
/// six-digit hex is skipped. Upstream `cvdClashes`.
pub fn cvd_clashes(fills: &[(String, String)]) -> Vec<CvdPair> {
    let mut out = Vec::new();
    let roles: Vec<&(String, String)> = fills
        .iter()
        .filter(|(_, hex)| {
            hex.len() == 7
                && hex.starts_with('#')
                && hex[1..].bytes().all(|b| b.is_ascii_hexdigit())
        })
        .collect();
    for (vision, matrix) in CVD {
        // Weak, not absent: partway from normal vision to the full matrix.
        let mut weak = [[0.0; 3]; 3];
        for (r, row) in matrix.iter().enumerate() {
            for (c, v) in row.iter().enumerate() {
                let identity = if r == c { 1.0 } else { 0.0 };
                weak[r][c] = identity + CVD_SEVERITY * (v - identity);
            }
        }
        let seen = |hex: &str| {
            let lin = hex_to_linear_rgb(hex);
            let [r, g, b] = weak.map(|row| {
                js_min(
                    1.0,
                    js_max(0.0, row[0] * lin[0] + row[1] * lin[1] + row[2] * lin[2]),
                )
            });
            linear_rgb_to_oklab(r, g, b)
        };
        for i in 0..roles.len() {
            for j in i + 1..roles.len() {
                let x = seen(&roles[i].1);
                let y = seen(&roles[j].1);
                if js_hypot_all(&[x[0] - y[0], x[1] - y[1], x[2] - y[2]]) < CVD_ALIKE {
                    out.push(CvdPair {
                        a: roles[i].0.clone(),
                        b: roles[j].0.clone(),
                        vision,
                    });
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::super::generator::generate_tonal_palette;
    use super::super::mapping::test_support::{assert_rendered, token_line};
    use super::super::types::{
        FamilyKind, ModeStrategy, NeutralProfile, TonalPaletteGenerationInput,
    };
    use super::*;

    // Expected values: derive-look.ts run under Node 24.

    fn seeds(accent: Option<&str>, neutral: Option<&str>) -> String {
        look_seeds(accent, neutral)
            .unwrap()
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join(",")
    }

    #[test]
    fn look_seeds_follow_the_accents_strength_and_lean() {
        assert_eq!(
            seeds(None, None),
            "red=#d62830,orange=#d57114,yellow=#f8c723,green=#368a39,teal=#0c7365,cyan=#0d6f82,blue=#0074e2,purple=#980fb2,pink=#b10e69,brown=#965e04,indigo=#6e8aff"
        );
        assert_eq!(
            seeds(Some("#5b3cc4"), Some("#6b6b70")),
            "red=#c14759,orange=#c97751,yellow=#f8c468,green=#3a8758,teal=#2f6f6b,cyan=#376b7f,blue=#4275c9,purple=#873aa5,pink=#9b3877,brown=#915f36,indigo=#7c8de5"
        );
        assert_eq!(
            seeds(Some("#e0a100"), Some("#706a62")),
            "red=#d72a00,orange=#d27300,yellow=#fdc500,green=#4b881d,teal=#16745b,cyan=#00717b,blue=#007dc5,purple=#a3009f,pink=#b70054,brown=#955f00,indigo=#8482ff"
        );
        // A gray accent leaves Core's strength and hue.
        assert_eq!(seeds(Some("#cccccc"), None), seeds(None, None));
        assert_eq!(
            seeds(Some("#0074e2"), Some("#475569")),
            "red=#d7175c,orange=#df653b,yellow=#ddd320,green=#008c5a,teal=#007273,cyan=#136d8a,blue=#0074e2,purple=#831bcf,pink=#a8128a,brown=#a1551c,indigo=#658dff"
        );
    }

    #[test]
    fn tint_stops_sit_deeper_for_a_strong_or_dark_accent_and_paler_for_a_pale_one() {
        assert_eq!(tint_stops(None), Ok([85.0, 25.0]));
        assert_eq!(tint_stops(Some("#5b3cc4")), Ok([85.0, 25.0]));
        assert_eq!(tint_stops(Some("#fde047")), Ok([90.0, 20.0]));
        assert_eq!(tint_stops(Some("#1e293b")), Ok([80.0, 30.0]));
        assert_eq!(tint_stops(Some("#ff0000")), Ok([85.0, 25.0]));
        assert_eq!(tint_stops(Some("#bbbbbb")), Ok([85.0, 25.0]));
        assert_eq!(tint_stops(Some("#0074e2")), Ok([85.0, 25.0]));
    }

    #[test]
    fn is_look_family_knows_the_families_look_seeds_grows() {
        let family = |id: &str, name: Option<&str>, seed: &str, kind: Option<FamilyKind>| {
            TonalPaletteFamilyInput {
                id: id.to_string(),
                name: name.map(str::to_string),
                seed: seed.to_string(),
                kind,
                anchors: None,
            }
        };
        assert!(is_look_family(&family("red", Some("Red"), "#d62830", None)));
        assert!(!is_look_family(&family("red", None, "#c0392b", None)));
        assert!(!is_look_family(&family("blue", None, "#ff0000", None)));
        assert!(!is_look_family(&family(
            "teal",
            Some("Teal"),
            "#0c7365",
            Some(FamilyKind::Chromatic)
        )));
        assert!(!is_look_family(&family("brand", None, "#d62830", None)));
        assert!(is_look_family(&family("yellow", None, "#f4c430", None)));
        assert!(is_look_family(&family(
            "indigo",
            Some("Indigo"),
            "#6f8aff",
            None
        )));
    }

    #[test]
    fn status_seeds_keep_each_roles_hue_at_the_accents_strength() {
        let base: Vec<(String, String)> = [
            ("success", "#0d8626"),
            ("warning", "#e9af08"),
            ("danger", "#e3193b"),
            ("info", "#0064e0"),
        ]
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
        let seeds = |accent| {
            status_seeds(&base, accent)
                .unwrap()
                .iter()
                .map(|(k, v)| format!("{k}={v}"))
                .collect::<Vec<_>>()
                .join(",")
        };
        assert_eq!(
            seeds(None),
            "success=#0d8626,warning=#e9af08,danger=#e3193b,info=#0064e0"
        );
        assert_eq!(
            seeds(Some("#5b3cc4")),
            "success=#39813e,warning=#ddb357,danger=#cd474e,info=#2f6ac4"
        );
        assert_eq!(
            seeds(Some("#ff0000")),
            "success=#008721,warning=#e9af00,danger=#e70037,info=#0064e1"
        );
    }

    #[test]
    fn cvd_clashes_find_fills_red_or_green_weak_vision_confuses() {
        let fills = |pairs: &[(&str, &str)]| -> Vec<(String, String)> {
            pairs
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect()
        };
        let clashes = |pairs: &[(&str, &str)]| -> Vec<String> {
            cvd_clashes(&fills(pairs))
                .iter()
                .map(|c| format!("{}/{}/{}", c.a, c.b, c.vision.as_str()))
                .collect()
        };
        assert!(
            clashes(&[
                ("success", "#0d8626"),
                ("danger", "#e3193b"),
                ("warning", "#e9af08"),
                ("info", "#0064e0")
            ])
            .is_empty()
        );
        assert_eq!(
            clashes(&[
                ("success", "#4a7a00"),
                ("danger", "#8a5a00"),
                ("info", "rgb(1,2,3)")
            ]),
            ["success/danger/green-weak"]
        );
        assert_eq!(
            clashes(&[("a", "#777777"), ("b", "#787878")]),
            ["a/b/red-weak", "a/b/green-weak"]
        );
    }

    #[test]
    fn js_hypot_all_sums_as_v8_does() {
        assert_eq!(js_hypot_all(&[3.0, 4.0, 12.0]), 13.0);
        assert_eq!(js_hypot_all(&[0.0, -0.0, 0.0]), 0.0);
        assert!(js_hypot_all(&[f64::NAN, 1.0, 2.0]).is_nan());
        assert_eq!(js_hypot_all(&[f64::NAN, f64::INFINITY, 2.0]), f64::INFINITY);
    }

    /// A dark-only palette of the look's hues, the statuses and a neutral.
    fn dark_palette() -> super::super::types::TonalPaletteCandidate {
        let mut families = vec![TonalPaletteFamilyInput {
            id: "neutral".into(),
            seed: "#6b6b70".into(),
            kind: Some(FamilyKind::Neutral),
            ..TonalPaletteFamilyInput::default()
        }];
        for (id, seed) in look_seeds(Some("#5b3cc4"), Some("#6b6b70")).unwrap() {
            families.push(TonalPaletteFamilyInput {
                id,
                seed,
                ..TonalPaletteFamilyInput::default()
            });
        }
        for (id, seed) in [
            ("success", "#1f8a3b"),
            ("warning", "#e0a100"),
            ("info", "#0a6cd6"),
            ("danger", "#d1242f"),
        ] {
            families.push(TonalPaletteFamilyInput {
                id: id.into(),
                seed: seed.into(),
                ..TonalPaletteFamilyInput::default()
            });
        }
        generate_tonal_palette(&TonalPaletteGenerationInput {
            mode_strategy: Some(ModeStrategy::DarkOnly),
            neutral_profile: Some(NeutralProfile::Custom),
            families,
            ..TonalPaletteGenerationInput::default()
        })
        .unwrap()
    }

    /// Upstream's category, chart and code colors of the dark-only palette,
    /// a one-mode theme writing its mode on both sides.
    const DARK_LOOK: &str = r##"token --color-background-red red 25 #6e1025 - - 0 | 25 #6e1025 - - 0
token --color-text-red red 80 #ffadb2 - - 0 | 80 #ffadb2 - - 0
token --color-icon-red red 75 #ff969f - - 0 | 75 #ff969f - - 0
token --color-border-red red 65 #e77682 - - 0 | 65 #e77682 - - 0
token --color-background-orange orange 25 #6b1d00 - - 0 | 25 #6b1d00 - - 0
token --color-text-orange orange 80 #ffb18e - - 0 | 80 #ffb18e - - 0
token --color-icon-orange orange 75 #f7a079 - - 0 | 75 #f7a079 - - 0
token --color-border-orange orange 65 #de8257 - - 0 | 65 #de8257 - - 0
token --color-background-yellow yellow 25 #503600 - - 0 | 25 #503600 - - 0
token --color-text-yellow yellow 80 #eabf76 - - 0 | 80 #eabf76 - - 0
token --color-icon-yellow yellow 75 #deb161 - - 0 | 75 #deb161 - - 0
token --color-border-yellow yellow 65 #c59437 - - 0 | 65 #c59437 - - 0
token --color-background-green green 25 #004825 - - 0 | 25 #004825 - - 0
token --color-text-green green 80 #a4d4b3 - - 0 | 80 #a4d4b3 - - 0
token --color-icon-green green 75 #91c8a2 - - 0 | 75 #91c8a2 - - 0
token --color-border-green green 65 #6cb083 - - 0 | 65 #6cb083 - - 0
token --color-background-teal teal 25 #004642 - - 0 | 25 #004642 - - 0
token --color-text-teal teal 80 #9ad4cf - - 0 | 80 #9ad4cf - - 0
token --color-icon-teal teal 75 #81c8c3 - - 0 | 75 #81c8c3 - - 0
token --color-border-teal teal 65 #50b0aa - - 0 | 65 #50b0aa - - 0
token --color-background-cyan cyan 25 #004256 - - 0 | 25 #004256 - - 0
token --color-text-cyan cyan 80 #8ed2ed - - 0 | 80 #8ed2ed - - 0
token --color-icon-cyan cyan 75 #7bc5e2 - - 0 | 75 #7bc5e2 - - 0
token --color-border-cyan cyan 65 #57aaca - - 0 | 65 #57aaca - - 0
token --color-background-blue blue 25 #0d377a - - 0 | 25 #0d377a - - 0
token --color-text-blue blue 80 #a6c8ff - - 0 | 80 #a6c8ff - - 0
token --color-icon-blue blue 75 #8fbaff - - 0 | 75 #8fbaff - - 0
token --color-border-blue blue 65 #6b9ef1 - - 0 | 65 #6b9ef1 - - 0
token --color-background-purple purple 25 #522265 - - 0 | 25 #522265 - - 0
token --color-text-purple purple 80 #e2b1f8 - - 0 | 80 #e2b1f8 - - 0
token --color-icon-purple purple 75 #d6a1ed - - 0 | 75 #d6a1ed - - 0
token --color-border-purple purple 65 #bc84d5 - - 0 | 65 #bc84d5 - - 0
token --color-background-pink pink 25 #65164a - - 0 | 25 #65164a - - 0
token --color-text-pink pink 80 #fda8d8 - - 0 | 80 #fda8d8 - - 0
token --color-icon-pink pink 75 #f297cb - - 0 | 75 #f297cb - - 0
token --color-border-pink pink 65 #d979b2 - - 0 | 65 #d979b2 - - 0
token --color-background-gray neutral 20 #303033 - - 0 | 20 #303033 - - 0
token --color-text-gray neutral 85 #d4d4d7 - - 0 | 85 #d4d4d7 - - 0
token --color-icon-gray neutral 65 #9d9da1 - - 0 | 65 #9d9da1 - - 0
token --color-border-gray neutral 15 #252528 - - 0 | 15 #252528 - - 0
token --color-data-categorical-blue blue 60 #5c90e5 - - 0 | 60 #5c90e5 - - 0
token --color-data-categorical-orange orange 60 #d17448 - - 0 | 60 #d17448 - - 0
token --color-data-categorical-purple purple 60 #af76c8 - - 0 | 60 #af76c8 - - 0
token --color-data-categorical-green green 60 #5ca375 - - 0 | 60 #5ca375 - - 0
token --color-data-categorical-pink pink 60 #cc6aa5 - - 0 | 60 #cc6aa5 - - 0
token --color-data-categorical-cyan cyan 60 #469dbd - - 0 | 60 #469dbd - - 0
token --color-data-categorical-red red 60 #d96875 - - 0 | 60 #d96875 - - 0
token --color-data-categorical-teal teal 60 #3aa39d - - 0 | 60 #3aa39d - - 0
token --color-data-categorical-brown brown 60 #c77c3c - - 0 | 60 #c77c3c - - 0
token --color-data-categorical-indigo indigo 60 #7889e3 - - 0 | 60 #7889e3 - - 0
token --color-data-neutral neutral 55 #838387 - - 0 | 55 #838387 - - 0
token --color-data-blue-1 blue 90 #d3e4ff - - 0 | 90 #d3e4ff - - 0
token --color-data-blue-2 blue 75 #8fbaff - - 0 | 75 #8fbaff - - 0
token --color-data-blue-3 blue 55 #4f82d7 - - 0 | 55 #4f82d7 - - 0
token --color-data-blue-4 blue 35 #204f9b - - 0 | 35 #204f9b - - 0
token --color-data-blue-5 blue 15 #022257 - - 0 | 15 #022257 - - 0
token --color-data-shamrock-1 green 90 #cbecd4 - - 0 | 90 #cbecd4 - - 0
token --color-data-shamrock-2 green 75 #91c8a2 - - 0 | 75 #91c8a2 - - 0
token --color-data-shamrock-3 green 55 #4d9668 - - 0 | 55 #4d9668 - - 0
token --color-data-shamrock-4 green 35 #196239 - - 0 | 35 #196239 - - 0
token --color-data-shamrock-5 green 15 #002f15 - - 0 | 15 #002f15 - - 0
token --color-data-orange-1 orange 90 #ffd9c9 - - 0 | 90 #ffd9c9 - - 0
token --color-data-orange-2 orange 75 #f7a079 - - 0 | 75 #f7a079 - - 0
token --color-data-orange-3 orange 55 #c46639 - - 0 | 55 #c46639 - - 0
token --color-data-orange-4 orange 35 #8a330e - - 0 | 35 #8a330e - - 0
token --color-data-orange-5 orange 15 #490c00 - - 0 | 15 #490c00 - - 0
token --color-data-pink-1 pink 90 #ffd4eb - - 0 | 90 #ffd4eb - - 0
token --color-data-pink-2 pink 75 #f297cb - - 0 | 75 #f297cb - - 0
token --color-data-pink-3 pink 55 #be5c98 - - 0 | 55 #be5c98 - - 0
token --color-data-pink-4 pink 35 #832b64 - - 0 | 35 #832b64 - - 0
token --color-data-pink-5 pink 15 #460531 - - 0 | 15 #460531 - - 0
token --color-data-purple-1 purple 90 #f2d7ff - - 0 | 90 #f2d7ff - - 0
token --color-data-purple-2 purple 75 #d6a1ed - - 0 | 75 #d6a1ed - - 0
token --color-data-purple-3 purple 55 #a268bb - - 0 | 55 #a268bb - - 0
token --color-data-purple-4 purple 35 #6d3782 - - 0 | 35 #6d3782 - - 0
token --color-data-purple-5 purple 15 #371146 - - 0 | 15 #371146 - - 0
token --color-data-red-1 red 90 #ffd7d9 - - 0 | 90 #ffd7d9 - - 0
token --color-data-red-2 red 75 #ff969f - - 0 | 75 #ff969f - - 0
token --color-data-red-3 red 55 #cb5968 - - 0 | 55 #cb5968 - - 0
token --color-data-red-4 red 35 #8f2639 - - 0 | 35 #8f2639 - - 0
token --color-data-red-5 red 15 #4d0014 - - 0 | 15 #4d0014 - - 0
token --color-data-teal-1 teal 90 #c6ebe8 - - 0 | 90 #c6ebe8 - - 0
token --color-data-teal-2 teal 75 #81c8c3 - - 0 | 75 #81c8c3 - - 0
token --color-data-teal-3 teal 55 #249690 - - 0 | 55 #249690 - - 0
token --color-data-teal-4 teal 35 #00605c - - 0 | 35 #00605c - - 0
token --color-data-teal-5 teal 15 #002d2b - - 0 | 15 #002d2b - - 0
token --color-data-yellow-1 yellow 90 #ffdda5 - - 0 | 90 #ffdda5 - - 0
token --color-data-yellow-2 yellow 75 #deb161 - - 0 | 75 #deb161 - - 0
token --color-data-yellow-3 yellow 55 #ab7900 - - 0 | 55 #ab7900 - - 0
token --color-data-yellow-4 yellow 35 #6d4b00 - - 0 | 35 #6d4b00 - - 0
token --color-data-yellow-5 yellow 15 #352200 - - 0 | 15 #352200 - - 0
token --color-data-gray-1 neutral 90 #e2e2e5 - - 0 | 90 #e2e2e5 - - 0
token --color-data-gray-2 neutral 75 #b8b8bc - - 0 | 75 #b8b8bc - - 0
token --color-data-gray-3 neutral 55 #838387 - - 0 | 55 #838387 - - 0
token --color-data-gray-4 neutral 35 #525255 - - 0 | 35 #525255 - - 0
token --color-data-gray-5 neutral 15 #252528 - - 0 | 15 #252528 - - 0
state derived derived false false
code --color-syntax-background neutral none | 5 #111112 - - 0
code --color-overlay neutral none | 5 #111112cc - cc 0
code --color-syntax-keyword purple none | 80 #e2b1f8 - - 0
code --color-syntax-string green none | 80 #a4d4b3 - - 0
code --color-syntax-comment neutral none | 75 #b8b8bc - - 0
code --color-syntax-number orange none | 80 #ffb18e - - 0
code --color-syntax-function blue none | 80 #a6c8ff - - 0
code --color-syntax-type purple none | 80 #e2b1f8 - - 0
code --color-syntax-variable neutral none | 90 #e2e2e5 - - 0
code --color-syntax-operator neutral none | 75 #b8b8bc - - 0
code --color-syntax-constant orange none | 80 #ffb18e - - 0
code --color-syntax-tag red none | 80 #ffadb2 - - 0
code --color-syntax-attribute yellow none | 80 #eabf76 - - 0
code --color-syntax-property teal none | 80 #9ad4cf - - 0
code --color-syntax-punctuation neutral none | 75 #b8b8bc - - 0"##;

    #[test]
    fn derives_category_chart_and_code_colors_for_a_one_mode_theme_as_upstream() {
        let palette = dark_palette();
        let text = |_: PaletteMode| vec!["#ffffff".to_string(), "#c6c6c6".to_string()];
        let grounds = |_: PaletteMode| vec!["#111111".to_string(), "#1b1b1b".to_string()];
        let look = derive_look_colors(&DeriveLookInput {
            palette: &palette.palette,
            stops: &palette.stops,
            modes: &[PaletteMode::Dark],
            text: &text,
            accent: Some("#5b3cc4"),
            grounds: &grounds,
            wanted: None,
            shown: None,
        })
        .unwrap();
        let mut lines: Vec<String> = look
            .tokens
            .iter()
            .map(|(k, v)| token_line("token", k, v))
            .collect();
        lines.push(format!(
            "state {} {} {} {}",
            look.badges.as_str(),
            look.chart.as_str(),
            look.alike,
            look.unread
        ));
        lines.extend(look.limits.iter().map(|l| format!("limit {l}")));
        lines.extend(look.moved.iter().map(|l| format!("moved {l}")));
        let code = derive_code_colors(&DeriveCodeInput {
            palette: &palette.palette,
            stops: &palette.stops,
            modes: &[PaletteMode::Dark],
            grounds: &grounds,
            code_ground: None,
            wanted: None,
        })
        .unwrap();
        lines.extend(code.tokens.iter().map(|(k, v)| token_line("code", k, v)));
        lines.extend(code.unreadable.iter().map(|u| format!("unreadable {u}")));
        // Upstream's code colors write only the modes the palette has (the
        // light side is "none"); here a one-mode theme's mode stands for both,
        // as its category and chart colors do upstream.
        let expected: Vec<String> = DARK_LOOK
            .lines()
            .map(|line| match line.strip_prefix("code ") {
                Some(rest) if rest.contains(" none | ") => {
                    let (head, dark) = rest.split_once(" none | ").unwrap();
                    format!("code {head} {dark} | {dark}")
                }
                _ => line.to_string(),
            })
            .collect();
        assert_rendered(&lines.join("\n"), &expected.join("\n"));
    }

    #[test]
    fn derive_status_colors_refuses_a_one_mode_text_on_fill_as_upstream_throws() {
        // Upstream reads onFill.light.family, which a dark-only theme never
        // sets: a TypeError there, an error here.
        let palette = dark_palette();
        let grounds = |_: PaletteMode| vec!["#111111".to_string(), "#1b1b1b".to_string()];
        let inks = |_: PaletteMode, _: &str| {
            vec![
                Stop {
                    stop: 5.0,
                    color: "#111111".into(),
                    family: Some("neutral".into()),
                    ..Stop::default()
                },
                Stop {
                    stop: 100.0,
                    color: "#ffffff".into(),
                    family: Some("white".into()),
                    ..Stop::default()
                },
            ]
        };
        let accent = |_: PaletteMode| Some("#7b5ce4".to_string());
        let roles: Vec<String> = ["info", "success", "warning", "danger"]
            .iter()
            .map(|r| r.to_string())
            .collect();
        let result = derive_status_colors(&DeriveStatusInput {
            palette: &palette.palette,
            stops: &palette.stops,
            modes: &[PaletteMode::Dark],
            roles: &roles,
            grounds: &grounds,
            inks: &inks,
            accent: &accent,
            wanted: None,
            shown: None,
        });
        assert!(result.is_err());
    }

    #[test]
    fn category_seeds_and_stops_equal_the_official_neutral_palette() {
        // HUE_SEEDS' category seeds are the Neutral palette's (a test upstream
        // keeps them equal to packages/themes/neutral/palette.config.json).
        let spec = super::super::generator::spec_json::spec_palette("neutral");
        let request = spec.get("request").unwrap();
        for family in request.get("families").unwrap().arr() {
            let id = family.get("id").unwrap().str();
            if id == "neutral" {
                continue;
            }
            assert_eq!(
                hue_seed(id),
                Some(family.get("seed").unwrap().str()),
                "{id}"
            );
        }
    }
}
