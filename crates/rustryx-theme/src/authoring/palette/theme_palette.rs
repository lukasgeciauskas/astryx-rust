//! A theme's colors from a few seeds: upstream `palette/themePalette.ts`.
//!
//! [`theme_palette`] takes a [`ThemePaletteInput`] (an accent, a neutral and the
//! status colors, a vibrancy and a recipe) and plans the generator request the
//! theme editor would write for those settings ([`plan_palette_request`]),
//! generates the tonal palette with the pinned recipe
//! ([`generate_tonal_palette`]) and maps its stops to the color tokens
//! against Core's defaults ([`map_palette`]). The result is a
//! [`ThemePalette`]: each mapped token's `[light, dark]` pair, and the part
//! styles that lift secondary text on the accent tint, the neutral track and
//! the coloured cards. `defineTheme({palette})` reads it; the theme's own
//! tokens and part styles still win, one by one.
//!
//! Upstream also refuses an input that is not an object, has a key it does
//! not read, or names a recipe it does not know. A [`ThemePaletteInput`] can
//! do none of those; a reader of a JSON input checks its keys with
//! [`check_theme_palette_keys`] and reads its recipe with
//! [`theme_palette_recipe`], which refuse with upstream's messages.

use std::collections::HashSet;

use super::super::TOKEN_DEFAULTS;
use super::super::color::{ColorTriple, parse_color};
use super::super::js::{js_number_to_string, js_trim};
use super::super::token_values::{ResolvedThemeMode, resolve_default_tokens};
use super::generator::{PALETTE_RECIPE, PALETTE_RECIPES, generate_tonal_palette};
use super::mapping::{
    LIFT_PARTS, PALETTE_DIALS, PaletteBase, PaletteColorReader, PaletteDial, PaletteMappingContext,
    PalettePlanOptions, hex, map_palette, plan_palette_request,
};
use super::types::{PaletteError, PaletteLift, PaletteRecipe, ThemePalette, ThemePaletteInput};

/// The keys a palette input reads, in upstream's order: the six color
/// settings ([`PALETTE_DIALS`]), then `vibrancy` and `recipe`. Upstream
/// `INPUT_KEYS`, for a reader of a JSON input.
pub const THEME_PALETTE_INPUT_KEYS: [&str; 8] = [
    "accent", "neutral", "success", "warning", "danger", "info", "vibrancy", "recipe",
];

/// The `kind` of every [`ThemePalette`]: `theme-palette`, which
/// `defineTheme` checks to know the palette was made by [`theme_palette`].
pub const THEME_PALETTE_KIND: &str = "theme-palette";

/// Core's color reader: hex, `rgb()` and `rgba()`, `black`, `white` and
/// `transparent`, read with [`parse_color`]. A value it cannot read (a
/// `var()`, an `oklch()`) has no RGB and an opacity of 1; `transparent`, in
/// any case and with spaces around it, has an opacity of 0. Upstream
/// `coreReader`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CoreReader;

impl PaletteColorReader for CoreReader {
    fn rgb(&self, value: &str) -> Option<ColorTriple> {
        parse_color(value).map(|read| [read.r, read.g, read.b])
    }

    fn alpha(&self, value: &str) -> f64 {
        // Upstream's /^transparent$/i, which folds ASCII letters only.
        if js_trim(value).eq_ignore_ascii_case("transparent") {
            return 0.0;
        }
        parse_color(value).map_or(1.0, |read| read.a)
    }
}

/// Core's default colors in each mode, what a palette is measured against:
/// every token's default resolved by [`resolve_default_tokens`] in light and
/// in dark, and no part styles. Read per call, from the defaults alone, so
/// nothing is kept between calls. Upstream `defaultBase`.
pub fn default_base() -> PaletteBase {
    PaletteBase {
        light: resolve_default_tokens(ResolvedThemeMode::Light)
            .into_iter()
            .collect(),
        dark: resolve_default_tokens(ResolvedThemeMode::Dark)
            .into_iter()
            .collect(),
        components: Some(Vec::new()),
    }
}

/// Makes a theme's colors from a few seeds. The palette is generated with
/// the recipe the input names (`astryx-oklch-v1` when it names none) and
/// mapped to the color tokens as the theme editor maps it, against Core's
/// defaults ([`default_base`], read with [`CoreReader`]). Pass the result to
/// `defineTheme` as its palette; the theme's own tokens still win, token by
/// token. Upstream `themePalette`.
///
/// Each seed is read as Core reads a color ([`parse_color`]) and becomes the
/// six-digit hex of its channels, so `rgb(91, 60, 196)` and `#5B3CC4` make
/// the same palette. The tokens are in the mapping's order; the part styles
/// are in [`LIFT_PARTS`] order, each part once with its blocks in order, and
/// only for a background the mapping lifts text on in both modes.
///
/// Refuses, with upstream's message:
/// - a seed that is not one opaque color (`themePalette(): accent is
///   "var(--brand)", and a seed is one opaque color: …`);
/// - a vibrancy that is not a whole number from 0 to 100
///   (`themePalette(): vibrancy is a whole number from 0 to 100, got 101.`);
/// - an input with no seed (`themePalette() needs at least one seed color:
///   accent, neutral, success, warning, danger, info.`);
/// - a palette the generator or the mapping cannot make, with their message.
pub fn theme_palette(input: &ThemePaletteInput) -> Result<ThemePalette, PaletteError> {
    // A typed recipe is always one of PALETTE_RECIPES; a recipe read as text
    // is checked by theme_palette_recipe.
    let recipe = input.recipe.unwrap_or(PALETTE_RECIPE);
    if let Some(vibrancy) = input.vibrancy
        && !((0.0..=100.0).contains(&vibrancy) && vibrancy.fract() == 0.0)
    {
        return Err(PaletteError(format!(
            "themePalette(): vibrancy is a whole number from 0 to 100, got {}.",
            json_number(vibrancy)
        )));
    }
    let mut dials: Vec<PaletteDial> = Vec::new();
    for id in PALETTE_DIALS {
        let Some(value) = seed_of(input, id) else {
            continue;
        };
        let read = parse_color(value).filter(|read| read.a >= 1.0);
        let Some(read) = read else {
            return Err(PaletteError(format!(
                "themePalette(): {id} is {}, and a seed is one opaque color: a hex such as \"#5b3cc4\", rgb(), black or white.",
                json_string(value)
            )));
        };
        dials.push(PaletteDial {
            id: id.to_string(),
            seed: hex(&[read.r, read.g, read.b]),
            only: None,
        });
    }
    if dials.is_empty() {
        return Err(PaletteError(format!(
            "themePalette() needs at least one seed color: {}.",
            PALETTE_DIALS.join(", ")
        )));
    }
    let base = default_base();
    let core_tokens: HashSet<&str> = TOKEN_DEFAULTS.iter().map(|&(name, _)| name).collect();
    let has = |token: &str| core_tokens.contains(token);
    let reader = CoreReader;
    let mut context = PaletteMappingContext::new(&has, &reader);
    context.base = Some(&base);
    let options = PalettePlanOptions {
        recipe: Some(recipe),
        vibrancy: input.vibrancy,
    };
    let plan = plan_palette_request(&dials, options, &context)?
        .ok_or_else(|| PaletteError::new("themePalette() needs at least one seed color."))?;
    let candidate = generate_tonal_palette(&plan.config)?;
    let mapping = map_palette(&candidate, &dials, &plan.fills, &context)?;
    let tokens: Vec<(String, [String; 2])> = mapping
        .tokens
        .iter()
        .map(|(token, mapped)| {
            let pair = [mapped.light.color.clone(), mapped.dark.color.clone()];
            (token.to_string(), pair)
        })
        .collect();
    let mut components: Vec<(String, Vec<(String, PaletteLift)>)> = Vec::new();
    for (part, block, ground) in LIFT_PARTS {
        let lifted = mapping
            .lift
            .iter()
            .flatten()
            .find(|(background, _)| background == ground);
        let Some((_, lifted)) = lifted else {
            continue;
        };
        let lift = PaletteLift {
            light: lifted.light.color.clone(),
            dark: lifted.dark.color.clone(),
        };
        let index = match components.iter().position(|(p, _)| p == part) {
            Some(index) => index,
            None => {
                components.push((part.to_string(), Vec::new()));
                components.len() - 1
            }
        };
        let blocks = &mut components[index].1;
        match blocks.iter_mut().find(|(b, _)| b == block) {
            Some(entry) => entry.1 = lift,
            None => blocks.push((block.to_string(), lift)),
        }
    }
    Ok(ThemePalette {
        kind: THEME_PALETTE_KIND,
        recipe,
        tokens,
        components,
    })
}

/// Refuses the keys of a palette input that [`theme_palette`] does not read,
/// naming each: `themePalette(): "primary" is not a palette key. It takes
/// accent, neutral, success, warning, danger, info, vibrancy, recipe.` The
/// key check of upstream `themePalette`, for a reader of a JSON input (a
/// [`ThemePaletteInput`] has only the keys it reads).
pub fn check_theme_palette_keys<'k>(
    keys: impl IntoIterator<Item = &'k str>,
) -> Result<(), PaletteError> {
    let unknown: Vec<String> = keys
        .into_iter()
        .filter(|key| !THEME_PALETTE_INPUT_KEYS.contains(key))
        .map(|key| format!("\"{key}\""))
        .collect();
    if unknown.is_empty() {
        return Ok(());
    }
    Err(PaletteError(format!(
        "themePalette(): {} is not a palette key. It takes {}.",
        unknown.join(", "),
        THEME_PALETTE_INPUT_KEYS.join(", ")
    )))
}

/// The recipe a palette input names as text, or upstream's refusal of one
/// it does not know: `themePalette(): recipe "astryx-oklch-v9" is not a
/// palette recipe. It takes astryx-oklch-v1, astryx-oklch-v2.` The recipe
/// check of upstream `themePalette`, for a reader of a JSON input.
pub fn theme_palette_recipe(name: &str) -> Result<PaletteRecipe, PaletteError> {
    PaletteRecipe::from_name(name)
        .filter(|recipe| PALETTE_RECIPES.contains(recipe))
        .ok_or_else(|| {
            let known: Vec<&str> = PALETTE_RECIPES.iter().map(|r| r.as_str()).collect();
            PaletteError(format!(
                "themePalette(): recipe \"{name}\" is not a palette recipe. It takes {}.",
                known.join(", ")
            ))
        })
}

/// The seed the input gives the color setting `id` (upstream
/// `input[id]`).
fn seed_of<'i>(input: &'i ThemePaletteInput, id: &str) -> Option<&'i str> {
    let seed = match id {
        "accent" => &input.accent,
        "neutral" => &input.neutral,
        "success" => &input.success,
        "warning" => &input.warning,
        "danger" => &input.danger,
        "info" => &input.info,
        _ => &None,
    };
    seed.as_deref()
}

/// A string as `JSON.stringify` writes it: in double quotes, `"` and `\`
/// escaped, the control characters as `\b`, `\t`, `\n`, `\f`, `\r` or
/// `\u00xx`, and every other character as written.
fn json_string(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{8}' => out.push_str("\\b"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\u{c}' => out.push_str("\\f"),
            '\r' => out.push_str("\\r"),
            c if u32::from(c) < 0x20 => out.push_str(&format!("\\u{:04x}", u32::from(c))),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// A number as `JSON.stringify` writes it: as `String(n)` does, and `null`
/// for NaN and the infinities.
fn json_number(n: f64) -> String {
    if n.is_finite() {
        js_number_to_string(n)
    } else {
        "null".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::super::generator::PALETTE_RECIPE_V2;
    use super::*;

    // Expected values below come from running upstream's compiled
    // packages/core/dist/palette/themePalette.js under Node v24.11.1, unless a
    // test says otherwise. A palette is rendered as `render` writes it (the
    // recipe, a `token <name> <light> <dark>` line per token, a
    // `lift <part> <block> <light> <dark>` line per part style); a long one is
    // kept as the FNV-1a digest and line count of that rendering.

    const SIX_SEEDS: [(&str, &str); 6] = [
        ("accent", "#5b3cc4"),
        ("neutral", "#6b6b70"),
        ("success", "#1f8a3b"),
        ("warning", "#e0a100"),
        ("danger", "#d1242f"),
        ("info", "#0a6cd6"),
    ];

    /// An input with these seeds, by setting id.
    fn seeds(seeds: &[(&str, &str)]) -> ThemePaletteInput {
        let mut input = ThemePaletteInput::default();
        for &(id, seed) in seeds {
            let slot = match id {
                "accent" => &mut input.accent,
                "neutral" => &mut input.neutral,
                "success" => &mut input.success,
                "warning" => &mut input.warning,
                "danger" => &mut input.danger,
                "info" => &mut input.info,
                _ => panic!("no setting {id}"),
            };
            *slot = Some(seed.to_string());
        }
        input
    }

    fn palette(input: &ThemePaletteInput) -> ThemePalette {
        theme_palette(input).unwrap_or_else(|e| panic!("{input:?}: {e}"))
    }

    fn token<'p>(palette: &'p ThemePalette, name: &str) -> Option<&'p [String; 2]> {
        palette
            .tokens
            .iter()
            .find(|(token, _)| token == name)
            .map(|(_, pair)| pair)
    }

    fn lift<'p>(palette: &'p ThemePalette, part: &str, block: &str) -> Option<&'p PaletteLift> {
        let (_, blocks) = palette.components.iter().find(|(p, _)| p == part)?;
        blocks.iter().find(|(b, _)| b == block).map(|(_, l)| l)
    }

    /// A palette as the fixtures write it, line by line.
    fn render(palette: &ThemePalette) -> String {
        let mut out = vec![format!("recipe {}", palette.recipe)];
        for (name, [light, dark]) in &palette.tokens {
            out.push(format!("token {name} {light} {dark}"));
        }
        for (part, blocks) in &palette.components {
            for (block, lift) in blocks {
                out.push(format!("lift {part} {block} {} {}", lift.light, lift.dark));
            }
        }
        out.join("\n")
    }

    /// FNV-1a, 64 bits, of a rendering.
    fn fnv64(text: &str) -> u64 {
        text.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
        })
    }

    /// A rendering whose digest and line count are upstream's.
    fn assert_upstream(label: &str, actual: &str, (digest, lines): (u64, usize)) {
        let (got, count) = (fnv64(actual), actual.lines().count());
        assert!(
            got == digest && count == lines,
            "{label}: rendering differs from upstream's (digest {digest:#018x}, {lines} lines; got {got:#018x}, {count} lines)"
        );
    }

    /// Upstream's `HEX`: a lowercase `#rrggbb` or `#rrggbbaa`.
    fn is_token_hex(value: &str) -> bool {
        let digits = value.strip_prefix('#').unwrap_or("");
        (digits.len() == 6 || digits.len() == 8)
            && digits
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    }

    fn is_six_digit_hex(value: &str) -> bool {
        value.len() == 7 && is_token_hex(value)
    }

    /// Every token a palette sets is one Core has, and a lowercase hex in
    /// both modes: upstream `expectTokensApplied` short of a theme (reading
    /// the tokens back through `defineTheme` waits on that port).
    fn assert_tokens_applied(palette: &ThemePalette, label: &str) {
        let core: HashSet<&str> = TOKEN_DEFAULTS.iter().map(|&(name, _)| name).collect();
        for (name, [light, dark]) in &palette.tokens {
            assert!(core.contains(name.as_str()), "{label}: {name}");
            assert!(is_token_hex(light), "{label}: {name} light {light}");
            assert!(is_token_hex(dark), "{label}: {name} dark {dark}");
        }
    }

    /// Upstream themePalette.test.ts's `seeded`: a linear congruential
    /// sequence, and colors drawn from it.
    struct Seeded(f64);

    impl Seeded {
        fn next(&mut self) -> f64 {
            self.0 = (self.0 * 1664525.0 + 1013904223.0) % 4294967296.0;
            self.0 / 4294967296.0
        }

        fn color(&mut self) -> String {
            format!(
                "#{:06x}",
                (self.next() * f64::from(0xff_ffff)).floor() as u32
            )
        }
    }

    /// The palette of `{accent: '#5b3cc4', neutral: '#6b6b70'}`, upstream's
    /// `defineTheme({palette})` example.
    const TWO: &str = r"recipe astryx-oklch-v1
token --color-background-body #f0f0f3 #1b1b1d
token --color-background-surface #ffffff #252528
token --color-background-card #ffffff #1b1b1d
token --color-background-popover #ffffff #1b1b1d
token --color-background-muted #f0f0f3 #1b1b1d
token --color-text-primary #000000 #ffffff
token --color-text-disabled #909096 #535356
token --color-icon-primary #000000 #ffffff
token --color-icon-disabled #909096 #535356
token --color-border-emphasized #d4d4d8 #535356
token --color-skeleton #d4d4d8 #535356
token --color-on-dark #ffffff #ffffff
token --color-on-light #111112 #111112
token --color-background-inverted #1b1b1e #f0f0f3
token --color-border #00000014 #ffffff1a
token --color-overlay-hover #0000000d #ffffff0d
token --color-overlay-pressed #0000001a #ffffff1a
token --color-text-secondary #46464b #9d9da1
token --color-icon-secondary #6b6b70 #9d9da1
token --color-accent #5b3cc4 #a59efa
token --color-on-accent #ffffff #111112
token --color-accent-muted #d0cfff #443688
token --color-text-accent #5b3cc4 #c2beff
token --color-icon-accent #5b3cc4 #c2beff
token --focus-outline-color #5b3cc4 #a59efa
lift item selected #46464b #c6c6c9
lift tree-list-item selected #46464b #c6c6c9
lift file-input mode:dropzone #46464b #c6c6c9
lift citation variant:number #46464b #c6c6c9
lift segmented-control-item base #46464b #d4d4d7
lift card variant:blue #46464b #ababaf
lift card variant:cyan #46464b #b8b8bc
lift card variant:green #46464b #b8b8bc
lift card variant:orange #46464b #b8b8bc
lift card variant:pink #46464b #ababaf
lift card variant:purple #46464b #ababaf
lift card variant:red #46464b #ababaf
lift card variant:teal #46464b #b8b8bc
lift card variant:yellow #46464b #b8b8bc";

    #[test]
    fn maps_two_seeds_as_upstream() {
        let two = palette(&seeds(&[("accent", "#5b3cc4"), ("neutral", "#6b6b70")]));
        let actual = render(&two);
        for (index, (a, e)) in actual.lines().zip(TWO.lines()).enumerate() {
            assert_eq!(a, e, "line {}", index + 1);
        }
        assert_eq!(actual.lines().count(), TWO.lines().count());
        assert_eq!(two.kind, "theme-palette");
    }

    #[test]
    fn maps_the_seeds_to_the_full_color_token_set_status_included() {
        // Upstream themePalette.test.ts.
        let six = palette(&seeds(&SIX_SEEDS));
        assert_tokens_applied(&six, "six");
        for name in [
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
            assert!(token(&six, name).is_some(), "{name}");
        }
        // The accent keeps the exact color given, as the editor anchors it.
        assert_eq!(token(&six, "--color-accent").unwrap()[0], "#5b3cc4");
        assert!(lift(&six, "item", "selected").is_some());
        assert_upstream("six", &render(&six), SIX_DIGEST);
    }

    #[test]
    fn runs_v1_when_no_recipe_is_named_and_v2_only_when_named() {
        // Upstream themePalette.test.ts.
        let base = seeds(&[("accent", "#5b3cc4"), ("neutral", "#706a62")]);
        let plain = palette(&base);
        assert_eq!(plain.recipe, PALETTE_RECIPE);
        let named = ThemePaletteInput {
            recipe: Some(PALETTE_RECIPE),
            ..base.clone()
        };
        assert_eq!(plain, palette(&named));
        let v2 = palette(&ThemePaletteInput {
            recipe: Some(PALETTE_RECIPE_V2),
            ..base
        });
        assert_eq!(v2.recipe, PALETTE_RECIPE_V2);
        assert_ne!(v2.tokens, plain.tokens);
        assert_upstream("v1", &render(&plain), V1_DIGEST);
        assert_upstream("v2", &render(&v2), V2_DIGEST);
    }

    #[test]
    fn gives_the_same_result_for_the_same_input() {
        // Upstream themePalette.test.ts; upstream also checks the result is
        // frozen, which a Rust value is unless its owner makes it mutable.
        let input = ThemePaletteInput {
            vibrancy: Some(20.0),
            ..seeds(&[("accent", "#c43c7a"), ("info", "#2b6cb0")])
        };
        let a = palette(&input);
        assert_eq!(palette(&input), a);
        assert_upstream("same input", &render(&a), SAME_INPUT_DIGEST);
    }

    #[test]
    fn builds_a_palette_for_random_seeds_settings_and_recipes() {
        // Upstream themePalette.test.ts: 40 random inputs.
        let mut seeded = Seeded(20261116.0);
        let mut rendered = Vec::new();
        for i in 0..40 {
            let mut chosen: Vec<(&str, String)> = Vec::new();
            for dial in PALETTE_DIALS {
                if seeded.next() < 0.6 {
                    chosen.push((dial, seeded.color()));
                }
            }
            if chosen.is_empty() {
                chosen.push(("accent", seeded.color()));
            }
            let chosen: Vec<(&str, &str)> = chosen.iter().map(|(k, v)| (*k, v.as_str())).collect();
            let mut input = seeds(&chosen);
            if seeded.next() < 0.5 {
                input.vibrancy = Some((seeded.next() * 101.0).floor());
            }
            if seeded.next() < 0.5 {
                input.recipe = Some(PALETTE_RECIPE_V2);
            }
            let palette = palette(&input);
            assert_tokens_applied(&palette, &format!("random {i}"));
            rendered.push(format!("palette {i}"));
            rendered.push(render(&palette));
        }
        assert_upstream("40 random", &rendered.join("\n"), RANDOM_40_DIGEST);
    }

    #[test]
    fn builds_a_palette_for_extreme_seeds_in_every_role_both_recipes_and_the_vibrancy_ends() {
        // Upstream themePalette.test.ts.
        let mut rendered = Vec::new();
        for seed in [
            "#000000", "#ffffff", "#ff0000", "#00ff00", "#0000ff", "#ffff00", "#808080", "#010101",
            "#fefefe",
        ] {
            let every: Vec<(&str, &str)> = PALETTE_DIALS.iter().map(|d| (*d, seed)).collect();
            for recipe in [PALETTE_RECIPE, PALETTE_RECIPE_V2] {
                for vibrancy in [0.0, 100.0] {
                    let input = ThemePaletteInput {
                        recipe: Some(recipe),
                        vibrancy: Some(vibrancy),
                        ..seeds(&every)
                    };
                    let palette = palette(&input);
                    assert_tokens_applied(&palette, seed);
                    rendered.push(format!("palette {seed} {recipe} {vibrancy}"));
                    rendered.push(render(&palette));
                }
            }
        }
        assert_upstream("extreme", &rendered.join("\n"), EXTREME_DIGEST);
    }

    #[test]
    fn reads_a_seed_in_any_form_core_reads_a_color_in() {
        // Upstream themePalette.test.ts: rgb() and an uppercase hex make one
        // palette. The other forms, and the digests, are from Node.
        let hex = palette(&seeds(&[("accent", "#5B3CC4")]));
        for form in [
            "rgb(91, 60, 196)",
            "  #5b3cc4  ",
            "rgba(91,60,196,1)",
            "#5b3cc4ff",
            "rgb(35.7% 23.5% 76.9%)",
        ] {
            assert_eq!(palette(&seeds(&[("accent", form)])), hex, "{form}");
        }
        for (form, digest) in [
            ("#5B3CC4", FORM_DIGESTS[0]),
            ("black", FORM_DIGESTS[1]),
            ("WHITE", FORM_DIGESTS[2]),
            ("#abc", FORM_DIGESTS[3]),
        ] {
            let rendered = render(&palette(&seeds(&[("accent", form)])));
            assert_eq!(fnv64(&rendered), digest, "{form}:\n{rendered}");
        }
    }

    #[test]
    fn gives_upstreams_palette_for_300_random_inputs() {
        // Upstream themePalette.test.ts's "gives the same palette for 300
        // random inputs" measures themePalette against Core's defaults as
        // defineTheme and resolveThemeTokens give them; until those are
        // ported, this checks the same 300 inputs against upstream's own
        // themePalette output.
        let mut seeded = Seeded(300.0);
        let mut rendered = Vec::new();
        for i in 0..300 {
            let mut chosen: Vec<(&str, String)> = Vec::new();
            for dial in PALETTE_DIALS {
                if seeded.next() < 0.5 {
                    chosen.push((dial, seeded.color()));
                }
            }
            if chosen.is_empty() {
                chosen.push(("accent", seeded.color()));
            }
            let chosen: Vec<(&str, &str)> = chosen.iter().map(|(k, v)| (*k, v.as_str())).collect();
            let mut input = seeds(&chosen);
            if seeded.next() < 0.4 {
                input.vibrancy = Some((seeded.next() * 101.0).floor());
            }
            input.recipe = Some(if seeded.next() < 0.5 {
                PALETTE_RECIPE_V2
            } else {
                PALETTE_RECIPE
            });
            rendered.push(format!("palette {i}"));
            rendered.push(render(&palette(&input)));
        }
        assert_upstream("300 random", &rendered.join("\n"), RANDOM_300_DIGEST);
    }

    #[test]
    fn refuses_malformed_input_saying_what_to_fix() {
        // Upstream themePalette.test.ts's refusals, with the whole messages
        // upstream throws; the vibrancy, transparent, escaped and oklch()
        // cases are extra ones run the same way.
        let accent = |vibrancy: f64| ThemePaletteInput {
            vibrancy: Some(vibrancy),
            ..seeds(&[("accent", "#5b3cc4")])
        };
        let seed_message = |id: &str, json: &str| {
            format!(
                "themePalette(): {id} is {json}, and a seed is one opaque color: a hex such as \"#5b3cc4\", rgb(), black or white."
            )
        };
        let vibrancy_message = |got: &str| {
            format!("themePalette(): vibrancy is a whole number from 0 to 100, got {got}.")
        };
        let cases: Vec<(ThemePaletteInput, String)> = vec![
            (
                ThemePaletteInput::default(),
                "themePalette() needs at least one seed color: accent, neutral, success, warning, danger, info.".to_string(),
            ),
            (
                seeds(&[("accent", "var(--brand)")]),
                seed_message("accent", "\"var(--brand)\""),
            ),
            (
                seeds(&[("accent", "#5b3cc480")]),
                seed_message("accent", "\"#5b3cc480\""),
            ),
            (accent(101.0), vibrancy_message("101")),
            (accent(2.5), vibrancy_message("2.5")),
            (accent(f64::NAN), vibrancy_message("null")),
            (accent(-1.0), vibrancy_message("-1")),
            (
                seeds(&[("accent", "transparent")]),
                seed_message("accent", "\"transparent\""),
            ),
            (
                seeds(&[("neutral", "say \"hi\"\\\n\t")]),
                seed_message("neutral", r#""say \"hi\"\\\n\t""#),
            ),
            (
                seeds(&[("accent", "#5b3cc4"), ("info", "oklch(0.5 0.1 200)")]),
                seed_message("info", "\"oklch(0.5 0.1 200)\""),
            ),
        ];
        for (input, message) in cases {
            assert_eq!(
                theme_palette(&input),
                Err(PaletteError(message)),
                "{input:?}"
            );
        }
        // The vibrancy ends are whole numbers in range.
        assert!(theme_palette(&accent(0.0)).is_ok());
        assert!(theme_palette(&accent(100.0)).is_ok());
    }

    #[test]
    fn refuses_unknown_keys_and_recipes_as_upstream() {
        // Upstream themePalette.test.ts's "primary" and astryx-oklch-v9
        // refusals, with the whole messages upstream throws.
        assert_eq!(
            check_theme_palette_keys(["accent", "primary"]),
            Err(PaletteError(
                "themePalette(): \"primary\" is not a palette key. It takes accent, neutral, success, warning, danger, info, vibrancy, recipe.".into()
            ))
        );
        assert_eq!(
            check_theme_palette_keys(["accent", "primary", "secondary"]),
            Err(PaletteError(
                "themePalette(): \"primary\", \"secondary\" is not a palette key. It takes accent, neutral, success, warning, danger, info, vibrancy, recipe.".into()
            ))
        );
        assert_eq!(check_theme_palette_keys(THEME_PALETTE_INPUT_KEYS), Ok(()));
        assert_eq!(
            theme_palette_recipe("astryx-oklch-v9"),
            Err(PaletteError(
                "themePalette(): recipe \"astryx-oklch-v9\" is not a palette recipe. It takes astryx-oklch-v1, astryx-oklch-v2.".into()
            ))
        );
        assert_eq!(theme_palette_recipe("astryx-oklch-v1"), Ok(PALETTE_RECIPE));
        assert_eq!(
            theme_palette_recipe("astryx-oklch-v2"),
            Ok(PALETTE_RECIPE_V2)
        );
    }

    #[test]
    fn input_keys_are_the_settings_then_vibrancy_and_recipe() {
        // Upstream INPUT_KEYS is [...PALETTE_DIALS, 'vibrancy', 'recipe'].
        let mut expected: Vec<&str> = PALETTE_DIALS.to_vec();
        expected.extend(["vibrancy", "recipe"]);
        assert_eq!(THEME_PALETTE_INPUT_KEYS.to_vec(), expected);
    }

    #[test]
    fn lifts_secondary_text_on_each_coloured_card() {
        // Upstream themePalette.test.ts (defineTheme({palette})).
        let two = palette(&seeds(&[("accent", "#5b3cc4"), ("neutral", "#6b6b70")]));
        for color in ["blue", "green", "red", "yellow"] {
            let lift = lift(&two, "card", &format!("variant:{color}")).expect(color);
            assert!(lift.light.starts_with('#'), "{color}");
            assert!(lift.dark.starts_with('#'), "{color}");
        }
    }

    #[test]
    fn lifts_selected_item_text_to_a_six_digit_hex_in_each_mode() {
        // The palette half of upstream's "compiles the lifted secondary text
        // into each mode a browser reads"; the CSS half waits on
        // generateThemeCSS.
        let two = palette(&seeds(&[("accent", "#5b3cc4"), ("neutral", "#6b6b70")]));
        let lift = lift(&two, "item", "selected").unwrap();
        assert!(is_six_digit_hex(&lift.light), "{}", lift.light);
        assert!(is_six_digit_hex(&lift.dark), "{}", lift.dark);
    }

    #[test]
    fn core_reader_reads_hex_rgb_names_and_transparent() {
        // From reading coreReader: parseColor's channels, an opacity of 1
        // for what it cannot read, and 0 for transparent in any case.
        let reader = CoreReader;
        assert_eq!(reader.rgb("#5b3cc4"), Some([91.0, 60.0, 196.0]));
        assert_eq!(reader.rgb("rgb(91, 60, 196)"), Some([91.0, 60.0, 196.0]));
        assert_eq!(reader.rgb("white"), Some([255.0, 255.0, 255.0]));
        assert_eq!(reader.rgb("var(--brand)"), None);
        assert_eq!(reader.alpha("#00000080"), 128.0 / 255.0);
        assert_eq!(reader.alpha("rgba(0, 0, 0, 0.25)"), 0.25);
        assert_eq!(reader.alpha("  Transparent "), 0.0);
        assert_eq!(reader.alpha("var(--brand)"), 1.0);
    }

    #[test]
    fn default_base_is_cores_defaults_resolved_in_each_mode() {
        // Core's resolved defaults (resolveDefaultTokens): text-primary is
        // #0A1317 in light, and the base has no part styles.
        let base = default_base();
        assert_eq!(
            base.light.get("--color-text-primary").map(String::as_str),
            Some("#0A1317")
        );
        assert_eq!(base.light.len(), TOKEN_DEFAULTS.len());
        assert_eq!(base.dark.len(), TOKEN_DEFAULTS.len());
        assert_eq!(base.components, Some(Vec::new()));
    }

    #[test]
    fn json_string_escapes_as_json_stringify() {
        // JSON.stringify's QuoteJSONString: quotes, backslashes and control
        // characters escaped, the rest written as is.
        assert_eq!(json_string("a\"b\\c"), r#""a\"b\\c""#);
        assert_eq!(
            json_string("\u{8}\t\n\u{c}\r\u{1}\u{1f}"),
            r#""\b\t\n\f\r\u0001\u001f""#
        );
        assert_eq!(json_string("é/\u{2028}"), "\"é/\u{2028}\"");
        assert_eq!(json_number(101.0), "101");
        assert_eq!(json_number(f64::INFINITY), "null");
    }

    const SIX_DIGEST: (u64, usize) = (0x5e9f_482a_e05a_84d8, 68);
    const V1_DIGEST: (u64, usize) = (0x692b_f48c_5f14_118a, 40);
    const V2_DIGEST: (u64, usize) = (0x62d6_038a_56bb_7309, 40);
    const SAME_INPUT_DIGEST: (u64, usize) = (0xecbc_deff_7f2a_5191, 28);
    const RANDOM_40_DIGEST: (u64, usize) = (0xa3a2_a769_92b3_23e4, 1953);
    const EXTREME_DIGEST: (u64, usize) = (0xb8a5_7e8c_ab76_5caa, 2484);
    const RANDOM_300_DIGEST: (u64, usize) = (0xecff_4fcc_c5a7_f0f3, 12745);
    /// `#5B3CC4` (and the forms that read as it), `black`, `WHITE`, `#abc`.
    const FORM_DIGESTS: [u64; 4] = [
        0x4cdc_2a61_b18a_da5c,
        0xe319_a24d_0f13_eb6f,
        0x4061_8f0d_c924_780d,
        0xa3b6_d02e_452b_7f0f,
    ];
}
