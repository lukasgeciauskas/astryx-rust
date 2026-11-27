//! Astryx design tokens and themes for GPUI.
//!
//! The token surface ([`Tokens`]) and core's default values are generated
//! from upstream Astryx by `cargo xtask codegen`; see `UPSTREAM.md`. The
//! published themes live in `astryx-themes`.

pub mod authoring;
pub mod color;
pub mod fonts;
#[cfg(feature = "json")]
pub mod spec;
mod styled;
mod theme;
pub mod types;

#[rustfmt::skip]
pub mod generated {
    pub mod tokens;
    pub mod default_theme;
    pub mod token_defaults;
}

pub use generated::tokens::*;
pub use styled::ThemeStyled;
pub use theme::{
    ResolvedTheme, Theme, ThemeMode, ThemeScope, active, apply_style, current, init, prepare,
    register_fonts, scope, set_mode, set_theme, with_tokens,
};
pub use types::*;

/// Everything a generated theme module refers to.
#[doc(hidden)]
pub mod generated_prelude {
    pub use crate::Theme;
    pub use crate::generated::tokens::*;
    pub use crate::types::*;
    pub use std::borrow::Cow;
    pub use std::time::Duration;
}

#[cfg(test)]
mod tests;
