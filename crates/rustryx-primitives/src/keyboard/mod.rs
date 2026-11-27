//! The on-screen keyboard, upstream `keyboard/` (spec AST-099): Core's one source for
//! the software keyboard's height (FR1, FR2), the window's keyboard layout guide that times
//! motion with it, and the ride on the keyboard's own slide (FR8).
//!
//! - [`measure_keyboard_height`] and [`get_keyboard_height`]: the height, from GPUI's
//!   visual viewport and insets ([`height`]).
//! - [`subscribe_keyboard_height`]: one sampling per window, however many subscribe; its
//!   reports are the viewport's and, on request, focus moves.
//! - [`register_field`]: the fields drawn each frame, which the text editor registers,
//!   with their nearest scroller and the keyboard targets around them.
//! - [`KeyboardModel`], [`keyboard_layout_guide`]: the guide and its clock-free core.
//! - [`ride_keyboard`]: moves an element with the keyboard.
//! - [`distance_into_band`]: the reveal's arithmetic.
//! - [`PanWatch`]: the browser's own reveal pan, put back.
//!
//! Everything here is internal to Astryx in upstream; it is public in Rust so the
//! component crate's surfaces can read it, and no component exposes the height (FR3).

mod band;
mod guide;
mod height;
mod model;
mod pan;
mod ride;
mod target;

#[cfg(test)]
mod tests;

pub use band::distance_into_band;
pub use guide::{
    GuideListener, GuideSubscription, KeyboardLayoutGuide, keyboard_layout_guide,
    keyboard_profile_of,
};
pub use height::{
    FieldKind, FocusTarget, KEYBOARD_EPSILON, KeyboardField, KeyboardHeightEvent, KeyboardListener,
    KeyboardSubscription, KeyboardTargetId, KeyboardTargetScope, drawn_fields, focused_field,
    get_keyboard_height, has_virtual_keyboard, is_finger_on_screen, is_keyboard_measurable,
    is_text_entry_focused, keyboard_clock, keyboard_now, keyboard_probe, layout_viewport_height,
    listener_count, measure_keyboard_height, nearest_scroller, pop_scroller,
    provide_keyboard_target, push_scroller, register_field, set_virtual_keyboard_api,
    subscribe_keyboard_height, touch_starts,
};
pub use model::{
    HIDE_SLIDE, KeyboardCurve, KeyboardModel, KeyboardProfile, KeyboardTransition,
    KeyboardTransitionKind, SHOW_SLIDE,
};
pub use pan::{PanWatch, RevealPanWatch, pan_visual_viewport_to};
pub use ride::ride_keyboard;
pub(crate) use target::sync_keeper;
pub use target::{SMOOTH_SCROLL_MS, scroll_range_of, scroll_top_of, scroll_top_to, smooth_scroll};

/// Upstream `followsKeyboard` (`keyboardTarget.ts`): whether the window keeps fields
/// above a keyboard at all: one that measures the keyboard, under a coarse primary
/// pointer.
pub fn follows_keyboard(window: &mut gpui::Window, cx: &mut gpui::App) -> bool {
    is_keyboard_measurable(window) && crate::coarse_primary_pointer(window, cx)
}

/// Upstream `FIELD_OFFSET` (`keyboardTarget.ts`): the gap a field keeps above the
/// keyboard's top edge, px: the clearance a layer keeps above the keyboard.
pub const FIELD_OFFSET: f64 = 10.;

/// Upstream `FIELD_CLEARANCE_AT_REPORT`: Chrome on Android pans the page itself if the
/// keyboard still covers the focused field about 100 to 170 ms after the report, so a show
/// sets the field this far above the keyboard's top at once, px, and only the rest rides.
pub const FIELD_CLEARANCE_AT_REPORT: f64 = 4.;

/// Upstream `coveredBy`: how far a bottom edge at `bottom` sits under the keyboard's top,
/// plus `offset`, px; 0 when it clears it. An overlap under one px is rounding.
pub fn covered_by(bottom: f64, height: f64, offset: f64, layout_height: f64) -> f64 {
    let covered = bottom + offset - (layout_height - height);
    if height > 0. && covered > KEYBOARD_EPSILON {
        covered
    } else {
        0.
    }
}
