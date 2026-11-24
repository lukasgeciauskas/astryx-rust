//! Astryx, Meta's design system, for GPUI.
//!
//! This facade re-exports every Astryx crate. Most apps depend on this crate
//! only and import [`prelude`].

pub use astryx_components as components;
pub use astryx_icons as icons;
pub use astryx_primitives as primitives;
pub use astryx_theme as theme;

/// The types most apps use.
pub mod prelude {
    pub use astryx_primitives::{ElementSize, provide, resolve_size, use_context};
}
