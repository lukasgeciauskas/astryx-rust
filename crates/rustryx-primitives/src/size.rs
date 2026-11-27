//! The standard element size cascade (`architecture:component-size-cascade`).
//!
//! A participating component resolves one effective size in this order:
//! its explicit prop, then the nearest [`ElementSize`] provider, then its own
//! documented default.

use gpui::App;

use crate::context::use_context;

/// The shared `sm | md | lg` element size axis.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ElementSize {
    Sm,
    #[default]
    Md,
    Lg,
}

impl ElementSize {
    /// The value Astryx uses in `data-size` and in theme override conditions.
    pub fn as_str(self) -> &'static str {
        match self {
            ElementSize::Sm => "sm",
            ElementSize::Md => "md",
            ElementSize::Lg => "lg",
        }
    }
}

/// Resolves a component's effective size: `explicit`, else the nearest
/// provider, else `default`.
pub fn resolve_size(explicit: Option<ElementSize>, default: ElementSize, cx: &App) -> ElementSize {
    explicit
        .or_else(|| use_context::<ElementSize>(cx))
        .unwrap_or(default)
}
