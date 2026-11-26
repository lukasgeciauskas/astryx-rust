//! AppShell and its mobile context. Upstream: `AppShell/` (component spec `AppShell`).

#[allow(clippy::module_inception)]
mod app_shell;
pub mod examples;
mod mobile;

#[cfg(test)]
mod tests;

pub use app_shell::{
    AppShell, AppShellBreakpoint, AppShellMobileNav, AppShellVariant, MobileNavConfig, SlotFn,
};
pub use mobile::{AppShellMobile, OpenFn, use_app_shell_mobile};

use crate::doc::{ComponentDoc, PropMapping, RustApi};

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

pub const APP_SHELL_DOC: ComponentDoc = ComponentDoc {
    name: "AppShell",
    rust_type: "astryx::components::app_shell::AppShell",
    props: &[
        p("children", RustApi::Trait("ParentElement")),
        p("contentPadding", RustApi::Method("content_padding")),
        p("topNav", RustApi::Method("top_nav")),
        p("sideNav", RustApi::Method("side_nav")),
        p("mobileNav", RustApi::Method("mobile_nav")),
        p("banner", RustApi::Method("banner")),
        p("height", RustApi::Method("height")),
        p("variant", RustApi::Method("variant")),
        p("xstyle", RustApi::Trait("Styled")),
    ],
    example: include_str!("examples.rs"),
};

pub const USE_APP_SHELL_MOBILE_DOC: ComponentDoc = ComponentDoc {
    name: "useAppShellMobile",
    rust_type: "astryx::components::app_shell::use_app_shell_mobile -> AppShellMobile",
    props: &[],
    example: include_str!("examples.rs"),
};
