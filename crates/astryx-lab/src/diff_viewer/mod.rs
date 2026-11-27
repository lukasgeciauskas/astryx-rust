//! DiffViewer (upstream lab `DiffViewer/`): a read-only, syntax-highlighted diff of one
//! file for code review surfaces, with the patch parser and the two-text differ that
//! build its file model.

pub mod create;
pub mod examples;
pub mod inline;
pub mod parse;
pub mod types;
pub mod view;

#[cfg(test)]
mod tests;

use astryx_components::doc::{ComponentDoc, PropMapping, RustApi};

pub use create::{CreateDiffFileOptions, create_diff_file};
pub use parse::parse_unified_diff;
pub use types::*;
pub use view::{
    DiffViewer, DiffViewerChangeIndicator, DiffViewerInlineHighlight, DiffViewerViewMode,
    infer_language, line_ref,
};

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

/// Upstream lab `DiffViewer`'s Rust documentation.
pub const DIFF_VIEWER_DOC: ComponentDoc = ComponentDoc {
    name: "DiffViewer",
    rust_type: "astryx::lab::diff_viewer::DiffViewer",
    props: &[
        p("file", RustApi::Argument("file (DiffViewerFile)")),
        p("viewMode", RustApi::Method("view_mode")),
        p("language", RustApi::Method("language")),
        p("codeLanguages", RustApi::Method("code_languages")),
        p("hasLineNumbers", RustApi::Method("line_numbers")),
        p("isWrapped", RustApi::Method("wrapped")),
        p("changeIndicator", RustApi::Method("change_indicator")),
        p("inlineHighlight", RustApi::Method("inline_highlight")),
        p("hasChangeBackground", RustApi::Method("change_background")),
        p("isCollapsible", RustApi::Method("collapsible")),
        p("isCollapsed", RustApi::Method("collapsed")),
        p("defaultIsCollapsed", RustApi::Method("default_collapsed")),
        p("onCollapsedChange", RustApi::Method("on_collapsed_change")),
        p("headerEnd", RustApi::Method("header_end")),
        p(
            "renderLineAnnotation",
            RustApi::Method("render_line_annotation"),
        ),
        p("onLineNumberClick", RustApi::Method("on_line_number_click")),
        p("maxHeight", RustApi::Method("max_height")),
        p("syntaxTheme", RustApi::Method("syntax_theme")),
        p(
            "ref",
            RustApi::Waived("GPUI has no DOM refs; the element is the component"),
        ),
        p("xstyle", RustApi::Trait("Styled")),
    ],
    example: include_str!("examples.rs"),
};
