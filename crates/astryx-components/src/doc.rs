//! The Rust side of each component's documentation.
//!
//! Upstream documents every component in a `.doc.mjs` file (exported to
//! `spec/manifest.json`). Each ported component adds a [`ComponentDoc`] here
//! that maps every upstream prop onto the Rust API and points at the example
//! code the gallery renders, so `astryx component <Name>` can show an agent
//! both sides and `astryx parity` can tell what is left.

/// How one upstream prop appears in the Rust API.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RustApi {
    /// A builder method, such as `.variant(ButtonVariant::Primary)`.
    Method(&'static str),
    /// A constructor argument, such as `label` in `Button::new(id, label)`.
    Argument(&'static str),
    /// Covered by a trait the component implements: `Styled` stands in for
    /// `className`/`xstyle`/`style`, `ParentElement` for `children`.
    Trait(&'static str),
    /// Deliberately not ported, with the reason (a web-only prop, say).
    Waived(&'static str),
}

/// One upstream prop and its Rust counterpart.
#[derive(Clone, Copy, Debug)]
pub struct PropMapping {
    pub prop: &'static str,
    pub rust: RustApi,
}

/// A ported component's Rust documentation.
#[derive(Clone, Copy, Debug)]
pub struct ComponentDoc {
    /// The upstream component name, exactly as in its `.doc.mjs`.
    pub name: &'static str,
    /// The Rust type, such as `astryx::components::Button`.
    pub rust_type: &'static str,
    /// Every upstream prop, in upstream order.
    pub props: &'static [PropMapping],
    /// The source of the component's examples module, which the gallery
    /// renders, so the shown code always compiles.
    pub example: &'static str,
}

/// Every ported component. A component module adds its `DOC` here when it
/// lands; `astryx parity` and the CLI tests read this list.
pub const REGISTRY: &[ComponentDoc] = &[];

/// The registered doc for an upstream component name.
pub fn find(name: &str) -> Option<&'static ComponentDoc> {
    REGISTRY
        .iter()
        .find(|doc| doc.name.eq_ignore_ascii_case(name))
}
