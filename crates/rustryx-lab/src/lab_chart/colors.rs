//! The lab's chart palettes (upstream lab `getChartColors.ts` and `useChartColors.ts`), read
//! from the theme's data tokens through rustryx-charts' token reading.
//!
//! The lab's palettes are the charts package's older form: `categorical(n)` gives at most the
//! ten slots (no wrapping), and the sequential and diverging scales pick from the five
//! ramp stops (no interpolation past them).

use gpui::{App, Hsla, Window};
use rustryx_charts::colors::{ChartColors, SemanticColors, StructuralColors, TokenColor};
use rustryx_theme::{Appearance, Theme};

pub use rustryx_charts::colors::SequentialHue;

/// The lab's chart colors (upstream lab `ChartColorsAPI`).
#[derive(Clone, Debug, PartialEq)]
pub struct ChartColorsApi {
    colors: ChartColors,
    categorical: Vec<Hsla>,
    /// Each hue's five stops, from step 5 to step 1, in [`SequentialHue::ALL`] order.
    ramps: Vec<Vec<Hsla>>,
    gray_1: Hsla,
    /// Upstream `semantic`: positive, negative, warning and neutral.
    pub semantic: SemanticColors,
    /// Upstream `structural`: the chrome's axis, grid, tick and label colors.
    pub structural: StructuralColors,
}

/// JavaScript's `Math.round`.
fn js_round(x: f64) -> f64 {
    rustryx_theme::authoring::js::js_round(x)
}

/// Upstream `pickFromRamp`: `n` of the five stops (the middle one for 1, all five for 5 or more).
fn pick_from_ramp(stops: &[Hsla], n: usize) -> Vec<Hsla> {
    match n {
        0 => Vec::new(),
        n if n >= 5 => stops.to_vec(),
        1 => vec![stops[2]],
        n => (0..n)
            .map(|i| stops[js_round((i * 4) as f64 / (n - 1) as f64) as usize])
            .collect(),
    }
}

impl ChartColorsApi {
    fn from_colors(colors: ChartColors) -> Self {
        let ramps: Vec<Vec<Hsla>> = SequentialHue::ALL
            .iter()
            .map(|h| colors.sequential(*h, 5))
            .collect();
        ChartColorsApi {
            categorical: colors.categorical(10),
            gray_1: ramps[SequentialHue::Gray as usize][4],
            ramps,
            semantic: colors.semantic,
            structural: colors.structural,
            colors,
        }
    }

    /// Upstream `getChartColorsFromResolver`: colors from a resolver of token names
    /// (`--color-data-blue-1`); a token it cannot answer is transparent.
    pub fn from_resolver<V: TokenColor>(resolve: impl FnMut(&str) -> Option<V>) -> Self {
        Self::from_colors(ChartColors::from_resolver(resolve))
    }

    /// `n` series colors (upstream `categorical(n)`): the first `n` of the ten slots.
    pub fn categorical(&self, n: usize) -> Vec<Hsla> {
        self.categorical.iter().take(n).copied().collect()
    }

    /// `n` colors of one hue's ramp, darkest first (upstream `sequential[hue](n)`).
    pub fn sequential(&self, hue: SequentialHue, n: usize) -> Vec<Hsla> {
        pick_from_ramp(&self.ramps[hue as usize], n)
    }

    /// The two-sided scales (upstream `diverging`).
    pub fn diverging(&self) -> Diverging<'_> {
        Diverging(self)
    }

    /// `color` at `opacity` (upstream `alpha(hex, opacity)`): the opacity wins over the color's own
    /// alpha, clamped to 0 to 1 (1 when it is not finite).
    pub fn alpha(&self, color: Hsla, opacity: f64) -> Hsla {
        self.colors.alpha(color, opacity)
    }

    /// Upstream `buildDiverging`: `n / 2` stops of the negative ramp, the midpoint for an odd `n`,
    /// and the positive ramp's stops reversed.
    fn build(
        &self,
        neg: SequentialHue,
        pos: SequentialHue,
        n: usize,
        midpoint: Option<Hsla>,
    ) -> Vec<Hsla> {
        let midpoint = midpoint.unwrap_or(self.gray_1);
        if n == 0 {
            return Vec::new();
        }
        if n == 1 {
            return vec![midpoint];
        }
        let half = n / 2;
        let mut out = pick_from_ramp(&self.ramps[neg as usize], half);
        if n % 2 == 1 {
            out.push(midpoint);
        }
        let mut pos_side = pick_from_ramp(&self.ramps[pos as usize], half);
        pos_side.reverse();
        out.extend(pos_side);
        out
    }
}

/// The diverging scales (upstream `ChartColorsAPI.diverging`).
pub struct Diverging<'a>(&'a ChartColorsApi);

impl Diverging<'_> {
    /// Upstream `positiveNegative(n)`: shamrock to red.
    pub fn positive_negative(&self, n: usize) -> Vec<Hsla> {
        self.0
            .build(SequentialHue::Shamrock, SequentialHue::Red, n, None)
    }

    /// Upstream `coldHot(n)`: blue to red.
    pub fn cold_hot(&self, n: usize) -> Vec<Hsla> {
        self.0
            .build(SequentialHue::Blue, SequentialHue::Red, n, None)
    }

    /// Upstream `custom(neg, pos, n, midpoint)`: the midpoint is gray step 1 by default.
    pub fn custom(
        &self,
        neg: SequentialHue,
        pos: SequentialHue,
        n: usize,
        midpoint: Option<Hsla>,
    ) -> Vec<Hsla> {
        self.0.build(neg, pos, n, midpoint)
    }
}

/// Upstream lab `getChartColorsFromResolver`.
pub fn get_chart_colors_from_resolver<V: TokenColor>(
    resolve: impl FnMut(&str) -> Option<V>,
) -> ChartColorsApi {
    ChartColorsApi::from_resolver(resolve)
}

/// Upstream lab `getChartColors(theme, mode)`: a theme's colors in one appearance, for drawing
/// outside a window. Upstream reads only the tokens the theme itself sets; here a data token
/// it does not set reads as its default.
pub fn get_chart_colors(theme: &Theme, appearance: Appearance) -> ChartColorsApi {
    ChartColorsApi::from_colors(ChartColors::from_theme(theme, appearance))
}

/// Upstream lab `useChartColors`: the colors of the theme drawing the window.
pub fn use_chart_colors(window: &Window, cx: &App) -> ChartColorsApi {
    ChartColorsApi::from_colors(rustryx_charts::colors::use_chart_colors(window, cx))
}
