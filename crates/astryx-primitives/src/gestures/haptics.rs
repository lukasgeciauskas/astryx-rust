//! Haptics, upstream `gestures/haptics.ts`.
//!
//! The web has one haptics API, `navigator.vibrate` (Android browsers);
//! [`play_haptic`] uses it on wasm with each UIKit feedback kind's pattern
//! (`HAPTIC_VIBRATION_PATTERNS`). On macOS it plays the trackpad's feedback
//! (`NSHapticFeedbackManager`), which a Force Touch trackpad gives while a
//! finger is on it: a selection tick as a level change, an impact (a sheet
//! settling onto a detent) as an alignment, a notification as the generic
//! pattern. Elsewhere natively [`play_haptic`] calls a player an app installs
//! with [`set_haptic_player`] (a Taptic Engine or a gamepad rumble), and
//! plays nothing without one; an installed player replaces the platform's
//! everywhere. UIKit's haptics follow the system setting, not Reduce Motion,
//! so no path reads it.

use std::cell::RefCell;
use std::rc::Rc;

pub use crate::motion::uikit::gestures::HapticFeedbackKind;

/// A haptic player: plays a kind, returns whether anything played.
pub type HapticPlayer = Rc<dyn Fn(HapticFeedbackKind) -> bool>;

thread_local! {
    static PLAYER: RefCell<Option<HapticPlayer>> = const { RefCell::new(None) };
}

/// Installs (or, with `None`, removes) the player [`play_haptic`] calls.
/// On the web it replaces `navigator.vibrate` (tests stub it this way).
pub fn set_haptic_player(player: Option<HapticPlayer>) {
    PLAYER.with(|slot| *slot.borrow_mut() = player);
}

fn installed() -> Option<HapticPlayer> {
    PLAYER.with(|slot| slot.borrow().clone())
}

/// Whether anything can vibrate: an installed player, the browser's
/// `navigator.vibrate`, or macOS's trackpad feedback.
pub fn can_vibrate() -> bool {
    installed().is_some() || web::can_vibrate() || mac::can_vibrate()
}

/// Whether the platform knows iOS Safari's switch control, the one element
/// whose real tap plays a haptic there. GPUI draws its own controls, so no.
pub fn supports_switch_control() -> bool {
    false
}

/// Vibrates for a feedback kind; returns whether anything played.
pub fn play_haptic(kind: HapticFeedbackKind) -> bool {
    if let Some(player) = installed() {
        return player(kind);
    }
    if mac::can_vibrate() {
        return mac::play(kind);
    }
    web::vibrate(kind.vibration_pattern())
}

/// `attachTapHaptic`'s moment: the light impact a real tap on a control's
/// label plays in iOS Safari. GPUI has no label element to give a hidden
/// switch, so the tap plays `ImpactLight` through [`play_haptic`].
pub fn play_tap_haptic() -> bool {
    play_haptic(HapticFeedbackKind::ImpactLight)
}

#[cfg(target_family = "wasm")]
mod web {
    use wasm_bindgen::JsValue;

    fn navigator() -> Option<web_sys::Navigator> {
        web_sys::window().map(|window| window.navigator())
    }

    pub fn can_vibrate() -> bool {
        navigator().is_some_and(|navigator| {
            js_sys::Reflect::get(&navigator, &JsValue::from_str("vibrate"))
                .is_ok_and(|f| f.is_function())
        })
    }

    pub fn vibrate(pattern: &[u32]) -> bool {
        if !can_vibrate() {
            return false;
        }
        let Some(navigator) = navigator() else {
            return false;
        };
        let array = js_sys::Array::new();
        for ms in pattern {
            array.push(&JsValue::from(*ms));
        }
        navigator.vibrate_with_pattern(&array)
    }
}

#[cfg(not(target_family = "wasm"))]
mod web {
    pub fn can_vibrate() -> bool {
        false
    }

    pub fn vibrate(_pattern: &[u32]) -> bool {
        false
    }
}

#[cfg(target_os = "macos")]
mod mac {
    //! The trackpad's feedback, `NSHapticFeedbackManager`.

    use objc2::MainThreadMarker;
    use objc2_app_kit::{
        NSHapticFeedbackManager, NSHapticFeedbackPattern, NSHapticFeedbackPerformanceTime,
        NSHapticFeedbackPerformer,
    };

    use super::HapticFeedbackKind;

    /// AppKit's feedback runs on the main thread, where GPUI's app runs.
    pub fn can_vibrate() -> bool {
        MainThreadMarker::new().is_some()
    }

    /// The pattern a UIKit kind plays as.
    pub fn pattern(kind: HapticFeedbackKind) -> NSHapticFeedbackPattern {
        match kind {
            HapticFeedbackKind::Selection => NSHapticFeedbackPattern::LevelChange,
            HapticFeedbackKind::ImpactLight
            | HapticFeedbackKind::ImpactMedium
            | HapticFeedbackKind::ImpactHeavy
            | HapticFeedbackKind::ImpactSoft
            | HapticFeedbackKind::ImpactRigid => NSHapticFeedbackPattern::Alignment,
            HapticFeedbackKind::NotificationSuccess
            | HapticFeedbackKind::NotificationWarning
            | HapticFeedbackKind::NotificationError => NSHapticFeedbackPattern::Generic,
        }
    }

    /// Asks the trackpad to play `kind`. It plays only while a finger rests on a
    /// Force Touch trackpad, which AppKit does not report, so this says whether
    /// it was asked.
    pub fn play(kind: HapticFeedbackKind) -> bool {
        if !can_vibrate() {
            return false;
        }
        NSHapticFeedbackManager::defaultPerformer().performFeedbackPattern_performanceTime(
            pattern(kind),
            NSHapticFeedbackPerformanceTime::Now,
        );
        true
    }
}

#[cfg(not(target_os = "macos"))]
mod mac {
    use super::HapticFeedbackKind;

    pub fn can_vibrate() -> bool {
        false
    }

    pub fn play(_kind: HapticFeedbackKind) -> bool {
        false
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use objc2_app_kit::NSHapticFeedbackPattern;

    use super::*;

    #[test]
    fn uikit_kinds_map_to_trackpad_patterns() {
        assert_eq!(
            mac::pattern(HapticFeedbackKind::Selection),
            NSHapticFeedbackPattern::LevelChange
        );
        assert_eq!(
            mac::pattern(HapticFeedbackKind::ImpactLight),
            NSHapticFeedbackPattern::Alignment
        );
        assert_eq!(
            mac::pattern(HapticFeedbackKind::NotificationError),
            NSHapticFeedbackPattern::Generic
        );
    }
}
