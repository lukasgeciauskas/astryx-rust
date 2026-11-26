//! The shared springs, upstream `motion/uikit/springs.ts`: every spring a
//! touch presentation plays, each in its source's vocabulary (each value's
//! source is in upstream's table). The rows spec:AST-094 FR4 names are
//! motion roles ([`role`]); the others are internal measured constants.

use crate::motion::SpringDescription;
use crate::motion::roles::TableValue;

const fn db(duration: f64, bounce: f64) -> SpringDescription {
    SpringDescription::DurationBounce { duration, bounce }
}

const fn rd(response: f64, damping_ratio: f64) -> SpringDescription {
    SpringDescription::ResponseDamping {
        response,
        damping_ratio,
    }
}

const fn physics(mass: f64, stiffness: f64, damping: f64) -> SpringDescription {
    SpringDescription::Physics {
        mass,
        stiffness,
        damping,
    }
}

/// `settle`: SwiftUI's `.smooth`.
pub const SETTLE: SpringDescription = db(0.5, 0.);
/// `quickChange`: SwiftUI's `.snappy`.
pub const QUICK_CHANGE: SpringDescription = db(0.5, 0.15);
/// `emphasis`: SwiftUI's `.bouncy`.
pub const EMPHASIS: SpringDescription = db(0.5, 0.3);
/// `fingerTracking`: SwiftUI's `interactiveSpring`.
pub const FINGER_TRACKING: SpringDescription = rd(0.15, 0.86);
/// `stateChange`: SwiftUI's `.spring` default.
pub const STATE_CHANGE: SpringDescription = rd(0.55, 1.);
/// `directChange`.
pub const DIRECT_CHANGE: SpringDescription = rd(0.5, 0.825);
/// `timedChange`.
pub const TIMED_CHANGE: SpringDescription = db(0.5, 0.);
/// `viewChange`: UIKit's default spring.
pub const VIEW_CHANGE: SpringDescription = physics(3., 1000., 500.);
/// `navigationPush`.
pub const NAVIGATION_PUSH: SpringDescription = rd(0.3, 1.);
/// `navigationPop`.
pub const NAVIGATION_POP: SpringDescription = rd(0.3, 1.);
/// `navigationInteractivePopFinish`.
pub const NAVIGATION_INTERACTIVE_POP_FINISH: SpringDescription = rd(0.3, 0.85);
/// `navigationBarElements`.
pub const NAVIGATION_BAR_ELEMENTS: SpringDescription = db(0.6, 0.);
/// `sheetPresent`.
pub const SHEET_PRESENT: SpringDescription = physics(3., 1000., 500.);
/// `sheetDismiss`.
pub const SHEET_DISMISS: SpringDescription = physics(3., 1000., 500.);
/// `sheetDetent`.
pub const SHEET_DETENT: SpringDescription = rd(0.3441442326, 1.);
/// `sheetReleaseFast`.
pub const SHEET_RELEASE_FAST: SpringDescription = rd(0.3441442326, 0.8);
/// `sheetDismissSwipe`.
pub const SHEET_DISMISS_SWIPE: SpringDescription = rd(0.3441442326, 0.8);
/// `alertPresent`.
pub const ALERT_PRESENT: SpringDescription = physics(1., 522.35, 45.71);
/// `alertDismiss`.
pub const ALERT_DISMISS: SpringDescription = physics(1., 522.35, 45.71);
/// `actionSheetPresent`.
pub const ACTION_SHEET_PRESENT: SpringDescription = physics(1., 522.35, 45.71);
/// `actionSheetDismiss`.
pub const ACTION_SHEET_DISMISS: SpringDescription = physics(1., 522.35, 45.71);
/// `menuOpen`.
pub const MENU_OPEN: SpringDescription = rd(0.32, 0.8);
/// `menuClose`.
pub const MENU_CLOSE: SpringDescription = rd(0.35, 0.85);
/// `subMenuOpen`.
pub const SUB_MENU_OPEN: SpringDescription = rd(0.4, 0.85);
/// `subMenuBack`.
pub const SUB_MENU_BACK: SpringDescription = rd(0.4, 1.);
/// `contextMenuLift`.
pub const CONTEXT_MENU_LIFT: SpringDescription = rd(0.32, 0.8);
/// `contextMenuDismiss`.
pub const CONTEXT_MENU_DISMISS: SpringDescription = rd(0.35, 0.85);
/// `contextMenuMorph`.
pub const CONTEXT_MENU_MORPH: SpringDescription = rd(0.3, 0.8);
/// `menuReducedMotion`: UIKit's menu spring under Reduce Motion.
pub const MENU_REDUCED_MOTION: SpringDescription = rd(0.4, 1.);
/// `switchThumbMove`.
pub const SWITCH_THUMB_MOVE: SpringDescription = rd(0.3, 1.);
/// `keyboard`: the keyboard's frame (curve 7).
pub const KEYBOARD: SpringDescription = physics(1., 555.0265, 47.118);
/// `banner`.
pub const BANNER: SpringDescription = physics(3., 1000., 500.);
/// `sliderRelease`.
pub const SLIDER_RELEASE: SpringDescription = rd(0.4, 1.);
/// `sliderLift`.
pub const SLIDER_LIFT: SpringDescription = rd(0.215, 0.6);
/// `sliderDrop`.
pub const SLIDER_DROP: SpringDescription = rd(0.5, 0.7);

/// The springs that are motion roles, each named `springs.<name>`
/// (upstream `SPRING_ROLES`): hand these to the animator so the theme's
/// tempo applies.
pub mod role {
    use crate::motion::MotionSpring;

    macro_rules! roles {
        ($($name:ident = $id:literal),* $(,)?) => {
            $(
                #[doc = concat!("The `", $id, "` role.")]
                pub const $name: MotionSpring = MotionSpring::role($id, super::$name);
            )*
        };
    }

    roles! {
        FINGER_TRACKING = "springs.fingerTracking",
        EMPHASIS = "springs.emphasis",
        SETTLE = "springs.settle",
        QUICK_CHANGE = "springs.quickChange",
        VIEW_CHANGE = "springs.viewChange",
        TIMED_CHANGE = "springs.timedChange",
        STATE_CHANGE = "springs.stateChange",
        DIRECT_CHANGE = "springs.directChange",
        MENU_OPEN = "springs.menuOpen",
        MENU_CLOSE = "springs.menuClose",
        SUB_MENU_OPEN = "springs.subMenuOpen",
        SUB_MENU_BACK = "springs.subMenuBack",
        SHEET_DISMISS_SWIPE = "springs.sheetDismissSwipe",
        CONTEXT_MENU_LIFT = "springs.contextMenuLift",
        CONTEXT_MENU_DISMISS = "springs.contextMenuDismiss",
        CONTEXT_MENU_MORPH = "springs.contextMenuMorph",
        SLIDER_RELEASE = "springs.sliderRelease",
        SLIDER_LIFT = "springs.sliderLift",
        SLIDER_DROP = "springs.sliderDrop",
    }
}

/// Every row of upstream's `springs` table, by key.
pub const TABLE: [(&str, SpringDescription); 35] = [
    ("settle", SETTLE),
    ("quickChange", QUICK_CHANGE),
    ("emphasis", EMPHASIS),
    ("fingerTracking", FINGER_TRACKING),
    ("stateChange", STATE_CHANGE),
    ("directChange", DIRECT_CHANGE),
    ("timedChange", TIMED_CHANGE),
    ("viewChange", VIEW_CHANGE),
    ("navigationPush", NAVIGATION_PUSH),
    ("navigationPop", NAVIGATION_POP),
    (
        "navigationInteractivePopFinish",
        NAVIGATION_INTERACTIVE_POP_FINISH,
    ),
    ("navigationBarElements", NAVIGATION_BAR_ELEMENTS),
    ("sheetPresent", SHEET_PRESENT),
    ("sheetDismiss", SHEET_DISMISS),
    ("sheetDetent", SHEET_DETENT),
    ("sheetReleaseFast", SHEET_RELEASE_FAST),
    ("sheetDismissSwipe", SHEET_DISMISS_SWIPE),
    ("alertPresent", ALERT_PRESENT),
    ("alertDismiss", ALERT_DISMISS),
    ("actionSheetPresent", ACTION_SHEET_PRESENT),
    ("actionSheetDismiss", ACTION_SHEET_DISMISS),
    ("menuOpen", MENU_OPEN),
    ("menuClose", MENU_CLOSE),
    ("subMenuOpen", SUB_MENU_OPEN),
    ("subMenuBack", SUB_MENU_BACK),
    ("contextMenuLift", CONTEXT_MENU_LIFT),
    ("contextMenuDismiss", CONTEXT_MENU_DISMISS),
    ("contextMenuMorph", CONTEXT_MENU_MORPH),
    ("menuReducedMotion", MENU_REDUCED_MOTION),
    ("switchThumbMove", SWITCH_THUMB_MOVE),
    ("keyboard", KEYBOARD),
    ("banner", BANNER),
    ("sliderRelease", SLIDER_RELEASE),
    ("sliderLift", SLIDER_LIFT),
    ("sliderDrop", SLIDER_DROP),
];

/// The keys of [`TABLE`] that are roles (upstream `SPRING_ROLES`).
pub const ROLES: [&str; 19] = [
    "fingerTracking",
    "emphasis",
    "settle",
    "quickChange",
    "viewChange",
    "timedChange",
    "stateChange",
    "directChange",
    "menuOpen",
    "menuClose",
    "subMenuOpen",
    "subMenuBack",
    "sheetDismissSwipe",
    "contextMenuLift",
    "contextMenuDismiss",
    "contextMenuMorph",
    "sliderRelease",
    "sliderLift",
    "sliderDrop",
];

/// The table as the role registry reads it: only the role rows, as
/// upstream registers `SPRING_ROLES`.
pub(crate) fn leaves() -> Vec<(String, TableValue)> {
    TABLE
        .iter()
        .filter(|(key, _)| ROLES.contains(key))
        .map(|(key, spring)| (key.to_string(), TableValue::Spring(*spring)))
        .collect()
}
