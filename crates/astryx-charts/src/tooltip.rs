//! `ChartTooltip`: the grouped tooltip at the hovered x (upstream `ChartTooltip` and
//! `tooltip.ts`). It follows the chart's pointer and redraws only when the hovered
//! datum changes: a band highlight (bars on a band scale) or a crosshair, a hover dot
//! on each series' point, and a card listing each series' value there.

use std::rc::Rc;

use astryx_components::text::{Text, TextType, TextWeight};
use astryx_primitives::{LayerOptions, LayerPosition, Placement, anchored_layer};
use astryx_theme::{ThemeStyled, Tokens};
use gpui::prelude::*;
use gpui::{
    AnyElement, App, Bounds, Hsla, Pixels, Role, SharedString, Window, canvas, div, px, size,
};

use crate::context::{ChartContext, use_chart};
use crate::marks::stroke_segment;
use crate::swatch::{ChartSwatch, swatch_variant_for_type};
use crate::types::{ChartScale, SeriesDef, is_utility_mark_type};
use crate::utils::x_pixel;
use crate::{ChartValue, js_string};

/// One series' row in the tooltip (upstream `TooltipSeriesValue`).
#[derive(Clone, Debug, PartialEq)]
pub struct TooltipSeriesValue {
    /// The series' key.
    pub key: SharedString,
    /// Its label, else its key.
    pub label: SharedString,
    /// Its color, as the legend shows it.
    pub color: Hsla,
    /// Its mark type, which picks the swatch.
    pub kind: SharedString,
    /// Its value at the hovered x (`None` when the row lacks it).
    pub value: Option<ChartValue>,
}

/// The rows for a hovered datum (upstream `deriveTooltipSeriesValues`): each primary
/// series that reads a data key and was laid out, in order.
pub fn derive_tooltip_series_values(
    series: &[SeriesDef],
    chart: &ChartContext,
    datum: Option<&crate::Datum>,
    accent: Hsla,
) -> Vec<TooltipSeriesValue> {
    let Some(datum) = datum else {
        return Vec::new();
    };
    series
        .iter()
        .filter(|s| !is_utility_mark_type(s.kind()))
        .filter_map(|s| {
            let keys = s.data_keys();
            let first = keys.first()?;
            let info = chart.info(s)?;
            chart.resolved_for(&info.uid)?;
            Some(TooltipSeriesValue {
                key: s.key(),
                label: s.label().unwrap_or_else(|| s.key()),
                // The legend's precedence, so the two swatches match.
                color: s.color().or(info.resolved_color).unwrap_or(accent),
                kind: s.kind().into(),
                value: datum.get(first).cloned(),
            })
        })
        .collect()
}

/// More than one primary series: the default card labels every row (upstream
/// `hasMultiplePrimarySeries`).
pub fn has_multiple_primary_series(series: &[SeriesDef]) -> bool {
    series
        .iter()
        .filter(|s| !is_utility_mark_type(s.kind()))
        .count()
        > 1
}

/// A series draws a hover dot at the hovered index: not bars (they get the band
/// highlight) and not utility marks (upstream `shouldRenderHoverDot`).
pub fn should_render_hover_dot(kind: &str) -> bool {
    kind != "bar" && !is_utility_mark_type(kind)
}

/// Where the card sits against the hovered point (upstream `ChartTooltipPlacement`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ChartTooltipPlacement {
    /// Right of the point, flipping left when it would overflow.
    #[default]
    Auto,
    Right,
    Left,
    Top,
}

/// A custom card (upstream `render`): the hovered x value and a row per series;
/// `None` hides the card but keeps the highlight and dots.
pub type TooltipRender = Rc<
    dyn Fn(Option<&ChartValue>, &[TooltipSeriesValue], &mut Window, &mut App) -> Option<AnyElement>,
>;

/// A chart's grouped tooltip (upstream `ChartTooltip`): through `Chart::tooltip`, or
/// among a chart's children with the chart's own series for full control.
#[derive(IntoElement, Clone)]
pub struct ChartTooltip {
    pub(crate) series: Vec<SeriesDef>,
    render: Option<TooltipRender>,
    hover_indicator: bool,
    show_hover_dots: bool,
    placement: ChartTooltipPlacement,
}

impl Default for ChartTooltip {
    fn default() -> Self {
        ChartTooltip {
            series: Vec::new(),
            render: None,
            hover_indicator: true,
            show_hover_dots: true,
            placement: ChartTooltipPlacement::Auto,
        }
    }
}

impl ChartTooltip {
    pub fn new() -> Self {
        Self::default()
    }

    /// Upstream `series`: the chart's series, which `Chart::tooltip` passes itself.
    pub fn series(mut self, series: impl IntoIterator<Item = SeriesDef>) -> Self {
        self.series = series.into_iter().collect();
        self
    }

    /// Upstream `render`: a custom card.
    pub fn render(
        mut self,
        render: impl Fn(
            Option<&ChartValue>,
            &[TooltipSeriesValue],
            &mut Window,
            &mut App,
        ) -> Option<AnyElement>
        + 'static,
    ) -> Self {
        self.render = Some(Rc::new(render));
        self
    }

    /// Upstream `hoverIndicator`: a band highlight for bars on a band scale, else a
    /// crosshair (on by default).
    pub fn hover_indicator(mut self, show: bool) -> Self {
        self.hover_indicator = show;
        self
    }

    /// Upstream `showHoverDots`: a dot on each series' hovered point, bars aside (on by
    /// default).
    pub fn show_hover_dots(mut self, show: bool) -> Self {
        self.show_hover_dots = show;
        self
    }

    /// Upstream `placement`.
    pub fn placement(mut self, placement: ChartTooltipPlacement) -> Self {
        self.placement = placement;
        self
    }
}

/// The hovered datum and where its point is, across frames.
#[derive(Default, PartialEq)]
struct Hover {
    index: Option<usize>,
    px: f64,
}

/// Where the card goes (upstream `positionCard`): beside or above the point at the
/// plot's top, kept 8 px inside the window.
pub(crate) fn place_card(
    anchor: Bounds<Pixels>,
    card: gpui::Size<Pixels>,
    viewport: Bounds<Pixels>,
    placement: ChartTooltipPlacement,
) -> LayerPosition {
    let gap = 8.;
    let sx = f64::from(anchor.origin.x);
    let sy = f64::from(anchor.origin.y);
    let (w, h) = (f64::from(card.width), f64::from(card.height));
    let (vw, vh) = (
        f64::from(viewport.size.width),
        f64::from(viewport.size.height),
    );
    let mut y = sy;
    let (mut x, side) = match placement {
        ChartTooltipPlacement::Left => (sx - w - gap, Placement::Start),
        ChartTooltipPlacement::Right => (sx + gap, Placement::End),
        ChartTooltipPlacement::Top => {
            y = sy - h - gap;
            (sx - w / 2., Placement::Above)
        }
        ChartTooltipPlacement::Auto => {
            let right = sx + gap;
            if right + w > vw {
                (sx - w - gap, Placement::Start)
            } else {
                (right, Placement::End)
            }
        }
    };
    // Oversized content keeps its size and may run past the far edge.
    x = gap.max(x.min(vw - w - gap));
    y = gap.max(y.min(vh - h - gap));
    LayerPosition {
        origin: gpui::point(px(x as f32), px(y as f32)),
        side,
    }
}

/// One labelled row of the default card: the series' swatch, label and value.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CardRow {
    pub color: Hsla,
    pub kind: SharedString,
    pub label: SharedString,
    pub value: SharedString,
}

/// The default card's body: the one value bare, or a labelled row per series.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum CardBody {
    Value(SharedString),
    Rows(Vec<CardRow>),
}

/// What the default card shows (upstream `DefaultTooltipContent`): the hovered x as its
/// title, then the bare value of a single-series chart or a labelled row per series.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DefaultCard {
    pub title: SharedString,
    pub body: CardBody,
}

impl DefaultCard {
    /// The card for the hovered x and its rows; `label_rows` labels every row, as a chart
    /// of several primary series does even when one row resolves.
    pub(crate) fn new(
        x_value: Option<&ChartValue>,
        rows: &[TooltipSeriesValue],
        label_rows: bool,
    ) -> Self {
        let body = if rows.len() == 1 && !label_rows {
            CardBody::Value(js_string(rows[0].value.as_ref()))
        } else {
            CardBody::Rows(
                rows.iter()
                    .map(|row| CardRow {
                        color: row.color,
                        kind: row.kind.clone(),
                        label: row.label.clone(),
                        value: js_string(row.value.as_ref()),
                    })
                    .collect(),
            )
        };
        DefaultCard {
            title: js_string(x_value),
            body,
        }
    }

    /// The card's text in reading order (the DOM's `textContent`).
    #[cfg(test)]
    pub(crate) fn text(&self) -> String {
        let mut out = self.title.to_string();
        match &self.body {
            CardBody::Value(value) => out += value,
            CardBody::Rows(rows) => {
                for row in rows {
                    out += &row.label;
                    out += &row.value;
                }
            }
        }
        out
    }

    fn into_element(self, gap: gpui::AbsoluteLength, row_gap: gpui::AbsoluteLength) -> AnyElement {
        let title = Text::new(self.title)
            .text_type(TextType::Body)
            .weight(TextWeight::Semibold);
        let body: AnyElement = match self.body {
            CardBody::Value(value) => Text::new(value)
                .text_type(TextType::Supporting)
                .into_any_element(),
            CardBody::Rows(rows) => div()
                .flex()
                .flex_col()
                .gap(gap)
                .children(rows.into_iter().map(|row| {
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(row_gap)
                        .child(
                            ChartSwatch::new(row.color)
                                .variant(swatch_variant_for_type(Some(&row.kind))),
                        )
                        .child(Text::new(row.label).text_type(TextType::Supporting))
                        .child(
                            Text::new(row.value)
                                .text_type(TextType::Supporting)
                                .weight(TextWeight::Semibold),
                        )
                }))
                .into_any_element(),
        };
        div()
            .flex()
            .flex_col()
            .gap(gap)
            .child(title)
            .child(body)
            .into_any_element()
    }
}

/// The hover indicator (upstream's `hoverIndicatorElement`): a soft column for bars
/// on a band scale (its left, width and how far it reaches above the plot), else a
/// crosshair at an x.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Indicator {
    Band { x: f64, width: f64, top: f64 },
    Crosshair(f64),
}

/// A hover dot (upstream's `<circle r={4}>`): its center and stroke color.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct HoverDot {
    pub x: f64,
    pub y: f64,
    pub stroke: Hsla,
}

impl ChartTooltip {
    /// The indicator and dots at the hovered data index (upstream's
    /// `hoverIndicatorElement` and `dots`), with `tokens` for the dots' default stroke.
    pub(crate) fn hover_marks(
        &self,
        chart: &ChartContext,
        index: Option<usize>,
        tokens: &Tokens,
    ) -> (Option<Indicator>, Vec<HoverDot>) {
        let Some(i) = index else {
            return (None, Vec::new());
        };
        let mut dots = Vec::new();
        if self.show_hover_dots {
            for s in &self.series {
                if !should_render_hover_dot(s.kind()) {
                    continue;
                }
                let Some(info) = chart.info(s) else {
                    continue;
                };
                let point = chart
                    .resolved_for(&info.uid)
                    .and_then(|pts| pts.iter().find(|p| p.data_index == i));
                // A resolved point exists at every index, but its coordinates are NaN
                // where this series' value is missing; the mark skips those too.
                if let Some(p) = point.filter(|p| p.px.is_finite() && p.py.is_finite()) {
                    let stroke = s
                        .color()
                        .or(info.resolved_color)
                        .unwrap_or(tokens.color.border_emphasized);
                    dots.push(HoverDot {
                        x: p.px,
                        y: p.py,
                        stroke,
                    });
                }
            }
        }
        let indicator = (|| {
            if !self.hover_indicator {
                return None;
            }
            let d = chart.data.get(i)?;
            if matches!(d.get(&chart.x_key), None | Some(ChartValue::Null)) {
                return None;
            }
            let has_bars = self.series.iter().any(|s| s.kind() == "bar");
            // Bar charts on a band scale: a soft column highlight.
            if has_bars && let ChartScale::Band(band) = &chart.x_scale {
                let x = band.scale(&d.string(&chart.x_key)).unwrap_or(0.);
                let pad = 8.;
                let left = (x - pad).max(0.);
                let right = (x + band.bandwidth() + pad).min(chart.width);
                return Some(Indicator::Band {
                    x: left,
                    width: right - left,
                    top: pad.min(chart.margin.top),
                });
            }
            // Line, area and dot charts: a crosshair through the point, skipped for
            // a non-finite x on a linear scale.
            let px = x_pixel(d, &chart.x_key, &chart.x_scale);
            px.is_finite().then_some(Indicator::Crosshair(px))
        })();
        (indicator, dots)
    }
}

impl RenderOnce for ChartTooltip {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let Some(chart) = use_chart(cx) else {
            return div().into_any_element();
        };
        let theme = astryx_theme::current(window, cx);
        let t = theme.tokens();
        let hover =
            window.use_keyed_state((chart.id.clone(), "tooltip"), cx, |_, _| Hover::default());
        // Follow the pointer; redraw only when the hovered point changes.
        chart.on_pointer({
            let hover = hover.clone();
            move |e, _, cx| {
                let next = Hover {
                    index: e.nearest.as_ref().map(|n| n.point.data_index),
                    px: e.nearest.as_ref().map_or(0., |n| n.point.px),
                };
                hover.update(cx, |h, cx| {
                    if *h != next {
                        *h = next;
                        cx.notify();
                    }
                });
            }
        });
        let Hover {
            index,
            px: anchor_px,
        } = *hover.read(cx);
        let datum = index.and_then(|i| chart.data.get(i));
        let x_value = datum.and_then(|d| d.get(&chart.x_key)).cloned();
        let accent = t.color.accent;
        let rows = derive_tooltip_series_values(&self.series, &chart, datum, accent);
        let label_rows = has_multiple_primary_series(&self.series);

        let (indicator, dots) = self.hover_marks(&chart, index, t);

        let surface = t.color.background_surface;
        let crosshair = t.color.text_primary;
        let height = chart.height;
        let highlight = t.color.text_primary.opacity(0.06);
        let marks = canvas(
            |_, _, _| {},
            move |bounds: Bounds<Pixels>, _, window, _| {
                let o = bounds.origin;
                match indicator {
                    Some(Indicator::Band { x, width, top }) => {
                        let rect = Bounds::new(
                            o + gpui::point(px(x as f32), px(-top as f32)),
                            size(px(width.max(0.) as f32), px((height + top) as f32)),
                        );
                        window.paint_quad(gpui::fill(rect, highlight).corner_radii(px(4.)));
                    }
                    Some(Indicator::Crosshair(x)) => {
                        stroke_segment(window, o, (x, 0.), (x, height), 1., crosshair, &[]);
                    }
                    None => {}
                }
                for &HoverDot { x, y, stroke } in &dots {
                    // A 4 px dot with a 2 px stroke centered on its edge: 3 to 5 px.
                    let ring = Bounds::new(
                        o + gpui::point(px((x - 5.) as f32), px((y - 5.) as f32)),
                        size(px(10.), px(10.)),
                    );
                    window.paint_quad(
                        gpui::fill(ring, surface)
                            .corner_radii(px(5.))
                            .border_widths(px(2.))
                            .border_color(stroke),
                    );
                }
            },
        )
        .absolute()
        .size_full();

        // The card, beside the hovered point at the plot's top.
        let content = match (index, datum) {
            (Some(_), Some(_)) => match &self.render {
                Some(render) => render(x_value.as_ref(), &rows, window, cx),
                None => Some(
                    DefaultCard::new(x_value.as_ref(), &rows, label_rows)
                        .into_element(t.spacing.s1, t.spacing.s2),
                ),
            },
            _ => None,
        };
        let trigger = div().absolute().left(px(anchor_px as f32)).top_0().size_0();
        let placement = self.placement;
        let mut layer = anchored_layer((chart.id.clone(), "tooltip-card"), trigger)
            .light_dismiss(false)
            .position_with(move |anchor, card, viewport, _| {
                place_card(anchor, card, viewport, placement)
            });
        if let Some(content) = content {
            let card = div()
                .id((chart.id.clone(), "tooltip-surface"))
                .debug_selector(|| "chart-tooltip-card".into())
                .role(Role::Tooltip)
                .bg(t.color.background_popover)
                .border_1()
                .border_color(t.color.border)
                .rounded(t.radius.container)
                .pt(t.spacing.s2)
                .pb(t.spacing.s2)
                .pl(t.spacing.s3)
                .pr(t.spacing.s3)
                .shadow_token(&t.shadow.med)
                .whitespace_nowrap()
                .font_stack(&t.font.family.body)
                .child(content);
            let hover_dismiss = hover.clone();
            layer = layer.surface(card, LayerOptions::default(), move |_, cx| {
                hover_dismiss.update(cx, |h, cx| {
                    h.index = None;
                    cx.notify();
                });
            });
        }
        div()
            .relative()
            .size_full()
            .child(marks)
            .child(layer)
            .into_any_element()
    }
}
