//! The published Astryx themes: butter, chocolate, gothic, matcha, neutral,
//! stone and y2k. Each is a cargo feature (all are on by default) and brings
//! its fonts. Generated from upstream by `cargo xtask codegen`.
//!
//! ```ignore
//! astryx_theme::set_theme(astryx_themes::neutral::theme(), cx);
//! ```

#[rustfmt::skip]
mod generated;

pub use generated::*;
