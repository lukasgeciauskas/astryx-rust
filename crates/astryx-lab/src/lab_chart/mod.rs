//! Upstream lab `Chart`: the lab's composition API for charts (`<Chart><ChartLine/>…</Chart>`),
//! older than and distinct from astryx-charts' configured `Chart`. A [`Chart`] builds the
//! scales and its children draw with them: marks (`ChartLine`, `ChartBar`, `ChartArea`, `ChartDot`,
//! `ChartErrorBar`, `ChartCandlestick`, `ChartReferenceLine`), chrome (`ChartAxis`, `ChartGrid`,
//! `ChartLegend`, `ChartTooltip`), interactions (`ChartBrush`, `ChartSelect`, `ChartZoom`) and the
//! GL marks (`ChartDotGl`, `ChartDotGlInteractive`, `ChartHeatmapGl`, `ChartStreamGl`).
//!
//! The names match astryx-charts' (`Chart`, `ChartAxis`, …), so use them through this module.
//! Scales, curves, colors and the data rows are astryx-charts', the d3 pieces both share.

mod area;
mod axis;
mod bar;
mod brush;
mod candlestick;
mod chart;
mod colors;
mod context;
mod dot;
mod dot_gl;
mod dot_gl_interactive;
mod error_bar;
mod formatters;
mod grid;
mod heatmap_gl;
mod legend;
mod line;
mod m4;
mod paint;
mod range;
mod reference_line;
mod select;
mod stream_gl;
mod tooltip;
mod utils;
mod webgl;
mod zoom;

/// The gallery's lab chart examples (upstream's stories).
pub mod examples;

#[cfg(test)]
mod tests;

pub use area::{ChartArea, area_geometry};
pub use astryx_charts::{ChartScale, ChartValue, Datum};
pub use axis::{
    AxisPosition, AxisTick, ChartAxis, TickFormat, axis_line_y, axis_ticks, truncate_label,
};
pub use bar::{BarRect, ChartBar, bar_geometry};
pub use brush::{
    BrushMode, BrushOutcome, BrushRange, BrushRect, ChartBrush, brush_outcome, brush_rect,
};
pub use candlestick::{Candle, CandlestickVariant, ChartCandlestick, candlestick_geometry};
pub use chart::{
    Chart, MAX_TABLE_POINTS, YBaseline, accessible_label, chart_scales, shows_data_table,
};
pub use colors::{
    ChartColorsApi, Diverging, SequentialHue, get_chart_colors, get_chart_colors_from_resolver,
    use_chart_colors,
};
pub use context::{
    ChartContext, ChartMargin, DEFAULT_MARGIN, DataPoint, MarginOverride, use_chart,
};
pub use dot::{ChartDot, dot_geometry};
pub use dot_gl::{ChartDotGl, dot_gl_positions, dot_gl_radius};
pub use dot_gl_interactive::{
    ChartDotGlInteractive, DotGlTooltipRender, color_to_index, index_to_color, pick_point,
};
pub use error_bar::{ChartErrorBar, ErrorBarGeometry, Segment, error_bar_geometry};
pub use formatters::{compact_number, currency, month_year, percent, short_date};
pub use grid::{ChartGrid, grid_geometry};
pub use heatmap_gl::{
    ChartHeatmapGl, HeatmapCell, heatmap_cells, heatmap_domain, heatmap_rows, sample_ramp,
};
pub use legend::{ChartLegend, ChartLegendItem, LegendTickFormat, gradient_ticks};
pub use line::{ChartLine, line_geometry};
pub use m4::{M4Point, m4_reduce};
pub use range::{
    ChartRange, ChartRangeState, UseChartRangeOptions, UseChartRangeReturn, use_chart_range,
};
pub use reference_line::{
    Badge, ChartReferenceLine, ReferenceLabelPosition, ReferenceLineGeometry,
    reference_line_geometry,
};
pub use select::{ChartSelect, next_selection, select_nearest, selection_rings};
pub use stream_gl::{ChartStreamGl, ChartStreamGlHandle, StreamRing};
pub use tooltip::{
    ChartCrosshairMode, ChartTooltip, TooltipRender, XFormat, YFormat, find_nearest,
};
pub use utils::{is_band_scale, x_pixel};
pub use webgl::{POINT_SIZE_COMPENSATION, canvas_dpr, hex_to_gl};
pub use zoom::{ChartZoom, ZoomToolbarPosition, pan_domain, wheel_factor, zoom_axis};

use astryx_components::doc::{ComponentDoc, PropMapping, RustApi};

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

const EXAMPLE: &str = include_str!("examples.rs");

/// Upstream lab `ChartArea` (its doc lists no props; these are `ChartAreaProps` in order).
pub const CHART_AREA_DOC: ComponentDoc = ComponentDoc {
    name: "ChartArea",
    rust_type: "astryx::lab::lab_chart::ChartArea",
    props: &[
        p("yUpper", RustApi::Method("y_upper")),
        p("yLower", RustApi::Method("y_lower")),
        p("baseline", RustApi::Method("baseline")),
        p("color", RustApi::Argument("color")),
        p("opacity", RustApi::Method("opacity")),
        p("stroke", RustApi::Method("stroke")),
        p("strokeWidth", RustApi::Method("stroke_width")),
    ],
    example: EXAMPLE,
};

/// Upstream lab `ChartBar` (`ChartBarProps` in order).
pub const CHART_BAR_DOC: ComponentDoc = ComponentDoc {
    name: "ChartBar",
    rust_type: "astryx::lab::lab_chart::ChartBar",
    props: &[
        p("dataKey", RustApi::Argument("data_key")),
        p("color", RustApi::Argument("color")),
        p("radius", RustApi::Method("radius")),
    ],
    example: EXAMPLE,
};

/// Upstream lab `ChartCandlestick` (`ChartCandlestickProps` in order).
pub const CHART_CANDLESTICK_DOC: ComponentDoc = ComponentDoc {
    name: "ChartCandlestick",
    rust_type: "astryx::lab::lab_chart::ChartCandlestick",
    props: &[
        p("high", RustApi::Argument("high")),
        p("low", RustApi::Argument("low")),
        p("open", RustApi::Argument("open")),
        p("close", RustApi::Argument("close")),
        p("variant", RustApi::Method("variant")),
        p("upColor", RustApi::Method("up_color")),
        p("downColor", RustApi::Method("down_color")),
        p("color", RustApi::Method("color")),
        p("bodyWidth", RustApi::Method("body_width")),
        p("strokeWidth", RustApi::Method("stroke_width")),
        p("radius", RustApi::Method("radius")),
    ],
    example: EXAMPLE,
};

/// Upstream lab `ChartDot` (`ChartDotProps` in order).
pub const CHART_DOT_DOC: ComponentDoc = ComponentDoc {
    name: "ChartDot",
    rust_type: "astryx::lab::lab_chart::ChartDot",
    props: &[
        p("dataKey", RustApi::Argument("data_key")),
        p("color", RustApi::Argument("color")),
        p("radius", RustApi::Method("radius")),
    ],
    example: EXAMPLE,
};

/// Upstream lab `ChartDotGL` (`ChartDotGLProps` in order).
pub const CHART_DOT_GL_DOC: ComponentDoc = ComponentDoc {
    name: "ChartDotGL",
    rust_type: "astryx::lab::lab_chart::ChartDotGl",
    props: &[
        p("dataKey", RustApi::Argument("data_key")),
        p("color", RustApi::Argument("color")),
        p("size", RustApi::Method("size")),
        p("opacity", RustApi::Method("opacity")),
    ],
    example: EXAMPLE,
};

/// Upstream lab `ChartDotGLInteractive` (`ChartDotGLInteractiveProps` in order).
pub const CHART_DOT_GL_INTERACTIVE_DOC: ComponentDoc = ComponentDoc {
    name: "ChartDotGLInteractive",
    rust_type: "astryx::lab::lab_chart::ChartDotGlInteractive",
    props: &[
        p("dataKey", RustApi::Argument("data_key")),
        p("color", RustApi::Argument("color")),
        p("size", RustApi::Method("size")),
        p("opacity", RustApi::Method("opacity")),
        p("renderTooltip", RustApi::Method("render_tooltip")),
    ],
    example: EXAMPLE,
};

/// Upstream lab `ChartHeatmapGL` (`ChartHeatmapGLProps` in order).
pub const CHART_HEATMAP_GL_DOC: ComponentDoc = ComponentDoc {
    name: "ChartHeatmapGL",
    rust_type: "astryx::lab::lab_chart::ChartHeatmapGl",
    props: &[
        p("xKey", RustApi::Argument("x_key")),
        p("yKey", RustApi::Argument("y_key")),
        p("valueKey", RustApi::Argument("value_key")),
        p("colorRange", RustApi::Argument("color_range")),
        p("domain", RustApi::Method("domain")),
        p("cellGap", RustApi::Method("cell_gap")),
    ],
    example: EXAMPLE,
};

/// Upstream lab `ChartLine` (`ChartLineProps` in order).
pub const CHART_LINE_DOC: ComponentDoc = ComponentDoc {
    name: "ChartLine",
    rust_type: "astryx::lab::lab_chart::ChartLine",
    props: &[
        p("dataKey", RustApi::Argument("data_key")),
        p("color", RustApi::Argument("color")),
        p("strokeWidth", RustApi::Method("stroke_width")),
        p("dots", RustApi::Method("dots")),
        p("dotRadius", RustApi::Method("dot_radius")),
    ],
    example: EXAMPLE,
};

/// Upstream lab `ChartStreamGL` (`ChartStreamGLProps` in order).
pub const CHART_STREAM_GL_DOC: ComponentDoc = ComponentDoc {
    name: "ChartStreamGL",
    rust_type: "astryx::lab::lab_chart::ChartStreamGl",
    props: &[
        p(
            "ref",
            RustApi::Waived(
                "the SVG marker element upstream positions its canvas with; GPUI elements have no refs",
            ),
        ),
        p(
            "handleRef",
            RustApi::Method("handle (a ChartStreamGlHandle)"),
        ),
        p("color", RustApi::Argument("color")),
        p("bufferSize", RustApi::Method("buffer_size")),
        p("lineWidth", RustApi::Method("line_width")),
        p("opacity", RustApi::Method("opacity")),
    ],
    example: EXAMPLE,
};
