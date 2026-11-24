//! Astryx components for GPUI. See `astryx docs porting` for how each one
//! is ported, and `astryx parity` for what is done.

pub mod content;
pub mod doc;
pub mod icon;
pub mod layout;
mod motion;
pub mod status;
pub mod text;

pub use doc::{ComponentDoc, PropMapping, RustApi};
