//! `ChartTooltip` (upstream lab `ChartTooltip.tsx`): a hover card with optional crosshair lines,
//! in the top layer.

use std::rc::Rc;

use astryx_charts::{ChartValue, Datum};
use astryx_primitives::{LayerOptions, LayerPosition, Placement, anchored_layer};
use astryx_theme::ThemeStyled;
use astryx_theme::authoring::js::js_to_fixed;
use gpui::prelude::*;
use gpui::{
    AnyElement, App, Bounds, ElementId, FontWeight, Hsla, MouseMoveEvent, Pixels, Role,
    SharedString, Window, canvas, div, point, px,
};

use super::context::{ChartContext, DataPoint, use_chart};
use super::paint::{Anchor, Baseline, fill_rect, paint_text, stroke_circle, stroke_segment};
use super::utils::x_pixel;

/// The crosshair's lines (upstream `ChartCrosshairMode`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ChartCrosshairMode {
    /// `'x'`: a vertical line (the default).
    #[default]
    X,
    /// `'y'`: a horizontal line.
    Y,
    /// `'xy'`: both.
    Xy,
    /// `false`: no lines, the card only.
    None,
}

/// A custom card (upstream `render`): the nearest datum and its index; `None` hides the card
/// and keeps the crosshair.
pub type TooltipRender = Rc<dyn Fn(&Datum, usize, &mut Window, &mut App) -> Option<AnyElement>>;
/// The x crosshair label's text (upstream `xFormat`).
pub type XFormat = Rc<dyn Fn(Option<&ChartValue>) -> SharedString>;
/// The y crosshair label's text (upstream `yFormat`).
pub type YFormat = Rc<dyn Fn(f64) -> SharedString>;

/// A hover tooltip with a crosshair (upstream lab `ChartTooltip`).
#[derive(IntoElement)]
pub struct ChartTooltip {
    render: Option<TooltipRender>,
    crosshair: ChartCrosshairMode,
    crosshair_labels: bool,
    snap: bool,
    x_format: Option<XFormat>,
    y_format: Option<YFormat>,
    crosshair_color: Option<Hsla>,
    dot: bool,
}

impl Default for ChartTooltip {
    fn default() -> Self {
        Self::new()
    }
}

impl ChartTooltip {
    /// A snapping tooltip with a vertical crosshair and a dot (upstream's defaults).
    pub fn new() -> Self {
        ChartTooltip {
            render: None,
            crosshair: ChartCrosshairMode::X,
            crosshair_labels: false,
            snap: true,
            x_format: None,
            y_format: None,
            crosshair_color: None,
            dot: true,
        }
    }

    /// Upstream `render`: the card's content (every key as "key: value" by default).
    pub fn render(
        mut self,
        render: impl Fn(&Datum, usize, &mut Window, &mut App) -> Option<AnyElement> + 'static,
    ) -> Self {
        self.render = Some(Rc::new(render));
        self
    }

    /// Upstream `crosshair`.
    pub fn crosshair(mut self, mode: ChartCrosshairMode) -> Self {
        self.crosshair = mode;
        self
    }

    /// Upstream `crosshairLabels`: axis value badges on the crosshair lines (off).
    pub fn crosshair_labels(mut self, labels: bool) -> Self {
        self.crosshair_labels = labels;
        self
    }

    /// Upstream `snap`: snap to the nearest datum (on), or follow the pointer.
    pub fn snap(mut self, snap: bool) -> Self {
        self.snap = snap;
        self
    }

    /// Upstream `xFormat`: the x badge's text (a number to one decimal by default).
    pub fn x_format(
        mut self,
        format: impl Fn(Option<&ChartValue>) -> SharedString + 'static,
    ) -> Self {
        self.x_format = Some(Rc::new(format));
        self
    }

    /// Upstream `yFormat`: the y badge's text (one decimal by default).
    pub fn y_format(mut self, format: impl Fn(f64) -> SharedString + 'static) -> Self {
        self.y_format = Some(Rc::new(format));
        self
    }

    /// Upstream `crosshairColor` (the emphasized border color).
    pub fn crosshair_color(mut self, color: Hsla) -> Self {
        self.crosshair_color = Some(color);
        self
    }

    /// Upstream `dot`: a ring at the snapped point (on).
    pub fn dot(mut self, dot: bool) -> Self {
        self.dot = dot;
        self
    }
}

/// The keys the nearest point is measured on: the first datum's number fields other than
/// the x key, in its order.
fn y_keys(ctx: &ChartContext) -> Vec<SharedString> {
    ctx.data.first().map_or_else(Vec::new, |d| {
        d.fields()
            .filter(|(k, v)| k.as_ref() != ctx.x_key.as_ref() && v.is_number())
            .map(|(k, _)| k.clone())
            .collect()
    })
}

/// The datum nearest a plot position (upstream `findNearest`): by squared distance to each
/// of its number fields' points, and the closest field's value and pixel. `None` without data.
pub fn find_nearest(ctx: &ChartContext, px: f64, py: f64) -> Option<(usize, DataPoint)> {
    if ctx.data.is_empty() {
        return None;
    }
    let keys = y_keys(ctx);
    let (mut closest, mut min_dist) = (0, f64::INFINITY);
    for (i, d) in ctx.data.iter().enumerate() {
        let dx = x_pixel(d, &ctx.x_key, &ctx.x_scale) - px;
        let mut best = f64::INFINITY;
        for k in &keys {
            if let Some(ChartValue::Number(v)) = d.get(k) {
                let dy = ctx.y_scale.scale(*v) - py;
                let dist = dx * dx + dy * dy;
                if dist < best {
                    best = dist;
                }
            }
        }
        if best < min_dist {
            min_dist = best;
            closest = i;
        }
    }
    let d = &ctx.data[closest];
    let dpx = x_pixel(d, &ctx.x_key, &ctx.x_scale);
    let (mut best_y, mut best_py, mut best) = (0., ctx.height / 2., f64::INFINITY);
    for k in &keys {
        if let Some(ChartValue::Number(v)) = d.get(k) {
            let dpy = ctx.y_scale.scale(*v);
            let (dx, dy) = (dpx - px, dpy - py);
            let dist = dx * dx + dy * dy;
            if dist < best {
                best = dist;
                best_y = *v;
                best_py = dpy;
            }
        }
    }
    let x = match d.get(&ctx.x_key) {
        None | Some(ChartValue::Null) => None,
        Some(v) => Some(v.clone()),
    };
    Some((
        closest,
        DataPoint {
            x,
            y: best_y,
            px: dpx,
            py: best_py,
        },
    ))
}

/// The default x badge text: a number to one decimal, anything else as text, blank for
/// none.
fn default_x(value: Option<&ChartValue>) -> SharedString {
    match value {
        None => "".into(),
        Some(ChartValue::Number(n)) => js_to_fixed(*n, 1).into(),
        Some(v) => v.to_js_string(),
    }
}

/// What the tooltip shows: the hovered datum (`None` for a free pointer) and point, and where
/// the card goes in the plot.
#[derive(Clone, Default)]
pub(crate) struct HoverState {
    hover: Option<(Option<usize>, DataPoint)>,
    card_at: (f64, f64),
}

/// The default card (upstream `defaultRender`): every field as "key: value".
pub(crate) fn default_card(d: &Datum, t: &astryx_theme::Tokens) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap(px(2.))
        .text_size(px(12.))
        .children(d.fields().map(|(k, v)| {
            div()
                .flex()
                .gap(px(4.))
                .child(
                    div()
                        .text_color(t.color.text_secondary)
                        .child(format!("{k}:")),
                )
                .child(
                    div()
                        .font_weight(FontWeight::MEDIUM)
                        .child(v.to_js_string()),
                )
        }))
        .into_any_element()
}

/// The card's surface (upstream's popover-colored, bordered, shadowed box).
pub(crate) fn card_surface(
    id: ElementId,
    t: &astryx_theme::Tokens,
    content: AnyElement,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .role(Role::Tooltip)
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
        .child(content)
}

/// A layer whose top-left corner is the trigger's, as upstream's fixed layer is placed at
/// the pointer.
pub(crate) fn at_trigger(
    anchor: Bounds<Pixels>,
    _: gpui::Size<Pixels>,
    _: Bounds<Pixels>,
    _: &LayerOptions,
) -> LayerPosition {
    LayerPosition {
        origin: anchor.origin,
        side: Placement::Below,
    }
}

impl RenderOnce for ChartTooltip {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let Some(ctx) = use_chart(cx) else {
            return div().into_any_element();
        };
        let t = astryx_theme::current(window, cx).tokens().clone();
        let crosshair = self.crosshair_color.unwrap_or(t.color.border_emphasized);
        let state = window.use_keyed_state(
            ElementId::Name(format!("{}-lab-tooltip", ctx.id).into()),
            cx,
            |_, _| HoverState::default(),
        );
        let hover = state.read(cx).clone();
        let snap = self.snap;
        let (move_state, leave_state, move_ctx) = (state.clone(), state.clone(), ctx.clone());
        let capture = div()
            .id(ElementId::Name(
                format!("{}-lab-tooltip-capture", ctx.id).into(),
            ))
            .debug_selector(|| "lab-chart-tooltip-capture".into())
            .absolute()
            .size_full()
            .occlude()
            .on_mouse_move(move |e: &MouseMoveEvent, _, cx| {
                let (lx, ly) = move_ctx.to_local(e.position);
                let point = move_ctx.pixel_to_data(lx, ly);
                let result = if snap {
                    find_nearest(&move_ctx, point.px, point.py).map(|(i, p)| (Some(i), p))
                } else {
                    Some((None, point))
                };
                if let Some(result) = result {
                    move_state.update(cx, |s, cx| {
                        s.hover = Some(result);
                        s.card_at = (lx + 12., ly - 8.);
                        cx.notify();
                    });
                }
            })
            .on_hover(move |hovered, _, cx| {
                if !*hovered {
                    leave_state.update(cx, |s, cx| {
                        s.hover = None;
                        cx.notify();
                    });
                }
            });

        let (show_x, show_y) = (
            matches!(
                self.crosshair,
                ChartCrosshairMode::X | ChartCrosshairMode::Xy
            ),
            matches!(
                self.crosshair,
                ChartCrosshairMode::Y | ChartCrosshairMode::Xy
            ),
        );
        let (width, height) = (ctx.width, ctx.height);
        let surface = t.color.background_surface;
        let popover = t.color.background_popover;
        let text = t.color.text_primary;
        let x_label = hover.hover.as_ref().map(|(_, p)| match &self.x_format {
            Some(f) => f(p.x.as_ref()),
            None => default_x(p.x.as_ref()),
        });
        let y_label = hover.hover.as_ref().map(|(_, p)| match &self.y_format {
            Some(f) => f(p.y),
            None => js_to_fixed(p.y, 1).into(),
        });
        let (show_labels, show_dot) = (self.crosshair_labels, self.dot && snap);
        let marked = hover.hover.clone();
        let marks = canvas(
            |_, _, _| {},
            move |bounds: Bounds<Pixels>, _, window, cx| {
                let Some((index, p)) = &marked else {
                    return;
                };
                let o = bounds.origin;
                if show_x {
                    stroke_segment(
                        window,
                        o,
                        (p.px, 0.),
                        (p.px, height),
                        1.,
                        crosshair,
                        &[3., 3.],
                    );
                }
                if show_y {
                    stroke_segment(
                        window,
                        o,
                        (0., p.py),
                        (width, p.py),
                        1.,
                        crosshair,
                        &[3., 3.],
                    );
                }
                if show_dot && index.is_some() {
                    stroke_circle(window, o, (p.px, p.py), 4., 2., crosshair, surface);
                }
                if show_labels
                    && show_x
                    && p.x.is_some()
                    && let Some(label) = &x_label
                {
                    fill_rect(
                        window,
                        o,
                        (p.px - 24., height + 2., 48., 16.),
                        4.,
                        popover,
                        Some((0.5, crosshair)),
                    );
                    paint_text(
                        window,
                        cx,
                        o,
                        label,
                        (p.px, height + 14.),
                        Anchor::Middle,
                        Baseline::Alphabetic,
                        10.,
                        None,
                        text,
                    );
                }
                if show_labels
                    && show_y
                    && let Some(label) = &y_label
                {
                    fill_rect(
                        window,
                        o,
                        (-48., p.py - 8., 44., 16.),
                        4.,
                        popover,
                        Some((0.5, crosshair)),
                    );
                    paint_text(
                        window,
                        cx,
                        o,
                        label,
                        (-26., p.py),
                        Anchor::Middle,
                        Baseline::Em(0.35),
                        10.,
                        None,
                        text,
                    );
                }
            },
        )
        .absolute()
        .size_full();

        // The card, in the top layer at the pointer.
        let content = match &hover.hover {
            Some((Some(index), _)) => ctx.data.get(*index).and_then(|d| match &self.render {
                Some(render) => render(d, *index, window, cx),
                None => Some(default_card(d, &t)),
            }),
            _ => None,
        };
        let (cx_at, cy_at) = hover.card_at;
        let trigger = div()
            .absolute()
            .left(px(cx_at as f32))
            .top(px(cy_at as f32))
            .size_0();
        let mut layer = anchored_layer(
            ElementId::Name(format!("{}-lab-tooltip-card", ctx.id).into()),
            trigger,
        )
        .light_dismiss(false)
        .position_with(at_trigger);
        if let Some(content) = content {
            let card = card_surface(
                ElementId::Name(format!("{}-lab-tooltip-surface", ctx.id).into()),
                &t,
                content,
            )
            .debug_selector(|| "lab-chart-tooltip-card".into());
            let dismiss = state.clone();
            layer = layer.surface(card, LayerOptions::default(), move |_, cx| {
                dismiss.update(cx, |s, cx| {
                    s.hover = None;
                    cx.notify();
                });
            });
        }
        let _ = point(px(0.), px(0.));
        div()
            .relative()
            .size_full()
            .child(capture)
            .child(marks)
            .child(layer)
            .into_any_element()
    }
}
