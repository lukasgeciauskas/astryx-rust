//! UIKit's measured motion constants, upstream `motion/uikit/`: one module
//! per domain table (each value's source is in upstream's table). The
//! motion roles among them register in [`crate::motion::roles`].
//!
//! Component tables that no motion role reads (alert, datePicker,
//! popover, reorder, slider) arrive with their components; the sheet's
//! release constants are in [`sheet`] and the gesture recognizers'
//! thresholds in [`gestures`].

pub mod gestures;
pub mod navigation;
pub mod paging;
pub mod picker;
pub mod progress_bar;
pub mod scroll;
pub mod segmented;
pub mod sheet;
pub mod springs;
pub mod switch_control;
pub mod timing;
pub mod zoom_media;

/// `UIKIT_VELOCITY` (`motion/uikit/gestures.ts`).
pub use gestures::velocity;

/// `UIKIT_PAN`'s hysteresis, px: the travel that begins a pan.
pub const PAN_HYSTERESIS: f64 = gestures::pan::HYSTERESIS;

/// Core Animation's `easeInEaseOut`.
pub const EASE_IN_EASE_OUT: (f32, f32, f32, f32) = (0.42, 0., 0.58, 1.);
