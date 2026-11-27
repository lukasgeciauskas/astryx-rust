//! The parser, upstream `Markdown/parser.ts`: its canonical-AST path, split by the file's
//! own sections. Blocks are read line by line (`block`, `list`, with `lines` and
//! `link_definitions` shared), and each block's text inline (`inline`, `inline_index`,
//! `emphasis`, `autolink`).
//!
//! Plugins' extension syntax is tried where upstream tries it (`extension`, upstream
//! `matchExtensionSyntax`), and the source-lines option records block lines as upstream's
//! does. The incremental parser (`incremental`, upstream "Incremental parsing") parses a
//! streamed document's settled prefix once and its tail on every call, and `streaming`
//! holds the trimming that keeps partial syntax out of a streamed snapshot.

mod autolink;
mod block;
mod emphasis;
mod extension;
mod incremental;
mod inline;
mod inline_index;
mod lines;
mod link_definitions;
mod list;
mod streaming;

use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap};
use std::rc::Rc;

pub(crate) use block::parse_markdown_impl;
pub(crate) use incremental::parse_markdown_incremental_ast_blocks;
pub use incremental::{IncrementalState, IncrementalWork};
pub(crate) use inline::{InlineContext, parse_inline_entry};
pub(crate) use streaming::trim_streaming_artifacts;

use crate::plugins::PreparedMarkdownPlugins;
use crate::plugins::diagnostics::Diagnostics;

/// Link reference definitions, by normalized label: the destination each defines.
type LinkDefinitions = HashMap<String, String>;

/// What lazy-continuation probes learned: whether the blocks a nested parse of an input
/// at a depth produced end in a paragraph.
type NestedParses = HashMap<(usize, String), bool>;

/// The deepest lists and blockquotes may nest (upstream `MAX_BLOCK_NESTING`); content
/// nested deeper is one paragraph of text.
const MAX_BLOCK_NESTING: usize = 100;

/// The most characters, in UTF-16 units, a link label may hold (CommonMark 0.31 §4.7).
const MAX_LINK_LABEL_LENGTH: usize = 999;

/// Upstream `ResolvedOptions`: the caller's options and what the parser threads through
/// its recursion.
#[derive(Clone)]
pub(crate) struct ResolvedOptions<'a> {
    /// The prepared plugins, when any are installed.
    plugins: Option<&'a PreparedMarkdownPlugins>,
    /// Where plugin failures are reported.
    diagnostics: &'a Diagnostics,
    /// Whether the source is complete; while it is not, a plugin may defer.
    is_final: bool,
    /// Whether block extension syntax applies: only at the top level.
    allow_block_syntax: bool,
    /// Whether blocks record their source lines (the source-lines plugin).
    source_lines: bool,
    /// The 0-based document line of each input line of a nested parse (upstream
    /// `lineOrigin`); a nested parse without it records no lines.
    line_origin: Option<Rc<Vec<usize>>>,
    /// Citation source ids: `[id]` and `【id】` naming one become citations.
    source_ids: Option<&'a BTreeSet<String>>,
    /// GFM autolinks (upstream `autolink: 'gfm'`).
    autolink_gfm: bool,
    /// `$…$` and `$$…$$` math.
    math: bool,
    /// Whether this parse stamps `position` offsets on its blocks: only the top level
    /// does, since nested content is reassembled text.
    ast_positions: bool,
    /// Upstream `baseOffset`: where this parse's input starts in the document its
    /// positions address. The incremental parser parses slices, whose blocks' offsets
    /// must come out absolute.
    base_offset: usize,
    /// Upstream `baseLine`: the 0-based document line this parse's first input line is,
    /// for the slices the incremental parser parses while lines are recorded.
    base_line: usize,
    /// The link reference definitions in scope.
    link_defs: Option<Rc<LinkDefinitions>>,
    /// How many lists and blockquotes enclose this parse.
    block_depth: usize,
    /// Shared by one document's nested parses, so lazy-continuation probes at every
    /// nesting level cost no more than one parse.
    nested_parses: Rc<RefCell<NestedParses>>,
    /// Whether this parse runs inside a lazy-continuation probe, and so records what it
    /// produced in `nested_parses`.
    probing: bool,
    /// How many image descriptions enclose the one being read (upstream's module-level
    /// `imageAltDepth`, threaded through instead).
    image_alt_depth: usize,
}

impl<'a> ResolvedOptions<'a> {
    /// Upstream `resolveOptions`, with the plugins already prepared.
    pub(crate) fn new(
        options: &'a crate::ParseOptions,
        plugins: Option<&'a PreparedMarkdownPlugins>,
        diagnostics: &'a Diagnostics,
    ) -> Self {
        ResolvedOptions {
            plugins,
            diagnostics,
            is_final: options.is_final,
            allow_block_syntax: true,
            source_lines: plugins.is_some_and(|plugins| plugins.source_lines),
            line_origin: None,
            source_ids: options.source_ids.as_ref(),
            autolink_gfm: options.autolink_gfm,
            math: options.math,
            ast_positions: true,
            base_offset: 0,
            base_line: 0,
            link_defs: None,
            block_depth: 0,
            nested_parses: Rc::default(),
            probing: false,
            image_alt_depth: 0,
        }
    }

    /// Upstream `nested`: the same options for content parsed out of an enclosing list
    /// item or blockquote. Positions are a top-level contract: that content is
    /// reassembled with its markers stripped, so offsets into it would not address the
    /// document.
    fn nested(&self) -> Self {
        ResolvedOptions {
            allow_block_syntax: false,
            ast_positions: false,
            block_depth: self.block_depth + 1,
            base_line: 0,
            line_origin: None,
            ..self.clone()
        }
    }

    /// Upstream `nestedAt`: [`Self::nested`] for content whose input lines came from the
    /// document lines `origin` names.
    fn nested_at(&self, origin: Option<Vec<usize>>) -> Self {
        ResolvedOptions {
            line_origin: origin.map(Rc::new),
            ..self.nested()
        }
    }

    /// Upstream `protectedInlineOptions`: the same options without plugins, for link text
    /// and image descriptions, which extension syntax never reads.
    fn protected(&self) -> Self {
        ResolvedOptions {
            plugins: None,
            ..self.clone()
        }
    }

    /// The plugins whose inline syntax this parse tries, when any contribute some.
    fn inline_extension_starts(&self) -> Option<&'a std::collections::HashMap<char, Vec<usize>>> {
        self.plugins
            .map(|plugins| &plugins.inline_by_first_char)
            .filter(|starts| !starts.is_empty())
    }
}
