//! Outline from Markdown, upstream `Outline/parseOutlineFromMarkdown.ts`: a document's
//! heading outline through Markdown's own parser, transforms and heading projection, so
//! every Outline id matches the id Markdown renders.

use crate::heading_projection::{OutlineItem, project};
use crate::plugins::MarkdownPlugin;
use crate::plugins::diagnostics::Diagnostics;
use crate::{ParseOptions, parse_markdown_ast_prepared, prepare_plugins};

/// Upstream `parseOutlineFromMarkdown`: the outline of `markdown` under `plugins`, with
/// Markdown's transform finality (`is_final`, false while it streams). A plugin list that
/// fails preparation is dropped, as upstream drops it.
pub fn parse_outline_from_markdown(
    markdown: &str,
    plugins: &[MarkdownPlugin],
    is_final: bool,
) -> Vec<OutlineItem> {
    parse_outline_from_markdown_with(
        markdown,
        &ParseOptions {
            plugins: plugins.to_vec(),
            is_final,
            ..ParseOptions::default()
        },
    )
}

/// [`parse_outline_from_markdown`] with every parse option, the diagnostic callback
/// included.
pub fn parse_outline_from_markdown_with(
    markdown: &str,
    options: &ParseOptions,
) -> Vec<OutlineItem> {
    let diagnostics = Diagnostics::new(options.on_plugin_diagnostic.clone());
    let prepared = prepare_plugins(options, &diagnostics);
    let root = parse_markdown_ast_prepared(markdown, options, prepared.as_ref(), &diagnostics);
    project(&root, prepared.as_ref(), &diagnostics).outline
}
