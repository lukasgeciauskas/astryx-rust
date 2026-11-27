//! The weight probe (upstream lab `SVGIcon/weightProbe.ts` and `weighted.stylex.ts`).
//! Upstream cannot read `font-weight` from CSS, so it registers a probe font whose
//! `1ch` is `(1000 + weight) / 1000` em and reads the weight from a container query on
//! that width. GPUI hands an element the surrounding text style, so the probe reads the
//! inherited font weight directly; the bands are upstream's container queries, and
//! there is no font to register (upstream `registerWeightProbe` has nothing to do).

use astryx_primitives::{provide, use_context};
use gpui::{App, IntoElement};

use super::build::IconWeight;

/// An ancestor's pinned weight (upstream's `--astryx-icon-weight` on an ancestor).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct IconWeightOverride(pub f32);

/// Upstream's `--astryx-icon-weight` on an ancestor: pins every weighted icon inside
/// `child` to `weight`.
pub fn pin_icon_weight(weight: f32, child: impl IntoElement) -> impl IntoElement {
    provide(IconWeightOverride(weight), child)
}

/// The pinned weight in scope, if an ancestor set one.
pub(crate) fn pinned_icon_weight(cx: &App) -> Option<f32> {
    use_context::<IconWeightOverride>(cx).map(|o| o.0)
}

/// Upstream's probe reading (`--astryx-icon-weight-read`): the band the probe's width,
/// `(1000 + weight) / 1000` em, falls in. Weights below 300 read 300; a reading outside
/// every band (no probe font) leaves the icon on its fallback.
pub fn read_weight_probe(font_weight: f32) -> Option<IconWeight> {
    let width = (1000. + f64::from(font_weight)) / 1000.;
    Some(match width {
        w if w > 1. && w < 1.35 => 300,
        w if (1.35..1.45).contains(&w) => 400,
        w if (1.45..1.55).contains(&w) => 500,
        w if (1.55..1.65).contains(&w) => 600,
        w if (1.65..2.1).contains(&w) => 700,
        _ => return None,
    })
}

/// The current weight, as every layer computes it: `clamp(300, round(weight, 100),
/// 700)` of the pinned weight, else the probe reading, else 400. CSS `round()` rounds
/// half up.
pub fn effective_icon_weight(pinned: Option<f32>, read: Option<IconWeight>) -> IconWeight {
    let weight = pinned
        .map(f64::from)
        .or(read.map(f64::from))
        .unwrap_or(400.);
    let rounded = (weight / 100. + 0.5).floor() * 100.;
    rounded.clamp(300., 700.) as IconWeight
}

/// A layer's opacity (upstream `weightRanges`): `clamp(0, min(weight - (from - 100),
/// (to + 100) - weight) / 100, 1)`, exactly 0 or 1 for whole weights.
pub fn layer_opacity(from: IconWeight, to: IconWeight, weight: IconWeight) -> f32 {
    let (from, to, weight) = (f32::from(from), f32::from(to), f32::from(weight));
    ((weight - (from - 100.)).min((to + 100.) - weight) / 100.).clamp(0., 1.)
}
