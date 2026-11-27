//! `ChartBrush` (upstream lab `ChartBrush.tsx`): drag to select a range, along x or as a
//! rectangle.

use std::rc::Rc;

use astryx_charts::{ChartValue, Datum};
use gpui::prelude::*;
use gpui::{
    App, Bounds, CursorStyle, DispatchPhase, ElementId, Hsla, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, Pixels, Window, canvas, div,
};

use super::context::{ChartContext, use_chart};
use super::paint::fill_rect;

/// The brush's direction (upstream `BrushMode`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum BrushMode {
    /// `'x'`: a horizontal range (the default, for time ranges).
    #[default]
    X,
    /// `'xy'`: a rectangle (for scatter plots).
    Xy,
}

/// A brushed range in data coordinates (upstream `BrushRange`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BrushRange {
    pub x: [f64; 2],
    /// The y range, in `Xy` mode.
    pub y: Option<[f64; 2]>,
}

type OnBrush = Rc<dyn Fn(&BrushRange, &[Datum], &mut Window, &mut App)>;
type OnClear = Rc<dyn Fn(&mut Window, &mut App)>;

/// A drag-to-select brush (upstream lab `ChartBrush`).
#[derive(IntoElement)]
pub struct ChartBrush {
    mode: BrushMode,
    on_brush: Option<OnBrush>,
    on_clear: Option<OnClear>,
    color: Option<Hsla>,
    opacity: f64,
}

impl Default for ChartBrush {
    fn default() -> Self {
        Self::new()
    }
}

impl ChartBrush {
    /// A horizontal brush in the accent color.
    pub fn new() -> Self {
        ChartBrush {
            mode: BrushMode::X,
            on_brush: None,
            on_clear: None,
            color: None,
            opacity: 0.15,
        }
    }

    /// Upstream `mode`.
    pub fn mode(mut self, mode: BrushMode) -> Self {
        self.mode = mode;
        self
    }

    /// Upstream `onBrush`: the finished gesture's range and the data inside it.
    pub fn on_brush(
        mut self,
        f: impl Fn(&BrushRange, &[Datum], &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_brush = Some(Rc::new(f));
        self
    }

    /// Upstream `onClear`: a press without a drag.
    pub fn on_clear(mut self, f: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_clear = Some(Rc::new(f));
        self
    }

    /// Upstream `color` (the accent color).
    pub fn color(mut self, color: Hsla) -> Self {
        self.color = Some(color);
        self
    }

    /// Upstream `opacity`: the overlay's fill (0.15).
    pub fn opacity(mut self, opacity: f64) -> Self {
        self.opacity = opacity;
        self
    }
}

/// The brush rectangle in plot pixels (upstream's `brush` state).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BrushRect {
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
}

/// The rectangle a drag from `start` to `at` covers (upstream's pointer move): full height
/// in `X` mode.
pub fn brush_rect(mode: BrushMode, start: (f64, f64), at: (f64, f64), height: f64) -> BrushRect {
    let (sx, sy) = start;
    let (x, y) = at;
    match mode {
        BrushMode::X => BrushRect {
            x0: sx.min(x),
            y0: 0.,
            x1: sx.max(x),
            y1: height,
        },
        BrushMode::Xy => BrushRect {
            x0: sx.min(x),
            y0: sy.min(y),
            x1: sx.max(x),
            y1: sy.max(y),
        },
    }
}

/// What a finished gesture does (upstream's pointer up).
#[derive(Clone, Debug, PartialEq)]
pub enum BrushOutcome {
    /// Under 3 px: a click, which clears.
    Clear,
    /// A band x scale, which cannot be inverted to a range: nothing is reported.
    Ignored,
    /// The range and the data in it.
    Brush(BrushRange, Vec<Datum>),
}

/// The result of releasing `brush`: a datum is in an x range when its x is a number in it;
/// in `Xy` mode it also needs a number field (other than x) in the y range.
pub fn brush_outcome(ctx: &ChartContext, mode: BrushMode, brush: &BrushRect) -> BrushOutcome {
    let dx = (brush.x1 - brush.x0).abs();
    let dy = (brush.y1 - brush.y0).abs();
    if dx < 3. && (mode == BrushMode::X || dy < 3.) {
        return BrushOutcome::Clear;
    }
    let Some(linear) = ctx.x_scale.as_linear() else {
        return BrushOutcome::Ignored;
    };
    let (x_min, x_max) = (linear.invert(brush.x0), linear.invert(brush.x1));
    let mut range = BrushRange {
        x: [x_min, x_max],
        y: None,
    };
    let x_of = |d: &Datum| match d.get(&ctx.x_key) {
        Some(ChartValue::Number(v)) => Some(*v),
        _ => None,
    };
    let selected: Vec<Datum> = match mode {
        BrushMode::Xy => {
            let (y_min, y_max) = (ctx.y_scale.invert(brush.y1), ctx.y_scale.invert(brush.y0));
            range.y = Some([y_min, y_max]);
            ctx.data
                .iter()
                .filter(|d| {
                    x_of(d).is_some_and(|x| !(x < x_min || x > x_max))
                        && d.fields().any(|(k, v)| {
                            k.as_ref() != ctx.x_key.as_ref()
                                && matches!(v, ChartValue::Number(n) if *n >= y_min && *n <= y_max)
                        })
                })
                .cloned()
                .collect()
        }
        BrushMode::X => ctx
            .data
            .iter()
            .filter(|d| x_of(d).is_some_and(|x| x >= x_min && x <= x_max))
            .cloned()
            .collect(),
    };
    BrushOutcome::Brush(range, selected)
}

#[derive(Default)]
struct BrushState {
    brush: Option<BrushRect>,
    dragging: bool,
    start: (f64, f64),
}

impl RenderOnce for ChartBrush {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let Some(ctx) = use_chart(cx) else {
            return div().into_any_element();
        };
        let color = self
            .color
            .unwrap_or(astryx_theme::current(window, cx).tokens().color.accent);
        let state = window.use_keyed_state(
            ElementId::Name(format!("{}-lab-brush", ctx.id).into()),
            cx,
            |_, _| BrushState::default(),
        );
        let (brush, dragging) = {
            let s = state.read(cx);
            (s.brush, s.dragging)
        };
        let mode = self.mode;
        let down_state = state.clone();
        let down_ctx = ctx.clone();
        let surface = div()
            .id(ElementId::Name(
                format!("{}-lab-brush-capture", ctx.id).into(),
            ))
            .debug_selector(|| "lab-chart-brush-capture".into())
            .absolute()
            .size_full()
            .occlude()
            .cursor(CursorStyle::Crosshair)
            .on_mouse_down(MouseButton::Left, move |e: &MouseDownEvent, _, cx| {
                let at = down_ctx.to_local(e.position);
                down_state.update(cx, |s, cx| {
                    s.dragging = true;
                    s.start = at;
                    s.brush = Some(BrushRect {
                        x0: at.0,
                        y0: at.1,
                        x1: at.0,
                        y1: at.1,
                    });
                    cx.notify();
                });
            });
        let (on_brush, on_clear) = (self.on_brush.clone(), self.on_clear.clone());
        let opacity = self.opacity;
        // While dragging, the brush follows the pointer anywhere (upstream's pointer
        // capture), and the release anywhere finishes it.
        let drag_state = state.clone();
        let drag_ctx = ctx.clone();
        let tracker = canvas(
            |_, _, _| {},
            move |bounds: Bounds<Pixels>, _, window, _| {
                if let Some(b) = brush
                    && (b.x1 - b.x0 > 1. || b.y1 - b.y0 > 1.)
                {
                    fill_rect(
                        window,
                        bounds.origin,
                        (b.x0, b.y0, b.x1 - b.x0, b.y1 - b.y0),
                        if mode == BrushMode::Xy { 2. } else { 0. },
                        color.opacity(opacity.clamp(0., 1.) as f32),
                        Some((1., color.opacity(0.4))),
                    );
                }
                if !dragging {
                    return;
                }
                let (move_state, move_ctx) = (drag_state.clone(), drag_ctx.clone());
                window.on_mouse_event(move |e: &MouseMoveEvent, phase, _, cx| {
                    if phase != DispatchPhase::Capture {
                        return;
                    }
                    let at = move_ctx.to_local(e.position);
                    let height = move_ctx.height;
                    move_state.update(cx, |s, cx| {
                        if s.dragging {
                            s.brush = Some(brush_rect(mode, s.start, at, height));
                            cx.notify();
                        }
                    });
                });
                let (up_state, up_ctx, on_brush, on_clear) = (
                    drag_state.clone(),
                    drag_ctx.clone(),
                    on_brush.clone(),
                    on_clear.clone(),
                );
                window.on_mouse_event(move |_: &MouseUpEvent, phase, window, cx| {
                    if phase != DispatchPhase::Capture {
                        return;
                    }
                    let brush = up_state.update(cx, |s, _| {
                        let was = s.dragging;
                        s.dragging = false;
                        if was { s.brush } else { None }
                    });
                    let Some(brush) = brush else {
                        return;
                    };
                    match brush_outcome(&up_ctx, mode, &brush) {
                        BrushOutcome::Clear => {
                            up_state.update(cx, |s, cx| {
                                s.brush = None;
                                cx.notify();
                            });
                            if let Some(f) = &on_clear {
                                f(window, cx);
                            }
                        }
                        BrushOutcome::Ignored => up_state.update(cx, |s, cx| {
                            s.brush = None;
                            cx.notify();
                        }),
                        BrushOutcome::Brush(range, selected) => {
                            up_state.update(cx, |_, cx| cx.notify());
                            if let Some(f) = &on_brush {
                                f(&range, &selected, window, cx);
                            }
                        }
                    }
                });
            },
        )
        .absolute()
        .size_full();
        div()
            .relative()
            .size_full()
            .child(surface)
            .child(tracker)
            .into_any_element()
    }
}
