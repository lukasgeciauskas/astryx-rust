//! Astryx's public gesture hooks (spec:AST-107): the values a product needs to follow a
//! drag, hold a press, run a menu's press model and paint a press under a finger, each a
//! thin hook over a factory, with the gesture machinery
//! (`astryx_primitives::gestures`) kept underneath.
//!
//! - [`pan`]: `create_pan_gesture` / `use_pan_gesture` (FR1, FR6).
//! - [`long_press`]: `use_long_press`, over its detector.
//! - [`menu_press`]: `use_menu_press`, over the pure `menu_press_step` machine.
//! - [`press_feedback`]: `use_press_feedback`, the touch press model's window controller
//!   over the pure [`press_gesture`] machine.

pub mod action_group;
mod clock;
pub mod examples;
pub mod long_press;
pub mod menu_press;
pub mod pan;
pub mod press_feedback;
pub mod press_gesture;
pub mod scroller;
#[doc(hidden)]
pub mod touch_hit_area;

/// Work a gesture listener hands the window and the app (a pan's `on_pan` runs inside
/// the input).
pub use astryx_primitives::gestures::host::after_gesture;
pub use long_press::{
    LongPressArea, LongPressHandlers, LongPressOptions, LongPressTouch, use_long_press,
};
pub use pan::{
    PanAxis, PanGesture, PanGestureEvent, PanGestureOptions, PanState, create_pan_gesture,
    use_pan_gesture,
};
pub use press_feedback::{PressFeedback, PressSurface, use_press_feedback};
pub use press_gesture::PressPaint;

#[cfg(test)]
mod tests;

// --- Docs ---------------------------------------------------------------------

use crate::doc::{ComponentDoc, PropMapping, RustApi};

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

pub const CREATE_PAN_GESTURE_DOC: ComponentDoc = ComponentDoc {
    name: "createPanGesture",
    rust_type: "astryx::components::gestures::create_pan_gesture -> PanGesture",
    props: &[
        p(
            "element",
            RustApi::Argument(
                "host (the window's gesture host); wrap the element in PanGesture::area",
            ),
        ),
        p("options", RustApi::Argument("options: PanGestureOptions")),
        p("options.axis", RustApi::Method("PanGestureOptions::axis")),
        p(
            "options.onPan",
            RustApi::Argument("PanGestureOptions::new(on_pan)"),
        ),
    ],
    example: include_str!("examples.rs"),
};

pub const USE_PAN_GESTURE_DOC: ComponentDoc = ComponentDoc {
    name: "usePanGesture",
    rust_type: "astryx::components::gestures::use_pan_gesture -> PanGesture",
    props: &[
        p(
            "ref",
            RustApi::Argument("id (the element's id; wrap it in PanGesture::area)"),
        ),
        p("options", RustApi::Argument("options: PanGestureOptions")),
        p("options.axis", RustApi::Method("PanGestureOptions::axis")),
        p(
            "options.onPan",
            RustApi::Argument("PanGestureOptions::new(on_pan)"),
        ),
    ],
    example: include_str!("examples.rs"),
};

pub const USE_LONG_PRESS_DOC: ComponentDoc = ComponentDoc {
    name: "useLongPress",
    rust_type: "astryx::components::gestures::use_long_press -> LongPressHandlers",
    props: &[
        p("options", RustApi::Argument("options: LongPressOptions")),
        p(
            "options.onLongPress",
            RustApi::Argument("LongPressOptions::new(on_long_press)"),
        ),
        p(
            "options.disabled",
            RustApi::Method("LongPressOptions::disabled"),
        ),
        p(
            "options.delayMs",
            RustApi::Method("LongPressOptions::delay_ms"),
        ),
        p(
            "options.moveCancelPx",
            RustApi::Method("LongPressOptions::move_cancel_px"),
        ),
    ],
    example: include_str!("examples.rs"),
};

pub const USE_MENU_PRESS_DOC: ComponentDoc = ComponentDoc {
    name: "useMenuPress",
    rust_type: "astryx::components::gestures::menu_press::use_menu_press -> MenuPress",
    props: &[
        p("options", RustApi::Argument("options: MenuPressOptions")),
        p(
            "options.menuRef",
            RustApi::Method("MenuPress::menu (wraps each level of the menu)"),
        ),
        p(
            "options.itemSelector",
            RustApi::Method("MenuPress::row (wraps each enabled row)"),
        ),
        p(
            "options.triggerRef",
            RustApi::Method("MenuPress::trigger (wraps the trigger)"),
        ),
        p(
            "options.onTriggerPress",
            RustApi::Method("MenuPressOptions::on_trigger_press"),
        ),
        p(
            "options.onHighlight",
            RustApi::Method("MenuPressOptions::on_highlight"),
        ),
        p(
            "options.onActivate",
            RustApi::Method("MenuPressOptions::on_activate (else MenuPressRow::on_activate)"),
        ),
        p(
            "options.onDismiss",
            RustApi::Method("MenuPressOptions::on_dismiss"),
        ),
        p(
            "options.getScroller",
            RustApi::Method("MenuPressOptions::scroller"),
        ),
        p(
            "options.longPressDelayMs",
            RustApi::Method("MenuPressOptions::long_press_delay_ms"),
        ),
        p(
            "options.isEnabled",
            RustApi::Method("MenuPressOptions::disabled"),
        ),
    ],
    example: include_str!("examples.rs"),
};

pub const USE_PRESS_FEEDBACK_DOC: ComponentDoc = ComponentDoc {
    name: "usePressFeedback",
    rust_type: "astryx::components::gestures::use_press_feedback -> PressFeedback",
    props: &[],
    example: include_str!("examples.rs"),
};
