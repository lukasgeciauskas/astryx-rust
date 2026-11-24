//! Astryx components for GPUI. See `astryx docs porting` for how each one
//! is ported, and `astryx parity` for what is done.

pub mod button;
pub mod choice;
pub mod collapsible;
pub mod containers;
pub mod content;
pub mod doc;
pub mod feedback;
pub mod field;
mod focus;
pub mod icon;
pub mod indicator;
pub mod item;
pub mod layout;
pub mod link;
pub mod media;
mod motion;
pub mod people;
mod press;
pub mod segmented;
pub mod status;
pub mod tabs;
pub mod text;
pub mod time;
pub mod token;
pub mod toolbar;

pub use doc::{ComponentDoc, PropMapping, RustApi};
