//! The Rust side of each chart component's documentation (see
//! `rustryx_components::doc`): `rustryx component <Name> --package charts` and
//! `rustryx parity` read this list.

pub use rustryx_components::doc::{ComponentDoc, PropMapping, RustApi};

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

const EXAMPLE: &str = include_str!("examples.rs");

/// Upstream `Chart`.
pub const CHART_DOC: ComponentDoc = ComponentDoc {
    name: "Chart",
    rust_type: "rustryx::charts::Chart",
    props: &[
        p("data", RustApi::Argument("data")),
        p("xKey", RustApi::Argument("x_key")),
        p("series", RustApi::Method("series")),
        p("height", RustApi::Method("height")),
        p("margin", RustApi::Method("margin")),
        p("yBaseline", RustApi::Method("y_baseline")),
        p("yDomain", RustApi::Method("y_domain")),
        p("xDomain", RustApi::Method("x_domain")),
        p("grid", RustApi::Method("grid")),
        p("axes", RustApi::Method("axes")),
        p("legend", RustApi::Method("legend")),
        p("tooltip", RustApi::Method("tooltip")),
        p("interactions", RustApi::Method("interaction")),
        p("children", RustApi::Trait("ParentElement")),
        p("title", RustApi::Method("title")),
        p("subtitle", RustApi::Method("subtitle")),
        p(
            "ref",
            RustApi::Waived(
                "GPUI elements have no DOM node to forward; the element id identifies the chart",
            ),
        ),
        p("xstyle", RustApi::Trait("Styled")),
        p("className", RustApi::Trait("Styled")),
        p("style", RustApi::Trait("Styled")),
    ],
    example: EXAMPLE,
};

/// Upstream `ChartAxis`.
pub const CHART_AXIS_DOC: ComponentDoc = ComponentDoc {
    name: "ChartAxis",
    rust_type: "rustryx::charts::ChartAxis",
    props: &[
        p("position", RustApi::Argument("position")),
        p("tickCount", RustApi::Method("tick_count")),
        p("maxTicks", RustApi::Method("max_ticks")),
        p("tickFormat", RustApi::Method("tick_format")),
        p("truncate", RustApi::Method("truncate")),
        p("animated", RustApi::Method("animated")),
        p("showAxisLine", RustApi::Method("show_axis_line")),
        p("showTicks", RustApi::Method("show_ticks")),
    ],
    example: EXAMPLE,
};

/// Upstream `ChartGrid`.
pub const CHART_GRID_DOC: ComponentDoc = ComponentDoc {
    name: "ChartGrid",
    rust_type: "rustryx::charts::ChartGrid",
    props: &[
        p("horizontal", RustApi::Method("horizontal")),
        p("vertical", RustApi::Method("vertical")),
        p("tickCount", RustApi::Method("tick_count")),
    ],
    example: EXAMPLE,
};

/// Upstream `ChartLegend`.
pub const CHART_LEGEND_DOC: ComponentDoc = ComponentDoc {
    name: "ChartLegend",
    rust_type: "rustryx::charts::ChartLegend",
    props: &[
        p("items", RustApi::Method("items")),
        p("position", RustApi::Method("position")),
        p("alignment", RustApi::Method("alignment")),
    ],
    example: EXAMPLE,
};

/// Upstream `ChartSwatch`.
pub const CHART_SWATCH_DOC: ComponentDoc = ComponentDoc {
    name: "ChartSwatch",
    rust_type: "rustryx::charts::ChartSwatch",
    props: &[
        p("color", RustApi::Argument("color")),
        p("variant", RustApi::Method("variant")),
    ],
    example: EXAMPLE,
};

/// Upstream `ChartTooltip`.
pub const CHART_TOOLTIP_DOC: ComponentDoc = ComponentDoc {
    name: "ChartTooltip",
    rust_type: "rustryx::charts::ChartTooltip",
    props: &[
        p("series", RustApi::Method("series")),
        p("render", RustApi::Method("render")),
        p("hoverIndicator", RustApi::Method("hover_indicator")),
        p("showHoverDots", RustApi::Method("show_hover_dots")),
        p("placement", RustApi::Method("placement")),
    ],
    example: EXAMPLE,
};

/// Every ported chart component.
pub const REGISTRY: &[ComponentDoc] = &[
    CHART_DOC,
    CHART_AXIS_DOC,
    CHART_GRID_DOC,
    CHART_LEGEND_DOC,
    CHART_SWATCH_DOC,
    CHART_TOOLTIP_DOC,
];
