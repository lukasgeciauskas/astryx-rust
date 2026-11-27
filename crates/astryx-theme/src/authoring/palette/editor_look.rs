//! The theme editor's own rules: upstream `palette/editor-look.ts`.
//!
//! The colors the theme editor grows beyond `themePalette()`'s mapping, as
//! the mapping's `context.editor`: the look's category, chart and code
//! colors, the status roles the theme leaves unset (each from the palette's
//! families, by [`derive_look`](super::derive_look)), the status fills
//! someone red-weak or green-weak may not tell apart, and a base theme's own
//! text/fill pairs kept readable where a look color would break them. Its
//! own module, as upstream's is, so a mapping without an editor reads none
//! of it.

use super::super::color::composite_over;
use super::derive_look::{
    CATEGORY_HUES, DeriveCodeInput, DeriveLookInput, DeriveStatusInput, Stop, cvd_clashes,
    derive_code_colors, derive_look_colors, derive_status_colors, look_seeds, status_seeds,
};
use super::mapping::{
    BasePair, CategoryMapping, ColorTriple, CvdClash, KeptAtBase, MappedTokens, PaletteBase,
    PaletteColorReader, PaletteDial, PaletteMappingContext, PaletteTools, STATUS_TOKEN,
    StatusMapping, StopRef, StyleValue, TEXT, TEXT_PROPS, hex, is_look_token, ratio, round2,
    split_top_comma,
};
use super::types::{PaletteError, PaletteMode, TonalPaletteCandidate, TonalPaletteFamilyInput};

/// What [`map_palette`](super::mapping::map_palette) hands the editor's
/// rules. Upstream `EditorLookInput`.
pub struct EditorLookInput<'a> {
    /// The palette.
    pub candidate: &'a TonalPaletteCandidate,
    /// The color settings.
    pub dials: &'a [PaletteDial],
    /// What the mapping reads around the palette.
    pub context: &'a PaletteMappingContext<'a>,
    /// The tokens mapped so far, which the rules write into.
    pub tokens: &'a mut MappedTokens,
    /// The modes mapped (both).
    pub modes: &'a [PaletteMode],
    /// Whether a token is set exactly.
    pub set_exactly: &'a dyn Fn(&str) -> bool,
    /// The mapping's look-ups.
    pub t: &'a PaletteTools<'a>,
}

/// The editor's rules as the mapping reads them (`context.editor`).
/// Upstream `PaletteEditorRules`.
pub trait PaletteEditorRules {
    /// The colors the rules grow, written into `input.tokens`. Upstream
    /// `growLook`.
    fn grow_look(&self, input: EditorLookInput<'_>) -> Result<EditorLookResult, PaletteError>;
}

/// The editor's rules: this module, for [`PaletteMappingContext::editor`]
/// (upstream passes the `editor-look` module itself).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EditorLook;

impl PaletteEditorRules for EditorLook {
    fn grow_look(&self, input: EditorLookInput<'_>) -> Result<EditorLookResult, PaletteError> {
        grow_look(input)
    }
}

/// What the editor's rules add to a mapping. Upstream `EditorLookResult`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EditorLookResult {
    /// The category and chart colors.
    pub categories: CategoryMapping,
    /// The status roles the theme leaves unset.
    pub statuses: StatusMapping,
    /// The code colors that keep the base theme's.
    pub code: Vec<String>,
    /// Status fills someone red-weak or green-weak may not tell apart.
    pub cvd: Vec<CvdClash>,
    /// The look's colors kept at the base theme's. Upstream `keptAtBase`.
    pub kept_at_base: Vec<KeptAtBase>,
}

/// The parts whose `variant:<hue>` and `color:<hue>` rules paint a category
/// fill.
const HUE_FILL_PARTS: [&str; 4] = ["card", "clickable-card", "selectable-card", "token"];

/// The text a card tint carries, as the look gives it.
const LOOK_TEXT: [&str; 3] = [
    "--color-text-primary",
    "--color-text-secondary",
    "--color-text-accent",
];

/// The grounds the look's colors are read on, as the look gives them.
const LOOK_GROUNDS: [&str; 3] = [
    "--color-background-body",
    "--color-background-surface",
    "--color-background-card",
];

/// The text/fill pairs a base theme draws: Core's own (each category's text
/// on its fill; each status's text on its fill and on its tint) and those of
/// the base theme's rules (a coloured card's text on its fill, a part's text
/// color on its background), as `(pair, text, fill)` values a mode resolves.
/// Upstream `basePairs`.
fn base_pairs(base: Option<&PaletteBase>) -> Vec<(BasePair, String, String)> {
    let mut out = Vec::new();
    for hue in CATEGORY_HUES {
        out.push((
            BasePair::Category {
                hue: hue.to_string(),
            },
            format!("var(--color-text-{hue})"),
            format!("var(--color-background-{hue})"),
        ));
    }
    for role in ["info", "success", "warning", "error"] {
        out.push((
            BasePair::Status {
                role: role.to_string(),
                tint: false,
            },
            format!("var(--color-on-{role})"),
            format!("var(--color-{role})"),
        ));
        out.push((
            BasePair::Status {
                role: role.to_string(),
                tint: true,
            },
            format!("var(--color-on-{role}-muted)"),
            format!("var(--color-{role}-muted)"),
        ));
    }
    for (part, rules) in base
        .and_then(|b| b.components.as_deref())
        .unwrap_or_default()
    {
        let StyleValue::Block(rules) = rules else {
            continue;
        };
        for (key, raw) in rules {
            if !matches!(raw, StyleValue::Block(_)) {
                continue;
            }
            let hue = key
                .strip_prefix("variant:")
                .or_else(|| key.strip_prefix("color:"))
                .filter(|h| !h.is_empty() && h.bytes().all(|b| b.is_ascii_lowercase()));
            let fill = match raw.get("backgroundColor") {
                Some(StyleValue::Text(fill)) => Some(fill.clone()),
                Some(_) => None,
                None => hue
                    .filter(|h| {
                        HUE_FILL_PARTS.contains(&part.as_str()) && CATEGORY_HUES.contains(h)
                    })
                    .map(|h| format!("var(--color-background-{h})")),
            };
            let Some(fill) = fill else {
                continue;
            };
            for prop in TEXT_PROPS {
                if let Some(StyleValue::Text(text)) = raw.get(prop) {
                    out.push((
                        BasePair::Part {
                            part: part.clone(),
                            block: key.clone(),
                            prop: prop.to_string(),
                        },
                        text.clone(),
                        fill.clone(),
                    ));
                }
            }
        }
    }
    out
}

/// The token a value is exactly `var(--name)` of (`/^var\((--[\w-]+)\)$/`).
fn var_ref(value: &str) -> Option<&str> {
    let name = value.strip_prefix("var(")?.strip_suffix(')')?;
    is_var_name(name).then_some(name)
}

/// Whether `name` is `--` and one or more word characters or hyphens.
fn is_var_name(name: &str) -> bool {
    name.strip_prefix("--").is_some_and(|rest| {
        !rest.is_empty()
            && rest
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    })
}

/// Every `var(--name)` in a value, left to right (`/var\((--[\w-]+)\)/g`).
fn var_refs(value: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(found) = value[from..].find("var(") {
        let start = from + found + 4;
        let rest = &value[start..];
        let len = rest
            .bytes()
            .enumerate()
            .take_while(|&(i, b)| {
                if i < 2 {
                    b == b'-'
                } else {
                    b.is_ascii_alphanumeric() || b == b'_' || b == b'-'
                }
            })
            .count();
        if len > 2 && rest.as_bytes().get(len) == Some(&b')') && rest.starts_with("--") {
            out.push(&rest[..len]);
            from = start + len + 1;
        } else {
            from = from + found + 1;
        }
    }
    out
}

/// A value's color in a mode, through the tokens it reads (`now`: the
/// look's tokens first, else the base theme's alone): a `light-dark()` half,
/// a `var()` followed up to six deep, and a translucent color over the page.
/// Upstream `colorOf` (keepWhereBaseReads).
fn color_of(
    v: &str,
    mode: PaletteMode,
    now: bool,
    depth: u32,
    tokens: &MappedTokens,
    base: Option<&PaletteBase>,
    reader: &dyn PaletteColorReader,
) -> Option<ColorTriple> {
    if depth > 6 {
        return None;
    }
    let mut x = v
        .trim_matches(super::super::js::is_js_whitespace)
        .to_string();
    let lowered = x.to_ascii_lowercase();
    if lowered.starts_with("light-dark(") && x.ends_with(')') && x.len() >= "light-dark()".len() {
        let halves = split_top_comma(&x["light-dark(".len()..x.len() - 1]);
        if halves.len() == 2 {
            let i = usize::from(mode == PaletteMode::Dark);
            x = halves[i]
                .trim_matches(super::super::js::is_js_whitespace)
                .to_string();
        }
    }
    let base_value = |token: &str| base.and_then(|b| b.mode(mode).get(token).cloned());
    if let Some(name) = var_ref(&x) {
        let own = if now {
            tokens.color(name, mode).map(str::to_string)
        } else {
            None
        };
        let next = own.or_else(|| base_value(name)).unwrap_or_default();
        return color_of(&next, mode, now, depth + 1, tokens, base, reader);
    }
    let rgb = reader.rgb(&x)?;
    let page_value = (if now {
        tokens
            .color("--color-background-body", mode)
            .map(str::to_string)
    } else {
        None
    })
    .or_else(|| base_value("--color-background-body"))
    .unwrap_or_else(|| {
        if mode == PaletteMode::Light {
            "#ffffff"
        } else {
            "#000000"
        }
        .to_string()
    });
    let page = reader.rgb(&page_value);
    let a = reader.alpha(&x);
    match page {
        Some(page) if a < 1.0 => Some(composite_over(rgb, a, page)),
        _ => Some(rgb),
    }
}

/// Keeps the base theme's value, in a mode, for each look color (category
/// or status) that would leave a text/fill pair the base theme draws below
/// 4.5:1 where the base's own values read better. Pairs are checked again
/// until none changes (six rounds at most), so a fill kept at the base's
/// sends its text back too. `tokens` is written in place. Upstream
/// `keepWhereBaseReads`.
fn keep_where_base_reads(
    tokens: &mut MappedTokens,
    base: Option<&PaletteBase>,
    modes: &[PaletteMode],
    is_look: &dyn Fn(&str) -> bool,
    reader: &dyn PaletteColorReader,
) -> Vec<KeptAtBase> {
    let mut kept = Vec::new();
    let pairs = base_pairs(base);
    for _ in 0..6 {
        let mut changed = false;
        for &m in modes {
            for (pair, text, fill) in &pairs {
                let reads = |v: &str| -> Vec<String> {
                    var_refs(v)
                        .into_iter()
                        .filter(|k| is_look(k) && tokens.contains(k))
                        .map(str::to_string)
                        .collect()
                };
                let mut involved = reads(text);
                involved.extend(reads(fill));
                involved.retain(|k| {
                    tokens.get(k).and_then(|t| t.get(m).family.as_deref()) != Some("literal")
                });
                if involved.is_empty() {
                    continue;
                }
                let ft = color_of(text, m, true, 0, tokens, base, reader);
                let bt = color_of(fill, m, true, 0, tokens, base, reader);
                let fb = color_of(text, m, false, 0, tokens, base, reader);
                let bb = color_of(fill, m, false, 0, tokens, base, reader);
                let (Some(ft), Some(bt)) = (ft, bt) else {
                    continue;
                };
                let now = ratio(&ft, &bt);
                let was = match (fb, bb) {
                    (Some(fb), Some(bb)) => ratio(&fb, &bb),
                    _ => 0.0,
                };
                if now >= TEXT || now >= was - 0.01 {
                    continue;
                }
                for k in involved {
                    let Some(value) = base.and_then(|b| b.mode(m).get(&k).cloned()) else {
                        continue;
                    };
                    if let Some(token) = tokens.get(&k) {
                        let mut token = token.clone();
                        token.set(
                            m,
                            StopRef {
                                stop: 0.0,
                                color: value,
                                family: Some("literal".to_string()),
                                ..StopRef::default()
                            },
                        );
                        tokens.insert(k.clone(), token);
                    }
                    kept.push(KeptAtBase {
                        token: k,
                        mode: m,
                        pair: pair.clone(),
                        ratio: round2(now),
                    });
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }
    kept
}

/// Whether a token is one of a status role's (`--color-[on-|text-|icon-|
/// border-]<word>[-muted]`) for the word.
fn is_status_token_of(token: &str, word: &str) -> bool {
    let Some(rest) = token.strip_prefix("--color-") else {
        return false;
    };
    let prefixed = ["on-", "text-", "icon-", "border-"]
        .iter()
        .find_map(|p| rest.strip_prefix(p));
    [Some(rest), prefixed]
        .into_iter()
        .flatten()
        .any(|r| r == word || r.strip_suffix("-muted") == Some(word))
}

/// A category or status token: the colors the look derives. Upstream
/// `isLookOrStatus`.
fn is_look_or_status(token: &str) -> bool {
    is_look_token(token)
        || ["info", "success", "warning", "error"]
            .iter()
            .any(|word| is_status_token_of(token, word))
}

/// A setting id's display name: its first letter uppercased.
fn named(id: &str, seed: String) -> TonalPaletteFamilyInput {
    let mut chars = id.chars();
    let name = match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    };
    TonalPaletteFamilyInput {
        id: id.to_string(),
        name: Some(name),
        seed,
        kind: None,
        anchors: None,
    }
}

/// The families the theme editor grows beside the theme's own colors, in
/// order: the category and chart hues of the look ([`look_seeds`]), then the
/// status roles the theme leaves unset, from the base theme's light color
/// for each ([`status_seeds`]). Empty without a color setting. Upstream
/// `grownFamilies`.
pub fn grown_families(
    dials: &[PaletteDial],
    base: Option<&PaletteBase>,
    reader: &dyn PaletteColorReader,
) -> Result<Vec<TonalPaletteFamilyInput>, PaletteError> {
    if dials.is_empty() {
        return Ok(Vec::new());
    }
    let seed_of = |id: &str| dials.iter().find(|d| d.id == id).map(|d| d.seed.as_str());
    let accent = seed_of("accent");
    let mut out: Vec<TonalPaletteFamilyInput> = look_seeds(accent, seed_of("neutral"))?
        .into_iter()
        .map(|(id, seed)| named(&id, seed))
        .collect();
    let mut base_colors: Vec<(String, String)> = Vec::new();
    for (d, word) in STATUS_TOKEN {
        if dials.iter().any(|f| f.id == d) {
            continue;
        }
        let Some(v) = base.and_then(|b| b.light.get(&format!("--color-{word}"))) else {
            continue;
        };
        if v.is_empty() {
            continue;
        }
        if let Some(rgb) = reader.rgb(v)
            && reader.alpha(v) >= 1.0
        {
            base_colors.push((d.to_string(), hex(&rgb)));
        }
    }
    for (id, seed) in status_seeds(&base_colors, accent)? {
        if !out.iter().any(|f| f.id == id) {
            out.push(named(&id, seed));
        }
    }
    Ok(out)
}

/// The colors the theme editor grows beyond `themePalette()`'s mapping,
/// written into `input.tokens` as shades of the palette: the category and
/// chart colors, the status roles the theme leaves unset, the code colors,
/// then the status fills that clash for red- or green-weak vision and the
/// look colors kept at the base theme's. A token the theme sets by hand
/// keeps its value. Upstream `growLook`.
pub fn grow_look(input: EditorLookInput<'_>) -> Result<EditorLookResult, PaletteError> {
    let EditorLookInput {
        candidate,
        dials,
        context,
        tokens,
        modes,
        set_exactly,
        t,
    } = input;
    let base = context.base;
    let has = context.has;
    let stops = t.stops;
    let wanted = |k: &str| has(k) && !set_exactly(k);
    let base_value = |m: PaletteMode, k: &str| base.and_then(|b| b.mode(m).get(k).cloned());
    // The look's own colors for tokens in a mode: what the palette gives
    // them, else the base theme's. A color set exactly is not the look's.
    let look_colors = |tokens: &MappedTokens, ks: &[&str], m: PaletteMode| -> Vec<String> {
        ks.iter()
            .filter_map(|k| {
                tokens
                    .color(k, m)
                    .map(str::to_string)
                    .or_else(|| t.color_in(Some(&base_value(m, k).unwrap_or_default()), m))
            })
            .filter(|v| !v.is_empty() && t.parse_color(v).is_some() && t.alpha_of(v) >= 1.0)
            .collect()
    };
    let write = |tokens: &mut MappedTokens, derived: &MappedTokens| {
        for (k, v) in derived.iter() {
            tokens.insert(k, v.clone());
        }
    };

    // The category colors (Badge, Token and Card tints) and the chart colors.
    let categories = {
        let derived = {
            let now: &MappedTokens = tokens;
            let text = |m: PaletteMode| look_colors(now, &LOOK_TEXT, m);
            let grounds = |m: PaletteMode| look_colors(now, &LOOK_GROUNDS, m);
            derive_look_colors(&DeriveLookInput {
                palette: &candidate.palette,
                stops,
                modes,
                text: &text,
                accent: dials
                    .iter()
                    .find(|f| f.id == "accent")
                    .map(|f| f.seed.as_str()),
                grounds: &grounds,
                wanted: Some(&wanted),
                shown: context.shown,
            })?
        };
        write(tokens, &derived.tokens);
        CategoryMapping {
            badges: derived.badges,
            chart: derived.chart,
            limits: derived.limits,
            moved: derived.moved,
            alike: derived.alike,
            unread: derived.unread,
        }
    };

    // The status roles the theme leaves unset.
    let roles: Vec<String> = STATUS_TOKEN
        .iter()
        .map(|(d, _)| *d)
        .filter(|d| !dials.iter().any(|f| f.id == *d) && candidate.family(d).is_some())
        .map(str::to_string)
        .collect();
    let statuses = if roles.is_empty() {
        StatusMapping::default()
    } else {
        let derived = {
            let now: &MappedTokens = tokens;
            let grounds = |m: PaletteMode| look_colors(now, &LOOK_GROUNDS, m);
            let inks = |m: PaletteMode, id: &str| -> Vec<Stop> {
                t.on_choices(id, m)
                    .into_iter()
                    .map(|r| Stop {
                        stop: r.stop,
                        color: r.color,
                        family: Some(r.family.unwrap_or_else(|| "neutral".to_string())),
                        ..Stop::default()
                    })
                    .collect()
            };
            let accent = |m: PaletteMode| now.color("--color-accent", m).map(str::to_string);
            derive_status_colors(&DeriveStatusInput {
                palette: &candidate.palette,
                stops,
                modes,
                roles: &roles,
                grounds: &grounds,
                inks: &inks,
                accent: &accent,
                wanted: Some(&wanted),
                shown: context.shown,
            })?
        };
        write(tokens, &derived.tokens);
        StatusMapping {
            roles: derived.roles,
            why: derived.why,
            short: derived.short,
        }
    };

    // The code colors and the overlay scrim; returns the roles that keep the
    // base theme's.
    let code = {
        let derived = {
            let now: &MappedTokens = tokens;
            let grounds = |m: PaletteMode| look_colors(now, &LOOK_GROUNDS, m);
            let code_ground = |m: PaletteMode| -> Option<String> {
                let v = t.exact_of("--color-syntax-background", m)?;
                let rgb = t.parse_color(&v)?;
                (t.alpha_of(&v) >= 1.0).then(|| hex(&rgb))
            };
            derive_code_colors(&DeriveCodeInput {
                palette: &candidate.palette,
                stops,
                modes,
                grounds: &grounds,
                code_ground: Some(&code_ground),
                wanted: Some(&wanted),
            })?
        };
        write(tokens, &derived.tokens);
        derived.unreadable
    };

    // The status fills, as the theme shows them, that someone red-weak or
    // green-weak may not tell apart, in each mode, where the theme changed at
    // least one of the two (a base theme's own pair is the base's).
    let cvd = {
        let words = ["info", "success", "warning", "error"];
        let fallback = |m: PaletteMode| {
            if m == PaletteMode::Light {
                "#ffffff"
            } else {
                "#000000"
            }
        };
        let page_of = |m: PaletteMode| -> String {
            let v = tokens
                .color("--color-background-body", m)
                .map(str::to_string)
                .or_else(|| {
                    t.color_in(
                        Some(&base_value(m, "--color-background-body").unwrap_or_default()),
                        m,
                    )
                })
                .unwrap_or_else(|| fallback(m).to_string());
            t.parse_color(&v)
                .map_or_else(|| fallback(m).to_string(), |c| hex(&c))
        };
        // (Every mode's page: a status color set with no mode shows on each.)
        let pages: Vec<String> = modes.iter().map(|&m| page_of(m)).collect();
        let mut out = Vec::new();
        for &m in modes {
            let mut fills: Vec<(String, String)> = Vec::new();
            for w in words {
                let k = format!("--color-{w}");
                let v = t
                    .exact_of(&k, m)
                    .or_else(|| tokens.color(&k, m).map(str::to_string))
                    .or_else(|| t.color_in(Some(&base_value(m, &k).unwrap_or_default()), m));
                if let Some(v) = v.filter(|v| !v.is_empty())
                    && let Some(rgb) = t.parse_color(&v)
                    && t.alpha_of(&v) >= 1.0
                {
                    fills.push((w.to_string(), hex(&rgb)));
                }
            }
            let changed = |w: &str| {
                let k = format!("--color-{w}");
                tokens.contains(&k) || t.exact_of(&k, m).is_some()
            };
            let page = page_of(m);
            for c in cvd_clashes(&fills) {
                if changed(&c.a) || changed(&c.b) {
                    let color = |w: &str| {
                        fills
                            .iter()
                            .find(|(k, _)| k == w)
                            .map(|(_, v)| v.clone())
                            .unwrap_or_default()
                    };
                    out.push(CvdClash {
                        colors: [color(&c.a), color(&c.b)],
                        a: c.a,
                        b: c.b,
                        vision: c.vision,
                        mode: m,
                        page: Some(page.clone()),
                        pages: Some(pages.clone()),
                    });
                }
            }
        }
        out
    };

    // A status token of a role the theme leaves unset: the editor's to grow.
    let grown_status = |k: &str| {
        STATUS_TOKEN
            .iter()
            .any(|(d, word)| !dials.iter().any(|f| f.id == *d) && is_status_token_of(k, word))
    };
    let is_look = |k: &str| is_look_token(k) || (is_look_or_status(k) && grown_status(k));
    let kept_at_base = keep_where_base_reads(tokens, base, modes, &is_look, context.reader);
    Ok(EditorLookResult {
        categories,
        statuses,
        code,
        cvd,
        kept_at_base,
    })
}

#[cfg(test)]
mod tests {
    use super::super::generator::generate_tonal_palette;
    use super::super::generator::spec_json::spec_theme_definition;
    use super::super::mapping::test_support::*;
    use super::super::mapping::{
        PaletteConfig, PaletteFills, anchor_dark_fills, dial_family, fill_at, map_palette,
    };
    use super::super::types::ModeStrategy;
    use super::*;

    /// The theme editor's plan for its settings (its own families, then the
    /// families it grows, then the dark anchors), the palette and the mapping
    /// with these rules, rendered as the fixtures are.
    /// What a scenario's context sets exactly, `(token, mode, value)`, and
    /// the base values it replaces.
    type Settings<'s> = &'s [(&'s str, PaletteMode, &'s str)];

    fn editor_scenario(
        dials: &[(&str, &str)],
        components: Vec<(String, StyleValue)>,
        shown: Option<&[PaletteMode]>,
    ) -> String {
        editor_scenario_with(dials, components, shown, &[], &[])
    }

    fn editor_scenario_with(
        dials: &[(&str, &str)],
        components: Vec<(String, StyleValue)>,
        shown: Option<&[PaletteMode]>,
        settings: Settings<'_>,
        overrides: Settings<'_>,
    ) -> String {
        let dials: Vec<PaletteDial> = dials
            .iter()
            .map(|(id, seed)| PaletteDial {
                id: id.to_string(),
                seed: seed.to_string(),
                only: None,
            })
            .collect();
        let mut base = default_base(components);
        for (token, mode, value) in overrides {
            let values = match mode {
                PaletteMode::Light => &mut base.light,
                PaletteMode::Dark => &mut base.dark,
            };
            values.insert(token.to_string(), value.to_string());
        }
        let reader = CoreReader;
        let setting_of = |token: &str, mode: PaletteMode| {
            settings
                .iter()
                .find(|(t, m, _)| *t == token && *m == mode)
                .map(|(_, _, v)| v.to_string())
        };
        let set_exactly = |token: &str| settings.iter().any(|(t, _, _)| *t == token);
        let mut context = PaletteMappingContext::new(&has_default, &reader);
        context.base = Some(&base);
        context.shown = shown;
        if !settings.is_empty() {
            context.setting_of = Some(&setting_of);
            context.set_exactly = Some(&set_exactly);
        }
        let mut config = PaletteConfig {
            mode_strategy: Some(ModeStrategy::LightAndDark),
            ..PaletteConfig::default()
        };
        let mut fills: PaletteFills = Vec::new();
        for dial in &dials {
            let (family, stop) = dial_family(dial, &config, &PaletteMode::BOTH).unwrap();
            if dial.id != "neutral" {
                fills.push((dial.id.clone(), fill_at(stop)));
            }
            config.families.push(family);
        }
        for family in grown_families(&dials, Some(&base), &reader).unwrap() {
            if !config.families.iter().any(|f| f.id == family.id) {
                config.families.push(family);
            }
        }
        config.families = anchor_dark_fills(&config, &dials, &mut fills, &context, None).unwrap();
        let candidate = generate_tonal_palette(&config).unwrap();
        let editor = EditorLook;
        context.editor = Some(&editor);
        let mapping = map_palette(&candidate, &dials, &fills, &context).unwrap();
        format!(
            "{}\n----\n{}",
            render_plan(&config.families, &fills),
            render(&mapping)
        )
    }

    fn neutral_components() -> Vec<(String, StyleValue)> {
        styles_from_json(spec_theme_definition("neutral").get("components").unwrap())
    }

    /// Upstream's plan and mapping for this scenario (the theme editor's plan,
    /// grownFamilies and growLook, run under Node 24 against Core's resolved
    /// defaults).
    const EDITOR_NEUTRAL: &str = r##"accent - #5b3cc4 light:40:#5b3cc4
neutral neutral #6b6b70 light:45:#6b6b70,dark:45:#6b6b70
red - #c14759 
orange - #c97751 
yellow - #f8c468 
green - #3a8758 
teal - #2f6f6b 
cyan - #376b7f 
blue - #4275c9 
purple - #873aa5 
pink - #9b3877 
brown - #915f36 
indigo - #7c8de5 
success - #39813e 
warning - #ddb357 
danger - #cd474e 
info - #2f6ac4 
fills accent:40:false:2.1:your exact color stands out from the dark page at only 2.1:1, and a fill needs 3:1
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
token --color-background-red red 85 #ffc2c6 - - 0 | 20 #5e051c - - 0
token --color-text-red red 25 #740022 - - 0 | 80 #ffadb2 - - 0
token --color-icon-red red 25 #740022 - - 0 | 75 #ff969f - - 0
token --color-border-red red 80 #ffadb2 - - 0 | 65 #e77682 - - 0
token --color-background-orange orange 85 #ffc6ac - - 0 | 20 #5a1400 - - 0
token --color-text-orange orange 25 #6b1c00 - - 0 | 80 #ffb18e - - 0
token --color-icon-orange orange 25 #6b1c00 - - 0 | 75 #f7a079 - - 0
token --color-border-orange orange 80 #ffb18e - - 0 | 65 #de8257 - - 0
token --color-background-yellow yellow 85 #fbcd7d - - 0 | 20 #422c00 - - 0
token --color-text-yellow yellow 25 #503600 - - 0 | 80 #eabf76 - - 0
token --color-icon-yellow yellow 25 #503600 - - 0 | 75 #deb161 - - 0
token --color-border-yellow yellow 80 #f0bd64 - - 0 | 65 #c59437 - - 0
token --color-background-green green 85 #b2e2c0 - - 0 | 20 #003c1c - - 0
token --color-text-green green 25 #004924 - - 0 | 80 #a4d4b3 - - 0
token --color-icon-green green 25 #004924 - - 0 | 75 #91c8a2 - - 0
token --color-border-green green 80 #9dd7af - - 0 | 65 #6cb083 - - 0
token --color-background-teal teal 85 #aae1dc - - 0 | 20 #003936 - - 0
token --color-text-teal teal 25 #004642 - - 0 | 80 #9ad4cf - - 0
token --color-icon-teal teal 25 #004642 - - 0 | 75 #81c8c3 - - 0
token --color-border-teal teal 80 #90d6d0 - - 0 | 65 #50b0aa - - 0
token --color-background-cyan cyan 85 #97e1fe - - 0 | 20 #003648 - - 0
token --color-text-cyan cyan 25 #004256 - - 0 | 80 #8ed2ed - - 0
token --color-icon-cyan cyan 25 #004256 - - 0 | 75 #7bc5e2 - - 0
token --color-border-cyan cyan 80 #81d4f4 - - 0 | 65 #57aaca - - 0
token --color-background-blue blue 85 #bcd6ff - - 0 | 20 #062c69 - - 0
token --color-text-blue blue 25 #003485 - - 0 | 80 #a6c8ff - - 0
token --color-icon-blue blue 25 #003485 - - 0 | 75 #8fbaff - - 0
token --color-border-blue blue 80 #a6c8ff - - 0 | 65 #6b9ef1 - - 0
token --color-background-purple purple 85 #ecc2ff - - 0 | 20 #451955 - - 0
token --color-text-purple purple 25 #561a6c - - 0 | 80 #e2b1f8 - - 0
token --color-icon-purple purple 25 #561a6c - - 0 | 75 #d6a1ed - - 0
token --color-border-purple purple 80 #e6adff - - 0 | 65 #bc84d5 - - 0
token --color-background-pink pink 85 #ffbee2 - - 0 | 20 #550c3e - - 0
token --color-text-pink pink 25 #6b034d - - 0 | 80 #fda8d8 - - 0
token --color-icon-pink pink 25 #6b034d - - 0 | 75 #f297cb - - 0
token --color-border-pink pink 80 #ffa6d9 - - 0 | 65 #d979b2 - - 0
token --color-background-gray neutral 90 #e2e2e5 - - 0 | 20 #303033 - - 0
token --color-text-gray neutral 10 #1b1b1e - - 0 | 85 #d4d4d7 - - 0
token --color-icon-gray neutral 30 #46464b - - 0 | 65 #9d9da1 - - 0
token --color-border-gray neutral 85 #d4d4d8 - - 0 | 15 #252528 - - 0
token --color-data-categorical-blue blue 50 #3673d7 - - 0 | 60 #5c90e5 - - 0
token --color-data-categorical-orange orange 50 #bf510d - - 0 | 60 #d17448 - - 0
token --color-data-categorical-purple purple 50 #9a54b6 - - 0 | 60 #af76c8 - - 0
token --color-data-categorical-green green 50 #2e8b56 - - 0 | 60 #5ca375 - - 0
token --color-data-categorical-pink pink 50 #b8448e - - 0 | 60 #cc6aa5 - - 0
token --color-data-categorical-cyan cyan 50 #0083a7 - - 0 | 60 #469dbd - - 0
token --color-data-categorical-red red 50 #c73f56 - - 0 | 60 #d96875 - - 0
token --color-data-categorical-teal teal 50 #008984 - - 0 | 60 #3aa39d - - 0
token --color-data-categorical-brown brown 50 #b05f00 - - 0 | 60 #c77c3c - - 0
token --color-data-categorical-indigo indigo 50 #5b6bd5 - - 0 | 60 #7889e3 - - 0
token --color-data-neutral neutral 55 #848489 - - 0 | 55 #848488 - - 0
token --color-data-blue-1 blue 90 #d3e4ff - - 0 | 90 #d3e4ff - - 0
token --color-data-blue-2 blue 75 #8fbaff - - 0 | 75 #8fbaff - - 0
token --color-data-blue-3 blue 55 #4381e5 - - 0 | 55 #4f82d7 - - 0
token --color-data-blue-4 blue 35 #124ca8 - - 0 | 35 #204f9b - - 0
token --color-data-blue-5 blue 15 #001f5e - - 0 | 15 #022257 - - 0
token --color-data-shamrock-1 green 90 #c6eed2 - - 0 | 90 #cbecd4 - - 0
token --color-data-shamrock-2 green 75 #88cb9e - - 0 | 75 #91c8a2 - - 0
token --color-data-shamrock-3 green 55 #3e9862 - - 0 | 55 #4d9668 - - 0
token --color-data-shamrock-4 green 35 #006434 - - 0 | 35 #196239 - - 0
token --color-data-shamrock-5 green 15 #002f15 - - 0 | 15 #002f15 - - 0
token --color-data-orange-1 orange 90 #ffd9c9 - - 0 | 90 #ffd9c9 - - 0
token --color-data-orange-2 orange 75 #ff9c6e - - 0 | 75 #f7a079 - - 0
token --color-data-orange-3 orange 55 #cd5f23 - - 0 | 55 #c46639 - - 0
token --color-data-orange-4 orange 35 #8e2d00 - - 0 | 35 #8a330e - - 0
token --color-data-orange-5 orange 15 #490c00 - - 0 | 15 #490c00 - - 0
token --color-data-pink-1 pink 90 #ffd4eb - - 0 | 90 #ffd4eb - - 0
token --color-data-pink-2 pink 75 #fa90cf - - 0 | 75 #f297cb - - 0
token --color-data-pink-3 pink 55 #c6529b - - 0 | 55 #be5c98 - - 0
token --color-data-pink-4 pink 35 #8b1d67 - - 0 | 35 #832b64 - - 0
token --color-data-pink-5 pink 15 #490033 - - 0 | 15 #460531 - - 0
token --color-data-purple-1 purple 90 #f2d7ff - - 0 | 90 #f2d7ff - - 0
token --color-data-purple-2 purple 75 #da9cf6 - - 0 | 75 #d6a1ed - - 0
token --color-data-purple-3 purple 55 #a762c4 - - 0 | 55 #a268bb - - 0
token --color-data-purple-4 purple 35 #71308b - - 0 | 35 #6d3782 - - 0
token --color-data-purple-5 purple 15 #3a084b - - 0 | 15 #371146 - - 0
token --color-data-red-1 red 90 #ffd7d9 - - 0 | 90 #ffd7d9 - - 0
token --color-data-red-2 red 75 #ff969f - - 0 | 75 #ff969f - - 0
token --color-data-red-3 red 55 #d64e62 - - 0 | 55 #cb5968 - - 0
token --color-data-red-4 red 35 #971334 - - 0 | 35 #8f2639 - - 0
token --color-data-red-5 red 15 #4e0013 - - 0 | 15 #4d0014 - - 0
token --color-data-teal-1 teal 90 #c1ede9 - - 0 | 90 #c6ebe8 - - 0
token --color-data-teal-2 teal 75 #75cac4 - - 0 | 75 #81c8c3 - - 0
token --color-data-teal-3 teal 55 #009892 - - 0 | 55 #249690 - - 0
token --color-data-teal-4 teal 35 #00605c - - 0 | 35 #00605c - - 0
token --color-data-teal-5 teal 15 #002d2b - - 0 | 15 #002d2b - - 0
token --color-data-yellow-1 yellow 90 #ffdda5 - - 0 | 90 #ffdda5 - - 0
token --color-data-yellow-2 yellow 75 #e5af4b - - 0 | 75 #deb161 - - 0
token --color-data-yellow-3 yellow 55 #ab7900 - - 0 | 55 #ab7900 - - 0
token --color-data-yellow-4 yellow 35 #6d4b00 - - 0 | 35 #6d4b00 - - 0
token --color-data-yellow-5 yellow 15 #352200 - - 0 | 15 #352200 - - 0
token --color-data-gray-1 neutral 90 #e2e2e5 - - 0 | 90 #e2e2e5 - - 0
token --color-data-gray-2 neutral 75 #b8b8bd - - 0 | 75 #b8b8bc - - 0
token --color-data-gray-3 neutral 55 #848489 - - 0 | 55 #848488 - - 0
token --color-data-gray-4 neutral 35 #535357 - - 0 | 35 #535356 - - 0
token --color-data-gray-5 neutral 15 #252528 - - 0 | 15 #252528 - - 0
token --color-success success 45 #347d39 - - 0 | 70 #86bb87 - - 0
token --color-success-muted success 85 #b8e1b8 - - 0 | 75 #98c7983d - 3d 0
token --color-text-success success 35 #1b6323 - - 0 | 80 #aad3aa - - 0
token --color-on-success-muted success 25 #034910 - - 0 | 80 #aad3aa - - 0
token --color-icon-success success 45 #347d39 - - 0 | 70 #86bb87 - - 0
token --color-border-success success 45 #347d39 - - 0 | 70 #86bb87 - - 0
token --color-on-success neutral 100 #ffffff neutral - 0 | 5 #111112 neutral - 0
token --color-warning warning 85 #f6cf7d - - 0 | 85 #f1d08d - - 0
token --color-warning-muted warning 90 #ffde9b - - 0 | 75 #dab3613d - 3d 0
token --color-text-warning warning 40 #785900 - - 0 | 80 #e5c276 - - 0
token --color-on-warning-muted warning 25 #4d3700 - - 0 | 80 #e5c276 - - 0
token --color-icon-warning warning 55 #a77c00 - - 0 | 85 #f1d08d - - 0
token --color-border-warning warning 55 #a77c00 - - 0 | 85 #f1d08d - - 0
token --color-on-warning neutral 5 #111112 neutral - 0 | 5 #111112 neutral - 0
token --color-error danger 50 #ca3e47 - - 0 | 70 #f68585 - - 0
token --color-error-muted danger 85 #ffc3c1 - - 0 | 75 #ff97963d - 3d 0
token --color-text-error danger 40 #aa2030 - - 0 | 80 #ffaeab - - 0
token --color-on-error-muted danger 25 #750016 - - 0 | 80 #ffaeab - - 0
token --color-icon-error danger 50 #ca3e47 - - 0 | 70 #f68585 - - 0
token --color-border-error danger 50 #ca3e47 - - 0 | 70 #f68585 - - 0
token --color-on-error neutral 100 #ffffff neutral - 0 | 5 #111112 neutral - 0
token --color-info info 50 #3073d9 - - 0 | 80 #a5c8ff - - 0
token --color-info-muted info 85 #bcd6ff - - 0 | 75 #8ebaff3d - 3d 0
token --color-text-info info 35 #074caa - - 0 | 80 #a5c8ff - - 0
token --color-on-info-muted info 25 #003583 - - 0 | 80 #a5c8ff - - 0
token --color-icon-info info 50 #3073d9 - - 0 | 80 #a5c8ff - - 0
token --color-border-info info 50 #3073d9 - - 0 | 80 #a5c8ff - - 0
token --color-on-info neutral 100 #ffffff neutral - 0 | 5 #111112 neutral - 0
token --color-syntax-background neutral 100 #ffffff - - 0 | 5 #111112 - - 0
token --color-overlay neutral 10 #1b1b1e80 - 80 0 | 5 #111112cc - cc 0
token --color-syntax-keyword purple 30 #63257b - - 0 | 80 #e2b1f8 - - 0
token --color-syntax-string green 30 #00562c - - 0 | 80 #a4d4b3 - - 0
token --color-syntax-comment neutral 35 #535357 - - 0 | 75 #b8b8bc - - 0
token --color-syntax-number orange 30 #7d2400 - - 0 | 80 #ffb18e - - 0
token --color-syntax-function blue 30 #064097 - - 0 | 80 #a6c8ff - - 0
token --color-syntax-type purple 30 #63257b - - 0 | 80 #e2b1f8 - - 0
token --color-syntax-variable neutral 5 #111112 - - 0 | 90 #e2e2e5 - - 0
token --color-syntax-operator neutral 35 #535357 - - 0 | 75 #b8b8bc - - 0
token --color-syntax-constant orange 30 #7d2400 - - 0 | 80 #ffb18e - - 0
token --color-syntax-tag red 30 #87012a - - 0 | 80 #ffadb2 - - 0
token --color-syntax-attribute yellow 30 #5e4100 - - 0 | 80 #eabf76 - - 0
token --color-syntax-property teal 30 #00524f - - 0 | 80 #9ad4cf - - 0
token --color-syntax-punctuation neutral 35 #535357 - - 0 | 75 #b8b8bc - - 0
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
check --color-text-secondary light 30 30 5.6 4.5 true [the accent tint over the surfaces] note=--color-text-secondary on selected items and rows, a dropzone, numbered citations uses #46464b in light, not #474747, so it stays readable (5.6:1) on the accent tint and under a hover dials=accent,neutral fix=-
check --color-text-secondary dark 80 80 5.03 4.5 true [the accent tint over the surfaces] note=--color-text-secondary on selected items and rows, a dropzone, numbered citations uses #c6c6c9 in dark, not #c6c6c6, so it stays readable (5.03:1) on the accent tint and under a hover dials=accent,neutral fix=-
check --color-text-secondary light 30 30 6.18 4.5 true [the neutral track over the surfaces] note=--color-text-secondary on segmented-control labels uses #46464b in light, not #474747, so it stays readable (6.18:1) on the neutral track and under a hover dials=accent,neutral fix=-
check --color-text-secondary dark 85 80 5.11 4.5 true [the neutral track over the surfaces] note=--color-text-secondary on segmented-control labels uses #d4d4d7 in dark, not #c6c6c6, so it stays readable (5.11:1) on the neutral track and under a hover dials=accent,neutral fix=-
check --color-text-secondary light 30 30 5.69 4.5 true [the blue card fill over the surfaces] note=--color-text-secondary on blue cards uses #46464b in light, not #474747, so it stays readable (5.69:1) on the blue card fill and under a hover dials=accent,neutral fix=-
check --color-text-secondary dark 75 75 5.53 4.5 true [the blue card fill over the surfaces] note=--color-text-secondary on blue cards uses #b8b8bc in dark, not #b9b9b9, so it stays readable (5.53:1) on the blue card fill and under a hover dials=accent,neutral fix=-
check --color-text-secondary light 30 30 6.12 4.5 true [the cyan card fill over the surfaces] note=--color-text-secondary on cyan cards uses #46464b in light, not #474747, so it stays readable (6.12:1) on the cyan card fill and under a hover dials=accent,neutral fix=-
check --color-text-secondary dark 75 75 4.88 4.5 true [the cyan card fill over the surfaces] note=--color-text-secondary on cyan cards uses #b8b8bc in dark, not #b9b9b9, so it stays readable (4.88:1) on the cyan card fill and under a hover dials=accent,neutral fix=-
check --color-text-secondary light 30 30 6.22 4.5 true [the green card fill over the surfaces] note=--color-text-secondary on green cards uses #46464b in light, not #474747, so it stays readable (6.22:1) on the green card fill and under a hover dials=accent,neutral fix=-
check --color-text-secondary dark 75 75 4.71 4.5 true [the green card fill over the surfaces] note=--color-text-secondary on green cards uses #b8b8bc in dark, not #b9b9b9, so it stays readable (4.71:1) on the green card fill and under a hover dials=accent,neutral fix=-
check --color-text-secondary light 30 30 6.13 4.5 true [the orange card fill over the surfaces] note=--color-text-secondary on orange cards uses #46464b in light, not #474747, so it stays readable (6.13:1) on the orange card fill and under a hover dials=accent,neutral fix=-
check --color-text-secondary dark 75 75 4.88 4.5 true [the orange card fill over the surfaces] note=--color-text-secondary on orange cards uses #b8b8bc in dark, not #b9b9b9, so it stays readable (4.88:1) on the orange card fill and under a hover dials=accent,neutral fix=-
check --color-text-secondary light 30 30 5.75 4.5 true [the pink card fill over the surfaces] note=--color-text-secondary on pink cards uses #46464b in light, not #474747, so it stays readable (5.75:1) on the pink card fill and under a hover dials=accent,neutral fix=-
check --color-text-secondary dark 75 75 5.34 4.5 true [the pink card fill over the surfaces] note=--color-text-secondary on pink cards uses #b8b8bc in dark, not #b9b9b9, so it stays readable (5.34:1) on the pink card fill and under a hover dials=accent,neutral fix=-
check --color-text-secondary light 30 30 5.7 4.5 true [the purple card fill over the surfaces] note=--color-text-secondary on purple cards uses #46464b in light, not #474747, so it stays readable (5.7:1) on the purple card fill and under a hover dials=accent,neutral fix=-
check --color-text-secondary dark 75 75 5.42 4.5 true [the purple card fill over the surfaces] note=--color-text-secondary on purple cards uses #b8b8bc in dark, not #b9b9b9, so it stays readable (5.42:1) on the purple card fill and under a hover dials=accent,neutral fix=-
check --color-text-secondary light 30 30 5.4 4.5 true [the red card fill over the surfaces] note=--color-text-secondary on red cards uses #46464b in light, not #474747, so it stays readable (5.4:1) on the red card fill and under a hover dials=accent,neutral fix=-
check --color-text-secondary dark 75 75 5.82 4.5 true [the red card fill over the surfaces] note=--color-text-secondary on red cards uses #b8b8bc in dark, not #b9b9b9, so it stays readable (5.82:1) on the red card fill and under a hover dials=accent,neutral fix=-
check --color-text-secondary light 30 30 6.25 4.5 true [the teal card fill over the surfaces] note=--color-text-secondary on teal cards uses #46464b in light, not #474747, so it stays readable (6.25:1) on the teal card fill and under a hover dials=accent,neutral fix=-
check --color-text-secondary dark 75 75 4.73 4.5 true [the teal card fill over the surfaces] note=--color-text-secondary on teal cards uses #b8b8bc in dark, not #b9b9b9, so it stays readable (4.73:1) on the teal card fill and under a hover dials=accent,neutral fix=-
check --color-text-secondary light 30 30 6.48 4.5 true [the yellow card fill over the surfaces] note=--color-text-secondary on yellow cards uses #46464b in light, not #474747, so it stays readable (6.48:1) on the yellow card fill and under a hover dials=accent,neutral fix=-
check --color-text-secondary dark 75 75 4.54 4.5 true [the yellow card fill over the surfaces] note=--color-text-secondary on yellow cards uses #b8b8bc in dark, not #b9b9b9, so it stays readable (4.54:1) on the yellow card fill and under a hover dials=accent,neutral fix=-
lift --color-accent-muted 30 #46464b - - 0 | 80 #c6c6c9 - - 0
lift --color-neutral 30 #46464b - - 0 | 85 #d4d4d7 - - 0
lift --color-background-blue 30 #46464b - - 0 | 75 #b8b8bc - - 0
lift --color-background-cyan 30 #46464b - - 0 | 75 #b8b8bc - - 0
lift --color-background-green 30 #46464b - - 0 | 75 #b8b8bc - - 0
lift --color-background-orange 30 #46464b - - 0 | 75 #b8b8bc - - 0
lift --color-background-pink 30 #46464b - - 0 | 75 #b8b8bc - - 0
lift --color-background-purple 30 #46464b - - 0 | 75 #b8b8bc - - 0
lift --color-background-red 30 #46464b - - 0 | 75 #b8b8bc - - 0
lift --color-background-teal 30 #46464b - - 0 | 75 #b8b8bc - - 0
lift --color-background-yellow 30 #46464b - - 0 | 75 #b8b8bc - - 0
statuses success,warning,danger,info |  | 
categories derived derived false false"##;
    /// The FNV-1a digest and line count of upstream's plan and mapping for
    /// this scenario, run under Node 24 and rendered as [`render`] renders
    /// it (a mismatch prints the rendering to diff against upstream's).
    const EDITOR_AMBER: (u64, usize) = (0xcd9d43bfde8a7526, 189);
    /// The FNV-1a digest and line count of upstream's plan and mapping for
    /// this scenario, run under Node 24 and rendered as [`render`] renders
    /// it (a mismatch prints the rendering to diff against upstream's).
    const EDITOR_STATUS: (u64, usize) = (0x54d630e4da344361, 236);

    #[test]
    fn grows_the_look_on_the_neutral_themes_part_styles_as_upstream() {
        assert_rendered(
            &editor_scenario(
                &[("accent", "#5b3cc4"), ("neutral", "#6b6b70")],
                neutral_components(),
                Some(&PaletteMode::BOTH),
            ),
            EDITOR_NEUTRAL,
        );
    }

    #[test]
    fn grows_the_look_of_an_accent_alone_for_a_light_theme_as_upstream() {
        assert_digest(
            &editor_scenario(
                &[("accent", "#e0a100")],
                Vec::new(),
                Some(&[PaletteMode::Light]),
            ),
            EDITOR_AMBER,
        );
    }

    #[test]
    fn grows_the_status_roles_a_theme_leaves_unset_as_upstream() {
        assert_digest(
            &editor_scenario(
                &[
                    ("accent", "#0e7490"),
                    ("neutral", "#475569"),
                    ("danger", "#be123c"),
                ],
                neutral_components(),
                None,
            ),
            EDITOR_STATUS,
        );
    }

    /// The FNV-1a digest and line count of upstream's plan and mapping for
    /// a weak accent with status fills red- or green-weak vision confuses,
    /// and a code background no syntax color reads on (run under Node 24 and rendered as [`render`] renders it; a
    /// mismatch prints the rendering to diff against upstream's).
    const EDITOR_CVD: (u64, usize) = (0xaa197355b0b64a2e, 202);
    /// The FNV-1a digest and line count of upstream's plan and mapping for
    /// a base theme whose page is the red tint and whose secondary text is
    /// faint, with a part drawing syntax text on its own dark fill (run under Node 24 and rendered as [`render`] renders it; a
    /// mismatch prints the rendering to diff against upstream's).
    const EDITOR_KEPT: (u64, usize) = (0xec6de361a3be77cc, 191);
    /// The FNV-1a digest and line count of upstream's plan and mapping for
    /// a base theme whose page is the red tint (run under Node 24 and rendered as [`render`] renders it; a
    /// mismatch prints the rendering to diff against upstream's).
    const EDITOR_MOVED: (u64, usize) = (0x2bb2e40ae92ae45f, 190);

    fn code_block_components() -> Vec<(String, StyleValue)> {
        let mut components = neutral_components();
        components.push((
            "code-block".to_string(),
            StyleValue::Block(vec![(
                "base".to_string(),
                StyleValue::Block(vec![
                    (
                        "backgroundColor".to_string(),
                        StyleValue::Text("#1e1e1e".into()),
                    ),
                    (
                        "color".to_string(),
                        StyleValue::Text("var(--color-syntax-keyword)".into()),
                    ),
                ]),
            )]),
        ));
        components
    }

    #[test]
    fn reports_status_fills_weak_vision_confuses_and_code_that_cannot_read_as_upstream() {
        assert_digest(
            &editor_scenario_with(
                &[
                    ("accent", "#7d7a8c"),
                    ("success", "#4a7a00"),
                    ("danger", "#8a5a00"),
                ],
                code_block_components(),
                Some(&PaletteMode::BOTH),
                &[("--color-syntax-background", PaletteMode::Light, "#000000")],
                &[],
            ),
            EDITOR_CVD,
        );
    }

    #[test]
    fn keeps_base_colors_a_pair_needs_and_notes_tints_that_cannot_carry_text_as_upstream() {
        assert_digest(
            &editor_scenario_with(
                &[("accent", "#5b3cc4")],
                code_block_components(),
                Some(&PaletteMode::BOTH),
                &[],
                &[
                    ("--color-background-body", PaletteMode::Light, "#ffc2c6"),
                    ("--color-text-secondary", PaletteMode::Light, "#a0a0a0"),
                ],
            ),
            EDITOR_KEPT,
        );
    }

    #[test]
    fn moves_a_tint_drawn_like_the_page_as_upstream() {
        assert_digest(
            &editor_scenario_with(
                &[("accent", "#5b3cc4")],
                Vec::new(),
                Some(&PaletteMode::BOTH),
                &[],
                &[("--color-background-body", PaletteMode::Light, "#ffc2c6")],
            ),
            EDITOR_MOVED,
        );
    }

    #[test]
    fn grown_families_are_empty_without_a_setting() {
        let reader = CoreReader;
        assert_eq!(grown_families(&[], None, &reader), Ok(Vec::new()));
    }

    #[test]
    fn base_pairs_read_cores_pairs_and_the_parts_text_on_its_fill() {
        let components = vec![
            (
                "card".to_string(),
                StyleValue::Block(vec![
                    (
                        "variant:red".to_string(),
                        StyleValue::Block(vec![(
                            "color".to_string(),
                            StyleValue::Text("var(--color-text-red)".into()),
                        )]),
                    ),
                    (
                        "variant:brand".to_string(),
                        StyleValue::Block(vec![(
                            "color".to_string(),
                            StyleValue::Text("#000".into()),
                        )]),
                    ),
                ]),
            ),
            (
                "badge".to_string(),
                StyleValue::Block(vec![(
                    "variant:blue".to_string(),
                    StyleValue::Block(vec![
                        (
                            "backgroundColor".to_string(),
                            StyleValue::Text("var(--color-background-blue)".into()),
                        ),
                        (
                            "--color-text-primary".to_string(),
                            StyleValue::Text("#111".into()),
                        ),
                        (
                            "--color-text-secondary".to_string(),
                            StyleValue::Number(1.0),
                        ),
                    ]),
                )]),
            ),
        ];
        let base = PaletteBase {
            components: Some(components),
            ..PaletteBase::default()
        };
        let pairs = base_pairs(Some(&base));
        // Ten categories and eight status pairs, then a card's red text on its
        // red fill (a card colour paints its category fill; "brand" is not
        // one) and the badge's own background.
        assert_eq!(pairs.len(), 20);
        assert_eq!(
            pairs[18],
            (
                BasePair::Part {
                    part: "card".into(),
                    block: "variant:red".into(),
                    prop: "color".into()
                },
                "var(--color-text-red)".to_string(),
                "var(--color-background-red)".to_string()
            )
        );
        assert_eq!(pairs[19].1, "#111");
        assert_eq!(pairs[19].2, "var(--color-background-blue)");
    }

    #[test]
    fn var_references_are_read_as_upstreams_patterns_read_them() {
        assert_eq!(var_ref("var(--color-accent)"), Some("--color-accent"));
        assert_eq!(var_ref("var(--)"), None);
        assert_eq!(var_ref("var(--a, red)"), None);
        assert_eq!(
            var_refs("color-mix(var(--a) 20%, var(--b-2)) var(-- x) var(--c"),
            ["--a", "--b-2"]
        );
    }

    #[test]
    fn status_tokens_are_matched_by_their_word() {
        for token in [
            "--color-error",
            "--color-on-error",
            "--color-error-muted",
            "--color-on-error-muted",
            "--color-text-error",
            "--color-icon-error",
            "--color-border-error",
        ] {
            assert!(is_status_token_of(token, "error"), "{token}");
            assert!(is_look_or_status(token), "{token}");
        }
        assert!(!is_status_token_of("--color-text-error-muted-x", "error"));
        assert!(!is_status_token_of("--color-background-error", "error"));
        assert!(is_look_or_status("--color-background-red"));
    }
}
