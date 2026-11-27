//! `dot`: a dot (scatter) series (upstream `marks/dot.tsx`). Points with no finite
//! value are skipped, and `dodge` spreads points on one x side by side, ignoring the
//! ones that do not draw.

use std::collections::HashMap;
use std::rc::Rc;

use gpui::{AnyElement, App, Hsla, SharedString, Window};
use rustryx_theme::Tokens;

use super::paint::shapes_element;
use super::{clamp_opacity, non_negative};
use crate::mark_color::point_fill;
use crate::types::{
    ColorAccessor, GroupInfo, MarkShape, ResolvedPoint, Series, SeriesContext, SeriesDef,
    SeriesInfo, SeriesLayout, StackOffset,
};
use crate::utils::x_pixel;

/// A dot series' options (upstream `DotOptions`).
#[derive(Clone, Debug, Default)]
pub struct DotOptions {
    pub color: Option<ColorAccessor>,
    /// 4 by default.
    pub radius: Option<f64>,
    /// 0.8 by default.
    pub opacity: Option<f64>,
    /// Spread points that land on one x side by side, centered on it.
    pub dodge: bool,
    pub label: Option<SharedString>,
}

impl DotOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn color(mut self, color: impl Into<ColorAccessor>) -> Self {
        self.color = Some(color.into());
        self
    }

    pub fn radius(mut self, radius: f64) -> Self {
        self.radius = Some(radius);
        self
    }

    pub fn opacity(mut self, opacity: f64) -> Self {
        self.opacity = Some(opacity);
        self
    }

    pub fn dodge(mut self, dodge: bool) -> Self {
        self.dodge = dodge;
        self
    }

    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }
}

struct Dot {
    key: SharedString,
    color: Option<ColorAccessor>,
    radius: f64,
    opacity: f64,
    dodge: bool,
    label: SharedString,
}

/// A dot series of `data_key` (upstream `dot`).
pub fn dot(data_key: impl Into<SharedString>, options: DotOptions) -> SeriesDef {
    let key = data_key.into();
    Rc::new(Dot {
        label: options.label.unwrap_or_else(|| key.clone()),
        key,
        color: options.color,
        radius: non_negative(options.radius, 4.),
        opacity: clamp_opacity(options.opacity, 0.8),
        dodge: options.dodge,
    })
}

impl Series for Dot {
    fn kind(&self) -> &'static str {
        "dot"
    }

    fn key(&self) -> SharedString {
        self.key.clone()
    }

    fn data_keys(&self) -> Vec<SharedString> {
        vec![self.key.clone()]
    }

    fn color(&self) -> Option<Hsla> {
        self.color.as_ref().and_then(ColorAccessor::fixed)
    }

    fn label(&self) -> Option<SharedString> {
        Some(self.label.clone())
    }

    fn layout(&self) -> SeriesLayout {
        SeriesLayout {
            x_overhang: Some(self.radius),
            ..SeriesLayout::default()
        }
    }

    fn resolve(
        &self,
        ctx: &SeriesContext,
        _: Option<&[StackOffset]>,
        _: Option<GroupInfo>,
    ) -> Vec<ResolvedPoint> {
        let mut points: Vec<ResolvedPoint> = ctx
            .data
            .iter()
            .enumerate()
            .map(|(i, d)| ResolvedPoint {
                px: x_pixel(d, &ctx.x_key, &ctx.x_scale),
                py: ctx.y_scale.scale(d.number_or_nan(&self.key)),
                py0: ctx.y_scale.scale(0.),
                data_index: i,
            })
            .collect();
        if self.dodge {
            // Only points that draw (finite x and y) join a group, in first-seen order
            // of their rounded x, so a missing value cannot push the rest off center.
            let mut order: Vec<i64> = Vec::new();
            let mut groups: HashMap<i64, Vec<usize>> = HashMap::new();
            for (i, p) in points.iter().enumerate() {
                if !p.px.is_finite() || !p.py.is_finite() {
                    continue;
                }
                let key = rustryx_theme::authoring::js::js_round(p.px) as i64;
                groups.entry(key).or_insert_with(|| {
                    order.push(key);
                    Vec::new()
                });
                if let Some(g) = groups.get_mut(&key) {
                    g.push(i);
                }
            }
            let spacing = self.radius * 2. + 1.;
            for key in order {
                let group = &groups[&key];
                if group.len() < 2 {
                    continue;
                }
                let mid = (group.len() as f64 - 1.) / 2.;
                for (idx, &i) in group.iter().enumerate() {
                    points[i].px += (idx as f64 - mid) * spacing;
                }
            }
        }
        points
    }

    fn shapes(
        &self,
        resolved: &[ResolvedPoint],
        ctx: &SeriesContext,
        info: &SeriesInfo,
        tokens: &Tokens,
    ) -> Vec<MarkShape> {
        let accent = tokens.color.accent;
        resolved
            .iter()
            .filter_map(|p| {
                let d = ctx.data.get(p.data_index)?;
                if !p.px.is_finite() || !p.py.is_finite() {
                    return None;
                }
                Some(MarkShape::Circle {
                    cx: p.px,
                    cy: p.py,
                    r: self.radius,
                    color: point_fill(info, self.color.as_ref(), d, p.data_index, accent),
                    opacity: self.opacity,
                })
            })
            .collect()
    }

    fn render(
        &self,
        resolved: &[ResolvedPoint],
        ctx: &SeriesContext,
        info: &SeriesInfo,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<AnyElement> {
        let theme = rustryx_theme::current(window, cx);
        Some(shapes_element(self.shapes(
            resolved,
            ctx,
            info,
            theme.tokens(),
        )))
    }
}
