//! The theme's timing of a motion, upstream `motion/themeTiming.ts`: a
//! duration and an easing (a token's value, or an element's transition) as
//! the eased progress from 0 to 1 over the duration, one [`super::MotionCurve`].
//!
//! Upstream reads computed styles; here the caller hands in the token values
//! where the element is (a lookup from a token name such as
//! `--duration-fast` to its text, `None` where nothing sets one), and a
//! token with no value reads the default core declares, its references
//! (`var(--duration-fast)`) read through the same lookup.

use std::collections::HashMap;

use super::FnCurve;
use super::bezier::cubic_bezier;
use super::role_value::{MotionCurvePoints, MotionRoleKind, RoleValue};
use super::roles::{parse_css_easing, parse_motion_role_value};

/// The value core declares for each timing token, as the token file
/// declares it (upstream `declaredTimingTokens`).
pub const DECLARED_TIMING_TOKENS: [(&str, &str); 14] = [
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
    ("--ease-standard", "cubic-bezier(0.24, 1, 0.4, 1)"),
    ("--ease-entrance", "cubic-bezier(0.24, 1, 0.4, 1)"),
    ("--ease-move", "cubic-bezier(0.24, 1, 0.4, 1)"),
    ("--ease-exit", "cubic-bezier(0.3, 0, 0.6, 0.6)"),
];

/// The token values set where an element is: a token name to its text,
/// `None` where nothing sets one.
pub type TokenLookup<'a> = &'a dyn Fn(&str) -> Option<String>;

/// The theme tokens that time one motion (upstream `ThemeTimingTokens`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ThemeTimingTokens {
    /// A duration token, such as `--duration-fast`.
    pub duration: &'static str,
    /// An easing token, such as `--ease-standard`.
    pub easing: &'static str,
}

/// A duration and an easing, each `None` when unresolved (upstream
/// `ResolvedTiming`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ResolvedTiming {
    /// The duration in ms, zero or more.
    pub ms: Option<f64>,
    pub points: Option<MotionCurvePoints>,
}

/// The eased progress from 0 to 1 over `seconds` (upstream
/// `easedProgress`), its velocity in progress per second. A curve of no
/// length is at its end. With `jumps`, a jump in the easing (`steps()`)
/// carries no speed: the gentler of the slopes either side counts.
pub fn eased_progress(seconds: f64, ease: impl Fn(f64) -> f64 + 'static, jumps: bool) -> FnCurve {
    let ease = std::rc::Rc::new(ease);
    let at = {
        let ease = ease.clone();
        move |t: f64| {
            if seconds <= 0. || t >= seconds {
                1.
            } else {
                ease(t.max(0.) / seconds)
            }
        }
    };
    let value = at.clone();
    let step = 1e-3f64.min(seconds / 100.);
    FnCurve::new(seconds, at, move |t| {
        if seconds <= 0. || t >= seconds {
            return 0.;
        }
        let before = (t - step).max(0.);
        let after = (t + step).min(seconds);
        if !jumps {
            return (value(after) - value(before)) / (after - before);
        }
        let here = value(t);
        let mut slopes = Vec::new();
        if t > before {
            slopes.push((here - value(before)) / (t - before));
        }
        if after > t {
            slopes.push((value(after) - here) / (after - t));
        }
        slopes
            .into_iter()
            .reduce(|a, b| if b.abs() < a.abs() { b } else { a })
            .unwrap_or(0.)
    })
}

/// A duration and an easing written in CSS (upstream `resolveTiming`, for
/// the plain values it parses without a probe): the duration is `None` when
/// it is not a time of zero or more, the easing when it is no cubic Bézier.
pub fn resolve_timing(duration: Option<&str>, easing: Option<&str>) -> ResolvedTiming {
    ResolvedTiming {
        ms: duration.and_then(|text| {
            match parse_motion_role_value(MotionRoleKind::Duration, text) {
                Some(RoleValue::DurationMs(ms)) => Some(ms),
                _ => None,
            }
        }),
        points: easing.and_then(parse_css_easing),
    }
}

/// Replaces `var(--name)` and `var(--name, fallback)` in `text` with the
/// values `lookup` gives (upstream `resolveTokenReferences`).
fn resolve_references(text: &str, lookup: &dyn Fn(&str) -> Option<String>, depth: usize) -> String {
    let Some(start) = text.find("var(") else {
        return text.to_string();
    };
    if depth > 16 {
        return text.to_string();
    }
    let open = start + 3;
    let mut level = 0;
    let mut close = None;
    for (i, c) in text[open..].char_indices() {
        match c {
            '(' => level += 1,
            ')' => {
                level -= 1;
                if level == 0 {
                    close = Some(open + i);
                    break;
                }
            }
            _ => {}
        }
    }
    let Some(close) = close else {
        return text.to_string();
    };
    let inner = &text[open + 1..close];
    let (name, fallback) = match inner.split_once(',') {
        Some((name, fallback)) => (name.trim(), Some(fallback.trim())),
        None => (inner.trim(), None),
    };
    let value = lookup(name)
        .or_else(|| fallback.map(str::to_string))
        .unwrap_or_default();
    let value = resolve_references(&value, lookup, depth + 1);
    let rest = resolve_references(&text[close + 1..], lookup, depth + 1);
    format!("{}{value}{rest}", &text[..start])
}

/// The text of `token` where the element is: its own value, or the default
/// core declares with that default's references read there too (upstream
/// `readThemeTokenText`).
pub fn read_theme_token_text(set: &dyn Fn(&str) -> Option<String>, token: &str) -> String {
    let declared: HashMap<&str, &str> = DECLARED_TIMING_TOKENS.into_iter().collect();
    let read = |name: &str| -> Option<String> {
        set(name)
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .or_else(|| declared.get(name).map(|v| v.to_string()))
    };
    match read(token) {
        Some(value) => resolve_references(&value, &read, 0),
        None => String::new(),
    }
}

/// The duration and easing of theme tokens where the element is (upstream
/// `readThemeTokens`); a token left out gives `None`.
pub fn read_theme_tokens(
    set: &dyn Fn(&str) -> Option<String>,
    duration: Option<&str>,
    easing: Option<&str>,
) -> ResolvedTiming {
    let duration = duration.map(|token| read_theme_token_text(set, token));
    let easing = easing.map(|token| read_theme_token_text(set, token));
    resolve_timing(duration.as_deref(), easing.as_deref())
}

/// A CSS easing that is not a cubic Bézier, played as a browser plays it:
/// `steps(n[, position])` and `linear(…)` (upstream asks the browser).
pub fn css_easing(text: &str) -> Option<Box<dyn Fn(f64) -> f64>> {
    let text = text.trim();
    if let Some(points) = parse_css_easing(text) {
        let ease = cubic_bezier(points);
        return Some(Box::new(move |x| ease.ease(x)));
    }
    let args = |name: &str| {
        text.strip_prefix(name)
            .and_then(|rest| rest.trim_start().strip_prefix('('))
            .and_then(|rest| rest.strip_suffix(')'))
            .map(|inner| inner.split(',').map(str::trim).collect::<Vec<_>>())
    };
    let step_keyword = |keyword: &str| -> Option<(f64, f64)> {
        // (jumps before the first step, jumps counted)
        match keyword {
            "jump-start" | "start" => Some((1., 0.)),
            "jump-end" | "end" => Some((0., 0.)),
            "jump-none" => Some((0., -1.)),
            "jump-both" => Some((1., 1.)),
            _ => None,
        }
    };
    match text {
        "step-start" => return css_easing("steps(1, jump-start)"),
        "step-end" => return css_easing("steps(1, jump-end)"),
        _ => {}
    }
    if let Some(parts) = args("steps") {
        let n: f64 = parts.first()?.parse().ok()?;
        let (start_jump, extra) = step_keyword(parts.get(1).copied().unwrap_or("end"))?;
        if n.is_nan() || n < 1. || (extra < 0. && n < 2.) {
            return None;
        }
        let n = n.floor();
        let jumps = n + extra;
        return Some(Box::new(move |x: f64| {
            let x = x.clamp(0., 1.);
            let mut step = (x * n).floor() + start_jump;
            if x >= 1. {
                step = n + start_jump;
            }
            (step / jumps).clamp(0., 1.)
        }));
    }
    if let Some(parts) = args("linear") {
        let mut stops: Vec<(f64, Option<f64>)> = Vec::new();
        for part in parts {
            let mut words = part.split_whitespace();
            let value: f64 = words.next()?.parse().ok()?;
            let at = match words.next() {
                Some(word) => Some(word.strip_suffix('%')?.parse::<f64>().ok()? / 100.),
                None => None,
            };
            stops.push((value, at));
        }
        if stops.len() < 2 {
            return None;
        }
        let count = stops.len();
        let mut positions: Vec<Option<f64>> = stops.iter().map(|(_, at)| *at).collect();
        positions[0] = Some(positions[0].unwrap_or(0.));
        positions[count - 1] = Some(positions[count - 1].unwrap_or(1.));
        let mut i = 0;
        while i < count {
            if positions[i].is_none() {
                let before = i - 1;
                let mut after = i;
                while positions[after].is_none() {
                    after += 1;
                }
                let (a, b) = (
                    positions[before].unwrap_or(0.),
                    positions[after].unwrap_or(1.),
                );
                for (k, slot) in positions.iter_mut().enumerate().take(after).skip(i) {
                    *slot = Some(a + (b - a) * (k - before) as f64 / (after - before) as f64);
                }
                i = after;
            }
            i += 1;
        }
        let points: Vec<(f64, f64)> = stops
            .iter()
            .zip(positions)
            .map(|((value, _), at)| (at.unwrap_or(0.), *value))
            .collect();
        return Some(Box::new(move |x: f64| {
            if x <= points[0].0 {
                return points[0].1;
            }
            for pair in points.windows(2) {
                let ((x0, y0), (x1, y1)) = (pair[0], pair[1]);
                if x <= x1 {
                    return if x1 == x0 {
                        y1
                    } else {
                        y0 + (y1 - y0) * (x - x0) / (x1 - x0)
                    };
                }
            }
            points[points.len() - 1].1
        }));
    }
    None
}

fn progress_of(timing: ResolvedTiming, easing: &str) -> Option<FnCurve> {
    let ms = timing.ms.filter(|ms| *ms > 0.)?;
    if let Some(points) = timing.points {
        let ease = cubic_bezier(points);
        return Some(eased_progress(ms / 1000., move |x| ease.ease(x), false));
    }
    let ease = css_easing(easing)?;
    Some(eased_progress(ms / 1000., ease, true))
}

/// The eased progress that times a motion (upstream `readThemeTiming`):
/// with `tokens`, read from the theme's tokens where the element is; `None`
/// when the duration is not a positive length or the easing cannot play.
pub fn read_theme_timing(
    set: &dyn Fn(&str) -> Option<String>,
    tokens: ThemeTimingTokens,
) -> Option<FnCurve> {
    let easing = read_theme_token_text(set, tokens.easing);
    let duration = read_theme_token_text(set, tokens.duration);
    progress_of(resolve_timing(Some(&duration), Some(&easing)), &easing)
}

/// The eased progress of an element's own transition, its first
/// `transition-duration` and `transition-timing-function` (upstream
/// `readThemeTiming` with no tokens).
pub fn read_transition_timing(duration: &str, easing: &str) -> Option<FnCurve> {
    let first_duration = duration.split(',').next().unwrap_or("");
    let mut depth = 0;
    let mut cut = easing.len();
    for (i, c) in easing.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => {
                cut = i;
                break;
            }
            _ => {}
        }
    }
    let first_easing = easing[..cut].trim();
    progress_of(
        ResolvedTiming {
            ms: resolve_timing(Some(first_duration), None).ms,
            points: parse_css_easing(first_easing),
        },
        first_easing,
    )
}

/// The theme's timing, or a curve of no length that lands at once when it
/// does not resolve or there is no element (upstream
/// `readThemeTimingOrInstant`).
pub fn read_theme_timing_or_instant(
    set: Option<TokenLookup<'_>>,
    tokens: ThemeTimingTokens,
) -> FnCurve {
    set.and_then(|set| read_theme_timing(set, tokens))
        .unwrap_or_else(|| eased_progress(0., |x| x, false))
}
