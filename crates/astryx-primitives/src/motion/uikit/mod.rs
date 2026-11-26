//! UIKit's measured motion constants, upstream `motion/uikit/`: one module
//! per domain table (each value's source is in upstream's table). The
//! motion roles among them register in [`crate::motion::roles`].
//!
//! Component tables that no motion role reads (alert, datePicker,
//! popover, reorder, sheet, slider) and the gesture recognizers' thresholds
//! arrive with their components and recognizers.

pub mod navigation;
pub mod paging;
pub mod picker;
pub mod progress_bar;
pub mod scroll;
pub mod segmented;
pub mod springs;
pub mod switch_control;
pub mod timing;
pub mod zoom_media;

/// `UIKIT_VELOCITY` (`motion/uikit/gestures.ts`).
pub mod velocity {
    /// `currentWeight`: a pan's newest interval in its velocity.
    pub const CURRENT_WEIGHT: f64 = 0.2;
    /// `scrollReleaseWeight`: a scrolling surface's newest pan value.
    pub const SCROLL_RELEASE_WEIGHT: f64 = 0.25;
    /// `stillness`, ms: a contact at rest this long releases at 0.
    pub const STILLNESS_MS: f64 = 40.;
}

/// `UIKIT_PAN`'s hysteresis, px: the travel that begins a pan.
pub const PAN_HYSTERESIS: f64 = 10.;

/// Core Animation's `easeInEaseOut`.
pub const EASE_IN_EASE_OUT: (f32, f32, f32, f32) = (0.42, 0., 0.58, 1.);
