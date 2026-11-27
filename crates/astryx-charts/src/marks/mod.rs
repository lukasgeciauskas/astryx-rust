//! The marks (upstream `marks/`): factories that return a series definition, which
//! resolves its points on the chart's scales and draws itself.

mod area;
mod band;
mod bar;
mod candlestick;
mod dot;
mod error_bar;
mod gl;
mod line;
mod paint;
mod reference_line;

pub use area::{AreaOptions, area};
pub use band::{BandOptions, band};
pub use bar::{BarOptions, bar};
pub use candlestick::{CandlestickOptions, candlestick};
pub use dot::{DotOptions, dot};
pub use error_bar::{ErrorBarOptions, error_bar};
pub use gl::*;
pub use line::{CurveType, LineOptions, line};
#[allow(unused_imports)]
pub(crate) use paint::{clamp_js, clamp_opacity, fill_rect, non_negative, stroke_segment};
pub use reference_line::{ReferenceLabelPosition, ReferenceLineOptions, reference_line};
