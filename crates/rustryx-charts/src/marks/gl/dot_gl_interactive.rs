//! `dot_gl_interactive`: the WebGL scatter with hover (upstream
//! `marks/dotGLInteractive.tsx`). Upstream picks the hovered point on the GPU from an
//! offscreen buffer; [`PickBuffer`] finds the same point on the CPU. The hovered
//! point gets a halo and an outlined marker, and a card beside the pointer shows its
//! row.

use std::rc::Rc;

use gpui::prelude::*;
use gpui::{
    AnyElement, App, Bounds, DispatchPhase, ElementId, FontWeight, Hitbox, HitboxBehavior, Hsla,
    IntoElement, MouseMoveEvent, Pixels, SharedString, Window, canvas, div, px,
};
use rustryx_theme::ThemeStyled;
use rustryx_theme::authoring::js::js_max;

use super::pick::PickBuffer;
use super::{fill_circle, gl_rgb, resolve_dots, stroke_circle};
use crate::Datum;
use crate::context::use_chart;
use crate::types::{
    GroupInfo, ResolvedPoint, Series, SeriesContext, SeriesDef, SeriesInfo, StackOffset,
};

/// A custom hover card (upstream `renderTooltip`): the hovered row and its index.
/// `None` leaves the card empty, as a React `null` does.
pub type DotGlTooltipRender =
    Rc<dyn Fn(&Datum, usize, &mut Window, &mut App) -> Option<AnyElement>>;

/// An interactive WebGL scatter's options (upstream `DotGLInteractiveOptions`).
#[derive(Clone)]
pub struct DotGlInteractiveOptions {
    /// The points' color (upstream `color`, which is required).
    pub color: Hsla,
    /// The points' diameter in pixels (upstream `size`), 6 by default.
    pub size: Option<f64>,
    /// The points' opacity (upstream `opacity`), 0.8 by default.
    pub opacity: Option<f64>,
    /// A custom hover card (upstream `renderTooltip`); the default lists the row's
    /// fields.
    pub render_tooltip: Option<DotGlTooltipRender>,
}

impl DotGlInteractiveOptions {
    /// Options with `color` (upstream `{color}`).
    pub fn new(color: Hsla) -> Self {
        Self {
            color,
            size: None,
            opacity: None,
            render_tooltip: None,
        }
    }

    /// Sets the points' diameter (upstream `size`).
    pub fn size(mut self, size: f64) -> Self {
        self.size = Some(size);
        self
    }

    /// Sets the points' opacity (upstream `opacity`).
    pub fn opacity(mut self, opacity: f64) -> Self {
        self.opacity = Some(opacity);
        self
    }

    /// Sets the hover card (upstream `renderTooltip`).
    pub fn render_tooltip(
        mut self,
        render: impl Fn(&Datum, usize, &mut Window, &mut App) -> Option<AnyElement> + 'static,
    ) -> Self {
        self.render_tooltip = Some(Rc::new(render));
        self
    }
}

impl std::fmt::Debug for DotGlInteractiveOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DotGlInteractiveOptions")
            .field("color", &self.color)
            .field("size", &self.size)
            .field("opacity", &self.opacity)
            .field("render_tooltip", &self.render_tooltip.is_some())
            .finish()
    }
}

struct DotGlInteractive {
    key: SharedString,
    color: Hsla,
    size: f64,
    opacity: f64,
    render_tooltip: Option<DotGlTooltipRender>,
}

/// An interactive WebGL scatter of `data_key` (upstream `dotGLInteractive`): the
/// points of [`dot_gl`](super::dot_gl) with a hovered point picked as upstream's GPU
/// picks it, highlighted, with a card of its row beside the pointer. It draws with
/// the chart's other marks, under the axes, clipped to the plot.
pub fn dot_gl_interactive(
    data_key: impl Into<SharedString>,
    options: DotGlInteractiveOptions,
) -> SeriesDef {
    Rc::new(DotGlInteractive {
        key: data_key.into(),
        color: options.color,
        size: options.size.unwrap_or(6.),
        opacity: options.opacity.unwrap_or(0.8),
        render_tooltip: options.render_tooltip,
    })
}

/// Every point's position as its vertex buffer holds it, index-aligned with the data
/// (NaN where a point is missing, which does not rasterize).
pub(crate) fn interactive_positions(resolved: &[ResolvedPoint]) -> Vec<[f32; 2]> {
    resolved
        .iter()
        .map(|p| [p.px as f32, p.py as f32])
        .collect()
}

/// The visible pass's circle radius on a display of `dpr`: the sprite is `size * dpr`
/// device pixels (at least 1; none when not positive), its circle half of that.
pub(crate) fn interactive_radius(size: f64, dpr: f64) -> Option<f64> {
    let sprite = size * dpr;
    (sprite > 0.).then(|| sprite.max(1.) * 0.5 / dpr)
}

/// The color one visible-pass point leaves on the page. Upstream draws `(color,
/// opacity)` with `SRC_ALPHA, ONE_MINUS_SRC_ALPHA` blending into a canvas that is not
/// premultiplied, so a point alone stores `color * a` at alpha `a * a`, which the page
/// composites as such.
pub(crate) fn interactive_color(rgb: [f64; 3], opacity: f64) -> Option<Hsla> {
    let a = opacity.clamp(0., 1.);
    if a.is_nan() || a <= 0. {
        return None;
    }
    let [r, g, b] = rgb.map(|c| (c * a) as f32);
    Some(
        gpui::Rgba {
            r,
            g,
            b,
            a: (a * a) as f32,
        }
        .into(),
    )
}

/// Where the hover card goes in the plot (upstream's `foreignObject`): 12 pixels right
/// of the pointer and 40 above it, no higher than the plot's top.
pub(crate) fn card_origin(mouse: (f64, f64)) -> (f64, f64) {
    (mouse.0 + 12., js_max(0., mouse.1 - 40.))
}

/// The hover state across frames (upstream's `hoverIndex`, -1 for none, and
/// `mousePos`), with the pick buffer of the positions it was built for.
struct HoverState {
    index: i64,
    mouse: Option<(f64, f64)>,
    positions: Rc<Vec<[f32; 2]>>,
    pick: Option<(PickKey, Rc<PickBuffer>)>,
}

impl Default for HoverState {
    fn default() -> Self {
        Self {
            index: -1,
            mouse: None,
            positions: Rc::default(),
            pick: None,
        }
    }
}

/// The same positions, bit for bit (NaN included).
fn same_positions(a: &[[f32; 2]], b: &[[f32; 2]]) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b)
            .all(|(p, q)| p[0].to_bits() == q[0].to_bits() && p[1].to_bits() == q[1].to_bits())
}

/// What a pick buffer depends on besides the positions.
#[derive(Clone, Copy, PartialEq)]
struct PickKey {
    width: u64,
    height: u64,
    size: u64,
    dpr: u64,
}

impl Series for DotGlInteractive {
    fn kind(&self) -> &'static str {
        "dotGLInteractive"
    }

    fn key(&self) -> SharedString {
        self.key.clone()
    }

    fn data_keys(&self) -> Vec<SharedString> {
        vec![self.key.clone()]
    }

    fn resolve(
        &self,
        ctx: &SeriesContext,
        _: Option<&[StackOffset]>,
        _: Option<GroupInfo>,
    ) -> Vec<ResolvedPoint> {
        resolve_dots(ctx, &self.key)
    }

    fn render(
        &self,
        resolved: &[ResolvedPoint],
        ctx: &SeriesContext,
        info: &SeriesInfo,
        _: &mut Window,
        _: &mut App,
    ) -> Option<AnyElement> {
        Some(
            InteractiveDots {
                uid: info.uid.clone(),
                positions: Rc::new(interactive_positions(resolved)),
                color: self.color,
                size: self.size,
                opacity: self.opacity,
                width: ctx.width,
                height: ctx.height,
                data: ctx.data.clone(),
                render_tooltip: self.render_tooltip.clone(),
            }
            .into_any_element(),
        )
    }
}

/// The scatter's canvas, hover highlight and card (upstream
/// `DotGLInteractiveCanvas`).
#[derive(IntoElement)]
struct InteractiveDots {
    uid: SharedString,
    positions: Rc<Vec<[f32; 2]>>,
    color: Hsla,
    size: f64,
    opacity: f64,
    width: f64,
    height: f64,
    data: Rc<[Datum]>,
    render_tooltip: Option<DotGlTooltipRender>,
}

impl RenderOnce for InteractiveDots {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        if self.width <= 0. || self.height <= 0. {
            return div().into_any_element();
        }
        let chart_id: ElementId = use_chart(cx).map_or_else(|| "chart".into(), |chart| chart.id);
        let state = window.use_keyed_state(
            (chart_id, format!("dot-gl-interactive-{}", self.uid)),
            cx,
            |_, _| HoverState::default(),
        );
        // `window.devicePixelRatio || 1`.
        let scale = f64::from(window.scale_factor());
        let dpr = if scale > 0. { scale } else { 1. };
        // Keep the pick buffer while the points stay the same.
        let positions = state.update(cx, |s, _| {
            if !same_positions(&s.positions, &self.positions) {
                s.positions = self.positions.clone();
                s.pick = None;
            }
            s.positions.clone()
        });
        let key = PickKey {
            width: self.width.to_bits(),
            height: self.height.to_bits(),
            size: self.size.to_bits(),
            dpr: dpr.to_bits(),
        };
        let (index, mouse) = {
            let s = state.read(cx);
            (s.index, s.mouse)
        };
        let datum = usize::try_from(index)
            .ok()
            .and_then(|i| self.data.get(i).map(|d| (i, d.clone())));

        let theme = rustryx_theme::current(window, cx);
        let t = theme.tokens();
        let rgb = gl_rgb(self.color);
        let fill = interactive_color(rgb, self.opacity);
        let solid = super::rgb_color(rgb);
        let body = t.color.background_body;
        let size = self.size;
        let (width, height) = (self.width, self.height);
        let hovered_at = datum.as_ref().and_then(|(i, _)| positions.get(*i).copied());

        let dots = {
            let state = state.clone();
            let positions = positions.clone();
            canvas(
                move |bounds, window, _| window.insert_hitbox(bounds, HitboxBehavior::Normal),
                move |bounds: Bounds<Pixels>, hitbox: Hitbox, window, _| {
                    // The visible pass: one circle per point, in data order.
                    if let (Some(fill), Some(r)) = (fill, interactive_radius(size, dpr)) {
                        for [x, y] in positions.iter() {
                            fill_circle(
                                window,
                                bounds.origin,
                                (f64::from(*x), f64::from(*y)),
                                r,
                                fill,
                            );
                        }
                    }
                    // The hovered point: a soft halo that reads over its neighbours, and a
                    // solid marker outlined in the page's background.
                    if let Some([x, y]) = hovered_at {
                        let at = (f64::from(x), f64::from(y));
                        stroke_circle(
                            window,
                            bounds.origin,
                            at,
                            size / 2. + 7.,
                            2.,
                            solid.opacity(0.35),
                            gpui::transparent_black(),
                        );
                        stroke_circle(window, bounds.origin, at, size / 2. + 2.5, 2., body, solid);
                    }
                    let origin = bounds.origin;
                    window.on_mouse_event(move |e: &MouseMoveEvent, phase, window, cx| {
                        if phase != DispatchPhase::Bubble {
                            return;
                        }
                        let next = if hitbox.is_hovered(window) {
                            let local = (
                                f64::from(e.position.x - origin.x),
                                f64::from(e.position.y - origin.y),
                            );
                            let pick = state.update(cx, |s, _| {
                                if let Some((k, pick)) = &s.pick
                                    && *k == key
                                {
                                    return pick.clone();
                                }
                                let pick = Rc::new(PickBuffer::new(
                                    &s.positions,
                                    width,
                                    height,
                                    size,
                                    dpr,
                                ));
                                s.pick = Some((key, pick.clone()));
                                pick
                            });
                            let index = pick.hover(local.0, local.1);
                            (index, (index >= 0).then_some(local))
                        } else {
                            // The pointer left the scatter (upstream `handleMouseLeave`).
                            (-1, None)
                        };
                        state.update(cx, |s, cx| {
                            if (s.index, s.mouse) != next {
                                (s.index, s.mouse) = next;
                                cx.notify();
                            }
                        });
                    });
                },
            )
            .absolute()
            .size_full()
        };

        let card = match (&datum, mouse) {
            (Some((i, d)), Some(mouse)) => {
                let content = match &self.render_tooltip {
                    Some(render) => render(d, *i, window, cx),
                    None => Some(default_tooltip(
                        d,
                        *i,
                        t.color.text_primary,
                        t.color.text_secondary,
                    )),
                };
                let (x, y) = card_origin(mouse);
                Some(
                    div()
                        .id("dot-gl-tooltip")
                        .debug_selector(|| "dot-gl-tooltip".into())
                        .absolute()
                        .left(px(x as f32))
                        .top(px(y as f32))
                        .bg(t.color.background_popover)
                        .border_1()
                        .border_color(t.color.border)
                        .rounded(px(8.))
                        .py(px(8.))
                        .px(px(12.))
                        .shadow_token(&t.shadow.med)
                        .whitespace_nowrap()
                        .font_stack(&t.font.family.body)
                        .children(content),
                )
            }
            _ => None,
        };

        div()
            .relative()
            .size_full()
            .child(dots)
            .children(card)
            .into_any_element()
    }
}

/// Upstream's default card: "Point {i}", then each field and its `String(value)`.
fn default_tooltip(d: &Datum, index: usize, primary: Hsla, secondary: Hsla) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap(px(2.))
        .text_size(px(12.))
        .text_color(primary)
        .child(
            div()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(primary)
                .child(format!("Point {index}")),
        )
        .children(d.fields().map(|(k, v)| {
            div()
                .flex()
                .flex_row()
                .child(div().text_color(secondary).child(format!("{k}:")))
                .child(div().child(" "))
                .child(
                    div()
                        .font_weight(FontWeight::MEDIUM)
                        .child(v.to_js_string()),
                )
        }))
        .into_any_element()
}
