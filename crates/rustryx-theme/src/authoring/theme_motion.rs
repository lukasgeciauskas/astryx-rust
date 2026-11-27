//! A theme's motion configuration resolved to tokens: upstream
//! `theme/themeMotion.ts` (`spec:AST-108` FR1, FR2, FR5, FR6).
//!
//! A theme's `motion` is the knobs ([`ThemeMotionKnobs`]), a motion scale
//! ([`MotionScaleConfig`]), or both, held here as one [`ThemeMotionConfig`].
//! `tempo` scales every duration token, clamped to 0.75 to 1.5; `character`
//! picks the entrance and move curves. With the defaults (tempo 1,
//! `standard`) every token is the core default. A motion scale's durations win
//! over `tempo` (FR5); a theme's explicit tokens win over both where the theme
//! resolves its values.
//!
//! The play-time side, the tempo springs and timings play at, is
//! `rustryx_primitives::motion`; this module only writes and resolves the
//! theme's tokens.

use std::collections::HashMap;

use super::js::{is_js_whitespace, js_number_to_string as js_number_string, js_round};
use super::motion_knobs::resolve_knobs;
pub use super::motion_knobs::{
    MotionCharacter, ResolvedMotionKnobs, TEMPO_RANGE, ThemeMotionKnobs, ThemeReducedMotion,
};
use super::motion_scale::{MotionScaleConfig, expand_motion_scale};
use super::token_references::resolve_token_references;

/// The core duration tokens and their defaults, in upstream's order (upstream
/// `durationDefaults` in `theme/tokens.stylex.ts`). `--duration-exit`, the
/// standard dismissal duration, is a reference to `--duration-fast`.
pub const DURATION_DEFAULTS: [(&str, &str); 10] = [
    ("--duration-fast-min", "130ms"),
    ("--duration-fast", "175ms"),
    ("--duration-fast-max", "230ms"),
    ("--duration-medium-min", "310ms"),
    ("--duration-medium", "410ms"),
    ("--duration-medium-max", "550ms"),
    ("--duration-slow-min", "730ms"),
    ("--duration-slow", "975ms"),
    ("--duration-slow-max", "1300ms"),
    ("--duration-exit", "var(--duration-fast)"),
];

/// The core easing tokens and their defaults, in upstream's order (upstream
/// `easeDefaults` in `theme/tokens.stylex.ts`). `--ease-standard` decelerates
/// (arrivals and state changes); `--ease-entrance` and `--ease-move` equal it
/// at the `standard` character; `--ease-exit` accelerates (things leaving).
pub const EASE_DEFAULTS: [(&str, &str); 4] = [
    ("--ease-standard", "cubic-bezier(0.24, 1, 0.4, 1)"),
    ("--ease-entrance", "cubic-bezier(0.24, 1, 0.4, 1)"),
    ("--ease-move", "cubic-bezier(0.24, 1, 0.4, 1)"),
    ("--ease-exit", "cubic-bezier(0.3, 0, 0.6, 0.6)"),
];

/// The entrance and move curve at each character (upstream `CHARACTER_EASE`).
/// `standard` has none: it is the theme's `--ease-standard`, so a theme that
/// overrides that easing keeps its entrances on it.
pub const CHARACTER_EASE: [(MotionCharacter, Option<&str>); 3] = [
    (MotionCharacter::Crisp, Some("cubic-bezier(0, 0, 0.2, 1)")),
    (MotionCharacter::Standard, None),
    (
        MotionCharacter::Expressive,
        Some("cubic-bezier(0.3, 1.2, 0.5, 1)"),
    ),
];

/// `character`'s entrance and move curve, `CHARACTER_EASE[character]`.
pub fn character_ease(character: MotionCharacter) -> Option<&'static str> {
    CHARACTER_EASE
        .iter()
        .find(|(c, _)| *c == character)
        .and_then(|(_, ease)| *ease)
}

/// The private custom property a theme with a `tempo` writes beside its
/// duration tokens (upstream `MOTION_TEMPO_PROPERTY`). A layer whose durations
/// come from a duration scale alone writes 1. The animator's springs read it,
/// and so does a style that scales a clock of its own (ProgressBar's move).
pub const MOTION_TEMPO_PROPERTY: &str = "--_astryx-motion-tempo";

/// The custom property a theme whose `reducedMotion` resolves to `always` sets
/// to `1` on its root (upstream `REDUCED_MOTION_PROPERTY`), for styles that
/// cannot key a selector on the mark.
pub const REDUCED_MOTION_PROPERTY: &str = "--_astryx-reduced-motion";

/// A theme's `motion` (upstream `ThemeMotionConfig`): the knobs, a motion
/// scale, or both, as one object with every field optional. It is a motion
/// scale only when `fast`, `medium` and `ratio` are all set
/// ([`ThemeMotionConfig::scale`]); a scale's fields win over `tempo` for the
/// durations they set.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ThemeMotionConfig {
    /// [`MotionScaleConfig::fast`].
    pub fast: Option<f64>,
    /// [`MotionScaleConfig::medium`].
    pub medium: Option<f64>,
    /// [`MotionScaleConfig::slow`].
    pub slow: Option<f64>,
    /// [`MotionScaleConfig::ratio`].
    pub ratio: Option<f64>,
    /// [`MotionScaleConfig::easing`].
    pub easing: Option<String>,
    /// [`ThemeMotionKnobs::tempo`].
    pub tempo: Option<f64>,
    /// [`ThemeMotionKnobs::character`].
    pub character: Option<MotionCharacter>,
}

impl ThemeMotionConfig {
    /// The knobs this config sets.
    pub fn knobs(&self) -> ThemeMotionKnobs {
        ThemeMotionKnobs {
            tempo: self.tempo,
            character: self.character,
        }
    }

    /// The motion scale this config is, when it sets `fast`, `medium` and
    /// `ratio` (upstream `isScaleConfig`). A config with only some of them is
    /// knobs alone here; upstream's `defineTheme` refuses it.
    pub fn scale(&self) -> Option<MotionScaleConfig> {
        Some(MotionScaleConfig {
            fast: self.fast?,
            medium: self.medium?,
            slow: self.slow,
            ratio: self.ratio?,
            easing: self.easing.clone(),
        })
    }
}

impl From<ThemeMotionKnobs> for ThemeMotionConfig {
    fn from(knobs: ThemeMotionKnobs) -> Self {
        Self {
            tempo: knobs.tempo,
            character: knobs.character,
            ..Self::default()
        }
    }
}

impl From<MotionScaleConfig> for ThemeMotionConfig {
    fn from(scale: MotionScaleConfig) -> Self {
        Self {
            fast: Some(scale.fast),
            medium: Some(scale.medium),
            slow: scale.slow,
            ratio: Some(scale.ratio),
            easing: scale.easing,
            ..Self::default()
        }
    }
}

/// The knobs of `config`, defaults filled in and `tempo` clamped (upstream
/// `resolveMotionKnobs`).
pub fn resolve_motion_knobs(config: Option<&ThemeMotionConfig>) -> ResolvedMotionKnobs {
    resolve_knobs(config.map(ThemeMotionConfig::knobs).as_ref())
}

/// JavaScript's `Number.parseFloat`: the longest decimal number at the start of
/// `value` after leading whitespace (`175ms` is 175), `NaN` when there is
/// none.
fn parse_float(value: &str) -> f64 {
    let s = value.trim_start_matches(is_js_whitespace);
    let bytes = s.as_bytes();
    let mut end = usize::from(matches!(bytes.first(), Some(b'+' | b'-')));
    if s[end..].starts_with("Infinity") {
        return if s.starts_with('-') {
            f64::NEG_INFINITY
        } else {
            f64::INFINITY
        };
    }
    let int_start = end;
    while bytes.get(end).is_some_and(u8::is_ascii_digit) {
        end += 1;
    }
    let mut digits = end - int_start;
    if bytes.get(end) == Some(&b'.') {
        let mut frac = end + 1;
        while bytes.get(frac).is_some_and(u8::is_ascii_digit) {
            frac += 1;
        }
        let frac_digits = frac - end - 1;
        if digits + frac_digits > 0 {
            end = frac;
            digits += frac_digits;
        }
    }
    if digits == 0 {
        return f64::NAN;
    }
    if matches!(bytes.get(end), Some(b'e' | b'E')) {
        let mut exp = end + 1;
        if matches!(bytes.get(exp), Some(b'+' | b'-')) {
            exp += 1;
        }
        let exp_start = exp;
        while bytes.get(exp).is_some_and(u8::is_ascii_digit) {
            exp += 1;
        }
        if exp > exp_start {
            end = exp;
        }
    }
    s[..end].parse().unwrap_or(f64::NAN)
}

/// A duration token's value in ms, `175ms` → 175 (upstream `msOf`).
fn ms_of(value: &str) -> f64 {
    parse_float(value)
}

/// The duration defaults that are values, not references (upstream
/// `TIMED_DURATION_DEFAULTS`): a reference such as `--duration-exit` follows
/// the token it names, so tempo writes no value for it.
fn timed_duration_defaults() -> impl Iterator<Item = (&'static str, &'static str)> {
    DURATION_DEFAULTS
        .into_iter()
        .filter(|(_, value)| !value.starts_with("var("))
}

/// The default of the core easing token `name`.
fn ease_default(name: &str) -> &'static str {
    EASE_DEFAULTS
        .iter()
        .find(|(key, _)| *key == name)
        .map(|(_, value)| *value)
        .unwrap_or_default()
}

/// The duration tokens at `tempo`: each default that is a value, times
/// `tempo`, rounded to the ms (upstream `durationTokensAt`).
pub fn duration_tokens_at(tempo: f64) -> Vec<(String, String)> {
    timed_duration_defaults()
        .map(|(name, value)| {
            let ms = js_round(ms_of(value) * tempo);
            (name.to_string(), format!("{}ms", js_number_string(ms)))
        })
        .collect()
}

/// The private custom property holding a duration token's value before
/// `tempo` scaled it (upstream `preTempoName`): `--duration-slow` →
/// `--_astryx-pre-tempo-duration-slow`. Any other name is returned as is.
/// Busy loops (Spinner, Skeleton) read it, falling back to the token, so their
/// timing follows a duration scale or an explicit token but never `tempo`
/// (FR4).
pub fn pre_tempo_name(token: &str) -> String {
    match token.strip_prefix("--duration-") {
        Some(rest) => format!("--_astryx-pre-tempo-duration-{rest}"),
        None => token.to_string(),
    }
}

/// `tokens[name] = value` on a JavaScript object: a name already there keeps
/// its place and takes the new value, a new one goes last.
fn assign(tokens: &mut Vec<(String, String)>, name: &str, value: &str) {
    match tokens.iter_mut().find(|(key, _)| key == name) {
        Some(entry) => entry.1 = value.to_string(),
        None => tokens.push((name.to_string(), value.to_string())),
    }
}

/// The motion tokens `config` writes (upstream `expandThemeMotion`, FR2 and
/// FR5), in upstream's order:
///
/// - with a `tempo`, every `--duration-*` that is a value times it, each
///   default's pre-tempo copy ([`pre_tempo_name`]), and
///   [`MOTION_TEMPO_PROPERTY`];
/// - with a motion scale, its tokens, winning over the tempo's (and with a
///   tempo, the pre-tempo copies of its durations);
/// - `--ease-entrance` and `--ease-move` at the character's curve; at
///   `standard` the scale's easing, or `--ease-standard`'s default when the
///   config names the character.
///
/// A knob the config leaves out writes nothing, so a theme extending another
/// keeps the base's value; a knob it sets is written even at its default, so a
/// child can set a base's tempo back to 1.
pub fn expand_theme_motion(config: &ThemeMotionConfig) -> Vec<(String, String)> {
    let knobs = resolve_motion_knobs(Some(config));
    let mut tokens = Vec::new();
    if knobs.tempo_is_set {
        for (name, value) in duration_tokens_at(knobs.tempo) {
            assign(&mut tokens, &name, &value);
        }
        for (name, value) in timed_duration_defaults() {
            assign(&mut tokens, &pre_tempo_name(name), value);
        }
        // The press release fade, a fixed clock in the stylesheet, scales by
        // the tempo itself (FR4; `component:Button` FR15), read where it plays.
        assign(
            &mut tokens,
            MOTION_TEMPO_PROPERTY,
            &js_number_string(knobs.tempo),
        );
    }
    let scale = config.scale();
    if let Some(scale) = &scale {
        let scale_tokens = expand_motion_scale(scale);
        for (name, value) in &scale_tokens {
            assign(&mut tokens, name, value);
        }
        if knobs.tempo_is_set {
            for (name, value) in &scale_tokens {
                if name.starts_with("--duration-") {
                    assign(&mut tokens, &pre_tempo_name(name), value);
                }
            }
        }
    }
    let scale_easing = scale.as_ref().and(config.easing.as_deref());
    let ease = character_ease(knobs.character).or_else(|| {
        if config.character.is_some() {
            Some(scale_easing.unwrap_or_else(|| ease_default("--ease-standard")))
        } else {
            scale_easing
        }
    });
    if let Some(ease) = ease {
        assign(&mut tokens, "--ease-entrance", ease);
        assign(&mut tokens, "--ease-move", ease);
    }
    tokens
}

/// What [`resolve_theme_motion`] reads from a theme (upstream's parameter of
/// `resolveThemeMotion`).
#[derive(Clone, Copy, Debug, Default)]
pub struct ThemeMotionSource<'a> {
    /// The theme's own `motion`.
    pub motion: Option<&'a ThemeMotionConfig>,
    /// A defined theme's motion axis (upstream `__axes.motion`): its own merged
    /// with its base's. It wins over `motion`.
    pub axes_motion: Option<&'a ThemeMotionConfig>,
    /// The theme's explicit tokens whose value is a plain string (a light/dark
    /// pair is not a motion value, so upstream leaves those out).
    pub tokens: &'a [(String, String)],
    /// The theme's local tokens whose value is a plain string.
    pub local_tokens: &'a [(String, String)],
}

/// What [`resolve_theme_motion`] returns for a theme (upstream `ThemeMotion`).
#[derive(Clone, Debug, PartialEq)]
pub struct ThemeMotion {
    /// The knobs, defaults filled in.
    pub knobs: ResolvedMotionKnobs,
    /// Every `--duration-*` token's value, in [`DURATION_DEFAULTS`] order.
    pub durations: Vec<(String, String)>,
    /// `--ease-standard`, `--ease-entrance` and `--ease-move`.
    pub easings: Vec<(String, String)>,
}

/// A theme's motion values, for the animator and for code that has no
/// stylesheet (upstream `resolveThemeMotion`, FR6): the knobs, and the duration
/// and easing tokens they resolve to with the theme's explicit tokens applied.
///
/// The values resolve over the core defaults, then the knobs' tokens, then the
/// theme's local tokens and its tokens; a token written as a reference
/// (`var(--name)`) resolves through them (`architecture:theme-tokens` INV6).
/// `--ease-entrance` and `--ease-move` are `--ease-standard`'s value unless
/// the theme or its knobs write them.
pub fn resolve_theme_motion(theme: &ThemeMotionSource<'_>) -> ThemeMotion {
    // A defined theme's motion is its axes': its own merged with its base's.
    let config = theme.axes_motion.or(theme.motion);
    let knobs = resolve_motion_knobs(config);
    let generated = config.map(expand_theme_motion).unwrap_or_default();

    let mut graph: Vec<(String, String)> = DURATION_DEFAULTS
        .iter()
        .chain(EASE_DEFAULTS.iter())
        .map(|(name, value)| (name.to_string(), value.to_string()))
        .collect();
    for (name, value) in &generated {
        assign(&mut graph, name, value);
    }
    for tokens in [theme.local_tokens, theme.tokens] {
        for (name, value) in tokens {
            assign(&mut graph, name, value);
        }
    }

    let explicit = |name: &str| {
        theme.tokens.iter().any(|(key, _)| key == name)
            || generated.iter().any(|(key, _)| key == name)
    };
    let resolved: HashMap<String, String> = resolve_token_references(&graph).into_iter().collect();
    let value = |name: &str| resolved.get(name).cloned().unwrap_or_default();

    let durations = DURATION_DEFAULTS
        .iter()
        .map(|(name, _)| (name.to_string(), value(name)))
        .collect();
    let standard = value("--ease-standard");
    let follow = |name: &str| {
        if explicit(name) {
            value(name)
        } else {
            standard.clone()
        }
    };
    let entrance = follow("--ease-entrance");
    let move_ease = follow("--ease-move");
    ThemeMotion {
        knobs,
        durations,
        easings: vec![
            ("--ease-standard".to_string(), standard),
            ("--ease-entrance".to_string(), entrance),
            ("--ease-move".to_string(), move_ease),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustryx_primitives::motion::theme_timing::DECLARED_TIMING_TOKENS;

    /// Today's band durations in ms, in [`DURATION_DEFAULTS`] order (upstream's
    /// `TODAY_MS`).
    const TODAY_MS: [f64; 9] = [130., 175., 230., 310., 410., 550., 730., 975., 1300.];
    const STANDARD: &str = "cubic-bezier(0.24, 1, 0.4, 1)";

    fn get<'a>(tokens: &'a [(String, String)], name: &str) -> Option<&'a str> {
        tokens
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }

    /// The band durations of `tokens` in ms (upstream's `durationsMs`).
    fn durations_ms(tokens: &[(String, String)]) -> Vec<f64> {
        timed_duration_defaults()
            .map(|(name, _)| get(tokens, name).map_or(f64::NAN, parse_float))
            .collect()
    }

    fn tempo(tempo: f64) -> ThemeMotionConfig {
        ThemeMotionConfig {
            tempo: Some(tempo),
            ..ThemeMotionConfig::default()
        }
    }

    fn character(character: MotionCharacter) -> ThemeMotionConfig {
        ThemeMotionConfig {
            character: Some(character),
            ..ThemeMotionConfig::default()
        }
    }

    fn snappy() -> MotionScaleConfig {
        MotionScaleConfig::new(100.0, 250.0, 0.75)
    }

    fn ms(value: f64) -> String {
        format!("{}ms", js_number_string(js_round(value)))
    }

    fn owned(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(name, value)| (name.to_string(), value.to_string()))
            .collect()
    }

    // The defaults match the ones the play-time engine declares.
    #[test]
    fn defaults_match_the_declared_timing_tokens() {
        let ours: Vec<(&str, &str)> = DURATION_DEFAULTS.into_iter().chain(EASE_DEFAULTS).collect();
        assert_eq!(ours, DECLARED_TIMING_TOKENS);
    }

    // Upstream themeMotion.test.ts: "are today's at the defaults".
    #[test]
    fn tokens_are_todays_at_the_defaults() {
        assert!(expand_theme_motion(&ThemeMotionConfig::default()).is_empty());
        let explicit = expand_theme_motion(&ThemeMotionConfig {
            tempo: Some(1.0),
            character: Some(MotionCharacter::Standard),
            ..ThemeMotionConfig::default()
        });
        assert_eq!(durations_ms(&explicit), TODAY_MS);
        assert_eq!(get(&explicit, "--ease-entrance"), Some(STANDARD));
        assert_eq!(get(&explicit, "--ease-move"), Some(STANDARD));
        let motion = resolve_theme_motion(&ThemeMotionSource::default());
        assert_eq!(durations_ms(&motion.durations), TODAY_MS);
        assert_eq!(
            motion.easings,
            owned(&[
                ("--ease-standard", STANDARD),
                ("--ease-entrance", STANDARD),
                ("--ease-move", STANDARD),
            ])
        );
        assert_eq!(ease_default("--ease-entrance"), STANDARD);
        assert_eq!(ease_default("--ease-move"), STANDARD);
    }

    // Upstream: "scale every duration by tempo %s, to the ms".
    #[test]
    fn tempo_scales_every_duration_to_the_ms() {
        for t in [0.75, 1.2, 1.5] {
            let tokens = expand_theme_motion(&tempo(t));
            let expected: Vec<f64> = TODAY_MS.iter().map(|ms| js_round(ms * t)).collect();
            assert_eq!(durations_ms(&tokens), expected, "tempo {t}");
        }
    }

    // Upstream: "clamp the scale: a tempo of 3 is 1.5".
    #[test]
    fn tempo_above_the_range_scales_by_1_5() {
        let expected: Vec<f64> = TODAY_MS.iter().map(|ms| js_round(ms * 1.5)).collect();
        assert_eq!(durations_ms(&expand_theme_motion(&tempo(3.0))), expected);
    }

    // Upstream: "set the entrance and move curves at %s, and leave the standard
    // and exit curves alone".
    #[test]
    fn character_sets_only_the_entrance_and_move_curves() {
        for c in [MotionCharacter::Crisp, MotionCharacter::Expressive] {
            let tokens = expand_theme_motion(&character(c));
            assert_eq!(get(&tokens, "--ease-entrance"), character_ease(c));
            assert_eq!(get(&tokens, "--ease-move"), character_ease(c));
            assert_eq!(get(&tokens, "--ease-standard"), None);
            assert_eq!(get(&tokens, "--ease-exit"), None);
            assert!(
                !tokens
                    .iter()
                    .any(|(name, _)| name.starts_with("--duration"))
            );
        }
        assert_eq!(
            character_ease(MotionCharacter::Crisp),
            Some("cubic-bezier(0, 0, 0.2, 1)")
        );
        assert_eq!(
            character_ease(MotionCharacter::Expressive),
            Some("cubic-bezier(0.3, 1.2, 0.5, 1)")
        );
        assert_eq!(character_ease(MotionCharacter::Standard), None);
    }

    // Upstream: "keep entrances and moves on a theme's own standard curve at
    // standard".
    #[test]
    fn standard_follows_the_scales_own_easing() {
        let easing = "cubic-bezier(0.2, 0, 0, 1)";
        let tokens = expand_theme_motion(&ThemeMotionConfig::from(MotionScaleConfig {
            easing: Some(easing.to_string()),
            ..MotionScaleConfig::new(175.0, 410.0, 0.75)
        }));
        assert_eq!(get(&tokens, "--ease-standard"), Some(easing));
        assert_eq!(get(&tokens, "--ease-entrance"), Some(easing));
        assert_eq!(get(&tokens, "--ease-move"), Some(easing));
    }

    // Upstream FR5: "a MotionScaleConfig generates exactly what it did" and
    // "implies no tempo for the animator".
    #[test]
    fn a_motion_scale_alone_writes_what_it_did() {
        let config = ThemeMotionConfig::from(snappy());
        assert_eq!(expand_theme_motion(&config), expand_motion_scale(&snappy()));
        assert!(!resolve_motion_knobs(Some(&config)).tempo_is_set);
    }

    // Upstream: "its durations win over tempo; tempo scales the durations it
    // leaves out".
    #[test]
    fn scale_durations_win_over_tempo() {
        let tokens = expand_theme_motion(&ThemeMotionConfig {
            tempo: Some(1.5),
            ..ThemeMotionConfig::from(snappy())
        });
        for (name, value) in expand_motion_scale(&snappy()) {
            assert_eq!(get(&tokens, &name), Some(value.as_str()), "{name}");
        }
        // No slow band in the scale: tempo scales today's.
        assert_eq!(
            get(&tokens, "--duration-slow"),
            Some(ms(975.0 * 1.5).as_str())
        );
        assert_eq!(get(&tokens, "--duration-slow"), Some("1463ms"));
    }

    // Upstream --duration-exit: "defaults to --duration-fast".
    #[test]
    fn duration_exit_defaults_to_duration_fast() {
        assert_eq!(
            get(&owned(&DURATION_DEFAULTS), "--duration-exit"),
            Some("var(--duration-fast)")
        );
        let motion = resolve_theme_motion(&ThemeMotionSource::default());
        assert_eq!(get(&motion.durations, "--duration-exit"), Some("175ms"));
    }

    // Upstream: "follows tempo through --duration-fast, and tempo writes no value
    // for it".
    #[test]
    fn duration_exit_follows_tempo_through_duration_fast() {
        let tokens = expand_theme_motion(&tempo(1.2));
        assert_eq!(get(&tokens, "--duration-exit"), None);
        assert_eq!(get(&tokens, "--_astryx-pre-tempo-duration-exit"), None);
        assert!(!tokens.iter().any(|(_, value)| value.contains("NaN")));
        let config = tempo(1.2);
        let motion = resolve_theme_motion(&ThemeMotionSource {
            motion: Some(&config),
            ..ThemeMotionSource::default()
        });
        assert_eq!(
            get(&motion.durations, "--duration-exit"),
            Some(ms(175.0 * 1.2).as_str())
        );
    }

    // Upstream: "takes a theme override" (its resolveThemeMotion half).
    #[test]
    fn duration_exit_takes_a_theme_override() {
        let tokens = owned(&[("--duration-exit", "90ms")]);
        let motion = resolve_theme_motion(&ThemeMotionSource {
            tokens: &tokens,
            ..ThemeMotionSource::default()
        });
        assert_eq!(get(&motion.durations, "--duration-exit"), Some("90ms"));
    }

    // Upstream FR6: "returns the knobs, durations and easings with explicit
    // tokens applied".
    #[test]
    fn resolve_theme_motion_applies_explicit_tokens() {
        let config = ThemeMotionConfig {
            tempo: Some(1.2),
            character: Some(MotionCharacter::Expressive),
            ..ThemeMotionConfig::default()
        };
        let tokens = owned(&[("--duration-fast", "200ms")]);
        let motion = resolve_theme_motion(&ThemeMotionSource {
            motion: Some(&config),
            tokens: &tokens,
            ..ThemeMotionSource::default()
        });
        assert_eq!(
            motion.knobs,
            ResolvedMotionKnobs {
                tempo: 1.2,
                character: MotionCharacter::Expressive,
                reduced_motion: ThemeReducedMotion::System,
                tempo_is_set: true,
            }
        );
        assert_eq!(get(&motion.durations, "--duration-fast"), Some("200ms"));
        assert_eq!(
            get(&motion.durations, "--duration-medium"),
            Some(ms(410.0 * 1.2).as_str())
        );
        assert_eq!(
            get(&motion.easings, "--ease-entrance"),
            character_ease(MotionCharacter::Expressive)
        );
        assert_eq!(get(&motion.easings, "--ease-standard"), Some(STANDARD));
    }

    // Upstream: "resolves tokens written as references through the theme's
    // tokens (architecture:theme-tokens INV6)".
    #[test]
    fn resolve_theme_motion_follows_references() {
        let tokens = owned(&[
            ("--duration-fast", "90ms"),
            ("--duration-medium", "var(--duration-fast)"),
            ("--duration-slow", "var(--duration-unknown, 700ms)"),
            ("--ease-standard", "linear"),
            ("--ease-entrance", "var(--ease-standard)"),
        ]);
        let motion = resolve_theme_motion(&ThemeMotionSource {
            tokens: &tokens,
            ..ThemeMotionSource::default()
        });
        assert_eq!(get(&motion.durations, "--duration-medium"), Some("90ms"));
        assert_eq!(get(&motion.durations, "--duration-slow"), Some("700ms"));
        assert_eq!(get(&motion.easings, "--ease-entrance"), Some("linear"));
        assert_eq!(get(&motion.easings, "--ease-move"), Some("linear"));
        let values = motion.durations.iter().chain(&motion.easings);
        assert!(!values.into_iter().any(|(_, value)| value.contains("var(")));
    }

    // Upstream: "resolves a reference to the theme's own local token".
    #[test]
    fn resolve_theme_motion_reads_local_tokens() {
        let tokens = owned(&[("--duration-medium", "var(--brand-beat)")]);
        let local_tokens = owned(&[("--brand-beat", "333ms")]);
        let motion = resolve_theme_motion(&ThemeMotionSource {
            tokens: &tokens,
            local_tokens: &local_tokens,
            ..ThemeMotionSource::default()
        });
        assert_eq!(get(&motion.durations, "--duration-medium"), Some("333ms"));
    }

    // Read from the TS: `theme.__axes?.motion ?? theme.motion`.
    #[test]
    fn resolve_theme_motion_prefers_the_motion_axis() {
        let own = tempo(0.8);
        let axes = tempo(1.2);
        let motion = resolve_theme_motion(&ThemeMotionSource {
            motion: Some(&own),
            axes_motion: Some(&axes),
            ..ThemeMotionSource::default()
        });
        assert_eq!(motion.knobs.tempo, 1.2);
        assert_eq!(get(&motion.durations, "--duration-medium"), Some("492ms"));
    }

    // Upstream: "writes the tempo beside the duration tokens for the release
    // fade, and nothing without one".
    #[test]
    fn tempo_writes_the_motion_tempo_property() {
        assert_eq!(
            get(&expand_theme_motion(&tempo(1.5)), MOTION_TEMPO_PROPERTY),
            Some("1.5")
        );
        assert_eq!(
            get(
                &expand_theme_motion(&character(MotionCharacter::Crisp)),
                MOTION_TEMPO_PROPERTY
            ),
            None
        );
    }

    // Upstream: "at tempo %s the pre-tempo durations stay today's while the
    // tokens scale".
    #[test]
    fn pre_tempo_durations_stay_todays() {
        for t in [0.75, 1.5] {
            let tokens = expand_theme_motion(&tempo(t));
            assert_eq!(
                get(&tokens, "--_astryx-pre-tempo-duration-slow-min"),
                Some("730ms")
            );
            assert_eq!(
                get(&tokens, "--_astryx-pre-tempo-duration-medium-max"),
                Some("550ms")
            );
            assert_eq!(
                get(&tokens, "--duration-slow-min"),
                Some(ms(730.0 * t).as_str())
            );
        }
    }

    // Upstream: "a duration scale still times the loops, and an explicit token
    // too" (its expandThemeMotion half).
    #[test]
    fn a_scale_times_the_pre_tempo_durations() {
        let tokens = expand_theme_motion(&ThemeMotionConfig {
            tempo: Some(1.5),
            ..ThemeMotionConfig::from(snappy())
        });
        assert_eq!(
            get(&tokens, "--_astryx-pre-tempo-duration-medium-max"),
            get(&expand_motion_scale(&snappy()), "--duration-medium-max")
        );
    }

    // Upstream: "names every property it writes beside the core tokens privately,
    // with --_astryx- (architecture:theme-tokens INV1)".
    #[test]
    fn extra_properties_are_private() {
        let tokens = expand_theme_motion(&ThemeMotionConfig {
            tempo: Some(1.2),
            character: Some(MotionCharacter::Crisp),
            ..ThemeMotionConfig::from(snappy())
        });
        let core: Vec<&str> = DURATION_DEFAULTS
            .iter()
            .chain(EASE_DEFAULTS.iter())
            .map(|(name, _)| *name)
            .collect();
        let extra: Vec<&str> = tokens
            .iter()
            .map(|(name, _)| name.as_str())
            .filter(|name| !core.contains(name))
            .collect();
        assert!(!extra.is_empty());
        for name in extra {
            assert!(name.starts_with("--_astryx-"), "{name}");
        }
    }

    // Order read from the TS: the tempo's durations, their pre-tempo copies,
    // the tempo, then the curves.
    #[test]
    fn tokens_keep_upstream_order() {
        let tokens = expand_theme_motion(&ThemeMotionConfig {
            tempo: Some(1.2),
            character: Some(MotionCharacter::Crisp),
            ..ThemeMotionConfig::default()
        });
        let names: Vec<&str> = tokens.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(names.len(), 21);
        assert_eq!(names[0], "--duration-fast-min");
        assert_eq!(names[9], "--_astryx-pre-tempo-duration-fast-min");
        assert_eq!(names[18], MOTION_TEMPO_PROPERTY);
        assert_eq!(&names[19..], ["--ease-entrance", "--ease-move"]);
    }

    #[test]
    fn pre_tempo_name_renames_only_duration_tokens() {
        assert_eq!(
            pre_tempo_name("--duration-slow"),
            "--_astryx-pre-tempo-duration-slow"
        );
        assert_eq!(pre_tempo_name("--ease-standard"), "--ease-standard");
    }

    // A config with only part of a scale is knobs alone (upstream's
    // `isScaleConfig` needs fast, medium and ratio).
    #[test]
    fn a_partial_scale_is_not_a_scale() {
        let partial = ThemeMotionConfig {
            fast: Some(100.0),
            tempo: Some(1.2),
            ..ThemeMotionConfig::default()
        };
        assert_eq!(partial.scale(), None);
        assert_eq!(
            get(&expand_theme_motion(&partial), "--duration-fast"),
            Some("210ms")
        );
    }

    // Expected values: what JavaScript's `Number.parseFloat` answers.
    #[test]
    fn parse_float_reads_a_leading_number() {
        assert_eq!(parse_float("175ms"), 175.0);
        assert_eq!(parse_float("  1.5e2ms"), 150.0);
        assert_eq!(parse_float(".5s"), 0.5);
        assert_eq!(parse_float("5.ms"), 5.0);
        assert_eq!(parse_float("1e"), 1.0);
        assert_eq!(parse_float("-Infinity"), f64::NEG_INFINITY);
        assert!(parse_float("ms").is_nan());
        assert!(parse_float(".").is_nan());
    }
}
