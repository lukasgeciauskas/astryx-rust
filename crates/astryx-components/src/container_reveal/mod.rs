//! useContainerReveal, upstream `hooks/useContainerReveal.ts` and
//! `containerReveal.stylex.ts`: content inside a container that shows while the
//! pointer rests on the container or focus is inside it (always, on a coarse pointer),
//! fading over the fast duration.

pub mod examples;
mod reveal;

#[cfg(test)]
mod tests;

pub use reveal::{
    ContainerReveal, ContainerRevealOptions, ContentRevealOptions, RevealState, RevealVisibility,
    UseContainerRevealOptions, use_container_reveal,
};

use crate::doc::{ComponentDoc, PropMapping, RustApi};

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

pub const USE_CONTAINER_REVEAL_DOC: ComponentDoc = ComponentDoc {
    name: "useContainerReveal",
    rust_type: "astryx::components::container_reveal::use_container_reveal -> ContainerReveal",
    props: &[
        p("options", RustApi::Argument("options")),
        p(
            "options.isEnabled",
            RustApi::Method("UseContainerRevealOptions::enabled"),
        ),
    ],
    example: include_str!("examples.rs"),
};
