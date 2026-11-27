//! `dot_gl`: a scatter of round points (upstream `marks/dotGL.tsx`), upstream's
//! WebGL scatter drawn as GPUI quads. Points that are not finite are skipped.

use std::rc::Rc;

use gpui::prelude::*;
use gpui::{AnyElement, App, Bounds, Hsla, IntoElement, Pixels, SharedString, Window, canvas};

use super::{
    POINT_SIZE_COMPENSATION, canvas_dpr, fill_circle, gl_rgb, premultiplied, resolve_dots,
};
use crate::types::{
    GroupInfo, ResolvedPoint, Series, SeriesContext, SeriesDef, SeriesInfo, StackOffset,
};

/// A WebGL scatter's options (upstream `DotGLOptions`).
#[derive(Clone, Debug)]
pub struct DotGlOptions {
    /// The points' color (upstream `color`, which is required). The chart's palette
    /// color does not apply, as upstream's mark never reads it.
    pub color: Hsla,
    /// The points' diameter in pixels (upstream `size`), 6 by default.
    pub size: Option<f64>,
    /// The points' opacity (upstream `opacity`), 0.8 by default.
    pub opacity: Option<f64>,
}

impl DotGlOptions {
    /// Options with `color` (upstream `{color}`).
    pub fn new(color: Hsla) -> Self {
        Self {
            color,
            size: None,
            opacity: None,
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
}

struct DotGl {
    key: SharedString,
    color: Hsla,
    size: f64,
    opacity: f64,
}

/// A WebGL scatter of `data_key` (upstream `dotGL`): one round point per row, at
/// `size` diameter and `opacity`, drawn above the chart's axes inside the plot.
pub fn dot_gl(data_key: impl Into<SharedString>, options: DotGlOptions) -> SeriesDef {
    Rc::new(DotGl {
        key: data_key.into(),
        color: options.color,
        size: options.size.unwrap_or(6.),
        opacity: options.opacity.unwrap_or(0.8),
    })
}

/// The points a dot scatter draws (upstream's `positions`): the finite ones, as the
/// 32-bit floats its vertex buffer holds.
pub(crate) fn dot_gl_positions(resolved: &[ResolvedPoint]) -> Vec<[f32; 2]> {
    resolved
        .iter()
        .filter(|p| p.px.is_finite() && p.py.is_finite())
        .map(|p| [p.px as f32, p.py as f32])
        .collect()
}

/// The visible radius of a `size` point on a display of `scale_factor`: the sprite is
/// `size * dpr / 0.96` device pixels of upstream's supersampled canvas (GL draws a
/// point at least 1 pixel wide, and none for a size that is not positive), and the
/// circle ends where the smoothstep edge starts, at 0.48 of it. `None` for no point.
pub(crate) fn dot_gl_radius(size: f64, scale_factor: f32) -> Option<f64> {
    let dpr = canvas_dpr(scale_factor);
    let sprite = size * dpr * POINT_SIZE_COMPENSATION;
    (sprite > 0.).then(|| sprite.max(1.) * 0.48 / dpr)
}

impl Series for DotGl {
    fn kind(&self) -> &'static str {
        "dotGL"
    }

    fn key(&self) -> SharedString {
        self.key.clone()
    }

    fn data_keys(&self) -> Vec<SharedString> {
        vec![self.key.clone()]
    }

    fn draws_above(&self) -> bool {
        true
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
        _: &SeriesInfo,
        _: &mut Window,
        _: &mut App,
    ) -> Option<AnyElement> {
        if ctx.width <= 0. || ctx.height <= 0. {
            return None;
        }
        let positions = dot_gl_positions(resolved);
        let color = premultiplied(gl_rgb(self.color), self.opacity);
        let size = self.size;
        Some(
            canvas(
                |_, _, _| {},
                move |bounds: Bounds<Pixels>, _, window, _| {
                    let (Some(color), Some(r)) =
                        (color, dot_gl_radius(size, window.scale_factor()))
                    else {
                        return;
                    };
                    for [x, y] in &positions {
                        fill_circle(
                            window,
                            bounds.origin,
                            (f64::from(*x), f64::from(*y)),
                            r,
                            color,
                        );
                    }
                },
            )
            .size_full()
            .into_any_element(),
        )
    }
}
