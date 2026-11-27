//! `ChartDotGLInteractive` (upstream lab `ChartDotGLInteractive.tsx`): a WebGL scatter with hover.
//! Upstream picks the hovered point on the GPU from an offscreen buffer of index colors;
//! [`pick_point`] finds the same point by testing the same sprites.

use std::rc::Rc;

use gpui::prelude::*;
use gpui::{
    AnyElement, App, Bounds, ContentMask, ElementId, FontWeight, Hsla, MouseMoveEvent, Pixels,
    SharedString, Window, canvas, div, px, transparent_black,
};
use rustryx_charts::Datum;

use super::context::{ChartContext, use_chart};
use super::paint::{fill_circle, stroke_circle};
use super::tooltip::card_surface;
use super::webgl::hex_to_gl;

/// A custom hover card (upstream `renderTooltip`): the hovered datum and its index.
pub type DotGlTooltipRender =
    Rc<dyn Fn(&Datum, usize, &mut Window, &mut App) -> Option<AnyElement>>;

/// A WebGL scatter with hover (upstream lab `ChartDotGLInteractive`).
#[derive(IntoElement)]
pub struct ChartDotGlInteractive {
    data_key: SharedString,
    color: Hsla,
    size: f64,
    opacity: f64,
    render_tooltip: Option<DotGlTooltipRender>,
}

impl ChartDotGlInteractive {
    /// Upstream `dataKey` and `color`.
    pub fn new(data_key: impl Into<SharedString>, color: Hsla) -> Self {
        ChartDotGlInteractive {
            data_key: data_key.into(),
            color,
            size: 6.,
            opacity: 0.8,
            render_tooltip: None,
        }
    }

    /// Upstream `size`: the points' diameter (6).
    pub fn size(mut self, size: f64) -> Self {
        self.size = size;
        self
    }

    /// Upstream `opacity` (0.8).
    pub fn opacity(mut self, opacity: f64) -> Self {
        self.opacity = opacity;
        self
    }

    /// Upstream `renderTooltip`: the hover card's content ("Point {i}" and every field by
    /// default).
    pub fn render_tooltip(
        mut self,
        render: impl Fn(&Datum, usize, &mut Window, &mut App) -> Option<AnyElement> + 'static,
    ) -> Self {
        self.render_tooltip = Some(Rc::new(render));
        self
    }
}

/// Upstream's `indexToColor`: an index as a pick color, `index + 1` so 0 is the background.
pub fn index_to_color(i: usize) -> [u8; 3] {
    let id = (i + 1) as u32;
    [
        ((id >> 16) & 0xff) as u8,
        ((id >> 8) & 0xff) as u8,
        (id & 0xff) as u8,
    ]
}

/// Upstream's `colorToIndex`: a pick color as an index, `None` for the background.
pub fn color_to_index(r: u8, g: u8, b: u8) -> Option<usize> {
    let id = (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b);
    (id != 0).then(|| (id - 1) as usize)
}

/// The point under a plot position (upstream's `readPixels` of the pick buffer): the last
/// point whose `size + 4` pick sprite covers that device pixel, at the display's pixel ratio
/// `dpr`, as the GPU rasterizes it (pixel centers inside the sprite's circle).
pub fn pick_point(positions: &[[f32; 2]], size: f64, dpr: f64, at: (f64, f64)) -> Option<usize> {
    let sprite = (size + 4.) * dpr;
    if sprite.is_nan() || sprite <= 0. {
        return None;
    }
    let sprite = sprite.max(1.);
    let (cx, cy) = ((at.0 * dpr).floor() + 0.5, (at.1 * dpr).floor() + 0.5);
    positions.iter().enumerate().rev().find_map(|(i, [x, y])| {
        let (sx, sy) = (f64::from(*x) * dpr, f64::from(*y) * dpr);
        let (u, v) = (
            (cx - (sx - sprite / 2.)) / sprite - 0.5,
            (cy - (sy - sprite / 2.)) / sprite - 0.5,
        );
        (u.abs() <= 0.5 && v.abs() <= 0.5 && u * u + v * v <= 0.25).then_some(i)
    })
}

/// The default hover card (upstream `defaultTooltip`): "Point {i}", then every field.
fn default_tooltip(d: &Datum, i: usize, t: &rustryx_theme::Tokens) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap(px(2.))
        .text_size(px(12.))
        .child(
            div()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(t.color.text_primary)
                .child(format!("Point {i}")),
        )
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

#[derive(Default)]
struct Hover {
    index: Option<usize>,
    mouse: Option<(f64, f64)>,
}

fn positions(ctx: &ChartContext, key: &str) -> Vec<[f32; 2]> {
    super::dot_gl::dot_gl_positions(ctx, key)
}

impl RenderOnce for ChartDotGlInteractive {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let Some(ctx) = use_chart(cx) else {
            return div().into_any_element();
        };
        if ctx.width <= 0. || ctx.height <= 0. {
            return div().into_any_element();
        }
        let t = rustryx_theme::current(window, cx).tokens().clone();
        let points = Rc::new(positions(&ctx, &self.data_key));
        let state = window.use_keyed_state(
            ElementId::Name(format!("{}-lab-dot-gl-hover", ctx.id).into()),
            cx,
            |_, _| Hover::default(),
        );
        let (index, mouse) = {
            let h = state.read(cx);
            (h.index.filter(|i| *i < ctx.data.len()), h.mouse)
        };
        let [r, g, b] = hex_to_gl(self.color);
        // The visible pass blends straight alpha (`SRC_ALPHA, ONE_MINUS_SRC_ALPHA`) with hard edges.
        let fill = gpui::Rgba {
            r: r as f32,
            g: g as f32,
            b: b as f32,
            a: self.opacity.clamp(0., 1.) as f32,
        };
        let (size, color) = (self.size, self.color);
        let drawn = points.clone();
        let visible = canvas(
            |_, _, _| {},
            move |bounds: Bounds<Pixels>, _, window, _| {
                // Upstream's foreignObject is clipped to the plot.
                window.with_content_mask(Some(ContentMask { bounds }), |window| {
                    for [x, y] in drawn.iter() {
                        fill_circle(
                            window,
                            bounds.origin,
                            (f64::from(*x), f64::from(*y)),
                            size / 2.,
                            fill.into(),
                        );
                    }
                    if let Some(i) = index
                        && let Some([x, y]) = drawn.get(i)
                    {
                        stroke_circle(
                            window,
                            bounds.origin,
                            (f64::from(*x), f64::from(*y)),
                            size / 2. + 3.,
                            2.,
                            color.opacity(0.8),
                            transparent_black(),
                        );
                    }
                });
            },
        )
        .absolute()
        .size_full();
        let (move_state, leave_state, move_ctx, picked) =
            (state.clone(), state.clone(), ctx.clone(), points.clone());
        let capture = div()
            .id(ElementId::Name(
                format!("{}-lab-dot-gl-capture", ctx.id).into(),
            ))
            .absolute()
            .size_full()
            .occlude()
            .on_mouse_move(move |e: &MouseMoveEvent, window, cx| {
                let at = move_ctx.to_local(e.position);
                let dpr = f64::from(window.scale_factor()).max(1.);
                let i = pick_point(&picked, size, dpr, at);
                move_state.update(cx, |h, cx| {
                    h.index = i;
                    h.mouse = i.map(|_| at);
                    cx.notify();
                });
            })
            .on_hover(move |hovered, _, cx| {
                if !*hovered {
                    leave_state.update(cx, |h, cx| {
                        *h = Hover::default();
                        cx.notify();
                    });
                }
            });
        let card = match (index, mouse) {
            (Some(i), Some((mx, my))) => {
                let content = match &self.render_tooltip {
                    Some(render) => render(&ctx.data[i], i, window, cx),
                    None => Some(default_tooltip(&ctx.data[i], i, &t)),
                };
                content.map(|content| {
                    div()
                        .absolute()
                        .left(px((mx + 12.) as f32))
                        .top(px((my - 40.).max(0.) as f32))
                        .child(card_surface(
                            ElementId::Name(format!("{}-lab-dot-gl-card", ctx.id).into()),
                            &t,
                            content,
                        ))
                })
            }
            _ => None,
        };
        div()
            .relative()
            .size_full()
            .child(visible)
            .child(capture)
            .children(card)
            .into_any_element()
    }
}
