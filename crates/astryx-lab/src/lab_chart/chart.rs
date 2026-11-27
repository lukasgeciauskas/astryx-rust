//! `Chart`: the root of the lab's composition API (upstream lab `Chart.tsx`). It measures its
//! width, builds the x and y scales from the data, and gives them to its children, which
//! draw in a plot inset by the margins (the d3 margin convention).
//!
//! The plot is a named image (`Role::Image`, labelled by `label` or a derived default),
//! and a small dataset is mirrored in a visually hidden table.

use std::cell::Cell;
use std::panic::Location;
use std::rc::Rc;

use astryx_charts::scale::{BandScale, LinearScale};
use astryx_charts::{ChartScale, ChartValue, Datum};
use astryx_components::layout::VisuallyHidden;
use gpui::prelude::*;
use gpui::{
    AnyElement, App, AvailableSpace, Bounds, Element, ElementId, GlobalElementId,
    InspectorElementId, LayoutId, Pixels, Role, SharedString, Style, StyleRefinement, Window,
    canvas, div, point, px, relative,
};

use super::context::{ChartContext, ChartMargin, DEFAULT_MARGIN, MarginOverride};

/// How the y domain is computed (upstream `YBaseline`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum YBaseline {
    /// `'auto'`: zero is in the domain when every value is on one side of it (the default,
    /// for bars).
    #[default]
    Auto,
    /// `'zero'`: symmetric around zero (profit and loss, sentiment).
    Zero,
    /// `'data'`: the data's extent alone.
    Data,
}

/// Above this many data points (rows × series) the hidden data table is left out
/// (upstream `MAX_TABLE_POINTS`).
pub const MAX_TABLE_POINTS: usize = 100;

/// The root of a lab chart (upstream lab `Chart`): computes the scales from `data` and gives
/// them to its children (axes, marks, tooltips), read with [`use_chart`](super::use_chart).
///
/// ```ignore
/// Chart::new("revenue", data, "month", ["revenue"])
///     .height(300.)
///     .child(ChartAxis::new(AxisPosition::Bottom))
///     .child(ChartAxis::new(AxisPosition::Left))
///     .child(ChartBar::new("revenue", colors.categorical(1)[0]))
/// ```
#[derive(IntoElement)]
pub struct Chart {
    id: ElementId,
    data: Rc<[Datum]>,
    x_key: SharedString,
    y_keys: Vec<SharedString>,
    height: f64,
    margin: MarginOverride,
    y_baseline: YBaseline,
    y_domain: Option<[f64; 2]>,
    x_domain: Option<[f64; 2]>,
    interactive: bool,
    label: Option<SharedString>,
    children: Vec<AnyElement>,
    style: StyleRefinement,
    source: &'static Location<'static>,
}

impl Chart {
    /// Upstream `data`, `xKey` (string values make a band scale, numbers a linear one) and
    /// `yKeys` (the keys whose values make the y domain). `id` keys the chart's state.
    #[track_caller]
    pub fn new(
        id: impl Into<ElementId>,
        data: impl Into<Rc<[Datum]>>,
        x_key: impl Into<SharedString>,
        y_keys: impl IntoIterator<Item = impl Into<SharedString>>,
    ) -> Self {
        Chart {
            id: id.into(),
            data: data.into(),
            x_key: x_key.into(),
            y_keys: y_keys.into_iter().map(Into::into).collect(),
            height: 300.,
            margin: MarginOverride::default(),
            y_baseline: YBaseline::Auto,
            y_domain: None,
            x_domain: None,
            interactive: false,
            label: None,
            children: Vec::new(),
            style: StyleRefinement::default(),
            source: Location::caller(),
        }
    }

    /// Upstream `height`, in pixels (300). The width fills the container.
    pub fn height(mut self, height: f64) -> Self {
        self.height = height;
        self
    }

    /// Upstream `margin`: the sides set replace the defaults (16, 16, 32, 48).
    pub fn margin(mut self, margin: impl Into<MarginOverride>) -> Self {
        self.margin = margin.into();
        self
    }

    /// Upstream `yBaseline` (`Auto`).
    pub fn y_baseline(mut self, baseline: YBaseline) -> Self {
        self.y_baseline = baseline;
        self
    }

    /// Upstream `yDomain`: the y domain as given, with no baseline and no rounding.
    pub fn y_domain(mut self, min: f64, max: f64) -> Self {
        self.y_domain = Some([min, max]);
        self
    }

    /// Upstream `xDomain`: the x domain as given for numeric x (a streaming window); a
    /// categorical x ignores it.
    pub fn x_domain(mut self, min: f64, max: f64) -> Self {
        self.x_domain = Some([min, max]);
        self
    }

    /// Upstream `interactive`: upstream stops touch scrolling over the chart for brushes,
    /// zooms and crosshairs; GPUI has no touch scrolling to stop, so it only marks intent.
    pub fn interactive(mut self, interactive: bool) -> Self {
        self.interactive = interactive;
        self
    }

    /// Upstream `label`: the plot's accessible name ("Chart of revenue by month" by default).
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }
}

impl ParentElement for Chart {
    /// Upstream `children`: axes, marks and interactions, drawn in order in the plot.
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl Styled for Chart {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

/// The plot's accessible name: `label`, else "Chart of {yKeys} by {xKey}" (upstream's English
/// default).
pub fn accessible_label(label: Option<&str>, y_keys: &[SharedString], x_key: &str) -> SharedString {
    match label {
        Some(label) => label.to_string().into(),
        None => {
            let keys: Vec<&str> = y_keys.iter().map(|k| k.as_ref()).collect();
            format!("Chart of {} by {x_key}", keys.join(", ")).into()
        }
    }
}

/// Whether the hidden table is drawn (upstream `showDataTable`).
pub fn shows_data_table(rows: usize, y_keys: usize) -> bool {
    rows > 0 && rows * y_keys.max(1) <= MAX_TABLE_POINTS
}

/// The chart's scales for a plot of `width` by `height` (upstream's `xScale` and `yScale`
/// memos): a linear x when every x value is a number (nice unless `x_domain` is given),
/// else a band x padded 0.2; a linear y from every y key's numbers, adjusted by the
/// baseline and made nice, or `y_domain` as given.
#[allow(clippy::too_many_arguments)]
pub fn chart_scales(
    data: &[Datum],
    x_key: &str,
    y_keys: &[SharedString],
    width: f64,
    height: f64,
    y_baseline: YBaseline,
    y_domain: Option<[f64; 2]>,
    x_domain: Option<[f64; 2]>,
) -> (ChartScale, LinearScale) {
    let numeric = data
        .iter()
        .all(|d| d.get(x_key).is_some_and(ChartValue::is_number));
    let x_scale = if numeric {
        match x_domain {
            Some([a, b]) => ChartScale::Linear(LinearScale::new().domain(a, b).range(0., width)),
            None => {
                let (mut min, mut max) = (f64::INFINITY, f64::NEG_INFINITY);
                for d in data {
                    let n = d.get(x_key).map_or(f64::NAN, ChartValue::to_number);
                    if n < min {
                        min = n;
                    }
                    if n > max {
                        max = n;
                    }
                }
                ChartScale::Linear(LinearScale::new().domain(min, max).range(0., width).nice())
            }
        }
    } else {
        ChartScale::Band(
            BandScale::new()
                .domain(data.iter().map(|d| astryx_charts::js_string(d.get(x_key))))
                .range(0., width)
                .padding(0.2),
        )
    };
    let y_scale = match y_domain {
        Some([a, b]) => LinearScale::new().domain(a, b).range(height, 0.),
        None => {
            let (mut min, mut max) = (f64::INFINITY, f64::NEG_INFINITY);
            for d in data {
                for key in y_keys {
                    if let Some(ChartValue::Number(v)) = d.get(key) {
                        if *v < min {
                            min = *v;
                        }
                        if *v > max {
                            max = *v;
                        }
                    }
                }
            }
            match y_baseline {
                YBaseline::Zero => {
                    let abs = min.abs().max(max.abs());
                    min = -abs;
                    max = abs;
                }
                YBaseline::Auto => {
                    if min > 0. {
                        min = 0.;
                    }
                    if max < 0. {
                        max = 0.;
                    }
                }
                YBaseline::Data => {}
            }
            LinearScale::new().domain(min, max).range(height, 0.).nice()
        }
    };
    (x_scale, y_scale)
}

/// What a chart drew, for tests: its accessible name and the hidden table's rows.
#[cfg(test)]
#[derive(Clone, Debug, Default)]
pub(crate) struct ChartDrawn {
    pub label: String,
    pub table: Option<(String, Vec<Vec<String>>)>,
    pub plot: Option<(f64, f64)>,
}

#[cfg(test)]
thread_local! {
    pub(crate) static CHARTS: std::cell::RefCell<std::collections::HashMap<String, ChartDrawn>> = Default::default();
}

impl RenderOnce for Chart {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let margin = self.margin.over(DEFAULT_MARGIN);
        let label = accessible_label(self.label.as_deref(), &self.y_keys, &self.x_key);
        let show_table = shows_data_table(self.data.len(), self.y_keys.len());
        let table = show_table.then(|| {
            data_table(
                &self.data,
                &self.x_key,
                &self.y_keys,
                format!("{label} data").into(),
            )
        });
        #[cfg(test)]
        CHARTS.with(|c| {
            let rows = show_table.then(|| {
                let mut rows = vec![
                    std::iter::once(self.x_key.to_string())
                        .chain(self.y_keys.iter().map(|k| k.to_string()))
                        .collect(),
                ];
                rows.extend(self.data.iter().map(|d| {
                    std::iter::once(table_cell(d.get(&self.x_key)).to_string())
                        .chain(self.y_keys.iter().map(|k| table_cell(d.get(k)).to_string()))
                        .collect()
                }));
                (format!("{label} data"), rows)
            });
            let mut c = c.borrow_mut();
            let entry = c.entry(self.id.to_string()).or_default();
            entry.label = label.to_string();
            entry.table = rows;
        });
        let height = self.height.max(0.);
        let input = PlotInput {
            id: self.id.clone(),
            data: self.data,
            x_key: self.x_key,
            y_keys: self.y_keys,
            margin,
            y_baseline: self.y_baseline,
            y_domain: self.y_domain,
            x_domain: self.x_domain,
            label,
            children: self.children,
        };
        let plot = MeasuredArea {
            id: (self.id.clone(), "plot").into(),
            height,
            build: Some(Box::new(move |bounds| build_plot(input, bounds, height))),
            child: None,
            source: self.source,
        };
        let mut root = div().id(self.id).relative().w_full();
        root.style().refine(&self.style);
        root.child(plot).children(table)
    }
}

/// A table cell's text (upstream `tableCell`): blank for `null` and `undefined`.
fn table_cell(value: Option<&ChartValue>) -> SharedString {
    match value {
        None | Some(ChartValue::Null) => "".into(),
        Some(v) => v.to_js_string(),
    }
}

/// The hidden data table (upstream's `VisuallyHidden` table): the x key's column, then
/// each y key's, a row per datum.
fn data_table(
    data: &[Datum],
    x_key: &SharedString,
    y_keys: &[SharedString],
    caption: SharedString,
) -> AnyElement {
    let cell = |text: SharedString, role: Role, id: String| {
        div().id(id).role(role).aria_label(text.clone()).child(text)
    };
    let header = div()
        .id("lab-chart-table-head")
        .role(Role::Row)
        .child(cell(x_key.clone(), Role::ColumnHeader, "h-x".into()))
        .children(
            y_keys
                .iter()
                .enumerate()
                .map(|(i, k)| cell(k.clone(), Role::ColumnHeader, format!("h-{i}"))),
        );
    let rows = data.iter().enumerate().map(|(r, d)| {
        div()
            .id(("lab-chart-table-row", r))
            .role(Role::Row)
            .child(cell(
                table_cell(d.get(x_key)),
                Role::RowHeader,
                format!("r{r}-x"),
            ))
            .children(
                y_keys
                    .iter()
                    .enumerate()
                    .map(|(i, k)| cell(table_cell(d.get(k)), Role::Cell, format!("r{r}-{i}"))),
            )
    });
    VisuallyHidden::new()
        .child(
            div()
                .id("lab-chart-table")
                .role(Role::Table)
                .aria_label(caption)
                .child(header)
                .children(rows),
        )
        .into_any_element()
}

struct PlotInput {
    id: ElementId,
    data: Rc<[Datum]>,
    x_key: SharedString,
    y_keys: Vec<SharedString>,
    margin: ChartMargin,
    y_baseline: YBaseline,
    y_domain: Option<[f64; 2]>,
    x_domain: Option<[f64; 2]>,
    label: SharedString,
    children: Vec<AnyElement>,
}

/// A box as wide as it is given and `height` tall that builds its content once its width is
/// known, from its bounds (upstream measures its container with a ResizeObserver and draws
/// nothing at width 0).
pub(crate) struct MeasuredArea {
    pub(crate) id: ElementId,
    pub(crate) height: f64,
    pub(crate) build: Option<Box<dyn FnOnce(Bounds<Pixels>) -> AnyElement>>,
    pub(crate) child: Option<AnyElement>,
    pub(crate) source: &'static Location<'static>,
}

impl Element for MeasuredArea {
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
        let Some(build) = self.build.take() else {
            return;
        };
        if bounds.size.width <= px(0.) {
            return;
        }
        let mut element = build(bounds);
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

fn build_plot(input: PlotInput, bounds: Bounds<Pixels>, height: f64) -> AnyElement {
    let PlotInput {
        id,
        data,
        x_key,
        y_keys,
        margin,
        y_baseline,
        y_domain,
        x_domain,
        label,
        children,
    } = input;
    let width = f64::from(bounds.size.width);
    let inner_width = (width - margin.left - margin.right).max(0.);
    let inner_height = (height - margin.top - margin.bottom).max(0.);
    let (x_scale, y_scale) = chart_scales(
        &data,
        &x_key,
        &y_keys,
        inner_width,
        inner_height,
        y_baseline,
        y_domain,
        x_domain,
    );
    #[cfg(test)]
    CHARTS.with(|c| {
        c.borrow_mut().entry(id.to_string()).or_default().plot = Some((inner_width, inner_height))
    });
    let (ml, mt) = (px(margin.left as f32), px(margin.top as f32));
    let ctx = ChartContext {
        id,
        width: inner_width,
        height: inner_height,
        margin,
        x_key,
        data,
        x_scale,
        y_scale,
        origin: Rc::new(Cell::new(bounds.origin + point(ml, mt))),
        overlays: Rc::default(),
        portal: Rc::default(),
    };
    let overlays = ctx.overlays.clone();
    let portal = ContainerPortal {
        items: ctx.portal.clone(),
        child: None,
    };
    let (iw, ih) = (px(inner_width as f32), px(inner_height as f32));
    let layer = div()
        .absolute()
        .left(ml)
        .top(mt)
        .w(iw)
        .h(ih)
        .children(
            children
                .into_iter()
                .map(|child| div().absolute().top_0().left_0().size_full().child(child)),
        )
        .child(
            canvas(
                |_, _, _| {},
                move |bounds: Bounds<Pixels>, _, window, cx| {
                    let paints: Vec<_> = overlays.borrow_mut().drain(..).collect();
                    for paint in paints {
                        paint(bounds.origin, window, cx);
                    }
                },
            )
            .absolute()
            .top_0()
            .left_0()
            .size_full(),
        );
    let plot = div()
        .id("lab-chart-plot")
        .relative()
        .size_full()
        .role(Role::Image)
        .aria_label(label)
        .child(layer);
    let container = div().relative().size_full().child(plot).child(portal);
    astryx_primitives::provide(ctx, container).into_any_element()
}

/// The chart's container slot (upstream's portal target, the SVG's parent): it lays out
/// what the plot's children put there while they rendered, which happens before it.
struct ContainerPortal {
    items: Rc<std::cell::RefCell<Vec<AnyElement>>>,
    child: Option<AnyElement>,
}

impl Element for ContainerPortal {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let items: Vec<AnyElement> = self.items.borrow_mut().drain(..).collect();
        let mut child = div()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .children(items)
            .into_any_element();
        let id = child.request_layout(window, cx);
        self.child = Some(child);
        (id, ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        if let Some(child) = self.child.as_mut() {
            child.prepaint(window, cx);
        }
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
