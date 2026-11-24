//! Astryx components for GPUI. See `astryx docs porting` for how each one
//! is ported, and `astryx parity` for what is done.

pub mod breadcrumbs;
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
pub mod input;
pub mod item;
pub mod layout;
pub mod link;
pub mod media;
mod motion;
pub mod pagination;
pub mod people;
mod press;
pub mod segmented;
pub mod slider;
pub mod status;
pub mod stepper;
pub mod tabs;
pub mod text;
pub mod time;
pub mod token;
pub mod toolbar;

pub use doc::{ComponentDoc, PropMapping, RustApi};
