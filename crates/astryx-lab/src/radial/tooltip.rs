//! `RadialTooltip`: the hover tooltip of a radial chart (upstream lab
//! `RadialTooltip`). It hit-tests the pointer against the slices (pie, donut) or the
//! nearest axis (spider), draws the tooltip in a layer beside the pointer, and pushes a
//! ring out along a hovered slice.
//!
//! A mouse hovers. A finger scrubs only in an `interactive` chart: the touch that lands
//! on the chart shows the tooltip and is kept from scrolling, drags update it and the
//! lift hides it; elsewhere a finger is left alone.

use std::f64::consts::PI;
use std::rc::Rc;

use astryx_charts::ChartValue;
use astryx_primitives::layer;
use astryx_theme::ThemeStyled;
use gpui::prelude::*;
use gpui::{
    Anchor, AnyElement, App, Bounds, DispatchPhase, ElementId, HitboxBehavior, MouseMoveEvent,
    Pixels, Point, SharedString, TouchPhase, Window, canvas, div, point, px, relative,
};

use super::marks::stroke_circle;
use super::trig::{js_cos, js_sin};
use super::{RadialContext, RadialMode, use_radial};

/// The hovered slice or axis (upstream `RadialTooltipDatum`).
#[derive(Clone, Debug, PartialEq)]
pub struct RadialTooltipDatum {
    /// The slice's key or the axis's name.
    pub label: SharedString,
    /// The numeric value.
    pub value: f64,
    /// Pie and donut: the share of the total.
    pub percentage: Option<f64>,
    /// The row's index in the dataset.
    pub index: usize,
}

/// Upstream `hitTestPie`: the slice under a plot point, by its distance from the
/// center (between the inner and outer radii) and its angle.
pub fn hit_test_pie(ctx: &RadialContext, x: f64, y: f64) -> Option<RadialTooltipDatum> {
    let slices = ctx.slices.as_ref().filter(|s| !s.is_empty())?;
    let (dx, dy) = (x - ctx.cx, y - ctx.cy);
    let dist = (dx * dx + dy * dy).sqrt();
    if dist > ctx.radius || dist < ctx.inner_radius {
        return None;
    }
    let mut angle = dy.atan2(dx);
    if angle < -PI / 2. {
        angle += 2. * PI;
    }
    slices
        .iter()
        .enumerate()
        .find(|(_, s)| angle >= s.start_angle && angle <= s.end_angle)
        .map(|(i, s)| RadialTooltipDatum {
            label: s.key.clone(),
            value: s.value,
            percentage: Some(s.percentage),
            index: i,
        })
}

/// Upstream `hitTestSpider`: the axis nearest a plot point's angle (within half the
/// gap to the next, and 20 px past the outer ring), with the row whose value on it is
/// largest.
pub fn hit_test_spider(ctx: &RadialContext, x: f64, y: f64) -> Option<RadialTooltipDatum> {
    let (axes, angles) = (ctx.axes.as_ref()?, ctx.angle_by_axis.as_ref()?);
    ctx.axis_domains.as_ref()?;
    let (dx, dy) = (x - ctx.cx, y - ctx.cy);
    let dist = (dx * dx + dy * dy).sqrt();
    if dist > ctx.radius + 20. {
        return None;
    }
    let angle = dy.atan2(dx);
    let mut nearest = axes.first()?.clone();
    let mut min_diff = f64::INFINITY;
    for (key, axis_angle) in angles {
        let mut diff = (angle - axis_angle).abs();
        if diff > PI {
            diff = 2. * PI - diff;
        }
        if diff < min_diff {
            min_diff = diff;
            nearest = key.clone();
        }
    }
    if min_diff > PI / axes.len() as f64 {
        return None;
    }
    let (mut best_index, mut best_value) = (0, 0.);
    for (i, d) in ctx.data.iter().enumerate() {
        if let Some(ChartValue::Number(v)) = d.get(&nearest)
            && *v > best_value
        {
            best_value = *v;
            best_index = i;
        }
    }
    Some(RadialTooltipDatum {
        label: nearest,
        value: best_value,
        percentage: None,
        index: best_index,
    })
}

type RenderFn = Rc<dyn Fn(&RadialTooltipDatum, &mut Window, &mut App) -> AnyElement>;

/// The tooltip of a radial chart (upstream lab `RadialTooltip`).
#[derive(IntoElement)]
pub struct RadialTooltip {
    render: Option<RenderFn>,
    highlight: bool,
    highlight_offset: f64,
}

impl Default for RadialTooltip {
    fn default() -> Self {
        Self::new()
    }
}

impl RadialTooltip {
    /// A tooltip with the default content (label, value and share).
    pub fn new() -> Self {
        RadialTooltip {
            render: None,
            highlight: true,
            highlight_offset: 6.,
        }
    }

    /// Upstream `render`: the tooltip's content for the hovered slice or point.
    pub fn render(
        mut self,
        f: impl Fn(&RadialTooltipDatum, &mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        self.render = Some(Rc::new(f));
        self
    }

    /// Upstream `highlight`: a ring pushed out along the hovered slice (on by default).
    pub fn highlight(mut self, highlight: bool) -> Self {
        self.highlight = highlight;
        self
    }

    /// Upstream `highlightOffset`: how far the ring moves out, px (6 by default).
    pub fn highlight_offset(mut self, offset: f64) -> Self {
        self.highlight_offset = offset;
        self
    }
}

/// The tooltip's state.
#[derive(Default)]
struct HoverState {
    hover: Option<RadialTooltipDatum>,
    /// Where the tooltip's corner goes, window px.
    at: Point<Pixels>,
    /// A finger is scrubbing.
    active: bool,
    /// The chart was interactive last frame.
    was_interactive: bool,
}

/// What a hover at `local` (plot px) shows.
fn hit(ctx: &RadialContext, local: (f64, f64)) -> Option<RadialTooltipDatum> {
    match ctx.mode {
        RadialMode::Pie => hit_test_pie(ctx, local.0, local.1),
        RadialMode::Spider => hit_test_spider(ctx, local.0, local.1),
    }
}

impl RenderOnce for RadialTooltip {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let ctx = use_radial(cx);
        let theme = astryx_theme::current(window, cx);
        let t = theme.tokens().clone();
        let state = window.use_keyed_state(ElementId::Name("radial-tooltip".into()), cx, |_, _| {
            HoverState::default()
        });
        // Touch interaction turned off ends an active scrub.
        state.update(cx, |s, cx| {
            if s.was_interactive && !ctx.interactive && s.active {
                s.active = false;
                s.hover = None;
                cx.notify();
            }
            s.was_interactive = ctx.interactive;
        });
        let (hover, tooltip_at) = {
            let s = state.read(cx);
            (s.hover.clone(), s.at)
        };

        // The ring along the hovered slice.
        let ring = hover
            .as_ref()
            .filter(|_| self.highlight && ctx.mode == RadialMode::Pie)
            .and_then(|h| ctx.slices.as_ref()?.get(h.index).cloned())
            .map(|slice| {
                let mid = (slice.start_angle + slice.end_angle) / 2.;
                (
                    ctx.cx + js_cos(mid) * self.highlight_offset,
                    ctx.cy + js_sin(mid) * self.highlight_offset,
                )
            });
        let emphasized = super::marks::faded(t.color.border_emphasized, 0.3);
        let capture_r = ctx.radius + 10.;
        let events = {
            let state = state.clone();
            let ctx = ctx.clone();
            canvas(
                |bounds, window, _| window.insert_hitbox(bounds, HitboxBehavior::Normal),
                move |bounds: Bounds<Pixels>, hitbox, window, _| {
                    if let Some(c) = ring {
                        stroke_circle(window, bounds, c, ctx.radius, 2., emphasized);
                    }
                    let local = move |p: Point<Pixels>| {
                        let d = p - bounds.origin;
                        (f64::from(d.x), f64::from(d.y))
                    };
                    let inside = move |(x, y): (f64, f64)| {
                        let (dx, dy) = (x - ctx.cx, y - ctx.cy);
                        (dx * dx + dy * dy).sqrt() <= capture_r
                    };
                    let show = {
                        let ctx = ctx.clone();
                        move |s: &mut HoverState, position: Point<Pixels>| -> bool {
                            let next = hit(&ctx, local(position));
                            let changed = next != s.hover;
                            s.hover = next;
                            s.at = position + point(px(12.), px(-8.));
                            changed || s.hover.is_some()
                        }
                    };
                    // A mouse hovers the capture circle; leaving it hides the tooltip
                    // unless a finger is scrubbing.
                    {
                        let state = state.clone();
                        let show = show.clone();
                        window.on_mouse_event(move |e: &MouseMoveEvent, phase, window, cx| {
                            if phase != DispatchPhase::Bubble || window.last_input_was_touch() {
                                return;
                            }
                            let over = hitbox.is_hovered(window) && inside(local(e.position));
                            state.update(cx, |s, cx| {
                                if over {
                                    if show(s, e.position) {
                                        cx.notify();
                                    }
                                } else if !s.active && s.hover.is_some() {
                                    s.hover = None;
                                    cx.notify();
                                }
                            });
                        });
                    }
                    // A finger scrubs only while the chart is interactive.
                    let interactive = ctx.interactive;
                    let state = state.clone();
                    window.on_touch_event(move |e, _, cx| {
                        let mut claimed = false;
                        state.update(cx, |s, cx| match e.phase {
                            TouchPhase::Started => {
                                if interactive
                                    && bounds.contains(&e.position)
                                    && inside(local(e.position))
                                {
                                    s.active = true;
                                    show(s, e.position);
                                    claimed = true;
                                    cx.notify();
                                }
                            }
                            TouchPhase::Moved => {
                                if s.active {
                                    show(s, e.position);
                                    claimed = true;
                                    cx.notify();
                                }
                            }
                            TouchPhase::Ended | TouchPhase::Cancelled => {
                                if s.active {
                                    s.active = false;
                                    s.hover = None;
                                    claimed = true;
                                    cx.notify();
                                }
                            }
                        });
                        claimed
                    });
                },
            )
            .absolute()
            .top_0()
            .left_0()
            .size_full()
        };

        let tip = hover.map(|datum| {
            let content = match &self.render {
                Some(render) => render(&datum, window, cx),
                None => default_content(&datum, &t, cx),
            };
            let hide = state.clone();
            layer(
                div()
                    .bg(t.color.background_popover)
                    .border_1()
                    .border_color(t.color.border)
                    .rounded(px(8.))
                    .py(px(8.))
                    .px(px(12.))
                    .shadow_token(&t.shadow.med)
                    .whitespace_nowrap()
                    .font_stack(&t.font.family.body)
                    .text_color(t.color.text_primary)
                    .debug_selector(|| "radial-tooltip".into())
                    .child(content),
                move |_, cx| {
                    hide.update(cx, |s, cx| {
                        s.hover = None;
                        s.active = false;
                        cx.notify();
                    })
                },
            )
            .anchor(tooltip_at, Anchor::TopLeft)
            .light_dismiss(false)
        });
        div()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .child(events)
            .children(tip)
    }
}

/// Upstream's `defaultRender`: the label, the value and, for a slice, its share.
fn default_content(d: &RadialTooltipDatum, t: &astryx_theme::Tokens, cx: &App) -> AnyElement {
    let i18n = astryx_i18n::current(cx);
    let row = |name: &'static str, value: String| {
        div()
            .flex()
            .gap(px(4.))
            .child(div().text_color(t.color.text_secondary).child(name))
            .child(div().font_weight(gpui::FontWeight(500.)).child(value))
    };
    div()
        .flex()
        .flex_col()
        .gap(px(2.))
        .text_size(px(12.))
        .line_height(relative(1.4))
        .child(
            div()
                .font_weight(gpui::FontWeight(600.))
                .child(d.label.clone()),
        )
        .child(row("Value:", i18n.number(d.value)))
        .children(d.percentage.map(|p| {
            row(
                "Share:",
                format!(
                    "{}%",
                    astryx_theme::authoring::js::js_number_to_string(
                        astryx_theme::authoring::js::js_round(p * 100.)
                    )
                ),
            )
        }))
        .into_any_element()
}
