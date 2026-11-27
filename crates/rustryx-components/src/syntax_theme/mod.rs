//! SyntaxTheme, `useSyntaxTheme` and the syntax engine's GPUI side. Upstream:
//! `theme/syntax/`. The engine itself is `rustryx_syntax`.

pub mod examples;
mod theme;

#[cfg(test)]
mod tests;

pub(crate) use theme::highlighted_line;
pub use theme::{SyntaxColors, UseSyntaxTheme, syntax_colors, syntax_theme, use_syntax_theme};

use crate::doc::{ComponentDoc, PropMapping, RustApi};

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

pub const SYNTAX_THEME_DOC: ComponentDoc = ComponentDoc {
    name: "SyntaxTheme",
    rust_type: "rustryx::components::syntax_theme::syntax_theme (presets: rustryx::syntax)",
    props: &[
        p("theme", RustApi::Argument("theme (SyntaxThemeDefinition)")),
        p("children", RustApi::Argument("child")),
    ],
    example: include_str!("examples.rs"),
};
