//! Markdown's parser, upstream `Markdown/parser.ts`, `ast.ts`, `characterReferences.ts` and
//! `url.ts`: Astryx's own parser (not a CommonMark library), onto its canonical
//! mdast-aligned AST. Offsets are UTF-8 bytes (upstream's are UTF-16 units).

pub mod ast;
mod character_references;
mod js;
mod parser;
mod slug;
mod url;

#[rustfmt::skip]
mod generated {
    pub mod character_references;
}

use std::collections::BTreeSet;

pub use character_references::{
    decode_literal_text, decode_markdown_character_references, is_ascii_punctuation,
    match_character_reference,
};
pub use slug::{slugify, unique_slug};
pub use url::{
    is_safe_markdown_parser_url, normalized_markdown_destination, sanitize_markdown_link_url,
};

use ast::{Phrasing, Root};
use parser::{InlineContext, ResolvedOptions};

/// Upstream `ParseOptions` (with `MathParseOptions`'s `math`). Plugins are not ported
/// yet.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ParseOptions {
    /// Citation source ids: `[id]` and `【id】` markers naming one become citation nodes
    /// instead of text or links.
    pub source_ids: Option<BTreeSet<String>>,
    /// Upstream `autolink: 'gfm'`: bare `https?://…` and `www.…` URLs, `<URL>` and
    /// `<email>`, and `user@host` emails become links (the GFM autolink-literal extension
    /// and CommonMark §6.5). Unlike strict GFM, a trailing `&entity;` stays in the URL and
    /// any plausible TLD is accepted.
    pub autolink_gfm: bool,
    /// Parse `$…$` and `$$…$$` into `inlineMath` and `math` nodes.
    pub math: bool,
    /// Upstream `sourceRanges`: asks for each top-level block's `range` in the released
    /// node projection, and positions on plugins' block nodes, neither of which is ported
    /// yet. The canonical tree always carries `position` offsets on its top-level blocks,
    /// so this changes nothing in it.
    pub source_ranges: bool,
}

/// Upstream `parseMarkdownAst`: block Markdown as the canonical tree. Top-level blocks
/// carry `position` offsets into `input`; nested blocks do not.
pub fn parse_markdown_ast(input: &str, options: &ParseOptions) -> Root {
    let opts = ResolvedOptions::new(options);
    // Upstream runs plugins' transforms over the root here.
    Root {
        children: parser::parse_markdown_impl(input, &opts),
        position: None,
    }
}

/// Upstream `parseInlineAst`: inline Markdown as canonical phrasing nodes. Reference links
/// do not resolve, since no definitions are in scope.
pub fn parse_inline_ast(input: &str, options: &ParseOptions) -> Vec<Phrasing> {
    let opts = ResolvedOptions::new(options);
    // Upstream runs plugins' transforms over the nodes here.
    parser::parse_inline_entry(input, &opts, InlineContext::Default)
}
