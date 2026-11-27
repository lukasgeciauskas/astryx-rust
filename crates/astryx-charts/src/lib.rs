//! Astryx charts for GPUI (upstream `@astryxdesign/charts`): the scales and shapes the
//! marks need (the d3 subset upstream uses), `Chart` and its axis, grid, legend, swatch
//! and tooltip, and the marks.

mod axis;
mod chart;
pub mod colors;
mod context;
pub mod doc;
pub mod examples;
pub mod format;
mod grid;
mod label;
pub mod layout;
mod legend;
mod mark_color;
pub mod marks;
pub mod scale;
pub mod shape;
mod swatch;
mod tooltip;
mod types;
mod utils;
mod value;

#[cfg(test)]
mod chrome_tests;
#[cfg(test)]
mod mark_tests;
#[cfg(test)]
mod tests;

pub use axis::{AxisPosition, ChartAxis, TickFormat};
pub use chart::Chart;
pub use context::{ChartContext, PointerHandler, use_chart};
pub use grid::ChartGrid;
pub use legend::{ChartLegend, LegendAlignment, LegendItem, LegendPosition, derive_legend_items};
pub use mark_color::{point_fill, series_fill};
pub use marks::*;
pub use swatch::{ChartSwatch, ChartSwatchVariant, swatch_variant_for, swatch_variant_for_type};
pub use tooltip::{
    ChartTooltip, ChartTooltipPlacement, TooltipRender, TooltipSeriesValue,
    derive_tooltip_series_values, has_multiple_primary_series, should_render_hover_dot,
};
pub use types::*;
pub use utils::{is_band_scale, x_pixel};
pub use value::{ChartValue, Datum, js_number, js_string};
