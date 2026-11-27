//! Theme authoring (M12): upstream's `theme/` and `palette/` runtime, which builds a
//! theme's tokens from a definition (`defineTheme`), with the color science, palette
//! engine and scale expansions behind it. The shipped themes are generated from
//! upstream's resolved output; these modules let an app define its own the way upstream
//! does, and the parity tests check them against that output.

pub mod adaptations;
pub mod color;
pub mod color_scale;
pub mod contrast;
pub mod css_values;
pub mod define;
pub mod dynamic_type;
pub mod hct;
pub(crate) mod js;
pub mod local_tokens;
pub mod merge_components;
pub mod motion_knobs;
pub mod motion_scale;
pub mod on_media;
pub mod palette;
pub mod radius_scale;
pub mod registry;
pub mod resolve_values;
pub mod status_color;
pub mod status_roles;
pub mod theme_motion;
pub mod theme_tokens;
pub mod to_theme;
pub mod token_references;
pub mod token_values;
pub mod type_scale;
pub mod uikit_typography;

pub use crate::generated::token_defaults::TOKEN_DEFAULTS;
