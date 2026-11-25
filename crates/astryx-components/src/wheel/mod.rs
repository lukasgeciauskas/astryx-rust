//! The picker wheels of the date and time sheets, upstream `DateInput/Wheel.tsx`,
//! `PickerWheel.tsx`, `wheelOptions.ts`, `wheelPhysics.ts` and
//! `wheelStrip.ts`.

pub(crate) mod options;
pub(crate) mod physics;
#[allow(clippy::module_inception)]
pub(crate) mod wheel;

pub(crate) use options::WheelOption;
pub(crate) use wheel::{Wheel, WheelHandle};

#[cfg(test)]
mod pure_tests;
