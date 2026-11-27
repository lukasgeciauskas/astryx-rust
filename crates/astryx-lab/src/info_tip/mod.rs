//! Upstream lab `InfoTip`: the lab entry InfoTip kept for one release after it moved
//! to Core. Each export here is Core's, marked deprecated with upstream's reason.

pub mod examples;

#[cfg(test)]
mod tests;

use astryx_components::doc::{ComponentDoc, PropMapping, RustApi};

/// An info button that shows short help (upstream lab `InfoTip`, Core's component
/// re-exported).
#[deprecated(
    note = "InfoTip moved to Core: import it from `astryx_components::overlay`. This Lab export is removed in the next release."
)]
pub type InfoTip = astryx_components::overlay::InfoTip;

/// The info icon's size (upstream lab `InfoTipSize`, Core's `IconSize`).
#[deprecated(
    note = "Import `InfoTipSize` from Core (`astryx_components::icon::IconSize`). This Lab export is removed in the next release."
)]
pub type InfoTipSize = astryx_components::icon::IconSize;

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

/// Upstream lab `InfoTip`: Core's props, through the deprecated re-export.
pub const INFO_TIP_DOC: ComponentDoc = ComponentDoc {
    name: "InfoTip",
    rust_type: "astryx::lab::info_tip::InfoTip",
    props: &[
        p("content", RustApi::Argument("content")),
        p("label", RustApi::Method("label")),
        p("size", RustApi::Method("size")),
        p("xstyle", RustApi::Trait("Styled")),
    ],
    example: include_str!("examples.rs"),
};
