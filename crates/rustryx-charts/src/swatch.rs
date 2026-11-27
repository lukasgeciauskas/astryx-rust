//! `ChartSwatch`: a series' color chip for legends, tooltips and custom chrome
//! (upstream `ChartSwatch`).

use gpui::prelude::*;
use gpui::{App, Hsla, SharedString, Window, div, px};

/// The swatch's shape (upstream `ChartSwatchVariant`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ChartSwatchVariant {
    /// A square chip, for bar series.
    #[default]
    Square,
    /// A short stroke, for every other mark.
    Line,
}

/// A color swatch (upstream `ChartSwatch`), hidden from assistive technology.
#[derive(IntoElement, Clone, Copy, Debug)]
pub struct ChartSwatch {
    color: Hsla,
    variant: ChartSwatchVariant,
}

impl ChartSwatch {
    /// A swatch of `color` (upstream `color`), a square by default.
    pub fn new(color: Hsla) -> Self {
        ChartSwatch {
            color,
            variant: ChartSwatchVariant::Square,
        }
    }

    /// Upstream `variant`.
    pub fn variant(mut self, variant: ChartSwatchVariant) -> Self {
        self.variant = variant;
        self
    }
}

/// The swatch of a mark type (upstream `swatchVariantForType`): a square for bars, a
/// stroke for everything else, so legends, tooltips and custom chrome agree.
pub fn swatch_variant_for_type(kind: Option<&str>) -> ChartSwatchVariant {
    if kind == Some("bar") {
        ChartSwatchVariant::Square
    } else {
        ChartSwatchVariant::Line
    }
}

/// [`swatch_variant_for_type`] for a type name held as a shared string.
pub fn swatch_variant_for(kind: Option<&SharedString>) -> ChartSwatchVariant {
    swatch_variant_for_type(kind.map(|k| k.as_ref()))
}

impl RenderOnce for ChartSwatch {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let chip = div()
            .debug_selector(|| "chart-swatch".into())
            .flex_none()
            .bg(self.color);
        match self.variant {
            ChartSwatchVariant::Square => chip.w(px(8.)).h(px(8.)).rounded(px(2.)).mx(px(1.)),
            ChartSwatchVariant::Line => chip.w(px(10.)).h(px(3.)).rounded(px(1.5)),
        }
    }
}
