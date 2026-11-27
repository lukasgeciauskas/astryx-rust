//! Motion scale expansion: upstream `theme/expandMotionScale.ts`.
//!
//! Three base durations and a ratio expand into the duration tokens: each band's
//! `-min` is its base × ratio and its `-max` its base / ratio, rounded to the
//! nearest 5 ms. A snappy theme lowers the bases, a cinematic one raises them,
//! and the proportions between the variants stay.

use super::js::{js_number_to_string as js_number_string, js_round};

/// A motion scale (upstream `MotionScaleConfig`), such as `{ fast: 175,
/// medium: 410, slow: 975, ratio: 0.75 }` for Astryx's own, `{ fast: 100,
/// medium: 250, ratio: 0.75 }` for a snappy theme or `{ fast: 200, medium: 500,
/// slow: 1200, ratio: 0.7 }` for a cinematic one.
#[derive(Clone, Debug, PartialEq)]
pub struct MotionScaleConfig {
    /// The base duration of micro-interactions in ms (hover, toggle, checkbox).
    pub fast: f64,
    /// The base duration of entrances and exits in ms (dialog, drawer, panel).
    pub medium: f64,
    /// The base duration of slow and continuous animations in ms (spinner,
    /// progress). The slow band is written only when this is set.
    pub slow: Option<f64>,
    /// The ratio of the min and max variants: min = base × ratio, max = base /
    /// ratio. Typically 0.65 to 0.85; upstream's documented default is 0.75.
    pub ratio: f64,
    /// An easing curve for `--ease-standard`. Unset (or empty) leaves it to the
    /// token's default.
    pub easing: Option<String>,
}

impl MotionScaleConfig {
    /// A scale of `fast` and `medium` at `ratio`, with no slow band and no
    /// easing.
    pub fn new(fast: f64, medium: f64, ratio: f64) -> Self {
        Self {
            fast,
            medium,
            slow: None,
            ratio,
            easing: None,
        }
    }
}

/// The tokens [`expand_motion_scale`] writes (upstream `MotionScaleTokens`): CSS
/// custom property names and their CSS values, in upstream's order.
pub type MotionScaleTokens = Vec<(String, String)>;

/// A duration in ms rounded to the nearest 5 ms, for clean token values
/// (upstream `roundMs`).
fn round_ms(ms: f64) -> f64 {
    js_round(ms / 5.0) * 5.0
}

/// `ms` as a duration token's value: `130ms`.
fn ms(value: f64) -> String {
    format!("{}ms", js_number_string(round_ms(value)))
}

/// The duration and easing tokens of `config` (upstream `expandMotionScale`):
///
/// - `--duration-fast-min`, `--duration-fast` and `--duration-fast-max`: fast
///   × ratio, fast, fast / ratio;
/// - the same three for `medium`;
/// - the same three for `slow`, when it is set;
/// - `--ease-standard`, when `easing` is set and not empty.
pub fn expand_motion_scale(config: &MotionScaleConfig) -> MotionScaleTokens {
    let MotionScaleConfig {
        fast,
        medium,
        slow,
        ratio,
        ref easing,
    } = *config;

    let mut tokens: MotionScaleTokens = vec![
        ("--duration-fast-min".to_string(), ms(fast * ratio)),
        ("--duration-fast".to_string(), ms(fast)),
        ("--duration-fast-max".to_string(), ms(fast / ratio)),
        ("--duration-medium-min".to_string(), ms(medium * ratio)),
        ("--duration-medium".to_string(), ms(medium)),
        ("--duration-medium-max".to_string(), ms(medium / ratio)),
    ];

    // The slow band, only when its base is set.
    if let Some(slow) = slow {
        tokens.push(("--duration-slow-min".to_string(), ms(slow * ratio)));
        tokens.push(("--duration-slow".to_string(), ms(slow)));
        tokens.push(("--duration-slow-max".to_string(), ms(slow / ratio)));
    }

    // The easing, only when set (upstream tests it for truthiness, so an empty
    // string writes nothing); its default comes from the ease tokens.
    if let Some(easing) = easing.as_deref().filter(|easing| !easing.is_empty()) {
        tokens.push(("--ease-standard".to_string(), easing.to_string()));
    }

    tokens
}

#[cfg(test)]
mod tests {
    use super::*;

    fn get<'a>(tokens: &'a MotionScaleTokens, name: &str) -> Option<&'a str> {
        tokens
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }

    fn with_slow(fast: f64, medium: f64, slow: f64, ratio: f64) -> MotionScaleConfig {
        MotionScaleConfig {
            slow: Some(slow),
            ..MotionScaleConfig::new(fast, medium, ratio)
        }
    }

    fn duration_count(tokens: &MotionScaleTokens) -> usize {
        tokens
            .iter()
            .filter(|(name, _)| name.starts_with("--duration-"))
            .count()
    }

    // Upstream expandMotionScale.test.ts: "computes default XDS motion scale".
    #[test]
    fn default_scale_rounds_to_five_ms() {
        let tokens = expand_motion_scale(&MotionScaleConfig::new(175.0, 410.0, 0.75));
        assert_eq!(get(&tokens, "--duration-fast-min"), Some("130ms")); // 131.25 → 130
        assert_eq!(get(&tokens, "--duration-fast"), Some("175ms"));
        assert_eq!(get(&tokens, "--duration-fast-max"), Some("235ms")); // 233.3 → 235
        assert_eq!(get(&tokens, "--duration-medium-min"), Some("310ms")); // 307.5 → 310
        assert_eq!(get(&tokens, "--duration-medium"), Some("410ms"));
        assert_eq!(get(&tokens, "--duration-medium-max"), Some("545ms")); // 546.6 → 545
    }

    // Upstream: "computes snappy motion scale".
    #[test]
    fn snappy_scale() {
        let tokens = expand_motion_scale(&MotionScaleConfig::new(100.0, 250.0, 0.75));
        assert_eq!(get(&tokens, "--duration-fast-min"), Some("75ms"));
        assert_eq!(get(&tokens, "--duration-fast"), Some("100ms"));
        assert_eq!(get(&tokens, "--duration-fast-max"), Some("135ms"));
        assert_eq!(get(&tokens, "--duration-medium-min"), Some("190ms"));
        assert_eq!(get(&tokens, "--duration-medium"), Some("250ms"));
        assert_eq!(get(&tokens, "--duration-medium-max"), Some("335ms"));
    }

    // Upstream: "computes cinematic motion scale".
    #[test]
    fn cinematic_scale() {
        let tokens = expand_motion_scale(&MotionScaleConfig::new(200.0, 500.0, 0.7));
        assert_eq!(get(&tokens, "--duration-fast-min"), Some("140ms"));
        assert_eq!(get(&tokens, "--duration-fast"), Some("200ms"));
        assert_eq!(get(&tokens, "--duration-fast-max"), Some("285ms"));
        assert_eq!(get(&tokens, "--duration-medium-min"), Some("350ms"));
        assert_eq!(get(&tokens, "--duration-medium"), Some("500ms"));
        assert_eq!(get(&tokens, "--duration-medium-max"), Some("715ms"));
    }

    // Upstream: "does not include easing token when not specified" and
    // "includes easing override when specified".
    #[test]
    fn easing_is_written_only_when_set() {
        let plain = expand_motion_scale(&MotionScaleConfig::new(175.0, 410.0, 0.75));
        assert_eq!(get(&plain, "--ease-standard"), None);
        let eased = expand_motion_scale(&MotionScaleConfig {
            easing: Some("cubic-bezier(0.0, 0.0, 0.2, 1)".to_string()),
            ..MotionScaleConfig::new(175.0, 410.0, 0.75)
        });
        assert_eq!(
            get(&eased, "--ease-standard"),
            Some("cubic-bezier(0.0, 0.0, 0.2, 1)")
        );
        // Upstream tests `easing` for truthiness: an empty one writes nothing.
        let empty = expand_motion_scale(&MotionScaleConfig {
            easing: Some(String::new()),
            ..MotionScaleConfig::new(175.0, 410.0, 0.75)
        });
        assert_eq!(get(&empty, "--ease-standard"), None);
    }

    // Upstream: "produces exactly 6 duration tokens without slow" and
    // "produces 9 duration tokens with slow".
    #[test]
    fn slow_band_adds_three_durations() {
        let without = expand_motion_scale(&MotionScaleConfig::new(175.0, 410.0, 0.75));
        assert_eq!(duration_count(&without), 6);
        let with = expand_motion_scale(&with_slow(175.0, 410.0, 975.0, 0.75));
        assert_eq!(duration_count(&with), 9);
    }

    // Upstream: "computes default slow band".
    #[test]
    fn default_slow_band() {
        let tokens = expand_motion_scale(&with_slow(175.0, 410.0, 975.0, 0.75));
        assert_eq!(get(&tokens, "--duration-slow-min"), Some("730ms")); // 731.25 → 730
        assert_eq!(get(&tokens, "--duration-slow"), Some("975ms"));
        assert_eq!(get(&tokens, "--duration-slow-max"), Some("1300ms"));
    }

    // Upstream: "does not emit slow tokens when slow is omitted".
    #[test]
    fn no_slow_band_without_slow() {
        let tokens = expand_motion_scale(&MotionScaleConfig::new(175.0, 410.0, 0.75));
        assert_eq!(get(&tokens, "--duration-slow-min"), None);
        assert_eq!(get(&tokens, "--duration-slow"), None);
        assert_eq!(get(&tokens, "--duration-slow-max"), None);
    }

    // Upstream: "maintains ordering: min < base < max".
    #[test]
    fn variants_ascend_within_and_across_bands() {
        let tokens = expand_motion_scale(&with_slow(175.0, 410.0, 975.0, 0.75));
        let ms = |name: &str| -> f64 {
            get(&tokens, name)
                .and_then(|value| value.strip_suffix("ms"))
                .and_then(|value| value.parse().ok())
                .unwrap()
        };
        let order = [
            "--duration-fast-min",
            "--duration-fast",
            "--duration-fast-max",
            "--duration-medium-min",
            "--duration-medium",
            "--duration-medium-max",
            "--duration-slow-min",
            "--duration-slow",
            "--duration-slow-max",
        ];
        for pair in order.windows(2) {
            assert!(ms(pair[0]) < ms(pair[1]), "{} < {}", pair[0], pair[1]);
        }
    }

    // The shipped themes' durations, from `spec/themes/neutral.json` and
    // `spec/themes/y2k.json` (upstream's resolved output for their motion
    // scales), in upstream's token order.
    #[test]
    fn matches_the_shipped_themes() {
        let values = |tokens: MotionScaleTokens| -> Vec<String> {
            tokens.into_iter().map(|(_, value)| value).collect()
        };
        let neutral = expand_motion_scale(&with_slow(125.0, 300.0, 700.0, 0.75));
        assert_eq!(
            values(neutral),
            [
                "95ms", "125ms", "165ms", "225ms", "300ms", "400ms", "525ms", "700ms", "935ms"
            ]
        );
        let y2k = expand_motion_scale(&with_slow(100.0, 250.0, 600.0, 0.8));
        assert_eq!(
            values(y2k),
            [
                "80ms", "100ms", "125ms", "200ms", "250ms", "315ms", "480ms", "600ms", "750ms"
            ]
        );
    }
}
