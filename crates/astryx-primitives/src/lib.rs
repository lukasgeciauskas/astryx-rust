//! Headless behavior primitives for Astryx on GPUI.
//!
//! These modules own behavior only: context propagation, focus, keyboard,
//! layers and editing. Visual decisions belong to `astryx-components` and
//! the theme.

pub mod anchor;
pub mod anchored;
pub mod announce;
pub mod context;
pub mod diagnostics;
pub mod editing;
pub mod focus;
pub mod gestures;
pub mod hotkeys;
pub mod keyboard;
pub mod layer;
pub mod link;
pub mod list_focus;
#[doc(hidden)]
pub mod list_window;
pub mod modality;
pub mod motion;
pub mod platform;
pub mod props;
pub mod scroll_area;
pub mod size;
pub mod spell;
pub mod storage;
pub mod tree_focus;
pub mod typeahead;

pub use anchored::{LayerAlign, LayerOptions, LayerPosition, Placement, position_layer};
pub use announce::{
    LiveContainer, Politeness, announce, announcer, in_live_container, provide_live_container,
};
pub use context::{
    LayerScoped, Provider, provide, provide_in_layer, use_context, use_layer_context, with_context,
};
pub use diagnostics::dev_warn_once;
pub use focus::{
    FocusNext, FocusPrevious, OutOfTabOrder, RovingScope, control_focus, request_focus,
    take_focus_request,
};
pub use hotkeys::{Hotkey, hotkeys};
pub use layer::{
    AnchoredLayer, EscapeBehavior, Layer, LayerDismissal, LayerDismissalHandle, PositionFn,
    anchored_layer, has_open_layer, layer, layer_dismissal,
};
pub use link::{LinkHandler, Navigation, link_provider, set_link_handler, use_link_handler};
pub use list_focus::{ListFocus, ListKeys, ListOrientation};
#[doc(hidden)]
pub use list_window::{
    ListAnchor, ListPitch, ListWindowKey, ScrollHost, WindowedList, WindowedListAxis,
};
pub use modality::{
    InteractionModality, coarse_primary_pointer, interaction_modality, is_focus_visible,
    is_focus_within_visible, prefers_reduced_motion, set_reduced_motion,
};
pub use platform::{is_apple_platform, is_context_click, is_primary_click};
pub use size::{ElementSize, resolve_size};
pub use typeahead::Typeahead;

#[cfg(test)]
mod tests;
