//! Headless behavior primitives for Astryx on GPUI.
//!
//! These modules own behavior only: context propagation, focus, keyboard,
//! layers and editing. Visual decisions belong to `astryx-components` and
//! the theme.

pub mod announce;
pub mod context;
pub mod focus;
pub mod hotkeys;
pub mod layer;
pub mod link;
pub mod list_focus;
pub mod modality;
pub mod props;
pub mod size;
pub mod typeahead;

pub use announce::{Politeness, announce, announcer};
pub use context::{Provider, provide, use_context, with_context};
pub use focus::{FocusNext, FocusPrevious, RovingScope, control_focus};
pub use hotkeys::{Hotkey, hotkeys};
pub use layer::{EscapeBehavior, Layer, has_open_layer, layer};
pub use link::{LinkHandler, Navigation, link_provider, set_link_handler, use_link_handler};
pub use list_focus::{ListFocus, ListKeys, ListOrientation};
pub use modality::{
    InteractionModality, interaction_modality, is_focus_visible, is_focus_within_visible,
    prefers_reduced_motion, set_reduced_motion,
};
pub use size::{ElementSize, resolve_size};
pub use typeahead::Typeahead;

#[cfg(test)]
mod tests;
