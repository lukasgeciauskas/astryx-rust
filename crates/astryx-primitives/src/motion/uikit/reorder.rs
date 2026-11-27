//! The reorder control's claim, upstream `motion/uikit/reorder.ts`'s `claim`: a still
//! finger on a reorder control claims the drag after the scroll view's content-touch
//! delay, and travel before then hands the touch to the scroll view. Measured on a
//! `UITableView` in editing mode (iOS 26.2 simulator, iPhone 17 Pro). The edge
//! autoscroll's table (`reorder.autoscroll`) is the reorder model's, in
//! `astryx_components`.

/// `claim.delayMs`: the scroll view's content-touch delay, ms.
pub const CLAIM_DELAY_MS: f64 = super::scroll::DELAYS_CONTENT_TOUCHES_MS;

/// `claim.allowableMovement`: travel before the claim that hands the touch to the scroll
/// view (the pan's hysteresis), px.
pub const CLAIM_ALLOWABLE_MOVEMENT: f64 = super::gestures::pan::HYSTERESIS;
