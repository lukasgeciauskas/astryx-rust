//! CSS effects GPUI does not draw natively, built from its existing paint
//! APIs (see `docs/guide/effects.md`). Effects that need new shader code are
//! patches in our GPUI fork instead (`docs/gpui-fork.md`).
//!
//! - [`gradient`]: linear gradients with any number of stops at lengths or
//!   fractions.
//! - [`fade`]: edge fades, upstream's `mask-image` on scroll regions.
//! - [`shape`]: polygons, upstream's `clip-path: polygon(..)`.

pub mod fade;
pub mod gradient;
pub mod shape;

pub use fade::{
    EdgeStop, FadeEdge, MaskAxis, SCROLL_FADE, edge_fade, edge_fade_stops, edge_mask, edge_stop,
};
pub use gradient::{Gradient, GradientDirection, GradientStop, StopPosition, gradient, stop};
pub use shape::{Pointing, Shape, polygon, triangle};
