//! `ThreeDAxis` (upstream lab `ThreeDAxis.tsx`): the three axes from the cube's origin corner,
//! labelled with their keys and domains.

use astryx_theme::ThemeStyled;
use astryx_theme::authoring::js::js_number_to_string;
use gpui::prelude::*;
use gpui::{App, Bounds, Pixels, SharedString, Window, canvas, div};

use super::context::{ThreeDContext, use_3d};
use super::types::ProjectedPoint;
use crate::lab_chart::paint::{Anchor, Baseline, paint_text, stroke_segment};

/// The 3D axes (upstream lab `ThreeDAxis`).
#[derive(IntoElement)]
pub struct ThreeDAxis {
    labels: bool,
}

impl Default for ThreeDAxis {
    fn default() -> Self {
        Self::new()
    }
}

impl ThreeDAxis {
    /// Labelled axes.
    pub fn new() -> Self {
        ThreeDAxis { labels: true }
    }

    /// Upstream `labels`: each axis's key and domain at its end (on).
    pub fn labels(mut self, labels: bool) -> Self {
        self.labels = labels;
        self
    }
}

/// The axes: the origin, the x, y and z ends, and with `labels` each end's label ("x [0-2]").
pub fn axis_geometry(
    ctx: &ThreeDContext,
    labels: bool,
) -> (ProjectedPoint, [(ProjectedPoint, Option<SharedString>); 3]) {
    let label = |key: &str, d: [f64; 2]| -> Option<SharedString> {
        labels.then(|| {
            format!(
                "{key} [{}-{}]",
                js_number_to_string(d[0]),
                js_number_to_string(d[1])
            )
            .into()
        })
    };
    (
        ctx.project(0., 0., 0.),
        [
            (ctx.project(1., 0., 0.), label(&ctx.x_key, ctx.x_domain)),
            (ctx.project(0., 1., 0.), label(&ctx.y_key, ctx.y_domain)),
            (ctx.project(0., 0., 1.), label(&ctx.z_key, ctx.z_domain)),
        ],
    )
}

impl RenderOnce for ThreeDAxis {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let Some(ctx) = use_3d(cx) else {
            return div().into_any_element();
        };
        let t = astryx_theme::current(window, cx).tokens().clone();
        let (line, text) = (t.color.border_emphasized, t.color.text_secondary);
        let (origin, ends) = axis_geometry(&ctx, self.labels);
        let shapes = canvas(
            |_, _, _| {},
            move |bounds: Bounds<Pixels>, _, window, cx| {
                let o = bounds.origin;
                for (end, _) in &ends {
                    stroke_segment(
                        window,
                        o,
                        (origin.px, origin.py),
                        (end.px, end.py),
                        1.5,
                        line,
                        &[],
                    );
                }
                for (end, label) in &ends {
                    if let Some(label) = label {
                        paint_text(
                            window,
                            cx,
                            o,
                            label,
                            (end.px + 8., end.py),
                            Anchor::Start,
                            Baseline::Central,
                            11.,
                            None,
                            text,
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
