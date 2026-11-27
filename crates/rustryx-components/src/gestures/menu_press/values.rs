//! Upstream `hooks/menuPressValues.ts` and `hooks/menuPressMarker.ts`: the UIKit value the menu
//! press model reads, one entry of the UIKit table (`motion::uikit::gestures::menu_press`).

use rustryx_primitives::motion::uikit::gestures::menu_press;

/// How far the opening finger travels after the menu opens before it picks, px.
pub const MENU_PRESS_HANDOFF_PX: f64 = menu_press::HANDOFF;

/// How far a finger held on a menu trigger may move before the open, px: the 13 pt a
/// trigger in a scroll view allows.
pub const MENU_PRESS_TRIGGER_DRIFT_PX: f64 = menu_press::HOLD_OPEN_SLOP;
