//! Markdown's parser, upstream `Markdown/parser.ts`, `ast.ts`, `characterReferences.ts` and
//! `url.ts`: Astryx's own parser (not a CommonMark library), onto its canonical
//! mdast-aligned AST, with upstream's plugin system (`plugins/`), heading projection
//! (`headingProjection.ts`), Outline's `parseOutlineFromMarkdown` and the Remark adapter's
//! meaning (`remark.ts`). Offsets are UTF-8 bytes (upstream's are UTF-16 units).
//!
//! Besides the canonical tree, the parser keeps upstream's released projection
//! ([`parse_markdown`], [`parse_inline`], the [`legacy`] node shapes) and its streaming
//! entry points: an incremental parse that reuses a document's settled prefix
//! ([`parse_markdown_ast_incremental`], [`parse_markdown_incremental`]) and the trimming
//! that keeps partial syntax out of a snapshot ([`trim_streaming_artifacts`]):
//!
//! ```
//! use rustryx_markdown::{
//!     ParseOptions, create_incremental_state, parse_markdown_ast, parse_markdown_ast_incremental,
//!     trim_streaming_artifacts,
//! };
//!
//! let document = "# Title\n\nSome **bold** text.\n\n- one\n- two";
//! let mut state = create_incremental_state();
//! let streaming = ParseOptions { is_final: false, ..ParseOptions::default() };
//! for end in (1..document.len()).filter(|&end| document.is_char_boundary(end)) {
//!     let snapshot = trim_streaming_artifacts(&document[..end], streaming.math);
//!     let _tree = parse_markdown_ast_incremental(&snapshot, &mut state, &streaming);
//! }
//! let done = ParseOptions { is_final: true, ..ParseOptions::default() };
//! let tree = parse_markdown_ast_incremental(document, &mut state, &done);
//! assert_eq!(tree, parse_markdown_ast(document, &done));
//! assert_eq!(state.settled_text(), "# Title\n\nSome **bold** text.");
//! ```
//!
//! A plugin in Rust, an inline `@{name}` mention:
//!
//! ```
//! use rustryx_markdown::ast::{ExtensionDisplay, ExtensionNode};
//! use rustryx_markdown::plugins::{
//!     ExtensionRenderer, PluginDefinition, SyntaxCapability, SyntaxContribution,
//!     TokenizeResult, create_markdown_plugin,
//! };
//! use rustryx_markdown::{ParseOptions, parse_inline_ast};
//!
//! let mentions = create_markdown_plugin(PluginDefinition {
//!     parse_key: Some("v1".into()),
//!     syntax: Some(SyntaxCapability {
//!         inline: vec![SyntaxContribution::new(["@{"], 80, |input| {
//!             let Some(close) = input.source[input.offset + 2..].find('}') else {
//!                 return Ok(if input.is_final { TokenizeResult::NoMatch } else { TokenizeResult::Defer });
//!             };
//!             let close = input.offset + 2 + close;
//!             Ok(TokenizeResult::Match {
//!                 end: close + 1,
//!                 node: ExtensionNode {
//!                     plugin: "mentions".into(),
//!                     name: "mention".into(),
//!                     data: serde_json::json!({"label": &input.source[input.offset + 2..close]}),
//!                     display: ExtensionDisplay::Inline,
//!                     source: None,
//!                     position: None,
//!                 },
//!             })
//!         })],
//!         block: vec![],
//!     }),
//!     renderers: [(
//!         "mention".to_string(),
//!         ExtensionRenderer::new(|node| Ok(format!("@{}", node.data["label"].as_str().unwrap_or("")))),
//!     )]
//!     .into(),
//!     ..PluginDefinition::new("mentions")
//! })
//! .expect("a valid plugin");
//!
//! let nodes = parse_inline_ast("Hi @{Ada}", &ParseOptions {
//!     plugins: vec![mentions],
//!     ..ParseOptions::default()
//! });
//! assert_eq!(nodes.len(), 2);
//! ```

pub mod ast;
mod character_references;
mod heading_projection;
mod js;
pub mod legacy;
mod outline;
mod parser;
pub mod plugins;
pub mod remark;
mod slug;
mod url;

#[rustfmt::skip]
mod generated {
    pub mod character_references;
}

use std::collections::BTreeSet;
use std::fmt;

pub use character_references::{
    decode_literal_text, decode_markdown_character_references, is_ascii_punctuation,
    match_character_reference,
};
pub use heading_projection::{
    MarkdownHeadingProjection, OutlineItem, ProjectedHeading, project_markdown_headings,
};
pub use outline::{parse_outline_from_markdown, parse_outline_from_markdown_with};

/// The task-facing name of upstream's `OutlineItem`.
pub type OutlineEntry = OutlineItem;
pub use slug::{slugify, unique_slug};
pub use url::{
    is_safe_markdown_parser_url, normalized_markdown_destination, sanitize_markdown_link_url,
};

pub use legacy::{
    BlockNode, InlineNode, ListItemNode, SourceRange, TableCellNode, project_block_node,
    project_inline_node, project_inline_nodes, project_markdown_root,
};
pub use parser::{IncrementalState, IncrementalWork};

use ast::{Block, ExtensionDisplay, Paragraph, Phrasing, Root};
use parser::{InlineContext, ResolvedOptions};
use plugins::diagnostics::{
    CONFIGURATION_PLUGIN, DiagnosticCallback, DiagnosticCode, Diagnostics, PluginPhase,
};
use plugins::{MarkdownPlugin, PreparedMarkdownPlugins};

/// Upstream `ParseOptions` (with `MathParseOptions`'s `math` and the incremental options'
/// `isFinal`). Build it with `..ParseOptions::default()`.
///
/// It holds closures (plugins, the diagnostic callback), so it has no `PartialEq`; a
/// renderer caching parses keys them on [`ParseOptions::parse_identity`].
#[derive(Clone)]
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
    /// node projection ([`parse_markdown`], [`parse_markdown_incremental`]). The canonical
    /// tree always carries `position` offsets on its top-level blocks, so this changes
    /// nothing in it.
    pub source_ranges: bool,
    /// Upstream `plugins`: ordered opt-in extensions. Omitted (empty) is the released
    /// parser exactly (FR12). A list that fails preparation (two entries of one name) is
    /// reported as [`DiagnosticCode::DuplicateName`] and the parse runs without plugins:
    /// no entry point fails (FR19).
    pub plugins: Vec<MarkdownPlugin>,
    /// Upstream `onPluginDiagnostic` (FR19): one source-free event per plugin failure or
    /// advisory, at most [`plugins::diagnostics::MAX_DIAGNOSTICS_PER_PARSE`] distinct ones a
    /// parse.
    pub on_plugin_diagnostic: Option<DiagnosticCallback>,
    /// Upstream's final-input signal (`isFinal`): false while more source may stream in, so
    /// plugin syntax may defer and transforms can wait for complete input. Default true.
    /// Upstream's incremental entry points default it to false; here a streaming caller
    /// passes `is_final: false` for each chunk and `true` for the terminal snapshot.
    pub is_final: bool,
}

impl Default for ParseOptions {
    fn default() -> Self {
        ParseOptions {
            source_ids: None,
            autolink_gfm: false,
            math: false,
            source_ranges: false,
            plugins: Vec::new(),
            on_plugin_diagnostic: None,
            is_final: true,
        }
    }
}

impl fmt::Debug for ParseOptions {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ParseOptions")
            .field("source_ids", &self.source_ids)
            .field("autolink_gfm", &self.autolink_gfm)
            .field("math", &self.math)
            .field("source_ranges", &self.source_ranges)
            .field(
                "plugins",
                &self
                    .plugins
                    .iter()
                    .map(MarkdownPlugin::name)
                    .collect::<Vec<_>>(),
            )
            .field("on_plugin_diagnostic", &self.on_plugin_diagnostic.is_some())
            .field("is_final", &self.is_final)
            .finish()
    }
}

impl ParseOptions {
    /// FR15's parse identity: every parse-affecting option, then each syntax-bearing
    /// plugin's name, API version and parse key, in order (upstream `syntaxIdentity`; the
    /// source-lines plugin counts as one). Two option sets with one identity parse a
    /// source to the same tree before transforms run; transforms and renderers are not
    /// part of it, since they do not change what the parser reads, so a cache keyed on it
    /// still reruns transforms when they change.
    pub fn parse_identity(&self) -> String {
        let source_ids = self.source_ids.as_ref().map_or_else(
            || "-".to_string(),
            |ids| {
                ids.iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>()
                    .join("\u{0}")
            },
        );
        let syntax = plugins::prepare(&self.plugins)
            .map(|prepared| prepared.syntax_identity().to_string())
            .unwrap_or_default();
        format!(
            "sourceIds={source_ids}\u{1}autolink={}\u{1}math={}\u{1}sourceRanges={}\u{1}final={}\u{1}plugins={syntax}",
            self.autolink_gfm, self.math, self.source_ranges, self.is_final,
        )
    }
}

/// The options' plugins, prepared; a list that fails preparation is reported and dropped
/// (upstream Outline's rule, and FR19's for every entry point).
fn prepare_plugins(
    options: &ParseOptions,
    diagnostics: &Diagnostics,
) -> Option<PreparedMarkdownPlugins> {
    if options.plugins.is_empty() {
        return None;
    }
    match plugins::prepare(&options.plugins) {
        Ok(prepared) => Some(prepared),
        Err((_, plugin)) => {
            let plugin = if plugin.is_empty() {
                CONFIGURATION_PLUGIN
            } else {
                &plugin
            };
            diagnostics.report(
                plugin,
                PluginPhase::Configuration,
                DiagnosticCode::DuplicateName,
            );
            None
        }
    }
}

/// Upstream `parseMarkdownAst` (and `parseMarkdownAstInternal`, with `is_final`): block
/// Markdown as the canonical tree, after every plugin transform. Top-level blocks carry
/// `position` offsets into `input`; nested blocks do not (with the source-lines plugin,
/// every block carries its lines).
pub fn parse_markdown_ast(input: &str, options: &ParseOptions) -> Root {
    let diagnostics = Diagnostics::new(options.on_plugin_diagnostic.clone());
    let prepared = prepare_plugins(options, &diagnostics);
    parse_markdown_ast_prepared(input, options, prepared.as_ref(), &diagnostics)
}

pub(crate) fn parse_markdown_ast_prepared(
    input: &str,
    options: &ParseOptions,
    prepared: Option<&PreparedMarkdownPlugins>,
    diagnostics: &Diagnostics,
) -> Root {
    let opts = ResolvedOptions::new(options, prepared, diagnostics);
    let root = Root {
        children: parser::parse_markdown_impl(input, &opts),
        ..Root::default()
    };
    match prepared {
        Some(plugins) => plugins::apply_markdown_transforms(
            root,
            plugins,
            input,
            options.is_final,
            ExtensionDisplay::Block,
            diagnostics,
        ),
        None => root,
    }
}

/// Upstream `parseInlineAst`: inline Markdown as canonical phrasing nodes, after every
/// plugin transform (which sees them as a root holding one paragraph). Reference links do
/// not resolve, since no definitions are in scope.
pub fn parse_inline_ast(input: &str, options: &ParseOptions) -> Vec<Phrasing> {
    let diagnostics = Diagnostics::new(options.on_plugin_diagnostic.clone());
    let prepared = prepare_plugins(options, &diagnostics);
    let opts = ResolvedOptions::new(options, prepared.as_ref(), &diagnostics);
    let nodes = parser::parse_inline_entry(input, &opts, InlineContext::Default);
    let Some(plugins) = prepared.as_ref().filter(|plugins| plugins.has_transforms()) else {
        return nodes;
    };
    let root = Root {
        children: vec![Block::Paragraph(Paragraph {
            children: nodes.clone(),
            ..Paragraph::default()
        })],
        ..Root::default()
    };
    let transformed = plugins::apply_markdown_transforms(
        root,
        plugins,
        input,
        options.is_final,
        ExtensionDisplay::Inline,
        &diagnostics,
    );
    // A trusted helper may leave no paragraph (frontmatter while streaming): the parsed
    // nodes stand.
    match transformed.children.into_iter().next() {
        Some(Block::Paragraph(paragraph)) => paragraph.children,
        _ => nodes,
    }
}

/// Upstream `parseMarkdown`: block Markdown in the released `BlockNode` shape, projected
/// from [`parse_markdown_ast`]'s tree. Top-level blocks carry a `range` when
/// `source_ranges` is set.
pub fn parse_markdown(input: &str, options: &ParseOptions) -> Vec<BlockNode> {
    legacy::project_markdown_root(&parse_markdown_ast(input, options), options.source_ranges)
}

/// Upstream `parseInline`: inline Markdown in the released `InlineNode` shape, projected
/// from [`parse_inline_ast`]'s nodes.
pub fn parse_inline(input: &str, options: &ParseOptions) -> Vec<InlineNode> {
    legacy::project_inline_nodes(&parse_inline_ast(input, options), options.source_ranges)
}

/// Upstream `createIncrementalState`: an empty cache for one streamed document.
pub fn create_incremental_state() -> IncrementalState {
    IncrementalState::new()
}

/// Upstream `getIncrementalParseWork`: deterministic counters of what the state's most
/// recent incremental parse visited.
pub fn get_incremental_parse_work(state: &IncrementalState) -> IncrementalWork {
    state.work()
}

/// Upstream `trimStreamingArtifacts`: the text a streamed snapshot should parse as, with
/// trailing incomplete inline syntax removed (an unclosed code span, link, image or
/// strikethrough) and an emphasis run that already has content closed, so partial syntax
/// never flashes. With `math`, an incomplete display expression is held back as one unit,
/// and an unmatched inline `$` opener too. Only the last line changes otherwise.
pub fn trim_streaming_artifacts(input: &str, math: bool) -> String {
    parser::trim_streaming_artifacts(input, math)
}

/// Upstream `parseMarkdownAstIncremental`: one cumulative snapshot of a streamed document
/// as the canonical tree, after every plugin transform. Pass each snapshot (the whole text
/// so far) with `is_final: false`, then the complete text with `is_final: true`.
///
/// The settled prefix, before the last blank line no open fence or display expression
/// holds, is parsed once and reused; only the tail is parsed each call, with the lines
/// that would show partial structure (a bare list marker, a lone table header, an open
/// display expression) held back. Positions are absolute offsets into `input`. A snapshot
/// that does not extend the settled prefix, or a change of any parse-affecting option or
/// syntax plugin (FR15), parses everything again; an empty snapshot resets the state.
pub fn parse_markdown_ast_incremental(
    input: &str,
    state: &mut IncrementalState,
    options: &ParseOptions,
) -> Root {
    let diagnostics = Diagnostics::new(options.on_plugin_diagnostic.clone());
    let prepared = prepare_plugins(options, &diagnostics);
    let opts = ResolvedOptions::new(options, prepared.as_ref(), &diagnostics);
    let root = Root {
        children: parser::parse_markdown_incremental_ast_blocks(input, state, &opts, options),
        ..Root::default()
    };
    match prepared.as_ref() {
        Some(plugins) => plugins::apply_markdown_transforms(
            root,
            plugins,
            input,
            options.is_final,
            ExtensionDisplay::Block,
            &diagnostics,
        ),
        None => root,
    }
}

/// Upstream `parseMarkdownIncremental`: [`parse_markdown_ast_incremental`] in the released
/// `BlockNode` shape. It also refreshes the state's
/// [`settled_blocks`](IncrementalState::settled_blocks).
pub fn parse_markdown_incremental(
    input: &str,
    state: &mut IncrementalState,
    options: &ParseOptions,
) -> Vec<BlockNode> {
    let root = parse_markdown_ast_incremental(input, state, options);
    let projected = legacy::project_markdown_root(&root, options.source_ranges);
    state.project_settled_blocks(options.source_ranges);
    projected
}
