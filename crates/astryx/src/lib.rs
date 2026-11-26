//! Astryx, Meta's design system, for GPUI.
//!
//! This facade re-exports every Astryx crate. Most apps depend on this crate
//! only:
//!
//! ```ignore
//! gpui_platform::application()
//!     .with_assets(astryx::Assets::new())
//!     .run(|cx| {
//!         astryx::init(cx);
//!         astryx::theme::set_theme(astryx::themes::neutral::theme(), cx);
//!         // open windows…
//!     });
//! ```

pub use astryx_components as components;
pub use astryx_effects as effects;
pub use astryx_i18n as i18n;
pub use astryx_icons as icons;
pub use astryx_markdown as markdown;
pub use astryx_primitives as primitives;
pub use astryx_syntax as syntax;
pub use astryx_theme as theme;
#[cfg(any(
    feature = "theme-butter",
    feature = "theme-chocolate",
    feature = "theme-gothic",
    feature = "theme-matcha",
    feature = "theme-neutral",
    feature = "theme-stone",
    feature = "theme-y2k"
))]
pub use astryx_themes as themes;

/// The asset source that serves Astryx's icons. Pass it to
/// `Application::with_assets`, or wrap the app's own source with
/// [`Assets::with_fallback`].
pub type Assets<A = ()> = astryx_icons::IconAssets<A>;

/// Sets up Astryx: core's default theme in system mode, Tab/Shift+Tab focus
/// traversal, and the editing keys of text fields. Call once, before opening
/// windows.
pub fn init(cx: &mut gpui::App) {
    astryx_theme::init(cx);
    astryx_primitives::focus::init(cx);
    astryx_primitives::editing::init(cx);
}

/// The types most apps use.
pub mod prelude {
    pub use astryx_i18n::{Direction, I18n, LogicalStyled};
    pub use astryx_primitives::{ElementSize, provide, resolve_size, use_context};
    pub use astryx_theme::{ThemeMode, ThemeStyled};
}
