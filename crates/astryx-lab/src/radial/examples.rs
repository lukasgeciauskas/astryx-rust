//! The RadialChart gallery (upstream's `Lab/RadialChart` stories).

use std::rc::Rc;

use astryx_charts::colors::use_chart_colors;
use astryx_charts::{ChartLegend, Datum, LegendItem, datum};
use astryx_components::layout::{SpacingStep, Stack};
use astryx_components::text::{Heading, HeadingLevel};
use gpui::{AnyElement, App, IntoElement, ParentElement, Styled, Window, div, px};

use super::{RadialArea, RadialAxis, RadialChart, RadialGrid, RadialSlice, RadialTooltip};

fn spider_data() -> Rc<[Datum]> {
    vec![
        datum! { "model" => "Model A", "speed" => 85, "handling" => 70, "comfort" => 90, "safety" => 95, "efficiency" => 60 },
        datum! { "model" => "Model B", "speed" => 70, "handling" => 95, "comfort" => 60, "safety" => 80, "efficiency" => 85 },
        datum! { "model" => "Model C", "speed" => 95, "handling" => 60, "comfort" => 75, "safety" => 70, "efficiency" => 90 },
    ]
    .into()
}

const REGIONS: [(&str, i32); 5] = [
    ("North America", 42),
    ("Europe", 28),
    ("Asia Pacific", 18),
    ("Latin America", 8),
    ("Africa", 4),
];

fn pie_data() -> Rc<[Datum]> {
    REGIONS
        .iter()
        .map(|(region, revenue)| datum! { "region" => *region, "revenue" => *revenue })
        .collect::<Vec<_>>()
        .into()
}

const AXES: [&str; 5] = ["speed", "handling", "comfort", "safety", "efficiency"];

fn story(title: &'static str, chart: impl IntoElement, legend: Option<ChartLegend>) -> AnyElement {
    Stack::vertical()
        .gap(SpacingStep::S4)
        .child(Heading::new(HeadingLevel::H3, title))
        .child(div().w(px(600.)).max_w_full().child(chart))
        .children(legend)
        .into_any_element()
}

/// The RadialChart examples: a spider chart, a pie, a donut, a spider with an inner
/// radius and a donut with its tooltip. Upstream's stories put the legend inside the
/// chart's SVG, where a browser does not draw HTML; here it sits below.
pub fn examples(window: &mut Window, cx: &mut App) -> AnyElement {
    let colors = use_chart_colors(window, cx);
    let c = colors.categorical(5);
    let legend = |labels: &[&str]| {
        ChartLegend::new().items(labels.iter().enumerate().map(|(i, label)| LegendItem {
            label: label.to_string().into(),
            color: c[i % c.len()],
            kind: None,
        }))
    };
    let regions: Vec<&str> = REGIONS.iter().map(|(r, _)| *r).collect();
    Stack::vertical()
        .gap(SpacingStep::S8)
        .child(story(
            "Spider Chart",
            RadialChart::new("radial-spider", spider_data())
                .axes(AXES)
                .height(400.)
                .child(RadialGrid::new().rings(5))
                .child(RadialArea::new("Model A", c[0]).dots(true))
                .child(RadialArea::new("Model B", c[1]).dots(true))
                .child(RadialArea::new("Model C", c[2]).dots(true))
                .child(RadialAxis::new()),
            Some(legend(&["Model A", "Model B", "Model C"])),
        ))
        .child(story(
            "Pie Chart",
            RadialChart::new("radial-pie", pie_data())
                .value_key("revenue")
                .label_key("region")
                .height(400.)
                .child(RadialSlice::new(c.clone())),
            Some(legend(&regions)),
        ))
        .child(story(
            "Donut Chart",
            RadialChart::new("radial-donut", pie_data())
                .value_key("revenue")
                .label_key("region")
                .inner_radius(0.55)
                .height(400.)
                .child(RadialSlice::new(c.clone())),
            Some(legend(&regions)),
        ))
        .child(story(
            "Spider with Inner Radius",
            RadialChart::new("radial-spider-donut", spider_data())
                .axes(AXES)
                .inner_radius(0.2)
                .height(400.)
                .child(RadialGrid::new().rings(4))
                .child(RadialArea::new("Model A", c[0]).dots(true))
                .child(RadialArea::new("Model B", c[1]).dots(true))
                .child(RadialAxis::new()),
            None,
        ))
        // Not a story upstream: the tooltip on hover (or a finger, as the chart is
        // interactive).
        .child(story(
            "Tooltip",
            RadialChart::new("radial-tooltip", pie_data())
                .value_key("revenue")
                .label_key("region")
                .inner_radius(0.55)
                .interactive(true)
                .height(400.)
                .child(RadialSlice::new(c.clone()))
                .child(RadialTooltip::new()),
            Some(legend(&regions)),
        ))
        .into_any_element()
}
