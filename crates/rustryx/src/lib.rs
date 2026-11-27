//! Rustryx: Meta's Astryx design system for GPUI.
//!
//! This facade re-exports every Rustryx crate. Most apps depend on this crate
//! only:
//!
//! ```ignore
//! gpui_platform::application()
//!     .with_assets(rustryx::Assets::new())
//!     .run(|cx| {
//!         rustryx::init(cx);
//!         rustryx::theme::set_theme(rustryx::themes::neutral::theme(), cx);
//!         // open windows…
//!     });
//! ```

#[cfg(feature = "charts")]
pub use rustryx_charts as charts;
pub use rustryx_components as components;
pub use rustryx_effects as effects;
pub use rustryx_i18n as i18n;
pub use rustryx_icons as icons;
#[cfg(feature = "lab")]
pub use rustryx_lab as lab;
pub use rustryx_markdown as markdown;
pub use rustryx_primitives as primitives;
pub use rustryx_syntax as syntax;
pub use rustryx_theme as theme;
#[cfg(any(
    feature = "theme-butter",
    feature = "theme-chocolate",
    feature = "theme-gothic",
    feature = "theme-matcha",
    feature = "theme-neutral",
    feature = "theme-stone",
    feature = "theme-y2k"
))]
pub use rustryx_themes as themes;

/// The asset source that serves Astryx's icons. Pass it to
/// `Application::with_assets`, or wrap the app's own source with
/// [`Assets::with_fallback`].
pub type Assets<A = ()> = rustryx_icons::IconAssets<A>;

/// Sets up Rustryx: core's default theme in system mode, Tab/Shift+Tab focus
/// traversal, and the editing keys of text fields. Call once, before opening
/// windows.
pub fn init(cx: &mut gpui::App) {
    rustryx_theme::init(cx);
    rustryx_primitives::focus::init(cx);
    rustryx_primitives::editing::init(cx);
}

/// The types most apps use.
pub mod prelude {
    pub use rustryx_i18n::{Direction, I18n, LogicalStyled};
    pub use rustryx_primitives::{ElementSize, provide, resolve_size, use_context};
    pub use rustryx_theme::{ThemeMode, ThemeStyled};
}
