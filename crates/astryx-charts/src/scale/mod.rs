//! The d3-scale subset the charts use: linear and band scales, with d3-array's tick
//! arithmetic and the d3-format number formats d3-scale's `tickFormat` picks.

mod band;
mod linear;
mod number_format;
mod ticks;

pub use band::BandScale;
pub use linear::LinearScale;
pub use number_format::{
    FormatSpecifier, InvalidFormat, NumberFormat, precision_fixed, precision_prefix,
    precision_round, tick_format,
};
pub use ticks::{nice, tick_increment, tick_step, ticks};

#[cfg(test)]
mod tests;
