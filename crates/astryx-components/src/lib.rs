//! Astryx components for GPUI. See `astryx docs porting` for how each one
//! is ported, and `astryx parity` for what is done.

pub mod containers;
pub mod content;
pub mod doc;
pub mod feedback;
mod focus;
pub mod icon;
pub mod layout;
mod motion;
pub mod people;
pub mod status;
pub mod text;

pub use doc::{ComponentDoc, PropMapping, RustApi};
