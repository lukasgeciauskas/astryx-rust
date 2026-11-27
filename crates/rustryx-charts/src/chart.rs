//! `Chart`: the root that lays out every series on one shared x and y scale, draws
//! them in order (grid, marks, axes, pointer layer, interactions, tooltip, children)
//! and dispatches pointer events (upstream `Chart`). It knows no mark types.
//!
//! The plot is a named image (`Role::Image`, labelled by `title` or a derived
//! default), and a small dataset is mirrored in a visually hidden table.

use std::panic::Location;
use std::rc::Rc;

use gpui::prelude::*;
use gpui::{
    AnyElement, App, AvailableSpace, Bounds, Element, ElementId, GlobalElementId, Hsla,
    InspectorElementId, LayoutId, MouseMoveEvent, Pixels, Role, SharedString, Style,
    StyleRefinement, Window, div, point, px, relative,
};
use rustryx_components::layout::{SpacingStep, Stack, VisuallyHidden};
use rustryx_components::text::{Text, TextDisplay, TextType, TextWeight};
use rustryx_i18n::Arg;

use crate::Datum;
use crate::context::ChartContext;
use crate::layout::{LayoutInput, LayoutResult, compute_layout};
use crate::legend::{ChartLegend, LegendPosition, derive_legend_items};
use crate::tooltip::ChartTooltip;
use crate::types::{
    ChartMargin, ChartPointerEvent, MarginOverride, NearestPoint, SeriesContext, SeriesDef,
    SeriesInfo, YBaseline, is_utility_mark_type,
};

/// Above this many data points (rows × data keys) the hidden data table is left out:
/// a table that large is more noise than help (upstream `MAX_TABLE_POINTS`).
const MAX_TABLE_POINTS: usize = 100;

/// Several series on one responsive x and y coordinate system (upstream `Chart`).
///
/// ```ignore
/// Chart::new("revenue", data, "month")
///     .series([bar("revenue"), line("trend")])
///     .title("Monthly revenue")
///     .grid(ChartGrid::new())
///     .axis(ChartAxis::new(AxisPosition::Bottom))
///     .axis(ChartAxis::new(AxisPosition::Left))
///     .legend(true)
///     .tooltip(true)
/// ```
#[derive(IntoElement)]
pub struct Chart {
    id: ElementId,
    data: Rc<[Datum]>,
    x_key: SharedString,
    series: Vec<SeriesDef>,
    height: f64,
    margin: MarginOverride,
    y_baseline: Option<YBaseline>,
    y_domain: Option<[f64; 2]>,
    x_domain: Option<[f64; 2]>,
    grid: Vec<AnyElement>,
    axes: Vec<AnyElement>,
    legend: Option<ChartLegend>,
    tooltip: Option<ChartTooltip>,
    interactions: Vec<AnyElement>,
    children: Vec<AnyElement>,
    title: Option<SharedString>,
    subtitle: Option<SharedString>,
    style: StyleRefinement,
    source: &'static Location<'static>,
}

impl Chart {
    /// A chart of `data` (upstream `data`), reading each row's `x_key` (upstream
    /// `xKey`) for the shared x scale. `id` keys its pointer state.
    #[track_caller]
    pub fn new(
        id: impl Into<ElementId>,
        data: impl Into<Rc<[Datum]>>,
        x_key: impl Into<SharedString>,
    ) -> Self {
        Chart {
            id: id.into(),
            data: data.into(),
            x_key: x_key.into(),
            series: Vec::new(),
            height: 300.,
            margin: MarginOverride::default(),
            y_baseline: None,
            y_domain: None,
            x_domain: None,
            grid: Vec::new(),
            axes: Vec::new(),
            legend: None,
            tooltip: None,
            interactions: Vec::new(),
            children: Vec::new(),
            title: None,
            subtitle: None,
            style: StyleRefinement::default(),
            source: Location::caller(),
        }
    }

    /// Upstream `series`: the marks drawn on the shared scales, in order.
    pub fn series(mut self, series: impl IntoIterator<Item = SeriesDef>) -> Self {
        self.series.extend(series);
        self
    }

    /// Upstream `height`, in pixels (300 by default; a non-finite or negative height is 0).
    pub fn height(mut self, height: f64) -> Self {
        self.height = height;
        self
    }

    /// Upstream `margin`: the sides it sets replace the default inset (24 above and
    /// right, 32 below, 48 left).
    pub fn margin(mut self, margin: impl Into<MarginOverride>) -> Self {
        self.margin = margin.into();
        self
    }

    /// Upstream `yBaseline` (`Auto` by default).
    pub fn y_baseline(mut self, baseline: YBaseline) -> Self {
        self.y_baseline = Some(baseline);
        self
    }

    /// Upstream `yDomain`: an exact y domain, with no baseline or headroom.
    pub fn y_domain(mut self, min: f64, max: f64) -> Self {
        self.y_domain = Some([min, max]);
        self
    }

    /// Upstream `xDomain`: an exact x domain for numeric x, kept with no data (a steady
    /// streaming window); a categorical x ignores it.
    pub fn x_domain(mut self, min: f64, max: f64) -> Self {
        self.x_domain = Some([min, max]);
        self
    }

    /// Upstream `grid`: drawn behind the series, normally a `ChartGrid`. Each call
    /// adds one.
    pub fn grid(mut self, grid: impl IntoElement) -> Self {
        self.grid.push(grid.into_any_element());
        self
    }

    /// One of upstream's `axes`: drawn after the series, normally a `ChartAxis`.
    pub fn axis(mut self, axis: impl IntoElement) -> Self {
        self.axes.push(axis.into_any_element());
        self
    }

    /// Upstream `axes`.
    pub fn axes(mut self, axes: impl IntoIterator<Item = impl IntoElement>) -> Self {
        self.axes
            .extend(axes.into_iter().map(IntoElement::into_any_element));
        self
    }

    /// Upstream `legend={true}`: the legend derived from the series, below the plot.
    pub fn legend(mut self, show: bool) -> Self {
        self.legend = show.then(ChartLegend::new);
        self
    }

    /// Upstream `legend={props}`: a legend with these items or this layout.
    pub fn legend_config(mut self, legend: ChartLegend) -> Self {
        self.legend = Some(legend);
        self
    }

    /// Upstream `tooltip={true}`: the grouped pointer tooltip.
    pub fn tooltip(mut self, show: bool) -> Self {
        self.tooltip = show.then(ChartTooltip::new);
        self
    }

    /// Upstream `tooltip={props}`: the tooltip with this presentation (the chart passes
    /// its series).
    pub fn tooltip_config(mut self, tooltip: ChartTooltip) -> Self {
        self.tooltip = Some(tooltip);
        self
    }

    /// One of upstream's `interactions`: an overlay above the pointer layer.
    pub fn interaction(mut self, interaction: impl IntoElement) -> Self {
        self.interactions.push(interaction.into_any_element());
        self
    }

    /// Upstream `title`: the visible heading and the plot's accessible name.
    pub fn title(mut self, title: impl Into<SharedString>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Upstream `subtitle`: the visible supporting text and the plot's description.
    pub fn subtitle(mut self, subtitle: impl Into<SharedString>) -> Self {
        self.subtitle = Some(subtitle.into());
        self
    }
}

impl ParentElement for Chart {
    /// Upstream `children`: drawn last, above the tooltip.
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl Styled for Chart {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

/// The chart's state across frames: the last layout and what it was made from.
#[derive(Default)]
struct ChartState {
    layout: Option<(LayoutKey, Rc<LayoutResult>)>,
}

/// What a layout depends on: the data and series by identity, and the plot's size and
/// domains.
#[derive(Clone, PartialEq)]
struct LayoutKey {
    data: usize,
    series: Vec<usize>,
    x_key: SharedString,
    size: [u64; 2],
    y_baseline: Option<YBaseline>,
    y_domain: Option<[u64; 2]>,
    x_domain: Option<[u64; 2]>,
}

fn bits(d: Option<[f64; 2]>) -> Option<[u64; 2]> {
    d.map(|[a, b]| [a.to_bits(), b.to_bits()])
}

/// The palette color of each series (`None` for one with a static color or a utility
/// mark): every primary series without a static color takes the next color of the
/// categorical palette `palette(n)`, wrapping around it.
pub(crate) fn palette_colors(
    series: &[SeriesDef],
    palette: impl FnOnce(usize) -> Vec<Hsla>,
) -> Vec<Option<Hsla>> {
    let needs_color: Vec<usize> = series
        .iter()
        .enumerate()
        .filter(|(_, s)| !is_utility_mark_type(s.kind()) && s.color().is_none())
        .map(|(i, _)| i)
        .collect();
    let palette = palette(needs_color.len().max(1));
    let mut colors = vec![None; series.len()];
    if !palette.is_empty() {
        for (k, i) in needs_color.iter().enumerate() {
            colors[*i] = Some(palette[k % palette.len()]);
        }
    }
    colors
}

/// The plot's accessible name: the title, else one derived from the primary series'
/// labels and the x key ("Chart of revenue and profit by month"), else "Chart".
pub(crate) fn accessible_label(
    title: Option<&SharedString>,
    series: &[SeriesDef],
    x_key: &str,
    i18n: &rustryx_i18n::I18n,
) -> SharedString {
    if let Some(title) = title {
        return title.clone();
    }
    let labels: Vec<SharedString> = series
        .iter()
        .filter(|s| !is_utility_mark_type(s.kind()))
        .map(|s| s.label().unwrap_or_else(|| s.key()))
        .collect();
    if labels.is_empty() {
        return i18n.t("@astryx.chart.label");
    }
    let list = crate::format::format_list(&labels, i18n.locale());
    i18n.format(
        "@astryx.chart.labelWithSeries",
        &[
            ("series", Arg::from(list.as_str())),
            ("xKey", Arg::from(x_key)),
        ],
    )
}

/// The hidden table's columns: each primary series' data keys, once each.
pub(crate) fn table_keys(series: &[SeriesDef]) -> Vec<SharedString> {
    let mut keys: Vec<SharedString> = Vec::new();
    for s in series.iter().filter(|s| !is_utility_mark_type(s.kind())) {
        for k in s.data_keys() {
            if !keys.contains(&k) {
                keys.push(k);
            }
        }
    }
    keys
}

/// The hidden table is drawn for a small dataset only.
pub(crate) fn shows_data_table(rows: usize, keys: usize) -> bool {
    rows > 0 && keys > 0 && rows * keys <= MAX_TABLE_POINTS
}

/// A finite, non-negative length (upstream's clamp of the plot's size).
fn non_negative(x: f64) -> f64 {
    if x.is_finite() { x.max(0.) } else { 0. }
}

impl RenderOnce for Chart {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = rustryx_theme::current(window, cx);
        let t = theme.tokens();
        let i18n = rustryx_i18n::current(cx);
        let state = window.use_keyed_state(self.id.clone(), cx, |_, _| ChartState::default());
        let margin = self.margin.over(ChartMargin::default());
        let safe_height = non_negative(self.height);

        let colors = crate::colors::use_chart_colors(window, cx);
        let palette_colors = palette_colors(&self.series, |n| colors.categorical(n));
        let accessible_label =
            accessible_label(self.title.as_ref(), &self.series, &self.x_key, &i18n);
        let table_keys = table_keys(&self.series);
        let show_data_table = shows_data_table(self.data.len(), table_keys.len());
        let data_table = show_data_table.then(|| {
            data_table(
                &self.data,
                &self.x_key,
                &table_keys,
                i18n.format(
                    "@astryx.chart.dataTableCaption",
                    &[("label", Arg::from(accessible_label.as_ref()))],
                ),
            )
        });

        let accent = t.color.accent;
        let legend_position = self
            .legend
            .as_ref()
            .and_then(|l| l.position)
            .unwrap_or_default();
        let legend = self.legend.map(|config| {
            let items = config
                .items
                .clone()
                .unwrap_or_else(|| derive_legend_items(&self.series, &palette_colors, accent));
            ChartLegend {
                items: Some(items),
                ..config
            }
        });
        let series = self.series.clone();
        let tooltip = self.tooltip.map(|tooltip| {
            if tooltip.series.is_empty() {
                tooltip.series(series.iter().cloned())
            } else {
                tooltip
            }
        });

        let plot_id: ElementId = (self.id.clone(), "plot").into();
        let input = PlotInput {
            id: self.id.clone(),
            data: self.data.clone(),
            x_key: self.x_key.clone(),
            series: self.series,
            palette_colors,
            margin,
            y_baseline: self.y_baseline,
            y_domain: self.y_domain,
            x_domain: self.x_domain,
            grid: self.grid,
            axes: self.axes,
            tooltip,
            interactions: self.interactions,
            children: self.children,
            label: accessible_label,
            subtitle: self.subtitle.clone(),
            state,
        };
        let plot = PlotArea {
            id: plot_id,
            height: safe_height,
            input: Some(input),
            child: None,
            source: self.source,
        };

        let body: AnyElement = match legend {
            None => plot.into_any_element(),
            Some(legend)
                if matches!(legend_position, LegendPosition::Start | LegendPosition::End) =>
            {
                // The plot takes the row's room beside the legend (upstream sizes it
                // from the whole chart's width and clips the far side).
                let area = div().flex_1().min_w_0().overflow_hidden().child(plot);
                let start = legend_position == LegendPosition::Start;
                let (before, after) = if start {
                    (Some(legend), None)
                } else {
                    (None, Some(legend))
                };
                Stack::horizontal()
                    .gap(SpacingStep::S4)
                    .children(before)
                    .child(area)
                    .children(after)
                    .into_any_element()
            }
            Some(legend) => {
                let top = legend_position == LegendPosition::Top;
                let (before, after) = if top {
                    (Some(legend), None)
                } else {
                    (None, Some(legend))
                };
                Stack::vertical()
                    .gap(SpacingStep::S4)
                    .children(before)
                    .child(plot)
                    .children(after)
                    .into_any_element()
            }
        };

        let mut root = div().id(self.id.clone()).w_full().flex().flex_col();
        root.style().refine(&self.style);
        root.when(self.title.is_some() || self.subtitle.is_some(), |root| {
            root.child(
                div()
                    .mb(t.spacing.s4)
                    .when_some(self.title.clone(), |d, title| {
                        d.child(
                            Text::new(title)
                                .text_type(TextType::Body)
                                .weight(TextWeight::Semibold)
                                .display(TextDisplay::Block),
                        )
                    })
                    .when_some(self.subtitle.clone(), |d, subtitle| {
                        d.child(
                            Text::new(subtitle)
                                .text_type(TextType::Supporting)
                                .display(TextDisplay::Block),
                        )
                    }),
            )
        })
        .child(body)
        .children(data_table)
    }
}

/// The hidden data table (upstream's `VisuallyHidden` table): a column per primary
/// data key, a row per datum.
fn data_table(
    data: &[Datum],
    x_key: &SharedString,
    keys: &[SharedString],
    caption: SharedString,
) -> AnyElement {
    let cell = |text: SharedString, role: Role, id: String| {
        div().id(id).role(role).aria_label(text.clone()).child(text)
    };
    let header = div()
        .id("chart-table-head")
        .role(Role::Row)
        .child(cell(x_key.clone(), Role::ColumnHeader, "h-x".into()))
        .children(
            keys.iter()
                .enumerate()
                .map(|(i, k)| cell(k.clone(), Role::ColumnHeader, format!("h-{i}"))),
        );
    let table_cell = |v: Option<&crate::ChartValue>| -> SharedString {
        match v {
            None | Some(crate::ChartValue::Null) => "".into(),
            Some(v) => v.to_js_string(),
        }
    };
    let rows = data.iter().enumerate().map(|(r, d)| {
        div()
            .id(("chart-table-row", r))
            .role(Role::Row)
            .child(cell(
                table_cell(d.get(x_key)),
                Role::RowHeader,
                format!("r{r}-x"),
            ))
            .children(
                keys.iter()
                    .enumerate()
                    .map(|(i, k)| cell(table_cell(d.get(k)), Role::Cell, format!("r{r}-{i}"))),
            )
    });
    VisuallyHidden::new()
        .child(
            div()
                .id("chart-table")
                .role(Role::Table)
                .aria_label(caption)
                .child(header)
                .children(rows),
        )
        .into_any_element()
}

/// What the plot draws once it knows its width.
struct PlotInput {
    id: ElementId,
    data: Rc<[Datum]>,
    x_key: SharedString,
    series: Vec<SeriesDef>,
    palette_colors: Vec<Option<gpui::Hsla>>,
    margin: ChartMargin,
    y_baseline: Option<YBaseline>,
    y_domain: Option<[f64; 2]>,
    x_domain: Option<[f64; 2]>,
    grid: Vec<AnyElement>,
    axes: Vec<AnyElement>,
    tooltip: Option<ChartTooltip>,
    interactions: Vec<AnyElement>,
    children: Vec<AnyElement>,
    label: SharedString,
    subtitle: Option<SharedString>,
    state: gpui::Entity<ChartState>,
}

/// The plot: as wide as it is given and `height` tall; it lays out and builds its
/// layers once its width is known (upstream measures it with a ResizeObserver and
/// draws nothing at width 0).
struct PlotArea {
    id: ElementId,
    height: f64,
    input: Option<PlotInput>,
    child: Option<AnyElement>,
    source: &'static Location<'static>,
}

impl Element for PlotArea {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
    }

    fn source_location(&self) -> Option<&'static Location<'static>> {
        Some(self.source)
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = px(self.height as f32).into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        let Some(input) = self.input.take() else {
            return;
        };
        if bounds.size.width <= px(0.) {
            return;
        }
        let mut element = build_plot(input, bounds, window, cx);
        element.layout_as_root(bounds.size.map(AvailableSpace::Definite), window, cx);
        element.prepaint_at(bounds.origin, window, cx);
        self.child = Some(element);
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        if let Some(child) = self.child.as_mut() {
            child.paint(window, cx);
        }
    }
}

/// Lays the series out at the plot's size and builds its layers.
fn build_plot(
    input: PlotInput,
    bounds: Bounds<Pixels>,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let PlotInput {
        id,
        data,
        x_key,
        series,
        palette_colors,
        margin,
        y_baseline,
        y_domain,
        x_domain,
        grid,
        axes,
        tooltip,
        interactions,
        children,
        label,
        subtitle,
        state,
    } = input;
    let width = f64::from(bounds.size.width);
    let height = f64::from(bounds.size.height);
    let inner_width = non_negative(width - margin.left - margin.right);
    let inner_height = non_negative(height - margin.top - margin.bottom);

    let key = LayoutKey {
        data: Rc::as_ptr(&data) as *const () as usize,
        series: series
            .iter()
            .map(|s| Rc::as_ptr(s) as *const () as usize)
            .collect(),
        x_key: x_key.clone(),
        size: [inner_width.to_bits(), inner_height.to_bits()],
        y_baseline,
        y_domain: bits(y_domain),
        x_domain: bits(x_domain),
    };
    let cached = state
        .read(cx)
        .layout
        .as_ref()
        .filter(|(k, _)| *k == key)
        .map(|(_, l)| l.clone());
    let layout = match cached {
        Some(layout) => layout,
        None => {
            let layout = Rc::new(compute_layout(LayoutInput {
                data: &data,
                x_key: &x_key,
                series: &series,
                width: inner_width,
                height: inner_height,
                y_baseline,
                y_domain,
                x_domain,
            }));
            state.update(cx, |s, _| s.layout = Some((key, layout.clone())));
            layout
        }
    };

    let infos: Vec<SeriesInfo> = layout
        .info
        .iter()
        .zip(&palette_colors)
        .map(|(info, color)| SeriesInfo {
            resolved_color: *color,
            ..info.clone()
        })
        .collect();
    let series_ctx = SeriesContext {
        data: data.clone(),
        x_key: x_key.clone(),
        x_scale: layout.x_scale.clone(),
        y_scale: layout.y_scale,
        y_band_scale: layout.y_band_scale.clone(),
        width: inner_width,
        height: inner_height,
    };
    let resolved = Rc::new(layout.resolved.clone());
    let chart = ChartContext {
        id,
        width: inner_width,
        height: inner_height,
        margin,
        data,
        x_key,
        x_scale: layout.x_scale.clone(),
        y_scale: layout.y_scale,
        y_band_scale: layout.y_band_scale.clone(),
        resolved: resolved.clone(),
        series: series.iter().cloned().zip(infos.iter().cloned()).collect(),
        handlers: Default::default(),
    };

    let (ml, mt) = (px(margin.left as f32), px(margin.top as f32));
    let (iw, ih) = (px(inner_width as f32), px(inner_height as f32));
    let layer = |child: AnyElement| div().absolute().left(ml).top(mt).w(iw).h(ih).child(child);

    // Each series draws itself, clipped to the plot so curve overshoot stays out of
    // the margins.
    let mut marks = Vec::new();
    let mut above = Vec::new();
    for (s, info) in series.iter().zip(&infos) {
        let Some((_, points)) = resolved.iter().find(|(uid, _)| *uid == info.uid) else {
            continue;
        };
        if let Some(element) = s.render(points, &series_ctx, info, window, cx) {
            let element = div().absolute().top_0().left_0().size_full().child(element);
            if s.draws_above() {
                above.push(element);
            } else {
                marks.push(element);
            }
        }
    }

    // The pointer layer: the nearest point by x (the whole column is its hover zone).
    let plot_origin = bounds.origin + point(ml, mt);
    let handlers = chart.handlers.clone();
    let leave_handlers = chart.handlers.clone();
    let event_layer = div()
        .id("chart-pointer")
        .debug_selector(|| "chart-pointer".into())
        .absolute()
        .left(ml)
        .top(mt)
        .w(iw)
        .h(ih)
        .on_mouse_move(move |e: &MouseMoveEvent, window, cx| {
            let x = f64::from(e.position.x - plot_origin.x);
            let y = f64::from(e.position.y - plot_origin.y);
            let mut nearest = None;
            let mut best = f64::INFINITY;
            for (series_key, points) in resolved.iter() {
                for p in points {
                    let d = (p.px - x).abs();
                    if d < best {
                        best = d;
                        nearest = Some(NearestPoint {
                            point: *p,
                            series_key: series_key.clone(),
                        });
                    }
                }
            }
            let event = ChartPointerEvent {
                x,
                y,
                nearest,
                active: e.pressed_button.is_some(),
            };
            let listeners = handlers.borrow().clone();
            for handler in listeners {
                handler(&event, window, cx);
            }
        })
        .on_hover(move |hovered, window, cx| {
            if !*hovered {
                let event = ChartPointerEvent {
                    x: -1.,
                    y: -1.,
                    nearest: None,
                    active: false,
                };
                let listeners = leave_handlers.borrow().clone();
                for handler in listeners {
                    handler(&event, window, cx);
                }
            }
        });

    let mut plot = div()
        .id("chart-plot")
        .relative()
        .size_full()
        .role(Role::Image)
        .aria_label(label);
    if let Some(subtitle) = subtitle {
        plot = plot.aria_description(subtitle);
    }
    let plot = plot
        .children(grid.into_iter().map(layer))
        .child(
            div()
                .absolute()
                .left(ml)
                .top(mt)
                .w(iw)
                .h(ih)
                .overflow_hidden()
                .children(marks),
        )
        .children(axes.into_iter().map(layer))
        .child(event_layer)
        .children(interactions.into_iter().map(layer))
        .children(tooltip.map(|t| layer(t.into_any_element())))
        .children(children.into_iter().map(layer))
        .when(!above.is_empty(), |plot| {
            plot.child(
                div()
                    .absolute()
                    .left(ml)
                    .top(mt)
                    .w(iw)
                    .h(ih)
                    .overflow_hidden()
                    .children(above),
            )
        });
    rustryx_primitives::provide(chart, plot).into_any_element()
}
