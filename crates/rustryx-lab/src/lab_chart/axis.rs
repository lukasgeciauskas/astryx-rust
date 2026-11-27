//! `ChartAxis` (upstream lab `ChartAxis.tsx`): an axis on one edge of the plot, its ticks
//! sliding smoothly when the domain shifts (streaming).

use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;

use gpui::prelude::*;
use gpui::{App, Bounds, ElementId, Pixels, SharedString, Window, canvas, div, point, px};
use rustryx_charts::{ChartScale, ChartValue};
use rustryx_theme::{CubicBezier, ThemeStyled};
use web_time::Instant;

use super::context::{ChartContext, use_chart};
use super::paint::{Anchor, Baseline, paint_text, stroke_segment};

/// Which edge an axis is on (upstream `position`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AxisPosition {
    Top,
    Right,
    Bottom,
    Left,
}

impl AxisPosition {
    fn is_horizontal(self) -> bool {
        matches!(self, AxisPosition::Top | AxisPosition::Bottom)
    }

    fn name(self) -> &'static str {
        match self {
            AxisPosition::Top => "top",
            AxisPosition::Right => "right",
            AxisPosition::Bottom => "bottom",
            AxisPosition::Left => "left",
        }
    }
}

/// A tick's text from its value (upstream `tickFormat`: `(value: unknown) => string`).
pub type TickFormat = Rc<dyn Fn(&ChartValue) -> SharedString>;

/// How long a tick slides and fades (upstream's `transform 150ms linear, opacity 150ms ease`).
const TICK_TRANSITION: Duration = Duration::from_millis(150);
/// The tick marks' length (upstream `tickSize`).
const TICK_SIZE: f64 = 6.;
const LABEL_SIZE: f64 = 12.;

/// An axis (upstream lab `ChartAxis`): an axis line, tick marks and labels for x (top,
/// bottom) or y (left, right).
#[derive(IntoElement)]
pub struct ChartAxis {
    position: AxisPosition,
    tick_count: f64,
    max_ticks: Option<usize>,
    tick_format: Option<TickFormat>,
    truncate: Option<usize>,
    animated: bool,
}

impl ChartAxis {
    /// Upstream `position`.
    pub fn new(position: AxisPosition) -> Self {
        ChartAxis {
            position,
            tick_count: 5.,
            max_ticks: None,
            tick_format: None,
            truncate: None,
            animated: true,
        }
    }

    /// Upstream `tickCount`: about how many ticks a linear scale gets (5; d3 decides).
    pub fn tick_count(mut self, count: f64) -> Self {
        self.tick_count = count;
        self
    }

    /// Upstream `maxTicks`: at most this many labels, skipping evenly past it.
    pub fn max_ticks(mut self, max: usize) -> Self {
        self.max_ticks = Some(max);
        self
    }

    /// Upstream `tickFormat` (`String` by default).
    pub fn tick_format(mut self, format: impl Fn(&ChartValue) -> SharedString + 'static) -> Self {
        self.tick_format = Some(Rc::new(format));
        self
    }

    /// Upstream `truncate`: labels longer than this many characters are cut and end in "…".
    pub fn truncate(mut self, length: usize) -> Self {
        self.truncate = Some(length);
        self
    }

    /// Upstream `animated`: ticks slide when the domain shifts (on).
    pub fn animated(mut self, animated: bool) -> Self {
        self.animated = animated;
        self
    }
}

/// One tick: its value and where it falls along the axis.
#[derive(Clone, Debug, PartialEq)]
pub struct AxisTick {
    pub value: ChartValue,
    pub offset: f64,
}

/// The axis's ticks (upstream's `ticks` memo): each band's center, or the linear scale's
/// ticks; with `max_ticks` exceeded, every `ceil(n / max)`th one.
pub fn axis_ticks(
    ctx: &ChartContext,
    position: AxisPosition,
    tick_count: f64,
    max_ticks: Option<usize>,
) -> Vec<AxisTick> {
    let mut ticks: Vec<AxisTick> = if position.is_horizontal() {
        match &ctx.x_scale {
            ChartScale::Band(band) => band
                .get_domain()
                .iter()
                .map(|d| AxisTick {
                    value: ChartValue::Text(d.clone()),
                    offset: band.scale(d).unwrap_or(0.) + band.bandwidth() / 2.,
                })
                .collect(),
            ChartScale::Linear(linear) => linear
                .ticks(tick_count)
                .into_iter()
                .map(|t| AxisTick {
                    value: ChartValue::Number(t),
                    offset: linear.scale(t),
                })
                .collect(),
        }
    } else {
        ctx.y_scale
            .ticks(tick_count)
            .into_iter()
            .map(|t| AxisTick {
                value: ChartValue::Number(t),
                offset: ctx.y_scale.scale(t),
            })
            .collect()
    };
    if let Some(max) = max_ticks.filter(|m| *m > 0)
        && ticks.len() > max
    {
        let step = ticks.len().div_ceil(max);
        ticks = ticks
            .into_iter()
            .enumerate()
            .filter(|(i, _)| i % step == 0)
            .map(|(_, t)| t)
            .collect();
    }
    ticks
}

/// A label cut to `truncate` characters plus "…" when it is longer (upstream's `slice`,
/// counting UTF-16 units as JavaScript does); a `truncate` of 0 leaves it.
pub fn truncate_label(label: &str, truncate: Option<usize>) -> SharedString {
    match truncate.filter(|t| *t > 0) {
        Some(t) if label.encode_utf16().count() > t => {
            let units: Vec<u16> = label.encode_utf16().take(t).collect();
            format!("{}\u{2026}", String::from_utf16_lossy(&units)).into()
        }
        _ => label.to_string().into(),
    }
}

/// Where the bottom axis's line goes: at the zero line when the y domain spans zero
/// (upstream's `axisLineY`), relative to the plot's bottom edge.
pub fn axis_line_y(ctx: &ChartContext, position: AxisPosition) -> f64 {
    if position != AxisPosition::Bottom {
        return 0.;
    }
    let [d0, d1] = ctx.y_scale.get_domain();
    if d0 < 0. && d1 > 0. {
        ctx.y_scale.scale(0.) - ctx.height
    } else {
        0.
    }
}

#[derive(Clone, Copy, Debug)]
struct TickMotion {
    from: f64,
    to: f64,
    start: Instant,
    opacity_from: f64,
    opacity_to: f64,
    opacity_start: Instant,
}

impl TickMotion {
    fn progress(start: Instant, now: Instant) -> f64 {
        (now.saturating_duration_since(start).as_secs_f64() / TICK_TRANSITION.as_secs_f64()).min(1.)
    }

    fn offset(&self, now: Instant) -> f64 {
        self.from + (self.to - self.from) * Self::progress(self.start, now)
    }

    fn opacity(&self, now: Instant) -> f64 {
        let eased = f64::from(
            CubicBezier(0.25, 0.1, 0.25, 1.).ease(Self::progress(self.opacity_start, now) as f32),
        );
        self.opacity_from + (self.opacity_to - self.opacity_from) * eased
    }

    fn moving(&self, now: Instant) -> bool {
        Self::progress(self.start, now) < 1. || Self::progress(self.opacity_start, now) < 1.
    }
}

#[derive(Default)]
struct AxisState {
    ticks: HashMap<SharedString, TickMotion>,
}

impl RenderOnce for ChartAxis {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let Some(ctx) = use_chart(cx) else {
            return div().into_any_element();
        };
        let theme = rustryx_theme::current(window, cx);
        let t = theme.tokens().clone();
        let line_color = t.color.border_emphasized;
        let label_color = t.color.text_secondary;
        let position = self.position;
        let horizontal = position.is_horizontal();
        let (width, height) = (ctx.width, ctx.height);
        let line_y = axis_line_y(&ctx, position);
        let format = self.tick_format.clone();
        let truncate = self.truncate;
        let ticks = axis_ticks(&ctx, position, self.tick_count, self.max_ticks);
        let span = if horizontal { width } else { height };

        // Each tick, keyed by its label, slides from where it shows to its new offset.
        let now = cx.background_executor().now();
        let state = window.use_keyed_state(
            ElementId::Name(format!("{}-lab-axis-{}", ctx.id, position.name()).into()),
            cx,
            |_, _| AxisState::default(),
        );
        let animated = self.animated;
        let placed: Vec<(SharedString, f64, f64)> = state.update(cx, |state, _| {
            let mut next = HashMap::new();
            let settled = now.checked_sub(TICK_TRANSITION).unwrap_or(now);
            let placed = ticks
                .into_iter()
                .map(|tick| {
                    let raw = match &format {
                        Some(f) => f(&tick.value),
                        None => tick.value.to_js_string(),
                    };
                    let label = truncate_label(&raw, truncate);
                    let visible = if tick.offset >= -10. && tick.offset <= span + 10. {
                        1.
                    } else {
                        0.
                    };
                    let motion = match state.ticks.get(&label).copied() {
                        Some(prev) if animated => {
                            let mut m = prev;
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
                    next.insert(label.clone(), motion);
                    (label, motion.offset(now), motion.opacity(now))
                })
                .collect();
            state.ticks = next;
            placed
        });
        if state.read(cx).ticks.values().any(|m| m.moving(now)) {
            window.request_animation_frame();
        }
        let shapes = canvas(
            |_, _, _| {},
            move |bounds: Bounds<Pixels>, _, window, cx| {
                // The axis group's translate: the bottom axis at the plot's bottom, the right
                // one at its right.
                let (ox, oy) = match position {
                    AxisPosition::Bottom => (0., height),
                    AxisPosition::Right => (width, 0.),
                    _ => (0., 0.),
                };
                let o = bounds.origin + point(px(ox as f32), px(oy as f32));
                if horizontal {
                    stroke_segment(
                        window,
                        o,
                        (0., line_y),
                        (width, line_y),
                        1.,
                        line_color,
                        &[],
                    );
                } else {
                    stroke_segment(window, o, (0., 0.), (0., height), 1., line_color, &[]);
                }
                for (label, offset, opacity) in &placed {
                    let opacity = *opacity as f32;
                    if opacity <= 0. {
                        continue;
                    }
                    if horizontal {
                        let tick_y = if position == AxisPosition::Bottom {
                            TICK_SIZE
                        } else {
                            -TICK_SIZE
                        };
                        stroke_segment(
                            window,
                            o,
                            (*offset, 0.),
                            (*offset, tick_y),
                            1.,
                            line_color.opacity(opacity),
                            &[],
                        );
                        let text_y = if position == AxisPosition::Bottom {
                            TICK_SIZE + 12.
                        } else {
                            -(TICK_SIZE + 4.)
                        };
                        paint_text(
                            window,
                            cx,
                            o,
                            label,
                            (*offset, text_y),
                            Anchor::Middle,
                            Baseline::Alphabetic,
                            LABEL_SIZE,
                            None,
                            label_color.opacity(opacity),
                        );
                    } else {
                        let tick_x = if position == AxisPosition::Left {
                            -TICK_SIZE
                        } else {
                            TICK_SIZE
                        };
                        stroke_segment(
                            window,
                            o,
                            (0., *offset),
                            (tick_x, *offset),
                            1.,
                            line_color.opacity(opacity),
                            &[],
                        );
                        let (text_x, anchor) = if position == AxisPosition::Left {
                            (-(TICK_SIZE + 4.), Anchor::End)
                        } else {
                            (TICK_SIZE + 4., Anchor::Start)
                        };
                        paint_text(
                            window,
                            cx,
                            o,
                            label,
                            (text_x, *offset),
                            anchor,
                            Baseline::Em(0.32),
                            LABEL_SIZE,
                            None,
                            label_color.opacity(opacity),
                        );
                    }
                }
            },
        )
        .size_full();
        div()
            .size_full()
            .font_stack(&t.font.family.body)
            .child(shapes)
            .into_any_element()
    }
}
