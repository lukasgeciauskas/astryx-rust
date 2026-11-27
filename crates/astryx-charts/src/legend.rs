//! The legend's items (upstream `legend.ts`) and `ChartLegend`.

use astryx_components::text::{Text, TextType};
use gpui::prelude::*;
use gpui::{App, Hsla, Role, SharedString, Window, div};

use crate::swatch::{ChartSwatch, swatch_variant_for};

use crate::types::{SeriesDef, is_utility_mark_type};

/// One legend entry (upstream `LegendItem`).
#[derive(Clone, Debug, PartialEq)]
pub struct LegendItem {
    pub label: SharedString,
    pub color: Hsla,
    /// The mark type, which picks the swatch (a square for bars, a line otherwise).
    pub kind: Option<SharedString>,
}

/// The legend items of the series (upstream `deriveLegendItems`): each primary
/// series, colored by its static color, else the palette color the chart gave it
/// (`palette[i]` for `series[i]`), else `accent`.
pub fn derive_legend_items(
    series: &[SeriesDef],
    palette: &[Option<Hsla>],
    accent: Hsla,
) -> Vec<LegendItem> {
    series
        .iter()
        .enumerate()
        .filter(|(_, s)| !is_utility_mark_type(s.kind()))
        .map(|(i, s)| LegendItem {
            label: s.label().unwrap_or_else(|| s.key()),
            color: s
                .color()
                .or(palette.get(i).copied().flatten())
                .unwrap_or(accent),
            kind: Some(s.kind().into()),
        })
        .collect()
}

/// The legend list's accessible name, in the locale (upstream
/// `t('@astryx.chartLegend.label')`): "Chart legend" in English.
pub(crate) fn legend_label(i18n: &astryx_i18n::I18n) -> SharedString {
    i18n.t("@astryx.chartLegend.label")
}

/// Where the legend sits (upstream `LegendPosition`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LegendPosition {
    Top,
    #[default]
    Bottom,
    Start,
    End,
}

/// How the legend's items align (upstream `LegendAlignment`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LegendAlignment {
    #[default]
    Start,
    Center,
    End,
}

/// A chart's legend (upstream `ChartLegend`), on its own or through `Chart::legend`: a
/// labelled list of swatches and labels, in a wrapping row above or below the plot, or
/// a column beside it.
#[derive(IntoElement, Clone, Debug, Default)]
pub struct ChartLegend {
    pub(crate) items: Option<Vec<LegendItem>>,
    pub(crate) position: Option<LegendPosition>,
    pub(crate) alignment: Option<LegendAlignment>,
}

impl ChartLegend {
    pub fn new() -> Self {
        Self::default()
    }

    /// Upstream `items`.
    pub fn items(mut self, items: impl IntoIterator<Item = LegendItem>) -> Self {
        self.items = Some(items.into_iter().collect());
        self
    }

    /// Upstream `position`.
    pub fn position(mut self, position: LegendPosition) -> Self {
        self.position = Some(position);
        self
    }

    /// Upstream `alignment`.
    pub fn alignment(mut self, alignment: LegendAlignment) -> Self {
        self.alignment = Some(alignment);
        self
    }
}

impl RenderOnce for ChartLegend {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let items = self.items.unwrap_or_default();
        if items.is_empty() {
            return div().into_any_element();
        }
        let theme = astryx_theme::current(window, cx);
        let t = theme.tokens();
        let i18n = astryx_i18n::current(cx);
        let position = self.position.unwrap_or_default();
        let alignment = self.alignment.unwrap_or_default();
        let vertical = matches!(position, LegendPosition::Start | LegendPosition::End);
        // Labels need not be unique, so each row is keyed by its index too.
        let rows = items.into_iter().enumerate().map(|(i, item)| {
            div()
                .id(("chart-legend-item", i))
                .debug_selector(move || format!("chart-legend-item-{i}"))
                .role(Role::ListItem)
                .flex()
                .flex_row()
                .items_center()
                .gap(t.spacing.s2)
                .child(ChartSwatch::new(item.color).variant(swatch_variant_for(item.kind.as_ref())))
                .child(Text::new(item.label).text_type(TextType::Supporting))
        });
        let list = div()
            .id("chart-legend")
            .debug_selector(|| "chart-legend".into())
            .role(Role::List)
            .aria_label(legend_label(&i18n))
            .flex();
        let list = if vertical {
            let list = list.flex_col().gap(t.spacing.s2);
            match alignment {
                LegendAlignment::Start => list.items_start(),
                LegendAlignment::Center => list.items_center(),
                LegendAlignment::End => list.items_end(),
            }
        } else {
            let list = list.flex_row().flex_wrap().items_center().gap(t.spacing.s4);
            match alignment {
                LegendAlignment::Start => list.justify_start(),
                LegendAlignment::Center => list.justify_center(),
                LegendAlignment::End => list.justify_end(),
            }
        };
        list.children(rows).into_any_element()
    }
}
