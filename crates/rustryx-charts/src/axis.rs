//! `ChartAxis`: an axis on one edge, from the chart's scales (upstream `ChartAxis`).
//! Tick labels always draw; the edge line and the tick marks are each optional (the
//! bottom axis draws its line by default). Labels use d3's tick format on a continuous
//! scale unless `tick_format` is given, and thin out to fit the axis. Ticks slide and
//! fade (150 ms) as the scale moves, for streaming charts.

use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;

use gpui::prelude::*;
use gpui::{App, Bounds, ElementId, Pixels, Role, SharedString, Window, canvas, div, px};
use rustryx_theme::ThemeStyled;
use unicode_segmentation::UnicodeSegmentation;
use web_time::Instant;

use crate::ChartValue;
use crate::context::{ChartContext, use_chart};
use crate::grid::safe_tick_count;
use crate::label::{Anchor, paint_label};
use crate::marks::stroke_segment;
use crate::types::ChartScale;

/// The tick mark's length.
const TICK_SIZE: f64 = 6.;
/// The labels' font size.
const LABEL_SIZE: f64 = 12.;
/// How long a tick slides and fades.
const TICK_TRANSITION: Duration = Duration::from_millis(150);

/// Which edge the axis draws on (upstream `position`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AxisPosition {
    Top,
    Right,
    Bottom,
    Left,
}

impl AxisPosition {
    fn name(self) -> &'static str {
        match self {
            AxisPosition::Top => "top",
            AxisPosition::Right => "right",
            AxisPosition::Bottom => "bottom",
            AxisPosition::Left => "left",
        }
    }

    fn is_horizontal(self) -> bool {
        matches!(self, AxisPosition::Top | AxisPosition::Bottom)
    }
}

/// A tick label format (upstream `tickFormat`): `None` shows the default text.
pub type TickFormat = Rc<dyn Fn(&ChartValue) -> Option<SharedString>>;

/// A chart axis (upstream `ChartAxis`).
///
/// ```ignore
/// ChartAxis::new(AxisPosition::Bottom)                    // edge line and labels
/// ChartAxis::new(AxisPosition::Left)                      // labels only
/// ChartAxis::new(AxisPosition::Left).show_axis_line(true).show_ticks(true)
/// ```
#[derive(IntoElement, Clone)]
pub struct ChartAxis {
    position: AxisPosition,
    tick_count: f64,
    max_ticks: Option<usize>,
    tick_format: Option<TickFormat>,
    truncate: Option<usize>,
    animated: bool,
    show_axis_line: Option<bool>,
    show_ticks: bool,
}

impl ChartAxis {
    /// An axis on `position` (upstream `position`).
    pub fn new(position: AxisPosition) -> Self {
        ChartAxis {
            position,
            tick_count: 5.,
            max_ticks: None,
            tick_format: None,
            truncate: None,
            animated: true,
            show_axis_line: None,
            show_ticks: false,
        }
    }

    /// Upstream `tickCount`: about how many ticks (d3 picks the final count); 5 by default.
    pub fn tick_count(mut self, count: f64) -> Self {
        self.tick_count = count;
        self
    }

    /// Upstream `maxTicks`: at most this many labels, the first and last kept. Unset,
    /// labels thin to fit the axis; set, it wins, so labels may overlap when narrow.
    pub fn max_ticks(mut self, max: usize) -> Self {
        self.max_ticks = Some(max);
        self
    }

    /// Upstream `tickFormat`: a label for a tick value, `None` for the default text.
    pub fn tick_format(
        mut self,
        format: impl Fn(&ChartValue) -> Option<SharedString> + 'static,
    ) -> Self {
        self.tick_format = Some(Rc::new(format));
        self
    }

    /// Upstream `truncate`: labels cut to this many user-perceived characters, with "…".
    pub fn truncate(mut self, characters: usize) -> Self {
        self.truncate = Some(characters);
        self
    }

    /// Upstream `animated`: ticks slide as the scale moves (on by default).
    pub fn animated(mut self, animated: bool) -> Self {
        self.animated = animated;
        self
    }

    /// Upstream `showAxisLine`: the edge line, on by default for the bottom axis only,
    /// and always on with tick marks.
    pub fn show_axis_line(mut self, show: bool) -> Self {
        self.show_axis_line = Some(show);
        self
    }

    /// Upstream `showTicks`: tick marks at each label (off by default).
    pub fn show_ticks(mut self, show: bool) -> Self {
        self.show_ticks = show;
        self
    }
}

/// Keeps at most `cap` ticks: every `step`-th from the first, plus the last (the data's
/// end, often the newest value), which replaces the kept tick before it when that one
/// is closer than a step (upstream `thinTicks`).
pub(crate) fn thin_ticks<T: Clone>(ticks: &[T], cap: usize) -> Vec<T> {
    if ticks.len() <= cap {
        return ticks.to_vec();
    }
    if cap < 2 {
        return ticks[..cap].to_vec();
    }
    let step = ticks.len().div_ceil(cap);
    let last = ticks.len() - 1;
    let mut kept: Vec<T> = ticks
        .iter()
        .enumerate()
        .filter(|(i, _)| i % step == 0 && last - i >= step)
        .map(|(_, t)| t.clone())
        .collect();
    kept.push(ticks[last].clone());
    kept
}

/// Upstream `characterCount`: user-perceived characters.
fn character_count(s: &str) -> usize {
    s.graphemes(true).count()
}

/// `s` cut to `max` user-perceived characters, with "…" when it was cut.
fn truncate_label(s: SharedString, max: Option<usize>) -> SharedString {
    match max {
        Some(max) if max > 0 && character_count(&s) > max => {
            let cut: String = s.graphemes(true).take(max).collect();
            format!("{cut}\u{2026}").into()
        }
        _ => s,
    }
}

/// One labelled tick (upstream's `{value, offset, label}`): its value's key, where it
/// sits along the axis and its label.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Tick {
    pub key: SharedString,
    pub offset: f64,
    pub label: SharedString,
}

impl ChartAxis {
    /// Whether the edge line draws: `show_axis_line` (the bottom axis by default),
    /// forced on by tick marks, which need a line to stand on.
    pub(crate) fn draws_axis_line(&self) -> bool {
        self.show_axis_line
            .unwrap_or(self.position == AxisPosition::Bottom)
            || self.show_ticks
    }

    /// The labelled ticks of `chart`'s scale, formatted once each and thinned to fit
    /// (upstream's `ticks`).
    pub(crate) fn ticks(&self, chart: &ChartContext) -> Vec<Tick> {
        let horizontal = self.position.is_horizontal();
        let count = safe_tick_count(self.tick_count);
        let (width, height) = (chart.width, chart.height);
        // A categorical y (a heatmap's rows) shows its categories, not value ticks.
        let band = if horizontal {
            chart.x_scale.as_band().cloned()
        } else {
            chart.y_band_scale.clone()
        };
        let linear = if horizontal {
            match &chart.x_scale {
                ChartScale::Linear(l) => Some(*l),
                ChartScale::Band(_) => None,
            }
        } else {
            Some(chart.y_scale)
        };
        let format =
            |value: &ChartValue, auto: &dyn Fn(&ChartValue) -> SharedString| -> SharedString {
                let text = self
                    .tick_format
                    .as_ref()
                    .and_then(|f| f(value))
                    .unwrap_or_else(|| auto(value));
                truncate_label(text, self.truncate)
            };
        let mut ticks: Vec<Tick> = match (&band, &linear) {
            (Some(band), _) => band
                .get_domain()
                .iter()
                .map(|d| {
                    let value = ChartValue::Text(d.clone());
                    Tick {
                        key: d.clone(),
                        offset: band.scale(d).unwrap_or(0.) + band.bandwidth() / 2.,
                        label: format(&value, &|v| v.to_js_string()),
                    }
                })
                .collect(),
            (None, Some(linear)) => {
                // d3's own tick format: sensible precision, no floating-point dust.
                let auto = linear.tick_format(count as f64, None);
                linear
                    .ticks(count as f64)
                    .into_iter()
                    .map(|d| {
                        let value = ChartValue::Number(d);
                        Tick {
                            key: value.to_js_string(),
                            offset: linear.scale(d),
                            label: format(&value, &|v| auto.format(v.to_number()).into()),
                        }
                    })
                    .collect()
            }
            (None, None) => Vec::new(),
        };

        // Cap the label count to avoid overlap. An explicit `max_ticks` always wins;
        // otherwise the cap comes from the room and the label size: the width over
        // the widest label (horizontal), the height over a line (vertical). The first
        // and last labels stay.
        let mut cap = self.max_ticks;
        if cap.is_none() && ticks.len() > 1 {
            if horizontal && width > 0. {
                let widest = ticks
                    .iter()
                    .fold(1, |m, t| m.max(character_count(&t.label)));
                let approx = widest as f64 * 7. + 16.;
                cap = Some(((width / approx).floor() as usize).max(1));
            } else if !horizontal && height > 0. {
                // A 12 px glyph and breathing room keep stacked labels apart.
                cap = Some(((height / 18.).floor() as usize).max(1));
            }
        }
        if let Some(cap) = cap
            && cap > 0
            && ticks.len() > cap
        {
            ticks = thin_ticks(&ticks, cap);
        }
        ticks
    }
}

/// Where an axis' own origin sits in the plot (upstream's `transform`): the bottom
/// edge, the right edge, or the plot's origin.
pub(crate) fn axis_origin(position: AxisPosition, width: f64, height: f64) -> (f64, f64) {
    match position {
        AxisPosition::Bottom => (0., height),
        AxisPosition::Right => (width, 0.),
        _ => (0., 0.),
    }
}

/// The y of the bottom axis' line, from its origin (upstream `axisLineY`): at the
/// zero line when the domain spans it, so the line still stands for zero. A
/// categorical y has no zero: its line frames the plot's edge.
pub(crate) fn axis_line_y(position: AxisPosition, chart: &ChartContext) -> f64 {
    if position != AxisPosition::Bottom || chart.y_band_scale.is_some() {
        return 0.;
    }
    let [d0, d1] = chart.y_scale.get_domain();
    if d0 < 0. && d1 > 0. {
        chart.y_scale.scale(0.) - chart.height
    } else {
        0.
    }
}

/// The axis' edge line from its origin: across the plot, or down it.
pub(crate) fn axis_line(
    position: AxisPosition,
    width: f64,
    height: f64,
    line_y: f64,
) -> ((f64, f64), (f64, f64)) {
    if position.is_horizontal() {
        ((0., line_y), (width, line_y))
    } else {
        ((0., 0.), (0., height))
    }
}

/// A tick's mark from the axis' origin, outward from the plot.
pub(crate) fn tick_mark(position: AxisPosition, offset: f64) -> ((f64, f64), (f64, f64)) {
    match position {
        AxisPosition::Bottom => ((offset, 0.), (offset, TICK_SIZE)),
        AxisPosition::Top => ((offset, 0.), (offset, -TICK_SIZE)),
        AxisPosition::Left => ((0., offset), (-TICK_SIZE, offset)),
        AxisPosition::Right => ((0., offset), (TICK_SIZE, offset)),
    }
}

/// A tick label's point from the axis' origin, its x and baseline, and its anchor
/// (upstream's `<text>` x, y and `text-anchor`): centered below or above a
/// horizontal axis, beside a vertical one.
pub(crate) fn label_place(position: AxisPosition, offset: f64) -> ((f64, f64), Anchor) {
    match position {
        AxisPosition::Bottom => ((offset, TICK_SIZE + 12.), Anchor::Middle),
        AxisPosition::Top => ((offset, -(TICK_SIZE + 4.)), Anchor::Middle),
        // SVG `dy="0.32em"` centers the label on its tick.
        AxisPosition::Left => ((-(TICK_SIZE + 4.), offset + 0.32 * LABEL_SIZE), Anchor::End),
        AxisPosition::Right => ((TICK_SIZE + 4., offset + 0.32 * LABEL_SIZE), Anchor::Start),
    }
}

/// A tick's motion: from where it showed toward where it sits now.
#[derive(Clone, Copy)]
struct TickMotion {
    from: f64,
    to: f64,
    start: Instant,
    opacity_from: f64,
    opacity_to: f64,
    opacity_start: Instant,
}

impl TickMotion {
    fn offset(&self, now: Instant) -> f64 {
        let p = (now.saturating_duration_since(self.start).as_secs_f64()
            / TICK_TRANSITION.as_secs_f64())
        .min(1.);
        self.from + (self.to - self.from) * p
    }

    fn opacity(&self, now: Instant) -> f64 {
        let p = (now
            .saturating_duration_since(self.opacity_start)
            .as_secs_f64()
            / TICK_TRANSITION.as_secs_f64())
        .min(1.);
        // CSS `ease`.
        let eased = rustryx_primitives::motion::cubic_bezier([0.25, 0.1, 0.25, 1.]).ease(p);
        self.opacity_from + (self.opacity_to - self.opacity_from) * eased
    }

    fn moving(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.start) < TICK_TRANSITION
            || now.saturating_duration_since(self.opacity_start) < TICK_TRANSITION
    }
}

/// The axis' ticks across frames, by tick key.
#[derive(Default)]
struct AxisState {
    ticks: HashMap<SharedString, TickMotion>,
}

impl RenderOnce for ChartAxis {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let Some(chart) = use_chart(cx) else {
            return div().into_any_element();
        };
        let theme = rustryx_theme::current(window, cx);
        let t = theme.tokens();
        let line_color = t.color.text_primary;
        let label_color = t.color.text_secondary;
        let position = self.position;
        let horizontal = position.is_horizontal();
        let draws_line = self.draws_axis_line();
        let (width, height) = (chart.width, chart.height);
        let ticks = self.ticks(&chart);
        let line = axis_line(position, width, height, axis_line_y(position, &chart));

        // Each tick slides from where it showed (a new tick appears in place).
        let now = cx.background_executor().now();
        let state = window.use_keyed_state((chart.id.clone(), position.name()), cx, |_, _| {
            AxisState::default()
        });
        let span = if horizontal { width } else { height };
        let placed: Vec<(Tick, f64, f64)> = state.update(cx, |state, _| {
            let mut next = HashMap::new();
            let placed = ticks
                .into_iter()
                .map(|tick| {
                    let visible = if tick.offset >= -10. && tick.offset <= span + 10. {
                        1.
                    } else {
                        0.
                    };
                    let settled = now.checked_sub(TICK_TRANSITION).unwrap_or(now);
                    let motion = match state.ticks.get(&tick.key).copied() {
                        Some(prev) if self.animated => {
                            let mut m = prev;
                            // A moved tick slides from where it shows now.
                            if prev.to != tick.offset {
                                m.from = prev.offset(now);
                                m.to = tick.offset;
                                m.start = now;
                            }
                            if prev.opacity_to != visible {
                                m.opacity_from = prev.opacity(now);
                                m.opacity_to = visible;
                                m.opacity_start = now;
                            }
                            m
                        }
                        _ => TickMotion {
                            from: tick.offset,
                            to: tick.offset,
                            start: settled,
                            opacity_from: visible,
                            opacity_to: visible,
                            opacity_start: settled,
                        },
                    };
                    next.insert(tick.key.clone(), motion);
                    let shown = motion.offset(now);
                    let opacity = motion.opacity(now);
                    (tick, shown, opacity)
                })
                .collect();
            state.ticks = next;
            placed
        });
        let animating = state.read(cx).ticks.values().any(|m| m.moving(now));
        if animating {
            window.request_animation_frame();
        }

        let show_ticks = self.show_ticks;
        let shapes = canvas(
            |_, _, _| {},
            move |bounds: Bounds<Pixels>, _, window, cx| {
                let (ox, oy) = axis_origin(position, width, height);
                let o = bounds.origin + gpui::point(px(ox as f32), px(oy as f32));
                if draws_line {
                    stroke_segment(window, o, line.0, line.1, 1., line_color, &[]);
                }
                for (tick, offset, opacity) in &placed {
                    let opacity = *opacity as f32;
                    if opacity <= 0. {
                        continue;
                    }
                    if show_ticks {
                        let (from, to) = tick_mark(position, *offset);
                        stroke_segment(window, o, from, to, 1., line_color.opacity(opacity), &[]);
                    }
                    let (at, anchor) = label_place(position, *offset);
                    paint_label(
                        window,
                        cx,
                        o,
                        &tick.label,
                        at,
                        anchor,
                        LABEL_SIZE,
                        label_color.opacity(opacity),
                    );
                }
            },
        )
        .size_full();
        div()
            .id(ElementId::Name(
                format!("chart-axis-{}", position.name()).into(),
            ))
            .role(Role::Group)
            .aria_label(axis_label(position))
            .size_full()
            .font_stack(&t.font.family.body)
            .child(shapes)
            .into_any_element()
    }
}

/// The axis group's accessible name (upstream `aria-label`): "bottom axis".
pub(crate) fn axis_label(position: AxisPosition) -> SharedString {
    format!("{} axis", position.name()).into()
}
