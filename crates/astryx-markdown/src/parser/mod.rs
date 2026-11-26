//! The parser, upstream `Markdown/parser.ts`: its canonical-AST path, split by the file's
//! own sections. Blocks are read line by line (`block`, `list`, with `lines` and
//! `link_definitions` shared), and each block's text inline (`inline`, `inline_index`,
//! `emphasis`, `autolink`).
//!
//! Not ported yet: plugins (extension syntax and transforms), the incremental and
//! streaming entry points, and the released `InlineNode`/`BlockNode` projection. The
//! places upstream consults plugins are marked where they fall.

mod autolink;
mod block;
mod emphasis;
mod inline;
mod inline_index;
mod lines;
mod link_definitions;
mod list;

use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap};
use std::rc::Rc;

pub(crate) use block::parse_markdown_impl;
pub(crate) use inline::{InlineContext, parse_inline_entry};

use crate::ParseOptions;

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
/// its recursion. Upstream's `isFinal`, `plugins`, `allowBlockSyntax`, `baseOffset` and
/// source-line fields serve plugins and incremental parsing, which are not ported yet.
#[derive(Clone)]
pub(crate) struct ResolvedOptions<'a> {
    /// Citation source ids: `[id]` and `【id】` naming one become citations.
    source_ids: Option<&'a BTreeSet<String>>,
    /// GFM autolinks (upstream `autolink: 'gfm'`).
    autolink_gfm: bool,
    /// `$…$` and `$$…$$` math.
    math: bool,
    /// Whether this parse stamps `position` offsets on its blocks: only the top level
    /// does, since nested content is reassembled text.
    ast_positions: bool,
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
    /// Upstream `resolveOptions` for a full, final parse.
    pub(crate) fn new(options: &'a ParseOptions) -> Self {
        ResolvedOptions {
            source_ids: options.source_ids.as_ref(),
            autolink_gfm: options.autolink_gfm,
            math: options.math,
            ast_positions: true,
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
            ast_positions: false,
            block_depth: self.block_depth + 1,
            ..self.clone()
        }
    }
}
