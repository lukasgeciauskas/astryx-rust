//! Theme authoring (M12): upstream's `theme/` and `palette/` runtime, which builds a
//! theme's tokens from a definition (`defineTheme`), with the color science, palette
//! engine and scale expansions behind it. The shipped themes are generated from
//! upstream's resolved output; these modules let an app define its own the way upstream
//! does, and the parity tests check them against that output.

pub mod color;
pub mod contrast;
pub mod hct;
pub(crate) mod js;
pub mod motion_knobs;
pub mod motion_scale;
pub mod radius_scale;
pub mod status_color;
pub mod theme_motion;
pub mod token_references;
pub mod type_scale;
pub mod uikit_typography;
