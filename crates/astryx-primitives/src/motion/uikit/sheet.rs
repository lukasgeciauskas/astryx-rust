//! The sheet's release constants, upstream `motion/uikit/sheet.ts`: when
//! `UISheetPresentationController` counts a release as a flick, how near a
//! detent a flick counts as on it, how far ahead a release is projected, and
//! the speed below which it is not. Read from UIKit with a
//! `[.medium(), .large()]` sheet in an injected host app (iOS 26.2 simulator,
//! iPhone 17 Pro); speeds in pt/s (px/s). Its springs are
//! [`super::springs::SHEET_PRESENT`], `SHEET_DISMISS`, `SHEET_DETENT`,
//! `SHEET_RELEASE_FAST` and `SHEET_DISMISS_SWIPE`.

/// `sheet.release`.
pub mod release {
    /// Release speed (either direction) from which UIKit's sheet counts a
    /// release as a flick: it goes at least to the next position in its
    /// direction, and on past it when its projection does. The same speed
    /// switches the settle from `springs.sheetDetent` (critically damped) to
    /// `springs.sheetReleaseFast` (80% damping). Measured inside
    /// (996.7, 1028.3] over 134 drag releases, Oct 23 and 24 2026.
    pub const FAST_SPEED: f64 = 1008.;
    /// How far ahead a release is projected, as seconds of its velocity; the
    /// dismissed position counts as one of the positions. Every one of 164
    /// releases fits 0.092 to 0.104 s.
    pub const PROJECTION_SECONDS: f64 = 0.1;
    /// Release speed below which a release is not projected: it settles on
    /// the position nearest where it was let go. Measured inside (244, 250.6].
    pub const MIN_PROJECTION_SPEED: f64 = 250.;
    /// How close to a position (pt) a flick's offset counts as on it, so the
    /// next position beyond is the one after it (0 to 6.6 fits).
    pub const DETENT_TOLERANCE: f64 = 1.;
}
