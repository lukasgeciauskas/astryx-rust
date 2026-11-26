//! MobileNav and MobileNavToggle. Upstream: `MobileNav/` (component spec `MobileNav`).

pub mod examples;
#[allow(clippy::module_inception)]
mod mobile_nav;
mod toggle;

#[cfg(test)]
mod tests;

pub use mobile_nav::{MobileNav, MobileNavSide};
pub use toggle::MobileNavToggle;

use crate::doc::{ComponentDoc, PropMapping, RustApi};

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

pub const MOBILE_NAV_DOC: ComponentDoc = ComponentDoc {
    name: "MobileNav",
    rust_type: "astryx::components::mobile_nav::MobileNav (toggle: MobileNavToggle)",
    props: &[
        p("isOpen", RustApi::Method("open")),
        p("onOpenChange", RustApi::Method("on_open_change")),
        p("children", RustApi::Trait("ParentElement")),
        p("header", RustApi::Method("header / header_text")),
        p("width", RustApi::Method("width")),
        p("side", RustApi::Method("side")),
        p("label", RustApi::Method("label")),
        p(
            "data-testid",
            RustApi::Waived("GPUI has no DOM test ids; tests use debug selectors"),
        ),
        p("xstyle", RustApi::Trait("Styled")),
    ],
    example: include_str!("examples.rs"),
};
