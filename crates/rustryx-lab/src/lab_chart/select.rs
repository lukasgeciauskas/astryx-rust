//! `ChartSelect` (upstream lab `ChartSelect.tsx`): press to select the nearest data point.

use std::rc::Rc;

use gpui::prelude::*;
use gpui::{
    App, Bounds, CursorStyle, ElementId, Hsla, MouseButton, MouseUpEvent, Pixels, Window, canvas,
    div, transparent_black,
};
use rustryx_charts::{ChartValue, Datum};

use super::context::{ChartContext, use_chart};
use super::paint::stroke_circle;
use super::utils::x_pixel;

type OnSelect = Rc<dyn Fn(&Datum, usize, &mut Window, &mut App)>;
type OnSelectionChange = Rc<dyn Fn(Vec<usize>, &mut Window, &mut App)>;

/// Point selection (upstream lab `ChartSelect`): a press within 30 px of a point selects it;
/// shift or the platform key adds or removes it from `selected`.
#[derive(IntoElement)]
pub struct ChartSelect {
    on_select: Option<OnSelect>,
    on_selection_change: Option<OnSelectionChange>,
    selected: Vec<usize>,
    color: Option<Hsla>,
    radius: f64,
}

impl Default for ChartSelect {
    fn default() -> Self {
        Self::new()
    }
}

impl ChartSelect {
    /// A selection with nothing selected, ringed in the accent color.
    pub fn new() -> Self {
        ChartSelect {
            on_select: None,
            on_selection_change: None,
            selected: Vec::new(),
            color: None,
            radius: 6.,
        }
    }

    /// Upstream `onSelect`: the pressed datum and its index.
    pub fn on_select(mut self, f: impl Fn(&Datum, usize, &mut Window, &mut App) + 'static) -> Self {
        self.on_select = Some(Rc::new(f));
        self
    }

    /// Upstream `onSelectionChange`: the selection after a press.
    pub fn on_selection_change(
        mut self,
        f: impl Fn(Vec<usize>, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_selection_change = Some(Rc::new(f));
        self
    }

    /// Upstream `selected`: the ringed points' indices (controlled).
    pub fn selected(mut self, selected: impl IntoIterator<Item = usize>) -> Self {
        self.selected = selected.into_iter().collect();
        self
    }

    /// Upstream `color`: the rings' (the accent color).
    pub fn color(mut self, color: Hsla) -> Self {
        self.color = Some(color);
        self
    }

    /// Upstream `radius`: the rings' (6).
    pub fn radius(mut self, radius: f64) -> Self {
        self.radius = radius;
        self
    }
}

/// A datum's point: its x pixel and the y of its first number field other than x (0
/// without one).
fn point_of(ctx: &ChartContext, d: &Datum) -> (f64, f64) {
    let py = d
        .fields()
        .find_map(|(k, v)| match v {
            ChartValue::Number(n) if k.as_ref() != ctx.x_key.as_ref() => {
                Some(ctx.y_scale.scale(*n))
            }
            _ => None,
        })
        .unwrap_or(0.);
    (x_pixel(d, &ctx.x_key, &ctx.x_scale), py)
}

/// The datum a press at (`x`, `y`) selects (upstream's pointer up): the nearest point, unless
/// it is more than 30 px away.
pub fn select_nearest(ctx: &ChartContext, x: f64, y: f64) -> Option<usize> {
    let (mut closest, mut min_dist) = (0, f64::INFINITY);
    for (i, d) in ctx.data.iter().enumerate() {
        let (px, py) = point_of(ctx, d);
        let dist = rustryx_theme::authoring::js::js_hypot(px - x, py - y);
        if dist < min_dist {
            min_dist = dist;
            closest = i;
        }
    }
    (min_dist.is_nan() || min_dist <= 30.).then_some(closest)
}

/// The selection after selecting `closest`: toggled in it when `additive` (shift or the
/// platform key), else alone.
pub fn next_selection(selected: &[usize], closest: usize, additive: bool) -> Vec<usize> {
    if !additive {
        return vec![closest];
    }
    if selected.contains(&closest) {
        selected.iter().copied().filter(|i| *i != closest).collect()
    } else {
        selected
            .iter()
            .copied()
            .chain(std::iter::once(closest))
            .collect()
    }
}

/// The selected points' ring centers, out-of-range indices left out.
pub fn selection_rings(ctx: &ChartContext, selected: &[usize]) -> Vec<(f64, f64)> {
    selected
        .iter()
        .filter_map(|i| ctx.data.get(*i))
        .map(|d| point_of(ctx, d))
        .collect()
}

impl RenderOnce for ChartSelect {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let Some(ctx) = use_chart(cx) else {
            return div().into_any_element();
        };
        let color = self
            .color
            .unwrap_or(rustryx_theme::current(window, cx).tokens().color.accent);
        let rings = selection_rings(&ctx, &self.selected);
        let (on_select, on_change, selected) = (
            self.on_select.clone(),
            self.on_selection_change.clone(),
            self.selected.clone(),
        );
        let up_ctx = ctx.clone();
        let surface = div()
            .id(ElementId::Name(format!("{}-lab-select", ctx.id).into()))
            .debug_selector(|| "lab-chart-select-capture".into())
            .absolute()
            .size_full()
            .occlude()
            .cursor(CursorStyle::PointingHand)
            .on_mouse_up(MouseButton::Left, move |e: &MouseUpEvent, window, cx| {
                let (x, y) = up_ctx.to_local(e.position);
                let Some(closest) = select_nearest(&up_ctx, x, y) else {
                    return;
                };
                if let Some(f) = &on_select {
                    f(&up_ctx.data[closest], closest, window, cx);
                }
                if let Some(f) = &on_change {
                    let additive = e.modifiers.shift || e.modifiers.platform;
                    f(next_selection(&selected, closest, additive), window, cx);
                }
            });
        let radius = self.radius;
        let marks = canvas(
            |_, _, _| {},
            move |bounds: Bounds<Pixels>, _, window, _| {
                for p in &rings {
                    stroke_circle(
                        window,
                        bounds.origin,
                        *p,
                        radius,
                        2.,
                        color,
                        transparent_black(),
                    );
                }
            },
        )
        .absolute()
        .size_full();
        div()
            .relative()
            .size_full()
            .child(surface)
            .child(marks)
            .into_any_element()
    }
}
