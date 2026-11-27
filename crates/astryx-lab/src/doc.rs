//! The Rust side of each lab component's documentation (see
//! `astryx_components::doc`): `astryx component <Name> --package lab` and
//! `astryx parity` read this list.

pub use astryx_components::doc::{ComponentDoc, PropMapping, RustApi};

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

const EXAMPLE: &str = include_str!("examples.rs");

/// Upstream lab `Stat`.
pub const STAT_DOC: ComponentDoc = ComponentDoc {
    name: "Stat",
    rust_type: "astryx::lab::Stat",
    props: &[
        p("label", RustApi::Argument("label")),
        p("value", RustApi::Argument("value")),
        p("delta", RustApi::Method("delta")),
        p("description", RustApi::Method("description")),
        p("media", RustApi::Method("media")),
        p("size", RustApi::Method("size")),
        p("xstyle", RustApi::Trait("Styled")),
    ],
    example: EXAMPLE,
};

/// Every ported lab component.
pub const REGISTRY: &[ComponentDoc] = &[STAT_DOC];
