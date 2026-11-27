//! `ChartZoom` (upstream lab `ChartZoom.tsx`): wheel zoom and drag pan through the chart's
//! domain callbacks, with an optional toolbar.

use std::rc::Rc;

use astryx_charts::ChartScale;
use astryx_components::button::{ButtonVariant, IconButton};
use astryx_primitives::ElementSize;
use gpui::prelude::*;
use gpui::{
    App, Bounds, CursorStyle, DispatchPhase, ElementId, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, PathBuilder, Pixels, ScrollEvent, Window, canvas, div, point, px,
};

use super::context::{ChartContext, use_chart};

/// Where the toolbar sits (upstream `ZoomToolbarPosition`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ZoomToolbarPosition {
    #[default]
    TopRight,
    TopLeft,
    BottomRight,
    BottomLeft,
}

type DomainChange = Rc<dyn Fn([f64; 2], &mut Window, &mut App)>;

/// Zoom and pan (upstream lab `ChartZoom`): the wheel zooms around the pointer, a drag pans,
/// and the toolbar zooms around the center or resets to the first render's domains.
/// The chart's `x_domain` and `y_domain` should come from what the callbacks report.
#[derive(IntoElement)]
pub struct ChartZoom {
    on_x_domain_change: Option<DomainChange>,
    on_y_domain_change: Option<DomainChange>,
    zoom_speed: f64,
    x_only: bool,
    y_only: bool,
    toolbar: Option<ZoomToolbarPosition>,
}

impl Default for ChartZoom {
    fn default() -> Self {
        Self::new()
    }
}

impl ChartZoom {
    /// Zoom on both axes at 0.1 per wheel tick, with the toolbar at the top right.
    pub fn new() -> Self {
        ChartZoom {
            on_x_domain_change: None,
            on_y_domain_change: None,
            zoom_speed: 0.1,
            x_only: false,
            y_only: false,
            toolbar: Some(ZoomToolbarPosition::TopRight),
        }
    }

    /// Upstream `onXDomainChange`.
    pub fn on_x_domain_change(
        mut self,
        f: impl Fn([f64; 2], &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_x_domain_change = Some(Rc::new(f));
        self
    }

    /// Upstream `onYDomainChange`.
    pub fn on_y_domain_change(
        mut self,
        f: impl Fn([f64; 2], &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_y_domain_change = Some(Rc::new(f));
        self
    }

    /// Upstream `zoomSpeed`: the zoom per wheel tick (0.1).
    pub fn zoom_speed(mut self, speed: f64) -> Self {
        self.zoom_speed = speed;
        self
    }

    /// Upstream `xOnly`: zoom and pan x only.
    pub fn x_only(mut self, x_only: bool) -> Self {
        self.x_only = x_only;
        self
    }

    /// Upstream `yOnly`: zoom and pan y only.
    pub fn y_only(mut self, y_only: bool) -> Self {
        self.y_only = y_only;
        self
    }

    /// Upstream `toolbar`: its corner, or `None` (upstream `false`) for none.
    pub fn toolbar(mut self, toolbar: Option<ZoomToolbarPosition>) -> Self {
        self.toolbar = toolbar;
        self
    }
}

/// A domain zoomed by `factor` around `pivot` (upstream `zoomAxis`): above 1 zooms out, below 1
/// in, the pivot staying where it is.
pub fn zoom_axis(domain: [f64; 2], factor: f64, pivot: f64) -> [f64; 2] {
    let [lo, hi] = domain;
    let range = hi - lo;
    let left = (pivot - lo) / range;
    let next = range * factor;
    [pivot - next * left, pivot + next * (1. - left)]
}

/// The factor a wheel delta zooms by (upstream's `rawFactor`): the delta clamped to 50 px,
/// so one tick zooms at most `zoom_speed`.
pub fn wheel_factor(delta_y: f64, zoom_speed: f64) -> f64 {
    let clamped = delta_y.signum() * delta_y.abs().min(50.);
    1. + (clamped / 50.) * zoom_speed
}

/// A domain panned by a drag of `d` pixels over a plot `span` long: x moves against the
/// drag, y with it (its pixels grow downward).
pub fn pan_domain(domain: [f64; 2], d: f64, span: f64, invert: bool) -> [f64; 2] {
    let [lo, hi] = domain;
    let shift = if invert {
        -(d / span) * (hi - lo)
    } else {
        (d / span) * (hi - lo)
    };
    [lo + shift, hi + shift]
}

fn x_domain(ctx: &ChartContext) -> Option<[f64; 2]> {
    match &ctx.x_scale {
        ChartScale::Linear(l) => Some(l.get_domain()),
        ChartScale::Band(_) => None,
    }
}

/// A drag's start and the domains it started from.
type Drag = ((f64, f64), [f64; 2], [f64; 2]);

struct ZoomState {
    /// The domains of the first render (upstream's `initialDomains`).
    initial: (Option<[f64; 2]>, [f64; 2]),
    /// The drag's start and the domains it started from.
    drag: Option<Drag>,
}

/// The toolbar's glyphs (upstream's 16 × 16 inline SVGs), stroked in the button's text color.
#[derive(Clone, Copy)]
enum Glyph {
    ZoomIn,
    ZoomOut,
    Reset,
}

fn glyph(g: Glyph) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |bounds: Bounds<Pixels>, _, window, _| {
            let color = window.text_style().color;
            let o = bounds.origin;
            let p = |x: f32, y: f32| o + point(px(x), px(y));
            let mut b = PathBuilder::stroke(px(1.5)).with_style(gpui::PathStyle::Stroke(
                gpui::StrokeOptions {
                    line_width: 1.5,
                    line_cap: gpui::LineCap::Round,
                    line_join: gpui::LineJoin::Round,
                    ..Default::default()
                },
            ));
            match g {
                Glyph::ZoomIn => {
                    b.move_to(p(8., 3.));
                    b.line_to(p(8., 13.));
                    b.move_to(p(3., 8.));
                    b.line_to(p(13., 8.));
                }
                Glyph::ZoomOut => {
                    b.move_to(p(3., 8.));
                    b.line_to(p(13., 8.));
                }
                Glyph::Reset => {
                    // "M2 8a6 6 0 1 1 1.5 4M2 12V8h4"
                    b.move_to(p(2., 8.));
                    b.arc_to(point(px(6.), px(6.)), px(0.), true, true, p(3.5, 12.));
                    b.move_to(p(2., 12.));
                    b.line_to(p(2., 8.));
                    b.line_to(p(6., 8.));
                }
            }
            if let Ok(path) = b.build() {
                window.paint_path(path, color);
            }
        },
    )
    .size(px(16.))
}

impl RenderOnce for ChartZoom {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let Some(ctx) = use_chart(cx) else {
            return div().into_any_element();
        };
        let start = (x_domain(&ctx), ctx.y_scale.get_domain());
        let state = window.use_keyed_state(
            ElementId::Name(format!("{}-lab-zoom", ctx.id).into()),
            cx,
            move |_, _| ZoomState {
                initial: start,
                drag: None,
            },
        );
        let (x_only, y_only, speed) = (self.x_only, self.y_only, self.zoom_speed);
        let (on_x, on_y) = (
            self.on_x_domain_change.clone(),
            self.on_y_domain_change.clone(),
        );

        // Zooms both axes by `factor` around a pivot pixel, or the domains' middles.
        let zoom_by = {
            let (ctx, on_x, on_y) = (ctx.clone(), on_x.clone(), on_y.clone());
            Rc::new(
                move |factor: f64, pivot: Option<(f64, f64)>, window: &mut Window, cx: &mut App| {
                    if !y_only && let ChartScale::Linear(linear) = &ctx.x_scale {
                        let [lo, hi] = linear.get_domain();
                        let p = pivot.map_or((lo + hi) / 2., |(x, _)| linear.invert(x));
                        if let Some(f) = &on_x {
                            f(zoom_axis([lo, hi], factor, p), window, cx);
                        }
                    }
                    if !x_only {
                        let [lo, hi] = ctx.y_scale.get_domain();
                        let p = pivot.map_or((lo + hi) / 2., |(_, y)| ctx.y_scale.invert(y));
                        if let Some(f) = &on_y {
                            f(zoom_axis([lo, hi], factor, p), window, cx);
                        }
                    }
                },
            )
        };

        let wheel_zoom = zoom_by.clone();
        let (wheel_ctx, down_ctx, down_state) = (ctx.clone(), ctx.clone(), state.clone());
        let surface = div()
            .id(ElementId::Name(
                format!("{}-lab-zoom-surface", ctx.id).into(),
            ))
            .debug_selector(|| "lab-chart-zoom-surface".into())
            .absolute()
            .size_full()
            .occlude()
            .cursor(CursorStyle::OpenHand)
            .on_scroll(move |e: &ScrollEvent, window, cx| {
                // The DOM's deltaY grows as the wheel turns toward the user; GPUI's delta
                // is the content's movement, the other way.
                let delta_y = -f64::from(e.delta.pixel_delta(px(16.)).y);
                // A sideways wheel zooms nothing, so the page keeps its scroll.
                if delta_y == 0. {
                    return;
                }
                cx.stop_propagation();
                let at = wheel_ctx.to_local(e.position);
                wheel_zoom(wheel_factor(delta_y, speed), Some(at), window, cx);
            })
            .on_mouse_down(MouseButton::Left, move |e: &MouseDownEvent, _, cx| {
                let at = (f64::from(e.position.x), f64::from(e.position.y));
                let x = x_domain(&down_ctx).unwrap_or([0., 0.]);
                let y = down_ctx.y_scale.get_domain();
                down_state.update(cx, |s, cx| {
                    s.drag = Some((at, x, y));
                    cx.notify();
                });
            });
        let dragging = state.read(cx).drag.is_some();
        let (track_state, track_ctx) = (state.clone(), ctx.clone());
        let (pan_x, pan_y) = (on_x.clone(), on_y.clone());
        // While dragging, the pan follows the pointer anywhere (upstream's pointer capture).
        let tracker = canvas(
            |_, _, _| {},
            move |_, _, window, _| {
                if !dragging {
                    return;
                }
                let (move_state, move_ctx, pan_x, pan_y) = (
                    track_state.clone(),
                    track_ctx.clone(),
                    pan_x.clone(),
                    pan_y.clone(),
                );
                window.on_mouse_event(move |e: &MouseMoveEvent, phase, window, cx| {
                    if phase != DispatchPhase::Capture {
                        return;
                    }
                    let Some(((sx, sy), xd, yd)) = move_state.read(cx).drag else {
                        return;
                    };
                    let (dx, dy) = (f64::from(e.position.x) - sx, f64::from(e.position.y) - sy);
                    if !y_only
                        && move_ctx.x_scale.as_linear().is_some()
                        && let Some(f) = &pan_x
                    {
                        f(pan_domain(xd, dx, move_ctx.width, true), window, cx);
                    }
                    if !x_only && let Some(f) = &pan_y {
                        f(pan_domain(yd, dy, move_ctx.height, false), window, cx);
                    }
                });
                let up_state = track_state.clone();
                window.on_mouse_event(move |_: &MouseUpEvent, phase, _, cx| {
                    if phase == DispatchPhase::Capture {
                        up_state.update(cx, |s, cx| {
                            s.drag = None;
                            cx.notify();
                        });
                    }
                });
            },
        )
        .absolute()
        .size_full();

        if let Some(position) = self.toolbar {
            let (zoom_in, zoom_out) = (zoom_by.clone(), zoom_by.clone());
            let (reset_state, reset_x, reset_y) = (state.clone(), on_x.clone(), on_y.clone());
            let id = ctx.id.clone();
            let button = |key: &str, label: &'static str, g: Glyph| {
                IconButton::new(
                    ElementId::Name(format!("{id}-lab-zoom-{key}").into()),
                    label,
                    glyph(g),
                )
                .variant(ButtonVariant::Ghost)
                .size(ElementSize::Sm)
            };
            let bar = div()
                .debug_selector(|| "lab-chart-zoom-toolbar".into())
                .absolute()
                .flex()
                .flex_col()
                .gap(px(4.))
                .map(|d| match position {
                    ZoomToolbarPosition::TopRight => d.top(px(8.)).right(px(8.)),
                    ZoomToolbarPosition::TopLeft => d.top(px(8.)).left(px(8.)),
                    ZoomToolbarPosition::BottomRight => d.bottom(px(8.)).right(px(8.)),
                    ZoomToolbarPosition::BottomLeft => d.bottom(px(8.)).left(px(8.)),
                })
                .child(
                    button("in", "Zoom in", Glyph::ZoomIn).on_click(move |_, window, cx| {
                        zoom_in(1. / (1. + speed), None, window, cx)
                    }),
                )
                .child(
                    button("out", "Zoom out", Glyph::ZoomOut)
                        .on_click(move |_, window, cx| zoom_out(1. + speed, None, window, cx)),
                )
                .child(button("reset", "Reset zoom", Glyph::Reset).on_click(
                    move |_, window, cx| {
                        let (x, y) = reset_state.read(cx).initial;
                        if let (Some(x), Some(f)) = (x, &reset_x) {
                            f(x, window, cx);
                        }
                        if let Some(f) = &reset_y {
                            f(y, window, cx);
                        }
                    },
                ));
            ctx.portal(bar);
        }
        div()
            .relative()
            .size_full()
            .child(surface)
            .child(tracker)
            .into_any_element()
    }
}
