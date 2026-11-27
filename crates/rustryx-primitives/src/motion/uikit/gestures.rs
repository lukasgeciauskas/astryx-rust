//! UIKit's gesture thresholds, upstream `motion/uikit/gestures.ts` and
//! `motion/uikit/longPress.ts`: one module per recognizer or domain. Units
//! are CSS px (one UIKit point) and milliseconds. Every value's source
//! (Apple's documentation, a measurement in the UIKit lab, or a guess with
//! the measurement that will replace it) is in upstream's table.

use super::scroll::DELAYS_CONTENT_TOUCHES_MS;
use super::timing::press::HIGHLIGHT_OUT_MS;

/// `LONG_PRESS_DURATION_MS`: how long a finger rests before a long press
/// begins (`UILongPressGestureRecognizer.minimumPressDuration`).
pub const LONG_PRESS_DURATION_MS: f64 = 500.;
/// `LONG_PRESS_ALLOWABLE_MOVEMENT_PX`: how far a pending long press's
/// finger may travel, as a distance.
pub const LONG_PRESS_ALLOWABLE_MOVEMENT_PX: f64 = 10.;

/// `UIKIT_TAP`.
pub mod tap {
    /// `allowableMovement`, px: each finger's travel before a tap fails.
    pub const ALLOWABLE_MOVEMENT: f64 = 45.;
    /// `maximumIntervalBetweenTaps`, ms.
    pub const MAXIMUM_INTERVAL_BETWEEN_TAPS: f64 = 350.;
    /// `allowableSeparation`, px: how far a later tap may land from the first.
    pub const ALLOWABLE_SEPARATION: f64 = 45.;
    /// `maximumTapDuration`, ms: the longest hold that still taps.
    pub const MAXIMUM_TAP_DURATION: f64 = 1500.;
}

/// `UIKIT_LONG_PRESS`.
pub mod long_press {
    /// `minimumPressDuration`, ms.
    pub const MINIMUM_PRESS_DURATION: f64 = super::LONG_PRESS_DURATION_MS;
    /// `allowableMovement`, px, a distance.
    pub const ALLOWABLE_MOVEMENT: f64 = super::LONG_PRESS_ALLOWABLE_MOVEMENT_PX;
}

/// `UIKIT_PAN`.
pub mod pan {
    /// `hysteresis`, px, per axis: the travel that begins a pan.
    pub const HYSTERESIS: f64 = 10.;
}

/// `UIKIT_SWIPE`.
pub mod swipe {
    /// `minimumDistance`, px along the direction.
    pub const MINIMUM_DISTANCE: f64 = 50.;
    /// `maximumDuration`, ms.
    pub const MAXIMUM_DURATION: f64 = 500.;
    /// `maximumSecondaryMovement`, px across the direction.
    pub const MAXIMUM_SECONDARY_MOVEMENT: f64 = 50.;
    /// `minimumMovementDecay`, a ratio (read, not modelled).
    pub const MINIMUM_MOVEMENT_DECAY: f64 = 0.06;
}

/// `UIKIT_PINCH`.
pub mod pinch {
    /// `hysteresis`, px: the span change that begins a pinch.
    pub const HYSTERESIS: f64 = 8.;
}

/// `UIKIT_ROTATION`.
pub mod rotation {
    /// `hysteresis`, radians (5 degrees; a guess, M-G10).
    pub const HYSTERESIS: f64 = 0.087_266_462_599_716_47;
}

/// `UIKIT_SCREEN_EDGE_PAN`.
pub mod screen_edge_pan {
    /// `edgeRegion`, px: the band at the screen edge a touch must land in.
    pub const EDGE_REGION: f64 = 40.2;
    /// `hysteresis`, px.
    pub const HYSTERESIS: f64 = 15.;
}

/// `UIKIT_CONTROL`.
pub mod control {
    /// `trackingExpansion`, px: how far past its bounds a press stays inside.
    pub const TRACKING_EXPANSION: f64 = 70.;
}

/// `UIKIT_PRESS`.
pub mod press {
    /// `contentTouchDelay`, ms (`UIScrollView.delaysContentTouches`).
    pub const CONTENT_TOUCH_DELAY: f64 = super::DELAYS_CONTENT_TOUCHES_MS;
    /// `quickTapFlash`, ms.
    pub const QUICK_TAP_FLASH: f64 = 100.;
    /// `slop`, px: the travel at which a press is a scroll.
    pub const SLOP: f64 = 10.;
    /// `scrollBrake`, ms: a touch this soon after a finger's scroll moved brakes it.
    pub const SCROLL_BRAKE: f64 = 150.;
    /// `momentumMemory`, ms (a guess, M-G19).
    pub const MOMENTUM_MEMORY: f64 = 3000.;
    /// The press release clock a painter uses (`timing.press.highlightOutMs`).
    pub const RELEASE_MS: f64 = super::HIGHLIGHT_OUT_MS;
}

/// `UIKIT_MENU_PRESS`.
pub mod menu_press {
    /// `holdOpenSlop`, px (a guess, M-G21).
    pub const HOLD_OPEN_SLOP: f64 = 13.;
    /// `handoff`, px.
    pub const HANDOFF: f64 = 10.;
}

/// `UIKIT_VELOCITY`.
pub mod velocity {
    /// `currentWeight`: a pan's newest interval in its velocity.
    pub const CURRENT_WEIGHT: f64 = 0.2;
    /// `scrollReleaseWeight`: a scrolling surface's newest pan value.
    pub const SCROLL_RELEASE_WEIGHT: f64 = 0.25;
    /// `stillness`, ms: a contact at rest this long releases at 0.
    pub const STILLNESS_MS: f64 = 40.;
}

/// `UIKIT_CLICK` (deprecated upstream for `CLICK_GUARD_WINDOW_MS`).
pub mod click {
    /// `strayClickWindow`, ms.
    pub const STRAY_CLICK_WINDOW: f64 = 400.;
}

/// UIKit's feedback kinds (`HAPTIC_FEEDBACK_KINDS`):
/// `UISelectionFeedbackGenerator`, `UIImpactFeedbackGenerator` and
/// `UINotificationFeedbackGenerator`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HapticFeedbackKind {
    Selection,
    ImpactLight,
    ImpactMedium,
    ImpactHeavy,
    ImpactSoft,
    ImpactRigid,
    NotificationSuccess,
    NotificationWarning,
    NotificationError,
}

impl HapticFeedbackKind {
    /// Every kind, in upstream's order.
    pub const ALL: [HapticFeedbackKind; 9] = [
        Self::Selection,
        Self::ImpactLight,
        Self::ImpactMedium,
        Self::ImpactHeavy,
        Self::ImpactSoft,
        Self::ImpactRigid,
        Self::NotificationSuccess,
        Self::NotificationWarning,
        Self::NotificationError,
    ];

    /// Upstream's name for the kind.
    pub fn name(self) -> &'static str {
        match self {
            Self::Selection => "selection",
            Self::ImpactLight => "impactLight",
            Self::ImpactMedium => "impactMedium",
            Self::ImpactHeavy => "impactHeavy",
            Self::ImpactSoft => "impactSoft",
            Self::ImpactRigid => "impactRigid",
            Self::NotificationSuccess => "notificationSuccess",
            Self::NotificationWarning => "notificationWarning",
            Self::NotificationError => "notificationError",
        }
    }

    /// `HAPTIC_VIBRATION_PATTERNS`: the `navigator.vibrate` pattern, ms (a
    /// guess, M-G18: the durations only keep the strengths in order).
    pub fn vibration_pattern(self) -> &'static [u32] {
        match self {
            Self::Selection => &[5],
            Self::ImpactLight => &[8],
            Self::ImpactMedium => &[12],
            Self::ImpactHeavy => &[18],
            Self::ImpactSoft => &[10],
            Self::ImpactRigid => &[6],
            Self::NotificationSuccess => &[8, 60, 12],
            Self::NotificationWarning => &[12, 80, 12],
            Self::NotificationError => &[12, 50, 12, 50, 18],
        }
    }
}
