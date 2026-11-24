//! Headless behavior primitives for Astryx on GPUI.
//!
//! These modules own behavior only: context propagation, focus, keyboard,
//! layers and editing. Visual decisions belong to `astryx-components` and
//! the theme.

pub mod context;
pub mod props;
pub mod size;

pub use context::{Provider, provide, use_context, with_context};
pub use size::{ElementSize, resolve_size};

#[cfg(test)]
mod tests;
