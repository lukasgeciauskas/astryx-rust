//! Astryx components for GPUI. See `astryx docs porting` for how each one
//! is ported, and `astryx parity` for what is done.

pub mod button;
pub mod containers;
pub mod content;
pub mod doc;
pub mod feedback;
mod focus;
pub mod icon;
pub mod layout;
pub mod link;
pub mod media;
mod motion;
pub mod people;
mod press;
pub mod status;
pub mod text;
pub mod time;

pub use doc::{ComponentDoc, PropMapping, RustApi};
