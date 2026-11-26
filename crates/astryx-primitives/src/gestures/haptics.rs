//! Haptics, upstream `gestures/haptics.ts`.
//!
//! The web has one haptics API, `navigator.vibrate` (Android browsers);
//! [`play_haptic`] uses it on wasm with each UIKit feedback kind's pattern
//! (`HAPTIC_VIBRATION_PATTERNS`). Natively GPUI has no haptics, so
//! [`play_haptic`] calls a player an app installs with
//! [`set_haptic_player`] (a Taptic Engine or a gamepad rumble), and plays
//! nothing without one. UIKit's haptics follow the system setting, not
//! Reduce Motion, so neither path reads it.

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

/// Whether anything can vibrate: an installed player, or the browser's
/// `navigator.vibrate`.
pub fn can_vibrate() -> bool {
    installed().is_some() || web::can_vibrate()
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
