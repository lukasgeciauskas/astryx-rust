//! Theme-aware data-visualization colors (upstream `getChartColors` and
//! `useChartColors`).
//!
//! [`ChartColors`] is upstream's `ChartColorsAPI`, built from a token resolver
//! ([`ChartColors::from_resolver`]), from resolved tokens, from a theme in one
//! appearance, or from the theme a component draws with ([`use_chart_colors`]).
//!
//! Palettes degrade gracefully for any count: the categorical scale wraps around
//! its slots, and the sequential and diverging scales pick hand-tuned token stops
//! when enough exist and interpolate across the ramp when more are asked for. The
//! theme entry points read the theme's resolved tokens, so data tokens fall back
//! to their defaults even when a theme doesn't override them.
//!
//! Upstream's colors are CSS strings; here they are `Hsla`. A token a resolver
//! cannot answer (upstream's empty string) reads as transparent, and is the one
//! color interpolation cannot parse, as upstream cannot parse a non-hex value.

use gpui::{App, Hsla, Rgba, Window};
use rustryx_theme::{Appearance, ResolvedTheme, Theme, TokenValue, Tokens};

/// A hue of the sequential ramps, upstream `SequentialHue`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SequentialHue {
    /// `'blue'`.
    Blue,
    /// `'shamrock'`.
    Shamrock,
    /// `'orange'`.
    Orange,
    /// `'pink'`.
    Pink,
    /// `'purple'`.
    Purple,
    /// `'red'`.
    Red,
    /// `'teal'`.
    Teal,
    /// `'yellow'`.
    Yellow,
    /// `'gray'`.
    Gray,
}

impl SequentialHue {
    /// Every hue, in upstream's `SEQUENTIAL_HUES` order.
    pub const ALL: [SequentialHue; 9] = [
        SequentialHue::Blue,
        SequentialHue::Shamrock,
        SequentialHue::Orange,
        SequentialHue::Pink,
        SequentialHue::Purple,
        SequentialHue::Red,
        SequentialHue::Teal,
        SequentialHue::Yellow,
        SequentialHue::Gray,
    ];

    /// The hue's name as its tokens spell it: `blue` in `--color-data-blue-1`.
    pub fn name(self) -> &'static str {
        match self {
            SequentialHue::Blue => "blue",
            SequentialHue::Shamrock => "shamrock",
            SequentialHue::Orange => "orange",
            SequentialHue::Pink => "pink",
            SequentialHue::Purple => "purple",
            SequentialHue::Red => "red",
            SequentialHue::Teal => "teal",
            SequentialHue::Yellow => "yellow",
            SequentialHue::Gray => "gray",
        }
    }

    fn index(self) -> usize {
        self as usize
    }
}

/// The categorical palette's slots, upstream `CATEGORICAL_TOKENS`.
const CATEGORICAL_TOKENS: [&str; 10] = [
    "--color-data-categorical-blue",
    "--color-data-categorical-orange",
    "--color-data-categorical-purple",
    "--color-data-categorical-green",
    "--color-data-categorical-pink",
    "--color-data-categorical-cyan",
    "--color-data-categorical-red",
    "--color-data-categorical-teal",
    "--color-data-categorical-brown",
    "--color-data-categorical-indigo",
];

/// The color an unanswered token paints: nothing, as upstream's empty string.
const UNRESOLVED: Hsla = gpui::transparent_black();

/// A resolved token value read as a color: an `Hsla` as it is, a color
/// [`TokenValue`] as its color, and any other kind of value as no color.
pub trait TokenColor {
    /// The color, or `None` when the value is not one.
    fn token_color(self) -> Option<Hsla>;
}

impl TokenColor for Hsla {
    fn token_color(self) -> Option<Hsla> {
        Some(self)
    }
}

impl TokenColor for Rgba {
    fn token_color(self) -> Option<Hsla> {
        Some(self.into())
    }
}

impl TokenColor for TokenValue {
    fn token_color(self) -> Option<Hsla> {
        match self {
            TokenValue::Color(color) => Some(color),
            _ => None,
        }
    }
}

impl TokenColor for &TokenValue {
    fn token_color(self) -> Option<Hsla> {
        self.clone().token_color()
    }
}

/// The status roles, upstream `ChartColorsAPI.semantic`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SemanticColors {
    /// `--color-data-categorical-green`.
    pub positive: Hsla,
    /// `--color-data-categorical-red`.
    pub negative: Hsla,
    /// `--color-data-categorical-orange`.
    pub warning: Hsla,
    /// `--color-data-neutral`.
    pub neutral: Hsla,
}

/// Colors for chart chrome (axes, grid, ticks, labels), upstream
/// `ChartColorsAPI.structural`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StructuralColors {
    /// `--color-border-emphasized`.
    pub axis: Hsla,
    /// `--color-border`.
    pub grid: Hsla,
    /// `--color-border-emphasized`.
    pub tick: Hsla,
    /// `--color-text-secondary`.
    pub label: Hsla,
}

/// A theme's data-visualization palettes, upstream `ChartColorsAPI`: what
/// `getChartColors`, `getChartColorsFromResolver` and `useChartColors` return.
///
/// The tokens are read once, when it is built; two built from the same tokens
/// are equal, as upstream's hook returns one palette per theme and mode.
#[derive(Clone, Debug, PartialEq)]
pub struct ChartColors {
    categorical: [Option<Hsla>; 10],
    /// Each hue's ramp from step 5 to step 1, in [`SequentialHue::ALL`] order.
    ramps: [[Option<Hsla>; 5]; 9],
    gray_1: Option<Hsla>,
    /// The status roles, upstream `semantic`.
    pub semantic: SemanticColors,
    /// The chrome roles, upstream `structural`.
    pub structural: StructuralColors,
}

impl ChartColors {
    /// Colors from a token resolver, upstream `getChartColorsFromResolver`: the
    /// lowest-level entry point. `resolve` maps a token's CSS name
    /// (`--color-data-blue-1`) to its value; `None` is a token it cannot answer.
    pub fn from_resolver<V: TokenColor>(mut resolve: impl FnMut(&str) -> Option<V>) -> Self {
        let mut read = |name: &str| resolve(name).and_then(TokenColor::token_color);
        let categorical = CATEGORICAL_TOKENS.map(&mut read);
        let ramps = SequentialHue::ALL.map(|hue| {
            [5, 4, 3, 2, 1].map(|step| read(&format!("--color-data-{}-{step}", hue.name())))
        });
        let gray_1 = read("--color-data-gray-1");
        let mut paint = |name: &str| read(name).unwrap_or(UNRESOLVED);
        let semantic = SemanticColors {
            positive: paint("--color-data-categorical-green"),
            negative: paint("--color-data-categorical-red"),
            warning: paint("--color-data-categorical-orange"),
            neutral: paint("--color-data-neutral"),
        };
        let structural = StructuralColors {
            axis: paint("--color-border-emphasized"),
            grid: paint("--color-border"),
            tick: paint("--color-border-emphasized"),
            label: paint("--color-text-secondary"),
        };
        Self {
            categorical,
            ramps,
            gray_1,
            semantic,
            structural,
        }
    }

    /// Colors from one theme's resolved tokens, read by their CSS names
    /// ([`Tokens::get`]).
    pub fn from_tokens(tokens: &Tokens) -> Self {
        Self::from_resolver(|name| tokens.get(name))
    }

    /// Colors from a theme in one appearance, upstream `getChartColors(theme, mode)`:
    /// for drawing outside a window (canvas setup, tests, scripts). A data token
    /// the theme does not set reads as its default.
    pub fn from_theme(theme: &Theme, appearance: Appearance) -> Self {
        Self::from_tokens(theme.tokens(appearance))
    }

    /// Colors from a theme resolved for drawing, with its adaptations and
    /// scoped overrides applied.
    pub fn from_resolved(theme: &ResolvedTheme) -> Self {
        Self::from_tokens(theme.tokens())
    }

    /// `n` series colors, upstream `categorical(n)`: the ten palette slots in order,
    /// wrapping around past the tenth so any series count gets a deterministic
    /// color.
    pub fn categorical(&self, n: usize) -> Vec<Hsla> {
        (0..n)
            .map(|i| paint(self.categorical[i % self.categorical.len()]))
            .collect()
    }

    /// `n` colors along one hue's ramp from its darkest step (5) to its lightest
    /// (1), upstream `sequential[hue](n)`.
    pub fn sequential(&self, hue: SequentialHue, n: usize) -> Vec<Hsla> {
        sample_ramp(&self.ramps[hue.index()], n)
            .into_iter()
            .map(paint)
            .collect()
    }

    /// The two-sided scales, upstream `diverging`.
    pub fn diverging(&self) -> Diverging<'_> {
        Diverging(self)
    }

    /// `color` at `opacity`, upstream `alpha(hex, opacity)`. The opacity always wins over
    /// any alpha the color has; it is clamped to 0 to 1, and a non-finite one is 1.
    /// The channels round to 8 bits and the alpha to four decimals, as upstream
    /// writes `rgba(r, g, b, a)` (or `#RRGGBB` when opaque).
    pub fn alpha(&self, color: Hsla, opacity: f64) -> Hsla {
        hex_alpha(color, opacity)
    }

    fn ramp(&self, hue: SequentialHue) -> &[Option<Hsla>; 5] {
        &self.ramps[hue.index()]
    }

    /// Upstream `buildDiverging`.
    fn build_diverging(
        &self,
        neg: SequentialHue,
        pos: SequentialHue,
        n: usize,
        midpoint: Option<Hsla>,
    ) -> Vec<Hsla> {
        let midpoint = midpoint.or(self.gray_1);
        if n == 0 {
            return Vec::new();
        }
        if n == 1 {
            return vec![paint(midpoint)];
        }
        let half = n / 2;
        let has_center = n % 2 == 1;
        let neg_side = sample_ramp(self.ramp(neg), half);
        let mut pos_side = sample_ramp(self.ramp(pos), half);
        pos_side.reverse();
        let mut out: Vec<Hsla> = neg_side.into_iter().map(paint).collect();
        if has_center {
            out.push(paint(midpoint));
        }
        out.extend(pos_side.into_iter().map(paint));
        out
    }
}

/// The two-sided scales of a [`ChartColors`], upstream `ChartColorsAPI.diverging`.
///
/// Each side takes `n / 2` colors, sampled like a sequential ramp: the first hue
/// runs from step 5 to step 1, the second from step 1 to step 5, and an odd count
/// puts the midpoint (gray-1 unless given) in the center.
#[derive(Clone, Copy, Debug)]
pub struct Diverging<'a>(&'a ChartColors);

impl Diverging<'_> {
    /// Shamrock through gray-1 to red, upstream `positiveNegative(n)`.
    pub fn positive_negative(&self, n: usize) -> Vec<Hsla> {
        self.0
            .build_diverging(SequentialHue::Shamrock, SequentialHue::Red, n, None)
    }

    /// Blue through gray-1 to red, upstream `coldHot(n)`.
    pub fn cold_hot(&self, n: usize) -> Vec<Hsla> {
        self.0
            .build_diverging(SequentialHue::Blue, SequentialHue::Red, n, None)
    }

    /// `neg` through the midpoint to `pos`, upstream `custom(neg, pos, n, midpoint)`:
    /// a `midpoint` replaces gray-1 in the center.
    pub fn custom(
        &self,
        neg: SequentialHue,
        pos: SequentialHue,
        n: usize,
        midpoint: Option<Hsla>,
    ) -> Vec<Hsla> {
        self.0.build_diverging(neg, pos, n, midpoint)
    }
}

/// The chart colors of the theme a component draws with, upstream
/// `useChartColors()`: the nearest theme scope, else the app's theme in its mode
/// for this window ([`rustryx_theme::current`]). Read it during render.
pub fn use_chart_colors(window: &Window, cx: &App) -> ChartColors {
    ChartColors::from_resolved(&rustryx_theme::current(window, cx))
}

fn paint(color: Option<Hsla>) -> Hsla {
    color.unwrap_or(UNRESOLVED)
}

/// `Math.round`: the nearest integer, halves toward +∞.
fn js_round(x: f64) -> f64 {
    let floor = x.floor();
    if x - floor >= 0.5 { floor + 1. } else { floor }
}

/// A color's sRGB channels on 0 to 255.
fn channels(color: Hsla) -> [f64; 3] {
    let rgb = color.to_rgb();
    [rgb.r, rgb.g, rgb.b].map(|c| f64::from(c) * 255.)
}

/// A color's 8-bit sRGB channels, as upstream's `parseHex` reads a hex token.
fn hex_channels(color: Hsla) -> [f64; 3] {
    channels(color).map(|c| js_round(c).clamp(0., 255.))
}

/// An opaque color from sRGB channels on 0 to 255, each rounded to 8 bits:
/// upstream's `formatHex`.
fn format_hex(r: f64, g: f64, b: f64) -> Hsla {
    format_rgba(r, g, b, 1.)
}

fn format_rgba(r: f64, g: f64, b: f64, a: f64) -> Hsla {
    let channel = |c: f64| (js_round(c).clamp(0., 255.) / 255.) as f32;
    Rgba {
        r: channel(r),
        g: channel(g),
        b: channel(b),
        a: a as f32,
    }
    .into()
}

/// Interpolates two colors in sRGB, upstream `lerpHex`. A color that cannot be
/// parsed (an unanswered token) falls back to the nearer endpoint.
fn lerp_hex(a: Option<Hsla>, b: Option<Hsla>, t: f64) -> Option<Hsla> {
    let (Some(ca), Some(cb)) = (a, b) else {
        return if t < 0.5 { a } else { b };
    };
    let [ar, ag, ab] = hex_channels(ca);
    let [br, bg, bb] = hex_channels(cb);
    let mix = |x: f64, y: f64| x + (y - x) * t;
    Some(format_hex(mix(ar, br), mix(ag, bg), mix(ab, bb)))
}

/// Samples `n` colors from an ordered ramp of stops, upstream `sampleRamp`:
/// - `n <= stops.len()` picks the nearest exact stops, preserving the hand-tuned
///   design-token values;
/// - `n > stops.len()` interpolates across the ramp, so any count is supported
///   instead of silently capping at the number of stops;
/// - `n == 1` is the middle stop, a single representative tone.
fn sample_ramp(stops: &[Option<Hsla>], n: usize) -> Vec<Option<Hsla>> {
    let len = stops.len();
    if n == 0 || len == 0 {
        return Vec::new();
    }
    if n == 1 {
        return vec![stops[(len - 1) / 2]];
    }
    if len == 1 {
        return vec![stops[0]; n];
    }
    let position = |i: usize| (i * (len - 1)) as f64 / (n - 1) as f64;
    if n <= len {
        return (0..n)
            .map(|i| stops[js_round(position(i)) as usize])
            .collect();
    }
    (0..n)
        .map(|i| {
            let t = position(i);
            let lo = t.floor();
            let frac = t - lo;
            let lo = lo as usize;
            if frac == 0. {
                stops[lo]
            } else {
                lerp_hex(stops[lo], stops[lo + 1], frac)
            }
        })
        .collect()
}

/// Upstream `hexAlpha`: the color's channels at `opacity`, written as upstream's
/// `formatColor` writes them.
fn hex_alpha(color: Hsla, opacity: f64) -> Hsla {
    let a = if opacity.is_finite() {
        opacity.clamp(0., 1.)
    } else {
        1.
    };
    let [r, g, b] = channels(color);
    if a >= 1. {
        format_hex(r, g, b)
    } else {
        format_rgba(r, g, b, to_fixed_4(a))
    }
}

/// `parseFloat(a.toFixed(4))` for `a` in 0 to 1: the nearest multiple of 0.0001,
/// the larger on an exact tie, as `Number.prototype.toFixed` picks.
fn to_fixed_4(a: f64) -> f64 {
    // A double's decimal expansion is finite; 1100 places hold every one in 0 to 1.
    let exact = format!("{a:.1100}");
    let (int, frac) = exact.split_once('.').unwrap_or((&exact, ""));
    let mut digits: Vec<u8> = int
        .bytes()
        .chain(frac.bytes().take(4))
        .map(|d| d - b'0')
        .collect();
    let rest = &frac.as_bytes()[4.min(frac.len())..];
    let round_up = rest.first().is_some_and(|&d| d >= b'5');
    if round_up {
        let mut i = digits.len();
        loop {
            if i == 0 {
                digits.insert(0, 1);
                break;
            }
            i -= 1;
            if digits[i] == 9 {
                digits[i] = 0;
            } else {
                digits[i] += 1;
                break;
            }
        }
    }
    let scaled = digits.iter().fold(0u64, |n, &d| n * 10 + u64::from(d));
    scaled as f64 / 10_000.
}

#[cfg(test)]
mod tests;
